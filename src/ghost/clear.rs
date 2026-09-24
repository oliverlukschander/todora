use bevy::prelude::*;

use super::{Ghost, Recording, Records, store};
use crate::{Reset, lap::LapTimer, pause::Halt};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ResetGhosts {
    Current,
    All,
}

#[derive(Component)]
pub(crate) struct Notice;

#[derive(Message)]
pub(crate) struct ResetRequest(pub Option<ResetGhosts>);

#[derive(Resource, Default)]
pub(super) struct Confirmation(Option<ResetGhosts>);

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_reset(
    halt: Res<Halt>,
    mut requests: MessageReader<ResetRequest>,
    mut notices: Query<&mut Text, With<Notice>>,
    mut confirmation: ResMut<Confirmation>,
    mut ghost: ResMut<Ghost>,
    mut timer: ResMut<LapTimer>,
    mut reset: MessageWriter<Reset>,
    context: Option<(Res<crate::track::Track>, Res<crate::car::Mode>)>,
    mut records: Option<ResMut<Records>>,
) {
    if *halt != Halt::Pause {
        requests.clear();
        confirmation.0 = None;
        let note = crate::text::t("pause.reset_note");
        for mut text in &mut notices {
            if text.0 != note {
                text.0 = note.into();
            }
        }
        return;
    }
    for request in requests.read() {
        let Some(scope) = request.0 else {
            confirmation.0 = None;
            for mut text in &mut notices {
                text.0 = crate::text::t("pause.reset_note").into();
            }
            continue;
        };
        let message = if confirmation.0 != Some(scope) {
            confirmation.0 = Some(scope);
            match scope {
                ResetGhosts::Current => "clear.confirm_one",
                ResetGhosts::All => "clear.confirm_all",
            }
        } else {
            confirmation.0 = None;
            // Remove the active file first. Even if another file fails during
            // an all-reset, the current board must agree with what is on disk.
            match ghost.saved.as_ref().map_or(Ok(()), |saved| saved.remove()) {
                Err(error) => {
                    warn!("Cannot reset this ghost: {error}");
                    "clear.fail_one"
                }
                Ok(()) => {
                    ghost.best = None;
                    if let (Some(records), Some((track, mode))) = (records.as_mut(), &context) {
                        records.set(track.circuit().id, **mode, None);
                        if scope == ResetGhosts::All {
                            records.clear();
                        }
                    }
                    ghost.recording = Recording::default();
                    ghost.delta = None;
                    *timer = LapTimer::default();
                    reset.write(Reset);
                    if scope == ResetGhosts::All {
                        match store::remove_all() {
                            Ok(()) => "clear.done_all",
                            Err(error) => {
                                warn!("Cannot reset all ghosts: {error}");
                                "clear.fail_all"
                            }
                        }
                    } else {
                        "clear.done_one"
                    }
                }
            }
        };
        for mut text in &mut notices {
            text.0 = crate::text::t(message).into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_needs_two_activations_and_clears_the_board_and_partial_recording() {
        let mut app = App::new();
        let car = app.world_mut().spawn_empty().id();
        let mut recording = Recording::default();
        recording.push(4.0, 0.2, &Transform::default());
        app.insert_resource(Ghost {
            last: None,
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
        .add_message::<ResetRequest>()
        .add_systems(Update, handle_reset);
        app.world_mut().resource_mut::<LapTimer>().remember(42.0);
        app.world_mut()
            .write_message(ResetRequest(Some(ResetGhosts::Current)));
        app.update();
        assert!(app.world().resource::<Ghost>().best.is_some());
        assert_eq!(app.world().resource::<LapTimer>().best, Some(42.0));
        app.world_mut()
            .write_message(ResetRequest(Some(ResetGhosts::Current)));
        app.update();
        let ghost = app.world().resource::<Ghost>();
        assert!(ghost.best.is_none());
        assert!(ghost.recording.samples.is_empty());
        assert!(ghost.delta.is_none());
        assert!(app.world().resource::<LapTimer>().best.is_none());
        assert_eq!(app.world().resource::<Messages<Reset>>().len(), 1);

        // Leaving the menu cancels a pending confirmation.
        app.world_mut()
            .write_message(ResetRequest(Some(ResetGhosts::Current)));
        app.update();
        assert!(app.world().resource::<Confirmation>().0.is_some());
        app.world_mut().write_message(ResetRequest(None));
        app.update();
        assert!(
            app.world().resource::<Confirmation>().0.is_none(),
            "moving focus cancels confirmation"
        );
        app.world_mut()
            .write_message(ResetRequest(Some(ResetGhosts::Current)));
        app.update();
        *app.world_mut().resource_mut::<Halt>() = Halt::Nothing;
        app.update();
        assert!(app.world().resource::<Confirmation>().0.is_none());
    }
}
