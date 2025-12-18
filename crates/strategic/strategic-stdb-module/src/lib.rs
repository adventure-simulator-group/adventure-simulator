use spacetimedb::{reducer, table, Identity, ReducerContext, ScheduleAt, Table, TimeDuration};

const WORLD_TICK_MICROS: i64 = 50_000; // 20 Hz
const PLAYER_SPEED: f32 = 5.0;
const HAZARD_SPEED: f32 = 2.2;
const INTERACT_RADIUS: f32 = 2.0;
const HAZARD_DAMAGE: i32 = 10;
const HAZARD_DAMAGE_COOLDOWN_MICROS: i64 = 600_000;
const RESPAWN_DELAY_MICROS: i64 = 5_000_000;

const QUEST_PET_CAT: &str = "quest.pet_cat";

#[derive(Clone, Debug)]
#[table(name = player, public)]
pub struct Player {
    #[primary_key]
    pub identity: Identity,
    pub character_id: String,
    pub display_name: String,
}

#[derive(Clone, Debug)]
#[table(name = character, public)]
pub struct Character {
    #[primary_key]
    pub identity: Identity,
    pub name: String,
    pub hp_current: i32,
    pub hp_max: i32,
    pub alive: bool,
    pub deaths: i32,
    pub xp: i32,
    /// 0 means "no respawn scheduled".
    pub respawn_at_micros: i64,
    /// Used to rate-limit hazard damage.
    pub last_damage_at_micros: i64,
}

#[derive(Clone, Debug)]
#[table(name = player_input, public)]
pub struct PlayerInput {
    #[primary_key]
    pub identity: Identity,
    pub dx: f32,
    pub dz: f32,
}

#[derive(Clone, Debug)]
#[table(name = player_transform, public)]
pub struct PlayerTransform {
    #[primary_key]
    pub identity: Identity,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Clone, Debug)]
#[table(name = hazard_bot, public)]
pub struct HazardBot {
    #[primary_key]
    pub id: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Clone, Debug)]
#[table(name = static_entity, public)]
pub struct StaticEntity {
    #[primary_key]
    pub id: u64,
    pub kind: String,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Clone, Debug)]
#[table(name = pickup_item, public)]
pub struct PickupItem {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub item_id: String,
    pub qty: i32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Clone, Debug)]
#[table(name = quest_def, public)]
pub struct QuestDef {
    #[primary_key]
    pub quest_id: String,
    pub title: String,
    pub description: String,
    pub reward_text: String,
}

#[derive(Clone, Debug)]
#[table(name = character_quest, public)]
pub struct CharacterQuest {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub owner: Identity,
    pub quest_id: String,
    pub status: String,
    pub updated_at_micros: i64,
}

#[derive(Clone, Debug)]
#[table(name = inventory_item, public)]
pub struct InventoryItem {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub owner: Identity,
    pub item_id: String,
    pub qty: i32,
}

#[derive(Clone, Debug)]
#[table(name = loot_bag, public)]
pub struct LootBag {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub owner: Identity,
    pub created_at_micros: i64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Clone, Debug)]
#[table(name = loot_bag_item, public)]
pub struct LootBagItem {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub bag_id: u64,
    pub item_id: String,
    pub qty: i32,
}

// Scheduled reducer boilerplate.
#[derive(Clone, Debug)]
#[table(name = world_tick_schedule, scheduled(world_tick))]
pub struct WorldTickSchedule {
    #[primary_key]
    pub scheduled_id: u64,
    pub scheduled_at: ScheduleAt,
}

#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    seed_static_world(ctx);
    start_world_tick(ctx);
}

fn seed_static_world(ctx: &ReducerContext) {
    // Static entities: id 1 = quest giver, id 2 = cat.
    if ctx.db.static_entity().id().find(&1).is_none() {
        ctx.db.static_entity().insert(StaticEntity {
            id: 1,
            kind: "quest_giver".to_string(),
            x: 2.5,
            y: 0.45,
            z: -1.5,
        });
    }
    if ctx.db.static_entity().id().find(&2).is_none() {
        ctx.db.static_entity().insert(StaticEntity {
            id: 2,
            kind: "cat".to_string(),
            x: 5.0,
            y: 0.55,
            z: 0.0,
        });
    }

    // One hazard bot: id 1.
    if ctx.db.hazard_bot().id().find(&1).is_none() {
        ctx.db.hazard_bot().insert(HazardBot {
            id: 1,
            x: -6.0,
            y: 0.6,
            z: -3.0,
        });
    }

    // A single pickup item (like the old demo).
    // Note: `init` runs once per new database; this will not duplicate unless you clear the DB.
    let _ = ctx.db.pickup_item().insert(PickupItem {
        id: 0,
        item_id: "healing_herb".to_string(),
        qty: 1,
        x: -1.8,
        y: 0.18,
        z: 2.2,
    });

    // Quest definition.
    if ctx
        .db
        .quest_def()
        .quest_id()
        .find(&QUEST_PET_CAT.to_string())
        .is_none()
    {
        ctx.db.quest_def().insert(QuestDef {
            quest_id: QUEST_PET_CAT.to_string(),
            title: "Pet the Cat".to_string(),
            description: "Find the cat and pet it. Avoid the red hazard bot.".to_string(),
            reward_text: "50 XP, 3× silver_coin, 1× healing_potion".to_string(),
        });
    }
}

fn start_world_tick(ctx: &ReducerContext) {
    let dt = TimeDuration::from_micros(WORLD_TICK_MICROS);
    ctx.db.world_tick_schedule().insert(WorldTickSchedule {
        scheduled_id: 0,
        scheduled_at: dt.into(),
    });
}

#[reducer]
pub fn join_world(
    ctx: &ReducerContext,
    character_id: String,
    display_name: String,
) -> Result<(), String> {
    let identity = ctx.sender;

    upsert_player(ctx, identity, character_id, display_name)?;
    ensure_character(ctx, identity)?;
    ensure_transform(ctx, identity)?;
    ensure_input(ctx, identity)?;
    ensure_quest_row(ctx, identity, QUEST_PET_CAT)?;

    Ok(())
}

#[reducer]
pub fn set_input(ctx: &ReducerContext, dx: f32, dz: f32) -> Result<(), String> {
    let identity = ctx.sender;
    if !is_alive(ctx, identity) {
        return Ok(());
    }
    let Some(mut input) = ctx.db.player_input().identity().find(&identity) else {
        return Err("not joined (no player_input row)".to_string());
    };
    input.dx = dx;
    input.dz = dz;
    ctx.db.player_input().identity().update(input);
    Ok(())
}

#[reducer]
pub fn interact(ctx: &ReducerContext) -> Result<(), String> {
    let identity = ctx.sender;
    if !is_alive(ctx, identity) {
        return Ok(());
    }
    let Some(pos) = ctx.db.player_transform().identity().find(&identity) else {
        return Err("not joined (no player_transform row)".to_string());
    };

    // 1) Pickups
    for pickup in ctx.db.pickup_item().iter() {
        if dist3(pos.x, pos.y, pos.z, pickup.x, pickup.y, pickup.z) <= INTERACT_RADIUS {
            add_item(ctx, identity, &pickup.item_id, pickup.qty)?;
            ctx.db.pickup_item().id().delete(pickup.id);
            return Ok(());
        }
    }

    // 2) Loot bags (owned)
    for bag in ctx.db.loot_bag().owner().filter(&identity) {
        if dist3(pos.x, pos.y, pos.z, bag.x, bag.y, bag.z) <= INTERACT_RADIUS {
            claim_loot_bag(ctx, identity, bag.id)?;
            return Ok(());
        }
    }

    // 3) Quest giver
    if let Some(giver) = ctx.db.static_entity().id().find(&1) {
        if dist3(pos.x, pos.y, pos.z, giver.x, giver.y, giver.z) <= INTERACT_RADIUS {
            set_quest_status(ctx, identity, QUEST_PET_CAT, "active")?;
            return Ok(());
        }
    }

    // 4) Cat
    if let Some(cat) = ctx.db.static_entity().id().find(&2) {
        if dist3(pos.x, pos.y, pos.z, cat.x, cat.y, cat.z) <= INTERACT_RADIUS {
            let status =
                get_quest_status(ctx, identity, QUEST_PET_CAT).unwrap_or("not-started".to_string());
            if status == "active" {
                complete_pet_cat(ctx, identity)?;
            }
            return Ok(());
        }
    }

    Ok(())
}

#[reducer]
pub fn respawn(ctx: &ReducerContext) -> Result<(), String> {
    let identity = ctx.sender;
    let now_micros = now_micros(ctx);

    let Some(mut character) = ctx.db.character().identity().find(&identity) else {
        return Err("not joined (no character row)".to_string());
    };
    if character.alive {
        return Ok(());
    }
    if character.respawn_at_micros != 0 && now_micros < character.respawn_at_micros {
        return Ok(());
    }

    character.alive = true;
    character.hp_current = character.hp_max;
    character.respawn_at_micros = 0;
    character.last_damage_at_micros = 0;
    ctx.db.character().identity().update(character);

    // Starter item on respawn.
    add_item(ctx, identity, "bandage", 1)?;
    Ok(())
}

#[reducer]
pub fn world_tick(ctx: &ReducerContext, _sched: WorldTickSchedule) -> Result<(), String> {
    // Prevent clients from calling our "game loop" reducer manually.
    if ctx.sender != ctx.identity() {
        return Err("world_tick may only be invoked via scheduling".into());
    }

    tick_players(ctx)?;
    tick_hazard_bot(ctx)?;

    Ok(())
}

fn tick_players(ctx: &ReducerContext) -> Result<(), String> {
    let dt = WORLD_TICK_MICROS as f32 / 1_000_000.0;

    // Collect identities first to avoid borrow issues across table updates.
    let identities = ctx
        .db
        .player()
        .iter()
        .map(|p| p.identity)
        .collect::<Vec<_>>();

    for identity in identities {
        if !is_alive(ctx, identity) {
            continue;
        }

        let Some(mut t) = ctx.db.player_transform().identity().find(&identity) else {
            continue;
        };
        let input = ctx
            .db
            .player_input()
            .identity()
            .find(&identity)
            .unwrap_or(PlayerInput {
                identity,
                dx: 0.0,
                dz: 0.0,
            });
        let (dir_x, dir_z) = normalize2(input.dx, input.dz);
        t.x += dir_x * PLAYER_SPEED * dt;
        t.z += dir_z * PLAYER_SPEED * dt;
        t.y = 0.9;
        ctx.db.player_transform().identity().update(t);
    }
    Ok(())
}

fn tick_hazard_bot(ctx: &ReducerContext) -> Result<(), String> {
    let dt = WORLD_TICK_MICROS as f32 / 1_000_000.0;
    let now_micros = now_micros(ctx);

    let Some(mut bot) = ctx.db.hazard_bot().id().find(&1) else {
        return Ok(());
    };

    // Find nearest alive player.
    let mut nearest: Option<(Identity, PlayerTransform, f32)> = None;
    for t in ctx.db.player_transform().iter() {
        if !is_alive(ctx, t.identity) {
            continue;
        }
        let d = dist3(bot.x, bot.y, bot.z, t.x, t.y, t.z);
        match nearest {
            None => nearest = Some((t.identity, t, d)),
            Some((_id, _t, best)) if d < best => nearest = Some((t.identity, t, d)),
            _ => {}
        }
    }

    if let Some((_id, player_t, dist)) = nearest {
        let (dx, dz) = normalize2(player_t.x - bot.x, player_t.z - bot.z);
        bot.x += dx * HAZARD_SPEED * dt;
        bot.z += dz * HAZARD_SPEED * dt;
        bot.y = 0.6;
        ctx.db.hazard_bot().id().update(bot);

        // Damage if close (cooldown per character).
        if dist <= 1.4 {
            apply_hazard_damage(ctx, player_t.identity, now_micros)?;
        }
    } else {
        // Still update y to keep consistent.
        bot.y = 0.6;
        ctx.db.hazard_bot().id().update(bot);
    }

    Ok(())
}

fn apply_hazard_damage(
    ctx: &ReducerContext,
    victim: Identity,
    now_micros: i64,
) -> Result<(), String> {
    let Some(mut character) = ctx.db.character().identity().find(&victim) else {
        return Ok(());
    };
    if !character.alive {
        return Ok(());
    }
    if character.last_damage_at_micros != 0
        && now_micros - character.last_damage_at_micros < HAZARD_DAMAGE_COOLDOWN_MICROS
    {
        return Ok(());
    }
    character.last_damage_at_micros = now_micros;

    let new_hp = (character.hp_current - HAZARD_DAMAGE).max(0);
    if new_hp > 0 {
        character.hp_current = new_hp;
        ctx.db.character().identity().update(character);
        return Ok(());
    }

    // Death.
    character.hp_current = 0;
    character.alive = false;
    character.deaths += 1;
    character.respawn_at_micros = now_micros + RESPAWN_DELAY_MICROS;
    ctx.db.character().identity().update(character);

    drop_inventory_as_loot(ctx, victim, now_micros)?;
    Ok(())
}

fn drop_inventory_as_loot(
    ctx: &ReducerContext,
    victim: Identity,
    now_micros: i64,
) -> Result<(), String> {
    let pos = ctx.db.player_transform().identity().find(&victim);
    let (x, y, z) = pos.map(|p| (p.x, p.y, p.z)).unwrap_or((0.0, 0.9, 0.0));

    let bag = ctx.db.loot_bag().insert(LootBag {
        id: 0,
        owner: victim,
        created_at_micros: now_micros,
        x,
        y,
        z,
    });

    // Move inventory → loot items.
    let inv_rows = ctx
        .db
        .inventory_item()
        .owner()
        .filter(&victim)
        .collect::<Vec<_>>();
    for it in inv_rows {
        if it.qty <= 0 {
            ctx.db.inventory_item().id().delete(it.id);
            continue;
        }
        ctx.db.loot_bag_item().insert(LootBagItem {
            id: 0,
            bag_id: bag.id,
            item_id: it.item_id.clone(),
            qty: it.qty,
        });
        ctx.db.inventory_item().id().delete(it.id);
    }

    Ok(())
}

fn claim_loot_bag(ctx: &ReducerContext, owner: Identity, bag_id: u64) -> Result<(), String> {
    if !is_alive(ctx, owner) {
        return Ok(());
    }

    let Some(bag) = ctx.db.loot_bag().id().find(&bag_id) else {
        return Ok(());
    };
    if bag.owner != owner {
        return Ok(());
    }

    let items = ctx
        .db
        .loot_bag_item()
        .bag_id()
        .filter(&bag_id)
        .collect::<Vec<_>>();
    for it in items {
        if it.qty > 0 {
            add_item(ctx, owner, &it.item_id, it.qty)?;
        }
        ctx.db.loot_bag_item().id().delete(it.id);
    }

    ctx.db.loot_bag().id().delete(bag_id);
    Ok(())
}

fn complete_pet_cat(ctx: &ReducerContext, identity: Identity) -> Result<(), String> {
    set_quest_status(ctx, identity, QUEST_PET_CAT, "completed")?;
    add_xp(ctx, identity, 50)?;
    add_item(ctx, identity, "silver_coin", 3)?;
    add_item(ctx, identity, "healing_potion", 1)?;
    Ok(())
}

fn upsert_player(
    ctx: &ReducerContext,
    identity: Identity,
    character_id: String,
    display_name: String,
) -> Result<(), String> {
    let existing = ctx.db.player().identity().find(&identity);
    match existing {
        Some(mut player) => {
            player.character_id = character_id;
            player.display_name = display_name;
            ctx.db.player().identity().update(player);
        }
        None => {
            ctx.db.player().insert(Player {
                identity,
                character_id,
                display_name,
            });
        }
    }
    Ok(())
}

fn ensure_character(ctx: &ReducerContext, identity: Identity) -> Result<(), String> {
    if ctx.db.character().identity().find(&identity).is_some() {
        return Ok(());
    }
    ctx.db.character().insert(Character {
        identity,
        name: "Demo Adventurer".to_string(),
        hp_current: 100,
        hp_max: 100,
        alive: true,
        deaths: 0,
        xp: 0,
        respawn_at_micros: 0,
        last_damage_at_micros: 0,
    });

    // Starter items.
    add_item(ctx, identity, "apple", 2)?;
    add_item(ctx, identity, "copper_coin", 10)?;

    Ok(())
}

fn ensure_transform(ctx: &ReducerContext, identity: Identity) -> Result<(), String> {
    if ctx
        .db
        .player_transform()
        .identity()
        .find(&identity)
        .is_some()
    {
        return Ok(());
    }
    ctx.db.player_transform().insert(PlayerTransform {
        identity,
        x: 0.0,
        y: 0.9,
        z: 0.0,
    });
    Ok(())
}

fn ensure_input(ctx: &ReducerContext, identity: Identity) -> Result<(), String> {
    if ctx.db.player_input().identity().find(&identity).is_some() {
        return Ok(());
    }
    ctx.db.player_input().insert(PlayerInput {
        identity,
        dx: 0.0,
        dz: 0.0,
    });
    Ok(())
}

fn ensure_quest_row(ctx: &ReducerContext, owner: Identity, quest_id: &str) -> Result<(), String> {
    let existing = ctx
        .db
        .character_quest()
        .owner()
        .filter(&owner)
        .any(|q| q.quest_id == quest_id);
    if existing {
        return Ok(());
    }
    ctx.db.character_quest().insert(CharacterQuest {
        id: 0,
        owner,
        quest_id: quest_id.to_string(),
        status: "not-started".to_string(),
        updated_at_micros: now_micros(ctx),
    });
    Ok(())
}

fn get_quest_status(ctx: &ReducerContext, owner: Identity, quest_id: &str) -> Option<String> {
    ctx.db
        .character_quest()
        .owner()
        .filter(&owner)
        .find(|q| q.quest_id == quest_id)
        .map(|q| q.status)
}

fn set_quest_status(
    ctx: &ReducerContext,
    owner: Identity,
    quest_id: &str,
    status: &str,
) -> Result<(), String> {
    // Enforce single row per (owner, quest_id).
    let rows = ctx
        .db
        .character_quest()
        .owner()
        .filter(&owner)
        .filter(|q| q.quest_id == quest_id)
        .collect::<Vec<_>>();

    let now = now_micros(ctx);
    match rows.as_slice() {
        [] => {
            ctx.db.character_quest().insert(CharacterQuest {
                id: 0,
                owner,
                quest_id: quest_id.to_string(),
                status: status.to_string(),
                updated_at_micros: now,
            });
        }
        [first, rest @ ..] => {
            for extra in rest {
                ctx.db.character_quest().id().delete(extra.id);
            }
            let mut updated = first.clone();
            updated.status = status.to_string();
            updated.updated_at_micros = now;
            ctx.db.character_quest().id().update(updated);
        }
    }
    Ok(())
}

fn add_item(ctx: &ReducerContext, owner: Identity, item_id: &str, qty: i32) -> Result<(), String> {
    if qty <= 0 {
        return Ok(());
    }

    let existing = ctx
        .db
        .inventory_item()
        .owner()
        .filter(&owner)
        .find(|row| row.item_id == item_id);

    match existing {
        Some(row) => {
            let mut updated = row.clone();
            updated.qty = updated.qty.saturating_add(qty);
            ctx.db.inventory_item().id().update(updated);
        }
        None => {
            ctx.db.inventory_item().insert(InventoryItem {
                id: 0,
                owner,
                item_id: item_id.to_string(),
                qty,
            });
        }
    }
    Ok(())
}

fn add_xp(ctx: &ReducerContext, identity: Identity, xp: i32) -> Result<(), String> {
    if xp <= 0 {
        return Ok(());
    }
    let Some(mut c) = ctx.db.character().identity().find(&identity) else {
        return Ok(());
    };
    if !c.alive {
        return Ok(());
    }
    c.xp = c.xp.saturating_add(xp);
    ctx.db.character().identity().update(c);
    Ok(())
}

fn is_alive(ctx: &ReducerContext, identity: Identity) -> bool {
    ctx.db
        .character()
        .identity()
        .find(&identity)
        .map(|c| c.alive)
        .unwrap_or(false)
}

fn now_micros(ctx: &ReducerContext) -> i64 {
    ctx.timestamp.to_micros_since_unix_epoch()
}

fn dist3(ax: f32, ay: f32, az: f32, bx: f32, by: f32, bz: f32) -> f32 {
    let dx = ax - bx;
    let dy = ay - by;
    let dz = az - bz;
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn normalize2(x: f32, z: f32) -> (f32, f32) {
    let len2 = x * x + z * z;
    if len2 <= 1.0e-6 {
        return (0.0, 0.0);
    }
    let inv = 1.0 / len2.sqrt();
    (x * inv, z * inv)
}

// Note: This module is intended to be compiled to WASM and published to SpacetimeDB.
