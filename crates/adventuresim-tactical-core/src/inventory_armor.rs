use adventuresim_core::equipment::LayeredArmor;

use crate::inventory::ArmorItem;

pub(super) fn fold_armor_layers<'a>(
    index: usize,
    armor: impl IntoIterator<Item = &'a ArmorItem>,
) -> LayeredArmor {
    let mut result = LayeredArmor {
        range_of_motion: 1.0,
        ..Default::default()
    };
    let mut weighted_flexibility = 0.0;
    for armor in armor.into_iter().filter(|armor| armor.covered_parts[index]) {
        result.coverage = 1.0 - (1.0 - result.coverage) * (1.0 - armor.coverage.clamp(0.0, 1.0));
        let resistance = armor.resistance.max(0.0);
        result.resistance += resistance;
        result.padding += armor.padding.max(0.0);
        weighted_flexibility += armor.flexibility.clamp(0.0, 1.0) * resistance;
        result.range_of_motion = result
            .range_of_motion
            .min(armor.range_of_motion.clamp(0.0, 1.0));
    }
    result.flexibility = if result.resistance > f32::EPSILON {
        weighted_flexibility / result.resistance
    } else {
        0.0
    };
    result
}
