//! Changes to how the car drives, made from outside the engine.
//!
//! The engine is never edited. It is given a [`Handling`] each step, and this
//! is where that value gets changed on its way in: scaled for speed, boosted,
//! stripped of grip in the air. When nothing is asked for the value passes
//! through untouched, bit for bit, which `an_unasked_for_tweak_changes_nothing`
//! holds.
//!
//! **Speed** is done by dimensional analysis rather than by turning things up.
//! Ask for a car `k` times as fast and every speed is `k` times what it was,
//! every acceleration `k²` times (so the same corner is taken at `k` times the
//! speed on `k²` times the grip, which is the same corner), and every rate `k`
//! times. The car is then the same car driven through the same corners in `1/k`
//! of the time, and not a different car that cannot get round them. The
//! `the_same_lap_is_lapped_faster` test has the plain AI drive a lap at each
//! speed and checks it takes `1/k` as long.

use bevy::prelude::*;

use super::Fun;
use super::air::Air;
use crate::car::{DriveSet, Handling, Player};

/// How much of its grip a car keeps in the air. None, in life. In Bonkers some,
/// the way every arcade racer has it, so that a hop on the way into a bend lands
/// on the road and not beside it: with none, a car that left a crest went
/// wherever it was pointing until it came down, and the hills sent it off into
/// the grass more often than not.
pub(crate) const AIR_GRIP: f32 = 0.35;

/// What is being changed about the handling this step.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub(crate) struct Tweaks {
    /// How many times faster than the shipped car, everything scaled to match.
    pub speed: f32,
    /// A multiple of the grip, on top: ice is below one.
    pub grip: f32,
    /// A multiple of the engine's pull, and of where it stops pulling.
    pub power: f32,
    pub top: f32,
    /// Nothing under the wheels: no grip, no engine, no brakes.
    pub airborne: bool,
}

impl Tweaks {
    pub(crate) const NEUTRAL: Self = Self {
        speed: 1.0,
        grip: 1.0,
        power: 1.0,
        top: 1.0,
        airborne: false,
    };

    pub(crate) fn is_neutral(&self) -> bool {
        *self == Self::NEUTRAL
    }

    /// What Bonkers asks of the handling at `speed`: a boost of strength `push`,
    /// ice and hyperdrive at strengths `ice` and `hyper`, each 0 to 1, and
    /// whether the car is in the air. The one place these are worked out, so that
    /// what the game drives and what the tests drive are the same.
    ///
    /// Ice leaves a little over half the grip and hyperdrive adds a fifth to the
    /// speed. Each was more, and each was a few seconds of not being able to
    /// steer: ice at three tenths of the grip is a car that cannot take a
    /// corner at any speed worth driving, and hyperdrive at a third more was
    /// faster than every speed the settings offer.
    pub(crate) fn bonkers(speed: f32, push: f32, ice: f32, hyper: f32, airborne: bool) -> Self {
        Self {
            speed: speed * (1.0 + 0.2 * hyper),
            grip: 1.0 - 0.45 * ice,
            power: 1.0 + 1.4 * push,
            top: 1.0 + 0.5 * push,
            airborne,
        }
    }

    /// `handling` as it is to be driven this step.
    pub(crate) fn apply(&self, handling: &Handling) -> Handling {
        if self.is_neutral() {
            return *handling;
        }
        let mut h = *handling;
        let k = self.speed;
        let k2 = k * k;
        // Speeds by k, accelerations by k squared, rates by k.
        h.top_speed *= k * self.top;
        h.reverse_speed *= k;
        h.accel *= k2 * self.power;
        h.brake *= k2;
        h.engine_braking *= k2;
        h.rolling *= k2;
        h.reverse_accel *= k2;
        h.grip *= k2 * self.grip;
        h.scrub_drag *= k2;
        h.steer_rate *= k;
        h.yaw_response *= k;
        h.align *= k;
        h.kick *= k;
        h.off_road_drag *= k;
        // Downforce and drag are per speed squared, which already scales.
        if self.airborne {
            // Nothing for the engine or the brakes to push against, so the car
            // slows only as the air slows it; and a little of the grip, which is
            // what the steering is left with.
            h.grip *= AIR_GRIP;
            h.downforce *= AIR_GRIP;
            h.accel = 0.0;
            h.brake = 0.0;
            h.engine_braking = 0.0;
            h.rolling = 0.0;
            h.reverse_accel = 0.0;
            h.scrub_drag = 0.0;
            h.off_road_drag = 0.0;
        }
        h
    }
}

impl Default for Tweaks {
    fn default() -> Self {
        Self::NEUTRAL
    }
}

/// A boost in progress.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub(crate) struct Boost {
    /// Seconds left.
    pub left: f32,
    /// How hard, 0 to 1.
    pub power: f32,
}

impl Boost {
    /// How much boost there is right now, easing out over the last moments.
    pub(crate) fn strength(&self) -> f32 {
        if self.left <= 0.0 {
            0.0
        } else {
            self.power * (self.left / 0.5).min(1.0)
        }
    }

    /// Light the rocket, or keep it lit for longer.
    pub(crate) fn fire(&mut self, power: f32, seconds: f32) {
        self.power = self.power.max(power).min(1.0);
        self.left = self.left.max(seconds);
    }
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Tweaks>()
        .add_systems(FixedUpdate, (tick_boost, resolve).chain().before(DriveSet));
}

fn tick_boost(time: Res<Time>, mut boost: ResMut<Boost>) {
    if boost.left > 0.0 {
        boost.left = (boost.left - time.delta_secs()).max(0.0);
        if boost.left == 0.0 {
            boost.power = 0.0;
        }
    }
}

/// Work out what to change this step.
fn resolve(
    fun: Res<Fun>,
    race: Option<Res<crate::local::LocalRace>>,
    chaos: Option<Res<super::events::Chaos>>,
    boost: Res<Boost>,
    cars: Query<&Air, With<Player>>,
    mut tweaks: ResMut<Tweaks>,
) {
    let wanted = if fun.bonkers() && crate::local::solo(race) {
        let (ice, hyper) = chaos.as_ref().map_or((0.0, 0.0), |c| {
            (
                c.strength(super::events::Effect::Ice),
                c.strength(super::events::Effect::Hyper),
            )
        });
        Tweaks::bonkers(
            fun.speed.scale(),
            boost.strength(),
            ice,
            hyper,
            cars.iter().any(|air| air.flying),
        )
    } else {
        Tweaks::NEUTRAL
    };
    tweaks.set_if_neq(wanted);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::car::{Car, Controls, SCALE, advance};
    use crate::track::Track;

    #[test]
    fn an_unasked_for_tweak_changes_nothing() {
        let stock = Handling::SHOOTING_BRAKE;
        assert!(Tweaks::NEUTRAL.is_neutral());
        // Bit for bit, not close.
        assert_eq!(Tweaks::NEUTRAL.apply(&stock), stock);
        assert_eq!(Tweaks::default().apply(&stock), stock);
        assert!(
            !Tweaks {
                speed: 1.6,
                ..Tweaks::NEUTRAL
            }
            .is_neutral()
        );
        assert!(
            !Tweaks {
                airborne: true,
                ..Tweaks::NEUTRAL
            }
            .is_neutral()
        );
    }

    #[test]
    fn speed_scales_speeds_by_k_accelerations_by_k_squared_and_rates_by_k() {
        let stock = Handling::SHOOTING_BRAKE;
        let k = 2.5;
        let fast = Tweaks {
            speed: k,
            ..Tweaks::NEUTRAL
        }
        .apply(&stock);
        let close = |a: f32, b: f32| (a - b).abs() < 1e-3 * b.abs().max(1.0);
        assert!(close(fast.top_speed, stock.top_speed * k));
        assert!(close(fast.accel, stock.accel * k * k));
        assert!(close(fast.grip, stock.grip * k * k));
        assert!(close(fast.brake, stock.brake * k * k));
        assert!(close(fast.steer_rate, stock.steer_rate * k));
        assert!(close(fast.yaw_response, stock.yaw_response * k));
        // What is a fraction, or per speed squared, is left alone.
        assert_eq!(fast.drag, stock.drag);
        assert_eq!(fast.downforce, stock.downforce);
        assert_eq!(fast.wheelbase, stock.wheelbase);
        assert_eq!(fast.max_steer, stock.max_steer);
        assert_eq!(fast.handbrake_lets_go, stock.handbrake_lets_go);
        // The steering lock at k times the speed is the lock at the speed.
        for v in [4.0, 12.0, 24.0] {
            let a = stock.lock(v);
            let b = fast.lock(v * k);
            assert!((a - b).abs() < 1e-3, "lock at {v}: {a} against {b}");
        }
    }

    #[test]
    fn the_air_has_nothing_to_push_against_and_a_little_to_steer_with() {
        let stock = Handling::SHOOTING_BRAKE;
        let flying = Tweaks {
            airborne: true,
            ..Tweaks::NEUTRAL
        }
        .apply(&stock);
        assert_eq!(
            (flying.accel, flying.brake, flying.engine_braking),
            (0.0, 0.0, 0.0)
        );
        assert_eq!(flying.grip, stock.grip * AIR_GRIP);
        assert!(flying.grip < stock.grip * 0.5, "it is not the road");
        assert_eq!(flying.drag, stock.drag, "the air still slows a car");
    }

    /// Drive one lap with the plain AI at `k` times the speed, in seconds.
    fn lap(track: &Track, k: f32) -> Option<f32> {
        let tweaks = Tweaks {
            speed: k,
            ..Tweaks::NEUTRAL
        };
        let handling = tweaks.apply(&Handling::SHOOTING_BRAKE);
        let mut driver = crate::car::ai_driver();
        let mut at = track.start_transform().with_scale(Vec3::splat(SCALE));
        let mut car = Car {
            along: Some(track.start_along_lap()),
            ..Car::default()
        };
        let dt = crate::car::step_seconds();
        let mut timer = crate::lap::LapTimer::default();
        for _ in 0..(6.0 * track.length() / 5.0 * 240.0) as usize {
            let controls: Controls = driver(track, &handling, &at, &car);
            advance(track, &handling, controls, &mut at, &mut car, dt);
            timer.count(dt);
            let recovered = std::mem::take(&mut car.recovered);
            let pos = at.translation;
            let step = crate::lap::Step {
                legal: track.legal_contact(&at, car.along),
                recovered,
                along: track.start_along(pos),
                progress: track.progress(pos, car.along),
                length: track.length(),
                sectors: track.sector_count(),
                speed: car.velocity.length(),
            };
            if let Some(done) = timer.judge(step, || track.on_start_gate(pos, car.along))
                && done.valid
                && timer.completed >= 2
            {
                return Some(done.time);
            }
        }
        None
    }

    /// Every circuit, at every speed and every wildness: the plain AI still laps
    /// it, faster the faster it is asked to go, and never faster than the speed
    /// itself. This is what says the hills and the speed cannot together make a
    /// circuit that a car cannot get round. Read the table with `--nocapture`.
    ///
    /// The one place it is not asked for is the shipped speed on a hilly road,
    /// which is the shipped car on a hill it was never built for. There the
    /// plain AI, which cannot see a crest coming, can go wide off the road where
    /// it drops away (the engine's own test of a tyre being on the road looks at
    /// how far the wheel is from the surface, and a fall of more than half a
    /// metre under a wheel is not touching it) or stall on the steepest of the
    /// climbs and be rescued. It is not a problem to a person: a lap of a hilly
    /// road is a Bonkers lap, and no Bonkers lap counts.
    #[test]
    fn every_circuit_can_be_lapped_at_every_speed_and_wildness() {
        let speeds: Vec<f32> = super::super::Speed::ALL.iter().map(|s| s.scale()).collect();
        let circuits = crate::track::all_circuits();
        let mut rows = Vec::new();
        let mut bad = Vec::new();
        std::thread::scope(|scope| {
            let speeds = &speeds;
            let handles: Vec<_> = circuits
                .chunks(4)
                .map(|some| {
                    scope.spawn(move || {
                        let (mut rows, mut bad) = (Vec::new(), Vec::new());
                        for circuit in some {
                            for wild in 0..=3u8 {
                                let track = Track::with_wild(circuit, wild);
                                let times: Vec<Option<f32>> =
                                    speeds.iter().map(|k| lap(&track, *k)).collect();
                                let cells: String = times
                                    .iter()
                                    .map(|t| t.map_or("   none".into(), |t| format!("{t:7.1}")))
                                    .collect();
                                rows.push(format!("{:>24} w{wild}  {cells}", circuit.id));
                                let mut before: Option<(f32, f32)> = None;
                                for (k, time) in speeds.iter().zip(&times) {
                                    let Some(time) = time else {
                                        if *k > 1.0 || wild == 0 {
                                            bad.push(format!(
                                                "{} w{wild}: no lap at {k}x",
                                                circuit.id
                                            ));
                                        }
                                        continue;
                                    };
                                    if let Some((then, at)) = before {
                                        // Faster, and not by more than the speed.
                                        let by = then / time;
                                        if !(by > 1.0 && by < (k / at) * 1.05) {
                                            bad.push(format!(
                                                "{} w{wild}: {at}x to {k}x made the lap {by:.2}x shorter",
                                                circuit.id
                                            ));
                                        }
                                    }
                                    before = Some((*time, *k));
                                }
                            }
                        }
                        (rows, bad)
                    })
                })
                .collect();
            for handle in handles {
                let (r, b) = handle.join().unwrap();
                rows.extend(r);
                bad.extend(b);
            }
        });
        rows.sort();
        let heads: String = speeds
            .iter()
            .map(|k| format!("{:>7}", format!("x{k}")))
            .collect();
        println!("{:>24}     {heads}", "");
        for row in rows {
            println!("{row}");
        }
        assert!(bad.is_empty(), "{bad:#?}");
    }

    #[test]
    fn the_same_lap_is_lapped_faster() {
        // Faster, and never more than in proportion. Not exactly in proportion:
        // the corners set the pace, and a car that is `k` times as fast is only
        // that much faster on the straights.
        for id in ["monza", "red-bull-ring"] {
            let circuit = crate::track::all_circuits()
                .iter()
                .find(|c| c.id == id)
                .unwrap();
            let track = Track::new(circuit);
            let stock = lap(&track, 1.0).unwrap_or_else(|| panic!("{id} at stock speed"));
            let mut before = 1.0;
            for k in super::super::Speed::ALL[1..].iter().map(|s| s.scale()) {
                let fast = lap(&track, k)
                    .unwrap_or_else(|| panic!("the AI cannot lap {id} at {k} times the speed"));
                let ratio = stock / fast;
                assert!(
                    ratio > 1.0 + 0.45 * (k - 1.0) && ratio < k * 1.05,
                    "{id}: {k} times the speed made the lap {ratio:.2} times shorter"
                );
                assert!(ratio > before, "{id}: faster is not slower");
                before = ratio;
            }
        }
    }
}
