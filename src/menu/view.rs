//! The menu's layout. Circuit thumbnails come from the registered traces, so
//! adding a circuit never needs a second, manually maintained image catalogue.
use super::*;
use crate::ui::{ACCENT, LINE, MUTED, RAISED, SURFACE, TEXT, label};
use bevy::{
    asset::RenderAssetUsages,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

#[derive(Component)]
pub(super) struct Panel;
#[derive(Component)]
pub(super) struct Launcher;
#[derive(Component)]
pub(super) struct Selected;
#[derive(Component)]
pub(super) struct Primary;

#[derive(Resource, Default)]
pub(super) struct Previews {
    images: Vec<Handle<Image>>,
}

pub(super) fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(Previews {
        images: all_circuits()
            .iter()
            .map(|c| images.add(outline(c)))
            .collect(),
    });
    commands.spawn((
        Panel,
        GlobalZIndex(20),
        Node {
            display: Display::None,
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(Color::srgba(0.018, 0.026, 0.032, 0.93)),
    ));
    commands
        .spawn((
            Launcher,
            GlobalZIndex(5),
            Node {
                position_type: PositionType::Absolute,
                bottom: px(24),
                right: px(24),
                column_gap: px(8),
                ..default()
            },
        ))
        .with_children(|row| {
            for (page, title) in [
                (Page::Circuit, "T   Circuits"),
                (Page::Car, "C   Garage & setup"),
            ] {
                row.spawn(button(Action::Open(page))).with_children(|b| {
                    b.spawn(label(title, 15.0, TEXT));
                });
            }
        });
}

fn column() -> Node {
    Node {
        flex_direction: FlexDirection::Column,
        row_gap: px(12),
        min_width: px(0),
        ..default()
    }
}

fn button(action: Action) -> impl Bundle {
    (
        Button,
        action,
        Node {
            padding: UiRect::axes(px(18), px(12)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(8)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            column_gap: px(8),
            ..default()
        },
        BackgroundColor(if matches!(action, Action::Apply) {
            ACCENT
        } else {
            SURFACE
        }),
        BorderColor::all(LINE),
    )
}

fn card(action: Action, selected: bool, node: Node) -> impl Bundle {
    (
        Button,
        action,
        node,
        BackgroundColor(if selected { RAISED } else { SURFACE }),
        BorderColor::all(if selected { ACCENT } else { LINE }),
    )
}

#[allow(clippy::type_complexity)] // Bevy query filters keep the menu's buttons isolated.
pub(super) fn hover(
    mut buttons: Query<
        (
            &Interaction,
            Option<&Selected>,
            Option<&Primary>,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        (With<Action>, Changed<Interaction>),
    >,
) {
    for (interaction, selected, primary, mut background, mut border) in &mut buttons {
        let active = selected.is_some();
        let hover = *interaction != Interaction::None;
        background.0 = if primary.is_some() {
            if hover {
                Color::srgb(0.86, 1.0, 0.54)
            } else {
                ACCENT
            }
        } else if hover || active {
            RAISED
        } else {
            SURFACE
        };
        *border = BorderColor::all(if active || hover || primary.is_some() {
            ACCENT
        } else {
            LINE
        });
    }
}

pub(super) fn show_launcher(halt: Res<Halt>, mut launchers: Query<&mut Node, With<Launcher>>) {
    if halt.is_changed() {
        for mut launcher in &mut launchers {
            launcher.display = if *halt == Halt::Nothing {
                Display::Flex
            } else {
                Display::None
            };
        }
    }
}

pub(super) fn draw(
    mut commands: Commands,
    menu: Res<Menu>,
    track: Res<Track>,
    spec: Res<Spec>,
    previews: Option<Res<Previews>>,
    mut panels: Query<(Entity, &mut Node), With<Panel>>,
) {
    if !menu.is_changed() {
        return;
    }
    let Ok((root, mut node)) = panels.single_mut() else {
        return;
    };
    let Some(page) = menu.page else {
        node.display = Display::None;
        return;
    };
    node.display = Display::Flex;
    let Some(previews) = previews.as_ref() else {
        return;
    };
    let entries = menu.entries();
    let selected = entries.contains(&menu.at);
    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|root| {
        root.spawn((Node {
            width: px(1160), height: px(734), min_height: px(0), padding: UiRect::all(px(28)), border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(18)), row_gap: px(22), ..column()
        }, BackgroundColor(Color::srgb(0.045, 0.06, 0.073)), BorderColor::all(LINE)))
        .with_children(|panel| {
            panel.spawn(Node { align_items: AlignItems::Center, justify_content: JustifyContent::SpaceBetween, ..default() })
                .with_children(|header| {
                    header.spawn(Node { column_gap: px(12), align_items: AlignItems::Center, ..default() }).with_children(|brand| {
                        brand.spawn((Node { width: px(6), height: px(30), border_radius: BorderRadius::all(px(3)), ..default() }, BackgroundColor(ACCENT)));
                        brand.spawn(label("TODORA", 24.0, TEXT));
                        brand.spawn(label("/   FREE DRIVE", 12.0, MUTED));
                    });
                    header.spawn(Node { column_gap: px(8), align_items: AlignItems::Center, ..default() }).with_children(|tabs| {
                        tabs.spawn(label("LB / RB  Tabs", 12.0, MUTED));
                        for (tab, title) in [(Page::Circuit, "01   Circuits"), (Page::Car, "02   Garage & setup")] {
                            let mut tab_button = tabs.spawn(button(Action::Open(tab)));
                            if page == tab { tab_button.insert(Selected); }
                            tab_button.with_children(|b| { b.spawn(label(title, 15.0, if page == tab { ACCENT } else { MUTED })); });
                        }
                        tabs.spawn(button(Action::Back)).with_children(|b| { b.spawn(label("Close  /  Esc", 15.0, MUTED)); });
                    });
                });
            panel.spawn(Node { row_gap: px(5), ..column() }).with_children(|heading| {
                heading.spawn(label(if page == Page::Circuit { "Find your next lap." } else { "Make it your drive." }, 38.0, TEXT));
                heading.spawn(label(if page == Page::Circuit { "Pick a circuit. Learn the corners. Chase your best." }
                    else { "Three different characters. One setup that feels right to you." }, 16.0, MUTED));
            });
            panel.spawn(Node { column_gap: px(24), height: px(428), min_height: px(0), flex_shrink: 0.0, ..default() }).with_children(|body| {
                if page == Page::Circuit { circuits(body, &menu, previews, &entries, &track); }
                else { garage(body, &menu, &spec); }
            });
            panel.spawn((Node {
                padding: UiRect::top(px(18)), border: UiRect::top(px(1)),
                align_items: AlignItems::Center, justify_content: JustifyContent::SpaceBetween, ..default()
            }, BorderColor::all(LINE))).with_children(|footer| {
                footer.spawn(Node { row_gap: px(4), ..column() }).with_children(|hint| {
                    hint.spawn(label(if page == Page::Circuit { "Stick / D-pad / Arrows  Browse     PgUp / PgDn  Page     A / Enter  Drive" }
                        else { "↑↓  Car     ←→  Setup     Y / M  Mode     A / Enter  Apply" }, 13.0, TEXT));
                    hint.spawn(label(if page == Page::Circuit { "Changing circuit starts a new session. Start / B / Esc returns to your current lap." }
                        else { "Apply restarts the lap. Best times and ghosts are saved separately for each mode." }, 12.0, MUTED));
                });
                if selected {
                    footer.spawn((button(Action::Apply), Primary)).with_children(|b| {
                        b.spawn(label(if page == Page::Circuit { "Drive this circuit  →" } else { "Apply & drive  →" }, 17.0, SURFACE));
                    });
                }
            });
        });
    });
}

fn circuits(
    body: &mut ChildSpawnerCommands,
    menu: &Menu,
    previews: &Previews,
    entries: &[usize],
    track: &Track,
) {
    let position = menu.position();
    let first = position / PAGE_SIZE * PAGE_SIZE;
    body.spawn(Node {
        width: px(700),
        row_gap: px(12),
        ..column()
    })
    .with_children(|browser| {
        browser
            .spawn(Node {
                height: px(44),
                column_gap: px(8),
                ..default()
            })
            .with_children(|search| {
                search
                    .spawn((
                        Node {
                            flex_grow: 1.0,
                            flex_basis: px(0),
                            min_width: px(0),
                            overflow: Overflow::clip(),
                            padding: UiRect::axes(px(14), px(10)),
                            border: UiRect::all(px(1)),
                            border_radius: BorderRadius::all(px(8)),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BackgroundColor(SURFACE),
                        BorderColor::all(LINE),
                    ))
                    .with_children(|field| {
                        field.spawn((
                            TextLayout::no_wrap(),
                            label(
                                if menu.search.is_empty() {
                                    "Search circuits… just start typing".into()
                                } else {
                                    format!("Search: {}", menu.search)
                                },
                                16.0,
                                if menu.search.is_empty() { MUTED } else { TEXT },
                            ),
                        ));
                    });
                search.spawn(button(Action::Clear)).with_children(|b| {
                    b.spawn(label("Clear", 14.0, MUTED));
                });
            });
        browser
            .spawn(Node {
                height: px(316),
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(10),
                row_gap: px(10),
                align_content: AlignContent::Start,
                ..default()
            })
            .with_children(|grid| {
                if entries.is_empty() {
                    grid.spawn(Node {
                        padding: UiRect::all(px(28)),
                        ..column()
                    })
                    .with_children(|empty| {
                        empty.spawn(label("No circuits found", 24.0, TEXT));
                        empty.spawn(label(
                            "Try a shorter name or clear your search.",
                            16.0,
                            MUTED,
                        ));
                    });
                }
                for &at in entries.iter().skip(first).take(PAGE_SIZE) {
                    let circuit = &all_circuits()[at];
                    let selected = at == menu.at;
                    let mut item = grid.spawn(card(
                        Action::Select(at),
                        selected,
                        Node {
                            width: px(345),
                            height: px(98),
                            padding: UiRect::all(px(10)),
                            border: UiRect::all(px(1)),
                            border_radius: BorderRadius::all(px(10)),
                            column_gap: px(12),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                    ));
                    if selected {
                        item.insert(Selected);
                    }
                    item.with_children(|card| {
                        card.spawn((
                            ImageNode::new(previews.images[at].clone()),
                            Node {
                                width: px(106),
                                height: px(72),
                                flex_shrink: 0.0,
                                ..default()
                            },
                        ));
                        card.spawn(Node {
                            row_gap: px(5),
                            flex_grow: 1.0,
                            ..column()
                        })
                        .with_children(|info| {
                            info.spawn(label(
                                circuit.name,
                                17.0,
                                if selected { ACCENT } else { TEXT },
                            ));
                            info.spawn(label(
                                if circuit.id == track.circuit().id {
                                    "CURRENT CIRCUIT"
                                } else if selected {
                                    "SELECTED"
                                } else {
                                    "CIRCUIT"
                                },
                                10.0,
                                MUTED,
                            ));
                        });
                    });
                }
            });
        browser
            .spawn(Node {
                height: px(42),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::SpaceBetween,
                ..default()
            })
            .with_children(|pagination| {
                pagination.spawn(label(
                    if entries.is_empty() {
                        "0 circuits".into()
                    } else {
                        format!(
                            "{}–{} of {} circuits  /  Page {} of {}",
                            first + 1,
                            (first + PAGE_SIZE).min(entries.len()),
                            entries.len(),
                            first / PAGE_SIZE + 1,
                            entries.len().div_ceil(PAGE_SIZE)
                        )
                    },
                    13.0,
                    MUTED,
                ));
                pagination
                    .spawn(Node {
                        column_gap: px(8),
                        ..default()
                    })
                    .with_children(|buttons| {
                        for (action, title) in
                            [(Action::Previous, "← Previous"), (Action::Next, "Next →")]
                        {
                            buttons.spawn(button(action)).with_children(|b| {
                                b.spawn(label(title, 13.0, TEXT));
                            });
                        }
                    });
            });
    });
    body.spawn((
        Node {
            width: px(378),
            padding: UiRect::all(px(22)),
            border_radius: BorderRadius::all(px(12)),
            row_gap: px(12),
            ..column()
        },
        BackgroundColor(SURFACE),
    ))
    .with_children(|detail| {
        detail.spawn(label("CIRCUIT BRIEF", 11.0, ACCENT));
        if !entries.contains(&menu.at) {
            detail.spawn(label("A new favourite is out there.", 27.0, TEXT));
            detail.spawn(label(
                "Search the circuit library to find your next drive.",
                16.0,
                MUTED,
            ));
            return;
        }
        let circuit = &all_circuits()[menu.at];
        detail.spawn(label(circuit.name, 29.0, TEXT));
        detail.spawn((
            ImageNode::new(previews.images[menu.at].clone()),
            Node {
                width: percent(100),
                height: px(168),
                ..default()
            },
        ));
        detail.spawn(label("CIRCUIT OUTLINE   /   START IN LIME", 10.0, MUTED));
        detail
            .spawn(Node {
                column_gap: px(28),
                ..default()
            })
            .with_children(|stats| {
                for (name, value) in [
                    ("IN-GAME LAP", format!("{:.0} m", circuit.lap)),
                    ("SESSION", "Free drive".into()),
                ] {
                    stats
                        .spawn(Node {
                            row_gap: px(5),
                            ..column()
                        })
                        .with_children(|stat| {
                            stat.spawn(label(name, 10.0, MUTED));
                            stat.spawn(label(value, 24.0, TEXT));
                        });
                }
            });
        detail.spawn(label(
            "Your best lap becomes the ghost to beat.",
            14.0,
            MUTED,
        ));
    });
}

fn character(spec: Spec) -> (&'static str, &'static str) {
    match spec {
        Spec::Tourer => (
            "THE ALL-ROUNDER",
            "A composed balance of grip and straight-line pace.",
        ),
        Spec::Clubman => (
            "THE CORNER CARVER",
            "More grip. More confidence through every bend.",
        ),
        Spec::Express => (
            "THE STRAIGHT-LINE SPECIALIST",
            "Long legs on the straights. A lighter hold in the corners.",
        ),
    }
}

fn garage(body: &mut ChildSpawnerCommands, menu: &Menu, current: &Spec) {
    body.spawn(Node {
        width: px(650),
        row_gap: px(10),
        ..column()
    })
    .with_children(|cars| {
        for (at, spec) in Spec::ALL.into_iter().enumerate() {
            let selected = menu.at == at;
            let sheet = spec.sheet();
            let (tag, description) = character(spec);
            let mut car = cars.spawn(card(
                Action::Select(at),
                selected,
                Node {
                    height: px(136),
                    padding: UiRect::all(px(18)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(12)),
                    column_gap: px(18),
                    align_items: AlignItems::Center,
                    ..default()
                },
            ));
            if selected {
                car.insert(Selected);
            }
            car.with_children(|card| {
                card.spawn((
                    Node {
                        width: px(64),
                        height: px(94),
                        border_radius: BorderRadius::all(px(9)),
                        flex_shrink: 0.0,
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(sheet.paint),
                ))
                .with_children(|paint| {
                    paint.spawn(label(format!("0{}", at + 1), 28.0, TEXT));
                });
                card.spawn(Node {
                    flex_grow: 1.0,
                    row_gap: px(6),
                    ..column()
                })
                .with_children(|info| {
                    info.spawn(Node {
                        column_gap: px(14),
                        align_items: AlignItems::Center,
                        ..default()
                    })
                    .with_children(|title| {
                        title.spawn(label(
                            sheet.name,
                            26.0,
                            if selected { ACCENT } else { TEXT },
                        ));
                        title.spawn(label(
                            if spec == *current { "DRIVING" } else { tag },
                            10.0,
                            MUTED,
                        ));
                    });
                    info.spawn(label(description, 14.0, MUTED));
                    info.spawn(Node {
                        column_gap: px(18),
                        margin: UiRect::top(px(6)),
                        ..default()
                    })
                    .with_children(|ratings| {
                        for (name, value) in sheet.stars.rows() {
                            ratings
                                .spawn(Node {
                                    row_gap: px(5),
                                    ..column()
                                })
                                .with_children(|rating| {
                                    rating.spawn(label(name, 10.0, MUTED));
                                    rating
                                        .spawn(Node {
                                            column_gap: px(4),
                                            ..default()
                                        })
                                        .with_children(|bars| {
                                            for i in 0..5 {
                                                bars.spawn((
                                                    Node {
                                                        width: px(22),
                                                        height: px(4),
                                                        border_radius: BorderRadius::all(px(2)),
                                                        ..default()
                                                    },
                                                    BackgroundColor(if i < value {
                                                        if selected { ACCENT } else { TEXT }
                                                    } else {
                                                        LINE
                                                    }),
                                                ));
                                            }
                                        });
                                });
                        }
                    });
                });
            });
        }
    });
    body.spawn((
        Node {
            width: px(428),
            padding: UiRect::all(px(22)),
            row_gap: px(10),
            border_radius: BorderRadius::all(px(12)),
            ..column()
        },
        BackgroundColor(SURFACE),
    ))
    .with_children(|setup| {
        setup.spawn(label("DRIVING MODE", 11.0, ACCENT));
        setup
            .spawn(Node {
                column_gap: px(6),
                ..default()
            })
            .with_children(|modes| {
                for mode in Mode::ALL {
                    let selected = menu.mode == mode;
                    let mut choice = modes.spawn(card(
                        Action::Mode(mode),
                        selected,
                        Node {
                            flex_basis: percent(0),
                            flex_grow: 1.0,
                            padding: UiRect::axes(px(8), px(10)),
                            border: UiRect::all(px(1)),
                            border_radius: BorderRadius::all(px(8)),
                            align_items: AlignItems::Center,
                            row_gap: px(3),
                            ..column()
                        },
                    ));
                    if selected {
                        choice.insert(Selected);
                    }
                    choice.with_children(|choice| {
                        choice.spawn(label(
                            mode.name(),
                            15.0,
                            if selected { ACCENT } else { TEXT },
                        ));
                        choice.spawn(label(
                            match mode {
                                Mode::Beginner => "−20% speed",
                                Mode::Regular => "100% speed",
                                Mode::Pro => "+20% speed",
                            },
                            12.0,
                            MUTED,
                        ));
                    });
                }
            });
        setup.spawn(label("HANDLING SETUP", 11.0, ACCENT));
        for (at, wanted) in Setup::ALL.into_iter().enumerate() {
            let (title, hint) = match wanted {
                Setup::Understeer => ("Stable", "Gentler rotation. Easier to catch."),
                Setup::Balanced => ("Balanced", "The car’s natural handling."),
                Setup::Oversteer => ("Loose", "Eager rotation. Holds a slide longer."),
            };
            let selected = menu.setup == wanted;
            let mut preset = setup.spawn(card(
                Action::Tune(wanted),
                selected,
                Node {
                    padding: UiRect::axes(px(14), px(10)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(8)),
                    column_gap: px(14),
                    align_items: AlignItems::Center,
                    ..default()
                },
            ));
            if selected {
                preset.insert(Selected);
            }
            preset.with_children(|preset| {
                preset.spawn(label(
                    format!("{}", at + 1),
                    21.0,
                    if selected { ACCENT } else { MUTED },
                ));
                preset
                    .spawn(Node {
                        row_gap: px(3),
                        ..column()
                    })
                    .with_children(|text| {
                        text.spawn(label(title, 18.0, if selected { ACCENT } else { TEXT }));
                        text.spawn(label(hint, 13.0, MUTED));
                    });
            });
        }
        setup.spawn(label(
            "1 / 2 / 3 changes handling while driving.",
            12.0,
            MUTED,
        ));
    });
}

fn outline(circuit: &crate::track::Circuit) -> Image {
    const W: usize = 512;
    const H: usize = 320;
    let mut image = Image::new_fill(
        Extent3d {
            width: W as u32,
            height: H as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let points: Vec<Vec2> = circuit
        .centreline
        .iter()
        .map(|p| Vec2::new(p[0], p[2]))
        .collect();
    let min = points
        .iter()
        .copied()
        .fold(Vec2::splat(f32::INFINITY), Vec2::min);
    let max = points
        .iter()
        .copied()
        .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
    let scale = ((W as f32 - 52.0) / (max.x - min.x).max(1.0))
        .min((H as f32 - 52.0) / (max.y - min.y).max(1.0));
    let centre = (min + max) * 0.5;
    let points: Vec<Vec2> = points
        .iter()
        .map(|p| (*p - centre) * scale + Vec2::new(W as f32, H as f32) * 0.5)
        .collect();
    let data = image.data.as_mut().unwrap();
    // Antialiased capsule strokes, rasterised only in each segment's bounds.
    let mut stroke = |a: Vec2, b: Vec2, radius: f32, color: [u8; 3]| {
        let lo = (a.min(b) - Vec2::splat(radius + 1.0)).max(Vec2::ZERO);
        let hi =
            (a.max(b) + Vec2::splat(radius + 1.0)).min(Vec2::new(W as f32 - 1.0, H as f32 - 1.0));
        let ab = b - a;
        for y in lo.y as usize..=hi.y as usize {
            for x in lo.x as usize..=hi.x as usize {
                let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let t = ((p - a).dot(ab) / ab.length_squared().max(0.001)).clamp(0.0, 1.0);
                let alpha = ((radius + 0.5 - p.distance(a + t * ab)).clamp(0.0, 1.0) * 255.0) as u8;
                let offset = (y * W + x) * 4;
                if alpha > 0 && alpha >= data[offset + 3] {
                    data[offset..offset + 4]
                        .copy_from_slice(&[color[0], color[1], color[2], alpha]);
                }
            }
        }
    };
    for (a, b) in points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
    {
        stroke(*a, *b, 3.2, [196, 230, 213]);
    }
    if let Some(start) = points.first() {
        stroke(*start, *start, 8.0, [197, 245, 92]);
    }
    image
}
