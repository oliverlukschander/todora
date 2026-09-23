//! Circuit browsing and a garage with draft car/setup choices.
//! Browsing pauses the game; only Apply commits a selection.
mod view;
use crate::ui::Navigation;

use crate::{
    Reset,
    car::{Mode, Setup, Spec},
    input::InputSet,
    pause::{Halt, HaltSet},
    track::{GoTo, Track, all_circuits, circuit_at},
};
use bevy::{
    input::{
        ButtonState,
        keyboard::KeyboardInput,
        mouse::{MouseScrollUnit, MouseWheel},
    },
    prelude::*,
};

#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct MenuSet;

/// Open a menu page from elsewhere: the title screen's Circuits button.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct OpenMenu(pub Page);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Page {
    Car,
    Circuit,
}

impl Page {
    fn chosen(self, spec: &Spec, track: &Track) -> usize {
        match self {
            Self::Car => spec.at(),
            Self::Circuit => circuit_at(track.circuit()),
        }
    }
}

const PAGE_SIZE: usize = 6;

#[derive(Resource, Default)]
pub(crate) struct Menu {
    page: Option<Page>,
    at: usize,
    setup: Setup,
    mode: Mode,
    search: String,
}

impl Menu {
    fn entries(&self) -> Vec<usize> {
        match self.page {
            Some(Page::Car) => (0..Spec::ALL.len()).collect(),
            _ => {
                let needle = searchable(&self.search);
                all_circuits()
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| searchable(c.name).contains(&needle))
                    .map(|(i, _)| i)
                    .collect()
            }
        }
    }

    fn position(&self) -> usize {
        self.entries()
            .iter()
            .position(|i| *i == self.at)
            .unwrap_or(0)
    }

    fn move_by(&mut self, by: i32) {
        let entries = self.entries();
        if !entries.is_empty() {
            let next = (self.position() as i32 + by).clamp(0, entries.len() as i32 - 1);
            self.at = entries[next as usize];
        }
    }

    fn filter(&mut self) {
        let entries = self.entries();
        if !entries.contains(&self.at) {
            self.at = entries.first().copied().unwrap_or(0);
        }
    }
}

// Searching "nurburg" should find Nürburgring on any keyboard layout.
fn searchable(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .flat_map(|c| match c {
            'ä' | 'á' | 'à' | 'â' | 'ã' => "a".chars().collect::<Vec<_>>(),
            'ö' | 'ó' | 'ò' | 'ô' | 'õ' => vec!['o'],
            'ü' | 'ú' | 'ù' | 'û' => vec!['u'],
            'é' | 'è' | 'ê' | 'ë' => vec!['e'],
            'í' | 'ì' | 'î' | 'ï' => vec!['i'],
            'ç' => vec!['c'],
            'ñ' => vec!['n'],
            'ß' => vec!['s', 's'],
            '\u{300}'..='\u{36f}' => vec![],
            _ => vec![c],
        })
        .collect()
}

#[derive(Component, Clone, Copy)]
enum Action {
    Open(Page),
    Select(usize),
    Tune(Setup),
    Mode(Mode),
    Previous,
    Next,
    Clear,
    Apply,
    Back,
}

pub struct MenuPlugin;
impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<OpenMenu>();
        app.init_resource::<Menu>()
            .add_systems(Startup, view::setup)
            .add_systems(
                PreUpdate,
                (search, open, walk, clicks)
                    .chain()
                    .run_if(crate::multiplayer::offline)
                    .in_set(MenuSet)
                    .after(HaltSet)
                    .after(InputSet)
                    .after(bevy::ui::UiSystems::Focus),
            )
            .add_systems(Update, (view::draw, view::hover, view::show_launcher));
    }
}

fn pressed(
    keys: &ButtonInput<KeyCode>,
    pads: &Query<&Gamepad>,
    key: KeyCode,
    button: GamepadButton,
) -> bool {
    keys.just_pressed(key) || pads.iter().any(|pad| pad.just_pressed(button))
}

#[allow(clippy::too_many_arguments)] // Bevy-managed input and selection resources.
fn open(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    track: Res<Track>,
    spec: Res<Spec>,
    setup: Res<Setup>,
    mode: Res<Mode>,
    mut asked: MessageReader<OpenMenu>,
    mut menu: ResMut<Menu>,
    mut halt: ResMut<Halt>,
) {
    if let Some(OpenMenu(page)) = asked.read().last().copied() {
        enter(page, &mut menu, &mut halt, &spec, &setup, &mode, &track);
        return;
    }
    if let Some(page) = menu.page {
        if pads.iter().any(|pad| {
            pad.just_pressed(GamepadButton::LeftTrigger)
                || pad.just_pressed(GamepadButton::RightTrigger)
        }) {
            let next = match page {
                Page::Car => Page::Circuit,
                Page::Circuit => Page::Car,
            };
            enter(next, &mut menu, &mut halt, &spec, &setup, &mode, &track);
        }
        return;
    }
    if *halt != Halt::Nothing {
        return;
    }
    let page = if pressed(&keys, &pads, KeyCode::KeyC, GamepadButton::LeftTrigger) {
        Page::Car
    } else if pressed(&keys, &pads, KeyCode::KeyT, GamepadButton::Select) {
        Page::Circuit
    } else {
        return;
    };
    enter(page, &mut menu, &mut halt, &spec, &setup, &mode, &track);
}

fn enter(
    page: Page,
    menu: &mut Menu,
    halt: &mut Halt,
    spec: &Spec,
    setup: &Setup,
    mode: &Mode,
    track: &Track,
) {
    menu.at = page.chosen(spec, track);
    menu.setup = *setup;
    menu.mode = *mode;
    menu.search.clear();
    menu.page = Some(page);
    *halt = Halt::Menu;
}

fn search(
    mut events: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<Menu>,
) {
    for event in events.read() {
        if menu.page != Some(Page::Circuit)
            || event.state != ButtonState::Pressed
            || keys.any_pressed([
                KeyCode::ControlLeft,
                KeyCode::ControlRight,
                KeyCode::SuperLeft,
                KeyCode::SuperRight,
            ])
        {
            continue;
        }
        if event.key_code == KeyCode::Backspace {
            menu.search.pop();
            menu.filter();
        } else if let Some(text) = &event.text {
            let text: String = text.chars().filter(|c| !c.is_control()).collect();
            if !text.is_empty() && menu.search.chars().count() < 60 {
                menu.search.push_str(&text);
                menu.filter();
            }
        }
    }
}

#[allow(clippy::too_many_arguments)] // Each parameter is a Bevy-managed input or game resource.
fn walk(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut wheel: MessageReader<MouseWheel>,
    mut scroll_remainder: Local<f32>,
    mut navigation: Local<Navigation>,
    time: Res<Time<Real>>,
    mut menu: ResMut<Menu>,
    mut halt: ResMut<Halt>,
    mut spec: ResMut<Spec>,
    mut setup: ResMut<Setup>,
    mut mode: ResMut<Mode>,
    mut reset: MessageWriter<Reset>,
    mut go: MessageWriter<GoTo>,
) {
    let scroll: f32 = wheel
        .read()
        .map(|event| match event.unit {
            MouseScrollUnit::Line => event.y,
            MouseScrollUnit::Pixel => event.y / 40.0,
        })
        .sum();
    let Some(page) = menu.page else {
        *scroll_remainder = 0.0;
        *navigation = Navigation::default();
        return;
    };
    let nudge = navigation.read(&keys, &pads, time.elapsed_secs_f64());
    let step = nudge.y;
    let horizontal = nudge.x;
    if step != 0 {
        menu.move_by(step * if page == Page::Circuit { 2 } else { 1 });
    }
    if horizontal != 0 {
        if page == Page::Car {
            menu.setup = menu.setup.slid(horizontal);
        } else {
            menu.move_by(horizontal);
        }
    }
    if page == Page::Circuit {
        let page_step = i32::from(keys.just_pressed(KeyCode::PageDown))
            - i32::from(keys.just_pressed(KeyCode::PageUp));
        if page_step != 0 {
            menu.move_by(page_step * PAGE_SIZE as i32);
        }
        *scroll_remainder += scroll;
        let rows = scroll_remainder.trunc() as i32;
        if rows != 0 {
            menu.move_by(-2 * rows);
            *scroll_remainder -= rows as f32;
        }
        if keys.just_pressed(KeyCode::Home) {
            menu.move_by(-(all_circuits().len() as i32));
        }
        if keys.just_pressed(KeyCode::End) {
            menu.move_by(all_circuits().len() as i32);
        }
    } else {
        if pressed(&keys, &pads, KeyCode::KeyM, GamepadButton::North) {
            menu.mode = menu.mode.next();
        }
        for (key, wanted) in [KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3]
            .into_iter()
            .zip(Setup::ALL)
        {
            if keys.just_pressed(key) {
                menu.setup = wanted;
            }
        }
    }
    if pressed(&keys, &pads, KeyCode::Escape, GamepadButton::East)
        || pads
            .iter()
            .any(|pad| pad.just_pressed(GamepadButton::Start))
    {
        close(&mut menu, &mut halt);
    } else if pressed(&keys, &pads, KeyCode::Enter, GamepadButton::South) {
        apply(
            &mut menu, &mut halt, &mut spec, &mut setup, &mut mode, &mut reset, &mut go,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn clicks(
    mouse: Res<ButtonInput<MouseButton>>,
    buttons: Query<(&Interaction, &Action), Changed<Interaction>>,
    mut menu: ResMut<Menu>,
    mut halt: ResMut<Halt>,
    track: Res<Track>,
    mut spec: ResMut<Spec>,
    mut setup: ResMut<Setup>,
    mut mode: ResMut<Mode>,
    mut reset: MessageWriter<Reset>,
    mut go: MessageWriter<GoTo>,
) {
    for (interaction, action) in &buttons {
        if *interaction != Interaction::Pressed || !mouse.just_pressed(MouseButton::Left) {
            continue;
        }
        if let Action::Open(page) = action {
            if *halt != Halt::Pause && menu.page != Some(*page) {
                enter(*page, &mut menu, &mut halt, &spec, &setup, &mode, &track);
            }
            continue;
        }
        if menu.page.is_none() {
            continue;
        }
        match action {
            Action::Select(at) => menu.at = *at,
            Action::Tune(wanted) => menu.setup = *wanted,
            Action::Mode(wanted) => menu.mode = *wanted,
            Action::Previous => menu.move_by(-(PAGE_SIZE as i32)),
            Action::Next => menu.move_by(PAGE_SIZE as i32),
            Action::Clear => {
                menu.search.clear();
                menu.filter();
            }
            Action::Apply => apply(
                &mut menu, &mut halt, &mut spec, &mut setup, &mut mode, &mut reset, &mut go,
            ),
            Action::Back => close(&mut menu, &mut halt),
            Action::Open(_) => (),
        }
    }
}

fn apply(
    menu: &mut Menu,
    halt: &mut Halt,
    spec: &mut Spec,
    setup: &mut Setup,
    mode: &mut ResMut<Mode>,
    reset: &mut MessageWriter<Reset>,
    go: &mut MessageWriter<GoTo>,
) {
    if menu.entries().is_empty() {
        return;
    }
    match menu.page {
        Some(Page::Car) => {
            let wanted = Spec::ALL[menu.at];
            if wanted != *spec || menu.setup != *setup || menu.mode != **mode {
                *spec = wanted;
                *setup = menu.setup;
                mode.set_if_neq(menu.mode);
                reset.write(Reset);
            }
        }
        Some(Page::Circuit) => {
            go.write(GoTo(&all_circuits()[menu.at]));
        }
        None => return,
    }
    close(menu, halt);
}

fn close(menu: &mut Menu, halt: &mut Halt) {
    menu.page = None;
    *halt = Halt::Nothing;
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::pause::PausePlugin;

    /// Keep the catalogue metadata accurate without building tracks while browsing.
    #[test]
    fn the_menu_shows_the_lap_it_will_drive() {
        for circuit in all_circuits() {
            let built = Track::new(circuit).length();
            assert!(
                (circuit.lap - built).abs() < 0.1,
                "{}: the module says {:.1} m, but the circuit builds {built:.1} m",
                circuit.name,
                circuit.lap,
            );
        }
    }

    fn game() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<Reset>()
            .add_message::<GoTo>()
            .init_resource::<ButtonInput<KeyCode>>()
            // The two resources the pages read, which `CarPlugin` and
            // `TrackPlugin` own in the game.
            .init_resource::<Spec>()
            .init_resource::<Setup>()
            .init_resource::<Mode>()
            .init_resource::<ButtonInput<MouseButton>>()
            .add_message::<KeyboardInput>()
            .add_message::<MouseWheel>()
            .insert_resource(Track::any())
            .add_plugins((PausePlugin, MenuPlugin));
        app.world_mut().resource_mut::<Schedules>().remove(Startup);
        app
    }

    /// One key, tapped: nothing here runs Bevy's own input plugin, so the
    /// keyboard is worked by hand. Let go of everything first, because a key
    /// that is already down cannot be pressed again.
    fn press(app: &mut App, key: KeyCode) {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release_all();
        keys.press(key);
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
    }

    /// A menu is one of the things that can stand in front of the game, so it
    /// stops the world the same way a pause does — and gives it back.
    #[test]
    fn opening_a_menu_stops_the_game_and_closing_it_starts_it_again() {
        let mut app = game();
        app.update();
        assert_eq!(*app.world().resource::<Halt>(), Halt::Nothing);

        press(&mut app, KeyCode::KeyC);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Menu);
        assert_eq!(app.world().resource::<Menu>().page, Some(Page::Car));

        press(&mut app, KeyCode::Escape);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Nothing);
        assert!(app.world().resource::<Menu>().page.is_none());
    }

    /// `Esc` and `Enter` belong to whichever of the two is in front of the game.
    /// With a menu up, `Esc` backs out of it rather than paving a pause over it,
    /// and `Enter` takes a row rather than starting the game again underneath.
    #[test]
    fn the_pause_keys_belong_to_the_menu_while_it_is_up() {
        let mut app = game();
        app.update();
        press(&mut app, KeyCode::KeyC);

        // Enter takes the row under the cursor, which is the car already being
        // driven, and the game is running again with nothing changed.
        press(&mut app, KeyCode::Enter);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Nothing);
        assert_eq!(*app.world().resource::<Spec>(), Spec::default());
        assert_eq!(app.world().resource::<Messages<Reset>>().len(), 0);

        // And a pause is still a pause: Esc stops the game, Enter starts it.
        press(&mut app, KeyCode::Escape);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Pause);
        press(&mut app, KeyCode::Enter);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Nothing);
    }

    /// The cursor opens on what is already chosen, moves one row at a time, and
    /// stops at both ends rather than wrapping — a list this short is read, and
    /// a cursor that reappears at the other end has to be found again.
    #[test]
    fn the_cursor_starts_on_the_chosen_row_and_stops_at_both_ends() {
        let mut app = game();
        app.update();
        *app.world_mut().resource_mut::<Spec>() = Spec::Clubman;
        press(&mut app, KeyCode::KeyC);
        assert_eq!(app.world().resource::<Menu>().at, Spec::Clubman.at());

        for _ in 0..5 {
            press(&mut app, KeyCode::ArrowUp);
        }
        assert_eq!(app.world().resource::<Menu>().at, 0);
        for _ in 0..5 {
            press(&mut app, KeyCode::ArrowDown);
        }
        assert_eq!(app.world().resource::<Menu>().at, Spec::ALL.len() - 1);
    }

    /// Taking a different car gives up the lap in progress, because half a lap
    /// in one car and half in another is not a lap in either. Taking the one
    /// already being driven gives up nothing.
    #[test]
    fn a_different_car_gives_up_the_lap_and_the_same_one_does_not() {
        let mut app = game();
        app.update();
        press(&mut app, KeyCode::KeyC);
        press(&mut app, KeyCode::ArrowDown);
        press(&mut app, KeyCode::Enter);
        assert_eq!(*app.world().resource::<Spec>(), Spec::ALL[1]);
        assert_eq!(
            app.world().resource::<Messages<Reset>>().len(),
            1,
            "changing car kept the lap that was driven in the other one"
        );

        app.world_mut().resource_mut::<Messages<Reset>>().clear();
        press(&mut app, KeyCode::KeyC);
        press(&mut app, KeyCode::Enter);
        assert_eq!(*app.world().resource::<Spec>(), Spec::ALL[1]);
        assert_eq!(
            app.world().resource::<Messages<Reset>>().len(),
            0,
            "taking the car already being driven threw the lap away"
        );
    }

    /// The circuit page is the same menu with different rows in it, and it
    /// hands the choice on rather than acting on it: a circuit is built by the
    /// module that knows how, and what a switch costs is that module's to say.
    #[test]
    fn the_circuit_page_asks_for_the_circuit_under_the_cursor() {
        let mut app = game();
        app.update();
        press(&mut app, KeyCode::KeyT);
        assert_eq!(app.world().resource::<Menu>().page, Some(Page::Circuit));
        let driving = circuit_at(app.world().resource::<Track>().circuit());
        assert_eq!(
            app.world().resource::<Menu>().at,
            driving,
            "the cursor did not open on the circuit being driven"
        );

        press(&mut app, KeyCode::ArrowDown);
        press(&mut app, KeyCode::Enter);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Nothing);
        let asked: Vec<&str> = app
            .world()
            .resource::<Messages<GoTo>>()
            .iter_current_update_messages()
            .map(|GoTo(circuit)| circuit.id)
            .collect();
        let next = (driving + 2).min(all_circuits().len() - 1);
        assert_eq!(asked, vec![all_circuits()[next].id]);
        assert_eq!(
            app.world().resource::<Messages<Reset>>().len(),
            0,
            "the menu threw the lap away itself instead of leaving it to the track"
        );
    }

    /// Two pages, two keys, and each opens its own.
    #[test]
    fn each_key_opens_its_own_page() {
        let mut app = game();
        app.update();
        for (key, page) in [(KeyCode::KeyC, Page::Car), (KeyCode::KeyT, Page::Circuit)] {
            press(&mut app, key);
            assert_eq!(app.world().resource::<Menu>().page, Some(page));
            press(&mut app, KeyCode::Escape);
            assert!(app.world().resource::<Menu>().page.is_none());
        }
    }

    /// Backing out changes nothing, which is the whole of what `Esc` promises.
    #[test]
    fn backing_out_of_a_menu_changes_nothing() {
        let mut app = game();
        app.update();
        press(&mut app, KeyCode::KeyC);
        press(&mut app, KeyCode::ArrowDown);
        press(&mut app, KeyCode::Escape);
        assert_eq!(*app.world().resource::<Spec>(), Spec::default());
        assert_eq!(app.world().resource::<Messages<Reset>>().len(), 0);
    }

    /// A menu is opened from a running game, not from over the top of a pause:
    /// two things in front of the game at once would give `Enter` two jobs.
    #[test]
    fn a_menu_does_not_open_over_a_pause() {
        let mut app = game();
        app.update();
        press(&mut app, KeyCode::Escape);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Pause);
        press(&mut app, KeyCode::KeyC);
        assert!(app.world().resource::<Menu>().page.is_none());
        assert_eq!(*app.world().resource::<Halt>(), Halt::Pause);
    }
    #[test]
    fn setup_is_a_draft_until_applied_and_cancel_discards_it() {
        let mut app = game();
        app.update();
        press(&mut app, KeyCode::KeyC);
        press(&mut app, KeyCode::Digit3);
        assert_eq!(app.world().resource::<Menu>().setup, Setup::Oversteer);
        assert_eq!(*app.world().resource::<Setup>(), Setup::Balanced);
        press(&mut app, KeyCode::Escape);
        press(&mut app, KeyCode::KeyC);
        assert_eq!(app.world().resource::<Menu>().setup, Setup::Balanced);
        press(&mut app, KeyCode::ArrowRight);
        press(&mut app, KeyCode::Enter);
        assert_eq!(*app.world().resource::<Setup>(), Setup::Oversteer);
        assert_eq!(app.world().resource::<Messages<Reset>>().len(), 1);
    }

    #[test]
    fn mode_is_a_draft_and_applying_it_restarts_the_lap() {
        #[derive(Resource, Default)]
        struct ModeChanges(usize);
        let mut app = game();
        app.init_resource::<ModeChanges>().add_systems(
            Update,
            |mode: Res<Mode>, mut changes: ResMut<ModeChanges>| {
                if mode.is_changed() {
                    changes.0 += 1;
                }
            },
        );
        app.update();
        press(&mut app, KeyCode::KeyC);
        press(&mut app, KeyCode::KeyM);
        assert_eq!(app.world().resource::<Menu>().mode, Mode::Pro);
        assert_eq!(*app.world().resource::<Mode>(), Mode::Regular);
        press(&mut app, KeyCode::Escape);
        press(&mut app, KeyCode::KeyC);
        assert_eq!(app.world().resource::<Menu>().mode, Mode::Regular);
        press(&mut app, KeyCode::KeyM);
        press(&mut app, KeyCode::KeyM);
        assert_eq!(app.world().resource::<Menu>().mode, Mode::Beginner);
        press(&mut app, KeyCode::Enter);
        assert_eq!(*app.world().resource::<Mode>(), Mode::Beginner);
        assert_eq!(app.world().resource::<Messages<Reset>>().len(), 1);
        press(&mut app, KeyCode::KeyC);
        assert_eq!(app.world().resource::<Menu>().mode, Mode::Beginner);
        let mode_changes = app.world().resource::<ModeChanges>().0;
        press(&mut app, KeyCode::Digit3);
        press(&mut app, KeyCode::Enter);
        assert_eq!(
            app.world().resource::<ModeChanges>().0,
            mode_changes,
            "changing handling should not reload the mode's records"
        );
    }

    #[test]
    fn xbox_y_cycles_modes_and_a_applies() {
        let mut app = game();
        let pad = app.world_mut().spawn(Gamepad::default()).id();
        app.update();
        pad_press(&mut app, pad, GamepadButton::LeftTrigger);
        pad_press(&mut app, pad, GamepadButton::North);
        assert_eq!(app.world().resource::<Menu>().mode, Mode::Pro);
        assert_eq!(*app.world().resource::<Mode>(), Mode::Regular);
        pad_press(&mut app, pad, GamepadButton::South);
        assert_eq!(*app.world().resource::<Mode>(), Mode::Pro);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Nothing);
    }

    #[test]
    fn shoulder_buttons_switch_menu_tabs_without_resetting_or_applying() {
        let mut app = game();
        app.add_plugins(crate::input::InputPlugin);
        let controller = app.world_mut().spawn(Gamepad::default()).id();
        app.update();
        pad_press(&mut app, controller, GamepadButton::LeftTrigger);
        assert_eq!(app.world().resource::<Menu>().page, Some(Page::Car));
        for (button, page) in [
            (GamepadButton::RightTrigger, Page::Circuit),
            (GamepadButton::RightTrigger, Page::Car),
            (GamepadButton::LeftTrigger, Page::Circuit),
            (GamepadButton::LeftTrigger, Page::Car),
        ] {
            pad_press(&mut app, controller, button);
            assert_eq!(app.world().resource::<Menu>().page, Some(page));
            assert_eq!(*app.world().resource::<Halt>(), Halt::Menu);
            assert_eq!(*app.world().resource::<Spec>(), Spec::Tourer);
            assert_eq!(*app.world().resource::<Mode>(), Mode::Regular);
            assert!(app.world().resource::<Messages<Reset>>().is_empty());
            assert!(app.world().resource::<Messages<GoTo>>().is_empty());
        }
        pad_press(&mut app, controller, GamepadButton::Start);
        pad_press(&mut app, controller, GamepadButton::RightTrigger);
        assert_eq!(app.world().resource::<Messages<Reset>>().len(), 1);
    }

    #[test]
    fn search_accepts_accents_and_never_applies_an_empty_result() {
        assert_eq!(searchable("Nürburgring"), searchable("NurburgRing"));
        assert_eq!(searchable("São José"), "sao jose");
        let mut app = game();
        app.update();
        press(&mut app, KeyCode::KeyT);
        {
            let mut menu = app.world_mut().resource_mut::<Menu>();
            menu.search = "no such circuit".into();
            menu.filter();
        }
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.world().resource::<Menu>().page, Some(Page::Circuit));
        assert_eq!(app.world().resource::<Messages<GoTo>>().len(), 0);
        {
            let mut menu = app.world_mut().resource_mut::<Menu>();
            menu.search = "spa".into();
            menu.filter();
        }
        press(&mut app, KeyCode::Enter);
        assert!(app.world().resource::<Menu>().page.is_none());
        assert_eq!(app.world().resource::<Messages<GoTo>>().len(), 1);
    }

    #[test]
    fn paging_reaches_the_last_circuit_and_respects_filtered_results() {
        let mut menu = Menu {
            page: Some(Page::Circuit),
            ..default()
        };
        for _ in 0..all_circuits().len() {
            menu.move_by(PAGE_SIZE as i32);
        }
        assert_eq!(menu.at, all_circuits().len() - 1);
        menu.search = "spa".into();
        menu.filter();
        let first = menu.at;
        menu.move_by(PAGE_SIZE as i32);
        assert_eq!(menu.at, first);
        assert!(all_circuits()[first].name.to_lowercase().contains("spa"));
    }

    #[test]
    fn mouse_can_select_a_car_and_apply_its_setup() {
        let mut app = game();
        app.update();
        press(&mut app, KeyCode::KeyC);
        let button = app
            .world_mut()
            .spawn((Interaction::Pressed, Action::Select(2)))
            .id();
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        assert_eq!(app.world().resource::<Menu>().at, 2);
        assert_eq!(*app.world().resource::<Spec>(), Spec::Tourer);
        app.world_mut().despawn(button);
        let mode_button = app
            .world_mut()
            .spawn((Interaction::Pressed, Action::Mode(Mode::Beginner)))
            .id();
        app.update();
        assert_eq!(app.world().resource::<Menu>().mode, Mode::Beginner);
        assert_eq!(*app.world().resource::<Mode>(), Mode::Regular);
        app.world_mut().despawn(mode_button);
        app.world_mut().spawn((Interaction::Pressed, Action::Apply));
        app.update();
        assert_eq!(*app.world().resource::<Spec>(), Spec::Express);
        assert_eq!(*app.world().resource::<Mode>(), Mode::Beginner);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Nothing);
    }

    #[test]
    fn opening_a_menu_holds_the_clock_in_the_same_frame() {
        let mut app = game();
        app.update();
        press(&mut app, KeyCode::KeyT);
        assert!(app.world().resource::<Time<Virtual>>().is_paused());
        press(&mut app, KeyCode::Escape);
        assert!(!app.world().resource::<Time<Virtual>>().is_paused());
    }
    fn pad_press(app: &mut App, controller: Entity, button: GamepadButton) {
        let mut pad = app.world_mut().get_mut::<Gamepad>(controller).unwrap();
        pad.digital_mut().release_all();
        pad.digital_mut().press(button);
        app.update();
        app.world_mut()
            .get_mut::<Gamepad>(controller)
            .unwrap()
            .digital_mut()
            .clear();
    }

    #[test]
    fn dpad_and_arrow_navigation_match_in_both_menus_including_held_repeat() {
        for open in [KeyCode::KeyC, KeyCode::KeyT] {
            let mut keyboard = game();
            let mut gamepad = game();
            let controller = gamepad.world_mut().spawn(Gamepad::default()).id();
            for app in [&mut keyboard, &mut gamepad] {
                app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                    std::time::Duration::from_millis(100),
                ));
                press(app, open);
                app.world_mut()
                    .resource_mut::<ButtonInput<KeyCode>>()
                    .reset_all();
            }
            for (key, button) in [
                (KeyCode::ArrowDown, GamepadButton::DPadDown),
                (KeyCode::ArrowRight, GamepadButton::DPadRight),
                (KeyCode::ArrowLeft, GamepadButton::DPadLeft),
                (KeyCode::ArrowUp, GamepadButton::DPadUp),
            ] {
                press(&mut keyboard, key);
                pad_press(&mut gamepad, controller, button);
                for _ in 0..7 {
                    let a = keyboard.world().resource::<Menu>();
                    let b = gamepad.world().resource::<Menu>();
                    assert_eq!((a.at, a.setup), (b.at, b.setup));
                    keyboard.update();
                    gamepad.update();
                }
            }
        }
    }

    #[test]
    fn xbox_x_is_not_garage_and_start_pauses_resumes_and_backs_out() {
        let mut app = game();
        let controller = app.world_mut().spawn(Gamepad::default()).id();
        app.update();
        pad_press(&mut app, controller, GamepadButton::West);
        assert!(app.world().resource::<Menu>().page.is_none());
        pad_press(&mut app, controller, GamepadButton::Start);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Pause);
        pad_press(&mut app, controller, GamepadButton::Start);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Nothing);
        pad_press(&mut app, controller, GamepadButton::LeftTrigger);
        assert_eq!(app.world().resource::<Menu>().page, Some(Page::Car));
        pad_press(&mut app, controller, GamepadButton::Start);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Nothing);
        assert!(app.world().resource::<Menu>().page.is_none());
        pad_press(&mut app, controller, GamepadButton::Start);
        pad_press(&mut app, controller, GamepadButton::South);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Nothing);
    }

    #[test]
    fn xbox_stick_selects_car_and_setup_and_a_applies_them() {
        let mut app = game();
        let controller = app.world_mut().spawn(Gamepad::default()).id();
        app.update();
        pad_press(&mut app, controller, GamepadButton::LeftTrigger);
        app.world_mut()
            .get_mut::<Gamepad>(controller)
            .unwrap()
            .analog_mut()
            .set(GamepadAxis::LeftStickY, -1.0);
        app.update();
        assert_eq!(app.world().resource::<Menu>().at, 1);
        {
            let mut pad = app.world_mut().get_mut::<Gamepad>(controller).unwrap();
            pad.analog_mut().set(GamepadAxis::LeftStickY, 0.0);
            pad.analog_mut().set(GamepadAxis::LeftStickX, 1.0);
        }
        app.update();
        assert_eq!(app.world().resource::<Menu>().setup, Setup::Oversteer);
        pad_press(&mut app, controller, GamepadButton::South);
        assert_eq!(*app.world().resource::<Spec>(), Spec::Clubman);
        assert_eq!(*app.world().resource::<Setup>(), Setup::Oversteer);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Nothing);
    }

    #[test]
    fn menu_stick_ignores_drift_and_repeats_at_a_readable_pace() {
        let mut stick = Navigation::default();
        assert_eq!(stick.step(Vec2::splat(0.2), 0.0), IVec2::ZERO);
        assert_eq!(stick.step(Vec2::Y, 0.1), IVec2::NEG_Y);
        assert_eq!(stick.step(Vec2::Y, 0.2), IVec2::ZERO);
        assert_eq!(stick.step(Vec2::Y, 0.46), IVec2::NEG_Y);
        assert_eq!(stick.step(Vec2::Y, 0.50), IVec2::ZERO);
        assert_eq!(stick.step(Vec2::Y, 0.59), IVec2::NEG_Y);
        assert_eq!(stick.step(Vec2::ZERO, 0.6), IVec2::ZERO);
        assert_eq!(stick.step(Vec2::NEG_X, 0.61), IVec2::NEG_X);
    }
}
