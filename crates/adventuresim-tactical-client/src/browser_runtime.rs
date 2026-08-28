//! Persistent browser renderer shared by strategic scenes and tactical play.
//!
//! The proof of concept deliberately installs the complete tactical plugin
//! graph and retains eager asset loading. This module supplies only the
//! lifecycle seam: JavaScript queues typed scene commands, Bevy owns the one
//! running world, and scene-scoped entities are removed without recreating the
//! Wasm application, WebGPU device, or persistent asset stores.

use std::{
    collections::VecDeque,
    sync::{Mutex, OnceLock},
};

use adventuresim_tactical_netcode::prelude::AdventureSimulatorClient;
use adventuresim_weapon_model::{
    MaterialClass, WeaponDesign, default_design, derive_material_masses, derive_properties, encode,
    generate,
};
use bevy::{
    asset::RenderAssetUsages,
    camera::Exposure,
    light::GlobalAmbientLight,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use serde::Deserialize;

use crate::{
    Args, player::LocalCharacterId, presentation::TacticalGameplayCamera, ui::TacticalUiRoot,
};

static COMMANDS: OnceLock<Mutex<VecDeque<BrowserCommand>>> = OnceLock::new();

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Resource)]
pub(crate) enum BrowserMode {
    #[default]
    Strategic,
    Tactical,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum BrowserCommand {
    ShowStrategicScene {
        scene: StrategicScene,
    },
    OrbitForge {
        delta_x: f32,
        delta_y: f32,
    },
    ZoomForge {
        delta: f32,
    },
    HideStrategicScene,
    EnterTactical {
        server_addr: String,
        character_id: u64,
    },
    ExitTactical,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum StrategicScene {
    Forge {
        #[serde(default = "default_catalog_id")]
        catalog_id: String,
        design_json: Option<String>,
    },
}

fn default_catalog_id() -> String {
    "longsword".into()
}

#[derive(Component)]
struct StrategicSceneEntity;

#[derive(Component)]
struct StrategicSceneRoot;

#[derive(Resource)]
struct ForgePreviewView {
    yaw: f32,
    pitch: f32,
    distance: f32,
}

impl Default for ForgePreviewView {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            distance: 1.75,
        }
    }
}

impl ForgePreviewView {
    fn orbit(&mut self, delta_x: f32, delta_y: f32) {
        self.yaw += delta_x * 0.008;
        self.pitch = (self.pitch + delta_y * 0.008).clamp(-1.2, 1.2);
    }

    fn zoom(&mut self, delta: f32) {
        self.distance = (self.distance + delta * 0.002).clamp(0.9, 4.5);
    }
}

pub(crate) struct BrowserRuntimePlugin {
    initial_mode: BrowserMode,
}

impl BrowserRuntimePlugin {
    pub(crate) fn new(initial_tactical: bool) -> Self {
        Self {
            initial_mode: if initial_tactical {
                BrowserMode::Tactical
            } else {
                BrowserMode::Strategic
            },
        }
    }
}

impl Plugin for BrowserRuntimePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.initial_mode)
            .init_resource::<ForgePreviewView>()
            .add_systems(
                Update,
                (drain_browser_commands, sync_tactical_ui_visibility),
            );
    }
}

pub(crate) fn queue_json(json: &str) -> Result<(), String> {
    let command = serde_json::from_str(json)
        .map_err(|error| format!("invalid browser renderer command: {error}"))?;
    COMMANDS
        .get_or_init(|| Mutex::new(VecDeque::new()))
        .lock()
        .map_err(|_| "browser renderer command queue is unavailable".to_string())?
        .push_back(command);
    Ok(())
}

fn drain_browser_commands(
    mut commands: Commands,
    mut mode: ResMut<BrowserMode>,
    mut args: ResMut<Args>,
    mut local_character: ResMut<LocalCharacterId>,
    scene_entities: Query<Entity, With<StrategicSceneEntity>>,
    clients: Query<Entity, With<AdventureSimulatorClient>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cameras: Query<
        (&mut Transform, &mut Exposure),
        (With<TacticalGameplayCamera>, Without<StrategicSceneRoot>),
    >,
    mut scene_roots: Query<&mut Transform, (With<StrategicSceneRoot>, Without<Camera3d>)>,
    mut preview_view: ResMut<ForgePreviewView>,
    mut ambient: ResMut<GlobalAmbientLight>,
) {
    let pending = COMMANDS
        .get_or_init(|| Mutex::new(VecDeque::new()))
        .lock()
        .map(|mut queue| queue.drain(..).collect::<Vec<_>>())
        .unwrap_or_default();

    for command in pending {
        match command {
            BrowserCommand::ShowStrategicScene { scene } => {
                *mode = BrowserMode::Strategic;
                despawn_strategic_scene(&mut commands, &scene_entities);
                for (mut camera, mut exposure) in &mut cameras {
                    *camera = Transform::IDENTITY.looking_to(Vec3::NEG_Z, Vec3::Y);
                    exposure.ev100 = 8.0;
                }
                ambient.color = Color::srgb(0.95, 0.82, 0.66);
                ambient.brightness = 1_200.0;
                match scene {
                    StrategicScene::Forge {
                        catalog_id,
                        design_json,
                    } => spawn_forge_scene(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        &catalog_id,
                        design_json.as_deref(),
                        &preview_view,
                    ),
                }
            }
            BrowserCommand::OrbitForge { delta_x, delta_y } => {
                preview_view.orbit(delta_x, delta_y);
                for mut root in &mut scene_roots {
                    apply_preview_view(&preview_view, &mut root);
                }
            }
            BrowserCommand::ZoomForge { delta } => {
                preview_view.zoom(delta);
                for mut root in &mut scene_roots {
                    apply_preview_view(&preview_view, &mut root);
                }
            }
            BrowserCommand::HideStrategicScene => {
                despawn_strategic_scene(&mut commands, &scene_entities);
                *preview_view = ForgePreviewView::default();
            }
            BrowserCommand::EnterTactical {
                server_addr,
                character_id,
            } => {
                despawn_strategic_scene(&mut commands, &scene_entities);
                for (_, mut exposure) in &mut cameras {
                    *exposure = Exposure::SUNLIGHT;
                }
                ambient.color = Color::srgb(0.36, 0.48, 0.72);
                ambient.brightness = 0.6;
                if clients.is_empty() {
                    args.id = character_id;
                    args.server_addr.clone_from(&server_addr);
                    local_character.0 = character_id;
                    commands.spawn(AdventureSimulatorClient {
                        player_id: character_id,
                        server_url: server_addr,
                    });
                }
                *mode = BrowserMode::Tactical;
            }
            BrowserCommand::ExitTactical => {
                for entity in &clients {
                    commands.entity(entity).despawn();
                }
                *mode = BrowserMode::Strategic;
            }
        }
    }
}

fn despawn_strategic_scene(
    commands: &mut Commands,
    scene_entities: &Query<Entity, With<StrategicSceneEntity>>,
) {
    for entity in scene_entities {
        commands.entity(entity).despawn();
    }
}

fn spawn_forge_scene(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    catalog_id: &str,
    design_json: Option<&str>,
    view: &ForgePreviewView,
) {
    let design = design_json
        .and_then(|json| serde_json::from_str::<WeaponDesign>(json).ok())
        .or_else(|| default_design(catalog_id));
    let Some(design) = design else {
        warn!(
            catalog_id,
            "browser forge preview rejected unknown weapon chassis"
        );
        return;
    };
    let Ok(generated) = generate(&design) else {
        warn!(
            catalog_id,
            "browser forge preview failed to generate weapon mesh"
        );
        return;
    };

    let center =
        (Vec3::from_array(generated.bounds.min) + Vec3::from_array(generated.bounds.max)) * 0.5;
    let extent = Vec3::from_array(generated.bounds.max) - Vec3::from_array(generated.bounds.min);
    let scale = 1.45 / extent.max_element().max(0.001);
    let root = commands
        .spawn((
            Name::new("Strategic forge weapon preview"),
            StrategicSceneEntity,
            StrategicSceneRoot,
            preview_transform(view, scale),
        ))
        .id();

    for part in generated.parts {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, part.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, part.normals);
        mesh.insert_indices(Indices::U32(part.indices));
        commands.spawn((
            Name::new(format!("Strategic forge part {}", part.component_id)),
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(materials.add(preview_material(part.material))),
            Transform::from_translation(-center),
            ChildOf(root),
        ));
    }

    commands.spawn((
        Name::new("Strategic forge preview light"),
        StrategicSceneEntity,
        DirectionalLight {
            illuminance: 60_000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-3.0, 4.0, 2.0).looking_at(Vec3::new(0.0, 0.0, -2.2), Vec3::Y),
    ));

    commands.spawn((
        Name::new("Strategic forge preview fill light"),
        StrategicSceneEntity,
        PointLight {
            intensity: 12_000.0,
            range: 8.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(2.0, 1.5, 0.5),
    ));
}

fn preview_transform(view: &ForgePreviewView, scale: f32) -> Transform {
    Transform::from_xyz(0.0, 0.0, -view.distance)
        .with_rotation(Quat::from_euler(EulerRot::YXZ, view.yaw, view.pitch, -0.55))
        .with_scale(Vec3::splat(scale))
}

fn apply_preview_view(view: &ForgePreviewView, transform: &mut Transform) {
    let scale = transform.scale.x;
    *transform = preview_transform(view, scale);
}

pub(crate) fn default_design_json(catalog_id: &str) -> Result<String, String> {
    let design = default_design(catalog_id).ok_or("unknown melee weapon chassis")?;
    serde_json::to_string(&design).map_err(|error| error.to_string())
}

pub(crate) fn catalog_json() -> Result<String, String> {
    serde_json::to_string(adventuresim_weapon_model::MELEE_CATALOG_IDS)
        .map_err(|error| error.to_string())
}

pub(crate) fn encode_design_json(json: &str) -> Result<Vec<u8>, String> {
    let design: WeaponDesign = serde_json::from_str(json).map_err(|error| error.to_string())?;
    encode(&design).map_err(|error| error.to_string())
}

pub(crate) fn editor_fields_json(json: &str) -> Result<String, String> {
    let design: WeaponDesign = serde_json::from_str(json).map_err(|error| error.to_string())?;
    serde_json::to_string(&adventuresim_weapon_model::numeric_editor_fields(&design))
        .map_err(|error| error.to_string())
}

pub(crate) fn quote_design_json(json: &str) -> Result<String, String> {
    let design: WeaponDesign = serde_json::from_str(json).map_err(|error| error.to_string())?;
    let physical = derive_properties(&design).map_err(|errors| format!("{errors:?}"))?;
    let minutes =
        60 + (physical.mass_kg * 120.0).ceil() as u64 + design.components.len() as u64 * 12;
    let mut materials = std::collections::BTreeMap::<&str, f32>::new();
    for mass in derive_material_masses(&design).map_err(|errors| format!("{errors:?}"))? {
        let stock = match mass.material {
            MaterialClass::Steel | MaterialClass::DarkSteel => "steel_stock",
            MaterialClass::Leather | MaterialClass::DarkLeather => "leather_stock",
            MaterialClass::Brass => "brass_stock",
            MaterialClass::Wood => "wood_stock",
        };
        *materials.entry(stock).or_default() += mass.mass_kg;
    }
    serde_json::to_string(&serde_json::json!({ "minutes": minutes, "materials": materials }))
        .map_err(|error| error.to_string())
}

fn preview_material(class: MaterialClass) -> StandardMaterial {
    let base_color = match class {
        MaterialClass::Wood => Color::srgb(0.30, 0.18, 0.09),
        MaterialClass::Leather => Color::srgb(0.16, 0.09, 0.05),
        MaterialClass::DarkLeather => Color::srgb(0.055, 0.045, 0.038),
        MaterialClass::Brass => Color::srgb(0.68, 0.50, 0.18),
        MaterialClass::Steel => Color::srgb(0.68, 0.72, 0.76),
        MaterialClass::DarkSteel => Color::srgb(0.30, 0.33, 0.37),
    };
    let metallic = matches!(
        class,
        MaterialClass::Brass | MaterialClass::Steel | MaterialClass::DarkSteel
    );
    StandardMaterial {
        base_color,
        metallic: if metallic { 0.35 } else { 0.0 },
        perceptual_roughness: if metallic { 0.48 } else { 0.76 },
        ..default()
    }
}

fn sync_tactical_ui_visibility(
    mode: Res<BrowserMode>,
    mut roots: Query<&mut Visibility, With<TacticalUiRoot>>,
) {
    let visibility = if *mode == BrowserMode::Tactical {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut current in &mut roots {
        *current = visibility;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_commands_are_closed_and_typed() {
        let forge: BrowserCommand =
            serde_json::from_str(r#"{"type":"show-strategic-scene","scene":{"type":"forge"}}"#)
                .unwrap();
        assert!(matches!(
            forge,
            BrowserCommand::ShowStrategicScene {
                scene: StrategicScene::Forge { catalog_id, .. }
            } if catalog_id == "longsword"
        ));
        assert!(serde_json::from_str::<BrowserCommand>(r#"{"type":"unknown"}"#).is_err());
        assert!(matches!(
            serde_json::from_str::<BrowserCommand>(
                r#"{"type":"orbit-forge","delta_x":4.0,"delta_y":-2.0}"#
            )
            .unwrap(),
            BrowserCommand::OrbitForge { .. }
        ));
        assert!(matches!(
            serde_json::from_str::<BrowserCommand>(r#"{"type":"zoom-forge","delta":12.0}"#)
                .unwrap(),
            BrowserCommand::ZoomForge { .. }
        ));
    }

    #[test]
    fn forge_view_is_manual_and_bounded() {
        let mut view = ForgePreviewView::default();
        view.orbit(20.0, 10_000.0);
        assert!(view.yaw > 0.0);
        assert_eq!(view.pitch, 1.2);
        view.zoom(-10_000.0);
        assert_eq!(view.distance, 0.9);
        view.zoom(10_000.0);
        assert_eq!(view.distance, 4.5);
    }
}
