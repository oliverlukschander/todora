//! The lights.
//!
//! The road becomes a club. The camera gets a stack of post-processing —
//! bloom, so anything brighter than white glows; a colour grade that turns the
//! whole world through the rainbow and flashes a little on every beat; a chromatic
//! split at the edges; a vignette that closes in with speed; and a barrel
//! distortion that a boost stretches — and the circuit is fitted with neon: rails
//! along both kerbs and a dashed line down the middle whose rainbow scrolls
//! along the road, pylons that chase, laser beams that sweep, and a pool of light
//! under the car.
//!
//! The rainbow moves without anything being rebuilt. The rails are a mesh with
//! its texture coordinate running along the lap, painted with one strip of
//! rainbow, and the whole scrolling is the material's `uv_transform` shifting by
//! a little every frame. The beat pulses the material's brightness, which is
//! above 1 on purpose: that is what bloom picks up.
//!
//! Nothing here flashes when reduced motion is on: the grade holds still, the
//! aberration is off, the lasers stand and the rails keep their glow but not their
//! pulse.

use bevy::{
    asset::RenderAssetUsages,
    camera::{Hdr, visibility::NoFrustumCulling},
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    light::NotShadowCaster,
    math::Affine2,
    mesh::{Indices, PrimitiveTopology},
    post_process::{
        bloom::Bloom,
        effect_stack::{ChromaticAberration, LensDistortion, Vignette},
    },
    prelude::*,
    render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        view::ColorGrading,
    },
};

use super::beat::Beat;
use super::mount::Pose;
use super::parts::{Kit, Shape, rainbow};
use super::{Fun, rng::Rng, tweak::Boost};
use crate::car::Player;
use crate::track::{ROAD_HALF, Track};

/// How many metres of road one turn of the rainbow covers.
const TILE: f32 = 42.0;
/// How far the rails stand from the centreline: just outside the kerbs.
const RAIL_AT: f32 = ROAD_HALF + 0.12;
const RAIL_WIDTH: f32 = 0.16;
/// Clear of the grass and the kerb, which the loft sits on.
const LIFT: f32 = 0.03;
/// Chasing pylons, metres apart along each side.
const PYLON_EVERY: f32 = 22.0;
const PYLON_STEPS: usize = 8;
/// Beams in the light show, and how far up they reach.
const BEAMS: usize = 10;
const BEAM_LENGTH: f32 = 70.0;

/// What is on the road, and what it is made of.
#[derive(Resource, Default)]
struct Neon {
    /// The rainbow the rails are painted with.
    gradient: Option<Handle<Image>>,
    rails: Option<Handle<StandardMaterial>>,
    halo: Option<Handle<Image>>,
    ramp: Vec<Handle<StandardMaterial>>,
    beams: Vec<Handle<StandardMaterial>>,
    glow: Option<Handle<StandardMaterial>>,
}

/// The parent of every piece of neon, so it can all go at once.
#[derive(Component)]
struct NeonRoot;

/// A pylon, and its place along the road, which is where its colour comes from.
#[derive(Component)]
struct Pylon(usize);

/// A laser: how it turns, and its own phase.
#[derive(Component)]
struct Laser {
    phase: f32,
    rate: f32,
    lean: f32,
}

/// The pool of light under the car.
#[derive(Component)]
struct Underglow;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Neon>().add_systems(
        Update,
        (
            attach_stack,
            animate_stack,
            build_neon,
            animate_neon,
            glow_under_the_car,
        )
            .chain(),
    );
}

/// The camera's effects, on while the neon is and off when it is not.
fn attach_stack(
    mut commands: Commands,
    fun: Res<Fun>,
    cameras: Query<(Entity, Has<Bloom>), With<Camera3d>>,
) {
    for (camera, has) in &cameras {
        if fun.neon && !has {
            commands.entity(camera).insert((
                Bloom {
                    intensity: 0.20,
                    low_frequency_boost: 0.65,
                    ..Bloom::NATURAL
                },
                ColorGrading::default(),
                ChromaticAberration {
                    intensity: 0.0,
                    ..default()
                },
                Vignette {
                    intensity: 0.0,
                    radius: 0.9,
                    smoothness: 2.2,
                    ..default()
                },
                LensDistortion {
                    intensity: 0.0,
                    ..default()
                },
            ));
        } else if !fun.neon && has {
            // Bloom brings the high dynamic range render target with it, which the
            // shipped camera does not have and would not thank it for.
            commands.entity(camera).remove::<(
                Bloom,
                Hdr,
                ColorGrading,
                ChromaticAberration,
                Vignette,
                LensDistortion,
            )>();
        }
    }
}

/// Turn the world through the rainbow to the beat, and squeeze it with speed.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn animate_stack(
    fun: Res<Fun>,
    halt: Res<crate::pause::Halt>,
    beat: Res<Beat>,
    pose: Res<Pose>,
    boost: Res<Boost>,
    chaos: Res<super::events::Chaos>,
    time: Res<Time<Real>>,
    mut turned: Local<f32>,
    mut pushed_on: Local<f32>,
    mut cameras: Query<
        (
            &mut Bloom,
            &mut ColorGrading,
            &mut ChromaticAberration,
            &mut Vignette,
            &mut LensDistortion,
        ),
        With<Camera3d>,
    >,
) {
    let dt = time.delta_secs().min(0.1);
    let calm = fun.calm;
    let kick = if calm { 0.0 } else { beat.kick() };
    // The title and the replay are not being driven, whatever the car last did.
    let driven = !matches!(
        *halt,
        crate::pause::Halt::Title | crate::pause::Halt::Replay
    );
    let top = 24.0 * fun.speed.scale();
    let speed = if driven {
        (pose.speed.abs() / top).clamp(0.0, 1.4)
    } else {
        0.0
    };
    // A boost is felt as a swell over a moment and never a step, and not at all
    // by anyone who has asked for calm.
    let pushed = smoothed(
        &mut pushed_on,
        if calm || !driven {
            0.0
        } else {
            boost.strength()
        },
        dt,
    );
    let disco = chaos.strength(super::events::Effect::Disco);
    let fisheye = chaos.strength(super::events::Effect::Fisheye);
    // The hue is added up a frame at a time from how fast it is turning, and not
    // worked out from the time and the speed: the speed changes as a disco starts
    // and stops, and time is a large number, so the product jumps by turns.
    *turned += dt * (0.30 + 1.6 * disco);
    for (mut bloom, mut grade, mut aberration, mut vignette, mut lens) in &mut cameras {
        bloom.intensity = 0.20 + 0.10 * kick + 0.10 * pushed;
        // The world turns slowly through the rainbow, and lurches a little on
        // the beat. Reduced motion picks one hue and keeps it.
        grade.global.hue = if calm {
            0.9
        } else {
            *turned + 0.16 * kick + 0.25 * (std::f32::consts::TAU * beat.bar()).sin()
        };
        grade.global.exposure = -0.55 + 0.34 * kick + 0.3 * pushed;
        grade.global.post_saturation = 1.32;
        grade.midtones.contrast = 1.12;
        grade.highlights.gain = 1.05 + 0.1 * pushed;
        aberration.intensity = if calm {
            0.0
        } else {
            0.003 + 0.012 * kick + 0.018 * pushed + 0.004 * speed
        };
        // The world closes in as it gets faster.
        vignette.intensity = 0.25 + 0.55 * speed.min(1.0) + 0.2 * pushed;
        lens.intensity = if calm {
            0.0
        } else {
            0.10 * speed.min(1.0) + 0.35 * pushed + 0.9 * fisheye
        };
        lens.scale = 1.0 + 0.5 * lens.intensity.max(0.0);
    }
}

/// `now` moved toward `target` over a fraction of a second: a swell in place of
/// a step.
fn smoothed(now: &mut f32, target: f32, dt: f32) -> f32 {
    *now += (target - *now) * (1.0 - (-dt * 7.0).exp());
    *now
}

/// A rainbow, as a strip one pixel high.
fn gradient() -> Image {
    let width = 256usize;
    let mut data = Vec::with_capacity(width * 4 * 2);
    for _row in 0..2 {
        for x in 0..width {
            let c = rainbow(x as f32 / width as f32, 0.55).to_srgba();
            data.extend_from_slice(&[
                (c.red * 255.0) as u8,
                (c.green * 255.0) as u8,
                (c.blue * 255.0) as u8,
                255,
            ]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: width as u32,
            height: 2,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// A soft round glow: white, and see-through toward the edge, so it can be
/// tinted and added to whatever is under it without a rim.
fn soft_glow() -> Image {
    let size = 64usize;
    let mut data = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let dy = (y as f32 + 0.5) / size as f32 * 2.0 - 1.0;
            let r = (dx * dx + dy * dy).sqrt();
            // Bright at the middle, gone by the rim, gentle in between.
            let fall = (1.0 - r).clamp(0.0, 1.0);
            let alpha = fall * fall * (3.0 - 2.0 * fall);
            data.extend_from_slice(&[255, 255, 255, (alpha * 255.0) as u8]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: size as u32,
            height: size as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        ..default()
    });
    image
}

/// A ribbon along the lap `at` metres to the right of the centreline, `width`
/// wide, whose texture runs along the lap once per [`TILE`].
fn ribbon(track: &Track, at: f32, width: f32) -> Mesh {
    let spots: Vec<_> = track.spots().collect();
    let n = spots.len();
    let mut positions = Vec::with_capacity((n + 1) * 2);
    let mut uvs = Vec::with_capacity((n + 1) * 2);
    // One more pair than there are stations, so the last quad ends at the lap
    // length and not back at zero, and the texture does not smear at the line.
    for i in 0..=n {
        let spot = spots[i % n];
        let s = if i == n { track.length() } else { spot.s };
        for side in [-0.5f32, 0.5] {
            let p = spot.pos + spot.right * (at + side * width);
            let y = track.ground_from(p, Some(spot.s)).height + LIFT;
            positions.push([p.x, y, p.z]);
            uvs.push([s / TILE, if side < 0.0 { 0.0 } else { 1.0 }]);
        }
    }
    let mut indices = Vec::with_capacity(n * 6);
    for i in 0..n as u32 {
        let a = i * 2;
        indices.extend([a, a + 1, a + 2, a + 1, a + 3, a + 2]);
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

/// Dashes down the middle: `dash` metres of light, `gap` of dark.
fn dashes(track: &Track, width: f32, dash: f32, gap: f32) -> Mesh {
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    let lap = track.length();
    let mut start = 0.0;
    while start + dash < lap {
        for (k, s) in [start, start + dash * 0.5, start + dash]
            .into_iter()
            .enumerate()
        {
            let spot = track.spot_at(s);
            for side in [-0.5f32, 0.5] {
                let p = spot.pos + spot.right * (side * width);
                let y = track.ground_from(p, Some(spot.s)).height + LIFT;
                positions.push([p.x, y, p.z]);
                uvs.push([s / TILE, if side < 0.0 { 0.0 } else { 1.0 }]);
            }
            if k > 0 {
                let a = positions.len() as u32 - 4;
                indices.extend([a, a + 1, a + 2, a + 1, a + 3, a + 2]);
            }
        }
        start += dash + gap;
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

/// A colour brighter than the screen can show, which is what glows.
fn hot(colour: Color, power: f32) -> Color {
    let c = colour.to_linear();
    Color::linear_rgb(c.red * power, c.green * power, c.blue * power)
}

/// Put the neon on the road, and take it away again; rebuilt on a new circuit.
#[allow(clippy::too_many_arguments)]
fn build_neon(
    mut commands: Commands,
    fun: Res<Fun>,
    track: Res<Track>,
    kit: Option<ResMut<Kit>>,
    mut neon: ResMut<Neon>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    roots: Query<Entity, With<NeonRoot>>,
) {
    let wanted = fun.neon;
    let present = !roots.is_empty();
    if present && (!wanted || track.is_changed()) {
        for root in &roots {
            commands.entity(root).despawn();
        }
    }
    if !wanted || (present && !track.is_changed()) {
        return;
    }
    let Some(mut kit) = kit else {
        return;
    };
    let gradient_handle = neon
        .gradient
        .get_or_insert_with(|| images.add(gradient()))
        .clone();
    let rails = neon
        .rails
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: hot(Color::WHITE, 3.0),
                base_color_texture: Some(gradient_handle.clone()),
                unlit: true,
                double_sided: true,
                cull_mode: None,
                depth_bias: 12.0,
                ..default()
            })
        })
        .clone();
    if neon.ramp.is_empty() {
        for i in 0..PYLON_STEPS {
            let colour = hot(rainbow(i as f32 / PYLON_STEPS as f32, 0.55), 3.5);
            neon.ramp.push(materials.add(StandardMaterial {
                base_color: colour,
                unlit: true,
                ..default()
            }));
        }
        for i in 0..4 {
            let colour = rainbow(i as f32 * 0.27 + 0.02, 0.58).with_alpha(0.55);
            let c = colour.to_linear();
            neon.beams.push(materials.add(StandardMaterial {
                base_color: Color::linear_rgba(c.red * 2.6, c.green * 2.6, c.blue * 2.6, 0.5),
                unlit: true,
                alpha_mode: AlphaMode::Add,
                double_sided: true,
                cull_mode: None,
                fog_enabled: false,
                ..default()
            }));
        }
    }
    let pole = kit.paint(&mut materials, Color::srgb(0.06, 0.07, 0.10));
    let ramp = neon.ramp.clone();
    let beams = neon.beams.clone();
    let kit = &*kit;

    commands
        .spawn((NeonRoot, Transform::IDENTITY, Visibility::Inherited))
        .with_children(|root| {
            // Rails on both kerbs, and a dashed line down the middle.
            for side in [-1.0f32, 1.0] {
                root.spawn((
                    Mesh3d(meshes.add(ribbon(&track, side * RAIL_AT, RAIL_WIDTH))),
                    MeshMaterial3d(rails.clone()),
                    NotShadowCaster,
                    NoFrustumCulling,
                ));
            }
            root.spawn((
                Mesh3d(meshes.add(dashes(&track, 0.11, 1.3, 2.3))),
                MeshMaterial3d(rails.clone()),
                NotShadowCaster,
                NoFrustumCulling,
            ));

            // Pylons, alternating sides, a pole and a glowing orb each.
            let lap = track.length();
            let mut s = PYLON_EVERY * 0.5;
            let mut index = 0;
            while s < lap - 12.0 {
                // Not through the start line's own furniture, and not on a bridge.
                let near_line = s < 26.0 || s > lap - 26.0;
                if !near_line && !track.bridge_at(s) {
                    let spot = track.spot_at(s);
                    let side = if index % 2 == 0 { -1.0 } else { 1.0 };
                    let p = spot.pos + spot.right * (side * (ROAD_HALF + 0.85));
                    let base = track.ground_from(p, Some(spot.s)).height;
                    root.spawn((
                        Transform::from_translation(Vec3::new(p.x, base, p.z)),
                        Visibility::Inherited,
                    ))
                    .with_children(|pylon| {
                        pylon.spawn((
                            Mesh3d(kit.mesh(Shape::Cylinder)),
                            MeshMaterial3d(pole.clone()),
                            Transform::from_xyz(0.0, 0.32, 0.0)
                                .with_scale(Vec3::new(0.05, 0.64, 0.05)),
                        ));
                        pylon.spawn((
                            Pylon(index),
                            Mesh3d(kit.mesh(Shape::Sphere)),
                            MeshMaterial3d(ramp[index % PYLON_STEPS].clone()),
                            Transform::from_xyz(0.0, 0.70, 0.0).with_scale(Vec3::splat(0.20)),
                            NotShadowCaster,
                        ));
                    });
                }
                index += 1;
                s += PYLON_EVERY;
            }

            // The light show: beams from beside the road, up into the sky.
            let mut rng = Rng::of(track.circuit().id, 0x1A5E);
            for i in 0..BEAMS {
                let s = lap * (i as f32 + 0.5) / BEAMS as f32;
                if track.bridge_at(s) {
                    continue;
                }
                let spot = track.spot_at(s);
                let side = if i % 2 == 0 { 1.0 } else { -1.0 };
                let p = spot.pos + spot.right * (side * (ROAD_HALF + 5.5));
                let base = track.ground_from(p, Some(spot.s)).height;
                root.spawn((
                    Laser {
                        phase: rng.range(0.0, std::f32::consts::TAU),
                        rate: rng.range(0.35, 0.8) * if i % 2 == 0 { 1.0 } else { -1.0 },
                        lean: rng.range(0.25, 0.55),
                    },
                    Transform::from_translation(Vec3::new(p.x, base, p.z)),
                    Visibility::Inherited,
                ))
                .with_children(|laser| {
                    laser.spawn((
                        Mesh3d(kit.mesh(Shape::Cylinder)),
                        MeshMaterial3d(beams[i % beams.len()].clone()),
                        Transform::from_xyz(0.0, BEAM_LENGTH * 0.5, 0.0).with_scale(Vec3::new(
                            0.10,
                            BEAM_LENGTH,
                            0.10,
                        )),
                        NotShadowCaster,
                        NoFrustumCulling,
                    ));
                });
            }
        });
}

/// Scroll the rainbow, pulse the rails, chase the pylons, sweep the beams.
#[allow(clippy::too_many_arguments)]
fn animate_neon(
    fun: Res<Fun>,
    beat: Res<Beat>,
    boost: Res<Boost>,
    time: Res<Time<Real>>,
    mut slid: Local<f32>,
    mut pushed_on: Local<f32>,
    neon: Res<Neon>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut pylons: Query<(&Pylon, &mut MeshMaterial3d<StandardMaterial>)>,
    mut lasers: Query<(&Laser, &mut Transform)>,
) {
    if !fun.neon {
        return;
    }
    let t = time.elapsed_secs();
    let dt = time.delta_secs().min(0.1);
    let calm = fun.calm;
    let kick = if calm { 0.0 } else { beat.kick() };
    let pushed = smoothed(
        &mut pushed_on,
        if calm { 0.0 } else { boost.strength() },
        dt,
    );
    if let Some(mut rails) = neon.rails.as_ref().and_then(|h| materials.get_mut(h)) {
        // The rainbow slides down the road, faster when boosting. How far it has
        // gone is added up a frame at a time, because how fast it is going
        // changes with the boost and the time is a large number, and a product
        // of the two would put it somewhere else altogether every frame that the
        // boost changed in.
        if !calm {
            *slid = (*slid + dt * (0.18 + 0.5 * pushed)).rem_euclid(1.0);
        }
        rails.uv_transform = Affine2::from_translation(Vec2::new(-*slid, 0.0));
        let power = 2.6 + 3.2 * kick + 2.0 * pushed;
        rails.base_color = hot(Color::WHITE, power);
    }
    // The pylons' colours step along one place on every beat.
    let step = if calm { 0 } else { beat.count() as usize };
    for (pylon, mut material) in &mut pylons {
        let wanted = &neon.ramp[(pylon.0 + PYLON_STEPS - step % PYLON_STEPS) % PYLON_STEPS];
        if material.0 != *wanted {
            material.0 = wanted.clone();
        }
    }
    for (laser, mut transform) in &mut lasers {
        let sweep = if calm { 0.0 } else { t * laser.rate };
        let fan = laser.lean
            + if calm {
                0.0
            } else {
                0.22 * (t * 0.9 + laser.phase).sin()
            };
        transform.rotation =
            Quat::from_rotation_y(laser.phase + sweep) * Quat::from_rotation_z(fan);
    }
}

/// A pool of light under the car, in the colour of the moment.
#[allow(clippy::too_many_arguments)]
fn glow_under_the_car(
    mut commands: Commands,
    fun: Res<Fun>,
    beat: Res<Beat>,
    boost: Res<Boost>,
    time: Res<Time<Real>>,
    kit: Option<Res<Kit>>,
    mut neon: ResMut<Neon>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    cars: Query<Entity, With<Player>>,
    glows: Query<Entity, With<Underglow>>,
) {
    let Ok(car) = cars.single() else {
        return;
    };
    if !fun.neon {
        for glow in &glows {
            commands.entity(glow).despawn();
        }
        return;
    }
    let colour = |at: f32, kick: f32, push: f32| {
        let c = rainbow(at, 0.58).to_linear();
        let power = 2.4 + 2.6 * kick + 2.4 * push;
        Color::linear_rgba(c.red * power, c.green * power, c.blue * power, 0.9)
    };
    let t = time.elapsed_secs();
    let kick = if fun.calm { 0.0 } else { beat.kick() };
    let halo = neon
        .halo
        .get_or_insert_with(|| images.add(soft_glow()))
        .clone();
    let material = neon
        .glow
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color: colour(0.0, 0.0, 0.0),
                base_color_texture: Some(halo),
                unlit: true,
                alpha_mode: AlphaMode::Add,
                double_sided: true,
                cull_mode: None,
                depth_bias: 20.0,
                fog_enabled: false,
                ..default()
            })
        })
        .clone();
    if let Some(mut m) = materials.get_mut(&material) {
        m.base_color = if fun.calm {
            colour(0.55, 0.0, 0.0)
        } else {
            colour(t * 0.11, kick, boost.strength())
        };
    }
    if glows.is_empty()
        && let Some(kit) = kit
    {
        commands.entity(car).with_children(|car| {
            car.spawn((
                Underglow,
                Mesh3d(kit.mesh(Shape::Disc)),
                MeshMaterial3d(material),
                // In the model's units, which the car scales down: a pool a
                // little wider than the mount, just off the road.
                Transform::from_xyz(0.0, 0.06, 0.1).with_scale(Vec3::new(4.8, 1.0, 6.4)),
                NotShadowCaster,
            ));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::all_circuits;

    /// A camera with the whole stack on it, and everything the stack reads, in
    /// an app that ticks a sixtieth of a second at a time.
    fn stage() -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_micros(16_667),
            ))
            .insert_resource(Fun::of(&crate::settings::Settings {
                bonkers: true,
                ..crate::settings::Settings::default()
            }))
            .init_resource::<crate::pause::Halt>()
            .init_resource::<Beat>()
            .init_resource::<Pose>()
            .init_resource::<Boost>()
            .init_resource::<super::super::events::Chaos>()
            .add_systems(Update, animate_stack);
        let camera = app
            .world_mut()
            .spawn((
                Camera3d::default(),
                Bloom::default(),
                ColorGrading::default(),
                ChromaticAberration::default(),
                Vignette::default(),
                LensDistortion::default(),
            ))
            .id();
        (app, camera)
    }

    /// The world's hue turns steadily, and never lurches: not when a disco starts
    /// or stops, however long the game has been going. It used to be worked out
    /// as the time so far times how fast it was turning, and the speed changing
    /// as the disco faded in put the hue somewhere else every frame for a
    /// second, at a rate that grew the longer anyone had been playing.
    #[test]
    fn the_hue_turns_steadily_through_a_disco_however_long_it_has_been_going() {
        let (mut app, camera) = stage();
        for _ in 0..3_000 {
            app.update();
        }
        let hue = |app: &App| app.world().get::<ColorGrading>(camera).unwrap().global.hue;
        let mut last = hue(&app);
        let mut worst = 0.0f32;
        for frame in 0..900 {
            if frame == 60 {
                app.world_mut()
                    .resource_mut::<super::super::events::Chaos>()
                    .start(super::super::events::Effect::Disco);
            }
            app.update();
            let now = hue(&app);
            worst = worst.max((now - last).abs());
            last = now;
        }
        // At its quickest it turns two radians a second: a thirtieth of a
        // radian a frame.
        assert!(worst < 0.1, "the hue moved {worst} radians in one frame");
    }

    /// Reduced motion holds the hue where it is, and a boost does not brighten
    /// the world in a step.
    #[test]
    fn calm_holds_the_grade_still_and_a_boost_swells_and_does_not_step() {
        let (mut app, camera) = stage();
        app.world_mut().resource_mut::<Fun>().calm = true;
        for _ in 0..30 {
            app.update();
        }
        let grade = |app: &App| {
            let g = &app.world().get::<ColorGrading>(camera).unwrap().global;
            (g.hue, g.exposure)
        };
        let before = grade(&app);
        app.world_mut().resource_mut::<Boost>().fire(1.0, 3.0);
        for _ in 0..60 {
            app.update();
            assert_eq!(grade(&app), before, "calm moved with a boost");
        }
        // Not calm: the exposure rises with it, over a moment. (Half way between
        // two beats, where the kick of the music is not what is being looked at.)
        app.world_mut()
            .resource_mut::<Beat>()
            .advance(60.0 / super::super::beat::BPM * 0.5);
        app.world_mut().resource_mut::<Fun>().calm = false;
        app.update();
        let mut last = grade(&app).1;
        let mut biggest = 0.0f32;
        for _ in 0..60 {
            app.update();
            let now = grade(&app).1;
            biggest = biggest.max((now - last).abs());
            last = now;
        }
        assert!(last > before.1 + 0.1, "a boost did not brighten anything");
        assert!(
            biggest < 0.12,
            "the exposure stepped by {biggest} in one frame"
        );
    }

    #[test]
    fn the_rainbow_is_a_strip_that_repeats() {
        let image = gradient();
        assert_eq!(image.width(), 256);
        let data = image.data.as_ref().unwrap();
        // Bright at every pixel, and not the same colour twice in a row of eight.
        let pixel = |x: usize| [data[x * 4], data[x * 4 + 1], data[x * 4 + 2]];
        assert_ne!(pixel(0), pixel(64));
        assert_ne!(pixel(64), pixel(128));
        assert!(data.iter().step_by(4).any(|r| *r > 200));
    }

    #[test]
    fn a_rail_follows_the_lap_and_its_texture_runs_along_it_once() {
        let circuit = all_circuits().iter().find(|c| c.id == "monza").unwrap();
        let track = Track::new(circuit);
        let mesh = ribbon(&track, RAIL_AT, RAIL_WIDTH);
        let Some(bevy::mesh::VertexAttributeValues::Float32x2(uvs)) =
            mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("a rail has texture coordinates");
        };
        let lap = track.length();
        let last = uvs.last().unwrap()[0];
        assert!(
            (last - lap / TILE).abs() < 1e-3,
            "{last} against {}",
            lap / TILE
        );
        assert!(
            uvs.windows(2).all(|w| w[1][0] >= w[0][0] - 1e-6),
            "the texture never runs back"
        );
        // And the mesh stands next to the road, above it.
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("a rail has positions");
        };
        for p in positions.iter().step_by(37) {
            let here = Vec3::from(*p);
            let ground = track.ground_from(here, None);
            assert!(
                ground.lateral.abs() > ROAD_HALF - 0.4,
                "on the road at {here}"
            );
            assert!(here.y > ground.height, "under the ground");
        }
    }

    #[test]
    fn the_dashes_are_short_and_fill_the_lap() {
        let circuit = all_circuits().iter().find(|c| c.id == "monza").unwrap();
        let track = Track::new(circuit);
        let mesh = dashes(&track, 0.11, 1.3, 2.3);
        let count = mesh.count_vertices();
        let expected = (track.length() / 3.6) as usize * 6;
        assert!(
            count.abs_diff(expected) < 12,
            "{count} vertices for about {expected}"
        );
    }
}
