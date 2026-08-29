//! Authoritative measured food lots and immediate free-form cooking.

use adventuresim_core::{
    disease::{self, DiseaseId},
    durability::{DamageBins, effective_weapon_stat},
    food::{self, CookingMethod, FoodPreparation, IngredientPreparationAction},
    herbalism,
    inventory_measurement::ConsumableFractionMicros,
    material::Microliters,
    physical_object::{
        CarriedInventoryScope, InventoryLocation, OperationalCustody, PhysicalObjectId,
    },
    prelude::{PlayerSkills, Skill, apply_direct_training},
    strategic_place::{StrategicFixtureId, StrategicPlaceId},
};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::{
    character::{
        character, character__view as _, character_attributes, character_attributes__view as _,
        character_limbs, character_skills, character_skills__view as _,
    },
    condition::{character_needs, initialize_character_condition},
    container_liquid,
    inventory_container::{inventory_containment__view as _, inventory_object__view as _},
    inventory_containment, inventory_item, inventory_item_amount, inventory_object,
    item::{inventory_item__view as _, item, item__view as _},
    medicinal_component, party_item_amount,
    repair::{item_condition, item_condition__view as _},
    strategic::{
        PartyInventoryItem, StrategicEncounterStatus, party_authority, party_authority__view as _,
        party_inventory_item, party_inventory_item__view as _, party_item_condition,
        party_item_condition__view as _, party_journey_authority,
        road_challenge_authority__view as _, settlement, strategic_encounter__view as _,
    },
    time::{character_time, character_time__view as _},
};

// Assembles the ordered food owners in their existing SpacetimeDB module scope.
include!("model.rs");
include!("ingredient_preparation.rs");
include!("preparation_authority.rs");
include!("projections.rs");
include!("lot_inventory.rs");
include!("fireplace_custody.rs");
include!("cooking.rs");
include!("consumption.rs");

#[cfg(test)]
pub(crate) const FOOD_SOURCE: &str = concat!(
    include_str!("model.rs"),
    include_str!("ingredient_preparation.rs"),
    include_str!("preparation_authority.rs"),
    include_str!("projections.rs"),
    include_str!("lot_inventory.rs"),
    include_str!("fireplace_custody.rs"),
    include_str!("cooking.rs"),
    include_str!("consumption.rs"),
    include_str!("mod.rs"),
);

#[cfg(test)]
mod tests {
    use super::*;

    mod preparation {
        use super::*;
        include!("tests/preparation.rs");
    }

    mod lots_and_consumption {
        use super::*;
        include!("tests/lots_and_consumption.rs");
    }

    mod fireplace_and_cooking {
        use super::*;
        include!("tests/fireplace_and_cooking.rs");
    }
}
