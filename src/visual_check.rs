//! Opt-in reproducible screenshots; absent from the packaged game.
//! TODORA_CAPTURE=/absolute/path.png TODORA_CIRCUIT=suzuka
//! TODORA_PROGRESS=0.78 cargo run --features visual-check
//! TODORA_ZOOM=2.4 sets the chase camera's zoom, which is also how far back
//! speed stretches the boom. TODORA_BEHIND=15 holds the chase camera that many
//! metres behind the car in plan, as the follow lag does at speed.
use crate::{
    car::{Car, Player},
    track::{Track, all_circuits},
};
use bevy::{
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};
#[derive(Resource)]
struct Capture {
    path: String,
    progress: f32,
    wide: bool,
    zoom: Option<f32>,
    behind: Option<f32>,
    frame: u32,
    preview: bool,
}
pub fn configure(app: &mut App) {
    let Ok(path) = std::env::var("TODORA_CAPTURE") else {
        return;
    };
    let circuit = std::env::var("TODORA_CIRCUIT").unwrap_or_else(|_| "suzuka".into());
    let track = Track::new(
        all_circuits()
            .iter()
            .find(|c| c.id == circuit)
            .expect("capture circuit"),
    );
    app.insert_resource(track)
        .insert_resource(Capture {
            path,
            progress: std::env::var("TODORA_PROGRESS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.78),
            wide: std::env::var_os("TODORA_WIDE").is_some(),
            zoom: std::env::var("TODORA_ZOOM")
                .ok()
                .and_then(|s| s.parse().ok()),
            behind: std::env::var("TODORA_BEHIND")
                .ok()
                .and_then(|s| s.parse().ok()),
            frame: 0,
            preview: std::env::var_os("TODORA_PREVIEW").is_some(),
        })
        .add_systems(Startup, window_size)
        .add_systems(
            PreUpdate,
            place
                .after(crate::track::TrackSet)
                .after(crate::input::InputSet)
                .after(crate::lap::ClockSet),
        )
        .add_systems(
            PostUpdate,
            capture.before(bevy::transform::TransformSystems::Propagate),
        );
}
fn place(
    track: Res<Track>,
    capture: Res<Capture>,
    mut cars: Query<(&mut Transform, &mut Car), With<Player>>,
    mut cameras: Query<&mut Transform, (With<Camera3d>, Without<Player>)>,
    mut timer: ResMut<crate::lap::LapTimer>,
) {
    if capture.preview {
        timer.invalid = true;
        timer.current = 32.45;
    }
    let points: Vec<_> = track.map_points().collect();
    let index = ((points.len() as f32 * capture.progress) as usize).min(points.len() - 1);
    let (pos, along) = points[index];
    if let Ok((mut at, mut car)) = cars.single_mut() {
        let scale = at.scale;
        *at = Transform::from_translation(pos)
            .looking_to(points[(index + 1) % points.len()].0 - pos, Vec3::Y)
            .with_scale(scale);
        car.along = Some(along);
        car.velocity = Vec3::ZERO;
        // The height is left to the camera, which is what is being checked.
        if let (Some(behind), Ok(mut camera)) = (capture.behind, cameras.single_mut()) {
            let back = at.translation - crate::car::level(*at.forward()) * behind;
            camera.translation.x = back.x;
            camera.translation.z = back.z;
        }
    }
}
fn capture(
    mut commands: Commands,
    mut capture: ResMut<Capture>,
    cars: Query<&Transform, With<Player>>,
    mut cameras: Query<&mut Transform, (With<Camera3d>, Without<Player>)>,
    mut follow: Query<&mut crate::camera::FollowCam>,
    mut exit: MessageWriter<AppExit>,
) {
    capture.frame += 1;
    if let Some(zoom) = capture.zoom {
        for mut camera in &mut follow {
            camera.zoom = zoom;
        }
    }
    if capture.wide
        && let (Ok(car), Ok(mut camera)) = (cars.single(), cameras.single_mut())
    {
        *camera =
            Transform::from_translation(car.translation - *car.forward() * 16.0 + Vec3::Y * 22.0)
                .looking_at(car.translation + *car.forward() * 8.0, Vec3::Y);
    }
    if capture.frame == 100 {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(capture.path.clone()));
    }
    if capture.frame > 140 {
        exit.write(AppExit::Success);
    }
}

fn window_size(mut windows: Query<&mut Window>) {
    if std::env::var_os("TODORA_SMALL").is_some() {
        for mut window in &mut windows {
            window.resolution.set(800.0, 600.0);
        }
    }
}
