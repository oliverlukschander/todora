//! Corner markers: little diamonds up the verge and round the bend.
//!
//! A marker belongs to the circuit and to nothing else. It says where the corner
//! is and nothing about how to drive it, so nothing here knows what the car can
//! do — this reads the same curvature the loft is swept over, finds where the
//! road starts to turn, and lays a line of diamonds along the verge either side
//! of it. A new circuit gets its markers the moment it gets its centreline, with
//! no more said about it than the trace already says.
//!
//! The line covers the corner *and* the approach to it, which is the whole
//! point: the diamonds counting you in do not stop at the turn, they carry on
//! round it, so the ones ahead are the shape of the corner before you can see
//! the shape of the corner. Where it tightens they crowd up on the inside; where
//! it opens they run away from you. The approach reaches 16 m back because the
//! car's hardest stop — top speed down to a corner at
//! [`super::ribbon::MIN_RADIUS`] — takes 15.1 m, so the first diamond of a line
//! is the brakes for the slowest corners and the ones after it are the brakes
//! for everything quicker. They are all the same diamond the same distance
//! apart, because a dotted line is read as a rhythm and a rhythm is only
//! information while it is regular.
//!
//! These are the one mark on the circuit the loft cannot make. Every other one —
//! the kerb stripes, the edge lines, the start/finish paint — is a strip of the
//! sweep taking a colour, which fixes what shape it may be: a rectangle, square
//! to the road, at least one station long and its whole band wide. A diamond is
//! a fifth of the width of the band it sits in and is turned forty-five degrees
//! to the road, so no way of writing the table produces one. It is four vertices
//! of its own instead, sitting [`LIFT`] above the verge and lying along the fall
//! of it. They go into the loft's own mesh rather than a second one, so there is
//! still one surface, one material, and one thing to replace when the circuit
//! changes.

use bevy::prelude::*;

use super::profile::{EDGE, HALF_WIDTH, Profile, paint};
use super::ribbon::{STEP, Station};

/// Tighter than this and the car cannot carry its top speed through: a corner.
/// A radius rather than a curvature, because a corner is a shape, and 35 m is
/// the shape of one — at [`super::ribbon::MIN_RADIUS`] the game's tightest
/// corner is 10 m, and above about 30 m the car simply does not lift.
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
/// Stations from one diamond to the next: 3.2 m.
///
/// In stations rather than in metres because the line has to be evenly spaced
/// and only this is. A gap in metres falls between two stations and rounds to a
/// different number of them at different multiples, which is a rhythm that is
/// regular everywhere except where it is not.
const SPACING: usize = 8;
/// How far the line reaches back up the road from the corner, in diamonds. Five
/// of them is 16 m, which is the whole of the hardest stop the car has in it.
const APPROACH: usize = 5;
/// Half a diamond's diagonal. 0.4 m point to point, a third of the width of the
/// car: enough to carry down a straight, where at 0.3 m a diamond was there when
/// you arrived at it and not before, and still a twentieth of the road.
const HALF: f32 = 0.20;
/// Metres either side of the centreline the line of diamonds runs down. Half a
/// metre outside the kerb — close enough to the road to sit in the corner of the
/// eye, far enough not to be taken for part of it — and well inside the
/// narrowest verge a circuit is allowed to carry, so a cramped circuit draws its
/// verge in without drawing it out from under these.
const LATERAL: f32 = 4.5;
/// How far a diamond floats above the verge. A hair, and only so the depth
/// buffer has something to separate the two by: a fifth of the kerb's lip.
const LIFT: f32 = 0.01;
/// The line runs through these in order, one to a diamond, and starts again.
///
/// Five of them, which is [`APPROACH`], so one turn of the cycle is exactly the
/// approach: from the diamond where the brakes go on for the slowest corners you
/// see the whole palette once, in order, and the colour coming back round is the
/// corner entry. A run of identical marks cannot say that — you would have to
/// count them, and counting is what there is no time for.
///
/// They are the first five pool balls, which is where the diamonds came from and
/// which is a set picked to be told apart across a table at a glance, brightened
/// for a verge that is already green. White is not among them on purpose: it is
/// what the edge lines and the kerb stripes are, and the road's paint and the
/// circuit's markers should not be the same thing seen twice.
#[cfg(test)]
pub(super) const PALETTE_LEN: usize = PALETTE.len();

const PALETTE: [(f32, f32, f32); 5] = [
    (0.98, 0.80, 0.10), // yellow
    (0.15, 0.50, 0.95), // blue
    (0.90, 0.15, 0.12), // red
    (0.62, 0.25, 0.80), // purple
    (0.97, 0.48, 0.06), // orange
];

// A diamond lies on the verge, clear of the kerb and inside the loft. Both are
// settled here rather than found later: the cross-section is a constant and so
// is [`LATERAL`], so a change that puts the two through each other has no
// business compiling. What the constants cannot say is how far a given circuit's
// verge actually reaches, which narrows to what that circuit has room for —
// `diamonds_lie_on_the_verge` is what asks each of them.
const _: () = assert!(LATERAL - HALF > HALF_WIDTH);
const _: () = assert!(LATERAL + HALF < EDGE);

/// One diamond, ready to go into the loft's mesh.
pub(super) struct Diamond {
    /// Four corners wound to face up: back, out, front, in.
    pub corners: [Vec3; 4],
    /// Its turn of [`PALETTE`], linear for the vertex colour attribute.
    pub color: [f32; 4],
}

/// Every diamond on the circuit. Both verges, so a marked station makes two —
/// the same colour on each, because the line is one line seen from either side.
///
/// Each corner takes the verge's own height at its own distance off the
/// centreline, so a diamond lies along the fall of the verge instead of floating
/// flat over it.
pub(super) fn diamonds(stations: &[Station], profile: &Profile) -> Vec<Diamond> {
    let marked = markers(stations);
    let mut out = Vec::with_capacity(marked.len() * 2);
    for (at, step) in marked {
        let station = &stations[at];
        let (r, g, b) = PALETTE[step % PALETTE.len()];
        for side in [1.0, -1.0] {
            // `across` is not flipped with the side, so both verges wind the
            // same way round and both faces point up.
            let corner = |across: f32, along: f32| {
                let lateral = LATERAL * side + across;
                station.pos
                    + station.right * lateral
                    + station.tangent * along
                    + Vec3::Y * (profile.height(lateral) + LIFT)
            };
            out.push(Diamond {
                corners: [
                    corner(0.0, -HALF),
                    corner(HALF, 0.0),
                    corner(0.0, HALF),
                    corner(-HALF, 0.0),
                ],
                color: paint(r, g, b),
            });
        }
    }
    out
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
#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{Track, circuits};

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
                let across = diamond.corners[1].distance(diamond.corners[3]);
                let along = diamond.corners[0].distance(diamond.corners[2]);
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
            }
        }
    }

    /// Diamonds lie on the verge of the circuit actually being driven. They sit
    /// at a fixed distance off the centreline while the verge either side
    /// narrows to whatever that circuit has room for, and the const assertions
    /// above cannot see how far that is.
    #[test]
    fn diamonds_lie_on_the_verge() {
        for (name, track) in every_track() {
            let edge = track.profile.edge();
            assert!(
                LATERAL + HALF < edge,
                "{name} carries {edge:.2} m of cross-section, which does not \
                 reach the diamonds at {:.2}",
                LATERAL + HALF
            );
            for diamond in diamonds(track.ribbon.stations(), &track.profile) {
                for corner in diamond.corners {
                    let lateral = track.ribbon.locate(corner).lateral.abs();
                    assert!(
                        (HALF_WIDTH..edge).contains(&lateral),
                        "{name}: a diamond corner sits {lateral:.2} m out"
                    );
                }
            }
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
