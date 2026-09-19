//! Grass filling the hole the loft leaves inside a circuit.
//!
//! The road is a ribbon: tarmac, paint, kerbs and verge, with sky showing
//! through the middle. This takes the inner lip of that verge, triangulates the
//! polygon it encloses, and paints it the same grass, a shade darker so the
//! shoulder still reads. Heights of interior vertices follow the nearest lip,
//! so a hill on the circuit is a hill in the infield rather than a plane
//! cutting through it.
//!
//! A circuit that crosses itself in plan — Suzuka, and the synthetic figure of
//! eight — is not a simple hole. Ear clipping refuses those, and a winding-number
//! grid fills each loop instead.

use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};
use std::collections::HashMap;

use super::profile::{self, HALF_WIDTH, Profile, RIBS};
use super::ribbon::Ribbon;

/// Longest triangle edge kept after the polygon is ear-clipped. Longer spans
/// are split so interior heights can follow the lip instead of a single plane
/// across the hole.
const MAX_EDGE: f32 = 6.0;
/// How far apart lip samples are allowed to be. Stations are 0.4 m; taking
/// every other still follows the verge and keeps ear clipping cheap.
const SAMPLE: f32 = 0.8;
/// Drop the fill under the lip so the two surfaces do not z-fight.
const SINK: f32 = 0.03;
/// Grid step used when the inner lip is not a simple polygon.
const CELL: f32 = 3.0;

/// Grass of the infield, a little darker than the verge so the shoulder remains
/// a shoulder.
fn paint() -> [f32; 4] {
    profile::paint(0.28, 0.46, 0.20)
}

pub(super) fn fill(profile: &Profile, ribbon: &Ribbon) -> Mesh {
    let Some(ring) = inner_lip(profile, ribbon) else {
        return empty();
    };
    let mut positions = if is_simple(&ring) {
        tessellate(&ring).unwrap_or_else(|| rasterize(&ring))
    } else {
        rasterize(&ring)
    };
    positions = drop_asphalt(positions, ribbon);
    if positions.is_empty() {
        return empty();
    }
    follow_the_lip(&mut positions, &ring);
    for p in &mut positions {
        p.y -= SINK;
    }
    for face in positions.as_chunks_mut::<3>().0 {
        if (face[1] - face[0]).cross(face[2] - face[0]).y < 0.0 {
            face.swap(1, 2);
        }
    }
    let color = paint();
    let n = positions.len();
    // `tessellate` / `rasterize` store triangles as consecutive triples.
    let indices: Vec<u32> = (0..n as u32).collect();
    let normals = vec![[0.0, 1.0, 0.0]; n];
    let colors = vec![color; n];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        positions
            .into_iter()
            .map(|p| p.to_array())
            .collect::<Vec<_>>(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}

fn empty() -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
}

/// Inner lip of the verge, wound so the hole is on its left.
fn inner_lip(profile: &Profile, ribbon: &Ribbon) -> Option<Vec<Vec3>> {
    let stations = ribbon.stations();
    let n = stations.len();
    if n < 3 {
        return None;
    }
    // The hole is the side of the road that faces the circuit's centroid.
    // Signed area of the centreline is the wrong question here: a clockwise
    // lap can still come out positive in XZ depending on how the trace was
    // projected, and using the outer lip fills the road as well as the grass.
    let centroid = stations.iter().fold(Vec2::ZERO, |s, st| s + xz(st.pos)) / n as f32;
    let right_inside = stations
        .iter()
        .filter(|st| (centroid - xz(st.pos)).dot(xz(st.right)) > 0.0)
        .count()
        * 2
        > n;
    let mut ring = Vec::with_capacity(n);
    for (i, station) in stations.iter().enumerate() {
        let ribs = profile.at(i);
        let rib = if right_inside {
            ribs[RIBS - 1]
        } else {
            ribs[0]
        };
        ring.push(station.pos + station.right * rib.0 + Vec3::Y * rib.1);
    }
    if right_inside {
        ring.reverse();
    }
    Some(resample(&ring, SAMPLE))
}

fn resample(ring: &[Vec3], spacing: f32) -> Vec<Vec3> {
    let mut out = vec![ring[0]];
    let min_sq = spacing * spacing;
    for &p in &ring[1..] {
        if out.last().unwrap().distance_squared(p) >= min_sq {
            out.push(p);
        }
    }
    if out.len() >= 3 && out[0].distance_squared(*out.last().unwrap()) < min_sq * 0.25 {
        out.pop();
    }
    out
}

fn xz(p: Vec3) -> Vec2 {
    Vec2::new(p.x, p.z)
}

fn cross(a: Vec2, b: Vec2) -> f32 {
    a.perp_dot(b)
}

fn is_ccw(a: Vec3, b: Vec3, c: Vec3) -> bool {
    cross(xz(b) - xz(a), xz(c) - xz(a)) > 1e-8
}

fn point_in_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> bool {
    let p = xz(p);
    let (a, b, c) = (xz(a), xz(b), xz(c));
    let d1 = cross(b - a, p - a);
    let d2 = cross(c - b, p - b);
    let d3 = cross(a - c, p - c);
    (d1 >= 0.0 && d2 >= 0.0 && d3 >= 0.0) || (d1 <= 0.0 && d2 <= 0.0 && d3 <= 0.0)
}

fn segments_cross(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> bool {
    let (a, b, c, d) = (xz(a), xz(b), xz(c), xz(d));
    let ab = b - a;
    let cd = d - c;
    let d1 = cross(ab, c - a);
    let d2 = cross(ab, d - a);
    let d3 = cross(cd, a - c);
    let d4 = cross(cd, b - c);
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

fn is_simple(ring: &[Vec3]) -> bool {
    let n = ring.len();
    for i in 0..n {
        let j = (i + 1) % n;
        for k in (i + 2)..n {
            let l = (k + 1) % n;
            if i == l || j == k {
                continue;
            }
            if segments_cross(ring[i], ring[j], ring[k], ring[l]) {
                return false;
            }
        }
    }
    true
}

fn winding(p: Vec2, ring: &[Vec3]) -> i32 {
    let mut w = 0;
    let n = ring.len();
    for i in 0..n {
        let a = xz(ring[i]);
        let b = xz(ring[(i + 1) % n]);
        if a.y <= p.y {
            if b.y > p.y && cross(b - a, p - a) > 0.0 {
                w += 1;
            }
        } else if b.y <= p.y && cross(b - a, p - a) < 0.0 {
            w -= 1;
        }
    }
    w
}

fn is_ear(ring: &[Vec3], rest: &[usize], prev: usize, at: usize, next: usize) -> bool {
    if !is_ccw(ring[prev], ring[at], ring[next]) {
        return false;
    }
    rest.iter().all(|&j| {
        j == prev
            || j == at
            || j == next
            || !point_in_triangle(ring[j], ring[prev], ring[at], ring[next])
    })
}

/// Ear-clip, then split long edges so interior heights have somewhere to sit.
/// Returns a flat triangle list (three positions per face).
fn tessellate(ring: &[Vec3]) -> Option<Vec<Vec3>> {
    let n = ring.len();
    if n < 3 {
        return None;
    }
    let mut rest: Vec<usize> = (0..n).collect();
    let mut faces: Vec<[usize; 3]> = Vec::with_capacity(n - 2);
    let mut guard = 0;
    while rest.len() > 3 {
        guard += 1;
        if guard > n * n {
            return None;
        }
        let mut clipped = false;
        for i in 0..rest.len() {
            let prev = rest[(i + rest.len() - 1) % rest.len()];
            let at = rest[i];
            let next = rest[(i + 1) % rest.len()];
            if !is_ear(ring, &rest, prev, at, next) {
                continue;
            }
            faces.push([prev, at, next]);
            rest.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            return None;
        }
    }
    faces.push([rest[0], rest[1], rest[2]]);

    let mut positions = ring.to_vec();
    let mut faces: Vec<[u32; 3]> = faces
        .into_iter()
        .map(|[a, b, c]| [a as u32, b as u32, c as u32])
        .collect();
    let max_sq = MAX_EDGE * MAX_EDGE;
    for _ in 0..32 {
        let mut next = Vec::with_capacity(faces.len() * 2);
        let mut midpoints = HashMap::new();
        let mut split = false;
        for face in &faces {
            let [a, b, c] = *face;
            let pa = positions[a as usize];
            let pb = positions[b as usize];
            let pc = positions[c as usize];
            let ab = pa.distance_squared(pb);
            let bc = pb.distance_squared(pc);
            let ca = pc.distance_squared(pa);
            let longest = ab.max(bc).max(ca);
            if longest <= max_sq {
                next.push(*face);
                continue;
            }
            split = true;
            let (p, q, r) = if ab >= bc && ab >= ca {
                (a, b, c)
            } else if bc >= ca {
                (b, c, a)
            } else {
                (c, a, b)
            };
            let key = (p.min(q), p.max(q));
            let m = *midpoints.entry(key).or_insert_with(|| {
                positions.push(positions[p as usize].lerp(positions[q as usize], 0.5));
                (positions.len() - 1) as u32
            });
            next.push([p, m, r]);
            next.push([m, q, r]);
        }
        faces = next;
        if !split {
            break;
        }
    }

    let mut out = Vec::with_capacity(faces.len() * 3);
    for [a, b, c] in faces {
        let pa = positions[a as usize];
        let pb = positions[b as usize];
        let pc = positions[c as usize];
        if !is_ccw(pa, pb, pc) && (pb - pa).cross(pc - pa).length_squared() > 1e-12 {
            out.extend_from_slice(&[pa, pc, pb]);
        } else {
            out.extend_from_slice(&[pa, pb, pc]);
        }
    }
    Some(out)
}

/// Fallback for a lip that crosses itself: fill every grid cell whose centre
/// sits in a non-zero winding of the lip.
fn rasterize(ring: &[Vec3]) -> Vec<Vec3> {
    let mut min = Vec2::splat(f32::MAX);
    let mut max = Vec2::splat(f32::MIN);
    for &p in ring {
        let q = xz(p);
        min = min.min(q);
        max = max.max(q);
    }
    if !min.x.is_finite() || (max - min).max_element() < CELL {
        return Vec::new();
    }
    let nx = ((max.x - min.x) / CELL).ceil().max(1.0) as usize;
    let nz = ((max.y - min.y) / CELL).ceil().max(1.0) as usize;
    let mut inside = vec![false; (nx + 1) * (nz + 1)];
    let at = |x: usize, z: usize| z * (nx + 1) + x;
    for z in 0..=nz {
        for x in 0..=nx {
            let p = Vec2::new(min.x + x as f32 * CELL, min.y + z as f32 * CELL);
            inside[at(x, z)] = winding(p, ring) != 0;
        }
    }
    let mut out = Vec::new();
    for z in 0..nz {
        for x in 0..nx {
            let corners = [(x, z), (x + 1, z), (x + 1, z + 1), (x, z + 1)];
            if !corners.iter().all(|&(cx, cz)| inside[at(cx, cz)]) {
                continue;
            }
            let point = |cx: usize, cz: usize| {
                Vec3::new(min.x + cx as f32 * CELL, 0.0, min.y + cz as f32 * CELL)
            };
            let [a, b, c, d] = corners.map(|(cx, cz)| point(cx, cz));
            out.extend_from_slice(&[a, b, c, a, c, d]);
        }
    }
    out
}

/// Interior vertices take their height from the nearest point of the lip, so a
/// climb on the circuit is a climb in the grass rather than a chord through it.
/// A triangle whose centre still sits on the tarmac is not infield — Suzuka's
/// two roads cross, and a winding fill of that figure of eight would paint the
/// crossing.
fn drop_asphalt(positions: Vec<Vec3>, ribbon: &Ribbon) -> Vec<Vec3> {
    positions
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|face| {
            let mid = (face[0] + face[1] + face[2]) / 3.0;
            ribbon.locate(mid).lateral.abs() >= HALF_WIDTH
        })
        .copied()
        .flatten()
        .collect()
}

fn follow_the_lip(positions: &mut [Vec3], lip: &[Vec3]) {
    if lip.is_empty() {
        return;
    }
    let thinned = resample(lip, 2.0);
    let samples: &[Vec3] = if thinned.len() < 3 { lip } else { &thinned };
    for p in positions {
        let mut best = f32::MAX;
        let mut height = p.y;
        let n = samples.len();
        for i in 0..n {
            let a = samples[i];
            let b = samples[(i + 1) % n];
            let ab = xz(b - a);
            let t = if ab.length_squared() < 1e-8 {
                0.0
            } else {
                ((xz(*p) - xz(a)).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
            };
            let q = a.lerp(b, t);
            let d = xz(*p).distance_squared(xz(q));
            if d < best {
                best = d;
                height = q.y;
            }
        }
        p.y = height;
    }
}

#[cfg(test)]
mod tests {
    use super::super::profile::TARMAC_HALF;
    use super::*;
    use crate::track::{Track, circuits};

    fn every_track() -> impl Iterator<Item = (&'static str, Track)> {
        circuits::all()
            .iter()
            .map(|circuit| (circuit.name, Track::new(circuit)))
    }

    #[test]
    fn every_circuit_fills_its_hole() {
        for (name, track) in every_track() {
            let mesh = fill(&track.profile, &track.ribbon);
            let verts = mesh.count_vertices();
            assert!(
                verts >= 3,
                "{name}: the infield is empty ({verts} vertices)"
            );
            assert_eq!(verts % 3, 0, "{name}: the infield is not triangles");
        }
    }

    #[test]
    fn the_infield_faces_up() {
        for (name, track) in every_track() {
            let mesh = fill(&track.profile, &track.ribbon);
            let Some(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
                panic!("{name}: infield lost its positions");
            };
            let positions = positions.as_float3().expect("positions are float3");
            for face in positions.as_chunks::<3>().0 {
                let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(face[k]));
                let normal = (b - a).cross(c - a);
                assert!(
                    normal.y > 0.0 || normal.length_squared() < 1e-12,
                    "{name}: an infield face at {a} winds the wrong way"
                );
            }
        }
    }

    #[test]
    fn the_infield_does_not_cover_the_asphalt() {
        for (name, track) in every_track() {
            let mesh = fill(&track.profile, &track.ribbon);
            let Some(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
                panic!("{name}: infield lost its positions");
            };
            let positions = positions.as_float3().expect("positions are float3");
            for face in positions.as_chunks::<3>().0 {
                let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(face[k]));
                let mid = (a + b + c) / 3.0;
                let ground = track.ground_from(mid, None);
                assert!(
                    ground.lateral.abs() + 0.05 >= TARMAC_HALF,
                    "{name}: an infield triangle sits on the asphalt at {mid} \
                     ({:.2} m off the line)",
                    ground.lateral.abs()
                );
            }
        }
    }
}
