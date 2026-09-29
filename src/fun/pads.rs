//! Pads in the road, and boxes over it.
//!
//! A **boost pad** is a patch of road whose chevrons stream the way the lap
//! runs; drive over one and the car is thrown forward and the engine is given a
//! second and a half of everything it has. A **jump pad** is a ring of light
//! across the whole road; drive over one and the car goes up. A **mystery box** is
//! a spinning cube of every colour hanging over the road in threes, and opening
//! one does something to you that nobody, including the game, has decided yet.
//!
//! They are entities with no physics of their own. Each is checked against the
//! car once a physics step, and what it does goes through [`Boost`] and
//! [`Air`], the same doors everything else uses. Neither pad is ever asked to
//! mean anything to a lap time, because there is no such lap: see
//! [`crate::lap::Why::Bonkers`].

use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    light::NotShadowCaster,
    math::Affine2,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use super::air::{Air, Launched};
use super::announcer::{Announce, Points};
use super::course::{Layout, PadKind, PadSpot};
use super::juice::Jolt;
use super::particles::{Burst, Kind, Tint};
use super::parts::{Kit, Shape, rainbow};
use super::tweak::Boost;
use super::{Fun, Silliness};
use crate::car::{Car, Player, level, step_seconds};
use crate::sound::{Sfx, SfxKind};
use crate::track::Track;

/// A pad's size, along the road and across it, in metres.
const BOOST: (f32, f32) = (3.4, 2.5);
const JUMP: (f32, f32) = (3.6, 3.3);
/// How close, along and across, the car has to be to be on one.
const BOOST_REACH: (f32, f32) = (1.7, 1.35);
const JUMP_REACH: (f32, f32) = (1.8, 1.7);
/// Above the road, clear of it and of the rails.
const LIFT: f32 = 0.04;
/// Seconds before a pad works again.
const REARM: f32 = 1.4;
/// A box's reach, and how long before it comes back.
const BOX_REACH: f32 = 0.85;
const BOX_RETURNS: f32 = 16.0;
/// Boost: what it adds to the speed at once (at the shipped car's pace), how
/// hard the engine is pushed and for how long.
const KICK: f32 = 5.5;
const BOOST_POWER: f32 = 1.0;
const BOOST_FOR: f32 = 1.8;
/// Jump: the launch speed, at the shipped car's pace.
const LAUNCH: f32 = 9.6;

/// The chevrons and the ring, painted once, and the paint that wears them.
#[derive(Resource, Default)]
struct Art {
    boost: Option<Handle<StandardMaterial>>,
    jump: Option<Handle<StandardMaterial>>,
    cube: Option<Handle<StandardMaterial>>,
}

/// What a pad entity is.
#[derive(Component)]
struct PadMark;

/// Everything in the road that this module put there.
type Furniture = Or<(With<PadMark>, With<MysteryBox>)>;

/// The player's car, when it is being driven.
type Driven = (With<Player>, Without<crate::countdown::Held>);

/// A mystery box: where it is, and whether it is waiting to come back.
#[derive(Component)]
struct MysteryBox {
    spin: f32,
    back_in: f32,
    /// Where it hangs, before it bobs.
    base: f32,
}

/// A box has been opened.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct Opened;

/// Which pads have just been driven over, as seconds until they work again.
#[derive(Resource, Default)]
struct Cooling(Vec<f32>);

/// Which edition of the course what is built came from.
#[derive(Resource, Default)]
struct Built(u32);

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Art>()
        .init_resource::<Cooling>()
        .init_resource::<Built>()
        .add_message::<Opened>()
        .add_systems(Update, (build, animate).chain().run_if(super::bonkers))
        .add_systems(Update, clear.run_if(not(super::bonkers)))
        .add_systems(FixedUpdate, (drive_over, open_boxes).run_if(super::bonkers));
}

/// Bright chevrons, pointing toward increasing `v`, in rows.
fn chevrons() -> Image {
    let (w, h) = (64usize, 128usize);
    let mut data = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let u = x as f32 / w as f32;
            let v = y as f32 / h as f32;
            // Lines of constant `v + |u - 1/2| * 0.6` are chevrons pointing up
            // the road; three to a pad, with soft edges.
            let phase = (3.0 * (v + (u - 0.5).abs() * 0.6)).rem_euclid(1.0);
            let band = smooth(phase / 0.07) * smooth((0.42 - phase) / 0.07);
            let edge = smooth(u / 0.10) * smooth((1.0 - u) / 0.10);
            data.extend_from_slice(&[255, 255, 255, (band * edge * 255.0) as u8]);
        }
    }
    image(
        w,
        h,
        data,
        ImageAddressMode::ClampToEdge,
        ImageAddressMode::Repeat,
    )
}

/// Concentric rings, bright at the middle, for the jump pad.
fn rings() -> Image {
    let size = 96usize;
    let mut data = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let dy = (y as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let r = (dx * dx + dy * dy).sqrt();
            let ring = |at: f32, width: f32| smooth(1.0 - (r - at).abs() / width);
            let fill = smooth((0.32 - r) / 0.30) * 0.65;
            let alpha =
                (ring(0.45, 0.10) + ring(0.75, 0.09) + fill).min(1.0) * smooth((1.0 - r) / 0.1);
            data.extend_from_slice(&[255, 255, 255, (alpha * 255.0) as u8]);
        }
    }
    image(
        size,
        size,
        data,
        ImageAddressMode::ClampToEdge,
        ImageAddressMode::ClampToEdge,
    )
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn image(w: usize, h: usize, data: Vec<u8>, u: ImageAddressMode, v: ImageAddressMode) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: w as u32,
            height: h as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: u,
        address_mode_v: v,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// A flat patch of `length` by `width` laid on the road at `spot`, following
/// the ground, with texture coordinates running across and along.
fn patch(track: &Track, pad: &PadSpot, length: f32, width: f32) -> Mesh {
    let (cols, rows) = (4usize, 8usize);
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    for row in 0..=rows {
        let along = row as f32 / rows as f32;
        for col in 0..=cols {
            let across = col as f32 / cols as f32;
            let p = pad.at
                + pad.tangent * ((along - 0.5) * length)
                + pad.right * ((across - 0.5) * width);
            let y = track.ground_from(p, Some(pad.s)).height + LIFT;
            positions.push([p.x, y, p.z]);
            uvs.push([across, along]);
        }
    }
    let mut indices = Vec::new();
    let stride = (cols + 1) as u32;
    for row in 0..rows as u32 {
        for col in 0..cols as u32 {
            let a = row * stride + col;
            indices.extend([a, a + stride, a + 1, a + 1, a + stride, a + stride + 1]);
        }
    }
    let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

fn glowing(colour: Color, power: f32, texture: Option<Handle<Image>>) -> StandardMaterial {
    let c = colour.to_linear();
    StandardMaterial {
        base_color: Color::linear_rgba(c.red * power, c.green * power, c.blue * power, 0.95),
        base_color_texture: texture,
        unlit: true,
        alpha_mode: AlphaMode::Add,
        double_sided: true,
        cull_mode: None,
        depth_bias: 14.0,
        ..default()
    }
}

/// Put the pads and the boxes on the road, again when the course is new.
#[allow(clippy::too_many_arguments)]
fn build(
    mut commands: Commands,
    layout: Res<Layout>,
    track: Res<Track>,
    kit: Option<ResMut<Kit>>,
    mut built: ResMut<Built>,
    mut cooling: ResMut<Cooling>,
    mut art: ResMut<Art>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    old: Query<Entity, Furniture>,
) {
    if built.0 == layout.edition {
        return;
    }
    let Some(_) = kit.as_ref() else {
        return;
    };
    built.0 = layout.edition;
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let boost_art = art
        .boost
        .get_or_insert_with(|| {
            materials.add(glowing(
                Color::srgb(1.0, 0.55, 0.05),
                3.4,
                Some(images.add(chevrons())),
            ))
        })
        .clone();
    let jump_art = art
        .jump
        .get_or_insert_with(|| {
            materials.add(glowing(
                Color::srgb(0.35, 0.75, 1.0),
                3.2,
                Some(images.add(rings())),
            ))
        })
        .clone();
    let cube = art
        .cube
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: Color::BLACK,
                emissive: LinearRgba::rgb(2.0, 2.0, 2.0),
                double_sided: true,
                cull_mode: None,
                fog_enabled: false,
                ..default()
            })
        })
        .clone();
    cooling.0 = vec![0.0; layout.course.pads.len()];
    for pad in &layout.course.pads {
        let (size, art) = match pad.kind {
            PadKind::Boost => (BOOST, boost_art.clone()),
            PadKind::Jump => (JUMP, jump_art.clone()),
        };
        commands.spawn((
            PadMark,
            Mesh3d(meshes.add(patch(&track, pad, size.0, size.1))),
            MeshMaterial3d(art),
            NotShadowCaster,
            bevy::camera::visibility::NoFrustumCulling,
        ));
    }
    let Some(kit) = kit else {
        return;
    };
    for spot in &layout.course.boxes {
        let ground = track.ground_from(spot.at, Some(spot.s)).height;
        commands
            .spawn((
                MysteryBox {
                    spin: spot.at.x * 3.0 + spot.at.z,
                    back_in: 0.0,
                    base: ground + 0.62,
                },
                Transform::from_translation(Vec3::new(spot.at.x, ground + 0.62, spot.at.z)),
                Visibility::Inherited,
            ))
            .with_children(|b| {
                b.spawn((
                    Mesh3d(kit.mesh(Shape::Cube)),
                    MeshMaterial3d(cube.clone()),
                    Transform::from_scale(Vec3::splat(0.42)),
                    NotShadowCaster,
                ));
                b.spawn((
                    Mesh3d(kit.mesh(Shape::Star)),
                    MeshMaterial3d(cube.clone()),
                    Transform::from_xyz(0.0, 0.42, 0.0).with_scale(Vec3::splat(0.36)),
                    NotShadowCaster,
                ));
            });
    }
}

/// Take it all away, when the game is not silly enough for it.
fn clear(mut commands: Commands, mut built: ResMut<Built>, old: Query<Entity, Furniture>) {
    if built.0 != 0 {
        built.0 = 0;
        for entity in &old {
            commands.entity(entity).despawn();
        }
    }
}

/// Stream the chevrons, pulse the rings, spin and recolour the boxes.
fn animate(
    time: Res<Time<Real>>,
    fun: Res<Fun>,
    art: Res<Art>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut boxes: Query<(&mut MysteryBox, &mut Transform, &mut Visibility)>,
    time_game: Res<Time>,
) {
    let t = time.elapsed_secs();
    let calm = fun.calm;
    if let Some(mut boost) = art.boost.as_ref().and_then(|h| materials.get_mut(h)) {
        boost.uv_transform =
            Affine2::from_translation(Vec2::new(0.0, -t * if calm { 0.0 } else { 1.1 }));
    }
    if let Some(mut jump) = art.jump.as_ref().and_then(|h| materials.get_mut(h)) {
        let s = 1.0 + if calm { 0.0 } else { 0.09 * (t * 5.0).sin() };
        jump.uv_transform = Affine2::from_translation(Vec2::splat(0.5))
            * Affine2::from_scale(Vec2::splat(s))
            * Affine2::from_translation(Vec2::splat(-0.5));
    }
    if let Some(mut cube) = art.cube.as_ref().and_then(|h| materials.get_mut(h)) {
        // The boxes cycle through the rainbow, unless that is not wanted.
        let c = rainbow(if calm { 0.6 } else { t * 0.35 }, 0.58).to_linear();
        cube.emissive = LinearRgba::rgb(c.red * 3.0, c.green * 3.0, c.blue * 3.0);
    }
    let dt = time_game.delta_secs();
    for (mut spot, mut transform, mut visibility) in &mut boxes {
        if spot.back_in > 0.0 {
            spot.back_in = (spot.back_in - dt).max(0.0);
            visibility.set_if_neq(if spot.back_in > 0.0 {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            });
        }
        spot.spin += dt * if calm { 0.4 } else { 2.2 };
        transform.rotation = Quat::from_euler(EulerRot::YXZ, spot.spin, spot.spin * 0.6, 0.0);
        let bob = if calm {
            0.0
        } else {
            0.07 * (t * 2.6 + spot.spin).sin()
        };
        transform.translation.y = spot.base + bob;
    }
}

/// Whether a car at `car` is on a pad, given how far it may be along and across.
fn covers(pad: &PadSpot, car: Vec3, reach: (f32, f32)) -> bool {
    let d = car - pad.at;
    d.dot(pad.tangent).abs() < reach.0 && d.dot(pad.right).abs() < reach.1
}

/// Boost and jump.
#[allow(clippy::too_many_arguments)]
fn drive_over(
    layout: Res<Layout>,
    fun: Res<Fun>,
    race: Option<Res<crate::local::LocalRace>>,
    mut cooling: ResMut<Cooling>,
    mut boost: ResMut<Boost>,
    mut cars: Query<(&Transform, &mut Car, &mut Air), Driven>,
    mut said: MessageWriter<Announce>,
    mut points: MessageWriter<Points>,
    mut launched: MessageWriter<Launched>,
    mut bursts: MessageWriter<Burst>,
    mut jolts: MessageWriter<Jolt>,
    mut sounds: MessageWriter<Sfx>,
) {
    if !crate::local::solo(race) || cooling.0.len() != layout.course.pads.len() {
        return;
    }
    let dt = step_seconds();
    for cool in &mut cooling.0 {
        *cool = (*cool - dt).max(0.0);
    }
    let k = fun.speed.scale();
    let Ok((at, mut car, mut air)) = cars.single_mut() else {
        return;
    };
    if air.flying {
        return;
    }
    for (i, pad) in layout.course.pads.iter().enumerate() {
        let reach = match pad.kind {
            PadKind::Boost => BOOST_REACH,
            PadKind::Jump => JUMP_REACH,
        };
        if cooling.0[i] > 0.0 || car.velocity.length() < 2.0 || !covers(pad, at.translation, reach)
        {
            continue;
        }
        cooling.0[i] = REARM;
        let heading = level(*at.forward());
        match pad.kind {
            PadKind::Boost => {
                boost.fire(BOOST_POWER, BOOST_FOR);
                car.velocity += heading * KICK * k;
                said.write(Announce::big("say.boost"));
                points.write(Points {
                    amount: 75,
                    what: "pop.boost",
                    at: Some(at.translation + Vec3::Y * 0.7),
                });
                sounds.write(Sfx::new(SfxKind::Zoom).gain(0.9));
                jolts.write(Jolt(0.30));
                if !fun.calm {
                    bursts.write(
                        Burst::new(Kind::Spark, at.translation + Vec3::Y * 0.1, 26)
                            .toward(-heading + Vec3::Y * 0.5, 0.7)
                            .speed(6.0)
                            .size(0.07)
                            .tint(Tint::Fire),
                    );
                }
            }
            PadKind::Jump => {
                air.launch(at.translation.y, LAUNCH * k);
                launched.write(Launched { power: 1.0 });
                said.write(Announce::big("say.jump"));
                points.write(Points {
                    amount: 50,
                    what: "pop.jump",
                    at: Some(at.translation + Vec3::Y * 0.7),
                });
                sounds.write(Sfx::new(SfxKind::SlideUp).gain(0.8));
                if !fun.calm {
                    bursts.write(
                        Burst::new(Kind::Star, at.translation + Vec3::Y * 0.2, 10)
                            .toward(Vec3::Y, 0.5)
                            .speed(4.0)
                            .size(0.5)
                            .tint(Tint::Ice),
                    );
                }
            }
        }
    }
}

/// Open any box the car is in.
fn open_boxes(
    fun: Res<Fun>,
    mut boxes: Query<(&mut MysteryBox, &GlobalTransform)>,
    cars: Query<(&Transform, &Air), Driven>,
    mut opened: MessageWriter<Opened>,
    mut points: MessageWriter<Points>,
    mut bursts: MessageWriter<Burst>,
    mut sounds: MessageWriter<Sfx>,
) {
    if fun.level < Silliness::Bonkers {
        return;
    }
    let Ok((at, air)) = cars.single() else {
        return;
    };
    // Where the car is, which in the air is above the road it is being held to.
    let car = at.translation + Vec3::Y * (0.4 + air.height);
    for (mut spot, place) in &mut boxes {
        if spot.back_in > 0.0 {
            continue;
        }
        let here = place.translation();
        if here.distance(car) > BOX_REACH {
            continue;
        }
        spot.back_in = BOX_RETURNS;
        opened.write(Opened);
        points.write(Points {
            amount: 100,
            what: "pop.box",
            at: Some(here),
        });
        sounds.write(Sfx::new(SfxKind::Coin).gain(0.8));
        sounds.write(Sfx::new(SfxKind::Sparkle).gain(0.6));
        bursts.write(
            Burst::new(Kind::Confetti, here, 40)
                .toward(Vec3::Y, 1.0)
                .speed(5.0)
                .size(0.09)
                .tint(Tint::Party),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(at: Vec3, tangent: Vec3) -> PadSpot {
        PadSpot {
            kind: PadKind::Boost,
            at,
            tangent,
            right: tangent.cross(Vec3::Y),
            s: 100.0,
        }
    }

    #[test]
    fn a_box_is_opened_by_a_car_that_reaches_it_and_not_by_one_flying_over() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(Fun::of(&crate::settings::Settings::default()))
            .add_message::<Opened>()
            .add_message::<Points>()
            .add_message::<Burst>()
            .add_message::<Sfx>()
            .add_systems(Update, open_boxes);
        let place = Vec3::new(0.0, 0.8, 0.0);
        let boxed = app
            .world_mut()
            .spawn((
                MysteryBox {
                    spin: 0.0,
                    back_in: 0.0,
                    base: 0.8,
                },
                GlobalTransform::from_translation(place),
            ))
            .id();
        let car = app
            .world_mut()
            .spawn((
                Player,
                Transform::from_translation(Vec3::ZERO),
                Air::default(),
            ))
            .id();
        let closed = |app: &App| app.world().get::<MysteryBox>(boxed).unwrap().back_in > 0.0;
        // Well over it, on a jump.
        app.world_mut().get_mut::<Air>(car).unwrap().height = 6.0;
        app.update();
        assert!(!closed(&app), "a car flying high over it opened it");
        // Down at its height, driving through it or coming down on it.
        app.world_mut().get_mut::<Air>(car).unwrap().height = 0.4;
        app.update();
        assert!(closed(&app), "a car that reached it did not");
    }

    #[test]
    fn a_pad_covers_the_car_that_is_on_it_and_nobody_else() {
        let p = pad(Vec3::new(10.0, 1.0, 5.0), Vec3::NEG_Z);
        assert!(covers(&p, Vec3::new(10.0, 1.0, 5.5), BOOST_REACH));
        assert!(covers(&p, Vec3::new(10.9, 1.0, 4.0), BOOST_REACH));
        assert!(
            !covers(&p, Vec3::new(10.0, 1.0, 8.0), BOOST_REACH),
            "ahead of it"
        );
        assert!(
            !covers(&p, Vec3::new(12.5, 1.0, 5.0), BOOST_REACH),
            "beside it"
        );
    }

    #[test]
    fn the_chevrons_point_up_the_road_and_the_rings_are_round() {
        let art = chevrons();
        let data = art.data.as_ref().unwrap();
        let alpha = |x: usize, y: usize| data[(y * 64 + x) * 4 + 3];
        // Somewhere is bright, somewhere is clear, and the edges of the road fade.
        let lit = (0..128).filter(|y| alpha(32, *y) > 200).count();
        assert!((10..64).contains(&lit), "{lit} rows lit down the middle");
        assert!((0..128).all(|y| alpha(0, y) < 30), "the edge is clear");
        // A chevron's arms trail behind its tip: at the same row, the middle is
        // further along the road than the sides.
        let tip = (0..128).find(|y| alpha(32, *y) > 200).unwrap();
        let side = (0..128).find(|y| alpha(12, *y) > 200).unwrap();
        assert_ne!(tip, side, "the arms are not level with the tip");
        let ring = rings();
        let d = ring.data.as_ref().unwrap();
        let a = |x: usize, y: usize| d[(y * 96 + x) * 4 + 3];
        assert_eq!(a(48, 20), a(20, 48), "round, not oval");
        assert!(a(48, 48) > 100 && a(0, 0) < 10);
    }

    #[test]
    fn a_patch_lies_on_the_road_it_is_put_on() {
        let circuit = crate::track::all_circuits()
            .iter()
            .find(|c| c.id == "monza")
            .unwrap();
        let track = Track::new(circuit);
        let course = super::super::course::plan(&track);
        let pad = course.pads.first().expect("a pad");
        let mesh = patch(&track, pad, BOOST.0, BOOST.1);
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions");
        };
        assert_eq!(positions.len(), 5 * 9);
        for p in positions {
            let here = Vec3::from(*p);
            let ground = track.ground_from(here, Some(pad.s)).height;
            assert!(
                (here.y - ground - LIFT).abs() < 0.01,
                "off the road by {}",
                here.y - ground
            );
        }
    }
}
