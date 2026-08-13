pub(super) mod rock;
pub(super) mod tree;

use super::*;
use rock::{TacticalRockMaterial, procedural_rock_mesh, rock_material};
use tree::{PendingTreePresentation, canopy_competition};

pub(in crate::presentation) fn on_scene_obstacle_added(
    event: On<Add, SceneObstacle>,
    mut commands: Commands,
    obstacles: Query<&SceneObstacle>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut rock_materials: ResMut<Assets<TacticalRockMaterial>>,
    asset_server: Res<AssetServer>,
) -> Result {
    let obstacle = obstacles.get(event.entity)?;
    match *obstacle {
        SceneObstacle::Tree => {
            commands
                .entity(event.entity)
                .insert(PendingTreePresentation);
        }
        SceneObstacle::Rock(recipe) => {
            commands.entity(event.entity).insert((
                Name::new("Presented tactical rock"),
                ProceduralRockVisual,
                Mesh3d(meshes.add(procedural_rock_mesh(recipe))),
                MeshMaterial3d(rock_materials.add(rock_material(recipe, &asset_server))),
            ));
        }
    }
    Ok(())
}

// The presentation facade is compiled into several binaries, while only the
// deterministic scene viewer consumes this review-specimen helper.
#[allow(dead_code)]
pub(crate) fn oak_review_terminal_specimen(
    root: Vec3,
    canopy_bps: u16,
) -> (Mesh, Mesh, Mesh, Mesh, Vec3, Vec3) {
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
        procedural_oak_textured_leaf_mesh(&specimen_leaves),
        procedural_oak_leaf_card_mesh(&specimen_leaves),
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
