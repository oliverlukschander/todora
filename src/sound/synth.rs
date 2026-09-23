// Prepared by tools/prepare_tyre_audio.py. Attribution travels in assets/audio.
const TYRES: &[u8] = include_bytes!("../../assets/audio/tyres.s16le");

#[derive(Default)]
pub(super) struct Tyres {
    position: f32,
    filtered: f32,
    rolling_position: f32,
    rolling_filters: [f32; 4],
    rolling: f32,
    scrub: f32,
    squeal: f32,
}

impl Tyres {
    pub fn sample(&mut self, rolling: f32, scrub: f32, squeal: f32) -> f32 {
        // Rounded attacks and releases, rather than hiss switched on each frame.
        self.rolling += (rolling - self.rolling) * 0.0005;
        self.scrub += (scrub - self.scrub) * 0.0005;
        self.squeal += (squeal - self.squeal) * 0.0007;
        let slide = (self.squeal / 0.32).clamp(0.0, 1.0);
        let length = TYRES.len() / 2;
        let at = self.position as usize;
        let fraction = self.position.fract();
        let read = |i: usize| i16::from_le_bytes([TYRES[i * 2], TYRES[i * 2 + 1]]) as f32 / 32768.0;
        let sample = read(at) * (1.0 - fraction) + read((at + 1) % length) * fraction;
        self.position = (self.position + 0.76 + 0.22 * slide) % length as f32;
        // The same rubber sound: muted and low near the limit, opening up as it slides.
        self.filtered += (sample - self.filtered) * (0.14 + 0.25 * slide);
        // A separate, much slower read keeps rolling contact deep even while
        // the cornering squeal rises. Four low-pass stages remove the upper
        // harmonics, leaving a muffled bass rumble underneath the grip warning.
        let at = self.rolling_position as usize;
        let fraction = self.rolling_position.fract();
        let mut rumble = read(at) * (1.0 - fraction) + read((at + 1) % length) * fraction;
        self.rolling_position = (self.rolling_position + 0.09) % length as f32;
        for low in &mut self.rolling_filters {
            *low += (rumble - *low) * 0.014;
            rumble = *low;
        }
        rumble * self.rolling * 0.75 + self.filtered * (self.scrub + self.squeal)
    }
}

/// The engine: a few harmonics of a firing note, a little filtered noise
/// under load, and a pitch that climbs through six gears with speed. All of it
/// made here, sample by sample, from two numbers the game publishes: how far
/// through the rev range the engine is, and how hard it is being asked to pull.
#[derive(Default)]
pub(super) struct Engine {
    phase: f32,
    rpm: f32,
    load: f32,
    rumble: f32,
    noise: u32,
}

impl Engine {
    /// `rpm` and `load` are 0 to 1; `level` is the volume, 0 when silent.
    pub fn sample(&mut self, rpm: f32, load: f32, level: f32) -> f32 {
        // Slewed per sample so a gear change is a quick swoop, not a click.
        self.rpm += (rpm - self.rpm) * 0.0009;
        self.load += (load - self.load) * 0.0006;
        let pitch = 48.0 * (1.0 + 3.2 * self.rpm);
        self.phase = (self.phase + pitch / RATE * std::f32::consts::TAU) % std::f32::consts::TAU;
        let p = self.phase;
        let tone =
            p.sin() + 0.55 * (2.0 * p).sin() + 0.3 * (3.0 * p + 0.4).sin() + 0.12 * (5.0 * p).sin();
        // A cheap, fixed noise source, filtered low for the intake roar.
        self.noise = self
            .noise
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        let white = (self.noise >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
        self.rumble += (white - self.rumble) * 0.05;
        let voice = tone * (0.35 + 0.65 * self.load) + self.rumble * 1.2 * self.load;
        voice * level * 0.045
    }
}

/// Where the engine is in its rev range at `speed` of `top` metres a second:
/// six gears, each climbing from a little above idle to the top of the range,
/// and idle when stopped.
pub(crate) fn revs(speed: f32, top: f32) -> f32 {
    const GEARS: f32 = 6.0;
    let through = (speed.abs() / top.max(1.0)).clamp(0.0, 1.0) * GEARS;
    if through < 0.02 {
        return 0.12;
    }
    let gear = through.floor().min(GEARS - 1.0);
    let within = through - gear;
    // First gear starts from idle; the others drop back as the next is taken.
    let floor = if gear == 0.0 { 0.12 } else { 0.42 };
    floor + (0.95 - floor) * within
}

/// What the game can ask the beeper for.
#[derive(bevy::prelude::Message, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cue {
    /// A red start light.
    Red,
    /// The green one.
    Go,
    /// A new best lap: three rising notes.
    Best,
    /// The crowd, for a new best: a swell of voices, no tune.
    Cheer,
}

impl Cue {
    pub(super) fn code(self) -> u32 {
        match self {
            Self::Red => 0,
            Self::Go => 1,
            Self::Best => 2,
            Self::Cheer => 3,
        }
    }

    pub(super) fn from_code(code: u32) -> Self {
        match code {
            1 => Self::Go,
            2 => Self::Best,
            3 => Self::Cheer,
            _ => Self::Red,
        }
    }

    /// The notes, as pitch and length.
    fn notes(self) -> &'static [(f32, f32)] {
        match self {
            Self::Red => &[(660.0, 0.2)],
            Self::Go => &[(1_320.0, 0.42)],
            Self::Best => &[(880.0, 0.13), (1_174.7, 0.13), (1_760.0, 0.38)],
            Self::Cheer => &[],
        }
    }
}

/// The start lights' beep and the best-lap chime: short sines with a soft
/// attack and release, one note after another. Rendered sample by sample on
/// the audio thread, so nothing is allocated for them.
#[derive(Default)]
pub(super) struct Beep {
    phase: f32,
    step: f32,
    left: u32,
    length: u32,
    /// Notes still to come after this one.
    queue: &'static [(f32, f32)],
    /// The cheer: samples left, noise state, and two filters that make white
    /// noise sound like many voices far off.
    cheer_left: u32,
    noise: u32,
    low: f32,
    lower: f32,
}

/// How long a cheer lasts, in seconds.
const CHEER: f32 = 2.4;

/// Samples a second, per channel, as the mixer runs.
const RATE: f32 = 44_100.0;

impl Beep {
    pub fn start(&mut self, cue: Cue) {
        if cue == Cue::Cheer {
            self.cheer_left = (CHEER * RATE) as u32;
            return;
        }
        self.queue = cue.notes();
        self.next();
    }

    fn cheer(&mut self) -> f32 {
        if self.cheer_left == 0 {
            return 0.0;
        }
        self.cheer_left -= 1;
        let into = CHEER - self.cheer_left as f32 / RATE;
        // Swells over a third of a second, then falls away.
        let envelope = (into / 0.35).min(1.0) * (self.cheer_left as f32 / RATE / 1.6).min(1.0);
        self.noise = self
            .noise
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        let white = (self.noise >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
        // A band around the voice's range: low-pass, less a lower low-pass.
        self.low += (white - self.low) * 0.18;
        self.lower += (white - self.lower) * 0.02;
        (self.low - self.lower) * envelope * 0.35
    }

    fn next(&mut self) {
        let Some((&(pitch, seconds), rest)) = self.queue.split_first() else {
            return;
        };
        self.queue = rest;
        self.phase = 0.0;
        self.step = pitch / RATE * std::f32::consts::TAU;
        self.length = (seconds * RATE) as u32;
        self.left = self.length;
    }

    pub fn sample(&mut self) -> f32 {
        let crowd = self.cheer();
        if self.left == 0 {
            self.next();
            if self.left == 0 {
                return crowd;
            }
        }
        let done = (self.length - self.left) as f32 / RATE;
        let remaining = self.left as f32 / RATE;
        let envelope = (done / 0.008).min(1.0) * (remaining / 0.06).min(1.0);
        self.left -= 1;
        self.phase = (self.phase + self.step) % std::f32::consts::TAU;
        self.phase.sin() * envelope * 0.16 + crowd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_engine_climbs_through_six_gears_and_idles_when_stopped() {
        assert_eq!(revs(0.0, 22.0), 0.12);
        let low = revs(1.0, 22.0);
        let high_in_first = revs(3.6, 22.0);
        assert!(high_in_first > low, "revs climb within a gear");
        let second = revs(3.8, 22.0);
        assert!(second < high_in_first, "and drop as the next gear is taken");
        assert!(revs(22.0, 22.0) <= 0.95);
        let mut engine = Engine::default();
        let loud: Vec<f32> = (0..44_100).map(|_| engine.sample(0.8, 1.0, 1.0)).collect();
        assert!(loud.iter().all(|s| s.abs() < 0.2));
        assert!(loud.iter().any(|s| s.abs() > 0.02));
        let mut quiet = Engine::default();
        assert!((0..1000).all(|_| quiet.sample(0.8, 1.0, 0.0) == 0.0));
    }

    /// Twelve seconds of the engine pulling from a stop to top speed, lifting
    /// for a corner and pulling again, as `dist/engine.wav`, for a listen:
    /// `cargo test --locked --lib write_the_engine -- --ignored`.
    #[test]
    #[ignore = "writes dist/engine.wav"]
    fn write_the_engine() {
        let mut engine = Engine::default();
        let mut pcm = Vec::new();
        let (top, mut speed) = (22.0f32, 0.0f32);
        for i in 0..(12 * 44_100) {
            let t = i as f32 / 44_100.0;
            let throttle = if (7.0..8.5).contains(&t) { 0.0 } else { 1.0 };
            speed = (speed
                + (throttle * 3.2 * (1.0 - speed / top) - 1.5 * (1.0 - throttle)) / 44_100.0)
                .clamp(0.0, top);
            let sample = engine.sample(revs(speed, top), throttle, 1.0) * 4.0;
            pcm.extend_from_slice(&((sample.clamp(-1.0, 1.0) * 32_000.0) as i16).to_le_bytes());
        }
        let mut wav = Vec::new();
        let chunk = |wav: &mut Vec<u8>, id: &[u8], len: u32| {
            wav.extend_from_slice(id);
            wav.extend_from_slice(&len.to_le_bytes());
        };
        chunk(&mut wav, b"RIFF", 36 + pcm.len() as u32);
        wav.extend_from_slice(b"WAVE");
        chunk(&mut wav, b"fmt ", 16);
        for v in [1u16, 1] {
            wav.extend_from_slice(&v.to_le_bytes());
        }
        wav.extend_from_slice(&44_100u32.to_le_bytes());
        wav.extend_from_slice(&(44_100u32 * 2).to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        chunk(&mut wav, b"data", pcm.len() as u32);
        wav.extend_from_slice(&pcm);
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("dist");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("engine.wav"), wav).unwrap();
    }

    #[test]
    fn a_beep_is_bounded_and_ends_in_silence() {
        for cue in [Cue::Red, Cue::Go, Cue::Best, Cue::Cheer] {
            let mut beep = Beep::default();
            assert_eq!(beep.sample(), 0.0);
            beep.start(cue);
            let samples: Vec<f32> = (0..(3 * 44_100)).map(|_| beep.sample()).collect();
            assert!(samples.iter().all(|s| s.abs() <= 0.2), "{cue:?} too loud");
            assert!(samples.iter().any(|s| s.abs() > 0.02), "{cue:?} silent");
            assert!(
                samples[(26 * 4_410)..].iter().all(|s| *s == 0.0),
                "{cue:?} rang on"
            );
            assert_eq!(Cue::from_code(cue.code()), cue);
        }
    }

    #[test]
    fn tyre_asset_is_a_bounded_loop_with_a_clean_join() {
        assert_eq!(TYRES.len() % 2, 0);
        assert!(TYRES.len() > 2 * 44_100);
        let samples: Vec<f32> = TYRES
            .chunks_exact(2)
            .map(|p| i16::from_le_bytes([p[0], p[1]]) as f32 / 32768.0)
            .collect();
        assert!(samples.iter().all(|x| x.abs() < 0.8));
        assert!((samples[0] - samples[samples.len() - 1]).abs() < 0.05);
    }

    #[test]
    fn rolling_alone_stays_audible_and_fades_when_the_wheels_stop() {
        let mut tyres = Tyres::default();
        let mut energy = 0.0;
        for _ in 0..44_100 {
            let sample = tyres.sample(0.38, 0.0, 0.0);
            assert!(sample.is_finite() && sample.abs() < 0.1);
            energy += sample * sample;
        }
        assert!(
            energy / 44_100.0 > 0.000001,
            "rolling contact must be audible without a slide"
        );
        for _ in 0..22_050 {
            tyres.sample(0.0, 0.0, 0.0);
        }
        assert!(tyres.sample(0.0, 0.0, 0.0).abs() < 1e-6);
    }
}
