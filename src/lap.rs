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
    pub valid: bool,
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
    /// The fastest each sector has ever been driven on a lap that was still
    /// valid at the time, on this circuit and mode, whichever lap it was in.
    pub ever_sectors: Vec<f32>,
    /// The world record's sectors, when its ghost has been downloaded: what
    /// purple means then. Never saved, and never mistaken for your own.
    pub world_sectors: Vec<f32>,
    /// Set when [`Self::ever_sectors`] improved, until someone saves it.
    pub ever_improved: bool,
    /// How each sector of this lap has gone so far.
    pub splits: Vec<Split>,
    /// Why this lap stopped counting, the first time it did.
    pub why: Option<Why>,
    /// The last lap finished, for the summary card.
    pub report: Option<LapReport>,
    top_speed: f32,
    /// The slowest the car went this lap, and how far round it was then.
    slowest: Option<(f32, f32)>,
    pub sector_notice: Option<SectorNotice>,
    /// Physics steps since the lap started. The clock is this times the step,
    /// multiplied once rather than added up step by step, which over a minute
    /// of f32 additions drifts by milliseconds.
    ticks: u32,
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
    pub split: Split,
}

/// What made a lap stop counting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// All four wheels left the asphalt and kerbs, in this sector.
    OffTrack { sector: usize },
    /// The car was fetched back to the road, in this sector.
    Rescued { sector: usize },
    /// The game was paused during a shared-practice session.
    Paused,
}

/// Everything about one finished lap the summary card shows.
#[derive(Clone, Debug, PartialEq)]
pub struct LapReport {
    /// Laps finished this session, this one included.
    pub number: u32,
    pub time: f32,
    pub valid: bool,
    /// A new best lap.
    pub best: bool,
    /// The best lap before this one, if there was one.
    pub previous_best: Option<f32>,
    pub sectors: Vec<f32>,
    pub splits: Vec<Split>,
    pub why: Option<Why>,
    /// Metres a second.
    pub top_speed: f32,
    /// The slowest point of the lap: speed in metres a second, and the sector.
    pub slowest: Option<(f32, usize)>,
}

/// How a sector went, in the colours timing screens use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Split {
    /// Faster than that sector has ever been driven.
    Purple,
    /// Faster than the same sector of the best lap.
    Green,
    /// Slower than the best lap there.
    Yellow,
    /// Nothing to compare with yet, or a lap that is already invalid.
    Plain,
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
        self.ticks = 0;
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
        self.splits.clear();
        self.why = None;
        self.top_speed = 0.0;
        self.slowest = None;
    }

    /// The lap in progress no longer counts, and this is why. The first reason
    /// is the one kept: it is where the lap was lost.
    pub(crate) fn invalidate(&mut self, why: Why) {
        self.invalid = true;
        self.why.get_or_insert(why);
    }

    /// The timer as it stands the step a lap starts: running from zero, with
    /// the car `along` past the line and `progress` round the lap. Where a
    /// replay picks the lap up.
    pub(crate) fn armed(along: f32, progress: f32) -> Self {
        Self {
            running: true,
            net_progress: progress,
            prev_along: Some(along),
            prev_progress: Some(progress),
            ..Self::default()
        }
    }

    /// Count one physics step of the clock, as `tick` does in the game.
    pub(crate) fn count(&mut self, dt: f32) {
        if self.running {
            self.ticks += 1;
            self.current = self.ticks as f32 * dt;
        }
    }

    /// Show a sector notice for the usual few seconds.
    #[cfg(feature = "visual-check")]
    pub(crate) fn announce(&mut self, notice: SectorNotice) {
        self.sector_notice = Some(notice);
        self.notice_left = 3.5;
    }

    fn sector(&mut self, time: f32) {
        let duration = time - self.sector_started;
        let index = self.sectors.len();
        let best = self.best_sectors.get(index).copied();
        let ever = self.ever_sectors.get(index).copied().or(best);
        // Purple is the world's fastest when the record's ghost is here.
        let fastest = match (ever, self.world_sectors.get(index).copied()) {
            (Some(ever), Some(world)) => Some(ever.min(world)),
            (ever, world) => ever.or(world),
        };
        let split = if self.invalid {
            Split::Plain
        } else if fastest.is_some_and(|fastest| duration < fastest) {
            Split::Purple
        } else if best.is_some_and(|best| duration <= best) {
            Split::Green
        } else if best.is_some() {
            Split::Yellow
        } else {
            Split::Plain
        };
        if !self.invalid && ever.is_none_or(|ever| duration < ever) {
            if index < self.ever_sectors.len() {
                self.ever_sectors[index] = duration;
                self.ever_improved = true;
            } else if index == self.ever_sectors.len() {
                self.ever_sectors.push(duration);
                self.ever_improved = true;
            }
        }
        self.splits.push(split);
        self.sector_notice = Some(SectorNotice {
            number: index + 1,
            time: duration,
            delta: best.map(|best| duration - best),
            split,
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
            ever_sectors: Vec::new(),
            world_sectors: Vec::new(),
            ever_improved: false,
            splits: Vec::new(),
            why: None,
            report: None,
            top_speed: 0.0,
            slowest: None,
            ticks: 0,
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
    timer.count(time.delta_secs());
}

/// What the judge is told about the car after one physics step. Everything in
/// it is read off the track by [`gate`], so the judge itself is a plain function
/// of the timer and this: the game, the tests and a server replaying a lap from
/// its inputs all judge it with the same code.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Step {
    /// All four wheels on asphalt or kerb.
    pub legal: bool,
    /// The car was fetched back to the road this step.
    pub recovered: bool,
    /// Signed distance past the start/finish plane, along the circuit.
    pub along: f32,
    /// How far round the lap, 0 to 1, on the road the car is on.
    pub progress: f32,
    /// Plan length of one lap, in metres.
    pub length: f32,
    /// How many sectors the lap is split into.
    pub sectors: usize,
    /// Metres a second, for the summary's top speed and slowest corner.
    pub speed: f32,
}

impl LapTimer {
    /// Judge one step. `on_gate` says whether the car crossed the line on the
    /// road rather than out in the runoff or under a bridge; it is only asked
    /// when the car has just crossed the start plane, because asking costs a
    /// lookup. Returns the lap that this step finished, if any.
    pub(crate) fn judge(
        &mut self,
        step: Step,
        on_gate: impl FnOnce() -> bool,
    ) -> Option<LapFinished> {
        let Step {
            legal,
            recovered,
            along,
            progress,
            length,
            sectors,
            speed,
        } = step;
        if self.running {
            let sector = self.sectors.len() + 1;
            if recovered {
                self.invalidate(Why::Rescued { sector });
            } else if !legal {
                self.invalidate(Why::OffTrack { sector });
            }
            self.top_speed = self.top_speed.max(speed);
            if self.slowest.is_none_or(|(slowest, _)| speed < slowest) {
                self.slowest = Some((speed, self.net_progress));
            }
        }
        let before = self.net_progress;
        if let Some(previous) = self.prev_progress {
            // Unwrap the closed circuit, retaining direction. Reversing across the
            // line spends progress rather than instantly qualifying most of a lap.
            let step = (progress - previous + 0.5).rem_euclid(1.0) - 0.5;
            self.net_progress += step;
            // After a shortcut the nearest-road progress may have jumped. An
            // invalid lap can still end after a real forward run-up to the line.
            // Briefly reversing over the line cannot wash away its invalidity.
            let metres = step * length;
            if !legal || recovered || metres < -0.001 || (0.02..0.8).contains(&progress) {
                self.finish_runup = 0.0;
            } else if progress > 0.8 && (0.0..2.0).contains(&metres) {
                self.finish_runup += metres;
            }
        }
        if self.running {
            let after = self.net_progress;
            self.sectors_through(before, after, sectors);
        }
        self.prev_progress = Some(progress);
        let Some(prev) = self.prev_along else {
            self.prev_along = Some(along);
            return None;
        };
        let mut finished = None;
        if !recovered && prev <= 0.0 && along > 0.0 && on_gate() {
            if !self.running && legal {
                // The end of the run-up. Everything before this is the driver's own
                // time, spent getting up to speed, and none of it is the lap.
                self.running = true;
                self.current = 0.0;
                self.ticks = 0;
                self.net_progress = progress;
                self.invalid = false;
                self.why = None;
                self.top_speed = speed;
                self.slowest = None;
                self.finish_runup = 0.0;
            } else if self.running
                && (self.net_progress > 0.95 || (self.invalid && self.finish_runup > 5.0))
            {
                let time = self.current;
                if self.sectors.len() + 1 == sectors {
                    self.sector(time);
                } else {
                    self.sector_notice = None;
                }
                let best = !self.invalid
                    && self.sectors.len() == sectors
                    && self.best.is_none_or(|best| time < best);
                finished = Some(LapFinished {
                    time,
                    best,
                    valid: !self.invalid,
                });
                let start = self.net_progress - 1.0;
                self.report = Some(LapReport {
                    number: self.completed + 1,
                    time,
                    valid: !self.invalid,
                    best,
                    previous_best: self.best,
                    sectors: self.sectors.clone(),
                    splits: self.splits.clone(),
                    why: self.why,
                    top_speed: self.top_speed,
                    slowest: self.slowest.map(|(speed, at)| {
                        let into = (at - start).clamp(0.0, 0.999);
                        (speed, (into * sectors as f32) as usize + 1)
                    }),
                });
                if !self.invalid {
                    self.last = Some(time);
                }
                if best {
                    self.best = Some(time);
                    self.best_sectors = self.sectors.clone();
                }
                self.completed += 1;
                self.current = 0.0;
                self.ticks = 0;
                self.net_progress = progress;
                self.invalid = !legal;
                self.why = (!legal).then_some(Why::OffTrack { sector: 1 });
                self.top_speed = speed;
                self.slowest = None;
                self.finish_runup = 0.0;
                self.sectors.clear();
                self.splits.clear();
                self.sector_started = 0.0;
            }
        }
        self.prev_along = Some(along);
        self.previous_time = self.current;
        finished
    }
}

/// Read what the judge needs off the track, and let it judge.
fn gate(
    track: Res<Track>,
    mut timer: ResMut<LapTimer>,
    mut finished: MessageWriter<LapFinished>,
    mut cars: Query<(&Transform, &mut Car), With<Player>>,
) {
    let Ok((at, mut car)) = cars.single_mut() else {
        return;
    };
    let recovered = car.recovered;
    car.recovered = false;
    let pos = at.translation;
    // Where the car says it is round the lap, which on a circuit that passes
    // over itself is the difference between the lap the driver is on and the
    // road underneath it. The clock reads the car's own answer rather than
    // asking the map again, so that a lap is timed on the road it was driven
    // on — see `Track::fix`.
    let step = Step {
        legal: track.legal_contact(at, car.along),
        recovered,
        along: track.start_along(pos),
        progress: track.progress(pos, car.along),
        length: track.length(),
        sectors: track.sector_count(),
        speed: car.velocity.length(),
    };
    if let Some(lap) = timer.judge(step, || track.on_start_gate(pos, car.along)) {
        finished.write(lap);
    }
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

    /// The judge needs nothing but the timer and what the track says: no app,
    /// no systems. This is how a lap replayed from its inputs will be judged.
    #[test]
    fn the_judge_times_a_lap_without_an_app() {
        let track = Track::any();
        let points: Vec<_> = track.map_points().collect();
        let mut timer = LapTimer::default();
        let mut laps = Vec::new();
        for i in (points.len() - 5)..(points.len() * 2 + 3) {
            let (pos, along) = points[i % points.len()];
            let at = Transform::from_translation(pos)
                .looking_to(points[(i + 1) % points.len()].0 - pos, Vec3::Y);
            timer.current += 0.01;
            let step = Step {
                legal: track.legal_contact(&at, Some(along)),
                recovered: false,
                along: track.start_along(pos),
                progress: track.progress(pos, Some(along)),
                length: track.length(),
                sectors: track.sector_count(),
                speed: 20.0 - 10.0 * (i % 50) as f32 / 50.0,
            };
            laps.extend(timer.judge(step, || track.on_start_gate(pos, Some(along))));
        }
        assert_eq!(laps.len(), 1);
        assert!(laps[0].valid && laps[0].best);
        assert_eq!(timer.best_sectors.len(), track.sector_count());
        let report = timer.report.as_ref().expect("a report of the lap");
        assert_eq!(report.number, 1);
        assert!(report.valid && report.best && report.why.is_none());
        assert_eq!(report.sectors.len(), track.sector_count());
        assert_eq!(report.top_speed, 20.0);
        let (slowest, sector) = report.slowest.unwrap();
        assert!(slowest < 10.3 && (1..=track.sector_count()).contains(&sector));
    }

    #[test]
    fn sectors_are_purple_green_or_yellow_against_the_ever_best_and_the_best_lap() {
        let mut timer = LapTimer {
            best_sectors: vec![10.0, 10.0, 10.0],
            ever_sectors: vec![9.0, 9.5, 9.8],
            ..default()
        };
        for (end, want) in [
            (8.5, Split::Purple),
            (18.5, Split::Green),
            (29.0, Split::Yellow),
        ] {
            timer.sector(end);
            timer.sector_started = end;
            assert_eq!(timer.sector_notice.unwrap().split, want);
        }
        assert_eq!(
            timer.splits,
            vec![Split::Purple, Split::Green, Split::Yellow]
        );
        assert_eq!(timer.ever_sectors, vec![8.5, 9.5, 9.8]);
        assert!(timer.ever_improved);
        timer.abandon();
        assert!(timer.splits.is_empty());
        assert_eq!(
            timer.ever_sectors,
            vec![8.5, 9.5, 9.8],
            "a restart forgot the best sectors"
        );
    }

    #[test]
    fn with_the_record_downloaded_purple_is_the_worlds_fastest() {
        let mut timer = LapTimer {
            best_sectors: vec![10.0],
            ever_sectors: vec![9.5],
            world_sectors: vec![9.0],
            ..default()
        };
        timer.sector(9.2);
        assert_eq!(
            timer.sector_notice.unwrap().split,
            Split::Green,
            "beats yours, not the world's"
        );
        assert_eq!(timer.ever_sectors, vec![9.2], "still your own best ever");
        timer.abandon();
        timer.sector(8.9);
        assert_eq!(timer.sector_notice.unwrap().split, Split::Purple);
    }

    #[test]
    fn an_invalid_lap_never_sets_a_best_sector() {
        let mut timer = LapTimer {
            best_sectors: vec![10.0],
            ever_sectors: vec![9.0],
            invalid: true,
            ..default()
        };
        timer.sector(5.0);
        assert_eq!(timer.sector_notice.unwrap().split, Split::Plain);
        assert_eq!(timer.ever_sectors, vec![9.0]);
        assert!(!timer.ever_improved);
    }

    #[test]
    fn a_first_lap_has_nothing_to_colour_against_but_starts_the_record() {
        let mut timer = LapTimer::default();
        timer.sector(12.0);
        assert_eq!(timer.sector_notice.unwrap().split, Split::Plain);
        assert_eq!(timer.ever_sectors, vec![12.0]);
    }

    #[test]
    fn an_invalid_lap_keeps_the_first_reason_it_stopped_counting() {
        let mut timer = LapTimer {
            running: true,
            sectors: vec![10.0, 10.0],
            ..default()
        };
        let step = |legal, recovered| Step {
            legal,
            recovered,
            along: -3.0,
            progress: 0.5,
            length: 800.0,
            sectors: 5,
            speed: 12.0,
        };
        timer.judge(step(false, false), || false);
        timer.judge(step(false, true), || false);
        assert!(timer.invalid);
        assert_eq!(timer.why, Some(Why::OffTrack { sector: 3 }));
        timer.abandon();
        assert_eq!(timer.why, None);
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
