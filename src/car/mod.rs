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

#[cfg(test)]
mod determinism;
mod driver;
mod garage;
mod mode;
mod physics;
mod setup;

use bevy::{gltf::GltfMaterialName, prelude::*, world_serialization::WorldInstanceReady};

use crate::Reset;
use crate::input::InputSet;
use crate::menu::MenuSet;
use crate::track::{Track, TrackSet};
pub(crate) use garage::Spec;
pub(crate) use mode::Mode;
pub(crate) use physics::{
    Car, Controls, FRONT_AXLE, HALF_TRACK, Handling, PHYSICS_VERSION, REAR_AXLE, SCALE, Surface,
    WHEEL_WIDTH,
};
pub(crate) use setup::Setup;

pub(crate) const MODEL: &str = "models/omarchy_gt_95.glb";
/// The engine steps at this rate whatever the frame rate, so the car handles
/// the same at 30 frames a second as at 144. Bevy carries leftover frame time
/// into the next frame instead of using a shorter final step.
const SUBSTEP: f32 = 1.0 / 240.0;
/// Physics steps a second.
pub(crate) const STEP_HZ: f64 = 240.0;

/// One physics step's `dt`, to the bit, as `Time<Fixed>` hands it to the
/// systems: a duration first, then seconds. A replay that used `1.0 / 240.0`
/// instead could differ in the last bit and time the lap differently.
pub(crate) fn step_seconds() -> f32 {
    std::time::Duration::from_secs_f64(1.0 / STEP_HZ).as_secs_f32()
}
/// Body roll per lateral g and dive per longitudinal g, in radians, and how
/// quickly the body settles onto its springs.
const ROLL_PER_G: f32 = 0.07;
const DIVE_PER_G: f32 = 0.05;
const LEAN_RATE: f32 = 9.0;

#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct DriveSet;
#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct CarResetSet;

/// The car the player is driving. [`crate::input`] fills its [`Controls`].
#[derive(Component)]
pub(crate) struct Player;

/// The model, as a child of the car. It leans; the car does not.
#[derive(Component, Default)]
struct Body {
    /// The g the springs have settled onto, which lags what the car is doing.
    lean: Vec2,
}

/// A body panel of the glTF, and the material it came with. The finish — the
/// metal flake, how it takes a highlight — is the model's; only the colour is
/// the car's, so a repaint starts from what the model shipped rather than from
/// numbers copied out of the Blender script.
#[derive(Component)]
struct Bodywork(Handle<StandardMaterial>);

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
        app.init_resource::<Setup>()
            .init_resource::<Spec>()
            .init_resource::<Mode>()
            .insert_resource(Time::<Fixed>::from_hz(STEP_HZ))
            .add_systems(Startup, setup)
            // After the input, which is what asks for both; after the menus,
            // which are what choose the car; and after the track, because a
            // switch writes the reset that puts the car back and the grid it
            // goes back to is the new circuit's. All said outright: an app built
            // without one of those plugins still has to run these in the right
            // place.
            .add_systems(
                PreUpdate,
                (tune, restart)
                    .in_set(CarResetSet)
                    .after(InputSet)
                    .after(MenuSet)
                    .after(TrackSet),
            )
            .add_systems(FixedUpdate, drive.in_set(DriveSet))
            .add_systems(Update, (turn_wheels, lean_body, repaint));
    }
}

fn setup(
    mut commands: Commands,
    track: Res<Track>,
    chosen: Res<Setup>,
    spec: Res<Spec>,
    mode: Res<Mode>,
    asset_server: Res<AssetServer>,
) {
    commands
        .spawn((
            Car {
                // Put down on the grid knowing where the grid is, so that the
                // very first lookup is answered by continuity like every one
                // after it rather than by guessing from height.
                along: Some(track.start_along_lap()),
                ..Car::default()
            },
            mode.applied_to(chosen.applied_to(spec.handling())),
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
            .observe(attach_wheels)
            .observe(find_the_bodywork);
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
        // `make_omarchy_gt.py` names the hubs WheelFL, WheelFR, WheelRL, WheelRR.
        let Some(corner) = name.as_str().strip_prefix("Wheel") else {
            continue;
        };
        let rest = transforms
            .get(entity)
            .map(|t| t.rotation)
            .unwrap_or_default();
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
    let controls = controls.quantised();
    let mut left = dt;
    while left > 1e-6 {
        let h = left.min(SUBSTEP);
        let heading = level(*transform.forward());
        // The deck the car is on, not the nearest road in plan: over a bridge
        // those are two different surfaces with two different slopes, and this
        // is the one the engine is given.
        let ground = track.ground_from(transform.translation, car.along);
        let surface = Surface {
            grip: ground.grip,
            // The grade runs along the circuit; the car gets the component of it
            // that lies along its nose. Projecting rather than taking the sign
            // matters once the car is sideways: at ninety degrees to the road
            // the sign flips on nothing at all.
            slope: ground.slope * ground.tangent.dot(heading),
        };
        let yaw = physics::step(
            car,
            handling,
            heading,
            heading.cross(Vec3::Y),
            controls,
            surface,
            h,
        );
        transform.rotation = physics::turn(yaw) * transform.rotation;
        transform.translation += car.velocity * h;
        track.hold(transform, car, h);
        left -= h;
    }
}

fn drive(
    time: Res<Time>,
    track: Res<Track>,
    mut cars: Query<
        (&mut Transform, &mut Car, &Handling, &Controls),
        Without<crate::countdown::Held>,
    >,
) {
    for (mut transform, mut car, handling, controls) in &mut cars {
        advance(
            &track,
            handling,
            *controls,
            &mut transform,
            &mut car,
            time.delta_secs(),
        );
    }
}

/// The car that is being driven, leaned the way the slider says.
///
/// The car, handling setup and speed mode survive restarts and apply together.
fn tune(
    chosen: Res<Setup>,
    spec: Res<Spec>,
    mode: Res<Mode>,
    mut cars: Query<&mut Handling, With<Player>>,
) {
    if !chosen.is_changed() && !spec.is_changed() && !mode.is_changed() {
        return;
    }
    for mut handling in &mut cars {
        *handling = mode.applied_to(chosen.applied_to(spec.handling()));
    }
}

/// Which parts of the model are painted, taken once as the glTF arrives. The
/// name comes from the material in `make_omarchy_gt.py`, so the model says
/// what its own bodywork is rather than this guessing from a colour.
fn find_the_bodywork(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    painted: Query<(&GltfMaterialName, &MeshMaterial3d<StandardMaterial>)>,
) {
    for entity in children.iter_descendants(ready.entity) {
        if let Ok((name, material)) = painted.get(entity)
            && name.0 == "Paint"
        {
            commands.entity(entity).insert(Bodywork(material.0.clone()));
        }
    }
}

/// Put the chosen car's colour on the body panels.
///
/// The three cars are the same model, so the colour is the whole of what tells
/// them apart from the outside. It is laid over the model's own paint material
/// rather than a fresh one, so the flake and the highlight are the same on all
/// three and only the colour under them moves.
fn repaint(
    spec: Res<Spec>,
    arrived: Query<(), Added<Bodywork>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut panels: Query<(&Bodywork, &mut MeshMaterial3d<StandardMaterial>)>,
) {
    if !spec.is_changed() && arrived.is_empty() {
        return;
    }
    let paint = spec.sheet().paint;
    for (bodywork, mut material) in &mut panels {
        let Some(shipped) = materials.get(&bodywork.0) else {
            continue;
        };
        let mut repainted = shipped.clone();
        repainted.base_color = paint;
        material.0 = materials.add(repainted);
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
        *car = Car {
            along: Some(track.start_along_lap()),
            ..Car::default()
        };
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
            wheel.roll = (wheel.roll + roll).rem_euclid(std::f32::consts::TAU);
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

/// The plain AI driver, as a function from the car's situation to what it
/// asks for.
#[cfg(test)]
pub(crate) fn ai_driver() -> impl FnMut(&Track, &Handling, &Transform, &Car) -> Controls {
    let mut driver = driver::Driver::new(driver::Style::Plain);
    move |track, handling, at, car| driver.decide(track, handling, at, car)
}

/// One timed lap by the plain AI driver in the default car, balanced, in
/// `mode`: what the provisional medal times are made from.
#[cfg(test)]
pub(crate) fn ai_lap_time(track: &Track, mode: Mode) -> Option<f32> {
    let handling = mode.applied_to(Setup::Balanced.applied_to(Spec::Tourer.handling()));
    driver::tests::lap_time(track, driver::Style::Plain, handling)
}

/// Flatten a direction into the XZ plane. The car drives on the loft's surface
/// but its own frame stays level, so gravity is the only thing a slope changes.
pub(crate) fn level(direction: Vec3) -> Vec3 {
    Vec3::new(direction.x, 0.0, direction.z).normalize_or(Vec3::NEG_Z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn simulation() -> (App, Entity) {
        let track = Track::any();
        let start = track.start_transform();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<Reset>()
            .insert_resource(track)
            .init_resource::<ButtonInput<KeyCode>>()
            // What the body panels are repainted through, without a renderer.
            .init_resource::<Assets<StandardMaterial>>()
            .add_plugins((
                crate::pause::PausePlugin,
                crate::input::InputPlugin,
                CarPlugin,
                crate::lap::LapPlugin,
            ));
        // Exercise the production schedules without spawning the rendered model.
        app.world_mut().resource_mut::<Schedules>().remove(Startup);
        let car = app
            .world_mut()
            .spawn((
                Player,
                Car::default(),
                Controls::default(),
                Handling::SHOOTING_BRAKE,
                start,
            ))
            .id();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
        app.update();
        (app, car)
    }

    #[test]
    fn driving_and_the_lap_clock_share_fixed_time_at_all_frame_rates() {
        let run = |frames: &[u64]| {
            let (mut app, car) = simulation();
            for &nanos in frames {
                app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_nanos(
                    nanos,
                )));
                app.update();
            }
            let position = app.world().get::<Transform>(car).unwrap().translation;
            let velocity = app.world().get::<Car>(car).unwrap().velocity;
            (
                position,
                velocity,
                app.world().resource::<crate::lap::LapTimer>().current,
            )
        };
        // Equal elapsed time, including a frame stall and rates that do not
        // divide 240 Hz. Remainders must carry across frame boundaries.
        let reference = run(&[10_000_000; 200]);
        for frames in [
            vec![100_000_000; 20],
            vec![20_000_000; 100],
            [vec![7_000_000; 250], vec![250_000_000]].concat(),
        ] {
            let actual = run(&frames);
            assert!(reference.0.distance(actual.0) < 1e-4);
            assert!((reference.1 - actual.1).length() < 1e-4);
            assert_eq!(reference.2, actual.2);
        }
    }

    #[test]
    fn setup_and_restart_apply_before_the_next_physics_step() {
        let (mut app, entity) = simulation();
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            20,
        )));
        for _ in 0..20 {
            app.update();
        }
        assert!(app.world().get::<Car>(entity).unwrap().velocity.length() > 1.0);
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(KeyCode::KeyW);
        keys.press(KeyCode::Digit3);
        keys.press(KeyCode::KeyR);
        app.update();
        assert_eq!(
            *app.world().get::<Handling>(entity).unwrap(),
            Setup::Oversteer.applied_to(Handling::SHOOTING_BRAKE)
        );
        assert_eq!(app.world().get::<Car>(entity).unwrap().velocity, Vec3::ZERO);
        assert_eq!(app.world().resource::<crate::lap::LapTimer>().current, 0.0);
        assert_eq!(app.world().resource::<crate::lap::LapTimer>().completed, 0);
    }

    #[test]
    fn speed_mode_applies_before_driving_and_survives_a_reset() {
        let (mut app, entity) = simulation();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release_all();
        app.insert_resource(Mode::Beginner);
        app.world_mut().write_message(Reset);
        app.update();
        assert_eq!(
            *app.world().get::<Handling>(entity).unwrap(),
            Mode::Beginner.applied_to(Handling::SHOOTING_BRAKE)
        );
        app.world_mut().write_message(Reset);
        app.update();
        assert_eq!(*app.world().resource::<Mode>(), Mode::Beginner);
        assert_eq!(app.world().get::<Car>(entity).unwrap().velocity, Vec3::ZERO);
    }

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

    #[test]
    fn a_long_frame_preserves_driving_time() {
        let track = Track::any();
        let controls = Controls {
            throttle: 1.0,
            ..default()
        };
        let start = track.start_transform();
        let (mut a, mut ca) = (start, Car::default());
        let (mut b, mut cb) = (start, Car::default());
        advance(
            &track,
            &Handling::SHOOTING_BRAKE,
            controls,
            &mut a,
            &mut ca,
            0.1,
        );
        for _ in 0..24 {
            advance(
                &track,
                &Handling::SHOOTING_BRAKE,
                controls,
                &mut b,
                &mut cb,
                SUBSTEP,
            );
        }
        assert!(
            a.translation.distance(b.translation) < 1e-4,
            "long frame lost driving time"
        );
        assert!((ca.velocity - cb.velocity).length() < 1e-4);
    }

    /// The engine steps at a fixed rate however the frames come, so a slow
    /// machine and a fast one drive the same car. One long frame and several
    /// short ones adding up to it must land in the same place.
    #[test]
    fn advancing_is_frame_rate_independent() {
        let track = Track::any();
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
            advance(
                &track,
                &handling,
                corner,
                &mut slow_t,
                &mut slow_car,
                1.0 / 30.0,
            );
            for _ in 0..4 {
                advance(
                    &track,
                    &handling,
                    corner,
                    &mut fast_t,
                    &mut fast_car,
                    1.0 / 120.0,
                );
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
