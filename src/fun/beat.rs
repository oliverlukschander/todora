//! The beat, for everything that flashes to it.
//!
//! When the techno is playing the beat is *its* beat: the audio thread counts
//! sixteenths as it renders them and publishes where it has got to. That number
//! arrives in lumps — the audio thread works in buffers, and the game reads it
//! once a frame — so used directly it would make the lights stutter. Instead the
//! game keeps its own clock at the tempo it was told and pulls it toward the
//! audio's a little every frame, which is a phase-locked loop in three lines and
//! is smooth to the eye and right to the ear.
//!
//! With no techno there is still a beat, ticked over from real time at the same
//! tempo, so the neon pulses whether or not anyone is listening. It runs on the
//! real clock, not the game's: the title screen and the pause have a pulse too.

use bevy::prelude::*;

/// The tempo, in beats a minute. Shared with the sequencer that plays it.
pub(crate) const BPM: f32 = 140.0;

/// Where in the music the game is.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct Beat {
    /// Position in beats since the start.
    pos: f64,
    /// The last position the audio gave, and for how long it has given that one:
    /// a sequencer that has stopped moving is not one to keep time by.
    heard: f64,
    still: f32,
}

/// How much faster than the tempo the clock may be made to run to catch up with
/// the music: a quarter. The beat is what the lights flash on, and a clock that
/// raced at four times the tempo to close a gap would flash them at four times
/// the rate, which is more than anybody should have to look at.
const CATCH_UP: f64 = 0.25;
/// Seconds without the audio's position moving after which it is taken not to be.
const STALLED: f32 = 0.25;

impl Beat {
    /// Where in the beat, 0 to 1.
    pub(crate) fn phase(&self) -> f32 {
        self.pos.rem_euclid(1.0) as f32
    }

    /// Whole beats so far.
    pub(crate) fn count(&self) -> u64 {
        self.pos.max(0.0) as u64
    }

    /// Where in the bar of four, 0 to 1.
    pub(crate) fn bar(&self) -> f32 {
        (self.pos.rem_euclid(4.0) / 4.0) as f32
    }

    /// One on the beat, falling away quickly: what a kick drum looks like.
    pub(crate) fn kick(&self) -> f32 {
        (1.0 - self.phase()).powi(5)
    }

    /// The sixteenth of the bar, 0 to 15.
    #[cfg(test)]
    pub(crate) fn sixteenth(&self) -> u32 {
        (self.pos.rem_euclid(4.0) * 4.0) as u32 % 16
    }

    /// Move the clock on by `dt` seconds of real time.
    pub(crate) fn advance(&mut self, dt: f32) {
        self.pos += f64::from(dt) * f64::from(BPM) / 60.0;
    }

    /// Pull toward where the audio says it is, in beats, over `dt` seconds.
    ///
    /// The pull is a fraction of the disagreement, and never enough to run the
    /// clock at more than a quarter over the tempo (see [`CATCH_UP`]). A
    /// disagreement of more than a beat — the audio thread started long before
    /// the game looked, or stalled and has come back, or the clock was asleep —
    /// is not closed at all but stepped over: a step is at most one flash, and a
    /// chase would be several. And if the audio has stopped saying anything new
    /// the clock is left to keep the tempo on its own.
    pub(crate) fn lock(&mut self, audio: f64, dt: f32) {
        if audio == self.heard {
            self.still += dt;
        } else {
            self.heard = audio;
            self.still = 0.0;
        }
        if self.still > STALLED {
            return;
        }
        let error = audio - self.pos;
        if error.abs() > 1.0 {
            self.pos = audio;
        } else {
            let most = f64::from(dt) * f64::from(BPM) / 60.0 * CATCH_UP;
            self.pos += (error * 0.12).clamp(-most, most);
        }
    }
}

pub(super) fn plugin(app: &mut App) {
    app.init_resource::<Beat>().add_systems(PreUpdate, tick);
}

fn tick(
    time: Res<Time<Real>>,
    fun: Res<super::Fun>,
    sound: Res<crate::sound::Sound>,
    mut beat: ResMut<Beat>,
) {
    beat.advance(time.delta_secs());
    if fun.techno
        && let Some(audio) = sound.beat()
    {
        beat.lock(audio, time.delta_secs());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_keeps_the_tempo() {
        let mut beat = Beat::default();
        for _ in 0..3_600 {
            beat.advance(1.0 / 60.0);
        }
        // A minute at 140 beats a minute.
        assert!((beat.count() as i64 - 140).abs() <= 1, "{}", beat.count());
        assert!(beat.phase() >= 0.0 && beat.phase() < 1.0);
    }

    #[test]
    fn a_kick_is_loudest_on_the_beat_and_gone_by_the_next() {
        let mut beat = Beat::default();
        let on = beat.kick();
        beat.advance(60.0 / BPM * 0.5);
        let half = beat.kick();
        beat.advance(60.0 / BPM * 0.49);
        assert_eq!(on, 1.0);
        assert!(half < 0.05, "{half}");
        assert!(beat.kick() < 0.001);
    }

    const FRAME: f32 = 1.0 / 60.0;

    #[test]
    fn locking_to_the_audio_is_smooth_and_finds_it() {
        let mut beat = Beat {
            pos: 10.0,
            ..default()
        };
        // The audio is a fifth of a beat ahead, and going at the tempo: close the
        // gap without a jump.
        let tempo = f64::from(BPM) / 60.0 * f64::from(FRAME);
        let mut audio = 10.2;
        for _ in 0..200 {
            beat.advance(FRAME);
            audio += tempo;
            beat.lock(audio, FRAME);
        }
        assert!((beat.pos - audio).abs() < 0.001, "{} for {audio}", beat.pos);
        let mut one = Beat {
            pos: 5.0,
            ..default()
        };
        one.lock(5.2, FRAME);
        assert!(
            one.pos > 5.0 && one.pos < 5.05,
            "one step is a fraction of it"
        );
        // A long way out is simply adopted.
        let mut lost = Beat::default();
        lost.lock(100.0, FRAME);
        assert_eq!(lost.pos, 100.0);
    }

    /// However the audio and the clock disagree, the clock never runs more than a
    /// quarter faster than the tempo, so that the lights never flash faster than
    /// the music does; the one time it does not run at all is when it steps over a
    /// gap of more than a beat, once.
    #[test]
    fn the_clock_never_races_to_catch_the_music_up() {
        let tempo = f64::from(BPM) / 60.0 * f64::from(FRAME);
        for gap in [0.1, 0.4, 0.9, 1.0] {
            let mut beat = Beat::default();
            let mut audio = 50.0;
            beat.pos = audio - gap;
            let mut worst = 0.0f64;
            for _ in 0..600 {
                let before = beat.pos;
                beat.advance(FRAME);
                audio += tempo;
                beat.lock(audio, FRAME);
                worst = worst.max(beat.pos - before);
            }
            assert!(
                worst <= tempo * 1.25 + 1e-9,
                "a gap of {gap} made it run at {:.2} times the tempo",
                worst / tempo
            );
            assert!((audio - beat.pos).abs() < 0.02, "and it does arrive: {gap}");
        }
        // A bigger one is stepped over, once, and then it is in step.
        let mut beat = Beat::default();
        let mut audio = 90.0;
        let mut steps = 0;
        for _ in 0..600 {
            let before = beat.pos;
            beat.advance(FRAME);
            audio += tempo;
            beat.lock(audio, FRAME);
            if beat.pos - before > tempo * 1.25 {
                steps += 1;
            }
        }
        assert_eq!(steps, 1, "one step over a big gap, and no chase");
    }

    /// When the audio stops (the device has gone, the stream is not being pulled)
    /// its last position is not the time, and the lights keep on with the tempo.
    #[test]
    fn a_stalled_audio_clock_is_left_alone() {
        let mut beat = Beat::default();
        for _ in 0..30 {
            beat.advance(FRAME);
            beat.lock(0.5, FRAME);
        }
        let start = beat.pos;
        for _ in 0..600 {
            beat.advance(FRAME);
            beat.lock(0.5, FRAME);
        }
        // Ten seconds at 140 beats a minute, nowhere near the frozen half beat.
        assert!(
            beat.pos - start > 20.0,
            "the clock followed a frozen audio to {}",
            beat.pos
        );
    }

    #[test]
    fn the_sixteenths_count_round_the_bar() {
        let mut beat = Beat::default();
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..960 {
            seen.insert(beat.sixteenth());
            beat.advance(1.0 / 240.0);
        }
        assert_eq!(seen.len(), 16);
        assert_eq!(*seen.iter().max().unwrap(), 15);
    }
}
