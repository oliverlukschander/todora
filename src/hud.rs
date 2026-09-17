use bevy::prelude::*;

use crate::car::{Car, Setup};
use crate::ghost::Ghost;
use crate::lap::{LapTimer, format_time};

const AMBER: Color = Color::srgb(1.0, 0.72, 0.12);
const AMBER_DIM: Color = Color::srgb(0.72, 0.48, 0.08);
const PANEL: Color = Color::srgba(0.04, 0.03, 0.02, 0.82);
/// The delta to the ghost: green when this lap is ahead of it, red when behind.
const AHEAD: Color = Color::srgb(0.38, 0.86, 0.42);
const BEHIND: Color = Color::srgb(0.96, 0.32, 0.26);

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
            (draw_clock, draw_g_meter, draw_setup, draw_delta).after(crate::ghost::GhostSet),
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
struct DeltaReadout;

fn setup(mut commands: Commands) {
    commands.spawn((
        Text::new(
            "W — throttle\nS — brake, reverse at a stop\nA / D — steer\nSpace — handbrake\n1 / 2 / 3 — setup\nG — ghost\nR — restart\nScroll — zoom",
        ),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(Color::srgb(0.82, 0.82, 0.78)),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(16),
            left: px(16),
            ..default()
        },
    ));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(16),
            left: px(16),
            padding: UiRect::all(px(8)),
            border: UiRect::all(px(2)),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(6),
            ..default()
        },
        BackgroundColor(PANEL),
        BorderColor::all(AMBER_DIM),
        children![
            (
                Node {
                    width: px(METER),
                    height: px(METER),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BorderColor::all(AMBER_DIM),
                children![
                    // Crosshair, so a reading can be seen against straight ahead.
                    (
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(0),
                            top: px(METER / 2.0),
                            width: px(METER),
                            height: px(1),
                            ..default()
                        },
                        BackgroundColor(AMBER_DIM.with_alpha(0.45)),
                    ),
                    (
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(METER / 2.0),
                            top: px(0),
                            width: px(1),
                            height: px(METER),
                            ..default()
                        },
                        BackgroundColor(AMBER_DIM.with_alpha(0.45)),
                    ),
                    (
                        Needle,
                        Node {
                            position_type: PositionType::Absolute,
                            left: px((METER - NEEDLE) / 2.0),
                            top: px((METER - NEEDLE) / 2.0),
                            width: px(NEEDLE),
                            height: px(NEEDLE),
                            border: UiRect::all(px(1)),
                            border_radius: BorderRadius::all(px(NEEDLE / 2.0)),
                            ..default()
                        },
                        BackgroundColor(AMBER),
                        BorderColor::all(AMBER_DIM),
                    ),
                ],
            ),
            (
                SpeedReadout,
                Text::new("0 KM/H"),
                TextFont {
                    font_size: FontSize::Px(20.0),
                    ..default()
                },
                TextColor(AMBER),
            ),
            (
                GReadout,
                Text::new("0.00 G"),
                TextFont {
                    font_size: FontSize::Px(15.0),
                    ..default()
                },
                TextColor(AMBER_DIM),
            ),
            // The setup slider: a track with three notches, and a knob on one.
            (
                Node {
                    width: px(SLIDER),
                    height: px(TRACK),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(TRACK / 2.0)),
                    margin: UiRect::top(px(2)),
                    ..default()
                },
                BackgroundColor(AMBER_DIM.with_alpha(0.18)),
                BorderColor::all(AMBER_DIM),
                children![
                    notch_mark(0),
                    notch_mark(1),
                    notch_mark(2),
                    (
                        SetupKnob,
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(knob_left(1)),
                            top: px(-1),
                            width: px(KNOB),
                            height: px(TRACK),
                            border: UiRect::all(px(1)),
                            border_radius: BorderRadius::all(px(TRACK / 2.0)),
                            ..default()
                        },
                        BackgroundColor(AMBER),
                        BorderColor::all(AMBER_DIM),
                    ),
                ],
            ),
            (
                SetupName,
                Text::new(Setup::default().name()),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(AMBER_DIM),
            ),
        ],
    ));

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            top: px(16),
            right: px(16),
            padding: UiRect::axes(px(16), px(12)),
            border: UiRect::all(px(2)),
            flex_direction: FlexDirection::Column,
            row_gap: px(2),
            ..default()
        },
        BackgroundColor(PANEL),
        BorderColor::all(AMBER_DIM),
        children![
            (
                ClockReadout,
                Text::new(clock_text(&LapTimer::default())),
                TextFont {
                    font_size: FontSize::Px(22.0),
                    ..default()
                },
                TextColor(AMBER),
            ),
            (
                DeltaReadout,
                Text::new("GHOST  --"),
                TextFont {
                    font_size: FontSize::Px(26.0),
                    ..default()
                },
                TextColor(AMBER_DIM),
                Node {
                    margin: UiRect::top(px(6)),
                    ..default()
                },
            ),
        ],
    ));
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
        text.0 = format!("{:.0} KM/H", car.velocity.length() * 3.6);
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
