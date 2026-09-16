//! A bicycle model: two tyres, a mass, and a friction budget they have to share.
//!
//! Everything the car does comes out of one step. Turning is what the front tyre
//! can pull sideways, acceleration is what the rear tyre can push, and when
//! either is asked for more than its contact patch can give, it slides — which
//! is the same arithmetic, not a special case.
//!
//! Longitudinal and lateral forces come out of one budget per axle, so a rear
//! tyre spending its grip on throttle has less left for cornering. That is where
//! the drift comes from.

use bevy::prelude::*;

/// The glTF is modelled at full size; this is the fraction of it we drive.
pub(crate) const SCALE: f32 = 0.8;
/// From `tools/make_shooting_brake.py`, at driving scale.
pub(crate) const WHEEL_RADIUS: f32 = 0.20 * SCALE;
pub(crate) const WHEEL_WIDTH: f32 = 0.16 * SCALE;
pub(crate) const HALF_TRACK: f32 = 0.50 * SCALE;
/// Axle distances from the model origin, which sits between them.
pub(crate) const FRONT_AXLE: f32 = 0.70 * SCALE;
pub(crate) const REAR_AXLE: f32 = 0.76 * SCALE;
const WHEELBASE: f32 = FRONT_AXLE + REAR_AXLE;

const GRAVITY: f32 = 9.81;
/// Kerb weight. Only ratios of force to mass matter, but carrying a real number
/// keeps the engine and tyre figures readable.
const MASS: f32 = 1180.0;
/// Yaw inertia of a slab the size of the car.
const YAW_INERTIA: f32 = MASS * (1.92 * 1.92 + 0.88 * 0.88) / 12.0;
/// Centre of gravity height, which is what turns acceleration into weight
/// transfer: squat under power, dive under brakes.
const CG_HEIGHT: f32 = 0.42 * SCALE;

/// Peak grip as a multiple of the load on the tyre.
const FRICTION: f32 = 1.15;
/// The rear tyres are given a little more than the front, which is what makes a
/// road car run wide rather than swap ends when it is pushed too hard. Throttle
/// and the handbrake still spend that margin, so a drift is something you ask
/// for rather than something that happens to you.
const REAR_GRIP_BIAS: f32 = 1.12;
/// Shape of the tyre curve: grip climbs with slip angle, peaks, then falls away.
/// `STIFFNESS` sets how fast it climbs, `FALLOFF` how sharply it lets go.
const STIFFNESS: f32 = 9.0;
const FALLOFF: f32 = 1.5;
/// Slip angle where that curve peaks. Past it the tyre is sliding, not gripping.
const PEAK_SLIP: f32 = 0.192;

/// Drive force at a standstill, tapering to nothing at top speed.
const ENGINE: f32 = 7_600.0;
const TOP_SPEED: f32 = 24.0;
const REVERSE_SPEED: f32 = 9.0;
/// Reverse is a fraction of the forward pull, and only up to a crawl.
const REVERSE_EFFORT: f32 = 0.42;
/// Rolling slower than this counts as stopped, which is when the brake key
/// becomes reverse.
const STOPPED: f32 = 0.5;
/// The brakes can lock any wheel, so what stops the car is the tyre, not the
/// pedal: each axle is asked for this much of the grip it has, whatever the load
/// on it and whatever it is standing on. Asking for a shade under everything
/// leaves a sliver to steer with, which is the difference between braking hard
/// and braking well, and it is also why a straight-line stop leaves no marks.
const BRAKE_EFFORT: f32 = 0.95;
/// The handbrake locks the rear axle rather than merely slowing it, so it has to
/// ask for more than the rear tyres can ever give. The tyre still only delivers
/// what it has; the difference is what marks the road.
const HANDBRAKE: f32 = 14_000.0;

const DRAG: f32 = 2.6;
const ROLLING_RESISTANCE: f32 = 260.0;

/// Steering lock at a standstill. Above walking pace the lock is cut to what the
/// front tyres can actually hold — see [`lock`].
const MAX_STEER: f32 = 0.56;
/// How far past that limit the driver is allowed to ask. A little over is what
/// makes it possible to provoke a slide on purpose.
const LOCK_MARGIN: f32 = 1.2;
/// How fast the wheels follow the key, so steering has weight.
const STEER_RATE: f32 = 7.0;

/// Below this the tyre model has no meaningful slip angle to work from, and a
/// tyre has no sideways bite either: a stopped wheel turned to full lock pushes
/// nothing, however large its slip angle works out to be.
const CRAWL: f32 = 1.2;

/// What the driver is asking for this frame.
#[derive(Clone, Copy, Default)]
pub(crate) struct Controls {
    /// 0 to 1.
    pub(crate) throttle: f32,
    /// 0 to 1. The brake while the car is rolling, reverse once it has stopped —
    /// two pedals on two keys, the way every arcade racer does it.
    pub(crate) brake: f32,
    /// Positive steers left, matching Bevy's left-handed yaw about +Y.
    pub(crate) steer: f32,
    pub(crate) handbrake: bool,
}

/// What the road is offering at the contact patches.
#[derive(Clone, Copy)]
pub(crate) struct Surface {
    /// Fraction of tarmac grip.
    pub(crate) grip: f32,
    /// Rise over run along `heading`; positive is a climb.
    pub(crate) slope: f32,
}

/// The car's motion, in world space except where noted.
#[derive(Component, Default)]
pub(crate) struct Car {
    /// Metres per second across the ground. Y is unused: the loft carries height.
    pub(crate) velocity: Vec3,
    /// Radians per second about +Y.
    pub(crate) yaw_rate: f32,
    /// Where the front wheels actually point, which lags the key.
    pub(crate) steer_angle: f32,
    /// Acceleration in g, in the car's own frame: x to the right, y forward.
    /// Straight to the meter.
    pub(crate) g_force: Vec2,
    /// How far past its grip the rear axle is, 0 while it holds and climbing
    /// once it lets go. What decides whether a tyre leaves a mark.
    pub(crate) rear_slip: f32,
    /// Seconds spent off the circuit getting nowhere. The track uses it to
    /// decide when to fetch the car back.
    pub(crate) stranded: f32,
}

impl Car {
    /// Speed along the car's own nose. Negative in reverse.
    pub(crate) fn speed(&self, heading: Vec3) -> f32 {
        self.velocity.dot(heading)
    }
}

/// Advance the car by `dt` and report the yaw to apply, in radians.
///
/// `heading` and `right` are the car's own axes, level in XZ.
pub(crate) fn step(
    car: &mut Car,
    heading: Vec3,
    right: Vec3,
    controls: Controls,
    surface: Surface,
    dt: f32,
) -> f32 {
    car.steer_angle = car.steer_angle.lerp(
        controls.steer * lock(car.velocity.length()),
        (STEER_RATE * dt).min(1.0),
    );
    let steer = car.steer_angle;

    // Body frame: along the nose, and out of the driver's right window. Velocity
    // itself stays in world space, so the basis rotating with the car is what
    // produces the centripetal turn — there is nothing extra to add for it.
    let along = car.velocity.dot(heading);
    let across = car.velocity.dot(right);
    let pace = along.abs().max(CRAWL);

    // Static load, shifted by whatever the car did last frame. Squat puts the
    // rear tyre down harder, which is why power-on drifts hook up again.
    let weight = MASS * GRAVITY;
    let transfer = MASS * car.g_force.y * GRAVITY * CG_HEIGHT / WHEELBASE;
    let front_load = (weight * REAR_AXLE / WHEELBASE - transfer).max(weight * 0.12);
    let rear_load = (weight * FRONT_AXLE / WHEELBASE + transfer).max(weight * 0.12);

    // Slip angle: where a tyre is travelling, less where it points, both measured
    // toward `right`. Yaw carries the front axle left and the rear axle right, so
    // the two contact patches see different sideways speeds.
    let front_angle = ((across - FRONT_AXLE * car.yaw_rate) / pace).atan() + steer * along.signum();
    let rear_angle = ((across + REAR_AXLE * car.yaw_rate) / pace).atan();

    let front_budget = FRICTION * surface.grip * front_load;
    let rear_budget = FRICTION * REAR_GRIP_BIAS * surface.grip * rear_load;

    // Longitudinal demand. Rear wheel drive: the engine only ever asks the rear.
    let rolling = along.abs() > STOPPED;
    let drive = if controls.throttle > 0.0 {
        // Pull holds up most of the way and then falls off a cliff, so the car
        // gets out of a hairpin without pinning the top speed to the engine.
        let fade = 1.0 - (along / TOP_SPEED).clamp(0.0, 1.0).powi(2);
        controls.throttle * ENGINE * fade
    } else if controls.brake > 0.0 && along <= STOPPED && along > -REVERSE_SPEED {
        -controls.brake * ENGINE * REVERSE_EFFORT
    } else {
        0.0
    };
    // Braking takes a share of each axle's grip rather than a fixed force, which
    // is ideal brake proportioning for free: the axle carrying the weight does
    // the stopping, and neither ever locks on its own.
    let effort = if controls.brake > 0.0 && rolling && along > 0.0 {
        BRAKE_EFFORT * controls.brake
    } else {
        0.0
    };
    let rear_handbrake = if controls.handbrake && rolling {
        -HANDBRAKE * along.signum()
    } else {
        0.0
    };
    let front_long = -effort * front_budget;
    // What the rear axle is being asked for, before the tyre has its say. The
    // difference between the two is what tells a hard stop from a locked wheel.
    let rear_demand = drive - effort * rear_budget + rear_handbrake;

    // Sideways grip arrives with speed. Without this the car fights its own
    // steering at a crawl, and the drag of a fully-locked front wheel is enough
    // to stop it pulling away at all.
    let bite = (car.velocity.length() / CRAWL).clamp(0.0, 1.0);
    let (front_long, front_lat) = tyre(front_angle, front_long, front_budget);
    let (rear_long, rear_lat) = tyre(rear_angle, rear_demand, rear_budget);
    let (front_lat, rear_lat) = (front_lat * bite, rear_lat * bite);

    // Gravity along the slope, and the losses that stop the car coasting forever.
    let climb = -GRAVITY * surface.slope / (1.0 + surface.slope * surface.slope).sqrt();
    let resistance = if rolling {
        -DRAG * along * along.abs() - ROLLING_RESISTANCE * along.signum()
    } else {
        0.0
    };

    let forward_accel =
        (front_long + rear_long + resistance - front_lat * steer.sin()) / MASS + climb;
    let lateral_accel = (front_lat * steer.cos() + rear_lat) / MASS;
    // A leftward force ahead of the centre of mass yaws the car left; the same
    // force behind it yaws the car right.
    let yaw_accel =
        (REAR_AXLE * rear_lat - FRONT_AXLE * front_lat * steer.cos()) / YAW_INERTIA;

    car.velocity += (heading * forward_accel + right * lateral_accel) * dt;
    car.yaw_rate += yaw_accel * dt;
    // Tyre scrub about the vertical axis. Without it the car keeps spinning long
    // after the tyres have let go.
    car.yaw_rate *= 1.0 - (2.2 * dt).min(1.0);

    if controls.throttle == 0.0 && controls.brake == 0.0 && car.velocity.length() < 0.3 {
        car.velocity = Vec3::ZERO;
        car.yaw_rate = 0.0;
    }

    car.g_force = Vec2::new(lateral_accel, forward_accel) / GRAVITY;
    car.rear_slip = slide(rear_angle, rear_demand, rear_budget, car.velocity.length());

    car.yaw_rate * dt
}

/// Steering lock at `speed`, in radians.
///
/// A corner of radius `R` taken at `v` needs `v^2 / R` of lateral acceleration,
/// and a wheelbase `L` at lock `d` gives `R = L / d`. So the lock the tyres can
/// actually hold is `mu * g * L / v^2`, and asking for much more than that only
/// scrubs the fronts away. Winding the lock off with speed is what lets a binary
/// key hold a corner at the limit instead of spearing off it.
fn lock(speed: f32) -> f32 {
    let holds = LOCK_MARGIN * FRICTION * GRAVITY * WHEELBASE / speed.max(1.0).powi(2);
    MAX_STEER.min(holds)
}

/// One tyre's share of the friction budget.
///
/// Longitudinal demand is served first — the driver asked for it — and the tyre
/// curve gets whatever circle is left. That coupling is the whole model: spend
/// the budget on throttle and there is none left to hold the corner.
fn tyre(slip: f32, long: f32, budget: f32) -> (f32, f32) {
    let long = long.clamp(-budget, budget);
    let spare = (budget * budget - long * long).max(0.0).sqrt();
    // Grip rises with slip angle, peaks, then falls away as the tyre lets go.
    let lat = -spare * (FALLOFF * (STIFFNESS * slip).atan()).sin();
    (long, lat)
}

/// How hard the rear axle is scrubbing rather than rolling, 0 while it grips.
/// What decides whether a tyre leaves a mark.
///
/// Two ways to lose it: past the slip angle where the tyre curve peaks, or asked
/// for more lengthways than the tyre has, which is a wheel locking or spinning
/// rather than merely a hard stop. `demand` is the force wanted, not the force
/// the tyre managed — braking as hard as the tyre allows leaves no mark.
fn slide(angle: f32, demand: f32, budget: f32, speed: f32) -> f32 {
    let sideways = (angle.abs() / PEAK_SLIP - 1.0).max(0.0);
    let locked = (demand.abs() / budget.max(1.0) - 1.0).max(0.0) * 4.0;
    ((sideways + locked) * (speed / CRAWL).min(1.0)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rolling(speed: f32) -> Car {
        Car {
            velocity: Vec3::NEG_Z * speed,
            ..default()
        }
    }

    const FLAT: Surface = Surface {
        grip: 1.0,
        slope: 0.0,
    };

    fn coast(car: &mut Car, controls: Controls, surface: Surface, seconds: f32) -> f32 {
        let mut yaw = 0.0f32;
        let dt = 1.0 / 120.0;
        for _ in 0..(seconds / dt) as usize {
            let heading = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
            let right = heading.cross(Vec3::Y);
            yaw += step(car, heading, right, controls, surface, dt);
        }
        yaw
    }

    #[test]
    fn a_tyre_never_exceeds_its_budget() {
        for slip in [-1.2, -0.3, 0.0, 0.05, 0.4, 1.5] {
            for long in [-20_000.0, -3_000.0, 0.0, 4_000.0, 20_000.0] {
                let budget = 9_000.0;
                let (fx, fy) = tyre(slip, long, budget);
                let total = (fx * fx + fy * fy).sqrt();
                assert!(
                    total <= budget + 1.0,
                    "slip {slip} long {long} pulled {total} from a {budget} budget"
                );
            }
        }
    }

    /// Spending the budget on throttle has to leave less for the corner.
    #[test]
    fn power_costs_cornering_grip() {
        let budget = 9_000.0;
        let (_, gentle) = tyre(0.2, 0.0, budget);
        let (_, flat_out) = tyre(0.2, budget * 0.9, budget);
        assert!(
            flat_out.abs() < gentle.abs() * 0.6,
            "throttle barely cost anything: {flat_out} against {gentle}"
        );
    }

    #[test]
    fn it_accelerates_and_tops_out() {
        let mut car = Car::default();
        let gas = Controls {
            throttle: 1.0,
            ..default()
        };
        coast(&mut car, gas, FLAT, 4.0);
        let quick = car.velocity.length();
        assert!((10.0..TOP_SPEED).contains(&quick), "4 s got to {quick} m/s");
        coast(&mut car, gas, FLAT, 20.0);
        let flat_out = car.velocity.length();
        assert!(
            flat_out > quick && flat_out < TOP_SPEED + 1.0,
            "topped out at {flat_out}"
        );
    }

    /// The lock on offer must be what the front tyres can hold, or a key held
    /// down spears the car off every corner.
    #[test]
    fn the_lock_never_asks_more_than_the_tyres_have() {
        assert_eq!(lock(0.0), MAX_STEER, "no lock at a standstill");
        for speed in [6.0f32, 10.0, 16.0, 24.0] {
            let turn = speed * speed * lock(speed) / WHEELBASE;
            assert!(
                turn <= FRICTION * GRAVITY * LOCK_MARGIN + 0.01,
                "full lock at {speed} m/s asks for {:.1} m/s^2",
                turn
            );
        }
        assert!(lock(24.0) < lock(10.0), "the lock has to wind off with speed");
    }

    #[test]
    fn it_turns_the_way_the_wheel_is_pointed() {
        let mut car = rolling(14.0);
        let yaw = coast(
            &mut car,
            Controls {
                throttle: 0.3,
                steer: 1.0,
                ..default()
            },
            FLAT,
            1.5,
        );
        assert!(yaw > 0.3, "left lock only yawed {yaw} rad");
        assert!(car.g_force.x.abs() > 0.3, "no lateral g in a corner");
    }

    /// A climb pulls back and a descent pulls on, and neither runs away.
    #[test]
    fn gravity_works_both_ways() {
        let mut uphill = rolling(14.0);
        coast(&mut uphill, Controls::default(), Surface { grip: 1.0, slope: 0.15 }, 2.0);
        let mut downhill = rolling(14.0);
        coast(&mut downhill, Controls::default(), Surface { grip: 1.0, slope: -0.15 }, 2.0);
        assert!(
            downhill.velocity.length() > uphill.velocity.length() + 2.0,
            "slope did nothing: {} against {}",
            downhill.velocity.length(),
            uphill.velocity.length()
        );
        assert!(uphill.g_force.y < 0.0, "a climb should read as deceleration");
    }

    #[test]
    fn grass_grips_less_than_tarmac() {
        let corner = Controls {
            throttle: 0.5,
            steer: 1.0,
            ..default()
        };
        let mut tarmac = rolling(16.0);
        let on_road = coast(&mut tarmac, corner, FLAT, 1.2);
        let mut grass = rolling(16.0);
        let off_road = coast(&mut grass, corner, Surface { grip: 0.38, slope: 0.0 }, 1.2);
        assert!(
            off_road < on_road * 0.8,
            "grass turned as well as tarmac: {off_road} against {on_road}"
        );
    }

    /// A stopped car with the wheel turned must still pull away: a tyre that is
    /// not rolling has nothing to push sideways against.
    #[test]
    fn it_pulls_away_on_full_lock() {
        let mut car = Car::default();
        coast(
            &mut car,
            Controls {
                throttle: 1.0,
                steer: 1.0,
                ..default()
            },
            FLAT,
            1.5,
        );
        assert!(
            car.velocity.length() > 4.0,
            "only reached {} m/s off the line",
            car.velocity.length()
        );
    }

    /// The pedal must not be what limits the stop — the tyres must be. Anything
    /// less and the brakes feel like a suggestion.
    #[test]
    fn the_brakes_pull_close_to_the_grip_limit() {
        let mut car = rolling(20.0);
        let before = car.velocity.length();
        let stop = Controls {
            brake: 1.0,
            ..default()
        };
        coast(&mut car, stop, FLAT, 0.5);
        let shed = (before - car.velocity.length()) / 0.5 / GRAVITY;
        assert!(
            shed > 0.95,
            "full brakes only shed {shed:.2} g of a possible {FRICTION}"
        );
        assert!(shed <= FRICTION + 0.1, "{shed:.2} g is more grip than exists");
    }

    /// Grip is what the brakes spend, so less of it has to mean a longer stop.
    #[test]
    fn brakes_are_worth_less_on_grass() {
        let stop = Controls {
            brake: 1.0,
            ..default()
        };
        let mut tarmac = rolling(16.0);
        coast(&mut tarmac, stop, FLAT, 0.6);
        let mut grass = rolling(16.0);
        coast(&mut grass, stop, Surface { grip: 0.38, slope: 0.0 }, 0.6);
        assert!(
            grass.velocity.length() > tarmac.velocity.length() + 3.0,
            "grass stopped it nearly as well as tarmac: {} against {}",
            grass.velocity.length(),
            tarmac.velocity.length()
        );
    }

    /// Braking flat out still has to leave something to steer with, or every
    /// corner entry is a straight line into the grass.
    #[test]
    fn there_is_grip_left_to_turn_on_the_brakes() {
        let mut car = rolling(16.0);
        let yaw = coast(
            &mut car,
            Controls {
                brake: 1.0,
                steer: 1.0,
                ..default()
            },
            FLAT,
            0.8,
        );
        assert!(yaw > 0.1, "full brakes killed the steering: {yaw} rad");
    }

    /// One key has to do both pedals: brake while rolling, reverse from a stop.
    #[test]
    fn the_brake_key_becomes_reverse_once_stopped() {
        let mut car = rolling(10.0);
        let stop = Controls {
            brake: 1.0,
            ..default()
        };
        coast(&mut car, stop, FLAT, 1.2);
        assert!(car.speed(Vec3::NEG_Z) < 0.1, "it never came to a stop");
        coast(&mut car, stop, FLAT, 1.5);
        let backwards = -car.speed(Vec3::NEG_Z);
        assert!(backwards > 1.5, "it only backed up at {backwards} m/s");
        assert!(backwards < REVERSE_SPEED, "reverse ran away to {backwards}");
    }

    /// The handbrake has to break the rear loose, and the model has to notice.
    #[test]
    fn the_handbrake_lets_the_back_go() {
        let mut car = rolling(16.0);
        coast(
            &mut car,
            Controls {
                steer: 1.0,
                handbrake: true,
                ..default()
            },
            FLAT,
            0.8,
        );
        assert!(car.rear_slip > 0.3, "rear only slipped {}", car.rear_slip);
        assert!(
            car.velocity.length() > 4.0,
            "the handbrake stopped the car instead of stepping it out"
        );
    }

    /// Nothing may produce a NaN, however hard it is thrown around.
    #[test]
    fn it_stays_finite_under_abuse() {
        let mut car = Car::default();
        let mut yaw = 0.0f32;
        let dt = 1.0 / 60.0;
        for i in 0..4_000 {
            let heading = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
            let controls = Controls {
                throttle: ((i / 17) % 2) as f32,
                brake: ((i / 23) % 2) as f32,
                steer: (((i / 11) % 3) as f32 - 1.0) * 1.0,
                handbrake: (i / 31) % 2 == 0,
            };
            let surface = Surface {
                grip: if i % 7 == 0 { 0.38 } else { 1.0 },
                slope: ((i as f32) * 0.03).sin() * 0.16,
            };
            yaw += step(&mut car, heading, heading.cross(Vec3::Y), controls, surface, dt);
            assert!(car.velocity.is_finite(), "velocity blew up at step {i}");
            assert!(car.yaw_rate.is_finite(), "yaw blew up at step {i}");
            assert!(car.g_force.is_finite(), "g-force blew up at step {i}");
            assert!(
                car.velocity.length() < TOP_SPEED * 1.5,
                "speed ran away to {} at step {i}",
                car.velocity.length()
            );
        }
    }
}

