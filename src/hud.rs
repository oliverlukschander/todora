use bevy::prelude::*;

use crate::lap::{format_time, LapTimer};

const AMBER: Color = Color::srgb(1.0, 0.72, 0.12);
const AMBER_DIM: Color = Color::srgb(0.72, 0.48, 0.08);
const PANEL: Color = Color::srgba(0.04, 0.03, 0.02, 0.82);

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, draw_clock);
    }
}

#[derive(Component)]
struct ClockReadout;

fn setup(mut commands: Commands) {
    commands.spawn((
        Text::new("WASD / arrows — drive\nShift — brake\nScroll — zoom"),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(Color::srgb(0.82, 0.82, 0.78)),
        Node {
            position_type: PositionType::Absolute,
            top: px(16),
            left: px(16),
            ..default()
        },
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
