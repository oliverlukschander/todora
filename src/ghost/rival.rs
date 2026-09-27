//! A lap downloaded from the board, driven again beside you as a second ghost.
//!
//! The run is replayed on the async compute pool with the game's own engine —
//! which also checks it: a run that does not replay to a lap is not raced — and
//! the poses it passes through become a recording like your own best. It waits
//! at the line and runs with your clock, cool white where your ghost is amber.
//! **G** cycles what is shown: both ghosts, yours, theirs, neither; the delta
//! follows whichever is shown, with a second line for theirs when both are.
//!
//! If the lap is the world record, its sectors are what purple means.

use bevy::{
    light::NotShadowCaster,
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
    world_serialization::WorldInstanceReady,
};

use super::{FADE, Recording};
use crate::Reset;
use crate::car::{MODEL, Mode, Player, SCALE};
use crate::lap::LapTimer;
use crate::online::run::Run;
use crate::track::Track;

/// A cool white, so the two ghosts are never mistaken for each other.
const GLOW: LinearRgba = LinearRgba::new(0.10, 0.16, 0.22, 1.0);

/// A downloaded lap as a ghost, and its sectors.
type Rebuilt = (Recording, Vec<f32>);

#[derive(Resource)]
pub(crate) struct Rival {
    /// Whether it is shown.
    pub on: bool,
    pub delta: Option<f32>,
    /// Whose lap, and where it stood.
    pub name: String,
    pub rank: Option<u64>,
    pub country: String,
    pub world_record: bool,
    preparing: Option<crate::online::ui::GhostTarget>,
    lap: Option<Recording>,
    building: Option<Task<Option<Rebuilt>>>,
    car: Entity,
}

impl Rival {
    pub(crate) fn loaded(&self) -> bool {
        self.lap.is_some()
    }

    pub(crate) fn lap(&self) -> Option<&Recording> {
        self.lap.as_ref()
    }

    /// The downloaded lap's time, if there is one.
    pub(crate) fn lap_time(&self) -> Option<f32> {
        self.lap.as_ref().map(Recording::duration)
    }
}

pub(super) fn plugin(app: &mut App) {
    app.add_systems(Startup, setup)
        .add_systems(
            PreUpdate,
            forget
                .after(crate::lap::ClockSet)
                .after(crate::track::TrackSet),
        )
        .add_systems(
            Update,
            (take, build, replay).chain().before(super::GhostSet),
        );
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    let car = commands
        .spawn((
            Transform::from_scale(Vec3::splat(SCALE)),
            Visibility::Hidden,
            WorldAssetRoot(asset_server.load(GltfAssetLabel::Scene(0).from_asset(MODEL))),
        ))
        .observe(fade)
        .id();
    commands.insert_resource(Rival {
        on: true,
        delta: None,
        name: String::new(),
        rank: None,
        country: String::new(),
        world_record: false,
        preparing: None,
        lap: None,
        building: None,
        car,
    });
}

fn fade(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    painted: Query<&MeshMaterial3d<StandardMaterial>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for entity in children.iter_descendants(ready.entity) {
        let Some(material) = painted.get(entity).ok().and_then(|h| materials.get(&h.0)) else {
            continue;
        };
        let mut ghostly = material.clone();
        ghostly.base_color = Color::srgba(0.9, 0.95, 1.0, FADE);
        ghostly.alpha_mode = AlphaMode::Blend;
        ghostly.emissive = GLOW;
        let ghostly = materials.add(ghostly);
        commands
            .entity(entity)
            .insert((MeshMaterial3d(ghostly), NotShadowCaster));
    }
}

/// A downloaded lap for this circuit and mode starts being rebuilt.
#[allow(clippy::too_many_arguments)]
fn take(
    online: Option<ResMut<crate::online::Online>>,
    track: Res<Track>,
    mut mode: ResMut<Mode>,
    mut rival: ResMut<Rival>,
    halt: Res<crate::pause::Halt>,
    mut go: MessageWriter<crate::track::GoTo>,
) {
    let Some(mut online) = online else {
        return;
    };
    if *halt == crate::pause::Halt::Loading {
        return;
    }
    let Some(target) = online.wanted.clone() else {
        online.ghost = None;
        return;
    };
    let Some((run_id, bytes)) = online.ghost.as_ref() else {
        return;
    };
    if *run_id != target.place.run {
        online.ghost = None;
        return;
    }
    let Ok(run) = Run::decode(bytes) else {
        online.ghost = None;
        online.wanted = None;
        online.ghost_status = crate::text::t("rival.bad").into();
        return;
    };
    if run.circuit != target.circuit || run.mode != target.mode {
        online.ghost = None;
        online.wanted = None;
        online.ghost_status = crate::text::t("rival.other").into();
        return;
    }
    if track.circuit().id != target.circuit || *mode != target.mode {
        let Some(circuit) = crate::track::all_circuits()
            .iter()
            .find(|c| c.id == target.circuit)
        else {
            online.wanted = None;
            online.ghost = None;
            online.ghost_status = crate::text::t("rival.other").into();
            return;
        };
        mode.set_if_neq(target.mode);
        go.write(crate::track::GoTo(circuit));
        return;
    }
    online.ghost = None;
    if run.fingerprint != track.fingerprint() {
        online.wanted = None;
        online.ghost_status = crate::text::t("rival.other").into();
        return;
    }
    rival.preparing = Some(target);
    let circuit = track.circuit();
    rival.building = Some(AsyncComputeTaskPool::get().spawn(async move {
        let track = Track::new(circuit);
        let mut lap = Recording::default();
        let verdict = crate::online::replay::replay_with(&run, &track, |time, progress, at| {
            lap.push(time, progress, at);
        });
        (verdict.steps == Some(run.steps) && verdict.valid).then_some((lap, run.sectors))
    }));
}

/// A rebuilt lap is ready: it is the rival now.
#[allow(clippy::too_many_arguments)]
fn build(
    mut rival: ResMut<Rival>,
    mut timer: ResMut<LapTimer>,
    mut online: Option<ResMut<crate::online::Online>>,
    mut halt: ResMut<crate::pause::Halt>,
    mut reset: MessageWriter<Reset>,
    mut mine: ResMut<super::Ghost>,
) {
    let Some(online) = online.as_mut() else {
        return;
    };
    if rival.preparing.as_ref().is_some_and(|target| {
        online
            .wanted
            .as_ref()
            .is_none_or(|wanted| wanted.place.run != target.place.run)
    }) {
        rival.preparing = None;
        rival.building = None;
        return;
    }
    let Some(task) = rival.building.as_mut() else {
        return;
    };
    let Some(result) = block_on(poll_once(task)) else {
        return;
    };
    rival.building = None;
    let Some(target) = rival.preparing.take() else {
        return;
    };
    online.wanted = None;
    match result {
        Some((lap, sectors)) => {
            timer.world_sectors = if target.world_record {
                sectors
            } else {
                Vec::new()
            };
            rival.world_record = target.world_record;
            rival.name = target.place.name;
            rival.country = target.place.country.unwrap_or_default();
            rival.rank = Some(target.place.rank);
            rival.lap = Some(lap);
            rival.on = true;
            mine.on = false;
            online.ghost_status.clear();
            online.say(crate::text::tf("rival.racing", &[&rival.name]));
            reset.write(Reset);
            *halt = crate::pause::Halt::Nothing;
        }
        None => {
            online.ghost_status = crate::text::t("rival.bad").into();
        }
    }
}

/// A new circuit or mode leaves the rival behind.
fn forget(
    mut resets: MessageReader<Reset>,
    track: Res<Track>,
    mode: Res<Mode>,
    mut rival: ResMut<Rival>,
    mut timer: ResMut<LapTimer>,
) {
    resets.clear();
    if track.is_changed() || mode.is_changed() {
        rival.lap = None;
        rival.building = None;
        rival.preparing = None;
        rival.delta = None;
        timer.world_sectors.clear();
    }
}

/// Put the rival where its lap was at this point of the clock.
fn replay(
    timer: Res<LapTimer>,
    mut rival: ResMut<Rival>,
    mut cars: Query<(&mut Transform, &mut Visibility), Without<Player>>,
) {
    let car = rival.car;
    let (pose, delta) = match &rival.lap {
        Some(lap) => (
            lap.pose_at(timer.current),
            lap.time_at(timer.progress())
                .map(|then| timer.current - then),
        ),
        None => (None, None),
    };
    rival.delta = delta;
    let Ok((mut at, mut visibility)) = cars.get_mut(car) else {
        return;
    };
    visibility.set_if_neq(if rival.on && pose.is_some() {
        Visibility::Visible
    } else {
        Visibility::Hidden
    });
    if let Some((translation, rotation)) = pose {
        at.translation = translation;
        at.rotation = rotation;
    }
}

/// What `G` shows next: with a rival, both → yours → theirs → neither.
pub(super) fn next_shown(mine: bool, theirs: bool, rival: bool) -> (bool, bool) {
    if !rival {
        return (!mine, theirs);
    }
    match (mine, theirs) {
        (true, true) => (true, false),
        (true, false) => (false, true),
        (false, true) => (false, false),
        (false, false) => (true, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g_cycles_both_yours_theirs_and_neither() {
        let mut shown = (true, true);
        let mut seen = vec![shown];
        for _ in 0..4 {
            shown = next_shown(shown.0, shown.1, true);
            seen.push(shown);
        }
        assert_eq!(
            seen,
            vec![
                (true, true),
                (true, false),
                (false, true),
                (false, false),
                (true, true)
            ]
        );
        assert_eq!(
            next_shown(true, true, false),
            (false, true),
            "without a rival G is on and off"
        );
    }

    #[test]
    fn a_downloaded_lap_rebuilds_into_a_ghost_that_finishes_where_it_should() {
        let bytes = crate::verify::ai_run("monza", "regular").unwrap();
        let run = Run::decode(&bytes).unwrap();
        let track = Track::new(
            crate::track::all_circuits()
                .iter()
                .find(|c| c.id == "monza")
                .unwrap(),
        );
        let mut lap = Recording::default();
        let verdict =
            crate::online::replay::replay_with(&run, &track, |t, p, at| lap.push(t, p, at));
        assert_eq!(verdict.steps, Some(run.steps));
        assert!((lap.duration() - run.steps as f32 / 240.0).abs() < 1e-3);
        assert_eq!(lap.time_at(1.0), Some(lap.duration()));
    }
    #[test]
    fn choosing_a_ghost_prepares_its_circuit_then_starts_a_fresh_race() {
        use crate::online::{Online, client::Place, ui::GhostTarget};
        use crate::pause::Halt;
        let bytes = crate::verify::ai_run("monza", "regular").unwrap();
        let run = Run::decode(&bytes).unwrap();
        let circuit = |id| {
            crate::track::all_circuits()
                .iter()
                .find(|c| c.id == id)
                .unwrap()
        };
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .insert_resource(Track::new(circuit("red-bull-ring")))
            .insert_resource(Mode::Pro)
            .insert_resource(Halt::Board)
            .init_resource::<LapTimer>()
            .add_message::<Reset>()
            .add_message::<crate::track::GoTo>();
        let car = app.world_mut().spawn_empty().id();
        app.insert_resource(super::super::Ghost {
            on: true,
            delta: None,
            best: None,
            recording: Recording::default(),
            last: None,
            car,
            saved: None,
        });
        app.insert_resource(Rival {
            on: false,
            delta: None,
            name: String::new(),
            rank: None,
            country: String::new(),
            world_record: false,
            preparing: None,
            lap: None,
            building: None,
            car,
        });
        let mut online = Online::default();
        online.wanted = Some(GhostTarget {
            place: Place {
                rank: 1,
                player: "rival".into(),
                name: "Test Rival".into(),
                country: Some("DK".into()),
                seconds: run.steps as f64 / 240.0,
                steps: run.steps,
                car: "tourer".into(),
                setup: "balanced".into(),
                multiplayer: false,
                run: "test-run".into(),
            },
            circuit: "monza".into(),
            mode: Mode::Regular,
            world_record: false,
        });
        online.ghost = Some(("test-run".into(), bytes));
        app.insert_resource(online)
            .add_systems(Update, (take, build).chain());
        app.update();
        assert_eq!(*app.world().resource::<Mode>(), Mode::Regular);
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<crate::track::GoTo>>()
                .drain()
                .next()
                .unwrap()
                .0
                .id,
            "monza"
        );
        assert!(!app.world().resource::<Rival>().loaded());
        app.insert_resource(Track::new(circuit("monza")));
        for _ in 0..2000 {
            app.update();
            if app.world().resource::<Rival>().loaded() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        let rival = app.world().resource::<Rival>();
        assert!(rival.loaded());
        assert_eq!(rival.name, "Test Rival");
        assert_eq!(rival.country, "DK");
        assert!(!rival.world_record, "a country #1 is not the world record");
        assert!(app.world().resource::<LapTimer>().world_sectors.is_empty());
        assert_eq!(*app.world().resource::<Halt>(), Halt::Nothing);
        assert!(!app.world().resource::<super::super::Ghost>().on);
        assert!(!app.world().resource::<Messages<Reset>>().is_empty());
        assert!(app.world().resource::<Online>().wanted.is_none());
    }
}
