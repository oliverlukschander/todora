//! Reading the boards and downloading the laps on them.
//!
//! | Method | Path | What |
//! | --- | --- | --- |
//! | GET | `/v1/boards/{circuit}/{mode}` | the top ten, the world record, and with `?player=` that player ±5 |
//! | GET | `/v1/runs/{id}` | a lap's run bytes, to race as a ghost |
//!
//! `?country=AT` and `?car=clubman` narrow a board; `?season=` reads an
//! earlier physics version's board; `?rivals=id,id` adds up to five pinned
//! players' places. Every read is a few indexed queries.

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use serde::{Deserialize, Serialize};
use todora::verify;

use crate::api::{AppState, Problem, bad, internal};
use crate::db::{self, Board, Place};

/// Places shown at the top, and either side of the player.
const TOP: u64 = 10;
const AROUND: u64 = 5;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/v1/boards/{circuit}/{mode}", get(board))
        .route("/v1/runs/{id}", get(run))
}

#[derive(Deserialize, Default)]
pub struct Asked {
    player: Option<String>,
    country: Option<String>,
    car: Option<String>,
    season: Option<u32>,
    rivals: Option<String>,
    /// `2026-W39`: that week's board, from laps set during it.
    week: Option<String>,
}

/// `2026-W39` as (2026, 39).
fn parse_week(text: &str) -> Option<(i32, u32)> {
    let (year, week) = text.split_once("-W")?;
    let (year, week) = (year.parse().ok()?, week.parse().ok()?);
    (1..=53).contains(&week).then_some((year, week))
}

/// The most rivals one board read looks up.
const RIVALS: usize = 5;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Standing {
    pub circuit: String,
    pub mode: String,
    pub season: u32,
    pub total: u64,
    pub top: Vec<Place>,
    /// The player and up to five either side, when a player was asked about
    /// and is on the board.
    pub around: Vec<Place>,
    pub you: Option<Place>,
    /// Pinned rivals who are on the board, fastest first.
    #[serde(default)]
    pub rivals: Vec<Place>,
}

async fn board(
    State(state): State<AppState>,
    Path((circuit, mode)): Path<(String, String)>,
    Query(asked): Query<Asked>,
) -> Result<Json<Standing>, Problem> {
    if !verify::circuits().iter().any(|(id, _)| *id == circuit) {
        return Err(Problem(StatusCode::NOT_FOUND, "no such circuit".into()));
    }
    if !verify::modes().contains(&mode.as_str()) {
        return Err(Problem(StatusCode::NOT_FOUND, "no such mode".into()));
    }
    let country = match asked.country.as_deref() {
        Some(code) => Some(verify::valid_country(code).ok_or_else(|| bad("not a country code"))?),
        None => None,
    };
    let season = asked.season.unwrap_or(verify::PHYSICS_VERSION);
    let board = Board {
        circuit: &circuit,
        mode: &mode,
        physics: season,
        country: country.as_deref(),
        car: asked.car.as_deref(),
    };
    let db = state.db.lock().expect("the database");
    if let Some(week) = &asked.week {
        let (year, number) = parse_week(week).ok_or_else(|| bad("a week is written 2026-W39"))?;
        let all = db::week(
            &db,
            &circuit,
            &mode,
            season,
            verify::week_bounds(year, number),
        )
        .map_err(internal)?;
        let at = asked
            .player
            .as_ref()
            .and_then(|p| all.iter().position(|place| place.player == *p));
        let around = at.map_or(Vec::new(), |at| {
            all[at.saturating_sub(AROUND as usize)..(at + AROUND as usize + 1).min(all.len())]
                .to_vec()
        });
        return Ok(Json(Standing {
            circuit,
            mode,
            season,
            total: all.len() as u64,
            top: all.iter().take(TOP as usize).cloned().collect(),
            you: at.map(|at| all[at].clone()),
            around,
            rivals: Vec::new(),
        }));
    }
    let total = db::count(&db, &board).map_err(internal)?;
    let top = db::places(&db, &board, 0, TOP).map_err(internal)?;
    let (mut around, mut you) = (Vec::new(), None);
    if let Some(player) = &asked.player
        && let Some(rank) = db::rank_of(&db, &board, player).map_err(internal)?
    {
        let first = rank.saturating_sub(AROUND + 1);
        around = db::places(&db, &board, first, AROUND * 2 + 1).map_err(internal)?;
        you = around.iter().find(|p| p.player == *player).cloned();
    }
    let mut rivals = Vec::new();
    for rival in asked.rivals.iter().flat_map(|r| r.split(',')).take(RIVALS) {
        if let Some(rank) = db::rank_of(&db, &board, rival).map_err(internal)? {
            rivals.extend(db::places(&db, &board, rank - 1, 1).map_err(internal)?);
        }
    }
    rivals.sort_by_key(|p| p.rank);
    Ok(Json(Standing {
        circuit,
        mode,
        season,
        total,
        top,
        around,
        you,
        rivals,
    }))
}

async fn run(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, Problem> {
    if id.len() != 16 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Problem(StatusCode::NOT_FOUND, "no such lap".into()));
    }
    let bytes = db::run_bytes(&state.db.lock().expect("the database"), &id)
        .map_err(internal)?
        .ok_or_else(|| Problem(StatusCode::NOT_FOUND, "no such lap".into()))?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (header::CACHE_CONTROL, "public, max-age=86400, immutable"),
        ],
        bytes,
    ))
}

#[cfg(test)]
mod tests {
    use crate::api::tests::*;
    use crate::db;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use todora::verify;

    fn get(path: &str) -> Request<Body> {
        Request::builder().uri(path).body(Body::empty()).unwrap()
    }

    /// Put `n` players straight onto the Monza board with made-up times,
    /// the way `seed` does, without replaying anything.
    fn fill(state: &crate::api::AppState, n: u32) -> Vec<String> {
        let db = state.db.lock().unwrap();
        (0..n)
            .map(|i| {
                let id = format!("p{i:04}");
                db::add_player(
                    &db,
                    &id,
                    &i.to_le_bytes().repeat(8),
                    &format!("Driver {i}"),
                    if i % 2 == 0 { Some("AT") } else { Some("DE") },
                )
                .unwrap();
                let checked = verify::Checked {
                    circuit: "monza".into(),
                    mode: "regular".into(),
                    car: if i % 3 == 0 { "clubman" } else { "tourer" }.into(),
                    setup: "balanced".into(),
                    steps: 12_000 + i * 10,
                    sectors: vec![],
                    app_version: verify::APP_VERSION.into(),
                    physics: verify::PHYSICS_VERSION,
                    fingerprint: 0,
                    multiplayer: false,
                    lap: u64::from(i),
                };
                db::keep_run(&db, &format!("{i:016x}"), &id, &checked, b"run").unwrap();
                id
            })
            .collect()
    }

    #[tokio::test]
    async fn a_board_shows_the_top_and_the_player_five_either_side() {
        let (app, state) = app();
        let players = fill(&state, 40);
        let (status, board) = send(
            &app,
            get(&format!("/v1/boards/monza/regular?player={}", players[20])),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{board}");
        assert_eq!(board["total"], 40);
        assert_eq!(board["top"].as_array().unwrap().len(), 10);
        assert_eq!(board["top"][0]["name"], "Driver 0");
        assert_eq!(board["top"][0]["rank"], 1);
        let around = board["around"].as_array().unwrap();
        assert_eq!(around.len(), 11);
        assert_eq!(around[0]["rank"], 16);
        assert_eq!(board["you"]["rank"], 21);
        assert_eq!(board["you"]["player"], players[20]);
    }

    #[tokio::test]
    async fn pinned_rivals_come_back_wherever_they_are() {
        let (app, state) = app();
        let players = fill(&state, 30);
        let asked = format!(
            "/v1/boards/monza/regular?rivals={},{},nobody",
            players[25], players[3]
        );
        let (_, board) = send(&app, get(&asked)).await;
        let rivals = board["rivals"].as_array().unwrap();
        assert_eq!(rivals.len(), 2);
        assert_eq!(rivals[0]["rank"], 4);
        assert_eq!(rivals[1]["rank"], 26);
    }

    #[tokio::test]
    async fn a_weeks_board_holds_only_that_weeks_laps() {
        let (app, state) = app();
        fill(&state, 6);
        let now = db::now();
        let (year, week) = verify::iso_week(now);
        {
            let db = state.db.lock().unwrap();
            // Two of the laps were set long ago; one player also improved this week.
            db.execute(
                "UPDATE runs SET created_at = 0 WHERE player IN ('p0000', 'p0001')",
                [],
            )
            .unwrap();
        }
        let path = format!("/v1/boards/monza/regular?week={year}-W{week:02}&player=p0003");
        let (status, board) = send(&app, get(&path)).await;
        assert_eq!(status, StatusCode::OK, "{board}");
        assert_eq!(board["total"], 4);
        assert_eq!(board["top"][0]["name"], "Driver 2");
        assert_eq!(board["you"]["rank"], 2);
        assert_eq!(
            send(&app, get("/v1/boards/monza/regular?week=soon"))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn a_board_narrows_to_a_country_or_a_car() {
        let (app, state) = app();
        fill(&state, 12);
        let (_, board) = send(&app, get("/v1/boards/monza/regular?country=de")).await;
        assert_eq!(board["total"], 6);
        assert_eq!(board["top"][0]["name"], "Driver 1");
        assert_eq!(board["top"][0]["rank"], 1, "ranked within the country");
        let (_, board) = send(&app, get("/v1/boards/monza/regular?car=clubman")).await;
        assert_eq!(board["total"], 4);
    }

    #[tokio::test]
    async fn unknown_boards_and_laps_are_not_found() {
        let (app, _) = app();
        assert_eq!(
            send(&app, get("/v1/boards/atlantis/regular")).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, get("/v1/boards/monza/turbo")).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, get("/v1/runs/0000000000000000")).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, get("/v1/runs/../../etc")).await.0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, get("/v1/boards/monza/regular?country=Austria"))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn a_submitted_lap_can_be_downloaded_again() {
        let (app, _) = app();
        let key = keypair();
        let id = join(&app, &key, "Kestrel").await;
        let (_, reply) = send(
            &app,
            signed(&key, Some(&id), "POST", "/v1/runs", monza().to_vec()),
        )
        .await;
        let run = reply["run"].as_str().unwrap();
        let response = tower::ServiceExt::oneshot(app.clone(), get(&format!("/v1/runs/{run}")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = http_body_util::BodyExt::collect(response.into_body())
            .await
            .unwrap()
            .to_bytes();
        assert_eq!(&bytes[..], monza());
    }

    #[tokio::test]
    async fn an_older_season_stays_readable_and_apart() {
        let (app, state) = app();
        fill(&state, 3);
        let (_, board) = send(
            &app,
            get(&format!(
                "/v1/boards/monza/regular?season={}",
                verify::PHYSICS_VERSION + 1
            )),
        )
        .await;
        assert_eq!(board["total"], 0);
    }
}
