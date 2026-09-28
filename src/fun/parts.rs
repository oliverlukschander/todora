//! The shapes and paints everything silly is built from.
//!
//! Nothing here comes from a model file. A chicken, a cow, a cone, a hat and a
//! firework are all a handful of unit primitives — a sphere of diameter one, a
//! cube of side one — scaled, tilted and painted, and the primitives are made
//! once. That keeps the whole cast in a few dozen meshes and a few dozen
//! materials however many of them are on the road, and it keeps the branch free
//! of binary assets to review.

use std::collections::HashMap;

use bevy::{
    asset::RenderAssetUsages,
    mesh::{
        Capsule3dMeshBuilder, ConeMeshBuilder, CylinderMeshBuilder, Indices, MeshBuilder,
        PrimitiveTopology, SphereKind, SphereMeshBuilder,
    },
    prelude::*,
};

/// The unit shapes. Each is one unit across (or high), centred on the origin,
/// so a part is sized by its scale alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Shape {
    /// Diameter 1.
    Sphere,
    /// Side 1.
    Cube,
    /// Diameter 1, height 1, along Y.
    Cylinder,
    /// Diameter 1 at the base, height 1, point up.
    Cone,
    /// Diameter 1, 1 long between the caps (2 in all), along Y.
    Capsule,
    /// A flat disc of diameter 1 in the XZ plane, facing up.
    Disc,
    /// A 1 by 1 square in the XY plane, facing +Z, drawn from both sides.
    Quad,
    /// A four-pointed sparkle in the XY plane, 1 across, drawn from both sides.
    Star,
}

/// The primitives and the paint pot.
#[derive(Resource)]
pub(crate) struct Kit {
    meshes: HashMap<Shape, Handle<Mesh>>,
    paints: HashMap<u32, Handle<StandardMaterial>>,
    glows: HashMap<u32, Handle<StandardMaterial>>,
}

impl Kit {
    pub(crate) fn new(meshes: &mut Assets<Mesh>) -> Self {
        let mut all = HashMap::new();
        let mut put = |shape, mesh: Mesh| {
            all.insert(shape, meshes.add(mesh));
        };
        put(
            Shape::Sphere,
            SphereMeshBuilder::new(0.5, SphereKind::Uv {
                sectors: 20,
                stacks: 14,
            })
            .build(),
        );
        put(Shape::Cube, Cuboid::new(1.0, 1.0, 1.0).into());
        put(Shape::Cylinder, CylinderMeshBuilder::new(0.5, 1.0, 20).build());
        put(Shape::Cone, ConeMeshBuilder::new(0.5, 1.0, 20).build());
        put(Shape::Capsule, Capsule3dMeshBuilder::new(0.5, 1.0, 14, 6).build());
        put(Shape::Disc, disc(24));
        put(Shape::Quad, Rectangle::new(1.0, 1.0).into());
        put(Shape::Star, star());
        Self {
            meshes: all,
            paints: HashMap::new(),
            glows: HashMap::new(),
        }
    }

    pub(crate) fn mesh(&self, shape: Shape) -> Handle<Mesh> {
        self.meshes[&shape].clone()
    }

    /// A matte paint, made once per colour.
    pub(crate) fn paint(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        colour: Color,
    ) -> Handle<StandardMaterial> {
        self.paints
            .entry(key(colour))
            .or_insert_with(|| {
                materials.add(StandardMaterial {
                    base_color: colour,
                    perceptual_roughness: 0.78,
                    reflectance: 0.25,
                    ..default()
                })
            })
            .clone()
    }

    /// A paint that shines by itself, drawn from both sides, for light beams,
    /// sparks and confetti. `power` above one is what bloom picks up.
    pub(crate) fn glow(
        &mut self,
        materials: &mut Assets<StandardMaterial>,
        colour: Color,
        power: f32,
    ) -> Handle<StandardMaterial> {
        let id = key(colour) ^ power.to_bits().rotate_left(7);
        self.glows
            .entry(id)
            .or_insert_with(|| {
                let linear = colour.to_linear();
                materials.add(StandardMaterial {
                    base_color: Color::BLACK,
                    emissive: LinearRgba::rgb(
                        linear.red * power,
                        linear.green * power,
                        linear.blue * power,
                    ),
                    unlit: false,
                    double_sided: true,
                    cull_mode: None,
                    fog_enabled: false,
                    ..default()
                })
            })
            .clone()
    }
}

/// A colour as one number, to the resolution a paint pot cares about.
fn key(colour: Color) -> u32 {
    let c = colour.to_srgba();
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    byte(c.red) << 24 | byte(c.green) << 16 | byte(c.blue) << 8 | byte(c.alpha)
}

/// Build the kit before anything asks for it.
pub(crate) fn setup(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    commands.insert_resource(Kit::new(&mut meshes));
}

/// One primitive, placed. Returns the commands so the caller can hang more on
/// it: a component to animate it, or parts of its own.
pub(crate) fn add<'a>(
    parent: &'a mut ChildSpawnerCommands,
    kit: &Kit,
    shape: Shape,
    paint: &Handle<StandardMaterial>,
    at: Vec3,
    size: Vec3,
) -> EntityCommands<'a> {
    parent.spawn((
        Mesh3d(kit.mesh(shape)),
        MeshMaterial3d(paint.clone()),
        Transform::from_translation(at).with_scale(size),
    ))
}

/// As [`add`], turned about. The rotation is applied about the part's own
/// centre, before it is moved to `at`.
pub(crate) fn add_turned<'a>(
    parent: &'a mut ChildSpawnerCommands,
    kit: &Kit,
    shape: Shape,
    paint: &Handle<StandardMaterial>,
    at: Vec3,
    size: Vec3,
    turn: Quat,
) -> EntityCommands<'a> {
    parent.spawn((
        Mesh3d(kit.mesh(shape)),
        MeshMaterial3d(paint.clone()),
        Transform::from_translation(at)
            .with_rotation(turn)
            .with_scale(size),
    ))
}

/// A joint: an empty node to hang parts on and turn.
pub(crate) fn joint<'a>(
    parent: &'a mut ChildSpawnerCommands,
    at: Vec3,
    turn: Quat,
) -> EntityCommands<'a> {
    parent.spawn((
        Transform::from_translation(at).with_rotation(turn),
        Visibility::Inherited,
    ))
}

/// A flat disc facing up.
fn disc(segments: u32) -> Mesh {
    let mut positions = vec![[0.0, 0.0, 0.0]];
    let mut normals = vec![[0.0, 1.0, 0.0]];
    let mut uvs = vec![[0.5, 0.5]];
    let mut indices = Vec::new();
    for i in 0..segments {
        let a = i as f32 / segments as f32 * std::f32::consts::TAU;
        positions.push([a.cos() * 0.5, 0.0, a.sin() * 0.5]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([0.5 + a.cos() * 0.5, 0.5 + a.sin() * 0.5]);
    }
    for i in 0..segments {
        indices.extend([0, 1 + (i + 1) % segments, 1 + i]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

/// A four-pointed sparkle: long points up, down, left and right, pinched in
/// between. Faces +Z, and is meant to be drawn from both sides.
fn star() -> Mesh {
    let tip = 0.5;
    let pinch = 0.11;
    let mut positions = vec![[0.0, 0.0, 0.0]];
    for i in 0..8 {
        let a = i as f32 / 8.0 * std::f32::consts::TAU;
        let r = if i % 2 == 0 { tip } else { pinch };
        positions.push([a.cos() * r, a.sin() * r, 0.0]);
    }
    let normals = vec![[0.0, 0.0, 1.0]; positions.len()];
    let uvs: Vec<[f32; 2]> = positions
        .iter()
        .map(|p| [p[0] + 0.5, 0.5 - p[1]])
        .collect();
    let mut indices = Vec::new();
    for i in 0..8u32 {
        indices.extend([0, 1 + i, 1 + (i + 1) % 8]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

/// The colours of a good time, for confetti, balloons and anything else that
/// wants to look like it is having one.
pub(crate) const PARTY: [Color; 8] = [
    Color::srgb(1.00, 0.24, 0.36),
    Color::srgb(1.00, 0.62, 0.10),
    Color::srgb(1.00, 0.91, 0.20),
    Color::srgb(0.30, 0.90, 0.40),
    Color::srgb(0.16, 0.84, 0.95),
    Color::srgb(0.30, 0.46, 1.00),
    Color::srgb(0.72, 0.36, 1.00),
    Color::srgb(1.00, 0.36, 0.80),
];

/// A colour from the rainbow: `t` in turns, so 0 and 1 are both red.
pub(crate) fn rainbow(t: f32, lightness: f32) -> Color {
    Color::hsl(t.rem_euclid(1.0) * 360.0, 1.0, lightness)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kit_has_every_shape_and_shares_its_paint() {
        let mut meshes = Assets::<Mesh>::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let mut kit = Kit::new(&mut meshes);
        for shape in [
            Shape::Sphere,
            Shape::Cube,
            Shape::Cylinder,
            Shape::Cone,
            Shape::Capsule,
            Shape::Disc,
            Shape::Quad,
            Shape::Star,
        ] {
            assert!(meshes.get(&kit.mesh(shape)).is_some(), "{shape:?}");
        }
        let red = Color::srgb(1.0, 0.0, 0.0);
        let a = kit.paint(&mut materials, red);
        let b = kit.paint(&mut materials, red);
        assert_eq!(a, b, "one colour, one material");
        assert_ne!(a, kit.paint(&mut materials, Color::WHITE));
        let dim = kit.glow(&mut materials, red, 2.0);
        assert_ne!(dim, kit.glow(&mut materials, red, 6.0));
        let lit = materials.get(&dim).unwrap();
        assert!(lit.emissive.red > 1.0, "glow is brighter than white");
    }

    #[test]
    fn the_rainbow_wraps_and_the_party_is_varied() {
        assert_eq!(rainbow(0.0, 0.5), rainbow(1.0, 0.5));
        assert_ne!(rainbow(0.0, 0.5), rainbow(0.33, 0.5));
        let mut seen = std::collections::HashSet::new();
        for c in PARTY {
            assert!(seen.insert(key(c)), "two party colours are the same");
        }
    }
}
