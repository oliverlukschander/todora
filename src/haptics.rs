//! What the pad feels: the road under the tyres, and the moments that matter.
//!
//! Two kinds of rumble. Continuous ones follow the car — a low buzz on the
//! grass, a lighter one on a kerb, and the weak motor rising as the tyres give
//! up grip — sent as short pulses renewed a few times a second while they last,
//! so nothing is left running if the frame rate drops. Pulses mark events: GO,
//! a hit on the wall, and a new best. Everything stops the moment the game is
//! paused, and nothing is sent with **Rumble** off in the settings.
//!
//! Bevy passes these to gilrs, which rumbles pads on Linux and Windows; on
//! macOS support depends on the pad and the system, and a pad that cannot
//! rumble simply ignores it.

use std::time::Duration;

use bevy::input::gamepad::{GamepadRumbleIntensity, GamepadRumbleRequest};
use bevy::prelude::*;

use crate::car::{Car, Player};
use crate::pause::Halt;
use crate::settings::Settings;
use crate::track::Track;

/// How long each continuous pulse lasts, and how often it is renewed.
const PULSE: Duration = Duration::from_millis(140);
const RENEW: f32 = 0.1;
/// A drop in speed this sharp in one frame is the wall.
const HIT: f32 = 3.5;

pub struct HapticsPlugin;

impl Plugin for HapticsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, feel);
    }
}

/// What the car is doing, as the two motors should feel it.
pub(crate) fn road(
    grip: f32,
    lateral: f32,
    grip_used: f32,
    rear_slip: f32,
    speed: f32,
) -> GamepadRumbleIntensity {
    let moving = (speed / 4.0).clamp(0.0, 1.0);
    let grass = if grip < 0.6 { 0.35 } else { 0.0 };
    let kerb = if (crate::track::ROAD_HALF - 0.3..crate::track::ROAD_HALF).contains(&lateral.abs())
    {
        0.18
    } else {
        0.0
    };
    let slip = ((grip_used - 0.95) / 0.5)
        .clamp(0.0, 1.0)
        .max(rear_slip.clamp(0.0, 1.0));
    GamepadRumbleIntensity {
        strong_motor: (grass + kerb) * moving,
        weak_motor: 0.5 * slip * moving,
    }
}

#[allow(clippy::too_many_arguments)]
fn feel(
    time: Res<Time>,
    halt: Res<Halt>,
    settings: Option<Res<Settings>>,
    track: Res<Track>,
    pads: Query<Entity, With<Gamepad>>,
    cars: Query<(&Car, &Transform), With<Player>>,
    mut lights: MessageReader<crate::countdown::StartLight>,
    mut cues: MessageReader<crate::sound::Cue>,
    mut rumble: MessageWriter<GamepadRumbleRequest>,
    mut state: Local<(f32, f32, bool)>,
) {
    let (renew, last_speed, stopped) = &mut *state;
    let on = settings.is_none_or(|s| s.rumble);
    let pulse =
        |rumble: &mut MessageWriter<GamepadRumbleRequest>, strong: f32, weak: f32, ms: u64| {
            for gamepad in &pads {
                rumble.write(GamepadRumbleRequest::Add {
                    duration: Duration::from_millis(ms),
                    intensity: GamepadRumbleIntensity {
                        strong_motor: strong,
                        weak_motor: weak,
                    },
                    gamepad,
                });
            }
        };
    if halt.stopped() || !on {
        if !*stopped {
            for gamepad in &pads {
                rumble.write(GamepadRumbleRequest::Stop { gamepad });
            }
            *stopped = true;
        }
        lights.clear();
        cues.clear();
        return;
    }
    *stopped = false;
    if lights.read().any(|light| light.go) {
        pulse(&mut rumble, 0.6, 0.3, 180);
    }
    if cues.read().any(|cue| *cue == crate::sound::Cue::Best) {
        pulse(&mut rumble, 0.4, 0.7, 450);
    }
    let Ok((car, at)) = cars.single() else {
        return;
    };
    let speed = car.velocity.length();
    if *last_speed - speed > HIT {
        pulse(&mut rumble, 0.9, 0.4, 220);
    }
    *last_speed = speed;
    *renew -= time.delta_secs();
    if *renew > 0.0 {
        return;
    }
    *renew = RENEW;
    let ground = track.ground_from(at.translation, car.along);
    let feel = road(
        ground.grip,
        ground.lateral,
        car.grip_used,
        car.rear_slip,
        speed,
    );
    if feel.strong_motor > 0.01 || feel.weak_motor > 0.01 {
        pulse(
            &mut rumble,
            feel.strong_motor,
            feel.weak_motor,
            PULSE.as_millis() as u64,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_road_is_felt_on_grass_kerbs_and_at_the_limit_and_not_at_rest() {
        let tarmac = road(1.0, 0.0, 0.3, 0.0, 20.0);
        assert_eq!((tarmac.strong_motor, tarmac.weak_motor), (0.0, 0.0));
        assert!(road(0.4, 3.0, 0.3, 0.0, 20.0).strong_motor > 0.3, "grass");
        let kerb = road(1.0, crate::track::ROAD_HALF - 0.1, 0.3, 0.0, 20.0);
        assert!(kerb.strong_motor > 0.1 && kerb.strong_motor < 0.3, "kerb");
        assert!(road(1.0, 0.0, 1.3, 0.0, 20.0).weak_motor > 0.3, "sliding");
        assert_eq!(
            road(0.4, 3.0, 1.3, 1.0, 0.0).strong_motor,
            0.0,
            "standing still"
        );
    }
}
