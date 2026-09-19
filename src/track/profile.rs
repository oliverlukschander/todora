//! The cross-section of the circuit, and everything that is read from it.
//!
//! One shape, [`section`], is the source of truth for three things: the mesh
//! (every strip of the loft is a pair of its ribs), what the car stands on
//! ([`Profile::height`]) and what the car has to hold on with ([`grip`]).
//! Because they all read the same ribs, the car rides the kerb because the kerb
//! is 5 cm proud here, and loses grip on the grass because the grass starts
//! here — nothing is said twice.
//!
//! The road is the same on every circuit and at every point of one. The verge
//! is not, and it is not even the same on both sides of the car: how far a
//! cross-section may reach is set by the circuit it is swept along, *where it
//! is swept*, and a circuit that runs back past itself has less room on the
//! side it runs back on than on the other. [`Profile::fit`] asks the circuit at
//! every station, on each side, and narrows the verge to the answer.
//!
//! That is a change of kind, not of degree. The verge used to be fitted once
//! for the whole circuit, to the tightest corner anywhere on it and the closest
//! it came to itself anywhere on it — so one pinch cost the whole lap its
//! shoulder, and a circuit with one pinch too many was refused outright. Now a
//! pinch costs the road either side of it and nothing else, which is what lets
//! the loft sweep with no clamping anywhere *and* lets in circuits that are
//! cramped in one place rather than everywhere.

use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};

use super::markers;
use super::ribbon::{Ribbon, Station};

/// Half of the 3.6 m road, kerbs and edge lines included.
pub(crate) const HALF_WIDTH: f32 = 1.8;
/// Asphalt ends where the edge line and kerb begin.
pub(super) const TARMAC_HALF: f32 = HALF_WIDTH - KERB_WIDTH - LINE_WIDTH;
/// Where the white edge line stops and the kerb begins.
const KERB_INNER: f32 = HALF_WIDTH - KERB_WIDTH;
/// How wide the kerb is, and the white line inside it.
///
/// The kerb is a place to put a wheel, so it is sized against the wheel: a
/// little wider than the tyre is, which is what makes riding it a thing the
/// driver chooses rather than a thing that happens. The line is paint and is
/// sized to be seen — a twentieth of the road, where multiplying the old one
/// down would have given a sixtieth.
const KERB_WIDTH: f32 = 0.25;
const LINE_WIDTH: f32 = 0.08;
/// Half the width of the car's rolling footprint: the outside of the outer
/// tyre. What the asphalt has to be able to hold.
const ROLLING_HALF: f32 = crate::car::HALF_TRACK + crate::car::WHEEL_WIDTH / 2.0;
const _: () = assert!(TARMAC_HALF > 2.0 * ROLLING_HALF);
const _: () = assert!(KERB_WIDTH > crate::car::WHEEL_WIDTH);
/// Height of the kerb's outer lip, which the verge hangs off.
pub(super) const KERB_TOP: f32 = 0.05;
/// Runoff for the smaller car; tighter sections use less, via [`Profile::fit`].
pub(super) const VERGE: f32 = 1.5;
/// The whole cross-section at its widest, either side of the centreline.
pub(super) const EDGE: f32 = HALF_WIDTH + VERGE;
/// How much of the verge is grass before the lip it falls away over.
const GRASS_OF_VERGE: f32 = 2.0 / 3.0;
/// How steeply the grass falls away from the kerb, and how steeply the lip
/// outside it falls away from the grass.
///
/// The verge is described by its slopes rather than by the heights they arrive
/// at, which is what lets it be any width. The old table named the heights —
/// −0.20 m at the grass and −0.95 m at the lip — and those are what a 3 m verge
/// comes to at these slopes, so a circuit with the room is unchanged to the
/// last bit. A circuit without it gets a shallower verge instead of the same
/// 0.95 m drop crammed into a fifth of the run, which would have been a cliff
/// with a corner marker leaning off it.
const GRASS_FALL: f32 = 0.125;
const SKIRT_FALL: f32 = 0.75;
/// How much of a corner's radius the outermost rib leaves unused on the inside
/// of the bend.
///
/// An offset curve is regular only while `1 - curvature * lateral > 0`; at the
/// radius of curvature it cusps and past it folds back through itself. Holding
/// back this much keeps the innermost strip a surface rather than a sliver
/// wrapped round the middle of the corner, and it is the *jacobian* that is
/// held: a rib fitted to this bound has `1 - curvature * lateral` equal to it
/// exactly, which is what `every_offset_stays_regular` reads back.
///
/// A fraction, because what it is protecting scales with the corner: a tenth of
/// a 50 m sweep is 5 m of slack and a tenth of a 5 m hairpin is 0.5 m, and both
/// are a tenth of the way to the cusp. This also only applies to the inside of
/// a bend. Offsetting outward from a curve is regular however far it goes, and
/// the old fit narrowed both verges for a corner that only ever threatened one
/// of them.
const SPARE: f32 = 0.15;
/// Clear air between the outer edge of the cross-section and whatever else of
/// the circuit is near it.
///
/// A distance rather than a fraction, because what it is protecting does not
/// scale with anything: two stretches of road passing each other need to not
/// touch, and 0.25 m of grass between them is 0.25 m of grass whether the gap
/// was 5 m or 15. Paul Ricard's two verges used to meet *exactly* at its
/// narrowest point, because the old fit took the whole of half the gap and
/// stopped. Nothing folded; nothing was clear either, and "nothing overlaps
/// anything" was true only in the sense that two coincident edges are not
/// crossing.
///
/// Kept apart from [`SPARE`] on purpose. Charging a corner's proportional
/// margin against a pinch as well would cost a seventh of the gap where a
/// quarter of a metre is the whole of what is needed, and a seventh of the gap
/// is the difference between three more circuits fitting a road and not.
const CLEARANCE: f32 = 0.25;
/// How fast the verge may change width, in metres across per metre along.
///
/// A shoulder that appeared and vanished station by station would be a ragged
/// edge to the mesh and a wall that jumped in and out under the car. Tapering
/// only ever takes width away — a station narrowed by the circuit around it is
/// never widened back by its neighbours being roomier, or the taper would undo
/// the fit it is smoothing.
const TAPER: f32 = 0.25;
/// Verge enough to be a verge.
///
/// This replaces a flat 1.5 m, which was a measure of generous runoff rather
/// than of whether the road and its shoulder actually fit. What has to fit is:
/// the car, which is held [`super::WALL_INSET`] inside the outer edge and must
/// therefore have that much verge before the wall is inside the road; and a
/// corner marker, which stands on the grass and needs its own footprint and a
/// clearance either side of it, on a grass band that is
/// [`GRASS_OF_VERGE`] of the verge. Whichever of those is larger is the floor,
/// and the const assertions below are what keep this honest as the parts move.
///
/// The old figure admitted none of the six circuits that fail for want of room,
/// and it was not measuring anything: 1.5 m of grass is pleasant, and a circuit
/// is not unbuildable for having 1.4.
const LEAST_VERGE: f32 = 0.5;
const _: () = assert!(LEAST_VERGE >= super::WALL_INSET);
const _: () = assert!(GRASS_OF_VERGE * LEAST_VERGE >= markers::ROOM);
/// One kerb stripe and the start/finish paint, in stations. [`ribbon::STEP`] is
/// the station spacing, so a stripe is two stations long: about a third of the
/// 2.4 m car.
const STRIPE: usize = 2;
/// Fraction of tarmac grip the kerbs and the grass give back.
pub(super) const KERB_GRIP: f32 = 0.72;
pub(super) const GRASS_GRIP: f32 = 0.38;

/// How far a cross-section reaches from the top of its kerb to the bottom of
/// its verge, on a circuit with room for all of it.
///
/// What a bridge has to be raised by, over and above the air it is meant to
/// leave and the deck that carries it: the lowest thing about the road on top
/// is the outer edge of its verge and the highest thing about the road below is
/// the lip of its kerb, and the clearance a driver sees is between those two
/// rather than between two centrelines.
pub(super) const SECTION_DEEP: f32 =
    GRASS_FALL * GRASS_OF_VERGE * VERGE + SKIRT_FALL * (VERGE - GRASS_OF_VERGE * VERGE);

/// How thick the deck of a bridge is, from the road on top of it to the soffit
/// underneath. The same figure [`super::DECK`] reserves when it decides how far
/// the road has to be lifted, so what is drawn and what was cleared for are one
/// number.
pub(super) const DECK: f32 = super::DECK;
/// Strips the structure under a bridge is made of: a soffit, and a fascia down
/// each side of it. Counted only by the tests; the loft writes them out as it
/// goes.
#[cfg(test)]
pub(super) const UNDER: usize = 3;
/// How far the road has to be off the ground before anything is drawn holding
/// it up.
///
/// A ramp leaves the ground at nothing a station, so without a floor the first
/// strip of structure is a strip of no width and two triangles of no area — a
/// thing that renders as nothing, shades as nothing, and shows up in
/// `no_triangle_of_the_loft_is_degenerate` as a defect, which is what it is.
/// Under five centimetres the road is on its embankment and there is nothing to
/// hold up.
const SHOWS: f32 = 0.05;

/// How deep the structure under a station is: a deck's worth over the span of a
/// bridge, tapering out with its ramps, and nothing at all where the taper has
/// got thin enough that a strip of it would be a strip of nothing.
fn deep(station: &Station) -> f32 {
    let deep = DECK * station.deck;
    if deep <= SHOWS { 0.0 } else { deep }
}
/// Concrete. Not a [`Band`], because a band is a strip of the sweep and this is
/// not swept — it exists only where the road is off the ground.
const STRUCTURE: (f32, f32, f32) = (0.46, 0.45, 0.43);

/// Ribs in one cross-section, and strips between them.
pub(super) const RIBS: usize = 10;
pub(super) const BANDS: usize = RIBS - 1;

/// One cross-section, left verge to right verge: `(lateral, height, surface)`.
/// `lateral` is metres right of the centreline, `height` is metres above the
/// road surface, and `surface` covers the strip from this rib to the next — so
/// the last rib only contributes its edge.
pub(super) type Section = [(f32, f32, Band); RIBS];

/// The cross-section with `left` and `right` metres of verge beyond the kerb.
///
/// Everything from the kerb inward is the road, and the road is the same
/// everywhere, so only the four ribs outside it move. The two sides are given
/// separately because a circuit that runs back past itself is cramped on one
/// side and not on the other, and taking the smaller of the two for both would
/// throw away the shoulder on the side that has one.
pub(super) fn section(left: f32, right: f32) -> Section {
    let grass = |verge: f32| GRASS_OF_VERGE * verge;
    [
        (
            -(HALF_WIDTH + left),
            KERB_TOP - fall(left, left),
            Band::Skirt,
        ),
        (
            -(HALF_WIDTH + grass(left)),
            KERB_TOP - fall(grass(left), left),
            Band::Grass,
        ),
        (-HALF_WIDTH, KERB_TOP, Band::Kerb),
        (-KERB_INNER, 0.0, Band::Line),
        (-TARMAC_HALF, 0.0, Band::Tarmac),
        (TARMAC_HALF, 0.0, Band::Line),
        (KERB_INNER, 0.0, Band::Kerb),
        (HALF_WIDTH, KERB_TOP, Band::Grass),
        (
            HALF_WIDTH + grass(right),
            KERB_TOP - fall(grass(right), right),
            Band::Skirt,
        ),
        (HALF_WIDTH + right, KERB_TOP - fall(right, right), Band::End),
    ]
}

/// How far a verge `verge` wide has fallen `across` metres out from the kerb.
fn fall(across: f32, verge: f32) -> f32 {
    let grass = GRASS_OF_VERGE * verge;
    if across <= grass {
        GRASS_FALL * across
    } else {
        GRASS_FALL * grass + SKIRT_FALL * (across - grass)
    }
}

/// Height of a cross-section at `lateral`, above the road surface.
fn height_of(ribs: &Section, lateral: f32) -> f32 {
    let at = lateral.clamp(ribs[0].0, ribs[RIBS - 1].0);
    for rib in ribs.windows(2) {
        if at <= rib[1].0 {
            let t = (at - rib[0].0) / (rib[1].0 - rib[0].0);
            return rib[0].1.lerp(rib[1].1, t);
        }
    }
    0.0
}

/// What a strip of the loft is made of. Colour only — every strip is the same
/// surface geometrically.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Band {
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

/// One colour of the circuit's paint, linear for the vertex colour attribute.
/// [`markers`] reads its palette through here too, so there is one place a
/// colour is turned into what the mesh carries.
pub(super) fn paint(r: f32, g: f32, b: f32) -> [f32; 4] {
    let c = Color::srgb(r, g, b).to_linear();
    [c.red, c.green, c.blue, c.alpha]
}

/// The cross-section fitted to one circuit: a verge width either side of the
/// road at every station, and nothing else. The road and the shape of the verge
/// are constants; these are the only numbers a circuit gets to change.
pub(super) struct Profile {
    left: Vec<f32>,
    right: Vec<f32>,
}

impl Profile {
    /// The widest cross-section this circuit can be swept with, station by
    /// station and side by side, up to the full [`VERGE`]. A stretch with no
    /// room for a road and a shoulder is what says no — rather than the mesh
    /// folding.
    ///
    /// Three passes, in this order and for a reason.
    ///
    /// 1. **Fit.** Two bounds at each station and on each side, and the smaller
    ///    wins. The bend: on the inside of a corner, all but [`SPARE`] of the
    ///    radius of curvature *here*, and nothing at all on the outside, where
    ///    offsetting is regular however far it goes. The room:
    ///    [`Ribbon::room`], the largest disc touching the centreline here and
    ///    nothing else, less [`CLEARANCE`]. Both capped at [`VERGE`] of verge,
    ///    which is all the cross-section has to offer.
    /// 2. **Taper.** Sweep both ways round the lap, never widening, so the verge
    ///    changes by at most [`TAPER`] metres across per metre along. Only ever
    ///    narrowing is what keeps this a smoothing rather than a second fit: a
    ///    station held in by the circuit around it stays held in.
    /// 3. **Judge.** What is left has to be a verge everywhere, or this is a
    ///    circuit Todora cannot hold a road on at this scale, and it says where.
    pub(super) fn fit(ribbon: &Ribbon) -> Result<Self, String> {
        let stations = ribbon.stations();
        let n = stations.len();
        // Nothing asks for more room than the full cross-section can use plus
        // the air it keeps around it, so the disc stops growing there and the
        // search stops with it.
        let cap = EDGE + CLEARANCE;
        let fitted = |side: f32| -> Vec<f32> {
            (0..n)
                .map(|i| {
                    let inward = stations[i].curvature * side;
                    let bend = if inward > 1e-7 {
                        (1.0 - SPARE) / inward
                    } else {
                        f32::MAX
                    };
                    let room = ribbon.room(i, side, cap) - CLEARANCE;
                    EDGE.min(bend).min(room) - HALF_WIDTH
                })
                .collect()
        };
        let mut left = fitted(-1.0);
        let mut right = fitted(1.0);
        let step = ribbon.length() / n as f32;
        taper(&mut left, step);
        taper(&mut right, step);

        let worst = left.iter().chain(&right).copied().fold(f32::MAX, f32::min);
        if worst < LEAST_VERGE {
            let at = left
                .iter()
                .chain(&right)
                .position(|&v| v == worst)
                .unwrap_or(0)
                % n;
            return Err(format!(
                "at {:.0} m round the lap it carries {worst:.2} m of verge beside \
                 its {HALF_WIDTH} m half-road, where {LEAST_VERGE:.2} m is the \
                 least a shoulder can be and still hold the car in and carry a \
                 corner marker",
                stations[at].s
            ));
        }
        Ok(Self { left, right })
    }

    /// The cross-section at station `at`.
    pub(super) fn at(&self, at: usize) -> Section {
        section(self.left[at], self.right[at])
    }

    /// The cross-section `t` of the way from station `at` to the next.
    ///
    /// The verge widths are interpolated and the shape is rebuilt from them,
    /// rather than the ribs of the two stations being interpolated directly.
    /// They come to the same thing — every rib is a fixed fraction of the verge
    /// and every height a fixed slope along it — and this way there is one
    /// description of what a cross-section is.
    pub(super) fn between(&self, at: usize, t: f32) -> Section {
        let next = (at + 1) % self.left.len();
        section(
            self.left[at].lerp(self.left[next], t),
            self.right[at].lerp(self.right[next], t),
        )
    }

    /// Height of the cross-section at `lateral`, above the road surface. What
    /// the car stands on: the kerb lip and the fall of the verge, read from the
    /// same ribs the mesh was swept from, at the same place along the lap.
    pub(super) fn height(&self, at: usize, t: f32, lateral: f32) -> f32 {
        height_of(&self.between(at, t), lateral)
    }

    /// How far the cross-section reaches from the centreline here, on the side
    /// `lateral` is. What the wall is set inside of.
    pub(super) fn reach(&self, at: usize, t: f32, lateral: f32) -> f32 {
        let next = (at + 1) % self.left.len();
        let verge = if lateral < 0.0 {
            self.left[at].lerp(self.left[next], t)
        } else {
            self.right[at].lerp(self.right[next], t)
        };
        HALF_WIDTH + verge
    }

    /// How much grass the shoulder has on side `side` here, before the lip it
    /// falls away over. What a corner marker has to stand on.
    pub(super) fn grass(&self, at: usize, t: f32, side: f32) -> f32 {
        GRASS_OF_VERGE * (self.reach(at, t, side) - HALF_WIDTH)
    }

    /// The same cross-section with `by` metres added to one station's left
    /// verge. Test scaffolding: a width-only change, which is the one kind of
    /// change [`super::Track::fingerprint`] used to be unable to see.
    #[cfg(test)]
    pub(super) fn nudged(&self, at: usize, by: f32) -> Self {
        let mut left = self.left.clone();
        left[at] += by;
        Self {
            left,
            right: self.right.clone(),
        }
    }

    /// How many quads of structure this circuit's loft carries under its
    /// bridges. None at all on thirty-nine of the forty.
    #[cfg(test)]
    pub(super) fn structure(&self, ribbon: &Ribbon) -> usize {
        let stations = ribbon.stations();
        let n = stations.len();
        UNDER
            * (0..n)
                .filter(|&i| deep(&stations[i]) > 0.0 && deep(&stations[(i + 1) % n]) > 0.0)
                .count()
    }

    /// The narrowest and widest the cross-section gets on this circuit, either
    /// side. What the report prints, and the one number that used to be the
    /// whole answer — the game itself never asks now, because nothing about the
    /// road is decided by the worst place on the lap any more.
    #[cfg(test)]
    pub(super) fn span(&self) -> (f32, f32) {
        let reach = |v: &f32| HALF_WIDTH + v;
        (
            self.left
                .iter()
                .chain(&self.right)
                .map(reach)
                .fold(f32::MAX, f32::min),
            self.left
                .iter()
                .chain(&self.right)
                .map(reach)
                .fold(0.0f32, f32::max),
        )
    }

    /// Separate the textured road from the painted lines, kerbs and verge.
    /// Both meshes retain the exact loft positions and normals; the finish
    /// stripe stays with the paint, so textures cannot dim its white surface.
    pub(super) fn surfaces(&self, ribbon: &Ribbon) -> (Mesh, Mesh) {
        let mut scenery = self.loft(ribbon);
        let n = ribbon.stations().len();
        let band = self
            .at(0)
            .iter()
            .position(|rib| rib.2 == Band::Tarmac)
            .unwrap();
        let first = (band * n + STRIPE) * 4;
        let end = (band + 1) * n * 4;
        let positions = scenery
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap()[first..end]
            .to_vec();
        let normals = scenery
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .unwrap()
            .as_float3()
            .unwrap()[first..end]
            .to_vec();
        let uvs: Vec<_> = positions.iter().map(|p| [p[0] / 0.8, p[2] / 0.8]).collect();
        let Some(Indices::U32(indices)) = scenery.indices_mut() else {
            unreachable!("loft uses u32 indices");
        };
        let road_indices: Vec<_> = indices
            .drain((band * n + STRIPE) * 6..(band + 1) * n * 6)
            .map(|i| i - first as u32)
            .collect();
        let road = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(road_indices));
        (scenery, road)
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
    /// Every strip is swept from the one cross-section fitted at each station,
    /// so neighbouring strips still share their edge vertices exactly however
    /// much the verge is changing width — the seam is the same rib read twice,
    /// not two ribs that agree.
    ///
    /// The ring-per-station, quad-between-rings shape follows `bevy_more_shapes`'
    /// tube loft, with the frame locked to world up instead of Frenet-Serret — a
    /// road must not roll with the curve's torsion.
    pub(super) fn loft(&self, ribbon: &Ribbon) -> Mesh {
        let stations = ribbon.stations();
        let n = stations.len();
        let sections: Vec<Section> = (0..n).map(|i| self.at(i)).collect();
        let mut positions = Vec::with_capacity(n * BANDS * 4);
        let mut normals = Vec::with_capacity(n * BANDS * 4);
        let mut colors = Vec::with_capacity(n * BANDS * 4);
        let mut indices = Vec::with_capacity(n * BANDS * 6);

        for band in 0..BANDS {
            let rim: Vec<[Vec3; 2]> = stations
                .iter()
                .zip(&sections)
                .map(|(station, ribs)| {
                    let edge = |rib: (f32, f32, Band)| {
                        station.pos + station.right * rib.0 + Vec3::Y * rib.1
                    };
                    [edge(ribs[band]), edge(ribs[band + 1])]
                })
                .collect();
            let rim_normal = |i: usize, side: usize| {
                let across = rim[i][1] - rim[i][0];
                let along = rim[(i + 1) % n][side] - rim[(i + n - 1) % n][side];
                across.cross(along).normalize_or(Vec3::Y).to_array()
            };

            for (i, ribs) in sections.iter().enumerate() {
                let j = (i + 1) % n;
                let color = ribs[band].2.paint(i);
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

        // What holds a bridge up, where there is one. Not part of the sweep:
        // the sweep makes one surface seen from above, and this is the only
        // thing in the mesh that is meant to be seen from below. Drawn only
        // where the road was lifted off the ground to get over something — see
        // `Station::lifted` — and as deep there as the road was lifted, up to
        // the thickness of a deck, so that a bridge comes out of its own
        // embankment rather than beginning in mid-air.
        //
        // Three strips: a soffit across the underside, and a fascia down each
        // side from the edge of the road to meet it. Between them they close
        // the one place a one-sided road would be seen through, which is from
        // underneath a bridge, which is exactly where a driver on the lower
        // road is looking.
        let under = paint(STRUCTURE.0, STRUCTURE.1, STRUCTURE.2);
        for i in 0..n {
            let j = (i + 1) % n;
            if deep(&stations[i]) <= 0.0 || deep(&stations[j]) <= 0.0 {
                continue;
            }
            let rim = |at: usize, side: usize| {
                let ribs = &sections[at];
                let rib = if side == 0 { ribs[0] } else { ribs[RIBS - 1] };
                stations[at].pos + stations[at].right * rib.0 + Vec3::Y * rib.1
            };
            let sink = |at: usize, side: usize| rim(at, side) - Vec3::Y * deep(&stations[at]);
            // Each quad is given in the same order the swept ones are — this
            // station's two corners, then the next station's — and which way it
            // ends up facing is which way round those two corners go. The
            // soffit is the road's own order reversed, so it looks down where
            // the road looks up; a fascia runs from the road's edge down to the
            // soffit, so it looks out.
            for corners in [
                [sink(i, 1), sink(i, 0), sink(j, 1), sink(j, 0)],
                [sink(i, 0), rim(i, 0), sink(j, 0), rim(j, 0)],
                [rim(i, 1), sink(i, 1), rim(j, 1), sink(j, 1)],
            ] {
                let normal = (corners[1] - corners[0])
                    .cross(corners[2] - corners[0])
                    .normalize_or(Vec3::Y)
                    .to_array();
                for corner in corners {
                    positions.push(corner.to_array());
                    normals.push(normal);
                    colors.push(under);
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

/// Hold the verge to [`TAPER`] metres of change per metre along the lap, by
/// narrowing whichever end of a step is the wider. Twice round each way,
/// because the lap is closed and a run of narrowing started near the end of the
/// array has to be allowed to carry on past the start of it.
fn taper(verge: &mut [f32], step: f32) {
    let n = verge.len();
    let most = TAPER * step;
    for _ in 0..2 {
        for i in 0..n {
            let next = (i + 1) % n;
            verge[next] = verge[next].min(verge[i] + most);
        }
        for i in (0..n).rev() {
            let next = (i + 1) % n;
            verge[i] = verge[i].min(verge[next] + most);
        }
    }
}

/// Fraction of tarmac grip at `lateral` metres off the centreline. The road is
/// the same on every circuit and at every point of one, so this needs no
/// profile to read it from: a fitted verge moves where the grass *ends*, never
/// where it starts.
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

    /// Every circuit, so a new one has to clear the same bar as the old ones.
    fn every_track() -> impl Iterator<Item = (&'static str, Track)> {
        circuits::all()
            .iter()
            .map(|circuit| (circuit.name, Track::new(circuit)))
    }

    #[test]
    fn textured_road_preserves_every_original_face() {
        for (name, track) in every_track() {
            let original = track.profile.loft(&track.ribbon);
            let (scenery, road) = track.profile.surfaces(&track.ribbon);
            let faces = |mesh: &Mesh| {
                let positions = mesh
                    .attribute(Mesh::ATTRIBUTE_POSITION)
                    .unwrap()
                    .as_float3()
                    .unwrap();
                mesh.indices()
                    .unwrap()
                    .iter()
                    .map(|i| positions[i])
                    .collect::<Vec<_>>()
            };
            let mut combined = faces(&scenery);
            let n = track.ribbon.stations().len();
            let band = track
                .profile
                .at(0)
                .iter()
                .position(|rib| rib.2 == Band::Tarmac)
                .unwrap();
            combined.splice(
                (band * n + STRIPE) * 6..(band * n + STRIPE) * 6,
                faces(&road),
            );
            assert_eq!(
                combined,
                faces(&original),
                "{name}: texture changed the surface"
            );
            assert!(road.attribute(Mesh::ATTRIBUTE_COLOR).is_none());
            assert_eq!(
                road.attribute(Mesh::ATTRIBUTE_UV_0).unwrap().len(),
                road.count_vertices()
            );
        }
    }

    /// A cross-section is a cross-section however wide its two verges are:
    /// ordered outward, road untouched, verge reaching exactly as far as asked,
    /// and falling at the slopes that describe it.
    ///
    /// Asymmetric widths on purpose, including the extreme of one side at its
    /// widest and the other at its narrowest, which is the shape the old single
    /// `edge` could not express at all.
    #[test]
    fn a_section_is_a_cross_section_at_any_width() {
        for left in [VERGE, 2.1, LEAST_VERGE] {
            for right in [VERGE, 0.9, LEAST_VERGE] {
                let ribs = section(left, right);
                assert_eq!(ribs[0].0, -(HALF_WIDTH + left));
                assert_eq!(ribs[RIBS - 1].0, HALF_WIDTH + right);
                assert_eq!(ribs[RIBS - 1].2, Band::End);
                for pair in ribs.windows(2) {
                    assert!(
                        pair[1].0 > pair[0].0,
                        "rib {} does not come after {} at {left}/{right}",
                        pair[1].0,
                        pair[0].0
                    );
                }
                // The road is the road: the six ribs inside the kerb lip are
                // the same six on every circuit and at every station.
                let full = section(VERGE, VERGE);
                for k in 2..=7 {
                    assert_eq!(ribs[k], full[k], "the road moved at {left}/{right}");
                }
                // And the verge falls at its slopes rather than to a height.
                for (verge, side) in [(left, -1.0f32), (right, 1.0)] {
                    let lip = height_of(&ribs, side * (HALF_WIDTH + GRASS_OF_VERGE * verge));
                    let edge = height_of(&ribs, side * (HALF_WIDTH + verge));
                    let grass = GRASS_OF_VERGE * verge;
                    assert!((lip - (KERB_TOP - GRASS_FALL * grass)).abs() < 1e-5);
                    assert!(
                        (edge - (KERB_TOP - GRASS_FALL * grass - SKIRT_FALL * (verge - grass)))
                            .abs()
                            < 1e-5
                    );
                }
            }
        }
    }

    /// The cross-section a circuit with room gets, rib by rib and by name.
    ///
    /// Written out rather than derived, because everything else in this module
    /// derives it and a table that checks its own derivation checks nothing.
    /// This is the road: 3.6 m across the outsides of the kerbs, of which
    /// 2.94 m is asphalt, a white line 0.08 m wide either side of that, and a
    /// kerb 0.25 m wide outside each line standing 5 cm proud. Then 1.5 metres
    /// of verge, grass for the first metre and a lip for the last half, arriving
    /// at −0.45 m.
    ///
    /// That the asphalt holds the whole car and the kerb holds a wheel — the
    /// reason the road was cut up this way rather than multiplied down from the
    /// old one — is said at compile time, beside the widths themselves.
    #[test]
    fn a_full_verge_is_the_section_it_says_it_is() {
        let was: &[(f32, f32, Band)] = &[
            (-3.30000, -0.45000, Band::Skirt),
            (-2.80000, -0.07500, Band::Grass),
            (-1.80000, 0.05, Band::Kerb),
            (-1.55000, 0.00, Band::Line),
            (-1.47000, 0.00, Band::Tarmac),
            (1.47000, 0.00, Band::Line),
            (1.55000, 0.00, Band::Kerb),
            (1.80000, 0.05, Band::Grass),
            (2.80000, -0.07500, Band::Skirt),
            (3.30000, -0.45000, Band::End),
        ];
        let now = section(VERGE, VERGE);
        assert_eq!(now.len(), was.len());
        for (rib, was) in now.iter().zip(was) {
            assert!((rib.0 - was.0).abs() < 1e-4, "{} against {}", rib.0, was.0);
            assert!((rib.1 - was.1).abs() < 1e-4, "{} against {}", rib.1, was.1);
            assert_eq!(rib.2, was.2);
        }
        // Symmetric when both sides are given the same room, which is the one
        // thing the old table got for free by being written down once.
        for (a, b) in now.iter().zip(now.iter().rev()) {
            assert_eq!(a.0, -b.0);
            assert_eq!(a.1, b.1);
        }
    }

    /// The whole reason the loft needs no clamping, for every circuit in the
    /// game. Widen the road, or drop in a circuit with nowhere to put a
    /// shoulder, and this says so.
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
            let (narrowest, widest) = track.profile.span();
            assert!(
                narrowest >= HALF_WIDTH + LEAST_VERGE,
                "{}: {narrowest:.2} m of cross-section at its narrowest",
                circuit.name
            );
            assert!(
                widest <= EDGE,
                "{}: {widest:.2} m of cross-section at its widest",
                circuit.name
            );
        }
    }

    /// Stated directly, station by station and side by side, on every circuit:
    /// no rib of the swept profile ever reaches its own centre of curvature, so
    /// no strip can fold back on itself.
    ///
    /// The margin is [`SPARE`] exactly, and not by coincidence: the bend bound
    /// in the fit is written in terms of this same stored curvature, so a rib
    /// held in by a corner comes out with a jacobian of precisely [`SPARE`] and
    /// every other rib comes out above it. What this catches is a rib that got
    /// past the fit some other way — a verge widened by tapering, a cap raised,
    /// a road made wider than the corners can carry.
    #[test]
    fn every_offset_stays_regular() {
        for circuit in circuits::all() {
            let track = Track::new(circuit);
            let mut tightest = f32::MAX;
            for (i, station) in track.ribbon.stations().iter().enumerate() {
                for rib in track.profile.at(i) {
                    let jacobian = 1.0 - station.curvature * rib.0;
                    tightest = tightest.min(jacobian);
                    assert!(
                        jacobian > SPARE - 1e-3,
                        "{}: rib {} folds at s={} (jacobian {jacobian})",
                        circuit.name,
                        rib.0,
                        station.s
                    );
                }
            }
            assert!(tightest < 1.0, "{}: nothing turns at all", circuit.name);
        }
    }

    /// Where the circuit crowds itself, the verge gives way — and only there,
    /// and only on the side doing the crowding.
    ///
    /// This is the whole of the change stated as a measurement. Paul Ricard
    /// used to carry 6.84 m either side for the whole of its lap because of one
    /// pinch; now it carries the full 7 m nearly everywhere and draws in to
    /// 5.82 m at the pinch, on the side the pinch is on. The Red Bull Ring is
    /// the same story with a smaller number.
    #[test]
    fn the_verge_gives_way_where_the_circuit_crowds() {
        let mut anywhere = false;
        for (name, track) in every_track() {
            let n = track.ribbon.stations().len();
            let profile = &track.profile;
            let (narrowest, widest) = profile.span();
            // Apply the same centimetre tolerance as the asymmetry check:
            // a 3 mm trim cannot produce a 10 mm difference between sides.
            if widest - narrowest <= 0.01 {
                continue;
            }
            anywhere = true;
            // Narrow somewhere and full somewhere else: a circuit does not pay
            // for its pinch all the way round.
            assert_eq!(widest, EDGE, "{name} is nowhere at its full width");
            // And the two sides are allowed to disagree, which is the point of
            // fitting them apart.
            let asymmetric = (0..n).any(|i| {
                let ribs = profile.at(i);
                (ribs[0].0 + ribs[RIBS - 1].0).abs() > 0.01
            });
            assert!(
                asymmetric,
                "{name} narrows both sides together, so nothing was gained by \
                 fitting them apart"
            );
        }
        assert!(
            anywhere,
            "no circuit in the game has a fitted verge, so nothing here is tested"
        );
    }

    /// A shoulder that appeared and vanished station by station would be a
    /// ragged edge and a wall that jumped under the car. It changes at
    /// [`TAPER`] at the most, and it does it on both sides independently.
    #[test]
    fn the_verge_tapers_into_its_neighbours() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let n = stations.len();
            let step = track.ribbon.length() / n as f32;
            let most = TAPER * step + 1e-4;
            for (i, station) in stations.iter().enumerate() {
                let (here, next) = (track.profile.at(i), track.profile.at((i + 1) % n));
                for k in [0, RIBS - 1] {
                    let change = (next[k].0 - here[k].0).abs();
                    assert!(
                        change <= most,
                        "{name}: the verge moves {change:.3} m across in {step:.2} m \
                         along at s={}, against a taper of {TAPER}",
                        station.s
                    );
                }
            }
        }
    }

    /// Every rib of every cross-section belongs to the station it was swept
    /// from — ask the circuit where that point is and it says *here*.
    ///
    /// This is what "nothing overlaps anything" now means, and it is a stronger
    /// claim than the thing it replaces. The old fit narrowed every verge to
    /// half the closest the circuit came to itself, skipping any two stretches
    /// within 60 m of each other along the lap; that both cost the whole lap
    /// for one pinch and quietly excused two genuinely separate turns that
    /// happened to be 50 m apart round the circuit. Neither is assumed here.
    /// If any part of the swept surface grew over any other part, the lookup
    /// would find the other one, and it is asked directly.
    ///
    /// Asked at the seams as well as at the stations, because the surface is a
    /// quad between two stations and the middle of one is where two cross-
    /// sections of different widths are furthest from agreeing.
    #[test]
    fn every_rib_belongs_to_the_station_it_was_swept_from() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let n = stations.len();
            let mut worst = 0.0f32;
            for i in 0..n {
                for t in [0.0f32, 0.5] {
                    let station = &stations[i];
                    let next = &stations[(i + 1) % n];
                    let along = station.pos.lerp(next.pos, t);
                    let across = station
                        .right
                        .lerp(next.right, t)
                        .normalize_or(station.right);
                    for rib in track.profile.between(i, t) {
                        // The rib in the world, height and all. The height is
                        // what tells a bridge from the road under it, and the
                        // plan cannot: at a crossing the two are the same point
                        // of the map on purpose.
                        let at = along + across * rib.0 + Vec3::Y * rib.1;
                        let found = track.fix(at, None);
                        worst = worst.max((found.lateral - rib.0).abs());
                        assert!(
                            (found.lateral - rib.0).abs() < 0.05,
                            "{name}: the rib {:.2} m out at s={:.0} reads as {:.2} m \
                             out at s={:.0} — some other part of the circuit has \
                             grown over it",
                            rib.0,
                            station.s,
                            found.lateral,
                            found.s
                        );
                    }
                }
            }
            assert!(worst.is_finite());
        }
    }

    /// The triangles the loft actually makes are all wound the same way and
    /// none of them is degenerate.
    ///
    /// Winding is held over the whole mesh by `loft_faces_up`; this is the other
    /// half, which that one cannot see — a triangle of no area has a normal of
    /// no length, passes every test about which way it points, and renders as
    /// nothing. A verge narrowing to a line would make a strip of them.
    #[test]
    fn no_triangle_of_the_loft_is_degenerate() {
        let over = Track::new(super::super::figure_of_eight(0.0, 0.75, 1.6));
        for (name, track) in every_track().chain([("a figure of eight", over)]) {
            let mesh = track.profile.loft(&track.ribbon);
            let positions = mesh
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .expect("the loft has positions")
                .as_float3()
                .expect("positions are float3");
            let Some(Indices::U32(indices)) = mesh.indices() else {
                panic!("loft lost its indices");
            };
            let mut smallest = f32::MAX;
            for face in indices.chunks_exact(3) {
                let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(positions[face[k] as usize]));
                let area = (b - a).cross(c - a).length() / 2.0;
                smallest = smallest.min(area);
            }
            assert!(
                smallest > 1e-5,
                "{name}: a triangle of the loft has an area of {smallest}"
            );
        }
    }

    /// What the car stands on is the surface that was drawn, to within a
    /// centimetre — and the centimetre is the point.
    ///
    /// [`Profile::height`] reads the cross-section interpolated between two
    /// stations, which is a bilinear surface over the quad. The mesh draws two
    /// triangles. Those are not the same surface where the quad is twisted, and
    /// a quad *is* twisted wherever the verge is changing width: its four
    /// corners no longer lie in a plane. So this does not assert they are
    /// equal. It measures the gap, at the middle of every quad, which is where
    /// a bilinear patch is furthest from either triangle through its corners,
    /// and holds it under the depth of the kerb lip — the smallest feature the
    /// car is meant to feel.
    #[test]
    fn the_ground_is_the_surface_that_was_drawn() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let n = stations.len();
            let mut worst = 0.0f32;
            for i in 0..n {
                let (here, next) = (track.profile.at(i), track.profile.at((i + 1) % n));
                for band in 0..BANDS {
                    // The middle of the quad, in the parameters the sweep uses.
                    let middle = |ribs: &Section| (ribs[band].0 + ribs[band + 1].0) / 2.0;
                    let lateral = (middle(&here) + middle(&next)) / 2.0;
                    let bilinear = track.profile.height(i, 0.5, lateral);
                    // The same point off the two triangles the quad is drawn
                    // as: the diagonal runs from this station's outer corner to
                    // the next station's inner one, so the middle sits on it
                    // and both triangles answer there.
                    let corner = |ribs: &Section, side: usize| ribs[band + side].1;
                    let diagonal = (corner(&here, 1) + corner(&next, 0)) / 2.0;
                    worst = worst.max((bilinear - diagonal).abs());
                }
            }
            assert!(
                worst < KERB_TOP,
                "{name}: the height the car is given and the height that was \
                 drawn are {worst:.4} m apart, which is more than the kerb is proud"
            );
        }
    }

    /// What holds a bridge up faces away from the road, and stays out of the
    /// air the bridge was built to leave.
    ///
    /// The other half of `loft_faces_up`, for the one part of the mesh that is
    /// meant to be seen from below. Every face of the structure points out of
    /// the solid it is a face of: the soffit down, the two fascias out to their
    /// own sides. A fascia wound the other way would be a bridge you could see
    /// into from beside it, and a soffit wound the other way would be a bridge
    /// that was not there at all when you drove under it, which is the thing
    /// this whole strip exists to prevent.
    ///
    /// And it takes nothing from the clearance: the soffit is where the deck
    /// was reserved to be, [`DECK`] under the road and no further, so the air
    /// measured in `a_bridge_has_the_air_under_it_that_was_asked_for` is air the
    /// structure does not come down into.
    #[test]
    fn what_holds_a_bridge_up_faces_away_from_the_road() {
        let circuit = super::super::figure_of_eight(0.0, 0.75, 1.6);
        let track = Track::new(circuit);
        let quads = track.profile.structure(&track.ribbon);
        assert!(quads > 0, "the bridge has nothing holding it up");
        assert_eq!(quads % UNDER, 0, "a soffit without both of its fascias");

        let mesh = track.profile.loft(&track.ribbon);
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .expect("the loft has positions")
            .as_float3()
            .expect("positions are float3");
        let Some(Indices::U32(indices)) = mesh.indices() else {
            panic!("loft lost its indices");
        };
        let from = indices.len() - quads * 6;
        let mut soffits = 0;
        let mut sides = 0;
        for face in indices[from..].chunks_exact(3) {
            let [a, b, c] = [0, 1, 2].map(|k| Vec3::from(positions[face[k] as usize]));
            let normal = (b - a).cross(c - a);
            assert!(
                normal.length_squared() > 1e-10,
                "a face of the structure has no area"
            );
            let normal = normal.normalize();
            // Out of the road: either down, or sideways. Never up, which is
            // where the road already is.
            assert!(
                normal.y < 0.01,
                "a face of the structure at {a} points upward"
            );
            if normal.y < -0.9 {
                soffits += 1;
            } else {
                sides += 1;
                assert!(
                    normal.y.abs() < 0.5,
                    "a fascia at {a} lies down rather than standing up"
                );
            }
        }
        assert_eq!(soffits, quads / UNDER * 2, "the soffit is not flat");
        assert_eq!(sides, quads / UNDER * 4, "there are not two fascias");

        // The structure hangs from the road by exactly what was reserved for
        // it, which is what makes the clearance measured elsewhere the
        // clearance a driver gets. The lowest thing about a bridge is the
        // soffit under the outer edge of its verge; `a_bridge_has_the_air_under
        // _it_that_was_asked_for` measures to that same point, because it
        // measures from every rib and takes the worst.
        let stations = track.ribbon.stations();
        let mut lifted = 0;
        for (i, station) in stations.iter().enumerate() {
            if deep(station) <= 0.0 {
                assert!(
                    DECK * station.deck <= SHOWS,
                    "a station {:.2} m off the ground has nothing holding it up",
                    station.deck
                );
                continue;
            }
            lifted += 1;
            assert!(
                deep(station) <= DECK + 1e-6,
                "the deck is {:.2} m thick where the clearance reserved {DECK}",
                deep(station)
            );
            let ribs = track.profile.at(i);
            let soffit = ribs[0].1.min(ribs[RIBS - 1].1) - deep(station);
            assert!(
                soffit >= -(SECTION_DEEP + DECK) - 1e-6,
                "the soffit hangs {soffit:.2} m under the centreline, past the \
                 {:.2} m the rise allowed for it",
                SECTION_DEEP + DECK
            );
        }
        assert!(lifted > 40, "only {lifted} stations of bridge");
    }

    /// A closed oval: two straights `gap` apart, joined by half circles. The
    /// shape is written out rather than splined, so it is exactly what it says
    /// and its room is arithmetic — `gap / 2` down the straights, and the same
    /// again as a radius of curvature round the ends.
    fn oval(gap: f32, straight: f32) -> Ribbon {
        let r = gap / 2.0;
        let bend = std::f32::consts::PI * r;
        let round = 2.0 * (straight + bend);
        let count = (round / ribbon::STEP).round() as usize;
        let step = round / count as f32;
        let line = (0..count)
            .map(|k| {
                let d = k as f32 * step;
                let (x, z) = if d < straight {
                    (r, -straight / 2.0 + d)
                } else if d < straight + bend {
                    let a = (d - straight) / r;
                    (r * a.cos(), straight / 2.0 + r * a.sin())
                } else if d < 2.0 * straight + bend {
                    (-r, straight / 2.0 - (d - straight - bend))
                } else {
                    let a = (d - 2.0 * straight - bend) / r;
                    (-r * a.cos(), -straight / 2.0 - r * a.sin())
                };
                Vec3::new(x, 0.0, z)
            })
            .collect();
        Ribbon::from_polyline(line)
    }

    /// A shoulder has to be somewhere the car can be held and a corner can be
    /// marked, and a circuit with nowhere for one is still refused.
    ///
    /// This is the negative half of a property correction. The floor used to be
    /// a flat 1.5 m of verge, and 1.5 m was not measuring anything: it is a
    /// pleasant amount of grass, and a stretch of circuit is not unbuildable
    /// for having 1.4. What it cost was real — it is the reason none of the six
    /// circuits that fail for want of room could be let in, including three
    /// that have 5 m of envelope at their tightest and the full road everywhere
    /// else. What it bought was nothing that could be stated.
    ///
    /// The floor is now the sum of what has to fit on the shoulder: the wall
    /// the car is held at, and the footprint of a corner marker with its
    /// clearances, on grass that is two thirds of the verge. Lowering it is not
    /// the same as removing it, and this is what says so — an oval whose two
    /// straights run 4.2 m apart has 1.85 m of cross-section to give against a
    /// road that is 1.8 m to the kerb, and it is turned away with the place
    /// named. Widen the same oval to 6 m and it is a circuit.
    #[test]
    fn a_circuit_with_nowhere_to_put_a_shoulder_is_still_refused() {
        let cramped = Profile::fit(&oval(4.2, 120.0));
        let why = cramped
            .err()
            .expect("4.2 m apart is not a road and two verges");
        assert!(why.contains("round the lap"), "{why}");
        assert!(why.contains("verge"), "{why}");

        // The same shape with room for a shoulder, which is what makes the
        // refusal above a bar rather than a wall.
        let roomy = Profile::fit(&oval(6.0, 120.0)).expect("6 m apart is a circuit");
        let (narrowest, widest) = roomy.span();
        assert!(
            narrowest >= HALF_WIDTH + LEAST_VERGE,
            "{narrowest} m at its narrowest"
        );
        // And it is narrowed by the shape rather than let through whole. Both
        // bounds are live on this one oval and they bind in different places:
        // the ends are 6.5 m corners, so the inside of them keeps [`SPARE`] of
        // that radius, and that is the tightest the cross-section gets. Down
        // the straights it is the 13 m gap less the clearance instead, which is
        // wider — see `a_pinch_near_along_the_lap_is_still_a_pinch`.
        let bend = (1.0 - SPARE) * 3.0;
        assert!(
            (narrowest - bend).abs() < 0.05,
            "{narrowest} m at the ends, where a 3 m corner allows {bend:.2}"
        );
        assert!(widest <= EDGE);
    }

    /// Two stretches of circuit that pass close while being near each other
    /// round the lap are still two stretches.
    ///
    /// The other half of the same correction. The fit used to take half the
    /// closest the circuit came to itself, comparing only pairs of stations
    /// more than 60 m apart *along the lap* — everything nearer than that was
    /// assumed to be the same piece of road seen twice. It mostly was, at a 10 m
    /// corner target: a circuit can only double back within 60 m of itself by
    /// turning tightly, and a tight turn was caught by the corner bound
    /// instead. But it was an assumption about how sharply a circuit may bend
    /// standing in for a measurement of how close it comes to itself, and the
    /// two stop agreeing the moment the corner target moves.
    ///
    /// Nothing is excluded now. The room at a station is the largest disc that
    /// touches the centreline there and nothing else, and the road running on
    /// ahead is tangent to that disc rather than inside it, so a station's
    /// neighbours drop out by geometry instead of by a rule. Here is the shape
    /// that used to be exempt: the straights of this oval are 21 m apart round
    /// the lap and 6 m apart across it, and the verge between them is fitted
    /// to the 6.
    #[test]
    fn a_pinch_near_along_the_lap_is_still_a_pinch() {
        let ribbon = oval(6.0, 12.0);
        let apart = 12.0 + std::f32::consts::PI * 3.0;
        assert!(
            apart < 60.0,
            "the straights are {apart:.0} m apart round the lap"
        );
        let profile = Profile::fit(&ribbon).expect("6 m apart is a circuit");

        // The middle of one straight, where there is no corner to blame and the
        // only thing nearby is the other straight. Its `right` points across
        // the oval, so the right-hand verge is the one facing the gap.
        let stations = ribbon.stations();
        let middle = (0..stations.len())
            .filter(|&i| stations[i].pos.x > 0.0)
            .min_by(|&a, &b| stations[a].pos.z.abs().total_cmp(&stations[b].pos.z.abs()))
            .expect("the oval has a right-hand straight");
        assert!(
            stations[middle].right.x < -0.99,
            "the right-hand verge is not the one facing the gap"
        );
        assert!(
            stations[middle].curvature.abs() < 1e-3,
            "the middle of a straight is not straight"
        );
        let facing = profile.reach(middle, 0.0, 1.0);
        assert!(
            (facing - (3.0 - CLEARANCE)).abs() < 0.05,
            "{facing} m of cross-section across a 6 m gap, where {:.2} is what \
             the gap allows — the old fit would have given it the full {EDGE}, \
             because the two straights are near each other round the lap",
            3.0 - CLEARANCE
        );
        // And the side facing outward, where there is nothing at all, keeps
        // everything the cross-section has.
        assert_eq!(profile.reach(middle, 0.0, -1.0), EDGE);
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
            // A sanity range rather than a design target: the circuits are
            // shrunk alike and come out the length they come out. The top of it
            // is set by Baku, which is 6 km of city street built at two and a
            // half times the shared plan scale because nothing narrower fits
            // between its old-town walls — see `Circuit::plan_scale`.
            assert!(
                (400.0..2100.0).contains(&lap),
                "{} laps {lap} m",
                circuit.name
            );
            let mesh = track.profile.loft(&track.ribbon);
            let verts = mesh.count_vertices();
            // The road and bridge structure; markers have their own meshes.
            let quads =
                track.ribbon.stations().len() * BANDS + track.profile.structure(&track.ribbon);
            assert_eq!(verts, quads * 4);
            let Some(Indices::U32(indices)) = mesh.indices() else {
                panic!("loft lost its indices");
            };
            assert_eq!(indices.len(), quads * 6);
            assert!(indices.iter().all(|&i| (i as usize) < verts));
        }
    }

    /// Every triangle winds the same way round, so the circuit is not visible
    /// from below and invisible from above.
    ///
    /// What it is *not* held over is the structure under a bridge, and that is
    /// not a weakening of it. A soffit is a surface meant to be seen from
    /// below; asking it to face up would be asking it not to exist. It is the
    /// last thing in the mesh and it has a check of its own —
    /// `what_holds_a_bridge_up_faces_away_from_the_road` — which asks of it the
    /// thing this asks of the road: that every face points out of the solid it
    /// is a face of.
    #[test]
    fn loft_faces_up() {
        for circuit in circuits::all()
            .iter()
            .chain([super::super::figure_of_eight(0.0, 0.75, 1.6)])
        {
            let track = Track::new(circuit);
            let mesh = track.profile.loft(&track.ribbon);
            let Some(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
                panic!("loft lost its positions");
            };
            let positions = positions.as_float3().expect("positions are float3");
            let Some(Indices::U32(indices)) = mesh.indices() else {
                panic!("loft lost its indices");
            };
            let road = indices.len() - track.profile.structure(&track.ribbon) * 6;
            for face in indices[..road].chunks_exact(3) {
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
