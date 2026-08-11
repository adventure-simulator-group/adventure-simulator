use adventuresim_core::starting_character::{
    StartingAgeTier, StartingCharacterSpec, StartingInclination, StartingPersonalityTrait,
    StartingPresentation, StartingSex, StartingSlot,
};
use spacetimedb::{
    Identity, ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view,
};
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::{
    ItemSlot, Settlement, add_inventory_item,
    alcohol::alcohol_consumption,
    capability::{character_capability, character_capability__view},
    condition::{
        character_condition, character_condition__view, character_exposure,
        character_exposure__view, character_morale_source, character_morale_source__view,
        character_needs, character_needs__view, character_strategic_condition,
        character_strategic_condition__view, morale_event, religious_demand,
    },
    disease::{
        character_illness_status, committed_cut, disease_notice, infection_episode,
        physiology_administration,
    },
    enter_mission, inventory_item,
    item::item,
    organization::{organization_membership, organization_presentation},
    personality::{
        character_personality, character_personality_scores, personality_development_event,
    },
    relationship::{child_identity_reservation, npc_policy},
    repair::{item_condition, repair_order},
    settlement_population::settlement_resident_presence,
    strategic::{
        inventory_quantity_target, party_authority, party_member, settlement, strategic_encounter,
        strategic_gateway_authority__view,
    },
    surgery::{limb_injury, retained_projectile},
    tactical::tactical_server_authority,
    time::{
        character_time, character_time__view, character_training_schedule,
        character_training_schedule__view,
    },
};

/// General character info
#[derive(Clone, Debug)]
#[table(accessor = character)]
pub struct Character {
    #[primary_key]
    pub id: u64,
    /// Private full-table scan seam used only by trusted projections.
    #[index(btree)]
    pub scan_id: u64,
    pub name: String,
    pub xp: u32,
    pub level: u32,
    pub gold: u32,
    pub current_settlement_id: Option<String>,
    pub party_id: Option<String>,
    #[index(btree)]
    pub server: Identity,
    pub in_server: bool,
    pub temporary: bool,
    #[default(25)]
    pub age_years: u16,
    /// Strategic life state. Death transitions are intentionally deferred to the
    /// future death system, but parties already use this to govern succession.
    #[default(true)]
    pub alive: bool,
    /// Explicit ordinary care preference for another member of this
    /// Character's current party. Context-specific refusals take precedence.
    pub party_treatment_decision: crate::world_actor::ContextualDecisionState,
}

/// Full Character rows are private because a globally exclusive NPC may have
/// been born or moved at a date the requesting player has not reached. The
/// strategic gateway remains the sole broad reader and applies actor-specific
/// chronological filtering in its public presentations.
#[view(accessor = backend_characters, public)]
pub fn backend_characters(ctx: &ViewContext) -> Vec<Character> {
    if !character_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db.character().scan_id().filter(0u64..).collect()
}

fn character_view_is_gateway(ctx: &ViewContext) -> bool {
    ctx.db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|authority| authority.identity == ctx.sender())
}

fn gateway_character_ids(ctx: &ViewContext) -> Vec<u64> {
    if !character_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .character()
        .scan_id()
        .filter(0u64..)
        .map(|character| character.id)
        .collect()
}

#[view(accessor = backend_character_attributes, public)]
pub fn backend_character_attributes(ctx: &ViewContext) -> Vec<CharacterAttributes> {
    gateway_character_ids(ctx)
        .into_iter()
        .filter_map(|id| ctx.db.character_attributes().character_id().find(id))
        .collect()
}

#[view(accessor = backend_character_stats, public)]
pub fn backend_character_stats(ctx: &ViewContext) -> Vec<CharacterStats> {
    gateway_character_ids(ctx)
        .into_iter()
        .filter_map(|id| ctx.db.character_stats().character_id().find(id))
        .collect()
}

#[view(accessor = backend_character_skills, public)]
pub fn backend_character_skills(ctx: &ViewContext) -> Vec<CharacterSkills> {
    gateway_character_ids(ctx)
        .into_iter()
        .filter_map(|id| ctx.db.character_skills().character_id().find(id))
        .collect()
}

#[view(accessor = backend_character_limbs, public)]
pub fn backend_character_limbs(ctx: &ViewContext) -> Vec<CharacterLimbs> {
    gateway_character_ids(ctx)
        .into_iter()
        .filter_map(|id| ctx.db.character_limbs().character_id().find(id))
        .collect()
}

#[view(accessor = backend_character_times, public)]
pub fn backend_character_times(ctx: &ViewContext) -> Vec<crate::time::CharacterTime> {
    gateway_character_ids(ctx)
        .into_iter()
        .filter_map(|id| ctx.db.character_time().character_id().find(id))
        .collect()
}

#[view(accessor = backend_character_training_schedules, public)]
pub fn backend_character_training_schedules(
    ctx: &ViewContext,
) -> Vec<crate::time::CharacterTrainingSchedule> {
    gateway_character_ids(ctx)
        .into_iter()
        .filter_map(|id| ctx.db.character_training_schedule().character_id().find(id))
        .collect()
}

#[view(accessor = backend_character_capabilities, public)]
pub fn backend_character_capabilities(
    ctx: &ViewContext,
) -> Vec<crate::capability::CharacterCapability> {
    gateway_character_ids(ctx)
        .into_iter()
        .filter_map(|id| ctx.db.character_capability().character_id().find(id))
        .collect()
}

#[view(accessor = backend_character_conditions, public)]
pub fn backend_character_conditions(
    ctx: &ViewContext,
) -> Vec<crate::condition::CharacterCondition> {
    gateway_character_ids(ctx)
        .into_iter()
        .filter_map(|id| ctx.db.character_condition().character_id().find(id))
        .collect()
}

#[view(accessor = backend_character_needs, public)]
pub fn backend_character_needs(ctx: &ViewContext) -> Vec<crate::condition::CharacterNeeds> {
    gateway_character_ids(ctx)
        .into_iter()
        .filter_map(|id| ctx.db.character_needs().character_id().find(id))
        .collect()
}

#[view(accessor = backend_character_exposures, public)]
pub fn backend_character_exposures(ctx: &ViewContext) -> Vec<crate::condition::CharacterExposure> {
    gateway_character_ids(ctx)
        .into_iter()
        .filter_map(|id| ctx.db.character_exposure().character_id().find(id))
        .collect()
}

#[view(accessor = backend_character_strategic_conditions, public)]
pub fn backend_character_strategic_conditions(
    ctx: &ViewContext,
) -> Vec<crate::condition::CharacterStrategicCondition> {
    gateway_character_ids(ctx)
        .into_iter()
        .filter_map(|id| {
            ctx.db
                .character_strategic_condition()
                .character_id()
                .find(id)
        })
        .collect()
}

/// Morale source labels can reveal private religion, household, and romantic
/// state (including the spouse-leisure source). Only the trusted gateway may
/// read the broad projection; browser handlers must still scope rows to the
/// selected character or an authorized party member.
#[view(accessor = backend_character_morale_sources, public)]
pub fn backend_character_morale_sources(
    ctx: &ViewContext,
) -> Vec<crate::condition::CharacterMoraleSource> {
    let mut rows = Vec::new();
    for id in gateway_character_ids(ctx) {
        rows.extend(ctx.db.character_morale_source().character_id().filter(id));
    }
    rows
}

/// Durable receipt for an idempotent first-character confirmation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum StartingAgeTierCoordinate {
    Young,
    Adult,
    Old,
}

impl StartingAgeTierCoordinate {
    fn core(self) -> StartingAgeTier {
        match self {
            Self::Young => StartingAgeTier::Young,
            Self::Adult => StartingAgeTier::Adult,
            Self::Old => StartingAgeTier::Old,
        }
    }
}

/// Durable receipt for an idempotent first-character confirmation.
#[derive(Clone, Debug)]
#[table(accessor = starting_character_claim)]
pub struct StartingCharacterClaim {
    #[primary_key]
    pub request_key: String,
    #[unique]
    pub character_id: u64,
    #[index(btree)]
    pub owner_key: String,
    pub generator_version: u16,
    pub seed: String,
    pub age_tier: StartingAgeTierCoordinate,
    pub slot: u8,
}

#[derive(Clone, Debug, SpacetimeType)]
pub enum DeathCause {
    Combat,
    Injury,
    Disease,
    RespiratoryFailure,
    CirculatoryFailure,
    HomeostaticFailure,
    NeurologicFailure,
    Starvation,
    Dehydration,
    Other,
    DevTest,
}

#[derive(Clone, Debug, SpacetimeType)]
pub enum DeathSource {
    Tactical,
    Autoresolve,
    Strategic,
    Disease,
    DevTest,
}

/// Immutable first-known death context. Tactical state is deliberately absent;
/// committed outcomes pass only their durable cause/source into this boundary.
#[derive(Clone, Debug)]
#[table(accessor = character_death)]
pub struct CharacterDeath {
    #[primary_key]
    pub character_id: u64,
    pub cause: DeathCause,
    pub source: DeathSource,
    pub source_id: Option<String>,
    pub strategic_minute: u64,
}

/// Death authority is private because its timestamp may lie beyond another
/// character's personal frontier. Strategic simulation consumes this trusted
/// broad projection. Any player-facing presentation must additionally compare
/// `strategic_minute` with the selected observer's CharacterTime.
#[view(accessor = backend_character_deaths, public)]
pub fn backend_character_deaths(ctx: &ViewContext) -> Vec<CharacterDeath> {
    if !character_view_is_gateway(ctx) {
        return Vec::new();
    }
    gateway_character_ids(ctx)
        .into_iter()
        .filter_map(|id| ctx.db.character_death().character_id().find(id))
        .collect()
}

pub fn require_living_character(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<Character, String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if !character.alive {
        return Err("Dead characters cannot perform this action".into());
    }
    Ok(character)
}

/// Authoritative, idempotent life-state transition shared by strategic disease
/// and committed tactical/autoresolve outcomes. Repeated calls retain the first
/// recorded cause, source, and strategic minute.
pub fn transition_character_to_dead(
    ctx: &ReducerContext,
    character_id: u64,
    cause: DeathCause,
    source: DeathSource,
    source_id: Option<String>,
) -> Result<CharacterDeath, String> {
    let strategic_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |time| time.minutes);
    transition_character_to_dead_at(
        ctx,
        character_id,
        cause,
        source,
        source_id,
        strategic_minute,
    )
}

/// Exact-minute variant for materializing already-elapsed authored history.
/// The character's clock is never rewound; a future death is rejected.
pub fn transition_character_to_dead_at(
    ctx: &ReducerContext,
    character_id: u64,
    cause: DeathCause,
    source: DeathSource,
    source_id: Option<String>,
    strategic_minute: u64,
) -> Result<CharacterDeath, String> {
    if let Some(death) = ctx.db.character_death().character_id().find(character_id) {
        crate::food::cleanup_fireplace_custody_for_death(ctx, character_id)?;
        return Ok(death);
    }
    let mut character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let current_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |time| time.minutes);
    if strategic_minute > current_minute {
        return Err("Character death cannot be after their personal clock".into());
    }
    let death = ctx.db.character_death().insert(CharacterDeath {
        character_id,
        cause,
        source,
        source_id,
        strategic_minute,
    });
    crate::food::cleanup_fireplace_custody_for_death(ctx, character_id)?;
    crate::corpse::persist_character_death_corpse(
        ctx,
        character_id,
        death.source_id.as_deref().unwrap_or("character-death"),
        strategic_minute,
    )?;
    crate::social::settle_shared_party_time(ctx, character_id);
    crate::social::close_physiology_presence(ctx, character_id);
    character.alive = false;
    if let Some(mut presence) = ctx
        .db
        .settlement_resident_presence()
        .character_id()
        .find(character_id)
    {
        presence.context_suppressed = false;
        presence.health_suppressed = true;
        ctx.db
            .settlement_resident_presence()
            .character_id()
            .update(presence);
    }
    // Keep an active tactical assignment until that server commits or removes
    // the character. This lets mission teardown find and cascade temporary
    // combatants that died in transient tactical state.
    let party_id = character.party_id.clone();
    ctx.db.character().id().update(character);
    crate::browser_session::clear_dead_character_selection(ctx, character_id);
    crate::continuity::settle_pending_inheritances_for_heir(ctx, character_id, strategic_minute)?;
    crate::continuity::record_estate_disposition_for_death(ctx, character_id, strategic_minute)?;
    crate::relationship::settle_relationship_lifecycle_for_death(
        ctx,
        character_id,
        strategic_minute,
    )?;
    crate::social::prune_invalid_automatic_social_chats(ctx);
    if let Some(party_id) = party_id {
        crate::strategic::normalize_and_elect_party_leader(ctx, &party_id)?;
    }
    Ok(death)
}

/// Non-destructive upgrade path for legacy rows that predate durable death
/// context and standing vote normalization.
#[reducer]
pub fn backfill_character_deaths_and_leadership(ctx: &ReducerContext) -> Result<(), String> {
    let dead_ids: Vec<_> = ctx
        .db
        .character()
        .iter()
        .filter(|character| !character.alive)
        .map(|character| character.id)
        .collect();
    for character_id in dead_ids {
        transition_character_to_dead(
            ctx,
            character_id,
            DeathCause::Other,
            DeathSource::Strategic,
            Some("legacy-backfill".into()),
        )?;
    }
    let party_ids: Vec<_> = ctx
        .db
        .party_authority()
        .iter()
        .map(|party| party.id)
        .collect();
    for party_id in party_ids {
        crate::strategic::normalize_and_elect_party_leader(ctx, &party_id)?;
    }
    Ok(())
}

/// [`Character`] attributes
#[derive(Clone, Debug)]
#[table(accessor = character_attributes)]
pub struct CharacterAttributes {
    #[primary_key]
    pub character_id: u64,
    pub endurance: f32,
    pub immunity: f32,
    pub gut: f32,
    pub intelligence: f32,
    pub instinct: f32,
    pub eyesight: f32,
    pub hearing: f32,
    pub left_arm_strength: f32,
    pub right_arm_strength: f32,
    pub left_leg_strength: f32,
    pub right_leg_strength: f32,
    pub left_arm_agility: f32,
    pub right_arm_agility: f32,
    pub left_leg_agility: f32,
    pub right_leg_agility: f32,
}

/// [`Character`] stats
#[derive(Clone, Debug)]
#[table(accessor = character_stats)]
pub struct CharacterStats {
    #[primary_key]
    pub character_id: u64,
    pub calories_used: f32,
    pub focus: f32,
}

/// [`Character`] skills
#[derive(Clone, Debug)]
#[table(accessor = character_skills)]
pub struct CharacterSkills {
    #[primary_key]
    pub character_id: u64,
    pub polearm_hours: f32,
    pub axe_hours: f32,
    pub bludgeon_hours: f32,
    pub sword_hours: f32,
    pub knife_hours: f32,
    pub dodge_hours: f32,
    pub block_hours: f32,
    pub bow_hours: f32,
    pub crossbow_hours: f32,
    pub firearm_hours: f32,
    pub throw_hours: f32,
    pub will_hours: f32,
    pub insight_hours: f32,
    pub charm_hours: f32,
    pub command_hours: f32,
    pub deception_hours: f32,
    pub physiology_hours: f32,
    pub cooking_hours: f32,
    pub herbalism_hours: f32,
    pub religion_hours: adventuresim_world_schema::ReligionHours,
    pub bestiary_hours: adventuresim_world_schema::BestiaryHours,
    pub oral_languages: adventuresim_world_schema::OralLanguageHours,
    pub written_languages: adventuresim_world_schema::WrittenLanguageHours,
    pub stealth_hours: f32,
    pub balance_hours: f32,
    pub terrain_plains_hours: f32,
    pub terrain_forest_hours: f32,
    pub terrain_hills_hours: f32,
    pub terrain_wetlands_hours: f32,
    pub terrain_urban_hours: f32,
    pub terrain_snow_hours: f32,
    pub surgery_hours: f32,
    pub tailoring_hours: f32,
    pub smithing_hours: f32,
}

/// [`Character`] limbs
#[derive(Clone, Debug)]
#[table(accessor = character_limbs)]
pub struct CharacterLimbs {
    #[primary_key]
    pub character_id: u64,
    pub left_arm_health: f32,
    pub right_arm_health: f32,
    pub left_leg_health: f32,
    pub right_leg_health: f32,
    pub head_health: f32,
    pub chest_health: f32,
    pub stomach_health: f32,
}

/// One normalized equipped inventory instance. The occupancy rows are the
/// authoritative body anchors and selected item-attachment edges.
#[derive(Clone, Debug)]
#[table(
    accessor = character_equipped_item, public,
    index(accessor = character_id, btree(columns = [character_id]))
)]
pub struct CharacterEquippedItem {
    #[primary_key]
    pub inventory_item_id: u64,
    pub character_id: u64,
    pub placement_id: String,
}

#[derive(spacetimedb::SpacetimeType, Clone, Debug, PartialEq, Eq)]
pub struct EquipmentAttachmentTargetSelection {
    pub requirement_index: u16,
    pub parent_inventory_item_id: u64,
    pub attachment_point_id: String,
}

#[derive(spacetimedb::SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum EquipmentAnchorKind {
    CharacterLocation,
    ItemAttachment,
}

#[derive(Clone, Debug)]
#[table(
    accessor = equipment_occupancy, public,
    index(accessor = character_id, btree(columns = [character_id])),
    index(accessor = inventory_item_id, btree(columns = [inventory_item_id]))
)]
pub struct EquipmentOccupancy {
    /// Stable composite key for either a character anchor/channel/order cell
    /// or one capacity cell on an item-provided attachment point.
    #[primary_key]
    pub id: String,
    pub character_id: u64,
    pub inventory_item_id: u64,
    pub anchor_kind: EquipmentAnchorKind,
    pub location: Option<crate::item::EquipmentLocation>,
    pub parent_inventory_item_id: Option<u64>,
    pub attachment_point_id: Option<String>,
    pub channel: crate::item::EquipmentChannel,
    pub order: u16,
    pub requirement_index: u16,
    pub capacity_index: u16,
}

fn character_occupancy_id(
    character_id: u64,
    channel: crate::item::EquipmentChannel,
    order: u16,
    location: crate::item::EquipmentLocation,
) -> String {
    let order = if channel.singleton_per_location() {
        0
    } else {
        order
    };
    format!(
        "character:{character_id}:{location:?}:{}:{order}",
        channel.order()
    )
}

fn attachment_occupancy_id(
    parent_inventory_item_id: u64,
    attachment_point_id: &str,
    capacity_index: u16,
) -> String {
    format!("item:{parent_inventory_item_id}:{attachment_point_id}:{capacity_index}")
}

fn attachment_would_create_cycle(
    inventory_item_id: u64,
    parent_inventory_item_ids: impl IntoIterator<Item = u64>,
    mut parents_of: impl FnMut(u64) -> Vec<u64>,
) -> bool {
    let mut stack = parent_inventory_item_ids
        .into_iter()
        .map(|id| (id, false))
        .collect::<Vec<_>>();
    let mut active = std::collections::HashSet::new();
    let mut finished = std::collections::HashSet::new();
    while let Some((ancestor_id, exiting)) = stack.pop() {
        if ancestor_id == inventory_item_id {
            return true;
        }
        if exiting {
            active.remove(&ancestor_id);
            finished.insert(ancestor_id);
            continue;
        }
        if finished.contains(&ancestor_id) {
            continue;
        }
        if !active.insert(ancestor_id) {
            return true;
        }
        stack.push((ancestor_id, true));
        stack.extend(
            parents_of(ancestor_id)
                .into_iter()
                .map(|parent_id| (parent_id, false)),
        );
    }
    false
}

fn conflicting_root_requirements(
    requirements: &[crate::item::EquipmentOccupancyRequirement],
    inventory_item_id: u64,
    mut occupant_at: impl FnMut(crate::item::EquipmentOccupancyRequirement) -> Option<u64>,
) -> Vec<crate::item::EquipmentOccupancyRequirement> {
    requirements
        .iter()
        .copied()
        .filter(|requirement| {
            occupant_at(*requirement).is_some_and(|occupant| occupant != inventory_item_id)
        })
        .collect()
}

fn first_free_attachment_capacity(
    capacity: u16,
    inventory_item_id: u64,
    mut occupant_at: impl FnMut(u16) -> Option<u64>,
) -> Option<u16> {
    (0..capacity)
        .find(|index| occupant_at(*index).is_none_or(|occupant| occupant == inventory_item_id))
}

fn attachment_point_matches_requirement(
    point: &crate::item::EquipmentAttachmentPoint,
    requirement: crate::item::EquipmentParentRequirement,
) -> bool {
    point.channel == requirement.channel && point.order == requirement.order
}

pub(crate) fn wearable_is_equipped(ctx: &ReducerContext, inventory_item_id: u64) -> bool {
    ctx.db
        .character_equipped_item()
        .inventory_item_id()
        .find(inventory_item_id)
        .is_some()
}

pub(crate) fn inventory_item_is_equipped(
    ctx: &ReducerContext,
    _character_id: u64,
    inventory_item_id: u64,
) -> bool {
    wearable_is_equipped(ctx, inventory_item_id)
}

pub(crate) fn equipped_wearable_ids(ctx: &ReducerContext, character_id: u64) -> Vec<u64> {
    ctx.db
        .character_equipped_item()
        .character_id()
        .filter(character_id)
        .map(|row| row.inventory_item_id)
        .collect()
}

pub(crate) fn outermost_wearable_for_body_part(
    ctx: &ReducerContext,
    character_id: u64,
    part: adventuresim_core::body::BodyPart,
) -> Option<u64> {
    ctx.db
        .character_equipped_item()
        .character_id()
        .filter(character_id)
        .filter_map(|equipped| {
            let inventory = ctx
                .db
                .inventory_item()
                .id()
                .find(equipped.inventory_item_id)?;
            let definition = ctx.db.item().id().find(&inventory.item_id)?;
            let placement = definition
                .equipment_placements
                .iter()
                .find(|placement| placement.id == equipped.placement_id)?;
            if !placement
                .protection
                .iter()
                .any(|target| runtime_body_part(*target) == part)
            {
                return None;
            }
            let outer = ctx
                .db
                .equipment_occupancy()
                .inventory_item_id()
                .filter(equipped.inventory_item_id)
                .max_by_key(|row| (row.channel.order(), row.order, row.capacity_index));
            let (channel_order, order) =
                outer.map_or((0, 0), |row| (row.channel.order(), row.order));
            Some((channel_order, order, equipped.inventory_item_id))
        })
        .max()
        .map(|(_, _, inventory_item_id)| inventory_item_id)
}

pub(crate) fn unequip_wearable(ctx: &ReducerContext, inventory_item_id: u64) {
    let children: Vec<_> = ctx
        .db
        .equipment_occupancy()
        .iter()
        .filter(|row| row.parent_inventory_item_id == Some(inventory_item_id))
        .map(|row| row.inventory_item_id)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    for child in children {
        unequip_wearable(ctx, child);
    }
    let occupancies: Vec<_> = ctx
        .db
        .equipment_occupancy()
        .inventory_item_id()
        .filter(inventory_item_id)
        .collect();
    for occupancy in occupancies {
        ctx.db.equipment_occupancy().id().delete(occupancy.id);
    }
    ctx.db
        .character_equipped_item()
        .inventory_item_id()
        .delete(inventory_item_id);
}

fn runtime_body_part(part: crate::item::EquipmentBodyPart) -> adventuresim_core::body::BodyPart {
    use crate::item::EquipmentBodyPart as E;
    use adventuresim_core::body::BodyPart as B;
    match part {
        E::LeftArm => B::LeftArm,
        E::RightArm => B::RightArm,
        E::LeftLeg => B::LeftLeg,
        E::RightLeg => B::RightLeg,
        E::Chest => B::Chest,
        E::Stomach => B::Stomach,
        E::Head => B::Head,
    }
}

pub(crate) fn require_no_equipped_children(
    ctx: &ReducerContext,
    inventory_item_id: u64,
) -> Result<(), String> {
    if ctx
        .db
        .equipment_occupancy()
        .iter()
        .any(|row| row.parent_inventory_item_id == Some(inventory_item_id))
    {
        Err("Detach or remove attached/contained items first".into())
    } else {
        Ok(())
    }
}

pub(crate) fn restore_equipment_placement(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
    placement_id: &str,
    targets: Vec<EquipmentAttachmentTargetSelection>,
) -> Result<(), String> {
    let inventory = ctx
        .db
        .inventory_item()
        .id()
        .find(inventory_item_id)
        .ok_or("Inventory item not found while restoring equipment")?;
    let definition = ctx
        .db
        .item()
        .id()
        .find(&inventory.item_id)
        .ok_or("Item definition not found while restoring equipment")?;
    let placement_index = definition
        .equipment_placements
        .iter()
        .position(|placement| placement.id == placement_id)
        .and_then(|index| u16::try_from(index).ok())
        .ok_or("Saved equipment placement is no longer authored")?;
    equip_equipment_internal(
        ctx,
        character_id,
        inventory_item_id,
        placement_index,
        targets,
        true,
        false,
    )
}

fn refresh_equipment_dependents(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    crate::capability::refresh_character_capability(ctx, character_id)?;
    crate::condition::refresh_character_strategic_condition(ctx, character_id).map(|_| ())
}

/// Create a new random temporary character for the server.
#[reducer]
pub fn create_temporary_character(ctx: &ReducerContext, server: Identity) -> Result<(), String> {
    use petname::Generator;

    let tactical_server = ctx.db.tactical_server_authority().identity().find(server);
    if ctx.sender() != server || tactical_server.is_none() {
        return Err("Only a registered tactical server can create its temporary characters".into());
    }
    let tactical_server = tactical_server.expect("checked tactical server");

    let name = petname::Petnames::default()
        .generate(&mut ctx.rng(), 1, " ")
        .ok_or_else(|| "Can't generate a name for a temporary character".to_string())?;
    let name = format!("bot-{name}");

    let mut id = ctx.random();
    id |= 0b1000_1000_1000_1000;

    insert_new_character(ctx, name, id, true)?;
    scale_temporary_enemy(
        ctx,
        id,
        tactical_server.enemy_difficulty,
        tactical_server.enemy_combat_scale_bps,
        tactical_server.countermeasure_multiplier_bps,
    )?;
    enter_mission(ctx, id, server)
}

fn scale_temporary_enemy(
    ctx: &ReducerContext,
    character_id: u64,
    base_difficulty: i32,
    combat_scale_bps: u32,
    countermeasure_multiplier_bps: u32,
) -> Result<(), String> {
    let authored = 1.0 + (base_difficulty.clamp(1, 20) - 1) as f32 * 0.1;
    let (physical_scale, training_scale) =
        adventuresim_core::threat_escalation::combat_scaling_multipliers(
            combat_scale_bps,
            countermeasure_multiplier_bps,
        );
    let physical = authored * physical_scale;
    let training = authored * training_scale;
    let mut attributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .ok_or("Temporary enemy attributes are missing")?;
    for value in [
        &mut attributes.endurance,
        &mut attributes.immunity,
        &mut attributes.gut,
        &mut attributes.instinct,
        &mut attributes.eyesight,
        &mut attributes.hearing,
        &mut attributes.left_arm_strength,
        &mut attributes.right_arm_strength,
        &mut attributes.left_leg_strength,
        &mut attributes.right_leg_strength,
        &mut attributes.left_arm_agility,
        &mut attributes.right_arm_agility,
        &mut attributes.left_leg_agility,
        &mut attributes.right_leg_agility,
    ] {
        *value *= physical;
    }
    ctx.db
        .character_attributes()
        .character_id()
        .update(attributes);

    let mut limbs = ctx
        .db
        .character_limbs()
        .character_id()
        .find(character_id)
        .ok_or("Temporary enemy limbs are missing")?;
    for health in [
        &mut limbs.left_arm_health,
        &mut limbs.right_arm_health,
        &mut limbs.left_leg_health,
        &mut limbs.right_leg_health,
        &mut limbs.head_health,
        &mut limbs.chest_health,
        &mut limbs.stomach_health,
    ] {
        *health *= physical;
    }
    ctx.db.character_limbs().character_id().update(limbs);

    let mut skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Temporary enemy skills are missing")?;
    for hours in [
        &mut skills.polearm_hours,
        &mut skills.axe_hours,
        &mut skills.bludgeon_hours,
        &mut skills.sword_hours,
        &mut skills.knife_hours,
        &mut skills.dodge_hours,
        &mut skills.block_hours,
        &mut skills.bow_hours,
        &mut skills.crossbow_hours,
        &mut skills.firearm_hours,
        &mut skills.throw_hours,
        &mut skills.will_hours,
        &mut skills.stealth_hours,
        &mut skills.balance_hours,
    ] {
        *hours *= training;
    }
    ctx.db.character_skills().character_id().update(skills);
    Ok(())
}

/// Transactionally delete a temporary tactical character and every durable
/// strategic row that is owned by it. This must run before deleting Character
/// so no orphan can survive a successful reducer commit.
pub(crate) fn delete_temporary_character(
    ctx: &ReducerContext,
    character: Character,
) -> Result<(), String> {
    if !character.temporary {
        return Err("Refusing to cascade-delete a persistent character".into());
    }
    delete_character_data(ctx, character, true)
}

pub(crate) fn delete_character_for_world_import(
    ctx: &ReducerContext,
    character: Character,
) -> Result<(), String> {
    delete_character_data(ctx, character, false)
}

fn delete_character_data(
    ctx: &ReducerContext,
    character: Character,
    delete_party: bool,
) -> Result<(), String> {
    if delete_party {
        if let Some(party_id) = character.party_id.as_deref() {
            crate::strategic::delete_temporary_character_party(ctx, character.id, party_id)?;
        } else {
            for membership in ctx
                .db
                .party_member()
                .character_id()
                .filter(character.id)
                .collect::<Vec<_>>()
            {
                ctx.db.party_member().id().delete(membership.id);
            }
        }
    }

    // Repair custody changes InventoryItem.character_id to zero while retaining
    // the real owner on RepairOrder. Remove those custody rows before scanning
    // ordinary owned inventory so a temporary character cannot leave orphaned
    // smith inventory behind.
    for order in ctx
        .db
        .repair_order()
        .owner_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        if ctx
            .db
            .item_condition()
            .inventory_item_id()
            .find(order.inventory_item_id)
            .is_some()
        {
            ctx.db
                .item_condition()
                .inventory_item_id()
                .delete(order.inventory_item_id);
        }
        ctx.db.inventory_item().id().delete(order.inventory_item_id);
        ctx.db.repair_order().id().delete(order.id);
    }

    let inventory = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character.id)
        .collect::<Vec<_>>();
    for row in inventory {
        if ctx
            .db
            .item_condition()
            .inventory_item_id()
            .find(row.id)
            .is_some()
        {
            ctx.db.item_condition().inventory_item_id().delete(row.id);
        }
        for repair in ctx
            .db
            .repair_order()
            .iter()
            .filter(|repair| repair.inventory_item_id == row.id)
            .collect::<Vec<_>>()
        {
            ctx.db.repair_order().id().delete(repair.id);
        }
        ctx.db.inventory_item().id().delete(row.id);
        crate::food::delete_personal_food_lot(ctx, row.id);
    }
    for row in ctx
        .db
        .physiology_administration()
        .administration_patient_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        ctx.db.physiology_administration().id().delete(row.id);
    }

    for row in ctx
        .db
        .infection_episode()
        .character_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        ctx.db.infection_episode().id().delete(row.id);
    }
    for row in ctx
        .db
        .committed_cut()
        .character_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        ctx.db.committed_cut().id().delete(row.id);
    }
    for row in ctx
        .db
        .disease_notice()
        .character_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        ctx.db.disease_notice().id().delete(&row.id);
    }
    for row in ctx
        .db
        .morale_event()
        .character_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        ctx.db.morale_event().id().delete(row.id);
    }
    for row in ctx
        .db
        .character_morale_source()
        .character_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        ctx.db.character_morale_source().id().delete(&row.id);
    }
    crate::social::cleanup_character_social(ctx, character.id);
    for row in ctx
        .db
        .religious_demand()
        .character_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        ctx.db.religious_demand().id().delete(row.id);
    }
    for row in ctx
        .db
        .inventory_quantity_target()
        .owner_character_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        ctx.db.inventory_quantity_target().id().delete(&row.id);
    }
    for row in ctx
        .db
        .alcohol_consumption()
        .by_character()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        ctx.db.alcohol_consumption().id().delete(&row.id);
    }
    for row in ctx
        .db
        .organization_membership()
        .character_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        ctx.db.organization_membership().id().delete(row.id);
    }
    crate::social_roles::delete_character_social_roles(ctx, character.id);
    if ctx
        .db
        .organization_presentation()
        .character_id()
        .find(character.id)
        .is_some()
    {
        ctx.db
            .organization_presentation()
            .character_id()
            .delete(character.id);
    }

    ctx.db.character_stats().character_id().delete(character.id);
    ctx.db
        .character_skills()
        .character_id()
        .delete(character.id);
    ctx.db.character_time().character_id().delete(character.id);
    ctx.db
        .character_training_schedule()
        .character_id()
        .delete(character.id);
    crate::reputation::delete_character_reputation(ctx, character.id);
    crate::strategic::delete_activity_incident_entropy(ctx, character.id);
    ctx.db.character_limbs().character_id().delete(character.id);
    for row in ctx
        .db
        .character_equipped_item()
        .character_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        unequip_wearable(ctx, row.inventory_item_id);
    }
    ctx.db
        .character_attributes()
        .character_id()
        .delete(character.id);
    ctx.db
        .character_personality()
        .character_id()
        .delete(character.id);
    ctx.db
        .character_personality_scores()
        .character_id()
        .delete(character.id);
    // Development is character-owned audit history in this pre-launch model;
    // canonical deletion removes it so a reused development source cannot
    // point at a character that no longer exists.
    for event in ctx
        .db
        .personality_development_event()
        .character_id()
        .filter(character.id)
        .collect::<Vec<_>>()
    {
        ctx.db
            .personality_development_event()
            .source_id()
            .delete(&event.source_id);
    }
    if ctx
        .db
        .npc_policy()
        .character_id()
        .find(character.id)
        .is_some()
    {
        ctx.db.npc_policy().character_id().delete(character.id);
    }
    ctx.db
        .character_capability()
        .character_id()
        .delete(character.id);
    ctx.db
        .character_condition()
        .character_id()
        .delete(character.id);
    ctx.db.character_needs().character_id().delete(character.id);
    ctx.db
        .character_exposure()
        .character_id()
        .delete(character.id);
    ctx.db
        .character_strategic_condition()
        .character_id()
        .delete(character.id);
    if ctx
        .db
        .character_illness_status()
        .character_id()
        .find(character.id)
        .is_some()
    {
        ctx.db
            .character_illness_status()
            .character_id()
            .delete(character.id);
    }
    if ctx
        .db
        .character_death()
        .character_id()
        .find(character.id)
        .is_some()
    {
        ctx.db.character_death().character_id().delete(character.id);
    }
    ctx.db.character().delete(character);
    Ok(())
}

/// Create a new character with generated name and add initial items to it
#[reducer]
pub fn create_character(ctx: &ReducerContext, id: u64) -> Result<(), String> {
    use petname::Generator;

    let name = petname::Petnames::default()
        .generate(&mut ctx.rng(), 2, " ")
        .ok_or_else(|| format!("Can't generate a name for a character with id {id}"))?;

    insert_new_character(ctx, name, id, false)
}

/// Create a new character with name and add initial items to it
#[reducer]
pub fn create_named_character(ctx: &ReducerContext, name: String) -> Result<(), String> {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    ctx.timestamp.hash(&mut hasher);
    let id = hasher.finish();

    insert_new_character(ctx, name, id, false)
}

/// Create a named character with a caller-supplied ID.
#[reducer]
pub fn create_named_character_with_id(
    ctx: &ReducerContext,
    id: u64,
    name: String,
) -> Result<(), String> {
    insert_new_character(ctx, name, id, false)
}

/// Materialize one previewed candidate. The request coordinates, never browser-posted
/// character data, are the sole authority for the resulting rows.
#[reducer]
pub fn create_starting_character(
    ctx: &ReducerContext,
    owner_key: String,
    generator_version: u16,
    seed: String,
    age_tier: StartingAgeTierCoordinate,
    slot: u8,
) -> Result<(), String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    let coordinate = age_tier;
    let age_tier = coordinate.core();
    let spec =
        adventuresim_core::starting_character::generate(generator_version, &seed, age_tier, slot)
            .map_err(str::to_owned)?;
    let request_key = format!("{generator_version}:{seed}:{}:{slot}", age_tier.as_str());
    if let Some(claim) = ctx
        .db
        .starting_character_claim()
        .request_key()
        .find(&request_key)
    {
        let existing = ctx
            .db
            .character()
            .id()
            .find(claim.character_id)
            .ok_or("candidate claim exists without its character")?;
        if claim.owner_key != owner_key {
            return Err("candidate was already claimed by a different browser owner".into());
        }
        if existing.id == spec.id && existing.name == spec.name {
            crate::browser_session::grant_browser_character_internal(
                ctx,
                &owner_key,
                existing.id,
                &request_key,
            )?;
            return Ok(());
        }
        return Err("candidate claim does not match regenerated character".into());
    }
    if ctx.db.character().id().find(spec.id).is_some() {
        return Err("generated character ID collides with unrelated data".into());
    }
    insert_starting_character(ctx, &spec)?;
    ctx.db
        .starting_character_claim()
        .insert(StartingCharacterClaim {
            request_key: request_key.clone(),
            character_id: spec.id,
            owner_key: owner_key.clone(),
            generator_version,
            seed,
            age_tier: coordinate,
            slot,
        });
    crate::browser_session::grant_browser_character_internal(
        ctx,
        &owner_key,
        spec.id,
        &request_key,
    )?;
    Ok(())
}

/// Seed an injured character for local UI development and visual verification.
pub(crate) fn seed_damaged_character(ctx: &ReducerContext) -> Result<(), String> {
    const DAMAGED_CHARACTER_ID: u64 = 9_999_999_999_999_999;

    if ctx.db.character().id().find(DAMAGED_CHARACTER_ID).is_none() {
        insert_new_character(ctx, "Wounded Demo".to_string(), DAMAGED_CHARACTER_ID, false)?;
    }
    for (id, name, role) in [
        (9_000_001, "Mara", "Backup surgeon"),
        (9_000_002, "Orrin", "Critical patient"),
    ] {
        if ctx.db.character().id().find(id).is_none() {
            insert_new_npc_character(ctx, name.into(), id, false)?;
        }
        crate::strategic::attach_seeded_party_member(ctx, DAMAGED_CHARACTER_ID, id, role)?;
    }

    let mut limbs = ctx
        .db
        .character_limbs()
        .character_id()
        .find(DAMAGED_CHARACTER_ID)
        .ok_or_else(|| "Damaged demo character is missing limb data".to_string())?;

    // Surgery's durable injury rows are authoritative over the projection.
    // Start clean so this reducer remains idempotent during UI iteration.
    limbs.left_arm_health = 1.0;
    limbs.right_arm_health = 1.0;
    limbs.left_leg_health = 1.0;
    limbs.right_leg_health = 1.0;
    limbs.head_health = 1.0;
    limbs.chest_health = 1.0;
    limbs.stomach_health = 1.0;
    ctx.db.character_limbs().character_id().update(limbs);

    for character_id in [DAMAGED_CHARACTER_ID, 9_000_001, 9_000_002] {
        for injury in ctx
            .db
            .limb_injury()
            .character_id()
            .filter(character_id)
            .collect::<Vec<_>>()
        {
            ctx.db.limb_injury().id().delete(injury.id);
        }
        for projectile in ctx
            .db
            .retained_projectile()
            .character_id()
            .filter(character_id)
            .collect::<Vec<_>>()
        {
            ctx.db.retained_projectile().id().delete(projectile.id);
        }
    }
    crate::surgery::commit_hit_injury(
        ctx,
        DAMAGED_CHARACTER_ID,
        crate::surgery::LimbRegion::LeftArm,
        0.22,
        0.05,
        Some(crate::surgery::ProjectileKind::Arrowhead),
    )?;
    crate::surgery::commit_hit_injury(
        ctx,
        DAMAGED_CHARACTER_ID,
        crate::surgery::LimbRegion::RightArm,
        0.18,
        0.0,
        None,
    )?;
    let mut bandaged = crate::surgery::injury_for(
        ctx,
        DAMAGED_CHARACTER_ID,
        crate::surgery::LimbRegion::RightArm,
    );
    bandaged.bandaged = true;
    bandaged.stitched = true;
    bandaged.stitch_quality = 4.0;
    ctx.db.limb_injury().id().update(bandaged);
    crate::surgery::commit_hit_injury(
        ctx,
        DAMAGED_CHARACTER_ID,
        crate::surgery::LimbRegion::LeftLeg,
        0.0,
        0.42,
        None,
    )?;
    let mut splinted = crate::surgery::injury_for(
        ctx,
        DAMAGED_CHARACTER_ID,
        crate::surgery::LimbRegion::LeftLeg,
    );
    splinted.splint_owner_id = Some(DAMAGED_CHARACTER_ID);
    ctx.db.limb_injury().id().update(splinted);
    crate::surgery::commit_hit_injury(
        ctx,
        DAMAGED_CHARACTER_ID,
        crate::surgery::LimbRegion::Chest,
        0.15,
        0.08,
        Some(crate::surgery::ProjectileKind::Ball),
    )?;

    crate::add_inventory_item(ctx, DAMAGED_CHARACTER_ID, "bandage", 8);
    crate::add_inventory_item(ctx, DAMAGED_CHARACTER_ID, "surgery_kit", 1);
    crate::add_inventory_item(ctx, DAMAGED_CHARACTER_ID, "splint", 3);

    // A wounded primary surgeon, a less-skilled backup, and a second critical
    // patient make clock alignment and parallel triage visible in one fixture.
    let fixture_now = ctx
        .db
        .character_time()
        .character_id()
        .find(DAMAGED_CHARACTER_ID)
        .ok_or("Surgery demo primary surgeon is missing time")?
        .minutes
        .max(200);
    for (id, procedure_hours, lag) in [
        (DAMAGED_CHARACTER_ID, 20_000.0, 0),
        (9_000_001, 3_333.0, 100),
        (9_000_002, 500.0, 200),
    ] {
        let mut skills = ctx
            .db
            .character_skills()
            .character_id()
            .find(id)
            .ok_or("Surgery demo character is missing skills")?;
        skills.surgery_hours = procedure_hours;
        skills.knife_hours = procedure_hours;
        skills.tailoring_hours = procedure_hours;
        ctx.db.character_skills().character_id().update(skills);
        let mut time = ctx
            .db
            .character_time()
            .character_id()
            .find(id)
            .ok_or("Surgery demo character is missing time")?;
        time.minutes = fixture_now.saturating_sub(lag);
        ctx.db.character_time().character_id().update(time);
        crate::capability::refresh_character_capability(ctx, id)?;
    }
    crate::add_inventory_item(ctx, 9_000_001, "bandage", 6);
    crate::add_inventory_item(ctx, 9_000_001, "surgery_kit", 1);
    crate::add_inventory_item(ctx, 9_000_001, "splint", 2);
    crate::surgery::commit_hit_injury(
        ctx,
        9_000_002,
        crate::surgery::LimbRegion::RightLeg,
        0.36,
        0.08,
        Some(crate::surgery::ProjectileKind::Arrowhead),
    )?;
    crate::surgery::commit_hit_injury(
        ctx,
        9_000_002,
        crate::surgery::LimbRegion::LeftArm,
        0.04,
        0.50,
        None,
    )?;

    // Exercise field, settlement, and beyond-smith repair states across both
    // specialist screens. Equipped pieces make the durability column useful,
    // while the two spares also exercise the combined sell/repair row actions.
    let mut damaged_equipment = Vec::new();
    let armor_damage = [
        [0.18, 0.0, 0.0, 0.0, 0.0],
        [0.0, 0.10, 0.10, 0.0, 0.0],
        [0.10, 0.0, 0.08, 0.0, 0.0],
        [0.0, 0.0, 0.0, 0.12, 0.08],
        [0.0, 0.0, 0.15, 0.05, 0.0],
        [0.05, 0.10, 0.08, 0.07, 0.0],
        [0.0, 0.14, 0.0, 0.0, 0.0],
    ];
    damaged_equipment.extend(
        equipped_wearable_ids(ctx, DAMAGED_CHARACTER_ID)
            .into_iter()
            .enumerate()
            .map(|(index, id)| (Some(id), armor_damage[index % armor_damage.len()])),
    );
    for (inventory_item_id, bins) in damaged_equipment {
        if let Some(inventory_item_id) = inventory_item_id {
            set_demo_item_damage(ctx, inventory_item_id, bins)?;
        }
    }

    for (item_id, bins) in [
        ("arming_sword", [0.10, 0.08, 0.12, 0.05, 0.0]),
        ("brigandine", [0.06, 0.10, 0.08, 0.08, 0.04]),
    ] {
        let inventory_item_id = ctx
            .db
            .inventory_item()
            .character_id()
            .filter(DAMAGED_CHARACTER_ID)
            .find(|inventory| inventory.item_id == item_id)
            .map(|inventory| inventory.id)
            .or_else(|| add_inventory_item(ctx, DAMAGED_CHARACTER_ID, item_id, 1))
            .ok_or_else(|| format!("Failed to add {item_id} to damaged demo inventory"))?;
        set_demo_item_damage(ctx, inventory_item_id, bins)?;
    }

    Ok(())
}

/// Seed a character with direct training in every religion for local UI development.
pub(crate) fn seed_religion_scholar_character(ctx: &ReducerContext) -> Result<(), String> {
    const RELIGION_SCHOLAR_CHARACTER_ID: u64 = 9_999_999_999_999_988;

    if ctx
        .db
        .character()
        .id()
        .find(RELIGION_SCHOLAR_CHARACTER_ID)
        .is_none()
    {
        insert_new_character(
            ctx,
            "Religion Scholar Demo".to_string(),
            RELIGION_SCHOLAR_CHARACTER_ID,
            false,
        )?;
    }

    let mut skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(RELIGION_SCHOLAR_CHARACTER_ID)
        .ok_or_else(|| "Religion scholar demo is missing skill data".to_string())?;
    skills.religion_hours = adventuresim_world_schema::ReligionHours {
        roman_catholic: 100.0,
        lutheran: 200.0,
        reformed: 300.0,
        anglican: 400.0,
        eastern_orthodox: 500.0,
        islamic: 600.0,
        judaism: 700.0,
    };
    ctx.db.character_skills().character_id().update(skills);

    let mut condition = ctx
        .db
        .character_condition()
        .character_id()
        .find(RELIGION_SCHOLAR_CHARACTER_ID)
        .ok_or_else(|| "Religion scholar demo is missing condition data".to_string())?;
    condition.religion_id = Some(
        adventuresim_world_schema::OfficialReligion::RomanCatholic
            .religion_id()
            .to_string(),
    );
    ctx.db
        .character_condition()
        .character_id()
        .update(condition);

    crate::capability::refresh_character_capability(ctx, RELIGION_SCHOLAR_CHARACTER_ID)?;
    crate::condition::refresh_character_strategic_condition(ctx, RELIGION_SCHOLAR_CHARACTER_ID)?;
    Ok(())
}

/// Seed an isolated, selectable character for exercising every bounded
/// Herbalism method, public grade, and terrain-backed foraging flow in the
/// strategic UI.
pub(crate) fn seed_herbalism_demo_character(ctx: &ReducerContext) -> Result<(), String> {
    const HERBALISM_DEMO_CHARACTER_ID: u64 = 9_999_999_999_999_986;
    const HERBALISM_DEMO_SETTLEMENT_ID: &str = "dev-scenario-foraging";
    crate::strategic::ensure_foraging_demo_settlement(ctx)?;
    if ctx
        .db
        .character()
        .id()
        .find(HERBALISM_DEMO_CHARACTER_ID)
        .is_none()
    {
        insert_new_character(
            ctx,
            "Herbalism and Foraging Demo".to_string(),
            HERBALISM_DEMO_CHARACTER_ID,
            false,
        )?;
    }
    // Pin this durable visual fixture and its solo party to an empirically
    // sampled final-pack woodland cell so the integrated Forage action is
    // always demonstrable after a normal seed.
    let mut character = ctx
        .db
        .character()
        .id()
        .find(HERBALISM_DEMO_CHARACTER_ID)
        .ok_or_else(|| "Herbalism and foraging demo character is missing".to_string())?;
    character.name = "Herbalism and Foraging Demo".into();
    let party_id = character
        .party_id
        .clone()
        .ok_or_else(|| "Herbalism and foraging demo has no solo party".to_string())?;
    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or_else(|| "Herbalism and foraging demo party is missing".to_string())?;
    let members = ctx
        .db
        .party_member()
        .party_id()
        .filter(&party_id)
        .collect::<Vec<_>>();
    if !party.is_solo
        || party.leader_id != HERBALISM_DEMO_CHARACTER_ID
        || members.len() != 1
        || members[0].character_id != HERBALISM_DEMO_CHARACTER_ID
    {
        return Err("Herbalism and foraging demo no longer has its isolated solo party".into());
    }
    crate::investigation::set_character_case_site(ctx, HERBALISM_DEMO_CHARACTER_ID, None)?;
    character.current_settlement_id = Some(HERBALISM_DEMO_SETTLEMENT_ID.into());
    ctx.db.character().id().update(character);
    party.current_settlement_id = Some(HERBALISM_DEMO_SETTLEMENT_ID.into());
    party.current_case_site_id = None;
    party.camp_destination = None;
    party.camp_remaining_minutes = 0;
    ctx.db.party_authority().id().update(party);
    crate::strategic::finish_party_journey(ctx, &party_id);
    if ctx
        .db
        .strategic_encounter()
        .party_id()
        .find(&party_id)
        .is_some()
    {
        ctx.db.strategic_encounter().party_id().delete(&party_id);
    }
    let mut skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(HERBALISM_DEMO_CHARACTER_ID)
        .ok_or_else(|| "Herbalism demo is missing skill data".to_string())?;
    skills.herbalism_hours = 10_000.0;
    skills.physiology_hours = 4_000.0;
    ctx.db.character_skills().character_id().update(skills);
    for (item_id, quantity) in [
        ("willow_bark", 2),
        ("comfrey", 2),
        ("poppy", 2),
        ("tincture_spirit", 4),
        ("sage", 2),
        ("glass_bottle", 2),
        ("glass_jar", 1),
        ("mortar_and_pestle", 1),
        ("knife", 1),
    ] {
        if ctx
            .db
            .inventory_item()
            .character_and_item_id()
            .filter((HERBALISM_DEMO_CHARACTER_ID, item_id))
            .next()
            .is_none()
        {
            add_inventory_item(ctx, HERBALISM_DEMO_CHARACTER_ID, item_id, quantity)
                .ok_or_else(|| format!("Failed to add {item_id} to Herbalism and Foraging Demo"))?;
        }
    }
    crate::capability::refresh_character_capability(ctx, HERBALISM_DEMO_CHARACTER_ID)?;
    crate::condition::refresh_character_strategic_condition(ctx, HERBALISM_DEMO_CHARACTER_ID)?;
    Ok(())
}

/// Seed broad category knowledge for local Bestiary rail and evidence demos.
pub(crate) fn seed_bestiary_scholar_character(ctx: &ReducerContext) -> Result<(), String> {
    const BESTIARY_SCHOLAR_CHARACTER_ID: u64 = 9_999_999_999_999_987;

    if ctx
        .db
        .character()
        .id()
        .find(BESTIARY_SCHOLAR_CHARACTER_ID)
        .is_none()
    {
        insert_new_character(
            ctx,
            "Bestiary Scholar Demo".to_string(),
            BESTIARY_SCHOLAR_CHARACTER_ID,
            false,
        )?;
    }

    let mut skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(BESTIARY_SCHOLAR_CHARACTER_ID)
        .ok_or_else(|| "Bestiary scholar demo is missing skill data".to_string())?;
    skills.bestiary_hours = adventuresim_world_schema::BestiaryHours {
        beast: 4_000.0,
        undead: 4_000.0,
        human: 4_000.0,
        werekin: 4_000.0,
        elf: 3_000.0,
        dwarf: 3_000.0,
        fey: 4_000.0,
        spirit: 4_000.0,
        greenskin: 3_000.0,
        insectoid: 2_000.0,
        draconid: 2_000.0,
        construct: 2_000.0,
        wildmen: 4_000.0,
    };
    ctx.db.character_skills().character_id().update(skills);
    crate::capability::refresh_character_capability(ctx, BESTIARY_SCHOLAR_CHARACTER_ID)?;
    Ok(())
}

fn set_demo_item_damage(
    ctx: &ReducerContext,
    inventory_item_id: u64,
    bins: [f32; 5],
) -> Result<(), String> {
    let inventory = ctx
        .db
        .inventory_item()
        .id()
        .find(inventory_item_id)
        .ok_or_else(|| format!("Damaged demo item {inventory_item_id} is missing"))?;
    let quality = ctx
        .db
        .item()
        .id()
        .find(&inventory.item_id)
        .map_or(1, |definition| definition.quality.clamp(1, 5));
    let bins = adventuresim_core::durability::DamageBins(bins)
        .capped_to_quality(quality)
        .0;
    let mut condition = ctx
        .db
        .item_condition()
        .inventory_item_id()
        .find(inventory_item_id)
        .ok_or_else(|| format!("Damaged demo item {inventory_item_id} has no condition row"))?;
    [
        condition.tier_1,
        condition.tier_2,
        condition.tier_3,
        condition.tier_4,
        condition.tier_5,
    ] = bins;
    ctx.db
        .item_condition()
        .inventory_item_id()
        .update(condition);
    Ok(())
}

pub(crate) fn insert_new_character(
    ctx: &ReducerContext,
    name: String,
    id: u64,
    temporary: bool,
) -> Result<(), String> {
    let mode = if temporary {
        CharacterCreationMode::TemporaryNpc
    } else {
        CharacterCreationMode::Player
    };
    insert_character_with_origin(
        ctx,
        name,
        id,
        CharacterCreationOptions {
            origin_settlement_id: None,
            mode,
            create_solo_party: true,
            stable_seed: id,
            initial_time_minute: None,
            field_actor: false,
        },
        None,
        None,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CharacterCreationMode {
    Player,
    PersistentNpc,
    TemporaryNpc,
    Newborn,
}

impl CharacterCreationMode {
    const fn is_npc(self) -> bool {
        matches!(
            self,
            Self::PersistentNpc | Self::TemporaryNpc | Self::Newborn
        )
    }

    const fn temporary(self) -> bool {
        matches!(self, Self::TemporaryNpc)
    }

    const fn newborn(self) -> bool {
        matches!(self, Self::Newborn)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CharacterCreationOptions<'a> {
    pub origin_settlement_id: Option<&'a str>,
    pub mode: CharacterCreationMode,
    pub create_solo_party: bool,
    pub stable_seed: u64,
    pub initial_time_minute: Option<u64>,
    pub field_actor: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NpcLifeFacts {
    pub age_years: u16,
    pub organization_id: Option<String>,
    pub literacy: Option<adventuresim_world_schema::WrittenLanguage>,
}

impl NpcLifeFacts {
    /// Produce the baseline life facts for a persistent NPC without consulting
    /// reducer RNG. Authored organization and literacy can be overlaid by the
    /// population importer before creation.
    pub(crate) fn from_stable_seed(stable_seed: u64) -> Self {
        let draw = stable_mix(stable_seed ^ 0x6c69_6665_2d61_6765);
        Self {
            age_years: 18 + (draw % 43) as u16,
            organization_id: None,
            literacy: None,
        }
    }
}

fn stable_mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

pub(crate) fn insert_new_npc_character(
    ctx: &ReducerContext,
    name: String,
    id: u64,
    temporary: bool,
) -> Result<(), String> {
    let mode = if temporary {
        CharacterCreationMode::TemporaryNpc
    } else {
        CharacterCreationMode::PersistentNpc
    };
    let life = NpcLifeFacts::from_stable_seed(id);
    insert_character_with_origin(
        ctx,
        name,
        id,
        CharacterCreationOptions {
            origin_settlement_id: None,
            mode,
            create_solo_party: temporary,
            stable_seed: id,
            initial_time_minute: None,
            field_actor: false,
        },
        None,
        Some(&life),
    )
}

/// Create a persistent, full-component NPC at an explicit settlement. Unlike a
/// player character or tactical temporary, the NPC begins outside any party.
pub(crate) fn insert_persistent_npc_character(
    ctx: &ReducerContext,
    name: String,
    id: u64,
    origin_settlement_id: &str,
    stable_seed: u64,
    initial_time_minute: Option<u64>,
) -> Result<(), String> {
    let life = NpcLifeFacts::from_stable_seed(stable_seed);
    insert_character_with_origin(
        ctx,
        name,
        id,
        CharacterCreationOptions {
            origin_settlement_id: Some(origin_settlement_id),
            mode: CharacterCreationMode::PersistentNpc,
            create_solo_party: false,
            stable_seed,
            initial_time_minute,
            field_actor: false,
        },
        None,
        Some(&life),
    )
}

/// Create a durable, fully componentized character whose authoritative
/// placement is a non-settlement context (an encounter, case site, or group).
pub(crate) fn insert_persistent_field_character(
    ctx: &ReducerContext,
    name: String,
    id: u64,
    stable_seed: u64,
    initial_time_minute: Option<u64>,
) -> Result<(), String> {
    let life = NpcLifeFacts::from_stable_seed(stable_seed);
    insert_character_with_origin(
        ctx,
        name,
        id,
        CharacterCreationOptions {
            origin_settlement_id: None,
            mode: CharacterCreationMode::PersistentNpc,
            create_solo_party: false,
            stable_seed,
            initial_time_minute,
            field_actor: true,
        },
        None,
        Some(&life),
    )
}

fn insert_starting_character(
    ctx: &ReducerContext,
    spec: &StartingCharacterSpec,
) -> Result<(), String> {
    insert_character_with_origin(
        ctx,
        spec.name.clone(),
        spec.id,
        CharacterCreationOptions {
            origin_settlement_id: None,
            mode: CharacterCreationMode::Player,
            create_solo_party: true,
            stable_seed: spec.id,
            initial_time_minute: None,
            field_actor: false,
        },
        Some(spec),
        None,
    )
}

fn initial_membership_minutes(now: u64, dues_interval_days: Option<u32>) -> (u64, u64) {
    let paid_through = dues_interval_days.map_or(u64::MAX, |days| {
        now.saturating_add(u64::from(days) * adventuresim_core::strategic_time::MINUTES_PER_DAY)
    });
    (now, paid_through)
}

pub(crate) fn set_character_languages_for_settlement(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: &str,
    npc: bool,
) -> Result<(), String> {
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(settlement_id.to_string())
        .ok_or_else(|| format!("Unknown settlement {settlement_id}"))?;
    let mut skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or_else(|| format!("Character {character_id} has no skills"))?;
    let (oral, _) = adventuresim_world_schema::initial_character_languages(
        settlement.languages,
        character_id,
        npc,
    );
    skills.oral_languages = oral;
    // Relocating a character can replace their authored vernacular identity
    // during bootstrap, but literacy comes from organization roles and institutional
    // training and must not be erased by a change of settlement.
    ctx.db.character_skills().character_id().update(skills);
    Ok(())
}

pub(crate) fn shared_language_coefficient(
    ctx: &ReducerContext,
    left_id: u64,
    right_id: u64,
) -> f32 {
    let Some(left) = ctx.db.character_skills().character_id().find(left_id) else {
        return 0.0;
    };
    let Some(right) = ctx.db.character_skills().character_id().find(right_id) else {
        return 0.0;
    };
    let left_cap = ctx
        .db
        .character_attributes()
        .character_id()
        .find(left_id)
        .map_or(0.0, |attributes| attributes.instinct * 1_000.0);
    let right_cap = ctx
        .db
        .character_attributes()
        .character_id()
        .find(right_id)
        .map_or(0.0, |attributes| attributes.instinct * 1_000.0);
    adventuresim_world_schema::best_common_oral_language_capped(
        left.oral_languages,
        left_cap,
        right.oral_languages,
        right_cap,
    )
    .1
}

pub(crate) fn insert_character_with_origin(
    ctx: &ReducerContext,
    name: String,
    id: u64,
    options: CharacterCreationOptions<'_>,
    starting: Option<&StartingCharacterSpec>,
    npc_life: Option<&NpcLifeFacts>,
) -> Result<(), String> {
    log::info!("New character created: {name} (ID: {id})");
    let temporary = options.mode.temporary();
    let npc = options.mode.is_npc();
    let newborn = options.mode.newborn();
    let reserved = ctx.db.child_identity_reservation().character_id().find(id);
    if reserved.is_some() && !newborn {
        return Err("Character identity is reserved for a pending birth".into());
    }
    if newborn && reserved.is_none() {
        return Err("Newborn creation requires a reserved child identity".into());
    }

    let settlements: Vec<Settlement> = ctx.db.settlement().iter().collect();
    if settlements.is_empty() {
        return Err("Cannot create a character before at least one settlement is loaded".into());
    }
    let mut settlements = settlements;
    settlements.sort_by(|left, right| left.id.cmp(&right.id));
    let selector = starting.map_or_else(
        || {
            if npc {
                stable_mix(options.stable_seed ^ 0x7365_7474_6c65_6d65)
            } else {
                ctx.random::<u64>()
            }
        },
        |spec| spec.settlement_selector,
    );
    let start_settlement = if let Some(origin_settlement_id) = options.origin_settlement_id {
        settlements
            .iter()
            .find(|settlement| settlement.id == origin_settlement_id)
            .ok_or_else(|| format!("Unknown origin settlement {origin_settlement_id}"))?
    } else if let Some(starting_organization) = starting.and_then(|spec| spec.organization.as_ref())
    {
        let organization =
            adventuresim_core::organization::organization(&starting_organization.organization_id)
                .ok_or("Starting organization is not in the catalog")?;
        let eligible = settlements
            .iter()
            .filter(|settlement| {
                organization.has_chapter(&settlement.id)
                    && organization.recognition.includes(&settlement.id)
            })
            .collect::<Vec<_>>();
        if eligible.is_empty() {
            // Small development worlds do not load the researched Viabundus
            // settlements referenced by the organization catalog. Keep the
            // professional package intact and place the character
            // deterministically in the loaded world; a complete world still
            // prefers a recognized chapter settlement below.
            log::warn!(
                "No loaded settlement hosts starting organization {}; using a loaded settlement",
                organization.id
            );
            &settlements[selector as usize % settlements.len()]
        } else {
            eligible[selector as usize % eligible.len()]
        }
    } else {
        &settlements[selector as usize % settlements.len()]
    };

    let character = ctx.db.character().insert(Character {
        id,
        scan_id: id,
        name,
        xp: 0,
        level: 1,
        // Legacy scalar retained only for schema compatibility. Currency is
        // authoritative in inventory.
        gold: 0,
        current_settlement_id: (!options.field_actor).then(|| start_settlement.id.clone()),
        party_id: None,
        server: Identity::ZERO,
        in_server: false,
        temporary,
        age_years: starting.map_or_else(
            || npc_life.map_or(25, |facts| facts.age_years),
            |spec| spec.age_years,
        ),
        alive: true,
        party_treatment_decision: crate::world_actor::ContextualDecisionState::Allowed,
    });
    let initial_minute = options.initial_time_minute.unwrap_or(0);
    let birth_minute =
        i128::from(initial_minute).saturating_sub(i128::from(character.age_years).saturating_mul(
            i128::from(adventuresim_core::strategic_time::MINUTES_PER_YEAR),
        ));
    crate::relationship::record_character_birth(
        ctx,
        character.id,
        birth_minute.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64,
    );
    if !temporary && !newborn {
        let urban = matches!(
            start_settlement.category,
            crate::strategic::SettlementCategory::Town
                | crate::strategic::SettlementCategory::City
                | crate::strategic::SettlementCategory::Capital
        );
        crate::social_roles::ensure_character_social_roles(
            ctx,
            character.id,
            &start_settlement.id,
            urban,
        )?;
    }
    let _character_stats = ctx.db.character_stats().insert(CharacterStats {
        character_id: id,
        calories_used: 0.0,
        focus: 1.0,
    });
    let generated_attributes = starting.map(|spec| &spec.attributes);
    let character_attributes = CharacterAttributes {
        character_id: id,
        endurance: generated_attributes.map_or(2.0, |a| a.endurance),
        immunity: generated_attributes.map_or(2.0, |a| a.immunity),
        gut: generated_attributes.map_or(2.0, |a| a.gut),
        intelligence: generated_attributes.map_or(2.0, |a| a.intelligence),
        instinct: generated_attributes.map_or(2.0, |a| a.instinct),
        eyesight: generated_attributes.map_or(2.0, |a| a.eyesight),
        hearing: generated_attributes.map_or(2.0, |a| a.hearing),
        left_arm_strength: generated_attributes.map_or(3.0, |a| a.strength),
        right_arm_strength: generated_attributes.map_or(3.0, |a| a.strength),
        left_leg_strength: generated_attributes.map_or(3.0, |a| a.strength),
        right_leg_strength: generated_attributes.map_or(3.0, |a| a.strength),
        left_arm_agility: generated_attributes.map_or(3.0, |a| a.agility),
        right_arm_agility: generated_attributes.map_or(3.0, |a| a.agility),
        left_leg_agility: generated_attributes.map_or(3.0, |a| a.agility),
        right_leg_agility: generated_attributes.map_or(3.0, |a| a.agility),
    };
    let _character_attrs = ctx
        .db
        .character_attributes()
        .insert(character_attributes.clone());
    let (oral_languages, mut written_languages) =
        adventuresim_world_schema::initial_character_languages(start_settlement.languages, id, npc);
    let creation_literacy = npc_life
        .and_then(|facts| facts.literacy)
        .or(crate::social_roles::character_creation_literacy(ctx, id)?);
    let life_skills = (starting.is_none() && (!temporary || npc)).then(|| {
        let profile = adventuresim_core::strategic_schedule::ActivityTrainingProfile {
            combat: adventuresim_core::strategic_schedule::CombatTrainingProfile {
                weapons: adventuresim_core::equipment::WeaponSkillDistribution {
                    sword: 1.0,
                    ..Default::default()
                },
                block: 1.0,
                ..Default::default()
            },
        };
        let organization = npc_life
            .and_then(|facts| facts.organization_id.as_deref())
            .and_then(adventuresim_core::organization::organization);
        adventuresim_core::life_simulation::simulate_life(
            adventuresim_core::life_simulation::LifeSimulationInput {
                stable_seed: options.stable_seed ^ 0x6765_6e65_7269_6300,
                age_years: npc_life.map_or(25, |facts| facts.age_years),
                attributes: &character_attributes,
                organization,
                rank_requirements: &[],
                religion: None,
                activity_profile: profile,
                native_oral: oral_languages,
                literacy: creation_literacy,
            },
        )
    });
    let generated_skills = starting.map(|spec| &spec.skills);
    let persisted_oral_languages = life_skills.map_or(oral_languages, |simulated| simulated.oral);
    if let Some(generated) = generated_skills {
        written_languages = generated.written;
    } else if let Some(simulated) = life_skills {
        written_languages = simulated.written;
    }
    if let Some(language) = creation_literacy
        && starting.is_some()
    {
        adventuresim_core::life_simulation::apply_creation_literacy(
            &mut written_languages,
            character.age_years,
            language,
            &character_attributes,
        );
    }
    let _character_skills = ctx.db.character_skills().insert(CharacterSkills {
        character_id: id,
        polearm_hours: generated_skills.map_or_else(
            || life_skills.map_or(2000.0, |s| s.skills.polearm),
            |s| s.polearm,
        ),
        axe_hours: generated_skills
            .map_or_else(|| life_skills.map_or(2000.0, |s| s.skills.axe), |s| s.axe),
        bludgeon_hours: generated_skills.map_or_else(
            || life_skills.map_or(2000.0, |s| s.skills.bludgeon),
            |s| s.bludgeon,
        ),
        sword_hours: generated_skills.map_or_else(
            || life_skills.map_or(2000.0, |s| s.skills.sword),
            |s| s.sword,
        ),
        knife_hours: generated_skills.map_or_else(
            || life_skills.map_or(2000.0, |s| s.skills.knife),
            |s| s.knife,
        ),
        dodge_hours: generated_skills.map_or_else(
            || life_skills.map_or(1000.0, |s| s.skills.dodge),
            |s| s.dodge,
        ),
        block_hours: generated_skills.map_or_else(
            || life_skills.map_or(1000.0, |s| s.skills.block),
            |s| s.block,
        ),
        bow_hours: generated_skills
            .map_or_else(|| life_skills.map_or(1000.0, |s| s.skills.bow), |s| s.bow),
        crossbow_hours: generated_skills.map_or_else(
            || life_skills.map_or(1000.0, |s| s.skills.crossbow),
            |s| s.crossbow,
        ),
        firearm_hours: generated_skills.map_or_else(
            || life_skills.map_or(1000.0, |s| s.skills.firearm),
            |s| s.firearm,
        ),
        throw_hours: generated_skills.map_or_else(
            || life_skills.map_or(1000.0, |s| s.skills.throw),
            |s| s.throw,
        ),
        will_hours: generated_skills
            .map_or_else(|| life_skills.map_or(1000.0, |s| s.skills.will), |s| s.will),
        insight_hours: generated_skills.map_or_else(
            || life_skills.map_or(1000.0, |s| s.skills.insight),
            |s| s.insight,
        ),
        charm_hours: generated_skills.map_or_else(
            || life_skills.map_or(1000.0, |s| s.skills.charm),
            |s| s.charm,
        ),
        command_hours: generated_skills.map_or_else(
            || life_skills.map_or(1000.0, |s| s.skills.command),
            |s| s.command,
        ),
        deception_hours: generated_skills.map_or_else(
            || life_skills.map_or(1000.0, |s| s.skills.deception),
            |s| s.deception,
        ),
        physiology_hours: generated_skills.map_or_else(
            || life_skills.map_or(1000.0, |s| s.skills.physiology),
            |s| s.physiology,
        ),
        cooking_hours: generated_skills.map_or_else(
            || life_skills.map_or(0.0, |s| s.skills.cooking),
            |s| s.cooking,
        ),
        herbalism_hours: generated_skills.map_or_else(
            || life_skills.map_or(0.0, |s| s.skills.herbalism),
            |s| s.herbalism,
        ),
        religion_hours: generated_skills.map_or_else(
            || life_skills.map_or_else(Default::default, |s| s.skills.religion),
            |s| s.religion,
        ),
        bestiary_hours: generated_skills.map_or_else(
            || {
                life_skills.map_or(
                    adventuresim_world_schema::BestiaryHours {
                        beast: 1000.0,
                        human: 1000.0,
                        ..Default::default()
                    },
                    |s| s.skills.bestiary,
                )
            },
            |s| s.bestiary,
        ),
        oral_languages: persisted_oral_languages,
        written_languages,
        stealth_hours: generated_skills.map_or_else(
            || life_skills.map_or(1000.0, |s| s.skills.stealth),
            |s| s.stealth,
        ),
        balance_hours: generated_skills.map_or_else(
            || life_skills.map_or(1000.0, |s| s.skills.balance),
            |s| s.balance,
        ),
        terrain_plains_hours: generated_skills.map_or_else(
            || life_skills.map_or(0.0, |s| s.skills.terrain_plains),
            |s| s.terrain_plains,
        ),
        terrain_forest_hours: generated_skills.map_or_else(
            || life_skills.map_or(0.0, |s| s.skills.terrain_forest),
            |s| s.terrain_forest,
        ),
        terrain_hills_hours: generated_skills.map_or_else(
            || life_skills.map_or(0.0, |s| s.skills.terrain_hills),
            |s| s.terrain_hills,
        ),
        terrain_wetlands_hours: generated_skills.map_or_else(
            || life_skills.map_or(0.0, |s| s.skills.terrain_wetlands),
            |s| s.terrain_wetlands,
        ),
        terrain_urban_hours: generated_skills.map_or_else(
            || life_skills.map_or(0.0, |s| s.skills.terrain_urban),
            |s| s.terrain_urban,
        ),
        terrain_snow_hours: generated_skills.map_or_else(
            || life_skills.map_or(0.0, |s| s.skills.terrain_snow),
            |s| s.terrain_snow,
        ),
        surgery_hours: generated_skills.map_or_else(
            || life_skills.map_or(1000.0, |s| s.skills.surgery),
            |s| s.surgery,
        ),
        tailoring_hours: generated_skills.map_or_else(
            || life_skills.map_or(1000.0, |s| s.skills.tailoring),
            |s| s.tailoring,
        ),
        smithing_hours: generated_skills.map_or_else(
            || life_skills.map_or(1000.0, |s| s.skills.smithing),
            |s| s.smithing,
        ),
    });
    crate::time::initialize_character_time(ctx, id)?;
    if let Some(minutes) = options.initial_time_minute {
        let mut time = ctx
            .db
            .character_time()
            .character_id()
            .find(id)
            .ok_or("New character time was not initialized")?;
        time.minutes = minutes;
        ctx.db.character_time().character_id().update(time);
    }
    let _character_limbs = ctx.db.character_limbs().insert(CharacterLimbs {
        character_id: id,
        left_arm_health: 1.0,
        right_arm_health: 1.0,
        left_leg_health: 1.0,
        right_leg_health: 1.0,
        head_health: 1.0,
        chest_health: 1.0,
        stomach_health: 1.0,
    });
    crate::condition::initialize_character_condition(ctx, character.id)?;
    if let Some(starting) = starting {
        let mut personality = crate::personality::CharacterPersonality::neutral(id);
        for personality_trait in &starting.personality.traits {
            use crate::personality::{
                Conscience, Conviction, Courtship, Drive, Hygiene, Mirth, Nerve, Outlook,
                SelfKnowledge, SelfRegard, Sociability, Temperance, Transparency,
            };
            match personality_trait {
                StartingPersonalityTrait::Brave => personality.nerve = Nerve::Brave,
                StartingPersonalityTrait::Fearful => personality.nerve = Nerve::Fearful,
                StartingPersonalityTrait::Ambitious => personality.drive = Drive::Ambitious,
                StartingPersonalityTrait::Content => personality.drive = Drive::Content,
                StartingPersonalityTrait::Sanguine => personality.outlook = Outlook::Sanguine,
                StartingPersonalityTrait::Brooding => personality.outlook = Outlook::Brooding,
                StartingPersonalityTrait::Gregarious => {
                    personality.sociability = Sociability::Gregarious
                }
                StartingPersonalityTrait::Solitary => {
                    personality.sociability = Sociability::Solitary
                }
                StartingPersonalityTrait::Compassionate => {
                    personality.conscience = Conscience::Compassionate
                }
                StartingPersonalityTrait::Callous => personality.conscience = Conscience::Callous,
                StartingPersonalityTrait::Cruel => personality.conscience = Conscience::Cruel,
                StartingPersonalityTrait::Proud => personality.self_regard = SelfRegard::Proud,
                StartingPersonalityTrait::Humble => personality.self_regard = SelfRegard::Humble,
                StartingPersonalityTrait::Zealous => personality.conviction = Conviction::Zealous,
                StartingPersonalityTrait::Irreverent => {
                    personality.conviction = Conviction::Irreverent
                }
                StartingPersonalityTrait::Slovenly => personality.hygiene = Hygiene::Slovenly,
                StartingPersonalityTrait::Cleanly => personality.hygiene = Hygiene::Cleanly,
                StartingPersonalityTrait::Temperate => {
                    personality.temperance = Temperance::Temperate
                }
                StartingPersonalityTrait::Drunkard => personality.temperance = Temperance::Drunkard,
                StartingPersonalityTrait::Merry => personality.mirth = Mirth::Merry,
                StartingPersonalityTrait::Grave => personality.mirth = Mirth::Grave,
                StartingPersonalityTrait::Amorous => personality.courtship = Courtship::Amorous,
                StartingPersonalityTrait::Proper => personality.courtship = Courtship::Proper,
                StartingPersonalityTrait::Open => personality.transparency = Transparency::Open,
                StartingPersonalityTrait::Guarded => {
                    personality.transparency = Transparency::Guarded
                }
                StartingPersonalityTrait::Introspective => {
                    personality.self_knowledge = SelfKnowledge::Introspective
                }
                StartingPersonalityTrait::SelfDeceiving => {
                    personality.self_knowledge = SelfKnowledge::SelfDeceiving
                }
            }
        }
        personality.sex = match starting.personality.sex {
            StartingSex::Female => crate::personality::Sex::Female,
            StartingSex::Male => crate::personality::Sex::Male,
        };
        personality.presentation = match starting.personality.presentation {
            StartingPresentation::Man => crate::personality::Presentation::Man,
            StartingPresentation::Ambiguous => crate::personality::Presentation::Ambiguous,
            StartingPresentation::Woman => crate::personality::Presentation::Woman,
        };
        personality.inclination = match starting.personality.inclination {
            StartingInclination::Men => crate::personality::Inclination::Men,
            StartingInclination::Either => crate::personality::Inclination::Either,
            StartingInclination::Women => crate::personality::Inclination::Women,
            StartingInclination::Neither => crate::personality::Inclination::Neither,
        };
        // Starting-character preview traits become score endpoints; the
        // gateway row is only their derived visible projection.
        crate::personality::initialize_personality_from_visible(ctx, personality);
    } else {
        if npc {
            crate::personality::initialize_npc_personality(ctx, id, options.stable_seed);
        } else {
            crate::personality::initialize_personality(ctx, id, false);
        }
    }

    // Newborns receive the full durable character component surface, but no
    // adult economic or equipment package.
    if !newborn {
        crate::item::credit_personal_currency(
            ctx,
            character.id,
            &start_settlement.id,
            starting.map_or(100, |s| s.currency),
        )?;
    }
    if let Some(spec) = starting {
        for item in &spec.inventory {
            if let Some(slot) = item.equipped {
                let destination = match slot {
                    StartingSlot::LeftHand => ItemSlot::LeftHolding,
                    StartingSlot::RightHand => ItemSlot::RightHolding,
                    StartingSlot::LeftArm => ItemSlot::LeftArm,
                    StartingSlot::RightArm => ItemSlot::RightArm,
                    StartingSlot::LeftLeg => ItemSlot::LeftLeg,
                    StartingSlot::RightLeg => ItemSlot::RightLeg,
                    StartingSlot::Head => ItemSlot::Head,
                    StartingSlot::Chest => ItemSlot::Chest,
                    StartingSlot::Stomach => ItemSlot::Stomach,
                };
                add_and_equip_item(ctx, character.id, &item.item_id, destination)?;
                if item.quantity > 1 {
                    add_inventory_item(ctx, character.id, &item.item_id, item.quantity - 1);
                }
            } else {
                add_inventory_item(ctx, character.id, &item.item_id, item.quantity);
            }
        }
    } else if !newborn {
        add_inventory_item(ctx, character.id, "torch", 1);
        add_inventory_item(ctx, character.id, "bandage", 3);
        for (item, slot) in [
            ("buckler", ItemSlot::LeftHolding),
            ("katzbalger", ItemSlot::RightHolding),
            ("quilted_sleeve", ItemSlot::LeftArm),
            ("quilted_sleeve", ItemSlot::RightArm),
            ("arming_cap", ItemSlot::Head),
            ("arming_doublet", ItemSlot::Chest),
            ("padded_skirt", ItemSlot::Stomach),
            ("padded_chausses", ItemSlot::LeftLeg),
            ("padded_chausses", ItemSlot::RightLeg),
        ] {
            add_and_equip_item(ctx, character.id, item, slot)?;
        }
    }
    if !newborn {
        provision_generated_weapon_carry(ctx, character.id)?;
    }

    if options.create_solo_party {
        crate::strategic::create_solo_party_for_character(ctx, character.id)?;
    }
    if let Some(starting_organization) = starting.and_then(|spec| spec.organization.as_ref()) {
        let definition =
            adventuresim_core::organization::organization(&starting_organization.organization_id)
                .ok_or("Starting organization is not in the catalog")?;
        if definition.role(&starting_organization.role_id).is_none() {
            return Err("Starting role is not in the organization catalog".into());
        }
        let now = ctx
            .db
            .character_time()
            .character_id()
            .find(character.id)
            .ok_or("Starting character time was not initialized")?
            .minutes;
        let (joined_minute, paid_through) = initial_membership_minutes(
            now,
            definition.dues.as_ref().map(|dues| dues.interval_days),
        );
        ctx.db
            .organization_membership()
            .insert(crate::organization::OrganizationMembership {
                id: 0,
                character_id: character.id,
                organization_id: starting_organization.organization_id.clone(),
                joined_minute,
                dues_paid_through_minute: paid_through,
                status: crate::organization::MEMBERSHIP_ACTIVE.into(),
                apprenticeship_minutes_accrued: 0,
                practice_minutes_accrued: 0,
            });
        ctx.db
            .organization_presentation()
            .insert(crate::organization::OrganizationPresentation {
                character_id: character.id,
                organization_id: starting_organization.organization_id.clone(),
            });
        crate::social_roles::ensure_character_professional_role(
            ctx,
            character.id,
            &starting_organization.organization_id,
            &starting_organization.role_id,
        )?;
    }
    crate::capability::refresh_character_capability(ctx, character.id)?;
    if let Some(religion_id) = starting.and_then(|spec| spec.religion_id.as_ref()) {
        let mut condition = ctx
            .db
            .character_condition()
            .character_id()
            .find(character.id)
            .ok_or("Starting character condition was not initialized")?;
        condition.religion_id = Some(religion_id.clone());
        ctx.db
            .character_condition()
            .character_id()
            .update(condition);
    }
    crate::condition::refresh_character_strategic_condition(ctx, character.id)?;
    crate::equipment_law::enforce_equipment_compliance(ctx, character.id)?;
    validate_full_character_components(ctx, character.id)?;

    Ok(())
}

/// Verify the ordinary durable component set shared by player characters and
/// persistent NPCs. This is deliberately a runtime invariant rather than a
/// migration/link-table shim: population import must create real Characters.
pub(crate) fn validate_full_character_components(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<(), String> {
    let mut missing = Vec::new();
    if ctx.db.character().id().find(character_id).is_none() {
        missing.push("character");
    }
    if ctx
        .db
        .character_stats()
        .character_id()
        .find(character_id)
        .is_none()
    {
        missing.push("stats");
    }
    if ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .is_none()
    {
        missing.push("attributes");
    }
    if ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .is_none()
    {
        missing.push("skills");
    }
    if ctx
        .db
        .character_limbs()
        .character_id()
        .find(character_id)
        .is_none()
    {
        missing.push("limbs");
    }
    if ctx
        .db
        .character_personality()
        .character_id()
        .find(character_id)
        .is_none()
    {
        missing.push("personality");
    }
    if ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .is_none()
    {
        missing.push("time");
    }
    if ctx
        .db
        .character_training_schedule()
        .character_id()
        .find(character_id)
        .is_none()
    {
        missing.push("training_schedule");
    }
    if ctx
        .db
        .character_capability()
        .character_id()
        .find(character_id)
        .is_none()
    {
        missing.push("capability");
    }
    if ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .is_none()
    {
        missing.push("condition");
    }
    if ctx
        .db
        .character_needs()
        .character_id()
        .find(character_id)
        .is_none()
    {
        missing.push("needs");
    }
    if ctx
        .db
        .character_exposure()
        .character_id()
        .find(character_id)
        .is_none()
    {
        missing.push("exposure");
    }
    if ctx
        .db
        .character_strategic_condition()
        .character_id()
        .find(character_id)
        .is_none()
    {
        missing.push("strategic_condition");
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Character {character_id} is missing full components: {}",
            missing.join(", ")
        ))
    }
}

#[reducer]
pub fn equip_item(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
    destination: ItemSlot,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    equip_item_internal(ctx, character_id, inventory_item_id, destination, true)
}

fn equip_item_internal(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
    destination: ItemSlot,
    enforce_law: bool,
) -> Result<(), String> {
    crate::strategic::require_character_no_unresolved_encounter(ctx, character_id)?;
    require_living_character(ctx, character_id)?;
    let inventory = ctx
        .db
        .inventory_item()
        .character_and_id()
        .filter((character_id, inventory_item_id))
        .next()
        .ok_or_else(|| {
            format!(
            "Can't equip item: InventoryItem@{inventory_item_id} doesn't exist for Character@{character_id}"
            )
        })?;
    let definition = ctx
        .db
        .item()
        .id()
        .find(&inventory.item_id)
        .ok_or_else(|| format!("Can't equip unknown item {}", inventory.item_id))?;
    if destination == ItemSlot::None {
        require_no_equipped_children(ctx, inventory_item_id)?;
        unequip_wearable(ctx, inventory_item_id);
        refresh_equipment_dependents(ctx, character_id)?;
        return Ok(());
    }
    let placement_index = equipment_placement_for_legacy_destination(
        ctx,
        character_id,
        inventory_item_id,
        &definition,
        destination,
    )
    .ok_or_else(|| {
        format!(
            "Can't equip {} at {destination:?}; choose one of its authored placements",
            inventory.item_id
        )
    })?;
    equip_equipment_internal(
        ctx,
        character_id,
        inventory_item_id,
        placement_index,
        Vec::new(),
        enforce_law,
        false,
    )
}

fn equipment_placement_for_legacy_destination(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
    item: &crate::Item,
    destination: ItemSlot,
) -> Option<u16> {
    use crate::item::EquipmentChannel as C;
    use crate::item::EquipmentLocation as L;
    let accepts = |requirement: &crate::item::EquipmentOccupancyRequirement| match destination {
        ItemSlot::LeftArm => requirement.location == L::LeftArm,
        ItemSlot::RightArm => requirement.location == L::RightArm,
        ItemSlot::AnyArm => matches!(requirement.location, L::LeftArm | L::RightArm),
        ItemSlot::LeftLeg => requirement.location == L::LeftLeg,
        ItemSlot::RightLeg => requirement.location == L::RightLeg,
        ItemSlot::AnyLeg => matches!(requirement.location, L::LeftLeg | L::RightLeg),
        ItemSlot::Head => requirement.location == L::Head,
        ItemSlot::Chest => requirement.location == L::Chest,
        ItemSlot::Stomach => requirement.location == L::Stomach,
        ItemSlot::LeftHolding => {
            requirement.location == L::LeftHand && requirement.channel == C::Held
        }
        ItemSlot::RightHolding => {
            requirement.location == L::RightHand && requirement.channel == C::Held
        }
        ItemSlot::AnyHolding => requirement.channel == C::Held,
        _ => false,
    };
    item.equipment_placements
        .iter()
        .position(|placement| {
            placement.parents.is_empty()
                && !placement.occupancy.is_empty()
                && placement.occupancy.iter().any(accepts)
                && conflicting_root_requirements(
                    &placement.occupancy,
                    inventory_item_id,
                    |requirement| {
                        ctx.db
                            .equipment_occupancy()
                            .id()
                            .find(character_occupancy_id(
                                character_id,
                                requirement.channel,
                                requirement.order,
                                requirement.location,
                            ))
                            .map(|row| row.inventory_item_id)
                    },
                )
                .is_empty()
        })
        .and_then(|index| u16::try_from(index).ok())
}

#[reducer]
pub fn equip_item_at_placement(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
    placement_index: u16,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    equip_equipment_internal(
        ctx,
        character_id,
        inventory_item_id,
        placement_index,
        Vec::new(),
        true,
        false,
    )
}

#[reducer]
pub fn attach_item_at_placement(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
    placement_index: u16,
    targets: Vec<EquipmentAttachmentTargetSelection>,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    equip_equipment_internal(
        ctx,
        character_id,
        inventory_item_id,
        placement_index,
        targets,
        true,
        false,
    )
}

/// Equip at an authored placement while atomically removing occupants of the
/// selected body or attachment cells. Every destination and displaced item is
/// validated before any equipment row is changed.
#[reducer]
pub fn replace_item_at_placement(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
    placement_index: u16,
    targets: Vec<EquipmentAttachmentTargetSelection>,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    equip_equipment_internal(
        ctx,
        character_id,
        inventory_item_id,
        placement_index,
        targets,
        true,
        true,
    )
}

fn equip_equipment_internal(
    ctx: &ReducerContext,
    character_id: u64,
    inventory_item_id: u64,
    placement_index: u16,
    targets: Vec<EquipmentAttachmentTargetSelection>,
    enforce_law: bool,
    replace_occupied: bool,
) -> Result<(), String> {
    crate::strategic::require_character_no_unresolved_encounter(ctx, character_id)?;
    require_living_character(ctx, character_id)?;
    let inventory = ctx
        .db
        .inventory_item()
        .character_and_id()
        .filter((character_id, inventory_item_id))
        .next()
        .ok_or_else(|| {
            format!(
                "Can't equip item: InventoryItem@{inventory_item_id} doesn't exist for Character@{character_id}"
            )
        })?;
    let definition = ctx
        .db
        .item()
        .id()
        .find(&inventory.item_id)
        .ok_or_else(|| format!("Can't equip unknown item {}", inventory.item_id))?;
    let placement = definition
        .equipment_placements
        .get(usize::from(placement_index))
        .ok_or_else(|| {
            format!(
                "Invalid placement {placement_index} for {}; expected 0..{}",
                inventory.item_id,
                definition.equipment_placements.len()
            )
        })?;
    if definition.kind == crate::item::ItemKind::Weapon {
        match adventuresim_core::item_catalog::weapon_carry(&inventory.item_id) {
            Some(adventuresim_core::item_catalog::WeaponCarry::HandOnly)
                if !hand_only_placement_is_held_root(placement) =>
            {
                return Err(format!(
                    "Can't equip hand-only weapon {} anywhere except one held left/right hand root",
                    inventory.item_id
                ));
            }
            Some(adventuresim_core::item_catalog::WeaponCarry::Sheathable) => {}
            _ if !placement.parents.is_empty() => {
                return Err(format!(
                    "Can't attach weapon {} without an authored sheathable carry contract",
                    inventory.item_id
                ));
            }
            _ => {}
        }
    }
    if enforce_law {
        crate::equipment_law::require_item_legal(ctx, character_id, inventory_item_id)?;
    }
    crate::inventory_container::detach_row_for_action(ctx, "personal", inventory_item_id)?;
    let root_occupancies = placement
        .occupancy
        .iter()
        .copied()
        .enumerate()
        .map(|(index, requirement)| {
            u16::try_from(index)
                .map(|requirement_index| (requirement_index, requirement))
                .map_err(|_| "Too many physical occupancy requirements")
        })
        .collect::<Result<Vec<_>, _>>()?;

    let existing = ctx
        .db
        .character_equipped_item()
        .inventory_item_id()
        .find(inventory_item_id);
    if existing.is_some() {
        require_no_equipped_children(ctx, inventory_item_id)?;
    }

    // Validate the entire graph move before mutating either normalized table.
    let conflicts =
        conflicting_root_requirements(&placement.occupancy, inventory_item_id, |requirement| {
            ctx.db
                .equipment_occupancy()
                .id()
                .find(character_occupancy_id(
                    character_id,
                    requirement.channel,
                    requirement.order,
                    requirement.location,
                ))
                .map(|row| row.inventory_item_id)
        });
    if !replace_occupied && !conflicts.is_empty() {
        let details = conflicts
            .iter()
            .map(|requirement| {
                format!(
                    "{:?} ({:?}, order {})",
                    requirement.location, requirement.channel, requirement.order
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Can't equip {}: occupied at {details}",
            inventory.item_id
        ));
    }
    let mut displaced_item_ids = std::collections::BTreeSet::new();
    if replace_occupied {
        for requirement in &conflicts {
            if let Some(occupant) = ctx
                .db
                .equipment_occupancy()
                .id()
                .find(character_occupancy_id(
                    character_id,
                    requirement.channel,
                    requirement.order,
                    requirement.location,
                ))
                .map(|row| row.inventory_item_id)
                .filter(|occupant| *occupant != inventory_item_id)
            {
                displaced_item_ids.insert(occupant);
            }
        }
    }
    if targets.len() != placement.parents.len() {
        return Err(format!(
            "Placement {} requires {} attachment target(s), received {}",
            placement.id,
            placement.parents.len(),
            targets.len()
        ));
    }
    let mut selected_requirements = std::collections::BTreeSet::new();
    let mut reserved_capacity = std::collections::BTreeSet::new();
    let mut parent_occupancies = Vec::new();
    for target in &targets {
        let requirement_index = usize::from(target.requirement_index);
        let requirement = placement.parents.get(requirement_index).ok_or_else(|| {
            format!(
                "Invalid parent requirement {} for placement {}",
                target.requirement_index, placement.id
            )
        })?;
        if !selected_requirements.insert(target.requirement_index) {
            return Err(format!(
                "Parent requirement {} was selected more than once",
                target.requirement_index
            ));
        }
        let parent_id = target.parent_inventory_item_id;
        let point_id = target.attachment_point_id.as_str();
        if parent_id == inventory_item_id {
            return Err("An item cannot be attached to itself".into());
        }
        ctx.db
            .character_equipped_item()
            .inventory_item_id()
            .find(parent_id)
            .filter(|row| row.character_id == character_id)
            .ok_or("Parent item is not equipped by this character")?;
        let parent_inventory = ctx
            .db
            .inventory_item()
            .id()
            .find(parent_id)
            .ok_or("Parent inventory item is missing")?;
        let parent_definition = ctx
            .db
            .item()
            .id()
            .find(&parent_inventory.item_id)
            .ok_or("Parent item definition is missing")?;
        let point = parent_definition
            .attachment_points
            .iter()
            .find(|point| point.id == point_id)
            .ok_or_else(|| format!("Parent has no attachment point {point_id}"))?;
        if !attachment_point_matches_requirement(point, *requirement) {
            return Err(format!(
                "Attachment point {point_id} uses {:?} order {}, but placement requires {:?} order {}",
                point.channel, point.order, requirement.channel, requirement.order
            ));
        }
        if !point.accepts_tags.is_empty()
            && !definition
                .attachment_tags
                .iter()
                .any(|tag| point.accepts_tags.contains(tag))
        {
            return Err(format!(
                "{} is incompatible with attachment point {point_id}",
                inventory.item_id
            ));
        }
        let capacity_index =
            first_free_attachment_capacity(point.capacity, inventory_item_id, |index| {
                if reserved_capacity.contains(&(parent_id, point_id.to_owned(), index)) {
                    return Some(u64::MAX);
                }
                ctx.db
                    .equipment_occupancy()
                    .id()
                    .find(attachment_occupancy_id(parent_id, point_id, index))
                    .map(|row| row.inventory_item_id)
            })
            .or_else(|| {
                replace_occupied.then(|| {
                    (0..point.capacity).find(|index| {
                        !reserved_capacity.contains(&(parent_id, point_id.to_owned(), *index))
                            && ctx
                                .db
                                .equipment_occupancy()
                                .id()
                                .find(attachment_occupancy_id(parent_id, point_id, *index))
                                .is_some_and(|row| row.inventory_item_id != inventory_item_id)
                    })
                })?
            })
            .ok_or_else(|| format!("Attachment point {point_id} is full"))?;
        if let Some(displaced) = ctx
            .db
            .equipment_occupancy()
            .id()
            .find(attachment_occupancy_id(parent_id, point_id, capacity_index))
            .map(|row| row.inventory_item_id)
            .filter(|occupant| *occupant != inventory_item_id)
        {
            displaced_item_ids.insert(displaced);
        }
        reserved_capacity.insert((parent_id, point_id.to_owned(), capacity_index));
        parent_occupancies.push((
            parent_id,
            point_id.to_owned(),
            capacity_index,
            target.requirement_index,
            *requirement,
        ));
    }
    if attachment_would_create_cycle(
        inventory_item_id,
        targets.iter().map(|target| target.parent_inventory_item_id),
        |ancestor_id| {
            ctx.db
                .equipment_occupancy()
                .inventory_item_id()
                .filter(ancestor_id)
                .filter_map(|row| row.parent_inventory_item_id)
                .collect()
        },
    ) {
        return Err("Attachment would create an equipment cycle".into());
    }

    for displaced_item_id in &displaced_item_ids {
        ctx.db
            .character_equipped_item()
            .inventory_item_id()
            .find(*displaced_item_id)
            .filter(|row| row.character_id == character_id)
            .ok_or("Displaced item is not equipped by this character")?;
        require_no_equipped_children(ctx, *displaced_item_id)?;
    }

    unequip_wearable(ctx, inventory_item_id);
    for displaced_item_id in displaced_item_ids {
        unequip_wearable(ctx, displaced_item_id);
    }
    ctx.db
        .character_equipped_item()
        .insert(CharacterEquippedItem {
            inventory_item_id,
            character_id,
            placement_id: placement.id.clone(),
        });
    for (requirement_index, requirement) in root_occupancies {
        ctx.db.equipment_occupancy().insert(EquipmentOccupancy {
            id: character_occupancy_id(
                character_id,
                requirement.channel,
                requirement.order,
                requirement.location,
            ),
            character_id,
            inventory_item_id,
            anchor_kind: EquipmentAnchorKind::CharacterLocation,
            location: Some(requirement.location),
            parent_inventory_item_id: None,
            attachment_point_id: None,
            channel: requirement.channel,
            order: requirement.order,
            requirement_index,
            capacity_index: 0,
        });
    }
    for (parent_id, point_id, capacity_index, requirement_index, requirement) in parent_occupancies
    {
        ctx.db.equipment_occupancy().insert(EquipmentOccupancy {
            id: attachment_occupancy_id(parent_id, &point_id, capacity_index),
            character_id,
            inventory_item_id,
            anchor_kind: EquipmentAnchorKind::ItemAttachment,
            location: None,
            parent_inventory_item_id: Some(parent_id),
            attachment_point_id: Some(point_id),
            channel: requirement.channel,
            order: requirement.order,
            requirement_index,
            capacity_index,
        });
    }
    refresh_equipment_dependents(ctx, character_id)?;
    Ok(())
}

fn hand_only_placement_is_held_root(placement: &crate::item::EquipmentPlacement) -> bool {
    placement.parents.is_empty()
        && placement.occupancy.len() == 1
        && matches!(
            placement.occupancy[0].location,
            crate::item::EquipmentLocation::LeftHand | crate::item::EquipmentLocation::RightHand
        )
        && placement.occupancy[0].channel == crate::item::EquipmentChannel::Held
        && placement.occupancy[0].order == 0
}

/// Adds the physical carry kit implied by a generated loadout. A suitable
/// weapon already authored in a hand stays there; a suitable sidearm that was
/// not initially equipped is placed inside the generated sheath.
fn provision_generated_weapon_carry(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    let sheathable: Vec<_> = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter(|inventory| {
            adventuresim_core::item_catalog::is_sheathable_weapon(&inventory.item_id)
        })
        .collect();
    if sheathable.is_empty() {
        return Ok(());
    }
    if sheathable.len() != 1 {
        return Err(format!(
            "Generated loadout has {} sheathable weapons but the authored belt kit carries exactly one",
            sheathable.len()
        ));
    }

    let belt = add_inventory_item(ctx, character_id, "leather_belt", 1)
        .ok_or("Could not add generated leather belt")?;
    let belt_placement = authored_placement_index(ctx, "leather_belt", "worn")?;
    equip_equipment_internal(
        ctx,
        character_id,
        belt,
        belt_placement,
        Vec::new(),
        false,
        false,
    )?;

    let sheath = add_inventory_item(ctx, character_id, "sword_sheath", 1)
        .ok_or("Could not add generated sword sheath")?;
    let sheath_placement = authored_placement_index(ctx, "sword_sheath", "attached")?;
    equip_equipment_internal(
        ctx,
        character_id,
        sheath,
        sheath_placement,
        vec![
            EquipmentAttachmentTargetSelection {
                requirement_index: 0,
                parent_inventory_item_id: belt,
                attachment_point_id: "left".into(),
            },
            EquipmentAttachmentTargetSelection {
                requirement_index: 1,
                parent_inventory_item_id: belt,
                attachment_point_id: "right".into(),
            },
        ],
        false,
        false,
    )?;

    let weapon = &sheathable[0];
    let initially_held = ctx
        .db
        .character_equipped_item()
        .inventory_item_id()
        .find(weapon.id)
        .is_some();
    if !initially_held {
        let placement = sheath_compatible_parent_placement_index(ctx, &weapon.item_id)?;
        equip_equipment_internal(
            ctx,
            character_id,
            weapon.id,
            placement,
            vec![EquipmentAttachmentTargetSelection {
                requirement_index: 0,
                parent_inventory_item_id: sheath,
                attachment_point_id: "blade".into(),
            }],
            false,
            false,
        )?;
    }
    Ok(())
}

fn authored_placement_index(
    ctx: &ReducerContext,
    item_id: &str,
    placement_id: &str,
) -> Result<u16, String> {
    let definition = ctx
        .db
        .item()
        .id()
        .find(item_id.to_owned())
        .ok_or_else(|| format!("Generated carry kit item {item_id} is missing"))?;
    definition
        .equipment_placements
        .iter()
        .position(|placement| placement.id == placement_id)
        .and_then(|index| u16::try_from(index).ok())
        .ok_or_else(|| format!("{item_id} lacks authored placement {placement_id}"))
}

fn sheath_compatible_parent_placement_index(
    ctx: &ReducerContext,
    item_id: &str,
) -> Result<u16, String> {
    let definition = ctx
        .db
        .item()
        .id()
        .find(item_id.to_owned())
        .ok_or_else(|| format!("Generated sidearm {item_id} is missing"))?;
    select_sheath_compatible_parent_placement(&definition.equipment_placements).ok_or_else(|| {
        format!("Sheathable sidearm {item_id} lacks a single containment parent placement")
    })
}

fn select_sheath_compatible_parent_placement(
    placements: &[crate::item::EquipmentPlacement],
) -> Option<u16> {
    placements
        .iter()
        .position(|placement| {
            placement.parents.len() == 1
                && placement.parents[0].channel == crate::item::EquipmentChannel::Containment
                && placement.parents[0].order == 0
        })
        .and_then(|index| u16::try_from(index).ok())
}

pub fn add_and_equip_item(
    ctx: &ReducerContext,
    character_id: u64,
    item_id: &str,
    destination: ItemSlot,
) -> Result<(), String> {
    let id = add_inventory_item(ctx, character_id, item_id, 1)
        .ok_or_else(|| "Can't add item to inventory".to_string())?;
    // This helper is used only while materializing starter equipment. The
    // completed character runs the ordinary compliance pass once every starter
    // item is present, avoiding partial creation when its initial settlement
    // restricts arms or armor.
    equip_item_internal(ctx, character_id, id, destination, false)
}

#[cfg(test)]
mod starting_character_boundary_tests {
    use super::{
        CharacterCreationMode, NpcLifeFacts, attachment_point_matches_requirement,
        attachment_would_create_cycle, conflicting_root_requirements,
        first_free_attachment_capacity, hand_only_placement_is_held_root,
        initial_membership_minutes, select_sheath_compatible_parent_placement,
    };
    use crate::item::{
        EquipmentAttachmentPoint, EquipmentChannel, EquipmentLocation,
        EquipmentOccupancyRequirement, EquipmentParentRequirement, EquipmentPlacement,
    };

    #[test]
    fn membership_period_is_anchored_to_current_character_time() {
        assert_eq!(initial_membership_minutes(9_000, None), (9_000, u64::MAX));
        assert_eq!(
            initial_membership_minutes(9_000, Some(30)),
            (
                9_000,
                9_000 + 30 * adventuresim_core::strategic_time::MINUTES_PER_DAY
            )
        );
    }

    #[test]
    fn herbalism_foraging_demo_resets_mutually_exclusive_presence() {
        let source = include_str!("character.rs");
        let fixture = source
            .split("pub(crate) fn seed_herbalism_demo_character")
            .nth(1)
            .unwrap()
            .split("pub(crate) fn seed_bestiary_scholar_character")
            .next()
            .unwrap();
        for required in [
            "if !party.is_solo",
            "party.leader_id != HERBALISM_DEMO_CHARACTER_ID",
            "members.len() != 1",
            "members[0].character_id != HERBALISM_DEMO_CHARACTER_ID",
            "set_character_case_site(ctx, HERBALISM_DEMO_CHARACTER_ID, None)",
            "ensure_foraging_demo_settlement(ctx)",
            "character.current_settlement_id = Some(HERBALISM_DEMO_SETTLEMENT_ID.into())",
            "party.current_settlement_id = Some(HERBALISM_DEMO_SETTLEMENT_ID.into())",
            "party.current_case_site_id = None",
            "party.camp_destination = None",
            "party.camp_remaining_minutes = 0",
            "finish_party_journey(ctx, &party_id)",
            "strategic_encounter().party_id().delete(&party_id)",
        ] {
            assert!(fixture.contains(required), "missing reset step: {required}");
        }
        let validation = fixture.find("if !party.is_solo").unwrap();
        let first_reset = fixture
            .find("set_character_case_site(ctx, HERBALISM_DEMO_CHARACTER_ID, None)")
            .unwrap();
        assert!(
            validation < first_reset,
            "party ownership must be validated first"
        );
        let scenarios = include_str!("strategic/development_scenarios.rs");
        let settlement = scenarios
            .split("pub(crate) fn ensure_foraging_demo_settlement")
            .nth(1)
            .unwrap()
            .split("fn ensure_scenario_character_at")
            .next()
            .unwrap();
        assert!(settlement.contains("const ID: &str = \"dev-scenario-foraging\""));
        assert!(settlement.contains("settlement.coord_x = 9.75"));
        assert!(settlement.contains("settlement.coord_y = 51.75"));
        assert!(
            settlement.find("settlement.coord_y = 51.75").unwrap()
                < settlement.find("ensure_settlement_activity").unwrap()
        );
    }

    #[test]
    fn stable_npc_life_is_order_independent_and_adult_bounded() {
        let first = NpcLifeFacts::from_stable_seed(31);
        let _unrelated = NpcLifeFacts::from_stable_seed(99);
        let repeated = NpcLifeFacts::from_stable_seed(31);
        assert_eq!(first, repeated);
        assert!((18..=60).contains(&first.age_years));
    }

    #[test]
    fn creation_modes_separate_persistent_npcs_from_tactical_temporaries() {
        assert!(!CharacterCreationMode::Player.is_npc());
        assert!(!CharacterCreationMode::Player.temporary());
        assert!(CharacterCreationMode::PersistentNpc.is_npc());
        assert!(!CharacterCreationMode::PersistentNpc.temporary());
        assert!(CharacterCreationMode::TemporaryNpc.is_npc());
        assert!(CharacterCreationMode::TemporaryNpc.temporary());
        assert!(CharacterCreationMode::Newborn.is_npc());
        assert!(!CharacterCreationMode::Newborn.temporary());
        assert!(CharacterCreationMode::Newborn.newborn());

        let source = include_str!("character.rs");
        let persistent = source
            .split("pub(crate) fn insert_persistent_npc_character")
            .nth(1)
            .unwrap()
            .split("fn insert_starting_character")
            .next()
            .unwrap();
        assert!(persistent.contains("origin_settlement_id: Some(origin_settlement_id)"));
        assert!(persistent.contains("CharacterCreationMode::PersistentNpc"));
        assert!(persistent.contains("create_solo_party: false"));
        assert!(persistent.contains("initial_time_minute"));
    }

    #[test]
    fn full_character_rows_are_gateway_only() {
        let source = include_str!("character.rs");
        assert!(source.contains("#[table(accessor = character)]"));
        assert!(!source.contains("#[table(accessor = character, public)]"));
        for component in [
            "character_attributes",
            "character_stats",
            "character_skills",
            "character_limbs",
        ] {
            assert!(source.contains(&format!("#[table(accessor = {component})]")));
            assert!(!source.contains(&format!("#[table(accessor = {component}, public)]")));
        }
        let projection = source.split("pub fn backend_characters").nth(1).unwrap();
        assert!(projection.contains("strategic_gateway_authority()"));
        assert!(projection.contains("authority.identity == ctx.sender()"));
        assert!(projection.contains("character().scan_id().filter(0u64..)"));
        for view in [
            "backend_character_attributes",
            "backend_character_stats",
            "backend_character_skills",
            "backend_character_limbs",
            "backend_character_times",
            "backend_character_training_schedules",
            "backend_character_capabilities",
            "backend_character_conditions",
            "backend_character_needs",
            "backend_character_exposures",
            "backend_character_strategic_conditions",
            "backend_character_morale_sources",
            "backend_character_deaths",
        ] {
            assert!(projection.contains(&format!("pub fn {view}")));
        }
        assert!(source.contains("#[table(accessor = character_death)]"));
        assert!(!source.contains("#[table(accessor = character_death, public)]"));
        let condition_source = include_str!("condition.rs");
        assert!(condition_source.contains("#[table(accessor = character_morale_source)]"));
        assert!(!condition_source.contains("#[table(accessor = character_morale_source, public)]"));
    }

    #[test]
    fn newborns_have_full_components_without_an_adult_starter_package() {
        let source = include_str!("character.rs");
        let insertion = source
            .split("pub(crate) fn insert_character_with_origin")
            .nth(1)
            .unwrap()
            .split("pub(crate) fn validate_full_character_components")
            .next()
            .unwrap();
        assert!(insertion.contains("Newborn creation requires a reserved child identity"));
        assert!(insertion.contains("if !newborn {\n        crate::item::credit_personal_currency"));
        assert!(insertion.contains("} else if !newborn {\n        add_inventory_item"));
        assert!(insertion.contains("validate_full_character_components(ctx, character.id)?"));
    }

    #[test]
    fn completed_creation_checks_the_full_component_invariant() {
        let source = include_str!("character.rs");
        let insertion = source
            .split("pub(crate) fn insert_character_with_origin")
            .nth(1)
            .unwrap()
            .split("pub(crate) fn validate_full_character_components")
            .next()
            .unwrap();
        assert!(insertion.contains("validate_full_character_components(ctx, character.id)?"));
        for component in [
            "character_stats",
            "character_attributes",
            "character_skills",
            "character_limbs",
            "character_personality",
            "character_time",
            "character_training_schedule",
            "character_capability",
            "character_condition",
            "character_needs",
            "character_exposure",
            "character_strategic_condition",
        ] {
            assert!(
                source.contains(component),
                "missing invariant for {component}"
            );
        }
    }

    #[test]
    fn non_newborn_materialization_adds_one_authored_belt_sheath_kit() {
        let source = include_str!("character.rs");
        let insertion = source
            .split("pub(crate) fn insert_character_with_origin")
            .nth(1)
            .unwrap()
            .split("pub(crate) fn validate_full_character_components")
            .next()
            .unwrap();
        assert!(insertion.contains(
            "if !newborn {\n        provision_generated_weapon_carry(ctx, character.id)?;\n    }"
        ));
        let helper = source
            .split("fn provision_generated_weapon_carry")
            .nth(1)
            .unwrap()
            .split("pub fn add_and_equip_item")
            .next()
            .unwrap();
        for evidence in [
            "is_sheathable_weapon",
            "leather_belt",
            "sword_sheath",
            "attachment_point_id: \"left\"",
            "attachment_point_id: \"right\"",
            "attachment_point_id: \"blade\"",
            "if !initially_held",
        ] {
            assert!(helper.contains(evidence), "missing {evidence}");
        }
    }

    #[test]
    fn creation_initializes_condition_before_capability_dependent_side_effects() {
        let source = include_str!("character.rs");
        let insertion = source
            .split("pub(crate) fn insert_character_with_origin")
            .nth(1)
            .unwrap()
            .split("pub(crate) fn validate_full_character_components")
            .next()
            .unwrap();
        let condition = insertion
            .find("crate::condition::initialize_character_condition(ctx, character.id)?")
            .unwrap();
        let currency = insertion
            .find("crate::item::credit_personal_currency(")
            .unwrap();
        let starter_inventory = insertion
            .find("add_inventory_item(ctx, character.id, \"torch\", 1)")
            .unwrap();
        let capability = insertion
            .find("crate::capability::refresh_character_capability(ctx, character.id)?")
            .unwrap();
        assert!(
            condition < capability,
            "capability refresh loads the character condition and must run after initialization"
        );
        assert!(
            condition < currency,
            "currency mutations may refresh capability and must run after condition initialization"
        );
        assert!(
            condition < starter_inventory,
            "starter inventory mutations may refresh capability and must run after condition initialization"
        );
    }

    #[test]
    fn public_starting_character_reducer_requires_the_gateway() {
        let source = include_str!("character.rs");
        let reducer = source
            .split("pub fn create_starting_character")
            .nth(1)
            .unwrap()
            .split("#[reducer]")
            .next()
            .unwrap();
        assert!(reducer.contains("require_strategic_gateway(ctx)?"));
    }

    #[test]
    fn equipment_graph_rejects_self_descendant_and_corrupt_ancestor_cycles() {
        assert!(attachment_would_create_cycle(7, [7], |_| vec![]));
        assert!(attachment_would_create_cycle(7, [9], |id| match id {
            9 => vec![8],
            8 => vec![7],
            _ => vec![],
        }));
        assert!(attachment_would_create_cycle(7, [9], |id| match id {
            9 => vec![8],
            8 => vec![9],
            _ => vec![],
        }));
        assert!(!attachment_would_create_cycle(7, [9], |id| {
            (id == 9).then_some(8).into_iter().collect()
        }));
        assert!(!attachment_would_create_cycle(7, [9, 10], |id| match id {
            9 | 10 => vec![8],
            _ => vec![],
        }));
    }

    #[test]
    fn equipment_mutation_preflights_before_deleting_the_old_graph_rows() {
        let source = include_str!("character.rs");
        let reducer = source
            .split("fn equip_equipment_internal")
            .nth(1)
            .unwrap()
            .split("pub fn add_and_equip_item")
            .next()
            .unwrap();
        let conflict_check = reducer
            .find("if !replace_occupied && !conflicts.is_empty()")
            .unwrap();
        let parent_check = reducer
            .find("if targets.len() != placement.parents.len()")
            .unwrap();
        let orphan_guard = reducer.find("require_no_equipped_children").unwrap();
        let mutation = reducer
            .find("unequip_wearable(ctx, inventory_item_id)")
            .unwrap();
        let displaced_guard = reducer
            .find("for displaced_item_id in &displaced_item_ids")
            .unwrap();
        assert!(conflict_check < mutation);
        assert!(parent_check < mutation);
        assert!(orphan_guard < mutation);
        assert!(displaced_guard < mutation);

        let orphan_helper = source
            .split("fn require_no_equipped_children")
            .nth(1)
            .unwrap()
            .split("pub(crate) fn restore_equipment_placement")
            .next()
            .unwrap();
        assert!(orphan_helper.contains("row.parent_inventory_item_id == Some(inventory_item_id)"));
        assert!(orphan_helper.contains("Detach or remove attached/contained items first"));
        assert!(reducer.contains("WeaponCarry::HandOnly"));
        assert!(
            reducer.find("WeaponCarry::HandOnly").unwrap() < mutation,
            "hand-only parent placement must fail before mutation"
        );
    }

    #[test]
    fn durable_hand_only_boundary_accepts_only_one_held_hand_root() {
        let held = EquipmentOccupancyRequirement {
            location: EquipmentLocation::LeftHand,
            channel: EquipmentChannel::Held,
            order: 0,
        };
        let mut placement = EquipmentPlacement {
            id: "left_hand".into(),
            occupancy: vec![held],
            parents: Vec::new(),
            protection: Vec::new(),
        };
        assert!(hand_only_placement_is_held_root(&placement));

        placement.occupancy[0].location = EquipmentLocation::Chest;
        assert!(!hand_only_placement_is_held_root(&placement));
        placement.occupancy[0] = held;
        placement.occupancy.push(held);
        assert!(!hand_only_placement_is_held_root(&placement));
        placement.occupancy.pop();
        placement.parents.push(EquipmentParentRequirement {
            channel: EquipmentChannel::Containment,
            order: 0,
        });
        assert!(!hand_only_placement_is_held_root(&placement));
    }

    #[test]
    fn generated_sheath_selection_skips_arbitrary_parent_placements() {
        let parent_placement = |id: &str, parents| EquipmentPlacement {
            id: id.into(),
            occupancy: Vec::new(),
            parents,
            protection: Vec::new(),
        };
        let placements = vec![
            parent_placement(
                "mounted",
                vec![EquipmentParentRequirement {
                    channel: EquipmentChannel::Mount,
                    order: 0,
                }],
            ),
            parent_placement(
                "two_parents",
                vec![
                    EquipmentParentRequirement {
                        channel: EquipmentChannel::Containment,
                        order: 0,
                    },
                    EquipmentParentRequirement {
                        channel: EquipmentChannel::Containment,
                        order: 0,
                    },
                ],
            ),
            parent_placement(
                "wrong_order",
                vec![EquipmentParentRequirement {
                    channel: EquipmentChannel::Containment,
                    order: 1,
                }],
            ),
            parent_placement(
                "sheathed",
                vec![EquipmentParentRequirement {
                    channel: EquipmentChannel::Containment,
                    order: 0,
                }],
            ),
        ];
        assert_eq!(
            select_sheath_compatible_parent_placement(&placements),
            Some(3)
        );
    }

    #[test]
    fn strategic_slot_swaps_use_the_explicit_atomic_reducer() {
        let source = include_str!("character.rs");
        let reducer = source
            .split("pub fn replace_item_at_placement")
            .nth(1)
            .unwrap()
            .split("fn equip_equipment_internal")
            .next()
            .unwrap();
        assert!(reducer.contains("require_strategic_character_authority"));
        assert!(reducer.contains("true,\n        true,"));
    }

    #[test]
    fn multi_location_conflicts_are_collected_without_displacing_any_item() {
        let left = EquipmentOccupancyRequirement {
            location: EquipmentLocation::LeftArm,
            channel: EquipmentChannel::Padding,
            order: 0,
        };
        let right = EquipmentOccupancyRequirement {
            location: EquipmentLocation::RightArm,
            channel: EquipmentChannel::Padding,
            order: 0,
        };
        let conflicts = conflicting_root_requirements(&[left, right], 10, |requirement| {
            match requirement.location {
                EquipmentLocation::LeftArm => Some(20),
                EquipmentLocation::RightArm => Some(30),
                _ => None,
            }
        });
        assert_eq!(conflicts, vec![left, right]);
        assert!(conflicting_root_requirements(&[left], 20, |_| Some(20)).is_empty());
    }

    #[test]
    fn attachment_capacity_is_stable_full_and_reparent_idempotent() {
        assert_eq!(
            first_free_attachment_capacity(3, 99, |index| match index {
                0 => Some(10),
                1 => None,
                _ => Some(11),
            }),
            Some(1)
        );
        assert_eq!(first_free_attachment_capacity(2, 99, |_| Some(10)), None);
        assert_eq!(
            first_free_attachment_capacity(2, 99, |index| { (index == 0).then_some(99) }),
            Some(0),
            "an item's existing capacity cell remains a valid atomic reparent target"
        );
    }

    #[test]
    fn parent_requirements_match_both_channel_and_authored_order() {
        let point = EquipmentAttachmentPoint {
            id: "right".into(),
            channel: EquipmentChannel::Mount,
            capacity: 1,
            order: 1,
            accepts_tags: Vec::new(),
        };
        assert!(attachment_point_matches_requirement(
            &point,
            EquipmentParentRequirement {
                channel: EquipmentChannel::Mount,
                order: 1,
            }
        ));
        assert!(!attachment_point_matches_requirement(
            &point,
            EquipmentParentRequirement {
                channel: EquipmentChannel::Mount,
                order: 0,
            }
        ));
    }

    #[test]
    fn normalized_equipment_mutations_refresh_capability_and_condition() {
        let source = include_str!("character.rs");
        let helper = source
            .split("fn refresh_equipment_dependents")
            .nth(1)
            .unwrap()
            .split("#[reducer]")
            .next()
            .unwrap();
        assert!(helper.contains("refresh_character_capability"));
        assert!(helper.contains("refresh_character_strategic_condition"));
        let reducer = source
            .split("fn equip_equipment_internal")
            .nth(1)
            .unwrap()
            .split("pub fn add_and_equip_item")
            .next()
            .unwrap();
        assert!(reducer.contains("refresh_equipment_dependents(ctx, character_id)"));
    }

    #[test]
    fn production_full_characters_simulate_life_but_tactical_fixtures_keep_authored_overrides() {
        let source = include_str!("character.rs");
        let insertion = source
            .split("fn insert_character_with_origin")
            .nth(1)
            .unwrap()
            .split("#[reducer]\npub fn equip_item")
            .next()
            .unwrap();
        assert!(insertion.contains("starting.is_none() && (!temporary || npc)"));
        assert!(insertion.contains("life_simulation::simulate_life"));
        let tactical = source
            .split("pub fn create_temporary_character")
            .nth(1)
            .unwrap()
            .split("fn scale_temporary_enemy")
            .next()
            .unwrap();
        assert!(tactical.contains("insert_new_character"));
        assert!(tactical.contains("scale_temporary_enemy"));
    }

    #[test]
    fn creation_persists_only_the_current_skill_projection() {
        let source = include_str!("character.rs");
        assert!(source.contains("character_skills().insert"));
        for forbidden in [
            concat!("CharacterTraining", "History"),
            concat!("character_training_", "history"),
            concat!("CharacterSchedule", "History"),
            concat!("character_schedule_", "history"),
            concat!("CharacterActivity", "History"),
            concat!("character_activity_", "history"),
        ] {
            assert!(!source.contains(forbidden));
        }
    }
}
