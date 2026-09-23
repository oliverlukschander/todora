//! Quiet radio and recorded wheel sounds driven by speed and grip.
mod radio;
mod synth;
use synth::Tyres;

use bevy::{audio::AddAudioSource, prelude::*, window::PrimaryWindow};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering::Relaxed},
};

use crate::{
    car::{Car, Player},
    pause::Halt,
    track::Track,
};

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Sound>()
            .add_message::<SoundToggle>()
            .add_audio_source::<radio::Soundtrack>()
            .add_systems(Startup, start)
            .add_systems(Update, (apply_toggles, update, draw_toggles, beep).chain())
            .add_systems(Last, shutdown_on_exit);
    }
}

#[derive(Resource)]
pub(crate) struct Sound {
    pub music: bool,
    pub effects: bool,
    signal: Arc<Signal>,
}

impl Default for Sound {
    fn default() -> Self {
        Self {
            music: true,
            effects: true,
            signal: Arc::new(Signal::default()),
        }
    }
}

/// Pause-menu buttons that turn the radio or the tyres off without touching
/// the other. `M` and `F8` do the same job while driving.
#[derive(Component, Message, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SoundToggle {
    Music,
    Effects,
}

impl Sound {
    pub fn station(&self) -> &'static str {
        if !self.music {
            return "off";
        }
        match self.signal.status.load(Relaxed) {
            1 => "playing",
            2 => "reconnecting",
            _ => "connecting",
        }
    }
}

/// Only these small values cross into the audio and network threads.
#[derive(Default)]
struct Signal {
    shutting_down: AtomicBool,
    enabled: AtomicBool,
    status: AtomicU8,
    music: AtomicU32,
    scrub: AtomicU32,
    squeal: AtomicU32,
    rolling: AtomicU32,
    /// How far everything the car makes is turned down, 0 (full volume) to 1.
    /// Stored as the cut rather than the volume so a fresh signal is audible.
    effects_cut: AtomicU32,
    /// Start-light beeps asked for: a count, doubled, plus one for GO.
    beep: AtomicU32,
}

impl Signal {
    fn shutdown(&self) {
        if !self.shutting_down.swap(true, Relaxed) {
            self.enabled.store(false, Relaxed);
            info!("Stopping audio and Omarchy radio");
        }
    }
}

// Native macOS Quit clears the world directly, without another update.
impl Drop for Sound {
    fn drop(&mut self) {
        self.signal.shutdown();
    }
}

fn shutdown_on_exit(
    mut exits: MessageReader<AppExit>,
    sound: Res<Sound>,
    mut sinks: Query<&mut AudioSink>,
) {
    if exits.read().next().is_some() {
        sound.signal.shutdown();
        for mut sink in &mut sinks {
            sink.set_volume(bevy::audio::Volume::Linear(0.0));
            sink.stop();
        }
    }
}

fn start(mut commands: Commands, mut assets: ResMut<Assets<radio::Soundtrack>>, sound: Res<Sound>) {
    commands.spawn(AudioPlayer(
        assets.add(radio::Soundtrack(sound.signal.clone())),
    ));
}

fn update(
    keys: Res<ButtonInput<KeyCode>>,
    halt: Res<Halt>,
    windows: Query<&Window, With<PrimaryWindow>>,
    players: Query<(&Car, &Transform), With<Player>>,
    track: Res<Track>,
    settings: Option<Res<crate::settings::Settings>>,
    mut sound: ResMut<Sound>,
) {
    if sound.signal.shutting_down.load(Relaxed) {
        return;
    }
    let (music_volume, effects_volume) = settings
        .as_ref()
        .map_or((0.8, 1.0), |s| (s.music_volume, s.effects_volume));
    sound
        .signal
        .effects_cut
        .store((1.0 - effects_volume).to_bits(), Relaxed);
    // Audio shortcuts work while driving and paused; garage input stays separate.
    if keys.just_pressed(KeyCode::KeyN) && *halt != Halt::Menu {
        sound.music = !sound.music;
        if sound.music {
            sound.signal.status.store(0, Relaxed);
        }
    }
    if keys.just_pressed(KeyCode::F8) && *halt != Halt::Menu {
        sound.effects = !sound.effects;
    }
    let audible = windows.iter().all(|window| window.focused);
    let (rolling, scrub, squeal) = if audible && sound.effects && !halt.stopped() {
        players
            .single()
            .map(|(car, pose)| {
                let grip = track.ground_from(pose.translation, car.along).grip;
                let (scrub, squeal) = tyre_levels(car, grip);
                (rolling_level(car, grip), scrub, squeal)
            })
            .unwrap_or_default()
    } else {
        (0.0, 0.0, 0.0)
    };
    sound.signal.rolling.store(rolling.to_bits(), Relaxed);
    // A little extra room for the tyre warning when a slide becomes loud.
    // 0.8 is where the radio has always sat.
    let music = if audible && sound.music {
        (0.075 - 0.08 * squeal) * music_volume / 0.8
    } else {
        0.0
    };
    sound.signal.enabled.store(audible && sound.music, Relaxed);
    sound.signal.music.store(music.to_bits(), Relaxed);
    sound.signal.scrub.store(scrub.to_bits(), Relaxed);
    sound.signal.squeal.store(squeal.to_bits(), Relaxed);
}

/// A start light has come on. It is heard with the tyres, so the tyre toggle
/// silences it too, and not at all while the window is in the background.
fn beep(
    mut lights: MessageReader<crate::countdown::StartLight>,
    windows: Query<&Window, With<PrimaryWindow>>,
    sound: Res<Sound>,
) {
    for light in lights.read() {
        if sound.effects && windows.iter().all(|window| window.focused) {
            let asked = sound.signal.beep.load(Relaxed);
            let next = ((asked >> 1).wrapping_add(1) << 1) | u32::from(light.go);
            sound.signal.beep.store(next, Relaxed);
        }
    }
}

fn apply_toggles(
    halt: Res<Halt>,
    mut toggles: MessageReader<SoundToggle>,
    mut sound: ResMut<Sound>,
) {
    if *halt != Halt::Pause || sound.signal.shutting_down.load(Relaxed) {
        toggles.clear();
        return;
    }
    for which in toggles.read() {
        match which {
            SoundToggle::Music => {
                sound.music = !sound.music;
                if sound.music {
                    sound.signal.status.store(0, Relaxed);
                }
            }
            SoundToggle::Effects => sound.effects = !sound.effects,
        }
    }
}

fn draw_toggles(
    sound: Res<Sound>,
    buttons: Query<(&SoundToggle, &Children)>,
    mut texts: Query<&mut Text>,
) {
    for (&which, children) in &buttons {
        let wanted = match which {
            SoundToggle::Music => {
                format!("Music  /  N    {}", if sound.music { "on" } else { "off" })
            }
            SoundToggle::Effects => format!(
                "Sound effects  /  F8    {}",
                if sound.effects { "on" } else { "off" }
            ),
        };
        for child in children {
            if let Ok(mut text) = texts.get_mut(*child)
                && text.0 != wanted
            {
                text.0 = wanted.clone();
            }
        }
    }
}

fn smooth(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn tyre_levels(car: &Car, surface_grip: f32) -> (f32, f32) {
    let rolling = smooth((car.velocity.length() - 1.0) / 5.0);
    // Rubber squeals on the paved surface; grass should not sound like tarmac.
    let pavement = smooth((surface_grip - 0.4) / 0.45);
    let scrub = smooth((car.grip_used - 0.65) / 0.35);
    let slide = smooth((car.grip_used - 0.98) / 0.45).max(car.rear_slip.clamp(0.0, 1.0));
    (
        0.18 * rolling * pavement * scrub.max(slide),
        0.32 * rolling * pavement * slide,
    )
}

fn rolling_level(car: &Car, surface_grip: f32) -> f32 {
    let speed = car.velocity.length();
    // Rolling contact stays audible when coasting and below the grip limit.
    // Rough ground adds a little weight; stopped wheels make no sound.
    smooth(speed / 0.6)
        * (0.18 + 0.20 * smooth(speed / 24.0))
        * (1.0 + 0.25 * (1.0 - surface_grip).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_actions_toggle_music_and_effects_independently_and_update_labels() {
        let mut app = App::new();
        app.init_resource::<Sound>()
            .insert_resource(Halt::Pause)
            .add_message::<SoundToggle>()
            .add_systems(Update, (apply_toggles, draw_toggles).chain());
        app.world_mut()
            .spawn(SoundToggle::Music)
            .with_children(|parent| {
                parent.spawn(Text::new(""));
            });
        app.world_mut().write_message(SoundToggle::Music);
        app.update();
        assert!(!app.world().resource::<Sound>().music);
        assert!(app.world().resource::<Sound>().effects);
        let mut labels = app.world_mut().query::<&Text>();
        assert_eq!(labels.single(app.world()).unwrap().0, "Music  /  N    off");
        app.world_mut().write_message(SoundToggle::Effects);
        app.update();
        assert!(!app.world().resource::<Sound>().effects);
        app.world_mut().write_message(SoundToggle::Music);
        app.update();
        assert!(app.world().resource::<Sound>().music);
        assert!(!app.world().resource::<Sound>().effects);
        *app.world_mut().resource_mut::<Halt>() = Halt::Menu;
        app.world_mut().write_message(SoundToggle::Effects);
        app.update();
        assert!(
            !app.world().resource::<Sound>().effects,
            "pause actions must not leak into other menus"
        );
    }

    #[test]
    fn both_exit_messages_and_native_world_cleanup_stop_audio() {
        let mut app = App::new();
        app.init_resource::<Sound>()
            .add_systems(Last, shutdown_on_exit);
        let signal = app.world().resource::<Sound>().signal.clone();
        signal.enabled.store(true, Relaxed);
        app.world_mut().write_message(AppExit::Success);
        app.update();
        assert!(signal.shutting_down.load(Relaxed));
        assert!(!signal.enabled.load(Relaxed));

        let mut app = App::new();
        app.init_resource::<Sound>();
        let signal = app.world().resource::<Sound>().signal.clone();
        signal.enabled.store(true, Relaxed);
        app.world_mut().clear_all();
        assert!(signal.shutting_down.load(Relaxed));
        assert!(!signal.enabled.load(Relaxed));
    }

    #[test]
    fn rolling_is_audible_below_the_limit_in_both_directions_and_on_grass() {
        let mut car = Car::default();
        assert_eq!(rolling_level(&car, 1.0), 0.0);
        car.velocity = Vec3::Z * 2.0;
        let slow = rolling_level(&car, 1.0);
        assert!(slow > 0.0);
        car.velocity = Vec3::Z * 24.0;
        let fast = rolling_level(&car, 1.0);
        assert!(fast > slow);
        assert_eq!(tyre_levels(&car, 1.0), (0.0, 0.0));
        car.velocity = -car.velocity;
        assert_eq!(rolling_level(&car, 1.0), fast);
        assert!(rolling_level(&car, 0.38) > fast);
    }

    #[test]
    fn pause_focus_and_mute_controls_keep_music_and_tyres_separate() {
        let track = Track::any();
        let pose = track.start_transform();
        let mut app = App::new();
        app.init_resource::<Sound>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Halt>()
            .insert_resource(track)
            .add_systems(Update, update)
            .add_systems(Last, shutdown_on_exit);
        app.world_mut().spawn((
            Player,
            pose,
            Car {
                velocity: Vec3::Z * 20.0,
                grip_used: 1.2,
                rear_slip: 0.7,
                ..default()
            },
        ));
        let window = app
            .world_mut()
            .spawn((Window::default(), PrimaryWindow))
            .id();
        app.update();
        let signal = app.world().resource::<Sound>().signal.clone();
        let volume = |value: &AtomicU32| f32::from_bits(value.load(Relaxed));
        assert!(volume(&signal.scrub) > 0.0 && volume(&signal.music) > 0.0);
        *app.world_mut().resource_mut::<Halt>() = Halt::Pause;
        app.update();
        assert_eq!(volume(&signal.scrub), 0.0);
        assert_eq!(volume(&signal.rolling), 0.0);
        assert_eq!(volume(&signal.squeal), 0.0);
        assert_eq!(volume(&signal.rolling), 0.0);
        assert!(volume(&signal.music) > 0.0);
        *app.world_mut().resource_mut::<Halt>() = Halt::Nothing;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyM);
        app.update();
        assert!(
            volume(&signal.music) > 0.0,
            "multiplayer must not mute music"
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyN);
        app.update();
        assert_eq!(volume(&signal.music), 0.0);
        assert!(!signal.enabled.load(Relaxed));
        assert!(volume(&signal.squeal) > 0.0);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyN);
        app.update();
        assert!(volume(&signal.music) > 0.0);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F8);
        app.update();
        assert_eq!(volume(&signal.squeal), 0.0);
        assert_eq!(volume(&signal.rolling), 0.0);
        assert!(volume(&signal.music) > 0.0, "effects off leaves the radio");
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut().resource_mut::<Sound>().effects = true;
        app.world_mut().resource_mut::<Sound>().music = true;
        app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
        app.update();
        assert_eq!(volume(&signal.music), 0.0);
        assert_eq!(volume(&signal.scrub), 0.0);
        assert_eq!(volume(&signal.rolling), 0.0);
        app.world_mut().get_mut::<Window>(window).unwrap().focused = true;
        app.update();
        assert!(volume(&signal.scrub) > 0.0 && volume(&signal.music) > 0.0);
    }

    #[test]
    fn tyres_warn_before_the_limit_and_get_clearer_when_sliding() {
        let mut car = Car {
            velocity: Vec3::Z * 20.0,
            ..default()
        };
        assert_eq!(tyre_levels(&car, 1.0), (0.0, 0.0));
        car.grip_used = 0.8;
        let near = tyre_levels(&car, 1.0);
        car.grip_used = 1.0;
        let limit = tyre_levels(&car, 1.0);
        car.grip_used = 1.4;
        let beyond = tyre_levels(&car, 1.0);
        assert!(near.0 > 0.0 && limit.0 > near.0);
        assert!(beyond.1 > limit.1 && limit.1 > near.1);
        car.grip_used = 0.4;
        car.rear_slip = 0.9;
        assert!(
            tyre_levels(&car, 1.0).1 > limit.1,
            "a rear slide is audible even below base grip"
        );
        assert_eq!(tyre_levels(&car, 0.38), (0.0, 0.0));
        car.velocity = Vec3::ZERO;
        assert_eq!(tyre_levels(&car, 1.0), (0.0, 0.0));
    }

    #[test]
    fn synthesis_is_bounded_and_fades_to_silence() {
        let mut tyres = Tyres::default();
        let mut energy = 0.0;
        for _ in 0..44_100 {
            let sample = tyres.sample(0.38, 0.28, 0.30);
            assert!(sample.is_finite() && sample.abs() < 0.8);
            energy += sample * sample;
        }
        assert!(energy / 44_100.0 > 0.0005);
        for _ in 0..22_050 {
            tyres.sample(0.0, 0.0, 0.0);
        }
        assert!(tyres.sample(0.0, 0.0, 0.0).abs() < 1e-6);
    }
}
