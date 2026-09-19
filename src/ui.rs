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
        app.add_systems(PreStartup, font).add_systems(Update, scale);
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
