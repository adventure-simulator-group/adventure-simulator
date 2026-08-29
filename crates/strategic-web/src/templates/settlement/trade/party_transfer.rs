//! Direct party-member inventory transfer presentation.

use super::*;
use super::{equipment::*, inventory::*};

/// Party inventory comparison.
#[expect(
    clippy::too_many_arguments,
    reason = "the inventory page boundary composes independent custody and encumbrance projections"
)]
pub fn party_inventory_page(
    location: &LocationView,
    selected: &CharacterView,
    selected_inventory: &[InventoryItem],
    active_character: &CharacterView,
    active_inventory: &[InventoryItem],
    items: &[crate::spacetimedb::CatalogItemView],
    food_lots: &[FoodLot],
    party_members: &[CharacterView],
    selected_equip: Option<&CharacterEquipmentGraph>,
    active_equip: Option<&CharacterEquipmentGraph>,
    selected_targets: &[InventoryQuantityTarget],
    active_targets: &[InventoryQuantityTarget],
    selected_encumbrance: EncumbranceSummary,
    active_encumbrance: EncumbranceSummary,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (party_trade_inventory_rail(selected, selected_inventory, items, food_lots, active_character.id, "right", selected_equip, active_targets, selected_encumbrance, false))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(active_character), &location.base_path(), Some(selected.id)))
            (visual_stage(VisualStageKind::Character, &selected.name, "Party member and trading companion"))
            (player_chat_area(location, selected, active_character))
            form id="party-offer" class="party-offer" action=(format!("{}/party/{}/inventory/offer", location.base_path(), selected.id)) method="post" hidden
                role="dialog" aria-modal="true" aria-label="Confirm party item offer" tabindex="-1" {
                span class="party-offer-summary" { "Review and send the staged item offer." }
                button type="button" class="party-offer-cancel" data-cancel-trade="party" { "Cancel" }
                button type="submit" disabled { "Offer" }
            }
        }
        aside class="right-sidebar" {
            (party_trade_inventory_rail(active_character, active_inventory, items, food_lots, selected.id, "left", active_equip, selected_targets, active_encumbrance, true))
        }
    };
    location.render_layout("Party", content, Some(&active_character.name))
}
#[expect(
    clippy::too_many_arguments,
    reason = "the trade rail combines independent custody, selection, and encumbrance projections"
)]
pub(super) fn party_trade_inventory_rail(
    character: &CharacterView,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::CatalogItemView],
    food_lots: &[FoodLot],
    recipient_id: u64,
    direction: &str,
    equip: Option<&CharacterEquipmentGraph>,
    recipient_targets: &[InventoryQuantityTarget],
    encumbrance: EncumbranceSummary,
    medication_is_self: bool,
) -> Markup {
    let title = format!("{}'s inventory", character.name);
    html! {
        (sidebar_section(&title, html! {
            (encumbrance_inventory_rail(html! {
                @if inventory.is_empty() {
                    p class="text-muted small-copy" { "No items carried." }
                } @else {
                    (trade_inventory_table(if direction == "left" { "party-transfer-right" } else { "party-transfer-left" }, InventoryColumnSet::All, true, true, false, html! {
                        @for item in inventory {
                            @let is_equipped = equip.is_some_and(|equip| equip.contains(item.id));
                            @let definition = items.iter().find(|definition| definition.id == item.item_id);
                            @let food_lot = food_lots.iter().find(|lot| lot.inventory_item_id == Some(item.id));
                            @let display_name = food_lot.map_or_else(|| item_display_name(&item.item_id), |lot| lot.display_name.clone());
                            @let target = target_quantity(recipient_targets, &item.item_id);
                            @let item_name = item_display_name(&item.item_id);
                                tr class=(if direction == "left" { "trade-inventory-row trade-row-player" } else { "trade-inventory-row trade-row-merchant" }) data-item-key=(&item.item_id) data-personal-inventory-id=(item.id) {
                                    td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                                    td class="inventory-item-name" {
                                        (item_name_with_food_lot(&item.item_id, &display_name, definition, food_lot))
                                        span class="inventory-row-actions" {
                                            @if is_equipped {
                                                (disabled_transfer_button(direction, "Equipped items cannot be transferred"))
                                            } @else {
                                                button type="button" class=(format!("trade-transfer trade-transfer-{direction} party-draft-transfer")) data-dynamic-transfer data-default-transfer-mode="one" data-from=(character.id) data-to=(recipient_id) data-item=(item.id) data-key=(&item.item_id) data-count=(item.quantity) data-target=(target) data-transfer-mode="one" data-label-one=(format!("Transfer one {item_name}")) data-label-target=(format!("Transfer {item_name} to target")) data-label-all=(format!("Transfer all {item_name}")) aria-label=(format!("Transfer one {item_name}")) title=(format!("Transfer one {item_name}")) { (transfer_glyph(1)) }
                                            }
                                        }
                                    }
                                    td class="inventory-count" { (item.quantity) }
                                    td class="inventory-equipped" { (equipment_control(item, definition, is_equipped, medication_is_self, equip)) }
                                    td class="inventory-weight" { (item_weight(definition)) }
                                    td class="inventory-gold" { (item_value(definition)) }
                                }
                            }
                    }))
                }
            }, inventory_footer_controls(if direction == "left" { "party-left" } else { "party-right" }, "Transfer to targets", "Transfer everything"), encumbrance))
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_transfer_button_keeps_a_disabled_action_slot() {
        let rendered =
            disabled_transfer_button("left", "Equipped items cannot be transferred").into_string();

        assert!(rendered.contains("trade-transfer trade-transfer-left"));
        assert!(rendered.contains("Equipped items cannot be transferred"));
        assert!(rendered.contains("disabled"));
        assert!(rendered.contains("inventory-transfer-glyph"));
    }
}
