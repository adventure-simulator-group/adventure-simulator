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
    q_attacker: Query<Has<AttackState>>,
) {
    let Some(entity) = event.client_id.entity() else {
        return;
    };
    if q_attacker.get(entity).unwrap_or_default() {
        return;
    }

    cmd.entity(entity)
        .insert(AttackState::new(attack_config.pre_hit_delay));
}

fn attack_state_update_system(
    mut cmd: Commands,
    time: Res<Time>,
    config: Res<AttackConfig>,
    mut q_attack: Query<(
        Entity,
        &RigidBodyColliders,
        &Transform,
        &CharacterLook,
        &mut AttackState,
    )>,
    q_player: Query<&Player>,
    q_collider: Query<&ColliderOf>,
    spatial_query: SpatialQuery,
    viewer: TacticalPlayerViewer,
) {
    for (entity, colliders, transform, look, mut state) in &mut q_attack {
        state.pre_hit_timer.tick(time.delta());
        if !state.pre_hit_timer.is_finished() {
            continue;
        }
        cmd.entity(entity).remove::<AttackState>();

        let Some(attacker_view) = viewer.get(entity) else {
            continue;
        };

        let look = look.to_quat();
        let origin = transform.translation + Vec3::new(0.0, 0.85, 0.0);
        let mut hitreg_transform = Transform::from_translation(origin + config.hitreg_translation);
        hitreg_transform.rotate_around(origin, look);

        let filter = SpatialQueryFilter::default()
            .with_mask(HITBOX_LAYER)
            .with_excluded_entities(colliders);

        let hit = spatial_query.shape_intersections(
            &config.hitreg_shape,
            hitreg_transform.translation,
            hitreg_transform.rotation,
            &filter,
        );

        let mut total_damage = 0.0;
        for &hit_entity in &hit {
            let defender_entity = q_collider
                .get(hit_entity)
                .ok()
                .map(|c| c.body)
                .filter(|&e| q_player.contains(e))
                .or_else(|| {
                    if q_player.contains(hit_entity) {
                        Some(hit_entity)
                    } else {
                        None
                    }
                });

            let Some(defender_entity) = defender_entity else {
                continue;
            };

            let Some(defender_view) = viewer.get(defender_entity) else {
                continue;
            };

            let Some(attacker_side) = attacker_view.weapon_holding_side() else {
                warn!("Attacker isn't holding any weapon!");
                continue;
            };
            let result = attacker_view.resolve_melee_attack(attacker_side, &defender_view);

            let cut = result.cut_damage;
            let blunt = result.blunt_damage;
            let damage = cut + blunt;

            if damage > 0.0 {
                let chest = defender_view.body_part_health(BodyPart::Chest);
                cmd.entity(defender_entity).insert(Limbs {
                    chest: (chest - damage).max(0.0),
                    left_arm: defender_view.body_part_health(BodyPart::LeftArm),
                    right_arm: defender_view.body_part_health(BodyPart::RightArm),
                    left_leg: defender_view.body_part_health(BodyPart::LeftLeg),
                    right_leg: defender_view.body_part_health(BodyPart::RightLeg),
                    stomach: defender_view.body_part_health(BodyPart::Stomach),
                    head: defender_view.body_part_health(BodyPart::Head),
                });
            }

            info!(
                "hit'{defender_entity:?}' for {damage} total damage ({cut:.1} cut + {blunt:.1} blunt) | outcome: {:?}",
                result.outcome,
            );

            total_damage += damage;
        }

        cmd.server_trigger(ToClients {
            mode: SendMode::CLIENTS_ONLY,
            message: SuccessfulAttackResponse {
                attacker: entity,
                hit,
                hitreg: config.hitreg_shape.clone(),
                hitreg_transform,
                total_damage,
            },
        });
    }
}
