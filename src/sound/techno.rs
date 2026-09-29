//! Techno, made of nothing, that gets louder the faster you go.
//!
//! A step sequencer on the audio thread: a kick on every beat with the
//! sidechain duck that makes the rest of it breathe, claps on two and four, hats
//! on the off-beat, an acid bass line through a resonant filter, a delayed
//! arpeggio, pad chords, and a riser and a fill on the way out of every eighth
//! bar. Four chords, A minor, F, C and G, a bar each.
//!
//! What plays is up to one number, the *energy*: 0 is a kick and a hat, 0.3 is
//! the bass and the pad, 0.5 the claps and the open hat, 0.7 the arpeggio and
//! the sixteenth hats, and above that the fills. The game sets it from how fast
//! the car is going and whether it is boosting, and the music follows over a
//! third of a second, so the track builds as the car does.
//!
//! The sequencer counts sixteenths as it renders them and hands the count to the
//! game ([`Techno::position`]), which is how the lights stay on the beat.
//! Nothing here allocates once it has started, except the echo's buffer, which
//! is made with the mixer.

use super::sfx::RATE;
use crate::fun::beat::BPM;

const TAU: f32 = std::f32::consts::TAU;
/// Samples in a sixteenth note.
const STEP: f32 = RATE * 60.0 / BPM / 4.0;

/// The notes of the four chords, as MIDI numbers: the bass root, then the three
/// tones of the chord, voiced close so the pad barely has to move.
const CHORDS: [(i32, [i32; 3]); 4] = [
    (45, [57, 60, 64]), // A minor
    (41, [57, 60, 65]), // F
    (48, [55, 60, 64]), // C
    (43, [55, 59, 62]), // G
];

/// The bass, a step at a time: semitones above the chord's root, `None` for a
/// rest; then whether the step is accented and whether it slides to the next.
const BASS: [Option<(i32, bool, bool)>; 16] = [
    None,
    Some((0, false, false)),
    Some((0, true, false)),
    Some((12, false, true)),
    None,
    Some((0, false, false)),
    Some((0, true, false)),
    None,
    None,
    Some((7, false, false)),
    Some((0, true, false)),
    Some((10, false, true)),
    None,
    Some((0, false, false)),
    Some((12, true, false)),
    Some((7, false, true)),
];

/// The arpeggio: which chord tone, and how many octaves up from the chord.
const ARP: [(usize, i32); 16] = [
    (0, 1),
    (1, 1),
    (2, 1),
    (2, 2),
    (1, 1),
    (2, 1),
    (0, 2),
    (1, 2),
    (2, 1),
    (1, 1),
    (0, 1),
    (1, 1),
    (2, 1),
    (0, 2),
    (2, 1),
    (1, 1),
];

/// How far through its rise the riser is, if it is sounding. `step` is the
/// sixteenth being heard and `to_step` how much of it is still to come. It is the
/// second half of the last bar of the phrase, which is where the claps roll and
/// the kick drops out, and only when the music is loud enough for a drop to be
/// worth leading into.
fn riser_at(step: u64, to_step: f32, energy: f32) -> Option<f32> {
    let (bar, s) = ((step / 16) % 8, step % 16);
    (bar == 7 && s >= 8 && energy > 0.7)
        .then(|| (((s - 8) as f32 * STEP + (STEP - to_step)) / (8.0 * STEP)).clamp(0.0, 1.0))
}

fn hz(midi: i32) -> f32 {
    440.0 * ((midi - 69) as f32 / 12.0).exp2()
}

/// 0 below `low`, 1 above `high`, smooth between: how much of a layer is in.
fn gate(energy: f32, low: f32, high: f32) -> f32 {
    let t = ((energy - low) / (high - low)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A state-variable filter, the workhorse of every synth voice here.
#[derive(Clone, Copy, Default)]
struct Svf {
    low: f32,
    band: f32,
}

impl Svf {
    /// `f` is `2 sin(pi fc / rate)`; `q1` is the reciprocal of the resonance.
    fn run(&mut self, x: f32, f: f32, q1: f32) -> (f32, f32, f32) {
        self.low += f * self.band;
        let high = x - self.low - q1 * self.band;
        self.band += f * high;
        (self.low, self.band, high)
    }
}

fn coefficient(cutoff: f32) -> f32 {
    2.0 * (std::f32::consts::PI * cutoff.clamp(30.0, 9_000.0) / RATE).sin()
}

#[derive(Default)]
struct Kick {
    t: f32,
    phase: f32,
    on: bool,
}

impl Kick {
    fn trigger(&mut self) {
        self.t = 0.0;
        self.phase = 0.0;
        self.on = true;
    }

    fn sample(&mut self, noise: f32) -> f32 {
        if !self.on {
            return 0.0;
        }
        // A sine that falls from a thump to a boom, and a click at the front.
        let f = 46.0 + 118.0 * (-self.t * 34.0).exp();
        self.phase = (self.phase + f / RATE).fract();
        let body = (TAU * self.phase).sin() * (-self.t * 8.0).exp();
        let click = noise * (-self.t * 900.0).exp() * 0.35;
        self.t += 1.0 / RATE;
        if self.t > 0.45 {
            self.on = false;
        }
        ((body * 1.5).tanh() + click) * 0.62
    }
}

#[derive(Default)]
struct Clap {
    t: f32,
    on: bool,
    filter: Svf,
}

impl Clap {
    fn trigger(&mut self) {
        self.t = 0.0;
        self.on = true;
    }

    fn sample(&mut self, noise: f32) -> f32 {
        if !self.on {
            return 0.0;
        }
        // Three quick smacks and a tail: a handful of hands.
        let mut env = (-(self.t - 0.033).max(0.0) * 26.0).exp() * 0.55;
        for k in 0..3 {
            let since = self.t - k as f32 * 0.011;
            if since >= 0.0 {
                env += (-since * 190.0).exp();
            }
        }
        let (_, band, _) = self.filter.run(noise, coefficient(1_500.0), 0.55);
        self.t += 1.0 / RATE;
        if self.t > 0.3 {
            self.on = false;
        }
        band * env * 0.34
    }
}

#[derive(Default)]
struct Hat {
    env: f32,
    decay: f32,
    last: f32,
}

impl Hat {
    fn trigger(&mut self, level: f32, decay: f32) {
        self.env = level;
        self.decay = decay;
    }

    fn sample(&mut self, noise: f32) -> f32 {
        if self.env < 1e-4 {
            return 0.0;
        }
        // Differencing white noise leaves the top of it.
        let bright = noise - self.last;
        self.last = noise;
        self.env *= 1.0 - self.decay / RATE;
        bright * self.env * 0.22
    }
}

#[derive(Default)]
struct Bass {
    phase: f32,
    freq: f32,
    target: f32,
    slide: bool,
    t: f32,
    accent: bool,
    on: bool,
    filter: Svf,
    filter_two: Svf,
}

impl Bass {
    /// A step with no note in it. A slide runs into the note that follows it and
    /// no further, so a rest is where one ends: otherwise the next note, however
    /// late, would carry on from the last instead of being played.
    fn rest(&mut self) {
        self.slide = false;
    }

    fn trigger(&mut self, freq: f32, accent: bool, slide: bool) {
        // A slid note does not restart: it carries on from the last.
        if !(self.slide && self.on) {
            self.t = 0.0;
        }
        self.target = freq;
        if self.freq == 0.0 || !self.slide {
            self.freq = freq;
        }
        self.slide = slide;
        self.accent = accent;
        self.on = true;
    }

    fn sample(&mut self, drive: f32) -> f32 {
        if !self.on {
            return 0.0;
        }
        self.freq += (self.target - self.freq) * 0.0016;
        self.phase = (self.phase + self.freq / RATE).fract();
        let saw = 2.0 * self.phase - 1.0;
        // The filter opens on every note and closes again: the acid sound.
        let open = if self.accent { 2_900.0 } else { 1_500.0 };
        let cutoff = 170.0 + open * drive * (-self.t * 13.0).exp();
        let f = coefficient(cutoff);
        let (low, _, _) = self.filter.run(saw, f, 0.22);
        let (low, _, _) = self.filter_two.run(low, f, 0.7);
        let env = (self.t / 0.004).min(1.0) * (-self.t * if self.accent { 4.6 } else { 7.5 }).exp();
        self.t += 1.0 / RATE;
        if self.t > 0.6 {
            self.on = false;
        }
        (low * 2.6).tanh() * env * 0.36
    }
}

/// One plucked voice of the arpeggio: two saws a few cents apart.
#[derive(Default)]
struct Pluck {
    phases: [f32; 2],
    freq: f32,
    t: f32,
    on: bool,
    filter: Svf,
}

impl Pluck {
    fn trigger(&mut self, freq: f32) {
        self.freq = freq;
        self.t = 0.0;
        self.on = true;
    }

    fn sample(&mut self) -> f32 {
        if !self.on {
            return 0.0;
        }
        let detune = [0.9965, 1.0035];
        let mut x = 0.0;
        for (phase, d) in self.phases.iter_mut().zip(detune) {
            *phase = (*phase + self.freq * d / RATE).fract();
            x += 2.0 * *phase - 1.0;
        }
        let cutoff = 700.0 + 3_600.0 * (-self.t * 16.0).exp();
        let (low, _, _) = self.filter.run(x * 0.5, coefficient(cutoff), 0.9);
        let env = (self.t / 0.003).min(1.0) * (-self.t * 11.0).exp();
        self.t += 1.0 / RATE;
        if self.t > 0.5 {
            self.on = false;
        }
        low * env * 0.30
    }
}

/// The sustained chord: three notes, two saws each, through a slow filter.
struct Pad {
    phases: [f32; 6],
    freqs: [f32; 3],
    filter: Svf,
    lfo: f32,
}

impl Default for Pad {
    fn default() -> Self {
        Self {
            phases: [0.0; 6],
            freqs: [hz(57), hz(60), hz(64)],
            filter: Svf::default(),
            lfo: 0.0,
        }
    }
}

impl Pad {
    fn sample(&mut self, chord: [i32; 3]) -> f32 {
        let mut x = 0.0;
        for (i, &note) in chord.iter().enumerate() {
            self.freqs[i] += (hz(note) - self.freqs[i]) * 0.0006;
            for (j, d) in [0.996f32, 1.004].into_iter().enumerate() {
                let phase = &mut self.phases[i * 2 + j];
                *phase = (*phase + self.freqs[i] * d / RATE).fract();
                x += 2.0 * *phase - 1.0;
            }
        }
        self.lfo = (self.lfo + 0.13 / RATE).fract();
        let cutoff = 950.0 + 500.0 * (TAU * self.lfo).sin();
        let (low, _, _) = self.filter.run(x / 6.0, coefficient(cutoff), 1.4);
        low * 0.22
    }
}

/// The noise sweep that leads into the drop.
#[derive(Default)]
struct Riser {
    filter: Svf,
}

pub(super) struct Techno {
    to_step: f32,
    step: u64,
    energy: f32,
    level: f32,
    noise: u32,
    /// The sidechain: 1 at a kick, easing back to 0 in a couple of tenths.
    duck: f32,
    kick: Kick,
    clap: Clap,
    closed: Hat,
    open: Hat,
    bass: Bass,
    pluck: Pluck,
    pad: Pad,
    riser: Riser,
    echo: Vec<f32>,
    echo_at: usize,
    /// Nothing has been asked for, and everything that was ringing has been let
    /// go of, so that nothing is left waiting to sound off the beat.
    silent: bool,
    /// The last input and output of the filter that takes the DC out of the mix.
    dc: (f32, f32),
}

impl Default for Techno {
    fn default() -> Self {
        Self {
            to_step: 0.0,
            step: 0,
            energy: 0.0,
            level: 0.0,
            noise: 0x1234_5678,
            duck: 0.0,
            kick: Kick::default(),
            clap: Clap::default(),
            closed: Hat::default(),
            open: Hat::default(),
            bass: Bass::default(),
            pluck: Pluck::default(),
            pad: Pad::default(),
            riser: Riser::default(),
            // A dotted eighth: three sixteenths.
            echo: vec![0.0; (STEP * 3.0) as usize],
            echo_at: 0,
            silent: false,
            dc: (0.0, 0.0),
        }
    }
}

impl Techno {
    /// Where the music is, in 256ths of a beat since it began.
    pub(super) fn position(&self) -> u32 {
        let elapsed = self.step.saturating_sub(1);
        let within = ((STEP - self.to_step.max(0.0)) / STEP).clamp(0.0, 0.999);
        (elapsed as u32).wrapping_mul(64) + (within * 64.0) as u32
    }

    fn white(&mut self) -> f32 {
        self.noise ^= self.noise << 13;
        self.noise ^= self.noise >> 17;
        self.noise ^= self.noise << 5;
        self.noise as f32 / 2_147_483_648.0 - 1.0
    }

    /// Start whatever plays on this sixteenth.
    fn on_step(&mut self) {
        let s = (self.step % 16) as usize;
        let bar = ((self.step / 16) % 8) as usize;
        let (root, chord) = CHORDS[bar % 4];
        let e = self.energy;

        // The kick, every beat; a bar's last beat is left out before the drop.
        let cut = bar == 7 && s >= 12 && e > 0.7;
        if s.is_multiple_of(4) && !cut {
            self.kick.trigger();
            self.duck = 1.0;
        }
        // Claps on two and four, and a roll into the drop.
        let roll = bar == 7 && s >= 8 && e > 0.7 && (s.is_multiple_of(2) || s >= 12);
        if (s == 4 || s == 12 || roll) && e > 0.35 {
            self.clap.trigger();
        }
        // Hats: closed on the off-beat, open on every other one, ghosts above.
        if s % 4 == 2 && e > 0.08 {
            self.closed.trigger(1.0, 95.0);
        } else if s % 2 == 1 && e > 0.68 {
            self.closed.trigger(0.42, 130.0);
        }
        if (s == 6 || s == 14) && e > 0.5 {
            self.open.trigger(0.85, 22.0);
        }
        // The bass, from a third of the way up, in the octave the chord's root is
        // in: under the pad, and not on top of it.
        if e > 0.22 {
            match BASS[s] {
                Some((semitones, accent, slide)) => {
                    self.bass.trigger(hz(root + semitones), accent, slide);
                }
                None => self.bass.rest(),
            }
        }
        // The arpeggio, on every step but the last of each beat.
        if e > 0.6 && s % 4 != 3 {
            let (tone, octave) = ARP[s];
            self.pluck.trigger(hz(chord[tone] + 12 * octave));
        }
    }

    /// Let go of everything that is sounding, and everything that has been set
    /// to sound: the drums, the bass and the echo. What is left ringing when the
    /// music is turned off would otherwise start the moment it was turned on
    /// again, in the middle of a beat.
    fn let_go(&mut self) {
        self.kick.on = false;
        self.clap.on = false;
        self.clap.filter = Svf::default();
        self.closed.env = 0.0;
        self.open.env = 0.0;
        self.bass.on = false;
        self.bass.slide = false;
        self.bass.filter = Svf::default();
        self.bass.filter_two = Svf::default();
        self.pluck.on = false;
        self.pluck.filter = Svf::default();
        self.riser.filter = Svf::default();
        self.duck = 0.0;
        self.echo.fill(0.0);
    }

    /// The next sample, in `[-1, 1]`, for the given target `energy` and `level`.
    pub(super) fn sample(&mut self, energy: f32, level: f32) -> f32 {
        self.energy += (energy - self.energy) * 0.00007;
        self.level += (level - self.level) * 0.0004;
        self.to_step -= 1.0;
        if self.to_step <= 0.0 {
            self.to_step += STEP;
            // The time goes on while it is silent, and the notes do not.
            if !self.silent {
                self.on_step();
            }
            self.step += 1;
        }
        // Silent, and nothing asked: keep the time, skip the work.
        if self.level < 1e-4 && level <= 0.0 {
            if !self.silent {
                self.silent = true;
                self.let_go();
            }
            return 0.0;
        }
        self.silent = false;
        let e = self.energy;
        let noise = self.white();
        let bar = ((self.step.saturating_sub(1) / 16) % 8) as usize;
        let (_, chord) = CHORDS[bar % 4];
        self.duck *= 1.0 - 9.5 / RATE;
        let breathe = 1.0 - 0.68 * self.duck;

        let kick = self.kick.sample(noise);
        let clap = self.clap.sample(noise) * gate(e, 0.3, 0.45);
        let closed = self.closed.sample(noise) * gate(e, 0.05, 0.15);
        let open = self.open.sample(noise) * gate(e, 0.45, 0.55);
        let bass = self.bass.sample(0.55 + 0.6 * e) * gate(e, 0.18, 0.3) * breathe;
        let pad = self.pad.sample(chord) * gate(e, 0.12, 0.3) * (0.4 + 0.6 * breathe);
        let pluck = self.pluck.sample();
        let lead = pluck * gate(e, 0.55, 0.68) * breathe;
        // The echo: what was played three sixteenths ago, fed back on itself.
        let heard = self.echo[self.echo_at];
        self.echo[self.echo_at] = lead + heard * 0.42;
        self.echo_at = (self.echo_at + 1) % self.echo.len();
        let lead = lead + heard * 0.5;

        // The riser: noise sweeping up through the last half of the last bar,
        // under the roll into the drop.
        let mut riser = 0.0;
        if let Some(done) = riser_at(self.step.saturating_sub(1), self.to_step, e) {
            let cutoff = 400.0 * 22.0f32.powf(done);
            let (_, band, _) = self.riser.filter.run(noise, coefficient(cutoff), 0.5);
            riser = band * done * done * 0.20;
        }

        let mix = kick + clap + closed + open + bass + pad + lead + riser;
        // A note that dies away as it rises leaves a little more of one side of
        // the wave than the other, and the lower the note the more of it. It is
        // taken out below anything that can be heard, a few hertz, so that it does
        // not use up the room above it or thump when the music stops.
        let level_mix = mix - self.dc.0 + 0.9995 * self.dc.1;
        self.dc = (mix, level_mix);
        (level_mix * 1.25).tanh() * 0.8 * self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(seconds: f32, energy: f32, level: f32) -> Vec<f32> {
        let mut techno = Techno::default();
        (0..(seconds * RATE) as usize)
            .map(|_| techno.sample(energy, level))
            .collect()
    }

    #[test]
    fn every_energy_is_finite_bounded_and_audible() {
        for energy in [0.0, 0.1, 0.3, 0.5, 0.7, 0.9, 1.0] {
            // Long enough to reach the fills of the eighth bar.
            let samples = render(24.0, energy, 1.0);
            assert!(samples.iter().all(|s| s.is_finite()), "energy {energy}");
            let peak = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
            assert!(peak > 0.15, "energy {energy} peaks at only {peak}");
            assert!(peak < 0.95, "energy {energy} peaks at {peak}");
            let mean = samples.iter().sum::<f32>() / samples.len() as f32;
            assert!(
                mean.abs() < 0.03,
                "energy {energy} has a DC offset of {mean}"
            );
        }
    }

    #[test]
    fn more_energy_is_more_music() {
        // The kick is the same at every energy and owns most of the power, so
        // what the layers add shows in the brightness: the power of the
        // difference between one sample and the next.
        let measure = |energy: f32| {
            let samples = render(12.0, energy, 1.0);
            // Past the slew, so it is the settled level that is measured.
            let settled = &samples[(5.0 * RATE) as usize..];
            let power = settled.iter().map(|s| s * s).sum::<f32>() / settled.len() as f32;
            let bright = settled
                .windows(2)
                .map(|w| (w[1] - w[0]).powi(2))
                .sum::<f32>()
                / settled.len() as f32;
            (power, bright)
        };
        let (low, mid, high) = (measure(0.05), measure(0.45), measure(0.95));
        assert!(mid.1 > low.1 * 1.3, "{low:?} {mid:?}");
        assert!(high.1 > mid.1 * 1.3, "{mid:?} {high:?}");
        assert!(
            mid.0 >= low.0 * 0.95 && high.0 >= mid.0 * 0.95,
            "louder, not quieter"
        );
    }

    #[test]
    fn nothing_is_heard_at_zero_level_but_time_still_passes() {
        let mut techno = Techno::default();
        let mut peak = 0.0f32;
        for _ in 0..(6.0 * RATE) as usize {
            peak = peak.max(techno.sample(1.0, 0.0).abs());
        }
        assert!(peak < 1e-3, "{peak}");
        // Six seconds at 140 beats a minute is fourteen beats.
        let beats = techno.position() as f32 / 256.0;
        assert!((beats - 14.0).abs() < 0.2, "{beats}");
    }

    #[test]
    fn the_music_comes_back_on_the_beat_and_not_between_beats() {
        // Only the kick is heard at this energy, so what is heard first is what
        // was started first.
        let mut techno = Techno::default();
        for _ in 0..(4.0 * RATE) as usize {
            techno.sample(0.05, 1.0);
        }
        // Turned off, and left off past the point where it stops rendering,
        // until it is half way through a beat.
        let mut quiet = 0;
        while quiet < RATE as usize || !(100..160).contains(&(techno.position() % 256)) {
            techno.sample(0.05, 0.0);
            quiet += 1;
        }
        let mut first = None;
        for _ in 0..(RATE * 0.6) as usize {
            if techno.sample(0.05, 1.0).abs() > 0.03 {
                first = Some(techno.position() as f32 / 256.0);
                break;
            }
        }
        let beats = first.expect("nothing was heard when it was turned back on");
        assert!(
            (beats - beats.round()).abs() < 0.03,
            "the first thing heard was at {beats:.2} beats, between two of them"
        );
    }

    #[test]
    fn the_riser_is_under_the_roll_into_the_drop_and_only_rises() {
        // Sixteenths being heard, across one phrase of eight bars.
        let rising: Vec<u64> = (0..128)
            .filter(|&s| riser_at(s, STEP / 2.0, 1.0).is_some())
            .collect();
        assert_eq!(rising, (120..128).collect::<Vec<_>>());
        // Quiet music has no drop to lead into.
        assert!(riser_at(125, STEP / 2.0, 0.5).is_none());
        // And through the half bar it goes from nothing to nearly everything.
        let mut last = -1.0;
        for s in 120..128u64 {
            for left in [STEP * 0.9, STEP * 0.5, STEP * 0.1] {
                let done = riser_at(s, left, 1.0).unwrap();
                assert!(
                    (0.0..=1.0).contains(&done) && done > last,
                    "{done} after {last}"
                );
                last = done;
            }
        }
        assert!(last > 0.97);
    }

    #[test]
    fn a_rest_ends_a_slide_so_the_next_note_is_played() {
        let mut bass = Bass::default();
        bass.trigger(hz(57), false, true);
        for _ in 0..4_000 {
            bass.sample(0.8);
        }
        assert!(bass.t > 0.05);
        // The step after a slide is a rest, and the one after that is a note.
        bass.rest();
        bass.trigger(hz(60), false, false);
        assert_eq!(bass.t, 0.0, "the note carried on from the last one");
        // A note that is slid into, on the other hand, does not start again.
        let mut slid = Bass::default();
        slid.trigger(hz(57), false, true);
        for _ in 0..4_000 {
            slid.sample(0.8);
        }
        slid.trigger(hz(60), false, false);
        assert!(slid.t > 0.05, "a slide restarted");
    }

    #[test]
    fn the_position_runs_at_the_tempo_it_says() {
        let mut techno = Techno::default();
        let mut last = 0;
        for i in 0..(60.0 * RATE) as usize {
            techno.sample(0.5, 1.0);
            if i % 4_410 == 0 {
                let now = techno.position();
                assert!(now >= last, "the position went backwards at sample {i}");
                last = now;
            }
        }
        let beats = techno.position() as f32 / 256.0;
        assert!((beats - BPM).abs() < 0.3, "a minute of it is {beats} beats");
    }

    #[test]
    fn a_kick_lands_on_every_beat() {
        // The floor of the mix at low energy is the kick: look for its thump.
        let samples = render(4.0, 0.05, 1.0);
        let beat = (RATE * 60.0 / BPM) as usize;
        let window = |from: usize| -> f32 {
            samples[from..from + 800]
                .iter()
                .map(|s| s.abs())
                .sum::<f32>()
                / 800.0
        };
        for n in 1..6 {
            let on = window(n * beat + 300);
            let off = window(n * beat + beat / 2 + 300);
            assert!(on > off * 3.0, "beat {n}: {on} against {off}");
        }
    }

    #[test]
    fn the_level_rises_without_a_click() {
        // Noise-made percussion jumps from sample to sample by its nature, so
        // a click is looked for in the body of the sound: the average of a
        // few dozen samples must not step.
        let mut techno = Techno::default();
        let samples: Vec<f32> = (0..(3.0 * RATE) as usize)
            .map(|_| techno.sample(0.6, 1.0))
            .collect();
        let smooth: Vec<f32> = samples
            .windows(48)
            .map(|w| w.iter().sum::<f32>() / 48.0)
            .collect();
        let worst = smooth
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0f32, f32::max);
        assert!(worst < 0.06, "the body of the sound stepped by {worst}");
    }

    /// Thirty seconds climbing from a kick to everything, as
    /// `dist/techno.wav`, for a listen:
    /// `cargo test --lib write_the_techno -- --ignored`.
    #[test]
    #[ignore = "writes dist/techno.wav"]
    fn write_the_techno() {
        let mut techno = Techno::default();
        let mut pcm = Vec::new();
        for i in 0..(64.0 * RATE) as usize {
            let t = i as f32 / RATE;
            let energy = (t / 48.0).min(1.0);
            let s = techno.sample(energy, 1.0);
            pcm.extend_from_slice(&((s.clamp(-1.0, 1.0) * 30_000.0) as i16).to_le_bytes());
        }
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&44_100u32.to_le_bytes());
        wav.extend_from_slice(&88_200u32.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
        wav.extend_from_slice(&pcm);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("dist");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("techno.wav"), wav).unwrap();
    }
}
