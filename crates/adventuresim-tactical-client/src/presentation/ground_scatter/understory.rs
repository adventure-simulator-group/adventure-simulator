use adventuresim_tactical_core::prelude::{GroundCover, SceneGround, SceneTerrain};
use bevy::{
    camera::visibility::VisibilityRange,
    prelude::{Commands, Mesh3d, MeshMaterial3d, Name, Vec2},
};

use crate::presentation::{splitmix64, unit_hash};

use super::{
    GroundScatterLayer, HazelPresentationCache, TreeLeafRepresentation, foliage_transform,
};

pub(super) fn spawn(
    commands: &mut Commands,
    terrain: &SceneTerrain,
    ground: &SceneGround,
    cache: &HazelPresentationCache,
    base_seed: u64,
    chance: f32,
) {
    let spacing = 3.2;
    let count_x = (terrain.width() / spacing).floor() as i32;
    let count_z = (terrain.depth() / spacing).floor() as i32;
    let half_x = terrain.width() * 0.5;
    let half_z = terrain.depth() * 0.5;
    for z in 0..count_z {
        for x in 0..count_x {
            let cell = ((x as u32 as u64) << 32) | z as u32 as u64;
            let hash = splitmix64(base_seed ^ cell ^ 0xa04f_63d2_719b_e850);
            if unit_hash(hash) >= chance {
                continue;
            }
            let jitter_x = unit_hash(splitmix64(hash ^ 0x39bd_7f21)) - 0.5;
            let jitter_z = unit_hash(splitmix64(hash ^ 0xe651_34aa)) - 0.5;
            let world_x = -half_x + (x as f32 + 0.5 + jitter_x * 0.72) * spacing;
            let world_z = -half_z + (z as f32 + 0.5 + jitter_z * 0.72) * spacing;
            if ground
                .ground_at(Vec2::new(world_x, world_z))
                .is_none_or(|sample| sample.cover == GroundCover::LeafLitter)
            {
                continue;
            }
            let Some(transform) = foliage_transform(terrain, world_x, world_z, hash) else {
                continue;
            };
            commands.spawn((
                Name::new("Shared common hazel shrub wood"),
                GroundScatterLayer::Understory,
                Mesh3d(cache.branches.as_ref().unwrap().clone()),
                MeshMaterial3d(cache.bark.as_ref().unwrap().clone()),
                VisibilityRange::abrupt(0.0, 92.0),
                transform,
            ));
            commands.spawn((
                Name::new("Shared common hazel cambered leaves"),
                GroundScatterLayer::Understory,
                TreeLeafRepresentation::TexturedMesh,
                Mesh3d(cache.cambered_leaves.as_ref().unwrap().clone()),
                MeshMaterial3d(cache.leaves.as_ref().unwrap().clone()),
                VisibilityRange {
                    start_margin: 0.0..0.0,
                    end_margin: 26.0..34.0,
                    use_aabb: true,
                },
                transform,
            ));
            commands.spawn((
                Name::new("Shared common hazel alpha-card leaves"),
                GroundScatterLayer::Understory,
                TreeLeafRepresentation::AlphaCard,
                Mesh3d(cache.leaf_cards.as_ref().unwrap().clone()),
                MeshMaterial3d(cache.leaves.as_ref().unwrap().clone()),
                VisibilityRange {
                    start_margin: 26.0..34.0,
                    end_margin: 84.0..96.0,
                    use_aabb: true,
                },
                transform,
            ));
        }
    }
}
