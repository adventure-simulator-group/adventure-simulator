use spacetimedb::{Identity, ReducerContext, SpacetimeType, Table, reducer, table};

use crate::{
    character::{
        character, character_attributes, character_equip, character_limbs, character_skills,
    },
    item::{InventoryItem, inventory_item, item},
    tactical::tactical_server_request,
    time::advance_character_time,
};
use std::collections::{BinaryHeap, HashMap, HashSet};

const MERCHANT_MARGIN: f32 = 1.25;
const SALES_TAX: f32 = 0.10;
const WALKING_SPEED_KM_PER_HOUR: u64 = 5;
const QUEST_TRAVEL_SPEED_DIVISOR: u64 = 4;
const METERS_PER_KILOMETER: u64 = 1_000;
const MINUTES_PER_HOUR: u64 = 60;
const QUESTS_PER_SETTLEMENT: usize = 3;

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestStatus {
    Available,
    Accepted,
    Completed,
}

#[derive(Clone, Debug)]
#[table(name = settlement, public)]
pub struct Settlement {
    #[primary_key]
    pub id: String,
    pub name: String,
    pub coord_x: f64,
    pub coord_y: f64,
    pub population_level: i32,
    /// Approximate population in inhabitants; zero means the world data has no estimate.
    pub population_estimate: u32,
    pub scene_key: String,
    /// Viabundus node that supplies this settlement, if it was imported from
    /// the historical world dataset. Demo settlements deliberately leave this
    /// empty.
    pub source_node_id: Option<u64>,
}

/// A navigational point in the imported Viabundus network. This contains the
/// topology required for strategic routing, not tactical state or map artwork.
#[derive(Clone, Debug)]
#[table(name = world_node, public)]
pub struct WorldNode {
    #[primary_key]
    pub id: u64,
    pub parent_node_id: Option<u64>,
    pub latitude: f64,
    pub longitude: f64,
    pub is_settlement: bool,
    pub is_town: bool,
    pub is_ferry: bool,
    pub is_harbour: bool,
}

/// An active 1544 land-network segment. Geometry remains an offline map asset;
/// the strategic database needs only endpoint topology and travel metadata.
#[derive(Clone, Debug)]
#[table(name = travel_edge, public)]
pub struct TravelEdge {
    #[primary_key]
    pub id: u64,
    #[index(btree)]
    pub from_node_id: u64,
    #[index(btree)]
    pub to_node_id: u64,
    pub kind: String,
    pub length_m: u32,
    pub slope_multiplier: f32,
    pub certainty: u8,
    pub section: String,
}

/// The identity that started the one-time local world-data import. All later
/// batches must come from the same identity.
#[derive(Clone, Debug)]
#[table(name = world_data_import, public)]
pub struct WorldDataImport {
    #[primary_key]
    pub id: u8,
    pub owner: Identity,
}

#[derive(SpacetimeType, Clone, Debug)]
pub struct WorldNodeImport {
    pub id: u64,
    pub parent_node_id: Option<u64>,
    pub latitude: f64,
    pub longitude: f64,
    pub is_settlement: bool,
    pub is_town: bool,
    pub is_ferry: bool,
    pub is_harbour: bool,
}

#[derive(SpacetimeType, Clone, Debug)]
pub struct TravelEdgeImport {
    pub id: u64,
    pub from_node_id: u64,
    pub to_node_id: u64,
    pub kind: String,
    pub length_m: u32,
    pub slope_multiplier: f32,
    pub certainty: u8,
    pub section: String,
}

#[derive(SpacetimeType, Clone, Debug)]
pub struct SettlementImport {
    pub id: String,
    pub source_node_id: u64,
    pub name: String,
    pub longitude: f64,
    pub latitude: f64,
    pub population_level: i32,
    /// Viabundus records this approximation in thousands of inhabitants; zero means absent.
    pub population_estimate: u32,
    pub scene_key: String,
}

/// Start a world import. This must be called before sending any import batch.
/// The first caller becomes the owner of this import session; in production the
/// deployment operator must claim it before the database is opened to players.
#[reducer]
pub fn begin_world_data_import(ctx: &ReducerContext) -> Result<(), String> {
    match ctx.db.world_data_import().id().find(0) {
        Some(import) if import.owner == ctx.sender => Ok(()),
        Some(_) => Err("World data import is owned by another identity".into()),
        None => {
            ctx.db.world_data_import().insert(WorldDataImport {
                id: 0,
                owner: ctx.sender,
            });
            Ok(())
        }
    }
}

fn require_world_import_owner(ctx: &ReducerContext) -> Result<(), String> {
    let Some(import) = ctx.db.world_data_import().id().find(0) else {
        return Err("Call begin_world_data_import before loading world data".into());
    };
    if import.owner != ctx.sender {
        return Err("Only the world data import owner may load batches".into());
    }
    Ok(())
}

#[reducer]
pub fn import_world_nodes(ctx: &ReducerContext, nodes: Vec<WorldNodeImport>) -> Result<(), String> {
    require_world_import_owner(ctx)?;
    if nodes.is_empty() {
        return Err("World-node batch is empty".into());
    }
    for node in nodes {
        let row = WorldNode {
            id: node.id,
            parent_node_id: node.parent_node_id,
            latitude: node.latitude,
            longitude: node.longitude,
            is_settlement: node.is_settlement,
            is_town: node.is_town,
            is_ferry: node.is_ferry,
            is_harbour: node.is_harbour,
        };
        if ctx.db.world_node().id().find(row.id).is_some() {
            ctx.db.world_node().id().update(row);
        } else {
            ctx.db.world_node().insert(row);
        }
    }
    Ok(())
}

#[reducer]
pub fn import_travel_edges(
    ctx: &ReducerContext,
    edges: Vec<TravelEdgeImport>,
) -> Result<(), String> {
    require_world_import_owner(ctx)?;
    if edges.is_empty() {
        return Err("Travel-edge batch is empty".into());
    }
    for edge in edges {
        if ctx.db.world_node().id().find(edge.from_node_id).is_none()
            || ctx.db.world_node().id().find(edge.to_node_id).is_none()
        {
            return Err(format!(
                "Travel edge {} references an unknown world node",
                edge.id
            ));
        }
        let row = TravelEdge {
            id: edge.id,
            from_node_id: edge.from_node_id,
            to_node_id: edge.to_node_id,
            kind: edge.kind,
            length_m: edge.length_m,
            slope_multiplier: edge.slope_multiplier,
            certainty: edge.certainty,
            section: edge.section,
        };
        if ctx.db.travel_edge().id().find(row.id).is_some() {
            ctx.db.travel_edge().id().update(row);
        } else {
            ctx.db.travel_edge().insert(row);
        }
    }
    Ok(())
}

#[reducer]
pub fn import_settlements(
    ctx: &ReducerContext,
    settlements: Vec<SettlementImport>,
) -> Result<(), String> {
    require_world_import_owner(ctx)?;
    if settlements.is_empty() {
        return Err("Settlement batch is empty".into());
    }
    for settlement in settlements {
        if ctx
            .db
            .world_node()
            .id()
            .find(settlement.source_node_id)
            .is_none()
        {
            return Err(format!(
                "Settlement {} references an unknown world node",
                settlement.id
            ));
        }
        let row = Settlement {
            id: settlement.id,
            name: settlement.name,
            coord_x: settlement.longitude,
            coord_y: settlement.latitude,
            population_level: settlement.population_level,
            population_estimate: settlement.population_estimate,
            scene_key: settlement.scene_key,
            source_node_id: Some(settlement.source_node_id),
        };
        let settlement_id = row.id.clone();
        if ctx.db.settlement().id().find(&row.id).is_some() {
            ctx.db.settlement().id().update(row);
        } else {
            ctx.db.settlement().insert(row);
        }
        ensure_settlement_quests(ctx, &settlement_id)?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
#[table(name = quest, public)]
pub struct Quest {
    #[primary_key]
    pub id: String,
    pub title: String,
    pub description: String,
    pub difficulty: i32,
    pub gold_reward: i32,
    pub xp_reward: i32,
    #[index(btree)]
    pub settlement_id: String,
    pub status: QuestStatus,
    pub accepted_by: Option<String>,
    pub enemy_type: String,
    pub enemy_count: i32,
    pub location_description: String,
    pub location_scene_key: String,
    pub location_coord_x: f64,
    pub location_coord_y: f64,
    pub coordinates_are_geographic: bool,
    pub distance_m: u64,
}

#[derive(Clone, Debug)]
#[table(name = party, public)]
pub struct Party {
    #[primary_key]
    pub id: String,
    pub name: String,
    pub leader_id: u64,
    pub current_settlement_id: Option<String>,
    pub current_quest_location_id: Option<String>,
    pub active_quest_id: Option<String>,
    pub is_solo: bool,
}

#[derive(Clone, Debug)]
#[table(name = party_member, public)]
pub struct PartyMember {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub party_id: String,
    #[index(btree)]
    pub character_id: u64,
    pub role: Option<String>,
    pub recruitment_role_id: Option<u64>,
}

#[derive(SpacetimeType, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecruitmentRequirements {
    pub melee: bool,
    pub ranged: bool,
    pub precise: bool,
    pub heavy: bool,
    pub quarter_armor: bool,
    pub half_armor: bool,
    pub three_quarter_armor: bool,
    pub full_armor: bool,
    pub blunt: bool,
    pub slash: bool,
    pub pierce: bool,
    pub athletics: u8,
    pub endurance: u8,
    pub medicine: u8,
    pub surgery: u8,
    pub charisma: u8,
    pub faith: u8,
}

impl From<RecruitmentRequirements> for adventuresim_core::capability::RoleRequirements {
    fn from(value: RecruitmentRequirements) -> Self {
        Self {
            melee: value.melee,
            ranged: value.ranged,
            weapon_precision: adventuresim_core::capability::legacy_weapon_precision(
                value.precise,
                value.blunt,
                value.slash,
                value.pierce,
            ),
            heavy: value.heavy,
            quarter_armor: value.quarter_armor,
            half_armor: value.half_armor,
            three_quarter_armor: value.three_quarter_armor,
            full_armor: value.full_armor,
            athletics: value.athletics,
            endurance: value.endurance,
            medicine: value.medicine,
            surgery: value.surgery,
            charisma: value.charisma,
            faith: value.faith,
        }
    }
}

#[derive(Clone, Debug)]
#[table(name = party_recruitment_role, public)]
pub struct PartyRecruitmentRole {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub party_id: String,
    pub name: String,
    pub requirements: RecruitmentRequirements,
    pub quantity: u32,
    #[default(0.0)]
    pub weapon_precision: f32,
}

#[derive(Clone, Debug)]
#[table(name = saved_recruitment_role, public)]
pub struct SavedRecruitmentRole {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub owner_character_id: u64,
    pub name: String,
    pub requirements: RecruitmentRequirements,
    #[default(0.0)]
    pub weapon_precision: f32,
}

#[derive(Clone, Debug)]
#[table(name = party_join_request, public)]
pub struct PartyJoinRequest {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub party_id: String,
    #[index(btree)]
    pub recruitment_role_id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub meets_requirements: bool,
}

#[reducer]
pub fn update_character(ctx: &ReducerContext, id: u64, name: String) -> Result<(), String> {
    let Some(mut character) = ctx.db.character().id().find(id) else {
        return Err("Character not found".into());
    };

    character.name = name;
    ctx.db.character().id().update(character);
    Ok(())
}

pub(crate) fn create_solo_party_for_character(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<String, String> {
    let Some(mut character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    let party_id = format!("solo-{character_id}");
    if ctx.db.party().id().find(&party_id).is_none() {
        ctx.db.party().insert(Party {
            id: party_id.clone(),
            name: format!("{}'s party", character.name),
            leader_id: character_id,
            current_settlement_id: character.current_settlement_id.clone(),
            current_quest_location_id: character.current_quest_location_id.clone(),
            active_quest_id: None,
            is_solo: true,
        });
        ctx.db.party_member().insert(PartyMember {
            id: 0,
            party_id: party_id.clone(),
            character_id,
            role: Some("Leader".into()),
            recruitment_role_id: None,
        });
    }
    character.party_id = Some(party_id.clone());
    ctx.db.character().id().update(character);
    Ok(party_id)
}

#[reducer]
pub fn backfill_solo_parties(ctx: &ReducerContext) -> Result<(), String> {
    let ids: Vec<_> = ctx
        .db
        .character()
        .iter()
        .filter(|c| c.party_id.is_none())
        .map(|c| c.id)
        .collect();
    for id in ids {
        create_solo_party_for_character(ctx, id)?;
    }
    Ok(())
}

#[reducer]
pub fn create_recruitment_role(
    ctx: &ReducerContext,
    leader_id: u64,
    name: String,
    quantity: u32,
    requirements: RecruitmentRequirements,
    weapon_precision: f32,
    save_role: bool,
) -> Result<(), String> {
    if quantity == 0 || quantity > 8 {
        return Err("Role quantity must be between 1 and 8".into());
    }
    if !(0.0..=adventuresim_core::capability::WEAPON_PRECISION_RAPIER).contains(&weapon_precision)
        || (weapon_precision * 2.0).fract() != 0.0
    {
        return Err("Weapon precision must use a 0.5 step between 0 and 2".into());
    }
    if [
        requirements.athletics,
        requirements.endurance,
        requirements.medicine,
        requirements.surgery,
        requirements.charisma,
        requirements.faith,
    ]
    .iter()
    .any(|v| *v > 5)
    {
        return Err("Role ratings must be between 0 and 5".into());
    }
    let leader = ctx
        .db
        .character()
        .id()
        .find(leader_id)
        .ok_or("Leader not found")?;
    let party_id = leader.party_id.ok_or("Leader has no party")?;
    let party = ctx
        .db
        .party()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != leader_id {
        return Err("Only the party leader can create roles".into());
    }
    let role_name = if name.trim().is_empty() {
        "Any adventurer".to_string()
    } else {
        name.trim().to_string()
    };
    ctx.db
        .party_recruitment_role()
        .insert(PartyRecruitmentRole {
            id: 0,
            party_id,
            name: role_name.clone(),
            requirements,
            quantity,
            weapon_precision,
        });
    if save_role {
        if name.trim().is_empty() {
            return Err("Name a role before saving it".into());
        }
        ctx.db
            .saved_recruitment_role()
            .insert(SavedRecruitmentRole {
                id: 0,
                owner_character_id: leader_id,
                name: role_name,
                requirements,
                weapon_precision,
            });
    }
    Ok(())
}

#[reducer]
pub fn delete_saved_recruitment_role(
    ctx: &ReducerContext,
    owner_id: u64,
    role_id: u64,
) -> Result<(), String> {
    let role = ctx
        .db
        .saved_recruitment_role()
        .id()
        .find(role_id)
        .ok_or("Saved role not found")?;
    if role.owner_character_id != owner_id {
        return Err("Cannot delete another character's saved role".into());
    }
    ctx.db.saved_recruitment_role().id().delete(role_id);
    Ok(())
}

fn filled_role_slots(ctx: &ReducerContext, role_id: u64) -> u32 {
    ctx.db
        .party_member()
        .iter()
        .filter(|member| member.recruitment_role_id == Some(role_id))
        .count() as u32
}

fn role_requirements(
    role: &PartyRecruitmentRole,
) -> adventuresim_core::capability::RoleRequirements {
    let mut requirements = adventuresim_core::capability::RoleRequirements::from(role.requirements);
    requirements.weapon_precision = requirements.weapon_precision.max(role.weapon_precision);
    requirements
}

#[reducer]
pub fn request_to_join_party(
    ctx: &ReducerContext,
    character_id: u64,
    recruitment_role_id: u64,
) -> Result<(), String> {
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    let current_party_id = character
        .party_id
        .clone()
        .ok_or("Character has no solo party")?;
    let current_party = ctx
        .db
        .party()
        .id()
        .find(&current_party_id)
        .ok_or("Current party not found")?;
    if !current_party.is_solo || current_party.leader_id != character_id {
        return Err("Only a solo adventurer may request to join".into());
    }
    let role = ctx
        .db
        .party_recruitment_role()
        .id()
        .find(recruitment_role_id)
        .ok_or("Recruitment role not found")?;
    let party_id = role.party_id.clone();
    let Some(party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };
    if character.current_settlement_id != party.current_settlement_id
        || character.current_quest_location_id != party.current_quest_location_id
    {
        return Err("Must be in the same settlement as the party".into());
    }
    if filled_role_slots(ctx, role.id) >= role.quantity {
        return Err("Recruitment role is full".into());
    }
    if ctx
        .db
        .party_join_request()
        .character_id()
        .filter(character_id)
        .any(|request| request.recruitment_role_id == recruitment_role_id)
    {
        return Err("A join request is already pending".into());
    }
    let capabilities = crate::capability::refresh_character_capability(ctx, character_id)?;
    ctx.db.party_join_request().insert(PartyJoinRequest {
        id: 0,
        party_id,
        recruitment_role_id,
        character_id,
        meets_requirements: capabilities.meets(role_requirements(&role)),
    });
    Ok(())
}

#[reducer]
pub fn accept_party_join_request(
    ctx: &ReducerContext,
    leader_id: u64,
    request_id: u64,
) -> Result<(), String> {
    let Some(request) = ctx.db.party_join_request().id().find(request_id) else {
        return Err("Join request not found".into());
    };
    let Some(party) = ctx.db.party().id().find(&request.party_id) else {
        return Err("Party not found".into());
    };
    if party.leader_id != leader_id {
        return Err("Only the party leader can accept join requests".into());
    }
    let role = ctx
        .db
        .party_recruitment_role()
        .id()
        .find(request.recruitment_role_id)
        .ok_or("Recruitment role not found")?;
    if filled_role_slots(ctx, role.id) >= role.quantity {
        return Err("Recruitment role is full".into());
    }
    let mut character = ctx
        .db
        .character()
        .id()
        .find(request.character_id)
        .ok_or("Applicant not found")?;
    let solo_id = character
        .party_id
        .clone()
        .ok_or("Applicant has no solo party")?;
    let solo = ctx
        .db
        .party()
        .id()
        .find(&solo_id)
        .ok_or("Applicant solo party not found")?;
    if !solo.is_solo {
        return Err("Applicant is no longer solo".into());
    }
    for member in ctx
        .db
        .party_member()
        .party_id()
        .filter(&solo_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_member().id().delete(member.id);
    }
    for solo_role in ctx
        .db
        .party_recruitment_role()
        .party_id()
        .filter(&solo_id)
        .collect::<Vec<_>>()
    {
        for pending in ctx
            .db
            .party_join_request()
            .recruitment_role_id()
            .filter(solo_role.id)
            .collect::<Vec<_>>()
        {
            ctx.db.party_join_request().id().delete(pending.id);
        }
        ctx.db.party_recruitment_role().id().delete(solo_role.id);
    }
    ctx.db.party().id().delete(&solo_id);
    ctx.db.party_member().insert(PartyMember {
        id: 0,
        party_id: request.party_id.clone(),
        character_id: request.character_id,
        role: Some(role.name.clone()),
        recruitment_role_id: Some(role.id),
    });
    character.party_id = Some(request.party_id.clone());
    ctx.db.character().id().update(character);
    if party.is_solo {
        let mut party = party;
        party.is_solo = false;
        ctx.db.party().id().update(party);
    }
    let requests: Vec<_> = ctx
        .db
        .party_join_request()
        .character_id()
        .filter(request.character_id)
        .collect();
    for pending in requests {
        ctx.db.party_join_request().id().delete(pending.id);
    }
    if filled_role_slots(ctx, role.id) >= role.quantity {
        for pending in ctx
            .db
            .party_join_request()
            .recruitment_role_id()
            .filter(role.id)
            .collect::<Vec<_>>()
        {
            ctx.db.party_join_request().id().delete(pending.id);
        }
    }
    Ok(())
}

#[reducer]
pub fn reject_party_join_request(
    ctx: &ReducerContext,
    leader_id: u64,
    request_id: u64,
) -> Result<(), String> {
    let Some(request) = ctx.db.party_join_request().id().find(request_id) else {
        return Err("Join request not found".into());
    };
    let Some(party) = ctx.db.party().id().find(&request.party_id) else {
        return Err("Party not found".into());
    };
    if party.leader_id != leader_id {
        return Err("Only the party leader can reject join requests".into());
    }
    ctx.db.party_join_request().id().delete(request_id);
    Ok(())
}

#[reducer]
pub fn seed_bot_join_requests(
    ctx: &ReducerContext,
    recruitment_role_id: u64,
) -> Result<(), String> {
    use petname::Generator;

    let role = ctx
        .db
        .party_recruitment_role()
        .id()
        .find(recruitment_role_id)
        .ok_or("Recruitment role not found")?;
    let party = ctx
        .db
        .party()
        .id()
        .find(&role.party_id)
        .ok_or("Party not found")?;
    for index in 0..role.quantity.saturating_mul(2).min(16) {
        let name = petname::Petnames::default()
            .generate(&mut ctx.rng(), 2, " ")
            .ok_or("Could not generate bot applicant name")?;
        let mut id = ctx.random::<u64>() | (1_u64 << 63);
        while ctx.db.character().id().find(id).is_some() {
            id = ctx.random::<u64>() | (1_u64 << 63);
        }
        crate::character::insert_new_character(ctx, name, id, true)?;
        let mut bot = ctx.db.character().id().find(id).unwrap();
        bot.current_settlement_id = party.current_settlement_id.clone();
        bot.current_quest_location_id = party.current_quest_location_id.clone();
        ctx.db.character().id().update(bot);
        let mut attrs = ctx
            .db
            .character_attributes()
            .character_id()
            .find(id)
            .unwrap();
        attrs.endurance = 10.0;
        attrs.left_arm_strength = 10.0;
        attrs.right_arm_strength = 10.0;
        attrs.left_arm_agility = 10.0;
        attrs.right_arm_agility = 10.0;
        attrs.left_leg_agility = 10.0;
        attrs.right_leg_agility = 10.0;
        attrs.intelligence = 10.0;
        attrs.instinct = 10.0;
        let mut skills = ctx.db.character_skills().character_id().find(id).unwrap();
        skills.medicine_hours = 1_000_000.0;
        skills.surgeon_hours = 1_000_000.0;
        skills.charisma_hours = 1_000_000.0;
        skills.faith_hours = 1_000_000.0;
        crate::character::add_and_equip_item(
            ctx,
            id,
            "bot_multirole_weapon",
            crate::ItemSlot::RightHolding,
        )?;
        if role.requirements.full_armor {
            for (item, slot) in [
                ("bot_plate_arm", crate::ItemSlot::LeftArm),
                ("bot_plate_arm", crate::ItemSlot::RightArm),
                ("bot_plate_leg", crate::ItemSlot::LeftLeg),
                ("bot_plate_leg", crate::ItemSlot::RightLeg),
                ("bot_plate_chest", crate::ItemSlot::Chest),
                ("bot_plate_stomach", crate::ItemSlot::Stomach),
                ("bot_plate_helmet", crate::ItemSlot::Head),
            ] {
                crate::character::add_and_equip_item(ctx, id, item, slot)?;
            }
        }

        if index % 2 == 1 {
            let requirements = role.requirements;
            if requirements.endurance > 0 {
                attrs.endurance = 0.0;
            } else if requirements.athletics > 0 || requirements.heavy {
                attrs.left_arm_strength = 0.0;
                attrs.right_arm_strength = 0.0;
                if requirements.athletics > 0 {
                    attrs.left_arm_agility = 0.0;
                    attrs.right_arm_agility = 0.0;
                    attrs.left_leg_agility = 0.0;
                    attrs.right_leg_agility = 0.0;
                    attrs.endurance = 0.0;
                }
            } else if requirements.medicine > 0 {
                skills.medicine_hours = 0.0;
            } else if requirements.surgery > 0 {
                skills.surgeon_hours = 0.0;
            } else if requirements.charisma > 0 {
                skills.charisma_hours = 0.0;
                attrs.intelligence = 0.0;
                attrs.instinct = 0.0;
            } else if requirements.faith > 0 {
                skills.faith_hours = 0.0;
            } else if requirements.quarter_armor
                || requirements.half_armor
                || requirements.three_quarter_armor
                || requirements.full_armor
            {
                let mut equip = ctx.db.character_equip().character_id().find(id).unwrap();
                equip.left_arm_armor_id = None;
                equip.right_arm_armor_id = None;
                equip.left_leg_armor_id = None;
                equip.right_leg_armor_id = None;
                equip.chest_armor_id = None;
                equip.stomach_armor_id = None;
                equip.head_armor_id = None;
                ctx.db.character_equip().character_id().update(equip);
            } else if requirements.melee {
                crate::character::add_and_equip_item(
                    ctx,
                    id,
                    "short_bow",
                    crate::ItemSlot::RightHolding,
                )?;
            } else if requirements.ranged {
                crate::character::add_and_equip_item(
                    ctx,
                    id,
                    "club",
                    crate::ItemSlot::RightHolding,
                )?;
            } else if role.weapon_precision > 0.0 {
                let mut equip = ctx.db.character_equip().character_id().find(id).unwrap();
                equip.left_hand_item_id = None;
                equip.right_hand_item_id = None;
                ctx.db.character_equip().character_id().update(equip);
            }
        }
        ctx.db.character_attributes().character_id().update(attrs);
        ctx.db.character_skills().character_id().update(skills);
        request_to_join_party(ctx, id, recruitment_role_id)?;
    }
    Ok(())
}

/// Transfer a stack of items between two members of the same party.
#[reducer]
pub fn transfer_party_item(
    ctx: &ReducerContext,
    from_character_id: u64,
    to_character_id: u64,
    inventory_item_id: u64,
    quantity: u32,
) -> Result<(), String> {
    if quantity == 0 || from_character_id == to_character_id {
        return Err("Transfer quantity must be positive and between different characters".into());
    }
    let Some(from) = ctx.db.character().id().find(from_character_id) else {
        return Err("Source character not found".into());
    };
    let Some(to) = ctx.db.character().id().find(to_character_id) else {
        return Err("Recipient character not found".into());
    };
    if from.party_id.is_none() || from.party_id != to.party_id {
        return Err("Characters must belong to the same party".into());
    }
    let Some(source_item) = ctx.db.inventory_item().id().find(inventory_item_id) else {
        return Err("Inventory item not found".into());
    };
    if source_item.character_id != from_character_id || source_item.quantity < quantity {
        return Err("Source character does not have that quantity".into());
    }
    if ctx
        .db
        .character_equip()
        .character_id()
        .find(from_character_id)
        .is_some_and(|equip| equip.is_equiped(inventory_item_id).is_some())
    {
        return Err("Unequip an item before transferring it".into());
    }

    if source_item.quantity == quantity {
        ctx.db.inventory_item().id().delete(inventory_item_id);
    } else {
        let mut updated = source_item.clone();
        updated.quantity -= quantity;
        ctx.db.inventory_item().id().update(updated);
    }
    if let Some(mut destination_item) = ctx
        .db
        .inventory_item()
        .character_and_item_id()
        .filter((to_character_id, &source_item.item_id))
        .next()
    {
        destination_item.quantity = destination_item.quantity.saturating_add(quantity);
        ctx.db.inventory_item().id().update(destination_item);
    } else {
        ctx.db.inventory_item().insert(InventoryItem {
            id: 0,
            character_id: to_character_id,
            item_id: source_item.item_id,
            quantity,
        });
    }
    Ok(())
}

#[reducer]
pub fn finalize_party_offer(
    ctx: &ReducerContext,
    from_character_ids: Vec<u64>,
    to_character_ids: Vec<u64>,
    inventory_item_ids: Vec<u64>,
    quantities: Vec<u32>,
) -> Result<(), String> {
    if from_character_ids.len() != to_character_ids.len()
        || from_character_ids.len() != inventory_item_ids.len()
        || from_character_ids.len() != quantities.len()
        || from_character_ids.is_empty()
    {
        return Err("Offer entries must be non-empty and aligned".into());
    }
    for index in 0..from_character_ids.len() {
        let from_id = from_character_ids[index];
        let to_id = to_character_ids[index];
        let quantity = quantities[index];
        let Some(from) = ctx.db.character().id().find(from_id) else {
            return Err("Source character not found".into());
        };
        let Some(to) = ctx.db.character().id().find(to_id) else {
            return Err("Recipient character not found".into());
        };
        let Some(item) = ctx.db.inventory_item().id().find(inventory_item_ids[index]) else {
            return Err("Inventory item not found".into());
        };
        if quantity == 0
            || from_id == to_id
            || from.party_id.is_none()
            || from.party_id != to.party_id
            || item.character_id != from_id
            || item.quantity < quantity
        {
            return Err("Invalid party trade offer".into());
        }
        if ctx
            .db
            .character_equip()
            .character_id()
            .find(from_id)
            .is_some_and(|equip| equip.is_equiped(item.id).is_some())
        {
            return Err("Unequip an item before offering it".into());
        }
    }
    for index in 0..from_character_ids.len() {
        transfer_party_item(
            ctx,
            from_character_ids[index],
            to_character_ids[index],
            inventory_item_ids[index],
            quantities[index],
        )?;
    }
    Ok(())
}

#[reducer]
pub fn finalize_merchant_trade(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
    buy_item_ids: Vec<String>,
    buy_quantities: Vec<u32>,
    sell_inventory_ids: Vec<u64>,
    sell_quantities: Vec<u32>,
) -> Result<(), String> {
    if buy_item_ids.len() != buy_quantities.len()
        || sell_inventory_ids.len() != sell_quantities.len()
    {
        return Err("Trade entries must be aligned".into());
    }
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    if character.current_settlement_id.as_deref() != Some(&settlement_id) {
        return Err("Character must be at this settlement to trade".into());
    }
    let mut net: HashMap<String, i64> = HashMap::new();
    for (item_id, quantity) in buy_item_ids.iter().zip(&buy_quantities) {
        *net.entry(item_id.clone()).or_default() += *quantity as i64;
    }
    for (inventory_id, quantity) in sell_inventory_ids.iter().zip(&sell_quantities) {
        let Some(inventory) = ctx.db.inventory_item().id().find(*inventory_id) else {
            return Err("Inventory item not found".into());
        };
        *net.entry(inventory.item_id).or_default() -= *quantity as i64;
    }
    let buy_item_ids: Vec<String> = net
        .iter()
        .filter(|(_, value)| **value > 0)
        .map(|(item, _)| item.clone())
        .collect();
    let buy_quantities: Vec<u32> = buy_item_ids.iter().map(|item| net[item] as u32).collect();
    let sell_inventory_ids: Vec<u64> = sell_inventory_ids
        .into_iter()
        .filter(|id| {
            ctx.db
                .inventory_item()
                .id()
                .find(*id)
                .is_some_and(|entry| net.get(&entry.item_id).copied().unwrap_or(0) < 0)
        })
        .collect();
    let sell_quantities: Vec<u32> = sell_inventory_ids
        .iter()
        .map(|id| {
            let entry = ctx.db.inventory_item().id().find(*id).unwrap();
            (-net[&entry.item_id]) as u32
        })
        .collect();
    let mut cost = 0_u32;
    for (item_id, quantity) in buy_item_ids.iter().zip(&buy_quantities) {
        let Some(item) = ctx.db.item().id().find(item_id) else {
            return Err("Merchant item not found".into());
        };
        if item.kind == crate::ItemKind::Currency || *quantity == 0 {
            return Err("Invalid merchant purchase".into());
        }
        cost = cost.saturating_add(
            (item.base_value.unwrap_or(1) as f32 * MERCHANT_MARGIN * (1.0 + SALES_TAX)).ceil()
                as u32
                * quantity,
        );
    }
    let mut proceeds = 0_u32;
    for (inventory_id, quantity) in sell_inventory_ids.iter().zip(&sell_quantities) {
        let Some(inventory) = ctx.db.inventory_item().id().find(*inventory_id) else {
            return Err("Inventory item not found".into());
        };
        let Some(item) = ctx.db.item().id().find(&inventory.item_id) else {
            return Err("Item definition not found".into());
        };
        if inventory.character_id != character_id
            || inventory.quantity < *quantity
            || *quantity == 0
            || item.kind == crate::ItemKind::Currency
        {
            return Err("Invalid merchant sale".into());
        }
        if ctx
            .db
            .character_equip()
            .character_id()
            .find(character_id)
            .is_some_and(|equip| equip.is_equiped(*inventory_id).is_some())
        {
            return Err("Unequip an item before selling it".into());
        }
        proceeds = proceeds.saturating_add(
            (item.base_value.unwrap_or(1) as f32 / MERCHANT_MARGIN)
                .floor()
                .max(1.0) as u32
                * quantity,
        );
    }
    let coins: u32 = ctx
        .db
        .inventory_item()
        .character_and_item_id()
        .filter((character_id, &"gold_coin".to_string()))
        .map(|coin| coin.quantity)
        .sum();
    if coins.saturating_add(proceeds) < cost {
        return Err("Not enough gold".into());
    }
    for (inventory_id, quantity) in sell_inventory_ids.iter().zip(&sell_quantities) {
        let inventory = ctx.db.inventory_item().id().find(*inventory_id).unwrap();
        if inventory.quantity == *quantity {
            ctx.db.inventory_item().id().delete(*inventory_id);
        } else {
            let mut updated = inventory;
            updated.quantity -= quantity;
            ctx.db.inventory_item().id().update(updated);
        }
    }
    let equip = ctx.db.character_equip().character_id().find(character_id);
    for (item_id, quantity) in buy_item_ids.iter().zip(&buy_quantities) {
        // Never add purchases to an equipped stack. An equipped item must stay
        // independently sellable from an otherwise identical spare item.
        if let Some(mut stack) = ctx
            .db
            .inventory_item()
            .character_and_item_id()
            .filter((character_id, item_id))
            .find(|stack| {
                !equip
                    .as_ref()
                    .is_some_and(|equip| equip.is_equiped(stack.id).is_some())
            })
        {
            stack.quantity = stack.quantity.saturating_add(*quantity);
            ctx.db.inventory_item().id().update(stack);
        } else {
            ctx.db.inventory_item().insert(InventoryItem {
                id: 0,
                character_id,
                item_id: item_id.clone(),
                quantity: *quantity,
            });
        }
    }
    let net = proceeds as i64 - cost as i64;
    if net != 0 {
        if let Some(mut coin) = ctx
            .db
            .inventory_item()
            .character_and_item_id()
            .filter((character_id, &"gold_coin".to_string()))
            .next()
        {
            coin.quantity = (coin.quantity as i64 + net) as u32;
            ctx.db.inventory_item().id().update(coin);
        } else if net > 0 {
            ctx.db.inventory_item().insert(InventoryItem {
                id: 0,
                character_id,
                item_id: "gold_coin".into(),
                quantity: net as u32,
            });
        }
    }
    Ok(())
}

/// Adds two deterministic companions to the specified character's party for
/// strategic UI development. Safe to call repeatedly.
#[reducer]
pub fn seed_party_companions(ctx: &ReducerContext, leader_id: u64) -> Result<(), String> {
    let leader = ctx
        .db
        .character()
        .id()
        .find(leader_id)
        .ok_or("Leader not found")?;
    let party_id = leader.party_id.clone().ok_or("Leader has no party")?;
    let role = ctx
        .db
        .party_recruitment_role()
        .insert(PartyRecruitmentRole {
            id: 0,
            party_id: party_id.clone(),
            name: "Companion".into(),
            requirements: RecruitmentRequirements::default(),
            quantity: 2,
            weapon_precision: 0.0,
        });

    for (id, name) in [(9_000_001_u64, "Mara"), (9_000_002_u64, "Orrin")] {
        if ctx.db.character().id().find(id).is_none() {
            crate::character::insert_new_character(ctx, name.into(), id, false)?;
        }
        let mut companion = ctx.db.character().id().find(id).unwrap();
        if companion.party_id.as_deref() == Some(&party_id) {
            continue;
        }
        companion.current_settlement_id = leader.current_settlement_id.clone();
        companion.current_quest_location_id = leader.current_quest_location_id.clone();
        ctx.db.character().id().update(companion);
        request_to_join_party(ctx, id, role.id)?;
        let request = ctx
            .db
            .party_join_request()
            .character_id()
            .filter(id)
            .find(|request| request.recruitment_role_id == role.id)
            .unwrap();
        accept_party_join_request(ctx, leader_id, request.id)?;
    }
    Ok(())
}

#[reducer]
pub fn leave_party(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    let Some(mut character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };

    let Some(party_id) = character.party_id.clone() else {
        return Err("Character is not in a party".into());
    };

    let Some(party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };

    if party.leader_id == character_id {
        return Err("Party leader cannot leave. Use disband_party instead.".into());
    }

    if let Some(membership) = ctx
        .db
        .party_member()
        .character_id()
        .filter(character_id)
        .find(|m| m.party_id == party_id)
    {
        ctx.db.party_member().id().delete(membership.id);
    }

    character.party_id = None;
    ctx.db.character().id().update(character);
    create_solo_party_for_character(ctx, character_id)?;
    Ok(())
}

#[reducer]
pub fn disband_party(ctx: &ReducerContext, party_id: String) -> Result<(), String> {
    let Some(party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };
    if party.current_quest_location_id.is_some() {
        return Err("Travel to a settlement before disbanding the party".into());
    }

    let members: Vec<_> = ctx.db.party_member().party_id().filter(&party_id).collect();
    let member_ids: Vec<_> = members.iter().map(|member| member.character_id).collect();
    for member in members {
        if let Some(mut character) = ctx.db.character().id().find(member.character_id) {
            character.party_id = None;
            ctx.db.character().id().update(character);
        }
        ctx.db.party_member().id().delete(member.id);
    }

    let requests: Vec<_> = ctx
        .db
        .party_join_request()
        .party_id()
        .filter(&party_id)
        .collect();
    for request in requests {
        ctx.db.party_join_request().id().delete(request.id);
    }
    for role in ctx
        .db
        .party_recruitment_role()
        .party_id()
        .filter(&party_id)
        .collect::<Vec<_>>()
    {
        ctx.db.party_recruitment_role().id().delete(role.id);
    }

    if let Some(quest_id) = party.active_quest_id {
        ctx.db.quest().id().delete(&quest_id);
    }

    ctx.db.party().id().delete(&party_id);
    for character_id in member_ids {
        create_solo_party_for_character(ctx, character_id)?;
    }
    Ok(())
}

#[reducer]
pub fn accept_quest(
    ctx: &ReducerContext,
    character_id: u64,
    quest_id: String,
) -> Result<(), String> {
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };

    let Some(party_id) = character.party_id.clone() else {
        return Err("Must be in a party to accept quests".into());
    };

    let Some(mut party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };

    if party.leader_id != character_id {
        return Err("Only the party leader can accept quests".into());
    }

    if party.active_quest_id.is_some() {
        return Err("Party already has an active quest".into());
    }

    let Some(mut quest) = ctx.db.quest().id().find(&quest_id) else {
        return Err("Quest not found".into());
    };

    if quest.status != QuestStatus::Available {
        return Err("Quest is not available".into());
    }

    if character.current_settlement_id.as_ref() != Some(&quest.settlement_id) {
        return Err("Must be at the quest's settlement to accept it".into());
    }

    quest.status = QuestStatus::Accepted;
    quest.accepted_by = Some(party_id.clone());
    let settlement_id = quest.settlement_id.clone();
    ctx.db.quest().id().update(quest);

    party.active_quest_id = Some(quest_id);
    ctx.db.party().id().update(party);
    generate_quest_for_settlement(ctx, &settlement_id)?;
    Ok(())
}

#[reducer]
pub fn abandon_quest(
    ctx: &ReducerContext,
    character_id: u64,
    quest_id: String,
) -> Result<(), String> {
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };

    let Some(party_id) = character.party_id.clone() else {
        return Err("Not in a party".into());
    };

    let Some(mut party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };

    if party.leader_id != character_id {
        return Err("Only the party leader can abandon quests".into());
    }
    if character.current_quest_location_id.is_some() {
        return Err("Travel to a settlement before abandoning the quest".into());
    }

    let Some(quest) = ctx.db.quest().id().find(&quest_id) else {
        return Err("Quest not found".into());
    };

    if quest.accepted_by.as_ref() != Some(&party_id) {
        return Err("This quest is not accepted by your party".into());
    }

    ctx.db.quest().id().delete(&quest.id);

    party.active_quest_id = None;
    ctx.db.party().id().update(party);
    Ok(())
}

fn travel_neighbors(ctx: &ReducerContext, node: u64) -> Vec<(u64, u32)> {
    let mut neighbors: Vec<_> = ctx
        .db
        .travel_edge()
        .from_node_id()
        .filter(&node)
        .filter_map(|edge| {
            matches!(edge.kind.as_str(), "land" | "ferry")
                .then_some((edge.to_node_id, edge.length_m))
        })
        .collect();
    neighbors.extend(
        ctx.db
            .travel_edge()
            .to_node_id()
            .filter(&node)
            .filter_map(|edge| {
                matches!(edge.kind.as_str(), "land" | "ferry")
                    .then_some((edge.from_node_id, edge.length_m))
            }),
    );
    neighbors
}

/// Returns the next settlements reached from a source. Paths end at the first
/// settlement encountered, so journeys cannot skip intermediate settlements.
fn connected_settlement_distances(ctx: &ReducerContext, source_node_id: u64) -> HashMap<u64, u64> {
    let settlement_nodes: HashSet<u64> = ctx
        .db
        .settlement()
        .iter()
        .filter_map(|settlement| settlement.source_node_id)
        .collect();
    let mut distances = HashMap::from([(source_node_id, 0_u64)]);
    let mut pending = BinaryHeap::from([std::cmp::Reverse((0_u64, source_node_id))]);
    let mut destinations = HashMap::new();

    while let Some(std::cmp::Reverse((distance, node))) = pending.pop() {
        if distances.get(&node).is_some_and(|known| *known != distance) {
            continue;
        }
        if node != source_node_id && settlement_nodes.contains(&node) {
            destinations.insert(node, distance);
            continue;
        }
        for (neighbor, length_m) in travel_neighbors(ctx, node) {
            let next_distance = distance.saturating_add(u64::from(length_m));
            if distances
                .get(&neighbor)
                .is_none_or(|known| next_distance < *known)
            {
                distances.insert(neighbor, next_distance);
                pending.push(std::cmp::Reverse((next_distance, neighbor)));
            }
        }
    }
    destinations
}

fn journey_minutes(distance_m: u64) -> u64 {
    distance_m
        .saturating_mul(MINUTES_PER_HOUR)
        .div_ceil(WALKING_SPEED_KM_PER_HOUR * METERS_PER_KILOMETER)
        .max(1)
}

fn quest_journey_minutes(distance_m: u64) -> u64 {
    journey_minutes(distance_m).saturating_mul(QUEST_TRAVEL_SPEED_DIVISOR)
}

fn straight_line_distance_m(
    from_x: f64,
    from_y: f64,
    to_x: f64,
    to_y: f64,
    geographic: bool,
) -> u64 {
    if geographic {
        let earth_radius_m = 6_371_000.0_f64;
        let lat1 = from_y.to_radians();
        let lat2 = to_y.to_radians();
        let delta_lat = (to_y - from_y).to_radians();
        let delta_lon = (to_x - from_x).to_radians();
        let a = (delta_lat / 2.0).sin().powi(2)
            + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2);
        (earth_radius_m * 2.0 * a.sqrt().atan2((1.0 - a).sqrt())).round() as u64
    } else {
        (((from_x - to_x).powi(2) + (from_y - to_y).powi(2)).sqrt() * METERS_PER_KILOMETER as f64)
            .round() as u64
    }
}

#[reducer]
pub fn travel_to_quest(
    ctx: &ReducerContext,
    character_id: u64,
    quest_id: String,
) -> Result<(), String> {
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    let Some(party_id) = character.party_id.clone() else {
        return Err("Must be in a party to travel to a quest".into());
    };
    let Some(mut party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };
    if party.leader_id != character_id {
        return Err("Only the party leader can travel".into());
    }
    if party.active_quest_id.as_deref() != Some(&quest_id) {
        return Err("This is not the party's active quest".into());
    }
    let Some(quest) = ctx.db.quest().id().find(&quest_id) else {
        return Err("Quest not found".into());
    };
    if quest.status != QuestStatus::Accepted || quest.accepted_by.as_ref() != Some(&party_id) {
        return Err("Quest is not accepted by this party".into());
    }
    if character.current_settlement_id.as_ref() != Some(&quest.settlement_id) {
        return Err("Travel to the quest must begin at its posting settlement".into());
    }

    let travel_minutes = quest_journey_minutes(quest.distance_m);
    for membership in ctx.db.party_member().party_id().filter(&party_id) {
        if let Some(mut member) = ctx.db.character().id().find(membership.character_id) {
            advance_character_time(ctx, member.id, travel_minutes)?;
            member.current_settlement_id = None;
            member.current_quest_location_id = Some(quest_id.clone());
            ctx.db.character().id().update(member);
        }
    }
    party.current_settlement_id = None;
    party.current_quest_location_id = Some(quest_id);
    ctx.db.party().id().update(party);
    Ok(())
}

#[reducer]
pub fn travel_to_settlement(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
) -> Result<(), String> {
    let Some(destination) = ctx.db.settlement().id().find(&settlement_id) else {
        return Err("Settlement not found".into());
    };

    let Some(mut character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };

    if let Some(origin_id) = &character.current_settlement_id {
        let Some(origin) = ctx.db.settlement().id().find(origin_id) else {
            return Err("Character's current settlement does not exist".into());
        };
        // Demo settlements remain usable before a Viabundus world is loaded.
        // Imported journeys must lead to the next settlement on the road graph.
        if let (Some(origin_node), Some(destination_node)) =
            (origin.source_node_id, destination.source_node_id)
        {
            let Some(distance_m) = connected_settlement_distances(ctx, origin_node)
                .get(&destination_node)
                .copied()
            else {
                return Err("That settlement is not directly connected by land or ferry".into());
            };
            advance_character_time(ctx, character_id, journey_minutes(distance_m))?;
        } else {
            let distance_km = ((origin.coord_x - destination.coord_x).powi(2)
                + (origin.coord_y - destination.coord_y).powi(2))
            .sqrt()
            .ceil() as u64;
            advance_character_time(
                ctx,
                character_id,
                journey_minutes(distance_km.saturating_mul(METERS_PER_KILOMETER)),
            )?;
        }
    } else if let Some(quest_id) = &character.current_quest_location_id {
        let Some(quest) = ctx.db.quest().id().find(quest_id) else {
            return Err("Character's current quest location does not exist".into());
        };
        let distance_m = straight_line_distance_m(
            quest.location_coord_x,
            quest.location_coord_y,
            destination.coord_x,
            destination.coord_y,
            quest.coordinates_are_geographic && destination.source_node_id.is_some(),
        );
        advance_character_time(ctx, character_id, quest_journey_minutes(distance_m))?;
    } else {
        return Err("Character is not at a known location".into());
    }

    let departing_quest = character.current_quest_location_id.clone();
    character.current_settlement_id = Some(settlement_id.clone());
    character.current_quest_location_id = None;
    let party_id = character.party_id.clone();
    ctx.db.character().id().update(character);

    if let Some(party_id) = party_id {
        if let Some(mut party) = ctx.db.party().id().find(&party_id) {
            if party.leader_id == character_id {
                if departing_quest.is_some() {
                    let members: Vec<_> =
                        ctx.db.party_member().party_id().filter(&party_id).collect();
                    for membership in members {
                        if membership.character_id == character_id {
                            continue;
                        }
                        if let Some(mut member) =
                            ctx.db.character().id().find(membership.character_id)
                        {
                            let quest = ctx
                                .db
                                .quest()
                                .id()
                                .find(departing_quest.as_ref().unwrap())
                                .ok_or("Party's quest location does not exist")?;
                            let distance_m = straight_line_distance_m(
                                quest.location_coord_x,
                                quest.location_coord_y,
                                destination.coord_x,
                                destination.coord_y,
                                quest.coordinates_are_geographic
                                    && destination.source_node_id.is_some(),
                            );
                            advance_character_time(
                                ctx,
                                member.id,
                                quest_journey_minutes(distance_m),
                            )?;
                            member.current_settlement_id = Some(settlement_id.clone());
                            member.current_quest_location_id = None;
                            ctx.db.character().id().update(member);
                        }
                    }
                }
                party.current_settlement_id = Some(settlement_id);
                party.current_quest_location_id = None;
                ctx.db.party().id().update(party);
            }
        }
    }

    Ok(())
}

#[reducer]
pub fn complete_quest(ctx: &ReducerContext, quest_id: String) -> Result<(), String> {
    let Some(mut quest) = ctx.db.quest().id().find(&quest_id) else {
        return Err("Quest not found".into());
    };

    if quest.status != QuestStatus::Accepted {
        return Err("Quest is not in accepted state".into());
    }

    let Some(party_id) = quest.accepted_by.clone() else {
        return Err("Quest has no party assigned".into());
    };

    let Some(mut party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };

    let members: Vec<_> = ctx.db.party_member().party_id().filter(&party_id).collect();
    let gold_per_member = quest.gold_reward.max(0) as u32 / members.len().max(1) as u32;
    let xp_per_member = quest.xp_reward.max(0) as u32 / members.len().max(1) as u32;

    for member in members {
        if let Some(mut character) = ctx.db.character().id().find(member.character_id) {
            character.gold += gold_per_member;
            character.xp += xp_per_member;
            character.level = 1 + character.xp / 100;
            ctx.db.character().id().update(character);
        }
    }

    quest.status = QuestStatus::Completed;
    ctx.db.quest().id().update(quest);

    party.active_quest_id = None;
    ctx.db.party().id().update(party);
    Ok(())
}

#[reducer]
pub fn autoresolve_quest(
    ctx: &ReducerContext,
    character_id: u64,
    quest_id: String,
) -> Result<(), String> {
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };
    let Some(party_id) = character.party_id else {
        return Err("Must be in a party".into());
    };
    let Some(party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };
    if party.leader_id != character_id {
        return Err("Only the party leader can autoresolve".into());
    }
    if party.active_quest_id.as_deref() != Some(&quest_id)
        || party.current_quest_location_id.as_deref() != Some(&quest_id)
    {
        return Err("Party must be at its active quest location".into());
    }

    for member in ctx.db.party_member().party_id().filter(&party_id) {
        if let Some(mut limbs) = ctx
            .db
            .character_limbs()
            .character_id()
            .find(member.character_id)
        {
            let damage = 0.05 + (ctx.random::<u64>() % 16) as f32 / 100.0;
            match ctx.random::<u64>() % 7 {
                0 => limbs.left_arm_health = (limbs.left_arm_health - damage).max(0.0),
                1 => limbs.right_arm_health = (limbs.right_arm_health - damage).max(0.0),
                2 => limbs.left_leg_health = (limbs.left_leg_health - damage).max(0.0),
                3 => limbs.right_leg_health = (limbs.right_leg_health - damage).max(0.0),
                4 => limbs.head_health = (limbs.head_health - damage).max(0.0),
                5 => limbs.chest_health = (limbs.chest_health - damage).max(0.0),
                _ => limbs.stomach_health = (limbs.stomach_health - damage).max(0.0),
            }
            ctx.db.character_limbs().character_id().update(limbs);
        }
    }
    complete_quest(ctx, quest_id)
}

#[reducer]
pub fn cancel_mission_request(ctx: &ReducerContext, mission_id: String) -> Result<(), String> {
    ctx.db
        .tactical_server_request()
        .mission_id()
        .delete(&mission_id);
    Ok(())
}

#[reducer]
pub fn seed_world(ctx: &ReducerContext) -> Result<(), String> {
    let settlements = [
        ("riverdale", "Riverdale", 0.0, 0.0, 3, "hills"),
        ("ironforge", "Ironforge", 100.0, 50.0, 4, "desert"),
        ("willowmere", "Willowmere", -50.0, 75.0, 2, "hills"),
    ];

    for (id, name, x, y, pop, scene) in settlements {
        if ctx.db.settlement().id().find(&id.to_string()).is_none() {
            ctx.db.settlement().insert(Settlement {
                id: id.into(),
                name: name.into(),
                coord_x: x,
                coord_y: y,
                population_level: pop,
                population_estimate: 0,
                scene_key: scene.into(),
                source_node_id: None,
            });
        }
    }

    let settlement_ids: Vec<_> = ctx
        .db
        .settlement()
        .iter()
        .map(|settlement| settlement.id)
        .collect();
    for settlement_id in settlement_ids {
        ensure_settlement_quests(ctx, &settlement_id)?;
    }

    Ok(())
}

fn ensure_settlement_quests(ctx: &ReducerContext, settlement_id: &str) -> Result<(), String> {
    let available = ctx
        .db
        .quest()
        .settlement_id()
        .filter(&settlement_id.to_string())
        .filter(|quest| quest.status == QuestStatus::Available)
        .count();
    for _ in available..QUESTS_PER_SETTLEMENT {
        generate_quest_for_settlement(ctx, settlement_id)?;
    }
    Ok(())
}

fn generate_quest_for_settlement(ctx: &ReducerContext, settlement_id: &str) -> Result<(), String> {
    let Some(settlement) = ctx.db.settlement().id().find(&settlement_id.to_string()) else {
        return Err("Settlement not found".into());
    };
    let archetypes = [
        (
            "Clear the Goblin Cave",
            "A band of goblins has taken up residence nearby.",
            "goblins",
            "cave",
            "You arrive at a cave.",
            2,
        ),
        (
            "Break Up the Bandit Camp",
            "Bandits have been raiding merchant caravans.",
            "bandits",
            "camp",
            "You arrive at a rough camp.",
            3,
        ),
        (
            "Hunt the Wolf Pack",
            "Wolves have been attacking livestock outside the walls.",
            "wolves",
            "woods",
            "You arrive at a wooded hollow.",
            1,
        ),
        (
            "Purge the Old Mine",
            "Giant spiders have infested an abandoned mine.",
            "spiders",
            "mine",
            "You arrive at an old mine.",
            3,
        ),
        (
            "Recover the Stolen Ore",
            "Thieves are hiding with a stolen ore shipment.",
            "thieves",
            "camp",
            "You arrive at a hidden camp.",
            2,
        ),
        (
            "Quiet the Restless Dead",
            "Travelers report dead men walking near a ruined chapel.",
            "skeletons",
            "ruins",
            "You arrive at ruined chapel.",
            4,
        ),
    ];
    let occupied: HashSet<String> = ctx
        .db
        .quest()
        .settlement_id()
        .filter(&settlement.id)
        .filter(|quest| quest.status != QuestStatus::Completed)
        .map(|quest| quest.title)
        .collect();
    let start = ctx.random::<u64>() as usize % archetypes.len();
    let Some((title, description, enemy, scene, arrival, difficulty)) = (0..archetypes.len())
        .map(|offset| archetypes[(start + offset) % archetypes.len()])
        .find(|archetype| !occupied.contains(&format!("{} near {}", archetype.0, settlement.name)))
    else {
        return Err("No distinct quest archetype is available".into());
    };
    let distance_m = 4_000 + ctx.random::<u64>() % 17_000;
    let angle = (ctx.random::<u64>() as f64 / u64::MAX as f64) * std::f64::consts::TAU;
    let geographic = settlement.source_node_id.is_some();
    let (offset_x, offset_y) = if geographic {
        let distance_km = distance_m as f64 / 1_000.0;
        let latitude_scale = 111.0;
        let longitude_scale = latitude_scale * settlement.coord_y.to_radians().cos().abs().max(0.1);
        (
            angle.cos() * distance_km / longitude_scale,
            angle.sin() * distance_km / latitude_scale,
        )
    } else {
        let distance_km = distance_m as f64 / 1_000.0;
        (angle.cos() * distance_km, angle.sin() * distance_km)
    };
    let enemy_count = difficulty * 2 + (ctx.random::<u64>() % 4) as i32;
    let nonce = ctx.random::<u64>();
    ctx.db.quest().insert(Quest {
        id: format!("{}-{nonce:016x}", settlement.id),
        title: format!("{title} near {}", settlement.name),
        description: description.into(),
        difficulty,
        gold_reward: difficulty * 35 + distance_m.div_ceil(1_000) as i32 * 2,
        xp_reward: difficulty * 20,
        settlement_id: settlement.id,
        status: QuestStatus::Available,
        accepted_by: None,
        enemy_type: enemy.into(),
        enemy_count,
        location_description: arrival.into(),
        location_scene_key: scene.into(),
        location_coord_x: settlement.coord_x + offset_x,
        location_coord_y: settlement.coord_y + offset_y,
        coordinates_are_geographic: geographic,
        distance_m,
    });
    Ok(())
}
