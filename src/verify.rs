//! What the leaderboard server needs from the game, and nothing else.
//!
//! The server links this library so that a lap is judged by exactly the code
//! that timed it: the circuits as the game builds them, the same engine, the
//! same lap judge. Everything here is plain data in and out — no Bevy app, no
//! window, no sound — so it runs on a headless box.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::car::{Mode, Setup, Spec};
use crate::online::{replay, run::Run};
use crate::track::{Track, all_circuits};

/// The lap rules and engine a run must have been driven under.
pub const PHYSICS_VERSION: u32 = crate::car::PHYSICS_VERSION;
/// The version of the game this library is.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Largest run accepted.
pub const MAX_RUN_BYTES: usize = crate::online::run::MAX_BYTES;

/// A run that has been driven again and found to be the lap it claims.
#[derive(Clone, Debug, PartialEq)]
pub struct Checked {
    pub circuit: String,
    /// `beginner`, `regular` or `pro`.
    pub mode: String,
    pub car: String,
    pub setup: String,
    /// The lap time in physics steps; [`seconds`] turns it into seconds.
    pub steps: u32,
    pub sectors: Vec<f32>,
    pub app_version: String,
    pub physics: u32,
    pub fingerprint: u64,
    pub multiplayer: bool,
    /// A hash of the lap itself — where it started, the car and setup, and
    /// every input — and of nothing a copier could change for free, like the
    /// version string. Two submissions with the same one are the same lap.
    pub lap: u64,
}

/// Seconds for a lap of `steps` physics steps.
pub fn seconds(steps: u32) -> f64 {
    f64::from(steps) / crate::car::STEP_HZ
}

/// The circuits, as `(id, name)`, in menu order.
pub fn circuits() -> Vec<(&'static str, &'static str)> {
    all_circuits().iter().map(|c| (c.id, c.name)).collect()
}

/// The driving modes, as stored.
pub fn modes() -> [&'static str; 3] {
    Mode::ALL.map(mode_name)
}

fn mode_name(mode: Mode) -> &'static str {
    match mode {
        Mode::Beginner => "beginner",
        Mode::Regular => "regular",
        Mode::Pro => "pro",
    }
}

fn car_name(car: Spec) -> &'static str {
    match car {
        Spec::Tourer => "tourer",
        Spec::Clubman => "clubman",
        Spec::Express => "express",
    }
}

fn setup_name(setup: Setup) -> &'static str {
    match setup {
        Setup::Understeer => "understeer",
        Setup::Balanced => "balanced",
        Setup::Oversteer => "oversteer",
    }
}

/// Builds each circuit the first time a run for it arrives, and keeps it.
#[derive(Default)]
pub struct Verifier {
    tracks: Mutex<HashMap<&'static str, Arc<Track>>>,
}

impl Verifier {
    pub fn new() -> Self {
        Self::default()
    }

    fn track(&self, circuit: &str) -> Option<Arc<Track>> {
        let found = all_circuits().iter().find(|c| c.id == circuit)?;
        let mut tracks = self.tracks.lock().expect("the track cache");
        Some(
            tracks
                .entry(found.id)
                .or_insert_with(|| Arc::new(Track::new(found)))
                .clone(),
        )
    }

    /// The fingerprint of a circuit as this build lays it out.
    pub fn fingerprint(&self, circuit: &str) -> Option<u64> {
        Some(self.track(circuit)?.fingerprint())
    }

    /// Decode a run, hold it to this build's circuit and rules, and drive it
    /// again. Slow — a lap is replayed step by step — so a server calls it off
    /// its request threads.
    pub fn check(&self, bytes: &[u8]) -> Result<Checked, String> {
        let run = Run::decode(bytes).map_err(str::to_string)?;
        if run.physics != PHYSICS_VERSION {
            return Err("driven under other physics".into());
        }
        let track = self.track(&run.circuit).ok_or("no such circuit")?;
        if run.fingerprint != track.fingerprint() {
            return Err("driven on another layout".into());
        }
        replay::verify(&run, &track).map_err(str::to_string)?;
        Ok(Checked {
            circuit: run.circuit.clone(),
            mode: mode_name(run.mode).into(),
            car: car_name(run.car).into(),
            setup: setup_name(run.setup).into(),
            steps: run.steps,
            sectors: run.sectors.clone(),
            app_version: run.app_version.clone(),
            physics: run.physics,
            fingerprint: run.fingerprint,
            multiplayer: run.multiplayer,
            lap: lap_hash(&run),
        })
    }
}

fn lap_hash(run: &Run) -> u64 {
    let same = Run {
        app_version: String::new(),
        multiplayer: false,
        ..run.clone()
    };
    Run::hash(&same.encode())
}

/// A valid lap of `circuit` in `mode` by the plain AI driver, as run bytes:
/// for the server's tests and for filling a board to look at.
pub fn ai_run(circuit: &str, mode: &str) -> Option<Vec<u8>> {
    let found = all_circuits().iter().find(|c| c.id == circuit)?;
    let mode = Mode::ALL.into_iter().find(|m| mode_name(*m) == mode)?;
    let track = Track::new(found);
    crate::online::ai_laps(&track, mode, 2)
        .into_iter()
        .find(|(_, lap, _)| lap.valid)
        .map(|(mut run, _, _)| {
            run.app_version = APP_VERSION.into();
            run.encode()
        })
}

/// The ISO week `unix` seconds fall in, as (year, week).
pub fn iso_week(unix: i64) -> (i32, u32) {
    let days = unix.div_euclid(86_400);
    // 1970-01-01 was a Thursday; ISO weeks belong to the year of their Thursday.
    let weekday = (days + 3).rem_euclid(7); // Monday = 0
    let thursday = days - weekday + 3;
    let (year, ordinal) = year_and_day(thursday);
    (year, (ordinal / 7 + 1) as u32)
}

/// Unix seconds at the start (Monday 00:00 UTC) and end of an ISO week.
pub fn week_bounds(year: i32, week: u32) -> (i64, i64) {
    // The week holding 4 January is week 1.
    let jan4 = days_from_civil(year, 1, 4);
    let monday = jan4 - (jan4 + 3).rem_euclid(7) + 7 * (i64::from(week) - 1);
    (monday * 86_400, (monday + 7) * 86_400)
}

/// Which circuit a week's challenge is on, as an index into [`circuits`]:
/// a fixed shuffle of all of them, walked one a week, so no circuit comes back
/// within as many weeks as there are circuits and every copy of the game agrees.
///
/// The shuffle runs over the circuits in the order of their ids, not the
/// menu's, so renaming a circuit (which re-sorts the menu) never changes a
/// week's challenge, and an older server still agrees with a newer game.
pub fn challenge_circuit(year: i32, week: u32) -> usize {
    let n = all_circuits().len();
    let mut by_id: Vec<usize> = (0..n).collect();
    by_id.sort_by_key(|&at| all_circuits()[at].id);
    let mut order: Vec<usize> = (0..n).collect();
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    for i in (1..n).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        order.swap(i, (seed % (i as u64 + 1)) as usize);
    }
    let (start, _) = week_bounds(year, week);
    let weeks = start.div_euclid(7 * 86_400);
    by_id[order[weeks.rem_euclid(n as i64) as usize]]
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn year_and_day(days: i64) -> (i32, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    if month <= 2 {
        year += 1;
    }
    let jan1 = days_from_civil(year as i32, 1, 1);
    (year as i32, days - jan1)
}

/// Whether `name` may appear on a board: 3 to 16 characters of letters,
/// digits, spaces, `-`, `_` and `.`, not starting or ending with a space, and
/// nothing from the blocklist. Returns it trimmed.
pub fn valid_name(name: &str) -> Result<String, &'static str> {
    let name = name.trim();
    let count = name.chars().count();
    if !(3..=16).contains(&count) {
        return Err("3 to 16 characters");
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
    {
        return Err("letters, digits, spaces, - _ and . only");
    }
    let folded: String = name
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .map(|c| match c {
            '0' => 'o',
            '1' => 'i',
            '3' => 'e',
            '4' => 'a',
            '5' => 's',
            '7' => 't',
            other => other,
        })
        .collect();
    if BLOCKED.iter().any(|word| folded.contains(word)) {
        return Err("not that name");
    }
    Ok(name.to_string())
}

/// Kept short and obvious; reports catch the rest.
const BLOCKED: &[&str] = &[
    "fuck",
    "shit",
    "cunt",
    "nigg",
    "fag",
    "rape",
    "nazi",
    "hitler",
    "whore",
    "slut",
    "bitch",
    "admin",
    "moderator",
    "todora",
];

/// A two-letter country code, upper case, or `None` for anything else.
pub fn valid_country(code: &str) -> Option<String> {
    let code = code.trim().to_uppercase();
    (code.len() == 2 && code.chars().all(|c| c.is_ascii_uppercase())).then_some(code)
}

/// The version of a lap-rules and engine combination, and whether a build
/// of `app` may submit: the same major and minor version as this one.
pub fn compatible_app(app: &str) -> bool {
    let minor = |v: &str| v.split('.').take(2).collect::<Vec<_>>().join(".");
    minor(app) == minor(APP_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ai_lap_passes_and_a_tampered_one_does_not() {
        let verifier = Verifier::new();
        let bytes = ai_run("monza", "regular").expect("an AI lap");
        let checked = verifier.check(&bytes).expect("the AI lap is a lap");
        assert_eq!(checked.circuit, "monza");
        assert_eq!(checked.mode, "regular");
        assert!(seconds(checked.steps) > 20.0);
        // Hold full throttle for the first ten seconds, whatever was driven:
        // the lap that makes is not the lap that was claimed.
        let mut run = Run::decode(&bytes).unwrap();
        for controls in &mut run.inputs[..2400] {
            controls.throttle = 1.0;
            controls.brake = 0.0;
        }
        assert!(verifier.check(&run.encode()).is_err());
        let mut flipped = bytes.clone();
        flipped[20] ^= 0x40;
        assert!(verifier.check(&flipped).is_err(), "a changed header byte");
    }

    /// A lap recorded on the Mac that blessed it, checked wherever the tests
    /// run: what a Linux server does with a lap driven on a Mac.
    #[test]
    fn a_lap_recorded_on_another_machine_still_checks() {
        let bytes = include_bytes!("online/monza-mac.run");
        let checked = Verifier::new()
            .check(bytes)
            .expect("a Mac lap checks here too; see determinism.txt if the layout moved");
        assert_eq!(checked.circuit, "monza");
    }

    /// Writes the fixture above. Run on the Mac after a deliberate change:
    /// `TODORA_BLESS=1 cargo test --locked --lib write_the_mac_lap -- --ignored`.
    #[test]
    #[ignore = "writes src/online/monza-mac.run"]
    fn write_the_mac_lap() {
        if std::env::var_os("TODORA_BLESS").is_some() {
            let bytes = ai_run("monza", "regular").expect("an AI lap");
            let path =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/online/monza-mac.run");
            std::fs::write(path, bytes).expect("the fixture is writable");
        }
    }

    #[test]
    fn names_are_held_to_the_rules() {
        assert_eq!(valid_name("  Amber Falcon "), Ok("Amber Falcon".into()));
        assert!(valid_name("ab").is_err());
        assert!(valid_name("seventeen letters").is_err());
        assert!(valid_name("<script>").is_err());
        assert!(valid_name("sh1thead").is_err());
        assert!(valid_name("Todora_Admin").is_err());
        assert_eq!(valid_name("Jürgen-95"), Ok("Jürgen-95".into()));
        assert_eq!(valid_country(" at "), Some("AT".into()));
        assert_eq!(valid_country("AUT"), None);
    }

    #[test]
    fn weeks_are_iso_weeks_and_every_copy_picks_the_same_circuit() {
        // Thursday 24 September 2026 is in week 39; 1 January 2021 in 2020's week 53.
        assert_eq!(iso_week(1_790_208_000), (2026, 39));
        assert_eq!(iso_week(1_609_459_200), (2020, 53));
        let (start, end) = week_bounds(2026, 39);
        assert_eq!(iso_week(start), (2026, 39));
        assert_eq!(iso_week(end - 1), (2026, 39));
        assert_eq!(iso_week(end), (2026, 40));
        assert_eq!(end - start, 7 * 86_400);
        // Forty consecutive weeks visit forty different circuits.
        let mut seen = std::collections::HashSet::new();
        let mut at = start;
        for _ in 0..circuits().len() {
            let (y, w) = iso_week(at);
            assert!(
                seen.insert(challenge_circuit(y, w)),
                "a circuit came back early"
            );
            at += 7 * 86_400;
        }
        assert_eq!(challenge_circuit(2026, 39), challenge_circuit(2026, 39));
        // What the game and the server have always picked, whatever the menu
        // calls these circuits or however it sorts them.
        let id = |week| all_circuits()[challenge_circuit(2026, week)].id;
        assert_eq!(
            [id(39), id(40), id(41), id(42)],
            ["paul-ricard", "bahrain", "marina-bay", "imola"]
        );
    }

    #[test]
    fn only_the_same_minor_version_may_submit() {
        assert!(compatible_app(APP_VERSION));
        assert!(!compatible_app("0.1.0"));
    }
}
