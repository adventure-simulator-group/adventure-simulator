pub(super) mod rock;
pub(super) mod tree;

use super::*;
use rock::procedural_rock_mesh;

#[derive(Component)]
pub(in crate::presentation) struct PendingTreePresentation;

pub(in crate::presentation) fn on_scene_obstacle_added(
    event: On<Add, SceneObstacle>,
    mut commands: Commands,
    obstacles: Query<(&SceneObstacle, &Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) -> Result {
    let (obstacle, transform) = obstacles.get(event.entity)?;
    let seed = obstacle_seed(transform.translation);
    match *obstacle {
        SceneObstacle::Tree => {
            commands
                .entity(event.entity)
                .insert(PendingTreePresentation);
        }
        SceneObstacle::Rock => {
            commands.entity(event.entity).insert((
                Name::new("Presented tactical rock"),
                ProceduralRockVisual,
                Mesh3d(meshes.add(procedural_rock_mesh(seed))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb_u8(104, 101, 94),
                    perceptual_roughness: 1.0,
                    ..default()
                })),
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(in crate::presentation) fn present_pending_trees(
    mut commands: Commands,
    pending: Query<(Entity, &Transform), With<PendingTreePresentation>>,
    environments: Query<&SceneEnvironment>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut tree_materials: ResMut<Assets<TacticalTreeImpostorMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut tree_cache: ResMut<TreePresentationCache>,
) {
    let Some(environment) = environments.iter().next() else {
        return;
    };
    let competition = canopy_competition(environment.canopy_bps);
    for (entity, transform) in &pending {
        let seed = obstacle_seed(transform.translation);
        let variant_seed = splitmix64(0x6f61_6b00 ^ (seed & 3));
        let competition_key = (competition * 4095.0).round() as u64;
        let cache_key = variant_seed ^ competition_key.rotate_left(32);
        let cached = if let Some(cached) = tree_cache.variants.get(&cache_key) {
            cached.clone()
        } else {
            let branches = procedural_tree_skeleton(variant_seed, competition);
            let leaves = procedural_oak_leaves(variant_seed, &branches);
            let bark_texture = images.add(procedural_oak_bark_image(variant_seed));
            let bark_material = materials.add(StandardMaterial {
                base_color: Color::WHITE,
                base_color_texture: Some(bark_texture),
                perceptual_roughness: 0.95,
                ..default()
            });
            let leaf_material = materials.add(StandardMaterial {
                base_color: Color::srgb(0.3, 0.52, 0.14),
                perceptual_roughness: 0.82,
                reflectance: 0.18,
                diffuse_transmission: 0.4,
                thickness: 0.001,
                double_sided: true,
                cull_mode: None,
                ..default()
            });
            let branch_meshes =
                [3, 2, 1, 0].map(|depth| meshes.add(procedural_tree_branch_mesh(&branches, depth)));
            let leaf_mesh = meshes.add(procedural_oak_leaf_mesh(&leaves));
            let baked_lods = (1..5)
                .map(|lod| bake_tree_lod(variant_seed, &branches, &leaves, lod))
                .collect::<Vec<_>>();
            for bake in &baked_lods {
                validate_tree_bake_provenance(&bake.provenance);
            }
            let cached = CachedTreePresentation {
                branch_meshes,
                leaf_mesh,
                card_meshes: core::array::from_fn(|index| {
                    meshes.add(baked_lods[index].mesh.clone())
                }),
                bark_material,
                leaf_material,
                card_materials: core::array::from_fn(|index| {
                    let bake = &baked_lods[index];
                    let texture = images.add(bake.image.clone());
                    tree_materials.add(tree_impostor_material(variant_seed, bake.lod, texture))
                }),
                provenance: core::array::from_fn(|index| baked_lods[index].provenance.clone()),
            };
            tree_cache.variants.insert(cache_key, cached.clone());
            cached
        };
        spawn_cached_tree(&mut commands, entity, &cached);
        commands.entity(entity).remove::<PendingTreePresentation>();
    }
}

fn canopy_competition(canopy_bps: u16) -> f32 {
    let normalized = f32::from(canopy_bps) / 10_000.0;
    normalized * normalized * (3.0 - 2.0 * normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canopy_competition_is_continuous_and_bounded() {
        assert_eq!(canopy_competition(0), 0.0);
        assert_eq!(canopy_competition(10_000), 1.0);
        let samples = (0..=10_000_u16)
            .step_by(100)
            .map(canopy_competition)
            .collect::<Vec<_>>();
        assert!(samples.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(canopy_competition(3_500) > 0.25 && canopy_competition(3_500) < 0.3);
    }
}
