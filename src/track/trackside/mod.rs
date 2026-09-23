//! Everything that stands beside the road: braking boards, the start/finish
//! posts, the pit building and grandstand, tyre walls, advertising boards,
//! marshal posts, trees and bushes.
//!
//! All of it is two unlit meshes built when the circuit is: shapes in vertex
//! colours, and the faces that carry fine detail (lettering, numbers, chequers,
//! belts, the crowd) textured from one [`atlas`]. Two draw calls and nothing
//! per frame. Solid shapes get their shading baked into the colours from the
//! scene's sun, so they read as solid without lighting. None of it is anything
//! the car touches or stands on.
//!
//! Nothing is laid on top of anything else: a detailed face replaces the
//! plain one it would have covered, so no two faces ever share a depth and
//! fight over it.
//!
//! Pieces are placed in order of importance through one [`Site`], which keeps
//! each clear of every road, off the bridges, on the terrain and out of the
//! way of everything placed before it.

pub(super) mod atlas;
mod barriers;
mod marshals;
mod stands;
mod trees;

use super::{
    Track, boards, markers,
    profile::{HALF_WIDTH, deep, paint},
    start,
};
use bevy::{
    asset::RenderAssetUsages, light::NotShadowCaster, prelude::*,
    render::render_resource::PrimitiveTopology,
};
use std::collections::HashMap;

#[derive(Component)]
pub(super) struct Trackside;

pub(super) fn rebuild(
    mut commands: Commands,
    track: Res<Track>,
    old: Query<Entity, With<Trackside>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut material: Local<Option<[Handle<StandardMaterial>; 2]>>,
) {
    for entity in &old {
        commands.entity(entity).despawn();
    }
    let [shapes, signs] = material
        .get_or_insert_with(|| {
            [
                materials.add(StandardMaterial {
                    unlit: true,
                    ..default()
                }),
                materials.add(StandardMaterial {
                    base_color_texture: Some(images.add(atlas::image())),
                    unlit: true,
                    ..default()
                }),
            ]
        })
        .clone();
    let [shape_mesh, sign_mesh] = meshes_for(&track);
    for (mesh, material) in [(shape_mesh, shapes), (sign_mesh, signs)] {
        commands.spawn((
            Trackside,
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material),
            NotShadowCaster,
        ));
    }
}

/// The whole of it for one circuit: shapes, and textured faces.
pub(super) fn meshes_for(track: &Track) -> [Mesh; 2] {
    let mut out = Builder::default();
    let mut site = Site::new(track);
    build(track, &mut site, &mut out);
    let mesh = || {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
    };
    let signs = out.signs;
    [
        mesh()
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, out.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, out.normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, out.colors),
        mesh()
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, signs.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, signs.normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, signs.uvs),
    ]
}

/// In order of importance: what is already placed wins.
fn build(track: &Track, site: &mut Site, out: &mut Builder) {
    furnish(track, site, out);
    for (_, decorate) in DECORATIONS {
        decorate(track, site, out);
    }
}

/// What the driver reads the road by, which everything else keeps clear of:
/// the chevron plaques (drawn on their own), braking boards and start posts.
fn furnish(track: &Track, site: &mut Site, out: &mut Builder) {
    for plaque in markers::plaque_centres(track) {
        site.claim(plaque, markers::HALF * 1.5);
    }
    for board in boards::placed(track) {
        for point in boards::footprint(track, &board) {
            site.claim(point, 0.15);
        }
    }
    for post in start::post_bases(track) {
        site.claim(post, 0.3);
    }
    boards::build(track, out);
    start::posts(track, out);
}

type Decorate = fn(&Track, &mut Site, &mut Builder);

/// The decorations, most important first.
const DECORATIONS: [(&str, Decorate); 6] = [
    ("stands", stands::build),
    ("tyre walls", barriers::tyre_walls),
    ("marshal posts", marshals::build),
    ("advertising", barriers::hoardings),
    ("trees", trees::build),
    // Flags fly from the stands and marshal posts; nothing of their own.
    ("", |_, _, _| {}),
];

/// Where things already stand, as circles in plan, and what the ground is.
pub(super) struct Site<'a> {
    track: &'a Track,
    cells: HashMap<(i32, i32), Vec<(Vec2, f32)>>,
    min: Vec2,
    max: Vec2,
}

/// Grid cell for the occupancy lookup; larger than anything claimed.
const CELL: f32 = 4.0;

impl<'a> Site<'a> {
    fn new(track: &'a Track) -> Self {
        let (min, max) = track.terrain().bounds();
        Self {
            track,
            cells: HashMap::new(),
            min,
            max,
        }
    }

    fn cell(p: Vec2) -> (i32, i32) {
        ((p.x / CELL).floor() as i32, (p.y / CELL).floor() as i32)
    }

    /// Whether a circle of `radius` at `point` is on the terrain, at least
    /// `gap` outside the kerb of every road, off any bridge, and clear of
    /// everything claimed so far.
    fn fits(&self, point: Vec3, radius: f32, gap: f32) -> bool {
        let p = point.xz();
        if !self.track.terrain().contains(p)
            || p.cmplt(self.min + radius).any()
            || p.cmpgt(self.max - radius).any()
        {
            return false;
        }
        let road = self.track.fix(point, None);
        if road.lateral.abs() < HALF_WIDTH + gap + radius
            || deep(&self.track.ribbon.stations()[road.at]) > 0.0
        {
            return false;
        }
        let (cx, cy) = Self::cell(p);
        (cx - 1..=cx + 1).all(|x| {
            (cy - 1..=cy + 1).all(|y| {
                self.cells
                    .get(&(x, y))
                    .is_none_or(|circles| circles.iter().all(|&(q, r)| q.distance(p) >= r + radius))
            })
        })
    }

    fn claim(&mut self, point: Vec3, radius: f32) {
        debug_assert!(radius <= CELL);
        let p = point.xz();
        self.cells
            .entry(Self::cell(p))
            .or_default()
            .push((p, radius));
    }

    /// [`Site::fits`] for every point, and if so claims them all.
    fn take(&mut self, points: &[Vec3], radius: f32, gap: f32) -> bool {
        let fits = points.iter().all(|&p| self.fits(p, radius, gap));
        if fits {
            for &p in points {
                self.claim(p, radius);
            }
        }
        fits
    }

    /// Ground height at a point away from the road.
    fn ground(&self, point: Vec3) -> f32 {
        self.track.ground_from(point, None).height
    }

    /// The lowest ground under any of `points`.
    fn lowest(&self, points: &[Vec3]) -> f32 {
        points
            .iter()
            .map(|&p| self.ground(p))
            .fold(f32::MAX, f32::min)
    }
}

/// A point `out` metres beyond the kerb on `side` of station `at`, at the
/// centreline's height. Wraps round the lap.
fn beside(track: &Track, at: i64, side: f32, out: f32) -> Vec3 {
    let stations = track.ribbon.stations();
    let station = &stations[at.rem_euclid(stations.len() as i64) as usize];
    station.pos + station.right * side * (HALF_WIDTH + out)
}

/// A stable pseudo-random number in [0, 1) for a circuit and two integers.
fn random(seed: u64, a: i64, b: i64) -> f32 {
    let mut x = seed
        ^ (a as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (b as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

/// The same numbers every time a circuit is built, different per circuit.
fn seed(track: &Track) -> u64 {
    track
        .circuit()
        .id
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325, |h, b| {
            (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

/// How much of the light a face turned towards `normal` gets: a floor of sky
/// light, and the rest from the sun.
fn light(normal: Vec3) -> f32 {
    let sun = crate::world::sun() * Vec3::Z;
    0.62 + 0.38 * normal.normalize_or_zero().dot(sun).max(0.0)
}

#[derive(Default)]
pub(in crate::track) struct Builder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    signs: Signs,
}

/// The faces textured from the [`atlas`].
#[derive(Default)]
struct Signs {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
}

impl Builder {
    /// A quad showing an atlas cell, corners given bottom-left, bottom-right,
    /// top-right, top-left as seen from `outward`. It is the face, not a
    /// sticker on one: leave out whatever face it stands in for.
    pub(in crate::track) fn sign(&mut self, corners: [Vec3; 4], outward: Vec3, cell: atlas::Cell) {
        let (lo, hi) = cell.uv();
        let uvs = [
            Vec2::new(lo.x, hi.y),
            Vec2::new(hi.x, hi.y),
            Vec2::new(hi.x, lo.y),
            Vec2::new(lo.x, lo.y),
        ];
        let normal = outward.normalize().to_array();
        for [i, j, k] in [[0, 1, 2], [0, 2, 3]] {
            let (mut b, mut c) = (j, k);
            if (corners[b] - corners[i])
                .cross(corners[c] - corners[i])
                .dot(outward)
                < 0.0
            {
                std::mem::swap(&mut b, &mut c);
            }
            for v in [i, b, c] {
                self.signs.positions.push(corners[v].to_array());
                self.signs.normals.push(normal);
                self.signs.uvs.push(uvs[v].to_array());
            }
        }
    }

    /// A flat quad, wound to face `outward`, in exactly `color`.
    pub(in crate::track) fn quad(
        &mut self,
        corners: [Vec3; 4],
        outward: Vec3,
        color: (f32, f32, f32),
    ) {
        let [a, b, c, d] = corners;
        self.triangle([a, b, c], outward, color);
        self.triangle([a, c, d], outward, color);
    }

    /// The same, shaded by where it faces.
    fn lit(&mut self, corners: [Vec3; 4], outward: Vec3, color: (f32, f32, f32)) {
        let k = light(outward);
        self.quad(corners, outward, (color.0 * k, color.1 * k, color.2 * k));
    }

    /// A triangle wound to face `outward`.
    fn triangle(&mut self, mut corners: [Vec3; 3], outward: Vec3, color: (f32, f32, f32)) {
        let [a, b, c] = corners;
        if (b - a).cross(c - a).dot(outward) < 0.0 {
            corners.swap(1, 2);
        }
        let color = paint(color.0, color.1, color.2);
        let normal = outward.normalize().to_array();
        for corner in corners {
            self.positions.push(corner.to_array());
            self.normals.push(normal);
            self.colors.push(color);
        }
    }

    /// A triangle shaded by its own facing, which is taken to be away from
    /// `inside`.
    fn lit_triangle(&mut self, corners: [Vec3; 3], inside: Vec3, color: (f32, f32, f32)) {
        let [a, b, c] = corners;
        let mut normal = (b - a).cross(c - a).normalize_or_zero();
        if normal.dot((a + b + c) / 3.0 - inside) < 0.0 {
            normal = -normal;
        }
        let k = light(normal);
        self.triangle(corners, normal, (color.0 * k, color.1 * k, color.2 * k));
    }

    /// An upright box from `bottom` to `top`, `x` across and `z` deep, with
    /// no bottom face and, if `lid` is off, no top.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::track) fn column(
        &mut self,
        base: Vec3,
        bottom: f32,
        top: f32,
        x: Vec3,
        z: Vec3,
        faces: [(f32, f32, f32); 3],
        lid: bool,
    ) {
        let [front, back, sides] = faces;
        self.open_column(
            base,
            bottom,
            top,
            x,
            z,
            [Some(front), Some(back)],
            sides,
            lid,
        );
    }

    /// The same with the front (+`z`) or back face left out, where a sign
    /// takes its place.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::track) fn open_column(
        &mut self,
        base: Vec3,
        bottom: f32,
        top: f32,
        x: Vec3,
        z: Vec3,
        faces: [Option<(f32, f32, f32)>; 2],
        sides: (f32, f32, f32),
        lid: bool,
    ) {
        let at = |sx: f32, sz: f32, y: f32| {
            let mut p = base + x * sx + z * sz;
            p.y = y;
            p
        };
        for (sz, colour) in [(1.0, faces[0]), (-1.0, faces[1])] {
            let Some(colour) = colour else { continue };
            self.quad(
                [
                    at(-1.0, sz, bottom),
                    at(1.0, sz, bottom),
                    at(1.0, sz, top),
                    at(-1.0, sz, top),
                ],
                z * sz,
                colour,
            );
        }
        for sx in [-1.0, 1.0] {
            self.quad(
                [
                    at(sx, -1.0, bottom),
                    at(sx, 1.0, bottom),
                    at(sx, 1.0, top),
                    at(sx, -1.0, top),
                ],
                x * sx,
                sides,
            );
        }
        if lid {
            self.quad(
                [
                    at(-1.0, -1.0, top),
                    at(1.0, -1.0, top),
                    at(1.0, 1.0, top),
                    at(-1.0, 1.0, top),
                ],
                Vec3::Y,
                sides,
            );
        }
    }

    /// A box on four arbitrary plan corners (counter-clockwise or not) from
    /// `bottom` to `top`, every face in `color` shaded by where it faces.
    fn block(&mut self, corners: [Vec3; 4], bottom: f32, top: f32, color: (f32, f32, f32)) {
        self.open_block(corners, bottom, top, color, None);
    }

    /// The same without the side from `corners[open]` to the next, which
    /// something else will fill.
    fn open_block(
        &mut self,
        corners: [Vec3; 4],
        bottom: f32,
        top: f32,
        color: (f32, f32, f32),
        open: Option<usize>,
    ) {
        let at = |y: f32| corners.map(|p| Vec3::new(p.x, y, p.z));
        self.prism(at(bottom), at(top), color, open);
    }

    /// A box between four bottom corners and the four top corners above them,
    /// which need not be level: a module of a stand follows the ground. No
    /// bottom face, and without the side from corner `open` to the next.
    fn prism(
        &mut self,
        bottom: [Vec3; 4],
        top: [Vec3; 4],
        color: (f32, f32, f32),
        open: Option<usize>,
    ) {
        let centre = bottom.iter().sum::<Vec3>() / 4.0;
        for i in (0..4).filter(|&i| Some(i) != open) {
            let j = (i + 1) % 4;
            let mid = (bottom[i] + bottom[j]) / 2.0;
            let outward = (mid - centre).with_y(0.0);
            self.lit([bottom[i], bottom[j], top[j], top[i]], outward, color);
        }
        let up = (top[1] - top[0]).cross(top[3] - top[0]);
        self.lit(top, up * up.y.signum(), color);
    }

    /// Triangles so far.
    #[cfg(test)]
    fn triangles(&self) -> usize {
        self.positions.len() / 3
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::circuits;

    fn every_track() -> impl Iterator<Item = (&'static str, Track)> {
        circuits::all()
            .iter()
            .map(|circuit| (circuit.name, Track::new(circuit)))
    }

    /// Every face is wound to face the normal it carries, the whole of it
    /// stays well inside a triangle budget, and it comes out the same twice.
    #[test]
    fn the_trackside_mesh_is_sound_small_and_repeatable() {
        for (name, track) in every_track() {
            let mut out = Builder::default();
            build(&track, &mut Site::new(&track), &mut out);
            assert!(
                out.triangles() < TRIANGLE_BUDGET,
                "{name}: {} trackside triangles",
                out.triangles()
            );
            let positions: Vec<_> = out.positions.iter().chain(&out.signs.positions).collect();
            let normals: Vec<_> = out.normals.iter().chain(&out.signs.normals).collect();
            for (triangle, normal) in positions.chunks_exact(3).zip(normals.chunks_exact(3)) {
                let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(*triangle[i]));
                let n = (b - a).cross(c - a);
                assert!(
                    n.length() < 1e-9 || n.dot(Vec3::from(*normal[0])) > 0.0,
                    "{name}: a face is wound inside out"
                );
            }
            let mut again = Builder::default();
            build(&track, &mut Site::new(&track), &mut again);
            assert_eq!(out.positions, again.positions, "{name}: not repeatable");
        }
    }

    /// Each decoration on its own, in the order they are placed.
    fn decorations(track: &Track) -> (Vec<Vec3>, Vec<(&'static str, Builder)>) {
        let mut site = Site::new(track);
        let mut furniture = Builder::default();
        furnish(track, &mut site, &mut furniture);
        let mut kept: Vec<Vec3> = markers::plaque_centres(track);
        kept.extend(start::post_bases(track));
        for board in boards::placed(track) {
            kept.extend(boards::footprint(track, &board));
        }
        let kinds = DECORATIONS
            .iter()
            .filter(|(name, _)| !name.is_empty())
            .map(|&(name, decorate)| {
                let mut out = Builder::default();
                decorate(track, &mut site, &mut out);
                (name, out)
            })
            .collect();
        (kept, kinds)
    }

    /// No part of any decoration comes within 0.3 m of a kerb, a chevron
    /// plaque, a braking board or a start post, and every circuit has some of
    /// each: stands at the line, walls and marshals at its big corners,
    /// advertising down its straights and trees on its grass.
    #[test]
    fn decorations_keep_clear_of_the_road_and_what_the_driver_reads() {
        for (name, track) in every_track() {
            let (kept, kinds) = decorations(&track);
            for (kind, out) in &kinds {
                assert!(out.triangles() > 0, "{name} has no {kind}");
                for &p in &out.positions {
                    let p = Vec3::from(p);
                    let road = track.fix(p, None);
                    assert!(
                        road.lateral.abs() > HALF_WIDTH + 0.3,
                        "{name}: {kind} {:.2} m from a kerb",
                        road.lateral.abs() - HALF_WIDTH
                    );
                    for &q in &kept {
                        assert!(
                            p.xz().distance(q.xz()) > 0.3,
                            "{name}: {kind} on top of a plaque, board or post"
                        );
                    }
                }
            }
        }
    }

    /// The stands are at the start line on every circuit, and on nearly all
    /// of them the pits and the grandstand face each other across it. Where
    /// another road passes close to the line, one side may have no room.
    #[test]
    fn the_stands_face_each_other_across_the_line() {
        let mut one_sided = Vec::new();
        for (name, track) in every_track() {
            let (_, kinds) = decorations(&track);
            let start = track.ribbon.start();
            let stands = &kinds[0].1.positions;
            for &p in stands {
                let along = track.start_along(Vec3::from(p));
                assert!(
                    along.abs() < 20.0,
                    "{name}: stands {along:.0} m from the line"
                );
            }
            let side = |p: &[f32; 3]| (Vec3::from(*p) - start.pos).dot(start.right) > 0.0;
            if !(stands.iter().any(side) && stands.iter().any(|p| !side(p))) {
                one_sided.push(name);
            }
        }
        assert!(
            one_sided.len() <= 3,
            "stands on one side only: {one_sided:?}"
        );
    }

    /// Nothing is laid on anything: no two faces facing the same way lie in
    /// the same plane and overlap, in either mesh or between them. Such a pair
    /// is drawn at the same depth and the two fight over every pixel, which is
    /// the flicker this whole module is built to be free of. Faces that only
    /// touch along an edge, or lie back to back, are fine.
    #[test]
    fn no_two_faces_fight_over_the_same_depth() {
        for (name, track) in every_track() {
            let mut out = Builder::default();
            build(&track, &mut Site::new(&track), &mut out);
            let triangles: Vec<[Vec3; 3]> = out
                .positions
                .chunks_exact(3)
                .chain(out.signs.positions.chunks_exact(3))
                .map(|t| [0, 1, 2].map(|i| Vec3::from(t[i])))
                .collect();
            // Triangles by rounded normal: offset along it, normal, index.
            let mut planes: HashMap<IVec3, Vec<(f32, Vec3, usize)>> = HashMap::new();
            for (k, &[a, b, c]) in triangles.iter().enumerate() {
                let n = (b - a).cross(c - a);
                if n.length() < 1e-8 {
                    continue;
                }
                let n = n.normalize();
                let key = (n * 100.0).round().as_ivec3();
                planes.entry(key).or_default().push((n.dot(a), n, k));
            }
            for group in planes.values_mut() {
                group.sort_by(|x, y| x.0.total_cmp(&y.0));
                for (i, &(d, n, k)) in group.iter().enumerate() {
                    for &(e, m, j) in &group[i + 1..] {
                        if e - d > 0.0005 {
                            break;
                        }
                        if n.dot(m) > 0.99999 && overlap(triangles[k], triangles[j], n) {
                            panic!("{name}: two faces share a plane near {:?}", triangles[k][0]);
                        }
                    }
                }
            }
        }
    }

    /// The check above can see an overlap, and does not see a shared edge.
    #[test]
    fn overlap_tells_laid_on_from_side_by_side() {
        let a = [Vec3::ZERO, Vec3::X, Vec3::Z];
        let inside = [
            Vec3::splat(0.1).with_y(0.0),
            Vec3::new(0.5, 0.0, 0.1),
            Vec3::new(0.1, 0.0, 0.5),
        ];
        let beside = [Vec3::X, Vec3::Z, Vec3::new(1.0, 0.0, 1.0)];
        assert!(overlap(a, a, Vec3::Y));
        assert!(overlap(a, inside, Vec3::Y));
        assert!(!overlap(a, beside, Vec3::Y));
    }

    /// Whether two coplanar triangles overlap by more than a sliver, by
    /// separating axes in their plane.
    fn overlap(a: [Vec3; 3], b: [Vec3; 3], normal: Vec3) -> bool {
        let edges = |t: [Vec3; 3]| [t[1] - t[0], t[2] - t[1], t[0] - t[2]];
        edges(a).into_iter().chain(edges(b)).all(|edge| {
            let axis = normal.cross(edge).normalize_or_zero();
            let span = |t: [Vec3; 3]| {
                t.iter()
                    .map(|p| p.dot(axis))
                    .fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)))
            };
            let ((a0, a1), (b0, b1)) = (span(a), span(b));
            a1.min(b1) - a0.max(b0) > 1e-4
        })
    }

    /// A budget for the whole of one circuit's trackside mesh.
    const TRIANGLE_BUDGET: usize = 40_000;
}
