use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::message::{ImpactSound, SuccessfulAttackResponse};
use bevy::audio::{PlaybackMode, Volume};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::{audio_config::TacticalAudioConfig, presentation::ProceduralEnvironmentAssets};

mod blood_decals;
use blood_decals::{BloodDecalPlugin, BloodMaskMaterial, BloodMaterialAssets, BloodSurfaceQuery};

const SPARK_COUNT: usize = 10;
const BLOOD_COUNT: usize = 14;
const GRAVITY_METRES_PER_SECOND_SQUARED: f32 = 9.81;

pub struct CombatEffectsPlugin;

impl Plugin for CombatEffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((crate::movement_audio::MovementAudioPlugin, BloodDecalPlugin))
            .init_resource::<CombatEffectAssets>()
            .add_observer(spawn_combat_effects)
            .add_systems(Update, (move_combat_particles, fade_sparks));
    }
}

#[derive(Resource, Default)]
struct CombatEffectAssets {
    spark_mesh: Handle<Mesh>,
    blood_mesh: Handle<Mesh>,
    spark_material: Handle<StandardMaterial>,
    blood_material: Handle<StandardMaterial>,
}

impl CombatEffectAssets {
    fn prepare(&mut self, meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) {
        if self.spark_mesh == Handle::default() {
            self.spark_mesh = meshes.add(Cuboid::new(0.008, 0.008, 0.09));
            self.blood_mesh =
                meshes.add(Sphere::new(0.018).mesh().ico(1).expect("valid icosphere"));
            self.spark_material = materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.55, 0.08),
                emissive: LinearRgba::new(8.0, 2.2, 0.15, 1.0),
                ..default()
            });
            self.blood_material = materials.add(StandardMaterial {
                base_color: Color::srgb(0.22, 0.002, 0.006),
                perceptual_roughness: 0.9,
                ..default()
            });
        }
    }
}

#[derive(SystemParam)]
struct CombatEffectResources<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    images: ResMut<'w, Assets<Image>>,
    blood_materials: ResMut<'w, Assets<BloodMaskMaterial>>,
    assets: ResMut<'w, CombatEffectAssets>,
    asset_server: Res<'w, AssetServer>,
    config: Res<'w, TacticalAudioConfig>,
}

#[derive(Component)]
struct CombatParticle {
    velocity: Vec3,
    remaining_seconds: f32,
    blood: bool,
}

#[derive(Component)]
struct Spark;

fn spread_direction(normal: Vec3, index: usize, count: usize, upward: f32) -> Vec3 {
    let angle = index as f32 * std::f32::consts::TAU / count as f32 + index as f32 * 0.71;
    let tangent = normal.any_orthonormal_vector();
    let bitangent = normal.cross(tangent).normalize_or_zero();
    (normal * 0.55 + tangent * angle.cos() + bitangent * angle.sin() + Vec3::Y * upward)
        .normalize_or(Vec3::Y)
}

#[expect(
    clippy::too_many_arguments,
    reason = "combat effects independently borrow event targets, render assets, hierarchy, surfaces, and deterministic variation state"
)]
fn spawn_combat_effects(
    event: On<SuccessfulAttackResponse>,
    mut commands: Commands,
    targets: Query<&GlobalTransform>,
    mut resources: CombatEffectResources,
    parents: Query<&ChildOf>,
    surfaces: BloodSurfaceQuery,
    mut sound_sequence: Local<u64>,
    mut effect_sequence: Local<u64>,
) {
    if !event.impact_effects.metal_sparks
        && !event.impact_effects.blood
        && event.impact_effects.sound == ImpactSound::None
    {
        return;
    }
    let Some(target) = event.hit.first().copied() else {
        return;
    };
    let Ok(target_transform) = targets.get(target) else {
        return;
    };
    resources
        .assets
        .prepare(&mut resources.meshes, &mut resources.materials);
    let world_point = target_transform.transform_point(event.impact_point);
    let world_normal = target_transform
        .affine()
        .transform_vector3(event.impact_normal)
        .normalize_or(Vec3::Y);
    play_impact_sound(
        &mut commands,
        &resources.asset_server,
        event.impact_effects.sound,
        world_point,
        &mut sound_sequence,
        &resources.config,
    );

    if event.impact_effects.metal_sparks {
        for index in 0..SPARK_COUNT {
            let direction = spread_direction(world_normal, index, SPARK_COUNT, 0.35);
            commands.spawn((
                Name::new("metal-impact-spark"),
                Mesh3d(resources.assets.spark_mesh.clone()),
                MeshMaterial3d(resources.assets.spark_material.clone()),
                Transform::from_translation(world_point + world_normal * 0.015)
                    .with_rotation(Quat::from_rotation_arc(Vec3::Z, direction)),
                CombatParticle {
                    velocity: direction * (2.8 + index as f32 * 0.09),
                    remaining_seconds: 0.24,
                    blood: false,
                },
                Spark,
            ));
        }
    }

    if event.impact_effects.blood {
        *effect_sequence = effect_sequence.wrapping_add(1);
        blood_decals::stamp_character_blood(
            &mut commands,
            target,
            world_point,
            *effect_sequence,
            &parents,
            &surfaces,
            &mut BloodMaterialAssets {
                meshes: &resources.meshes,
                images: &mut resources.images,
                standard: &resources.materials,
                blood: &mut resources.blood_materials,
            },
        );
        for index in 0..BLOOD_COUNT {
            let direction = spread_direction(world_normal, index, BLOOD_COUNT, 0.55);
            commands.spawn((
                Name::new("blood-particle"),
                Mesh3d(resources.assets.blood_mesh.clone()),
                MeshMaterial3d(resources.assets.blood_material.clone()),
                Transform::from_translation(world_point + world_normal * 0.02),
                CombatParticle {
                    velocity: direction * (1.35 + index as f32 * 0.035),
                    remaining_seconds: 1.6,
                    blood: true,
                },
            ));
        }
    }
}

fn play_impact_sound(
    commands: &mut Commands,
    asset_server: &AssetServer,
    sound: ImpactSound,
    world_point: Vec3,
    sequence: &mut u64,
    config: &TacticalAudioConfig,
) {
    let family = match sound {
        ImpactSound::None => return,
        ImpactSound::Metal => "impactMetal_medium_00",
        ImpactSound::CutFlesh => "impactSoft_medium_00",
        ImpactSound::BluntFlesh => "impactPunch_medium_00",
        ImpactSound::NonMetalWeapon => "impactWood_medium_00",
    };
    *sequence = sequence.wrapping_add(1);
    let mut sample = sequence
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(u64::from(world_point.x.to_bits()))
        .wrapping_add(u64::from(world_point.z.to_bits()).rotate_left(23));
    sample ^= sample >> 29;
    let variant = sample % 3;
    let pitch_fraction = ((sample >> 32) as u32) as f32 / u32::MAX as f32;
    let pitch = config.combat.impact_pitch_randomization;
    let speed = pitch[0] + pitch_fraction * (pitch[1] - pitch[0]);
    let path = format!("audio/combat/kenney-impact-sounds/{family}{variant}.ogg");
    commands.spawn((
        Name::new("combat-impact-sound"),
        AudioPlayer::new(asset_server.load(path)),
        PlaybackSettings {
            mode: PlaybackMode::Despawn,
            speed,
            spatial: true,
            volume: Volume::Linear(config.combat.impact_relative_volume),
            ..default()
        },
        Transform::from_translation(world_point),
    ));
}

#[expect(
    clippy::too_many_arguments,
    reason = "particle integration independently borrows collision, terrain presentation, mutable masks, and particle state"
)]
fn move_combat_particles(
    mut commands: Commands,
    time: Res<Time>,
    spatial: SpatialQuery,
    procedural_assets: Res<ProceduralEnvironmentAssets>,
    terrains: Query<&SceneTerrain>,
    mut images: ResMut<Assets<Image>>,
    mut terrain_stain_sequence: Local<u64>,
    mut particles: Query<(Entity, &mut Transform, &mut CombatParticle)>,
) {
    let delta_seconds = time.delta_secs();
    for (entity, mut transform, mut particle) in &mut particles {
        particle.remaining_seconds -= delta_seconds;
        if particle.remaining_seconds <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        if particle.blood {
            particle.velocity.y -= GRAVITY_METRES_PER_SECOND_SQUARED * delta_seconds;
        }
        let movement = particle.velocity * delta_seconds;
        let distance = movement.length();
        if particle.blood
            && distance > f32::EPSILON
            && let Ok(direction) = Dir3::new(movement)
            && let Some(hit) = spatial.cast_ray(
                transform.translation,
                direction,
                distance,
                true,
                &SpatialQueryFilter::from_mask(TACTICAL_TERRAIN_LAYER),
            )
        {
            let point = transform.translation + movement.normalize() * hit.distance;
            if let Some(terrain) = terrains.iter().find(|terrain| {
                point.x.abs() <= terrain.width() * 0.5 && point.z.abs() <= terrain.depth() * 0.5
            }) && let Some(mut mask) = images.get_mut(&procedural_assets.terrain_blood_mask)
            {
                *terrain_stain_sequence = terrain_stain_sequence.wrapping_add(1);
                blood_decals::stamp_terrain_blood(
                    &mut mask,
                    point,
                    Vec2::new(terrain.width(), terrain.depth()),
                    *terrain_stain_sequence,
                );
            }
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation += movement;
        if !particle.blood {
            transform.rotation =
                Quat::from_rotation_arc(Vec3::Z, particle.velocity.normalize_or(Vec3::Y));
        }
    }
}

fn fade_sparks(time: Res<Time>, mut sparks: Query<&mut Transform, With<Spark>>) {
    let scale = (1.0 - time.delta_secs() * 7.0).max(0.35);
    for mut transform in &mut sparks {
        transform.scale *= scale;
    }
}
