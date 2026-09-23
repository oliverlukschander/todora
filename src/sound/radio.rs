use super::{Signal, Tyres, synth::Beep};
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
                self.beep.start(beeps & 1 == 1);
            }
            self.vehicle_sample += self.beep.sample();
            self.music += (f32::from_bits(self.signal.music.load(Relaxed)) - self.music) * 0.001;
        }
        self.right = !self.right;
        let music = self.chunk.next().unwrap_or(0.0);
        Some((music * self.music + self.vehicle_sample).clamp(-1.0, 1.0))
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
