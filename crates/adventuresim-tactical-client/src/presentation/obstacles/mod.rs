pub(super) mod rock;
pub(super) mod tree;

use super::*;
use rock::procedural_rock_mesh;

pub(in crate::presentation) fn on_scene_obstacle_added(
    event: On<Add, SceneObstacle>,
    mut commands: Commands,
    obstacles: Query<(&SceneObstacle, &Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut tree_materials: ResMut<Assets<TacticalTreeImpostorMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut tree_cache: ResMut<TreePresentationCache>,
) -> Result {
    let (obstacle, transform) = obstacles.get(event.entity)?;
    let seed = obstacle_seed(transform.translation);
    match *obstacle {
        SceneObstacle::Tree => {
            // Four deterministic mature-oak forms give visible population
            // variation while allowing exact, expensive descendant renders to
            // be shared by trees whose high-resolution geometry is identical.
            let variant_seed = splitmix64(0x6f61_6b00 ^ (seed & 3));
            let cached = if let Some(cached) = tree_cache.variants.get(&variant_seed) {
                cached.clone()
            } else {
                let branches = procedural_tree_skeleton(variant_seed);
                let leaves = procedural_oak_leaves(variant_seed, &branches);
                let bark_texture = images.add(procedural_oak_bark_image(variant_seed));
                let bark_material = materials.add(StandardMaterial {
                    base_color: Color::WHITE,
                    base_color_texture: Some(bark_texture),
                    perceptual_roughness: 0.95,
                    ..default()
                });
                let leaf_material = materials.add(StandardMaterial {
                    base_color: Color::srgb(0.2, 0.5, 0.105),
                    perceptual_roughness: 0.86,
                    diffuse_transmission: 0.38,
                    thickness: 0.001,
                    unlit: true,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                });
                let branch_meshes = [3, 2, 1, 0]
                    .map(|depth| meshes.add(procedural_tree_branch_mesh(&branches, depth)));
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
                tree_cache.variants.insert(variant_seed, cached.clone());
                cached
            };
            spawn_cached_tree(&mut commands, event.entity, &cached);
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
