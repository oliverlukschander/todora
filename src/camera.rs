use bevy::{
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
};

use crate::Reset;
use crate::car::{Car, level};
use crate::track::Track;
use crate::world::SKY;

// Framed on the car, which is about 1.9 m long.
const BACK: f32 = 6.8;
const HEIGHT: f32 = 3.5;
const LOOK_AHEAD: f32 = 1.8;
const LOOK_HEIGHT: f32 = 0.6;
const FOLLOW_XZ: f32 = 6.5;
const FOLLOW_Y: f32 = 11.0;
const ZOOM_MIN: f32 = 0.35;
const ZOOM_MAX: f32 = 2.4;
const ZOOM_STEP: f32 = 0.12;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, (zoom.run_if(crate::pause::running), follow).chain());
    }
}

#[derive(Component)]
struct FollowCam {
    zoom: f32,
}

fn setup(mut commands: Commands, track: Res<Track>) {
    let pose = track.start_transform();
    let start = pose.translation - *pose.forward() * BACK + Vec3::Y * HEIGHT;
    let look = pose.translation + *pose.forward() * LOOK_AHEAD + Vec3::Y * LOOK_HEIGHT;
    commands.spawn((
        Camera3d::default(),
        FollowCam { zoom: 1.0 },
        Transform::from_translation(start).looking_at(look, Vec3::Y),
        // The loft stops 20 m off the centreline; haze rather than a hard edge.
        DistanceFog {
            color: SKY,
            falloff: FogFalloff::Linear {
                start: 130.0,
                end: 380.0,
            },
            ..default()
        },
    ));
}

fn zoom(scroll: Res<AccumulatedMouseScroll>, mut cameras: Query<&mut FollowCam>) {
    let ticks = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
    };
    if ticks.abs() <= f32::EPSILON {
        return;
    }
    for mut camera in &mut cameras {
        camera.zoom = (camera.zoom * (1.0 - ticks * ZOOM_STEP)).clamp(ZOOM_MIN, ZOOM_MAX);
    }
}

fn follow(
    time: Res<Time>,
    mut resets: MessageReader<Reset>,
    cars: Query<&Transform, (With<Car>, Without<FollowCam>)>,
    mut cameras: Query<(&FollowCam, &mut Transform), Without<Car>>,
) {
    // The car has been put back on the grid, which is somewhere else — and on a
    // switch, somewhere else entirely. Chasing it there means a second of flying
    // across the circuit, or between two of them.
    let cut = resets.read().next().is_some();
    let Ok(car) = cars.single() else {
        return;
    };
    let Ok((follow, mut camera)) = cameras.single_mut() else {
        return;
    };
    let dt = time.delta_secs();
    // The car lies along the slope; the camera must not. Hanging the boom off
    // the pitched nose lifts it a metre and tilts it ten degrees steeper on a
    // descent, which leaves the driver looking at the roof with the corner
    // ahead crushed into the bottom of the frame.
    let ahead = level(*car.forward());
    let desired = car.translation - ahead * (BACK * follow.zoom) + Vec3::Y * (HEIGHT * follow.zoom);
    let t_xz = if cut {
        1.0
    } else {
        1.0 - (-FOLLOW_XZ * dt).exp()
    };
    let t_y = if cut {
        1.0
    } else {
        1.0 - (-FOLLOW_Y * dt).exp()
    };
    camera.translation.x = camera.translation.x.lerp(desired.x, t_xz);
    camera.translation.z = camera.translation.z.lerp(desired.z, t_xz);
    camera.translation.y = camera.translation.y.lerp(desired.y, t_y);

    let look = car.translation + ahead * LOOK_AHEAD + Vec3::Y * LOOK_HEIGHT;
    camera.look_at(look, Vec3::Y);
}
