//! Tyre marks: the line a rear wheel leaves when it stops rolling and starts
//! sliding.
//!
//! One mesh holds every mark. Each sliding wheel extends a stroke by a quad at a
//! time, and a stroke ends the moment the tyre hooks up again, so marks start and
//! stop where the slide did instead of joining across the straight in between.

use std::collections::VecDeque;

use bevy::{
    asset::RenderAssetUsages,
    mesh::Indices,
    prelude::*,
    render::render_resource::PrimitiveTopology,
};

use crate::car::{level, Car, DriveSet, HALF_TRACK, REAR_AXLE, WHEEL_WIDTH};
use crate::track::Track;
use crate::Reset;

/// How far past its grip the rear axle has to be before it scrubs rubber off.
const LETS_GO: f32 = 0.3;
/// Metres of travel per mark segment.
const SEGMENT: f32 = 0.18;
/// Marks kept, oldest dropped. About a lap's worth of sliding.
const KEPT: usize = 2_600;
/// Clear of the road surface. The loft is smooth, so this is plenty.
const LIFT: f32 = 0.015;
/// Fraction of the trail that fades out, so the oldest mark does not vanish.
const FADE: f32 = 0.15;

pub struct SkidPlugin;

impl Plugin for SkidPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, (wipe, lay, redraw).chain().after(DriveSet));
    }
}

#[derive(Resource)]
struct Marks {
    /// Made on the first mark, not before: a mesh with no vertices in it upsets
    /// the renderer's allocator, and an empty trail is the usual case.
    mesh: Option<Handle<Mesh>>,
    /// What is drawing that mesh, so a reset can take it away again.
    entity: Option<Entity>,
    material: Handle<StandardMaterial>,
    /// Each mark is the quad between where a tyre was and where it is now.
    quads: VecDeque<[Vec3; 4]>,
    /// The leading edge of the stroke each rear tyre is drawing, if it is
    /// sliding. `None` means the tyre is gripping and the stroke has ended.
    drawing: [Option<[Vec3; 2]>; 2],
    dirty: bool,
}

fn setup(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(Marks {
        mesh: None,
        entity: None,
        material: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            // Rubber on tarmac is a stain, not a surface: no shading, and a depth
            // bias so it wins against the road it is lying on.
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            depth_bias: 16.0,
            ..default()
        }),
        quads: VecDeque::new(),
        drawing: [None, None],
        dirty: false,
    });
}

/// Wipe the road clean. The entity goes with the marks rather than being left
/// holding an empty mesh, which is the one thing the renderer objects to.
fn wipe(mut commands: Commands, mut resets: MessageReader<Reset>, mut marks: ResMut<Marks>) {
    if resets.read().next().is_none() {
        return;
    }
    if let Some(entity) = marks.entity.take() {
        commands.entity(entity).despawn();
    }
    marks.mesh = None;
    marks.quads.clear();
    marks.drawing = [None, None];
    marks.dirty = false;
}

fn lay(track: Res<Track>, mut marks: ResMut<Marks>, cars: Query<(&Transform, &Car)>) {
    let Ok((transform, car)) = cars.single() else {
        return;
    };
    let sliding = car.rear_slip > LETS_GO && car.velocity.length() > 1.0;
    if !sliding {
        marks.drawing = [None, None];
        return;
    }
    let heading = level(*transform.forward());
    let right = heading.cross(Vec3::Y);
    let axle = transform.translation - heading * REAR_AXLE;

    for (i, side) in [-1.0f32, 1.0].into_iter().enumerate() {
        let hub = axle + right * (side * HALF_TRACK);
        let height = track.ground(hub).height + LIFT;
        let edge = [
            Vec3::new(hub.x, height, hub.z) - right * (WHEEL_WIDTH * 0.5),
            Vec3::new(hub.x, height, hub.z) + right * (WHEEL_WIDTH * 0.5),
        ];
        match marks.drawing[i] {
            // Wait until the tyre has travelled far enough to be worth a quad,
            // so a car sliding in place does not fill the buffer.
            Some(from) if midpoint(from).distance(midpoint(edge)) < SEGMENT => continue,
            Some(from) => {
                if marks.quads.len() == KEPT {
                    marks.quads.pop_front();
                }
                marks.quads.push_back([from[0], from[1], edge[1], edge[0]]);
                marks.dirty = true;
            }
            None => {}
        }
        marks.drawing[i] = Some(edge);
    }
}

fn redraw(mut commands: Commands, mut marks: ResMut<Marks>, mut meshes: ResMut<Assets<Mesh>>) {
    if !marks.dirty {
        return;
    }
    marks.dirty = false;
    let rebuilt = build(&marks.quads);
    match &marks.mesh {
        Some(handle) => {
            if let Some(mut mesh) = meshes.get_mut(handle) {
                *mesh = rebuilt;
            }
        }
        None => {
            let handle = meshes.add(rebuilt);
            let entity = commands
                .spawn((Mesh3d(handle.clone()), MeshMaterial3d(marks.material.clone())))
                .id();
            marks.mesh = Some(handle);
            marks.entity = Some(entity);
        }
    }
}

fn midpoint(edge: [Vec3; 2]) -> Vec3 {
    (edge[0] + edge[1]) * 0.5
}

fn build(quads: &VecDeque<[Vec3; 4]>) -> Mesh {
    let mut positions = Vec::with_capacity(quads.len() * 4);
    let mut colors = Vec::with_capacity(quads.len() * 4);
    let mut indices = Vec::with_capacity(quads.len() * 6);
    let oldest = (quads.len() as f32 * FADE).max(1.0);

    for (i, quad) in quads.iter().enumerate() {
        let alpha = 0.62 * (i as f32 / oldest).min(1.0);
        let base = positions.len() as u32;
        for corner in quad {
            positions.push(corner.to_array());
            colors.push([0.05, 0.05, 0.06, alpha]);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    fn quad(at: f32) -> [Vec3; 4] {
        [
            Vec3::new(0.0, 0.0, at),
            Vec3::new(0.1, 0.0, at),
            Vec3::new(0.1, 0.0, at + 0.1),
            Vec3::new(0.0, 0.0, at + 0.1),
        ]
    }

    /// Nothing should ever build an empty trail — the entity waits for the first
    /// mark — but it must not be a way to produce a broken mesh either.
    #[test]
    fn an_empty_trail_is_still_a_mesh() {
        let mesh = build(&VecDeque::new());
        assert_eq!(mesh.count_vertices(), 0);
    }

    #[test]
    fn every_mark_becomes_two_triangles() {
        let quads: VecDeque<_> = (0..5).map(|i| quad(i as f32)).collect();
        let mesh = build(&quads);
        assert_eq!(mesh.count_vertices(), 20);
        let Some(Indices::U32(indices)) = mesh.indices() else {
            panic!("marks lost their indices");
        };
        assert_eq!(indices.len(), 30);
        assert!(indices.iter().all(|&i| i < 20));
    }

    /// The mesh must arrive with the first mark and not before: a mesh with no
    /// vertices in it makes the renderer's allocator complain, and a car that
    /// has not slid yet is the usual case.
    #[test]
    fn the_mesh_arrives_with_the_first_mark() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()))
            .init_asset::<Mesh>()
            .insert_resource(Marks {
                mesh: None,
                entity: None,
                material: Handle::default(),
                quads: VecDeque::new(),
                drawing: [None, None],
                dirty: false,
            })
            .add_systems(Update, redraw);

        app.update();
        assert!(app.world().resource::<Marks>().mesh.is_none());
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 0);

        let mut marks = app.world_mut().resource_mut::<Marks>();
        marks.quads.push_back(quad(0.0));
        marks.dirty = true;
        app.update();

        let handle = app.world().resource::<Marks>().mesh.clone();
        let handle = handle.expect("the first mark did not bring a mesh");
        let meshes = app.world().resource::<Assets<Mesh>>();
        assert_eq!(meshes.get(&handle).map(|m| m.count_vertices()), Some(4));

        // A later mark extends the same mesh rather than spawning another.
        let mut marks = app.world_mut().resource_mut::<Marks>();
        marks.quads.push_back(quad(1.0));
        marks.dirty = true;
        app.update();
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), 1);
        let meshes = app.world().resource::<Assets<Mesh>>();
        assert_eq!(meshes.get(&handle).map(|m| m.count_vertices()), Some(8));
    }

    /// A reset wipes the road, and takes the entity with it rather than leaving
    /// it holding an empty mesh.
    #[test]
    fn a_reset_wipes_the_road() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()))
            .init_asset::<Mesh>()
            .add_message::<Reset>()
            .insert_resource(Marks {
                mesh: None,
                entity: None,
                material: Handle::default(),
                quads: VecDeque::from([quad(0.0), quad(1.0)]),
                drawing: [Some([Vec3::ZERO, Vec3::X]), None],
                dirty: true,
            })
            .add_systems(Update, (wipe, redraw).chain());

        app.update();
        let drawn = app.world().resource::<Marks>().entity;
        assert!(drawn.is_some(), "the marks never got drawn");

        app.world_mut().write_message(Reset);
        app.update();
        let marks = app.world().resource::<Marks>();
        assert!(marks.quads.is_empty(), "marks survived the reset");
        assert!(marks.mesh.is_none() && marks.entity.is_none());
        assert_eq!(marks.drawing, [None, None]);
        assert!(app.world().get_entity(drawn.unwrap()).is_err(), "entity survived");
    }

    /// The oldest marks have to be on their way out, or the trail ends in a
    /// hard edge where the buffer wraps.
    #[test]
    fn the_trail_fades_at_its_tail() {
        let quads: VecDeque<_> = (0..100).map(|i| quad(i as f32)).collect();
        let mesh = build(&quads);
        let Some(VertexAttributeValues::Float32x4(colors)) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("marks lost their vertex colours");
        };
        assert_eq!(colors[0][3], 0.0, "the oldest mark is not faded out");
        assert!(colors[colors.len() - 1][3] > 0.5, "the newest mark is faint");
    }
}
