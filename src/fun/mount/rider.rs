//! The one who is actually driving.
//!
//! A small racer in the car's paint with the orange trim the Omarchy livery
//! has, a helmet with a dark visor, boots, and a scarf that streams. The same
//! rider sits on every mount; only the seat changes.

use bevy::prelude::*;

use super::{Part, Spring, hinge, limb};
use crate::fun::parts::{Kit, Shape, add, add_turned};

/// How many sections the scarf has.
pub(super) const SCARF: u8 = 6;

/// The rider's paints, made once.
pub(super) struct Outfit {
    suit: Handle<StandardMaterial>,
    orange: Handle<StandardMaterial>,
    skin: Handle<StandardMaterial>,
    helmet: Handle<StandardMaterial>,
    visor: Handle<StandardMaterial>,
    boot: Handle<StandardMaterial>,
    glove: Handle<StandardMaterial>,
}

impl Outfit {
    pub(super) fn new(
        kit: &mut Kit,
        materials: &mut Assets<StandardMaterial>,
        paint: Color,
    ) -> Self {
        let mut colour = |c: Color| kit.paint(materials, c);
        Self {
            suit: colour(paint),
            orange: colour(Color::srgb(1.0, 0.44, 0.06)),
            skin: colour(Color::srgb(0.94, 0.72, 0.56)),
            helmet: colour(Color::srgb(0.96, 0.97, 0.98)),
            visor: colour(Color::srgb(0.03, 0.05, 0.09)),
            boot: colour(Color::srgb(0.10, 0.10, 0.12)),
            glove: colour(Color::srgb(0.12, 0.12, 0.14)),
        }
    }
}

/// Seat a rider at `seat`, facing -Z, hands reaching for `reins`, both in the
/// mount's own frame.
pub(super) fn seat(
    rig: &mut ChildSpawnerCommands,
    kit: &Kit,
    outfit: &Outfit,
    seat: Vec3,
    reins: Vec3,
) {
    let Outfit {
        suit,
        orange,
        skin,
        helmet,
        visor,
        boot,
        glove,
    } = outfit;

    let mut rider = hinge(rig, seat, Part::Rider);
    rider.with_children(|r| {
        // Hips, a belt, and a torso leaning into the wind.
        add(r, kit, Shape::Sphere, suit, Vec3::new(0.0, 0.08, 0.0), Vec3::new(0.56, 0.34, 0.50));
        add(r, kit, Shape::Cylinder, orange, Vec3::new(0.0, 0.22, -0.02), Vec3::new(0.535, 0.07, 0.50));
        limb(r, kit, suit, Vec3::new(0.0, 0.16, 0.02), Vec3::new(0.0, 0.50, -0.10), 0.52);
        // A stripe down the chest.
        add_turned(
            r,
            kit,
            Shape::Sphere,
            orange,
            Vec3::new(0.0, 0.38, -0.20),
            Vec3::new(0.10, 0.36, 0.10),
            Quat::from_rotation_x(-0.30),
        );
        add(r, kit, Shape::Sphere, skin, Vec3::new(0.0, 0.62, -0.14), Vec3::splat(0.18));

        // The head: a face, a helmet over it, a visor across the front.
        let mut head = hinge(r, Vec3::new(0.0, 0.84, -0.18), Part::RiderHead);
        head.insert(Spring::default());
        head.with_children(|h| {
            add(h, kit, Shape::Sphere, skin, Vec3::ZERO, Vec3::splat(0.46));
            add(h, kit, Shape::Sphere, helmet, Vec3::new(0.0, 0.06, 0.03), Vec3::new(0.70, 0.66, 0.72));
            add(h, kit, Shape::Sphere, visor, Vec3::new(0.0, 0.0, -0.25), Vec3::new(0.56, 0.29, 0.32));
            add_turned(
                h,
                kit,
                Shape::Sphere,
                orange,
                Vec3::new(0.0, 0.36, 0.02),
                Vec3::new(0.09, 0.05, 0.68),
                Quat::IDENTITY,
            );
            // Two little ear-pods, because helmets have them.
            for side in [-1.0, 1.0] {
                add(h, kit, Shape::Sphere, orange, Vec3::new(side * 0.32, 0.02, 0.05), Vec3::new(0.06, 0.15, 0.15));
            }
        });

        // Arms out to the reins.
        for side in [-1.0f32, 1.0] {
            let shoulder = Vec3::new(side * 0.30, 0.40, -0.06);
            let hand = reins - seat + Vec3::new(side * 0.16, 0.0, 0.0);
            limb(r, kit, suit, shoulder, hand, 0.16);
            add(r, kit, Shape::Sphere, glove, hand, Vec3::splat(0.18));
        }
        // Legs astride, boots at the sides.
        for side in [-1.0f32, 1.0] {
            let hip = Vec3::new(side * 0.20, 0.06, 0.0);
            let knee = Vec3::new(side * 0.52, 0.02, -0.30);
            let foot = Vec3::new(side * 0.60, -0.40, -0.24);
            limb(r, kit, suit, hip, knee, 0.20);
            limb(r, kit, suit, knee, foot, 0.18);
            add(r, kit, Shape::Sphere, boot, foot + Vec3::new(0.0, -0.04, -0.06), Vec3::new(0.20, 0.17, 0.34));
        }

        // The scarf: a chain of hinges, each a little further down the wind.
        let mut root = hinge(r, Vec3::new(0.0, 0.56, 0.08), Part::Scarf(0));
        let mut section = 0;
        root.with_children(|s| chain(s, kit, orange, suit, &mut section));
    });
}

/// One section, and inside it the next.
fn chain(
    parent: &mut ChildSpawnerCommands,
    kit: &Kit,
    orange: &Handle<StandardMaterial>,
    suit: &Handle<StandardMaterial>,
    section: &mut u8,
) {
    let at = *section;
    let length = 0.22;
    let paint = if at % 2 == 0 { orange } else { suit };
    add(
        parent,
        kit,
        Shape::Cube,
        paint,
        Vec3::new(0.0, 0.0, length * 0.5),
        Vec3::new(0.15 * (1.0 - at as f32 * 0.06), 0.03, length * 1.04),
    );
    if at + 1 < SCARF {
        *section += 1;
        let next = *section;
        let mut joint = hinge(parent, Vec3::new(0.0, 0.0, length), Part::Scarf(next));
        joint.with_children(|s| chain(s, kit, orange, suit, section));
    }
}
