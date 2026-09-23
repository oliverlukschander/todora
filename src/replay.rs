//! Watching a lap again, and taking a picture of it.
//!
//! **Replay** in the pause menu plays a lap — your best, your last lap that
//! counted, or a downloaded ghost's (**Q / E** or **LB / RB** change which) —
//! with a car of its own, while the real car waits where it was paused. **A /
//! Enter** plays and pauses, **← →** scrub, **↑ ↓** set the speed from a quarter
//! to four times, and **Y** changes camera: TV-style cameras placed along the
//! circuit, the chase camera, or the bonnet. **X / P** is photo mode: a free
//! camera within 15 m of the car (**WASD** or the left stick to move, the arrows
//! or the right stick to look, **Q / E** or the triggers for height), the HUD
//! gone, and **Enter / A** saves a PNG to `Pictures/Todora`. **Esc / B** goes
//! back, from photo mode to the replay and from the replay to the pause.
//!
//! The replay runs on the real clock, because the game's is stopped; nothing
//! of the lap in progress moves.

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use crate::car::{MODEL, SCALE, level};
use crate::ghost::Recording;
use crate::hud::{AMBER_DIM, FRONT};
use crate::lap::format_time;
use crate::pause::{Halt, HaltSet};
use crate::track::Track;
use crate::ui::{LINE, TEXT, label};

/// Distance between trackside cameras, and how far out and up they stand.
const CAMERA_EVERY: f32 = 70.0;
const CAMERA_OUT: f32 = 7.0;
const CAMERA_UP: f32 = 2.6;
/// How far the photo camera may wander from the car.
const PHOTO_REACH: f32 = 15.0;
const SPEEDS: [f32; 5] = [0.25, 0.5, 1.0, 2.0, 4.0];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Source {
    #[default]
    Best,
    Last,
    Downloaded,
}

impl Source {
    const ALL: [Self; 3] = [Self::Best, Self::Last, Self::Downloaded];
    fn name(self) -> &'static str {
        match self {
            Self::Best => "Your best",
            Self::Last => "Your last lap",
            Self::Downloaded => "Downloaded ghost",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum View {
    #[default]
    Trackside,
    Chase,
    Bonnet,
}

impl View {
    fn next(self) -> Self {
        match self {
            Self::Trackside => Self::Chase,
            Self::Chase => Self::Bonnet,
            Self::Bonnet => Self::Trackside,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Trackside => "Trackside",
            Self::Chase => "Chase",
            Self::Bonnet => "Bonnet",
        }
    }
}

/// The replay's state.
#[derive(Resource, Default)]
pub(crate) struct Replay {
    source: Source,
    view: View,
    clock: f32,
    playing: bool,
    speed: usize,
    photo: bool,
    /// Where the photo camera is, and which way it looks.
    eye: Vec3,
    yaw: f32,
    pitch: f32,
    /// Trackside cameras, and how far round the lap each stands.
    cameras: Vec<(Vec3, f32)>,
    /// The camera where the replay opened, to put back.
    before: Option<Transform>,
    /// A picture is due next frame, once the hints are hidden.
    shoot: bool,
    car: Option<Entity>,
}

/// One trackside camera every [`CAMERA_EVERY`] metres, alternating sides,
/// standing clear of the road and above the ground beneath it.
pub(crate) fn trackside_cameras(track: &Track) -> Vec<(Vec3, f32)> {
    let mut out = Vec::new();
    let mut next = 0.0;
    let mut side = 1.0;
    for (point, s) in track.map_points() {
        if s < next {
            continue;
        }
        next = s + CAMERA_EVERY;
        let ground = track.ground_from(point, Some(s));
        let mut at = ground.centre + ground.right * side * (crate::track::ROAD_HALF + CAMERA_OUT);
        at.y = track.ground_from(at, Some(s)).height.max(ground.centre.y) + CAMERA_UP;
        out.push((at, s));
        side = -side;
    }
    out
}

/// The trackside camera for a car `s` metres round a lap of `length`: the one
/// it is nearest to, looking ahead half a spacing so each camera sees the car
/// come and go.
pub(crate) fn camera_for(cameras: &[(Vec3, f32)], s: f32, length: f32) -> Option<Vec3> {
    let s = (s + CAMERA_EVERY / 2.0).rem_euclid(length);
    cameras
        .iter()
        .rev()
        .find(|(_, at)| *at <= s)
        .or(cameras.last())
        .map(|(pos, _)| *pos)
}

pub struct ReplayPlugin;

impl Plugin for ReplayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Replay>()
            .add_systems(Startup, setup)
            .add_systems(PreUpdate, drive.after(HaltSet))
            .add_systems(Update, (play, draw).chain().after(crate::ghost::GhostSet));
    }
}

#[derive(Component)]
struct Panel;
#[derive(Component)]
struct Status;
#[derive(Component)]
struct Hint;
#[derive(Component)]
struct ReplayCar;

fn setup(mut commands: Commands, asset_server: Res<AssetServer>, mut replay: ResMut<Replay>) {
    replay.speed = 2;
    replay.car = Some(
        commands
            .spawn((
                ReplayCar,
                Transform::from_scale(Vec3::splat(SCALE)),
                Visibility::Hidden,
                WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(MODEL))),
            ))
            .id(),
    );
    commands
        .spawn((
            Panel,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(24),
                left: px(0),
                right: px(0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    padding: UiRect::axes(px(20), px(12)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(12)),
                    ..default()
                },
                BackgroundColor(FRONT),
                BorderColor::all(LINE),
            ))
            .with_children(|panel| {
                panel.spawn((Status, label("", 20.0, TEXT)));
                panel.spawn((Hint, label("", 13.0, AMBER_DIM)));
            });
        });
}

fn recording<'a>(
    source: Source,
    ghost: &'a crate::ghost::Ghost,
    rival: Option<&'a crate::ghost::Rival>,
) -> Option<&'a Recording> {
    match source {
        Source::Best => ghost.best_lap(),
        Source::Last => ghost.last_lap(),
        Source::Downloaded => rival.and_then(|r| r.lap()),
    }
}

#[allow(clippy::too_many_arguments)]
fn drive(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    time: Res<Time<Real>>,
    track: Res<Track>,
    mut halt: ResMut<Halt>,
    mut replay: ResMut<Replay>,
    cameras: Query<&Transform, With<Camera3d>>,
) {
    if *halt != Halt::Replay {
        return;
    }
    if halt.is_changed() {
        replay.clock = 0.0;
        replay.playing = true;
        replay.photo = false;
        replay.cameras = trackside_cameras(&track);
        replay.before = cameras.single().ok().copied();
        return;
    }
    let pad = |button| pads.iter().any(|pad| pad.just_pressed(button));
    let back = keys.just_pressed(KeyCode::Escape) || pad(GamepadButton::East);
    let dt = time.delta_secs();
    if replay.photo {
        if back {
            replay.photo = false;
            return;
        }
        if keys.just_pressed(KeyCode::Enter) || pad(GamepadButton::South) {
            replay.shoot = true;
        }
        let axis = |minus: KeyCode, plus: KeyCode| {
            f32::from(u8::from(keys.pressed(plus))) - f32::from(u8::from(keys.pressed(minus)))
        };
        let stick = pads.iter().next().map_or((Vec2::ZERO, Vec2::ZERO), |p| {
            (p.left_stick(), p.right_stick())
        });
        let lift = pads.iter().next().map_or(0.0, |p| {
            p.get(GamepadButton::RightTrigger2).unwrap_or(0.0)
                - p.get(GamepadButton::LeftTrigger2).unwrap_or(0.0)
        });
        let walk = Vec2::new(
            axis(KeyCode::KeyA, KeyCode::KeyD),
            axis(KeyCode::KeyS, KeyCode::KeyW),
        ) + stick.0;
        let look = Vec2::new(
            axis(KeyCode::ArrowLeft, KeyCode::ArrowRight),
            axis(KeyCode::ArrowDown, KeyCode::ArrowUp),
        ) + stick.1;
        replay.yaw -= look.x * 1.6 * dt;
        replay.pitch = (replay.pitch + look.y * 1.2 * dt).clamp(-1.3, 1.3);
        let forward = Quat::from_rotation_y(replay.yaw) * Vec3::NEG_Z;
        let right = forward.cross(Vec3::Y);
        let up = axis(KeyCode::KeyQ, KeyCode::KeyE) + lift;
        let step = (forward * walk.y + right * walk.x + Vec3::Y * up) * 5.0 * dt;
        replay.eye += step;
        return;
    }
    if back {
        *halt = Halt::Pause;
        return;
    }
    let step = |code, button| keys.just_pressed(code) || pad(button);
    let source_step = i32::from(step(KeyCode::KeyE, GamepadButton::RightTrigger))
        - i32::from(step(KeyCode::KeyQ, GamepadButton::LeftTrigger));
    if source_step != 0 {
        let at = Source::ALL
            .iter()
            .position(|s| *s == replay.source)
            .unwrap_or(0) as i32;
        replay.source =
            Source::ALL[(at + source_step).rem_euclid(Source::ALL.len() as i32) as usize];
        replay.clock = 0.0;
    }
    if step(KeyCode::Enter, GamepadButton::South) {
        replay.playing = !replay.playing;
    }
    if step(KeyCode::KeyY, GamepadButton::North) {
        replay.view = replay.view.next();
    }
    if step(KeyCode::ArrowUp, GamepadButton::DPadUp) {
        replay.speed = (replay.speed + 1).min(SPEEDS.len() - 1);
    }
    if step(KeyCode::ArrowDown, GamepadButton::DPadDown) {
        replay.speed = replay.speed.saturating_sub(1);
    }
    let scrub = f32::from(u8::from(
        keys.pressed(KeyCode::ArrowRight)
            || pads.iter().any(|p| p.pressed(GamepadButton::DPadRight)),
    )) - f32::from(u8::from(
        keys.pressed(KeyCode::ArrowLeft) || pads.iter().any(|p| p.pressed(GamepadButton::DPadLeft)),
    ));
    replay.clock += scrub * 4.0 * dt;
    if step(KeyCode::KeyX, GamepadButton::West) || keys.just_pressed(KeyCode::KeyP) {
        replay.photo = true;
        replay.playing = false;
        if let Ok(camera) = cameras.single() {
            replay.eye = camera.translation;
            let (yaw, pitch, _) = camera.rotation.to_euler(EulerRot::YXZ);
            replay.yaw = yaw;
            replay.pitch = pitch;
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn play(
    mut commands: Commands,
    time: Res<Time<Real>>,
    halt: Res<Halt>,
    track: Res<Track>,
    ghost: Res<crate::ghost::Ghost>,
    rival: Option<Res<crate::ghost::Rival>>,
    mut replay: ResMut<Replay>,
    mut cars: Query<(&mut Transform, &mut Visibility), (With<ReplayCar>, Without<Camera3d>)>,
    mut cameras: Query<&mut Transform, (With<Camera3d>, Without<ReplayCar>)>,
) {
    let open = *halt == Halt::Replay;
    let Some(car) = replay.car else {
        return;
    };
    let Ok((mut at, mut visibility)) = cars.get_mut(car) else {
        return;
    };
    if !open {
        visibility.set_if_neq(Visibility::Hidden);
        if let (Some(before), Ok(mut camera)) = (replay.before.take(), cameras.single_mut()) {
            *camera = before;
        }
        return;
    }
    let lap = recording(replay.source, &ghost, rival.as_deref());
    let Some(lap) = lap else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    let duration = lap.duration().max(0.01);
    if replay.playing {
        replay.clock += time.delta_secs() * SPEEDS[replay.speed];
    }
    replay.clock = replay.clock.rem_euclid(duration);
    let Some((position, rotation)) = lap.pose_at(replay.clock) else {
        return;
    };
    at.translation = position;
    at.rotation = rotation;
    visibility.set_if_neq(Visibility::Visible);
    let Ok(mut camera) = cameras.single_mut() else {
        return;
    };
    if replay.photo {
        let offset = replay.eye - position;
        if offset.length() > PHOTO_REACH {
            replay.eye = position + offset.normalize() * PHOTO_REACH;
        }
        let floor = track.ground_from(replay.eye, None).height + 0.3;
        replay.eye.y = replay.eye.y.max(floor);
        camera.translation = replay.eye;
        camera.rotation = Quat::from_euler(EulerRot::YXZ, replay.yaw, replay.pitch, 0.0);
        if replay.shoot {
            replay.shoot = false;
            if let Some(folder) = pictures() {
                let _ = std::fs::create_dir_all(&folder);
                let name = format!("todora-{}.png", crate::online::client::unix_now());
                commands
                    .spawn(Screenshot::primary_window())
                    .observe(save_to_disk(folder.join(name)));
            }
        }
        return;
    }
    let ahead = level(rotation * Vec3::NEG_Z);
    match replay.view {
        View::Trackside => {
            let s = track.ground_from(position, None).s;
            if let Some(eye) = camera_for(&replay.cameras, s, track.length()) {
                camera.translation = eye;
                camera.look_at(position + Vec3::Y * 0.3, Vec3::Y);
            }
        }
        View::Chase => {
            camera.translation = position - ahead * 6.0 + Vec3::Y * 2.8;
            camera.look_at(position + ahead * 1.8 + Vec3::Y * 0.6, Vec3::Y);
        }
        View::Bonnet => {
            let eye = position + ahead * 0.22 + Vec3::Y * 0.34;
            camera.translation = eye;
            camera.look_at(eye + ahead * 12.0 - Vec3::Y * 0.4, Vec3::Y);
        }
    }
}

/// Where pictures go: the system's Pictures folder, in a Todora folder.
fn pictures() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
    Some(home.join("Pictures").join("Todora"))
}

#[allow(clippy::type_complexity)]
fn draw(
    halt: Res<Halt>,
    replay: Res<Replay>,
    ghost: Res<crate::ghost::Ghost>,
    rival: Option<Res<crate::ghost::Rival>>,
    mut panels: Query<&mut Visibility, With<Panel>>,
    mut statuses: Query<&mut Text, (With<Status>, Without<Hint>)>,
    mut hints: Query<&mut Text, (With<Hint>, Without<Status>)>,
) {
    let open = *halt == Halt::Replay && !(replay.photo && replay.shoot);
    for mut visibility in &mut panels {
        visibility.set_if_neq(if open && !replay.photo {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
    if !open {
        return;
    }
    let lap = recording(replay.source, &ghost, rival.as_deref());
    let status = match lap {
        Some(lap) => format!(
            "{}    {} / {}    {}×    {}{}",
            replay.source.name(),
            format_time(replay.clock),
            format_time(lap.duration()),
            SPEEDS[replay.speed],
            replay.view.name(),
            if replay.playing { "" } else { "    PAUSED" }
        ),
        None => format!("{}    — nothing to replay yet", replay.source.name()),
    };
    if let Ok(mut text) = statuses.single_mut()
        && text.0 != status
    {
        text.0 = status;
    }
    let hint = "A / Enter  Play    ← →  Scrub    ↑ ↓  Speed    Q / E  Lap    Y  Camera    X / P  Photo    Esc / B  Back";
    if let Ok(mut text) = hints.single_mut()
        && text.0 != hint
    {
        text.0 = hint.into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trackside_cameras_line_the_whole_lap_and_one_always_sees_the_car() {
        let track = Track::any();
        let cameras = trackside_cameras(&track);
        let expected = (track.length() / CAMERA_EVERY).ceil() as usize;
        assert!(
            (expected - 1..=expected + 1).contains(&cameras.len()),
            "{} cameras",
            cameras.len()
        );
        for (at, s) in &cameras {
            let ground = track.ground_from(*at, Some(*s));
            assert!(
                ground.lateral.abs() > crate::track::ROAD_HALF,
                "a camera on the road"
            );
            assert!(at.y > ground.height, "a camera under the ground");
        }
        for step in 0..100 {
            let s = track.length() * step as f32 / 100.0;
            let eye = camera_for(&cameras, s, track.length()).unwrap();
            let car = track
                .map_points()
                .min_by(|a, b| (a.1 - s).abs().total_cmp(&(b.1 - s).abs()))
                .unwrap()
                .0;
            assert!(
                eye.distance(car) < CAMERA_EVERY * 1.2 + CAMERA_OUT + 10.0,
                "a camera far from the car"
            );
        }
    }
}
