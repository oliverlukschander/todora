//! Corner markers: a dotted line up the verge and round the bend.
//!
//! A marker belongs to the circuit and to nothing else. It says where the corner
//! is and nothing about how to drive it, so nothing here knows what the car can
//! do — this reads the same curvature the loft is swept over, finds where the
//! road starts to turn, and lays a run of blocks along the verge either side of
//! it. A new circuit gets its markers the moment it gets its centreline, with no
//! more said about it than the trace already says.
//!
//! The run covers the corner *and* the approach to it, which is the whole point:
//! the markers counting you in do not stop at the turn, they carry on round it,
//! so the dots ahead are the shape of the corner before you can see the shape of
//! the corner. Where it tightens they crowd on the inside; where it opens they
//! run away from you. Stopping them at the entry threw that away and left the
//! corner itself unmarked, which is the part you cannot see.
//!
//! They are all the same block, all the same distance apart — 0.8 m every 3.2 m
//! — because a dotted line is read as a rhythm and a rhythm is only information
//! while it is regular. One that is longer than the next, or further from it,
//! has to be identified before it can be read, and at 24 m/s there is no time
//! for that. The approach reaches 16 m back because the car's hardest stop — top
//! speed down to a corner at [`super::ribbon::MIN_RADIUS`] — takes 15.1 m, so
//! the first block of the run is the brakes for the slowest corners and the
//! ones after it are the brakes for everything quicker.

use super::ribbon::{STEP, Station};

/// Tighter than this and the car cannot carry its top speed through: a corner.
/// A radius rather than a curvature, because a corner is a shape, and 35 m is
/// the shape of one — at [`super::ribbon::MIN_RADIUS`] the game's tightest
/// corner is 10 m, and above about 30 m the car simply does not lift.
const CORNER_RADIUS: f32 = 35.0;
/// Bends with less than this much straight between them are one corner. Without
/// it a chicane is four corners, each laying its own approach over the last
/// one's exit, and the run through it comes out in pieces.
const MERGE: f32 = 6.0;
/// Stations the curvature is averaged over before it is read. The stations are
/// 0.4 m apart and their curvature comes off a polyline, so it is noisy at the
/// scale of one station; a corner is not, and 3.6 m is enough to tell them apart
/// without rounding off a real one.
const SMOOTH: usize = 9;
/// Stations from the start of one marker to the start of the next: 3.2 m.
///
/// In stations rather than in metres because the run has to be evenly spaced and
/// only this is. A gap in metres falls between two stations and rounds to a
/// different number of them at different multiples, which is a rhythm that is
/// regular everywhere except where it is not.
const SPACING: usize = 8;
/// How far the run reaches back up the road from the corner, in markers. Five of
/// them is 16 m, which is the whole of the hardest stop the car has in it.
const APPROACH: usize = 5;
/// How long a marker is, in stations: 0.8 m, a third of the car. Small enough
/// that a run of them reads as a line rather than as a row of boards, which is
/// what lets it be laid through the corner as well as up to it.
const MARKER: usize = 2;

/// Which stations carry marker paint. One flag per station, in the order
/// [`super::profile::Profile::loft`] sweeps them.
pub(super) fn marks(stations: &[Station]) -> Vec<bool> {
    let n = stations.len();
    let mut marks = vec![false; n];
    for start in markers(stations) {
        for d in 0..MARKER {
            marks[(start + d) % n] = true;
        }
    }
    marks
}

/// Every marker on the circuit, as the station it starts at.
///
/// One run per stretch of marked road, laid from the head of it so that the
/// spacing is exact all the way down and the last marker of the approach lands
/// on the corner entry itself. A marker that would hang off the end of its run
/// is not laid at all, so no two runs can grow into each other and read as one
/// long mark.
fn markers(stations: &[Station]) -> Vec<usize> {
    let n = stations.len();
    let zone = zone(&corners(stations));
    let mut out = Vec::new();
    for head in 0..n {
        if !zone[head] || zone[(head + n - 1) % n] {
            continue;
        }
        let mut at = 0;
        while at < n && (0..MARKER).all(|d| zone[(head + at + d) % n]) {
            out.push((head + at) % n);
            at += SPACING;
        }
    }
    out
}

/// The road the markers are laid along: every corner, and [`APPROACH`] markers'
/// worth of the run up to it. Corners close enough together share one stretch,
/// so the dots carry on through a complex instead of restarting inside it.
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

    /// Every circuit, so a new one has to clear the same bar as the old ones.
    fn every_track() -> impl Iterator<Item = (&'static str, Track)> {
        circuits::all()
            .iter()
            .map(|circuit| (circuit.name, Track::new(circuit)))
    }

    /// Markers mean a corner, so a marker where there is no corner means
    /// nothing. Every one of them is either in a bend or inside the run up to
    /// one — never a stray dot down the back straight.
    #[test]
    fn a_marker_means_a_corner() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let n = stations.len();
            let corner = corners(stations);
            for (i, marked) in marks(stations).iter().enumerate() {
                let coming = corner[i] || (1..=APPROACH * SPACING).any(|d| corner[(i + d) % n]);
                assert!(
                    !marked || coming,
                    "{name}: a marker at station {i} has no corner to mark"
                );
            }
        }
    }

    /// The part that stopping at the entry threw away. Every corner long enough
    /// to hold a marker carries one, so the run goes round the bend rather than
    /// up to it and no further.
    #[test]
    fn the_run_goes_through_the_corner() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let n = stations.len();
            let corner = corners(stations);
            let marks = marks(stations);
            let mut checked = 0;
            for entry in 0..n {
                if !corner[entry] || corner[(entry + n - 1) % n] {
                    continue;
                }
                let length = (0..n).take_while(|&d| corner[(entry + d) % n]).count();
                if length < SPACING + MARKER {
                    continue;
                }
                checked += 1;
                assert!(
                    (0..length).any(|d| marks[(entry + d) % n]),
                    "{name}: the corner at station {entry} is {length} stations \
                     long and carries no marker"
                );
            }
            assert!(checked > 0, "{name} has no corner long enough to test");
        }
    }

    /// The rhythm. Down any one stretch of marked road every marker is the same
    /// length as the last and exactly [`SPACING`] from it — a run is read rather
    /// than counted, and only a regular one can be.
    #[test]
    fn the_run_keeps_its_rhythm() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let n = stations.len();
            let zone = zone(&corners(stations));
            let marks = marks(stations);

            for i in 0..n {
                if !marks[i] || marks[(i + n - 1) % n] {
                    continue;
                }
                let length = (0..n).take_while(|&d| marks[(i + d) % n]).count();
                assert_eq!(
                    length, MARKER,
                    "{name}: a marker at station {i} is {length} stations long"
                );
                // The next marker, and the road between here and it.
                let Some(gap) = (length..n).find(|&d| marks[(i + d) % n]) else {
                    continue;
                };
                if (0..gap).all(|d| zone[(i + d) % n]) {
                    assert_eq!(
                        gap, SPACING,
                        "{name}: the marker at station {i} is {gap} stations \
                         from the next one down the same stretch"
                    );
                }
            }
        }
    }

    /// Enough of them to be a line, on all three. The run is far longer than the
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
            let markers = markers(stations).len();
            assert!(
                markers > entries * APPROACH,
                "{name}: {markers} markers for {entries} corners is the approach \
                 and nothing round the bend"
            );
        }
    }
}
