//! Couch competition: distinct inputs, cameras and clocks; no account required.
mod view;
use crate::{
    Reset,
    car::{Car, Controls, Handling, Mode, Setup, Spec},
    lap::{LapTimer, Step},
    pause::Halt,
    track::Track,
};
use bevy::{camera::Viewport, prelude::*};

pub(crate) struct LocalPlugin;
impl Plugin for LocalPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalRace>()
            .add_message::<OpenLocal>()
            .add_message::<Action>()
            .add_systems(
                PreUpdate,
                (view::click, open, lobby, actions, inputs)
                    .chain()
                    .after(crate::pause::HaltSet)
                    .after(bevy::ui::UiSystems::Focus)
                    .before(crate::input::InputSet),
            )
            .add_systems(PreUpdate, reset.after(crate::car::CarResetSet))
            .add_systems(
                FixedUpdate,
                hold.before(crate::car::DriveSet)
                    .after(crate::countdown::CountdownSet),
            )
            .add_systems(FixedUpdate, clocks.after(crate::car::DriveSet))
            .add_systems(Update, (cameras, view::draw, view::hud).chain());
    }
}
#[derive(Resource, Default)]
pub(crate) struct LocalRace {
    pub active: bool,
    devices: Vec<Device>,
    circuit: usize,
    ghost_was_on: bool,
    rival_was_on: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Device {
    Wasd,
    Arrows,
    Pad(Entity),
}
impl Device {
    fn name(self, pads: &Query<&Gamepad>) -> String {
        match self {
            Self::Wasd => "W A S D · Space".into(),
            Self::Arrows => "↑ ↓ ← → · Right Ctrl".into(),
            Self::Pad(e) => pads.get(e).map_or_else(
                |_| crate::text::t("local.disconnected").into(),
                |_| "Gamepad".into(),
            ),
        }
    }
}
#[derive(Message)]
pub(crate) struct OpenLocal;
#[derive(Message, Clone, Copy)]
pub(crate) enum Action {
    Start,
    Back,
    Leave,
    Resume,
    Again,
    Previous,
    Next,
    JoinWasd,
    JoinArrows,
    Remove(usize),
}
#[derive(Component)]
pub(crate) struct Seat {
    pub(crate) index: usize,
    device: Device,
    timer: LapTimer,
}
#[derive(Component)]
pub(crate) struct Parked;
#[derive(Component)]
struct LocalEntity;
#[derive(Component)]
pub(crate) struct LocalCamera(usize);

pub(crate) fn solo(race: Option<Res<LocalRace>>) -> bool {
    !race.is_some_and(|r| r.active)
}
pub(crate) fn active(race: Option<&LocalRace>) -> bool {
    race.is_some_and(|r| r.active)
}
const COLOURS: [Color; 4] = [
    crate::ui::ACCENT,
    Color::srgb(0.30, 0.70, 1.0),
    Color::srgb(1.0, 0.49, 0.30),
    Color::srgb(0.83, 0.55, 1.0),
];

fn open(
    mut requests: MessageReader<OpenLocal>,
    mut race: ResMut<LocalRace>,
    track: Res<Track>,
    mut halt: ResMut<Halt>,
    session: Res<crate::multiplayer::Session>,
    mut guide: ResMut<crate::onboarding::Guide>,
) {
    if requests.read().next().is_some() && !session.active() {
        guide.dismiss();
        race.devices.clear();
        race.circuit = crate::track::circuit_at(track.circuit());
        *halt = Halt::LocalLobby;
    }
}
fn join(devices: &mut Vec<Device>, device: Device) {
    if devices.len() < 4 && !devices.contains(&device) {
        devices.push(device);
    }
}
fn lobby(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<(Entity, &Gamepad)>,
    mut race: ResMut<LocalRace>,
    mut halt: ResMut<Halt>,
    mut actions: MessageWriter<Action>,
) {
    if halt.is_changed() {
        return;
    }
    if race.active {
        if *halt == Halt::Nothing {
            let disconnected = race
                .devices
                .iter()
                .any(|d| matches!(d, Device::Pad(e) if pads.get(*e).is_err()));
            if disconnected
                || keys.just_pressed(KeyCode::Escape)
                || pads
                    .iter()
                    .any(|(_, p)| p.just_pressed(GamepadButton::Start))
            {
                *halt = Halt::LocalPause;
            }
        } else if *halt == Halt::LocalPause {
            if keys.just_pressed(KeyCode::Escape)
                || pads
                    .iter()
                    .any(|(_, p)| p.just_pressed(GamepadButton::Start))
            {
                actions.write(Action::Resume);
            }
            if keys.just_pressed(KeyCode::KeyR) {
                actions.write(Action::Again);
            }
            if keys.just_pressed(KeyCode::F10)
                || pads
                    .iter()
                    .any(|(_, p)| p.just_pressed(GamepadButton::East))
            {
                actions.write(Action::Leave);
            }
            // Reconnect a replacement controller into the first vacant pad seat.
            for (entity, pad) in &pads {
                if pad.just_pressed(GamepadButton::South)
                    && !race.devices.contains(&Device::Pad(entity))
                    && let Some(slot) = race
                        .devices
                        .iter_mut()
                        .find(|d| matches!(d, Device::Pad(e) if pads.get(*e).is_err()))
                {
                    *slot = Device::Pad(entity);
                }
            }
        }
        return;
    }
    if *halt != Halt::LocalLobby {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        *halt = Halt::Title;
        return;
    }
    if keys.just_pressed(KeyCode::Enter) {
        join(&mut race.devices, Device::Wasd);
    }
    if keys.just_pressed(KeyCode::ShiftRight) {
        join(&mut race.devices, Device::Arrows);
    }
    if keys.just_pressed(KeyCode::Backspace) {
        race.devices.pop();
    }
    let previous = keys.just_pressed(KeyCode::ArrowLeft)
        || pads
            .iter()
            .any(|(_, p)| p.just_pressed(GamepadButton::DPadLeft));
    let next = keys.just_pressed(KeyCode::ArrowRight)
        || pads
            .iter()
            .any(|(_, p)| p.just_pressed(GamepadButton::DPadRight));
    if previous {
        actions.write(Action::Previous);
    }
    if next {
        actions.write(Action::Next);
    }
    for (entity, pad) in &pads {
        if pad.just_pressed(GamepadButton::South) {
            join(&mut race.devices, Device::Pad(entity));
        }
        if pad.just_pressed(GamepadButton::East) {
            if !race.devices.contains(&Device::Pad(entity)) {
                *halt = Halt::Title;
            }
            race.devices.retain(|d| *d != Device::Pad(entity));
        }
    }
    if keys.just_pressed(KeyCode::Space)
        || pads
            .iter()
            .any(|(_, p)| p.just_pressed(GamepadButton::Start))
    {
        actions.write(Action::Start);
    }
}

#[allow(clippy::too_many_arguments)]
fn actions(
    mut commands: Commands,
    mut actions: MessageReader<Action>,
    mut race: ResMut<LocalRace>,
    mut halt: ResMut<Halt>,
    mut go: MessageWriter<crate::track::GoTo>,
    mut resets: MessageWriter<Reset>,
    track: Res<Track>,
    mode: Res<Mode>,
    spec: Res<Spec>,
    setup: Res<Setup>,
    assets: Res<AssetServer>,
    entities: Query<Entity, With<LocalEntity>>,
    mut main: Query<(Entity, &mut Visibility), With<crate::car::Player>>,
    mut cameras: Query<&mut Camera, With<crate::camera::FollowCam>>,
    (mut ghost, mut rival): (ResMut<crate::ghost::Ghost>, ResMut<crate::ghost::Rival>),
    pads: Query<&Gamepad>,
) {
    for &action in actions.read() {
        match action {
            Action::Back => {
                *halt = Halt::Title;
            }
            Action::JoinWasd => join(&mut race.devices, Device::Wasd),
            Action::JoinArrows => join(&mut race.devices, Device::Arrows),
            Action::Remove(i) => {
                if i < race.devices.len() {
                    race.devices.remove(i);
                }
            }
            Action::Previous => {
                race.circuit = (race.circuit + crate::track::all_circuits().len() - 1)
                    % crate::track::all_circuits().len()
            }
            Action::Next => race.circuit = (race.circuit + 1) % crate::track::all_circuits().len(),
            Action::Resume => {
                if race
                    .devices
                    .iter()
                    .all(|d| !matches!(d, Device::Pad(e) if pads.get(*e).is_err()))
                {
                    *halt = Halt::Nothing;
                }
            }
            Action::Again => {
                resets.write(Reset);
                *halt = Halt::Nothing;
            }
            Action::Leave => {
                for entity in &entities {
                    commands.entity(entity).despawn();
                }
                for (entity, mut visible) in &mut main {
                    commands.entity(entity).remove::<Parked>();
                    *visible = Visibility::Visible;
                }
                for mut camera in &mut cameras {
                    camera.is_active = true;
                }
                ghost.on = race.ghost_was_on;
                rival.on = race.rival_was_on;
                race.active = false;
                *halt = Halt::LocalLobby;
                resets.write(Reset);
            }
            Action::Start => {
                if race.active
                    || race.devices.len() < 2
                    || race
                        .devices
                        .iter()
                        .any(|d| matches!(d, Device::Pad(e) if pads.get(*e).is_err()))
                {
                    continue;
                }
                race.active = true;
                race.ghost_was_on = ghost.on;
                race.rival_was_on = rival.on;
                ghost.on = false;
                rival.on = false;
                for (entity, mut visible) in &mut main {
                    commands.entity(entity).insert(Parked);
                    *visible = Visibility::Hidden;
                }
                for mut camera in &mut cameras {
                    camera.is_active = false;
                }
                for (index, &device) in race.devices.iter().enumerate() {
                    commands
                        .spawn((
                            LocalEntity,
                            Seat {
                                index,
                                device,
                                timer: LapTimer::default(),
                            },
                            Car::default(),
                            Controls::default(),
                            mode.applied_to(setup.applied_to(spec.handling())),
                            grid(&track, index),
                            Visibility::default(),
                        ))
                        .with_children(|parent| {
                            crate::car::local_body(parent, &assets, COLOURS[index])
                        });
                    commands.spawn((
                        LocalEntity,
                        LocalCamera(index),
                        Camera3d::default(),
                        Camera {
                            order: index as isize,
                            ..default()
                        },
                        DistanceFog {
                            color: crate::world::SKY,
                            falloff: FogFalloff::Linear {
                                start: 130.0,
                                end: 380.0,
                            },
                            ..default()
                        },
                        Transform::default(),
                    ));
                }
                commands.spawn((
                    LocalEntity,
                    Camera2d,
                    Camera {
                        order: 100,
                        clear_color: ClearColorConfig::None,
                        ..default()
                    },
                    IsDefaultUiCamera,
                ));
                *halt = Halt::Nothing;
                go.write(crate::track::GoTo(
                    &crate::track::all_circuits()[race.circuit],
                ));
                resets.write(Reset);
            }
        }
    }
}
fn grid(track: &Track, index: usize) -> Transform {
    track.local_grid(index).0
}

fn reset(
    mut resets: MessageReader<Reset>,
    track: Res<Track>,
    mode: Res<Mode>,
    spec: Res<Spec>,
    setup: Res<Setup>,
    mut cars: Query<(&mut Transform, &mut Car, &mut Seat, &mut Handling)>,
) {
    if resets.read().next().is_none() && !track.is_changed() {
        return;
    }
    for (mut at, mut car, mut seat, mut handling) in &mut cars {
        *at = grid(&track, seat.index);
        *car = Car {
            along: Some(track.local_grid(seat.index).1),
            ..default()
        };
        *handling = mode.applied_to(setup.applied_to(spec.handling()));
        seat.timer = LapTimer::default();
    }
}
fn inputs(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    race: Res<LocalRace>,
    halt: Res<Halt>,
    settings: Res<crate::settings::Settings>,
    mut cars: Query<(&mut Seat, &mut Controls)>,
) {
    for (mut seat, mut controls) in &mut cars {
        seat.device = race.devices[seat.index];
        *controls = if halt.stopped() {
            Controls::default()
        } else {
            controls_for(
                seat.device,
                &keys,
                &pads,
                settings.deadzone,
                settings.steering,
            )
        };
    }
}
fn controls_for(
    device: Device,
    keys: &ButtonInput<KeyCode>,
    pads: &Query<&Gamepad>,
    deadzone: f32,
    sensitivity: f32,
) -> Controls {
    if let Device::Pad(entity) = device {
        let Ok(pad) = pads.get(entity) else {
            return Controls::default();
        };
        return pad_controls(pad, deadzone, sensitivity);
    }
    let [up, down, left, right, handbrake] = match device {
        Device::Wasd => [
            KeyCode::KeyW,
            KeyCode::KeyS,
            KeyCode::KeyA,
            KeyCode::KeyD,
            KeyCode::Space,
        ],
        _ => [
            KeyCode::ArrowUp,
            KeyCode::ArrowDown,
            KeyCode::ArrowLeft,
            KeyCode::ArrowRight,
            KeyCode::ControlRight,
        ],
    };
    Controls {
        throttle: f32::from(keys.pressed(up)),
        brake: f32::from(keys.pressed(down)),
        steer: f32::from(keys.pressed(left)) - f32::from(keys.pressed(right)),
        handbrake: keys.pressed(handbrake),
    }
}
fn pad_controls(pad: &Gamepad, deadzone: f32, sensitivity: f32) -> Controls {
    let stick = crate::input::stick_steer(-pad.left_stick().x, deadzone, sensitivity);
    let dpad = f32::from(pad.pressed(GamepadButton::DPadLeft))
        - f32::from(pad.pressed(GamepadButton::DPadRight));
    Controls {
        throttle: pad
            .get(GamepadButton::RightTrigger2)
            .unwrap_or(0.0)
            .max(f32::from(pad.pressed(GamepadButton::South))),
        brake: pad
            .get(GamepadButton::LeftTrigger2)
            .unwrap_or(0.0)
            .max(f32::from(pad.pressed(GamepadButton::West))),
        steer: if dpad.abs() > stick.abs() {
            dpad
        } else {
            stick
        },
        handbrake: pad.pressed(GamepadButton::East),
    }
}
fn hold(
    mut commands: Commands,
    start: Res<crate::countdown::Start>,
    cars: Query<(Entity, Has<crate::countdown::Held>), With<Seat>>,
) {
    for (entity, held) in &cars {
        if start.held() && !held {
            commands.entity(entity).insert(crate::countdown::Held);
        }
        if !start.held() && held {
            commands.entity(entity).remove::<crate::countdown::Held>();
        }
    }
}
fn clocks(
    time: Res<Time<Fixed>>,
    track: Res<Track>,
    mut cars: Query<(&Transform, &mut Car, &mut Seat), Without<crate::countdown::Held>>,
) {
    for (at, mut car, mut seat) in &mut cars {
        seat.timer.count(time.delta_secs());
        let pos = at.translation;
        let step = Step {
            legal: track.legal_contact(at, car.along),
            recovered: car.recovered,
            along: track.start_along(pos),
            progress: track.progress(pos, car.along),
            length: track.length(),
            sectors: track.sector_count(),
            speed: car.velocity.length(),
        };
        car.recovered = false;
        seat.timer
            .judge(step, || track.on_start_gate(pos, car.along));
    }
}
/// Partition physical pixels, including odd sizes, without gaps or overlap.
fn viewport(size: UVec2, count: usize, index: usize) -> (UVec2, UVec2) {
    let rows = if count == 2 { 1 } else { 2 };
    let col = index % 2;
    let row = index / 2;
    let left = size.x * col as u32 / 2;
    let top = size.y * row as u32 / rows;
    let right = size.x * (col + 1) as u32 / 2;
    let bottom = size.y * (row + 1) as u32 / rows;
    (
        UVec2::new(left, top),
        UVec2::new(right - left, bottom - top),
    )
}
fn cameras(
    time: Res<Time>,
    race: Res<LocalRace>,
    track: Res<Track>,
    windows: Query<&Window>,
    cars: Query<(&Seat, &Transform, &Car), Without<LocalCamera>>,
    mut cameras: Query<(&LocalCamera, &mut Camera, &mut Transform), Without<Seat>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    for (local, mut camera, mut at) in &mut cameras {
        let (position, size) = viewport(window.physical_size(), race.devices.len(), local.0);
        camera.viewport = Some(Viewport {
            physical_position: position,
            physical_size: size.max(UVec2::ONE),
            ..default()
        });
        let Some((_, car_at, car)) = cars.iter().find(|(seat, _, _)| seat.index == local.0) else {
            continue;
        };
        let forward = crate::car::level(*car_at.forward());
        let desired = car_at.translation - forward * 6.0 + Vec3::Y * 3.2;
        let target = car_at.translation + forward * 2.0 + Vec3::Y * 0.6;
        let cut = at.translation.distance_squared(desired) > 900.0 || track.is_changed();
        let blend = if cut {
            1.0
        } else {
            1.0 - (-7.0 * time.delta_secs()).exp()
        };
        at.translation = at.translation.lerp(desired, blend);
        let (floor, rise) = crate::camera::clearance(
            &track,
            car.along,
            at.translation,
            car_at.translation + Vec3::Y * 0.35,
            1.0,
        );
        at.translation.y = at.translation.y.max(floor).max(rise);
        at.look_at(target, Vec3::Y);
    }
}

#[cfg(feature = "visual-check")]
#[allow(clippy::too_many_arguments)]
pub(crate) fn preview(
    mut commands: Commands,
    track: Res<Track>,
    mut race: ResMut<LocalRace>,
    mut halt: ResMut<Halt>,
    mut actions: MessageWriter<Action>,
    mut frames: Local<u32>,
    cars: Query<&Seat>,
    cameras: Query<&Camera, With<LocalCamera>>,
    main: Query<&Camera, With<crate::camera::FollowCam>>,
) {
    let Ok(screen) = std::env::var("TODORA_SCREEN") else {
        return;
    };
    if !matches!(screen.as_str(), "local" | "local-lobby" | "local-cycle") {
        return;
    }
    *frames += 1;
    let count = std::env::var("TODORA_PLAYERS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(2)
        .clamp(2, 4);
    if *frames == 2 {
        race.circuit = crate::track::circuit_at(track.circuit());
        race.devices = vec![Device::Wasd, Device::Arrows];
        for _ in 2..count {
            race.devices
                .push(Device::Pad(commands.spawn(Gamepad::default()).id()));
        }
        *halt = Halt::LocalLobby;
    }
    if *frames == 3 && screen != "local-lobby" {
        actions.write(Action::Start);
    }
    if *frames == 60 && screen != "local-lobby" {
        assert!(race.active);
        assert_eq!(cars.iter().count(), count);
        assert_eq!(cameras.iter().count(), count);
        assert!(main.iter().all(|c| !c.is_active));
        assert!(cameras.iter().all(|c| {
            c.viewport
                .as_ref()
                .is_some_and(|v| v.physical_size.x > 0 && v.physical_size.y > 0)
        }));
        info!("Split-screen verified: {count} cars, cameras and independent clocks");
        if screen == "local-cycle" {
            actions.write(Action::Leave);
        }
    }
    if *frames == 80 && screen == "local-cycle" {
        assert!(!race.active);
        assert_eq!(cars.iter().count(), 0);
        assert_eq!(cameras.iter().count(), 0);
        assert!(main.iter().all(|c| c.is_active));
        assert_eq!(*halt, Halt::LocalLobby);
        info!("Split-screen exit verified: solo camera restored, all local entities removed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn devices_join_once_and_four_is_the_limit() {
        let mut devices = Vec::new();
        join(&mut devices, Device::Wasd);
        join(&mut devices, Device::Wasd);
        assert_eq!(devices.len(), 1);
        for _ in 0..8 {
            join(&mut devices, Device::Arrows);
        }
        assert_eq!(devices.len(), 2);
        let mut world = World::new();
        for _ in 0..4 {
            join(&mut devices, Device::Pad(world.spawn_empty().id()));
        }
        assert_eq!(devices.len(), 4);
    }
    #[test]
    fn viewports_cover_every_pixel_without_overlapping() {
        for size in [UVec2::new(1280, 800), UVec2::new(1279, 799)] {
            for count in [2, 4] {
                let mut pixels = vec![0u8; (size.x * size.y) as usize];
                for i in 0..count {
                    let (p, s) = viewport(size, count, i);
                    for y in p.y..p.y + s.y {
                        for x in p.x..p.x + s.x {
                            pixels[(y * size.x + x) as usize] += 1;
                        }
                    }
                }
                assert!(pixels.iter().all(|p| *p == 1));
            }
        }
        assert_eq!(
            viewport(UVec2::new(1280, 800), 3, 2),
            (UVec2::new(0, 400), UVec2::new(640, 400))
        );
    }
    #[test]
    fn keyboards_and_controllers_drive_only_their_own_car_and_pause_releases_all() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<LocalRace>()
            .init_resource::<Halt>()
            .init_resource::<crate::settings::Settings>()
            .add_systems(Update, inputs);
        let pad1 = app.world_mut().spawn(Gamepad::default()).id();
        let pad2 = app.world_mut().spawn(Gamepad::default()).id();
        let devices = vec![
            Device::Wasd,
            Device::Arrows,
            Device::Pad(pad1),
            Device::Pad(pad2),
        ];
        app.world_mut().resource_mut::<LocalRace>().devices = devices.clone();
        let cars: Vec<_> = devices
            .iter()
            .enumerate()
            .map(|(index, &device)| {
                app.world_mut()
                    .spawn((
                        Seat {
                            index,
                            device,
                            timer: LapTimer::default(),
                        },
                        Controls::default(),
                    ))
                    .id()
            })
            .collect();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowLeft);
        app.world_mut()
            .get_mut::<Gamepad>(pad1)
            .unwrap()
            .digital_mut()
            .press(GamepadButton::West);
        app.world_mut()
            .get_mut::<Gamepad>(pad2)
            .unwrap()
            .analog_mut()
            .set(GamepadAxis::LeftStickX, 1.0);
        app.update();
        let read = |app: &App, i| *app.world().get::<Controls>(cars[i]).unwrap();
        assert_eq!(
            read(&app, 0),
            Controls {
                throttle: 1.0,
                ..default()
            }
        );
        assert_eq!(
            read(&app, 1),
            Controls {
                steer: 1.0,
                ..default()
            }
        );
        assert_eq!(
            read(&app, 2),
            Controls {
                brake: 1.0,
                ..default()
            }
        );
        assert_eq!(
            read(&app, 3),
            Controls {
                steer: -1.0,
                ..default()
            }
        );
        app.world_mut().despawn(pad2);
        app.update();
        assert_eq!(read(&app, 3), Controls::default());
        app.insert_resource(Halt::LocalPause);
        app.update();
        for i in 0..4 {
            assert_eq!(read(&app, i), Controls::default());
        }
    }
    #[test]
    fn every_local_grid_slot_stays_on_the_starting_road() {
        for circuit in crate::track::all_circuits() {
            let track = Track::new(circuit);
            for i in 0..4 {
                let at = grid(&track, i);
                assert!(
                    track.legal_contact(&at, Some(track.start_along_lap())),
                    "{} player {}",
                    circuit.id,
                    i + 1
                );
            }
        }
    }
}
