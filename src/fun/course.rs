//! What is on the road, and where.
//!
//! A circuit is a lap of asphalt and this decides what else is on it: boost
//! pads and jump pads on the straights, mystery boxes in rows of three, and
//! set pieces for the car to plough through — a slalom of cones, ten bowling
//! pins in a triangle, cows crossing, a parade of ducks, a balloon arch — and
//! tube-men waving from the verge.
//!
//! It is a plain function of the [`Track`]: no entities, no clock, no chance
//! except a generator seeded from the circuit's own id. A circuit gets the same
//! road furniture on every visit, and the tests can look at all of it at once.
//! Everything keeps clear of the start line and its grid, of bridges, and of
//! corners, where it would be a hazard and not a joke.

use bevy::prelude::*;

use super::rng::Rng;
use crate::track::{ROAD_HALF, Spot, Track};

/// Metres after the start line, and before it, that stay clear: the grid, the
/// run-up and the gantry.
pub(crate) const AFTER_LINE: f32 = 80.0;
pub(crate) const BEFORE_LINE: f32 = 45.0;
/// How straight a stretch has to be for a pad to go on it: the largest
/// curvature, in 1/m, that is allowed anywhere on it.
const STRAIGHT: f32 = 0.032;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PadKind {
    Boost,
    Jump,
}

/// A pad in the road.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PadSpot {
    pub kind: PadKind,
    pub at: Vec3,
    pub tangent: Vec3,
    pub right: Vec3,
    pub s: f32,
}

/// A mystery box, hung over the road.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BoxSpot {
    pub at: Vec3,
    pub s: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PropKind {
    Cone,
    Pin,
    Cow,
    Duck,
    Melon,
    Crate,
    Balloon,
}

/// Something to hit.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PropSpot {
    pub kind: PropKind,
    pub at: Vec3,
    pub s: f32,
    /// Which way it faces, about Y.
    pub yaw: f32,
    /// Which set piece it belongs to, so a strike can be told from ten hits.
    pub group: u16,
}

/// A tube-man on the verge.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ManSpot {
    pub at: Vec3,
    pub s: f32,
    pub tint: u8,
}

/// The lot.
#[derive(Clone, Debug, Default)]
pub(crate) struct Course {
    pub pads: Vec<PadSpot>,
    pub boxes: Vec<BoxSpot>,
    pub props: Vec<PropSpot>,
    pub men: Vec<ManSpot>,
}

/// The worst curvature over `from..to` metres of the lap.
fn worst_bend(track: &Track, from: f32, to: f32) -> f32 {
    let mut worst = 0.0f32;
    let mut s = from;
    while s <= to {
        worst = worst.max(track.spot_at(s).curvature.abs());
        s += 4.0;
    }
    worst
}

/// The steepest the road is over `from..to` metres, rise over run.
fn worst_grade(track: &Track, from: f32, to: f32) -> f32 {
    let (mut worst, mut s) = (0.0f32, from);
    while s <= to {
        let (a, b) = (track.spot_at(s), track.spot_at(s + 4.0));
        worst = worst.max(((b.pos.y - a.pos.y) / 4.0).abs());
        s += 4.0;
    }
    worst
}

/// Whether everything from `from` to `to` metres round the lap is somewhere
/// things may stand: clear of the line, and of bridges.
fn open(track: &Track, from: f32, to: f32) -> bool {
    let lap = track.length();
    if from < AFTER_LINE || to > lap - BEFORE_LINE {
        return false;
    }
    let mut at = from;
    while at <= to + 4.0 {
        if track.bridge_at(at.min(lap - 1.0)) {
            return false;
        }
        at += 6.0;
    }
    true
}

fn on_the_road(spot: Spot, lateral: f32) -> Vec3 {
    spot.pos + spot.right * lateral
}

/// Lay out a circuit.
pub(crate) fn plan(track: &Track) -> Course {
    let lap = track.length();
    let mut rng = Rng::of(track.circuit().id, 0xC0_5E);
    let mut course = Course::default();
    // Places already taken, as distances round the lap: nothing shares a stretch.
    let mut taken: Vec<f32> = Vec::new();
    let free = |taken: &[f32], s: f32, gap: f32| {
        taken.iter().all(|t| {
            let d = (t - s).abs();
            d.min(lap - d) >= gap
        })
    };

    // Pads first, because they need the straightest ground.
    let boosts = ((lap / 260.0).round() as usize).clamp(2, 8);
    let jumps = ((lap / 560.0).round() as usize).clamp(1, 3);
    for (kind, count, before, after) in [
        (PadKind::Jump, jumps, 24.0, 85.0),
        (PadKind::Boost, boosts, 18.0, 42.0),
    ] {
        for slot in 0..count {
            // The straightest place near where this slot would put it.
            let ideal = lap * (slot as f32 + 0.5 + rng.range(-0.2, 0.2)) / count as f32;
            let mut best: Option<(f32, f32)> = None;
            for step in 0..60 {
                let offset = (step as f32 - 30.0) * 5.0;
                let s = (ideal + offset).rem_euclid(lap);
                if !open(track, s - 8.0, s + 8.0)
                    || !free(&taken, s, 50.0)
                    || worst_grade(track, s - before, s + after) > 0.28
                {
                    continue;
                }
                let bend = worst_bend(track, s - before, s + after);
                if bend > STRAIGHT {
                    continue;
                }
                if best.is_none_or(|(b, _)| bend < b) {
                    best = Some((bend, s));
                }
            }
            if let Some((_, s)) = best {
                let spot = track.spot_at(s);
                let lateral = if kind == PadKind::Jump {
                    0.0
                } else {
                    rng.range(-0.55, 0.55)
                };
                course.pads.push(PadSpot {
                    kind,
                    at: on_the_road(spot, lateral),
                    tangent: spot.tangent,
                    right: spot.right,
                    s,
                });
                taken.push(s);
            }
        }
    }

    // Mystery boxes: three across, on gentle ground.
    let boxes = ((lap / 320.0).round() as usize).clamp(2, 6);
    for slot in 0..boxes {
        let ideal = lap * (slot as f32 + 0.15 + rng.range(0.0, 0.5)) / boxes as f32;
        for step in 0..40 {
            let s = (ideal + step as f32 * 7.0).rem_euclid(lap);
            if open(track, s - 8.0, s + 8.0)
                && free(&taken, s, 36.0)
                && worst_bend(track, s - 12.0, s + 12.0) < 0.09
            {
                let spot = track.spot_at(s);
                for lateral in [-0.95, 0.0, 0.95] {
                    course.boxes.push(BoxSpot {
                        at: on_the_road(spot, lateral),
                        s,
                    });
                }
                taken.push(s);
                break;
            }
        }
    }

    // Set pieces to plough through, every so often round the lap. A slot that
    // will not take one is tried a little further along, and a little back,
    // before it is given up.
    let mut s = AFTER_LINE + rng.range(8.0, 30.0);
    let mut group = 0u16;
    while s < lap - BEFORE_LINE {
        let piece = rng.below(100);
        let reach = 30.0;
        for shift in [0.0, 8.0, 16.0, 24.0, 32.0, -8.0, -16.0] {
            let at = s + shift;
            if open(track, at - 4.0, at + reach)
                && free(&taken, at, 26.0)
                && worst_bend(track, at - 4.0, at + reach) < 0.15
            {
                group += 1;
                place_piece(track, &mut course, &mut rng, at, piece, group);
                taken.push(at);
                s = at;
                break;
            }
        }
        s += rng.range(42.0, 70.0);
    }

    // A circuit too twisty for set pieces still gets its share: single things,
    // wherever there is room for one, until it has enough to be going on with.
    if group < 4 {
        let mut s = AFTER_LINE + rng.range(4.0, 14.0);
        while s < lap - BEFORE_LINE {
            if open(track, s - 2.0, s + 2.0)
                && free(&taken, s, 15.0)
                && worst_bend(track, s - 3.0, s + 3.0) < 0.35
            {
                group += 1;
                let kind = *rng.pick(&[
                    PropKind::Cone,
                    PropKind::Duck,
                    PropKind::Melon,
                    PropKind::Crate,
                    PropKind::Cow,
                ]);
                let spot = track.spot_at(s);
                course.props.push(PropSpot {
                    kind,
                    at: on_the_road(spot, rng.range(-0.9, 0.9)),
                    s,
                    yaw: rng.range(0.0, std::f32::consts::TAU),
                    group,
                });
                taken.push(s);
            }
            s += rng.range(16.0, 26.0);
        }
    }

    // Tube-men, on the outside of corners, wherever there is a corner.
    let mut s = AFTER_LINE;
    while s < lap - BEFORE_LINE {
        let bend = track.spot_at(s).curvature;
        if bend.abs() > 0.03
            && open(track, s - 6.0, s + 6.0)
            && course.men.iter().all(|m| (m.s - s).abs() > 90.0)
        {
            // The outside is the side the corner turns away from.
            let spot = track.spot_at(s);
            let outside = if bend > 0.0 { -1.0 } else { 1.0 };
            course.men.push(ManSpot {
                at: on_the_road(spot, outside * (ROAD_HALF + 1.5)),
                s,
                tint: rng.below(6) as u8,
            });
            s += 30.0;
        }
        s += 6.0;
    }
    course
}

/// One set piece, starting at `s`.
fn place_piece(track: &Track, course: &mut Course, rng: &mut Rng, s: f32, roll: usize, group: u16) {
    let mut put = |kind: PropKind, at: f32, lateral: f32, yaw: f32| {
        let spot = track.spot_at(at);
        course.props.push(PropSpot {
            kind,
            at: on_the_road(spot, lateral),
            s: at,
            yaw,
            group,
        });
    };
    match roll {
        // A slalom: cones, alternately left and right.
        0..=27 => {
            for i in 0..6 {
                put(
                    PropKind::Cone,
                    s + i as f32 * 5.5,
                    if i % 2 == 0 { -0.40 } else { 0.40 },
                    0.0,
                );
            }
        }
        // A bowling alley: ten pins in a triangle.
        28..=39 => {
            for row in 0..4usize {
                for i in 0..=row {
                    let lateral = (i as f32 - row as f32 * 0.5) * 0.42;
                    put(PropKind::Pin, s + row as f32 * 0.5, lateral, 0.0);
                }
            }
        }
        // Cows, crossing.
        40..=55 => {
            let n = 1 + rng.below(3);
            for i in 0..n {
                put(
                    PropKind::Cow,
                    s + i as f32 * 3.6,
                    rng.range(-0.8, 0.8),
                    std::f32::consts::FRAC_PI_2 * if rng.one_in(2) { 1.0 } else { -1.0 },
                );
            }
        }
        // A parade of ducks across the road.
        56..=66 => {
            for i in 0..5 {
                put(
                    PropKind::Duck,
                    s + i as f32 * 0.3,
                    -1.1 + i as f32 * 0.55,
                    0.0,
                );
            }
        }
        // A pile of watermelons.
        67..=76 => {
            for (lateral, back) in [
                (-0.3, 0.0),
                (0.25, 0.0),
                (0.0, 0.4),
                (-0.6, 0.5),
                (0.55, 0.5),
            ] {
                put(PropKind::Melon, s + back, lateral, 0.0);
            }
        }
        // Boxes, stacked in the road as if somebody had meant to.
        77..=86 => {
            for (lateral, back) in [(-0.6, 0.0), (0.0, 0.0), (0.6, 0.0), (-0.3, 0.7), (0.3, 0.7)] {
                put(PropKind::Crate, s + back, lateral, rng.range(-0.4, 0.4));
            }
        }
        // A balloon arch, over the whole road.
        _ => {
            for i in 0..14 {
                let across = -1.5 + 3.0 * i as f32 / 13.0;
                put(PropKind::Balloon, s + rng.range(-0.6, 0.6), across, 0.0);
            }
        }
    }
}

/// The course being driven, worked out again whenever the circuit or how wild
/// it is changes.
#[derive(Resource, Default)]
pub(crate) struct Layout {
    pub course: Course,
    /// What it was planned for.
    made_for: Option<(String, u8, u32)>,
    /// Counts up each time it is replanned, so what is built from it knows.
    pub edition: u32,
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Layout>().add_systems(PreUpdate, replan);
}

/// Plan the course for the circuit in hand, once per circuit and wildness.
fn replan(fun: Res<super::Fun>, track: Res<Track>, mut layout: ResMut<Layout>) {
    let want = fun
        .silly()
        .then(|| (track.circuit().id.to_string(), track.wild_used()));
    let have = layout
        .made_for
        .as_ref()
        .map(|(id, wild, _)| (id.clone(), *wild));
    if want == have {
        return;
    }
    layout.edition += 1;
    layout.course = if want.is_some() {
        plan(&track)
    } else {
        Course::default()
    };
    layout.made_for = want.map(|(id, wild)| (id, wild, layout.edition));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::all_circuits;

    /// Every circuit and its course, built once for all the tests here.
    fn every_course() -> impl Iterator<Item = &'static (Track, Course)> {
        static ALL: std::sync::OnceLock<Vec<(Track, Course)>> = std::sync::OnceLock::new();
        ALL.get_or_init(|| {
            // Built a few to a thread: a circuit takes a couple of seconds to
            // build in a test profile, and there are fifty-one of them.
            let circuits = all_circuits();
            let mut built: Vec<Option<(Track, Course)>> =
                (0..circuits.len()).map(|_| None).collect();
            std::thread::scope(|scope| {
                for (some, slots) in circuits.chunks(5).zip(built.chunks_mut(5)) {
                    scope.spawn(move || {
                        for (circuit, slot) in some.iter().zip(slots) {
                            let track = Track::new(circuit);
                            let course = plan(&track);
                            *slot = Some((track, course));
                        }
                    });
                }
            });
            built.into_iter().map(|b| b.expect("built")).collect()
        })
        .iter()
    }

    /// What is where on a circuit, for finding something to look at:
    /// `TODORA_SHOW=monza TODORA_WILD=2 cargo test --lib show_the_course -- --ignored --nocapture`.
    #[test]
    #[ignore = "prints a plan"]
    fn show_the_course() {
        let id = std::env::var("TODORA_SHOW").unwrap_or_else(|_| "monza".into());
        let circuit = all_circuits()
            .iter()
            .find(|c| c.id == id)
            .expect("a circuit");
        let wild = std::env::var("TODORA_WILD")
            .ok()
            .and_then(|w| w.parse().ok())
            .unwrap_or(2);
        let track = Track::with_wild(circuit, wild);
        let course = plan(&track);
        println!(
            "{} is {:.0} m round, wildness {wild}",
            circuit.name,
            track.length()
        );
        for pad in &course.pads {
            println!("  pad   {:?} at {:.0} m", pad.kind, pad.s);
        }
        let mut last = (u16::MAX, PropKind::Cone);
        for prop in &course.props {
            if (prop.group, prop.kind) != last {
                println!(
                    "  props {:?} from {:.0} m (group {})",
                    prop.kind, prop.s, prop.group
                );
                last = (prop.group, prop.kind);
            }
        }
        for spot in course.boxes.iter().step_by(3) {
            println!("  boxes at {:.0} m", spot.s);
        }
        for man in &course.men {
            println!("  tube-man at {:.0} m", man.s);
        }
    }

    #[test]
    fn a_circuit_gets_the_same_road_every_time() {
        let circuit = all_circuits().iter().find(|c| c.id == "monza").unwrap();
        let track = Track::new(circuit);
        let (a, b) = (plan(&track), plan(&track));
        assert_eq!(a.pads.len(), b.pads.len());
        assert_eq!(a.props.len(), b.props.len());
        for (x, y) in a.props.iter().zip(&b.props) {
            assert_eq!((x.kind, x.at, x.group), (y.kind, y.at, y.group));
        }
        assert!(!a.pads.is_empty() && !a.props.is_empty() && !a.boxes.is_empty());
    }

    #[test]
    fn nothing_is_on_the_grid_the_line_or_a_bridge() {
        for (track, course) in every_course().map(|(t, c)| (t, c)) {
            let name = track.circuit().name;
            let lap = track.length();
            let bad =
                |s: f32| s < AFTER_LINE - 1.0 || s > lap - BEFORE_LINE + 1.0 || track.bridge_at(s);
            for pad in &course.pads {
                assert!(!bad(pad.s), "{name}: a pad at {:.0} m", pad.s);
            }
            for prop in &course.props {
                assert!(!bad(prop.s), "{name}: a {:?} at {:.0} m", prop.kind, prop.s);
            }
            for spot in &course.boxes {
                assert!(!bad(spot.s), "{name}: a box at {:.0} m", spot.s);
            }
            for man in &course.men {
                assert!(!bad(man.s), "{name}: a tube-man at {:.0} m", man.s);
            }
        }
    }

    #[test]
    fn everything_stands_on_the_road_or_beside_it() {
        for (track, course) in every_course().map(|(t, c)| (t, c)) {
            let name = track.circuit().name;
            let check = |at: Vec3, s: f32, reach: f32, what: &str| {
                let lateral = track.lateral_of(at, Some(s));
                assert!(
                    lateral.abs() <= reach,
                    "{name}: {what} {lateral:.2} m from the centreline at {s:.0} m"
                );
            };
            for pad in &course.pads {
                check(pad.at, pad.s, ROAD_HALF, "a pad");
            }
            for prop in &course.props {
                check(prop.at, prop.s, ROAD_HALF, "a prop");
            }
            for spot in &course.boxes {
                check(spot.at, spot.s, ROAD_HALF, "a box");
            }
            for man in &course.men {
                check(man.at, man.s, ROAD_HALF + 2.5, "a tube-man");
            }
        }
    }

    #[test]
    fn pads_are_on_straights_and_spread_out() {
        for (track, course) in every_course().map(|(t, c)| (t, c)) {
            let name = track.circuit().name;
            for pad in &course.pads {
                let bend = worst_bend(track, pad.s - 18.0, pad.s + 42.0);
                assert!(bend <= STRAIGHT + 1e-4, "{name}: a pad on a bend of {bend}");
            }
            let mut spots: Vec<f32> = course.pads.iter().map(|p| p.s).collect();
            spots.sort_by(f32::total_cmp);
            for pair in spots.windows(2) {
                assert!(
                    pair[1] - pair[0] >= 48.0,
                    "{name}: two pads {:.0} m apart",
                    pair[1] - pair[0]
                );
            }
        }
    }

    #[test]
    fn the_bigger_the_lap_the_more_there_is_on_it() {
        let mut laps: Vec<(f32, usize)> = every_course()
            .map(|(track, course)| (track.length(), course.props.len() + course.pads.len()))
            .collect();
        laps.sort_by(|a, b| a.0.total_cmp(&b.0));
        let half = laps.len() / 2;
        let small: usize = laps[..half].iter().map(|l| l.1).sum();
        let large: usize = laps[half..].iter().map(|l| l.1).sum();
        assert!(
            large > small,
            "long laps have {large} things to short laps' {small}"
        );
        // No circuit is left bare.
        assert!(
            laps.iter().all(|l| l.1 >= 6),
            "some circuit has almost nothing: {laps:?}"
        );
    }

    #[test]
    fn a_bowling_alley_is_ten_pins() {
        for (_, course) in every_course() {
            let mut groups = std::collections::BTreeMap::<u16, usize>::new();
            for prop in course.props.iter().filter(|p| p.kind == PropKind::Pin) {
                *groups.entry(prop.group).or_default() += 1;
            }
            assert!(groups.values().all(|n| *n == 10), "{groups:?}");
        }
    }
}
