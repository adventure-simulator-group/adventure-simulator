use adventuresim_tactical_core::prelude::{
    GroundCover, RockArchetype, RockLithology, RockRecipe, SceneGround, SceneTerrain,
};
use bevy::{
    camera::visibility::VisibilityRange,
    light::NotShadowCaster,
    prelude::{Assets, Commands, Mesh, Mesh3d, MeshMaterial3d, Name, StandardMaterial, Vec2},
};

use crate::presentation::obstacles::rock::{procedural_rock_mesh, rock_color};
use crate::presentation::{bps, splitmix64, unit_hash};

use super::{GroundScatterLayer, foliage_transform};

const MESH_VARIANTS: u64 = 4;
const PASSES_PER_SAMPLE: u64 = 3;

pub(super) fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    terrain: &SceneTerrain,
    ground: &SceneGround,
    base_seed: u64,
) {
    let recipes = (0..MESH_VARIANTS).map(recipe).collect::<Vec<_>>();
    let stone_meshes = recipes
        .iter()
        .map(|recipe| meshes.add(procedural_rock_mesh(*recipe)))
        .collect::<Vec<_>>();
    let stone_materials = recipes
        .iter()
        .map(|recipe| {
            materials.add(StandardMaterial {
                base_color: rock_color(recipe.lithology),
                perceptual_roughness: 1.0,
                ..Default::default()
            })
        })
        .collect::<Vec<_>>();
    for (index, sample) in ground.samples().iter().enumerate() {
        if sample.cover != GroundCover::LooseStone {
            continue;
        }
        let grid_x = index % ground.grid_width();
        let grid_z = index / ground.grid_width();
        let cell_origin = Vec2::new(
            grid_x as f32 * ground.grid_scale() - ground.width() * 0.5,
            grid_z as f32 * ground.grid_scale() - ground.depth() * 0.5,
        );
        let density = bps(sample.cover_density_bps);
        for pass in 0..PASSES_PER_SAMPLE {
            let hash =
                splitmix64(base_seed ^ index as u64 ^ pass.rotate_left(17) ^ 0x7374_6f6e_655f_7363);
            if unit_hash(hash) >= density {
                continue;
            }
            let jitter = ground.grid_scale() * 0.72;
            let position = cell_origin
                + Vec2::new(
                    unit_hash(splitmix64(hash ^ 0x672a_1f04)) - 0.5,
                    unit_hash(splitmix64(hash ^ 0xeeb0_31cd)) - 0.5,
                ) * jitter;
            if ground
                .ground_at(position)
                .is_none_or(|surface| surface.cover != GroundCover::LooseStone)
            {
                continue;
            }
            let Some(mut transform) = foliage_transform(terrain, position.x, position.y, hash)
            else {
                continue;
            };
            let variant = (hash % MESH_VARIANTS) as usize;
            let scale = 0.075 + unit_hash(splitmix64(hash ^ 0x51d2_9ec4)) * 0.085;
            transform.scale *= scale;
            transform.translation.y += scale * 0.46;
            commands.spawn((
                Name::new("Tactical loose-stone scatter"),
                GroundScatterLayer::LooseStone,
                NotShadowCaster,
                Mesh3d(stone_meshes[variant].clone()),
                MeshMaterial3d(stone_materials[variant].clone()),
                VisibilityRange::abrupt(0.0, 58.0),
                transform,
            ));
        }
    }
}

fn recipe(variant: u64) -> RockRecipe {
    let archetype = match variant % 3 {
        0 => RockArchetype::Rounded,
        1 => RockArchetype::Angular,
        _ => RockArchetype::Slab,
    };
    RockRecipe {
        seed: splitmix64(0x7065_6262_6c65_0000 ^ variant),
        archetype,
        lithology: match variant % 3 {
            0 => RockLithology::Granite,
            1 => RockLithology::Limestone,
            _ => RockLithology::Sandstone,
        },
        dimensions_cm: match archetype {
            RockArchetype::Rounded => [126, 96, 116],
            RockArchetype::Angular => [132, 104, 120],
            RockArchetype::Slab => [140, 66, 128],
        },
        collision_radius_cm: 75,
    }
}
