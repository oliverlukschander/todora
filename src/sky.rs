//! Clouds. One opaque dome around the camera, painted once on a worker thread
//! and never again: a frame costs one draw call, one texture read per pixel of
//! sky and one transform. There is no blending, no per-pixel noise and no
//! per-frame upload. Until the painting arrives the plain clear colour shows,
//! which is the sky the game had before.
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    tasks::{AsyncComputeTaskPool, Task, futures::check_ready},
    transform::TransformSystems,
};
use std::f32::consts::{FRAC_PI_2, TAU};

use crate::world::SKY;

/// Inside the camera's 1 km culling distance, beyond every circuit's grass.
const RADIUS: f32 = 900.0;
/// The painting covers the sky from the horizon to this elevation; the dome
/// above it takes the top row, which is clear. The chase camera sees little
/// more than the lowest fifteen degrees.
const TOP: f32 = 50.0_f32.to_radians();
/// Below the horizon, so a hill crest never shows the edge of the dome.
const SKIRT: f32 = -20.0_f32.to_radians();
const WIDTH: usize = 4096;
const HEIGHT: usize = 512;
/// A full turn of the sky every forty minutes: drifting, never moving.
const DRIFT: f32 = TAU / (40.0 * 60.0);

const ZENITH: Color = Color::srgb(0.26, 0.48, 0.80);
const CLOUD_LIT: Color = Color::srgb(0.99, 0.99, 0.98);
const CLOUD_SHADE: Color = Color::srgb(0.70, 0.76, 0.86);

pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, paint)
            .add_systems(Update, hang)
            .add_systems(PostUpdate, drift.before(TransformSystems::Propagate));
    }
}

#[derive(Component)]
struct Painting(Task<Vec<u8>>);

#[derive(Component)]
struct Dome;

fn paint(mut commands: Commands) {
    let task = AsyncComputeTaskPool::get().spawn(async { pixels() });
    commands.spawn(Painting(task));
}

/// Put the dome up once its painting is dry.
fn hang(
    mut commands: Commands,
    mut paintings: Query<(Entity, &mut Painting)>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, mut painting) in &mut paintings {
        let Some(data) = check_ready(&mut painting.0) else {
            continue;
        };
        commands.entity(entity).despawn();
        let mut image = Image::new(
            Extent3d {
                width: WIDTH as u32,
                height: HEIGHT as u32,
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
        commands.spawn((
            Dome,
            Mesh3d(meshes.add(dome())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color_texture: Some(images.add(image)),
                unlit: true,
                fog_enabled: false,
                cull_mode: None,
                ..default()
            })),
            NotShadowCaster,
            Transform::default(),
        ));
    }
}

/// Centred on the camera, so the sky is infinitely far away, and turning.
fn drift(
    time: Res<Time>,
    cameras: Query<&Transform, (With<Camera3d>, Without<Dome>)>,
    mut domes: Query<&mut Transform, With<Dome>>,
) {
    let Ok(camera) = cameras.single() else {
        return;
    };
    for mut dome in &mut domes {
        dome.translation = camera.translation;
        dome.rotation = Quat::from_rotation_y(time.elapsed_secs() * DRIFT);
    }
}

/// A hemisphere with a skirt, its rings dense near the horizon where the
/// camera looks. `u` is azimuth and `v` elevation over the painting, both
/// linear in angle so a triangle interpolates them as the sphere would.
fn dome() -> Mesh {
    const AROUND: usize = 128;
    let mut rings = vec![SKIRT, 0.0];
    rings.extend((1..=20).map(|i| TOP * (i as f32 / 20.0).powf(1.4)));
    rings.extend((1..=6).map(|i| TOP + (FRAC_PI_2 - TOP) * i as f32 / 6.0));

    let mut positions = Vec::new();
    let mut uvs = Vec::new();
    for &elevation in &rings {
        let v = 1.0 - (elevation / TOP).clamp(0.0, 1.0);
        for i in 0..=AROUND {
            let u = i as f32 / AROUND as f32;
            let azimuth = u * TAU;
            let (sin_e, cos_e) = elevation.sin_cos();
            positions.push([
                RADIUS * cos_e * azimuth.cos(),
                RADIUS * sin_e,
                RADIUS * cos_e * azimuth.sin(),
            ]);
            uvs.push([u, v]);
        }
    }
    let row = AROUND as u32 + 1;
    let mut indices = Vec::new();
    for r in 0..rings.len() as u32 - 1 {
        for i in 0..AROUND as u32 {
            let a = r * row + i;
            let b = a + row;
            indices.extend([a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

/// The painting: sRGB bytes, row 0 at `TOP` and the last row on the horizon.
///
/// Clouds are a noise field on a flat layer overhead, seen in perspective, so
/// they shrink and flatten towards the horizon as real ones do. The layer is
/// read through a softened projection — distance grows with 1/(elevation + k)
/// rather than 1/tan — or at the chase camera's low angles every cloud would
/// be a thread. Near the horizon they melt into the haze, which is the fog
/// colour the circuit fades into, so the two still meet without a seam.
fn pixels() -> Vec<u8> {
    let density = density();
    let sky = SKY.to_linear().to_vec3();
    let zenith = ZENITH.to_linear().to_vec3();
    let lit = CLOUD_LIT.to_linear().to_vec3();
    let shade = CLOUD_SHADE.to_linear().to_vec3();

    let mut data = Vec::with_capacity(WIDTH * HEIGHT * 4);
    for y in 0..HEIGHT {
        let elevation = elevation(y);
        let height = elevation / TOP;
        let blue = sky.lerp(zenith, height.powf(0.7));
        // Clouds fade into the haze over the lowest degrees, and thin out
        // towards the top of the painting so its clamped edge is clear sky.
        let haze = smoothstep(0.0, 3.0_f32.to_radians(), elevation);
        let thin = 1.0 - smoothstep(0.6, 1.0, height);
        // A cloud's upper edge faces the sun; its underside is in its own
        // shadow. Up the painting is the previous row.
        let above = y.saturating_sub(4);
        for x in 0..WIDTH {
            let d = density[y * WIDTH + x];
            let cover = smoothstep(0.5, 0.64, d) * thin;
            let light = (0.6 + (d - density[above * WIDTH + x]) * 14.0).clamp(0.0, 1.0);
            let cloud = shade.lerp(lit, light).lerp(blue, 1.0 - haze);
            let colour = blue.lerp(cloud, cover);
            let srgb = Color::linear_rgb(colour.x, colour.y, colour.z).to_srgba();
            data.extend([
                (srgb.red * 255.0).round() as u8,
                (srgb.green * 255.0).round() as u8,
                (srgb.blue * 255.0).round() as u8,
                255,
            ]);
        }
    }
    data
}

fn elevation(y: usize) -> f32 {
    TOP * (1.0 - y as f32 / (HEIGHT - 1) as f32)
}

/// Cloud density in 0..1 at every texel.
fn density() -> Vec<f32> {
    let mut out = Vec::with_capacity(WIDTH * HEIGHT);
    for y in 0..HEIGHT {
        let distance = 1.0 / (elevation(y) + 0.22);
        for x in 0..WIDTH {
            let azimuth = x as f32 / WIDTH as f32 * TAU;
            // Wider than deep, as a layer seen edge-on looks.
            let p = Vec2::new(azimuth.cos(), azimuth.sin()) * distance * 1.25;
            out.push(fbm(p));
        }
    }
    out
}

fn fbm(mut p: Vec2) -> f32 {
    // Each octave turned against the last, so the grid never lines up.
    const TURN: Mat2 = Mat2::from_cols_array(&[0.8, 0.6, -0.6, 0.8]);
    let mut sum = 0.0;
    let mut amplitude = 0.5;
    for _ in 0..5 {
        sum += amplitude * noise(p);
        p = TURN * p * 2.03 + Vec2::splat(17.1);
        amplitude *= 0.5;
    }
    sum / 0.96875
}

/// Value noise, 0..1, smooth to the second derivative.
fn noise(p: Vec2) -> f32 {
    let cell = p.floor();
    let f = p - cell;
    let w = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let (x, y) = (cell.x as i32, cell.y as i32);
    let a = hash(x, y);
    let b = hash(x + 1, y);
    let c = hash(x, y + 1);
    let d = hash(x + 1, y + 1);
    a + (b - a) * w.x + (c - a) * w.y + (a - b - c + d) * w.x * w.y
}

fn hash(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841);
    h = (h ^ (h >> 15)).wrapping_mul(0x2c1b_3c6d);
    h = (h ^ (h >> 12)).wrapping_mul(0x297a_2d39);
    (h ^ (h >> 15)) as f32 / u32::MAX as f32
}

fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texel(data: &[u8], x: usize, y: usize) -> [u8; 3] {
        let i = (y * WIDTH + x) * 4;
        [data[i], data[i + 1], data[i + 2]]
    }

    #[test]
    fn the_horizon_is_the_fog() {
        let data = pixels();
        let fog = SKY.to_srgba();
        let fog = [fog.red, fog.green, fog.blue].map(|c| (c * 255.0).round() as u8);
        for x in 0..WIDTH {
            assert_eq!(texel(&data, x, HEIGHT - 1), fog, "horizon texel {x}");
        }
    }

    #[test]
    fn the_top_row_is_clear_and_the_seam_is_invisible() {
        let data = pixels();
        let top = texel(&data, 0, 0);
        for x in 0..WIDTH {
            assert_eq!(texel(&data, x, 0), top, "top texel {x}");
        }
        // Repeat addressing joins the last column to the first; they must
        // differ no more than any two neighbours do.
        for y in 0..HEIGHT {
            let a = texel(&data, WIDTH - 1, y);
            let b = texel(&data, 0, y);
            let c = texel(&data, 1, y);
            let seam = a.iter().zip(b).map(|(a, b)| a.abs_diff(b)).max().unwrap();
            let step = b.iter().zip(c).map(|(b, c)| b.abs_diff(c)).max().unwrap();
            assert!(seam <= step.max(4) + 4, "row {y}: seam {seam}, step {step}");
        }
    }

    #[test]
    fn there_are_clouds() {
        let density = density();
        let low = &density[(HEIGHT - 60) * WIDTH..(HEIGHT - 40) * WIDTH];
        let cloudy = low.iter().filter(|&&d| d > 0.6).count() as f32 / low.len() as f32;
        assert!((0.1..0.6).contains(&cloudy), "cloud cover {cloudy}");
    }
}
