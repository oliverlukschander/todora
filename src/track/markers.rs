//! Subtle corner diamonds on both verges. The layout follows the circuit's
//! corners and their approaches, with every other pair retained for display.
//! Only the nearest four pairs within 60 m along the road in the driving
//! direction are visible. Separate meshes let that limit follow the car
//! without rebuilding the track or confusing nearby stretches at a crossing.

use bevy::{asset::RenderAssetUsages, prelude::*, render::render_resource::PrimitiveTopology};

use super::Track;
use crate::car::{Car, Player};

use super::profile::{HALF_WIDTH, Profile, paint};
use super::ribbon::{STEP, Station};

/// Tighter than this and the car cannot carry its top speed through: a corner.
/// A radius rather than a curvature, because a corner is a shape, and 35 m is
/// the shape of one — at [`super::ribbon::MIN_RADIUS`] the game's tightest
/// corner is 5 m, the car the game ships on holds 22.2 m/s on the flat and
/// wants 35 m of radius to carry it, and above that it simply does not lift.
const CORNER_RADIUS: f32 = 35.0;
/// Bends with less than this much straight between them are one corner. Without
/// it a chicane is four corners, each laying its own approach over the last
/// one's exit, and the line through it comes out in pieces.
const MERGE: f32 = 6.0;
/// Stations the curvature is averaged over before it is read. The stations are
/// 0.4 m apart and their curvature comes off a polyline, so it is noisy at the
/// scale of one station; a corner is not, and 3.6 m is enough to tell them apart
/// without rounding off a real one.
const SMOOTH: usize = 9;
/// Base layout spacing: 4.4 m. Display keeps every other pair (8.8 m),
/// while the approach stays at its original 22 m from the corner.
const SPACING: usize = 11;
/// How far the line reaches back up the road from the corner, in diamonds. Five
/// of them is 22 m, which covers the whole of the hardest stop the car has in
/// it with a little over.
const APPROACH: usize = 5;
/// Half a diamond's diagonal. 0.4 m point to point, a third of the width of the
/// car: enough to carry down a straight, where at 0.3 m a diamond was there when
/// you arrived at it and not before, and still a twentieth of the road. The
/// footprint has not had to grow now that they stand up — see [`TALL`], which is
/// where the reach came from instead.
pub(super) const HALF: f32 = 0.20;
/// What a diamond keeps between itself and the kerb on one side, and the lip
/// the verge falls away over on the other. Small, because on a narrow shoulder
/// it is all there is: the diamond does not shrink to fit, so what gives is the
/// gap either side of it.
pub(super) const CLEAR: f32 = 0.05;
/// Grass a diamond needs under it: its own footprint, and a clearance either
/// side. [`super::profile`] takes this as one of the things that decides
/// whether a circuit has a verge at all — a shoulder too narrow to mark a
/// corner on is not a shoulder, and a corner does not go unmarked to let a
/// circuit in.
pub(super) const ROOM: f32 = 2.0 * (HALF + CLEAR);
/// How far a diamond rises above the verge it stands on.
///
/// Low. This is a mark on the verge, not a bollard beside it: about as tall as
/// one of the car's wheels and three times the kerb's own lip, which is enough
/// to give it a lit side, a shadow, and a height that does not foreshorten away,
/// and not enough for it to read as something that has to be missed. Nearly all
/// of the reach was bought by leaving the ground at all; height past this was
/// only making a spike.
///
/// It is also what decides how steep the four faces are, and they have to stay
/// off vertical: the verge falls away under the diamond, so the face on the low
/// side is the steepest, and even that one leans forty-one degrees off the
/// vertical. `no_face_of_a_diamond_stands_on_its_edge` is what holds that as the
/// shape changes.
const TALL: f32 = 0.15;
/// Faces a diamond has: four sides and no bottom.
const FACES: usize = 4;
/// Where on the shoulder the line of diamonds runs: half a metre out from the
/// kerb, where the shoulder has that much grass to give.
///
/// Close enough to the road to sit in the corner of the eye, far enough not to
/// be taken for part of it. It used to be a distance from the centreline, and a
/// constant, which it could be while every circuit carried the same verge for
/// the whole of its lap. Now the verge is fitted station by station, so this is
/// where a diamond goes when there is room and [`lateral`] is what it comes to
/// when there is not.
const OUT: f32 = 0.5;
/// How far a diamond floats above the verge. A hair, and only so the depth
/// buffer has something to separate the two by: a fifth of the kerb's lip.
const LIFT: f32 = 0.01;
/// Muted ochre, slate, brick, plum and bronze, shared by each left/right pair.
const PALETTE: [(f32, f32, f32); 5] = [
    (0.36, 0.31, 0.19),
    (0.21, 0.28, 0.34),
    (0.36, 0.23, 0.21),
    (0.29, 0.23, 0.32),
    (0.37, 0.28, 0.19),
];
const MAX_PAIRS: usize = 4;
const VISIBLE_AHEAD: f32 = 60.0;

/// One left/right pair, fixed at this distance around the lap.
#[derive(Component)]
pub(super) struct MarkerPair {
    s: f32,
}

pub(super) fn rebuild(
    mut commands: Commands,
    track: Res<Track>,
    old: Query<Entity, With<MarkerPair>>,
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
    let diamonds = diamonds(stations, &track.profile);
    for ((at, step), pair) in markers(stations).into_iter().zip(diamonds.chunks_exact(2)) {
        if step % 2 != 0 {
            continue;
        }
        commands.spawn((
            MarkerPair { s: stations[at].s },
            Mesh3d(meshes.add(mesh(pair))),
            MeshMaterial3d(material.clone()),
            Visibility::Hidden,
        ));
    }
}

pub(super) fn show(
    track: Res<Track>,
    cars: Query<(&Car, &Transform), With<Player>>,
    mut pairs: Query<(Entity, &MarkerPair, &mut Visibility)>,
) {
    let Ok((car, transform)) = cars.single() else {
        for (_, _, mut visibility) in &mut pairs {
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
    let mut ahead: Vec<_> = pairs
        .iter()
        .filter_map(|(entity, pair, _)| {
            let distance = ((pair.s - ground.s) * direction).rem_euclid(track.ribbon.length());
            (distance > 0.0 && distance <= VISIBLE_AHEAD).then_some((entity, distance))
        })
        .collect();
    ahead.sort_unstable_by(|a, b| a.1.total_cmp(&b.1));
    ahead.truncate(MAX_PAIRS);
    for (entity, _, mut visibility) in &mut pairs {
        *visibility = if ahead.iter().any(|&(next, _)| next == entity) {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

fn mesh(pair: &[Diamond]) -> Mesh {
    let mut positions = Vec::with_capacity(pair.len() * FACES * 3);
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut colors = Vec::with_capacity(positions.capacity());
    for diamond in pair {
        for face in diamond.faces() {
            let normal = (face[1] - face[0])
                .cross(face[2] - face[0])
                .normalize()
                .to_array();
            for corner in face {
                positions.push(corner.to_array());
                normals.push(normal);
                colors.push(diamond.color);
            }
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

/// How far off the centreline a diamond stands, on a shoulder whose grass is
/// `grass` metres wide.
///
/// Out at [`OUT`] from the kerb where the grass reaches that far, and drawn in
/// against the kerb where it does not — never off the lip at the far side, and
/// never overhanging the kerb at the near one. The diamond keeps its size while
/// it moves: what gives on a narrow shoulder is where the line runs, not what
/// the line is made of, because a mark that shrank would be a mark that read as
/// further away than it is.
///
/// The two const assertions this replaces could be checked at compile time
/// because both the verge and this were constants. Only one of them still can
/// be — that a shoulder wide enough to be a shoulder is wide enough to hold a
/// diamond — and it is in [`super::profile`] beside the floor it is about.
/// Where the diamonds actually landed on each circuit is
/// `a_diamond_stands_on_the_shoulder_it_was_given`.
fn lateral(grass: f32) -> f32 {
    let nearest = HALF + CLEAR;
    let furthest = (grass - HALF - CLEAR).max(nearest);
    HALF_WIDTH + OUT.clamp(nearest, furthest)
}

/// One diamond, ready to go into its pair's mesh.
pub(super) struct Diamond {
    /// The four corners it stands on, wound to face up: back, out, front, in.
    pub base: [Vec3; 4],
    /// The point, [`TALL`] above the middle of them.
    pub apex: Vec3,
    /// Its turn of [`PALETTE`], linear for the vertex colour attribute.
    pub color: [f32; 4],
}

impl Diamond {
    /// The four faces, each wound to face outward and up.
    ///
    /// The base is wound to face up, so walking it in order and closing each
    /// pair onto the point gives faces that wind outward without anything having
    /// to know which side of the road this diamond is on.
    pub fn faces(&self) -> [[Vec3; 3]; FACES] {
        std::array::from_fn(|i| [self.base[i], self.base[(i + 1) % FACES], self.apex])
    }
}

/// Every diamond on the circuit. Both verges, so a marked station makes two —
/// the same colour on each, because the line is one line seen from either side.
///
/// Each corner takes the verge's own height at its own distance off the
/// centreline, so a diamond stands on the fall of the verge instead of floating
/// flat over it. The point goes straight up from the middle of them, because a
/// marker leaning out with the camber is a marker that has been knocked.
pub(super) fn diamonds(stations: &[Station], profile: &Profile) -> Vec<Diamond> {
    let n = stations.len();
    let marked = markers(stations);
    let mut out = Vec::with_capacity(marked.len() * 2);
    for (at, step) in marked {
        let station = &stations[at];
        let (r, g, b) = PALETTE[step % PALETTE.len()];
        for side in [1.0, -1.0] {
            // The narrowest the grass gets anywhere under this diamond, not the
            // width at its middle. A diamond is 0.4 m long and the shoulder is
            // fitted station by station, so its nose and tail stand on
            // cross-sections that are not this one — and where the verge is
            // tapering, the tighter of them is what the clearance has to be
            // measured against. Placed off the middle alone, a diamond on
            // Madring's narrowest shoulder hangs a millimetre over the lip.
            let here = lateral(
                [-HALF, 0.0, HALF]
                    .into_iter()
                    .map(|along| {
                        let (on, t) = under(at, along, n);
                        profile.grass(on, t, side)
                    })
                    .fold(f32::MAX, f32::min),
            ) * side;
            // `across` is not flipped with the side, so both verges wind the
            // same way round and both faces point up.
            //
            // The fore and aft corners are half a station up and down the road,
            // where the shoulder is not necessarily the width it is here, so
            // each corner is stood on the cross-section at its own place rather
            // than on this station's four times over. On a tapering verge that
            // is the difference between a diamond lying on the ground and one
            // with a corner in the air.
            let corner = |across: f32, along: f32| {
                let lateral = here + across;
                let (on, t) = under(at, along, n);
                station.pos
                    + station.right * lateral
                    + station.tangent * along
                    + Vec3::Y * (profile.height(on, t, lateral) + LIFT)
            };
            let base = [
                corner(0.0, -HALF),
                corner(HALF, 0.0),
                corner(0.0, HALF),
                corner(-HALF, 0.0),
            ];
            out.push(Diamond {
                apex: base.iter().sum::<Vec3>() / FACES as f32 + Vec3::Y * TALL,
                base,
                color: paint(r, g, b),
            });
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

/// The stations that carry a diamond, each with its place in the line — which is
/// what the colour is read off, so the cycle starts where the line does.
///
/// One line per stretch of marked road, laid from the head of it so that the
/// spacing is exact all the way down and the last diamond of the approach lands
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

/// The road the diamonds are laid along: every corner, and [`APPROACH`] of them
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
    let n = stations.len();
    let bend: Vec<bool> = (0..n)
        .map(|i| {
            let mut sum = 0.0;
            for d in 0..SMOOTH {
                sum += stations[(i + n + d - SMOOTH / 2) % n].curvature;
            }
            // `radius < CORNER_RADIUS`, without dividing by a curvature that is
            // zero down every straight on the circuit.
            (sum / SMOOTH as f32).abs() * CORNER_RADIUS > 1.0
        })
        .collect();
    let reach = (MERGE / STEP) as usize;
    (0..n)
        .map(|i| (0..=reach).any(|d| bend[(i + d) % n] || bend[(i + n - d) % n]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{Track, circuits};
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn each_pair_has_two_complete_muted_diamonds() {
        for (_, track) in every_track() {
            for pair in diamonds(track.ribbon.stations(), &track.profile).chunks_exact(2) {
                let mesh = mesh(pair);
                assert_eq!(mesh.count_vertices(), 2 * FACES * 3);
                let Some(VertexAttributeValues::Float32x4(colors)) =
                    mesh.attribute(Mesh::ATTRIBUTE_COLOR)
                else {
                    panic!("markers lost their colours");
                };
                for (diamond, vertices) in pair.iter().zip(colors.chunks_exact(FACES * 3)) {
                    assert!(vertices.iter().all(|&color| color == diamond.color));
                    assert!(diamond.color[..3].iter().all(|&channel| channel < 0.12));
                }
                let normals = mesh
                    .attribute(Mesh::ATTRIBUTE_NORMAL)
                    .unwrap()
                    .as_float3()
                    .unwrap();
                assert!(
                    normals
                        .iter()
                        .all(|n| Vec3::from(*n).is_finite() && n[1] > 0.0)
                );
            }
        }
    }

    #[test]
    fn only_the_next_four_alternating_pairs_show_on_each_circuit() {
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
                    let mut query = world.query::<(&MarkerPair, &Visibility)>();
                    // A switch replaces every old pair, and never creates the skipped ones.
                    assert_eq!(query.iter(world).count(), kept.len());
                    assert!(query.iter(world).all(|(pair, _)| kept.contains(&pair.s)));
                    let mut visible = Vec::new();
                    let mut hidden_ahead = Vec::new();
                    for (pair, visibility) in query.iter(world) {
                        let distance = ((pair.s - from) * direction).rem_euclid(length);
                        if *visibility == Visibility::Visible {
                            assert!(
                                distance > 0.0 && distance <= VISIBLE_AHEAD,
                                "{}: from {} to {}, direction {}, distance {}",
                                world.resource::<Track>().circuit.name,
                                station.s,
                                pair.s,
                                direction,
                                distance
                            );
                            visible.push(distance);
                        } else if distance > 0.0 && distance <= VISIBLE_AHEAD {
                            hidden_ahead.push(distance);
                        }
                    }
                    assert!(visible.len() <= 4);
                    assert_eq!(visible.len(), (visible.len() + hidden_ahead.len()).min(4));
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

    /// Diamonds mean a corner, so a diamond where there is no corner means
    /// nothing. Every one is either in a bend or inside the run up to one —
    /// never a stray dot down the back straight.
    #[test]
    fn a_diamond_means_a_corner() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let n = stations.len();
            let corner = corners(stations);
            for (at, _) in markers(stations) {
                let coming = corner[at] || (1..=APPROACH * SPACING).any(|d| corner[(at + d) % n]);
                assert!(
                    coming,
                    "{name}: a diamond at station {at} has no corner to mark"
                );
            }
        }
    }

    /// The point of the line. Every corner long enough to hold a diamond carries
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
                     long and carries no diamond"
                );
            }
            assert!(checked > 0, "{name} has no corner long enough to test");
        }
    }

    /// The rhythm. Down any one stretch of marked road every diamond is exactly
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
                        "{name}: the diamond at station {at} is {gap} stations \
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

    /// The colours turn over exactly as far as the brakes do. [`PALETTE`] is
    /// [`APPROACH`] long, so between the diamond where the brakes go on for the
    /// slowest corners and the diamond on the corner entry you see every colour
    /// once, and the one coming back round is the turn itself.
    #[test]
    fn the_palette_turns_over_the_approach() {
        assert_eq!(
            PALETTE.len(),
            APPROACH,
            "a turn of the cycle is an approach"
        );
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let n = stations.len();
            let corner = corners(stations);
            let marked = markers(stations);
            let mut checked = 0;
            for (k, &(at, step)) in marked.iter().enumerate() {
                // A corner entry whose approach is its own. In a complex the
                // stretch starts back at the corner before, and the line with
                // it, so the entry lands wherever the count has got to.
                if !corner[at] || corner[(at + n - 1) % n] || step != APPROACH {
                    continue;
                }
                checked += 1;
                // The head of this diamond's stretch: `marked` runs stretch by
                // stretch, and `step` is how far down its own it sits.
                let (head, place) = marked[k - step];
                assert_eq!(place, 0, "{name}: the line does not start at its head");
                assert_eq!(
                    (at + n - head) % n,
                    APPROACH * SPACING,
                    "{name}: the approach to the corner at station {at} is not \
                     {APPROACH} diamonds long"
                );
                assert_eq!(
                    PALETTE[step % PALETTE.len()],
                    PALETTE[place],
                    "{name}: the corner at station {at} does not land on the \
                     colour its approach started with"
                );
            }
            assert!(
                checked > 0,
                "{name} has no corner with an approach of its own"
            );
        }
    }

    /// One diamond is every other diamond, and both verges carry the same line
    /// in the same colours. These are geometry rather than paint, so nothing
    /// about the sweep holds them to a size or a shape — this does.
    #[test]
    fn every_diamond_is_the_same_diamond() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let diamonds = diamonds(stations, &track.profile);
            assert_eq!(
                diamonds.len(),
                markers(stations).len() * 2,
                "{name}: the two verges do not carry the same line"
            );
            for pair in diamonds.chunks_exact(2) {
                assert_eq!(
                    pair[0].color, pair[1].color,
                    "{name}: the two verges disagree about a diamond's colour"
                );
            }
            for diamond in &diamonds {
                // Across the verge the diagonal picks up the fall of it, so it
                // comes out a little longer than the flat one.
                let across = diamond.base[1].distance(diamond.base[3]);
                let along = diamond.base[0].distance(diamond.base[2]);
                assert!(
                    (HALF * 2.0..HALF * 2.0 + 0.02).contains(&across),
                    "{name}: a diamond is {across:.4} m across, not {:.2}",
                    HALF * 2.0
                );
                assert!(
                    (along - HALF * 2.0).abs() < 1e-3,
                    "{name}: a diamond is {along:.4} m long, not {:.2}",
                    HALF * 2.0
                );
                // The point is straight up from the middle of the base, by the
                // same amount everywhere. A marker that took the camber of the
                // verge with it would lean out over the road on one side and in
                // on the other, and the two verges would not be one line.
                let middle = diamond.base.iter().sum::<Vec3>() / 4.0;
                assert!(
                    (diamond.apex - middle - Vec3::Y * TALL).length() < 1e-4,
                    "{name}: a diamond points {:?} rather than straight up",
                    diamond.apex - middle
                );
                // And it stands over all four of them. Not by its full height:
                // the verge falls across the footprint, so the corner on the
                // high side is already some way up the point.
                for corner in diamond.base {
                    assert!(
                        diamond.apex.y - corner.y > TALL * 0.75,
                        "{name}: a diamond stands only {:.3} m over its own corner",
                        diamond.apex.y - corner.y
                    );
                }
            }
        }
    }

    /// The reason a marker is a pyramid and not a post: every face leans further
    /// up than sideways, so the line is lit from above like the rest of the
    /// circuit and no face of it is ever seen edge-on. `loft_faces_up` holds the
    /// whole mesh to the sign of that; this holds it to a margin, on the verge
    /// each circuit actually builds, which is the one that tips the low face
    /// over further. Ten degrees, and today it has forty-one.
    #[test]
    fn no_face_of_a_diamond_stands_on_its_edge() {
        /// Degrees a face has to keep between itself and vertical.
        const UPRIGHT: f32 = 10.0;
        for (name, track) in every_track() {
            let mut steepest = 90.0f32;
            for diamond in diamonds(track.ribbon.stations(), &track.profile) {
                for face in diamond.faces() {
                    // The normal leans off vertical by as much as the face leans
                    // off horizontal, so this is the face's own tilt.
                    let normal = (face[1] - face[0]).cross(face[2] - face[0]);
                    let tilt = (normal.y / normal.length()).acos().to_degrees();
                    steepest = steepest.min(90.0 - tilt);
                }
            }
            assert!(
                steepest > UPRIGHT,
                "{name}: a diamond has a face {steepest:.0} degrees off vertical"
            );
        }
    }

    /// A diamond stands on the shoulder it was given, all four corners of it,
    /// with the grass under every one and clear air past the last.
    ///
    /// The line used to run at a fixed 4.5 m off the centreline, which two
    /// compile-time assertions could hold against a cross-section that was also
    /// fixed. It is not fixed any more: the shoulder is fitted station by
    /// station, so where the line runs is an answer rather than a constant, and
    /// the four corners of one diamond are at four different places along the
    /// road and so on four different cross-sections. This asks each of them.
    ///
    /// On the grass specifically, not merely inside the mesh. Past the grass is
    /// the lip the verge falls away over, at six times the slope, and a marker
    /// hanging off that is a marker that has been knocked over.
    ///
    /// On a bridge the question needs the deck as well as the plan: a diamond
    /// on the shoulder of a bridge is directly above the road underneath, and a
    /// lookup that only knew where it was in plan would report it as sitting in
    /// the middle of a road it is two metres above.
    #[test]
    fn a_diamond_stands_on_the_shoulder_it_was_given() {
        for (name, track) in every_track() {
            let profile = &track.profile;
            let mut nearest = f32::MAX;
            for diamond in diamonds(track.ribbon.stations(), profile) {
                for corner in diamond.base {
                    // Asked of the deck-aware lookup rather than of the plan,
                    // because a diamond on the shoulder of a bridge is directly
                    // above the road going under it and the plan cannot tell
                    // the two apart. The corner carries its own height, which
                    // is what the lookup goes on when nothing tells it where it
                    // was.
                    let fix = track.fix(corner, None);
                    let across = fix.lateral.abs();
                    let grass = profile.grass(fix.at, fix.t, fix.lateral);
                    assert!(
                        across > HALF_WIDTH,
                        "{name}: a diamond corner sits {across:.2} m out, on the road"
                    );
                    assert!(
                        across < HALF_WIDTH + grass,
                        "{name}: a diamond corner sits {across:.2} m out, past the \
                         {:.2} m of grass there is there",
                        HALF_WIDTH + grass
                    );
                    nearest = nearest
                        .min(across - HALF_WIDTH)
                        .min(HALF_WIDTH + grass - across);
                }
            }
            assert!(
                nearest >= CLEAR - 1e-3,
                "{name}: a diamond comes within {nearest:.3} m of the edge of its \
                 own grass, against a clearance of {CLEAR}"
            );
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
                "{name}: {marked} diamonds for {entries} corners is the approach \
                 and nothing round the bend"
            );
        }
    }
}
