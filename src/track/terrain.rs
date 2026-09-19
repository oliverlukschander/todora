//! Continuous grass around the road, extending to a rectangular outer boundary.
use super::{
    profile::{self, Profile, RIBS},
    ribbon::Ribbon,
};
use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, VertexBuffers, math::point,
    path::Path,
};
use std::collections::HashMap;

const MAX_EDGE: f32 = 5.0;
const MARGIN: f32 = 50.0;

pub(super) fn fill(profile: &Profile, ribbon: &Ribbon) -> Mesh {
    let lips = lips(profile, ribbon);
    let mut min = Vec2::splat(f32::MAX);
    let mut max = Vec2::splat(f32::MIN);
    for p in lips.iter().flatten() {
        min = min.min(xz(*p));
        max = max.max(xz(*p));
    }
    let mut path = Path::builder();
    // Even-odd fill of an enclosing rectangle minus the road ribbon produces
    // the exact infields and exterior, sharing the existing verge boundaries.
    // Measure the margin from the outermost grass lip, not the centreline.
    min -= Vec2::splat(MARGIN);
    max += Vec2::splat(MARGIN);
    let bounds = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)];
    for ring in [
        bounds.to_vec(),
        lips[0].iter().map(|p| xz(*p)).collect(),
        lips[1].iter().map(|p| xz(*p)).collect(),
    ] {
        path.begin(point(ring[0].x, ring[0].y));
        for p in &ring[1..] {
            path.line_to(point(p.x, p.y));
        }
        path.end(true);
    }
    let mut buffers: VertexBuffers<Vec2, u32> = VertexBuffers::new();
    FillTessellator::new()
        .tessellate_path(
            &path.build(),
            &FillOptions::default().with_fill_rule(FillRule::EvenOdd),
            &mut BuffersBuilder::new(&mut buffers, |v: FillVertex| {
                Vec2::new(v.position().x, v.position().y)
            }),
        )
        .expect("finite circuit verge paths must tessellate");
    let mut faces: Vec<[u32; 3]> = buffers
        .indices
        .chunks_exact(3)
        .map(|f| [f[0], f[1], f[2]])
        .filter(|f| {
            let p = (buffers.vertices[f[0] as usize]
                + buffers.vertices[f[1] as usize]
                + buffers.vertices[f[2] as usize])
                / 3.0;
            // At an overpass, XOR also creates the overlap of the two road
            // strips. It is road, not grass, and is excluded as a whole face.
            let fix = ribbon.locate(Vec3::new(p.x, 0.0, p.y));
            fix.lateral.abs() > profile.reach(fix.at, fix.t, fix.lateral)
        })
        .collect();
    refine(&mut buffers.vertices, &mut faces, profile, ribbon);
    let mut positions: Vec<_> = buffers
        .vertices
        .iter()
        .map(|p| [p.x, height(*p, &lips), p.y])
        .collect();
    // XZ counter-clockwise is downwards in the game's right-handed XYZ space.
    for f in &mut faces {
        let [a, b, c] = f.map(|i| buffers.vertices[i as usize]);
        if (b - a).perp_dot(c - a) > 0.0 {
            f.swap(1, 2);
        }
    }
    // Sample the smooth height field for shading. Averaging face normals on
    // long, narrow boundary triangles leaves visible fan-shaped streaks.
    let mut normals: Vec<_> = buffers
        .vertices
        .iter()
        .zip(&positions)
        .map(|(p, position)| {
            let dx = (height(*p + Vec2::X * 0.05, &lips) - position[1]) / 0.05;
            let dz = (height(*p + Vec2::Y * 0.05, &lips) - position[1]) / 0.05;
            // At a bridge, adjacent height branches meet in a narrow bank.
            // Keep its lighting continuous with the grass around it.
            let slope = Vec2::new(dx, dz).clamp_length_max(0.5);
            Vec3::new(-slope.x, 1.0, -slope.y).normalize().to_array()
        })
        .collect();
    close_verge_joins(&lips, &mut positions, &mut normals, &mut faces);
    ground_beneath_bridges(
        profile,
        ribbon,
        &lips,
        &mut positions,
        &mut normals,
        &mut faces,
    );
    let colors = vec![profile::paint(0.28, 0.46, 0.20); positions.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(faces.into_iter().flatten().collect()))
}

// The upper road cuts a strip from the top surface, but it must still have
// earth beneath it. Continue the lower verge under the elevated bridge span.
fn ground_beneath_bridges(
    profile: &Profile,
    ribbon: &Ribbon,
    lips: &[Vec<Vec3>; 2],
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    faces: &mut Vec<[u32; 3]>,
) {
    let mut quad = |points: [Vec3; 4]| {
        let first = positions.len() as u32;
        positions.extend(points.map(|p| p.to_array()));
        normals.extend([Vec3::Y.to_array(); 4]);
        for mut face in [
            [first, first + 1, first + 2],
            [first + 1, first + 3, first + 2],
        ] {
            let [a, b, c] = face.map(|v| Vec3::from(positions[v as usize]));
            if (b - a).cross(c - a).y < 0.0 {
                face.swap(1, 2);
            }
            faces.push(face);
        }
    };
    for &crossing in ribbon.crossings() {
        let branches = ribbon.nearby(crossing, super::ribbon::AT_CROSSING);
        let Some(lower) = branches
            .iter()
            .min_by(|a, b| a.point.y.total_cmp(&b.point.y))
        else {
            continue;
        };
        let ribs = profile.at(lower.at);
        let base = lower.point.y + ribs[0].1.min(ribs[RIBS - 1].1) - 0.05;
        let stations = ribbon.stations();
        let spans: Vec<_> = stations
            .iter()
            .map(|station| {
                xz(station.pos).distance(xz(crossing)) <= super::ribbon::AT_CROSSING
                    && station.pos.y >= lower.point.y + 2.0
            })
            .collect();
        for (i, &span) in spans.iter().enumerate() {
            if !span {
                continue;
            }
            let j = (i + 1) % stations.len();
            let floor = [lips[0][i], lips[1][i], lips[0][j], lips[1][j]].map(|p| {
                let y = base + (p - lower.point).dot(lower.tangent) * lower.slope;
                Vec3::new(p.x, y, p.z)
            });
            quad(floor);
            // Join the earth under the span to the banks alongside it. Leave
            // the full lower road and verge open through the bridge.
            for (a, b, lip) in [(0, 2, 0), (1, 3, 1)] {
                let start = lips[lip][i];
                let end = lips[lip][j];
                let along = xz(end - start);
                let mut cuts = vec![0.0, 1.0];
                for ring in lips {
                    for k in 0..ring.len() {
                        let c = ring[k];
                        let d = ring[(k + 1) % ring.len()];
                        if (c.y + d.y) * 0.5 > lower.point.y + 1.0 {
                            continue;
                        }
                        let across = xz(d - c);
                        let denominator = along.perp_dot(across);
                        if denominator.abs() < 1e-6 {
                            continue;
                        }
                        let t = xz(c - start).perp_dot(across) / denominator;
                        let u = xz(c - start).perp_dot(along) / denominator;
                        if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
                            cuts.push(t);
                        }
                    }
                }
                cuts.sort_by(f32::total_cmp);
                for pair in cuts.windows(2) {
                    let [from, to] = [pair[0], pair[1]];
                    let p = start.lerp(end, (from + to) * 0.5);
                    let nearby = ribbon.nearby(p, super::ribbon::AT_CROSSING);
                    let ground = nearby
                        .iter()
                        .min_by(|a, b| a.point.y.total_cmp(&b.point.y))
                        .unwrap();
                    if ground.lateral.abs() > profile.reach(ground.at, ground.t, ground.lateral) {
                        quad([
                            floor[a].lerp(floor[b], from),
                            floor[a].lerp(floor[b], to),
                            start.lerp(end, from),
                            start.lerp(end, to),
                        ]);
                    }
                }
            }
            // End banks close the ground beneath the bridge approaches.
            if !spans[(i + stations.len() - 1) % stations.len()] {
                quad([floor[0], floor[1], lips[0][i], lips[1][i]]);
            }
            if !spans[j] {
                quad([floor[2], floor[3], lips[0][j], lips[1][j]]);
            }
        }
    }
}

// An intersection has one terrain height but two verge heights. Close the
// vertical wedge beneath the upper verge, without filling the lower roadway.
fn close_verge_joins(
    lips: &[Vec<Vec3>; 2],
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    faces: &mut Vec<[u32; 3]>,
) {
    let mut edges = HashMap::new();
    for &[a, b, c] in faces.iter() {
        for (a, b) in [(a, b), (b, c), (c, a)] {
            let entry = edges.entry((a.min(b), a.max(b))).or_insert((a, b, 0));
            entry.2 += 1;
        }
    }
    for (_, (a, b, count)) in edges {
        if count != 1 {
            continue;
        }
        let pa = Vec3::from(positions[a as usize]);
        let pb = Vec3::from(positions[b as usize]);
        let midpoint = xz((pa + pb) * 0.5);
        let project = |p: Vec2, start: Vec3, end: Vec3| {
            let ab = xz(end - start);
            start.lerp(
                end,
                ((p - xz(start)).dot(ab) / ab.length_squared().max(1e-10)).clamp(0.0, 1.0),
            )
        };
        let (start, end) = lips
            .iter()
            .flat_map(|ring| (0..ring.len()).map(move |i| (ring[i], ring[(i + 1) % ring.len()])))
            .min_by(|&(a, b), &(c, d)| {
                midpoint
                    .distance_squared(xz(project(midpoint, a, b)))
                    .total_cmp(&midpoint.distance_squared(xz(project(midpoint, c, d))))
            })
            .unwrap();
        // The rectangle's outer edge ends freely; only verge joins need caps.
        if midpoint.distance_squared(xz(project(midpoint, start, end))) > 0.0001 {
            continue;
        }
        let top_a = Vec3::new(pa.x, project(xz(pa), start, end).y, pa.z);
        let top_b = Vec3::new(pb.x, project(xz(pb), start, end).y, pb.z);
        if (top_a.y - pa.y).abs().max((top_b.y - pb.y).abs()) < 0.001 {
            continue;
        }
        let first = positions.len() as u32;
        positions.extend([
            pa.to_array(),
            pb.to_array(),
            top_a.to_array(),
            top_b.to_array(),
        ]);
        // These narrow joins continue the grass colour. A vertical face
        // normal turns them into black wedges beneath the overpass.
        normals.extend([Vec3::Y.to_array(); 4]);
        faces.extend([
            [first, first + 2, first + 1],
            [first + 1, first + 2, first + 3],
        ]);
    }
}

fn xz(p: Vec3) -> Vec2 {
    Vec2::new(p.x, p.z)
}
fn lips(profile: &Profile, ribbon: &Ribbon) -> [Vec<Vec3>; 2] {
    [0, RIBS - 1].map(|rib| {
        ribbon
            .stations()
            .iter()
            .enumerate()
            .map(|(i, st)| {
                let (across, up, _) = profile.at(i)[rib];
                st.pos + st.right * across + Vec3::Y * up
            })
            .collect()
    })
}

// Split every shared long edge on both faces in the same pass. Independent
// longest-edge subdivision leaves T-junctions that open when heights change.
fn refine(vertices: &mut Vec<Vec2>, faces: &mut Vec<[u32; 3]>, profile: &Profile, ribbon: &Ribbon) {
    let spacing = |p: Vec2| {
        let fix = ribbon.locate(Vec3::new(p.x, 0.0, p.y));
        let distance = p.distance(xz(fix.point)) - profile.reach(fix.at, fix.t, fix.lateral);
        MAX_EDGE + (distance.max(0.0) * 0.5).min(15.0)
    };
    let mut sizes: Vec<_> = vertices.iter().copied().map(spacing).collect();
    loop {
        let mut midpoints = HashMap::new();
        for &[a, b, c] in faces.iter() {
            for (a, b) in [(a, b), (b, c), (c, a)] {
                let edge = sizes[a as usize].min(sizes[b as usize]);
                if vertices[a as usize].distance_squared(vertices[b as usize]) > edge * edge {
                    let key = (a.min(b), a.max(b));
                    midpoints.entry(key).or_insert_with(|| {
                        let id = vertices.len() as u32;
                        vertices.push((vertices[a as usize] + vertices[b as usize]) * 0.5);
                        sizes.push(spacing(vertices[id as usize]));
                        id
                    });
                }
            }
        }
        if midpoints.is_empty() {
            break;
        }
        let mut next = Vec::with_capacity(faces.len() * 2);
        for &[a, b, c] in faces.iter() {
            let midpoint = |a: u32, b: u32| midpoints.get(&(a.min(b), a.max(b))).copied();
            match (midpoint(a, b), midpoint(b, c), midpoint(c, a)) {
                (None, None, None) => next.push([a, b, c]),
                (Some(ab), None, None) => next.extend([[a, ab, c], [ab, b, c]]),
                (None, Some(bc), None) => next.extend([[a, b, bc], [a, bc, c]]),
                (None, None, Some(ca)) => next.extend([[a, b, ca], [b, c, ca]]),
                (Some(ab), Some(bc), None) => next.extend([[ab, b, bc], [a, ab, c], [ab, bc, c]]),
                (Some(ab), None, Some(ca)) => next.extend([[a, ab, ca], [ab, b, c], [ab, c, ca]]),
                (None, Some(bc), Some(ca)) => next.extend([[ca, bc, c], [a, b, ca], [b, bc, ca]]),
                (Some(ab), Some(bc), Some(ca)) => {
                    next.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]])
                }
            }
        }
        *faces = next;
    }
}

// Blend nearby verge heights continuously across the ground. Exactly on an
// edge use its original height so the two meshes meet, with no lowered seam.
fn height(p: Vec2, lips: &[Vec<Vec3>; 2]) -> f32 {
    let mut total = 0.0_f64;
    let mut weighted = 0.0_f64;
    let mut boundary = f32::INFINITY;
    for ring in lips {
        for i in 0..ring.len() {
            let a = ring[i];
            let b = ring[(i + 1) % ring.len()];
            let ab = xz(b - a);
            let t = ((p - xz(a)).dot(ab) / ab.length_squared().max(1e-10)).clamp(0.0, 1.0);
            let q = a.lerp(b, t);
            let distance = p.distance_squared(xz(q));
            if distance < 1e-8 {
                boundary = boundary.min(q.y);
            }
            let weight = f64::from(ab.length() / (distance + 0.01).powi(2));
            weighted += weight * f64::from(q.y);
            total += weight;
        }
    }
    if boundary.is_finite() {
        boundary
    } else {
        (weighted / total) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::super::profile::TARMAC_HALF;
    use super::*;
    use crate::track::{Track, all_circuits};

    #[test]
    fn ground_continues_beneath_both_sides_of_suzukas_bridge() {
        let track = Track::new(all_circuits().iter().find(|c| c.id == "suzuka").unwrap());
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut faces = Vec::new();
        ground_beneath_bridges(
            &track.profile,
            &track.ribbon,
            &lips(&track.profile, &track.ribbon),
            &mut positions,
            &mut normals,
            &mut faces,
        );
        for f in &faces {
            let [a, b, c] = f.map(|i| Vec3::from(positions[i as usize]));
            for p in [
                (a + b + c) / 3.0,
                a * 0.8 + b * 0.1 + c * 0.1,
                a * 0.1 + b * 0.8 + c * 0.1,
                a * 0.1 + b * 0.1 + c * 0.8,
            ] {
                let ground = track.ground_from(p, None);
                assert!(
                    ground.lateral.abs() + 0.05 >= TARMAC_HALF || p.y < ground.height - 0.02,
                    "bridge ground blocks the road at {p}"
                );
            }
        }
        let branches = track.ribbon.nearby(
            track.ribbon.crossings()[0],
            super::super::ribbon::AT_CROSSING,
        );
        let lower = branches
            .iter()
            .min_by(|a, b| a.point.y.total_cmp(&b.point.y))
            .unwrap();
        for side in [-1.0, 1.0] {
            let across = side * (track.profile.reach(lower.at, lower.t, side) + 0.25);
            let p = xz(lower.point + lower.right * across);
            assert!(
                faces.iter().any(|f| {
                    let [a, b, c] = f.map(|i| xz(Vec3::from(positions[i as usize])));
                    let area = (b - a).perp_dot(c - a);
                    if area.abs() < 1e-6 {
                        return false;
                    }
                    let u = (b - p).perp_dot(c - p) / area;
                    let v = (c - p).perp_dot(a - p) / area;
                    u >= -1e-4 && v >= -1e-4 && u + v <= 1.0001
                }),
                "no ground beside the lower road on side {side}"
            );
        }
    }

    #[test]
    fn every_course_has_complete_ground_with_a_fifty_metre_margin() {
        for circuit in all_circuits() {
            let track = Track::new(circuit);
            let mesh = fill(&track.profile, &track.ribbon);
            let positions = mesh
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .unwrap()
                .as_float3()
                .unwrap();
            let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
            assert!(indices.len() >= 3, "{}: no grass", circuit.name);
            let boundaries = lips(&track.profile, &track.ribbon);
            let mut min = Vec2::splat(f32::INFINITY);
            let mut max = Vec2::splat(f32::NEG_INFINITY);
            for p in boundaries.iter().flatten() {
                min = min.min(xz(*p));
                max = max.max(xz(*p));
            }
            min -= Vec2::splat(50.0);
            max += Vec2::splat(50.0);
            let mut drawn_min = Vec2::splat(f32::INFINITY);
            let mut drawn_max = Vec2::splat(f32::NEG_INFINITY);
            for &i in &indices {
                let p = xz(Vec3::from(positions[i]));
                drawn_min = drawn_min.min(p);
                drawn_max = drawn_max.max(p);
            }
            assert!(
                drawn_min.distance(min) < 0.001,
                "{}: lower margin",
                circuit.name
            );
            assert!(
                drawn_max.distance(max) < 0.001,
                "{}: upper margin",
                circuit.name
            );
            let mut edges = HashMap::<(usize, usize), usize>::new();
            for f in indices.chunks_exact(3) {
                let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[f[i]]));
                assert!(a.is_finite() && b.is_finite() && c.is_finite());
                assert!(
                    (b - a).cross(c - a).y >= -1e-7,
                    "{}: downward face",
                    circuit.name
                );
                for (a, b) in [(f[0], f[1]), (f[1], f[2]), (f[2], f[0])] {
                    *edges.entry((a.min(b), a.max(b))).or_default() += 1;
                }
                for p in [
                    (a + b + c) / 3.0,
                    a * 0.8 + b * 0.1 + c * 0.1,
                    a * 0.1 + b * 0.8 + c * 0.1,
                    a * 0.1 + b * 0.1 + c * 0.8,
                ] {
                    let ground = track.ground_from(p, None);
                    assert!(
                        ground.lateral.abs() + 0.05 >= TARMAC_HALF || p.y < ground.height - 0.02,
                        "{}: grass over road at {p}",
                        circuit.name
                    );
                }
            }
            let mut outer_perimeter = 0.0;
            for ((a, b), count) in edges {
                assert!(count <= 2, "{}: overlapping faces", circuit.name);
                if count == 2 {
                    continue;
                }
                let p = xz((Vec3::from(positions[a]) + Vec3::from(positions[b])) / 2.0);
                let midpoint = (Vec3::from(positions[a]) + Vec3::from(positions[b])) * 0.5;
                let ground = track.ground_from(midpoint, None);
                // The ends of the bridge's lower ground layer lie beneath
                // the upper road, with their side edges on the verge outline.
                if ribbon_near_bridge(&track.ribbon, p) && midpoint.y < ground.height - 0.02 {
                    continue;
                }
                let border_distance = (p.x - min.x)
                    .abs()
                    .min((p.x - max.x).abs())
                    .min((p.y - min.y).abs())
                    .min((p.y - max.y).abs());
                if border_distance < 0.001 {
                    outer_perimeter +=
                        xz(Vec3::from(positions[a])).distance(xz(Vec3::from(positions[b])));
                    continue;
                }
                let distance = boundaries
                    .iter()
                    .flat_map(|ring| {
                        (0..ring.len()).map(move |i| {
                            let a = xz(ring[i]);
                            let b = xz(ring[(i + 1) % ring.len()]);
                            let ab = b - a;
                            p.distance(
                                a + ab
                                    * ((p - a).dot(ab) / ab.length_squared().max(1e-10))
                                        .clamp(0.0, 1.0),
                            )
                        })
                    })
                    .fold(f32::INFINITY, f32::min);
                assert!(
                    distance < 0.015,
                    "{}: internal hole or crack at {p} ({distance} from verge)",
                    circuit.name
                );
            }
            let expected_perimeter = 2.0 * ((max.x - min.x) + (max.y - min.y));
            assert!(
                (outer_perimeter - expected_perimeter).abs() < 0.1,
                "{}: incomplete rectangle boundary ({outer_perimeter}/{expected_perimeter})",
                circuit.name
            );
        }
    }

    fn ribbon_near_bridge(ribbon: &Ribbon, p: Vec2) -> bool {
        ribbon
            .crossings()
            .iter()
            .any(|c| xz(*c).distance(p) < super::super::ribbon::AT_CROSSING + profile::EDGE)
    }
}
