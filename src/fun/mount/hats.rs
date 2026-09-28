//! Hats. Every mount with a head has a place on it for one.
//!
//! Each is built standing on the origin with its brim or base on the ground
//! plane and the rest going up, so it can be set on any head by moving its
//! joint. The joint is a [`Part::Hat`], which sways on a spring: a hat that
//! stayed put through a corner would be a hat glued on.

use bevy::prelude::*;

use super::{Part, Rest, Spring};
use crate::fun::Hat;
use crate::fun::parts::{Kit, Shape, add, joint};

/// Every paint a hat is made of, made once so that putting one on needs no
/// more than a shared look at the kit.
pub(super) struct Wardrobe {
    pink: Handle<StandardMaterial>,
    yellow: Handle<StandardMaterial>,
    black: Handle<StandardMaterial>,
    red: Handle<StandardMaterial>,
    gold: Handle<StandardMaterial>,
    orange: Handle<StandardMaterial>,
    white: Handle<StandardMaterial>,
    tan: Handle<StandardMaterial>,
    brown: Handle<StandardMaterial>,
    blue: Handle<StandardMaterial>,
    green: Handle<StandardMaterial>,
}

impl Wardrobe {
    pub(super) fn new(kit: &mut Kit, materials: &mut Assets<StandardMaterial>) -> Self {
        let mut colour = |c: Color| kit.paint(materials, c);
        Self {
            pink: colour(Color::srgb(0.98, 0.26, 0.66)),
            yellow: colour(Color::srgb(1.0, 0.86, 0.16)),
            black: colour(Color::srgb(0.04, 0.04, 0.05)),
            red: colour(Color::srgb(0.85, 0.08, 0.10)),
            gold: colour(Color::srgb(1.0, 0.72, 0.10)),
            orange: colour(Color::srgb(1.0, 0.42, 0.04)),
            white: colour(Color::srgb(0.98, 0.98, 0.96)),
            tan: colour(Color::srgb(0.72, 0.50, 0.26)),
            brown: colour(Color::srgb(0.32, 0.18, 0.08)),
            blue: colour(Color::srgb(0.16, 0.42, 0.95)),
            green: colour(Color::srgb(0.20, 0.80, 0.34)),
        }
    }
}

/// Put `hat` on a head whose top is at `at`, scaled by `size` (1 fits the
/// chicken). Nothing is built for a bare head.
pub(super) fn wear(
    parent: &mut ChildSpawnerCommands,
    kit: &Kit,
    wardrobe: &Wardrobe,
    hat: Hat,
    at: Vec3,
    size: f32,
) {
    if matches!(hat, Hat::Bare | Hat::Surprise) {
        return;
    }
    let Wardrobe {
        pink,
        yellow,
        black,
        red,
        gold,
        orange,
        white,
        tan,
        brown,
        blue,
        green,
    } = wardrobe;
    let (pink, yellow, black, red, gold, orange, white, tan, brown, blue, green) =
        (pink, yellow, black, red, gold, orange, white, tan, brown, blue, green);

    let mut hat_joint = joint(parent, at, Quat::IDENTITY);
    hat_joint.insert((
        Part::Hat,
        Rest(Transform::from_translation(at)),
        Spring::default(),
    ));
    hat_joint.with_children(|h| {
        // One more joint to scale the hat as a whole without scaling its rest.
        let mut sized = joint(h, Vec3::ZERO, Quat::IDENTITY);
        sized.insert(Transform::from_scale(Vec3::splat(size)));
        sized.with_children(|s| match hat {
            Hat::Party => {
                add(s, kit, Shape::Cone, pink, Vec3::new(0.0, 0.29, 0.0), Vec3::new(0.36, 0.58, 0.36));
                add(s, kit, Shape::Cylinder, yellow, Vec3::new(0.0, 0.05, 0.0), Vec3::new(0.375, 0.06, 0.375));
                add(s, kit, Shape::Cylinder, blue, Vec3::new(0.0, 0.17, 0.0), Vec3::new(0.30, 0.05, 0.30));
                add(s, kit, Shape::Sphere, yellow, Vec3::new(0.0, 0.60, 0.0), Vec3::splat(0.13));
            }
            Hat::Top => {
                add(s, kit, Shape::Cylinder, black, Vec3::new(0.0, 0.02, 0.0), Vec3::new(0.66, 0.04, 0.66));
                add(s, kit, Shape::Cylinder, black, Vec3::new(0.0, 0.26, 0.0), Vec3::new(0.40, 0.46, 0.40));
                add(s, kit, Shape::Cylinder, red, Vec3::new(0.0, 0.10, 0.0), Vec3::new(0.415, 0.09, 0.415));
            }
            Hat::Crown => {
                add(s, kit, Shape::Cylinder, gold, Vec3::new(0.0, 0.07, 0.0), Vec3::new(0.42, 0.14, 0.42));
                for i in 0..5 {
                    let a = i as f32 / 5.0 * std::f32::consts::TAU;
                    let (x, z) = (a.cos() * 0.17, a.sin() * 0.17);
                    add(s, kit, Shape::Cone, gold, Vec3::new(x, 0.25, z), Vec3::new(0.10, 0.24, 0.10));
                    add(s, kit, Shape::Sphere, red, Vec3::new(x, 0.39, z), Vec3::splat(0.075));
                }
                add(s, kit, Shape::Sphere, blue, Vec3::new(0.0, 0.08, -0.215), Vec3::splat(0.09));
            }
            Hat::Cone => {
                add(s, kit, Shape::Cube, orange, Vec3::new(0.0, 0.015, 0.0), Vec3::new(0.50, 0.03, 0.50));
                add(s, kit, Shape::Cone, orange, Vec3::new(0.0, 0.32, 0.0), Vec3::new(0.40, 0.60, 0.40));
                add(s, kit, Shape::Cylinder, white, Vec3::new(0.0, 0.20, 0.0), Vec3::new(0.30, 0.09, 0.30));
                add(s, kit, Shape::Cylinder, white, Vec3::new(0.0, 0.38, 0.0), Vec3::new(0.20, 0.07, 0.20));
            }
            Hat::Propeller => {
                add(s, kit, Shape::Sphere, red, Vec3::new(0.0, 0.05, 0.0), Vec3::new(0.46, 0.26, 0.46));
                add(s, kit, Shape::Sphere, blue, Vec3::new(0.0, 0.05, -0.12), Vec3::new(0.28, 0.24, 0.30));
                add(s, kit, Shape::Sphere, yellow, Vec3::new(0.0, 0.05, 0.13), Vec3::new(0.28, 0.24, 0.26));
                add(s, kit, Shape::Cylinder, black, Vec3::new(0.0, 0.22, 0.0), Vec3::new(0.03, 0.12, 0.03));
                let mut spin = joint(s, Vec3::new(0.0, 0.29, 0.0), Quat::IDENTITY);
                spin.insert((Part::Spin(1.0), Rest(Transform::from_xyz(0.0, 0.29, 0.0))));
                spin.with_children(|p| {
                    add(p, kit, Shape::Cube, green, Vec3::ZERO, Vec3::new(0.78, 0.015, 0.09));
                    add(p, kit, Shape::Cube, yellow, Vec3::ZERO, Vec3::new(0.09, 0.015, 0.78));
                    add(p, kit, Shape::Sphere, red, Vec3::new(0.0, 0.02, 0.0), Vec3::splat(0.07));
                });
            }
            Hat::Chef => {
                add(s, kit, Shape::Cylinder, white, Vec3::new(0.0, 0.11, 0.0), Vec3::new(0.40, 0.22, 0.40));
                for (x, z, r) in [(0.0, 0.0, 0.34), (0.13, 0.08, 0.26), (-0.13, 0.08, 0.26), (0.0, -0.14, 0.26)] {
                    add(s, kit, Shape::Sphere, white, Vec3::new(x, 0.30, z), Vec3::splat(r));
                }
                add(s, kit, Shape::Cylinder, blue, Vec3::new(0.0, 0.03, 0.0), Vec3::new(0.415, 0.06, 0.415));
            }
            Hat::Cowboy => {
                add(s, kit, Shape::Cylinder, tan, Vec3::new(0.0, 0.02, 0.0), Vec3::new(0.92, 0.035, 0.66));
                add(s, kit, Shape::Sphere, tan, Vec3::new(0.0, 0.16, 0.0), Vec3::new(0.36, 0.34, 0.36));
                add(s, kit, Shape::Cylinder, brown, Vec3::new(0.0, 0.08, 0.0), Vec3::new(0.375, 0.07, 0.375));
                add(s, kit, Shape::Sphere, gold, Vec3::new(0.0, 0.08, -0.19), Vec3::new(0.07, 0.06, 0.03));
            }
            Hat::Bare | Hat::Surprise => {}
        });
    });
}
