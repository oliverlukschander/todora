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

/// The start lights' beep: a short sine with a soft attack and release, low
/// for a red light and an octave up for GO. Rendered sample by sample on the
/// audio thread, so nothing is allocated for it.
#[derive(Default)]
pub(super) struct Beep {
    phase: f32,
    step: f32,
    left: u32,
    length: u32,
}

/// Samples a second, per channel, as the mixer runs.
const RATE: f32 = 44_100.0;

impl Beep {
    pub fn start(&mut self, go: bool) {
        let (pitch, seconds) = if go { (1_320.0, 0.42) } else { (660.0, 0.2) };
        self.phase = 0.0;
        self.step = pitch / RATE * std::f32::consts::TAU;
        self.length = (seconds * RATE) as u32;
        self.left = self.length;
    }

    pub fn sample(&mut self) -> f32 {
        if self.left == 0 {
            return 0.0;
        }
        let done = (self.length - self.left) as f32 / RATE;
        let remaining = self.left as f32 / RATE;
        let envelope = (done / 0.008).min(1.0) * (remaining / 0.06).min(1.0);
        self.left -= 1;
        self.phase = (self.phase + self.step) % std::f32::consts::TAU;
        self.phase.sin() * envelope * 0.16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_beep_is_bounded_and_ends_in_silence() {
        for go in [false, true] {
            let mut beep = Beep::default();
            assert_eq!(beep.sample(), 0.0);
            beep.start(go);
            let samples: Vec<f32> = (0..30_000).map(|_| beep.sample()).collect();
            assert!(samples.iter().all(|s| s.abs() <= 0.16));
            assert!(samples.iter().any(|s| s.abs() > 0.1));
            assert!(samples[25_000..].iter().all(|s| *s == 0.0));
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
