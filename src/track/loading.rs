//! Keep rendering while the next circuit's road, terrain and scenery are built.
use super::*;
use crate::pause::Halt;
use bevy::tasks::{AsyncComputeTaskPool, Task, futures::check_ready};

#[derive(Resource, Default)]
pub(super) struct Loading {
    target: Option<&'static Circuit>,
    task: Option<Task<Built>>,
    resume: Option<Halt>,
}

struct Built {
    track: Track,
    surfaces: [Mesh; 3],
    trackside: [Mesh; 2],
}

impl Built {
    fn new(circuit: &'static Circuit) -> Self {
        let track = Track::new(circuit);
        let surfaces = surfaces(&track);
        let trackside = trackside::meshes_for(&track);
        Self {
            track,
            surfaces,
            trackside,
        }
    }
}

/// Only the completed circuit becomes visible to reset consumers. Keep the
/// loading screen up through that frame's marker, start-line and GPU updates.
#[allow(clippy::too_many_arguments)]
pub(super) fn switch(
    mut asked: MessageReader<GoTo>,
    mut loading: ResMut<Loading>,
    mut halt: ResMut<Halt>,
    mut track: ResMut<Track>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut loft: Query<&mut Mesh3d, LoftMesh>,
    mut ground: Query<&mut Mesh3d, TerrainMesh>,
    mut road: Query<&mut Mesh3d, AsphaltMesh>,
    mut scenery: ResMut<trackside::Prepared>,
    mut reset: MessageWriter<Reset>,
) {
    if loading.task.is_none()
        && let Some(resume) = loading.resume.take()
    {
        if *halt == Halt::Loading {
            *halt = resume;
        }
        loading.target = None;
    }
    if let Some(GoTo(next)) = asked.read().last()
        && (next.id != track.circuit.id || loading.task.is_some())
    {
        loading.target = Some(next);
        loading.resume.get_or_insert(*halt);
        *halt = Halt::Loading;
    }
    let Some(target) = loading.target else {
        return;
    };
    if let Some(task) = &mut loading.task {
        let Some(built) = check_ready(task) else {
            return;
        };
        loading.task = None;
        if built.track.circuit.id == target.id {
            let [scenery_mesh, asphalt, grass] = built.surfaces;
            if let Ok(mut mesh) = loft.single_mut() {
                mesh.0 = meshes.add(scenery_mesh);
            }
            if let Ok(mut mesh) = road.single_mut() {
                mesh.0 = meshes.add(asphalt);
            }
            if let Ok(mut mesh) = ground.single_mut() {
                mesh.0 = meshes.add(grass);
            }
            scenery.0 = Some(built.trackside);
            *track = built.track;
            reset.write(Reset);
            return;
        }
    }
    // Coalesce further requests without launching competing terrain builders.
    // Returning to the active circuit also discards an obsolete result.
    if target.id != track.circuit.id {
        loading.task = Some(AsyncComputeTaskPool::get().spawn(async move { Built::new(target) }));
    }
}

#[derive(Component)]
pub(super) struct Overlay;
#[derive(Component)]
pub(super) struct CircuitName;
#[derive(Component)]
pub(super) struct Light(usize);

pub(super) fn setup(mut commands: Commands) {
    use crate::ui::{ACCENT, LINE, MUTED, SURFACE, TEXT, label};
    commands
        .spawn((
            Overlay,
            GlobalZIndex(100),
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(SURFACE),
            bevy::ui::FocusPolicy::Block,
        ))
        .with_children(|overlay| {
            overlay
                .spawn(Node {
                    max_width: percent(90),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    row_gap: px(18),
                    ..default()
                })
                .with_children(|card| {
                    card.spawn(crate::text::label("loading.circuit", 14.0, ACCENT));
                    card.spawn((
                        CircuitName,
                        label("", 38.0, TEXT),
                        TextLayout::justify(Justify::Center),
                    ));
                    card.spawn(Node {
                        column_gap: px(8),
                        margin: UiRect::vertical(px(8)),
                        ..default()
                    })
                    .with_children(|row| {
                        for i in 0..5 {
                            row.spawn((
                                Light(i),
                                Node {
                                    width: px(44),
                                    height: px(5),
                                    border_radius: BorderRadius::all(px(3)),
                                    ..default()
                                },
                                BackgroundColor(LINE),
                            ));
                        }
                    });
                    card.spawn(crate::text::label("loading.wait", 17.0, MUTED));
                });
        });
}

pub(super) fn show(
    loading: Res<Loading>,
    time: Res<Time<Real>>,
    settings: Res<crate::settings::Settings>,
    mut overlay: Query<&mut Node, With<Overlay>>,
    mut names: Query<&mut Text, With<CircuitName>>,
    mut lights: Query<(&Light, &mut BackgroundColor)>,
) {
    let display = if loading.target.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut overlay {
        if node.display != display {
            node.display = display;
        }
    }
    let Some(circuit) = loading.target else {
        return;
    };
    for mut name in &mut names {
        if name.0 != circuit.name {
            name.0 = circuit.name.into();
        }
    }
    for (light, mut color) in &mut lights {
        let glow = if settings.reduced_motion {
            1.0
        } else {
            let phase = time.elapsed_secs() * 3.0 - light.0 as f32 * 0.65;
            (phase.sin() * 0.5 + 0.5).powi(3)
        };
        color.0 = crate::ui::LINE.mix(&crate::ui::ACCENT, glow);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn loading_animates_with_a_paused_clock_and_respects_reduced_motion() {
        let circuit = circuits::all().iter().find(|c| c.id == "le-mans").unwrap();
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Loading>()
            .init_resource::<crate::settings::Settings>()
            .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
                Duration::from_millis(100),
            ))
            .add_systems(Startup, setup)
            .add_systems(Update, show);
        app.world_mut().resource_mut::<Time<Virtual>>().pause();
        app.update();
        let world = app.world_mut();
        let overlay = world
            .query_filtered::<Entity, With<Overlay>>()
            .single(world)
            .unwrap();
        let light = world
            .query_filtered::<Entity, With<Light>>()
            .iter(world)
            .next()
            .unwrap();
        assert_eq!(world.get::<Node>(overlay).unwrap().display, Display::None);
        world.resource_mut::<Loading>().target = Some(circuit);
        app.update();
        let world = app.world_mut();
        assert_eq!(world.get::<Node>(overlay).unwrap().display, Display::Flex);
        assert_eq!(
            world
                .query_filtered::<&Text, With<CircuitName>>()
                .single(world)
                .unwrap()
                .0,
            circuit.name
        );
        let before = world.get::<BackgroundColor>(light).unwrap().0;
        app.update();
        assert_ne!(app.world().get::<BackgroundColor>(light).unwrap().0, before);
        assert_eq!(
            app.world().resource::<Time<Virtual>>().elapsed(),
            Duration::ZERO
        );

        app.world_mut()
            .resource_mut::<crate::settings::Settings>()
            .reduced_motion = true;
        app.update();
        let before = app.world().get::<BackgroundColor>(light).unwrap().0;
        app.update();
        assert_eq!(app.world().get::<BackgroundColor>(light).unwrap().0, before);
        app.world_mut().resource_mut::<Loading>().target = None;
        app.update();
        assert_eq!(
            app.world().get::<Node>(overlay).unwrap().display,
            Display::None
        );
    }

    #[test]
    fn returning_to_the_active_circuit_discards_the_pending_build_without_resetting() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<Mesh>>()
            .init_resource::<trackside::Prepared>()
            .insert_resource(Track::new(circuits::first()))
            .insert_resource(Halt::Loading)
            .add_message::<Reset>()
            .add_message::<GoTo>()
            .add_systems(Update, switch);
        let next = &circuits::all()[1];
        let pool = bevy::tasks::TaskPoolBuilder::new().num_threads(1).build();
        app.insert_resource(Loading {
            target: Some(next),
            task: Some(pool.spawn(std::future::pending())),
            resume: Some(Halt::Pause),
        });
        // A duplicate request must keep the existing job, not restart it.
        app.world_mut().write_message(GoTo(next));
        app.update();
        assert!(
            !app.world()
                .resource::<Loading>()
                .task
                .as_ref()
                .unwrap()
                .is_finished()
        );
        app.world_mut().write_message(GoTo(circuits::first()));
        app.update();
        assert_eq!(
            app.world().resource::<Loading>().target.unwrap().id,
            circuits::first().id
        );

        // Release the obsolete result without spending time building scenery
        // that must never be installed. Real geometry is covered by the switch test.
        let track = Track::new(next);
        let mesh = track.profile.loft(&track.ribbon);
        let built = Built {
            track,
            surfaces: [mesh.clone(), mesh.clone(), mesh.clone()],
            trackside: [mesh.clone(), mesh],
        };
        app.world_mut().resource_mut::<Loading>().task = Some(pool.spawn(async move { built }));
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while app.world().resource::<Loading>().target.is_some() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
            app.update();
            assert!(app.world().resource::<Messages<Reset>>().is_empty());
            assert_eq!(
                app.world().resource::<Track>().circuit.id,
                circuits::first().id
            );
        }
        assert!(app.world().resource::<trackside::Prepared>().0.is_none());
        assert_eq!(*app.world().resource::<Halt>(), Halt::Pause);
    }
}
