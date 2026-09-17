//! The circuit: one closed centreline spline and one cross-section profile.
//!
//! Tarmac, edge lines, kerbs and grass are all strips of a single loft over the
//! stations in [`ribbon`]. Every strip is generated from the same station, the
//! same `right` vector and the same station index, so neighbouring strips share
//! their edge vertices exactly. Nothing overlaps anything, so nothing can
//! z-fight, tear open, or stair-step away from its neighbour.
//!
//! The sweep itself has no special cases — no clamping, no per-corner fudge. It
//! can afford that because the cross-section is checked against what the circuit
//! can actually carry: see [`profile`].
//!
//! This module is the circuit as the rest of the game sees it: a [`Track`] that
//! answers where the ground is and what it is made of, holds the car on the
//! loft, and knows where the start line is. The shape lives in [`ribbon`]; the
//! cross-section in [`profile`].

mod layout;
mod profile;
mod ribbon;

use bevy::prelude::*;

use crate::car::{level, Car, DriveSet};
use layout::CENTERLINE;
use profile::{EDGE, HALF_WIDTH};
use ribbon::Ribbon;

/// The car is the ruler: ~2.4 m long, ~1.1 m wide, 1 unit = 1 metre.
const PLAN_SCALE: f32 = 0.4 / 3.0;
/// Elevation as a fraction of the real Red Bull Ring, then smoothed and
/// grade-capped. The plan is scaled far harder than this, so the hills come out
/// steeper than real by the ratio of the two: at 0.4 that was three times, and
/// every descent arrived at its corner too fast to take. This still rolls.
const HEIGHT_SCALE: f32 = 0.28;
/// The car may run wide onto the verge, but not off the loft into the sky.
const WALL: f32 = EDGE - 0.6;
/// Fraction of the impact the wall gives back. Absorbing it all lets a car that
/// spun in nose-first sit there with its wheels spinning, because everything it
/// does is outward and everything outward is deleted.
const BOUNCE: f32 = 0.45;
/// Sitting off the circuit going nowhere for this long earns a lift back to the
/// racing line. A barrier you can wedge yourself against for good is worse than
/// no barrier at all.
const RESCUE_AFTER: f32 = 1.6;
const GOING_NOWHERE: f32 = 1.5;

pub struct TrackPlugin;

impl Plugin for TrackPlugin {
    fn build(&self, app: &mut App) {
        // Built here rather than in a startup system so the car and the camera
        // can read the grid slot the moment they spawn.
        app.insert_resource(Track::new())
            .add_systems(Startup, setup)
            .add_systems(Update, confine.after(DriveSet));
    }
}

#[derive(Resource)]
pub struct Track {
    ribbon: Ribbon,
}

impl Track {
    pub(crate) fn new() -> Self {
        let control: Vec<Vec3> = CENTERLINE
            .iter()
            .map(|p| Vec3::new(p[0] * PLAN_SCALE, p[1] * HEIGHT_SCALE, p[2] * PLAN_SCALE))
            .collect();
        let ribbon = Ribbon::new(&control);
        // The contract in [`profile`], checked against the circuit that was
        // actually built. Swap the layout or widen the road and this is what
        // says so, rather than the mesh quietly folding. Quadratic in stations,
        // so debug only.
        debug_assert_eq!(profile::check(&ribbon), Ok(()));
        Self { ribbon }
    }

    /// Pose of the grid slot, facing down the start/finish straight.
    pub fn start_transform(&self) -> Transform {
        let start = self.ribbon.start();
        Transform::from_translation(start.pos).looking_to(start.tangent, Vec3::Y)
    }

    /// Signed distance past the start/finish plane, along the circuit.
    pub(crate) fn start_along(&self, pos: Vec3) -> f32 {
        let start = self.ribbon.start();
        (pos - start.pos).reject_from(Vec3::Y).dot(start.tangent)
    }

    pub(crate) fn on_start_gate(&self, pos: Vec3) -> bool {
        let start = self.ribbon.start();
        (pos - start.pos).reject_from(Vec3::Y).dot(start.right).abs() < HALF_WIDTH + 1.5
    }

    /// 0 at start/finish, approaching 1 at the end of the lap.
    pub(crate) fn progress(&self, pos: Vec3) -> f32 {
        self.ribbon.locate(pos).s / self.ribbon.length()
    }

    /// Sit the car on the loft, hold it inside the outermost strip, and fetch it
    /// back if it has stranded itself out there.
    ///
    /// This is the only thing the circuit does to the car. Everything else the
    /// road asks of it — grip, the pull of a climb, the kerb under a wheel —
    /// reaches the car through [`Track::ground`], so the driving model stays in
    /// one place.
    pub(crate) fn hold(&self, transform: &mut Transform, car: &mut Car, dt: f32) {
        let ground = self.ground(transform.translation);
        transform.translation.y = ground.height;
        // Sit the car on the slope rather than level on top of it. On the steep
        // parts that is nine degrees, which is the nose buried in the road — and
        // a hill you cannot see coming is a hill you arrive at far too fast.
        let heading = level(*transform.forward());
        let grade = ground.slope * ground.tangent.dot(heading);
        transform.look_to(heading + Vec3::Y * grade, Vec3::Y);

        // Off the road and going nowhere: a spin into the barrier leaves the car
        // nose-first against it, where everything it does is outward and
        // everything outward is taken away.
        if ground.lateral.abs() > HALF_WIDTH && car.velocity.length() < GOING_NOWHERE {
            car.stranded += dt;
        } else {
            car.stranded = 0.0;
        }
        if car.stranded > RESCUE_AFTER {
            self.rescue(transform, car);
            return;
        }

        if ground.lateral.abs() <= WALL {
            return;
        }
        let held = ground.lateral.clamp(-WALL, WALL);
        let correction = ground.right * (held - ground.lateral);
        transform.translation += Vec3::new(correction.x, 0.0, correction.z);
        // Take out whatever was carrying it outward and push a little of it back,
        // leaving the speed along the circuit alone.
        let side = ground.lateral.signum();
        let outward = car.velocity.dot(ground.right) * side;
        if outward > 0.0 {
            car.velocity -= ground.right * (outward * (1.0 + BOUNCE) * side);
        }
    }

    /// Put the car back on the racing line at the nearest point, stopped and
    /// pointing the way the lap runs. What the driver gets from the reset key,
    /// and what [`confine`] does for a car that has stranded itself.
    pub(crate) fn rescue(&self, transform: &mut Transform, car: &mut Car) {
        let ground = self.ground(transform.translation);
        *transform = Transform::from_translation(ground.centre)
            .looking_to(ground.tangent, Vec3::Y)
            .with_scale(transform.scale);
        *car = Car::default();
    }

    /// What the car is standing on. The loft is the only surface in the world,
    /// so this reads the same [`profile`] the mesh was swept from — the car
    /// rides the kerb because the kerb is 5 cm proud in the profile, not because
    /// anything says so twice.
    pub(crate) fn ground(&self, pos: Vec3) -> Ground {
        let fix = self.ribbon.locate(pos);
        Ground {
            centre: fix.point,
            height: fix.point.y + profile::height(fix.lateral),
            tangent: fix.tangent,
            right: fix.right,
            lateral: fix.lateral,
            slope: fix.slope,
            curvature: fix.curvature,
            grip: profile::grip(fix.lateral),
        }
    }
}

/// What the car is standing on, at one point.
pub(crate) struct Ground {
    /// Nearest point on the centreline, at circuit elevation.
    pub(crate) centre: Vec3,
    /// Surface height, kerb lip and verge fall included.
    pub(crate) height: f32,
    /// Unit heading of the circuit here, level in XZ.
    pub(crate) tangent: Vec3,
    pub(crate) right: Vec3,
    /// Metres right of the centreline; negative is left.
    pub(crate) lateral: f32,
    /// Rise over run along `tangent`. Gravity pulls against this.
    pub(crate) slope: f32,
    /// Signed curvature of the circuit here; its reciprocal is the corner radius.
    /// Only the driver in `car`'s driveability test reads it — it is how that
    /// driver knows to slow down for what is coming.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) curvature: f32,
    /// Fraction of tarmac grip.
    pub(crate) grip: f32,
}

fn setup(
    mut commands: Commands,
    track: Res<Track>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(profile::loft(&track.ribbon))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.9,
            ..default()
        })),
    ));
}

fn confine(time: Res<Time>, track: Res<Track>, mut cars: Query<(&mut Transform, &mut Car)>) {
    let dt = time.delta_secs();
    for (mut transform, mut car) in &mut cars {
        track.hold(&mut transform, &mut car, dt);
    }
}

#[cfg(test)]
mod tests {
    use super::profile::{GRASS_GRIP, KERB_GRIP, KERB_TOP, TARMAC_HALF};
    use super::*;

    fn track() -> Track {
        Track::new()
    }

    #[test]
    fn hills_roll_instead_of_stepping() {
        let track = track();
        let stations = track.ribbon.stations();
        let n = stations.len();
        let mut steepest = 0.0f32;
        for i in 0..n {
            let a = stations[i].pos;
            let b = stations[(i + 1) % n].pos;
            let run = Vec3::new(b.x - a.x, 0.0, b.z - a.z).length().max(1e-4);
            steepest = steepest.max((b.y - a.y).abs() / run);
        }
        assert!(steepest < 0.2, "max grade {steepest} is a cliff, not a roll");
        let (low, high) = stations.iter().fold((f32::MAX, f32::MIN), |(l, h), s| {
            (l.min(s.pos.y), h.max(s.pos.y))
        });
        assert!(high - low > 12.0, "smoothing flattened the circuit away");
    }

    /// Walk the whole lap the way the timer sees it: progress climbs from the
    /// line back to the line, and the gate opens exactly once, at the line.
    #[test]
    fn a_lap_reads_as_one_lap() {
        let track = track();
        let stations = track.ribbon.stations();
        let mut highest = 0.0f32;
        let mut crossings = 0;
        let mut previous = track.start_along(stations[stations.len() - 1].pos);
        for station in stations {
            // Drive a little off the racing line, as a car would.
            let pos = station.pos + station.right * (TARMAC_HALF * 0.5);
            let progress = track.progress(pos);
            assert!(
                progress >= highest - 0.02,
                "progress went backwards at s={}: {progress} after {highest}",
                station.s
            );
            highest = highest.max(progress);
            let along = track.start_along(pos);
            if previous <= 0.0 && along > 0.0 && track.on_start_gate(pos) {
                crossings += 1;
            }
            previous = along;
        }
        assert!(highest > 0.97, "the lap only reached {highest}");
        assert_eq!(crossings, 1, "the start gate opened {crossings} times");
    }

    /// What the car stands on has to be the same cross-section the mesh was
    /// swept from, or the car rides at a height the road is not at.
    #[test]
    fn the_ground_reads_the_profile_it_was_lofted_from() {
        let track = track();
        let start = track.start_transform();
        let right = *start.right();
        for (across, grip, height) in [
            (0.0, 1.0, 0.0),
            (TARMAC_HALF - 0.01, 1.0, 0.0),
            (HALF_WIDTH - 0.01, KERB_GRIP, KERB_TOP * (0.69 / 0.70)),
            (HALF_WIDTH + 2.0, GRASS_GRIP, -0.20),
        ] {
            for side in [-1.0f32, 1.0] {
                let ground = track.ground(start.translation + right * (side * across));
                assert_eq!(ground.grip, grip, "grip {across} m off the line");
                assert!(
                    (ground.height - ground.centre.y - height).abs() < 0.02,
                    "height {across} m off the line: {} against {height}",
                    ground.height - ground.centre.y
                );
                assert!((ground.lateral - side * across).abs() < 0.02);
                // The centreline point is on the centreline, whatever we asked about.
                assert!(track.ground(ground.centre).lateral.abs() < 0.02);
            }
        }
    }

    /// A car that has stranded itself has to be left somewhere it can drive away
    /// from, whatever mess it was in.
    #[test]
    fn a_rescue_puts_the_car_back_on_the_line() {
        let track = track();
        let start = track.start_transform();
        let right = *start.right();
        for stuck in [WALL, -WALL, HALF_WIDTH + 1.0, 0.0] {
            let mut transform = Transform::from_translation(start.translation + right * stuck)
                // Facing backwards, sideways, and scaled like the real car.
                .looking_to(-*start.forward(), Vec3::Y)
                .with_scale(Vec3::splat(0.8));
            let mut car = Car {
                velocity: right * 9.0,
                yaw_rate: 3.0,
                stranded: 4.0,
                ..default()
            };
            track.rescue(&mut transform, &mut car);

            let ground = track.ground(transform.translation);
            assert!(ground.lateral.abs() < 0.01, "rescue {stuck} m out landed off-line");
            assert!(car.velocity.length() < 1e-4, "rescue left the car moving");
            assert_eq!(car.yaw_rate, 0.0);
            assert_eq!(car.stranded, 0.0);
            assert_eq!(transform.scale, Vec3::splat(0.8), "rescue resized the car");
            assert!(
                transform.forward().dot(ground.tangent) > 0.99,
                "rescue {stuck} m out left the car facing the wrong way"
            );
        }
    }

    /// On the steep parts the car has to follow the road, not stay level on top
    /// of it with its nose in the tarmac.
    #[test]
    fn the_car_lies_along_the_slope() {
        let track = track();
        let steepest = track
            .ribbon
            .stations()
            .iter()
            .max_by(|a, b| a.slope.abs().total_cmp(&b.slope.abs()))
            .expect("the circuit has stations");
        assert!(steepest.slope.abs() > 0.1, "nowhere steep enough to test");

        for facing in [1.0f32, -1.0] {
            let mut transform = Transform::from_translation(steepest.pos)
                .looking_to(steepest.tangent * facing, Vec3::Y);
            let mut car = Car::default();
            track.hold(&mut transform, &mut car, 1.0 / 60.0);

            // Running down the hill the nose points down, and up it points up.
            let grade = steepest.slope * facing;
            let want = grade / (1.0 + grade * grade).sqrt();
            assert!(
                (transform.forward().y - want).abs() < 0.01,
                "facing {facing}, nose at {} against a grade of {want}",
                transform.forward().y
            );
            // And it is still pointing the way it was, in plan.
            let heading = crate::car::level(*transform.forward());
            assert!(heading.dot(steepest.tangent * facing) > 0.999, "the pitch turned it");
        }
    }

    /// The grid slot is on the tarmac, pointing down the circuit.
    #[test]
    fn start_is_on_the_road() {
        let track = track();
        let pose = track.start_transform();
        let fix = track.ribbon.locate(pose.translation);
        assert!(fix.lateral.abs() < 0.01);
        assert!(track.on_start_gate(pose.translation));
        assert!(track.progress(pose.translation) < 0.01);
        let ahead = pose.translation + *pose.forward() * 3.0;
        assert!(track.start_along(ahead) > 2.9);
    }
}

