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
    mut leaf_materials: ResMut<Assets<TacticalTreeLeafMaterial>>,
    mut leaf_card_materials: ResMut<Assets<TacticalTreeLeafCardMaterial>>,
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
            let leaves = procedural_oak_leaves(variant_seed, &branches, competition);
            let bark_texture = images.add(procedural_oak_bark_image(variant_seed));
            let bark_material = materials.add(StandardMaterial {
                base_color: Color::WHITE,
                base_color_texture: Some(bark_texture),
                perceptual_roughness: 0.95,
                ..default()
            });
            let leaf_material = leaf_materials.add(TacticalTreeLeafMaterial {
                parameters: Vec4::new(0.74, 0.67, 0.035, 1.15),
            });
            let rendered_leaf = images.add(rendered_oak_leaf_card_image());
            let leaf_card_material = leaf_card_materials.add(TacticalTreeLeafCardMaterial {
                rendered_leaf,
                parameters: Vec4::new(0.74, 0.67, 0.035, 1.15),
            });
            let bud_material = materials.add(StandardMaterial {
                base_color: Color::srgb(0.36, 0.27, 0.1),
                perceptual_roughness: 0.92,
                ..default()
            });
            let baked_lods = (1..5)
                .map(|lod| bake_tree_lod(variant_seed, &branches, &leaves, lod))
                .collect::<Vec<_>>();
            for bake in &baked_lods {
                validate_tree_bake_provenance(&bake.provenance);
            }
            let clusters = (0..TREE_PRIMARY_GROUP_COUNT)
                .map(|primary_group| {
                    let source = baked_lods[0]
                        .clusters
                        .iter()
                        .find(|cluster| cluster.primary_group == primary_group)
                        .expect("every generated primary scaffold has a baked cluster");
                    CachedTreeClusterPresentation {
                        primary_group,
                        center: source.center,
                        radius: source.radius,
                        branch_meshes: [3, 2, 1].map(|depth| {
                            meshes.add(procedural_tree_branch_group_mesh(
                                &branches,
                                depth,
                                primary_group,
                            ))
                        }),
                        leaf_mesh: meshes.add(procedural_oak_leaf_sector_mesh(
                            &leaves,
                            usize::from(primary_group),
                        )),
                        leaf_card_mesh: meshes
                            .add(procedural_oak_leaf_card_group_mesh(&leaves, primary_group)),
                        bud_mesh: meshes
                            .add(procedural_oak_bud_group_mesh(&branches, primary_group)),
                        card_meshes: core::array::from_fn(|index| {
                            let cluster = baked_lods[index]
                                .clusters
                                .iter()
                                .find(|cluster| cluster.primary_group == primary_group)
                                .expect("every recursive LOD preserves primary scaffold identity");
                            meshes.add(cluster.mesh.clone())
                        }),
                    }
                })
                .collect();
            let cached = CachedTreePresentation {
                trunk_mesh: meshes.add(procedural_tree_branch_mesh(&branches, 0)),
                clusters,
                whole_tree_card_mesh: meshes.add(baked_lods[3].mesh.clone()),
                bark_material,
                leaf_material,
                leaf_card_material,
                bud_material,
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

// The presentation facade is compiled into several binaries, while only the
// deterministic scene viewer consumes this review-specimen helper.
#[allow(dead_code)]
pub(crate) fn oak_review_terminal_specimen(
    root: Vec3,
    canopy_bps: u16,
) -> (Mesh, Mesh, Mesh, Vec3, Vec3) {
    let seed = obstacle_seed(root);
    let variant_seed = splitmix64(0x6f61_6b00 ^ (seed & 3));
    let branches = procedural_tree_skeleton(variant_seed, canopy_competition(canopy_bps));
    let competition = canopy_competition(canopy_bps);
    let leaves = procedural_oak_leaves(variant_seed, &branches, competition);
    let camera_direction = Vec3::new(1.0, 0.0, 1.0).normalize();
    let preferred_height = 2.5;
    let (shoot_id, shoot) = branches
        .iter()
        .filter(|branch| branch.depth == 3 && branch.is_limb_tip)
        .enumerate()
        .max_by(|(_, left), (_, right)| {
            let score = |branch: &TreeBranchSegment| {
                branch.end.dot(camera_direction) - (branch.end.y - preferred_height).abs() * 0.35
            };
            score(left).total_cmp(&score(right))
        })
        .map(|(index, branch)| (index as u16, *branch))
        .expect("procedural oak has terminal shoots");
    let offset = -shoot.start;
    let mut specimen_shoot = shoot;
    specimen_shoot.start += offset;
    specimen_shoot.end += offset;
    let specimen_leaves = leaves
        .iter()
        .filter(|leaf| leaf.shoot_id == shoot_id)
        .copied()
        .map(|mut leaf| {
            leaf.petiole_start += offset;
            leaf.center += offset;
            leaf
        })
        .collect::<Vec<_>>();
    // Frame the entire biological unit, not merely the leaf centroid: the
    // parent junction and terminal bud are both required to judge shoot
    // phyllotaxy. Blade length is included as a conservative bound because
    // leaves can tilt substantially out of the shoot's local frame.
    let mut minimum = specimen_shoot.start.min(specimen_shoot.end);
    let mut maximum = specimen_shoot.start.max(specimen_shoot.end);
    for leaf in &specimen_leaves {
        let extent = Vec3::splat(leaf.length.max(leaf.width) * 0.6);
        minimum = minimum.min(leaf.center - extent);
        maximum = maximum.max(leaf.center + extent);
    }
    let focus = (minimum + maximum) * 0.5;
    let shoot_direction = (specimen_shoot.end - specimen_shoot.start).normalize();
    let mut review_direction = Vec3::Z;
    let mut review_score = f32::NEG_INFINITY;
    for elevation in [-0.2_f32, 0.1, 0.35] {
        for azimuth_index in 0..24 {
            let azimuth = azimuth_index as f32 * core::f32::consts::TAU / 24.0;
            let candidate = Vec3::new(
                azimuth.cos() * elevation.cos(),
                elevation.sin(),
                azimuth.sin() * elevation.cos(),
            );
            let face_area = specimen_leaves
                .iter()
                .map(|leaf| leaf.right.cross(leaf.up).normalize().dot(candidate).abs())
                .sum::<f32>();
            let axial_penalty =
                shoot_direction.dot(candidate).abs() * specimen_leaves.len() as f32 * 0.55;
            let score = face_area - axial_penalty;
            if score > review_score {
                review_score = score;
                review_direction = candidate;
            }
        }
    }
    (
        procedural_tree_branch_mesh(&[specimen_shoot], 3),
        procedural_oak_leaf_mesh(&specimen_leaves),
        procedural_oak_bud_mesh(&[specimen_shoot]),
        focus,
        review_direction,
    )
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
