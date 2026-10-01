//! The other driver, as the receiving Mac draws them.
//!
//! Snapshots arrive late, unevenly, out of order or not at all, each stamped
//! with the sender's clock. The car is drawn from those stamps, not from when
//! the packets happened to land. A playhead follows the sender's clock a little
//! behind, by just enough that the next snapshot is nearly always in hand, and
//! the car is interpolated along it. What still goes wrong is covered by
//! predicting ahead, and any jump that causes is faded out rather than shown.
use super::session::Snapshot;
use bevy::prelude::*;
use std::collections::VecDeque;

/// Snapshots kept, a second's worth.
const KEEP: usize = 64;
/// Packets the link statistics look back over, about five seconds.
const WINDOW: usize = 450;
/// Share of packets the playhead waits for. The slowest few are predicted past.
const COVER: f64 = 0.95;
/// How much of a snapshot's spacing the playhead keeps in hand ahead of it.
const AHEAD: f64 = 0.5;
/// Extra seconds behind the sender's clock for the estimate being a little off.
const SLACK: f64 = 0.005;
/// Extra lead while the statistics are young, fading over `WARM` packets.
const PRIOR: f64 = 0.06;
const WARM: usize = 60;
/// The most the playhead will ever trail the fastest packets.
const MAX_LEAD: f64 = 0.5;
/// The playhead speeds up or slows down to take up a change in the link, by an
/// error's worth every `CATCH_UP` seconds and never more than `SLEW` of its
/// speed, so the car never seems to lurch.
const SLEW: f64 = 0.1;
const CATCH_UP: f64 = 3.0;
/// Beyond this much error the playhead is put back where it belongs at once.
const RESYNC: f64 = 0.5;
/// Silence after which the car is no longer drawn, and the gap between two
/// snapshots beyond which they are not the same drive.
pub const SILENCE: f64 = 1.5;
/// A step further than this, plus what the car's speed explains, is a teleport.
const TELEPORT: f32 = 12.0;
/// How long a correction takes to fade, and the most that is ever faded.
const BLEND: f64 = 0.2;
const BLEND_MAX: f32 = 8.0;
/// Prediction past the newest snapshot goes at full speed for `FREE` seconds,
/// then eases to a stop over `TAIL` more, which is a car nobody is steering.
const FREE: f64 = 0.4;
const TAIL: f64 = 0.3;
/// Speed change, in m/s², that prediction carries on with, and for how long.
const ACCEL_MAX: f32 = 20.0;
const ACCEL_FOR: f32 = 0.3;
/// The fastest turn, in radians a second, that prediction carries on with, and
/// the speed below which the car's direction says nothing about where it goes.
const TURN_MAX: f32 = 4.0;
const CRAWL: f32 = 3.0;

/// One packet's arrival.
struct Arrival {
    /// Seconds from the sender's stamp to our clock. Includes the clocks' own
    /// difference, so only how it varies means anything.
    delay: f64,
    /// Sequence numbers this packet moved on from the last: 1 when nothing was lost.
    gap: u64,
    /// Seconds between this snapshot and the one before, when none was lost.
    spacing: f64,
    /// The playhead had already run out of snapshots when this arrived.
    late: bool,
}

/// What the link has been like lately, for the HUD and the log.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
    /// How far behind the fastest packets the car is drawn, in seconds.
    pub buffer: f64,
    /// How much later than the fastest the slowest few packets come, in seconds.
    pub jitter: f64,
    /// Seconds between snapshots.
    pub interval: f64,
    /// Share of packets that never came, and of those that came too late.
    pub lost: f64,
    pub late: f64,
}

impl Stats {
    /// One line for the HUD.
    pub fn line(&self) -> String {
        format!(
            "{:.0} ms behind · jitter {:.0} ms · lost {:.1}% · late {:.1}%",
            self.buffer * 1e3,
            self.jitter * 1e3,
            self.lost * 1e2,
            self.late * 1e2,
        )
    }
}

#[derive(Default)]
pub struct Remote {
    samples: VecDeque<Snapshot>,
    pub received_at: f64,
    arrivals: VecDeque<Arrival>,
    /// The playhead: this sender time at our time `at`, moving at `rate`.
    play: f64,
    at: f64,
    rate: f64,
    /// A correction still fading out, from `since`.
    shift: Vec3,
    turn: Quat,
    since: f64,
}

fn place(s: &Snapshot) -> (Vec3, Quat) {
    (
        Vec3::from_array(s.position),
        Quat::from_array(s.rotation).normalize(),
    )
}

/// The ground-plane part of a snapshot's velocity. The car's own vertical
/// speed is not in it: the road carries the height.
fn flat(v: [f32; 3]) -> Vec3 {
    Vec3::new(v[0], 0.0, v[2])
}

/// How far ahead prediction has got after `tau` seconds of silence.
fn reach(tau: f64) -> f64 {
    if tau <= FREE {
        tau
    } else {
        FREE + TAIL * (1.0 - (-(tau - FREE) / TAIL).exp())
    }
}

/// The value below which `share` of `values` lie.
fn quantile(mut values: Vec<f64>, share: f64) -> Option<f64> {
    let n = values.len();
    (n > 0).then(|| {
        let at = ((n - 1) as f64 * share).round() as usize;
        *values.select_nth_unstable_by(at, f64::total_cmp).1
    })
}

impl Remote {
    pub fn latest(&self) -> Option<&Snapshot> {
        self.samples.back()
    }

    /// Where the playhead is at our time `now`, in the sender's time.
    fn playhead(&self, now: f64) -> f64 {
        self.play + (now - self.at) * self.rate
    }

    pub fn push(&mut self, sample: Snapshot, now: f64) {
        if !sample.valid() {
            return;
        }
        let last = self.samples.back();
        if last
            .is_some_and(|l| sample.seq <= l.seq || sample.time <= l.time || sample.reset < l.reset)
        {
            return;
        }
        // Not the same drive as the last snapshot: a restart, a rescue, or a
        // silence too long to draw through. Nothing is interpolated across it.
        let jump = last.is_some_and(|l| {
            let dt = sample.time - l.time;
            let speed = (flat(l.velocity).length() + flat(sample.velocity).length()) * 0.5;
            sample.reset != l.reset
                || dt > SILENCE
                || Vec3::from_array(sample.position).distance(Vec3::from_array(l.position))
                    > TELEPORT + 1.5 * speed * dt as f32
        });
        self.arrivals.push_back(Arrival {
            delay: now - sample.time,
            gap: last.map_or(1, |l| sample.seq - l.seq),
            spacing: last.map_or(0.0, |l| sample.time - l.time),
            late: last.is_some_and(|l| self.playhead(now) > l.time),
        });
        while self.arrivals.len() > WINDOW {
            self.arrivals.pop_front();
        }

        let before = self.shown(now);
        if jump {
            self.samples.clear();
        }
        self.samples.push_back(sample);
        while self.samples.len() > KEEP {
            self.samples.pop_front();
        }
        self.received_at = now;
        self.steer(now, before.is_none());
        let play = self.playhead(now);
        while self.samples.len() > 3 && self.samples[1].time < play {
            self.samples.pop_front();
        }

        // What was on screen a moment ago and what the new snapshot says should
        // be there differ; fade the difference away instead of jumping.
        self.shift = Vec3::ZERO;
        self.turn = Quat::IDENTITY;
        self.since = now;
        if let (false, Some((p, r)), Some((p2, r2))) = (jump, before, self.raw(play))
            && p.distance(p2) < BLEND_MAX
        {
            self.shift = p - p2;
            self.turn = r * r2.inverse();
        }
    }

    /// Where the playhead should be, now that the link may have changed, and
    /// how fast to move it there.
    fn steer(&mut self, now: f64, fresh: bool) {
        let target = now - self.lead();
        let error = target - self.playhead(now);
        if fresh || error.abs() > RESYNC {
            self.play = target;
            self.rate = 1.0;
        } else {
            self.play = self.playhead(now);
            self.rate = 1.0 + (error / CATCH_UP).clamp(-SLEW, SLEW);
        }
        self.at = now;
    }

    /// How far behind our clock the playhead wants to be, in seconds: the
    /// fastest packets' delay, plus what nearly all packets add to it, plus a
    /// snapshot's spacing so there is one to interpolate towards.
    fn lead(&self) -> f64 {
        let delays: Vec<f64> = self.arrivals.iter().map(|a| a.delay).collect();
        let floor = delays.iter().copied().fold(f64::MAX, f64::min);
        let cover = quantile(delays, COVER).unwrap_or(floor);
        let spacings = self
            .arrivals
            .iter()
            .filter(|a| a.gap == 1 && a.spacing > 0.0)
            .map(|a| a.spacing)
            .collect();
        let spacing = quantile(spacings, 0.9).unwrap_or(1.0 / 30.0);
        let young = 1.0 - self.arrivals.len() as f64 / WARM as f64;
        floor + (cover - floor + spacing * AHEAD + SLACK + PRIOR * young.max(0.0)).min(MAX_LEAD)
    }

    /// The car at sender time `t`, from the snapshots alone.
    fn raw(&self, t: f64) -> Option<(Vec3, Quat)> {
        let after = self.samples.partition_point(|s| s.time <= t);
        Some(match after {
            0 => place(self.samples.front()?),
            n if n == self.samples.len() => self.predict(t)?,
            n => Self::between(&self.samples[n - 1], &self.samples[n], t),
        })
    }

    fn between(a: &Snapshot, b: &Snapshot, t: f64) -> (Vec3, Quat) {
        let h = (b.time - a.time) as f32;
        let u = ((t - a.time) / (b.time - a.time)) as f32;
        let ((pa, ra), (pb, rb)) = (place(a), place(b));
        let mut p = pa.lerp(pb, u);
        // The velocities bend the path between snapshots, as long as they
        // agree with the distance actually covered. Height stays a straight line.
        let (va, vb) = (flat(a.velocity), flat(b.velocity));
        let chord = Vec3::new(pb.x - pa.x, 0.0, pb.z - pa.z);
        if (chord - (va + vb) * 0.5 * h).length() <= 0.25 + 0.3 * chord.length() {
            let (u2, u3) = (u * u, u * u * u);
            let curve = pa * (2.0 * u3 - 3.0 * u2 + 1.0)
                + va * h * (u3 - 2.0 * u2 + u)
                + pb * (3.0 * u2 - 2.0 * u3)
                + vb * h * (u3 - u2);
            p.x = curve.x;
            p.z = curve.z;
        }
        (p, ra.slerp(rb, u))
    }

    /// Carry on past the newest snapshot the way the car was going: at its
    /// speed, turning as it was turning, and gaining or losing speed as it was.
    fn predict(&self, t: f64) -> Option<(Vec3, Quat)> {
        let last = self.samples.back()?;
        let (mut p, mut r) = place(last);
        let ahead = reach((t - last.time).max(0.0)) as f32;
        let v = flat(last.velocity);
        let Some(before) = self
            .samples
            .iter()
            .rev()
            .nth(1)
            .filter(|b| last.time - b.time > 0.002)
        else {
            return Some((p + v * ahead, r));
        };
        let dt = (last.time - before.time) as f32;
        let vb = flat(before.velocity);
        let (mut heading, mut speed) = (v.z.atan2(v.x), v.length());
        let (mut turn, mut gain) = (0.0, 0.0);
        if speed > CRAWL && vb.length() > CRAWL {
            turn = (vb.x * v.z - vb.z * v.x)
                .atan2(vb.dot(v))
                .clamp(-TURN_MAX * dt, TURN_MAX * dt)
                / dt;
            gain = ((speed - vb.length()) / dt).clamp(-ACCEL_MAX, ACCEL_MAX);
        }
        // Walk the arc in small steps.
        let step = ahead / 8.0;
        for i in 0..8 {
            p.x += heading.cos() * speed * step;
            p.z += heading.sin() * speed * step;
            heading += turn * step;
            if step * i as f32 <= ACCEL_FOR {
                speed = (speed + gain * step).max(0.0);
            }
        }
        p.y += ((last.position[1] - before.position[1]) / dt).clamp(-15.0, 15.0) * ahead;
        let (_, rb) = place(before);
        let spin = ((r * rb.inverse()).to_scaled_axis() / dt).clamp_length_max(TURN_MAX);
        r = Quat::from_scaled_axis(spin * ahead.min(FREE as f32)) * r;
        Some((p, r))
    }

    fn shown(&self, now: f64) -> Option<(Vec3, Quat)> {
        let (p, r) = self.raw(self.playhead(now))?;
        let fade = (-(now - self.since) / BLEND).exp() as f32;
        Some((
            p + self.shift * fade,
            Quat::IDENTITY.slerp(self.turn, fade) * r,
        ))
    }

    pub fn pose(&self, now: f64) -> Option<(Vec3, Quat)> {
        if !(0.0..SILENCE).contains(&(now - self.received_at)) {
            return None;
        }
        self.shown(now)
    }

    pub fn stats(&self, now: f64) -> Option<Stats> {
        let n = self.arrivals.len();
        if n < 2 {
            return None;
        }
        let delays: Vec<f64> = self.arrivals.iter().map(|a| a.delay).collect();
        let floor = delays.iter().copied().fold(f64::MAX, f64::min);
        let cover = quantile(delays, COVER)?;
        let sent: u64 = self.arrivals.iter().map(|a| a.gap).sum();
        let steady: Vec<f64> = self
            .arrivals
            .iter()
            .filter(|a| a.gap == 1 && a.spacing > 0.0)
            .map(|a| a.spacing)
            .collect();
        Some(Stats {
            buffer: now - self.playhead(now) - floor,
            jitter: cover - floor,
            interval: steady.iter().sum::<f64>() / steady.len().max(1) as f64,
            lost: (sent - n as u64) as f64 / sent as f64,
            late: self.arrivals.iter().filter(|a| a.late).count() as f64 / n as f64,
        })
    }
}
