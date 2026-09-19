//! Drivers who are not the player.
//!
//! A [`Driver`] turns the road ahead into the same [`Controls`] the keys
//! produce, so the engine cannot tell who is driving. Two styles. The *plain*
//! driver steers proportionally, looks a long way up the road and brakes on
//! time — if it cannot get round, nobody can. The *clumsy* driver is a person on
//! a keyboard: full lock or nothing, a reaction time, a short look ahead, brakes
//! that go on late. If it gets round, the car is easy to learn — and that is the
//! test that matters, because the plain driver was lapping happily while the
//! person holding the keys was not.
//!
//! Nothing spawns a driver yet; the lap harness in the tests is its first user,
//! and opponents or a ghost car would be its second. It is a module rather than
//! a test fixture so that day needs no rework.
#![cfg_attr(not(test), allow(dead_code))]

use std::collections::VecDeque;

use bevy::prelude::*;

use super::level;
use super::physics::{Car, Controls, GRAVITY, Handling};
use crate::track::{ROAD_HALF, Track};

/// Metres of road between one look-ahead probe and the next.
///
/// The resolution of the look-ahead, not its reach — how far each driver looks
/// is a count of these and is part of what makes it that driver. A corner at
/// the game's tightest radius is 5 m of road; sampling every 1.5 m puts three
/// probes inside one, where the old 3 m against a 10 m radius put three inside
/// one as well. The same statement about a road half the width.
const PROBE: f32 = 1.5;
/// How hard the driver winds the wheel to come back to the middle of the road,
/// at a full road half-width off it.
///
/// Per road half-width and not per metre, which is the correction: a metre off
/// line is most of the way to the grass on a 3.3 m road and a quarter of the
/// way there on the 8 m road this was tuned against, and a driver that pulls
/// the same amount in both cases is either asleep on one road or sawing at the
/// wheel on the other. The figure itself is what the old per-metre gain came to
/// at the edge of the old road, so on that road nothing has changed.
const PULL: f32 = 0.64;
/// How hard it winds the wheel to point back down the road, per radian of
/// error. This is the damping — the term that stops [`PULL`] overshooting — and
/// it is about the car and the road's direction rather than the road's width,
/// so it did not move.
const STRAIGHTEN: f32 = 1.6;
/// How wrong it has to look before the clumsy driver touches the wheel at all.
///
/// In the units of `correction`, which is worth unpacking, because it is a
/// claim about a person: this is the nose eleven degrees off the road, or the
/// car half way from the middle of the road to the grass, or some mix of the
/// two. Under that, a driver on a keyboard does nothing — there is no
/// small input available to them, so the alternative to doing nothing is full
/// lock, and full lock for a nose that is nearly straight is a swerve.
///
/// It used to be one degree, and one degree was never a keyboard driver's idea
/// of crooked. It was a figure that happened to stay out of trouble on an 8 m
/// road, because on an 8 m road full lock has room to be wrong in: the swerve
/// it causes is a fifth of the road. On a 3.3 m road the same swerve is half
/// the road, the correction for it is another swerve, and the car saws itself
/// into the grass and stays there. Every circuit goes round anywhere from 0.26
/// to at least 0.36, and this is the middle of that; at 0.22 the clumsy driver
/// gets 90% of the way round Estoril against a bar of 90, which is not a
/// different kind of behaviour so much as an unlucky place to stand.
const NOTICES: f32 = 0.30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Style {
    Plain,
    Clumsy,
}

pub(crate) struct Driver {
    pub style: Style,
    /// What the driver is reacting to: the world as it was a reaction time ago.
    seen: VecDeque<Seen>,
}

/// What makes one driver that driver, rather than the other one.
struct Habits {
    /// Frames of delay at 120 Hz: the world it is reacting to is the world as
    /// it was this long ago.
    reaction: usize,
    /// Probes up the road, [`PROBE`] apart.
    lookahead: usize,
    /// How far past its own corner limit it gets before the brakes go on.
    late: f32,
    /// How much of the tyre it *plans* to use in a corner.
    ///
    /// Not how much is there — how much it leaves itself. The rest is the
    /// margin for a bump, a slide, and for not being exactly on the line when
    /// the corner arrives, and that last one is why this is smaller than it
    /// was: on an 8 m road a car a metre and a half wide of its line is still
    /// on the asphalt, and on a 3.3 m road it is in the grass with a tenth of
    /// the drag it had. Monaco is where that showed — its line comes
    /// immediately after Rascasse, and a plain driver planning on 70% of the
    /// tyre arrived at Anthony Noghès 5% too fast, ran wide, bogged down in the
    /// grass and crossed its own start line at 3 m/s.
    ///
    /// The clumsy driver keeps the old figure, and that is the point of it
    /// rather than an oversight. It is the driver who does not plan: it brakes
    /// late on purpose, and a margin it left itself deliberately would be a
    /// different driver. It pays for the optimism by being off the road half
    /// the time, which is what it is there to show.
    plans_on: f32,
}

#[derive(Clone, Copy)]
struct Seen {
    correction: f32,
    speed: f32,
    limit: f32,
}

impl Driver {
    pub(crate) fn new(style: Style) -> Self {
        Self {
            style,
            seen: VecDeque::new(),
        }
    }

    /// What makes this driver that driver.
    ///
    /// The lookahead counts are what they are so that the *distances* are what
    /// they were: 48 m for the plain driver and 18 m for the clumsy one, which
    /// is the difference between seeing the corner and arriving at it. Halving
    /// the probe spacing without doubling the counts would have quietly made
    /// both drivers short-sighted and called it the narrower road's fault. 48 m
    /// is also comfortably past the hardest stop the game has in it, which is
    /// what the plain driver's promise — if it cannot get round, nobody can —
    /// rests on.
    fn habits(&self) -> Habits {
        match self.style {
            Style::Plain => Habits {
                reaction: 0,
                lookahead: 32,
                late: 1.04,
                plans_on: 0.55,
            },
            Style::Clumsy => Habits {
                reaction: 18,
                lookahead: 12,
                late: 1.15,
                plans_on: 0.70,
            },
        }
    }

    /// What this driver does with the car where it is now.
    pub(crate) fn decide(
        &mut self,
        track: &Track,
        handling: &Handling,
        transform: &Transform,
        car: &Car,
    ) -> Controls {
        let Habits {
            reaction,
            lookahead,
            late,
            plans_on,
        } = self.habits();
        let heading = level(*transform.forward());
        let ground = track.ground(transform.translation);
        // The way the lap runs, not the way the car happens to be pointing:
        // aligning the target to the car lets a driver lap backwards.
        let ahead = ground.tangent;
        // Signed, so facing the wrong way reads as half a turn of error rather
        // than as no error at all.
        let astray = f32::atan2(heading.cross(ahead).y, heading.dot(ahead));
        let correction = astray * STRAIGHTEN + PULL * ground.lateral / ROAD_HALF;

        // Hold a speed the tyres can corner at, looking at the tightest bend
        // between here and as far up the road as this driver looks. Conservative
        // about grip — no credit for downforce — and it knows a descent has the
        // hill working against the brakes.
        let hold = plans_on * handling.grip * ground.grip;
        let downhill = (-ground.slope * ground.tangent.dot(heading) * GRAVITY).max(0.0);
        let stopping = (0.75 * handling.brake * ground.grip - downhill).max(hold * 0.4);
        // Up the road, not off down the car's nose. A straight-line probe
        // leaves the road at the first corner: three metres past the apex it is
        // out in the grass, and on a circuit that doubles back it lands on the
        // neighbouring straight and reports that straight's curvature as the
        // corner about to arrive. Stepping along the ribbon by arc distance
        // follows whatever road the car is on, round the corner and through it.
        let mut limit = handling.top_speed;
        for step in 0..=lookahead {
            let reach = step as f32 * PROBE;
            let bend = track.curvature_ahead(&ground, reach);
            let corner = hold / bend.abs().max(0.002);
            limit = limit.min((corner + 2.0 * stopping * reach).sqrt());
        }
        let speed = car.velocity.length();

        self.seen.push_back(Seen {
            correction,
            speed,
            limit,
        });
        let Seen {
            correction,
            speed: seen_speed,
            limit,
        } = if self.seen.len() > reaction {
            self.seen.pop_front().unwrap()
        } else {
            Seen {
                correction,
                speed,
                limit,
            }
        };

        match self.style {
            Style::Clumsy => {
                let too_fast = seen_speed > limit * late;
                Controls {
                    throttle: if too_fast { 0.0 } else { 1.0 },
                    brake: if too_fast { 1.0 } else { 0.0 },
                    steer: if correction > NOTICES {
                        1.0
                    } else if correction < -NOTICES {
                        -1.0
                    } else {
                        0.0
                    },
                    handbrake: false,
                }
            }
            Style::Plain => {
                // Does not drive hard at anything but the road ahead — unless
                // stopped, when sitting still pointing the wrong way is the one
                // thing that gets you nowhere.
                let busy = car.g_force.x.abs() > 0.75 || car.rear_slip > 0.2;
                Controls {
                    throttle: if speed < 1.0
                        || (seen_speed < limit * 0.96 && !busy && astray.abs() < 0.6)
                    {
                        1.0
                    } else {
                        0.0
                    },
                    brake: if seen_speed > limit * late { 1.0 } else { 0.0 },
                    steer: correction.clamp(-1.0, 1.0),
                    handbrake: false,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Can the car get round the circuit? Everything else is a number in
    //! isolation; these lap the real track through the real game path — the same
    //! `advance` the system calls, the same `hold` the track applies.
    //!
    //! Neither driver is a tuning oracle. Chasing their numbers rewards a slow,
    //! dull car; what they guard is that the car stays drivable.

    use super::*;
    use crate::car::physics::{self, Surface};
    use crate::car::{SCALE, Setup, Spec, advance};
    use crate::track::{MIN_RADIUS, all_circuits};

    const FLAT: Surface = Surface {
        grip: 1.0,
        slope: 0.0,
    };

    /// Metres a second the time budget assumes. Well under what the car can do,
    /// because these drivers are not quick — it is how long a lap is *allowed*
    /// to take, not how long it should.
    const PACE: f32 = 5.9;

    #[derive(Default)]
    struct Lap {
        distance: f32,
        progress: f32,
        off_road: f32,
        in_the_weeds: f32,
        stopped: f32,
        descending: f32,
        off_road_descending: f32,
    }

    /// Drive `track` for `laps` laps' worth of time at [`PACE`]. The budget
    /// follows the circuit rather than being a fixed number of seconds: the
    /// circuits are different lengths on purpose, and 90 seconds that is most of
    /// a lap of Spielberg is half a lap of Spa.
    fn lap(track: &Track, style: Style, handling: Handling, laps: f32) -> Lap {
        let seconds = laps * track.length() / PACE;
        let mut driver = Driver::new(style);
        let mut transform = track.start_transform().with_scale(Vec3::splat(SCALE));
        let mut car = Car::default();
        let dt = 1.0 / 120.0;
        let mut lap = Lap::default();
        let mut previous = track.progress(transform.translation, car.along);

        for _ in 0..(seconds / dt) as usize {
            let controls = driver.decide(track, &handling, &transform, &car);
            let speed = car.velocity.length();
            advance(track, &handling, controls, &mut transform, &mut car, dt);

            lap.distance += speed * dt;
            let ground = track.ground_from(transform.translation, car.along);
            let heading = level(*transform.forward());
            // Off the road is past the kerb, and in the weeds is most of the
            // way out to where the car is held. Both read off the road rather
            // than off the numbers the road happened to have when these were
            // written.
            let off = ROAD_HALF;
            let weeds = ROAD_HALF + 0.5 * (ground.edge - ROAD_HALF);
            if ground.lateral.abs() > off {
                lap.off_road += dt;
            }
            if ground.lateral.abs() > weeds {
                lap.in_the_weeds += dt;
            }
            if ground.slope * ground.tangent.dot(heading) < -0.04 {
                lap.descending += dt;
                if ground.lateral.abs() > off {
                    lap.off_road_descending += dt;
                }
            }
            if speed < 1.5 {
                lap.stopped += dt;
            }
            let progress = track.progress(transform.translation, car.along);
            lap.progress += (progress - previous + 0.5).rem_euclid(1.0) - 0.5;
            previous = progress;
        }
        lap
    }

    /// Drive one timed lap, the way the game times one: the run-up arms the
    /// clock at the line and the next crossing of it, a lap's worth of progress
    /// later, stops it. `None` if the driver never got round — which is a
    /// result, and the one the balance tests would rather hear about than
    /// average away.
    fn lap_time(track: &Track, style: Style, handling: Handling) -> Option<f32> {
        let mut driver = Driver::new(style);
        let mut transform = track.start_transform().with_scale(Vec3::splat(SCALE));
        let mut car = Car::default();
        let dt = 1.0 / 120.0;
        let mut was = track.start_along(transform.translation);
        let mut previous = track.progress(transform.translation, car.along);
        let (mut running, mut clock, mut round) = (false, 0.0f32, 0.0f32);
        // Three laps' worth of time at PACE before giving up on one.
        for _ in 0..(3.0 * track.length() / PACE / dt) as usize {
            let controls = driver.decide(track, &handling, &transform, &car);
            advance(track, &handling, controls, &mut transform, &mut car, dt);
            let progress = track.progress(transform.translation, car.along);
            round += (progress - previous + 0.5).rem_euclid(1.0) - 0.5;
            previous = progress;
            if running {
                clock += dt;
            }
            let along = track.start_along(transform.translation);
            if was <= 0.0 && along > 0.0 && track.on_start_gate(transform.translation, car.along) {
                if !running {
                    running = true;
                    clock = 0.0;
                    round = 0.0;
                } else if round > 0.95 {
                    return Some(clock);
                }
            }
            was = along;
        }
        None
    }

    fn report(who: &str, lap: &Lap) {
        println!(
            "{who}: round {:.0}%  {:.0} m  off-road {:.1} s  weeds {:.1} s  stopped {:.1} s  \
             (descending {:.1} s, {:.0}% of it off-road; elsewhere {:.0}%)",
            lap.progress * 100.0,
            lap.distance,
            lap.off_road,
            lap.in_the_weeds,
            lap.stopped,
            lap.descending,
            100.0 * lap.off_road_descending / lap.descending.max(0.1),
            100.0 * (lap.off_road - lap.off_road_descending) / (90.0 - lap.descending).max(0.1)
        );
    }

    /// Every circuit, because a circuit nobody can get round is not a circuit.
    /// This is the net under the hills: the elevation comes from a real DEM at a
    /// scale that makes it steeper than life, and a climb the car cannot take or
    /// a descent it cannot stop on fails here rather than under the player.
    #[test]
    fn a_plain_driver_gets_round() {
        for circuit in all_circuits() {
            let track = Track::new(circuit);
            let budget = track.length() / PACE;
            let lap = lap(&track, Style::Plain, Handling::SHOOTING_BRAKE, 1.0);
            report(circuit.name, &lap);
            assert!(
                lap.progress > 0.9,
                "{}: {budget:.0} s only got {:.0}% round",
                circuit.name,
                lap.progress * 100.0
            );
            // Loose on purpose: the bounds of "can get round", not a target.
            assert!(
                lap.off_road < budget / 3.0,
                "{}: off the road {:.0} s of {budget:.0}",
                circuit.name,
                lap.off_road
            );
            assert!(
                lap.stopped < budget / 6.0,
                "{}: going nowhere {:.0} s of {budget:.0}",
                circuit.name,
                lap.stopped
            );
        }
    }

    /// Full lock or nothing, 150 ms behind, brakes late, cannot see far. This is
    /// the person holding the keys, and the car has to be drivable by them.
    #[test]
    fn a_clumsy_driver_still_gets_round() {
        for circuit in all_circuits() {
            let track = Track::new(circuit);
            let budget = track.length() / PACE;
            let lap = lap(&track, Style::Clumsy, Handling::SHOOTING_BRAKE, 1.0);
            report(circuit.name, &lap);
            assert!(
                lap.progress > 0.9,
                "{}: {budget:.0} s only got {:.0}% round",
                circuit.name,
                lap.progress * 100.0
            );
            // Loose: the grass is a gravel trap, so every excursion this
            // driver makes is a slow one, and it makes plenty. What matters is
            // that it is never stuck out there.
            assert!(
                lap.off_road < budget / 2.0,
                "{}: off the road {:.0} s of {budget:.0}",
                circuit.name,
                lap.off_road
            );
            assert!(
                lap.stopped < budget / 4.5,
                "{}: going nowhere {:.0} s of {budget:.0}",
                circuit.name,
                lap.stopped
            );
        }
    }

    /// A circuit that passes over itself is driven round it, not through it.
    ///
    /// The plain driver takes the synthetic figure of eight both ways round,
    /// and three things are watched all the way.
    ///
    /// It never changes deck. The car's own idea of how far round the lap it is
    /// moves by centimetres a step; if the lookup ever handed it the road
    /// underneath instead, that number would jump by most of a lap, and it is
    /// the number the clock, the ghost and the wall all read.
    ///
    /// It is always standing on the road it thinks it is on. The two decks are
    /// metres apart in height at the crossing, so a car on the wrong one is a
    /// car in the air or a car in the deck.
    ///
    /// And the lap turns over exactly once. A figure of eight passes its own
    /// crossing twice a lap, and a progress reading that took the other deck
    /// either time would count most of a lap in one step.
    #[test]
    fn a_plain_driver_gets_round_a_circuit_that_crosses_itself() {
        let eights = [1.0f32, -1.0].map(|turn| crate::track::figure_of_eight(turn, 0.75, 1.6));
        let crossers = all_circuits()
            .iter()
            .filter(|circuit| !circuit.crossings.is_empty())
            .chain(eights);
        for circuit in crossers {
            let track = Track::new(circuit);
            let handling = Handling::SHOOTING_BRAKE;
            let mut driver = Driver::new(Style::Plain);
            let mut transform = track.start_transform().with_scale(Vec3::splat(SCALE));
            let mut car = Car {
                along: Some(track.start_along_lap()),
                ..Car::default()
            };
            let dt = 1.0 / 120.0;
            let lap = track.length();
            let mut round = 0.0f32;
            let mut previous = track.progress(transform.translation, car.along);
            let mut wraps = 0;
            let mut furthest = 0.0f32;
            for _ in 0..(4.0 * lap / PACE / dt) as usize {
                let was = car
                    .along
                    .expect("the car was put on the grid knowing where");
                let controls = driver.decide(&track, &handling, &transform, &car);
                advance(&track, &handling, controls, &mut transform, &mut car, dt);
                let now = car.along.expect("the car is still on the circuit");
                let moved = (now - was).abs();
                furthest = furthest.max(moved.min(lap - moved));
                // Standing on what it thinks it is standing on.
                let ground = track.ground_from(transform.translation, Some(now));
                assert!(
                    (transform.translation.y - ground.height).abs() < 0.05,
                    "the car is {:.2} m off the deck it says it is on",
                    transform.translation.y - ground.height
                );
                let progress = track.progress(transform.translation, car.along);
                let step = progress - previous;
                if step < -0.5 {
                    wraps += 1;
                    // The lap proper is between one crossing of the line and
                    // the next, so the count starts at the first of them.
                    if wraps == 1 {
                        round = 0.0;
                    }
                } else {
                    assert!(
                        step.abs() < 0.02,
                        "progress jumped from {previous} to {progress} in one step"
                    );
                    round += step;
                }
                previous = progress;
                if wraps == 2 {
                    break;
                }
            }
            assert!(
                furthest < 1.0,
                "{}: the car moved {furthest:.1} m round the lap in one step of \
                 1/120 s, which is a change of deck rather than a change of place",
                circuit.name
            );
            assert_eq!(
                wraps, 2,
                "{}: the line went by {wraps} times rather than twice",
                circuit.name
            );
            // Between the two crossings is one lap of progress and no more —
            // and a figure of eight passes its own bridge twice in it.
            assert!(
                (round - 1.0).abs() < 0.02,
                "{}: one lap between two crossings of the line came to {round:.3}",
                circuit.name
            );
        }
    }

    /// Every notch of the setup slider has to be lappable by the person holding
    /// the keys, not just the one the car ships on — and now on every car, not
    /// just the one the game opens on. A setup that looks good on a dial and
    /// cannot get round is not a setup, and a car that only one notch can drive
    /// is not a car.
    #[test]
    fn a_clumsy_driver_gets_round_on_every_setup() {
        for spec in Spec::ALL {
            for notch in Setup::ALL {
                let handling = notch.applied_to(spec.handling());
                let track = Track::any();
                let budget = 0.5 * track.length() / PACE;
                let lap = lap(&track, Style::Clumsy, handling, 0.5);
                report(&format!("{} on {}", spec.name(), notch.name()), &lap);
                // Low, because Oversteer is meant to be slow for a driver who
                // holds the throttle through a slide — sliding costs speed, and
                // that setup slides. What is guarded is getting round and never
                // stopping.
                assert!(
                    lap.progress > 0.3,
                    "{} on {:?}: {budget:.0} s only got {:.0}% round",
                    spec.name(),
                    notch,
                    lap.progress * 100.0
                );
                assert!(
                    lap.stopped < budget / 3.7,
                    "{} on {:?}: going nowhere {:.0} s of {budget:.0}",
                    spec.name(),
                    notch,
                    lap.stopped
                );
            }
        }
    }

    /// The point of a garage: no car is *the* car to pick.
    ///
    /// Measured rather than asserted by taste — all three lap every circuit with
    /// the plain driver, and two things have to hold. Nobody may be far off the
    /// pace anywhere, or the menu has a wrong answer in it; and the quickest car
    /// has to change from circuit to circuit, or it has a right one. Which is
    /// the whole difference between three cars and one car with two worse
    /// copies of itself.
    ///
    /// The driver is not a racing driver and these are not lap records. That is
    /// fine: it is the same driver in all three, so what is being compared is
    /// the cars.
    #[test]
    fn no_car_is_the_car_to_pick() {
        /// How far off the quickest car of a circuit the slowest may be. The
        /// three sit inside a fortieth of it today; this is the bar, not the
        /// reading, so retuning has room before it has a wrong answer in it.
        const SPREAD: f32 = 0.06;
        let mut winners = Vec::new();
        for circuit in all_circuits() {
            let track = Track::new(circuit);
            let times: Vec<(Spec, f32)> = Spec::ALL
                .into_iter()
                .map(|spec| {
                    let time =
                        lap_time(&track, Style::Plain, spec.handling()).unwrap_or_else(|| {
                            panic!("{} never got a lap in round {}", spec.name(), circuit.name)
                        });
                    (spec, time)
                })
                .collect();
            let (best, quickest) = times
                .iter()
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .copied()
                .expect("three cars");
            let slowest = times.iter().map(|(_, t)| *t).fold(0.0f32, f32::max);
            assert!(
                slowest < quickest * (1.0 + SPREAD),
                "{}: {:.1} s to {:.1} s is {:.0}% between the cars",
                circuit.name,
                quickest,
                slowest,
                100.0 * (slowest / quickest - 1.0)
            );
            winners.push(best);
        }
        winners.sort_by_key(|spec| spec.at());
        winners.dedup();
        assert!(
            winners.len() > 1,
            "{} is quickest everywhere, so there is nothing to choose",
            winners[0].name()
        );
    }

    /// Where a car stops accelerating on the flat, which is not `top_speed`:
    /// that is only where the engine's push fades out, and drag and rolling
    /// resistance are still there when it does.
    fn settles_at(handling: &Handling) -> f32 {
        let mut car = Car::default();
        let dt = 1.0 / 240.0;
        for _ in 0..240 * 120 {
            physics::step(
                &mut car,
                handling,
                Vec3::NEG_Z,
                Vec3::X,
                Controls {
                    throttle: 1.0,
                    ..default()
                },
                FLAT,
                dt,
            );
        }
        car.velocity.length()
    }

    /// The hardest stop this car has in it: from where it settles down to what
    /// it can carry through the tightest corner the game allows, on the brakes
    /// the whole way. What the line of corner markers is a ruler for.
    fn hardest_stop(handling: &Handling) -> f32 {
        let entry = {
            // The speed a corner of MIN_RADIUS holds, which depends on the speed
            // through the downforce, so it is found rather than solved.
            let mut v = 5.0f32;
            for _ in 0..200 {
                v = (handling.grip_at(v) * MIN_RADIUS).sqrt();
            }
            v
        };
        let mut car = Car {
            velocity: Vec3::NEG_Z * settles_at(handling),
            ..default()
        };
        let dt = 1.0 / 240.0;
        let mut metres = 0.0;
        while car.velocity.length() > entry && metres < 200.0 {
            metres += car.velocity.length() * dt;
            physics::step(
                &mut car,
                handling,
                Vec3::NEG_Z,
                Vec3::X,
                Controls {
                    brake: 1.0,
                    ..default()
                },
                FLAT,
                dt,
            );
        }
        metres
    }

    /// What the three cars actually do, circuit by circuit. The table the
    /// garage was balanced against.
    ///
    /// `cargo test --locked --lib the_garage -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn the_garage() {
        println!(
            "{:<22}{:>14}{:>14}   stars",
            "", "settles at", "hardest stop"
        );
        for spec in Spec::ALL {
            let handling = spec.handling();
            let stars = spec.sheet().stars;
            println!(
                "{:<22}{:>10.1} m/s{:>10.1} m   {:?} {:?} {:?}",
                spec.name(),
                settles_at(&handling),
                hardest_stop(&handling),
                stars.handling,
                stars.acceleration,
                stars.top_speed
            );
        }
        println!();
        print!("{:<22}", "circuit");
        for spec in Spec::ALL {
            print!("{:>12}", spec.name());
        }
        println!("{:>10}", "spread");
        for circuit in all_circuits() {
            let track = Track::new(circuit);
            let times: Vec<f32> = Spec::ALL
                .into_iter()
                .map(|spec| lap_time(&track, Style::Plain, spec.handling()).unwrap_or(f32::NAN))
                .collect();
            let quickest = times.iter().copied().fold(f32::MAX, f32::min);
            print!("{:<22}", circuit.name);
            for time in &times {
                let mark = if *time == quickest { "*" } else { " " };
                print!("{:>11.2}{mark}", time);
            }
            let slowest = times.iter().copied().fold(0.0f32, f32::max);
            println!("{:>9.1}%", 100.0 * (slowest / quickest - 1.0));
        }
    }

    /// Sliding costs speed, and the loose setup slides — so a driver who does
    /// not manage the throttle pays for it. A driver who does must not: the
    /// setup slider trades forgiveness for pace, and if the pace were missing
    /// on the loose end it would be a penalty slider. The plain driver lifts
    /// when the tyres are busy, and covers the same ground on every notch.
    #[test]
    fn a_driver_who_lifts_is_not_punished_for_oversteer() {
        let track = Track::any();
        let balanced = lap(
            &track,
            Style::Plain,
            Setup::Balanced.applied_to(Handling::SHOOTING_BRAKE),
            0.5,
        );
        let loose = lap(
            &track,
            Style::Plain,
            Setup::Oversteer.applied_to(Handling::SHOOTING_BRAKE),
            0.5,
        );
        assert!(
            loose.distance > balanced.distance * 0.94,
            "the loose setup cost a competent driver {:.0} m against {:.0}",
            loose.distance,
            balanced.distance
        );
        assert!(
            loose.off_road < 6.0,
            "off the road {:.1} s on the loose setup",
            loose.off_road
        );
    }

    /// Which notch is quickest depends on who is driving — that is the whole
    /// point of a setup slider. Run both drivers over all three.
    ///
    /// `cargo test --lib setup_slider -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn setup_slider() {
        for style in [Style::Clumsy, Style::Plain] {
            for notch in Setup::ALL {
                let handling = notch.applied_to(Handling::SHOOTING_BRAKE);
                let lap = lap(&Track::any(), style, handling, 0.7);
                println!(
                    "{:?} on {:<11} {:6.0} m   off-road {:5.1} s",
                    style,
                    notch.name(),
                    lap.distance,
                    lap.off_road
                );
            }
        }
    }

    /// What kind of lap the physics makes of this circuit, for picking grip and
    /// top speed by something other than taste.
    ///
    /// Walks the centreline and solves the fastest lap the tyres allow, the way
    /// a racing-line solver does: corner speeds from the radii, a forward pass
    /// for what the engine can add, a backward pass for what the brakes must take
    /// away. Then it reports what that lap feels like —
    ///
    /// - **flat out**: how much of it is spent at top speed. Near zero and the
    ///   engine is wasted; near half and there is nothing to brake for.
    /// - **braking**: how much of it is spent slowing down. This is the part a
    ///   driver can be good or bad at.
    /// - **spread**: fastest corner over slowest. Variety.
    /// - **90% loss**: what a driver using nine tenths of the grip gives up over
    ///   a lap. Small and there is nothing left to master; huge and one mistake
    ///   ends the lap.
    ///
    /// `cargo test --lib speed_profile -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn speed_profile() {
        let track = Track::any();
        let step = 0.5f32;
        let mut here = track.start_transform().translation;
        let mut radii = Vec::new();
        for _ in 0..4000 {
            let g = track.ground(here);
            radii.push((1.0 / g.curvature.abs().max(1e-4)).min(1e4));
            here = g.centre + g.tangent * step;
            if radii.len() > 40 && track.progress(here, None) < 0.01 {
                break;
            }
        }
        let n = radii.len();

        println!(
            "{:>5} {:>5} {:>6} {:>7} {:>7} {:>8} {:>7} {:>8}",
            "grip", "top", "lap s", "flatout", "braking", "spread", "slowest", "90%loss"
        );
        for grip in [1.00f32, Handling::SHOOTING_BRAKE.grip / GRAVITY, 1.7] {
            for top in [18.0f32, 21.0, 24.0, 28.0] {
                let engine = 5.0f32;
                let solve = |lat: f32| -> (Vec<f32>, f32) {
                    let mut v: Vec<f32> = radii.iter().map(|r| (lat * r).sqrt().min(top)).collect();
                    for _ in 0..3 {
                        for i in 0..n {
                            let j = (i + 1) % n;
                            v[j] = v[j].min((v[i] * v[i] + 2.0 * engine * step).sqrt());
                        }
                        for i in (0..n).rev() {
                            let j = (i + 1) % n;
                            v[i] = v[i].min((v[j] * v[j] + 2.0 * lat * step).sqrt());
                        }
                    }
                    let time = v.iter().map(|s| step / s).sum();
                    (v, time)
                };
                let (v, time) = solve(grip * GRAVITY);
                let (_, sloppy) = solve(0.9 * grip * GRAVITY);
                let flat_out = v.iter().filter(|s| **s >= top - 0.2).count();
                let braking = (0..n).filter(|&i| v[(i + 1) % n] < v[i] - 0.05).count();
                let mut sorted = v.clone();
                sorted.sort_by(f32::total_cmp);
                println!(
                    "{grip:5.2} {top:5.0} {time:6.1} {:6.0}% {:6.0}% {:7.1}x {:7.1} {:+7.2}s",
                    100.0 * flat_out as f32 / n as f32,
                    100.0 * braking as f32 / n as f32,
                    sorted[n - 1] / sorted[0],
                    sorted[0],
                    sloppy - time,
                );
            }
        }
    }

    /// The run-up does what it is there for. The car is set down short of the
    /// line so a lap can start at speed; this is what says it arrives with some.
    /// Full throttle from the grid, and the speed as the line goes by.
    ///
    /// Spielberg and Monza have a straight behind their lines and reach 20 m/s
    /// of the 22.2 the car actually holds on the flat. Silverstone has the least
    /// of one — its line is on the Hamilton Straight, which is not 45 m long, so
    /// the grid slot sits back in Club — and arrives at 14.2. Still a rolling
    /// start, and still Silverstone.
    #[test]
    fn the_run_up_reaches_the_line_at_speed() {
        for circuit in crate::track::all_circuits() {
            let track = Track::new(circuit);
            let handling = Handling::SHOOTING_BRAKE;
            let mut driver = Driver::new(Style::Plain);
            let mut transform = track.start_transform().with_scale(Vec3::splat(SCALE));
            let mut car = Car::default();
            let dt = 1.0 / 240.0;
            let mut was = track.start_along(transform.translation);
            let mut at_the_line = None;
            for _ in 0..240 * 20 {
                let controls = driver.decide(&track, &handling, &transform, &car);
                advance(&track, &handling, controls, &mut transform, &mut car, dt);
                let along = track.start_along(transform.translation);
                if was <= 0.0 && along > 0.0 {
                    at_the_line = Some(car.velocity.length());
                    break;
                }
                was = along;
            }
            let speed = at_the_line
                .unwrap_or_else(|| panic!("{}: the run-up never reached the line", circuit.name));
            assert!(
                speed > 8.0,
                "{}: the car crosses its own line at {speed:.1} m/s, which is \
                 barely a start at all",
                circuit.name
            );
        }
    }
}
