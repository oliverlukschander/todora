//! Whether a lap can be driven again from its inputs, on any machine.
//!
//! Online lap times are to be checked by replaying the recorded inputs through
//! this same engine on a server, which only works if the engine gives the same
//! answer to the bit wherever it runs: a Mac, a Linux box, arm64 or x86-64. So
//! the test asks two things of every circuit. Replayed from its own inputs, a
//! drive is the same drive, step for step. And the whole of it — the finished
//! surface's fingerprint and the state of the car after twenty seconds of the
//! plain driver — comes out as `determinism.txt` says, which was written on the
//! machine that blessed it and is read on every machine that runs the tests.
//!
//! When a change to the physics or the circuits moves those numbers on
//! purpose, write them again with `TODORA_BLESS=1 cargo test --locked --lib
//! determinism` and bump [`super::physics::PHYSICS_VERSION`] if lap times can
//! have moved.

use super::PHYSICS_VERSION;
use super::driver::{Driver, Style};
use super::{Car, Controls, SCALE, Spec, advance};
use crate::track::{Track, all_circuits};
use bevy::prelude::*;

/// One physics step, as the game takes them.
const DT: f32 = 1.0 / 240.0;
/// Twenty seconds: the run-up, the line, and well into the lap.
const STEPS: usize = 240 * 20;
const FIXTURE: &str = "src/car/determinism.txt";

/// FNV-1a over the bits of everything the next step depends on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Digest(u64);

impl Digest {
    const START: Self = Self(0xcbf2_9ce4_8422_2325);

    fn take(&mut self, values: &[f32]) {
        for value in values {
            for byte in value.to_bits().to_le_bytes() {
                self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }

    fn state(&mut self, at: &Transform, car: &Car) {
        let (p, r, v) = (at.translation, at.rotation, car.velocity);
        self.take(&[
            p.x,
            p.y,
            p.z,
            r.x,
            r.y,
            r.z,
            r.w,
            v.x,
            v.y,
            v.z,
            car.yaw_rate,
            car.steer_angle,
            car.stranded,
            car.along.unwrap_or(f32::NAN),
            f32::from(u8::from(car.reversing)),
            f32::from(u8::from(car.recovered)),
        ]);
    }
}

fn grid(track: &Track) -> (Transform, Car) {
    (
        track.start_transform().with_scale(Vec3::splat(SCALE)),
        Car {
            along: Some(track.start_along_lap()),
            ..Car::default()
        },
    )
}

/// The plain driver's first twenty seconds, and what it asked for each step.
fn drive(track: &Track) -> (Vec<Controls>, Digest) {
    let handling = Spec::Tourer.handling();
    let mut driver = Driver::new(Style::Plain);
    let (mut at, mut car) = grid(track);
    let mut asked = Vec::with_capacity(STEPS);
    let mut digest = Digest::START;
    for _ in 0..STEPS {
        let controls = driver.decide(track, &handling, &at, &car);
        advance(track, &handling, controls, &mut at, &mut car, DT);
        asked.push(controls.quantised());
        digest.state(&at, &car);
    }
    (asked, digest)
}

/// The same drive again, from nothing but its inputs.
fn replay(track: &Track, asked: &[Controls]) -> Digest {
    let handling = Spec::Tourer.handling();
    let (mut at, mut car) = grid(track);
    let mut digest = Digest::START;
    for controls in asked {
        advance(track, &handling, *controls, &mut at, &mut car, DT);
        digest.state(&at, &car);
    }
    digest
}

fn header() -> String {
    format!("# physics {PHYSICS_VERSION}: circuit, surface fingerprint, digest of {STEPS} steps\n")
}

#[test]
fn every_lap_replays_from_its_inputs_to_the_bit_and_agrees_with_the_blessed_machine() {
    let mut rows = header();
    for circuit in all_circuits() {
        let track = Track::new(circuit);
        let (asked, driven) = drive(&track);
        assert_eq!(
            replay(&track, &asked),
            driven,
            "{} did not replay from its own inputs",
            circuit.id
        );
        rows += &format!(
            "{} {:016x} {:016x}\n",
            circuit.id,
            track.fingerprint(),
            driven.0
        );
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
    if std::env::var_os("TODORA_BLESS").is_some() {
        std::fs::write(&path, &rows).expect("the fixture is writable");
        return;
    }
    let blessed = std::fs::read_to_string(&path).expect("the fixture is there");
    let differ: Vec<_> = rows
        .lines()
        .zip(blessed.lines())
        .filter(|(here, there)| here != there)
        .map(|(here, there)| format!("  here:    {here}\n  blessed: {there}"))
        .collect();
    assert!(
        differ.is_empty() && rows.lines().count() == blessed.lines().count(),
        "{} of {} rows differ from {FIXTURE} on {}-{}:\n{}",
        differ.len(),
        all_circuits().len(),
        std::env::consts::OS,
        std::env::consts::ARCH,
        differ.join("\n")
    );
}

#[test]
fn quantising_keeps_every_input_in_range_and_is_idempotent() {
    for raw in [
        -2.0,
        -1.0,
        -0.5,
        0.0,
        0.001,
        0.25,
        0.9999,
        1.0,
        3.0,
        f32::NAN,
    ] {
        let once = Controls {
            throttle: raw,
            brake: raw,
            steer: raw,
            handbrake: false,
        }
        .quantised();
        assert!((0.0..=1.0).contains(&once.throttle) && (0.0..=1.0).contains(&once.brake));
        assert!((-1.0..=1.0).contains(&once.steer));
        assert_eq!(once.quantised(), once);
        assert!((once.throttle * 255.0).fract() == 0.0);
        if raw.is_nan() {
            assert_eq!(once, Controls::default(), "a NaN was not let go of");
        }
    }
}
