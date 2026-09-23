//! Three quiet chevron plaques on the outside verge of the upcoming bend.
//! Retains the original alternating marker positions and 60 m visibility range.

use super::{
    Track,
    profile::{HALF_WIDTH, paint},
    ribbon::{STEP, Station},
};
use crate::car::{Car, Player};
use bevy::{
    asset::RenderAssetUsages, light::NotShadowCaster, prelude::*,
    render::render_resource::PrimitiveTopology,
};

const CORNER_RADIUS: f32 = 35.0;
const MERGE: f32 = 6.0;
const SMOOTH: usize = 9;
/// Original layout spacing; every other position is retained for display.
const SPACING: usize = 11;
const APPROACH: usize = 5;
/// Compact plaques leave room for the wider road at tight passages.
pub(super) const HALF: f32 = 0.12;
pub(super) const CLEAR: f32 = 0.04;
pub(super) const ROOM: f32 = 2.0 * (HALF + CLEAR);
const OUT: f32 = 0.5;
const LIFT: f32 = 0.015;
const MAX_MARKERS: usize = 3;
const VISIBLE_AHEAD: f32 = 60.0;
const BASE: (f32, f32, f32) = (0.16, 0.19, 0.18);
const CHEVRON: (f32, f32, f32) = (0.56, 0.59, 0.51);

#[derive(Component)]
pub(super) struct Marker {
    s: f32,
    /// Zero works in both directions. On a straight between opposite bends,
    /// the outside verge depends on which bend the driver is approaching.
    direction: f32,
}

pub(super) fn rebuild(
    mut commands: Commands,
    track: Res<Track>,
    old: Query<Entity, With<Marker>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let material = materials.add(StandardMaterial {
        perceptual_roughness: 1.0,
        reflectance: 0.1,
        ..default()
    });
    let stations = track.ribbon.stations();
    let bends = smoothed_curvature(stations);
    let mut placed = vec![false; stations.len()];
    for (at, step) in markers(stations) {
        if step % 2 != 0 || placed[at] {
            continue;
        }
        placed[at] = true;
        let forward = outside(&bends, at, 1);
        let reverse = outside(&bends, at, -1);
        for (side, direction) in if forward == reverse {
            vec![(forward, 0.0)]
        } else {
            vec![(forward, 1.0), (reverse, -1.0)]
        } {
            commands.spawn((
                Marker {
                    s: stations[at].s,
                    direction,
                },
                Mesh3d(meshes.add(plaque(&track, at, side))),
                MeshMaterial3d(material.clone()),
                Visibility::Hidden,
                NotShadowCaster,
            ));
        }
    }
}

pub(super) fn show(
    track: Res<Track>,
    cars: Query<(&Car, &Transform), With<Player>>,
    mut markers: Query<(Entity, &Marker, &mut Visibility)>,
) {
    let Ok((car, transform)) = cars.single() else {
        for (_, _, mut visibility) in &mut markers {
            *visibility = Visibility::Hidden;
        }
        return;
    };
    let ground = track.ground_from(transform.translation, car.along);
    let travel = if car.velocity.length_squared() > 1.0 {
        car.velocity
    } else {
        *transform.forward()
    };
    let direction = if travel.dot(ground.tangent) < 0.0 {
        -1.0
    } else {
        1.0
    };
    let mut ahead: Vec<_> = markers
        .iter()
        .filter_map(|(entity, marker, _)| {
            let distance = ((marker.s - ground.s) * direction).rem_euclid(track.ribbon.length());
            (distance > 0.0
                && distance <= VISIBLE_AHEAD
                && (marker.direction == 0.0 || marker.direction == direction))
                .then_some((entity, distance))
        })
        .collect();
    ahead.sort_unstable_by(|a, b| a.1.total_cmp(&b.1));
    ahead.truncate(MAX_MARKERS);
    for (entity, _, mut visibility) in &mut markers {
        visibility.set_if_neq(if ahead.iter().any(|&(next, _)| next == entity) {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
}

fn smoothed_curvature(stations: &[Station]) -> Vec<f32> {
    smooth(&stations.iter().map(|s| s.curvature).collect::<Vec<_>>())
}

fn smooth(curvature: &[f32]) -> Vec<f32> {
    let n = curvature.len();
    (0..n)
        .map(|i| {
            (0..SMOOTH)
                .map(|d| curvature[(i + n + d - SMOOTH / 2) % n])
                .sum::<f32>()
                / SMOOTH as f32
        })
        .collect()
}

/// Positive curvature turns right, so its outside is the left verge.
/// Approach markers inherit the bend ahead; exit markers fall back to the
/// bend just passed. Never choose by world-space proximity across a hairpin.
fn outside(bends: &[f32], at: usize, direction: i32) -> f32 {
    let reach = APPROACH * SPACING + (MERGE / STEP) as usize + SMOOTH;
    for travel in [direction, -direction] {
        for d in 0..=reach {
            let i = (at as i32 + travel * d as i32).rem_euclid(bends.len() as i32) as usize;
            if bends[i].abs() * CORNER_RADIUS > 1.0 {
                return -bends[i].signum();
            }
        }
    }
    unreachable!("a marked station has a corner within its approach or exit")
}

fn lateral(grass: f32) -> f32 {
    let nearest = HALF + CLEAR;
    let furthest = (grass - HALF - CLEAR).max(nearest);
    HALF_WIDTH + OUT.clamp(nearest, furthest)
}

/// A flat plate with clipped corners and a thin chevron, following the verge's
/// surface. It keeps the old marker's centre and clearance without a tall tip.
fn plaque(track: &Track, at: usize, side: f32) -> Mesh {
    let stations = track.ribbon.stations();
    let profile = &track.profile;
    let station = &stations[at];
    let here = lateral(
        [-HALF, 0.0, HALF]
            .into_iter()
            .map(|along| {
                let (on, t) = under(at, along, stations.len());
                profile.grass(on, t, side)
            })
            .fold(f32::MAX, f32::min),
    ) * side;
    let point = |across: f32, along: f32, lift: f32| {
        let lateral = here + across;
        let mut position = station.pos + station.right * lateral + station.tangent * along;
        position.y = track.ground_from(position, Some(station.s)).height + LIFT + lift;
        position
    };
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut colors = Vec::new();
    let mut triangle = |mut face: [Vec3; 3], color| {
        if (face[1] - face[0]).cross(face[2] - face[0]).y < 0.0 {
            face.swap(1, 2);
        }
        let normal = (face[1] - face[0])
            .cross(face[2] - face[0])
            .normalize()
            .to_array();
        for corner in face {
            positions.push(corner.to_array());
            normals.push(normal);
            colors.push(color);
        }
    };
    let edge = [
        (-HALF * 0.75, -HALF),
        (HALF * 0.75, -HALF),
        (HALF, -HALF * 0.75),
        (HALF, HALF * 0.75),
        (HALF * 0.75, HALF),
        (-HALF * 0.75, HALF),
        (-HALF, HALF * 0.75),
        (-HALF, -HALF * 0.75),
    ];
    for i in 0..edge.len() {
        let (a, b) = (edge[i], edge[(i + 1) % edge.len()]);
        triangle(
            [
                point(0.0, 0.0, 0.0),
                point(a.0, a.1, 0.0),
                point(b.0, b.1, 0.0),
            ],
            paint(BASE.0, BASE.1, BASE.2),
        );
    }
    for arm in [
        [(-0.11, 0.0), (0.11, -0.12), (0.11, -0.06), (-0.01, 0.0)],
        [(-0.11, 0.0), (-0.01, 0.0), (0.11, 0.06), (0.11, 0.12)],
    ] {
        let p = arm.map(|(x, z)| point(x * side * (HALF / 0.20), z * (HALF / 0.20), 0.002));
        for face in [[p[0], p[1], p[2]], [p[0], p[2], p[3]]] {
            triangle(face, paint(CHEVRON.0, CHEVRON.1, CHEVRON.2));
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
}

/// The centre of every plaque in either direction, as [`rebuild`] lays them.
#[cfg(test)]
pub(super) fn plaque_centres(track: &Track) -> Vec<Vec3> {
    let stations = track.ribbon.stations();
    let bends = smoothed_curvature(stations);
    let mut out = Vec::new();
    for (at, step) in markers(stations) {
        if step % 2 != 0 {
            continue;
        }
        for direction in [1, -1] {
            let mesh = plaque(track, at, outside(&bends, at, direction));
            let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap();
            out.push(Vec3::from(positions.as_float3().unwrap()[0]));
        }
    }
    out
}

/// Which cross-section a point `along` metres up or down the road from station
/// `at` stands on, as a station and a fraction of the way to the next.
fn under(at: usize, along: f32, n: usize) -> (usize, f32) {
    let down = (at as f32 + along / STEP).rem_euclid(n as f32);
    (down as usize % n, down.fract())
}

/// Original marker stations and their sequence numbers within each run.
/// Keeping the even sequence numbers preserves the established spacing.
///
/// One line per stretch of marked road, laid from the head of it so that the
/// spacing is exact all the way down and the last marker of the approach lands
/// on the corner entry itself.
fn markers(stations: &[Station]) -> Vec<(usize, usize)> {
    let n = stations.len();
    let zone = zone(&corners(stations));
    let mut out = Vec::new();
    for head in 0..n {
        if !zone[head] || zone[(head + n - 1) % n] {
            continue;
        }
        let mut at = 0;
        while at < n && zone[(head + at) % n] {
            out.push(((head + at) % n, at / SPACING));
            at += SPACING;
        }
    }
    out
}

/// The road the markers are laid along: every corner, and [`APPROACH`] of them
/// worth of the run up to it. Corners close enough together share one stretch,
/// so the line carries on through a complex instead of restarting inside it.
fn zone(corner: &[bool]) -> Vec<bool> {
    let n = corner.len();
    let mut zone = corner.to_vec();
    for entry in 0..n {
        // The first station of a corner, which is what the approach counts into.
        if !corner[entry] || corner[(entry + n - 1) % n] {
            continue;
        }
        for d in 1..=APPROACH * SPACING {
            zone[(entry + n - d) % n] = true;
        }
    }
    zone
}

/// Which stations are inside a corner, bends closer than [`MERGE`] joined.
fn corners(stations: &[Station]) -> Vec<bool> {
    corners_of(&stations.iter().map(|s| s.curvature).collect::<Vec<_>>())
}

/// The same from the curvature at each station alone. Shared with the braking
/// boards, which count their distances back from the first station of these.
pub(super) fn corners_of(curvature: &[f32]) -> Vec<bool> {
    let n = curvature.len();
    let bend: Vec<bool> = smooth(curvature)
        .into_iter()
        .map(|curvature| curvature.abs() * CORNER_RADIUS > 1.0)
        .collect();
    let reach = (MERGE / STEP) as usize;
    (0..n)
        .map(|i| (0..=reach).any(|d| bend[(i + d) % n] || bend[(i + n - d) % n]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::circuits;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn outside_follows_the_bend_including_approaches_in_either_direction() {
        let mut bends = vec![0.0; 200];
        bends[40..60].fill(0.05);
        bends[120..140].fill(-0.05);
        assert_eq!(outside(&bends, 20, 1), -1.0);
        assert_eq!(outside(&bends, 50, 1), -1.0);
        assert_eq!(outside(&bends, 50, -1), -1.0);
        assert_eq!(outside(&bends, 80, 1), 1.0);
        assert_eq!(outside(&bends, 80, -1), -1.0);
        assert_eq!(outside(&bends, 130, -1), 1.0);
        assert_eq!(outside(&bends, 195, 1), -1.0);
    }

    #[test]
    fn plaques_stay_low_on_the_outside_shoulder_and_point_inward() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let bends = smoothed_curvature(stations);
            for (at, step) in markers(stations) {
                if step % 2 != 0 {
                    continue;
                }
                for direction in [1, -1] {
                    let side = outside(&bends, at, direction);
                    if bends[at].abs() * CORNER_RADIUS > 1.0 {
                        assert!(side * bends[at] < 0.0, "{name}: marker inside the bend");
                    }
                    let mesh = plaque(&track, at, side);
                    let positions = mesh
                        .attribute(Mesh::ATTRIBUTE_POSITION)
                        .unwrap()
                        .as_float3()
                        .unwrap();
                    assert_eq!(positions.len(), 36);
                    for position in positions {
                        let position = Vec3::from(*position);
                        let ground = track.ground_from(position, Some(stations[at].s));
                        assert!(
                            ground.lateral * side > HALF_WIDTH + CLEAR - 0.01,
                            "{name}: plaque touches the kerb"
                        );
                        let fix = track.fix(position, Some(stations[at].s));
                        let grass = track.profile.grass(fix.at, fix.t, fix.lateral);
                        assert!(
                            ground.lateral.abs() < HALF_WIDTH + grass - CLEAR + 0.01,
                            "{name}: plaque overhangs the verge"
                        );
                        assert!(
                            (0.005..0.03).contains(&(position.y - ground.height)),
                            "{name}: plaque at {at} is {} above ground",
                            position.y - ground.height
                        );
                    }
                    for triangle in positions.chunks_exact(3) {
                        let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(triangle[i]));
                        assert!(
                            (b - a).cross(c - a).y > 0.0,
                            "{name}: degenerate or downward face"
                        );
                    }
                    // First vertex of the chevron is its tip, directed toward the road.
                    let tip = Vec3::from(positions[24]);
                    let tail = Vec3::from(positions[25]);
                    assert!((tip - tail).dot(stations[at].right) * side < 0.0);
                    let Some(VertexAttributeValues::Float32x4(colors)) =
                        mesh.attribute(Mesh::ATTRIBUTE_COLOR)
                    else {
                        panic!("missing colours")
                    };
                    assert!(
                        colors[..24]
                            .iter()
                            .all(|&c| c == paint(BASE.0, BASE.1, BASE.2))
                    );
                    assert!(
                        colors[24..]
                            .iter()
                            .all(|&c| c == paint(CHEVRON.0, CHEVRON.1, CHEVRON.2))
                    );
                }
            }
        }
    }

    #[test]
    fn only_the_first_three_alternating_positions_show_on_each_circuit() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_systems(
                Update,
                (rebuild.run_if(resource_changed::<Track>), show).chain(),
            );
        let player = app
            .world_mut()
            .spawn((Player, Car::default(), Transform::default()))
            .id();
        for (_, track) in every_track() {
            let stations = track.ribbon.stations();
            let positions: Vec<_> = [
                0,
                stations.len() / 4,
                stations.len() / 2,
                stations.len() - 1,
            ]
            .map(|at| stations[at])
            .into_iter()
            .collect();
            let kept: Vec<_> = markers(stations)
                .into_iter()
                .filter(|&(_, step)| step % 2 == 0)
                .map(|(at, _)| stations[at].s)
                .collect();
            let length = track.ribbon.length();
            app.insert_resource(track);
            for station in positions {
                for (direction, speed) in [(1.0, 0.0), (-1.0, 0.0), (1.0, 10.0), (-1.0, 10.0)] {
                    app.world_mut().entity_mut(player).insert((
                        Car {
                            along: Some(station.s),
                            velocity: station.tangent * direction * speed,
                            ..default()
                        },
                        Transform::from_translation(station.pos)
                            .looking_to(station.tangent * direction, Vec3::Y),
                    ));
                    app.update();
                    let world = app.world_mut();
                    let from = world
                        .resource::<Track>()
                        .ground_from(station.pos, Some(station.s))
                        .s;
                    assert!((from - station.s).abs() < 0.01);
                    let mut query = world.query::<(&Marker, &Visibility)>();
                    // Switching replaces the old markers without moving the retained positions.
                    let mut actual: Vec<_> =
                        query.iter(world).map(|(marker, _)| marker.s).collect();
                    actual.sort_unstable_by(f32::total_cmp);
                    actual.dedup();
                    let mut expected = kept.clone();
                    expected.sort_unstable_by(f32::total_cmp);
                    expected.dedup();
                    assert_eq!(actual, expected, "marker positions changed");
                    assert!(
                        query
                            .iter(world)
                            .all(|(marker, _)| kept.contains(&marker.s))
                    );
                    let mut visible = Vec::new();
                    let mut hidden_ahead = Vec::new();
                    for (marker, visibility) in query.iter(world) {
                        let distance = ((marker.s - from) * direction).rem_euclid(length);
                        let eligible = marker.direction == 0.0 || marker.direction == direction;
                        if *visibility == Visibility::Visible {
                            assert!(eligible);
                            assert!(
                                distance > 0.0 && distance <= VISIBLE_AHEAD,
                                "{}: from {} to {}, direction {}, distance {}",
                                world.resource::<Track>().circuit.name,
                                station.s,
                                marker.s,
                                direction,
                                distance
                            );
                            visible.push(distance);
                        } else if eligible && distance > 0.0 && distance <= VISIBLE_AHEAD {
                            hidden_ahead.push(distance);
                        }
                    }
                    assert!(visible.len() <= 3);
                    assert_eq!(visible.len(), (visible.len() + hidden_ahead.len()).min(3));
                    assert!(visible.iter().all(|v| hidden_ahead.iter().all(|h| v <= h)));
                }
            }
        }
    }

    /// Every circuit, so a new one has to clear the same bar as the old ones.
    fn every_track() -> impl Iterator<Item = (&'static str, Track)> {
        circuits::all()
            .iter()
            .map(|circuit| (circuit.name, Track::new(circuit)))
    }

    /// Markers mean a corner, so a marker where there is no corner means
    /// nothing. Every one is either in a bend or inside the run up to one —
    /// never a stray dot down the back straight.
    #[test]
    fn a_marker_means_a_corner() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let n = stations.len();
            let corner = corners(stations);
            for (at, _) in markers(stations) {
                let coming = corner[at] || (1..=APPROACH * SPACING).any(|d| corner[(at + d) % n]);
                assert!(
                    coming,
                    "{name}: a marker at station {at} has no corner to mark"
                );
            }
        }
    }

    /// The point of the line. Every corner long enough to hold a marker carries
    /// one, so it goes round the bend rather than up to it and no further.
    #[test]
    fn the_line_goes_through_the_corner() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let n = stations.len();
            let corner = corners(stations);
            let marked = markers(stations);
            let mut checked = 0;
            for entry in 0..n {
                if !corner[entry] || corner[(entry + n - 1) % n] {
                    continue;
                }
                let length = (0..n).take_while(|&d| corner[(entry + d) % n]).count();
                if length < SPACING {
                    continue;
                }
                checked += 1;
                assert!(
                    marked.iter().any(|&(at, _)| (at + n - entry) % n < length),
                    "{name}: the corner at station {entry} is {length} stations \
                     long and carries no marker"
                );
            }
            assert!(checked > 0, "{name} has no corner long enough to test");
        }
    }

    /// The rhythm. Down any one stretch of marked road every marker is exactly
    /// [`SPACING`] from the last, and one further along the line than it — a
    /// line is read rather than counted, and only a regular one can be.
    #[test]
    fn the_line_keeps_its_rhythm() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let n = stations.len();
            let zone = zone(&corners(stations));
            let marked = markers(stations);
            for pair in marked.windows(2) {
                let ((at, step), (next, then)) = (pair[0], pair[1]);
                let gap = (next + n - at) % n;
                if (0..gap).all(|d| zone[(at + d) % n]) {
                    assert_eq!(
                        gap, SPACING,
                        "{name}: the marker at station {at} is {gap} stations \
                         from the next one down the same stretch"
                    );
                    assert_eq!(
                        then,
                        step + 1,
                        "{name}: the line skips from place {step} to {then}"
                    );
                }
            }
        }
    }

    /// Enough of them to be a line, on all three. The count is far more than the
    /// approach alone, because most of it is the corner.
    #[test]
    fn every_circuit_is_marked_all_the_way_round_its_corners() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let n = stations.len();
            let corner = corners(stations);
            let entries = (0..n)
                .filter(|&i| corner[i] && !corner[(i + n - 1) % n])
                .count();
            let marked = markers(stations).len();
            assert!(
                marked > entries * APPROACH,
                "{name}: {marked} markers for {entries} corners is the approach \
                 and nothing round the bend"
            );
        }
    }
}
