//! Continuous grass bounded by the actual verge edges, including crossed laps.
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

pub(super) fn fill(profile: &Profile, ribbon: &Ribbon) -> Mesh {
    let lips = lips(profile, ribbon);
    let centre: Vec<_> = ribbon.stations().iter().map(|s| xz(s.pos)).collect();
    let mut min = Vec2::splat(f32::MAX);
    let mut max = Vec2::splat(f32::MIN);
    for p in lips.iter().flatten() {
        min = min.min(xz(*p));
        max = max.max(xz(*p));
    }
    let mut path = Path::builder();
    // Even-odd fill of an enclosing rectangle minus the road ribbon produces
    // the exact infields and the exterior. Classifying those whole faces then
    // removes the exterior; unlike a grid, no partial boundary cells disappear.
    let bounds = [
        min - Vec2::ONE,
        Vec2::new(max.x + 1.0, min.y - 1.0),
        max + Vec2::ONE,
        Vec2::new(min.x - 1.0, max.y + 1.0),
    ];
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
            winding(p, &centre) != 0
                && fix.lateral.abs() > profile.reach(fix.at, fix.t, fix.lateral)
        })
        .collect();
    refine(&mut buffers.vertices, &mut faces);
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
fn winding(p: Vec2, ring: &[Vec2]) -> i32 {
    let mut w = 0;
    for i in 0..ring.len() {
        let a = ring[i];
        let b = ring[(i + 1) % ring.len()];
        let side = (b - a).perp_dot(p - a);
        if a.y <= p.y && b.y > p.y && side > 0.0 {
            w += 1;
        }
        if a.y > p.y && b.y <= p.y && side < 0.0 {
            w -= 1;
        }
    }
    w
}

// Split every shared long edge on both faces in the same pass. Independent
// longest-edge subdivision leaves T-junctions that open when heights change.
fn refine(vertices: &mut Vec<Vec2>, faces: &mut Vec<[u32; 3]>) {
    loop {
        let mut midpoints = HashMap::new();
        for &[a, b, c] in faces.iter() {
            for (a, b) in [(a, b), (b, c), (c, a)] {
                if vertices[a as usize].distance_squared(vertices[b as usize]) > MAX_EDGE * MAX_EDGE
                {
                    let key = (a.min(b), a.max(b));
                    midpoints.entry(key).or_insert_with(|| {
                        let id = vertices.len() as u32;
                        vertices.push((vertices[a as usize] + vertices[b as usize]) * 0.5);
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

// Blend nearby verge heights continuously across the interior. Exactly on an
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
    fn every_infield_is_closed_smooth_and_clear_of_the_road() {
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
                    assert!(
                        track.ground_from(p, None).lateral.abs() + 0.05 >= TARMAC_HALF,
                        "{}: grass over road at {p}",
                        circuit.name
                    );
                }
            }
            let boundaries = lips(&track.profile, &track.ribbon);
            for ((a, b), count) in edges {
                assert!(count <= 2, "{}: overlapping faces", circuit.name);
                if count == 2 {
                    continue;
                }
                let p = xz((Vec3::from(positions[a]) + Vec3::from(positions[b])) / 2.0);
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
        }
    }
}
