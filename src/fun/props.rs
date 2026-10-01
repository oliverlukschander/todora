//! Things to hit.
//!
//! The road has cows on it. It also has cones, bowling pins, ducks, watermelons,
//! cardboard boxes and balloons, laid out by [`super::course`] and built here
//! out of the kit's primitives. Drive into one and it goes flying, with a noise
//! that suits it, a scatter of whatever it is made of, and points; a bowling pin
//! knocked over knocks the ones behind it, so a strike is possible and worth
//! having; a balloon simply pops.
//!
//! A prop has two lives. Standing, it is a place and a size, and the car is
//! tested against it once a physics step, along the path it took and not just
//! where it ended, so that nothing is ever driven through. Flung, it is a small
//! ballistic body with a spin, bouncing on the ground it came from, that goes
//! after a couple of seconds. What a hit does to the car itself is a small
//! knock, which is nothing like the stop a wall would be; it is not a game about
//! cows winning.
//!
//! The tube-men are only scenery. They wave.

use bevy::{ecs::system::SystemParam, prelude::*};

use super::Fun;
use super::air::Air;
use super::announcer::{Announce, Points};
use super::course::{Layout, ManSpot, PropKind, PropSpot};
use super::juice::Jolt;
use super::particles::{Burst, Kind, Tint};
use super::parts::{Kit, PARTY, Shape, add, add_turned, rainbow};
use super::rng::Rng;
use crate::car::{Car, Player, level};
use crate::lap::LapFinished;
use crate::sound::{Sfx, SfxKind};
use crate::track::Track;

/// The car's own reach, for the purpose of hitting things, in metres.
const CAR_REACH: f32 = 0.30;
/// Gravity for thrown things, in metres a second squared. A little less than a
/// real one, so that a cow stays up long enough to be seen.
const GRAVITY: f32 = 13.0;
/// Seconds a flung prop lasts.
const FLIGHT: f32 = 2.6;

/// Whether a prop is where it was put or on its way somewhere.
enum State {
    Standing,
    /// Falling out of the sky, to land on its feet.
    Falling {
        vy: f32,
    },
    Flung {
        vel: Vec3,
        spin: Vec3,
        age: f32,
    },
}

/// It is raining cows.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct Rain {
    pub count: u32,
}

#[derive(Component)]
struct Prop {
    kind: PropKind,
    group: u16,
    radius: f32,
    /// How tall it stands, so a car in the air can clear it.
    height: f32,
    /// Where it stood, and how high its middle is above the ground.
    home: Vec3,
    rest: f32,
    state: State,
}

/// A tube-man's wave: a phase, and the segments that carry it.
#[derive(Component)]
struct Wave {
    phase: f32,
    index: usize,
}

/// Bowling pins hit in each group, and how long ago the first went down.
#[derive(Resource, Default)]
struct Alley(Vec<(u16, u8, f32)>);

/// What the props were last built for: the course, and how many props that put
/// on the road.
#[derive(Resource, Default)]
struct Built(u32, usize);

/// A cow that fell out of the sky. It is on the road for a while and not for good:
/// there is nothing else to take it off, and a game that is left running would
/// otherwise keep every one it was ever rained on.
#[derive(Component)]
struct Rained {
    left: f32,
}

/// Seconds a rained cow stays, at the least, and how near the car it may be
/// when it goes: never in its face.
const RAINED_FOR: f32 = 40.0;
const OUT_OF_SIGHT: f32 = 25.0;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Alley>()
        .init_resource::<Built>()
        .add_message::<Rain>()
        .add_systems(
            Update,
            (
                (build, rebuild_on_lap).chain().run_if(super::bonkers),
                clear.run_if(not(super::bonkers)),
                (rain, retire_rain, fly).chain().run_if(super::bonkers),
                sway,
            )
                .chain(),
        )
        .add_systems(FixedUpdate, hit.run_if(super::bonkers));
}

/// Put the props and the tube-men on the road, again when the course is new.
#[allow(clippy::too_many_arguments)]
fn build(
    mut commands: Commands,
    layout: Res<Layout>,
    track: Res<Track>,
    kit: Option<ResMut<Kit>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut built: ResMut<Built>,
    mut alley: ResMut<Alley>,
    old: Query<Entity, Scenery>,
) {
    let Some(mut kit) = kit else {
        return;
    };
    if built.0 == layout.edition {
        return;
    }
    *built = Built(layout.edition, layout.course.props.len());
    alley.0.clear();
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let mut rng = Rng::of(track.circuit().id, 0x9A_11);
    for spot in &layout.course.props {
        spawn_prop(
            &mut commands,
            &mut kit,
            &mut materials,
            &track,
            spot,
            &mut rng,
            None,
        );
    }
    for man in &layout.course.men {
        spawn_man(&mut commands, &mut kit, &mut materials, &track, man);
    }
}

/// The whole tube-man, for taking away at once.
#[derive(Component)]
struct TubeMan;

/// The player's car, which is not a prop.
type Striker = (With<Player>, Without<Prop>);

/// Everything this module put on the road.
type Scenery = Or<(With<Prop>, With<TubeMan>)>;

/// The plain game has none of this on the road.
fn clear(mut commands: Commands, mut built: ResMut<Built>, old: Query<Entity, Scenery>) {
    if old.is_empty() {
        built.0 = 0;
        return;
    }
    built.0 = 0;
    for entity in &old {
        commands.entity(entity).despawn();
    }
}

/// A fresh road for every lap: whatever was smashed is back. If anything was:
/// putting every prop and every tube-man back a frame after the last was a
/// hitch, and a road nothing had touched was not worth it.
fn rebuild_on_lap(
    mut laps: MessageReader<LapFinished>,
    mut resets: MessageReader<crate::Reset>,
    mut built: ResMut<Built>,
    props: Query<(&Prop, Has<Rained>)>,
) {
    if laps.read().next().is_some() | resets.read().next().is_some() {
        let standing = props
            .iter()
            .filter(|(prop, rained)| !rained && matches!(prop.state, State::Standing))
            .count();
        let rained = props.iter().any(|(_, rained)| rained);
        if standing != built.1 || rained {
            built.0 = 0;
        }
    }
}

/// The colour a balloon is, by where it hangs.
fn balloon_colour(at: Vec3) -> Color {
    PARTY[((at.x * 7.0 + at.z * 3.0).abs() as usize) % PARTY.len()]
}

fn spawn_prop(
    commands: &mut Commands,
    kit: &mut Kit,
    materials: &mut Assets<StandardMaterial>,
    track: &Track,
    spot: &PropSpot,
    rng: &mut Rng,
    dropped_from: Option<f32>,
) -> Entity {
    let ground = track.ground_from(spot.at, Some(spot.s)).height;
    let (radius, height) = match spot.kind {
        PropKind::Cone => (0.19, 0.5),
        PropKind::Pin => (0.15, 0.72),
        PropKind::Cow => (0.50, 0.98),
        PropKind::Duck => (0.21, 0.42),
        PropKind::Melon => (0.20, 0.36),
        PropKind::Crate => (0.26, 0.42),
        PropKind::Balloon => (0.21, 1.6),
    };
    let lift = if spot.kind == PropKind::Balloon {
        0.55 + rng.range(0.0, 0.5)
    } else {
        0.0
    };
    let home = Vec3::new(spot.at.x, ground, spot.at.z);
    let mut paint = |c: Color| kit.paint(materials, c);
    let white = paint(Color::WHITE);
    let black = paint(Color::srgb(0.06, 0.06, 0.07));
    let pink = paint(Color::srgb(1.0, 0.62, 0.72));
    let orange = paint(Color::srgb(1.0, 0.42, 0.04));
    let yellow = paint(Color::srgb(1.0, 0.85, 0.10));
    let beak = paint(Color::srgb(1.0, 0.5, 0.05));
    let green = paint(Color::srgb(0.16, 0.62, 0.22));
    let dark_green = paint(Color::srgb(0.06, 0.32, 0.10));
    let cardboard = paint(Color::srgb(0.74, 0.54, 0.30));
    let tape = paint(Color::srgb(0.90, 0.80, 0.55));
    let red = paint(Color::srgb(0.88, 0.10, 0.12));
    let horn = paint(Color::srgb(0.90, 0.85, 0.70));
    let gold = paint(Color::srgb(1.0, 0.74, 0.12));
    let balloon = paint(balloon_colour(spot.at));
    let string = paint(Color::srgb(0.9, 0.9, 0.9));
    let kit = &*kit;

    let mut root = commands.spawn((
        Prop {
            kind: spot.kind,
            group: spot.group,
            radius,
            height,
            home,
            rest: height * 0.5,
            state: dropped_from.map_or(State::Standing, |_| State::Falling { vy: 0.0 }),
        },
        Transform::from_translation(home + Vec3::Y * (lift + dropped_from.unwrap_or(0.0)))
            .with_rotation(Quat::from_rotation_y(spot.yaw)),
        Visibility::Inherited,
    ));
    root.with_children(|p| match spot.kind {
        PropKind::Cone => {
            add(
                p,
                kit,
                Shape::Cube,
                &orange,
                Vec3::new(0.0, 0.02, 0.0),
                Vec3::new(0.42, 0.04, 0.42),
            );
            add(
                p,
                kit,
                Shape::Cone,
                &orange,
                Vec3::new(0.0, 0.27, 0.0),
                Vec3::new(0.34, 0.46, 0.34),
            );
            add(
                p,
                kit,
                Shape::Cylinder,
                &white,
                Vec3::new(0.0, 0.22, 0.0),
                Vec3::new(0.245, 0.09, 0.245),
            );
        }
        PropKind::Pin => {
            add(
                p,
                kit,
                Shape::Sphere,
                &white,
                Vec3::new(0.0, 0.24, 0.0),
                Vec3::new(0.29, 0.44, 0.29),
            );
            add(
                p,
                kit,
                Shape::Cylinder,
                &white,
                Vec3::new(0.0, 0.50, 0.0),
                Vec3::new(0.11, 0.24, 0.11),
            );
            add(
                p,
                kit,
                Shape::Sphere,
                &white,
                Vec3::new(0.0, 0.63, 0.0),
                Vec3::splat(0.17),
            );
            add(
                p,
                kit,
                Shape::Cylinder,
                &red,
                Vec3::new(0.0, 0.43, 0.0),
                Vec3::new(0.14, 0.035, 0.14),
            );
            add(
                p,
                kit,
                Shape::Cylinder,
                &red,
                Vec3::new(0.0, 0.40, 0.0),
                Vec3::new(0.15, 0.03, 0.15),
            );
        }
        PropKind::Cow => {
            add(
                p,
                kit,
                Shape::Sphere,
                &white,
                Vec3::new(0.0, 0.62, 0.0),
                Vec3::new(0.52, 0.50, 1.05),
            );
            add(
                p,
                kit,
                Shape::Sphere,
                &black,
                Vec3::new(0.21, 0.70, -0.10),
                Vec3::new(0.14, 0.30, 0.34),
            );
            add(
                p,
                kit,
                Shape::Sphere,
                &black,
                Vec3::new(-0.22, 0.66, 0.26),
                Vec3::new(0.14, 0.26, 0.30),
            );
            add(
                p,
                kit,
                Shape::Sphere,
                &black,
                Vec3::new(0.02, 0.87, 0.18),
                Vec3::new(0.30, 0.10, 0.28),
            );
            add(
                p,
                kit,
                Shape::Sphere,
                &white,
                Vec3::new(0.0, 0.78, -0.62),
                Vec3::new(0.34, 0.32, 0.36),
            );
            add(
                p,
                kit,
                Shape::Sphere,
                &pink,
                Vec3::new(0.0, 0.71, -0.82),
                Vec3::new(0.26, 0.19, 0.16),
            );
            for side in [-1.0f32, 1.0] {
                add(
                    p,
                    kit,
                    Shape::Sphere,
                    &black,
                    Vec3::new(side * 0.06, 0.72, -0.90),
                    Vec3::splat(0.035),
                );
                add(
                    p,
                    kit,
                    Shape::Sphere,
                    &white,
                    Vec3::new(side * 0.12, 0.85, -0.75),
                    Vec3::splat(0.08),
                );
                add(
                    p,
                    kit,
                    Shape::Sphere,
                    &black,
                    Vec3::new(side * 0.13, 0.85, -0.79),
                    Vec3::splat(0.05),
                );
                add_turned(
                    p,
                    kit,
                    Shape::Cone,
                    &horn,
                    Vec3::new(side * 0.13, 0.99, -0.60),
                    Vec3::new(0.05, 0.13, 0.05),
                    Quat::from_rotation_z(-side * 0.5),
                );
                add(
                    p,
                    kit,
                    Shape::Sphere,
                    &white,
                    Vec3::new(side * 0.21, 0.90, -0.56),
                    Vec3::new(0.14, 0.05, 0.09),
                );
                for back in [-0.32f32, 0.32] {
                    add(
                        p,
                        kit,
                        Shape::Cylinder,
                        &white,
                        Vec3::new(side * 0.17, 0.19, back),
                        Vec3::new(0.10, 0.38, 0.10),
                    );
                    add(
                        p,
                        kit,
                        Shape::Cylinder,
                        &black,
                        Vec3::new(side * 0.17, 0.03, back),
                        Vec3::new(0.11, 0.06, 0.11),
                    );
                }
            }
            add(
                p,
                kit,
                Shape::Sphere,
                &pink,
                Vec3::new(0.0, 0.36, 0.26),
                Vec3::new(0.20, 0.14, 0.20),
            );
            add(
                p,
                kit,
                Shape::Sphere,
                &gold,
                Vec3::new(0.0, 0.56, -0.52),
                Vec3::splat(0.11),
            );
            add_turned(
                p,
                kit,
                Shape::Cylinder,
                &white,
                Vec3::new(0.0, 0.68, 0.60),
                Vec3::new(0.035, 0.42, 0.035),
                Quat::from_rotation_x(0.5),
            );
            add(
                p,
                kit,
                Shape::Sphere,
                &black,
                Vec3::new(0.0, 0.50, 0.70),
                Vec3::splat(0.09),
            );
        }
        PropKind::Duck => {
            add(
                p,
                kit,
                Shape::Sphere,
                &yellow,
                Vec3::new(0.0, 0.16, 0.0),
                Vec3::new(0.32, 0.26, 0.40),
            );
            add(
                p,
                kit,
                Shape::Sphere,
                &yellow,
                Vec3::new(0.0, 0.34, -0.14),
                Vec3::splat(0.21),
            );
            add(
                p,
                kit,
                Shape::Sphere,
                &beak,
                Vec3::new(0.0, 0.32, -0.27),
                Vec3::new(0.13, 0.05, 0.11),
            );
            add_turned(
                p,
                kit,
                Shape::Cone,
                &yellow,
                Vec3::new(0.0, 0.26, 0.24),
                Vec3::new(0.10, 0.16, 0.10),
                Quat::from_rotation_x(1.0),
            );
            for side in [-1.0f32, 1.0] {
                add(
                    p,
                    kit,
                    Shape::Sphere,
                    &black,
                    Vec3::new(side * 0.07, 0.38, -0.23),
                    Vec3::splat(0.035),
                );
            }
        }
        PropKind::Melon => {
            add(
                p,
                kit,
                Shape::Sphere,
                &green,
                Vec3::new(0.0, 0.18, 0.0),
                Vec3::new(0.36, 0.34, 0.40),
            );
            for a in 0..3 {
                let turn = a as f32 * 1.05;
                add_turned(
                    p,
                    kit,
                    Shape::Sphere,
                    &dark_green,
                    Vec3::new(0.0, 0.18, 0.0),
                    Vec3::new(0.365, 0.345, 0.05),
                    Quat::from_rotation_y(turn),
                );
            }
        }
        PropKind::Crate => {
            add(
                p,
                kit,
                Shape::Cube,
                &cardboard,
                Vec3::new(0.0, 0.21, 0.0),
                Vec3::splat(0.42),
            );
            add(
                p,
                kit,
                Shape::Cube,
                &tape,
                Vec3::new(0.0, 0.21, 0.0),
                Vec3::new(0.10, 0.425, 0.425),
            );
            add(
                p,
                kit,
                Shape::Cube,
                &tape,
                Vec3::new(0.0, 0.212, 0.0),
                Vec3::new(0.425, 0.06, 0.06),
            );
        }
        PropKind::Balloon => {
            add(
                p,
                kit,
                Shape::Sphere,
                &balloon,
                Vec3::new(0.0, 0.0, 0.0),
                Vec3::new(0.34, 0.40, 0.34),
            );
            add(
                p,
                kit,
                Shape::Sphere,
                &balloon,
                Vec3::new(0.0, -0.21, 0.0),
                Vec3::splat(0.05),
            );
            add(
                p,
                kit,
                Shape::Cylinder,
                &string,
                Vec3::new(0.0, -0.75, 0.0),
                Vec3::new(0.008, 1.0, 0.008),
            );
        }
    });
    root.id()
}

/// A tube-man: a stack of segments that wave, and two flailing arms.
fn spawn_man(
    commands: &mut Commands,
    kit: &mut Kit,
    materials: &mut Assets<StandardMaterial>,
    track: &Track,
    man: &ManSpot,
) {
    let ground = track.ground_from(man.at, Some(man.s)).height;
    let colour = rainbow(f32::from(man.tint) / 6.0 + 0.02, 0.55);
    let glow = kit.glow(materials, colour, 1.6);
    let dark = kit.paint(materials, Color::srgb(0.08, 0.08, 0.11));
    let kit = &*kit;
    // Facing the road.
    let spot = track.spot_at(man.s);
    let toward = -(man.at - spot.pos)
        .reject_from(Vec3::Y)
        .normalize_or(Vec3::X);
    commands
        .spawn((
            TubeMan,
            Transform::from_translation(Vec3::new(man.at.x, ground, man.at.z))
                .looking_to(toward, Vec3::Y),
            Visibility::Inherited,
        ))
        .with_children(|root| {
            // A base blower, and six segments each hung from the last.
            add(
                root,
                kit,
                Shape::Cylinder,
                &dark,
                Vec3::new(0.0, 0.08, 0.0),
                Vec3::new(0.5, 0.16, 0.5),
            );
            let mut stack = root.spawn((
                Transform::from_xyz(0.0, 0.16, 0.0),
                Visibility::Inherited,
                Wave {
                    phase: f32::from(man.tint) * 1.1,
                    index: 0,
                },
            ));
            stack.with_children(|s| segment(s, kit, &glow, 0));
        });
}

/// One segment of a tube-man, and inside it the next.
fn segment(
    parent: &mut ChildSpawnerCommands,
    kit: &Kit,
    paint: &Handle<StandardMaterial>,
    at: usize,
) {
    let width = 0.42 - at as f32 * 0.025;
    add(
        parent,
        kit,
        Shape::Capsule,
        paint,
        Vec3::new(0.0, 0.30, 0.0),
        Vec3::new(width, 0.42, width),
    );
    if at == 4 {
        for side in [-1.0f32, 1.0] {
            add_turned(
                parent,
                kit,
                Shape::Capsule,
                paint,
                Vec3::new(side * 0.34, 0.42, 0.0),
                Vec3::new(0.10, 0.45, 0.10),
                Quat::from_rotation_z(-side * 0.9),
            );
        }
    }
    if at == 5 {
        add(
            parent,
            kit,
            Shape::Sphere,
            paint,
            Vec3::new(0.0, 0.62, 0.0),
            Vec3::splat(0.34),
        );
        return;
    }
    let mut next = parent.spawn((
        Transform::from_xyz(0.0, 0.56, 0.0),
        Visibility::Inherited,
        Wave {
            phase: 0.0,
            index: at + 1,
        },
    ));
    next.with_children(|n| segment(n, kit, paint, at + 1));
}

/// The tube-men wave, from the base up: each segment leans a little further
/// than the one below it, a little later.
fn sway(time: Res<Time<Real>>, fun: Res<Fun>, mut waves: Query<(&Wave, &mut Transform)>) {
    let t = time.elapsed_secs();
    let calm = fun.calm;
    for (wave, mut transform) in &mut waves {
        let lag = wave.index as f32 * 0.55;
        let amount = if calm { 0.12 } else { 0.50 } * (0.35 + wave.index as f32 * 0.14);
        let lean = (t * 3.4 + wave.phase - lag).sin() * amount;
        let twist = (t * 2.3 + wave.phase * 1.7 - lag).cos() * amount * 0.7;
        transform.rotation = Quat::from_rotation_z(lean) * Quat::from_rotation_x(twist);
    }
}

/// The point on the segment `a`–`b` nearest `p`, in plan.
fn nearest_on(a: Vec3, b: Vec3, p: Vec3) -> Vec3 {
    let ab = (b - a).reject_from(Vec3::Y);
    let len2 = ab.length_squared();
    if len2 < 1e-9 {
        return a;
    }
    let t = ((p - a).reject_from(Vec3::Y).dot(ab) / len2).clamp(0.0, 1.0);
    a + (b - a) * t
}

/// What each prop does when it is hit.
struct Smash {
    sound: SfxKind,
    pitch: f32,
    gain: f32,
    points: u32,
    label: &'static str,
    say: Option<&'static str>,
    /// How much of the car's speed it takes.
    slow: f32,
    jolt: f32,
    tint: Tint,
    kind: Kind,
    pieces: u32,
}

fn smash_of(kind: PropKind) -> Smash {
    match kind {
        PropKind::Cone => Smash {
            sound: SfxKind::Bonk,
            pitch: 1.0,
            gain: 0.7,
            points: 50,
            label: "pop.cone",
            say: None,
            slow: 0.01,
            jolt: 0.08,
            tint: Tint::One(Color::srgb(1.0, 0.42, 0.04)),
            kind: Kind::Chunk,
            pieces: 4,
        },
        PropKind::Pin => Smash {
            sound: SfxKind::Clatter,
            pitch: 1.0,
            gain: 0.55,
            points: 40,
            label: "pop.pin",
            say: None,
            slow: 0.004,
            jolt: 0.05,
            tint: Tint::White,
            kind: Kind::Chunk,
            pieces: 3,
        },
        PropKind::Cow => Smash {
            sound: SfxKind::HornMoo,
            pitch: 1.25,
            gain: 0.85,
            points: 300,
            label: "pop.cow",
            say: Some("say.moo"),
            slow: 0.07,
            jolt: 0.42,
            tint: Tint::White,
            kind: Kind::Chunk,
            pieces: 9,
        },
        PropKind::Duck => Smash {
            sound: SfxKind::Squeak,
            pitch: 0.9,
            gain: 0.8,
            points: 100,
            label: "pop.duck",
            say: None,
            slow: 0.01,
            jolt: 0.1,
            tint: Tint::One(Color::srgb(1.0, 0.85, 0.1)),
            kind: Kind::Feather,
            pieces: 7,
        },
        PropKind::Melon => Smash {
            sound: SfxKind::Splat,
            pitch: 1.0,
            gain: 0.8,
            points: 120,
            label: "pop.melon",
            say: None,
            slow: 0.02,
            jolt: 0.16,
            tint: Tint::One(Color::srgb(0.95, 0.15, 0.25)),
            kind: Kind::Chunk,
            pieces: 10,
        },
        PropKind::Crate => Smash {
            sound: SfxKind::Crunch,
            pitch: 1.0,
            gain: 0.8,
            points: 60,
            label: "pop.crate",
            say: None,
            slow: 0.03,
            jolt: 0.14,
            tint: Tint::One(Color::srgb(0.74, 0.54, 0.30)),
            kind: Kind::Chunk,
            pieces: 7,
        },
        PropKind::Balloon => Smash {
            sound: SfxKind::Pop,
            pitch: 1.0,
            gain: 0.7,
            points: 25,
            label: "pop.balloon",
            say: None,
            slow: 0.0,
            jolt: 0.03,
            tint: Tint::Party,
            kind: Kind::Confetti,
            pieces: 12,
        },
    }
}

/// Knock a standing prop away with `vel`, and everything that goes with it.
#[allow(clippy::too_many_arguments)]
fn knock(
    prop: &mut Prop,
    transform: &Transform,
    vel: Vec3,
    rng: &mut Rng,
    alley: &mut Alley,
    now: f32,
    out: &mut Effects,
    by_car: bool,
) -> bool {
    if !matches!(prop.state, State::Standing) {
        return false;
    }
    let smash = smash_of(prop.kind);
    let at = transform.translation + Vec3::Y * (prop.height * 0.5);
    if prop.kind == PropKind::Balloon {
        prop.state = State::Flung {
            vel: Vec3::ZERO,
            spin: Vec3::ZERO,
            age: FLIGHT,
        };
        out.burst(
            Burst::new(smash.kind, at, smash.pieces)
                .toward(Vec3::Y, 1.0)
                .speed(3.5)
                .size(0.07)
                .tint(Tint::One(balloon_colour(prop.home))),
        );
    } else {
        let lift = 3.0 + vel.length() * 0.22 + rng.range(0.0, 2.5);
        let spin = rng.direction() * rng.range(5.0, 14.0);
        prop.state = State::Flung {
            vel: Vec3::new(vel.x, 0.0, vel.z) * rng.range(0.8, 1.1)
                + Vec3::Y * lift
                + rng.direction() * 1.2,
            spin,
            age: 0.0,
        };
        out.burst(
            Burst::new(smash.kind, at, smash.pieces)
                .toward(Vec3::Y * 0.7 + vel.normalize_or_zero(), 0.8)
                .speed(4.0)
                .size(0.09)
                .tint(smash.tint),
        );
    }
    // A bowling pin's noise is made once for the whole set, by the first.
    let first_of_alley =
        prop.kind == PropKind::Pin && !alley.0.iter().any(|(g, _, _)| *g == prop.group);
    if prop.kind != PropKind::Pin || first_of_alley {
        out.sound(
            Sfx::new(smash.sound)
                .pitch(smash.pitch * rng.range(0.94, 1.08))
                .gain(smash.gain),
        );
    }
    if by_car || prop.kind != PropKind::Pin {
        out.points(Points {
            amount: smash.points,
            what: smash.label,
            at: Some(at),
        });
        out.jolt(Jolt(smash.jolt));
        if let Some(say) = smash.say {
            out.say(Announce::big(say));
        }
    }
    if prop.kind == PropKind::Pin {
        match alley.0.iter_mut().find(|(g, _, _)| *g == prop.group) {
            Some(entry) => entry.1 += 1,
            None => alley.0.push((prop.group, 1, now)),
        }
    }
    true
}

/// What a smash asks of the rest of the game: particles, noise, points, a
/// banner, a jolt for the camera.
#[derive(SystemParam)]
struct Effects<'w> {
    bursts: MessageWriter<'w, Burst>,
    sounds: MessageWriter<'w, Sfx>,
    points: MessageWriter<'w, Points>,
    said: MessageWriter<'w, Announce>,
    jolts: MessageWriter<'w, Jolt>,
}

impl Effects<'_> {
    fn burst(&mut self, b: Burst) {
        self.bursts.write(b);
    }
    fn sound(&mut self, s: Sfx) {
        self.sounds.write(s);
    }
    fn points(&mut self, p: Points) {
        self.points.write(p);
    }
    fn say(&mut self, a: Announce) {
        self.said.write(a);
    }
    fn jolt(&mut self, j: Jolt) {
        self.jolts.write(j);
    }
}

/// Cows, from the sky, onto the road ahead of the car.
#[allow(clippy::too_many_arguments)]
fn rain(
    mut commands: Commands,
    mut asked: MessageReader<Rain>,
    track: Res<Track>,
    kit: Option<ResMut<Kit>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut rng: Local<Option<Rng>>,
    cars: Query<&Car, With<Player>>,
) {
    let (Some(mut kit), Ok(car)) = (kit, cars.single()) else {
        asked.clear();
        return;
    };
    let rng = rng.get_or_insert_with(Rng::random);
    let lap = track.length();
    let here = car.along.unwrap_or(0.0);
    for rain in asked.read() {
        for _ in 0..rain.count {
            let s = (here + rng.range(18.0, 90.0)).rem_euclid(lap);
            let spot = track.spot_at(s);
            let at = spot.pos + spot.right * rng.range(-1.3, 1.3);
            let piece = PropSpot {
                kind: PropKind::Cow,
                at,
                s,
                yaw: rng.range(0.0, std::f32::consts::TAU),
                group: 0,
            };
            let height = rng.range(10.0, 26.0);
            let cow = spawn_prop(
                &mut commands,
                &mut kit,
                &mut materials,
                &track,
                &piece,
                rng,
                Some(height),
            );
            commands.entity(cow).insert(Rained { left: RAINED_FOR });
        }
    }
}

/// Rained cows go, once they have stood for their time and are behind.
fn retire_rain(
    mut commands: Commands,
    time: Res<Time>,
    cars: Query<&Transform, Striker>,
    mut cows: Query<(Entity, &mut Rained, &Prop, &Transform), Without<Player>>,
) {
    let dt = time.delta_secs();
    let car = cars.single().ok().map(|at| at.translation);
    for (entity, mut rained, prop, transform) in &mut cows {
        rained.left -= dt;
        let clear = car.is_none_or(|car| car.distance(transform.translation) > OUT_OF_SIGHT);
        if rained.left <= 0.0 && clear && matches!(prop.state, State::Standing) {
            commands.entity(entity).despawn();
        }
    }
}

/// The car against the props.
#[allow(clippy::too_many_arguments)]
fn hit(
    time: Res<Time>,
    mut cars: Query<(&Transform, &mut Car, &Air), Striker>,
    mut props: Query<(&mut Prop, &Transform)>,
    mut previous: Local<Option<Vec3>>,
    mut rng: Local<Option<Rng>>,
    mut alley: ResMut<Alley>,
    mut effects: Effects,
) {
    let Ok((at, mut car, air)) = cars.single_mut() else {
        *previous = None;
        return;
    };
    let now = time.elapsed_secs();
    // Where it was a step ago, and not where it was before it was put back on the
    // grid: a car that has been taken somewhere else has not driven the whole way
    // there, through everything in between.
    let from = previous
        .filter(|before| before.distance(at.translation) < 3.0)
        .unwrap_or(at.translation);
    *previous = Some(at.translation);
    let heading = level(*at.forward());
    // The car is a capsule from tail to nose, swept along where it has been.
    let nose = at.translation + heading * 0.34;
    let tail = at.translation - heading * 0.30;
    let rng = rng.get_or_insert_with(Rng::random);
    let mut slow = 0.0f32;
    for (mut prop, transform) in &mut props {
        if !matches!(prop.state, State::Standing) {
            continue;
        }
        let home = transform.translation;
        // Sweep the nose along its path and the tail as well.
        let reach = prop.radius + CAR_REACH;
        let near = |a: Vec3, b: Vec3| {
            let p = nearest_on(a, b, home);
            (p - home).reject_from(Vec3::Y).length() < reach
        };
        if !(near(from + heading * 0.34, nose)
            || near(from - heading * 0.30, tail)
            || near(from, at.translation))
        {
            continue;
        }
        // Flying over it: a car well above a thing does not hit it, though a
        // balloon hangs high enough to be hit from most of a jump.
        let clear = if prop.kind == PropKind::Balloon {
            2.2
        } else {
            prop.height + 0.15
        };
        if air.height > clear {
            continue;
        }
        let vel = car.velocity;
        if knock(
            &mut prop,
            transform,
            vel,
            rng,
            &mut alley,
            now,
            &mut effects,
            true,
        ) {
            slow = slow.max(smash_of(prop.kind).slow);
        }
    }
    if slow > 0.0 {
        car.velocity *= 1.0 - slow;
    }
    // A strike: all ten, and quickly.
    let done: Vec<u16> = alley
        .0
        .iter()
        .filter(|(_, n, first)| *n >= 10 && now - first < 2.5)
        .map(|(g, _, _)| *g)
        .collect();
    for group in done {
        alley.0.retain(|(g, _, _)| *g != group);
        effects.say(Announce::gold("say.strike"));
        effects.points(Points {
            amount: 1_000,
            what: "pop.strike",
            at: Some(at.translation + Vec3::Y * 0.9),
        });
        effects.sound(Sfx::new(SfxKind::Tada));
        effects.burst(
            Burst::new(Kind::Confetti, at.translation + Vec3::Y, 70)
                .toward(Vec3::Y, 1.0)
                .speed(7.0)
                .size(0.1),
        );
    }
}

/// Everything in the air: fall, spin, bounce, knock what it lands on, go.
#[allow(clippy::too_many_arguments)]
fn fly(
    mut commands: Commands,
    time: Res<Time>,
    track: Res<Track>,
    mut props: Query<(Entity, &mut Prop, &mut Transform)>,
    mut alley: ResMut<Alley>,
    mut rng: Local<Option<Rng>>,
    mut effects: Effects,
) {
    let dt = time.delta_secs().min(1.0 / 30.0);
    if dt <= 0.0 {
        return;
    }
    let rng = rng.get_or_insert_with(Rng::random);
    let now = time.elapsed_secs();
    // Where the things that are moving are, for the ones standing to be hit by.
    let mut moving: Vec<(u16, Vec3, Vec3)> = Vec::new();
    for (entity, mut prop, mut transform) in &mut props {
        let (kind, rest, group) = (prop.kind, prop.rest, prop.group);
        if let State::Falling { vy } = &mut prop.state {
            *vy -= GRAVITY * dt;
            transform.translation.y += *vy * dt;
            transform.rotate_y(2.0 * dt);
            let ground = track.ground_from(transform.translation, None).height;
            if transform.translation.y <= ground {
                transform.translation.y = ground;
                let at = transform.translation;
                prop.state = State::Standing;
                effects.sound(Sfx::new(SfxKind::Thud).gain(0.9));
                effects.burst(
                    Burst::new(Kind::Puff, at + Vec3::Y * 0.1, 8)
                        .toward(Vec3::Y * 0.2, 1.0)
                        .speed(2.5)
                        .size(0.3)
                        .tint(Tint::Smoke),
                );
                effects.jolt(Jolt(0.06));
            }
            continue;
        }
        let State::Flung { vel, spin, age } = &mut prop.state else {
            continue;
        };
        if *age >= FLIGHT {
            commands.entity(entity).despawn();
            continue;
        }
        *age += dt;
        vel.y -= GRAVITY * dt;
        *vel *= 1.0 - 0.25 * dt;
        transform.translation += *vel * dt;
        transform.rotate(Quat::from_euler(
            EulerRot::XYZ,
            spin.x * dt,
            spin.y * dt,
            spin.z * dt,
        ));
        let ground = track.ground_from(transform.translation, None).height + rest;
        if transform.translation.y < ground && vel.y < 0.0 {
            transform.translation.y = ground;
            vel.y = -vel.y * 0.38;
            vel.x *= 0.72;
            vel.z *= 0.72;
            *spin *= 0.6;
            if vel.y < 0.8 {
                vel.y = 0.0;
                *spin *= 0.5;
            }
        }
        // The last half second: shrink away.
        if *age > FLIGHT - 0.5 {
            transform.scale = Vec3::splat(((FLIGHT - *age) / 0.5).clamp(0.05, 1.0));
        }
        if vel.length() > 2.0
            && matches!(
                kind,
                PropKind::Pin | PropKind::Crate | PropKind::Melon | PropKind::Cow | PropKind::Duck
            )
        {
            moving.push((group, transform.translation, *vel));
        }
    }
    if moving.is_empty() {
        return;
    }
    // A pin that hits a pin, a box that hits a box.
    for (_, mut prop, transform) in &mut props {
        if !matches!(prop.state, State::Standing) {
            continue;
        }
        let home = transform.translation;
        let hit = moving.iter().find(|(group, at, _)| {
            *group == prop.group
                && (*at - home).reject_from(Vec3::Y).length() < prop.radius + 0.18
                && prop.kind != PropKind::Balloon
        });
        if let Some((_, _, vel)) = hit {
            let pushed = *vel * 0.8;
            knock(
                &mut prop,
                &transform,
                pushed,
                rng,
                &mut alley,
                now,
                &mut effects,
                false,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_point_on_a_swept_path_is_found_and_clamped() {
        let (a, b) = (Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 1.0, -10.0));
        let p = nearest_on(a, b, Vec3::new(0.3, 0.0, -4.0));
        assert!((p - Vec3::new(0.0, 1.0, -4.0)).length() < 1e-4);
        assert_eq!(
            nearest_on(a, b, Vec3::new(0.0, 0.0, 5.0)),
            a,
            "before the start"
        );
        assert_eq!(
            nearest_on(a, b, Vec3::new(0.0, 0.0, -50.0)),
            b,
            "past the end"
        );
        assert_eq!(nearest_on(a, a, Vec3::ONE), a, "a path of nothing");
    }

    /// A prop of `kind` standing at `at`.
    fn standing(kind: PropKind, at: Vec3) -> (Prop, Transform) {
        (
            Prop {
                kind,
                group: 0,
                radius: 0.2,
                height: 0.5,
                home: at,
                rest: 0.0,
                state: State::Standing,
            },
            Transform::from_translation(at),
        )
    }

    #[test]
    fn a_road_nothing_touched_is_not_built_again_and_one_that_was_touched_is() {
        use crate::lap::LapFinished;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<LapFinished>()
            .add_message::<crate::Reset>()
            .insert_resource(Built(7, 3))
            .add_systems(Update, rebuild_on_lap);
        let mut ids = Vec::new();
        for i in 0..3 {
            let (prop, at) = standing(PropKind::Cone, Vec3::new(i as f32 * 2.0, 0.0, 0.0));
            ids.push(app.world_mut().spawn((prop, at)).id());
        }
        app.world_mut().write_message(crate::Reset);
        app.update();
        assert_eq!(app.world().resource::<Built>().0, 7, "nothing was touched");
        // One was smashed and is gone.
        app.world_mut().entity_mut(ids[1]).despawn();
        app.world_mut().write_message(crate::Reset);
        app.update();
        assert_eq!(app.world().resource::<Built>().0, 0, "one was");
        // A cow that fell out of the sky is not part of what was built either.
        app.insert_resource(Built(8, 2));
        app.update();
        assert_eq!(app.world().resource::<Built>().0, 8);
        let (prop, at) = standing(PropKind::Cow, Vec3::new(9.0, 0.0, 0.0));
        app.world_mut()
            .spawn((prop, at, Rained { left: RAINED_FOR }));
        app.world_mut().write_message(crate::Reset);
        app.update();
        assert_eq!(app.world().resource::<Built>().0, 0, "rain is not the road");
    }

    #[test]
    fn a_rained_cow_goes_when_its_time_is_up_and_it_is_out_of_the_way() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_systems(Update, retire_rain);
        app.world_mut()
            .spawn((Player, Transform::from_translation(Vec3::ZERO)));
        let cow = |app: &mut App, at: Vec3, left: f32| {
            let (prop, transform) = standing(PropKind::Cow, at);
            app.world_mut()
                .spawn((prop, transform, Rained { left }))
                .id()
        };
        let far = cow(&mut app, Vec3::new(100.0, 0.0, 0.0), 0.0);
        let near = cow(&mut app, Vec3::new(5.0, 0.0, 0.0), 0.0);
        let fresh = cow(&mut app, Vec3::new(100.0, 0.0, 5.0), 30.0);
        app.update();
        assert!(app.world().get_entity(far).is_err(), "a stale one, behind");
        assert!(
            app.world().get_entity(near).is_ok(),
            "not in the car's face"
        );
        assert!(app.world().get_entity(fresh).is_ok(), "not yet");
    }

    /// The car against the props, with one prop between where it was and where
    /// it is.
    fn hit_across(jump: f32) -> bool {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<Burst>()
            .add_message::<Sfx>()
            .add_message::<Points>()
            .add_message::<Announce>()
            .add_message::<Jolt>()
            .init_resource::<Alley>()
            .add_systems(Update, hit);
        let car = app
            .world_mut()
            .spawn((
                Player,
                Car::default(),
                Air::default(),
                Transform::from_translation(Vec3::ZERO).looking_to(Vec3::X, Vec3::Y),
            ))
            .id();
        let (prop, at) = standing(PropKind::Cone, Vec3::new(jump / 2.0, 0.0, 0.0));
        let cone = app.world_mut().spawn((prop, at)).id();
        app.update();
        // Then the car is somewhere else: driven there, or put there.
        app.world_mut()
            .get_mut::<Transform>(car)
            .unwrap()
            .translation = Vec3::new(jump, 0.0, 0.0);
        app.update();
        !matches!(
            app.world().get::<Prop>(cone).unwrap().state,
            State::Standing
        )
    }

    #[test]
    fn a_car_drives_through_what_is_in_its_way_and_is_not_carried_through_it() {
        assert!(hit_across(1.0), "a step of the drive hits what is on it");
        assert!(
            !hit_across(40.0),
            "a car put back on the grid smashed what was between"
        );
    }

    #[test]
    fn every_kind_has_a_smash_that_scores_and_a_cow_says_so() {
        for kind in [
            PropKind::Cone,
            PropKind::Pin,
            PropKind::Cow,
            PropKind::Duck,
            PropKind::Melon,
            PropKind::Crate,
            PropKind::Balloon,
        ] {
            let smash = smash_of(kind);
            assert!(
                smash.points > 0 && smash.slow < 0.2 && smash.pieces > 0,
                "{kind:?}"
            );
            assert!(!smash.label.is_empty());
        }
        assert!(smash_of(PropKind::Cow).say.is_some());
        assert!(smash_of(PropKind::Cow).points > smash_of(PropKind::Cone).points);
        assert_eq!(
            smash_of(PropKind::Balloon).slow,
            0.0,
            "a balloon does not slow a car"
        );
    }

    #[test]
    fn a_balloon_is_the_colour_of_where_it_hangs_and_stays_that_colour() {
        let at = Vec3::new(12.0, 0.0, -3.0);
        assert_eq!(balloon_colour(at), balloon_colour(at));
        let seen: std::collections::HashSet<_> = (0..40)
            .map(|i| format!("{:?}", balloon_colour(Vec3::new(i as f32 * 1.3, 0.0, 0.0))))
            .collect();
        assert!(seen.len() > 3, "a balloon arch has more than one colour");
    }
}
