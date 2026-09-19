//! Speed presets, independent of the car and its handling setup.
use bevy::prelude::*;

use super::physics::Handling;

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    Beginner,
    #[default]
    Regular,
    Pro,
}

impl Mode {
    pub const ALL: [Self; 3] = [Self::Beginner, Self::Regular, Self::Pro];

    pub fn name(self) -> &'static str {
        match self {
            Self::Beginner => "Beginner",
            Self::Regular => "Regular",
            Self::Pro => "Pro",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Beginner => Self::Regular,
            Self::Regular => Self::Pro,
            Self::Pro => Self::Beginner,
        }
    }

    pub fn speed(self) -> f32 {
        match self {
            Self::Beginner => 0.8,
            Self::Regular => 1.0,
            Self::Pro => 1.2,
        }
    }

    pub fn applied_to(self, base: Handling) -> Handling {
        let speed = self.speed();
        Handling {
            top_speed: base.top_speed * speed,
            // Drag also determines the speed the car actually settles at.
            // Scale its velocity reference so that flat-road top speed changes
            // by exactly the requested fraction, despite drag and rolling loss.
            drag: base.drag / speed.powi(2),
            reverse_speed: base.reverse_speed * speed,
            ..base
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::car::{Car, Controls, Setup, Spec, Surface, physics};

    #[test]
    fn modes_change_actual_speed_for_every_car_and_setup() {
        let settles_at = |handling: &Handling| {
            let mut car = Car::default();
            for _ in 0..240 * 90 {
                physics::step(
                    &mut car,
                    handling,
                    Vec3::NEG_Z,
                    Vec3::X,
                    Controls {
                        throttle: 1.0,
                        ..default()
                    },
                    Surface {
                        grip: 1.0,
                        slope: 0.0,
                    },
                    1.0 / 240.0,
                );
            }
            car.velocity.length()
        };
        for spec in Spec::ALL {
            for setup in Setup::ALL {
                let base = setup.applied_to(spec.handling());
                assert_eq!(Mode::Regular.applied_to(base), base);
                let regular = settles_at(&base);
                for mode in Mode::ALL {
                    let speed = settles_at(&mode.applied_to(base));
                    assert!(
                        (speed / regular - mode.speed()).abs() < 0.001,
                        "{spec:?} / {setup:?} / {mode:?}: {speed} against {regular}"
                    );
                }
            }
        }
    }
}
