//! Bronze, silver, gold and author: a target on every circuit and mode.
//!
//! The author time is a lap set by whoever made the game, and the other three
//! are fixed steps above it: gold 2% slower, silver 6%, bronze 12%. A medal is
//! read off the best lap, so nothing new is saved for it.
//!
//! The author times live in `author.rs`, written by
//! `TODORA_AUTHOR=1 cargo test --locked --lib write_the_author_times -- --ignored`.
//! That takes the best lap saved on the machine it runs on for each circuit and
//! mode, and where there is none the plain AI driver's lap scaled by
//! [`PROVISIONAL`], marked provisional. Each entry carries the fingerprint of the circuit it was
//! set on, so a layout change makes it stale rather than wrong, and a test says
//! which ones.

mod author;

use crate::car::Mode;
use crate::track::Track;
use author::AUTHOR;

/// A stand-in author time is the plain AI's lap times this. The AI is not
/// quick: on the one circuit with both, the Red Bull Ring, a lap driven by hand
/// took 33.62 s against the AI's 44.11, or 0.76 of it. A stand-in a touch slower
/// than that keeps gold something to work for, where "the AI less 3%" would
/// hand it over on a first lap.
#[cfg(test)]
pub(crate) const PROVISIONAL: f32 = 0.78;

/// Gold, silver and bronze, as multiples of the author time.
pub(crate) const STEPS: [(Medal, f32); 4] = [
    (Medal::Author, 1.0),
    (Medal::Gold, 1.02),
    (Medal::Silver, 1.06),
    (Medal::Bronze, 1.12),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Medal {
    Bronze,
    Silver,
    Gold,
    Author,
}

impl Medal {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Bronze => "bronze",
            Self::Silver => "silver",
            Self::Gold => "gold",
            Self::Author => "author",
        }
    }

    /// The badge colour. The name is always written beside it, so a medal
    /// reads without telling the colours apart.
    pub(crate) fn colour(self) -> bevy::color::Color {
        use bevy::color::Color;
        match self {
            Self::Bronze => Color::srgb(0.80, 0.52, 0.30),
            Self::Silver => Color::srgb(0.78, 0.81, 0.84),
            Self::Gold => Color::srgb(0.98, 0.80, 0.25),
            Self::Author => Color::srgb(0.46, 0.86, 0.94),
        }
    }
}

/// One circuit's author times.
pub(super) struct Entry {
    pub circuit: &'static str,
    pub fingerprint: u64,
    /// Beginner, Regular and Pro, in seconds.
    pub times: [f32; 3],
    /// Whether each is the AI's stand-in rather than a lap somebody drove.
    pub provisional: [bool; 3],
}

fn mode_index(mode: Mode) -> usize {
    Mode::ALL.iter().position(|m| *m == mode).unwrap_or(1)
}

/// The targets for a circuit by name. The menu uses these without building
/// every circuit, which is safe because a test holds every entry to the
/// circuit's current fingerprint.
pub(crate) fn targets_for(circuit: &str, mode: Mode) -> Option<(Targets, u64)> {
    let entry = AUTHOR.iter().find(|e| e.circuit == circuit)?;
    Some((
        Targets {
            author: entry.times[mode_index(mode)],
            provisional: entry.provisional[mode_index(mode)],
        },
        entry.fingerprint,
    ))
}

/// The four targets for a circuit and mode, fastest first, if there is an
/// author time for the circuit as it stands.
pub(crate) fn targets(track: &Track, mode: Mode) -> Option<Targets> {
    let circuit = track.circuit().id;
    let entry = AUTHOR
        .iter()
        .find(|e| e.circuit == circuit && e.fingerprint == track.fingerprint())?;
    Some(Targets {
        author: entry.times[mode_index(mode)],
        provisional: entry.provisional[mode_index(mode)],
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Targets {
    pub author: f32,
    pub provisional: bool,
}

impl Targets {
    pub(crate) fn time(&self, medal: Medal) -> f32 {
        let step = STEPS
            .iter()
            .find(|(m, _)| *m == medal)
            .map_or(1.0, |(_, s)| *s);
        self.author * step
    }

    /// The best medal a lap of `time` has earned.
    pub(crate) fn medal(&self, time: f32) -> Option<Medal> {
        STEPS
            .iter()
            .find(|(medal, _)| time <= self.time(*medal))
            .map(|(medal, _)| *medal)
    }

    /// The next medal up and how much quicker it asks for, or `None` once the
    /// author time is beaten.
    pub(crate) fn next(&self, time: Option<f32>) -> Option<(Medal, f32)> {
        let have = time.and_then(|time| self.medal(time));
        let want = match have {
            None => Medal::Bronze,
            Some(Medal::Bronze) => Medal::Silver,
            Some(Medal::Silver) => Medal::Gold,
            Some(Medal::Gold) => Medal::Author,
            Some(Medal::Author) => return None,
        };
        Some((want, time.map_or(self.time(want), |t| t - self.time(want))))
    }
}

/// A short line saying where a lap stands: the medal it earned, and what the
/// next one asks for.
pub(crate) fn standing(targets: &Targets, best: Option<f32>) -> String {
    let have = best.and_then(|t| targets.medal(t));
    let next = targets.next(best);
    match (have, next) {
        (Some(medal), None) => format!("{} — author time beaten", medal.name().to_uppercase()),
        (Some(medal), Some((want, gap))) => format!(
            "{} — {:.2} to {}",
            medal.name().to_uppercase(),
            gap,
            want.name()
        ),
        (None, Some((want, gap))) if best.is_some() => {
            format!("{:.2} to {}", gap, want.name())
        }
        (None, Some((want, time))) => {
            format!("{} in {}", want.name(), crate::lap::format_time(time))
        }
        (None, None) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::all_circuits;

    fn at(author: f32) -> Targets {
        Targets {
            author,
            provisional: false,
        }
    }

    #[test]
    fn medal_thresholds_are_ordered_and_read_off_the_time() {
        let t = at(60.0);
        assert!((t.time(Medal::Gold) - 61.2).abs() < 1e-4);
        assert!((t.time(Medal::Silver) - 63.6).abs() < 1e-4);
        assert!((t.time(Medal::Bronze) - 67.2).abs() < 1e-4);
        assert_eq!(t.medal(59.0), Some(Medal::Author));
        assert_eq!(t.medal(61.0), Some(Medal::Gold));
        assert_eq!(t.medal(63.0), Some(Medal::Silver));
        assert_eq!(t.medal(67.0), Some(Medal::Bronze));
        assert_eq!(t.medal(70.0), None);
        assert!(Medal::Author > Medal::Gold && Medal::Gold > Medal::Bronze);
    }

    #[test]
    fn the_standing_says_what_the_next_medal_asks_for() {
        let t = at(60.0);
        assert_eq!(standing(&t, None), "bronze in 1:07.20");
        assert_eq!(standing(&t, Some(70.0)), "2.80 to bronze");
        assert_eq!(standing(&t, Some(61.0)), "GOLD — 1.00 to author");
        assert_eq!(standing(&t, Some(59.5)), "AUTHOR — author time beaten");
    }

    #[test]
    fn every_circuit_and_mode_has_an_author_time_for_the_layout_it_has_now() {
        for circuit in all_circuits() {
            let track = Track::new(circuit);
            for mode in Mode::ALL {
                let targets = targets(&track, mode).unwrap_or_else(|| {
                    panic!(
                        "{} {:?} has no author time for its current layout; \
                         run TODORA_AUTHOR=1 cargo test --locked --lib write_the_author_times -- --ignored",
                        circuit.id, mode
                    )
                });
                assert!(targets.author.is_finite() && targets.author > 10.0);
            }
        }
    }

    /// Between the AI's stand-ins, at least; a lap somebody drove is whatever
    /// it was.
    #[test]
    fn a_quicker_mode_asks_for_a_quicker_time() {
        for entry in AUTHOR.iter().filter(|e| e.provisional == [true; 3]) {
            assert!(
                entry.times[0] > entry.times[1] && entry.times[1] > entry.times[2],
                "{}: {:?}",
                entry.circuit,
                entry.times
            );
        }
    }

    /// Writes `author.rs`. Your own best lap wherever one is saved on this
    /// machine; the AI's lap scaled by [`PROVISIONAL`], marked provisional,
    /// elsewhere.
    #[test]
    #[ignore = "writes src/medals/author.rs; run with TODORA_AUTHOR=1"]
    fn write_the_author_times() {
        if std::env::var_os("TODORA_AUTHOR").is_none() {
            return;
        }
        let mut out = String::from(
            "//! Written by `write_the_author_times`; see the module above. Times in\n\
             //! seconds for Beginner, Regular and Pro.\n\n\
             use super::Entry;\n\n\
             pub(super) const AUTHOR: &[Entry] = &[\n",
        );
        for circuit in all_circuits() {
            let track = Track::new(circuit);
            let mut times = [0.0f32; 3];
            let mut provisional = [false; 3];
            for (i, mode) in Mode::ALL.into_iter().enumerate() {
                (times[i], provisional[i]) = match crate::ghost::saved_time(&track, mode) {
                    Some(time) => (time, false),
                    None => {
                        let ai = crate::car::ai_lap_time(&track, mode)
                            .unwrap_or_else(|| panic!("the AI did not get round {}", circuit.id));
                        (ai * PROVISIONAL, true)
                    }
                };
            }
            out += &format!(
                "    Entry {{\n        circuit: {:?},\n        fingerprint: 0x{:016x},\n        \
                 times: [{:.2}, {:.2}, {:.2}],\n        provisional: {:?},\n    }},\n",
                circuit.id,
                track.fingerprint(),
                times[0],
                times[1],
                times[2],
                provisional
            );
        }
        out += "];\n";
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/medals/author.rs");
        std::fs::write(path, out).expect("author.rs is writable");
    }
}
