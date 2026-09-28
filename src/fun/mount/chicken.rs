//! The chicken.
//!
//! A rooster, mostly: white, with a red comb and wattle, a fan of green-black
//! tail feathers, yellow legs that actually run, and wings that come out when it
//! is airborne or has been given something to be excited about. It wears a
//! saddle in the car's paint and a hat if it has one. Its head does the thing a
//! chicken's head does — stays where it is while the rest of it goes past — and
//! its eyes, if they are googly, slosh about under the corners.
//!
//! Model units are the car's: about 2.6 long, so a chicken is about the same
//! footprint as the thing it replaces, and tall.

use bevy::prelude::*;

use super::{Look, Part, Rest, Spring, animated, hats, hinge, rider};
use crate::fun::parts::{Kit, Shape, add, add_turned};

/// Build the chicken and its rider into `rig`.
pub(super) fn build(
    rig: &mut ChildSpawnerCommands,
    kit: &mut Kit,
    materials: &mut Assets<StandardMaterial>,
    look: &Look,
) {
    let mut paint = |c: Color| kit.paint(materials, c);
    let feathers = paint(Color::srgb(0.98, 0.96, 0.92));
    let shade = paint(Color::srgb(0.91, 0.87, 0.80));
    let red = paint(Color::srgb(0.92, 0.08, 0.12));
    let beak = paint(Color::srgb(1.0, 0.64, 0.06));
    let leg = paint(Color::srgb(1.0, 0.76, 0.14));
    let dark = paint(Color::srgb(0.04, 0.20, 0.19));
    let green = paint(Color::srgb(0.10, 0.48, 0.34));
    let rust = paint(Color::srgb(0.82, 0.30, 0.07));
    let white = paint(Color::srgb(1.0, 1.0, 1.0));
    let black = paint(Color::srgb(0.02, 0.02, 0.03));
    let saddle = paint(look.paint);
    let trim = paint(Color::srgb(1.0, 0.44, 0.06));
    let wardrobe = hats::Wardrobe::new(kit, materials);
    let outfit = rider::Outfit::new(kit, materials, look.paint);

    let kit = &*kit;

    // The body, and the breast pushed out in front of it.
    add(rig, kit, Shape::Sphere, &feathers, Vec3::new(0.0, 1.46, 0.08), Vec3::new(1.26, 1.30, 1.92));
    add(rig, kit, Shape::Sphere, &feathers, Vec3::new(0.0, 1.52, -0.52), Vec3::new(1.06, 1.14, 1.10));
    // A tuft of belly feathers, so the legs come out of something.
    add(rig, kit, Shape::Sphere, &shade, Vec3::new(0.0, 0.98, 0.10), Vec3::new(0.95, 0.42, 1.20));

    // The saddle blanket, in the car's paint with the orange piping.
    add(rig, kit, Shape::Sphere, &saddle, Vec3::new(0.0, 2.03, 0.08), Vec3::new(1.02, 0.20, 1.12));
    add(rig, kit, Shape::Sphere, &trim, Vec3::new(0.0, 2.05, 0.08), Vec3::new(0.22, 0.20, 1.16));

    // Legs: hip, knee, ankle, and three toes forward and one back.
    for (side, phase) in [(-1.0f32, 0.0f32), (1.0, std::f32::consts::PI)] {
        let hip = Vec3::new(side * 0.36, 1.06, 0.14);
        hinge(rig, hip, Part::Hip(side, phase)).with_children(|h| {
            add(h, kit, Shape::Sphere, &feathers, Vec3::new(0.0, -0.10, 0.0), Vec3::new(0.46, 0.66, 0.52));
            hinge(h, Vec3::new(0.0, -0.36, 0.0), Part::Knee(side, phase)).with_children(|k| {
                add(k, kit, Shape::Cylinder, &leg, Vec3::new(0.0, -0.30, 0.0), Vec3::new(0.10, 0.62, 0.10));
                hinge(k, Vec3::new(0.0, -0.60, 0.0), Part::Ankle(side, phase)).with_children(|a| {
                    add(a, kit, Shape::Sphere, &leg, Vec3::ZERO, Vec3::splat(0.14));
                    for toe in [-0.42f32, 0.0, 0.42] {
                        add_turned(
                            a,
                            kit,
                            Shape::Capsule,
                            &leg,
                            Vec3::new(toe * 0.33, -0.01, -0.20),
                            Vec3::new(0.075, 0.30, 0.075),
                            Quat::from_rotation_y(-toe) * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
                        );
                    }
                    add_turned(
                        a,
                        kit,
                        Shape::Capsule,
                        &leg,
                        Vec3::new(0.0, -0.01, 0.10),
                        Vec3::new(0.07, 0.12, 0.07),
                        Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                    );
                });
            });
        });
    }

    // Wings: folded against the body, with a fan of long feathers at the tip
    // that only shows when the wing opens.
    for side in [-1.0f32, 1.0] {
        let shoulder = Vec3::new(side * 0.64, 1.86, 0.02);
        hinge(rig, shoulder, Part::Wing(side)).with_children(|w| {
            add_turned(
                w,
                kit,
                Shape::Sphere,
                &shade,
                Vec3::new(side * 0.06, -0.40, 0.14),
                Vec3::new(0.16, 0.92, 1.12),
                Quat::from_rotation_x(0.16),
            );
            for i in 0..5 {
                let f = i as f32 / 4.0;
                add_turned(
                    w,
                    kit,
                    Shape::Sphere,
                    if i % 2 == 0 { &feathers } else { &shade },
                    Vec3::new(side * 0.06, -0.86 - f * 0.10, 0.62 - f * 0.28),
                    Vec3::new(0.09, 0.62, 0.20),
                    Quat::from_rotation_x(0.14 + f * 0.05) * Quat::from_rotation_z(side * (f - 0.5) * 0.30),
                );
            }
        });
    }

    // The tail: a fan of long feathers on a hinge at the base.
    hinge(rig, Vec3::new(0.0, 1.66, 0.94), Part::Tail).with_children(|t| {
        for i in 0..7u8 {
            let spread = (f32::from(i) - 3.0) * 0.30;
            let lift = 0.55 + 0.11 * (3.0 - (f32::from(i) - 3.0).abs());
            let rest = Quat::from_rotation_x(lift) * Quat::from_rotation_z(spread);
            let paint = match i {
                3 => &rust,
                1 | 5 => &green,
                _ => &dark,
            };
            let length = 1.15 - 0.09 * (f32::from(i) - 3.0).abs();
            let mut feather = t.spawn((
                Transform::from_rotation(rest),
                Visibility::Inherited,
                Part::Feather(i),
                Rest(Transform::from_rotation(rest)),
            ));
            feather.with_children(|f| {
                add(f, kit, Shape::Sphere, paint, Vec3::new(0.0, length * 0.5, 0.0), Vec3::new(0.24, length, 0.07));
            });
        }
    });

    // The neck rises from the breast and the head sits on top of it.
    hinge(rig, Vec3::new(0.0, 1.98, -0.66), Part::Neck).with_children(|n| {
        add(n, kit, Shape::Capsule, &feathers, Vec3::new(0.0, 0.40, 0.0), Vec3::new(0.32, 0.56, 0.32));
        hinge(n, Vec3::new(0.0, 0.92, 0.0), Part::Head).with_children(|h| {
            add(h, kit, Shape::Sphere, &feathers, Vec3::ZERO, Vec3::splat(0.68));
            // The beak: an upper half fixed, a lower half on a hinge.
            add_turned(
                h,
                kit,
                Shape::Cone,
                &beak,
                Vec3::new(0.0, 0.02, -0.52),
                Vec3::new(0.34, 0.52, 0.24),
                Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            );
            hinge(h, Vec3::new(0.0, -0.07, -0.28), Part::Jaw).with_children(|j| {
                add_turned(
                    j,
                    kit,
                    Shape::Cone,
                    &beak,
                    Vec3::new(0.0, 0.0, -0.20),
                    Vec3::new(0.26, 0.38, 0.14),
                    Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
                );
            });
            // The wattle under the chin, and the comb on top.
            let mut wattle = animated(h, kit, Shape::Sphere, &red, Vec3::new(0.0, -0.31, -0.32), Vec3::new(0.11, 0.22, 0.10), Part::Wattle);
            wattle.insert(Spring::default());
            hinge(h, Vec3::new(0.0, 0.30, 0.0), Part::Comb).insert(Spring::default()).with_children(|c| {
                for (z, h_, w) in [(-0.17, 0.14, 0.10), (-0.06, 0.25, 0.11), (0.06, 0.22, 0.11), (0.17, 0.13, 0.10)] {
                    add(c, kit, Shape::Sphere, &red, Vec3::new(0.0, h_ * 0.5, z), Vec3::new(w, h_, 0.16));
                }
            });
            // The eyes. Googly ones are big, white, and loose.
            for side in [-1.0f32, 1.0] {
                if look.eyes {
                    add(h, kit, Shape::Sphere, &white, Vec3::new(side * 0.23, 0.11, -0.22), Vec3::splat(0.30));
                    let mut pupil = animated(
                        h,
                        kit,
                        Shape::Sphere,
                        &black,
                        Vec3::new(side * 0.235, 0.085, -0.35),
                        Vec3::new(0.14, 0.14, 0.05),
                        Part::Pupil(side),
                    );
                    pupil.insert(Spring::default());
                } else {
                    add(h, kit, Shape::Sphere, &black, Vec3::new(side * 0.24, 0.08, -0.17), Vec3::splat(0.11));
                    add(h, kit, Shape::Sphere, &white, Vec3::new(side * 0.26, 0.11, -0.21), Vec3::splat(0.04));
                }
            }
            hats::wear(h, kit, &wardrobe, look.hat, Vec3::new(0.0, 0.44, 0.0), 1.1);
        });
    });

    rider::seat(
        rig,
        kit,
        &outfit,
        Vec3::new(0.0, 2.12, 0.02),
        Vec3::new(0.0, 2.15, -0.66),
    );
}
