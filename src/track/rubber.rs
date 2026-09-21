//! Rubber is vertex colour in the textured asphalt itself: no overlay, shadows
//! or depth fighting, and the start paint and edge markings stay untouched.
use super::{profile::TARMAC_HALF, ribbon::Ribbon};
use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};

fn offsets(ribbon: &Ribbon) -> Vec<f32> {
    let stations = ribbon.stations();
    // Local curvature minus the approach/exit curvature puts the car inside at
    // the apex and outside on either side. Smooth cyclically through the seam.
    let mut line: Vec<f32> = stations
        .iter()
        .map(|s| {
            let shoulders =
                (ribbon.along(s.s, -12.0).curvature + ribbon.along(s.s, 12.0).curvature) * 0.5;
            ((s.curvature - shoulders) * 30.0).tanh() * (TARMAC_HALF - 0.48)
        })
        .collect();
    let n = line.len();
    for _ in 0..160 {
        line = (0..n)
            .map(|i| (line[(i + n - 1) % n] + 2.0 * line[i] + line[(i + 1) % n]) * 0.25)
            .collect();
    }
    line
}

pub(super) fn apply(mesh: &mut Mesh, ribbon: &Ribbon) {
    let line = offsets(ribbon);
    let positions = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .unwrap()
        .as_float3()
        .unwrap();
    let mut points = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();
    // Each asphalt quad retains its original boundary and gains cross-road
    // samples for a soft, irregular rubber band, fading before the edge lines.
    for (q, quad) in positions.chunks_exact(4).enumerate() {
        let corners: [Vec3; 4] = std::array::from_fn(|i| Vec3::from_array(quad[i]));
        let i = q + super::profile::STRIPE;
        let j = (i + 1) % line.len();
        let base = points.len() as u32;
        for (row, station) in [i, j].into_iter().enumerate() {
            let phase = ribbon.stations()[station].s / ribbon.length() * std::f32::consts::TAU;
            for col in 0..=24 {
                let t = col as f32 / 24.0;
                let lateral = (2.0 * t - 1.0) * TARMAC_HALF;
                points.push(corners[row * 2].lerp(corners[row * 2 + 1], t).to_array());
                let width = 0.30 + 0.035 * (phase * 37.0).sin();
                let band = (-0.5 * ((lateral - line[station]) / width).powi(2)).exp();
                let edge = ((TARMAC_HALF - lateral.abs()) / 0.18).clamp(0.0, 1.0);
                let shade =
                    1.0 - band * edge * (0.19 + 0.025 * (phase * 53.0 + lateral * 17.0).sin());
                colors.push([shade, shade, shade, 1.0]);
            }
        }
        for col in 0..24 {
            let a = base + col;
            indices.extend([a, a + 1, a + 25, a + 1, a + 26, a + 25]);
        }
    }
    let uv: Vec<_> = points.iter().map(|p| [p[0] / 0.8, p[2] / 0.8]).collect();
    let normals = vec![Vec3::Y.to_array(); points.len()];
    *mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, points)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{Track, all_circuits};
    #[test]
    fn racing_line_is_smooth_bounded_and_cyclic_on_every_layout() {
        for circuit in all_circuits() {
            let track = Track::new(circuit);
            let line = offsets(&track.ribbon);
            assert!(
                line.iter().any(|x| x.abs() > 0.3),
                "{}: no racing path",
                circuit.name
            );
            for i in 0..line.len() {
                assert!(line[i].abs() < TARMAC_HALF - 0.45);
                assert!(
                    (line[i] - line[(i + 1) % line.len()]).abs() < 0.12,
                    "{}: abrupt rubber path",
                    circuit.name
                );
            }
            let (_, mut road, _) = track.profile.surfaces(&track.ribbon);
            apply(&mut road, &track.ribbon);
            let bevy::mesh::VertexAttributeValues::Float32x4(colors) =
                road.attribute(Mesh::ATTRIBUTE_COLOR).unwrap()
            else {
                panic!()
            };
            for row in colors.chunks_exact(25) {
                assert_eq!(row[0], [1.0; 4]);
                assert_eq!(row[24], [1.0; 4]);
                assert!(row.iter().all(|c| c[0] >= 0.78 && c[0] <= 1.0));
            }
        }
    }
}
