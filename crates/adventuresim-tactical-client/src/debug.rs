use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::ClientTriggerExt, prelude::DebugGameTimeScaleRequest,
};
use bevy::{color::palettes::tailwind, prelude::*};

use crate::{
    animation::TerrainIkEnabled,
    camera::{CameraAimState, CameraDebugEnabled, CameraRigConfig, CameraRigDebugState},
    player::{ClientPlayer, HitPerformed, LimbHitbox},
};

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugVisualsConfig>()
            .init_resource::<DebugGameSpeed>()
            .register_required_components_with::<Collider, _>(DebugRender::none)
            .add_systems(Update, toggle_debug_visuals)
            .add_systems(Update, draw_debug_rays)
            .add_systems(Update, draw_camera_rig)
            .add_observer(on_hit_performed);
    }
}

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DebugGameSpeed {
    pub(crate) quarter_speed: bool,
}

#[derive(Resource)]
struct DebugVisualsConfig {
    physics_colliders: bool,
    hitboxes: bool,
    raycast: bool,
}

impl Default for DebugVisualsConfig {
    fn default() -> Self {
        Self {
            physics_colliders: false,
            hitboxes: false,
            raycast: true,
        }
    }
}

#[derive(Component)]
struct DebugRay {
    timer: Timer,
    handle: Handle<GizmoAsset>,
}

fn toggle_debug_visuals(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut config: ResMut<DebugVisualsConfig>,
    mut terrain_ik: ResMut<TerrainIkEnabled>,
    mut game_speed: ResMut<DebugGameSpeed>,
    mut virtual_time: ResMut<Time<Virtual>>,
    q_colliders: Query<(Entity, Option<&LimbHitbox>), (With<Collider>, Without<ClientPlayer>)>,
    mut cmd: Commands,
) {
    if keyboard.just_pressed(KeyCode::F7) {
        game_speed.quarter_speed = !game_speed.quarter_speed;
        let request = DebugGameTimeScaleRequest {
            quarter_speed: game_speed.quarter_speed,
        };
        let relative_speed = request.relative_speed();
        virtual_time.set_relative_speed(relative_speed);
        cmd.client_trigger(request);
        info!(relative_speed, "Debug game speed toggled");
    }

    if keyboard.just_pressed(KeyCode::F2) {
        config.physics_colliders = !config.physics_colliders;
        for (entity, hitbox) in &q_colliders {
            if hitbox.is_none() {
                cmd.entity(entity).insert(if config.physics_colliders {
                    DebugRender::collider(tailwind::AMBER_200.into()).with_axes(Vec3::splat(0.33))
                } else {
                    DebugRender::none()
                });
            }
        }
    }

    if keyboard.just_pressed(KeyCode::F3) {
        config.hitboxes = !config.hitboxes;
        for (entity, hitbox) in &q_colliders {
            if let Some(hitbox) = hitbox {
                let color = limb_hitbox_color(hitbox.0);
                cmd.entity(entity).insert(if config.hitboxes {
                    DebugRender::collider(color)
                } else {
                    DebugRender::none()
                });
            }
        }
    }

    if keyboard.just_pressed(KeyCode::F4) {
        config.raycast = !config.raycast;
    }

    if keyboard.just_pressed(KeyCode::F8) {
        terrain_ik.0 = !terrain_ik.0;
        info!(enabled = terrain_ik.0, "Terrain leg IK toggled");
    }
}

fn limb_hitbox_color(body_part: BodyPart) -> Color {
    match body_part {
        BodyPart::LeftArm | BodyPart::RightArm => tailwind::LIME_600,
        BodyPart::LeftLeg | BodyPart::RightLeg => tailwind::SKY_600,
        BodyPart::Head => tailwind::RED_300,
        BodyPart::Chest => tailwind::PINK_300,
        BodyPart::Stomach => tailwind::PURPLE_300,
    }
    .into()
}

fn on_hit_performed(
    event: On<HitPerformed>,
    mut cmd: Commands,
    mut gizmo_assets: ResMut<Assets<GizmoAsset>>,
    config: Res<DebugVisualsConfig>,
) {
    if !config.raycast {
        return;
    }

    let hit = event.event();
    let end = hit.origin + *hit.direction * hit.length;

    let mut asset = GizmoAsset::default();
    asset.line(hit.origin, end, tailwind::ROSE_600);
    let handle = gizmo_assets.add(asset);

    cmd.spawn((
        DebugRay {
            timer: Timer::from_seconds(2.0, TimerMode::Once),
            handle: handle.clone(),
        },
        Gizmo {
            handle,
            line_config: GizmoLineConfig {
                width: 8.0,
                ..default()
            },
            depth_bias: 0.0,
        },
    ));
}

fn draw_debug_rays(
    time: Res<Time>,
    mut cmd: Commands,
    mut gizmo_assets: ResMut<Assets<GizmoAsset>>,
    mut q_rays: Query<(Entity, &mut DebugRay)>,
) {
    for (entity, mut ray) in &mut q_rays {
        ray.timer.tick(time.delta());

        if ray.timer.is_finished() {
            cmd.entity(entity).despawn();
            continue;
        }

        let alpha = EaseFunction::QuadraticOut.sample_unchecked(ray.timer.fraction_remaining());
        if let Some(mut asset) = gizmo_assets.get_mut(&ray.handle) {
            for color in &mut asset.list_colors {
                color.set_alpha(alpha);
            }
        }
    }
}

fn draw_camera_rig(
    enabled: Res<CameraDebugEnabled>,
    rig: Res<CameraRigDebugState>,
    aim: Res<CameraAimState>,
    config: Res<CameraRigConfig>,
    mut gizmos: Gizmos,
) {
    if !enabled.0 || !rig.active {
        return;
    }
    gizmos.line(rig.subject, rig.focus, tailwind::LIME_400);
    gizmos.line(rig.focus, rig.shoulder, tailwind::SKY_300);
    gizmos.line(rig.shoulder, rig.desired_endpoint, tailwind::AMBER_300);
    gizmos.line(rig.shoulder, rig.final_endpoint, tailwind::CYAN_300);
    if rig.collision_entity.is_some() {
        gizmos.line(
            rig.final_endpoint,
            rig.final_endpoint + rig.collision_normal * 0.6,
            tailwind::RED_400,
        );
    }
    if rig.soft_occluder.is_some() {
        gizmos.line(
            rig.soft_occluder_point - Vec3::Y * 0.25,
            rig.soft_occluder_point + Vec3::Y * 0.25,
            tailwind::ORANGE_400,
        );
    }
    let radius = Vec3::splat(config.collision_radius);
    gizmos.line(
        rig.final_endpoint - Vec3::X * radius.x,
        rig.final_endpoint + Vec3::X * radius.x,
        tailwind::CYAN_200,
    );
    gizmos.line(
        rig.final_endpoint - Vec3::Y * radius.y,
        rig.final_endpoint + Vec3::Y * radius.y,
        tailwind::CYAN_200,
    );
    if aim.active {
        gizmos.line(aim.camera_origin, aim.camera_target, tailwind::PURPLE_300);
        gizmos.line(
            aim.muzzle_origin,
            aim.actual_target,
            if aim.blocked {
                tailwind::RED_500
            } else {
                tailwind::GREEN_400
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f8_toggles_terrain_ik() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<DebugVisualsConfig>()
            .init_resource::<DebugGameSpeed>()
            .init_resource::<TerrainIkEnabled>()
            .init_resource::<Time<Virtual>>()
            .add_systems(Update, toggle_debug_visuals);

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F8);
        app.update();
        assert!(!app.world().resource::<TerrainIkEnabled>().0);

        {
            let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keyboard.release(KeyCode::F8);
            keyboard.clear_just_pressed(KeyCode::F8);
            keyboard.clear_just_released(KeyCode::F8);
            keyboard.press(KeyCode::F8);
        }
        app.update();
        assert!(app.world().resource::<TerrainIkEnabled>().0);
    }
}
