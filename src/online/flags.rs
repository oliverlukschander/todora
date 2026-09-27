//! Bundled flags, independent of operating-system emoji fonts.
use bevy::prelude::*;

#[derive(Resource)]
pub(crate) struct Flags(Handle<Image>);

pub(super) fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Flags(assets.load("ui/flags/countries.png")));
}

impl Flags {
    pub(crate) fn image(&self, country: &str) -> ImageNode {
        let at = crate::identity::countries::ALL
            .iter()
            .position(|(code, _)| *code == country);
        let Some(at) = at else {
            return ImageNode {
                color: Color::NONE,
                ..default()
            };
        };
        let min = Vec2::new((at % 16) as f32 * 64.0, (at / 16) as f32 * 48.0);
        ImageNode {
            image: self.0.clone(),
            rect: Some(Rect::from_corners(min, min + Vec2::new(64.0, 48.0))),
            ..default()
        }
    }
}
