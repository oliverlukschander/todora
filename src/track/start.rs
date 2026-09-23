//! The start/finish line: a chequered band laid over the painted one, and a
//! post with a chequered panel on either side of the road.
//!
//! The band is its own quad with a texture made once at startup and mipmapped
//! in linear light like the surface textures, so it greys evenly with distance
//! instead of shimmering. The posts are part of the trackside meshes and cost
//! no draw call of their own. Neither is anything the car touches.

use super::{
    Track,
    profile::{HALF_WIDTH, STRIPE, TARMAC_HALF},
    textures,
    trackside::{Builder, atlas::Cell},
};
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    light::NotShadowCaster,
    prelude::*,
    render::render_resource::{Extent3d, PrimitiveTopology, TextureDimension, TextureFormat},
};

/// Squares across the tarmac and along the band. The band is the painted
/// stripe's [`STRIPE`] stations long, 0.8 m, so the squares come out 0.37 m
/// across by 0.4 m along: square enough to read as a chequer.
const ACROSS: usize = 8;
const ALONG: usize = 2;
/// Texels per square. Mipmaps take it down from there.
const TEXELS: usize = 32;
/// Over the painted stripe, and drawn in front of it.
const LIFT: f32 = 0.004;
const DEPTH_BIAS: f32 = 8.0;

/// Post centres, metres out from the edge of the kerb, and their panels.
const OUT: f32 = 1.1;
const POST_HALF: f32 = 0.03;
const POST_HEIGHT: f32 = 1.35;
const SINK: f32 = 0.08;
const PANEL_HALF_WIDTH: f32 = 0.28;
const PANEL_HALF_HEIGHT: f32 = 0.16;
const PANEL_HALF_DEPTH: f32 = 0.012;

const POST: (f32, f32, f32) = (0.42, 0.43, 0.43);
const POST_SHADE: (f32, f32, f32) = (0.33, 0.34, 0.34);
const EDGE: (f32, f32, f32) = (0.25, 0.28, 0.27);

#[derive(Component)]
pub(super) struct StartLine;

/// The flag waved over the left-hand post as a lap ends, and where it swings.
#[derive(Component)]
pub(super) struct Flag {
    /// Facing down the road, before any swing.
    rest: Quat,
}

/// The band's material, so the finish can light it.
#[derive(Resource)]
pub(super) struct Band(Handle<StandardMaterial>);

/// How long the band stays lit and the flag keeps waving after a lap.
const FLASH_FOR: f32 = 0.6;
const WAVE_FOR: f32 = 3.0;
/// The flag: half its size, and how far its staff rises above the post.
const FLAG_HALF: Vec2 = Vec2::new(0.26, 0.18);
const STAFF: f32 = 0.55;

/// Seconds of flash and of waving left.
#[derive(Resource, Default)]
pub(super) struct Finish {
    flash: f32,
    wave: f32,
    clock: f32,
}

pub(super) fn rebuild(
    mut commands: Commands,
    track: Res<Track>,
    old: Query<Entity, With<StartLine>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut material: Local<Option<Handle<StandardMaterial>>>,
) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let chequer = images.add(chequer());
    let material = material
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color_texture: Some(chequer.clone()),
                unlit: true,
                depth_bias: DEPTH_BIAS,
                ..default()
            })
        })
        .clone();
    commands.insert_resource(Band(material.clone()));
    commands.spawn((
        StartLine,
        Mesh3d(meshes.add(band(&track))),
        MeshMaterial3d(material),
        NotShadowCaster,
    ));
    // A staff above the left-hand post, and the flag on it, seen from both
    // sides; hidden until a lap ends.
    let (base, _, _) = frame(&track, -1.0);
    let pivot = base + Vec3::Y * (POST_HEIGHT + STAFF);
    let facing =
        Transform::from_translation(pivot).looking_to(track.ribbon.start().tangent, Vec3::Y);
    commands
        .spawn((
            StartLine,
            Flag {
                rest: facing.rotation,
            },
            facing,
            Visibility::Hidden,
        ))
        .with_children(|pivot| {
            pivot.spawn((
                Mesh3d(meshes.add(Rectangle::new(2.0 * FLAG_HALF.x, 2.0 * FLAG_HALF.y))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color_texture: Some(chequer),
                    unlit: true,
                    cull_mode: None,
                    ..default()
                })),
                Transform::from_xyz(FLAG_HALF.x, -FLAG_HALF.y, 0.0),
                NotShadowCaster,
            ));
            pivot.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.02, STAFF + 0.1, 0.02))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(POST.0, POST.1, POST.2),
                    unlit: true,
                    ..default()
                })),
                Transform::from_xyz(0.0, -STAFF / 2.0, 0.0),
                NotShadowCaster,
            ));
        });
}

/// A lap ended: light the band for a moment and wave the flag. Neither
/// happens with reduced motion.
pub(super) fn finish(
    time: Res<Time>,
    mut laps: MessageReader<crate::lap::LapFinished>,
    settings: Option<Res<crate::settings::Settings>>,
    band: Option<Res<Band>>,
    mut finish: ResMut<Finish>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut flags: Query<(&Flag, &mut Transform, &mut Visibility)>,
) {
    let still = settings.is_some_and(|s| s.reduced_motion);
    if laps.read().last().is_some() && !still {
        finish.flash = FLASH_FOR;
        finish.wave = WAVE_FOR;
    }
    if finish.flash == 0.0 && finish.wave == 0.0 {
        return;
    }
    let dt = time.delta_secs();
    let was_lit = finish.flash > 0.0;
    finish.flash = (finish.flash - dt).max(0.0);
    finish.wave = (finish.wave - dt).max(0.0);
    finish.clock += dt;
    if let Some(mut material) = band.and_then(|b| materials.get_mut(&b.0)) {
        material.base_color = if finish.flash > 0.0 {
            // Warm gold fading back to the plain chequer.
            let lit = finish.flash / FLASH_FOR;
            Color::srgb(1.0, 1.0 - 0.25 * lit, 1.0 - 0.7 * lit)
        } else if was_lit {
            Color::WHITE
        } else {
            material.base_color
        };
    }
    for (flag, mut at, mut visibility) in &mut flags {
        visibility.set_if_neq(if finish.wave > 0.0 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
        // Swung about the staff, the way a marshal waves it.
        let swing = (finish.clock * 9.0).sin() * 0.7;
        at.rotation = flag.rest * Quat::from_rotation_y(swing);
    }
}

/// The chequer, [`ACROSS`] by [`ALONG`] squares, with its mip chain.
fn chequer() -> Image {
    let (width, height) = (ACROSS * TEXELS, ALONG * TEXELS);
    let mut data = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let white = (x / TEXELS + y / TEXELS).is_multiple_of(2);
            let [r, g, b] = if white { [236, 236, 232] } else { [22, 22, 24] };
            data.extend_from_slice(&[r, g, b, 255]);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: width as u32,
            height: height as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    textures::mipmaps(&mut image);
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::ClampToEdge,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 16,
        ..default()
    });
    image
}

/// One quad per station gap over the painted stripe, tarmac edge to tarmac
/// edge. The tarmac is flat across and the loft is straight between stations,
/// so these lie exactly on it.
fn band(track: &Track) -> Mesh {
    let stations = track.ribbon.stations();
    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    for (j, station) in stations.iter().take(STRIPE + 1).enumerate() {
        for (u, lateral) in [(0.0, -TARMAC_HALF), (1.0, TARMAC_HALF)] {
            let mut point = station.pos + station.right * lateral;
            point.y += track.profile.height(j, 0.0, lateral) + LIFT;
            positions.push(point.to_array());
            uvs.push([u, j as f32 / STRIPE as f32]);
        }
    }
    let mut indices = Vec::new();
    for j in 0..STRIPE as u16 {
        let (a, b, c, d) = (2 * j, 2 * j + 1, 2 * j + 3, 2 * j + 2);
        indices.extend([a, b, c, a, c, d]);
    }
    let normals = vec![[0.0, 1.0, 0.0]; positions.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(bevy::mesh::Indices::U16(indices))
}

/// Where the two posts stand.
pub(super) fn post_bases(track: &Track) -> [Vec3; 2] {
    [-1.0, 1.0].map(|side| frame(track, side).0)
}

/// The corners of each post and its panel in plan.
#[cfg(test)]
fn footprints(track: &Track) -> Vec<Vec3> {
    let mut points = Vec::new();
    for side in [-1.0, 1.0] {
        let (base, across, normal) = frame(track, side);
        for x in [-PANEL_HALF_WIDTH, PANEL_HALF_WIDTH] {
            for z in [-POST_HALF, POST_HALF] {
                points.push(base + across * x + normal * z);
            }
        }
    }
    points
}

/// A post's foot at ground height, across its panel as the driver sees it,
/// and the panel's normal, facing down the road at the traffic coming.
fn frame(track: &Track, side: f32) -> (Vec3, Vec3, Vec3) {
    let start = track.ribbon.start();
    let mut base = start.pos + start.right * side * (HALF_WIDTH + OUT);
    base.y = track.ground_from(base, Some(start.s)).height;
    let normal = -start.tangent;
    (base, (-normal).cross(Vec3::Y).normalize(), normal)
}

/// Both posts and their panels, into the trackside meshes.
pub(super) fn posts(track: &Track, out: &mut Builder) {
    for side in [-1.0, 1.0] {
        let (base, across, normal) = frame(track, side);
        let top = base.y + POST_HEIGHT;
        let bottom = top - 2.0 * PANEL_HALF_HEIGHT;
        // The post stops under the panel rather than showing through it.
        out.column(
            base,
            base.y - SINK,
            bottom,
            across * POST_HALF,
            normal * POST_HALF,
            [POST_SHADE, POST_SHADE, POST],
            true,
        );
        out.open_column(
            base,
            bottom,
            top,
            across * PANEL_HALF_WIDTH,
            normal * PANEL_HALF_DEPTH,
            [None, None],
            EDGE,
            true,
        );
        // Both faces are chequers from the atlas, standing in for the
        // panel's own.
        for facing in [1.0, -1.0] {
            let face = normal * facing;
            // Left to right as seen from this side.
            let right = (-face).cross(Vec3::Y).normalize();
            let at = |x: f32, y: f32| {
                let mut p = base + right * x + face * PANEL_HALF_DEPTH;
                p.y = y;
                p
            };
            out.sign(
                [
                    at(-PANEL_HALF_WIDTH, bottom),
                    at(PANEL_HALF_WIDTH, bottom),
                    at(PANEL_HALF_WIDTH, top),
                    at(-PANEL_HALF_WIDTH, top),
                ],
                face,
                Cell::Chequer,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{boards, circuits, markers};

    fn every_track() -> impl Iterator<Item = (&'static str, Track)> {
        circuits::all()
            .iter()
            .map(|circuit| (circuit.name, Track::new(circuit)))
    }

    /// Exactly over the painted stripe, kerb to kerb, just proud of the road,
    /// facing up, with its leading edge on the line the lap is timed across.
    #[test]
    fn the_chequer_lies_on_the_painted_stripe_at_the_timing_line() {
        for (name, track) in every_track() {
            let mesh = band(&track);
            let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap();
            let positions: Vec<Vec3> = positions
                .as_float3()
                .unwrap()
                .iter()
                .map(|&p| Vec3::from(p))
                .collect();
            let stations = track.ribbon.stations();
            for (i, point) in positions.iter().enumerate() {
                let station = &stations[i / 2];
                let lateral = (*point - station.pos).dot(station.right);
                assert!((lateral.abs() - TARMAC_HALF).abs() < 1e-3, "{name}");
                let ground = track.ground_from(*point, Some(station.s)).height;
                assert!(
                    (point.y - ground - LIFT).abs() < 2e-3,
                    "{name}: chequer {} above the road",
                    point.y - ground
                );
                let along = track.start_along(*point);
                if i < 2 {
                    assert!(along.abs() < 1e-3, "{name}: {along} m off the line");
                } else {
                    assert!(along > 0.0, "{name}: chequer before the line");
                }
            }
            let Some(bevy::mesh::Indices::U16(indices)) = mesh.indices() else {
                panic!("indexed")
            };
            for triangle in indices.chunks_exact(3) {
                let [a, b, c] = [0, 1, 2].map(|k| positions[triangle[k] as usize]);
                assert!((b - a).cross(c - a).y > 0.0, "{name}: chequer faces down");
            }
        }
    }

    /// A full chain down to one texel, filtered for grazing angles, and the
    /// squares alternate in both directions.
    #[test]
    fn the_chequer_texture_is_mipmapped_and_alternates() {
        let image = chequer();
        let width = ACROSS * TEXELS;
        assert_eq!(image.texture_descriptor.mip_level_count, width.ilog2() + 1);
        let ImageSampler::Descriptor(sampler) = &image.sampler else {
            panic!("sampler set")
        };
        assert_eq!(sampler.anisotropy_clamp, 16);
        assert_eq!(sampler.mipmap_filter, ImageFilterMode::Linear);
        let data = image.data.as_ref().unwrap();
        let texel = |x: usize, y: usize| data[(y * width + x) * 4];
        let mid = TEXELS / 2;
        assert!(texel(mid, mid) > 200);
        assert!(texel(TEXELS + mid, mid) < 50);
        assert!(texel(mid, TEXELS + mid) < 50);
        assert!(texel(TEXELS + mid, TEXELS + mid) > 200);
    }

    /// One post either side of the road at the line, clear of every kerb and
    /// of the chevron plaques and braking boards.
    #[test]
    fn the_posts_stand_clear_either_side_of_the_line() {
        for (name, track) in every_track() {
            let start = track.ribbon.start();
            let plaques = markers::plaque_centres(&track);
            let boards: Vec<Vec3> = boards::placed(&track)
                .iter()
                .map(|board| {
                    let points = boards::footprint(&track, board);
                    points.iter().sum::<Vec3>() / points.len() as f32
                })
                .collect();
            for side in [-1.0, 1.0] {
                let (base, _, _) = frame(&track, side);
                assert!(track.start_along(base).abs() < 1e-3, "{name}");
                assert!(
                    (base - start.pos).dot(start.right) * side > HALF_WIDTH,
                    "{name}"
                );
                for plaque in &plaques {
                    assert!(
                        (*plaque - base).xz().length() > PANEL_HALF_WIDTH + markers::HALF * 1.5,
                        "{name}: start post on a plaque"
                    );
                }
                for board in &boards {
                    assert!(
                        (*board - base).xz().length() > boards::POST_ROOM - 0.01,
                        "{name}: start post on a braking board"
                    );
                }
            }
            for point in footprints(&track) {
                let road = track.fix(point, None);
                assert!(
                    road.lateral.abs() > HALF_WIDTH + boards::CLEAR,
                    "{name}: start post {:.2} m from a kerb",
                    road.lateral.abs() - HALF_WIDTH
                );
            }
        }
    }
}

#[cfg(test)]
mod finish_tests {
    use super::*;
    use crate::lap::LapFinished;

    fn app(reduced_motion: bool) -> (App, Entity) {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<LapFinished>()
            .init_resource::<Finish>()
            .init_resource::<Assets<StandardMaterial>>()
            .insert_resource(crate::settings::Settings {
                reduced_motion,
                ..Default::default()
            })
            .add_systems(Update, finish);
        let flag = app
            .world_mut()
            .spawn((
                Flag {
                    rest: Quat::IDENTITY,
                },
                Transform::default(),
                Visibility::Hidden,
            ))
            .id();
        (app, flag)
    }

    fn lap(app: &mut App) {
        app.world_mut().write_message(LapFinished {
            time: 60.0,
            best: false,
            valid: true,
        });
        app.update();
    }

    #[test]
    fn a_finished_lap_waves_the_flag_and_it_settles_again() {
        let (mut app, flag) = app(false);
        lap(&mut app);
        assert_eq!(
            app.world().get::<Visibility>(flag),
            Some(&Visibility::Visible)
        );
        app.world_mut().resource_mut::<Finish>().wave = 0.001;
        std::thread::sleep(std::time::Duration::from_millis(5));
        app.update();
        app.update();
        assert_eq!(
            app.world().get::<Visibility>(flag),
            Some(&Visibility::Hidden)
        );
    }

    #[test]
    fn reduced_motion_keeps_the_flag_down() {
        let (mut app, flag) = app(true);
        lap(&mut app);
        assert_eq!(
            app.world().get::<Visibility>(flag),
            Some(&Visibility::Hidden)
        );
    }
}
