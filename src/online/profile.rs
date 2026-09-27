//! An explicit draft: typing never changes a public profile until Save succeeds.
use super::{
    client::{Ask, Client},
    flags::Flags,
};
use crate::hud::FRONT;
use crate::identity::countries;
use crate::{
    pause::Halt,
    settings::Settings,
    ui::{ACCENT, LINE, MUTED, SURFACE, TEXT, label},
};
use bevy::{
    input::keyboard::{Key, KeyboardInput},
    prelude::*,
};

#[derive(Message)]
pub(crate) struct OpenProfile(pub Halt);

#[derive(Resource, Default)]
pub(crate) struct Profile {
    pub name: String,
    pub country: String,
    online: bool,
    query: String,
    field: usize,
    country_at: usize,
    select_all: bool,
    pub saving: bool,
    pub error: String,
    pub return_to: Halt,
}

impl Profile {
    fn countries(&self) -> Vec<(&'static str, &'static str)> {
        let query = self.query.trim().to_lowercase();
        std::iter::once(("", crate::text::t("profile.no_country")))
            .chain(countries::ALL.iter().copied())
            .filter(|(code, name)| {
                code.to_lowercase().contains(&query) || name.to_lowercase().contains(&query)
            })
            .collect()
    }

    fn validation(&self, settings: &Settings) -> Result<String, &'static str> {
        // A server-confirmed owner may keep a reserved name when editing country.
        if self.name.trim() == settings.name && !self.name.is_empty() {
            Ok(self.name.trim().into())
        } else {
            crate::verify::valid_name(&self.name)
        }
    }
}

#[derive(Component)]
struct Panel;
#[derive(Component, Clone, Copy)]
enum Action {
    Field(usize),
    Country(usize),
    Shuffle,
    Save,
    Cancel,
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Profile>()
        .add_message::<OpenProfile>()
        .add_systems(Startup, setup)
        .add_systems(
            PreUpdate,
            input
                .after(crate::pause::HaltSet)
                .after(bevy::ui::UiSystems::Focus),
        )
        .add_systems(Update, (open, draw).chain());
}

fn setup(mut commands: Commands) {
    commands.spawn((
        Panel,
        GlobalZIndex(25),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            display: Display::None,
            ..default()
        },
        BackgroundColor(Color::srgba(0.018, 0.026, 0.032, 0.94)),
    ));
}

fn open(
    mut asked: MessageReader<OpenProfile>,
    settings: Res<Settings>,
    mut profile: ResMut<Profile>,
    mut halt: ResMut<Halt>,
) {
    let Some(OpenProfile(from)) = asked.read().last() else {
        return;
    };
    *profile = Profile {
        name: if settings.name.is_empty() {
            super::suggest_name(super::random_index())
        } else {
            settings.name.clone()
        },
        country: settings.country.clone(),
        online: settings.online || !settings.online_asked,
        select_all: true,
        return_to: *from,
        ..default()
    };
    profile.country_at = profile
        .countries()
        .iter()
        .position(|(code, _)| *code == profile.country)
        .unwrap_or(0);
    *halt = Halt::Profile;
}

#[allow(clippy::too_many_arguments)]
fn input(
    mut profile: ResMut<Profile>,
    mut halt: ResMut<Halt>,
    mut settings: ResMut<Settings>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut typed: MessageReader<KeyboardInput>,
    clicks: Query<(&Action, &Interaction), Changed<Interaction>>,
    client: Option<Res<Client>>,
) {
    if *halt != Halt::Profile || halt.is_changed() {
        typed.clear();
        return;
    }
    if profile.saving {
        typed.clear();
        return;
    }
    let pad = |b| pads.iter().any(|p| p.just_pressed(b));
    let mut action = None;
    for (a, i) in &clicks {
        if *i == Interaction::Pressed {
            action = Some(*a);
        }
    }
    if keys.just_pressed(KeyCode::Escape) || pad(GamepadButton::East) {
        action = Some(Action::Cancel);
    }
    if keys.just_pressed(KeyCode::Tab) || pad(GamepadButton::RightTrigger) {
        profile.field = (profile.field + 1) % 5;
        profile.select_all = true;
    }
    if pad(GamepadButton::LeftTrigger) {
        profile.field = (profile.field + 4) % 5;
        profile.select_all = true;
    }
    if pad(GamepadButton::North) {
        action = Some(Action::Shuffle);
    }
    let select_all = keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight)
        || keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight);
    if select_all && keys.just_pressed(KeyCode::KeyA) {
        profile.select_all = true;
    }
    for event in typed.read() {
        if !event.state.is_pressed() || select_all || profile.field > 1 {
            continue;
        }
        let field = profile.field;
        let mut value = if field == 0 {
            profile.name.clone()
        } else {
            profile.query.clone()
        };
        match &event.logical_key {
            Key::Backspace => {
                if profile.select_all {
                    value.clear();
                } else {
                    value.pop();
                }
            }
            Key::Character(chars) => {
                if profile.select_all {
                    value.clear();
                }
                for c in chars
                    .chars()
                    .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
                {
                    if value.chars().count() < if field == 0 { 16 } else { 48 } {
                        value.push(c);
                    }
                }
            }
            Key::Space => {
                if profile.select_all {
                    value.clear();
                }
                if value.chars().count() < if field == 0 { 16 } else { 48 } {
                    value.push(' ');
                }
            }
            _ => continue,
        }
        profile.select_all = false;
        profile.error.clear();
        if field == 0 {
            profile.name = value;
        } else {
            profile.query = value;
            profile.country_at = 0;
        }
    }
    let vertical = 6
        * (i32::from(keys.just_pressed(KeyCode::PageDown))
            - i32::from(keys.just_pressed(KeyCode::PageUp)))
        + i32::from(keys.just_pressed(KeyCode::ArrowDown) || pad(GamepadButton::DPadDown))
        - i32::from(keys.just_pressed(KeyCode::ArrowUp) || pad(GamepadButton::DPadUp));
    if vertical != 0 {
        if profile.field == 1 {
            profile.country_at = (profile.country_at as i32 + vertical)
                .clamp(0, profile.countries().len().saturating_sub(1) as i32)
                as usize;
        } else {
            profile.field = (profile.field as i32 + vertical).rem_euclid(5) as usize;
        }
    }
    if keys.just_pressed(KeyCode::Enter) || pad(GamepadButton::South) {
        action = Some(match profile.field {
            0 => Action::Field(1),
            1 => Action::Country(profile.country_at),
            2 => Action::Field(2),
            3 => Action::Save,
            _ => Action::Cancel,
        });
    }
    match action {
        Some(Action::Field(field)) => {
            profile.field = field;
            profile.select_all = true;
            if field == 2 {
                profile.online = !profile.online;
            }
        }
        Some(Action::Country(at)) => {
            if let Some((code, _)) = profile.countries().get(at) {
                profile.country = (*code).into();
                profile.country_at = at;
            }
        }
        Some(Action::Shuffle) => {
            profile.name = super::suggest_name(super::random_index());
            profile.select_all = true;
            profile.error.clear();
        }
        Some(Action::Cancel) => {
            if profile.return_to == Halt::Nothing {
                settings.online_asked = true;
            }
            *halt = profile.return_to;
        }
        Some(Action::Save) => match profile.validation(&settings) {
            Err(why) => profile.error = why.into(),
            Ok(name) => {
                if profile.online {
                    if let Some(client) = client {
                        profile.saving = true;
                        profile.error.clear();
                        client.ask(Ask::SaveProfile {
                            name,
                            country: (!profile.country.is_empty()).then(|| profile.country.clone()),
                        });
                    } else {
                        profile.error = crate::text::t("profile.unavailable").into();
                    }
                } else {
                    settings.name = name;
                    settings.country = profile.country.clone();
                    settings.online = false;
                    settings.online_asked = true;
                    *halt = profile.return_to;
                }
            }
        },
        None => {}
    }
}

fn button(
    parent: &mut ChildSpawnerCommands,
    text: impl Into<String>,
    action: Action,
    active: bool,
) {
    let filled = active && !matches!(action, Action::Field(0 | 1));
    parent
        .spawn((
            Button,
            action,
            Node {
                padding: UiRect::axes(px(14), px(10)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(8)),
                ..default()
            },
            BackgroundColor(if filled { ACCENT } else { SURFACE }),
            BorderColor::all(if active { ACCENT } else { LINE }),
        ))
        .with_children(|b| {
            b.spawn(label(text, 16.0, if filled { FRONT } else { TEXT }));
        });
}

fn draw(
    mut commands: Commands,
    halt: Res<Halt>,
    profile: Res<Profile>,
    settings: Res<Settings>,
    flags: Res<Flags>,
    mut panel: Query<(Entity, &mut Node), With<Panel>>,
) {
    let Ok((root, mut node)) = panel.single_mut() else {
        return;
    };
    node.display = if *halt == Halt::Profile {
        Display::Flex
    } else {
        Display::None
    };
    if *halt != Halt::Profile
        || !(profile.is_changed() || halt.is_changed() || settings.is_changed())
    {
        return;
    }
    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|root| {
        root.spawn((
            Node {
                width: px(940),
                padding: UiRect::all(px(32)),
                flex_direction: FlexDirection::Column,
                row_gap: px(20),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(18)),
                ..default()
            },
            BackgroundColor(FRONT),
            BorderColor::all(LINE),
        ))
        .with_children(|panel| {
            panel.spawn(label(crate::text::t("profile.title"), 30.0, TEXT));
            panel.spawn(label(crate::text::t("profile.about"), 16.0, MUTED));
            panel
                .spawn(Node {
                    column_gap: px(32),
                    ..default()
                })
                .with_children(|body| {
                    body.spawn(Node {
                        width: px(490),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(10),
                        ..default()
                    })
                    .with_children(|form| {
                        form.spawn(label(crate::text::t("profile.name"), 13.0, MUTED));
                        button(
                            form,
                            format!(
                                "{}{}",
                                profile.name,
                                if profile.field == 0 { " |" } else { "" }
                            ),
                            Action::Field(0),
                            profile.field == 0,
                        );
                        form.spawn(label(crate::text::t("profile.name_help"), 13.0, MUTED));
                        form.spawn(label(crate::text::t("profile.country_search"), 13.0, MUTED));
                        button(
                            form,
                            if profile.query.is_empty() {
                                crate::text::t("profile.search").into()
                            } else {
                                profile.query.clone()
                            },
                            Action::Field(1),
                            profile.field == 1,
                        );
                        let countries = profile.countries();
                        let first = profile
                            .country_at
                            .saturating_sub(2)
                            .min(countries.len().saturating_sub(6));
                        form.spawn(Node {
                            height: px(222),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(3),
                            ..default()
                        })
                        .with_children(|list| {
                            for (i, (code, name)) in
                                countries.iter().enumerate().skip(first).take(6)
                            {
                                let active = profile.country_at == i;
                                list.spawn((
                                    Button,
                                    Action::Country(i),
                                    Node {
                                        height: px(34),
                                        padding: UiRect::axes(px(10), px(5)),
                                        column_gap: px(12),
                                        align_items: AlignItems::Center,
                                        border_radius: BorderRadius::all(px(5)),
                                        ..default()
                                    },
                                    BackgroundColor(if active { SURFACE } else { Color::NONE }),
                                ))
                                .with_children(|row| {
                                    row.spawn((
                                        flags.image(code),
                                        Node {
                                            width: px(28),
                                            height: px(21),
                                            ..default()
                                        },
                                    ));
                                    row.spawn(label(
                                        *name,
                                        15.0,
                                        if *code == profile.country {
                                            ACCENT
                                        } else {
                                            TEXT
                                        },
                                    ));
                                });
                            }
                            if countries.is_empty() {
                                list.spawn(label(crate::text::t("menu.none_found"), 16.0, MUTED));
                            }
                        });
                    });
                    body.spawn((
                        Node {
                            width: px(320),
                            padding: UiRect::all(px(24)),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(18),
                            border_radius: BorderRadius::all(px(14)),
                            ..default()
                        },
                        BackgroundColor(SURFACE),
                    ))
                    .with_children(|card| {
                        card.spawn(label(crate::text::t("profile.preview"), 12.0, ACCENT));
                        card.spawn((
                            flags.image(&profile.country),
                            Node {
                                width: px(80),
                                height: px(60),
                                ..default()
                            },
                        ));
                        card.spawn(label(&profile.name, 26.0, TEXT));
                        card.spawn(label(
                            if profile.country.is_empty() {
                                crate::text::t("profile.no_country")
                            } else {
                                countries::name(&profile.country)
                            },
                            16.0,
                            MUTED,
                        ));
                        button(
                            card,
                            crate::text::t(if profile.online {
                                "profile.online"
                            } else {
                                "profile.offline"
                            }),
                            Action::Field(2),
                            profile.field == 2,
                        );
                        card.spawn(label(crate::text::t("profile.online_help"), 14.0, MUTED));
                        button(
                            card,
                            crate::text::t("profile.shuffle"),
                            Action::Shuffle,
                            false,
                        );
                    });
                });
            let error = if !profile.error.is_empty() {
                profile.error.as_str()
            } else {
                profile.validation(&settings).err().unwrap_or("")
            };
            panel.spawn(label(
                if profile.saving {
                    crate::text::t("profile.saving")
                } else {
                    error
                },
                15.0,
                ACCENT,
            ));
            panel
                .spawn(Node {
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|footer| {
                    footer.spawn(label(crate::text::t("profile.keys"), 13.0, MUTED));
                    footer
                        .spawn(Node {
                            column_gap: px(10),
                            ..default()
                        })
                        .with_children(|actions| {
                            button(
                                actions,
                                crate::text::t("profile.cancel"),
                                Action::Cancel,
                                profile.field == 4,
                            );
                            button(actions, crate::text::t("profile.save"), Action::Save, true);
                        });
                });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(Settings {
                name: "Original".into(),
                country: "AT".into(),
                ..default()
            })
            .insert_resource(Halt::Profile)
            .insert_resource(Profile {
                name: "New Driver".into(),
                country: "DK".into(),
                return_to: Halt::Board,
                ..default()
            })
            .add_message::<KeyboardInput>()
            .add_systems(Update, input);
        app.update();
        app
    }
    #[test]
    fn cancel_discards_the_draft_and_save_is_explicit() {
        let mut app = app();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert_eq!(app.world().resource::<Settings>().name, "Original");
        assert_eq!(*app.world().resource::<Halt>(), Halt::Board);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        *app.world_mut().resource_mut::<Halt>() = Halt::Profile;
        app.update();
        app.world_mut().resource_mut::<Profile>().field = 3;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();
        assert_eq!(app.world().resource::<Settings>().name, "New Driver");
        assert_eq!(app.world().resource::<Settings>().country, "DK");
        assert_eq!(*app.world().resource::<Halt>(), Halt::Board);
    }
    #[test]
    fn cancelling_the_first_online_offer_does_not_offer_again() {
        let mut app = app();
        app.world_mut().resource_mut::<Profile>().return_to = Halt::Nothing;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert_eq!(*app.world().resource::<Halt>(), Halt::Nothing);
        assert!(app.world().resource::<Settings>().online_asked);
        assert!(!app.world().resource::<Settings>().online);
        assert_eq!(app.world().resource::<Settings>().name, "Original");
    }
    #[test]
    fn a_reserved_name_never_reaches_save_and_country_search_covers_the_catalogue() {
        let mut app = app();
        {
            let mut p = app.world_mut().resource_mut::<Profile>();
            p.name = "D.H.H".into();
            p.field = 3;
        }
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Enter);
        app.update();
        assert_eq!(app.world().resource::<Settings>().name, "Original");
        assert!(!app.world().resource::<Profile>().error.is_empty());
        let p = Profile {
            query: "nepal".into(),
            ..default()
        };
        assert_eq!(p.countries(), vec![("NP", "Nepal")]);
        assert_eq!(Profile::default().countries().len(), 250);
    }
}
