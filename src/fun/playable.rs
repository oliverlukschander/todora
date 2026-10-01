//! Whether a person can steer it.
//!
//! Bonkers was first made fast by multiplying everything: 2.4 times the speed and
//! nearly six times the grip, so the same corners at 2.4 times the pace. That is
//! the same car, and it is also 2.4 times less time for every corner, which
//! nobody's hands have. It looked wonderful and could not be steered. So what Bonkers offers is measured here rather than
//! guessed at: the game's own drivers drive it the way it is played, over the hills
//! and off them, across the boost pads and up off the jump pads, through the
//! engine the game drives and the tweaks, air and pads Bonkers puts round it.
//!
//! The one that matters is the clumsy driver, who is a person on a keyboard: full
//! lock or nothing, a seventh of a second behind, eighteen metres of look-ahead,
//! brakes that go on late. It is held to keeping Bonkers on the road about as well
//! as it keeps the game that shipped there.

use bevy::prelude::*;

use super::air::{self, Air, Event};
use super::course::{self, PadKind};
use super::events::Effect;
use super::pads;
use super::tweak::{Boost, Tweaks};
use crate::car::{Car, Controls, Driver, Handling, SCALE, Style, advance, level, step_seconds};
use crate::lap::{LapTimer, Step};
use crate::track::{ROAD_HALF, Track};

/// How a circuit is driven.
#[derive(Clone, Copy, Debug)]
pub(super) struct Ride {
    /// How many times the shipped car's speed.
    pub speed: f32,
    /// A crest steep enough is left, as Bonkers leaves it.
    pub crests: bool,
    /// The boost pads and the jump pads are on the road.
    pub boosts: bool,
    pub jumps: bool,
    /// Something happening for the whole of the drive, at full strength.
    pub chaos: Option<Effect>,
}

impl Ride {
    /// The game as it shipped.
    pub(super) const SHIPPED: Self = Self {
        speed: 1.0,
        crests: false,
        boosts: false,
        jumps: false,
        chaos: None,
    };

    /// Bonkers at `speed`, all of it.
    pub(super) fn bonkers(speed: f32) -> Self {
        Self {
            speed,
            crests: true,
            boosts: true,
            jumps: true,
            chaos: None,
        }
    }

    fn bonkers_at_all(&self) -> bool {
        self.crests || self.boosts || self.jumps || self.chaos.is_some()
    }
}

/// What came of a drive.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Drive {
    pub seconds: f32,
    /// Laps the clock finished, and how many of those it would have counted.
    pub laps: u32,
    pub clean: u32,
    /// Seconds with the car past the kerb.
    pub off_road: f32,
    /// Times the car had to be fetched back to the road.
    pub rescues: u32,
    /// Times it left the ground, and seconds spent off it.
    pub launches: u32,
    pub airtime: f32,
}

impl Drive {
    /// The share of the drive spent off the road.
    pub(super) fn off(&self) -> f32 {
        self.off_road / self.seconds.max(1e-3)
    }
}

/// Drive `laps` laps of `track` as `style` drives, the way `ride` says.
pub(super) fn drive(track: &Track, style: Style, ride: Ride, laps: u32) -> Drive {
    let dt = step_seconds();
    let k = ride.speed;
    let mut course = if ride.boosts || ride.jumps {
        course::plan(track)
    } else {
        course::Course::default()
    };
    course.pads.retain(|pad| match pad.kind {
        PadKind::Boost => ride.boosts,
        PadKind::Jump => ride.jumps,
    });
    let strength = |effect| f32::from(u8::from(ride.chaos == Some(effect)));
    let mut cooling = vec![0.0f32; course.pads.len()];
    let mut driver = Driver::new(style);
    let mut at = track.start_transform().with_scale(Vec3::splat(SCALE));
    let mut car = Car {
        along: Some(track.start_along_lap()),
        ..Car::default()
    };
    let mut air = Air::default();
    let mut boost = Boost::default();
    if ride.chaos == Some(Effect::Turbo) {
        boost.fire(super::events::TURBO, f32::INFINITY);
    }
    let mut timer = LapTimer::default();
    let mut controls = Controls::default();
    let mut out = Drive::default();
    // Time for the run-up and the laps, at the pace of a car having a hard time.
    let seconds = (laps as f32 + 1.0) * track.length() / 4.0;
    for i in 0..(seconds / dt) as usize {
        if boost.left > 0.0 {
            boost.left = (boost.left - dt).max(0.0);
            if boost.left == 0.0 {
                boost.power = 0.0;
            }
        }
        let tweaks = if ride.bonkers_at_all() {
            Tweaks::bonkers(
                k,
                boost.strength(),
                strength(Effect::Ice),
                strength(Effect::Hyper),
                air.flying,
            )
        } else {
            Tweaks {
                speed: k,
                ..Tweaks::NEUTRAL
            }
        };
        let handling = tweaks.apply(&Handling::SHOOTING_BRAKE);
        // A driver's reaction time is counted in looks at 120 a second, as it
        // is in the tests that set it.
        if i % 2 == 0 {
            controls = driver.decide(track, &handling, &at, &car);
        }
        let given = if air.flying {
            air::aloft(controls)
        } else {
            controls
        };
        advance(track, &handling, given, &mut at, &mut car, dt);
        timer.count(dt);
        let recovered = std::mem::take(&mut car.recovered);
        out.rescues += u32::from(recovered);
        if ride.bonkers_at_all() {
            // The speed in force, hyperdrive and all, as the game has it.
            let k = tweaks.speed;
            let ground = track.ground_from(at.translation, car.along);
            let moon = if air.flying {
                super::events::moon_gravity(strength(Effect::Moon))
            } else {
                1.0
            };
            // A crest is only ever left at a speed nothing reaches when they
            // are not being left at all.
            let from = if ride.crests {
                air::crest_from(k, car.velocity.length(), air::bend_ahead(track, &ground))
            } else {
                f32::MAX
            };
            let event = air.step(
                dt,
                air::gravity(k) * moon,
                ground.centre.y,
                at.translation.y,
                car.velocity.length(),
                from,
            );
            out.launches += u32::from(matches!(event, Some(Event::Launched { .. })));
            for cool in &mut cooling {
                *cool = (*cool - dt).max(0.0);
            }
            for (cool, pad) in cooling.iter_mut().zip(&course.pads) {
                if air.flying
                    || *cool > 0.0
                    || car.velocity.length() < 2.0
                    || !pads::under(pad, at.translation)
                {
                    continue;
                }
                *cool = pads::REARM;
                match pad.kind {
                    PadKind::Boost => {
                        boost.fire(pads::BOOST_POWER, pads::BOOST_FOR);
                        car.velocity += level(*at.forward()) * pads::KICK * k;
                    }
                    PadKind::Jump => {
                        air.launch(at.translation.y, pads::LAUNCH * k);
                        out.launches += 1;
                    }
                }
            }
            if air.flying {
                out.airtime += dt;
            }
        }
        let pos = at.translation;
        if track.ground_from(pos, car.along).lateral.abs() > ROAD_HALF {
            out.off_road += dt;
        }
        out.seconds += dt;
        let step = Step {
            legal: track.legal_contact(&at, car.along),
            recovered,
            along: track.start_along(pos),
            progress: track.progress(pos, car.along),
            length: track.length(),
            sectors: track.sector_count(),
            speed: car.velocity.length(),
        };
        if let Some(done) = timer.judge(step, || track.on_start_gate(pos, car.along)) {
            out.laps += 1;
            out.clean += u32::from(done.valid);
            if out.laps >= laps {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::track::all_circuits;

    /// Every third circuit, which is a little of everything: fast and slow,
    /// narrow and wide, flat and hilly.
    fn sample() -> impl Iterator<Item = &'static crate::track::Circuit> {
        all_circuits().iter().step_by(3)
    }

    /// What the two tests below measure, measured once for both: per circuit
    /// of the sample, the game as shipped and Bonkers as it comes, by both
    /// drivers, and the clumsy driver at Bonkers' fastest and at the first
    /// Bonkers' 2.4 times.
    struct Measured {
        shipped: Vec<(Drive, Drive)>,
        bonkers: Vec<(Drive, Drive)>,
        fastest: Vec<Drive>,
        first: Vec<Drive>,
    }

    fn measured() -> &'static Measured {
        static ONCE: std::sync::OnceLock<Measured> = std::sync::OnceLock::new();
        ONCE.get_or_init(|| {
            let defaults = crate::settings::Settings::default();
            let (speed, wild) = (defaults.speed.scale(), defaults.wild.level());
            let fastest = super::super::Speed::ALL
                .iter()
                .map(|s| s.scale())
                .fold(1.0, f32::max);
            let circuits: Vec<_> = sample().collect();
            let mut out: Vec<Option<[Drive; 6]>> = vec![None; circuits.len()];
            std::thread::scope(|scope| {
                for (some, slots) in circuits.chunks(2).zip(out.chunks_mut(2)) {
                    scope.spawn(move || {
                        for (circuit, slot) in some.iter().zip(slots) {
                            let stock = Track::new(circuit);
                            let wild = Track::with_wild(circuit, wild);
                            let drives = [
                                drive(&stock, Style::Plain, Ride::SHIPPED, 1),
                                drive(&stock, Style::Clumsy, Ride::SHIPPED, 1),
                                drive(&wild, Style::Plain, Ride::bonkers(speed), 1),
                                drive(&wild, Style::Clumsy, Ride::bonkers(speed), 1),
                                drive(&wild, Style::Clumsy, Ride::bonkers(fastest), 1),
                                drive(&wild, Style::Clumsy, Ride::bonkers(2.4), 1),
                            ];
                            *slot = Some(drives);
                        }
                    });
                }
            });
            let mut m = Measured {
                shipped: Vec::new(),
                bonkers: Vec::new(),
                fastest: Vec::new(),
                first: Vec::new(),
            };
            for [a, b, c, d, e, f] in out.into_iter().map(|o| o.expect("driven")) {
                m.shipped.push((a, b));
                m.bonkers.push((c, d));
                m.fastest.push(e);
                m.first.push(f);
            }
            m
        })
    }

    fn mean(values: impl Iterator<Item = f32>) -> f32 {
        let (sum, n) = values.fold((0.0, 0), |(sum, n), v| (sum + v, n + 1));
        sum / n.max(1) as f32
    }

    /// Bonkers as it comes, at the speed and the wildness its rows start at, is
    /// as steerable as the game as shipped. The clumsy driver, who is a person on
    /// a keyboard, spends little more of the drive off the road than it does in
    /// the game as shipped; the plain one gets round every circuit, and keeps to
    /// the road nearly all the way.
    #[test]
    fn bonkers_as_it_comes_is_as_steerable_as_the_game_as_shipped() {
        let m = measured();
        let shipped = mean(m.shipped.iter().map(|(_, clumsy)| clumsy.off()));
        let bonkers = mean(m.bonkers.iter().map(|(_, clumsy)| clumsy.off()));
        let plain = mean(m.bonkers.iter().map(|(plain, _)| plain.off()));
        println!(
            "off the road: a person {:.1}% in Bonkers, {:.1}% as shipped; the plain driver {:.1}%",
            bonkers * 100.0,
            shipped * 100.0,
            plain * 100.0
        );
        assert!(
            bonkers <= shipped * 1.12,
            "a person is off the road {:.1}% of the drive in Bonkers, and {:.1}% as shipped",
            bonkers * 100.0,
            shipped * 100.0
        );
        assert!(
            plain < 0.03,
            "the plain driver is off the road {:.1}% of the drive",
            plain * 100.0
        );
        for (circuit, (plain, _)) in sample().zip(&m.bonkers) {
            assert!(
                plain.laps >= 1,
                "{}: the plain driver never got round Bonkers",
                circuit.name
            );
        }
    }

    /// And the fastest Bonkers goes is nearer the game as shipped than the first
    /// Bonkers was, which nobody could steer.
    #[test]
    fn even_the_fastest_bonkers_is_nearer_the_game_than_the_first_one() {
        let m = measured();
        let shipped = mean(m.shipped.iter().map(|(_, clumsy)| clumsy.off()));
        let fastest = mean(m.fastest.iter().map(Drive::off));
        let first = mean(m.first.iter().map(Drive::off));
        println!(
            "a person off the road: {:.1}% as shipped, {:.1}% at Bonkers' fastest, {:.1}% in \
             the first Bonkers",
            shipped * 100.0,
            fastest * 100.0,
            first * 100.0
        );
        // Or none of this measures anything.
        assert!(
            first > shipped + 0.1,
            "the first Bonkers was easy to steer after all: {first:.2} against {shipped:.2}"
        );
        assert!(
            fastest - shipped < (first - shipped) / 2.0,
            "a person is off the road {:.1}% of the drive at Bonkers' fastest, {:.1}% as \
             shipped and {:.1}% in the first Bonkers",
            fastest * 100.0,
            shipped * 100.0,
            first * 100.0
        );
    }

    /// Every circuit at every wildness, at a handful of speeds, by both drivers,
    /// as a table: what the defaults were chosen from.
    /// `TODORA_SPEEDS=1,1.2 TODORA_WILD=0,2 cargo test --lib bonkers_as_a_table -- --ignored --nocapture`
    #[test]
    #[ignore = "drives every circuit dozens of times"]
    fn bonkers_as_a_table() {
        let list = |name: &str, default: &str| -> Vec<f32> {
            std::env::var(name)
                .unwrap_or_else(|_| default.into())
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect()
        };
        let speeds = list("TODORA_SPEEDS", "1,1.15,1.3,1.6,2.4");
        let wilds: Vec<u8> = list("TODORA_WILD", "0,1,2,3")
            .into_iter()
            .map(|w| w as u8)
            .collect();
        let parts = std::env::var("TODORA_PARTS").unwrap_or_else(|_| "crests,boosts,jumps".into());
        let chaos = std::env::var("TODORA_CHAOS").ok().and_then(|name| {
            Effect::ALL
                .into_iter()
                .find(|e| format!("{e:?}").eq_ignore_ascii_case(&name))
        });
        let bonkers_ride = |speed| Ride {
            speed,
            crests: parts.contains("crests"),
            boosts: parts.contains("boosts"),
            jumps: parts.contains("jumps"),
            chaos,
        };
        let only = std::env::var("TODORA_ONLY").unwrap_or_default();
        let circuits: Vec<_> = all_circuits()
            .iter()
            .filter(|c| only.is_empty() || only.split(',').any(|id| id == c.id))
            .collect();
        // (style, wild, speed, bonkers) -> per-circuit drives.
        let mut rows: Vec<(String, Vec<(&'static str, Drive)>)> = Vec::new();
        let mut cells: Vec<(Style, u8, f32, bool)> = Vec::new();
        for style in [Style::Plain, Style::Clumsy] {
            cells.push((style, 0, 1.0, false));
            for &wild in &wilds {
                for &speed in &speeds {
                    cells.push((style, wild, speed, true));
                }
            }
        }
        let results = std::sync::Mutex::new(vec![Vec::new(); cells.len()]);
        std::thread::scope(|scope| {
            for some in circuits.chunks(circuits.len().div_ceil(12).max(1)) {
                let (cells, results, wilds, bonkers_ride) =
                    (&cells, &results, &wilds, &bonkers_ride);
                scope.spawn(move || {
                    for circuit in some {
                        let mut tracks = vec![(0u8, Track::new(circuit))];
                        for &wild in wilds.iter().filter(|w| **w > 0) {
                            tracks.push((wild, Track::with_wild(circuit, wild)));
                        }
                        for (n, &(style, wild, speed, bonkers)) in cells.iter().enumerate() {
                            let track = &tracks.iter().find(|(w, _)| *w == wild).unwrap().1;
                            let ride = if bonkers {
                                bonkers_ride(speed)
                            } else {
                                Ride::SHIPPED
                            };
                            let drive = drive(track, style, ride, 2);
                            results.lock().unwrap()[n].push((circuit.id, drive));
                        }
                    }
                });
            }
        });
        let results = results.into_inner().unwrap();
        for (cell, drives) in cells.iter().zip(results) {
            let (style, wild, speed, bonkers) = cell;
            let name = format!(
                "{:>6} w{wild} x{speed:<4} {}",
                format!("{style:?}"),
                if *bonkers { "bonkers" } else { "shipped" }
            );
            rows.push((name, drives));
        }
        println!(
            "{:<26} {:>6} {:>6} {:>6} {:>6} {:>7} {:>6} {:>6}  worst",
            "", "round", "clean", "off%", "off%max", "rescue", "jumps", "air%"
        );
        for (name, drives) in rows {
            let n = drives.len() as f32;
            let finished = drives.iter().filter(|(_, d)| d.laps >= 2).count();
            let laps: u32 = drives.iter().map(|(_, d)| d.laps).sum();
            let clean: u32 = drives.iter().map(|(_, d)| d.clean).sum();
            let off = drives.iter().map(|(_, d)| d.off()).sum::<f32>() / n;
            let (worst, worst_off) = drives
                .iter()
                .map(|(id, d)| (*id, d.off()))
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap_or(("", 0.0));
            let rescues: u32 = drives.iter().map(|(_, d)| d.rescues).sum();
            let jumps =
                drives.iter().map(|(_, d)| d.launches).sum::<u32>() as f32 / laps.max(1) as f32;
            let air = drives
                .iter()
                .map(|(_, d)| d.airtime / d.seconds.max(1e-3))
                .sum::<f32>()
                / n;
            println!(
                "{name:<26} {:>5}% {:>5}% {:>5.1}% {:>6.1}% {:>7} {:>6.1} {:>5.1}%  {worst}",
                (100 * finished) / drives.len().max(1),
                (100 * clean) / laps.max(1),
                off * 100.0,
                worst_off * 100.0,
                rescues,
                jumps,
                air * 100.0,
            );
        }
    }
}
