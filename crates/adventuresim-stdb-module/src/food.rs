//! Authoritative measured food lots and immediate free-form cooking.

use adventuresim_core::{
    disease::{self, DiseaseId},
    food,
    prelude::{PlayerSkills, Skill},
};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::{
    character::{character, character_attributes, character_limbs, character_skills},
    condition::{character_needs, initialize_character_condition},
    container_liquid,
    disease::{InfectionEpisodeRow, infection_episode},
    inventory_containment, inventory_item, inventory_item_amount, inventory_object,
    party_item_amount,
    strategic::{
        PartyInventoryItem, party_authority, party_inventory_item, party_journey_authority,
        settlement,
    },
    time::character_time,
};

#[derive(Clone, Copy, Debug, PartialEq, SpacetimeType)]
pub enum FoodPreparation {
    Raw,
    Preserved,
    PanFried,
    Stewed,
    Roasted,
    Baked,
}

#[derive(Clone, Copy, Debug, PartialEq, SpacetimeType)]
pub enum CookingMethod {
    PanFry,
    Stew,
    Roast,
    Bake,
}

impl CookingMethod {
    fn core(self) -> food::CookingMethod {
        match self {
            Self::PanFry => food::CookingMethod::PanFry,
            Self::Stew => food::CookingMethod::Stew,
            Self::Roast => food::CookingMethod::Roast,
            Self::Bake => food::CookingMethod::Bake,
        }
    }
    fn preparation(self) -> FoodPreparation {
        match self {
            Self::PanFry => FoodPreparation::PanFried,
            Self::Stew => FoodPreparation::Stewed,
            Self::Roast => FoodPreparation::Roasted,
            Self::Bake => FoodPreparation::Baked,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::PanFry => "Pan-fried",
            Self::Stew => "Stewed",
            Self::Roast => "Roasted",
            Self::Bake => "Baked",
        }
    }
}

/// Public, inspectable description of one non-fungible inventory batch.
#[derive(Clone, Debug)]
#[table(accessor = food_lot, public)]
pub struct FoodLot {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub inventory_item_id: Option<u64>,
    pub party_inventory_item_id: Option<u64>,
    pub display_name: String,
    pub preparation: FoodPreparation,
    pub ingredient_item_ids: Vec<String>,
    /// Fractional source-unit provenance is conserved when a lot is partly eaten.
    pub ingredient_quantities: Vec<f32>,
    pub salty_kg: f32,
    pub spicy_kg: f32,
    pub sweet_kg: f32,
    pub sour_kg: f32,
    pub savory_kg: f32,
    /// Durable quality tier shared with item craftsmanship name colors.
    pub quality: u8,
    pub mass_kg: f32,
    pub nutrition_kcal: f32,
    pub total_value: f32,
    pub created_at_minute: u64,
}

/// Hidden microbial state. The browser can inspect provenance, never pathogen load.
#[derive(Clone, Debug)]
#[table(accessor = food_contamination)]
pub struct FoodContamination {
    #[primary_key]
    pub food_lot_id: u64,
    pub concentration_anchor: f32,
    pub growth_per_hour: f32,
    pub anchor_minute: u64,
}

/// Private character-owned state for one exact physical fireplace context.
/// The portrait is environmental/shared, but neither its tool nor dish leaks
/// across player timelines.
#[derive(Clone, Debug)]
#[table(accessor = fireplace_station)]
pub struct FireplaceStation {
    #[primary_key]
    pub key: String,
    #[index(btree)]
    pub character_id: u64,
    pub context_key: String,
    pub instrument_item_id: Option<String>,
    /// Stable root object for a placed cooking vessel. `None` is the loose
    /// spit-roast lane or a legacy empty station.
    pub instrument_object_id: Option<u64>,
    /// `personal` or `party`; retained so removing/replacing returns custody to
    /// the source that installed the tool.
    pub instrument_source: Option<String>,
    pub instrument_party_id: Option<String>,
}

/// Irreversibly consolidated meal escrow. Hidden microbial load stays in this
/// private table and is copied to FoodContamination only on retrieval.
#[derive(Clone, Debug)]
#[table(accessor = fireplace_dish)]
pub struct FireplaceDish {
    #[primary_key]
    pub station_key: String,
    #[index(btree)]
    pub character_id: u64,
    pub contributor_name: String,
    pub method: CookingMethod,
    pub cooking_check: f32,
    pub started_at_minute: u64,
    pub target_minutes: u32,
    pub display_name: String,
    pub ingredient_item_ids: Vec<String>,
    pub ingredient_quantities: Vec<f32>,
    pub salty_kg: f32,
    pub spicy_kg: f32,
    pub sweet_kg: f32,
    pub sour_kg: f32,
    pub savory_kg: f32,
    pub ready_quality: u8,
    pub mass_kg: f32,
    pub raw_nutrition_kcal: f32,
    pub ready_nutrition_retention: f32,
    pub ingredient_value: f32,
    pub raw_contamination: f32,
    pub raw_growth_per_hour: f32,
    pub cooked_growth_per_hour: f32,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendFireplaceStation {
    pub key: String,
    pub character_id: u64,
    pub context_key: String,
    pub instrument_item_id: Option<String>,
    pub instrument_object_id: Option<u64>,
    pub instrument_source: Option<String>,
}

#[view(accessor = backend_fireplace_stations, public)]
pub fn backend_fireplace_stations(ctx: &ViewContext) -> Vec<BackendFireplaceStation> {
    if !crate::strategic::strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .fireplace_station()
        .character_id()
        .filter(0u64..)
        .map(|row| BackendFireplaceStation {
            key: row.key,
            character_id: row.character_id,
            context_key: row.context_key,
            instrument_item_id: row.instrument_item_id,
            instrument_object_id: row.instrument_object_id,
            instrument_source: row.instrument_source,
        })
        .collect()
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendFireplaceDish {
    pub station_key: String,
    pub character_id: u64,
    pub contributor_name: String,
    pub method: CookingMethod,
    pub started_at_minute: u64,
    pub target_minutes: u32,
    pub display_name: String,
}

#[view(accessor = backend_fireplace_dishes, public)]
pub fn backend_fireplace_dishes(ctx: &ViewContext) -> Vec<BackendFireplaceDish> {
    if !crate::strategic::strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .fireplace_dish()
        .character_id()
        .filter(0u64..)
        .map(|row| BackendFireplaceDish {
            station_key: row.station_key,
            character_id: row.character_id,
            contributor_name: row.contributor_name,
            method: row.method,
            started_at_minute: row.started_at_minute,
            target_minutes: row.target_minutes,
            display_name: row.display_name,
        })
        .collect()
}

fn station_key(character_id: u64, context_key: &str) -> String {
    format!("{character_id}|{context_key}")
}

pub(crate) fn require_clear_current_camp_fireplace(
    ctx: &ReducerContext,
    party_id: &str,
    departure_minute: u64,
    movement_minute: u64,
) -> Result<(), String> {
    let context = format!("camp|{party_id}|{departure_minute}|{movement_minute}");
    let occupied = ctx
        .db
        .fireplace_station()
        .iter()
        .any(|row| row.context_key == context && row.instrument_item_id.is_some())
        || ctx
            .db
            .fireplace_dish()
            .iter()
            .any(|row| row.station_key.ends_with(&format!("|{context}")));
    if occupied {
        Err("Retrieve every dish and remove every cooking instrument before breaking camp".into())
    } else {
        Ok(())
    }
}

pub(crate) fn require_members_clear_current_camp_fireplace(
    ctx: &ReducerContext,
    party_id: &str,
    character_ids: &[u64],
) -> Result<(), String> {
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(party_id.to_string())
        .ok_or("Party not found")?;
    if party.current_settlement_id.is_some()
        || party.current_case_site_id.is_some()
        || party.camp_destination.is_none()
    {
        return Ok(());
    }
    let journey = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(party_id.to_string())
        .ok_or("Journey camp not found")?;
    if !journey
        .camp_stop_minutes
        .contains(&journey.completed_minutes)
    {
        return Ok(());
    }
    let context = format!(
        "camp|{party_id}|{}|{}",
        journey.departure_minute, journey.completed_minutes
    );
    let context_suffix = format!("|{context}");
    let occupied = character_ids.iter().copied().any(|character_id| {
        ctx.db
            .fireplace_station()
            .character_id()
            .filter(character_id)
            .any(|row| row.context_key == context && row.instrument_item_id.is_some())
            || ctx
                .db
                .fireplace_dish()
                .character_id()
                .filter(character_id)
                .any(|row| row.station_key.ends_with(&context_suffix))
    });
    if occupied {
        Err("Retrieve this member's dish and remove their cooking instrument before they leave the camp party".into())
    } else {
        Ok(())
    }
}

/// Resolves only the dead character's private station rows. Unretrieved food is
/// abandoned. Tools return to their exact recorded source when it still exists;
/// otherwise they move to the dead character's personal estate inventory. If
/// even that character row is absent, the tool is abandoned with the station.
/// A stale party reference can therefore never lock travel or leak another
/// player's dish.
pub(crate) fn cleanup_fireplace_custody_for_death(ctx: &ReducerContext, character_id: u64) {
    let personal_estate_exists = ctx.db.character().id().find(character_id).is_some();
    for dish in ctx
        .db
        .fireplace_dish()
        .character_id()
        .filter(character_id)
        .collect::<Vec<_>>()
    {
        ctx.db
            .fireplace_dish()
            .station_key()
            .delete(dish.station_key);
    }
    for station in ctx
        .db
        .fireplace_station()
        .character_id()
        .filter(character_id)
        .collect::<Vec<_>>()
    {
        if let Some(item_id) = station.instrument_item_id.as_deref() {
            if let Some(object_id) = station.instrument_object_id {
                let party_destination = station.instrument_source.as_deref() == Some("party")
                    && station
                        .instrument_party_id
                        .as_deref()
                        .is_some_and(|party_id| {
                            ctx.db
                                .party_authority()
                                .id()
                                .find(party_id.to_string())
                                .is_some()
                        });
                let destination = if party_destination {
                    station
                        .instrument_party_id
                        .as_deref()
                        .map(|party_id| ("party", party_id.to_string()))
                } else if personal_estate_exists {
                    Some(("personal", character_id.to_string()))
                } else {
                    None
                };
                if let Some((kind, owner)) = destination
                    && let Some(mut object) = ctx.db.inventory_object().id().find(object_id)
                {
                    let row_id = if kind == "party" {
                        ctx.db
                            .party_inventory_item()
                            .insert(PartyInventoryItem {
                                id: 0,
                                party_id: owner.clone(),
                                item_id: item_id.into(),
                                quantity: 1,
                            })
                            .id
                    } else {
                        ctx.db
                            .inventory_item()
                            .insert(crate::InventoryItem {
                                id: 0,
                                character_id,
                                item_id: item_id.into(),
                                quantity: 1,
                            })
                            .id
                    };
                    object.location_kind = kind.into();
                    object.location_owner = owner.clone();
                    object.inventory_row_id = row_id;
                    ctx.db.inventory_object().id().update(object);
                    let _ =
                        crate::inventory_container::rehome_subtree(ctx, object_id, kind, &owner);
                }
                ctx.db.fireplace_station().key().delete(station.key);
                continue;
            }
            let returned_to_exact_party = station.instrument_source.as_deref() == Some("party")
                && station
                    .instrument_party_id
                    .as_deref()
                    .is_some_and(|party_id| {
                        ctx.db
                            .party_authority()
                            .id()
                            .find(party_id.to_string())
                            .is_some()
                            && crate::strategic::add_to_party_inventory_checked(
                                ctx, party_id, item_id, 1,
                            )
                            .is_ok()
                    });
            if !returned_to_exact_party && personal_estate_exists {
                // Personal-origin tools and any invalid exact-party return become
                // part of the dead character's estate inventory.
                ctx.db.inventory_item().insert(crate::InventoryItem {
                    id: 0,
                    character_id,
                    item_id: item_id.into(),
                    quantity: 1,
                });
            }
        }
        ctx.db.fireplace_station().key().delete(station.key);
    }
}

fn validate_fireplace_context(
    ctx: &ReducerContext,
    actor: &crate::Character,
    context_key: &str,
) -> Result<(), String> {
    let parts = context_key.split('|').collect::<Vec<_>>();
    match parts.as_slice() {
        ["settlement", settlement_id, building]
            if !building.is_empty() && !matches!(*building, "public-square" | "map") =>
        {
            if actor.current_settlement_id.as_deref() != Some(*settlement_id) {
                return Err("The character is not at this settlement fireplace".into());
            }
            let settlement = ctx
                .db
                .settlement()
                .id()
                .find((*settlement_id).to_string())
                .ok_or("Settlement not found")?;
            let standard_available = match *building {
                "residences" => Some(true),
                "keep" => Some(matches!(
                    settlement.category,
                    crate::strategic::SettlementCategory::Town
                        | crate::strategic::SettlementCategory::City
                        | crate::strategic::SettlementCategory::Capital
                )),
                "market" => Some(
                    adventuresim_core::organization::service_npc_location_available(
                        &settlement.economy,
                        "merchants",
                    ),
                ),
                "forge" => Some(
                    adventuresim_core::organization::service_npc_location_available(
                        &settlement.economy,
                        "weapons",
                    ),
                ),
                "armoury" => Some(
                    adventuresim_core::organization::service_npc_location_available(
                        &settlement.economy,
                        "armor",
                    ),
                ),
                "tailor" => Some(
                    adventuresim_core::organization::service_npc_location_available(
                        &settlement.economy,
                        "clothing",
                    ),
                ),
                "herbalist" => Some(
                    adventuresim_core::organization::service_npc_location_available(
                        &settlement.economy,
                        "herbalist",
                    ),
                ),
                "inn" => Some(
                    adventuresim_core::organization::service_npc_location_available(
                        &settlement.economy,
                        "inn",
                    ),
                ),
                "church" => Some(
                    adventuresim_core::organization::service_npc_location_available(
                        &settlement.economy,
                        "religion",
                    ),
                ),
                "bookstore" => Some(
                    adventuresim_core::organization::service_npc_location_available(
                        &settlement.economy,
                        "books",
                    ),
                ),
                _ => None,
            };
            let available = standard_available.unwrap_or_else(|| {
                adventuresim_core::organization::organization_chapter_at(settlement_id, building)
                    .is_some_and(|(organization, chapter)| {
                        adventuresim_core::organization::chapter_has_standalone_building(
                            organization,
                            chapter,
                            &settlement.economy,
                        )
                    })
            });
            if !available {
                return Err("This settlement building has no fireplace".into());
            }
            Ok(())
        }
        ["camp", party_id, departure, movement] => {
            let departure = departure
                .parse::<u64>()
                .map_err(|_| "Invalid camp fireplace")?;
            let movement = movement
                .parse::<u64>()
                .map_err(|_| "Invalid camp fireplace")?;
            if actor.party_id.as_deref() != Some(*party_id) {
                return Err("The character is not in this camp's party".into());
            }
            let party = ctx
                .db
                .party_authority()
                .id()
                .find((*party_id).to_string())
                .ok_or("Party not found")?;
            let journey = ctx
                .db
                .party_journey_authority()
                .party_id()
                .find((*party_id).to_string())
                .ok_or("Journey camp not found")?;
            if party.current_settlement_id.is_some()
                || party.current_case_site_id.is_some()
                || party.camp_destination.is_none()
                || journey.departure_minute != departure
                || journey.completed_minutes != movement
                || !journey.camp_stop_minutes.contains(&movement)
            {
                return Err("This is not the party's current journey camp".into());
            }
            Ok(())
        }
        _ => Err("Invalid fireplace context".into()),
    }
}

fn fireplace_station_for(
    ctx: &ReducerContext,
    character_id: u64,
    context_key: &str,
) -> FireplaceStation {
    let key = station_key(character_id, context_key);
    ctx.db
        .fireplace_station()
        .key()
        .find(key.clone())
        .unwrap_or(FireplaceStation {
            key,
            character_id,
            context_key: context_key.into(),
            instrument_item_id: None,
            instrument_object_id: None,
            instrument_source: None,
            instrument_party_id: None,
        })
}

fn method_for_instrument(item_id: Option<&str>) -> Result<CookingMethod, String> {
    match item_id {
        None => Ok(CookingMethod::Roast),
        Some("cooking_pan") => Ok(CookingMethod::PanFry),
        Some("cooking_pot") => Ok(CookingMethod::Stew),
        Some("portable_oven") => Ok(CookingMethod::Bake),
        _ => Err("That item is not a cooking instrument".into()),
    }
}

fn return_installed_tool(
    ctx: &ReducerContext,
    actor: &crate::Character,
    station: &FireplaceStation,
) -> Result<(), String> {
    let Some(item_id) = station.instrument_item_id.as_deref() else {
        return Ok(());
    };
    match station.instrument_source.as_deref() {
        Some("personal") => {
            ctx.db.inventory_item().insert(crate::InventoryItem {
                id: 0,
                character_id: actor.id,
                item_id: item_id.into(),
                quantity: 1,
            });
            Ok(())
        }
        Some("party") => {
            let party_id = station.instrument_party_id.as_deref().ok_or("The original party inventory is no longer available; the instrument remains installed")?;
            if ctx
                .db
                .party_authority()
                .id()
                .find(party_id.to_string())
                .is_none()
            {
                return Err("The original party inventory is no longer available; the instrument remains installed".into());
            }
            crate::strategic::add_to_party_inventory_checked(ctx, party_id, item_id, 1)
                .map_err(|_| "The original party inventory is no longer available; the instrument remains installed".to_string())
        }
        _ => Err("The instrument's original inventory is unknown; it remains installed".into()),
    }
}

#[reducer]
pub fn set_fireplace_instrument(
    ctx: &ReducerContext,
    character_id: u64,
    context_key: String,
    inventory_scope: String,
    inventory_item_id: Option<u64>,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    let actor = crate::character::require_living_character(ctx, character_id)?;
    if actor.in_server {
        return Err("Cooking is unavailable during a tactical encounter".into());
    }
    validate_fireplace_context(ctx, &actor, &context_key)?;
    if inventory_item_id.is_some() {
        return Err("Cooking tools must be placed as containers over the fire".into());
    }
    let mut station = fireplace_station_for(ctx, character_id, &context_key);
    if ctx
        .db
        .fireplace_dish()
        .station_key()
        .find(station.key.clone())
        .is_some()
    {
        return Err("Retrieve the current dish before changing instruments".into());
    }
    let replacement_party_id = if inventory_item_id.is_some() && inventory_scope == "party" {
        Some(
            actor
                .party_id
                .clone()
                .ok_or("Character has no party inventory")?,
        )
    } else {
        None
    };
    let replacement = if let Some(id) = inventory_item_id {
        let item_id = match inventory_scope.as_str() {
            "personal" => {
                let mut row = ctx
                    .db
                    .inventory_item()
                    .id()
                    .find(id)
                    .ok_or("Instrument not found")?;
                if row.character_id != character_id
                    || crate::character::wearable_is_equipped(ctx, id)
                {
                    return Err("Equipped or foreign items cannot be installed".into());
                }
                method_for_instrument(Some(&row.item_id))?;
                let item_id = row.item_id.clone();
                if row.quantity == 1 {
                    ctx.db.inventory_item().id().delete(id);
                } else {
                    row.quantity -= 1;
                    ctx.db.inventory_item().id().update(row);
                }
                item_id
            }
            "party" => {
                let party_id = actor
                    .party_id
                    .as_deref()
                    .ok_or("Character has no party inventory")?;
                let mut row = ctx
                    .db
                    .party_inventory_item()
                    .id()
                    .find(id)
                    .ok_or("Instrument not found")?;
                if row.party_id != party_id {
                    return Err("Instrument is not in this party inventory".into());
                }
                method_for_instrument(Some(&row.item_id))?;
                let item_id = row.item_id.clone();
                if row.quantity == 1 {
                    ctx.db.party_inventory_item().id().delete(id);
                } else {
                    row.quantity -= 1;
                    ctx.db.party_inventory_item().id().update(row);
                }
                item_id
            }
            _ => return Err("Invalid inventory scope".into()),
        };
        Some(item_id)
    } else {
        None
    };
    // Reducer transactions make this return-and-replace sequence atomic. If
    // stale source custody prevents a return, the staged replacement is rolled back.
    return_installed_tool(ctx, &actor, &station)?;
    station.instrument_item_id = replacement;
    station.instrument_object_id = None;
    station.instrument_source = station.instrument_item_id.as_ref().map(|_| inventory_scope);
    station.instrument_party_id = station
        .instrument_item_id
        .as_ref()
        .and(replacement_party_id);
    let existed = ctx
        .db
        .fireplace_station()
        .key()
        .find(station.key.clone())
        .is_some();
    if station.instrument_item_id.is_none() {
        if existed {
            ctx.db.fireplace_station().key().delete(station.key);
        }
    } else if existed {
        ctx.db.fireplace_station().key().update(station);
    } else {
        ctx.db.fireplace_station().insert(station);
    }
    Ok(())
}

fn vessel_station_key(character_id: u64, context_key: &str, object_id: u64) -> String {
    format!("{character_id}|{context_key}|container:{object_id}")
}

/// Places one exact vessel and its entire subtree over this exact fireplace.
/// The root legacy row is removed, so ordinary inventory/trade views cannot
/// remotely transfer it. Children retain their stable object edges.
#[reducer]
pub fn place_fireplace_container(
    ctx: &ReducerContext,
    character_id: u64,
    context_key: String,
    inventory_scope: String,
    inventory_item_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    let actor = crate::character::require_living_character(ctx, character_id)?;
    if actor.in_server {
        return Err("Cooking is unavailable during a tactical encounter".into());
    }
    validate_fireplace_context(ctx, &actor, &context_key)?;
    let mut object = crate::inventory_container::ensure_object(
        ctx,
        character_id,
        &inventory_scope,
        inventory_item_id,
        true,
    )?;
    if crate::inventory_container::object_is_nested(ctx, object.id) {
        return Err(
            "Remove a vessel from its parent container before placing it over a fire".into(),
        );
    }
    method_for_instrument(Some(&object.item_id))?;
    let source = object.location_kind.clone();
    let party_id = (source == "party").then(|| object.location_owner.clone());
    match source.as_str() {
        "personal" => {
            ctx.db.inventory_item().id().delete(object.inventory_row_id);
        }
        "party" => {
            ctx.db
                .party_inventory_item()
                .id()
                .delete(object.inventory_row_id);
        }
        _ => return Err("Container is not in carried inventory".into()),
    }
    let key = vessel_station_key(character_id, &context_key, object.id);
    object.location_kind = "fireplace".into();
    object.location_owner = context_key.clone();
    object.inventory_row_id = 0;
    ctx.db.inventory_object().id().update(object.clone());
    ctx.db.fireplace_station().insert(FireplaceStation {
        key,
        character_id,
        context_key,
        instrument_item_id: Some(object.item_id),
        instrument_object_id: Some(object.id),
        instrument_source: Some(source),
        instrument_party_id: party_id,
    });
    Ok(())
}

#[reducer]
pub fn retrieve_fireplace_container(
    ctx: &ReducerContext,
    character_id: u64,
    context_key: String,
    container_object_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    let actor = crate::character::require_living_character(ctx, character_id)?;
    validate_fireplace_context(ctx, &actor, &context_key)?;
    let key = vessel_station_key(character_id, &context_key, container_object_id);
    let station = ctx
        .db
        .fireplace_station()
        .key()
        .find(key.clone())
        .ok_or("Container is not at this fireplace")?;
    if ctx
        .db
        .fireplace_dish()
        .station_key()
        .find(key.clone())
        .is_some()
    {
        return Err("Retrieve the cooked dish before removing its container".into());
    }
    let item_id = station
        .instrument_item_id
        .clone()
        .ok_or("Fireplace vessel is missing")?;
    let (location_kind, location_owner, inventory_row_id) =
        match station.instrument_source.as_deref() {
            Some("personal") => {
                let row = ctx.db.inventory_item().insert(crate::InventoryItem {
                    id: 0,
                    character_id,
                    item_id: item_id.clone(),
                    quantity: 1,
                });
                ("personal".to_string(), character_id.to_string(), row.id)
            }
            Some("party") => {
                let party_id = station
                    .instrument_party_id
                    .clone()
                    .ok_or("Original party inventory is unavailable")?;
                if ctx
                    .db
                    .party_authority()
                    .id()
                    .find(party_id.clone())
                    .is_none()
                {
                    return Err("Original party inventory is unavailable".into());
                }
                let row =
                    ctx.db
                        .party_inventory_item()
                        .insert(crate::strategic::PartyInventoryItem {
                            id: 0,
                            party_id: party_id.clone(),
                            item_id: item_id.clone(),
                            quantity: 1,
                        });
                ("party".to_string(), party_id, row.id)
            }
            _ => return Err("Container source inventory is unknown".into()),
        };
    let mut object = ctx
        .db
        .inventory_object()
        .id()
        .find(container_object_id)
        .ok_or("Container object is missing")?;
    object.location_kind = location_kind.clone();
    object.location_owner = location_owner.clone();
    object.inventory_row_id = inventory_row_id;
    ctx.db.inventory_object().id().update(object);
    crate::inventory_container::rehome_subtree(
        ctx,
        container_object_id,
        &location_kind,
        &location_owner,
    )?;
    ctx.db.fireplace_station().key().delete(key);
    crate::inventory_container::merge_empty_container(ctx, container_object_id)?;
    Ok(())
}

fn current_minute(ctx: &ReducerContext, character_id: u64) -> u64 {
    ctx.db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |row| row.minutes)
}

pub fn create_personal_food_lot(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
    item_id: &str,
    quantity: u32,
) -> Result<FoodLot, String> {
    let definition = food::definition(item_id).ok_or("Food definition not found")?;
    let minute = current_minute(ctx, character_id);
    let lot = ctx.db.food_lot().insert(FoodLot {
        id: 0,
        inventory_item_id: Some(inventory_item_id),
        party_inventory_item_id: None,
        display_name: definition.name.into(),
        preparation: if definition.class == food::FoodClass::Ration {
            FoodPreparation::Preserved
        } else {
            FoodPreparation::Raw
        },
        ingredient_item_ids: vec![item_id.into()],
        ingredient_quantities: vec![quantity as f32],
        salty_kg: definition.flavors_per_unit.salty * quantity as f32,
        spicy_kg: definition.flavors_per_unit.spicy * quantity as f32,
        sweet_kg: definition.flavors_per_unit.sweet * quantity as f32,
        sour_kg: definition.flavors_per_unit.sour * quantity as f32,
        savory_kg: definition.flavors_per_unit.savory * quantity as f32,
        quality: definition.default_quality.clamp(1, 5),
        mass_kg: definition.mass_kg_per_unit * quantity as f32,
        nutrition_kcal: definition.kcal_per_unit * quantity as f32,
        total_value: definition.value_per_unit * quantity as f32,
        created_at_minute: minute,
    });
    ctx.db.food_contamination().insert(FoodContamination {
        food_lot_id: lot.id,
        concentration_anchor: food::deterministic_initial_contamination(
            ctx.random::<u64>() ^ lot.id ^ character_id,
        ),
        growth_per_hour: definition.growth_per_hour,
        anchor_minute: minute,
    });
    Ok(lot)
}

pub fn create_party_food_lot(
    ctx: &ReducerContext,
    inventory_item_id: u64,
    item_id: &str,
    quantity: u32,
    minute: u64,
) -> Option<FoodLot> {
    let definition = food::definition(item_id)?;
    let lot = ctx.db.food_lot().insert(FoodLot {
        id: 0,
        inventory_item_id: None,
        party_inventory_item_id: Some(inventory_item_id),
        display_name: definition.name.into(),
        preparation: if definition.class == food::FoodClass::Ration {
            FoodPreparation::Preserved
        } else {
            FoodPreparation::Raw
        },
        ingredient_item_ids: vec![item_id.into()],
        ingredient_quantities: vec![quantity as f32],
        salty_kg: definition.flavors_per_unit.salty * quantity as f32,
        spicy_kg: definition.flavors_per_unit.spicy * quantity as f32,
        sweet_kg: definition.flavors_per_unit.sweet * quantity as f32,
        sour_kg: definition.flavors_per_unit.sour * quantity as f32,
        savory_kg: definition.flavors_per_unit.savory * quantity as f32,
        quality: definition.default_quality.clamp(1, 5),
        mass_kg: definition.mass_kg_per_unit * quantity as f32,
        nutrition_kcal: definition.kcal_per_unit * quantity as f32,
        total_value: definition.value_per_unit * quantity as f32,
        created_at_minute: minute,
    });
    ctx.db.food_contamination().insert(FoodContamination {
        food_lot_id: lot.id,
        concentration_anchor: food::deterministic_initial_contamination(
            ctx.random::<u64>() ^ lot.id,
        ),
        growth_per_hour: definition.growth_per_hour,
        anchor_minute: minute,
    });
    Some(lot)
}

pub fn delete_personal_food_lot(ctx: &ReducerContext, inventory_item_id: u64) {
    for lot in ctx
        .db
        .food_lot()
        .iter()
        .filter(|lot| lot.inventory_item_id == Some(inventory_item_id))
        .collect::<Vec<_>>()
    {
        ctx.db.food_contamination().food_lot_id().delete(lot.id);
        ctx.db.food_lot().id().delete(lot.id);
    }
}

pub fn delete_party_food_lot(ctx: &ReducerContext, inventory_item_id: u64) {
    for lot in ctx
        .db
        .food_lot()
        .iter()
        .filter(|lot| lot.party_inventory_item_id == Some(inventory_item_id))
        .collect::<Vec<_>>()
    {
        ctx.db.food_contamination().food_lot_id().delete(lot.id);
        ctx.db.food_lot().id().delete(lot.id);
    }
}

pub fn remove_party_lot_quantity(
    ctx: &ReducerContext,
    inventory_item_id: u64,
    removed: u32,
    original: u32,
) -> Result<(), String> {
    if removed == original {
        delete_party_food_lot(ctx, inventory_item_id);
        return Ok(());
    }
    let mut lot = ctx
        .db
        .food_lot()
        .iter()
        .find(|lot| lot.party_inventory_item_id == Some(inventory_item_id))
        .ok_or("Food lot metadata not found")?;
    let keep = 1.0 - removed as f32 / original as f32;
    retain_lot_fraction(&mut lot, keep);
    ctx.db.food_lot().id().update(lot);
    Ok(())
}

fn split_ingredient_quantities(
    quantities: &[f32],
    taken: u32,
    original: u32,
) -> (Vec<f32>, Vec<f32>) {
    let ratio = taken as f32 / original as f32;
    let child = quantities
        .iter()
        .map(|quantity| food::retained_component(*quantity, ratio))
        .collect::<Vec<_>>();
    let source = quantities
        .iter()
        .zip(&child)
        .map(|(quantity, child_quantity)| (quantity - child_quantity).max(0.0))
        .collect();
    (source, child)
}

fn retain_lot_fraction(lot: &mut FoodLot, retained: f32) {
    lot.mass_kg = food::retained_component(lot.mass_kg, retained);
    lot.nutrition_kcal = food::retained_component(lot.nutrition_kcal, retained);
    lot.total_value = food::retained_component(lot.total_value, retained);
    lot.salty_kg = food::retained_component(lot.salty_kg, retained);
    lot.spicy_kg = food::retained_component(lot.spicy_kg, retained);
    lot.sweet_kg = food::retained_component(lot.sweet_kg, retained);
    lot.sour_kg = food::retained_component(lot.sour_kg, retained);
    lot.savory_kg = food::retained_component(lot.savory_kg, retained);
    for quantity in &mut lot.ingredient_quantities {
        *quantity = food::retained_component(*quantity, retained);
    }
}

pub fn personal_lot(ctx: &ReducerContext, inventory_item_id: u64) -> Option<FoodLot> {
    ctx.db
        .food_lot()
        .iter()
        .find(|lot| lot.inventory_item_id == Some(inventory_item_id))
}

pub fn party_lot(ctx: &ReducerContext, inventory_item_id: u64) -> Option<FoodLot> {
    ctx.db
        .food_lot()
        .iter()
        .find(|lot| lot.party_inventory_item_id == Some(inventory_item_id))
}

fn lot_for_inventory(ctx: &ReducerContext, inventory_item_id: u64) -> Result<FoodLot, String> {
    personal_lot(ctx, inventory_item_id).ok_or("Food lot metadata not found".into())
}

fn contamination(
    ctx: &ReducerContext,
    lot: &FoodLot,
    minute: u64,
) -> Result<(FoodContamination, f32), String> {
    let row = ctx
        .db
        .food_contamination()
        .food_lot_id()
        .find(lot.id)
        .ok_or("Food contamination state not found")?;
    let current = food::contamination_at(
        row.concentration_anchor,
        row.growth_per_hour,
        minute.saturating_sub(row.anchor_minute),
    );
    Ok((row, current))
}

pub fn split_lot(
    ctx: &ReducerContext,
    source_inventory_id: u64,
    destination_inventory_id: u64,
    taken: u32,
    original: u32,
) -> Result<(), String> {
    if taken == 0 || original == 0 || taken > original {
        return Err("Invalid food lot split".into());
    }
    let mut source = lot_for_inventory(ctx, source_inventory_id)?;
    if taken == original {
        source.inventory_item_id = Some(destination_inventory_id);
        ctx.db.food_lot().id().update(source);
        return Ok(());
    }
    let ratio = taken as f32 / original as f32;
    let mut child = source.clone();
    child.id = 0;
    child.inventory_item_id = Some(destination_inventory_id);
    retain_lot_fraction(&mut child, ratio);
    let (source_ingredients, child_ingredients) =
        split_ingredient_quantities(&source.ingredient_quantities, taken, original);
    child.ingredient_quantities = child_ingredients;
    retain_lot_fraction(&mut source, 1.0 - ratio);
    source.ingredient_quantities = source_ingredients;
    let contamination = ctx
        .db
        .food_contamination()
        .food_lot_id()
        .find(source.id)
        .ok_or("Food contamination state not found")?;
    let child = ctx.db.food_lot().insert(child);
    ctx.db.food_contamination().insert(FoodContamination {
        food_lot_id: child.id,
        ..contamination
    });
    ctx.db.food_lot().id().update(source);
    Ok(())
}

pub fn remove_lot_quantity(
    ctx: &ReducerContext,
    inventory_item_id: u64,
    removed: u32,
    original: u32,
) -> Result<(), String> {
    if removed == 0 || original == 0 || removed > original {
        return Err("Invalid food lot quantity change".into());
    }
    if removed == original {
        delete_personal_food_lot(ctx, inventory_item_id);
        return Ok(());
    }
    let mut lot = lot_for_inventory(ctx, inventory_item_id)?;
    let keep = 1.0 - removed as f32 / original as f32;
    retain_lot_fraction(&mut lot, keep);
    ctx.db.food_lot().id().update(lot);
    Ok(())
}

pub fn move_or_split_to_party(
    ctx: &ReducerContext,
    source_inventory_id: u64,
    destination_party_id: u64,
    taken: u32,
    original: u32,
) -> Result<(), String> {
    let mut source = lot_for_inventory(ctx, source_inventory_id)?;
    if taken == original {
        source.inventory_item_id = None;
        source.party_inventory_item_id = Some(destination_party_id);
        ctx.db.food_lot().id().update(source);
        return Ok(());
    }
    let ratio = taken as f32 / original as f32;
    let mut child = source.clone();
    child.id = 0;
    child.inventory_item_id = None;
    child.party_inventory_item_id = Some(destination_party_id);
    retain_lot_fraction(&mut child, ratio);
    let (source_ingredients, child_ingredients) =
        split_ingredient_quantities(&source.ingredient_quantities, taken, original);
    child.ingredient_quantities = child_ingredients;
    retain_lot_fraction(&mut source, 1.0 - ratio);
    source.ingredient_quantities = source_ingredients;
    let hidden = ctx
        .db
        .food_contamination()
        .food_lot_id()
        .find(source.id)
        .ok_or("Food contamination state not found")?;
    let child = ctx.db.food_lot().insert(child);
    ctx.db.food_contamination().insert(FoodContamination {
        food_lot_id: child.id,
        ..hidden
    });
    ctx.db.food_lot().id().update(source);
    Ok(())
}

pub fn move_or_split_to_personal(
    ctx: &ReducerContext,
    source_party_id: u64,
    destination_inventory_id: u64,
    taken: u32,
    original: u32,
) -> Result<(), String> {
    let mut source = ctx
        .db
        .food_lot()
        .iter()
        .find(|lot| lot.party_inventory_item_id == Some(source_party_id))
        .ok_or("Food lot metadata not found")?;
    if taken == original {
        source.party_inventory_item_id = None;
        source.inventory_item_id = Some(destination_inventory_id);
        ctx.db.food_lot().id().update(source);
        return Ok(());
    }
    let ratio = taken as f32 / original as f32;
    let mut child = source.clone();
    child.id = 0;
    child.party_inventory_item_id = None;
    child.inventory_item_id = Some(destination_inventory_id);
    retain_lot_fraction(&mut child, ratio);
    let (source_ingredients, child_ingredients) =
        split_ingredient_quantities(&source.ingredient_quantities, taken, original);
    child.ingredient_quantities = child_ingredients;
    retain_lot_fraction(&mut source, 1.0 - ratio);
    source.ingredient_quantities = source_ingredients;
    let hidden = ctx
        .db
        .food_contamination()
        .food_lot_id()
        .find(source.id)
        .ok_or("Food contamination state not found")?;
    let child = ctx.db.food_lot().insert(child);
    ctx.db.food_contamination().insert(FoodContamination {
        food_lot_id: child.id,
        ..hidden
    });
    ctx.db.food_lot().id().update(source);
    Ok(())
}

fn item_quantity(ctx: &ReducerContext, character_id: u64, item_id: &str) -> u32 {
    ctx.db
        .inventory_item()
        .character_and_item_id()
        .filter((character_id, item_id))
        .map(|row| row.quantity)
        .sum()
}

fn equipment_reason(
    ctx: &ReducerContext,
    character_id: u64,
    method: CookingMethod,
) -> Option<&'static str> {
    match method {
        CookingMethod::PanFry if item_quantity(ctx, character_id, "cooking_pan") == 0 => {
            Some("A pan is required")
        }
        CookingMethod::Stew if item_quantity(ctx, character_id, "cooking_pot") == 0 => {
            Some("A pot is required")
        }
        CookingMethod::Bake if item_quantity(ctx, character_id, "portable_oven") == 0 => {
            Some("A portable oven is required")
        }
        _ => None,
    }
}

fn cooking_check(ctx: &ReducerContext, character_id: u64) -> Result<f32, String> {
    let attributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .ok_or("Character attributes not found")?;
    let limbs = ctx
        .db
        .character_limbs()
        .character_id()
        .find(character_id)
        .ok_or("Character limbs not found")?;
    let skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Character skills not found")?;
    Ok(Skill::Cooking.capped_rank_for_aptitude(
        skills.effective_skill_hours(Skill::Cooking),
        Skill::Cooking.governing_aptitude(&attributes),
    ) * limbs.head_health.clamp(0.0, 1.0))
}

fn stew_water_required_ml(amounts_milliunits: &[u32]) -> Option<f32> {
    let total = amounts_milliunits
        .iter()
        .try_fold(0_u64, |sum, amount| sum.checked_add(u64::from(*amount)))?;
    let required =
        500.0 + total as f32 / crate::inventory_amount::FULL_AMOUNT_MILLIUNITS as f32 * 100.0;
    required.is_finite().then_some(required)
}

#[reducer]
pub fn add_fireplace_ingredients(
    ctx: &ReducerContext,
    character_id: u64,
    context_key: String,
    inventory_scope: String,
    inventory_item_ids: Vec<u64>,
    amounts_milliunits: Vec<u32>,
) -> Result<(), String> {
    add_fireplace_ingredients_at(
        ctx,
        character_id,
        context_key,
        inventory_scope,
        inventory_item_ids,
        amounts_milliunits,
        None,
    )
}

/// Starts the independent dish lane belonging to one placed vessel. Every
/// contained cookable food lot at any nesting depth is consumed in full;
/// non-food solids and nested containers remain in place. Container water is used by the cooking evaluator and is
/// mandatory for pots.
#[reducer]
pub fn start_fireplace_container_cooking(
    ctx: &ReducerContext,
    character_id: u64,
    context_key: String,
    container_object_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    let actor = crate::character::require_living_character(ctx, character_id)?;
    validate_fireplace_context(ctx, &actor, &context_key)?;
    let key = vessel_station_key(character_id, &context_key, container_object_id);
    let station = ctx
        .db
        .fireplace_station()
        .key()
        .find(key.clone())
        .ok_or("Container is not over this fireplace")?;
    if ctx.db.fireplace_dish().station_key().find(key).is_some() {
        return Err("This container is already cooking".into());
    }
    let scope = station
        .instrument_source
        .clone()
        .ok_or("Container source inventory is unknown")?;
    let mut ids = Vec::new();
    let mut amounts = Vec::new();
    let mut consumed_objects = Vec::new();
    for object_id in crate::inventory_container::subtree_object_ids(ctx, container_object_id)? {
        if object_id == container_object_id {
            continue;
        }
        let child = ctx
            .db
            .inventory_object()
            .id()
            .find(object_id)
            .ok_or("Contained object is missing")?;
        if !food::is_cookable_ingredient(&child.item_id) {
            continue;
        }
        let (lot, amount) = match scope.as_str() {
            "personal" => (
                personal_lot(ctx, child.inventory_row_id),
                crate::inventory_amount::personal_amount(ctx, child.inventory_row_id),
            ),
            "party" => (
                party_lot(ctx, child.inventory_row_id),
                crate::inventory_amount::party_amount(ctx, child.inventory_row_id),
            ),
            _ => (None, None),
        };
        let (Some(lot), Some(amount)) = (lot, amount) else {
            continue;
        };
        if lot.preparation != FoodPreparation::Raw && lot.preparation != FoodPreparation::Preserved
        {
            return Err("A cooked meal cannot be cooked again".into());
        }
        ids.push(child.inventory_row_id);
        amounts.push(amount);
        consumed_objects.push(child.id);
    }
    if ids.is_empty() {
        return Err("Put at least one uncooked food lot in the container".into());
    }
    add_fireplace_ingredients_at(
        ctx,
        character_id,
        context_key,
        scope,
        ids,
        amounts,
        Some(station),
    )?;
    for object_id in consumed_objects {
        ctx.db
            .inventory_containment()
            .child_object_id()
            .delete(object_id);
        ctx.db.inventory_object().id().delete(object_id);
    }
    Ok(())
}

fn add_fireplace_ingredients_at(
    ctx: &ReducerContext,
    character_id: u64,
    context_key: String,
    inventory_scope: String,
    inventory_item_ids: Vec<u64>,
    amounts_milliunits: Vec<u32>,
    vessel_station: Option<FireplaceStation>,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    let actor = crate::character::require_living_character(ctx, character_id)?;
    if actor.in_server {
        return Err("Cooking is unavailable during a tactical encounter".into());
    }
    validate_fireplace_context(ctx, &actor, &context_key)?;
    let is_vessel = vessel_station.is_some();
    let station =
        vessel_station.unwrap_or_else(|| fireplace_station_for(ctx, character_id, &context_key));
    if ctx
        .db
        .fireplace_dish()
        .station_key()
        .find(station.key.clone())
        .is_some()
    {
        return Err("This fireplace already holds a dish".into());
    }
    if inventory_item_ids.is_empty()
        || inventory_item_ids.len() != amounts_milliunits.len()
        || !is_vessel && inventory_item_ids.len() > 32
    {
        return Err("Select between one and 32 food lots".into());
    }
    let method = if is_vessel {
        method_for_instrument(station.instrument_item_id.as_deref())?
    } else {
        CookingMethod::Roast
    };
    let check = cooking_check(ctx, character_id)?;
    initialize_character_condition(ctx, character_id)?;
    let minute = current_minute(ctx, character_id);
    let mut seen = std::collections::BTreeSet::new();
    let mut selected = Vec::new();
    let mut safety = Vec::new();
    let mut name_parts = Vec::new();
    let mut ingredient_ids = Vec::new();
    let mut ingredient_quantities = Vec::new();
    let mut flavors = food::FlavorProfile::default();
    let mut mass = 0.0;
    let mut kcal = 0.0;
    let mut value = 0.0;
    let mut culinary_fat_mass = 0.0;
    let mut growth = Vec::new();
    let mut growth_mass = 0.0;
    let mut loads = Vec::new();
    for (&id, &amount) in inventory_item_ids.iter().zip(&amounts_milliunits) {
        if amount == 0 || !seen.insert(id) {
            return Err("Food lot selections must be unique and positive".into());
        }
        let (item_id, available, lot) = match inventory_scope.as_str() {
            "personal" => {
                let row = ctx
                    .db
                    .inventory_item()
                    .id()
                    .find(id)
                    .ok_or("Ingredient inventory row not found")?;
                if row.character_id != character_id
                    || crate::character::wearable_is_equipped(ctx, id)
                {
                    return Err("Ingredient is equipped or not in this inventory".into());
                }
                (
                    row.item_id,
                    crate::inventory_amount::personal_amount(ctx, id)
                        .ok_or("Ingredient amount state is missing")?,
                    personal_lot(ctx, id).ok_or("Food lot metadata not found")?,
                )
            }
            "party" => {
                let party_id = actor
                    .party_id
                    .as_deref()
                    .ok_or("Character has no party inventory")?;
                let row = ctx
                    .db
                    .party_inventory_item()
                    .id()
                    .find(id)
                    .ok_or("Ingredient inventory row not found")?;
                if row.party_id != party_id {
                    return Err("Ingredient is not in this party inventory".into());
                }
                (
                    row.item_id,
                    crate::inventory_amount::party_amount(ctx, id)
                        .ok_or("Ingredient amount state is missing")?,
                    party_lot(ctx, id).ok_or("Food lot metadata not found")?,
                )
            }
            _ => return Err("Invalid inventory scope".into()),
        };
        if amount > available {
            return Err("Ingredient is not available in that amount".into());
        }
        if !food::is_cookable_ingredient(&item_id) {
            return Err("A cooked meal cannot be cooked again".into());
        }
        let ratio = amount as f32 / available as f32;
        if ![
            lot.mass_kg,
            lot.nutrition_kcal,
            lot.total_value,
            lot.salty_kg,
            lot.spicy_kg,
            lot.sweet_kg,
            lot.sour_kg,
            lot.savory_kg,
        ]
        .into_iter()
        .all(|v| v.is_finite() && v >= 0.0)
        {
            return Err("Ingredient lot contains invalid food values".into());
        }
        let (cont, current) = contamination(ctx, &lot, minute)?;
        safety.push(food::definition(&item_id).map_or(5, |d| d.cooking_minutes));
        name_parts.push(lot.display_name.clone());
        ingredient_ids.extend(lot.ingredient_item_ids.clone());
        ingredient_quantities.extend(
            lot.ingredient_quantities
                .iter()
                .map(|q| food::retained_component(*q, ratio)),
        );
        let selected_mass = lot.mass_kg * ratio;
        mass += selected_mass;
        kcal += lot.nutrition_kcal * ratio;
        value += lot.total_value * ratio;
        flavors.add_assign(
            food::FlavorProfile::new(
                lot.salty_kg,
                lot.spicy_kg,
                lot.sweet_kg,
                lot.sour_kg,
                lot.savory_kg,
            )
            .scaled(ratio),
        );
        if lot
            .ingredient_item_ids
            .iter()
            .any(|i| food::definition(i).is_some_and(|d| d.culinary_fat))
        {
            culinary_fat_mass += selected_mass;
        }
        growth.push(cont.growth_per_hour);
        growth_mass += cont.growth_per_hour.max(0.0) * selected_mass;
        loads.push(current * selected_mass);
        selected.push((id, amount, available, lot));
    }
    let ingredient_mass = mass;
    let contained_water_ml = station
        .instrument_object_id
        .and_then(|object_id| {
            ctx.db
                .container_liquid()
                .container_object_id()
                .find(object_id)
        })
        .map_or(0, |liquid| liquid.water_ml);
    let water_ml = if station.instrument_object_id.is_some() {
        if method == CookingMethod::Stew && contained_water_ml == 0 {
            return Err("Stew requires water inside the cooking pot".into());
        }
        contained_water_ml as f32
    } else if method == CookingMethod::Stew {
        stew_water_required_ml(&amounts_milliunits).ok_or("Stew water could not be calculated")?
    } else {
        0.0
    };
    let mut needs = ctx
        .db
        .character_needs()
        .character_id()
        .find(character_id)
        .ok_or("Character needs not found")?;
    let pooled = actor
        .party_id
        .as_deref()
        .and_then(|id| ctx.db.party_authority().id().find(id.to_string()))
        .map_or(0.0, |p| p.pooled_water_ml);
    if station.instrument_object_id.is_none() && pooled + needs.carried_water_ml < water_ml {
        return Err("Stew requires enough pooled or carried water".into());
    }
    mass += water_ml / 1_000.0;
    let target = food::cooking_duration_minutes_for_check(method.core(), &safety, mass, check)
        .ok_or("Cooking duration could not be calculated")?;
    let flavor_quality = food::aggregate_flavor_quality(method.core(), flavors, mass);
    let quality = food::cooked_quality(
        food::chef_quality_tier(check),
        flavor_quality,
        method == CookingMethod::PanFry
            && !food::pan_fry_has_enough_fat(culinary_fat_mass, ingredient_mass),
    );
    // Everything above is preflight. Mutation starts here and remains atomic.
    if station.instrument_object_id.is_some() && contained_water_ml > 0 {
        ctx.db
            .container_liquid()
            .container_object_id()
            .delete(station.instrument_object_id.unwrap());
    }
    if method == CookingMethod::Stew {
        if let Some(object_id) = station.instrument_object_id {
            let _ = object_id; // contained water was consumed above
        } else if let Some(party_id) = actor.party_id.as_deref()
            && let Some(mut party) = ctx.db.party_authority().id().find(party_id.to_string())
        {
            let used = water_ml.min(party.pooled_water_ml);
            party.pooled_water_ml -= used;
            ctx.db.party_authority().id().update(party);
            needs.carried_water_ml -= water_ml - used;
        } else {
            needs.carried_water_ml -= water_ml;
        }
        ctx.db.character_needs().character_id().update(needs);
    }
    for (id, amount, available, mut lot) in selected {
        if amount == available {
            match inventory_scope.as_str() {
                "personal" => {
                    ctx.db
                        .inventory_item_amount()
                        .inventory_item_id()
                        .delete(id);
                    ctx.db.inventory_item().id().delete(id);
                    delete_personal_food_lot(ctx, id);
                }
                "party" => {
                    ctx.db
                        .party_item_amount()
                        .party_inventory_item_id()
                        .delete(id);
                    ctx.db.party_inventory_item().id().delete(id);
                    delete_party_food_lot(ctx, id);
                }
                _ => unreachable!(),
            }
        } else {
            retain_lot_fraction(&mut lot, 1.0 - amount as f32 / available as f32);
            ctx.db.food_lot().id().update(lot);
            match inventory_scope.as_str() {
                "personal" => {
                    ctx.db.inventory_item_amount().inventory_item_id().update(
                        crate::InventoryItemAmount {
                            inventory_item_id: id,
                            remaining_milliunits: available - amount,
                        },
                    );
                }
                "party" => {
                    ctx.db.party_item_amount().party_inventory_item_id().update(
                        crate::PartyItemAmount {
                            party_inventory_item_id: id,
                            remaining_milliunits: available - amount,
                        },
                    );
                }
                _ => unreachable!(),
            };
        }
    }
    name_parts.sort();
    name_parts.dedup();
    // Water adds mass but never microbial load.
    let raw_contamination = food::microbial_concentration(loads.iter().sum(), mass);
    let raw_growth_per_hour = if ingredient_mass > 0.0 {
        growth_mass / ingredient_mass
    } else {
        0.0
    };
    let ready_nutrition_retention =
        food::cooked_nutrition_retention(check) * food::method_nutrition_retention(method.core());
    ctx.db.fireplace_dish().insert(FireplaceDish {
        station_key: station.key.clone(),
        character_id,
        contributor_name: actor.name,
        method,
        cooking_check: check,
        started_at_minute: minute,
        target_minutes: target,
        display_name: format!("{} {}", method.name(), name_parts.join(", ")),
        ingredient_item_ids: ingredient_ids,
        ingredient_quantities,
        salty_kg: flavors.salty,
        spicy_kg: flavors.spicy,
        sweet_kg: flavors.sweet,
        sour_kg: flavors.sour,
        savory_kg: flavors.savory,
        ready_quality: quality,
        mass_kg: mass,
        raw_nutrition_kcal: kcal,
        ready_nutrition_retention,
        ingredient_value: value,
        raw_contamination,
        raw_growth_per_hour,
        cooked_growth_per_hour: food::cooked_growth_per_hour(&growth, method.core()),
    });
    if ctx
        .db
        .fireplace_station()
        .key()
        .find(station.key.clone())
        .is_none()
    {
        ctx.db.fireplace_station().insert(station);
    }
    Ok(())
}

#[reducer]
pub fn retrieve_fireplace_dish(
    ctx: &ReducerContext,
    character_id: u64,
    context_key: String,
    inventory_scope: String,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    let actor = crate::character::require_living_character(ctx, character_id)?;
    if actor.in_server {
        return Err("Cooking is unavailable during a tactical encounter".into());
    }
    validate_fireplace_context(ctx, &actor, &context_key)?;
    let container_object_id = inventory_scope
        .strip_prefix("container:")
        .and_then(|id| id.parse::<u64>().ok());
    let key = container_object_id.map_or_else(
        || station_key(character_id, &context_key),
        |object_id| vessel_station_key(character_id, &context_key, object_id),
    );
    let vessel_station = ctx.db.fireplace_station().key().find(key.clone());
    let dish = ctx
        .db
        .fireplace_dish()
        .station_key()
        .find(key)
        .ok_or("No dish is in this fireplace")?;
    let minute = current_minute(ctx, character_id);
    let elapsed = minute.saturating_sub(dish.started_at_minute);
    let doneness = food::doneness_outcome(elapsed, dish.target_minutes);
    let quality = dish
        .ready_quality
        .saturating_sub(doneness.quality_penalty)
        .max(1);
    let kcal = dish.raw_nutrition_kcal
        * food::doneness_nutrition_factor(dish.ready_nutrition_retention, doneness);
    let value =
        dish.ingredient_value * food::quality_value_multiplier(quality) * doneness.calorie_factor;
    let personal_id;
    let party_id;
    let effective_scope = vessel_station
        .as_ref()
        .and_then(|station| station.instrument_source.as_deref())
        .unwrap_or(&inventory_scope);
    match effective_scope {
        "personal" => {
            let row = ctx.db.inventory_item().insert(crate::InventoryItem {
                id: 0,
                character_id,
                item_id: "cooked_meal".into(),
                quantity: 1,
            });
            crate::inventory_amount::initialize_personal(ctx, row.id);
            personal_id = Some(row.id);
            party_id = None;
        }
        "party" => {
            let owner = actor
                .party_id
                .as_deref()
                .ok_or("Character has no party inventory")?;
            let row = ctx.db.party_inventory_item().insert(PartyInventoryItem {
                id: 0,
                party_id: owner.into(),
                item_id: "cooked_meal".into(),
                quantity: 1,
            });
            crate::inventory_amount::initialize_party(ctx, row.id);
            personal_id = None;
            party_id = Some(row.id);
        }
        _ => return Err("Invalid retrieval inventory".into()),
    }
    if let Some(parent_object_id) = container_object_id {
        let row_id = personal_id.or(party_id).expect("cooked meal inventory row");
        let meal = crate::inventory_container::ensure_object(
            ctx,
            character_id,
            effective_scope,
            row_id,
            false,
        )?;
        ctx.db
            .inventory_containment()
            .insert(crate::InventoryContainment {
                child_object_id: meal.id,
                parent_object_id,
            });
    }
    let lot = ctx.db.food_lot().insert(FoodLot {
        id: 0,
        inventory_item_id: personal_id,
        party_inventory_item_id: party_id,
        display_name: dish.display_name,
        preparation: dish.method.preparation(),
        ingredient_item_ids: dish.ingredient_item_ids,
        ingredient_quantities: dish.ingredient_quantities,
        salty_kg: dish.salty_kg,
        spicy_kg: dish.spicy_kg,
        sweet_kg: dish.sweet_kg,
        sour_kg: dish.sour_kg,
        savory_kg: dish.savory_kg,
        quality,
        mass_kg: dish.mass_kg,
        nutrition_kcal: kcal,
        total_value: value,
        created_at_minute: minute,
    });
    ctx.db.food_contamination().insert(FoodContamination {
        food_lot_id: lot.id,
        concentration_anchor: food::partially_cooked_contamination(
            dish.raw_contamination,
            dish.method.core(),
            doneness.contamination_kill_progress,
        ),
        growth_per_hour: food::partially_cooked_growth(
            dish.raw_growth_per_hour,
            dish.cooked_growth_per_hour,
            doneness.contamination_kill_progress,
        ),
        anchor_minute: minute,
    });
    ctx.db
        .fireplace_dish()
        .station_key()
        .delete(dish.station_key.clone());
    if let Some(station) = ctx.db.fireplace_station().key().find(dish.station_key)
        && station.instrument_item_id.is_none()
    {
        ctx.db.fireplace_station().key().delete(station.key);
    }
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(())
}

pub fn preview_cooking(
    ctx: &ReducerContext,
    character_id: u64,
    method: CookingMethod,
    inventory_ids: &[u64],
    amounts_milliunits: &[u32],
) -> Result<u32, String> {
    if inventory_ids.is_empty()
        || inventory_ids.len() != amounts_milliunits.len()
        || inventory_ids.len() > 32
    {
        return Err("Select between one and 32 food lots".into());
    }
    if let Some(reason) = equipment_reason(ctx, character_id, method) {
        return Err(reason.into());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut safety = Vec::new();
    let mut mass = 0.0;
    for (&id, &amount) in inventory_ids.iter().zip(amounts_milliunits) {
        if amount == 0 || !seen.insert(id) {
            return Err("Food lot selections must be unique and positive".into());
        }
        let inventory = ctx
            .db
            .inventory_item()
            .id()
            .find(id)
            .ok_or("Ingredient inventory row not found")?;
        let available = crate::inventory_amount::personal_amount(ctx, id).unwrap_or(0);
        if inventory.character_id != character_id || amount > available {
            return Err("Ingredient is not available in that amount".into());
        }
        if !food::is_cookable_ingredient(&inventory.item_id) {
            return Err("A cooked meal cannot be cooked again".into());
        }
        let lot = lot_for_inventory(ctx, id)?;
        safety.push(
            food::definition(&inventory.item_id).map_or(5, |definition| definition.cooking_minutes),
        );
        mass += lot.mass_kg * amount as f32 / available as f32;
    }
    food::cooking_duration_minutes_for_check(
        method.core(),
        &safety,
        mass,
        cooking_check(ctx, character_id)?,
    )
    .ok_or("Cooking duration could not be calculated".into())
}

fn expose_to_dysentery(
    ctx: &ReducerContext,
    character_id: u64,
    lot_id: u64,
    minute: u64,
    dose: f32,
) -> Result<(), String> {
    if dose <= 0.0 {
        return Ok(());
    }
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(3.0, |row| row.immunity);
    let episodes = crate::disease::character_episodes(ctx, character_id)?;
    if disease::has_unresolved_disease(&episodes, DiseaseId::Dysentery, minute, immunity) {
        return Ok(());
    }
    let prior = disease::acquired_immunity(&episodes, DiseaseId::Dysentery, minute, immunity);
    let seed =
        disease::outbreak_exposure_seed(character_id, &format!("food:{lot_id}:{}", minute / 1));
    let protected_dose = crate::disease::protected_point_exposure(
        ctx,
        character_id,
        minute,
        adventuresim_core::disease::TransmissionVector::FoodWater,
        dose,
    )?;
    if disease::acquisition_succeeds(
        seed,
        disease::definition(DiseaseId::Dysentery),
        immunity,
        prior,
        protected_dose,
    ) {
        ctx.db.infection_episode().insert(InfectionEpisodeRow {
            id: 0,
            character_id,
            disease_id: "dysentery".into(),
            contracted_at: minute,
            ruleset_version: adventuresim_core::physiology::PHYSIOLOGY_RULESET_VERSION,
            phenotype_key_version: adventuresim_core::physiology::PHENOTYPE_KEY_VERSION,
        });
    }
    Ok(())
}

fn consume_food_amount(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_id: u64,
    kcal: f32,
    explicit: bool,
) -> Result<f32, String> {
    initialize_character_condition(ctx, character_id)?;
    let inventory = ctx
        .db
        .inventory_item()
        .id()
        .find(inventory_id)
        .ok_or("Food inventory row not found")?;
    if inventory.character_id != character_id {
        return Err("Food is not in this inventory".into());
    }
    crate::inventory_container::reconcile_consumed_row(ctx, "personal", inventory_id, false)?;
    let mut lot = lot_for_inventory(ctx, inventory_id)?;
    let mut needs = ctx
        .db
        .character_needs()
        .character_id()
        .find(character_id)
        .ok_or("Character needs not found")?;
    let wanted = if explicit {
        food::explicit_meal_consumption(needs.food_balance_kcal, lot.nutrition_kcal)
    } else {
        food::travel_consumption(needs.food_balance_kcal, lot.nutrition_kcal)
    }
    .min(kcal.max(0.0));
    if wanted <= 0.0 {
        return Ok(0.0);
    }
    let ratio = (wanted / lot.nutrition_kcal).clamp(0.0, 1.0);
    let minute = current_minute(ctx, character_id);
    let (_, current) = contamination(ctx, &lot, minute)?;
    expose_to_dysentery(
        ctx,
        character_id,
        lot.id,
        minute,
        current * ratio * lot.mass_kg,
    )?;
    needs.food_balance_kcal += wanted;
    ctx.db.character_needs().character_id().update(needs);
    if ratio >= 0.999_999 {
        crate::inventory_container::reconcile_consumed_row(ctx, "personal", inventory.id, true)?;
        ctx.db
            .inventory_item_amount()
            .inventory_item_id()
            .delete(inventory.id);
        ctx.db.inventory_item().id().delete(inventory.id);
        delete_personal_food_lot(ctx, inventory.id);
    } else {
        let retained = 1.0 - ratio;
        let state = ctx
            .db
            .inventory_item_amount()
            .inventory_item_id()
            .find(inventory.id)
            .ok_or("Food amount state is missing")?;
        retain_lot_fraction(&mut lot, retained);
        ctx.db.food_lot().id().update(lot);
        ctx.db
            .inventory_item_amount()
            .inventory_item_id()
            .update(crate::InventoryItemAmount {
                inventory_item_id: inventory.id,
                remaining_milliunits: ((state.remaining_milliunits as f32) * retained)
                    .floor()
                    .max(1.0) as u32,
            });
    }
    crate::capability::refresh_character_capability(ctx, character_id)?;
    Ok(wanted)
}

pub fn consume_travel_food_to_zero(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    initialize_character_condition(ctx, character_id)?;
    let actor = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if let Some(party_id) = actor.party_id.as_deref() {
        let mut candidates: Vec<_> = ctx
            .db
            .party_inventory_item()
            .party_id()
            .filter(party_id)
            .filter_map(|inventory| {
                let lot = ctx
                    .db
                    .food_lot()
                    .iter()
                    .find(|lot| lot.party_inventory_item_id == Some(inventory.id))?;
                Some((lot.created_at_minute, inventory.id, inventory, lot))
            })
            .collect();
        candidates.sort_by_key(|row| (row.0, row.1));
        for (_, _, inventory, mut lot) in candidates {
            crate::inventory_container::reconcile_consumed_row(ctx, "party", inventory.id, false)?;
            let deficit = ctx
                .db
                .character_needs()
                .character_id()
                .find(character_id)
                .map_or(0.0, |n| n.food_balance_kcal);
            let wanted = food::travel_consumption(deficit, lot.nutrition_kcal);
            if wanted <= 0.0 {
                break;
            }
            let ratio = (wanted / lot.nutrition_kcal).clamp(0.0, 1.0);
            let minute = current_minute(ctx, character_id);
            let (_, current) = contamination(ctx, &lot, minute)?;
            expose_to_dysentery(
                ctx,
                character_id,
                lot.id,
                minute,
                current * ratio * lot.mass_kg,
            )?;
            let mut needs = ctx
                .db
                .character_needs()
                .character_id()
                .find(character_id)
                .unwrap();
            needs.food_balance_kcal = (needs.food_balance_kcal + wanted).min(0.0);
            ctx.db.character_needs().character_id().update(needs);
            if ratio >= 0.999_999 {
                crate::inventory_container::reconcile_consumed_row(
                    ctx,
                    "party",
                    inventory.id,
                    true,
                )?;
                ctx.db
                    .party_item_amount()
                    .party_inventory_item_id()
                    .delete(inventory.id);
                ctx.db.party_inventory_item().id().delete(inventory.id);
                ctx.db.food_contamination().food_lot_id().delete(lot.id);
                ctx.db.food_lot().id().delete(lot.id);
            } else {
                let retained = 1.0 - ratio;
                let state = ctx
                    .db
                    .party_item_amount()
                    .party_inventory_item_id()
                    .find(inventory.id)
                    .ok_or("Party food amount state is missing")?;
                retain_lot_fraction(&mut lot, retained);
                ctx.db.food_lot().id().update(lot);
                ctx.db.party_item_amount().party_inventory_item_id().update(
                    crate::PartyItemAmount {
                        party_inventory_item_id: inventory.id,
                        remaining_milliunits: ((state.remaining_milliunits as f32) * retained)
                            .floor()
                            .max(1.0) as u32,
                    },
                );
            }
        }
    }
    let mut personal: Vec<_> = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter_map(|inventory| {
            lot_for_inventory(ctx, inventory.id)
                .ok()
                .map(|lot| (lot.created_at_minute, inventory.id))
        })
        .collect();
    personal.sort_unstable();
    for (_, id) in personal {
        if ctx
            .db
            .character_needs()
            .character_id()
            .find(character_id)
            .is_some_and(|n| n.food_balance_kcal >= 0.0)
        {
            break;
        }
        consume_food_amount(ctx, character_id, id, f32::MAX, false)?;
    }
    Ok(())
}

pub fn clear_stomach_fullness(ctx: &ReducerContext, character_id: u64) {
    if let Some(mut needs) = ctx.db.character_needs().character_id().find(character_id) {
        needs.food_balance_kcal = needs.food_balance_kcal.min(0.0);
        ctx.db.character_needs().character_id().update(needs);
    }
}

#[reducer]
pub fn eat_food(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    crate::character::require_living_character(ctx, character_id)?;
    let actor = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if actor.in_server {
        return Err("Eating is unavailable during a tactical encounter".into());
    }
    consume_food_amount(ctx, character_id, inventory_item_id, f32::MAX, true)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn method_mapping_is_stable() {
        assert_eq!(CookingMethod::Roast.core(), food::CookingMethod::Roast);
    }

    #[test]
    fn container_cooking_is_distinct_from_legacy_loose_roasting() {
        let source = include_str!("food.rs");
        let legacy = source
            .split("pub fn set_fireplace_instrument")
            .nth(1)
            .unwrap()
            .split("fn vessel_station_key")
            .next()
            .unwrap();
        assert!(legacy.contains("Cooking tools must be placed as containers over the fire"));
        let cooking = source
            .split("fn add_fireplace_ingredients_at")
            .nth(1)
            .unwrap();
        assert!(cooking.contains("let is_vessel = vessel_station.is_some()"));
        assert!(cooking.contains("!is_vessel && inventory_item_ids.len() > 32"));
        assert!(cooking.contains("CookingMethod::Roast"));
    }

    #[test]
    fn recursive_vessel_selection_requires_authoritative_food_lots() {
        let source = include_str!("food.rs");
        let reducer = source
            .split("pub fn start_fireplace_container_cooking")
            .nth(1)
            .unwrap()
            .split("fn add_fireplace_ingredients_at")
            .next()
            .unwrap();
        assert!(reducer.contains("subtree_object_ids"));
        assert!(reducer.contains("personal_lot(ctx, child.inventory_row_id)"));
        assert!(reducer.contains("party_lot(ctx, child.inventory_row_id)"));
        assert!(reducer.contains("let (Some(lot), Some(amount))"));
        assert!(reducer.contains("A cooked meal cannot be cooked again"));
    }

    #[test]
    fn eating_and_travel_reconcile_stable_container_objects() {
        let source = include_str!("food.rs");
        assert!(source.contains("reconcile_consumed_row(ctx, \"personal\""));
        assert!(source.contains("reconcile_consumed_row(ctx, \"party\""));
    }

    #[test]
    fn authoritative_preview_rejects_cooked_output_as_an_ingredient() {
        let source = include_str!("food.rs");
        let preview = source
            .split("pub fn preview_cooking")
            .nth(1)
            .and_then(|tail| tail.split("fn expose_to_dysentery").next())
            .expect("preview cooking implementation");
        assert!(preview.contains("food::is_cookable_ingredient(&inventory.item_id)"));
        assert!(preview.contains("A cooked meal cannot be cooked again"));
    }

    #[test]
    fn partial_lot_retains_quality_and_scales_every_flavor() {
        let mut lot = FoodLot {
            id: 1,
            inventory_item_id: Some(2),
            party_inventory_item_id: None,
            display_name: "Roasted test".into(),
            preparation: FoodPreparation::Roasted,
            ingredient_item_ids: vec!["salt".into()],
            ingredient_quantities: vec![1.0],
            salty_kg: 1.0,
            spicy_kg: 0.8,
            sweet_kg: 0.6,
            sour_kg: 0.4,
            savory_kg: 0.2,
            quality: 4,
            mass_kg: 1.0,
            nutrition_kcal: 100.0,
            total_value: 10.0,
            created_at_minute: 0,
        };
        retain_lot_fraction(&mut lot, 0.25);
        assert_eq!(lot.quality, 4);
        assert_eq!(lot.salty_kg, 0.25);
        assert_eq!(lot.spicy_kg, 0.2);
        assert_eq!(lot.sweet_kg, 0.15);
        assert_eq!(lot.sour_kg, 0.1);
        assert_eq!(lot.savory_kg, 0.05);
    }

    #[test]
    fn stew_water_and_fireplace_escrow_contract_are_explicit() {
        assert_eq!(
            stew_water_required_ml(&[crate::inventory_amount::FULL_AMOUNT_MILLIUNITS]),
            Some(600.0)
        );
        let source = include_str!("food.rs");
        let cook = source
            .split("pub fn add_fireplace_ingredients")
            .nth(1)
            .and_then(|tail| tail.split("pub fn retrieve_fireplace_dish").next())
            .expect("fireplace ingredient reducer source");
        assert!(cook.contains("mass += water_ml / 1_000.0"));
        assert!(cook.contains("food::microbial_concentration(loads.iter().sum(), mass)"));
        assert!(cook.contains("if method == CookingMethod::Stew"));
        assert!(cook.contains("ctx.db.fireplace_dish().insert"));
        assert!(!cook.contains("advance_character_wait_time"));
        assert!(!cook.contains("consume_food_amount(ctx, character_id"));
        assert!(cook.contains("pan_fry_has_enough_fat"));
        assert!(cook.contains("chef_quality_tier"));
    }

    #[test]
    fn fireplace_authority_is_private_location_bound_and_race_safe() {
        let source = include_str!("food.rs");
        assert!(source.contains("#[table(accessor = fireplace_station)]"));
        assert!(source.contains("#[table(accessor = fireplace_dish)]"));
        assert!(source.contains("#[view(accessor = backend_fireplace_stations, public)]"));
        assert!(source.contains("#[view(accessor = backend_fireplace_dishes, public)]"));
        let dish_projection = source
            .split("pub struct BackendFireplaceDish")
            .nth(1)
            .and_then(|tail| {
                tail.split("#[view(accessor = backend_fireplace_dishes")
                    .next()
            })
            .expect("gateway dish projection");
        assert!(!dish_projection.contains("raw_contamination"));
        assert!(source.contains("journey.departure_minute != departure"));
        assert!(source.contains("journey.completed_minutes != movement"));
        assert!(source.contains("This fireplace already holds a dish"));
        assert!(source.contains("Food lot selections must be unique and positive"));
        assert!(source.contains("Retrieve the current dish before changing instruments"));
        assert!(source.contains("original party inventory is no longer available"));
        assert!(source.contains("instrument_party_id"));
    }

    #[test]
    fn camp_departure_and_retrieval_cleanup_enforce_fireplace_custody() {
        let food_source = include_str!("food.rs");
        let travel_source = include_str!("strategic/travel_reducers.rs");
        assert!(travel_source.contains("require_clear_current_camp_fireplace"));
        assert!(food_source.contains(
            "Retrieve every dish and remove every cooking instrument before breaking camp"
        ));
        let retrieval = food_source
            .split("pub fn retrieve_fireplace_dish")
            .nth(1)
            .and_then(|tail| tail.split("pub fn preview_cooking").next())
            .expect("dish retrieval reducer");
        assert!(retrieval.contains("fireplace_station().key().delete"));
        assert!(!retrieval.contains("train_skill"));
        assert!(!retrieval.contains("morale"));
    }

    #[test]
    fn party_exit_and_death_have_explicit_fireplace_custody_policy() {
        let food_source = include_str!("food.rs");
        let party_source = include_str!("strategic/inventory_trade.rs");
        let character_source = include_str!("character.rs");
        let removal = party_source
            .split("pub fn remove_party_member")
            .nth(1)
            .and_then(|tail| tail.split("pub fn disband_party").next())
            .expect("party member removal reducer");
        let disband = party_source
            .split("pub fn disband_party")
            .nth(1)
            .expect("party disband reducer");
        assert!(removal.contains("require_members_clear_current_camp_fireplace"));
        assert!(disband.contains("require_members_clear_current_camp_fireplace"));
        assert!(
            character_source
                .contains("crate::food::cleanup_fireplace_custody_for_death(ctx, character_id)")
        );

        let cleanup = food_source
            .split("pub(crate) fn cleanup_fireplace_custody_for_death")
            .nth(1)
            .and_then(|tail| tail.split("fn validate_fireplace_context").next())
            .expect("death fireplace cleanup");
        assert!(cleanup.contains("fireplace_dish()"));
        assert!(cleanup.contains(".character_id()"));
        assert!(cleanup.contains("add_to_party_inventory_checked"));
        assert!(cleanup.contains("ctx.db.inventory_item().insert"));
        assert!(cleanup.contains("fireplace_station().key().delete"));
    }

    #[test]
    fn catalog_quality_is_copied_when_lots_are_acquired() {
        let source = include_str!("food.rs");
        let constructor = source
            .split("pub fn create_personal_food_lot")
            .nth(1)
            .and_then(|tail| tail.split("pub fn create_party_food_lot").next())
            .expect("personal lot constructor");
        assert!(constructor.contains("quality: definition.default_quality.clamp(1, 5)"));
    }

    #[test]
    fn hidden_food_contamination_uses_explicit_food_water_prevention() {
        let source = include_str!("food.rs");
        let exposure = source
            .split("fn expose_to_dysentery")
            .nth(1)
            .and_then(|tail| tail.split("fn consume_food_amount").next())
            .expect("foodborne exposure source");
        assert!(exposure.contains("protected_point_exposure"));
        assert!(exposure.contains("TransmissionVector::FoodWater"));
        assert!(exposure.contains("protected_dose"));
    }
}
