use bevy::prelude::*;

use crate::car::{Car, DriveSet};
use crate::track::Track;
use crate::Reset;

pub struct LapPlugin;

impl Plugin for LapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LapTimer>()
            .add_systems(Update, tick)
            .add_systems(Update, gate.after(DriveSet))
            .add_systems(Update, start_again.after(DriveSet));
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
    max_progress: f32,
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
            max_progress: 0.0,
        }
    }
}

fn tick(time: Res<Time>, mut timer: ResMut<LapTimer>, cars: Query<&Car>) {
    if timer.running {
        timer.current += time.delta_secs();
        return;
    }
    if cars.iter().any(|car| car.velocity.length() > 0.4) {
        timer.running = true;
    }
}

fn gate(track: Res<Track>, mut timer: ResMut<LapTimer>, cars: Query<&Transform, With<Car>>) {
    let Ok(car) = cars.single() else {
        return;
    };
    let pos = car.translation;
    let along = track.start_along(pos);
    let progress = track.progress(pos);
    if progress > timer.max_progress && progress < 0.97 {
        timer.max_progress = progress;
    }
    let Some(prev) = timer.prev_along else {
        timer.prev_along = Some(along);
        return;
    };
    if prev <= 0.0 && along > 0.0 && track.on_start_gate(pos) && timer.max_progress > 0.55 {
        timer.last = Some(timer.current);
        timer.best = Some(
            timer
                .best
                .map_or(timer.current, |best| best.min(timer.current)),
        );
        timer.completed += 1;
        timer.current = 0.0;
        timer.max_progress = 0.0;
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
    let t = secs.max(0.0);
    let m = (t / 60.0) as u32;
    let s = t % 60.0;
    format!("{m}:{s:05.2}")
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
                max_progress: 0.8,
            })
            .add_systems(Update, start_again);

        app.update();
        assert_eq!(app.world().resource::<LapTimer>().completed, 3, "reset by itself");

        app.world_mut().write_message(Reset);
        app.update();
        let timer = app.world().resource::<LapTimer>();
        assert_eq!(timer.completed, 0);
        assert_eq!(timer.current, 0.0);
        assert!(timer.last.is_none() && timer.best.is_none());
        assert!(!timer.running);
    }

    #[test]
    fn formats_arcade_clock() {
        assert_eq!(format_time(0.0), "0:00.00");
        assert_eq!(format_time(5.3), "0:05.30");
        assert_eq!(format_time(83.456), "1:23.46");
    }
}
