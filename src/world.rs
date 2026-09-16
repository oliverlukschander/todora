use bevy::{light::CascadeShadowConfigBuilder, prelude::*};

/// Sky, and the colour the circuit fades into at the edge of the loft.
pub const SKY: Color = Color::srgb(0.47, 0.68, 0.87);
/// Enough bounce to keep the shaded sides readable without flattening them.
/// Filling the scene with ambient is what made the circuit look hazy.
const AMBIENT: f32 = 260.0;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(SKY))
            .insert_resource(GlobalAmbientLight {
                color: SKY,
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
            shadow_maps_enabled: true,
            ..default()
        },
        // Tight cascades: the chase camera only ever sees a couple of hundred
        // metres of circuit, so the shadow texels go where they read.
        CascadeShadowConfigBuilder {
            num_cascades: 4,
            first_cascade_far_bound: 24.0,
            maximum_distance: 220.0,
            ..default()
        }
        .build(),
        Transform::from_rotation(Quat::from_euler(EulerRot::ZYX, 0.0, 0.6, -0.9)),
    ));
}
