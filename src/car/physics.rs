//! An arcade handling model: a velocity, a heading, and grip that decides how
//! far the two may disagree.
//!
//! The steering says how fast the car rotates. Grip says how much of the car's
//! sideways speed the tyres can scrub off each frame. When a corner asks for more
//! than that, the car keeps rotating but its velocity does not follow, and the
//! difference is the slide — nose inside the line of travel, tyres marking the
//! road. Let go and the slide pulls the nose back into line.
//!
//! This is deliberately not a tyre simulation. The first version of this file
//! was one — slip angles, a friction ellipse, load transfer — and it did what a
//! simulation does: spun under braking, ran wide on hills, and punished a
//! keyboard for not being a pedal. What arcade racers do instead is the model
//! here, and it is the model in every reference worth reading: Marco Monster's
//! "Car Physics for Games" for the kinematics, and the drift-racer pattern of a
//! lateral grip coefficient that the handbrake and the throttle pull down.
//!
//! Everything is in metres per second squared and fractions of grip, because
//! those are the numbers a designer can reason about. Weight is in the lag —
//! the car rotates toward where the wheels point rather than snapping there.

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

// --- grip ------------------------------------------------------------------

/// Sideways acceleration the tyres can produce on tarmac: how hard the car can
/// corner, and how fast a slide is scrubbed off. Sporty, deliberately — the
/// circuit is tight and the wheelbase is short.
const GRIP: f32 = 12.0;
/// How much of that the handbrake takes away. The rear lets go, the nose keeps
/// rotating, and the car drifts. The reference figure across arcade racers is a
/// slide at about a third of full grip.
const HANDBRAKE_LETS_GO: f32 = 0.68;
/// How much the throttle takes away once the car is already at the limit. Power
/// on, mid-corner, at the edge of grip: the back steps out. That is the rear
/// wheel drive, and the reason to be patient with the right foot.
const POWER_LETS_GO: f32 = 0.4;
/// Grip is worth a little less the further the car is already sliding, which is
/// what lets a drift be held rather than snapping straight the moment the
/// handbrake comes off.
const SLIDING_COSTS: f32 = 0.35;

// --- steering ----------------------------------------------------------------

/// Lock at a standstill. Above walking pace it is wound off — see [`lock`].
const MAX_STEER: f32 = 0.7;
/// Full lock asks for this much more turn than the grip can give, so it always
/// slides a little. Enough to lean on; not enough to lose it.
const LOCK_MARGIN: f32 = 1.35;
/// How fast the wheels follow the key.
const STEER_RATE: f32 = 5.0;
/// How fast the car rotates toward where the wheels are asking. This is the
/// weight: low and it wallows, high and it darts.
const YAW_RESPONSE: f32 = 5.5;
/// How hard a slide pulls the nose back into line with travel. This is what
/// ends a drift when the inputs let it, and what keeps a lift-off from becoming
/// a spin.
const ALIGN: f32 = 3.2;

// --- longitudinal --------------------------------------------------------------

const TOP_SPEED: f32 = 24.0;
/// Pull off the line. It fades as the square of speed, so the car gets out of a
/// hairpin hard and then eases into its top speed.
const ACCEL: f32 = 9.5;
/// Stopping power on tarmac, about 1.3 g. Always on offer, however hard the car
/// is turning: the brakes never spend the grip the corner is using. That is the
/// one place this model refuses to be a simulation, because a simulation is
/// what spins a keyboard driver under braking.
const BRAKE: f32 = 12.8;
/// Off the throttle the engine holds the car back, rising with speed. It is what
/// keeps a descent from running away.
const ENGINE_BRAKING: f32 = 1.9;
const DRAG: f32 = 0.0022;
const ROLLING: f32 = 0.3;
/// Soft ground rolls badly: rolling resistance climbs as grip falls, so grass is
/// about three times the drag of tarmac. That is how going off should cost you
/// — the car bogs down and comes back, rather than being thrown.
const SOFT_GROUND: f32 = 4.0;
/// Reverse: from a standstill, up to a crawl.
const REVERSE_ACCEL: f32 = 4.5;
const REVERSE_SPEED: f32 = 8.0;
/// How much of the slope's pull the car feels. A hill still adds speed, but a
/// hill at three times its real gradient — which is what the height scale gives
/// this circuit — would otherwise arrive at every corner far too fast.
const SLOPE_PULL: f32 = 0.7;

// --- the seams ---------------------------------------------------------------

/// Slower than this is stopped, which is when the brake key becomes reverse.
const STOPPED: f32 = 0.5;
/// Sideways faster than this and the car is sliding, not backing up.
const SIDEWAYS: f32 = 2.0;
/// A slide is worth a tyre mark from this angle, and is a full one at this angle.
const MARK_FROM: f32 = 0.12;
const MARK_FULL: f32 = 0.45;
/// Part of a scrubbed-off slide comes back as forward speed. Not physics — it is
/// the arcade convention that a drift held well should not cost the exit.
const SLIDE_RECOVERY: f32 = 0.2;

/// What the driver is asking for this frame.
#[derive(Clone, Copy, Default)]
pub(crate) struct Controls {
    /// 0 to 1.
    pub(crate) throttle: f32,
    /// 0 to 1. The brake while rolling, reverse once stopped.
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
    /// What an accelerometer bolted to the seat would read.
    pub(crate) g_force: Vec2,
    /// How hard the rear is sliding, 0 while it grips. Decides the tyre marks.
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
    let speed = car.velocity.length();
    let forward = car.velocity.dot(heading);
    let lateral = car.velocity.dot(right);
    let was = car.velocity;
    // Positive when travelling to the right of the nose.
    let slip = lateral.atan2(forward.abs());

    // Steering. The wheels follow the key with a lag, to a lock that shrinks
    // with speed.
    car.steer_angle = car
        .steer_angle
        .lerp(controls.steer * lock(speed), (STEER_RATE * dt).min(1.0));

    // How much grip there is to work with right now. The handbrake and the
    // throttle both take from it, and a car already sliding has a little less.
    let rolling = speed > STOPPED;
    let reversing = controls.brake > 0.0
        && forward < STOPPED
        && speed < REVERSE_SPEED
        && lateral.abs() < SIDEWAYS;
    let asking = (forward * car.yaw_rate).abs();
    let at_the_limit = (asking / (GRIP * surface.grip).max(0.1)).min(1.0);
    let power_slide = if rolling && !reversing {
        controls.throttle * at_the_limit * POWER_LETS_GO
    } else {
        0.0
    };
    let handbrake = if controls.handbrake && rolling { HANDBRAKE_LETS_GO } else { 0.0 };
    let already_sliding = (slip.abs() / MARK_FULL).min(1.0) * SLIDING_COSTS;
    let hold = (1.0 - handbrake) * (1.0 - power_slide) * (1.0 - already_sliding);
    let grip = GRIP * surface.grip * hold;

    // Yaw. The wheels ask for the rate that would carry the car round the arc
    // they point along; the slide asks the nose to come back toward travel. The
    // car turns toward the sum, with the lag that gives it weight. A rear that
    // has let go pulls the nose back weakly, and that is the drift: the nose
    // runs on ahead of where the car is going.
    let bite = (speed / 2.0).min(1.0);
    let kinematic = forward * car.steer_angle.tan() / WHEELBASE;
    let align = -slip * ALIGN * hold * bite;
    let target = kinematic * bite + align;
    car.yaw_rate += (target - car.yaw_rate) * (YAW_RESPONSE * dt).min(1.0);
    let yaw = car.yaw_rate * dt;

    // The car has turned; its velocity has not. In the new frame some of what
    // was forward speed is now sideways, and the tyres scrub off as much of that
    // as grip allows. What they cannot is the slide.
    let turned = Quat::from_rotation_y(yaw);
    let heading = turned * heading;
    let right = turned * right;
    let sideways = car.velocity.dot(right);
    let scrub = sideways.clamp(-grip * dt, grip * dt);
    car.velocity -= right * scrub;
    if forward.abs() > STOPPED {
        car.velocity += heading * (scrub.abs() * SLIDE_RECOVERY * forward.signum());
    }

    // Along the nose: engine, brakes, and everything that slows a car down.
    let forward = car.velocity.dot(heading);
    let mut push = 0.0;
    if controls.throttle > 0.0 {
        let fade = 1.0 - (forward / TOP_SPEED).clamp(0.0, 1.0).powi(2);
        push += ACCEL * controls.throttle * fade;
    } else if rolling {
        push -= ENGINE_BRAKING * (forward / TOP_SPEED).clamp(-1.0, 1.0);
    }
    if reversing {
        if forward > -REVERSE_SPEED {
            push -= REVERSE_ACCEL * controls.brake;
        }
    } else if controls.brake > 0.0 && rolling {
        // Independent of the corner. See `BRAKE`.
        push -= BRAKE * surface.grip * controls.brake * forward.signum();
    }
    if rolling {
        let rolling_drag = ROLLING * (1.0 + SOFT_GROUND * (1.0 - surface.grip));
        push -= DRAG * speed * forward + rolling_drag * forward.signum();
    }
    let climb = -GRAVITY * SLOPE_PULL * surface.slope / (1.0 + surface.slope * surface.slope).sqrt();
    car.velocity += heading * ((push + climb) * dt);

    // A car braked to a crawl stops, rather than creeping on the last of its
    // rounding error.
    if controls.throttle == 0.0 && !reversing && car.velocity.length() < 0.3 {
        car.velocity = Vec3::ZERO;
        car.yaw_rate = 0.0;
    }

    // What the seat feels: the change in velocity the tyres made, which is the
    // total change less the slope's share.
    let accel = (car.velocity - was) / dt.max(1e-4) - heading * climb;
    car.g_force = Vec2::new(accel.dot(right), accel.dot(heading)) / GRAVITY;
    car.rear_slip = if speed > 2.0 {
        ((slip.abs() - MARK_FROM) / (MARK_FULL - MARK_FROM)).clamp(0.0, 1.0).max(handbrake)
    } else {
        0.0
    };

    yaw
}

/// Steering lock at `speed`, in radians.
///
/// A wheelbase `L` at lock `d` turns a car doing `v` at `v^2 tan(d) / L`. Full
/// lock is set to ask for a little more than the grip can give, so it always
/// slides a touch and never spins. The lock shrinks as the square of speed, and
/// that is the whole reason a fast corner is a wide corner.
fn lock(speed: f32) -> f32 {
    let asks = LOCK_MARGIN * GRIP * WHEELBASE / speed.max(1.0).powi(2);
    asks.atan().min(MAX_STEER)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLAT: Surface = Surface {
        grip: 1.0,
        slope: 0.0,
    };
    const GRASS: Surface = Surface {
        grip: 0.38,
        slope: 0.0,
    };

    fn rolling(speed: f32) -> Car {
        Car {
            velocity: Vec3::NEG_Z * speed,
            ..default()
        }
    }

    /// Drive with fixed controls, turning the car as the model asks. Returns the
    /// heading yawed through.
    fn drive(car: &mut Car, controls: Controls, surface: Surface, seconds: f32) -> f32 {
        let mut yaw = 0.0f32;
        let dt = 1.0 / 120.0;
        for _ in 0..(seconds / dt) as usize {
            let heading = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
            yaw += step(car, heading, heading.cross(Vec3::Y), controls, surface, dt);
        }
        yaw
    }

    /// How far the car is travelling from where it points, in radians.
    fn adrift(car: &Car, yaw: f32) -> f32 {
        let heading = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
        car.velocity.normalize_or(heading).angle_between(heading)
    }

    /// Full controls held for `seconds`; the worst drift angle seen while still
    /// moving, and the peak lateral g.
    fn worst_of(speed: f32, controls: Controls, surface: Surface, seconds: f32) -> (f32, f32) {
        let mut car = rolling(speed);
        let mut yaw = 0.0f32;
        let dt = 1.0 / 240.0;
        let (mut worst, mut peak) = (0.0f32, 0.0f32);
        for i in 0..(seconds / dt) as usize {
            let heading = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
            yaw += step(&mut car, heading, heading.cross(Vec3::Y), controls, surface, dt);
            if car.velocity.length() > 5.0 {
                worst = worst.max(adrift(&car, yaw));
            }
            if i > 30 {
                peak = peak.max(car.g_force.x.abs());
            }
        }
        (worst, peak)
    }

    #[test]
    fn it_accelerates_and_tops_out() {
        let mut car = Car::default();
        let gas = Controls {
            throttle: 1.0,
            ..default()
        };
        drive(&mut car, gas, FLAT, 4.0);
        let quick = car.velocity.length();
        assert!((12.0..TOP_SPEED).contains(&quick), "4 s got to {quick} m/s");
        drive(&mut car, gas, FLAT, 20.0);
        let flat_out = car.velocity.length();
        assert!(
            flat_out > quick && flat_out < TOP_SPEED + 0.5,
            "topped out at {flat_out}"
        );
    }

    #[test]
    fn it_turns_the_way_the_wheel_is_pointed() {
        let mut car = rolling(14.0);
        let yaw = drive(
            &mut car,
            Controls {
                throttle: 0.3,
                steer: 1.0,
                ..default()
            },
            FLAT,
            1.5,
        );
        assert!(yaw > 0.5, "left lock only yawed {yaw} rad");
        assert!(car.g_force.x.abs() > 0.5, "no lateral g in a corner");
    }

    /// The whole point of the model. A key held down must turn the car at every
    /// speed, must get it near the grip it has, and must never spin it. A fast
    /// corner is a wide corner, not a lost one.
    #[test]
    fn full_lock_always_turns_and_never_spins() {
        for speed in [6.0f32, 10.0, 14.0, 18.0, 22.0] {
            let (worst, peak) = worst_of(
                speed,
                Controls {
                    steer: 1.0,
                    throttle: 0.3,
                    ..default()
                },
                FLAT,
                2.5,
            );
            assert!(
                peak > 0.8 * GRIP / GRAVITY,
                "full lock at {speed} m/s only pulled {peak:.2} g"
            );
            assert!(
                worst < 0.5,
                "full lock at {speed} m/s swung the car {:.0} degrees off line",
                worst.to_degrees()
            );
        }
    }

    /// Nothing the tyres do sideways may exceed the grip they have.
    #[test]
    fn lateral_acceleration_never_exceeds_grip() {
        for speed in [8.0f32, 16.0, 24.0] {
            let (_, peak) = worst_of(
                speed,
                Controls {
                    steer: 1.0,
                    ..default()
                },
                FLAT,
                2.0,
            );
            assert!(peak <= GRIP / GRAVITY + 0.05, "{peak:.2} g from {} of grip", GRIP / GRAVITY);
        }
    }

    /// Braking is the answer to arriving too fast, so it has to work — hard,
    /// and while turning, and without swapping the ends. This is the corner
    /// the first version of this model could not take.
    #[test]
    fn braking_into_a_corner_slows_it_and_does_not_spin_it() {
        for steer in [-1.0f32, 1.0] {
            let mut car = rolling(22.0);
            let mut yaw = 0.0f32;
            let dt = 1.0 / 240.0;
            let mut worst = 0.0f32;
            for _ in 0..(1.2 / dt) as usize {
                let heading = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
                yaw += step(
                    &mut car,
                    heading,
                    heading.cross(Vec3::Y),
                    Controls {
                        brake: 1.0,
                        steer,
                        ..default()
                    },
                    FLAT,
                    dt,
                );
                if car.velocity.length() > 5.0 {
                    worst = worst.max(adrift(&car, yaw));
                }
            }
            let shed = (22.0 - car.velocity.length()) / 1.2 / GRAVITY;
            assert!(shed > 1.0, "brakes on {steer} lock only shed {shed:.2} g");
            assert!(
                worst < 0.4,
                "braking on {steer} lock swung the car {:.0} degrees off line",
                worst.to_degrees()
            );
            assert!(yaw.abs() > 0.15, "braking killed the steering: {yaw} rad");
        }
    }

    #[test]
    fn the_brakes_pull_close_to_the_grip_limit() {
        let mut car = rolling(20.0);
        drive(
            &mut car,
            Controls {
                brake: 1.0,
                ..default()
            },
            FLAT,
            0.5,
        );
        let shed = (20.0 - car.velocity.length()) / 0.5 / GRAVITY;
        assert!(shed > 1.1, "full brakes only shed {shed:.2} g");
    }

    #[test]
    fn brakes_are_worth_less_on_grass() {
        let stop = Controls {
            brake: 1.0,
            ..default()
        };
        let mut tarmac = rolling(16.0);
        drive(&mut tarmac, stop, FLAT, 0.6);
        let mut grass = rolling(16.0);
        drive(&mut grass, stop, GRASS, 0.6);
        assert!(
            grass.velocity.length() > tarmac.velocity.length() + 3.0,
            "grass stopped it nearly as well as tarmac: {} against {}",
            grass.velocity.length(),
            tarmac.velocity.length()
        );
    }

    #[test]
    fn grass_grips_less_than_tarmac() {
        let corner = Controls {
            throttle: 0.5,
            steer: 1.0,
            ..default()
        };
        let (on_road, _) = worst_of(16.0, corner, FLAT, 1.5);
        let (off_road, _) = worst_of(16.0, corner, GRASS, 1.5);
        assert!(
            off_road > on_road + 0.1,
            "grass slid no more than tarmac: {off_road:.2} against {on_road:.2} rad"
        );
    }

    /// The handbrake has to step the back out, keep the speed, and the model
    /// has to notice — that is where the tyre marks come from.
    #[test]
    fn the_handbrake_lets_the_back_go() {
        let mut car = rolling(16.0);
        let yaw = drive(
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
        assert!(adrift(&car, yaw) > 0.25, "the back never came round");
        assert!(
            car.velocity.length() > 6.0,
            "the handbrake stopped the car instead of stepping it out"
        );
    }

    /// Let go of everything mid-drift and the car has to straighten up, not
    /// carry on round.
    #[test]
    fn a_drift_recovers_when_released() {
        let mut car = rolling(16.0);
        let mut yaw = drive(
            &mut car,
            Controls {
                steer: 1.0,
                handbrake: true,
                ..default()
            },
            FLAT,
            0.8,
        );
        let sliding = adrift(&car, yaw);
        assert!(sliding > 0.25, "never got it sliding: {sliding:.2} rad");
        let dt = 1.0 / 120.0;
        for _ in 0..(1.5 / dt) as usize {
            let heading = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
            yaw += step(&mut car, heading, heading.cross(Vec3::Y), Controls::default(), FLAT, dt);
        }
        let after = adrift(&car, yaw);
        assert!(
            after < sliding * 0.4,
            "released, the slide only came back from {sliding:.2} to {after:.2} rad"
        );
    }

    /// Rear wheel drive: flat out at the limit of grip has to cost the corner.
    #[test]
    fn power_on_at_the_limit_steps_the_back_out() {
        let steady = Controls {
            steer: 1.0,
            throttle: 0.3,
            ..default()
        };
        let flat_out = Controls {
            steer: 1.0,
            throttle: 1.0,
            ..default()
        };
        let (gentle, _) = worst_of(14.0, steady, FLAT, 2.0);
        let (booted, _) = worst_of(14.0, flat_out, FLAT, 2.0);
        assert!(
            booted > gentle + 0.08,
            "the throttle cost nothing: {booted:.2} against {gentle:.2} rad of slide"
        );
    }

    /// A climb pulls back and a descent pulls on, and neither runs away.
    #[test]
    fn gravity_works_both_ways() {
        let mut uphill = rolling(14.0);
        drive(&mut uphill, Controls::default(), Surface { grip: 1.0, slope: 0.15 }, 2.0);
        let mut downhill = rolling(14.0);
        drive(&mut downhill, Controls::default(), Surface { grip: 1.0, slope: -0.15 }, 2.0);
        assert!(
            downhill.velocity.length() > uphill.velocity.length() + 2.0,
            "slope did nothing: {} against {}",
            downhill.velocity.length(),
            uphill.velocity.length()
        );
    }

    /// Lift off at the top of a hill and the hill must not run away with the
    /// car: nothing steers at a speed the circuit has no radius for.
    #[test]
    fn a_descent_does_not_run_away() {
        // Steeper than anything the circuit has: `hills_roll_instead_of_stepping`
        // holds its grade under 20%.
        let mut car = rolling(12.0);
        drive(&mut car, Controls::default(), Surface { grip: 1.0, slope: -0.2 }, 25.0);
        let settled = car.velocity.length();
        let radius = settled * settled / GRIP;
        assert!(radius < 22.0, "coasts to {settled:.1} m/s, wanting {radius:.0} m of corner");
        assert!(settled > 7.0, "the hill gave nothing back: {settled:.1} m/s");
    }

    /// Pointing one way and travelling sixty degrees off it, which is what
    /// having lost it looks like. Returns the speed and the drift angle left
    /// after `seconds`.
    fn slide(seconds: f32, controls: Controls, surface: Surface) -> (f32, f32) {
        let mut car = Car {
            velocity: Quat::from_rotation_y(1.05) * Vec3::NEG_Z * 18.0,
            ..default()
        };
        let yaw = drive(&mut car, controls, surface, seconds);
        (car.velocity.length(), adrift(&car, yaw))
    }

    /// A car that has lost it has to come back into line again — on the grass,
    /// and downhill, which is where it happens — and braking must help, not
    /// hurt. The first version of this model failed both: sideways, nothing
    /// slowed it, and the brake key became the throttle.
    #[test]
    fn a_slide_comes_back_down() {
        let stop = Controls {
            brake: 1.0,
            ..default()
        };
        for surface in [FLAT, GRASS, Surface { grip: 0.38, slope: -0.16 }] {
            let (coasted, still_adrift) = slide(4.0, Controls::default(), surface);
            assert!(
                still_adrift < 0.15,
                "grip {} slope {}: still {:.0} degrees sideways after four seconds",
                surface.grip,
                surface.slope,
                still_adrift.to_degrees()
            );
            assert!(
                coasted < 13.0,
                "grip {} slope {}: coasting held {coasted:.1} m/s after four seconds",
                surface.grip,
                surface.slope
            );
            // Compared before the brakes have stopped the car altogether, at
            // which point the held key is reverse and the speed is backwards.
            let (coasting, _) = slide(2.0, Controls::default(), surface);
            let (braked, _) = slide(2.0, stop, surface);
            assert!(
                braked < coasting - 3.0,
                "grip {} slope {}: braking barely helped, {braked:.1} against {coasting:.1}",
                surface.grip,
                surface.slope
            );
        }
    }

    /// The brake key must not turn into the throttle just because the car has
    /// spun: travelling backwards along its own nose at speed is the second half
    /// of a spin, not a request to reverse.
    #[test]
    fn the_brake_stays_a_brake_in_a_spin() {
        let mut car = Car {
            velocity: Vec3::Z * 14.0,
            ..default()
        };
        drive(
            &mut car,
            Controls {
                brake: 1.0,
                ..default()
            },
            FLAT,
            1.0,
        );
        assert!(
            car.velocity.length() < 11.0,
            "the brakes did nothing in a spin: 14 to {:.1} m/s",
            car.velocity.length()
        );
    }

    /// One key has to do both pedals: brake while rolling, reverse from a stop.
    #[test]
    fn the_brake_key_becomes_reverse_once_stopped() {
        let mut car = rolling(10.0);
        let stop = Controls {
            brake: 1.0,
            ..default()
        };
        drive(&mut car, stop, FLAT, 1.2);
        assert!(car.speed(Vec3::NEG_Z) < 0.1, "it never came to a stop");
        drive(&mut car, stop, FLAT, 1.5);
        let backwards = -car.speed(Vec3::NEG_Z);
        assert!(backwards > 1.5, "it only backed up at {backwards} m/s");
        assert!(backwards < REVERSE_SPEED + 0.1, "reverse ran away to {backwards}");
    }

    /// A stopped car with the wheel turned must still pull away.
    #[test]
    fn it_pulls_away_on_full_lock() {
        let mut car = Car::default();
        drive(
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
                steer: ((i / 11) % 3) as f32 - 1.0,
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
                car.velocity.length() < TOP_SPEED * 1.4,
                "speed ran away to {} at step {i}",
                car.velocity.length()
            );
        }
    }
}

