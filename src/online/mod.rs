//! Laps that can go online: recorded as runs, kept in an outbox until they are
//! sent.
//!
//! Every lap is recorded as it is driven — the car's state as it crossed the
//! line, then the quantised pedals and wheel of every physics step — and a lap
//! that was valid and a new best is written to `outbox/` as a [`run::Run`].
//! It waits there, whether or not the player has gone online, and is sent when
//! they have; see `client`. Recording is a push of four numbers per physics
//! step into a list reserved once per lap.

mod board;
#[cfg(feature = "visual-check")]
pub(crate) use board::Browse;
pub(crate) mod client;
pub(crate) mod replay;
pub(crate) mod run;
mod ui;

pub(crate) use ui::Online;

use bevy::prelude::*;

use crate::car::{Car, Controls, Mode, Player, Setup, Spec};
use crate::lap::{LapFinished, LapSet, LapTimer};
use crate::track::Track;
use run::{Entry, LONGEST, Run};

pub struct OnlinePlugin;

impl Plugin for OnlinePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Recorder>()
            .add_systems(Startup, start_client)
            .add_systems(FixedUpdate, record.after(LapSet));
        ui::plugin(app);
        board::plugin(app);
    }
}

/// Start the worker that talks to the server, unless this is a visual check,
/// which must not touch the network or the player's outbox.
fn start_client(mut commands: Commands, read_only: Option<Res<crate::settings::ReadOnly>>) {
    if read_only.is_some() {
        return;
    }
    let server = std::env::var("TODORA_BOARD_URL").unwrap_or_else(|_| client::SERVER.into());
    commands.insert_resource(client::Client::start(server, crate::settings::data_dir()));
}

/// Adjectives and birds, for a name that is nobody's until someone picks it.
pub(crate) const NAMES: [&str; 16] = [
    "Amber Falcon",
    "Swift Heron",
    "Quiet Kestrel",
    "Late Magpie",
    "Blue Osprey",
    "Red Merlin",
    "Grey Wren",
    "Brave Swallow",
    "Calm Harrier",
    "Keen Plover",
    "Bold Curlew",
    "Warm Lapwing",
    "Neat Sparrow",
    "Wild Dunlin",
    "Deep Tern",
    "Bright Avocet",
];

/// A name to suggest: one of [`NAMES`] and two digits.
pub(crate) fn suggest_name(at: usize) -> String {
    format!("{} {:02}", NAMES[at % NAMES.len()], random_index() % 100)
}

pub(crate) fn random_index() -> usize {
    let mut bytes = [0u8; 8];
    let _ = getrandom::fill(&mut bytes);
    usize::from_le_bytes(bytes)
}

/// The lap being recorded.
#[derive(Resource, Default)]
pub(crate) struct Recorder {
    entry: Option<Entry>,
    inputs: Vec<Controls>,
    was_running: bool,
}

/// A lap the recorder saw finish.
pub(crate) struct Finished {
    pub entry: Entry,
    pub inputs: Vec<Controls>,
}

impl Recorder {
    /// One physics step. `controls` are what the engine was given this step,
    /// `running` whether the clock is on after it, `finished` the lap this step
    /// ended, and `now` the car as it is at the end of the step — which is the
    /// entry state of whatever lap starts here.
    pub(crate) fn step(
        &mut self,
        controls: Controls,
        running: bool,
        finished: bool,
        now: Entry,
    ) -> Option<Finished> {
        if !running {
            self.entry = None;
            self.inputs.clear();
            self.was_running = false;
            return None;
        }
        let mut done = None;
        if self.was_running && self.entry.is_some() {
            if self.inputs.len() < LONGEST as usize {
                self.inputs.push(controls.quantised());
            } else {
                // Nobody is uploading this lap; stop recording it.
                self.entry = None;
                self.inputs = Vec::new();
            }
        }
        if finished && let Some(entry) = self.entry.take() {
            done = Some(Finished {
                entry,
                inputs: std::mem::take(&mut self.inputs),
            });
        }
        if !self.was_running || finished {
            // A lap starts here: at the end of the run-up, or at the line that
            // finished the last one.
            self.entry = Some(now);
            self.inputs.clear();
            self.inputs.reserve(240 * 120);
        }
        self.was_running = true;
        done
    }
}

#[allow(clippy::too_many_arguments)]
fn record(
    mut laps: MessageReader<LapFinished>,
    timer: Res<LapTimer>,
    track: Res<Track>,
    mode: Res<Mode>,
    spec: Res<Spec>,
    setup: Res<Setup>,
    session: Option<Res<crate::multiplayer::Session>>,
    read_only: Option<Res<crate::settings::ReadOnly>>,
    client: Option<Res<client::Client>>,
    players: Query<(&Transform, &Car, &Controls), With<Player>>,
    mut recorder: ResMut<Recorder>,
) {
    let lap = laps.read().last().copied();
    let Ok((at, car, controls)) = players.single() else {
        return;
    };
    let finished = recorder.step(
        *controls,
        timer.running(),
        lap.is_some(),
        Entry::of(at, car),
    );
    let (Some(finished), Some(lap)) = (finished, lap) else {
        return;
    };
    // An assisted lap counts at home and never on the world boards.
    let assisted = timer.report.as_ref().is_some_and(|r| r.assisted);
    if !(lap.valid && lap.best) || assisted || read_only.is_some() {
        return;
    }
    let run = Run {
        app_version: env!("CARGO_PKG_VERSION").into(),
        physics: crate::car::PHYSICS_VERSION,
        circuit: track.circuit().id.into(),
        fingerprint: track.fingerprint(),
        mode: *mode,
        car: *spec,
        setup: *setup,
        multiplayer: session.is_some_and(|s| s.driving()),
        steps: finished.inputs.len() as u32,
        sectors: timer
            .report
            .as_ref()
            .map(|r| r.sectors.clone())
            .unwrap_or_default(),
        entry: finished.entry,
        inputs: finished.inputs,
    };
    match client {
        Some(client) => {
            let bytes = run.encode();
            client.ask(client::Ask::Keep {
                name: outbox::name(&run, &bytes),
                bytes,
            });
        }
        None => outbox::keep(run),
    }
}

/// The plain AI drives from the grid through the run-up and `count` laps,
/// timed by the judge and recorded by the recorder exactly as in the game.
/// Every lap it finishes must replay from its run to the same step count,
/// validity and sectors.
pub(crate) fn ai_laps(
    track: &Track,
    mode: Mode,
    count: usize,
) -> Vec<(Run, crate::lap::LapFinished, Vec<f32>)> {
    use crate::car::{SCALE, step_seconds};
    use crate::lap::Step;
    let handling = mode.applied_to(Setup::Balanced.applied_to(Spec::Tourer.handling()));
    let mut driver = crate::car::ai_driver();
    let mut at = track.start_transform().with_scale(Vec3::splat(SCALE));
    let mut car = Car {
        along: Some(track.start_along_lap()),
        ..Car::default()
    };
    let dt = step_seconds();
    let mut timer = LapTimer::default();
    let mut recorder = Recorder::default();
    let mut laps = Vec::new();
    let budget = (3.5 * track.length() / 5.9 * 240.0) as usize;
    for _ in 0..budget {
        let controls = driver(track, &handling, &at, &car);
        crate::car::advance(track, &handling, controls, &mut at, &mut car, dt);
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
        if let Some(done) = recorder.step(
            controls,
            timer.running(),
            lap.is_some(),
            Entry::of(&at, &car),
        ) {
            let lap = lap.unwrap();
            let sectors = timer.report.as_ref().unwrap().sectors.clone();
            let run = Run {
                app_version: "test".into(),
                physics: crate::car::PHYSICS_VERSION,
                circuit: track.circuit().id.into(),
                fingerprint: track.fingerprint(),
                mode,
                car: Spec::Tourer,
                setup: Setup::Balanced,
                multiplayer: false,
                steps: done.inputs.len() as u32,
                sectors: sectors.clone(),
                entry: done.entry,
                inputs: done.inputs,
            };
            laps.push((run, lap, sectors));
            if laps.len() == count {
                break;
            }
        }
    }
    laps
}

pub(crate) mod outbox {
    //! Runs waiting to be sent: one file each, named for what they are and
    //! the hash of their bytes, written off the main thread.

    use super::run::Run;
    use std::path::PathBuf;

    pub(crate) fn folder() -> Option<PathBuf> {
        Some(crate::settings::data_dir()?.join("outbox"))
    }

    pub(crate) fn name(run: &Run, bytes: &[u8]) -> String {
        format!(
            "{}-{}-{:016x}.run",
            run.circuit,
            run.mode.name().to_lowercase(),
            Run::hash(bytes)
        )
    }

    /// Keep `run` until it can be sent.
    pub(crate) fn keep(run: Run) {
        let Some(folder) = folder() else {
            return;
        };
        std::thread::spawn(move || {
            let bytes = run.encode();
            let path = folder.join(name(&run, &bytes));
            let beside = path.with_extension("writing");
            let written = std::fs::create_dir_all(&folder)
                .and_then(|()| std::fs::write(&beside, &bytes))
                .and_then(|()| std::fs::rename(&beside, &path));
            if let Err(trouble) = written {
                bevy::log::warn!(
                    "cannot keep the lap for upload at {}: {trouble}",
                    path.display()
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::all_circuits;

    #[test]
    fn a_recorded_lap_replays_to_the_same_lap() {
        for id in ["monza", "suzuka", "monaco"] {
            let track = Track::new(all_circuits().iter().find(|c| c.id == id).unwrap());
            let laps = ai_laps(&track, Mode::Regular, 2);
            assert_eq!(laps.len(), 2, "{id}: the AI did not finish two laps");
            for (run, lap, sectors) in &laps {
                let bytes = run.encode();
                let back = Run::decode(&bytes).unwrap();
                let verdict = replay::replay(&back, &track);
                assert_eq!(verdict.steps, Some(run.steps), "{id}");
                assert_eq!(verdict.valid, lap.valid, "{id}");
                assert_eq!(&verdict.sectors, sectors, "{id}");
                assert!(
                    (run.seconds() - lap.time).abs() < 1e-3,
                    "{id}: {} vs {}",
                    run.seconds(),
                    lap.time
                );
                if lap.valid {
                    assert_eq!(
                        replay::verify(&back, &track).map(|v| v.steps),
                        Ok(Some(run.steps))
                    );
                }
            }
        }
    }

    #[test]
    fn a_forged_time_or_layout_or_entry_is_refused() {
        let track = Track::new(all_circuits().iter().find(|c| c.id == "monza").unwrap());
        let (run, lap, _) = ai_laps(&track, Mode::Regular, 2)
            .into_iter()
            .find(|(_, lap, _)| lap.valid)
            .expect("a valid lap");
        assert!(lap.valid);
        let quicker = Run {
            steps: run.steps - 240,
            inputs: run.inputs[..run.inputs.len() - 240].to_vec(),
            ..run.clone()
        };
        assert!(replay::verify(&quicker, &track).is_err(), "a lap cut short");
        let mut sectors = run.clone();
        sectors.sectors[0] -= 0.5;
        assert_eq!(replay::verify(&sectors, &track), Err("the sectors differ"));
        let mut teleported = run.clone();
        teleported.entry.translation.x += 30.0;
        assert_eq!(replay::verify(&teleported, &track), Err("not at the line"));
        let mut rocket = run.clone();
        rocket.entry.velocity *= 3.0;
        assert_eq!(
            replay::verify(&rocket, &track),
            Err("faster than the car goes")
        );
    }

    #[test]
    fn the_recorder_forgets_a_lap_given_up() {
        let mut recorder = Recorder::default();
        let entry = run::tests::sample().entry;
        let c = Controls::default();
        assert!(recorder.step(c, true, false, entry).is_none());
        for _ in 0..10 {
            recorder.step(c, true, false, entry);
        }
        assert_eq!(recorder.inputs.len(), 10);
        recorder.step(c, false, false, entry);
        assert!(recorder.entry.is_none() && recorder.inputs.is_empty());
    }
}
