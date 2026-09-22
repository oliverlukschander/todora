//! Two-player practice protocol. GameKit transports bytes; this owns the rules.
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const VERSION: &str = concat!(env!("CARGO_PKG_VERSION"), "/practice-1");
pub const MAX_PACKET: usize = 2048;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub circuit: String,
    pub mode: usize,
    pub fingerprint: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Packet {
    Hello { version: String, config: Config },
    Ready,
    Ping { id: u32 },
    Pong { id: u32, remote: f64 },
    Start { at: f64, offset: f64 },
    Ack,
    State(Snapshot),
    Leave,
}

impl Packet {
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > MAX_PACKET {
            return None;
        }
        let packet: Self = serde_json::from_slice(bytes).ok()?;
        let valid = match &packet {
            Self::Hello { version, config } => {
                version.len() <= 64 && config.circuit.len() <= 64 && config.mode < 3
            }
            Self::Pong { remote, .. } => valid_time(*remote),
            Self::Start { at, offset } => {
                valid_time(*at) && offset.is_finite() && offset.abs() < 1e9
            }
            Self::State(s) => s.valid(),
            _ => true,
        };
        valid.then_some(packet)
    }
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("locally generated multiplayer packet")
    }
    pub fn reliable(&self) -> bool {
        !matches!(self, Self::State(_))
    }
}

fn valid_time(time: f64) -> bool {
    time.is_finite() && (0.0..1e9).contains(&time)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub seq: u64,
    pub reset: u64,
    pub time: f64,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub velocity: [f32; 3],
    pub lap: f32,
    pub invalid: bool,
    pub paused: bool,
    pub laps: u32,
    pub best: Option<f32>,
}

impl Snapshot {
    fn valid(&self) -> bool {
        valid_time(self.time)
            && self
                .position
                .iter()
                .all(|x| x.is_finite() && x.abs() < 100_000.0)
            && self
                .velocity
                .iter()
                .all(|x| x.is_finite() && x.abs() <= 200.0)
            && self.rotation.iter().all(|x| x.is_finite())
            && (Quat::from_array(self.rotation).length_squared() - 1.0).abs() < 0.02
            && self.lap.is_finite()
            && (0.0..86400.0).contains(&self.lap)
            && self
                .best
                .is_none_or(|x| x.is_finite() && (0.0..86400.0).contains(&x) && x > 0.0)
            && self.laps < 1_000_000
    }
}

#[derive(Debug, PartialEq, Eq, Default)]
pub enum Phase {
    #[default]
    Idle,
    Finding,
    Loading,
    Waiting,
    Syncing,
    Countdown,
    Driving,
}

#[derive(Debug)]
pub enum Action {
    Send(Packet),
    Load(Config),
    Reset,
    End,
}

#[derive(Resource, Default)]
pub struct Session {
    pub phase: Phase,
    pub host: bool,
    pub peer: String,
    pub config: Option<Config>,
    pub status: String,
    pub start_at: f64,
    pub local_laps: u32,
    pub local_best: Option<f32>,
    pub remote: Remote,
    last_heard: f64,
    phase_since: f64,
    hello: bool,
    peer_ready: bool,
    local_ready: bool,
    acknowledged: bool,
    ping: Option<(u32, f64)>,
}

impl Session {
    pub fn active(&self) -> bool {
        self.phase != Phase::Idle
    }
    pub fn driving(&self) -> bool {
        self.phase == Phase::Driving
    }
    pub fn blocks_drive(&self) -> bool {
        self.active() && !self.driving()
    }

    pub fn begin(&mut self, now: f64) {
        *self = Self {
            phase: Phase::Finding,
            phase_since: now,
            last_heard: now,
            status: "Finding another driver…  M to cancel".into(),
            ..default()
        };
    }

    pub fn connected(&mut self, host: bool, peer: String, config: Config, now: f64) -> Vec<Action> {
        if self.phase != Phase::Finding {
            return vec![];
        }
        self.host = host;
        self.peer = peer.chars().filter(|c| !c.is_control()).take(40).collect();
        self.config = Some(config.clone());
        self.phase = Phase::Loading;
        self.phase_since = now;
        self.last_heard = now;
        self.status = "Agreeing on a circuit…".into();
        vec![Action::Send(Packet::Hello {
            version: VERSION.into(),
            config,
        })]
    }

    pub fn end(&mut self, reason: impl Into<String>) -> Vec<Action> {
        self.phase = Phase::Idle;
        self.remote = Remote::default();
        self.status = reason.into();
        vec![Action::End]
    }

    pub fn receive(&mut self, packet: Packet, now: f64) -> Vec<Action> {
        if !self.active() || self.phase == Phase::Finding {
            return vec![];
        }
        self.last_heard = now;
        match packet {
            Packet::Hello { version, config } if !self.hello && self.phase == Phase::Loading => {
                if version != VERSION {
                    return self.end("Both drivers need the same Todora prototype version.");
                }
                if !crate::track::all_circuits()
                    .iter()
                    .any(|c| c.id == config.circuit)
                    || config.mode >= 3
                {
                    return self.end("The other driver selected an unknown circuit or mode.");
                }
                self.hello = true;
                if !self.host {
                    self.config = Some(config);
                }
                self.status = "Loading the shared circuit…".into();
                vec![Action::Load(self.config.clone().unwrap()), Action::Reset]
            }
            Packet::Ready
                if self.hello && matches!(self.phase, Phase::Loading | Phase::Waiting) =>
            {
                self.peer_ready = true;
                self.maybe_sync(now)
            }
            Packet::Ping { id }
                if !self.host && self.local_ready && self.phase == Phase::Waiting =>
            {
                vec![Action::Send(Packet::Pong { id, remote: now })]
            }
            Packet::Pong { id, remote }
                if self.host
                    && self.phase == Phase::Syncing
                    && self.ping.is_some_and(|p| p.0 == id) =>
            {
                let sent = self.ping.unwrap().1;
                let rtt = now - sent;
                if !(0.0..=2.0).contains(&rtt) {
                    return self.end("Connection too slow to start together. Please retry.");
                }
                let offset = remote - (sent + now) * 0.5;
                self.start_at = now + 3.0;
                self.phase = Phase::Countdown;
                vec![Action::Send(Packet::Start {
                    at: self.start_at,
                    offset,
                })]
            }
            Packet::Start { at, offset }
                if !self.host && self.phase == Phase::Waiting && self.local_ready =>
            {
                let local_at = at + offset;
                if !(1.0..=5.0).contains(&(local_at - now)) {
                    return self.end("Start arrived too late. Please retry.");
                }
                self.start_at = local_at;
                self.phase = Phase::Countdown;
                self.acknowledged = true;
                vec![Action::Send(Packet::Ack)]
            }
            Packet::Ack if self.host && self.phase == Phase::Countdown => {
                self.acknowledged = true;
                vec![]
            }
            Packet::State(snapshot) if self.driving() => {
                self.remote.push(snapshot, now);
                vec![]
            }
            Packet::Leave => self.end("The other driver left. Back to solo driving."),
            _ => vec![], // Late/duplicate control packets cannot restart a session.
        }
    }

    // Called after the actual track switch and reset systems have run.
    pub fn loaded(&mut self, current: &Config, now: f64) -> Vec<Action> {
        if self.phase != Phase::Loading || !self.hello {
            return vec![];
        }
        if let Some(expected) = &self.config {
            if expected.circuit != current.circuit || expected.mode != current.mode {
                return vec![];
            }
            if expected.fingerprint != current.fingerprint {
                return self.end("Circuit data differs. Both drivers need the same build.");
            }
        }
        self.local_ready = true;
        self.phase = Phase::Waiting;
        self.status = "Waiting for the other driver…".into();
        let mut actions = vec![Action::Send(Packet::Ready)];
        actions.extend(self.maybe_sync(now));
        actions
    }

    fn maybe_sync(&mut self, now: f64) -> Vec<Action> {
        if self.host && self.peer_ready && self.local_ready && self.phase == Phase::Waiting {
            self.phase = Phase::Syncing;
            self.ping = Some((1, now));
            vec![Action::Send(Packet::Ping { id: 1 })]
        } else {
            vec![]
        }
    }

    pub fn tick(&mut self, now: f64) -> Vec<Action> {
        if !self.active() {
            return vec![];
        }
        if self.phase == Phase::Finding {
            if now - self.phase_since > 180.0 {
                return self.end("Matchmaking timed out. Please retry.");
            }
        } else if now - self.last_heard > if self.driving() { 15.0 } else { 60.0 } {
            return self.end("Connection lost. Back to solo driving.");
        }
        if self.phase == Phase::Countdown && now >= self.start_at {
            if !self.acknowledged {
                return self.end("The other driver did not confirm the start.");
            }
            self.phase = Phase::Driving;
            self.status = "Shared practice · no car contact · M to leave".into();
        }
        vec![]
    }
}

#[derive(Default)]
pub struct Remote {
    samples: VecDeque<Snapshot>,
    pub received_at: f64,
}

impl Remote {
    pub fn latest(&self) -> Option<&Snapshot> {
        self.samples.back()
    }
    pub fn push(&mut self, sample: Snapshot, now: f64) {
        if !sample.valid() {
            return;
        }
        if let Some(last) = self.samples.back() {
            if sample.seq <= last.seq || sample.time <= last.time || sample.reset < last.reset {
                return;
            }
            if sample.reset != last.reset
                || Vec3::from_array(sample.position).distance(Vec3::from_array(last.position))
                    > 12.0
            {
                self.samples.clear();
            }
        }
        self.received_at = now;
        self.samples.push_back(sample);
        while self.samples.len() > 32 {
            self.samples.pop_front();
        }
    }
    pub fn pose(&self, now: f64) -> Option<(Vec3, Quat)> {
        let last = self.samples.back()?;
        let age = now - self.received_at;
        if !(0.0..1.5).contains(&age) {
            return None;
        }
        // Render 100 ms behind the newest sender timestamp. Never project a
        // remote car far into the future when packets stop arriving.
        let target = last.time - 0.1 + age.min(0.2);
        let pose = |s: &Snapshot| {
            (
                Vec3::from_array(s.position),
                Quat::from_array(s.rotation).normalize(),
            )
        };
        for (a, b) in self.samples.iter().zip(self.samples.iter().skip(1)) {
            if a.time <= target && target <= b.time {
                let t = ((target - a.time) / (b.time - a.time)) as f32;
                let (ap, ar) = pose(a);
                let (bp, br) = pose(b);
                return Some((ap.lerp(bp, t), ar.slerp(br, t)));
            }
        }
        if target < self.samples.front()?.time {
            return Some(pose(self.samples.front()?));
        }
        let (p, r) = pose(last);
        Some((
            p + Vec3::from_array(last.velocity) * (target - last.time).clamp(0.0, 0.1) as f32,
            r,
        ))
    }
}
