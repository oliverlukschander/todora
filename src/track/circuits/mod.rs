//! Every circuit in the game, one module each.
//!
//! A circuit is a name and a trace of the real thing, in real metres. Nothing
//! here is scaled or smoothed: [`super`] applies the same plan and height scales
//! to every circuit, so they all shrink alike and lap times mean the same thing
//! from one to the next. What a circuit can carry either side of the road it
//! settles for itself — see [`super::profile`].
//!
//! Adding one is a file and a line: run `tools/make_track.py` against an id from
//! bacinger/f1-circuits and it writes the module and registers it here.

mod albert_park;
mod algarve;
mod americas;
mod bahrain;
mod baku;
mod barcelona_catalunya;
mod buenos_aires;
mod estoril;
mod gilles_villeneuve;
mod hermanos_rodriguez;
mod hockenheim;
mod hungaroring;
mod imola;
mod indianapolis;
mod interlagos;
mod istanbul_park;
mod jacarepagua;
mod jeddah;
mod kyalami;
mod las_vegas;
mod losail;
mod madring;
mod magny_cours;
mod marina_bay;
mod miami;
mod monaco;
mod monza;
mod mugello;
mod nurburgring;
mod paul_ricard;
mod red_bull_ring;
mod sepang;
mod shanghai;
mod silverstone;
mod sochi;
mod spa_francorchamps;
mod suzuka;
mod watkins_glen;
mod yas_marina;
mod zandvoort;

pub(crate) struct Circuit {
    /// Stable and file-safe: it names the lap saved to disk.
    pub(crate) id: &'static str,
    /// What the driver is told they are driving.
    pub(crate) name: &'static str,
    /// How hard this circuit's corners are pushed away from its own mean line.
    /// 1 is the trace as surveyed, and 1 is what every circuit now is.
    ///
    /// It was not. Monza was pushed three and a half times and Spa twice,
    /// because the road was 8 m wide on a plan shrunk seven and a half times —
    /// five times too wide for the land it was laid on — and the whole of
    /// Monza's Rettifilo displaced the car by less than half a road width, so
    /// the quick way through a chicane was not to steer. The road is 3.3 m now
    /// and both of them are corners again as surveyed.
    ///
    /// The lever stays, at 1, because the property it was there for has not
    /// gone away: `corners_are_corners` is what says a circuit has come out
    /// mostly straight, and a circuit that fails it can be pushed away from its
    /// own mean line until it does not. Nothing in the game needs that today,
    /// and a circuit that did would be saying something about its own layout —
    /// which is why this belongs to the circuit and not to the game.
    pub(crate) corners: f32,
    /// What one lap of it comes out as, in metres, once it has been shrunk and
    /// had its corners opened.
    ///
    /// Written down rather than measured, because the circuit menu shows it and
    /// building all thirty-nine circuits to fill a menu is four tenths of a
    /// second the menu does not have. It is the one number here that is not a
    /// fact about the real circuit, so it is the one that can go stale:
    /// `the_menu_shows_the_lap_it_will_drive` builds every circuit and holds
    /// this to what came out, which is also where the figure to write comes
    /// from when a new circuit is added or the pipeline moves.
    ///
    /// The menu shows it because [`Circuit::plan_scale`] took away the thing
    /// that used to make it guessable: laps were all shrunk alike, so a longer
    /// circuit meant a longer lap in the same proportion. Baku's does not.
    pub(crate) lap: f32,
    /// What this circuit's plan is multiplied by, on top of the scale every
    /// circuit gets. 1 is the shared scale, and 1 is what almost every circuit
    /// has.
    ///
    /// This is the one place Todora treats a circuit differently from the
    /// others, and it is worth being clear about what it costs. Every circuit
    /// is otherwise shrunk alike, which is what made a lap time mean the same
    /// thing from one to the next and made the relative lengths of the real
    /// circuits survive into the game: Spa is half again as long as Spielberg
    /// in life and half again as long here. A circuit with a multiplier breaks
    /// that. Baku at 2.4 comes out longer than Spa, which it is not.
    ///
    /// The alternative was a road that changes width from circuit to circuit,
    /// and that is worse: the difficult circuit would get a different amount of
    /// steering room as well as being difficult, and Baku would need a road
    /// narrower than two cars before there was any verge at all. The other
    /// alternative was raising the shared scale for all forty, which makes
    /// every existing race substantially longer to pay for four.
    ///
    /// So: seconds, metres, the car and the road stay common, and the ratio of
    /// one lap's length to another's does not. The circuit menu shows the
    /// finished lap length for that reason. Records are never normalised by
    /// this — cornering and acceleration do not scale linearly, so a lap of
    /// Baku divided by 2.4 is not a lap of anything.
    ///
    /// Each one is the smallest multiplier that clears the fitted mesh and the
    /// driving tests, found by trying them, not a number picked to reach a lap
    /// length. `every_exception_is_needed` is what stops one outliving its
    /// reason.
    pub(crate) plan_scale: f32,
    /// Metres from the circuit centroid, at full size. Y is height above the
    /// lowest point on the lap. The last sample joins back to the first.
    pub(crate) centreline: &'static [[f32; 3]],
    /// Where this circuit passes over itself, if it does. Almost none do.
    pub(crate) crossings: &'static [Crossing],
}

/// A place where a circuit passes over itself.
///
/// Anchored to the trace rather than to the finished circuit: `at` is in the
/// same metres-from-the-centroid frame as [`Circuit::centreline`], and `over`
/// is how far round that trace the stretch on top is. Both survive everything
/// downstream of them. A finished station number would not — it moves when the
/// spline moves, when the corner target moves, when the plan scale moves.
///
/// The height is the part that is *not* surveyed, and saying so is the point of
/// this comment. The elevation model behind [`Circuit::centreline`] is a 90 m
/// ground model read at the trace's own fixes: it reads both stretches of a
/// crossing at the height of the ground between them, because ground is what it
/// measures. It has nothing to say about a road deck. So `clearance` is
/// authored for the game — enough air under the span for the car to go through
/// it and see that it did — and what comes from the source is the *topology*:
/// that there is a crossing here, where it is, and which of the two stretches
/// is the one on top.
#[derive(Clone, Copy)]
pub(crate) struct Crossing {
    /// Where the two stretches cross, in the trace's own metres: east and
    /// south of the centroid, the same frame the centreline is in.
    pub(crate) at: [f32; 2],
    /// How far round the trace the stretch that goes over is, as a fraction of
    /// its length.
    pub(crate) over: f32,
    /// Clear air between the road below and the underside of the span above,
    /// in finished metres. Authored. Not from the elevation model.
    pub(crate) clearance: f32,
    /// Where the fact of the crossing came from, and where the clearance came
    /// from. One line, so that a reader can tell the two apart.
    ///
    /// Nothing in the game reads it, which is the point: it is here for the
    /// person who finds a number in a data file and wants to know whether
    /// anybody measured it. `a_bridge_says_where_it_came_from` is what keeps it
    /// from quietly going missing.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) provenance: &'static str,
}

/// Every circuit, in the order the menu lists them, which is alphabetical:
/// twenty of them is far too many to remember a running order for, and a
/// list you can find a name in is worth more than one that tells you which
/// was added when.
const ALL: &[Circuit] = &[
    silverstone::CIRCUIT,
    spa_francorchamps::CIRCUIT,
    hockenheim::CIRCUIT,
    interlagos::CIRCUIT,
    miami::CIRCUIT,
    sochi::CIRCUIT,
    istanbul_park::CIRCUIT,
    magny_cours::CIRCUIT,
    baku::CIRCUIT,
    barcelona_catalunya::CIRCUIT,
    hungaroring::CIRCUIT,
    bahrain::CIRCUIT,
    nurburgring::CIRCUIT,
    suzuka::CIRCUIT,
    watkins_glen::CIRCUIT,
    monaco::CIRCUIT,
    hermanos_rodriguez::CIRCUIT,
    kyalami::CIRCUIT,
    indianapolis::CIRCUIT,
    yas_marina::CIRCUIT,
    marina_bay::CIRCUIT,
    estoril::CIRCUIT,
    americas::CIRCUIT,
    madring::CIRCUIT,
    sepang::CIRCUIT,
    las_vegas::CIRCUIT,
    zandvoort::CIRCUIT,
    buenos_aires::CIRCUIT,
    monza::CIRCUIT,
    losail::CIRCUIT,
    paul_ricard::CIRCUIT,
    jeddah::CIRCUIT,
    jacarepagua::CIRCUIT,
    algarve::CIRCUIT,
    imola::CIRCUIT,
    albert_park::CIRCUIT,
    gilles_villeneuve::CIRCUIT,
    red_bull_ring::CIRCUIT,
    mugello::CIRCUIT,
    shanghai::CIRCUIT,
];

/// The circuit the game opens on. Named rather than taken off the top of the
/// list, because the list is alphabetical and the Styrian Bowl is not — it is
/// the one Todora was built around, and one of the shortest laps in it.
pub(crate) fn first() -> &'static Circuit {
    &red_bull_ring::CIRCUIT
}

/// Where `circuit` sits in the list, which is where the menu opens its cursor.
pub(crate) fn at(circuit: &Circuit) -> usize {
    ALL.iter()
        .position(|c| c.id == circuit.id)
        .expect("every circuit in play came from this list")
}

pub(crate) fn all() -> &'static [Circuit] {
    ALL
}
