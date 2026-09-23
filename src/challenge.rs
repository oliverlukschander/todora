//! The weekly challenge: one circuit a week, the same for everyone.
//!
//! Each ISO week, from Monday 00:00 UTC, picks a circuit from a fixed shuffle
//! of all forty (see [`crate::verify::challenge_circuit`]), so every copy of
//! the game agrees without asking anyone and no circuit comes back within forty
//! weeks. Any car and setup, in Regular mode. Your best lap there this week is
//! kept in the settings; online, a lap that beats it is uploaded even when it is
//! not your best ever there, and the week's board ranks the laps set in it.

use bevy::prelude::*;

use crate::car::Mode;
use crate::lap::{LapFinished, LapSet};
use crate::settings::Settings;
use crate::track::{Circuit, Track, all_circuits};

/// This week's challenge.
#[derive(Resource, Clone, Debug, PartialEq)]
pub(crate) struct Challenge {
    pub year: i32,
    pub week: u32,
    /// Index into [`all_circuits`].
    pub at: usize,
    /// Unix seconds when the week ends.
    pub ends_at: i64,
    /// A lap that finished this step beat this week's best.
    pub improved: bool,
}

impl Challenge {
    pub(crate) fn at(unix: i64) -> Self {
        let (year, week) = crate::verify::iso_week(unix);
        Self {
            year,
            week,
            at: crate::verify::challenge_circuit(year, week),
            ends_at: crate::verify::week_bounds(year, week).1,
            improved: false,
        }
    }

    pub(crate) fn circuit(&self) -> &'static Circuit {
        &all_circuits()[self.at]
    }

    /// `2026-W39`, as the server and the settings write it.
    pub(crate) fn label(&self) -> String {
        format!("{}-W{:02}", self.year, self.week)
    }

    /// How long is left, as a player reads it: `4 d 07 h`, `5 h 20 min`.
    pub(crate) fn left(&self, unix: i64) -> String {
        let left = (self.ends_at - unix).max(0);
        let (days, hours, minutes) = (left / 86_400, left % 86_400 / 3600, left % 3600 / 60);
        if days > 0 {
            format!("{days} d {hours:02} h")
        } else {
            format!("{hours} h {minutes:02} min")
        }
    }

    /// Whether a lap of `time` on `circuit` in `mode` counts for it at all.
    pub(crate) fn counts(&self, circuit: &str, mode: Mode) -> bool {
        circuit == self.circuit().id && mode == Mode::Regular
    }
}

impl Default for Challenge {
    fn default() -> Self {
        Self::at(crate::online::client::unix_now())
    }
}

/// Where the challenge judges a lap; anything that wants to know whether the
/// lap just finished improved this week runs after it.
#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct ChallengeSet;

pub struct ChallengePlugin;

impl Plugin for ChallengePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Challenge>()
            .add_systems(FixedUpdate, hear.in_set(ChallengeSet).after(LapSet))
            .add_systems(Update, roll_over);
    }
}

/// A valid lap on this week's circuit in Regular mode, quicker than this
/// week's best, is the new best.
fn hear(
    mut laps: MessageReader<LapFinished>,
    track: Res<Track>,
    mode: Res<Mode>,
    mut challenge: ResMut<Challenge>,
    mut settings: Option<ResMut<Settings>>,
) {
    challenge.improved = false;
    for lap in laps.read() {
        if !lap.valid || !challenge.counts(track.circuit().id, *mode) {
            continue;
        }
        let label = challenge.label();
        let Some(settings) = settings.as_mut() else {
            continue;
        };
        let best = (settings.challenge_week == label)
            .then_some(settings.challenge_best)
            .flatten();
        if best.is_none_or(|best| lap.time < best) {
            settings.challenge_week = label;
            settings.challenge_best = Some(lap.time);
            challenge.improved = true;
        }
    }
}

/// A new week, a new circuit, checked once a minute.
fn roll_over(time: Res<Time<Real>>, mut next: Local<f64>, mut challenge: ResMut<Challenge>) {
    let now = time.elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + 60.0;
    let fresh = Challenge::at(crate::online::client::unix_now());
    if fresh.label() != challenge.label() {
        *challenge = fresh;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_week_names_its_circuit_its_label_and_the_time_left() {
        let thursday = 1_790_208_000; // 2026-09-24 00:00 UTC
        let challenge = Challenge::at(thursday);
        assert_eq!(challenge.label(), "2026-W39");
        assert_eq!(challenge.left(thursday), "4 d 00 h");
        assert_eq!(
            challenge.left(challenge.ends_at - 3 * 3600 - 120),
            "3 h 02 min"
        );
        assert!(challenge.counts(challenge.circuit().id, Mode::Regular));
        assert!(!challenge.counts(challenge.circuit().id, Mode::Pro));
        assert_eq!(
            Challenge::at(thursday + 86_400),
            challenge,
            "the same all week"
        );
    }

    #[test]
    fn only_a_quicker_valid_lap_on_the_circuit_improves_the_week() {
        let challenge = Challenge::default();
        let track = Track::new(challenge.circuit());
        let mut app = App::new();
        app.add_message::<LapFinished>()
            .insert_resource(track)
            .insert_resource(Mode::Regular)
            .insert_resource(challenge)
            .init_resource::<Settings>()
            .add_systems(Update, hear);
        let lap = |time, valid| LapFinished {
            time,
            best: false,
            valid,
        };
        for (time, valid, improves) in [
            (70.0, true, true),
            (72.0, true, false),
            (65.0, false, false),
            (69.0, true, true),
        ] {
            app.world_mut().write_message(lap(time, valid));
            app.update();
            assert_eq!(
                app.world().resource::<Challenge>().improved,
                improves,
                "{time} {valid}"
            );
        }
        assert_eq!(
            app.world().resource::<Settings>().challenge_best,
            Some(69.0)
        );
    }
}
