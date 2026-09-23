//! The settings page: four tabs of rows, driven like the pause menu.
//!
//! Up and down choose a row, left and right change its value (Enter or A
//! steps it forward too, which is how a switch is flipped), LB and RB or Q and
//! E change tab, and Esc, B or Start go back to the pause. A click chooses a row
//! and steps it. Every change lands in [`Settings`] at once and is saved a
//! second later, so there is no Apply button to forget.

use bevy::prelude::*;

use super::{Countdown, Settings, Units};
use crate::hud::{AMBER, AMBER_DIM, FRONT};
use crate::pause::{Halt, HaltSet};
use crate::ui::{LINE, Navigation, SURFACE, TEXT, label};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Tab {
    Audio,
    Display,
    Hud,
    Controls,
}

impl Tab {
    const ALL: [Self; 4] = [Self::Audio, Self::Display, Self::Hud, Self::Controls];

    fn name(self) -> &'static str {
        match self {
            Self::Audio => "Audio",
            Self::Display => "Display",
            Self::Hud => "HUD",
            Self::Controls => "Controls",
        }
    }

    fn rows(self) -> &'static [Row] {
        use Row::*;
        match self {
            Self::Audio => &[Music, MusicVolume, Effects, EffectsVolume],
            Self::Display => &[
                Fullscreen,
                Vsync,
                Antialiasing,
                FpsCap,
                RenderScale,
                UiScale,
                TvMargin,
                Fov,
                ShowFps,
            ],
            Self::Hud => &[Units, Minimap, GMeter, Countdown],
            Self::Controls => &[Steering, Deadzone, Rumble],
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
    Fullscreen,
    Vsync,
    Antialiasing,
    FpsCap,
    RenderScale,
    UiScale,
    TvMargin,
    Fov,
    ShowFps,
    Units,
    Minimap,
    GMeter,
    Countdown,
    Steering,
    Deadzone,
    Rumble,
}

/// The most rows any tab has; that many row nodes are built once.
const ROWS: usize = 9;
const FPS_CAPS: [u32; 5] = [0, 30, 60, 120, 144];

fn on_off(on: bool) -> String {
    if on { "On" } else { "Off" }.into()
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
            Self::Music => "Radio",
            Self::MusicVolume => "Music volume",
            Self::Effects => "Tyre sound and beeps",
            Self::EffectsVolume => "Effects volume",
            Self::Fullscreen => "Full screen",
            Self::Vsync => "Vertical sync",
            Self::Antialiasing => "Anti-aliasing",
            Self::FpsCap => "Frame limit",
            Self::RenderScale => "Render scale",
            Self::UiScale => "Text and HUD size",
            Self::TvMargin => "TV-safe margin",
            Self::Fov => "Field of view",
            Self::ShowFps => "Show frame rate",
            Self::Units => "Speed units",
            Self::Minimap => "Mini-map",
            Self::GMeter => "G-meter",
            Self::Countdown => "Start countdown",
            Self::Steering => "Steering sensitivity",
            Self::Deadzone => "Stick dead zone",
            Self::Rumble => "Rumble",
        }
    }

    pub(crate) fn value(self, s: &Settings) -> String {
        match self {
            Self::Music => on_off(s.music),
            Self::MusicVolume => percent(s.music_volume),
            Self::Effects => on_off(s.effects),
            Self::EffectsVolume => percent(s.effects_volume),
            Self::Fullscreen => on_off(s.fullscreen),
            Self::Vsync => on_off(s.vsync),
            Self::Antialiasing => if s.antialiasing { "4×" } else { "Off" }.into(),
            Self::FpsCap => match s.fps_cap {
                0 => "Off".into(),
                cap => format!("{cap} fps"),
            },
            Self::RenderScale => percent(s.render_scale),
            Self::UiScale => percent(s.ui_scale),
            Self::TvMargin => on_off(s.tv_margin),
            Self::Fov => format!("{:.0}°", s.fov),
            Self::ShowFps => on_off(s.show_fps),
            Self::Units => match s.units {
                Units::Kmh => "km/h",
                Units::Mph => "mph",
            }
            .into(),
            Self::Minimap => on_off(s.minimap),
            Self::GMeter => on_off(s.g_meter),
            Self::Countdown => match s.countdown {
                Countdown::Full => "Always full",
                Countdown::Short => "Short on restart",
                Countdown::Off => "Off",
            }
            .into(),
            Self::Steering => percent(s.steering),
            Self::Deadzone => percent(s.deadzone),
            Self::Rumble => on_off(s.rumble),
        }
    }

    /// Move the value one step; `dir` is -1 or 1. A switch flips either way.
    pub(crate) fn change(self, s: &mut Settings, dir: i32) {
        match self {
            Self::Music => s.music = !s.music,
            Self::MusicVolume => s.music_volume = nudge(s.music_volume, dir, 0.1, 0.0, 1.0),
            Self::Effects => s.effects = !s.effects,
            Self::EffectsVolume => s.effects_volume = nudge(s.effects_volume, dir, 0.1, 0.0, 1.0),
            Self::Fullscreen => s.fullscreen = !s.fullscreen,
            Self::Vsync => s.vsync = !s.vsync,
            Self::Antialiasing => s.antialiasing = !s.antialiasing,
            Self::FpsCap => s.fps_cap = cycle(&FPS_CAPS, s.fps_cap, dir),
            Self::RenderScale => s.render_scale = cycle(&[0.5, 0.75, 1.0], s.render_scale, dir),
            Self::UiScale => s.ui_scale = nudge(s.ui_scale, dir, 0.1, 0.9, 1.5),
            Self::TvMargin => s.tv_margin = !s.tv_margin,
            Self::Fov => s.fov = nudge(s.fov, dir, 5.0, 35.0, 75.0),
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
            Self::Steering => s.steering = nudge(s.steering, dir, 0.1, 0.5, 1.5),
            Self::Deadzone => s.deadzone = nudge(s.deadzone, dir, 0.02, 0.02, 0.4),
            Self::Rumble => s.rumble = !s.rumble,
        }
    }
}

/// Which tab and row the cursor is on.
#[derive(Resource, Default)]
pub(crate) struct Page {
    tab: usize,
    row: usize,
    navigation: Navigation,
}

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
                    panel.spawn(label("TODORA  /  SETTINGS", 12.0, AMBER));
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
                                    tab_node.spawn(label(tab.name(), 17.0, TEXT));
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
                    panel.spawn(label(
                        "↑ ↓  Choose    ← →  Change    LB / RB  or  Q / E  Tab\nEsc / B  Back to the pause",
                        13.0,
                        AMBER_DIM,
                    ));
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
) {
    if *halt != Halt::Settings {
        return;
    }
    // The press that opened the page is not also a press on its first row.
    if halt.is_changed() {
        *page = Page::default();
        return;
    }
    let pad = |button| pads.iter().any(|pad| pad.just_pressed(button));
    if keys.just_pressed(KeyCode::Escape) || pad(GamepadButton::East) || pad(GamepadButton::Start) {
        *halt = Halt::Pause;
        return;
    }
    let tab_step = i32::from(keys.just_pressed(KeyCode::KeyE) || pad(GamepadButton::RightTrigger))
        - i32::from(keys.just_pressed(KeyCode::KeyQ) || pad(GamepadButton::LeftTrigger));
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
    let now = time.elapsed_secs_f64();
    let step = page.navigation.read(&keys, &pads, now);
    if step.y != 0 {
        page.row = (page.row as i32 + step.y).clamp(0, count as i32 - 1) as usize;
    }
    let forward = clicked || keys.just_pressed(KeyCode::Enter) || pad(GamepadButton::South);
    let dir = if forward { 1 } else { step.x };
    if dir != 0 {
        let row = page.row();
        row.change(&mut settings, dir);
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
            let wanted = row.value(&settings);
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
