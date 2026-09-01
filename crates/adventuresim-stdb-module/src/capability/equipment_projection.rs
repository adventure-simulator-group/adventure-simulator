use super::*;

pub(super) fn combat_weapon(
    item: &Item,
    instance: Option<adventuresim_core::equipment::ParametricWeaponCombatGeometry>,
) -> CombatWeapon {
    let definition = adventuresim_core::item_catalog::definition(&item.id).unwrap_or_else(|| {
        panic!(
            "equipped weapon {} is absent from the authored catalog",
            item.id
        )
    });
    let equipment = definition.equipment.as_ref().unwrap_or_else(|| {
        panic!(
            "equipped weapon {} has no authored equipment geometry",
            item.id
        )
    });
    let grip_to_tip_m = instance.map_or(equipment.physical.grip_to_tip_m, |value| {
        value.grip_to_tip_m
    });
    let [striking_width_m, total_length_m, striking_depth_m] = equipment.physical.dimensions_m;
    let striking_head_length_m = instance.map_or(striking_width_m.max(striking_depth_m), |value| {
        value.striking_head_length_m
    });
    let moment_of_inertia_kg_m2 = instance.map_or(item.moment_of_inertia_kg_m2, |value| {
        value.moment_of_inertia_kg_m2
    });
    let weight = instance.map_or(item.weight, |value| value.mass_kg);
    CombatWeapon {
        skills: item.weapon_skills,
        melee: item.melee,
        ranged: item.ranged,
        blunt: item.blunt,
        slash: item.slash,
        pierce: item.pierce,
        accuracy: item.accuracy,
        swing_precision: item.swing_precision,
        stab_precision: item.stab_precision,
        preferred_melee_style: item.preferred_melee_style,
        weight,
        moment_of_inertia_kg_m2,
        penetration: item.penetration,
        melee_reach: melee_reach(item, instance),
        grip_to_tip_m,
        total_length_m: instance.map_or(total_length_m, |value| value.total_length_m),
        striking_head_length_m,
        distal_headed: adventuresim_core::combat::has_distal_striking_surface(
            grip_to_tip_m,
            striking_head_length_m,
            equipment.material,
            equipment.striking_material,
        ),
        body_material: equipment.material,
        striking_material: equipment.striking_material,
        ranged_range: if item.ranged { item.reach } else { 0.0 },
        attack_interval_seconds: weapon_attack_interval(item, moment_of_inertia_kg_m2),
        precise: item.precise,
        balance: instance.map_or(item.balance, |value| value.balance),
        ranged_force_joules: 40.0 * weight.max(0.5),
    }
}

fn melee_reach(
    item: &Item,
    instance: Option<adventuresim_core::equipment::ParametricWeaponCombatGeometry>,
) -> f32 {
    if item.melee {
        instance.map_or(item.reach, |value| value.melee_reach_m())
    } else {
        0.0
    }
}

fn weapon_attack_interval(item: &Item, moment_of_inertia_kg_m2: f32) -> f32 {
    if item.melee {
        let timing = adventuresim_core::equipment::melee_attack_timing(
            item.preferred_melee_style,
            moment_of_inertia_kg_m2,
            false,
        );
        timing.preparation_secs + timing.recovery_secs
    } else {
        (0.4 + item.weight.max(0.1) * 0.15 + 0.45).clamp(0.35, 3.0)
    }
}
