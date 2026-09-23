use bevy::prelude::*;

/// Sky, and the colour the circuit fades into at the edge of the loft.
pub const SKY: Color = Color::srgb(0.47, 0.68, 0.87);
/// Soft neutral fill keeps the cars readable from every direction.
const AMBIENT: f32 = 1200.0;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(SKY))
            .insert_resource(GlobalAmbientLight {
                color: Color::WHITE,
                brightness: AMBIENT,
                ..default()
            })
            .add_systems(Startup, setup);
    }
}

fn setup(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: 16_000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_rotation(sun()),
    ));
}

/// Which way the sun faces. Its light travels along the rotated -Z, so the
/// direction towards the sun is `sun() * Vec3::Z`; the unlit trackside bakes
/// its shading from the same.
pub(crate) fn sun() -> Quat {
    Quat::from_euler(EulerRot::ZYX, 0.0, 0.6, -0.9)
}
