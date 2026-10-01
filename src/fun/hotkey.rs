//! F9: Bonkers mode on or off, from anywhere you can drive.
//!
//! A Bonkers lap never counts, so getting back to one that does should not take
//! the pause menu and a tab. F9 switches between Bonkers and the plain game and
//! says which it is now. It changes the same switch the title's button and the
//! Bonkers tab of the settings do.

use bevy::prelude::*;

use crate::pause::Halt;
use crate::settings::Settings;
use crate::text::t;

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
        (switch.run_if(|fun: Res<super::Together>| !fun.0), fade).chain(),
    );
}

fn switch(
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
    settings.bonkers = !settings.bonkers;
    for notice in &old {
        commands.entity(notice).despawn();
    }
    let (words, colour) = if settings.bonkers {
        (t("fun.on"), crate::ui::Palette::STANDARD.purple)
    } else {
        (t("fun.off"), crate::ui::TEXT)
    };
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
            // Purple for Bonkers, the way the lap readout is, so the colour says
            // a lap will not count before the words do.
            notice.spawn((
                Text::new(words),
                TextFont {
                    font_size: FontSize::Px(34.0),
                    weight: FontWeight::BLACK,
                    ..default()
                },
                TextColor(colour),
            ));
        });
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

    fn app(halt: Halt, together: bool) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ButtonInput<KeyCode>>()
            .insert_resource(halt)
            .insert_resource(super::super::Together(together))
            .insert_resource(Settings::default());
        plugin(&mut app);
        app.update();
        app
    }

    /// One press of F9, and let go: nothing else in a test app clears what was
    /// just pressed, and a key that stays just pressed is pressed every frame.
    fn press(app: &mut App) {
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F9);
        app.update();
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(KeyCode::F9);
        keys.clear();
    }

    #[test]
    fn a_press_switches_bonkers_and_puts_a_notice_up_that_goes_away() {
        let mut app = app(Halt::Nothing, false);
        let on = |app: &App| app.world().resource::<Settings>().bonkers;
        assert!(!on(&app), "the plain game to begin with");
        press(&mut app);
        assert!(on(&app));
        let notices = |app: &mut App| app.world_mut().query::<&Notice>().iter(app.world()).count();
        assert_eq!(notices(&mut app), 1);
        // Pressing again switches back and replaces the notice rather than
        // piling them up.
        press(&mut app);
        assert!(!on(&app), "and back to the plain game");
        app.update();
        assert_eq!(notices(&mut app), 1);
        // And it goes away on its own.
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_secs_f32(SHOWN / 8.0),
        ));
        for _ in 0..12 {
            app.update();
        }
        assert_eq!(notices(&mut app), 0, "the notice stayed up");
    }

    #[test]
    fn a_menu_in_front_or_a_friend_beside_leaves_the_switch_alone() {
        for (halt, together) in [
            (Halt::Settings, false),
            (Halt::Title, false),
            (Halt::Nothing, true),
        ] {
            let mut app = app(halt, together);
            press(&mut app);
            assert!(
                !app.world().resource::<Settings>().bonkers,
                "{halt:?} together={together}"
            );
        }
    }
}
