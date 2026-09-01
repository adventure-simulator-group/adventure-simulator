use adventuresim_core::equipment::LayeredArmor;

use crate::inventory::{ArmorItem, ArmorLayerContact, InventoryView, body_part_index};

impl InventoryView<'_, '_, '_> {
    pub fn armor_layer_chain(
        &self,
        part: adventuresim_core::body::BodyPart,
        sample: f32,
    ) -> Vec<ArmorLayerContact> {
        let index = body_part_index(part);
        let mut layers = self
            .iter()
            .filter_map(|item| {
                let armor = item
                    .armor
                    .filter(|armor| armor.covered_parts[index] && armor.coverage > 0.0)?;
                Some((item, armor))
            })
            .collect::<Vec<_>>();
        layers.sort_by_key(|(_, armor)| std::cmp::Reverse(armor.layer_order));
        let selected = layers.iter().position(|(_, armor)| {
            armor.coverage_spans[index]
                .expect("covered armor part has authored surface geometry")
                .contains(sample)
        });
        layers
            .into_iter()
            .enumerate()
            .map(|(layer_index, (item, armor))| {
                let geometry = armor.coverage_geometry[index]
                    .expect("covered armor part retains authored geometry");
                ArmorLayerContact {
                    item_id: item.properties.id.clone(),
                    inventory_item_id: item.inventory_item_id.map(|id| id.0),
                    material: armor.material,
                    geometry,
                    intersected: geometry.span.contains(sample),
                    selected: selected == Some(layer_index),
                    surface: adventuresim_core::equipment::ArmorSurface {
                        inventory_item_id: item.inventory_item_id.map(|id| id.0),
                        material: Some(armor.material),
                        resistance: armor.resistance,
                        padding: armor.padding,
                        flexibility: armor.flexibility,
                    },
                }
            })
            .collect()
    }
}

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
