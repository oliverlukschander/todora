//! Braking markers: three blocks on the verge counting into every corner.
//!
//! A marker belongs to the circuit and to nothing else. It says how far there is
//! to go and nothing about how to drive it, so nothing here knows what the car
//! can do — this reads the same curvature the loft is swept over, finds where
//! the road starts to turn, and steps back from it. A new circuit gets its
//! markers the moment it gets its centreline, with no more said about it than
//! the trace already says.
//!
//! They are all the same marker, the same distance apart, close in to the
//! corner: at 4.8, 9.6 and 14.4 m, which is two, four and six car lengths. You
//! do not read a marker, you count them, and counting only works if they are
//! interchangeable — one that is bigger than the next, or further from it, has
//! to be identified before it can be counted, and at 24 m/s there is no time for
//! that. The set reaches 14.4 m out because the car's hardest stop — top speed
//! down to a corner at [`super::ribbon::MIN_RADIUS`] — takes 15.1 m. So the
//! first marker is the brakes for the slowest corners, the second for the medium
//! ones, the third for the quick ones, and none of them for what the car holds
//! flat.

use super::ribbon::{STEP, Station};

/// Tighter than this and the car cannot carry its top speed through: a corner.
/// A radius rather than a curvature, because a corner is a shape, and 35 m is
/// the shape of one — at [`super::ribbon::MIN_RADIUS`] the game's tightest
/// corner is 10 m, and above about 30 m the car simply does not lift.
const CORNER_RADIUS: f32 = 35.0;
/// Bends with less than this much straight between them are one corner and
/// share one set of markers. Without it a chicane is four corners and its
/// approach carries four sets of markers on top of each other, counting into the
/// wrong thing four times.
const MERGE: f32 = 6.0;
/// Stations the curvature is averaged over before it is read. The stations are
/// 0.4 m apart and their curvature comes off a polyline, so it is noisy at the
/// scale of one station; a corner is not, and 3.6 m is enough to tell them apart
/// without rounding off a real one.
const SMOOTH: usize = 9;
/// Stations from one marker to the next, and from the last one to the corner.
/// Twelve of them is 4.8 m, two car lengths.
///
/// In stations rather than in metres because the markers have to be evenly
/// spaced and only this is: a gap in metres falls between two stations and
/// rounds to a different number of them at different multiples, which is a set
/// that is evenly spaced everywhere except where it is not.
const SPACING: usize = 12;
const MARKERS: usize = 3;
/// How long a marker is, in stations. Three is 1.2 m — half the length of the
/// car and about the width of it, so it reads as a block rather than as a mark
/// on the grass.
const MARKER: usize = 3;

/// Which stations carry marker paint. One flag per station, in the order
/// [`super::profile::Profile::loft`] sweeps them.
pub(super) fn marks(stations: &[Station]) -> Vec<bool> {
    let n = stations.len();
    let mut marks = vec![false; n];
    for near in markers(stations) {
        for d in 0..MARKER {
            marks[(near + n - d) % n] = true;
        }
    }
    marks
}

/// Every marker on the circuit, as the station at its near edge — the last of it
/// the car passes, and the end the distance is measured from.
fn markers(stations: &[Station]) -> Vec<usize> {
    let n = stations.len();
    let corner = corners(stations);
    let mut out = Vec::new();
    for entry in 0..n {
        // The first station of a corner, which is what the markers count into.
        if !corner[entry] || corner[(entry + n - 1) % n] {
            continue;
        }
        for step in 1..=MARKERS {
            // A marker has to be able to see its corner. The approach is
            // everything from its far edge to the corner itself, and all of it
            // has to be clear road — a corner in the middle of that would leave
            // the marker counting into that one instead, from the wrong place.
            let back = step * SPACING;
            if (1..back + MARKER).any(|d| corner[(entry + n - d) % n]) {
                continue;
            }
            out.push((entry + n - back) % n);
        }
    }
    out
}

/// Which stations are inside a corner, bends closer than [`MERGE`] joined.
///
/// This is also what keeps the markers off the corners: a straight is whatever
/// this leaves over, and a marker only stands on one.
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

    /// The thing a braking marker must never do. One painted through a corner is
    /// telling a driver who is already turning that there is a corner coming,
    /// and the one place it would be read is the one place it is wrong.
    #[test]
    fn no_marker_stands_in_a_corner() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let corner = corners(stations);
            for (i, marked) in marks(stations).iter().enumerate() {
                assert!(
                    !marked || !corner[i],
                    "{name}: a marker is painted through the corner at station {i}"
                );
            }
        }
    }

    /// The whole of what a marker claims: it stands a whole number of steps from
    /// the corner it counts into, and never further out than the set reaches.
    #[test]
    fn every_marker_is_a_whole_step_from_its_corner() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let n = stations.len();
            let corner = corners(stations);
            let entries: Vec<usize> = (0..n)
                .filter(|&i| corner[i] && !corner[(i + n - 1) % n])
                .collect();
            assert!(!entries.is_empty(), "{name} has no corners at all");

            for near in markers(stations) {
                // The corner it counts into: the next one up the road from it.
                let ahead = entries
                    .iter()
                    .map(|&e| (e + n - near) % n)
                    .min()
                    .expect("the circuit has corners");
                assert_eq!(
                    ahead % SPACING,
                    0,
                    "{name}: a marker stands {ahead} stations from its corner, \
                     which is not a whole step of {SPACING}"
                );
                assert!(
                    (1..=MARKERS).contains(&(ahead / SPACING)),
                    "{name}: a marker stands {} steps out, past the {MARKERS} \
                     the set reaches",
                    ahead / SPACING
                );
            }
        }
    }

    /// One marker is every other marker. They are counted rather than read, so
    /// each has to come out the same length as the last, and none may run into
    /// its neighbour and be counted once.
    #[test]
    fn every_marker_is_the_same_marker() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let n = stations.len();
            let marks = marks(stations);
            let mut found = 0;
            for i in 0..n {
                if !marks[i] || marks[(i + n - 1) % n] {
                    continue;
                }
                let mut len = 0;
                while marks[(i + len) % n] {
                    len += 1;
                }
                assert_eq!(
                    len, MARKER,
                    "{name}: a marker at station {i} is {len} stations long"
                );
                found += 1;
            }
            assert_eq!(
                found,
                markers(stations).len(),
                "{name}: markers ran into each other on the way to the paint"
            );
        }
    }

    /// Markers where they are worth having, on all three. A corner with another
    /// corner close behind it gets fewer of them, and one with a corner right
    /// behind it gets none — the driver is already braking — but that is the
    /// exception on every circuit, not the rule.
    #[test]
    fn every_circuit_is_worth_a_set_of_markers() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let n = stations.len();
            let corner = corners(stations);
            let entries = (0..n)
                .filter(|&i| corner[i] && !corner[(i + n - 1) % n])
                .count();
            let markers = markers(stations).len();
            assert!(
                markers > entries,
                "{name}: {markers} markers for {entries} corners is barely one each"
            );
            assert!(
                markers <= entries * MARKERS,
                "{name}: {markers} markers is more than {MARKERS} to a corner"
            );
        }
    }
}
