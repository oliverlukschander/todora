//! Automated two-process loopback smoke check, excluded from shipping builds.
use super::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};

#[derive(Resource)]
struct Check {
    path: String,
    begun: bool,
    placed: bool,
    capture_at: Option<f64>,
    captured: bool,
    /// The start lights, shot once while the agreed countdown is on.
    lights: bool,
    origin: Vec3,
}

pub(super) fn configure(app: &mut App) {
    let Ok(path) = std::env::var("TODORA_MULTIPLAYER_CHECK") else {
        return;
    };
    let id = std::env::var("TODORA_CHECK_CIRCUIT").unwrap_or("suzuka".into());
    let circuit = crate::track::all_circuits()
        .iter()
        .find(|c| c.id == id)
        .expect("test circuit");
    app.insert_resource(Track::new(circuit))
        .insert_resource(Check {
            path,
            begun: false,
            placed: false,
            capture_at: None,
            captured: false,
            lights: false,
            origin: Vec3::ZERO,
        })
        .add_systems(Startup, window)
        .add_systems(PreUpdate, start.before(NetworkSet))
        .add_systems(
            PreUpdate,
            drive.after(ReadySet).after(crate::input::InputSet),
        )
        .add_systems(PostUpdate, capture);
}

fn window(mut windows: Query<&mut Window>) {
    for mut w in &mut windows {
        w.resolution.set(800.0, 600.0);
        w.window_level = bevy::window::WindowLevel::AlwaysOnTop;
        w.position = bevy::window::WindowPosition::At(IVec2::new(
            if std::env::var("TODORA_LOCAL_PEER").as_deref() == Ok("host") {
                0
            } else {
                820
            },
            40,
        ));
        w.title = format!(
            "Todora — {}",
            std::env::var("TODORA_LOCAL_PEER").unwrap_or_default()
        );
    }
}

fn start(
    time: Res<Time<Real>>,
    mut check: ResMut<Check>,
    mut s: ResMut<Session>,
    mut transport: ResMut<Transport>,
) {
    if !check.begun && time.elapsed_secs_f64() > 1.0 {
        check.begun = true;
        s.begin(time.elapsed_secs_f64());
        transport.start();
    }
}

fn drive(
    track: Res<Track>,
    s: Res<Session>,
    mut check: ResMut<Check>,
    mut player: Query<(&mut Transform, &mut Car, &mut Controls), With<Player>>,
) {
    if !s.driving() {
        return;
    }
    let Ok((mut at, mut car, mut controls)) = player.single_mut() else {
        return;
    };
    if !check.placed {
        let points: Vec<_> = track.map_points().collect();
        let progress: f32 = std::env::var("TODORA_CHECK_PROGRESS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.78);
        let index = (progress.clamp(0.0, 0.99) * points.len() as f32) as usize;
        let (position, along) = points[index];
        let direction = (points[(index + 1) % points.len()].0 - position).normalize();
        let right = direction.cross(Vec3::Y).normalize();
        at.translation = position + right * if s.host { -0.45 } else { 0.45 };
        at.rotation = Transform::IDENTITY.looking_to(direction, Vec3::Y).rotation;
        car.along = Some(along);
        car.velocity = direction * 1.5;
        check.origin = at.translation;
        check.placed = true;
    }
    // Keep the cars slowly moving for a visible, short packet exchange.
    controls.throttle = 0.12;
    let ground = track.ground_from(at.translation, car.along);
    let heading = crate::car::level(*at.forward());
    controls.steer = (heading.cross(ground.tangent).y * 3.0).clamp(-1.0, 1.0);
}

fn capture(
    time: Res<Time<Real>>,
    mut check: ResMut<Check>,
    s: Res<Session>,
    track: Res<Track>,
    mut commands: Commands,
    mut exit: MessageWriter<AppExit>,
    player: Query<&Transform, With<Player>>,
) {
    let now = time.elapsed_secs_f64();
    if !check.lights && s.seconds_to_start(now).is_some_and(|left| left < 1.5) {
        check.lights = true;
        let path = std::path::Path::new(&check.path);
        std::fs::create_dir_all(path).expect("capture directory");
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path.join("countdown.png")))
            .observe(nonblank);
    }
    if s.driving() && s.remote.latest().is_some() && check.capture_at.is_none() {
        check.capture_at = Some(now + 2.0);
    }
    if !check.captured && check.capture_at.is_some_and(|at| now >= at) {
        check.captured = true;
        let travelled = player.single().unwrap().translation.distance(check.origin);
        assert!(
            travelled > 0.2,
            "the local car must actually drive during packet exchange"
        );
        let path = std::path::Path::new(&check.path);
        std::fs::create_dir_all(path).expect("capture directory");
        let report = serde_json::json!({"driving": s.driving(), "circuit": track.circuit().id,
            "host": s.host, "remote_seq": s.remote.latest().unwrap().seq,
            "remote_visible": s.remote.pose(now).is_some(), "remote_position": s.remote.latest().unwrap().position,
            "start_at": s.start_at, "travelled": travelled});
        std::fs::write(
            path.join("report.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path.join("screen.png")))
            .observe(nonblank);
    }
    if check.captured && check.capture_at.is_some_and(|at| now > at + 2.0) {
        exit.write(AppExit::Success);
    } else if now > 45.0 {
        panic!(
            "multiplayer smoke check timed out: {:?}: {}",
            s.phase, s.status
        );
    }
}

fn nonblank(capture: On<ScreenshotCaptured>) {
    let pixels = capture.image.data.as_ref().expect("captured pixels");
    assert!(
        pixels
            .chunks_exact(4)
            .any(|p| p[..3].iter().any(|c| *c > 0)),
        "capture is black; keep both test windows visible on the desktop"
    );
}
