// Prepared by tools/prepare_tyre_audio.py. Attribution travels in assets/audio.
const TYRES: &[u8] = include_bytes!("../../assets/audio/tyres.s16le");

#[derive(Default)]
pub(super) struct Tyres {
    position: f32,
    filtered: f32,
    rolling_low: f32,
    rolling_filtered: f32,
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
        // Low-pass the recording twice for quiet tyre contact underneath the
        // grip warning. No motor oscillators and no generated white-noise bed.
        self.rolling_low += (sample - self.rolling_low) * 0.05;
        self.rolling_filtered += (self.rolling_low - self.rolling_filtered) * 0.05;
        self.rolling_filtered * self.rolling + self.filtered * (self.scrub + self.squeal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
