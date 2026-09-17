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
use super::physics::{Car, Controls, Handling, GRAVITY};
use crate::track::Track;

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

    /// Frames of delay at 120 Hz, probes up the road three metres apart, and
    /// how far past the limit before the brakes go on.
    fn habits(&self) -> (usize, usize, f32) {
        match self.style {
            Style::Plain => (0, 16, 1.04),
            Style::Clumsy => (18, 6, 1.15),
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
        let (reaction, lookahead, late) = self.habits();
        let heading = level(*transform.forward());
        let ground = track.ground(transform.translation);
        // The way the lap runs, not the way the car happens to be pointing:
        // aligning the target to the car lets a driver lap backwards.
        let ahead = ground.tangent;
        // Signed, so facing the wrong way reads as half a turn of error rather
        // than as no error at all.
        let astray = f32::atan2(heading.cross(ahead).y, heading.dot(ahead));
        let correction = astray * 1.6 + ground.lateral * 0.16;

        // Hold a speed the tyres can corner at, looking at the tightest bend
        // between here and as far up the road as this driver looks. Conservative
        // about grip — no credit for downforce — and it knows a descent has the
        // hill working against the brakes.
        let hold = 0.7 * handling.grip * ground.grip;
        let downhill = (-ground.slope * ground.tangent.dot(heading) * GRAVITY).max(0.0);
        let stopping = (0.75 * handling.brake * ground.grip - downhill).max(hold * 0.4);
        let mut limit = handling.top_speed;
        for step in 0..=lookahead {
            let reach = step as f32 * 3.0;
            let probe = track.ground(transform.translation + ahead * reach);
            let corner = hold / probe.curvature.abs().max(0.002);
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
                    steer: if correction > 0.03 {
                        1.0
                    } else if correction < -0.03 {
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
    use crate::car::{advance, Setup, SCALE};

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

    fn lap(style: Style, handling: Handling, seconds: f32) -> Lap {
        let track = Track::new();
        let mut driver = Driver::new(style);
        let mut transform = track.start_transform().with_scale(Vec3::splat(SCALE));
        let mut car = Car::default();
        let dt = 1.0 / 120.0;
        let mut lap = Lap::default();

        for _ in 0..(seconds / dt) as usize {
            let controls = driver.decide(&track, &handling, &transform, &car);
            let speed = car.velocity.length();
            advance(&track, &handling, controls, &mut transform, &mut car, dt);
            track.hold(&mut transform, &mut car, dt);

            lap.distance += speed * dt;
            let ground = track.ground(transform.translation);
            let heading = level(*transform.forward());
            if ground.lateral.abs() > 4.0 {
                lap.off_road += dt;
            }
            if ground.lateral.abs() > 5.5 {
                lap.in_the_weeds += dt;
            }
            if ground.slope * ground.tangent.dot(heading) < -0.04 {
                lap.descending += dt;
                if ground.lateral.abs() > 4.0 {
                    lap.off_road_descending += dt;
                }
            }
            if speed < 1.5 {
                lap.stopped += dt;
            }
            lap.progress = lap.progress.max(track.progress(transform.translation));
        }
        lap
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

    #[test]
    fn a_plain_driver_gets_round() {
        let lap = lap(Style::Plain, Handling::SHOOTING_BRAKE, 90.0);
        report("plain", &lap);
        assert!(lap.progress > 0.9, "90 s only got {:.0}% round", lap.progress * 100.0);
        // Loose on purpose: these are the bounds of "can get round", not a target.
        assert!(lap.off_road < 30.0, "off the road {:.0} s of 90", lap.off_road);
        assert!(lap.stopped < 15.0, "going nowhere {:.0} s of 90", lap.stopped);
    }

    /// Full lock or nothing, 150 ms behind, brakes late, cannot see far. This is
    /// the person holding the keys, and the car has to be drivable by them.
    #[test]
    fn a_clumsy_driver_still_gets_round() {
        let lap = lap(Style::Clumsy, Handling::SHOOTING_BRAKE, 90.0);
        report("clumsy", &lap);
        assert!(lap.progress > 0.9, "90 s only got {:.0}% round", lap.progress * 100.0);
        // Loose: the grass is a gravel trap, so every excursion this driver
        // makes is a slow one, and it makes plenty. What matters is that it is
        // never stuck out there.
        assert!(lap.off_road < 45.0, "off the road {:.0} s of 90", lap.off_road);
        assert!(lap.stopped < 20.0, "going nowhere {:.0} s of 90", lap.stopped);
    }

    /// Every notch of the setup slider has to be lappable by the person holding
    /// the keys, not just the one the car ships on. A setup that looks good on a
    /// dial and cannot get round is not a setup.
    #[test]
    fn a_clumsy_driver_gets_round_on_every_setup() {
        for notch in Setup::ALL {
            let handling = notch.applied_to(Handling::SHOOTING_BRAKE);
            let lap = lap(Style::Clumsy, handling, 45.0);
            report(notch.name(), &lap);
            assert!(
                lap.progress > 0.35,
                "{:?}: 45 s only got {:.0}% round",
                notch,
                lap.progress * 100.0
            );
            assert!(lap.stopped < 12.0, "{:?}: going nowhere {:.0} s of 45", notch, lap.stopped);
        }
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
                let lap = lap(style, handling, 60.0);
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
        let track = Track::new();
        let step = 0.5f32;
        let mut here = track.start_transform().translation;
        let mut radii = Vec::new();
        for _ in 0..4000 {
            let g = track.ground(here);
            radii.push((1.0 / g.curvature.abs().max(1e-4)).min(1e4));
            here = g.centre + g.tangent * step;
            if radii.len() > 40 && track.progress(here) < 0.01 {
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
}
