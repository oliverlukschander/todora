use super::{Engine, Signal, Tyres, sfx::Bank, synth::Beep, techno::Techno};
use bevy::{
    audio::{ChannelCount, SampleRate, Source},
    prelude::*,
};
use rodio::{Decoder, source::UniformSourceIterator};
use std::{
    io::{self, Read, Seek, SeekFrom},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::Duration,
};

// CLIamp's own Omarchy station; checked against stream.pls on 2026-09-19.
const STATION: &str = "https://radio.cliamp.stream/omarchy/stream";
const CHUNK: usize = 4096;
type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Asset, TypePath)]
pub(super) struct Soundtrack(pub(super) Arc<Signal>);

impl Decodable for Soundtrack {
    type Decoder = Mixer;

    fn decoder(&self) -> Mixer {
        // Less than a second of PCM. Network reads and decoding never run in
        // the game loop or audio callback; starvation simply produces silence.
        let (send, receive) = mpsc::sync_channel(16);
        let stop = Arc::new(AtomicBool::new(false));
        let signal = self.0.clone();
        let done = stop.clone();
        if let Err(error) = thread::Builder::new()
            .name("omarchy-radio".into())
            .spawn(move || {
                listen(signal, done, send);
            })
        {
            warn!("Could not start Omarchy radio: {error}");
        }
        Mixer {
            signal: self.0.clone(),
            stop,
            receive,
            chunk: Vec::new().into_iter(),
            tyres: Tyres::default(),
            beep: Beep::default(),
            engine: Engine::default(),
            sfx: Bank::following(&self.0.sfx),
            cluck_left: 0,
            cluck_seed: 0x2545_F491,
            techno: Techno::default(),
            techno_sample: 0.0,
            report_in: 0,
            beeps: self.0.beep.load(Relaxed),
            vehicle_sample: 0.0,
            right: false,
            music: 0.0,
        }
    }
}

pub(super) struct Mixer {
    signal: Arc<Signal>,
    stop: Arc<AtomicBool>,
    receive: Receiver<Vec<f32>>,
    chunk: std::vec::IntoIter<f32>,
    tyres: Tyres,
    beep: Beep,
    engine: Engine,
    /// The silly noises, and the next cluck of a chicken's engine.
    sfx: Bank,
    cluck_left: u32,
    cluck_seed: u32,
    /// The techno, what it made this sample, and samples till its clock is next
    /// reported to the game.
    techno: Techno,
    techno_sample: f32,
    report_in: u32,
    /// The last beep the game asked for, so each is started once.
    beeps: u32,
    vehicle_sample: f32,
    right: bool,
    music: f32,
}

impl Iterator for Mixer {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.signal.shutting_down.load(Relaxed) || self.stop.load(Relaxed) {
            return None;
        }
        if !self.right {
            if self.chunk.len() == 0
                && let Ok(chunk) = self.receive.try_recv()
            {
                self.chunk = chunk.into_iter();
            }
            self.vehicle_sample = self.tyres.sample(
                f32::from_bits(self.signal.rolling.load(Relaxed)),
                f32::from_bits(self.signal.scrub.load(Relaxed)),
                f32::from_bits(self.signal.squeal.load(Relaxed)),
            );
            let beeps = self.signal.beep.load(Relaxed);
            if beeps != self.beeps {
                self.beeps = beeps;
                self.beep.start(super::synth::Cue::from_code(beeps & 3));
            }
            self.vehicle_sample += self.beep.sample();
            self.cluck();
            self.vehicle_sample += self.sfx.sample(&self.signal.sfx);
            self.vehicle_sample += self.engine.sample(
                f32::from_bits(self.signal.engine_rpm.load(Relaxed)),
                f32::from_bits(self.signal.engine_load.load(Relaxed)),
                f32::from_bits(self.signal.engine_level.load(Relaxed)),
            );
            self.vehicle_sample *= 1.0 - f32::from_bits(self.signal.effects_cut.load(Relaxed));
            self.techno_sample = self.techno.sample(
                f32::from_bits(self.signal.techno_energy.load(Relaxed)),
                f32::from_bits(self.signal.techno_level.load(Relaxed)),
            );
            // Tell the game where the beat is, often enough to steer a light by
            // and seldom enough not to matter. Never zero once begun: zero means
            // "no clock" to the other side.
            if self.report_in == 0 {
                self.report_in = 256;
                self.signal
                    .beat
                    .store(self.techno.position().max(1), Relaxed);
            }
            self.report_in -= 1;
            self.music += (f32::from_bits(self.signal.music.load(Relaxed)) - self.music) * 0.001;
        }
        self.right = !self.right;
        let music = self.chunk.next().unwrap_or(0.0);
        Some(ceiling(
            music * self.music + self.vehicle_sample + self.techno_sample,
        ))
    }
}

/// The mix's own soft ceiling. Anything below [`KNEE`] goes through as it is;
/// what is above it is squeezed into the room that is left under full scale, so
/// that a great many things at once is louder and not clipped, which is what a
/// hard clamp made of it. Never quite full scale, and never anything but a
/// number.
fn ceiling(x: f32) -> f32 {
    const KNEE: f32 = 0.8;
    if !x.is_finite() {
        return 0.0;
    }
    let over = x.abs() - KNEE;
    if over <= 0.0 {
        x
    } else {
        x.signum() * (KNEE + (1.0 - KNEE) * (over / (1.0 - KNEE)).tanh())
    }
}

impl Mixer {
    /// The engine of a chicken: a cluck at the rate the game says, a little
    /// higher or lower each time, and never quite evenly spaced.
    fn cluck(&mut self) {
        let rate = f32::from_bits(self.signal.cluck_rate.load(Relaxed));
        if rate <= 0.05 {
            self.cluck_left = 0;
            return;
        }
        if self.cluck_left == 0 {
            self.cluck_seed = self
                .cluck_seed
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223);
            let a = (self.cluck_seed >> 8) as f32 / (1u32 << 24) as f32;
            self.cluck_seed = self
                .cluck_seed
                .wrapping_mul(1_664_525)
                .wrapping_add(1_013_904_223);
            let b = (self.cluck_seed >> 8) as f32 / (1u32 << 24) as f32;
            let level = f32::from_bits(self.signal.cluck_level.load(Relaxed));
            self.sfx
                .start(super::sfx::Kind::Cluck, 0.8 + 0.55 * a, level);
            self.cluck_left = (super::sfx::RATE / rate * (0.55 + 0.9 * b)) as u32;
        }
        self.cluck_left = self.cluck_left.saturating_sub(1);
    }
}

impl Source for Mixer {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        ChannelCount::new(2).unwrap()
    }
    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(44_100).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Drop for Mixer {
    fn drop(&mut self) {
        self.stop.store(true, Relaxed);
    }
}

fn listen(signal: Arc<Signal>, stop: Arc<AtomicBool>, send: SyncSender<Vec<f32>>) {
    // A timeout per socket read, not a deadline for the endless response body.
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(8))
        .timeout_write(Duration::from_secs(8))
        .build();
    while !stop.load(Relaxed) && !signal.shutting_down.load(Relaxed) {
        if !signal.enabled.load(Relaxed) {
            thread::sleep(Duration::from_millis(100));
            continue;
        }
        signal.status.store(0, Relaxed);
        if let Err(error) = stream(&agent, &signal, &stop, &send) {
            if stop.load(Relaxed) || signal.shutting_down.load(Relaxed) {
                break;
            }
            warn!("Omarchy radio unavailable; retrying: {error}");
            signal.status.store(2, Relaxed);
            for _ in 0..50 {
                if stop.load(Relaxed)
                    || signal.shutting_down.load(Relaxed)
                    || !signal.enabled.load(Relaxed)
                {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn stream(
    agent: &ureq::Agent,
    signal: &Signal,
    stop: &AtomicBool,
    send: &SyncSender<Vec<f32>>,
) -> Result<(), Error> {
    let response = agent.get(STATION).set("Icy-MetaData", "0").call()?;
    let reader = LiveReader(response.into_reader());
    let decoder = Decoder::builder()
        .with_data(reader)
        .with_hint("mp3")
        .with_seekable(false)
        .build()?;
    let mut samples = UniformSourceIterator::new(
        decoder,
        ChannelCount::new(2).unwrap(),
        SampleRate::new(44_100).unwrap(),
    );
    while !stop.load(Relaxed) && !signal.shutting_down.load(Relaxed) && signal.enabled.load(Relaxed)
    {
        let chunk: Vec<_> = samples.by_ref().take(CHUNK).collect();
        if chunk.is_empty() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "radio stream ended").into());
        }
        send.send(chunk)?;
        signal.status.store(1, Relaxed);
    }
    Ok(())
}

/// Rodio requires Seek even for explicitly non-seekable live sources.
struct LiveReader(Box<dyn Read + Send + Sync>);

impl Read for LiveReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl Seek for LiveReader {
    fn seek(&mut self, _: SeekFrom) -> io::Result<u64> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "live radio cannot seek",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ceiling_is_the_identity_until_the_knee_and_never_reaches_full_scale() {
        for x in [-0.8, -0.3, 0.0, 0.05, 0.5, 0.8] {
            assert_eq!(ceiling(x), x, "{x} was touched");
        }
        // Rising all the way, and getting slower to: at first not quite full
        // scale, and in the end as near as a float can say.
        let mut before = 0.8;
        for i in 1..=200 {
            let x = 0.8 + i as f32 * 0.05;
            let y = ceiling(x);
            assert!(y >= before && y <= 1.0, "{x} came to {y}");
            if x < 1.5 {
                assert!(y > before && y < 1.0, "{x} came to {y}");
            }
            assert_eq!(ceiling(-x), -y, "and it is the same either way up");
            before = y;
        }
        assert!(ceiling(50.0) <= 1.0);
        assert_eq!(ceiling(f32::NAN), 0.0);
        assert_eq!(ceiling(f32::INFINITY), 0.0);
    }

    #[test]
    fn a_missing_radio_never_stops_the_tyres_or_blocks_playback() {
        let (send, receive) = mpsc::sync_channel(1);
        let signal = Arc::new(Signal::default());
        signal.scrub.store(0.28_f32.to_bits(), Relaxed);
        signal.squeal.store(0.30_f32.to_bits(), Relaxed);
        let stop = Arc::new(AtomicBool::new(false));
        let mut mixer = Mixer {
            signal,
            stop: stop.clone(),
            receive,
            chunk: Vec::new().into_iter(),
            tyres: Tyres::default(),
            beep: Beep::default(),
            engine: Engine::default(),
            sfx: Bank::default(),
            cluck_left: 0,
            cluck_seed: 1,
            techno: Techno::default(),
            techno_sample: 0.0,
            report_in: 0,
            beeps: 0,
            vehicle_sample: 0.0,
            right: false,
            music: 0.0,
        };
        // An open but empty queue represents a stalled connection.
        assert!(mixer.by_ref().take(8820).any(|sample| sample.abs() > 0.05));
        drop(send);
        assert!(mixer.by_ref().take(8820).any(|sample| sample.abs() > 0.05));
        mixer.chunk = vec![0.5; CHUNK].into_iter();
        mixer.signal.shutdown();
        assert_eq!(mixer.next(), None, "shutdown must discard buffered sound");
        drop(mixer);
        assert!(stop.load(Relaxed));
    }

    #[test]
    fn shutdown_worker_exits_without_opening_a_connection() {
        let signal = Arc::new(Signal::default());
        signal.enabled.store(true, Relaxed);
        signal.shutdown();
        let (send, receive) = mpsc::sync_channel(1);
        listen(signal, Arc::new(AtomicBool::new(false)), send);
        assert!(matches!(
            receive.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
    }

    #[test]
    #[ignore = "requires the live Omarchy station"]
    fn omarchy_stream_decodes() {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(8))
            .timeout_read(Duration::from_secs(8))
            .build();
        let reader = LiveReader(agent.get(STATION).call().unwrap().into_reader());
        let decoder = Decoder::builder()
            .with_data(reader)
            .with_hint("mp3")
            .with_seekable(false)
            .build()
            .unwrap();
        let count = decoder.sample_rate().get() as usize * decoder.channels().get() as usize * 12;
        let samples: Vec<_> = decoder.take(count).collect();
        assert_eq!(
            samples.len(),
            count,
            "live playback must outlast the per-read timeout"
        );
        assert!(samples.iter().all(|sample| sample.is_finite()));
        assert!(samples.iter().any(|sample| sample.abs() > 0.01));
    }
}
