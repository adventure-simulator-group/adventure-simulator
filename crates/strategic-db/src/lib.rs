//! Strategic Layer SpacetimeDB Module
//!
//! Simple Mount & Blade style architecture:
//! - Strategic layer: character, inventory, missions, settlements, quests, parties (SpacetimeDB)
//! - Tactical layer: real-time gameplay (Lightyear server process)
//!
//! Flow:
//! 1. Player clicks "Enter Town" → creates mission with status "pending"
//! 2. Spawner sees pending mission → starts tactical-server process
//! 3. Tactical-server writes connection info → status becomes "ready"
//! 4. Browser client sees "ready" → connects via WebTransport/WebSocket
//! 5. Mission ends → tactical-server commits results, exits

use spacetimedb::{reducer, table, ReducerContext, SpacetimeType, Table};

// ============================================================================
// Types
// ============================================================================

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TacticalStatus {
    Pending,
    Ready,
    Ended,
}

#[derive(SpacetimeType, Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestStatus {
    Available,
    Accepted,
    Completed,
}

// ============================================================================
// Tables
// ============================================================================

/// Character (strategic state only - no HP, that's tactical)
#[derive(Clone, Debug)]
#[table(name = character, public)]
pub struct Character {
    #[primary_key]
    pub id: String,
    pub name: String,
    pub xp: i32,
    pub level: i32,
    pub gold: i32,
    pub current_settlement_id: Option<String>,
    pub party_id: Option<String>,
}

/// Settlement - a location on the strategic map
#[derive(Clone, Debug)]
#[table(name = settlement, public)]
pub struct Settlement {
    #[primary_key]
    pub id: String,
    pub name: String,
    pub coord_x: f64,
    pub coord_y: f64,
    pub population_level: i32, // 1-5 scale (affects services available)
    pub scene_key: String,     // for tactical layer
}

/// Quest - bounty-style missions available at settlements
#[derive(Clone, Debug)]
#[table(name = quest, public)]
pub struct Quest {
    #[primary_key]
    pub id: String,
    pub title: String,
    pub description: String,
    pub difficulty: i32, // 1-5 (affects reward calculation)
    pub gold_reward: i32,
    pub xp_reward: i32,
    #[index(btree)]
    pub settlement_id: String,
    pub status: QuestStatus,
    pub accepted_by: Option<String>, // party_id
    pub enemy_type: String,          // e.g., "goblins", "bandits"
    pub enemy_count: i32,
}

/// Party - a group of characters traveling together
#[derive(Clone, Debug)]
#[table(name = party, public)]
pub struct Party {
    #[primary_key]
    pub id: String,
    pub name: String,
    pub leader_id: String, // character_id
    pub current_settlement_id: Option<String>,
    pub active_quest_id: Option<String>,
}

/// Party membership - links characters to parties
#[derive(Clone, Debug)]
#[table(name = party_member, public)]
pub struct PartyMember {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub party_id: String,
    #[index(btree)]
    pub character_id: String,
    pub role: Option<String>, // e.g., "ranged specialist", "tank"
}

/// Inventory item
#[derive(Clone, Debug)]
#[table(name = inventory_item, public)]
pub struct InventoryItem {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: String,
    pub item_id: String,
    pub qty: i32,
}

/// Active tactical server instance
/// Written by tactical-server when it starts, read by browser client
#[derive(Clone, Debug)]
#[table(name = tactical_server, public)]
pub struct TacticalServer {
    #[primary_key]
    pub mission_id: String,
    pub scene_key: String,
    pub status: TacticalStatus,
    /// Connection info (written by tactical-server)
    pub addr: String,
    pub cert_digest: String,
    /// Character in this mission
    pub character_id: String,
}

// ============================================================================
// Reducers
// ============================================================================

/// Create or update a character
#[reducer]
pub fn create_character(ctx: &ReducerContext, id: String, name: String) -> Result<(), String> {
    if ctx.db.character().id().find(&id).is_some() {
        return Ok(()); // Already exists
    }

    ctx.db.character().insert(Character {
        id: id.clone(),
        name,
        xp: 0,
        level: 1,
        gold: 100, // Starting gold
        current_settlement_id: None,
        party_id: None,
    });

    // Starter items
    ctx.db.inventory_item().insert(InventoryItem {
        id: 0,
        character_id: id.clone(),
        item_id: "torch".into(),
        qty: 1,
    });
    ctx.db.inventory_item().insert(InventoryItem {
        id: 0,
        character_id: id,
        item_id: "bandage".into(),
        qty: 3,
    });

    Ok(())
}

/// Update a character's name
#[reducer]
pub fn update_character(ctx: &ReducerContext, id: String, name: String) -> Result<(), String> {
    let Some(mut character) = ctx.db.character().id().find(&id) else {
        return Err("Character not found".into());
    };
    character.name = name;
    ctx.db.character().id().update(character);
    Ok(())
}

// ============================================================================
// Settlement Reducers
// ============================================================================

/// Create a new settlement
#[reducer]
pub fn create_settlement(
    ctx: &ReducerContext,
    id: String,
    name: String,
    coord_x: f64,
    coord_y: f64,
    population_level: i32,
    scene_key: String,
) -> Result<(), String> {
    if ctx.db.settlement().id().find(&id).is_some() {
        return Ok(()); // Already exists
    }
    ctx.db.settlement().insert(Settlement {
        id,
        name,
        coord_x,
        coord_y,
        population_level,
        scene_key,
    });
    Ok(())
}

/// Travel a character to a settlement
#[reducer]
pub fn travel_to_settlement(
    ctx: &ReducerContext,
    character_id: String,
    settlement_id: String,
) -> Result<(), String> {
    let Some(mut character) = ctx.db.character().id().find(&character_id) else {
        return Err("Character not found".into());
    };
    if ctx.db.settlement().id().find(&settlement_id).is_none() {
        return Err("Settlement not found".into());
    }

    character.current_settlement_id = Some(settlement_id.clone());
    ctx.db.character().id().update(character);

    // If character is in a party and is the leader, move the party too
    if let Some(party_id) = ctx
        .db
        .character()
        .id()
        .find(&character_id)
        .and_then(|c| c.party_id.clone())
    {
        if let Some(mut party) = ctx.db.party().id().find(&party_id) {
            if party.leader_id == character_id {
                party.current_settlement_id = Some(settlement_id);
                ctx.db.party().id().update(party);
            }
        }
    }

    log::info!("Character {} traveled to settlement", character_id);
    Ok(())
}

// ============================================================================
// Party Reducers
// ============================================================================

/// Create a new party
#[reducer]
pub fn create_party(
    ctx: &ReducerContext,
    id: String,
    name: String,
    leader_id: String,
) -> Result<(), String> {
    let Some(mut leader) = ctx.db.character().id().find(&leader_id) else {
        return Err("Leader character not found".into());
    };

    if leader.party_id.is_some() {
        return Err("Leader is already in a party".into());
    }

    let current_settlement_id = leader.current_settlement_id.clone();

    ctx.db.party().insert(Party {
        id: id.clone(),
        name,
        leader_id: leader_id.clone(),
        current_settlement_id,
        active_quest_id: None,
    });

    // Add leader as party member
    ctx.db.party_member().insert(PartyMember {
        id: 0,
        party_id: id.clone(),
        character_id: leader_id.clone(),
        role: Some("Leader".into()),
    });

    // Update leader's party_id
    leader.party_id = Some(id);
    ctx.db.character().id().update(leader);

    log::info!("Party created with leader {}", leader_id);
    Ok(())
}

/// Join an existing party
#[reducer]
pub fn join_party(ctx: &ReducerContext, character_id: String, party_id: String) -> Result<(), String> {
    let Some(mut character) = ctx.db.character().id().find(&character_id) else {
        return Err("Character not found".into());
    };

    if character.party_id.is_some() {
        return Err("Character is already in a party".into());
    }

    let Some(party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };

    // Must be in same settlement as party
    if character.current_settlement_id != party.current_settlement_id {
        return Err("Must be in the same settlement as the party".into());
    }

    ctx.db.party_member().insert(PartyMember {
        id: 0,
        party_id: party_id.clone(),
        character_id: character_id.clone(),
        role: None,
    });

    character.party_id = Some(party_id);
    ctx.db.character().id().update(character);

    log::info!("Character {} joined party", character_id);
    Ok(())
}

/// Leave current party
#[reducer]
pub fn leave_party(ctx: &ReducerContext, character_id: String) -> Result<(), String> {
    let Some(mut character) = ctx.db.character().id().find(&character_id) else {
        return Err("Character not found".into());
    };

    let Some(party_id) = character.party_id.clone() else {
        return Err("Character is not in a party".into());
    };

    let Some(party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };

    // Leaders can't leave, they must disband
    if party.leader_id == character_id {
        return Err("Party leader cannot leave. Use disband_party instead.".into());
    }

    // Remove party membership
    if let Some(membership) = ctx
        .db
        .party_member()
        .character_id()
        .filter(&character_id)
        .find(|m| m.party_id == party_id)
    {
        ctx.db.party_member().id().delete(&membership.id);
    }

    character.party_id = None;
    ctx.db.character().id().update(character);

    log::info!("Character {} left party", character_id);
    Ok(())
}

/// Disband a party (leader only)
#[reducer]
pub fn disband_party(ctx: &ReducerContext, party_id: String) -> Result<(), String> {
    let Some(party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };

    // Remove all party memberships
    let members: Vec<_> = ctx
        .db
        .party_member()
        .party_id()
        .filter(&party_id)
        .collect();

    for member in members {
        if let Some(mut character) = ctx.db.character().id().find(&member.character_id) {
            character.party_id = None;
            ctx.db.character().id().update(character);
        }
        ctx.db.party_member().id().delete(&member.id);
    }

    // Abandon active quest if any
    if let Some(quest_id) = party.active_quest_id {
        if let Some(mut quest) = ctx.db.quest().id().find(&quest_id) {
            quest.status = QuestStatus::Available;
            quest.accepted_by = None;
            ctx.db.quest().id().update(quest);
        }
    }

    ctx.db.party().id().delete(&party_id);

    log::info!("Party {} disbanded", party_id);
    Ok(())
}

// ============================================================================
// Quest Reducers
// ============================================================================

/// Create a new quest
#[reducer]
pub fn create_quest(
    ctx: &ReducerContext,
    id: String,
    title: String,
    description: String,
    difficulty: i32,
    gold_reward: i32,
    xp_reward: i32,
    settlement_id: String,
    enemy_type: String,
    enemy_count: i32,
) -> Result<(), String> {
    if ctx.db.quest().id().find(&id).is_some() {
        return Ok(()); // Already exists
    }
    if ctx.db.settlement().id().find(&settlement_id).is_none() {
        return Err("Settlement not found".into());
    }
    ctx.db.quest().insert(Quest {
        id,
        title,
        description,
        difficulty,
        gold_reward,
        xp_reward,
        settlement_id,
        status: QuestStatus::Available,
        accepted_by: None,
        enemy_type,
        enemy_count,
    });
    Ok(())
}

/// Accept a quest (character must be in a party)
#[reducer]
pub fn accept_quest(ctx: &ReducerContext, character_id: String, quest_id: String) -> Result<(), String> {
    let Some(character) = ctx.db.character().id().find(&character_id) else {
        return Err("Character not found".into());
    };

    let Some(party_id) = character.party_id.clone() else {
        return Err("Must be in a party to accept quests".into());
    };

    let Some(mut party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };

    // Only leader can accept quests
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

    // Must be at the quest's settlement
    if character.current_settlement_id.as_ref() != Some(&quest.settlement_id) {
        return Err("Must be at the quest's settlement to accept it".into());
    }

    quest.status = QuestStatus::Accepted;
    quest.accepted_by = Some(party_id.clone());
    ctx.db.quest().id().update(quest);

    party.active_quest_id = Some(quest_id.clone());
    ctx.db.party().id().update(party);

    log::info!("Party {} accepted quest {}", party_id, quest_id);
    Ok(())
}

/// Abandon a quest
#[reducer]
pub fn abandon_quest(ctx: &ReducerContext, character_id: String, quest_id: String) -> Result<(), String> {
    let Some(character) = ctx.db.character().id().find(&character_id) else {
        return Err("Character not found".into());
    };

    let Some(party_id) = character.party_id.clone() else {
        return Err("Not in a party".into());
    };

    let Some(mut party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };

    // Only leader can abandon quests
    if party.leader_id != character_id {
        return Err("Only the party leader can abandon quests".into());
    }

    let Some(mut quest) = ctx.db.quest().id().find(&quest_id) else {
        return Err("Quest not found".into());
    };

    if quest.accepted_by.as_ref() != Some(&party_id) {
        return Err("This quest is not accepted by your party".into());
    }

    quest.status = QuestStatus::Available;
    quest.accepted_by = None;
    ctx.db.quest().id().update(quest);

    party.active_quest_id = None;
    ctx.db.party().id().update(party);

    log::info!("Party {} abandoned quest {}", party_id, quest_id);
    Ok(())
}

/// Complete a quest (called after tactical mission success)
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

    // Distribute rewards to all party members
    let members: Vec<_> = ctx
        .db
        .party_member()
        .party_id()
        .filter(&party_id)
        .collect();

    let gold_reward = quest.gold_reward;
    let xp_reward = quest.xp_reward;
    let gold_per_member = gold_reward / members.len().max(1) as i32;
    let xp_per_member = xp_reward / members.len().max(1) as i32;

    for member in members {
        if let Some(mut character) = ctx.db.character().id().find(&member.character_id) {
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

    log::info!(
        "Quest {} completed! {} gold and {} xp distributed",
        quest_id,
        gold_reward,
        xp_reward
    );
    Ok(())
}

// ============================================================================
// Seed Data Reducer
// ============================================================================

/// Seed initial settlements and quests for testing
#[reducer]
pub fn seed_world(ctx: &ReducerContext) -> Result<(), String> {
    // Create settlements if they don't exist
    let settlements = [
        ("riverdale", "Riverdale", 0.0, 0.0, 3, "town_a"),
        ("ironforge", "Ironforge", 100.0, 50.0, 4, "town_b"),
        ("willowmere", "Willowmere", -50.0, 75.0, 2, "town_a"),
    ];

    for (id, name, x, y, pop, scene) in settlements {
        if ctx.db.settlement().id().find(&id.to_string()).is_none() {
            ctx.db.settlement().insert(Settlement {
                id: id.into(),
                name: name.into(),
                coord_x: x,
                coord_y: y,
                population_level: pop,
                scene_key: scene.into(),
            });
        }
    }

    // Create quests for Riverdale
    let quests = [
        (
            "goblin-cave-1",
            "Clear the Goblin Cave",
            "A band of goblins has taken up residence in a nearby cave. Clear them out!",
            2,
            50,
            25,
            "riverdale",
            "goblins",
            5,
        ),
        (
            "bandit-camp-1",
            "Bandit Troubles",
            "Bandits have been raiding merchant caravans. Find their camp and deal with them.",
            3,
            100,
            50,
            "riverdale",
            "bandits",
            8,
        ),
        (
            "wolf-hunt-1",
            "Wolf Pack",
            "A pack of wolves has been attacking livestock. Hunt them down.",
            1,
            25,
            15,
            "riverdale",
            "wolves",
            4,
        ),
    ];

    for (id, title, desc, diff, gold, xp, settlement, enemy, count) in quests {
        if ctx.db.quest().id().find(&id.to_string()).is_none() {
            ctx.db.quest().insert(Quest {
                id: id.into(),
                title: title.into(),
                description: desc.into(),
                difficulty: diff,
                gold_reward: gold,
                xp_reward: xp,
                settlement_id: settlement.into(),
                status: QuestStatus::Available,
                accepted_by: None,
                enemy_type: enemy.into(),
                enemy_count: count,
            });
        }
    }

    // Create quests for Ironforge
    let ironforge_quests = [
        (
            "mine-infestation-1",
            "Mine Infestation",
            "Giant spiders have infested the old mine. Exterminate them.",
            3,
            75,
            40,
            "ironforge",
            "spiders",
            6,
        ),
        (
            "ore-thieves-1",
            "Ore Thieves",
            "Thieves have been stealing ore shipments. Put a stop to it.",
            2,
            60,
            30,
            "ironforge",
            "thieves",
            4,
        ),
    ];

    for (id, title, desc, diff, gold, xp, settlement, enemy, count) in ironforge_quests {
        if ctx.db.quest().id().find(&id.to_string()).is_none() {
            ctx.db.quest().insert(Quest {
                id: id.into(),
                title: title.into(),
                description: desc.into(),
                difficulty: diff,
                gold_reward: gold,
                xp_reward: xp,
                settlement_id: settlement.into(),
                status: QuestStatus::Available,
                accepted_by: None,
                enemy_type: enemy.into(),
                enemy_count: count,
            });
        }
    }

    log::info!("World seeded with settlements and quests");
    Ok(())
}

/// Player wants to enter a tactical mission
/// Creates a "pending" entry - spawner will see this and start server
#[reducer]
pub fn enter_mission(
    ctx: &ReducerContext,
    character_id: String,
    scene_key: String,
) -> Result<(), String> {
    // Validate scene
    if scene_key != "town_a" && scene_key != "town_b" {
        return Err(format!("Invalid scene: {}", scene_key));
    }

    // Check character exists
    if ctx.db.character().id().find(&character_id).is_none() {
        return Err("Character not found".into());
    }

    // Check not already in a mission
    if ctx
        .db
        .tactical_server()
        .iter()
        .any(|t| t.character_id == character_id && t.status != TacticalStatus::Ended)
    {
        return Err("Already in a mission".into());
    }

    let mission_id = format!(
        "{}-{}",
        character_id,
        ctx.timestamp.to_micros_since_unix_epoch()
    );

    ctx.db.tactical_server().insert(TacticalServer {
        mission_id: mission_id.clone(),
        scene_key,
        status: TacticalStatus::Pending,
        addr: String::new(),
        cert_digest: String::new(),
        character_id,
    });

    log::info!("Mission {} created (pending)", mission_id);
    Ok(())
}

/// Called by tactical-server when it starts - provides connection info
#[reducer]
pub fn tactical_server_ready(
    ctx: &ReducerContext,
    mission_id: String,
    addr: String,
    cert_digest: String,
) -> Result<(), String> {
    let Some(mut server) = ctx.db.tactical_server().mission_id().find(&mission_id) else {
        return Err("Mission not found".into());
    };

    server.status = TacticalStatus::Ready;
    server.addr = addr.clone();
    server.cert_digest = cert_digest;
    ctx.db.tactical_server().mission_id().update(server);

    log::info!("Mission {} ready on {}", mission_id, addr);
    Ok(())
}

/// Called by tactical-server when mission ends - applies rewards
#[reducer]
pub fn commit_mission(
    ctx: &ReducerContext,
    mission_id: String,
    success: bool,
    xp_gained: i32,
) -> Result<(), String> {
    let Some(mut server) = ctx.db.tactical_server().mission_id().find(&mission_id) else {
        return Err("Mission not found".into());
    };

    if server.status == TacticalStatus::Ended {
        return Ok(()); // Idempotent
    }

    // Apply rewards
    if let Some(mut character) = ctx.db.character().id().find(&server.character_id) {
        if success && xp_gained > 0 {
            character.xp += xp_gained;
            character.level = 1 + character.xp / 100;
            ctx.db.character().id().update(character.clone());

            // Victory loot
            add_item(ctx, &server.character_id, "gold_coin", 10);
            add_item(ctx, &server.character_id, "health_potion", 2);
        }
    }

    server.status = TacticalStatus::Ended;
    ctx.db.tactical_server().mission_id().update(server);

    log::info!(
        "Mission {} ended: success={}, xp={}",
        mission_id,
        success,
        xp_gained
    );
    Ok(())
}

/// Leave/cancel a mission
#[reducer]
pub fn leave_mission(ctx: &ReducerContext, mission_id: String) -> Result<(), String> {
    if let Some(mut server) = ctx.db.tactical_server().mission_id().find(&mission_id) {
        server.status = TacticalStatus::Ended;
        ctx.db.tactical_server().mission_id().update(server);
    }
    Ok(())
}

// ============================================================================
// Helpers
// ============================================================================

fn add_item(ctx: &ReducerContext, character_id: &str, item_id: &str, qty: i32) {
    // Find existing stack
    let existing = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(&character_id.to_string())
        .find(|i| i.item_id == item_id);

    match existing {
        Some(mut item) => {
            item.qty += qty;
            ctx.db.inventory_item().id().update(item);
        }
        None => {
            ctx.db.inventory_item().insert(InventoryItem {
                id: 0,
                character_id: character_id.into(),
                item_id: item_id.into(),
                qty,
            });
        }
    }
}
