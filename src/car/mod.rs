//! The car: the glTF, the wheels, and the keys that reach [`physics`].
//!
//! This module only turns input and the road under the wheels into a [`Controls`]
//! and a [`Surface`], hands both to one `step`, and puts the result back on the
//! transform. Nothing here decides how the car behaves.

mod physics;

use bevy::{prelude::*, world_serialization::WorldInstanceReady};

use crate::track::Track;
use crate::Reset;
use physics::{Controls, Surface};
pub(crate) use physics::{Car, HALF_TRACK, REAR_AXLE, SCALE, WHEEL_WIDTH};

const MODEL: &str = "models/shooting_brake.glb";
/// A long frame must not let the car tunnel through a corner.
const MAX_STEP: f32 = 1.0 / 30.0;

#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct DriveSet;

pub struct CarPlugin;

impl Plugin for CarPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, drive.in_set(DriveSet))
            .add_systems(Update, turn_wheels.after(DriveSet));
    }
}

/// A wheel of the glTF, and how it is allowed to move.
#[derive(Component)]
struct Wheel {
    /// Front wheels follow the steering; rear wheels only roll.
    steers: bool,
    /// The pose the model was exported in, which both motions build on.
    rest: Quat,
    roll: f32,
}

fn setup(mut commands: Commands, track: Res<Track>, asset_server: Res<AssetServer>) {
    commands
        .spawn((
            Car::default(),
            track.start_transform().with_scale(Vec3::splat(SCALE)),
            Visibility::default(),
            WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(MODEL))),
        ))
        .observe(attach_wheels);
}

fn attach_wheels(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    names: Query<&Name>,
    transforms: Query<&Transform>,
) {
    for entity in children.iter_descendants(ready.entity) {
        let Ok(name) = names.get(entity) else {
            continue;
        };
        // `make_shooting_brake.py` names the hubs WheelFL, WheelFR, WheelRL, WheelRR.
        let Some(corner) = name.as_str().strip_prefix("Wheel") else {
            continue;
        };
        let rest = transforms.get(entity).map(|t| t.rotation).unwrap_or_default();
        commands.entity(entity).insert(Wheel {
            steers: corner.starts_with('F'),
            rest,
            roll: 0.0,
        });
    }
}

fn drive(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    track: Res<Track>,
    mut reset: MessageWriter<Reset>,
    mut cars: Query<(&mut Transform, &mut Car)>,
) {
    let dt = time.delta_secs().min(MAX_STEP);
    let controls = Controls {
        throttle: axis(&keys, KeyCode::KeyW, KeyCode::ArrowUp),
        // Shift still brakes, for anyone who learned it that way.
        brake: axis(&keys, KeyCode::KeyS, KeyCode::ArrowDown)
            .max(axis(&keys, KeyCode::ShiftLeft, KeyCode::ShiftRight)),
        steer: axis(&keys, KeyCode::KeyA, KeyCode::ArrowLeft)
            - axis(&keys, KeyCode::KeyD, KeyCode::ArrowRight),
        handbrake: keys.pressed(KeyCode::Space),
    };

    if keys.just_pressed(KeyCode::KeyR) {
        reset.write(Reset);
        for (mut transform, mut car) in &mut cars {
            *transform = track.start_transform().with_scale(transform.scale);
            *car = Car::default();
        }
        return;
    }

    for (mut transform, mut car) in &mut cars {
        let heading = level(*transform.forward());
        let right = heading.cross(Vec3::Y);
        let ground = track.ground(transform.translation);
        let surface = Surface {
            grip: ground.grip,
            // The grade runs along the circuit; the car gets the component of it
            // that lies along its nose. Projecting rather than taking the sign
            // matters once the car is sideways: at ninety degrees to the road the
            // sign flips on nothing at all, and gravity would slam back and forth
            // frame to frame.
            slope: ground.slope * ground.tangent.dot(heading),
        };
        let yaw = physics::step(&mut car, heading, right, controls, surface, dt);
        transform.rotate_y(yaw);
        transform.translation += car.velocity * dt;
    }
}

/// Point the front wheels where the steering does, and roll all four at the
/// speed the car is actually travelling.
fn turn_wheels(
    time: Res<Time>,
    cars: Query<(Entity, &Transform, &Car)>,
    children: Query<&Children>,
    mut wheels: Query<(&mut Transform, &mut Wheel), Without<Car>>,
) {
    let dt = time.delta_secs();
    for (entity, transform, car) in &cars {
        // Forward is -Z, so a wheel rolling the car forward turns the other way.
        let roll = -car.speed(level(*transform.forward())) / physics::WHEEL_RADIUS * dt;
        for descendant in children.iter_descendants(entity) {
            let Ok((mut transform, mut wheel)) = wheels.get_mut(descendant) else {
                continue;
            };
            wheel.roll += roll;
            let steer = if wheel.steers { car.steer_angle } else { 0.0 };
            transform.rotation =
                wheel.rest * Quat::from_rotation_y(steer) * Quat::from_rotation_x(wheel.roll);
        }
    }
}

/// Flatten a direction into the XZ plane. The car drives on the loft's surface
/// but its own frame stays level, so gravity is the only thing a slope changes.
pub(crate) fn level(direction: Vec3) -> Vec3 {
    Vec3::new(direction.x, 0.0, direction.z).normalize_or(Vec3::NEG_Z)
}

fn axis(keys: &ButtonInput<KeyCode>, a: KeyCode, b: KeyCode) -> f32 {
    if keys.any_pressed([a, b]) { 1.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    //! Can the car get round the circuit? Everything else is a number in
    //! isolation; this is the one that answers whether the thing is driveable.

    use super::*;
    use crate::track::Track;

    /// Two drivers round the real circuit, through the real physics and the
    /// real barriers.
    ///
    /// The plain one aims at the centreline with proportional steering and
    /// carries the speed the tyres can hold through the tightest bend it can
    /// see a long way ahead. If it cannot get round, neither can anyone.
    ///
    /// The clumsy one is a person on a keyboard: full lock or nothing, a
    /// reaction time, a short look up the road, and brakes that go on late. If
    /// *it* gets round, the car is easy to learn — and that is the test that
    /// matters, because the plain driver was lapping happily while the person
    /// holding the keys was not.
    ///
    /// Neither is a tuning oracle. Chasing their numbers rewards a slow, dull car.
    fn drive_one_lap(seconds: f32, clumsy: bool) -> Lap {
        use std::collections::VecDeque;
        let track = Track::new();
        let mut transform = track.start_transform().with_scale(Vec3::splat(SCALE));
        let mut car = Car::default();
        let dt = 1.0 / 120.0;
        let mut lap = Lap::default();
        let mut travelled = 0.0f32;
        // What the clumsy driver is reacting to: the world as it was 150 ms ago.
        let mut seen: VecDeque<(f32, f32, f32)> = VecDeque::new();
        let reaction = if clumsy { 18 } else { 0 };
        let lookahead = if clumsy { 6 } else { 16 };
        let late = if clumsy { 1.15 } else { 1.04 };

        for _ in 0..(seconds / dt) as usize {
            let heading = level(*transform.forward());
            let ground = track.ground(transform.translation);
            let ahead = ground.tangent;
            let astray = f32::atan2(heading.cross(ahead).y, heading.dot(ahead));
            let correction = astray * 1.6 + ground.lateral * 0.16;

            let hold = 0.95 * 9.81 * ground.grip;
            let downhill = (-ground.slope * ground.tangent.dot(heading) * 9.81).max(0.0);
            let stopping = (hold - downhill).max(hold * 0.4);
            let mut limit: f32 = 24.0;
            for step in 0..=lookahead {
                let reach = step as f32 * 3.0;
                let probe = track.ground(transform.translation + ahead * reach);
                let corner = hold / probe.curvature.abs().max(0.002);
                limit = limit.min((corner + 2.0 * stopping * reach).sqrt());
            }
            let speed = car.velocity.length();

            seen.push_back((correction, speed, limit));
            let (correction, seen_speed, limit) = if seen.len() > reaction {
                seen.pop_front().unwrap()
            } else {
                (correction, speed, limit)
            };

            let controls = if clumsy {
                Controls {
                    throttle: if seen_speed > limit * late { 0.0 } else { 1.0 },
                    brake: if seen_speed > limit * late { 1.0 } else { 0.0 },
                    steer: if correction > 0.03 {
                        1.0
                    } else if correction < -0.03 {
                        -1.0
                    } else {
                        0.0
                    },
                    handbrake: false,
                }
            } else {
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
            };

            let surface = Surface {
                grip: ground.grip,
                slope: ground.slope * ground.tangent.dot(heading),
            };
            let yaw = physics::step(&mut car, heading, heading.cross(Vec3::Y), controls, surface, dt);
            transform.rotate_y(yaw);
            transform.translation += car.velocity * dt;
            travelled += speed * dt;

            track.hold(&mut transform, &mut car, dt);
            let ground = track.ground(transform.translation);
            if ground.lateral.abs() > 4.0 {
                lap.off_road += dt;
            }
            if ground.lateral.abs() > 5.5 {
                lap.in_the_weeds += dt;
            }
            let heading = level(*transform.forward());
            if ground.slope * ground.tangent.dot(heading) < -0.04 {
                lap.descending += dt;
                if ground.lateral.abs() > 4.0 {
                    lap.off_road_descending += dt;
                }
            }
            if speed < 1.5 {
                lap.stopped += dt;
            }
            lap.distance = travelled;
            lap.progress = lap.progress.max(track.progress(transform.translation));
        }
        lap
    }

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

    /// Full lock or nothing, 150 ms behind, brakes late, cannot see far. This is
    /// the person holding the keys, and the car has to be drivable by them.
    #[test]
    fn a_clumsy_driver_still_gets_round() {
        let lap = drive_one_lap(90.0, true);
        report("clumsy", &lap);
        assert!(lap.progress > 0.9, "90 s only got {:.0}% round", lap.progress * 100.0);
        // Loose: the grass is a gravel trap now, so every excursion this driver
        // makes is a slow one, and it makes plenty. What matters is that it is
        // never stuck out there.
        assert!(lap.off_road < 45.0, "off the road {:.0} s of 90", lap.off_road);
        assert!(lap.stopped < 20.0, "going nowhere {:.0} s of 90", lap.stopped);
    }

    #[test]
    fn a_plain_driver_gets_round() {
        let lap = drive_one_lap(90.0, false);
        report("plain", &lap);
        assert!(
            lap.progress > 0.9,
            "90 s only got {:.0}% round, {:.0} m",
            lap.progress * 100.0,
            lap.distance
        );
        // Loose on purpose: these are the bounds of "a plain driver can get
        // round", not a target to tune against.
        assert!(
            lap.off_road < 30.0,
            "spent {:.0} s of 90 off the road",
            lap.off_road
        );
        assert!(
            lap.stopped < 15.0,
            "spent {:.0} s of 90 going nowhere",
            lap.stopped
        );
    }
}

#[cfg(test)]
mod analysis {
    use super::*;
    use crate::track::Track;

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

        println!("{:>5} {:>5} {:>6} {:>7} {:>7} {:>8} {:>7} {:>8}",
            "grip", "top", "lap s", "flatout", "braking", "spread", "slowest", "90%loss");
        for grip in [1.00f32, physics::GRIP / 9.81, 1.7] {
        for top in [18.0f32, 21.0, 24.0, 28.0] {
        let engine = 5.0f32;
            let lat = grip * 9.81;
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
            let time: f32 = v.iter().map(|s| step / s).sum();
            let flat_out = v.iter().filter(|s| **s >= top - 0.2).count();
            let corner = (0..n)
                .filter(|&i| v[i] <= (lat * radii[i]).sqrt() + 0.2 && v[i] < top - 0.2)
                .count();
            let braking = (0..n)
                .filter(|&i| v[(i + 1) % n] < v[i] - 0.05)
                .count();
            let mut sorted = v.clone();
            sorted.sort_by(f32::total_cmp);
            // What a driver using only 90% of the grip loses over a lap. Small
            // and there is nothing to master; huge and a mistake ends the lap.
            let sloppy: f32 = {
                let lat = 0.9 * grip * 9.81;
                let mut w: Vec<f32> = radii.iter().map(|r| (lat * r).sqrt().min(top)).collect();
                for _ in 0..3 {
                    for i in 0..n {
                        let j = (i + 1) % n;
                        w[j] = w[j].min((w[i] * w[i] + 2.0 * engine * step).sqrt());
                    }
                    for i in (0..n).rev() {
                        let j = (i + 1) % n;
                        w[i] = w[i].min((w[j] * w[j] + 2.0 * lat * step).sqrt());
                    }
                }
                w.iter().map(|s| step / s).sum()
            };
            let _ = corner;
            println!(
                "{grip:5.2} {top:5.0} {time:6.1} {:6.0}% {:6.0}% {:7.1}x {:7.1} {:+7.2}s",
                100.0 * flat_out as f32 / n as f32,
                100.0 * braking as f32 / n as f32,
                sorted[n - 1] / sorted[0],
                sorted[0],
                sloppy - time,
            );
        }}
    }
}
