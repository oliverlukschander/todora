//! Leaving the ground.
//!
//! The engine keeps the car on the road: [`crate::track::Track::hold`] sets its
//! height from the ground every step and nothing in it knows what air is. That
//! is left exactly as it is. What this layer adds is a height *above* the road,
//! carried beside the car in an [`Air`], that the model rides on and the camera
//! follows. The car underneath keeps driving along the ground; the thing on top
//! of it is somewhere else.

use bevy::prelude::*;

/// How far off the road a car is, and what it did on the way down.
#[derive(Component, Default, Clone, Copy, Debug)]
pub(crate) struct Air {
    /// Height above the road in game units. Zero on the ground.
    pub height: f32,
    /// Vertical speed, up positive, in game units a second.
    pub vy: f32,
    /// How hard the last landing was, until the pose has used it.
    pub landed: f32,
}
