//! The car: the glTF, the wheels, and the keys that reach [`physics`].
//!
//! This module only turns input and the road under the wheels into a [`Controls`]
//! and a [`Surface`], hands both to one `step`, and puts the result back on the
//! transform. Nothing here decides how the car behaves.

mod physics;

use bevy::{prelude::*, world_serialization::WorldInstanceReady};

use crate::track::Track;
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

    let reset = keys.just_pressed(KeyCode::KeyR);

    for (mut transform, mut car) in &mut cars {
        if reset {
            track.rescue(&mut transform, &mut car);
            continue;
        }
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
