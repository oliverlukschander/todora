//! F9: how silly, from anywhere you can drive.
//!
//! A Bonkers lap never counts, so getting back to one that does should not take
//! the pause menu and a tab. F9 steps the silliness round, Serious to Silly to
//! Bonkers and back to Serious, and says where it landed. It changes the same
//! setting the Fun tab does, so it is saved the same way.

use bevy::prelude::*;

use super::Silliness;
use crate::pause::Halt;
use crate::settings::Settings;
use crate::text::{t, tf};

/// Seconds the notice stays up, the last part of which it spends fading.
const SHOWN: f32 = 1.8;
const FADE: f32 = 0.4;

#[derive(Component)]
struct Notice {
    age: f32,
}

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (cycle.run_if(|fun: Res<super::Together>| !fun.0), fade).chain(),
    );
}

/// The level after `level`, and the first one again after the last.
pub(super) fn next(level: Silliness) -> Silliness {
    let all = Silliness::ALL;
    let at = all.iter().position(|l| *l == level).unwrap_or(0);
    all[(at + 1) % all.len()]
}

fn cycle(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    halt: Res<Halt>,
    settings: Option<ResMut<Settings>>,
    old: Query<Entity, With<Notice>>,
) {
    if !keys.just_pressed(KeyCode::F9) || !matches!(*halt, Halt::Nothing | Halt::Pause) {
        return;
    }
    let Some(mut settings) = settings else {
        return;
    };
    settings.silliness = next(settings.silliness);
    for notice in &old {
        commands.entity(notice).despawn();
    }
    let words = tf("fun.now", &[&t(settings.silliness.key())]);
    let size = 34.0;
    commands
        .spawn((
            Notice { age: 0.0 },
            GlobalZIndex(40),
            // Top left, under the name of the circuit: the top middle is where the
            // score is, and the banners, and the toasts come.
            Node {
                position_type: PositionType::Absolute,
                top: px(108),
                left: px(24),
                ..default()
            },
            Pickable::IGNORE,
        ))
        .with_children(|notice| {
            notice.spawn((
                Text::new(words),
                TextFont {
                    font_size: FontSize::Px(size),
                    weight: FontWeight::BLACK,
                    ..default()
                },
                TextColor(colour(settings.silliness)),
            ));
        });
}

/// Purple for Bonkers, the way the lap readout is, so the colour says a lap
/// will not count before the words do.
fn colour(level: Silliness) -> Color {
    match level {
        Silliness::Bonkers => Color::srgb(0.78, 0.45, 1.0),
        Silliness::Silly => Color::srgb(1.0, 0.82, 0.18),
        Silliness::Serious => crate::ui::TEXT,
    }
}

fn fade(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut notices: Query<(Entity, &mut Notice, &Children)>,
    mut words: Query<&mut TextColor>,
) {
    for (entity, mut notice, children) in &mut notices {
        notice.age += time.delta_secs();
        if notice.age >= SHOWN {
            commands.entity(entity).despawn();
            continue;
        }
        let alpha = ((SHOWN - notice.age) / FADE).clamp(0.0, 1.0);
        for child in children.iter() {
            if let Ok(mut colour) = words.get_mut(child) {
                colour.0 = colour.0.with_alpha(alpha);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_levels_go_round_and_come_back() {
        assert_eq!(next(Silliness::Serious), Silliness::Silly);
        assert_eq!(next(Silliness::Silly), Silliness::Bonkers);
        assert_eq!(next(Silliness::Bonkers), Silliness::Serious);
        let mut level = Silliness::Serious;
        for _ in Silliness::ALL {
            level = next(level);
        }
        assert_eq!(
            level,
            Silliness::Serious,
            "one lap of the dial is all of it"
        );
    }

    #[test]
    fn a_press_changes_the_setting_and_puts_a_notice_up_that_goes_away() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(Halt::Nothing)
            .insert_resource(super::super::Together(false))
            .insert_resource(Settings {
                silliness: Silliness::Serious,
                ..Settings::default()
            });
        plugin(&mut app);
        app.update();
        assert_eq!(
            app.world().resource::<Settings>().silliness,
            Silliness::Serious
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F9);
        app.update();
        assert_eq!(
            app.world().resource::<Settings>().silliness,
            Silliness::Silly
        );
        let notices = |app: &mut App| app.world_mut().query::<&Notice>().iter(app.world()).count();
        assert_eq!(notices(&mut app), 1);
        // Pressing again replaces the notice rather than piling them up.
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::F9);
            keys.clear();
            keys.press(KeyCode::F9);
        }
        app.update();
        assert_eq!(
            app.world().resource::<Settings>().silliness,
            Silliness::Bonkers
        );
        app.update();
        assert_eq!(notices(&mut app), 1);
    }

    #[test]
    fn a_menu_in_front_or_a_friend_beside_leaves_the_dial_alone() {
        for (halt, together) in [
            (Halt::Settings, false),
            (Halt::Title, false),
            (Halt::Nothing, true),
        ] {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins)
                .init_resource::<ButtonInput<KeyCode>>()
                .insert_resource(halt)
                .insert_resource(super::super::Together(together))
                .insert_resource(Settings {
                    silliness: Silliness::Serious,
                    ..Settings::default()
                });
            plugin(&mut app);
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::F9);
            app.update();
            assert_eq!(
                app.world().resource::<Settings>().silliness,
                Silliness::Serious,
                "{halt:?} together={together}"
            );
        }
    }
}
