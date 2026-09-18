//! Menus: a list of things, one of them under the cursor.
//!
//! A menu stands in front of the game rather than beside it. Opening one stops
//! the world through the same [`Halt`] the pause key uses, so there is no second
//! idea of "not driving now" to keep in agreement with the first — the car is
//! already still, the clock is already held, and the driver's keys are already
//! not being read, all for the reason they are during a pause. What the menu
//! adds is somewhere for those keys to go instead.
//!
//! The two modules own opposite ends of the same state and never reach into each
//! other's: [`crate::pause`] takes `Esc` only from a game that is running and
//! `Enter` only from a game that is paused, so while a menu is up both keys are
//! the menu's — `Enter` takes what is under the cursor and `Esc` backs out
//! having changed nothing.
//!
//! Choosing a car gives up the lap in progress, because half a lap in one car
//! and half in another is not a lap in either. It gives up nothing else: the
//! laps already driven, the best of them and the ghost belong to the circuit,
//! and the circuit has not moved.

use bevy::prelude::*;

use crate::Reset;
use crate::car::{Spec, Stars};
use crate::hud::{AMBER, AMBER_DIM, FRONT};
use crate::pause::{Halt, HaltSet};

/// Opening, moving and choosing all run in here, after the pause has had its
/// look at the keys and before anything that acts on what was chosen.
#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct MenuSet;

/// Which menu is up. One so far.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Page {
    /// `C`, or the pad's west button.
    Car,
}

impl Page {
    /// Every page, so the panel can be built once with all of them in it.
    const ALL: [Page; 1] = [Page::Car];

    /// What the panel is headed with.
    fn title(self) -> &'static str {
        match self {
            Page::Car => "CAR",
        }
    }

    /// The rows of this page: what each one is called, and what it is good at
    /// where that is worth showing.
    fn entries(self) -> Vec<(&'static str, Option<Stars>)> {
        match self {
            Page::Car => Spec::ALL
                .iter()
                .map(|spec| (spec.name(), Some(spec.sheet().stars)))
                .collect(),
        }
    }

    /// Which row is under the cursor when it opens: whatever is already chosen,
    /// so opening a menu and taking what it offers changes nothing.
    fn chosen(self, spec: &Spec) -> usize {
        match self {
            Page::Car => spec.at(),
        }
    }
}

/// The menu as it stands. `None` is no menu, which is most of the time.
#[derive(Resource, Default)]
pub(crate) struct Menu {
    page: Option<Page>,
    at: usize,
}

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Menu>()
            .add_systems(Startup, setup)
            .add_systems(PreUpdate, walk.in_set(MenuSet).after(HaltSet))
            .add_systems(Update, draw);
    }
}

/// Side of one of the five pips a rating is drawn with, and the gap between
/// them. Pips rather than text: the setup slider alongside is already drawn out
/// of little nodes, a rating is the same kind of reading, and five squares need
/// no glyph the font might not carry.
const PIP: f32 = 7.0;
const PIP_GAP: f32 = 3.0;
const OUT_OF: u8 = 5;
/// The cursor's gutter, the width a name is given, and one rating's own width.
const CURSOR: f32 = 12.0;
const NAME: f32 = 108.0;
const RATING: f32 = OUT_OF as f32 * (PIP + PIP_GAP) + 14.0;

/// A part of the panel that is shown or not shown.
///
/// Shown by `display` rather than by `Visibility`, because a hidden node still
/// takes its place in the layout and a hidden one does not: the panel is built
/// once with every page's rows in it, and it has to come out the size of the
/// page that is up rather than the size of all of them stacked.
#[derive(Component)]
enum Piece {
    /// The panel itself.
    Panel,
    /// The column headings, which mean something only on a rated page.
    Heading,
    /// One row of one page, and where it sits in that page.
    Row { page: Page, at: usize },
}

#[derive(Component)]
struct Title;

/// The name on a row: lit and stepped out of the gutter when it is the one under
/// the cursor.
#[derive(Component)]
struct RowName;

fn setup(mut commands: Commands) {
    commands
        .spawn((
            Piece::Panel,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                top: percent(26),
                left: percent(50),
                margin: UiRect::left(px(-(CURSOR * 2.0 + NAME + 3.0 * RATING) / 2.0)),
                padding: UiRect::axes(px(18), px(14)),
                border: UiRect::all(px(2)),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                ..default()
            },
            BackgroundColor(FRONT),
            BorderColor::all(AMBER_DIM),
        ))
        .with_children(|panel| {
            panel.spawn((
                Title,
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(20.0),
                    ..default()
                },
                TextColor(AMBER),
                Node {
                    margin: UiRect::bottom(px(6)),
                    ..default()
                },
            ));

            panel
                .spawn((
                    Piece::Heading,
                    Node {
                        flex_direction: FlexDirection::Row,
                        margin: UiRect::left(px(CURSOR * 2.0 + NAME)),
                        ..default()
                    },
                ))
                .with_children(|heading| {
                    for label in ["HANDLING", "ACCEL", "TOP"] {
                        heading.spawn((
                            Node {
                                width: px(RATING),
                                ..default()
                            },
                            children![(
                                Text::new(label),
                                TextFont {
                                    font_size: FontSize::Px(10.0),
                                    ..default()
                                },
                                TextColor(AMBER_DIM),
                            )],
                        ));
                    }
                });

            for page in Page::ALL {
                for (at, (name, stars)) in page.entries().into_iter().enumerate() {
                    panel
                        .spawn((
                            Piece::Row { page, at },
                            Node {
                                display: Display::None,
                                flex_direction: FlexDirection::Row,
                                align_items: AlignItems::Center,
                                padding: UiRect::axes(px(0), px(3)),
                                ..default()
                            },
                        ))
                        .with_children(|row| {
                            row.spawn((
                                RowName,
                                Text::new(name),
                                TextFont {
                                    font_size: FontSize::Px(16.0),
                                    ..default()
                                },
                                TextColor(AMBER_DIM),
                                Node {
                                    width: px(CURSOR + NAME),
                                    margin: UiRect::left(px(CURSOR)),
                                    ..default()
                                },
                            ));
                            for (_, out_of_five) in stars.into_iter().flat_map(Stars::rows) {
                                row.spawn((
                                    Node {
                                        width: px(RATING),
                                        flex_direction: FlexDirection::Row,
                                        align_items: AlignItems::Center,
                                        ..default()
                                    },
                                    Children::spawn(SpawnIter((0..OUT_OF).map(move |i| {
                                        (
                                            Node {
                                                width: px(PIP),
                                                height: px(PIP),
                                                margin: UiRect::right(px(PIP_GAP)),
                                                border: UiRect::all(px(1)),
                                                ..default()
                                            },
                                            BackgroundColor(if i < out_of_five {
                                                AMBER
                                            } else {
                                                Color::NONE
                                            }),
                                            BorderColor::all(AMBER_DIM),
                                        )
                                    }))),
                                ));
                            }
                        });
                }
            }

            panel.spawn((
                Text::new("UP / DOWN — MOVE     ENTER — TAKE     ESC — BACK"),
                TextFont {
                    font_size: FontSize::Px(11.0),
                    ..default()
                },
                TextColor(AMBER_DIM),
                Node {
                    margin: UiRect::top(px(8)),
                    ..default()
                },
            ));
        });
}

/// Open a menu, move down it, take what is under the cursor, or back out.
///
/// `Esc` and `Enter` are the menu's while one is up, which is why [`Halt`] is
/// only ever moved to and from [`Halt::Menu`] here: the pause key takes `Esc`
/// from a running game and gives `Enter` back to a paused one, and neither of
/// those transitions can fire while the halt says a menu is open.
fn walk(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut menu: ResMut<Menu>,
    mut halt: ResMut<Halt>,
    mut spec: ResMut<Spec>,
    mut reset: MessageWriter<Reset>,
) {
    let pressed = |key: KeyCode, button: GamepadButton| {
        keys.just_pressed(key) || pads.iter().any(|pad| pad.just_pressed(button))
    };
    let Some(page) = menu.page else {
        // A menu may only be opened from a running game. From a pause it would
        // be a second thing standing in front of the game, and `Enter` would
        // have two jobs at once.
        if *halt == Halt::Nothing && pressed(KeyCode::KeyC, GamepadButton::West) {
            menu.at = Page::Car.chosen(&spec);
            menu.page = Some(Page::Car);
            *halt = Halt::Menu;
        }
        return;
    };

    let step = i32::from(pressed(KeyCode::ArrowDown, GamepadButton::DPadDown))
        - i32::from(pressed(KeyCode::ArrowUp, GamepadButton::DPadUp));
    if step != 0 {
        let last = page.entries().len() as i32 - 1;
        menu.at = (menu.at as i32 + step).clamp(0, last) as usize;
    }

    if pressed(KeyCode::Enter, GamepadButton::South) {
        take(page, menu.at, &mut spec, &mut reset);
        close(&mut menu, &mut halt);
    } else if pressed(KeyCode::Escape, GamepadButton::East) {
        close(&mut menu, &mut halt);
    }
}

/// What the row under the cursor means.
fn take(page: Page, at: usize, spec: &mut Spec, reset: &mut MessageWriter<Reset>) {
    match page {
        Page::Car => {
            let Some(&wanted) = Spec::ALL.get(at) else {
                return;
            };
            if wanted != *spec {
                *spec = wanted;
                // The lap so far was driven in the other car.
                reset.write(Reset);
            }
        }
    }
}

fn close(menu: &mut Menu, halt: &mut Halt) {
    menu.page = None;
    *halt = Halt::Nothing;
}

/// Show the page that is up, with the cursor on its row.
fn draw(
    menu: Res<Menu>,
    mut pieces: Query<(&Piece, &Children, &mut Node)>,
    mut title: Query<&mut Text, With<Title>>,
    mut names: Query<(&mut TextColor, &mut Node), Without<Piece>>,
) {
    if !menu.is_changed() {
        return;
    }
    if let (Some(page), Ok(mut text)) = (menu.page, title.single_mut()) {
        text.0 = page.title().into();
    }
    for (piece, children, mut node) in &mut pieces {
        let shown = match (piece, menu.page) {
            (_, None) => false,
            (Piece::Panel | Piece::Heading, Some(_)) => true,
            (Piece::Row { page, .. }, Some(up)) => *page == up,
        };
        node.display = if shown { Display::Flex } else { Display::None };

        let Piece::Row { page, at } = piece else {
            continue;
        };
        let under = menu.page == Some(*page) && *at == menu.at;
        for child in children {
            if let Ok((mut color, mut name)) = names.get_mut(*child) {
                color.0 = if under { AMBER } else { AMBER_DIM };
                // The cursor is the name stepping out of its gutter, which needs
                // no glyph the font might not carry.
                name.margin.left = px(if under { CURSOR * 2.0 } else { CURSOR });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pause::PausePlugin;

    fn game() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<Reset>()
            .init_resource::<ButtonInput<KeyCode>>()
            // The garage's own resource, which `CarPlugin` owns in the game.
            .init_resource::<Spec>()
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
}
