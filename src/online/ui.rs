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
    pub wanted: Option<(String, String, u64)>,
    /// Your place and the board's size on each circuit, as last seen.
    pub ranks: std::collections::HashMap<String, (u64, u64)>,
    /// The name and country the server last confirmed.
    confirmed: Option<(String, String)>,
    /// A line to show for a few seconds, and how long it has left.
    toast: Option<(String, f32)>,
    /// Seconds since the name or country last changed on the page.
    renamed_ago: Option<f32>,
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
            offer.after(HaltSet).after(crate::countdown::CountdownSet),
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
fn offer(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    start: Res<crate::countdown::Start>,
    client: Option<Res<Client>>,
    online: Res<Online>,
    mut halt: ResMut<Halt>,
    mut settings: ResMut<Settings>,
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
        settings.online_asked = true;
        settings.online = yes;
        *halt = Halt::Nothing;
    }
}

/// Everything the worker has said, turned into what the page and HUD show.
fn listen(client: Option<Res<Client>>, mut online: ResMut<Online>, mut settings: ResMut<Settings>) {
    let Some(client) = client else {
        return;
    };
    for heard in client.heard() {
        match heard {
            Heard::Joined { name, .. } => {
                settings.online_note = format!("On  ·  {name}");
                online.confirmed = Some((name.clone(), settings.country.clone()));
                if settings.name != name {
                    settings.name = name;
                }
            }
            Heard::Sent(sent) => {
                let place = match sent.rank {
                    Some(rank) => format!("#{rank} of {}", sent.total),
                    None => "on the board".into(),
                };
                let circuit = crate::track::all_circuits()
                    .iter()
                    .find(|c| c.id == sent.circuit)
                    .map_or(sent.circuit.as_str(), |c| c.name);
                online.say(format!(
                    "{circuit}: {} — world {place}",
                    crate::lap::format_time(sent.seconds as f32)
                ));
            }
            Heard::Refused { circuit, why } => {
                online.say(format!("{circuit}: lap not accepted — {why}"));
            }
            Heard::Waiting(n) => {
                if settings.pending != n {
                    settings.bypass_change_detection().pending = n;
                }
            }
            Heard::Board(standing) => {
                if let Some(you) = &standing.you {
                    online
                        .ranks
                        .insert(standing.circuit.clone(), (you.rank, standing.total));
                }
                online.board = Some(standing);
            }
            Heard::Ranks(ranks) => {
                online.ranks = ranks.into_iter().map(|(c, r, t)| (c, (r, t))).collect();
            }
            Heard::Ghost { run, bytes } => {
                online.say(format!(
                    "Ghost {} downloaded ({} KB)",
                    &run[..6.min(run.len())],
                    bytes.len() / 1024
                ));
                online.ghost = Some((run, bytes));
            }
            Heard::Trouble(why) => {
                if settings.online {
                    settings.bypass_change_detection().online_note = format!("Offline  ·  {why}");
                }
            }
            Heard::Forgotten => {
                settings.online = false;
                settings.online_note.clear();
                online.confirmed = None;
                online.say("Your online data has been deleted.");
            }
        }
    }
}

/// The page changed something the server should know: going online or off,
/// a new name or country (sent once typing has settled), or deleting it all.
fn follow_settings(
    time: Res<Time<Real>>,
    client: Option<Res<Client>>,
    mut online: ResMut<Online>,
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
    let wanted = (settings.name.clone(), settings.country.clone());
    let changed = online.confirmed.as_ref().is_some_and(|c| *c != wanted);
    if !changed || !settings.online {
        online.renamed_ago = None;
        return;
    }
    let ago = online.renamed_ago.get_or_insert(0.0);
    *ago += time.delta_secs();
    if *ago > 2.0 && crate::verify::valid_name(&wanted.0).is_ok() {
        online.renamed_ago = None;
        online.confirmed = Some(wanted.clone());
        client.ask(Ask::Rename {
            name: wanted.0,
            country: country(&settings),
        });
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
                    panel.spawn(label("TODORA  /  WORLD LEADERBOARDS", 12.0, AMBER));
                    panel.spawn(label("Put your laps on the world boards?", 30.0, TEXT));
                    panel.spawn(label(
                        "Your best laps go to todora.lukschander.com, hosted in the EU, and are \
                         driven again there before they count. Stored: a random id, the name \
                         below, a country if you pick one, and your laps. No email, no account, \
                         no address. Settings → Online changes the name or deletes everything.",
                        16.0,
                        AMBER_DIM,
                    ));
                    panel.spawn((OfferName, label("", 20.0, TEXT)));
                    panel.spawn(label(
                        "A / Enter  Go online        B / Esc  Not now",
                        15.0,
                        AMBER,
                    ));
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
        let wanted = format!("You would appear as  {}", settings.name);
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
