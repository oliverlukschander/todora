mod achievements;
mod camera;
mod car;
mod challenge;
mod countdown;
mod ghost;
mod haptics;
mod hud;
mod input;
mod lap;
mod medals;
mod menu;
mod minimap;
mod multiplayer;
mod onboarding;
mod online;
mod pause;
mod replay;
mod settings;
mod skid;
mod sky;
mod sound;
mod summary;
mod track;
mod ui;
pub mod verify;
#[cfg(feature = "visual-check")]
mod visual_check;
mod world;

use bevy::{asset::AssetPlugin, prelude::*};

pub fn run() {
    let mut app = App::new();
    app.add_plugins(
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
    .add_plugins(GamePlugin);
    #[cfg(feature = "visual-check")]
    visual_check::configure(&mut app);
    app.run();
}

/// Start again: car on the grid, clock at zero, marks wiped. Sent by the reset
/// key, and by a change of circuit, which is a reset onto somewhere else. Each
/// plugin puts its own state back, because each plugin is what owns it.
///
/// This gives up the lap in progress, not the session. The laps already driven,
/// the best of them and the ghost belong to the circuit rather than to the lap,
/// so they survive a reset and go when the circuit does — which the plugins that
/// hold them tell apart by whether [`track::Track`] changed this frame.
#[derive(Message)]
pub(crate) struct Reset;

/// Where the assets are, relative to Bevy's base path. Under `cargo run` that
/// base is the crate and the assets sit in `assets/`. Inside a macOS bundle the
/// base is `Contents/MacOS`, where the executable is, and the assets sit in
/// `Contents/Resources`, where Finder expects resources to be. A Linux package
/// keeps `assets/` next to the binary; Bevy still resolves a relative path
/// from the process cwd, so that case is handed an absolute path.
fn asset_folder() -> String {
    let Ok(exe) = std::env::current_exe() else {
        return "assets".into();
    };
    if exe.to_string_lossy().contains(".app/Contents/MacOS/") {
        return "../Resources/assets".into();
    }
    if let Some(dir) = exe.parent() {
        let beside = dir.join("assets");
        if beside.is_dir() {
            return beside.to_string_lossy().into_owned();
        }
    }
    "assets".into()
}

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Reset>()
            // First, so every plugin after it starts from what was saved.
            .add_plugins(settings::SettingsPlugin)
            .add_plugins((
                ui::UiPlugin,
                world::WorldPlugin,
                sky::SkyPlugin,
                track::TrackPlugin,
                input::InputPlugin,
                car::CarPlugin,
                camera::CameraPlugin,
                lap::LapPlugin,
                menu::MenuPlugin,
                pause::PausePlugin,
                ghost::GhostPlugin,
                skid::SkidPlugin,
                sound::SoundPlugin,
                hud::HudPlugin,
                minimap::MinimapPlugin,
            ))
            .add_plugins((
                countdown::CountdownPlugin,
                summary::SummaryPlugin,
                onboarding::OnboardingPlugin,
                online::OnlinePlugin,
                challenge::ChallengePlugin,
                haptics::HapticsPlugin,
                achievements::AchievementsPlugin,
                replay::ReplayPlugin,
                multiplayer::MultiplayerPlugin,
            ))
            .add_systems(Update, quit);
    }
}

fn quit(keys: Res<ButtonInput<KeyCode>>, mut exit: MessageWriter<AppExit>) {
    let modifier = if cfg!(target_os = "macos") {
        [KeyCode::SuperLeft, KeyCode::SuperRight]
    } else {
        [KeyCode::ControlLeft, KeyCode::ControlRight]
    };
    if keys.any_pressed(modifier) && keys.just_pressed(KeyCode::KeyQ) {
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quit_shortcut_requires_the_modifier() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, quit);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyQ);
        app.update();
        assert!(app.should_exit().is_none());
        let modifier = if cfg!(target_os = "macos") {
            KeyCode::SuperLeft
        } else {
            KeyCode::ControlLeft
        };
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(modifier);
        app.update();
        assert!(app.should_exit().is_some());
    }
}
