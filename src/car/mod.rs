//! The car as an entity: the glTF, the wheels, the body that leans, and the
//! systems that carry [`Controls`] into [`physics::step`] and the result back
//! onto a [`Transform`].
//!
//! Two entities. The parent carries the [`Car`], its [`Handling`] and its
//! [`Controls`], and is what the physics moves. Its child is the [`Body`]: the
//! model, which rolls and dives with the g the seat feels, and holds the
//! wheels. Nothing here decides how the car behaves — that is [`physics`] — and
//! nothing here decides what the driver wants — that is [`crate::input`], or a
//! [`driver::Driver`].

mod driver;
mod physics;

use bevy::{prelude::*, world_serialization::WorldInstanceReady};

use crate::input::InputSet;
use crate::track::Track;
use crate::Reset;
pub(crate) use physics::{Car, Controls, Handling, Surface, HALF_TRACK, REAR_AXLE, SCALE, WHEEL_WIDTH};

const MODEL: &str = "models/shooting_brake.glb";
/// The engine steps at this rate whatever the frame rate, so the car handles
/// the same at 30 frames a second as at 144. A frame is cut into as many of
/// these as it needs.
const SUBSTEP: f32 = 1.0 / 240.0;
/// A long frame must not let the car tunnel through a corner.
const MAX_STEP: f32 = 1.0 / 30.0;
/// Body roll per lateral g and dive per longitudinal g, in radians, and how
/// quickly the body settles onto its springs.
const ROLL_PER_G: f32 = 0.07;
const DIVE_PER_G: f32 = 0.05;
const LEAN_RATE: f32 = 9.0;

#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct DriveSet;

/// The car the player is driving. [`crate::input`] fills its [`Controls`].
#[derive(Component)]
pub(crate) struct Player;

/// The model, as a child of the car. It leans; the car does not.
#[derive(Component, Default)]
struct Body {
    /// The g the springs have settled onto, which lags what the car is doing.
    lean: Vec2,
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

pub struct CarPlugin;

impl Plugin for CarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, restart.after(InputSet).before(DriveSet))
            .add_systems(Update, drive.in_set(DriveSet).after(InputSet))
            .add_systems(Update, (turn_wheels, lean_body).after(DriveSet));
    }
}

fn setup(mut commands: Commands, track: Res<Track>, asset_server: Res<AssetServer>) {
    commands
        .spawn((
            Car::default(),
            Handling::SHOOTING_BRAKE,
            Controls::default(),
            Player,
            track.start_transform().with_scale(Vec3::splat(SCALE)),
            Visibility::default(),
        ))
        .with_children(|car| {
            car.spawn((
                Body::default(),
                Transform::IDENTITY,
                Visibility::default(),
                WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(MODEL))),
            ))
            .observe(attach_wheels);
        });
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

/// Carry the car forward by `dt`, through the engine, in fixed substeps.
///
/// The one path from a [`Controls`] to a moved car, shared by the game and by
/// the drivers' lap harness, so what the tests lap is what the player drives.
pub(crate) fn advance(
    track: &Track,
    handling: &Handling,
    controls: Controls,
    transform: &mut Transform,
    car: &mut Car,
    dt: f32,
) {
    let mut left = dt.min(MAX_STEP);
    while left > 1e-6 {
        let h = left.min(SUBSTEP);
        let heading = level(*transform.forward());
        let ground = track.ground(transform.translation);
        let surface = Surface {
            grip: ground.grip,
            // The grade runs along the circuit; the car gets the component of it
            // that lies along its nose. Projecting rather than taking the sign
            // matters once the car is sideways: at ninety degrees to the road
            // the sign flips on nothing at all.
            slope: ground.slope * ground.tangent.dot(heading),
        };
        let yaw = physics::step(car, handling, heading, heading.cross(Vec3::Y), controls, surface, h);
        transform.rotate_y(yaw);
        transform.translation += car.velocity * h;
        left -= h;
    }
}

fn drive(
    time: Res<Time>,
    track: Res<Track>,
    mut cars: Query<(&mut Transform, &mut Car, &Handling, &Controls)>,
) {
    for (mut transform, mut car, handling, controls) in &mut cars {
        advance(&track, handling, *controls, &mut transform, &mut car, time.delta_secs());
    }
}

/// Back to the grid, stopped, on a reset.
fn restart(
    mut resets: MessageReader<Reset>,
    track: Res<Track>,
    mut cars: Query<(&mut Transform, &mut Car)>,
) {
    if resets.read().next().is_none() {
        return;
    }
    for (mut transform, mut car) in &mut cars {
        *transform = track.start_transform().with_scale(transform.scale);
        *car = Car::default();
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

/// Sit the body on its springs: it rolls away from a corner and dives under the
/// brakes, lagging the car a little, which is what makes it read as weight.
fn lean_body(
    time: Res<Time>,
    cars: Query<(Entity, &Car)>,
    children: Query<&Children>,
    mut bodies: Query<(&mut Transform, &mut Body)>,
) {
    let settle = (LEAN_RATE * time.delta_secs()).min(1.0);
    for (entity, car) in &cars {
        for descendant in children.iter_descendants(entity) {
            if let Ok((mut transform, mut body)) = bodies.get_mut(descendant) {
                body.lean = body.lean.lerp(car.g_force, settle);
                transform.rotation = lean(body.lean);
            }
        }
    }
}

/// How the body sits under `g`, in the car's frame: x to the right, y forward.
fn lean(g: Vec2) -> Quat {
    Quat::from_rotation_z(g.x * ROLL_PER_G) * Quat::from_rotation_x(g.y * DIVE_PER_G)
}

/// Flatten a direction into the XZ plane. The car drives on the loft's surface
/// but its own frame stays level, so gravity is the only thing a slope changes.
pub(crate) fn level(direction: Vec3) -> Vec3 {
    Vec3::new(direction.x, 0.0, direction.z).normalize_or(Vec3::NEG_Z)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A body leans *out* of a corner and dips its nose under the brakes. Get
    /// either sign wrong and the car looks like it is being pushed rather than
    /// driven.
    #[test]
    fn the_body_leans_the_right_way() {
        // Accelerating to the right (a right-hand corner): the roof goes left.
        let roof = lean(Vec2::new(1.0, 0.0)) * Vec3::Y;
        assert!(roof.x < -0.01, "roof went {roof:?} in a right-hander");
        // Braking: the nose goes down.
        let nose = lean(Vec2::new(0.0, -1.0)) * Vec3::NEG_Z;
        assert!(nose.y < -0.01, "nose went {nose:?} under the brakes");
        assert_eq!(lean(Vec2::ZERO), Quat::IDENTITY);
    }

    /// The engine steps at a fixed rate however the frames come, so a slow
    /// machine and a fast one drive the same car. One long frame and several
    /// short ones adding up to it must land in the same place.
    #[test]
    fn advancing_is_frame_rate_independent() {
        let track = Track::new();
        let handling = Handling::SHOOTING_BRAKE;
        let corner = Controls {
            throttle: 0.7,
            steer: 0.6,
            ..default()
        };
        let start = track.start_transform().with_scale(Vec3::splat(SCALE));
        let (mut slow_t, mut slow_car) = (start, Car::default());
        let (mut fast_t, mut fast_car) = (start, Car::default());
        for _ in 0..90 {
            advance(&track, &handling, corner, &mut slow_t, &mut slow_car, 1.0 / 30.0);
            for _ in 0..4 {
                advance(&track, &handling, corner, &mut fast_t, &mut fast_car, 1.0 / 120.0);
            }
        }
        assert!(
            (slow_car.velocity - fast_car.velocity).length() < 1e-3,
            "30 fps {:?} against 120 fps {:?}",
            slow_car.velocity,
            fast_car.velocity
        );
        assert!(
            slow_t.translation.distance(fast_t.translation) < 1e-2,
            "30 fps at {:?}, 120 fps at {:?}",
            slow_t.translation,
            fast_t.translation
        );
    }
}
