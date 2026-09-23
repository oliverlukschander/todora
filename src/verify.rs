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
    fn only_the_same_minor_version_may_submit() {
        assert!(compatible_app(APP_VERSION));
        assert!(!compatible_app("0.1.0"));
    }
}
