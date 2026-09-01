use super::*;

impl InventoryView<'_, '_, '_> {
    pub(crate) fn iter(&self) -> impl Iterator<Item = ItemQueryItem<'_, '_>> + use<'_> {
        let items = self
            .q_inventory
            .get(self.entity)
            .into_iter()
            .flat_map(|inventory| inventory.iter());
        self.q_item.iter_many(items)
    }

    pub fn item_at_slot(&self, slot: EquipSlot) -> Option<(&str, Option<u64>)> {
        self.iter().find_map(|item| {
            (item.slot == Some(&slot)).then_some((
                item.properties.id.as_str(),
                item.inventory_item_id.map(|id| id.0),
            ))
        })
    }
}
