use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::ClientTriggerExt,
    prelude::{DebugDumpWorldRequest, DebugGameTimeScaleRequest},
};
use bevy::{
    color::palettes::tailwind,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};
// Not a wildcard import: `adventuresim_tactical_core::prelude` already
// exports its own, unrelated `ActionState` (animation-system playback
// state), which would collide with this crate's.
use bevy_enhanced_input::prelude::{ActionMock, Actions};

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
            .init_resource::<DebugDumpWorldTrigger>()
            .register_required_components_with::<Collider, _>(|| DebugRender::none())
            .add_systems(Update, toggle_debug_visuals)
            .add_systems(Update, draw_debug_rays)
            .add_systems(Update, draw_camera_rig)
            .add_systems(Update, trigger_debug_dump_from_brp)
            .add_observer(on_hit_performed);
        register_input_mock_types(app);
    }
}

/// `bevy_enhanced_input`'s action components (`Actions<C>`, `ActionMock`)
/// derive `Reflect` but don't attach `#[reflect(Component)]` themselves (as
/// of 0.23), so BRP can't see them without this - it doesn't touch the
/// external crate, just fills in the missing type data on our own
/// `App`. Also used by `crates/adventuresim-tactical-brp-generator` so
/// `ActionMock`/`Actions<Player>` show up as generated Python classes.
///
/// Inserting `ActionMock` via BRP is how tests simulate an attack/dodge/
/// parry: it reproduces the *real* input-processing pipeline
/// (`bevy_enhanced_input::action::mock` - `Fire<A>`/`Start<A>`/etc all still
/// fire), not a bypass of it. Finding the right action entity to insert it
/// on still needs `Actions<Player>` (this client's `Player` input context,
/// `adventuresim_tactical_core::player::Player`) read via BRP.
pub fn register_input_mock_types(app: &mut App) {
    app.register_type::<Actions<Player>>();
    app.register_type_data::<Actions<Player>, ReflectComponent>();
    app.register_type::<ActionMock>();
    app.register_type_data::<ActionMock, ReflectComponent>();
}

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DebugGameSpeed {
    pub(crate) quarter_speed: bool,
}

/// Present only in `--headless` mode (see `configure_headless_render_target`
/// in `main.rs`), once the gameplay camera has been pointed at an
/// off-screen render target instead of a (nonexistent) window - F12 has
/// nothing to capture from otherwise.
#[derive(Resource)]
pub(crate) struct HeadlessScreenshotTarget(pub(crate) Handle<Image>);

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
    headless_target: Option<Res<HeadlessScreenshotTarget>>,
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

    if keyboard.just_pressed(KeyCode::F10) {
        cmd.client_trigger(DebugDumpWorldRequest);
        info!("Requested a server-side world dump");
    }

    if keyboard.just_pressed(KeyCode::F12) {
        request_screenshot(&mut cmd, headless_target.as_deref());
    }
}

/// Captures whatever the gameplay camera currently sees, windowed or
/// headless. In headless mode there's no window to capture from - see
/// `configure_headless_render_target` in `main.rs`, which points the camera
/// at an off-screen render target instead and publishes it here as
/// [`HeadlessScreenshotTarget`].
fn request_screenshot(cmd: &mut Commands, headless_target: Option<&HeadlessScreenshotTarget>) {
    let dir = std::path::Path::new("screenshots");
    if let Err(error) = std::fs::create_dir_all(dir) {
        error!(?error, "Failed to create screenshots directory");
        return;
    }
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = dir.join(format!("screenshot_{timestamp}.png"));

    let screenshot = match headless_target {
        Some(target) => Screenshot::image(target.0.clone()),
        None => Screenshot::primary_window(),
    };
    cmd.spawn(screenshot).observe(save_to_disk(path.clone()));
    info!(path = %path.display(), "Requested a screenshot");
}

/// Debug-only: a BRP-settable equivalent of the F10 keypress
/// (`DebugDumpWorldRequest`) - lets tests request a server-side world dump
/// without simulating a key event. Set to `true` via BRP
/// (`world.insert_resources`); `trigger_debug_dump_from_brp` sends the
/// request on the next frame and resets this back to `false`.
#[derive(Resource, Default, Reflect)]
#[reflect(Resource)]
pub(crate) struct DebugDumpWorldTrigger(pub(crate) bool);

fn trigger_debug_dump_from_brp(mut cmd: Commands, mut trigger: ResMut<DebugDumpWorldTrigger>) {
    if !trigger.0 {
        return;
    }
    trigger.0 = false;
    cmd.client_trigger(DebugDumpWorldRequest);
    info!("Debug: requested a server-side world dump via BRP");
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
        assert!(app.world().resource::<TerrainIkEnabled>().0);

        {
            let mut keyboard = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keyboard.release(KeyCode::F8);
            keyboard.clear_just_pressed(KeyCode::F8);
            keyboard.clear_just_released(KeyCode::F8);
            keyboard.press(KeyCode::F8);
        }
        app.update();
        assert!(!app.world().resource::<TerrainIkEnabled>().0);
    }
}
