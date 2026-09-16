use bevy::{prelude::*, world_serialization::WorldInstanceReady};

use crate::track::Track;

const ACCEL: f32 = 22.0;
const BRAKE: f32 = 28.0;
const COAST: f32 = 10.0;
const MAX_SPEED: f32 = 24.0;
const MAX_REVERSE: f32 = 8.0;
const STEER_RATE: f32 = 2.4;
const MODEL: &str = "models/shooting_brake.glb";
/// Matches `WHEEL_R` in `tools/make_shooting_brake.py`.
const WHEEL_RADIUS: f32 = 0.20;

#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct DriveSet;

pub struct CarPlugin;

impl Plugin for CarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, drive.in_set(DriveSet))
            .add_systems(Update, spin_wheels.after(DriveSet));
    }
}

#[derive(Component, Default)]
pub(crate) struct Car {
    pub(crate) speed: f32,
}

#[derive(Component)]
struct Wheel;

fn setup(mut commands: Commands, track: Res<Track>, asset_server: Res<AssetServer>) {
    commands
        .spawn((
            Car::default(),
            track.start_transform(),
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
) {
    for entity in children.iter_descendants(ready.entity) {
        let Ok(name) = names.get(entity) else {
            continue;
        };
        if name.as_str().starts_with("Wheel") {
            commands.entity(entity).insert(Wheel);
        }
    }
}

fn drive(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cars: Query<(&mut Transform, &mut Car)>,
) {
    let dt = time.delta_secs();
    let throttle = axis(&keys, KeyCode::KeyW, KeyCode::ArrowUp)
        - axis(&keys, KeyCode::KeyS, KeyCode::ArrowDown);
    let steer = axis(&keys, KeyCode::KeyA, KeyCode::ArrowLeft)
        - axis(&keys, KeyCode::KeyD, KeyCode::ArrowRight);
    let braking = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    for (mut transform, mut car) in &mut cars {
        if braking {
            car.speed = move_towards(car.speed, 0.0, BRAKE * dt);
        } else if throttle != 0.0 {
            car.speed += ACCEL * throttle * dt;
        } else {
            car.speed = move_towards(car.speed, 0.0, COAST * dt);
        }
        car.speed = car.speed.clamp(-MAX_REVERSE, MAX_SPEED);

        let steer_scale = (car.speed.abs() / MAX_SPEED).clamp(0.15, 1.0);
        transform.rotate_y(steer * STEER_RATE * steer_scale * dt);

        let forward = transform.forward();
        let fwd = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
        transform.translation += fwd * car.speed * dt;
    }
}

fn spin_wheels(
    time: Res<Time>,
    cars: Query<(Entity, &Car)>,
    children: Query<&Children>,
    mut wheels: Query<&mut Transform, With<Wheel>>,
) {
    let dt = time.delta_secs();
    for (entity, car) in &cars {
        // Forward is -Z; right-hand rotation around +X would move the contact
        // patch forward, so roll is the opposite sign of speed.
        let roll = -car.speed / WHEEL_RADIUS * dt;
        if roll.abs() <= f32::EPSILON {
            continue;
        }
        for descendant in children.iter_descendants(entity) {
            if let Ok(mut transform) = wheels.get_mut(descendant) {
                transform.rotate_local_x(roll);
            }
        }
    }
}

fn axis(keys: &ButtonInput<KeyCode>, a: KeyCode, b: KeyCode) -> f32 {
    if keys.pressed(a) || keys.pressed(b) {
        1.0
    } else {
        0.0
    }
}

fn move_towards(value: f32, target: f32, max_delta: f32) -> f32 {
    let delta = target - value;
    if delta.abs() <= max_delta {
        target
    } else {
        value + delta.signum() * max_delta
    }
}
