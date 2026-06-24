use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::message::SuccessfulAttackResponse;
use bevy::{color::palettes::tailwind, prelude::*};

use crate::player::{ClientPlayer, HitPerformed, LimbHitbox};

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DebugVisualsConfig>()
            .register_required_components_with::<Collider, _>(|| DebugRender::none())
            .add_systems(Update, toggle_debug_visuals)
            .add_systems(Update, draw_debug_rays)
            .add_observer(on_successful_attack)
            .add_observer(on_hit_performed);
    }
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
    q_colliders: Query<(Entity, Option<&LimbHitbox>), (With<Collider>, Without<ClientPlayer>)>,
    mut cmd: Commands,
) {
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
        if let Some(asset) = gizmo_assets.get_mut(&ray.handle) {
            for color in &mut asset.list_colors {
                color.set_alpha(alpha);
            }
        }
    }
}

fn on_successful_attack(event: On<SuccessfulAttackResponse>) {
    info!(
        "Hit on {:?} bodypart for {:.1} damage ({:.1} cut + {:.1} blunt)",
        event.body_part,
        event.total_damage(),
        event.cut_damage,
        event.blunt_damage
    );
}
