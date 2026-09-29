//! The other things you can ride, none of which is a chicken.
//!
//! All three are on the car's own four wheels and turn them the way the car
//! does, so however daft the shape the thing under the rider still steers.

use bevy::prelude::*;

use super::{Look, Part, Rest, animated, hats, hinge, rider};
use crate::car::{FRONT_AXLE, HALF_TRACK, REAR_AXLE, SCALE, WHEEL_WIDTH};
use crate::fun::parts::{Kit, Shape, add, add_turned};

/// The four wheels, on the car's own axles: it is the car's wheelbase and track
/// that the physics thinks it is driving.
fn wheels(
    rig: &mut ChildSpawnerCommands,
    kit: &Kit,
    tyre: &Handle<StandardMaterial>,
    hub: &Handle<StandardMaterial>,
    radius: f32,
) {
    let axle = |front: bool| {
        if front {
            -FRONT_AXLE / SCALE
        } else {
            REAR_AXLE / SCALE
        }
    };
    for front in [true, false] {
        for side in [-1.0f32, 1.0] {
            let at = Vec3::new(side * HALF_TRACK / SCALE, radius, axle(front));
            let mut wheel = hinge(rig, at, Part::Wheel(if front { 1.0 } else { 0.0 }));
            wheel.with_children(|w| {
                let width = WHEEL_WIDTH / SCALE;
                add_turned(
                    w,
                    kit,
                    Shape::Cylinder,
                    tyre,
                    Vec3::ZERO,
                    Vec3::new(radius * 2.0, width, radius * 2.0),
                    Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                );
                add_turned(
                    w,
                    kit,
                    Shape::Cylinder,
                    hub,
                    Vec3::new(side * width * 0.5, 0.0, 0.0),
                    Vec3::new(radius * 1.1, 0.02, radius * 1.1),
                    Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                );
            });
        }
    }
}

/// A rubber duck, the size of a small car.
pub(super) fn duck(
    rig: &mut ChildSpawnerCommands,
    kit: &mut Kit,
    materials: &mut Assets<StandardMaterial>,
    look: &Look,
) {
    let mut paint = |c: Color| kit.paint(materials, c);
    let yellow = paint(Color::srgb(1.0, 0.85, 0.10));
    let orange = paint(Color::srgb(1.0, 0.50, 0.05));
    let black = paint(Color::srgb(0.02, 0.02, 0.03));
    let white = paint(Color::WHITE);
    let tyre = paint(Color::srgb(0.06, 0.06, 0.07));
    let hub = paint(Color::srgb(0.75, 0.78, 0.80));
    let wardrobe = hats::Wardrobe::new(kit, materials);
    let outfit = rider::Outfit::new(kit, materials, look.paint);
    let kit = &*kit;

    wheels(rig, kit, &tyre, &hub, 0.22);
    let mut body = animated(
        rig,
        kit,
        Shape::Sphere,
        &yellow,
        Vec3::new(0.0, 0.95, 0.20),
        Vec3::new(1.55, 1.15, 2.15),
        Part::Chassis,
    );
    body.with_children(|_| {});
    add(
        rig,
        kit,
        Shape::Sphere,
        &yellow,
        Vec3::new(0.0, 1.05, -0.62),
        Vec3::new(1.20, 1.0, 1.0),
    );
    // A tail flick, a head, a beak.
    add_turned(
        rig,
        kit,
        Shape::Cone,
        &yellow,
        Vec3::new(0.0, 1.42, 1.28),
        Vec3::new(0.46, 0.66, 0.46),
        Quat::from_rotation_x(0.9),
    );
    hinge(rig, Vec3::new(0.0, 1.80, -0.82), Part::Head).with_children(|h| {
        add(
            h,
            kit,
            Shape::Sphere,
            &yellow,
            Vec3::ZERO,
            Vec3::splat(0.98),
        );
        add(
            h,
            kit,
            Shape::Sphere,
            &orange,
            Vec3::new(0.0, -0.10, -0.50),
            Vec3::new(0.62, 0.16, 0.48),
        );
        for side in [-1.0f32, 1.0] {
            add(
                h,
                kit,
                Shape::Sphere,
                &white,
                Vec3::new(side * 0.30, 0.16, -0.36),
                Vec3::splat(0.26),
            );
            add(
                h,
                kit,
                Shape::Sphere,
                &black,
                Vec3::new(side * 0.31, 0.14, -0.47),
                Vec3::splat(0.14),
            );
        }
        hats::wear(h, kit, &wardrobe, look.hat, Vec3::new(0.0, 0.48, 0.0), 1.5);
    });
    for side in [-1.0f32, 1.0] {
        add_turned(
            rig,
            kit,
            Shape::Sphere,
            &yellow,
            Vec3::new(side * 0.80, 1.05, 0.30),
            Vec3::new(0.14, 0.62, 0.95),
            Quat::from_rotation_z(side * 0.25),
        );
    }
    rider::seat(
        rig,
        kit,
        &outfit,
        Vec3::new(0.0, 1.62, 0.55),
        Vec3::new(0.0, 1.62, -0.40),
    );
    let _ = Rest(Transform::IDENTITY);
}

/// A bathtub, on wheels, with a duck in it and a rider who has chosen this.
pub(super) fn tub(
    rig: &mut ChildSpawnerCommands,
    kit: &mut Kit,
    materials: &mut Assets<StandardMaterial>,
    look: &Look,
) {
    let mut paint = |c: Color| kit.paint(materials, c);
    let china = paint(Color::srgb(0.96, 0.97, 0.98));
    let water = paint(Color::srgb(0.30, 0.68, 0.98));
    let chrome = paint(Color::srgb(0.78, 0.80, 0.84));
    let yellow = paint(Color::srgb(1.0, 0.85, 0.10));
    let orange = paint(Color::srgb(1.0, 0.50, 0.05));
    let tyre = paint(Color::srgb(0.06, 0.06, 0.07));
    let bubbles = paint(Color::srgb(0.95, 0.99, 1.0));
    let outfit = rider::Outfit::new(kit, materials, look.paint);
    let wardrobe = hats::Wardrobe::new(kit, materials);
    let kit = &*kit;

    wheels(rig, kit, &tyre, &chrome, 0.24);
    animated(
        rig,
        kit,
        Shape::Cube,
        &china,
        Vec3::new(0.0, 0.72, 0.10),
        Vec3::new(1.45, 0.72, 2.30),
        Part::Chassis,
    )
    .with_children(|hull| {
        // The inside: a scoop of water that sloshes, and its bubbles.
        let _ = hull;
    });
    add(
        rig,
        kit,
        Shape::Sphere,
        &china,
        Vec3::new(0.0, 0.72, -1.05),
        Vec3::new(1.45, 0.72, 0.75),
    );
    add(
        rig,
        kit,
        Shape::Sphere,
        &china,
        Vec3::new(0.0, 0.72, 1.25),
        Vec3::new(1.45, 0.72, 0.75),
    );
    animated(
        rig,
        kit,
        Shape::Cube,
        &water,
        Vec3::new(0.0, 1.04, 0.10),
        Vec3::new(1.15, 0.05, 2.35),
        Part::Slosh,
    );
    for (x, y, z, r) in [
        (0.3, 1.12, -0.6, 0.30),
        (-0.35, 1.14, 0.2, 0.36),
        (0.2, 1.12, 0.8, 0.28),
        (-0.1, 1.16, -0.2, 0.26),
    ] {
        add(
            rig,
            kit,
            Shape::Sphere,
            &bubbles,
            Vec3::new(x, y, z),
            Vec3::splat(r),
        );
    }
    // The taps, at the back, and a plug on a chain at the front.
    add(
        rig,
        kit,
        Shape::Cylinder,
        &chrome,
        Vec3::new(0.0, 1.35, 1.35),
        Vec3::new(0.10, 0.6, 0.10),
    );
    add_turned(
        rig,
        kit,
        Shape::Cylinder,
        &chrome,
        Vec3::new(0.0, 1.62, 1.12),
        Vec3::new(0.09, 0.5, 0.09),
        Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
    );
    // The duck along for the ride.
    add(
        rig,
        kit,
        Shape::Sphere,
        &yellow,
        Vec3::new(0.0, 1.22, -0.85),
        Vec3::new(0.42, 0.34, 0.5),
    );
    hinge(rig, Vec3::new(0.0, 1.46, -0.98), Part::Head).with_children(|h| {
        add(
            h,
            kit,
            Shape::Sphere,
            &yellow,
            Vec3::ZERO,
            Vec3::splat(0.30),
        );
        add(
            h,
            kit,
            Shape::Sphere,
            &orange,
            Vec3::new(0.0, -0.03, -0.17),
            Vec3::new(0.20, 0.06, 0.16),
        );
        hats::wear(h, kit, &wardrobe, look.hat, Vec3::new(0.0, 0.14, 0.0), 0.5);
    });
    rider::seat(
        rig,
        kit,
        &outfit,
        Vec3::new(0.0, 1.05, 0.55),
        Vec3::new(0.0, 1.30, -0.15),
    );
}

/// A shopping trolley: a basket on a frame, a bar to hold, wonky wheels.
pub(super) fn cart(
    rig: &mut ChildSpawnerCommands,
    kit: &mut Kit,
    materials: &mut Assets<StandardMaterial>,
    look: &Look,
) {
    let mut paint = |c: Color| kit.paint(materials, c);
    let steel = paint(Color::srgb(0.72, 0.75, 0.80));
    let red = paint(Color::srgb(0.90, 0.10, 0.14));
    let tyre = paint(Color::srgb(0.06, 0.06, 0.07));
    let bread = paint(Color::srgb(0.82, 0.60, 0.30));
    let lettuce = paint(Color::srgb(0.30, 0.78, 0.30));
    let wardrobe = hats::Wardrobe::new(kit, materials);
    let outfit = rider::Outfit::new(kit, materials, look.paint);
    let kit = &*kit;

    wheels(rig, kit, &tyre, &steel, 0.18);
    // The frame under the basket.
    animated(
        rig,
        kit,
        Shape::Cube,
        &steel,
        Vec3::new(0.0, 0.46, 0.10),
        Vec3::new(1.20, 0.06, 2.20),
        Part::Chassis,
    );
    // The basket: a grid of bars on the bottom, and bars up each side.
    for i in 0..7 {
        let z = -0.95 + i as f32 * 0.32;
        add(
            rig,
            kit,
            Shape::Cube,
            &steel,
            Vec3::new(0.0, 0.72, z),
            Vec3::new(1.16, 0.03, 0.03),
        );
    }
    for i in 0..5 {
        let x = -0.55 + i as f32 * 0.275;
        add(
            rig,
            kit,
            Shape::Cube,
            &steel,
            Vec3::new(x, 0.72, 0.10),
            Vec3::new(0.03, 0.03, 2.0),
        );
    }
    for side in [-1.0f32, 1.0] {
        for z in [-0.95, -0.35, 0.25, 0.85] {
            add(
                rig,
                kit,
                Shape::Cube,
                &steel,
                Vec3::new(side * 0.6, 1.02, z),
                Vec3::new(0.03, 0.62, 0.03),
            );
        }
        add(
            rig,
            kit,
            Shape::Cube,
            &steel,
            Vec3::new(side * 0.6, 1.33, 0.0),
            Vec3::new(0.03, 0.03, 2.0),
        );
        add(
            rig,
            kit,
            Shape::Cube,
            &steel,
            Vec3::new(side * 0.6, 0.92, 0.0),
            Vec3::new(0.03, 0.03, 2.0),
        );
    }
    add(
        rig,
        kit,
        Shape::Cube,
        &steel,
        Vec3::new(0.0, 1.33, -1.02),
        Vec3::new(1.2, 0.03, 0.03),
    );
    // The handle, in red, at the back, and a flag on a whippy pole.
    add(
        rig,
        kit,
        Shape::Cube,
        &red,
        Vec3::new(0.0, 1.45, 1.15),
        Vec3::new(1.26, 0.09, 0.09),
    );
    // The shopping.
    add(
        rig,
        kit,
        Shape::Sphere,
        &bread,
        Vec3::new(-0.25, 0.92, -0.6),
        Vec3::new(0.5, 0.3, 0.32),
    );
    add(
        rig,
        kit,
        Shape::Sphere,
        &lettuce,
        Vec3::new(0.25, 0.96, -0.5),
        Vec3::splat(0.36),
    );
    hinge(rig, Vec3::new(0.0, 1.6, -1.0), Part::Head).with_children(|h| {
        hats::wear(h, kit, &wardrobe, look.hat, Vec3::ZERO, 0.8);
    });
    rider::seat(
        rig,
        kit,
        &outfit,
        Vec3::new(0.0, 0.92, 0.30),
        Vec3::new(0.0, 1.36, -0.05),
    );
}
