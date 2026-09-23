//! The settings page: four tabs of rows, driven like the pause menu.
//!
//! Up and down choose a row, left and right change its value (Enter or A
//! steps it forward too, which is how a switch is flipped), LB and RB or Q and
//! E change tab, and Esc, B or Start go back to the pause. A click chooses a row
//! and steps it. Every change lands in [`Settings`] at once and is saved a
//! second later, so there is no Apply button to forget.

use bevy::prelude::*;

use super::{CameraView, Countdown, Settings, Units};
use crate::hud::{AMBER, AMBER_DIM, FRONT};
use crate::pause::{Halt, HaltSet};
use crate::ui::{LINE, Navigation, SURFACE, TEXT, label};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Tab {
    Audio,
    Display,
    Hud,
    Controls,
    Keys,
    Online,
}

impl Tab {
    fn key(self) -> &'static str {
        match self {
            Self::Audio => "tab.audio",
            Self::Display => "tab.display",
            Self::Hud => "tab.hud",
            Self::Controls => "tab.controls",
            Self::Keys => "tab.keys",
            Self::Online => "tab.online",
        }
    }

    const ALL: [Self; 6] = [
        Self::Audio,
        Self::Display,
        Self::Hud,
        Self::Controls,
        Self::Keys,
        Self::Online,
    ];

    fn rows(self) -> &'static [Row] {
        use Row::*;
        match self {
            Self::Audio => &[Music, MusicVolume, Effects, EffectsVolume, EngineVolume],
            Self::Display => &[
                Fullscreen,
                Vsync,
                Antialiasing,
                FpsCap,
                RenderScale,
                UiScale,
                TvMargin,
                Fov,
                Camera,
                ShowFps,
            ],
            Self::Hud => &[
                Units,
                Minimap,
                GMeter,
                Countdown,
                ColourBlind,
                HighContrast,
                ReducedMotion,
                Language,
            ],
            Self::Controls => &[Steering, Deadzone, Rumble, StickyPedals, Assists],
            Self::Keys => &[
                Bind(0),
                Bind(1),
                Bind(2),
                Bind(3),
                Bind(4),
                Bind(5),
                Bind(6),
                Bind(7),
                Bind(8),
                Defaults,
            ],
            Self::Online => &[Online, Name, Country, Pending, Forget],
        }
    }
}

/// One line of the page: what it is called, what it says now, how it moves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Row {
    Music,
    MusicVolume,
    Effects,
    EffectsVolume,
    EngineVolume,
    Fullscreen,
    Vsync,
    Antialiasing,
    FpsCap,
    RenderScale,
    UiScale,
    TvMargin,
    Fov,
    Camera,
    ShowFps,
    Units,
    Minimap,
    GMeter,
    Countdown,
    ColourBlind,
    HighContrast,
    ReducedMotion,
    Language,
    StickyPedals,
    Assists,
    Steering,
    Deadzone,
    Rumble,
    Online,
    Name,
    Country,
    Pending,
    Forget,
    /// One action's key and button, by its place in [`super::bindings::Act::ALL`].
    Bind(u8),
    Defaults,
}

/// Countries offered on the page, as two-letter codes; empty is none.
pub(crate) const COUNTRIES: [&str; 24] = [
    "", "AT", "DE", "CH", "GB", "IE", "FR", "IT", "ES", "PT", "NL", "BE", "DK", "SE", "NO", "FI",
    "PL", "CZ", "HU", "US", "CA", "BR", "JP", "AU",
];

/// The most rows any tab has; that many row nodes are built once.
const ROWS: usize = 10;
const FPS_CAPS: [u32; 5] = [0, 30, 60, 120, 144];

fn on_off(on: bool) -> String {
    crate::text::t(if on { "word.on" } else { "word.off" }).into()
}

fn percent(value: f32) -> String {
    format!("{:.0}%", value * 100.0)
}

/// Step `value` by `step` in `dir`, kept inside `low..=high`, rounded to the
/// step so repeated presses land on round numbers.
fn nudge(value: f32, dir: i32, step: f32, low: f32, high: f32) -> f32 {
    ((value / step).round() + dir as f32).clamp(low / step, high / step) * step
}

/// Move through a short list, wrapping round.
fn cycle<T: PartialEq + Copy>(options: &[T], now: T, dir: i32) -> T {
    let at = options.iter().position(|o| *o == now).unwrap_or(0) as i32;
    options[(at + dir).rem_euclid(options.len() as i32) as usize]
}

impl Row {
    fn name(self) -> &'static str {
        match self {
            Self::Music => crate::text::t("row.music"),
            Self::MusicVolume => crate::text::t("row.music_volume"),
            Self::Effects => crate::text::t("row.effects"),
            Self::EffectsVolume => crate::text::t("row.effects_volume"),
            Self::EngineVolume => crate::text::t("row.engine_volume"),
            Self::Fullscreen => crate::text::t("row.fullscreen"),
            Self::Vsync => crate::text::t("row.vsync"),
            Self::Antialiasing => crate::text::t("row.antialiasing"),
            Self::FpsCap => crate::text::t("row.fps_cap"),
            Self::RenderScale => crate::text::t("row.render_scale"),
            Self::UiScale => crate::text::t("row.ui_scale"),
            Self::TvMargin => crate::text::t("row.tv_margin"),
            Self::Fov => crate::text::t("row.fov"),
            Self::Camera => crate::text::t("row.camera"),
            Self::ShowFps => crate::text::t("row.show_fps"),
            Self::Units => crate::text::t("row.units"),
            Self::Minimap => crate::text::t("row.minimap"),
            Self::GMeter => crate::text::t("row.g_meter"),
            Self::Countdown => crate::text::t("row.countdown"),
            Self::ColourBlind => crate::text::t("row.colour_blind"),
            Self::HighContrast => crate::text::t("row.high_contrast"),
            Self::ReducedMotion => crate::text::t("row.reduced_motion"),
            Self::StickyPedals => crate::text::t("row.sticky"),
            Self::Assists => crate::text::t("row.assists"),
            Self::Steering => crate::text::t("row.steering"),
            Self::Deadzone => crate::text::t("row.deadzone"),
            Self::Rumble => crate::text::t("row.rumble"),
            Self::Online => crate::text::t("row.online"),
            Self::Name => crate::text::t("row.name"),
            Self::Country => crate::text::t("row.country"),
            Self::Pending => crate::text::t("row.pending"),
            Self::Forget => crate::text::t("row.forget"),
            Self::Bind(at) => super::bindings::Act::ALL[at as usize].name(),
            Self::Language => crate::text::t("row.language"),
            Self::Defaults => crate::text::t("row.defaults"),
        }
    }

    pub(crate) fn value(self, s: &Settings) -> String {
        match self {
            Self::Music => on_off(s.music),
            Self::MusicVolume => percent(s.music_volume),
            Self::Effects => on_off(s.effects),
            Self::EffectsVolume => percent(s.effects_volume),
            Self::EngineVolume => percent(s.engine_volume),
            Self::Fullscreen => on_off(s.fullscreen),
            Self::Vsync => on_off(s.vsync),
            Self::Antialiasing => if s.antialiasing {
                "4×"
            } else {
                crate::text::t("word.off")
            }
            .into(),
            Self::FpsCap => match s.fps_cap {
                0 => crate::text::t("word.off").into(),
                cap => format!("{cap} fps"),
            },
            Self::RenderScale => percent(s.render_scale),
            Self::UiScale => percent(s.ui_scale),
            Self::TvMargin => on_off(s.tv_margin),
            Self::Fov => format!("{:.0}°", s.fov),
            Self::Camera => s.camera.name().into(),
            Self::ShowFps => on_off(s.show_fps),
            Self::Units => match s.units {
                Units::Kmh => "km/h",
                Units::Mph => "mph",
            }
            .into(),
            Self::Minimap => on_off(s.minimap),
            Self::GMeter => on_off(s.g_meter),
            Self::Countdown => match s.countdown {
                Countdown::Full => crate::text::t("value.always_full"),
                Countdown::Short => crate::text::t("value.short"),
                Countdown::Off => crate::text::t("word.off"),
            }
            .into(),
            Self::ColourBlind => on_off(s.colour_blind),
            Self::HighContrast => on_off(s.high_contrast),
            Self::ReducedMotion => on_off(s.reduced_motion),
            Self::Language => crate::text::Language::from_code(&s.language)
                .map_or(
                    crate::text::t("value.automatic"),
                    crate::text::Language::name,
                )
                .into(),
            Self::StickyPedals => on_off(s.sticky_pedals),
            Self::Assists => on_off(s.assists),
            Self::Steering => percent(s.steering),
            Self::Deadzone => percent(s.deadzone),
            Self::Rumble => on_off(s.rumble),
            Self::Online if s.online && !s.online_note.is_empty() => s.online_note.clone(),
            Self::Online => on_off(s.online),
            Self::Name if s.name.is_empty() => "—".into(),
            Self::Name => s.name.clone(),
            Self::Country if s.country.is_empty() => crate::text::t("word.none").into(),
            Self::Country => s.country.clone(),
            Self::Pending => s.pending.to_string(),
            Self::Bind(at) => s.bindings.shown(super::bindings::Act::ALL[at as usize]),
            Self::Defaults => "Enter".into(),
            Self::Forget => match s.forget_presses {
                0 => crate::text::t("value.press").into(),
                _ => crate::text::t("value.press_again").into(),
            },
        }
    }

    /// Move the value one step; `dir` is -1 or 1. A switch flips either way.
    pub(crate) fn change(self, s: &mut Settings, dir: i32) {
        match self {
            Self::Music => s.music = !s.music,
            Self::MusicVolume => s.music_volume = nudge(s.music_volume, dir, 0.1, 0.0, 1.0),
            Self::Effects => s.effects = !s.effects,
            Self::EffectsVolume => s.effects_volume = nudge(s.effects_volume, dir, 0.1, 0.0, 1.0),
            Self::EngineVolume => s.engine_volume = nudge(s.engine_volume, dir, 0.1, 0.0, 1.0),
            Self::Fullscreen => s.fullscreen = !s.fullscreen,
            Self::Vsync => s.vsync = !s.vsync,
            Self::Antialiasing => s.antialiasing = !s.antialiasing,
            Self::FpsCap => s.fps_cap = cycle(&FPS_CAPS, s.fps_cap, dir),
            Self::RenderScale => s.render_scale = cycle(&[0.5, 0.75, 1.0], s.render_scale, dir),
            Self::UiScale => s.ui_scale = nudge(s.ui_scale, dir, 0.1, 0.9, 1.5),
            Self::TvMargin => s.tv_margin = !s.tv_margin,
            Self::Fov => s.fov = nudge(s.fov, dir, 5.0, 35.0, 75.0),
            Self::Camera => s.camera = cycle(&CameraView::ALL, s.camera, dir),
            Self::ShowFps => s.show_fps = !s.show_fps,
            Self::Units => s.units = cycle(&[Units::Kmh, Units::Mph], s.units, dir),
            Self::Minimap => s.minimap = !s.minimap,
            Self::GMeter => s.g_meter = !s.g_meter,
            Self::Countdown => {
                s.countdown = cycle(
                    &[Countdown::Full, Countdown::Short, Countdown::Off],
                    s.countdown,
                    dir,
                );
            }
            Self::ColourBlind => s.colour_blind = !s.colour_blind,
            Self::HighContrast => s.high_contrast = !s.high_contrast,
            Self::ReducedMotion => s.reduced_motion = !s.reduced_motion,
            Self::Language => {
                let codes = ["auto", "en", "de", "fr", "es", "it"];
                let at = codes.iter().position(|c| *c == s.language).unwrap_or(0) as i32;
                s.language = codes[(at + dir).rem_euclid(codes.len() as i32) as usize].into();
            }
            Self::StickyPedals => s.sticky_pedals = !s.sticky_pedals,
            Self::Assists => s.assists = !s.assists,
            Self::Steering => s.steering = nudge(s.steering, dir, 0.1, 0.5, 1.5),
            Self::Deadzone => s.deadzone = nudge(s.deadzone, dir, 0.02, 0.02, 0.4),
            Self::Rumble => s.rumble = !s.rumble,
            Self::Online => {
                s.online = !s.online;
                s.online_asked = true;
                if s.name.is_empty() {
                    s.name = crate::online::suggest_name(0);
                }
            }
            Self::Name => {
                let at = crate::online::NAMES
                    .iter()
                    .position(|n| s.name.starts_with(n))
                    .unwrap_or(0) as i32;
                s.name = crate::online::suggest_name(
                    (at + dir).rem_euclid(crate::online::NAMES.len() as i32) as usize,
                );
            }
            Self::Country => {
                let code = COUNTRIES.iter().position(|c| *c == s.country).unwrap_or(0) as i32;
                s.country =
                    COUNTRIES[(code + dir).rem_euclid(COUNTRIES.len() as i32) as usize].into();
            }
            Self::Pending => {}
            Self::Forget => s.forget_presses = s.forget_presses.saturating_add(1).min(2),
            // Rebinding waits for a key; see `drive`.
            Self::Bind(_) => {}
            Self::Defaults => s.bindings = super::bindings::Bindings::default(),
        }
    }
}

/// Which tab and row the cursor is on.
#[derive(Resource, Default)]
pub(crate) struct Page {
    tab: usize,
    row: usize,
    navigation: Navigation,
    /// Waiting for a key or button for this row, until this time.
    capturing: Option<(usize, f64)>,
}

/// How long a rebind waits for a key or button.
const CAPTURE_FOR: f64 = 5.0;

impl Page {
    /// Open on a tab and row, for a visual check.
    #[cfg(feature = "visual-check")]
    pub(crate) fn show(&mut self, tab: usize, row: usize) {
        self.tab = tab.min(Tab::ALL.len() - 1);
        self.row = row.min(self.tab().rows().len() - 1);
    }

    fn tab(&self) -> Tab {
        Tab::ALL[self.tab]
    }
    fn row(&self) -> Row {
        self.tab().rows()[self.row]
    }
}

#[derive(Component)]
struct Panel;
#[derive(Component)]
struct TabLabel(usize);
#[derive(Component)]
struct RowLine(usize);
#[derive(Component)]
struct RowName(usize);
#[derive(Component)]
struct RowValue(usize);

/// The page reads its keys here.
#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct PageSet;

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Page>()
        .add_systems(Startup, setup)
        .add_systems(PreUpdate, drive.in_set(PageSet).after(HaltSet))
        .add_systems(Update, draw);
}

fn setup(mut commands: Commands) {
    commands
        .spawn((
            Panel,
            GlobalZIndex(16),
            Node {
                position_type: PositionType::Absolute,
                width: percent_of(100.0),
                height: percent_of(100.0),
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
                        width: px(640),
                        padding: UiRect::all(px(30)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(16)),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(10),
                        ..default()
                    },
                    BackgroundColor(FRONT),
                    BorderColor::all(LINE),
                ))
                .with_children(|panel| {
                    panel.spawn(crate::text::label("settings.label", 12.0, AMBER));
                    panel
                        .spawn(Node {
                            column_gap: px(8),
                            margin: UiRect::bottom(px(8)),
                            ..default()
                        })
                        .with_children(|tabs| {
                            for (i, tab) in Tab::ALL.iter().enumerate() {
                                tabs.spawn((
                                    TabLabel(i),
                                    Button,
                                    Node {
                                        padding: UiRect::axes(px(14), px(8)),
                                        border_radius: BorderRadius::all(px(8)),
                                        ..default()
                                    },
                                    BackgroundColor(SURFACE),
                                ))
                                .with_children(|tab_node| {
                                    tab_node.spawn(crate::text::label(tab.key(), 17.0, TEXT));
                                });
                            }
                        });
                    for i in 0..ROWS {
                        panel
                            .spawn((
                                RowLine(i),
                                Button,
                                Node {
                                    padding: UiRect::axes(px(14), px(9)),
                                    border: UiRect::all(px(2)),
                                    border_radius: BorderRadius::all(px(8)),
                                    justify_content: JustifyContent::SpaceBetween,
                                    ..default()
                                },
                                BackgroundColor(SURFACE),
                                BorderColor::all(LINE),
                            ))
                            .with_children(|row| {
                                row.spawn((RowName(i), label("", 17.0, TEXT)));
                                row.spawn((RowValue(i), label("", 17.0, TEXT)));
                            });
                    }
                    panel.spawn(crate::text::label("settings.hint", 13.0, AMBER_DIM));
                });
        });
}

fn percent_of(value: f32) -> Val {
    Val::Percent(value)
}

/// Keys, pad and pointer, while the page is up.
#[allow(clippy::too_many_arguments)]
fn drive(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    time: Res<Time<Real>>,
    mut halt: ResMut<Halt>,
    mut page: ResMut<Page>,
    mut settings: ResMut<Settings>,
    rows: Query<(&RowLine, Ref<Interaction>)>,
    tabs: Query<(&TabLabel, Ref<Interaction>)>,
    mut typed: MessageReader<bevy::input::keyboard::KeyboardInput>,
    title: Option<Res<crate::title::Title>>,
) {
    if *halt != Halt::Settings {
        typed.clear();
        return;
    }
    // The press that opened the page is not also a press on its first row.
    if halt.is_changed() {
        *page = Page::default();
        return;
    }
    let pad = |button| pads.iter().any(|pad| pad.just_pressed(button));
    let now = time.elapsed_secs_f64();
    if let Some((row, until)) = page.capturing {
        typed.clear();
        if keys.just_pressed(KeyCode::Escape) || now > until {
            page.capturing = None;
            return;
        }
        let Row::Bind(at) = page.tab().rows()[row] else {
            page.capturing = None;
            return;
        };
        let act = super::bindings::Act::ALL[at as usize];
        let bindings = &mut settings.bindings;
        let bound = match (
            super::bindings::Bindings::pressed_key(&keys),
            super::bindings::Bindings::pressed_pad(&pads),
        ) {
            (Some(key), _) => bindings.bind_key(act, key),
            (None, Some(button)) => bindings.bind_pad(act, button),
            (None, None) => false,
        };
        if bound {
            page.capturing = None;
        }
        return;
    }
    // On the name row the keyboard types; Q and E are letters there.
    let naming = page.row() == Row::Name;
    for key in typed.read() {
        if naming && key.state.is_pressed() {
            type_into(&mut settings.name, &key.logical_key);
        }
    }
    if keys.just_pressed(KeyCode::Escape) || pad(GamepadButton::East) || pad(GamepadButton::Start) {
        *halt = crate::title::Title::back_to(title.as_deref());
        return;
    }
    let key = |code| !naming && keys.just_pressed(code);
    let tab_step = i32::from(key(KeyCode::KeyE) || pad(GamepadButton::RightTrigger))
        - i32::from(key(KeyCode::KeyQ) || pad(GamepadButton::LeftTrigger));
    let mut clicked_tab = None;
    for (tab, interaction) in &tabs {
        if interaction.is_changed() && *interaction == Interaction::Pressed {
            clicked_tab = Some(tab.0);
        }
    }
    if tab_step != 0 || clicked_tab.is_some() {
        page.tab = clicked_tab.unwrap_or_else(|| {
            (page.tab as i32 + tab_step).rem_euclid(Tab::ALL.len() as i32) as usize
        });
        page.row = 0;
        return;
    }
    let count = page.tab().rows().len();
    let mut clicked = false;
    for (row, interaction) in &rows {
        if interaction.is_changed() && *interaction == Interaction::Pressed && row.0 < count {
            page.row = row.0;
            clicked = true;
        }
    }
    let step = page.navigation.read(&keys, &pads, now);
    if step.y != 0 {
        page.row = (page.row as i32 + step.y).clamp(0, count as i32 - 1) as usize;
    }
    let forward = clicked || keys.just_pressed(KeyCode::Enter) || pad(GamepadButton::South);
    if forward && matches!(page.row(), Row::Bind(_)) {
        page.capturing = Some((page.row, now + CAPTURE_FOR));
        return;
    }
    let dir = if forward { 1 } else { step.x };
    if dir != 0 {
        let row = page.row();
        row.change(&mut settings, dir);
    }
}

/// A key pressed on the name row: a letter, digit or allowed mark joins the
/// name (up to 16), Backspace takes one off.
fn type_into(name: &mut String, key: &bevy::input::keyboard::Key) {
    use bevy::input::keyboard::Key;
    match key {
        Key::Backspace => {
            name.pop();
        }
        Key::Character(text) => {
            for c in text.chars() {
                let allowed = c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.');
                if allowed && name.chars().count() < 16 {
                    name.push(c);
                }
            }
        }
        Key::Space if name.chars().count() < 16 => name.push(' '),
        _ => {}
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw(
    halt: Res<Halt>,
    page: Res<Page>,
    settings: Res<Settings>,
    mut panels: Query<&mut Visibility, With<Panel>>,
    mut tabs: Query<(&TabLabel, &Children, &mut BackgroundColor), Without<RowLine>>,
    mut lines: Query<
        (&RowLine, &mut Node, &mut BackgroundColor, &mut BorderColor),
        Without<TabLabel>,
    >,
    mut names: Query<(&RowName, &mut Text, &mut TextColor), Without<RowValue>>,
    mut values: Query<(&RowValue, &mut Text, &mut TextColor), Without<RowName>>,
    mut tab_text: Query<&mut TextColor, (Without<RowName>, Without<RowValue>)>,
) {
    let open = *halt == Halt::Settings;
    for mut visibility in &mut panels {
        visibility.set_if_neq(if open {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
    if !open || !(page.is_changed() || settings.is_changed() || halt.is_changed()) {
        return;
    }
    let rows = page.tab().rows();
    for (tab, children, mut background) in &mut tabs {
        let active = tab.0 == page.tab;
        background.set_if_neq(BackgroundColor(if active { AMBER } else { SURFACE }));
        for child in children.iter() {
            if let Ok(mut colour) = tab_text.get_mut(child) {
                colour.set_if_neq(TextColor(if active { FRONT } else { TEXT }));
            }
        }
    }
    for (line, mut node, mut background, mut border) in &mut lines {
        let display = if line.0 < rows.len() {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
        let active = line.0 == page.row;
        background.set_if_neq(BackgroundColor(if active { AMBER } else { SURFACE }));
        *border = BorderColor::all(if active { AMBER } else { LINE });
    }
    for (name, mut text, mut colour) in &mut names {
        if let Some(row) = rows.get(name.0) {
            if text.0 != row.name() {
                text.0 = row.name().into();
            }
            colour.set_if_neq(TextColor(if name.0 == page.row { FRONT } else { TEXT }));
        }
    }
    for (value, mut text, mut colour) in &mut values {
        if let Some(row) = rows.get(value.0) {
            let wanted = if page.capturing.is_some_and(|(at, _)| at == value.0) {
                crate::text::t("value.waiting").to_string()
            } else {
                row.value(&settings)
            };
            if text.0 != wanted {
                text.0 = wanted;
            }
            colour.set_if_neq(TextColor(if value.0 == page.row { FRONT } else { AMBER }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_changes_both_ways_and_stays_in_range() {
        for tab in Tab::ALL {
            assert!(
                tab.rows().len() <= ROWS,
                "{tab:?} has more rows than the page"
            );
            for row in tab.rows() {
                let mut settings = Settings::default();
                // Read-only, or changed by pressing the key itself.
                if matches!(row, Row::Pending | Row::Bind(_) | Row::Defaults) {
                    continue;
                }
                let before = row.value(&settings);
                let mut up = settings.clone();
                row.change(&mut up, 1);
                row.change(&mut settings, -1);
                assert!(
                    row.value(&up) != before || row.value(&settings) != before,
                    "{row:?} did not move"
                );
                for _ in 0..40 {
                    row.change(&mut settings, 1);
                }
                for _ in 0..80 {
                    row.change(&mut settings, -1);
                }
                let text = settings.text();
                // What the page shows but the file never keeps.
                settings.forget_presses = 0;
                settings.pending = 0;
                settings.online_note.clear();
                assert_eq!(Settings::parse(&text), settings, "{row:?} left the range");
            }
        }
    }

    #[test]
    fn steps_land_on_round_numbers() {
        let mut settings = Settings::default();
        for _ in 0..3 {
            Row::MusicVolume.change(&mut settings, -1);
        }
        assert_eq!(Row::MusicVolume.value(&settings), "50%");
        Row::Fov.change(&mut settings, 1);
        assert_eq!(Row::Fov.value(&settings), "50°");
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Settings>()
            .insert_resource(Halt::Pause)
            .init_resource::<Page>()
            .add_message::<bevy::input::keyboard::KeyboardInput>()
            .add_systems(Update, drive);
        app.update();
        app
    }

    fn tap(app: &mut App, key: KeyCode) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(key);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(key);
        keys.clear();
    }

    #[test]
    fn the_keyboard_reaches_every_row_and_goes_back_to_the_pause() {
        let mut app = app();
        *app.world_mut().resource_mut::<Halt>() = Halt::Settings;
        app.update();
        // The first row is the radio: Enter flips it.
        tap(&mut app, KeyCode::Enter);
        assert!(!app.world().resource::<Settings>().music);
        for tab in Tab::ALL {
            for _ in 1..tab.rows().len() {
                tap(&mut app, KeyCode::ArrowDown);
            }
            assert_eq!(
                app.world().resource::<Page>().row(),
                *tab.rows().last().unwrap()
            );
            tap(&mut app, KeyCode::KeyE);
        }
        assert_eq!(
            app.world().resource::<Page>().tab(),
            Tab::Audio,
            "tabs wrap"
        );
        tap(&mut app, KeyCode::ArrowRight);
        tap(&mut app, KeyCode::Escape);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Pause);
    }

    #[test]
    fn a_name_is_typed_with_the_rules_the_boards_keep() {
        use bevy::input::keyboard::Key;
        let mut name = String::new();
        for text in ["A", "m", "b", "<", "3", "r"] {
            type_into(&mut name, &Key::Character(text.into()));
        }
        type_into(&mut name, &Key::Space);
        type_into(&mut name, &Key::Backspace);
        assert_eq!(name, "Amb3r");
        for _ in 0..30 {
            type_into(&mut name, &Key::Character("x".into()));
        }
        assert_eq!(name.chars().count(), 16);
    }

    #[test]
    fn a_key_row_waits_for_the_key_and_takes_it() {
        let mut app = app();
        *app.world_mut().resource_mut::<Halt>() = Halt::Settings;
        app.update();
        while app.world().resource::<Page>().tab() != Tab::Keys {
            tap(&mut app, KeyCode::KeyE);
        }
        tap(&mut app, KeyCode::Enter);
        assert!(app.world().resource::<Page>().capturing.is_some());
        tap(&mut app, KeyCode::KeyI);
        assert!(app.world().resource::<Page>().capturing.is_none());
        let settings = app.world().resource::<Settings>();
        assert_eq!(
            settings.bindings.key(super::super::bindings::Act::Throttle),
            Some(KeyCode::KeyI)
        );
        // Esc gives up waiting without leaving the page.
        tap(&mut app, KeyCode::Enter);
        tap(&mut app, KeyCode::Escape);
        assert!(app.world().resource::<Page>().capturing.is_none());
        assert_eq!(*app.world().resource::<Halt>(), Halt::Settings);
    }

    #[test]
    fn the_press_that_opens_the_page_does_not_change_a_row() {
        let mut app = app();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        *app.world_mut().resource_mut::<Halt>() = Halt::Settings;
        app.update();
        assert!(app.world().resource::<Settings>().music);
    }
}
