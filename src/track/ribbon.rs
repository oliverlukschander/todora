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
/// No corner may be tighter than this. A parallel curve offset by `w` stays
/// regular only while `w` is inside the radius of curvature; past that it cusps
/// and folds back through itself. Scaled to ⅓, Spielberg's hairpins come out at
/// well under a metre of radius, so the corners have to be opened before the road
/// can carry its full width through them.
///
/// This is one half of the contract the cross-section in [`super`] is checked
/// against; the other half is [`Ribbon::min_separation`]. Together they are what
/// lets the loft sweep the profile with no clamping anywhere.
pub(crate) const MIN_RADIUS: f32 = 10.0;
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
/// Stretches of circuit closer together than this along the lap are neighbours,
/// and are expected to be close in space too.
const APART: f32 = 60.0;
/// Corner-opening sweeps. Each pass relaxes the tight stations and then respaces
/// them; respacing shifts the curvature a little, so the two alternate until the
/// circuit settles and the loop stops early. Spielberg takes a few hundred and
/// about 30 ms; the bound is only there so a circuit that cannot be opened fails
/// the profile check rather than spinning.
const RELAX_PASSES: usize = 1200;
const RELAX_STEPS: usize = 12;
/// How far a too-tight station moves toward its neighbours' midpoint per step.
const RELAX_RATE: f32 = 0.5;

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
}

/// The closed centreline, ready to loft.
pub struct Ribbon {
    stations: Vec<Station>,
    length: f32,
    kept: f32,
}

/// Where a world position sits relative to the circuit.
pub struct Fix {
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
    pub fn new(control: &[Vec3], corners: f32) -> Self {
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
        cap_grade(&mut line);
        let mut ribbon = Self::from_polyline(line);
        ribbon.kept = ribbon.length / before.max(1e-4);
        ribbon
    }

    pub fn stations(&self) -> &[Station] {
        &self.stations
    }

    /// Plan length of one lap.
    pub fn length(&self) -> f32 {
        self.length
    }

    /// How much of the circuit came through opening its corners, as a fraction
    /// of the spline it was built from.
    ///
    /// Opening a corner cuts it, so every circuit loses a little — the two in
    /// the game keep about nine tenths. A circuit whose corners are nearly all
    /// tighter than [`MIN_RADIUS`] at this scale loses far more than that: there
    /// is nothing left to relax against, and pass after pass pulls the whole lap
    /// toward its own centre until it is a loop with no corners in it. Monaco
    /// shrinks from 3.3 km to a hundred metres that way. Every station on it is
    /// perfectly well formed, so nothing else notices; this does.
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

    /// Tightest corner on the circuit. The cross-section may not reach past
    /// this, or its outer ribs cusp.
    pub fn min_radius(&self) -> f32 {
        self.stations
            .iter()
            .filter(|s| s.curvature.abs() > 1e-7)
            .fold(f32::MAX, |r, s| r.min(1.0 / s.curvature.abs()))
    }

    /// Closest approach between two stretches of the circuit that are not
    /// neighbours. Half of this is the widest the cross-section can reach before
    /// the verge of one stretch grows through the verge of another — the bound a
    /// per-station curvature check cannot see, because nothing is wrong locally.
    pub fn min_separation(&self) -> f32 {
        let n = self.stations.len();
        let skip = (APART / STEP) as usize;
        let mut best = f32::MAX;
        for i in 0..n {
            for j in (i + skip)..n {
                // `j` runs ahead of `i`, so it is only far enough away if it is
                // also far enough from `i` the long way round the lap.
                if n - j + i < skip {
                    continue;
                }
                best = best.min(flat(self.stations[j].pos - self.stations[i].pos).length());
            }
        }
        best
    }

    /// Nearest point on the centreline. A linear scan of the stations, which is
    /// well under a millisecond for the handful of callers that ask each frame.
    pub fn locate(&self, pos: Vec3) -> Fix {
        let n = self.stations.len();
        let step = self.length / n as f32;
        let mut best = f32::MAX;
        let mut fix = Fix {
            point: self.stations[0].pos,
            tangent: self.stations[0].tangent,
            right: self.stations[0].right,
            lateral: 0.0,
            slope: self.stations[0].slope,
            curvature: self.stations[0].curvature,
            s: 0.0,
        };
        for i in 0..n {
            let a = &self.stations[i];
            let b = &self.stations[(i + 1) % n];
            let ab = flat(b.pos - a.pos);
            let len2 = ab.length_squared();
            if len2 < 1e-9 {
                continue;
            }
            let t = (flat(pos - a.pos).dot(ab) / len2).clamp(0.0, 1.0);
            let point = a.pos + (b.pos - a.pos) * t;
            let d = flat(pos - point).length_squared();
            if d < best {
                best = d;
                let right = a.right.lerp(b.right, t).normalize_or(a.right);
                fix = Fix {
                    point,
                    tangent: a.tangent.lerp(b.tangent, t).normalize_or(a.tangent),
                    right,
                    lateral: flat(pos - point).dot(right),
                    slope: a.slope.lerp(b.slope, t),
                    curvature: a.curvature.lerp(b.curvature, t),
                    s: a.s + step * t,
                };
            }
        }
        fix
    }

    fn from_polyline(line: Vec<Vec3>) -> Self {
        let n = line.len();
        let length = closed_length(&line);
        let step = length / n as f32;
        let stations = (0..n)
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
                }
            })
            .collect();
        Self {
            stations,
            length,
            kept: 1.0,
        }
    }
}

/// Drop the height: the cross-section and every distance along the lap are
/// measured in plan, so the loft's spacing does not stretch on the climbs.
fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
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
