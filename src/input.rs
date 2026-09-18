//! Who is driving the player's car, and how.
//!
//! Keys and a gamepad both end in the same [`Controls`]; the car does not know
//! or care which. When both are speaking, the stronger one is heard per channel,
//! so a pad on the desk does not silence the keyboard and a nudged stick does
//! not override a held key. The stick and the triggers are analogue, which on
//! this car is the difference between full lock and the lock you meant.

use bevy::prelude::*;

use crate::Reset;
use crate::car::{Controls, Player, Setup};
use crate::pause::{HaltSet, running};

/// Everything that reads the player runs in here, ahead of the car.
#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct InputSet;

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        // After the pause, which is what decides whether the driver is being
        // listened to at all: while the game is stopped nothing they press
        // reaches the car, and the frame it starts again is a frame the keys
        // are read on.
        app.add_systems(
            PreUpdate,
            read.in_set(InputSet)
                .after(bevy::input::InputSystems)
                .after(HaltSet)
                .run_if(running),
        );
    }
}

/// A stick this far from centre is resting, not steering.
const DEADZONE: f32 = 0.12;

fn read(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut reset: MessageWriter<Reset>,
    mut chosen: ResMut<Setup>,
    mut players: Query<&mut Controls, With<Player>>,
) {
    // The setup slider. Number keys pick a notch outright; the pad slides along
    // it. Only written when it actually moves, so the car is not re-leaned every
    // frame the key is held.
    let mut wanted = *chosen;
    for (key, notch) in [
        (KeyCode::Digit1, Setup::Understeer),
        (KeyCode::Digit2, Setup::Balanced),
        (KeyCode::Digit3, Setup::Oversteer),
    ] {
        if keys.just_pressed(key) {
            wanted = notch;
        }
    }

    let mut asked = Controls {
        throttle: held(&keys, [KeyCode::KeyW, KeyCode::ArrowUp]),
        // Shift still brakes, for anyone who learned it that way.
        brake: held(
            &keys,
            [
                KeyCode::KeyS,
                KeyCode::ArrowDown,
                KeyCode::ShiftLeft,
                KeyCode::ShiftRight,
            ],
        ),
        steer: held(&keys, [KeyCode::KeyA, KeyCode::ArrowLeft])
            - held(&keys, [KeyCode::KeyD, KeyCode::ArrowRight]),
        handbrake: keys.pressed(KeyCode::Space),
    };
    let mut restart = keys.just_pressed(KeyCode::KeyR);

    for pad in &pads {
        // Stick right is steer right, which is negative here. Past the deadzone
        // the travel is rescaled so full stick is still full lock.
        let stick = -pad.left_stick().x;
        let steer = if stick.abs() > DEADZONE {
            (stick - DEADZONE * stick.signum()) / (1.0 - DEADZONE)
        } else {
            0.0
        };
        if steer.abs() > asked.steer.abs() {
            asked.steer = steer;
        }
        asked.throttle = asked
            .throttle
            .max(pad.get(GamepadButton::RightTrigger2).unwrap_or(0.0));
        asked.brake = asked
            .brake
            .max(pad.get(GamepadButton::LeftTrigger2).unwrap_or(0.0));
        asked.handbrake |= pad.pressed(GamepadButton::South);
        restart |= pad.just_pressed(GamepadButton::Start);
        if pad.just_pressed(GamepadButton::DPadLeft) {
            wanted = wanted.slid(-1);
        }
        if pad.just_pressed(GamepadButton::DPadRight) {
            wanted = wanted.slid(1);
        }
    }

    if wanted != *chosen {
        *chosen = wanted;
    }

    if restart {
        reset.write(Reset);
    }
    for mut controls in &mut players {
        *controls = asked;
    }
}

fn held<const N: usize>(keys: &ButtonInput<KeyCode>, any: [KeyCode; N]) -> f32 {
    if keys.any_pressed(any) { 1.0 } else { 0.0 }
}
