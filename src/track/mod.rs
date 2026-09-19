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
#[allow(clippy::approx_constant)]
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
use ribbon::{Overpass, Ribbon};

/// The car is the ruler: ~2.4 m long, ~1.1 m wide, 1 unit = 1 metre. Every
/// circuit is scaled by this, so a longer circuit makes a longer lap rather than
/// a bigger world, and a lap time means the same thing wherever it was set.
///
/// A few circuits are scaled by this *and* by a multiplier of their own,
/// because no amount of narrowing the road makes them fit at the shared scale.
/// See [`Circuit::plan_scale`], which is also where what that gives up is
/// written down.
const PLAN_SCALE: f32 = 0.4 / 3.0;
/// Elevation as a fraction of the real circuit, then smoothed and grade-capped.
/// The plan is scaled far harder than this, so the hills come out steeper than
/// real by the ratio of the two: at 0.4 that was three times, and every descent
/// arrived at its corner too fast to take. This still rolls.
const HEIGHT_SCALE: f32 = 0.28;
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

/// How much of its relief a circuit has to bring through the smoothing.
///
/// Lower than it looks, because the smoothing span is fixed in metres while
/// relief is not: a circuit whose height is in short features loses more of it
/// than one whose height is in long climbs. The worst are the Nürburgring and
/// Zandvoort at 65% and Hockenheim at 66%, and what the smoothing is refusing
/// on all three is an elevation model with steps in it that no road has —
/// 16 m between two fixes forty metres apart, which is a 40% gradient and is an
/// embankment the model has mistaken for the road.
///
/// The bar has to be low enough to let that through and high enough to still
/// catch a span long enough to iron the circuits flat, and it is: double the
/// span and the worst goes to 45%, with five circuits under this figure.
#[cfg(test)]
const KEPT_RELIEF: f32 = 0.6;

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

/// How far the trace may end up from the circuit built out of it, on average.
///
/// On average, and not at worst, because at worst is a question the retention
/// guard already answers. Opening a corner to the target radius moves its apex
/// by something like that radius, and the tighter the corner was the further it
/// moves: the Nürburgring's worst fix ends up 45 m from the circuit, and that
/// one fix is a hairpin that was far tighter than the loft can carry. Averaged
/// over the whole trace it is 5.4 m, and that is the number that says whether
/// the circuit as a whole is still where it was surveyed or has drifted off
/// somewhere else.
///
/// The bar is the diameter of the tightest corner the game allows, which is the
/// largest single change the pipeline is entitled to make to a corner. Nothing
/// is near it: the Nürburgring is worst at 5.4 m and Spa is at 0.7.
#[cfg(test)]
const MOVED: f32 = 2.0 * ribbon::MIN_RADIUS;

/// The least turning that ends one turn and starts another, rather than being
/// the road not going perfectly straight in the middle of a corner. A fifth of
/// a right angle.
#[cfg(test)]
const A_TURN: f32 = 0.35;

/// Turning a circuit has to do in a lap, in whole turns, before it is a lap of
/// a circuit rather than a lap of a ring.
///
/// A ring turns once. The circuits turn between two and five times — Monza is
/// lowest at 2.0 and Monaco highest — so this has room under all of them and a
/// long way to fall before it reaches a ring. It is the thing retention was
/// invented to catch and cannot: a circuit can keep nine tenths of its length
/// while the nine tenths it kept is a loop with the corners taken out of it.
#[cfg(test)]
const A_CIRCUIT_TURNS: f32 = 1.5;

/// How much of its trace's turning a circuit has to come out with.
///
/// The other half of the same question, and the half that notices a circuit
/// that was always going to be round. Gilles-Villeneuve is worst at 68% and
/// Monza next at 77%; what they lose is the corner-opening pass cutting the
/// corners, which is the pass working. Erase a chicane and this is what moves,
/// because the turning that was in it is simply gone.
#[cfg(test)]
const TURNING_KEPT: f32 = 0.6;

/// How many of its trace's changes of direction a circuit has to come out with.
///
/// Loose, because a change of direction is a threshold question and the two
/// sides of it are sampled forty metres apart and forty centimetres apart.
/// Buenos Aires is worst at four of eight, its other four being kinks of little
/// more than [`A_TURN`] that the spline rounds into the corners either side.
/// What it is here for is the wholesale case — a circuit that came out with
/// two changes of direction where its trace had fourteen has not been shrunk,
/// it has been replaced.
#[cfg(test)]
const CHANGES_KEPT: f32 = 0.5;

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
    #[cfg(test)]
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
        let mut ground = self.ground_from(transform.translation, car.along);
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
            ground = self.ground_from(transform.translation, car.along);
        }
        // Read back after the wall has moved the car, and before the height is
        // applied, so that what the car is put down on and what it is recorded
        // as being on are the same road.
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
            ..Car::default()
        };
    }

    /// What the car is standing on. The loft is the only surface in the world,
    /// so this reads the same [`profile`] the mesh was swept from — the car
    /// rides the kerb because the kerb is 5 cm proud in the profile, not because
    /// anything says so twice.
    pub(crate) fn ground(&self, pos: Vec3) -> Ground {
        self.ground_from(pos, None)
    }

    /// The same, for something that knows where it was last time. See
    /// [`Track::fix`]: on a circuit that passes over itself, that is the
    /// difference between the road the car is on and the one under it.
    pub(crate) fn ground_from(&self, pos: Vec3, was: Option<f32>) -> Ground {
        let fix = self.fix(pos, was);
        Ground {
            centre: fix.point,
            height: fix.point.y + self.profile.height(fix.at, fix.t, fix.lateral),
            tangent: fix.tangent,
            right: fix.right,
            lateral: fix.lateral,
            s: fix.s,
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
            // And what the cap does to get there is shave the crests, not press
            // the circuit flat: within a per cent, the relief it leaves is the
            // relief the smoothing handed it, the worst being Kyalami at 0.8%.
            // Worth stating, because the two are easy to blame for each other's
            // work — the relief a circuit loses, it loses entirely to the
            // smoothing — and only one of them is guarded by the fraction
            // below.
            assert!(
                track.ribbon.relief() > 0.98 * track.ribbon.smoothed_relief(),
                "{name}: the grade cap took {:.2} m of the {:.2} m of relief the \
                 smoothing left",
                track.ribbon.smoothed_relief() - track.ribbon.relief(),
                track.ribbon.smoothed_relief()
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
            let wanted = relief_of(track.circuit()) * HEIGHT_SCALE;
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
            let mut was_progress = track.progress(off_line(last), None);
            let mut was_along = track.start_along(off_line(last));
            for station in stations {
                let pos = off_line(station);
                let progress = track.progress(pos, None);
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
                if was_along <= 0.0 && along > 0.0 && track.on_start_gate(pos, None) {
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
                track.progress(pose.translation, None) > 1.0 - (RUN_UP + 1.0) / lap,
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
            let back = (1.0 - track.progress(pose.translation, None)) * track.ribbon.length();
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

    /// The plan of a circuit, as the trace drew it and at the scale it is built
    /// at. Heights left out: this is about where the circuit goes.
    fn traced(circuit: &Circuit) -> Vec<Vec3> {
        let plan = PLAN_SCALE * circuit.plan_scale;
        circuit
            .centreline
            .iter()
            .map(|p| Vec3::new(p[0] * plan, 0.0, p[2] * plan))
            .collect()
    }

    /// The order a closed plan changes direction in: `1` for right and `-1` for
    /// left, one entry each time the circuit stops turning one way and starts
    /// turning the other by at least [`A_TURN`].
    ///
    /// Turning is accumulated rather than read off single steps, and a change
    /// of direction only ends a turn once the turn is worth having ended. That
    /// is what lets the same question be asked of a trace whose fixes are forty
    /// metres apart and of a centreline whose stations are forty centimetres
    /// apart: a wobble on either is absorbed into the turn it is inside, and
    /// what comes out is the shape rather than the sampling.
    ///
    /// Two turns the same way running are then one entry, not two. Where one
    /// long right ends and the next begins is a matter of how straight the road
    /// got in between, and the answer moves by a turn or two between a trace
    /// and the circuit splined from it — Albert Park's back section reads as
    /// four rights on the trace and three on the circuit, and is the same four
    /// corners either way. What does not move is the order of the *changes*: a
    /// chicane is a left and then a right, and losing one is losing an entry.
    fn turns(line: &[Vec3]) -> Vec<i8> {
        let mut out: Vec<i8> = swings(line).into_iter().map(|(way, _)| way).collect();
        out.dedup();
        // The walk starts in the middle of whatever turn station zero is in, so
        // one turn can come out as two, one at each end. They are the same turn.
        if out.len() > 1 && out[0] == out[out.len() - 1] {
            out.pop();
        }
        out
    }

    /// Every turn of at least [`A_TURN`] in the closed plan, as a direction and
    /// how far it went through, in the order they come.
    fn swings(line: &[Vec3]) -> Vec<(i8, f32)> {
        let n = line.len();
        let mut out = Vec::new();
        let mut running = 0.0f32;
        for i in 0..n {
            let from = line[(i + 1) % n] - line[i];
            let to = line[(i + 2) % n] - line[(i + 1) % n];
            if from.length() < 1e-6 || to.length() < 1e-6 {
                continue;
            }
            let turn = f32::atan2(to.cross(from).y, to.dot(from));
            if running != 0.0 && turn.signum() != running.signum() && running.abs() >= A_TURN {
                out.push((running.signum() as i8, running.abs()));
                running = 0.0;
            }
            running += turn;
        }
        if running.abs() >= A_TURN {
            out.push((running.signum() as i8, running.abs()));
        }
        out
    }

    /// A circuit still goes where it was traced.
    ///
    /// Retention is a guard against a circuit being rounded off into a ring,
    /// and it is a good one, but it measures a length rather than a shape and
    /// it starts *after* the spline: a corner the splining removed was never in
    /// the lap it compares against. These two ask the shape directly.
    ///
    /// First, displacement. Every fix of the trace has to be near the finished
    /// centreline — near being measured against the cross-section, because a
    /// circuit displaced by less than its own road is a circuit the road still
    /// covers, and one displaced by more has moved. The bar is generous on
    /// purpose: splining a trace whose fixes are forty metres apart rounds its
    /// corners by design, and opening a corner cuts it by design. What it
    /// catches is a circuit that has gone somewhere else.
    ///
    /// Second, turn order. A chicane is a left and then a right; a hairpin is
    /// one long turn. Lose either and the sequence of turns changes, and the
    /// sequence is what a driver remembers a circuit by. Compared against the
    /// raw trace, so it sees what the spline and the corner-opening pass did as
    /// well as what the rest of the pipeline did.
    #[test]
    fn every_circuit_is_still_the_circuit_it_was_traced_from() {
        for (name, track) in every_track() {
            let drawn = traced(track.circuit());
            let moved = drifted(&track, &drawn);
            assert!(
                moved < MOVED,
                "{name}: the trace ends up {moved:.1} m from the circuit built \
                 out of it on average, against {MOVED} m"
            );

            let (was, now) = (wound(&drawn), wound(&plan_of(&track)));
            assert!(
                now > A_CIRCUIT_TURNS,
                "{name}: the circuit turns through {now:.1} laps' worth of \
                 corners, which is not far off a ring's one"
            );
            assert!(
                now > TURNING_KEPT * was,
                "{name}: the trace turns through {was:.1} laps' worth of corners \
                 and the circuit through {now:.1}"
            );

            let (was, now) = (turns(&drawn), turns(&plan_of(&track)));
            assert_eq!(
                now.first(),
                was.first(),
                "{name}: the trace and the circuit do not start turning the same way"
            );
            assert!(
                now.len() as f32 >= CHANGES_KEPT * was.len() as f32,
                "{name}: the trace changes direction {} times and the circuit {}",
                was.len(),
                now.len()
            );
        }
    }

    /// How far the trace ends up from the circuit built out of it, on average.
    fn drifted(track: &Track, drawn: &[Vec3]) -> f32 {
        drawn
            .iter()
            .map(|&fix| flat(fix - track.ribbon.locate(fix).point).length())
            .sum::<f32>()
            / drawn.len() as f32
    }

    /// The two measures above, put to things that are wrong on purpose.
    ///
    /// A property that has never been seen to fail is a property nobody knows
    /// the shape of, and both of these are measures rather than assertions —
    /// they have thresholds and detectors in them, and a detector that never
    /// says no is a detector that says nothing.
    ///
    /// The first is a circuit moved bodily off where it was traced. The second
    /// is the one the plan is actually about: a chicane taken out. A left and a
    /// right within a few metres of each other is two entries in the sequence;
    /// straighten the road between the same two points and both entries go, and
    /// nothing about the length of the lap has to change for that to happen —
    /// which is exactly what retention cannot see.
    #[test]
    fn the_identity_checks_notice_a_circuit_that_has_been_changed() {
        let track = track();
        let drawn = traced(track.circuit());
        assert!(drifted(&track, &drawn) < MOVED);
        let shoved: Vec<Vec3> = drawn.iter().map(|&p| p + Vec3::X * 3.0 * MOVED).collect();
        assert!(
            drifted(&track, &shoved) > MOVED,
            "a circuit moved bodily off its own trace reads as being on it"
        );

        // A square lap with a chicane down each of its four sides. Squared off
        // rather than round so that the corners and the chicanes are separate
        // things: the four right-angles are one lap of turning between them, and
        // the four chicanes are most of another.
        let lay = |chicanes: bool| -> Vec<Vec3> {
            let mut out = Vec::new();
            let (mut at, mut heading) = (Vec3::ZERO, Vec3::Z);
            let walk = |at: &mut Vec3, heading: Vec3, run: usize, out: &mut Vec<Vec3>| {
                for _ in 0..run {
                    *at += heading * 2.0;
                    out.push(*at);
                }
            };
            for _ in 0..4 {
                walk(&mut at, heading, 20, &mut out);
                if chicanes {
                    heading = Quat::from_rotation_y(0.7) * heading;
                    walk(&mut at, heading, 6, &mut out);
                    heading = Quat::from_rotation_y(-0.7) * heading;
                }
                walk(&mut at, heading, 20, &mut out);
                heading = Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2) * heading;
            }
            out
        };
        let (with, without) = (wound(&lay(true)), wound(&lay(false)));
        assert!(
            (without - 1.0).abs() < 0.05,
            "the square without its chicanes should turn through one lap, not {without:.2}"
        );
        assert!(
            without < TURNING_KEPT * with,
            "taking the chicanes out of a circuit left it turning through \
             {without:.2} laps against {with:.2}, which is not enough of a change \
             to notice"
        );
        assert!(
            (turns(&lay(false)).len() as f32) < CHANGES_KEPT * turns(&lay(true)).len() as f32,
            "taking the chicanes out left the changes of direction at {} of {}",
            turns(&lay(false)).len(),
            turns(&lay(true)).len()
        );
    }

    /// How far a closed plan turns through in a lap, in whole turns. A ring
    /// comes out at one however big it is; a circuit comes out at as many
    /// corners as it has.
    fn wound(line: &[Vec3]) -> f32 {
        swings(line).iter().map(|(_, through)| through).sum::<f32>() / std::f32::consts::TAU
    }

    /// The finished centreline, in plan.
    fn plan_of(track: &Track) -> Vec<Vec3> {
        track
            .ribbon
            .stations()
            .iter()
            .map(|s| flat(s.pos))
            .collect()
    }

    fn flat(v: Vec3) -> Vec3 {
        Vec3::new(v.x, 0.0, v.z)
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

    /// A circuit that passes over itself, built to order.
    ///
    /// A lemniscate: two lobes meeting at the origin at right angles, which is
    /// the smallest honest figure of eight there is. `turn` rolls the start
    /// line round it, so that the crossing can be put a long way from the line
    /// or right on top of it. Flat, because the point of it is the bridge and a
    /// bridge the elevation model helped with would prove less.
    ///
    /// This is the fixture the bridge was built against, before Suzuka. A real
    /// circuit brings a surveyed layout, a real elevation model and one
    /// crossing whose geometry is whatever it is; this brings a crossing at a
    /// known place, at a known angle, with known heights either side of it, and
    /// it can be asked for another one somewhere else.
    pub(crate) fn figure_of_eight(turn: f32, over: f32, clearance: f32) -> &'static Circuit {
        const FIXES: usize = 240;
        const REACH: f32 = 950.0;
        // A negative turn runs the same shape the other way round, which is a
        // circuit whose bridge is met from the other end and whose lap counts
        // the other way. Everything about the crossing is the same; everything
        // about arriving at it is reversed.
        let (turn, widdershins) = (turn.abs(), turn < 0.0);
        let plan: Vec<[f32; 3]> = (0..FIXES)
            .map(|k| {
                let k = if widdershins { FIXES - k - 1 } else { k };
                let t = turn + std::f32::consts::TAU * k as f32 / FIXES as f32;
                let (sin, cos) = t.sin_cos();
                let below = 1.0 + sin * sin;
                [REACH * cos / below, 0.0, REACH * sin * cos / below]
            })
            .collect();
        Box::leak(Box::new(Circuit {
            id: "figure-of-eight",
            name: "Figure of Eight",
            corners: 1.0,
            lap: 0.0,
            plan_scale: 1.0,
            centreline: Box::leak(plan.into_boxed_slice()),
            crossings: Box::leak(Box::new([circuits::Crossing {
                at: [0.0, 0.0],
                over,
                clearance,
                provenance: "a fixture: the crossing is where the shape puts it",
            }])),
        }))
    }

    /// A bridge has the air under it that was asked for, everywhere the two
    /// roads are one above the other.
    ///
    /// Measured over the whole of the overlapping footprint rather than at the
    /// point where the centrelines cross, because the roads are 7.3 m wide and
    /// they cross at an angle: the tightest place is out at the edge of one of
    /// them, not in the middle. Every rib of the upper deck, at every station
    /// of it that has road underneath, against the surface of whatever is
    /// underneath at that point of the map.
    ///
    /// The clearance is measured to the *underside* of the deck, which is
    /// [`DECK`] below the road. That is what a car driving under it has over
    /// its head, and it is the only part of the two numbers that a driver ever
    /// sees.
    #[test]
    fn a_bridge_has_the_air_under_it_that_was_asked_for() {
        for circuit in bridged() {
            air_under(circuit);
        }
    }

    /// A bridge says where it came from, and says which half of it is which.
    ///
    /// The only thing in a circuit module that is not a fact about the real
    /// circuit is the clearance: a 90 m ground model has nothing to say about
    /// how far a road deck is above the road under it, and an authored number
    /// dressed up as a measured one would be the worst kind of thing to leave
    /// in a data file. So every crossing carries a line about where each half
    /// of it came from, and this is what stops that line being dropped.
    #[test]
    fn a_bridge_says_where_it_came_from() {
        let mut found = 0;
        for circuit in circuits::all() {
            for crossing in circuit.crossings {
                found += 1;
                assert!(
                    crossing.provenance.contains("authored"),
                    "{}: a crossing whose clearance does not say it was authored: {}",
                    circuit.name,
                    crossing.provenance
                );
                assert!(
                    crossing.clearance > 0.0 && crossing.clearance < 10.0,
                    "{}: a clearance of {} m",
                    circuit.name,
                    crossing.clearance
                );
                assert!((0.0..1.0).contains(&crossing.over));
            }
        }
        assert_eq!(found, 1, "the game has {found} crossings, not one");
    }

    /// Every circuit that passes over itself: the one in the game, and the
    /// fixture the feature was built against before there was one.
    fn bridged() -> impl Iterator<Item = &'static Circuit> {
        circuits::all()
            .iter()
            .filter(|circuit| !circuit.crossings.is_empty())
            .chain([figure_of_eight(0.0, 0.75, 1.6)])
    }

    /// Where a circuit's declared crossings are, in the finished plan.
    fn crossings_of(circuit: &'static Circuit) -> Vec<Vec3> {
        let plan = PLAN_SCALE * circuit.plan_scale;
        circuit
            .crossings
            .iter()
            .map(|c| Vec3::new(c.at[0] * plan, 0.0, c.at[1] * plan))
            .collect()
    }

    fn air_under(circuit: &'static Circuit) {
        let track = Track::new(circuit);
        let at = crossings_of(circuit);
        let wanted = circuit
            .crossings
            .iter()
            .map(|c| c.clearance)
            .fold(f32::MAX, f32::min);
        let stations = track.ribbon.stations();
        let lap = track.ribbon.length();
        let mut tightest = f32::MAX;
        let mut measured = 0;
        for (i, station) in stations.iter().enumerate() {
            // Only at a bridge. Elsewhere a circuit can perfectly well have one
            // stretch a couple of metres above another and a few metres to the
            // side of it — Suzuka's esses do, on the slope they are cut into —
            // and nothing about that is a span with air under it.
            if !at
                .iter()
                .any(|at| (station.pos - *at).reject_from(Vec3::Y).length() < ribbon::AT_CROSSING)
            {
                continue;
            }
            for rib in track.profile.at(i) {
                let on_top = station.pos + station.right * rib.0 + Vec3::Y * rib.1;
                for below in track.ribbon.nearby(on_top, UNDER_THE_CAR) {
                    let apart = (below.s - station.s).abs();
                    // The road this rib was swept from is not underneath it.
                    if apart.min(lap - apart) < A_STEP_ALONG {
                        continue;
                    }
                    let under =
                        below.point.y + track.profile.height(below.at, below.t, below.lateral);
                    // Only where this rib really is above the other road, and
                    // not beside it: a bridge is not asked to clear its own
                    // approach.
                    if on_top.y - under < A_DECK_APART {
                        continue;
                    }
                    measured += 1;
                    tightest = tightest.min(on_top.y - DECK - under);
                }
            }
        }
        assert!(
            measured > 100,
            "{}: only {measured} places where one road is over another, so it is \
             not crossing itself",
            circuit.name
        );
        assert!(
            tightest > wanted - 0.01,
            "{}: the tightest place under the bridge has {tightest:.2} m of air, \
             against the {wanted:.2} m it was built with",
            circuit.name
        );
    }

    /// A bridge is road, so it obeys the same rules the rest of the road does:
    /// the grade cap, and one lap's worth of everything.
    ///
    /// The cap is the one worth saying out loud, because the bridge is built
    /// *after* the cap has run — it has to be, or the cap takes it straight
    /// back off — so nothing downstream is checking it and the ramps have to be
    /// right by construction. A raised cosine's steepest point is half of π
    /// times its average, so a rise of `r` over a ramp of [`RAMP_PER_RISE`]
    /// times `r` peaks at 15.7% however big the bridge is, on top of whatever
    /// the ground was doing — which the lift levels to a straight line first,
    /// so that "whatever the ground was doing" is a number and not a worry.
    #[test]
    fn a_bridge_is_a_piece_of_road_like_any_other() {
        for circuit in bridged().chain([figure_of_eight(1.4, 0.0, 1.6)]) {
            let track = Track::new(circuit);
            let stations = track.ribbon.stations();
            let n = stations.len();
            let step = track.ribbon.length() / n as f32;
            let mut steepest = 0.0f32;
            for i in 0..n {
                let (a, b) = (stations[i].pos, stations[(i + 1) % n].pos);
                let run = (b - a).reject_from(Vec3::Y).length().max(1e-4);
                steepest = steepest.max((b.y - a.y).abs() / run);
            }
            assert!(
                steepest <= ribbon::MAX_GRADE,
                "{}: the bridge ramp reaches {steepest:.3}, past the cap of {}",
                circuit.name,
                ribbon::MAX_GRADE
            );
            // There is a deck, it is a deck's worth thick at the middle, and
            // it is drawn for exactly as long as its span and its ramps say.
            // Both come out of `Crossing`, so a bridge is described rather than
            // drawn by hand.
            let thickest = stations.iter().map(|s| s.deck).fold(0.0f32, f32::max);
            assert!(
                (thickest - 1.0).abs() < 1e-3,
                "{}: the deck is {thickest:.2} of a deck thick at its middle",
                circuit.name
            );
            let drawn = stations.iter().filter(|s| s.deck > 0.0).count() as f32 * step;
            let mut span = 0.0f32;
            for crossing in circuit.crossings {
                let rise = crossing.clearance + DECK + profile::SECTION_DEEP;
                span += 2.0 * (DECK_SPAN + RAMP_PER_RISE * rise);
            }
            assert!(
                (drawn - span).abs() < 2.0 * step,
                "{}: there is {drawn:.0} m of deck where its span and ramps come \
                 to {span:.0} m",
                circuit.name
            );

            // And the two roads that meet are as far apart as the clearance
            // needs, however much of that the elevation model had already given
            // and however much had to be added. Suzuka's model gives all of it
            // and more; the figure of eight is flat and is given none.
            for (crossing, at) in circuit.crossings.iter().zip(crossings_of(circuit)) {
                let rise = crossing.clearance + DECK + profile::SECTION_DEEP;
                let mut heights: Vec<f32> = stations
                    .iter()
                    .filter(|s| {
                        (s.pos - at).reject_from(Vec3::Y).length() < 2.0 * profile::HALF_WIDTH
                    })
                    .map(|s| s.pos.y)
                    .collect();
                heights.sort_by(f32::total_cmp);
                let apart = heights[heights.len() - 1] - heights[0];
                assert!(
                    apart > rise - 0.01,
                    "{}: the two roads that meet are {apart:.2} m apart where the \
                     clearance needs {rise:.2} m",
                    circuit.name
                );
            }
        }
    }

    /// What the car stands on over a bridge is the deck it is on, and the two
    /// things that decide that both work.
    ///
    /// Walked over every station of both decks, on the centreline and out at
    /// both wheels, which is where a lookup by height alone is least reliable —
    /// the outer wheel on the deck is lower than the middle of it, and on a
    /// ramp it is lower still.
    ///
    /// Three questions, and the third is the one worth having. Told where it
    /// was, the lookup keeps the car on the road it was on. Told nothing, it
    /// picks by height and gets the same answer, which is what a ghost dropped
    /// onto a saved pose relies on. Told that it was on the *other* deck, it
    /// says the other deck — because if it did not, continuity would not be
    /// deciding anything and the first answer would be luck.
    #[test]
    fn what_the_car_stands_on_over_a_bridge_is_the_deck_it_is_on() {
        let track = Track::new(figure_of_eight(0.0, 0.75, 1.6));
        let stations = track.ribbon.stations();
        let lap = track.ribbon.length();
        let wheel = crate::car::HALF_TRACK;
        let round = |a: f32, b: f32| {
            let d = (a - b).abs();
            d.min(lap - d)
        };
        let mut crossed = 0;
        for (i, station) in stations.iter().enumerate() {
            for across in [-wheel, 0.0, wheel] {
                let lateral = across;
                let on = station.pos
                    + station.right * lateral
                    + Vec3::Y * track.profile.height(i, 0.0, lateral);
                // Told where it was.
                let kept = track.fix(on, Some(station.s));
                assert!(
                    round(kept.s, station.s) < 1.0,
                    "at s={} and {across:.1} m across, the car reads as being at \
                     s={} — it has changed decks standing still",
                    station.s,
                    kept.s
                );
                // Told nothing.
                let guessed = track.fix(on, None);
                assert!(
                    round(guessed.s, station.s) < 1.0,
                    "at s={} and {across:.1} m across, a lookup with no history \
                     reads s={}",
                    station.s,
                    guessed.s
                );
                // Told it was on the other deck, where there is one.
                let others: Vec<f32> = track
                    .ribbon
                    .nearby(on, UNDER_THE_CAR)
                    .iter()
                    .map(|fix| fix.s)
                    .filter(|s| round(*s, station.s) > A_STEP_ALONG)
                    .collect();
                for other in others {
                    crossed += 1;
                    let swapped = track.fix(on, Some(other));
                    assert!(
                        round(swapped.s, other) < 1.0,
                        "told it was at s={other}, the lookup put the car at \
                         s={} instead",
                        swapped.s
                    );
                }
            }
        }
        assert!(
            crossed > 20,
            "only {crossed} places with two roads to choose between: the fixture \
             is not crossing itself"
        );
    }

    /// Driving under the start line is not a lap.
    ///
    /// The start plane goes on for ever and the start line does not. With the
    /// crossing a few metres past the line, the lower road passes under the
    /// plane, across it, and within a road's width of the middle of it — every
    /// test the gate used to make — and it is not the line, because it is nine
    /// metres below it and half a lap away.
    #[test]
    fn driving_under_the_start_line_is_not_a_lap() {
        // The crossing put within half a metre of the start line, which is the
        // only place this question can be asked from.
        let track = Track::new(figure_of_eight(1.5, 0.0, 1.6));
        let stations = track.ribbon.stations();
        let lap = track.ribbon.length();
        let start = stations[0];
        let round = |a: f32| {
            let d = (a - start.s).abs();
            d.min(lap - d)
        };
        // The stretch that goes under, where it passes beneath the line.
        let below: Vec<&ribbon::Station> = stations
            .iter()
            .filter(|s| {
                round(s.s) > A_STEP_ALONG && (s.pos - start.pos).reject_from(Vec3::Y).length() < 2.0
            })
            .collect();
        assert!(
            !below.is_empty(),
            "nothing passes under the line, so there is nothing to refuse"
        );
        for station in &below {
            assert!(
                start.pos.y - station.pos.y > A_DECK_APART,
                "the road under the line is not under it"
            );
            // It crosses the plane, it is beside the middle of the line, and it
            // is still not the line.
            let across = (station.pos - start.pos)
                .reject_from(Vec3::Y)
                .dot(start.right);
            assert!(across.abs() < HALF_WIDTH + 2.0 * crate::car::HALF_TRACK);
            assert!(
                !track.on_start_gate(station.pos, Some(station.s)),
                "a lap was completed on the road under the line"
            );
            assert!(
                !track.on_start_gate(station.pos, None),
                "a lap was completed on the road under the line, with no history"
            );
        }
        // And the line itself still opens, which is the other half of it.
        assert!(track.on_start_gate(start.pos, Some(start.s)));
        assert!(track.on_start_gate(start.pos, None));
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
        let taller = Ribbon::new(&taller, circuit.corners, &[]);
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

    /// Scratch: the finished lap of each circuit, by module.
    #[test]
    #[ignore]
    fn the_laps() {
        for circuit in circuits::all() {
            println!("{}|{:.1}", circuit.id, Track::new(circuit).ribbon.length());
        }
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
            let window = straight_window(&track, n);
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

    /// How much a circuit's elevation model says the road climbs.
    ///
    /// Not the raw range, which is the proxy this replaces and which one bad
    /// sample can make up entirely. The model is a 90 m ground elevation read
    /// at the trace's own fixes, on circuits that run between grandstands,
    /// under bridges and through car parks, so a single fix tens of metres
    /// above both of its neighbours is a building. Miami's raw range is 12 m
    /// and 10 of them are one fix; Las Vegas's is 30 m and 11 of them are;
    /// Hockenheim has a lone 16 m sample between a 6 and a 5.
    ///
    /// So a height is taken with its two neighbours and the middle one kept — a
    /// hill two fixes wide is a hill, and one fix on its own is not. Nothing
    /// downstream reads this: it is what the *test* expects the circuit to
    /// climb, and correcting it is what stopped the smoothing being blamed for
    /// refusing to build a grandstand.
    fn relief_of(circuit: &Circuit) -> f32 {
        let heights: Vec<f32> = circuit.centreline.iter().map(|p| p[1]).collect();
        let n = heights.len();
        let (low, high) = (0..n)
            .map(|i| {
                let mut three = [heights[(i + n - 1) % n], heights[i], heights[(i + 1) % n]];
                three.sort_by(f32::total_cmp);
                three[1]
            })
            .fold((f32::MAX, f32::MIN), |(l, h), y| (l.min(y), h.max(y)));
        high - low
    }

    /// Stations in the straight line a circuit is measured against.
    ///
    /// 40 m of *source* circuit, which is 40 m of Todora on the thirty-five
    /// circuits built at the shared scale and more on the four that are not.
    /// The question being asked is about the layout — is there a corner here,
    /// or a bend the road can be driven straight through — and the layout
    /// belongs to the real circuit. Ask it with a fixed 40 m and a circuit
    /// built at two and a half times the plan is asked whether a line fits down
    /// a fortieth of itself rather than a fifteenth, which every circuit would
    /// pass and which says nothing about any of them.
    fn straight_window(track: &Track, n: usize) -> usize {
        let step = track.ribbon.length() / n as f32;
        (40.0 * track.circuit().plan_scale / step) as usize
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
            let window = straight_window(&track, n);
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
