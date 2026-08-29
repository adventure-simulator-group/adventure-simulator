//! Shared party-pool inventory transfer presentation.

use super::*;
use super::{discard::*, equipment::*, inventory::*};

/// Two-sided transfer view for the equally owned party chest.
#[expect(
    clippy::too_many_arguments,
    reason = "the party pool page boundary composes independent personal and shared custody projections"
)]
pub fn party_pool_page(
    location: &LocationView,
    character: &CharacterView,
    inventory: &[InventoryItem],
    pooled: &[PartyInventoryItem],
    stake: u64,
    items: &[crate::spacetimedb::CatalogItemView],
    food_lots: &[FoodLot],
    preparation_plans: &[BackendIngredientPreparationPlan],
    party_members: &[CharacterView],
    equip: Option<&CharacterEquipmentGraph>,
    personal_targets: &[InventoryQuantityTarget],
    party_targets: &[InventoryQuantityTarget],
    party_encumbrance: EncumbranceSummary,
    personal_encumbrance: EncumbranceSummary,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Party inventory", html! {
                (encumbrance_inventory_rail(html! {
                    @for plan in preparation_plans.iter().filter(|plan| plan.actor_character_id == character.id && plan.inventory_scope == "party") {
                        (ingredient_preparation_submission_form(plan, &format!("{}/party-inventory", location.base_path())))
                    }
                    div class="party-stake-summary" tabindex="0"
                        data-strategic-tooltip="Withdrawals use your stake; personal coin covers an indivisible item's shortfall."
                        aria-label=(format!("Your available stake: {stake} coin. Withdrawals use your stake; personal coin covers an indivisible item's shortfall.")) {
                        span { "Your available stake" }
                        strong { (stake) " coin" }
                    }
                    (trade_inventory_table("party-pool-left", InventoryColumnSet::All, true, false, false, html! {
                        @for item in pooled {
                            @let definition = items.iter().find(|definition| definition.id == item.item_id);
                            @let food_lot = food_lots.iter().find(|lot| lot.party_inventory_item_id == Some(item.id));
                            @let food_display_name = food_lot.map_or_else(|| item_display_name(&item.item_id), |lot| lot.display_name.clone());
                            @let value = definition.and_then(|definition| definition.base_value).unwrap_or(0) as u64;
                            @let target = target_quantity(personal_targets, &item.item_id);
                            @let current = inventory.iter().find(|personal| personal.item_id == item.item_id).map_or(0, |personal| personal.quantity);
                            @let item_name = item_display_name(&item.item_id);
                            @let cut_plan = preparation_plans.iter().find(|plan| plan.actor_character_id == character.id && plan.inventory_scope == "party" && plan.inventory_item_id == item.id && plan.action == IngredientPreparationAction::Cut);
                            @let grind_plan = preparation_plans.iter().find(|plan| plan.actor_character_id == character.id && plan.inventory_scope == "party" && plan.inventory_item_id == item.id && plan.action == IngredientPreparationAction::Grind);
                            tr class="trade-inventory-row" data-party-inventory-id=(item.id) {
                                td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                                td class="inventory-item-name" {
                                    (item_name_with_food_lot(&item.item_id, &food_display_name, definition, food_lot))
                                span class="inventory-row-actions" {
                                    @for (plan, action, label) in [(cut_plan, "cut", "Cut"), (grind_plan, "grind", "Grind")] {
                                        @if let Some(plan) = plan {
                                            button type="submit" form=(ingredient_preparation_form_id(plan)) class="btn btn-secondary btn-small" aria-label=(format!("{label} {item_name}")) data-preparation-action=(action) data-strategic-tooltip=(format!("{label} · {} min", plan.duration_minutes)) { (label) }
                                        }
                                    }
                                    button type="button" class="trade-transfer trade-transfer-right" data-dynamic-transfer data-default-transfer-mode="one" data-pool-stage=(item.id) data-pool-direction="withdraw" data-transfer-mode="one" data-count=(item.quantity) data-current=(current) data-target=(target) data-label-one=(format!("Withdraw one {item_name}")) data-label-target=(format!("Withdraw {item_name} to target")) data-label-all=(format!("Withdraw all {item_name}")) title=(if value > stake { format!("Withdraw one {item_name}; {} personal coin required", value - stake) } else { format!("Withdraw one {item_name} using your stake") }) aria-label=(format!("Withdraw one {item_name}")) { (transfer_glyph(1)) }
                                }
                                }
                                td class="inventory-count" { (quantity_target_control(item.quantity, target_quantity(party_targets, &item.item_id), &item.item_id, true)) }
                                td class="inventory-weight" { (item_weight(definition)) }
                                td class="inventory-gold" { (item_value(definition)) }
                            }
                        }
                    }))
                }, inventory_footer_controls("withdraw", "Withdraw to personal targets", "Withdraw everything"), party_encumbrance))
            }))
        }
        main class="center-content settlement-main" {
            (party_portrait_overlay(party_members, Some(character), &location.base_path(), None))
            (visual_stage(VisualStageKind::Chest, "Party chest", "Shared supplies and each member's stake"))
            (settlement_chat_area("Party inventory", Some(character)))
        }
        aside class="right-sidebar" {
            (sidebar_section(&format!("{}'s inventory", character.name), html! {
                (encumbrance_inventory_rail(html! {
                    (trade_inventory_table("party-pool-right", InventoryColumnSet::All, true, true, false, html! {
                        @for item in inventory {
                            @let definition = items.iter().find(|definition| definition.id == item.item_id);
                            @let food_lot = food_lots.iter().find(|lot| lot.inventory_item_id == Some(item.id));
                            @let food_display_name = food_lot.map_or_else(|| item_display_name(&item.item_id), |lot| lot.display_name.clone());
                            @let equipped = equip.is_some_and(|equip| equip.contains(item.id));
                            @let target = target_quantity(party_targets, &item.item_id);
                            @let current = pooled.iter().find(|pooled| pooled.item_id == item.item_id).map_or(0, |pooled| pooled.quantity);
                            @let item_name = item_display_name(&item.item_id);
                            tr class="trade-inventory-row" data-personal-inventory-id=(item.id) {
                                td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                                td class="inventory-item-name" {
                                    (item_name_with_food_lot(&item.item_id, &food_display_name, definition, food_lot))
                                    span class="inventory-row-actions" {
                                        @if equipped {
                                            (disabled_transfer_button("left", "Equipped items cannot be deposited"))
                                        } @else {
                                            button type="button" class="trade-transfer trade-transfer-left" data-dynamic-transfer data-default-transfer-mode="one" data-pool-stage=(item.id) data-pool-direction="deposit" data-transfer-mode="one" data-count=(item.quantity) data-current=(current) data-target=(target) data-label-one=(format!("Deposit one {item_name}")) data-label-target=(format!("Deposit {item_name} to target")) data-label-all=(format!("Deposit all {item_name}")) aria-label=(format!("Deposit one {item_name} at its objective coin value")) data-strategic-tooltip=(format!("Deposit one {item_name} at its objective coin value")) { (transfer_glyph(1)) }
                                        }
                                    }
                                }
                                td class="inventory-count" { (quantity_target_control(item.quantity, target_quantity(personal_targets, &item.item_id), &item.item_id, false)) }
                                td class="inventory-equipped" { (equipment_control(item, definition, equipped, true, equip)) }
                                td class="inventory-weight" { (item_weight(definition)) }
                                td class="inventory-gold" { (item_value(definition)) }
                            }
                        }
                    }))
                }, inventory_footer_controls("deposit", "Deposit to party targets", "Deposit everything"), personal_encumbrance))
            }))
        }
        form method="post" action=(format!("{}/party-inventory/deposit", location.base_path())) id="pool-transfer-offer" class="party-offer" hidden role="dialog" aria-modal="true" aria-label="Confirm party inventory transfer" tabindex="-1" { span class="party-offer-summary" { "Apply the staged party inventory transfer?" } button type="button" data-cancel-pool class="party-offer-cancel" { "Cancel" } button type="submit" disabled { "Offer" } }
    };
    location.render_layout("Party inventory", content, Some(&character.name))
}
