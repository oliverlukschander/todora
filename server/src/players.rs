//! A player's own data, reports, and the little moderation there is.
//!
//! | Method | Path | What |
//! | --- | --- | --- |
//! | PATCH | `/v1/players/{id}` | a new name (once a week) or country (signed) |
//! | DELETE | `/v1/players/{id}` | delete the player and every lap and report of theirs (signed) |
//! | POST | `/v1/reports` | report a lap that looks wrong (signed) |
//! | GET | `/admin/reports` | reported laps, most reported first (admin token) |
//! | POST | `/admin/runs/{id}/hide` | take a lap off the boards (admin token) |
//!
//! Times appear the moment they are verified; a report puts a lap in front of
//! the admin, who watches it as a ghost and can hide it.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::{get, patch, post},
};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use todora::verify;

use crate::api::{AppState, Problem, bad, internal, signed_by};
use crate::db;

/// Seconds between name changes.
const RENAME_EVERY: i64 = 7 * 24 * 3600;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/players/{id}", patch(rename).delete(forget))
        .route("/v1/reports", post(report))
        .route("/admin/reports", get(reports))
        .route("/admin/runs/{id}/hide", post(hide))
}

#[derive(Deserialize)]
struct Change {
    name: Option<String>,
    country: Option<String>,
}

async fn rename(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<db::Player>, Problem> {
    let path = format!("/v1/players/{id}");
    let player = signed_by(&state, &headers, "PATCH", &path, &body)?;
    if player != id {
        return Err(Problem(StatusCode::FORBIDDEN, "not your player".into()));
    }
    let change: Change = serde_json::from_slice(&body).map_err(|_| bad("not a change"))?;
    let db = state.db.lock().expect("the database");
    let (mut current, _, renamed_at) = db::player_by_id(&db, &id)
        .map_err(internal)?
        .ok_or_else(|| Problem(StatusCode::NOT_FOUND, "no such player".into()))?;
    if let Some(name) = change.name {
        let name = verify::valid_name(&name).map_err(bad)?;
        if name != current.name {
            if db::now() - renamed_at < RENAME_EVERY {
                return Err(Problem(
                    StatusCode::TOO_MANY_REQUESTS,
                    "a name can change once a week".into(),
                ));
            }
            db.execute(
                "UPDATE players SET name = ?1, renamed_at = ?2 WHERE id = ?3",
                params![name, db::now(), id],
            )
            .map_err(internal)?;
            current.name = name;
        }
    }
    if let Some(country) = change.country {
        let country = if country.is_empty() {
            None
        } else {
            Some(verify::valid_country(&country).ok_or_else(|| bad("not a country code"))?)
        };
        db.execute(
            "UPDATE players SET country = ?1 WHERE id = ?2",
            params![country, id],
        )
        .map_err(internal)?;
        current.country = country;
    }
    Ok(Json(current))
}

/// Everything the server holds about a player goes: the player, their laps,
/// their places, their reports. The database cascades it.
async fn forget(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, Problem> {
    let path = format!("/v1/players/{id}");
    let player = signed_by(&state, &headers, "DELETE", &path, b"")?;
    if player != id {
        return Err(Problem(StatusCode::FORBIDDEN, "not your player".into()));
    }
    let db = state.db.lock().expect("the database");
    db.execute("DELETE FROM rejections WHERE player = ?1", params![id])
        .map_err(internal)?;
    db.execute("DELETE FROM players WHERE id = ?1", params![id])
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct Report {
    run: String,
    reason: String,
}

async fn report(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, Problem> {
    let player = signed_by(&state, &headers, "POST", "/v1/reports", &body)?;
    let report: Report = serde_json::from_slice(&body).map_err(|_| bad("not a report"))?;
    let reason: String = report.reason.trim().chars().take(200).collect();
    let db = state.db.lock().expect("the database");
    if !db::run_known(&db, &report.run).map_err(internal)? {
        return Err(Problem(StatusCode::NOT_FOUND, "no such lap".into()));
    }
    db.execute(
        "INSERT OR REPLACE INTO reports (run, reporter, reason, at) VALUES (?1, ?2, ?3, ?4)",
        params![report.run, player, reason, db::now()],
    )
    .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

fn admin(state: &AppState, headers: &HeaderMap) -> Result<(), Problem> {
    let given = headers.get("x-admin-token").and_then(|v| v.to_str().ok());
    match (&state.admin_token, given) {
        (Some(token), Some(given)) if token == given => Ok(()),
        _ => Err(Problem(StatusCode::UNAUTHORIZED, "admin only".into())),
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Reported {
    run: String,
    player: String,
    name: String,
    circuit: String,
    mode: String,
    seconds: f64,
    reports: u32,
    reasons: Vec<String>,
    hidden: bool,
}

async fn reports(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Reported>>, Problem> {
    admin(&state, &headers)?;
    let db = state.db.lock().expect("the database");
    let mut statement = db
        .prepare(
            "SELECT r.id, p.id, p.name, r.circuit, r.mode, r.steps, COUNT(*), GROUP_CONCAT(x.reason, '\n'), r.hidden
             FROM reports x JOIN runs r ON r.id = x.run JOIN players p ON p.id = r.player
             GROUP BY r.id ORDER BY COUNT(*) DESC, MAX(x.at) DESC LIMIT 200",
        )
        .map_err(internal)?;
    let rows = statement
        .query_map([], |row| {
            Ok(Reported {
                run: row.get(0)?,
                player: row.get(1)?,
                name: row.get(2)?,
                circuit: row.get(3)?,
                mode: row.get(4)?,
                seconds: verify::seconds(row.get(5)?),
                reports: row.get(6)?,
                reasons: row
                    .get::<_, String>(7)?
                    .lines()
                    .map(str::to_string)
                    .collect(),
                hidden: row.get::<_, i64>(8)? != 0,
            })
        })
        .map_err(internal)?;
    Ok(Json(rows.collect::<Result<_, _>>().map_err(internal)?))
}

/// Take a lap off the boards. The player's place falls back to their next
/// best lap on that board, if they have one.
async fn hide(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<StatusCode, Problem> {
    admin(&state, &headers)?;
    let db = state.db.lock().expect("the database");
    let found: Option<(String, String, String, u32)> = db
        .query_row(
            "SELECT player, circuit, mode, physics FROM runs WHERE id = ?1",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(internal)?;
    let Some((player, circuit, mode, physics)) = found else {
        return Err(Problem(StatusCode::NOT_FOUND, "no such lap".into()));
    };
    db.execute("UPDATE runs SET hidden = 1 WHERE id = ?1", params![id])
        .map_err(internal)?;
    db.execute(
        "DELETE FROM best WHERE player = ?1 AND circuit = ?2 AND mode = ?3 AND physics = ?4",
        params![player, circuit, mode, physics],
    )
    .map_err(internal)?;
    db.execute(
        "INSERT INTO best (player, circuit, mode, physics, run, steps, set_at)
         SELECT player, circuit, mode, physics, id, steps, created_at FROM runs
         WHERE player = ?1 AND circuit = ?2 AND mode = ?3 AND physics = ?4 AND hidden = 0
         ORDER BY steps, created_at LIMIT 1",
        params![player, circuit, mode, physics],
    )
    .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use crate::api::tests::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};

    #[tokio::test]
    async fn a_name_changes_once_a_week_and_a_country_any_time() {
        let (app, state) = app();
        let key = keypair();
        let id = join(&app, &key, "Kestrel").await;
        let path = format!("/v1/players/{id}");
        // Registered just now: the first rename has to wait a week too.
        let body = br#"{"name": "Merlin"}"#.to_vec();
        let (status, _) = send(&app, signed(&key, Some(&id), "PATCH", &path, body.clone())).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        state
            .db
            .lock()
            .unwrap()
            .execute("UPDATE players SET renamed_at = 0", [])
            .unwrap();
        let (status, reply) = send(&app, signed(&key, Some(&id), "PATCH", &path, body)).await;
        assert_eq!(status, StatusCode::OK, "{reply}");
        assert_eq!(reply["name"], "Merlin");
        let (_, reply) = send(
            &app,
            signed(
                &key,
                Some(&id),
                "PATCH",
                &path,
                br#"{"country": "de"}"#.to_vec(),
            ),
        )
        .await;
        assert_eq!(reply["country"], "DE");
        let other = keypair();
        let other_id = join(&app, &other, "Magpie").await;
        let (status, _) = send(
            &app,
            signed(
                &other,
                Some(&other_id),
                "PATCH",
                &path,
                br#"{"country": "FR"}"#.to_vec(),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "someone else's player");
    }

    #[tokio::test]
    async fn delete_my_data_removes_everything() {
        let (app, state) = app();
        let key = keypair();
        let id = join(&app, &key, "Kestrel").await;
        send(
            &app,
            signed(&key, Some(&id), "POST", "/v1/runs", monza().to_vec()),
        )
        .await;
        let path = format!("/v1/players/{id}");
        let (status, _) = send(&app, signed(&key, Some(&id), "DELETE", &path, Vec::new())).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let db = state.db.lock().unwrap();
        for table in ["players", "runs", "best", "reports", "rejections"] {
            let n: i64 = db
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "{table} kept something");
        }
    }

    #[tokio::test]
    async fn a_reported_lap_can_be_hidden_by_the_admin_only() {
        let (app, _) = app();
        let (owner, reporter) = (keypair(), keypair());
        let owner_id = join(&app, &owner, "Kestrel").await;
        let reporter_id = join(&app, &reporter, "Magpie").await;
        let (_, reply) = send(
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
        let run = reply["run"].as_str().unwrap().to_string();
        let body = serde_json::json!({"run": run, "reason": "impossible line through the chicane"})
            .to_string()
            .into_bytes();
        let (status, _) = send(
            &app,
            signed(&reporter, Some(&reporter_id), "POST", "/v1/reports", body),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let admin = |path: &str, method: &str, token: &str| {
            Request::builder()
                .method(method)
                .uri(path)
                .header("x-admin-token", token)
                .body(Body::empty())
                .unwrap()
        };
        assert_eq!(
            send(&app, admin("/admin/reports", "GET", "wrong")).await.0,
            StatusCode::UNAUTHORIZED
        );
        let (status, list) = send(&app, admin("/admin/reports", "GET", "secret")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(list[0]["run"], run);
        assert_eq!(list[0]["reports"], 1);
        let (status, _) = send(
            &app,
            admin(&format!("/admin/runs/{run}/hide"), "POST", "secret"),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let board = Request::builder()
            .uri("/v1/boards/monza/regular")
            .body(Body::empty())
            .unwrap();
        let (_, board) = send(&app, board).await;
        assert_eq!(board["total"], 0, "a hidden lap is off the board");
        let ghost = Request::builder()
            .uri(format!("/v1/runs/{run}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(send(&app, ghost).await.0, StatusCode::NOT_FOUND);
    }
}
