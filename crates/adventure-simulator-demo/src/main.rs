use std::time::{Duration, Instant};

use bevy::prelude::*;
use clap::Parser;
use strategic_core::{Character, CharacterLifeState, InventoryItem, LootBag, Quest, RewardGrant};

#[derive(Parser, Debug, Clone)]
#[command(version, about)]
#[derive(Resource)]
struct DemoConfig {
    /// Strategic server base URL (strategic-server)
    #[arg(long, env = "STRATEGIC_URL", default_value = "http://127.0.0.1:8080")]
    strategic_url: String,

    /// Character id in Postgres
    #[arg(long, default_value = "demo")]
    character_id: String,
}

fn main() {
    let config = DemoConfig::parse();

    App::new()
        .insert_resource(config)
        .insert_resource(StrategicClient::default())
        .insert_resource(LocalCharacterState::default())
        .insert_resource(LocalInventoryState::default())
        .insert_resource(LocalLootState::default())
        .insert_resource(PollTick::default())
        .insert_resource(DamageTick::default())
        .insert_resource(QuestLocalState::default())
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, (setup_scene, bootstrap_strategic))
        .add_systems(
            Update,
            (
                player_movement,
                bot_follow_and_damage,
                interactions,
                poll_strategic_state,
                sync_loot_markers,
                respawn_loop,
                sync_ui_text,
            ),
        )
        .run();
}

#[derive(Resource, Default)]
struct StrategicClient {
    http: reqwest::blocking::Client,
}

#[derive(Resource, Default, Debug)]
struct LocalCharacterState {
    character: Option<Character>,
    last_error: Option<String>,
}

#[derive(Resource, Default, Debug)]
struct LocalInventoryState {
    items: Vec<InventoryItem>,
}

#[derive(Resource, Default, Debug)]
struct LocalLootState {
    bags: Vec<LootBag>,
}

#[derive(Resource, Default)]
struct QuestLocalState {
    pet_cat_status: String,
    last_refresh: Option<Instant>,
}

#[derive(Resource, Default)]
struct PollTick {
    last_poll: Option<Instant>,
}

#[derive(Resource, Default)]
struct DamageTick {
    last_tick: Option<Instant>,
}

#[derive(Component)]
struct Player;

#[derive(Component)]
struct HazardBot;

#[derive(Component)]
struct Cat;

#[derive(Component)]
struct QuestGiver;

#[derive(Component)]
struct LootBagMarker {
    id: String,
}

#[derive(Component)]
struct PickupItem {
    item_id: String,
    qty: i32,
}

#[derive(Component)]
struct HudText;

fn setup_scene(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 8.0, 14.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(AmbientLight {
        brightness: 1800.0,
        ..default()
    });

    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            illuminance: 12000.0,
            ..default()
        },
        Transform::from_xyz(6.0, 12.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Ground
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(30.0, 0.1, 30.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.14, 0.12).into(),
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    // Player
    commands.spawn((
        Player,
        Mesh3d(meshes.add(Capsule3d::new(0.35, 0.9))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.6, 1.0).into(),
            ..default()
        })),
        Transform::from_xyz(0.0, 0.9, 0.0),
    ));

    // Cat (quest objective)
    commands.spawn((
        Cat,
        Mesh3d(meshes.add(Sphere::new(0.55))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.95, 0.8, 0.25).into(),
            ..default()
        })),
        Transform::from_xyz(5.0, 0.55, 0.0),
    ));

    // Quest giver cube
    commands.spawn((
        QuestGiver,
        Mesh3d(meshes.add(Cuboid::new(0.9, 0.9, 0.9))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.9, 0.2, 0.9).into(),
            ..default()
        })),
        Transform::from_xyz(2.5, 0.45, -1.5),
    ));

    // Hazard bot sphere
    commands.spawn((
        HazardBot,
        Mesh3d(meshes.add(Sphere::new(0.6))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.9, 0.2, 0.2).into(),
            ..default()
        })),
        Transform::from_xyz(-6.0, 0.6, -3.0),
    ));

    // Quick pickup item (demonstrates inventory writes)
    commands.spawn((
        PickupItem {
            item_id: "healing_herb".to_string(),
            qty: 1,
        },
        Mesh3d(meshes.add(Cuboid::new(0.35, 0.35, 0.35))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.9, 0.35).into(),
            ..default()
        })),
        Transform::from_xyz(-1.8, 0.18, 2.2),
    ));

    // HUD (Bevy 0.17 UI text is a component)
    commands.spawn((
        HudText,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(14.0),
            top: Val::Px(14.0),
            ..default()
        },
        bevy::ui::widget::Text::new("Loading…"),
        bevy::text::TextFont {
            font_size: 18.0,
            ..default()
        },
        bevy::text::TextColor(Color::WHITE),
    ));
}

fn bootstrap_strategic(
    config: Res<DemoConfig>,
    client: Res<StrategicClient>,
    mut state: ResMut<LocalCharacterState>,
    mut inventory: ResMut<LocalInventoryState>,
    mut loot: ResMut<LocalLootState>,
    mut quest: ResMut<QuestLocalState>,
) {
    // Seed a demo quest + character via the strategic-server API (idempotent).
    let quest_def = Quest {
        id: "quest.pet_cat".to_string(),
        title: "Pet the Cat".to_string(),
        description: "Find the cat and pet it. Avoid the red hazard bot.".to_string(),
        status: strategic_core::QuestStatus::Available,
        reward: Some("50 XP, 3× silver_coin, 1× healing_potion".to_string()),
    };
    let _ = client
        .http
        .post(format!("{}/api/quests", config.as_ref().strategic_url))
        .json(&quest_def)
        .send();

    let character = Character {
        id: config.as_ref().character_id.clone(),
        name: "Demo Adventurer".to_string(),
        hp_current: 100,
        hp_max: 100,
        life: CharacterLifeState::Alive,
        deaths: 0,
        xp: 0,
        respawn_at_ms: None,
    };
    let _ = client
        .http
        .post(format!("{}/api/characters", config.as_ref().strategic_url))
        .json(&character)
        .send();
    let _ = client
        .http
        .post(format!(
            "{}/api/characters/{}/items/add",
            config.as_ref().strategic_url,
            config.as_ref().character_id
        ))
        .json(&serde_json::json!({"item_id":"apple","qty":2}))
        .send();
    let _ = client
        .http
        .post(format!(
            "{}/api/characters/{}/items/add",
            config.as_ref().strategic_url,
            config.as_ref().character_id
        ))
        .json(&serde_json::json!({"item_id":"copper_coin","qty":10}))
        .send();

    // First poll.
    match fetch_character(&client.http, &config.as_ref().strategic_url, &config.as_ref().character_id) {
        Ok(c) => {
            state.character = Some(c);
            state.last_error = None;
        }
        Err(err) => state.last_error = Some(err),
    }
    inventory.items = fetch_inventory(&client.http, &config.as_ref().strategic_url, &config.as_ref().character_id)
        .unwrap_or_default();
    loot.bags = fetch_loot_bags(&client.http, &config.as_ref().strategic_url, &config.as_ref().character_id)
        .unwrap_or_default();
    quest.pet_cat_status = fetch_quest_status(&client.http, &config.as_ref().strategic_url, &config.as_ref().character_id, "quest.pet_cat")
        .unwrap_or_else(|_| "unknown".to_string());
    quest.last_refresh = Some(Instant::now());
}

fn player_movement(keys: Res<ButtonInput<KeyCode>>, time: Res<Time>, mut query: Query<&mut Transform, With<Player>>, state: Res<LocalCharacterState>) {
    let Some(character) = &state.character else { return; };
    if character.life == CharacterLifeState::Dead {
        return;
    }

    let Ok(mut transform) = query.single_mut() else {
        return;
    };
    let mut dir = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        dir.z -= 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        dir.z += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        dir.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        dir.x += 1.0;
    }
    if dir.length_squared() > 0.0 {
        dir = dir.normalize();
    }

    let speed = 5.0;
    transform.translation += dir * speed * time.delta_secs();
    transform.translation.y = 0.9;
}

fn bot_follow_and_damage(
    time: Res<Time>,
    config: Res<DemoConfig>,
    client: Res<StrategicClient>,
    mut tick: ResMut<DamageTick>,
    mut state: ResMut<LocalCharacterState>,
    player_q: Query<&Transform, With<Player>>,
    mut bot_q: Query<&mut Transform, (With<HazardBot>, Without<Player>)>,
) {
    let Some(character) = &state.character else { return; };

    let Ok(player_t) = player_q.single() else {
        return;
    };
    let Ok(mut bot_t) = bot_q.single_mut() else {
        return;
    };

    // Simple follow logic
    let to_player = player_t.translation - bot_t.translation;
    let dist = to_player.length().max(0.001);
    let desired = to_player / dist;
    let speed = 2.2;
    bot_t.translation += desired * speed * time.delta_secs();
    bot_t.translation.y = 0.6;

    // Damage if close
    if character.life == CharacterLifeState::Dead {
        return;
    }
    if dist > 1.4 {
        return;
    }

    let now = Instant::now();
    let should_tick = tick
        .last_tick
        .map(|t| now.duration_since(t) >= Duration::from_millis(600))
        .unwrap_or(true);
    if !should_tick {
        return;
    }
    tick.last_tick = Some(now);

    let url = format!(
        "{}/api/characters/{}/damage",
        config.as_ref().strategic_url,
        config.as_ref().character_id
    );
    let p = player_t.translation;
    let body = serde_json::json!({ "amount": 10, "world_pos": [p.x, p.y, p.z] });
    match client.http.post(url).json(&body).send().and_then(|r| r.error_for_status()) {
        Ok(resp) => match resp.json::<Character>() {
            Ok(updated) => {
                state.character = Some(updated);
                state.last_error = None;
            }
            Err(err) => state.last_error = Some(format!("bad damage response: {err}")),
        },
        Err(err) => {
            state.last_error = Some(format!("damage call failed: {err}"));
        }
    }
}

fn interactions(
    keys: Res<ButtonInput<KeyCode>>,
    config: Res<DemoConfig>,
    client: Res<StrategicClient>,
    mut state: ResMut<LocalCharacterState>,
    mut quest: ResMut<QuestLocalState>,
    mut inventory: ResMut<LocalInventoryState>,
    player_q: Query<&Transform, With<Player>>,
    cat_q: Query<&Transform, With<Cat>>,
    giver_q: Query<&Transform, With<QuestGiver>>,
    pickups_q: Query<(Entity, &Transform, &PickupItem)>,
    loot_q: Query<(Entity, &Transform, &LootBagMarker)>,
    mut commands: Commands,
) {
    if !keys.just_pressed(KeyCode::KeyE) {
        return;
    }
    let Some(character) = &state.character else { return; };
    if character.life == CharacterLifeState::Dead {
        return;
    }

    let Ok(player) = player_q.single().map(|t| t.translation) else {
        return;
    };
    let Ok(cat) = cat_q.single().map(|t| t.translation) else {
        return;
    };
    let Ok(giver) = giver_q.single().map(|t| t.translation) else {
        return;
    };

    let interact_radius = 2.0;

    for (entity, t, pickup) in pickups_q.iter() {
        if player.distance(t.translation) <= interact_radius {
            let url = format!(
                "{}/api/characters/{}/items/add",
                config.as_ref().strategic_url,
                config.as_ref().character_id
            );
            let body = serde_json::json!({ "item_id": pickup.item_id.clone(), "qty": pickup.qty });
            if let Err(err) = client.http.post(url).json(&body).send().and_then(|r| r.error_for_status()) {
                state.last_error = Some(format!("pickup failed: {err}"));
                return;
            }
            commands.entity(entity).despawn();
            inventory.items = fetch_inventory(&client.http, &config.as_ref().strategic_url, &config.as_ref().character_id)
                .unwrap_or_default();
            state.last_error = None;
            return;
        }
    }

    for (entity, t, bag) in loot_q.iter() {
        if player.distance(t.translation) <= interact_radius {
            let url = format!("{}/api/loot_bags/{}/claim", config.as_ref().strategic_url, bag.id);
            let body = serde_json::json!({ "character_id": config.as_ref().character_id });
            if let Err(err) = client.http.post(url).json(&body).send().and_then(|r| r.error_for_status()) {
                state.last_error = Some(format!("claim failed: {err}"));
                return;
            }
            commands.entity(entity).despawn();
            inventory.items = fetch_inventory(&client.http, &config.as_ref().strategic_url, &config.as_ref().character_id)
                .unwrap_or_default();
            state.last_error = None;
            return;
        }
    }

    if player.distance(giver) <= interact_radius {
        let url = format!(
            "{}/api/characters/{}/quests/{}/start",
            config.as_ref().strategic_url,
            config.as_ref().character_id,
            "quest.pet_cat"
        );
        if let Err(err) = client.http.post(url).send().and_then(|r| r.error_for_status()) {
            state.last_error = Some(format!("accept failed: {err}"));
            return;
        }
        quest.pet_cat_status = "active".to_string();
        quest.last_refresh = Some(Instant::now());
        state.last_error = None;
        return;
    }

    if player.distance(cat) <= interact_radius {
        let url = format!(
            "{}/api/characters/{}/quests/{}/complete",
            config.as_ref().strategic_url,
            config.as_ref().character_id,
            "quest.pet_cat"
        );
        let rewards = RewardGrant {
            xp: 50,
            items: vec![
                InventoryItem {
                    item_id: "silver_coin".to_string(),
                    qty: 3,
                },
                InventoryItem {
                    item_id: "healing_potion".to_string(),
                    qty: 1,
                },
            ],
        };
        if let Err(err) = client.http.post(url).json(&rewards).send().and_then(|r| r.error_for_status()) {
            state.last_error = Some(format!("complete failed: {err}"));
            return;
        }
        quest.pet_cat_status = "completed".to_string();
        quest.last_refresh = Some(Instant::now());
        inventory.items = fetch_inventory(&client.http, &config.as_ref().strategic_url, &config.as_ref().character_id)
            .unwrap_or_default();
        state.last_error = None;
    }
}

fn poll_strategic_state(
    config: Res<DemoConfig>,
    client: Res<StrategicClient>,
    mut tick: ResMut<PollTick>,
    mut state: ResMut<LocalCharacterState>,
    mut inv: ResMut<LocalInventoryState>,
    mut loot: ResMut<LocalLootState>,
    mut quest: ResMut<QuestLocalState>,
) {
    let now = Instant::now();
    let should_poll = tick
        .last_poll
        .map(|t| now.duration_since(t) >= Duration::from_millis(750))
        .unwrap_or(true);
    if !should_poll {
        return;
    }
    tick.last_poll = Some(now);

    if let Ok(c) = fetch_character(&client.http, &config.as_ref().strategic_url, &config.as_ref().character_id) {
        state.character = Some(c);
        state.last_error = None;
    }
    if let Ok(items) = fetch_inventory(&client.http, &config.as_ref().strategic_url, &config.as_ref().character_id) {
        inv.items = items;
    }
    if let Ok(bags) = fetch_loot_bags(&client.http, &config.as_ref().strategic_url, &config.as_ref().character_id) {
        loot.bags = bags;
    }
    if let Ok(status) = fetch_quest_status(&client.http, &config.as_ref().strategic_url, &config.as_ref().character_id, "quest.pet_cat") {
        quest.pet_cat_status = status;
        quest.last_refresh = Some(now);
    }
}

fn sync_loot_markers(
    mut commands: Commands,
    loot: Res<LocalLootState>,
    existing: Query<(Entity, &LootBagMarker)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    use std::collections::{HashMap, HashSet};

    let mut existing_map: HashMap<&str, Entity> = HashMap::new();
    for (e, m) in existing.iter() {
        existing_map.insert(m.id.as_str(), e);
    }

    let mut keep: HashSet<&str> = HashSet::new();
    for bag in &loot.bags {
        keep.insert(bag.id.as_str());
        if existing_map.contains_key(bag.id.as_str()) {
            continue;
        }
        let Some(pos) = bag.world_pos else { continue; };
        commands.spawn((
            LootBagMarker { id: bag.id.clone() },
            Mesh3d(meshes.add(Sphere::new(0.35))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgb(0.65, 0.45, 0.22).into(),
                emissive: Color::srgb(0.12, 0.06, 0.01).into(),
                ..default()
            })),
            Transform::from_xyz(pos[0], 0.35, pos[2]),
        ));
    }

    for (e, m) in existing.iter() {
        if !keep.contains(m.id.as_str()) {
            commands.entity(e).despawn();
        }
    }
}

fn respawn_loop(
    config: Res<DemoConfig>,
    client: Res<StrategicClient>,
    mut state: ResMut<LocalCharacterState>,
    mut player_q: Query<&mut Transform, With<Player>>,
) {
    let Some(character) = &state.character else { return; };
    if character.life == CharacterLifeState::Alive {
        return;
    }

    // Try to respawn once the timer is up.
    let now_ms = current_ms();
    if let Some(at) = character.respawn_at_ms {
        if now_ms < at {
            return;
        }
    }

    let url = format!(
        "{}/api/characters/{}/respawn",
        config.as_ref().strategic_url,
        config.as_ref().character_id
    );
    match client.http.post(url).send().and_then(|r| r.error_for_status()) {
        Ok(resp) => match resp.json::<Character>() {
            Ok(updated) => {
                let alive_now = updated.life == CharacterLifeState::Alive;
                state.character = Some(updated);
                state.last_error = None;
                if alive_now {
                    let Ok(mut player_t) = player_q.single_mut() else {
                        return;
                    };
                    player_t.translation = Vec3::new(0.0, 0.9, 0.0);
                }
            }
            Err(err) => state.last_error = Some(format!("bad respawn response: {err}")),
        },
        Err(err) => state.last_error = Some(format!("respawn call failed: {err}")),
    }
}

fn sync_ui_text(
    mut hud: Query<&mut bevy::ui::widget::Text, With<HudText>>,
    state: Res<LocalCharacterState>,
    inventory: Res<LocalInventoryState>,
    loot: Res<LocalLootState>,
    quest: Res<QuestLocalState>,
) {
    let Ok(mut text) = hud.single_mut() else {
        return;
    };
    let (headline, details) = if let Some(character) = &state.character {
        let life = match character.life {
            CharacterLifeState::Alive => "ALIVE",
            CharacterLifeState::Dead => "DEAD",
        };
        let respawn = if character.life == CharacterLifeState::Dead {
            character
                .respawn_at_ms
                .map(|ms| {
                    let remain = (ms - current_ms()).max(0) as f32 / 1000.0;
                    format!(" (respawn in {:.1}s)", remain)
                })
                .unwrap_or_default()
        } else {
            "".to_string()
        };
        (
            format!(
                "{} · HP {}/{} · {}{}\n",
                character.name, character.hp_current, character.hp_max, life, respawn
            ),
            format!(
                "Quest quest.pet_cat: {}\nInventory items: {} · Loot bags: {}\nWASD move · E interact\n{}",
                quest.pet_cat_status,
                inventory.items.len(),
                loot.bags.len(),
                state
                    .last_error
                    .as_deref()
                    .map(|e| format!("Error: {e}"))
                    .unwrap_or_else(|| "Strategic: OK".to_string())
            ),
        )
    } else {
        (
            "No character loaded\n".to_string(),
            state
                .last_error
                .as_deref()
                .map(|e| format!("Error: {e}"))
                .unwrap_or_else(|| "Waiting…".to_string()),
        )
    };

    text.0 = format!("{headline}{details}");
}

fn fetch_character(
    http: &reqwest::blocking::Client,
    base: &str,
    character_id: &str,
) -> Result<Character, String> {
    let url = format!("{}/api/characters/{}", base, character_id);
    let resp = http.get(url).send().map_err(|e| e.to_string())?;
    let resp = resp.error_for_status().map_err(|e| e.to_string())?;
    resp.json::<Character>().map_err(|e| e.to_string())
}

fn fetch_quest_status(
    http: &reqwest::blocking::Client,
    base: &str,
    character_id: &str,
    quest_id: &str,
) -> Result<String, String> {
    let url = format!("{}/api/characters/{}/quests/{}", base, character_id, quest_id);
    let resp = http.get(url).send().map_err(|e| e.to_string())?;
    let resp = resp.error_for_status().map_err(|e| e.to_string())?;
    resp.json::<String>().map_err(|e| e.to_string())
}

fn fetch_inventory(
    http: &reqwest::blocking::Client,
    base: &str,
    character_id: &str,
) -> Result<Vec<InventoryItem>, String> {
    let url = format!("{}/api/characters/{}/inventory", base, character_id);
    let resp = http.get(url).send().map_err(|e| e.to_string())?;
    let resp = resp.error_for_status().map_err(|e| e.to_string())?;
    resp.json::<Vec<InventoryItem>>().map_err(|e| e.to_string())
}

fn fetch_loot_bags(
    http: &reqwest::blocking::Client,
    base: &str,
    character_id: &str,
) -> Result<Vec<LootBag>, String> {
    let url = format!("{}/api/characters/{}/loot_bags", base, character_id);
    let resp = http.get(url).send().map_err(|e| e.to_string())?;
    let resp = resp.error_for_status().map_err(|e| e.to_string())?;
    resp.json::<Vec<LootBag>>().map_err(|e| e.to_string())
}

fn current_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    dur.as_millis() as i64
}
