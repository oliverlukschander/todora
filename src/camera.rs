use bevy::{
    input::mouse::{AccumulatedMouseScroll, MouseScrollUnit},
    prelude::*,
};

use crate::Reset;
use crate::car::{Car, level};
use crate::track::Track;
use crate::world::SKY;

// A close, low chase view with room to read the road ahead.
const BACK: f32 = 6.0;
const HEIGHT: f32 = 2.8;
const LOOK_AHEAD: f32 = 1.8;
const LOOK_HEIGHT: f32 = 0.6;
const FOLLOW_XZ: f32 = 6.5;
const FOLLOW_Y: f32 = 11.0;
const ZOOM_MIN: f32 = 0.35;
const ZOOM_MAX: f32 = 2.4;
const ZOOM_STEP: f32 = 0.12;
/// Least height over the ground beneath the camera, scaled down with a closer
/// zoom but not up with a wider one, where the sight line is what matters. The
/// level boom clears flat road by [`HEIGHT`]; this only bites once the road
/// behind the car stands a metre higher, which is a descent of 16% or more at
/// the resting distance and a gentler one when speed stretches the boom.
const ABOVE_GROUND: f32 = 1.8;
/// Never closer to the ground than this, whatever the zoom: the near plane.
const FLOOR: f32 = 0.5;
/// The point of the car that has to stay in sight: the roof, about.
const SIGHT: f32 = 0.35;
/// How far the sight line to it keeps above crests and banks in between.
const SIGHT_CLEAR: f32 = 0.3;
/// How much of the sight line is checked, from the camera towards the car. The
/// rest is the road the car stands on.
const SIGHT_REACH: f32 = 0.75;
/// Plan distance between checks along it, fine enough to catch the drop off
/// the edge of a verge, and a cap on how many a stretched, zoomed-out boom gets.
const SIGHT_STEP: f32 = 0.6;
const SIGHT_SAMPLES: usize = 32;
/// A change in the ground between two neighbouring checks that is worth looking
/// between, and how many pieces to look at it in.
const SIGHT_JUMP: f32 = 0.4;
const SIGHT_REFINE: usize = 6;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(
            Update,
            ((zoom, switch_view).run_if(crate::pause::running), follow)
                .chain()
                .run_if(crate::local::solo),
        );
    }
}

#[derive(Component)]
pub(crate) struct FollowCam {
    pub(crate) zoom: f32,
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

/// The driver's eye in the car model's own coordinates (glTF: +Y up, −Z
/// forward), from `tools/make_omarchy_gt.py`: the left seat, just under the
/// roof liner, behind the steering wheel.
const COCKPIT_EYE: Vec3 = Vec3::new(-0.19, 0.60, 0.22);
/// Where the driver looks, in the same coordinates: down the road, a touch
/// below level, which puts the wheel and the dash in the bottom of the frame.
const COCKPIT_LOOK: Vec3 = Vec3::new(0.0, -0.10, -1.0);
/// The near plane in the cockpit, where the wheel is 10 cm from the eye, and
/// on the bonnet, which is closer still; the chase view keeps Bevy's 10 cm.
const COCKPIT_NEAR: f32 = 0.01;
const USUAL_NEAR: f32 = 0.1;
/// The bonnet camera, in the car model's own coordinates like the cockpit's:
/// just above the bonnet between the front wheels, where a real car's is
/// about a metre off the road, looking down it so the nose is in frame.
const BONNET_EYE: Vec3 = Vec3::new(0.0, 0.56, -0.50);
const BONNET_LOOK: Vec3 = Vec3::new(0.0, -0.05, -1.0);

/// V, or the pad's D-pad up, steps through the camera views.
fn switch_view(
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    settings: Option<ResMut<crate::settings::Settings>>,
) {
    let pressed = crate::settings::bindings::current(settings.as_deref()).just(
        &keys,
        &pads,
        crate::settings::bindings::Act::Camera,
    );
    if let (true, Some(mut settings)) = (pressed, settings) {
        settings.camera = settings.camera.next();
    }
}

/// The driver's eye, the way they look and which way is up, for a car at `car`.
fn cockpit(car: &Transform) -> (Vec3, Vec3, Vec3) {
    (
        car.transform_point(COCKPIT_EYE),
        car.rotation * COCKPIT_LOOK.normalize(),
        car.rotation * Vec3::Y,
    )
}

/// Where the bonnet camera is and what it looks at, for a car at `car`.
fn bonnet(car: &Transform) -> (Vec3, Vec3, Vec3) {
    (
        car.transform_point(BONNET_EYE),
        car.rotation * BONNET_LOOK.normalize(),
        car.rotation * Vec3::Y,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn follow(
    time: Res<Time>,
    halt: Option<Res<crate::pause::Halt>>,
    mut resets: MessageReader<Reset>,
    track: Res<Track>,
    settings: Option<Res<crate::settings::Settings>>,
    fun: Option<Res<crate::fun::Fun>>,
    cars: Query<(&Car, &Transform, Option<&crate::fun::air::Air>), Without<FollowCam>>,
    mut cameras: Query<(&FollowCam, &mut Transform, &mut Projection), Without<Car>>,
) {
    // The car has been put back on the grid, which is somewhere else — and on a
    // switch, somewhere else entirely. Chasing it there means a second of flying
    // across the circuit, or between two of them.
    let cut = resets.read().next().is_some();
    // The replay moves the camera itself.
    if halt.is_some_and(|h| matches!(*h, crate::pause::Halt::Replay | crate::pause::Halt::Title)) {
        return;
    }
    let Ok((state, car, air)) = cars.single() else {
        return;
    };
    // Off the road, the camera goes up a little way after the car, and looks up
    // the rest of the way, so that a jump is seen rather than followed.
    let lift = air.map_or(0.0, |air| air.height);
    let up = Vec3::Y * lift;
    // A faster car is followed faster, so that the boom is as long at three
    // times the speed as it was at one.
    let quick = fun.as_ref().map_or(1.0, |fun| fun.speed.scale());
    let chase = fun
        .as_ref()
        .is_none_or(|fun| fun.mount == crate::fun::Mount::Car);
    let Ok((follow, mut camera, mut projection)) = cameras.single_mut() else {
        return;
    };
    let dt = time.delta_secs();
    let view = settings.map_or(crate::settings::CameraView::Far, |s| s.camera);
    // Only the car has a cockpit or a bonnet to sit in.
    let view = if chase {
        view
    } else {
        crate::settings::CameraView::Far
    };
    let near = if view != crate::settings::CameraView::Far {
        COCKPIT_NEAR
    } else {
        USUAL_NEAR
    };
    if let Projection::Perspective(perspective) = projection.as_mut()
        && perspective.near != near
    {
        perspective.near = near;
    }
    if view == crate::settings::CameraView::Cockpit {
        // In the seat: the car's pitch and roll are the driver's.
        let (eye, look, up) = cockpit(car);
        camera.translation = eye;
        camera.look_to(look, up);
        return;
    }
    if view == crate::settings::CameraView::Bonnet {
        // Rigid on the car, pitching and rolling with it so the bonnet stays
        // where it is in the frame; nothing to lag or clear.
        let (eye, look, up) = bonnet(car);
        camera.translation = eye;
        camera.look_to(look, up);
        return;
    }
    let zoom = follow.zoom;
    // The car lies along the slope; the camera must not. Hanging the boom off
    // the pitched nose lifts it a metre and tilts it ten degrees steeper on a
    // descent, which leaves the driver looking at the roof with the corner
    // ahead crushed into the bottom of the frame.
    let ahead = level(*car.forward());
    let desired = car.translation - ahead * (BACK * zoom) + Vec3::Y * (HEIGHT * zoom) + up * 0.55;
    let t_xz = if cut {
        1.0
    } else {
        1.0 - (-FOLLOW_XZ * quick * dt).exp()
    };
    let t_y = if cut {
        1.0
    } else {
        1.0 - (-FOLLOW_Y * quick * dt).exp()
    };
    camera.translation.x = camera.translation.x.lerp(desired.x, t_xz);
    camera.translation.z = camera.translation.z.lerp(desired.z, t_xz);
    // Raised, never lowered, from where the camera actually is rather than
    // where it is headed: at speed the lag stretches the boom to twice its
    // length, and twice as far back down a descent is twice as far into the hill.
    let (floor, rise) = clearance(
        &track,
        state.along,
        Vec3::new(camera.translation.x, desired.y, camera.translation.z),
        car.translation + Vec3::Y * SIGHT + up * 0.75,
        zoom,
    );
    camera.translation.y = camera
        .translation
        .y
        .lerp(desired.y.max(rise), t_y)
        .max(floor);

    let look = car.translation + ahead * LOOK_AHEAD + Vec3::Y * LOOK_HEIGHT + up * 0.75;
    camera.look_at(look, Vec3::Y);
}

/// The lowest the camera may be at its plan position `camera`, and the height
/// it wants to be at to keep `target` in view: [`ABOVE_GROUND`] over the ground
/// beneath it and a sight line that clears everything in between. Only terrain
/// and road heights are read, the same lookups the wheels use; nothing is cast
/// against a mesh. On flat and uphill road both come out below the level boom.
pub(crate) fn clearance(
    track: &Track,
    along: Option<f32>,
    camera: Vec3,
    target: Vec3,
    zoom: f32,
) -> (f32, f32) {
    let beneath = track.ground_from(camera, along).height;
    let mut rise = beneath + ABOVE_GROUND * zoom.min(1.0);
    let reach = (target - camera).reject_from(Vec3::Y).length() * SIGHT_REACH;
    // On a road the fun layer has made hills for, the ground is looked at twice
    // as closely: a ridge between two checks can stand above the line between
    // them without either of them seeing it. The circuits as surveyed have none,
    // and their camera is exactly the one that shipped.
    let fine = track.wild_used() > 0;
    let (step, most) = if fine {
        (SIGHT_STEP / 2.0, SIGHT_SAMPLES * 2)
    } else {
        (SIGHT_STEP, SIGHT_SAMPLES)
    };
    let samples = ((reach / step).ceil() as usize).clamp(4, most);
    // The height at the camera that puts the line through a ground `height`,
    // plus the margin, at fraction `f` of the way to the target.
    let needs = |f: f32, height: f32| (height + SIGHT_CLEAR - f * target.y) / (1.0 - f);
    let mut before: Option<(f32, f32)> = None;
    for k in 1..=samples {
        let f = SIGHT_REACH * k as f32 / samples as f32;
        let ground = track.ground_from(camera.lerp(target, f), along).height;
        rise = rise.max(needs(f, ground));
        // A cliff between two samples has its top between them, which is the
        // one place neither sample can see: the edge of a ridge the road runs
        // along. Look again, closer, wherever the ground changes that much, on
        // a road with hills.
        if fine
            && let Some((behind, then)) = before
            && (ground - then).abs() > SIGHT_JUMP
        {
            for j in 1..SIGHT_REFINE {
                let between = behind + (f - behind) * j as f32 / SIGHT_REFINE as f32;
                let height = track
                    .ground_from(camera.lerp(target, between), along)
                    .height;
                rise = rise.max(needs(between, height));
            }
        }
        before = Some((f, ground));
    }
    (beneath + FLOOR, rise)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both on-board cameras ride inside the car's own length and above its
    /// wheels, and look down the road it is pointing along.
    #[test]
    fn the_on_board_cameras_sit_in_the_car_and_look_ahead() {
        let car = Transform::from_translation(Vec3::new(3.0, 1.0, -2.0))
            .with_rotation(Quat::from_rotation_y(0.7))
            .with_scale(Vec3::splat(crate::car::SCALE));
        for (eye, look, up) in [cockpit(&car), bonnet(&car)] {
            let local = car.compute_affine().inverse().transform_point3(eye);
            assert!(local.z.abs() < 1.25 && local.x.abs() < 0.5, "{local}");
            assert!((0.45..0.72).contains(&local.y), "{local}");
            assert!(look.dot(*car.forward()) > 0.9);
            assert!(up.dot(Vec3::Y) > 0.99);
        }
    }

    /// Where the camera ends up with the car at station `i`, facing down the
    /// road, with the boom `stretch` times its resting length the way speed
    /// stretches it; the point of the car it has to see; and the lift.
    fn raised(
        track: &Track,
        points: &[(Vec3, f32)],
        i: usize,
        stretch: f32,
        zoom: f32,
    ) -> (Vec3, Vec3, f32) {
        let (pos, s) = points[i];
        let ahead = level(points[(i + 1) % points.len()].0 - pos);
        let car = Vec3::new(pos.x, track.ground_from(pos, Some(s)).height, pos.z);
        let mut camera = car - ahead * (BACK * zoom * stretch) + Vec3::Y * (HEIGHT * zoom);
        let target = car + Vec3::Y * SIGHT;
        let (floor, rise) = clearance(track, Some(s), camera, target, zoom);
        let level = camera.y;
        camera.y = camera.y.max(rise).max(floor);
        (camera, target, camera.y - level)
    }

    fn every_track() -> impl Iterator<Item = (Track, Vec<(Vec3, f32)>)> {
        every_track_at(0)
    }

    /// Every circuit, made `wild` wild.
    fn every_track_at(wild: u8) -> impl Iterator<Item = (Track, Vec<(Vec3, f32)>)> {
        crate::track::all_circuits().iter().map(move |circuit| {
            let track = Track::with_wild(circuit, wild);
            let points = track.map_points().collect();
            (track, points)
        })
    }

    /// The sight line from the camera to the roof never passes through the
    /// ground: not at the checks the camera is placed by, and not between
    /// them either. At the resting boom, at the length speed stretches it to,
    /// and at both ends of the zoom.
    #[test]
    fn the_car_stays_in_sight_on_every_circuit() {
        the_car_is_in_sight_on(every_track(), 5, 0.8);
    }

    /// And on the wildest hills, which have crests the camera has to be lifted
    /// over and dips it has to be kept out of. As far as it looks: the last
    /// quarter of the way to the car it takes to be the road the car is on, and
    /// with a crest a metre behind a car that has just come over it that is not
    /// so, which is a car going over a hill and being hidden by it for a moment.
    #[test]
    fn the_car_stays_in_sight_on_the_wildest_roads() {
        the_car_is_in_sight_on(every_track_at(3), 9, SIGHT_REACH);
    }

    /// Every `every`th station of each track, as far along the sight line as
    /// `reach`.
    fn the_car_is_in_sight_on(
        tracks: impl Iterator<Item = (Track, Vec<(Vec3, f32)>)>,
        every: usize,
        reach: f32,
    ) {
        for (track, points) in tracks {
            for i in (0..points.len()).step_by(every) {
                for (stretch, zoom) in [(1.0, 1.0), (2.5, 1.0), (1.0, ZOOM_MIN), (1.5, ZOOM_MAX)] {
                    let (camera, target, _) = raised(&track, &points, i, stretch, zoom);
                    for k in 0..=40 {
                        let f = k as f32 / 40.0 * reach;
                        let sight = camera.lerp(target, f);
                        let ground = track.ground_from(sight, Some(points[i].1)).height;
                        assert!(
                            sight.y > ground,
                            "{}: station {i}, boom x{stretch} zoom {zoom}: the sight line \
                             is {:.2} m under the ground {:.0}% of the way to the car",
                            track.circuit().name,
                            ground - sight.y,
                            f * 100.0
                        );
                    }
                }
            }
        }
    }

    /// Flat and uphill road keeps the level boom. With the car at rest, only
    /// the hilly circuits below lift the camera at all, and even Monaco's
    /// descents lift it for well under a sixth of the lap.
    #[test]
    fn the_level_boom_is_kept_almost_everywhere() {
        const HILLY: [&str; 12] = [
            "fuji",
            "imola",
            "interlagos",
            "kyalami",
            "laguna-seca",
            "mid-ohio",
            "monaco",
            "mosport",
            "road-america",
            "road-atlanta",
            "spa-francorchamps",
            "virginia-international-raceway",
        ];
        for (track, points) in every_track() {
            let n = points.len();
            let lifted = (0..n)
                .filter(|&i| raised(&track, &points, i, 1.0, 1.0).2 > 0.0)
                .count();
            let name = track.circuit().name;
            assert!(lifted * 6 < n, "{name}: lifted at {lifted} of {n} stations");
            if !HILLY.contains(&track.circuit().id) {
                assert_eq!(lifted, 0, "{name}: lifted at {lifted} stations");
            }
        }
    }
}
