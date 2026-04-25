use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{bevy_replicon::prelude::FromClient, prelude::AttackCommand};
use bevy::prelude::*;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AttackConfig>()
            .add_observer(on_attack_action_triggered)
            .add_systems(Update, attack_state_update_system);
    }
}

fn on_attack_action_triggered(
    event: On<FromClient<AttackCommand>>,
    mut cmd: Commands,
    attack_config: Res<AttackConfig>,
) {
    let Some(entity) = event.client_id.entity() else {
        return;
    };

    cmd.entity(entity)
        .insert(AttackState::new(attack_config.pre_hit_delay));
}

fn attack_state_update_system(
    mut cmd: Commands,
    time: Res<Time>,
    config: Res<AttackConfig>,
    mut q_attack: Query<(Entity, &Transform, &mut AttackState)>,
    spatial_query: SpatialQuery,
) {
    for (entity, transform, mut state) in &mut q_attack {
        state.pre_hit_timer.tick(time.delta());
        if !state.pre_hit_timer.is_finished() {
            continue;
        }

        let origin = transform.translation + config.hitreg_translation;

        let filter = SpatialQueryFilter::default()
            .with_mask(HITBOX_LAYER)
            .with_excluded_entities([entity]);

        let cast_config = ShapeCastConfig::DEFAULT;

        if let Some(hit) = spatial_query.cast_shape(
            &config.hitreg_shape,
            origin,
            Quat::IDENTITY,
            Dir3::Z,
            &cast_config,
            &filter,
        ) {
            info!(
                "Entity {:?} hit entity {:?} at distance {}",
                entity, hit.entity, hit.distance
            );
        }

        cmd.entity(entity).remove::<AttackState>();
    }
}
