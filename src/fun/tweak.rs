//! Changes to how the car drives, made from outside the engine.
//!
//! The engine is never edited. It is given a [`Handling`](crate::car::Handling)
//! each step, and this is where that value gets changed on its way in: scaled
//! for speed, loosened on ice, pushed by a boost. When nothing is asked for the
//! value passes through untouched, bit for bit, which is what
//! `an_unasked_for_tweak_changes_nothing` holds.

use bevy::prelude::*;

/// A boost in progress.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub(crate) struct Boost {
    /// Seconds left.
    pub left: f32,
    /// How hard, 0 to 1.
    pub power: f32,
}

impl Boost {
    /// How much boost there is right now, easing out over the last moments.
    pub(crate) fn strength(&self) -> f32 {
        if self.left <= 0.0 {
            0.0
        } else {
            self.power * (self.left / 0.5).min(1.0)
        }
    }
}
