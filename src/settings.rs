//! What the player chose, kept between sessions.
//!
//! One small JSON file, `settings.json`, beside the saved laps. Every field
//! has a default, so a file from an older version fills in whatever it does
//! not mention and a field this version does not know is ignored; a file that
//! cannot be read at all means the defaults and one line in the log. It is
//! written a second after the last change, off the main thread, beside the real
//! file and then moved onto it, so a crash mid-write keeps the old settings.
//!
//! The circuit, car, setup and driving mode are remembered too, so the game
//! opens where it was left. They are stored by name, so a circuit that has
//! since been removed is simply not found and the first one is driven instead.

use std::path::PathBuf;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::car::{Mode, Setup, Spec};
use crate::track::Track;

/// Bumped when a field changes meaning; [`Settings::migrate`] then says how
/// to read the old one.
pub(crate) const VERSION: u32 = 1;
const FILE: &str = "settings.json";
/// Seconds after the last change before it is written.
const SETTLE: f64 = 1.0;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Units {
    #[default]
    Kmh,
    Mph,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Countdown {
    /// The whole countdown on every start, restarts included.
    Full,
    /// The whole one onto a new circuit, mode or car; a short one on `R`.
    #[default]
    Short,
    /// No countdown: the car is free at once.
    Off,
}

#[derive(Resource, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub(crate) struct Settings {
    pub version: u32,
    // Audio.
    pub music: bool,
    pub effects: bool,
    /// 0 to 1.
    pub music_volume: f32,
    /// 0 to 1: tyres, beeps and everything else the car makes.
    pub effects_volume: f32,
    // Display.
    pub fullscreen: bool,
    pub vsync: bool,
    pub antialiasing: bool,
    /// On top of the automatic scale for the window size; 0.9 to 1.5.
    pub ui_scale: f32,
    /// Keep the HUD inside the middle 90% of the screen, for televisions.
    pub tv_margin: bool,
    /// Frames a second at most; 0 for no limit.
    pub fps_cap: u32,
    /// Fraction of the display's pixels the game renders, 0.5 to 1.
    pub render_scale: f32,
    pub show_fps: bool,
    /// The chase camera's vertical field of view, in degrees.
    pub fov: f32,
    // HUD.
    pub units: Units,
    pub minimap: bool,
    pub g_meter: bool,
    pub countdown: Countdown,
    // Controls.
    /// Multiplies steering input; 0.5 to 1.5.
    pub steering: f32,
    /// How far a stick has to move before it steers, 0.02 to 0.4.
    pub deadzone: f32,
    pub rumble: bool,
    // What was being driven.
    pub ghost: bool,
    pub onboarding_done: bool,
    pub circuit: String,
    pub car: String,
    pub setup: String,
    pub mode: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: VERSION,
            music: true,
            effects: true,
            music_volume: 0.8,
            effects_volume: 1.0,
            fullscreen: false,
            vsync: true,
            antialiasing: true,
            ui_scale: 1.0,
            tv_margin: false,
            fps_cap: 0,
            render_scale: 1.0,
            show_fps: false,
            fov: 45.0,
            units: Units::Kmh,
            minimap: true,
            g_meter: true,
            countdown: Countdown::Short,
            steering: 1.0,
            deadzone: 0.12,
            rumble: true,
            ghost: true,
            onboarding_done: false,
            circuit: String::new(),
            car: String::new(),
            setup: String::new(),
            mode: String::new(),
        }
    }
}

impl Settings {
    /// Read a file's contents; anything unreadable is the defaults.
    pub(crate) fn parse(text: &str) -> Self {
        match serde_json::from_str::<Self>(text) {
            Ok(settings) => settings.migrate().sanitised(),
            Err(trouble) => {
                warn!("settings could not be read ({trouble}); using defaults");
                Self::default()
            }
        }
    }

    /// Bring a file written by an older version up to this one. There is only
    /// one version so far; a change of meaning goes in here as a match arm.
    fn migrate(mut self) -> Self {
        self.version = VERSION;
        self
    }

    /// Every number inside the range its row offers, and nothing that is not
    /// a number at all.
    fn sanitised(mut self) -> Self {
        let within = |value: f32, low: f32, high: f32, default: f32| {
            if value.is_finite() {
                value.clamp(low, high)
            } else {
                default
            }
        };
        let d = Self::default();
        self.music_volume = within(self.music_volume, 0.0, 1.0, d.music_volume);
        self.effects_volume = within(self.effects_volume, 0.0, 1.0, d.effects_volume);
        self.ui_scale = within(self.ui_scale, 0.9, 1.5, d.ui_scale);
        self.render_scale = within(self.render_scale, 0.5, 1.0, d.render_scale);
        self.fov = within(self.fov, 35.0, 75.0, d.fov);
        self.steering = within(self.steering, 0.5, 1.5, d.steering);
        self.deadzone = within(self.deadzone, 0.02, 0.4, d.deadzone);
        self.fps_cap = self.fps_cap.min(240);
        self
    }

    fn text(&self) -> String {
        serde_json::to_string_pretty(self).expect("settings serialise")
    }

    pub(crate) fn spec(&self) -> Option<Spec> {
        Spec::ALL.into_iter().find(|s| key(s) == self.car)
    }

    pub(crate) fn chosen_setup(&self) -> Option<Setup> {
        Setup::ALL.into_iter().find(|s| key(s) == self.setup)
    }

    pub(crate) fn chosen_mode(&self) -> Option<Mode> {
        Mode::ALL.into_iter().find(|m| key(m) == self.mode)
    }

    pub(crate) fn chosen_circuit(&self) -> Option<&'static crate::track::Circuit> {
        crate::track::all_circuits()
            .iter()
            .find(|c| c.id == self.circuit)
    }
}

/// A variant's name, as stored.
fn key(value: &impl std::fmt::Debug) -> String {
    format!("{value:?}").to_lowercase()
}

/// Where this machine keeps what Todora saves: the laps and these settings.
pub(crate) fn data_dir() -> Option<PathBuf> {
    #[cfg(all(target_os = "macos", feature = "game-center"))]
    let base = crate::multiplayer::support_directory();
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(any(
        target_os = "windows",
        all(target_os = "macos", feature = "game-center")
    )))]
    let base = {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        if cfg!(target_os = "macos") {
            home.map(|home| home.join("Library/Application Support"))
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| home.map(|home| home.join(".local/share")))
        }
    };
    Some(base?.join("Todora"))
}

fn load() -> Settings {
    let Some(path) = data_dir().map(|dir| dir.join(FILE)) else {
        return Settings::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => Settings::parse(&text),
        Err(trouble) if trouble.kind() == std::io::ErrorKind::NotFound => Settings::default(),
        Err(trouble) => {
            warn!("cannot read {}: {trouble}", path.display());
            Settings::default()
        }
    }
}

/// Write beside the file and move it into place, on a thread of its own.
fn save(text: String) {
    let Some(dir) = data_dir() else {
        return;
    };
    std::thread::spawn(move || {
        let (path, beside) = (dir.join(FILE), dir.join("settings.writing"));
        let written = std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::write(&beside, text))
            .and_then(|()| std::fs::rename(&beside, &path));
        if let Err(trouble) = written {
            warn!("cannot save settings to {}: {trouble}", path.display());
        }
    });
}

/// Present in runs that must not touch the player's saved settings: visual
/// checks, which put the car wherever the capture wants it.
#[derive(Resource)]
pub(crate) struct ReadOnly;

/// When the last change was, until it has been written.
#[derive(Resource, Default)]
struct Unsaved(Option<f64>);

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    /// Loaded while the app is being built, so the car, the circuit and the
    /// sound start as they were left rather than changing on the first frame.
    fn build(&self, app: &mut App) {
        let settings = load();
        if let Some(spec) = settings.spec() {
            app.insert_resource(spec);
        }
        if let Some(setup) = settings.chosen_setup() {
            app.insert_resource(setup);
        }
        if let Some(mode) = settings.chosen_mode() {
            app.insert_resource(mode);
        }
        app.insert_resource(settings)
            .init_resource::<Unsaved>()
            .add_systems(PostStartup, apply_saved)
            .add_systems(Last, (remember, write).chain());
    }
}

/// The toggles that live in their own plugins take the saved values.
fn apply_saved(
    settings: Res<Settings>,
    mut sound: ResMut<crate::sound::Sound>,
    mut ghost: ResMut<crate::ghost::Ghost>,
) {
    sound.music = settings.music;
    sound.effects = settings.effects;
    ghost.on = settings.ghost;
}

/// Keep what was chosen elsewhere — a key, a menu — in the settings, touching
/// them only when something actually differs so an unchanged frame saves
/// nothing.
fn remember(
    mut settings: ResMut<Settings>,
    track: Res<Track>,
    spec: Res<Spec>,
    setup: Res<Setup>,
    mode: Res<Mode>,
    sound: Res<crate::sound::Sound>,
    ghost: Res<crate::ghost::Ghost>,
) {
    let current = &*settings;
    let circuit = track.circuit().id;
    let differs = current.circuit != circuit
        || current.car != key(&*spec)
        || current.setup != key(&*setup)
        || current.mode != key(&*mode)
        || current.music != sound.music
        || current.effects != sound.effects
        || current.ghost != ghost.on;
    if differs {
        let settings = settings.as_mut();
        settings.circuit = circuit.into();
        settings.car = key(&*spec);
        settings.setup = key(&*setup);
        settings.mode = key(&*mode);
        settings.music = sound.music;
        settings.effects = sound.effects;
        settings.ghost = ghost.on;
    }
}

fn write(
    settings: Res<Settings>,
    time: Res<Time<Real>>,
    read_only: Option<Res<ReadOnly>>,
    mut unsaved: ResMut<Unsaved>,
) {
    if read_only.is_some() {
        return;
    }
    let now = time.elapsed_secs_f64();
    if settings.is_changed() {
        unsaved.0 = Some(now);
    }
    if unsaved.0.is_some_and(|at| now - at >= SETTLE) {
        unsaved.0 = None;
        save(settings.text());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        let settings = Settings {
            music_volume: 0.35,
            units: Units::Mph,
            countdown: Countdown::Off,
            circuit: "monza".into(),
            car: "express".into(),
            ..default()
        };
        assert_eq!(Settings::parse(&settings.text()), settings);
    }

    #[test]
    fn a_missing_field_takes_its_default_and_an_unknown_one_is_ignored() {
        let settings = Settings::parse(r#"{"version": 1, "units": "mph", "flying": true}"#);
        assert_eq!(settings.units, Units::Mph);
        assert_eq!(settings.music_volume, Settings::default().music_volume);
    }

    #[test]
    fn a_broken_file_gives_the_defaults() {
        assert_eq!(Settings::parse("{ not json"), Settings::default());
        assert_eq!(Settings::parse(""), Settings::default());
    }

    #[test]
    fn numbers_are_held_to_what_their_rows_offer() {
        let settings = Settings::parse(
            r#"{"music_volume": 7, "ui_scale": 0.1, "fov": 500, "deadzone": -1, "fps_cap": 100000}"#,
        );
        assert_eq!(settings.music_volume, 1.0);
        assert_eq!(settings.ui_scale, 0.9);
        assert_eq!(settings.fov, 75.0);
        assert_eq!(settings.deadzone, 0.02);
        assert_eq!(settings.fps_cap, 240);
    }

    #[test]
    fn an_older_version_is_brought_up_to_date() {
        assert_eq!(Settings::parse(r#"{"version": 0}"#).version, VERSION);
    }

    #[test]
    fn what_was_driven_is_found_again_by_name() {
        let settings = Settings {
            circuit: "spa-francorchamps".into(),
            car: key(&Spec::Clubman),
            setup: key(&Setup::Oversteer),
            mode: key(&Mode::Pro),
            ..default()
        };
        assert_eq!(settings.spec(), Some(Spec::Clubman));
        assert_eq!(settings.chosen_setup(), Some(Setup::Oversteer));
        assert_eq!(settings.chosen_mode(), Some(Mode::Pro));
        assert_eq!(settings.chosen_circuit().unwrap().id, "spa-francorchamps");
        let gone = Settings {
            circuit: "a-circuit-that-was-removed".into(),
            ..default()
        };
        assert!(gone.chosen_circuit().is_none() && gone.spec().is_none());
    }
}
