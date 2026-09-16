mod camera;
mod car;
mod hud;
mod lap;
mod track;
mod world;

use bevy::prelude::*;

pub fn run() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Todora".into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(GamePlugin)
        .run();
}

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            world::WorldPlugin,
            track::TrackPlugin,
            car::CarPlugin,
            camera::CameraPlugin,
            lap::LapPlugin,
            hud::HudPlugin,
        ));
    }
}
