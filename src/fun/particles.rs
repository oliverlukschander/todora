//! Things that fly about.
//!
//! Confetti, cartoon puffs, sparks, flames, sparkles, feathers, debris and the
//! streaks that go past at speed. Every one is a small entity built from the
//! kit's shared meshes and paints, moved by one system and gone when its time is
//! up, and a hard cap on how many there can be keeps a big party from costing a
//! frame. They are asked for with a [`Burst`], which says what, where, which
//! way and how many, and the spawning system does the rest.
//!
//! Nothing here touches the drive: the particles are all show.
//!
//! The emitters are here too: tyre smoke from a sliding tail, flames from the
//! exhaust under a boost and a backfire, dust on a landing, sparks off a big
//! thump, the streaks of light that fly by when the car is going fast, and
//! confetti and fireworks when a lap is finished.

use bevy::{light::NotShadowCaster, prelude::*};

use super::Fun;
use super::air::Landed;
use super::mount::Honk;
use super::parts::{Kit, PARTY, Shape, rainbow};
use super::rng::Rng;
use super::tweak::Boost;
use crate::car::{Car, Controls, Player, SCALE, level};
use crate::lap::LapFinished;
use crate::track::Track;

/// The most particles alive at once.
const CAP: usize = 900;

/// What a particle is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Confetti,
    Puff,
    Spark,
    Flame,
    Star,
    Feather,
    Chunk,
    Streak,
    Rocket,
}

/// What colours a burst is made in.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Tint {
    Party,
    Fire,
    Ice,
    Gold,
    White,
    Smoke,
    Rainbow(f32),
    One(Color),
}

/// A request for a handful of particles.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct Burst {
    pub kind: Kind,
    pub at: Vec3,
    /// The way they mostly go, and how far from that they scatter, 0 to 1.
    pub toward: Vec3,
    pub spread: f32,
    pub speed: f32,
    pub count: u32,
    pub size: f32,
    pub tint: Tint,
}

impl Burst {
    pub(crate) fn new(kind: Kind, at: Vec3, count: u32) -> Self {
        Self {
            kind,
            at,
            toward: Vec3::Y,
            spread: 1.0,
            speed: 3.0,
            count,
            size: 0.1,
            tint: Tint::Party,
        }
    }
    pub(crate) fn toward(mut self, dir: Vec3, spread: f32) -> Self {
        self.toward = dir.normalize_or(Vec3::Y);
        self.spread = spread;
        self
    }
    pub(crate) fn speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }
    pub(crate) fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
    pub(crate) fn tint(mut self, tint: Tint) -> Self {
        self.tint = tint;
        self
    }
}

/// One particle.
#[derive(Component)]
struct Bit {
    kind: Kind,
    vel: Vec3,
    spin: Vec3,
    life: f32,
    max: f32,
    gravity: f32,
    drag: f32,
    size: f32,
    /// The lowest it may fall to, from where it started.
    floor: f32,
    /// Fireworks: what to burst into at the top.
    burst: Option<Tint>,
}

/// How many are alive, so a burst can be trimmed to fit.
#[derive(Resource, Default)]
struct Alive(usize);

pub(super) fn plugin(app: &mut App) {
    app.add_message::<Burst>()
        .init_resource::<Alive>()
        .add_systems(
            Update,
            (
                (tyre_smoke, flames, dust, streaks, celebrate).run_if(super::bonkers),
                spawn,
                fly,
            )
                .chain(),
        );
}

/// A hue, in turns, rounded to one of a couple of dozen. Every colour a glowing
/// particle can be is a material of its own, made once and kept: a rainbow that
/// could be any of thousands would make thousands, and a couple of dozen look
/// the same.
pub(super) fn step_of_hue(hue: f32) -> f32 {
    (hue.rem_euclid(1.0) * HUES).round() / HUES
}

const HUES: f32 = 24.0;

/// The paint a particle of this kind and colour wears.
fn paint(
    kit: &mut Kit,
    materials: &mut Assets<StandardMaterial>,
    kind: Kind,
    tint: Tint,
    rng: &mut Rng,
) -> Handle<StandardMaterial> {
    let colour = match tint {
        Tint::Party => *rng.pick(&PARTY),
        Tint::Fire => *rng.pick(&[
            Color::srgb(1.0, 0.55, 0.08),
            Color::srgb(1.0, 0.82, 0.20),
            Color::srgb(1.0, 0.30, 0.05),
        ]),
        Tint::Ice => *rng.pick(&[
            Color::srgb(0.30, 0.80, 1.0),
            Color::srgb(0.70, 0.95, 1.0),
            Color::srgb(0.45, 0.55, 1.0),
        ]),
        Tint::Gold => *rng.pick(&[
            Color::srgb(1.0, 0.82, 0.20),
            Color::srgb(1.0, 0.92, 0.55),
            Color::srgb(1.0, 0.68, 0.10),
        ]),
        Tint::White => Color::WHITE,
        Tint::Smoke => *rng.pick(&[Color::srgb(0.96, 0.96, 0.98), Color::srgb(0.84, 0.86, 0.90)]),
        Tint::Rainbow(offset) => rainbow(step_of_hue(offset + rng.range(0.0, 0.25)), 0.55),
        Tint::One(colour) => colour,
    };
    match kind {
        // Things that are meant to glow, and that bloom will pick up.
        Kind::Spark | Kind::Flame | Kind::Star | Kind::Rocket | Kind::Streak => {
            kit.glow(materials, colour, 2.8)
        }
        Kind::Confetti => kit.glow(materials, colour, 1.35),
        Kind::Puff | Kind::Feather | Kind::Chunk => kit.paint(materials, colour),
    }
}

fn shape(kind: Kind) -> Shape {
    match kind {
        Kind::Confetti | Kind::Feather => Shape::Quad,
        Kind::Puff | Kind::Spark | Kind::Flame | Kind::Rocket => Shape::Sphere,
        Kind::Star => Shape::Star,
        Kind::Chunk => Shape::Cube,
        Kind::Streak => Shape::Cylinder,
    }
}

/// Turn every request into particles, up to the cap.
#[allow(clippy::too_many_arguments)]
fn spawn(
    mut commands: Commands,
    mut bursts: MessageReader<Burst>,
    kit: Option<ResMut<Kit>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    track: Res<Track>,
    mut alive: ResMut<Alive>,
    mut rng: Local<Option<Rng>>,
) {
    let Some(mut kit) = kit else {
        bursts.clear();
        return;
    };
    let rng = rng.get_or_insert_with(Rng::random);
    for burst in bursts.read() {
        let room = CAP.saturating_sub(alive.0);
        let count = (burst.count as usize).min(room);
        if count == 0 {
            continue;
        }
        let floor = track.ground_from(burst.at, None).height;
        for _ in 0..count {
            let scatter = rng.direction();
            let dir = (burst.toward + scatter * burst.spread).normalize_or(burst.toward);
            let speed = burst.speed * rng.range(0.35, 1.0);
            let (life, gravity, drag) = match burst.kind {
                Kind::Confetti => (rng.range(1.6, 3.0), 3.2, 1.6),
                Kind::Puff => (rng.range(0.55, 1.0), -0.4, 2.2),
                Kind::Spark => (rng.range(0.25, 0.7), 7.0, 0.6),
                Kind::Flame => (rng.range(0.14, 0.32), -1.0, 1.4),
                Kind::Star => (rng.range(0.5, 0.9), 0.0, 3.0),
                Kind::Feather => (rng.range(1.2, 2.2), 1.2, 2.6),
                Kind::Chunk => (rng.range(0.7, 1.4), 9.0, 0.3),
                Kind::Streak => (rng.range(0.4, 0.7), 0.0, 0.0),
                Kind::Rocket => (rng.range(0.8, 1.1), 2.0, 0.0),
            };
            let size = burst.size * rng.range(0.7, 1.3);
            let spin = rng.direction() * rng.range(3.0, 12.0);
            let mut transform = Transform::from_translation(burst.at).with_scale(Vec3::splat(size));
            if matches!(burst.kind, Kind::Confetti | Kind::Feather) {
                transform.rotation = Quat::from_euler(
                    EulerRot::XYZ,
                    rng.range(0.0, std::f32::consts::TAU),
                    rng.range(0.0, std::f32::consts::TAU),
                    rng.range(0.0, std::f32::consts::TAU),
                );
                transform.scale = Vec3::new(size * 1.5, size * 0.9, size);
            }
            let vel = if burst.kind == Kind::Streak {
                Vec3::ZERO
            } else {
                dir * speed
            };
            if burst.kind == Kind::Streak {
                transform.rotation = Quat::from_rotation_arc(Vec3::Y, burst.toward);
                transform.scale = Vec3::new(0.03, burst.speed.max(0.2), 0.03);
            }
            let paint = paint(&mut kit, &mut materials, burst.kind, burst.tint, rng);
            commands.spawn((
                Bit {
                    kind: burst.kind,
                    vel,
                    spin,
                    life,
                    max: life,
                    gravity,
                    drag,
                    size,
                    floor: floor + 0.02,
                    burst: (burst.kind == Kind::Rocket).then_some(burst.tint),
                },
                Mesh3d(kit.mesh(shape(burst.kind))),
                MeshMaterial3d(paint),
                transform,
                NotShadowCaster,
            ));
            alive.0 += 1;
        }
    }
}

/// Move everything, shrink it toward its end, and take it away when it is over.
#[allow(clippy::too_many_arguments)]
fn fly(
    mut commands: Commands,
    time: Res<Time>,
    mut alive: ResMut<Alive>,
    camera: Query<&Transform, (With<Camera3d>, Without<Bit>)>,
    mut bursts: MessageWriter<Burst>,
    mut bits: Query<(Entity, &mut Bit, &mut Transform)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let facing = camera.single().map(|c| c.rotation).ok();
    let mut count = 0usize;
    for (entity, mut bit, mut transform) in &mut bits {
        bit.life -= dt;
        if bit.life <= 0.0 {
            commands.entity(entity).despawn();
            // A rocket that runs out of climb goes off.
            if let (Some(tint), Kind::Rocket) = (bit.burst, bit.kind) {
                bursts.write(
                    Burst::new(Kind::Spark, transform.translation, 60)
                        .speed(7.0)
                        .size(0.12)
                        .tint(tint),
                );
                bursts.write(
                    Burst::new(Kind::Star, transform.translation, 14)
                        .speed(5.0)
                        .size(0.5)
                        .tint(Tint::White),
                );
            }
            continue;
        }
        count += 1;
        let drag = (1.0 - bit.drag * dt).max(0.0);
        bit.vel.y -= bit.gravity * dt;
        bit.vel *= drag;
        transform.translation += bit.vel * dt;
        if transform.translation.y < bit.floor && bit.kind != Kind::Puff {
            transform.translation.y = bit.floor;
            bit.vel = Vec3::new(bit.vel.x * 0.4, 0.0, bit.vel.z * 0.4);
            bit.spin *= 0.4;
        }
        let age = 1.0 - bit.life / bit.max;
        // What size it is at this point of its life.
        let grow = match bit.kind {
            Kind::Puff => (age * 5.0).min(1.0) * 0.6 + 0.4 * (1.0 - age).powf(0.7),
            Kind::Confetti | Kind::Feather => (bit.life / bit.max * 4.0).min(1.0),
            Kind::Star => (age * 6.0).min(1.0) * (1.0 - age),
            Kind::Streak => (bit.life / bit.max * 3.0).min(1.0).min(age * 8.0 + 0.15),
            _ => 1.0 - age,
        };
        match bit.kind {
            Kind::Streak => {
                transform.scale.x = 0.03 * grow;
                transform.scale.z = 0.03 * grow;
            }
            Kind::Confetti | Kind::Feather => {
                let flutter = 1.0 + 0.7 * (time.elapsed_secs() * 9.0 + bit.spin.x).sin().abs();
                transform.scale =
                    Vec3::new(bit.size * 1.5, bit.size * 0.9 * flutter, bit.size) * grow;
                transform.rotate(Quat::from_euler(
                    EulerRot::XYZ,
                    bit.spin.x * dt,
                    bit.spin.y * dt,
                    bit.spin.z * dt,
                ));
            }
            Kind::Star => {
                transform.scale = Vec3::splat(bit.size * grow * 1.6);
                if let Some(facing) = facing {
                    transform.rotation = facing * Quat::from_rotation_z(bit.spin.z * age);
                }
            }
            Kind::Puff => {
                transform.scale = Vec3::splat(bit.size * (1.0 + age * 2.2) * grow);
            }
            _ => {
                transform.scale = Vec3::splat(bit.size * grow.max(0.02));
            }
        }
        // A rocket has its trail.
        if bit.kind == Kind::Rocket && (bit.life * 60.0).fract() < dt * 60.0 {
            bursts.write(
                Burst::new(Kind::Spark, transform.translation, 2)
                    .toward(Vec3::NEG_Y, 0.5)
                    .speed(1.5)
                    .size(0.06)
                    .tint(Tint::Gold),
            );
        }
    }
    alive.0 = count;
}

/// Where the player's car is as it looks, with its height off the road.
fn mount_at(at: &Transform, air: Option<&super::air::Air>) -> Transform {
    let mut at = *at;
    at.translation.y += air.map_or(0.0, |a| a.height);
    at
}

/// Cartoon smoke from the tail of a car that is sliding.
#[allow(clippy::type_complexity)]
fn tyre_smoke(
    time: Res<Time>,
    mut bursts: MessageWriter<Burst>,
    mut owed: Local<f32>,
    cars: Query<(&Transform, &Car, Option<&super::air::Air>), With<Player>>,
) {
    let Ok((at, car, air)) = cars.single() else {
        return;
    };
    if air.is_some_and(|a| a.flying) {
        return;
    }
    let sliding = ((car.rear_slip - 0.25) / 0.6).clamp(0.0, 1.0);
    let speed = car.velocity.length();
    if sliding <= 0.0 || speed < 3.0 {
        *owed = 0.0;
        return;
    }
    *owed += time.delta_secs() * (14.0 + 30.0 * sliding);
    let heading = level(*at.forward());
    let right = heading.cross(Vec3::Y);
    while *owed >= 1.0 {
        *owed -= 1.0;
        for side in [-1.0f32, 1.0] {
            let tail = at.translation - heading * 0.42 + right * side * 0.20 + Vec3::Y * 0.06;
            bursts.write(
                Burst::new(Kind::Puff, tail, 1)
                    .toward(Vec3::Y * 0.6 - car.velocity.normalize_or_zero() * 0.4, 0.5)
                    .speed(1.4)
                    .size(0.22 + 0.16 * sliding)
                    .tint(Tint::Smoke),
            );
        }
    }
}

/// Flames from the tail: a lot under a boost, and three pops on lifting off.
#[allow(clippy::too_many_arguments)]
fn flames(
    time: Res<Time>,
    halt: Res<crate::pause::Halt>,
    fun: Res<Fun>,
    boost: Res<Boost>,
    mut bursts: MessageWriter<Burst>,
    mut sounds: MessageWriter<crate::sound::Sfx>,
    mut owed: Local<f32>,
    mut was: Local<f32>,
    cars: Query<(&Transform, &Car, &Controls, Option<&super::air::Air>), With<Player>>,
) {
    // Nothing is lit or coughed while the game is stopped, and a driver who lets
    // go of the throttle by pausing has not lifted off.
    if halt.stopped() {
        return;
    }
    let Ok((at, car, controls, air)) = cars.single() else {
        return;
    };
    let at = mount_at(at, air);
    let heading = level(*at.forward());
    let tail = at.translation - heading * (1.3 * SCALE) + Vec3::Y * (1.5 * SCALE);
    let push = boost.strength();
    if push > 0.05 {
        *owed += time.delta_secs() * (60.0 * push);
        while *owed >= 1.0 {
            *owed -= 1.0;
            bursts.write(
                Burst::new(Kind::Flame, tail, 1)
                    .toward(-heading + Vec3::Y * 0.1, 0.35)
                    .speed(3.0 + car.velocity.length() * 0.25)
                    .size(0.16 + 0.1 * push)
                    .tint(Tint::Fire),
            );
            if (time.elapsed_secs() * 30.0).fract() < 0.3 {
                bursts.write(
                    Burst::new(Kind::Flame, tail, 1)
                        .toward(-heading, 0.25)
                        .speed(3.5)
                        .size(0.12)
                        .tint(Tint::Ice),
                );
            }
        }
    }
    // Lifting off at speed: the exhaust coughs. Not for anyone who has asked for
    // calm, whom a sudden burst of sparks would not suit.
    let now = controls.throttle;
    if *was > 0.7 && now < 0.15 && car.velocity.length() > 9.0 && !fun.calm {
        for _ in 0..3 {
            bursts.write(
                Burst::new(Kind::Flame, tail, 2)
                    .toward(-heading + Vec3::Y * 0.2, 0.4)
                    .speed(4.5)
                    .size(0.18)
                    .tint(Tint::Fire),
            );
        }
        bursts.write(
            Burst::new(Kind::Spark, tail, 6)
                .toward(-heading, 0.6)
                .speed(5.0)
                .size(0.05)
                .tint(Tint::Gold),
        );
        sounds.write(crate::sound::Sfx::new(crate::sound::SfxKind::Backfire).gain(0.7));
    }
    *was = now;
}

/// Dust and feathers where the car comes down.
fn dust(
    mut landed: MessageReader<Landed>,
    mut honks: MessageReader<Honk>,
    mut bursts: MessageWriter<Burst>,
    cars: Query<(&Transform, &Car), With<Player>>,
) {
    let Ok((at, car)) = cars.single() else {
        landed.clear();
        honks.clear();
        return;
    };
    for down in landed.read() {
        let strength = (down.impact / 6.0).clamp(0.3, 2.0);
        bursts.write(
            Burst::new(
                Kind::Puff,
                at.translation + Vec3::Y * 0.05,
                (6.0 * strength) as u32,
            )
            .toward(Vec3::Y * 0.2, 1.0)
            .speed(2.4 * strength)
            .size(0.28)
            .tint(Tint::Smoke),
        );
        bursts.write(
            Burst::new(
                Kind::Feather,
                at.translation + Vec3::Y * 0.3,
                (5.0 * strength) as u32,
            )
            .toward(Vec3::Y, 0.9)
            .speed(2.5)
            .size(0.1)
            .tint(Tint::White),
        );
    }
    for _ in honks.read() {
        bursts.write(
            Burst::new(Kind::Feather, at.translation + Vec3::Y * 0.6, 4)
                .toward(car.velocity + Vec3::Y, 0.8)
                .speed(1.6)
                .size(0.09)
                .tint(Tint::White),
        );
    }
}

/// Streaks of light that fly past at speed, hung in the air where the car is
/// going and left there to be passed.
fn streaks(
    time: Res<Time>,
    fun: Res<Fun>,
    mut bursts: MessageWriter<Burst>,
    mut owed: Local<f32>,
    mut rng: Local<Option<Rng>>,
    cars: Query<(&Transform, &Car), With<Player>>,
) {
    if !fun.bonkers() || fun.calm {
        return;
    }
    let Ok((at, car)) = cars.single() else {
        return;
    };
    let speed = car.velocity.length();
    let fast = speed / (24.0 * fun.speed.scale());
    if fast < 0.5 {
        *owed = 0.0;
        return;
    }
    *owed += time.delta_secs() * 70.0 * (fast - 0.4).min(1.0);
    let rng = rng.get_or_insert_with(Rng::random);
    let heading = car.velocity.normalize_or_zero();
    let right = heading.cross(Vec3::Y).normalize_or_zero();
    while *owed >= 1.0 {
        *owed -= 1.0;
        let ahead = rng.range(9.0, 34.0);
        let across = rng.range(-9.0, 9.0);
        let up = rng.range(0.2, 4.0);
        let position = at.translation + heading * ahead + right * across + Vec3::Y * up;
        bursts.write(
            Burst::new(Kind::Streak, position, 1)
                .toward(heading, 0.0)
                // A streak's length rides in its speed.
                .speed(0.8 + speed * 0.05)
                .tint(Tint::One(rainbow(step_of_hue(rng.unit()), 0.75))),
        );
    }
}

/// A lap is finished: confetti from both sides of the road, and fireworks over
/// the line.
fn celebrate(
    mut laps: MessageReader<LapFinished>,
    fun: Res<Fun>,
    track: Res<Track>,
    mut bursts: MessageWriter<Burst>,
    mut sounds: MessageWriter<crate::sound::Sfx>,
    mut rng: Local<Option<Rng>>,
) {
    let Some(lap) = laps.read().last().copied() else {
        return;
    };
    let rng = rng.get_or_insert_with(Rng::random);
    let line = track.spot_at(0.0);
    let ground = track.ground_from(line.pos, Some(line.s)).height;
    for side in [-1.0f32, 1.0] {
        let at = line.pos + line.right * (side * 2.6) + Vec3::Y * (ground - line.pos.y + 0.2);
        bursts.write(
            Burst::new(Kind::Confetti, at, 90)
                .toward(Vec3::Y * 1.6 - line.right * side * 0.7, 0.7)
                .speed(9.0)
                .size(0.10)
                .tint(Tint::Party),
        );
    }
    sounds.write(crate::sound::Sfx::new(crate::sound::SfxKind::Popper));
    // Every lap ends under a few rockets, and a best under more of them: a
    // Bonkers lap is never a best, and is the one that most wants them. The
    // bursts of light they end in are not for anyone who asked for calm.
    if !fun.calm {
        for i in 0..if lap.best { 8 } else { 3 } {
            let side = if i % 2 == 0 { -1.0 } else { 1.0 };
            let at = line.pos
                + line.right * (side * rng.range(3.0, 7.0))
                + line.tangent * rng.range(-3.0, 6.0);
            let base = track.ground_from(at, Some(line.s)).height;
            bursts.write(
                Burst::new(Kind::Rocket, Vec3::new(at.x, base + 0.1, at.z), 1)
                    .toward(Vec3::Y + line.right * side * 0.15, 0.06)
                    .speed(rng.range(8.0, 11.0))
                    .size(0.14)
                    .tint(Tint::Rainbow(rng.unit())),
            );
        }
        sounds.write(crate::sound::Sfx::new(crate::sound::SfxKind::Rocket).gain(0.6));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_is_built_up_the_way_it_is_asked() {
        let burst = Burst::new(Kind::Spark, Vec3::ONE, 8)
            .toward(Vec3::new(0.0, 5.0, 0.0), 0.3)
            .speed(4.0)
            .size(0.2)
            .tint(Tint::Gold);
        assert_eq!(burst.count, 8);
        assert_eq!(burst.toward, Vec3::Y, "the direction is normalised");
        assert_eq!((burst.speed, burst.size, burst.spread), (4.0, 0.2, 0.3));
    }

    #[test]
    fn every_kind_has_a_shape_and_a_paint() {
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let mut kit = Kit::new(&mut meshes);
        let mut rng = Rng::new(3);
        for kind in [
            Kind::Confetti,
            Kind::Puff,
            Kind::Spark,
            Kind::Flame,
            Kind::Star,
            Kind::Feather,
            Kind::Chunk,
            Kind::Streak,
            Kind::Rocket,
        ] {
            for tint in [
                Tint::Party,
                Tint::Fire,
                Tint::Ice,
                Tint::Gold,
                Tint::White,
                Tint::Smoke,
                Tint::Rainbow(0.3),
            ] {
                let handle = paint(&mut kit, &mut materials, kind, tint, &mut rng);
                assert!(materials.get(&handle).is_some(), "{kind:?} {tint:?}");
            }
            assert!(meshes.get(&kit.mesh(shape(kind))).is_some());
        }
    }
}
