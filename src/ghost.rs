//! Your fastest lap, driven again alongside you, and the gap to it.
//!
//! While a lap is running the player's pose is recorded every frame, against
//! the clock and against how far round the circuit it is. When a lap finishes
//! faster than any before, that recording becomes the ghost: a translucent copy
//! of the car that replays it in step with the current lap's clock, and a delta
//! — how many seconds ahead or behind this lap is, against the ghost at the same
//! point of the circuit — read off continuously. The delta is keyed to position,
//! not to time: comparing where two cars are at the same instant gives a
//! distance, and a distance is not something you can drive against. Asking
//! "when did the ghost reach the place I am now" gives seconds, which is.
//!
//! `G` shows and hides it. A restart wipes it along with the best lap it came
//! from, because the best lap is where it came from.

use bevy::{light::NotShadowCaster, prelude::*, world_serialization::WorldInstanceReady};

use crate::car::{Player, MODEL, SCALE};
use crate::lap::{LapFinished, LapSet, LapTimer};
use crate::track::Track;
use crate::Reset;

/// How much of the car is left in the ghost.
const FADE: f32 = 0.38;
/// A faint amber warmth, so the ghost reads as the ghost and not as a second
/// car in the shadows.
const GLOW: LinearRgba = LinearRgba::new(0.22, 0.14, 0.02, 1.0);
/// Recordings longer than this are abandoned — nobody is chasing that lap.
const LONGEST: usize = 60_000;

pub struct GhostPlugin;

impl Plugin for GhostPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, (toggle, finish.after(LapSet)))
            .add_systems(PostUpdate, (record, replay).chain());
    }
}

/// One frame of a lap.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Sample {
    /// Seconds into the lap.
    time: f32,
    /// How far round the circuit, 0 to 1. Never decreases along a recording.
    progress: f32,
    translation: Vec3,
    rotation: Quat,
}

/// A lap as it was driven.
#[derive(Default, Debug)]
struct Recording {
    samples: Vec<Sample>,
}

impl Recording {
    fn push(&mut self, time: f32, progress: f32, transform: &Transform) {
        if self.samples.len() >= LONGEST {
            return;
        }
        // Progress is held from going backwards so it can be searched. A car
        // that spins and reverses still gets the time it *first* reached a
        // point, which is the time worth comparing against.
        let progress = self
            .samples
            .last()
            .map_or(progress, |last| last.progress.max(progress));
        self.samples.push(Sample {
            time,
            progress,
            translation: transform.translation,
            rotation: transform.rotation,
        });
    }

    /// How long the lap took.
    fn duration(&self) -> f32 {
        self.samples.last().map_or(0.0, |last| last.time)
    }

    /// Where the car was `time` seconds into the lap. Past the end it wraps to
    /// the start — the lap is a loop, and a ghost that has finished keeps
    /// going round, which is what lapping the slower car looks like.
    fn pose_at(&self, time: f32) -> Option<(Vec3, Quat)> {
        let (first, last) = (self.samples.first()?, self.samples.last()?);
        let duration = self.duration();
        if duration <= 0.0 {
            return Some((first.translation, first.rotation));
        }
        let time = time.rem_euclid(duration);
        let i = self.samples.partition_point(|s| s.time < time);
        Some(match i {
            0 => (first.translation, first.rotation),
            i if i >= self.samples.len() => (last.translation, last.rotation),
            i => {
                let (a, b) = (self.samples[i - 1], self.samples[i]);
                let t = ((time - a.time) / (b.time - a.time).max(1e-6)).clamp(0.0, 1.0);
                (a.translation.lerp(b.translation, t), a.rotation.slerp(b.rotation, t))
            }
        })
    }

    /// The clock when the lap first reached `progress` round the circuit.
    fn time_at(&self, progress: f32) -> Option<f32> {
        let (first, last) = (self.samples.first()?, self.samples.last()?);
        let i = self.samples.partition_point(|s| s.progress < progress);
        Some(match i {
            0 => first.time,
            i if i >= self.samples.len() => last.time,
            i => {
                let (a, b) = (self.samples[i - 1], self.samples[i]);
                let t = ((progress - a.progress) / (b.progress - a.progress).max(1e-6)).clamp(0.0, 1.0);
                a.time.lerp(b.time, t)
            }
        })
    }
}

#[derive(Resource)]
pub(crate) struct Ghost {
    /// Whether the ghost is shown. `G`.
    pub(crate) on: bool,
    /// Seconds this lap is behind the ghost at this point of the circuit;
    /// negative is ahead. `None` until there is a ghost to be behind.
    pub(crate) delta: Option<f32>,
    /// The fastest lap so far, if any.
    best: Option<Recording>,
    /// The lap being driven now.
    recording: Recording,
    /// The translucent car.
    car: Entity,
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    let car = commands
        .spawn((
            Transform::from_scale(Vec3::splat(SCALE)),
            Visibility::Hidden,
            WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(MODEL))),
        ))
        .observe(fade)
        .id();
    commands.insert_resource(Ghost {
        on: true,
        delta: None,
        best: None,
        recording: Recording::default(),
        car,
    });
}

/// Make the loaded model a ghost: every material translucent with a faint
/// amber warmth, and none of it casting a shadow, because a shadow would give
/// it a weight it must not have.
fn fade(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    painted: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for entity in children.iter_descendants(ready.entity) {
        let Ok(handle) = painted.get(entity) else {
            continue;
        };
        let Some(material) = materials.get(&handle.0) else {
            continue;
        };
        let mut ghostly = material.clone();
        ghostly.base_color = ghostly.base_color.with_alpha(FADE);
        ghostly.alpha_mode = AlphaMode::Blend;
        ghostly.emissive = GLOW;
        let ghostly = materials.add(ghostly);
        commands
            .entity(entity)
            .insert((MeshMaterial3d(ghostly), NotShadowCaster));
    }
}

/// Write down where the car is, every frame the clock is running.
fn record(
    timer: Res<LapTimer>,
    track: Res<Track>,
    player: Query<&Transform, With<Player>>,
    mut ghost: ResMut<Ghost>,
) {
    if !timer.running() {
        return;
    }
    let Ok(transform) = player.single() else {
        return;
    };
    let progress = track.progress(transform.translation);
    ghost.recording.push(timer.current, progress, transform);
}

/// A lap has finished: if it was the best, it is the ghost now. Either way the
/// next lap starts on a clean sheet. A restart throws everything away.
fn finish(
    mut laps: MessageReader<LapFinished>,
    mut resets: MessageReader<Reset>,
    mut ghost: ResMut<Ghost>,
) {
    if resets.read().next().is_some() {
        ghost.best = None;
        ghost.recording = Recording::default();
        ghost.delta = None;
        return;
    }
    for lap in laps.read() {
        let recording = std::mem::take(&mut ghost.recording);
        if lap.best && recording.samples.len() > 1 {
            ghost.best = Some(recording);
        }
    }
}

/// Put the ghost where the best lap was at this point of the clock, and read
/// the gap where the player is on the circuit.
fn replay(
    timer: Res<LapTimer>,
    track: Res<Track>,
    mut ghost: ResMut<Ghost>,
    player: Query<&Transform, With<Player>>,
    mut ghosts: Query<(&mut Transform, &mut Visibility), Without<Player>>,
) {
    let Ok((mut transform, mut visibility)) = ghosts.get_mut(ghost.car) else {
        return;
    };
    let (pose, delta) = match &ghost.best {
        None => (None, None),
        Some(best) => {
            let pose = best.pose_at(timer.current);
            let delta = player
                .single()
                .ok()
                .and_then(|me| best.time_at(track.progress(me.translation)))
                .map(|then| timer.current - then);
            (pose, delta)
        }
    };
    *visibility = if ghost.on && pose.is_some() {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };
    if let Some((translation, rotation)) = pose {
        transform.translation = translation;
        transform.rotation = rotation;
    }
    ghost.delta = delta;
}

/// `G`, or the pad's north button, shows and hides the ghost.
fn toggle(keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>, mut ghost: ResMut<Ghost>) {
    let pressed = keys.just_pressed(KeyCode::KeyG)
        || pads.iter().any(|pad| pad.just_pressed(GamepadButton::North));
    if pressed {
        ghost.on = !ghost.on;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lap of `n` frames a tenth of a second apart, moving one metre in x per
    /// frame, progress spread evenly round.
    fn lap_of(n: usize) -> Recording {
        let mut lap = Recording::default();
        for i in 0..n {
            let t = i as f32;
            lap.push(
                0.1 * t,
                t / (n - 1) as f32,
                &Transform::from_xyz(t, 0.0, 0.0).with_rotation(Quat::from_rotation_y(0.01 * t)),
            );
        }
        lap
    }

    #[test]
    fn the_pose_is_interpolated_and_wraps_round() {
        let lap = lap_of(11);
        assert_eq!(lap.duration(), 1.0);
        let (at, _) = lap.pose_at(0.15).unwrap();
        assert!((at.x - 1.5).abs() < 1e-4, "quarter way between frames: {at}");
        // A ghost that finished keeps going: past the end it is back at the start.
        let (again, _) = lap.pose_at(1.15).unwrap();
        assert!((again.x - 1.5).abs() < 1e-3, "did not wrap: {again}");
        let (start, _) = lap.pose_at(0.0).unwrap();
        assert_eq!(start.x, 0.0);
    }

    #[test]
    fn the_clock_is_read_at_a_point_of_the_circuit() {
        let lap = lap_of(11);
        let halfway = lap.time_at(0.5).unwrap();
        assert!((halfway - 0.5).abs() < 1e-4, "halfway round took {halfway}");
        assert_eq!(lap.time_at(-1.0), Some(0.0), "before the start is the start");
        assert_eq!(lap.time_at(2.0), Some(1.0), "past the end is the end");
    }

    /// Ahead of the ghost is a negative number: the ghost took 20 s to get to
    /// halfway, and this lap is there at 18.
    #[test]
    fn ahead_is_negative() {
        let mut lap = Recording::default();
        lap.push(0.0, 0.0, &Transform::IDENTITY);
        lap.push(20.0, 0.5, &Transform::IDENTITY);
        lap.push(40.0, 1.0, &Transform::IDENTITY);
        let then = lap.time_at(0.5).unwrap();
        assert_eq!(18.0 - then, -2.0);
        assert_eq!(22.0 - then, 2.0);
    }

    /// A spin puts the car backwards along the circuit for a moment. The
    /// recording must stay searchable, and the time it reports for a point is
    /// the time the car first got there.
    #[test]
    fn progress_never_goes_backwards() {
        let mut lap = Recording::default();
        lap.push(0.0, 0.0, &Transform::IDENTITY);
        lap.push(1.0, 0.5, &Transform::IDENTITY);
        lap.push(2.0, 0.4, &Transform::IDENTITY);
        lap.push(3.0, 0.6, &Transform::IDENTITY);
        assert!(lap.samples.windows(2).all(|w| w[1].progress >= w[0].progress));
        assert_eq!(lap.time_at(0.5), Some(1.0), "first reached halfway at 1 s");
    }

    #[test]
    fn an_empty_recording_has_no_ghost_in_it() {
        let lap = Recording::default();
        assert_eq!(lap.pose_at(3.0), None);
        assert_eq!(lap.time_at(0.5), None);
        assert_eq!(lap.duration(), 0.0);
    }
}
