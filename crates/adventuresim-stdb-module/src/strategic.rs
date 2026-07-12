use spacetimedb::{reducer, table, ReducerContext, SpacetimeType, Table};

use crate::{character::{character, character_equip}, item::{inventory_item, InventoryItem}, tactical::tactical_server_request};

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
    pub scene_key: String,
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
}

#[derive(Clone, Debug)]
#[table(name = party, public)]
pub struct Party {
    #[primary_key]
    pub id: String,
    pub name: String,
    pub leader_id: u64,
    pub current_settlement_id: Option<String>,
    pub active_quest_id: Option<String>,
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

#[reducer]
pub fn create_party(
    ctx: &ReducerContext,
    id: String,
    name: String,
    leader_id: u64,
) -> Result<(), String> {
    let Some(mut leader) = ctx.db.character().id().find(leader_id) else {
        return Err("Leader character not found".into());
    };

    if leader.party_id.is_some() {
        return Err("Leader is already in a party".into());
    }

    ctx.db.party().insert(Party {
        id: id.clone(),
        name,
        leader_id,
        current_settlement_id: leader.current_settlement_id.clone(),
        active_quest_id: None,
    });

    ctx.db.party_member().insert(PartyMember {
        id: 0,
        party_id: id.clone(),
        character_id: leader_id,
        role: Some("Leader".into()),
    });

    leader.party_id = Some(id);
    ctx.db.character().id().update(leader);
    Ok(())
}

#[reducer]
pub fn join_party(ctx: &ReducerContext, character_id: u64, party_id: String) -> Result<(), String> {
    let Some(mut character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };

    if character.party_id.is_some() {
        return Err("Character is already in a party".into());
    }

    let Some(party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };

    if character.current_settlement_id != party.current_settlement_id {
        return Err("Must be in the same settlement as the party".into());
    }

    ctx.db.party_member().insert(PartyMember {
        id: 0,
        party_id: party_id.clone(),
        character_id,
        role: None,
    });

    character.party_id = Some(party_id);
    ctx.db.character().id().update(character);
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
    let Some(from) = ctx.db.character().id().find(from_character_id) else { return Err("Source character not found".into()); };
    let Some(to) = ctx.db.character().id().find(to_character_id) else { return Err("Recipient character not found".into()); };
    if from.party_id.is_none() || from.party_id != to.party_id {
        return Err("Characters must belong to the same party".into());
    }
    let Some(source_item) = ctx.db.inventory_item().id().find(inventory_item_id) else { return Err("Inventory item not found".into()); };
    if source_item.character_id != from_character_id || source_item.quantity < quantity {
        return Err("Source character does not have that quantity".into());
    }
    if ctx.db.character_equip().character_id().find(from_character_id)
        .is_some_and(|equip| equip.is_equiped(inventory_item_id).is_some()) {
        return Err("Unequip an item before transferring it".into());
    }

    if source_item.quantity == quantity {
        ctx.db.inventory_item().id().delete(inventory_item_id);
    } else {
        let mut updated = source_item.clone();
        updated.quantity -= quantity;
        ctx.db.inventory_item().id().update(updated);
    }
    if let Some(mut destination_item) = ctx.db.inventory_item().character_and_item_id().filter((to_character_id, &source_item.item_id)).next() {
        destination_item.quantity = destination_item.quantity.saturating_add(quantity);
        ctx.db.inventory_item().id().update(destination_item);
    } else {
        ctx.db.inventory_item().insert(InventoryItem { id: 0, character_id: to_character_id, item_id: source_item.item_id, quantity });
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
        || from_character_ids.is_empty() {
        return Err("Offer entries must be non-empty and aligned".into());
    }
    for index in 0..from_character_ids.len() {
        let from_id = from_character_ids[index];
        let to_id = to_character_ids[index];
        let quantity = quantities[index];
        let Some(from) = ctx.db.character().id().find(from_id) else { return Err("Source character not found".into()); };
        let Some(to) = ctx.db.character().id().find(to_id) else { return Err("Recipient character not found".into()); };
        let Some(item) = ctx.db.inventory_item().id().find(inventory_item_ids[index]) else { return Err("Inventory item not found".into()); };
        if quantity == 0 || from_id == to_id || from.party_id.is_none() || from.party_id != to.party_id || item.character_id != from_id || item.quantity < quantity {
            return Err("Invalid party trade offer".into());
        }
        if ctx.db.character_equip().character_id().find(from_id).is_some_and(|equip| equip.is_equiped(item.id).is_some()) {
            return Err("Unequip an item before offering it".into());
        }
    }
    for index in 0..from_character_ids.len() {
        transfer_party_item(ctx, from_character_ids[index], to_character_ids[index], inventory_item_ids[index], quantities[index])?;
    }
    Ok(())
}

/// Adds two deterministic companions to the specified character's party for
/// strategic UI development. Safe to call repeatedly.
#[reducer]
pub fn seed_party_companions(ctx: &ReducerContext, leader_id: u64) -> Result<(), String> {
    let party_id = "demo-party".to_string();
    if ctx.db.party().id().find(&party_id).is_none() {
        create_party(ctx, party_id.clone(), "Riverdale Company".into(), leader_id)?;
    }

    for (id, name) in [(9_000_001_u64, "Mara"), (9_000_002_u64, "Orrin")] {
        if ctx.db.character().id().find(id).is_none() {
            crate::character::insert_new_character(ctx, name.into(), id, false)?;
        }
        if ctx.db.character().id().find(id).and_then(|character| character.party_id).is_none() {
            join_party(ctx, id, party_id.clone())?;
        }
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
    Ok(())
}

#[reducer]
pub fn disband_party(ctx: &ReducerContext, party_id: String) -> Result<(), String> {
    let Some(party) = ctx.db.party().id().find(&party_id) else {
        return Err("Party not found".into());
    };

    let members: Vec<_> = ctx.db.party_member().party_id().filter(&party_id).collect();
    for member in members {
        if let Some(mut character) = ctx.db.character().id().find(member.character_id) {
            character.party_id = None;
            ctx.db.character().id().update(character);
        }
        ctx.db.party_member().id().delete(member.id);
    }

    if let Some(quest_id) = party.active_quest_id {
        if let Some(mut quest) = ctx.db.quest().id().find(&quest_id) {
            quest.status = QuestStatus::Available;
            quest.accepted_by = None;
            ctx.db.quest().id().update(quest);
        }
    }

    ctx.db.party().id().delete(&party_id);
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
    ctx.db.quest().id().update(quest);

    party.active_quest_id = Some(quest_id);
    ctx.db.party().id().update(party);
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
    Ok(())
}

#[reducer]
pub fn travel_to_settlement(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: String,
) -> Result<(), String> {
    if ctx.db.settlement().id().find(&settlement_id).is_none() {
        return Err("Settlement not found".into());
    }

    let Some(mut character) = ctx.db.character().id().find(character_id) else {
        return Err("Character not found".into());
    };

    character.current_settlement_id = Some(settlement_id.clone());
    let party_id = character.party_id.clone();
    ctx.db.character().id().update(character);

    if let Some(party_id) = party_id {
        if let Some(mut party) = ctx.db.party().id().find(&party_id) {
            if party.leader_id == character_id {
                party.current_settlement_id = Some(settlement_id);
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
                scene_key: scene.into(),
            });
        }
    }

    seed_quest(
        ctx,
        "goblin-cave-1",
        "Clear the Goblin Cave",
        "A band of goblins has taken up residence nearby.",
        2,
        50,
        25,
        "riverdale",
        "goblins",
        5,
    )?;
    seed_quest(
        ctx,
        "bandit-camp-1",
        "Bandit Troubles",
        "Bandits have been raiding merchant caravans.",
        3,
        100,
        50,
        "riverdale",
        "bandits",
        8,
    )?;
    seed_quest(
        ctx,
        "wolf-hunt-1",
        "Wolf Pack",
        "A pack of wolves has been attacking livestock.",
        1,
        25,
        15,
        "riverdale",
        "wolves",
        4,
    )?;
    seed_quest(
        ctx,
        "mine-infestation-1",
        "Mine Infestation",
        "Giant spiders have infested the old mine.",
        3,
        75,
        40,
        "ironforge",
        "spiders",
        6,
    )?;
    seed_quest(
        ctx,
        "ore-thieves-1",
        "Ore Thieves",
        "Thieves have been stealing ore shipments.",
        2,
        60,
        30,
        "ironforge",
        "thieves",
        4,
    )?;

    Ok(())
}

fn seed_quest(
    ctx: &ReducerContext,
    id: &str,
    title: &str,
    description: &str,
    difficulty: i32,
    gold_reward: i32,
    xp_reward: i32,
    settlement_id: &str,
    enemy_type: &str,
    enemy_count: i32,
) -> Result<(), String> {
    if ctx.db.quest().id().find(&id.to_string()).is_some() {
        return Ok(());
    }

    ctx.db.quest().insert(Quest {
        id: id.into(),
        title: title.into(),
        description: description.into(),
        difficulty,
        gold_reward,
        xp_reward,
        settlement_id: settlement_id.into(),
        status: QuestStatus::Available,
        accepted_by: None,
        enemy_type: enemy_type.into(),
        enemy_count,
    });
    Ok(())
}
