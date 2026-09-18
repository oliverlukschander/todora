//! The three cars, and which one is being driven.
//!
//! Everything a car is, as a car, is one [`Handling`] value. That was the point
//! of putting it in a value in the first place, and this is the promise being
//! cashed: a second car is data, not code. All three are the same shooting
//! brake, the same glTF in a different colour, and they share every number that
//! makes it drive like itself — the brakes, the steering, the way a slide starts
//! and how it is caught. Three numbers differ, and they are the three the menu
//! shows: the grip the car corners on, the downforce that grip climbs with, and
//! where the engine stops pushing.
//!
//! They are meant to be worth choosing between rather than ranked, so they were
//! balanced by lapping rather than by taste. `the_garage` drives all three round
//! every circuit with the plain driver and prints the table; as it stands, each
//! of the three is quickest somewhere and no circuit spreads them by more than
//! 2.2%:
//!
//! ```text
//!                         settles at  hardest stop   stars
//! TOURER                    22.2 m/s        9.9 m    3 3 3
//! CLUBMAN                   18.7 m/s        4.6 m    4 3 2
//! EXPRESS                   26.0 m/s       16.0 m    2 3 4
//!
//! circuit                     TOURER     CLUBMAN     EXPRESS    spread
//! Red Bull Ring               41.20*      41.55       41.33       0.8%
//! Spa-Francorchamps           70.33       69.94*      71.48       2.2%
//! Monza                       64.90       65.50       64.71*      1.2%
//! ```
//!
//! `no_car_is_the_car_to_pick` is what holds that: nobody far off the pace
//! anywhere, and the quickest car not the same one everywhere. The first says
//! the menu has no wrong answer in it and the second says it has no right one,
//! and a menu with either is not a choice.
//!
//! Note that `top_speed` does two jobs in [`super::physics`]: it is where the
//! engine's push fades out, so a car with more of it is also pulling harder in
//! the middle of the range. That is why the three ended up as far apart in grip
//! as they are — the speed axis is the stronger of the two, and the grip had to
//! answer for it. What they do *not* differ in is `accel`, the pull off the
//! line, which is why all three carry the same three stars for it.
//!
//! The stars are not decoration either. `the_stars_say_what_the_numbers_say`
//! holds them to the dials they claim to describe, so a car cannot be retuned
//! into disagreeing with its own menu entry.

use bevy::prelude::*;

use super::physics::Handling;

/// Everything the three cars share, and the one they ship on: the shooting
/// brake as it has always been. The other two say only what they change, so a
/// change to how the car drives is still one edit in [`Handling::SHOOTING_BRAKE`]
/// and reaches all three.
const TOURER: Handling = Handling::SHOOTING_BRAKE;

/// Grip, and not much road speed. The extra downforce goes with the extra grip
/// because both are the same claim — this car holds on — and downforce is how
/// holding on survives a fast corner.
const CLUBMAN: Handling = Handling {
    grip: 16.8,
    downforce: 0.0115,
    top_speed: 19.8,
    ..Handling::SHOOTING_BRAKE
};

/// Road speed, and less to hold on with. It runs out of grip before the others
/// do and runs out of engine long after them.
const EXPRESS: Handling = Handling {
    grip: 12.3,
    downforce: 0.0072,
    top_speed: 28.8,
    ..Handling::SHOOTING_BRAKE
};

/// Which car is being driven. A resource, like [`super::Setup`], because it is a
/// preference rather than race state — but unlike the setup it gives up the lap
/// in progress when it moves, because half a lap in one car and half in another
/// is not a lap in either. The board and the ghost stay: they belong to the
/// circuit, not to the car.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Spec {
    /// The car as it ships, and the middle of everything.
    #[default]
    Tourer,
    /// Quicker where there are corners.
    Clubman,
    /// Quicker where there is road.
    Express,
}

/// Out of five, in the order the menu lists them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Stars {
    pub handling: u8,
    pub acceleration: u8,
    pub top_speed: u8,
}

/// What one car is: what it is called, what it drives like, what colour it is
/// painted and what it is good at.
#[derive(Clone, Copy)]
pub(crate) struct Sheet {
    pub name: &'static str,
    /// Before the setup slider leans it. See [`super::Setup::applied_to`].
    pub handling: Handling,
    /// The body panels. Everything else about the finish — the metal flake, the
    /// glass, the tyres — comes off the model, because it is the model's.
    pub paint: Color,
    pub stars: Stars,
}

impl Spec {
    /// In the order the menu lists them: the middle one first, because it is the
    /// one the game opens on and the one the other two are read against.
    pub const ALL: [Spec; 3] = [Spec::Tourer, Spec::Clubman, Spec::Express];

    pub fn sheet(self) -> Sheet {
        match self {
            Spec::Tourer => Sheet {
                name: "TOURER",
                handling: TOURER,
                // British racing green.
                paint: Color::srgb(0.04, 0.20, 0.11),
                stars: Stars {
                    handling: 3,
                    acceleration: 3,
                    top_speed: 3,
                },
            },
            Spec::Clubman => Sheet {
                name: "CLUBMAN",
                handling: CLUBMAN,
                // Rosso corsa.
                paint: Color::srgb(0.58, 0.05, 0.04),
                stars: Stars {
                    handling: 4,
                    acceleration: 3,
                    top_speed: 2,
                },
            },
            Spec::Express => Sheet {
                name: "EXPRESS",
                handling: EXPRESS,
                // Silver.
                paint: Color::srgb(0.64, 0.65, 0.68),
                stars: Stars {
                    handling: 2,
                    acceleration: 3,
                    top_speed: 4,
                },
            },
        }
    }

    pub fn name(self) -> &'static str {
        self.sheet().name
    }

    /// What this car drives like, before the setup slider leans it.
    pub fn handling(self) -> Handling {
        self.sheet().handling
    }

    /// Which one, from the top of the list.
    pub fn at(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

impl Stars {
    /// Labelled, in the order the menu lists them.
    pub fn rows(self) -> [(&'static str, u8); 3] {
        [
            ("HANDLING", self.handling),
            ("ACCEL", self.acceleration),
            ("TOP", self.top_speed),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::car::Setup;

    /// The stars are what the driver picks a car on, so they have to be the
    /// numbers and not a description of them. Held as an ordering rather than a
    /// scale: four stars does not have to mean any particular grip figure, but
    /// the car with four of them has to be the one with more of it.
    #[test]
    fn the_stars_say_what_the_numbers_say() {
        for a in Spec::ALL {
            for b in Spec::ALL {
                let (x, y) = (a.sheet(), b.sheet());
                // Grip at speed rather than at rest: a car's cornering is what
                // it has plus what the downforce adds, and both are its claim.
                assert_eq!(
                    x.stars.handling.cmp(&y.stars.handling),
                    x.handling
                        .grip_at(20.0)
                        .total_cmp(&y.handling.grip_at(20.0)),
                    "{} and {} disagree about which holds on better",
                    x.name,
                    y.name
                );
                assert_eq!(
                    x.stars.acceleration.cmp(&y.stars.acceleration),
                    x.handling.accel.total_cmp(&y.handling.accel),
                    "{} and {} disagree about which gets going better",
                    x.name,
                    y.name
                );
                assert_eq!(
                    x.stars.top_speed.cmp(&y.stars.top_speed),
                    x.handling.top_speed.total_cmp(&y.handling.top_speed),
                    "{} and {} disagree about which is faster",
                    x.name,
                    y.name
                );
            }
        }
    }

    /// Three cars, not one car and two ways of spelling it — and no car that is
    /// simply better. Every one of them is best at something and worst at
    /// something else, which is what makes the menu a choice.
    #[test]
    fn every_car_is_best_at_something_and_worst_at_something() {
        for spec in Spec::ALL {
            let mine = spec.sheet().stars;
            let others: Vec<Stars> = Spec::ALL
                .into_iter()
                .filter(|s| *s != spec)
                .map(|s| s.sheet().stars)
                .collect();
            let best = |pick: fn(Stars) -> u8| others.iter().all(|s| pick(*s) <= pick(mine));
            let worst = |pick: fn(Stars) -> u8| others.iter().all(|s| pick(*s) >= pick(mine));
            let dials: [fn(Stars) -> u8; 3] = [|s| s.handling, |s| s.acceleration, |s| s.top_speed];
            assert!(
                dials.iter().any(|&d| best(d)),
                "{} is not the best at anything",
                spec.name()
            );
            assert!(
                dials.iter().any(|&d| worst(d)),
                "{} is not the worst at anything",
                spec.name()
            );
        }
        let names: Vec<&str> = Spec::ALL.iter().map(|s| s.name()).collect();
        assert_eq!(names.len(), 3);
        assert_eq!(Spec::ALL.map(Spec::at), [0, 1, 2]);
    }

    /// The setup slider takes a fraction of what it is given, so a car with a
    /// loose enough rear could be leaned past the point where the throttle takes
    /// away all the grip there is. None of them is, and this is what says so —
    /// `power_lets_go` is a fraction of the grip, and a fraction cannot be more
    /// than the whole.
    #[test]
    fn no_setup_of_any_car_spends_more_grip_than_it_has() {
        for spec in Spec::ALL {
            for notch in Setup::ALL {
                let leaned = notch.applied_to(spec.handling());
                assert!(
                    (0.0..1.0).contains(&leaned.power_lets_go),
                    "{} on {} spends {} of its grip on the throttle",
                    spec.name(),
                    notch.name(),
                    leaned.power_lets_go
                );
                assert!(leaned.handbrake_lets_go < 1.0);
                assert!(leaned.grip > 0.0 && leaned.top_speed > 0.0);
            }
        }
    }
}
