//! Shared practice: local physics, a buffered remote car, and Game Center.
#[cfg(feature = "multiplayer-test")]
mod check;
mod session;
#[cfg(test)]
mod tests;
mod transport;
#[cfg(all(target_os = "macos", feature = "game-center"))]
pub(crate) use transport::support_directory;

use crate::{
    Reset,
    car::{Car, Controls, MODEL, Mode, Player, SCALE},
    lap::{ClockSet, LapFinished, LapTimer},
    pause::{Halt, HaltSet},
    track::{GoTo, Track, TrackSet},
    ui,
};
use bevy::{light::NotShadowCaster, prelude::*, world_serialization::WorldInstanceReady};
pub(crate) use session::Session;
use session::{Action, Config, Packet, Phase, Snapshot};
use transport::{Event, Transport};

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct NetworkSet;
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ReadySet;

pub struct MultiplayerPlugin;
impl Plugin for MultiplayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Session>();
        if !cfg!(any(feature = "game-center", feature = "multiplayer-test")) {
            return;
        }
        app.init_resource::<Transport>()
            .init_resource::<Sending>()
            .add_systems(Startup, setup)
            .add_systems(
                PreUpdate,
                network
                    .in_set(NetworkSet)
                    .after(bevy::input::InputSystems)
                    .after(bevy::ui::UiSystems::Focus)
                    .before(HaltSet),
            )
            .add_systems(
                PreUpdate,
                (loaded, observe_resets)
                    .chain()
                    .in_set(ReadySet)
                    .after(ClockSet)
                    .after(crate::car::CarResetSet)
                    .after(TrackSet),
            )
            .add_systems(Update, (send_state, draw).chain());
        #[cfg(feature = "multiplayer-test")]
        check::configure(app);
    }
}

pub(crate) fn offline(session: Option<Res<Session>>) -> bool {
    session.is_none_or(|s| !s.active())
}

#[derive(Component)]
struct RemoteCar;
#[derive(Component)]
struct MultiplayerButton;
#[derive(Component)]
struct ButtonLabel;
#[derive(Component)]
struct Status;
#[derive(Component)]
struct MultiplayerPanel;
#[derive(Resource, Default)]
struct Sending {
    next: f64,
    seq: u64,
    reset: u64,
}

fn config(track: &Track, mode: Mode) -> Config {
    Config {
        circuit: track.circuit().id.into(),
        mode: Mode::ALL.iter().position(|m| *m == mode).unwrap(),
        fingerprint: track.fingerprint(),
    }
}

fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    commands
        .spawn((
            RemoteCar,
            Transform::from_scale(Vec3::splat(SCALE)),
            Visibility::Hidden,
            WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(MODEL))),
        ))
        .observe(paint_remote);
    commands
        .spawn((
            MultiplayerPanel,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(24),
                left: percent(38),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(6),
                max_width: px(470),
                ..default()
            },
            GlobalZIndex(25),
        ))
        .with_children(|panel| {
            panel.spawn((
                Status,
                ui::label("", 15.0, ui::TEXT),
                Node {
                    max_width: px(470),
                    ..default()
                },
            ));
            panel
                .spawn((
                    Button,
                    MultiplayerButton,
                    Node {
                        padding: UiRect::axes(px(16), px(10)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(8)),
                        ..default()
                    },
                    BackgroundColor(ui::SURFACE),
                    BorderColor::all(ui::LINE),
                ))
                .with_children(|b| {
                    b.spawn((ButtonLabel, ui::label("F9   Multiplayer", 16.0, ui::ACCENT)));
                });
        });
}

fn paint_remote(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    painted: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for entity in children.iter_descendants(ready.entity) {
        let Ok(handle) = painted.get(entity) else {
            continue;
        };
        let Some(source) = materials.get(&handle.0) else {
            continue;
        };
        let mut material = source.clone();
        // Cyan tint separates a live driver from the amber saved ghost.
        material.base_color = material.base_color.mix(&Color::srgb(0.15, 0.8, 0.95), 0.45);
        material.emissive = LinearRgba::new(0.015, 0.08, 0.1, 1.0);
        commands
            .entity(entity)
            .insert((MeshMaterial3d(materials.add(material)), NotShadowCaster));
    }
}

fn perform(
    actions: Vec<Action>,
    transport: &mut Transport,
    mode: &mut ResMut<Mode>,
    go: &mut MessageWriter<GoTo>,
    reset: &mut MessageWriter<Reset>,
) {
    for action in actions {
        match action {
            Action::Send(packet) => transport.send(&packet),
            Action::Load(config) => {
                if let Some(circuit) = crate::track::all_circuits()
                    .iter()
                    .find(|c| c.id == config.circuit)
                {
                    if **mode != Mode::ALL[config.mode] {
                        **mode = Mode::ALL[config.mode];
                    }
                    go.write(GoTo(circuit));
                }
            }
            Action::Reset => {
                reset.write(Reset);
            }
            Action::End => {
                transport.send(&Packet::Leave);
                transport.leave();
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn network(
    time: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    buttons: Query<Ref<Interaction>, With<MultiplayerButton>>,
    mut session: ResMut<Session>,
    mut transport: ResMut<Transport>,
    track: Res<Track>,
    mut mode: ResMut<Mode>,
    mut go: MessageWriter<GoTo>,
    mut reset: MessageWriter<Reset>,
    halt: Res<Halt>,
) {
    let now = time.elapsed_secs_f64();
    let pressed = keys.just_pressed(KeyCode::F9)
        || pads
            .iter()
            .any(|p| p.just_pressed(GamepadButton::RightThumb))
        || buttons
            .iter()
            .any(|i| i.is_changed() && *i == Interaction::Pressed);
    if pressed && (session.active() || *halt == Halt::Nothing) {
        if session.active() {
            let actions = session.end("Left shared practice. Back to solo driving.");
            perform(actions, &mut transport, &mut mode, &mut go, &mut reset);
        } else {
            session.begin(now);
            transport.start();
        }
    }
    for event in transport.poll() {
        let actions = match event {
            Event::Status(message) if session.active() => {
                session.status = message;
                vec![]
            }
            Event::Connected(host, peer) => {
                session.connected(host, peer, config(&track, *mode), now)
            }
            Event::Data(bytes) => {
                Packet::decode(&bytes).map_or_else(Vec::new, |p| session.receive(p, now))
            }
            Event::End(reason) if session.active() => session.end(reason),
            _ => vec![],
        };
        perform(actions, &mut transport, &mut mode, &mut go, &mut reset);
    }
    let actions = session.tick(now);
    perform(actions, &mut transport, &mut mode, &mut go, &mut reset);
}

#[allow(clippy::too_many_arguments)]
fn loaded(
    time: Res<Time<Real>>,
    track: Res<Track>,
    mut mode: ResMut<Mode>,
    mut session: ResMut<Session>,
    mut transport: ResMut<Transport>,
    mut reset: MessageWriter<Reset>,
    mut go: MessageWriter<GoTo>,
    halt: Res<Halt>,
    mut timer: ResMut<LapTimer>,
    mut players: Query<&mut Controls, With<Player>>,
    assets: Res<AssetServer>,
    remote: Query<&WorldAssetRoot, With<RemoteCar>>,
) {
    let ready = remote
        .iter()
        .any(|root| assets.is_loaded_with_dependencies(&root.0));
    let actions = if ready {
        session.loaded(&config(&track, *mode), time.elapsed_secs_f64())
    } else {
        vec![]
    };
    perform(actions, &mut transport, &mut mode, &mut go, &mut reset);
    if session.blocks_drive() {
        for mut controls in &mut players {
            *controls = Controls::default();
        }
    }
    // Online menus release the pedals, but cannot pause the other driver's
    // clock. A lap interrupted by the local pause cannot become a record.
    if session.driving() && halt.stopped() && timer.running() {
        timer.invalid = true;
    }
}

fn observe_resets(
    mut events: MessageReader<Reset>,
    mut sending: ResMut<Sending>,
    session: Res<Session>,
    mut players: Query<&mut Transform, With<Player>>,
) {
    if events.read().count() > 0 {
        sending.reset += 1;
        if session.active() && session.phase != Phase::Finding {
            for mut at in &mut players {
                let right = *at.right();
                at.translation += right * if session.host { -0.45 } else { 0.45 };
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn send_state(
    time: Res<Time<Real>>,
    mut session: ResMut<Session>,
    mut transport: ResMut<Transport>,
    mut sending: ResMut<Sending>,
    player: Query<(&Transform, &Car), With<Player>>,
    timer: Res<LapTimer>,
    halt: Res<Halt>,
    mut laps: MessageReader<LapFinished>,
) {
    for lap in laps.read() {
        if session.driving() && lap.valid {
            session.local_laps += 1;
            session.local_best = Some(
                session
                    .local_best
                    .map_or(lap.time, |best| best.min(lap.time)),
            );
        }
    }
    let now = time.elapsed_secs_f64();
    if !session.driving() || now < sending.next {
        return;
    }
    let Ok((at, car)) = player.single() else {
        return;
    };
    sending.next = now + 1.0 / 30.0;
    sending.seq += 1;
    transport.send(&Packet::State(Snapshot {
        seq: sending.seq,
        reset: sending.reset,
        time: now,
        position: at.translation.to_array(),
        rotation: at.rotation.to_array(),
        velocity: car.velocity.to_array(),
        lap: timer.current,
        invalid: timer.invalid,
        paused: halt.stopped(),
        laps: session.local_laps,
        best: session.local_best,
    }));
}

fn draw(
    time: Res<Time<Real>>,
    session: Res<Session>,
    mut remote: Query<(&mut Transform, &mut Visibility), With<RemoteCar>>,
    mut text: Query<&mut Text, (With<Status>, Without<ButtonLabel>)>,
    mut button: Query<&mut Text, (With<ButtonLabel>, Without<Status>)>,
    halt: Res<Halt>,
    mut panels: Query<&mut Node, With<MultiplayerPanel>>,
) {
    let now = time.elapsed_secs_f64();
    for mut panel in &mut panels {
        panel.display = if session.active() || *halt == Halt::Nothing {
            Display::Flex
        } else {
            Display::None
        };
    }
    if let Ok((mut at, mut visible)) = remote.single_mut() {
        if session.driving()
            && let Some((position, rotation)) = session.remote.pose(now)
        {
            at.translation = position;
            at.rotation = rotation;
            *visible = Visibility::Visible;
        } else {
            *visible = Visibility::Hidden;
        }
    }
    if let Ok(mut text) = text.single_mut() {
        text.0 = if session.phase == Phase::Countdown {
            format!(
                "{} · Starting in {}",
                session.peer,
                (session.start_at - now).ceil().max(1.0) as u32
            )
        } else if session.driving() {
            let best = |t: Option<f32>| t.map_or("—".into(), |t| format!("{t:.3}s"));
            let remote = session.remote.latest();
            let state = if now - session.remote.received_at > 1.5 {
                " · reconnecting…"
            } else if remote.is_some_and(|r| r.paused) {
                " · paused"
            } else if remote.is_some_and(|r| r.invalid) {
                " · lap invalid"
            } else {
                ""
            };
            format!(
                "SHARED PRACTICE · NO CONTACT\nYou: {} laps · best {}\n{}: {} laps · best {}{}",
                session.local_laps,
                best(session.local_best),
                session.peer,
                remote.map_or(0, |s| s.laps),
                best(remote.and_then(|s| s.best)),
                state
            )
        } else {
            session.status.clone()
        };
    }
    if let Ok(mut label) = button.single_mut() {
        label.0 = if session.active() {
            "F9 / R3   Leave multiplayer"
        } else {
            "F9 / R3   Multiplayer"
        }
        .into();
    }
}
