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
use crate::car::{Car, DriveSet, Mode, Player};
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
    pub invalid: bool,
    pub best_sectors: Vec<f32>,
    pub sector_notice: Option<SectorNotice>,
    sectors: Vec<f32>,
    sector_started: f32,
    previous_time: f32,
    notice_left: f32,
    running: bool,
    prev_along: Option<f32>,
    prev_progress: Option<f32>,
    net_progress: f32,
    finish_runup: f32,
}

#[derive(Clone, Copy)]
pub struct SectorNotice {
    pub number: usize,
    pub time: f32,
    pub delta: Option<f32>,
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
        self.finish_runup = 0.0;
        self.invalid = false;
        self.sectors.clear();
        self.sector_started = 0.0;
        self.previous_time = 0.0;
        self.sector_notice = None;
        self.notice_left = 0.0;
    }

    fn sector(&mut self, time: f32) {
        let duration = time - self.sector_started;
        let index = self.sectors.len();
        self.sector_notice = Some(SectorNotice {
            number: index + 1,
            time: duration,
            delta: self.best_sectors.get(index).map(|best| duration - best),
        });
        self.notice_left = 3.5;
        self.sectors.push(duration);
        self.sector_started = time;
    }

    fn sectors_through(&mut self, before: f32, after: f32, count: usize) {
        if after <= before {
            return;
        }
        while self.sectors.len() + 1 < count {
            let boundary = (self.sectors.len() + 1) as f32 / count as f32;
            if after < boundary {
                break;
            }
            let fraction = ((boundary - before) / (after - before)).clamp(0.0, 1.0);
            self.sector(self.previous_time.lerp(self.current, fraction));
        }
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
            invalid: false,
            best_sectors: Vec::new(),
            sector_notice: None,
            sectors: Vec::new(),
            sector_started: 0.0,
            previous_time: 0.0,
            notice_left: 0.0,
            running: false,
            prev_along: None,
            prev_progress: None,
            net_progress: 0.0,
            finish_runup: 0.0,
        }
    }
}

/// The clock only ever runs between one crossing of the line and the next.
/// [`gate`] is what starts it.
fn tick(time: Res<Time>, mut timer: ResMut<LapTimer>) {
    timer.notice_left = (timer.notice_left - time.delta_secs()).max(0.0);
    if timer.notice_left == 0.0 {
        timer.sector_notice = None;
    }
    if timer.running {
        timer.current += time.delta_secs();
    }
}

fn gate(
    track: Res<Track>,
    mut timer: ResMut<LapTimer>,
    mut finished: MessageWriter<LapFinished>,
    mut cars: Query<(&Transform, &mut Car), With<Player>>,
) {
    let Ok((at, mut car)) = cars.single_mut() else {
        return;
    };
    let legal = track.legal_contact(at, car.along);
    let recovered = car.recovered;
    if timer.running && (!legal || car.recovered) {
        timer.invalid = true;
    }
    car.recovered = false;
    let before = timer.net_progress;
    let pos = at.translation;
    let along = track.start_along(pos);
    // Where the car says it is round the lap, which on a circuit that passes
    // over itself is the difference between the lap the driver is on and the
    // road underneath it. The clock reads the car's own answer rather than
    // asking the map again, so that a lap is timed on the road it was driven
    // on — see `Track::fix`.
    let progress = track.progress(pos, car.along);
    if let Some(previous) = timer.prev_progress {
        // Unwrap the closed circuit, retaining direction. Reversing across the
        // line spends progress rather than instantly qualifying most of a lap.
        let step = (progress - previous + 0.5).rem_euclid(1.0) - 0.5;
        timer.net_progress += step;
        // After a shortcut the nearest-road progress may have jumped. An
        // invalid lap can still end after a real forward run-up to the line.
        // Briefly reversing over the line cannot wash away its invalidity.
        let metres = step * track.length();
        if !legal || recovered || metres < -0.001 || (0.02..0.8).contains(&progress) {
            timer.finish_runup = 0.0;
        } else if progress > 0.8 && (0.0..2.0).contains(&metres) {
            timer.finish_runup += metres;
        }
    }
    if timer.running {
        let after = timer.net_progress;
        timer.sectors_through(before, after, track.sector_count());
    }
    timer.prev_progress = Some(progress);
    let Some(prev) = timer.prev_along else {
        timer.prev_along = Some(along);
        return;
    };
    if !recovered && prev <= 0.0 && along > 0.0 && track.on_start_gate(pos, car.along) {
        if !timer.running && legal {
            // The end of the run-up. Everything before this is the driver's own
            // time, spent getting up to speed, and none of it is the lap.
            timer.running = true;
            timer.current = 0.0;
            timer.net_progress = progress;
            timer.invalid = false;
            timer.finish_runup = 0.0;
        } else if timer.running
            && (timer.net_progress > 0.95 || (timer.invalid && timer.finish_runup > 5.0))
        {
            let time = timer.current;
            if timer.sectors.len() + 1 == track.sector_count() {
                timer.sector(time);
            } else {
                timer.sector_notice = None;
            }
            let best = !timer.invalid
                && timer.sectors.len() == track.sector_count()
                && timer.best.is_none_or(|best| time < best);
            finished.write(LapFinished { time, best });
            if !timer.invalid {
                timer.last = Some(time);
            }
            if best {
                timer.best = Some(time);
                timer.best_sectors = timer.sectors.clone();
            }
            timer.completed += 1;
            timer.current = 0.0;
            timer.net_progress = progress;
            timer.invalid = !legal;
            timer.finish_runup = 0.0;
            timer.sectors.clear();
            timer.sector_started = 0.0;
        }
    }
    timer.prev_along = Some(along);
    timer.previous_time = timer.current;
}

/// A new circuit or driving mode starts a fresh board; a restart gives
/// up the lap in progress and nothing else.
///
/// The track key builds the circuit before this runs, so a changed [`Track`] is
/// how a circuit switch tells itself apart from a restart. A mode change also
/// clears the board before the ghost loads its saved best — including the first frame of
/// all, where the board is empty anyway.
fn start_again(
    mut resets: MessageReader<Reset>,
    track: Res<Track>,
    mode: Res<Mode>,
    mut timer: ResMut<LapTimer>,
) {
    let restarted = resets.read().next().is_some();
    if track.is_changed() || mode.is_changed() {
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
            ..default()
        }
    }

    #[test]
    fn an_invalid_shortcut_can_start_fresh_after_a_forward_runup() {
        let track = Track::any();
        let points: Vec<_> = track.map_points().collect();
        let first = points.len() - 30;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<LapFinished>()
            .insert_resource(LapTimer {
                running: true,
                invalid: true,
                net_progress: 0.3,
                best: Some(100.0),
                ..default()
            })
            .insert_resource(track)
            .add_systems(Update, gate);
        let car = app
            .world_mut()
            .spawn((Player, Car::default(), Transform::default()))
            .id();
        for i in first..points.len() + 3 {
            let (point, along) = points[i % points.len()];
            let next = points[(i + 1) % points.len()].0;
            *app.world_mut().get_mut::<Transform>(car).unwrap() =
                Transform::from_translation(point).looking_to(next - point, Vec3::Y);
            app.world_mut().get_mut::<Car>(car).unwrap().along = Some(along);
            app.world_mut().resource_mut::<LapTimer>().current += 0.01;
            app.update();
        }
        let timer = app.world().resource::<LapTimer>();
        assert_eq!(timer.completed, 1);
        assert!(!timer.invalid);
        assert_eq!(timer.best, Some(100.0));
    }

    #[test]
    fn sectors_interpolate_once_and_reverse_travel_spends_time() {
        let mut timer = LapTimer {
            best_sectors: vec![10.0; 4],
            previous_time: 8.0,
            current: 12.0,
            ..default()
        };
        timer.sectors_through(0.20, 0.30, 4);
        assert_eq!(timer.sectors, vec![10.0]);
        assert_eq!(timer.sector_notice.unwrap().delta, Some(0.0));
        timer.previous_time = 12.0;
        timer.current = 18.0;
        timer.sectors_through(0.30, 0.20, 4);
        timer.sectors_through(0.20, 0.30, 4);
        assert_eq!(
            timer.sectors.len(),
            1,
            "recrossing a sector cannot complete it twice"
        );
        timer.previous_time = 25.0;
        timer.current = 27.0;
        timer.sectors_through(0.49, 0.51, 4);
        assert!((timer.sectors[1] - 16.0).abs() < 1e-4);
        assert!((timer.sector_notice.unwrap().delta.unwrap() - 6.0).abs() < 1e-4);
        timer.abandon();
        assert!(timer.sectors.is_empty() && timer.sector_notice.is_none());
        assert_eq!(timer.best_sectors, vec![10.0; 4]);
    }

    #[test]
    fn invalid_lap_keeps_clock_and_reference_until_a_fresh_start() {
        let track = Track::any();
        let points: Vec<_> = track.map_points().collect();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<LapFinished>()
            .insert_resource(LapTimer {
                running: true,
                best: Some(100.0),
                best_sectors: vec![25.0; 4],
                ..default()
            })
            .insert_resource(track)
            .add_systems(Update, gate);
        let car = app
            .world_mut()
            .spawn((Player, Car::default(), Transform::default()))
            .id();
        for (i, (point, along)) in points.iter().enumerate() {
            let next = points[(i + 1) % points.len()].0;
            let mut at = Transform::from_translation(*point).looking_to(next - *point, Vec3::Y);
            if i == points.len() / 3 {
                at.translation += *at.right() * 5.0;
            }
            app.world_mut()
                .get_mut::<Transform>(car)
                .unwrap()
                .clone_from(&at);
            app.world_mut().get_mut::<Car>(car).unwrap().along = Some(*along);
            app.world_mut().resource_mut::<LapTimer>().current += 0.01;
            app.update();
            if i > points.len() / 3 {
                assert!(app.world().resource::<LapTimer>().invalid);
            }
        }
        assert!(app.world().resource::<LapTimer>().current > 1.0);
        // Forward crossing closes the invalid lap and starts a fresh valid one.
        for &(point, along) in points.iter().take(3) {
            app.world_mut()
                .get_mut::<Transform>(car)
                .unwrap()
                .translation = point;
            app.world_mut().get_mut::<Car>(car).unwrap().along = Some(along);
            app.update();
        }
        let timer = app.world().resource::<LapTimer>();
        assert_eq!(timer.completed, 1);
        assert!(!timer.invalid);
        assert_eq!(timer.best, Some(100.0));
        assert_eq!(timer.best_sectors, vec![25.0; 4]);
        assert!(timer.last.is_none());
    }

    #[test]
    fn all_circuits_time_each_sector_on_both_decks_and_close_the_seam() {
        for circuit in crate::track::all_circuits() {
            let track = Track::new(circuit);
            let count = track.sector_count();
            assert!((4..=8).contains(&count));
            let points: Vec<_> = track.map_points().collect();
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .add_message::<LapFinished>()
                .init_resource::<LapTimer>()
                .insert_resource(track)
                .add_systems(Update, gate);
            let car = app
                .world_mut()
                .spawn((Player, Car::default(), Transform::default()))
                .id();
            for i in (points.len() - 5)..(points.len() * 2 + 3) {
                let (point, along) = points[i % points.len()];
                let next = points[(i + 1) % points.len()].0;
                *app.world_mut().get_mut::<Transform>(car).unwrap() =
                    Transform::from_translation(point).looking_to(next - point, Vec3::Y);
                app.world_mut().get_mut::<Car>(car).unwrap().along = Some(along);
                app.world_mut().resource_mut::<LapTimer>().current += 0.01;
                app.update();
            }
            let timer = app.world().resource::<LapTimer>();
            assert_eq!(timer.completed, 1, "{}", circuit.name);
            assert!(!timer.invalid, "{}", circuit.name);
            assert_eq!(timer.best_sectors.len(), count, "{}", circuit.name);
            assert!((timer.best_sectors.iter().sum::<f32>() - timer.best.unwrap()).abs() < 0.001);
            assert!(timer.best_sectors.iter().all(|s| *s > 0.0));
        }
    }

    #[test]
    fn changing_mode_clears_the_current_lap_and_board() {
        let mut app = App::new();
        app.add_message::<Reset>()
            .insert_resource(Track::any())
            .init_resource::<Mode>()
            .init_resource::<LapTimer>()
            .add_systems(Update, start_again);
        app.update();
        app.insert_resource(mid_lap());
        app.insert_resource(Mode::Pro);
        app.world_mut().write_message(Reset);
        app.update();
        let timer = app.world().resource::<LapTimer>();
        assert_eq!(timer.current, 0.0);
        assert_eq!(timer.completed, 0);
        assert!(!timer.running);
        assert!(timer.best.is_none() && timer.last.is_none());
    }

    /// A restart puts the clock back to the line and leaves the board alone.
    /// The best lap is what the driver is chasing; taking it away for pressing
    /// `R` would punish the restart rather than the lap that went wrong.
    #[test]
    fn a_restart_puts_the_clock_back_and_keeps_the_board() {
        let mut app = App::new();
        app.add_message::<Reset>()
            .insert_resource(Track::any())
            .init_resource::<Mode>()
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
            .init_resource::<Mode>()
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
