//! Three red lights and a green one: the car waits on the grid until GO.
//!
//! The countdown sits in front of the run-up, not in place of it. The car is
//! held where [`crate::track::Track::start_transform`] put it, the lights come
//! on one a second, and at GO it is let go to drive the same 40.5 m up to the
//! line it always drove, where the clock starts exactly as before. So no lap is
//! timed differently, and no ghost saved before the countdown existed means
//! anything else now.
//!
//! Holding is not pausing. The world goes on — the camera settles, the sky
//! drifts, the ghost waits at the line — and only the player's car is kept out
//! of the engine, so it cannot creep, roll back down a sloping grid, or reverse
//! off a held brake. The pedals are still read: throttle held through the
//! lights reaches the first step after GO. The countdown runs on the virtual
//! clock, so a pause freezes it where it stands and resuming carries on.
//!
//! A new circuit, driving mode or car gets the whole countdown; a restart gets
//! a short one, because a driver pressing `R` wants to go again, not to wait.

use bevy::prelude::*;

use crate::Reset;
use crate::car::{CarResetSet, Mode, Player, Spec};
use crate::hud::FRONT;
use crate::lap::ClockSet;
use crate::track::Track;
use crate::ui::{LINE, TEXT, label};

/// Seconds from the countdown starting to GO, for the whole one: a moment of
/// READY, then a light a second.
const FULL: f32 = 3.8;
/// A restart: all three lights, then GO.
const SHORT: f32 = 0.6;
/// How long the green lights stay up once the car has gone.
const GREEN_FOR: f32 = 0.6;
/// Diameter of one light, in pixels before the UI scale.
const LIGHT: f32 = 46.0;
const RED: Color = Color::srgb(0.93, 0.17, 0.13);
const GREEN: Color = Color::srgb(0.30, 0.90, 0.36);
const DARK: Color = Color::srgb(0.10, 0.12, 0.13);

/// Held out of the engine until GO. See [`crate::car`]'s drive step.
#[derive(Component)]
pub(crate) struct Held;

/// A light has just come on: `go` is the green one. The sound plays it.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StartLight {
    pub go: bool,
}

/// Where the countdown is.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub(crate) struct Start {
    /// Seconds since this countdown began, on the virtual clock.
    pub(crate) elapsed: f32,
    /// When GO is.
    go_at: f32,
    /// How many of the four signals (three red, then green) have been given.
    given: u8,
}

impl Default for Start {
    /// The game opens on a full countdown.
    fn default() -> Self {
        Self::of(FULL)
    }
}

impl Start {
    fn of(go_at: f32) -> Self {
        Self {
            elapsed: 0.0,
            go_at,
            given: 0,
        }
    }

    /// No countdown at all: free at once, nothing shown, nothing heard.
    fn none() -> Self {
        Self {
            elapsed: 0.0,
            go_at: 0.0,
            given: 4,
        }
    }

    /// Waiting for something else to start the countdown — the other driver,
    /// in shared practice. Held, with nothing shown.
    fn waiting() -> Self {
        Self {
            elapsed: f32::NEG_INFINITY,
            go_at: FULL,
            given: 0,
        }
    }

    /// Whether there is anything to show: the lights once the countdown has
    /// begun, and the green ones for a moment after GO.
    fn showing(&self) -> bool {
        (self.held() && self.elapsed >= 0.0) || self.green()
    }

    /// Follow a shared-practice session. Its start time is agreed with the
    /// other driver on the real clock, so the lights are set from it rather
    /// than counted here: the third light a second before the agreed start,
    /// and GO when the session says driving has begun.
    fn follow(&mut self, to_go: Option<f64>, driving: bool) {
        match to_go {
            Some(to_go) => {
                if self.go_at != FULL || !self.held() {
                    *self = Self::waiting();
                }
                self.elapsed = (FULL - to_go as f32).clamp(0.0, FULL - 0.001);
            }
            None if driving => {
                if self.held() {
                    self.elapsed = self.go_at;
                }
            }
            None => {
                if self.showing() || !self.held() {
                    *self = Self::waiting();
                }
            }
        }
    }

    /// Whether the car is still waiting for GO.
    pub(crate) fn held(&self) -> bool {
        self.elapsed < self.go_at
    }

    /// How many red lights are lit: one a second, the third a second before
    /// GO. A countdown shorter than that lights all three at once.
    fn red(&self) -> u8 {
        if !self.held() {
            return 0;
        }
        let first = self.go_at - 3.0;
        ((self.elapsed - first).floor() + 1.0).clamp(0.0, 3.0) as u8
    }

    /// Whether the green lights are showing.
    fn green(&self) -> bool {
        self.go_at > 0.0 && !self.held() && self.elapsed < self.go_at + GREEN_FOR
    }

    /// Signals given so far, counting GO as the fourth.
    fn signals(&self) -> u8 {
        if self.held() { self.red() } else { 4 }
    }

    /// The word under the lights.
    fn caption(&self) -> &'static str {
        match (self.held(), self.red()) {
            (false, _) => "GO",
            (true, 0) => "READY",
            _ => match (self.go_at - self.elapsed).ceil() as u8 {
                3.. => "3",
                2 => "2",
                _ => "1",
            },
        }
    }
}

pub struct CountdownPlugin;

impl Plugin for CountdownPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Start>()
            .add_message::<StartLight>()
            .add_systems(Startup, setup)
            .add_systems(
                PreUpdate,
                (begin, tick, hold)
                    .chain()
                    .in_set(CountdownSet)
                    .after(ClockSet)
                    .after(CarResetSet)
                    .after(crate::multiplayer::ReadySet),
            )
            .add_systems(Update, draw);
    }
}

/// The countdown decides who is held here, after every reset has been heard.
#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct CountdownSet;

/// A reset starts a countdown: the whole one onto a different circuit, mode or
/// car, a short one for a restart. Shared practice has its own countdown,
/// agreed with the other driver, and the lights show that one instead.
#[allow(clippy::too_many_arguments)]
fn begin(
    mut resets: MessageReader<Reset>,
    track: Res<Track>,
    mode: Res<Mode>,
    spec: Res<Spec>,
    real: Res<Time<Real>>,
    session: Option<Res<crate::multiplayer::Session>>,
    settings: Option<Res<crate::settings::Settings>>,
    mut start: ResMut<Start>,
) {
    use crate::settings::Countdown;
    let restarted = resets.read().next().is_some();
    if let Some(session) = session.filter(|session| session.active()) {
        let now = real.elapsed_secs_f64();
        start.follow(session.seconds_to_start(now), session.driving());
        return;
    }
    let chosen = settings.map_or(Countdown::Short, |s| s.countdown);
    let new_start = track.is_changed() || mode.is_changed() || spec.is_changed();
    // A session that ended mid-countdown leaves nobody to start it.
    let again = restarted || start.elapsed == f32::NEG_INFINITY;
    if new_start || again {
        *start = match chosen {
            Countdown::Off => Start::none(),
            Countdown::Full => Start::of(FULL),
            Countdown::Short if new_start => Start::of(FULL),
            Countdown::Short => Start::of(SHORT),
        };
    }
}

fn tick(time: Res<Time>, mut start: ResMut<Start>, mut lights: MessageWriter<StartLight>) {
    if start.elapsed.is_finite() {
        start.elapsed += time.delta_secs();
    }
    let signals = start.signals();
    if signals > start.given {
        start.given = signals;
        lights.write(StartLight { go: signals == 4 });
    }
}

/// Keep the player's car out of the engine until GO.
fn hold(
    mut commands: Commands,
    start: Res<Start>,
    held: Query<Entity, (With<Player>, With<Held>)>,
    free: Query<Entity, (With<Player>, Without<Held>)>,
) {
    if start.held() {
        for car in &free {
            commands.entity(car).insert(Held);
        }
    } else {
        for car in &held {
            commands.entity(car).remove::<Held>();
        }
    }
}

#[derive(Component)]
struct Pod;
#[derive(Component)]
struct Light(u8);
#[derive(Component)]
struct Caption;

fn setup(mut commands: Commands) {
    commands
        .spawn((
            Pod,
            Node {
                position_type: PositionType::Absolute,
                top: px(96),
                left: px(0),
                right: px(0),
                justify_content: JustifyContent::Center,
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|pod| {
            // One dark panel behind the lights and the word under them, so
            // both read against bright sky and pale asphalt alike.
            pod.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(8),
                    padding: UiRect::new(px(28), px(28), px(20), px(12)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(26)),
                    ..default()
                },
                BackgroundColor(FRONT),
                BorderColor::all(LINE),
            ))
            .with_children(|panel| {
                panel
                    .spawn(Node {
                        column_gap: px(24),
                        ..default()
                    })
                    .with_children(|row| {
                        for i in 0..3 {
                            row.spawn((
                                Light(i),
                                Node {
                                    width: px(LIGHT),
                                    height: px(LIGHT),
                                    border: UiRect::all(px(2)),
                                    border_radius: BorderRadius::all(px(LIGHT / 2.0)),
                                    ..default()
                                },
                                BackgroundColor(DARK),
                                BorderColor::all(LINE),
                            ));
                        }
                    });
                panel.spawn((Caption, label("", 50.0, TEXT)));
            });
        });
}

/// Colours and words change only when the countdown moves on.
fn draw(
    start: Res<Start>,
    mut pods: Query<&mut Visibility, With<Pod>>,
    mut lights: Query<(&Light, &mut BackgroundColor)>,
    mut captions: Query<(&mut Text, &mut TextColor), With<Caption>>,
) {
    let showing = start.showing();
    for mut visibility in &mut pods {
        visibility.set_if_neq(if showing {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    if !showing {
        return;
    }
    let red = start.red();
    for (light, mut colour) in &mut lights {
        let wanted = if start.green() {
            GREEN
        } else if light.0 < red {
            RED
        } else {
            DARK
        };
        colour.set_if_neq(BackgroundColor(wanted));
    }
    for (mut text, mut colour) in &mut captions {
        if text.0 != start.caption() {
            text.0 = start.caption().into();
        }
        colour.set_if_neq(TextColor(if start.green() { GREEN } else { TEXT }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::car::{Car, CarPlugin, Controls, Handling};
    use crate::lap::{LapPlugin, LapTimer};
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    #[test]
    fn the_lights_come_on_a_second_apart_then_go() {
        let at = |elapsed| Start {
            elapsed,
            ..Start::of(FULL)
        };
        assert_eq!((at(0.5).red(), at(0.5).caption()), (0, "READY"));
        assert_eq!((at(1.0).red(), at(1.0).caption()), (1, "3"));
        assert_eq!((at(2.0).red(), at(2.0).caption()), (2, "2"));
        assert_eq!((at(3.0).red(), at(3.0).caption()), (3, "1"));
        assert!(at(3.79).held());
        assert!(!at(3.8).held() && at(3.8).green());
        assert_eq!(at(3.9).caption(), "GO");
        assert!(!at(4.5).green());

        let short = Start::of(SHORT);
        assert_eq!(short.red(), 3, "a restart lights all three at once");
    }

    /// The car, the clock and the countdown, on the production schedules,
    /// ten milliseconds a frame, with the throttle held from the first one.
    fn game() -> App {
        let track = Track::any();
        let start = track.start_transform();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_message::<Reset>()
            .insert_resource(track)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Assets<StandardMaterial>>()
            .add_plugins((
                crate::pause::PausePlugin,
                crate::input::InputPlugin,
                CarPlugin,
                LapPlugin,
            ))
            .init_resource::<Start>()
            .add_message::<StartLight>()
            .add_systems(
                PreUpdate,
                (begin, tick, hold)
                    .chain()
                    .after(ClockSet)
                    .after(CarResetSet),
            );
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

    fn car(app: &mut App) -> (Vec3, f32) {
        let mut cars = app
            .world_mut()
            .query_filtered::<(&Transform, &Car), With<Player>>();
        let (at, car) = cars.single(app.world()).unwrap();
        (at.translation, car.velocity.length())
    }

    fn frames(app: &mut App, n: usize) {
        for _ in 0..n {
            app.update();
        }
    }

    #[test]
    fn the_car_does_not_move_before_go_even_at_full_throttle() {
        let mut app = game();
        let grid = Track::any().start_transform().translation;
        frames(&mut app, 370);
        assert!(app.world().resource::<Start>().held());
        let (at, speed) = car(&mut app);
        assert_eq!(at, grid, "the car crept off the grid");
        assert_eq!(speed, 0.0);
        assert!(!app.world().resource::<LapTimer>().running());
    }

    #[test]
    fn throttle_held_through_go_launches_at_once_and_the_clock_starts_at_the_line() {
        let mut app = game();
        frames(&mut app, 390);
        assert!(!app.world().resource::<Start>().held());
        frames(&mut app, 3);
        assert!(car(&mut app).1 > 0.1, "the car did not launch at GO");
        let mut armed = false;
        for _ in 0..600 {
            app.update();
            if app.world().resource::<LapTimer>().running() {
                armed = true;
                break;
            }
        }
        assert!(armed, "the car never reached the line");
        // Armed at the line within the frame that crossed it: at most the
        // physics steps of one 10 ms frame are on the clock.
        assert!(app.world().resource::<LapTimer>().current < 0.0101);
    }

    #[test]
    fn a_restart_runs_the_short_countdown_and_keeps_the_board() {
        let mut app = game();
        frames(&mut app, 500);
        app.world_mut().resource_mut::<LapTimer>().best = Some(58.0);
        app.world_mut().write_message(Reset);
        frames(&mut app, 1);
        let start = *app.world().resource::<Start>();
        assert!(start.held());
        assert_eq!(start.go_at, SHORT);
        assert_eq!(car(&mut app).1, 0.0, "the restart left the car moving");
        frames(&mut app, 65);
        assert!(!app.world().resource::<Start>().held());
        assert_eq!(app.world().resource::<LapTimer>().best, Some(58.0));
    }

    #[test]
    fn a_new_circuit_mode_or_car_runs_the_whole_countdown() {
        for change in 0..3 {
            let mut app = game();
            frames(&mut app, 500);
            match change {
                0 => app.insert_resource(Track::any()),
                1 => app.insert_resource(Mode::Pro),
                _ => app.insert_resource(Spec::Express),
            };
            app.world_mut().write_message(Reset);
            frames(&mut app, 1);
            assert_eq!(
                app.world().resource::<Start>().go_at,
                FULL,
                "change {change}"
            );
        }
    }

    #[test]
    fn a_pause_freezes_the_countdown_and_resuming_carries_on() {
        let mut app = game();
        frames(&mut app, 150);
        let before = app.world().resource::<Start>().elapsed;
        app.world_mut().resource_mut::<Time<Virtual>>().pause();
        *app.world_mut().resource_mut::<crate::pause::Halt>() = crate::pause::Halt::Pause;
        frames(&mut app, 200);
        let during = app.world().resource::<Start>().elapsed;
        assert!(
            (during - before).abs() < 0.011,
            "the countdown ran on through the pause"
        );
        *app.world_mut().resource_mut::<crate::pause::Halt>() = crate::pause::Halt::Nothing;
        frames(&mut app, 20);
        assert!(app.world().resource::<Start>().elapsed > during + 0.1);
        assert!(app.world().resource::<Start>().held());
    }

    #[test]
    fn shared_practice_sets_the_lights_from_the_agreed_start() {
        let mut start = Start::of(FULL);
        start.follow(None, false);
        assert!(
            start.held() && !start.showing(),
            "lights shown before the countdown"
        );
        start.follow(Some(3.2), false);
        assert_eq!((start.red(), start.caption()), (0, "READY"));
        start.follow(Some(2.9), false);
        assert_eq!((start.red(), start.caption()), (1, "3"));
        start.follow(Some(0.5), false);
        assert_eq!((start.red(), start.caption()), (3, "1"));
        start.follow(Some(0.0), false);
        assert!(start.held(), "released before the session said go");
        start.follow(None, true);
        assert!(!start.held() && start.green());
        start.follow(None, true);
        assert!(!start.held(), "driving put the car back on hold");
        // Leaving the session and joining another starts from waiting again.
        start.follow(None, false);
        assert!(start.held() && !start.showing());
    }

    #[test]
    fn the_countdown_setting_decides_how_long_a_start_waits() {
        use crate::settings::{Countdown, Settings};
        for (chosen, restart_waits) in [
            (Countdown::Full, FULL),
            (Countdown::Short, SHORT),
            (Countdown::Off, 0.0),
        ] {
            let mut app = game();
            app.insert_resource(Settings {
                countdown: chosen,
                ..Default::default()
            });
            frames(&mut app, 500);
            app.world_mut().write_message(Reset);
            frames(&mut app, 1);
            assert_eq!(
                app.world().resource::<Start>().go_at,
                restart_waits,
                "{chosen:?}"
            );
        }
    }

    #[test]
    fn every_signal_is_given_once() {
        #[derive(Resource, Default)]
        struct Heard(Vec<bool>);
        fn listen(mut lights: MessageReader<StartLight>, mut heard: ResMut<Heard>) {
            heard.0.extend(lights.read().map(|light| light.go));
        }
        let mut app = game();
        app.init_resource::<Heard>().add_systems(Update, listen);
        frames(&mut app, 450);
        assert_eq!(
            app.world().resource::<Heard>().0,
            vec![false, false, false, true]
        );
    }
}
