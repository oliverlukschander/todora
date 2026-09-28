//! What the driver rides.
//!
//! The car is a glTF and stays one; a mount is built out of primitives (see
//! [`super::parts`]) and stands in its place. The engine is told nothing about
//! it: [`Car`] still moves, still leans in the [`Body`], and the mount is a
//! second child of the car that follows the same numbers. Whatever it is called,
//! it is the same handling underneath.
//!
//! Animation is one resource and one system. [`Pose`] is every channel the
//! mounts move on — how hard the legs are pumping, where in the stride they
//! are, how far the body is leaning, how high off the road, how squashed —
//! worked out from the car once a frame. Each moving piece of a mount carries a
//! [`Part`] naming what it is, and [`animate`] turns a pose into a transform
//! for it. A new mount is a builder and a few arms of a `match`.

use bevy::prelude::*;

use super::parts::{Kit, Shape, add, add_turned, joint};
use super::{Fun, Hat, Mount};
use crate::car::{Body, Car, Player, SCALE, Spec, level};

mod chicken;
mod pose;
mod hats;
mod rider;
mod things;

/// The root of a mount's model, a child of the car.
#[derive(Component)]
pub(crate) struct Rig(pub Key);

/// What a rig was built from. A rig is rebuilt when any of it changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Key {
    pub mount: Mount,
    pub hat: Hat,
    pub eyes: bool,
    pub paint: [u8; 3],
    /// Changes when the car goes back to the grid, so a surprise is a new one.
    pub round: u32,
}

/// Which piece of a mount a node is, so [`animate`] knows how to move it.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(crate) enum Part {
    /// The whole chicken's neck, from its base.
    Neck,
    /// The head at the end of the neck.
    Head,
    /// The lower half of the beak, hinged.
    Jaw,
    Comb,
    Wattle,
    /// A googly pupil; `side` is -1 for the left eye and 1 for the right.
    Pupil(f32),
    /// A wing at its shoulder, `side` as above.
    Wing(f32),
    /// The leg at the hip, then the shin at the knee, then the foot at the ankle.
    Hip(f32, f32),
    Knee(f32, f32),
    Ankle(f32, f32),
    /// One of the tail feathers, by its place in the fan.
    Feather(u8),
    /// The whole tail, at its base.
    Tail,
    /// The rider from the hips up.
    Rider,
    /// The rider's head and helmet, which lag the torso.
    RiderHead,
    /// A hat, on its springs.
    Hat,
    /// A propeller: turns faster the faster it goes.
    Spin(f32),
    /// A road wheel of a wheeled mount: rolls with the speed, and steers if the
    /// second number is 1.
    Wheel(f32),
    /// The whole mount body, for things with no legs: bounces with the road.
    Chassis,
    /// A duck's or a tub's water: wobbles.
    Slosh,
    /// A section of the scarf, by its place along it.
    Scarf(u8),
}

/// The pose a part rests in, which the animation is applied to.
#[derive(Component, Clone, Copy)]
pub(crate) struct Rest(pub Transform);

/// A little secondary motion: a position and velocity that chase a target.
#[derive(Component, Default, Clone, Copy)]
pub(crate) struct Spring {
    pub pos: Vec3,
    pub vel: Vec3,
}

impl Spring {
    /// Semi-implicit Euler, so a stiff spring does not blow up on a long frame.
    pub(crate) fn chase(&mut self, target: Vec3, stiffness: f32, damping: f32, dt: f32) {
        let dt = dt.min(1.0 / 30.0);
        self.vel += ((target - self.pos) * stiffness - self.vel * damping) * dt;
        self.pos += self.vel * dt;
    }
}

/// Every channel the mounts move on.
#[derive(Resource, Clone, Debug)]
pub(crate) struct Pose {
    pub time: f32,
    /// Where in the stride the legs are, in radians.
    pub stride: f32,
    /// How hard the legs are pumping, 0 standing to 1 flat out.
    pub run: f32,
    /// Speed along the nose in metres a second; negative in reverse.
    pub speed: f32,
    /// Smoothed g in the car's frame: x to the right, y forward.
    pub lean: Vec2,
    /// The front wheels' angle.
    pub steer: f32,
    /// How far a road wheel has rolled, in radians.
    pub wheel: f32,
    /// Height off the road in game units, and how fast it is changing.
    pub lift: f32,
    pub rise: f32,
    /// Squash on landing: positive squashed, negative stretched.
    pub squash: f32,
    squash_speed: f32,
    /// How far the wings are spread, and where in the beat they are.
    pub flap: f32,
    pub flap_phase: f32,
    /// How wide the beak is open, decaying after a honk.
    pub beak: f32,
    /// A honk, for the eyes to bulge at.
    pub honk: f32,
    /// How hard a boost is pushing.
    pub boost: f32,
    /// A head-dip while standing about, 0 to 1.
    pub peck: f32,
    /// The wind on the rider, in the rig's frame, for the scarf.
    pub wind: Vec3,
    /// Which way the road ahead turns, from what the steering says.
    pub turn: f32,
    was_airborne: bool,
}

impl Default for Pose {
    fn default() -> Self {
        Self {
            time: 0.0,
            stride: 0.0,
            run: 0.0,
            speed: 0.0,
            lean: Vec2::ZERO,
            steer: 0.0,
            wheel: 0.0,
            lift: 0.0,
            rise: 0.0,
            squash: 0.0,
            squash_speed: 0.0,
            flap: 0.0,
            flap_phase: 0.0,
            beak: 0.0,
            honk: 0.0,
            boost: 0.0,
            peck: 0.0,
            wind: Vec3::ZERO,
            turn: 0.0,
            was_airborne: false,
        }
    }
}

/// How far a stride goes, in game units per cycle, before it is capped.
const STRIDE: f32 = 1.5;
/// Cycles a second no leg goes faster than: past this a frame is more than a
/// tenth of a cycle and the legs would strobe instead of run.
const MOST_HZ: f32 = 6.0;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Pose>()
        .init_resource::<Round>()
        .add_message::<Honk>()
        .add_systems(
            Update,
            (
                rebuild,
                show_the_right_body,
                update_pose,
                animate_rig,
                animate,
            )
                .chain()
                .after(crate::car::LeanSet),
        )
        .add_systems(PreUpdate, new_round.after(crate::car::CarResetSet));
}

/// Somebody leaned on the horn.
#[derive(Message, Clone, Copy)]
pub(crate) struct Honk;

/// Counts the times the car has gone back to the grid.
#[derive(Resource, Default)]
struct Round(u32);

fn new_round(mut resets: MessageReader<crate::Reset>, mut round: ResMut<Round>) {
    if resets.read().next().is_some() {
        round.0 = round.0.wrapping_add(1);
    }
}

/// The colour the car is painted, as bytes, so it can be compared and kept.
fn paint_bytes(spec: &Spec) -> [u8; 3] {
    let c = spec.sheet().paint.to_srgba();
    [
        (c.red * 255.0) as u8,
        (c.green * 255.0) as u8,
        (c.blue * 255.0) as u8,
    ]
}

/// Build the mount the settings ask for, and take the last one away.
#[allow(clippy::too_many_arguments)]
fn rebuild(
    mut commands: Commands,
    fun: Res<Fun>,
    spec: Res<Spec>,
    round: Res<Round>,
    kit: Option<ResMut<Kit>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    cars: Query<Entity, With<Player>>,
    rigs: Query<(Entity, &Rig)>,
) {
    let Some(mut kit) = kit else {
        return;
    };
    let Ok(car) = cars.single() else {
        return;
    };
    let hat = match fun.hat {
        Hat::Surprise => {
            // A hat by the round, the same on every frame of it.
            *super::rng::Rng::new(u64::from(round.0) ^ 0x4841_5453).pick(&Hat::REAL)
        }
        other => other,
    };
    let wanted = Key {
        mount: fun.mount,
        hat,
        eyes: fun.eyes,
        paint: paint_bytes(&spec),
        round: if fun.hat == Hat::Surprise { round.0 } else { 0 },
    };
    let existing = rigs.iter().next();
    if existing.is_some_and(|(_, rig)| rig.0 == wanted) {
        return;
    }
    if let Some((old, _)) = existing {
        commands.entity(old).despawn();
    }
    if wanted.mount == Mount::Car {
        return;
    }
    let look = Look {
        paint: spec.sheet().paint,
        eyes: wanted.eyes,
        hat: wanted.hat,
    };
    commands.entity(car).with_children(|car| {
        let mut rig = car.spawn((
            Rig(wanted),
            Transform::IDENTITY,
            Visibility::Inherited,
        ));
        rig.with_children(|rig| match wanted.mount {
            Mount::Chicken => chicken::build(rig, &mut kit, &mut materials, &look),
            Mount::Duck => things::duck(rig, &mut kit, &mut materials, &look),
            Mount::Tub => things::tub(rig, &mut kit, &mut materials, &look),
            Mount::Cart => things::cart(rig, &mut kit, &mut materials, &look),
            Mount::Car => {}
        });
    });
}

/// What a mount is dressed as.
pub(crate) struct Look {
    pub paint: Color,
    pub eyes: bool,
    pub hat: Hat,
}

/// The glTF car shows when nothing stands in for it.
fn show_the_right_body(fun: Res<Fun>, mut bodies: Query<&mut Visibility, With<Body>>) {
    let wanted = if fun.mount == Mount::Car {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut visibility in &mut bodies {
        visibility.set_if_neq(wanted);
    }
}

/// Work out every channel from the car.
fn update_pose(
    time: Res<Time>,
    mut pose: ResMut<Pose>,
    mut honks: MessageReader<Honk>,
    cars: Query<(&Car, &Transform, Option<&super::air::Air>), With<Player>>,
    boost: Option<Res<super::tweak::Boost>>,
) {
    let dt = time.delta_secs();
    if honks.read().next().is_some() {
        pose.honk = 1.0;
        pose.beak = 1.0;
    }
    if dt <= 0.0 {
        return;
    }
    let Ok((car, at, air)) = cars.single() else {
        return;
    };
    pose.time += dt;
    let heading = level(*at.forward());
    let speed = car.speed(heading);
    pose.speed = speed;
    let glide = |from: f32, to: f32, rate: f32| from + (to - from) * (1.0 - (-rate * dt).exp());

    let moving = ((speed.abs() - 0.4) / 2.5).clamp(0.0, 1.0);
    pose.run = glide(pose.run, moving, 9.0);
    let hz = (speed.abs() / STRIDE).min(MOST_HZ);
    pose.stride =
        (pose.stride + hz * std::f32::consts::TAU * dt * speed.signum()).rem_euclid(std::f32::consts::TAU);
    pose.lean = pose.lean.lerp(car.g_force, (9.0 * dt).min(1.0));
    pose.steer = car.steer_angle;
    pose.wheel = (pose.wheel - speed / crate::car::WHEEL_RADIUS * dt).rem_euclid(std::f32::consts::TAU);
    pose.turn = glide(pose.turn, car.yaw_rate, 8.0);

    let (lift, airborne) = air.map_or((0.0, false), |a| (a.height, a.height > 0.001));
    pose.rise = (lift - pose.lift) / dt;
    pose.lift = lift;
    // Landing: a squash, delivered as a kick to a spring.
    if pose.was_airborne && !airborne {
        let impact = air.map_or(0.0, |a| a.landed).clamp(0.0, 14.0);
        pose.squash_speed += impact * 3.2;
    }
    pose.was_airborne = airborne;
    // Leaving the ground stretches; the spring settles it either way.
    let k = 260.0;
    let c = 16.0;
    let step = dt.min(1.0 / 30.0);
    pose.squash_speed += (-pose.squash * k - pose.squash_speed * c) * step;
    pose.squash = (pose.squash + pose.squash_speed * step).clamp(-0.6, 0.8);

    let boosting = boost.map_or(0.0, |b| b.strength());
    pose.boost = glide(pose.boost, boosting, 10.0);
    // Wings: spread in the air and under a boost, flapping harder the faster.
    let want = if airborne { 1.0 } else { 0.0f32 }.max(pose.boost);
    pose.flap = glide(pose.flap, want, 12.0);
    let beat = 9.0 + 9.0 * pose.flap;
    pose.flap_phase = (pose.flap_phase + beat * dt).rem_euclid(std::f32::consts::TAU);
    pose.honk = (pose.honk - dt * 2.6).max(0.0);
    pose.beak = (pose.beak - dt * 3.2).max(0.0);

    // An idle head-dip every few seconds while stood about.
    let t = pose.time;
    let cycle = (t * 0.31 + (t * 0.83).sin() * 0.2).rem_euclid(1.0);
    let dip = if cycle < 0.14 {
        (cycle / 0.14 * std::f32::consts::PI).sin()
    } else {
        0.0
    };
    pose.peck = glide(pose.peck, dip * (1.0 - pose.run), 25.0);

    // The rider's wind: from ahead, in the rig's own frame, plus what the
    // corner is throwing sideways.
    pose.wind = Vec3::new(pose.lean.x * 0.6, 0.15, speed.clamp(-30.0, 30.0) / 24.0);
}

/// The lean, lift, squash and bob of the whole mount.
fn animate_rig(pose: Res<Pose>, mut rigs: Query<&mut Transform, With<Rig>>) {
    let roll = Quat::from_rotation_z(pose.lean.x * 0.11);
    let dive = Quat::from_rotation_x(pose.lean.y * 0.08);
    let step = (2.0 * pose.stride).sin().abs() * pose.run;
    let bob = 0.05 * step;
    let squash = pose.squash;
    let scale = Vec3::new(1.0 + squash * 0.24, 1.0 - squash * 0.42, 1.0 + squash * 0.24);
    for mut transform in &mut rigs {
        transform.translation = Vec3::new(0.0, pose.lift / SCALE + bob, 0.0);
        transform.rotation = roll * dive;
        transform.scale = scale;
    }
}

/// Move every part to where the pose puts it.
fn animate(
    time: Res<Time>,
    pose: Res<Pose>,
    mut parts: Query<(&mut Transform, &Rest, &Part, Option<&mut Spring>)>,
) {
    let dt = time.delta_secs();
    for (mut transform, rest, part, spring) in &mut parts {
        let moved = pose::part(*part, &rest.0, &pose, spring, dt);
        if *transform != moved {
            *transform = moved;
        }
    }
}

/// Spawn a part that moves: its rest pose is remembered.
pub(crate) fn animated<'a>(
    parent: &'a mut ChildSpawnerCommands,
    kit: &Kit,
    shape: Shape,
    paint: &Handle<StandardMaterial>,
    at: Vec3,
    size: Vec3,
    part: Part,
) -> EntityCommands<'a> {
    let mut entity = add(parent, kit, shape, paint, at, size);
    entity.insert((part, Rest(Transform::from_translation(at).with_scale(size))));
    entity
}

/// A moving joint: an empty node with a part on it.
pub(crate) fn hinge<'a>(
    parent: &'a mut ChildSpawnerCommands,
    at: Vec3,
    part: Part,
) -> EntityCommands<'a> {
    let mut entity = joint(parent, at, Quat::IDENTITY);
    entity.insert((part, Rest(Transform::from_translation(at))));
    entity
}

/// A limb: a capsule laid between two points.
pub(crate) fn limb<'a>(
    parent: &'a mut ChildSpawnerCommands,
    kit: &Kit,
    paint: &Handle<StandardMaterial>,
    from: Vec3,
    to: Vec3,
    width: f32,
) -> EntityCommands<'a> {
    let along = to - from;
    let length = along.length().max(1e-4);
    add_turned(
        parent,
        kit,
        Shape::Capsule,
        paint,
        from.midpoint(to),
        // A capsule is a unit long between its caps and half a unit of cap at
        // either end, so it is the straight part that is shortened.
        Vec3::new(width, (length - width).max(0.01), width),
        Quat::from_rotation_arc(Vec3::Y, along / length),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spring_settles_on_its_target_without_blowing_up() {
        let mut spring = Spring::default();
        for _ in 0..600 {
            spring.chase(Vec3::new(1.0, -2.0, 0.5), 120.0, 9.0, 1.0 / 30.0);
        }
        assert!(spring.pos.distance(Vec3::new(1.0, -2.0, 0.5)) < 1e-3);
        // A long frame does not throw it across the room.
        let mut spring = Spring::default();
        spring.chase(Vec3::X, 400.0, 6.0, 5.0);
        assert!(spring.pos.length() < 10.0 && spring.vel.length() < 100.0);
    }

    #[test]
    fn a_limb_reaches_from_one_point_to_the_other() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>();
        let kit = Kit::new(&mut app.world_mut().resource_mut::<Assets<Mesh>>());
        let mut materials = Assets::<StandardMaterial>::default();
        let mut kit = kit;
        let paint = kit.paint(&mut materials, Color::WHITE);
        let world = app.world_mut();
        let mut commands = world.commands();
        let (from, to) = (Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 2.0, -1.0));
        let mut reached = Vec3::ZERO;
        commands.spawn_empty().with_children(|parent| {
            let entity = limb(parent, &kit, &paint, from, to, 0.2).id();
            reached = Vec3::splat(entity.index_u32() as f32);
        });
        let _ = reached;
        world.flush();
        let mut found = world.query::<(&Transform, &Mesh3d)>();
        let (transform, _) = found.iter(world).next().expect("the limb");
        assert!(transform.translation.distance(from.midpoint(to)) < 1e-5);
        let axis = transform.rotation * Vec3::Y;
        assert!(axis.distance((to - from).normalize()) < 1e-4);
    }

    #[test]
    fn the_stride_is_capped_below_the_strobing_rate() {
        let fast = (300.0f32 / STRIDE).min(MOST_HZ);
        assert_eq!(fast, MOST_HZ);
        // At sixty frames a second a cycle is at least ten frames long.
        assert!(60.0 / MOST_HZ >= 10.0);
    }
}
