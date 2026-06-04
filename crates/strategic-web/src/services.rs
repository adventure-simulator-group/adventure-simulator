//! SQL-backed strategic services.

use std::net::TcpListener;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::config::Config;
use crate::models::{
    Character, CharacterAttributes, CharacterEquip, CharacterLimbs, CharacterSkills,
    CharacterStats, ConnectedPlayer, ConnectedPlayerItem, InventoryItem, MissionRecord, Party,
    PartyMember, Quest, Settlement, TacticalCharacter, TacticalInventoryItemRow, TacticalItem,
    TacticalServer,
};

pub fn chrono_id() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as i64
}

pub fn u64_to_i64(id: u64) -> anyhow::Result<i64> {
    i64::try_from(id).context("id is too large for SQLite INTEGER")
}

pub async fn seed_world(pool: &SqlitePool) -> anyhow::Result<()> {
    seed_items(pool).await?;

    let settlements = [
        ("riverdale", "Riverdale", 0.0, 0.0, 3, "hills"),
        ("ironforge", "Ironforge", 100.0, 50.0, 4, "desert"),
        ("willowmere", "Willowmere", -50.0, 75.0, 2, "hills"),
    ];

    for (id, name, x, y, pop, scene) in settlements {
        sqlx::query(
            r#"
            INSERT INTO settlements (id, name, coord_x, coord_y, population_level, scene_key)
            VALUES (?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO NOTHING
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(x)
        .bind(y)
        .bind(pop)
        .bind(scene)
        .execute(pool)
        .await?;
    }

    seed_quest(
        pool,
        "cave-1",
        "Clear the Cave",
        "A hostile band has taken up residence nearby.",
        2,
        50,
        25,
        "riverdale",
        "raiders",
        5,
    )
    .await?;
    seed_quest(
        pool,
        "bandit-camp-1",
        "Bandit Troubles",
        "Bandits have been raiding merchant caravans.",
        3,
        100,
        50,
        "riverdale",
        "bandits",
        8,
    )
    .await?;
    seed_quest(
        pool,
        "wolf-hunt-1",
        "Wolf Pack",
        "A pack has been attacking livestock.",
        1,
        25,
        15,
        "riverdale",
        "wolves",
        4,
    )
    .await?;
    seed_quest(
        pool,
        "mine-infestation-1",
        "Mine Infestation",
        "Giant spiders have infested the old mine.",
        3,
        75,
        40,
        "ironforge",
        "spiders",
        6,
    )
    .await?;
    seed_quest(
        pool,
        "ore-thieves-1",
        "Ore Thieves",
        "Thieves have been stealing ore shipments.",
        2,
        60,
        30,
        "ironforge",
        "thieves",
        4,
    )
    .await?;

    Ok(())
}

async fn seed_items(pool: &SqlitePool) -> anyhow::Result<()> {
    define_item(pool, "torch", 0.5).await?;
    define_item(pool, "bandage", 0.05).await?;
    define_item(pool, "gold_coin", 0.01).await?;
    define_item(pool, "health_potion", 0.2).await?;
    define_shield(pool, "buckler", 1.0, 1.0).await?;
    define_weapon(pool, "short_sword", 1.5, 1.5).await?;
    define_weapon(pool, "knife", 0.5, 2.0).await?;
    define_weapon(pool, "zweihander", 6.0, 0.6).await?;
    define_armor(pool, "leather_armguard", 0.5, "AnyArm", 1.0, 0.2).await?;
    define_armor(pool, "leather_helmet", 0.5, "Head", 1.0, 0.3).await?;
    define_armor(pool, "leather_vest", 0.5, "Chest", 1.0, 0.3).await?;
    define_armor(pool, "leather_belt", 0.5, "Stomach", 1.0, 0.2).await?;
    define_armor(pool, "leather_cuisse", 0.5, "AnyLeg", 1.0, 0.4).await?;
    define_armor(pool, "steel_sallet", 2.0, "Head", 0.6, 0.7).await?;
    Ok(())
}

async fn define_item(pool: &SqlitePool, id: &str, weight: f32) -> anyhow::Result<()> {
    upsert_item(pool, id, weight, "None", "Simple", 0.0, 0.0, 0.0, 0.0).await
}

async fn define_weapon(
    pool: &SqlitePool,
    id: &str,
    weight: f32,
    accuracy: f32,
) -> anyhow::Result<()> {
    upsert_item(pool, id, weight, "None", "Weapon", accuracy, 0.0, 0.0, 0.0).await
}

async fn define_shield(pool: &SqlitePool, id: &str, weight: f32, block: f32) -> anyhow::Result<()> {
    upsert_item(pool, id, weight, "None", "Shield", 0.0, block, 0.0, 0.0).await
}

async fn define_armor(
    pool: &SqlitePool,
    id: &str,
    weight: f32,
    slot: &str,
    dodge: f32,
    coverage: f32,
) -> anyhow::Result<()> {
    upsert_item(pool, id, weight, slot, "Armor", 0.0, 0.0, dodge, coverage).await
}

async fn upsert_item(
    pool: &SqlitePool,
    id: &str,
    weight: f32,
    slot: &str,
    kind: &str,
    accuracy: f32,
    block: f32,
    dodge: f32,
    coverage: f32,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO items (id, weight, slot, kind, accuracy, block, dodge, coverage)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            weight = excluded.weight,
            slot = excluded.slot,
            kind = excluded.kind,
            accuracy = excluded.accuracy,
            block = excluded.block,
            dodge = excluded.dodge,
            coverage = excluded.coverage
        "#,
    )
    .bind(id)
    .bind(weight)
    .bind(slot)
    .bind(kind)
    .bind(accuracy)
    .bind(block)
    .bind(dodge)
    .bind(coverage)
    .execute(pool)
    .await?;
    Ok(())
}

async fn seed_quest(
    pool: &SqlitePool,
    id: &str,
    title: &str,
    description: &str,
    difficulty: i32,
    gold_reward: i64,
    xp_reward: i64,
    settlement_id: &str,
    enemy_type: &str,
    enemy_count: i32,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO quests (
            id, title, description, difficulty, gold_reward, xp_reward,
            settlement_id, status, accepted_by, enemy_type, enemy_count
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, 'Available', NULL, ?, ?)
        ON CONFLICT(id) DO NOTHING
        "#,
    )
    .bind(id)
    .bind(title)
    .bind(description)
    .bind(difficulty)
    .bind(gold_reward)
    .bind(xp_reward)
    .bind(settlement_id)
    .bind(enemy_type)
    .bind(enemy_count)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_characters(pool: &SqlitePool) -> anyhow::Result<Vec<Character>> {
    sqlx::query_as::<_, Character>(
        r#"
        SELECT id, name, xp, level, gold, current_settlement_id, party_id,
               active_mission_id, in_mission
        FROM characters
        ORDER BY name
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub async fn get_character(pool: &SqlitePool, id: i64) -> anyhow::Result<Option<Character>> {
    sqlx::query_as::<_, Character>(
        r#"
        SELECT id, name, xp, level, gold, current_settlement_id, party_id,
               active_mission_id, in_mission
        FROM characters
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

pub async fn get_character_name(pool: &SqlitePool, id: Option<&str>) -> Option<String> {
    let id = id?.parse::<i64>().ok()?;
    get_character(pool, id).await.ok().flatten().map(|c| c.name)
}

pub async fn create_named_character_with_id(
    pool: &SqlitePool,
    id: i64,
    name: String,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO characters (
            id, name, xp, level, gold, current_settlement_id, party_id,
            active_mission_id, in_mission
        )
        VALUES (?, ?, 0, 1, 100, 'riverdale', NULL, NULL, 0)
        "#,
    )
    .bind(id)
    .bind(name)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO character_stats (character_id, calories_used, focus)
        VALUES (?, 0.0, 1.0)
        "#,
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO character_skills (character_id, melee_hours, dodge_hours, block_hours)
        VALUES (?, 2000.0, 1000.0, 1000.0)
        "#,
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO character_limbs (
            character_id, left_arm, right_arm, left_leg, right_leg, head, chest, stomach
        )
        VALUES (?, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0)
        "#,
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO character_attributes (
            character_id, endurance, immunity, gut, strength, precision, agility,
            intelligence, instinct, eyesight, hearing
        )
        VALUES (?, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0)
        "#,
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    let torch_id = add_inventory_item_tx(&mut tx, id, "torch", 1).await?;
    let bandage_id = add_inventory_item_tx(&mut tx, id, "bandage", 3).await?;
    let buckler_id = add_inventory_item_tx(&mut tx, id, "buckler", 1).await?;
    let sword_id = add_inventory_item_tx(&mut tx, id, "short_sword", 1).await?;
    let left_arm_id = add_inventory_item_tx(&mut tx, id, "leather_armguard", 1).await?;
    let right_arm_id = add_inventory_item_tx(&mut tx, id, "leather_armguard", 1).await?;
    let helmet_id = add_inventory_item_tx(&mut tx, id, "leather_helmet", 1).await?;
    let chest_id = add_inventory_item_tx(&mut tx, id, "leather_vest", 1).await?;
    let stomach_id = add_inventory_item_tx(&mut tx, id, "leather_vest", 1).await?;
    let left_leg_id = add_inventory_item_tx(&mut tx, id, "leather_cuisse", 1).await?;
    let right_leg_id = add_inventory_item_tx(&mut tx, id, "leather_cuisse", 1).await?;

    let _ = (torch_id, bandage_id);

    sqlx::query(
        r#"
        INSERT INTO character_equip (
            character_id, left_hand_item_id, right_hand_item_id,
            left_arm_armor_id, right_arm_armor_id, left_leg_armor_id, right_leg_armor_id,
            head_armor_id, chest_armor_id, stomach_armor_id
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(id)
    .bind(buckler_id)
    .bind(sword_id)
    .bind(left_arm_id)
    .bind(right_arm_id)
    .bind(left_leg_id)
    .bind(right_leg_id)
    .bind(helmet_id)
    .bind(chest_id)
    .bind(stomach_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

async fn add_inventory_item_tx(
    tx: &mut Transaction<'_, Sqlite>,
    character_id: i64,
    item_id: &str,
    quantity: i64,
) -> anyhow::Result<i64> {
    let result = sqlx::query(
        r#"
        INSERT INTO inventory_items (character_id, item_id, quantity)
        VALUES (?, ?, ?)
        "#,
    )
    .bind(character_id)
    .bind(item_id)
    .bind(quantity)
    .execute(&mut **tx)
    .await?;

    Ok(result.last_insert_rowid())
}

async fn change_inventory_item_tx(
    tx: &mut Transaction<'_, Sqlite>,
    character_id: i64,
    item_id: &str,
    by_quantity: i64,
) -> anyhow::Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE inventory_items
        SET quantity = MAX(quantity + ?, 0)
        WHERE character_id = ? AND item_id = ?
        "#,
    )
    .bind(by_quantity)
    .bind(character_id)
    .bind(item_id)
    .execute(&mut **tx)
    .await?;

    if result.rows_affected() == 0 && by_quantity > 0 {
        add_inventory_item_tx(tx, character_id, item_id, by_quantity).await?;
    }

    Ok(())
}

pub async fn update_character(pool: &SqlitePool, id: i64, name: String) -> anyhow::Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE characters
        SET name = ?, updated_at = CURRENT_TIMESTAMP
        WHERE id = ?
        "#,
    )
    .bind(name)
    .bind(id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        bail!("Character not found");
    }

    Ok(())
}

pub async fn inventory_for_character(
    pool: &SqlitePool,
    character_id: i64,
) -> anyhow::Result<Vec<InventoryItem>> {
    sqlx::query_as::<_, InventoryItem>(
        r#"
        SELECT id, character_id, item_id, quantity
        FROM inventory_items
        WHERE character_id = ?
        ORDER BY id
        "#,
    )
    .bind(character_id)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub async fn list_parties(pool: &SqlitePool) -> anyhow::Result<Vec<Party>> {
    sqlx::query_as::<_, Party>(
        r#"
        SELECT id, name, leader_id, current_settlement_id, active_quest_id
        FROM parties
        ORDER BY name
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub async fn get_party(pool: &SqlitePool, id: &str) -> anyhow::Result<Option<Party>> {
    sqlx::query_as::<_, Party>(
        r#"
        SELECT id, name, leader_id, current_settlement_id, active_quest_id
        FROM parties
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

pub async fn list_party_members(
    pool: &SqlitePool,
    party_id: &str,
) -> anyhow::Result<Vec<PartyMember>> {
    sqlx::query_as::<_, PartyMember>(
        r#"
        SELECT id, party_id, character_id, role
        FROM party_members
        WHERE party_id = ?
        ORDER BY id
        "#,
    )
    .bind(party_id)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub async fn create_party(
    pool: &SqlitePool,
    id: String,
    name: String,
    leader_id: i64,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    let mut leader = get_character_for_update(&mut tx, leader_id).await?;

    if leader.party_id.is_some() {
        bail!("Leader is already in a party");
    }

    sqlx::query(
        r#"
        INSERT INTO parties (id, name, leader_id, current_settlement_id, active_quest_id)
        VALUES (?, ?, ?, ?, NULL)
        "#,
    )
    .bind(&id)
    .bind(name)
    .bind(leader_id)
    .bind(leader.current_settlement_id.clone())
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO party_members (party_id, character_id, role)
        VALUES (?, ?, 'Leader')
        "#,
    )
    .bind(&id)
    .bind(leader_id)
    .execute(&mut *tx)
    .await?;

    leader.party_id = Some(id.clone());
    update_character_party_tx(&mut tx, leader.id, leader.party_id.as_deref()).await?;

    tx.commit().await?;
    Ok(())
}

pub async fn join_party(
    pool: &SqlitePool,
    character_id: i64,
    party_id: String,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    let mut character = get_character_for_update(&mut tx, character_id).await?;

    if character.party_id.is_some() {
        bail!("Character is already in a party");
    }

    let party = get_party_for_update(&mut tx, &party_id).await?;
    if character.current_settlement_id != party.current_settlement_id {
        bail!("Must be in the same settlement as the party");
    }

    sqlx::query(
        r#"
        INSERT INTO party_members (party_id, character_id, role)
        VALUES (?, ?, NULL)
        "#,
    )
    .bind(&party_id)
    .bind(character_id)
    .execute(&mut *tx)
    .await?;

    character.party_id = Some(party_id);
    update_character_party_tx(&mut tx, character.id, character.party_id.as_deref()).await?;

    tx.commit().await?;
    Ok(())
}

pub async fn leave_party(pool: &SqlitePool, character_id: i64) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    let character = get_character_for_update(&mut tx, character_id).await?;
    let Some(party_id) = character.party_id.clone() else {
        bail!("Character is not in a party");
    };

    let party = get_party_for_update(&mut tx, &party_id).await?;
    if party.leader_id == character_id {
        bail!("Party leader cannot leave. Use disband_party instead.");
    }

    sqlx::query(
        r#"
        DELETE FROM party_members
        WHERE party_id = ? AND character_id = ?
        "#,
    )
    .bind(&party_id)
    .bind(character_id)
    .execute(&mut *tx)
    .await?;

    update_character_party_tx(&mut tx, character_id, None).await?;

    tx.commit().await?;
    Ok(())
}

pub async fn disband_party(pool: &SqlitePool, party_id: String) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    let party = get_party_for_update(&mut tx, &party_id).await?;

    sqlx::query(
        r#"
        UPDATE characters
        SET party_id = NULL, updated_at = CURRENT_TIMESTAMP
        WHERE party_id = ?
        "#,
    )
    .bind(&party_id)
    .execute(&mut *tx)
    .await?;

    if let Some(quest_id) = party.active_quest_id {
        sqlx::query(
            r#"
            UPDATE quests
            SET status = 'Available', accepted_by = NULL
            WHERE id = ?
            "#,
        )
        .bind(quest_id)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query("DELETE FROM party_members WHERE party_id = ?")
        .bind(&party_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM parties WHERE id = ?")
        .bind(&party_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn list_quests(pool: &SqlitePool) -> anyhow::Result<Vec<Quest>> {
    sqlx::query_as::<_, Quest>(
        r#"
        SELECT id, title, description, difficulty, gold_reward, xp_reward,
               settlement_id, status, accepted_by, enemy_type, enemy_count
        FROM quests
        ORDER BY settlement_id, difficulty, title
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub async fn get_quest(pool: &SqlitePool, id: &str) -> anyhow::Result<Option<Quest>> {
    sqlx::query_as::<_, Quest>(
        r#"
        SELECT id, title, description, difficulty, gold_reward, xp_reward,
               settlement_id, status, accepted_by, enemy_type, enemy_count
        FROM quests
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

pub async fn quests_at_settlement(
    pool: &SqlitePool,
    settlement_id: &str,
) -> anyhow::Result<Vec<Quest>> {
    sqlx::query_as::<_, Quest>(
        r#"
        SELECT id, title, description, difficulty, gold_reward, xp_reward,
               settlement_id, status, accepted_by, enemy_type, enemy_count
        FROM quests
        WHERE settlement_id = ?
        ORDER BY difficulty, title
        "#,
    )
    .bind(settlement_id)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub async fn accept_quest(
    pool: &SqlitePool,
    character_id: i64,
    quest_id: String,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    let character = get_character_for_update(&mut tx, character_id).await?;
    let Some(party_id) = character.party_id.clone() else {
        bail!("Must be in a party to accept quests");
    };

    let mut party = get_party_for_update(&mut tx, &party_id).await?;
    if party.leader_id != character_id {
        bail!("Only the party leader can accept quests");
    }
    if party.active_quest_id.is_some() {
        bail!("Party already has an active quest");
    }

    let quest = get_quest_for_update(&mut tx, &quest_id).await?;
    if !quest.status.eq_ignore_ascii_case("available") {
        bail!("Quest is not available");
    }
    if character.current_settlement_id.as_ref() != Some(&quest.settlement_id) {
        bail!("Must be at the quest's settlement to accept it");
    }

    sqlx::query(
        r#"
        UPDATE quests
        SET status = 'Accepted', accepted_by = ?
        WHERE id = ?
        "#,
    )
    .bind(&party_id)
    .bind(&quest_id)
    .execute(&mut *tx)
    .await?;

    party.active_quest_id = Some(quest_id);
    sqlx::query("UPDATE parties SET active_quest_id = ? WHERE id = ?")
        .bind(party.active_quest_id)
        .bind(party_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn abandon_quest(
    pool: &SqlitePool,
    character_id: i64,
    quest_id: String,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    let character = get_character_for_update(&mut tx, character_id).await?;
    let Some(party_id) = character.party_id.clone() else {
        bail!("Not in a party");
    };
    let party = get_party_for_update(&mut tx, &party_id).await?;
    if party.leader_id != character_id {
        bail!("Only the party leader can abandon quests");
    }

    let quest = get_quest_for_update(&mut tx, &quest_id).await?;
    if quest.accepted_by.as_ref() != Some(&party_id) {
        bail!("This quest is not accepted by your party");
    }

    sqlx::query(
        r#"
        UPDATE quests
        SET status = 'Available', accepted_by = NULL
        WHERE id = ?
        "#,
    )
    .bind(&quest_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE parties SET active_quest_id = NULL WHERE id = ?")
        .bind(&party_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn complete_quest(pool: &SqlitePool, quest_id: String) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    complete_quest_tx(&mut tx, &quest_id).await?;
    tx.commit().await?;
    Ok(())
}

async fn complete_quest_tx(tx: &mut Transaction<'_, Sqlite>, quest_id: &str) -> anyhow::Result<()> {
    let quest = get_quest_for_update(tx, quest_id).await?;
    if !quest.status.eq_ignore_ascii_case("accepted") {
        bail!("Quest is not in accepted state");
    }
    let Some(party_id) = quest.accepted_by.clone() else {
        bail!("Quest has no party assigned");
    };

    let members = sqlx::query_as::<_, PartyMember>(
        r#"
        SELECT id, party_id, character_id, role
        FROM party_members
        WHERE party_id = ?
        "#,
    )
    .bind(&party_id)
    .fetch_all(&mut **tx)
    .await?;

    let member_count = members.len().max(1) as i64;
    let gold_per_member = quest.gold_reward.max(0) / member_count;
    let xp_per_member = quest.xp_reward.max(0) / member_count;

    for member in members {
        add_character_rewards_tx(tx, member.character_id, xp_per_member, gold_per_member).await?;
    }

    sqlx::query("UPDATE quests SET status = 'Completed' WHERE id = ?")
        .bind(quest_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("UPDATE parties SET active_quest_id = NULL WHERE id = ?")
        .bind(&party_id)
        .execute(&mut **tx)
        .await?;

    Ok(())
}

async fn add_character_rewards_tx(
    tx: &mut Transaction<'_, Sqlite>,
    character_id: i64,
    xp: i64,
    gold: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE characters
        SET xp = xp + ?,
            gold = gold + ?,
            level = 1 + ((xp + ?) / 100),
            updated_at = CURRENT_TIMESTAMP
        WHERE id = ?
        "#,
    )
    .bind(xp.max(0))
    .bind(gold.max(0))
    .bind(xp.max(0))
    .bind(character_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn list_settlements(pool: &SqlitePool) -> anyhow::Result<Vec<Settlement>> {
    sqlx::query_as::<_, Settlement>(
        r#"
        SELECT id, name, coord_x, coord_y, population_level, scene_key
        FROM settlements
        ORDER BY name
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub async fn get_settlement(pool: &SqlitePool, id: &str) -> anyhow::Result<Option<Settlement>> {
    sqlx::query_as::<_, Settlement>(
        r#"
        SELECT id, name, coord_x, coord_y, population_level, scene_key
        FROM settlements
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

pub async fn parties_at_settlement(
    pool: &SqlitePool,
    settlement_id: &str,
) -> anyhow::Result<Vec<Party>> {
    sqlx::query_as::<_, Party>(
        r#"
        SELECT id, name, leader_id, current_settlement_id, active_quest_id
        FROM parties
        WHERE current_settlement_id = ?
        ORDER BY name
        "#,
    )
    .bind(settlement_id)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub async fn travel_to_settlement(
    pool: &SqlitePool,
    character_id: i64,
    settlement_id: String,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    if get_settlement_tx(&mut tx, &settlement_id).await?.is_none() {
        bail!("Settlement not found");
    }

    let character = get_character_for_update(&mut tx, character_id).await?;
    sqlx::query(
        r#"
        UPDATE characters
        SET current_settlement_id = ?, updated_at = CURRENT_TIMESTAMP
        WHERE id = ?
        "#,
    )
    .bind(&settlement_id)
    .bind(character_id)
    .execute(&mut *tx)
    .await?;

    if let Some(party_id) = character.party_id {
        let party = get_party_for_update(&mut tx, &party_id).await?;
        if party.leader_id == character_id {
            sqlx::query("UPDATE parties SET current_settlement_id = ? WHERE id = ?")
                .bind(&settlement_id)
                .bind(&party_id)
                .execute(&mut *tx)
                .await?;
        }
    }

    tx.commit().await?;
    Ok(())
}

pub struct MissionLaunch {
    pub id: String,
    pub scene_key: String,
    pub bind_addr: String,
    pub public_addr: String,
}

pub async fn request_tactical_mission(
    pool: &SqlitePool,
    config: &Config,
    requester_character_id: i64,
) -> anyhow::Result<MissionLaunch> {
    let character = get_character(pool, requester_character_id)
        .await?
        .ok_or_else(|| anyhow!("Character not found"))?;
    let Some(party_id) = character.party_id.clone() else {
        bail!("Character is not in a party");
    };
    let party = get_party(pool, &party_id)
        .await?
        .ok_or_else(|| anyhow!("Party not found"))?;
    if party.leader_id != requester_character_id {
        bail!("Only the party leader can start a mission");
    }
    let Some(quest_id) = party.active_quest_id.clone() else {
        bail!("Party has no active quest");
    };
    let quest = get_quest(pool, &quest_id)
        .await?
        .ok_or_else(|| anyhow!("Quest not found"))?;
    let settlement = get_settlement(pool, &quest.settlement_id)
        .await?
        .ok_or_else(|| anyhow!("Settlement not found"))?;

    let mission_id = format!("party-{}-{}", party_id, chrono_id());
    let port = choose_tactical_port(&config.tactical_bind_host)?;
    let bind_addr = format!("{}:{}", config.tactical_bind_host, port);
    let public_addr = format!("{}:{}", config.tactical_public_host, port);

    sqlx::query(
        r#"
        INSERT INTO missions (
            id, scene_key, status, party_id, quest_id, requester_character_id,
            addr, cert_digest, pid, success, xp_gained, result_committed
        )
        VALUES (?, ?, 'requested', ?, ?, ?, ?, '', NULL, NULL, 0, 0)
        "#,
    )
    .bind(&mission_id)
    .bind(&settlement.scene_key)
    .bind(&party_id)
    .bind(&quest_id)
    .bind(requester_character_id)
    .bind(&public_addr)
    .execute(pool)
    .await?;

    Ok(MissionLaunch {
        id: mission_id,
        scene_key: settlement.scene_key,
        bind_addr,
        public_addr,
    })
}

fn choose_tactical_port(bind_host: &str) -> anyhow::Result<u16> {
    let listener = TcpListener::bind((bind_host, 0))
        .with_context(|| format!("failed to reserve tactical port on {bind_host}"))?;
    Ok(listener.local_addr()?.port())
}

pub async fn spawn_tactical_server(
    pool: &SqlitePool,
    config: &Config,
    launch: &MissionLaunch,
) -> anyhow::Result<()> {
    let child = Command::new(&config.tactical_server_bin)
        .args([
            "--mission-id",
            &launch.id,
            "--scene-key",
            &launch.scene_key,
            "--addr",
            &launch.bind_addr,
            "--public-addr",
            &launch.public_addr,
            "--strategic-url",
            &config.strategic_internal_url,
        ])
        .spawn()
        .with_context(|| {
            format!(
                "failed to spawn tactical server binary '{}'",
                config.tactical_server_bin
            )
        })?;

    let pid = i64::from(child.id());
    sqlx::query(
        r#"
        UPDATE missions
        SET status = 'starting', pid = ?, addr = ?, updated_at = CURRENT_TIMESTAMP
        WHERE id = ?
        "#,
    )
    .bind(pid)
    .bind(&launch.public_addr)
    .bind(&launch.id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn mark_mission_failed(pool: &SqlitePool, mission_id: &str) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE missions
        SET status = 'failed', updated_at = CURRENT_TIMESTAMP
        WHERE id = ? AND status NOT IN ('ended', 'cancelled')
        "#,
    )
    .bind(mission_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_mission_ready(
    pool: &SqlitePool,
    mission_id: &str,
    addr: String,
    cert_digest: String,
) -> anyhow::Result<()> {
    let result = sqlx::query(
        r#"
        UPDATE missions
        SET status = 'ready',
            addr = ?,
            cert_digest = ?,
            ready_at = CURRENT_TIMESTAMP,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = ? AND status NOT IN ('ended', 'cancelled')
        "#,
    )
    .bind(addr)
    .bind(cert_digest)
    .bind(mission_id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        bail!("Mission not found or no longer startable");
    }

    Ok(())
}

pub async fn get_mission_for_viewer(
    pool: &SqlitePool,
    mission_id: &str,
) -> anyhow::Result<Option<TacticalServer>> {
    sqlx::query_as::<_, TacticalServer>(
        r#"
        SELECT id, scene_key, status,
               COALESCE(addr, '') AS addr,
               COALESCE(cert_digest, '') AS cert_digest,
               requester_character_id,
               party_id
        FROM missions
        WHERE id = ?
        "#,
    )
    .bind(mission_id)
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

pub async fn get_mission(
    pool: &SqlitePool,
    mission_id: &str,
) -> anyhow::Result<Option<MissionRecord>> {
    sqlx::query_as::<_, MissionRecord>(
        r#"
        SELECT id, scene_key, status, party_id, quest_id, requester_character_id,
               addr, cert_digest, pid, success, xp_gained, result_committed
        FROM missions
        WHERE id = ?
        "#,
    )
    .bind(mission_id)
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

pub fn can_view_mission(viewer: &Character, mission: &TacticalServer) -> bool {
    if mission.character_id == Some(viewer.id) {
        return true;
    }

    match (&viewer.party_id, &mission.party_id) {
        (Some(viewer_party), Some(mission_party)) => viewer_party == mission_party,
        _ => false,
    }
}

pub async fn can_cancel_mission(
    pool: &SqlitePool,
    viewer: &Character,
    mission: &TacticalServer,
) -> anyhow::Result<bool> {
    if let Some(party_id) = &mission.party_id {
        if let Some(party) = get_party(pool, party_id).await? {
            return Ok(party.leader_id == viewer.id);
        }
    }

    Ok(mission.character_id == Some(viewer.id))
}

pub async fn cancel_mission(pool: &SqlitePool, mission_id: &str) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE missions
        SET status = 'cancelled', updated_at = CURRENT_TIMESTAMP
        WHERE id = ? AND status IN ('requested', 'starting', 'ready')
        "#,
    )
    .bind(mission_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn load_tactical_player_data(
    pool: &SqlitePool,
    mission_id: &str,
    character_id: i64,
) -> anyhow::Result<ConnectedPlayer> {
    let mission = get_mission(pool, mission_id)
        .await?
        .ok_or_else(|| anyhow!("Mission not found"))?;
    if matches!(mission.status.as_str(), "ended" | "failed" | "cancelled") {
        bail!("Mission is not joinable");
    }

    let character = get_character(pool, character_id)
        .await?
        .ok_or_else(|| anyhow!("Character not found"))?;

    if !character_is_allowed_for_mission(&character, &mission) {
        bail!("Character is not authorized for this mission");
    }

    let attrs = sqlx::query_as::<_, CharacterAttributes>(
        "SELECT * FROM character_attributes WHERE character_id = ?",
    )
    .bind(character_id)
    .fetch_one(pool)
    .await?;
    let stats =
        sqlx::query_as::<_, CharacterStats>("SELECT * FROM character_stats WHERE character_id = ?")
            .bind(character_id)
            .fetch_one(pool)
            .await?;
    let skills = sqlx::query_as::<_, CharacterSkills>(
        "SELECT * FROM character_skills WHERE character_id = ?",
    )
    .bind(character_id)
    .fetch_one(pool)
    .await?;
    let limbs =
        sqlx::query_as::<_, CharacterLimbs>("SELECT * FROM character_limbs WHERE character_id = ?")
            .bind(character_id)
            .fetch_one(pool)
            .await?;
    let equip =
        sqlx::query_as::<_, CharacterEquip>("SELECT * FROM character_equip WHERE character_id = ?")
            .bind(character_id)
            .fetch_optional(pool)
            .await?;

    let item_rows = sqlx::query_as::<_, TacticalInventoryItemRow>(
        r#"
        SELECT inventory_items.id AS inventory_item_id,
               inventory_items.quantity,
               items.id AS item_id,
               items.weight,
               items.slot,
               items.kind,
               items.accuracy,
               items.block,
               items.dodge,
               items.coverage
        FROM inventory_items
        JOIN items ON items.id = inventory_items.item_id
        WHERE inventory_items.character_id = ?
        ORDER BY inventory_items.id
        "#,
    )
    .bind(character_id)
    .fetch_all(pool)
    .await?;

    let items = item_rows
        .into_iter()
        .map(|row| ConnectedPlayerItem {
            quantity: row.quantity.max(0) as u32,
            equipped: equip
                .as_ref()
                .and_then(|equip| equip.equipped_slot(row.inventory_item_id))
                .map(str::to_string),
            item: TacticalItem {
                id: row.item_id,
                weight: row.weight,
                slot: row.slot,
                kind: row.kind,
                accuracy: row.accuracy,
                block: row.block,
                dodge: row.dodge,
                coverage: row.coverage,
            },
        })
        .collect();

    Ok(ConnectedPlayer {
        character: TacticalCharacter {
            id: u64::try_from(character.id).context("negative character id")?,
            name: character.name,
        },
        items,
        skills,
        stats,
        attrs,
        limbs,
    })
}

fn character_is_allowed_for_mission(character: &Character, mission: &MissionRecord) -> bool {
    if mission.requester_character_id == Some(character.id) {
        return true;
    }

    match (&character.party_id, &mission.party_id) {
        (Some(character_party), Some(mission_party)) => character_party == mission_party,
        _ => false,
    }
}

pub async fn enter_tactical_mission(
    pool: &SqlitePool,
    mission_id: &str,
    character_id: i64,
) -> anyhow::Result<()> {
    let mission = get_mission(pool, mission_id)
        .await?
        .ok_or_else(|| anyhow!("Mission not found"))?;
    let character = get_character(pool, character_id)
        .await?
        .ok_or_else(|| anyhow!("Character not found"))?;

    if !character_is_allowed_for_mission(&character, &mission) {
        bail!("Character is not authorized for this mission");
    }
    if matches!(mission.status.as_str(), "ended" | "failed" | "cancelled") {
        bail!("Mission is not joinable");
    }

    sqlx::query(
        r#"
        UPDATE characters
        SET in_mission = 1,
            active_mission_id = ?,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = ?
        "#,
    )
    .bind(mission_id)
    .bind(character_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn leave_tactical_mission(
    pool: &SqlitePool,
    mission_id: &str,
    character_id: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE characters
        SET in_mission = 0,
            active_mission_id = NULL,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = ? AND active_mission_id = ?
        "#,
    )
    .bind(character_id)
    .bind(mission_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn commit_mission_result(
    pool: &SqlitePool,
    mission_id: &str,
    success: bool,
    xp_gained: i64,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    let mission = get_mission_tx(&mut tx, mission_id)
        .await?
        .ok_or_else(|| anyhow!("Mission not found"))?;

    if mission.result_committed || mission.status == "cancelled" {
        tx.commit().await?;
        return Ok(());
    }

    let active_character_ids: Vec<i64> = sqlx::query_scalar(
        r#"
        SELECT id
        FROM characters
        WHERE active_mission_id = ?
        "#,
    )
    .bind(mission_id)
    .fetch_all(&mut *tx)
    .await?;

    for character_id in active_character_ids {
        if xp_gained > 0 {
            add_character_rewards_tx(&mut tx, character_id, xp_gained, 0).await?;
        }
        if success {
            change_inventory_item_tx(&mut tx, character_id, "gold_coin", 10).await?;
            change_inventory_item_tx(&mut tx, character_id, "health_potion", 2).await?;
        }
    }

    sqlx::query(
        r#"
        UPDATE characters
        SET in_mission = 0,
            active_mission_id = NULL,
            updated_at = CURRENT_TIMESTAMP
        WHERE active_mission_id = ?
        "#,
    )
    .bind(mission_id)
    .execute(&mut *tx)
    .await?;

    if success {
        if let Some(quest_id) = &mission.quest_id {
            if let Err(error) = complete_quest_tx(&mut tx, quest_id).await {
                tracing::warn!("Mission {mission_id} could not complete quest {quest_id}: {error}");
            }
        }
    }

    sqlx::query(
        r#"
        UPDATE missions
        SET status = 'ended',
            success = ?,
            xp_gained = ?,
            result_committed = 1,
            ended_at = CURRENT_TIMESTAMP,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = ?
        "#,
    )
    .bind(success)
    .bind(xp_gained)
    .bind(mission_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

async fn get_character_for_update(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
) -> anyhow::Result<Character> {
    sqlx::query_as::<_, Character>(
        r#"
        SELECT id, name, xp, level, gold, current_settlement_id, party_id,
               active_mission_id, in_mission
        FROM characters
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| anyhow!("Character not found"))
}

async fn update_character_party_tx(
    tx: &mut Transaction<'_, Sqlite>,
    character_id: i64,
    party_id: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE characters
        SET party_id = ?, updated_at = CURRENT_TIMESTAMP
        WHERE id = ?
        "#,
    )
    .bind(party_id)
    .bind(character_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn get_party_for_update(tx: &mut Transaction<'_, Sqlite>, id: &str) -> anyhow::Result<Party> {
    sqlx::query_as::<_, Party>(
        r#"
        SELECT id, name, leader_id, current_settlement_id, active_quest_id
        FROM parties
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| anyhow!("Party not found"))
}

async fn get_quest_for_update(tx: &mut Transaction<'_, Sqlite>, id: &str) -> anyhow::Result<Quest> {
    sqlx::query_as::<_, Quest>(
        r#"
        SELECT id, title, description, difficulty, gold_reward, xp_reward,
               settlement_id, status, accepted_by, enemy_type, enemy_count
        FROM quests
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| anyhow!("Quest not found"))
}

async fn get_settlement_tx(
    tx: &mut Transaction<'_, Sqlite>,
    id: &str,
) -> anyhow::Result<Option<Settlement>> {
    sqlx::query_as::<_, Settlement>(
        r#"
        SELECT id, name, coord_x, coord_y, population_level, scene_key
        FROM settlements
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(Into::into)
}

async fn get_mission_tx(
    tx: &mut Transaction<'_, Sqlite>,
    mission_id: &str,
) -> anyhow::Result<Option<MissionRecord>> {
    sqlx::query_as::<_, MissionRecord>(
        r#"
        SELECT id, scene_key, status, party_id, quest_id, requester_character_id,
               addr, cert_digest, pid, success, xp_gained, result_committed
        FROM missions
        WHERE id = ?
        "#,
    )
    .bind(mission_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(Into::into)
}
