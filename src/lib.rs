mod camera;
mod car;
mod hud;
mod lap;
mod skid;
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

/// Start again: car on the grid, clock at zero, marks wiped. Sent by the reset
/// key; each plugin puts its own state back, because each plugin is what owns it.
#[derive(Message)]
pub(crate) struct Reset;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Reset>().add_plugins((
            world::WorldPlugin,
            track::TrackPlugin,
            car::CarPlugin,
            camera::CameraPlugin,
            lap::LapPlugin,
            skid::SkidPlugin,
            hud::HudPlugin,
        ));
    }
}
