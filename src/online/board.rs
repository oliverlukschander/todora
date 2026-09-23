//! The world leaderboard, over the game.
//!
//! **L**, or **World leaderboard** in the pause, opens it on the circuit and
//! mode being driven. Three views, changed with **Q / E** or **LB / RB**: the
//! world (the top ten, then you and five either side), your country, and the
//! rivals you pinned. **← →** walk the circuits, **↑ ↓** choose a row, **A /
//! Enter** races that lap's ghost, **X / P** pins or unpins a rival, and **Esc /
//! B** goes back where it came from. Boards are only fetched once the player has
//! gone online, because even a read tells the server who is asking; offline,
//! the board last seen is shown with its age.

use bevy::prelude::*;

use super::client::{Ask, Client, Place, Standing};
use super::ui::Online;
use crate::car::Mode;
use crate::hud::{AMBER, AMBER_DIM, FRONT};
use crate::lap::format_time;
use crate::pause::{Halt, HaltSet};
use crate::settings::Settings;
use crate::track::{Track, all_circuits};
use crate::ui::{LINE, MUTED, Navigation, SURFACE, TEXT, label};

/// Rows the board shows at once.
const ROWS: usize = 14;
/// Rivals a player may pin.
const MOST_RIVALS: usize = 5;
const RIVAL: Color = Color::srgb(0.46, 0.86, 0.94);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum View {
    #[default]
    World,
    Country,
    Rivals,
    /// Every circuit: your best, its medal and your place.
    Records,
    /// The weekly challenge's board.
    Week,
    /// The fifty achievements, earned or not.
    Awards,
}

impl View {
    const ALL: [Self; 6] = [
        Self::World,
        Self::Country,
        Self::Rivals,
        Self::Week,
        Self::Records,
        Self::Awards,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::World => "World",
            Self::Country => "Country",
            Self::Rivals => "Rivals",
            Self::Week => "This week",
            Self::Records => "Records",
            Self::Awards => "Awards",
        }
    }
}

/// One circuit's row of the records: name, your best, its medal, your place.
pub(crate) fn record_line(
    name: &str,
    best: Option<f32>,
    medal: Option<crate::medals::Medal>,
    rank: Option<(u64, u64)>,
) -> String {
    format!(
        "{:<24} {:>9}   {:<7} {}",
        name,
        best.map_or("—".into(), format_time),
        medal.map_or("", |m| m.name()).to_uppercase(),
        rank.map_or(String::new(), |(r, t)| format!("#{r} of {t}")),
    )
}

/// The medal count for the records' footer.
pub(crate) fn medal_totals(medals: &[Option<crate::medals::Medal>]) -> String {
    use crate::medals::Medal;
    let count = |m| medals.iter().filter(|x| **x == Some(m)).count();
    let none = medals.iter().filter(|x| x.is_none()).count();
    format!(
        "{} author  ·  {} gold  ·  {} silver  ·  {} bronze  ·  {} to go",
        count(Medal::Author),
        count(Medal::Gold),
        count(Medal::Silver),
        count(Medal::Bronze),
        none
    )
}

/// Which board is up, and where the cursor is.
#[derive(Resource, Default)]
pub(crate) struct Browse {
    circuit: usize,
    view: View,
    row: usize,
    /// Where Esc goes back to.
    from_pause: bool,
    navigation: Navigation,
    /// Whether the board shown is the one asked for.
    asked: Option<(usize, View)>,
}

impl Browse {
    /// Open on a view, for a visual check: 0 world … 3 this week, 4 records.
    #[cfg(feature = "visual-check")]
    pub(crate) fn show(&mut self, view: usize) {
        let view = View::ALL[view.min(View::ALL.len() - 1)];
        if self.view != view {
            self.view = view;
        }
    }
}

/// One line of the board: a place, or a gap between the top and you.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Line {
    Place(Place),
    Gap,
}

/// The lines a view shows: the top ten, then — if you are further down — a gap
/// and you with five either side; for rivals, the rivals and you, in order.
pub(crate) fn lines(standing: &Standing, view: View) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::new();
    match view {
        View::Records | View::Awards => {}
        View::Rivals => {
            let mut places = standing.rivals.clone();
            if let Some(you) = &standing.you
                && !places.iter().any(|p| p.player == you.player)
            {
                places.push(you.clone());
            }
            places.sort_by_key(|p| p.rank);
            out.extend(places.into_iter().map(Line::Place));
        }
        View::World | View::Country | View::Week => {
            out.extend(standing.top.iter().cloned().map(Line::Place));
            let last_top = standing.top.last().map_or(0, |p| p.rank);
            let rest: Vec<_> = standing
                .around
                .iter()
                .filter(|p| p.rank > last_top)
                .cloned()
                .collect();
            if let Some(first) = rest.first() {
                if first.rank > last_top + 1 {
                    out.push(Line::Gap);
                }
                out.extend(rest.into_iter().map(Line::Place));
            }
        }
    }
    out.truncate(ROWS);
    out
}

/// The line under the board: where you stand against the record.
pub(crate) fn footer(standing: &Standing) -> String {
    let record = standing.top.first();
    match (&standing.you, record) {
        (Some(you), Some(record)) => format!(
            "You #{} of {}  ·  {}  ·  record {} by {}  ·  {:+.2}",
            you.rank,
            standing.total,
            format_time(you.seconds as f32),
            format_time(record.seconds as f32),
            record.name,
            you.seconds - record.seconds
        ),
        (None, Some(record)) => format!(
            "{} on the board  ·  record {} by {}",
            standing.total,
            format_time(record.seconds as f32),
            record.name
        ),
        _ => "Nobody has set a time here yet.".into(),
    }
}

#[derive(Component)]
struct Panel;
#[derive(Component)]
struct Title;
#[derive(Component)]
struct ViewTab(usize);
#[derive(Component)]
struct RowLine(usize);
#[derive(Component)]
struct RowText(usize);
#[derive(Component)]
struct Footer;
#[derive(Component)]
struct Note;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Browse>()
        .add_systems(Startup, setup)
        .add_systems(PreUpdate, (open, drive).chain().after(HaltSet))
        .add_systems(Update, (ask, draw).chain());
}

fn setup(mut commands: Commands) {
    commands
        .spawn((
            Panel,
            GlobalZIndex(16),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.018, 0.026, 0.032, 0.86)),
            Visibility::Hidden,
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        width: px(760),
                        padding: UiRect::all(px(26)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(16)),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(6),
                        ..default()
                    },
                    BackgroundColor(FRONT),
                    BorderColor::all(LINE),
                ))
                .with_children(|panel| {
                    panel.spawn(label("TODORA  /  WORLD LEADERBOARD", 12.0, AMBER));
                    panel
                        .spawn(Node {
                            justify_content: JustifyContent::SpaceBetween,
                            align_items: AlignItems::Center,
                            margin: UiRect::bottom(px(8)),
                            ..default()
                        })
                        .with_children(|header| {
                            header.spawn((Title, label("", 24.0, TEXT)));
                            header
                                .spawn(Node {
                                    column_gap: px(8),
                                    ..default()
                                })
                                .with_children(|tabs| {
                                    for (i, view) in View::ALL.iter().enumerate() {
                                        tabs.spawn((
                                            ViewTab(i),
                                            Node {
                                                padding: UiRect::axes(px(12), px(6)),
                                                border_radius: BorderRadius::all(px(8)),
                                                ..default()
                                            },
                                            BackgroundColor(SURFACE),
                                        ))
                                        .with_children(|tab| {
                                            tab.spawn(label(view.name(), 15.0, TEXT));
                                        });
                                    }
                                });
                        });
                    for i in 0..ROWS {
                        panel
                            .spawn((
                                RowLine(i),
                                Node {
                                    padding: UiRect::axes(px(12), px(5)),
                                    border_radius: BorderRadius::all(px(6)),
                                    ..default()
                                },
                                BackgroundColor(Color::NONE),
                            ))
                            .with_children(|row| {
                                row.spawn((RowText(i), TextLayout::no_wrap(), label("", 17.0, TEXT)));
                            });
                    }
                    panel.spawn((Note, label("", 15.0, MUTED)));
                    panel.spawn((Footer, label("", 15.0, AMBER_DIM)));
                    panel.spawn(label(
                        "← →  Circuit    ↑ ↓  Row    Q / E  or  LB / RB  View    A / Enter  Race ghost    X / P  Pin rival    Esc / B  Back",
                        12.0,
                        AMBER_DIM,
                    ));
                });
        });
}

/// `L` while driving; the pause's button sets the halt itself.
fn open(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    settings: Option<Res<Settings>>,
    track: Res<Track>,
    mut halt: ResMut<Halt>,
    mut browse: ResMut<Browse>,
) {
    let asked = crate::settings::bindings::current(settings.as_deref()).just(
        &keys,
        &pads,
        crate::settings::bindings::Act::Board,
    );
    if *halt == Halt::Nothing && asked {
        *halt = Halt::Board;
        browse.from_pause = false;
    } else if *halt == Halt::Board && halt.is_changed() && !asked {
        browse.from_pause = true;
    } else {
        return;
    }
    browse.circuit = all_circuits()
        .iter()
        .position(|c| c.id == track.circuit().id)
        .unwrap_or(0);
    browse.row = 0;
    browse.asked = None;
}

#[allow(clippy::too_many_arguments)]
fn drive(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    time: Res<Time<Real>>,
    client: Option<Res<Client>>,
    mut online: ResMut<Online>,
    mut halt: ResMut<Halt>,
    mut browse: ResMut<Browse>,
    mut settings: ResMut<Settings>,
    challenge: Res<crate::challenge::Challenge>,
    mut go: MessageWriter<crate::track::GoTo>,
    mut mode: ResMut<Mode>,
) {
    if *halt != Halt::Board || halt.is_changed() {
        return;
    }
    let pad = |button| pads.iter().any(|pad| pad.just_pressed(button));
    if keys.just_pressed(KeyCode::Escape) || pad(GamepadButton::East) || pad(GamepadButton::Start) {
        *halt = if browse.from_pause {
            Halt::Pause
        } else {
            Halt::Nothing
        };
        return;
    }
    let view_step = i32::from(keys.just_pressed(KeyCode::KeyE) || pad(GamepadButton::RightTrigger))
        - i32::from(keys.just_pressed(KeyCode::KeyQ) || pad(GamepadButton::LeftTrigger));
    if view_step != 0 {
        let at = View::ALL
            .iter()
            .position(|v| *v == browse.view)
            .unwrap_or(0) as i32;
        browse.view = View::ALL[(at + view_step).rem_euclid(View::ALL.len() as i32) as usize];
        browse.row = 0;
    }
    let step = browse
        .navigation
        .read(&keys, &pads, time.elapsed_secs_f64());
    if browse.view == View::Week {
        browse.circuit = challenge.at;
        // D, or the pad's north button: drive this week's circuit, in Regular.
        if keys.just_pressed(KeyCode::KeyD) || pad(GamepadButton::North) {
            go.write(crate::track::GoTo(challenge.circuit()));
            mode.set_if_neq(Mode::Regular);
            *halt = Halt::Nothing;
            return;
        }
    }
    if browse.view == View::Awards {
        let n = crate::achievements::all().len() as i32;
        browse.row = (browse.row as i32 + step.y).clamp(0, n - 1) as usize;
        return;
    }
    if browse.view == View::Records {
        let n = all_circuits().len() as i32;
        browse.row = (browse.row as i32 + step.y).clamp(0, n - 1) as usize;
        // A circuit's row opens its board.
        if keys.just_pressed(KeyCode::Enter) || pad(GamepadButton::South) {
            browse.circuit = browse.row;
            browse.view = View::World;
            browse.row = 0;
            browse.asked = None;
        }
        return;
    }
    if step.x != 0 && browse.view != View::Week {
        let n = all_circuits().len() as i32;
        browse.circuit = (browse.circuit as i32 + step.x).rem_euclid(n) as usize;
        browse.row = 0;
    }
    let week = (browse.view == View::Week).then(|| challenge.label());
    let shown = online
        .board
        .as_ref()
        .filter(|b| b.circuit == all_circuits()[browse.circuit].id && b.week == week)
        .map(|b| lines(b, browse.view))
        .unwrap_or_default();
    if step.y != 0 && !shown.is_empty() {
        browse.row = (browse.row as i32 + step.y).clamp(0, shown.len() as i32 - 1) as usize;
    }
    let Some(Line::Place(place)) = shown.get(browse.row) else {
        return;
    };
    if keys.just_pressed(KeyCode::KeyP) || pad(GamepadButton::West) {
        if let Some(at) = settings.rivals.iter().position(|r| *r == place.player) {
            settings.rivals.remove(at);
        } else if settings.rivals.len() < MOST_RIVALS {
            settings.rivals.push(place.player.clone());
        }
        browse.asked = None;
    }
    if (keys.just_pressed(KeyCode::Enter) || pad(GamepadButton::South))
        && let Some(client) = client
    {
        let place = place.clone();
        client.ask(Ask::Ghost(place.run.clone()));
        online.say(format!("Downloading {}'s lap…", place.name));
        online.wanted = Some((place.run, place.name, place.rank));
    }
}

/// Ask for the board on screen, once per circuit and view.
fn ask(
    halt: Res<Halt>,
    mode: Res<Mode>,
    settings: Res<Settings>,
    client: Option<Res<Client>>,
    challenge: Res<crate::challenge::Challenge>,
    mut browse: ResMut<Browse>,
) {
    if *halt != Halt::Board {
        return;
    }
    let Some(client) = client else {
        return;
    };
    let wanted = (browse.circuit, browse.view);
    if browse.view == View::Awards {
        return;
    }
    if browse.view == View::Records {
        if browse.asked != Some(wanted) {
            browse.asked = Some(wanted);
            client.ask(Ask::Ranks {
                mode: mode.name().to_lowercase(),
            });
        }
        return;
    }
    if !settings.online {
        return;
    }
    if browse.asked == Some(wanted) {
        return;
    }
    browse.asked = Some(wanted);
    let country = (browse.view == View::Country && !settings.country.is_empty())
        .then(|| settings.country.clone());
    let weekly = browse.view == View::Week;
    client.ask(Ask::Board {
        circuit: all_circuits()[browse.circuit].id.into(),
        mode: if weekly {
            "regular".into()
        } else {
            mode.name().to_lowercase()
        },
        country,
        rivals: settings.rivals.clone(),
        week: weekly.then(|| challenge.label()),
    });
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw(
    halt: Res<Halt>,
    mode: Res<Mode>,
    browse: Res<Browse>,
    online: Res<Online>,
    settings: Res<Settings>,
    challenge: Res<crate::challenge::Challenge>,
    records: Option<Res<crate::ghost::Records>>,
    earned: Option<Res<crate::achievements::Earned>>,
    mut panels: Query<&mut Visibility, With<Panel>>,
    mut titles: Query<
        &mut Text,
        (
            With<Title>,
            Without<RowText>,
            Without<Footer>,
            Without<Note>,
        ),
    >,
    mut tabs: Query<(&ViewTab, &mut BackgroundColor), Without<RowLine>>,
    mut lines_q: Query<(&RowLine, &mut BackgroundColor), Without<ViewTab>>,
    mut texts: Query<
        (&RowText, &mut Text, &mut TextColor),
        (Without<Title>, Without<Footer>, Without<Note>),
    >,
    mut footers: Query<
        &mut Text,
        (
            With<Footer>,
            Without<Title>,
            Without<RowText>,
            Without<Note>,
        ),
    >,
    mut notes: Query<
        &mut Text,
        (
            With<Note>,
            Without<Title>,
            Without<RowText>,
            Without<Footer>,
        ),
    >,
) {
    let open = *halt == Halt::Board;
    for mut visibility in &mut panels {
        visibility.set_if_neq(if open {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
    if !open {
        return;
    }
    for (tab, mut colour) in &mut tabs {
        let active = View::ALL[tab.0] == browse.view;
        colour.set_if_neq(BackgroundColor(if active { AMBER } else { SURFACE }));
    }
    if browse.view == View::Awards {
        let all = crate::achievements::all();
        let first = browse.row.saturating_sub(ROWS / 2).min(all.len() - ROWS);
        let got = |id: &str| earned.as_ref().and_then(|e| e.earned.get(id).copied());
        if let Ok(mut text) = titles.single_mut() {
            let n = all.iter().filter(|(id, _, _)| got(id).is_some()).count();
            let wanted = format!("Awards   ·   {n} of {}", all.len());
            if text.0 != wanted {
                text.0 = wanted;
            }
        }
        for (line, mut colour) in &mut lines_q {
            let active = first + line.0 == browse.row;
            colour.set_if_neq(BackgroundColor(if active { SURFACE } else { Color::NONE }));
        }
        for (row, mut text, mut colour) in &mut texts {
            let Some((id, name, what)) = all.get(first + row.0) else {
                continue;
            };
            let wanted = format!("{name:<26} {what}");
            if text.0 != wanted {
                text.0 = wanted;
            }
            colour.set_if_neq(TextColor(if got(id).is_some() { AMBER } else { MUTED }));
        }
        if let Ok(mut text) = footers.single_mut() {
            let km = earned.as_ref().map_or(0.0, |e| e.metres / 1000.0);
            let wanted = format!("{km:.1} km driven  ·  earned ones are lit");
            if text.0 != wanted {
                text.0 = wanted;
            }
        }
        if let Ok(mut text) = notes.single_mut()
            && !text.0.is_empty()
        {
            text.0.clear();
        }
        return;
    }
    if browse.view == View::Records {
        let medal = |at: usize| {
            let best = records.as_ref().and_then(|r| r.best(at, *mode));
            let targets = crate::medals::targets_for(all_circuits()[at].id, *mode);
            (
                best,
                targets.and_then(|(t, _)| best.and_then(|b| t.medal(b))),
            )
        };
        let first = browse
            .row
            .saturating_sub(ROWS / 2)
            .min(all_circuits().len() - ROWS);
        if let Ok(mut text) = titles.single_mut() {
            let wanted = format!("Records   ·   {}", mode.name());
            if text.0 != wanted {
                text.0 = wanted;
            }
        }
        for (line, mut colour) in &mut lines_q {
            let active = first + line.0 == browse.row;
            colour.set_if_neq(BackgroundColor(if active { SURFACE } else { Color::NONE }));
        }
        for (row, mut text, mut colour) in &mut texts {
            let at = first + row.0;
            let circuit = &all_circuits()[at];
            let (best, earned) = medal(at);
            let rank = online.ranks.get(circuit.id).copied();
            let wanted = record_line(circuit.name, best, earned, rank);
            if text.0 != wanted {
                text.0 = wanted;
            }
            colour.set_if_neq(TextColor(earned.map_or(TEXT, crate::medals::Medal::colour)));
        }
        let all: Vec<_> = (0..all_circuits().len()).map(|at| medal(at).1).collect();
        if let Ok(mut text) = footers.single_mut() {
            let wanted = medal_totals(&all);
            if text.0 != wanted {
                text.0 = wanted;
            }
        }
        if let Ok(mut text) = notes.single_mut()
            && !text.0.is_empty()
        {
            text.0.clear();
        }
        return;
    }
    let circuit = &all_circuits()[browse.circuit];
    if let Ok(mut text) = titles.single_mut() {
        let wanted = if browse.view == View::Week {
            format!(
                "This week   ·   {}   ·   Regular   ·   {} left",
                circuit.name,
                challenge.left(super::client::unix_now())
            )
        } else {
            format!("{}   ·   {}", circuit.name, mode.name())
        };
        if text.0 != wanted {
            text.0 = wanted;
        }
    }
    let week = (browse.view == View::Week).then(|| challenge.label());
    let standing = online
        .board
        .as_ref()
        .filter(|b| b.circuit == circuit.id && b.week == week);
    let shown = standing.map(|b| lines(b, browse.view)).unwrap_or_default();
    let you = standing
        .and_then(|b| b.you.as_ref())
        .map(|p| p.player.clone());
    for (line, mut colour) in &mut lines_q {
        let active = line.0 == browse.row && line.0 < shown.len();
        colour.set_if_neq(BackgroundColor(if active { SURFACE } else { Color::NONE }));
    }
    for (row, mut text, mut colour) in &mut texts {
        let (wanted, tint) = match shown.get(row.0) {
            Some(Line::Place(p)) => (
                format!(
                    "{:>5}   {:<17} {:<3} {:>9}   {}",
                    format!("#{}", p.rank),
                    p.name,
                    p.country.as_deref().unwrap_or(""),
                    format_time(p.seconds as f32),
                    p.car.to_uppercase()
                ),
                if you.as_deref() == Some(p.player.as_str()) {
                    AMBER
                } else if settings.rivals.contains(&p.player) {
                    RIVAL
                } else {
                    TEXT
                },
            ),
            Some(Line::Gap) => ("        ·  ·  ·".into(), MUTED),
            None => (String::new(), TEXT),
        };
        if text.0 != wanted {
            text.0 = wanted;
        }
        colour.set_if_neq(TextColor(tint));
    }
    let weekly_best = (settings.challenge_week == challenge.label())
        .then_some(settings.challenge_best)
        .flatten();
    let note = if browse.view == View::Week && (!settings.online || standing.is_none()) {
        match weekly_best {
            Some(best) => format!(
                "Your best this week: {}    ·    D / Y  Drive it",
                format_time(best)
            ),
            None => "No lap this week yet.    ·    D / Y  Drive it".into(),
        }
    } else if !settings.online {
        "Go online in Settings → Online to see the world boards.".to_string()
    } else if browse.view == View::Country && settings.country.is_empty() {
        "Pick your country in Settings → Online.".into()
    } else if browse.view == View::Rivals && settings.rivals.is_empty() {
        "Pin up to five rivals with X / P on any board.".into()
    } else if let Some(b) = standing {
        let age = super::client::unix_now() - b.fetched_at;
        if age > 120 {
            format!("Offline: as it stood {} ago.", ago(age))
        } else {
            String::new()
        }
    } else {
        "Loading…".into()
    };
    if let Ok(mut text) = notes.single_mut()
        && text.0 != note
    {
        text.0 = note;
    }
    if let Ok(mut text) = footers.single_mut() {
        let wanted = standing.map(footer).unwrap_or_default();
        if text.0 != wanted {
            text.0 = wanted;
        }
    }
}

fn ago(seconds: i64) -> String {
    match seconds {
        s if s < 3600 => format!("{} min", s / 60),
        s if s < 86_400 => format!("{} h", s / 3600),
        s => format!("{} days", s / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(rank: u64, player: &str) -> Place {
        Place {
            rank,
            player: player.into(),
            name: format!("Driver {rank}"),
            country: None,
            steps: 10_000 + rank as u32,
            seconds: 41.0 + rank as f64 * 0.1,
            car: "tourer".into(),
            setup: "balanced".into(),
            multiplayer: false,
            run: format!("{rank:016x}"),
        }
    }

    fn standing(you: u64) -> Standing {
        Standing {
            circuit: "monza".into(),
            mode: "regular".into(),
            season: 1,
            total: 300,
            top: (1..=10).map(|r| place(r, &format!("p{r}"))).collect(),
            around: (you.saturating_sub(5).max(1)..=you + 5)
                .map(|r| place(r, &format!("p{r}")))
                .collect(),
            you: Some(place(you, &format!("p{you}"))),
            rivals: vec![place(40, "p40"), place(3, "p3")],
            fetched_at: 0,
            week: None,
        }
    }

    #[test]
    fn the_world_view_is_the_top_ten_a_gap_and_you_either_side() {
        let shown = lines(&standing(100), View::World);
        assert_eq!(shown.len(), ROWS, "held to the rows there are");
        assert_eq!(shown[10], Line::Gap);
        assert!(matches!(&shown[11], Line::Place(p) if p.rank == 95));
        let near_the_top = lines(&standing(8), View::World);
        assert!(
            !near_the_top.contains(&Line::Gap),
            "no gap when you are in reach of the top"
        );
        assert!(matches!(near_the_top.last(), Some(Line::Place(p)) if p.rank == 13));
    }

    #[test]
    fn the_rivals_view_puts_you_among_your_rivals_in_order() {
        let shown = lines(&standing(21), View::Rivals);
        let ranks: Vec<u64> = shown
            .iter()
            .map(|l| match l {
                Line::Place(p) => p.rank,
                Line::Gap => 0,
            })
            .collect();
        assert_eq!(ranks, vec![3, 21, 40]);
    }

    #[test]
    fn records_list_a_circuit_its_best_its_medal_and_your_place() {
        use crate::medals::Medal;
        assert_eq!(
            record_line("Monza", Some(62.88), Some(Medal::Silver), Some((96, 1390))),
            "Monza                      1:02.88   SILVER  #96 of 1390"
        );
        assert_eq!(
            medal_totals(&[
                Some(Medal::Gold),
                Some(Medal::Gold),
                None,
                Some(Medal::Author)
            ]),
            "1 author  ·  2 gold  ·  0 silver  ·  0 bronze  ·  1 to go"
        );
    }

    #[test]
    fn the_footer_measures_you_against_the_record() {
        assert_eq!(
            footer(&standing(21)),
            "You #21 of 300  ·  0:43.10  ·  record 0:41.10 by Driver 1  ·  +2.00"
        );
    }
}
