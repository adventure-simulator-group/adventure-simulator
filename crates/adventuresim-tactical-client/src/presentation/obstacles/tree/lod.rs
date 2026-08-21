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

/// Identifies a streamed playable-tree representation for benchmark isolation.
/// These markers never appear on understory or review-tree foliage, even where
/// those systems share a leaf representation component.
#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct PlayableTreeDetailedLeaves;

#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct PlayableTreeTrunk;

#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct PlayableTreeDetailedWood;

#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct PlayableTreeAggregateWood;

#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct PlayableTreeBuds;

#[derive(Component, Clone, Copy, Debug, Default)]
pub(crate) struct PlayableTreeCanopyCard;

/// Benchmark-only rendering isolation applied after production LOD selection.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub(crate) struct TacticalTreeBenchmarkIsolation {
    pub(crate) hide_detailed_leaves: bool,
    pub(crate) hide_canopy_cards: bool,
    pub(crate) hide_buds: bool,
    pub(crate) hide_trunks: bool,
    pub(crate) hide_branches: bool,
}

#[derive(Resource, Clone, Copy, Default)]
pub(crate) struct TreeLodRenderOverride {
    pub(crate) lod: Option<u8>,
    pub(crate) leaf: Option<TreeLeafRepresentation>,
    pub(crate) projected_scale: Option<f32>,
}

pub(in crate::presentation) fn update_tree_projected_lod_ranges(
    cameras: Query<(&Camera, &Projection), With<Camera3d>>,
    lod_override: Res<TreeLodRenderOverride>,
    isolation: Res<TacticalTreeBenchmarkIsolation>,
    mut lods: Query<(
        &TreeLod,
        Option<&TreeLodCluster>,
        Option<&TreeLeafRepresentation>,
        Has<PlayableTreeDetailedLeaves>,
        Has<PlayableTreeDetailedWood>,
        Has<PlayableTreeAggregateWood>,
        Has<PlayableTreeBuds>,
        Has<PlayableTreeCanopyCard>,
        &mut VisibilityRange,
        &mut Visibility,
    )>,
    mut trunks: Query<
        (&mut VisibilityRange, &mut Visibility),
        (
            With<TreeTrunkLod>,
            With<PlayableTreeTrunk>,
            Without<TreeLod>,
        ),
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
    for (
        lod,
        cluster,
        leaf_representation,
        detailed_leaves,
        detailed_wood,
        aggregate_wood,
        buds,
        canopy_card,
        mut range,
        mut visibility,
    ) in &mut lods
    {
        let (next_range, next_visibility) = if let Some(forced_lod) = lod_override.lod {
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
            (
                VisibilityRange::abrupt(0.0, f32::MAX),
                if lod.0 == forced_lod && selected_leaf {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
            )
        } else {
            let radius = cluster.map_or(3.5, |cluster| {
                debug_assert!(cluster.center.is_finite());
                debug_assert!(cluster.primary_group < TREE_PRIMARY_GROUP_COUNT);
                cluster.radius
            });
            (
                match leaf_representation {
                    Some(TreeLeafRepresentation::TexturedMesh) => tree_leaf_visibility(
                        TreeLeafRepresentation::TexturedMesh,
                        focal_scale,
                        radius,
                    ),
                    Some(TreeLeafRepresentation::AlphaCard) => {
                        tree_leaf_visibility(TreeLeafRepresentation::AlphaCard, focal_scale, radius)
                    }
                    None => tree_projected_lod_visibility(lod.0, focal_scale, radius),
                },
                Visibility::Inherited,
            )
        };
        let isolated = (isolation.hide_detailed_leaves && detailed_leaves)
            || (isolation.hide_canopy_cards && canopy_card)
            || (isolation.hide_buds && buds)
            || (isolation.hide_branches && (detailed_wood || aggregate_wood));
        let next_visibility = isolated
            .then_some(Visibility::Hidden)
            .unwrap_or(next_visibility);
        if *range != next_range {
            *range = next_range;
        }
        if *visibility != next_visibility {
            *visibility = next_visibility;
        }
    }
    for (mut range, mut visibility) in &mut trunks {
        let (next_range, next_visibility) = if let Some(forced_lod) = lod_override.lod {
            (
                VisibilityRange::abrupt(0.0, f32::MAX),
                if forced_lod < 4 {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
            )
        } else {
            let end = tree_projected_lod_visibility(3, focal_scale, 3.5).end_margin;
            (
                VisibilityRange {
                    start_margin: 0.0..0.0,
                    end_margin: end,
                    use_aabb: true,
                },
                Visibility::Inherited,
            )
        };
        let next_visibility = isolation
            .hide_trunks
            .then_some(Visibility::Hidden)
            .unwrap_or(next_visibility);
        if *range != next_range {
            *range = next_range;
        }
        if *visibility != next_visibility {
            *visibility = next_visibility;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_second_update_keeps_lod_component_ticks_cold() {
        let mut app = App::new();
        app.init_resource::<TreeLodRenderOverride>()
            .init_resource::<TacticalTreeBenchmarkIsolation>()
            .add_systems(Update, update_tree_projected_lod_ranges);
        app.world_mut().spawn((
            Camera::default(),
            Camera3d::default(),
            Projection::Perspective(PerspectiveProjection {
                fov: 80.0_f32.to_radians(),
                ..default()
            }),
        ));
        let lod = app
            .world_mut()
            .spawn((
                TreeLod(2),
                PlayableTreeCanopyCard,
                VisibilityRange::abrupt(0.0, f32::MAX),
                Visibility::Hidden,
            ))
            .id();
        let trunk = app
            .world_mut()
            .spawn((
                TreeTrunkLod,
                PlayableTreeTrunk,
                VisibilityRange::abrupt(0.0, f32::MAX),
                Visibility::Hidden,
            ))
            .id();

        app.update();
        app.world_mut().clear_trackers();
        app.update();

        for entity in [lod, trunk] {
            let entity_ref = app.world().entity(entity);
            assert!(
                !entity_ref
                    .get_ref::<VisibilityRange>()
                    .unwrap()
                    .is_changed()
            );
            assert!(!entity_ref.get_ref::<Visibility>().unwrap().is_changed());
        }
    }

    #[test]
    fn benchmark_isolation_separates_tree_representation_families() {
        let mut app = App::new();
        app.init_resource::<TreeLodRenderOverride>()
            .insert_resource(TacticalTreeBenchmarkIsolation {
                hide_detailed_leaves: false,
                hide_canopy_cards: false,
                hide_buds: false,
                hide_trunks: false,
                hide_branches: true,
            })
            .add_systems(Update, update_tree_projected_lod_ranges);
        app.world_mut().spawn((
            Camera::default(),
            Camera3d::default(),
            Projection::Perspective(PerspectiveProjection {
                fov: 80.0_f32.to_radians(),
                ..default()
            }),
        ));
        let leaves = app
            .world_mut()
            .spawn((
                TreeLod(1),
                PlayableTreeDetailedLeaves,
                VisibilityRange::abrupt(0.0, f32::MAX),
                Visibility::Inherited,
            ))
            .id();
        let detailed_wood = app
            .world_mut()
            .spawn((
                TreeLod(1),
                PlayableTreeDetailedWood,
                VisibilityRange::abrupt(0.0, f32::MAX),
                Visibility::Inherited,
            ))
            .id();
        let aggregate_wood = app
            .world_mut()
            .spawn((
                TreeLod(1),
                PlayableTreeAggregateWood,
                VisibilityRange::abrupt(0.0, f32::MAX),
                Visibility::Inherited,
            ))
            .id();
        let buds = app
            .world_mut()
            .spawn((
                TreeLod(1),
                PlayableTreeBuds,
                VisibilityRange::abrupt(0.0, f32::MAX),
                Visibility::Inherited,
            ))
            .id();
        let canopy_card = app
            .world_mut()
            .spawn((
                TreeLod(1),
                PlayableTreeCanopyCard,
                VisibilityRange::abrupt(0.0, f32::MAX),
                Visibility::Inherited,
            ))
            .id();
        let trunk = app
            .world_mut()
            .spawn((
                TreeTrunkLod,
                PlayableTreeTrunk,
                VisibilityRange::abrupt(0.0, f32::MAX),
                Visibility::Inherited,
            ))
            .id();

        app.update();

        for entity in [detailed_wood, aggregate_wood] {
            assert_eq!(
                *app.world().entity(entity).get::<Visibility>().unwrap(),
                Visibility::Hidden
            );
        }
        for entity in [leaves, buds, canopy_card, trunk] {
            assert_eq!(
                *app.world().entity(entity).get::<Visibility>().unwrap(),
                Visibility::Inherited
            );
        }

        let mut isolation = app
            .world_mut()
            .resource_mut::<TacticalTreeBenchmarkIsolation>();
        isolation.hide_detailed_leaves = true;
        isolation.hide_canopy_cards = true;
        isolation.hide_buds = true;
        isolation.hide_trunks = true;
        drop(isolation);
        app.update();

        for entity in [
            leaves,
            detailed_wood,
            aggregate_wood,
            buds,
            canopy_card,
            trunk,
        ] {
            assert_eq!(
                *app.world().entity(entity).get::<Visibility>().unwrap(),
                Visibility::Hidden
            );
        }
    }
}
