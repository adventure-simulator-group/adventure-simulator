//! GLB/GLTF Scene Loader for extracting spawn markers
//!
//! This module parses GLB files headlessly (without full Bevy asset loading)
//! to extract spawn point metadata for the tactical server.
//!
//! ## Spawn Marker Convention
//!
//! Nodes in the GLB file are identified as spawn markers by their name prefix:
//!
//! - `spawn_player` or `spawn_player_*` - Player spawn point(s)
//! - `spawn_enemy` or `spawn_enemy_*` - Enemy spawn point(s)
//! - `spawn_item` or `spawn_item_*` - Item pickup spawn point(s)
//! - `exit` or `exit_*` - Mission exit/extraction point(s)
//!
//! Alternatively, nodes can use GLTF "extras" JSON with a "spawn_type" field:
//! ```json
//! { "spawn_type": "player" }
//! { "spawn_type": "enemy", "enemy_type": "goblin" }
//! { "spawn_type": "item", "item_id": "gold_coin" }
//! ```
//!
//! The node's world transform (translation) is used as the spawn position.

use bevy::prelude::*;
use std::path::Path;

/// Type of spawn marker
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpawnMarkerType {
    /// Player spawn point
    Player,
    /// Enemy spawn point
    Enemy,
    /// Item pickup location
    Item,
    /// Mission exit/extraction point
    Exit,
}

/// A spawn marker extracted from the GLB file
#[derive(Debug, Clone)]
pub struct SpawnMarker {
    /// Original node name from GLB
    pub name: String,
    /// Type of spawn marker
    pub marker_type: SpawnMarkerType,
    /// World position (from node transform)
    pub position: Vec3,
}

/// Load spawn markers from a GLB/GLTF file
pub fn load_spawn_markers(path: &str) -> Result<Vec<SpawnMarker>, SpawnLoadError> {
    let path = Path::new(path);

    // Check if file exists
    if !path.exists() {
        return Err(SpawnLoadError::FileNotFound(path.to_string_lossy().to_string()));
    }

    // Load GLTF document
    let (document, _buffers, _images) = gltf::import(path)
        .map_err(|e| SpawnLoadError::GltfError(e.to_string()))?;

    let mut markers = Vec::new();

    // Iterate through all nodes in all scenes
    for scene in document.scenes() {
        for node in scene.nodes() {
            collect_markers_recursive(&node, Mat4::IDENTITY, &mut markers);
        }
    }

    Ok(markers)
}

fn collect_markers_recursive(
    node: &gltf::Node,
    parent_transform: Mat4,
    markers: &mut Vec<SpawnMarker>,
) {
    // Get this node's local transform
    let local_transform = node_transform(node);
    let world_transform = parent_transform * local_transform;

    // Check if this node is a spawn marker
    if let Some(marker) = parse_spawn_marker(node, world_transform) {
        markers.push(marker);
    }

    // Recurse into children
    for child in node.children() {
        collect_markers_recursive(&child, world_transform, markers);
    }
}

fn node_transform(node: &gltf::Node) -> Mat4 {
    let (translation, rotation, scale) = node.transform().decomposed();

    let translation = Vec3::from(translation);
    let rotation = Quat::from_array(rotation);
    let scale = Vec3::from(scale);

    Mat4::from_scale_rotation_translation(scale, rotation, translation)
}

fn parse_spawn_marker(node: &gltf::Node, world_transform: Mat4) -> Option<SpawnMarker> {
    let name = node.name().unwrap_or("").to_string();

    // First, try to parse from node name
    let marker_type = parse_marker_type_from_name(&name)
        .or_else(|| parse_marker_type_from_extras(node));

    marker_type.map(|marker_type| {
        // Extract position from world transform
        let (_scale, _rotation, translation) = world_transform.to_scale_rotation_translation();

        SpawnMarker {
            name,
            marker_type,
            position: translation,
        }
    })
}

fn parse_marker_type_from_name(name: &str) -> Option<SpawnMarkerType> {
    let lower = name.to_lowercase();

    if lower.starts_with("spawn_player") {
        Some(SpawnMarkerType::Player)
    } else if lower.starts_with("spawn_enemy") {
        Some(SpawnMarkerType::Enemy)
    } else if lower.starts_with("spawn_item") {
        Some(SpawnMarkerType::Item)
    } else if lower.starts_with("exit") {
        Some(SpawnMarkerType::Exit)
    } else {
        None
    }
}

fn parse_marker_type_from_extras(_node: &gltf::Node) -> Option<SpawnMarkerType> {
    // GLTF extras parsing is complex with the gltf crate's Void type.
    // For MVP, we rely on node name conventions only.
    // Full extras parsing can be added when needed:
    //   { "spawn_type": "enemy", "enemy_type": "goblin" }
    None
}

#[derive(Debug)]
pub enum SpawnLoadError {
    FileNotFound(String),
    GltfError(String),
}

impl std::fmt::Display for SpawnLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpawnLoadError::FileNotFound(path) => write!(f, "File not found: {}", path),
            SpawnLoadError::GltfError(e) => write!(f, "GLTF parse error: {}", e),
        }
    }
}

impl std::error::Error for SpawnLoadError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_marker_type_from_name() {
        assert_eq!(
            parse_marker_type_from_name("spawn_player"),
            Some(SpawnMarkerType::Player)
        );
        assert_eq!(
            parse_marker_type_from_name("spawn_player_1"),
            Some(SpawnMarkerType::Player)
        );
        assert_eq!(
            parse_marker_type_from_name("Spawn_Player_Start"),
            Some(SpawnMarkerType::Player)
        );
        assert_eq!(
            parse_marker_type_from_name("spawn_enemy"),
            Some(SpawnMarkerType::Enemy)
        );
        assert_eq!(
            parse_marker_type_from_name("spawn_enemy_goblin"),
            Some(SpawnMarkerType::Enemy)
        );
        assert_eq!(
            parse_marker_type_from_name("spawn_item_chest"),
            Some(SpawnMarkerType::Item)
        );
        assert_eq!(
            parse_marker_type_from_name("exit"),
            Some(SpawnMarkerType::Exit)
        );
        assert_eq!(
            parse_marker_type_from_name("exit_north"),
            Some(SpawnMarkerType::Exit)
        );
        assert_eq!(parse_marker_type_from_name("random_node"), None);
        assert_eq!(parse_marker_type_from_name(""), None);
    }
}
