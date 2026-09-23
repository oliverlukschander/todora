//! Opt-in reproducible screenshots; absent from the packaged game.
//! TODORA_CAPTURE=/absolute/path.png TODORA_CIRCUIT=suzuka
//! TODORA_PROGRESS=0.78 cargo run --features visual-check
//! TODORA_ZOOM=2.4 sets the chase camera's zoom, which is also how far back
//! speed stretches the boom. TODORA_BEHIND=15 holds the chase camera that many
//! metres behind the car in plan, as the follow lag does at speed.
//! TODORA_SPLITS=PGY colours the sector bar (Purple, Green, Yellow, Plain) and
//! shows the last as the sector notice, 0.18 s off the best lap.
//! TODORA_SETTINGS='{"tv_margin":true}' starts from those settings.
//! TODORA_SUMMARY=best|valid|invalid holds the lap summary card up; `best`
//! also holds the new-best banner and the lit clock.
//! TODORA_GUIDE=0|1|2 shows a first-drive card; TODORA_DEVICE=pad shows the
//! pad's buttons on it.
//! TODORA_SCREEN=board opens the leaderboard on TODORA_VIEW=0..4 (3 this week, 4 records).
//! TODORA_SCREEN=settings opens the settings page, on TODORA_TAB=0..3 and
//! TODORA_ROW=n.
//! TODORA_COUNTDOWN=ready|3|2|1|go freezes the start lights at that moment and,
//! unless TODORA_PROGRESS is also given, leaves the car on the grid.
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
    /// Seconds into the full countdown to hold the lights at.
    countdown: Option<f32>,
    /// Whether the car is put somewhere round the lap, or left on the grid.
    placed: bool,
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
        .insert_resource(crate::settings::ReadOnly)
        // Captures start from the defaults, whatever this machine saved, or
        // from TODORA_SETTINGS='{"units":"mph"}' when a check needs a setting.
        .insert_resource(
            std::env::var("TODORA_SETTINGS")
                .map(|text| crate::settings::Settings::parse(&text))
                .unwrap_or_default(),
        )
        .insert_resource(crate::car::Spec::default())
        .insert_resource(crate::car::Setup::default())
        .insert_resource(crate::car::Mode::default())
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
            countdown: std::env::var("TODORA_COUNTDOWN")
                .ok()
                .map(|moment| match moment.as_str() {
                    "ready" => 0.4,
                    "3" => 1.3,
                    "2" => 2.3,
                    "1" => 3.3,
                    "go" => 4.0,
                    other => panic!("TODORA_COUNTDOWN={other}: ready, 3, 2, 1 or go"),
                }),
            placed: std::env::var_os("TODORA_COUNTDOWN").is_none()
                || std::env::var_os("TODORA_PROGRESS").is_some(),
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
            PreUpdate,
            freeze_countdown.after(crate::countdown::CountdownSet),
        )
        .add_systems(PreUpdate, open_screen.after(crate::settings::PageSet))
        .add_systems(Update, (hold_summary, show_guide))
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
    if let Ok(splits) = std::env::var("TODORA_SPLITS") {
        use crate::lap::{SectorNotice, Split};
        timer.splits = splits
            .chars()
            .map(|c| match c {
                'P' => Split::Purple,
                'G' => Split::Green,
                'Y' => Split::Yellow,
                _ => Split::Plain,
            })
            .collect();
        if let Some(&split) = timer.splits.last() {
            let delta = match split {
                Split::Yellow => 0.18,
                Split::Plain => 0.0,
                _ => -0.18,
            };
            let number = timer.splits.len();
            timer.announce(SectorNotice {
                number,
                time: 11.02,
                delta: Some(delta),
                split,
            });
        }
    }
    if !capture.placed {
        return;
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

fn freeze_countdown(capture: Res<Capture>, mut start: ResMut<crate::countdown::Start>) {
    match capture.countdown {
        Some(at) => start.elapsed = at,
        // A car put down round the lap is past any countdown.
        None if capture.placed => start.elapsed = f32::MAX,
        None => {}
    }
}

fn hold_summary(
    mut card: ResMut<crate::summary::Card>,
    mut party: ResMut<crate::summary::Celebration>,
    mut timer: ResMut<crate::lap::LapTimer>,
) {
    if let Ok(kind) = std::env::var("TODORA_SUMMARY") {
        card.hold(&mut timer, crate::summary::Card::example(&kind));
        if kind == "best" {
            party.hold();
        }
    }
}

fn show_guide(
    mut guide: ResMut<crate::onboarding::Guide>,
    mut device: ResMut<crate::input::LastDevice>,
) {
    if let Some(at) = std::env::var("TODORA_GUIDE")
        .ok()
        .and_then(|s| s.parse().ok())
    {
        guide.show(at);
    }
    if std::env::var("TODORA_DEVICE").as_deref() == Ok("pad") {
        device.set_if_neq(crate::input::LastDevice::Pad);
    }
}

fn open_screen(
    mut halt: ResMut<crate::pause::Halt>,
    mut page: ResMut<crate::settings::Page>,
    mut browse: ResMut<crate::online::Browse>,
    mut frames: Local<u32>,
) {
    let number = |name: &str| {
        std::env::var(name)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    };
    if std::env::var("TODORA_SCREEN").as_deref() == Ok("board") {
        *frames += 1;
        if *frames > 1 {
            *halt = crate::pause::Halt::Board;
        }
        if *frames > 2 {
            browse.show(number("TODORA_VIEW"));
        }
    }
    if std::env::var("TODORA_SCREEN").as_deref() == Ok("settings") {
        *frames += 1;
        // Opened on the second frame, the way a key would open it.
        if *frames > 1 {
            *halt = crate::pause::Halt::Settings;
        }
        if *frames > 2 {
            page.show(number("TODORA_TAB"), number("TODORA_ROW"));
        }
    }
}

fn window_size(mut windows: Query<&mut Window>) {
    if std::env::var_os("TODORA_SMALL").is_some() {
        for mut window in &mut windows {
            window.resolution.set(800.0, 600.0);
        }
    }
}
