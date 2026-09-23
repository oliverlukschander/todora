//! The start/finish straight's buildings: the pit wall, pit lane and garages
//! on one side, a covered grandstand with its crowd and flags on the other.
//!
//! Both are laid in short straight modules from station to station, so they
//! follow a straight that bends. A module that does not fit is left out.

use super::{
    Builder, Site,
    atlas::{self, Cell},
    beside, random, seed,
};
use crate::track::Track;
use bevy::prelude::*;

/// Stations per module: 3.2 m.
const MODULE: i64 = 8;
/// Stations either side of the line the pits and the grandstand span.
const PITS: (i64, i64) = (-40, 32);
const STAND: (i64, i64) = (-32, 32);

/// Metres beyond the kerb.
const PIT_WALL: (f32, f32) = (1.75, 1.87);
const PIT_WALL_HEIGHT: f32 = 0.32;
const APRON: (f32, f32) = (1.95, 3.6);
const APRON_LIFT: f32 = 0.03;
const GARAGE: (f32, f32) = (3.75, 6.2);
const GARAGE_HEIGHT: f32 = 1.35;
const FASCIA: f32 = 0.24;
const DOOR_HEIGHT: f32 = 0.85;

const STAND_FRONT: f32 = 2.7;
const ROWS: usize = 6;
const ROW_DEPTH: f32 = 0.42;
const RISE: f32 = 0.3;
const FRONT_WALL: f32 = 0.45;
const ROOF: f32 = 3.2;
const ROOF_DEPTH: f32 = 0.08;
const FLAG_POLE: f32 = 0.7;
/// Flag poles are thick enough to stay a pixel wide far off, so they hold
/// steady instead of breaking up and crawling as the car moves.
pub(super) const POLE_HALF: f32 = 0.025;

const CONCRETE: (f32, f32, f32) = (0.74, 0.74, 0.71);
const ASPHALT: (f32, f32, f32) = (0.29, 0.29, 0.31);
const WALL: (f32, f32, f32) = (0.86, 0.86, 0.84);
const DOOR: (f32, f32, f32) = (0.17, 0.18, 0.2);
const ROOF_TOP: (f32, f32, f32) = (0.36, 0.38, 0.4);
const ROOF_UNDER: (f32, f32, f32) = (0.8, 0.81, 0.83);
const POLE: (f32, f32, f32) = (0.8, 0.8, 0.8);
/// Team colours for the garage fascias and flags.
const TEAMS: [(f32, f32, f32); 6] = [
    (0.78, 0.12, 0.1),
    (0.1, 0.25, 0.62),
    (0.95, 0.72, 0.1),
    (0.1, 0.5, 0.35),
    (0.93, 0.45, 0.1),
    (0.55, 0.62, 0.68),
];

pub(super) fn build(track: &Track, site: &mut Site, out: &mut Builder) {
    // The pits go on whichever side has room for more of them.
    let room = |side: f32| {
        modules(PITS)
            .filter(|&at| site_fits(site, track, at, side, PIT_WALL.0, GARAGE.1))
            .count()
    };
    let pits = if room(-1.0) > room(1.0) { -1.0 } else { 1.0 };
    let seed = seed(track);
    let placed: Vec<i64> = modules(PITS)
        .filter(|&at| site_take(site, track, at, pits, PIT_WALL.0, GARAGE.1))
        .collect();
    for (k, &at) in placed.iter().enumerate() {
        let floor = floors(track, site, at, pits, PIT_WALL.0, GARAGE.1);
        pit(track, site, out, at, pits, floor, TEAMS[k % TEAMS.len()]);
    }
    let depth = ROWS as f32 * ROW_DEPTH + 0.3;
    let (near, far) = (STAND_FRONT - 0.3, STAND_FRONT + depth);
    let placed: Vec<i64> = modules(STAND)
        .filter(|&at| site_take(site, track, at, -pits, near, far))
        .collect();
    for &at in &placed {
        let floor = floors(track, site, at, -pits, near, far);
        stand(track, out, at, -pits, floor, seed);
    }
}

/// The first station of each module.
fn modules((from, to): (i64, i64)) -> impl Iterator<Item = i64> {
    (from..to).step_by(MODULE as usize)
}

/// Points covering the module's plan, `near` to `far` metres beyond the kerb.
/// The last column stops short of the next module's first, so neighbours fit.
fn outline(track: &Track, at: i64, side: f32, near: f32, far: f32) -> Vec<Vec3> {
    let mut points = Vec::new();
    let steps = ((far - near) / 0.6).ceil() as usize;
    for s in [at, at + MODULE / 2, at + MODULE - 2] {
        for d in 0..=steps {
            let out = near + (far - near) * d as f32 / steps as f32;
            points.push(beside(track, s, side, out));
        }
    }
    points
}

const MODULE_RADIUS: f32 = 0.3;
/// Extra room from any kerb, for the corners between the points checked.
const MODULE_GAP: f32 = 0.4;

fn site_fits(site: &Site, track: &Track, at: i64, side: f32, near: f32, far: f32) -> bool {
    outline(track, at, side, near, far)
        .iter()
        .all(|&p| site.fits(p, MODULE_RADIUS, MODULE_GAP))
}

fn site_take(site: &mut Site, track: &Track, at: i64, side: f32, near: f32, far: f32) -> bool {
    site.take(
        &outline(track, at, side, near, far),
        MODULE_RADIUS,
        MODULE_GAP,
    )
}

/// The floor at each end of a module: the lowest ground across the building
/// at that station, a little sunk. Neighbours share the station between them
/// and so the floor, and the module slopes from one to the other, so a run of
/// them follows the ground without a step or an overlap anywhere.
fn floors(track: &Track, site: &Site, at: i64, side: f32, near: f32, far: f32) -> (f32, f32) {
    let across = |s: i64| {
        let points: Vec<Vec3> = (0..=6)
            .map(|d| beside(track, s, side, near + (far - near) * d as f32 / 6.0))
            .collect();
        site.lowest(&points) - 0.05
    };
    (across(at), across(at + MODULE))
}

/// A plan point at fraction `u` along the module and `out` beyond the kerb.
fn point(track: &Track, at: i64, side: f32, u: f32, out: f32) -> Vec3 {
    beside(track, at, side, out).lerp(beside(track, at + MODULE, side, out), u)
}

/// A strip of a face: from, to along it, and its colour.
type Tile = (f32, f32, (f32, f32, f32));

fn pit(
    track: &Track,
    site: &Site,
    out: &mut Builder,
    at: i64,
    side: f32,
    (h0, h1): (f32, f32),
    team: (f32, f32, f32),
) {
    // A point `u` along, `o` beyond the kerb, `y` above the floor there.
    let p = |u: f32, o: f32, y: f32| {
        let q = point(track, at, side, u, o);
        Vec3::new(q.x, h0 + (h1 - h0) * u + y, q.z)
    };
    let toward_road = (p(0.5, 0.0, 0.0) - p(0.5, 1.0, 0.0)).normalize();
    let wall_top = PIT_WALL_HEIGHT + 0.05;
    let box_at = |u0: f32, u1: f32, near: f32, far: f32, y: f32| {
        [p(u0, near, y), p(u1, near, y), p(u1, far, y), p(u0, far, y)]
    };
    out.prism(
        box_at(0.0, 1.0, PIT_WALL.0, PIT_WALL.1, 0.0),
        box_at(0.0, 1.0, PIT_WALL.0, PIT_WALL.1, wall_top),
        CONCRETE,
        None,
    );
    // Pit lane, following the ground at each corner.
    let lane = |u: f32, o: f32| {
        let q = point(track, at, side, u, o);
        Vec3::new(q.x, site.ground(q) + APRON_LIFT, q.z)
    };
    for (u0, u1) in [(0.0, 0.5), (0.5, 1.0)] {
        out.lit(
            [
                lane(u0, APRON.0),
                lane(u1, APRON.0),
                lane(u1, APRON.1),
                lane(u0, APRON.1),
            ],
            Vec3::Y,
            ASPHALT,
        );
    }
    // Garages, their front tiled into wall, doors and fascia rather than
    // having any of it laid over it.
    let top = GARAGE_HEIGHT + 0.05;
    out.prism(
        box_at(0.0, 1.0, GARAGE.0, GARAGE.1, 0.0),
        box_at(0.0, 1.0, GARAGE.0, GARAGE.1, top),
        WALL,
        Some(0),
    );
    let door = DOOR_HEIGHT + 0.05;
    let rows: [(f32, f32, &[Tile]); 4] = [
        (
            0.0,
            door,
            &[
                (0.0, 0.08, WALL),
                (0.08, 0.46, DOOR),
                (0.46, 0.54, WALL),
                (0.54, 0.92, DOOR),
                (0.92, 1.0, WALL),
            ],
        ),
        (door, top - FASCIA, &[(0.0, 1.0, WALL)]),
        (top - FASCIA, top - 0.03, &[(0.0, 1.0, team)]),
        (top - 0.03, top, &[(0.0, 1.0, WALL)]),
    ];
    for (y0, y1, tiles) in rows {
        for &(u0, u1, colour) in tiles {
            out.lit(
                [
                    p(u0, GARAGE.0, y0),
                    p(u1, GARAGE.0, y0),
                    p(u1, GARAGE.0, y1),
                    p(u0, GARAGE.0, y1),
                ],
                toward_road,
                colour,
            );
        }
    }
    // A flat roof with an overhang over the lane, seen from below too.
    let (near, far) = (GARAGE.0 - 0.3, GARAGE.1 + 0.05);
    out.prism(
        box_at(0.0, 1.0, near, far, top),
        box_at(0.0, 1.0, near, far, top + ROOF_DEPTH),
        ROOF_TOP,
        None,
    );
    out.lit(box_at(0.0, 1.0, near, far, top), -Vec3::Y, ROOF_UNDER);
}

fn stand(track: &Track, out: &mut Builder, at: i64, side: f32, (h0, h1): (f32, f32), seed: u64) {
    // A point `u` along, `v` back from the front, `y` above the floor there.
    let p = |u: f32, v: f32, y: f32| {
        let q = point(track, at, side, u, STAND_FRONT + v);
        Vec3::new(q.x, h0 + (h1 - h0) * u + y, q.z)
    };
    let back = ROWS as f32 * ROW_DEPTH;
    let toward_road = (p(0.5, 0.0, 0.0) - p(0.5, 1.0, 0.0)).normalize();
    let along = (p(1.0, 0.0, 0.0) - p(0.0, 0.0, 0.0)).normalize();
    let height = |row: usize| FRONT_WALL + (row + 1) as f32 * RISE;
    let face =
        |v: f32, y0: f32, y1: f32| [p(0.0, v, y0), p(1.0, v, y0), p(1.0, v, y1), p(0.0, v, y1)];
    out.lit(face(0.0, 0.0, FRONT_WALL), toward_road, CONCRETE);
    for row in 0..ROWS {
        let (v0, v1) = (row as f32 * ROW_DEPTH, (row + 1) as f32 * ROW_DEPTH);
        let (low, high) = (height(row) - RISE, height(row));
        // The riser is the crowd, from the atlas.
        let crowd = (random(seed, at, row as i64) * atlas::CROWDS as f32) as usize;
        let (left, right) = if along.dot((-toward_road).cross(Vec3::Y)) > 0.0 {
            (0.0, 1.0)
        } else {
            (1.0, 0.0)
        };
        out.sign(
            [
                p(left, v0, low),
                p(right, v0, low),
                p(right, v0, high),
                p(left, v0, high),
            ],
            toward_road,
            Cell::Crowd(crowd),
        );
        out.lit(
            [
                p(0.0, v0, high),
                p(1.0, v0, high),
                p(1.0, v1, high),
                p(0.0, v1, high),
            ],
            Vec3::Y,
            CONCRETE,
        );
        // Stepped ends.
        for (u, facing) in [(0.0, -along), (1.0, along)] {
            out.lit(
                [p(u, v0, 0.0), p(u, v1, 0.0), p(u, v1, high), p(u, v0, high)],
                facing,
                CONCRETE,
            );
        }
    }
    let top = height(ROWS - 1);
    let box_at =
        |v0: f32, v1: f32, y: f32| [p(0.0, v0, y), p(1.0, v0, y), p(1.0, v1, y), p(0.0, v1, y)];
    // Back wall and walkway.
    out.prism(
        box_at(back, back + 0.3, 0.0),
        box_at(back, back + 0.3, top),
        CONCRETE,
        None,
    );
    // Roof on two pillars at the back, seen from below too.
    for u in [0.12, 0.88] {
        let foot = p(u, back + 0.15, top);
        out.column(
            foot,
            foot.y,
            p(u, back + 0.15, ROOF).y,
            along * 0.05,
            toward_road * 0.05,
            [POLE, POLE, POLE],
            false,
        );
    }
    out.prism(
        box_at(-0.25, back + 0.35, ROOF),
        box_at(-0.25, back + 0.35, ROOF + ROOF_DEPTH),
        ROOF_TOP,
        None,
    );
    out.lit(box_at(-0.25, back + 0.35, ROOF), -Vec3::Y, ROOF_UNDER);
    // A flag on the front of the roof.
    let pole = p(0.5, -0.15, ROOF + ROOF_DEPTH);
    let peak = pole.y + FLAG_POLE;
    out.column(
        pole,
        pole.y - ROOF_DEPTH,
        peak,
        along * POLE_HALF,
        toward_road * POLE_HALF,
        [POLE, POLE, POLE],
        true,
    );
    let colour = TEAMS[(random(seed, at, 99) * 60.0) as usize % TEAMS.len()];
    flag(out, pole, peak, along, toward_road, colour);
}

/// A flag flying off a pole top, bent once as if in a breeze, both faces.
pub(super) fn flag(
    out: &mut Builder,
    pole: Vec3,
    peak: f32,
    along: Vec3,
    across: Vec3,
    colour: (f32, f32, f32),
) {
    const LENGTH: f32 = 0.36;
    const DEPTH: f32 = 0.22;
    let at = |x: f32, kink: f32, y: f32| (pole + along * x + across * kink).with_y(y);
    let halves = [
        [
            at(0.0, 0.0, peak - DEPTH),
            at(LENGTH / 2.0, 0.04, peak - DEPTH),
            at(LENGTH / 2.0, 0.04, peak),
            at(0.0, 0.0, peak),
        ],
        [
            at(LENGTH / 2.0, 0.04, peak - DEPTH),
            at(LENGTH, 0.0, peak - DEPTH - 0.02),
            at(LENGTH, 0.0, peak - 0.02),
            at(LENGTH / 2.0, 0.04, peak),
        ],
    ];
    for corners in halves {
        let normal = (corners[1] - corners[0])
            .cross(corners[3] - corners[0])
            .normalize();
        out.lit(corners, normal, colour);
        out.lit(corners, -normal, colour);
    }
}
