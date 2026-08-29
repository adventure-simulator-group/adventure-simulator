//! Shared inventory rows, quantities, encumbrance, names, and footer controls.

use super::equipment::{item_kind_tag, slot_wire_label};
use super::*;

pub(in crate::templates::settlement) fn item_weight(
    item: Option<&crate::spacetimedb::CatalogItemView>,
) -> String {
    item.map_or_else(|| "—".to_owned(), |item| weight_display(item.weight))
}
pub(in crate::templates::settlement) fn encumbrance_inventory_rail(
    content: Markup,
    footer_controls: Markup,
    summary: EncumbranceSummary,
) -> Markup {
    html! {
        div class="encumbrance-inventory-rail" {
            div class="encumbrance-inventory-scroll" { (content) }
            (footer_controls)
            (encumbrance_meter(summary))
        }
    }
}

pub(super) fn encumbrance_meter(summary: EncumbranceSummary) -> Markup {
    let penalty_percent = summary.penalty_fraction() * 100.0;
    let accessible_text = format!(
        "Weight {:.1} / {:.1} kilograms; Penalty -{penalty_percent:.1}%",
        summary.burden_kg, summary.capacity_kg
    );
    html! {
        div class="encumbrance" {
            div class="encumbrance-visual" {
                div class="encumbrance-meter"
                    tabindex="0"
                    data-strategic-tooltip=(&accessible_text)
                    role="meter"
                    aria-label="Encumbrance"
                    aria-valuemin="0"
                    aria-valuemax="100"
                    aria-valuenow=(format!("{penalty_percent:.1}"))
                    aria-valuetext=(accessible_text) {
                    span class="encumbrance-marker"
                        style=(format!("--encumbrance-position: {penalty_percent:.4}%")) {}
                }
            }
        }
    }
}
pub(in crate::templates::settlement) fn item_value(
    item: Option<&crate::spacetimedb::CatalogItemView>,
) -> String {
    item.and_then(|item| item.base_value)
        .map_or_else(|| "—".to_owned(), |value| value.to_string())
}

pub(in crate::templates) fn item_name_with_quality(
    item_id: &str,
    definition: Option<&crate::spacetimedb::CatalogItemView>,
) -> Markup {
    let currency_name = adventuresim_core::strategic_currency::currency_name(item_id);
    let edit_url = item_source_edit_url(item_id);
    if let Some(currency_name) = currency_name {
        html! {
            span class="inventory-item-label" data-item-name="Coin"
                data-item-kind="currency" data-currency-name=(currency_name)
                data-item-edit-url=[edit_url] { "Coin" }
        }
    } else {
        let display_name = item_display_name(item_id);
        item_name_with_display(item_id, &display_name, definition)
    }
}

pub(in crate::templates::settlement) fn item_name_with_display(
    item_id: &str,
    display_name: &str,
    definition: Option<&crate::spacetimedb::CatalogItemView>,
) -> Markup {
    item_name_with_display_quality(item_id, display_name, definition, None)
}

pub(in crate::templates) fn item_name_with_food_lot(
    item_id: &str,
    display_name: &str,
    definition: Option<&crate::spacetimedb::CatalogItemView>,
    food_lot: Option<&FoodLot>,
) -> Markup {
    item_name_with_display_quality(
        item_id,
        display_name,
        definition,
        food_lot.map(|lot| lot.quality.clamp(1, 5)),
    )
}

pub(super) fn item_name_with_display_quality(
    item_id: &str,
    display_name: &str,
    definition: Option<&crate::spacetimedb::CatalogItemView>,
    quality_override: Option<u8>,
) -> Markup {
    let edit_url = item_source_edit_url(item_id);
    let alcohol_group = definition
        .filter(|item| item.alcohol_serving_ml > 0)
        .map(|_| "alcohol");
    let food_quality =
        quality_override.is_some() || adventuresim_core::food::definition(item_id).is_some();
    let book_quality = adventuresim_core::item_catalog::definition(item_id)
        .is_some_and(|item| item.capabilities.book.is_some());
    let quality = quality_override.or_else(|| {
        definition
            .filter(|item| {
                matches!(
                    item.kind,
                    crate::spacetimedb::CatalogItemKind::Weapon
                        | crate::spacetimedb::CatalogItemKind::Armor
                        | crate::spacetimedb::CatalogItemKind::Shield
                        | crate::spacetimedb::CatalogItemKind::Food
                ) || adventuresim_core::food::definition(item_id).is_some()
                    || book_quality
            })
            .map(|item| item.quality.clamp(1, 5))
    });
    let label = quality.map(|quality| {
        if food_quality || book_quality {
            format!("Quality {quality}")
        } else {
            match quality {
                1 => "Quality 1".to_string(),
                2 => "Quality 2".to_string(),
                3 => "Quality 3 — munition grade".to_string(),
                4 => "Quality 4 — knightly commission".to_string(),
                5 => "Quality 5 — royal or heroic commission".to_string(),
                _ => unreachable!(),
            }
        }
    });
    let damage_types = definition.map(|item| {
        [
            item.blunt.then_some("Blunt"),
            item.slash.then_some("Slash"),
            item.pierce.then_some("Pierce"),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ")
    });
    html! {
        span class=(quality.map_or_else(|| "inventory-item-label".to_string(), |quality| format!("inventory-item-label item-quality-{quality}"))) title=[label]
            data-item-name=(item_id)
            data-item-kind=[definition.map(|item| item_kind_tag(item.kind))]
            data-item-melee=[definition.map(|item| item.melee)]
            data-item-weapon-holder=[matches!(item_id, "scabbard" | "weapon_loop").then_some("true")]
            data-item-ranged=[definition.map(|item| item.ranged)]
            data-item-group=[alcohol_group]
            data-group-name=[alcohol_group.map(|_| "Alcohol")]
            data-food-lot=[adventuresim_core::food::definition(item_id).map(|_| "true")]
            data-container-capacity-ml=[definition.and_then(|item| (item.container_capacity_ml > 0).then_some(item.container_capacity_ml))]
            data-exterior-volume-ml=[definition.map(|item| item.exterior_volume_ml)]
            data-stat-accuracy=[definition.map(|item| weight_display(item.accuracy))]
            data-stat-swing-precision=[definition.map(|item| weight_display(item.swing_precision))]
            data-stat-stab-precision=[definition.map(|item| weight_display(item.stab_precision))]
            data-stat-reach=[definition.map(|item| weight_display(item.reach))]
            data-stat-penetration=[definition.map(|item| weight_display(item.penetration))]
            data-stat-damage=[damage_types]
            data-stat-block=[definition.map(|item| weight_display(item.block))]
            data-stat-coverage=[definition.map(|item| weight_display(item.coverage))]
            data-stat-resistance=[definition.map(|item| weight_display(item.resistance))]
            data-stat-padding=[definition.map(|item| weight_display(item.padding))]
            data-stat-flexibility=[definition.map(|item| weight_display(item.flexibility))]
            data-stat-range-of-motion=[definition.map(|item| weight_display(item.range_of_motion))]
            data-detail-slot=[definition.map(|item| slot_wire_label(item.slot))]
            data-detail-balance=[definition.map(|item| weight_display(item.balance))]
            data-item-edit-url=[edit_url]
            data-detail-mode=[definition.map(|item| match (item.melee, item.ranged, item.precise) { (true, true, true) => "Melee, ranged, precise", (true, true, false) => "Melee and ranged", (true, false, true) => "Melee, precise", (false, true, true) => "Ranged, precise", (true, false, false) => "Melee", (false, true, false) => "Ranged", (false, false, true) => "Precise", _ => "—" }.to_string())] {
            (display_name)
        }
    }
}

pub(super) fn weight_display(weight: f32) -> String {
    let display = format!("{weight:.2}");
    display
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

pub(in crate::templates::settlement) fn trade_inventory_table(
    namespace: &str,
    optional_columns: InventoryColumnSet,
    show_quantities: bool,
    show_equipped: bool,
    show_condition: bool,
    rows: Markup,
) -> Markup {
    InventoryBrowser {
        namespace,
        show_quantities,
        show_equipped,
        show_condition,
        optional_columns,
        rows,
    }
    .render()
}

pub(super) fn target_quantity(targets: &[InventoryQuantityTarget], item_id: &str) -> u32 {
    targets
        .iter()
        .find(|target| target.item_id == item_id)
        .map_or(0, |target| target.quantity)
}

pub(super) fn quantity_target_control(
    quantity: u32,
    target: u32,
    item_id: &str,
    party_scope: bool,
) -> Markup {
    let item_name = item_display_name(item_id);
    html! {
        span class="inventory-target-control" data-target-control data-quantity=(quantity) data-item-id=(item_id) data-party-scope=(party_scope) title=(format!("Carrying {quantity}; target {target}")) {
            span class="inventory-target-value" data-target-value role="button" tabindex="0"
                aria-label=(format!("Target quantity for {item_name}"))
                title=(format!("Click to edit the target quantity for {item_name}")) { (target) }
        }
    }
}

pub(in crate::templates) fn transfer_glyph(count: usize) -> Markup {
    html! { span class=(format!("inventory-transfer-glyph arrows-{count}")) aria-hidden="true" { @for _ in 0..count { i {} } } }
}

pub(super) fn disabled_transfer_button(direction: &str, explanation: &str) -> Markup {
    html! {
        button type="button" class=(format!("trade-transfer trade-transfer-{direction}")) disabled title=(explanation) aria-label=(explanation) { (transfer_glyph(1)) }
    }
}
pub(in crate::templates) fn inventory_footer_controls(
    action: &str,
    target_label: &str,
    all_label: &str,
) -> Markup {
    inventory_footer_controls_with_leading(None, action, target_label, all_label)
}

pub(super) fn inventory_footer_controls_with_leading(
    leading: Option<Markup>,
    action: &str,
    target_label: &str,
    all_label: &str,
) -> Markup {
    let grouped = leading.is_some();
    html! { div class=(if grouped { "inventory-footer-actions inventory-footer-actions-grouped" } else { "inventory-footer-actions" }) {
        @if let Some(leading) = leading { (leading) }
        button type="button" class="trade-transfer inventory-footer-transfer" data-dynamic-transfer data-default-transfer-mode="target" data-inventory-bulk=(action) data-transfer-mode="target" data-label-target=(target_label) data-label-all=(all_label) aria-label=(target_label) title=(target_label) { (transfer_glyph(2)) }
    } }
}

pub(super) fn currency_header(label: &str) -> Markup {
    game_icon(label, "coins")
}

// Kept for one-sided placeholder/service tables that are intentionally not
// inventory browsers.
pub(in crate::templates::settlement) fn trade_inventory_table_header(
    show_equipped: bool,
    condition_header: Option<Markup>,
) -> Markup {
    html! { thead { tr {
        (item_type_header())
        th scope="col" class="inventory-column-item" { "Item" }
        th scope="col" class="inventory-column-count" title="Quantity" { (game_icon("Quantity", "open-chest")) }
        @if show_equipped { th scope="col" class="inventory-column-equipped" title="Equipped" { (game_icon("Equipped", "check-mark")) } }
        @if let Some(condition_header) = condition_header { th scope="col" class="inventory-column-durability" { (condition_header) } }
        th scope="col" class="inventory-column-weight" title="Weight" { (game_icon("Weight", "weight")) }
        th scope="col" class="inventory-column-gold" title="Currency" { (currency_header("Currency")) }
    } } }
}

#[cfg(test)]
mod tests {
    use super::super::equipment::equipment_location_wire_label;
    use super::*;
    use crate::spacetimedb::*;
    use adventuresim_core::equipment::EncumbranceSummary;

    #[test]
    fn inventory_html_discriminants_are_fixed_vectors() {
        use crate::spacetimedb::{CatalogItemKind, Slot};

        assert_eq!(
            [
                CatalogItemKind::Simple,
                CatalogItemKind::Weapon,
                CatalogItemKind::Armor,
                CatalogItemKind::Shield,
                CatalogItemKind::Clothing,
                CatalogItemKind::Container,
                CatalogItemKind::Currency,
                CatalogItemKind::Ingredient,
                CatalogItemKind::Medication,
                CatalogItemKind::Food,
            ]
            .map(item_kind_tag),
            [
                "simple",
                "weapon",
                "armor",
                "shield",
                "clothing",
                "container",
                "currency",
                "ingredient",
                "medication",
                "food",
            ]
        );
        assert_eq!(
            [
                Slot::None,
                Slot::LeftHolding,
                Slot::RightHolding,
                Slot::LeftArm,
                Slot::RightArm,
                Slot::LeftLeg,
                Slot::RightLeg,
                Slot::Chest,
                Slot::Stomach,
                Slot::Head,
                Slot::AnyHolding,
                Slot::AnyArm,
                Slot::AnyLeg,
            ]
            .map(slot_wire_label),
            [
                "None",
                "LeftHolding",
                "RightHolding",
                "LeftArm",
                "RightArm",
                "LeftLeg",
                "RightLeg",
                "Chest",
                "Stomach",
                "Head",
                "AnyHolding",
                "AnyArm",
                "AnyLeg",
            ]
        );
        assert_eq!(
            equipment_location_wire_label(CoreEquipmentLocation::BackLeftPocket),
            "BackLeftPocket"
        );
    }

    #[test]
    fn holder_inventory_rows_request_per_instance_icons() {
        for id in ["scabbard", "weapon_loop"] {
            let markup = item_name_with_display(id, id, None).into_string();
            assert!(markup.contains("data-item-weapon-holder=\"true\""), "{id}");
        }
        let markup = item_name_with_display("belt", "belt", None).into_string();
        assert!(!markup.contains("data-item-weapon-holder"));
    }

    #[test]
    fn encumbrance_meter_formats_exact_text_and_accessible_linear_position() {
        let markup = encumbrance_meter(EncumbranceSummary::new(85.36, 150.0)).into_string();
        assert!(markup.contains("Weight 85.4 / 150.0 kilograms; Penalty -56.9%"));
        assert!(!markup.contains("encumbrance-values"));
        assert!(!markup.contains(">85.4 / 150.0 kg<"));
        assert!(!markup.contains(">-56.9%<"));
        assert!(
            markup.contains(
                "data-strategic-tooltip=\"Weight 85.4 / 150.0 kilograms; Penalty -56.9%\""
            )
        );
        assert!(markup.contains("tabindex=\"0\""));
        assert!(markup.contains("role=\"meter\""));
        assert!(markup.contains("aria-valuenow=\"56.9\""));
        assert!(markup.contains("--encumbrance-position: 56.9067%"));
    }

    #[test]
    fn overloaded_meter_keeps_burden_but_clamps_penalty_and_marker() {
        let markup = encumbrance_meter(EncumbranceSummary::new(185.4, 150.0)).into_string();
        assert!(markup.contains("Weight 185.4 / 150.0 kilograms; Penalty -100.0%"));
        assert!(!markup.contains(">185.4 / 150.0 kg<"));
        assert!(markup.contains("--encumbrance-position: 100.0000%"));
    }

    #[test]
    fn encumbrance_css_uses_a_linear_midpoint_gradient_and_contrast_marker() {
        let css = include_str!("../../../../static/css/strategic.css");
        assert!(css.contains("linear-gradient(90deg, #238b45 0%, #f4d03f 50%, #c62828 100%)"));
        assert!(css.contains(".encumbrance-marker"));
        assert!(css.contains("background: #fff"));
    }

    #[test]
    fn encumbrance_rail_scrolls_items_but_keeps_footer_and_meter_outside() {
        let markup = encumbrance_inventory_rail(
            maud::html! { table class="test-items" {} },
            maud::html! { button class="test-footer" {} },
            EncumbranceSummary::new(10.0, 100.0),
        )
        .into_string();
        assert!(markup.contains(
            "<div class=\"encumbrance-inventory-scroll\"><table class=\"test-items\"></table></div><button class=\"test-footer\"></button><div class=\"encumbrance\">"
        ));

        let css = include_str!("../../../../static/css/strategic.css");
        assert!(css.contains(".sidebar-section:has(> .encumbrance-inventory-rail)"));
        assert!(css.contains(".encumbrance-inventory-scroll"));
        assert!(css.contains("overflow-y: auto"));
        assert!(css.contains("padding-left: 3.25rem"));
        assert!(css.contains("padding-right: 1.75rem"));
        assert!(css.contains("container-type: inline-size"));
        assert!(css.contains("flex: 1 1 100%"));
        assert!(css.contains("width: 100%"));
        assert!(!markup.contains("encumbrance-values"));
        assert!(css.contains(".encumbrance-meter"));
        assert!(css.contains("width: 100%"));
        assert!(css.contains("@container (max-width: 12rem)"));
        assert!(css.contains("padding-inline: 0.2rem"));
        assert!(css.contains("@container (max-width: 10rem)"));
        assert!(css.contains("padding-inline: 0.1rem"));
        assert!(!css.contains(".encumbrance-values"));
    }

    #[test]
    fn collapsed_currency_label_hides_the_historical_denomination() {
        let definition = crate::spacetimedb::CatalogItemView {
            id: "lubeck_mark".into(),
            kind: crate::spacetimedb::CatalogItemKind::Currency,
            base_value: Some(1),
            weight: 0.01,
            ..Default::default()
        };
        let rendered = item_name_with_quality(&definition.id, Some(&definition)).into_string();
        assert!(rendered.contains(">Coin<"));
        assert!(rendered.contains("data-item-edit-url=\"https://github.com/"));
        assert!(rendered.contains("content/items/catalog.yaml#L"));
        let historical_name = adventuresim_core::strategic_currency::currency_name("lubeck_mark")
            .expect("Lübeck mark remains an authored currency");
        assert!(rendered.contains(&format!("data-currency-name=\"{historical_name}\"")));
        assert!(!rendered.contains(&format!(">{historical_name}<")));
    }

    #[test]
    fn alcohol_labels_expose_a_shared_inventory_group() {
        let definition = crate::spacetimedb::CatalogItemView {
            id: "small_beer".into(),
            kind: crate::spacetimedb::CatalogItemKind::Simple,
            alcohol_serving_ml: 500,
            ..Default::default()
        };
        let rendered = item_name_with_quality(&definition.id, Some(&definition)).into_string();
        assert!(rendered.contains("data-item-name=\"small_beer\""));
        assert!(rendered.contains("data-item-group=\"alcohol\""));
        assert!(rendered.contains("data-group-name=\"Alcohol\""));
        assert!(rendered.contains("data-item-edit-url=\"https://github.com/"));
    }

    #[test]
    fn durable_item_names_expose_quality_color_and_description() {
        let definition = crate::spacetimedb::CatalogItemView {
            id: "commissioned_sword".into(),
            weight: 1.0,
            slot: Slot::AnyHolding,
            kind: crate::spacetimedb::CatalogItemKind::Weapon,
            base_value: None,
            nutrition_kcal: 0.0,
            water_capacity_ml: 0,
            quality: 4,
            durability_yield: 0.0,
            durability_fracture: 0.0,
            durability_wear: 0.0,
            durability_failure_share: 0.0,
            edge_sensitivity: 0.0,
            handling_sensitivity: 0.0,
            ..Default::default()
        };

        let rendered = item_name_with_quality(&definition.id, Some(&definition)).into_string();
        assert!(rendered.contains("item-quality-4"));
        assert!(rendered.contains("knightly commission"));
    }

    #[test]
    fn book_names_use_shared_quality_color_without_equipment_copy() {
        let definition = crate::spacetimedb::CatalogItemView {
            id: "human_anatomy".into(),
            kind: crate::spacetimedb::CatalogItemKind::Simple,
            quality: 4,
            ..Default::default()
        };

        let rendered = item_name_with_quality(&definition.id, Some(&definition)).into_string();
        assert!(rendered.contains("item-quality-4"));
        assert!(rendered.contains("title=\"Quality 4\""));
        assert!(!rendered.contains("knightly commission"));
    }

    #[test]
    fn merchant_stock_table_hides_quantity_and_target_columns() {
        let rendered = trade_inventory_table(
            "merchant-left",
            InventoryColumnSet::Basic,
            false,
            false,
            false,
            html! {},
        )
        .into_string();
        assert!(rendered.contains("<colgroup>"));
        assert!(!rendered.contains("inventory-column-count"));
        assert!(!rendered.contains("inventory-column-target"));
        assert!(rendered.contains("inventory-column-type"));
        assert!(rendered.contains("inventory-column-weight"));
        assert!(rendered.contains("inventory-column-gold"));
        assert!(rendered.contains("title=\"Currency\""));
        assert!(rendered.contains("aria-label=\"Currency\""));
        assert!(rendered.contains("/static/icons/game/coins.svg"));
    }

    #[test]
    fn inventory_type_header_and_row_share_the_first_column() {
        let rendered = trade_inventory_table(
            "test",
            InventoryColumnSet::Basic,
            true,
            false,
            false,
            html! {
                tr class="trade-inventory-row" {
                    td class="inventory-item-type" { (item_type_icon("arming_sword")) }
                    td class="inventory-item-name" { "Arming sword" }
                    td { "1" } td { "1" } td { "12" }
                }
            },
        )
        .into_string();
        let header = rendered.find("inventory-column-type").unwrap();
        let item_header = rendered.find("inventory-column-item").unwrap();
        let type_cell = rendered.find("inventory-item-type").unwrap();
        let item_cell = rendered.find("inventory-item-name").unwrap();
        assert!(header < item_header && type_cell < item_cell);
        assert!(rendered.contains("/static/icons/game/broadsword.svg"));
    }
}
