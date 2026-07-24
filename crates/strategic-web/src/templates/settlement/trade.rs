use super::*;

/// The currently available merchant storefronts. They share trade mechanics,
/// but each storefront limits the stock shown on its left-hand side.
#[derive(Clone, Copy)]
pub enum MerchantShop {
    General,
    Weapons,
    Armor,
    Clothing,
    Herbalist,
    Inn,
}
impl MerchantShop {
    pub fn storefront(self) -> adventuresim_core::settlement_economy::Storefront {
        use adventuresim_core::settlement_economy::Storefront as S;
        match self {
            Self::General => S::General,
            Self::Weapons => S::Weapons,
            Self::Armor => S::Armor,
            Self::Clothing => S::Clothing,
            Self::Herbalist => S::Herbalist,
            Self::Inn => S::Inn,
        }
    }

    pub fn available_at(self, settlement: &Settlement) -> bool {
        adventuresim_core::settlement_economy::storefront_available(
            &settlement.economy,
            self.storefront(),
        )
    }

    fn stocks_at(self, settlement: &Settlement, item: &crate::spacetimedb::ItemDefinition) -> bool {
        use adventuresim_core::settlement_economy::CatalogKind as C;
        let kind = match item.kind {
            crate::spacetimedb::ItemKind::Simple => C::Simple,
            crate::spacetimedb::ItemKind::Weapon => C::Weapon,
            crate::spacetimedb::ItemKind::Armor => C::Armor,
            crate::spacetimedb::ItemKind::Shield => C::Shield,
            crate::spacetimedb::ItemKind::Clothing => C::Clothing,
            crate::spacetimedb::ItemKind::Currency => C::Currency,
            crate::spacetimedb::ItemKind::Ingredient => C::Ingredient,
            crate::spacetimedb::ItemKind::Medication => C::Medication,
            crate::spacetimedb::ItemKind::Food => C::Food,
        };
        adventuresim_core::settlement_economy::storefront_stocks(
            &settlement.economy,
            self.storefront(),
            &item.id,
            kind,
        )
    }
    pub fn service_id(self) -> &'static str {
        match self {
            Self::General => "merchants",
            Self::Weapons => "weapons",
            Self::Armor => "armor",
            Self::Clothing => "clothing",
            Self::Herbalist => "herbalist",
            Self::Inn => "inn",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::General => "General Market",
            Self::Weapons => "Weaponsmith",
            Self::Armor => "Armourer",
            Self::Clothing => "Tailor",
            Self::Herbalist => "Herbalist",
            Self::Inn => "The Inn",
        }
    }

    fn stocks(self, item: &crate::spacetimedb::ItemDefinition) -> bool {
        let kind = item.kind;
        match self {
            Self::General => !matches!(
                kind,
                crate::spacetimedb::ItemKind::Currency
                    | crate::spacetimedb::ItemKind::Ingredient
                    | crate::spacetimedb::ItemKind::Medication
            ),
            Self::Weapons => matches!(
                kind,
                crate::spacetimedb::ItemKind::Weapon | crate::spacetimedb::ItemKind::Shield
            ),
            Self::Armor => kind == crate::spacetimedb::ItemKind::Armor,
            Self::Clothing => kind == crate::spacetimedb::ItemKind::Clothing,
            Self::Herbalist => matches!(
                kind,
                crate::spacetimedb::ItemKind::Ingredient | crate::spacetimedb::ItemKind::Medication
            ),
            Self::Inn => {
                adventuresim_core::food::definition(&item.id).is_some()
                    || matches!(
                        item.id.as_str(),
                        "cooking_pan" | "cooking_pot" | "portable_oven"
                    )
            }
        }
    }

    fn shows_inventory(self, item: &crate::spacetimedb::ItemDefinition) -> bool {
        item.kind == crate::spacetimedb::ItemKind::Currency || self.stocks(item)
    }
}

pub fn alchemy_page(
    settlement: &Settlement,
    character: &Character,
    party_members: &[Character],
    medicine: f32,
    selected: &adventuresim_core::disease::MedicationRecipe,
    inventory: &[InventoryItem],
    pooled: &[PartyInventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    personal_targets: &[InventoryQuantityTarget],
    party_targets: &[InventoryQuantityTarget],
    party_scope: bool,
) -> Markup {
    let scope = if party_scope { "party" } else { "personal" };
    let return_to = format!(
        "/locations/settlement/{}/alchemy?recipe={}&scope={scope}",
        settlement.id, selected.item_id
    );
    let herbalist_href = format!(
        "/settlements/{}/herbalist?return_to={}",
        settlement.id,
        return_to
            .replace('%', "%25")
            .replace('?', "%3F")
            .replace('&', "%26")
            .replace('=', "%3D")
    );
    let recipes: Vec<_> = adventuresim_core::disease::MEDICATION_RECIPES
        .iter()
        .filter(|recipe| adventuresim_core::disease::can_prepare_medication(medicine, recipe))
        .collect();
    let content = html! {
        aside class="left-sidebar alchemy-recipes" {
            (sidebar_section("Preparations", html! {
                nav class="alchemy-recipe-list" aria-label="Known medication recipes" {
                    @for recipe in recipes {
                        a class=[(recipe.item_id == selected.item_id).then_some("active")]
                            href=(format!("/locations/settlement/{}/alchemy?recipe={}&scope={}", settlement.id, recipe.item_id, if party_scope { "party" } else { "personal" })) {
                            strong { (recipe.name) }
                            span { "Medicine " (recipe.medicine_dc) " · " (recipe.preparation_minutes) " minutes" }
                        }
                    }
                }
            }))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(character), &format!("/locations/settlement/{}", settlement.id), Some(character.id), true))
            (visual_stage("alchemy", "Alchemy", "A working table of herbs, vessels, and prepared medicines"))
            a class="stage-context-link" href=(herbalist_href) {
                "Return to the herbalist"
            }
            (settlement_chat_area_with_info("Alchemy", Some(character), &[format!("Selected recipe: {}", selected.name)]))
            form method="post" action=(format!("/locations/settlement/{}/alchemy/craft", settlement.id)) class="alchemy-craft-form" {
                input type="hidden" name="disease_id" value=(format!("{:?}", selected.disease_id).to_ascii_lowercase());
                input type="hidden" name="party_scope" value=(party_scope);
                button type="submit" class="btn btn-primary" {
                    "Prepare " (selected.name) " · " (selected.preparation_minutes) " minutes"
                }
            }
        }
        aside class="right-sidebar inventory-owner-panel" data-inventory-tabs {
            nav class="inventory-owner-tabs" aria-label="Ingredient inventory" {
                a class=(if !party_scope { "inventory-owner-tab active" } else { "inventory-owner-tab" })
                    href=(format!("/locations/settlement/{}/alchemy?recipe={}&scope=personal", settlement.id, selected.item_id)) { "Player" }
                a class=(if party_scope { "inventory-owner-tab active" } else { "inventory-owner-tab" })
                    href=(format!("/locations/settlement/{}/alchemy?recipe={}&scope=party", settlement.id, selected.item_id)) { "Party" }
            }
            (sidebar_section("Required ingredients", html! {
                table class="trade-inventory-table" {
                    (trade_inventory_table_header(false, None))
                    tbody {
                    @for ingredient in selected.ingredients {
                        @let definition = items.iter().find(|item| item.id == ingredient.item_id);
                        @let quantity = if party_scope {
                            pooled.iter().filter(|item| item.item_id == ingredient.item_id).map(|item| item.quantity).sum()
                        } else {
                            inventory.iter().filter(|item| item.item_id == ingredient.item_id).map(|item| item.qty).sum()
                        };
                        @let target = target_quantity(if party_scope { party_targets } else { personal_targets }, ingredient.item_id);
                        tr class="trade-inventory-row trade-row-player" data-inventory-quantity=(quantity) data-target=(target) {
                            td class="inventory-item-type" { (item_type_icon(ingredient.item_id)) }
                            td class="inventory-item-name" { (item_name_with_quality(ingredient.item_id, definition)) }
                            td class="inventory-count" { (quantity_target_control(quantity, target, ingredient.item_id, party_scope)) }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (format!("need {}", ingredient.quantity)) }
                        }
                    }
                    }
                }
                p class="small-copy text-muted" { "Targets can be raised here for future purchasing. Crafting consumes the listed quantities from the selected inventory." }
            }))
        }
    };
    settlement_layout_with_session(
        "Alchemy",
        &settlement.name,
        &settlement.id,
        &settlement.category,
        "herbalist",
        Some(&settlement.religion_id),
        Some(&settlement.economy),
        content,
        Some(&character.name),
    )
}

/// Settlement information and the next destinations on the imported road and
/// ferry network.
/// Market interface shown while settlement stock is unavailable.
pub fn merchants_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    party_members: &[Character],
    logged_in_as: Option<&str>,
) -> Markup {
    service_page(
        settlement,
        "merchants",
        "Market Square",
        "Market Steward",
        "The market steward has no listed stock at present.",
        active_character,
        inventory,
        &[],
        party_members,
        logged_in_as,
        None,
        None,
        SoapRestPreview::default(),
    )
}

/// Church interface.
pub fn religion_page(
    settlement: &Settlement,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    limbs: Option<&CharacterLimbs>,
    stats: Option<&CharacterStats>,
    condition: Option<&CharacterCondition>,
    field_repair_minutes: u64,
    smith_wait_minutes: u64,
    soap_preview: SoapRestPreview,
    logged_in_as: Option<&str>,
) -> Markup {
    service_page(
        settlement,
        "religion",
        "Church",
        "Priest",
        "Faith, donations, and divine services require the religion and reputation systems.",
        active_character,
        inventory,
        items,
        party_members,
        logged_in_as,
        rest_default_minutes(
            limbs,
            stats,
            condition,
            field_repair_minutes,
            smith_wait_minutes,
        ),
        None,
        soap_preview,
    )
}

/// Party inventory comparison.
pub fn party_inventory_page(
    location: &LocationView,
    selected: &Character,
    selected_inventory: &[InventoryItem],
    active_character: &Character,
    active_inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    selected_equip: Option<&CharacterEquip>,
    active_equip: Option<&CharacterEquip>,
    selected_targets: &[InventoryQuantityTarget],
    active_targets: &[InventoryQuantityTarget],
    selected_encumbrance: EncumbranceSummary,
    active_encumbrance: EncumbranceSummary,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (party_trade_inventory_rail(selected, selected_inventory, items, active_character.id, "right", selected_equip, active_targets, selected_encumbrance))
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(party_members, Some(active_character), &location.base_path(), Some(selected.id), false))
            (visual_stage("character", &selected.name, "Party member and trading companion"))
            (player_chat_area(selected, active_character))
            form id="party-offer" class="party-offer" action=(format!("{}/party/{}/inventory/offer", location.base_path(), selected.id)) method="post" hidden
                role="dialog" aria-modal="true" aria-label="Confirm party item offer" tabindex="-1" {
                span class="party-offer-summary" { "Review and send the staged item offer." }
                button type="button" class="party-offer-cancel" data-cancel-trade="party" { "Cancel" }
                button type="submit" disabled { "Offer" }
            }
        }
        aside class="right-sidebar" {
            (party_trade_inventory_rail(active_character, active_inventory, items, selected.id, "left", active_equip, selected_targets, active_encumbrance))
        }
    };
    location.render_layout("Party", content, Some(&active_character.name))
}

/// The active character's inventory with a staged discard list.
pub fn party_discard_page(
    location: &LocationView,
    active_character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    equip: Option<&CharacterEquip>,
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
            (party_portrait_overlay(party_members, Some(active_character), &location.base_path(), Some(active_character.id), false))
            (visual_stage("character", &active_character.name, "Your carried equipment and supplies"))
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
            (discard_inventory_rail(active_character, inventory, items, equip, encumbrance))
        }
    };
    location.render_layout("Inventory", content, Some(&active_character.name))
}

/// Active character's combined strategic view.
pub fn party_personal_page(
    location: &LocationView,
    active_character: &Character,
    party_members: &[Character],
    capability: Option<&CharacterCapability>,
    attributes: Option<&CharacterAttributes>,
    skills: Option<&CharacterSkills>,
    limbs: Option<&CharacterLimbs>,
    condition: Option<&CharacterStrategicCondition>,
    morale_sources: &[crate::spacetimedb::CharacterMoraleSource],
    religion_id: Option<&str>,
    prayer_religion_check: f32,
    schedule: Option<&CharacterTrainingSchedule>,
    combat_profile: CombatTrainingProfile,
    activity_preview: ActivityPreviewRates,
    religious_demand: Option<&crate::spacetimedb::ReligiousDemand>,
    notoriety: f32,
    personality: Option<&crate::spacetimedb::CharacterPersonality>,
    medical: &MedicalPresentation,
    can_examine: bool,
    injuries: &[LimbInjury],
    projectiles: &[RetainedProjectile],
    filth: &[crate::spacetimedb::CharacterFilth],
    cooking: bool,
    inventory: &[InventoryItem],
    food_lots: &[FoodLot],
    item_definitions: &[ItemDefinition],
    character_action_dialog: Option<Markup>,
    surgery_open: Option<&str>,
    social_open: bool,
) -> Markup {
    let cooking_href = location.preserve_building(format!(
        "{}/party/{}?cook=true",
        location.base_path(),
        active_character.id
    ));
    let examination_action = location.preserve_building(format!(
        "{}/party/{}/examine",
        location.base_path(),
        active_character.id
    ));
    let cooking_open = cooking && medical.examination_id.is_none();
    let surgery_path_template = location.preserve_building(format!(
        "{}/party/{}/surgery/__limb__",
        location.base_path(),
        active_character.id
    ));
    let content = html! {
        aside class="left-sidebar" {
            (party_attributes_rail("Your attributes", attributes, limbs, medical, Some((&surgery_path_template, surgery_open)), injuries, projectiles))
            (strategic_condition_rail(condition, morale_sources, filth, &location.preserve_building(format!("{}/party/{}/social", location.base_path(), active_character.id)), social_open))
            (medical_rail(medical, &location.base_path(), active_character.id, active_character.id, true))
            @if let Some(demand) = religious_demand {
                (religious_demand_rail(demand, &location.base_path(), active_character.id))
            }
        }
        main class="center-content settlement-main party-member-stage" {
            (party_portrait_overlay(
                party_members,
                Some(active_character),
                &location.base_path(),
                Some(active_character.id),
                can_examine,
            ))
            (visual_stage("character", &active_character.name, "Your identity, condition, and capabilities"))
            (settlement_chat_area(&active_character.name, Some(active_character)))
            (medical_examination_popup(medical, location, active_character.id, limbs, injuries, projectiles))
        }
        aside class="right-sidebar" {
            (character_summary_rail(capability))
            (character_bio_rail(active_character, religion_id, notoriety, personality, true, &location.base_path()))
            @let schedule_action = format!("{}/party/{}/schedule", location.base_path(), active_character.id);
            (party_skills_rail(
                "Your skills", skills, limbs, schedule, Some(&schedule_action),
                Some(activity_preview), religion_id.is_some(), prayer_religion_check,
                religion_id.or(location.religion_id.as_deref()),
                combat_profile,
                CharacterSkillActions {
                    cooking_href: Some(&cooking_href),
                    cooking_open,
                    examination_action: can_examine.then_some(examination_action.as_str()),
                    examination_open: medical.examination_id.is_some(),
                },
            ))
        }
        @if cooking_open {
            (cooking_activity_dialog(location, active_character, inventory, food_lots, item_definitions))
        } @else if medical.examination_id.is_none() {
            @if let Some(dialog) = character_action_dialog { (dialog) }
        }
    };
    location.render_layout("Party", content, Some(&active_character.name))
}

pub(super) fn cooking_activity_dialog(
    location: &LocationView,
    active_character: &Character,
    inventory: &[InventoryItem],
    food_lots: &[FoodLot],
    item_definitions: &[ItemDefinition],
) -> Markup {
    let close_href = location.preserve_building(format!(
        "{}/party/{}",
        location.base_path(),
        active_character.id
    ));
    let cook_action = location.preserve_building(format!(
        "{}/party/{}/cook",
        location.base_path(),
        active_character.id
    ));
    let owns = |item_id: &str| {
        inventory
            .iter()
            .any(|row| row.item_id == item_id && row.qty > 0)
    };
    let pan = owns("cooking_pan");
    let pot = owns("cooking_pot");
    let oven = owns("portable_oven");
    let ingredients = inventory
        .iter()
        .filter(|item| {
            food_lots
                .iter()
                .any(|lot| lot.inventory_item_id == Some(item.id))
        })
        .collect::<Vec<_>>();
    html! {
        div class="character-action-overlay" data-character-action-dialog data-initial-focus="[data-cooking-method]:checked" {
            a class="character-action-backdrop" href=(&close_href) aria-label="Close cooking dialog" {}
            section class="character-action-dialog cooking-dialog" role="dialog" aria-modal="true" aria-labelledby="cooking-dialog-title" tabindex="-1" {
            header class="character-action-dialog-header" {
                h2 id="cooking-dialog-title" { "Cooking" }
                a class="character-action-dialog-close" href=(&close_href) aria-label="Close cooking dialog" { "×" }
            }
            div class="cooking-activity" data-cooking-activity {
            aside class="cooking-pot" aria-label="Cooking pot" {
                (sidebar_section("Pot", html! {
                    p class="text-muted small-copy cooking-pot-empty" data-cooking-pot-empty {
                        "Transfer ingredients here to prepare a meal."
                    }
                    (trade_inventory_table("cooking-pot-left", InventoryColumnSet::Basic, true, false, false, html! {}))
                }))
            }
            main class="cooking-stage" {
                section class="cooking-workspace" aria-label="Cooking workspace" {
                    div class="cooking-method-list" aria-label="Cooking instrument" {
                        (cooking_method("pan-fry", "Pan-fry", "meal", pan, "A pan is required", false))
                        (cooking_method("stew", "Stew", "water-bottle", pot, "A pot and water are required", false))
                        (cooking_method("roast", "Roast / skewer", "campfire", true, "", true))
                        (cooking_method("bake", "Bake", "bread", oven, "A portable oven is required", false))
                    }
                    img class="cooking-stage-placeholder" src="/static/icons/game/campfire.svg"
                        alt="Placeholder for the cooking vessel and fire";
                    p class="text-muted small-copy" { "Cooking scene placeholder" }
                }
                form id="cooking-submit-form" class="cooking-submit-form" method="post"
                    action=(&cook_action) {
                    input type="hidden" name="inventory_item_ids" value="" data-cooking-ids;
                    input type="hidden" name="quantities" value="" data-cooking-quantities;
                    div class="party-offer cooking-actions" {
                        a class="party-offer-cancel" href=(&close_href) { "Cancel" }
                        button type="submit" disabled title="Select at least one ingredient" data-cook-submit { "Cook" }
                    }
                }
            }
            aside class="cooking-ingredients" aria-label="Ingredient inventory" {
                @let title = format!("{}'s inventory", active_character.name);
                (sidebar_section(&title, html! {
                    @if ingredients.is_empty() {
                        (empty_state("No food carried.", None, None))
                    } @else {
                        (trade_inventory_table("cooking-inventory-right", InventoryColumnSet::Basic, true, false, false, html! {
                            @for item in ingredients {
                                @let definition = item_definitions.iter().find(|definition| definition.id == item.item_id);
                                @let food_lot = food_lots.iter().find(|lot| lot.inventory_item_id == Some(item.id));
                                @let display_name = food_lot.map_or_else(|| item_display_name(&item.item_id), |lot| lot.display_name.clone());
                                @let unit_mass = food_lot.map_or_else(|| definition.map_or(0.0, |definition| definition.weight), |lot| lot.mass_kg / item.qty.max(1) as f32);
                                @let value = food_lot.map_or_else(|| item_value(definition), |lot| weight_display(lot.total_value));
                                tr class="trade-inventory-row trade-row-player" data-cooking-source=(item.id) data-item-key=(&item.item_id) {
                                    td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                                    td class="inventory-item-name" {
                                        (item_name_with_display(&item.item_id, &display_name, definition))
                                        span class="inventory-row-actions" {
                                            @if food_lot.is_some() {
                                                @let safety = adventuresim_core::food::definition(&item.item_id).map_or(5, |food| food.cooking_minutes);
                                                button type="button" class="trade-transfer trade-transfer-left"
                                                    data-cooking-stage=(item.id) data-cooking-name=(&display_name)
                                                    data-count=(item.qty) data-mass=(format!("{unit_mass:.4}")) data-safety=(safety)
                                                    data-dynamic-transfer data-default-transfer-mode="one" data-transfer-mode="one"
                                                    data-label-one=(format!("Add one {display_name} to the pot"))
                                                    data-label-target=(format!("Add {display_name} to the pot"))
                                                    data-label-all=(format!("Add all {display_name} to the pot"))
                                                    aria-label=(format!("Add one {display_name} to the pot"))
                                                    title=(format!("Add one {display_name} to the pot")) { (transfer_glyph(1)) }
                                            } @else {
                                                (disabled_transfer_button("left", "Only food ingredients can be added to the pot"))
                                            }
                                        }
                                    }
                                    td class="inventory-count" { (item.qty) }
                                    td class="inventory-weight" { (weight_display(unit_mass)) }
                                    td class="inventory-gold" { (value) }
                                }
                            }
                        }))
                    }
                }))
            }
            }
            }
        }
    }
}

pub(super) fn cooking_method(
    value: &str,
    label: &str,
    icon: &str,
    available: bool,
    reason: &str,
    selected: bool,
) -> Markup {
    html! {
        label class=(if available { "cooking-method" } else { "cooking-method disabled" })
            title=(if available { label } else { reason }) {
            input type="radio" name="method" value=(value) form="cooking-submit-form"
                checked[selected] disabled[!available]
                data-cooking-method data-unavailable-reason=[(!available).then_some(reason)];
            span class="cooking-method-icon"
                style=(format!("--cooking-method-icon: url('/static/icons/game/{icon}.svg')"))
                aria-hidden="true" {}
            span class="sr-only" { (label) }
            @if !available { span class="sr-only" { (reason) } }
        }
    }
}

pub(super) fn filth_status_bar(deposits: &[crate::spacetimedb::CharacterFilth]) -> Markup {
    use crate::spacetimedb::{FilthOrigin, FilthSubstance};
    let dirt: u16 = deposits
        .iter()
        .filter(|d| d.substance == FilthSubstance::Dirt)
        .map(|d| d.amount)
        .fold(0, u16::saturating_add);
    let blood: u16 = deposits
        .iter()
        .filter(|d| d.substance == FilthSubstance::Blood)
        .map(|d| d.amount)
        .fold(0, u16::saturating_add);
    let total = dirt
        .saturating_add(blood)
        .min(adventuresim_core::filth::MAX_FILTH);
    let dirt_width = f32::from(dirt.min(total));
    let blood_width = f32::from(blood.min(total.saturating_sub(dirt.min(total))));
    let (own_blood, foreign_blood, unknown_blood) = deposits
        .iter()
        .filter(|d| d.substance == FilthSubstance::Blood)
        .fold((0_u16, 0_u16, 0_u16), |mut amounts, deposit| {
            match deposit.origin {
                FilthOrigin::Own => amounts.0 = amounts.0.saturating_add(deposit.amount),
                FilthOrigin::Foreign => amounts.1 = amounts.1.saturating_add(deposit.amount),
                FilthOrigin::Unknown => amounts.2 = amounts.2.saturating_add(deposit.amount),
            }
            amounts
        });
    let summary = format!(
        "Current: {total}/100 — {dirt} dirt, {blood} blood ({own_blood} own, {foreign_blood} foreign, {unknown_blood} unknown)."
    );
    let details = format!(
        "Filth accumulates from travel, combat, and medical treatment. Dirt and blood fill this bar. Foreign blood can transmit bloodborne disease, with greater risk through open cuts and lesser risk through bandaged cuts. Soap is used automatically before rest to wash filth away.\n\n{summary}"
    );
    html! {
        div class="filth-status" tabindex="0" role="meter" aria-valuemin="0" aria-valuemax="100"
            aria-valuenow=(total) aria-label=(format!("Filth {total} out of 100"))
            data-strategic-tooltip=(&details) {
            strong class="metric-label filth-status-label" { "Filth" }
            span class="filth-track" aria-hidden="true" {
                @if dirt > 0 {
                    span class="filth-segment filth-dirt" style=(format!("width:{dirt_width}%"))
                        data-strategic-tooltip=(format!("Dirt\n{dirt}")) {}
                }
                @if blood > 0 {
                    span class="filth-segment filth-blood" style=(format!("width:{blood_width}%"))
                        data-strategic-tooltip=(format!("Blood\n{blood}")) {}
                }
            }
        }
    }
}

pub(super) fn religious_demand_rail(
    demand: &crate::spacetimedb::ReligiousDemand,
    location_path: &str,
    character_id: u64,
) -> Markup {
    let action = format!(
        "{location_path}/party/{character_id}/religious-demand/{}",
        demand.id
    );
    html! {
        (sidebar_section("Conviction demands", html! {
            article class="religious-demand" {
                h3 { (&demand.title) }
                p { (&demand.description) }
                p class="text-muted small-copy" {
                    "Observe and bear the practical cost, or decline. Party Command automatically reduces the morale cost of neglect and can remove it entirely."
                }
                form method="post" action=(action) class="religious-demand-actions" {
                    button type="submit" name="choice" value="observe" class="btn btn-primary" { "Observe" }
                    button type="submit" name="choice" value="refuse" class="btn btn-danger" { "Do not observe" }
                }
            }
        }))
    }
}

/// Selected party member stats and biography.
pub(super) fn service_page(
    settlement: &Settlement,
    service_id: &str,
    title: &str,
    npc_name: &str,
    service_summary: &str,
    active_character: Option<&Character>,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    logged_in_as: Option<&str>,
    rest_default_minutes: Option<u64>,
    rest_summary: Option<&RestSummary>,
    soap_preview: SoapRestPreview,
) -> Markup {
    let trade_offers: Option<(&str, &[&str])> = match service_id {
        "merchants" => Some((
            "Merchant stock",
            &["Weapon offer", "Armour offer", "Provision offer"],
        )),
        "weapons" => Some((
            "Weapons",
            &["Weapon offer", "Shield offer", "Ammunition offer"],
        )),
        "armor" => Some((
            "Armour",
            &["Head protection", "Torso protection", "Limb protection"],
        )),
        "clothing" => Some((
            "Clothing",
            &["Travel attire", "Cold-weather clothing", "Fine clothing"],
        )),
        "inn" => Some((
            "Inn supplies",
            &["Rations", "Water", "Supplies", "Bed for the night"],
        )),
        _ => None,
    };
    let content = html! {
        aside class=(if service_id == "inn" || service_id == "religion" { "left-sidebar service-left-sidebar" } else { "left-sidebar" }) {
            @if service_id == "inn" {
                div class="service-left-stack" {
                    div class="service-inventory-area" { (merchant_offers_rail("Inn supplies", &["Rations", "Water", "Supplies", "Bed for the night"])) }
                    (rest_service_menu("Inn", &settlement.id, "inn", rest_default_minutes, rest_summary, soap_preview))
                }
            } @else if service_id == "religion" {
                div class="service-left-stack" {
                    div class="service-inventory-area" {
                        (sidebar_section("Church services", html! {
                            p title=[active_character.is_some().then_some("Speak with the priest to profess this faith. Renunciation is available from your biography. Shared conviction strengthens allied Command; conflicting conviction penalizes morale.")] {
                                "Faith: " strong { (religion_name(Some(&settlement.religion_id))) }
                            }
                        }))
                    }
                    (rest_service_menu("Temple", &settlement.id, "temple", rest_default_minutes, rest_summary, soap_preview))
                }
            } @else if let Some((stock_title, offers)) = trade_offers {
                (merchant_offers_rail(stock_title, offers))
            } @else {
                (sidebar_section("Service", html! {
                    p class="small-copy" { (service_summary) }
                }))
            }
        }
        main class="center-content settlement-main" {
            (party_portrait_overlay(party_members, active_character, &format!("/locations/settlement/{}", settlement.id), None, false))
            (npc_portrait_strip(&settlement.id, npc_location_id(service_id)))
            (npc_description_stage(npc_name, &format!("{title} host and service counter")))
            (settlement_npc_chat_area(title, active_character, &settlement.id, npc_location_id(service_id), Some(service_id)))
        }
        aside class="right-sidebar" {
            @if trade_offers.is_some() {
                (inventory_rail(
                    active_character,
                    inventory,
                    items,
                    None,
                    matches!(service_id, "weapons" | "armor" | "clothing"),
                ))
            } @else if service_id == "smith" {
                (inventory_rail(
                    active_character,
                    inventory,
                    items,
                    None,
                    true,
                ))
            } @else if service_id == "religion" {
                (inventory_rail(active_character, inventory, items, None, false))
            } @else {
                (sidebar_section("Service", html! {
                    p class="small-copy" { (service_summary) }
                }))
            }
        }
    };
    settlement_layout_with_session(
        title,
        &settlement.name,
        &settlement.id,
        &settlement.category,
        service_id,
        Some(&settlement.religion_id),
        Some(&settlement.economy),
        content,
        logged_in_as,
    )
}

pub(super) fn party_trade_inventory_rail(
    character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    recipient_id: u64,
    direction: &str,
    equip: Option<&CharacterEquip>,
    recipient_targets: &[InventoryQuantityTarget],
    encumbrance: EncumbranceSummary,
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
                            @let is_equipped = equip.is_some_and(|equip| [equip.left_hand_item_id, equip.right_hand_item_id, equip.left_arm_armor_id, equip.right_arm_armor_id, equip.left_leg_armor_id, equip.right_leg_armor_id, equip.head_armor_id, equip.chest_armor_id, equip.stomach_armor_id].contains(&Some(item.id)));
                            @let definition = items.iter().find(|definition| definition.id == item.item_id);
                            @let target = target_quantity(recipient_targets, &item.item_id);
                            @let item_name = item_display_name(&item.item_id);
                                tr class=(if direction == "left" { "trade-inventory-row trade-row-player" } else { "trade-inventory-row trade-row-merchant" }) data-item-key=(&item.item_id) {
                                    td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                                    td class="inventory-item-name" {
                                        (item_name_with_quality(&item.item_id, definition))
                                        span class="inventory-row-actions" {
                                            @if is_equipped {
                                                (disabled_transfer_button(direction, "Equipped items cannot be transferred"))
                                            } @else {
                                                button type="button" class=(format!("trade-transfer trade-transfer-{direction} party-draft-transfer")) data-dynamic-transfer data-default-transfer-mode="one" data-from=(character.id) data-to=(recipient_id) data-item=(item.id) data-key=(&item.item_id) data-count=(item.qty) data-target=(target) data-transfer-mode="one" data-label-one=(format!("Transfer one {item_name}")) data-label-target=(format!("Transfer {item_name} to target")) data-label-all=(format!("Transfer all {item_name}")) aria-label=(format!("Transfer one {item_name}")) title=(format!("Transfer one {item_name}")) { (transfer_glyph(1)) }
                                            }
                                        }
                                    }
                                    td class="inventory-count" { (item.qty) }
                                    td class="inventory-equipped" { (equipment_checkbox(item, definition, is_equipped)) }
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

pub(super) fn discard_inventory_rail(
    character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    equip: Option<&CharacterEquip>,
    encumbrance: EncumbranceSummary,
) -> Markup {
    let title = format!("{}'s inventory", character.name);
    html! {
        (sidebar_section(&title, html! {
            (encumbrance_inventory_rail(html! {
                @if inventory.is_empty() {
                    p class="text-muted small-copy" { "No items carried." }
                } @else {
                    (trade_inventory_table("discard-right", InventoryColumnSet::All, true, true, false, html! {
                        @for item in inventory {
                            @let is_equipped = equip.is_some_and(|equip| [equip.left_hand_item_id, equip.right_hand_item_id, equip.left_arm_armor_id, equip.right_arm_armor_id, equip.left_leg_armor_id, equip.right_leg_armor_id, equip.head_armor_id, equip.chest_armor_id, equip.stomach_armor_id].contains(&Some(item.id)));
                            @let definition = items.iter().find(|definition| definition.id == item.item_id);
                            @let item_name = item_display_name(&item.item_id);
                            tr class="trade-inventory-row trade-row-player" data-discard-source=(item.id) data-item-key=(&item.item_id) {
                                td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                                td class="inventory-item-name" {
                                    (item_name_with_quality(&item.item_id, definition))
                                    span class="inventory-row-actions" {
                                        @if is_equipped {
                                            (disabled_transfer_button("left", "Equipped items cannot be discarded"))
                                        } @else {
                                            button type="button" class="trade-transfer trade-transfer-left"
                                            data-discard-item=(item.id) data-count=(item.qty)
                                            data-dynamic-transfer data-default-transfer-mode="one" data-transfer-mode="one"
                                            data-label-one=(format!("Discard one {item_name}"))
                                            data-label-target=(format!("Discard {item_name} down to target"))
                                            data-label-all=(format!("Discard all {item_name}"))
                                            aria-label=(format!("Discard {item_name}"))
                                            title=(format!("Discard one {item_name}")) { (transfer_glyph(1)) }
                                        }
                                    }
                                }
                                td class="inventory-count" { (item.qty) }
                                td class="inventory-equipped" { (equipment_checkbox(item, definition, is_equipped)) }
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

pub fn live_merchant_shop_page(
    settlement: &Settlement,
    character: &Character,
    inventory: &[InventoryItem],
    items: &[crate::spacetimedb::ItemDefinition],
    food_lots: &[FoodLot],
    party_members: &[Character],
    equip: Option<&CharacterEquip>,
    personal_targets: &[InventoryQuantityTarget],
    party_targets: &[InventoryQuantityTarget],
    pooled: &[PartyInventoryItem],
    shop: MerchantShop,
    shared_language: f32,
    problem_buy_bps: i32,
    problem_sell_penalty_bps: i32,
    conditions: &[crate::spacetimedb::ItemCondition],
    smith: Option<&crate::spacetimedb::SettlementSmith>,
    repair_orders: &[crate::spacetimedb::RepairOrder],
    now_minutes: u64,
    personal_encumbrance: EncumbranceSummary,
    party_encumbrance: EncumbranceSummary,
    rest_default_minutes: Option<u64>,
    soap_preview: SoapRestPreview,
) -> Markup {
    let title = shop.title();
    let service_id = shop.service_id();
    // Herbalist purchases use a separate reducer and retain their specialized quote.
    let trade_language = if matches!(shop, MerchantShop::Herbalist) {
        1.0
    } else {
        shared_language
    };
    let smith_skill = smith
        .map(|smith| {
            if matches!(shop, MerchantShop::Armor) {
                smith.armourer_skill
            } else if matches!(shop, MerchantShop::Clothing) {
                smith.tailor_skill
            } else {
                smith.weaponsmith_skill
            }
        })
        .unwrap_or(0);
    let player_footer = if matches!(shop, MerchantShop::Herbalist) {
        html! {}
    } else {
        inventory_footer_controls_with_leading(
            matches!(
                shop,
                MerchantShop::Weapons | MerchantShop::Armor | MerchantShop::Clothing
            )
            .then(|| repair_all_control(settlement, service_id)),
            "sell",
            "Sell surplus",
            "Sell everything",
        )
    };
    let content = html! {
        aside class=(if matches!(shop, MerchantShop::Inn) { "left-sidebar smith-wares-column service-left-sidebar" } else { "left-sidebar smith-wares-column" }) {
        div class=(if matches!(shop, MerchantShop::Inn) { "service-left-stack" } else { "merchant-stock-stack" }) {
        div class=(if matches!(shop, MerchantShop::Inn) { "service-inventory-area" } else { "merchant-stock-area" }) {
        (sidebar_section(if matches!(shop, MerchantShop::Herbalist) { "Prepared medicines and ingredients" } else if matches!(shop, MerchantShop::Inn) { "Cooking supplies" } else { "Merchant stock" }, html! {
            div class="smith-wares-scroll" {
            (trade_inventory_table("merchant-left", if matches!(shop, MerchantShop::Weapons) { InventoryColumnSet::Weapons } else if matches!(shop, MerchantShop::Armor) { InventoryColumnSet::Armor } else { InventoryColumnSet::Basic }, false, false, false, html! {
                @for item in items.iter().filter(|item| shop.stocks_at(settlement, item)) {
                    @let is_currency = item.kind == crate::spacetimedb::ItemKind::Currency;
                    @let medication_recipe = adventuresim_core::disease::medication_recipe_for_item(&item.id);
                    @let buy_price = adventuresim_core::local_problem::adjust_price(adventuresim_core::strategic_economy::language_adjusted_buy_price(medication_recipe.map_or_else(
                        || adventuresim_core::strategic_economy::merchant_buy_price(item.base_value.unwrap_or(1)),
                        adventuresim_core::strategic_economy::herbalist_medication_price,
                    ), trade_language), problem_buy_bps);
                    @let sell_price = adventuresim_core::local_problem::adjust_price(adventuresim_core::strategic_economy::language_adjusted_sell_price((item.base_value.unwrap_or(1) as f32 / 1.25).floor().max(1.0) as u32, trade_language), -problem_sell_penalty_bps);
                    @let target = target_quantity(personal_targets, &item.id);
                    @let display_name = medication_recipe.map_or_else(|| item_display_name(&item.id), |recipe| recipe.name.to_owned());
                    tr class="trade-inventory-row trade-row-merchant" data-merchant-item=(&item.id) data-merchant-sell-price=(sell_price) data-group-summary="catalog" data-herbalist-medication-name=[medication_recipe.map(|recipe| recipe.name)] { td class="inventory-item-type" { (item_type_icon(&item.id)) } td class="inventory-item-name" { (item_name_with_display(&item.id, &display_name, Some(item))) @if !is_currency { (merchant_buy_controls(&item.id, buy_price, target, 999)) } } td class="inventory-count" hidden { "999" } td class="inventory-weight" { (weight_display(item.weight)) } td class="inventory-gold" { (buy_price) } }
                }
            }))
            (inventory_footer_controls("buy", "Buy to targets", "Buy everything"))
            @if matches!(shop, MerchantShop::Herbalist) {
                p class="small-copy text-muted" { "Prepared courses are sold into your personal inventory as separate, equippable items. Party-inventory purchasing is unavailable here." }
            }
            }
        }))
        }
        @if matches!(shop, MerchantShop::Inn) {
            (rest_service_menu("Inn", &settlement.id, "inn", rest_default_minutes, None, soap_preview))
        }
        }
        @if matches!(shop, MerchantShop::Weapons | MerchantShop::Armor | MerchantShop::Clothing) {
            (repair_custody_panel(settlement, shop, repair_orders, conditions, items, now_minutes, smith_skill))
        }
        }
        main class="center-content settlement-main" { (party_portrait_overlay(party_members, Some(character), &format!("/locations/settlement/{}", settlement.id), None, false)) (npc_portrait_strip(&settlement.id, npc_location_id(service_id))) (npc_description_stage(title, "Merchant counter and attending craftsperson")) (settlement_npc_chat_area(title, Some(character), &settlement.id, npc_location_id(service_id), Some(service_id))) form # "merchant-offer" class="party-offer" action=(if matches!(shop, MerchantShop::Herbalist) { format!("/settlements/{}/herbalist/purchase", settlement.id) } else { format!("/settlements/{}/merchants/offer", settlement.id) }) method="post" hidden role="dialog" aria-modal="true" aria-label="Confirm merchant offer" tabindex="-1" { span class="party-offer-summary" { "Review and submit the staged trade." } input type="hidden" name="return_to" value=(format!("/settlements/{}/{}", settlement.id, service_id)); input type="hidden" name="inventory_scope" value="player"; button type="button" class="party-offer-cancel" data-cancel-trade="merchant" { "Cancel" } button type="submit" disabled { "Offer" } } }
        aside class="right-sidebar inventory-owner-panel" data-inventory-tabs {
            nav class="inventory-owner-tabs" aria-label="Trading inventory" {
                button type="button" class="inventory-owner-tab active" data-inventory-tab="player" { "Player" }
                @if !matches!(shop, MerchantShop::Herbalist) {
                    button type="button" class="inventory-owner-tab" data-inventory-tab="party" { "Party" }
                }
            }
            div data-inventory-pane="player" {
            div class="sidebar-section" {
                (encumbrance_inventory_rail(html! {
                (trade_inventory_table("merchant-player-right", if matches!(shop, MerchantShop::Weapons) { InventoryColumnSet::Weapons } else if matches!(shop, MerchantShop::Armor) { InventoryColumnSet::Armor } else { InventoryColumnSet::Basic }, true, true, matches!(shop, MerchantShop::Weapons | MerchantShop::Armor | MerchantShop::Clothing), html! {
                    @for item in inventory.iter().filter(|item| items.iter().find(|definition| definition.id == item.item_id).is_some_and(|definition| shop.shows_inventory(definition))) {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let food_lot = food_lots.iter().find(|lot| lot.inventory_item_id == Some(item.id));
                        @let is_currency = definition.is_some_and(|definition| definition.kind == crate::spacetimedb::ItemKind::Currency);
                        @let is_equipped = equip.is_some_and(|equip| [equip.left_hand_item_id, equip.right_hand_item_id, equip.left_arm_armor_id, equip.right_arm_armor_id, equip.left_leg_armor_id, equip.right_leg_armor_id, equip.head_armor_id, equip.chest_armor_id, equip.stomach_armor_id].contains(&Some(item.id)));
                        @let sell_price = adventuresim_core::local_problem::adjust_price(adventuresim_core::strategic_economy::language_adjusted_sell_price(merchant_inventory_sell_price(definition, food_lot), trade_language), -problem_sell_penalty_bps);
                        @let target = target_quantity(personal_targets, &item.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&item.item_id) data-merchant-equipped=(is_equipped) data-inventory-quantity=(item.qty) data-target=(target) {
                        @let condition = conditions.iter().find(|condition| condition.inventory_item_id == item.id);
                        @let repair_skill = smith_skill;
                        @let durable_item = definition.is_some_and(|definition| matches!(definition.kind, crate::spacetimedb::ItemKind::Weapon | crate::spacetimedb::ItemKind::Armor | crate::spacetimedb::ItemKind::Shield | crate::spacetimedb::ItemKind::Clothing));
                        @let service_matches = definition.is_some_and(|definition| if matches!(shop, MerchantShop::Armor) { definition.kind == crate::spacetimedb::ItemKind::Armor } else if matches!(shop, MerchantShop::Clothing) { definition.kind == crate::spacetimedb::ItemKind::Clothing } else { matches!(definition.kind, crate::spacetimedb::ItemKind::Weapon | crate::spacetimedb::ItemKind::Shield) });
                        @let can_sell = !is_currency && !is_equipped;
                        td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                        td class="inventory-item-name" { (item_name_with_quality(&item.item_id, definition)) @if !matches!(shop, MerchantShop::Herbalist) && (can_sell || service_matches) { (merchant_sell_repair_controls(item.id, &item.item_id, sell_price, item.qty, target, can_sell, service_matches.then(|| repair_submit_control(settlement, service_id, item.id, condition, repair_skill)))) } }
                        td class="inventory-count" { (quantity_target_control(item.qty, target, &item.item_id, false)) } td class="inventory-equipped" { (equipment_checkbox(item, definition, is_equipped)) } td class="inventory-durability" { @if durable_item { (condition_bar(condition, service_matches.then_some(repair_skill))) } @else { "—" } } td class="inventory-weight" { (merchant_inventory_weight(definition, food_lot)) } td class="inventory-gold" { (sell_price) }
                    }}
                    @for target in personal_targets.iter().filter(|target| target.quantity > 0 && !inventory.iter().any(|item| item.item_id == target.item_id) && items.iter().find(|definition| definition.id == target.item_id).is_some_and(|definition| shop.shows_inventory(definition))) {
                        @let definition = items.iter().find(|definition| definition.id == target.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&target.item_id) data-inventory-quantity="0" data-target=(target.quantity) {
                            td class="inventory-item-type" { (item_type_icon(&target.item_id)) }
                            td class="inventory-item-name" { (item_name_with_quality(&target.item_id, definition)) }
                            td class="inventory-count" { (quantity_target_control(0, target.quantity, &target.item_id, false)) }
                            td class="inventory-equipped" { input type="checkbox" disabled; }
                            td class="inventory-durability" { "—" }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (item_value(definition)) }
                        }
                    }
                }))
                }, player_footer, personal_encumbrance))
            }
            }
            @if !matches!(shop, MerchantShop::Herbalist) { div data-inventory-pane="party" hidden {
            div class="sidebar-section" {
                (encumbrance_inventory_rail(html! {
                (trade_inventory_table("merchant-party-right", if matches!(shop, MerchantShop::Weapons) { InventoryColumnSet::Weapons } else if matches!(shop, MerchantShop::Armor) { InventoryColumnSet::Armor } else { InventoryColumnSet::Basic }, true, false, false, html! {
                    @for item in pooled.iter().filter(|item| items.iter().find(|definition| definition.id == item.item_id).is_some_and(|definition| shop.shows_inventory(definition))) {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        @let food_lot = food_lots.iter().find(|lot| lot.party_inventory_item_id == Some(item.id));
                        @let is_currency = definition.is_some_and(|definition| definition.kind == crate::spacetimedb::ItemKind::Currency);
                        @let sell_price = adventuresim_core::local_problem::adjust_price(adventuresim_core::strategic_economy::language_adjusted_sell_price(merchant_inventory_sell_price(definition, food_lot), trade_language), -problem_sell_penalty_bps);
                        @let target = target_quantity(party_targets, &item.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&item.item_id) data-party-inventory-id=(item.id) data-inventory-quantity=(item.quantity) data-target=(target) {
                            td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                            td class="inventory-item-name" { (item_name_with_quality(&item.item_id, definition)) @if !is_currency { (merchant_sell_controls(item.id, &item.item_id, sell_price, item.quantity, target)) } }
                            td class="inventory-count" { (quantity_target_control(item.quantity, target, &item.item_id, true)) }
                            td class="inventory-weight" { (merchant_inventory_weight(definition, food_lot)) }
                            td class="inventory-gold" { (sell_price) }
                        }
                    }
                    // Party purchases may spend pooled coin first and the active
                    // character's coin second. Show both funding sources as the
                    // same collapsed Coin row in this scope.
                    @for item in inventory.iter().filter(|item| items.iter().find(|definition| definition.id == item.item_id).is_some_and(|definition| definition.kind == crate::spacetimedb::ItemKind::Currency)) {
                        @let definition = items.iter().find(|definition| definition.id == item.item_id);
                        tr class="trade-inventory-row trade-row-player party-personal-currency" data-merchant-item=(&item.item_id) data-inventory-quantity=(item.qty) data-target="0" title="Personal coin available for party purchases" {
                            td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                            td class="inventory-item-name" { (item_name_with_quality(&item.item_id, definition)) }
                            td class="inventory-count" { (item.qty) }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (item_value(definition)) }
                        }
                    }
                    @for target in party_targets.iter().filter(|target| target.quantity > 0 && !pooled.iter().any(|item| item.item_id == target.item_id) && items.iter().find(|definition| definition.id == target.item_id).is_some_and(|definition| shop.shows_inventory(definition))) {
                        @let definition = items.iter().find(|definition| definition.id == target.item_id);
                        tr class="trade-inventory-row trade-row-player" data-merchant-item=(&target.item_id) data-inventory-quantity="0" data-target=(target.quantity) {
                            td class="inventory-item-type" { (item_type_icon(&target.item_id)) }
                            td class="inventory-item-name" { (item_name_with_quality(&target.item_id, definition)) }
                            td class="inventory-count" { (quantity_target_control(0, target.quantity, &target.item_id, true)) }
                            td class="inventory-weight" { (item_weight(definition)) }
                            td class="inventory-gold" { (item_value(definition)) }
                        }
                    }
                }))
                }, inventory_footer_controls("sell", "Sell surplus", "Sell everything"), party_encumbrance))
            }
            }
            }
        }
    };
    settlement_layout_with_session(
        title,
        &settlement.name,
        &settlement.id,
        &settlement.category,
        service_id,
        Some(&settlement.religion_id),
        Some(&settlement.economy),
        content,
        Some(&character.name),
    )
}

/// Two-sided transfer view for the equally owned party chest.
pub fn party_pool_page(
    location: &LocationView,
    character: &Character,
    inventory: &[InventoryItem],
    pooled: &[PartyInventoryItem],
    stake: u64,
    items: &[crate::spacetimedb::ItemDefinition],
    party_members: &[Character],
    equip: Option<&CharacterEquip>,
    personal_targets: &[InventoryQuantityTarget],
    party_targets: &[InventoryQuantityTarget],
    party_encumbrance: EncumbranceSummary,
    personal_encumbrance: EncumbranceSummary,
) -> Markup {
    let content = html! {
        aside class="left-sidebar" {
            (sidebar_section("Party inventory", html! {
                (encumbrance_inventory_rail(html! {
                    div class="party-stake-summary" {
                        span { "Your available stake" }
                        strong { (stake) " coin" }
                    }
                    p class="small-copy text-muted" { "Withdrawals use your stake. Personal coin automatically covers an indivisible item's shortfall." }
                    (trade_inventory_table("party-pool-left", InventoryColumnSet::All, true, false, false, html! {
                        @for item in pooled {
                            @let definition = items.iter().find(|definition| definition.id == item.item_id);
                            @let value = definition.and_then(|definition| definition.base_value).unwrap_or(0) as u64;
                            @let target = target_quantity(personal_targets, &item.item_id);
                            @let current = inventory.iter().find(|personal| personal.item_id == item.item_id).map_or(0, |personal| personal.qty);
                            @let item_name = item_display_name(&item.item_id);
                            tr class="trade-inventory-row" {
                                td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                                td class="inventory-item-name" {
                                    (item_name_with_quality(&item.item_id, definition))
                                span class="inventory-row-actions" { button type="button" class="trade-transfer trade-transfer-right" data-dynamic-transfer data-default-transfer-mode="one" data-pool-stage=(item.id) data-pool-direction="withdraw" data-transfer-mode="one" data-count=(item.quantity) data-current=(current) data-target=(target) data-label-one=(format!("Withdraw one {item_name}")) data-label-target=(format!("Withdraw {item_name} to target")) data-label-all=(format!("Withdraw all {item_name}")) title=(if value > stake { format!("Withdraw one {item_name}; {} personal coin required", value - stake) } else { format!("Withdraw one {item_name} using your stake") }) aria-label=(format!("Withdraw one {item_name}")) { (transfer_glyph(1)) } }
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
            (party_portrait_overlay(party_members, Some(character), &location.base_path(), None, false))
            (visual_stage("chest", "Party chest", "Shared supplies and each member's stake"))
            (settlement_chat_area("Party inventory", Some(character)))
        }
        aside class="right-sidebar" {
            (sidebar_section(&format!("{}'s inventory", character.name), html! {
                (encumbrance_inventory_rail(html! {
                    p class="small-copy text-muted" { "Add items at their objective coin value." }
                    (trade_inventory_table("party-pool-right", InventoryColumnSet::All, true, true, false, html! {
                        @for item in inventory {
                            @let definition = items.iter().find(|definition| definition.id == item.item_id);
                            @let equipped = equip.is_some_and(|equip| [equip.left_hand_item_id, equip.right_hand_item_id, equip.left_arm_armor_id, equip.right_arm_armor_id, equip.left_leg_armor_id, equip.right_leg_armor_id, equip.head_armor_id, equip.chest_armor_id, equip.stomach_armor_id].contains(&Some(item.id)));
                            @let target = target_quantity(party_targets, &item.item_id);
                            @let current = pooled.iter().find(|pooled| pooled.item_id == item.item_id).map_or(0, |pooled| pooled.quantity);
                            @let item_name = item_display_name(&item.item_id);
                            tr class="trade-inventory-row" {
                                td class="inventory-item-type" { (item_type_icon(&item.item_id)) }
                                td class="inventory-item-name" {
                                    (item_name_with_quality(&item.item_id, definition))
                                    span class="inventory-row-actions" {
                                        @if equipped {
                                            (disabled_transfer_button("left", "Equipped items cannot be deposited"))
                                        } @else {
                                            button type="button" class="trade-transfer trade-transfer-left" data-dynamic-transfer data-default-transfer-mode="one" data-pool-stage=(item.id) data-pool-direction="deposit" data-transfer-mode="one" data-count=(item.qty) data-current=(current) data-target=(target) data-label-one=(format!("Deposit one {item_name}")) data-label-target=(format!("Deposit {item_name} to target")) data-label-all=(format!("Deposit all {item_name}")) aria-label=(format!("Deposit one {item_name}")) title=(format!("Deposit one {item_name}")) { (transfer_glyph(1)) }
                                        }
                                    }
                                }
                                td class="inventory-count" { (quantity_target_control(item.qty, target_quantity(personal_targets, &item.item_id), &item.item_id, false)) }
                                td class="inventory-equipped" { (equipment_checkbox(item, definition, equipped)) }
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

pub(super) fn item_weight(item: Option<&crate::spacetimedb::ItemDefinition>) -> String {
    item.map_or_else(|| "—".to_owned(), |item| weight_display(item.weight))
}

pub(super) fn merchant_inventory_weight(
    definition: Option<&crate::spacetimedb::ItemDefinition>,
    food_lot: Option<&FoodLot>,
) -> String {
    food_lot.map_or_else(
        || item_weight(definition),
        |lot| weight_display(lot.mass_kg),
    )
}

pub(super) fn merchant_inventory_sell_price(
    definition: Option<&crate::spacetimedb::ItemDefinition>,
    food_lot: Option<&FoodLot>,
) -> u32 {
    food_lot.map_or_else(
        || {
            definition.map_or(0, |definition| {
                adventuresim_core::strategic_economy::merchant_sell_price(
                    definition.base_value.unwrap_or(1),
                )
            })
        },
        |lot| {
            adventuresim_core::strategic_economy::merchant_sell_food_lot_value(lot.total_value)
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(0)
        },
    )
}

pub(super) fn encumbrance_inventory_rail(
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
    let weight_text = format!("{:.1} / {:.1} kg", summary.burden_kg, summary.capacity_kg);
    let penalty_text = format!("-{penalty_percent:.1}%");
    let accessible_text = format!(
        "Weight {:.1} / {:.1} kilograms; Penalty -{penalty_percent:.1}%",
        summary.burden_kg, summary.capacity_kg
    );
    html! {
        div class="encumbrance" {
            div class="encumbrance-values" aria-hidden="true" {
                span class="encumbrance-weight" { (weight_text) }
                span class="encumbrance-penalty" { (penalty_text) }
            }
            div class="encumbrance-visual" {
                div class="encumbrance-meter"
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

pub(super) fn equipment_checkbox(
    inventory: &InventoryItem,
    definition: Option<&crate::spacetimedb::ItemDefinition>,
    equipped: bool,
) -> Markup {
    let equippable = definition.is_some_and(|definition| {
        definition.slot != ItemSlot::None
            || definition.kind == crate::spacetimedb::ItemKind::Medication
    });
    let item_name = item_display_name(&inventory.item_id);
    let label = if equipped {
        format!("Unequip {item_name}")
    } else {
        format!("Equip {item_name}")
    };
    html! {
        input type="checkbox"
            checked[equipped]
            disabled[!equippable]
            data-equipment-toggle
            data-inventory-item-id=(inventory.id)
            aria-label=(label)
            title=(if equippable { "Equip or unequip this item" } else { "This item cannot be equipped" });
    }
}

pub(super) fn item_value(item: Option<&crate::spacetimedb::ItemDefinition>) -> String {
    item.and_then(|item| item.base_value)
        .map_or_else(|| "—".to_owned(), |value| value.to_string())
}

pub(in crate::templates) fn item_name_with_quality(
    item_id: &str,
    definition: Option<&crate::spacetimedb::ItemDefinition>,
) -> Markup {
    let currency_name = adventuresim_core::strategic_currency::currency_name(item_id);
    if let Some(currency_name) = currency_name {
        html! {
            span class="inventory-item-label" data-item-name="Coin"
                data-item-kind="currency" data-currency-name=(currency_name) { "Coin" }
        }
    } else {
        let display_name = item_display_name(item_id);
        item_name_with_display(item_id, &display_name, definition)
    }
}

pub(super) fn item_name_with_display(
    item_id: &str,
    display_name: &str,
    definition: Option<&crate::spacetimedb::ItemDefinition>,
) -> Markup {
    let alcohol_group = definition
        .filter(|item| item.alcohol_serving_ml > 0)
        .map(|_| "alcohol");
    let quality = definition
        .filter(|item| {
            matches!(
                item.kind,
                crate::spacetimedb::ItemKind::Weapon
                    | crate::spacetimedb::ItemKind::Armor
                    | crate::spacetimedb::ItemKind::Shield
            )
        })
        .map(|item| item.quality.clamp(1, 5));
    let label = quality.map(|quality| match quality {
        1 => "Quality 1",
        2 => "Quality 2",
        3 => "Quality 3 — munition grade",
        4 => "Quality 4 — knightly commission",
        5 => "Quality 5 — royal or heroic commission",
        _ => unreachable!(),
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
            data-item-kind=[definition.map(|item| format!("{:?}", item.kind).to_ascii_lowercase())]
            data-item-group=[alcohol_group]
            data-group-name=[alcohol_group.map(|_| "Alcohol")]
            data-food-lot=[adventuresim_core::food::definition(item_id).map(|_| "true")]
            data-stat-accuracy=[definition.map(|item| weight_display(item.accuracy))]
            data-stat-reach=[definition.map(|item| weight_display(item.reach))]
            data-stat-penetration=[definition.map(|item| weight_display(item.penetration))]
            data-stat-damage=[damage_types]
            data-stat-block=[definition.map(|item| weight_display(item.block))]
            data-stat-coverage=[definition.map(|item| weight_display(item.coverage))]
            data-stat-resistance=[definition.map(|item| weight_display(item.resistance))]
            data-stat-padding=[definition.map(|item| weight_display(item.padding))]
            data-stat-flexibility=[definition.map(|item| weight_display(item.flexibility))]
            data-stat-range-of-motion=[definition.map(|item| weight_display(item.range_of_motion))]
            data-detail-slot=[definition.map(|item| format!("{:?}", item.slot))]
            data-detail-balance=[definition.map(|item| weight_display(item.balance))]
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

pub(super) fn trade_inventory_table(
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

pub(crate) fn transfer_glyph(count: usize) -> Markup {
    html! { span class=(format!("inventory-transfer-glyph arrows-{count}")) aria-hidden="true" { @for _ in 0..count { i {} } } }
}

pub(super) fn disabled_transfer_button(direction: &str, explanation: &str) -> Markup {
    html! {
        button type="button" class=(format!("trade-transfer trade-transfer-{direction}")) disabled title=(explanation) aria-label=(explanation) { (transfer_glyph(1)) }
    }
}

pub(super) fn merchant_buy_controls(
    item_id: &str,
    price: u32,
    target: u32,
    available: u32,
) -> Markup {
    let item_name = item_display_name(item_id);
    html! { span class="inventory-row-actions" {
        button type="button" class="trade-transfer trade-transfer-right" data-dynamic-transfer data-default-transfer-mode="one" data-merchant-buy=(item_id) data-merchant-buy-price=(price) data-transfer-mode="one" data-target=(target) data-count=(available) data-label-one=(format!("Buy one {item_name}")) data-label-target=(format!("Buy {item_name} to target")) data-label-all=(format!("Buy all {item_name}")) aria-label=(format!("Buy one {item_name}")) title=(format!("Buy one {item_name}")) { (transfer_glyph(1)) }
    } }
}

pub(super) fn merchant_sell_controls(
    id: u64,
    item_id: &str,
    price: u32,
    quantity: u32,
    target: u32,
) -> Markup {
    let item_name = item_display_name(item_id);
    html! { span class="inventory-row-actions" {
        button type="button" class="trade-transfer trade-transfer-left" data-dynamic-transfer data-default-transfer-mode="one" data-merchant-sell=(id) data-item-name=(item_id) data-merchant-sell-price=(price) data-transfer-mode="one" data-count=(quantity) data-target=(target) data-label-one=(format!("Sell one {item_name}")) data-label-target=(format!("Sell surplus {item_name}")) data-label-all=(format!("Sell all {item_name}")) aria-label=(format!("Sell one {item_name}")) title=(format!("Sell one {item_name}")) { (transfer_glyph(1)) }
    } }
}

pub(super) fn merchant_sell_repair_controls(
    id: u64,
    item_id: &str,
    price: u32,
    quantity: u32,
    target: u32,
    can_sell: bool,
    repair: Option<Markup>,
) -> Markup {
    let has_repair = repair.is_some();
    let item_name = item_display_name(item_id);
    html! { div class=(if has_repair { "inventory-row-actions smith-player-actions" } else { "inventory-row-actions" }) {
        @if let Some(repair) = repair { (repair) }
        @if can_sell {
            button type="button" class="trade-transfer trade-transfer-left" data-dynamic-transfer data-default-transfer-mode="one" data-merchant-sell=(id) data-item-name=(item_id) data-merchant-sell-price=(price) data-transfer-mode="one" data-count=(quantity) data-target=(target) data-label-one=(format!("Sell one {item_name}")) data-label-target=(format!("Sell surplus {item_name}")) data-label-all=(format!("Sell all {item_name}")) aria-label=(format!("Sell one {item_name}")) title=(format!("Sell one {item_name}")) { (transfer_glyph(1)) }
        } @else if has_repair {
            (disabled_transfer_button("left", "Equipped items cannot be sold"))
        }
    } }
}

pub(super) fn condition_bar(
    condition: Option<&crate::spacetimedb::ItemCondition>,
    repair_skill: Option<u8>,
) -> Markup {
    let bins = condition.map(|value| value.bins()).unwrap_or([0.0; 5]);
    let total = bins.iter().sum::<f32>().clamp(0.0, 1.0);
    let green = (1.0 - total).max(0.0);
    let label = if total <= f32::EPSILON {
        "Full durability".to_string()
    } else if repair_skill
        .is_some_and(|skill| bins.iter().take(skill.min(5) as usize).sum::<f32>() > f32::EPSILON)
    {
        "Damaged; the flashing portion can be repaired by this smith".to_string()
    } else {
        "Damaged beyond this smith's skill".to_string()
    };
    html! {
        span class="condition-bar" data-sort-value=(weight_display(green)) title=(&label) aria-label=(&label) {
            span class="condition-green" style=(format!("width:{}%", green * 100.0)) {}
            @for (index, amount) in bins.iter().enumerate() {
                @let repairable = repair_skill.is_some_and(|skill| index < skill.min(5) as usize);
                span class=(format!("condition-tier-{}{}", index + 1, if repairable { " condition-repairable" } else { "" })) style=(format!("width:{}%", amount.clamp(0.0, 1.0) * 100.0)) {}
            }
        }
    }
}

pub(super) fn completed_repair_condition_bar(
    condition: Option<&crate::spacetimedb::ItemCondition>,
    smith_skill: u8,
) -> Markup {
    let Some(condition) = condition else {
        return condition_bar(None, None);
    };
    let mut repaired = condition.clone();
    let mut bins = [
        &mut repaired.tier_1,
        &mut repaired.tier_2,
        &mut repaired.tier_3,
        &mut repaired.tier_4,
        &mut repaired.tier_5,
    ];
    for amount in bins.iter_mut().take(smith_skill.min(5) as usize) {
        **amount = 0.0;
    }
    condition_bar(Some(&repaired), None)
}

pub(super) fn repair_all_control(settlement: &Settlement, service_id: &str) -> Markup {
    html! {
        form class="repair-all-form inventory-footer-repair" action=(format!("/settlements/{}/{}/repair-all", settlement.id, service_id)) method="post" {
            button type="submit" class="repair-all-button" title="Entrust all eligible items for repair" aria-label="Repair all eligible items" {
                span class="repair-action-icon" aria-hidden="true" {}
            }
        }
    }
}

pub(super) fn repair_submit_control(
    settlement: &Settlement,
    service_id: &str,
    inventory_item_id: u64,
    condition: Option<&crate::spacetimedb::ItemCondition>,
    skill: u8,
) -> Markup {
    let total = condition.map_or(0.0, |value| value.total());
    let repairable = condition.map_or(0.0, |value| value.repairable(skill));
    let residual = condition.map_or(0.0, |value| value.residual(skill));
    let disabled = total <= f32::EPSILON || repairable <= f32::EPSILON;
    let explanation = if total <= f32::EPSILON {
        "Item is already in full condition".to_string()
    } else if repairable <= f32::EPSILON {
        format!("All damage requires Smithing above this smith's level {skill}")
    } else if residual > f32::EPSILON {
        "Repair all damage within this smith's skill; harder damage will remain".to_string()
    } else {
        format!("Repair all damage (smith level {skill})")
    };
    html! {
        form class="row-repair-form" action=(format!("/settlements/{}/{}/repair", settlement.id, service_id)) method="post" {
            input type="hidden" name="inventory_item_id" value=(inventory_item_id);
            @if disabled {
                span class="disabled-repair-explanation" tabindex="0" title=(&explanation) aria-label=(&explanation) {
                    button type="submit" class="repair-item-button" disabled { span class="repair-action-icon" aria-hidden="true" {} }
                }
            } @else {
                button type="submit" class="repair-item-button" title=(&explanation) aria-label=(&explanation) { span class="repair-action-icon" aria-hidden="true" {} }
            }
        }
    }
}

pub(super) fn repair_custody_panel(
    settlement: &Settlement,
    shop: MerchantShop,
    orders: &[crate::spacetimedb::RepairOrder],
    conditions: &[crate::spacetimedb::ItemCondition],
    items: &[crate::spacetimedb::ItemDefinition],
    now: u64,
    smith_skill: u8,
) -> Markup {
    let service_id = shop.service_id();
    let mut matching: Vec<_> = orders
        .iter()
        .filter(|order| {
            order.settlement_id == settlement.id
                && items
                    .iter()
                    .find(|item| item.id == order.item_id)
                    .is_some_and(|item| shop.stocks(item))
        })
        .collect();
    matching.sort_by_key(|order| (order.submitted_at_minutes, order.id));
    html! {
        section class="repair-custody-panel" aria-label="Items entrusted for repair" {
            header class="repair-custody-header" {
                h3 { @if matches!(shop, MerchantShop::Clothing) { "In the tailor's care" } @else { "In the smith's care" } }
                @let craft = if matches!(shop, MerchantShop::Clothing) { "Tailoring" } else { "Smithing" };
                span class="repair-custody-skill" title=(format!("{craft} {smith_skill}")) {
                    (stat_icon(craft, "skills", if craft == "Tailoring" { "sewing-needle" } else { "smithing" }, false))
                    (skill_rank_bar(f32::from(smith_skill), f32::from(smith_skill), &format!("{craft} {smith_skill}"), SkillRankBarOptions::default()))
                }
            }
            div class="repair-custody-scroll" {
                @if matching.is_empty() { p class="text-muted small-copy" { "No items entrusted." } }
                div class="repair-custody-list" {
                    table class="trade-inventory-table repair-custody-table" {
                        colgroup {
                            col class="inventory-column-type";
                            col class="inventory-column-item";
                            col class="inventory-column-durability";
                            col class="repair-column-eta";
                            col class="inventory-column-gold";
                            col class="inventory-column-actions";
                        }
                        thead { tr {
                            (item_type_header())
                            th scope="col" class="inventory-column-item" { "Item" }
                            th scope="col" class="inventory-column-durability" { "Durability" }
                            th scope="col" class="repair-column-eta" { "ETA" }
                            th scope="col" class="inventory-column-gold" title="Full repair cost (Currency)" { (currency_header("Full repair cost in Currency")) }
                            th class="inventory-actions-header" aria-label="Repair retrieval actions" {
                                div class="inventory-footer-actions repair-custody-header-actions" {
                                    form class="repair-retrieve-all-form" data-repair-retrieve-form data-bulk-action=(format!("/settlements/{}/{}/repairs/retrieve", settlement.id, service_id)) action=(format!("/settlements/{}/{}/repairs/retrieve", settlement.id, service_id)) method="post" {
                                        input type="hidden" name="limit" value="2";
                                        button type="submit" class="trade-transfer trade-transfer-right inventory-footer-transfer repair-retrieve-all" data-dynamic-transfer data-default-transfer-mode="target" data-transfer-mode="target" data-label-target="Retrieve up to two completed repairs" data-label-all="Retrieve all completed repairs" title="Retrieve up to two completed repairs" aria-label="Retrieve up to two completed repairs" { (transfer_glyph(2)) }
                                    }
                                }
                            }
                        } }
                        tbody {
                        @for order in matching {
                            @let condition = conditions.iter().find(|condition| condition.inventory_item_id == order.inventory_item_id);
                            @let definition = items.iter().find(|item| item.id == order.item_id);
                            @let ready = now >= order.ready_at_minutes;
                            @let remaining = order.ready_at_minutes.saturating_sub(now);
                            tr class="trade-inventory-row trade-row-merchant repair-order-row" {
                                td class="inventory-item-type" { (item_type_icon(&order.item_id)) }
                                td class="inventory-item-name" { (item_name_with_quality(&order.item_id, definition)) }
                                td class="inventory-durability" {
                                    @if ready { (completed_repair_condition_bar(condition, order.smith_skill)) }
                                    @else { (condition_bar(condition, Some(order.smith_skill))) }
                                }
                                td class="repair-column-eta" { @if ready { "Ready" } @else { (format!("{}h {}m", remaining / 60, remaining % 60)) } }
                                td class="inventory-gold" title="Quoted full-job cost, paid on retrieval" { (order.quoted_cost) }
                                td class="inventory-actions-cell" aria-label="Item actions" {
                                    span class="inventory-row-actions repair-retrieve-actions" {
                                        form data-repair-retrieve-form data-single-action=(format!("/settlements/{}/{}/repairs/{}/retrieve", settlement.id, service_id, order.id)) data-bulk-action=(format!("/settlements/{}/{}/repairs/retrieve", settlement.id, service_id)) action=(format!("/settlements/{}/{}/repairs/{}/retrieve", settlement.id, service_id, order.id)) method="post" {
                                            input type="hidden" name="item_id" value=(&order.item_id);
                                            input type="hidden" name="limit" value="1" disabled;
                                            button type="submit" class="trade-transfer trade-transfer-right" data-dynamic-transfer data-default-transfer-mode="one" data-transfer-mode="one" data-label-one="Retrieve this completed item" data-label-target="Retrieve up to two completed matching items" data-label-all="Retrieve all completed matching items" disabled[!ready] title=(if ready { "Retrieve this completed item" } else { "Repair is still underway" }) aria-label="Retrieve this completed item" { (transfer_glyph(1)) }
                                        }
                                    }
                                }
                            }
                        }
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn inventory_footer_controls(
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
pub(super) fn trade_inventory_table_header(
    show_equipped: bool,
    condition_header: Option<Markup>,
) -> Markup {
    html! { thead { tr {
        (item_type_header())
        th scope="col" class="inventory-column-item" { "Item" }
        th scope="col" class="inventory-column-count" { "#" }
        @if show_equipped { th scope="col" class="inventory-column-equipped" title="Equipped" { (game_icon("Equipped", "check-mark")) } }
        @if let Some(condition_header) = condition_header { th scope="col" class="inventory-column-durability" { (condition_header) } }
        th scope="col" class="inventory-column-weight" title="Weight" { (game_icon("Weight", "weight")) }
        th scope="col" class="inventory-column-gold" title="Currency" { (currency_header("Currency")) }
    } } }
}
