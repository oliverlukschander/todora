//! The circuit centreline: one closed spline, resampled into evenly spaced stations.
//!
//! This is the only place the shape of the circuit is decided. Every surface in
//! [`super`] is lofted over these stations, so tarmac, kerbs and grass share the
//! same `pos`, the same `right` and the same distance along the lap.

use bevy::math::cubic_splines::{CubicBSpline, CyclicCubicGenerator};
use bevy::prelude::*;

/// Metres between finished stations. Also the resolution of the kerb stripes and
/// of the start/finish paint, since both are coloured per station.
pub(crate) const STEP: f32 = 0.4;
/// Stations per lap are rounded to a multiple of this, so a pattern with a period
/// of one or two stations meets itself at the start/finish line.
const PERIOD: usize = 4;
/// Metres between spline control points. The GPS trace clusters fixes in the
/// corners and spreads them down the straights; a cubic B-spline is parameterised
/// uniformly, so feeding it uneven spacing makes it overshoot and loop. Respacing
/// the control polygon first is what keeps the curve behaved.
const CTRL_STEP: f32 = 6.0;
/// Samples per spline segment, before the arc-length resample.
const SUB: usize = 6;
/// Open tight corners before lofting. This shrinks with the plan to preserve
/// chicanes and hairpins; the local profile fit still checks every road edge
/// against curvature and neighbouring sections before accepting a circuit.
pub(crate) const MIN_RADIUS: f32 = 4.5;
/// Rise over run, after smoothing. The DEM heights are quantised to whole metres
/// and the plan is scaled far harder than the elevation, so a single 1 m DEM step
/// otherwise lands inside one station and reads as a wall.
///
/// This is a backstop, not a shape. At 0.12 it was doing the shaping: near half
/// of Spielberg and well over half of Spa came out pinned to it, so every hill
/// on both circuits was the same ramp and the only thing that varied was how
/// long it went on for. Eau Rouge in particular had neither a dip to drop into
/// nor a climb to haul out of — the plunge from La Source, the compression and
/// the run up to Raidillon were all one constant 12%. At 0.20 the DEM does the
/// shaping again: Eau Rouge steepens through the compression the way it should
/// (4%, 8%, 12%, 16%, 20%), a fifth of Spa touches the cap instead of three
/// fifths, and a long descent now carries the car past its own top speed,
/// because the engine stops pushing there and gravity does not.
pub(crate) const MAX_GRADE: f32 = 0.20;
/// Arc length the height profile is averaged over before the grade cap.
const SMOOTH_SPAN: f32 = 24.0;
/// Arc length the mean line is taken over. Shorter than this is a corner and
/// gets exaggerated; longer than this is the layout of the circuit and is left
/// where the trace put it, so Spa is still Spa rather than Spa with the corners
/// somewhere else.
const CORNER_SPAN: f32 = 45.0;
/// Corner-opening sweeps. Each pass relaxes the tight stations and then respaces
/// them; respacing shifts the curvature a little, so the two alternate until the
/// circuit settles and the loop stops early. Spielberg takes a few hundred and
/// about 30 ms; the bound is only there so a circuit that cannot be opened fails
/// the profile check rather than spinning.
const RELAX_PASSES: usize = 1200;
const RELAX_STEPS: usize = 12;
/// How far a too-tight station moves toward its neighbours' midpoint per step.
const RELAX_RATE: f32 = 0.5;
/// Segments in a leaf of [`Index`]. Descending costs a box test per node, and
/// under about this many segments that test costs more than the projections it
/// saves.
const LEAF: usize = 8;

/// A place where the circuit passes over itself, in the ribbon's own units.
///
/// One closed centreline carrying one road can still have two roads at a point
/// of the map, so long as they are at different heights. That is the whole of
/// what a bridge is here: no second ribbon and no second surface, just a
/// stretch of the one road lifted over another stretch of the same road, with
/// the lookup taught to ask which of the two the car is on.
///
/// The two stretches are found rather than named. `at` is where they cross in
/// plan and `over` is how far round the lap the one that goes over is, as a
/// fraction — both properties of the layout, which survive the spline, the
/// corner opening and the smoothing. A station number would not: it moves
/// whenever anything upstream of it does.
pub struct Overpass {
    /// Where the two stretches cross, in plan. Height ignored.
    pub at: Vec3,
    /// How far round the lap the stretch that goes over is, as a fraction of
    /// the lap. It only has to be nearer the right stretch than the wrong one,
    /// and the two are most of a lap apart.
    pub over: f32,
    /// How far the deck is lifted above what the elevation model gave it.
    pub rise: f32,
    /// Metres of road held level at the top of the lift, either side of the
    /// crossing, and metres of ramp beyond that.
    pub deck: f32,
    pub ramp: f32,
}

/// How near the crossing point a station has to be to belong to one of the two
/// stretches that meet there. Wide enough to catch both sides of the widest
/// cross-section and the road either side of it, narrow enough that a third
/// stretch of circuit is not swept up with them.
pub(crate) const AT_CROSSING: f32 = 12.0;
/// How far from a declared crossing the two-storey exemption below applies.
///
/// Tight on purpose. Away from a bridge, two roads at one point of the map is
/// the thing the fit exists to prevent, and it prevents it in plan because that
/// is where the lookup asks: Miami has two stretches nearly two metres apart in
/// height and a few metres apart in plan, and nothing about that is a bridge —
/// a rib of the lower one has to belong to the lower one. So the exemption is
/// not "different heights are different roads", which would quietly unfit half
/// the circuits in the game. It is "here, where a bridge was declared,
/// different heights are different roads".
const NEAR_A_CROSSING: f32 = 20.0;
/// How far apart in height two stretches of circuit have to be before neither
/// can reach the other, whatever their plans do.
///
/// The cross-section is 0.62 m deep from the kerb's lip to the bottom of the
/// verge, so two roads further apart than that cannot touch even where one is
/// directly above the other. Twice that, and then some, because it is also the
/// figure [`Ribbon::room`] uses to decide that a stretch of circuit is not in
/// its way — and a fit is a claim about clear air, not about the absence of a
/// collision. Every bridge the game builds rises by more than this; that is
/// checked rather than assumed.
pub(crate) const CLEAR_ABOVE: f32 = 1.5;

/// One cross-section of the circuit.
#[derive(Clone, Copy)]
pub struct Station {
    /// Centreline point, carrying the circuit's elevation.
    pub pos: Vec3,
    /// Unit heading, level in XZ. The loft never banks, so the cross-section
    /// stays horizontal and only `pos.y` carries the grade.
    pub tangent: Vec3,
    /// Unit vector 90° to the right of `tangent`.
    pub right: Vec3,
    /// Plan distance from the start/finish line.
    pub s: f32,
    /// Signed curvature, positive where the circuit turns toward `right`.
    /// A rib `lateral` metres off the centreline is a regular offset only while
    /// `1 - curvature * lateral > 0`.
    pub curvature: f32,
    /// Rise over run along `tangent`. What gravity pulls against on a climb.
    pub slope: f32,
    /// How thick the structure under this station is: a deck's worth over a
    /// bridge, tapering to nothing at the ends of its ramps, and zero
    /// everywhere else — which is everywhere, on thirty-nine of the forty.
    ///
    /// The road does not care: a bridge is road and is swept like road. What
    /// reads this is the structure under it, which is drawn only where there is
    /// something to hold up. See `Profile::loft`.
    ///
    /// It is set over the declared span of a bridge whether or not the heights
    /// had to be moved to build one. They often do not: the elevation model
    /// reads Suzuka's back straight six metres above the road it crosses, which
    /// is more than the clearance asks for, so nothing is raised there and
    /// there is still a bridge to draw the underside of.
    pub deck: f32,
}

/// The closed centreline, ready to loft.
pub struct Ribbon {
    stations: Vec<Station>,
    /// Where this circuit passes over itself, in plan. Empty for all but one.
    crossings: Vec<Vec3>,
    index: Index,
    length: f32,
    kept: f32,
    /// How much the circuit rose and fell once the heights were smoothed and
    /// before the grade cap shaved anything off it.
    ///
    /// Kept so that the two things that flatten a circuit can be told apart.
    /// They are different mechanisms with different reasons and different
    /// guards — smoothing is there because the elevation model is quantised to
    /// the metre, the cap is there because a step in it would read as a wall —
    /// and a test that measures their sum can only ever say that one of them
    /// did something.
    #[cfg(test)]
    smoothed_relief: f32,
}

/// Where a world position sits relative to the circuit.
pub struct Fix {
    /// The segment the nearest point is on, and how far along it — `0` at
    /// station `at`, `1` at the one after.
    ///
    /// Carried so that everything reading the cross-section reads it at the
    /// same place. The road is the same everywhere, but the verge is not: it is
    /// fitted station by station, so a height, a wall and a marker footprint
    /// are only the same surface if they are all taken from this one answer
    /// rather than each going and finding its own.
    pub at: usize,
    pub t: f32,
    /// Nearest point on the centreline, at circuit elevation.
    pub point: Vec3,
    /// Unit heading of the circuit here, level in XZ.
    pub tangent: Vec3,
    pub right: Vec3,
    /// Metres right of the centreline; negative is left.
    pub lateral: f32,
    /// Rise over run along `tangent`.
    pub slope: f32,
    /// Signed curvature; its reciprocal is the radius of the corner.
    pub curvature: f32,
    /// Plan distance of `point` from the start/finish line.
    pub s: f32,
}

impl Ribbon {
    /// Build the centreline from raw control points: spline, open the corners
    /// that are too tight to loft, then settle the elevation.
    pub fn new(control: &[Vec3], corners: f32, over: &[Overpass]) -> Self {
        let mut line = resample(&spline(control), STEP);
        exaggerate_corners(&mut line, corners);
        // Measured after exaggerating, because `kept` is about what opening the
        // corners costs, and exaggerating is what gives it something to open.
        let before = closed_length(&line);
        for _ in 0..RELAX_PASSES {
            let mut moved = false;
            for _ in 0..RELAX_STEPS {
                if !open_corners(&mut line) {
                    break;
                }
                moved = true;
            }
            line = resample(&line, STEP);
            if !moved {
                break;
            }
        }
        smooth_heights(&mut line);
        #[cfg(test)]
        let smoothed = relief(&line);
        cap_grade(&mut line);
        // After the cap and not before it. The cap shaves crests down to what
        // the grade allows, so a bridge built before it is a bridge the cap
        // takes straight back off. A bridge built after it has to come with its
        // own promise about grade instead, which is what the ramps are, and
        // `the_bridge_stays_inside_the_grade_cap` is what holds them to it.
        let mut deck = vec![0.0f32; line.len()];
        for pass in over {
            lift(&mut line, &mut deck, pass);
        }
        let mut ribbon = Self::from_polyline(line);
        for (station, thick) in ribbon.stations.iter_mut().zip(deck) {
            station.deck = thick;
        }
        ribbon.crossings = over.iter().map(|pass| flat(pass.at)).collect();
        ribbon.kept = ribbon.length / before.max(1e-4);
        #[cfg(test)]
        {
            ribbon.smoothed_relief = smoothed;
        }
        ribbon
    }

    pub fn stations(&self) -> &[Station] {
        &self.stations
    }

    pub fn crossings(&self) -> &[Vec3] {
        &self.crossings
    }

    /// Plan length of one lap.
    pub fn length(&self) -> f32 {
        self.length
    }

    /// How much of the circuit came through opening its corners, as a fraction
    /// of the spline it was built from.
    ///
    /// Opening a corner cuts it, so every circuit loses a little — the four in
    /// the game keep between four fifths and all of it. A circuit whose corners
    /// are nearly all tighter than [`MIN_RADIUS`] at this scale loses far more
    /// than that: there is nothing left to relax against, and pass after pass
    /// pulls the whole lap toward its own centre until it is a loop with no
    /// corners in it. Monaco shrinks from 3.3 km to a hundred-metre ring that
    /// way, keeping a quarter of itself. Every station on it is perfectly well
    /// formed, so nothing else notices; this does.
    ///
    /// Lowering [`MIN_RADIUS`] for such a circuit does not rescue it, which is
    /// worth knowing before anyone tries: at a 5 m target Monaco keeps 82% and
    /// laps 340 m, and then passes within 3.95 m of itself, which leaves 1.98 m
    /// either side of the centreline for a road that is 8 m wide. The circuit is
    /// not too tight in its corners so much as too tight in its land, and this
    /// game's road is five times too wide for that land already.
    pub fn kept(&self) -> f32 {
        self.kept
    }

    pub fn start(&self) -> &Station {
        &self.stations[0]
    }

    /// The station `back` metres before the start/finish line, the way the lap
    /// runs. The stations are evenly spaced by construction, so this is
    /// arithmetic on the spacing rather than a walk.
    pub fn before_start(&self, back: f32) -> &Station {
        let n = self.stations.len();
        let step = self.length / n as f32;
        let steps = (back / step).round() as usize % n;
        &self.stations[(n - steps) % n]
    }

    /// How much the circuit rose and fell once its heights were smoothed, and
    /// before the grade cap. What the smoothing alone can be blamed for.
    #[cfg(test)]
    pub(crate) fn smoothed_relief(&self) -> f32 {
        self.smoothed_relief
    }

    /// How much the finished circuit rises and falls. What the smoothing *and*
    /// the cap together left.
    #[cfg(test)]
    pub(crate) fn relief(&self) -> f32 {
        let (low, high) = self
            .stations
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, h), s| {
                (l.min(s.pos.y), h.max(s.pos.y))
            });
        high - low
    }

    /// The station `by` metres further along the lap from plan distance `s`.
    /// The stations are evenly spaced by construction, so this is arithmetic on
    /// the spacing rather than a walk, and it wraps at the line like the lap
    /// does.
    pub fn along(&self, s: f32, by: f32) -> &Station {
        let n = self.stations.len();
        let step = self.length / n as f32;
        let at = ((s + by) / step).round().rem_euclid(n as f32);
        &self.stations[at as usize % n]
    }

    /// Tightest corner on the circuit.
    ///
    /// No longer what the cross-section is fitted against — [`Ribbon::room`]
    /// measures the corner *here* rather than the tightest one anywhere — but
    /// still how the corner-opening pass is checked to have converged, and
    /// still what `tools/screen_tracks.py` reads off a candidate before anything
    /// else is built. Hence the allowance: the game itself does not ask.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn min_radius(&self) -> f32 {
        self.stations
            .iter()
            .filter(|s| s.curvature.abs() > 1e-7)
            .fold(f32::MAX, |r, s| r.min(1.0 / s.curvature.abs()))
    }

    /// How far the cross-section may reach from station `i` on side `side`
    /// before the surface it sweeps grows through itself — capped at `cap`,
    /// beyond which nothing asks.
    ///
    /// The largest disc that touches the centreline here and nowhere else.
    /// Grow a circle from the centreline at this station, tangent to the road
    /// and lying on the side in question, and stop when it reaches any other
    /// part of the circuit: everything inside it belongs to this station and to
    /// nothing else, so a rib anywhere inside it is a rib that cannot have come
    /// from somewhere else as well.
    ///
    /// That one measurement is both of the bounds the cross-section used to be
    /// given separately, and it is *local*, which the old pair were not:
    ///
    /// - Where the circuit is turning, the disc runs out of room at the centre
    ///   of the corner, so it measures the radius of curvature *here* rather
    ///   than the tightest corner anywhere on the lap.
    /// - Where the circuit runs back past itself, it runs out of room half way
    ///   across the gap, so it measures the space at *this* pinch rather than
    ///   the closest approach anywhere on the lap.
    ///
    /// And it needs no arbitrary exclusion of nearby stations to do it. The
    /// disc is tangent to the road, so the road running on ahead is tangent to
    /// it too and never inside it: a station's neighbours drop out of the
    /// measurement by geometry rather than by being told to. The old
    /// `min_separation` had to skip everything within 60 m along the lap, which
    /// is also how far apart two genuinely separate turns can be.
    ///
    /// Solved rather than searched. The disc of radius `w` is centred at
    /// `pos + side * w * right`, so a station at `d` from here is outside it
    /// while `|d - side * w * right|² ≥ w²`, which is `|d|² ≥ 2 w (side * d·right)`
    /// — so each station either says nothing, when it is behind the tangent
    /// line, or caps `w` at `|d|² / (2 * side * d·right)`.
    pub fn room(&self, i: usize, side: f32, cap: f32) -> f32 {
        let mut best = cap;
        self.index
            .narrow_room(&self.stations, i, side, self.two_storey(i), &mut best);
        best
    }

    /// Whether station `i` is somewhere the circuit was declared to pass over
    /// itself, and so somewhere a stretch of road at another height is a road
    /// on another deck rather than a road in the way.
    fn two_storey(&self, i: usize) -> bool {
        let here = flat(self.stations[i].pos);
        self.crossings
            .iter()
            .any(|at| (here - *at).length() < NEAR_A_CROSSING)
    }

    /// The same widest disc, measured against every station in turn. The
    /// oracle the indexed answer is held to.
    #[cfg(test)]
    pub(crate) fn room_exhaustively(&self, i: usize, side: f32, cap: f32) -> f32 {
        let here = &self.stations[i];
        let mut best = cap;
        for station in &self.stations {
            if self.two_storey(i) && (station.pos.y - here.pos.y).abs() > CLEAR_ABOVE {
                continue;
            }
            let d = flat(station.pos - here.pos);
            let across = d.dot(here.right) * side;
            if across > 0.0 {
                best = best.min(d.length_squared() / (2.0 * across));
            }
        }
        best
    }

    /// Nearest point on the centreline.
    ///
    /// Asked of every wheel, every skid sample, the camera, the clock and the
    /// ghost, several times per fixed step — so it is asked through [`Index`]
    /// rather than by projecting the car onto all thirteen hundred segments of
    /// the lap. The answer is the walk's answer, tie for tie.
    pub fn locate(&self, pos: Vec3) -> Fix {
        match self.index.nearest(&self.stations, pos) {
            Some((at, t)) => self.fix(pos, at, t),
            // Every segment degenerate, which no built circuit is: the resample
            // spaces them and the relax pass cannot bring two together. Sitting
            // on station zero is the one answer that cannot be wrong about
            // something that does not exist.
            None => Fix {
                at: 0,
                t: 0.0,
                point: self.stations[0].pos,
                tangent: self.stations[0].tangent,
                right: self.stations[0].right,
                lateral: 0.0,
                slope: self.stations[0].slope,
                curvature: self.stations[0].curvature,
                s: 0.0,
            },
        }
    }

    /// Every stretch of circuit within `within` of `pos` in plan, as a fix
    /// apiece, in the order the lap runs.
    ///
    /// One answer per stretch and not per segment: the fifty segments a car
    /// sits over are one piece of road and one answer, and the point of asking
    /// is the day there are two. Where the circuit passes over itself both come
    /// back, and something that knows which deck the car is on picks between
    /// them — see [`super::Track::ground`].
    ///
    /// Always at least one, whatever `within` is: a car out beyond everything
    /// still has a nearest piece of road, and returning nothing would only move
    /// the question somewhere else.
    pub fn nearby(&self, pos: Vec3, within: f32) -> Vec<Fix> {
        let n = self.stations.len();
        let mut found = self.index.within(&self.stations, pos, within);
        if found.is_empty() {
            return vec![self.locate(pos)];
        }
        found.sort_by_key(|&(at, _, _)| at);
        // Runs of consecutive segments are one stretch of road, and the best of
        // each run is that stretch's answer.
        let mut runs: Vec<(usize, f32, f32)> = Vec::new();
        let mut ends: Vec<usize> = Vec::new();
        for &(at, t, d) in &found {
            match (runs.last_mut(), ends.last_mut()) {
                (Some(run), Some(end)) if at == *end + 1 => {
                    *end = at;
                    if d < run.2 {
                        *run = (at, t, d);
                    }
                }
                _ => {
                    runs.push((at, t, d));
                    ends.push(at);
                }
            }
        }
        // A run that wraps past the start/finish line arrives as two, one at
        // each end of the array. It is one stretch of road.
        if runs.len() > 1 && found[0].0 == 0 && ends[ends.len() - 1] == n - 1 {
            let last = runs.pop().expect("more than one run");
            if last.2 < runs[0].2 {
                runs[0] = last;
            }
        }
        runs.into_iter()
            .map(|(at, t, _)| self.fix(pos, at, t))
            .collect()
    }

    /// The same nearest point, found by projecting onto every segment in turn.
    ///
    /// This is what [`Ribbon::locate`] used to be, kept as the oracle the index
    /// is measured against rather than as a comment claiming they agree. It also
    /// fixes what "agree" means where two segments are exactly equally near: the
    /// walk takes the first of them, so the index has to as well, or a car
    /// straddling a seam would sit on one segment under test and the other in
    /// the game.
    #[cfg(test)]
    pub(crate) fn locate_exhaustively(&self, pos: Vec3) -> Fix {
        let n = self.stations.len();
        let mut best = f32::MAX;
        let mut found = None;
        for i in 0..n {
            let a = &self.stations[i];
            let b = &self.stations[(i + 1) % n];
            let ab = flat(b.pos - a.pos);
            let len2 = ab.length_squared();
            if len2 < 1e-9 {
                continue;
            }
            let t = (flat(pos - a.pos).dot(ab) / len2).clamp(0.0, 1.0);
            let d = flat(pos - (a.pos + (b.pos - a.pos) * t)).length_squared();
            if d < best {
                best = d;
                found = Some((i, t));
            }
        }
        match found {
            Some((at, t)) => self.fix(pos, at, t),
            None => self.locate(pos),
        }
    }

    /// How many segments a lookup had to project onto to answer.
    ///
    /// The evidence that the index is worth having, and a count rather than a
    /// clock: it reads the same on a loaded machine as on an idle one, so it can
    /// be asserted on instead of watched. `the_index_looks_at_little_of_the_lap`
    /// is what holds it.
    #[cfg(test)]
    pub(crate) fn projections(&self, pos: Vec3) -> usize {
        let mut counted = 0;
        self.index
            .nearest_counting(&self.stations, pos, &mut counted);
        counted
    }

    /// The fix `t` of the way along segment `at`, as seen from `pos`.
    ///
    /// One place, so the index and the walk cannot differ in how they read a
    /// segment they both chose — only in which segment they chose.
    fn fix(&self, pos: Vec3, at: usize, t: f32) -> Fix {
        let n = self.stations.len();
        let step = self.length / n as f32;
        let a = &self.stations[at];
        let b = &self.stations[(at + 1) % n];
        let point = a.pos + (b.pos - a.pos) * t;
        let right = a.right.lerp(b.right, t).normalize_or(a.right);
        Fix {
            at,
            t,
            point,
            tangent: a.tangent.lerp(b.tangent, t).normalize_or(a.tangent),
            right,
            lateral: flat(pos - point).dot(right),
            slope: a.slope.lerp(b.slope, t),
            curvature: a.curvature.lerp(b.curvature, t),
            s: a.s + step * t,
        }
    }

    /// A ribbon straight from a closed polyline, with no splining, no corner
    /// opening and no smoothing. What [`Ribbon::new`] finishes with, and how a
    /// test builds a shape it wants exactly rather than approximately.
    pub(super) fn from_polyline(line: Vec<Vec3>) -> Self {
        let n = line.len();
        let length = closed_length(&line);
        let step = length / n as f32;
        let stations: Vec<Station> = (0..n)
            .map(|i| {
                let tangent = flat(line[(i + 1) % n] - line[(i + n - 1) % n]).normalize_or(Vec3::X);
                let (behind, ahead) = (line[(i + n - 1) % n], line[(i + 1) % n]);
                Station {
                    pos: line[i],
                    tangent,
                    right: tangent.cross(Vec3::Y).normalize_or(Vec3::Z),
                    s: step * i as f32,
                    curvature: curvature_at(&line, i),
                    slope: (ahead.y - behind.y) / flat(ahead - behind).length().max(1e-4),
                    deck: 0.0,
                }
            })
            .collect();
        Self {
            index: Index::new(&stations),
            stations,
            crossings: Vec::new(),
            length,
            kept: 1.0,
            #[cfg(test)]
            smoothed_relief: 0.0,
        }
    }
}

/// Where the circuit is, indexed.
///
/// Finding what the car is standing on used to mean projecting it onto every
/// segment of the lap — thirteen hundred of them on Spielberg, for one answer
/// about one of them, several times per fixed step. This is the same answer
/// reached by looking: a tree of plan bounding boxes over runs of consecutive
/// segments, descended only into the boxes that could still hold something
/// nearer than the best found so far.
///
/// Runs of *consecutive* segments, rather than a spatial split of them, because
/// a road is a curve: a stretch of one is compact in plan, so its box is tight,
/// and the runs fall out of the lap itself with nothing to sort.
///
/// The nearer child is descended first, which is where nearly all of the saving
/// is — reaching the stretch the car is actually on before opening anything else
/// makes the best distance small immediately, and a small best is what prunes
/// the rest of the lap. Descending in lap order instead leaves the best at
/// infinity until the walk happens to arrive somewhere near, and Albert Park
/// then projects a fifth of itself rather than a fortieth.
///
/// That ordering is no longer what settles a tie, so the tie is settled openly:
/// of two segments exactly as near, the earlier one wins, which is what walking
/// the stations in order did. A box exactly as far as the best is opened rather
/// than pruned, so an earlier tie is never missed. It matters because a car
/// straddling a seam must read the same segment here as it would have before,
/// not the other one.
///
/// Built once, with the ribbon, and never changed — the shape of the circuit is
/// settled by the time there is a station to index. The bounds are plan only,
/// because every distance this answers is measured in plan.
struct Index {
    nodes: Vec<Bounds>,
}

/// One node: the plan bounds of a run of consecutive segments.
struct Bounds {
    min: Vec2,
    max: Vec2,
    /// The first segment in the run, and one past the last. Segment `i` runs
    /// from station `i` to station `i + 1`, the closing one included.
    from: u32,
    to: u32,
    /// The right child. The left is always the node straight after this one, so
    /// only this one has to be written down, and zero marks a leaf — which is
    /// unambiguous, because node zero is the root and is nobody's child.
    right: u32,
}

impl Index {
    /// Every segment whose nearest point is within `within` of `pos` in plan,
    /// as `(segment, how far along it, squared distance)`.
    fn within(&self, stations: &[Station], pos: Vec3, within: f32) -> Vec<(usize, f32, f32)> {
        let mut out = Vec::new();
        self.gather(stations, pos, within * within, 0, &mut out);
        out
    }

    fn gather(
        &self,
        stations: &[Station],
        pos: Vec3,
        reach: f32,
        at: usize,
        out: &mut Vec<(usize, f32, f32)>,
    ) {
        let node = &self.nodes[at];
        if outside(node, pos) > reach {
            return;
        }
        if node.right == 0 {
            let n = stations.len();
            for i in node.from as usize..node.to as usize {
                let (a, b) = (stations[i].pos, stations[(i + 1) % n].pos);
                let ab = flat(b - a);
                let len2 = ab.length_squared();
                if len2 < 1e-9 {
                    continue;
                }
                let t = (flat(pos - a).dot(ab) / len2).clamp(0.0, 1.0);
                let d = flat(pos - (a + (b - a) * t)).length_squared();
                if d <= reach {
                    out.push((i, t, d));
                }
            }
            return;
        }
        self.gather(stations, pos, reach, at + 1, out);
        self.gather(stations, pos, reach, node.right as usize, out);
    }

    /// Index every segment of the closed centreline, the closing one included.
    fn new(stations: &[Station]) -> Self {
        let mut index = Self { nodes: Vec::new() };
        index.push(stations, 0, stations.len());
        index
    }

    /// Write the node for segments `from..to` and everything under it, and say
    /// where it went. Children first, so a node's box is the union of theirs.
    fn push(&mut self, stations: &[Station], from: usize, to: usize) -> usize {
        let at = self.nodes.len();
        self.nodes.push(Bounds {
            min: Vec2::MAX,
            max: Vec2::MIN,
            from: from as u32,
            to: to as u32,
            right: 0,
        });
        if to - from > LEAF {
            let mid = from + (to - from) / 2;
            self.push(stations, from, mid);
            self.nodes[at].right = self.push(stations, mid, to) as u32;
        }
        let n = stations.len();
        let (mut min, mut max) = (Vec2::MAX, Vec2::MIN);
        for i in from..to {
            for end in [stations[i].pos, stations[(i + 1) % n].pos] {
                min = min.min(Vec2::new(end.x, end.z));
                max = max.max(Vec2::new(end.x, end.z));
            }
        }
        self.nodes[at].min = min;
        self.nodes[at].max = max;
        at
    }

    /// The nearest segment to `pos` in plan, and how far along it the nearest
    /// point sits. `None` only if every segment is degenerate.
    fn nearest(&self, stations: &[Station], pos: Vec3) -> Option<(usize, f32)> {
        let mut best = (f32::MAX, None);
        self.descend(stations, pos, 0, &mut best, &mut 0);
        best.1
    }

    #[cfg(test)]
    fn nearest_counting(
        &self,
        stations: &[Station],
        pos: Vec3,
        counted: &mut usize,
    ) -> Option<(usize, f32)> {
        let mut best = (f32::MAX, None);
        self.descend(stations, pos, 0, &mut best, counted);
        best.1
    }

    fn descend(
        &self,
        stations: &[Station],
        pos: Vec3,
        at: usize,
        best: &mut (f32, Option<(usize, f32)>),
        counted: &mut usize,
    ) {
        let node = &self.nodes[at];
        if outside(node, pos) > slack(best.0) {
            return;
        }
        if node.right == 0 {
            let n = stations.len();
            for i in node.from as usize..node.to as usize {
                let (a, b) = (stations[i].pos, stations[(i + 1) % n].pos);
                let ab = flat(b - a);
                let len2 = ab.length_squared();
                if len2 < 1e-9 {
                    continue;
                }
                *counted += 1;
                let t = (flat(pos - a).dot(ab) / len2).clamp(0.0, 1.0);
                let d = flat(pos - (a + (b - a) * t)).length_squared();
                let earlier = best.1.is_some_and(|(was, _)| i < was);
                if d < best.0 || (d == best.0 && earlier) {
                    *best = (d, Some((i, t)));
                }
            }
            return;
        }
        let (left, right) = (at + 1, node.right as usize);
        let (near, far) = if outside(&self.nodes[left], pos) <= outside(&self.nodes[right], pos) {
            (left, right)
        } else {
            (right, left)
        };
        self.descend(stations, pos, near, best, counted);
        self.descend(stations, pos, far, best, counted);
    }

    /// Narrow `best` to the widest disc that touches the centreline at station
    /// `i` on side `side` and nothing else. See [`Ribbon::room`].
    ///
    /// The discs are nested — every one of them touches the centreline at the
    /// same point from the same side, so a smaller radius is a disc inside a
    /// larger one. Pruning against the disc for the best radius so far is
    /// therefore sound, and it tightens as the walk goes on.
    fn narrow_room(
        &self,
        stations: &[Station],
        i: usize,
        side: f32,
        two_storey: bool,
        best: &mut f32,
    ) {
        self.walk_room(stations, i, side, two_storey, 0, best);
    }

    fn walk_room(
        &self,
        stations: &[Station],
        i: usize,
        side: f32,
        two_storey: bool,
        at: usize,
        best: &mut f32,
    ) {
        let here = &stations[i];
        let centre = here.pos + here.right * (side * *best);
        let node = &self.nodes[at];
        if outside(node, centre) > slack(*best * *best) {
            return;
        }
        if node.right == 0 {
            for other in &stations[node.from as usize..node.to as usize] {
                // A stretch of circuit passing over or under this one is not
                // in its way — but only where the circuit was said to do that.
                // This is the whole of what a bridge costs the fit: a plan
                // measure being asked to notice that the map has two storeys,
                // in the one place it has two storeys.
                if two_storey && (other.pos.y - here.pos.y).abs() > CLEAR_ABOVE {
                    continue;
                }
                let d = flat(other.pos - here.pos);
                let across = d.dot(here.right) * side;
                // Behind the tangent line, so outside every one of the discs:
                // this station has nothing to say about how wide they get.
                if across > 0.0 {
                    *best = best.min(d.length_squared() / (2.0 * across));
                }
            }
            return;
        }
        let (left, right) = (at + 1, node.right as usize);
        let (near, far) =
            if outside(&self.nodes[left], centre) <= outside(&self.nodes[right], centre) {
                (left, right)
            } else {
                (right, left)
            };
        self.walk_room(stations, i, side, two_storey, near, best);
        self.walk_room(stations, i, side, two_storey, far, best);
    }
}

/// Squared plan distance from `pos` to a node's box, and zero inside it. A box
/// holds nothing nearer than this, so anything further away than the best found
/// so far can be left unopened.
fn outside(node: &Bounds, pos: Vec3) -> f32 {
    let at = Vec2::new(pos.x, pos.z);
    (at.clamp(node.min, node.max) - at).length_squared()
}

/// How far past the best a box has to be before it is pruned.
///
/// A box distance is a lower bound on what is inside it in exact arithmetic,
/// and in `f32` it is that bound give or take a last place. Pruning has to be
/// conservative about which way that lands, or the index and the walk part
/// company over a rounding error — and the whole worth of the index is that
/// they do not.
fn slack(best: f32) -> f32 {
    best * (1.0 + 1e-6) + 1e-9
}

/// Drop the height: the cross-section and every distance along the lap are
/// measured in plan, so the loft's spacing does not stretch on the climbs.
fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

/// How much a polyline rises and falls.
#[cfg(test)]
fn relief(line: &[Vec3]) -> f32 {
    let (low, high) = line
        .iter()
        .fold((f32::MAX, f32::MIN), |(l, h), p| (l.min(p.y), h.max(p.y)));
    high - low
}

fn closed_length(line: &[Vec3]) -> f32 {
    (0..line.len())
        .map(|i| flat(line[(i + 1) % line.len()] - line[i]).length())
        .sum()
}

/// Respace the control polygon, then run a closed cubic B-spline through it.
/// B-spline rather than Catmull-Rom: it approximates instead of interpolating, so
/// it stays inside the convex hull of the control polygon and cannot cusp, and it
/// rounds the corners the raw trace is too sharp for.
fn spline(control: &[Vec3]) -> Vec<Vec3> {
    let count = (closed_length(control) / CTRL_STEP).round().max(16.0) as usize;
    let points = resample(control, closed_length(control) / count as f32);
    let curve = CubicBSpline::new(points.clone())
        .to_curve_cyclic()
        .expect("the circuit has more than two control points");
    let segments = points.len();
    (0..segments * SUB)
        .map(|i| curve.position(i as f32 / SUB as f32))
        .collect()
}

/// Walk a closed polyline and drop a point every `step` metres of plan distance.
fn resample(line: &[Vec3], step: f32) -> Vec<Vec3> {
    let total = closed_length(line);
    let count = ((total / step / PERIOD as f32).round().max(4.0) as usize) * PERIOD;
    let step = total / count as f32;
    let mut out = Vec::with_capacity(count);
    let mut from = line[0];
    let mut at = 0;
    let mut left = step;
    out.push(from);
    while out.len() < count {
        let to = line[(at + 1) % line.len()];
        let span = flat(to - from).length();
        if span < 1e-9 {
            at += 1;
            from = line[at % line.len()];
            continue;
        }
        if left <= span {
            from += (to - from) * (left / span);
            out.push(from);
            left = step;
        } else {
            left -= span;
            at += 1;
            from = to;
        }
    }
    out
}

/// Signed curvature at station `i`, from the turn between its two chords.
/// Positive where the circuit turns toward `right`.
fn curvature_at(line: &[Vec3], i: usize) -> f32 {
    let n = line.len();
    let before = flat(line[i] - line[(i + n - 1) % n]);
    let after = flat(line[(i + 1) % n] - line[i]);
    let (len_before, len_after) = (before.length(), after.length());
    if len_before < 1e-6 || len_after < 1e-6 {
        return 0.0;
    }
    let (before, after) = (before / len_before, after / len_after);
    let turn = f32::atan2(
        after.dot(before.cross(Vec3::Y)),
        after.dot(before).clamp(-1.0, 1.0),
    );
    turn / (0.5 * (len_before + len_after))
}

/// Nudge every station tighter than [`MIN_RADIUS`] toward the midpoint of its
/// neighbours, in plan only. Touching just the offending stations keeps the rest
/// of the circuit where the GPS trace put it. Reports whether anything moved.
fn open_corners(line: &mut [Vec3]) -> bool {
    let n = line.len();
    let tight: Vec<usize> = (0..n)
        .filter(|&i| {
            let k = curvature_at(line, i).abs();
            k > 1e-7 && 1.0 / k < MIN_RADIUS
        })
        .collect();
    for &i in &tight {
        let mid = (line[(i + n - 1) % n] + line[(i + 1) % n]) * 0.5;
        line[i].x += RELAX_RATE * (mid.x - line[i].x);
        line[i].z += RELAX_RATE * (mid.z - line[i].z);
    }
    !tight.is_empty()
}

/// Push every station away from the circuit's own mean line, in plan.
///
/// A low-pass of the centreline is the layout — where the circuit goes. What is
/// left over is the corners. Amplifying only the leftovers exaggerates a corner
/// without moving the circuit: in curvature terms a long sweep is most of its
/// own mean line, so it comes through unchanged, while a chicane averages away
/// to nothing and comes through multiplied. Straights have nothing to amplify.
///
/// Plan only. The height profile is scaled in [`super`] and smoothed below, and
/// exaggerating it here as well would stack two treatments on one axis.
fn exaggerate_corners(line: &mut [Vec3], corners: f32) {
    let n = line.len();
    let half = ((CORNER_SPAN / STEP) as usize / 2).max(1).min(n / 2);
    let mean: Vec<Vec3> = (0..n)
        .map(|i| {
            (0..=2 * half)
                .map(|j| line[(i + n + j - half) % n])
                .sum::<Vec3>()
                / (2 * half + 1) as f32
        })
        .collect();
    for (point, mean) in line.iter_mut().zip(mean) {
        point.x = mean.x + corners * (point.x - mean.x);
        point.z = mean.z + corners * (point.z - mean.z);
    }
}

/// Average the height profile along the circuit so the quantised DEM stops
/// stair-stepping. The loft carries Y from here; there is no separate terrain
/// grid to disagree with.
fn smooth_heights(line: &mut [Vec3]) {
    let n = line.len();
    let half = ((SMOOTH_SPAN / STEP) as usize / 2).max(1);
    let heights: Vec<f32> = (0..n)
        .map(|i| {
            (0..=2 * half)
                .map(|j| line[(i + n + j - half) % n].y)
                .sum::<f32>()
                / (2 * half + 1) as f32
        })
        .collect();
    for (point, y) in line.iter_mut().zip(heights) {
        point.y = y;
    }
}

/// Carry one stretch of the circuit over another.
///
/// What this has to deliver is a separation, not a height: `pass.rise` between
/// the two centrelines, which is what the clearance under the span comes to
/// once the deck and the depth of a cross-section are taken off it. How much of
/// that the elevation model already gave is not the same question, and it
/// varies — the figure of eight is flat and needs all of it, while Suzuka's
/// model reads the back straight six metres above the road it crosses, which is
/// more than enough on its own. So what is added is the shortfall, and a
/// circuit whose model already separates its two roads is left alone.
///
/// Where a lift is needed, three things happen and the order matters.
///
/// The ground under it is levelled first: between the far ends of the two ramps
/// the height becomes a straight line from one to the other. A bridge is a
/// structure and a structure does not follow the field it crosses — and
/// levelling it is also what makes the grade of the finished thing knowable,
/// because everything the ramps then add is added to a constant slope.
///
/// Then the deck goes up by the shortfall, held level for `deck` metres either
/// side of the crossing, so that the road below passes under a flat span rather
/// than under the point of a roof.
///
/// Then the ramps, which are a raised cosine rather than a wedge so the car
/// meets a slope that starts at nothing and ends at nothing instead of two
/// creases. That costs gradient: the steepest part of a raised cosine is half
/// of π times its average, so a ramp has to be that much longer than a wedge
/// would be for the same rise.
///
/// The structure is written down either way, over the whole span and tapering
/// out with the ramps. A bridge that needed no raising is still a bridge, and
/// still has an underside a driver passes beneath.
fn lift(line: &mut [Vec3], deck: &mut [f32], pass: &Overpass) {
    let n = line.len();
    let step = closed_length(line) / n as f32;
    let Some((middle, under)) = crossing_stations(line, pass) else {
        return;
    };
    let flat_for = (pass.deck / step).round() as usize;
    let ramp = (pass.ramp / step).round().max(1.0) as usize;
    let span = flat_for + ramp;
    let short = (pass.rise - (line[middle].y - line[under].y)).max(0.0);
    // How much of the deck there is to draw, and how much of it has to be put
    // there: the first is the span, the second is the shortfall.
    let along = |d: usize| -> f32 {
        if d <= flat_for {
            1.0
        } else {
            let t = (d - flat_for) as f32 / ramp as f32;
            0.5 * (1.0 + (t * std::f32::consts::PI).cos())
        }
    };
    if short > 0.0 {
        // Level the ground the bridge stands on, end to end.
        let (from, to) = (line[(middle + n - span) % n].y, line[(middle + span) % n].y);
        for d in 0..=2 * span {
            let t = d as f32 / (2 * span) as f32;
            line[(middle + n + d - span) % n].y = from.lerp(to, t);
        }
    }
    // Out from the middle in both directions. The middle itself is one station
    // and is dealt with once.
    for d in 0..=span {
        for at in [(middle + d) % n, (middle + n - d) % n] {
            line[at].y += short * along(d);
            deck[at] = deck[at].max(along(d));
            if d == 0 {
                break;
            }
        }
    }
}

/// The middles of the two stretches that meet at a crossing: the one that goes
/// over, and the one that goes under.
///
/// Two stretches of circuit come within [`AT_CROSSING`] of the crossing
/// point, and they are two runs of consecutive stations with the whole rest of
/// the lap between them. Take the nearest station of each run; the one that
/// goes over is the one whose distance round the lap is nearer the fraction the
/// crossing was given.
fn crossing_stations(line: &[Vec3], pass: &Overpass) -> Option<(usize, usize)> {
    let n = line.len();
    let near = |i: usize| flat(line[i] - pass.at).length() < AT_CROSSING;
    let mut runs: Vec<usize> = Vec::new();
    for head in 0..n {
        if !near(head) || near((head + n - 1) % n) {
            continue;
        }
        let mut best = (f32::MAX, head);
        let mut d = 0;
        while d < n && near((head + d) % n) {
            let at = (head + d) % n;
            let far = flat(line[at] - pass.at).length();
            if far < best.0 {
                best = (far, at);
            }
            d += 1;
        }
        runs.push(best.1);
    }
    // Two stretches, or this is not a crossing and nothing is lifted.
    if runs.len() != 2 {
        return None;
    }
    let howfar = |i: usize| {
        let round = (i as f32 / n as f32 - pass.over).abs();
        round.min(1.0 - round)
    };
    Some(if howfar(runs[0]) <= howfar(runs[1]) {
        (runs[0], runs[1])
    } else {
        (runs[1], runs[0])
    })
}

/// Shave the crests until no step exceeds [`MAX_GRADE`]. Sweeping forward then
/// backward around the loop takes each height down to the lowest point reachable
/// at that slope, which flattens the cliffs and leaves the long climbs alone.
fn cap_grade(line: &mut [Vec3]) {
    let n = line.len();
    let rise = MAX_GRADE * (closed_length(line) / n as f32);
    for _ in 0..8 {
        for i in 0..n {
            let cap = line[i].y + rise;
            line[(i + 1) % n].y = line[(i + 1) % n].y.min(cap);
        }
        for i in (0..n).rev() {
            let cap = line[(i + 1) % n].y + rise;
            line[i].y = line[i].y.min(cap);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::track::{Track, circuits};

    /// Every circuit, so a new one has to clear the same bar as the old ones.
    fn every_track() -> impl Iterator<Item = (&'static str, Track)> {
        circuits::all()
            .iter()
            .map(|circuit| (circuit.name, Track::new(circuit)))
    }

    /// Positions worth asking about: the stations themselves, the seams between
    /// them, both edges of the road, a spread of points off it, and somewhere
    /// the circuit is not at all.
    ///
    /// The seams and the edges are the interesting ones. A station is the one
    /// place two segments meet, so it is where a tie is nearly reached and where
    /// picking the other segment would change `s` by a whole step; an edge is
    /// where the fix is read from furthest off the centreline, which is where a
    /// box has least to say about what is inside it.
    fn probes(ribbon: &Ribbon) -> Vec<Vec3> {
        let stations = ribbon.stations();
        let n = stations.len();
        let mut out = Vec::new();
        for (i, station) in stations.iter().enumerate() {
            out.push(station.pos);
            out.push(station.pos + station.right * 7.0);
            out.push(station.pos - station.right * 7.0);
            out.push((station.pos + stations[(i + 1) % n].pos) * 0.5);
            // Off the circuit entirely, at a spread of angles and distances
            // that does not line up with the stations.
            let turn = i as f32 * 0.7;
            let reach = 12.0 + (i % 37) as f32 * 4.0;
            out.push(station.pos + Vec3::new(turn.cos(), 0.0, turn.sin()) * reach);
            // And well above and below it: the lookup is in plan, so height
            // must not reach the answer.
            out.push(station.pos + Vec3::Y * 40.0);
        }
        out.push(Vec3::new(9_000.0, 0.0, -9_000.0));
        out.push(Vec3::new(-50_000.0, 120.0, 4.0));
        out
    }

    fn same_fix(name: &str, at: Vec3, found: &Fix, oracle: &Fix) {
        assert_eq!(found.point, oracle.point, "{name}: point at {at}");
        assert_eq!(found.s, oracle.s, "{name}: s at {at}");
        assert_eq!(found.lateral, oracle.lateral, "{name}: lateral at {at}");
        assert_eq!(found.tangent, oracle.tangent, "{name}: tangent at {at}");
        assert_eq!(found.right, oracle.right, "{name}: right at {at}");
        assert_eq!(found.slope, oracle.slope, "{name}: slope at {at}");
        assert_eq!(
            found.curvature, oracle.curvature,
            "{name}: curvature at {at}"
        );
    }

    /// The index is a faster way to the same answer, and "the same" is meant
    /// exactly: the whole fix, not just the nearest point, on every circuit in
    /// the game and at every kind of position anything asks about.
    ///
    /// Held to equality rather than to a tolerance on purpose. A lookup that was
    /// nearly right would move the car a hair off where the mesh puts it, shift
    /// the lap distance a hair, and do it differently in the two halves of a
    /// comparison nothing else can see. If the index ever has to be approximate,
    /// that is a decision to take deliberately, and this is what makes it one.
    #[test]
    fn the_index_agrees_with_the_walk() {
        for (name, track) in every_track() {
            let ribbon = &track.ribbon;
            for at in probes(ribbon) {
                same_fix(
                    name,
                    at,
                    &ribbon.locate(at),
                    &ribbon.locate_exhaustively(at),
                );
            }
            // And the other question the index answers: how much room the
            // circuit has at a station, which the cross-section is fitted to.
            let cap = 10.0;
            for i in (0..ribbon.stations().len()).step_by(7) {
                for side in [-1.0f32, 1.0] {
                    assert_eq!(
                        ribbon.room(i, side, cap),
                        ribbon.room_exhaustively(i, side, cap),
                        "{name}: the index and the walk disagree about the room \
                         at station {i} on side {side}"
                    );
                }
            }
        }
    }

    /// Where two segments are exactly equally near, both have to pick the same
    /// one — and a real circuit almost never gets a tie exactly, so this builds
    /// one that does.
    ///
    /// A square, asked about its own centre. Every corner of it is the same
    /// distance away, and the two segments meeting at the middle of each side
    /// both project onto that middle point exactly, so eight segments come out
    /// bit-for-bit equal and the answer is settled entirely by which one is
    /// looked at first. The walk keeps the earliest; the index descends in lap
    /// order so that it keeps the earliest too.
    #[test]
    fn a_tie_goes_to_the_earlier_segment() {
        let side = 100.0f32;
        let step = 4.0f32;
        let count = (side * 2.0 / step) as i32;
        let mut line = Vec::new();
        for k in 0..count {
            line.push(Vec3::new(side, 0.0, -side + k as f32 * step));
        }
        for k in 0..count {
            line.push(Vec3::new(side - k as f32 * step, 0.0, side));
        }
        for k in 0..count {
            line.push(Vec3::new(-side, 0.0, side - k as f32 * step));
        }
        for k in 0..count {
            line.push(Vec3::new(-side + k as f32 * step, 0.0, -side));
        }
        let ribbon = Ribbon::from_polyline(line);

        // The fixture is only worth anything if it really does tie.
        let n = ribbon.stations.len();
        let nearest: Vec<f32> = (0..n)
            .map(|i| {
                let (a, b) = (ribbon.stations[i].pos, ribbon.stations[(i + 1) % n].pos);
                let ab = flat(b - a);
                let t = (flat(-a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
                flat(-(a + (b - a) * t)).length_squared()
            })
            .collect();
        let closest = nearest.iter().copied().fold(f32::MAX, f32::min);
        let tied = nearest.iter().filter(|&&d| d == closest).count();
        assert_eq!(tied, 8, "the square does not tie the way it was built to");

        let centre = Vec3::ZERO;
        same_fix(
            "a square",
            centre,
            &ribbon.locate(centre),
            &ribbon.locate_exhaustively(centre),
        );
        // And it is the first of the eight, not merely one of them.
        let first = nearest.iter().position(|&d| d == closest).expect("a tie");
        let (chosen, _) = ribbon
            .index
            .nearest(&ribbon.stations, centre)
            .expect("the square has segments");
        assert_eq!(
            chosen, first,
            "the tie went to segment {chosen}, not {first}"
        );
    }

    /// What the index is for, stated as the thing actually claimed: the cost of
    /// a lookup does not grow with the length of the lap.
    ///
    /// A fixed count rather than a fraction, because a fraction would pass by
    /// the lap getting longer. Today the worst circuit reads 36 segments and
    /// the best 12, on laps of between 1,244 and 4,824 of them — the count
    /// follows how much circuit crowds into one box, not how much circuit there
    /// is, which is the whole claim. The bar has room in it because that
    /// crowding is the circuit's business and this is not a tuning target; it
    /// is here to notice the day the index stops pruning.
    ///
    /// Counted in segments projected rather than in seconds, so it reads the
    /// same on a loaded machine as on an idle one. The seconds are in
    /// `the_lookup`, where a number that moves with the weather belongs.
    #[test]
    fn the_index_looks_at_little_of_the_lap() {
        /// Segments a lookup may project, on average, whatever the circuit.
        const LOOKED_AT: f32 = 64.0;
        for (name, track) in every_track() {
            let ribbon = &track.ribbon;
            let n = ribbon.stations().len();
            let probes = probes(ribbon);
            let looked: usize = probes.iter().map(|&at| ribbon.projections(at)).sum();
            let average = looked as f32 / probes.len() as f32;
            assert!(
                average < LOOKED_AT,
                "{name}: a lookup projects {average:.0} of {n} segments, which is \
                 not much of an index"
            );
        }
    }

    /// What the index costs to build and what it buys, circuit by circuit, with
    /// the whole list built end to end at the bottom — which is what changing
    /// circuit in the menu does.
    ///
    /// `cargo test --locked --lib the_lookup -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn the_lookup() {
        println!(
            "{:<26}{:>10}{:>9}{:>12}{:>12}{:>9}",
            "circuit", "stations", "index", "walk", "indexed", "faster"
        );
        let mut probed = 0usize;
        let (mut walked, mut looked) = (0.0f64, 0.0f64);
        for (name, track) in every_track() {
            let ribbon = &track.ribbon;
            let probes = probes(ribbon);
            let building = Instant::now();
            let index = Index::new(ribbon.stations());
            let built = building.elapsed();
            assert!(!index.nodes.is_empty());

            let walk = Instant::now();
            let mut sink = 0.0f64;
            for &at in &probes {
                sink += ribbon.locate_exhaustively(at).s as f64;
            }
            let walk = walk.elapsed().as_secs_f64();
            let look = Instant::now();
            for &at in &probes {
                sink -= ribbon.locate(at).s as f64;
            }
            let look = look.elapsed().as_secs_f64();
            // Not to the bit: the two sums are over the same numbers in the
            // same order, but a release build is free to vectorise one loop and
            // not the other, and adding doubles in a different order is a
            // different answer in the last place. What is held to the bit is in
            // `the_index_agrees_with_the_walk`, one fix at a time.
            assert!(sink.abs() < 1e-6, "the two lookups did not agree: {sink}");

            let each = probes.len() as f64;
            let segments: usize = probes.iter().map(|&at| ribbon.projections(at)).sum();
            println!(
                "{name:<26}{:>10}{:>7.2} ms{:>9.2} us{:>9.2} us{:>8.1}x   {:.0} of {} segments",
                ribbon.stations().len(),
                built.as_secs_f64() * 1e3,
                walk / each * 1e6,
                look / each * 1e6,
                walk / look,
                segments as f64 / each,
                ribbon.stations().len(),
            );
            probed += probes.len();
            walked += walk;
            looked += look;
        }
        println!(
            "\n{probed} lookups: {:.0} ms walking, {:.0} ms indexed, {:.1}x",
            walked * 1e3,
            looked * 1e3,
            walked / looked
        );

        let building = Instant::now();
        let built: Vec<Track> = circuits::all().iter().map(Track::new).collect();
        println!(
            "building all {} circuits: {:.0} ms",
            built.len(),
            building.elapsed().as_secs_f64() * 1e3
        );
    }
}
