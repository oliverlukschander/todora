//! Shared typography and colours for the driving instruments and menus.
use bevy::prelude::*;

pub(crate) const TEXT: Color = Color::srgb(0.91, 0.94, 0.95);
pub(crate) const MUTED: Color = Color::srgb(0.57, 0.64, 0.68);
pub(crate) const ACCENT: Color = Color::srgb(0.77, 0.96, 0.36);
pub(crate) const SURFACE: Color = Color::srgb(0.075, 0.095, 0.11);
pub(crate) const RAISED: Color = Color::srgb(0.11, 0.14, 0.16);
pub(crate) const LINE: Color = Color::srgb(0.21, 0.26, 0.29);

/// The colours that carry meaning: ahead and behind, and the three sector
/// colours. The colour-blind set swaps red and green for orange and blue and
/// keeps purple apart from both, so no pair relies on telling red from green.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Palette {
    pub ahead: Color,
    pub behind: Color,
    pub purple: Color,
    pub green: Color,
    pub yellow: Color,
}

impl Palette {
    pub(crate) const STANDARD: Self = Self {
        ahead: Color::srgb(0.38, 0.86, 0.42),
        behind: Color::srgb(0.96, 0.32, 0.26),
        purple: Color::srgb(0.74, 0.42, 1.0),
        green: Color::srgb(0.38, 0.86, 0.42),
        yellow: Color::srgb(0.98, 0.83, 0.27),
    };
    pub(crate) const COLOUR_BLIND: Self = Self {
        ahead: Color::srgb(0.30, 0.62, 1.0),
        behind: Color::srgb(1.0, 0.60, 0.12),
        purple: Color::srgb(0.86, 0.50, 0.98),
        green: Color::srgb(0.30, 0.62, 1.0),
        yellow: Color::srgb(1.0, 0.60, 0.12),
    };

    pub(crate) fn of(settings: Option<&crate::settings::Settings>) -> Self {
        if settings.is_some_and(|s| s.colour_blind) {
            Self::COLOUR_BLIND
        } else {
            Self::STANDARD
        }
    }
}

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

/// The HUD is sized against the window as the display draws it natively, so
/// lowering the render scale (which lowers the scale factor, and so grows the
/// logical window) leaves it the same size on screen. The chosen text size
/// multiplies that.
fn scale(
    windows: Query<&Window>,
    settings: Option<Res<crate::settings::Settings>>,
    mut scale: ResMut<UiScale>,
) {
    if let Ok(window) = windows.single() {
        let native = window.resolution.base_scale_factor();
        let (width, height) = (
            window.physical_width() as f32 / native,
            window.physical_height() as f32 / native,
        );
        let chosen = settings.map_or(1.0, |s| s.ui_scale);
        let wanted = fit(width, height) * chosen * native / window.scale_factor();
        if (scale.0 - wanted).abs() > 0.001 {
            scale.0 = wanted;
        }
    }
}

/// How much the HUD is scaled for a window of this logical size.
pub(crate) fn fit(width: f32, height: f32) -> f32 {
    (width / 1280.0).min(height / 800.0).min(1.25)
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
    /// The arrows, WASD, the D-pad and the left stick.
    pub(crate) fn read(
        &mut self,
        keys: &ButtonInput<KeyCode>,
        pads: &Query<&Gamepad>,
        now: f64,
    ) -> IVec2 {
        self.read_keys(keys, pads, now, true)
    }

    /// As [`Self::read`], leaving W, A, S and D out when `letters` is false:
    /// on a field that is being typed into they are letters.
    pub(crate) fn read_keys(
        &mut self,
        keys: &ButtonInput<KeyCode>,
        pads: &Query<&Gamepad>,
        now: f64,
        letters: bool,
    ) -> IVec2 {
        let directions = [
            (
                KeyCode::ArrowLeft,
                KeyCode::KeyA,
                GamepadButton::DPadLeft,
                Vec2::NEG_X,
            ),
            (
                KeyCode::ArrowRight,
                KeyCode::KeyD,
                GamepadButton::DPadRight,
                Vec2::X,
            ),
            (
                KeyCode::ArrowUp,
                KeyCode::KeyW,
                GamepadButton::DPadUp,
                Vec2::Y,
            ),
            (
                KeyCode::ArrowDown,
                KeyCode::KeyS,
                GamepadButton::DPadDown,
                Vec2::NEG_Y,
            ),
        ];
        let mut digital = Vec2::ZERO;
        let mut tapped = false;
        for (arrow, letter, button, direction) in directions {
            let held = keys.pressed(arrow) || (letters && keys.pressed(letter));
            let fresh = keys.just_pressed(arrow) || (letters && keys.just_pressed(letter));
            if held || pads.iter().any(|pad| pad.pressed(button)) {
                digital += direction;
            }
            tapped |= fresh || pads.iter().any(|pad| pad.just_pressed(button));
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
