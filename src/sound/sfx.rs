//! Every silly noise in the game, made of nothing.
//!
//! No sample files: a honk is a sawtooth with a pitch envelope through a
//! resonant filter, a boing is a sine that wobbles as it falls, a cluck is two
//! short buzzes shaped to sound like a mouth. Each sound is a *recipe* — a short
//! list of [`Note`]s, each with a waveform, a pitch glide, a vibrato, a filter
//! that sweeps and an envelope — and a [`Bank`] of voices plays them.
//!
//! The game asks by writing an [`Sfx`] message; that goes into a lock-free ring
//! ([`Queue`]) and the audio thread, which owns the bank, picks it up on the next
//! sample. Nothing is allocated, locked or decoded on the audio thread: a voice
//! is a few floats and a copy of the note it is playing.
//!
//! The recipes are data, and data can be looked at. `write_the_sfx` renders
//! every one to `dist/sfx/<name>.wav`, and a test checks each is finite, audible,
//! bounded and over.

use std::sync::atomic::{
    AtomicU32, AtomicU64,
    Ordering::{Acquire, Relaxed, Release},
};

use bevy::prelude::Message;

/// Samples a second, as the mixer runs.
pub(super) const RATE: f32 = 44_100.0;
/// Voices that can sound at once. A chord and a rattle of pins fit; a stampede
/// steals whichever is quietest.
const VOICES: usize = 48;
/// The most of one sound that may start within [`WINDOW`] samples of each other:
/// a held horn or a string of pops is a burst, and not a wall of the same noise
/// starting at the same instant and adding up.
const ALIKE: usize = 3;
const WINDOW: u32 = (RATE * 0.05) as u32;
/// Where the bank's own soft ceiling sits. Below full scale on purpose: the
/// engine, the tyres and the music are added to it after, and the mixer's own
/// ceiling is what they share the rest of the room under.
const CEILING: f32 = 0.7;
/// Events the ring holds before the oldest is overwritten.
const SLOTS: usize = 128;
const TAU: f32 = std::f32::consts::TAU;

/// What a note is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Wave {
    Sine,
    Tri,
    Saw,
    Square,
    Noise,
}

/// What a note is filtered through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Filter {
    None,
    Low,
    Band,
    High,
}

/// One sound in a recipe.
#[derive(Clone, Copy, Debug)]
pub(super) struct Note {
    /// Seconds after the recipe starts, and how long it lasts.
    at: f32,
    dur: f32,
    wave: Wave,
    /// Pitch at the start and the end, in hertz, glided between exponentially.
    f0: f32,
    f1: f32,
    /// Above 1 the glide is quick at first and slow after; below 1, the other way.
    bend: f32,
    /// Vibrato rate in hertz, and its depth in semitones, fading over the note.
    vib_hz: f32,
    vib_st: f32,
    amp: f32,
    /// Seconds to fade in, and the rate of an exponential fade out.
    attack: f32,
    decay: f32,
    /// How much white noise is mixed into the tone.
    noise: f32,
    /// A square's pulse width.
    duty: f32,
    filter: Filter,
    /// Cutoff or centre in hertz at the start, the middle and the end.
    cutoff: [f32; 3],
    q: f32,
    /// A sine can be frequency-modulated: the modulator's ratio to the carrier
    /// and how far it pushes, which is how a bell or a laser is made.
    fm_ratio: f32,
    fm_depth: f32,
}

impl Note {
    const fn new(wave: Wave, at: f32, dur: f32, f0: f32, f1: f32) -> Self {
        Self {
            at,
            dur,
            wave,
            f0,
            f1,
            bend: 1.0,
            vib_hz: 0.0,
            vib_st: 0.0,
            amp: 0.25,
            attack: 0.004,
            decay: 0.0,
            noise: 0.0,
            duty: 0.5,
            filter: Filter::None,
            cutoff: [0.0; 3],
            q: 1.0,
            fm_ratio: 0.0,
            fm_depth: 0.0,
        }
    }
    const fn amp(mut self, v: f32) -> Self {
        self.amp = v;
        self
    }
    const fn attack(mut self, v: f32) -> Self {
        self.attack = v;
        self
    }
    const fn decay(mut self, v: f32) -> Self {
        self.decay = v;
        self
    }
    const fn bend(mut self, v: f32) -> Self {
        self.bend = v;
        self
    }
    const fn vib(mut self, hz: f32, semitones: f32) -> Self {
        self.vib_hz = hz;
        self.vib_st = semitones;
        self
    }
    const fn noise(mut self, v: f32) -> Self {
        self.noise = v;
        self
    }
    const fn duty(mut self, v: f32) -> Self {
        self.duty = v;
        self
    }
    const fn low(mut self, from: f32, to: f32) -> Self {
        self.filter = Filter::Low;
        self.cutoff = [from, (from + to) * 0.5, to];
        self
    }
    const fn low3(mut self, from: f32, mid: f32, to: f32) -> Self {
        self.filter = Filter::Low;
        self.cutoff = [from, mid, to];
        self
    }
    const fn band(mut self, from: f32, to: f32, q: f32) -> Self {
        self.filter = Filter::Band;
        self.cutoff = [from, (from + to) * 0.5, to];
        self.q = q;
        self
    }
    const fn band3(mut self, from: f32, mid: f32, to: f32, q: f32) -> Self {
        self.filter = Filter::Band;
        self.cutoff = [from, mid, to];
        self.q = q;
        self
    }
    const fn high(mut self, from: f32, to: f32) -> Self {
        self.filter = Filter::High;
        self.cutoff = [from, (from + to) * 0.5, to];
        self
    }
    const fn fm(mut self, ratio: f32, depth: f32) -> Self {
        self.fm_ratio = ratio;
        self.fm_depth = depth;
        self
    }
}

use Wave::{Noise, Saw, Sine, Square, Tri};

/// Declare every sound once: its name, and the notes it is made of.
macro_rules! recipes {
    ($($kind:ident => [$($note:expr),* $(,)?]),* $(,)?) => {
        /// A sound the game can ask for.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[repr(u8)]
        pub(crate) enum Kind { $($kind),* }

        impl Kind {
            pub(crate) const ALL: &'static [Kind] = &[$(Kind::$kind),*];

            pub(super) fn notes(self) -> &'static [Note] {
                match self {
                    $(Kind::$kind => {
                        static NOTES: &[Note] = &[$($note),*];
                        NOTES
                    }),*
                }
            }

            #[cfg(test)]
            pub(crate) fn name(self) -> &'static str {
                match self { $(Kind::$kind => stringify!($kind)),* }
            }
        }
    };
}

recipes! {
    // --- Horns. ---
    // Two tones at once, twice, the way a small car does it.
    HornBeep => [
        Note::new(Square, 0.00, 0.16, 420.0, 415.0).amp(0.10).low(2600.0, 2200.0),
        Note::new(Square, 0.00, 0.16, 525.0, 520.0).amp(0.10).low(2600.0, 2200.0),
        Note::new(Square, 0.21, 0.20, 420.0, 415.0).amp(0.10).low(2600.0, 2200.0),
        Note::new(Square, 0.21, 0.20, 525.0, 520.0).amp(0.10).low(2600.0, 2200.0),
    ],
    // A bulb horn: a reed that scoops up and falls away.
    HornClown => [
        Note::new(Saw, 0.00, 0.11, 250.0, 390.0).bend(2.0).amp(0.30).attack(0.01).band3(700.0, 1300.0, 1500.0, 1.6).noise(0.05),
        Note::new(Saw, 0.10, 0.30, 390.0, 285.0).amp(0.30).attack(0.01).decay(2.5).band3(1500.0, 1300.0, 800.0, 1.6).vib(7.0, 0.18).noise(0.05),
    ],
    // Three notes held together, loud, sagging a little.
    HornAir => [
        Note::new(Saw, 0.00, 0.85, 392.0 * 1.04, 392.0).bend(6.0).amp(0.11).attack(0.008).decay(1.6).low(3600.0, 1800.0),
        Note::new(Saw, 0.00, 0.85, 494.0 * 1.04, 494.0).bend(6.0).amp(0.11).attack(0.008).decay(1.6).low(3600.0, 1800.0),
        Note::new(Saw, 0.00, 0.85, 587.0 * 1.04, 587.0).bend(6.0).amp(0.11).attack(0.008).decay(1.6).low(3600.0, 1800.0),
        Note::new(Noise, 0.00, 0.20, 0.0, 0.0).amp(0.07).decay(9.0).band(1800.0, 900.0, 0.8),
    ],
    // AH-OO-GAH.
    HornOoga => [
        Note::new(Saw, 0.00, 0.20, 190.0, 315.0).amp(0.30).attack(0.02).low(700.0, 1100.0),
        Note::new(Saw, 0.20, 0.36, 315.0, 165.0).amp(0.30).attack(0.01).decay(2.0).low(1100.0, 600.0).vib(6.0, 0.25),
    ],
    // Meep meep.
    HornMeep => [
        Note::new(Tri, 0.00, 0.075, 1150.0, 1750.0).bend(1.5).amp(0.22).attack(0.006),
        Note::new(Sine, 0.00, 0.075, 2300.0, 3500.0).bend(1.5).amp(0.06).attack(0.006),
        Note::new(Tri, 0.115, 0.085, 1150.0, 1850.0).bend(1.5).amp(0.22).attack(0.006),
        Note::new(Sine, 0.115, 0.085, 2300.0, 3700.0).bend(1.5).amp(0.06).attack(0.006),
    ],
    // A cow, roughly: a low buzz through two vowel formants that open and close.
    HornMoo => [
        Note::new(Saw, 0.00, 1.05, 132.0, 100.0).amp(0.55).attack(0.16).decay(1.2).band3(300.0, 640.0, 330.0, 2.6).vib(5.2, 0.32).noise(0.03),
        Note::new(Saw, 0.00, 1.05, 132.0, 100.0).amp(0.30).attack(0.16).decay(1.2).band3(900.0, 1150.0, 800.0, 3.0).vib(5.2, 0.32),
    ],
    // Quack quack.
    HornQuack => [
        Note::new(Saw, 0.00, 0.13, 360.0, 250.0).amp(0.42).attack(0.006).decay(5.0).band(1250.0, 900.0, 4.0).noise(0.14),
        Note::new(Saw, 0.19, 0.11, 330.0, 230.0).amp(0.36).attack(0.006).decay(6.0).band(1200.0, 850.0, 4.0).noise(0.14),
    ],
    // wah wah wah waaaah.
    HornTrombone => [
        Note::new(Saw, 0.00, 0.30, 233.0, 229.0).amp(0.25).attack(0.03).low3(350.0, 1500.0, 500.0),
        Note::new(Saw, 0.34, 0.30, 220.0, 216.0).amp(0.25).attack(0.03).low3(350.0, 1400.0, 480.0),
        Note::new(Saw, 0.68, 0.30, 208.0, 204.0).amp(0.25).attack(0.03).low3(350.0, 1300.0, 460.0),
        Note::new(Saw, 1.02, 1.10, 196.0, 176.0).amp(0.27).attack(0.04).decay(1.0).low3(350.0, 1200.0, 300.0).vib(5.4, 0.55),
    ],
    // --- Chickens. ---
    // bok-k.
    Cluck => [
        Note::new(Saw, 0.000, 0.075, 520.0, 330.0).amp(0.34).attack(0.004).decay(26.0).band(1500.0, 1000.0, 2.2).noise(0.08),
        Note::new(Saw, 0.085, 0.055, 430.0, 290.0).amp(0.22).attack(0.003).decay(34.0).band(1300.0, 900.0, 2.2).noise(0.08),
    ],
    // BAWK.
    Bawk => [
        Note::new(Saw, 0.00, 0.10, 340.0, 720.0).bend(2.0).amp(0.34).attack(0.006).decay(5.0).band(1100.0, 1900.0, 2.0).noise(0.18).vib(28.0, 0.7),
        Note::new(Saw, 0.09, 0.26, 720.0, 380.0).amp(0.32).decay(6.0).band(1900.0, 900.0, 2.0).noise(0.20).vib(24.0, 0.9),
    ],
    // Three panicked ones in a row, each higher.
    Squawk => [
        Note::new(Saw, 0.00, 0.13, 460.0, 800.0).bend(2.0).amp(0.30).decay(5.0).band(1200.0, 2000.0, 2.0).noise(0.22).vib(30.0, 0.8),
        Note::new(Saw, 0.15, 0.13, 520.0, 900.0).bend(2.0).amp(0.30).decay(5.0).band(1300.0, 2200.0, 2.0).noise(0.22).vib(30.0, 0.8),
        Note::new(Saw, 0.30, 0.20, 600.0, 1000.0).bend(2.0).amp(0.30).decay(4.0).band(1400.0, 2300.0, 2.0).noise(0.22).vib(30.0, 0.8),
    ],
    // Cock-a-doodle-doo.
    Crow => [
        Note::new(Saw, 0.00, 0.13, 480.0, 520.0).amp(0.30).attack(0.01).band(1200.0, 1500.0, 2.0).noise(0.10),
        Note::new(Saw, 0.15, 0.10, 640.0, 660.0).amp(0.30).attack(0.01).band(1500.0, 1700.0, 2.0).noise(0.10),
        Note::new(Saw, 0.28, 0.36, 700.0, 860.0).amp(0.34).attack(0.02).band(1600.0, 2100.0, 2.0).noise(0.12).vib(9.0, 0.3),
        Note::new(Saw, 0.66, 0.13, 780.0, 690.0).amp(0.30).attack(0.01).band(1700.0, 1500.0, 2.0).noise(0.10),
        Note::new(Saw, 0.82, 0.85, 900.0, 560.0).bend(0.8).amp(0.34).attack(0.02).decay(1.4).band3(1900.0, 2300.0, 1200.0, 2.0).noise(0.16).vib(12.0, 0.7),
    ],
    // A chick.
    Peep => [
        Note::new(Sine, 0.00, 0.06, 2500.0, 3300.0).amp(0.16).decay(14.0),
        Note::new(Sine, 0.09, 0.05, 2700.0, 3500.0).amp(0.13).decay(16.0),
    ],
    // --- Bounces and pops. ---
    Boing => [
        Note::new(Sine, 0.00, 0.55, 420.0, 140.0).bend(2.5).amp(0.34).attack(0.003).decay(4.5).vib(13.0, 5.0),
        Note::new(Sine, 0.00, 0.55, 840.0, 280.0).bend(2.5).amp(0.08).decay(6.0).vib(13.0, 5.0),
    ],
    Squeak => [
        Note::new(Sine, 0.00, 0.09, 1500.0, 2500.0).amp(0.17).attack(0.005).decay(6.0),
        Note::new(Sine, 0.10, 0.07, 2100.0, 2800.0).amp(0.13).attack(0.005).decay(9.0),
    ],
    Pop => [
        Note::new(Noise, 0.00, 0.05, 0.0, 0.0).amp(0.26).attack(0.001).decay(60.0).low(6000.0, 1500.0),
        Note::new(Sine, 0.00, 0.06, 900.0, 160.0).bend(3.0).amp(0.34).attack(0.001).decay(30.0),
    ],
    Bonk => [
        Note::new(Sine, 0.00, 0.14, 420.0, 110.0).bend(3.0).amp(0.42).attack(0.001).decay(22.0).fm(1.5, 1.2),
        Note::new(Noise, 0.00, 0.03, 0.0, 0.0).amp(0.16).attack(0.001).decay(90.0).low(3000.0, 800.0),
    ],
    Thud => [
        Note::new(Sine, 0.00, 0.20, 150.0, 48.0).bend(2.5).amp(0.55).attack(0.002).decay(14.0),
        Note::new(Noise, 0.00, 0.10, 0.0, 0.0).amp(0.18).attack(0.001).decay(40.0).low(700.0, 200.0),
    ],
    Boom => [
        Note::new(Sine, 0.00, 0.60, 120.0, 30.0).bend(2.0).amp(0.62).attack(0.003).decay(5.5),
        Note::new(Noise, 0.00, 0.45, 0.0, 0.0).amp(0.34).attack(0.002).decay(8.0).low(1800.0, 250.0),
    ],
    Splat => [
        Note::new(Noise, 0.00, 0.20, 0.0, 0.0).amp(0.40).attack(0.002).decay(14.0).band(900.0, 260.0, 1.2),
        Note::new(Sine, 0.00, 0.13, 240.0, 80.0).bend(2.0).amp(0.30).decay(20.0),
    ],
    Crunch => [
        Note::new(Noise, 0.00, 0.05, 0.0, 0.0).amp(0.34).attack(0.001).decay(45.0).band(2600.0, 1800.0, 0.8),
        Note::new(Noise, 0.045, 0.05, 0.0, 0.0).amp(0.30).attack(0.001).decay(45.0).band(2200.0, 1500.0, 0.8),
        Note::new(Noise, 0.10, 0.07, 0.0, 0.0).amp(0.26).attack(0.001).decay(40.0).band(1800.0, 1100.0, 0.8),
    ],
    // Ten pins going over.
    Clatter => [
        Note::new(Sine, 0.000, 0.07, 1180.0, 900.0).amp(0.22).decay(38.0),
        Note::new(Sine, 0.022, 0.07, 860.0, 690.0).amp(0.20).decay(36.0),
        Note::new(Sine, 0.051, 0.07, 1320.0, 1000.0).amp(0.20).decay(38.0),
        Note::new(Sine, 0.078, 0.07, 980.0, 760.0).amp(0.18).decay(34.0),
        Note::new(Sine, 0.112, 0.07, 1240.0, 950.0).amp(0.18).decay(36.0),
        Note::new(Sine, 0.140, 0.07, 780.0, 620.0).amp(0.16).decay(34.0),
        Note::new(Sine, 0.181, 0.07, 1100.0, 860.0).amp(0.16).decay(36.0),
        Note::new(Sine, 0.236, 0.07, 940.0, 730.0).amp(0.14).decay(34.0),
        Note::new(Sine, 0.298, 0.07, 1280.0, 980.0).amp(0.12).decay(36.0),
        Note::new(Sine, 0.371, 0.07, 820.0, 650.0).amp(0.10).decay(34.0),
        Note::new(Noise, 0.000, 0.45, 0.0, 0.0).amp(0.12).attack(0.01).decay(6.0).band(600.0, 300.0, 0.7),
    ],
    // Exhaust: pop, pop-pop.
    Backfire => [
        Note::new(Noise, 0.000, 0.07, 0.0, 0.0).amp(0.30).attack(0.001).decay(40.0).low(2200.0, 600.0),
        Note::new(Sine, 0.000, 0.06, 150.0, 60.0).amp(0.28).decay(30.0),
        Note::new(Noise, 0.075, 0.05, 0.0, 0.0).amp(0.22).attack(0.001).decay(50.0).low(2000.0, 500.0),
        Note::new(Noise, 0.150, 0.06, 0.0, 0.0).amp(0.26).attack(0.001).decay(45.0).low(2100.0, 550.0),
        Note::new(Sine, 0.150, 0.05, 130.0, 55.0).amp(0.22).decay(30.0),
    ],
    // --- Rewards. ---
    Coin => [
        Note::new(Square, 0.00, 0.07, 988.0, 988.0).amp(0.09).attack(0.002).duty(0.25),
        Note::new(Square, 0.07, 0.40, 1319.0, 1319.0).amp(0.09).attack(0.002).decay(5.5).duty(0.25),
    ],
    Ding => [
        Note::new(Sine, 0.00, 0.70, 1568.0, 1568.0).amp(0.22).attack(0.002).decay(7.0),
        Note::new(Sine, 0.00, 0.40, 4700.0, 4700.0).amp(0.07).attack(0.002).decay(14.0),
    ],
    // C E G C, brightly.
    Tada => [
        Note::new(Saw, 0.00, 0.16, 262.0, 262.0).amp(0.09).attack(0.01).low(2600.0, 2600.0),
        Note::new(Saw, 0.00, 0.16, 330.0, 330.0).amp(0.09).attack(0.01).low(2600.0, 2600.0),
        Note::new(Saw, 0.00, 0.16, 392.0, 392.0).amp(0.09).attack(0.01).low(2600.0, 2600.0),
        Note::new(Saw, 0.17, 0.95, 523.0, 523.0).amp(0.10).attack(0.01).decay(2.2).low(3200.0, 1800.0).vib(5.5, 0.12),
        Note::new(Saw, 0.17, 0.95, 659.0, 659.0).amp(0.10).attack(0.01).decay(2.2).low(3200.0, 1800.0).vib(5.5, 0.12),
        Note::new(Saw, 0.17, 0.95, 784.0, 784.0).amp(0.10).attack(0.01).decay(2.2).low(3200.0, 1800.0).vib(5.5, 0.12),
        Note::new(Saw, 0.17, 0.95, 1046.0, 1046.0).amp(0.07).attack(0.01).decay(2.6).low(3200.0, 1800.0),
    ],
    Sparkle => [
        Note::new(Sine, 0.00, 0.14, 2093.0, 2093.0).amp(0.09).decay(13.0),
        Note::new(Sine, 0.05, 0.14, 2637.0, 2637.0).amp(0.09).decay(13.0),
        Note::new(Sine, 0.10, 0.14, 3136.0, 3136.0).amp(0.09).decay(13.0),
        Note::new(Sine, 0.15, 0.14, 3520.0, 3520.0).amp(0.08).decay(13.0),
        Note::new(Sine, 0.20, 0.25, 4186.0, 4186.0).amp(0.08).decay(11.0),
    ],
    // Combo going up.
    Chime => [
        Note::new(Sine, 0.00, 0.30, 1047.0, 1047.0).amp(0.13).decay(9.0).fm(2.0, 0.4),
        Note::new(Sine, 0.07, 0.30, 1319.0, 1319.0).amp(0.13).decay(9.0).fm(2.0, 0.4),
        Note::new(Sine, 0.14, 0.30, 1568.0, 1568.0).amp(0.13).decay(9.0).fm(2.0, 0.4),
        Note::new(Sine, 0.21, 0.55, 2093.0, 2093.0).amp(0.14).decay(6.0).fm(2.0, 0.4),
    ],
    // A party popper: a bang, then paper.
    Popper => [
        Note::new(Noise, 0.00, 0.06, 0.0, 0.0).amp(0.34).attack(0.001).decay(55.0).low(5000.0, 1200.0),
        Note::new(Sine, 0.00, 0.07, 700.0, 140.0).bend(3.0).amp(0.30).decay(30.0),
        Note::new(Noise, 0.05, 0.50, 0.0, 0.0).amp(0.10).attack(0.02).decay(5.0).high(3000.0, 6000.0),
        Note::new(Sine, 0.10, 0.12, 3136.0, 3136.0).amp(0.06).decay(12.0),
        Note::new(Sine, 0.16, 0.12, 3951.0, 3951.0).amp(0.06).decay(12.0),
    ],
    // --- Speed and flight. ---
    Zoom => [
        Note::new(Saw, 0.00, 0.55, 170.0, 1300.0).bend(2.2).amp(0.20).attack(0.02).decay(2.0).low(500.0, 5200.0),
        Note::new(Noise, 0.00, 0.55, 0.0, 0.0).amp(0.22).attack(0.03).decay(2.4).band(700.0, 3800.0, 1.4),
    ],
    Whoosh => [
        Note::new(Noise, 0.00, 0.55, 0.0, 0.0).amp(0.30).attack(0.12).decay(3.0).band3(300.0, 2600.0, 500.0, 1.1),
    ],
    SlideUp => [
        Note::new(Sine, 0.00, 0.48, 380.0, 1800.0).bend(1.3).amp(0.24).attack(0.02).vib(6.5, 0.25),
        Note::new(Sine, 0.00, 0.48, 760.0, 3600.0).bend(1.3).amp(0.04).attack(0.02),
    ],
    SlideDown => [
        Note::new(Sine, 0.00, 0.55, 1800.0, 340.0).bend(0.9).amp(0.24).attack(0.01).decay(1.0).vib(6.5, 0.25),
    ],
    Rocket => [
        Note::new(Sine, 0.00, 0.80, 480.0, 2600.0).bend(1.1).amp(0.14).attack(0.02).decay(1.0),
        Note::new(Noise, 0.00, 0.80, 0.0, 0.0).amp(0.08).attack(0.03).decay(1.6).band(2500.0, 5000.0, 1.0),
    ],
    // A firework: the crack and the crackle after.
    Bang => [
        Note::new(Noise, 0.00, 0.55, 0.0, 0.0).amp(0.42).attack(0.001).decay(6.5).low(6000.0, 250.0),
        Note::new(Sine, 0.00, 0.40, 95.0, 34.0).bend(2.0).amp(0.42).decay(7.0),
        Note::new(Noise, 0.08, 0.70, 0.0, 0.0).amp(0.10).attack(0.05).decay(4.5).high(4000.0, 7000.0),
    ],
    UhOh => [
        Note::new(Sine, 0.00, 0.13, 540.0, 500.0).amp(0.26).attack(0.01),
        Note::new(Sine, 0.19, 0.28, 400.0, 300.0).amp(0.26).attack(0.01).decay(2.5),
    ],
    // A flying saucer, warbling.
    Ufo => [
        Note::new(Sine, 0.00, 1.60, 420.0, 520.0).amp(0.13).attack(0.15).decay(0.8).vib(7.0, 3.5),
        Note::new(Sine, 0.00, 1.60, 640.0, 760.0).amp(0.10).attack(0.15).decay(0.8).vib(9.0, 2.5),
        Note::new(Sine, 0.00, 1.60, 880.0, 1040.0).amp(0.05).attack(0.15).decay(0.8).vib(11.0, 2.0),
    ],
    // A laser in the rafters.
    Zap => [
        Note::new(Sine, 0.00, 0.22, 2600.0, 260.0).bend(2.5).amp(0.22).attack(0.002).decay(6.0).fm(3.0, 1.5),
    ],
}

/// A sound asked for: what, how high, how loud.
#[derive(Message, Clone, Copy, Debug)]
pub(crate) struct Sfx {
    pub kind: Kind,
    /// 1 is the recipe as written; 2 is an octave up.
    pub pitch: f32,
    pub gain: f32,
}

impl Sfx {
    pub(crate) fn new(kind: Kind) -> Self {
        Self {
            kind,
            pitch: 1.0,
            gain: 1.0,
        }
    }

    pub(crate) fn pitch(mut self, pitch: f32) -> Self {
        self.pitch = pitch;
        self
    }

    pub(crate) fn gain(mut self, gain: f32) -> Self {
        self.gain = gain;
        self
    }
}

/// The ring the game thread writes and the audio thread reads. One writer and
/// one reader: the writer fills a slot and then publishes it by moving the
/// head, and the reader trusts a slot only once the head has passed it.
pub(super) struct Queue {
    slots: Vec<AtomicU64>,
    head: AtomicU32,
}

impl Default for Queue {
    fn default() -> Self {
        Self {
            slots: (0..SLOTS).map(|_| AtomicU64::new(0)).collect(),
            head: AtomicU32::new(0),
        }
    }
}

impl Queue {
    pub(super) fn push(&self, kind: Kind, pitch: f32, gain: f32) {
        let at = self.head.load(Relaxed);
        let loud = (gain.clamp(0.0, 4.0) * 60.0).round() as u64;
        let word = kind as u64 | loud << 8 | u64::from(pitch.to_bits()) << 16;
        self.slots[at as usize % SLOTS].store(word, Relaxed);
        self.head.store(at.wrapping_add(1), Release);
    }
}

/// One note being played.
#[derive(Clone, Copy)]
struct Voice {
    note: Note,
    /// Samples before it starts, and how many it has played of how many.
    delay: u32,
    age: u32,
    len: u32,
    pitch: f32,
    gain: f32,
    phase: f32,
    modulator: f32,
    low: f32,
    band: f32,
    noise: u32,
    active: bool,
}

const SILENT: Voice = Voice {
    note: Note::new(Sine, 0.0, 0.0, 1.0, 1.0),
    delay: 0,
    age: 0,
    len: 0,
    pitch: 1.0,
    gain: 1.0,
    phase: 0.0,
    modulator: 0.0,
    low: 0.0,
    band: 0.0,
    noise: 1,
    active: false,
};

/// Band-limits the edge of a saw or square so it does not alias.
fn blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

impl Voice {
    /// How much of a difference it makes to lose this voice: how loud it is
    /// now, and how much of it there is left to hear.
    fn loudness(&self) -> f32 {
        let t = self.age as f32 / RATE;
        let left = self.len.saturating_sub(self.age) as f32 / self.len.max(1) as f32;
        self.note.amp * self.gain * (-self.note.decay * t).exp() * left
    }

    fn sample(&mut self) -> f32 {
        if self.delay > 0 {
            self.delay -= 1;
            return 0.0;
        }
        if self.age >= self.len {
            self.active = false;
            return 0.0;
        }
        let n = &self.note;
        let t = self.age as f32 / RATE;
        let u = (t / n.dur.max(1e-4)).min(1.0);
        self.noise ^= self.noise << 13;
        self.noise ^= self.noise >> 17;
        self.noise ^= self.noise << 5;
        let white = self.noise as f32 / 2_147_483_648.0 - 1.0;

        let osc = if n.wave == Wave::Noise {
            white
        } else {
            let glide = if (n.bend - 1.0).abs() < 1e-3 {
                u
            } else {
                u.powf(1.0 / n.bend)
            };
            let mut f = n.f0 * (n.f1 / n.f0).powf(glide);
            if n.vib_st != 0.0 {
                f *= (n.vib_st / 12.0 * (TAU * n.vib_hz * t).sin() * (1.0 - u)).exp2();
            }
            f *= self.pitch;
            let dt = (f / RATE).min(0.45);
            self.phase += dt;
            self.phase -= self.phase.floor();
            let tone = match n.wave {
                Wave::Sine => {
                    let bend = if n.fm_depth != 0.0 {
                        self.modulator += dt * n.fm_ratio;
                        self.modulator -= self.modulator.floor();
                        n.fm_depth * (1.0 - u) * (TAU * self.modulator).sin()
                    } else {
                        0.0
                    };
                    (TAU * self.phase + bend).sin()
                }
                Wave::Tri => 4.0 * (self.phase - 0.5).abs() - 1.0,
                Wave::Saw => 2.0 * self.phase - 1.0 - blep(self.phase, dt),
                Wave::Square => {
                    let up = if self.phase < n.duty { 1.0 } else { -1.0 };
                    up + blep(self.phase, dt) - blep((self.phase - n.duty).rem_euclid(1.0), dt)
                }
                Wave::Noise => white,
            };
            tone * (1.0 - n.noise) + white * n.noise
        };

        let filtered = if n.filter == Filter::None {
            osc
        } else {
            let c = if u < 0.5 {
                n.cutoff[0] + (n.cutoff[1] - n.cutoff[0]) * (u * 2.0)
            } else {
                n.cutoff[1] + (n.cutoff[2] - n.cutoff[1]) * ((u - 0.5) * 2.0)
            };
            let f = 2.0
                * (std::f32::consts::PI * (c * self.pitch.sqrt()).clamp(40.0, 9000.0) / RATE).sin();
            // This filter goes unstable when `f * f + 2 * f * q1` reaches four,
            // and near the top of its range that is not far off: at 9 kHz `f` is
            // 1.2, and a resonance under about 0.9 would blow the voice up (and,
            // through the bank, the whole mix) until it ended. So the damping is
            // never allowed below what keeps it safely inside.
            let q1 = (1.0 / n.q.max(0.5)).min(0.9 * (4.0 - f * f).max(0.4) / (2.0 * f));
            self.low += f * self.band;
            let high = osc - self.low - q1 * self.band;
            self.band += f * high;
            match n.filter {
                Filter::Low => self.low,
                Filter::Band => self.band * q1 * 1.6,
                Filter::High => high,
                Filter::None => osc,
            }
        };

        let fade_in = (t / n.attack.max(1e-4)).min(1.0);
        let fade_out = ((self.len - self.age) as f32 / (0.006 * RATE)).min(1.0);
        let envelope = fade_in * (-n.decay * t).exp() * fade_out;
        self.age += 1;
        if self.age >= self.len {
            self.active = false;
        }
        filtered * envelope * n.amp * self.gain
    }
}

/// Every voice, and the place in the ring the audio thread has got to.
pub(super) struct Bank {
    voices: [Voice; VOICES],
    read: u32,
    /// Samples played, and when each of the last few sounds started (as its kind
    /// plus one, so that nothing at all is zero), to limit how many of one sound
    /// start together.
    clock: u32,
    recent: [(u8, u32); 12],
    next_recent: usize,
    /// Seeds each voice's noise differently, whichever slot it lands in.
    seed: u32,
}

impl Default for Bank {
    fn default() -> Self {
        Self {
            voices: [SILENT; VOICES],
            read: 0,
            clock: 0,
            recent: [(0, 0); 12],
            next_recent: 0,
            seed: 0x9E37_79B9,
        }
    }
}

impl Bank {
    /// A new bank that starts from wherever the ring is, so a sound asked for
    /// before the audio thread started is not played late.
    pub(super) fn following(queue: &Queue) -> Self {
        Self {
            read: queue.head.load(Acquire),
            ..Self::default()
        }
    }

    /// The voice that would be missed least: the one to give up when they are
    /// all in use. Not one that has yet to start (a later note of the sound being
    /// placed, or of one placed just before it), unless nothing else is playing.
    fn quietest(&self) -> usize {
        let by_loudness = |a: &usize, b: &usize| {
            self.voices[*a]
                .loudness()
                .total_cmp(&self.voices[*b].loudness())
        };
        (0..VOICES)
            .filter(|&i| self.voices[i].age > 0)
            .min_by(by_loudness)
            .or_else(|| (0..VOICES).min_by(by_loudness))
            .unwrap_or(0)
    }

    /// Start every note of `kind`, unless that many of it have only just begun.
    pub(super) fn start(&mut self, kind: Kind, pitch: f32, gain: f32) {
        let tag = kind as u8 + 1;
        let alike = self
            .recent
            .iter()
            .filter(|(k, at)| *k == tag && self.clock.wrapping_sub(*at) < WINDOW)
            .count();
        if alike >= ALIKE {
            return;
        }
        self.recent[self.next_recent] = (tag, self.clock);
        self.next_recent = (self.next_recent + 1) % self.recent.len();
        let pitch = pitch.clamp(0.25, 4.0);
        for note in kind.notes() {
            let slot = self
                .voices
                .iter()
                .position(|v| !v.active)
                .unwrap_or_else(|| self.quietest());
            self.seed = self
                .seed
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223);
            self.voices[slot] = Voice {
                note: *note,
                delay: (note.at * RATE) as u32,
                age: 0,
                len: (note.dur * RATE).max(8.0) as u32,
                pitch,
                gain,
                phase: 0.0,
                modulator: 0.0,
                low: 0.0,
                band: 0.0,
                // Never zero, or the generator would stay there.
                noise: self.seed | 1,
                active: true,
            };
        }
    }

    /// The next sample, taking up anything newly asked for.
    pub(super) fn sample(&mut self, queue: &Queue) -> f32 {
        let head = queue.head.load(Acquire);
        if self.read != head {
            // Lapped by the writer: skip what was overwritten.
            if head.wrapping_sub(self.read) as usize > SLOTS {
                self.read = head.wrapping_sub(SLOTS as u32);
            }
            while self.read != head {
                let word = queue.slots[self.read as usize % SLOTS].load(Relaxed);
                self.read = self.read.wrapping_add(1);
                if let Some(&kind) = Kind::ALL.get((word & 0xff) as usize) {
                    let gain = ((word >> 8) & 0xff) as f32 / 60.0;
                    let pitch = f32::from_bits((word >> 16) as u32);
                    self.start(kind, if pitch.is_finite() { pitch } else { 1.0 }, gain);
                }
            }
        }
        self.clock = self.clock.wrapping_add(1);
        let mut sum = 0.0;
        for voice in &mut self.voices {
            if voice.active {
                sum += voice.sample();
            }
        }
        // A soft ceiling: a great many things at once is louder, not broken.
        // Close to the identity for anything a single sound does.
        CEILING * (sum / CEILING).tanh()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every sample of `kind`, played alone for as long as it takes.
    pub(super) fn render(kind: Kind, pitch: f32) -> Vec<f32> {
        let queue = Queue::default();
        let mut bank = Bank::following(&queue);
        queue.push(kind, pitch, 1.0);
        let seconds = kind
            .notes()
            .iter()
            .map(|n| n.at + n.dur)
            .fold(0.0, f32::max)
            + 0.25;
        (0..(seconds * RATE) as usize)
            .map(|_| bank.sample(&queue))
            .collect()
    }

    #[test]
    fn every_sound_is_audible_finite_bounded_and_over() {
        for &kind in Kind::ALL {
            let samples = render(kind, 1.0);
            assert!(
                samples.iter().all(|s| s.is_finite()),
                "{} has a NaN in it",
                kind.name()
            );
            let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            assert!(peak > 0.05, "{} is silent (peak {peak})", kind.name());
            assert!(peak < 0.95, "{} is too loud (peak {peak})", kind.name());
            let tail = &samples[samples.len() - 2_000..];
            assert!(
                tail.iter().all(|s| s.abs() < 1e-4),
                "{} rings on past its recipe",
                kind.name()
            );
            let mean = samples.iter().sum::<f32>() / samples.len() as f32;
            assert!(
                mean.abs() < 0.02,
                "{} has a DC offset of {mean}",
                kind.name()
            );
            // No click at the start or the finish.
            assert!(samples[0].abs() < 0.05, "{} clicks in", kind.name());
        }
    }

    #[test]
    fn the_recipes_are_sane() {
        for &kind in Kind::ALL {
            for note in kind.notes() {
                assert!(note.dur > 0.0 && note.dur < 3.0, "{}", kind.name());
                assert!(note.at >= 0.0 && note.at < 3.0, "{}", kind.name());
                assert!(
                    note.amp > 0.0 && note.amp <= 0.7,
                    "{} note too loud",
                    kind.name()
                );
                if note.wave != Wave::Noise {
                    assert!(note.f0 > 20.0 && note.f1 > 20.0, "{}", kind.name());
                    assert!(note.f0 < 8000.0 && note.f1 < 8000.0, "{}", kind.name());
                }
                if note.filter != Filter::None {
                    assert!(
                        note.cutoff.iter().all(|c| *c > 40.0 && *c < 9000.0),
                        "{} cutoff",
                        kind.name()
                    );
                }
            }
        }
        // Names are unique, so a sound can be found by what it is called.
        let mut names: Vec<_> = Kind::ALL.iter().map(|k| k.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Kind::ALL.len());
    }

    /// Zero crossings a second: a fair measure of the pitch of a plain tone.
    fn crossings(samples: &[f32]) -> f32 {
        let n = samples
            .windows(2)
            .filter(|w| w[0] <= 0.0 && w[1] > 0.0)
            .count();
        n as f32 / (samples.len() as f32 / RATE)
    }

    #[test]
    fn pitch_moves_a_sound_the_way_it_says() {
        // The bell: a steady tone, so its crossings are its pitch.
        let plain = render(Kind::Ding, 1.0);
        let octave = render(Kind::Ding, 2.0);
        let ratio = crossings(&octave[..8000]) / crossings(&plain[..8000]);
        assert!(
            (ratio - 2.0).abs() < 0.25,
            "an octave up is {ratio} times faster"
        );
    }

    #[test]
    fn a_crowd_of_sounds_cannot_overload_the_bank_or_the_ring() {
        let queue = Queue::default();
        let mut bank = Bank::following(&queue);
        // Far more than the ring holds, before the reader looks.
        for i in 0..1_000 {
            queue.push(Kind::ALL[i % Kind::ALL.len()], 1.0, 1.0);
        }
        let mut peak = 0.0f32;
        for _ in 0..(2.0 * RATE) as usize {
            let s = bank.sample(&queue);
            assert!(s.is_finite());
            peak = peak.max(s.abs());
        }
        assert!(peak <= 1.0, "the soft ceiling holds ({peak})");
        // And it all dies away.
        for _ in 0..(6.0 * RATE) as usize {
            bank.sample(&queue);
        }
        assert!(bank.sample(&queue).abs() < 1e-3);
    }

    /// A voice of the given `note`, ready to play.
    fn voice_of(note: Note, pitch: f32) -> Voice {
        Voice {
            note,
            len: (note.dur * RATE) as u32,
            pitch,
            active: true,
            noise: 0x1234_5679,
            ..SILENT
        }
    }

    #[test]
    fn a_filter_with_little_resonance_stays_stable_at_the_top_of_its_range() {
        // What would go wrong with a resonance under 0.9 at 9 kHz, if it were
        // asked for: the filter runs away and never comes back.
        for q in [0.5, 0.7, 0.9, 1.0] {
            for pitch in [1.0, 4.0] {
                let mut note = Note::new(Noise, 0.0, 1.0, 440.0, 440.0).low(9_000.0, 9_000.0);
                note.q = q;
                let mut voice = voice_of(note, pitch);
                let mut peak = 0.0f32;
                for _ in 0..RATE as usize {
                    let s = voice.sample();
                    assert!(s.is_finite(), "q {q} at pitch {pitch} went to {s}");
                    peak = peak.max(s.abs());
                }
                assert!(peak < 2.0, "q {q} at pitch {pitch} peaks at {peak}");
            }
        }
    }

    #[test]
    fn a_burst_of_one_sound_is_capped_and_the_next_burst_is_not() {
        let queue = Queue::default();
        let mut bank = Bank::following(&queue);
        let playing = |bank: &Bank| bank.voices.iter().filter(|v| v.active).count();
        for _ in 0..20 {
            queue.push(Kind::Pop, 1.0, 1.0);
        }
        bank.sample(&queue);
        let notes = Kind::Pop.notes().len();
        assert_eq!(playing(&bank), ALIKE * notes, "{ALIKE} pops at a time");
        // A different sound is not held back by it.
        queue.push(Kind::Boing, 1.0, 1.0);
        bank.sample(&queue);
        assert_eq!(playing(&bank), ALIKE * notes + Kind::Boing.notes().len());
        // And a little later the pops may come again.
        for _ in 0..(WINDOW + 10) {
            bank.sample(&queue);
        }
        let before = playing(&bank);
        queue.push(Kind::Pop, 1.0, 1.0);
        bank.sample(&queue);
        assert!(playing(&bank) > before || before >= VOICES);
    }

    #[test]
    fn when_every_voice_is_busy_the_quietest_is_the_one_that_goes() {
        let mut bank = Bank::default();
        let loud = Note::new(Sine, 0.0, 4.0, 440.0, 440.0).amp(0.5);
        for voice in &mut bank.voices {
            *voice = voice_of(loud, 1.0);
            voice.age = 1_000;
        }
        // One that has all but faded away, though it is no older than the rest.
        bank.voices[7].note = loud.decay(40.0);
        bank.voices[7].age = 30_000;
        bank.start(Kind::Boing, 1.0, 1.0);
        assert!(bank.voices[7].age == 0, "the faded voice was kept");
        assert_eq!(
            bank.voices.iter().filter(|v| v.age == 1_000).count(),
            VOICES - 1 - (Kind::Boing.notes().len() - 1),
            "and the rest were left alone (a sound of several notes takes several)"
        );
    }

    #[test]
    fn nonsense_in_the_ring_is_ignored() {
        let queue = Queue::default();
        let mut bank = Bank::following(&queue);
        queue.push(Kind::Pop, f32::NAN, 1.0);
        queue.slots[1].store(u64::MAX, Relaxed);
        queue.head.store(2, Release);
        for _ in 0..4_000 {
            assert!(bank.sample(&queue).is_finite());
        }
    }

    /// A mono 16-bit WAV file of `samples`.
    fn wav(samples: &[f32]) -> Vec<u8> {
        let data = (samples.len() * 2) as u32;
        let mut out = Vec::with_capacity(44 + samples.len() * 2);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // PCM
        out.extend_from_slice(&1u16.to_le_bytes()); // mono
        out.extend_from_slice(&44_100u32.to_le_bytes());
        out.extend_from_slice(&(44_100u32 * 2).to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data.to_le_bytes());
        for s in samples {
            out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32_000.0) as i16).to_le_bytes());
        }
        out
    }

    #[test]
    fn a_wav_has_the_header_a_player_expects() {
        let bytes = wav(&[0.0, 0.5, -0.5]);
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 6);
        assert_eq!(bytes.len(), 44 + 6);
    }

    /// Every sound as a WAV, for a listen (and for `ffmpeg -lavfi
    /// showspectrumpic` to draw): `cargo test --lib write_the_sfx -- --ignored`.
    #[test]
    #[ignore = "writes dist/sfx"]
    fn write_the_sfx() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("dist/sfx");
        std::fs::create_dir_all(&dir).unwrap();
        for &kind in Kind::ALL {
            std::fs::write(
                dir.join(format!("{}.wav", kind.name())),
                wav(&render(kind, 1.0)),
            )
            .unwrap();
        }
    }
}
