//! What it feels like.
//!
//! The camera opens its lens as the car goes faster, shakes when something is
//! hit, leans into corners, and the car thumps on the way down from a jump.
//! Nothing here changes the drive: the field of view is the camera's, the shake
//! is added to the camera after it has been placed and taken off again before it
//! is next, and a jolt is a number that decays.
//!
//! Anything can shake the camera by writing a [`Jolt`]: a fraction of a full
//! shake, which is added to the trauma and squared on the way out, so that
//! small knocks are barely felt and a great many together are a great deal.

use bevy::prelude::*;

use super::Fun;
use super::air::{Landed, Launched};
use super::mount::{Honk, Pose};
use super::tweak::Boost;
use crate::camera::{FollowCam, follow};
use crate::sound::{Sfx, SfxKind};

/// A knock: 0 is nothing, 1 is everything the camera has.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct Jolt(pub f32);

#[derive(Resource, Default)]
struct Shake {
    trauma: f32,
    /// What was added to the camera last frame, to be taken off before it moves.
    applied: Vec3,
}

/// Trauma lost a second.
const CALM_DOWN: f32 = 1.7;
/// The most the camera moves at a full shake, in game units, and turns, in
/// radians.
const MOVE: f32 = 0.16;
const TURN: f32 = 0.035;
/// Degrees of field of view a car at its top speed adds, and a full boost.
const WIDEN: f32 = 13.0;
const WIDEN_BOOST: f32 = 9.0;

pub(super) fn plugin(app: &mut App) {
    app.add_message::<Jolt>()
        .init_resource::<Shake>()
        .add_systems(
            Update,
            (
                unshake.before(follow),
                (feel, lens, shake).chain().after(follow),
            ),
        );
}

/// Take last frame's shake off the camera, before it follows the car.
fn unshake(mut shake: ResMut<Shake>, mut cameras: Query<&mut Transform, With<FollowCam>>) {
    if shake.applied == Vec3::ZERO {
        return;
    }
    for mut camera in &mut cameras {
        camera.translation -= shake.applied;
    }
    shake.applied = Vec3::ZERO;
}

/// Thumps, and the knocks they give the camera.
fn feel(
    fun: Res<Fun>,
    mut launched: MessageReader<Launched>,
    mut landed: MessageReader<Landed>,
    mut honks: MessageReader<Honk>,
    mut jolts: MessageReader<Jolt>,
    mut shake: ResMut<Shake>,
    mut sounds: MessageWriter<Sfx>,
) {
    for jump in launched.read() {
        sounds.write(
            Sfx::new(if jump.power > 0.0 {
                SfxKind::Boing
            } else {
                SfxKind::Whoosh
            })
            .gain(0.6),
        );
        shake.trauma += 0.12;
    }
    for down in landed.read() {
        if down.impact > 1.2 {
            sounds.write(Sfx::new(SfxKind::Thud).gain((down.impact / 7.0).clamp(0.3, 1.3)));
        }
        if down.airtime > 0.45 {
            sounds.write(Sfx::new(SfxKind::Boing).pitch(1.3).gain(0.4));
        }
        shake.trauma += 0.10 + down.impact * 0.045;
    }
    if honks.read().next().is_some() {
        shake.trauma += 0.07;
    }
    for jolt in jolts.read() {
        shake.trauma += jolt.0;
    }
    shake.trauma = if fun.calm {
        0.0
    } else {
        shake.trauma.clamp(0.0, 1.0)
    };
}

/// The field of view: the driver's own, and wider the faster the car goes.
/// When the game stops being silly it is the driver's own again, put back the
/// once: nothing else writes it unless the setting is changed.
fn lens(
    fun: Res<Fun>,
    halt: Res<crate::pause::Halt>,
    settings: Option<Res<crate::settings::Settings>>,
    pose: Res<Pose>,
    boost: Res<Boost>,
    mut cameras: Query<&mut Projection, With<FollowCam>>,
    mut widened: Local<bool>,
) {
    // The title and the replay place the camera themselves, from the setting.
    let elsewhere = matches!(
        *halt,
        crate::pause::Halt::Title | crate::pause::Halt::Replay
    );
    let kick = if fun.silly() && !elsewhere {
        let top = 24.0 * fun.speed.scale();
        let quick = (pose.speed.abs() / top).clamp(0.0, 1.3).powf(1.4);
        (WIDEN * quick + WIDEN_BOOST * boost.strength()) * if fun.calm { 0.35 } else { 1.0 }
    } else {
        0.0
    };
    if kick == 0.0 && !*widened {
        return;
    }
    *widened = kick != 0.0;
    let base = settings.map_or(45.0, |s| s.fov);
    for mut projection in &mut cameras {
        if let Projection::Perspective(perspective) = projection.as_mut() {
            let wanted = (base + kick).to_radians();
            if (perspective.fov - wanted).abs() > 1e-4 {
                perspective.fov = wanted;
            }
        }
    }
}

/// Shake the camera, and lean it into the corner.
fn shake(
    time: Res<Time>,
    halt: Res<crate::pause::Halt>,
    fun: Res<Fun>,
    pose: Res<Pose>,
    chaos: Res<super::events::Chaos>,
    mut shake: ResMut<Shake>,
    mut cameras: Query<&mut Transform, With<FollowCam>>,
) {
    // The replay and the title place the camera themselves.
    if halt.stopped() || !fun.silly() {
        return;
    }
    let dt = time.delta_secs();
    shake.trauma = (shake.trauma - CALM_DOWN * dt).max(0.0);
    let t = time.elapsed_secs();
    let amount = shake.trauma * shake.trauma;
    let wobble = |seed: f32| (t * 29.0 + seed).sin() * 0.6 + (t * 47.0 + seed * 1.7).sin() * 0.4;
    for mut camera in &mut cameras {
        let offset = camera.right() * (wobble(1.0) * MOVE * amount)
            + camera.up() * (wobble(2.0) * MOVE * amount);
        camera.translation += offset;
        shake.applied = offset;
        // Into the corner, and a little for the shake.
        let lean = if fun.calm {
            0.0
        } else {
            pose.lean.x.clamp(-1.3, 1.3) * 0.028
        };
        // A barrel roll is all of a turn: it is skipped for anyone who would
        // rather the screen did not spin.
        let roll = if fun.calm { 0.0 } else { chaos.roll() };
        camera.rotation *= Quat::from_rotation_z(lean + roll + wobble(3.0) * TURN * amount)
            * Quat::from_rotation_x(wobble(4.0) * TURN * 0.6 * amount);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_field_of_view_goes_back_to_the_drivers_own_when_the_fun_stops() {
        use bevy::camera::PerspectiveProjection;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(crate::settings::Settings::default())
            .insert_resource(Fun::of(&crate::settings::Settings::default()))
            .init_resource::<Pose>()
            .init_resource::<Boost>()
            .init_resource::<crate::pause::Halt>()
            .add_systems(Update, lens);
        app.world_mut().resource_mut::<Pose>().speed = 60.0;
        let own = crate::settings::Settings::default().fov.to_radians();
        let camera = app
            .world_mut()
            .spawn((
                FollowCam { zoom: 1.0 },
                Projection::Perspective(PerspectiveProjection {
                    fov: own,
                    ..default()
                }),
            ))
            .id();
        let fov = |app: &App| match app.world().get::<Projection>(camera).unwrap() {
            Projection::Perspective(p) => p.fov,
            _ => unreachable!(),
        };
        app.update();
        assert!(fov(&app) > own + 0.1, "the lens did not open at speed");
        // Leaving the fun behind, as F9 does, at speed.
        app.world_mut().resource_mut::<Fun>().level = super::super::Silliness::Serious;
        app.update();
        assert_eq!(fov(&app), own, "the field of view was left wide");
        // And with nothing to undo it does not touch the camera again.
        if let Projection::Perspective(p) = app
            .world_mut()
            .get_mut::<Projection>(camera)
            .unwrap()
            .as_mut()
        {
            p.fov = 1.0;
        }
        app.update();
        assert_eq!(fov(&app), 1.0);
    }

    #[test]
    fn knocks_add_up_and_die_away() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Fun>()
            .init_resource::<Pose>()
            .init_resource::<Boost>()
            .init_resource::<crate::pause::Halt>()
            .init_resource::<super::super::events::Chaos>()
            .add_message::<Launched>()
            .add_message::<Landed>()
            .add_message::<Honk>()
            .add_message::<Sfx>()
            .add_message::<Jolt>()
            .init_resource::<Shake>()
            .add_systems(Update, (feel, shake).chain());
        app.world_mut().resource_mut::<Fun>().level = super::super::Silliness::Silly;
        for _ in 0..3 {
            app.world_mut().write_message(Jolt(0.2));
        }
        app.update();
        let after = app.world().resource::<Shake>().trauma;
        assert!(
            (after - 0.6).abs() < 0.05,
            "three knocks of 0.2 make {after}"
        );
        app.world_mut().write_message(Jolt(5.0));
        app.update();
        assert!(
            app.world().resource::<Shake>().trauma <= 1.0,
            "trauma is capped"
        );
        // And with reduced motion on, nothing shakes at all.
        app.world_mut().resource_mut::<Fun>().calm = true;
        app.world_mut().write_message(Jolt(1.0));
        app.update();
        assert_eq!(app.world().resource::<Shake>().trauma, 0.0);
    }

    #[test]
    fn a_hard_landing_thumps_and_a_soft_one_does_not() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Fun>()
            .add_message::<Launched>()
            .add_message::<Landed>()
            .add_message::<Honk>()
            .add_message::<Sfx>()
            .add_message::<Jolt>()
            .init_resource::<Shake>()
            .add_systems(Update, feel);
        app.world_mut().write_message(Landed {
            impact: 0.3,
            airtime: 0.1,
            peak: 0.02,
        });
        app.update();
        let quiet: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<Sfx>>()
            .drain()
            .collect();
        assert!(quiet.is_empty(), "a bump made a noise");
        app.world_mut().write_message(Landed {
            impact: 9.0,
            airtime: 1.2,
            peak: 3.0,
        });
        app.update();
        let loud: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<Sfx>>()
            .drain()
            .collect();
        assert!(loud.iter().any(|s| s.kind == SfxKind::Thud));
        assert!(
            loud.iter().any(|s| s.kind == SfxKind::Boing),
            "a long flight bounces"
        );
    }
}
