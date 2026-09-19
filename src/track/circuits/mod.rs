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
mod americas;
mod bahrain;
mod barcelona_catalunya;
mod gilles_villeneuve;
mod hermanos_rodriguez;
mod imola;
mod indianapolis;
mod istanbul_park;
mod jacarepagua;
mod las_vegas;
mod losail;
mod madring;
mod marina_bay;
mod monza;
mod paul_ricard;
mod red_bull_ring;
mod silverstone;
mod spa_francorchamps;
mod watkins_glen;

pub(crate) struct Circuit {
    /// Stable and file-safe: it names the lap saved to disk.
    pub(crate) id: &'static str,
    /// What the driver is told they are driving.
    pub(crate) name: &'static str,
    /// How hard this circuit's corners are pushed away from its own mean line.
    /// 1 is the trace as surveyed. See [`super::ribbon::exaggerate_corners`] for
    /// why that is not enough, and why the figure belongs to the circuit rather
    /// than to the game: what a circuit can take before it grows into itself is
    /// a property of its layout. Spielberg is wound tightly around a hillside
    /// and has almost no room; Monza is three long straights and has plenty.
    pub(crate) corners: f32,
    /// Metres from the circuit centroid, at full size. Y is height above the
    /// lowest point on the lap. The last sample joins back to the first.
    pub(crate) centreline: &'static [[f32; 3]],
}

/// Every circuit, in the order the menu lists them, which is alphabetical:
/// twenty of them is far too many to remember a running order for, and a
/// list you can find a name in is worth more than one that tells you which
/// was added when.
const ALL: &[Circuit] = &[
    albert_park::CIRCUIT,
    americas::CIRCUIT,
    bahrain::CIRCUIT,
    barcelona_catalunya::CIRCUIT,
    gilles_villeneuve::CIRCUIT,
    hermanos_rodriguez::CIRCUIT,
    imola::CIRCUIT,
    indianapolis::CIRCUIT,
    istanbul_park::CIRCUIT,
    jacarepagua::CIRCUIT,
    las_vegas::CIRCUIT,
    losail::CIRCUIT,
    madring::CIRCUIT,
    marina_bay::CIRCUIT,
    monza::CIRCUIT,
    paul_ricard::CIRCUIT,
    red_bull_ring::CIRCUIT,
    silverstone::CIRCUIT,
    spa_francorchamps::CIRCUIT,
    watkins_glen::CIRCUIT,
];

/// The circuit the game opens on. Named rather than taken off the top of the
/// list, because the list is alphabetical and the Red Bull Ring is not — it is
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
