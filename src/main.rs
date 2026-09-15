use bevy::prelude::*;

const ACCEL: f32 = 22.0;
const BRAKE: f32 = 28.0;
const COAST: f32 = 10.0;
const MAX_SPEED: f32 = 24.0;
const MAX_REVERSE: f32 = 8.0;
const STEER_RATE: f32 = 2.4;
const CAMERA_OFFSET: Vec3 = Vec3::new(0.0, 18.0, 16.0);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Todora".into(),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .add_systems(Update, (drive_car, follow_camera))
        .run();
}

#[derive(Component, Default)]
struct Car {
    speed: f32,
}

#[derive(Component)]
struct FollowCam;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(200.0, 200.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.22, 0.28, 0.22))),
    ));

    let pylon = meshes.add(Cylinder::new(0.35, 1.6));
    let pylon_mat = materials.add(Color::srgb(0.92, 0.55, 0.12));
    for [x, z] in [[-12.0, -12.0], [12.0, -12.0], [-12.0, 12.0], [12.0, 12.0]] {
        commands.spawn((
            Mesh3d(pylon.clone()),
            MeshMaterial3d(pylon_mat.clone()),
            Transform::from_xyz(x, 0.8, z),
        ));
    }

    let body = meshes.add(Cuboid::new(1.1, 0.32, 2.0));
    let cabin = meshes.add(Cuboid::new(0.9, 0.28, 0.9));
    let nose = meshes.add(Cuboid::new(0.7, 0.12, 0.35));
    commands.spawn((
        Car::default(),
        Transform::default(),
        Visibility::default(),
        children![
            (
                Mesh3d(body),
                MeshMaterial3d(materials.add(Color::srgb(0.82, 0.16, 0.18))),
                Transform::from_xyz(0.0, 0.26, 0.0),
            ),
            (
                Mesh3d(cabin),
                MeshMaterial3d(materials.add(Color::srgb(0.12, 0.14, 0.18))),
                Transform::from_xyz(0.0, 0.52, 0.28),
            ),
            (
                Mesh3d(nose),
                MeshMaterial3d(materials.add(Color::srgb(0.95, 0.78, 0.18))),
                Transform::from_xyz(0.0, 0.28, -1.12),
            ),
        ],
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::ZYX, 0.0, 0.6, -0.9)),
    ));

    commands.spawn((
        Camera3d::default(),
        FollowCam,
        Transform::from_translation(CAMERA_OFFSET).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        Text::new("WASD / arrows — drive\nShift — brake"),
        Node {
            position_type: PositionType::Absolute,
            top: px(16),
            left: px(16),
            ..default()
        },
    ));
}

fn drive_car(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut cars: Query<(&mut Transform, &mut Car)>,
) {
    let dt = time.delta_secs();
    let throttle = axis(&keys, KeyCode::KeyW, KeyCode::ArrowUp)
        - axis(&keys, KeyCode::KeyS, KeyCode::ArrowDown);
    let steer = axis(&keys, KeyCode::KeyA, KeyCode::ArrowLeft)
        - axis(&keys, KeyCode::KeyD, KeyCode::ArrowRight);
    let braking = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    for (mut transform, mut car) in &mut cars {
        if braking {
            car.speed = move_towards(car.speed, 0.0, BRAKE * dt);
        } else if throttle > 0.0 {
            car.speed += ACCEL * throttle * dt;
        } else if throttle < 0.0 {
            car.speed += ACCEL * throttle * dt;
        } else {
            car.speed = move_towards(car.speed, 0.0, COAST * dt);
        }
        car.speed = car.speed.clamp(-MAX_REVERSE, MAX_SPEED);

        let steer_scale = (car.speed.abs() / MAX_SPEED).clamp(0.15, 1.0);
        transform.rotate_y(steer * STEER_RATE * steer_scale * dt);

        let forward = transform.forward();
        transform.translation += *forward * car.speed * dt;
        transform.translation.y = 0.0;
    }
}

fn follow_camera(
    cars: Query<&Transform, (With<Car>, Without<FollowCam>)>,
    mut cameras: Query<&mut Transform, With<FollowCam>>,
) {
    let Ok(car) = cars.single() else {
        return;
    };
    let Ok(mut camera) = cameras.single_mut() else {
        return;
    };
    let look = car.translation + Vec3::Y * 0.4;
    camera.translation = car.translation + CAMERA_OFFSET;
    camera.look_at(look, Vec3::Y);
}

fn axis(keys: &ButtonInput<KeyCode>, a: KeyCode, b: KeyCode) -> f32 {
    if keys.pressed(a) || keys.pressed(b) {
        1.0
    } else {
        0.0
    }
}

fn move_towards(value: f32, target: f32, max_delta: f32) -> f32 {
    let delta = target - value;
    if delta.abs() <= max_delta {
        target
    } else {
        value + delta.signum() * max_delta
    }
}
