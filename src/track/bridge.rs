//! An obsidian deck with slim rails and fitted abutments.
use super::{
    profile::{self, Profile, RIBS},
    ribbon::Ribbon,
};
use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};

pub(super) const OBSIDIAN: [f32; 3] = [0.16, 0.175, 0.185];
const STEEL: [f32; 3] = [0.095, 0.115, 0.13];
const RAIL: [f32; 3] = [0.24, 0.27, 0.29];

pub(super) fn mesh(profile: &Profile, ribbon: &Ribbon, terrain: &Mesh) -> Option<Mesh> {
    if ribbon.crossings().is_empty() {
        return None;
    }
    let mut geometry = Geometry::default();
    let stations = ribbon.stations();
    let ground = super::terrain::ground_lips(profile, ribbon);
    for i in 0..stations.len() {
        let j = (i + 1) % stations.len();
        let [a, b] = [&stations[i], &stations[j]];
        let depth = [profile::deep(a), profile::deep(b)];
        if depth.contains(&0.0) {
            continue;
        }
        let ribs = [profile.at(i), profile.at(j)];
        // Every strip uses the station frames at both ends: adjacent pieces
        // share corners exactly, including on curved and tapering approaches.
        for across in [-1.6, 0.0, 1.6] {
            let centres = [a.pos + a.right * across, b.pos + b.right * across];
            let web = [0, 1].map(|k| centres[k] - Vec3::Y * depth[k] * 0.67);
            geometry.strip(
                web,
                [a.right, b.right],
                0.075,
                depth.map(|d| d * 0.64),
                STEEL,
            );
            let flange = [0, 1].map(|k| centres[k] - Vec3::Y * depth[k] * 0.95);
            geometry.strip(
                flange,
                [a.right, b.right],
                0.18,
                depth.map(|d| d * 0.10),
                RAIL,
            );
        }
        for side in [0, RIBS - 1] {
            let sign = ribs[0][side].0.signum();
            let centres = [
                a.pos + a.right * (ribs[0][side].0 + sign * 0.075),
                b.pos + b.right * (ribs[1][side].0 + sign * 0.075),
            ];
            geometry.strip(
                centres.map(|p| p + Vec3::Y * 0.065),
                [a.right, b.right],
                0.15,
                [0.13; 2],
                OBSIDIAN,
            );
            for y in [0.30, 0.56] {
                geometry.strip(
                    centres.map(|p| p + Vec3::Y * y),
                    [a.right, b.right],
                    0.045,
                    [0.055; 2],
                    RAIL,
                );
            }
            if i.is_multiple_of(6)
                || profile::deep(&stations[(i + stations.len() - 1) % stations.len()]) == 0.0
            {
                geometry.beam(
                    centres[0] - a.tangent * 0.025 + Vec3::Y * 0.33,
                    centres[0] + a.tangent * 0.025 + Vec3::Y * 0.33,
                    0.045,
                    0.49,
                    RAIL,
                );
            }
        }
    }
    // Two full-width concrete abutments carry the deck outside the lower
    // road's verge. Their bases follow the terrain instead of floating.
    for &crossing in ribbon.crossings() {
        let branches = ribbon.nearby(crossing, super::ribbon::AT_CROSSING);
        let lower = branches
            .iter()
            .min_by(|a, b| a.point.y.total_cmp(&b.point.y))?;
        let upper = branches
            .iter()
            .max_by(|a, b| a.point.y.total_cmp(&b.point.y))?;
        let angle = upper.tangent.dot(lower.right).abs();
        let half_span = (profile
            .reach(lower.at, lower.t, 1.0)
            .max(profile.reach(lower.at, lower.t, -1.0))
            + profile::EDGE * upper.right.dot(lower.right).abs()
            + 1.0)
            / angle.max(0.2);
        let abutments = [-1.0, 1.0].map(|direction| ribbon.along(upper.s, half_span * direction));
        let along = |s: f32| {
            (s - upper.s + ribbon.length() * 0.5).rem_euclid(ribbon.length())
                - ribbon.length() * 0.5
        };
        for station in abutments {
            let fix = ribbon.locate(station.pos);
            let ribs = profile.between(fix.at, fix.t);
            let left = station.pos + station.right * ribs[0].0;
            let right = station.pos + station.right * ribs[RIBS - 1].0;
            let pieces = ((ribs[RIBS - 1].0 - ribs[0].0) / 0.4).ceil() as usize;
            for piece in 0..pieces {
                let a = left.lerp(right, piece as f32 / pieces as f32);
                let b = left.lerp(right, (piece + 1) as f32 / pieces as f32);
                let offset = station.tangent * 0.275;
                let footprint = [a - offset, a + offset, b - offset, b + offset];
                let bottom = footprint.map(|p| {
                    Vec3::new(
                        p.x,
                        ground_height(terrain, p).expect("abutment stands on terrain") - 0.10,
                        p.z,
                    )
                });
                let top = footprint.map(|p| {
                    let fix = ribbon.locate(p);
                    let next = (fix.at + 1) % stations.len();
                    let depth = profile::deep(&stations[fix.at])
                        .lerp(profile::deep(&stations[next]), fix.t);
                    Vec3::new(p.x, fix.point.y - depth * profile::SLAB_FRACTION, p.z)
                });
                geometry.solid(
                    [
                        bottom[0], bottom[1], bottom[2], bottom[3], top[0], top[1], top[2], top[3],
                    ],
                    OBSIDIAN,
                );
            }
        }
        // Retaining cheeks seal the gap between the upper shoulders and
        // their banks. Keep the entire lower road and verge open at the portal.
        for i in 0..stations.len() {
            let j = (i + 1) % stations.len();
            let [a, b] = [&stations[i], &stations[j]];
            let depth = [profile::deep(a), profile::deep(b)];
            if depth.contains(&0.0)
                || !((along(a.s) <= along(abutments[0].s) && along(b.s) <= along(abutments[0].s))
                    || (along(a.s) >= along(abutments[1].s) && along(b.s) >= along(abutments[1].s)))
            {
                continue;
            }
            for side in [0, RIBS - 1] {
                let [pa, pb] = [
                    a.pos + a.right * profile.at(i)[side].0,
                    b.pos + b.right * profile.at(j)[side].0,
                ];
                if [pa, pb].iter().any(|p| {
                    let lateral = (*p - lower.point).dot(lower.right);
                    lateral.abs() < profile.reach(lower.at, lower.t, lateral) + 0.7
                }) {
                    continue;
                }
                let top = [
                    a.pos.y - depth[0] * profile::SLAB_FRACTION,
                    b.pos.y - depth[1] * profile::SLAB_FRACTION,
                ];
                geometry.wall(
                    pa,
                    pb,
                    [pa, pb].map(|p| {
                        ground_height(terrain, p).expect("retaining wall stands on terrain") - 0.10
                    }),
                    top,
                    0.10,
                    OBSIDIAN,
                );
            }
        }
    }
    // Close the transverse edges where the under-bridge ground rejoins
    // the ordinary road cutout; these were open slits at the ramp ends.
    for span in super::terrain::bridge_spans(&ground, profile, ribbon) {
        for &at in [span.first().unwrap(), span.last().unwrap()] {
            let st = &stations[at];
            let top = st.pos.y - profile::deep(st) * profile::SLAB_FRACTION;
            for pair in profile.at(at).windows(2) {
                let a = st.pos + st.right * pair[0].0;
                let b = st.pos + st.right * pair[1].0;
                let bottom = [a, b].map(|p| {
                    ground_height(terrain, p)
                        .expect("approach ends on terrain")
                        .min(top - 0.01)
                        - 0.10
                });
                geometry.wall(a, b, bottom, [top; 2], 0.10, OBSIDIAN);
            }
        }
    }
    Some(geometry.finish())
}

// Fit foundations to the triangles that are actually rendered. Interpolating
// the underlying height field again can leave a foot hovering above its mesh.
fn ground_height(mesh: &Mesh, p: Vec3) -> Option<f32> {
    let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION)?.as_float3()?;
    let mut indices = mesh.indices()?.iter();
    while let Some(a) = indices.next() {
        let [a, b, c] = [a, indices.next()?, indices.next()?].map(|i| Vec3::from(positions[i]));
        let xz = |v: Vec3| Vec2::new(v.x, v.z);
        let area = xz(b - a).perp_dot(xz(c - a));
        if area.abs() < 1e-8 {
            continue;
        }
        let u = xz(b - p).perp_dot(xz(c - p)) / area;
        let v = xz(c - p).perp_dot(xz(a - p)) / area;
        if u >= -1e-4 && v >= -1e-4 && u + v <= 1.0001 {
            return Some(a.y * u + b.y * v + c.y * (1.0 - u - v));
        }
    }
    None
}

#[derive(Default)]
struct Geometry {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

impl Geometry {
    fn beam(&mut self, from: Vec3, to: Vec3, width: f32, height: f32, color: [f32; 3]) {
        let right = Vec3::Y.cross(to - from).normalize_or(Vec3::X);
        self.strip([from, to], [right; 2], width, [height; 2], color);
    }

    fn strip(
        &mut self,
        points: [Vec3; 2],
        rights: [Vec3; 2],
        width: f32,
        heights: [f32; 2],
        color: [f32; 3],
    ) {
        let [a, b] = points;
        let [ra, rb] = rights.map(|r| r * width * 0.5);
        let [ya, yb] = heights.map(|h| Vec3::Y * h * 0.5);
        self.solid(
            [
                a - ra - ya,
                a + ra - ya,
                b - rb - yb,
                b + rb - yb,
                a - ra + ya,
                a + ra + ya,
                b - rb + yb,
                b + rb + yb,
            ],
            color,
        );
    }

    fn wall(
        &mut self,
        a: Vec3,
        b: Vec3,
        bottom: [f32; 2],
        top: [f32; 2],
        thickness: f32,
        color: [f32; 3],
    ) {
        let offset = Vec3::Y.cross(b - a).normalize_or(Vec3::X) * thickness * 0.5;
        let bottom = [
            Vec3::new(a.x, bottom[0], a.z),
            Vec3::new(b.x, bottom[1], b.z),
        ];
        let top = [Vec3::new(a.x, top[0], a.z), Vec3::new(b.x, top[1], b.z)];
        self.solid(
            [
                bottom[0] - offset,
                bottom[0] + offset,
                bottom[1] - offset,
                bottom[1] + offset,
                top[0] - offset,
                top[0] + offset,
                top[1] - offset,
                top[1] + offset,
            ],
            color,
        );
    }

    fn solid(&mut self, corners: [Vec3; 8], color: [f32; 3]) {
        let centre = corners.into_iter().sum::<Vec3>() / 8.0;
        for face in [
            [0, 1, 3, 2],
            [4, 6, 7, 5],
            [0, 4, 5, 1],
            [2, 3, 7, 6],
            [0, 2, 6, 4],
            [1, 5, 7, 3],
        ] {
            let mut points = face.map(|i| corners[i]);
            let mut normal = (points[1] - points[0])
                .cross(points[2] - points[0])
                .normalize();
            if normal.dot(points[0] - centre) < 0.0 {
                points.reverse();
                normal = -normal;
            }
            // Face tones describe the material without dynamic shadows.
            let tone = if normal.y > 0.5 {
                1.04
            } else if normal.y < -0.5 {
                0.91
            } else {
                1.0
            };
            let paint = profile::paint(color[0] * tone, color[1] * tone, color[2] * tone);
            let first = self.positions.len() as u32;
            self.positions.extend(points.map(|p| p.to_array()));
            self.normals.extend([normal.to_array(); 4]);
            self.colors.extend([paint; 4]);
            self.indices
                .extend([first, first + 1, first + 2, first, first + 2, first + 3]);
        }
    }

    fn finish(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{Track, all_circuits};

    #[test]
    fn obsidian_structure_is_finite_and_keeps_both_roads_clear() {
        let track = Track::new(all_circuits().iter().find(|c| c.id == "suzuka").unwrap());
        let terrain = super::super::terrain::fill(&track.profile, &track.ribbon);
        let mesh = mesh(&track.profile, &track.ribbon, &terrain).unwrap();
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        let normals = mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .unwrap()
            .as_float3()
            .unwrap();
        for p in positions {
            assert!(Vec3::from(*p).is_finite());
        }
        for n in normals {
            assert!((Vec3::from(*n).length() - 1.0).abs() < 1e-4);
        }
        let mut feet = 0;
        for (p, normal) in positions.iter().zip(normals) {
            let p = Vec3::from(*p);
            if normal[1] < -0.5 && track.ribbon.locate(p).point.y - p.y > 1.0 {
                let y = ground_height(&terrain, p).expect("foundation has terrain beneath it");
                assert!(
                    p.y <= y + 0.015,
                    "foundation floats above the rendered ground at {p}"
                );
                feet += 1;
            }
        }
        assert!(feet > 20, "the bridge needs grounded abutments");
        let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
        for f in indices.chunks_exact(3) {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(positions[f[i]]));
            assert!(
                (b - a).cross(c - a).length() > 1e-8,
                "degenerate detail at {a}"
            );
            for p in [
                (a + b + c) / 3.0,
                a * 0.8 + b * 0.1 + c * 0.1,
                a * 0.1 + b * 0.8 + c * 0.1,
                a * 0.1 + b * 0.1 + c * 0.8,
            ] {
                for fix in track.ribbon.nearby(p, super::super::ribbon::AT_CROSSING) {
                    let edge = track.profile.reach(fix.at, fix.t, fix.lateral);
                    let y = p.y - fix.point.y;
                    assert!(
                        fix.lateral.abs() >= edge - 0.06 || !(-0.01..1.6).contains(&y),
                        "structure enters a driving corridor at {p}: {} / {edge}, height {y}",
                        fix.lateral
                    );
                }
            }
        }
    }
}
