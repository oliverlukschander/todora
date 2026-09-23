//! The settings that belong to the window and the camera, applied when they
//! change: full screen, vertical sync, render scale, anti-aliasing, field of
//! view, the frame limit and the frame-rate readout. Nothing here runs on a
//! frame where no setting moved, except the frame limit, which is a sleep, and
//! the readout, which is rewritten four times a second while it is shown.

use std::time::{Duration, Instant};

use bevy::{
    camera::Projection,
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
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
        .add_systems(Update, (window, camera, readout))
        .add_systems(Last, limit);
}

/// Full screen, vertical sync and render scale, on the window.
///
/// Render scale lowers the scale factor the window is drawn at, which is the
/// one knob that makes the whole frame cheaper without a second render
/// target: at half scale a Retina display draws a quarter of the pixels. The
/// HUD keeps its size, because [`crate::ui`] scales it against the physical
/// window rather than the logical one.
fn window(
    settings: Res<Settings>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut seen: Local<Option<(bool, bool, f32)>>,
) {
    let wanted = (settings.fullscreen, settings.vsync, settings.render_scale);
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
    let native = window.resolution.base_scale_factor();
    window
        .resolution
        .set_scale_factor_override(render_factor(native, settings.render_scale));
}

/// The scale factor to draw at, or `None` for the display's own.
fn render_factor(native: f32, scale: f32) -> Option<f32> {
    (scale < 0.999).then_some(native * scale)
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
    fn render_scale_lowers_the_scale_factor_and_full_scale_leaves_it_alone() {
        assert_eq!(render_factor(2.0, 1.0), None);
        assert_eq!(render_factor(2.0, 0.5), Some(1.0));
        assert_eq!(render_factor(1.0, 0.75), Some(0.75));
    }

    #[test]
    fn a_frame_limit_is_a_budget_per_frame() {
        assert_eq!(frame_budget(0), None);
        assert_eq!(frame_budget(60), Some(Duration::from_secs_f64(1.0 / 60.0)));
    }
}
