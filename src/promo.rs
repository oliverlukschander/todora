//! Promotional footage, recorded from the game itself; absent from the
//! packaged game.
//!
//! `TODORA_PROMO=/absolute/shot.json cargo run --release --features promo`
//! renders one shot, frame by frame, into an image the size the shot asks
//! for, and pipes every frame to `ffmpeg`. Nothing is taken off the screen, so
//! a locked or covered display still records, and the clock advances exactly
//! one frame per frame, so a slow frame never becomes a stutter.
//!
//! The car is the player's own, driven by the game's plain AI driver through
//! the same engine and lap judge as a person at the keys; the HUD is the real
//! HUD. Cameras are either the game's own (chase, bonnet, cockpit, and the
//! replay's trackside cameras) or a rig described by the shot. The
//! capture is silent; start-light and chime timing is saved beside the video.
//!
//! `TODORA_PROMO_DATA=/absolute/folder` is where saved laps, settings and
//! achievements go instead of the player's own; the recording refuses to start
//! without it.
//! `marketing/promo/tools/capture.py` does that for every shot in a list.
use std::collections::BTreeMap;
use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::{
    camera::{ImageRenderTarget, Projection, RenderTarget},
    prelude::*,
    render::render_resource::{TextureFormat, TextureUsages},
    render::view::screenshot::{Screenshot, ScreenshotCaptured},
    time::TimeUpdateStrategy,
    window::PresentMode,
};
use serde::Deserialize;

use crate::car::{Car, Controls, Handling, Player, level};
use crate::track::{ROAD_HALF, Track, all_circuits};

/// Frames rendered, with the clock stopped, before anything is kept: the car
/// model, the sky and the textures load on other threads meanwhile.
const WARM_UP: u32 = 150;

#[derive(Deserialize, Clone)]
#[serde(default)]
struct Shot {
    circuit: String,
    /// The silent video to write; cue timings go beside it.
    out: String,
    width: u32,
    height: u32,
    /// Physical pixels per logical one, as on a Retina display: it is what
    /// sizes the HUD against the picture.
    scale_factor: f32,
    /// The HUD's own scale, if not the one the game picks for this size.
    ui_scale: Option<f32>,
    fps: u32,
    /// Seconds kept.
    seconds: f32,
    /// Seconds driven before the first kept frame.
    lead_in: f32,
    /// Where round the lap the car is put, 0 to 1; `None` is the grid, with
    /// the full countdown.
    progress: Option<f32>,
    /// Metres a second the car is put down at, along the road.
    speed: f32,
    /// Game time per second of footage: 0.5 is half speed.
    time_scale: f32,
    hud: bool,
    camera: Rig,
    /// Settings, as `settings.json` has them.
    settings: serde_json::Value,
    /// A constant-rate-factor for the intermediate; low is near lossless.
    crf: u32,
    /// Which AI driver has the wheel on each lap from the first: `plain`
    /// or `easy` (the plain driver never past 80% throttle); the
    /// last one keeps it. An easy lap then a plain one gives the plain one a
    /// best to beat, sector by sector.
    drivers: Vec<String>,
    /// The chase camera's zoom, as the scroll wheel sets it.
    zoom: Option<f32>,
    /// HUD panels left out when any of their text contains one of these, such
    /// as the key hints.
    hide: Vec<String>,
    /// Only the start lights, of the whole HUD.
    lights_only: bool,
}

impl Default for Shot {
    fn default() -> Self {
        Self {
            circuit: "suzuka".into(),
            out: "shot.mp4".into(),
            width: 1920,
            height: 1080,
            scale_factor: 2.0,
            ui_scale: None,
            fps: 60,
            seconds: 4.0,
            lead_in: 2.0,
            progress: Some(0.1),
            speed: 12.0,
            time_scale: 1.0,
            hud: true,
            camera: Rig::Game,
            settings: serde_json::json!({}),
            crf: 10,
            drivers: vec!["plain".into()],
            zoom: None,
            hide: Vec::new(),
            lights_only: false,
        }
    }
}

/// Where the camera is. Offsets are in the car's level frame — x to its
/// right, y up, z ahead — so a rig neither pitches nor rolls with the body.
#[derive(Deserialize, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Rig {
    /// The game's own camera, whichever view the settings chose.
    Game,
    /// The replay's trackside cameras.
    Trackside { fov: Option<f32> },
    /// Fixed to the car, moving from `from` to `to` over the shot, eased.
    Car {
        from: [f32; 3],
        to: Option<[f32; 3]>,
        look: [f32; 3],
        look_to: Option<[f32; 3]>,
        fov: Option<f32>,
        /// How quickly the rig swings round after the car's heading; 0 is rigid.
        lag: Option<f32>,
    },
    /// Standing beside the road `at` round the lap (0 to 1), `side` +1 right
    /// or −1 left, `out` metres beyond the road's edge and `up` above the
    /// ground, turning to follow the car.
    Stand {
        at: f32,
        side: f32,
        out: f32,
        up: f32,
        fov: Option<f32>,
        fov_to: Option<f32>,
        look: Option<[f32; 3]>,
    },
    /// Standing still where the car was when the warm-up ended, offset in
    /// its frame then, and turning to follow it (or looking at `look`, also
    /// in that frame, when `follow` is false).
    Anchor {
        at: [f32; 3],
        look: [f32; 3],
        follow: bool,
        fov: Option<f32>,
        fov_to: Option<f32>,
    },
    /// Round the car, `radius` out and `height` up, from `start` degrees
    /// (0 is dead ahead) turning `turn` degrees over the shot.
    Orbit {
        radius: f32,
        height: f32,
        start: f32,
        turn: f32,
        look: Option<[f32; 3]>,
        fov: Option<f32>,
    },
}

#[derive(Resource)]
struct Recorder {
    shot: Shot,
    frame: u32,
    kept: u32,
    total: u32,
    target: Option<Handle<Image>>,
    sink: Arc<Mutex<Sink>>,
    /// The rig's smoothed idea of which way the car points.
    heading: Option<Vec3>,
    /// Where the car stood, and which way it pointed, when the warm-up ended.
    anchor: Option<(Vec3, Vec3)>,
    /// The game clock when the warm-up ended.
    warm_up_time: f32,
    cues: Vec<(u32, crate::sound::Cue)>,
}

struct Sink {
    ffmpeg: Option<Child>,
    input: Option<ChildStdin>,
    next: u32,
    waiting: BTreeMap<u32, Vec<u8>>,
    size: UVec2,
}

impl Sink {
    fn take(&mut self, frame: u32, pixels: Vec<u8>) {
        self.waiting.insert(frame, pixels);
        while let Some(pixels) = self.waiting.remove(&self.next) {
            let expected = (self.size.x * self.size.y * 4) as usize;
            assert_eq!(
                pixels.len(),
                expected,
                "frame {} is the wrong size",
                self.next
            );
            if let Some(input) = self.input.as_mut() {
                input.write_all(&pixels).expect("ffmpeg takes the frame");
            }
            self.next += 1;
        }
    }
}

/// Without a window when recording: nothing waits on a display, and nothing
/// is shown on one.
pub fn plugins(plugins: bevy::app::PluginGroupBuilder) -> bevy::app::PluginGroupBuilder {
    if std::env::var_os("TODORA_PROMO").is_none() {
        return plugins;
    }
    plugins
        .set(bevy::window::WindowPlugin {
            primary_window: None,
            exit_condition: bevy::window::ExitCondition::DontExit,
            ..default()
        })
        .disable::<bevy::winit::WinitPlugin>()
        .add(bevy::app::ScheduleRunnerPlugin::run_loop(Duration::ZERO))
}

pub fn configure(app: &mut App) {
    let Ok(path) = std::env::var("TODORA_PROMO") else {
        return;
    };
    assert!(
        std::env::var_os("TODORA_PROMO_DATA").is_some(),
        "TODORA_PROMO_DATA keeps the recording's laps out of the player's own"
    );
    let text = std::fs::read_to_string(&path).expect("the shot file is readable");
    let shot: Shot = serde_json::from_str(&text).expect("the shot file is a shot");
    let circuit = all_circuits()
        .iter()
        .find(|c| c.id == shot.circuit)
        .expect("the shot's circuit");
    // Quiet, uncapped and as the defaults have it, apart from what the shot
    // asks for. The radio is never recorded.
    let mut settings = serde_json::json!({
        "music": false,
        "vsync": false,
        "fps_cap": 0,
        "antialiasing": true,
    });
    if let (Some(base), Some(extra)) = (settings.as_object_mut(), shot.settings.as_object()) {
        for (key, value) in extra {
            base.insert(key.clone(), value.clone());
        }
    }
    let fps = shot.fps.max(1);
    let total = (shot.seconds * fps as f32).round() as u32;
    let size = UVec2::new(shot.width, shot.height);
    let out = std::path::Path::new(&shot.out);
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).expect("the output folder can be made");
    }
    let mut ffmpeg = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-f", "rawvideo", "-pix_fmt", "rgba"])
        .args(["-s", &format!("{}x{}", size.x, size.y)])
        .args(["-r", &fps.to_string(), "-i", "-"])
        .args(["-c:v", "libx264", "-preset", "medium"])
        .args(["-crf", &shot.crf.to_string(), "-pix_fmt", "yuv444p"])
        .args([
            "-color_primaries",
            "bt709",
            "-color_trc",
            "bt709",
            "-colorspace",
            "bt709",
        ])
        .arg(&shot.out)
        .stdin(Stdio::piped())
        .spawn()
        .expect("ffmpeg is installed");
    let input = ffmpeg.stdin.take();
    let mut parsed = crate::settings::Settings::parse(&settings.to_string());
    parsed.bonkers = std::env::var_os("TODORA_BONKERS").is_some();
    let wild = crate::fun::Fun::of(&parsed).wild.level();
    let logical = Vec2::new(shot.width as f32, shot.height as f32) / shot.scale_factor;
    let ui_scale = shot
        .ui_scale
        .unwrap_or_else(|| crate::ui::fit(logical.x, logical.y))
        * shot.scale_factor;
    app.insert_resource(Track::with_wild(circuit, wild))
        .insert_resource(crate::track::WildRequest(wild))
        .insert_resource(crate::settings::ReadOnly)
        .insert_resource(crate::ui::CaptureScale(ui_scale))
        .insert_resource(parsed)
        .insert_resource(crate::car::Spec::default())
        .insert_resource(crate::car::Setup::default())
        .insert_resource(crate::car::Mode::default())
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO))
        .insert_resource(Recorder {
            shot,
            frame: 0,
            kept: 0,
            total,
            target: None,
            sink: Arc::new(Mutex::new(Sink {
                ffmpeg: Some(ffmpeg),
                input,
                next: 0,
                waiting: BTreeMap::new(),
                size,
            })),
            heading: None,
            anchor: None,
            warm_up_time: 0.0,
            cues: Vec::new(),
        })
        .add_systems(Startup, window)
        .add_systems(
            PreUpdate,
            (hold, clock)
                .after(crate::track::TrackSet)
                .after(crate::input::InputSet)
                .after(crate::lap::ClockSet)
                .after(crate::countdown::CountdownSet),
        )
        .add_systems(FixedUpdate, drive.before(crate::car::DriveSet))
        .add_systems(Update, (retarget, listen))
        .add_systems(
            PreUpdate,
            crate::visual_check::open_screen.after(crate::settings::PageSet),
        )
        .add_systems(PreUpdate, crate::local::preview)
        .add_systems(
            PostUpdate,
            (aim, keep)
                .chain()
                .before(bevy::transform::TransformSystems::Propagate),
        )
        .add_systems(
            PostUpdate,
            trim.before(bevy::camera::visibility::VisibilitySystems::VisibilityPropagate),
        );
}

/// An unshown window resource supplies layout dimensions; rendering is offscreen.
fn window(mut commands: Commands, recorder: Res<Recorder>) {
    // A virtual window supplies layout dimensions to the split-screen cameras.
    // Winit is disabled, so no OS window is created.
    commands.spawn(Window {
        resolution: bevy::window::WindowResolution::new(recorder.shot.width, recorder.shot.height),
        present_mode: PresentMode::AutoNoVsync,
        ..default()
    });
}

/// Point the 3D camera, and the HUD with it, at the recording's image once the
/// render-scale setting has had its first say about where the camera draws.
fn retarget(
    mut commands: Commands,
    mut recorder: ResMut<Recorder>,
    mut images: ResMut<Assets<Image>>,
    cameras: Query<(Entity, Option<&RenderTarget>), With<Camera>>,
) {
    let shot = recorder.shot.clone();
    if recorder.frame == 3 {
        let mut image =
            Image::new_target_texture(shot.width, shot.height, TextureFormat::Rgba8UnormSrgb, None);
        image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        let handle = images.add(image);
        recorder.target = Some(handle);
    }
    if let Some(handle) = &recorder.target {
        for (camera, target) in &cameras {
            if !matches!(target, Some(RenderTarget::Image(image)) if image.handle == *handle) {
                let mut entity = commands.entity(camera);
                entity.insert(RenderTarget::Image(ImageRenderTarget {
                    handle: handle.clone(),
                    scale_factor: 1.0,
                }));
                // Split-screen supplies its own UI target on each panel.
                if shot.hud && std::env::var("TODORA_SCREEN").as_deref() != Ok("local") {
                    entity.insert(IsDefaultUiCamera);
                }
            }
        }
    }
}

/// Before the first kept frame the clock is stopped and the car is held where
/// the shot puts it; after it, one frame of footage is `time_scale` frames of
/// game time.
fn clock(
    mut recorder: ResMut<Recorder>,
    time: Res<Time<Virtual>>,
    mut strategy: ResMut<TimeUpdateStrategy>,
    mut follow: Query<&mut crate::camera::FollowCam>,
) {
    if recorder.frame == WARM_UP {
        recorder.warm_up_time = time.elapsed_secs();
    }
    if let Some(zoom) = recorder.shot.zoom {
        for mut camera in &mut follow {
            camera.zoom = zoom;
        }
    }
    let step = if recorder.frame < WARM_UP {
        // Real time passes for the loaders meanwhile.
        std::thread::sleep(Duration::from_millis(12));
        Duration::ZERO
    } else {
        Duration::from_secs_f64(
            f64::from(recorder.shot.time_scale) / f64::from(recorder.shot.fps.max(1)),
        )
    };
    *strategy = TimeUpdateStrategy::ManualDuration(step);
}

fn hold(
    recorder: Res<Recorder>,
    track: Res<Track>,
    mut cars: Query<(&mut Transform, &mut Car), With<Player>>,
    mut start: ResMut<crate::countdown::Start>,
) {
    if recorder.frame >= WARM_UP {
        return;
    }
    let Some(progress) = recorder.shot.progress else {
        // On the grid, with the lights still to come.
        start.elapsed = 0.0;
        return;
    };
    start.elapsed = f32::MAX;
    let points: Vec<_> = track.map_points().collect();
    let index = ((points.len() as f32 * progress) as usize).min(points.len() - 1);
    let (pos, along) = points[index];
    let ahead = (points[(index + 1) % points.len()].0 - pos).normalize();
    if let Ok((mut at, mut car)) = cars.single_mut() {
        let scale = at.scale;
        *at = Transform::from_translation(pos)
            .looking_to(ahead, Vec3::Y)
            .with_scale(scale);
        car.along = Some(along);
        car.velocity = level(ahead) * recorder.shot.speed;
        car.yaw_rate = 0.0;
    }
}

type Brain = Box<dyn FnMut(&Track, &Handling, &Transform, &Car) -> Controls + Send + Sync>;

/// The AI driver the shot names for this lap has the wheel, once per physics
/// step.
fn drive(
    recorder: Res<Recorder>,
    track: Res<Track>,
    timer: Res<crate::lap::LapTimer>,
    time: Res<Time>,
    mut cars: Query<(&Transform, &Car, &Handling, &mut Controls), With<Player>>,
    mut driver: Local<Option<(String, Brain)>>,
    mut laps: Local<u32>,
) {
    if recorder.frame < WARM_UP {
        return;
    }
    if timer.completed != *laps {
        *laps = timer.completed;
        info!(
            "Lap {} done at {:.3} s of game time: last {:?}, best {:?}, splits {:?}",
            timer.completed,
            time.elapsed_secs() - recorder.warm_up_time,
            timer.last,
            timer.best,
            timer
                .report
                .as_ref()
                .map(|r| (r.valid, r.best, r.splits.clone(), r.why))
        );
    }
    let drivers = &recorder.shot.drivers;
    let wanted = drivers
        .get(timer.completed as usize)
        .or(drivers.last())
        .cloned()
        .unwrap_or_else(|| "plain".into());
    if driver.as_ref().is_none_or(|(name, _)| *name != wanted) {
        let brain: Brain = Box::new(crate::car::ai_driver());
        *driver = Some((wanted, brain));
    }
    let Some((name, brain)) = driver.as_mut() else {
        return;
    };
    for (at, car, handling, mut controls) in &mut cars {
        *controls = brain(&track, handling, at, car);
        if name == "easy" {
            controls.throttle = controls.throttle.min(0.8);
        }
    }
}

fn listen(
    mut recorder: ResMut<Recorder>,
    mut lights: MessageReader<crate::countdown::StartLight>,
    mut cues: MessageReader<crate::sound::Cue>,
) {
    let heard: Vec<_> = lights
        .read()
        .map(|light| {
            if light.go {
                crate::sound::Cue::Go
            } else {
                crate::sound::Cue::Red
            }
        })
        .chain(cues.read().copied())
        .collect();
    if let Some(kept) = recorder.frame.checked_sub(first_kept(&recorder.shot)) {
        for cue in heard {
            recorder.cues.push((kept, cue));
        }
    }
}

fn first_kept(shot: &Shot) -> u32 {
    WARM_UP + (shot.lead_in * shot.fps as f32 / shot.time_scale.max(0.01)).round() as u32
}

/// Put the camera where the shot's rig says.
#[allow(clippy::type_complexity)]
fn aim(
    mut recorder: ResMut<Recorder>,
    track: Res<Track>,
    cars: Query<(&Transform, &Car), With<Player>>,
    mut cameras: Query<(&mut Transform, &mut Projection), (With<Camera3d>, Without<Player>)>,
    mut trackside: Local<Vec<(Vec3, f32)>>,
) {
    let Ok((car, state)) = cars.single() else {
        return;
    };
    let Ok((mut camera, mut projection)) = cameras.single_mut() else {
        return;
    };
    let first = first_kept(&recorder.shot);
    let t = if recorder.total == 0 {
        0.0
    } else {
        (recorder.frame.saturating_sub(first) as f32 / recorder.total as f32).clamp(0.0, 1.0)
    };
    let eased = t * t * (3.0 - 2.0 * t);
    let dt = recorder.shot.time_scale / recorder.shot.fps.max(1) as f32;
    let ahead = level(*car.forward());
    let heading = match (recorder.heading, &recorder.shot.camera) {
        (Some(was), Rig::Car { lag: Some(lag), .. }) if *lag > 0.0 && recorder.frame > WARM_UP => {
            was.lerp(ahead, 1.0 - (-dt / lag).exp()).normalize()
        }
        _ => ahead,
    };
    recorder.heading = Some(heading);
    if recorder.frame <= WARM_UP {
        recorder.anchor = Some((car.translation, ahead));
    }
    let right = heading.cross(Vec3::Y);
    let frame = |o: [f32; 3]| right * o[0] + Vec3::Y * o[1] + heading * o[2];
    let mut fov = None;
    match recorder.shot.camera.clone() {
        Rig::Game => {}
        Rig::Trackside { fov: wanted } => {
            if trackside.is_empty() {
                *trackside = crate::replay::trackside_cameras(&track);
            }
            let s = state.along.unwrap_or(0.0);
            if let Some(at) = crate::replay::camera_for(&trackside, s, track.length()) {
                camera.translation = at;
                camera.look_at(car.translation + Vec3::Y * 0.3, Vec3::Y);
            }
            fov = wanted;
        }
        Rig::Car {
            from,
            to,
            look,
            look_to,
            fov: wanted,
            ..
        } => {
            let offset = Vec3::from(from).lerp(Vec3::from(to.unwrap_or(from)), eased);
            let aim = Vec3::from(look).lerp(Vec3::from(look_to.unwrap_or(look)), eased);
            camera.translation = clear(&track, state, car.translation + frame(offset.into()));
            camera.look_at(car.translation + frame(aim.into()), Vec3::Y);
            fov = wanted;
        }
        Rig::Stand {
            at,
            side,
            out,
            up,
            fov: wanted,
            fov_to,
            look,
        } => {
            let points: Vec<_> = track.map_points().collect();
            let index = ((points.len() as f32 * at.rem_euclid(1.0)) as usize).min(points.len() - 1);
            let (point, s) = points[index];
            let ground = track.ground_from(point, Some(s));
            let mut eye = ground.centre + ground.right * side * (ROAD_HALF + out);
            eye.y = track.ground_from(eye, Some(s)).height.max(ground.centre.y) + up;
            camera.translation = eye;
            camera.look_at(
                car.translation + frame(look.unwrap_or([0.0, 0.25, 0.0])),
                Vec3::Y,
            );
            fov = wanted.map(|a| a + (fov_to.unwrap_or(a) - a) * eased);
        }
        Rig::Anchor {
            at,
            look,
            follow,
            fov: wanted,
            fov_to,
        } => {
            let (origin, forward) = recorder.anchor.unwrap_or((car.translation, ahead));
            let side = forward.cross(Vec3::Y);
            let place = |o: [f32; 3]| origin + side * o[0] + Vec3::Y * o[1] + forward * o[2];
            camera.translation = clear(&track, state, place(at));
            let target = if follow {
                car.translation + frame(look)
            } else {
                place(look)
            };
            camera.look_at(target, Vec3::Y);
            fov = wanted.map(|a| a + (fov_to.unwrap_or(a) - a) * eased);
        }
        Rig::Orbit {
            radius,
            height,
            start,
            turn,
            look,
            fov: wanted,
        } => {
            let angle = (start + turn * eased).to_radians();
            let offset = [angle.sin() * radius, height, angle.cos() * radius];
            camera.translation = clear(&track, state, car.translation + frame(offset));
            camera.look_at(
                car.translation + frame(look.unwrap_or([0.0, 0.2, 0.0])),
                Vec3::Y,
            );
            fov = wanted;
        }
    }
    if let (Some(degrees), Projection::Perspective(perspective)) = (fov, projection.as_mut()) {
        perspective.fov = degrees.to_radians();
        perspective.near = 0.02;
    }
}

/// Leave out the HUD panels the shot does not want.
#[allow(clippy::type_complexity)]
fn trim(
    recorder: Res<Recorder>,
    mut roots: Query<
        (Entity, &mut Visibility, Has<crate::countdown::Pod>),
        (With<Node>, Without<ChildOf>),
    >,
    children: Query<&Children>,
    texts: Query<&Text>,
) {
    let shot = &recorder.shot;
    if shot.hud && shot.hide.is_empty() && !shot.lights_only {
        return;
    }
    for (root, mut visibility, lights) in &mut roots {
        let unwanted = if !shot.hud {
            true
        } else if shot.lights_only {
            !lights
        } else {
            std::iter::once(root)
                .chain(children.iter_descendants(root))
                .filter_map(|e| texts.get(e).ok())
                .any(|text| shot.hide.iter().any(|h| text.0.contains(h.as_str())))
        };
        if unwanted && *visibility != Visibility::Hidden {
            *visibility = Visibility::Hidden;
        }
    }
}

/// Where `eye` is, raised if need be to stand clear of the ground beneath it:
/// a low rig on the outside of a banked corner would otherwise look up
/// through the grass.
fn clear(track: &Track, car: &Car, eye: Vec3) -> Vec3 {
    let ground = track.ground_from(eye, car.along).height;
    eye.with_y(eye.y.max(ground + 0.15))
}

/// Keep the frame, once the warm-up and lead-in are over, and finish when the
/// last one is in.
#[allow(clippy::type_complexity)]
fn keep(mut commands: Commands, mut recorder: ResMut<Recorder>, mut exit: MessageWriter<AppExit>) {
    recorder.frame += 1;
    let first = first_kept(&recorder.shot);
    let Some(target) = recorder.target.clone() else {
        return;
    };
    if recorder.frame >= first && recorder.kept < recorder.total {
        let index = recorder.kept;
        recorder.kept += 1;
        let sink = recorder.sink.clone();
        commands.spawn(Screenshot::image(target)).observe(
            move |captured: On<ScreenshotCaptured>| {
                let pixels = captured
                    .image
                    .data
                    .clone()
                    .expect("a captured frame has pixels");
                sink.lock().unwrap().take(index, pixels);
            },
        );
    }
    if recorder.kept >= recorder.total {
        let done = recorder.sink.lock().unwrap().next >= recorder.total;
        if done {
            finish(&mut recorder);
            exit.write(AppExit::Success);
        } else {
            assert!(
                recorder.frame < first + recorder.total + 600,
                "frames stopped arriving"
            );
        }
    }
}

fn finish(recorder: &mut Recorder) {
    let mut sink = recorder.sink.lock().unwrap();
    drop(sink.input.take());
    if let Some(mut ffmpeg) = sink.ffmpeg.take() {
        let status = ffmpeg.wait().expect("ffmpeg finishes");
        assert!(status.success(), "ffmpeg failed: {status}");
    }
    let shot = &recorder.shot;
    // When each beep or chime began, in kept frames, for the edit.
    let cues: Vec<_> = recorder
        .cues
        .iter()
        .map(|(frame, cue)| serde_json::json!({ "frame": frame, "cue": format!("{cue:?}") }))
        .collect();
    std::fs::write(
        std::path::Path::new(&shot.out).with_extension("cues.json"),
        serde_json::to_string_pretty(&cues).expect("cues are JSON"),
    )
    .expect("the cue file is writable");
    info!(
        "Recorded {} frames of {} to {}",
        recorder.total, shot.circuit, shot.out
    );
}
