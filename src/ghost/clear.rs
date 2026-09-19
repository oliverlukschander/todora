use bevy::prelude::*;

use super::{Ghost, Recording, store};
use crate::{Reset, lap::LapTimer, pause::Halt};

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResetGhosts {
    Current,
    All,
}

#[derive(Component)]
pub(crate) struct Notice;

#[derive(Resource, Default)]
pub(super) struct Confirmation(Option<ResetGhosts>);

pub(super) fn buttons(
    halt: Res<Halt>,
    buttons: Query<(&ResetGhosts, &Interaction), Changed<Interaction>>,
    mut notices: Query<&mut Text, With<Notice>>,
    mut confirmation: ResMut<Confirmation>,
    mut ghost: ResMut<Ghost>,
    mut timer: ResMut<LapTimer>,
    mut reset: MessageWriter<Reset>,
) {
    if *halt != Halt::Pause {
        confirmation.0 = None;
        for mut text in &mut notices {
            text.0 = "Reset removes saved best times and restarts the lap.".into();
        }
        return;
    }
    for (&scope, interaction) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let message = if confirmation.0 != Some(scope) {
            confirmation.0 = Some(scope);
            match scope {
                ResetGhosts::Current => {
                    "Click again to erase this circuit/mode’s ghost and best time. Your lap will restart."
                }
                ResetGhosts::All => {
                    "Click again to erase every circuit/mode’s ghost and best time. Your lap will restart."
                }
            }
        } else {
            confirmation.0 = None;
            // Remove the active file first. Even if another file fails during
            // an all-reset, the current board must agree with what is on disk.
            match ghost.saved.as_ref().map_or(Ok(()), |saved| saved.remove()) {
                Err(error) => {
                    warn!("Cannot reset this ghost: {error}");
                    "Could not reset this ghost. Check save-folder permissions and try again."
                }
                Ok(()) => {
                    ghost.best = None;
                    ghost.recording = Recording::default();
                    ghost.delta = None;
                    *timer = LapTimer::default();
                    reset.write(Reset);
                    if scope == ResetGhosts::All {
                        match store::remove_all() {
                            Ok(()) => "All ghosts and best times reset. Ready for a fresh lap.",
                            Err(error) => {
                                warn!("Cannot reset all ghosts: {error}");
                                "This ghost was reset, but some saved ghosts could not be removed. Check save-folder permissions and retry."
                            }
                        }
                    } else {
                        "This ghost and best time reset. Ready for a fresh lap."
                    }
                }
            }
        };
        for mut text in &mut notices {
            text.0 = message.into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_needs_two_clicks_and_clears_the_board_and_partial_recording() {
        let mut app = App::new();
        let car = app.world_mut().spawn_empty().id();
        let mut recording = Recording::default();
        recording.push(4.0, 0.2, &Transform::default());
        app.insert_resource(Ghost {
            on: true,
            delta: Some(1.0),
            best: Some(Recording::default()),
            recording,
            car,
            saved: None,
        })
        .insert_resource(Halt::Pause)
        .init_resource::<Confirmation>()
        .init_resource::<LapTimer>()
        .add_message::<Reset>()
        .add_systems(Update, buttons);
        app.world_mut().resource_mut::<LapTimer>().remember(42.0);
        let button = app
            .world_mut()
            .spawn((ResetGhosts::Current, Interaction::Pressed))
            .id();
        app.update();
        assert!(app.world().resource::<Ghost>().best.is_some());
        assert_eq!(app.world().resource::<LapTimer>().best, Some(42.0));
        app.world_mut().entity_mut(button).insert(Interaction::None);
        app.update();
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Pressed);
        app.update();
        let ghost = app.world().resource::<Ghost>();
        assert!(ghost.best.is_none());
        assert!(ghost.recording.samples.is_empty());
        assert!(ghost.delta.is_none());
        assert!(app.world().resource::<LapTimer>().best.is_none());
        assert_eq!(app.world().resource::<Messages<Reset>>().len(), 1);

        // Leaving the menu cancels a pending confirmation.
        app.world_mut().entity_mut(button).insert(Interaction::None);
        app.update();
        app.world_mut()
            .entity_mut(button)
            .insert(Interaction::Pressed);
        app.update();
        assert!(app.world().resource::<Confirmation>().0.is_some());
        *app.world_mut().resource_mut::<Halt>() = Halt::Nothing;
        app.update();
        assert!(app.world().resource::<Confirmation>().0.is_none());
    }
}
