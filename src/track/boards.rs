//! Braking distance boards: "150", "100" and "50" on the outside verge before
//! every big corner, facing the way the lap runs.
//!
//! The boards are part of the one trackside mesh (see [`super::trackside`]),
//! so they cost no draw call of their own and nothing per frame. The numbers
//! are geometry, three glyphs' worth of quads, which stay sharp at any distance
//! without a texture. They are scenery only: nothing drives into them.

use super::{
    Track, markers,
    profile::{HALF_WIDTH, deep},
    trackside::{Builder, atlas::Cell},
};
use bevy::prelude::*;
use std::f32::consts::FRAC_PI_2;

/// Real-world board distances to Todora's. The circuits are drawn at a plan
/// scale of 0.12, and at that scale the car needs about 12 m to brake from top
/// speed into a hairpin, so the "100" board lands where hard braking starts on
/// every circuit: boards at 6, 12 and 18 m before the turn-in.
const BOARD_SCALE: f32 = 0.12;
/// Real metres to the turn-in each board says, nearest first.
const LABELS: [u32; 3] = [50, 100, 150];
/// A big corner turns the road more than this...
const BIG_TURN: f32 = FRAC_PI_2;
/// ...within this many metres of road.
const BIG_WITHIN: f32 = 20.0;
/// Board centre, metres out from the edge of the kerb: clear of the chevron
/// plaques, which never reach more than 0.67 m out.
const OUT: f32 = 1.1;
/// Turned this far from square-on to the road towards the driver on it.
const TOE: f32 = 0.26;
const PANEL_HALF_WIDTH: f32 = 0.26;
const PANEL_HALF_HEIGHT: f32 = 0.15;
const PANEL_HALF_DEPTH: f32 = 0.015;
/// Clear air under the panel, on two posts.
const POST_HEIGHT: f32 = 0.22;
const POST_HALF: f32 = 0.024;
const POST_SPREAD: f32 = 0.17;
/// How far the posts go into the ground, so a slope never shows a gap.
const SINK: f32 = 0.08;
/// Glyph height.
const GLYPH: f32 = 0.19;
/// Centre to centre, the least a board may stand from a start/finish post.
pub(super) const POST_ROOM: f32 = 0.7;
/// Nothing of a board comes nearer than this to the kerb of any road.
pub(super) const CLEAR: f32 = 0.5;

const FACE: (f32, f32, f32) = (0.93, 0.93, 0.90);
const NUMBER: (f32, f32, f32) = (0.07, 0.07, 0.08);
const FRAME: (f32, f32, f32) = (0.16, 0.19, 0.18);
const EDGE: (f32, f32, f32) = (0.25, 0.28, 0.27);
const POST: (f32, f32, f32) = (0.42, 0.43, 0.43);
const POST_SHADE: (f32, f32, f32) = (0.33, 0.34, 0.34);

/// One board, before it is known to fit: the station it stands at, which of
/// [`LABELS`] it carries, the verge it stands on and the corner it counts to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Board {
    pub(super) at: usize,
    pub(super) label: usize,
    pub(super) side: f32,
    pub(super) entry: usize,
}

/// Every board of a circuit that fits, into the trackside mesh.
pub(super) fn build(track: &Track, out: &mut Builder) {
    for board in placed(track) {
        out.board(track, &board);
    }
}

/// The planned boards that stand clear of every road, off any bridge and away
/// from the start/finish posts. A board that does not fit is treated like a
/// straight too short for it: it and the ones farther from its corner go, so a
/// corner never has a gap in its countdown.
pub(super) fn placed(track: &Track) -> Vec<Board> {
    let stations = track.ribbon.stations();
    let curvature: Vec<f32> = stations.iter().map(|s| s.curvature).collect();
    let step = track.ribbon.length() / stations.len() as f32;
    let posts = super::start::post_bases(track);
    let mut blocked = None;
    plan(&curvature, step)
        .into_iter()
        .filter(|board| {
            if blocked == Some(board.entry) {
                return false;
            }
            let centre = frame(track, board).0;
            let fits = deep(&stations[board.at]) == 0.0
                && clear(track, board)
                && posts
                    .iter()
                    .all(|post| (*post - centre).xz().length() > POST_ROOM);
            if !fits {
                blocked = Some(board.entry);
            }
            fits
        })
        .collect()
}

/// Where the boards go, from the curvature at each station and the spacing of
/// the stations alone.
///
/// A big corner is a corner (in the chevron plaques' sense) in which the road
/// turns more than [`BIG_TURN`] one way within [`BIG_WITHIN`]. Its boards count
/// back from its first station, which is where the turn-in is, and stand only
/// on the straight run up to it: where the previous corner is too close for
/// all three, the farthest go, and the rest keep their distances.
pub(super) fn plan(curvature: &[f32], step: f32) -> Vec<Board> {
    let n = curvature.len();
    let mut out = Vec::new();
    for corner in big_corners(curvature, step) {
        for (label, metres) in LABELS.into_iter().enumerate() {
            let back = (metres as f32 * BOARD_SCALE / step).round() as usize;
            if back >= corner.straight {
                break;
            }
            out.push(Board {
                at: (corner.entry + n - back) % n,
                label,
                side: corner.outside,
                entry: corner.entry,
            });
        }
    }
    out
}

/// A big corner: where it starts, how many stations it runs, which verge is
/// its outside, and how many stations of straight lead up to it.
#[derive(Clone, Copy, Debug)]
pub(super) struct BigCorner {
    pub(super) entry: usize,
    pub(super) length: usize,
    pub(super) outside: f32,
    pub(super) straight: usize,
}

/// Every corner (in the chevron plaques' sense) in which the road turns more
/// than [`BIG_TURN`] one way within [`BIG_WITHIN`].
pub(super) fn big_corners(curvature: &[f32], step: f32) -> Vec<BigCorner> {
    let n = curvature.len();
    let corner = markers::corners_of(curvature);
    let window = (BIG_WITHIN / step).round() as usize;
    let mut out = Vec::new();
    for entry in 0..n {
        if !corner[entry] || corner[(entry + n - 1) % n] {
            continue;
        }
        let length = (0..n).take_while(|&d| corner[(entry + d) % n]).count();
        // The first stretch of the corner that turns the road far enough one
        // way, which is the turn the driver brakes for. A complex that opens
        // with a kink and then hairpins the other way is braked for the hairpin.
        let Some(turn) = (0..length)
            .map(|d| {
                (0..window)
                    .map(|w| curvature[(entry + d + w) % n] * step)
                    .sum::<f32>()
            })
            .find(|turn| turn.abs() > BIG_TURN)
        else {
            continue;
        };
        out.push(BigCorner {
            entry,
            length,
            // Positive curvature turns right, so its outside is the left.
            outside: -turn.signum(),
            straight: (1..n).take_while(|&d| !corner[(entry + n - d) % n]).count(),
        });
    }
    out
}

/// Whether every point of the board is at least [`CLEAR`] outside the kerb of
/// whichever road it is nearest to, its own or another run past in plan.
fn clear(track: &Track, board: &Board) -> bool {
    footprint(track, board).into_iter().all(|point| {
        let road = track.fix(point, None);
        road.lateral.abs() > HALF_WIDTH + CLEAR
    })
}

/// The frame a board is built in: centre at ground level, its face's normal,
/// and the direction across the face left to right as the driver sees it.
fn frame(track: &Track, board: &Board) -> (Vec3, Vec3, Vec3) {
    let station = &track.ribbon.stations()[board.at];
    let toward_road = -station.right * board.side;
    let normal = (-station.tangent * TOE.cos() + toward_road * TOE.sin()).normalize();
    let across = (-normal).cross(Vec3::Y).normalize();
    let mut centre = station.pos + station.right * board.side * (HALF_WIDTH + OUT);
    centre.y = track.ground_from(centre, Some(station.s)).height;
    (centre, normal, across)
}

/// The corners of the board and its posts in plan.
pub(super) fn footprint(track: &Track, board: &Board) -> Vec<Vec3> {
    let (centre, normal, across) = frame(track, board);
    let mut points = Vec::new();
    for x in [-PANEL_HALF_WIDTH, PANEL_HALF_WIDTH] {
        for z in [-PANEL_HALF_DEPTH - 2.0 * POST_HALF, PANEL_HALF_DEPTH] {
            points.push(centre + across * x + normal * z);
        }
    }
    points
}

impl Builder {
    fn board(&mut self, track: &Track, board: &Board) {
        let station = &track.ribbon.stations()[board.at];
        let (centre, normal, across) = frame(track, board);
        let feet = [-POST_SPREAD, POST_SPREAD].map(|x| {
            let foot = centre + across * x;
            track.ground_from(foot, Some(station.s)).height
        });
        let low = feet[0].min(feet[1]) - SINK;
        let bottom = feet[0].max(feet[1]) + POST_HEIGHT;
        let top = bottom + 2.0 * PANEL_HALF_HEIGHT;
        for x in [-POST_SPREAD, POST_SPREAD] {
            self.column(
                centre + across * x - normal * (PANEL_HALF_DEPTH + POST_HALF),
                low,
                bottom + PANEL_HALF_HEIGHT,
                across * POST_HALF,
                normal * POST_HALF,
                [POST_SHADE, POST_SHADE, POST],
                true,
            );
        }
        // The face is the number, painted into the atlas: one quad that
        // stays sharp near and fades evenly far, with nothing laid over it.
        self.open_column(
            centre,
            bottom,
            top,
            across * PANEL_HALF_WIDTH,
            normal * PANEL_HALF_DEPTH,
            [None, Some(FRAME)],
            EDGE,
            true,
        );
        let face = |x: f32, y: f32| {
            let mut p = centre + across * x + normal * PANEL_HALF_DEPTH;
            p.y = y;
            p
        };
        self.sign(
            [
                face(-PANEL_HALF_WIDTH, bottom),
                face(PANEL_HALF_WIDTH, bottom),
                face(PANEL_HALF_WIDTH, top),
                face(-PANEL_HALF_WIDTH, top),
            ],
            normal,
            Cell::Label(board.label),
        );
    }
}

/// The colour at `(u, v)` across the face of the board carrying
/// `LABELS[label]`, `v` up: its number, black and centred on white.
pub(super) fn paint_label(label: usize, u: f32, v: f32) -> (f32, f32, f32) {
    let x = (u - 0.5) * 2.0 * PANEL_HALF_WIDTH;
    let y = (v - 0.5) * 2.0 * PANEL_HALF_HEIGHT;
    let text = LABELS[label].to_string();
    let width: f32 = text.chars().map(advance).sum::<f32>() - GAP;
    let mut pen = -width * GLYPH / 2.0;
    for digit in text.chars() {
        let (gx, gy) = ((x - pen) / GLYPH, y / GLYPH + 0.5);
        if glyph(digit).iter().any(|quad| inside(quad, gx, gy)) {
            return NUMBER;
        }
        pen += advance(digit) * GLYPH;
    }
    FACE
}

/// Whether a point is inside a convex quad listed corner by corner, either
/// way round.
fn inside(&[x0, y0, x1, y1, x2, y2, x3, y3]: &[f32; 8], x: f32, y: f32) -> bool {
    let corners = [(x0, y0), (x1, y1), (x2, y2), (x3, y3)];
    let sides: Vec<f32> = (0..4)
        .map(|i| {
            let (ax, ay) = corners[i];
            let (bx, by) = corners[(i + 1) % 4];
            (bx - ax) * (y - ay) - (by - ay) * (x - ax)
        })
        .collect();
    sides.iter().all(|&s| s >= 0.0) || sides.iter().all(|&s| s <= 0.0)
}

/// Space between glyphs, in glyph heights.
const GAP: f32 = 0.2;

fn advance(digit: char) -> f32 {
    match digit {
        '1' => 0.46 + GAP,
        _ => 0.62 + GAP,
    }
}

/// The three digits the boards need, as quads on a glyph one unit high, each
/// listed corner by corner. Blocky, like the stencilled numbers on real boards.
fn glyph(digit: char) -> &'static [[f32; 8]] {
    const S: f32 = 0.17;
    const W: f32 = 0.62;
    const MID: f32 = 0.52;
    match digit {
        '0' => &[
            [0.0, 0.0, S, 0.0, S, 1.0, 0.0, 1.0],
            [W - S, 0.0, W, 0.0, W, 1.0, W - S, 1.0],
            [S, 1.0 - S, W - S, 1.0 - S, W - S, 1.0, S, 1.0],
            [S, 0.0, W - S, 0.0, W - S, S, S, S],
        ],
        '1' => &[
            [0.24, 0.0, 0.24 + S, 0.0, 0.24 + S, 1.0, 0.24, 1.0],
            [0.02, 0.72, 0.24, 0.86, 0.24, 1.0, 0.02, 0.86],
            [0.04, 0.0, 0.46, 0.0, 0.46, S * 0.8, 0.04, S * 0.8],
        ],
        '5' => &[
            [0.0, 1.0 - S, W, 1.0 - S, W, 1.0, 0.0, 1.0],
            [0.0, MID, S, MID, S, 1.0 - S, 0.0, 1.0 - S],
            [0.0, MID - S, W, MID - S, W, MID, 0.0, MID],
            [W - S, S, W, S, W, MID - S, W - S, MID - S],
            [0.0, 0.0, W, 0.0, W, S, 0.0, S],
        ],
        _ => unreachable!("boards only say 50, 100 and 150"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::{circuits, ribbon::STEP};

    fn every_track() -> impl Iterator<Item = (&'static str, Track)> {
        circuits::all()
            .iter()
            .map(|circuit| (circuit.name, Track::new(circuit)))
    }

    /// Whether the corner that starts at `entry` turns the road more than `by`
    /// within [`BIG_WITHIN`] somewhere, towards the side opposite `outside`.
    /// Measured between tangents rather than by adding up curvature.
    fn turns(track: &Track, entry: usize, by: f32, outside: f32) -> bool {
        let stations = track.ribbon.stations();
        let n = stations.len();
        let curvature: Vec<f32> = stations.iter().map(|s| s.curvature).collect();
        let corner = markers::corners_of(&curvature);
        let window = (BIG_WITHIN / STEP) as usize;
        (0..n).take_while(|&d| corner[(entry + d) % n]).any(|d| {
            let a = stations[(entry + d) % n].tangent;
            let b = stations[(entry + d + window) % n].tangent;
            // A right turn takes the tangent clockwise seen from above, which
            // is negative about Y; its outside is the left, which is negative.
            a.angle_between(b) > by && a.cross(b).y * outside > 0.0
        })
    }

    #[test]
    fn every_circuit_has_braking_boards() {
        for (name, track) in every_track() {
            assert!(!placed(&track).is_empty(), "{name} has no braking boards");
        }
    }

    /// Only before big corners, on their outside, at the distance the board
    /// says, and with the farthest missing rather than any in between.
    #[test]
    fn boards_count_down_to_big_corners_on_their_outside() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let n = stations.len();
            let step = track.ribbon.length() / n as f32;
            let boards = placed(&track);
            for board in &boards {
                assert!(
                    turns(&track, board.entry, BIG_TURN * 0.9, board.side),
                    "{name}: boards at station {} are not on the outside of a big corner",
                    board.at
                );
                let back = ((board.entry + n - board.at) % n) as f32 * step;
                let says = LABELS[board.label] as f32 * BOARD_SCALE;
                assert!(
                    (back - says).abs() <= step,
                    "{name}: the {} board is {back:.1} m out",
                    LABELS[board.label]
                );
                let centre = frame(&track, board).0;
                let lateral = track
                    .ground_from(centre, Some(stations[board.at].s))
                    .lateral;
                assert!(
                    lateral * board.side > HALF_WIDTH,
                    "{name}: board on the wrong verge"
                );
                assert!(
                    (0..board.label).all(|nearer| boards
                        .iter()
                        .any(|b| b.entry == board.entry && b.label == nearer)),
                    "{name}: the {} board without the ones nearer the corner",
                    LABELS[board.label]
                );
            }
        }
    }

    /// Clear of every road's kerb, and of every chevron plaque in either
    /// direction; upright, facing the way the lap comes, and on the ground.
    #[test]
    fn boards_stand_clear_of_the_road_and_the_plaques() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let plaques = markers::plaque_centres(&track);
            for board in placed(&track) {
                let (centre, normal, across) = frame(&track, &board);
                for point in footprint(&track, &board) {
                    let road = track.fix(point, None);
                    assert!(
                        road.lateral.abs() > HALF_WIDTH + CLEAR - 0.01,
                        "{name}: board within {:.2} m of a kerb",
                        road.lateral.abs() - HALF_WIDTH
                    );
                }
                for plaque in &plaques {
                    // The plaque's corners, in the board's frame.
                    let d = *plaque - centre;
                    let x = (d.dot(across).abs() - PANEL_HALF_WIDTH).max(0.0);
                    let z = d.dot(normal) + PANEL_HALF_DEPTH + POST_HALF;
                    let z = (z.abs() - PANEL_HALF_DEPTH - POST_HALF).max(0.0);
                    assert!(
                        x.hypot(z) > markers::HALF * std::f32::consts::SQRT_2 + 0.05,
                        "{name}: board at station {} overlaps a plaque",
                        board.at
                    );
                }
                assert!(normal.dot(stations[board.at].tangent) < -0.9);
                assert!(normal.dot(stations[board.at].right * board.side) < 0.0);
            }
            let [mesh, _] = crate::track::trackside::meshes_for(&track);
            let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap();
            let normals = mesh.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap();
            let (positions, normals) =
                (positions.as_float3().unwrap(), normals.as_float3().unwrap());
            for (triangle, normal) in positions.chunks_exact(3).zip(normals.chunks_exact(3)) {
                let [a, b, c] = [0, 1, 2].map(|i| Vec3::from(triangle[i]));
                assert!(
                    (b - a).cross(c - a).dot(Vec3::from(normal[0])) > 0.0,
                    "{name}: a board face is wound inside out"
                );
            }
        }
    }

    /// A corner just past the start/finish line counts back across it.
    #[test]
    fn boards_count_back_across_the_lap_seam() {
        let n = 2000;
        let mut curvature = vec![0.0; n];
        // A right hairpin from station 5: 24 m of 7 m radius, 190 degrees.
        curvature[5..65].fill(0.14);
        let entry = markers::corners_of(&curvature)
            .iter()
            .enumerate()
            .find(|&(i, &c)| c && !markers::corners_of(&curvature)[(i + n - 1) % n])
            .unwrap()
            .0;
        let boards = plan(&curvature, STEP);
        assert_eq!(boards.len(), 3);
        for board in boards {
            assert_eq!(board.entry, entry);
            assert!(board.at > n - 60, "board at {} did not wrap", board.at);
            let back = (entry + n - board.at) % n;
            assert_eq!(
                back,
                (LABELS[board.label] as f32 * BOARD_SCALE / STEP).round() as usize
            );
            assert_eq!(board.side, -1.0, "a right-hander's outside is the left");
        }
    }

    /// Where the previous corner leaves room for only two, the 150 board is the
    /// one that goes; the 50 and the 100 keep their distances.
    #[test]
    fn a_short_straight_drops_the_farthest_boards() {
        let n = 2000;
        let hairpin = |curvature: &mut Vec<f32>, from: usize, sign: f32| {
            curvature[from..from + 60].fill(sign * 0.13);
        };
        for (gap, expected) in [(200, 3), (76, 2), (60, 1), (45, 0)] {
            let mut curvature = vec![0.0; n];
            hairpin(&mut curvature, 500, 1.0);
            hairpin(&mut curvature, 560 + gap, -1.0);
            let boards: Vec<_> = plan(&curvature, STEP)
                .into_iter()
                .filter(|b| b.at > 560 && b.at < 560 + gap)
                .collect();
            assert_eq!(
                boards.iter().map(|b| b.label).collect::<Vec<_>>(),
                (0..expected).collect::<Vec<_>>(),
                "with {:.0} m between the hairpins",
                gap as f32 * STEP
            );
        }
    }
}
