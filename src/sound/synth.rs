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

// Prepared by tools/prepare_engine_audio.py; CC BY 3.0, assets/audio/CREDITS.md.
// Each loop contains whole firing cycles, retimed to this common period.
const ENGINE_PERIOD: usize = 200;
const ENGINE_LOOPS: [&[u8]; 3] = [
    include_bytes!("../../assets/audio/engine-low.s16le"),
    include_bytes!("../../assets/audio/engine-mid.s16le"),
    include_bytes!("../../assets/audio/engine-high.s16le"),
];

/// Three recorded engine textures, aligned in pitch and blended as revs climb.
/// Throttle opens the intake's tone and volume; lifting leaves a muted overrun.
/// Playback allocates nothing and does no decoding on the audio thread.
#[derive(Default)]
pub(super) struct Engine {
    cycles: [usize; 3],
    phase: f32,
    rpm: f32,
    load: f32,
    level: f32,
    filtered: f32,
}

impl Engine {
    /// `rpm` and `load` are 0 to 1; `level` is the volume, 0 when silent.
    pub fn sample(&mut self, rpm: f32, load: f32, level: f32) -> f32 {
        // Slewed per sample so a gear change is a quick swoop, not a click.
        self.rpm += (rpm - self.rpm) * 0.0009;
        self.load += (load - self.load) * 0.0006;
        self.level += (level - self.level) * 0.001;
        let pitch = 70.0 + 250.0 * self.rpm;
        let blend = ((self.rpm - 0.12) / (0.95 - 0.12) * 2.0).clamp(0.0, 2.0);
        let mut voice = 0.0;
        for (i, pcm) in ENGINE_LOOPS.iter().enumerate() {
            let length = pcm.len() / 2;
            let position = self.phase * ENGINE_PERIOD as f32;
            let at = self.cycles[i] * ENGINE_PERIOD + position as usize;
            let fraction = position.fract();
            let read = |n: usize| i16::from_le_bytes([pcm[n * 2], pcm[n * 2 + 1]]) as f32 / 32768.0;
            let sample = read(at) * (1.0 - fraction) + read((at + 1) % length) * fraction;
            // The tonal layers are phase-aligned, so linear weights preserve
            // their level without boosting the middle of a transition.
            let weight = (1.0 - (blend - i as f32).abs()).max(0.0);
            voice += sample * weight;
        }
        // One shared firing phase prevents the layers drifting apart during
        // a long straight. Their natural texture still spans different cycles.
        self.phase += pitch / RATE;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            for (i, pcm) in ENGINE_LOOPS.iter().enumerate() {
                self.cycles[i] = (self.cycles[i] + 1) % (pcm.len() / 2 / ENGINE_PERIOD);
            }
        }
        self.filtered += (voice - self.filtered) * (0.16 + 0.44 * self.load);
        self.filtered * (0.4 + 0.6 * self.load) * self.level * 0.25
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

    #[test]
    fn engine_loops_are_level_matched_and_join_without_a_click() {
        for pcm in ENGINE_LOOPS {
            assert_eq!(pcm.len() % (2 * ENGINE_PERIOD), 0);
            let samples: Vec<f32> = pcm
                .chunks_exact(2)
                .map(|p| i16::from_le_bytes([p[0], p[1]]) as f32 / 32768.0)
                .collect();
            assert!(samples.len() > 22_050);
            assert!(samples.iter().all(|s| s.abs() < 0.85));
            // A join on a rising/falling waveform need not have equal sample
            // values; its step must be consistent with the surrounding slope.
            let last = samples.len() - 1;
            let neighbouring_step = (samples[1] - samples[0])
                .abs()
                .max((samples[last] - samples[last - 1]).abs());
            assert!((samples[0] - samples[last]).abs() < neighbouring_step + 0.005);
            let mean = samples.iter().sum::<f32>() / samples.len() as f32;
            let power = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
            assert!(mean.abs() < 0.0001, "DC offset");
            assert!((power.sqrt() - 0.15).abs() < 0.001, "uneven loop volume");
        }
    }

    fn pitch_drift(samples: &[f32], pitch: f32) -> f32 {
        let window = (20.0 * RATE / pitch).round() as usize;
        let (mut previous, mut phase, mut low, mut high) = (None, 0.0_f32, 0.0_f32, 0.0_f32);
        // Track the fundamental against a steady carrier. A clean boundary
        // alone misses the acceleration and pitch reset inside a short loop.
        for start in (0..samples.len() - window).step_by(window / 4) {
            let (mut real, mut imaginary) = (0.0, 0.0);
            for j in 0..window {
                let at = start + j;
                let angle = std::f32::consts::TAU * pitch * at as f32 / RATE;
                let weight = 0.5 - 0.5 * (std::f32::consts::TAU * j as f32 / window as f32).cos();
                real += samples[at] * weight * angle.cos();
                imaginary += samples[at] * weight * angle.sin();
            }
            let measured = imaginary.atan2(real);
            if let Some(previous) = previous {
                let delta: f32 = measured - previous;
                phase += delta.sin().atan2(delta.cos());
                low = low.min(phase);
                high = high.max(phase);
            }
            previous = Some(measured);
        }
        (high - low) / std::f32::consts::TAU
    }

    #[test]
    fn engine_loops_hold_a_steady_note_instead_of_repeating_an_acceleration() {
        for pcm in ENGINE_LOOPS {
            let samples: Vec<f32> = pcm
                .chunks_exact(2)
                .map(|p| i16::from_le_bytes([p[0], p[1]]) as f32 / 32768.0)
                .collect();
            let drift = pitch_drift(&samples, RATE / ENGINE_PERIOD as f32);
            assert!(drift < 0.05, "engine loop drifts by {drift:.3} cycles");
        }
    }

    #[test]
    fn engine_layers_stay_in_tune_at_every_cars_maximum_speed() {
        let cruising = crate::car::Spec::ALL.into_iter().map(|spec| {
            let h = spec.handling();
            let terminal =
                ((h.accel - h.rolling) / (h.accel / h.top_speed.powi(2) + h.drag)).sqrt();
            revs(terminal, h.top_speed)
        });
        for rpm in cruising.chain([0.95]) {
            let mut engine = Engine::default();
            for _ in 0..44_100 {
                engine.sample(rpm, 1.0, 1.0);
            }
            let pitch = 70.0 + 250.0 * engine.rpm;
            let samples: Vec<_> = (0..(10 * 44_100))
                .map(|_| engine.sample(rpm, 1.0, 1.0))
                .collect();
            let drift = pitch_drift(&samples, pitch);
            assert!(
                drift < 0.05,
                "steady {rpm:.3} revs drift by {drift:.3} cycles"
            );
        }
    }

    #[test]
    fn recorded_engine_handles_shifts_overrun_and_mute_without_spikes() {
        let mut engine = Engine::default();
        let mut previous = 0.0;
        for rpm in [0.0, 0.12, 0.95, 0.42, 0.535, 0.75, 1.0] {
            let mut power = [0.0; 2];
            for (i, load) in [0.0, 1.0].into_iter().enumerate() {
                for frame in 0..(4 * 44_100) {
                    let sample = engine.sample(rpm, load, 1.0);
                    assert!(sample.is_finite() && sample.abs() < 0.25);
                    assert!((sample - previous).abs() < 0.1, "audio discontinuity");
                    previous = sample;
                    if frame >= 44_100 {
                        power[i] += sample * sample;
                    }
                }
            }
            assert!(power[0] > 0.01, "idle and overrun must stay audible");
            assert!(
                power[1] > power[0] * 2.0,
                "throttle must open up the engine"
            );
        }
        for _ in 0..22_050 {
            engine.sample(0.8, 1.0, 0.0);
        }
        assert!(engine.sample(0.8, 1.0, 0.0).abs() < 1e-6);
    }

    /// Idle, acceleration, a sustained flat-road maximum, the speed ceiling
    /// and lifting off, as `dist/engine.wav`, for a listen:
    /// `cargo test --locked --lib write_the_engine -- --ignored`.
    #[test]
    #[ignore = "writes dist/engine.wav"]
    fn write_the_engine() {
        let mut engine = Engine::default();
        let mut pcm = Vec::new();
        let h = crate::car::Handling::SHOOTING_BRAKE;
        // On a flat straight: accel * (1 - (v / top)^2) = drag * v^2 + rolling.
        let terminal = ((h.accel - h.rolling) / (h.accel / h.top_speed.powi(2) + h.drag)).sqrt();
        for i in 0..(44 * 44_100) {
            let t = i as f32 / 44_100.0;
            let speed = match t {
                t if t < 2.0 => 0.0,
                t if t < 16.0 => terminal * (t - 2.0) / 14.0,
                t if t < 28.0 => terminal,
                t if t < 32.0 => terminal + (h.top_speed - terminal) * (t - 28.0) / 4.0,
                t if t < 40.0 => h.top_speed,
                _ => h.top_speed * (1.0 - (t - 40.0) / 8.0),
            };
            let throttle = if (2.0..40.0).contains(&t) { 1.0 } else { 0.0 };
            let sample = engine.sample(revs(speed, h.top_speed), throttle, 1.0) * 4.0;
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
