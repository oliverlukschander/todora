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
    let lips = ground_lips(profile, ribbon);
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
    // Cancel the upper ribbon's hole over each open span. Even-odd fill
    // then produces one continuous ground mesh underneath the bridge while
    // still cutting out the lower road, including the crossing itself.
    for span in bridge_spans(&lips, profile, ribbon) {
        let ring: Vec<_> = span
            .iter()
            .map(|&i| xz(lips[0][i]))
            .chain(span.iter().rev().map(|&i| xz(lips[1][i])))
            .collect();
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
        .collect();
    refine(&mut buffers.vertices, &mut faces, profile, ribbon);
    let positions: Vec<_> = buffers
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
    // Grass is unlit; normals need not resample the height field for shading.
    let normals = vec![Vec3::Y.to_array(); positions.len()];
    let uvs: Vec<_> = positions
        .iter()
        .map(|p| {
            [
                p[0] / super::textures::GRASS_TILE,
                p[2] / super::textures::GRASS_TILE,
            ]
        })
        .collect();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(faces.into_iter().flatten().collect()))
}

pub(super) fn bridge_spans(
    lips: &[Vec<Vec3>; 2],
    profile: &Profile,
    ribbon: &Ribbon,
) -> Vec<Vec<usize>> {
    let road_lips = lips_from_profile(profile, ribbon);
    let lowered: Vec<_> = (0..lips[0].len())
        .map(|i| (0..2).any(|side| lips[side][i].y < road_lips[side][i].y - 0.001))
        .collect();
    let n = lowered.len();
    let mut spans = Vec::new();
    for start in 0..n {
        if !lowered[start] || lowered[(start + n - 1) % n] {
            continue;
        }
        let mut span = Vec::new();
        let mut at = start;
        while lowered[at] {
            span.push(at);
            at = (at + 1) % n;
        }
        spans.push(span);
    }
    spans
}

fn xz(p: Vec3) -> Vec2 {
    Vec2::new(p.x, p.z)
}
fn lips_from_profile(profile: &Profile, ribbon: &Ribbon) -> [Vec<Vec3>; 2] {
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

// The bridge deck is a separate surface. Its verge must not pull the earth
// up through the lower road: continue that road's elevation beneath the span,
// then ease back to the upper approach outside the open passage.
pub(super) fn ground_lips(profile: &Profile, ribbon: &Ribbon) -> [Vec<Vec3>; 2] {
    let mut lips = lips_from_profile(profile, ribbon);
    for &crossing in ribbon.crossings() {
        let branches = ribbon.nearby(crossing, super::ribbon::AT_CROSSING);
        let Some(lower) = branches
            .iter()
            .min_by(|a, b| a.point.y.total_cmp(&b.point.y))
        else {
            continue;
        };
        for (i, station) in ribbon.stations().iter().enumerate() {
            let deck = profile::deep(station);
            if deck == 0.0 {
                continue;
            }
            for ring in &mut lips {
                let p = &mut ring[i];
                let lateral = (*p - lower.point).dot(lower.right);
                let passage = profile.reach(lower.at, lower.t, lateral) + 2.0;
                let t = ((lateral.abs() - passage) / 12.0).clamp(0.0, 1.0);
                let y = lower.point.y + (*p - lower.point).dot(lower.tangent) * lower.slope;
                // Keep the earth below the same soffit that the loft draws.
                p.y =
                    p.y.min(y)
                        .lerp(p.y, t * t * (3.0 - 2.0 * t))
                        .min(p.y - deck);
            }
        }
    }
    lips
}

// Split every shared long edge on both faces in the same pass. Independent
// longest-edge subdivision leaves T-junctions that open when heights change.
fn refine(vertices: &mut Vec<Vec2>, faces: &mut Vec<[u32; 3]>, profile: &Profile, ribbon: &Ribbon) {
    let spacing = |p: Vec2| {
        if ribbon.crossings().iter().any(|c| p.distance(xz(*c)) < 30.0) {
            return 1.0;
        }
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

/// Collision samples the exact rendered triangles, indexed once into 10 m cells.
/// This keeps the grass joined to the verge and avoids height-field/mesh drift.
pub(super) struct Surface {
    pub mesh: Mesh,
    triangles: Vec<[Vec3; 3]>,
    cells: HashMap<(i32, i32), Vec<usize>>,
    min: Vec2,
    max: Vec2,
}
impl Surface {
    pub fn new(mesh: Mesh) -> Self {
        let points = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
        let triangles: Vec<_> = indices
            .chunks_exact(3)
            .map(|f| {
                f.try_into()
                    .map(|a: [usize; 3]| a.map(|i| Vec3::from_array(points[i])))
                    .unwrap()
            })
            .collect();
        let mut cells: HashMap<_, Vec<usize>> = HashMap::new();
        let mut min = Vec2::splat(f32::INFINITY);
        let mut max = Vec2::splat(f32::NEG_INFINITY);
        for (i, tri) in triangles.iter().enumerate() {
            let lo = tri.iter().map(|p| xz(*p)).reduce(Vec2::min).unwrap();
            let hi = tri.iter().map(|p| xz(*p)).reduce(Vec2::max).unwrap();
            min = min.min(lo);
            max = max.max(hi);
            for x in (lo.x / 10.0).floor() as i32..=(hi.x / 10.0).floor() as i32 {
                for y in (lo.y / 10.0).floor() as i32..=(hi.y / 10.0).floor() as i32 {
                    cells.entry((x, y)).or_default().push(i);
                }
            }
        }
        Self {
            mesh,
            triangles,
            cells,
            min,
            max,
        }
    }
    /// The rectangle the terrain fills, in plan.
    pub fn bounds(&self) -> (Vec2, Vec2) {
        (self.min, self.max)
    }
    pub fn contains(&self, p: Vec2) -> bool {
        p.cmpge(self.min + Vec2::splat(0.5)).all() && p.cmple(self.max - Vec2::splat(0.5)).all()
    }
    pub fn sample(&self, p: Vec2) -> Option<(f32, Vec2)> {
        let cell = ((p.x / 10.0).floor() as i32, (p.y / 10.0).floor() as i32);
        for &i in self.cells.get(&cell)? {
            let [a, b, c] = self.triangles[i];
            // Subtract and solve in f64. Boundary triangles can be long and
            // very thin; f32 cross products cancel and misplace their height.
            let ab = xz(b).as_dvec2() - xz(a).as_dvec2();
            let ac = xz(c).as_dvec2() - xz(a).as_dvec2();
            let ap = p.as_dvec2() - xz(a).as_dvec2();
            let det = ab.perp_dot(ac);
            if det.abs() < 1e-12 {
                continue;
            }
            let u = ap.perp_dot(ac) / det;
            let v = ab.perp_dot(ap) / det;
            if u >= -1e-4 && v >= -1e-4 && u + v <= 1.0001 {
                let dy_b = f64::from(b.y) - f64::from(a.y);
                let dy_c = f64::from(c.y) - f64::from(a.y);
                let gradient = Vec2::new(
                    ((dy_b * ac.y - dy_c * ab.y) / det) as f32,
                    ((ab.x * dy_c - ac.x * dy_b) / det) as f32,
                );
                return Some(((f64::from(a.y) + u * dy_b + v * dy_c) as f32, gradient));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::super::profile::TARMAC_HALF;
    use super::*;
    use crate::track::{Track, all_circuits};

    #[test]
    fn driving_height_matches_rendered_grass_triangles() {
        for id in ["red-bull-ring", "spa-francorchamps", "suzuka"] {
            let track = Track::new(all_circuits().iter().find(|c| c.id == id).unwrap());
            let surface = track.terrain();
            for &[a, b, c] in surface.triangles.iter().step_by(17) {
                // Tessellation retains nearly collinear boundary slivers;
                // their rounded centroid need not lie inside the tiny face.
                if xz(b - a).perp_dot(xz(c - a)).abs() < 1e-4 {
                    continue;
                }
                let centre = (a + b + c) / 3.0;
                let Some((y, gradient)) = surface.sample(xz(centre)) else {
                    panic!(
                        "{id}: missing ground {a:?} {b:?} {c:?}, area {}",
                        xz(b - a).perp_dot(xz(c - a))
                    );
                };
                assert!(
                    (y - centre.y).abs() < 0.001,
                    "{id}: grass collision differs from mesh: expected {}, got {y}, area {}, triangle {a:?} {b:?} {c:?}",
                    centre.y,
                    xz(b - a).perp_dot(xz(c - a))
                );
                assert!(gradient.is_finite());
            }
        }
    }

    #[test]
    fn suzuka_has_continuous_low_ground_beneath_its_bridge() {
        let track = Track::new(all_circuits().iter().find(|c| c.id == "suzuka").unwrap());
        let boundaries = ground_lips(&track.profile, &track.ribbon);
        let branches = track.ribbon.nearby(
            track.ribbon.crossings()[0],
            super::super::ribbon::AT_CROSSING,
        );
        let lower = branches
            .iter()
            .min_by(|a, b| a.point.y.total_cmp(&b.point.y))
            .unwrap();
        let mesh = fill(&track.profile, &track.ribbon);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
        for side in [-1.0, 1.0] {
            for extra in [0.25, 0.75, 1.5] {
                let sample = lower.point
                    + lower.right * side * (track.profile.reach(lower.at, lower.t, side) + extra);
                assert!(
                    (height(xz(sample), &boundaries) - lower.point.y).abs() < 0.3,
                    "earth climbs into the passage at {sample}"
                );
                let drawn = indices
                    .chunks_exact(3)
                    .filter_map(|f| {
                        let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[f[i]]));
                        let area = xz(b - a).perp_dot(xz(c - a));
                        if area.abs() < 1e-6 {
                            return None;
                        }
                        let u = xz(b - sample).perp_dot(xz(c - sample)) / area;
                        let v = xz(c - sample).perp_dot(xz(a - sample)) / area;
                        (u >= -1e-4 && v >= -1e-4 && u + v <= 1.0001)
                            .then_some(a.y * u + b.y * v + c.y * (1.0 - u - v))
                    })
                    .collect::<Vec<_>>();
                assert!(!drawn.is_empty(), "hole beside the lower road at {sample}");
                assert!(
                    drawn.iter().all(|y| (y - sample.y).abs() < 0.35),
                    "raised ground beside the lower road at {sample}: {drawn:?}"
                );
            }
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
            let mut boundaries = lips_from_profile(&track.profile, &track.ribbon).to_vec();
            let ground_edges = ground_lips(&track.profile, &track.ribbon);
            for span in bridge_spans(&ground_edges, &track.profile, &track.ribbon) {
                for &at in [span.first().unwrap(), span.last().unwrap()] {
                    boundaries.push(vec![ground_edges[0][at], ground_edges[1][at]]);
                }
            }
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
                    // Inspect the road geometry, not driveable ground: the
                    // latter now correctly returns grass beneath the bridge.
                    let road = track.fix(p, None);
                    let road_height =
                        road.point.y + track.profile.height(road.at, road.t, road.lateral);
                    assert!(
                        road.lateral.abs() + 0.05 >= TARMAC_HALF || p.y < road_height - 0.02,
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
}
