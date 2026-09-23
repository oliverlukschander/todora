//! Everything the server keeps, in one SQLite file.
//!
//! Players are a random id, a public key, a name and optionally a country — no
//! email, no password, no address. Runs are the bytes that were verified, so a
//! ghost can be served and a board recomputed under new physics. `best` is each
//! player's quickest verified run per board, where a board is a circuit, a
//! mode and a physics version: new physics is a new season and the old boards
//! stay readable. IP addresses are never written here.

use rusqlite::{Connection, OptionalExtension, params};

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS players (
    id TEXT PRIMARY KEY,
    public_key BLOB NOT NULL UNIQUE,
    name TEXT NOT NULL,
    country TEXT,
    renamed_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY,
    player TEXT NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    circuit TEXT NOT NULL,
    mode TEXT NOT NULL,
    physics INTEGER NOT NULL,
    car TEXT NOT NULL,
    setup TEXT NOT NULL,
    multiplayer INTEGER NOT NULL,
    steps INTEGER NOT NULL,
    sectors TEXT NOT NULL,
    app_version TEXT NOT NULL,
    bytes BLOB NOT NULL,
    hidden INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS best (
    player TEXT NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    circuit TEXT NOT NULL,
    mode TEXT NOT NULL,
    physics INTEGER NOT NULL,
    run TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    steps INTEGER NOT NULL,
    set_at INTEGER NOT NULL,
    PRIMARY KEY (player, circuit, mode, physics)
);
CREATE INDEX IF NOT EXISTS best_board ON best (circuit, mode, physics, steps, set_at);
CREATE TABLE IF NOT EXISTS rejections (
    player TEXT NOT NULL,
    reason TEXT NOT NULL,
    at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS reports (
    run TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    reporter TEXT NOT NULL REFERENCES players(id) ON DELETE CASCADE,
    reason TEXT NOT NULL,
    at INTEGER NOT NULL,
    PRIMARY KEY (run, reporter)
);
";

pub fn open(path: &str) -> rusqlite::Result<Connection> {
    let db = Connection::open(path)?;
    db.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    db.execute_batch(SCHEMA)?;
    Ok(db)
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Player {
    pub id: String,
    pub name: String,
    pub country: Option<String>,
}

pub fn player_by_id(db: &Connection, id: &str) -> rusqlite::Result<Option<(Player, Vec<u8>, i64)>> {
    db.query_row(
        "SELECT id, name, country, public_key, renamed_at FROM players WHERE id = ?1",
        params![id],
        |row| {
            Ok((
                Player {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    country: row.get(2)?,
                },
                row.get(3)?,
                row.get(4)?,
            ))
        },
    )
    .optional()
}

pub fn player_by_key(db: &Connection, key: &[u8]) -> rusqlite::Result<Option<String>> {
    db.query_row(
        "SELECT id FROM players WHERE public_key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
}

pub fn add_player(
    db: &Connection,
    id: &str,
    key: &[u8],
    name: &str,
    country: Option<&str>,
) -> rusqlite::Result<()> {
    let at = now();
    db.execute(
        "INSERT INTO players (id, public_key, name, country, renamed_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
        params![id, key, name, country, at],
    )?;
    Ok(())
}

/// One place on a board.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Place {
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

/// A board: a circuit and mode under one physics version, optionally one
/// country's and one car's part of it.
#[derive(Clone, Debug)]
pub struct Board<'a> {
    pub circuit: &'a str,
    pub mode: &'a str,
    pub physics: u32,
    pub country: Option<&'a str>,
    pub car: Option<&'a str>,
}

impl Board<'_> {
    /// The `WHERE` that picks this board's rows, joined with players and runs.
    fn filter(&self) -> (&'static str, Vec<rusqlite::types::Value>) {
        use rusqlite::types::Value;
        let mut values = vec![
            Value::Text(self.circuit.into()),
            Value::Text(self.mode.into()),
            Value::Integer(self.physics.into()),
        ];
        let sql = match (self.country, self.car) {
            (None, None) => "b.circuit = ?1 AND b.mode = ?2 AND b.physics = ?3 AND r.hidden = 0",
            (Some(country), None) => {
                values.push(Value::Text(country.into()));
                "b.circuit = ?1 AND b.mode = ?2 AND b.physics = ?3 AND r.hidden = 0 AND p.country = ?4"
            }
            (None, Some(car)) => {
                values.push(Value::Text(car.into()));
                "b.circuit = ?1 AND b.mode = ?2 AND b.physics = ?3 AND r.hidden = 0 AND r.car = ?4"
            }
            (Some(country), Some(car)) => {
                values.push(Value::Text(country.into()));
                values.push(Value::Text(car.into()));
                "b.circuit = ?1 AND b.mode = ?2 AND b.physics = ?3 AND r.hidden = 0 AND p.country = ?4 AND r.car = ?5"
            }
        };
        (sql, values)
    }
}

const FROM: &str = "FROM best b JOIN players p ON p.id = b.player JOIN runs r ON r.id = b.run";

const COLUMNS: &str = "p.id, p.name, p.country, b.steps, r.car, r.setup, r.multiplayer, b.run";

/// How many are on the board.
pub fn count(db: &Connection, board: &Board) -> rusqlite::Result<u64> {
    let (filter, values) = board.filter();
    db.query_row(
        &format!("SELECT COUNT(*) {FROM} WHERE {filter}"),
        rusqlite::params_from_iter(values),
        |row| row.get::<_, i64>(0).map(|n| n as u64),
    )
}

/// Places `offset + 1` onwards, `limit` of them. Ties go to whoever set the
/// time first.
pub fn places(
    db: &Connection,
    board: &Board,
    offset: u64,
    limit: u64,
) -> rusqlite::Result<Vec<Place>> {
    let (filter, values) = board.filter();
    let mut statement = db.prepare(&format!(
        "SELECT {COLUMNS} {FROM} WHERE {filter} ORDER BY b.steps, b.set_at, b.player LIMIT {limit} OFFSET {offset}"
    ))?;
    let rows = statement.query_map(rusqlite::params_from_iter(values), |row| Ok(row_owned(row)))?;
    let mut out = Vec::new();
    for (i, row) in rows.enumerate() {
        out.push(row??.into_place(offset + i as u64 + 1));
    }
    Ok(out)
}

/// Where a player stands on a board, if they are on it.
pub fn rank_of(db: &Connection, board: &Board, player: &str) -> rusqlite::Result<Option<u64>> {
    let (filter, mut values) = board.filter();
    let mine = db
        .query_row(
            &format!(
                "SELECT b.steps, b.set_at {FROM} WHERE {filter} AND b.player = ?{}",
                values.len() + 1
            ),
            rusqlite::params_from_iter(
                values
                    .iter()
                    .cloned()
                    .chain([rusqlite::types::Value::Text(player.into())]),
            ),
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    let Some((steps, set_at)) = mine else {
        return Ok(None);
    };
    let n = values.len();
    values.push(steps.into());
    values.push(set_at.into());
    values.push(player.to_string().into());
    let ahead: i64 = db.query_row(
        &format!(
            "SELECT COUNT(*) {FROM} WHERE {filter} AND (b.steps < ?{a} OR (b.steps = ?{a} AND (b.set_at < ?{b} OR (b.set_at = ?{b} AND b.player < ?{c}))))",
            a = n + 1,
            b = n + 2,
            c = n + 3
        ),
        rusqlite::params_from_iter(values),
        |row| row.get(0),
    )?;
    Ok(Some(ahead as u64 + 1))
}

/// A row read out before the rank is known.
struct Owned {
    values: (
        String,
        String,
        Option<String>,
        u32,
        String,
        String,
        i64,
        String,
    ),
}

fn row_owned(row: &rusqlite::Row) -> rusqlite::Result<Owned> {
    Ok(Owned {
        values: (
            row.get(0)?,
            row.get(1)?,
            row.get(2)?,
            row.get(3)?,
            row.get(4)?,
            row.get(5)?,
            row.get(6)?,
            row.get(7)?,
        ),
    })
}

impl Owned {
    fn into_place(self, rank: u64) -> Place {
        let (player, name, country, steps, car, setup, multiplayer, run) = self.values;
        Place {
            rank,
            player,
            name,
            country,
            steps,
            seconds: todora::verify::seconds(steps),
            car,
            setup,
            multiplayer: multiplayer != 0,
            run,
        }
    }
}

pub fn run_known(db: &Connection, id: &str) -> rusqlite::Result<bool> {
    db.query_row("SELECT 1 FROM runs WHERE id = ?1", params![id], |_| Ok(()))
        .optional()
        .map(|found| found.is_some())
}

/// A week's board: each player's quickest lap set between `since` and
/// `until`, fastest first. Weekly boards are small, so they are ranked here
/// rather than kept in a table of their own.
pub fn week(
    db: &Connection,
    circuit: &str,
    mode: &str,
    physics: u32,
    (since, until): (i64, i64),
) -> rusqlite::Result<Vec<Place>> {
    let mut statement = db.prepare(
        "SELECT p.id, p.name, p.country, w.steps, w.car, w.setup, w.multiplayer, w.id
         FROM (SELECT r.*, ROW_NUMBER() OVER (PARTITION BY r.player ORDER BY r.steps, r.created_at) AS nth
               FROM runs r
               WHERE r.circuit = ?1 AND r.mode = ?2 AND r.physics = ?3 AND r.hidden = 0
                 AND r.created_at >= ?4 AND r.created_at < ?5) w
         JOIN players p ON p.id = w.player
         WHERE w.nth = 1
         ORDER BY w.steps, w.created_at, w.player",
    )?;
    let rows = statement.query_map(params![circuit, mode, physics, since, until], |row| {
        row_owned(row)
    })?;
    let mut out = Vec::new();
    for (i, row) in rows.enumerate() {
        out.push(row?.into_place(i as u64 + 1));
    }
    Ok(out)
}

/// Keep a verified run, and make it the player's best if it is.
pub fn keep_run(
    db: &Connection,
    id: &str,
    player: &str,
    checked: &todora::verify::Checked,
    bytes: &[u8],
) -> rusqlite::Result<bool> {
    let at = now();
    let tx = db.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO runs (id, player, circuit, mode, physics, car, setup, multiplayer, steps, sectors, app_version, bytes, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            id,
            player,
            checked.circuit,
            checked.mode,
            checked.physics,
            checked.car,
            checked.setup,
            checked.multiplayer,
            checked.steps,
            serde_json::to_string(&checked.sectors).unwrap_or_default(),
            checked.app_version,
            bytes,
            at
        ],
    )?;
    let improved = tx.execute(
        "INSERT INTO best (player, circuit, mode, physics, run, steps, set_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT (player, circuit, mode, physics)
         DO UPDATE SET run = excluded.run, steps = excluded.steps, set_at = excluded.set_at
         WHERE excluded.steps < best.steps",
        params![
            player,
            checked.circuit,
            checked.mode,
            checked.physics,
            id,
            checked.steps,
            at
        ],
    )? > 0;
    tx.commit()?;
    Ok(improved)
}

pub fn reject(db: &Connection, player: &str, reason: &str) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO rejections (player, reason, at) VALUES (?1, ?2, ?3)",
        params![player, reason, now()],
    )?;
    // Rejections are for spotting trouble, not a record of anyone: a month.
    db.execute(
        "DELETE FROM rejections WHERE at < ?1",
        params![now() - 30 * 24 * 3600],
    )?;
    Ok(())
}

pub fn run_bytes(db: &Connection, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
    db.query_row(
        "SELECT bytes FROM runs WHERE id = ?1 AND hidden = 0",
        params![id],
        |row| row.get(0),
    )
    .optional()
}

#[cfg(test)]
pub fn memory() -> Connection {
    let db = Connection::open_in_memory().expect("an in-memory database");
    db.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    db.execute_batch(SCHEMA).unwrap();
    db
}
