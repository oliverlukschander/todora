//! Things that happen to you.
//!
//! Every half a minute or so, and whenever a mystery box is opened, something is
//! done to the driving. The gravity goes. The road turns to ice. You become
//! enormous, or the size of a shoe. The camera does a barrel roll. Cows start
//! falling out of the sky. Each is for a few seconds, announced when it starts
//! with a banner and a noise, and gone when it is done.
//!
//! None of it takes the steering away: the wheel wired the other way round was
//! one of them once, and four seconds of a car that cannot be steered is four
//! seconds of a car going off the road, which is not a joke the second time.
//!
//! What is *in force* is here — [`Chaos`] — and what it does goes through the
//! same doors everything else uses: the gravity is [`Gravity`], which the air
//! reads; the grip and the speed are in [`super::tweak`]; the size is read by
//! the mount; the roll by the camera. Nothing reaches the engine.

use bevy::prelude::*;

use super::announcer::{Announce, Points};
use super::pads::Opened;
use super::particles::{Burst, Kind, Tint};
use super::props::Rain;
use super::rng::Rng;
use super::tweak::Boost;
use super::{Fun, Mount};
use crate::car::Player;
use crate::pause::Halt;
use crate::sound::{Sfx, SfxKind};

/// Something that can happen to you.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Effect {
    Moon,
    Ice,
    Turbo,
    Giant,
    Tiny,
    Cows,
    Roll,
    Fisheye,
    Disco,
    Hyper,
}

impl Effect {
    pub(crate) const ALL: [Self; 10] = [
        Self::Moon,
        Self::Ice,
        Self::Turbo,
        Self::Giant,
        Self::Tiny,
        Self::Cows,
        Self::Roll,
        Self::Fisheye,
        Self::Disco,
        Self::Hyper,
    ];

    /// The words on the banner.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Moon => "fx.moon",
            Self::Ice => "fx.ice",
            Self::Turbo => "fx.turbo",
            Self::Giant => "fx.giant",
            Self::Tiny => "fx.tiny",
            Self::Cows => "fx.cows",
            Self::Roll => "fx.roll",
            Self::Fisheye => "fx.fisheye",
            Self::Disco => "fx.disco",
            Self::Hyper => "fx.hyper",
        }
    }

    /// How long it lasts, in seconds.
    pub(crate) fn seconds(self) -> f32 {
        match self {
            Self::Moon => 9.0,
            Self::Ice => 6.0,
            Self::Turbo => 3.5,
            Self::Giant => 9.0,
            Self::Tiny => 9.0,
            Self::Cows => 7.0,
            Self::Roll => 1.9,
            Self::Fisheye => 7.0,
            Self::Disco => 10.0,
            Self::Hyper => 6.0,
        }
    }

    /// Whether it would do anything in this game: a giant car is only a giant if
    /// there is a mount to swell, and the lights are only what a disco or a
    /// fisheye is done with while there are lights and they are allowed to move.
    /// Nothing is announced that nothing would come of.
    pub(crate) fn applies(self, fun: &Fun) -> bool {
        match self {
            Self::Giant | Self::Tiny => fun.mount != Mount::Car,
            Self::Disco | Self::Fisheye => fun.neon && !fun.calm,
            Self::Roll => !fun.calm,
            _ => true,
        }
    }

    /// Whether it is something to be worried about, and gets a red banner.
    fn nasty(self) -> bool {
        matches!(self, Self::Ice)
    }

    fn noise(self) -> SfxKind {
        match self {
            Self::Moon => SfxKind::Ufo,
            Self::Ice => SfxKind::UhOh,
            Self::Turbo | Self::Hyper => SfxKind::Zoom,
            Self::Giant => SfxKind::Boom,
            Self::Tiny => SfxKind::Squeak,
            Self::Cows => SfxKind::HornMoo,
            Self::Roll => SfxKind::SlideUp,
            Self::Fisheye => SfxKind::Zap,
            Self::Disco => SfxKind::Tada,
        }
    }
}

/// What is in force, and for how much longer.
#[derive(Resource)]
pub(crate) struct Chaos {
    active: Vec<(Effect, f32)>,
    /// Seconds of driving until something happens of its own accord.
    next: f32,
    rng: Rng,
}

impl Default for Chaos {
    fn default() -> Self {
        let mut rng = Rng::random();
        Self {
            active: Vec::new(),
            next: rng.range(18.0, 28.0),
            rng,
        }
    }
}

impl Chaos {
    pub(crate) fn has(&self, effect: Effect) -> bool {
        self.active.iter().any(|(e, _)| *e == effect)
    }

    /// How much of `effect` there is, 0 to 1: it fades in over a moment and out
    /// over a longer one.
    pub(crate) fn strength(&self, effect: Effect) -> f32 {
        self.active
            .iter()
            .find(|(e, _)| *e == effect)
            .map_or(0.0, |(e, left)| {
                let elapsed = e.seconds() - left;
                (elapsed / 0.3).min(1.0).min((left / 0.7).min(1.0))
            })
    }

    /// The camera's extra roll, in radians: a full turn over the length of the
    /// effect, eased so it starts and ends still.
    pub(crate) fn roll(&self) -> f32 {
        self.active
            .iter()
            .find(|(e, _)| *e == Effect::Roll)
            .map_or(0.0, |(e, left)| {
                let t = (1.0 - left / e.seconds()).clamp(0.0, 1.0);
                let eased = t * t * (3.0 - 2.0 * t);
                eased * std::f32::consts::TAU
            })
    }

    /// How big the mount is, as a multiple of its size.
    pub(crate) fn size(&self) -> f32 {
        let giant = self.strength(Effect::Giant);
        let tiny = self.strength(Effect::Tiny);
        1.0 + giant * 0.9 - tiny * 0.5
    }

    /// Set something going, unless it already is.
    pub(super) fn start(&mut self, effect: Effect) -> bool {
        if self.has(effect) {
            return false;
        }
        self.active.push((effect, effect.seconds()));
        true
    }

    /// Pick something that is not already happening, and would do something.
    fn pick(&mut self, fun: &Fun) -> Effect {
        let useful: Vec<Effect> = Effect::ALL.into_iter().filter(|e| e.applies(fun)).collect();
        let choices: Vec<Effect> = useful.iter().copied().filter(|e| !self.has(*e)).collect();
        let from = if choices.is_empty() {
            &useful
        } else {
            &choices
        };
        *self.rng.pick(from)
    }
}

/// How hard turbo time pushes, as a boost: a boost pad's strength for most of five
/// seconds was a car arriving at every corner half as fast again as it could take
/// it, and well short of that, for not so long, is still a rush.
pub(crate) const TURBO: f32 = 0.4;

/// The world's gravity, as a fraction of its usual, for a car in the air.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub(crate) struct Gravity {
    pub scale: f32,
}

impl Default for Gravity {
    fn default() -> Self {
        Self { scale: 1.0 }
    }
}

impl Gravity {
    pub(crate) fn factor(&self) -> f32 {
        self.scale
    }
}

/// How much of the world's gravity the moon leaves, at strength `moon` (0 to 1).
/// Half, at the most: at a quarter a jump pad hung a car in the air for most of
/// eight seconds with no brakes.
pub(crate) fn moon_gravity(moon: f32) -> f32 {
    1.0 - 0.5 * moon
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Gravity>()
        .init_resource::<Chaos>()
        .add_systems(Update, (tick, open).chain().run_if(super::bonkers))
        .add_systems(Update, calm_down.run_if(not(super::bonkers)));
}

/// Something is happening to you: say so, and make the noise.
#[allow(clippy::too_many_arguments)]
fn begin(
    effect: Effect,
    chaos: &mut Chaos,
    said: &mut MessageWriter<Announce>,
    sounds: &mut MessageWriter<Sfx>,
    points: &mut MessageWriter<Points>,
    boost: &mut Boost,
    rain: &mut MessageWriter<Rain>,
    bursts: &mut MessageWriter<Burst>,
    at: Option<Vec3>,
) {
    if !chaos.start(effect) {
        return;
    }
    said.write(if effect.nasty() {
        Announce::alarm(effect.key())
    } else {
        Announce::big(effect.key())
    });
    sounds.write(Sfx::new(effect.noise()).gain(0.9));
    points.write(Points {
        amount: 150,
        what: "pop.chaos",
        at,
    });
    match effect {
        Effect::Turbo => boost.fire(TURBO, effect.seconds()),
        Effect::Cows => {
            rain.write(Rain { count: 12 });
        }
        _ => {}
    }
    if let Some(at) = at {
        bursts.write(
            Burst::new(Kind::Confetti, at + Vec3::Y * 0.6, 50)
                .toward(Vec3::Y, 1.0)
                .speed(6.0)
                .size(0.1)
                .tint(Tint::Party),
        );
    }
}

/// Count everything down, and every so often let something loose.
#[allow(clippy::too_many_arguments)]
fn tick(
    time: Res<Time>,
    halt: Res<Halt>,
    fun: Res<Fun>,
    mut chaos: ResMut<Chaos>,
    mut gravity: ResMut<Gravity>,
    mut boost: ResMut<Boost>,
    mut said: MessageWriter<Announce>,
    mut sounds: MessageWriter<Sfx>,
    mut points: MessageWriter<Points>,
    mut rain: MessageWriter<Rain>,
    mut bursts: MessageWriter<Burst>,
    cars: Query<&Transform, With<Player>>,
) {
    if *halt != Halt::Nothing {
        return;
    }
    let dt = time.delta_secs();
    let at = cars.single().ok().map(|c| c.translation);
    // What has run out.
    let mut over = false;
    for (_, left) in &mut chaos.active {
        *left -= dt;
    }
    chaos.active.retain(|(_, left)| {
        let keep = *left > 0.0;
        over |= !keep;
        keep
    });
    if over && chaos.active.is_empty() {
        said.write(Announce::small("fx.over"));
    }
    // Something of its own accord.
    if fun.chaos {
        chaos.next -= dt;
        if chaos.next <= 0.0 {
            chaos.next = chaos.rng.range(24.0, 38.0);
            let effect = chaos.pick(&fun);
            begin(
                effect,
                &mut chaos,
                &mut said,
                &mut sounds,
                &mut points,
                &mut boost,
                &mut rain,
                &mut bursts,
                at,
            );
        }
    }
    let scale = moon_gravity(chaos.strength(Effect::Moon));
    if (gravity.scale - scale).abs() > 1e-4 {
        gravity.scale = scale;
    }
}

/// A box has been opened: something happens, whether or not the timer would
/// have got there.
#[allow(clippy::too_many_arguments)]
fn open(
    mut opened: MessageReader<Opened>,
    fun: Res<Fun>,
    mut chaos: ResMut<Chaos>,
    mut boost: ResMut<Boost>,
    mut said: MessageWriter<Announce>,
    mut sounds: MessageWriter<Sfx>,
    mut points: MessageWriter<Points>,
    mut rain: MessageWriter<Rain>,
    mut bursts: MessageWriter<Burst>,
    cars: Query<&Transform, With<Player>>,
) {
    let at = cars.single().ok().map(|c| c.translation);
    for _ in opened.read() {
        let effect = chaos.pick(&fun);
        begin(
            effect,
            &mut chaos,
            &mut said,
            &mut sounds,
            &mut points,
            &mut boost,
            &mut rain,
            &mut bursts,
            at,
        );
    }
}

/// Not silly enough for any of it: nothing is in force.
fn calm_down(mut chaos: ResMut<Chaos>, mut gravity: ResMut<Gravity>) {
    if !chaos.active.is_empty() {
        chaos.active.clear();
    }
    if gravity.scale != 1.0 {
        gravity.scale = 1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chaos() -> Chaos {
        Chaos {
            active: Vec::new(),
            next: 30.0,
            rng: Rng::new(9),
        }
    }

    #[test]
    fn an_effect_fades_in_holds_and_fades_out() {
        let mut c = chaos();
        assert!(c.start(Effect::Moon));
        let total = Effect::Moon.seconds();
        c.active[0].1 = total; // just begun
        assert_eq!(c.strength(Effect::Moon), 0.0);
        c.active[0].1 = total - 0.15;
        assert!((c.strength(Effect::Moon) - 0.5).abs() < 0.01);
        c.active[0].1 = total / 2.0;
        assert_eq!(c.strength(Effect::Moon), 1.0);
        c.active[0].1 = 0.35;
        assert!((c.strength(Effect::Moon) - 0.5).abs() < 0.01);
        assert_eq!(c.strength(Effect::Ice), 0.0, "what is not on is not on");
    }

    /// Bonkers with everything on: a chicken, and neon, and no reduced motion.
    fn everything() -> Fun {
        Fun::of(&crate::settings::Settings {
            bonkers: true,
            ..crate::settings::Settings::default()
        })
    }

    #[test]
    fn nothing_starts_twice_and_something_is_always_picked() {
        let fun = everything();
        let mut c = chaos();
        assert!(c.start(Effect::Ice));
        assert!(!c.start(Effect::Ice));
        // With all but one running, that one is what comes next.
        for effect in Effect::ALL {
            if effect != Effect::Hyper {
                c.start(effect);
            }
        }
        assert_eq!(c.pick(&fun), Effect::Hyper);
        // And with everything running, it still picks something.
        c.start(Effect::Hyper);
        assert!(Effect::ALL.contains(&c.pick(&fun)));
    }

    #[test]
    fn only_what_would_do_something_is_ever_announced() {
        use crate::settings::Settings;
        let mut c = chaos();
        // A car has nothing to swell or shrink, and with the neon off there is
        // nothing for a disco or a fisheye to be done to.
        let plain = Fun::of(&Settings {
            bonkers: true,
            mount: Mount::Car,
            neon: false,
            ..Settings::default()
        });
        for _ in 0..500 {
            let effect = c.pick(&plain);
            assert!(
                !matches!(
                    effect,
                    Effect::Giant | Effect::Tiny | Effect::Disco | Effect::Fisheye
                ),
                "{effect:?} in a game it would do nothing in"
            );
        }
        // Reduced motion has no camera roll, no disco and no fisheye either.
        let calm = Fun::of(&Settings {
            bonkers: true,
            reduced_motion: true,
            ..Settings::default()
        });
        for _ in 0..500 {
            let effect = c.pick(&calm);
            assert!(
                !matches!(effect, Effect::Roll | Effect::Disco | Effect::Fisheye),
                "{effect:?} while it is meant to be calm"
            );
        }
        // And with everything on, all of them can come.
        let all = everything();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..2_000 {
            seen.insert(c.pick(&all));
        }
        assert_eq!(seen.len(), Effect::ALL.len());
    }

    #[test]
    fn the_size_swells_and_shrinks() {
        let mut c = chaos();
        assert_eq!(c.size(), 1.0);
        c.start(Effect::Giant);
        c.active[0].1 = Effect::Giant.seconds() / 2.0;
        assert!(c.size() > 1.8);
        let mut d = chaos();
        d.start(Effect::Tiny);
        d.active[0].1 = Effect::Tiny.seconds() / 2.0;
        assert!(d.size() < 0.6);
    }

    #[test]
    fn the_camera_rolls_once_and_ends_where_it_began() {
        let mut c = chaos();
        c.start(Effect::Roll);
        let total = Effect::Roll.seconds();
        c.active[0].1 = total;
        assert!(c.roll().abs() < 1e-4);
        c.active[0].1 = total / 2.0;
        assert!(
            (c.roll() - std::f32::consts::PI).abs() < 0.05,
            "half way round at half time"
        );
        c.active[0].1 = 0.001;
        assert!((c.roll() - std::f32::consts::TAU).abs() < 0.01);
    }

    #[test]
    fn every_effect_is_worded_lasts_a_while_and_makes_a_noise() {
        for effect in Effect::ALL {
            assert!(effect.key().starts_with("fx."));
            assert!((1.0..15.0).contains(&effect.seconds()), "{effect:?}");
            let _ = effect.noise();
        }
        let mut keys: Vec<_> = Effect::ALL.iter().map(|e| e.key()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), Effect::ALL.len());
    }
}
