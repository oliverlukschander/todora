//! Bonkers Edition: everything that is not serious.
//!
//! The rule for this whole module is that it stands *around* the engine and
//! never inside it. [`crate::car::advance`], the physics step and the way the
//! track holds the car are the same bit for bit as they always were, because a
//! lap on the leaderboard is a lap the server replays under those rules, and
//! the determinism digests pin them.
//!
//! What this layer does instead is sit beside them:
//!
//! - **Silly** dresses the game up and touches nothing else. A chicken to ride,
//!   googly eyes and hats, tyre smoke, confetti, a horn, an announcer. The car
//!   drives exactly as it did and laps still count.
//! - **Bonkers** changes how things go. The engine is fed a faster, looser
//!   [`Handling`](crate::car::Handling) (see [`tweak`]), the circuits grow
//!   hills to launch off, the road is lit like a rave and a techno track
//!   plays along to it, and things happen to you. A lap driven like that
//!   is flagged in the lap clock as not counting, the way an assisted lap is
//!   flagged as not going online: it is still a lap, and it is never a record.
//! - **Serious** is the game as it shipped. Every system here stands down.
//!
//! [`Fun`] is what says which of those is in force, worked out once a frame
//! from the settings so that nothing else has to look at them.

pub(crate) mod air;
pub(crate) mod mount;
pub(crate) mod parts;
pub(crate) mod rng;
pub(crate) mod tweak;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// How silly the game is allowed to be.
#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default, Hash,
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Silliness {
    /// The game as it shipped.
    Serious,
    /// Dressed up, and driving exactly the same.
    Silly,
    /// Everything, and no lap counts.
    #[default]
    Bonkers,
}

impl Silliness {
    pub(crate) const ALL: [Self; 3] = [Self::Serious, Self::Silly, Self::Bonkers];

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Serious => "fun.serious",
            Self::Silly => "fun.silly",
            Self::Bonkers => "fun.bonkers",
        }
    }
}

/// What you ride.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Mount {
    /// The Omarchy GT, as shipped.
    Car,
    /// A rooster in a racing helmet's care.
    #[default]
    Chicken,
    /// A rubber duck with somewhere to be.
    Duck,
    /// A bathtub on four small wheels.
    Tub,
    /// A shopping trolley, going downhill.
    Cart,
}

impl Mount {
    pub(crate) const ALL: [Self; 5] = [Self::Car, Self::Chicken, Self::Duck, Self::Tub, Self::Cart];

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Car => "mount.car",
            Self::Chicken => "mount.chicken",
            Self::Duck => "mount.duck",
            Self::Tub => "mount.tub",
            Self::Cart => "mount.cart",
        }
    }
}

/// What sits on the mount's head, if it has one.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Hat {
    /// A different one every time the car goes back to the grid.
    #[default]
    Surprise,
    Bare,
    Party,
    Top,
    Crown,
    Cone,
    Propeller,
    Chef,
    Cowboy,
}

impl Hat {
    pub(crate) const ALL: [Self; 9] = [
        Self::Surprise,
        Self::Bare,
        Self::Party,
        Self::Top,
        Self::Crown,
        Self::Cone,
        Self::Propeller,
        Self::Chef,
        Self::Cowboy,
    ];
    /// The hats a surprise chooses between.
    pub(crate) const REAL: [Self; 7] = [
        Self::Party,
        Self::Top,
        Self::Crown,
        Self::Cone,
        Self::Propeller,
        Self::Chef,
        Self::Cowboy,
    ];

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Surprise => "hat.surprise",
            Self::Bare => "hat.bare",
            Self::Party => "hat.party",
            Self::Top => "hat.top",
            Self::Crown => "hat.crown",
            Self::Cone => "hat.cone",
            Self::Propeller => "hat.propeller",
            Self::Chef => "hat.chef",
            Self::Cowboy => "hat.cowboy",
        }
    }
}

/// How much faster than the car it shipped as. Time itself is not touched: the
/// handling is scaled so the same corners are taken at higher speeds, which is
/// the same car driven faster and not a different one.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Speed {
    Normal,
    Fast,
    #[default]
    Ludicrous,
    Plaid,
}

impl Speed {
    pub(crate) const ALL: [Self; 4] = [Self::Normal, Self::Fast, Self::Ludicrous, Self::Plaid];

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Normal => "speed.normal",
            Self::Fast => "speed.fast",
            Self::Ludicrous => "speed.ludicrous",
            Self::Plaid => "speed.plaid",
        }
    }

    /// The factor speeds are multiplied by.
    pub(crate) fn scale(self) -> f32 {
        match self {
            Self::Normal => 1.0,
            Self::Fast => 1.6,
            Self::Ludicrous => 2.4,
            Self::Plaid => 3.6,
        }
    }
}

/// How much the circuits are allowed to rise and fall. The layout on the map
/// is never touched, so a circuit is still the circuit; it is the third
/// dimension that gets silly.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default, Hash)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Wild {
    /// The circuits as surveyed.
    Off,
    /// Rolling hills, a few small hops.
    Rolling,
    #[default]
    Rollercoaster,
    /// Do not.
    Absurd,
}

impl Wild {
    pub(crate) const ALL: [Self; 4] = [Self::Off, Self::Rolling, Self::Rollercoaster, Self::Absurd];

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Off => "wild.off",
            Self::Rolling => "wild.rolling",
            Self::Rollercoaster => "wild.rollercoaster",
            Self::Absurd => "wild.absurd",
        }
    }

    /// The number the track builder is told, 0 meaning as surveyed.
    pub(crate) fn level(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Rolling => 1,
            Self::Rollercoaster => 2,
            Self::Absurd => 3,
        }
    }
}

/// What is actually in force this frame: the settings, held to what the
/// silliness allows. Systems ask this and never the settings, so one place
/// decides that a Silly game has no techno and a Serious one has no chicken.
#[derive(Resource, Clone, Debug, PartialEq)]
pub(crate) struct Fun {
    pub level: Silliness,
    pub mount: Mount,
    pub hat: Hat,
    pub eyes: bool,
    pub speed: Speed,
    pub wild: Wild,
    pub neon: bool,
    pub techno: bool,
    pub chaos: bool,
    /// Reduced motion: nothing flashes, shakes or strobes, whatever else is on.
    pub calm: bool,
}

impl Default for Fun {
    /// Serious, so an app built without the settings behaves as it always did.
    fn default() -> Self {
        Self {
            level: Silliness::Serious,
            mount: Mount::Car,
            hat: Hat::Bare,
            eyes: false,
            speed: Speed::Normal,
            wild: Wild::Off,
            neon: false,
            techno: false,
            chaos: false,
            calm: false,
        }
    }
}

impl Fun {
    /// What `settings` come to once the silliness has had its say.
    pub(crate) fn of(settings: &crate::settings::Settings) -> Self {
        let level = settings.silliness;
        let silly = level >= Silliness::Silly;
        let bonkers = level >= Silliness::Bonkers;
        Self {
            level,
            mount: if silly { settings.mount } else { Mount::Car },
            hat: if silly { settings.hat } else { Hat::Bare },
            eyes: silly && settings.googly_eyes,
            speed: if bonkers { settings.speed } else { Speed::Normal },
            wild: if bonkers { settings.wild } else { Wild::Off },
            neon: bonkers && settings.neon,
            techno: bonkers && settings.techno,
            chaos: bonkers && settings.chaos,
            calm: settings.reduced_motion,
        }
    }

    /// Dressed up, at least.
    pub(crate) fn silly(&self) -> bool {
        self.level >= Silliness::Silly
    }

    /// Everything.
    pub(crate) fn bonkers(&self) -> bool {
        self.level >= Silliness::Bonkers
    }

    /// Whether anything about the drive itself differs from the shipped game,
    /// which is when the laps stop counting.
    pub(crate) fn changes_the_drive(&self) -> bool {
        self.bonkers()
    }
}

/// Run condition: the game is at least Silly.
pub(crate) fn silly(fun: Res<Fun>) -> bool {
    fun.silly()
}

/// Run condition: the game is Bonkers.
pub(crate) fn bonkers(fun: Res<Fun>) -> bool {
    fun.bonkers()
}

pub struct FunPlugin;

impl Plugin for FunPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Fun>()
            .init_resource::<tweak::Boost>()
            .add_systems(Startup, parts::setup)
            .add_systems(PreUpdate, resolve.before(crate::pause::HaltSet))
            .add_systems(Update, attach);
        mount::plugin(app);
    }
}

/// Give the player's car somewhere to keep its height off the road.
fn attach(
    mut commands: Commands,
    cars: Query<Entity, (With<crate::car::Player>, Without<air::Air>)>,
) {
    for car in &cars {
        commands.entity(car).insert(air::Air::default());
    }
}

/// Work out what is in force, once a frame, before anything reads it.
fn resolve(settings: Option<Res<crate::settings::Settings>>, mut fun: ResMut<Fun>) {
    if let Some(settings) = settings {
        let wanted = Fun::of(&settings);
        fun.set_if_neq(wanted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn serious_is_the_game_as_it_shipped() {
        let settings = Settings {
            silliness: Silliness::Serious,
            ..Settings::default()
        };
        let fun = Fun::of(&settings);
        assert_eq!(
            Fun {
                calm: settings.reduced_motion,
                ..Fun::default()
            },
            fun,
            "nothing survives a Serious game, whatever the rows say"
        );
        assert!(!fun.silly() && !fun.bonkers() && !fun.changes_the_drive());
    }

    #[test]
    fn silly_dresses_up_and_leaves_the_drive_alone() {
        let fun = Fun::of(&Settings {
            silliness: Silliness::Silly,
            ..Settings::default()
        });
        assert_eq!(fun.mount, Mount::Chicken);
        assert!(fun.eyes && fun.silly());
        assert_eq!(fun.speed, Speed::Normal, "the drive is untouched");
        assert_eq!(fun.wild, Wild::Off, "so are the circuits");
        assert!(!fun.neon && !fun.techno && !fun.chaos);
        assert!(!fun.changes_the_drive(), "and so are the laps");
    }

    #[test]
    fn bonkers_is_everything_the_rows_ask_for() {
        let fun = Fun::of(&Settings::default());
        assert!(fun.bonkers() && fun.changes_the_drive());
        assert_eq!(fun.speed, Speed::Ludicrous);
        assert_eq!(fun.wild, Wild::Rollercoaster);
        assert!(fun.neon && fun.techno && fun.chaos);
        let tame = Fun::of(&Settings {
            speed: Speed::Normal,
            wild: Wild::Off,
            ..Settings::default()
        });
        assert_eq!((tame.speed, tame.wild), (Speed::Normal, Wild::Off));
    }

    #[test]
    fn the_scale_grows_with_the_name() {
        let scales: Vec<f32> = Speed::ALL.iter().map(|s| s.scale()).collect();
        assert_eq!(scales[0], 1.0);
        assert!(scales.windows(2).all(|w| w[0] < w[1]));
        let levels: Vec<u8> = Wild::ALL.iter().map(|w| w.level()).collect();
        assert_eq!(levels, [0, 1, 2, 3]);
    }

    #[test]
    fn a_settings_file_from_before_all_this_still_reads() {
        let old = r#"{"version":2,"music":false,"fov":50.0}"#;
        let settings = Settings::parse(old);
        assert!(!settings.music);
        assert_eq!(settings.silliness, Silliness::Bonkers);
        let text = serde_json::to_string(&settings).unwrap();
        assert_eq!(Settings::parse(&text), settings, "and reads back the same");
    }
}
