//! The clock: what a lap is, when one has been driven, and how quick it was.
//!
//! The clock starts at the line, never on the grid. The car is set down a run-up
//! short of the start/finish line so a lap can begin at speed, and what happens
//! in that run-up is the driver's own business: the clock is armed by the first
//! crossing and every crossing after that finishes a lap and starts the next.
//! One rule, and it is the rule that was always wanted — the old one, that the
//! clock starts when the car moves off, only agreed with it because the car used
//! to be set down on the line itself.
//!
//! Two kinds of thing live in here, and the reset key is what tells them apart.
//! The lap in progress is the clock, how far round it has got, and whether it
//! has started — that is what `R` throws away. The records are the laps already
//! driven and the best of them, and a restart is not a reason to forget those:
//! you press it because the lap went wrong, and the thing you are chasing is
//! the one you are not driving. They go when the circuit does, because they
//! belong to it.

use bevy::prelude::*;

use crate::Reset;
use crate::car::{DriveSet, Player};
use crate::input::InputSet;
use crate::track::{Track, TrackSet};

/// Everything that judges the lap runs in here, after the car has moved.
/// Anything that wants to hear a lap finish in the same frame runs after it.
#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct LapSet;

/// Putting the clock and the board back runs in here. Anything that then has
/// something of its own to write onto the board — the ghost, with the time of
/// the lap saved for the circuit just arrived at — runs after it, or a switch
/// clears the board after the ghost has filled it in.
#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct ClockSet;

/// A lap has just been completed: its time, and whether it beat every lap
/// before it.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub(crate) struct LapFinished {
    pub time: f32,
    pub best: bool,
}

pub struct LapPlugin;

impl Plugin for LapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LapTimer>()
            .add_message::<LapFinished>()
            .add_systems(
                FixedUpdate,
                (tick, gate).chain().in_set(LapSet).after(DriveSet),
            )
            .add_systems(
                PreUpdate,
                start_again.in_set(ClockSet).after(InputSet).after(TrackSet),
            );
    }
}

#[derive(Resource)]
pub struct LapTimer {
    pub current: f32,
    pub last: Option<f32>,
    pub best: Option<f32>,
    pub completed: u32,
    running: bool,
    prev_along: Option<f32>,
    prev_progress: Option<f32>,
    net_progress: f32,
}

impl LapTimer {
    /// Whether the clock is going: the car has crossed the line, and a lap is on.
    pub fn running(&self) -> bool {
        self.running
    }

    /// Signed travel since the lap began, clamped for the ghost's lookup.
    pub(crate) fn progress(&self) -> f32 {
        self.net_progress.clamp(0.0, 1.0)
    }

    /// Give up the lap in progress and go back to the run-up. What the reset key
    /// does: the laps already driven and the best of them are not part of the lap
    /// in progress, so they stay, and the clock waits at the line again.
    fn abandon(&mut self) {
        self.current = 0.0;
        self.running = false;
        self.prev_along = None;
        self.prev_progress = None;
        self.net_progress = 0.0;
    }

    /// Count a lap driven before this session — the one saved as the ghost — as
    /// the best. It was driven round this circuit at this scale, so it is a time
    /// on the board like any other, and it is the time the delta is read against.
    pub(crate) fn remember(&mut self, time: f32) {
        self.best = Some(self.best.map_or(time, |best| best.min(time)));
    }
}

impl Default for LapTimer {
    fn default() -> Self {
        Self {
            current: 0.0,
            last: None,
            best: None,
            completed: 0,
            running: false,
            prev_along: None,
            prev_progress: None,
            net_progress: 0.0,
        }
    }
}

/// The clock only ever runs between one crossing of the line and the next.
/// [`gate`] is what starts it.
fn tick(time: Res<Time>, mut timer: ResMut<LapTimer>) {
    if timer.running {
        timer.current += time.delta_secs();
    }
}

fn gate(
    track: Res<Track>,
    mut timer: ResMut<LapTimer>,
    mut finished: MessageWriter<LapFinished>,
    cars: Query<&Transform, With<Player>>,
) {
    let Ok(car) = cars.single() else {
        return;
    };
    let pos = car.translation;
    let along = track.start_along(pos);
    let progress = track.progress(pos);
    if let Some(previous) = timer.prev_progress {
        // Unwrap the closed circuit, retaining direction. Reversing across the
        // line spends progress rather than instantly qualifying most of a lap.
        timer.net_progress += (progress - previous + 0.5).rem_euclid(1.0) - 0.5;
    }
    timer.prev_progress = Some(progress);
    let Some(prev) = timer.prev_along else {
        timer.prev_along = Some(along);
        return;
    };
    if prev <= 0.0 && along > 0.0 && track.on_start_gate(pos) {
        if !timer.running {
            // The end of the run-up. Everything before this is the driver's own
            // time, spent getting up to speed, and none of it is the lap.
            timer.running = true;
            timer.current = 0.0;
            timer.net_progress = 0.0;
        } else if timer.net_progress > 0.95 {
            let time = timer.current;
            finished.write(LapFinished {
                time,
                best: timer.best.is_none_or(|best| time < best),
            });
            timer.last = Some(timer.current);
            timer.best = Some(
                timer
                    .best
                    .map_or(timer.current, |best| best.min(timer.current)),
            );
            timer.completed += 1;
            timer.current = 0.0;
            timer.net_progress = 0.0;
        }
    }
    timer.prev_along = Some(along);
}

/// A new circuit has no history on it; a restart on the one being driven gives
/// up the lap in progress and nothing else.
///
/// The track key builds the circuit before this runs, so a changed [`Track`] is
/// how a switch tells itself apart from a restart — including the first frame of
/// all, where the board is empty anyway.
fn start_again(mut resets: MessageReader<Reset>, track: Res<Track>, mut timer: ResMut<LapTimer>) {
    let restarted = resets.read().next().is_some();
    if track.is_changed() {
        *timer = LapTimer::default();
    } else if restarted {
        timer.abandon();
    }
}

pub fn format_time(secs: f32) -> String {
    let hundredths = (secs.max(0.0) * 100.0).round() as u64;
    let m = hundredths / 6000;
    let s = (hundredths / 100) % 60;
    let fraction = hundredths % 100;
    format!("{m}:{s:02}.{fraction:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::car::Car;

    fn mid_lap() -> LapTimer {
        LapTimer {
            current: 42.0,
            last: Some(61.0),
            best: Some(58.0),
            completed: 3,
            running: true,
            prev_along: Some(2.0),
            prev_progress: Some(0.8),
            net_progress: 0.8,
        }
    }

    /// A restart puts the clock back to the line and leaves the board alone.
    /// The best lap is what the driver is chasing; taking it away for pressing
    /// `R` would punish the restart rather than the lap that went wrong.
    #[test]
    fn a_restart_puts_the_clock_back_and_keeps_the_board() {
        let mut app = App::new();
        app.add_message::<Reset>()
            .insert_resource(Track::any())
            .insert_resource(mid_lap())
            .add_systems(Update, start_again);

        // The first frame sees a freshly inserted track, so it clears the board
        // the way a switch does. From then on it is a restart.
        app.update();
        app.insert_resource(mid_lap());
        app.update();
        assert_eq!(
            app.world().resource::<LapTimer>().current,
            42.0,
            "the clock moved with no reset"
        );

        app.world_mut().write_message(Reset);
        app.update();
        let timer = app.world().resource::<LapTimer>();
        assert_eq!(timer.current, 0.0);
        assert!(!timer.running);
        assert_eq!(timer.net_progress, 0.0);
        assert!(timer.prev_along.is_none() && timer.prev_progress.is_none());
        assert_eq!(timer.completed, 3, "a restart forgot the laps driven");
        assert_eq!(timer.last, Some(61.0), "a restart forgot the last lap");
        assert_eq!(timer.best, Some(58.0), "a restart forgot the best lap");
    }

    /// A different circuit is a different board: nothing set on the old one
    /// means anything on the new one.
    #[test]
    fn a_new_circuit_clears_the_board() {
        let mut app = App::new();
        app.add_message::<Reset>()
            .insert_resource(Track::any())
            .insert_resource(mid_lap())
            .add_systems(Update, start_again);
        app.update();
        app.insert_resource(mid_lap());
        app.update();

        // What the track key does: a new Track, then a reset for everyone else.
        app.insert_resource(Track::any());
        app.world_mut().write_message(Reset);
        app.update();
        let timer = app.world().resource::<LapTimer>();
        assert_eq!(timer.completed, 0);
        assert_eq!(timer.current, 0.0);
        assert!(timer.last.is_none() && timer.best.is_none());
        assert!(!timer.running);
    }

    /// A lap saved from an earlier session is a time on the board, and only ever
    /// improves it.
    #[test]
    fn a_remembered_lap_is_the_best_until_it_is_beaten() {
        let mut timer = LapTimer::default();
        timer.remember(58.0);
        assert_eq!(timer.best, Some(58.0));
        timer.remember(61.0);
        assert_eq!(timer.best, Some(58.0), "a slower saved lap took the board");
        timer.remember(55.5);
        assert_eq!(timer.best, Some(55.5));
        assert!(
            timer.last.is_none(),
            "a saved lap is not this session's last"
        );
    }

    /// A finished lap has to announce itself, and say whether it was the best:
    /// the ghost is built on hearing it.
    #[test]
    fn a_finished_lap_is_announced() {
        #[derive(Resource, Default)]
        struct Heard(Vec<LapFinished>);
        fn collect(mut laps: MessageReader<LapFinished>, mut heard: ResMut<Heard>) {
            heard.0.extend(laps.read().copied());
        }

        let track = Track::any();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<LapFinished>()
            .init_resource::<Heard>()
            .init_resource::<LapTimer>()
            .insert_resource(Track::any())
            .add_systems(Update, (gate, collect).chain());
        let start = track.start_transform();
        let car = app.world_mut().spawn((Car::default(), Player, start)).id();
        app.update();

        // The run-up and then twice round, on the centreline, with the clock
        // ticking. Far enough for three crossings of the line: one to arm the
        // clock at the end of the run-up, and one to finish each lap.
        let mut here = start.translation;
        for _ in 0..((track.length() * 2.0 + 120.0) / 0.5) as usize {
            let ground = track.ground(here);
            here = ground.centre + ground.tangent * 0.5;
            app.world_mut()
                .entity_mut(car)
                .get_mut::<Transform>()
                .unwrap()
                .translation = here;
            app.world_mut().resource_mut::<LapTimer>().current += 0.02;
            app.update();
        }

        let heard = &app.world().resource::<Heard>().0;
        assert_eq!(heard.len(), 2, "heard {} laps of 2", heard.len());
        assert!(heard[0].best, "the first lap is always the best so far");
        assert!(heard[0].time > 15.0, "lap time {}", heard[0].time);
        // The second is a best exactly when it was quicker than the first —
        // whichever way the half-metre steps happened to land.
        assert_eq!(
            heard[1].best,
            heard[1].time < heard[0].time,
            "lap two took {} against {} and was called best={}",
            heard[1].time,
            heard[0].time,
            heard[1].best
        );
    }

    #[test]
    fn formats_arcade_clock() {
        assert_eq!(format_time(0.0), "0:00.00");
        assert_eq!(format_time(5.3), "0:05.30");
        assert_eq!(format_time(83.456), "1:23.46");
        assert_eq!(format_time(59.999), "1:00.00");
    }

    #[test]
    fn backing_up_then_recrossing_the_line_is_not_a_lap() {
        let track = Track::any();
        let start = track.start_transform();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<LapFinished>()
            .init_resource::<LapTimer>()
            .insert_resource(track)
            .add_systems(Update, gate);
        let car = app.world_mut().spawn((Car::default(), Player, start)).id();
        app.update();

        // The run-up and a good way past the line.
        let mut here = start.translation;
        let mut forward = Vec::new();
        for _ in 0..140 {
            let ground = app.world().resource::<Track>().ground(here);
            here = ground.centre + ground.tangent * 0.5;
            forward.push(here);
        }
        let drive = |app: &mut App, path: &mut dyn Iterator<Item = Vec3>| {
            for here in path {
                app.world_mut()
                    .get_mut::<Transform>(car)
                    .unwrap()
                    .translation = here;
                app.update();
            }
        };
        // Over the line, which arms the clock; back over it; and over it again.
        drive(&mut app, &mut forward.iter().copied());
        assert!(
            app.world().resource::<LapTimer>().running(),
            "driving up to the line did not start the clock"
        );
        drive(&mut app, &mut forward.iter().rev().copied());
        drive(&mut app, &mut forward.iter().copied());
        assert_eq!(app.world().resource::<LapTimer>().completed, 0);
    }

    /// The whole point of the run-up: the clock is armed by the line, not by the
    /// car moving off, so the metres spent getting up to speed are free. Drive
    /// up to the line and the clock is still at zero until the moment it is
    /// crossed.
    #[test]
    fn the_clock_starts_at_the_line_and_not_on_the_grid() {
        let track = Track::any();
        let start = track.start_transform();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<LapFinished>()
            .init_resource::<LapTimer>()
            .insert_resource(Track::any())
            .add_systems(Update, (tick, gate).chain());
        let car = app.world_mut().spawn((Car::default(), Player, start)).id();
        app.update();
        assert!(
            !app.world().resource::<LapTimer>().running(),
            "the clock started on the grid"
        );

        let mut here = start.translation;
        let mut armed = None;
        for _ in 0..200 {
            let ground = track.ground(here);
            here = ground.centre + ground.tangent * 0.5;
            app.world_mut()
                .get_mut::<Transform>(car)
                .unwrap()
                .translation = here;
            app.update();
            if app.world().resource::<LapTimer>().running() {
                armed = Some(here);
                break;
            }
        }
        let armed = armed.expect("the clock never started");
        let along = track.start_along(armed);
        assert!(
            (0.0..1.0).contains(&along),
            "the clock started {along:.1} m from the line, not at it"
        );
        assert_eq!(
            app.world().resource::<LapTimer>().current,
            0.0,
            "the lap began with time already on it"
        );
    }
}
