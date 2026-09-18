//! The circuit: one closed centreline spline and one cross-section profile.
//!
//! Tarmac, edge lines, kerbs and grass are all strips of a single loft over the
//! stations in [`ribbon`]. Every strip is generated from the same station, the
//! same `right` vector and the same station index, so neighbouring strips share
//! their edge vertices exactly. Nothing overlaps anything, so nothing can
//! z-fight, tear open, or stair-step away from its neighbour.
//!
//! The sweep itself has no special cases — no clamping, no per-corner fudge. It
//! can afford that because the cross-section is fitted to what the circuit can
//! actually carry: see [`profile`].
//!
//! Which circuit is a choice, not a constant. Every circuit in [`circuits`] is
//! the same kind of thing — a real trace in real metres — and gets the same
//! treatment here, so they shrink alike and drive alike. The track key builds
//! the next one and hands everything else a [`Reset`]; nothing downstream knows
//! how a circuit is made.
//!
//! This module is the circuit as the rest of the game sees it: a [`Track`] that
//! answers where the ground is and what it is made of, holds the car on the
//! loft, and knows where the start line is. The shape lives in [`ribbon`]; the
//! cross-section in [`profile`].

mod circuits;
mod markers;
mod profile;
mod ribbon;

use bevy::prelude::*;

use crate::Reset;
use crate::car::{Car, level};
use crate::menu::MenuSet;
pub(crate) use circuits::Circuit;
use profile::{HALF_WIDTH, Profile};
use ribbon::Ribbon;

/// The car is the ruler: ~2.4 m long, ~1.1 m wide, 1 unit = 1 metre. Every
/// circuit is scaled by this, so a longer circuit makes a longer lap rather than
/// a bigger world, and a lap time means the same thing wherever it was set.
const PLAN_SCALE: f32 = 0.4 / 3.0;
/// Elevation as a fraction of the real circuit, then smoothed and grade-capped.
/// The plan is scaled far harder than this, so the hills come out steeper than
/// real by the ratio of the two: at 0.4 that was three times, and every descent
/// arrived at its corner too fast to take. This still rolls.
const HEIGHT_SCALE: f32 = 0.28;
/// How far inside the edge of the loft the car is held. It may run wide onto the
/// verge, but not off into the sky.
const WALL_INSET: f32 = 0.6;
/// Fraction of the impact the wall gives back. Absorbing it all lets a car that
/// spun in nose-first sit there with its wheels spinning, because everything it
/// does is outward and everything outward is deleted.
const BOUNCE: f32 = 0.45;
/// Sitting off the circuit going nowhere for this long earns a lift back to the
/// racing line. A barrier you can wedge yourself against for good is worse than
/// no barrier at all.
const RESCUE_AFTER: f32 = 1.6;
const GOING_NOWHERE: f32 = 1.5;
/// How far before the start/finish line the car is set down, so that a lap
/// begins at speed instead of from a standstill.
///
/// It takes 45 m to reach 20 m/s, and 20 m/s is as good as this is going to get:
/// the car settles at 22.2 m/s on the flat, not at the 24 of `top_speed`, which
/// is only where the engine's push fades to nothing — drag and rolling
/// resistance are still there when it does. The last tenth costs another 65 m.
///
/// And 65 m is not there to spend. The run-up wants to be straight, or the car
/// arrives at the line slower for having cornered on the way, and a straight is
/// what the circuits have least of behind their lines: Spielberg has 48 m of it,
/// Monza 97, and Spa none at all, because Spa's line is inside La Source. At 45
/// every circuit sets the car down pointing very nearly the way the line does;
/// at 75 Spielberg sets it down sideways, mid-corner. `the_grid_is_a_run_up_to
/// _the_line` is what holds that.
const RUN_UP: f32 = 45.0;
/// How much of a circuit has to survive being shrunk and having its corners
/// opened for what is left to still be that circuit. See [`Ribbon::kept`]: the
/// two in the game keep about nine tenths, and a circuit that keeps a quarter
/// has been rounded off into a ring.
const LEAST_KEPT: f32 = 0.75;

/// Building the chosen circuit runs in here, after the menu that chose it and
/// before anything that puts itself back on a [`Reset`]. A switch writes that
/// reset, so by the time the car, the clock, the ghost and the marks act on it,
/// [`Track`] is already the new circuit.
#[derive(SystemSet, Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct TrackSet;

pub struct TrackPlugin;

impl Plugin for TrackPlugin {
    fn build(&self, app: &mut App) {
        // Built here rather than in a startup system so the car and the camera
        // can read the grid slot the moment they spawn.
        app.insert_resource(Track::new(circuits::first()))
            .add_message::<GoTo>()
            .add_systems(Startup, setup)
            .add_systems(PreUpdate, switch.in_set(TrackSet).after(MenuSet));
    }
}

/// The loft, so a switch knows whose mesh to replace.
#[derive(Component)]
struct Loft;

/// Every circuit there is, in the order the menu lists them.
pub(crate) fn all_circuits() -> &'static [Circuit] {
    circuits::all()
}

/// Where a circuit sits in that list.
pub(crate) fn circuit_at(circuit: &Circuit) -> usize {
    circuits::at(circuit)
}

/// Drive that one. Written by the circuit menu and acted on by [`switch`];
/// nothing outside this module knows how a circuit is built, and nothing inside
/// it knows which key was pressed.
#[derive(Message)]
pub(crate) struct GoTo(pub &'static Circuit);

/// The tightest corner the game allows, which is what a car's hardest stop is
/// measured down to. Read by the garage's report in [`crate::car`], so the
/// figure the corner markers are a ruler for comes from the same place the
/// markers themselves read it.
#[cfg(test)]
pub(crate) use ribbon::MIN_RADIUS;

#[derive(Resource)]
pub struct Track {
    circuit: &'static Circuit,
    ribbon: Ribbon,
    profile: Profile,
}

impl Track {
    /// Shrink a real circuit to Todora's scale and fit a road to it.
    ///
    /// Either half can fail. A circuit can be too tight to survive the shrink at
    /// all, and a circuit can be too cramped to carry a road and a verge. Both
    /// are properties of the circuit, so both are settled once, here, and a
    /// circuit that fails either is one that should not be in [`circuits`].
    /// `every_circuit_carries_a_road` is what catches that, rather than the
    /// player pressing the track key.
    pub(crate) fn new(circuit: &'static Circuit) -> Self {
        let control: Vec<Vec3> = circuit
            .centreline
            .iter()
            .map(|p| Vec3::new(p[0] * PLAN_SCALE, p[1] * HEIGHT_SCALE, p[2] * PLAN_SCALE))
            .collect();
        let ribbon = Ribbon::new(&control, circuit.corners);
        assert!(
            ribbon.kept() > LEAST_KEPT,
            "{} does not survive Todora's scale: opening its corners left \
             {:.0}% of the lap, {:.0} m of a circuit that should be {:.0} m",
            circuit.name,
            ribbon.kept() * 100.0,
            ribbon.length(),
            ribbon.length() / ribbon.kept(),
        );
        let profile = Profile::fit(&ribbon)
            .unwrap_or_else(|why| panic!("{} cannot carry a road: {why}", circuit.name));
        Self {
            circuit,
            ribbon,
            profile,
        }
    }

    /// The circuit the game opens on, for tests that just need somewhere to
    /// drive. Tests about the circuits themselves walk [`all_circuits`].
    #[cfg(test)]
    pub(crate) fn any() -> Self {
        Self::new(circuits::first())
    }

    /// The circuit being driven. Its name is what the driver is told; its id is
    /// what the lap saved for it is filed under.
    pub(crate) fn circuit(&self) -> &'static Circuit {
        self.circuit
    }

    /// Pose of the grid slot: [`RUN_UP`] metres before the start/finish line,
    /// facing the way the lap runs.
    ///
    /// The grid is not the line. The clock starts where the line is, so the run
    /// up to it is the driver's to spend and costs nothing — see [`crate::lap`].
    pub fn start_transform(&self) -> Transform {
        let grid = self.ribbon.before_start(RUN_UP);
        Transform::from_translation(grid.pos).looking_to(grid.tangent, Vec3::Y)
    }

    /// Signed distance past the start/finish plane, along the circuit.
    pub(crate) fn start_along(&self, pos: Vec3) -> f32 {
        let start = self.ribbon.start();
        (pos - start.pos).reject_from(Vec3::Y).dot(start.tangent)
    }

    pub(crate) fn on_start_gate(&self, pos: Vec3) -> bool {
        let start = self.ribbon.start();
        (pos - start.pos)
            .reject_from(Vec3::Y)
            .dot(start.right)
            .abs()
            < HALF_WIDTH + 1.5
    }

    /// A cheap hash of the shape the car actually drives on.
    ///
    /// Taken from the finished centreline rather than from the trace it came
    /// from, so it moves when *anything* that shapes a circuit moves: the scales
    /// here, the trace itself, and every constant in [`ribbon`] — how far the
    /// corners are opened, how the heights are smoothed, how steep a grade is
    /// allowed to be. A lap saved around one shape means nothing around another,
    /// so this goes into the saved file and is checked on the way back in.
    /// FNV-1a, because it only has to notice a change, not resist anyone.
    pub(crate) fn fingerprint(&self) -> u64 {
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;
        let mut hash = OFFSET;
        for station in self.ribbon.stations() {
            for value in station.pos.to_array() {
                for byte in value.to_bits().to_le_bytes() {
                    hash = (hash ^ byte as u64).wrapping_mul(PRIME);
                }
            }
        }
        hash
    }

    /// Plan length of one lap. Circuits are not the same length — they are all
    /// shrunk alike rather than to a common size — so anything budgeting time or
    /// distance has to ask. Only the drivers' lap harness does, so far.
    #[cfg(test)]
    pub(crate) fn length(&self) -> f32 {
        self.ribbon.length()
    }

    /// 0 at start/finish, approaching 1 at the end of the lap.
    pub(crate) fn progress(&self, pos: Vec3) -> f32 {
        self.ribbon.locate(pos).s / self.ribbon.length()
    }

    /// How far out the car is held. Inside the edge of the loft, wherever this
    /// circuit's verge had to stop.
    fn wall(&self) -> f32 {
        self.profile.edge() - WALL_INSET
    }

    /// Sit the car on the loft, hold it inside the outermost strip, and fetch it
    /// back if it has stranded itself out there.
    ///
    /// This is the only thing the circuit does to the car. Everything else the
    /// road asks of it — grip, the pull of a climb, the kerb under a wheel —
    /// reaches the car through [`Track::ground`], so the driving model stays in
    /// one place.
    pub(crate) fn hold(&self, transform: &mut Transform, car: &mut Car, dt: f32) {
        let mut ground = self.ground(transform.translation);
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

        let wall = self.wall();
        if ground.lateral.abs() > wall {
            let held = ground.lateral.clamp(-wall, wall);
            let correction = ground.right * (held - ground.lateral);
            transform.translation += Vec3::new(correction.x, 0.0, correction.z);
            // Absorb the outward impact, retaining motion along the circuit.
            let side = ground.lateral.signum();
            let outward = car.velocity.dot(ground.right) * side;
            if outward > 0.0 {
                car.velocity -= ground.right * (outward * (1.0 + BOUNCE) * side);
            }
            ground = self.ground(transform.translation);
        }

        transform.translation.y = ground.height;
        // Sit the car on the slope rather than level on top of it. On the steep
        // parts that is nine degrees, which is the nose buried in the road — and
        // a hill you cannot see coming is a hill you arrive at far too fast.
        let heading = level(*transform.forward());
        let grade = ground.slope * ground.tangent.dot(heading);
        transform.look_to(heading + Vec3::Y * grade, Vec3::Y);
    }

    /// Put the car back on the racing line at the nearest point, stopped and
    /// pointing the way the lap runs. What the driver gets from the reset key,
    /// and what [`Track::hold`] does for a car that has stranded itself.
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
            height: fix.point.y + self.profile.height(fix.lateral),
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
        Loft,
        Mesh3d(meshes.add(track.profile.loft(&track.ribbon))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.9,
            ..default()
        })),
    ));
}

/// Go where the menu said.
///
/// Building a circuit is a few tens of milliseconds of splining and
/// corner-opening — a visible hitch, once, at the moment the world is replaced
/// anyway, and the game is stopped behind the menu while it happens. The old
/// mesh goes when the last handle to it does.
fn switch(
    mut asked: MessageReader<GoTo>,
    mut track: ResMut<Track>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut loft: Query<&mut Mesh3d, With<Loft>>,
    mut reset: MessageWriter<Reset>,
) {
    let Some(GoTo(next)) = asked.read().last() else {
        return;
    };
    if next.id == track.circuit.id {
        return;
    }
    *track = Track::new(next);
    if let Ok(mut mesh) = loft.single_mut() {
        mesh.0 = meshes.add(track.profile.loft(&track.ribbon));
    }
    // Everything that owns a piece of the old lap puts it back itself.
    reset.write(Reset);
}

#[cfg(test)]
mod tests {
    use super::profile::{EDGE, GRASS_GRIP, KERB_GRIP, KERB_TOP, TARMAC_HALF};
    use super::*;

    fn track() -> Track {
        Track::any()
    }

    /// Every circuit, so a new one has to clear the same bar as the old ones.
    fn every_track() -> impl Iterator<Item = (&'static str, Track)> {
        circuits::all()
            .iter()
            .map(|circuit| (circuit.name, Track::new(circuit)))
    }

    /// Two things at once, on every circuit. No step is steeper than the cap —
    /// the DEM is quantised to whole metres and the plan is shrunk five times
    /// harder than the height, so an unsmoothed step would read as a wall. And
    /// almost all of the relief the trace carried survives being smoothed and
    /// capped, which is the other way round: a cap low enough to iron the hills
    /// flat would pass the first check and fail this one.
    ///
    /// Stated as a fraction rather than as metres, because the circuits are not
    /// equally hilly. Spa rises 27 m and Monza 6, and Monza is not broken — it
    /// is Monza.
    #[test]
    fn hills_roll_instead_of_stepping() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let n = stations.len();
            let mut steepest = 0.0f32;
            for i in 0..n {
                let a = stations[i].pos;
                let b = stations[(i + 1) % n].pos;
                let run = Vec3::new(b.x - a.x, 0.0, b.z - a.z).length().max(1e-4);
                steepest = steepest.max((b.y - a.y).abs() / run);
            }
            assert!(
                steepest <= ribbon::MAX_GRADE + 1e-3,
                "{name}: max grade {steepest} is past the cap of {}",
                ribbon::MAX_GRADE
            );

            let (low, high) = stations.iter().fold((f32::MAX, f32::MIN), |(l, h), s| {
                (l.min(s.pos.y), h.max(s.pos.y))
            });
            let (raw_low, raw_high) = track
                .circuit()
                .centreline
                .iter()
                .fold((f32::MAX, f32::MIN), |(l, h), p| (l.min(p[1]), h.max(p[1])));
            let wanted = (raw_high - raw_low) * HEIGHT_SCALE;
            assert!(
                high - low > 0.8 * wanted,
                "{name}: smoothing left {:.1} m of the {wanted:.1} m the circuit climbs",
                high - low
            );
        }
    }

    /// Walk the whole lap the way the timer sees it: progress climbs all the way
    /// round and comes back to where it started exactly once, at the line, which
    /// is also the one place the gate opens.
    ///
    /// The walk is driven a little off the racing line, as a car is. That is why
    /// the wrap is counted rather than assumed to fall at station zero: a car
    /// sitting on the line but a metre and a half to one side is nearest a
    /// centreline point that may be a hair either side of it, so which station
    /// the lap turns over on is the circuit's business, not the timer's.
    #[test]
    fn a_lap_reads_as_one_lap() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let off_line =
                |station: &ribbon::Station| station.pos + station.right * (TARMAC_HALF * 0.5);
            let last = &stations[stations.len() - 1];
            let mut crossings = 0;
            let mut wraps = 0;
            let mut climbed = 0.0f32;
            let mut was_progress = track.progress(off_line(last));
            let mut was_along = track.start_along(off_line(last));
            for station in stations {
                let pos = off_line(station);
                let progress = track.progress(pos);
                let step = progress - was_progress;
                if step < -0.5 {
                    wraps += 1;
                } else {
                    assert!(
                        step >= -0.02,
                        "{name}: progress went backwards at s={}: {progress} after {was_progress}",
                        station.s
                    );
                    climbed += step;
                }
                was_progress = progress;

                let along = track.start_along(pos);
                if was_along <= 0.0 && along > 0.0 && track.on_start_gate(pos) {
                    crossings += 1;
                }
                was_along = along;
            }
            assert_eq!(wraps, 1, "{name}: the lap turned over {wraps} times");
            assert!(climbed > 0.97, "{name}: the lap only covered {climbed}");
            assert_eq!(
                crossings, 1,
                "{name}: the start gate opened {crossings} times"
            );
        }
    }

    /// What the car stands on has to be the same cross-section the mesh was
    /// swept from, or the car rides at a height the road is not at.
    #[test]
    fn the_ground_reads_the_profile_it_was_lofted_from() {
        for (name, track) in every_track() {
            let start = track.start_transform();
            let right = *start.right();
            // The verge falls to the same depths however far out the edge is, so
            // the reading at the kerb is fixed and the one out on the grass is
            // read from the profile this circuit was actually fitted with.
            let verge = track.profile.edge() - HALF_WIDTH;
            for (across, grip, height) in [
                (0.0, 1.0, 0.0),
                (TARMAC_HALF - 0.01, 1.0, 0.0),
                (HALF_WIDTH - 0.01, KERB_GRIP, KERB_TOP * (0.69 / 0.70)),
                (HALF_WIDTH + verge * (2.0 / 3.0), GRASS_GRIP, -0.20),
            ] {
                for side in [-1.0f32, 1.0] {
                    let ground = track.ground(start.translation + right * (side * across));
                    assert_eq!(ground.grip, grip, "{name}: grip {across} m off the line");
                    assert!(
                        (ground.height - ground.centre.y - height).abs() < 0.02,
                        "{name}: height {across} m off the line: {} against {height}",
                        ground.height - ground.centre.y
                    );
                    assert!((ground.lateral - side * across).abs() < 0.02);
                    // The centreline point is on the centreline, whatever we asked.
                    assert!(track.ground(ground.centre).lateral.abs() < 0.02);
                }
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
        let wall = track.wall();
        for stuck in [wall, -wall, HALF_WIDTH + 1.0, 0.0] {
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
            assert!(
                ground.lateral.abs() < 0.01,
                "rescue {stuck} m out landed off-line"
            );
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

    #[test]
    fn a_wall_hit_stays_on_the_ground_and_does_not_add_energy() {
        for (name, track) in every_track() {
            let wall = track.wall();
            for station in track.ribbon.stations().iter().step_by(20) {
                for side in [-1.0, 1.0] {
                    let mut transform = Transform::from_translation(
                        station.pos + station.right * side * (wall + 0.4),
                    );
                    let mut car = Car {
                        velocity: station.right * side * 20.0 + station.tangent * 10.0,
                        ..default()
                    };
                    let speed = car.velocity.length();
                    track.hold(&mut transform, &mut car, 1.0 / 240.0);
                    let ground = track.ground(transform.translation);
                    assert!(
                        (transform.translation.y - ground.height).abs() < 0.01,
                        "{name}: wall left the car off the loft: {} vs {}",
                        transform.translation.y,
                        ground.height
                    );
                    assert!(ground.lateral.abs() <= wall + 0.01);
                    assert!(car.velocity.length() <= speed);
                    assert!(car.velocity.dot(ground.right) * side < 0.0);
                }
            }
        }
    }

    #[test]
    fn a_stranded_car_is_rescued_after_simulated_time() {
        let track = track();
        let start = track.start_transform();
        let mut transform = start;
        transform.translation += *start.right() * (HALF_WIDTH + 1.0);
        let mut car = Car::default();
        for _ in 0..380 {
            track.hold(&mut transform, &mut car, 1.0 / 240.0);
        }
        assert!(track.ground(transform.translation).lateral.abs() > HALF_WIDTH);
        for _ in 0..10 {
            track.hold(&mut transform, &mut car, 1.0 / 240.0);
        }
        assert!(track.ground(transform.translation).lateral.abs() < 0.01);
        assert_eq!(car.stranded, 0.0);
        assert_eq!(car.velocity, Vec3::ZERO);
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
            assert!(
                heading.dot(steepest.tangent * facing) > 0.999,
                "the pitch turned it"
            );
        }
    }

    /// The grid slot is on the tarmac, pointing down the circuit.
    #[test]
    fn start_is_on_the_road() {
        for (name, track) in every_track() {
            let pose = track.start_transform();
            let fix = track.ribbon.locate(pose.translation);
            assert!(fix.lateral.abs() < 0.01, "{name}");
            let lap = track.ribbon.length();
            assert!(
                track.progress(pose.translation) > 1.0 - (RUN_UP + 1.0) / lap,
                "{name} sets the car down further back than the run-up"
            );
        }
    }

    /// The grid is a run-up, and a run-up has to be usable. It sits behind the
    /// line by [`RUN_UP`] as the lap runs — which the line's own plane has to
    /// agree with, because that plane is what the clock reads to know the car
    /// has arrived — and it faces down the circuit rather than across it.
    ///
    /// The bar is what "across it" means, and it is loose on purpose. A car set
    /// down in the exit of the last corner is not a problem; that is where 45 m
    /// before a start/finish line usually *is*. Silverstone's grid faces 29
    /// degrees off its line because it sits in the exit of Woodcote, and the car
    /// still arrives doing 19.5 m/s — better than Monza. What a tighter bar here
    /// would catch is circuits that are fine, and what it would not catch is a
    /// run-up that is straight and still useless. How much speed the car
    /// actually brings to the line is a question for the car, so it is asked
    /// where the car is: `the_run_up_reaches_the_line_at_speed`.
    #[test]
    fn the_grid_is_a_run_up_to_the_line() {
        for (name, track) in every_track() {
            let pose = track.start_transform();
            let along = track.start_along(pose.translation);
            assert!(
                (-RUN_UP - 1.0..-RUN_UP * 0.9).contains(&along),
                "{name} reads {along:.1} m behind a line it is set down {RUN_UP} m from"
            );
            let turn = level(*pose.forward())
                .dot(track.ribbon.start().tangent)
                .acos()
                .to_degrees();
            assert!(
                turn < 45.0,
                "{name} sets the car down {turn:.0} degrees off the line, which is \
                 across it rather than down it"
            );
            // And the way out of the grid is toward the line, not away from it.
            let ahead = pose.translation + *pose.forward() * 3.0;
            assert!(track.start_along(ahead) > along, "{name} faces backwards");
        }
    }

    /// Shrinking a circuit cuts its corners, and a circuit is not allowed to be
    /// mostly corner-cutting. `Track::new` is what enforces that; this is the
    /// margin the circuits in the game actually have.
    #[test]
    fn every_circuit_survives_the_shrink() {
        for (name, track) in every_track() {
            let kept = track.ribbon.kept();
            assert!(kept > LEAST_KEPT, "{name} kept only {kept} of its lap");
            assert!(kept <= 1.0, "{name} grew to {kept} of its lap");
        }
    }

    /// Being sent to a circuit builds it and tells everything else to put
    /// itself back, in that order — the reset is only worth anything if
    /// [`Track`] is already the circuit being reset onto. Being sent to the one
    /// already being driven is not a switch and costs nothing, because the menu
    /// opens with the cursor on it and `Enter` is the obvious thing to press.
    #[test]
    fn a_switch_builds_the_circuit_before_it_says_so() {
        #[derive(Resource, Default)]
        struct ResetsHeard(usize);
        fn count(mut resets: MessageReader<Reset>, mut heard: ResMut<ResetsHeard>) {
            heard.0 += resets.read().count();
        }

        let mut app = App::new();
        app.add_message::<Reset>()
            .add_message::<GoTo>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<ResetsHeard>()
            .insert_resource(Track::new(circuits::first()))
            .add_systems(Update, (switch, count).chain());

        let road = {
            let track = app.world().resource::<Track>();
            let mesh = track.profile.loft(&track.ribbon);
            app.world_mut().resource_mut::<Assets<Mesh>>().add(mesh)
        };
        let loft = app.world_mut().spawn((Loft, Mesh3d(road.clone()))).id();

        app.update();
        assert_eq!(
            app.world().resource::<ResetsHeard>().0,
            0,
            "reset by itself"
        );
        assert_eq!(
            app.world().resource::<Track>().circuit().id,
            "red-bull-ring"
        );

        // Sent to where it already is: nothing is rebuilt and nothing is
        // thrown away.
        app.world_mut().write_message(GoTo(circuits::first()));
        app.update();
        assert_eq!(app.world().resource::<ResetsHeard>().0, 0, "reset in place");
        assert_eq!(
            app.world().get::<Mesh3d>(loft).expect("the loft").0,
            road,
            "the circuit being driven was rebuilt for nothing"
        );

        let next = &circuits::all()[1];
        app.world_mut().write_message(GoTo(next));
        app.update();
        assert_eq!(
            app.world().resource::<Track>().circuit().id,
            next.id,
            "the switch did not arrive"
        );
        assert_eq!(
            app.world().resource::<ResetsHeard>().0,
            1,
            "a switch has to reset what is left of the old lap"
        );

        // And the road being drawn is the new circuit's, not the old one's.
        let drawn = app
            .world()
            .get::<Mesh3d>(loft)
            .expect("the loft is still there")
            .0
            .clone();
        assert_ne!(drawn, road, "the old circuit is still being drawn");
        let world = app.world();
        let track = world.resource::<Track>();
        assert_eq!(
            world
                .resource::<Assets<Mesh>>()
                .get(&drawn)
                .expect("the new loft was added")
                .count_vertices(),
            track.profile.loft(&track.ribbon).count_vertices(),
            "the loft drawn is not the one this circuit sweeps"
        );
    }

    /// A corner has to be a corner: something the driver goes round, not a bend
    /// the road is wide enough to ignore.
    ///
    /// The plan is shrunk about seven and a half times and the road only about
    /// one and a half, so the road comes out five times too wide for the land it
    /// is laid on. Left alone, that turns a chicane into a straight — the whole
    /// of Monza's Rettifilo displaces this car by less than half a road width,
    /// so the quick way through is not to steer. `Circuit::corners` is what buys
    /// it back, and this is what says whether it bought enough.
    ///
    /// Measured by laying a 40 m straight line down the circuit at every station
    /// and asking whether it stays on the asphalt. Long straights count too and
    /// are meant to — Monza is a third straights and is not broken — so the bar
    /// is loose. It is there to catch a circuit that is nearly all straight.
    #[test]
    fn corners_are_corners() {
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let n = stations.len();
            let window = (40.0 / (track.ribbon.length() / n as f32)) as usize;
            let straight = (0..n)
                .filter(|&i| {
                    let from = stations[i].pos;
                    let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z);
                    let dir = flat(stations[(i + window) % n].pos - from).normalize_or(Vec3::X);
                    (0..=window).all(|k| {
                        let off = flat(stations[(i + k) % n].pos - from);
                        (off - dir * off.dot(dir)).length() <= TARMAC_HALF
                    })
                })
                .count();
            let fraction = straight as f32 / n as f32;
            assert!(
                fraction < 0.6,
                "{name}: a 40 m straight fits down {:.0}% of the lap — \
                 there is not much circuit in there to drive round",
                fraction * 100.0
            );
        }
    }

    /// The menu can reach every circuit and land on the right row for each: the
    /// list is what it is drawn from, and where a circuit sits in that list is
    /// where the cursor opens.
    #[test]
    fn the_menu_can_reach_every_circuit() {
        let all = all_circuits();
        assert!(all.len() > 1, "there is only one circuit to list");
        assert_eq!(all[0].id, circuits::first().id, "the game opens off-list");
        for (at, circuit) in all.iter().enumerate() {
            assert_eq!(circuit_at(circuit), at, "{} is listed twice", circuit.name);
            assert!(
                !circuit.id.is_empty() && !circuit.name.is_empty(),
                "a circuit with nothing to show in a menu"
            );
        }
    }

    /// Every circuit fits inside the ideal cross-section, and none of them is a
    /// second copy of another.
    ///
    /// Asked of the shape rather than of where the grid slot lands. Every
    /// circuit is laid out about its own centroid, so they all sit on top of
    /// each other near the origin and two grid slots being close together says
    /// nothing about the circuits — Silverstone's start is 40 m from Monza's,
    /// and they are not remotely the same place. [`Track::fingerprint`] is of
    /// the finished centreline, which is exactly what "a different circuit"
    /// means, and it is already trusted to tell one lap's shape from another's
    /// when a saved ghost is read back.
    #[test]
    fn every_circuit_is_its_own_place() {
        let mut shapes = Vec::new();
        for (name, track) in every_track() {
            assert!(track.profile.edge() <= EDGE, "{name}");
            shapes.push((name, track.fingerprint(), track.circuit().id));
        }
        for (i, (name, shape, id)) in shapes.iter().enumerate() {
            for (other, theirs, other_id) in &shapes[i + 1..] {
                assert_ne!(shape, theirs, "{name} and {other} are the same shape");
                assert_ne!(id, other_id, "{name} and {other} are filed under one id");
            }
        }
    }
}
