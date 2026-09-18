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

mod monza;
mod red_bull_ring;
mod spa_francorchamps;

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

/// In the order the track key walks them.
const ALL: &[Circuit] = &[
    red_bull_ring::CIRCUIT,
    spa_francorchamps::CIRCUIT,
    monza::CIRCUIT,
];

/// The circuit the game opens on.
pub(crate) fn first() -> &'static Circuit {
    &ALL[0]
}

/// The one after `circuit`, wrapping round at the end of the list.
pub(crate) fn after(circuit: &Circuit) -> &'static Circuit {
    let at = ALL
        .iter()
        .position(|c| c.id == circuit.id)
        .expect("every circuit in play came from this list");
    &ALL[(at + 1) % ALL.len()]
}

#[cfg(test)]
pub(crate) fn all() -> &'static [Circuit] {
    ALL
}
