//! The horn.
//!
//! Every mount has a voice, and the horn is a roulette: mostly it says what the
//! mount would say, and now and then it says anything at all. A chicken bawks,
//! clucks, squawks and, one time in twelve, crows; a duck quacks; the trolley
//! squeaks. Once in four presses it is somebody else's horn entirely, which is
//! the point of a horn button in a game like this.

use bevy::prelude::*;

use super::mount::Honk;
use super::rng::Rng;
use super::{Fun, Mount};
use crate::settings::bindings::{Act, current};
use crate::sound::{Sfx, SfxKind};

/// Seconds a held horn waits between honks.
const REPEAT: f64 = 0.34;

pub(super) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        honk.run_if(super::silly)
            .run_if(crate::pause::running)
            .run_if(crate::local::solo),
    );
}

/// What a press of the horn says, and how high.
pub(crate) fn voice(mount: Mount, rng: &mut Rng) -> (SfxKind, f32) {
    use SfxKind::*;
    let own: &[(SfxKind, u32)] = match mount {
        Mount::Chicken => &[(Bawk, 50), (Cluck, 20), (Squawk, 14), (Crow, 8), (Peep, 8)],
        Mount::Duck => &[(HornQuack, 80), (Squeak, 20)],
        Mount::Tub => &[(HornAir, 55), (HornClown, 45)],
        Mount::Cart => &[(HornMeep, 45), (Squeak, 55)],
        Mount::Car => &[(HornBeep, 100)],
    };
    // One press in four, the horn belongs to somebody else.
    const ANYONE: [SfxKind; 10] = [
        HornClown,
        HornOoga,
        HornMeep,
        HornMoo,
        HornQuack,
        HornAir,
        HornBeep,
        HornTrombone,
        Boing,
        Zap,
    ];
    let kind = if rng.one_in(4) {
        *rng.pick(&ANYONE)
    } else {
        let total: u32 = own.iter().map(|(_, w)| w).sum();
        let mut roll = rng.below(total as usize) as u32;
        own.iter()
            .find(|(_, w)| {
                let hit = roll < *w;
                roll = roll.saturating_sub(*w);
                hit
            })
            .map_or(own[0].0, |(k, _)| *k)
    };
    (kind, rng.range(0.9, 1.14))
}

#[allow(clippy::too_many_arguments)]
fn honk(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    settings: Option<Res<crate::settings::Settings>>,
    fun: Res<Fun>,
    time: Res<Time<Real>>,
    mut honks: MessageWriter<Honk>,
    mut sounds: MessageWriter<Sfx>,
    mut rng: Local<Option<Rng>>,
    mut last: Local<f64>,
) {
    let bindings = current(settings.as_deref());
    let now = time.elapsed_secs_f64();
    let pressed = bindings.just(&keys, &pads, Act::Horn);
    let held = bindings.key_held(&keys, Act::Horn)
        || bindings
            .pad(Act::Horn)
            .is_some_and(|b| pads.iter().any(|pad| pad.pressed(b)));
    if !(pressed || held && now - *last > REPEAT) {
        return;
    }
    *last = now;
    let rng = rng.get_or_insert_with(Rng::random);
    let (kind, pitch) = voice(fun.mount, rng);
    sounds.write(Sfx::new(kind).pitch(pitch));
    honks.write(Honk {
        crowed: kind == SfxKind::Crow,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chicken_mostly_speaks_chicken_and_a_horn_is_still_a_roulette() {
        let mut rng = Rng::new(42);
        let mut chicken = 0;
        let mut other = std::collections::HashSet::new();
        for _ in 0..4_000 {
            let (kind, pitch) = voice(Mount::Chicken, &mut rng);
            assert!((0.9..1.14).contains(&pitch));
            if matches!(
                kind,
                SfxKind::Bawk | SfxKind::Cluck | SfxKind::Squawk | SfxKind::Crow | SfxKind::Peep
            ) {
                chicken += 1;
            } else {
                other.insert(kind);
            }
        }
        assert!(chicken > 2_600 && chicken < 3_600, "{chicken} of 4000");
        assert!(other.len() >= 6, "the roulette only ever said {other:?}");
    }

    #[test]
    fn every_mount_has_something_to_say() {
        let mut rng = Rng::new(1);
        for mount in Mount::ALL {
            for _ in 0..50 {
                let (kind, _) = voice(mount, &mut rng);
                assert!(!kind.name().is_empty());
            }
        }
    }
}
