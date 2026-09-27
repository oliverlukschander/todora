//! The online side as the player sees it: the one-time offer to go online, a
//! toast when a lap reaches the board, and keeping the settings and the worker
//! in step.
//!
//! The offer comes after the first lap that counts, but it is not shown then,
//! mid-lap with the throttle down. It waits for the next time the car is held on
//! the grid for the lights, stops the world, and asks: **A / Enter** goes online
//! under a suggested name, **B / Esc** says not now. Either way it never asks
//! again; the Online tab in the settings changes it any time.

use bevy::prelude::*;

use super::client::{Ask, Client, Heard, Standing};
use crate::hud::{AMBER, AMBER_DIM, FRONT};
use crate::lap::{LapFinished, LapSet};
use crate::pause::{Halt, HaltSet};
use crate::settings::Settings;
use crate::ui::{LINE, TEXT, label};

/// Everything needed to prepare an explicitly selected online opponent.
#[derive(Clone)]
pub(crate) struct GhostTarget {
    pub place: super::client::Place,
    pub circuit: String,
    pub mode: crate::car::Mode,
    pub world_record: bool,
}

/// What the online side knows, for the rest of the game to read.
#[derive(Resource, Default)]
pub(crate) struct Online {
    /// A lap that counted has been driven since launch.
    pub earned_offer: bool,
    /// The last board fetched, if any.
    pub board: Option<Standing>,
    /// A downloaded ghost, as run bytes, and whose lap it was.
    pub ghost: Option<(String, Vec<u8>)>,
    /// The ghost asked for: its run, the driver's name and their place.
    pub wanted: Option<GhostTarget>,
    pub ghost_status: String,
    pub connection_error: String,
    /// Your place and the board's size on each circuit, as last seen.
    pub ranks: std::collections::HashMap<String, (u64, u64)>,
    /// Laps that reached the board since someone last looked.
    pub sent: Vec<super::client::Submitted>,
    /// A line to show for a few seconds, and how long it has left.
    toast: Option<(String, f32)>,
}

impl Online {
    pub(crate) fn say(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), 4.5));
    }
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Online>()
        .add_systems(Startup, setup)
        .add_systems(PostStartup, begin)
        .add_systems(
            PreUpdate,
            offer
                .run_if(crate::local::solo)
                .after(HaltSet)
                .after(crate::countdown::CountdownSet),
        )
        .add_systems(FixedUpdate, earn.after(LapSet))
        .add_systems(Update, (listen, follow_settings, draw).chain());
}

/// Go online at launch if the player already chose to.
fn begin(settings: Res<Settings>, client: Option<Res<Client>>) {
    if let Some(client) = client
        && settings.online
    {
        client.ask(Ask::Online {
            on: true,
            name: settings.name.clone(),
            country: country(&settings),
        });
    }
}

fn country(settings: &Settings) -> Option<String> {
    (!settings.country.is_empty()).then(|| settings.country.clone())
}

/// The first lap that counts earns the offer.
fn earn(mut laps: MessageReader<LapFinished>, mut online: ResMut<Online>) {
    if laps.read().any(|lap| lap.valid) {
        online.earned_offer = true;
    }
}

/// Show the offer when the car is held on the grid, and take the answer.
#[allow(clippy::too_many_arguments)]
fn offer(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    start: Res<crate::countdown::Start>,
    client: Option<Res<Client>>,
    online: Res<Online>,
    mut halt: ResMut<Halt>,
    mut settings: ResMut<Settings>,
    mut profile: MessageWriter<super::profile::OpenProfile>,
) {
    if client.is_none() || settings.online_asked || !online.earned_offer {
        return;
    }
    if *halt == Halt::Nothing && start.held() {
        *halt = Halt::Offer;
        if settings.name.is_empty() {
            settings.name = super::suggest_name(super::random_index());
        }
        return;
    }
    if *halt != Halt::Offer || halt.is_changed() {
        return;
    }
    let pad = |button| pads.iter().any(|pad| pad.just_pressed(button));
    let yes = keys.just_pressed(KeyCode::Enter) || pad(GamepadButton::South);
    let no = keys.just_pressed(KeyCode::Escape) || pad(GamepadButton::East);
    if yes || no {
        settings.online_asked = !yes;
        if yes {
            profile.write(super::profile::OpenProfile(Halt::Nothing));
        }
        *halt = Halt::Nothing;
    }
}

/// Everything the worker has said, turned into what the page and HUD show.
fn listen(
    client: Option<Res<Client>>,
    mut online: ResMut<Online>,
    mut settings: ResMut<Settings>,
    mut profile: ResMut<super::profile::Profile>,
    mut halt: ResMut<Halt>,
) {
    let Some(client) = client else {
        return;
    };
    for heard in client.heard() {
        match heard {
            Heard::Joined { name, country } => {
                online.connection_error.clear();
                settings.online_note = crate::text::tf("online.on", &[&name]);
                settings.name = name;
                settings.country = country.unwrap_or_default();
            }
            Heard::ProfileSaved { name, country } => {
                online.connection_error.clear();
                settings.online_note = crate::text::tf("online.on", &[&name]);
                settings.name = name;
                settings.country = country.unwrap_or_default();
                profile.saving = false;
                settings.online = true;
                settings.online_asked = true;
                online.board = None;
                *halt = profile.return_to;
                online.say(crate::text::t("profile.saved"));
            }
            Heard::ProfileFailed(why) => {
                profile.error = why.clone();
                profile.saving = false;
                settings.online_note = why;
            }
            Heard::Sent(sent) => {
                online.sent.push(sent.clone());
                let place = match sent.rank {
                    Some(rank) => crate::text::tf("board.place_of", &[&rank, &sent.total]),
                    None => crate::text::t("online.on_board").into(),
                };
                let circuit = crate::track::all_circuits()
                    .iter()
                    .find(|c| c.id == sent.circuit)
                    .map_or(sent.circuit.as_str(), |c| c.name);
                online.say(crate::text::tf(
                    "online.sent",
                    &[
                        &circuit,
                        &crate::lap::format_time(sent.seconds as f32),
                        &place,
                    ],
                ));
            }
            Heard::Refused { circuit, why } => {
                online.say(crate::text::tf("online.refused", &[&circuit, &why]));
            }
            Heard::Waiting(n) => {
                if settings.pending != n {
                    settings.bypass_change_detection().pending = n;
                }
            }
            Heard::Board(standing) => {
                online.connection_error.clear();
                if let Some(you) = &standing.you {
                    online
                        .ranks
                        .insert(standing.circuit.clone(), (you.rank, standing.total));
                }
                online.board = Some(*standing);
            }
            Heard::Ranks(ranks) => {
                online.ranks = ranks.into_iter().map(|(c, r, t)| (c, (r, t))).collect();
            }
            Heard::Ghost { run, bytes } => {
                if online
                    .wanted
                    .as_ref()
                    .is_some_and(|target| target.place.run == run)
                {
                    online.ghost_status = crate::text::t("ghost.preparing").into();
                    online.ghost = Some((run, bytes));
                }
            }
            Heard::GhostFailed { run, why } => {
                if online
                    .wanted
                    .as_ref()
                    .is_some_and(|target| target.place.run == run)
                {
                    online.wanted = None;
                    online.ghost_status = why;
                }
            }
            Heard::Trouble(why) => {
                online.connection_error = why.clone();
                if settings.online {
                    settings.bypass_change_detection().online_note =
                        crate::text::tf("online.offline", &[&why]);
                }
            }
            Heard::Forgotten => {
                settings.online = false;
                settings.online_note.clear();
                online.say(crate::text::t("online.deleted"));
            }
        }
    }
}

/// Online preference and account deletion follow settings; names and countries
/// are committed only by an explicit profile save.
fn follow_settings(
    client: Option<Res<Client>>,
    mut settings: ResMut<Settings>,
    mut was_online: Local<Option<bool>>,
) {
    let Some(client) = client else {
        return;
    };
    if *was_online != Some(settings.online) {
        if was_online.is_some() {
            client.ask(Ask::Online {
                on: settings.online,
                name: settings.name.clone(),
                country: country(&settings),
            });
        }
        *was_online = Some(settings.online);
    }
    if settings.forget_presses >= 2 {
        settings.forget_presses = 0;
        client.ask(Ask::Forget);
    }
}

#[derive(Component)]
struct OfferPanel;
#[derive(Component)]
struct OfferName;
#[derive(Component)]
struct Toast;

fn setup(mut commands: Commands) {
    commands
        .spawn((
            OfferPanel,
            GlobalZIndex(17),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.018, 0.026, 0.032, 0.8)),
            Visibility::Hidden,
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        width: px(620),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(14),
                        padding: UiRect::all(px(30)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(16)),
                        ..default()
                    },
                    BackgroundColor(FRONT),
                    BorderColor::all(LINE),
                ))
                .with_children(|panel| {
                    panel.spawn(crate::text::label("online.label", 12.0, AMBER));
                    panel.spawn(crate::text::label("online.ask", 30.0, TEXT));
                    panel.spawn(crate::text::label("online.explain", 16.0, AMBER_DIM));
                    panel.spawn((OfferName, label("", 20.0, TEXT)));
                    panel.spawn(crate::text::label("online.choose", 15.0, AMBER));
                });
        });
    commands.spawn((
        Toast,
        label("", 16.0, TEXT),
        Node {
            position_type: PositionType::Absolute,
            right: px(24),
            bottom: px(96),
            max_width: px(460),
            padding: UiRect::axes(px(14), px(10)),
            border_radius: BorderRadius::all(px(10)),
            ..default()
        },
        BackgroundColor(FRONT),
        Visibility::Hidden,
    ));
}

#[allow(clippy::type_complexity)]
fn draw(
    time: Res<Time<Real>>,
    halt: Res<Halt>,
    settings: Res<Settings>,
    mut online: ResMut<Online>,
    mut panels: Query<&mut Visibility, (With<OfferPanel>, Without<Toast>)>,
    mut names: Query<&mut Text, (With<OfferName>, Without<Toast>)>,
    mut toasts: Query<(&mut Text, &mut Visibility), (With<Toast>, Without<OfferPanel>)>,
) {
    let offering = *halt == Halt::Offer;
    for mut visibility in &mut panels {
        visibility.set_if_neq(if offering {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
    if offering && let Ok(mut text) = names.single_mut() {
        let wanted = crate::text::tf("online.appear", &[&settings.name]);
        if text.0 != wanted {
            text.0 = wanted;
        }
    }
    let dt = time.delta_secs();
    let toast = online.toast.as_mut().map(|(text, left)| {
        *left -= dt;
        (text.clone(), *left > 0.0)
    });
    if toast.as_ref().is_some_and(|(_, on)| !on) {
        online.toast = None;
    }
    if let Ok((mut text, mut visibility)) = toasts.single_mut() {
        match toast {
            Some((line, true)) if !halt.stopped() => {
                if text.0 != line {
                    text.0 = line;
                }
                visibility.set_if_neq(Visibility::Visible);
            }
            _ => {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
    }
}
