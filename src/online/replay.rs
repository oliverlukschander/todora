//! Drive a run again and judge it: what the server does with every upload,
//! and what the game does with every ghost it downloads.
//!
//! Nothing here trusts the run beyond its inputs. The car starts from the
//! recorded entry state, which [`plausible`] has to accept first; the same
//! engine and the same lap judge the game uses then drive it step by step, and
//! the verdict is whatever they say: how many steps the lap took, whether it
//! stayed valid, and its sectors. A run whose claim disagrees is not a lap.

use super::run::{Entry, Run};
use crate::car::{Car, Handling, advance, step_seconds};
use crate::lap::{LapTimer, Step};
use crate::track::Track;
use bevy::prelude::*;

/// What driving a run again found.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Verdict {
    /// Steps from the line to the line, if the lap finished within the run.
    pub steps: Option<u32>,
    pub valid: bool,
    pub sectors: Vec<f32>,
}

/// Whether a claimed entry could be a car crossing this circuit's line: on the
/// road at the line, pointing along it, no faster than the car can go in this
/// mode, and not already sliding or spinning. What cannot be checked is how
/// it got there; a crafted entry can gain a few tenths at most, which is what
/// was accepted when laps were chosen over whole stints.
pub(crate) fn plausible(
    entry: &Entry,
    track: &Track,
    handling: &Handling,
) -> Result<(), &'static str> {
    let at = entry.translation;
    if track.start_along(at).abs() > 1.0 || !track.on_start_gate(at, entry.along) {
        return Err("not at the line");
    }
    let tangent = track.ground_from(at, entry.along).tangent;
    let heading = crate::car::level(entry.rotation * Vec3::NEG_Z);
    if heading.dot(tangent) < 0.7 {
        return Err("not facing along the road");
    }
    // A long descent can carry the car a little past the engine's reach.
    if entry.velocity.length() > handling.top_speed * 1.25 {
        return Err("faster than the car goes");
    }
    if entry.yaw_rate.abs() > 3.0 || entry.slip_angle.abs() > 0.8 || entry.reversing {
        return Err("not driving along the road");
    }
    Ok(())
}

/// Drive `run` on `track` and judge the lap it makes.
pub(crate) fn replay(run: &Run, track: &Track) -> Verdict {
    replay_with(run, track, |_, _, _| {})
}

/// The same, telling `each_step` where the car is after every step — the
/// clock, how far round the lap, and its pose. How a downloaded lap becomes a
/// ghost to race.
pub(crate) fn replay_with(
    run: &Run,
    track: &Track,
    mut each_step: impl FnMut(f32, f32, &Transform),
) -> Verdict {
    let handling = run
        .mode
        .applied_to(run.setup.applied_to(run.car.handling()));
    let (mut at, mut car): (Transform, Car) = run.entry.car();
    let dt = step_seconds();
    let mut timer = LapTimer::armed(
        track.start_along(at.translation),
        track.progress(at.translation, car.along),
    );
    for (i, controls) in run.inputs.iter().enumerate() {
        advance(track, &handling, *controls, &mut at, &mut car, dt);
        timer.count(dt);
        let recovered = std::mem::take(&mut car.recovered);
        let pos = at.translation;
        let step = Step {
            legal: track.legal_contact(&at, car.along),
            recovered,
            along: track.start_along(pos),
            progress: track.progress(pos, car.along),
            length: track.length(),
            sectors: track.sector_count(),
            speed: car.velocity.length(),
        };
        let lap = timer.judge(step, || track.on_start_gate(pos, car.along));
        // The finishing step is the end of the lap, not the start of the next.
        match &lap {
            Some(lap) => each_step(lap.time, 1.0, &at),
            None => each_step(timer.current, timer.progress(), &at),
        }
        if let Some(lap) = lap {
            let report = timer.report.take();
            return Verdict {
                steps: Some(i as u32 + 1),
                valid: lap.valid,
                sectors: report.map(|r| r.sectors).unwrap_or_default(),
            };
        }
    }
    Verdict {
        steps: None,
        valid: !timer.invalid,
        sectors: Vec::new(),
    }
}

/// The whole check: the entry is possible, the lap finished when it said it
/// did, it counted, and its sectors are the ones claimed.
pub(crate) fn verify(run: &Run, track: &Track) -> Result<Verdict, &'static str> {
    let handling = run
        .mode
        .applied_to(run.setup.applied_to(run.car.handling()));
    plausible(&run.entry, track, &handling)?;
    let verdict = replay(run, track);
    match verdict.steps {
        None => Err("the lap never finished"),
        Some(steps) if steps != run.steps => Err("the lap took a different time"),
        _ if !verdict.valid => Err("the lap did not count"),
        _ if verdict.sectors != run.sectors => Err("the sectors differ"),
        _ => Ok(verdict),
    }
}
