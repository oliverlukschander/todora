//! The engine: an arcade handling model. A velocity, a heading, and grip that
//! decides how far the two may disagree.
//!
//! The steering says how fast the car rotates. Grip says how much of the car's
//! sideways speed the tyres can scrub off each step. When a corner asks for more
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
//! Everything a car is, as a car, lives in one [`Handling`] value — metres per
//! second squared and fractions of grip, the numbers a designer can reason
//! about. Weight is in the lag: the car rotates toward where the wheels point
//! rather than snapping there. [`step`] is pure: it takes a [`Car`], a
//! [`Handling`], what the driver wants and what the road offers, and advances by
//! `dt`. Nothing here knows about entities, frames or keys.

use bevy::prelude::*;

/// The glTF is modelled at full size; this is the fraction of it we drive.
/// Shared by the model, ghosts, wheel contacts and wheelbase.
pub(crate) const SCALE: f32 = 0.324;
/// From `tools/make_shooting_brake.py`, at driving scale.
pub(crate) const WHEEL_RADIUS: f32 = 0.20 * SCALE;
pub(crate) const WHEEL_WIDTH: f32 = 0.16 * SCALE;
pub(crate) const HALF_TRACK: f32 = 0.50 * SCALE;
/// Axle distances from the model origin, which sits between them.
pub(crate) const FRONT_AXLE: f32 = 0.70 * SCALE;
pub(crate) const REAR_AXLE: f32 = 0.76 * SCALE;

pub(crate) const GRAVITY: f32 = 9.81;

// The seams of the model, which are not a matter of setup.

/// Slower than this is stopped, which is when the brake key becomes reverse.
const STOPPED: f32 = 0.5;
/// Sideways faster than this and the car is sliding, not backing up.
const SIDEWAYS: f32 = 2.0;
/// A slide is worth a tyre mark from this angle, and is a full one at this angle.
const MARK_FROM: f32 = 0.12;
const MARK_FULL: f32 = 0.45;

/// Everything that makes one car drive like itself. A value, not a recompile:
/// a second car, or the same car on a different setup, is another one of these.
/// Accelerations are in metres per second squared; the rest are fractions.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(crate) struct Handling {
    /// Axle to axle. What turns a steering angle into a rate of rotation.
    pub wheelbase: f32,

    /// Sideways acceleration the tyres can produce on tarmac: how hard the car
    /// can corner, and how fast a slide is scrubbed off.
    pub grip: f32,
    /// Grip climbs with the square of speed, the way downforce does. A fast
    /// corner is still a wide corner, but not a hopeless one.
    pub downforce: f32,
    /// Braking loads the nose, and a loaded nose bites: braking into a corner
    /// turns the car tighter, not looser. A driver who has arrived too fast
    /// needs the pedal to help, and the intuitive thing to do must be right.
    pub brake_bite: f32,
    /// How much grip the handbrake takes away. The rear lets go, the nose keeps
    /// rotating, and the car drifts.
    pub handbrake_lets_go: f32,
    /// How much grip the throttle takes away once the car is already at the
    /// limit. It goes with the square of the throttle, so a feathered exit
    /// costs almost nothing and a booted one costs all of this. That is the
    /// rear wheel drive.
    pub power_lets_go: f32,
    /// Grip is worth a little less the further the car is already sliding,
    /// which is what lets a drift be held rather than snapping straight the
    /// moment the handbrake comes off. It starts past the slip a clean corner
    /// carries, or it taxes every corner on the way to the one it is meant for.
    pub sliding_costs: f32,

    /// Lock at a standstill. Above walking pace it is wound off — see
    /// [`Handling::lock`].
    pub max_steer: f32,
    /// Full lock asks for this much more turn than the grip can give. On a
    /// keyboard full lock is the only lock there is, so it sits close to the
    /// limit: over by much and every corner is a push wide.
    pub lock_margin: f32,
    /// How fast the wheels follow the driver.
    pub steer_rate: f32,
    /// How fast the car rotates toward where the wheels are asking. This is the
    /// weight: low and it wallows, high and it darts.
    pub yaw_response: f32,
    /// How hard a slide pulls the nose back into line with travel. What ends a
    /// drift when the inputs let it, and keeps a lift-off from becoming a spin.
    pub align: f32,
    /// How hard a rear that has let go throws the tail round instead. While the
    /// rear grips, a slide is damped by `align`; once the throttle or the
    /// handbrake has taken its grip away, the same slide feeds the rotation, and
    /// the car turns more than the wheel asked. That is the oversteer: without
    /// it a loose rear only ever ran the car wide with its nose tucked in, which
    /// is a slide that reads as understeer. It is scaled by how much the driver
    /// has let the rear go, so steering alone never gets any of it.
    pub kick: f32,

    pub top_speed: f32,
    /// Pull off the line. It fades as the square of speed, so the car gets out
    /// of a hairpin hard and then eases into its top speed. A rear the driver
    /// has let go delivers less of it: the friction it is spending sideways is
    /// not there to drive with. That is wheelspin, and it is why booting it in a
    /// slide does not make the slide faster.
    pub accel: f32,
    /// Stopping power on tarmac. Always on offer, however hard the car is
    /// turning: the brakes never spend the grip the corner is using. That is the
    /// one place this model refuses to be a simulation, because a simulation is
    /// what spins a keyboard driver under braking.
    pub brake: f32,
    /// Off the throttle the engine holds the car back, rising with speed to
    /// this at top speed. Strong, deliberately — about half the brakes — so the
    /// throttle is the speed control and the brakes are for the big stops.
    /// Lift for a corner and the car settles into it; that is most of what
    /// makes it fun to drive on one pedal, and it is what keeps a descent from
    /// running away.
    pub engine_braking: f32,
    /// Engine braking never falls below this fraction of itself, so it still
    /// bites in the slow corners, where the lift is used most.
    pub engine_braking_floor: f32,
    pub drag: f32,
    pub rolling: f32,
    /// Soft ground rolls badly: rolling resistance climbs as grip falls.
    pub soft_ground: f32,
    /// A sliding tyre drags. Beyond the slip a clean corner carries, the rubber
    /// is scrubbing across the road rather than rolling along it, and that costs
    /// speed in the direction of travel as well as sideways — this much of it,
    /// at a full slide. It is what makes tyre marks on the road and speed coming
    /// off the car the same event, however hard the throttle is down.
    pub scrub_drag: f32,
    /// Off the road the ground ploughs, against whichever way the car is going.
    /// The drag rises with speed and with the cube of how little grip there is,
    /// so a kerb costs next to nothing and the grass at pace costs most of a g.
    /// A car can still crawl back to the road.
    pub off_road_drag: f32,
    /// Reverse: from a standstill, up to a crawl.
    pub reverse_accel: f32,
    pub reverse_speed: f32,
}

impl Handling {
    /// The comic shooting brake, on the tyres it ships with.
    pub const SHOOTING_BRAKE: Handling = Handling {
        wheelbase: FRONT_AXLE + REAR_AXLE,
        grip: 14.0,
        downforce: 0.009,
        brake_bite: 0.25,
        handbrake_lets_go: 0.68,
        power_lets_go: 0.40,
        sliding_costs: 0.30,
        // Keep the previous low-speed turning radius with the shorter
        // wheelbase: atan(tan(0.7) * 0.405), so shrinking does not add steering scrub.
        max_steer: 0.328_748_2,
        lock_margin: 1.1,
        steer_rate: 10.0,
        yaw_response: 11.0,
        align: 3.6,
        kick: 2.0,
        top_speed: 24.0,
        accel: 9.5,
        brake: 12.8,
        engine_braking: 4.5,
        engine_braking_floor: 0.3,
        drag: 0.0022,
        rolling: 0.3,
        soft_ground: 4.0,
        scrub_drag: 3.0,
        off_road_drag: 1.8,
        reverse_accel: 4.5,
        reverse_speed: 8.0,
    };

    /// Sideways grip on tarmac at `speed`, downforce included.
    pub fn grip_at(&self, speed: f32) -> f32 {
        self.grip + self.downforce * speed * speed
    }

    /// Steering lock at `speed`, in radians.
    ///
    /// A wheelbase `L` at lock `d` turns a car doing `v` at `v^2 tan(d) / L`.
    /// Full lock is set to ask for a little more than the grip can give, so it
    /// always slides a touch and never spins. The lock shrinks as the square of
    /// speed, and that is the whole reason a fast corner is a wide corner.
    pub fn lock(&self, speed: f32) -> f32 {
        let asks = self.lock_margin * self.grip_at(speed) * self.wheelbase / speed.max(1.0).powi(2);
        asks.atan().min(self.max_steer)
    }
}

/// What the driver is asking for this step. Produced by the keys, a gamepad, or
/// a [`super::driver::Driver`]; the engine does not care which.
#[derive(Component, Clone, Copy, Default, Debug, PartialEq)]
pub(crate) struct Controls {
    /// 0 to 1.
    pub throttle: f32,
    /// 0 to 1. The brake while rolling, reverse once stopped.
    pub brake: f32,
    /// -1 to 1. Positive steers left, yawing about +Y.
    pub steer: f32,
    pub handbrake: bool,
}

/// What the road is offering at the contact patches.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Surface {
    /// Fraction of tarmac grip.
    pub grip: f32,
    /// Rise over run along `heading`; positive is a climb.
    pub slope: f32,
}

/// The car's motion, in world space except where noted. Everything after
/// `stranded` is telemetry: read by the meter, the tyre marks and anyone else
/// who wants to know what the car is doing, written only by [`step`].
#[derive(Component, Default, Debug)]
pub(crate) struct Car {
    /// Metres per second across the ground. Y is unused: the loft carries height.
    pub velocity: Vec3,
    /// Radians per second about +Y.
    pub yaw_rate: f32,
    /// Where the front wheels actually point, which lags the driver.
    pub steer_angle: f32,
    /// Reverse is engaged only from a stop, and held until the pedal is released.
    /// Backward motion in a spin must still get the brakes.
    pub reversing: bool,
    /// Seconds spent off the circuit getting nowhere. The track uses it to
    /// decide when to fetch the car back.
    pub stranded: f32,
    /// How far round the lap the car was last found, and `None` before it has
    /// been found at all.
    ///
    /// Carried by the car rather than worked out from where it is, because
    /// where it is does not always answer: a circuit that passes over itself
    /// has two roads at a point of the map, and which of them the car is on is
    /// a fact about the last frame rather than about this one. The track reads
    /// it and writes it back — see `Track::fix`. Nothing in the engine touches
    /// it; it is here for the same reason `stranded` is, which is that it
    /// belongs to this car and to no other.
    pub along: Option<f32>,

    /// Acceleration in g, in the car's own frame: x to the right, y forward.
    /// What an accelerometer bolted to the seat would read.
    pub g_force: Vec2,
    /// Angle between where the car points and where it is going, in radians.
    /// Positive when travelling to the right of the nose.
    pub slip_angle: f32,
    /// How much of the available sideways grip the corner is asking for. Past
    /// one, the car is being asked for more than it has.
    pub grip_used: f32,
    /// How hard the rear is sliding, 0 while it grips. Decides the tyre marks.
    pub rear_slip: f32,
}

impl Car {
    /// Speed along the car's own nose. Negative in reverse.
    pub fn speed(&self, heading: Vec3) -> f32 {
        self.velocity.dot(heading)
    }
}

/// Advance the car by `dt` and report the yaw to apply, in radians.
///
/// `heading` and `right` are the car's own axes, level in XZ. Call this pure step
/// with a small positive `dt`. The game uses a fixed timestep; this numerical
/// integration is only approximately equivalent at other step sizes.
pub(crate) fn step(
    car: &mut Car,
    h: &Handling,
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

    // Steering. The wheels follow the driver with a lag, to a lock that shrinks
    // with speed.
    car.steer_angle = car
        .steer_angle
        .lerp(controls.steer * h.lock(speed), (h.steer_rate * dt).min(1.0));

    // How much grip there is to work with right now. The handbrake and the
    // throttle both take from it, a car already sliding has a little less, and
    // a braking car has a little more at the nose.
    let rolling = speed > STOPPED;
    // Reverse from a stop, or once already backing up — and never in a slide. A
    // slide drops the forward component to nothing while the car is still
    // travelling at speed; what tells the two apart is whether it is going
    // sideways.
    car.reversing = controls.brake > 0.0
        && controls.throttle == 0.0
        && lateral.abs() < SIDEWAYS
        && (speed < STOPPED || (car.reversing && forward < STOPPED));
    let reversing = car.reversing;
    let base = h.grip_at(speed) * surface.grip;
    let asking = (forward * car.yaw_rate).abs() / base.max(0.1);
    let power_slide = if rolling && !reversing {
        controls.throttle.powi(2) * asking.min(1.0) * h.power_lets_go
    } else {
        0.0
    };
    let handbrake = if controls.handbrake && rolling {
        h.handbrake_lets_go
    } else {
        0.0
    };
    let already_sliding =
        ((slip.abs() - MARK_FROM) / (MARK_FULL - MARK_FROM)).clamp(0.0, 1.0) * h.sliding_costs;
    let hold = (1.0 - handbrake) * (1.0 - power_slide) * (1.0 - already_sliding);
    // How far the driver has let the rear go — the throttle and the handbrake,
    // not the slide itself, or a slide would feed on itself with no way out.
    let loose = (handbrake + power_slide).min(1.0);
    let nose_bite = if rolling && !reversing {
        1.0 + h.brake_bite * controls.brake
    } else {
        1.0
    };
    let grip = base * hold * nose_bite;

    // Yaw. The wheels ask for the rate that would carry the car round the arc
    // they point along. A slide asks two opposite things of the nose, and which
    // wins is what understeer and oversteer are here: a rear that grips pulls
    // the nose back toward travel and damps the slide; a rear the driver has let
    // go throws the tail round and feeds it, so the car turns more than it was
    // asked to. Lift, or wind the wheel back, and `loose` falls, `align` takes
    // over, and the car straightens — which is how a drift is held and how it is
    // caught. The car turns toward the sum, with the lag that gives it weight.
    let bite = (speed / 2.0).min(1.0);
    let kinematic = forward * car.steer_angle.tan() / h.wheelbase;
    // When backing up it is the tail, not the nose, that follows travel.
    let direction = if forward < 0.0 { -1.0 } else { 1.0 };
    let align = -slip * direction * h.align * hold * bite;
    let kick = slip * direction * h.kick * loose * bite;
    let target = kinematic * bite + align + kick;
    car.yaw_rate += (target - car.yaw_rate) * (h.yaw_response * dt).min(1.0);
    let yaw = car.yaw_rate * dt;

    // The car has turned; its velocity has not. In the new frame some of what
    // was forward speed is now sideways, and the tyres scrub off as much of that
    // as grip allows. What they cannot is the slide. What they scrub is gone —
    // a sliding tyre turns speed into heat, which is why a slide slows the car,
    // and why it must: an earlier version handed part of it back as forward
    // speed, and a sliding car accelerated.
    let turned = Quat::from_rotation_y(yaw);
    let heading = turned * heading;
    let right = turned * right;
    let sideways = car.velocity.dot(right);
    let scrub = sideways.clamp(-grip * dt, grip * dt);
    car.velocity -= right * scrub;

    // Along the nose: engine, brakes, and everything that slows a car down.
    let forward = car.velocity.dot(heading);
    let mut push = 0.0;
    let mut resistance = 0.0;
    if controls.throttle > 0.0 {
        // Off the road the wheels spin, and a rear that has let go spins too:
        // the engine gets the surface's grip, less what the slide is spending.
        let fade = 1.0 - (forward / h.top_speed).clamp(0.0, 1.0).powi(2);
        push += h.accel * controls.throttle * fade * surface.grip * (1.0 - loose);
    } else if speed > 0.0 && !reversing {
        let revs = (forward.abs() / h.top_speed).clamp(h.engine_braking_floor, 1.0);
        resistance += h.engine_braking * revs;
    }
    if reversing {
        let fade = 1.0 - (-forward / h.reverse_speed).clamp(0.0, 1.0).powi(2);
        push -= h.reverse_accel * controls.brake * fade * surface.grip;
    } else if controls.brake > 0.0 {
        // Independent of the corner. See `Handling::brake`.
        resistance += h.brake * surface.grip * controls.brake;
    }
    let soft = (1.0 - surface.grip).clamp(0.0, 1.0);
    if speed > 0.0 {
        let rolling_drag = h.rolling * (1.0 + h.soft_ground * soft);
        resistance += h.drag * speed * forward.abs() + rolling_drag;
    }
    let climb = -GRAVITY * surface.slope / (1.0 + surface.slope * surface.slope).sqrt();
    car.velocity += heading * ((push + climb) * dt);
    // Resistance can stop motion, never reverse it or kick it across zero.
    let driven = car.velocity.dot(heading);
    car.velocity -= heading * driven.clamp(-resistance * dt, resistance * dt);
    if reversing {
        // Holding reverse also controls a descent. Cancel only the excess speed
        // with the brakes, instead of alternating full reverse and full brake.
        let excess = (-car.velocity.dot(heading) - h.reverse_speed).max(0.0);
        car.velocity += heading * excess.min(h.brake * surface.grip * dt);
    }

    // Two drags against travel rather than against the nose, so a car going
    // sideways is slowed as hard as one going straight. The gravel trap:
    // ploughing through soft ground. And the slide itself: rubber scrubbing
    // across the road instead of rolling along it, from the slip where the
    // tyres start to mark.
    let ploughing = h.off_road_drag * soft.powi(3) * speed;
    let scrubbing =
        h.scrub_drag * ((slip.abs() - MARK_FROM) / (MARK_FULL - MARK_FROM)).clamp(0.0, 1.0);
    let dragging = ploughing + scrubbing;
    if rolling && dragging > 0.0 {
        let slow = (dragging * dt).min(car.velocity.length());
        car.velocity -= car.velocity.normalize_or_zero() * slow;
    }

    // A car braked to a crawl stops, rather than creeping on the last of its
    // rounding error.
    if controls.throttle == 0.0 && !reversing && car.velocity.length() < 0.3 {
        car.velocity = Vec3::ZERO;
        car.yaw_rate = 0.0;
    }

    // Telemetry. What the seat feels is the change in velocity the tyres made,
    // which is the total change less the slope's share.
    let accel = (car.velocity - was) / dt.max(1e-4) - heading * climb;
    car.g_force = Vec2::new(accel.dot(right), accel.dot(heading)) / GRAVITY;
    car.slip_angle = slip;
    car.grip_used = asking;
    car.rear_slip = if speed > 2.0 {
        ((slip.abs() - MARK_FROM) / (MARK_FULL - MARK_FROM))
            .clamp(0.0, 1.0)
            .max(handbrake)
    } else {
        0.0
    };

    yaw
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &Handling = &Handling::SHOOTING_BRAKE;
    const FLAT: Surface = Surface {
        grip: 1.0,
        slope: 0.0,
    };
    const GRASS: Surface = Surface {
        grip: 0.38,
        slope: 0.0,
    };

    #[test]
    fn a_crawl_coasts_to_rest() {
        for speed in [-0.49, -0.35, 0.35, 0.49, 1.0] {
            let mut car = rolling(speed);
            drive(&mut car, Controls::default(), FLAT, 3.0);
            assert_eq!(car.velocity, Vec3::ZERO, "kept creeping from {speed} m/s");
        }
    }

    #[test]
    fn a_light_reverse_pedal_does_not_chatter_at_walking_pace() {
        let mut car = Car::default();
        drive(
            &mut car,
            Controls {
                brake: 0.2,
                ..default()
            },
            FLAT,
            4.0,
        );
        let before = car.speed(Vec3::NEG_Z);
        drive(
            &mut car,
            Controls {
                brake: 0.2,
                ..default()
            },
            FLAT,
            1.0,
        );
        assert!(
            car.speed(Vec3::NEG_Z) < before - 0.1,
            "reverse stuck at {before}"
        );
    }

    #[test]
    fn reversing_at_the_limit_does_not_pulse_the_brakes() {
        let mut car = Car::default();
        drive(
            &mut car,
            Controls {
                brake: 1.0,
                ..default()
            },
            FLAT,
            12.0,
        );
        for _ in 0..240 {
            step(
                &mut car,
                H,
                Vec3::NEG_Z,
                Vec3::X,
                Controls {
                    brake: 1.0,
                    ..default()
                },
                FLAT,
                1.0 / 240.0,
            );
            assert!(
                car.g_force.y.abs() < 0.6,
                "reverse jolted: {:?}",
                car.g_force
            );
        }
    }

    #[test]
    fn a_backward_spin_does_not_engage_reverse_until_stopped() {
        let mut car = rolling(-4.0);
        step(
            &mut car,
            H,
            Vec3::NEG_Z,
            Vec3::X,
            Controls {
                brake: 1.0,
                ..default()
            },
            FLAT,
            1.0 / 240.0,
        );
        assert!(!car.reversing);
        assert!(car.speed(Vec3::NEG_Z) > -4.0);
    }

    #[test]
    fn alignment_follows_the_tail_when_moving_backwards() {
        let mut car = Car {
            velocity: Vec3::new(1.0, 0.0, 6.0),
            ..default()
        };
        let yaw = step(
            &mut car,
            H,
            Vec3::NEG_Z,
            Vec3::X,
            Controls::default(),
            FLAT,
            1.0 / 240.0,
        );
        // Travel is to the right of the tail: positive yaw turns the tail right.
        assert!(yaw > 0.0, "alignment amplified the backward slide");
    }

    #[test]
    fn both_pedals_hold_the_car_still_without_engaging_reverse() {
        let mut car = Car::default();
        drive(
            &mut car,
            Controls {
                throttle: 1.0,
                brake: 1.0,
                ..default()
            },
            FLAT,
            3.0,
        );
        assert_eq!(car.velocity, Vec3::ZERO);
        assert!(!car.reversing);
    }

    #[test]
    fn reverse_stays_limited_on_a_descent() {
        let mut car = Car::default();
        drive(
            &mut car,
            Controls {
                brake: 1.0,
                ..default()
            },
            Surface {
                slope: 0.12,
                ..FLAT
            },
            60.0,
        );
        assert!(
            car.velocity.length() <= H.reverse_speed + 0.05,
            "reverse ran downhill at {} m/s",
            car.velocity.length()
        );
    }

    #[test]
    fn steering_is_mirrored_in_forward_and_reverse_on_every_setup() {
        for setup in super::super::Setup::ALL {
            let h = setup.applied_to(*H);
            for reverse in [false, true] {
                let mut left = Car::default();
                let mut right = Car::default();
                let (mut yaw_l, mut yaw_r) = (0.0_f32, 0.0_f32);
                for _ in 0..2400 {
                    for (car, yaw, steer) in
                        [(&mut left, &mut yaw_l, 0.7), (&mut right, &mut yaw_r, -0.7)]
                    {
                        let heading = Quat::from_rotation_y(*yaw) * Vec3::NEG_Z;
                        *yaw += step(
                            car,
                            &h,
                            heading,
                            heading.cross(Vec3::Y),
                            Controls {
                                throttle: if reverse { 0.0 } else { 0.7 },
                                brake: if reverse { 0.7 } else { 0.0 },
                                steer,
                                handbrake: false,
                            },
                            FLAT,
                            1.0 / 240.0,
                        );
                    }
                    assert!((yaw_l + yaw_r).abs() < 1e-5);
                    assert!(
                        (left.velocity - right.velocity * Vec3::new(-1.0, 1.0, 1.0)).length()
                            < 1e-5
                    );
                }
                assert!(if reverse { yaw_l < -0.1 } else { yaw_l > 0.1 });
            }
        }
    }

    #[test]
    fn mixed_inputs_stay_bounded_across_setups_and_surfaces() {
        let mut seed = 42_u32;
        for setup in super::super::Setup::ALL {
            let h = setup.applied_to(*H);
            let mut car = Car::default();
            let mut yaw = 0.0;
            let mut controls = Controls::default();
            for i in 0..24_000 {
                if i % 60 == 0 {
                    let mut sample = || {
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        (seed >> 8) as f32 / 16_777_215.0
                    };
                    controls = Controls {
                        throttle: sample(),
                        brake: sample(),
                        steer: sample() * 2.0 - 1.0,
                        handbrake: sample() > 0.7,
                    };
                }
                let heading = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
                yaw += step(
                    &mut car,
                    &h,
                    heading,
                    heading.cross(Vec3::Y),
                    controls,
                    Surface {
                        grip: [1.0, 0.85, 0.38][(i / 240) % 3],
                        slope: (i as f32 * 0.001).sin() * 0.12,
                    },
                    1.0 / 240.0,
                );
                assert!(car.velocity.is_finite() && car.g_force.is_finite() && yaw.is_finite());
                assert!(car.velocity.length() < h.top_speed * 1.4);
                assert!(car.g_force.length() < 5.0);
                assert!((0.0..=1.0).contains(&car.rear_slip));
            }
        }
    }

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
            yaw += step(
                car,
                H,
                heading,
                heading.cross(Vec3::Y),
                controls,
                surface,
                dt,
            );
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
            yaw += step(
                &mut car,
                H,
                heading,
                heading.cross(Vec3::Y),
                controls,
                surface,
                dt,
            );
            if car.velocity.length() > 5.0 {
                worst = worst.max(adrift(&car, yaw));
            }
            if i > 30 {
                peak = peak.max(car.g_force.x.abs());
            }
        }
        (worst, peak)
    }

    /// The whole reason `step` is pure: advancing by one big step or by many
    /// small ones that add up to it must land in the same place, or the car
    /// handles differently at different frame rates.
    #[test]
    fn the_result_depends_only_on_the_time_advanced() {
        let corner = Controls {
            steer: 0.8,
            throttle: 0.6,
            ..default()
        };
        let mut coarse = rolling(14.0);
        let mut fine = rolling(14.0);
        let (mut yaw_c, mut yaw_f) = (0.0f32, 0.0f32);
        for _ in 0..60 {
            let heading = Quat::from_rotation_y(yaw_c) * Vec3::NEG_Z;
            yaw_c += step(
                &mut coarse,
                H,
                heading,
                heading.cross(Vec3::Y),
                corner,
                FLAT,
                1.0 / 60.0,
            );
            for _ in 0..4 {
                let heading = Quat::from_rotation_y(yaw_f) * Vec3::NEG_Z;
                yaw_f += step(
                    &mut fine,
                    H,
                    heading,
                    heading.cross(Vec3::Y),
                    corner,
                    FLAT,
                    1.0 / 240.0,
                );
            }
        }
        // Euler integration is not exact, so these will not be identical — but
        // they must be close, and the game substeps at a fixed rate so that in
        // play they are identical.
        assert!(
            (coarse.velocity - fine.velocity).length() < 0.6,
            "coarse {:?} against fine {:?}",
            coarse.velocity,
            fine.velocity
        );
        assert!((yaw_c - yaw_f).abs() < 0.08, "yaw {yaw_c} against {yaw_f}");
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
        assert!(
            (12.0..H.top_speed).contains(&quick),
            "4 s got to {quick} m/s"
        );
        drive(&mut car, gas, FLAT, 20.0);
        let flat_out = car.velocity.length();
        assert!(
            flat_out > quick && flat_out < H.top_speed + 0.5,
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
                peak > 0.8 * H.grip / GRAVITY,
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
            let most = H.grip_at(speed) / GRAVITY;
            assert!(peak <= most + 0.05, "{peak:.2} g from {most:.2} of grip");
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
                    H,
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

    /// Off the road, the ground does the stopping. The old gravel traps ended
    /// laps, and the grass here should cost most of a g at pace whether or not
    /// the driver is braking. A kerb is not the grass, and a car can crawl back.
    #[test]
    fn the_grass_is_a_gravel_trap() {
        let coast = |surface: Surface| {
            let mut car = rolling(16.0);
            drive(&mut car, Controls::default(), surface, 1.0);
            16.0 - car.velocity.length()
        };
        let lost_on_road = coast(FLAT);
        let lost_in_grass = coast(GRASS);
        let lost_on_kerb = coast(Surface {
            grip: 0.72,
            slope: 0.0,
        });
        assert!(
            lost_in_grass > 0.55 * GRAVITY,
            "a second in the grass only shed {:.2} g",
            lost_in_grass / GRAVITY
        );
        // Over and above what lifting off costs on tarmac, which is itself a lot.
        assert!(
            lost_in_grass - lost_on_road > 0.4 * GRAVITY,
            "the grass adds only {:.2} g over tarmac",
            (lost_in_grass - lost_on_road) / GRAVITY
        );
        // Over and above what the engine and the air take on any surface.
        let kerb_extra = lost_on_kerb - lost_on_road;
        let grass_extra = lost_in_grass - lost_on_road;
        assert!(
            kerb_extra < 0.3 * grass_extra,
            "the kerbs punish like the grass: {kerb_extra:.1} against {grass_extra:.1} m/s extra"
        );

        let mut beached = Car::default();
        drive(
            &mut beached,
            Controls {
                throttle: 1.0,
                ..default()
            },
            GRASS,
            2.0,
        );
        assert!(
            beached.velocity.length() > 2.0,
            "stuck in the grass at {:.1} m/s",
            beached.velocity.length()
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
            yaw += step(
                &mut car,
                H,
                heading,
                heading.cross(Vec3::Y),
                Controls::default(),
                FLAT,
                dt,
            );
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
        drive(
            &mut uphill,
            Controls::default(),
            Surface {
                grip: 1.0,
                slope: 0.15,
            },
            2.0,
        );
        let mut downhill = rolling(14.0);
        drive(
            &mut downhill,
            Controls::default(),
            Surface {
                grip: 1.0,
                slope: -0.15,
            },
            2.0,
        );
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
        drive(
            &mut car,
            Controls::default(),
            Surface {
                grip: 1.0,
                slope: -0.2,
            },
            25.0,
        );
        let settled = car.velocity.length();
        let radius = settled * settled / H.grip;
        assert!(
            radius < 22.0,
            "coasts to {settled:.1} m/s, wanting {radius:.0} m of corner"
        );
        // Off the throttle the engine is meant to hold the car on a hill; it is
        // the throttle that turns a descent into speed. So only: still rolling.
        assert!(
            settled > 3.0,
            "the engine stopped the car on a hill: {settled:.1} m/s"
        );
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
    /// and downhill, which is where it happens. On tarmac braking must help; in
    /// the grass the ground does the stopping by itself. The first version of
    /// this model failed all of it: sideways, nothing slowed the car, and the
    /// brake key became the throttle.
    #[test]
    fn a_slide_comes_back_down() {
        let stop = Controls {
            brake: 1.0,
            ..default()
        };
        for surface in [
            FLAT,
            GRASS,
            Surface {
                grip: 0.38,
                slope: -0.16,
            },
        ] {
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
        }
        // On tarmac, compared before the brakes have stopped the car altogether,
        // at which point the held key is reverse and the speed is backwards.
        let (coasting, _) = slide(2.0, Controls::default(), FLAT);
        let (braked, _) = slide(2.0, stop, FLAT);
        assert!(
            braked < coasting - 3.0,
            "braking barely helped on tarmac: {braked:.1} against {coasting:.1}"
        );
        // In the grass the surface stops a sliding car on its own.
        let (in_the_grass, _) = slide(2.0, Controls::default(), GRASS);
        assert!(
            in_the_grass < 8.0,
            "sliding through the grass still held {in_the_grass:.1} m/s"
        );
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
        assert!(
            backwards < H.reverse_speed + 0.1,
            "reverse ran away to {backwards}"
        );
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

    /// The telemetry has to mean what it says.
    #[test]
    fn telemetry_reads_the_corner() {
        let mut car = rolling(14.0);
        drive(
            &mut car,
            Controls {
                steer: 1.0,
                throttle: 0.3,
                ..default()
            },
            FLAT,
            1.5,
        );
        assert!(
            car.slip_angle > 0.0,
            "a left turn travels right of the nose: {}",
            car.slip_angle
        );
        assert!(
            car.grip_used > 0.5,
            "full lock is not using the grip: {}",
            car.grip_used
        );
        let mut straight = rolling(14.0);
        drive(&mut straight, Controls::default(), FLAT, 0.5);
        assert!(straight.slip_angle.abs() < 0.01 && straight.grip_used < 0.05);
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
            yaw += step(
                &mut car,
                H,
                heading,
                heading.cross(Vec3::Y),
                controls,
                surface,
                dt,
            );
            assert!(car.velocity.is_finite(), "velocity blew up at step {i}");
            assert!(car.yaw_rate.is_finite(), "yaw blew up at step {i}");
            assert!(car.g_force.is_finite(), "g-force blew up at step {i}");
            assert!(
                car.velocity.length() < H.top_speed * 1.4,
                "speed ran away to {} at step {i}",
                car.velocity.length()
            );
        }
    }
}
