//! A run: one lap, as it can be driven again.
//!
//! Everything a server needs to drive the lap itself and see whether it is
//! the lap that was claimed: which circuit, as which shape; which car, setup and
//! mode; the car's exact state as it crossed the line to start the lap; and the
//! pedals and wheel for every physics step until it crossed the line again. The
//! claimed time and sectors ride along so a replay has something to agree with.
//!
//! Little-endian, behind the magic `TODORUN1`. The inputs are run-length
//! encoded: a keyboard lap is a few hundred runs, a pad lap a few thousand,
//! against 14,000 steps a minute. Nothing here allocates per step of a lap
//! except the input list itself, which is reserved once.

use crate::car::{Car, Controls, Mode, Setup, Spec};
use bevy::prelude::*;

pub(crate) const MAGIC: [u8; 8] = *b"TODORUN1";
/// Bumped whenever the layout below changes.
pub(crate) const FORMAT: u16 = 1;
/// The most steps a run may hold: ten minutes. Nobody is uploading that lap.
pub(crate) const LONGEST: u32 = 240 * 60 * 10;
/// Largest payload a run may be, which is also what the server accepts.
pub(crate) const MAX_BYTES: usize = 256 * 1024;

/// The car as it crossed the line, to the bit: everything the next step reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Entry {
    pub translation: Vec3,
    pub rotation: Quat,
    pub velocity: Vec3,
    pub yaw_rate: f32,
    pub steer_angle: f32,
    pub reversing: bool,
    pub stranded: f32,
    pub along: Option<f32>,
    pub g_force: Vec2,
    pub slip_angle: f32,
    pub grip_used: f32,
    pub rear_slip: f32,
}

impl Entry {
    pub(crate) fn of(at: &Transform, car: &Car) -> Self {
        Self {
            translation: at.translation,
            rotation: at.rotation,
            velocity: car.velocity,
            yaw_rate: car.yaw_rate,
            steer_angle: car.steer_angle,
            reversing: car.reversing,
            stranded: car.stranded,
            along: car.along,
            g_force: car.g_force,
            slip_angle: car.slip_angle,
            grip_used: car.grip_used,
            rear_slip: car.rear_slip,
        }
    }

    /// The car and where it is, ready to drive on from.
    pub(crate) fn car(&self) -> (Transform, Car) {
        (
            Transform {
                translation: self.translation,
                rotation: self.rotation,
                scale: Vec3::splat(crate::car::SCALE),
            },
            Car {
                velocity: self.velocity,
                yaw_rate: self.yaw_rate,
                steer_angle: self.steer_angle,
                reversing: self.reversing,
                stranded: self.stranded,
                recovered: false,
                along: self.along,
                g_force: self.g_force,
                slip_angle: self.slip_angle,
                grip_used: self.grip_used,
                rear_slip: self.rear_slip,
            },
        )
    }

    fn floats(&self) -> [f32; 20] {
        let (t, r, v, g) = (self.translation, self.rotation, self.velocity, self.g_force);
        [
            t.x,
            t.y,
            t.z,
            r.x,
            r.y,
            r.z,
            r.w,
            v.x,
            v.y,
            v.z,
            self.yaw_rate,
            self.steer_angle,
            self.stranded,
            self.along.unwrap_or(f32::NAN),
            g.x,
            g.y,
            self.slip_angle,
            self.grip_used,
            self.rear_slip,
            f32::from(u8::from(self.reversing)),
        ]
    }

    fn from_floats(f: [f32; 20]) -> Self {
        Self {
            translation: Vec3::new(f[0], f[1], f[2]),
            rotation: Quat::from_xyzw(f[3], f[4], f[5], f[6]),
            velocity: Vec3::new(f[7], f[8], f[9]),
            yaw_rate: f[10],
            steer_angle: f[11],
            stranded: f[12],
            along: (!f[13].is_nan()).then_some(f[13]),
            g_force: Vec2::new(f[14], f[15]),
            slip_angle: f[16],
            grip_used: f[17],
            rear_slip: f[18],
            reversing: f[19] != 0.0,
        }
    }
}

/// One lap, as recorded.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Run {
    pub app_version: String,
    pub physics: u32,
    pub circuit: String,
    pub fingerprint: u64,
    pub mode: Mode,
    pub car: Spec,
    pub setup: Setup,
    /// Recorded in a shared-practice session.
    pub multiplayer: bool,
    /// The lap time claimed, in physics steps.
    pub steps: u32,
    /// The sector times claimed, in seconds.
    pub sectors: Vec<f32>,
    pub entry: Entry,
    /// What the engine was given each step, already quantised.
    pub inputs: Vec<Controls>,
}

impl Run {
    pub(crate) fn seconds(&self) -> f32 {
        self.steps as f32 / 240.0
    }

    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(256 + self.inputs.len() / 4);
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&FORMAT.to_le_bytes());
        string(&mut out, &self.app_version);
        out.extend_from_slice(&self.physics.to_le_bytes());
        string(&mut out, &self.circuit);
        out.extend_from_slice(&self.fingerprint.to_le_bytes());
        out.push(index(&Mode::ALL, self.mode));
        out.push(index(&Spec::ALL, self.car));
        out.push(index(&Setup::ALL, self.setup));
        out.push(u8::from(self.multiplayer));
        out.extend_from_slice(&self.steps.to_le_bytes());
        out.push(self.sectors.len() as u8);
        for sector in &self.sectors {
            out.extend_from_slice(&sector.to_le_bytes());
        }
        for value in self.entry.floats() {
            out.extend_from_slice(&value.to_le_bytes());
        }
        let runs = runs(&self.inputs);
        out.extend_from_slice(&(runs.len() as u32).to_le_bytes());
        for (repeat, controls) in runs {
            out.extend_from_slice(&repeat.to_le_bytes());
            let [throttle, brake, steer, handbrake] = bytes(controls);
            out.extend_from_slice(&[throttle, brake, steer, handbrake]);
        }
        out
    }

    /// The run in `bytes`, if it is one, whole and within bounds.
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() > MAX_BYTES {
            return Err("too large");
        }
        let mut at = Reader { bytes, at: 0 };
        if at.take(8)? != MAGIC {
            return Err("not a run");
        }
        if at.u16()? != FORMAT {
            return Err("another run format");
        }
        let app_version = at.string()?;
        let physics = at.u32()?;
        let circuit = at.string()?;
        let fingerprint = at.u64()?;
        let mode = *Mode::ALL.get(at.u8()? as usize).ok_or("no such mode")?;
        let car = *Spec::ALL.get(at.u8()? as usize).ok_or("no such car")?;
        let setup = *Setup::ALL.get(at.u8()? as usize).ok_or("no such setup")?;
        let multiplayer = at.u8()? != 0;
        let steps = at.u32()?;
        if steps == 0 || steps > LONGEST {
            return Err("impossible lap length");
        }
        let sectors = (0..at.u8()?)
            .map(|_| at.f32())
            .collect::<Result<Vec<_>, _>>()?;
        let mut floats = [0.0; 20];
        for value in &mut floats {
            *value = at.f32()?;
        }
        let entry = Entry::from_floats(floats);
        let finite = floats
            .iter()
            .enumerate()
            .all(|(i, f)| f.is_finite() || (i == 13 && f.is_nan()));
        if !finite || !entry.rotation.is_normalized() {
            return Err("impossible entry state");
        }
        let count = at.u32()?;
        let mut inputs = Vec::with_capacity(steps as usize);
        for _ in 0..count {
            let repeat = at.u16()?;
            let controls = controls(at.take(4)?.try_into().expect("four bytes"))?;
            if repeat == 0 || inputs.len() + repeat as usize > steps as usize {
                return Err("inputs do not match the lap length");
            }
            inputs.extend(std::iter::repeat_n(controls, repeat as usize));
        }
        if inputs.len() != steps as usize {
            return Err("inputs do not match the lap length");
        }
        if at.at != bytes.len() {
            return Err("trailing bytes");
        }
        Ok(Self {
            app_version,
            physics,
            circuit,
            fingerprint,
            mode,
            car,
            setup,
            multiplayer,
            steps,
            sectors,
            entry,
            inputs,
        })
    }

    /// A short name for the file and for the server's idempotency check.
    pub(crate) fn hash(bytes: &[u8]) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }
}

fn index<T: PartialEq>(all: &[T], value: T) -> u8 {
    all.iter().position(|v| *v == value).unwrap_or(0) as u8
}

fn string(out: &mut Vec<u8>, value: &str) {
    let bytes = &value.as_bytes()[..value.len().min(255)];
    out.push(bytes.len() as u8);
    out.extend_from_slice(bytes);
}

/// Controls as the four bytes a run keeps.
fn bytes(c: Controls) -> [u8; 4] {
    [
        (c.throttle * 255.0).round() as u8,
        (c.brake * 255.0).round() as u8,
        ((c.steer * 127.0).round() as i8) as u8,
        u8::from(c.handbrake),
    ]
}

fn controls([throttle, brake, steer, handbrake]: [u8; 4]) -> Result<Controls, &'static str> {
    let steer = steer as i8;
    if steer == i8::MIN || handbrake > 1 {
        return Err("impossible input");
    }
    Ok(Controls {
        throttle: f32::from(throttle) / 255.0,
        brake: f32::from(brake) / 255.0,
        steer: f32::from(steer) / 127.0,
        handbrake: handbrake == 1,
    })
}

/// Consecutive identical inputs, as (how many, which).
fn runs(inputs: &[Controls]) -> Vec<(u16, Controls)> {
    let mut out: Vec<(u16, Controls)> = Vec::new();
    for c in inputs {
        match out.last_mut() {
            Some((n, last)) if *last == *c && *n < u16::MAX => *n += 1,
            _ => out.push((1, *c)),
        }
    }
    out
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], &'static str> {
        let end = self.at.checked_add(n).ok_or("cut short")?;
        let slice = self.bytes.get(self.at..end).ok_or("cut short")?;
        self.at = end;
        Ok(slice)
    }
    fn u8(&mut self) -> Result<u8, &'static str> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, &'static str> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().expect("two")))
    }
    fn u32(&mut self) -> Result<u32, &'static str> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("four")))
    }
    fn u64(&mut self) -> Result<u64, &'static str> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("eight")))
    }
    fn f32(&mut self) -> Result<f32, &'static str> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().expect("four")))
    }
    fn string(&mut self) -> Result<String, &'static str> {
        let n = self.u8()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| "not text")
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn sample() -> Run {
        let inputs: Vec<Controls> = (0..2400)
            .map(|i| {
                Controls {
                    throttle: if i % 400 < 300 { 1.0 } else { 0.0 },
                    brake: if i % 400 >= 300 { 0.6 } else { 0.0 },
                    steer: ((i / 50) as f32 * 0.37).sin(),
                    handbrake: i % 1000 == 999,
                }
                .quantised()
            })
            .collect();
        Run {
            app_version: "0.11.0".into(),
            physics: 1,
            circuit: "monza".into(),
            fingerprint: 0x1234_5678_9abc_def0,
            mode: Mode::Pro,
            car: Spec::Clubman,
            setup: Setup::Oversteer,
            multiplayer: false,
            steps: inputs.len() as u32,
            sectors: vec![2.5, 2.5, 2.5, 2.5],
            entry: Entry {
                translation: Vec3::new(1.0, 2.0, 3.0),
                rotation: Quat::from_rotation_y(0.3),
                velocity: Vec3::new(18.0, 0.0, -4.0),
                yaw_rate: 0.1,
                steer_angle: 0.02,
                reversing: false,
                stranded: 0.0,
                along: Some(12.5),
                g_force: Vec2::new(0.1, -0.2),
                slip_angle: 0.01,
                grip_used: 0.4,
                rear_slip: 0.0,
            },
            inputs,
        }
    }

    #[test]
    fn a_run_survives_encoding_to_the_bit() {
        let run = sample();
        let bytes = run.encode();
        assert_eq!(Run::decode(&bytes), Ok(run));
        assert!(bytes.len() < 12_000, "{} bytes", bytes.len());
    }

    #[test]
    fn every_corruption_is_refused() {
        let bytes = sample().encode();
        for cut in [0, 7, 20, bytes.len() / 2, bytes.len() - 1] {
            assert!(Run::decode(&bytes[..cut]).is_err(), "cut at {cut}");
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert_eq!(Run::decode(&longer), Err("trailing bytes"));
        let mut magic = bytes.clone();
        magic[0] = b'X';
        assert_eq!(Run::decode(&magic), Err("not a run"));
        let mut format = bytes.clone();
        format[8] = 9;
        assert_eq!(Run::decode(&format), Err("another run format"));
        let mut steps = sample();
        steps.steps += 1;
        assert!(
            Run::decode(&steps.encode()).is_err(),
            "claimed more steps than inputs"
        );
        assert_eq!(Run::decode(&vec![0; MAX_BYTES + 1]), Err("too large"));
    }

    #[test]
    fn inputs_keep_the_quantised_values_exactly() {
        for raw in [0.0f32, 0.1, 0.5, 0.999, 1.0, -1.0, -0.33] {
            let c = Controls {
                throttle: raw.abs(),
                brake: raw.abs(),
                steer: raw,
                handbrake: true,
            }
            .quantised();
            assert_eq!(controls(bytes(c)), Ok(c));
        }
    }
}
