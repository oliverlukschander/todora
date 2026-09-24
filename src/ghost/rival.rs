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
fn take(
    online: Option<ResMut<crate::online::Online>>,
    track: Res<Track>,
    mode: Res<Mode>,
    mut rival: ResMut<Rival>,
) {
    let Some(mut online) = online else {
        return;
    };
    let Some((run_id, bytes)) = online.ghost.take() else {
        return;
    };
    let Ok(run) = Run::decode(&bytes) else {
        warn!("ghost {run_id} is not a run");
        return;
    };
    if run.circuit != track.circuit().id
        || run.mode != *mode
        || run.fingerprint != track.fingerprint()
    {
        online.say(crate::text::t("rival.other"));
        return;
    }
    let (name, rank) = online
        .wanted
        .take()
        .filter(|(id, _, _)| *id == run_id)
        .map_or((String::new(), None), |(_, name, rank)| (name, Some(rank)));
    rival.name = name;
    rival.rank = rank;
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
fn build(
    mut rival: ResMut<Rival>,
    mut timer: ResMut<LapTimer>,
    mut online: Option<ResMut<crate::online::Online>>,
) {
    let Some(task) = rival.building.as_mut() else {
        return;
    };
    let Some(result) = block_on(poll_once(task)) else {
        return;
    };
    rival.building = None;
    match result {
        Some((lap, sectors)) => {
            if rival.rank == Some(1) {
                timer.world_sectors = sectors;
            }
            rival.lap = Some(lap);
            rival.on = true;
            let who = if rival.name.is_empty() {
                crate::text::t("rival.someone").to_string()
            } else {
                rival.name.clone()
            };
            if let Some(online) = online.as_mut() {
                online.say(crate::text::tf("rival.racing", &[&who]));
            }
        }
        None => {
            if let Some(online) = online.as_mut() {
                online.say(crate::text::t("rival.bad"));
            }
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
}
