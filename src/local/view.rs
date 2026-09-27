use super::*;
use crate::ui::{ACCENT, LINE, MUTED, SURFACE, TEXT, label};
#[derive(Component)]
pub(super) struct Root;
#[derive(Component)]
pub(super) struct Button(Action);
#[derive(Component)]
pub(super) struct Readout(usize);
#[derive(Component)]
pub(super) struct Standings;

pub(super) fn click(
    buttons: Query<(&Button, &Interaction), Changed<Interaction>>,
    mut actions: MessageWriter<Action>,
) {
    for (button, interaction) in &buttons {
        if *interaction == Interaction::Pressed {
            actions.write(button.0);
        }
    }
}
fn button(
    parent: &mut ChildSpawnerCommands,
    title: impl Into<String>,
    action: Action,
    primary: bool,
) {
    parent
        .spawn((
            bevy::prelude::Button,
            Button(action),
            Node {
                padding: UiRect::axes(px(20), px(12)),
                border_radius: BorderRadius::all(px(10)),
                border: UiRect::all(px(1)),
                ..default()
            },
            BackgroundColor(if primary { ACCENT } else { SURFACE }),
            BorderColor::all(LINE),
        ))
        .with_children(|p| {
            p.spawn(label(
                title,
                19.0,
                if primary { crate::hud::FRONT } else { TEXT },
            ));
        });
}
pub(super) fn draw(
    mut commands: Commands,
    race: Res<LocalRace>,
    halt: Res<Halt>,
    pads: Query<&Gamepad>,
    roots: Query<Entity, With<Root>>,
    mut seen: Local<String>,
) {
    let connected: Vec<_> = race.devices.iter().map(|d| d.name(&pads)).collect();
    let key = format!(
        "{:?}/{}/{:?}/{:?}/{}",
        *halt, race.active, race.devices, connected, race.circuit
    );
    if *seen == key {
        return;
    }
    *seen = key;
    for root in &roots {
        commands.entity(root).despawn();
    }
    let screen = |z| {
        (
            Root,
            GlobalZIndex(z),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
        )
    };
    if race.active {
        commands.spawn(screen(5)).with_children(|root| {
            for (i, &colour) in COLOURS.iter().enumerate().take(race.devices.len()) {
                root.spawn(Node {
                    position_type: PositionType::Absolute,
                    left: percent((i % 2) as f32 * 50.0),
                    top: percent(if race.devices.len() == 2 {
                        0.0
                    } else {
                        (i / 2) as f32 * 50.0
                    }),
                    width: percent(50.0),
                    height: percent(if race.devices.len() == 2 { 100.0 } else { 50.0 }),
                    border: UiRect::all(px(1)),
                    ..default()
                })
                .insert(BorderColor::all(colour.with_alpha(0.5)))
                .with_children(|viewport| {
                    viewport
                        .spawn((
                            Node {
                                position_type: PositionType::Absolute,
                                left: px(16),
                                top: px(16),
                                padding: UiRect::all(px(12)),
                                flex_direction: FlexDirection::Column,
                                row_gap: px(4),
                                border_radius: BorderRadius::all(px(8)),
                                ..default()
                            },
                            BackgroundColor(crate::hud::FRONT.with_alpha(0.90)),
                        ))
                        .with_children(|p| {
                            p.spawn(label(
                                crate::text::tf("local.player", &[&(i + 1)]),
                                22.0,
                                colour,
                            ));
                            p.spawn((Readout(i), label("", 18.0, TEXT)));
                        });
                });
            }
            if race.devices.len() == 3 {
                root.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: percent(50),
                        top: percent(50),
                        width: percent(50),
                        height: percent(50),
                        padding: UiRect::all(px(30)),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::Center,
                        row_gap: px(18),
                        ..default()
                    },
                    BackgroundColor(crate::hud::FRONT),
                ))
                .with_children(|p| {
                    p.spawn(crate::text::label("local.session", 26.0, ACCENT));
                    p.spawn((Standings, label("", 22.0, TEXT)));
                    p.spawn(crate::text::label("local.race_hint", 16.0, MUTED));
                });
            }
        });
    }
    if !matches!(*halt, Halt::LocalLobby | Halt::LocalPause) {
        return;
    }
    commands
        .spawn(screen(25))
        .insert((
            BackgroundColor(Color::srgba(0.018, 0.026, 0.032, 0.92)),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(920),
                    padding: UiRect::all(px(32)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(16),
                    border_radius: BorderRadius::all(px(20)),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BackgroundColor(crate::hud::FRONT),
                BorderColor::all(LINE),
            ))
            .with_children(|panel| {
                panel.spawn(crate::text::label(
                    if race.active {
                        "local.paused"
                    } else {
                        "local.title"
                    },
                    40.0,
                    TEXT,
                ));
                panel.spawn(crate::text::label("local.subtitle", 18.0, MUTED));
                if !race.active {
                    panel
                        .spawn(Node {
                            align_items: AlignItems::Center,
                            justify_content: JustifyContent::SpaceBetween,
                            ..default()
                        })
                        .with_children(|row| {
                            button(row, "←", Action::Previous, false);
                            row.spawn(label(
                                crate::track::all_circuits()[race.circuit].name,
                                26.0,
                                ACCENT,
                            ));
                            button(row, "→", Action::Next, false);
                        });
                }
                for (i, &colour) in COLOURS.iter().enumerate() {
                    panel
                        .spawn((
                            Node {
                                padding: UiRect::axes(px(16), px(10)),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::SpaceBetween,
                                border_radius: BorderRadius::all(px(8)),
                                ..default()
                            },
                            BackgroundColor(SURFACE),
                        ))
                        .with_children(|row| {
                            row.spawn(label(
                                crate::text::tf("local.player", &[&(i + 1)]),
                                22.0,
                                colour,
                            ));
                            row.spawn(label(
                                connected
                                    .get(i)
                                    .map_or(crate::text::t("local.empty"), String::as_str),
                                18.0,
                                MUTED,
                            ));
                            if !race.active && i < race.devices.len() {
                                button(row, "×", Action::Remove(i), false);
                            }
                        });
                }
                if race.active {
                    panel.spawn(crate::text::label("local.reconnect", 16.0, MUTED));
                    panel
                        .spawn(Node {
                            column_gap: px(12),
                            ..default()
                        })
                        .with_children(|row| {
                            button(row, crate::text::t("local.resume"), Action::Resume, true);
                            button(row, crate::text::t("local.again"), Action::Again, false);
                            button(row, crate::text::t("local.leave"), Action::Leave, false);
                        });
                } else {
                    panel.spawn(crate::text::label("local.join_hint", 16.0, MUTED));
                    panel
                        .spawn(Node {
                            column_gap: px(12),
                            ..default()
                        })
                        .with_children(|row| {
                            button(row, crate::text::t("profile.back"), Action::Back, false);
                            button(row, "W A S D", Action::JoinWasd, false);
                            button(row, "↑ ↓ ← →", Action::JoinArrows, false);
                            button(
                                row,
                                crate::text::t("local.start"),
                                Action::Start,
                                race.devices.len() >= 2,
                            );
                        });
                    panel.spawn(crate::text::label("local.lobby_hint", 16.0, MUTED));
                }
            });
        });
}
pub(super) fn hud(
    cars: Query<(&Seat, &Car)>,
    mut readouts: Query<(&Readout, &mut Text), Without<Standings>>,
    mut standings: Query<&mut Text, With<Standings>>,
) {
    for (readout, mut text) in &mut readouts {
        let Some((seat, car)) = cars.iter().find(|(s, _)| s.index == readout.0) else {
            continue;
        };
        let best = seat.timer.best.map_or("—".into(), crate::lap::format_time);
        text.0 = format!(
            "{}  {}\n{}  {}\n{}  {:.0} km/h",
            crate::text::t("local.lap"),
            crate::lap::format_time(seat.timer.current),
            crate::text::t("local.best"),
            best,
            if seat.timer.invalid {
                crate::text::t("local.invalid")
            } else {
                ""
            },
            car.velocity.length() * 3.6 * crate::hud::DISPLAY_SPEED_SCALE
        );
    }
    for mut text in &mut standings {
        let mut times: Vec<_> = cars.iter().map(|(s, _)| (s.index, s.timer.best)).collect();
        times.sort_by(|a, b| {
            a.1.unwrap_or(f32::INFINITY)
                .total_cmp(&b.1.unwrap_or(f32::INFINITY))
        });
        text.0 = times
            .iter()
            .map(|(i, t)| {
                format!(
                    "{}   {}",
                    crate::text::tf("local.player", &[&(i + 1)]),
                    t.map_or("—".into(), crate::lap::format_time)
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
    }
}
