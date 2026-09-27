//! The visible action beside the board: choose a driver, then race their lap.
use super::*;
use crate::online::flags::Flags;
use crate::ui::ACCENT;

pub(super) fn action_button(
    parent: &mut ChildSpawnerCommands,
    text: impl Into<String>,
    action: Action,
    primary: bool,
) {
    parent
        .spawn((
            Button,
            action,
            Node {
                padding: UiRect::axes(px(12), px(9)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(7)),
                ..default()
            },
            BackgroundColor(if primary { ACCENT } else { FRONT }),
            BorderColor::all(if primary { ACCENT } else { LINE }),
        ))
        .with_children(|b| {
            b.spawn(label(text, 15.0, if primary { FRONT } else { TEXT }));
        });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_controls(
    mut commands: Commands,
    halt: Res<Halt>,
    browse: Res<Browse>,
    mode: Res<Mode>,
    online: Res<Online>,
    settings: Res<Settings>,
    flags: Res<Flags>,
    challenge: Res<crate::challenge::Challenge>,
    rival: Res<crate::ghost::Rival>,
    records: Res<crate::ghost::Records>,
    earned: Option<Res<crate::achievements::Earned>>,
    controls: Query<Entity, With<Controls>>,
    mut previous: Local<String>,
) {
    if *halt != Halt::Board {
        previous.clear();
        return;
    }
    let week = (browse.view == View::Week).then(|| challenge.label());
    let standing = matching_board(&online, &browse, *mode, &settings, &week);
    let rows = standing.map(|s| lines(s, browse.view)).unwrap_or_default();
    let selected = match rows.get(browse.row) {
        Some(Line::Place(place)) => Some(place),
        _ => None,
    };
    let key = format!(
        "{:?}:{}:{:?}:{}:{}:{}:{}:{}:{}:{}",
        browse.view,
        browse.row,
        selected,
        settings.name,
        settings.country,
        settings.online,
        settings.online_note,
        online.ghost_status,
        online.wanted.is_some(),
        rival.name
    );
    let personal = records.best(
        browse.circuit,
        if browse.view == View::Week {
            Mode::Regular
        } else {
            *mode
        },
    );
    let key = format!(
        "{key}:{}:{:?}:{personal:?}:{}",
        browse.circuit,
        *mode,
        earned
            .as_ref()
            .map_or(0, |e| e.modes.len() + e.earned.len())
    );
    if *previous == key {
        return;
    }
    *previous = key;
    let Ok(root) = controls.single() else {
        return;
    };
    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|card| {
        card.spawn(label(crate::text::t("profile.preview"), 12.0, ACCENT));
        card.spawn(Node {
            column_gap: px(10),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|row| {
            row.spawn((
                flags.image(&settings.country),
                Node {
                    width: px(32),
                    height: px(24),
                    ..default()
                },
            ));
            row.spawn(label(
                if settings.name.is_empty() {
                    crate::text::t("profile.guest")
                } else {
                    &settings.name
                },
                19.0,
                TEXT,
            ));
        });
        if !settings.online {
            card.spawn(label(crate::text::t("ghost.offline"), 15.0, MUTED));
            action_button(card, crate::text::t("profile.join"), Action::Profile, true);
        }
        if browse.view == View::Awards {
            if let Some(earned) = &earned {
                card.spawn(label(crate::text::t("ach.collections"), 20.0, TEXT));
                card.spawn(label(crate::text::t("ach.valid_progress"), 13.0, MUTED));
                for (name, done, total) in crate::achievements::tour_progress(earned) {
                    card.spawn(label(format!("{name}  {done}/{total}"), 16.0, TEXT));
                    card.spawn((
                        Node {
                            height: px(5),
                            width: percent(100),
                            ..default()
                        },
                        BackgroundColor(LINE),
                    ))
                    .with_children(|bar| {
                        bar.spawn((
                            Node {
                                height: percent(100),
                                width: percent(100.0 * done as f32 / total as f32),
                                ..default()
                            },
                            BackgroundColor(ACCENT),
                        ));
                    });
                }
            }
            return;
        }
        if browse.view == View::Records {
            card.spawn(label(crate::text::t("ghost.records_help"), 16.0, MUTED));
            return;
        }
        card.spawn((
            Node {
                height: px(1),
                margin: UiRect::axes(px(0), px(5)),
                ..default()
            },
            BackgroundColor(LINE),
        ));
        card.spawn(label(crate::text::t("ghost.opponent"), 12.0, ACCENT));
        if let Some(place) = selected {
            card.spawn(Node {
                column_gap: px(12),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    flags.image(place.country.as_deref().unwrap_or("")),
                    Node {
                        width: px(40),
                        height: px(30),
                        ..default()
                    },
                ));
                row.spawn(label(&place.name, 23.0, TEXT));
            });
            card.spawn(label(format_time(place.seconds as f32), 36.0, ACCENT));
            card.spawn(label(
                format!("#{}  ·  {}", place.rank, car_name(&place.car)),
                15.0,
                MUTED,
            ));
            if let Some(you) = standing.and_then(|s| s.you.as_ref()) {
                card.spawn(label(
                    crate::text::tf(
                        "ghost.gap",
                        &[&format!("{:+.2}", place.seconds - you.seconds)],
                    ),
                    14.0,
                    MUTED,
                ));
            }
            if online.wanted.is_some() {
                card.spawn(label(&online.ghost_status, 16.0, ACCENT));
                card.spawn(label(crate::text::t("ghost.ready_help"), 14.0, MUTED));
                action_button(card, crate::text::t("profile.cancel"), Action::Back, false);
            } else if settings.online {
                action_button(card, crate::text::t("ghost.race"), Action::Race, true);
                card.spawn(label(crate::text::t("ghost.race_help"), 14.0, MUTED));
                action_button(
                    card,
                    crate::text::t(if settings.rivals.contains(&place.player) {
                        "ghost.unpin"
                    } else {
                        "ghost.pin"
                    }),
                    Action::Pin,
                    false,
                );
            }
        } else {
            card.spawn(label(crate::text::t("ghost.select"), 16.0, MUTED));
        }
        if online.wanted.is_none() {
            if settings.online && standing.and_then(recommended).is_some() {
                action_button(
                    card,
                    crate::text::t("ghost.recommended"),
                    Action::Recommended,
                    false,
                );
            }
            action_button(
                card,
                crate::text::t(if personal.is_some() {
                    "ghost.personal"
                } else {
                    "ghost.first"
                }),
                Action::Personal,
                false,
            );
            if !online.ghost_status.is_empty() {
                card.spawn(label(&online.ghost_status, 14.0, ACCENT));
            }
        }
        if settings.online && online.wanted.is_none() {
            action_button(
                card,
                crate::text::t("ghost.refresh"),
                Action::Refresh,
                false,
            );
        }
        if rival.loaded() {
            card.spawn(label(
                crate::text::tf("ghost.current", &[&rival.name]),
                14.0,
                MUTED,
            ));
        }
    });
}

#[derive(Component)]
pub(super) struct CountryCell;

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_flags(
    browse: Res<Browse>,
    online: Res<Online>,
    mode: Res<Mode>,
    settings: Res<Settings>,
    challenge: Res<crate::challenge::Challenge>,
    flags: Res<Flags>,
    mut images: Query<(&CountryFlag, &mut ImageNode)>,
    mut cells: Query<&mut Node, With<CountryCell>>,
) {
    let visible = !matches!(browse.view, View::Awards | View::Records);
    for mut cell in &mut cells {
        cell.display = if visible {
            Display::Flex
        } else {
            Display::None
        };
    }
    let week = (browse.view == View::Week).then(|| challenge.label());
    let rows = matching_board(&online, &browse, *mode, &settings, &week)
        .map(|s| lines(s, browse.view))
        .unwrap_or_default();
    for (flag, mut image) in &mut images {
        let country = match rows.get(flag.0) {
            Some(Line::Place(p)) => p.country.as_deref().unwrap_or(""),
            _ => "",
        };
        let wanted = flags.image(country);
        if image.image != wanted.image || image.rect != wanted.rect || image.color != wanted.color {
            *image = wanted;
        }
    }
}
