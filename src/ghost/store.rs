//! Your best lap, kept between sessions.
//!
//! A [`super::Recording`] is a few thousand poses against a clock, which is a
//! few hundred kilobytes of `f32` and nothing else — so it is written as `f32`
//! and nothing else, little-endian, behind a short header. No format to keep up
//! with, and nothing to parse that is not a number.
//!
//! The header carries the fingerprint of the circuit the lap was driven round.
//! A lap only means something against the shape it was set on: move the
//! centreline a metre, or scale the world differently, and the saved car drives
//! through the scenery and the delta counts against a lap nobody could have
//! driven. So a file whose fingerprint has moved on is quietly not a lap any
//! more, and the driver starts again with a clean board.
//!
//! Nothing in here is allowed to be a problem. A machine with nowhere to save,
//! a file that has been half written or edited, a folder that cannot be made —
//! each one means no ghost, said once in the log, and a game that carries on.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use bevy::prelude::*;

use super::Recording;
use crate::car::Mode;
use crate::track::Track;

/// `TODORA LAP`, version 2 (four-wheel track limits). A file that does not start with this is not ours.
const MAGIC: [u8; 8] = *b"TODORAL2";
/// What one sample is written as: the clock, how far round, three of position,
/// four of rotation.
const FIELDS: usize = 9;
const HEADER: usize = MAGIC.len() + size_of::<u64>() + size_of::<u32>();

/// Where one circuit's best lap is kept, and the shape it has to have been
/// driven round to count.
pub(super) struct Saved {
    path: PathBuf,
    fingerprint: u64,
}

/// `TODORA SECTORS`, version 1: the best time ever driven in each sector.
const SECTOR_MAGIC: [u8; 8] = *b"TODORAS1";

impl Saved {
    /// Where this circuit's lap lives, or `None` on a machine that will not say
    /// where a game may keep things.
    pub(super) fn of(track: &Track, mode: Mode) -> Option<Self> {
        Some(Self {
            path: folder()?.join(filename(track.circuit().id, mode)),
            fingerprint: track.fingerprint(),
        })
    }

    pub(super) fn remove(&self) -> io::Result<()> {
        remove_file(&self.path)?;
        remove_file(&self.path.with_extension("sectors"))
    }

    /// The best time ever driven in each of `count` sectors, if they were
    /// saved for this circuit as it stands.
    pub(super) fn read_sectors(&self, count: usize) -> Option<Vec<f32>> {
        let bytes = fs::read(self.path.with_extension("sectors")).ok()?;
        decode_sectors(&bytes, self.fingerprint, count)
    }

    /// Keep the best sectors. Tiny, and written the same careful way as a lap.
    pub(super) fn write_sectors(&self, sectors: &[f32]) {
        let path = self.path.with_extension("sectors");
        let beside = path.with_extension("sectors-writing");
        let written = fs::create_dir_all(path.parent().unwrap_or(&path))
            .and_then(|()| fs::write(&beside, encode_sectors(sectors, self.fingerprint)))
            .and_then(|()| fs::rename(&beside, &path));
        if let Err(trouble) = written {
            warn!(
                "cannot save the best sectors to {}: {trouble}",
                path.display()
            );
        }
    }

    /// The lap saved here, if there is one and it is still a lap of this circuit.
    pub(super) fn read(&self) -> Option<Recording> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            // Nothing saved yet is the ordinary case, not a problem.
            Err(trouble) if trouble.kind() == io::ErrorKind::NotFound => return None,
            Err(trouble) => {
                warn!("cannot read {}: {trouble}", self.path.display());
                return None;
            }
        };
        let lap = decode(&bytes, self.fingerprint);
        if lap.is_none() {
            info!(
                "{} is not a lap of this circuit as it stands; starting fresh",
                self.path.display()
            );
        }
        lap
    }

    /// Keep this lap as the one to beat. Written beside the real file and moved
    /// onto it, so a crash halfway through loses the new lap rather than the old
    /// one.
    pub(super) fn write(&self, lap: &Recording) {
        let beside = self.path.with_extension("writing");
        let written = fs::create_dir_all(self.path.parent().unwrap_or(&self.path))
            .and_then(|()| fs::write(&beside, encode(lap, self.fingerprint)))
            .and_then(|()| fs::rename(&beside, &self.path));
        if let Err(trouble) = written {
            warn!("cannot save the lap to {}: {trouble}", self.path.display());
        }
    }
}

fn filename(id: &str, mode: Mode) -> String {
    if mode == Mode::Regular {
        format!("{id}.lap")
    } else {
        format!("{id}-{}.lap", mode.name().to_lowercase())
    }
}

fn remove_file(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

pub(super) fn remove_all() -> io::Result<()> {
    folder().map_or(Ok(()), |folder| remove_all_in(&folder))
}

fn remove_all_in(folder: &Path) -> io::Result<()> {
    // Only the circuit/mode files owned by the game; leave other files alone.
    for circuit in crate::track::all_circuits() {
        for mode in Mode::ALL {
            let lap = folder.join(filename(circuit.id, mode));
            remove_file(&lap)?;
            remove_file(&lap.with_extension("sectors"))?;
        }
    }
    Ok(())
}

fn encode_sectors(sectors: &[f32], fingerprint: u64) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(20 + sectors.len() * 4);
    bytes.extend_from_slice(&SECTOR_MAGIC);
    bytes.extend_from_slice(&fingerprint.to_le_bytes());
    bytes.extend_from_slice(&(sectors.len() as u32).to_le_bytes());
    for sector in sectors {
        bytes.extend_from_slice(&sector.to_le_bytes());
    }
    bytes
}

fn decode_sectors(bytes: &[u8], fingerprint: u64, count: usize) -> Option<Vec<f32>> {
    if bytes.len() != 20 + count * 4 || bytes[..8] != SECTOR_MAGIC {
        return None;
    }
    if u64::from_le_bytes(bytes[8..16].try_into().ok()?) != fingerprint
        || u32::from_le_bytes(bytes[16..20].try_into().ok()?) as usize != count
    {
        return None;
    }
    let sectors: Vec<f32> = bytes[20..]
        .chunks_exact(4)
        .map(|four| f32::from_le_bytes(four.try_into().expect("four bytes make an f32")))
        .collect();
    sectors
        .iter()
        .all(|s| s.is_finite() && *s > 0.0)
        .then_some(sectors)
}

/// Where this machine keeps what a game saves.
fn folder() -> Option<PathBuf> {
    #[cfg(all(target_os = "macos", feature = "game-center"))]
    let base = crate::multiplayer::support_directory();
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(not(any(
        target_os = "windows",
        all(target_os = "macos", feature = "game-center")
    )))]
    let base = {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        if cfg!(target_os = "macos") {
            home.map(|home| home.join("Library/Application Support"))
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| home.map(|home| home.join(".local/share")))
        }
    };
    Some(base?.join("Todora").join("laps"))
}

fn encode(lap: &Recording, fingerprint: u64) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(HEADER + lap.samples.len() * FIELDS * size_of::<f32>());
    bytes.extend_from_slice(&MAGIC);
    bytes.extend_from_slice(&fingerprint.to_le_bytes());
    bytes.extend_from_slice(&(lap.samples.len() as u32).to_le_bytes());
    for sample in &lap.samples {
        let (at, turn) = (sample.translation, sample.rotation);
        for value in [
            sample.time,
            sample.progress,
            at.x,
            at.y,
            at.z,
            turn.x,
            turn.y,
            turn.z,
            turn.w,
        ] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes
}

/// The lap in `bytes`, if it is one: ours, this circuit's, the length it says
/// it is, and a lap a car could have driven.
///
/// The last of those is the one that matters. [`Recording`] is searched by
/// binary search over both the clock and how far round the lap is, so both have
/// to climb; a file that says otherwise would not crash anything, it would
/// quietly answer the wrong question. Everything is checked here so that the
/// rest of the ghost can go on trusting a `Recording` the way it trusts one it
/// recorded itself.
fn decode(bytes: &[u8], fingerprint: u64) -> Option<Recording> {
    if bytes.len() < HEADER || bytes[..MAGIC.len()] != MAGIC {
        return None;
    }
    if u64::from_le_bytes(bytes[8..16].try_into().ok()?) != fingerprint {
        return None;
    }
    let count = u32::from_le_bytes(bytes[16..HEADER].try_into().ok()?) as usize;
    if count < 2 || bytes.len() != HEADER + count * FIELDS * size_of::<f32>() {
        return None;
    }

    let mut values = bytes[HEADER..]
        .chunks_exact(size_of::<f32>())
        .map(|four| f32::from_le_bytes(four.try_into().expect("four bytes make an f32")));
    let mut lap = Recording::default();
    for _ in 0..count {
        let [time, progress, x, y, z, i, j, k, w]: [f32; FIELDS] =
            std::array::from_fn(|_| values.next().unwrap_or(f32::NAN));
        let translation = Vec3::new(x, y, z);
        let rotation = Quat::from_xyzw(i, j, k, w);
        if !time.is_finite()
            || !progress.is_finite()
            || !translation.is_finite()
            || !rotation.is_finite()
            || !rotation.is_normalized()
        {
            return None;
        }
        // The clock only ever runs forwards. Progress is held from running
        // backwards by `push`, the same as when the lap was driven.
        if lap.samples.last().is_some_and(|last| time < last.time) {
            return None;
        }
        lap.push(
            time,
            progress,
            &Transform {
                translation,
                rotation,
                ..default()
            },
        );
    }
    (!lap.full && lap.duration() > 0.0).then_some(lap)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lap of `n` samples, a tenth of a second apart, going somewhere.
    fn lap_of(n: usize) -> Recording {
        let mut lap = Recording::default();
        for i in 0..n {
            let t = i as f32;
            lap.push(
                0.1 * t,
                t / (n - 1) as f32,
                &Transform::from_xyz(t, 0.5, -t).with_rotation(Quat::from_rotation_y(0.01 * t)),
            );
        }
        lap
    }

    #[test]
    fn best_sectors_round_trip_and_refuse_another_layout_or_count() {
        let bytes = encode_sectors(&[9.5, 10.25, 11.0], 77);
        assert_eq!(decode_sectors(&bytes, 77, 3), Some(vec![9.5, 10.25, 11.0]));
        assert_eq!(decode_sectors(&bytes, 78, 3), None, "another layout");
        assert_eq!(decode_sectors(&bytes, 77, 4), None, "another sector count");
        assert_eq!(decode_sectors(&bytes[..19], 77, 3), None, "cut short");
        let mut broken = bytes.clone();
        broken[20..24].copy_from_slice(&f32::NAN.to_le_bytes());
        assert_eq!(decode_sectors(&broken, 77, 3), None, "not a time");
    }

    #[test]
    fn pre_track_limits_ghosts_cannot_become_valid_references() {
        let mut bytes = encode(&lap_of(10), 123);
        bytes[..8].copy_from_slice(b"TODORAL1");
        assert!(decode(&bytes, 123).is_none());
    }

    #[test]
    fn a_lap_survives_the_round_trip() {
        let lap = lap_of(64);
        let back = decode(&encode(&lap, 7), 7).expect("a lap we just wrote");
        assert_eq!(back.samples.len(), lap.samples.len());
        assert_eq!(back.duration(), lap.duration());
        for (was, is) in lap.samples.iter().zip(&back.samples) {
            assert_eq!(was.time, is.time);
            assert_eq!(was.progress, is.progress);
            assert_eq!(was.translation, is.translation);
            // Rotations come back as themselves, to the last bit of an f32.
            assert!(was.rotation.abs_diff_eq(is.rotation, 1e-6));
        }
    }

    /// A lap set on a circuit that has since moved is not a lap of this one.
    #[test]
    fn a_lap_of_another_shape_is_refused() {
        let bytes = encode(&lap_of(16), 7);
        assert!(decode(&bytes, 7).is_some());
        assert!(decode(&bytes, 8).is_none());
    }

    /// Nothing a file can say makes a `Recording` the rest of the ghost cannot
    /// trust: not a truncation, not a header from somewhere else, not a clock
    /// that runs backwards, and not a number that is not one.
    #[test]
    fn a_broken_file_is_not_a_lap() {
        let good = encode(&lap_of(16), 7);
        assert!(decode(&good, 7).is_some());

        assert!(decode(&[], 7).is_none(), "nothing at all");
        assert!(decode(&good[..HEADER], 7).is_none(), "a header and no lap");
        assert!(decode(&good[..good.len() - 1], 7).is_none(), "cut short");
        let mut trailing = good.clone();
        trailing.push(0);
        assert!(decode(&trailing, 7).is_none(), "one byte too many");

        let mut alien = good.clone();
        alien[..MAGIC.len()].copy_from_slice(b"SOMEONE1");
        assert!(decode(&alien, 7).is_none(), "somebody else's file");

        let mut counted = good.clone();
        counted[16..HEADER].copy_from_slice(&9999u32.to_le_bytes());
        assert!(decode(&counted, 7).is_none(), "more samples than bytes");

        // The clock of the last sample, put back before the one before it.
        let mut backwards = good.clone();
        let last = backwards.len() - FIELDS * size_of::<f32>();
        backwards[last..last + 4].copy_from_slice(&0.0f32.to_le_bytes());
        assert!(decode(&backwards, 7).is_none(), "a clock running backwards");

        for spoiled in [f32::NAN, f32::INFINITY] {
            let mut broken = good.clone();
            broken[HEADER..HEADER + 4].copy_from_slice(&spoiled.to_le_bytes());
            assert!(decode(&broken, 7).is_none(), "a clock reading {spoiled}");
        }

        // A rotation that is not a rotation would point the ghost nowhere.
        let mut flat = good.clone();
        let turn = HEADER + 5 * size_of::<f32>();
        flat[turn..turn + 4 * size_of::<f32>()].fill(0);
        assert!(decode(&flat, 7).is_none(), "a rotation of nothing");
    }

    /// The whole way out and back, through a real folder that does not exist
    /// yet: the lap is written, read back as itself, and a lap of another shape
    /// left in its place is not mistaken for it.
    #[test]
    fn a_lap_goes_to_disk_and_comes_back() {
        let folder = std::env::temp_dir().join(format!(
            "todora-store-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let saved = Saved {
            path: folder.join("somewhere.lap"),
            fingerprint: 12345,
        };
        assert!(saved.read().is_none(), "a lap from nowhere");

        let lap = lap_of(200);
        saved.write(&lap);
        let back = saved.read().expect("the lap just written");
        assert_eq!(back.samples.len(), lap.samples.len());
        assert_eq!(back.duration(), lap.duration());
        assert!(
            !folder.join("somewhere.writing").exists(),
            "the half-written file was left behind"
        );

        // The same file, saved when the circuit was a different shape.
        fs::write(&saved.path, encode(&lap, 999)).expect("a writable folder");
        assert!(saved.read().is_none(), "a lap of another shape was taken");

        fs::remove_dir_all(&folder).expect("a folder we made");
    }

    #[test]
    fn reset_removes_only_requested_saves_and_reports_errors() {
        let folder = std::env::temp_dir().join(format!(
            "todora-reset-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&folder).unwrap();
        for circuit in crate::track::all_circuits() {
            for mode in Mode::ALL {
                fs::write(folder.join(filename(circuit.id, mode)), b"saved ghost").unwrap();
            }
        }
        fs::write(folder.join("notes.txt"), b"keep me").unwrap();
        let circuit = &crate::track::all_circuits()[0];
        let saved = Saved {
            path: folder.join(filename(circuit.id, Mode::Regular)),
            fingerprint: 0,
        };
        saved.remove().unwrap();
        saved.remove().unwrap(); // Resetting an empty slot is fine.
        assert!(!saved.path.exists());
        assert_eq!(
            fs::read_dir(&folder).unwrap().count(),
            crate::track::all_circuits().len() * Mode::ALL.len()
        );
        remove_all_in(&folder).unwrap();
        assert_eq!(fs::read_dir(&folder).unwrap().count(), 1);
        assert_eq!(fs::read(folder.join("notes.txt")).unwrap(), b"keep me");
        // A directory in place of a file is a portable removal failure, even as root.
        fs::create_dir(&saved.path).unwrap();
        assert!(saved.remove().is_err());
        assert!(remove_all_in(&folder).is_err());
        fs::remove_dir_all(folder).unwrap();
    }

    /// Every circuit files its lap somewhere of its own.
    #[test]
    fn each_circuit_and_mode_has_its_own_file() {
        let Some(folder) = folder() else {
            return;
        };
        assert!(folder.is_absolute(), "{} is not a place", folder.display());
        let mut paths: Vec<PathBuf> = crate::track::all_circuits()
            .iter()
            .flat_map(|circuit| {
                let track = Track::new(circuit);
                Mode::ALL
                    .into_iter()
                    .filter_map(move |mode| Saved::of(&track, mode).map(|saved| saved.path))
            })
            .collect();
        let filed = paths.len();
        paths.sort();
        paths.dedup();
        assert_eq!(filed, crate::track::all_circuits().len() * 3);
        assert_eq!(
            paths.len(),
            filed,
            "two circuit/mode combinations share a file"
        );
        let track = Track::any();
        assert_eq!(
            Saved::of(&track, Mode::Regular).unwrap().path,
            folder.join(format!("{}.lap", track.circuit().id)),
            "Regular must keep the existing saved laps"
        );
    }
}
