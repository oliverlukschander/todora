//! Todora's leaderboard server.
//!
//! One binary and one SQLite file. Every submitted lap is driven again by the
//! game's own engine and lap judge before it counts, so a time on the board is
//! a lap somebody drove. Configuration is three environment variables:
//!
//! - `TODORA_DB` — the database file (default `todora.db`)
//! - `TODORA_LISTEN` — the address to listen on (default `127.0.0.1:8787`;
//!   put a reverse proxy with TLS in front of it)
//! - `TODORA_ADMIN_TOKEN` — enables the moderation routes when set

mod api;
mod auth;
mod boards;
mod db;
mod players;
mod seed;

#[tokio::main]
async fn main() {
    let path = std::env::var("TODORA_DB").unwrap_or_else(|_| "todora.db".into());
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("seed") {
        let db = db::open(&path).unwrap_or_else(|trouble| panic!("cannot open {path}: {trouble}"));
        seed::run(&db, &args[1..]);
        return;
    }
    let listen = std::env::var("TODORA_LISTEN").unwrap_or_else(|_| "127.0.0.1:8787".into());
    let admin = std::env::var("TODORA_ADMIN_TOKEN")
        .ok()
        .filter(|t| t.len() >= 24);
    let db = db::open(&path).unwrap_or_else(|trouble| panic!("cannot open {path}: {trouble}"));
    let app = api::router(api::state(db, admin));
    let listener = tokio::net::TcpListener::bind(&listen)
        .await
        .unwrap_or_else(|trouble| panic!("cannot listen on {listen}: {trouble}"));
    eprintln!(
        "todora-server {} (game {}, physics {}) on {listen}, data in {path}",
        env!("CARGO_PKG_VERSION"),
        todora::verify::APP_VERSION,
        todora::verify::PHYSICS_VERSION
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("the server");
}
