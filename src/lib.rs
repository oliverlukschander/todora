mod camera;
mod car;
mod hud;
mod input;
mod lap;
mod skid;
mod track;
mod world;

use bevy::{asset::AssetPlugin, prelude::*};

pub fn run() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Todora".into(),
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: asset_folder(),
                    ..default()
                }),
        )
        .add_plugins(GamePlugin)
        .run();
}

/// Start again: car on the grid, clock at zero, marks wiped. Sent by the reset
/// key; each plugin puts its own state back, because each plugin is what owns it.
#[derive(Message)]
pub(crate) struct Reset;

/// Where the assets are, relative to Bevy's base path. Under `cargo run` that
/// base is the crate and the assets sit in `assets/`. Inside a macOS bundle the
/// base is `Contents/MacOS`, where the executable is, and the assets sit in
/// `Contents/Resources`, where Finder expects resources to be.
fn asset_folder() -> String {
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.to_str().map(|s| s.contains(".app/Contents/MacOS/")))
        .unwrap_or(false);
    if bundled { "../Resources/assets" } else { "assets" }.to_string()
}

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Reset>().add_plugins((
            world::WorldPlugin,
            track::TrackPlugin,
            input::InputPlugin,
            car::CarPlugin,
            camera::CameraPlugin,
            lap::LapPlugin,
            skid::SkidPlugin,
            hud::HudPlugin,
        ));
    }
}
