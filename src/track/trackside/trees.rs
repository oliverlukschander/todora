//! Trees and bushes out on the grass, in clumps and woods rather than evenly
//! sprinkled, the same every time a circuit is built.
//!
//! Trees keep well back from every road so they never stand between the chase
//! camera and the car; bushes are low and may come nearer.

use super::{Builder, Site, random, seed};
use crate::track::Track;
use bevy::prelude::*;
use std::f32::consts::TAU;

/// Candidate spacing, and the size of the clumps the noise makes.
const SPACING: f32 = 4.5;
const CLUMP: f32 = 28.0;
/// Noise above this is wood; the rest is open grass with the odd bush.
const WOOD: f32 = 0.52;
const MAX_TREES: usize = 520;
const MAX_BUSHES: usize = 300;
/// Beyond the kerb of any road: trees, and bushes.
const TREE_GAP: f32 = 6.5;
const BUSH_GAP: f32 = 3.0;
const TREE_RADIUS: f32 = 1.0;
const BUSH_RADIUS: f32 = 0.5;

const TRUNK: (f32, f32, f32) = (0.33, 0.24, 0.16);
const PINE: (f32, f32, f32) = (0.12, 0.29, 0.15);
const LEAVES: (f32, f32, f32) = (0.24, 0.43, 0.17);
const BUSH: (f32, f32, f32) = (0.2, 0.37, 0.14);

#[derive(Clone, Copy)]
enum Kind {
    Pine,
    Leafy,
    Bush,
}

pub(super) fn build(track: &Track, site: &mut Site, out: &mut Builder) {
    let seed = seed(track);
    let (min, max) = (site.min, site.max);
    let columns = ((max.x - min.x) / SPACING) as i64;
    let rows = ((max.y - min.y) / SPACING) as i64;
    let mut trees = Vec::new();
    let mut bushes = Vec::new();
    for i in 0..columns {
        for j in 0..rows {
            let jitter = Vec2::new(random(seed, i, j * 2), random(seed, i, j * 2 + 1));
            let p = min + (Vec2::new(i as f32, j as f32) + jitter) * SPACING;
            let wood = noise(seed, p / CLUMP);
            let roll = random(seed ^ 1, i, j);
            let key = random(seed ^ 2, i, j);
            if wood > WOOD && roll < (wood - WOOD) * 4.0 {
                let pines = noise(seed ^ 3, p / (CLUMP * 2.5));
                let kind = if pines > 0.5 { Kind::Pine } else { Kind::Leafy };
                trees.push((key, p, kind));
            } else if wood > 0.3 && roll < 0.1 {
                bushes.push((key, p, Kind::Bush));
            }
        }
    }
    // Keep the budget without favouring one corner of the map.
    for (list, cap) in [(&mut trees, MAX_TREES), (&mut bushes, MAX_BUSHES)] {
        list.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut kept = 0;
        for &(key, p, kind) in list.iter() {
            if kept == cap {
                break;
            }
            let (radius, gap) = match kind {
                Kind::Bush => (BUSH_RADIUS, BUSH_GAP),
                _ => (TREE_RADIUS, TREE_GAP),
            };
            let point = Vec3::new(p.x, 0.0, p.y);
            if !site.fits(point, radius, gap) {
                continue;
            }
            site.claim(point, radius);
            let base = point.with_y(site.ground(point));
            let scale = 0.75 + 0.55 * key;
            let turn = random(seed ^ 4, (p.x * 10.0) as i64, (p.y * 10.0) as i64) * TAU;
            let tint = 0.85 + 0.3 * random(seed ^ 5, (p.x * 10.0) as i64, (p.y * 10.0) as i64);
            match kind {
                Kind::Pine => pine(out, base, scale, turn, tint),
                Kind::Leafy => leafy(out, base, scale, turn, tint),
                Kind::Bush => bush(out, base, scale, turn, tint),
            }
            kept += 1;
        }
    }
}

/// Smooth value noise in [0, 1].
fn noise(seed: u64, p: Vec2) -> f32 {
    let (x, y) = (p.x.floor(), p.y.floor());
    let (fx, fy) = (p.x - x, p.y - y);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let at = |dx: f32, dy: f32| random(seed, (x + dx) as i64, (y + dy) as i64);
    let top = at(0.0, 0.0) + (at(1.0, 0.0) - at(0.0, 0.0)) * s(fx);
    let bottom = at(0.0, 1.0) + (at(1.0, 1.0) - at(0.0, 1.0)) * s(fx);
    top + (bottom - top) * s(fy)
}

fn tinted(colour: (f32, f32, f32), tint: f32) -> (f32, f32, f32) {
    (colour.0 * tint, colour.1 * tint, colour.2 * tint)
}

/// A ring of `sides` points round `centre`, turned by `turn`.
fn ring(centre: Vec3, radius: f32, sides: usize, turn: f32) -> Vec<Vec3> {
    (0..sides)
        .map(|k| {
            let a = turn + TAU * k as f32 / sides as f32;
            centre + Vec3::new(a.cos(), 0.0, a.sin()) * radius
        })
        .collect()
}

/// A cone or a band between two rings (a ring of one point is an apex),
/// shaded face by face as seen from outside `axis`.
fn band(out: &mut Builder, lower: &[Vec3], upper: &[Vec3], axis: Vec3, colour: (f32, f32, f32)) {
    let sides = lower.len().max(upper.len());
    for k in 0..sides {
        let l = |i: usize| lower[i % lower.len()];
        let u = |i: usize| upper[i % upper.len()];
        if lower.len() > 1 {
            out.lit_triangle([l(k), l(k + 1), u(k)], axis, colour);
        }
        if upper.len() > 1 {
            out.lit_triangle([u(k), l(k + 1), u(k + 1)], axis, colour);
        }
    }
}

fn trunk(out: &mut Builder, base: Vec3, height: f32, half: f32, turn: f32) {
    let foot = ring(base - Vec3::Y * 0.1, half, 4, turn);
    let head = ring(base + Vec3::Y * height, half, 4, turn);
    band(out, &foot, &head, base + Vec3::Y * height / 2.0, TRUNK);
}

fn pine(out: &mut Builder, base: Vec3, s: f32, turn: f32, tint: f32) {
    trunk(out, base, 0.5 * s, 0.07 * s, turn);
    for (radius, from, to) in [(0.95, 0.35, 2.0), (0.7, 1.2, 2.9)] {
        let skirt = ring(base + Vec3::Y * from * s, radius * s, 6, turn);
        let apex = [base + Vec3::Y * to * s];
        band(
            out,
            &skirt,
            &apex,
            base + Vec3::Y * from * s,
            tinted(PINE, tint),
        );
    }
}

fn leafy(out: &mut Builder, base: Vec3, s: f32, turn: f32, tint: f32) {
    trunk(out, base, 1.0 * s, 0.09 * s, turn);
    let centre = base + Vec3::Y * 1.65 * s;
    let (r, h) = (0.85 * s, 0.75 * s);
    let colour = tinted(LEAVES, tint);
    let bottom = [centre - Vec3::Y * h * 0.8];
    let lower = ring(centre - Vec3::Y * h * 0.3, r * 0.9, 5, turn);
    let upper = ring(centre + Vec3::Y * h * 0.35, r * 0.8, 5, turn + TAU / 10.0);
    let top = [centre + Vec3::Y * h];
    band(out, &bottom, &lower, centre, colour);
    band(out, &lower, &upper, centre, colour);
    band(out, &upper, &top, centre, colour);
}

fn bush(out: &mut Builder, base: Vec3, s: f32, turn: f32, tint: f32) {
    let centre = base + Vec3::Y * 0.2 * s;
    let colour = tinted(BUSH, tint);
    let foot = ring(base - Vec3::Y * 0.05, 0.4 * s, 5, turn);
    let waist = ring(centre, 0.55 * s, 5, turn + TAU / 10.0);
    let top = [base + Vec3::Y * 0.55 * s];
    band(out, &foot, &waist, centre, colour);
    band(out, &waist, &top, centre, colour);
}
