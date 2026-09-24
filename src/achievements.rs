//! Fifty things to have done, noticed when they happen and kept for good.
//!
//! Each is checked only when something happens that could earn it — a lap
//! finishing, a sector closing, a lap reaching the board — and never every
//! frame, except the distance, which is a sum. Earned ones are kept in
//! `achievements.json` beside the laps with the time they were earned, along
//! with what they are counted from (circuits lapped, modes and cars used,
//! distance), written off the main thread a second after anything changes. A
//! toast says so when one is earned; the leaderboard's **Awards** view lists
//! them all.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::car::{Car, Mode, Player, Spec};
use crate::lap::{LapFinished, LapSet, LapTimer, Split};
use crate::medals::Medal;
use crate::track::{Track, all_circuits};

const FILE: &str = "achievements.json";
const VERSION: u32 = 1;

/// The circuits with a gold of their own to earn.
const GOLD_AT: [&str; 16] = [
    "monaco",
    "spa-francorchamps",
    "suzuka",
    "monza",
    "silverstone",
    "interlagos",
    "imola",
    "nurburgring",
    "zandvoort",
    "baku",
    "red-bull-ring",
    "hungaroring",
    "barcelona-catalunya",
    "americas",
    "marina-bay",
    "jeddah",
];

/// Every achievement: its id, its name and what it asks for.
pub(crate) fn all() -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = [
        ("first-lap", "Off the line", "Finish a lap"),
        ("first-valid", "By the book", "Finish a lap that counts"),
        ("first-best", "Personal best", "Beat your own best lap"),
        ("circuits-5", "Tourist", "Finish a lap on 5 circuits"),
        (
            "circuits-20",
            "Travelling circus",
            "Finish a lap on 20 circuits",
        ),
        (
            "circuits-40",
            "The whole calendar",
            "Finish a lap on all 40 circuits",
        ),
        ("bronze", "On the podium", "Earn a bronze medal"),
        ("silver", "Silverware", "Earn a silver medal"),
        ("gold", "Gold", "Earn a gold medal"),
        ("author", "Author, author", "Beat an author time"),
        ("gold-5", "Five golds", "Gold on 5 circuits in one mode"),
        ("gold-20", "Twenty golds", "Gold on 20 circuits in one mode"),
        ("gold-40", "Midas", "Gold on every circuit in one mode"),
        ("author-5", "Ghost writer", "Beat 5 author times"),
        ("pro", "Pro", "Finish a lap that counts in Pro"),
        (
            "beginner",
            "Everyone starts somewhere",
            "Finish a lap that counts in Beginner",
        ),
        (
            "modes",
            "Three speeds",
            "Laps that count in all three modes on one circuit",
        ),
        ("cars", "Garage tour", "Laps that count in all three cars"),
        ("clean-5", "Consistent", "Five laps in a row that count"),
        ("clean-10", "Metronome", "Ten laps in a row that count"),
        ("purple", "Purple patch", "Drive a sector faster than ever"),
        ("all-purple", "Purple reign", "Every sector of a lap purple"),
        ("km-10", "Warm-up", "Drive 10 km"),
        ("km-100", "Long run", "Drive 100 km"),
        ("km-1000", "Endurance", "Drive 1,000 km"),
        ("online", "Hello, world", "Put a lap on the world boards"),
        ("top-100", "Top hundred", "Place in the top 100 of a board"),
        ("top-10", "Top ten", "Place in the top 10 of a board"),
        ("record", "World record", "Set a world record"),
        (
            "ghost-beaten",
            "Ghostbuster",
            "Beat a downloaded ghost's lap",
        ),
        (
            "weekly",
            "This week's",
            "Set a time in the weekly challenge",
        ),
        (
            "monaco-clean",
            "Harbour master",
            "A lap of Harbour Streets that counts",
        ),
        (
            "suzuka-clean",
            "Figure of eight",
            "A lap of Figure-Eight Hills that counts",
        ),
        (
            "baku-clean",
            "Old town",
            "A lap of Caspian Old Town that counts",
        ),
    ]
    .into_iter()
    .map(|(id, name, what)| {
        let (name, what) = crate::text::achievement(id).unwrap_or((name, what));
        (id.to_string(), name.to_string(), what.to_string())
    })
    .collect();
    for id in GOLD_AT {
        let name = all_circuits()
            .iter()
            .find(|c| c.id == id)
            .map_or(id, |c| c.name);
        out.push((
            format!("gold-{id}"),
            crate::text::tf("ach.gold_at", &[&name]),
            crate::text::tf("ach.gold_at_what", &[&name]),
        ));
    }
    out
}

/// What is kept: what was earned and when, and what it is counted from.
#[derive(Resource, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub(crate) struct Earned {
    pub version: u32,
    /// Id to Unix seconds.
    pub earned: BTreeMap<String, i64>,
    pub metres: f64,
    /// Circuits with a finished lap.
    pub lapped: BTreeSet<String>,
    /// Circuit to the modes with a lap that counts there.
    pub modes: BTreeMap<String, BTreeSet<String>>,
    /// Cars with a lap that counts.
    pub cars: BTreeSet<String>,
    /// Laps in a row that counted, this session and before.
    pub streak: u32,
    /// Author times beaten, by circuit and mode.
    pub authors: BTreeSet<String>,
}

impl Earned {
    /// Mark `id` earned now, if it was not; true when it is new.
    pub(crate) fn earn(&mut self, id: &str, now: i64) -> bool {
        if self.earned.contains_key(id) {
            return false;
        }
        self.earned.insert(id.to_string(), now);
        true
    }
}

/// Everything a finished lap can earn, as ids, given what was already counted.
#[allow(clippy::too_many_arguments)]
pub(crate) fn from_lap(
    earned: &mut Earned,
    lap: &LapFinished,
    circuit: &str,
    mode: Mode,
    car: Spec,
    splits: &[Split],
    medals_here: Option<Medal>,
    golds_in_mode: usize,
    beat_ghost: bool,
) -> Vec<String> {
    let mut out: Vec<String> = vec!["first-lap".into()];
    earned.lapped.insert(circuit.to_string());
    let lapped = earned.lapped.len();
    for (n, id) in [(5, "circuits-5"), (20, "circuits-20"), (40, "circuits-40")] {
        if lapped >= n {
            out.push(id.into());
        }
    }
    if !lap.valid {
        earned.streak = 0;
        return out;
    }
    out.push("first-valid".into());
    earned.streak += 1;
    if earned.streak >= 5 {
        out.push("clean-5".into());
    }
    if earned.streak >= 10 {
        out.push("clean-10".into());
    }
    if lap.best {
        out.push("first-best".into());
    }
    let mode_name = mode.name().to_lowercase();
    match mode {
        Mode::Pro => out.push("pro".into()),
        Mode::Beginner => out.push("beginner".into()),
        Mode::Regular => {}
    }
    let modes = earned.modes.entry(circuit.to_string()).or_default();
    modes.insert(mode_name.clone());
    if modes.len() == 3 {
        out.push("modes".into());
    }
    earned.cars.insert(format!("{car:?}").to_lowercase());
    if earned.cars.len() == 3 {
        out.push("cars".into());
    }
    if let Some(medal) = medals_here {
        out.push("bronze".into());
        if medal >= Medal::Silver {
            out.push("silver".into());
        }
        if medal >= Medal::Gold {
            out.push("gold".into());
            if let Some(id) = GOLD_AT.iter().find(|id| **id == circuit) {
                out.push(format!("gold-{id}"));
            }
        }
        if medal == Medal::Author {
            out.push("author".into());
            earned.authors.insert(format!("{circuit}-{mode_name}"));
            if earned.authors.len() >= 5 {
                out.push("author-5".into());
            }
        }
    }
    for (n, id) in [(5, "gold-5"), (20, "gold-20"), (40, "gold-40")] {
        if golds_in_mode >= n {
            out.push(id.into());
        }
    }
    if !splits.is_empty() && splits.iter().all(|s| *s == Split::Purple) {
        out.push("all-purple".into());
    }
    if beat_ghost {
        out.push("ghost-beaten".into());
    }
    match circuit {
        "monaco" => out.push("monaco-clean".into()),
        "suzuka" => out.push("suzuka-clean".into()),
        "baku" => out.push("baku-clean".into()),
        _ => {}
    }
    out
}

pub struct AchievementsPlugin;

impl Plugin for AchievementsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(load())
            .init_resource::<Toast>()
            .add_systems(Startup, setup)
            .add_systems(FixedUpdate, laps.after(LapSet))
            .add_systems(Update, (sectors, distance, online, draw, save).chain());
    }
}

fn path() -> Option<std::path::PathBuf> {
    Some(crate::settings::data_dir()?.join(FILE))
}

fn load() -> Earned {
    let earned = path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|text| serde_json::from_str::<Earned>(&text).ok())
        .unwrap_or_default();
    Earned {
        version: VERSION,
        ..earned
    }
}

/// Newly earned ones waiting to be shown, and the one being shown.
#[derive(Resource, Default)]
struct Toast {
    waiting: Vec<String>,
    showing: Option<(String, f32)>,
}

fn announce(earned: &mut Earned, toast: &mut Toast, ids: &[impl AsRef<str>]) {
    let now = crate::online::client::unix_now();
    for id in ids {
        let id = id.as_ref();
        if earned.earn(id, now)
            && let Some((_, name, _)) = all().into_iter().find(|(i, _, _)| i == id)
        {
            toast.waiting.push(name);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn laps(
    mut finished: MessageReader<LapFinished>,
    track: Res<Track>,
    mode: Res<Mode>,
    spec: Res<Spec>,
    timer: Res<LapTimer>,
    records: Option<Res<crate::ghost::Records>>,
    rival: Option<Res<crate::ghost::Rival>>,
    challenge: Option<Res<crate::challenge::Challenge>>,
    mut earned: ResMut<Earned>,
    mut toast: ResMut<Toast>,
) {
    for lap in finished.read() {
        let circuit = track.circuit().id;
        let targets = crate::medals::targets(&track, *mode);
        let best = timer.best.map_or(lap.time, |b| b.min(lap.time));
        let medal = targets.and_then(|t| lap.valid.then(|| t.medal(best)).flatten());
        let golds = records.as_ref().map_or(0, |records| {
            (0..all_circuits().len())
                .filter(|&at| {
                    let best = records.best(at, *mode);
                    crate::medals::targets_for(all_circuits()[at].id, *mode)
                        .and_then(|(t, _)| best.and_then(|b| t.medal(b)))
                        .is_some_and(|m| m >= Medal::Gold)
                })
                .count()
        });
        let beat = rival
            .as_ref()
            .and_then(|r| r.lap_time())
            .is_some_and(|theirs| lap.valid && lap.time < theirs);
        let splits = timer
            .report
            .as_ref()
            .map(|r| r.splits.clone())
            .unwrap_or_default();
        let mut ids = from_lap(
            &mut earned,
            lap,
            circuit,
            *mode,
            *spec,
            &splits,
            medal,
            golds,
            beat,
        );
        if lap.valid && challenge.as_ref().is_some_and(|c| c.counts(circuit, *mode)) {
            ids.push("weekly".into());
        }
        announce(&mut earned, &mut toast, &ids);
    }
}

fn sectors(timer: Res<LapTimer>, mut earned: ResMut<Earned>, mut toast: ResMut<Toast>) {
    if timer.is_changed() && timer.splits.last() == Some(&Split::Purple) {
        announce(&mut earned, &mut toast, &["purple"]);
    }
}

/// Distance is the one thing summed every frame: one multiply and add.
fn distance(
    time: Res<Time>,
    cars: Query<&Car, With<Player>>,
    mut earned: ResMut<Earned>,
    mut toast: ResMut<Toast>,
    mut counted: Local<f64>,
) {
    let Ok(car) = cars.single() else {
        return;
    };
    *counted += f64::from(car.velocity.length() * time.delta_secs());
    // Folded in once a kilometre-tenth, so the file is not rewritten each frame.
    if *counted < 100.0 {
        return;
    }
    earned.metres += *counted;
    *counted = 0.0;
    let km = earned.metres / 1000.0;
    let ids: Vec<&str> = [(10.0, "km-10"), (100.0, "km-100"), (1000.0, "km-1000")]
        .into_iter()
        .filter(|(n, _)| km >= *n)
        .map(|(_, id)| id)
        .collect();
    announce(&mut earned, &mut toast, &ids);
}

fn online(
    online: Option<ResMut<crate::online::Online>>,
    mut earned: ResMut<Earned>,
    mut toast: ResMut<Toast>,
) {
    let Some(mut online) = online else {
        return;
    };
    for sent in std::mem::take(&mut online.sent) {
        let mut ids = vec!["online"];
        match sent.rank {
            Some(1) => ids.extend(["top-100", "top-10", "record"]),
            Some(r) if r <= 10 => ids.extend(["top-100", "top-10"]),
            Some(r) if r <= 100 => ids.push("top-100"),
            _ => {}
        }
        announce(&mut earned, &mut toast, &ids);
    }
}

#[derive(Component)]
struct ToastText;

fn setup(mut commands: Commands) {
    commands.spawn((
        ToastText,
        crate::ui::label("", 17.0, crate::ui::TEXT),
        Node {
            position_type: PositionType::Absolute,
            top: px(24),
            left: percent(50),
            margin: UiRect::left(px(-200)),
            width: px(400),
            padding: UiRect::axes(px(16), px(10)),
            border_radius: BorderRadius::all(px(10)),
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(crate::hud::FRONT),
        Visibility::Hidden,
    ));
}

fn draw(
    time: Res<Time>,
    halt: Res<crate::pause::Halt>,
    mut toast: ResMut<Toast>,
    mut texts: Query<(&mut Text, &mut Visibility), With<ToastText>>,
) {
    let dt = time.delta_secs();
    if let Some((_, left)) = toast.showing.as_mut() {
        *left -= dt;
    }
    if toast.showing.as_ref().is_none_or(|(_, left)| *left <= 0.0) {
        toast.showing = (!toast.waiting.is_empty()).then(|| (toast.waiting.remove(0), 3.5));
    }
    let Ok((mut text, mut visibility)) = texts.single_mut() else {
        return;
    };
    match &toast.showing {
        Some((name, _)) if !halt.stopped() => {
            let wanted = crate::text::tf("ach.toast", &[name]);
            if text.0 != wanted {
                text.0 = wanted;
            }
            visibility.set_if_neq(Visibility::Visible);
        }
        _ => {
            visibility.set_if_neq(Visibility::Hidden);
        }
    }
}

/// Written a second after the last change, off the main thread.
fn save(
    earned: Res<Earned>,
    time: Res<Time<Real>>,
    read_only: Option<Res<crate::settings::ReadOnly>>,
    mut since: Local<Option<f64>>,
) {
    if read_only.is_some() {
        return;
    }
    let now = time.elapsed_secs_f64();
    if earned.is_changed() && !earned.is_added() {
        *since = Some(now);
    }
    if since.is_some_and(|at| now - at > 1.0) {
        *since = None;
        let (Some(path), Ok(text)) = (path(), serde_json::to_string_pretty(&*earned)) else {
            return;
        };
        std::thread::spawn(move || {
            let beside = path.with_extension("writing");
            let written = std::fs::create_dir_all(path.parent().unwrap_or(&path))
                .and_then(|()| std::fs::write(&beside, text))
                .and_then(|()| std::fs::rename(&beside, &path));
            if let Err(trouble) = written {
                warn!("cannot save achievements: {trouble}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lap(valid: bool, best: bool) -> LapFinished {
        LapFinished {
            time: 60.0,
            best,
            valid,
        }
    }

    #[test]
    fn there_are_fifty_with_different_ids() {
        let all = all();
        assert_eq!(all.len(), 50);
        let ids: BTreeSet<_> = all.iter().map(|(id, _, _)| id.clone()).collect();
        assert_eq!(ids.len(), 50);
        for id in GOLD_AT {
            assert!(
                all_circuits().iter().any(|c| c.id == id),
                "{id} is not a circuit"
            );
        }
    }

    #[test]
    fn a_lap_earns_what_it_should() {
        let mut earned = Earned::default();
        let first = from_lap(
            &mut earned,
            &lap(false, false),
            "monza",
            Mode::Regular,
            Spec::Tourer,
            &[],
            None,
            0,
            false,
        );
        assert_eq!(first, vec!["first-lap".to_string()]);
        let good = from_lap(
            &mut earned,
            &lap(true, true),
            "monaco",
            Mode::Pro,
            Spec::Tourer,
            &[Split::Purple, Split::Purple],
            Some(Medal::Gold),
            0,
            true,
        );
        for id in [
            "first-valid",
            "first-best",
            "pro",
            "bronze",
            "silver",
            "gold",
            "gold-monaco",
            "all-purple",
            "ghost-beaten",
            "monaco-clean",
        ] {
            assert!(good.iter().any(|g| g == id), "{id} missing from {good:?}");
        }
        assert!(!good.iter().any(|g| g == "author"));
    }

    #[test]
    fn streaks_break_on_a_lap_that_does_not_count() {
        let mut earned = Earned::default();
        for _ in 0..4 {
            from_lap(
                &mut earned,
                &lap(true, false),
                "monza",
                Mode::Regular,
                Spec::Tourer,
                &[],
                None,
                0,
                false,
            );
        }
        from_lap(
            &mut earned,
            &lap(false, false),
            "monza",
            Mode::Regular,
            Spec::Tourer,
            &[],
            None,
            0,
            false,
        );
        let after = from_lap(
            &mut earned,
            &lap(true, false),
            "monza",
            Mode::Regular,
            Spec::Tourer,
            &[],
            None,
            0,
            false,
        );
        assert!(!after.iter().any(|g| g == "clean-5"));
        assert_eq!(earned.streak, 1);
    }

    #[test]
    fn modes_and_cars_are_counted_across_laps() {
        let mut earned = Earned::default();
        let mut last = Vec::new();
        for (mode, car) in [
            (Mode::Beginner, Spec::Tourer),
            (Mode::Regular, Spec::Clubman),
            (Mode::Pro, Spec::Express),
        ] {
            last = from_lap(
                &mut earned,
                &lap(true, false),
                "spa-francorchamps",
                mode,
                car,
                &[],
                None,
                0,
                false,
            );
        }
        assert!(last.iter().any(|g| g == "modes") && last.iter().any(|g| g == "cars"));
    }

    #[test]
    fn earning_twice_is_earning_once() {
        let mut earned = Earned::default();
        assert!(earned.earn("gold", 1));
        assert!(!earned.earn("gold", 2));
        assert_eq!(earned.earned["gold"], 1);
    }

    #[test]
    fn the_file_round_trips() {
        let mut earned = Earned::default();
        earned.earn("online", 5);
        earned.metres = 1234.5;
        earned.lapped.insert("monza".into());
        let text = serde_json::to_string(&earned).unwrap();
        assert_eq!(serde_json::from_str::<Earned>(&text).unwrap(), earned);
        assert_eq!(
            serde_json::from_str::<Earned>("{}").unwrap(),
            Earned::default()
        );
    }
}
