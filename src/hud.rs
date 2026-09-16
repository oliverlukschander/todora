use bevy::prelude::*;

use crate::car::Car;
use crate::lap::{format_time, LapTimer};

const AMBER: Color = Color::srgb(1.0, 0.72, 0.12);
const AMBER_DIM: Color = Color::srgb(0.72, 0.48, 0.08);
const PANEL: Color = Color::srgba(0.04, 0.03, 0.02, 0.82);

/// Side of the g-meter's square, in pixels, and the reading that reaches its
/// edge. Tyres give up somewhere near 1.2 g, so a needle on the rim means the
/// car is at the limit.
const METER: f32 = 92.0;
const FULL_SCALE: f32 = 1.4;
const NEEDLE: f32 = 12.0;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, (draw_clock, draw_g_meter));
    }
}

#[derive(Component)]
struct ClockReadout;

#[derive(Component)]
struct Needle;

#[derive(Component)]
struct GReadout;

fn setup(mut commands: Commands) {
    commands.spawn((
        Text::new(
            "W — throttle\nS — brake, reverse at a stop\nA / D — steer\nSpace — handbrake\nScroll — zoom",
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
                GReadout,
                Text::new("0.00 G"),
                TextFont {
                    font_size: FontSize::Px(15.0),
                    ..default()
                },
                TextColor(AMBER),
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
        children![(
            ClockReadout,
            Text::new(clock_text(&LapTimer::default())),
            TextFont {
                font_size: FontSize::Px(22.0),
                ..default()
            },
            TextColor(AMBER),
        )],
    ));
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
) {
    let Ok(car) = cars.single() else {
        return;
    };
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
    let last = timer.last.map(format_time).unwrap_or_else(|| "--:--.--".into());
    let best = timer.best.map(format_time).unwrap_or_else(|| "--:--.--".into());
    format!(
        "LAP  {}\n{}\nLAST {}\nBEST {}",
        timer.completed,
        format_time(timer.current),
        last,
        best
    )
}
