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

// A trace is thousands of surveyed coordinates, and sooner or later one of them
// is 3.14 or 6.28 metres from the centroid of its circuit. It is a coordinate.
mod boards;
mod bridge;
#[allow(clippy::approx_constant)]
mod circuits;
mod loading;
mod markers;
mod profile;
mod ribbon;
mod rubber;
mod start;
mod terrain;
mod textures;
mod trackside;

use bevy::prelude::*;

use crate::Reset;
use crate::car::{Car, level};
use crate::menu::MenuSet;
pub(crate) use circuits::Circuit;
use profile::{HALF_WIDTH, Profile};
use ribbon::{Overpass, Ribbon};

/// Shared plan scale; cramped circuits also apply [`Circuit::plan_scale`].
/// Ten percent shorter than the previous 0.4 / 3.0 scale.
const PLAN_SCALE: f32 = 0.12;
/// Shrink elevation with the plan so shorter laps retain their hill gradients.
const HEIGHT_SCALE: f32 = 0.252;
/// How thick a bridge deck is: the road above, and the structure it is carried
/// on, between the surface the car drives on and the soffit the car below
/// drives under.
///
/// A number the game chooses, like the clearance beside it in
/// [`Circuit::crossings`]. What the elevation model knows about a crossing is
/// that there is one; how far apart the two roads are is not in a 90 m ground
/// model and is not pretended to be.
const DECK: f32 = 0.4;
/// How far either side of a crossing the deck is held level, and how long the
/// ramps onto it are, as a multiple of how far it has to rise.
///
/// The deck has to be level across the whole of the road underneath it and a
/// margin either side, or the road below passes under a slope and the clearance
/// is whatever the slope happens to be at the worst point of it. How far along
/// the deck that reaches depends on the angle the two roads cross at — square
/// on it is one cross-section either side, and shallower it is more — so three
/// cross-sections covers everything down to a crossing of twenty degrees.
///
/// The ramp is set against the rise rather than against the road, because what
/// it is for is a gradient: at ten times the rise a raised-cosine ramp peaks at
/// 15.7%, which is inside [`ribbon::MAX_GRADE`] with enough left over for the
/// ground it is built on to be sloping too.
const DECK_SPAN: f32 = 3.0 * profile::EDGE;
const RAMP_PER_RISE: f32 = 10.0;
/// How far out from the centreline the lookup will consider a stretch of road
/// to be under the car. The cross-section, and the wall's own slack beyond it.
const UNDER_THE_CAR: f32 = profile::EDGE + WALL_INSET;
/// The furthest round the lap the car can have got since the last time it was
/// asked, for the purpose of believing it is still on the same deck.
///
/// Generous: the car does 28 m/s at the very most and is asked at least once a
/// frame, so this is a third of a second at a standstill-to-flat-out pace it
/// does not have. It only has to be small against the distance between the two
/// decks of a crossing, which is most of a lap — at Suzuka it is 370 m.
///
/// What it is for is the difference between the car driving and the car being
/// put somewhere: a reset, a rescue, a change of circuit, a ghost being placed
/// at a saved pose. Those are not continuous, and reading them as continuous
/// would hold the car to a deck it is no longer anywhere near.
const A_STEP_ALONG: f32 = 12.0;
/// How near the start/finish line, measured round the lap, the car has to be
/// for crossing the start plane to be crossing the *line*.
///
/// The plane is infinite and the line is not. Without this, a car on the lower
/// road of a bridge that happens to sit under the start plane completes a lap
/// by driving under one, and so does a car cutting across a hairpin whose two
/// legs the plane runs through. The lateral test either side of this was doing
/// the same job in plan; this does it along the lap.
const ON_THE_LINE: f32 = 20.0;
/// How far apart in height two stretches of road have to be before they are two
/// decks rather than the circuit coming close to itself.
///
/// Both happen, and only one of them is a question. Baku runs back past itself
/// within five metres round the old town and Zandvoort within five at Hugenholtz
/// — near enough that a car in the middle of one road is inside the lookup's
/// reach of the other — and at both of them the nearest road is simply the road,
/// as it has always been. A bridge is the other thing: two roads at one point of
/// the map with air between them. Half of the shallowest bridge the game builds.
const A_DECK_APART: f32 = 1.0;

/// How far inside the edge of the loft the car is held. It may run wide onto the
/// verge, but not off into the sky.
const WALL_INSET: f32 = 0.45;
/// Fraction of the impact the wall gives back. Absorbing it all lets a car that
/// spun in nose-first sit there with its wheels spinning, because everything it
/// does is outward and everything outward is deleted.
const BOUNCE: f32 = 0.45;
/// Sitting off the circuit going nowhere for this long earns a lift back to the
/// racing line. A barrier you can wedge yourself against for good is worse than
/// no barrier at all.
const RESCUE_AFTER: f32 = 1.6;
const GOING_NOWHERE: f32 = 1.5;
/// Run-up to the line, shortened with the circuit so the grid stays on the
/// same approach. Driving tests check that every circuit reaches it at speed.
const RUN_UP: f32 = 40.5;
/// How much of a circuit has to survive being shrunk and having its corners
/// opened for what is left to still be that circuit. See [`Ribbon::kept`]: the
/// two in the game keep about nine tenths, and a circuit that keeps a quarter
/// has been rounded off into a ring.
const LEAST_KEPT: f32 = 0.75;

/// Installing the chosen circuit runs in here, after the menu that chose it and
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
        let circuit = app
            .world()
            .get_resource::<crate::settings::Settings>()
            .and_then(crate::settings::Settings::chosen_circuit)
            .unwrap_or_else(circuits::first);
        app.init_resource::<start::Finish>()
            .init_resource::<loading::Loading>()
            .init_resource::<trackside::Prepared>();
        app.insert_resource(Track::new(circuit))
            .add_message::<GoTo>()
            .add_systems(Startup, (setup, loading::setup))
            .add_systems(PreUpdate, loading::switch.in_set(TrackSet).after(MenuSet))
            .add_systems(Update, loading::show)
            .add_systems(
                Update,
                (
                    markers::rebuild.run_if(resource_changed::<Track>),
                    trackside::rebuild.run_if(resource_changed::<Track>),
                    start::rebuild.run_if(resource_changed::<Track>),
                    markers::show,
                    start::finish,
                    textures::prepare,
                )
                    .chain(),
            );
    }
}

/// The loft, so a switch knows whose mesh to replace.
#[derive(Component)]
struct Loft;

/// Grass inside and outside the loft, bounded by the course rectangle.
#[derive(Component)]
struct Terrain;

#[derive(Component)]
struct Asphalt;

type LoftMesh = (With<Loft>, Without<Terrain>, Without<Asphalt>);
type TerrainMesh = (With<Terrain>, Without<Loft>, Without<Asphalt>);
type AsphaltMesh = (With<Asphalt>, Without<Loft>, Without<Terrain>);

/// Every circuit there is, in the order the menu lists them.
pub(crate) fn all_circuits() -> &'static [Circuit] {
    circuits::all()
}

/// Where a circuit sits in that list.
pub(crate) fn circuit_at(circuit: &Circuit) -> usize {
    circuits::at(circuit)
}

/// Drive that one. Written by the circuit menu and acted on by [`loading::switch`];
/// nothing outside this module knows how a circuit is built, and nothing inside
/// it knows which key was pressed.
#[derive(Message)]
pub(crate) struct GoTo(pub &'static Circuit);

/// Half the width of the road. What anything asking "how far off-line is a
/// lot?" measures against — the drivers, and the harness that judges them.
pub(crate) use profile::HALF_WIDTH as ROAD_HALF;
/// The tightest corner the game allows, which is what a car's hardest stop is
/// measured down to. Read by the garage's report in [`crate::car`], so the
/// figure the corner markers are a ruler for comes from the same place the
/// markers themselves read it.
#[cfg(test)]
pub(crate) use ribbon::MIN_RADIUS;
/// A circuit that passes over itself, for anything that needs one to hand.
#[cfg(test)]
pub(crate) use tests::figure_of_eight;

#[derive(Resource)]
pub struct Track {
    circuit: &'static Circuit,
    ribbon: Ribbon,
    profile: Profile,
    terrain: std::sync::OnceLock<terrain::Surface>,
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
        let plan = PLAN_SCALE * circuit.plan_scale;
        let control: Vec<Vec3> = circuit
            .centreline
            .iter()
            .map(|p| Vec3::new(p[0] * plan, p[1] * HEIGHT_SCALE, p[2] * plan))
            .collect();
        // What a crossing is in the trace's terms, put into the ribbon's. The
        // rise is the clearance the crossing asked for plus the deck that
        // carries it; the deck span and the ramps follow from the road and from
        // that rise, so a circuit says only where its bridge is and how much
        // air it wants under it.
        let over: Vec<Overpass> = circuit
            .crossings
            .iter()
            .map(|c| {
                // What the two centrelines have to be apart for the two roads
                // to be `clearance` apart. The lowest thing about the road on
                // top is the outer edge of its verge and the highest thing
                // about the road below is the lip of its kerb, so the deck they
                // are measured between is not the whole of it.
                let rise = c.clearance + DECK + profile::SECTION_DEEP;
                Overpass {
                    at: Vec3::new(c.at[0] * plan, 0.0, c.at[1] * plan),
                    over: c.over,
                    rise,
                    deck: DECK_SPAN,
                    ramp: RAMP_PER_RISE * rise,
                }
            })
            .collect();
        let ribbon = Ribbon::new(&control, circuit.corners, &over);
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
            terrain: std::sync::OnceLock::new(),
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

    /// How far round the lap the grid slot is. What a car put down on the grid
    /// knows about itself before it has moved, so that the first thing it is
    /// asked is answered by continuity like every one after it.
    pub(crate) fn start_along_lap(&self) -> f32 {
        self.ribbon.before_start(RUN_UP).s
    }

    /// Signed distance past the start/finish plane, along the circuit.
    pub(crate) fn start_along(&self, pos: Vec3) -> f32 {
        let start = self.ribbon.start();
        (pos - start.pos).reject_from(Vec3::Y).dot(start.tangent)
    }

    /// Did the car cross the line near enough to the road for it to have been a
    /// lap? The road, and a car's width of grass either side of it — a driver
    /// who put two wheels on the verge over the line still drove the lap, and
    /// one who came past out in the runoff did not.
    ///
    /// Written against the car rather than as a distance, because the road is
    /// less than half the width it was and this has to keep meaning the same
    /// thing after it moved.
    pub(crate) fn on_start_gate(&self, pos: Vec3, was: Option<f32>) -> bool {
        let start = self.ribbon.start();
        let across = (pos - start.pos).reject_from(Vec3::Y).dot(start.right);
        if across.abs() >= HALF_WIDTH + 2.0 * crate::car::HALF_TRACK {
            return false;
        }
        // And on the road the line is on, which is not the same question: the
        // start plane goes on for ever, and the car may be under it rather than
        // over it.
        let along = self.fix(pos, was).s;
        along.min(self.ribbon.length() - along) < ON_THE_LINE
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
    pub(crate) fn length(&self) -> f32 {
        self.ribbon.length()
    }

    /// 0 at start/finish, approaching 1 at the end of the lap.
    pub(crate) fn progress(&self, pos: Vec3, was: Option<f32>) -> f32 {
        self.fix(pos, was).s / self.ribbon.length()
    }

    /// Which piece of road `pos` is on, and where on it.
    ///
    /// Almost always there is one piece of road at a point of the map and this
    /// is the ribbon's own answer. Where the circuit passes over itself there
    /// are two, and something has to choose. Two things do, in this order.
    ///
    /// **Continuity.** `was` is how far round the lap the asker was the last
    /// time it asked. A car cannot get from one deck of a bridge to the other
    /// without driving most of a lap, so a deck within [`A_STEP_ALONG`] of
    /// where the car already was is the deck the car is still on — and that
    /// holds on a ramp, off the centreline, and while the wall is pushing the
    /// car sideways, none of which height can be relied on for.
    ///
    /// **Height.** With no history, or with history that cannot be reconciled
    /// — the car has been put somewhere rather than driven there — the deck
    /// whose surface is nearest the height asked about wins. That is the
    /// documented answer to the arbitrary query, and it is why a ghost dropped
    /// onto a saved pose lands on the right road: the pose carries its height.
    ///
    /// Ties go to the earlier station, as everywhere else in the lookup.
    fn fix(&self, pos: Vec3, was: Option<f32>) -> ribbon::Fix {
        // A circuit that does not pass over itself has one road at a point of
        // the map, and the nearest of it is it. Thirty-eight of the thirty-nine
        // take this line and are answered exactly as they were before there
        // were bridges at all.
        if self.circuit.crossings.is_empty() {
            return self.ribbon.locate(pos);
        }
        let mut decks = self.ribbon.nearby(pos, UNDER_THE_CAR);
        let surface =
            |fix: &ribbon::Fix| fix.point.y + self.profile.height(fix.at, fix.t, fix.lateral);
        let across = |fix: &ribbon::Fix| (pos - fix.point).reject_from(Vec3::Y).length();
        let nearest = decks
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| across(a).total_cmp(&across(b)))
            .map(|(at, _)| at)
            .expect("at least one deck");
        // Only what is vertically clear of the nearest road counts as another
        // deck. The rest is the circuit running back past itself in plan, and
        // Suzuka does that in places that are not its bridge.
        let mine = surface(&decks[nearest]);
        let contenders: Vec<usize> = (0..decks.len())
            .filter(|&i| i == nearest || (surface(&decks[i]) - mine).abs() > A_DECK_APART)
            .collect();
        if contenders.len() == 1 {
            return decks.swap_remove(nearest);
        }
        let lap = self.ribbon.length();
        let round = |a: f32, b: f32| {
            let d = (a - b).abs();
            d.min(lap - d)
        };
        let picked = was
            .and_then(|was| {
                contenders
                    .iter()
                    .copied()
                    .filter(|&i| round(decks[i].s, was) < A_STEP_ALONG)
                    .min_by(|&a, &b| round(decks[a].s, was).total_cmp(&round(decks[b].s, was)))
            })
            .unwrap_or_else(|| {
                contenders
                    .iter()
                    .copied()
                    .min_by(|&a, &b| {
                        (surface(&decks[a]) - pos.y)
                            .abs()
                            .total_cmp(&(surface(&decks[b]) - pos.y).abs())
                    })
                    .expect("at least one deck")
            });
        decks.swap_remove(picked)
    }

    /// How sharply the circuit turns `by` metres further along the lap.
    ///
    /// Distance along the ribbon, not a straight line through the world. A
    /// driver looking ahead wants to know what the road it is on does next, and
    /// a probe fired down the car's nose leaves the road at the first corner —
    /// it can land on a neighbouring straight and read that straight's
    /// curvature as the corner it is about to arrive at. Following the ribbon
    /// cannot: the probe goes where the road goes.
    pub(crate) fn curvature_ahead(&self, from: &Ground, by: f32) -> f32 {
        self.ribbon.along(from.s, by).curvature
    }

    fn terrain(&self) -> &terrain::Surface {
        self.terrain
            .get_or_init(|| terrain::Surface::new(terrain::fill(&self.profile, &self.ribbon)))
    }

    pub(crate) fn map_points(&self) -> impl Iterator<Item = (Vec3, f32)> + '_ {
        self.ribbon.stations().iter().map(|s| (s.pos, s.s))
    }

    pub(crate) fn bridge_at(&self, along: f32) -> bool {
        profile::deep(self.ribbon.along(along, 0.0)) > 0.0
    }

    pub(crate) fn sector_count(&self) -> usize {
        (self.length() / 180.0).ceil().clamp(4.0, 8.0) as usize
    }

    /// A lap remains legal while any tyre contact overlaps asphalt or kerb.
    pub(crate) fn legal_contact(&self, at: &Transform, was: Option<f32>) -> bool {
        use crate::car::{FRONT_AXLE, HALF_TRACK, REAR_AXLE, WHEEL_WIDTH};
        let forward = level(*at.forward());
        let right = forward.cross(Vec3::Y);
        [FRONT_AXLE, -REAR_AXLE].into_iter().any(|axle| {
            [-HALF_TRACK, HALF_TRACK].into_iter().any(|side| {
                let hub = at.translation + forward * axle + right * side;
                let fix = self.fix(hub, was);
                fix.lateral.abs() <= HALF_WIDTH + WHEEL_WIDTH * 0.5
                    && (hub.y - fix.point.y).abs() < 0.5
            })
        })
    }

    /// Follow the road or surrounding grass; recover at the terrain boundary
    /// or when stranded. Only a raised bridge still needs an edge barrier.
    ///
    /// This is the only thing the circuit does to the car. Everything else the
    /// road asks of it — grip, the pull of a climb, the kerb under a wheel —
    /// reaches the car through [`Track::ground`], so the driving model stays in
    /// one place.
    pub(crate) fn hold(&self, transform: &mut Transform, car: &mut Car, dt: f32) {
        let ground = self.ground_from(transform.translation, car.along);
        // Off the road and going nowhere: a spin into the barrier leaves the car
        // nose-first against it, where everything it does is outward and
        // everything outward is taken away.
        if car.velocity.length() < GOING_NOWHERE
            && (ground.lateral.abs() > HALF_WIDTH || ground.grip <= profile::GRASS_GRIP)
        {
            car.stranded += dt;
        } else {
            car.stranded = 0.0;
        }
        if car.stranded > RESCUE_AFTER {
            self.rescue(transform, car);
            return;
        }

        // Only the outer terrain boundary recovers the car. The bridge rails
        // still guard a drop from its raised deck, using the existing bounce.
        if ground.lateral.abs() > ground.edge {
            let p = Vec2::new(transform.translation.x, transform.translation.z);
            if !self.terrain().contains(p) {
                self.rescue(transform, car);
                return;
            }
        }
        let fix = self.fix(transform.translation, car.along);
        if profile::deep(&self.ribbon.stations()[fix.at]) > 0.0
            && ground.lateral.abs() > ground.edge - WALL_INSET
            && (transform.translation.y - fix.point.y).abs() < 1.0
        {
            let side = ground.lateral.signum();
            transform.translation +=
                ground.right * (side * (ground.edge - WALL_INSET) - ground.lateral);
            let outward = car.velocity.dot(ground.right) * side;
            if outward > 0.0 {
                car.velocity -= ground.right * outward * (1.0 + BOUNCE) * side;
            }
        }
        let ground = self.ground_from(transform.translation, car.along);
        car.along = Some(ground.s);

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
        let ground = self.ground_from(transform.translation, car.along);
        *transform = Transform::from_translation(ground.centre)
            .looking_to(ground.tangent, Vec3::Y)
            .with_scale(transform.scale);
        *car = Car {
            // Everything about the lap the car was having is thrown away; where
            // it is is not, because it has been put back on the road it was
            // taken off and it is still on that road.
            along: Some(ground.s),
            recovered: true,
            ..Car::default()
        };
    }

    /// What the car is standing on. The loft is the only surface in the world,
    /// so this reads the same [`profile`] the mesh was swept from — the car
    /// rides the kerb because the kerb is 5 cm proud in the profile, not because
    /// anything says so twice.
    #[cfg(test)]
    pub(crate) fn ground(&self, pos: Vec3) -> Ground {
        self.ground_from(pos, None)
    }

    /// The same, for something that knows where it was last time. See
    /// [`Track::fix`]: on a circuit that passes over itself, that is the
    /// difference between the road the car is on and the one under it.
    pub(crate) fn ground_from(&self, pos: Vec3, was: Option<f32>) -> Ground {
        let beneath = |fix: &ribbon::Fix| {
            profile::deep(&self.ribbon.stations()[fix.at]) > 0.0
                && pos.y < fix.point.y - A_DECK_APART
        };
        let mut fix = self.fix(pos, was);
        // Grass now extends under the bridge. A car down there cannot join
        // its deck just because the upper road is nearest in plan.
        if beneath(&fix) {
            fix = self.fix(pos, None);
        }
        let edge = self.profile.reach(fix.at, fix.t, fix.lateral);
        let terrain = (fix.lateral.abs() > edge || beneath(&fix))
            .then(|| self.terrain().sample(Vec2::new(pos.x, pos.z)))
            .flatten();
        Ground {
            centre: fix.point,
            height: terrain.map_or_else(
                || fix.point.y + self.profile.height(fix.at, fix.t, fix.lateral),
                |(y, _)| y,
            ),
            tangent: fix.tangent,
            right: fix.right,
            lateral: fix.lateral,
            s: fix.s,
            edge: self.profile.reach(fix.at, fix.t, fix.lateral),
            slope: terrain.map_or(fix.slope, |(_, gradient)| {
                gradient.dot(Vec2::new(fix.tangent.x, fix.tangent.z))
            }),
            curvature: fix.curvature,
            grip: if terrain.is_some() {
                profile::GRASS_GRIP
            } else {
                profile::grip(fix.lateral)
            },
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
    /// Plan distance round the lap from the start/finish line. What a driver
    /// looking up the road counts from.
    pub(crate) s: f32,
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
    assets: Res<AssetServer>,
    track: Res<Track>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let [scenery, road, grass] = surfaces(&track);
    let (road_material, grass_material) = textures::materials(&mut commands, &assets);
    commands.spawn((
        Asphalt,
        Mesh3d(meshes.add(road)),
        MeshMaterial3d(materials.add(road_material)),
    ));
    commands.spawn((
        Loft,
        Mesh3d(meshes.add(scenery)),
        MeshMaterial3d(materials.add(StandardMaterial {
            unlit: true,
            ..default()
        })),
    ));
    commands.spawn((
        Terrain,
        Mesh3d(meshes.add(grass)),
        MeshMaterial3d(materials.add(grass_material)),
    ));
}

/// CPU-only geometry, shared by startup and background circuit loading.
fn surfaces(track: &Track) -> [Mesh; 3] {
    let (mut scenery, mut road, mut grass) = track.profile.surfaces(&track.ribbon);
    rubber::apply(&mut road, &track.ribbon);
    let terrain = track.terrain().mesh.clone();
    if let Some(details) = bridge::mesh(&track.profile, &track.ribbon, &terrain) {
        scenery
            .merge(&details)
            .expect("bridge shares the scenery attributes");
    }
    grass
        .merge(&terrain)
        .expect("grass meshes share attributes");
    [scenery, road, grass]
}

#[cfg(test)]
mod tests;
