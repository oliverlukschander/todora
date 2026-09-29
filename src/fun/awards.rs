//! Awards for being ridiculous.
//!
//! A second set of achievements for the things the fun layer adds — honking,
//! hitting cows, bowling a strike, staying in the air, a combo of eight, being
//! visited by weather — kept in the same `achievements.json` as the rest, toasted
//! the same way, and listed with them in the Awards view, but only when the game
//! is at least Silly. The serious catalogue is not changed by them: there are
//! still seventy-one of those, and a test says so.
//!
//! The counters are cumulative and live in [`Earned::fun`], so an award for two
//! hundred and fifty honks can be earned over many evenings. Which awards a set
//! of counters has earned is a plain function, [`earned_by`], and it is what the
//! tests look at.

use std::collections::BTreeSet;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::announcer::{Score, Stats};
use super::{Fun, Mount};
use crate::achievements::{Earned, Toast, announce};
use crate::lap::LapFinished;

/// Every silly award, by id. Their names and what they ask for are in the text
/// table under the same ids.
pub(crate) const IDS: [&str; 15] = [
    "fun-honk-25",
    "fun-honk-250",
    "fun-crow",
    "fun-cow-1",
    "fun-cow-25",
    "fun-strike",
    "fun-air-60",
    "fun-air-600",
    "fun-combo",
    "fun-score-10k",
    "fun-score-50k",
    "fun-chaos-10",
    "fun-balloons-50",
    "fun-boxes-20",
    "fun-mounts",
];

/// What has been done, added up over every session.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub(crate) struct Counts {
    pub honks: u32,
    pub crows: u32,
    pub cows: u32,
    pub strikes: u32,
    pub balloons: u32,
    pub boxes: u32,
    pub events: u32,
    /// Seconds in the air.
    pub airtime: f32,
    /// The biggest combo, and the biggest score in one lap.
    pub combo: u32,
    pub lap_score: u64,
    /// Rides that have finished a lap.
    pub mounts: BTreeSet<String>,
}

/// Which awards these counts have earned.
pub(crate) fn earned_by(counts: &Counts) -> Vec<&'static str> {
    let mut ids = Vec::new();
    let mut when = |condition: bool, id: &'static str| {
        if condition {
            ids.push(id);
        }
    };
    when(counts.honks >= 25, "fun-honk-25");
    when(counts.honks >= 250, "fun-honk-250");
    when(counts.crows >= 1, "fun-crow");
    when(counts.cows >= 1, "fun-cow-1");
    when(counts.cows >= 25, "fun-cow-25");
    when(counts.strikes >= 1, "fun-strike");
    when(counts.airtime >= 60.0, "fun-air-60");
    when(counts.airtime >= 600.0, "fun-air-600");
    when(counts.combo >= 8, "fun-combo");
    when(counts.lap_score >= 10_000, "fun-score-10k");
    when(counts.lap_score >= 50_000, "fun-score-50k");
    when(counts.events >= 10, "fun-chaos-10");
    when(counts.balloons >= 50, "fun-balloons-50");
    when(counts.boxes >= 20, "fun-boxes-20");
    when(counts.mounts.len() >= Mount::ALL.len(), "fun-mounts");
    ids
}

/// The silly awards as the Awards view lists them: id, name, what it asks for.
pub(crate) fn listed() -> Vec<(String, String, String)> {
    IDS.iter()
        .map(|id| {
            let (name, what) = crate::text::achievement(id).unwrap_or((id, ""));
            (id.to_string(), name.to_string(), what.to_string())
        })
        .collect()
}

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Update, (count, per_lap).chain().run_if(super::silly));
}

/// Fold what was done this frame into the totals, and see what it earned.
fn count(
    score: Res<Score>,
    mut earned: ResMut<Earned>,
    mut toast: ResMut<Toast>,
    mut seen: Local<Stats>,
) {
    let now = score.stats;
    let fresh = Stats {
        honks: now.honks.saturating_sub(seen.honks),
        crows: now.crows.saturating_sub(seen.crows),
        cows: now.cows.saturating_sub(seen.cows),
        strikes: now.strikes.saturating_sub(seen.strikes),
        balloons: now.balloons.saturating_sub(seen.balloons),
        boxes: now.boxes.saturating_sub(seen.boxes),
        events: now.events.saturating_sub(seen.events),
        airtime: (now.airtime - seen.airtime).max(0.0),
        ..Stats::default()
    };
    let nothing_new = fresh == Stats::default()
        && earned.fun.combo >= now.combo_peak
        && earned.fun.lap_score >= score.lap;
    *seen = now;
    if nothing_new {
        return;
    }
    let counts = &mut earned.fun;
    counts.honks += fresh.honks;
    counts.crows += fresh.crows;
    counts.cows += fresh.cows;
    counts.strikes += fresh.strikes;
    counts.balloons += fresh.balloons;
    counts.boxes += fresh.boxes;
    counts.events += fresh.events;
    counts.airtime += fresh.airtime;
    counts.combo = counts.combo.max(now.combo_peak);
    counts.lap_score = counts.lap_score.max(score.lap);
    let ids = earned_by(&earned.fun);
    announce(&mut earned, &mut toast, &ids);
}

/// A finished lap counts its ride, if it is one that has not been ridden.
fn per_lap(
    mut laps: MessageReader<LapFinished>,
    fun: Res<Fun>,
    mut earned: ResMut<Earned>,
    mut toast: ResMut<Toast>,
) {
    if laps.read().next().is_none() {
        return;
    }
    let ride = format!("{:?}", fun.mount).to_lowercase();
    if earned.fun.mounts.insert(ride) {
        let ids = earned_by(&earned.fun);
        announce(&mut earned, &mut toast, &ids);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_earned_by_nothing() {
        assert!(earned_by(&Counts::default()).is_empty());
    }

    #[test]
    fn each_award_arrives_at_its_threshold_and_not_before() {
        let at = |f: fn(&mut Counts)| {
            let mut c = Counts::default();
            f(&mut c);
            earned_by(&c)
        };
        assert_eq!(at(|c| c.honks = 24), Vec::<&str>::new());
        assert_eq!(at(|c| c.honks = 25), ["fun-honk-25"]);
        assert_eq!(at(|c| c.honks = 250), ["fun-honk-25", "fun-honk-250"]);
        assert_eq!(at(|c| c.cows = 1), ["fun-cow-1"]);
        assert_eq!(at(|c| c.strikes = 1), ["fun-strike"]);
        assert!(at(|c| c.airtime = 59.9).is_empty());
        assert_eq!(at(|c| c.airtime = 60.0), ["fun-air-60"]);
        assert_eq!(at(|c| c.combo = 7), Vec::<&str>::new());
        assert_eq!(at(|c| c.combo = 8), ["fun-combo"]);
        assert_eq!(at(|c| c.lap_score = 10_000), ["fun-score-10k"]);
        assert_eq!(at(|c| c.crows = 1), ["fun-crow"]);
    }

    #[test]
    fn a_ride_of_every_kind_earns_the_last_one() {
        let mut c = Counts::default();
        for mount in Mount::ALL {
            assert!(!earned_by(&c).contains(&"fun-mounts"));
            c.mounts.insert(format!("{mount:?}").to_lowercase());
        }
        assert!(earned_by(&c).contains(&"fun-mounts"));
    }

    #[test]
    fn every_id_can_be_earned_and_is_named_in_every_language() {
        let everything = Counts {
            honks: 999,
            crows: 9,
            cows: 99,
            strikes: 9,
            balloons: 99,
            boxes: 99,
            events: 99,
            airtime: 9_999.0,
            combo: 8,
            lap_score: 99_999,
            mounts: Mount::ALL
                .iter()
                .map(|m| format!("{m:?}").to_lowercase())
                .collect(),
        };
        let mut earned = earned_by(&everything);
        earned.sort_unstable();
        let mut all = IDS.to_vec();
        all.sort_unstable();
        assert_eq!(earned, all, "an award nobody can earn");
        for (id, name, what) in listed() {
            assert!(!name.is_empty() && !what.is_empty(), "{id} has no words");
        }
    }
}
