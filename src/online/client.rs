//! Talking to the leaderboard server, on a thread of its own.
//!
//! The game never waits on the network. It sends [`Ask`]s down a channel and
//! reads [`Heard`]s back with `try_recv` once a frame, which costs nothing when
//! there is nothing to read. The worker keeps the player's key, signs every
//! change, sends the runs waiting in the outbox whenever the player is online,
//! and backs off from 30 seconds to an hour while the server cannot be reached.
//!
//! The key lives in `identity.json` beside the laps, apart from the settings,
//! and never leaves the machine; the server only ever sees its public half.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD};
use bevy::prelude::*;
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};

/// Where the boards are, unless `TODORA_BOARD_URL` says otherwise.
pub(crate) const SERVER: &str = "https://todora.lukschander.com";
const FIRST_RETRY: Duration = Duration::from_secs(30);
const LAST_RETRY: Duration = Duration::from_secs(3600);

/// What the game asks of the worker.
#[derive(Debug)]
pub(crate) enum Ask {
    /// Go online (register if need be) or stay offline.
    Online {
        on: bool,
        name: String,
        country: Option<String>,
    },
    /// Keep a lap in the outbox under this name, and send it if online.
    Keep {
        name: String,
        bytes: Vec<u8>,
    },
    Board {
        circuit: String,
        mode: String,
        country: Option<String>,
        rivals: Vec<String>,
        /// A week's board, as `2026-W39`, instead of the all-time one.
        week: Option<String>,
    },
    Ghost(String),
    /// Your place on every circuit's board last seen, from the cache.
    Ranks {
        mode: String,
    },
    Rename {
        name: String,
        country: Option<String>,
    },
    /// Delete everything the server holds about this player, then the key.
    Forget,
}

/// What the worker says back.
#[derive(Debug, Clone)]
pub(crate) enum Heard {
    /// Registered, or already was, under this name.
    Joined {
        name: String,
    },
    /// A lap went up.
    Sent(Submitted),
    /// A lap was refused, and why; it is moved out of the outbox.
    Refused {
        circuit: String,
        why: String,
    },
    /// Laps still waiting.
    Waiting(usize),
    Board(Box<Standing>),
    Ghost {
        run: String,
        bytes: Vec<u8>,
    },
    /// Circuit, your place and how many are on the board, as last seen.
    Ranks(Vec<(String, u64, u64)>),
    /// The server could not be reached, or answered with trouble.
    Trouble(String),
    Forgotten,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) struct Submitted {
    pub run: String,
    pub seconds: f64,
    pub improved: bool,
    pub rank: Option<u64>,
    pub total: u64,
    #[serde(default)]
    pub circuit: String,
    #[serde(default)]
    pub mode: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) struct Place {
    pub rank: u64,
    pub player: String,
    pub name: String,
    pub country: Option<String>,
    pub steps: u32,
    pub seconds: f64,
    pub car: String,
    pub setup: String,
    pub multiplayer: bool,
    pub run: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) struct Standing {
    pub circuit: String,
    pub mode: String,
    pub season: u32,
    pub total: u64,
    pub top: Vec<Place>,
    pub around: Vec<Place>,
    pub you: Option<Place>,
    #[serde(default)]
    pub rivals: Vec<Place>,
    /// Unix seconds when this was fetched; set by the worker, kept in the cache.
    #[serde(default)]
    pub fetched_at: i64,
    /// The week this board is, for a weekly one; set by the worker.
    #[serde(default)]
    pub week: Option<String>,
}

pub(crate) fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// The two ends of the channel, as the game holds them.
#[derive(Resource)]
pub(crate) struct Client {
    asks: Sender<Ask>,
    heard: Mutex<Receiver<Heard>>,
}

impl Client {
    pub(crate) fn ask(&self, ask: Ask) {
        let _ = self.asks.send(ask);
    }

    /// Everything the worker has said since the last frame.
    pub(crate) fn heard(&self) -> impl Iterator<Item = Heard> + '_ {
        let receiver = self.heard.lock().expect("the channel");
        std::iter::from_fn(move || receiver.try_recv().ok())
    }

    /// Start the worker. `folder` is where the key and the outbox live.
    pub(crate) fn start(server: String, folder: Option<PathBuf>) -> Self {
        let (asks, from_game) = channel();
        let (to_game, heard) = channel();
        std::thread::Builder::new()
            .name("todora-online".into())
            .spawn(move || Worker::new(server, folder, to_game).run(from_game))
            .expect("the online thread");
        Self {
            asks,
            heard: Mutex::new(heard),
        }
    }
}

/// The key and who it belongs to, as kept on disk.
#[derive(Serialize, Deserialize, Default)]
struct Identity {
    /// The Ed25519 secret, base64. Made on this machine; never sent.
    secret: String,
    player: Option<String>,
}

struct Worker {
    server: String,
    folder: Option<PathBuf>,
    tell: Sender<Heard>,
    key: Option<SigningKey>,
    player: Option<String>,
    online: bool,
    retry: Duration,
}

impl Worker {
    fn new(server: String, folder: Option<PathBuf>, tell: Sender<Heard>) -> Self {
        Self {
            server: server.trim_end_matches('/').to_string(),
            folder,
            tell,
            key: None,
            player: None,
            online: false,
            retry: FIRST_RETRY,
        }
    }

    fn run(mut self, asks: Receiver<Ask>) {
        self.load();
        loop {
            let wait = if self.online { self.retry } else { LAST_RETRY };
            match asks.recv_timeout(wait) {
                Ok(ask) => self.handle(ask),
                Err(RecvTimeoutError::Timeout) => self.send_waiting(),
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn identity_path(&self) -> Option<PathBuf> {
        Some(self.folder.as_ref()?.join("identity.json"))
    }

    fn load(&mut self) {
        let Some(path) = self.identity_path() else {
            return;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        // One written before it was kept private is made private now.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        let Ok(identity) = serde_json::from_str::<Identity>(&text) else {
            warn!("identity.json could not be read; a new one is made on going online");
            return;
        };
        self.key = STANDARD
            .decode(&identity.secret)
            .ok()
            .and_then(|b| <[u8; 32]>::try_from(b).ok())
            .map(|b| SigningKey::from_bytes(&b));
        self.player = identity.player;
    }

    fn save(&self) {
        let (Some(path), Some(key)) = (self.identity_path(), &self.key) else {
            return;
        };
        let identity = Identity {
            secret: STANDARD.encode(key.to_bytes()),
            player: self.player.clone(),
        };
        let beside = path.with_extension("writing");
        let written = std::fs::create_dir_all(path.parent().unwrap_or(&path))
            .and_then(|()| {
                write_private(
                    &beside,
                    &serde_json::to_vec_pretty(&identity).unwrap_or_default(),
                )
            })
            .and_then(|()| std::fs::rename(&beside, &path));
        if let Err(trouble) = written {
            warn!("cannot save the online identity: {trouble}");
        }
    }

    fn key(&mut self) -> &SigningKey {
        if self.key.is_none() {
            let mut secret = [0u8; 32];
            getrandom::fill(&mut secret).expect("the system's random numbers");
            self.key = Some(SigningKey::from_bytes(&secret));
            self.save();
        }
        self.key.as_ref().expect("just made")
    }

    fn handle(&mut self, ask: Ask) {
        match ask {
            Ask::Online { on, name, country } => {
                self.online = on;
                if on {
                    if self.player.is_none() {
                        self.join(&name, country.as_deref());
                    } else {
                        let _ = self.tell.send(Heard::Joined { name });
                    }
                    self.send_waiting();
                }
            }
            Ask::Keep { name, bytes } => {
                if let Some(folder) = self.folder.as_ref().map(|f| f.join("outbox")) {
                    let path = folder.join(name);
                    let beside = path.with_extension("writing");
                    let written = std::fs::create_dir_all(&folder)
                        .and_then(|()| std::fs::write(&beside, &bytes))
                        .and_then(|()| std::fs::rename(&beside, &path));
                    if let Err(trouble) = written {
                        warn!(
                            "cannot keep the lap for upload at {}: {trouble}",
                            path.display()
                        );
                    }
                }
                self.retry = FIRST_RETRY;
                self.send_waiting();
            }
            Ask::Board {
                circuit,
                mode,
                country,
                rivals,
                week,
            } => self.board(
                &circuit,
                &mode,
                country.as_deref(),
                &rivals,
                week.as_deref(),
            ),
            Ask::Ghost(run) => self.ghost(&run),
            Ask::Ranks { mode } => {
                let ranks = self.cached_ranks(&mode);
                let _ = self.tell.send(Heard::Ranks(ranks));
            }
            Ask::Rename { name, country } => self.rename(&name, country.as_deref()),
            Ask::Forget => self.forget(),
        }
    }

    /// The headers that say who is asking and prove it.
    fn signed(
        &mut self,
        request: ureq::Request,
        method: &str,
        path: &str,
        body: &[u8],
    ) -> ureq::Request {
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let mut message = format!("{method} {path}\n{time}\n").into_bytes();
        message.extend_from_slice(body);
        let signature = STANDARD.encode(self.key().sign(&message).to_bytes());
        let mut request = request
            .set("x-todora-time", &time.to_string())
            .set("x-todora-signature", &signature)
            .set("user-agent", concat!("Todora/", env!("CARGO_PKG_VERSION")));
        if let Some(player) = &self.player {
            request = request.set("x-todora-player", player);
        }
        request
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.server)
    }

    fn agent() -> ureq::Agent {
        ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(20))
            .build()
    }

    fn trouble(&self, what: &str, error: ureq::Error) -> Option<String> {
        let text = match error {
            ureq::Error::Status(code, response) => {
                let reason = response
                    .into_json::<serde_json::Value>()
                    .ok()
                    .and_then(|v| v["error"].as_str().map(str::to_string))
                    .unwrap_or_else(|| format!("status {code}"));
                return Some(reason);
            }
            ureq::Error::Transport(transport) => format!("{what}: {transport}"),
        };
        let _ = self.tell.send(Heard::Trouble(text));
        None
    }

    fn join(&mut self, name: &str, country: Option<&str>) {
        let public_key = STANDARD.encode(self.key().verifying_key().to_bytes());
        let body = serde_json::json!({"public_key": public_key, "name": name, "country": country})
            .to_string()
            .into_bytes();
        let request = Self::agent().post(&self.url("/v1/players"));
        let request = self.signed(request, "POST", "/v1/players", &body);
        match request
            .set("content-type", "application/json")
            .send_bytes(&body)
        {
            Ok(response) => {
                let reply: serde_json::Value = response.into_json().unwrap_or_default();
                if let (Some(player), Some(name)) = (reply["id"].as_str(), reply["name"].as_str()) {
                    self.player = Some(player.to_string());
                    self.save();
                    let _ = self.tell.send(Heard::Joined {
                        name: name.to_string(),
                    });
                }
            }
            Err(error) => {
                if let Some(reason) = self.trouble("joining", error) {
                    let _ = self.tell.send(Heard::Trouble(reason));
                }
            }
        }
    }

    fn outbox(&self) -> Vec<PathBuf> {
        let Some(folder) = self.folder.as_ref().map(|f| f.join("outbox")) else {
            return Vec::new();
        };
        let mut runs: Vec<_> = std::fs::read_dir(folder)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|e| e == "run"))
            .collect();
        runs.sort_by_key(|path| std::fs::metadata(path).and_then(|m| m.modified()).ok());
        runs
    }

    /// Send every run waiting, oldest first; stop at the first sign the server
    /// cannot be reached, and try again later.
    fn send_waiting(&mut self) {
        if !self.online || self.player.is_none() {
            let _ = self.tell.send(Heard::Waiting(self.outbox().len()));
            return;
        }
        let waiting = self.outbox();
        for path in &waiting {
            let Ok(bytes) = std::fs::read(path) else {
                continue;
            };
            let request = Self::agent().post(&self.url("/v1/runs"));
            let request = self.signed(request, "POST", "/v1/runs", &bytes);
            let circuit = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.split('-').next())
                .unwrap_or_default()
                .to_string();
            match request
                .set("content-type", "application/octet-stream")
                .send_bytes(&bytes)
            {
                Ok(response) => {
                    if let Ok(mut sent) = response.into_json::<Submitted>() {
                        if let Ok(run) = super::run::Run::decode(&bytes) {
                            sent.circuit = run.circuit;
                            sent.mode = run.mode.name().to_lowercase();
                        }
                        let _ = self.tell.send(Heard::Sent(sent));
                    }
                    let _ = std::fs::remove_file(path);
                }
                Err(ureq::Error::Status(code, response)) if code != 429 && code < 500 => {
                    let why = response
                        .into_json::<serde_json::Value>()
                        .ok()
                        .and_then(|v| v["error"].as_str().map(str::to_string))
                        .unwrap_or_else(|| format!("status {code}"));
                    self.set_aside(path, &why);
                    let _ = self.tell.send(Heard::Refused { circuit, why });
                }
                Err(error) => {
                    self.trouble("sending a lap", error);
                    self.retry = (self.retry * 2).min(LAST_RETRY);
                    let _ = self.tell.send(Heard::Waiting(self.outbox().len()));
                    return;
                }
            }
        }
        self.retry = FIRST_RETRY;
        let _ = self.tell.send(Heard::Waiting(self.outbox().len()));
    }

    /// A refused run is kept, with the reason, but no longer sent.
    fn set_aside(&self, path: &std::path::Path, why: &str) {
        let Some(folder) = self.folder.as_ref().map(|f| f.join("refused")) else {
            return;
        };
        let _ = std::fs::create_dir_all(&folder);
        if let Some(name) = path.file_name() {
            let target = folder.join(name);
            let _ = std::fs::rename(path, &target);
            let _ = std::fs::write(target.with_extension("why"), why);
        }
    }

    fn board(
        &mut self,
        circuit: &str,
        mode: &str,
        country: Option<&str>,
        rivals: &[String],
        week: Option<&str>,
    ) {
        let mut url = self.url(&format!("/v1/boards/{circuit}/{mode}"));
        let mut query = Vec::new();
        if let Some(player) = &self.player {
            query.push(format!("player={player}"));
        }
        if let Some(country) = country {
            query.push(format!("country={country}"));
        }
        if !rivals.is_empty() {
            query.push(format!("rivals={}", rivals.join(",")));
        }
        if let Some(week) = week {
            query.push(format!("week={week}"));
        }
        if !query.is_empty() {
            url = format!("{url}?{}", query.join("&"));
        }
        let name = match week {
            Some(week) => format!("{circuit}-{mode}-{week}.json"),
            None => format!("{circuit}-{mode}.json"),
        };
        let cache = self.folder.as_ref().map(|f| f.join("boards").join(name));
        match Self::agent().get(&url).call() {
            Ok(response) => match response.into_json::<Standing>() {
                Ok(mut standing) => {
                    standing.fetched_at = unix_now();
                    standing.week = week.map(str::to_string);
                    if let Some(cache) = &cache {
                        let _ = std::fs::create_dir_all(cache.parent().unwrap_or(cache));
                        let _ = std::fs::write(
                            cache,
                            serde_json::to_vec(&standing).unwrap_or_default(),
                        );
                    }
                    let _ = self.tell.send(Heard::Board(Box::new(standing)));
                }
                Err(trouble) => {
                    let _ = self
                        .tell
                        .send(Heard::Trouble(format!("reading a board: {trouble}")));
                }
            },
            Err(error) => {
                if let Some(reason) = self.trouble("reading a board", error) {
                    let _ = self.tell.send(Heard::Trouble(reason));
                }
                // Offline, the board last seen is better than none.
                let cached = cache
                    .and_then(|c| std::fs::read(c).ok())
                    .and_then(|b| serde_json::from_slice::<Standing>(&b).ok());
                if let Some(standing) = cached {
                    let _ = self.tell.send(Heard::Board(Box::new(standing)));
                }
            }
        }
    }

    fn cached_ranks(&self, mode: &str) -> Vec<(String, u64, u64)> {
        let Some(folder) = self.folder.as_ref().map(|f| f.join("boards")) else {
            return Vec::new();
        };
        crate::track::all_circuits()
            .iter()
            .filter_map(|circuit| {
                let bytes =
                    std::fs::read(folder.join(format!("{}-{mode}.json", circuit.id))).ok()?;
                let standing: Standing = serde_json::from_slice(&bytes).ok()?;
                Some((circuit.id.to_string(), standing.you?.rank, standing.total))
            })
            .collect()
    }

    fn ghost(&mut self, run: &str) {
        match Self::agent()
            .get(&self.url(&format!("/v1/runs/{run}")))
            .call()
        {
            Ok(response) => {
                let mut bytes = Vec::new();
                let read = std::io::Read::read_to_end(
                    &mut std::io::Read::take(
                        response.into_reader(),
                        super::run::MAX_BYTES as u64 + 1,
                    ),
                    &mut bytes,
                );
                if read.is_ok() && bytes.len() <= super::run::MAX_BYTES {
                    let _ = self.tell.send(Heard::Ghost {
                        run: run.to_string(),
                        bytes,
                    });
                }
            }
            Err(error) => {
                if let Some(reason) = self.trouble("downloading a ghost", error) {
                    let _ = self.tell.send(Heard::Trouble(reason));
                }
            }
        }
    }

    fn rename(&mut self, name: &str, country: Option<&str>) {
        let Some(player) = self.player.clone() else {
            return;
        };
        let path = format!("/v1/players/{player}");
        let body = serde_json::json!({"name": name, "country": country.unwrap_or("")})
            .to_string()
            .into_bytes();
        let request = Self::agent().request("PATCH", &self.url(&path));
        let request = self.signed(request, "PATCH", &path, &body);
        match request
            .set("content-type", "application/json")
            .send_bytes(&body)
        {
            Ok(response) => {
                let reply: serde_json::Value = response.into_json().unwrap_or_default();
                if let Some(name) = reply["name"].as_str() {
                    let _ = self.tell.send(Heard::Joined {
                        name: name.to_string(),
                    });
                }
            }
            Err(error) => {
                if let Some(reason) = self.trouble("renaming", error) {
                    let _ = self.tell.send(Heard::Trouble(reason));
                }
            }
        }
    }

    fn forget(&mut self) {
        if let Some(player) = self.player.clone() {
            let path = format!("/v1/players/{player}");
            let request = Self::agent().request("DELETE", &self.url(&path));
            let request = self.signed(request, "DELETE", &path, b"");
            if let Err(error) = request.call() {
                if let Some(reason) = self.trouble("deleting", error) {
                    let _ = self.tell.send(Heard::Trouble(reason));
                }
                return;
            }
        }
        self.player = None;
        self.key = None;
        self.online = false;
        if let Some(path) = self.identity_path() {
            let _ = std::fs::remove_file(path);
        }
        let _ = self.tell.send(Heard::Forgotten);
    }
}

/// Write a file only its owner can read: it holds the key that signs laps.
fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    // A leftover from an earlier failed write keeps its old mode; fix it.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Everything the worker says within `seconds`, until `done` is heard.
    fn wait(client: &Client, seconds: u64, done: impl Fn(&Heard) -> bool) -> Vec<Heard> {
        let until = std::time::Instant::now() + Duration::from_secs(seconds);
        let mut heard = Vec::new();
        while std::time::Instant::now() < until {
            for said in client.heard() {
                let finished = done(&said);
                heard.push(said);
                if finished {
                    return heard;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("timed out; heard {heard:?}");
    }

    #[cfg(unix)]
    #[test]
    fn the_key_is_kept_where_only_its_owner_can_read_it() {
        use std::os::unix::fs::PermissionsExt;
        let folder = std::env::temp_dir().join(format!("todora-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let (tell, _heard) = channel();
        let mut worker = Worker::new(String::new(), Some(folder.clone()), tell);
        worker.key();
        worker.save();
        let path = folder.join("identity.json");
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        // One left readable by an older version is closed up on load.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let (tell, _heard) = channel();
        let mut again = Worker::new(String::new(), Some(folder.clone()), tell);
        again.load();
        assert_eq!(mode(&path), 0o600);
        assert!(again.key.is_some());
        let _ = std::fs::remove_dir_all(&folder);
    }

    /// The whole round trip against a running server:
    /// `TODORA_E2E_URL=http://127.0.0.1:18787 cargo test --locked --lib the_client_goes_online -- --ignored`
    #[test]
    #[ignore = "needs a running server at TODORA_E2E_URL"]
    fn the_client_goes_online_uploads_reads_downloads_and_forgets() {
        let Ok(url) = std::env::var("TODORA_E2E_URL") else {
            return;
        };
        let folder = std::env::temp_dir().join(format!("todora-e2e-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let client = Client::start(url, Some(folder.clone()));
        client.ask(Ask::Online {
            on: true,
            name: "Test Driver".into(),
            country: Some("AT".into()),
        });
        wait(&client, 20, |h| matches!(h, Heard::Joined { .. }));
        assert!(folder.join("identity.json").exists(), "the key was kept");

        let bytes = crate::verify::ai_run("monza", "regular").expect("an AI lap");
        client.ask(Ask::Keep {
            name: "monza-regular-e2e.run".into(),
            bytes: bytes.clone(),
        });
        let heard = wait(&client, 60, |h| {
            matches!(h, Heard::Sent(_) | Heard::Refused { .. })
        });
        let Some(Heard::Sent(sent)) = heard.last() else {
            panic!("the lap was refused: {heard:?}");
        };
        assert!(sent.improved && sent.rank.is_some(), "{sent:?}");
        assert!(
            !folder.join("outbox/monza-regular-e2e.run").exists(),
            "sent laps leave the outbox"
        );

        client.ask(Ask::Board {
            circuit: "monza".into(),
            mode: "regular".into(),
            country: Some("AT".into()),
            rivals: Vec::new(),
            week: None,
        });
        let heard = wait(&client, 20, |h| matches!(h, Heard::Board(_)));
        let Some(Heard::Board(board)) = heard.last() else {
            unreachable!()
        };
        assert_eq!(
            board.you.as_ref().map(|p| p.run.as_str()),
            Some(sent.run.as_str())
        );
        assert!(
            folder.join("boards/monza-regular.json").exists(),
            "the board was cached"
        );

        client.ask(Ask::Ghost(sent.run.clone()));
        let heard = wait(&client, 20, |h| matches!(h, Heard::Ghost { .. }));
        let Some(Heard::Ghost { bytes: ghost, .. }) = heard.last() else {
            unreachable!()
        };
        assert_eq!(ghost, &bytes);

        client.ask(Ask::Forget);
        wait(&client, 20, |h| matches!(h, Heard::Forgotten));
        assert!(!folder.join("identity.json").exists(), "the key is gone");
        let _ = std::fs::remove_dir_all(&folder);
    }
}
