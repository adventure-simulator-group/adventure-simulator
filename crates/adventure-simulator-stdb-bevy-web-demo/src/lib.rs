use bevy::prelude::*;
use serde::Deserialize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn receive_world_snapshot(snapshot_json: String) {
    WORLD_SNAPSHOT.with_borrow_mut(|s| {
        s.pending = Some(snapshot_json);
    });
}

#[wasm_bindgen]
pub fn set_connection_identity_hex(identity_hex: String) {
    WORLD_SNAPSHOT.with_borrow_mut(|s| {
        s.identity_hex = Some(identity_hex);
    });
}

thread_local! {
    static WORLD_SNAPSHOT: std::cell::RefCell<SnapshotBuffer> = const { std::cell::RefCell::new(SnapshotBuffer { pending: None, identity_hex: None }) };
}

struct SnapshotBuffer {
    pending: Option<String>,
    identity_hex: Option<String>,
}

#[derive(Resource, Default)]
struct SnapshotState {
    identity_hex: Option<String>,
    latest: Option<WorldSnapshot>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct WorldSnapshot {
    players: Vec<PlayerRow>,
    characters: Vec<CharacterRow>,
    transforms: Vec<PlayerTransformRow>,
    hazard_bots: Vec<HazardBotRow>,
    static_entities: Vec<StaticEntityRow>,
    pickups: Vec<PickupItemRow>,
    loot_bags: Vec<LootBagRow>,
}

#[derive(Debug, Clone, Deserialize)]
struct PlayerRow {
    identity_hex: String,
    display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CharacterRow {
    identity_hex: String,
    name: String,
    hp_current: i32,
    hp_max: i32,
    alive: bool,
    deaths: i32,
    xp: i32,
    respawn_at_micros: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PlayerTransformRow {
    identity_hex: String,
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Debug, Clone, Deserialize)]
struct HazardBotRow {
    id: String,
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Debug, Clone, Deserialize)]
struct StaticEntityRow {
    id: String,
    kind: String,
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Debug, Clone, Deserialize)]
struct PickupItemRow {
    id: String,
    item_id: String,
    qty: i32,
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Debug, Clone, Deserialize)]
struct LootBagRow {
    id: String,
    owner_identity_hex: String,
    x: f32,
    y: f32,
    z: f32,
}

#[derive(Resource, Default)]
struct LastSentInput {
    dx: f32,
    dz: f32,
}

#[derive(Resource, Default)]
struct WorldEntities {
    players: std::collections::HashMap<String, Entity>,
    pickups: std::collections::HashMap<String, Entity>,
    loot_bags: std::collections::HashMap<String, Entity>,
    statics: std::collections::HashMap<String, Entity>,
    hazard_bots: std::collections::HashMap<String, Entity>,
}

#[derive(Resource)]
struct SharedAssets {
    player_mesh: Handle<Mesh>,
    player_me_mat: Handle<StandardMaterial>,
    player_other_mat: Handle<StandardMaterial>,
    player_dead_mat: Handle<StandardMaterial>,
    hazard_mesh: Handle<Mesh>,
    hazard_mat: Handle<StandardMaterial>,
    giver_mesh: Handle<Mesh>,
    giver_mat: Handle<StandardMaterial>,
    cat_mesh: Handle<Mesh>,
    cat_mat: Handle<StandardMaterial>,
    pickup_mesh: Handle<Mesh>,
    pickup_mat: Handle<StandardMaterial>,
    loot_mesh: Handle<Mesh>,
    loot_mat: Handle<StandardMaterial>,
}

#[derive(Component)]
struct HudText;

#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(target_family = "wasm")]
    console_error_panic_hook::set_once();

    App::new()
        .add_plugins(
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    canvas: Some("#game-canvas".to_string()),
                    fit_canvas_to_parent: true,
                    prevent_default_event_handling: true,
                    ..default()
                }),
                ..default()
            }),
        )
        .insert_resource(SnapshotState::default())
        .insert_resource(WorldEntities::default())
        .insert_resource(LastSentInput::default())
        .add_systems(Startup, (setup_scene, setup_assets, setup_hud))
        .add_systems(Update, (ingest_snapshot, sync_world, send_input, update_hud))
        .run();
}

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

    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(30.0, 0.1, 30.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.14, 0.12).into(),
            ..default()
        })),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));
}

fn setup_assets(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>, mut materials: ResMut<Assets<StandardMaterial>>) {
    let assets = SharedAssets {
        player_mesh: meshes.add(Capsule3d::new(0.35, 0.9)),
        player_me_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.6, 1.0).into(),
            ..default()
        }),
        player_other_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.6, 0.8, 1.0).into(),
            ..default()
        }),
        player_dead_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.55, 0.55, 0.55).into(),
            ..default()
        }),
        hazard_mesh: meshes.add(Sphere::new(0.6)),
        hazard_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.9, 0.2, 0.2).into(),
            ..default()
        }),
        giver_mesh: meshes.add(Cuboid::new(0.9, 0.9, 0.9)),
        giver_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.9, 0.2, 0.9).into(),
            ..default()
        }),
        cat_mesh: meshes.add(Sphere::new(0.55)),
        cat_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.95, 0.8, 0.25).into(),
            ..default()
        }),
        pickup_mesh: meshes.add(Cuboid::new(0.35, 0.35, 0.35)),
        pickup_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.9, 0.35).into(),
            ..default()
        }),
        loot_mesh: meshes.add(Cuboid::new(0.35, 0.35, 0.35)),
        loot_mat: materials.add(StandardMaterial {
            base_color: Color::srgb(0.95, 0.95, 0.95).into(),
            ..default()
        }),
    };
    commands.insert_resource(assets);
}

fn setup_hud(mut commands: Commands) {
    commands.spawn((
        HudText,
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(14.0),
            top: Val::Px(14.0),
            ..default()
        },
        bevy::ui::widget::Text::new("Waiting…"),
        bevy::text::TextFont {
            font_size: 18.0,
            ..default()
        },
        bevy::text::TextColor(Color::WHITE),
    ));
}

fn ingest_snapshot(mut state: ResMut<SnapshotState>) {
    let mut pending = None;
    let mut identity_hex = None;
    WORLD_SNAPSHOT.with_borrow_mut(|s| {
        pending = s.pending.take();
        identity_hex = s.identity_hex.clone();
    });

    if let Some(hex) = identity_hex {
        state.identity_hex = Some(hex);
    }

    let Some(json) = pending else {
        return;
    };

    match serde_json::from_str::<WorldSnapshot>(&json) {
        Ok(snapshot) => {
            state.latest = Some(snapshot);
            state.last_error = None;
        }
        Err(err) => {
            state.last_error = Some(err.to_string());
        }
    }
}

fn sync_world(
    mut commands: Commands,
    assets: Res<SharedAssets>,
    mut entities: ResMut<WorldEntities>,
    state: Res<SnapshotState>,
) {
    let Some(snapshot) = &state.latest else {
        return;
    };

    // Build quick lookup maps.
    let alive_by_identity = snapshot
        .characters
        .iter()
        .map(|c| (c.identity_hex.as_str(), c.alive))
        .collect::<std::collections::HashMap<_, _>>();

    let me = state.identity_hex.as_deref();

    // Players
    {
        let mut seen = std::collections::HashSet::<String>::new();
        for t in &snapshot.transforms {
            let id = t.identity_hex.clone();
            seen.insert(id.clone());

            let alive = alive_by_identity
                .get(id.as_str())
                .copied()
                .unwrap_or(true);
            let material = if !alive {
                assets.player_dead_mat.clone()
            } else if Some(id.as_str()) == me {
                assets.player_me_mat.clone()
            } else {
                assets.player_other_mat.clone()
            };

            let entity = entities.players.get(&id).copied().unwrap_or_else(|| {
                let e = commands
                    .spawn((
                        Mesh3d(assets.player_mesh.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::from_xyz(t.x, t.y, t.z),
                    ))
                    .id();
                entities.players.insert(id.clone(), e);
                e
            });

            commands.entity(entity).insert((
                MeshMaterial3d(material),
                Transform::from_xyz(t.x, t.y, t.z),
            ));
        }

        let stale = entities
            .players
            .iter()
            .filter(|(k, _)| !seen.contains(*k))
            .map(|(k, v)| (k.clone(), *v))
            .collect::<Vec<_>>();
        for (k, e) in stale {
            commands.entity(e).despawn();
            entities.players.remove(&k);
        }
    }

    // Hazard bots
    {
        let mut seen = std::collections::HashSet::<String>::new();
        for b in &snapshot.hazard_bots {
            let id = b.id.clone();
            seen.insert(id.clone());

            let entity = entities.hazard_bots.get(&id).copied().unwrap_or_else(|| {
                let e = commands
                    .spawn((
                        Mesh3d(assets.hazard_mesh.clone()),
                        MeshMaterial3d(assets.hazard_mat.clone()),
                        Transform::from_xyz(b.x, b.y, b.z),
                    ))
                    .id();
                entities.hazard_bots.insert(id.clone(), e);
                e
            });

            commands.entity(entity).insert(Transform::from_xyz(b.x, b.y, b.z));
        }

        let stale = entities
            .hazard_bots
            .iter()
            .filter(|(k, _)| !seen.contains(*k))
            .map(|(k, v)| (k.clone(), *v))
            .collect::<Vec<_>>();
        for (k, e) in stale {
            commands.entity(e).despawn();
            entities.hazard_bots.remove(&k);
        }
    }

    // Static entities
    {
        let mut seen = std::collections::HashSet::<String>::new();
        for s in &snapshot.static_entities {
            let id = s.id.clone();
            seen.insert(id.clone());

            let (mesh, mat) = match s.kind.as_str() {
                "quest_giver" => (assets.giver_mesh.clone(), assets.giver_mat.clone()),
                "cat" => (assets.cat_mesh.clone(), assets.cat_mat.clone()),
                _ => (assets.giver_mesh.clone(), assets.giver_mat.clone()),
            };

            let entity = entities.statics.get(&id).copied().unwrap_or_else(|| {
                let e = commands
                    .spawn((Mesh3d(mesh), MeshMaterial3d(mat), Transform::from_xyz(s.x, s.y, s.z)))
                    .id();
                entities.statics.insert(id.clone(), e);
                e
            });

            commands.entity(entity).insert(Transform::from_xyz(s.x, s.y, s.z));
        }

        let stale = entities
            .statics
            .iter()
            .filter(|(k, _)| !seen.contains(*k))
            .map(|(k, v)| (k.clone(), *v))
            .collect::<Vec<_>>();
        for (k, e) in stale {
            commands.entity(e).despawn();
            entities.statics.remove(&k);
        }
    }

    // Pickups
    {
        let mut seen = std::collections::HashSet::<String>::new();
        for p in &snapshot.pickups {
            let id = p.id.clone();
            seen.insert(id.clone());

            let entity = entities.pickups.get(&id).copied().unwrap_or_else(|| {
                let e = commands
                    .spawn((
                        Mesh3d(assets.pickup_mesh.clone()),
                        MeshMaterial3d(assets.pickup_mat.clone()),
                        Transform::from_xyz(p.x, p.y, p.z),
                    ))
                    .id();
                entities.pickups.insert(id.clone(), e);
                e
            });

            commands.entity(entity).insert(Transform::from_xyz(p.x, p.y, p.z));
        }

        let stale = entities
            .pickups
            .iter()
            .filter(|(k, _)| !seen.contains(*k))
            .map(|(k, v)| (k.clone(), *v))
            .collect::<Vec<_>>();
        for (k, e) in stale {
            commands.entity(e).despawn();
            entities.pickups.remove(&k);
        }
    }

    // Loot bags
    {
        let mut seen = std::collections::HashSet::<String>::new();
        for b in &snapshot.loot_bags {
            let id = b.id.clone();
            seen.insert(id.clone());

            let entity = entities.loot_bags.get(&id).copied().unwrap_or_else(|| {
                let e = commands
                    .spawn((
                        Mesh3d(assets.loot_mesh.clone()),
                        MeshMaterial3d(assets.loot_mat.clone()),
                        Transform::from_xyz(b.x, b.y, b.z),
                    ))
                    .id();
                entities.loot_bags.insert(id.clone(), e);
                e
            });

            commands.entity(entity).insert(Transform::from_xyz(b.x, b.y, b.z));
        }

        let stale = entities
            .loot_bags
            .iter()
            .filter(|(k, _)| !seen.contains(*k))
            .map(|(k, v)| (k.clone(), *v))
            .collect::<Vec<_>>();
        for (k, e) in stale {
            commands.entity(e).despawn();
            entities.loot_bags.remove(&k);
        }
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = stdb_set_input)]
    fn stdb_set_input(dx: f32, dz: f32);
    #[wasm_bindgen(js_name = stdb_interact)]
    fn stdb_interact();
    #[wasm_bindgen(js_name = stdb_respawn)]
    fn stdb_respawn();
}

fn update_hud(keys: Res<ButtonInput<KeyCode>>, mut hud: Query<&mut bevy::ui::widget::Text, With<HudText>>, state: Res<SnapshotState>) {
    if keys.just_pressed(KeyCode::KeyE) {
        stdb_interact();
    }
    if keys.just_pressed(KeyCode::KeyR) {
        stdb_respawn();
    }

    let Ok(mut text) = hud.single_mut() else { return; };

    let headline = "SpacetimeDB Bevy Web Demo\n";
    let status = if let Some(err) = &state.last_error {
        format!("Snapshot error: {err}")
    } else if state.latest.is_some() {
        "Receiving snapshots".to_string()
    } else {
        "Waiting for connection…".to_string()
    };

    let me = state
        .identity_hex
        .clone()
        .unwrap_or_else(|| "(unknown)".to_string());

    let mut me_line = String::new();
    if let (Some(snapshot), Some(me_hex)) = (state.latest.as_ref(), state.identity_hex.as_deref()) {
        if let Some(c) = snapshot.characters.iter().find(|c| c.identity_hex == me_hex) {
            let life = if c.alive { "ALIVE" } else { "DEAD" };
            me_line = format!(
                "{} · HP {}/{} · {} · XP {} · deaths {}\n",
                c.name, c.hp_current, c.hp_max, life, c.xp, c.deaths
            );
        }
    }

    text.0 = format!("{headline}Identity: {me}\n{me_line}{status}\nWASD move · E interact · R respawn");
}

fn send_input(keys: Res<ButtonInput<KeyCode>>, mut last: ResMut<LastSentInput>) {
    let mut dx = 0.0;
    let mut dz = 0.0;
    if keys.pressed(KeyCode::KeyA) {
        dx -= 1.0;
    }
    if keys.pressed(KeyCode::KeyD) {
        dx += 1.0;
    }
    if keys.pressed(KeyCode::KeyW) {
        dz -= 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        dz += 1.0;
    }

    if dx == last.dx && dz == last.dz {
        return;
    }
    last.dx = dx;
    last.dz = dz;
    stdb_set_input(dx, dz);
}
