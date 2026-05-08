use adventuresim_tactical_core::prelude::*;
use adventuresim_tactical_netcode::{
    bevy_replicon::prelude::{FromClient, SendMode, ServerTriggerExt, ToClients},
    message::SuccessfulAttackResponse,
    prelude::AttackRequest,
};
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
    event: On<FromClient<AttackRequest>>,
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
    mut q_attack: Query<(Entity, &Transform, &CharacterLook, &mut AttackState)>,
    q_name: Query<NameOrEntity>,
    q_collider: Query<&ColliderOf>,
    spatial_query: SpatialQuery,
) {
    for (entity, transform, look, mut state) in &mut q_attack {
        state.pre_hit_timer.tick(time.delta());
        if !state.pre_hit_timer.is_finished() {
            continue;
        }
        cmd.entity(entity).remove::<AttackState>();

        let look = look.to_quat();
        let origin = transform.translation + Vec3::new(0.0, 0.85, 0.0);
        let mut hitreg_transform = Transform::from_translation(origin + config.hitreg_translation);
        hitreg_transform.rotate_around(origin, look);

        let filter = SpatialQueryFilter::default()
            .with_mask(HITBOX_LAYER)
            .with_excluded_entities([entity]);

        let hit = spatial_query.shape_intersections(
            &config.hitreg_shape,
            hitreg_transform.translation,
            hitreg_transform.rotation,
            &filter,
        );

        for &hit in &hit {
            let Ok([e1, e2]) = q_collider
                .get_many([entity, hit])
                .and_then(|[c1, c2]| q_name.get_many([c1.body, c2.body]))
            else {
                continue;
            };
            info!("'{e1}' hit '{e2}'");
        }

        cmd.server_trigger(ToClients {
            mode: SendMode::CLIENTS_ONLY,
            message: SuccessfulAttackResponse {
                attacker: entity,
                hit,
                hitreg: config.hitreg_shape.clone(),
                hitreg_transform,
            },
        });
    }
}
