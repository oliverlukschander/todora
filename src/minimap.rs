//! Small heading-up map, drawn from the same ribbon as the driving surface.
use crate::{
    car::{Car, Player, level},
    hud::Instrument,
    track::Track,
};
use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
const SIZE: usize = 256;
const CENTRE: Vec2 = Vec2::new(128.0, 143.0);
const SCALE: f32 = 2.15; // ~65 metres ahead: several seconds at racing speed.

pub struct MinimapPlugin;
impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, (draw, hide));
    }
}
#[derive(Component)]
struct Map;
fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let image = Image::new_fill(
        Extent3d {
            width: SIZE as u32,
            height: SIZE as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    commands.spawn((
        Map,
        Instrument,
        ImageNode::new(images.add(image)),
        Node {
            position_type: PositionType::Absolute,
            right: px(36),
            top: px(24),
            width: px(196),
            height: px(196),
            ..default()
        },
    ));
}
/// The settings can turn the map off.
fn hide(settings: Option<Res<crate::settings::Settings>>, mut maps: Query<&mut Node, With<Map>>) {
    let Some(settings) = settings.filter(|s| s.is_changed()) else {
        return;
    };
    for mut node in &mut maps {
        node.display = if settings.minimap {
            Display::Flex
        } else {
            Display::None
        };
    }
}

fn project(point: Vec3, at: &Transform) -> Vec2 {
    let forward = level(*at.forward());
    let delta = point - at.translation;
    CENTRE + Vec2::new(delta.dot(forward.cross(Vec3::Y)), -delta.dot(forward)) * SCALE
}
fn stroke(data: &mut [u8], a: Vec2, b: Vec2, radius: f32, color: [u8; 4]) {
    let lo = (a.min(b) - Vec2::splat(radius + 1.0))
        .floor()
        .max(Vec2::ZERO);
    let hi = (a.max(b) + Vec2::splat(radius + 1.0))
        .ceil()
        .min(Vec2::splat((SIZE - 1) as f32));
    let ab = b - a;
    for y in lo.y as usize..=hi.y as usize {
        for x in lo.x as usize..=hi.x as usize {
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            if p.distance(Vec2::splat(128.0)) > 123.0 {
                continue;
            }
            let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
            let alpha = (radius + 0.5 - p.distance(a + ab * t)).clamp(0.0, 1.0);
            let offset = (y * SIZE + x) * 4;
            for c in 0..4 {
                data[offset + c] =
                    (data[offset + c] as f32 * (1.0 - alpha) + color[c] as f32 * alpha) as u8;
            }
        }
    }
}
fn draw(
    track: Res<Track>,
    cars: Query<(&Transform, &Car), With<Player>>,
    maps: Query<&ImageNode, With<Map>>,
    mut images: ResMut<Assets<Image>>,
    time: Res<Time>,
    halt: Res<crate::pause::Halt>,
    mut last: Local<f32>,
) {
    if halt.stopped() || time.elapsed_secs() - *last < 1.0 / 30.0 {
        return;
    }
    *last = time.elapsed_secs();
    let (Ok((at, car)), Ok(map)) = (cars.single(), maps.single()) else {
        return;
    };
    let Some(mut image) = images.get_mut(&map.image) else {
        return;
    };
    let Some(data) = &mut image.data else {
        return;
    };
    render(data, &track, at, car.along);
}
fn render(data: &mut [u8], track: &Track, at: &Transform, along: Option<f32>) {
    for y in 0..SIZE {
        for x in 0..SIZE {
            let d = Vec2::new(x as f32 + 0.5, y as f32 + 0.5).distance(Vec2::splat(128.0));
            let color = if d > 126.0 {
                [0, 0, 0, 0]
            } else if d > 123.0 {
                [63, 78, 85, 240]
            } else {
                [12, 19, 23, 235]
            };
            data[(y * SIZE + x) * 4..(y * SIZE + x + 1) * 4].copy_from_slice(&color);
        }
    }
    let points: Vec<_> = track.map_points().collect();
    let here = along.unwrap_or_else(|| track.progress(at.translation, None) * track.length());
    let segments: Vec<_> = (0..points.len())
        .filter_map(|i| {
            let j = (i + 1) % points.len();
            let a = project(points[i].0, at);
            let b = project(points[j].0, at);
            (a.distance(Vec2::splat(128.0)) < 134.0 || b.distance(Vec2::splat(128.0)) < 134.0)
                .then_some((i, a, b))
        })
        .collect();
    // Complete each stroke before painting its interior. Then repeat for
    // the raised deck so its casing separates it from the lower road.
    for bridge in [false, true] {
        for outline in [true, false] {
            for &(i, a, b) in &segments {
                if bridge && !track.bridge_at(points[i].1) {
                    continue;
                }
                let ahead = (points[i].1 - here).rem_euclid(track.length());
                let color = if outline {
                    [12, 19, 23, 255]
                } else if ahead < 70.0 {
                    [176, 202, 133, 255]
                } else {
                    [130, 147, 155, 255]
                };
                stroke(data, a, b, if outline { 4.5 } else { 2.6 }, color);
            }
        }
    }
    // Fixed upward chevron with a dark surround stays legible on either deck.
    for (radius, color) in [(3.7, [8, 15, 19, 255]), (1.8, [211, 250, 120, 255])] {
        stroke(
            data,
            CENTRE + Vec2::new(-5.0, 5.0),
            CENTRE + Vec2::new(0.0, -6.0),
            radius,
            color,
        );
        stroke(
            data,
            CENTRE + Vec2::new(0.0, -6.0),
            CENTRE + Vec2::new(5.0, 5.0),
            radius,
            color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn road_strokes_are_continuous_rather_than_hidden_by_segment_caps() {
        for id in ["suzuka", "monza", "red-bull-ring"] {
            let track = Track::new(
                crate::track::all_circuits()
                    .iter()
                    .find(|c| c.id == id)
                    .unwrap(),
            );
            let at = track.start_transform();
            let along = track.start_along_lap();
            let mut pixels = vec![0; SIZE * SIZE * 4];
            render(&mut pixels, &track, &at, Some(along));
            let mut visible = 0;
            let mut readable = 0;
            for (point, s) in track.map_points() {
                let ahead = (s - along).rem_euclid(track.length());
                let p = project(point, &at);
                if (8.0..45.0).contains(&ahead) && p.distance(Vec2::splat(128.0)) < 115.0 {
                    visible += 1;
                    let offset = (p.y as usize * SIZE + p.x as usize) * 4;
                    if pixels[offset] > 100 {
                        readable += 1;
                    }
                }
            }
            assert!(
                visible > 30 && readable as f32 / visible as f32 > 0.9,
                "{id}: {readable}/{visible}"
            );
        }
    }

    #[test]
    fn player_heading_is_up_at_every_yaw() {
        for yaw in [0.0, 1.0, 2.0, 3.0] {
            let at =
                Transform::from_xyz(10.0, 3.0, -12.0).with_rotation(Quat::from_rotation_y(yaw));
            assert!(project(at.translation, &at).distance(CENTRE) < 1e-4);
            let ahead = project(at.translation + *at.forward() * 10.0, &at);
            assert!((ahead.x - CENTRE.x).abs() < 1e-4 && ahead.y < CENTRE.y);
        }
    }
}
