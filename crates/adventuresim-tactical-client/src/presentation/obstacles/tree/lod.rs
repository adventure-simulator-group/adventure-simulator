use super::TREE_PRIMARY_GROUP_COUNT;
use super::impostor::{tree_leaf_visibility, tree_projected_lod_visibility};
use bevy::{camera::visibility::VisibilityRange, prelude::*};

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TreeLod(pub(crate) u8);

#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct TreeLodCluster {
    pub(crate) primary_group: u8,
    pub(crate) center: Vec3,
    pub(crate) radius: f32,
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TreeLeafRepresentation {
    TexturedMesh,
    AlphaCard,
}

#[derive(Component)]
pub(crate) struct TreeTrunkLod;

#[derive(Resource, Clone, Copy, Default)]
pub(crate) struct TreeLodRenderOverride {
    pub(crate) lod: Option<u8>,
    pub(crate) leaf: Option<TreeLeafRepresentation>,
    pub(crate) projected_scale: Option<f32>,
}

pub(in crate::presentation) fn update_tree_projected_lod_ranges(
    cameras: Query<(&Camera, &Projection), With<Camera3d>>,
    lod_override: Res<TreeLodRenderOverride>,
    mut lods: Query<(
        &TreeLod,
        Option<&TreeLodCluster>,
        Option<&TreeLeafRepresentation>,
        &mut VisibilityRange,
        &mut Visibility,
    )>,
    mut trunks: Query<
        (&mut VisibilityRange, &mut Visibility),
        (With<TreeTrunkLod>, Without<TreeLod>),
    >,
) {
    let Ok((camera, projection)) = cameras.single() else {
        return;
    };
    let viewport_height = camera
        .physical_viewport_size()
        .map_or(720.0, |size| size.y as f32);
    let reference_focal = 720.0 / (2.0 * (80.0_f32.to_radians() * 0.5).tan());
    let focal = match projection {
        Projection::Perspective(perspective) => {
            viewport_height / (2.0 * (perspective.fov * 0.5).tan())
        }
        _ => reference_focal,
    };
    let focal_scale =
        (focal / reference_focal * lod_override.projected_scale.unwrap_or(1.0)).clamp(0.25, 4.0);
    for (lod, cluster, leaf_representation, mut range, mut visibility) in &mut lods {
        if let Some(forced_lod) = lod_override.lod {
            let selected_leaf = match (leaf_representation, lod_override.leaf) {
                (
                    Some(TreeLeafRepresentation::TexturedMesh),
                    None | Some(TreeLeafRepresentation::TexturedMesh),
                ) => true,
                (
                    Some(TreeLeafRepresentation::AlphaCard),
                    Some(TreeLeafRepresentation::AlphaCard),
                ) => true,
                (Some(_), _) => false,
                (None, _) => true,
            };
            *visibility = if lod.0 == forced_lod && selected_leaf {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            *range = VisibilityRange::abrupt(0.0, f32::MAX);
        } else {
            *visibility = Visibility::Inherited;
            let radius = cluster.map_or(3.5, |cluster| {
                debug_assert!(cluster.center.is_finite());
                debug_assert!(cluster.primary_group < TREE_PRIMARY_GROUP_COUNT);
                cluster.radius
            });
            *range = match leaf_representation {
                Some(TreeLeafRepresentation::TexturedMesh) => {
                    tree_leaf_visibility(TreeLeafRepresentation::TexturedMesh, focal_scale, radius)
                }
                Some(TreeLeafRepresentation::AlphaCard) => {
                    tree_leaf_visibility(TreeLeafRepresentation::AlphaCard, focal_scale, radius)
                }
                None => tree_projected_lod_visibility(lod.0, focal_scale, radius),
            };
        }
    }
    for (mut range, mut visibility) in &mut trunks {
        if let Some(forced_lod) = lod_override.lod {
            *visibility = if forced_lod < 4 {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            *range = VisibilityRange::abrupt(0.0, f32::MAX);
        } else {
            *visibility = Visibility::Inherited;
            let end = tree_projected_lod_visibility(3, focal_scale, 3.5).end_margin;
            *range = VisibilityRange {
                start_margin: 0.0..0.0,
                end_margin: end,
                use_aabb: true,
            };
        }
    }
}
