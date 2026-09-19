use bevy::prelude::*;

use crate::car::{Car, Mode, Setup, Spec};
use crate::ghost::Ghost;
use crate::lap::{LapTimer, format_time};
use crate::track::Track;

pub(crate) const AMBER: Color = crate::ui::ACCENT;
pub(crate) const AMBER_DIM: Color = crate::ui::MUTED;
pub(crate) const PANEL: Color = Color::srgba(0.035, 0.05, 0.065, 0.9);
pub(crate) const FRONT: Color = Color::srgba(0.045, 0.06, 0.073, 0.98);
/// The delta to the ghost: green when this lap is ahead of it, red when behind.
const AHEAD: Color = Color::srgb(0.38, 0.86, 0.42);
const BEHIND: Color = Color::srgb(0.96, 0.32, 0.26);

/// Arcade speedometer calibration for the miniature world. On a flat straight,
/// Clubman / Tourer / Express settle near 217 / 240 / 298 displayed km/h.
/// Shared by all cars and independent of model size; physics stays in m/s.
const DISPLAY_SPEED_SCALE: f32 = 3.0;

/// Side of the g-meter's square, in pixels, and the reading that reaches its
/// edge. Tyres give up somewhere near 1.2 g, so a needle on the rim means the
/// car is at the limit.
const METER: f32 = 92.0;
const FULL_SCALE: f32 = 1.4;
const NEEDLE: f32 = 12.0;
/// The setup slider: as wide as the meter above it, with three notches the knob
/// snaps between.
const SLIDER: f32 = METER;
const KNOB: f32 = 26.0;
const TRACK: f32 = 10.0;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(
            Update,
            (
                show,
                draw_controls,
                draw_clock,
                draw_g_meter,
                draw_setup,
                draw_car,
                draw_delta,
                draw_circuit,
            )
                .after(crate::ghost::GhostSet),
        );
    }
}

#[derive(Component)]
struct ClockReadout;

#[derive(Component)]
struct Needle;

#[derive(Component)]
struct GReadout;

#[derive(Component)]
struct SpeedReadout;

#[derive(Component)]
struct SetupKnob;

#[derive(Component)]
struct SetupName;

#[derive(Component)]
struct CarName;

#[derive(Component)]
struct DeltaReadout;

#[derive(Component)]
struct CircuitName;

#[derive(Component)]
struct Instrument;

#[derive(Component)]
struct ControlHints;

fn setup(mut commands: Commands) {
    use crate::ui::{LINE, TEXT, label};
    commands
        .spawn((
            Instrument,
            Node {
                position_type: PositionType::Absolute,
                top: px(24),
                left: px(24),
                padding: UiRect::axes(px(18), px(14)),
                border_radius: BorderRadius::all(px(10)),
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|session| {
            session.spawn(label("TODORA  /  FREE DRIVE", 12.0, AMBER));
            session.spawn((CircuitName, label("", 23.0, TEXT)));
        });
    commands
        .spawn((
            Instrument,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(94),
                left: px(24),
                padding: UiRect::all(px(18)),
                border_radius: BorderRadius::all(px(12)),
                column_gap: px(24),
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|instruments| {
            instruments
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(5),
                    min_width: px(138),
                    ..default()
                })
                .with_children(|speed| {
                    speed.spawn((CarName, label("", 12.0, AMBER_DIM)));
                    speed.spawn((SpeedReadout, label("0", 48.0, TEXT)));
                    speed.spawn(label("KM/H", 11.0, AMBER_DIM));
                    speed.spawn((SetupName, label("", 11.0, AMBER)));
                });
            instruments
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(9),
                    ..default()
                })
                .with_children(|g| {
                    g.spawn((
                        Node {
                            width: px(METER),
                            height: px(METER),
                            border: UiRect::all(px(1)),
                            border_radius: BorderRadius::all(px(METER / 2.0)),
                            ..default()
                        },
                        BorderColor::all(LINE),
                    ))
                    .with_children(|meter| {
                        for horizontal in [true, false] {
                            meter.spawn((
                                Node {
                                    position_type: PositionType::Absolute,
                                    left: px(if horizontal { 8.0 } else { METER / 2.0 }),
                                    top: px(if horizontal { METER / 2.0 } else { 8.0 }),
                                    width: px(if horizontal { METER - 16.0 } else { 1.0 }),
                                    height: px(if horizontal { 1.0 } else { METER - 16.0 }),
                                    ..default()
                                },
                                BackgroundColor(LINE),
                            ));
                        }
                        meter.spawn((
                            Needle,
                            Node {
                                position_type: PositionType::Absolute,
                                left: px((METER - NEEDLE) / 2.0),
                                top: px((METER - NEEDLE) / 2.0),
                                width: px(NEEDLE),
                                height: px(NEEDLE),
                                border_radius: BorderRadius::all(px(NEEDLE / 2.0)),
                                ..default()
                            },
                            BackgroundColor(AMBER),
                        ));
                    });
                    g.spawn((GReadout, label("0.00 G", 12.0, AMBER_DIM)));
                    g.spawn((
                        Node {
                            width: px(SLIDER),
                            height: px(TRACK),
                            border_radius: BorderRadius::all(px(5)),
                            ..default()
                        },
                        BackgroundColor(LINE),
                    ))
                    .with_children(|slider| {
                        for i in 0..3 {
                            slider.spawn(notch_mark(i));
                        }
                        slider.spawn((
                            SetupKnob,
                            Node {
                                position_type: PositionType::Absolute,
                                width: px(KNOB),
                                height: px(TRACK),
                                border_radius: BorderRadius::all(px(5)),
                                ..default()
                            },
                            BackgroundColor(AMBER),
                        ));
                    });
                });
        });
    commands
        .spawn((
            Instrument,
            Node {
                position_type: PositionType::Absolute,
                top: px(24),
                right: px(24),
                padding: UiRect::all(px(20)),
                border_radius: BorderRadius::all(px(12)),
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                min_width: px(220),
                ..default()
            },
            BackgroundColor(PANEL),
        ))
        .with_children(|timing| {
            timing.spawn(label("SESSION TIMING", 11.0, AMBER_DIM));
            timing.spawn((
                ClockReadout,
                label(clock_text(&LapTimer::default()), 24.0, TEXT),
            ));
            timing.spawn((DeltaReadout, label("GHOST  --", 20.0, AMBER_DIM)));
        });
    commands.spawn((Instrument, ControlHints, label("WASD / Arrows  Drive    Space  Handbrake    R  Restart\nG  Ghost    Scroll  Zoom    Esc  Pause", 12.0, TEXT), Node {
        position_type: PositionType::Absolute, bottom: px(24), left: px(24),
        padding: UiRect::axes(px(14), px(10)), border_radius: BorderRadius::all(px(8)), ..default()
    }, BackgroundColor(PANEL)));
}

fn draw_controls(pads: Query<&Gamepad>, mut text: Query<&mut Text, With<ControlHints>>) {
    let hint = if pads.is_empty() {
        "WASD / Arrows  Drive    Space  Handbrake    R  Restart\nG  Ghost    Scroll  Zoom    Esc  Pause"
    } else {
        "Left stick  Steer    A  Gas    X  Brake    B  Handbrake    RB  Reset\nStart  Pause    LB  Garage    View  Circuits    Y  Ghost"
    };
    if let Ok(mut text) = text.single_mut()
        && text.0 != hint
    {
        text.0 = hint.into();
    }
}

fn show(halt: Res<crate::pause::Halt>, mut instruments: Query<&mut Visibility, With<Instrument>>) {
    if halt.is_changed() {
        for mut visible in &mut instruments {
            *visible = if halt.stopped() {
                Visibility::Hidden
            } else {
                Visibility::Visible
            };
        }
    }
}

/// A tick on the slider's track, centred under the notch the knob sits on.
fn notch_mark(notch: usize) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: px(knob_left(notch) + KNOB / 2.0 - 1.0),
            top: px(TRACK / 2.0 - 1.0),
            width: px(2),
            height: px(2),
            ..default()
        },
        BackgroundColor(AMBER_DIM),
    )
}

/// Where the knob's left edge sits for a notch.
fn knob_left(notch: usize) -> f32 {
    notch as f32 * (SLIDER - KNOB) / 2.0
}

/// Slide the knob to the chosen notch and name it.
fn draw_setup(
    chosen: Res<Setup>,
    mut knob: Query<&mut Node, With<SetupKnob>>,
    mut name: Query<&mut Text, With<SetupName>>,
) {
    if !chosen.is_changed() {
        return;
    }
    if let Ok(mut node) = knob.single_mut() {
        node.left = px(knob_left(chosen.notch()));
    }
    if let Ok(mut text) = name.single_mut() {
        text.0 = chosen.name().into();
    }
}

/// Name the car being driven. It sits over the meter rather than in the menu,
/// because which car you are in is a thing you want to know while driving it and
/// the menu is only up when you are not.
fn draw_car(spec: Res<Spec>, mode: Res<Mode>, mut readout: Query<&mut Text, With<CarName>>) {
    if !spec.is_changed() && !mode.is_changed() {
        return;
    }
    if let Ok(mut text) = readout.single_mut() {
        text.0 = format!("{} / {}", spec.name(), mode.name().to_uppercase());
    }
}

/// Name the circuit being driven, so the board above it is read against the
/// right one.
fn draw_circuit(track: Res<Track>, mut readout: Query<&mut Text, With<CircuitName>>) {
    if !track.is_changed() {
        return;
    }
    if let Ok(mut text) = readout.single_mut() {
        text.0 = track.circuit().name.to_uppercase();
    }
}

/// The gap to the ghost, keyed to where the car is on the circuit: how many
/// seconds ahead or behind this lap is against the best, right now.
fn draw_delta(
    ghost: Res<Ghost>,
    mut readout: Query<(&mut Text, &mut TextColor), With<DeltaReadout>>,
) {
    let Ok((mut text, mut color)) = readout.single_mut() else {
        return;
    };
    let (shown, tint) = match (ghost.on, ghost.delta) {
        (false, _) => ("GHOST OFF".to_string(), AMBER_DIM),
        (true, None) => ("GHOST  --".to_string(), AMBER_DIM),
        (true, Some(delta)) => (
            format!("{delta:+.2}"),
            if delta <= 0.0 { AHEAD } else { BEHIND },
        ),
    };
    if text.0 != shown {
        text.0 = shown;
    }
    color.0 = tint;
}

fn draw_clock(timer: Res<LapTimer>, mut readout: Query<&mut Text, With<ClockReadout>>) {
    if !timer.is_changed() {
        return;
    }
    let Ok(mut text) = readout.single_mut() else {
        return;
    };
    text.0 = clock_text(&timer);
}

/// Put the needle where the car is pulling. Cornering moves it sideways,
/// braking and the climb out of a dip move it up and down.
fn draw_g_meter(
    cars: Query<&Car>,
    mut needle: Query<&mut Node, With<Needle>>,
    mut readout: Query<&mut Text, With<GReadout>>,
    mut speed: Query<&mut Text, (With<SpeedReadout>, Without<GReadout>)>,
) {
    let Ok(car) = cars.single() else {
        return;
    };
    if let Ok(mut text) = speed.single_mut() {
        // A hill adds a lot of speed, and a corner that will not come round is
        // usually a corner arrived at too fast. Worth being able to see.
        text.0 = format!("{:.0}", car.velocity.length() * 3.6 * DISPLAY_SPEED_SCALE);
    }
    let reading = (car.g_force / FULL_SCALE).clamp_length_max(1.0) * (METER - NEEDLE) / 2.0;
    if let Ok(mut node) = needle.single_mut() {
        node.left = px((METER - NEEDLE) / 2.0 + reading.x);
        // Screen y grows downward, so accelerating pushes the needle up.
        node.top = px((METER - NEEDLE) / 2.0 - reading.y);
    }
    if let Ok(mut text) = readout.single_mut() {
        text.0 = format!("{:.2} G", car.g_force.length());
    }
}

fn clock_text(timer: &LapTimer) -> String {
    let last = timer
        .last
        .map(format_time)
        .unwrap_or_else(|| "--:--.--".into());
    let best = timer
        .best
        .map(format_time)
        .unwrap_or_else(|| "--:--.--".into());
    format!(
        "LAP  {}\n{}\nLAST {}\nBEST {}",
        timer.completed,
        format_time(timer.current),
        last,
        best
    )
}
