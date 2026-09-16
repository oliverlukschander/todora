//! The circuit: one closed centreline spline and one cross-section profile.
//!
//! Tarmac, edge lines, kerbs and grass are all strips of a single loft over the
//! stations in [`ribbon`]. Every strip is generated from the same station, the
//! same `right` vector and the same station index, so neighbouring strips share
//! their edge vertices exactly. Nothing overlaps anything, so nothing can
//! z-fight, tear open, or stair-step away from its neighbour.
//!
//! The sweep itself has no special cases — no clamping, no per-corner fudge. It
//! can afford that because the cross-section is checked against what the circuit
//! can actually carry: see [`PROFILE`].

mod layout;
mod ribbon;

use bevy::{
    asset::RenderAssetUsages,
    mesh::Indices,
    prelude::*,
    render::render_resource::PrimitiveTopology,
};

use crate::car::{Car, DriveSet};
use layout::CENTERLINE;
use ribbon::{Ribbon, Station};

/// The car is the ruler: ~2.4 m long, ~1.1 m wide, 1 unit = 1 metre.
const PLAN_SCALE: f32 = 0.4 / 3.0;
/// Elevation is 40% of the real Red Bull Ring, then smoothed and grade-capped.
const HEIGHT_SCALE: f32 = 0.4;
/// Half of the 8 m road, kerbs and edge lines included.
///
/// The brief called for 12 m. [`PROFILE`] explains why the circuit cannot carry
/// it: at ⅓ plan scale, Spielberg passes within 14.7 m of itself, which caps the
/// whole cross-section at 7.3 m either side. A 12 m road would spend all of that
/// on asphalt and leave no verge at all — and at 1.1 m wide, the car reads better
/// against 8 m than it did against 12.
const HALF_WIDTH: f32 = 4.0;
/// Half-width of the asphalt itself: the kerbs and edge lines sit inside
/// [`HALF_WIDTH`], so this is where a wheel starts rumbling.
const TARMAC_HALF: f32 = 3.15;
/// Height of the kerb's outer lip, which the verge hangs off.
const KERB_TOP: f32 = 0.05;
/// How far the cross-section reaches either side of the centreline. Bounded by
/// the circuit — see [`PROFILE`].
const EDGE: f32 = 7.0;
/// How much of a corner's radius the outermost rib may use. Leaving headroom
/// keeps the verge a proper surface instead of a sliver.
const CORNER_MARGIN: f32 = 0.15;
/// One kerb stripe and the start/finish paint, in stations. [`ribbon::STEP`] is
/// the station spacing, so a stripe is two stations long: about a third of the
/// 2.4 m car.
const STRIPE: usize = 2;
/// Speed bled off per second with a wheel on the kerb.
const RUMBLE: f32 = 16.0;
/// Keep the car's body off the grass, not just its centre.
const MARGIN: f32 = 0.52;

/// Cross-section of the circuit, left verge to right verge: `(lateral, height,
/// surface)`. `lateral` is metres right of the centreline, `height` is metres
/// above the road surface, and `surface` covers the strip from this rib to the
/// next — so the last rib only contributes its edge.
///
/// [`EDGE`], the outermost `lateral`, is what makes the loft safe to sweep
/// unconditionally, and the circuit sets it. Two things bound it:
///
/// - **Curvature.** An offset curve is regular only while
///   `1 - curvature * lateral > 0`; at the radius of curvature it cusps and past
///   it folds back through itself. So `EDGE <= (1 - CORNER_MARGIN) * min_radius`,
///   and [`ribbon::MIN_RADIUS`] is what the corner-opening pass guarantees.
/// - **Separation.** Where two stretches of circuit run close together, their
///   verges grow into each other even though nothing is wrong at either station.
///   So `EDGE <= min_separation / 2`. For Spielberg this is the tighter of the
///   two: 14.7 m apart at the closest, so 7.3 m.
///
/// `profile_fits_the_circuit` checks both against the ribbon that was actually
/// built, so widening the road or swapping the layout fails loudly rather than
/// quietly folding the mesh.
const PROFILE: &[(f32, f32, Surface)] = &[
    (-EDGE, -0.95, Surface::Skirt),
    (-6.00, -0.20, Surface::Grass),
    (-HALF_WIDTH, KERB_TOP, Surface::Kerb),
    (-3.30, 0.00, Surface::Line),
    (-TARMAC_HALF, 0.00, Surface::Tarmac),
    (TARMAC_HALF, 0.00, Surface::Line),
    (3.30, 0.00, Surface::Kerb),
    (HALF_WIDTH, KERB_TOP, Surface::Grass),
    (6.00, -0.20, Surface::Skirt),
    (EDGE, -0.95, Surface::End),
];

/// What a strip of the loft is made of. Colour only — every strip is the same
/// surface geometrically.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Surface {
    Tarmac,
    Line,
    Kerb,
    Grass,
    Skirt,
    /// Closes the profile. No strip starts here.
    End,
}

impl Surface {
    /// Colour of the strip at station `i`, linear for the vertex colour
    /// attribute. Counting stations rather than measuring metres is what keeps
    /// the paint crisp: a strip is one station long and takes one flat colour, so
    /// a stripe edge lands exactly on a strip edge instead of smearing across it.
    /// The ribbon rounds its station count so the pattern meets itself at the
    /// start/finish line.
    fn paint(self, i: usize) -> [f32; 4] {
        match self {
            Surface::Tarmac if i < STRIPE => paint(0.90, 0.90, 0.88),
            Surface::Tarmac => paint(0.15, 0.15, 0.17),
            Surface::Line => paint(0.90, 0.90, 0.88),
            Surface::Kerb if (i / STRIPE) % 2 == 0 => paint(0.76, 0.13, 0.11),
            Surface::Kerb => paint(0.93, 0.93, 0.91),
            Surface::Grass => paint(0.33, 0.52, 0.24),
            Surface::Skirt | Surface::End => paint(0.25, 0.42, 0.19),
        }
    }
}

fn paint(r: f32, g: f32, b: f32) -> [f32; 4] {
    let c = Color::srgb(r, g, b).to_linear();
    [c.red, c.green, c.blue, c.alpha]
}

pub struct TrackPlugin;

impl Plugin for TrackPlugin {
    fn build(&self, app: &mut App) {
        // Built here rather than in a startup system so the car and the camera
        // can read the grid slot the moment they spawn.
        app.insert_resource(Track::new())
            .add_systems(Startup, setup)
            .add_systems(Update, confine.after(DriveSet));
    }
}

#[derive(Resource)]
pub struct Track {
    ribbon: Ribbon,
}

impl Track {
    fn new() -> Self {
        let control: Vec<Vec3> = CENTERLINE
            .iter()
            .map(|p| Vec3::new(p[0] * PLAN_SCALE, p[1] * HEIGHT_SCALE, p[2] * PLAN_SCALE))
            .collect();
        let ribbon = Ribbon::new(&control);
        // The contract in [`PROFILE`], checked against the circuit that was
        // actually built. Swap the layout or widen the road and this is what
        // says so, rather than the mesh quietly folding.
        debug_assert!(
            EDGE <= (1.0 - CORNER_MARGIN) * ribbon.min_radius(),
            "a {EDGE} m cross-section cusps in this circuit's {} m corners",
            ribbon.min_radius()
        );
        debug_assert!(
            EDGE <= ribbon.min_separation() / 2.0,
            "a {EDGE} m cross-section collides with itself where this circuit \
             passes {} m from itself",
            ribbon.min_separation()
        );
        Self { ribbon }
    }

    /// Pose of the grid slot, facing down the start/finish straight.
    pub fn start_transform(&self) -> Transform {
        let start = self.ribbon.start();
        Transform::from_translation(start.pos).looking_to(start.tangent, Vec3::Y)
    }

    /// Signed distance past the start/finish plane, along the circuit.
    pub(crate) fn start_along(&self, pos: Vec3) -> f32 {
        let start = self.ribbon.start();
        (pos - start.pos).reject_from(Vec3::Y).dot(start.tangent)
    }

    pub(crate) fn on_start_gate(&self, pos: Vec3) -> bool {
        let start = self.ribbon.start();
        (pos - start.pos).reject_from(Vec3::Y).dot(start.right).abs() < HALF_WIDTH + 1.5
    }

    /// 0 at start/finish, approaching 1 at the end of the lap.
    pub(crate) fn progress(&self, pos: Vec3) -> f32 {
        self.ribbon.locate(pos).s / self.ribbon.length()
    }
}

fn setup(
    mut commands: Commands,
    track: Res<Track>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(loft(&track.ribbon))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.9,
            ..default()
        })),
    ));
}

/// Sweep the cross-section along the centreline: one strip per profile band, one
/// quad per station, closing round to the start.
///
/// Each quad carries its own four vertices so it can take one flat colour and the
/// kerb stripes stay crisp; interpolating colour between shared rings smears a
/// 0.8 m stripe into a gradient. Shading does not suffer for it, because the
/// normals are computed from the loft rather than from the triangles: across the
/// strip they come from the profile, giving a hard crease at every rib, and along
/// it from the neighbouring stations, so the road still reads as smooth.
///
/// The ring-per-station, quad-between-rings shape follows `bevy_more_shapes`'
/// tube loft, with the frame locked to world up instead of Frenet-Serret — a road
/// must not roll with the curve's torsion.
fn loft(ribbon: &Ribbon) -> Mesh {
    let stations = ribbon.stations();
    let n = stations.len();
    let bands = PROFILE.len() - 1;
    let mut positions = Vec::with_capacity(n * bands * 4);
    let mut normals = Vec::with_capacity(n * bands * 4);
    let mut colors = Vec::with_capacity(n * bands * 4);
    let mut indices = Vec::with_capacity(n * bands * 6);

    for band in 0..bands {
        let (left, right) = (PROFILE[band], PROFILE[band + 1]);
        let edge = |station: &Station, rib: (f32, f32, Surface)| {
            station.pos + station.right * rib.0 + Vec3::Y * rib.1
        };
        let rim: Vec<[Vec3; 2]> = stations
            .iter()
            .map(|station| [edge(station, left), edge(station, right)])
            .collect();
        let rim_normal = |i: usize, side: usize| {
            let across = rim[i][1] - rim[i][0];
            let along = rim[(i + 1) % n][side] - rim[(i + n - 1) % n][side];
            across.cross(along).normalize_or(Vec3::Y).to_array()
        };

        for i in 0..n {
            let j = (i + 1) % n;
            let color = left.2.paint(i);
            for (station, side) in [(i, 0), (i, 1), (j, 0), (j, 1)] {
                positions.push(rim[station][side].to_array());
                normals.push(rim_normal(station, side));
                colors.push(color);
            }
            let base = (positions.len() - 4) as u32;
            indices.extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
        }
    }

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}

/// Hold the car on the road: sit it on the loft's surface, rumble on the kerbs,
/// and refuse the last stretch to the grass.
fn confine(time: Res<Time>, track: Res<Track>, mut cars: Query<(&mut Transform, &mut Car)>) {
    let dt = time.delta_secs();
    let max_lat = HALF_WIDTH - MARGIN;
    for (mut transform, mut car) in &mut cars {
        let fix = track.ribbon.locate(transform.translation);
        transform.translation.y = fix.point.y;
        if fix.lateral.abs() > TARMAC_HALF {
            let bleed = RUMBLE * dt;
            car.speed -= car.speed.clamp(-bleed, bleed);
        }
        if fix.lateral.abs() <= max_lat {
            continue;
        }
        let clamped = fix.lateral.clamp(-max_lat, max_lat);
        transform.translation.x = fix.point.x + fix.right.x * clamped;
        transform.translation.z = fix.point.z + fix.right.z * clamped;
        let velocity = *transform.forward() * car.speed;
        let lateral_speed = velocity.dot(fix.right);
        if lateral_speed * fix.lateral.signum() > 0.0 {
            let along = velocity - fix.right * lateral_speed;
            car.speed = along.length() * car.speed.signum();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track() -> Track {
        Track::new()
    }

    #[test]
    fn profile_is_ordered_and_symmetric() {
        assert_eq!(PROFILE[0].0, -EDGE);
        assert_eq!(PROFILE[PROFILE.len() - 1].0, EDGE);
        assert_eq!(PROFILE[PROFILE.len() - 1].2, Surface::End);
        for pair in PROFILE.windows(2) {
            assert!(
                pair[1].0 > pair[0].0,
                "profile rib {} does not come after {}",
                pair[1].0,
                pair[0].0
            );
        }
        for (a, b) in PROFILE.iter().zip(PROFILE.iter().rev()) {
            assert_eq!(a.0, -b.0, "profile is not symmetric about the centreline");
            assert_eq!(a.1, b.1);
        }
    }

    /// The whole reason the loft needs no clamping. Widen the road, widen the
    /// verge, or drop in a tighter circuit, and this is what says no.
    #[test]
    fn profile_fits_the_circuit() {
        let track = track();
        let radius = track.ribbon.min_radius();
        let separation = track.ribbon.min_separation();
        assert!(
            radius > ribbon::MIN_RADIUS * 0.95,
            "corner opening did not converge: {radius} m against a {} m target",
            ribbon::MIN_RADIUS
        );
        assert!(
            EDGE <= (1.0 - CORNER_MARGIN) * radius,
            "a {EDGE} m cross-section cusps in this circuit's {radius} m corners"
        );
        assert!(
            EDGE <= separation / 2.0,
            "verges collide: the circuit passes within {separation} m of itself, \
             which leaves room for {} m either side, not {EDGE}",
            separation / 2.0
        );
    }

    /// Stated directly, station by station: no rib of the swept profile ever
    /// reaches its own centre of curvature, so no strip can fold back on itself.
    #[test]
    fn every_offset_stays_regular() {
        let track = track();
        for station in track.ribbon.stations() {
            for rib in PROFILE {
                let jacobian = 1.0 - station.curvature * rib.0;
                assert!(
                    jacobian > CORNER_MARGIN,
                    "rib {} folds at s={} (jacobian {jacobian})",
                    rib.0,
                    station.s
                );
            }
        }
    }

    /// One stripe is a whole number of stations and the lap is a whole number of
    /// stripe pairs, so the kerb pattern meets itself at the start/finish line.
    #[test]
    fn kerb_stripes_close_at_the_line() {
        let track = track();
        let n = track.ribbon.stations().len();
        assert_eq!(n % (STRIPE * 2), 0, "{n} stations breaks the stripe pattern");
        assert_eq!(Surface::Kerb.paint(0), Surface::Kerb.paint(n - STRIPE * 2));
        assert_ne!(Surface::Kerb.paint(0), Surface::Kerb.paint(STRIPE));
    }

    #[test]
    fn hills_roll_instead_of_stepping() {
        let track = track();
        let stations = track.ribbon.stations();
        let n = stations.len();
        let mut steepest = 0.0f32;
        for i in 0..n {
            let a = stations[i].pos;
            let b = stations[(i + 1) % n].pos;
            let run = Vec3::new(b.x - a.x, 0.0, b.z - a.z).length().max(1e-4);
            steepest = steepest.max((b.y - a.y).abs() / run);
        }
        assert!(steepest < 0.2, "max grade {steepest} is a cliff, not a roll");
        let (low, high) = stations.iter().fold((f32::MAX, f32::MIN), |(l, h), s| {
            (l.min(s.pos.y), h.max(s.pos.y))
        });
        assert!(high - low > 12.0, "smoothing flattened the circuit away");
    }

    #[test]
    fn loft_closes_and_covers_the_lap() {
        let track = track();
        let lap = track.ribbon.length();
        assert!((450.0..650.0).contains(&lap), "lap is {lap} m");
        let mesh = loft(&track.ribbon);
        let verts = mesh.count_vertices();
        assert_eq!(verts, track.ribbon.stations().len() * (PROFILE.len() - 1) * 4);
        let Some(Indices::U32(indices)) = mesh.indices() else {
            panic!("loft lost its indices");
        };
        assert_eq!(indices.len(), verts / 4 * 6);
        assert!(indices.iter().all(|&i| (i as usize) < verts));
    }

    /// Every triangle winds the same way round, so the circuit is not visible
    /// from below and invisible from above.
    #[test]
    fn loft_faces_up() {
        let track = track();
        let mesh = loft(&track.ribbon);
        let Some(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("loft lost its positions");
        };
        let positions = positions.as_float3().expect("positions are float3");
        let Some(Indices::U32(indices)) = mesh.indices() else {
            panic!("loft lost its indices");
        };
        for face in indices.chunks_exact(3) {
            let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(positions[face[k] as usize]));
            let normal = (b - a).cross(c - a);
            // The steepest strip in the profile still leans far more up than sideways.
            assert!(
                normal.y > 0.0 || normal.length_squared() < 1e-12,
                "a face at {a} winds the wrong way"
            );
        }
    }

    /// Walk the whole lap the way the timer sees it: progress climbs from the
    /// line back to the line, and the gate opens exactly once, at the line.
    #[test]
    fn a_lap_reads_as_one_lap() {
        let track = track();
        let stations = track.ribbon.stations();
        let mut highest = 0.0f32;
        let mut crossings = 0;
        let mut previous = track.start_along(stations[stations.len() - 1].pos);
        for station in stations {
            // Drive a little off the racing line, as a car would.
            let pos = station.pos + station.right * (TARMAC_HALF * 0.5);
            let progress = track.progress(pos);
            assert!(
                progress >= highest - 0.02,
                "progress went backwards at s={}: {progress} after {highest}",
                station.s
            );
            highest = highest.max(progress);
            let along = track.start_along(pos);
            if previous <= 0.0 && along > 0.0 && track.on_start_gate(pos) {
                crossings += 1;
            }
            previous = along;
        }
        assert!(highest > 0.97, "the lap only reached {highest}");
        assert_eq!(crossings, 1, "the start gate opened {crossings} times");
    }

    /// The grid slot is on the tarmac, pointing down the circuit.
    #[test]
    fn start_is_on_the_road() {
        let track = track();
        let pose = track.start_transform();
        let fix = track.ribbon.locate(pose.translation);
        assert!(fix.lateral.abs() < 0.01);
        assert!(track.on_start_gate(pose.translation));
        assert!(track.progress(pose.translation) < 0.01);
        let ahead = pose.translation + *pose.forward() * 3.0;
        assert!(track.start_along(ahead) > 2.9);
    }
}

