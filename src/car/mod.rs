//! The car: the glTF, the wheels, and the keys that reach [`physics`].
//!
//! This module only turns input and the road under the wheels into a [`Controls`]
//! and a [`Surface`], hands both to one `step`, and puts the result back on the
//! transform. Nothing here decides how the car behaves.

mod physics;

use bevy::{prelude::*, world_serialization::WorldInstanceReady};

use crate::track::Track;
use crate::Reset;
use physics::{Controls, Surface};
pub(crate) use physics::{Car, HALF_TRACK, REAR_AXLE, SCALE, WHEEL_WIDTH};

const MODEL: &str = "models/shooting_brake.glb";
/// A long frame must not let the car tunnel through a corner.
const MAX_STEP: f32 = 1.0 / 30.0;

#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct DriveSet;

pub struct CarPlugin;

impl Plugin for CarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, drive.in_set(DriveSet))
            .add_systems(Update, turn_wheels.after(DriveSet));
    }
}

/// A wheel of the glTF, and how it is allowed to move.
#[derive(Component)]
struct Wheel {
    /// Front wheels follow the steering; rear wheels only roll.
    steers: bool,
    /// The pose the model was exported in, which both motions build on.
    rest: Quat,
    roll: f32,
}

fn setup(mut commands: Commands, track: Res<Track>, asset_server: Res<AssetServer>) {
    commands
        .spawn((
            Car::default(),
            track.start_transform().with_scale(Vec3::splat(SCALE)),
            Visibility::default(),
            WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(MODEL))),
        ))
        .observe(attach_wheels);
}

fn attach_wheels(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    names: Query<&Name>,
    transforms: Query<&Transform>,
) {
    for entity in children.iter_descendants(ready.entity) {
        let Ok(name) = names.get(entity) else {
            continue;
        };
        // `make_shooting_brake.py` names the hubs WheelFL, WheelFR, WheelRL, WheelRR.
        let Some(corner) = name.as_str().strip_prefix("Wheel") else {
            continue;
        };
        let rest = transforms.get(entity).map(|t| t.rotation).unwrap_or_default();
        commands.entity(entity).insert(Wheel {
            steers: corner.starts_with('F'),
            rest,
            roll: 0.0,
        });
    }
}

fn drive(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    track: Res<Track>,
    mut reset: MessageWriter<Reset>,
    mut cars: Query<(&mut Transform, &mut Car)>,
) {
    let dt = time.delta_secs().min(MAX_STEP);
    let controls = Controls {
        throttle: axis(&keys, KeyCode::KeyW, KeyCode::ArrowUp),
        // Shift still brakes, for anyone who learned it that way.
        brake: axis(&keys, KeyCode::KeyS, KeyCode::ArrowDown)
            .max(axis(&keys, KeyCode::ShiftLeft, KeyCode::ShiftRight)),
        steer: axis(&keys, KeyCode::KeyA, KeyCode::ArrowLeft)
            - axis(&keys, KeyCode::KeyD, KeyCode::ArrowRight),
        handbrake: keys.pressed(KeyCode::Space),
    };

    if keys.just_pressed(KeyCode::KeyR) {
        reset.write(Reset);
        for (mut transform, mut car) in &mut cars {
            *transform = track.start_transform().with_scale(transform.scale);
            *car = Car::default();
        }
        return;
    }

    for (mut transform, mut car) in &mut cars {
        let heading = level(*transform.forward());
        let right = heading.cross(Vec3::Y);
        let ground = track.ground(transform.translation);
        let surface = Surface {
            grip: ground.grip,
            // Positive where the circuit climbs the way the car is pointing.
            slope: ground.slope * ground.tangent.dot(heading).signum(),
        };
        let yaw = physics::step(&mut car, heading, right, controls, surface, dt);
        transform.rotate_y(yaw);
        transform.translation += car.velocity * dt;
    }
}

/// Point the front wheels where the steering does, and roll all four at the
/// speed the car is actually travelling.
fn turn_wheels(
    time: Res<Time>,
    cars: Query<(Entity, &Transform, &Car)>,
    children: Query<&Children>,
    mut wheels: Query<(&mut Transform, &mut Wheel), Without<Car>>,
) {
    let dt = time.delta_secs();
    for (entity, transform, car) in &cars {
        // Forward is -Z, so a wheel rolling the car forward turns the other way.
        let roll = -car.speed(level(*transform.forward())) / physics::WHEEL_RADIUS * dt;
        for descendant in children.iter_descendants(entity) {
            let Ok((mut transform, mut wheel)) = wheels.get_mut(descendant) else {
                continue;
            };
            wheel.roll += roll;
            let steer = if wheel.steers { car.steer_angle } else { 0.0 };
            transform.rotation =
                wheel.rest * Quat::from_rotation_y(steer) * Quat::from_rotation_x(wheel.roll);
        }
    }
}

/// Flatten a direction into the XZ plane. The car drives on the loft's surface
/// but its own frame stays level, so gravity is the only thing a slope changes.
pub(crate) fn level(direction: Vec3) -> Vec3 {
    Vec3::new(direction.x, 0.0, direction.z).normalize_or(Vec3::NEG_Z)
}

fn axis(keys: &ButtonInput<KeyCode>, a: KeyCode, b: KeyCode) -> f32 {
    if keys.any_pressed([a, b]) { 1.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    //! Can the car get round the circuit? Everything else is a number in
    //! isolation; this is the one that answers whether the thing is driveable.

    use super::*;
    use crate::track::Track;
use crate::Reset;

    /// A plain driver: aim at the centreline, and carry the speed the tyres can
    /// hold through the tightest bend it can see. No racing line, no reflexes —
    /// if this cannot get round, neither can anyone.
    ///
    /// It is a smoke test that the circuit is completable, not a tuning oracle.
    /// Chasing its numbers rewards a slow, dull car, because a faster one gives a
    /// driver this simple more to get wrong.
    fn drive_one_lap(seconds: f32) -> Lap {
        let track = Track::new();
        let mut transform = track.start_transform().with_scale(Vec3::splat(SCALE));
        let mut car = Car::default();
        let dt = 1.0 / 120.0;
        let mut lap = Lap::default();
        let mut travelled = 0.0f32;

        for _ in 0..(seconds / dt) as usize {
            let heading = level(*transform.forward());
            let ground = track.ground(transform.translation);
            // The way the lap runs, not the way the car happens to be pointing:
            // aligning the target to the car lets the driver lap backwards.
            let ahead = ground.tangent;
            // Signed, so facing the wrong way reads as half a turn of error
            // rather than as no error at all.
            let astray = f32::atan2(heading.cross(ahead).y, heading.dot(ahead));
            let correction = astray * 1.6 + ground.lateral * 0.16;
            let hold = 0.95 * 9.81 * ground.grip;
            let mut limit: f32 = 24.0;
            for step in 0..=16 {
                let reach = step as f32 * 3.0;
                let probe = track.ground(transform.translation + ahead * reach);
                let corner = hold / probe.curvature.abs().max(0.002);
                limit = limit.min((corner + 2.0 * hold * reach).sqrt());
            }
            let speed = car.velocity.length();
            let busy = car.g_force.x.abs() > 0.75 || car.rear_slip > 0.2;

            let controls = Controls {
                // Do not drive hard at anything but the road ahead.
                throttle: if speed < limit * 0.96 && !busy && astray.abs() < 0.6 {
                    1.0
                } else {
                    0.0
                },
                brake: if speed > limit * 1.04 { 1.0 } else { 0.0 },
                // Proportional, so this measures the car rather than the
                // driver: an on-off input oscillates harder the more capable the
                // car gets, which grades a better car as worse.
                steer: correction.clamp(-1.0, 1.0),
                handbrake: false,
            };

            let surface = Surface {
                grip: ground.grip,
                slope: ground.slope * ground.tangent.dot(heading).signum(),
            };
            let yaw = physics::step(&mut car, heading, heading.cross(Vec3::Y), controls, surface, dt);
            transform.rotate_y(yaw);
            transform.translation += car.velocity * dt;
            travelled += speed * dt;

            // The same thing the circuit does to the real car, not a stand-in
            // for it: barrier, kerb height and all.
            track.hold(&mut transform, &mut car, dt);
            let ground = track.ground(transform.translation);
            if ground.lateral.abs() > 4.0 {
                lap.off_road += dt;
            }
            if ground.lateral.abs() > 5.5 {
                lap.in_the_weeds += dt;
            }
            if speed < 1.5 {
                lap.stopped += dt;
            }
            lap.distance = travelled;
            lap.progress = lap.progress.max(track.progress(transform.translation));
        }
        lap
    }

    #[derive(Default)]
    struct Lap {
        distance: f32,
        progress: f32,
        off_road: f32,
        in_the_weeds: f32,
        stopped: f32,
    }

    #[test]
    fn a_plain_driver_gets_round() {
        let lap = drive_one_lap(90.0);
        println!(
            "round {:.0}%  {:.0} m  off-road {:.1} s  weeds {:.1} s  stopped {:.1} s",
            lap.progress * 100.0,
            lap.distance,
            lap.off_road,
            lap.in_the_weeds,
            lap.stopped
        );
        assert!(
            lap.progress > 0.9,
            "90 s only got {:.0}% round, {:.0} m",
            lap.progress * 100.0,
            lap.distance
        );
        // Loose on purpose: these are the bounds of "a plain driver can get
        // round", not a target to tune against.
        assert!(
            lap.off_road < 30.0,
            "spent {:.0} s of 90 off the road",
            lap.off_road
        );
        assert!(
            lap.stopped < 15.0,
            "spent {:.0} s of 90 going nowhere",
            lap.stopped
        );
    }
}
