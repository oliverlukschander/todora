//! The settings that belong to the window and the camera, applied when they
//! change: full screen, vertical sync, render scale, anti-aliasing, field of
//! view, the frame limit and the frame-rate readout. Nothing here runs on a
//! frame where no setting moved, except the frame limit, which is a sleep, and
//! the readout, which is rewritten four times a second while it is shown.

use std::time::{Duration, Instant};

use bevy::{
    camera::{ImageRenderTarget, Projection, RenderTarget},
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
    render::render_resource::TextureFormat,
    render::view::Msaa,
    window::{MonitorSelection, PresentMode, PrimaryWindow, WindowMode},
};

use super::Settings;
use crate::ui::{MUTED, label};

pub(super) fn plugin(app: &mut App) {
    if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default());
    }
    app.add_systems(Startup, spawn_readout)
        .add_systems(Update, (window, scale, camera, readout))
        .add_systems(Last, limit);
}

/// Full screen and vertical sync, on the window.
fn window(
    settings: Res<Settings>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut seen: Local<Option<(bool, bool)>>,
) {
    let wanted = (settings.fullscreen, settings.vsync);
    if *seen == Some(wanted) {
        return;
    }
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    *seen = Some(wanted);
    window.mode = if settings.fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    };
    window.present_mode = if settings.vsync {
        PresentMode::AutoVsync
    } else {
        PresentMode::AutoNoVsync
    };
}

/// The camera that puts a scaled-down world on the window, and the picture it
/// shows. Only there while the render scale is below 100%.
#[derive(Component)]
struct Present;

/// The size, in pixels, to draw the world at for a window this big, or `None`
/// to draw it straight to the window.
fn scaled_size(window: UVec2, scale: f32) -> Option<UVec2> {
    (scale < 0.999).then(|| {
        (window.as_vec2() * scale)
            .round()
            .as_uvec2()
            .max(UVec2::ONE)
    })
}

/// Render scale. Below 100% the 3D camera draws into an image that many times
/// the window's size, and a second camera stretches it over the window with
/// the HUD drawn on top at full resolution: at half scale the world costs a
/// quarter of the pixels, for one more full-screen quad. At 100% the world is
/// drawn straight to the window, as it always was.
fn scale(
    settings: Res<Settings>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    worlds: Query<Entity, With<Camera3d>>,
    presents: Query<Entity, With<Present>>,
    mut seen: Local<Option<Option<UVec2>>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let wanted = scaled_size(window.physical_size(), settings.render_scale);
    if *seen == Some(wanted) {
        return;
    }
    *seen = Some(wanted);
    for present in &presents {
        commands.entity(present).despawn();
    }
    let Some(size) = wanted else {
        for world in &worlds {
            commands
                .entity(world)
                .insert(RenderTarget::Window(bevy::window::WindowRef::Primary));
        }
        return;
    };
    let image = images.add(Image::new_target_texture(
        size.x,
        size.y,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    for world in &worlds {
        commands
            .entity(world)
            .insert(RenderTarget::Image(ImageRenderTarget {
                handle: image.clone(),
                scale_factor: 1.0,
            }));
    }
    commands.spawn((
        Present,
        Camera2d,
        Camera {
            order: 1,
            ..default()
        },
        IsDefaultUiCamera,
    ));
    commands.spawn((
        Present,
        ImageNode::new(image),
        GlobalZIndex(i32::MIN),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
    ));
}

/// Anti-aliasing and field of view, on the 3D camera.
fn camera(
    settings: Res<Settings>,
    mut commands: Commands,
    mut cameras: Query<(Entity, &mut Projection), With<Camera3d>>,
    mut seen: Local<Option<(bool, u32)>>,
) {
    let wanted = (settings.antialiasing, settings.fov.to_bits());
    if *seen == Some(wanted) {
        return;
    }
    for (camera, mut projection) in &mut cameras {
        *seen = Some(wanted);
        commands.entity(camera).insert(if settings.antialiasing {
            Msaa::Sample4
        } else {
            Msaa::Off
        });
        if let Projection::Perspective(perspective) = projection.as_mut() {
            perspective.fov = settings.fov.to_radians();
        }
    }
}

/// How long a frame has to take at `cap` frames a second, or `None` for none.
fn frame_budget(cap: u32) -> Option<Duration> {
    (cap > 0).then(|| Duration::from_secs_f64(1.0 / f64::from(cap)))
}

/// Sleep off whatever is left of the frame's budget. The last thing a frame
/// does, so the time slept is time nothing was waiting on.
fn limit(settings: Res<Settings>, mut last: Local<Option<Instant>>) {
    let now = Instant::now();
    if let (Some(budget), Some(then)) = (frame_budget(settings.fps_cap), *last) {
        let spent = now - then;
        if spent < budget {
            std::thread::sleep(budget - spent);
        }
    }
    *last = Some(Instant::now());
}

#[derive(Component)]
struct Readout;

fn spawn_readout(mut commands: Commands) {
    commands.spawn((
        Readout,
        label("", 13.0, MUTED),
        Node {
            position_type: PositionType::Absolute,
            top: px(6),
            left: percent(50),
            ..default()
        },
        Visibility::Hidden,
    ));
}

/// Frames a second, smoothed, while the setting is on.
fn readout(
    settings: Res<Settings>,
    diagnostics: Res<DiagnosticsStore>,
    time: Res<Time<Real>>,
    mut next: Local<f64>,
    mut texts: Query<(&mut Text, &mut Visibility), With<Readout>>,
) {
    for (mut text, mut visibility) in &mut texts {
        visibility.set_if_neq(if settings.show_fps {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
        let now = time.elapsed_secs_f64();
        if !settings.show_fps || now < *next {
            continue;
        }
        *next = now + 0.25;
        if let Some(fps) = diagnostics
            .get(&FrameTimeDiagnosticsPlugin::FPS)
            .and_then(|fps| fps.smoothed())
        {
            text.0 = format!("{fps:.0} fps");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_scale_draws_fewer_pixels_and_full_scale_draws_to_the_window() {
        assert_eq!(scaled_size(UVec2::new(2560, 1440), 1.0), None);
        assert_eq!(
            scaled_size(UVec2::new(2560, 1440), 0.5),
            Some(UVec2::new(1280, 720))
        );
        assert_eq!(
            scaled_size(UVec2::new(2560, 1440), 0.75),
            Some(UVec2::new(1920, 1080))
        );
    }

    #[test]
    fn a_frame_limit_is_a_budget_per_frame() {
        assert_eq!(frame_budget(0), None);
        assert_eq!(frame_budget(60), Some(Duration::from_secs_f64(1.0 / 60.0)));
    }
}
