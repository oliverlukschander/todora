//! Marshal posts: a small white hut with an orange roof and a yellow flag on
//! the outside of every big corner, behind the tyre wall, and one halfway down
//! each long straight.

use super::{Builder, Site, beside, stands::flag};
use crate::track::{Track, boards, markers};
use bevy::prelude::*;

/// Metres beyond the kerb to try, nearest first.
const OUT: [f32; 3] = [3.35, 4.1, 4.9];
const STRAIGHT_OUT: [f32; 2] = [2.3, 3.0];
/// A straight this long gets a post halfway down it.
const LONG_STRAIGHT: f32 = 50.0;
const HALF_ALONG: f32 = 0.25;
const HALF_DEEP: f32 = 0.22;
const HEIGHT: f32 = 0.42;
const ROOF_OVER: f32 = 0.05;
const POLE: f32 = 1.0;

const WALLS: (f32, f32, f32) = (0.9, 0.9, 0.88);
const WINDOW: (f32, f32, f32) = (0.16, 0.2, 0.24);
const ROOF: (f32, f32, f32) = (0.95, 0.45, 0.1);
const POLE_COLOUR: (f32, f32, f32) = (0.8, 0.8, 0.8);
const YELLOW: (f32, f32, f32) = (0.98, 0.84, 0.1);

pub(super) fn build(track: &Track, site: &mut Site, out: &mut Builder) {
    let stations = track.ribbon.stations();
    let n = stations.len();
    let step = track.ribbon.length() / n as f32;
    let curvature: Vec<f32> = stations.iter().map(|s| s.curvature).collect();
    for corner in boards::big_corners(&curvature, step) {
        let at = (corner.entry + corner.length / 2) % n;
        post(track, site, out, at, corner.outside, &OUT);
    }
    let corners = markers::corners_of(&curvature);
    for head in 0..n {
        if corners[head] || !corners[(head + n - 1) % n] {
            continue;
        }
        let length = (0..n).take_while(|&d| !corners[(head + d) % n]).count();
        if length as f32 * step >= LONG_STRAIGHT {
            // Across the road from the advertising, which takes the outside
            // of the corner the straight leads into.
            let end = head + length;
            let turn: f32 = (0..50).map(|d| curvature[(end + d) % n]).sum();
            post(
                track,
                site,
                out,
                (head + length / 2) % n,
                turn.signum(),
                &STRAIGHT_OUT,
            );
        }
    }
}

fn post(track: &Track, site: &mut Site, out: &mut Builder, at: usize, side: f32, tries: &[f32]) {
    let station = &track.ribbon.stations()[at];
    let along = station.tangent;
    let outward = station.right * side;
    for &distance in tries {
        let centre = beside(track, at as i64, side, distance + HALF_DEEP);
        if !site.fits(centre, 0.5, 0.5) {
            continue;
        }
        site.claim(centre, 0.5);
        hut(site, out, centre, along, outward);
        return;
    }
}

fn hut(site: &Site, out: &mut Builder, centre: Vec3, along: Vec3, outward: Vec3) {
    let corner = |a: f32, d: f32| centre + along * a + outward * d;
    let plan = [
        corner(-HALF_ALONG, -HALF_DEEP),
        corner(HALF_ALONG, -HALF_DEEP),
        corner(HALF_ALONG, HALF_DEEP),
        corner(-HALF_ALONG, HALF_DEEP),
    ];
    let ground = site.lowest(&plan);
    let top = ground + HEIGHT + 0.05;
    // The side facing the road is tiled around a window, not painted over.
    out.open_block(plan, ground - 0.05, top, WALLS, Some(0));
    let wall = |a: f32, y: f32| (centre + along * a - outward * HALF_DEEP).with_y(y);
    let (sill, lintel) = (top - 0.2, top - 0.08);
    let (left, right) = (-HALF_ALONG + 0.05, HALF_ALONG - 0.05);
    for (a0, a1, y0, y1, colour) in [
        (-HALF_ALONG, HALF_ALONG, ground - 0.05, sill, WALLS),
        (-HALF_ALONG, left, sill, lintel, WALLS),
        (left, right, sill, lintel, WINDOW),
        (right, HALF_ALONG, sill, lintel, WALLS),
        (-HALF_ALONG, HALF_ALONG, lintel, top, WALLS),
    ] {
        out.lit(
            [wall(a0, y0), wall(a1, y0), wall(a1, y1), wall(a0, y1)],
            -outward,
            colour,
        );
    }
    let over = HALF_ALONG + ROOF_OVER;
    let roof = [
        corner(-over, -HALF_DEEP - ROOF_OVER),
        corner(over, -HALF_DEEP - ROOF_OVER),
        corner(over, HALF_DEEP + ROOF_OVER),
        corner(-over, HALF_DEEP + ROOF_OVER),
    ];
    out.block(roof, top, top + 0.05, ROOF);
    // The flag pole at the end the traffic comes from.
    let pole = corner(-HALF_ALONG - 0.08, -HALF_DEEP + 0.05);
    let peak = ground + POLE;
    out.column(
        pole,
        ground - 0.05,
        peak,
        along * super::stands::POLE_HALF,
        outward * super::stands::POLE_HALF,
        [POLE_COLOUR; 3],
        true,
    );
    flag(out, pole, peak, -along, outward, YELLOW);
}
