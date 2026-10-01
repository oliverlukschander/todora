use super::*;
use remote::Remote;
use session::VERSION;
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
fn multiplayer_counts_only_valid_local_laps_and_never_imports_remote_records() {
    let mut app = App::new();
    let [s, _] = pair();
    app.insert_resource(s)
        .init_resource::<Transport>()
        .init_resource::<Sending>()
        .init_resource::<Time<Real>>()
        .init_resource::<Time<Fixed>>()
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

// A car driving straight along +x at 40 m/s, as its snapshots would say.
fn drive(seq: u64, time: f64) -> Snapshot {
    Snapshot {
        position: [40.0 * time as f32, 4.0, 0.0],
        velocity: [40.0, 0.0, 0.0],
        ..sample(seq, time, 0.0)
    }
}

/// Feed `snapshots` (stamp on the sender's clock, arrival on ours) to a remote
/// and draw it every 1/240 s until `until`, returning where it was drawn.
fn play(snapshots: &[(f64, f64)], skew: f64, until: f64) -> Vec<(f64, Option<Vec3>)> {
    let mut remote = Remote::default();
    let mut arrivals: Vec<_> = snapshots.iter().enumerate().collect();
    arrivals.sort_by(|a, b| a.1.1.total_cmp(&b.1.1));
    let mut next = 0;
    let mut drawn = vec![];
    let mut now = 0.0;
    while now < until {
        while next < arrivals.len() && arrivals[next].1.1 <= now {
            let (i, (stamp, _)) = arrivals[next];
            let snapshot = drive(i as u64 + 1, *stamp);
            // Their clock says a different time for the same place.
            remote.push(
                Snapshot {
                    time: stamp + skew,
                    ..snapshot
                },
                now,
            );
            next += 1;
        }
        drawn.push((now, remote.pose(now).map(|p| p.0)));
        now += 1.0 / 240.0;
    }
    drawn
}

/// 60 snapshots a second from `from` to `to`; each arrives `delay` later,
/// with `jitter` more that varies from one to the next.
fn stream(from: f64, to: f64, delay: f64, jitter: f64) -> Vec<(f64, f64)> {
    let mut noise = 12345u32;
    (0..((to - from) * 60.0) as usize)
        .map(|i| {
            noise = noise.wrapping_mul(1664525).wrapping_add(1013904223);
            let stamp = from + i as f64 / 60.0;
            (
                stamp,
                stamp + delay + jitter * (noise >> 8) as f64 / (1 << 24) as f64,
            )
        })
        .collect()
}

/// The furthest one frame's movement strays from a steady 40 m/s, in metres.
fn stumble(drawn: &[(f64, Option<Vec3>)], after: f64) -> f32 {
    drawn
        .windows(2)
        .filter(|w| w[0].0 > after)
        .map(|w| (w[1].1.unwrap().x - w[0].1.unwrap().x - 40.0 / 240.0).abs())
        .fold(0.0, f32::max)
}

#[test]
fn the_far_car_glides_however_unevenly_its_snapshots_arrive() {
    // Their clock is 87 s ahead of ours; packets take 30 to 50 ms, in any order.
    let drawn = play(&stream(0.0, 8.0, 0.03, 0.02), 87.0, 8.0);
    assert!(drawn.iter().skip(240).all(|d| d.1.is_some()));
    // At 240 frames a second 40 m/s is 16.7 cm a frame, and no frame may be
    // more than a tenth of that off.
    assert!(stumble(&drawn, 3.0) < 0.017, "{}", stumble(&drawn, 3.0));
    // Nor is the car drawn where it will be: it trails the newest snapshot.
    let x = drawn.last().unwrap().1.unwrap().x;
    assert!(x < 40.0 * (8.0 - 0.03) && x > 40.0 * (8.0 - 0.2), "{x}");
}

#[test]
fn the_far_car_keeps_going_when_snapshots_stop_and_rejoins_without_a_jump() {
    let mut snapshots = stream(0.0, 4.0, 0.03, 0.01);
    // Nothing arrives for 300 ms.
    snapshots.retain(|s| !(2.0..2.3).contains(&s.0));
    snapshots.extend(stream(4.0, 6.0, 0.03, 0.01));
    let drawn = play(&snapshots, 0.0, 6.0);
    assert!(drawn.iter().skip(240).all(|d| d.1.is_some()));
    // It is carried through the gap at speed, and eased back onto the real
    // path when the snapshots return.
    assert!(stumble(&drawn, 1.0) < 0.06, "{}", stumble(&drawn, 1.0));
}

#[test]
fn a_reset_puts_the_far_car_there_and_never_sweeps_it_across_the_circuit() {
    let mut r = Remote::default();
    for i in 0..300 {
        let t = i as f64 / 60.0;
        r.push(drive(i + 1, t), t + 0.03);
    }
    let then = 5.03;
    assert!(r.pose(then).unwrap().0.x > 150.0);
    // Back on the grid, stopped, under a new epoch.
    let mut grid = sample(301, 5.0, 0.0);
    grid.reset = 1;
    grid.velocity = [0.0; 3];
    r.push(grid, then);
    for frame in 0..30 {
        let x = r.pose(then + frame as f64 / 240.0).unwrap().0.x;
        assert!(x.abs() < 0.5, "swept through x = {x}");
    }
    // The same for a rescue, which changes nothing but where the car is.
    let mut r = Remote::default();
    for i in 0..60 {
        r.push(drive(i + 1, i as f64 / 60.0), i as f64 / 60.0 + 0.03);
    }
    let mut elsewhere = drive(61, 1.0);
    elsewhere.position = [900.0, 4.0, 500.0];
    r.push(elsewhere, 1.03);
    for frame in 0..30 {
        let p = r.pose(1.03 + frame as f64 / 240.0).unwrap().0;
        assert!(p.x > 800.0, "swept through {p}");
    }
}

#[test]
fn a_long_silence_hides_the_far_car_and_it_returns_where_it_is() {
    let mut r = Remote::default();
    for i in 0..120 {
        r.push(drive(i + 1, i as f64 / 60.0), i as f64 / 60.0 + 0.03);
    }
    assert!(r.pose(2.1).is_some());
    assert!(r.pose(2.0 + 1.6).is_none());
    r.push(drive(500, 10.0), 10.03);
    let x = r.pose(10.03).unwrap().0.x;
    assert!((x - 400.0).abs() < 5.0, "{x}");
}

#[test]
fn stale_duplicate_and_impossible_snapshots_change_nothing() {
    let mut r = Remote::default();
    for i in 0..60 {
        r.push(drive(i + 1, i as f64 / 60.0), i as f64 / 60.0 + 0.03);
    }
    let before = r.pose(1.03).unwrap();
    let mut old = drive(3, 0.05);
    old.position = [-500.0, 4.0, 0.0];
    r.push(old, 1.03);
    r.push(drive(60, 59.0 / 60.0), 1.03);
    let mut nan = drive(61, 1.0);
    nan.position[0] = f32::NAN;
    r.push(nan, 1.03);
    assert_eq!(r.latest().unwrap().seq, 60);
    assert_eq!(r.pose(1.03).unwrap(), before);
    // The height on the wire is the height drawn: a bridge is not the road below.
    assert_eq!(before.0.y, 4.0);
}

#[test]
fn the_link_is_described_by_what_arrived() {
    let mut r = Remote::default();
    // Every tenth snapshot is lost; the others take 20 ms plus up to 20 more.
    let snapshots: Vec<_> = stream(0.0, 5.0, 0.02, 0.02)
        .into_iter()
        .enumerate()
        .filter(|(i, _)| i % 10 != 9)
        .collect();
    let mut arrivals = snapshots.clone();
    arrivals.sort_by(|a, b| a.1.1.total_cmp(&b.1.1));
    for (i, (stamp, arrives)) in arrivals {
        r.push(drive(i as u64 + 1, stamp), arrives);
    }
    let link = r.stats(5.1).unwrap();
    assert!((link.lost - 0.1).abs() < 0.03, "{link:?}");
    assert!((0.008..0.03).contains(&link.jitter), "{link:?}");
    assert!((link.interval - 1.0 / 60.0).abs() < 0.004, "{link:?}");
}

#[test]
fn snapshots_from_the_last_build_still_decode() {
    // Exactly as the previous build put one on the wire.
    let wire = br#"{"State":{"seq":5,"reset":0,"time":12.5,"position":[1.0,4.0,-2.0],"rotation":[0.0,0.0,0.0,1.0],"velocity":[10.0,0.0,0.0],"lap":3.5,"invalid":false,"paused":false,"laps":0,"best":null}}"#;
    let Some(Packet::State(s)) = Packet::decode(wire) else {
        panic!("not decoded");
    };
    assert_eq!((s.seq, s.time, s.position), (5, 12.5, [1.0, 4.0, -2.0]));
    assert!(wire.len() + 200 > Packet::State(sample(1, 1.0, 1.0)).encode().len());
}

#[test]
fn snapshots_go_out_steadily_at_sixty_a_second_however_frames_come() {
    for fps in [30.0, 45.0, 60.0, 90.0, 144.0, 240.0] {
        let mut sending = Sending::default();
        let (mut now, mut sent, mut last, mut longest) = (10.0, 0, 0.0, 0.0f64);
        let mut noise = 7u32;
        while now < 20.0 {
            noise = noise.wrapping_mul(1664525).wrapping_add(1013904223);
            now += 1.0 / fps * (0.97 + 0.06 * (noise >> 8) as f64 / (1 << 24) as f64);
            if sending.due(now) {
                if last > 0.0 {
                    longest = longest.max(now - last);
                }
                sent += 1;
                last = now;
            }
        }
        let wanted = fps.min(60.0) * 10.0;
        assert!(
            (sent as f64 - wanted).abs() < wanted * 0.03,
            "{fps} fps sent {sent}"
        );
        // Never a snapshot late by more than the frame that was in the way.
        assert!(
            longest < 1.0 / fps.min(60.0) + 1.0 / fps + 0.003,
            "{fps} fps: {longest}"
        );
    }
}

#[test]
fn a_snapshot_is_stamped_with_when_the_car_was_where_it_is() {
    #[derive(Resource, Default)]
    struct Seen(Vec<(f64, f32)>);
    fn step(mut cars: Query<&mut Transform, With<Player>>, time: Res<Time>) {
        for mut car in &mut cars {
            car.translation.x += 10.0 * time.delta_secs();
        }
    }
    fn watch(
        real: Res<Time<Real>>,
        fixed: Res<Time<Fixed>>,
        cars: Query<&Transform, With<Player>>,
        mut seen: ResMut<Seen>,
    ) {
        seen.0.push((
            state_time(&real, &fixed),
            cars.single().unwrap().translation.x,
        ));
    }
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(Time::<Fixed>::from_hz(240.0))
        .init_resource::<Seen>()
        .add_systems(FixedUpdate, step)
        .add_systems(Update, watch);
    app.world_mut().spawn((Player, Transform::default()));
    // Frames of every length: shorter than a step, longer, and not a multiple.
    for ms in [16.7, 8.3, 33.0, 12.0, 4.0, 7.0, 21.0, 2.5]
        .into_iter()
        .cycle()
        .take(240)
    {
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_secs_f64(ms / 1e3),
        ));
        app.update();
    }
    let seen = &app.world().resource::<Seen>().0;
    assert!(seen.len() > 200);
    // The car at 10 m/s is at 10 * (the time its last step took it to). Stamped
    // with the frame's own time it would be up to 4 ms out, which is 4 cm.
    for (stamp, x) in seen.iter().skip(5) {
        assert!((f64::from(*x) / 10.0 - stamp).abs() < 2e-4, "{stamp}: {x}");
    }
}

#[test]
fn the_far_car_is_drawn_where_the_playback_puts_it_and_the_panel_reports_the_link() {
    fn feed(time: Res<Time<Real>>, mut session: ResMut<Session>, mut frame: Local<u64>) {
        // A snapshot per frame, taking 30 ms, from a clock 87 s ahead of ours.
        let now = time.elapsed_secs_f64();
        if now > 0.05 {
            *frame += 1;
            let stamp = now - 0.03;
            let snapshot = Snapshot {
                time: stamp + 87.0,
                ..drive(*frame, stamp)
            };
            session.receive(Packet::State(snapshot), now);
        }
    }
    let [session, _] = pair();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(session)
        .init_resource::<Halt>()
        .add_systems(Update, (feed, draw).chain());
    let car = app
        .world_mut()
        .spawn((RemoteCar, Transform::default(), Visibility::Hidden))
        .id();
    let status = app.world_mut().spawn((Status, Text::new(""))).id();
    for _ in 0..180 {
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_secs_f64(1.0 / 60.0),
        ));
        app.update();
    }
    let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
    let session = app.world().resource::<Session>();
    let (drawn, _) = session
        .remote
        .pose(now)
        .expect("the far car is on the road");
    assert_eq!(
        *app.world().get::<Visibility>(car).unwrap(),
        Visibility::Visible
    );
    assert_eq!(
        app.world().get::<Transform>(car).unwrap().translation,
        drawn
    );
    // Behind the newest snapshot by the link's delay, not ahead of it.
    let newest = 40.0 * (now - 0.03) as f32;
    assert!(
        drawn.x < newest && drawn.x > newest - 40.0 * 0.15,
        "{drawn}"
    );
    let text = &app.world().get::<Text>(status).unwrap().0;
    assert!(text.contains("ms behind · jitter"), "{text}");
}
