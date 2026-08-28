use super::*;

type EquipmentContactQuery<'world, 'state> = Query<
    'world,
    'state,
    (
        &'static ItemOf,
        &'static EquipSlot,
        &'static crate::player_projection::TacticalInventoryItemId,
        Option<&'static WeaponItem>,
        Option<&'static ShieldItem>,
        Option<&'static ArmorItem>,
    ),
>;

pub(super) fn apply_melee_attack_result(
    event: On<ApplyMeleeAttackResult>,
    mut combatants: Query<(&mut Limbs, &mut TacticalCombatState)>,
    mut velocities: Query<&mut LinearVelocity>,
    metadata: Query<(&TacticalCombatSide, &CharacterId)>,
    mut consequences: ResMut<TacticalConsequenceAccumulator>,
    items: EquipmentContactQuery<'_, '_>,
) {
    let Ok([attacker, defender]) = combatants.get_many_mut([event.attacker, event.target]) else {
        return;
    };
    let (_, mut attacker_state) = attacker;
    let (mut defender_limbs, mut defender_state) = defender;
    let applied = apply_transient_attack_result(
        &mut attacker_state,
        &mut defender_limbs,
        &mut defender_state,
        event.result,
        event.body_part,
    );
    if let Ok(mut velocity) = velocities.get_mut(event.impact_recipient) {
        velocity.0 += event.impact_velocity_change;
    }
    let attacker_metadata = metadata.get(event.attacker).ok();
    let defender_metadata = metadata.get(event.target).ok();
    if defender_metadata.is_some_and(|(side, _)| *side == TacticalCombatSide::Party)
        && let Some((cut_damage, blunt_damage)) = applied
    {
        let defender_id = defender_metadata.unwrap().1;
        record_party_injury(
            &mut consequences,
            *defender_id,
            event.body_part,
            cut_damage,
            blunt_damage,
        );
    }
    if attacker_metadata.is_some_and(|(side, _)| *side == TacticalCombatSide::Party) {
        let attacker_id = attacker_metadata.unwrap().1;
        let contact_stress = match event.result {
            AttackResult::ToAttacker { contact_force, .. }
            | AttackResult::ToDefender { contact_force, .. } => contact_force.max(0.0),
        };
        if event.attacker_weapon_contact
            && contact_stress > 0.0
            && let Some((_, _, provenance, _, _, _)) = items.iter().find(|row| {
                let (owner, slot, _, weapon, _, _) = row;
                attacker_weapon_contact_matches(
                    event.attacker,
                    owner.0,
                    **slot,
                    event.attacker_weapon_slot,
                    weapon.is_some(),
                )
            })
        {
            record_equipment_contact(
                &mut consequences,
                *attacker_id,
                provenance.0,
                contact_stress,
                false,
            );
        }
    }
    if defender_metadata.is_some_and(|(side, _)| *side == TacticalCombatSide::Party) {
        let defender_id = defender_metadata.unwrap().1;
        let (contact_stress, defender_slot, require_shield, require_armor) = match event.result {
            AttackResult::ToDefender {
                contact_force,
                armor_contact,
                ..
            } if armor_contact => (
                contact_force.max(0.0),
                Some(EquipSlot::from_armor_body_part(event.body_part)),
                false,
                true,
            ),
            AttackResult::ToAttacker {
                contact_force,
                physical_contact: true,
                ..
            } if event.defender_parry_slot.is_some() => (
                contact_force.max(0.0),
                event.defender_parry_slot,
                true,
                false,
            ),
            _ => (0.0, None, false, false),
        };
        if contact_stress > 0.0
            && let Some((_, _, provenance, _, _, _)) = items.iter().find(|row| {
                let (owner, slot, _, _, shield, armor) = row;
                defender_equipment_contact_matches(
                    event.target,
                    owner.0,
                    **slot,
                    defender_slot,
                    shield.is_some(),
                    armor.is_some(),
                    require_shield,
                    require_armor,
                )
            })
        {
            record_equipment_contact(
                &mut consequences,
                *defender_id,
                provenance.0,
                contact_stress,
                true,
            );
        }
    }
}

pub(super) fn record_party_injury(
    consequences: &mut TacticalConsequenceAccumulator,
    character_id: CharacterId,
    body_part: BodyPart,
    cut_damage: f32,
    blunt_damage: f32,
) {
    let consequence = consequences.party.entry(character_id).or_default();
    if let Some(injury) = consequence
        .injuries
        .iter_mut()
        .find(|injury| injury.body_part == body_part)
    {
        injury.cut_damage += cut_damage;
        injury.blunt_damage += blunt_damage;
        injury.max_single_hit_blunt_damage = injury.max_single_hit_blunt_damage.max(blunt_damage);
    } else {
        consequence.injuries.push(AppliedTacticalInjury {
            body_part,
            cut_damage,
            blunt_damage,
            max_single_hit_blunt_damage: blunt_damage,
        });
    }
    consequence.blood_loss_fraction = (consequence.blood_loss_fraction
        + blood_loss_from_applied_health_damage(body_part, cut_damage, blunt_damage))
    .clamp(0.0, 1.0);
}

pub(super) fn record_party_ammunition_use(
    consequences: &mut TacticalConsequenceAccumulator,
    character_id: CharacterId,
) {
    let consequence = consequences.party.entry(character_id).or_default();
    consequence.ammunition_used = consequence
        .ammunition_used
        .saturating_add(1)
        .min(adventuresim_core::mission::MAX_TACTICAL_AMMUNITION_USED);
}

pub(super) fn attacker_weapon_contact_matches(
    attacker: Entity,
    owner: Entity,
    slot: EquipSlot,
    authoritative_slot: EquipSlot,
    is_weapon: bool,
) -> bool {
    owner == attacker && slot == authoritative_slot && is_weapon
}

#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
pub(super) fn defender_equipment_contact_matches(
    defender: Entity,
    owner: Entity,
    slot: EquipSlot,
    required_slot: Option<EquipSlot>,
    is_shield: bool,
    is_armor: bool,
    require_shield: bool,
    require_armor: bool,
) -> bool {
    owner == defender
        && required_slot.is_none_or(|expected| slot == expected)
        && (!require_shield || is_shield)
        && (!require_armor || is_armor)
}

fn record_equipment_contact(
    consequences: &mut TacticalConsequenceAccumulator,
    character_id: CharacterId,
    inventory_item_id: u64,
    contact_stress: f32,
    defender_equipment: bool,
) {
    if let Some(existing) = consequences
        .equipment_contacts
        .iter_mut()
        .find(|contact| contact.inventory_item_id == inventory_item_id)
    {
        existing.contact_stress = (existing.contact_stress + contact_stress)
            .min(adventuresim_core::mission::MAX_TACTICAL_CONTACT_STRESS);
    } else if consequences.equipment_contacts.len()
        < adventuresim_core::mission::MAX_TACTICAL_EQUIPMENT_CONTACTS
    {
        consequences
            .equipment_contacts
            .push(AccumulatedEquipmentContact {
                character_id,
                inventory_item_id,
                contact_stress: contact_stress
                    .min(adventuresim_core::mission::MAX_TACTICAL_CONTACT_STRESS),
                defender_equipment,
            });
    }
}

pub(crate) fn apply_transient_attack_result(
    attacker_state: &mut TacticalCombatState,
    defender_limbs: &mut Limbs,
    defender_state: &mut TacticalCombatState,
    result: AttackResult,
    body_part: BodyPart,
) -> Option<(f32, f32)> {
    match result {
        AttackResult::ToAttacker { balance_damage, .. } => {
            attacker_state.imbalance += balance_damage.max(0.0);
            None
        }
        AttackResult::ToDefender { balance_damage, .. } => {
            defender_state.imbalance += balance_damage.max(0.0);
            let damage = health_damage_from_attack(result, body_part);
            let applied = apply_clamped_limb_damage(defender_limbs.health_mut(body_part), damage);
            let (applied_cut, applied_blunt) = apportion_attack_health_damage(result, applied);
            defender_state.blood_loss_fraction = (defender_state.blood_loss_fraction
                + blood_loss_from_applied_health_damage(body_part, applied_cut, applied_blunt))
            .clamp(0.0, 1.0);
            if applied > 0.0 && applied_cut + applied_blunt > 0.0 {
                Some((applied_cut, applied_blunt))
            } else {
                None
            }
        }
    }
}
