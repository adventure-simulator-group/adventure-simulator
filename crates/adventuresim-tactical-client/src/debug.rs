use adventuresim_tactical_core::avian3d::prelude::*;
use adventuresim_tactical_netcode::message::SuccessfulAttackResponse;
use bevy::prelude::*;

#[derive(Component)]
struct DebugAttackCollider {
    collider: Collider,
    timer: Timer,
}

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (toggle_debug_render, draw_debug_attack_colliders))
            .add_observer(on_successful_attack);
    }
}

fn toggle_debug_render(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut config: ResMut<PhysicsDebugRenderConfig>,
) {
    if keyboard.just_pressed(KeyCode::F3) {
        config.enable_colliders = !config.enable_colliders;
        config.enable_axes = config.enable_colliders;
    }
}

fn on_successful_attack(event: On<SuccessfulAttackResponse>, mut commands: Commands) {
    let resp = event.event();
    info!("Recieved attack response: total_damage={:.1}", resp.total_damage);
    commands.spawn((
        DebugAttackCollider {
            collider: event.hitreg.clone(),
            timer: Timer::from_seconds(0.33, TimerMode::Once),
        },
        event.hitreg_transform,
    ));
}

fn draw_debug_attack_colliders(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut DebugAttackCollider, &Transform)>,
    mut gizmos: Gizmos<PhysicsGizmos>,
) {
    for (entity, mut state, transform) in &mut query {
        state.timer.tick(time.delta());

        if state.timer.is_finished() {
            commands.entity(entity).despawn();
            continue;
        }

        let fraction = EaseFunction::CubicOut.sample_unchecked(state.timer.fraction_remaining());
        let color = Color::srgba(0.0, 1.0, 0.0, fraction);

        gizmos.draw_collider(
            &state.collider,
            transform.translation,
            transform.rotation,
            color,
        );
    }
}
