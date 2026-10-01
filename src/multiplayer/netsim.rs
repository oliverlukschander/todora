//! A deterministic stand-in for two Macs and the Internet between them.
//!
//! A car drives a fast figure of eight on the sending side. Its snapshots cross
//! a link with delay, jitter, loss, reordering and clock skew, and the receiver
//! draws them at its own frame rate. The report says how evenly the far car
//! moved on the receiver's screen.
use super::{remote::Remote, session::Snapshot};
use bevy::prelude::*;

/// The physics runs at this rate whatever the frame rate.
const STEP: f64 = 1.0 / 240.0;
/// The receiver's clock is this far ahead of the sender's.
const SKEW: f64 = 87.0;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    /// A one-sided tail: most draws are small, a few are several times the scale.
    fn tail(&mut self, scale: f64) -> f64 {
        -self.next().max(1e-12).ln() * scale
    }
}

/// Where the car is, which way it points and how fast it goes, at time `t`.
/// About 50 m/s on the straights and up to 2.5 g through the bends.
fn truth(t: f64) -> (Vec3, Quat, Vec3) {
    let w = 0.32;
    let at = |t: f64| {
        Vec3::new(
            (160.0 * (w * t).sin()) as f32,
            (4.0 + 3.0 * (0.6 * w * t).sin()) as f32,
            (80.0 * (2.0 * w * t).sin()) as f32,
        )
    };
    let ahead = at(t + 1e-3) - at(t - 1e-3);
    let v = ahead / 2e-3;
    let heading = Transform::IDENTITY
        .looking_to(ahead.normalize(), Vec3::Y)
        .rotation;
    // The game's own velocity leaves height to the road.
    (at(t), heading, Vec3::new(v.x, 0.0, v.z))
}

fn snapshot(seq: u64, stamp: f64, at: f64) -> Snapshot {
    let (p, r, v) = truth(at);
    Snapshot {
        seq,
        reset: 0,
        time: stamp,
        position: p.to_array(),
        rotation: r.to_array(),
        velocity: v.to_array(),
        lap: 20.0,
        invalid: false,
        paused: false,
        laps: 0,
        best: None,
    }
}

#[derive(Clone, Copy)]
struct Net {
    /// The fastest a packet ever crosses, in seconds.
    base: f64,
    /// Scale of the exponential tail added to every packet.
    jitter: f64,
    /// Chance a packet is caught behind a retransmission, and how long that takes.
    spike: (f64, f64),
    /// Long-run loss, and how many packets go together.
    loss: (f64, f64),
    /// The receiver's clock against the sender's, in parts per million.
    drift: f64,
    /// Between these seconds, everything is this much slower.
    step: (f64, f64, f64),
}

#[derive(Clone, Copy)]
struct Rig {
    /// The sender as this build has it: steady 60 a second, stamped with the
    /// moment the physics state was true. Otherwise as the last build had it:
    /// 30 a second from the frame's own clock.
    steady: bool,
    send_fps: f64,
    recv_fps: f64,
    /// Chance per receiving frame of a stall, and its length in seconds.
    hitch: (f64, f64),
}

const LAN: Net = Net {
    base: 0.002,
    jitter: 0.0005,
    spike: (0.0, 0.0),
    loss: (0.0, 1.0),
    drift: 0.0,
    step: (0.0, 0.0, 0.0),
};
const WIFI: Net = Net {
    base: 0.020,
    jitter: 0.004,
    spike: (0.004, 0.06),
    loss: (0.005, 1.5),
    drift: 30.0,
    step: (0.0, 0.0, 0.0),
};
const POOR: Net = Net {
    base: 0.055,
    jitter: 0.014,
    spike: (0.01, 0.15),
    loss: (0.03, 3.0),
    drift: -80.0,
    step: (0.0, 0.0, 0.0),
};
const AWFUL: Net = Net {
    base: 0.09,
    jitter: 0.03,
    spike: (0.02, 0.3),
    loss: (0.08, 4.0),
    drift: 150.0,
    step: (0.0, 0.0, 0.0),
};
/// A WiFi link that gets 80 ms slower for ten seconds.
const SLOWS: Net = Net {
    step: (20.0, 30.0, 0.08),
    ..WIFI
};

const RIG: Rig = Rig {
    steady: true,
    send_fps: 60.0,
    recv_fps: 60.0,
    hitch: (0.0, 0.0),
};
/// The build before this one, sending to this one.
const OLD_SENDER: Rig = Rig {
    steady: false,
    ..RIG
};

#[derive(Debug, Default)]
struct Report {
    /// Frames where the far car was not drawn at all.
    missing: usize,
    /// The car's mean age on screen, in ms.
    lag: f64,
    /// Judder, in cm: how far each frame's movement strays from what the
    /// frames around it were doing. A pop or a stutter shows here; a smooth
    /// change of speed does not. Median, 99th, 99.9th and worst.
    p50: f64,
    p99: f64,
    p999: f64,
    worst: f64,
    /// How far the car's speed on screen strays from its real speed, at worst
    /// for 1 frame in 100, in per cent.
    drift: f64,
    /// The furthest the far car strayed from the road it was on, in cm.
    off: f64,
}

/// Run `seconds` of driving and measure the far car on the receiver's screen.
fn run(net: Net, rig: Rig, seconds: f64, seed: u64) -> Report {
    let mut rng = Rng(seed | 1);
    let skew = |t: f64| SKEW + t * net.drift * 1e-6;
    // (arrives, stamped, snapshot)
    let mut flying: Vec<(f64, f64, Snapshot)> = vec![];

    // Sender frames, and everything they send.
    let (mut t, mut next, mut seq, mut lost) = (0.0, 0.0, 0, false);
    while t < seconds {
        t += 1.0 / rig.send_fps + (rng.next() - 0.5) * 0.0006;
        // The physics has advanced to the last whole step before this frame.
        let overstep = t % STEP;
        let (send, stamp) = if rig.steady {
            let due = t + 0.002 >= next;
            if due {
                next = (next + 1.0 / 60.0).max(t);
            }
            (due, t - overstep)
        } else {
            let due = t >= next;
            if due {
                next = t + 1.0 / 30.0;
            }
            (due, t)
        };
        if !send {
            continue;
        }
        seq += 1;
        // Gilbert-Elliott loss: bursts whose average matches the target.
        let (mean, burst) = net.loss;
        let leave = 1.0 / burst;
        let enter = mean * leave / (1.0 - mean);
        lost = if lost {
            rng.next() > leave
        } else {
            rng.next() < enter
        };
        if lost {
            continue;
        }
        let mut delay = net.base + rng.tail(net.jitter);
        if (net.step.0..net.step.1).contains(&t) {
            delay += net.step.2;
        }
        if rng.next() < net.spike.0 {
            delay += rng.next() * net.spike.1;
        }
        // GameKit hands the packet on once the frame has finished.
        delay += rng.next() * 0.004;
        flying.push((t + delay, stamp, snapshot(seq, stamp, t - overstep)));
    }
    flying.sort_by(|a, b| a.0.total_cmp(&b.0));

    // Receiver frames.
    let mut remote = Remote::default();
    let mut report = Report::default();
    let (mut g, mut i) = (0.0, 0);
    let mut shown: Vec<(f64, Vec3)> = vec![];
    while g < seconds {
        g += 1.0 / rig.recv_fps + (rng.next() - 0.5) * 0.0006;
        if rng.next() < rig.hitch.0 {
            g += rig.hitch.1;
        }
        while i < flying.len() && flying[i].0 <= g {
            // Their clock, not ours: the stamp says nothing about our time.
            let mut sample = flying[i].2.clone();
            sample.time += skew(flying[i].1);
            remote.push(sample, g);
            i += 1;
        }
        if g < 6.0 {
            continue;
        }
        match remote.pose(g) {
            Some((p, _)) => shown.push((g, p)),
            None => report.missing += 1,
        }
    }

    // Where on the road was each drawn position, and when?
    let (mut lags, mut road, mut guess) = (vec![], vec![], 0.0);
    for (g, p) in &shown {
        let closest = |from: f64, to: f64, by: f64| {
            let (mut best, mut s) = ((f32::MAX, 0.0), from);
            while s < to {
                let d = truth(s).0.distance(*p);
                if d < best.0 {
                    best = (d, s);
                }
                s += by;
            }
            best
        };
        let centre = if guess == 0.0 { g - 0.15 } else { guess };
        let coarse = closest(centre - 0.4, centre + 0.4, 0.001);
        let (off, at) = closest(coarse.1 - 0.001, coarse.1 + 0.001, 0.0001);
        report.off = report.off.max(off as f64 * 100.0);
        guess = at + 1.0 / rig.recv_fps;
        lags.push(g - at);
        road.push(truth(at).2.length() as f64);
    }

    // Speed on screen, frame to frame.
    let speeds: Vec<f64> = shown
        .windows(2)
        .map(|w| w[1].1.distance(w[0].1) as f64 / (w[1].0 - w[0].0))
        .collect();
    let (mut judder, mut drift) = (vec![], vec![]);
    for j in 2..speeds.len().saturating_sub(2) {
        let mut around = [
            speeds[j - 2],
            speeds[j - 1],
            speeds[j],
            speeds[j + 1],
            speeds[j + 2],
        ];
        around.sort_by(f64::total_cmp);
        let dt = shown[j + 1].0 - shown[j].0;
        judder.push((speeds[j] - around[2]).abs() * dt * 100.0);
        drift.push((speeds[j] / road[j + 1] - 1.0).abs() * 100.0);
    }
    judder.sort_by(f64::total_cmp);
    drift.sort_by(f64::total_cmp);
    let at = |v: &[f64], share: f64| v[((v.len() - 1) as f64 * share) as usize];
    report.p50 = at(&judder, 0.5);
    report.p99 = at(&judder, 0.99);
    report.p999 = at(&judder, 0.999);
    report.worst = at(&judder, 1.0);
    report.drift = at(&drift, 0.99);
    report.lag = lags.iter().sum::<f64>() / lags.len() as f64 * 1000.0;
    report
}

/// Every seed of the link must come out this smooth.
fn steady(net: Net, rig: Rig, judder_999: f64, worst: f64, lag: f64) {
    for seed in [1, 2, 3] {
        let r = run(net, rig, 60.0, seed);
        assert_eq!(r.missing, 0, "the car vanished: {r:?}");
        assert!(r.p99 < 0.1, "judders: {r:?}");
        assert!(r.p999 < judder_999, "judders: {r:?}");
        assert!(r.worst < worst, "pops: {r:?}");
        assert!(r.drift < 8.0, "speed wanders: {r:?}");
        assert!(r.lag < lag, "too far behind: {r:?}");
    }
}

#[test]
fn a_clean_link_shows_the_far_car_without_a_tremor() {
    // Under a millimetre a frame at 180 km/h, and barely behind.
    steady(LAN, RIG, 0.2, 1.0, 50.0);
}

#[test]
fn wifi_with_loss_and_retransmission_spikes_is_just_as_smooth() {
    steady(WIFI, RIG, 0.5, 3.0, 80.0);
}

#[test]
fn a_poor_link_costs_delay_not_smoothness() {
    steady(POOR, RIG, 0.5, 4.0, 130.0);
}

#[test]
fn frame_rates_that_differ_do_not_matter() {
    for (send_fps, recv_fps, judder, worst) in [
        (30.0, 60.0, 0.5, 4.0),
        (60.0, 120.0, 0.2, 2.0),
        (60.0, 30.0, 1.0, 2.0),
    ] {
        let rig = Rig {
            send_fps,
            recv_fps,
            ..RIG
        };
        steady(WIFI, rig, judder, worst, 100.0);
    }
}

#[test]
fn a_stall_on_the_receiving_mac_is_caught_up_not_stuttered() {
    // 120 ms frozen, one frame in two hundred: the car jumps to where it is,
    // which is what a stalled game should do, and does not wobble after.
    let rig = Rig {
        hitch: (0.005, 0.12),
        ..RIG
    };
    steady(WIFI, rig, 0.5, 6.0, 100.0);
}

#[test]
fn a_link_that_slows_by_80_ms_is_taken_up_without_a_lurch() {
    steady(SLOWS, RIG, 0.5, 4.0, 110.0);
}

#[test]
fn a_dreadful_link_never_freezes_or_loses_the_car() {
    for seed in [1, 2, 3] {
        // 8% loss in bursts, spikes up to 300 ms: whole half seconds go missing.
        let r = run(AWFUL, RIG, 60.0, seed);
        assert_eq!(r.missing, 0, "the car vanished: {r:?}");
        assert!(r.p99 < 0.5, "judders: {r:?}");
        assert!(r.p999 < 2.0, "judders: {r:?}");
        assert!(r.off < 100.0, "the car left the road: {r:?}");
        assert!(r.drift < 8.0, "speed wanders: {r:?}");
    }
}

#[test]
fn the_last_build_sending_still_plays_far_better_than_it_did() {
    // Its stamps are off by up to one physics step, which nothing on this side
    // can undo: about 10 cm of judder at worst, where playing them as they came
    // gave more than a metre.
    for seed in [1, 2, 3] {
        for net in [LAN, WIFI, SLOWS] {
            let r = run(net, OLD_SENDER, 60.0, seed);
            assert_eq!(r.missing, 0, "the car vanished: {r:?}");
            assert!(r.p99 < 15.0, "judders: {r:?}");
            assert!(r.worst < 30.0, "pops: {r:?}");
            assert!(r.drift < 30.0, "speed wanders: {r:?}");
        }
    }
}

#[test]
#[ignore = "prints the whole picture; run with --ignored --nocapture"]
fn report() {
    let rigs = [
        ("60/60", RIG),
        (
            "30/60",
            Rig {
                send_fps: 30.0,
                ..RIG
            },
        ),
        (
            "60/120",
            Rig {
                recv_fps: 120.0,
                ..RIG
            },
        ),
        (
            "60/30",
            Rig {
                recv_fps: 30.0,
                ..RIG
            },
        ),
        (
            "stalls",
            Rig {
                hitch: (0.005, 0.12),
                ..RIG
            },
        ),
        ("old tx", OLD_SENDER),
    ];
    println!(
        "{:8} {:7} {:>7} {:>8} {:>7} {:>7} {:>7} {:>7} {:>7}",
        "link", "fps", "lag ms", "p50 cm", "p99", "p99.9", "worst", "drift%", "off cm"
    );
    let links = [
        ("lan", LAN),
        ("wifi", WIFI),
        ("poor", POOR),
        ("awful", AWFUL),
        ("slows", SLOWS),
    ];
    for (name, net) in links {
        for (fps, rig) in rigs {
            let r = run(net, rig, 60.0, 7);
            println!(
                "{:8} {:7} {:7.1} {:8.3} {:7.2} {:7.2} {:7.1} {:7.1} {:7.1}",
                name, fps, r.lag, r.p50, r.p99, r.p999, r.worst, r.drift, r.off
            );
        }
    }
}
