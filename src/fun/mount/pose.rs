//! Where every moving part of a mount goes, for a pose.

use bevy::prelude::*;

use super::{Part, Pose, Spring};

/// Where `part` goes for this `pose`. `rest` is where it was built, and the
/// spring, for the parts that have one, is what gives them their sway.
pub(super) fn part(
    part: Part,
    rest: &Transform,
    pose: &Pose,
    spring: Option<Mut<Spring>>,
    dt: f32,
) -> Transform {
    let mut out = *rest;
    let run = pose.run;
    let s = pose.stride;
    let air = (pose.lift / 0.12).clamp(0.0, 1.0);
    let t = pose.time;
    match part {
        Part::Hip(_, phase) => {
            let swing = 0.66 * run * (s + phase).sin();
            let swing =
                swing * (1.0 - air) + 0.75 * air * (1.0 + 0.25 * (pose.flap_phase + phase).sin());
            out.rotation = Quat::from_rotation_x(swing);
        }
        Part::Knee(_, phase) => {
            let bend = run * (s + phase + 1.1).sin().max(0.0) * 1.1;
            out.rotation = Quat::from_rotation_x(-(bend * (1.0 - air) + 1.1 * air));
        }
        Part::Ankle(_, phase) => {
            let swing = 0.66 * run * (s + phase).sin();
            let bend = run * (s + phase + 1.1).sin().max(0.0) * 1.1;
            // Keep the foot level, and point the toes when there is nothing
            // under them.
            let level = -(swing - bend);
            out.rotation = Quat::from_rotation_x(level * (1.0 - air) + 0.9 * air);
        }
        Part::Neck => {
            let thrust = -0.13 * run * (s).sin();
            let pitch = -0.30 - 1.30 * pose.peck + 0.62 * pose.honk - 0.55 * pose.boost
                + pose.lean.y * 0.10;
            out.rotation = Quat::from_rotation_x(pitch);
            out.translation = rest.translation
                + Vec3::new(
                    0.0,
                    0.03 * run * (2.0 * s).cos(),
                    thrust - 0.18 * pose.boost,
                );
        }
        Part::Head => {
            // The head stays level whatever the neck does, and looks where the
            // corner is going.
            let neck = -0.30 - 1.30 * pose.peck + 0.62 * pose.honk - 0.55 * pose.boost;
            out.rotation = Quat::from_rotation_y(-pose.steer * 1.4)
                * Quat::from_rotation_z(-pose.lean.x * 0.12)
                * Quat::from_rotation_x(-neck * 0.85 + 0.05 * (t * 1.7).sin());
        }
        Part::Jaw => {
            out.rotation = Quat::from_rotation_x(-0.85 * pose.beak.max(0.35 * pose.boost));
        }
        Part::Comb | Part::Wattle => {
            if let Some(mut spring) = spring {
                let drive = Vec3::new(
                    -pose.lean.x * 0.35,
                    0.0,
                    pose.lean.y * 0.25 + 0.12 * run * (2.0 * s).cos(),
                );
                spring.chase(drive, 140.0, 7.0, dt);
                out.rotation =
                    Quat::from_rotation_z(spring.pos.x) * Quat::from_rotation_x(spring.pos.z);
            }
        }
        Part::Pupil(_) => {
            if let Some(mut spring) = spring {
                // Loose pupils hang at the bottom of the eye and swing out
                // against every corner, bump and honk.
                let drive = Vec3::new(
                    -pose.lean.x * 0.045 + 0.02 * run * (2.0 * s).sin(),
                    -0.05 + pose.lean.y * 0.02 + 0.03 * run * (2.0 * s).cos() + 0.05 * pose.honk,
                    0.0,
                );
                spring.chase(drive, 190.0, 5.5, dt);
                let flat = Vec2::new(spring.pos.x, spring.pos.y).clamp_length_max(0.05);
                out.translation = rest.translation + Vec3::new(flat.x, flat.y, 0.0);
            }
        }
        Part::Wing(side) => {
            let idle = 0.10 + 0.02 * (t * 1.3 + side).sin();
            let flap = pose.flap_phase.sin();
            let spread = idle + pose.flap * (0.85 + 0.55 * flap) + 0.9 * pose.honk;
            let spread = spread.clamp(0.04, 1.9);
            out.rotation = Quat::from_rotation_z(side * spread);
        }
        Part::Feather(i) => {
            let f = f32::from(i);
            let sway = 0.10 * (t * 6.0 + f * 0.8).sin() * (0.25 + run) + pose.lean.y * 0.08;
            let fan = 1.0 + 0.55 * pose.boost + 0.3 * pose.honk;
            let spread = (f - 3.0) * 0.30 * (fan - 1.0);
            out.rotation = rest.rotation
                * Quat::from_rotation_z(spread)
                * Quat::from_rotation_x(sway - 0.25 * pose.boost);
        }
        Part::Tail => {
            out.rotation = Quat::from_rotation_x(-pose.lean.y * 0.25 + 0.25 * air);
        }
        Part::Rider => {
            let forward = (pose.speed / 30.0).clamp(-0.3, 1.0);
            let bounce = 0.045 * run * (2.0 * s).cos().abs();
            out.translation = rest.translation + Vec3::new(0.0, bounce, 0.0);
            out.rotation = Quat::from_rotation_x(-0.10 * forward - pose.lean.y * 0.16)
                * Quat::from_rotation_z(-pose.lean.x * 0.26);
        }
        Part::RiderHead => {
            if let Some(mut spring) = spring {
                let drive = Vec3::new(pose.lean.x * 0.3, 0.0, -pose.lean.y * 0.3);
                spring.chase(drive, 90.0, 8.0, dt);
                out.rotation = Quat::from_rotation_y(-pose.steer * 1.6)
                    * Quat::from_rotation_z(spring.pos.x)
                    * Quat::from_rotation_x(spring.pos.z);
            }
        }
        Part::Hat => {
            if let Some(mut spring) = spring {
                let drive = Vec3::new(
                    pose.lean.x * 0.55,
                    (2.0 * s).cos() * run * 0.06 + pose.rise.clamp(-8.0, 8.0) * 0.012,
                    -pose.lean.y * 0.45 + 0.4 * pose.boost,
                );
                spring.chase(drive, 110.0, 5.0, dt);
                out.rotation =
                    Quat::from_rotation_z(-spring.pos.x) * Quat::from_rotation_x(spring.pos.z);
                out.translation = rest.translation + Vec3::new(0.0, spring.pos.y.max(-0.05), 0.0);
            }
        }
        Part::Spin(rate) => {
            let speed = 6.0 + pose.speed.abs() * 0.5 + 30.0 * pose.boost;
            out.rotation = Quat::from_rotation_y(t * speed * rate);
        }
        Part::Scarf(i) => {
            // Hangs down when there is no wind, streams straight back when
            // there is, and flutters at its own pace along its length.
            let f = f32::from(i);
            let wind = (pose.wind.z.abs()).clamp(0.0, 1.0);
            let pitch = 0.30 * (1.0 - wind) - 0.05 * wind
                + 0.18 * (t * 17.0 - f * 0.9).sin() * (0.25 + wind);
            let sweep = pose.wind.x * 0.40 + 0.12 * (t * 11.0 - f * 1.1).sin() * wind;
            out.rotation = Quat::from_rotation_y(sweep) * Quat::from_rotation_x(pitch);
        }
        Part::Wheel(steers) => {
            let steer = if steers > 0.5 { pose.steer } else { 0.0 };
            out.rotation =
                rest.rotation * Quat::from_rotation_y(steer) * Quat::from_rotation_x(pose.wheel);
        }
        Part::Chassis => {
            let jolt = 0.02 * run * (2.0 * s).sin() + pose.squash * 0.0;
            out.translation = rest.translation + Vec3::new(0.0, jolt, 0.0);
        }
        Part::Slosh => {
            let wobble = 0.03 * (t * 5.0).sin() + pose.lean.x * 0.05;
            out.rotation =
                Quat::from_rotation_z(wobble) * Quat::from_rotation_x(pose.lean.y * 0.05);
        }
    }
    out
}
