//! The circuit: one closed centreline spline and one cross-section profile.
//!
//! Tarmac, edge lines, kerbs and grass are all strips of a single loft over the
//! stations in [`ribbon`]. Every strip is generated from the same station, the
//! same `right` vector, the same station index and the one cross-section fitted
//! at that station, so neighbouring strips share their edge vertices exactly.
//! Nothing overlaps anything, so nothing can z-fight, tear open, or stair-step
//! away from its neighbour.
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
/// How much of a lap may sit at [`ribbon::MAX_GRADE`] before the cap has
/// stopped backing the hills up and started being their shape. At the cap the
/// game ships with, the worst circuit is Spa at 16%, then Imola at 9% and
/// Spielberg at 7%; halve the cap and Spa is at 67% and Spielberg at 60%, which
/// is every hill on both of them coming out as the same ramp.
#[cfg(test)]
const PINNED_TO_THE_CAP: f32 = 0.25;

/// How much of its relief a circuit has to bring through the smoothing. Lower
/// than it looks, because the smoothing span is fixed in metres while relief is
/// not: a circuit whose height is in short features loses more of it than one
/// whose height is in long climbs. Las Vegas is the worst of them at 73%, its
/// rises being underpasses shorter than the span they are averaged over.
#[cfg(test)]
const KEPT_RELIEF: f32 = 0.7;

/// How far behind the start plane the grid has to read, at the very least. Two
/// car lengths: nearer than that and the lap would begin before the driver had
/// touched anything.
#[cfg(test)]
const CLEAR_OF_THE_LINE: f32 = 5.0;

/// Relief a circuit may lose to smoothing however flat it is, in metres of
/// Todora height. Three and a half steps of a DEM quantised to the metre, at
/// [`HEIGHT_SCALE`]: below this, what was lost is under the resolution of what
/// the elevation model was able to say in the first place.
#[cfg(test)]
const FLATTENED: f32 = 1.0;

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
    /// Taken from the finished driving surface rather than from the trace it
    /// came from, so it moves when *anything* that shapes a circuit moves: the
    /// scales here, the trace itself, and every constant in [`ribbon`] — how far
    /// the corners are opened, how the heights are smoothed, how steep a grade
    /// is allowed to be. A lap saved around one shape means nothing around
    /// another, so this goes into the saved file and is checked on the way back
    /// in. FNV-1a, because it only has to notice a change, not resist anyone.
    ///
    /// The centreline used to be the whole of it, and that was right while every
    /// circuit carried the same road and the same verge for the whole of its
    /// lap: nothing about the surface could move unless the centreline did. It
    /// is not right now. The verge is fitted station by station, so a circuit
    /// can keep its centreline to the bit and still put the wall somewhere else,
    /// and a wall somewhere else is a lap that could not have been driven. So
    /// the cross-section goes in too — both verge widths at every station, which
    /// is everything the surface is that the centreline is not.
    ///
    /// What deliberately does *not* go in is paint. Recolouring a kerb stripe
    /// changes the mesh and changes nothing a lap time depends on, and throwing
    /// away everyone's ghosts over it would teach people to distrust the check.
    pub(crate) fn fingerprint(&self) -> u64 {
        hash(&surface(&self.ribbon, &self.profile))
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

    /// How far out the car is held where it is standing. Inside the edge of the
    /// loft, wherever this circuit's verge had to stop *here* — the verge is
    /// fitted station by station, so the wall follows it rather than being set
    /// once to the narrowest place on the lap.
    fn wall(&self, ground: &Ground) -> f32 {
        ground.edge - WALL_INSET
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

        let wall = self.wall(&ground);
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
            height: fix.point.y + self.profile.height(fix.at, fix.t, fix.lateral),
            tangent: fix.tangent,
            right: fix.right,
            lateral: fix.lateral,
            edge: self.profile.reach(fix.at, fix.t, fix.lateral),
            slope: fix.slope,
            curvature: fix.curvature,
            grip: profile::grip(fix.lateral),
        }
    }
}

/// Everything the shape of a lap is made of, as a flat run of numbers: the
/// centreline, and the cross-section swept at each station of it.
///
/// Written out rather than hashed in place so that what goes in can be looked
/// at. What is in here is what a lap time depends on; what is left out is
/// [`profile::Band`], which is paint.
fn surface(ribbon: &Ribbon, profile: &Profile) -> Vec<f32> {
    let stations = ribbon.stations();
    let mut out = Vec::with_capacity(stations.len() * (3 + 2 * profile::RIBS));
    for (i, station) in stations.iter().enumerate() {
        out.extend(station.pos.to_array());
        for (lateral, height, _) in profile.at(i) {
            out.push(lateral);
            out.push(height);
        }
    }
    out
}

/// FNV-1a over the bits. It only has to notice a change, not resist anyone.
fn hash(values: &[f32]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    for value in values {
        for byte in value.to_bits().to_le_bytes() {
            hash = (hash ^ byte as u64).wrapping_mul(PRIME);
        }
    }
    hash
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
    /// How far the cross-section reaches from the centreline on this side, here.
    /// The wall is set [`WALL_INSET`] inside it.
    pub(crate) edge: f32,
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

    /// Three things at once, on every circuit.
    ///
    /// No step is steeper than the cap — the DEM is quantised to whole metres
    /// and the plan is shrunk five times harder than the height, so an
    /// unsmoothed step would read as a wall.
    ///
    /// Little of the lap is *pinned* to that cap, which is what says the cap is
    /// backing the hills up rather than shaping them. This is the one that
    /// catches a cap set too low, and it catches it directly: halving the cap
    /// takes Spa from a sixth of its lap pinned to two thirds of it.
    ///
    /// And most of the relief the trace carried survives the smoothing, which
    /// catches the other end — a span long enough to iron the circuits flat.
    /// This one has to be read carefully, because the span is in metres and the
    /// relief is not: see [`KEPT_RELIEF`] and [`FLATTENED`].
    ///
    /// Stated as a fraction rather than as metres, because the circuits are not
    /// equally hilly. Spa rises 27 m and Monza 6, and Monza is not broken — it
    /// is Monza.
    ///
    /// A fraction on its own is not enough at the flat end, though, because the
    /// smoothing span is fixed in metres while the relief is not: the flatter a
    /// circuit is, the larger a share of it 24 m of averaging takes. Mexico City
    /// climbs 5 m in life, which is 1.4 m here, and comes out with 0.9 of them.
    /// So a circuit passes on either count — it kept most of its relief, or what
    /// it lost is under [`FLATTENED`], which is below the resolution of the
    /// question being asked.
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
            let pinned = (0..n)
                .filter(|&i| {
                    let a = stations[i].pos;
                    let b = stations[(i + 1) % n].pos;
                    let run = Vec3::new(b.x - a.x, 0.0, b.z - a.z).length().max(1e-4);
                    (b.y - a.y).abs() / run > ribbon::MAX_GRADE * 0.98
                })
                .count() as f32
                / n as f32;
            assert!(
                pinned < PINNED_TO_THE_CAP,
                "{name}: {:.0}% of the lap is pinned to the grade cap, which is \
                 the cap doing the shaping rather than backing it up",
                pinned * 100.0
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
            let lost = wanted - (high - low);
            assert!(
                high - low > KEPT_RELIEF * wanted || lost < FLATTENED,
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
            for side in [-1.0f32, 1.0] {
                // How far the grass reaches on this side *here*, which is what
                // the reading out on it has to be taken against: the verge is
                // fitted station by station now, so the depth at two thirds of
                // the way out is not a constant of the game.
                let edge = track.ground(start.translation + right * side).edge;
                let lip = HALF_WIDTH + (2.0 / 3.0) * (edge - HALF_WIDTH);
                for (across, grip, height) in [
                    (0.0, 1.0, 0.0),
                    (TARMAC_HALF - 0.01, 1.0, 0.0),
                    (HALF_WIDTH - 0.01, KERB_GRIP, KERB_TOP * (0.69 / 0.70)),
                    (lip, GRASS_GRIP, KERB_TOP - 0.125 * (lip - HALF_WIDTH)),
                ] {
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
        let wall = track.wall(&track.ground(start.translation));
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
            for station in track.ribbon.stations().iter().step_by(20) {
                for side in [-1.0, 1.0] {
                    let wall = track.wall(&track.ground(station.pos + station.right * side));
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
                    assert!(ground.lateral.abs() <= track.wall(&ground) + 0.01);
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

    /// The grid is a run-up, and a run-up has to be usable.
    ///
    /// Three things, and the first is what the grid *means*: it is [`RUN_UP`]
    /// metres back along the road, measured round the circuit the way a lap is.
    ///
    /// The second is that the start plane agrees it is behind there, with room
    /// to spare — that plane is what the clock reads to know the car has
    /// arrived, and a grid sitting on it would start the lap before the driver
    /// touched anything. How far behind it reads is *not* the run-up and cannot
    /// be asked to be: the plane measures a straight line while the road bends,
    /// so the two only agree where the run-up is straight. Mexico City's last
    /// 45 m are the stadium section, which turns the car through most of a half
    /// circle, so its grid reads 19 m behind the plane while being 45 m behind
    /// the line.
    ///
    /// The third is that the car faces the way the lap runs *where it is put
    /// down*, which is not the way the line faces and must not be confused with
    /// it. This used to ask for the two to agree within 20 degrees, which was
    /// only ever true of circuits whose run-up happened to be straight: Mexico
    /// City's grid faces 155 degrees away from its line and is perfectly correct
    /// — it is pointing down the stadium section, which is where the road goes.
    /// What a bent run-up costs is speed at the line, and that is asked where
    /// the car is, in `the_run_up_reaches_the_line_at_speed`.
    #[test]
    fn the_grid_is_a_run_up_to_the_line() {
        for (name, track) in every_track() {
            let pose = track.start_transform();
            let back = (1.0 - track.progress(pose.translation)) * track.ribbon.length();
            assert!(
                (RUN_UP - 1.0..RUN_UP + 1.0).contains(&back),
                "{name} sets the car down {back:.1} m back along the road, not {RUN_UP}"
            );

            let along = track.start_along(pose.translation);
            assert!(
                (-RUN_UP - 1.0..=-CLEAR_OF_THE_LINE).contains(&along),
                "{name} reads {along:.1} m behind its own start plane"
            );

            let here = track.ribbon.locate(pose.translation);
            assert!(
                level(*pose.forward()).dot(here.tangent) > 0.99,
                "{name} sets the car down across its own road rather than down it"
            );
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

    /// A lap is a lap of the surface it was driven on, and the fingerprint has
    /// to move when that surface does — in any of the ways it can.
    ///
    /// This is a property correction, and the wrong proxy is worth naming. The
    /// fingerprint used to be of the finished *centreline*, and that was a
    /// complete description of the driving surface for exactly as long as every
    /// circuit carried the same road and the same verge from its first station
    /// to its last: nothing about the surface could move unless the centreline
    /// moved. The verge is fitted station by station now. A circuit can keep
    /// its centreline to the bit and put the wall a metre further in, and a
    /// saved lap that used that metre would replay through the scenery with the
    /// clock saying it was fine.
    ///
    /// So the claim is now about the surface and not the line: the cross-section
    /// at every station goes into the hash as well. And the negative half of it
    /// is checked too — paint is deliberately not in there, because recolouring
    /// a kerb stripe changes the mesh, changes nothing a lap time depends on,
    /// and throwing away everyone's ghosts over it would teach people to
    /// distrust the check.
    #[test]
    fn a_lap_belongs_to_the_surface_it_was_driven_on() {
        let track = track();
        let ribbon = &track.ribbon;
        let profile = &track.profile;
        let mine = hash(&surface(ribbon, profile));
        assert_eq!(mine, track.fingerprint());
        // Building the same circuit again is the same surface.
        assert_eq!(mine, Track::any().fingerprint());

        // A centimetre off one station's left verge, and nothing else at all:
        // the same centreline, the same heights, the same paint. The old
        // centreline-only hash could not see this, which is the whole reason
        // this test exists.
        let narrower = profile.nudged(0, -0.01);
        assert_ne!(
            mine,
            hash(&surface(ribbon, &narrower)),
            "a width-only change left the fingerprint where it was"
        );

        // A height-only change: the same plan, the hills a hundredth taller.
        let circuit = track.circuit();
        let taller: Vec<Vec3> = circuit
            .centreline
            .iter()
            .map(|p| {
                Vec3::new(
                    p[0] * PLAN_SCALE,
                    p[1] * HEIGHT_SCALE * 1.01,
                    p[2] * PLAN_SCALE,
                )
            })
            .collect();
        let taller = Ribbon::new(&taller, circuit.corners);
        let fitted = Profile::fit(&taller).expect("the same circuit still carries a road");
        assert_ne!(
            mine,
            hash(&surface(&taller, &fitted)),
            "a height-only change left the fingerprint where it was"
        );

        // And paint is not in it. Two cross-sections of exactly the same shape,
        // one of them repainted: the same numbers go into the hash.
        let plain = profile::section(3.0, 3.0);
        let mut repainted = plain;
        repainted[0].2 = profile::Band::Grass;
        assert_ne!(plain[0].2, repainted[0].2, "nothing was actually repainted");
        let flatten = |ribs: &profile::Section| -> Vec<f32> {
            ribs.iter().flat_map(|&(l, h, _)| [l, h]).collect()
        };
        assert_eq!(
            hash(&flatten(&plain)),
            hash(&flatten(&repainted)),
            "repainting a strip would throw away every saved lap"
        );
        assert_ne!(
            hash(&flatten(&plain)),
            hash(&flatten(&profile::section(3.0, 2.99))),
            "a centimetre of verge reads as the same surface"
        );
    }

    /// What every circuit came out as: the table the bars in these tests are set
    /// against, and the first thing to look at when a new one will not go in.
    ///
    /// `cargo test --locked --lib the_circuits -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn the_circuits() {
        println!(
            "{:<26}{:>8}{:>7}{:>15}{:>8}{:>9}{:>8}",
            "circuit", "lap", "kept", "cross-section", "pinned", "straight", "relief"
        );
        for (name, track) in every_track() {
            let stations = track.ribbon.stations();
            let n = stations.len();
            let grade = |i: usize| {
                let (a, b) = (stations[i].pos, stations[(i + 1) % n].pos);
                let run = Vec3::new(b.x - a.x, 0.0, b.z - a.z).length().max(1e-4);
                (b.y - a.y).abs() / run
            };
            let pinned = (0..n)
                .filter(|&i| grade(i) > ribbon::MAX_GRADE * 0.98)
                .count();
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
            let (low, high) = stations.iter().fold((f32::MAX, f32::MIN), |(l, h), s| {
                (l.min(s.pos.y), h.max(s.pos.y))
            });
            let (narrowest, widest) = track.profile.span();
            println!(
                "{name:<26}{:>7.0}m{:>6.0}%{:>9.2}-{:>4.2}m{:>7.0}%{:>8.0}%{:>7.1}m",
                track.ribbon.length(),
                track.ribbon.kept() * 100.0,
                narrowest,
                widest,
                100.0 * pinned as f32 / n as f32,
                100.0 * straight as f32 / n as f32,
                high - low,
            );
        }
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
        assert!(
            all.iter().any(|c| c.id == circuits::first().id),
            "the game opens on a circuit the menu cannot reach"
        );
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
            assert!(track.profile.span().1 <= EDGE, "{name}");
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
