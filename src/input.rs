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
        app.init_resource::<LastDevice>()
            .add_systems(PreUpdate, notice.after(bevy::input::InputSystems));
        app.add_systems(
            PreUpdate,
            read.in_set(InputSet)
                .after(bevy::input::InputSystems)
                .after(HaltSet)
                .run_if(running),
        );
    }
}

/// What the player touched last, so hints can show the right buttons.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum LastDevice {
    #[default]
    Keyboard,
    Pad,
}

/// Any key makes it the keyboard; any pad button or a deliberate stick tilt
/// makes it the pad. Only written when it changes.
fn notice(keys: Res<ButtonInput<KeyCode>>, pads: Query<&Gamepad>, mut last: ResMut<LastDevice>) {
    let pad = pads
        .iter()
        .any(|pad| pad.get_just_pressed().next().is_some() || pad.left_stick().length() > 0.5);
    let wanted = if keys.get_just_pressed().next().is_some() {
        LastDevice::Keyboard
    } else if pad {
        LastDevice::Pad
    } else {
        return;
    };
    last.set_if_neq(wanted);
}

/// A stick this far from centre is resting, not steering, unless the
/// settings say otherwise.
const DEADZONE: f32 = 0.12;

/// A stick's steering for a deflection of `stick`: nothing inside the dead
/// zone, then the rest of the travel rescaled so full stick is still full lock,
/// through a curve the sensitivity bends. Above 1 the car answers more near the
/// centre; below 1 less. Either way the ends stay where they were.
pub(crate) fn stick_steer(stick: f32, deadzone: f32, sensitivity: f32) -> f32 {
    if stick.abs() <= deadzone {
        return 0.0;
    }
    let travel = ((stick.abs() - deadzone) / (1.0 - deadzone)).min(1.0);
    stick.signum() * travel.powf(1.0 / sensitivity)
}

fn read(
    keys: Res<ButtonInput<KeyCode>>,
    settings: Option<Res<crate::settings::Settings>>,
    pads: Query<&Gamepad>,
    mut reset: MessageWriter<Reset>,
    mut chosen: ResMut<Setup>,
    mut players: Query<&mut Controls, With<Player>>,
) {
    // Number keys pick a setup outright; the garage also exposes it.
    // Only write a change, so holding a key does not re-lean the car each frame.
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
        let (deadzone, sensitivity) = settings
            .as_ref()
            .map_or((DEADZONE, 1.0), |s| (s.deadzone, s.steering));
        let steer = stick_steer(stick, deadzone, sensitivity);
        let dpad_steer = f32::from(pad.pressed(GamepadButton::DPadLeft))
            - f32::from(pad.pressed(GamepadButton::DPadRight));
        let steer = if dpad_steer.abs() > steer.abs() {
            dpad_steer
        } else {
            steer
        };
        if steer.abs() > asked.steer.abs() {
            asked.steer = steer;
        }
        asked.throttle = asked
            .throttle
            .max(pad.get(GamepadButton::RightTrigger2).unwrap_or(0.0))
            .max(
                if pad.pressed(GamepadButton::South) || pad.pressed(GamepadButton::DPadUp) {
                    1.0
                } else {
                    0.0
                },
            );
        asked.brake = asked
            .brake
            .max(pad.get(GamepadButton::LeftTrigger2).unwrap_or(0.0))
            .max(
                if pad.pressed(GamepadButton::West) || pad.pressed(GamepadButton::DPadDown) {
                    1.0
                } else {
                    0.0
                },
            );
        asked.handbrake |= pad.pressed(GamepadButton::East);
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
    #[test]
    fn the_stick_curve_keeps_its_ends_and_bends_in_between() {
        use super::stick_steer;
        for sensitivity in [0.5, 1.0, 1.5] {
            assert_eq!(stick_steer(0.1, 0.12, sensitivity), 0.0);
            assert_eq!(stick_steer(1.0, 0.12, sensitivity), 1.0);
            assert_eq!(stick_steer(-1.0, 0.12, sensitivity), -1.0);
        }
        let half = |s| stick_steer(0.56, 0.12, s);
        assert!((half(1.0) - 0.5).abs() < 1e-6);
        assert!(half(1.5) > half(1.0) && half(1.0) > half(0.5));
    }

    use super::*;
    use crate::pause::Halt;

    #[test]
    fn the_dpad_drives_exactly_like_arrows_without_changing_setup() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Setup>()
            .init_resource::<Halt>()
            .add_message::<Reset>()
            .add_plugins(InputPlugin);
        let player = app.world_mut().spawn((Player, Controls::default())).id();
        let controller = app.world_mut().spawn(Gamepad::default()).id();
        for (key, button) in [
            (KeyCode::ArrowUp, GamepadButton::DPadUp),
            (KeyCode::ArrowDown, GamepadButton::DPadDown),
            (KeyCode::ArrowLeft, GamepadButton::DPadLeft),
            (KeyCode::ArrowRight, GamepadButton::DPadRight),
        ] {
            app.world_mut()
                .get_mut::<Gamepad>(controller)
                .unwrap()
                .digital_mut()
                .reset_all();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(key);
            app.update();
            let keyboard = *app.world().get::<Controls>(player).unwrap();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .reset_all();
            app.world_mut()
                .get_mut::<Gamepad>(controller)
                .unwrap()
                .digital_mut()
                .press(button);
            app.update();
            assert_eq!(*app.world().get::<Controls>(player).unwrap(), keyboard);
            assert_eq!(*app.world().resource::<Setup>(), Setup::Balanced);
        }
    }

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
