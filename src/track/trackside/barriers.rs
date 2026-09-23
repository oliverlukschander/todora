//! Tyre walls round the outside of big corners, and advertising boards along
//! the straights.
//!
//! Both are laid along the road station by station, so they keep an even
//! distance from it round a bend, and break off wherever anything else is.

use super::{
    Builder, Site,
    atlas::{self, Cell},
    beside, random, seed,
};
use crate::track::{Track, boards, markers, profile::HALF_WIDTH};
use bevy::prelude::*;

/// Stations between tyre wall nodes: 0.8 m.
const TYRE_STRIDE: usize = 2;
/// The wall's face, metres beyond the kerb, and how thick and tall it is.
const TYRE_OUT: f32 = 2.4;
const TYRE_DEEP: f32 = 0.34;
const TYRE_HEIGHT: f32 = 0.26;
/// How far past the end of the corner the wall carries on.
const TYRE_RUN_OUT: usize = 12;
const TYRE_BLACK: (f32, f32, f32) = (0.09, 0.09, 0.1);

/// A straight has to be this long to carry boards...
const LONG_STRAIGHT: f32 = 30.0;
/// ...and they keep this far from each end of it.
const STRAIGHT_MARGIN: usize = 10;
/// Stations per board and gap after it: 2.8 m and 0.4 m.
const BOARD: usize = 7;
const BOARD_GAP: usize = 1;
const BOARD_OUT: f32 = 1.95;
const BOARD_HEIGHT: f32 = 0.34;
const BOARD_DEEP: f32 = 0.05;
const BOARD_BACK: (f32, f32, f32) = (0.22, 0.23, 0.24);

pub(super) fn tyre_walls(track: &Track, site: &mut Site, out: &mut Builder) {
    let stations = track.ribbon.stations();
    let n = stations.len();
    let curvature: Vec<f32> = stations.iter().map(|s| s.curvature).collect();
    let step = track.ribbon.length() / n as f32;
    for corner in boards::big_corners(&curvature, step) {
        let side = corner.outside;
        let mut run: Vec<(i64, Vec3, Vec3)> = Vec::new();
        let mut belt = 0;
        let mut finish = |run: &mut Vec<(i64, Vec3, Vec3)>, site: &mut Site, out: &mut Builder| {
            if run.len() >= 4 {
                wall(site, out, run, &mut belt);
            }
            run.clear();
        };
        for d in (0..corner.length + TYRE_RUN_OUT).step_by(TYRE_STRIDE) {
            let at = (corner.entry + d) as i64;
            let station = &stations[at.rem_euclid(n as i64) as usize];
            // Where this stretch bends the other way tighter than the wall is
            // far out, the wall would fold over itself: break it there.
            let folds = station.curvature * side > 0.0
                && station.curvature.abs() * (HALF_WIDTH + TYRE_OUT + TYRE_DEEP + 1.0) > 1.0;
            let inner = beside(track, at, side, TYRE_OUT);
            let outer = beside(track, at, side, TYRE_OUT + TYRE_DEEP);
            let centre = (inner + outer) / 2.0;
            if !folds && site.fits(centre, TYRE_DEEP / 2.0 + 0.1, 0.3) {
                run.push((at, inner, outer));
            } else {
                finish(&mut run, site, out);
            }
        }
        finish(&mut run, site, out);
    }
}

/// One unbroken wall through `run`'s nodes: a black stack with a belt round
/// its face, capped at both ends.
fn wall(site: &mut Site, out: &mut Builder, run: &[(i64, Vec3, Vec3)], belt: &mut usize) {
    for &(_, inner, outer) in run {
        site.claim((inner + outer) / 2.0, TYRE_DEEP / 2.0 + 0.1);
    }
    let y = |p: Vec3, h: f32| Vec3::new(p.x, site.ground(p) + h, p.z);
    for pair in run.windows(2) {
        let [(_, a, a2), (_, b, b2)] = [pair[0], pair[1]];
        let toward_road = (a - a2 + b - b2).normalize();
        // The face is the tyres and their belt, from the atlas.
        let (left, right) = left_to_right(a, b, toward_road);
        out.sign(
            [
                y(left, -0.06),
                y(right, -0.06),
                y(right, TYRE_HEIGHT),
                y(left, TYRE_HEIGHT),
            ],
            toward_road,
            Cell::Belt(*belt % atlas::BELTS),
        );
        out.lit(
            [
                y(a2, -0.06),
                y(b2, -0.06),
                y(b2, TYRE_HEIGHT),
                y(a2, TYRE_HEIGHT),
            ],
            -toward_road,
            TYRE_BLACK,
        );
        out.lit(
            [
                y(a, TYRE_HEIGHT),
                y(b, TYRE_HEIGHT),
                y(b2, TYRE_HEIGHT),
                y(a2, TYRE_HEIGHT),
            ],
            Vec3::Y,
            TYRE_BLACK,
        );
        *belt += 1;
    }
    for (end, next) in [(run[0], run[1]), (run[run.len() - 1], run[run.len() - 2])] {
        let (_, inner, outer) = end;
        let facing = (end.1 - next.1).with_y(0.0);
        out.lit(
            [
                y(inner, -0.06),
                y(outer, -0.06),
                y(outer, TYRE_HEIGHT),
                y(inner, TYRE_HEIGHT),
            ],
            facing,
            TYRE_BLACK,
        );
    }
}

pub(super) fn hoardings(track: &Track, site: &mut Site, out: &mut Builder) {
    let stations = track.ribbon.stations();
    let n = stations.len();
    let step = track.ribbon.length() / n as f32;
    let curvature: Vec<f32> = stations.iter().map(|s| s.curvature).collect();
    let corner = markers::corners_of(&curvature);
    let seed = seed(track);
    let long = (LONG_STRAIGHT / step) as usize;
    for head in 0..n {
        if corner[head] || !corner[(head + n - 1) % n] {
            continue;
        }
        let length = (0..n).take_while(|&d| !corner[(head + d) % n]).count();
        if length < long {
            continue;
        }
        // On the outside of the corner the straight leads into, behind its
        // braking boards.
        let end = head + length;
        let turn: f32 = (0..50).map(|d| curvature[(end + d) % n]).sum();
        let side = -turn.signum();
        let mut at = head + STRAIGHT_MARGIN;
        let mut index = 0;
        while at + BOARD + STRAIGHT_MARGIN <= end {
            let (a, b) = (at as i64, (at + BOARD) as i64);
            let ends = [
                beside(track, a, side, BOARD_OUT),
                beside(track, b, side, BOARD_OUT),
            ];
            // Claimed short of the ends, so the next board along still fits.
            let claims = [a + 1, (a + b) / 2, b - 1].map(|s| beside(track, s, side, BOARD_OUT));
            if site.take(&claims, 0.3, 0.3) {
                // Two or three of a brand in a row, as they are sold.
                let brand = (random(seed, head as i64, index / 3) * atlas::BRANDS as f32) as usize;
                hoarding(
                    site,
                    out,
                    ends[0],
                    ends[1],
                    -stations[at % n].right * side,
                    brand,
                );
            }
            at += BOARD + BOARD_GAP;
            index += 1;
        }
    }
}

/// One board from `a` to `b` on the ground. Its face towards the road is the
/// brand from the atlas, standing in for the board's own front.
fn hoarding(site: &Site, out: &mut Builder, a: Vec3, b: Vec3, toward_road: Vec3, brand: usize) {
    let (ga, gb) = (site.ground(a), site.ground(b));
    let low = ga.min(gb) - 0.04;
    let top = ga.max(gb) + BOARD_HEIGHT;
    let back = -toward_road * BOARD_DEEP;
    out.open_block([a, b, b + back, a + back], low, top, BOARD_BACK, Some(0));
    let at = |p: Vec3, y: f32| Vec3::new(p.x, y, p.z);
    let (left, right) = left_to_right(a, b, toward_road);
    out.sign(
        [at(left, low), at(right, low), at(right, top), at(left, top)],
        toward_road,
        Cell::Brand(brand),
    );
}

/// `a` and `b` ordered left to right as seen by someone facing `-facing`.
fn left_to_right(a: Vec3, b: Vec3, facing: Vec3) -> (Vec3, Vec3) {
    let right = (-facing).cross(Vec3::Y);
    if (b - a).dot(right) > 0.0 {
        (a, b)
    } else {
        (b, a)
    }
}
