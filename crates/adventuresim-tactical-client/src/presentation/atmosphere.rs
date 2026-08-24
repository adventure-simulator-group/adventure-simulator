//! Freeze Bevy's physically based atmosphere into one generated cubemap.

use super::*;
use bevy::light::{GeneratedEnvironmentMapLight, LightProbe, Skybox};
use bevy::{
    pbr::{ExtractedAtmosphere, GpuAtmosphereSettings, extract_atmosphere},
    render::{
        Extract, ExtractSchedule, RenderApp, extract_component::DynamicUniformIndex,
        sync_world::RenderEntity,
    },
};

// Bevy #24884 contains the native fix. Delete this backport when 0.20 is released
// and the project upgrades: https://github.com/bevyengine/bevy/pull/24884
todo_or_die::crates_io!("bevy", ">=0.20.0");

const ATMOSPHERE_CUBEMAP_BAKE_FRAMES: u8 = 60;
const ATMOSPHERE_QUIESCENCE_FRAMES: u8 = 8;
/// 64 * 64 * 6 RGBA16F texels = 192 KiB.
const FROZEN_SKY_CUBEMAP_SIZE: u32 = 64;

#[derive(Resource, Debug, Default)]
pub(crate) struct FrozenAtmosphereStatus {
    phase: FrozenAtmospherePhase,
    pub(crate) completed_bakes: u32,
}

impl FrozenAtmosphereStatus {
    pub(crate) fn is_frozen(&self) -> bool {
        matches!(
            self.phase,
            FrozenAtmospherePhase::Quiescing { .. } | FrozenAtmospherePhase::Frozen { .. }
        )
    }
}

#[derive(Debug, Default)]
enum FrozenAtmospherePhase {
    #[default]
    WaitingForScene,
    Baking {
        scene: Entity,
        ready_frames: u8,
    },
    Quiescing {
        scene: Entity,
        elapsed_frames: u8,
    },
    Frozen {
        scene: Entity,
    },
}

#[derive(Component)]
pub(in crate::presentation) struct AtmosphereBakeProbe;

#[derive(Component)]
struct FrozenAtmosphereProbeAssets {
    _environment_map: Handle<Image>,
    _diffuse_map: Handle<Image>,
    _specular_map: Handle<Image>,
}

/// Backport of Bevy #24884 for Bevy 0.19.1.
///
/// Bevy's 0.19 extractor stops visiting a camera as soon as its
/// `AtmosphereSettings` is removed. That leaves `ExtractedAtmosphere` in the
/// render world, which keeps the atmospheric PBR pipeline specialization and
/// its per-fragment transmittance work alive after the sky has been frozen.
pub(in crate::presentation) fn install_atmosphere_cleanup_backport(app: &mut App) {
    let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
        return;
    };
    render_app.add_systems(
        ExtractSchedule,
        cleanup_removed_atmosphere_settings.after(extract_atmosphere),
    );
}

fn cleanup_removed_atmosphere_settings(
    mut commands: Commands,
    cameras: Extract<Query<(RenderEntity, Option<&AtmosphereSettings>), With<Camera3d>>>,
    atmospheres: Extract<Query<(), With<Atmosphere>>>,
) {
    let atmosphere_exists = !atmospheres.is_empty();
    for (render_entity, settings) in &cameras {
        if settings.is_some() && atmosphere_exists {
            continue;
        }
        // These are the public, render-affecting part of Bevy #24884.
        // `ExtractedAtmosphere` clears the ATMOSPHERE mesh-pipeline key. Bevy
        // 0.19's uniform preparation does not retire its generated index when
        // the source component disappears, so remove that index explicitly as
        // well: the sky render node queries it and would otherwise keep using
        // stale bind groups and flashing corrupted aerial-atmosphere output.
        commands.entity(render_entity).remove::<(
            ExtractedAtmosphere,
            GpuAtmosphereSettings,
            DynamicUniformIndex<GpuAtmosphereSettings>,
        )>();
    }
}

/// Convert the one generated atmosphere cube into a visible skybox and
/// static IBL, then retire every public producer component. The
/// global `Atmosphere` remains as the inert owner of its scattering asset.
pub(in crate::presentation) fn freeze_initialized_atmosphere(
    mut commands: Commands,
    celestial: Res<PresentedCelestialLighting>,
    mut status: ResMut<FrozenAtmosphereStatus>,
    camera: Single<(Entity, &GlobalTransform), With<TacticalGameplayCamera>>,
    probe: Query<
        (
            Entity,
            Option<&GeneratedEnvironmentMapLight>,
            Option<&EnvironmentMapLight>,
        ),
        With<AtmosphereBakeProbe>,
    >,
) {
    let Some(snapshot) = celestial.snapshot.as_ref() else {
        return;
    };
    let (camera_entity, camera_transform) = camera.into_inner();

    if let FrozenAtmospherePhase::Quiescing {
        scene,
        ref mut elapsed_frames,
    } = status.phase
    {
        *elapsed_frames = elapsed_frames.saturating_add(1);
        if *elapsed_frames < ATMOSPHERE_QUIESCENCE_FRAMES {
            return;
        }
        if let Ok((probe_entity, _, _)) = probe.single() {
            commands.entity(probe_entity).remove::<(
                AtmosphereBakeProbe,
                AtmosphereEnvironmentMapLight,
                GeneratedEnvironmentMapLight,
                EnvironmentMapLight,
            )>();
        }
        status.phase = FrozenAtmospherePhase::Frozen { scene };
        status.completed_bakes += 1;
        return;
    }

    if let FrozenAtmospherePhase::Frozen { scene } = status.phase {
        if scene == snapshot.scene {
            return;
        }
        commands
            .entity(camera_entity)
            .remove::<(Skybox, EnvironmentMapLight)>()
            .insert(AtmosphereSettings::default());
        spawn_bake_probe(&mut commands, camera_transform.translation());
        status.phase = FrozenAtmospherePhase::Baking {
            scene: snapshot.scene,
            ready_frames: 0,
        };
        return;
    }

    let FrozenAtmospherePhase::Baking {
        scene,
        ref mut ready_frames,
    } = status.phase
    else {
        spawn_bake_probe(&mut commands, camera_transform.translation());
        status.phase = FrozenAtmospherePhase::Baking {
            scene: snapshot.scene,
            ready_frames: 0,
        };
        return;
    };
    if scene != snapshot.scene {
        status.phase = FrozenAtmospherePhase::Baking {
            scene: snapshot.scene,
            ready_frames: 0,
        };
        return;
    }
    let Ok((probe_entity, Some(generated), Some(filtered))) = probe.single() else {
        *ready_frames = 0;
        return;
    };
    *ready_frames = ready_frames.saturating_add(1);
    if *ready_frames < ATMOSPHERE_CUBEMAP_BAKE_FRAMES {
        return;
    }

    let mut camera_commands = commands.entity(camera_entity);
    camera_commands.remove::<AtmosphereSettings>().insert((
        Skybox {
            image: Some(generated.environment_map.clone()),
            // Skybox extraction multiplies this by view exposure; the
            // environment compute stores unexposed physical radiance.
            brightness: 1.0,
            ..default()
        },
        filtered.clone(),
    ));
    commands
        .entity(probe_entity)
        .insert(FrozenAtmosphereProbeAssets {
            _environment_map: generated.environment_map.clone(),
            _diffuse_map: filtered.diffuse_map.clone(),
            _specular_map: filtered.specular_map.clone(),
        });
    status.phase = FrozenAtmospherePhase::Quiescing {
        scene,
        elapsed_frames: 0,
    };
}

fn spawn_bake_probe(commands: &mut Commands, observer_translation: Vec3) {
    commands.spawn((
        Name::new("One-shot atmosphere cubemap probe"),
        AtmosphereBakeProbe,
        LightProbe::default(),
        Transform::from_translation(observer_translation),
        AtmosphereEnvironmentMapLight {
            size: UVec2::splat(FROZEN_SKY_CUBEMAP_SIZE),
            ..default()
        },
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_cube_budget_and_submission_windows_are_bounded() {
        assert_eq!(ATMOSPHERE_CUBEMAP_BAKE_FRAMES, 60);
        assert_eq!(ATMOSPHERE_QUIESCENCE_FRAMES, 8);
        assert_eq!(FROZEN_SKY_CUBEMAP_SIZE.pow(2) * 6 * 8, 192 * 1024);
    }

    #[test]
    fn completed_bake_installs_both_consumers_and_retires_producers() {
        let mut app = App::new();
        app.init_resource::<FrozenAtmosphereStatus>()
            .init_resource::<PresentedCelestialLighting>()
            .init_resource::<ActiveTacticalScene>()
            .add_systems(
                Update,
                (
                    update_presented_celestial_lighting,
                    freeze_initialized_atmosphere,
                )
                    .chain(),
            );
        let scene = app
            .world_mut()
            .spawn(legacy_scene_environment(&SceneId("freeze-test".into())))
            .id();
        app.world_mut().resource_mut::<ActiveTacticalScene>().entity = Some(scene);
        let camera = app
            .world_mut()
            .spawn((
                Camera3d::default(),
                TacticalGameplayCamera,
                AtmosphereSettings::default(),
            ))
            .id();
        app.world_mut().spawn(Atmosphere::earth(Handle::default()));

        app.update();
        let probe = app
            .world_mut()
            .query_filtered::<Entity, With<AtmosphereBakeProbe>>()
            .single(app.world())
            .unwrap();
        app.world_mut().entity_mut(probe).insert((
            GeneratedEnvironmentMapLight::default(),
            EnvironmentMapLight::default(),
        ));
        for _ in 0..ATMOSPHERE_CUBEMAP_BAKE_FRAMES {
            app.update();
        }

        let camera_ref = app.world().entity(camera);
        assert_eq!(camera_ref.get::<Skybox>().unwrap().brightness, 1.0);
        assert!(camera_ref.contains::<EnvironmentMapLight>());
        assert!(!camera_ref.contains::<AtmosphereSettings>());
        assert!(
            app.world()
                .entity(probe)
                .contains::<FrozenAtmosphereProbeAssets>()
        );

        for _ in 0..ATMOSPHERE_QUIESCENCE_FRAMES {
            app.update();
        }
        let retired_probe = app.world().entity(probe);
        assert!(!retired_probe.contains::<AtmosphereBakeProbe>());
        assert!(!retired_probe.contains::<AtmosphereEnvironmentMapLight>());
        assert!(!retired_probe.contains::<GeneratedEnvironmentMapLight>());
        assert!(retired_probe.contains::<FrozenAtmosphereProbeAssets>());
        assert_eq!(
            app.world()
                .resource::<FrozenAtmosphereStatus>()
                .completed_bakes,
            1
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<Atmosphere>>()
                .iter(app.world())
                .count(),
            1
        );
    }
}
