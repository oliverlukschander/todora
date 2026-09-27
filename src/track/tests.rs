use super::profile::{EDGE, GRASS_GRIP, KERB_GRIP, KERB_TOP, TARMAC_HALF};
use super::*;

/// How much of a lap may sit at [`ribbon::MAX_GRADE`] before the cap has
/// stopped backing the hills up and started being their shape. At the cap the
/// game ships with, the worst circuit is Spa at 16%, then Imola at 9% and
/// Spielberg at 7%; halve the cap and Spa is at 67% and Spielberg at 60%, which
/// is every hill on both of them coming out as the same ramp.
const PINNED_TO_THE_CAP: f32 = 0.25;

/// How much of its relief a circuit has to bring through the smoothing.
///
/// Lower than it looks, because the smoothing span is fixed in metres while
/// relief is not: a circuit whose height is in short features loses more of it
/// than one whose height is in long climbs. The worst are the Nürburgring and
/// Zandvoort at 65% and Hockenheim at 66%, and what the smoothing is refusing
/// on all three is an elevation model with steps in it that no road has —
/// 16 m between two fixes forty metres apart, which is a 40% gradient and is an
/// embankment the model has mistaken for the road.
///
/// The bar has to be low enough to let that through and high enough to still
/// catch a span long enough to iron the circuits flat, and it is: double the
/// span and the worst goes to 45%, with five circuits under this figure.
const KEPT_RELIEF: f32 = 0.6;

/// How far behind the start plane the grid has to read, at the very least. Two
/// car lengths: nearer than that and the lap would begin before the driver had
/// touched anything.
const CLEAR_OF_THE_LINE: f32 = 5.0;

/// Relief a circuit may lose to smoothing however flat it is, in metres of
/// Todora height. Three and a half steps of a DEM quantised to the metre, at
/// [`HEIGHT_SCALE`]: below this, what was lost is under the resolution of what
/// the elevation model was able to say in the first place.
const FLATTENED: f32 = 1.0;

/// How far the trace may end up from the circuit built out of it, on average.
///
/// On average, and not at worst, because at worst is a question the retention
/// guard already answers. Opening a corner to the target radius moves its apex
/// by something like that radius, and the tighter the corner was the further it
/// moves: the Nürburgring's worst fix ends up 45 m from the circuit, and that
/// one fix is a hairpin that was far tighter than the loft can carry. Averaged
/// over the whole trace it is 5.4 m, and that is the number that says whether
/// the circuit as a whole is still where it was surveyed or has drifted off
/// somewhere else.
///
/// The bar is the diameter of the tightest corner the game allows, which is the
/// largest single change the pipeline is entitled to make to a corner. Nothing
/// is near it: the Nürburgring is worst at 5.4 m and Spa is at 0.7.
const MOVED: f32 = 2.0 * ribbon::MIN_RADIUS;

/// The least turning that ends one turn and starts another, rather than being
/// the road not going perfectly straight in the middle of a corner. A fifth of
/// a right angle.
const A_TURN: f32 = 0.35;

/// Turning a circuit has to do in a lap, in whole turns, before it is a lap of
/// a circuit rather than a lap of a ring.
///
/// A ring turns once. The circuits turn between two and five times — Monza is
/// lowest at 2.0 and Monaco highest — so this has room under all of them and a
/// long way to fall before it reaches a ring. It is the thing retention was
/// invented to catch and cannot: a circuit can keep nine tenths of its length
/// while the nine tenths it kept is a loop with the corners taken out of it.
const A_CIRCUIT_TURNS: f32 = 1.5;

/// How much of its trace's turning a circuit has to come out with.
///
/// The other half of the same question, and the half that notices a circuit
/// that was always going to be round. Gilles-Villeneuve is worst at 68% and
/// Monza next at 77%; what they lose is the corner-opening pass cutting the
/// corners, which is the pass working. Erase a chicane and this is what moves,
/// because the turning that was in it is simply gone.
const TURNING_KEPT: f32 = 0.6;

/// How many of its trace's changes of direction a circuit has to come out with.
///
/// Loose, because a change of direction is a threshold question and the two
/// sides of it are sampled forty metres apart and forty centimetres apart.
/// Buenos Aires is worst at four of eight, its other four being kinks of little
/// more than [`A_TURN`] that the spline rounds into the corners either side.
/// What it is here for is the wholesale case — a circuit that came out with
/// two changes of direction where its trace had fourteen has not been shrunk,
/// it has been replaced.
const CHANGES_KEPT: f32 = 0.5;

fn track() -> Track {
    Track::any()
}

/// Every circuit, so a new one has to clear the same bar as the old ones.
fn every_track() -> impl Iterator<Item = (&'static str, Track)> {
    circuits::all()
        .iter()
        .map(|circuit| (circuit.name, Track::new(circuit)))
}

/// Three things at once, on every circuit.
///
/// No step is steeper than the cap — the DEM is quantised to whole metres
/// and the plan is shrunk five times harder than the height, so an
/// unsmoothed step would read as a wall.
///
/// Little of the lap is *pinned* to that cap, which is what says the cap is
/// backing the hills up rather than shaping them. This is the one that
/// catches a cap set too low, and it catches it directly: halving the cap
/// takes Spa from a sixth of its lap pinned to two thirds of it.
///
/// And most of the relief the trace carried survives the smoothing, which
/// catches the other end — a span long enough to iron the circuits flat.
/// This one has to be read carefully, because the span is in metres and the
/// relief is not: see [`KEPT_RELIEF`] and [`FLATTENED`].
///
/// Stated as a fraction rather than as metres, because the circuits are not
/// equally hilly. Spa rises 27 m and Monza 6, and Monza is not broken — it
/// is Monza.
///
/// A fraction on its own is not enough at the flat end, though, because the
/// smoothing span is fixed in metres while the relief is not: the flatter a
/// circuit is, the larger a share of it 24 m of averaging takes. Mexico City
/// climbs 5 m in life, which is 1.4 m here, and comes out with 0.9 of them.
/// So a circuit passes on either count — it kept most of its relief, or what
/// it lost is under [`FLATTENED`], which is below the resolution of the
/// question being asked.
#[test]
fn a_car_stuck_with_two_wheels_on_kerbs_still_gets_recovered() {
    let track = track();
    let mut at = track.start_transform();
    at.translation += *at.right() * (HALF_WIDTH + 0.05);
    let mut car = Car::default();
    assert!(track.legal_contact(&at, car.along));
    for _ in 0..400 {
        track.hold(&mut at, &mut car, 1.0 / 240.0);
    }
    assert!(car.recovered);
    assert!(track.ground(at.translation).lateral.abs() < 0.01);
}

#[test]
fn grass_under_the_bridge_does_not_lift_the_car_onto_the_deck() {
    let track = Track::new(all_circuits().iter().find(|c| c.id == "suzuka").unwrap());
    let (station, y) = track
        .ribbon
        .stations()
        .iter()
        .find_map(|station| {
            if profile::deep(station) == 0.0 {
                return None;
            }
            let (y, _) = track
                .terrain()
                .sample(Vec2::new(station.pos.x, station.pos.z))?;
            (station.pos.y - y > 1.0).then_some((station, y))
        })
        .expect("grass beneath the span");
    let mut at = Transform::from_translation(Vec3::new(station.pos.x, y, station.pos.z))
        .looking_to(station.tangent, Vec3::Y);
    let mut car = Car {
        along: Some(station.s),
        velocity: station.tangent * 3.0,
        ..default()
    };
    track.hold(&mut at, &mut car, 1.0 / 240.0);
    assert!((at.translation.y - y).abs() < 0.001);
    assert_eq!(
        track.ground_from(at.translation, car.along).grip,
        profile::GRASS_GRIP
    );
    assert!(!track.legal_contact(&at, car.along));
}

#[test]
fn wheel_contacts_allow_kerbs_and_two_wheels_out_but_not_four() {
    let track = track();
    let start = track.start_transform();
    let mut at = start;
    at.translation += *start.right() * HALF_WIDTH;
    assert!(track.legal_contact(&at, Some(track.start_along_lap())));
    at.translation += *start.right() * (crate::car::HALF_TRACK + crate::car::WHEEL_WIDTH);
    assert!(!track.legal_contact(&at, Some(track.start_along_lap())));
    at = start;
    at.rotate_y(std::f32::consts::FRAC_PI_2);
    assert!(track.legal_contact(&at, Some(track.start_along_lap())));
}

#[test]
fn hills_roll_instead_of_stepping() {
    for (name, track) in every_track() {
        let stations = track.ribbon.stations();
        let n = stations.len();
        let mut steepest = 0.0f32;
        for i in 0..n {
            let a = stations[i].pos;
            let b = stations[(i + 1) % n].pos;
            let run = Vec3::new(b.x - a.x, 0.0, b.z - a.z).length().max(1e-4);
            steepest = steepest.max((b.y - a.y).abs() / run);
        }
        assert!(
            steepest <= ribbon::MAX_GRADE + 1e-3,
            "{name}: max grade {steepest} is past the cap of {}",
            ribbon::MAX_GRADE
        );
        // And what the cap does to get there is shave the crests, not press
        // the circuit flat: within a per cent, the relief it leaves is the
        // relief the smoothing handed it, the worst being Kyalami at 0.8%.
        // Worth stating, because the two are easy to blame for each other's
        // work — the relief a circuit loses, it loses entirely to the
        // smoothing — and only one of them is guarded by the fraction
        // below.
        assert!(
            track.ribbon.relief() > 0.98 * track.ribbon.smoothed_relief(),
            "{name}: the grade cap took {:.2} m of the {:.2} m of relief the \
             smoothing left",
            track.ribbon.smoothed_relief() - track.ribbon.relief(),
            track.ribbon.smoothed_relief()
        );
        let pinned = (0..n)
            .filter(|&i| {
                let a = stations[i].pos;
                let b = stations[(i + 1) % n].pos;
                let run = Vec3::new(b.x - a.x, 0.0, b.z - a.z).length().max(1e-4);
                (b.y - a.y).abs() / run > ribbon::MAX_GRADE * 0.98
            })
            .count() as f32
            / n as f32;
        assert!(
            pinned < PINNED_TO_THE_CAP,
            "{name}: {:.0}% of the lap is pinned to the grade cap, which is \
             the cap doing the shaping rather than backing it up",
            pinned * 100.0
        );

        let (low, high) = stations.iter().fold((f32::MAX, f32::MIN), |(l, h), s| {
            (l.min(s.pos.y), h.max(s.pos.y))
        });
        let wanted = relief_of(track.circuit()) * HEIGHT_SCALE;
        let lost = wanted - (high - low);
        assert!(
            high - low > KEPT_RELIEF * wanted || lost < FLATTENED,
            "{name}: smoothing left {:.1} m of the {wanted:.1} m the circuit climbs",
            high - low
        );
    }
}

/// Walk the whole lap the way the timer sees it: progress climbs all the way
/// round and comes back to where it started exactly once, at the line, which
/// is also the one place the gate opens.
///
/// The walk is driven a little off the racing line, as a car is. That is why
/// the wrap is counted rather than assumed to fall at station zero: a car
/// sitting on the line but a metre and a half to one side is nearest a
/// centreline point that may be a hair either side of it, so which station
/// the lap turns over on is the circuit's business, not the timer's.
#[test]
fn a_lap_reads_as_one_lap() {
    for (name, track) in every_track() {
        let stations = track.ribbon.stations();
        let off_line =
            |station: &ribbon::Station| station.pos + station.right * (TARMAC_HALF * 0.5);
        let last = &stations[stations.len() - 1];
        let mut crossings = 0;
        let mut wraps = 0;
        let mut climbed = 0.0f32;
        let mut was_progress = track.progress(off_line(last), None);
        let mut was_along = track.start_along(off_line(last));
        for station in stations {
            let pos = off_line(station);
            let progress = track.progress(pos, None);
            let step = progress - was_progress;
            if step < -0.5 {
                wraps += 1;
            } else {
                assert!(
                    step >= -0.02,
                    "{name}: progress went backwards at s={}: {progress} after {was_progress}",
                    station.s
                );
                climbed += step;
            }
            was_progress = progress;

            let along = track.start_along(pos);
            if was_along <= 0.0 && along > 0.0 && track.on_start_gate(pos, None) {
                crossings += 1;
            }
            was_along = along;
        }
        assert_eq!(wraps, 1, "{name}: the lap turned over {wraps} times");
        assert!(climbed > 0.97, "{name}: the lap only covered {climbed}");
        assert_eq!(
            crossings, 1,
            "{name}: the start gate opened {crossings} times"
        );
    }
}

/// Hold finished traversal and the starting car to the researched directions,
/// independently of the GeoJSON order. Suzuka's signed area is only a
/// regression signal for its pinned figure-eight layout; see the audit.
#[test]
fn every_circuit_runs_in_its_verified_racing_direction() {
    let audit = include_str!("../../docs/track-screening/directions.md");
    for circuit in all_circuits() {
        let row = audit
            .lines()
            .find(|row| row.starts_with(&format!("| {} |", circuit.id)))
            .expect("every circuit has a researched direction");
        let clockwise = row
            .split('|')
            .nth(3)
            .unwrap()
            .trim()
            .starts_with("Clockwise");
        let track = Track::new(circuit);
        let stations = track.ribbon.stations();
        let area: f32 = stations
            .iter()
            .zip(stations.iter().cycle().skip(1))
            .map(|(a, b)| a.pos.x * b.pos.z - b.pos.x * a.pos.z)
            .sum();
        assert_eq!(area > 0.0, clockwise, "{}: reversed ribbon", circuit.name);
        let grid = track.ribbon.before_start(RUN_UP);
        assert!(
            track.start_transform().forward().dot(grid.tangent) > 0.99,
            "{}: car faces away from racing direction",
            circuit.name
        );
    }
}

/// What the car stands on has to be the same cross-section the mesh was
/// swept from, or the car rides at a height the road is not at.
#[test]
fn the_ground_reads_the_profile_it_was_lofted_from() {
    for (name, track) in every_track() {
        let start = track.start_transform();
        let right = *start.right();
        for side in [-1.0f32, 1.0] {
            // How far the grass reaches on this side *here*, which is what
            // the reading out on it has to be taken against: the verge is
            // fitted station by station, with grass at road height.
            let edge = track.ground(start.translation + right * side).edge;
            let lip = HALF_WIDTH + (2.0 / 3.0) * (edge - HALF_WIDTH);
            for (across, grip, height) in [
                (0.0, 1.0, 0.0),
                (TARMAC_HALF - 0.01, 1.0, 0.0),
                (HALF_WIDTH - 0.01, KERB_GRIP, KERB_TOP * (0.69 / 0.70)),
                (lip, GRASS_GRIP, 0.0),
            ] {
                let ground = track.ground(start.translation + right * (side * across));
                assert_eq!(ground.grip, grip, "{name}: grip {across} m off the line");
                assert!(
                    (ground.height - ground.centre.y - height).abs() < 0.02,
                    "{name}: height {across} m off the line: {} against {height}",
                    ground.height - ground.centre.y
                );
                assert!((ground.lateral - side * across).abs() < 0.02);
                // The centreline point is on the centreline, whatever we asked.
                assert!(track.ground(ground.centre).lateral.abs() < 0.02);
            }
        }
    }
}

/// A car that has stranded itself has to be left somewhere it can drive away
/// from, whatever mess it was in.
#[test]
fn a_rescue_puts_the_car_back_on_the_line() {
    let track = track();
    let start = track.start_transform();
    let right = *start.right();
    let wall = 12.0;
    for stuck in [wall, -wall, HALF_WIDTH + 1.0, 0.0] {
        let mut transform = Transform::from_translation(start.translation + right * stuck)
            // Facing backwards, sideways, and scaled like the real car.
            .looking_to(-*start.forward(), Vec3::Y)
            .with_scale(Vec3::splat(0.8));
        let mut car = Car {
            velocity: right * 9.0,
            yaw_rate: 3.0,
            stranded: 4.0,
            ..default()
        };
        track.rescue(&mut transform, &mut car);

        let ground = track.ground(transform.translation);
        assert!(
            ground.lateral.abs() < 0.01,
            "rescue {stuck} m out landed off-line"
        );
        assert!(car.velocity.length() < 1e-4, "rescue left the car moving");
        assert_eq!(car.yaw_rate, 0.0);
        assert_eq!(car.stranded, 0.0);
        assert_eq!(transform.scale, Vec3::splat(0.8), "rescue resized the car");
        assert!(
            transform.forward().dot(ground.tangent) > 0.99,
            "rescue {stuck} m out left the car facing the wrong way"
        );
    }
}

#[test]
fn grass_is_driveable_beyond_the_old_verge_and_boundary_recovers() {
    let track = track();
    let mut at = track.start_transform();
    at.translation += *at.right() * 12.0;
    let before = at.translation;
    let mut car = Car {
        velocity: *at.right() * 5.0,
        ..default()
    };
    track.hold(&mut at, &mut car, 1.0 / 240.0);
    assert!(Vec2::new(at.translation.x - before.x, at.translation.z - before.z).length() < 0.001);
    assert!(!car.recovered);
    assert_eq!(at.translation.y, track.ground(at.translation).height);
    at.translation.x += 1000.0;
    track.hold(&mut at, &mut car, 1.0 / 240.0);
    assert!(car.recovered);
    assert!(track.ground(at.translation).lateral.abs() < 0.01);
}

#[test]
fn a_stranded_car_is_rescued_after_simulated_time() {
    let track = track();
    let start = track.start_transform();
    let mut transform = start;
    transform.translation += *start.right() * (HALF_WIDTH + 1.0);
    let mut car = Car::default();
    for _ in 0..380 {
        track.hold(&mut transform, &mut car, 1.0 / 240.0);
    }
    assert!(track.ground(transform.translation).lateral.abs() > HALF_WIDTH);
    for _ in 0..10 {
        track.hold(&mut transform, &mut car, 1.0 / 240.0);
    }
    assert!(track.ground(transform.translation).lateral.abs() < 0.01);
    assert_eq!(car.stranded, 0.0);
    assert_eq!(car.velocity, Vec3::ZERO);
}

/// On the steep parts the car has to follow the road, not stay level on top
/// of it with its nose in the tarmac.
#[test]
fn the_car_lies_along_the_slope() {
    let track = track();
    let steepest = track
        .ribbon
        .stations()
        .iter()
        .max_by(|a, b| a.slope.abs().total_cmp(&b.slope.abs()))
        .expect("the circuit has stations");
    assert!(steepest.slope.abs() > 0.1, "nowhere steep enough to test");

    for facing in [1.0f32, -1.0] {
        let mut transform = Transform::from_translation(steepest.pos)
            .looking_to(steepest.tangent * facing, Vec3::Y);
        let mut car = Car::default();
        track.hold(&mut transform, &mut car, 1.0 / 60.0);

        // Running down the hill the nose points down, and up it points up.
        let grade = steepest.slope * facing;
        let want = grade / (1.0 + grade * grade).sqrt();
        assert!(
            (transform.forward().y - want).abs() < 0.01,
            "facing {facing}, nose at {} against a grade of {want}",
            transform.forward().y
        );
        // And it is still pointing the way it was, in plan.
        let heading = crate::car::level(*transform.forward());
        assert!(
            heading.dot(steepest.tangent * facing) > 0.999,
            "the pitch turned it"
        );
    }
}

/// The grid slot is on the tarmac, pointing down the circuit.
#[test]
fn start_is_on_the_road() {
    for (name, track) in every_track() {
        let pose = track.start_transform();
        let fix = track.ribbon.locate(pose.translation);
        assert!(fix.lateral.abs() < 0.01, "{name}");
        let lap = track.ribbon.length();
        assert!(
            track.progress(pose.translation, None) > 1.0 - (RUN_UP + 1.0) / lap,
            "{name} sets the car down further back than the run-up"
        );
    }
}

/// The grid is a run-up, and a run-up has to be usable.
///
/// Three things, and the first is what the grid *means*: it is [`RUN_UP`]
/// metres back along the road, measured round the circuit the way a lap is.
///
/// The second is that the start plane agrees it is behind there, with room
/// to spare — that plane is what the clock reads to know the car has
/// arrived, and a grid sitting on it would start the lap before the driver
/// touched anything. How far behind it reads is *not* the run-up and cannot
/// be asked to be: the plane measures a straight line while the road bends,
/// so the two only agree where the run-up is straight. Mexico City's last
/// 45 m are the stadium section, which turns the car through most of a half
/// circle, so its grid reads 19 m behind the plane while being 45 m behind
/// the line.
///
/// The third is that the car faces the way the lap runs *where it is put
/// down*, which is not the way the line faces and must not be confused with
/// it. This used to ask for the two to agree within 20 degrees, which was
/// only ever true of circuits whose run-up happened to be straight: Mexico
/// City's grid faces 155 degrees away from its line and is perfectly correct
/// — it is pointing down the stadium section, which is where the road goes.
/// What a bent run-up costs is speed at the line, and that is asked where
/// the car is, in `the_run_up_reaches_the_line_at_speed`.
#[test]
fn the_grid_is_a_run_up_to_the_line() {
    for (name, track) in every_track() {
        let pose = track.start_transform();
        let back = (1.0 - track.progress(pose.translation, None)) * track.ribbon.length();
        assert!(
            (RUN_UP - 1.0..RUN_UP + 1.0).contains(&back),
            "{name} sets the car down {back:.1} m back along the road, not {RUN_UP}"
        );

        let along = track.start_along(pose.translation);
        assert!(
            (-RUN_UP - 1.0..=-CLEAR_OF_THE_LINE).contains(&along),
            "{name} reads {along:.1} m behind its own start plane"
        );

        let here = track.ribbon.locate(pose.translation);
        assert!(
            level(*pose.forward()).dot(here.tangent) > 0.99,
            "{name} sets the car down across its own road rather than down it"
        );
    }
}

/// The plan of a circuit, as the trace drew it and at the scale it is built
/// at. Heights left out: this is about where the circuit goes.
fn traced(circuit: &Circuit) -> Vec<Vec3> {
    let plan = PLAN_SCALE * circuit.plan_scale;
    circuit
        .centreline
        .iter()
        .map(|p| Vec3::new(p[0] * plan, 0.0, p[2] * plan))
        .collect()
}

/// The order a closed plan changes direction in: `1` for right and `-1` for
/// left, one entry each time the circuit stops turning one way and starts
/// turning the other by at least [`A_TURN`].
///
/// Turning is accumulated rather than read off single steps, and a change
/// of direction only ends a turn once the turn is worth having ended. That
/// is what lets the same question be asked of a trace whose fixes are forty
/// metres apart and of a centreline whose stations are forty centimetres
/// apart: a wobble on either is absorbed into the turn it is inside, and
/// what comes out is the shape rather than the sampling.
///
/// Two turns the same way running are then one entry, not two. Where one
/// long right ends and the next begins is a matter of how straight the road
/// got in between, and the answer moves by a turn or two between a trace
/// and the circuit splined from it — Albert Park's back section reads as
/// four rights on the trace and three on the circuit, and is the same four
/// corners either way. What does not move is the order of the *changes*: a
/// chicane is a left and then a right, and losing one is losing an entry.
fn turns(line: &[Vec3]) -> Vec<i8> {
    let mut out: Vec<i8> = swings(line).into_iter().map(|(way, _)| way).collect();
    out.dedup();
    // The walk starts in the middle of whatever turn station zero is in, so
    // one turn can come out as two, one at each end. They are the same turn.
    if out.len() > 1 && out[0] == out[out.len() - 1] {
        out.pop();
    }
    out
}

/// Every turn of at least [`A_TURN`] in the closed plan, as a direction and
/// how far it went through, in the order they come.
fn swings(line: &[Vec3]) -> Vec<(i8, f32)> {
    let n = line.len();
    let mut out = Vec::new();
    let mut running = 0.0f32;
    for i in 0..n {
        let from = line[(i + 1) % n] - line[i];
        let to = line[(i + 2) % n] - line[(i + 1) % n];
        if from.length() < 1e-6 || to.length() < 1e-6 {
            continue;
        }
        let turn = f32::atan2(to.cross(from).y, to.dot(from));
        if running != 0.0 && turn.signum() != running.signum() && running.abs() >= A_TURN {
            out.push((running.signum() as i8, running.abs()));
            running = 0.0;
        }
        running += turn;
    }
    if running.abs() >= A_TURN {
        out.push((running.signum() as i8, running.abs()));
    }
    out
}

/// A circuit still goes where it was traced.
///
/// Retention is a guard against a circuit being rounded off into a ring,
/// and it is a good one, but it measures a length rather than a shape and
/// it starts *after* the spline: a corner the splining removed was never in
/// the lap it compares against. These two ask the shape directly.
///
/// First, displacement. Every fix of the trace has to be near the finished
/// centreline — near being measured against the cross-section, because a
/// circuit displaced by less than its own road is a circuit the road still
/// covers, and one displaced by more has moved. The bar is generous on
/// purpose: splining a trace whose fixes are forty metres apart rounds its
/// corners by design, and opening a corner cuts it by design. What it
/// catches is a circuit that has gone somewhere else.
///
/// Second, turn order. A chicane is a left and then a right; a hairpin is
/// one long turn. Lose either and the sequence of turns changes, and the
/// sequence is what a driver remembers a circuit by. Compared against the
/// raw trace, so it sees what the spline and the corner-opening pass did as
/// well as what the rest of the pipeline did.
#[test]
fn every_circuit_is_still_the_circuit_it_was_traced_from() {
    for (name, track) in every_track() {
        let drawn = traced(track.circuit());
        let moved = drifted(&track, &drawn);
        assert!(
            moved < MOVED,
            "{name}: the trace ends up {moved:.1} m from the circuit built \
             out of it on average, against {MOVED} m"
        );

        let (was, now) = (wound(&drawn), wound(&plan_of(&track)));
        assert!(
            now > A_CIRCUIT_TURNS,
            "{name}: the circuit turns through {now:.1} laps' worth of \
             corners, which is not far off a ring's one"
        );
        assert!(
            now > TURNING_KEPT * was,
            "{name}: the trace turns through {was:.1} laps' worth of corners \
             and the circuit through {now:.1}"
        );

        let (was, now) = (turns(&drawn), turns(&plan_of(&track)));
        assert_eq!(
            now.first(),
            was.first(),
            "{name}: the trace and the circuit do not start turning the same way"
        );
        assert!(
            now.len() as f32 >= CHANGES_KEPT * was.len() as f32,
            "{name}: the trace changes direction {} times and the circuit {}",
            was.len(),
            now.len()
        );
    }
}

/// How far the trace ends up from the circuit built out of it, on average.
fn drifted(track: &Track, drawn: &[Vec3]) -> f32 {
    drawn
        .iter()
        .map(|&fix| flat(fix - track.ribbon.locate(fix).point).length())
        .sum::<f32>()
        / drawn.len() as f32
}

/// The two measures above, put to things that are wrong on purpose.
///
/// A property that has never been seen to fail is a property nobody knows
/// the shape of, and both of these are measures rather than assertions —
/// they have thresholds and detectors in them, and a detector that never
/// says no is a detector that says nothing.
///
/// The first is a circuit moved bodily off where it was traced. The second
/// is the one the plan is actually about: a chicane taken out. A left and a
/// right within a few metres of each other is two entries in the sequence;
/// straighten the road between the same two points and both entries go, and
/// nothing about the length of the lap has to change for that to happen —
/// which is exactly what retention cannot see.
#[test]
fn the_identity_checks_notice_a_circuit_that_has_been_changed() {
    let track = track();
    let drawn = traced(track.circuit());
    assert!(drifted(&track, &drawn) < MOVED);
    let shoved: Vec<Vec3> = drawn.iter().map(|&p| p + Vec3::X * 3.0 * MOVED).collect();
    assert!(
        drifted(&track, &shoved) > MOVED,
        "a circuit moved bodily off its own trace reads as being on it"
    );

    // A square lap with a chicane down each of its four sides. Squared off
    // rather than round so that the corners and the chicanes are separate
    // things: the four right-angles are one lap of turning between them, and
    // the four chicanes are most of another.
    let lay = |chicanes: bool| -> Vec<Vec3> {
        let mut out = Vec::new();
        let (mut at, mut heading) = (Vec3::ZERO, Vec3::Z);
        let walk = |at: &mut Vec3, heading: Vec3, run: usize, out: &mut Vec<Vec3>| {
            for _ in 0..run {
                *at += heading * 2.0;
                out.push(*at);
            }
        };
        for _ in 0..4 {
            walk(&mut at, heading, 20, &mut out);
            if chicanes {
                heading = Quat::from_rotation_y(0.7) * heading;
                walk(&mut at, heading, 6, &mut out);
                heading = Quat::from_rotation_y(-0.7) * heading;
            }
            walk(&mut at, heading, 20, &mut out);
            heading = Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2) * heading;
        }
        out
    };
    let (with, without) = (wound(&lay(true)), wound(&lay(false)));
    assert!(
        (without - 1.0).abs() < 0.05,
        "the square without its chicanes should turn through one lap, not {without:.2}"
    );
    assert!(
        without < TURNING_KEPT * with,
        "taking the chicanes out of a circuit left it turning through \
         {without:.2} laps against {with:.2}, which is not enough of a change \
         to notice"
    );
    assert!(
        (turns(&lay(false)).len() as f32) < CHANGES_KEPT * turns(&lay(true)).len() as f32,
        "taking the chicanes out left the changes of direction at {} of {}",
        turns(&lay(false)).len(),
        turns(&lay(true)).len()
    );
}

/// How far a closed plan turns through in a lap, in whole turns. A ring
/// comes out at one however big it is; a circuit comes out at as many
/// corners as it has.
fn wound(line: &[Vec3]) -> f32 {
    swings(line).iter().map(|(_, through)| through).sum::<f32>() / std::f32::consts::TAU
}

/// The finished centreline, in plan.
fn plan_of(track: &Track) -> Vec<Vec3> {
    track
        .ribbon
        .stations()
        .iter()
        .map(|s| flat(s.pos))
        .collect()
}

fn flat(v: Vec3) -> Vec3 {
    Vec3::new(v.x, 0.0, v.z)
}

/// Shrinking a circuit cuts its corners, and a circuit is not allowed to be
/// mostly corner-cutting. `Track::new` is what enforces that; this is the
/// margin the circuits in the game actually have.
#[test]
fn every_circuit_survives_the_shrink() {
    for (name, track) in every_track() {
        let kept = track.ribbon.kept();
        assert!(kept > LEAST_KEPT, "{name} kept only {kept} of its lap");
        assert!(kept <= 1.0, "{name} grew to {kept} of its lap");
    }
}

/// Being sent to a circuit builds it and tells everything else to put
/// itself back, in that order — the reset is only worth anything if
/// [`Track`] is already the circuit being reset onto. Being sent to the one
/// already being driven is not a switch and costs nothing, because the menu
/// opens with the cursor on it and `Enter` is the obvious thing to press.
#[test]
fn a_switch_builds_the_circuit_before_it_says_so() {
    #[derive(Resource, Default)]
    struct ResetsHeard(usize);
    fn count(mut resets: MessageReader<Reset>, mut heard: ResMut<ResetsHeard>) {
        heard.0 += resets.read().count();
    }

    let mut app = App::new();
    app.add_message::<Reset>()
        .add_message::<GoTo>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<ResetsHeard>()
        .insert_resource(Track::new(circuits::first()))
        .add_systems(Update, (switch, count).chain());

    let road = {
        let track = app.world().resource::<Track>();
        let mesh = track.profile.loft(&track.ribbon);
        app.world_mut().resource_mut::<Assets<Mesh>>().add(mesh)
    };
    let loft = app.world_mut().spawn((Loft, Mesh3d(road.clone()))).id();
    let asphalt = app.world_mut().spawn((Asphalt, Mesh3d(road.clone()))).id();
    let grass = app.world_mut().spawn((Terrain, Mesh3d(road.clone()))).id();

    app.update();
    assert_eq!(
        app.world().resource::<ResetsHeard>().0,
        0,
        "reset by itself"
    );
    assert_eq!(
        app.world().resource::<Track>().circuit().id,
        "red-bull-ring"
    );

    // Sent to where it already is: nothing is rebuilt and nothing is
    // thrown away.
    app.world_mut().write_message(GoTo(circuits::first()));
    app.update();
    assert_eq!(app.world().resource::<ResetsHeard>().0, 0, "reset in place");
    assert_eq!(
        app.world().get::<Mesh3d>(loft).expect("the loft").0,
        road,
        "the circuit being driven was rebuilt for nothing"
    );

    let next = &circuits::all()[1];
    app.world_mut().write_message(GoTo(next));
    app.update();
    assert_eq!(
        app.world().resource::<Track>().circuit().id,
        next.id,
        "the switch did not arrive"
    );
    assert_eq!(
        app.world().resource::<ResetsHeard>().0,
        1,
        "a switch has to reset what is left of the old lap"
    );

    // And the road being drawn is the new circuit's, not the old one's.
    let drawn = app
        .world()
        .get::<Mesh3d>(loft)
        .expect("the loft is still there")
        .0
        .clone();
    assert_ne!(drawn, road, "the old circuit is still being drawn");
    let world = app.world();
    let track = world.resource::<Track>();
    assert_eq!(
        world
            .resource::<Assets<Mesh>>()
            .get(&drawn)
            .expect("the new loft was added")
            .count_vertices(),
        track.profile.surfaces(&track.ribbon).0.count_vertices(),
        "the loft drawn is not the one this circuit sweeps"
    );

    // All three materials retain their mesh entities when switching
    // into a crossing and back out of it.
    let suzuka = circuits::all().iter().find(|c| c.id == "suzuka").unwrap();
    for circuit in [suzuka, circuits::first()] {
        let before =
            [loft, asphalt, grass].map(|e| app.world().get::<Mesh3d>(e).unwrap().0.clone());
        app.world_mut().write_message(GoTo(circuit));
        app.update();
        for (entity, old) in [loft, asphalt, grass].into_iter().zip(before) {
            assert_ne!(app.world().get::<Mesh3d>(entity).unwrap().0, old);
        }
    }
}

/// A circuit that passes over itself, built to order.
///
/// A lemniscate: two lobes meeting at the origin at right angles, which is
/// the smallest honest figure of eight there is. `turn` rolls the start
/// line round it, so that the crossing can be put a long way from the line
/// or right on top of it. Flat, because the point of it is the bridge and a
/// bridge the elevation model helped with would prove less.
///
/// This is the fixture the bridge was built against, before Suzuka. A real
/// circuit brings a surveyed layout, a real elevation model and one
/// crossing whose geometry is whatever it is; this brings a crossing at a
/// known place, at a known angle, with known heights either side of it, and
/// it can be asked for another one somewhere else.
pub(crate) fn figure_of_eight(turn: f32, over: f32, clearance: f32) -> &'static Circuit {
    const FIXES: usize = 240;
    const REACH: f32 = 950.0;
    // A negative turn runs the same shape the other way round, which is a
    // circuit whose bridge is met from the other end and whose lap counts
    // the other way. Everything about the crossing is the same; everything
    // about arriving at it is reversed.
    let (turn, widdershins) = (turn.abs(), turn < 0.0);
    let plan: Vec<[f32; 3]> = (0..FIXES)
        .map(|k| {
            let k = if widdershins { FIXES - k - 1 } else { k };
            let t = turn + std::f32::consts::TAU * k as f32 / FIXES as f32;
            let (sin, cos) = t.sin_cos();
            let below = 1.0 + sin * sin;
            [REACH * cos / below, 0.0, REACH * sin * cos / below]
        })
        .collect();
    Box::leak(Box::new(Circuit {
        id: "figure-of-eight",
        name: "Figure of Eight",
        corners: 1.0,
        lap: 0.0,
        plan_scale: 1.0,
        centreline: Box::leak(plan.into_boxed_slice()),
        crossings: Box::leak(Box::new([circuits::Crossing {
            at: [0.0, 0.0],
            over,
            clearance,
            provenance: "a fixture: the crossing is where the shape puts it",
        }])),
    }))
}

/// A bridge has the air under it that was asked for, everywhere the two
/// roads are one above the other.
///
/// Measured over the whole of the overlapping footprint rather than at the
/// point where the centrelines cross, because the roads are 7.3 m wide and
/// they cross at an angle: the tightest place is out at the edge of one of
/// them, not in the middle. Every rib of the upper deck, at every station
/// of it that has road underneath, against the surface of whatever is
/// underneath at that point of the map.
///
/// The clearance is measured to the *underside* of the deck, which is
/// [`DECK`] below the road. That is what a car driving under it has over
/// its head, and it is the only part of the two numbers that a driver ever
/// sees.
#[test]
fn a_bridge_has_the_air_under_it_that_was_asked_for() {
    for circuit in bridged() {
        air_under(circuit);
    }
}

/// Every circuit in the game came from one pinned revision of one source,
/// exactly once, and says so in three places that agree.
///
/// The three are the module's own header, the provenance file beside the
/// screening runs, and the list the menu is drawn from. They are written at
/// different times by different things — the generator writes the first two
/// and the third is edited by hand — so the way they go wrong is quietly:
/// a circuit generated twice under two names, a circuit whose trace was
/// refreshed from `master` while the rest came from a pinned commit, a
/// provenance entry left behind by a module that was deleted, a trace
/// turned to begin somewhere other than the line recorded for it.
///
/// That last one is why the line is checked here rather than in the game:
/// the game cannot tell, because the first sample of the trace *is* the
/// line as far as it is concerned, wherever it happens to be. Only the two
/// files can disagree, so only the two files can be held together.
///
/// Reading the repository from a test is unusual and is the point. What is
/// being checked is not what the code does with the data; it is that the
/// data is what it says it is.
#[test]
fn every_circuit_came_from_the_source_once() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let recorded = std::fs::read_to_string(root.join("docs/track-screening/sources.json"))
        .expect("the provenance file is beside the screening runs");
    let pinned = std::fs::read_to_string(root.join("tools/make_track.py"))
        .expect("the generator")
        .lines()
        .find_map(|line| {
            line.strip_prefix("REVISION = ")
                .map(|r| r.trim_matches('"').to_string())
        })
        .expect("the generator pins a revision");

    let mut sources: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(root.join("src/track/circuits")).expect("the circuits") {
        let path = entry.expect("a directory entry").path();
        if path.file_name().is_some_and(|name| name == "mod.rs") {
            continue;
        }
        let module = path
            .file_stem()
            .expect("a module name")
            .to_string_lossy()
            .to_string();
        let text = std::fs::read_to_string(&path).expect("a circuit module");
        let source = text
            .split("make_track.py ")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .unwrap_or_else(|| panic!("{module} does not say what it was generated from"))
            .to_string();
        assert!(
            text.contains(&pinned[..12]),
            "{module} does not name the pinned revision of the source"
        );
        assert!(
            recorded.contains(&format!("\"{module}\": {{")),
            "{module} is not in the provenance file"
        );
        assert!(
            recorded.contains(&format!("\"source_id\": \"{source}\"")),
            "{source} is not in the provenance file"
        );
        let line = text
            .split("start/finish line, ")
            .nth(1)
            .and_then(|rest| rest.lines().next())
            .unwrap_or_else(|| panic!("{module} does not say where its line is"))
            .trim_end_matches('.')
            .replace(", ", ",\n      ");
        assert!(
            recorded.contains(&format!("\"line\": [\n      {line}\n    ]")),
            "{module} begins at {line}, which is not the line recorded for it"
        );
        sources.push(source);
    }

    assert_eq!(
        sources.len(),
        circuits::all().len(),
        "there are {} circuit modules and {} circuits in the list",
        sources.len(),
        circuits::all().len()
    );
    assert_eq!(sources.len(), 40, "the source has forty circuits in it");
    sources.sort();
    let listed = sources.len();
    sources.dedup();
    assert_eq!(
        sources.len(),
        listed,
        "a circuit of the source was built twice under two names"
    );
}

/// A bridge says where it came from, and says which half of it is which.
///
/// The only thing in a circuit module that is not a fact about the real
/// circuit is the clearance: a 90 m ground model has nothing to say about
/// how far a road deck is above the road under it, and an authored number
/// dressed up as a measured one would be the worst kind of thing to leave
/// in a data file. So every crossing carries a line about where each half
/// of it came from, and this is what stops that line being dropped.
#[test]
fn a_bridge_says_where_it_came_from() {
    let mut found = 0;
    for circuit in circuits::all() {
        for crossing in circuit.crossings {
            found += 1;
            assert!(
                crossing.provenance.contains("authored"),
                "{}: a crossing whose clearance does not say it was authored: {}",
                circuit.name,
                crossing.provenance
            );
            assert!(
                crossing.clearance > 0.0 && crossing.clearance < 10.0,
                "{}: a clearance of {} m",
                circuit.name,
                crossing.clearance
            );
            assert!((0.0..1.0).contains(&crossing.over));
        }
    }
    assert_eq!(found, 1, "the game has {found} crossings, not one");
}

/// Every circuit that passes over itself: the one in the game, and the
/// fixture the feature was built against before there was one.
fn bridged() -> impl Iterator<Item = &'static Circuit> {
    circuits::all()
        .iter()
        .filter(|circuit| !circuit.crossings.is_empty())
        .chain([figure_of_eight(0.0, 0.75, 1.6)])
}

/// Where a circuit's declared crossings are, in the finished plan.
fn crossings_of(circuit: &'static Circuit) -> Vec<Vec3> {
    let plan = PLAN_SCALE * circuit.plan_scale;
    circuit
        .crossings
        .iter()
        .map(|c| Vec3::new(c.at[0] * plan, 0.0, c.at[1] * plan))
        .collect()
}

fn air_under(circuit: &'static Circuit) {
    let track = Track::new(circuit);
    let at = crossings_of(circuit);
    let wanted = circuit
        .crossings
        .iter()
        .map(|c| c.clearance)
        .fold(f32::MAX, f32::min);
    let stations = track.ribbon.stations();
    let lap = track.ribbon.length();
    let mut tightest = f32::MAX;
    let mut measured = 0;
    for (i, station) in stations.iter().enumerate() {
        // Only at a bridge. Elsewhere a circuit can perfectly well have one
        // stretch a couple of metres above another and a few metres to the
        // side of it — Suzuka's esses do, on the slope they are cut into —
        // and nothing about that is a span with air under it.
        if !at
            .iter()
            .any(|at| (station.pos - *at).reject_from(Vec3::Y).length() < ribbon::AT_CROSSING)
        {
            continue;
        }
        for rib in track.profile.at(i) {
            let on_top = station.pos + station.right * rib.0 + Vec3::Y * rib.1;
            for below in track.ribbon.nearby(on_top, UNDER_THE_CAR) {
                let apart = (below.s - station.s).abs();
                // The road this rib was swept from is not underneath it.
                if apart.min(lap - apart) < A_STEP_ALONG {
                    continue;
                }
                let under = below.point.y + track.profile.height(below.at, below.t, below.lateral);
                // Only where this rib really is above the other road, and
                // not beside it: a bridge is not asked to clear its own
                // approach.
                if on_top.y - under < A_DECK_APART {
                    continue;
                }
                measured += 1;
                tightest = tightest.min(on_top.y - DECK - under);
            }
        }
    }
    assert!(
        measured > 100,
        "{}: only {measured} places where one road is over another, so it is \
         not crossing itself",
        circuit.name
    );
    assert!(
        tightest > wanted - 0.01,
        "{}: the tightest place under the bridge has {tightest:.2} m of air, \
         against the {wanted:.2} m it was built with",
        circuit.name
    );
}

/// A bridge is road, so it obeys the same rules the rest of the road does:
/// the grade cap, and one lap's worth of everything.
///
/// The cap is the one worth saying out loud, because the bridge is built
/// *after* the cap has run — it has to be, or the cap takes it straight
/// back off — so nothing downstream is checking it and the ramps have to be
/// right by construction. A raised cosine's steepest point is half of π
/// times its average, so a rise of `r` over a ramp of [`RAMP_PER_RISE`]
/// times `r` peaks at 15.7% however big the bridge is, on top of whatever
/// the ground was doing — which the lift levels to a straight line first,
/// so that "whatever the ground was doing" is a number and not a worry.
#[test]
fn a_bridge_is_a_piece_of_road_like_any_other() {
    for circuit in bridged().chain([figure_of_eight(1.4, 0.0, 1.6)]) {
        let track = Track::new(circuit);
        let stations = track.ribbon.stations();
        let n = stations.len();
        let step = track.ribbon.length() / n as f32;
        let mut steepest = 0.0f32;
        for i in 0..n {
            let (a, b) = (stations[i].pos, stations[(i + 1) % n].pos);
            let run = (b - a).reject_from(Vec3::Y).length().max(1e-4);
            steepest = steepest.max((b.y - a.y).abs() / run);
        }
        assert!(
            steepest <= ribbon::MAX_GRADE,
            "{}: the bridge ramp reaches {steepest:.3}, past the cap of {}",
            circuit.name,
            ribbon::MAX_GRADE
        );
        // There is a deck, it is a deck's worth thick at the middle, and
        // it is drawn for exactly as long as its span and its ramps say.
        // Both come out of `Crossing`, so a bridge is described rather than
        // drawn by hand.
        let thickest = stations.iter().map(|s| s.deck).fold(0.0f32, f32::max);
        assert!(
            (thickest - 1.0).abs() < 1e-3,
            "{}: the deck is {thickest:.2} of a deck thick at its middle",
            circuit.name
        );
        let drawn = stations.iter().filter(|s| s.deck > 0.0).count() as f32 * step;
        let mut span = 0.0f32;
        for crossing in circuit.crossings {
            let rise = crossing.clearance + DECK + profile::SECTION_DEEP;
            span += 2.0 * (DECK_SPAN + RAMP_PER_RISE * rise);
        }
        assert!(
            (drawn - span).abs() < 2.0 * step,
            "{}: there is {drawn:.0} m of deck where its span and ramps come \
             to {span:.0} m",
            circuit.name
        );

        // And the two roads that meet are as far apart as the clearance
        // needs, however much of that the elevation model had already given
        // and however much had to be added. Suzuka's model gives all of it
        // and more; the figure of eight is flat and is given none.
        for (crossing, at) in circuit.crossings.iter().zip(crossings_of(circuit)) {
            let rise = crossing.clearance + DECK + profile::SECTION_DEEP;
            let mut heights: Vec<f32> = stations
                .iter()
                .filter(|s| (s.pos - at).reject_from(Vec3::Y).length() < 2.0 * profile::HALF_WIDTH)
                .map(|s| s.pos.y)
                .collect();
            heights.sort_by(f32::total_cmp);
            let apart = heights[heights.len() - 1] - heights[0];
            assert!(
                apart > rise - 0.01,
                "{}: the two roads that meet are {apart:.2} m apart where the \
                 clearance needs {rise:.2} m",
                circuit.name
            );
        }
    }
}

/// What the car stands on over a bridge is the deck it is on, and the two
/// things that decide that both work.
///
/// Walked over every station of both decks, on the centreline and out at
/// both wheels, which is where a lookup by height alone is least reliable —
/// the outer wheel on the deck is lower than the middle of it, and on a
/// ramp it is lower still.
///
/// Three questions, and the third is the one worth having. Told where it
/// was, the lookup keeps the car on the road it was on. Told nothing, it
/// picks by height and gets the same answer, which is what a ghost dropped
/// onto a saved pose relies on. Told that it was on the *other* deck, it
/// says the other deck — because if it did not, continuity would not be
/// deciding anything and the first answer would be luck.
#[test]
fn what_the_car_stands_on_over_a_bridge_is_the_deck_it_is_on() {
    let track = Track::new(figure_of_eight(0.0, 0.75, 1.6));
    let stations = track.ribbon.stations();
    let lap = track.ribbon.length();
    let wheel = crate::car::HALF_TRACK;
    let round = |a: f32, b: f32| {
        let d = (a - b).abs();
        d.min(lap - d)
    };
    let mut crossed = 0;
    for (i, station) in stations.iter().enumerate() {
        for across in [-wheel, 0.0, wheel] {
            let lateral = across;
            let on = station.pos
                + station.right * lateral
                + Vec3::Y * track.profile.height(i, 0.0, lateral);
            // Told where it was.
            let kept = track.fix(on, Some(station.s));
            assert!(
                round(kept.s, station.s) < 1.0,
                "at s={} and {across:.1} m across, the car reads as being at \
                 s={} — it has changed decks standing still",
                station.s,
                kept.s
            );
            // Told nothing.
            let guessed = track.fix(on, None);
            assert!(
                round(guessed.s, station.s) < 1.0,
                "at s={} and {across:.1} m across, a lookup with no history \
                 reads s={}",
                station.s,
                guessed.s
            );
            // Told it was on the other deck, where there is one.
            let others: Vec<f32> = track
                .ribbon
                .nearby(on, UNDER_THE_CAR)
                .iter()
                .map(|fix| fix.s)
                .filter(|s| round(*s, station.s) > A_STEP_ALONG)
                .collect();
            for other in others {
                crossed += 1;
                let swapped = track.fix(on, Some(other));
                assert!(
                    round(swapped.s, other) < 1.0,
                    "told it was at s={other}, the lookup put the car at \
                     s={} instead",
                    swapped.s
                );
            }
        }
    }
    assert!(
        crossed > 20,
        "only {crossed} places with two roads to choose between: the fixture \
         is not crossing itself"
    );
}

/// Driving under the start line is not a lap.
///
/// The start plane goes on for ever and the start line does not. With the
/// crossing a few metres past the line, the lower road passes under the
/// plane, across it, and within a road's width of the middle of it — every
/// test the gate used to make — and it is not the line, because it is nine
/// metres below it and half a lap away.
#[test]
fn driving_under_the_start_line_is_not_a_lap() {
    // The crossing put within half a metre of the start line, which is the
    // only place this question can be asked from.
    let track = Track::new(figure_of_eight(1.5, 0.0, 1.6));
    let stations = track.ribbon.stations();
    let lap = track.ribbon.length();
    let start = stations[0];
    let round = |a: f32| {
        let d = (a - start.s).abs();
        d.min(lap - d)
    };
    // The stretch that goes under, where it passes beneath the line.
    let below: Vec<&ribbon::Station> = stations
        .iter()
        .filter(|s| {
            round(s.s) > A_STEP_ALONG && (s.pos - start.pos).reject_from(Vec3::Y).length() < 2.0
        })
        .collect();
    assert!(
        !below.is_empty(),
        "nothing passes under the line, so there is nothing to refuse"
    );
    for station in &below {
        assert!(
            start.pos.y - station.pos.y > A_DECK_APART,
            "the road under the line is not under it"
        );
        // It crosses the plane, it is beside the middle of the line, and it
        // is still not the line.
        let across = (station.pos - start.pos)
            .reject_from(Vec3::Y)
            .dot(start.right);
        assert!(across.abs() < HALF_WIDTH + 2.0 * crate::car::HALF_TRACK);
        assert!(
            !track.on_start_gate(station.pos, Some(station.s)),
            "a lap was completed on the road under the line"
        );
        assert!(
            !track.on_start_gate(station.pos, None),
            "a lap was completed on the road under the line, with no history"
        );
    }
    // And the line itself still opens, which is the other half of it.
    assert!(track.on_start_gate(start.pos, Some(start.s)));
    assert!(track.on_start_gate(start.pos, None));
}

/// A lap is a lap of the surface it was driven on, and the fingerprint has
/// to move when that surface does — in any of the ways it can.
///
/// This is a property correction, and the wrong proxy is worth naming. The
/// fingerprint used to be of the finished *centreline*, and that was a
/// complete description of the driving surface for exactly as long as every
/// circuit carried the same road and the same verge from its first station
/// to its last: nothing about the surface could move unless the centreline
/// moved. The verge is fitted station by station now. A circuit can keep
/// its centreline to the bit and put the wall a metre further in, and a
/// saved lap that used that metre would replay through the scenery with the
/// clock saying it was fine.
///
/// So the claim is now about the surface and not the line: the cross-section
/// at every station goes into the hash as well. And the negative half of it
/// is checked too — paint is deliberately not in there, because recolouring
/// a kerb stripe changes the mesh, changes nothing a lap time depends on,
/// and throwing away everyone's ghosts over it would teach people to
/// distrust the check.
#[test]
fn a_lap_belongs_to_the_surface_it_was_driven_on() {
    let track = track();
    let ribbon = &track.ribbon;
    let profile = &track.profile;
    let mine = hash(&surface(ribbon, profile));
    assert_eq!(mine, track.fingerprint());
    // Building the same circuit again is the same surface.
    assert_eq!(mine, Track::any().fingerprint());

    // A centimetre off one station's left verge, and nothing else at all:
    // the same centreline, the same heights, the same paint. The old
    // centreline-only hash could not see this, which is the whole reason
    // this test exists.
    let narrower = profile.nudged(0, -0.01);
    assert_ne!(
        mine,
        hash(&surface(ribbon, &narrower)),
        "a width-only change left the fingerprint where it was"
    );

    // A height-only change: the same plan, the hills a hundredth taller.
    let circuit = track.circuit();
    let taller: Vec<Vec3> = circuit
        .centreline
        .iter()
        .map(|p| {
            Vec3::new(
                p[0] * PLAN_SCALE,
                p[1] * HEIGHT_SCALE * 1.01,
                p[2] * PLAN_SCALE,
            )
        })
        .collect();
    let taller = Ribbon::new(&taller, circuit.corners, &[]);
    let fitted = Profile::fit(&taller).expect("the same circuit still carries a road");
    assert_ne!(
        mine,
        hash(&surface(&taller, &fitted)),
        "a height-only change left the fingerprint where it was"
    );

    // And paint is not in it. Two cross-sections of exactly the same shape,
    // one of them repainted: the same numbers go into the hash.
    let plain = profile::section(3.0, 3.0);
    let mut repainted = plain;
    repainted[0].2 = profile::Band::Grass;
    assert_ne!(plain[0].2, repainted[0].2, "nothing was actually repainted");
    let flatten = |ribs: &profile::Section| -> Vec<f32> {
        ribs.iter().flat_map(|&(l, h, _)| [l, h]).collect()
    };
    assert_eq!(
        hash(&flatten(&plain)),
        hash(&flatten(&repainted)),
        "repainting a strip would throw away every saved lap"
    );
    assert_ne!(
        hash(&flatten(&plain)),
        hash(&flatten(&profile::section(3.0, 2.99))),
        "a centimetre of verge reads as the same surface"
    );
}

/// Scratch: the finished lap of each circuit, by module.
#[test]
#[ignore]
fn the_laps() {
    for circuit in circuits::all() {
        println!("{}|{:.1}", circuit.id, Track::new(circuit).ribbon.length());
    }
}

/// What every circuit came out as: the table the bars in these tests are set
/// against, and the first thing to look at when a new one will not go in.
///
/// `cargo test --locked --lib the_circuits -- --ignored --nocapture`
#[test]
#[ignore]
fn the_circuits() {
    println!(
        "{:<26}{:>8}{:>7}{:>15}{:>8}{:>9}{:>8}",
        "circuit", "lap", "kept", "cross-section", "pinned", "straight", "relief"
    );
    for (name, track) in every_track() {
        let stations = track.ribbon.stations();
        let n = stations.len();
        let grade = |i: usize| {
            let (a, b) = (stations[i].pos, stations[(i + 1) % n].pos);
            let run = Vec3::new(b.x - a.x, 0.0, b.z - a.z).length().max(1e-4);
            (b.y - a.y).abs() / run
        };
        let pinned = (0..n)
            .filter(|&i| grade(i) > ribbon::MAX_GRADE * 0.98)
            .count();
        let window = straight_window(&track, n);
        let straight = (0..n)
            .filter(|&i| {
                let from = stations[i].pos;
                let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
                let dir = flat(stations[(i + window) % n].pos - from).normalize_or(Vec3::X);
                (0..=window).all(|k| {
                    let off = flat(stations[(i + k) % n].pos - from);
                    (off - dir * off.dot(dir)).length() <= TARMAC_HALF
                })
            })
            .count();
        let (low, high) = stations.iter().fold((f32::MAX, f32::MIN), |(l, h), s| {
            (l.min(s.pos.y), h.max(s.pos.y))
        });
        let (narrowest, widest) = track.profile.span();
        println!(
            "{name:<26}{:>7.0}m{:>6.0}%{:>9.2}-{:>4.2}m{:>7.0}%{:>8.0}%{:>7.1}m",
            track.ribbon.length(),
            track.ribbon.kept() * 100.0,
            narrowest,
            widest,
            100.0 * pinned as f32 / n as f32,
            100.0 * straight as f32 / n as f32,
            high - low,
        );
    }
}

/// How much a circuit's elevation model says the road climbs.
///
/// Not the raw range, which is the proxy this replaces and which one bad
/// sample can make up entirely. The model is a 90 m ground elevation read
/// at the trace's own fixes, on circuits that run between grandstands,
/// under bridges and through car parks, so a single fix tens of metres
/// above both of its neighbours is a building. Miami's raw range is 12 m
/// and 10 of them are one fix; Las Vegas's is 30 m and 11 of them are;
/// Hockenheim has a lone 16 m sample between a 6 and a 5.
///
/// So a height is taken with its two neighbours and the middle one kept — a
/// hill two fixes wide is a hill, and one fix on its own is not. Nothing
/// downstream reads this: it is what the *test* expects the circuit to
/// climb, and correcting it is what stopped the smoothing being blamed for
/// refusing to build a grandstand.
fn relief_of(circuit: &Circuit) -> f32 {
    let heights: Vec<f32> = circuit.centreline.iter().map(|p| p[1]).collect();
    let n = heights.len();
    let (low, high) = (0..n)
        .map(|i| {
            let mut three = [heights[(i + n - 1) % n], heights[i], heights[(i + 1) % n]];
            three.sort_by(f32::total_cmp);
            three[1]
        })
        .fold((f32::MAX, f32::MIN), |(l, h), y| (l.min(y), h.max(y)));
    high - low
}

/// Stations in the straight line a circuit is measured against.
///
/// 40 m of *source* circuit, which is 40 m of Todora on the thirty-five
/// circuits built at the shared scale and more on the four that are not.
/// The question being asked is about the layout — is there a corner here,
/// or a bend the road can be driven straight through — and the layout
/// belongs to the real circuit. Ask it with a fixed 40 m and a circuit
/// built at two and a half times the plan is asked whether a line fits down
/// a fortieth of itself rather than a fifteenth, which every circuit would
/// pass and which says nothing about any of them.
fn straight_window(track: &Track, n: usize) -> usize {
    let step = track.ribbon.length() / n as f32;
    (40.0 * track.circuit().plan_scale / step) as usize
}

/// A corner has to be a corner: something the driver goes round, not a bend
/// the road is wide enough to ignore.
///
/// The plan is shrunk about seven and a half times and the road only about
/// one and a half, so the road comes out five times too wide for the land it
/// is laid on. Left alone, that turns a chicane into a straight — the whole
/// of Monza's Rettifilo displaces this car by less than half a road width,
/// so the quick way through is not to steer. `Circuit::corners` is what buys
/// it back, and this is what says whether it bought enough.
///
/// Measured by laying a 40 m straight line down the circuit at every station
/// and asking whether it stays on the asphalt. Long straights count too and
/// are meant to — Monza is a third straights and is not broken — so the bar
/// is loose. It is there to catch a circuit that is nearly all straight.
#[test]
fn corners_are_corners() {
    for (name, track) in every_track() {
        let stations = track.ribbon.stations();
        let n = stations.len();
        let window = straight_window(&track, n);
        let straight = (0..n)
            .filter(|&i| {
                let from = stations[i].pos;
                let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
                let dir = flat(stations[(i + window) % n].pos - from).normalize_or(Vec3::X);
                (0..=window).all(|k| {
                    let off = flat(stations[(i + k) % n].pos - from);
                    (off - dir * off.dot(dir)).length() <= TARMAC_HALF
                })
            })
            .count();
        let fraction = straight as f32 / n as f32;
        assert!(
            fraction < 0.6,
            "{name}: a 40 m straight fits down {:.0}% of the lap — \
             there is not much circuit in there to drive round",
            fraction * 100.0
        );
    }
}

/// The menu can reach every circuit and land on the right row for each: the
/// list is what it is drawn from, and where a circuit sits in that list is
/// where the cursor opens.
#[test]
fn the_menu_can_reach_every_circuit() {
    let all = all_circuits();
    assert!(all.len() > 1, "there is only one circuit to list");
    assert!(
        all.iter().any(|c| c.id == circuits::first().id),
        "the game opens on a circuit the menu cannot reach"
    );
    for (at, circuit) in all.iter().enumerate() {
        assert_eq!(circuit_at(circuit), at, "{} is listed twice", circuit.name);
        assert!(
            !circuit.id.is_empty() && !circuit.name.is_empty(),
            "a circuit with nothing to show in a menu"
        );
    }
}

/// Every circuit fits inside the ideal cross-section, and none of them is a
/// second copy of another.
///
/// Asked of the shape rather than of where the grid slot lands. Every
/// circuit is laid out about its own centroid, so they all sit on top of
/// each other near the origin and two grid slots being close together says
/// nothing about the circuits — Silverstone's start is 40 m from Monza's,
/// and they are not remotely the same place. [`Track::fingerprint`] is of
/// the finished centreline, which is exactly what "a different circuit"
/// means, and it is already trusted to tell one lap's shape from another's
/// when a saved ghost is read back.
#[test]
fn every_circuit_is_its_own_place() {
    let mut shapes = Vec::new();
    for (name, track) in every_track() {
        assert!(track.profile.span().1 <= EDGE, "{name}");
        shapes.push((name, track.fingerprint(), track.circuit().id));
    }
    for (i, (name, shape, id)) in shapes.iter().enumerate() {
        for (other, theirs, other_id) in &shapes[i + 1..] {
            assert_ne!(shape, theirs, "{name} and {other} are the same shape");
            assert_ne!(id, other_id, "{name} and {other} are filed under one id");
        }
    }
}
