use super::*;

impl InventoryView<'_, '_, '_> {
    pub fn item_at_slot(&self, slot: EquipSlot) -> Option<(&str, Option<u64>)> {
        self.iter().find_map(|item| {
            (item.slot == Some(&slot)).then_some((
                item.properties.id.as_str(),
                item.inventory_item_id.map(|id| id.0),
            ))
        })
    }
}
