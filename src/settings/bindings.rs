//! Which key and which pad button does what.
//!
//! Nine actions each have a key and a pad button, stored by name in the
//! settings. The arrow keys, Shift for the brake, the sticks and the analogue
//! triggers always work as well, and Escape, Enter and Start are the menus', so
//! none of those can be given away. Binding a key or button that another action
//! has swaps the two, so every action always has one.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Act {
    Throttle,
    Brake,
    Left,
    Right,
    Handbrake,
    Restart,
    Ghost,
    Camera,
    Board,
}

impl Act {
    pub(crate) const ALL: [Self; 9] = [
        Self::Throttle,
        Self::Brake,
        Self::Left,
        Self::Right,
        Self::Handbrake,
        Self::Restart,
        Self::Ghost,
        Self::Camera,
        Self::Board,
    ];

    pub(crate) fn name(self) -> &'static str {
        crate::text::t(match self {
            Self::Throttle => "act.throttle",
            Self::Brake => "act.brake",
            Self::Left => "act.left",
            Self::Right => "act.right",
            Self::Handbrake => "act.handbrake",
            Self::Restart => "act.restart",
            Self::Ghost => "act.ghost",
            Self::Camera => "act.camera",
            Self::Board => "act.board",
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(crate) struct Binding {
    pub key: String,
    pub pad: String,
}

fn bind(key: &str, pad: &str) -> Binding {
    Binding {
        key: key.into(),
        pad: pad.into(),
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub(crate) struct Bindings {
    pub throttle: Binding,
    pub brake: Binding,
    pub left: Binding,
    pub right: Binding,
    pub handbrake: Binding,
    pub restart: Binding,
    pub ghost: Binding,
    pub camera: Binding,
    pub board: Binding,
}

impl Default for Bindings {
    fn default() -> Self {
        Self {
            throttle: bind("KeyW", "South"),
            brake: bind("KeyS", "West"),
            left: bind("KeyA", "DPadLeft"),
            right: bind("KeyD", "DPadRight"),
            handbrake: bind("Space", "East"),
            restart: bind("KeyR", "RightTrigger"),
            ghost: bind("KeyG", "North"),
            camera: bind("KeyV", "DPadUp"),
            board: bind("KeyL", "LeftThumb"),
        }
    }
}

/// The keys that may be bound, by the name stored and the name shown.
const KEYS: &[(&str, &str, KeyCode)] = &[
    ("KeyA", "A", KeyCode::KeyA),
    ("KeyB", "B", KeyCode::KeyB),
    ("KeyC", "C", KeyCode::KeyC),
    ("KeyD", "D", KeyCode::KeyD),
    ("KeyE", "E", KeyCode::KeyE),
    ("KeyF", "F", KeyCode::KeyF),
    ("KeyG", "G", KeyCode::KeyG),
    ("KeyH", "H", KeyCode::KeyH),
    ("KeyI", "I", KeyCode::KeyI),
    ("KeyJ", "J", KeyCode::KeyJ),
    ("KeyK", "K", KeyCode::KeyK),
    ("KeyL", "L", KeyCode::KeyL),
    ("KeyM", "M", KeyCode::KeyM),
    ("KeyN", "N", KeyCode::KeyN),
    ("KeyO", "O", KeyCode::KeyO),
    ("KeyP", "P", KeyCode::KeyP),
    ("KeyQ", "Q", KeyCode::KeyQ),
    ("KeyR", "R", KeyCode::KeyR),
    ("KeyS", "S", KeyCode::KeyS),
    ("KeyT", "T", KeyCode::KeyT),
    ("KeyU", "U", KeyCode::KeyU),
    ("KeyV", "V", KeyCode::KeyV),
    ("KeyW", "W", KeyCode::KeyW),
    ("KeyX", "X", KeyCode::KeyX),
    ("KeyY", "Y", KeyCode::KeyY),
    ("KeyZ", "Z", KeyCode::KeyZ),
    ("Digit4", "4", KeyCode::Digit4),
    ("Digit5", "5", KeyCode::Digit5),
    ("Digit6", "6", KeyCode::Digit6),
    ("Digit7", "7", KeyCode::Digit7),
    ("Digit8", "8", KeyCode::Digit8),
    ("Digit9", "9", KeyCode::Digit9),
    ("Digit0", "0", KeyCode::Digit0),
    ("Space", "Space", KeyCode::Space),
    ("Tab", "Tab", KeyCode::Tab),
    ("ControlLeft", "Ctrl", KeyCode::ControlLeft),
    ("AltLeft", "Alt", KeyCode::AltLeft),
    ("Comma", ",", KeyCode::Comma),
    ("Period", ".", KeyCode::Period),
    ("Slash", "/", KeyCode::Slash),
    ("Semicolon", ";", KeyCode::Semicolon),
];

/// The pad buttons that may be bound.
const PADS: &[(&str, &str, GamepadButton)] = &[
    ("South", "A", GamepadButton::South),
    ("East", "B", GamepadButton::East),
    ("West", "X", GamepadButton::West),
    ("North", "Y", GamepadButton::North),
    ("LeftTrigger", "LB", GamepadButton::LeftTrigger),
    ("RightTrigger", "RB", GamepadButton::RightTrigger),
    ("DPadUp", "D-pad up", GamepadButton::DPadUp),
    ("DPadDown", "D-pad down", GamepadButton::DPadDown),
    ("DPadLeft", "D-pad left", GamepadButton::DPadLeft),
    ("DPadRight", "D-pad right", GamepadButton::DPadRight),
    ("LeftThumb", "L3", GamepadButton::LeftThumb),
    ("RightThumb", "R3", GamepadButton::RightThumb),
];

/// The defaults, made once, for anything that reads bindings without settings.
pub(crate) static DEFAULT: std::sync::LazyLock<Bindings> =
    std::sync::LazyLock::new(Bindings::default);

/// The bindings in force: the settings', or the defaults.
pub(crate) fn current(settings: Option<&super::Settings>) -> &Bindings {
    settings.map_or(&*DEFAULT, |s| &s.bindings)
}

impl Bindings {
    /// Whether `act`'s key is held.
    pub(crate) fn key_held(&self, keys: &ButtonInput<KeyCode>, act: Act) -> bool {
        self.key(act).is_some_and(|k| keys.pressed(k))
    }

    /// Whether `act`'s key or button was just pressed.
    pub(crate) fn just(
        &self,
        keys: &ButtonInput<KeyCode>,
        pads: &Query<&Gamepad>,
        act: Act,
    ) -> bool {
        self.key(act).is_some_and(|k| keys.just_pressed(k))
            || self
                .pad(act)
                .is_some_and(|b| pads.iter().any(|pad| pad.just_pressed(b)))
    }

    pub(crate) fn of(&self, act: Act) -> &Binding {
        match act {
            Act::Throttle => &self.throttle,
            Act::Brake => &self.brake,
            Act::Left => &self.left,
            Act::Right => &self.right,
            Act::Handbrake => &self.handbrake,
            Act::Restart => &self.restart,
            Act::Ghost => &self.ghost,
            Act::Camera => &self.camera,
            Act::Board => &self.board,
        }
    }

    fn of_mut(&mut self, act: Act) -> &mut Binding {
        match act {
            Act::Throttle => &mut self.throttle,
            Act::Brake => &mut self.brake,
            Act::Left => &mut self.left,
            Act::Right => &mut self.right,
            Act::Handbrake => &mut self.handbrake,
            Act::Restart => &mut self.restart,
            Act::Ghost => &mut self.ghost,
            Act::Camera => &mut self.camera,
            Act::Board => &mut self.board,
        }
    }

    pub(crate) fn key(&self, act: Act) -> Option<KeyCode> {
        let name = &self.of(act).key;
        KEYS.iter().find(|(n, _, _)| n == name).map(|(_, _, k)| *k)
    }

    pub(crate) fn pad(&self, act: Act) -> Option<GamepadButton> {
        let name = &self.of(act).pad;
        PADS.iter().find(|(n, _, _)| n == name).map(|(_, _, b)| *b)
    }

    /// What a row shows: the key and the button.
    pub(crate) fn shown(&self, act: Act) -> String {
        let binding = self.of(act);
        let key = KEYS
            .iter()
            .find(|(n, _, _)| *n == binding.key)
            .map_or("—", |k| k.1);
        let pad = PADS
            .iter()
            .find(|(n, _, _)| *n == binding.pad)
            .map_or("—", |p| p.1);
        format!("{key}   ·   {pad}")
    }

    /// Give `act` this key, if it may be bound; whoever had it gets `act`'s.
    pub(crate) fn bind_key(&mut self, act: Act, key: KeyCode) -> bool {
        let Some((name, _, _)) = KEYS.iter().find(|(_, _, k)| *k == key) else {
            return false;
        };
        let old = self.of(act).key.clone();
        if let Some(other) = Act::ALL
            .into_iter()
            .find(|a| *a != act && self.of(*a).key == *name)
        {
            self.of_mut(other).key = old;
        }
        self.of_mut(act).key = (*name).into();
        true
    }

    /// Give `act` this button, the same way.
    pub(crate) fn bind_pad(&mut self, act: Act, button: GamepadButton) -> bool {
        let Some((name, _, _)) = PADS.iter().find(|(_, _, b)| *b == button) else {
            return false;
        };
        let old = self.of(act).pad.clone();
        if let Some(other) = Act::ALL
            .into_iter()
            .find(|a| *a != act && self.of(*a).pad == *name)
        {
            self.of_mut(other).pad = old;
        }
        self.of_mut(act).pad = (*name).into();
        true
    }

    /// Any key that may be bound and was just pressed.
    pub(crate) fn pressed_key(keys: &ButtonInput<KeyCode>) -> Option<KeyCode> {
        KEYS.iter()
            .map(|(_, _, k)| *k)
            .find(|k| keys.just_pressed(*k))
    }

    /// Any button that may be bound and was just pressed.
    pub(crate) fn pressed_pad(pads: &Query<&Gamepad>) -> Option<GamepadButton> {
        PADS.iter()
            .map(|(_, _, b)| *b)
            .find(|b| pads.iter().any(|pad| pad.just_pressed(*b)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_default_binding_is_a_real_key_and_button_and_none_are_shared() {
        let b = Bindings::default();
        let keys: std::collections::HashSet<_> =
            Act::ALL.iter().map(|a| b.key(*a).unwrap()).collect();
        let pads: std::collections::HashSet<_> =
            Act::ALL.iter().map(|a| b.pad(*a).unwrap()).collect();
        assert_eq!((keys.len(), pads.len()), (9, 9));
    }

    #[test]
    fn binding_a_taken_key_swaps_rather_than_drops() {
        let mut b = Bindings::default();
        assert!(b.bind_key(Act::Throttle, KeyCode::KeyG));
        assert_eq!(b.key(Act::Throttle), Some(KeyCode::KeyG));
        assert_eq!(
            b.key(Act::Ghost),
            Some(KeyCode::KeyW),
            "ghost took throttle's old key"
        );
        assert!(b.bind_pad(Act::Camera, GamepadButton::South));
        assert_eq!(b.pad(Act::Throttle), Some(GamepadButton::DPadUp));
        assert!(
            !b.bind_key(Act::Brake, KeyCode::Escape),
            "the menus keep Escape"
        );
        assert!(
            !b.bind_key(Act::Brake, KeyCode::ArrowUp),
            "the arrows always drive"
        );
        assert_eq!(b.shown(Act::Ghost), "W   ·   Y");
    }
}
