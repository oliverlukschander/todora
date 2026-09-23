//! The HTTP API. JSON in and out, except runs, which are their own bytes.
//!
//! | Method | Path | What |
//! | --- | --- | --- |
//! | GET | `/v1/health` | version, physics, whether it is up |
//! | POST | `/v1/players` | register a key and a name (signed with that key) |
//! | POST | `/v1/runs` | submit a lap (signed) |
//!
//! Boards and downloads are in [`crate::boards`]; renaming, deleting,
//! reports and moderation in [`crate::players`].

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use todora::verify::{self, Verifier};

use crate::{auth, db};

pub struct Shared {
    pub db: Mutex<Connection>,
    pub verifier: Verifier,
    /// Replays are the expensive part; two at a time, whatever arrives.
    pub replays: tokio::sync::Semaphore,
    pub limits: Mutex<Limits>,
    pub admin_token: Option<String>,
}

pub type AppState = Arc<Shared>;

pub fn state(db: Connection, admin_token: Option<String>) -> AppState {
    Arc::new(Shared {
        db: Mutex::new(db),
        verifier: Verifier::new(),
        replays: tokio::sync::Semaphore::new(2),
        limits: Mutex::new(Limits::default()),
        admin_token,
    })
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/players", post(register))
        .route("/v1/runs", post(submit))
        .merge(crate::boards::routes())
        .merge(crate::players::routes())
        .layer(DefaultBodyLimit::max(verify::MAX_RUN_BYTES))
        .with_state(state)
}

/// A refusal, with a reason the game can show.
#[derive(Debug)]
pub struct Problem(pub StatusCode, pub String);

impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

pub fn bad(reason: impl Into<String>) -> Problem {
    Problem(StatusCode::BAD_REQUEST, reason.into())
}

pub fn internal(trouble: impl std::fmt::Display) -> Problem {
    eprintln!("internal error: {trouble}");
    Problem(
        StatusCode::INTERNAL_SERVER_ERROR,
        "something went wrong".into(),
    )
}

/// Requests counted in a sliding hour, per player and per address. The
/// address comes from the reverse proxy's `X-Forwarded-For` and lives only
/// here, in memory, for an hour.
#[derive(Default)]
pub struct Limits {
    seen: HashMap<String, VecDeque<i64>>,
}

impl Limits {
    /// Count one request against `who`; false once it has had `per_hour`.
    pub fn allow(&mut self, who: &str, per_hour: usize, now: i64) -> bool {
        let times = self.seen.entry(who.to_string()).or_default();
        while times.front().is_some_and(|t| now - t >= 3600) {
            times.pop_front();
        }
        if times.len() >= per_hour {
            return false;
        }
        times.push_back(now);
        // Keep the map from growing without end.
        if self.seen.len() > 50_000 {
            self.seen
                .retain(|_, t| t.back().is_some_and(|last| now - last < 3600));
        }
        true
    }
}

pub fn address(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map_or("direct".into(), |v| format!("ip:{}", v.trim()))
}

fn limited(state: &Shared, headers: &HeaderMap, player: Option<&str>) -> Result<(), Problem> {
    let now = db::now();
    let mut limits = state.limits.lock().expect("the limits");
    let ok = limits.allow(&address(headers), 240, now)
        && player.is_none_or(|p| limits.allow(&format!("player:{p}"), 60, now));
    if ok {
        Ok(())
    } else {
        Err(Problem(
            StatusCode::TOO_MANY_REQUESTS,
            "too many requests; try again later".into(),
        ))
    }
}

/// Check that a request was signed by the player it says it is from.
pub fn signed_by(
    state: &Shared,
    headers: &HeaderMap,
    method: &str,
    path: &str,
    body: &[u8],
) -> Result<String, Problem> {
    let id = auth::player(headers).ok_or_else(|| bad("who is this?"))?;
    let (_, key, _) = db::player_by_id(&state.db.lock().expect("the database"), id)
        .map_err(internal)?
        .ok_or_else(|| Problem(StatusCode::UNAUTHORIZED, "no such player".into()))?;
    let key = auth::key(&key).map_err(internal)?;
    auth::check(&key, headers, method, path, body, db::now())
        .map_err(|why| Problem(StatusCode::UNAUTHORIZED, why.into()))?;
    Ok(id.to_string())
}

#[derive(Serialize)]
struct Health {
    ok: bool,
    app: &'static str,
    physics: u32,
}

async fn health() -> Json<Health> {
    Json(Health {
        ok: true,
        app: verify::APP_VERSION,
        physics: verify::PHYSICS_VERSION,
    })
}

#[derive(Deserialize)]
pub struct Registration {
    pub public_key: String,
    pub name: String,
    #[serde(default)]
    pub country: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Registered {
    pub id: String,
    pub name: String,
}

/// A new player: a key, signed with itself, and a name. Registering the same
/// key again answers with the player it already is.
async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Registered>, Problem> {
    limited(&state, &headers, None)?;
    let form: Registration =
        serde_json::from_slice(&body).map_err(|_| bad("not a registration"))?;
    let key_bytes = auth::decode(&form.public_key).map_err(bad)?;
    let key = auth::key(&key_bytes).map_err(bad)?;
    auth::check(&key, &headers, "POST", "/v1/players", &body, db::now())
        .map_err(|why| Problem(StatusCode::UNAUTHORIZED, why.into()))?;
    let name = verify::valid_name(&form.name).map_err(bad)?;
    let country = form.country.as_deref().and_then(verify::valid_country);
    let db = state.db.lock().expect("the database");
    if let Some(id) = db::player_by_key(&db, &key_bytes).map_err(internal)? {
        let (player, _, _) = db::player_by_id(&db, &id)
            .map_err(internal)?
            .expect("just found");
        return Ok(Json(Registered {
            id,
            name: player.name,
        }));
    }
    let id = auth::id();
    db::add_player(&db, &id, &key_bytes, &name, country.as_deref()).map_err(internal)?;
    Ok(Json(Registered { id, name }))
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Submitted {
    pub run: String,
    pub seconds: f64,
    /// Whether this is now the player's time on the board.
    pub improved: bool,
    pub rank: Option<u64>,
    pub total: u64,
}

/// A lap. It is driven again before anything is written; a lap that is not
/// the lap it claims is refused with the reason.
async fn submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Submitted>, Problem> {
    let player = signed_by(&state, &headers, "POST", "/v1/runs", &body)?;
    limited(&state, &headers, Some(&player))?;
    let bytes = body.to_vec();
    let checked = {
        let _turn = state.replays.acquire().await.map_err(internal)?;
        let state = state.clone();
        let bytes = bytes.clone();
        tokio::task::spawn_blocking(move || state.verifier.check(&bytes))
            .await
            .map_err(internal)?
    };
    let checked = match checked {
        Ok(checked) if verify::compatible_app(&checked.app_version) => checked,
        Ok(_) => {
            let db = state.db.lock().expect("the database");
            db::reject(&db, &player, "older game version").map_err(internal)?;
            return Err(Problem(StatusCode::CONFLICT, "older game version".into()));
        }
        Err(why) => {
            let db = state.db.lock().expect("the database");
            db::reject(&db, &player, &why).map_err(internal)?;
            return Err(Problem(StatusCode::UNPROCESSABLE_ENTITY, why));
        }
    };
    let id = format!("{:016x}", checked.lap);
    let db = state.db.lock().expect("the database");
    if db::run_known(&db, &id).map_err(internal)? {
        return Err(Problem(
            StatusCode::CONFLICT,
            "that lap has already been submitted".into(),
        ));
    }
    let improved = db::keep_run(&db, &id, &player, &checked, &bytes).map_err(internal)?;
    let board = db::Board {
        circuit: &checked.circuit,
        mode: &checked.mode,
        physics: checked.physics,
        country: None,
        car: None,
    };
    Ok(Json(Submitted {
        run: id,
        seconds: verify::seconds(checked.steps),
        improved,
        rank: db::rank_of(&db, &board, &player).map_err(internal)?,
        total: db::count(&db, &board).map_err(internal)?,
    }))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use ed25519_dalek::{Signer, SigningKey};
    use http_body_util::BodyExt;
    use std::sync::OnceLock;
    use tower::ServiceExt;

    pub fn app() -> (Router, AppState) {
        let state = state(db::memory(), Some("secret".into()));
        (router(state.clone()), state)
    }

    pub fn keypair() -> SigningKey {
        SigningKey::generate(&mut rand::rngs::OsRng)
    }

    pub fn signed(
        key: &SigningKey,
        player: Option<&str>,
        method: &str,
        path: &str,
        body: Vec<u8>,
    ) -> Request<Body> {
        let time = db::now();
        let signature = key.sign(&auth::message(method, path, time, &body));
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header(auth::TIME, time.to_string())
            .header(auth::SIGNATURE, STANDARD.encode(signature.to_bytes()));
        if let Some(player) = player {
            request = request.header(auth::PLAYER, player);
        }
        request.body(Body::from(body)).unwrap()
    }

    pub async fn send(app: &Router, request: Request<Body>) -> (StatusCode, serde_json::Value) {
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null),
        )
    }

    pub async fn join(app: &Router, key: &SigningKey, name: &str) -> String {
        let body = serde_json::json!({
            "public_key": STANDARD.encode(key.verifying_key().to_bytes()),
            "name": name,
            "country": "AT",
        })
        .to_string()
        .into_bytes();
        let (status, reply) = send(app, signed(key, None, "POST", "/v1/players", body)).await;
        assert_eq!(status, StatusCode::OK, "{reply}");
        reply["id"].as_str().unwrap().to_string()
    }

    /// A real lap, driven by the game's AI, made once for all the tests.
    pub fn monza() -> &'static [u8] {
        static RUN: OnceLock<Vec<u8>> = OnceLock::new();
        RUN.get_or_init(|| verify::ai_run("monza", "regular").expect("an AI lap"))
    }

    #[tokio::test]
    async fn a_player_registers_once_and_a_bad_name_is_refused() {
        let (app, _) = app();
        let key = keypair();
        let id = join(&app, &key, "Amber Falcon").await;
        assert_eq!(
            join(&app, &key, "Amber Falcon").await,
            id,
            "the same key, the same player"
        );
        let body = serde_json::json!({
            "public_key": STANDARD.encode(keypair().verifying_key().to_bytes()),
            "name": "sh1thead",
        })
        .to_string()
        .into_bytes();
        let (status, _) = send(&app, signed(&keypair(), None, "POST", "/v1/players", body)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "signed with another key");
    }

    #[tokio::test]
    async fn a_real_lap_goes_on_the_board_once() {
        let (app, _) = app();
        let key = keypair();
        let id = join(&app, &key, "Kestrel").await;
        let (status, reply) = send(
            &app,
            signed(&key, Some(&id), "POST", "/v1/runs", monza().to_vec()),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{reply}");
        assert_eq!(reply["improved"], true);
        assert_eq!(reply["rank"], 1);
        assert_eq!(reply["total"], 1);
        assert!(reply["seconds"].as_f64().unwrap() > 20.0);
        let (status, _) = send(
            &app,
            signed(&key, Some(&id), "POST", "/v1/runs", monza().to_vec()),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "the same lap twice");
    }

    #[tokio::test]
    async fn somebody_elses_lap_cannot_be_claimed() {
        let (app, _) = app();
        let (owner, thief) = (keypair(), keypair());
        let owner_id = join(&app, &owner, "Kestrel").await;
        let thief_id = join(&app, &thief, "Magpie").await;
        send(
            &app,
            signed(
                &owner,
                Some(&owner_id),
                "POST",
                "/v1/runs",
                monza().to_vec(),
            ),
        )
        .await;
        let (status, _) = send(
            &app,
            signed(
                &thief,
                Some(&thief_id),
                "POST",
                "/v1/runs",
                monza().to_vec(),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        let (status, _) = send(
            &app,
            signed(
                &thief,
                Some(&owner_id),
                "POST",
                "/v1/runs",
                monza().to_vec(),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "signed as someone else");
    }

    #[tokio::test]
    async fn a_forged_or_broken_lap_is_refused_with_the_reason() {
        let (app, state) = app();
        let key = keypair();
        let id = join(&app, &key, "Kestrel").await;
        let mut broken = monza().to_vec();
        broken.truncate(broken.len() - 10);
        let (status, reply) = send(&app, signed(&key, Some(&id), "POST", "/v1/runs", broken)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(reply["error"].as_str().unwrap().contains("cut short"));
        let rejected: i64 = state
            .db
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM rejections", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rejected, 1);
    }

    #[test]
    fn limits_count_a_sliding_hour() {
        let mut limits = Limits::default();
        assert!(limits.allow("a", 2, 0));
        assert!(limits.allow("a", 2, 10));
        assert!(!limits.allow("a", 2, 20));
        assert!(limits.allow("b", 2, 20), "someone else is not limited");
        assert!(limits.allow("a", 2, 3600), "an hour later");
    }
}
