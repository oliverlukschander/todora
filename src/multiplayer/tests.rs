use super::*;
use session::{Remote, VERSION};
use std::collections::VecDeque;

#[test]
#[cfg(not(any(
    all(target_os = "macos", feature = "game-center"),
    feature = "multiplayer-test"
)))]
fn unsupported_platform_keeps_multiplayer_inert() {
    let mut app = App::new();
    app.add_plugins(MultiplayerPlugin);
    // Updating without window, UI or native resources must remain safe, even
    // when a Linux build has accidentally enabled the game-center feature.
    app.update();
    assert!(!app.world().resource::<Session>().active());
    assert!(!app.world().contains_resource::<Transport>());
    assert_eq!(
        app.world_mut()
            .query::<&MultiplayerButton>()
            .iter(app.world())
            .count(),
        0
    );
}

fn choice() -> Config {
    Config {
        circuit: "suzuka".into(),
        fingerprint: 123,
        mode: 1,
    }
}
fn sample(seq: u64, time: f64, x: f32) -> Snapshot {
    Snapshot {
        seq,
        reset: 0,
        time,
        position: [x, 4.0, 0.0],
        rotation: Quat::IDENTITY.to_array(),
        velocity: [10.0, 0.0, 0.0],
        lap: 20.0,
        invalid: false,
        paused: false,
        laps: 0,
        best: None,
    }
}

// Drive both ends through the same serialized packets used by GameKit. Their
// clocks deliberately differ by 87 seconds. The client initially has another
// circuit selected, and loading completes in a different order on each side.
fn pair() -> [Session; 2] {
    let mut sessions = [Session::default(), Session::default()];
    let mut queues = [VecDeque::new(), VecDeque::new()];
    let mut configs = [
        choice(),
        Config {
            circuit: "monza".into(),
            fingerprint: 456,
            mode: 0,
        },
    ];
    for i in 0..2 {
        let now = 10.0 + i as f64 * 87.0;
        sessions[i].begin(now);
        for action in sessions[i].connected(i == 0, "Driver".into(), configs[i].clone(), now) {
            if let Action::Send(p) = action {
                queues[1 - i].push_back(p.encode());
            }
        }
    }
    for tick in 1..100 {
        for i in 0..2 {
            let now = 10.0 + tick as f64 * 0.02 + i as f64 * 87.0;
            let mut actions = vec![];
            while let Some(bytes) = queues[i].pop_front() {
                actions.extend(sessions[i].receive(Packet::decode(&bytes).unwrap(), now));
            }
            for action in actions {
                match action {
                    Action::Send(p) => queues[1 - i].push_back(p.encode()),
                    Action::Load(c) => configs[i] = c,
                    Action::End => panic!("unexpected end: {}", sessions[i].status),
                    Action::Reset => {}
                }
            }
            if tick > 3 + i * 4 {
                for action in sessions[i].loaded(&configs[i], now) {
                    if let Action::Send(p) = action {
                        queues[1 - i].push_back(p.encode());
                    }
                }
            }
        }
    }
    assert_eq!(configs[0], configs[1]);
    assert!(sessions.iter().all(|s| s.phase == Phase::Countdown));
    assert!((sessions[1].start_at - sessions[0].start_at - 87.0).abs() < 0.025);
    for s in &mut sessions {
        assert!(s.blocks_drive());
        assert!(s.tick(s.start_at + 0.01).is_empty());
        assert!(s.driving());
    }
    sessions
}

#[test]
fn two_players_agree_on_track_mode_and_start_despite_different_clocks() {
    pair();
}

#[test]
fn same_named_circuit_with_different_geometry_never_becomes_ready() {
    let mut s = Session::default();
    s.begin(0.0);
    s.connected(false, "peer".into(), choice(), 0.0);
    s.receive(
        Packet::Hello {
            version: VERSION.into(),
            config: choice(),
        },
        0.1,
    );
    let mut actual = choice();
    actual.fingerprint += 1;
    s.loaded(&actual, 0.2);
    assert!(!s.active());
}

#[test]
fn idle_network_polling_does_not_reset_mode_or_lap_clock() {
    #[derive(Resource, Default)]
    struct Changes(u32);
    fn poll(
        mut t: ResMut<Transport>,
        mut mode: ResMut<Mode>,
        mut go: MessageWriter<GoTo>,
        mut reset: MessageWriter<Reset>,
    ) {
        perform(vec![], &mut t, &mut mode, &mut go, &mut reset);
    }
    fn changes(mode: Res<Mode>, mut changed: ResMut<Changes>) {
        if mode.is_changed() {
            changed.0 += 1;
        }
    }
    let mut app = App::new();
    app.init_resource::<Transport>()
        .init_resource::<Mode>()
        .init_resource::<Changes>()
        .add_message::<GoTo>()
        .add_message::<Reset>()
        .add_systems(Update, (poll, changes).chain());
    for _ in 0..10 {
        app.update();
    }
    assert_eq!(app.world().resource::<Changes>().0, 1);
}

#[test]
fn duplicate_control_packets_cannot_restart_driving_or_change_circuit() {
    let mut sessions = pair();
    for session in &mut sessions {
        let now = session.start_at + 1.0;
        for p in [
            Packet::Ready,
            Packet::Ack,
            Packet::Start {
                at: now + 3.0,
                offset: 0.0,
            },
            Packet::Hello {
                version: VERSION.into(),
                config: Config {
                    circuit: "monza".into(),
                    fingerprint: 456,
                    mode: 0,
                },
            },
        ] {
            assert!(session.receive(p, now).is_empty());
            assert!(session.driving());
            assert_eq!(session.config, Some(choice()));
        }
    }
}

#[test]
fn mismatched_version_and_unknown_track_fail_before_driving() {
    for (version, circuit) in [("old", "suzuka"), (VERSION, "missing")] {
        let mut s = Session::default();
        s.begin(0.0);
        s.connected(false, "driver".into(), choice(), 0.0);
        s.receive(
            Packet::Hello {
                version: version.into(),
                config: Config {
                    circuit: circuit.into(),
                    fingerprint: 123,
                    mode: 1,
                },
            },
            0.1,
        );
        assert!(!s.active());
    }
}

#[test]
fn connection_loss_and_leave_remove_remote_and_allow_a_fresh_match() {
    let [mut s, _] = pair();
    s.receive(Packet::State(sample(1, 0.2, 2.0)), s.start_at + 1.0);
    assert!(s.remote.latest().is_some());
    s.tick(s.start_at + 17.0);
    assert!(!s.active());
    assert!(s.remote.latest().is_none());
    s.begin(100.0);
    assert_eq!(s.local_laps, 0);
    assert_eq!(s.local_best, None);
    s.connected(true, "new peer".into(), choice(), 100.1);
    s.receive(Packet::Leave, 100.2);
    assert!(!s.active());
}

#[test]
fn unacknowledged_countdown_never_releases_the_car() {
    let mut s = Session::default();
    s.begin(1.0);
    s.connected(true, "peer".into(), choice(), 1.0);
    s.receive(
        Packet::Hello {
            version: VERSION.into(),
            config: choice(),
        },
        1.1,
    );
    s.loaded(&choice(), 1.2);
    s.receive(Packet::Ready, 1.3);
    s.receive(
        Packet::Pong {
            id: 1,
            remote: 10.0,
        },
        1.4,
    );
    assert_eq!(s.phase, Phase::Countdown);
    s.tick(s.start_at + 0.1);
    assert!(!s.active());
}

#[test]
fn countdown_correlation_survives_json_timestamp_rounding() {
    let now = 1.1449161538000001;
    let mut s = Session::default();
    s.begin(0.0);
    s.connected(true, "peer".into(), choice(), 0.0);
    s.receive(
        Packet::Hello {
            version: VERSION.into(),
            config: choice(),
        },
        0.1,
    );
    s.loaded(&choice(), 0.2);
    let actions = s.receive(Packet::Ready, now);
    let Action::Send(ping) = &actions[0] else {
        panic!("expected ping");
    };
    let Packet::Ping { id } = Packet::decode(&ping.encode()).unwrap() else {
        panic!("expected ping");
    };
    let pong = Packet::decode(
        &Packet::Pong {
            id,
            remote: now + 87.0,
        }
        .encode(),
    )
    .unwrap();
    s.receive(pong, now + 0.04);
    assert_eq!(s.phase, Phase::Countdown);
}

#[test]
fn malformed_oversized_and_nonphysical_packets_are_rejected() {
    assert!(Packet::decode(&[0; 2049]).is_none());
    assert!(Packet::decode(b"{bad").is_none());
    for bad in [f32::NAN, f32::INFINITY, 1e9] {
        let mut s = sample(1, 1.0, 0.0);
        s.position[0] = bad;
        assert!(Packet::decode(&Packet::State(s).encode()).is_none());
    }
    let mut s = sample(1, 1.0, 0.0);
    s.rotation = [0.0; 4];
    assert!(Packet::decode(&Packet::State(s).encode()).is_none());
    assert!(Packet::decode(&Packet::State(sample(1, 1.0, 0.0)).encode()).is_some());
}

#[test]
fn remote_smoothing_handles_reordering_resets_bridge_height_and_stale_data() {
    let mut r = Remote::default();
    r.push(sample(1, 1.0, 0.0), 20.0);
    r.push(sample(2, 1.2, 2.0), 20.2);
    let (p, _) = r.pose(20.2).unwrap();
    assert!((p.x - 1.0).abs() < 0.001);
    assert_eq!(
        p.y, 4.0,
        "use network elevation, never snap to the lower bridge road"
    );
    r.push(sample(1, 1.0, 100.0), 20.3);
    assert_eq!(r.latest().unwrap().seq, 2);
    assert!(r.pose(22.0).is_none());
    let mut reset = sample(3, 1.3, 100.0);
    reset.reset = 1;
    r.push(reset, 20.3);
    assert_eq!(
        r.pose(20.3).unwrap().0.x,
        100.0,
        "no sweep through the circuit after a reset"
    );
}

#[test]
fn multiplayer_counts_only_valid_local_laps_and_never_imports_remote_records() {
    let mut app = App::new();
    let [s, _] = pair();
    app.insert_resource(s)
        .init_resource::<Transport>()
        .init_resource::<Sending>()
        .init_resource::<Time<Real>>()
        .init_resource::<LapTimer>()
        .init_resource::<Halt>()
        .add_message::<LapFinished>()
        .add_systems(Update, send_state);
    app.world_mut().write_message(LapFinished {
        time: 1.0,
        best: false,
        valid: false,
    });
    app.world_mut().write_message(LapFinished {
        time: 30.0,
        best: true,
        valid: true,
    });
    app.update();
    let s = app.world().resource::<Session>();
    assert_eq!(s.local_laps, 1);
    assert_eq!(s.local_best, Some(30.0));
    let mut remote = sample(1, 1.0, 0.0);
    remote.best = Some(0.1);
    app.world_mut()
        .resource_mut::<Session>()
        .receive(Packet::State(remote), 30.0);
    assert_eq!(app.world().resource::<LapTimer>().best, None);
    assert_eq!(app.world().resource::<Session>().local_best, Some(30.0));
}

#[test]
fn online_pause_releases_controls_without_stopping_the_shared_clock() {
    let [session, _] = pair();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(session)
        .init_resource::<ButtonInput<KeyCode>>()
        .add_plugins(crate::pause::PausePlugin);
    app.world_mut().resource_mut::<Schedules>().remove(Startup);
    let car = app
        .world_mut()
        .spawn((
            Player,
            Controls {
                throttle: 1.0,
                ..default()
            },
        ))
        .id();
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Escape);
    app.update();
    assert_eq!(*app.world().resource::<Halt>(), Halt::Pause);
    assert!(!app.world().resource::<Time<Virtual>>().is_paused());
    assert_eq!(
        *app.world().get::<Controls>(car).unwrap(),
        Controls::default()
    );
    // Leaving while the pause panel is open restores normal solo pause behavior.
    app.world_mut().resource_mut::<Session>().end("left");
    app.world_mut()
        .resource_mut::<ButtonInput<KeyCode>>()
        .clear();
    app.update();
    assert!(app.world().resource::<Time<Virtual>>().is_paused());
}
