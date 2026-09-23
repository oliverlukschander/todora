//! The start/finish line: a chequered band laid over the painted one, and a
//! post with a chequered panel on either side of the road.
//!
//! The band is its own quad with a texture made once at startup and mipmapped
//! in linear light like the surface textures, so it greys evenly with distance
//! instead of shimmering. The posts are merged into the braking boards' mesh
//! and cost no draw call of their own. Neither is anything the car touches.

use super::{
    Track,
    boards::{self, Builder},
    profile::{HALF_WIDTH, STRIPE, TARMAC_HALF},
    textures,
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
/// Chequers on each panel face.
const PANEL_ACROSS: usize = 4;
const PANEL_DOWN: usize = 2;

const WHITE: (f32, f32, f32) = (0.93, 0.93, 0.91);
const BLACK: (f32, f32, f32) = (0.07, 0.07, 0.08);
const POST: (f32, f32, f32) = (0.42, 0.43, 0.43);
const POST_SHADE: (f32, f32, f32) = (0.33, 0.34, 0.34);
const EDGE: (f32, f32, f32) = (0.25, 0.28, 0.27);

#[derive(Component)]
pub(super) struct StartLine;

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
    let material = material
        .get_or_insert_with(|| {
            materials.add(StandardMaterial {
                base_color_texture: Some(images.add(chequer())),
                unlit: true,
                depth_bias: DEPTH_BIAS,
                ..default()
            })
        })
        .clone();
    commands.spawn((
        StartLine,
        Mesh3d(meshes.add(band(&track))),
        MeshMaterial3d(material),
        NotShadowCaster,
    ));
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

/// Both posts and their panels, into the boards' mesh.
pub(super) fn posts(track: &Track, out: &mut Builder) {
    for side in [-1.0, 1.0] {
        let (base, across, normal) = frame(track, side);
        let top = base.y + POST_HEIGHT;
        out.column(
            base,
            base.y - SINK,
            top - PANEL_HALF_HEIGHT,
            across * POST_HALF,
            normal * POST_HALF,
            [POST_SHADE, POST_SHADE, POST],
            false,
        );
        let centre = top - PANEL_HALF_HEIGHT;
        out.column(
            base,
            centre - PANEL_HALF_HEIGHT,
            top,
            across * PANEL_HALF_WIDTH,
            normal * PANEL_HALF_DEPTH,
            [EDGE, EDGE, EDGE],
            true,
        );
        // Chequers on both faces, a hair proud of the panel.
        for facing in [1.0, -1.0] {
            let face = normal * facing;
            for i in 0..PANEL_ACROSS {
                for k in 0..PANEL_DOWN {
                    let x0 =
                        -PANEL_HALF_WIDTH + 2.0 * PANEL_HALF_WIDTH * i as f32 / PANEL_ACROSS as f32;
                    let x1 = x0 + 2.0 * PANEL_HALF_WIDTH / PANEL_ACROSS as f32;
                    let y1 = top - 2.0 * PANEL_HALF_HEIGHT * k as f32 / PANEL_DOWN as f32;
                    let y0 = y1 - 2.0 * PANEL_HALF_HEIGHT / PANEL_DOWN as f32;
                    let at = |x: f32, y: f32| {
                        let mut p = base + across * x + face * (PANEL_HALF_DEPTH + boards::RAISED);
                        p.y = y;
                        p
                    };
                    let colour = if (i + k).is_multiple_of(2) {
                        WHITE
                    } else {
                        BLACK
                    };
                    out.quad(
                        [at(x0, y0), at(x1, y0), at(x1, y1), at(x0, y1)],
                        face,
                        colour,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{circuits, markers};

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
