//! Three cards for a first drive, and one hint if the laps keep not counting.
//!
//! On a first launch the cards sit over the grid while the car is held, and the
//! countdown waits for them: how to drive, what makes a lap, and the ghost. A
//! or Enter moves on, B or Esc skips the lot. They show the keys or the pad
//! buttons, whichever was touched last. Once closed they never come back by
//! themselves; **How to drive** in the pause menu shows them again.
//!
//! If three laps in a row do not count, a line suggests Beginner mode, once.

use bevy::prelude::*;

use crate::car::Mode;
use crate::hud::{AMBER, AMBER_DIM, FRONT};
use crate::input::LastDevice;
use crate::lap::{LapFinished, LapSet};
use crate::pause::{Halt, HaltSet};
use crate::settings::Settings;
use crate::ui::{LINE, TEXT, label};

const CARDS: usize = 3;
/// Laps in a row that did not count before the Beginner hint.
const MISSES: u32 = 3;
const HINT_FOR: f32 = 7.0;

/// Which card is up, if any.
#[derive(Resource, Default)]
pub(crate) struct Guide {
    card: Option<usize>,
    /// Laps in a row that did not count, and seconds the hint has left.
    misses: u32,
    hint_left: f32,
}

impl Guide {
    /// Put a card up, for a visual check.
    #[cfg(feature = "visual-check")]
    pub(crate) fn show(&mut self, at: usize) {
        if self.card != Some(at.min(CARDS - 1)) {
            self.card = Some(at.min(CARDS - 1));
        }
    }

    /// Whether the cards are up, holding the countdown.
    pub(crate) fn open(&self) -> bool {
        self.card.is_some()
    }
}

/// The title and body of a card, for the device in hand.
pub(crate) fn card(at: usize, device: LastDevice) -> (&'static str, &'static str) {
    use crate::text::t;
    let pad = device == LastDevice::Pad;
    match at {
        0 => (
            t("guide.drive"),
            t(if pad {
                "guide.drive_pad"
            } else {
                "guide.drive_keys"
            }),
        ),
        1 => (t("guide.lap"), t("guide.lap_body")),
        _ => (
            t("guide.ghost"),
            t(if pad {
                "guide.ghost_pad"
            } else {
                "guide.ghost_keys"
            }),
        ),
    }
}

fn footer(at: usize, device: LastDevice) -> String {
    use crate::text::{t, tf};
    let (next, skip) = match device {
        LastDevice::Pad => ("A", "B"),
        LastDevice::Keyboard => ("Enter", "Esc"),
    };
    let next_word = t(if at + 1 == CARDS {
        "guide.go"
    } else {
        "guide.next"
    });
    tf(
        "guide.footer",
        &[&(at + 1), &CARDS, &next, &next_word, &skip],
    )
}

pub struct OnboardingPlugin;

impl Plugin for OnboardingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Guide>()
            .add_systems(Startup, setup)
            .add_systems(PostStartup, first_launch)
            .add_systems(PreUpdate, drive.after(HaltSet))
            .add_systems(FixedUpdate, count_misses.after(LapSet))
            .add_systems(Update, draw);
    }
}

fn first_launch(
    settings: Option<Res<Settings>>,
    read_only: Option<Res<crate::settings::ReadOnly>>,
    mut guide: ResMut<Guide>,
) {
    if read_only.is_none() && settings.is_some_and(|s| !s.onboarding_done) {
        guide.card = Some(0);
    }
}

#[derive(Component)]
struct Panel;
#[derive(Component)]
struct Title;
#[derive(Component)]
struct Body;
#[derive(Component)]
struct Footer;
#[derive(Component)]
struct Hint;

fn setup(mut commands: Commands) {
    commands
        .spawn((
            Panel,
            GlobalZIndex(14),
            Node {
                position_type: PositionType::Absolute,
                top: px(250),
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
                    width: px(640),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(12),
                    padding: UiRect::all(px(26)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(16)),
                    ..default()
                },
                BackgroundColor(FRONT),
                BorderColor::all(LINE),
            ))
            .with_children(|card| {
                card.spawn((Title, label("", 26.0, AMBER)));
                card.spawn((Body, label("", 18.0, TEXT)));
                card.spawn((Footer, label("", 14.0, AMBER_DIM)));
            });
        });
    commands.spawn((
        Hint,
        crate::text::label("guide.beginner", 16.0, TEXT),
        Node {
            position_type: PositionType::Absolute,
            top: px(96),
            left: percent(50),
            margin: UiRect::left(px(-330)),
            width: px(660),
            padding: UiRect::axes(px(16), px(10)),
            border_radius: BorderRadius::all(px(10)),
            ..default()
        },
        BackgroundColor(FRONT),
        Visibility::Hidden,
    ));
}

/// Next, skip, and the pause menu's way in and back out.
fn drive(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut halt: ResMut<Halt>,
    mut guide: ResMut<Guide>,
    mut settings: Option<ResMut<Settings>>,
) {
    // Opened from the pause: the press that opened it is not also "next".
    if *halt == Halt::Guide && halt.is_changed() {
        guide.card = Some(0);
        return;
    }
    let Some(at) = guide.card else {
        return;
    };
    // The cards wait for the title and any page over the game to go.
    if !matches!(*halt, Halt::Nothing | Halt::Guide) || halt.is_changed() {
        return;
    }
    let pad = |button| pads.iter().any(|pad| pad.just_pressed(button));
    let skip = keys.just_pressed(KeyCode::Escape) || pad(GamepadButton::East);
    let next = keys.just_pressed(KeyCode::Enter) || pad(GamepadButton::South);
    if !skip && !next {
        return;
    }
    guide.card = (!skip && at + 1 < CARDS).then_some(at + 1);
    if guide.card.is_none() {
        if let Some(settings) = settings.as_mut() {
            settings.onboarding_done = true;
        }
        if *halt == Halt::Guide {
            *halt = Halt::Pause;
        }
    }
}

/// Three laps in a row that did not count bring the hint up, once ever.
fn count_misses(
    mut laps: MessageReader<LapFinished>,
    mode: Option<Res<Mode>>,
    mut settings: Option<ResMut<Settings>>,
    mut guide: ResMut<Guide>,
) {
    for lap in laps.read() {
        guide.misses = if lap.valid { 0 } else { guide.misses + 1 };
        let shown = settings.as_ref().is_none_or(|s| s.beginner_hint_shown);
        let beginner = mode.as_ref().is_some_and(|m| **m == Mode::Beginner);
        if guide.misses >= MISSES && !shown && !beginner {
            guide.hint_left = HINT_FOR;
            if let Some(settings) = settings.as_mut() {
                settings.beginner_hint_shown = true;
            }
        }
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw(
    time: Res<Time>,
    halt: Res<Halt>,
    device: Res<LastDevice>,
    mut guide: ResMut<Guide>,
    mut panels: Query<&mut Visibility, (With<Panel>, Without<Hint>)>,
    mut hints: Query<&mut Visibility, (With<Hint>, Without<Panel>)>,
    mut titles: Query<&mut Text, (With<Title>, Without<Body>, Without<Footer>)>,
    mut bodies: Query<&mut Text, (With<Body>, Without<Title>, Without<Footer>)>,
    mut footers: Query<&mut Text, (With<Footer>, Without<Title>, Without<Body>)>,
) {
    guide.hint_left = (guide.hint_left - time.delta_secs()).max(0.0);
    let hint = guide.hint_left > 0.0 && !halt.stopped();
    for mut visibility in &mut hints {
        visibility.set_if_neq(if hint {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
    let shown = guide
        .card
        .filter(|_| matches!(*halt, Halt::Nothing | Halt::Guide));
    for mut visibility in &mut panels {
        visibility.set_if_neq(if shown.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    let Some(at) = shown else {
        return;
    };
    if !guide.is_changed() && !device.is_changed() && !halt.is_changed() {
        return;
    }
    let (title, body) = card(at, *device);
    if let Ok(mut text) = titles.single_mut() {
        text.0 = title.into();
    }
    if let Ok(mut text) = bodies.single_mut() {
        text.0 = body.into();
    }
    if let Ok(mut text) = footers.single_mut() {
        text.0 = footer(at, *device);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(onboarded: bool) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<LapFinished>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Halt>()
            .init_resource::<LastDevice>()
            .insert_resource(Settings {
                onboarding_done: onboarded,
                ..Default::default()
            })
            .insert_resource(Mode::Regular)
            .init_resource::<Guide>()
            .add_systems(PostStartup, first_launch)
            .add_systems(Update, (drive, count_misses));
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
    fn the_cards_show_once_and_never_again() {
        let mut app = app(false);
        assert!(app.world().resource::<Guide>().open());
        for _ in 0..CARDS {
            tap(&mut app, KeyCode::Enter);
        }
        assert!(!app.world().resource::<Guide>().open());
        assert!(app.world().resource::<Settings>().onboarding_done);
        assert!(!self::app(true).world().resource::<Guide>().open());
    }

    #[test]
    fn skipping_closes_every_card_at_once() {
        let mut app = app(false);
        tap(&mut app, KeyCode::Escape);
        assert!(!app.world().resource::<Guide>().open());
        assert!(app.world().resource::<Settings>().onboarding_done);
    }

    #[test]
    fn the_pause_menu_shows_them_again_and_they_go_back_to_the_pause() {
        let mut app = app(true);
        *app.world_mut().resource_mut::<Halt>() = Halt::Guide;
        app.update();
        assert!(app.world().resource::<Guide>().open());
        tap(&mut app, KeyCode::Escape);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Pause);
    }

    #[test]
    fn the_cards_name_the_keys_or_the_buttons() {
        assert!(card(0, LastDevice::Keyboard).1.contains("W  accelerate"));
        assert!(card(0, LastDevice::Pad).1.contains("RT or A"));
        assert_eq!(
            footer(2, LastDevice::Pad),
            "3 / 3        A  drive      B  skip"
        );
    }

    #[test]
    fn three_laps_that_do_not_count_suggest_beginner_once() {
        let mut app = app(true);
        let miss = LapFinished {
            time: 60.0,
            best: false,
            valid: false,
        };
        for _ in 0..3 {
            app.world_mut().write_message(miss);
            app.update();
        }
        assert!(app.world().resource::<Guide>().hint_left > 0.0);
        assert!(app.world().resource::<Settings>().beginner_hint_shown);
        app.world_mut().resource_mut::<Guide>().hint_left = 0.0;
        for _ in 0..3 {
            app.world_mut().write_message(miss);
            app.update();
        }
        assert_eq!(
            app.world().resource::<Guide>().hint_left,
            0.0,
            "shown twice"
        );
    }
}
