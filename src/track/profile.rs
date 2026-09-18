//! The cross-section of the circuit, and everything that is read from it.
//!
//! One table, [`PROFILE`], is the source of truth for three things: the mesh
//! (every strip of the loft is a pair of its ribs), what the car stands on
//! ([`Profile::height`]) and what the car has to hold on with ([`grip`]). Because
//! they all read the same ribs, the car rides the kerb because the kerb is 5 cm
//! proud here, and loses grip on the grass because the grass starts here —
//! nothing is said twice.
//!
//! The road is the same on every circuit. The verge is not: how far a
//! cross-section may reach is set by the circuit it is swept along, and a
//! circuit that doubles back on itself has less room than one that does not.
//! [`Profile::fit`] asks the circuit and narrows the verge to the answer, which
//! is what lets the loft sweep with no clamping anywhere and lets a new circuit
//! be a file rather than a negotiation.

use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};

use super::ribbon::{Ribbon, Station};

/// Half of the 8 m road, kerbs and edge lines included.
///
/// The brief called for 12 m. [`PROFILE`] explains why the circuit cannot carry
/// it: at ⅓ plan scale, Spielberg passes within 14.7 m of itself, which caps the
/// whole cross-section at 7.3 m either side. A 12 m road would spend all of that
/// on asphalt and leave no verge at all — and at 1.1 m wide, the car reads better
/// against 8 m than it did against 12.
pub(super) const HALF_WIDTH: f32 = 4.0;
/// Half-width of the asphalt itself: the kerbs and edge lines sit inside
/// [`HALF_WIDTH`], so this is where a wheel starts rumbling.
pub(super) const TARMAC_HALF: f32 = 3.15;
/// Height of the kerb's outer lip, which the verge hangs off.
pub(super) const KERB_TOP: f32 = 0.05;
/// How far the cross-section reaches either side of the centreline on a circuit
/// with room for all of it. A circuit with less gets less — see [`Profile::fit`].
pub(super) const EDGE: f32 = 7.0;
/// Verge enough to be a verge: somewhere to run wide, and a shoulder for the
/// road to sit on. A circuit too cramped for even this is one Todora cannot
/// hold a road on at this scale, and [`Profile::fit`] says so.
const LEAST_VERGE: f32 = 1.5;
/// How much of a corner's radius the outermost rib may use. Leaving headroom
/// keeps the verge a proper surface instead of a sliver.
const CORNER_MARGIN: f32 = 0.15;
/// One kerb stripe and the start/finish paint, in stations. [`ribbon::STEP`] is
/// the station spacing, so a stripe is two stations long: about a third of the
/// 2.4 m car.
const STRIPE: usize = 2;
/// Fraction of tarmac grip the kerbs and the grass give back.
pub(super) const KERB_GRIP: f32 = 0.72;
pub(super) const GRASS_GRIP: f32 = 0.38;

/// Cross-section of the circuit, left verge to right verge: `(lateral, height,
/// surface)`. `lateral` is metres right of the centreline, `height` is metres
/// above the road surface, and `surface` covers the strip from this rib to the
/// next — so the last rib only contributes its edge.
///
/// This is the cross-section a circuit with room gets. Where there is less room
/// the ribs outside [`HALF_WIDTH`] are drawn in together, so the verge narrows
/// and the road never moves.
///
/// [`EDGE`], the outermost `lateral`, is what makes the loft safe to sweep
/// unconditionally, and the circuit sets it. Two things bound it:
///
/// - **Curvature.** An offset curve is regular only while
///   `1 - curvature * lateral > 0`; at the radius of curvature it cusps and past
///   it folds back through itself. So the edge stays inside
///   `(1 - CORNER_MARGIN) * min_radius`, and [`ribbon::MIN_RADIUS`] is what the
///   corner-opening pass guarantees.
/// - **Separation.** Where two stretches of circuit run close together, their
///   verges grow into each other even though nothing is wrong at either station.
///   So the edge stays inside `min_separation / 2`: Spielberg passes within
///   14.7 m of itself and allows 7.3 m, Spa within 25.2 m and allows 12.6 m.
///
/// [`Profile::fit`] takes the smallest of those and this, against the ribbon
/// that was actually built, so a new circuit narrows its own verge rather than
/// quietly folding its mesh.
const PROFILE: &[(f32, f32, Band)] = &[
    (-EDGE, -0.95, Band::Skirt),
    (-6.00, -0.20, Band::Grass),
    (-HALF_WIDTH, KERB_TOP, Band::Kerb),
    (-3.30, 0.00, Band::Line),
    (-TARMAC_HALF, 0.00, Band::Tarmac),
    (TARMAC_HALF, 0.00, Band::Line),
    (3.30, 0.00, Band::Kerb),
    (HALF_WIDTH, KERB_TOP, Band::Grass),
    (6.00, -0.20, Band::Skirt),
    (EDGE, -0.95, Band::End),
];

/// What a strip of the loft is made of. Colour only — every strip is the same
/// surface geometrically.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Band {
    Tarmac,
    Line,
    Kerb,
    Grass,
    Skirt,
    /// Closes the profile. No strip starts here.
    End,
}

impl Band {
    /// Colour of the strip at station `i`, linear for the vertex colour
    /// attribute. Counting stations rather than measuring metres is what keeps
    /// the paint crisp: a strip is one station long and takes one flat colour, so
    /// a stripe edge lands exactly on a strip edge instead of smearing across it.
    /// The ribbon rounds its station count so the pattern meets itself at the
    /// start/finish line.
    fn paint(self, i: usize) -> [f32; 4] {
        match self {
            Band::Tarmac if i < STRIPE => paint(0.90, 0.90, 0.88),
            Band::Tarmac => paint(0.15, 0.15, 0.17),
            Band::Line => paint(0.90, 0.90, 0.88),
            Band::Kerb if (i / STRIPE).is_multiple_of(2) => paint(0.76, 0.13, 0.11),
            Band::Kerb => paint(0.93, 0.93, 0.91),
            Band::Grass => paint(0.33, 0.52, 0.24),
            Band::Skirt | Band::End => paint(0.25, 0.42, 0.19),
        }
    }
}

fn paint(r: f32, g: f32, b: f32) -> [f32; 4] {
    let c = Color::srgb(r, g, b).to_linear();
    [c.red, c.green, c.blue, c.alpha]
}

/// [`PROFILE`], narrowed to what one circuit can carry.
pub(super) struct Profile {
    ribs: Vec<(f32, f32, Band)>,
    edge: f32,
}

impl Profile {
    /// The widest cross-section this circuit can be swept with, up to the full
    /// [`PROFILE`]. Widen the road, or drop in a circuit with no room at all for
    /// a verge, and this is what says no — rather than the mesh folding.
    pub(super) fn fit(ribbon: &Ribbon) -> Result<Self, String> {
        let corner = (1.0 - CORNER_MARGIN) * ribbon.min_radius();
        let between = ribbon.min_separation() / 2.0;
        let edge = EDGE.min(corner).min(between);
        if edge < HALF_WIDTH + LEAST_VERGE {
            return Err(format!(
                "this circuit carries only {edge:.2} m either side of the centreline: \
                 its tightest corner allows {corner:.2} m and it passes within \
                 {:.2} m of itself, which is not a {HALF_WIDTH} m road and a verge",
                between * 2.0
            ));
        }
        Ok(Self {
            ribs: narrowed(edge),
            edge,
        })
    }

    /// How far the cross-section reaches either side of the centreline.
    pub(super) fn edge(&self) -> f32 {
        self.edge
    }

    /// Height of the cross-section at `lateral`, above the road surface. What the
    /// car stands on: the kerb lip and the fall of the verge, read from the same
    /// ribs the mesh was swept from.
    pub(super) fn height(&self, lateral: f32) -> f32 {
        let at = lateral.clamp(-self.edge, self.edge);
        for rib in self.ribs.windows(2) {
            if at <= rib[1].0 {
                let t = (at - rib[0].0) / (rib[1].0 - rib[0].0);
                return rib[0].1.lerp(rib[1].1, t);
            }
        }
        0.0
    }

    /// Sweep the cross-section along the centreline: one strip per profile band,
    /// one quad per station, closing round to the start.
    ///
    /// Each quad carries its own four vertices so it can take one flat colour and
    /// the kerb stripes stay crisp; interpolating colour between shared rings
    /// smears a 0.8 m stripe into a gradient. Shading does not suffer for it,
    /// because the normals are computed from the loft rather than from the
    /// triangles: across the strip they come from the profile, giving a hard
    /// crease at every rib, and along it from the neighbouring stations, so the
    /// road still reads as smooth.
    ///
    /// The ring-per-station, quad-between-rings shape follows `bevy_more_shapes`'
    /// tube loft, with the frame locked to world up instead of Frenet-Serret — a
    /// road must not roll with the curve's torsion.
    pub(super) fn loft(&self, ribbon: &Ribbon) -> Mesh {
        let stations = ribbon.stations();
        let n = stations.len();
        let bands = self.ribs.len() - 1;
        let mut positions = Vec::with_capacity(n * bands * 4);
        let mut normals = Vec::with_capacity(n * bands * 4);
        let mut colors = Vec::with_capacity(n * bands * 4);
        let mut indices = Vec::with_capacity(n * bands * 6);

        for band in 0..bands {
            let (left, right) = (self.ribs[band], self.ribs[band + 1]);
            let edge = |station: &Station, rib: (f32, f32, Band)| {
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
                indices.extend_from_slice(&[
                    base,
                    base + 1,
                    base + 2,
                    base + 1,
                    base + 3,
                    base + 2,
                ]);
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
}

/// [`PROFILE`] with the verge drawn in to reach `edge` instead of [`EDGE`].
/// Everything from the kerb inward is the road, and the road is the same on
/// every circuit, so only the ribs outside it move.
fn narrowed(edge: f32) -> Vec<(f32, f32, Band)> {
    let squeeze = (edge - HALF_WIDTH) / (EDGE - HALF_WIDTH);
    PROFILE
        .iter()
        .map(|&(lateral, height, band)| {
            let over = lateral.abs() - HALF_WIDTH;
            let lateral = if over > 0.0 {
                lateral.signum() * (HALF_WIDTH + over * squeeze)
            } else {
                lateral
            };
            (lateral, height, band)
        })
        .collect()
}

/// Fraction of tarmac grip at `lateral` metres off the centreline. The road is
/// the same on every circuit, so this needs no profile to read it from.
pub(super) fn grip(lateral: f32) -> f32 {
    let across = lateral.abs();
    if across <= TARMAC_HALF {
        1.0
    } else if across <= HALF_WIDTH {
        KERB_GRIP
    } else {
        GRASS_GRIP
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{Track, circuits, ribbon};

    #[test]
    fn profile_is_ordered_and_symmetric() {
        assert_eq!(PROFILE[0].0, -EDGE);
        assert_eq!(PROFILE[PROFILE.len() - 1].0, EDGE);
        assert_eq!(PROFILE[PROFILE.len() - 1].2, Band::End);
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

    /// Narrowing takes the verge and leaves the road alone, and it stays a
    /// cross-section: ordered, symmetric, and reaching exactly as far as asked.
    #[test]
    fn narrowing_takes_the_verge_and_leaves_the_road() {
        for edge in [EDGE, 6.4, HALF_WIDTH + LEAST_VERGE] {
            let ribs = narrowed(edge);
            assert_eq!(ribs.len(), PROFILE.len());
            assert_eq!(ribs[0].0, -edge);
            assert_eq!(ribs[ribs.len() - 1].0, edge);
            for pair in ribs.windows(2) {
                assert!(pair[1].0 > pair[0].0, "rib {} out of order", pair[1].0);
            }
            for (rib, was) in ribs.iter().zip(PROFILE) {
                assert_eq!(rib.1, was.1, "narrowing moved a rib's height");
                assert_eq!(rib.2, was.2, "narrowing repainted a rib");
                if was.0.abs() <= HALF_WIDTH {
                    assert_eq!(rib.0, was.0, "narrowing moved the road");
                } else {
                    assert!(rib.0.abs() <= was.0.abs());
                }
            }
        }
    }

    /// The whole reason the loft needs no clamping, for every circuit in the
    /// game. Widen the road or drop in a circuit with no room and this says so.
    #[test]
    fn every_circuit_carries_a_road() {
        for circuit in circuits::all() {
            let track = Track::new(circuit);
            let radius = track.ribbon.min_radius();
            assert!(
                radius > ribbon::MIN_RADIUS * 0.95,
                "{}: corner opening did not converge: {radius} m against a {} m target",
                circuit.name,
                ribbon::MIN_RADIUS
            );
            let edge = track.profile.edge();
            assert!(
                (HALF_WIDTH + LEAST_VERGE..=EDGE).contains(&edge),
                "{} carries {edge} m either side",
                circuit.name
            );
        }
    }

    /// Stated directly, station by station, on every circuit: no rib of the
    /// swept profile ever reaches its own centre of curvature, so no strip can
    /// fold back on itself.
    #[test]
    fn every_offset_stays_regular() {
        for circuit in circuits::all() {
            let track = Track::new(circuit);
            for station in track.ribbon.stations() {
                for rib in &track.profile.ribs {
                    let jacobian = 1.0 - station.curvature * rib.0;
                    assert!(
                        jacobian > CORNER_MARGIN,
                        "{}: rib {} folds at s={} (jacobian {jacobian})",
                        circuit.name,
                        rib.0,
                        station.s
                    );
                }
            }
        }
    }

    /// One stripe is a whole number of stations and the lap is a whole number of
    /// stripe pairs, so the kerb pattern meets itself at the start/finish line.
    #[test]
    fn kerb_stripes_close_at_the_line() {
        for circuit in circuits::all() {
            let track = Track::new(circuit);
            let n = track.ribbon.stations().len();
            assert_eq!(
                n % (STRIPE * 2),
                0,
                "{}: {n} stations breaks the stripe pattern",
                circuit.name
            );
            assert_eq!(Band::Kerb.paint(0), Band::Kerb.paint(n - STRIPE * 2));
            assert_ne!(Band::Kerb.paint(0), Band::Kerb.paint(STRIPE));
        }
    }

    #[test]
    fn loft_closes_and_covers_the_lap() {
        for circuit in circuits::all() {
            let track = Track::new(circuit);
            let lap = track.ribbon.length();
            assert!(
                (400.0..1200.0).contains(&lap),
                "{} laps {lap} m",
                circuit.name
            );
            let mesh = track.profile.loft(&track.ribbon);
            let verts = mesh.count_vertices();
            assert_eq!(
                verts,
                track.ribbon.stations().len() * (PROFILE.len() - 1) * 4
            );
            let Some(Indices::U32(indices)) = mesh.indices() else {
                panic!("loft lost its indices");
            };
            assert_eq!(indices.len(), verts / 4 * 6);
            assert!(indices.iter().all(|&i| (i as usize) < verts));
        }
    }

    /// Every triangle winds the same way round, so the circuit is not visible
    /// from below and invisible from above.
    #[test]
    fn loft_faces_up() {
        for circuit in circuits::all() {
            let track = Track::new(circuit);
            let mesh = track.profile.loft(&track.ribbon);
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
                // The steepest strip in the profile still leans far more up than
                // sideways.
                assert!(
                    normal.y > 0.0 || normal.length_squared() < 1e-12,
                    "{}: a face at {a} winds the wrong way",
                    circuit.name
                );
            }
        }
    }
}
