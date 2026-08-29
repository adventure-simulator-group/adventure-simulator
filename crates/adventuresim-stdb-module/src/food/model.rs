// Owns the cohesive food-lot, contamination, preparation-receipt, and fireplace schema.
/// Public, inspectable description of one non-fungible inventory batch.
#[derive(Clone, Debug)]
#[table(accessor = food_lot, public)]
pub struct FoodLot {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub inventory_item_id: Option<u64>,
    pub party_inventory_item_id: Option<u64>,
    #[index(btree)]
    pub material_revision: u64,
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

/// Private source-material provenance carried through cooking into consumption.
#[derive(Clone, Debug)]
#[table(accessor = food_contamination_provenance)]
pub struct FoodContaminationProvenance {
    #[primary_key]
    pub food_lot_id: u64,
    pub contribution_ids: Vec<String>,
    pub contribution_loads: Vec<f32>,
    pub contribution_digest: String,
}

#[derive(Clone, Debug)]
#[table(accessor = ingredient_preparation_receipt)]
pub struct IngredientPreparationReceipt {
    #[primary_key]
    pub request_id: String,
    pub actor_character_id: u64,
    pub inventory_scope: String,
    pub inventory_item_id: u64,
    pub food_lot_id: u64,
    pub material_object_id: u64,
    pub expected_revision: u64,
    pub attempt_generation: u64,
    pub action: IngredientPreparationAction,
    pub canonical_place: String,
    pub custody_binding: String,
    pub authority_input_digest: String,
    pub duration_minutes: u32,
    pub interrupted: bool,
    pub resulting_revision: u64,
    pub material_input_digest: String,
}

/// Minimal server-owned liveness cursor. It lets the gateway issue a fresh
/// request after a clipped terminal attempt without exposing private receipts.
#[derive(Clone, Debug)]
#[table(accessor = ingredient_preparation_attempt_state)]
pub struct IngredientPreparationAttemptState {
    #[primary_key]
    pub key: String,
    pub next_generation: u64,
    pub completed: bool,
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
    /// Canonical `StrategicFixtureId::Fireplace` encoding.
    pub fireplace_fixture_id: String,
    pub instrument_item_id: Option<String>,
    /// Stable root object for a placed cooking vessel. `None` is the loose
    /// spit-roast lane or an empty station.
    pub instrument_object_id: Option<u64>,
    /// Exact immutable carried custody to which removal returns the tool.
    pub instrument_return_custody: Option<crate::PersistedOperationalCustody>,
}

#[derive(Clone, Debug)]
#[table(accessor = fireplace_dish)]
pub struct FireplaceDish {
    #[primary_key]
    pub station_key: String,
    #[index(btree)]
    pub character_id: u64,
    /// Canonical `StrategicFixtureId::Fireplace` encoding shared with its station.
    pub fireplace_fixture_id: String,
    /// Immutable operational return custody captured before ingredients are consumed.
    pub return_custody: crate::PersistedOperationalCustody,
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
    pub contamination_contribution_ids: Vec<String>,
    pub contamination_contribution_loads: Vec<f32>,
    pub contamination_contribution_digest: String,
    pub medicinal_profile_ids: Vec<String>,
    pub medicinal_profile_versions: Vec<u16>,
    pub medicinal_potency_units: Vec<f32>,
}
