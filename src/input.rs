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
        // Bevy's RightTrigger is the bumper (RB); RightTrigger2 is RT.
        restart |= pad.just_pressed(GamepadButton::RightTrigger);
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
            .max(pad.get(GamepadButton::RightTrigger2).unwrap_or(0.0))
            .max(if pad.pressed(GamepadButton::South) {
                1.0
            } else {
                0.0
            });
        asked.brake = asked
            .brake
            .max(pad.get(GamepadButton::LeftTrigger2).unwrap_or(0.0))
            .max(if pad.pressed(GamepadButton::West) {
                1.0
            } else {
                0.0
            });
        asked.handbrake |= pad.pressed(GamepadButton::East);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pause::Halt;

    #[test]
    fn right_bumper_resets_once_per_press_and_only_while_driving() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Setup>()
            .init_resource::<Halt>()
            .add_message::<Reset>()
            .add_plugins(InputPlugin);
        let mut pad = Gamepad::default();
        pad.digital_mut().press(GamepadButton::RightTrigger);
        let controller = app.world_mut().spawn(pad).id();
        app.update();
        assert_eq!(app.world().resource::<Messages<Reset>>().len(), 1);
        app.world_mut().resource_mut::<Messages<Reset>>().clear();
        app.world_mut()
            .get_mut::<Gamepad>(controller)
            .unwrap()
            .digital_mut()
            .clear();
        app.update();
        assert!(
            app.world().resource::<Messages<Reset>>().is_empty(),
            "holding RB repeated the reset"
        );
        for halt in [Halt::Pause, Halt::Menu, Halt::Nothing] {
            app.insert_resource(halt);
            let mut pad = app.world_mut().get_mut::<Gamepad>(controller).unwrap();
            pad.digital_mut().release(GamepadButton::RightTrigger);
            pad.digital_mut().press(GamepadButton::RightTrigger);
            app.update();
            assert_eq!(
                app.world().resource::<Messages<Reset>>().len(),
                usize::from(halt == Halt::Nothing)
            );
        }
    }

    #[test]
    fn xbox_face_buttons_drive_without_handbraking_or_restarting() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Setup>()
            .init_resource::<Halt>()
            .add_message::<Reset>()
            .add_plugins(InputPlugin);
        let player = app.world_mut().spawn((Player, Controls::default())).id();
        let mut pad = Gamepad::default();
        pad.digital_mut().press(GamepadButton::South);
        pad.digital_mut().press(GamepadButton::West);
        pad.digital_mut().press(GamepadButton::Start);
        let controller = app.world_mut().spawn(pad).id();
        app.update();
        let controls = app.world().get::<Controls>(player).unwrap();
        assert_eq!(controls.throttle, 1.0);
        assert_eq!(controls.brake, 1.0);
        assert!(!controls.handbrake);
        assert_eq!(app.world().resource::<Messages<Reset>>().len(), 0);

        app.world_mut()
            .get_mut::<Gamepad>(controller)
            .unwrap()
            .digital_mut()
            .release_all();
        app.update();
        assert_eq!(
            *app.world().get::<Controls>(player).unwrap(),
            Controls::default()
        );
    }

    #[test]
    fn stick_vertical_never_drives_and_horizontal_keeps_analogue_steering() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Setup>()
            .init_resource::<Halt>()
            .add_message::<Reset>()
            .add_plugins(InputPlugin);
        let player = app.world_mut().spawn((Player, Controls::default())).id();
        let controller = app.world_mut().spawn(Gamepad::default()).id();
        for y in [-1.0, 1.0] {
            app.world_mut()
                .get_mut::<Gamepad>(controller)
                .unwrap()
                .analog_mut()
                .set(GamepadAxis::LeftStickY, y);
            app.update();
            assert_eq!(
                *app.world().get::<Controls>(player).unwrap(),
                Controls::default()
            );
        }
        for (x, wanted) in [(0.05, 0.0), (0.56, -0.5), (-1.0, 1.0)] {
            app.world_mut()
                .get_mut::<Gamepad>(controller)
                .unwrap()
                .analog_mut()
                .set(GamepadAxis::LeftStickX, x);
            app.update();
            assert!((app.world().get::<Controls>(player).unwrap().steer - wanted).abs() < 0.001);
        }
        // Disconnecting cannot leave steering or a pedal latched.
        app.world_mut().despawn(controller);
        app.update();
        assert_eq!(
            *app.world().get::<Controls>(player).unwrap(),
            Controls::default()
        );
    }
}
