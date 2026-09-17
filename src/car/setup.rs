//! Where the car sits between safe and loose: three notches on one slider.
//!
//! This model has no front and rear axle to hand grip to one end of, so
//! understeer and oversteer cannot be built the way a simulation builds them.
//! Here they are the *slide* — the angle between where the car points and where
//! it is going — and four dials make it:
//!
//! - **`lock_margin`**, how much more turn full lock asks for than the grip can
//!   give. Under one the car can never slide from steering alone and simply runs
//!   wide; over it, every corner slides a little.
//! - **`align`**, how hard the nose is pulled back into line with travel. High
//!   and a slide dies the moment you stop asking for it; low and it carries on.
//! - **`power_lets_go`**, how much grip the throttle spends at the limit.
//! - **`kick`**, how hard a rear the driver has let go throws the tail round.
//!   This is the one that makes oversteer *oversteer*: the car turning more
//!   than the wheel asked, rather than running wide with its nose tucked in.
//! - **`yaw_response`**, how eagerly the car rotates toward where the wheels
//!   point.
//!
//! The slider moves all five together, and only so far: every notch still has to
//! turn at full lock and still has to refuse to spin from steering alone, which
//! is what the tests here hold it to. A setup should change how the car feels,
//! not whether it is drivable.

use bevy::prelude::*;

use super::physics::Handling;

/// The chosen notch. Lives as a resource because it is a preference rather than
/// race state: a restart does not move it.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Setup {
    /// Runs wide rather than coming round. Slowest to rotate, hardest to spin,
    /// and the one to learn a corner on.
    Understeer,
    /// The car as it ships.
    #[default]
    Balanced,
    /// Rotates eagerly and holds a slide once it has one. Quickest through a
    /// corner that is got right, and the one that bites.
    Oversteer,
}

impl Setup {
    /// Left to right along the slider.
    pub const ALL: [Setup; 3] = [Setup::Understeer, Setup::Balanced, Setup::Oversteer];

    /// Which notch, from the left.
    pub fn notch(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(1)
    }

    /// Move along the slider, stopping at the ends.
    pub fn slid(self, by: i32) -> Self {
        let at = (self.notch() as i32 + by).clamp(0, Self::ALL.len() as i32 - 1);
        Self::ALL[at as usize]
    }

    pub fn name(self) -> &'static str {
        match self {
            Setup::Understeer => "UNDERSTEER",
            Setup::Balanced => "BALANCED",
            Setup::Oversteer => "OVERSTEER",
        }
    }

    /// `base` leaned this way. Only the five dials that make a slide move; the
    /// `..base` says so, and everything else — grip, brakes, engine — is the
    /// same car on every notch.
    pub fn applied_to(self, base: Handling) -> Handling {
        match self {
            Setup::Understeer => Handling {
                lock_margin: 0.98,
                align: 4.2,
                power_lets_go: 0.32,
                kick: 0.8,
                yaw_response: 9.8,
                ..base
            },
            Setup::Balanced => base,
            Setup::Oversteer => Handling {
                lock_margin: 1.26,
                align: 2.4,
                power_lets_go: 0.60,
                kick: 3.0,
                yaw_response: 12.4,
                ..base
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::car::physics::{Car, Controls, GRAVITY, Surface, step};

    const FLAT: Surface = Surface {
        grip: 1.0,
        slope: 0.0,
    };

    fn tuned(setup: Setup) -> Handling {
        setup.applied_to(Handling::SHOOTING_BRAKE)
    }

    /// Hold full lock at `speed` for two and a half seconds. Returns the worst
    /// slide angle seen while still moving, and the peak lateral g.
    fn corner(setup: Setup, speed: f32, throttle: f32) -> (f32, f32) {
        let h = tuned(setup);
        let mut car = Car {
            velocity: Vec3::NEG_Z * speed,
            ..default()
        };
        let mut yaw = 0.0f32;
        let dt = 1.0 / 240.0;
        let (mut worst, mut peak) = (0.0f32, 0.0f32);
        for i in 0..(2.5 / dt) as usize {
            let heading = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
            yaw += step(
                &mut car,
                &h,
                heading,
                heading.cross(Vec3::Y),
                Controls {
                    steer: 1.0,
                    throttle,
                    ..default()
                },
                FLAT,
                dt,
            );
            if car.velocity.length() > 5.0 {
                let heading = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
                worst = worst.max(car.velocity.normalize_or(heading).angle_between(heading));
            }
            if i > 30 {
                peak = peak.max(car.g_force.x.abs());
            }
        }
        (worst, peak)
    }

    /// Drive `first` for `hold` seconds and then `then` for `release` seconds.
    /// Returns the worst slide while on `first`, and the slide left at the end.
    fn then_lift(
        setup: Setup,
        first: Controls,
        hold: f32,
        then: Controls,
        release: f32,
    ) -> (f32, f32) {
        let h = tuned(setup);
        let mut car = Car {
            velocity: Vec3::NEG_Z * 14.0,
            ..default()
        };
        let mut yaw = 0.0f32;
        let dt = 1.0 / 240.0;
        let slide = |car: &Car, yaw: f32| {
            let heading = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
            car.velocity.normalize_or(heading).angle_between(heading)
        };
        let mut worst = 0.0f32;
        for (controls, seconds, measure) in [(first, hold, true), (then, release, false)] {
            for _ in 0..(seconds / dt) as usize {
                let heading = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
                yaw += step(
                    &mut car,
                    &h,
                    heading,
                    heading.cross(Vec3::Y),
                    controls,
                    FLAT,
                    dt,
                );
                if measure && car.velocity.length() > 3.0 {
                    worst = worst.max(slide(&car, yaw));
                }
            }
        }
        (worst, slide(&car, yaw))
    }

    /// Oversteer has to *be* oversteer: full lock and full throttle bring the
    /// tail round, so the car turns more than the wheel asked. It must not go
    /// all the way round — a spin is not a drift — and lifting off must catch
    /// it. Understeer, given the same, stays planted. Without a rear that can
    /// feed the rotation, a loose rear only ever ran the car wide with its nose
    /// tucked in, which is a slide that reads as understeer.
    #[test]
    fn full_throttle_on_oversteer_brings_the_tail_round_and_lifting_catches_it() {
        let boot = Controls {
            steer: 1.0,
            throttle: 1.0,
            ..default()
        };
        let (loose, caught) = then_lift(Setup::Oversteer, boot, 1.0, Controls::default(), 1.5);
        assert!(
            loose > 0.6,
            "oversteer only came round {:.0} degrees on full throttle",
            loose.to_degrees()
        );
        assert!(
            loose < 1.4,
            "oversteer spun: {:.0} degrees",
            loose.to_degrees()
        );
        assert!(
            caught < 0.1,
            "lifting off left it {:.0} degrees sideways",
            caught.to_degrees()
        );

        let (planted, _) = then_lift(Setup::Understeer, boot, 1.0, Controls::default(), 0.5);
        assert!(
            planted < 0.2,
            "understeer stepped out {:.0} degrees on full throttle",
            planted.to_degrees()
        );
    }

    /// The slider has to be a slider: balanced in the middle of both leans, on
    /// every dial it moves. Retune the car and this is what catches the notches
    /// crossing over.
    #[test]
    fn the_notches_lean_in_order() {
        let [u, b, o] = Setup::ALL.map(tuned);
        assert!(u.lock_margin < b.lock_margin && b.lock_margin < o.lock_margin);
        // Higher align pulls the nose back harder, so it runs the other way.
        assert!(u.align > b.align && b.align > o.align);
        assert!(u.power_lets_go < b.power_lets_go && b.power_lets_go < o.power_lets_go);
        assert!(u.kick < b.kick && b.kick < o.kick);
        assert!(u.yaw_response < b.yaw_response && b.yaw_response < o.yaw_response);
        assert_eq!(Setup::ALL.map(Setup::notch), [0, 1, 2]);
    }

    /// The slider ends where it says it does, and does not wrap round.
    #[test]
    fn it_stops_at_both_ends() {
        assert_eq!(Setup::Understeer.slid(-1), Setup::Understeer);
        assert_eq!(Setup::Oversteer.slid(1), Setup::Oversteer);
        assert_eq!(Setup::Balanced.slid(-1), Setup::Understeer);
        assert_eq!(Setup::Balanced.slid(1), Setup::Oversteer);
    }

    /// The guarantee the whole handling model is built on, held for every notch:
    /// a key held down turns the car at any speed, gets near the grip it has,
    /// and never spins it. A setup may change how a car feels; it may not make
    /// it undrivable.
    #[test]
    fn every_setup_turns_and_none_spins() {
        for setup in Setup::ALL {
            for speed in [8.0f32, 14.0, 20.0] {
                let (worst, peak) = corner(setup, speed, 0.3);
                assert!(
                    peak > 0.8 * Handling::SHOOTING_BRAKE.grip / GRAVITY,
                    "{:?} at {speed} m/s only pulled {peak:.2} g",
                    setup
                );
                assert!(
                    worst < 0.6,
                    "{:?} at {speed} m/s swung the car {:.0} degrees off line",
                    setup,
                    worst.to_degrees()
                );
            }
        }
    }

    /// And the point of the slider: the lean has to be felt. The loose setup
    /// slides further on the same corner than the safe one, on a trailing
    /// throttle and more so on a full one.
    #[test]
    fn the_lean_is_worth_choosing() {
        for throttle in [0.3f32, 1.0] {
            let (safe, _) = corner(Setup::Understeer, 14.0, throttle);
            let (middle, _) = corner(Setup::Balanced, 14.0, throttle);
            let (loose, _) = corner(Setup::Oversteer, 14.0, throttle);
            assert!(
                loose > middle && middle > safe,
                "on {throttle} throttle the notches slid {:.1}, {:.1}, {:.1} degrees",
                safe.to_degrees(),
                middle.to_degrees(),
                loose.to_degrees()
            );
            assert!(
                loose - safe > 0.04,
                "the slider barely moved anything: {:.1} against {:.1} degrees",
                safe.to_degrees(),
                loose.to_degrees()
            );
        }
    }
}
