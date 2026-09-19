//! Stopping the world, and letting it go again.
//!
//! `Esc` stops the game where it stands and `Enter` starts it moving again.
//! There is no second, slower simulation running behind the pause and no state
//! machine deciding which systems are allowed to think this frame. The whole
//! game is driven by Bevy's virtual clock: driving steps `Time<Fixed>`, which is
//! filled from that clock, and the lap clock counts those same fixed steps. Stop
//! the virtual clock and both stop, together, because neither of them is given
//! anything. Nothing has to be told twice and nothing can drift out of step with
//! anything else, which is the whole reason it is done here rather than with a
//! flag on every system that moves.
//!
//! What a pause cannot stop is the driver's hands. A pedal held down when the
//! game stopped would still be held down when it started again — and worse,
//! whatever was pressed *during* the pause would arrive all at once. So the
//! controls are let go of on the way in and the keys are not read again until
//! the way out. The reset key, the setup slider and the ghost go quiet with
//! them: while the game is stopped, nothing the driver does reaches the car.
//!
//! [`Halt`] is what says so, and it says it for everyone. It is an enum rather
//! than a flag because a pause is not the only thing that will ever stand in
//! front of the game.

use bevy::prelude::*;

use crate::car::{Controls, Player};
use crate::hud::{AMBER, AMBER_DIM, FRONT};

/// What is standing in front of the game, if anything.
#[derive(Resource, Clone, Copy, Default, PartialEq, Eq, Debug)]
pub(crate) enum Halt {
    /// The game is being played.
    #[default]
    Nothing,
    /// `Esc`. `Enter` lets the game go again.
    Pause,
    /// A menu is up over the game. It owns both keys while it is, and closes
    /// itself — see [`crate::menu`].
    Menu,
}

impl Halt {
    /// Whether the world is stopped. Everything that only makes sense while the
    /// car can move asks this rather than naming a variant, so a second reason
    /// to stop costs nothing anywhere else.
    pub(crate) fn stopped(self) -> bool {
        self != Halt::Nothing
    }
}

/// Reading the pause keys runs in here, ahead of everything that goes quiet
/// while the game is stopped — the driver's own input first among them, so the
/// frame the game starts again is a frame the keys are read on.
#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct HaltSet;

/// Whether the game is running. The run condition everything that only makes
/// sense with the car moving hangs off.
pub(crate) fn running(halt: Res<Halt>) -> bool {
    !halt.stopped()
}

pub struct PausePlugin;

impl Plugin for PausePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Halt>()
            .add_systems(Startup, setup)
            .add_systems(
                PreUpdate,
                (
                    watch.in_set(HaltSet).after(bevy::input::InputSystems),
                    hold_the_clock.after(HaltSet).after(crate::menu::MenuSet),
                ),
            )
            .add_systems(Update, show);
    }
}

/// The banner, which is the only part of a pause there is to see.
#[derive(Component)]
struct Banner;

#[derive(Component)]
struct Resume;

fn setup(mut commands: Commands) {
    use crate::ui::{LINE, TEXT, label};
    commands
        .spawn((
            Banner,
            GlobalZIndex(15),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.018, 0.026, 0.032, 0.82)),
            Visibility::Hidden,
        ))
        .with_children(|overlay| {
            overlay
                .spawn((
                    Node {
                        width: px(440),
                        padding: UiRect::all(px(32)),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(16)),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(16),
                        ..default()
                    },
                    BackgroundColor(FRONT),
                    BorderColor::all(LINE),
                ))
                .with_children(|panel| {
                    panel.spawn(label("TODORA  /  SESSION PAUSED", 12.0, AMBER));
                    panel.spawn(label("Take a breather.", 36.0, TEXT));
                    panel.spawn(label("Your lap will be right here.", 17.0, AMBER_DIM));
                    panel
                        .spawn((
                            Button,
                            Resume,
                            Node {
                                padding: UiRect::all(px(14)),
                                margin: UiRect::top(px(8)),
                                border_radius: BorderRadius::all(px(8)),
                                justify_content: JustifyContent::Center,
                                ..default()
                            },
                            BackgroundColor(AMBER),
                        ))
                        .with_children(|button| {
                            button.spawn(label("Resume  /  A · Start · Enter", 17.0, FRONT));
                        });
                    for which in [
                        crate::sound::SoundToggle::Music,
                        crate::sound::SoundToggle::Effects,
                    ] {
                        panel
                            .spawn((
                                Button,
                                which,
                                Node {
                                    padding: UiRect::all(px(12)),
                                    border: UiRect::all(px(1)),
                                    border_radius: BorderRadius::all(px(8)),
                                    justify_content: JustifyContent::Center,
                                    ..default()
                                },
                                BorderColor::all(LINE),
                            ))
                            .with_children(|button| {
                                button.spawn(label("", 17.0, TEXT));
                            });
                    }
                    for (scope, title) in [
                        (
                            crate::ghost::clear::ResetGhosts::Current,
                            "Reset this ghost",
                        ),
                        (crate::ghost::clear::ResetGhosts::All, "Reset all ghosts"),
                    ] {
                        panel
                            .spawn((
                                Button,
                                scope,
                                Node {
                                    padding: UiRect::all(px(12)),
                                    border: UiRect::all(px(1)),
                                    border_radius: BorderRadius::all(px(8)),
                                    justify_content: JustifyContent::Center,
                                    ..default()
                                },
                                BorderColor::all(LINE),
                            ))
                            .with_children(|button| {
                                button.spawn(label(title, 17.0, TEXT));
                            });
                    }
                    panel.spawn((
                        crate::ghost::clear::Notice,
                        label(
                            "Reset removes saved best times and restarts the lap.",
                            14.0,
                            AMBER_DIM,
                        ),
                    ));
                    panel
                        .spawn((
                            Button,
                            crate::Quit,
                            Node {
                                padding: UiRect::all(px(12)),
                                justify_content: JustifyContent::Center,
                                ..default()
                            },
                        ))
                        .with_children(|button| {
                            button.spawn(label("Quit game", 17.0, AMBER_DIM));
                        });
                });
        });
}

/// `Esc` stops a running game, `Enter` lets a paused one go. Not one key
/// toggling: two keys, each with one job, is the rule that still works when
/// something else is standing in front of the game. Both transitions are
/// written from a named state rather than from any state, so while a menu is up
/// neither of them fires and both keys are the menu's — which is the whole of
/// the arrangement between the two modules, stated here and again there.
///
/// Letting go of the controls belongs here rather than to whatever stopped the
/// game, because it is true of every way of stopping it.
fn watch(
    keys: Res<ButtonInput<KeyCode>>,
    mut halt: ResMut<Halt>,
    pads: Query<&Gamepad>,
    mut players: Query<&mut Controls, With<Player>>,
    buttons: Query<&Interaction, (With<Resume>, Changed<Interaction>)>,
) {
    let resume_clicked = buttons.iter().any(|i| *i == Interaction::Pressed);
    let start = pads
        .iter()
        .any(|pad| pad.just_pressed(GamepadButton::Start));
    let confirm = pads
        .iter()
        .any(|pad| pad.just_pressed(GamepadButton::South));
    let wanted = match *halt {
        Halt::Nothing if keys.just_pressed(KeyCode::Escape) || start => Halt::Pause,
        Halt::Pause if keys.just_pressed(KeyCode::Enter) || start || confirm || resume_clicked => {
            Halt::Nothing
        }
        _ => return,
    };
    *halt = wanted;
    if halt.stopped() {
        for mut controls in &mut players {
            *controls = Controls::default();
        }
    }
}

/// Hold the clock the game is driven from, or let it run.
///
/// `Time<Virtual>` is what fills `Time<Fixed>`, so a paused virtual clock is a
/// `FixedUpdate` that never comes round: the physics does not step and the lap
/// clock, which counts those steps, does not tick. Everything in `Update` reads
/// a zero delta and sits still — the chase camera, the wheels, the body on its
/// springs. One clock, so there is nothing to keep in agreement.
fn hold_the_clock(
    halt: Res<Halt>,
    mut time: ResMut<Time<Virtual>>,
    mut players: Query<&mut Controls, With<Player>>,
) {
    if halt.stopped() {
        for mut controls in &mut players {
            *controls = Controls::default();
        }
    }
    if halt.stopped() != time.is_paused() {
        if halt.stopped() {
            time.pause();
        } else {
            time.unpause();
        }
    }
}

/// The banner is the pause's own, not every halt's. A menu is already standing
/// in front of the game and saying so; a second panel behind it saying the game
/// is stopped is the same news twice, through each other.
fn show(halt: Res<Halt>, mut banner: Query<&mut Visibility, With<Banner>>) {
    if !halt.is_changed() {
        return;
    }
    if let Ok(mut visibility) = banner.single_mut() {
        *visibility = if *halt == Halt::Pause {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    use crate::Reset;
    use crate::car::{Car, CarPlugin, Handling};
    use crate::lap::{LapPlugin, LapTimer};
    use crate::track::Track;

    /// The production schedules, without the rendered model.
    fn game() -> App {
        let track = Track::any();
        let start = track.start_transform();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<Reset>()
            .insert_resource(track)
            .init_resource::<ButtonInput<KeyCode>>()
            // What the body panels are repainted through, without a renderer.
            .init_resource::<Assets<StandardMaterial>>()
            .add_plugins((PausePlugin, crate::input::InputPlugin, CarPlugin, LapPlugin));
        app.world_mut().resource_mut::<Schedules>().remove(Startup);
        app.world_mut().spawn((
            Player,
            Car::default(),
            Controls::default(),
            Handling::SHOOTING_BRAKE,
            start,
        ));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
            10,
        )));
        app
    }

    /// Nothing here runs Bevy's own input plugin, which is what would normally
    /// turn the "just pressed" keys over at the end of a frame. Without it a
    /// press is just-pressed for ever, so the frame boundary is drawn by hand:
    /// one press is one press, and a held key stays held.
    fn run(app: &mut App, frames: usize) {
        for _ in 0..frames {
            app.update();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .clear();
        }
    }

    fn where_is_the_car(app: &mut App) -> Vec3 {
        let mut cars = app.world_mut().query_filtered::<&Transform, With<Player>>();
        cars.single(app.world())
            .expect("the car is still there")
            .translation
    }

    fn what_the_car_is_asked_for(app: &mut App) -> Controls {
        let mut cars = app.world_mut().query_filtered::<&Controls, With<Player>>();
        *cars.single(app.world()).expect("the car is still there")
    }

    /// The property the whole design rests on: one clock drives the car and the
    /// lap timer, so stopping it stops both, and neither of them creeps while
    /// the other waits. Driven far enough to be past the line with the clock
    /// actually running, because a clock that has not started proves nothing.
    #[test]
    fn esc_stops_the_car_and_the_clock_together_and_enter_starts_them_again() {
        let mut app = game();
        // Stop at the crossing, while the car is still moving. Its timing
        // depends on the current grid placement and circuit geometry.
        for _ in 0..400 {
            run(&mut app, 1);
            if app.world().resource::<LapTimer>().running() {
                break;
            }
        }
        assert!(
            app.world().resource::<LapTimer>().running(),
            "the car never reached the line, so there is no clock to stop"
        );
        let rolling = where_is_the_car(&mut app);
        assert!(rolling.distance(Track::any().start_transform().translation) > 40.0);

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        run(&mut app, 1);
        let clock = app.world().resource::<LapTimer>().current;
        let stopped_at = where_is_the_car(&mut app);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Pause);

        run(&mut app, 100);
        assert_eq!(
            where_is_the_car(&mut app),
            stopped_at,
            "the car drove on through the pause"
        );
        assert_eq!(
            app.world().resource::<LapTimer>().current,
            clock,
            "the lap clock ran through the pause"
        );

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        run(&mut app, 60);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Nothing);
        assert!(
            where_is_the_car(&mut app).distance(stopped_at) > 1.0,
            "the car did not start again"
        );
        assert!(
            app.world().resource::<LapTimer>().current > clock,
            "the lap clock did not start again"
        );
    }

    /// A pedal held down when the game stopped is not still held down when it
    /// starts again, and nothing pressed in between arrives late. Otherwise a
    /// pause would be somewhere to hold the throttle for free.
    #[test]
    fn the_controls_are_let_go_of_on_the_way_in() {
        let mut app = game();
        run(&mut app, 20);
        assert_eq!(
            what_the_car_is_asked_for(&mut app).throttle,
            1.0,
            "the test is not driving"
        );

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        run(&mut app, 10);
        assert_eq!(
            what_the_car_is_asked_for(&mut app),
            Controls::default(),
            "the car was still being asked for something while stopped"
        );
    }

    /// While the game is stopped nothing the driver presses reaches the car —
    /// including the keys that are not the pedals. The reset key is the one that
    /// would show: it moves the car, and a car that moved while paused is a car
    /// that was never really stopped.
    #[test]
    fn a_stopped_game_does_not_hear_the_reset_key() {
        let mut app = game();
        run(&mut app, 300);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        run(&mut app, 1);
        let stopped_at = where_is_the_car(&mut app);

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyR);
        run(&mut app, 20);
        assert_eq!(
            where_is_the_car(&mut app),
            stopped_at,
            "the reset key put the car back while the game was stopped"
        );

        // And the very same press is heard the moment the game is let go: the
        // pause reads the keys before the driver does, so the frame that starts
        // the game is a frame the driver is listened to on.
        let grid = Track::any().start_transform().translation;
        assert!(
            stopped_at.distance(grid) > 10.0,
            "the car never left the grid"
        );
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(KeyCode::KeyR);
        keys.press(KeyCode::Enter);
        keys.press(KeyCode::KeyR);
        run(&mut app, 1);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Nothing);
        assert!(
            where_is_the_car(&mut app).distance(grid) < 0.01,
            "the reset key was not heard once the game started again"
        );
    }

    /// Only one thing stands in front of the game at a time. The banner belongs
    /// to the pause, not to every halt, or a menu would have it showing through
    /// itself saying what the menu is already there to say.
    #[test]
    fn the_banner_is_the_pauses_own() {
        let mut app = game();
        app.world_mut().spawn((Banner, Visibility::Hidden));
        app.update();
        let shown = |app: &mut App| {
            let mut banners = app
                .world_mut()
                .query_filtered::<&Visibility, With<Banner>>();
            *banners.single(app.world()).expect("the banner is there") == Visibility::Visible
        };
        assert!(!shown(&mut app));
        for (halt, want) in [
            (Halt::Pause, true),
            (Halt::Menu, false),
            (Halt::Nothing, false),
        ] {
            *app.world_mut().resource_mut::<Halt>() = halt;
            app.update();
            assert_eq!(shown(&mut app), want, "the banner under {halt:?}");
        }
    }

    /// The virtual clock is the pause, so it has to follow [`Halt`] exactly —
    /// including back, or the game would start again with the world still held.
    #[test]
    fn the_clock_follows_the_halt() {
        let mut app = game();
        app.update();
        assert!(!app.world().resource::<Time<Virtual>>().is_paused());
        *app.world_mut().resource_mut::<Halt>() = Halt::Pause;
        app.update();
        assert!(app.world().resource::<Time<Virtual>>().is_paused());
        *app.world_mut().resource_mut::<Halt>() = Halt::Nothing;
        app.update();
        assert!(!app.world().resource::<Time<Virtual>>().is_paused());
    }
}
