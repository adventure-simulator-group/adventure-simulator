use super::*;

#[derive(Component)]
pub(crate) struct WeatherParticle {
    velocity: Vec3,
    ceiling: f32,
}

pub(super) fn spawn_weather_particles(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    environment: &SceneEnvironment,
) {
    let (mesh, material, fall_speed) = match environment.weather.precipitation {
        Precipitation::Clear => return,
        Precipitation::Rain => (
            meshes.add(Cuboid::new(0.06, 1.4, 0.06)),
            materials.add(StandardMaterial {
                base_color: Color::srgba(0.72, 0.84, 0.94, 0.8),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            }),
            24.0,
        ),
        Precipitation::Snow => (
            meshes.add(Sphere::new(0.065)),
            materials.add(StandardMaterial {
                base_color: Color::srgba(0.92, 0.96, 1.0, 0.9),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            }),
            3.2,
        ),
    };
    let count = 24 + usize::from(environment.weather.intensity_bps) * 104 / 10_000;
    let wind = f32::from(environment.weather.wind_speed_bps) / 10_000.0 * 8.0;
    let velocity = Vec3::new(wind, -fall_speed, wind * 0.27);
    let rotation = Quat::from_rotation_arc(Vec3::NEG_Y, velocity.normalize_or_zero());
    for index in 0..count {
        let x = fixture_coordinate(index as u64, 0) * 110.0;
        let z = fixture_coordinate(index as u64, 1) * 110.0;
        let y = 3.0 + (fixture_coordinate(index as u64, 2) + 0.5) * 32.0;
        commands.spawn((
            Name::new("Tactical weather particle"),
            WeatherParticle {
                velocity,
                ceiling: 35.0,
            },
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(x, y, z).with_rotation(rotation),
        ));
    }
}

pub(super) fn advance_weather_particles(
    time: Res<Time>,
    mut particles: Query<(&WeatherParticle, &mut Transform)>,
) {
    let delta = time.delta_secs();
    for (particle, mut transform) in &mut particles {
        transform.translation += particle.velocity * delta;
        if transform.translation.y < 0.0 {
            transform.translation.y = particle.ceiling;
            transform.translation.x = wrap_weather_coordinate(transform.translation.x);
            transform.translation.z = wrap_weather_coordinate(transform.translation.z);
        }
    }
}

pub(super) fn on_environment_added(
    event: On<Add, SceneEnvironment>,
    environments: Query<&SceneEnvironment>,
    particles: Query<Entity, With<WeatherParticle>>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) -> Result {
    let environment = environments.get(event.entity)?;
    for entity in &particles {
        commands.entity(entity).despawn();
    }
    spawn_weather_particles(&mut commands, &mut meshes, &mut materials, environment);
    Ok(())
}

pub(super) fn wrap_weather_coordinate(value: f32) -> f32 {
    (value + 55.0).rem_euclid(110.0) - 55.0
}

pub(super) fn fixture_coordinate(index: u64, axis: u64) -> f32 {
    let mut value = index ^ axis.wrapping_mul(0x9e37_79b9);
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    (value % 10_001) as f32 / 10_000.0 - 0.5
}
