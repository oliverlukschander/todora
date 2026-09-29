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
pub(crate) mod announcer;
pub(crate) mod awards;
pub(crate) mod beat;
pub(crate) mod course;
mod disco;
pub(crate) mod events;
mod horn;
mod hotkey;
pub(crate) mod juice;
pub(crate) mod mount;
pub(crate) mod pads;
pub(crate) mod particles;
pub(crate) mod parts;
pub(crate) mod props;
pub(crate) mod quips;
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
        Self::held_to(settings, settings.silliness)
    }

    /// As [`Fun::of`], with the silliness taken from somewhere else: what a game
    /// played with somebody else is held to, whatever the settings say.
    pub(crate) fn held_to(settings: &crate::settings::Settings, level: Silliness) -> Self {
        let silly = level >= Silliness::Silly;
        let bonkers = level >= Silliness::Bonkers;
        Self {
            level,
            mount: if silly { settings.mount } else { Mount::Car },
            hat: if silly { settings.hat } else { Hat::Bare },
            eyes: silly && settings.googly_eyes,
            speed: if bonkers {
                settings.speed
            } else {
                Speed::Normal
            },
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

/// What the engine's g telemetry is multiplied by to read as it would in the
/// shipped car. A car `k` times as fast pulls `k²` times the g in the same
/// corner, and the meter, the lean and the camera all want the corner's g.
pub(crate) fn g_scale(fun: Option<&Fun>) -> f32 {
    fun.map_or(1.0, |fun| fun.speed.scale().powi(-2))
}

/// Run condition: the game is at least Silly.
pub(crate) fn silly(fun: Res<Fun>) -> bool {
    fun.silly()
}

/// Run condition: the game is Bonkers.
pub(crate) fn bonkers(fun: Res<Fun>) -> bool {
    fun.bonkers()
}

/// The game is being played with somebody else: split-screen (from its lobby,
/// so the circuit is built once for it and not again as the race starts), or a
/// shared practice. Everyone there has to be driving the same car on the same
/// road, so the fun layer stands down until it is over, and comes back after.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Together(pub bool);

pub struct FunPlugin;

impl Plugin for FunPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Fun>()
            .init_resource::<Together>()
            .init_resource::<tweak::Boost>()
            .add_systems(Startup, parts::setup)
            .add_systems(PreUpdate, resolve.before(crate::pause::HaltSet));
        mount::plugin(app);
        horn::plugin(app);
        hotkey::plugin(app);
        beat::plugin(app);
        tweak::plugin(app);
        air::plugin(app);
        events::plugin(app);
        juice::plugin(app);
        particles::plugin(app);
        announcer::plugin(app);
        awards::plugin(app);
        course::plugin(app);
        pads::plugin(app);
        props::plugin(app);
        quips::plugin(app);
        disco::plugin(app);
    }
}

/// Work out what is in force, once a frame, before anything reads it.
///
/// Two things it does on the way that the levels do not say. A circuit that is
/// being built for company is still being built for company while the loading
/// screen is up: with the lobby gone from the halt, the request would change its
/// mind, the build that answered the last one would be thrown away as out of
/// date, and the wait would begin again for ever. And leaving the drive that
/// Bonkers made puts the car back on the grid, because it is still going at the
/// speed Bonkers gave it, and the next lap that starts is one that counts.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve(
    settings: Option<Res<crate::settings::Settings>>,
    race: Option<Res<crate::local::LocalRace>>,
    session: Option<Res<crate::multiplayer::Session>>,
    halt: Option<Res<crate::pause::Halt>>,
    mut fun: ResMut<Fun>,
    mut together: ResMut<Together>,
    request: Option<ResMut<crate::track::WildRequest>>,
    mut resets: MessageWriter<crate::Reset>,
) {
    use crate::pause::Halt;
    let halt = halt.map(|halt| *halt);
    let for_company_now = together.0 && halt == Some(Halt::Loading);
    let with_others = crate::local::active(race.as_deref())
        || halt == Some(Halt::LocalLobby)
        || session.is_some_and(|s| s.active())
        || for_company_now;
    together.set_if_neq(Together(with_others));
    if let Some(settings) = settings {
        let wanted = if with_others {
            Fun::held_to(&settings, Silliness::Serious)
        } else {
            Fun::of(&settings)
        };
        if let Some(mut request) = request
            && halt != Some(Halt::Settings)
        {
            // Left alone while the settings page is up: the circuit is built again
            // once, when the page is closed, and not for every step of the row,
            // with a loading screen in the middle of the page.
            request.set_if_neq(crate::track::WildRequest(wanted.wild.level()));
        }
        let was = fun.changes_the_drive();
        fun.set_if_neq(wanted);
        if was && !fun.changes_the_drive() {
            resets.write(crate::Reset);
        }
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
    fn a_game_with_company_is_held_to_serious_whatever_the_settings_say() {
        let settings = Settings::default();
        assert!(Fun::of(&settings).bonkers(), "the default is wild");
        let held = Fun::held_to(&settings, Silliness::Serious);
        assert!(!held.silly() && !held.changes_the_drive());
        assert_eq!(
            (held.mount, held.speed, held.wild),
            (Mount::Car, Speed::Normal, Wild::Off)
        );
        assert!(!held.neon && !held.techno && !held.chaos);
    }

    #[test]
    fn the_fun_layer_stands_down_for_company_and_comes_back_after() {
        use crate::pause::Halt;
        let mut app = App::new();
        app.init_resource::<Fun>()
            .init_resource::<Together>()
            .init_resource::<crate::local::LocalRace>()
            .init_resource::<crate::multiplayer::Session>()
            .init_resource::<crate::track::WildRequest>()
            .add_message::<crate::Reset>()
            .insert_resource(Halt::Nothing)
            .insert_resource(Settings::default())
            .add_systems(Update, resolve);
        let now = |app: &App| {
            (
                app.world().resource::<Fun>().level,
                app.world().resource::<Together>().0,
                app.world().resource::<crate::track::WildRequest>().0,
            )
        };
        app.update();
        assert_eq!(
            now(&app),
            (Silliness::Bonkers, false, 2),
            "alone, it is wild"
        );
        // The lobby counts, so the circuit is built once for the race and not
        // again when it starts.
        *app.world_mut().resource_mut::<Halt>() = Halt::LocalLobby;
        app.update();
        assert_eq!(now(&app), (Silliness::Serious, true, 0), "in the lobby");
        // Building the circuit for the lobby puts the loading screen where the
        // lobby was. The request must hold through it, or what was built is
        // thrown away for being what was asked for a moment ago, and the
        // lobby is loaded again for ever.
        *app.world_mut().resource_mut::<Halt>() = Halt::Loading;
        for _ in 0..5 {
            app.update();
            assert_eq!(now(&app), (Silliness::Serious, true, 0), "loading");
        }
        *app.world_mut().resource_mut::<Halt>() = Halt::LocalLobby;
        app.update();
        assert_eq!(
            now(&app),
            (Silliness::Serious, true, 0),
            "in the lobby again"
        );
        *app.world_mut().resource_mut::<Halt>() = Halt::Nothing;
        app.world_mut()
            .resource_mut::<crate::local::LocalRace>()
            .active = true;
        app.update();
        assert_eq!(now(&app), (Silliness::Serious, true, 0), "in the race");
        app.world_mut()
            .resource_mut::<crate::local::LocalRace>()
            .active = false;
        app.update();
        assert_eq!(now(&app), (Silliness::Bonkers, false, 2), "and back again");
        // Loading on its own account, alone, is not company.
        *app.world_mut().resource_mut::<Halt>() = Halt::Loading;
        app.update();
        assert_eq!(now(&app), (Silliness::Bonkers, false, 2), "loading alone");
    }

    #[test]
    fn the_circuit_is_built_again_when_the_settings_page_is_closed_and_not_before() {
        use crate::pause::Halt;
        let mut app = App::new();
        app.init_resource::<Fun>()
            .init_resource::<Together>()
            .init_resource::<crate::local::LocalRace>()
            .init_resource::<crate::multiplayer::Session>()
            .init_resource::<crate::track::WildRequest>()
            .add_message::<crate::Reset>()
            .insert_resource(Halt::Settings)
            .insert_resource(Settings::default())
            .add_systems(Update, resolve);
        let asked = |app: &App| app.world().resource::<crate::track::WildRequest>().0;
        for wild in [Wild::Rolling, Wild::Absurd, Wild::Off, Wild::Rollercoaster] {
            app.world_mut().resource_mut::<Settings>().wild = wild;
            app.update();
            assert_eq!(asked(&app), 0, "{wild:?} asked for under the page");
        }
        // Everything else the page changes is at once: that is what shows
        // behind it.
        assert!(app.world().resource::<Fun>().bonkers());
        *app.world_mut().resource_mut::<Halt>() = Halt::Pause;
        app.update();
        assert_eq!(asked(&app), 2, "and now, once");
    }

    /// The resets `resolve` has written so far, counted by a system that reads
    /// them, after one more frame.
    fn resets_in(app: &mut App) -> usize {
        #[derive(Resource, Default)]
        struct Heard(usize);
        if app.world().get_resource::<Heard>().is_none() {
            app.init_resource::<Heard>().add_systems(
                Update,
                (|mut resets: MessageReader<crate::Reset>, mut heard: ResMut<Heard>| {
                    heard.0 += resets.read().count();
                })
                .after(resolve),
            );
        }
        app.update();
        app.world().resource::<Heard>().0
    }

    #[test]
    fn leaving_a_drive_that_bonkers_made_puts_the_car_back_and_nothing_else_does() {
        let mut app = App::new();
        app.init_resource::<Fun>()
            .init_resource::<Together>()
            .init_resource::<crate::local::LocalRace>()
            .init_resource::<crate::multiplayer::Session>()
            .init_resource::<crate::track::WildRequest>()
            .add_message::<crate::Reset>()
            .insert_resource(crate::pause::Halt::Nothing)
            .insert_resource(Settings::default())
            .add_systems(Update, resolve);
        let set = |app: &mut App, level| {
            app.world_mut().resource_mut::<Settings>().silliness = level;
            resets_in(app)
        };
        assert_eq!(resets_in(&mut app), 0, "arriving at Bonkers at the start");
        // Bonkers to Serious: the car still has the speed Bonkers gave it.
        assert_eq!(set(&mut app, Silliness::Serious), 1);
        // Serious to Silly and on to Bonkers change nothing about a car that
        // has only ever been driven as shipped, and the lap clock flags the
        // one that becomes Bonkers.
        assert_eq!(set(&mut app, Silliness::Silly), 1, "no more than before");
        assert_eq!(set(&mut app, Silliness::Bonkers), 1);
        // Bonkers to Silly leaves it too.
        assert_eq!(set(&mut app, Silliness::Silly), 2);
        // And nothing is written while nothing changes.
        assert_eq!(resets_in(&mut app), 2);
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
