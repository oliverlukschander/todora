//! The title screen the game opens on, and the fade that covers a change of
//! circuit.
//!
//! Over a slow orbit of the circuit last driven: **Drive**, **Circuits**,
//! **World leaderboard**, **This week's challenge**, **Settings** and **Quit**,
//! chosen like the pause menu. The world is stopped behind it, so the countdown
//! and the first-drive cards wait for Drive. Settings and the leaderboard come
//! back here rather than to the pause. Visual checks skip it.
//!
//! A change of circuit fades in from black over a third of a second, so the
//! frame the new circuit is built on never shows half of it.

use bevy::prelude::*;

use crate::hud::{AMBER, AMBER_DIM, FRONT};
use crate::pause::{Halt, HaltSet};
use crate::track::Track;
use crate::ui::{LINE, Navigation, SURFACE, TEXT, label};

const FADE_FOR: f32 = 0.35;
/// One turn of the orbit, in seconds.
const ORBIT: f32 = 90.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Choice {
    Drive,
    Circuits,
    Board,
    Weekly,
    Settings,
    Quit,
}

impl Choice {
    const ALL: [Self; 6] = [
        Self::Drive,
        Self::Circuits,
        Self::Board,
        Self::Weekly,
        Self::Settings,
        Self::Quit,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Drive => "Drive",
            Self::Circuits => "Circuits",
            Self::Board => "World leaderboard",
            Self::Weekly => "This week's challenge",
            Self::Settings => "Settings",
            Self::Quit => "Quit",
        }
    }
}

/// Whether the title is where things go back to, and what is chosen on it.
#[derive(Resource, Default)]
pub(crate) struct Title {
    /// The title is up, or a page opened from it is.
    pub active: bool,
    at: usize,
    navigation: Navigation,
    clock: f32,
    fade: f32,
}

impl Title {
    /// Where a page opened from the title or the pause should go back to.
    pub(crate) fn back_to(title: Option<&Self>) -> Halt {
        if title.is_some_and(|t| t.active) {
            Halt::Title
        } else {
            Halt::Pause
        }
    }
}

pub struct TitlePlugin;

impl Plugin for TitlePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Title>()
            .add_systems(Startup, setup)
            .add_systems(PostStartup, open)
            .add_systems(PreUpdate, choose.after(HaltSet))
            .add_systems(Update, (orbit, draw, fade));
    }
}

/// The game opens on the title, except in a visual check.
fn open(
    read_only: Option<Res<crate::settings::ReadOnly>>,
    mut halt: ResMut<Halt>,
    mut title: ResMut<Title>,
) {
    if read_only.is_none() {
        *halt = Halt::Title;
        title.active = true;
    }
}

#[derive(Component)]
struct Panel;
#[derive(Component)]
struct Button(usize);
#[derive(Component)]
struct Curtain;

fn setup(mut commands: Commands) {
    commands.spawn((
        Curtain,
        GlobalZIndex(30),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundColor(Color::BLACK.with_alpha(0.0)),
        Visibility::Hidden,
    ));
    commands
        .spawn((
            Panel,
            GlobalZIndex(18),
            Node {
                position_type: PositionType::Absolute,
                left: px(64),
                top: px(0),
                bottom: px(0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                row_gap: px(10),
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|panel| {
            panel.spawn(label("TODORA", 72.0, TEXT));
            panel.spawn((
                label("Forty circuits. One lap at a time.", 20.0, AMBER_DIM),
                Node {
                    margin: UiRect::bottom(px(24)),
                    ..default()
                },
            ));
            for (i, choice) in Choice::ALL.iter().enumerate() {
                panel
                    .spawn((
                        Button(i),
                        Node {
                            width: px(340),
                            padding: UiRect::axes(px(18), px(11)),
                            border: UiRect::all(px(2)),
                            border_radius: BorderRadius::all(px(10)),
                            ..default()
                        },
                        BackgroundColor(FRONT),
                        BorderColor::all(LINE),
                    ))
                    .with_children(|button| {
                        button.spawn(label(choice.name(), 22.0, TEXT));
                    });
            }
            panel.spawn((
                label("↑ ↓  Choose     Enter / A  Go", 13.0, AMBER_DIM),
                Node {
                    margin: UiRect::top(px(16)),
                    ..default()
                },
            ));
        });
}

#[allow(clippy::too_many_arguments)]
fn choose(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    time: Res<Time<Real>>,
    challenge: Option<Res<crate::challenge::Challenge>>,
    mut halt: ResMut<Halt>,
    mut title: ResMut<Title>,
    mut menus: MessageWriter<crate::menu::OpenMenu>,
    mut go: MessageWriter<crate::track::GoTo>,
    mut mode: ResMut<crate::car::Mode>,
    mut exit: MessageWriter<AppExit>,
) {
    // Back from a page opened here, or on the way out to the road.
    if *halt == Halt::Nothing || *halt == Halt::Pause {
        title.active = false;
    }
    if *halt != Halt::Title || halt.is_changed() {
        return;
    }
    let step = title.navigation.read(&keys, &pads, time.elapsed_secs_f64());
    if step.y != 0 {
        title.at = (title.at as i32 + step.y).clamp(0, Choice::ALL.len() as i32 - 1) as usize;
    }
    let chosen = keys.just_pressed(KeyCode::Enter)
        || pads
            .iter()
            .any(|pad| pad.just_pressed(GamepadButton::South));
    if !chosen {
        return;
    }
    match Choice::ALL[title.at] {
        Choice::Drive => {
            title.active = false;
            *halt = Halt::Nothing;
        }
        Choice::Circuits => {
            title.active = false;
            *halt = Halt::Nothing;
            menus.write(crate::menu::OpenMenu(crate::menu::Page::Circuit));
        }
        Choice::Board => *halt = Halt::Board,
        Choice::Weekly => {
            title.active = false;
            if let Some(challenge) = challenge {
                go.write(crate::track::GoTo(challenge.circuit()));
                mode.set_if_neq(crate::car::Mode::Regular);
            }
            *halt = Halt::Nothing;
        }
        Choice::Settings => *halt = Halt::Settings,
        Choice::Quit => {
            exit.write(AppExit::Success);
        }
    }
}

/// A slow turn round the circuit, high enough to see all of it.
fn orbit(
    time: Res<Time<Real>>,
    halt: Res<Halt>,
    track: Res<Track>,
    mut title: ResMut<Title>,
    mut cameras: Query<&mut Transform, With<Camera3d>>,
    mut bounds: Local<Option<(String, Vec3, f32)>>,
) {
    if *halt != Halt::Title {
        return;
    }
    let id = track.circuit().id;
    if bounds.as_ref().is_none_or(|(b, _, _)| b != id) {
        let (mut low, mut high) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for (point, _) in track.map_points() {
            low = low.min(point);
            high = high.max(point);
        }
        let centre = (low + high) / 2.0;
        let radius = ((high - low) * Vec3::new(1.0, 0.0, 1.0)).length() / 2.0;
        *bounds = Some((id.to_string(), centre, radius));
    }
    let Some((_, centre, radius)) = bounds.as_ref() else {
        return;
    };
    title.clock += time.delta_secs();
    let angle = title.clock / ORBIT * std::f32::consts::TAU;
    let eye =
        *centre + Vec3::new(angle.cos(), 0.0, angle.sin()) * radius * 0.9 + Vec3::Y * radius * 0.55;
    for mut camera in &mut cameras {
        *camera = Transform::from_translation(eye).looking_at(*centre, Vec3::Y);
    }
}

fn draw(
    halt: Res<Halt>,
    title: Res<Title>,
    mut panels: Query<&mut Visibility, With<Panel>>,
    mut buttons: Query<(&Button, &mut BackgroundColor, &mut BorderColor)>,
) {
    let open = *halt == Halt::Title;
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
    for (button, mut colour, mut border) in &mut buttons {
        let active = button.0 == title.at;
        colour.set_if_neq(BackgroundColor(if active { SURFACE } else { FRONT }));
        *border = BorderColor::all(if active { AMBER } else { LINE });
    }
}

/// Fade in from black after the circuit changes.
fn fade(
    time: Res<Time<Real>>,
    track: Res<Track>,
    mut title: ResMut<Title>,
    mut curtains: Query<(&mut BackgroundColor, &mut Visibility), With<Curtain>>,
) {
    if track.is_changed() && !track.is_added() {
        title.fade = FADE_FOR;
    }
    if title.fade <= 0.0 {
        return;
    }
    title.fade = (title.fade - time.delta_secs()).max(0.0);
    for (mut colour, mut visibility) in &mut curtains {
        colour.0 = Color::BLACK.with_alpha(title.fade / FADE_FOR);
        visibility.set_if_neq(if title.fade > 0.0 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(Halt::Title)
            .insert_resource(Title {
                active: true,
                ..default()
            })
            .init_resource::<crate::car::Mode>()
            .add_message::<crate::menu::OpenMenu>()
            .add_message::<crate::track::GoTo>()
            .add_message::<AppExit>()
            .add_systems(Update, choose);
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
    fn drive_lets_the_game_go_and_settings_come_back_to_the_title() {
        let mut app = app();
        assert_eq!(
            Title::back_to(Some(app.world().resource::<Title>())),
            Halt::Title
        );
        for _ in 0..4 {
            tap(&mut app, KeyCode::ArrowDown);
        }
        tap(&mut app, KeyCode::Enter);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Settings);
        assert!(
            app.world().resource::<Title>().active,
            "settings go back to the title"
        );
        *app.world_mut().resource_mut::<Halt>() = Halt::Title;
        app.update();
        for _ in 0..4 {
            tap(&mut app, KeyCode::ArrowUp);
        }
        tap(&mut app, KeyCode::Enter);
        assert_eq!(*app.world().resource::<Halt>(), Halt::Nothing);
        assert!(!app.world().resource::<Title>().active);
        assert_eq!(
            Title::back_to(Some(app.world().resource::<Title>())),
            Halt::Pause
        );
    }
}
