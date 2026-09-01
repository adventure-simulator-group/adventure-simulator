use super::*;
use bevy::prelude::{Mesh3d, MeshMaterial3d, Name, Visibility};

/// Production geometry/material specimen used only by deterministic capture
/// views. It avoids coupling review availability to whichever species the
/// habitat sampler happened to place in a fixture.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(crate) struct UnderstoryReviewSpecimen {
    pub(crate) common_name: &'static str,
    pub(crate) focus: Vec3,
}

pub(crate) fn spawn_understory_review_specimens(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    standard_materials: &mut Assets<StandardMaterial>,
    leaf_materials: &mut Assets<TacticalTreeLeafCardMaterial>,
    cache: &mut WoodyUnderstoryPresentationCache,
    procedural_assets: &ProceduralTextureAssets,
    origin: Vec3,
) {
    ensure_understory_presentations(
        meshes,
        standard_materials,
        leaf_materials,
        cache,
        procedural_assets,
    );
    for (common_name, presentation) in [
        ("common hazel", &cache.hazel),
        ("blackthorn", &cache.blackthorn),
        ("common hawthorn", &cache.hawthorn),
    ] {
        let marker = UnderstoryReviewSpecimen {
            common_name,
            focus: origin,
        };
        commands.spawn((
            Name::new(format!("Capture {common_name} production shrub wood")),
            marker,
            Mesh3d(
                presentation
                    .branches
                    .as_ref()
                    .expect("understory review branch mesh exists")
                    .clone(),
            ),
            MeshMaterial3d(
                presentation
                    .bark
                    .as_ref()
                    .expect("understory review bark material exists")
                    .clone(),
            ),
            Visibility::Hidden,
            Transform::from_translation(origin),
        ));
        commands.spawn((
            Name::new(format!("Capture {common_name} production leaf cards")),
            marker,
            TreeLeafRepresentation::AlphaCard,
            Mesh3d(
                presentation
                    .leaf_cards
                    .as_ref()
                    .expect("understory review leaf-card mesh exists")
                    .clone(),
            ),
            MeshMaterial3d(
                presentation
                    .leaves
                    .as_ref()
                    .expect("understory review leaf material exists")
                    .clone(),
            ),
            Visibility::Hidden,
            Transform::from_translation(origin),
        ));
    }
}
