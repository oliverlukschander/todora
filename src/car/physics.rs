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
/// How quickly a rotation left to itself dies away. This is the tyres scrubbing
/// sideways, which the two-wheel model does not otherwise account for; too much
/// of it and the car will not rotate into a corner at all.
const YAW_SCRUB: f32 = 1.5;
/// Yaw inertia of a slab the size of the car.
const YAW_INERTIA: f32 = MASS * (1.92 * 1.92 + 0.88 * 0.88) / 12.0;
/// Centre of gravity height, which is what turns acceleration into weight
/// transfer: squat under power, dive under brakes.
const CG_HEIGHT: f32 = 0.42 * SCALE;

/// Peak grip as a multiple of the load on the tyre. Sporty, deliberately: the
/// circuit is tight and the wheelbase is short, and grip is what buys the margin
/// back.
const FRICTION: f32 = 1.22;
/// The rear tyres are given a little more than the front, which is what makes a
/// road car run wide rather than swap ends when it is pushed too hard. Throttle
/// and the handbrake still spend that margin, so a drift is something you ask
/// for rather than something that happens to you.
const REAR_GRIP_BIAS: f32 = 1.06;
/// Shape of the tyre curve: grip climbs with slip angle, peaks, then falls away.
/// `STIFFNESS` sets how fast it climbs, `FALLOFF` how sharply it lets go.
const STIFFNESS: f32 = 9.0;
const FALLOFF: f32 = 1.5;
/// Slip angle where that curve peaks. Past it the tyre is sliding, not gripping.
const PEAK_SLIP: f32 = 0.192;
/// How far past that peak the steering is allowed to reach. A little over gives
/// the driver somewhere to go when the car will not quite turn in; much over and
/// full lock only scrubs the fronts away.
const SLIP_HEADROOM: f32 = 1.0;

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
/// Engine braking, off the throttle and rising with road speed. This is what
/// stops a real car running away down a hill, and without it a descent here
/// coasts to 84 km/h — which needs 46 m of corner radius on a circuit whose
/// tightest is ten.
const ENGINE_BRAKING: f32 = 1_800.0;

/// Steering lock at a standstill. Above walking pace it is cut back — see [`lock`].
const MAX_STEER: f32 = 0.70;
/// How fast the wheels follow the key, so steering has weight.
const STEER_RATE: f32 = 4.0;

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

    // Static load, shifted by what the tyres pushed against the road last frame.
    // Squat puts the rear tyre down harder, which is why power-on drifts hook up
    // again. Gravity is deliberately not in it: it pulls on the centre of mass
    // rather than through the contact patches, so it cannot pitch the car — and
    // counting it was taking weight off the front wheels all the way down every
    // hill, which is exactly where you need them.
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
    // Off the throttle the wheels turn the engine. It reaches the road through
    // the same two contact patches as everything else, so it shares their grip
    // rather than being free deceleration on top of it.
    let engine_braking = if controls.throttle == 0.0 && rolling {
        -ENGINE_BRAKING * (along.abs() / TOP_SPEED).min(1.0) * along.signum()
    } else {
        0.0
    };
    let front_long = -effort * front_budget;
    // What the rear axle is being asked for, before the tyre has its say. The
    // difference between the two is what tells a hard stop from a locked wheel.
    let rear_demand = drive - effort * rear_budget + rear_handbrake + engine_braking;

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

    // What the tyres are doing, and what the car does — which differ by the
    // slope. The first is what pitches the car and what an accelerometer in it
    // would read; the second is what actually moves it.
    let traction = (front_long + rear_long + resistance - front_lat * steer.sin()) / MASS;
    let forward_accel = traction + climb;
    let lateral_accel = (front_lat * steer.cos() + rear_lat) / MASS;
    // A leftward force ahead of the centre of mass yaws the car left; the same
    // force behind it yaws the car right.
    let yaw_accel =
        (REAR_AXLE * rear_lat - FRONT_AXLE * front_lat * steer.cos()) / YAW_INERTIA;

    car.velocity += (heading * forward_accel + right * lateral_accel) * dt;
    car.yaw_rate += yaw_accel * dt;
    // Tyre scrub about the vertical axis. Enough to settle the car once the tyres
    // have let go, not so much that it fights every corner.
    car.yaw_rate *= 1.0 - (YAW_SCRUB * dt).min(1.0);

    if controls.throttle == 0.0 && controls.brake == 0.0 && car.velocity.length() < 0.3 {
        car.velocity = Vec3::ZERO;
        car.yaw_rate = 0.0;
    }

    car.g_force = Vec2::new(lateral_accel, traction) / GRAVITY;
    car.rear_slip = slide(rear_angle, rear_demand, rear_budget, car.velocity.length());

    car.yaw_rate * dt
}

/// Steering lock at `speed`, in radians. Two parts, and both are needed.
///
/// Bending the car through a corner of radius `R` takes `L / R` of lock, and the
/// tightest corner the tyres can hold at `v` is `v^2 / (mu g)` — so that part is
/// `mu g L / v^2`, and it falls away fast with speed.
///
/// On top of it the front tyre has to be running at a slip angle to make any
/// force at all, and the most that is ever worth is the angle its curve peaks at.
/// Leaving that out is what made the car feel like it would not turn: at 18 m/s
/// the geometry alone asks for three degrees of lock, and three degrees of lock
/// puts no load through a tyre.
fn lock(speed: f32) -> f32 {
    let bend = FRICTION * GRAVITY * WHEELBASE / speed.max(1.0).powi(2);
    MAX_STEER.min(bend + PEAK_SLIP * SLIP_HEADROOM)
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

    /// Hold the key down and see what happens: peak lateral g, and whether the
    /// car ended up travelling somewhere other than where it points.
    fn corner(speed: f32, steer: f32, throttle: f32) -> (f32, bool) {
        let mut car = rolling(speed);
        let mut yaw = 0.0f32;
        let dt = 1.0 / 240.0;
        let (mut peak, mut spun) = (0.0f32, false);
        for i in 0..(2.5 / dt) as usize {
            let heading = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
            yaw += step(
                &mut car,
                heading,
                heading.cross(Vec3::Y),
                Controls { throttle, steer, ..default() },
                FLAT,
                dt,
            );
            if i > 60 {
                peak = peak.max(car.g_force.x.abs());
            }
            if car.velocity.normalize_or(heading).dot(heading) < 0.7 {
                spun = true;
            }
        }
        (peak, spun)
    }

    /// The car has to be able to use the grip it has, at every speed, from a key
    /// held down. Too little lock and it will not turn — which is what a steering
    /// lock built from the cornering geometry alone gets you, because three
    /// degrees of lock puts no load through a tyre.
    #[test]
    fn full_lock_reaches_the_grip_it_has() {
        assert_eq!(lock(0.0), MAX_STEER, "no lock at a standstill");
        assert!(lock(22.0) < lock(10.0), "the lock has to wind off with speed");
        for speed in [8.0f32, 12.0, 16.0, 20.0] {
            let (peak, spun) = corner(speed, 1.0, 0.35);
            // Not the full figure: a car set up to run wide rather than swap
            // ends saturates its front tyres a little before its rears, and the
            // difference is the understeer that keeps it driveable.
            assert!(
                peak > FRICTION * 0.78,
                "full lock at {speed} m/s only pulled {peak:.2} g of {FRICTION}"
            );
            assert!(!spun, "full lock at {speed} m/s spun the car");
        }
    }

    /// Steering alone must not swap the ends round. A rear-drive car should need
    /// the throttle or the handbrake for that, not a key press.
    #[test]
    fn steering_alone_does_not_spin_it() {
        for speed in [10.0f32, 16.0, 22.0] {
            assert!(!corner(speed, 1.0, 0.0).1, "lifting off at {speed} m/s spun it");
        }
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

    /// Lift off at the top of a hill and the hill must not run away with the
    /// car. Nothing steers at a speed the circuit has no radius for, and without
    /// the engine holding it back a descent here coasted to 84 km/h — which
    /// wants 46 m of corner, on a circuit whose tightest is ten.
    #[test]
    fn a_descent_does_not_run_away() {
        // Steeper than anything the circuit has: `hills_roll_instead_of_stepping`
        // holds its grade under 20%.
        let downhill = Surface {
            grip: 1.0,
            slope: -0.2,
        };
        let mut car = rolling(12.0);
        coast(&mut car, Controls::default(), downhill, 25.0);
        let settled = car.velocity.length();
        let radius = settled * settled / (FRICTION * GRAVITY);
        assert!(
            radius < 25.0,
            "coasts to {settled:.1} m/s, wanting {radius:.0} m of corner"
        );
        // And it is coasting, not stopping: a hill should still be free speed.
        assert!(settled > 8.0, "the hill gave nothing back: {settled:.1} m/s");
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




