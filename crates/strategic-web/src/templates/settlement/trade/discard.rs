//! Personal discard staging and ingredient-preparation edge actions.

use super::*;
use super::{equipment::*, inventory::*};

/// The active character's inventory with a staged discard list.
#[expect(
    clippy::too_many_arguments,
    reason = "the discard page boundary composes independent custody, selection, and encumbrance projections"
)]
pub fn party_discard_page(
    location: &LocationView,
    active_character: &CharacterView,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::CatalogItemView],
    food_lots: &[FoodLot],
    preparation_plans: &[BackendIngredientPreparationPlan],
    party_members: &[CharacterView],
    equip: Option<&CharacterEquipmentGraph>,
    encumbrance: EncumbranceSummary,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Discard", html! {
                p class="text-muted small-copy" data-discard-empty { "Stage carried items here before discarding them." }
                div data-discard-table hidden {
                    (trade_inventory_table("discard-left", InventoryColumnSet::All, true, false, false, html! {}))
                }
            }))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(active_character), &location.base_path(), Some(active_character.id)))
            (visual_stage(VisualStageKind::Character, &active_character.name, "Your carried equipment and supplies"))
            (settlement_chat_area(&active_character.name, Some(active_character)))
            form id="inventory-discard" class="party-offer"
                action=(format!("{}/party/{}/inventory/discard", location.base_path(), active_character.id))
                method="post" hidden role="dialog" aria-modal="true" aria-label="Confirm discarded items" tabindex="-1" {
                span class="party-offer-summary" data-discard-confirmation { "Discard the staged items?" }
                button type="button" class="party-offer-cancel" data-cancel-trade="discard" { "Cancel" }
                button type="submit" disabled { "Discard" }
            }
        }
        aside class="right-sidebar" {
            (discard_inventory_rail(active_character, inventory, items, food_lots, preparation_plans, &format!("{}/party/{}/inventory", location.base_path(), active_character.id), equip, encumbrance))
        }
    };
    location.render_layout("Inventory", content, Some(&active_character.name))
}
#[expect(
    clippy::too_many_arguments,
    reason = "the discard rail combines independent custody, selection, and encumbrance projections"
)]
pub(super) fn discard_inventory_rail(
    character: &CharacterView,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::CatalogItemView],
    food_lots: &[FoodLot],
    preparation_plans: &[BackendIngredientPreparationPlan],
    return_to: &str,
    equip: Option<&CharacterEquipmentGraph>,
    encumbrance: EncumbranceSummary,
) -> Markup {
    let title = format!("{}'s inventory", character.name);
    html! {
        (sidebar_section(&title, html! {
            (encumbrance_inventory_rail(html! {
                @for plan in preparation_plans.iter().filter(|plan| plan.actor_character_id == character.id && plan.inventory_scope == "personal") {
                    (ingredient_preparation_submission_form(plan, return_to))
                }
                @if inventory.is_empty() {
                    p class="text-muted small-copy" { "No items carried." }
                } @else {
                    (trade_inventory_table("discard-right", InventoryColumnSet::All, true, true, false, html! {
                        @for item in inventory {
                            @let is_equipped = equip.is_some_and(|equip| equip.contains(item.id));
                            @let definition = items.iter().find(|definition| definition.id == item.item_id);
                            @let food_lot = food_lots.iter().find(|lot| lot.inventory_item_id == Some(item.id));
                            @let display_name = food_lot.map_or_else(|| item_display_name(&item.item_id), |lot| lot.display_name.clone());
                            @let item_name = item_display_name(&item.item_id);
                            @let cut_plan = preparation_plans.iter().find(|plan| plan.actor_character_id == character.id && plan.inventory_scope == "personal" && plan.inventory_item_id == item.id && plan.action == IngredientPreparationAction::Cut);
                            @let grind_plan = preparation_plans.iter().find(|plan| plan.actor_character_id == character.id && plan.inventory_scope == "personal" && plan.inventory_item_id == item.id && plan.action == IngredientPreparationAction::Grind);
                            tr class="trade-inventory-row trade-row-player" data-discard-source=(item.id) data-personal-inventory-id=(item.id) data-item-key=(&item.item_id) {
                                td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                                td class="inventory-item-name" {
                                    (item_name_with_food_lot(&item.item_id, &display_name, definition, food_lot))
                                    span class="inventory-row-actions" {
                                        @if let Some(plan) = cut_plan {
                                            button type="submit" form=(ingredient_preparation_form_id(plan)) class="btn btn-secondary btn-small" aria-label=(format!("Cut {item_name}")) data-strategic-tooltip=(format!("Cut · {} min · precise edged weapon", plan.duration_minutes)) { "Cut" }
                                        }
                                        @if let Some(plan) = grind_plan {
                                            button type="submit" form=(ingredient_preparation_form_id(plan)) class="btn btn-secondary btn-small" aria-label=(format!("Grind {item_name}")) data-strategic-tooltip=(format!("Grind · {} min", plan.duration_minutes)) { "Grind" }
                                        }
                                        @if is_equipped {
                                            (disabled_transfer_button("left", "Equipped items cannot be discarded"))
                                        } @else {
                                            button type="button" class="trade-transfer trade-transfer-left"
                                            data-discard-item=(item.id) data-count=(item.quantity)
                                            data-dynamic-transfer data-default-transfer-mode="one" data-transfer-mode="one"
                                            data-label-one=(format!("Discard one {item_name}"))
                                            data-label-target=(format!("Discard {item_name} down to target"))
                                            data-label-all=(format!("Discard all {item_name}"))
                                            aria-label=(format!("Discard {item_name}"))
                                            title=(format!("Discard one {item_name}")) { (transfer_glyph(1)) }
                                        }
                                    }
                                }
                                td class="inventory-count" { (item.quantity) }
                                td class="inventory-equipped" { (equipment_control(item, definition, is_equipped, true, equip)) }
                                td class="inventory-weight" { (item_weight(definition)) }
                                td class="inventory-gold" { (item_value(definition)) }
                            }
                        }
                    }))
                }
            }, html! {}, encumbrance))
        }))
    }
}

pub(super) fn ingredient_preparation_form_id(plan: &BackendIngredientPreparationPlan) -> String {
    let action = match plan.action {
        IngredientPreparationAction::Cut => "cut",
        IngredientPreparationAction::Grind => "grind",
    };
    format!(
        "ingredient-preparation-{}-{}-{}-{action}",
        plan.actor_character_id, plan.inventory_scope, plan.inventory_item_id
    )
}

pub(super) fn ingredient_preparation_submission_form(
    plan: &BackendIngredientPreparationPlan,
    return_to: &str,
) -> Markup {
    let action = match plan.action {
        IngredientPreparationAction::Cut => "cut",
        IngredientPreparationAction::Grind => "grind",
    };
    html! {
        form id=(ingredient_preparation_form_id(plan)) method="post" action="/api/inventory/prepare"
            class="inventory-edge-action" hidden {
            input type="hidden" name="inventory_item_id" value=(plan.inventory_item_id);
            input type="hidden" name="inventory_scope" value=(&plan.inventory_scope);
            input type="hidden" name="food_lot_id" value=(plan.food_lot_id);
            input type="hidden" name="material_object_id" value=(plan.material_object_id);
            input type="hidden" name="request_id" value=(&plan.request_id);
            input type="hidden" name="expected_revision" value=(plan.expected_revision);
            input type="hidden" name="attempt_generation" value=(plan.attempt_generation);
            input type="hidden" name="preparation_action" value=(action);
            input type="hidden" name="return_to" value=(return_to);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spacetimedb::*;
    use adventuresim_core::equipment::EncumbranceSummary;

    #[test]
    fn ingredient_preparation_is_an_accessible_inner_edge_action_not_a_skill_modal() {
        let source = concat!(include_str!("discard.rs"), include_str!("party_pool.rs"));
        assert!(source.contains("/api/inventory/prepare"));
        assert!(source.contains("name=\"preparation_action\""));
        assert!(source.contains("name=\"material_object_id\""));
        assert!(source.contains("name=\"request_id\""));
        assert!(source.contains("name=\"expected_revision\""));
        assert!(source.contains("name=\"attempt_generation\""));
        assert!(source.contains("name=\"return_to\""));
        assert!(source.contains("plan.inventory_scope == \"personal\""));
        assert!(source.contains("plan.inventory_scope == \"party\""));
        assert!(source.contains("party_pool_page"));
        assert!(!source.contains("name=\"action\" value=\"cut\""));
        assert!(source.contains("aria-label=(format!(\"Cut {item_name}\"))"));
        assert!(source.contains("plan.duration_minutes"));
        let removed_menu_marker = ["data-herbalism", "-activity"].concat();
        assert!(!source.contains(&removed_menu_marker));
    }

    #[test]
    fn ingredient_preparation_forms_are_detached_from_tables_with_complete_payloads() {
        let character = CharacterView {
            id: 7,
            name: "Cook".into(),
            xp: 0,
            level: 1,
            current_settlement_id: Some("test".into()),
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive: true,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        };
        let inventory = [InventoryItem {
            id: 11,
            character_id: character.id,
            item_id: "poppy".into(),
            quantity: 1,
        }];
        let plan = BackendIngredientPreparationPlan {
            actor_character_id: character.id,
            inventory_scope: "personal".into(),
            inventory_item_id: 11,
            food_lot_id: 13,
            material_object_id: 17,
            request_id: "request-token".into(),
            expected_revision: 19,
            attempt_generation: 23,
            action: IngredientPreparationAction::Cut,
            duration_minutes: 5,
            next_display_name: "Cut poppy".into(),
        };
        let form_id = ingredient_preparation_form_id(&plan);
        let rendered = discard_inventory_rail(
            &character,
            &inventory,
            &[],
            &[],
            std::slice::from_ref(&plan),
            "/locations/settlement/test/party/7/inventory",
            None,
            EncumbranceSummary::default(),
        )
        .into_string();

        let form_start = rendered
            .find(&format!("<form id=\"{form_id}\""))
            .expect("detached preparation form");
        let form_end = form_start
            + rendered[form_start..]
                .find("</form>")
                .expect("closed preparation form");
        let table_start = rendered.find("<table").expect("inventory table");
        assert!(
            form_end < table_start,
            "preparation form must precede the table"
        );
        let form = &rendered[form_start..form_end];
        for field in [
            "inventory_item_id",
            "inventory_scope",
            "food_lot_id",
            "material_object_id",
            "request_id",
            "expected_revision",
            "attempt_generation",
            "preparation_action",
            "return_to",
        ] {
            assert!(
                form.contains(&format!("name=\"{field}\"")),
                "missing {field}"
            );
        }
        assert!(form.contains("value=\"/locations/settlement/test/party/7/inventory\""));
        assert!(rendered.contains(&format!("type=\"submit\" form=\"{form_id}\"")));
        let tbody = rendered
            .split("<tbody>")
            .nth(1)
            .and_then(|tail| tail.split("</tbody>").next())
            .expect("inventory tbody");
        assert!(
            !tbody.contains("<form"),
            "forms inside tables are parser-unsafe"
        );
    }
}
