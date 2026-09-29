//! Opt-in reproducible screenshots; absent from the packaged game.
//! TODORA_CAPTURE=/absolute/path.png TODORA_CIRCUIT=suzuka
//! TODORA_PROGRESS=0.78 cargo run --features visual-check
//! TODORA_LOAD=le-mans selects another circuit after startup, captures its
//! loading screen, and waits for the completed switch before exiting.
//! TODORA_ZOOM=2.4 sets the chase camera's zoom, which is also how far back
//! speed stretches the boom. TODORA_BEHIND=15 holds the chase camera that many
//! metres behind the car in plan, as the follow lag does at speed.
//! TODORA_SPLITS=PGY colours the sector bar (Purple, Green, Yellow, Plain) and
//! shows the last as the sector notice, 0.18 s off the best lap.
//! TODORA_SETTINGS='{"tv_margin":true}' starts from those settings.
//! TODORA_SUMMARY=best|valid|invalid|bonkers holds the lap summary card up; `best`
//! also holds the new-best banner and the lit clock.
//! TODORA_GUIDE=0|1|2 shows a first-drive card; TODORA_DEVICE=pad shows the
//! pad's buttons on it.
//! TODORA_SCREEN=board opens the leaderboard on TODORA_VIEW=0..5 (3 this week, 4 records, 5 awards).
//! TODORA_SCREEN=settings opens the settings page, on TODORA_TAB=0..3 and
//! TODORA_ROW=n.
//! TODORA_SCREEN=title|replay|offer|circuits|garage opens that screen, and
//! TODORA_EXAMPLE=21 fills the leaderboard as if you were 21st of 300.
//! TODORA_COLLECTION=0..4 chooses All, DHH ’14, Endurance, Grand Prix or Heritage.
//! TODORA_CAM=yaw:pitch:distance holds the camera on an orbit round the car, for
//! looking at it from all sides: yaw 0 is straight behind, 90 the right, 180
//! in front; pitch is degrees up; distance is in game units.
//! TODORA_AT=metres puts the car that far round the lap instead of at
//! TODORA_PROGRESS, for looking at a particular place.
//! TODORA_DRIVE=1 puts the game's own AI driver at the wheel from TODORA_PROGRESS
//! onwards, so that things that move can be looked at moving. TODORA_FRAME=n
//! takes the picture on that frame instead of the 100th, and
//! TODORA_BURST=count:interval takes `count` pictures `interval` frames apart,
//! named `<path>-000.png` onwards.
//! TODORA_TIME=frames measures the frame rate instead: it prints how long that
//! many frames take once the first 240 have warmed everything up, with the
//! display's sync off so that a fast machine shows how fast, and exits. Give it
//! TODORA_DRIVE=1 to be measured while there is something going on.
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
    /// Yaw, pitch and distance of an orbit round the car.
    cam: Option<(f32, f32, f32)>,
    /// The AI drives.
    drive: bool,
    /// The frame to take the picture on.
    shoot: u32,
    /// How many pictures, and how many frames apart.
    burst: (u32, u32),
    zoom: Option<f32>,
    behind: Option<f32>,
    frame: u32,
    preview: bool,
    /// Seconds into the full countdown to hold the lights at.
    countdown: Option<f32>,
    /// Whether the car is put somewhere round the lap, or left on the grid.
    placed: bool,
    load: Option<&'static crate::track::Circuit>,
    loading_frames: u32,
    /// How many frames to time, and when the timing began.
    time: Option<u32>,
    started: Option<std::time::Instant>,
}

/// Time spent in the game's own schedules, from the first system of a frame to
/// the last, which is how much of the frame is the game and not the display.
#[derive(Resource, Default)]
struct Spent {
    since: Option<std::time::Instant>,
    total: f64,
    frames: u32,
}

fn clock_in(mut spent: ResMut<Spent>) {
    spent.since = Some(std::time::Instant::now());
}

fn clock_out(mut spent: ResMut<Spent>, capture: Res<Capture>) {
    if let (Some(since), Some(_)) = (spent.since, capture.time)
        && capture.frame >= 240
    {
        spent.total += since.elapsed().as_secs_f64();
        spent.frames += 1;
    }
}
pub fn configure(app: &mut App) {
    let Ok(path) = std::env::var("TODORA_CAPTURE") else {
        return;
    };
    let circuit = std::env::var("TODORA_CIRCUIT").unwrap_or_else(|_| "suzuka".into());
    // Captures start from the defaults, whatever this machine saved, or from
    // TODORA_SETTINGS='{"units":"mph"}' when a check needs a setting.
    let settings = std::env::var("TODORA_SETTINGS")
        .map(|text| crate::settings::Settings::parse(&text))
        .unwrap_or_default();
    // Built as wild as those settings say the circuits are to be.
    let wild = crate::fun::Fun::of(&settings).wild.level();
    let track = Track::with_wild(
        all_circuits()
            .iter()
            .find(|c| c.id == circuit)
            .expect("capture circuit"),
        wild,
    );
    let track_length = track.length();
    app.insert_resource(track)
        .insert_resource(crate::track::WildRequest(wild))
        .insert_resource(crate::settings::ReadOnly)
        .insert_resource(settings)
        .insert_resource(crate::car::Spec::default())
        .insert_resource(crate::car::Setup::default())
        .insert_resource(crate::car::Mode::default())
        .insert_resource(Capture {
            path,
            progress: std::env::var("TODORA_AT")
                .ok()
                .and_then(|s| s.parse::<f32>().ok())
                .map(|metres| metres / track_length)
                .or_else(|| {
                    std::env::var("TODORA_PROGRESS")
                        .ok()
                        .and_then(|s| s.parse().ok())
                })
                .unwrap_or(0.78),
            wide: std::env::var_os("TODORA_WIDE").is_some(),
            drive: std::env::var_os("TODORA_DRIVE").is_some(),
            shoot: std::env::var("TODORA_FRAME")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(100),
            burst: std::env::var("TODORA_BURST")
                .ok()
                .and_then(|text| {
                    let (count, every) = text.split_once(':')?;
                    Some((count.parse().ok()?, every.parse().ok()?))
                })
                .unwrap_or((1, 1)),
            cam: std::env::var("TODORA_CAM").ok().and_then(|text| {
                let mut parts = text.split(':').map(|p| p.parse::<f32>().ok());
                Some((parts.next()??, parts.next()??, parts.next()??))
            }),
            zoom: std::env::var("TODORA_ZOOM")
                .ok()
                .and_then(|s| s.parse().ok()),
            behind: std::env::var("TODORA_BEHIND")
                .ok()
                .and_then(|s| s.parse().ok()),
            load: std::env::var("TODORA_LOAD").ok().map(|id| {
                all_circuits()
                    .iter()
                    .find(|c| c.id == id)
                    .expect("loading circuit")
            }),
            loading_frames: 0,
            frame: 0,
            preview: std::env::var_os("TODORA_PREVIEW").is_some(),
            time: std::env::var("TODORA_TIME")
                .ok()
                .and_then(|n| n.parse().ok()),
            started: None,
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
        .add_systems(PreUpdate, ai_drive.after(crate::input::InputSet))
        .add_systems(PreUpdate, open_screen.after(crate::settings::PageSet))
        .add_systems(PreUpdate, crate::local::preview)
        .add_systems(Update, (hold_summary, show_guide))
        .init_resource::<Spent>()
        .add_systems(First, clock_in)
        .add_systems(Last, clock_out)
        .add_systems(
            PostUpdate,
            capture.before(bevy::transform::TransformSystems::Propagate),
        );
}
/// The game's own plain driver, at the wheel.
#[allow(clippy::type_complexity)]
fn ai_drive(
    capture: Res<Capture>,
    track: Res<Track>,
    tweaks: Res<crate::fun::tweak::Tweaks>,
    mut driver: Local<
        Option<
            Box<
                dyn FnMut(&Track, &crate::car::Handling, &Transform, &Car) -> crate::car::Controls
                    + Send
                    + Sync,
            >,
        >,
    >,
    mut cars: Query<
        (
            &Transform,
            &Car,
            &crate::car::Handling,
            &mut crate::car::Controls,
        ),
        With<Player>,
    >,
) {
    if !capture.drive || capture.frame < 3 {
        return;
    }
    let driver = driver.get_or_insert_with(|| Box::new(crate::car::ai_driver()));
    for (at, car, handling, mut controls) in &mut cars {
        // The driver is told what the engine is told.
        *controls = driver(&track, &tweaks.apply(handling), at, car);
        if capture.frame.is_multiple_of(60) {
            info!(
                "frame {}: {:.1} m/s, tweak x{}, throttle {:.2}, brake {:.2}",
                capture.frame,
                car.velocity.length(),
                tweaks.speed,
                controls.throttle,
                controls.brake
            );
        }
    }
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
    if !capture.placed || (capture.drive && capture.frame > 2) {
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
#[allow(clippy::too_many_arguments)]
fn capture(
    mut commands: Commands,
    mut capture: ResMut<Capture>,
    cars: Query<&Transform, With<Player>>,
    mut cameras: Query<&mut Transform, (With<Camera3d>, Without<Player>)>,
    mut follow: Query<&mut crate::camera::FollowCam>,
    mut exit: MessageWriter<AppExit>,
    mut go: MessageWriter<crate::track::GoTo>,
    track: Res<Track>,
    halt: Res<crate::pause::Halt>,
    spent: Res<Spent>,
) {
    capture.frame += 1;
    if let Some(frames) = capture.time {
        const WARM: u32 = 240;
        if capture.frame == WARM {
            capture.started = Some(std::time::Instant::now());
        }
        if capture.frame == WARM + frames {
            let took = capture.started.map_or(0.0, |at| at.elapsed().as_secs_f32());
            println!(
                "TIMED {frames} frames in {took:.2} s: {:.2} ms a frame, {:.0} fps; \
                 the game's own schedules took {:.2} ms of each",
                took / frames as f32 * 1000.0,
                frames as f32 / took,
                spent.total / f64::from(spent.frames.max(1)) * 1000.0
            );
            exit.write(AppExit::Success);
        }
    }
    if let Some(circuit) = capture.load {
        if capture.frame == 60 {
            go.write(crate::track::GoTo(circuit));
        }
        if *halt == crate::pause::Halt::Loading {
            capture.loading_frames += 1;
        }
    }
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
    if let Some((yaw, pitch, distance)) = capture.cam
        && let (Ok(car), Ok(mut camera)) = (cars.single(), cameras.single_mut())
    {
        let behind = -crate::car::level(*car.forward());
        let flat = Quat::from_rotation_y(-yaw.to_radians()) * behind;
        let rise = pitch.to_radians();
        let target = car.translation + Vec3::Y * 0.55;
        let offset = (flat * rise.cos() + Vec3::Y * rise.sin()) * distance;
        *camera = Transform::from_translation(target + offset).looking_at(target, Vec3::Y);
    }
    let take_picture = if capture.load.is_some() {
        capture.loading_frames == 2 && *halt == crate::pause::Halt::Loading
    } else {
        capture.frame >= capture.shoot
            && (capture.frame - capture.shoot).is_multiple_of(capture.burst.1)
            && (capture.frame - capture.shoot) / capture.burst.1 < capture.burst.0
    };
    if take_picture {
        let path = if capture.burst.0 > 1 {
            let n = (capture.frame - capture.shoot) / capture.burst.1;
            capture.path.replace(".png", &format!("-{n:03}.png"))
        } else {
            capture.path.clone()
        };
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
    let last = capture.shoot + capture.burst.0 * capture.burst.1 + 40;
    if capture.frame > last.max(140) {
        if let Some(circuit) = capture.load {
            assert!(capture.frame < 36_000, "circuit loading never completed");
            if track.circuit().id != circuit.id || *halt == crate::pause::Halt::Loading {
                return;
            }
            assert!(capture.loading_frames > 1, "loading blocked the frame loop");
            info!(
                "Circuit loaded with {} responsive loading frames",
                capture.loading_frames
            );
        }
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

#[allow(clippy::too_many_arguments)]
fn open_screen(
    mut halt: ResMut<crate::pause::Halt>,
    mut page: ResMut<crate::settings::Page>,
    mut browse: ResMut<crate::online::Browse>,
    mut title: ResMut<crate::title::Title>,
    mut online: ResMut<crate::online::Online>,
    track: Res<Track>,
    challenge: Res<crate::challenge::Challenge>,
    mut menus: MessageWriter<crate::menu::OpenMenu>,
    mut menu: ResMut<crate::menu::Menu>,
    mut frames: Local<u32>,
    mut profiles: MessageWriter<crate::online::profile::OpenProfile>,
) {
    let screen = std::env::var("TODORA_SCREEN").unwrap_or_default();
    if screen == "profile" {
        *frames += 1;
        if *frames == 2 {
            profiles.write(crate::online::profile::OpenProfile(
                crate::pause::Halt::Title,
            ));
        }
    }
    let simple = match screen.as_str() {
        "title" => Some(crate::pause::Halt::Title),
        "replay" => Some(crate::pause::Halt::Replay),
        "offer" => Some(crate::pause::Halt::Offer),
        _ => None,
    };
    if let Some(wanted) = simple {
        *frames += 1;
        if *frames > 1 && *halt != wanted {
            *halt = wanted;
            title.active = wanted == crate::pause::Halt::Title;
        }
    }
    if screen == "circuits" || screen == "garage" {
        *frames += 1;
        if *frames == 4
            && screen == "circuits"
            && let Ok(value) = std::env::var("TODORA_COLLECTION")
        {
            let at = value.parse::<usize>().expect("collection index 0..4");
            assert!(at < 5, "collection index 0..4");
            menu.preview_collection(at);
        }
        if *frames == 2 {
            menus.write(crate::menu::OpenMenu(if screen == "circuits" {
                crate::menu::Page::Circuit
            } else {
                crate::menu::Page::Car
            }));
        }
    }
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
        // TODORA_EXAMPLE=21 fills the board as if you were 21st of 300.
        if *frames == 3
            && let Some(you) = std::env::var("TODORA_EXAMPLE")
                .ok()
                .and_then(|s| s.parse().ok())
        {
            let mut standing = crate::online::example(you);
            standing.circuit = track.circuit().id.to_string();
            standing.fetched_at = crate::online::client::unix_now();
            if number("TODORA_VIEW") == 3 {
                standing.week = Some(challenge.label());
            }
            online.board = Some(standing);
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
    if std::env::var_os("TODORA_TIME").is_some() {
        for mut window in &mut windows {
            window.present_mode = bevy::window::PresentMode::AutoNoVsync;
        }
    }
}
