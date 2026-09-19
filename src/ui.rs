//! Shared typography and colours for the driving instruments and menus.
use bevy::prelude::*;

pub(crate) const TEXT: Color = Color::srgb(0.91, 0.94, 0.95);
pub(crate) const MUTED: Color = Color::srgb(0.57, 0.64, 0.68);
pub(crate) const ACCENT: Color = Color::srgb(0.77, 0.96, 0.36);
pub(crate) const SURFACE: Color = Color::srgb(0.075, 0.095, 0.11);
pub(crate) const RAISED: Color = Color::srgb(0.11, 0.14, 0.16);
pub(crate) const LINE: Color = Color::srgb(0.21, 0.26, 0.29);

pub(crate) struct UiPlugin;
impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, font)
            .add_systems(Update, (scale, cursor_visibility));
    }
}

fn cursor_visibility(
    halt: Res<crate::pause::Halt>,
    mut cursors: Query<&mut bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
) {
    for mut cursor in &mut cursors {
        if cursor.visible != halt.stopped() {
            cursor.visible = halt.stopped();
        }
    }
}

fn font(mut fonts: ResMut<Assets<Font>>) {
    // Replace the ASCII-oriented engine subset for every TextFont, including
    // HUD circuit names. Embed it so app bundles cannot lose their typeface.
    fonts
        .insert(
            bevy::asset::AssetId::default(),
            Font::from_bytes(include_bytes!("../assets/fonts/Figtree.ttf").to_vec()),
        )
        .expect("the default font asset is available");
}

fn scale(windows: Query<&Window>, mut scale: ResMut<UiScale>) {
    if let Ok(window) = windows.single() {
        let wanted = (window.width() / 1280.0)
            .min(window.height() / 800.0)
            .min(1.25);
        if (scale.0 - wanted).abs() > 0.001 {
            scale.0 = wanted;
        }
    }
}

pub(crate) fn label(value: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
    )
}

/// A deliberate stick tilt moves once, then repeats using real time while
/// the game clock is paused. Hysteresis stops small thumb movements chattering.
#[derive(Default)]
pub(crate) struct Navigation {
    direction: IVec2,
    repeat_at: f64,
}

impl Navigation {
    pub(crate) fn step(&mut self, stick: Vec2, now: f64) -> IVec2 {
        let threshold = if self.direction == IVec2::ZERO {
            0.55
        } else {
            0.35
        };
        let direction = if stick.abs().max_element() < threshold {
            IVec2::ZERO
        } else if stick.x.abs() > stick.y.abs() {
            IVec2::new(stick.x.signum() as i32, 0)
        } else {
            IVec2::new(0, -stick.y.signum() as i32)
        };
        if direction != self.direction {
            self.direction = direction;
            self.repeat_at = now + 0.35;
            direction
        } else if direction != IVec2::ZERO && now >= self.repeat_at {
            self.repeat_at = now + 0.12;
            direction
        } else {
            IVec2::ZERO
        }
    }
}

impl Navigation {
    pub(crate) fn read(
        &mut self,
        keys: &ButtonInput<KeyCode>,
        pads: &Query<&Gamepad>,
        now: f64,
    ) -> IVec2 {
        let directions = [
            (KeyCode::ArrowLeft, GamepadButton::DPadLeft, Vec2::NEG_X),
            (KeyCode::ArrowRight, GamepadButton::DPadRight, Vec2::X),
            (KeyCode::ArrowUp, GamepadButton::DPadUp, Vec2::Y),
            (KeyCode::ArrowDown, GamepadButton::DPadDown, Vec2::NEG_Y),
        ];
        let mut digital = Vec2::ZERO;
        let mut tapped = false;
        for (key, button, direction) in directions {
            if keys.pressed(key) || pads.iter().any(|pad| pad.pressed(button)) {
                digital += direction;
            }
            tapped |= keys.just_pressed(key) || pads.iter().any(|pad| pad.just_pressed(button));
        }
        let stick = pads
            .iter()
            .map(Gamepad::left_stick)
            .max_by(|a, b| a.length_squared().total_cmp(&b.length_squared()))
            .unwrap_or_default();
        // A fresh digital press is immediate, including repeated taps in the
        // same direction. Held arrows, D-pad and stick share the repeat timing.
        if tapped {
            self.direction = IVec2::ZERO;
        }
        self.step(
            if digital != Vec2::ZERO {
                digital
            } else {
                stick
            },
            now,
        )
    }
}
