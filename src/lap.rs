use bevy::prelude::*;

use crate::Reset;
use crate::car::{Car, DriveSet, Player};
use crate::input::InputSet;
use crate::track::Track;

/// Everything that judges the lap runs in here, after the car has moved.
/// Anything that wants to hear a lap finish in the same frame runs after it.
#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct LapSet;

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
            .add_systems(PreUpdate, start_again.after(InputSet));
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
    /// Whether the clock is going: the car has moved off, and a lap is on.
    pub fn running(&self) -> bool {
        self.running
    }

    /// Signed travel since the lap began, clamped for the ghost's lookup.
    pub(crate) fn progress(&self) -> f32 {
        self.net_progress.clamp(0.0, 1.0)
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

fn tick(time: Res<Time>, mut timer: ResMut<LapTimer>, cars: Query<&Car, With<Player>>) {
    if timer.running {
        timer.current += time.delta_secs();
        return;
    }
    if cars.iter().any(|car| car.velocity.length() > 0.4) {
        timer.running = true;
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
    if prev <= 0.0 && along > 0.0 && track.on_start_gate(pos) && timer.net_progress > 0.95 {
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
        timer.running = true;
    }
    timer.prev_along = Some(along);
}

fn start_again(mut resets: MessageReader<Reset>, mut timer: ResMut<LapTimer>) {
    if resets.read().next().is_some() {
        *timer = LapTimer::default();
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

    /// A reset has to leave no trace of the lap that was running.
    #[test]
    fn a_reset_puts_the_clock_back() {
        let mut app = App::new();
        app.add_message::<Reset>()
            .insert_resource(LapTimer {
                current: 42.0,
                last: Some(61.0),
                best: Some(58.0),
                completed: 3,
                running: true,
                prev_along: Some(2.0),
                prev_progress: Some(0.8),
                net_progress: 0.8,
            })
            .add_systems(Update, start_again);

        app.update();
        assert_eq!(
            app.world().resource::<LapTimer>().completed,
            3,
            "reset by itself"
        );

        app.world_mut().write_message(Reset);
        app.update();
        let timer = app.world().resource::<LapTimer>();
        assert_eq!(timer.completed, 0);
        assert_eq!(timer.current, 0.0);
        assert!(timer.last.is_none() && timer.best.is_none());
        assert!(!timer.running);
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

        let track = Track::new();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<LapFinished>()
            .init_resource::<Heard>()
            .init_resource::<LapTimer>()
            .insert_resource(Track::new())
            .add_systems(Update, (gate, collect).chain());
        let start = track.start_transform();
        let car = app.world_mut().spawn((Car::default(), Player, start)).id();
        app.update();

        // Twice round, on the centreline, with the clock ticking.
        let mut here = start.translation;
        for _ in 0..2200 {
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
        let track = Track::new();
        let start = track.start_transform();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<LapFinished>()
            .init_resource::<LapTimer>()
            .insert_resource(track)
            .add_systems(Update, gate);
        let car = app.world_mut().spawn((Car::default(), Player, start)).id();
        app.update();
        let mut here = start.translation;
        let mut backwards = Vec::new();
        for _ in 0..100 {
            let ground = app.world().resource::<Track>().ground(here);
            here = ground.centre - ground.tangent * 0.5;
            backwards.push(here);
            app.world_mut()
                .get_mut::<Transform>(car)
                .unwrap()
                .translation = here;
            app.update();
        }
        for here in backwards
            .into_iter()
            .rev()
            .chain([start.translation + *start.forward()])
        {
            app.world_mut()
                .get_mut::<Transform>(car)
                .unwrap()
                .translation = here;
            app.update();
        }
        assert_eq!(app.world().resource::<LapTimer>().completed, 0);
    }
}
