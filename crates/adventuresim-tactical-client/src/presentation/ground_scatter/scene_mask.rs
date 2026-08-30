use adventuresim_tactical_core::prelude::{
    FaultScarpRecipe, GroundCover, GroundSubstrate, GroundSurface, SceneEnvironment, SceneGround,
    SceneId, SceneTerrain, TerrainTransitionCollar,
};
use bevy::prelude::*;

use super::GroundScatterPresented;

pub(super) type GroundScatterSceneQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static SceneId,
        &'static SceneTerrain,
        &'static SceneGround,
        &'static SceneEnvironment,
        Option<&'static FaultScarpRecipe>,
    ),
    Without<GroundScatterPresented>,
>;

#[cfg(all(feature = "instanced-grass", not(target_family = "wasm")))]
pub(super) type InstancedGrassSceneQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static SceneId,
        &'static SceneTerrain,
        &'static SceneGround,
        &'static SceneEnvironment,
        Option<&'static FaultScarpRecipe>,
    ),
    Without<super::instanced_grass::InstancedGrassPresented>,
>;

pub(super) fn scatter_ground_without_patch(
    ground: &SceneGround,
    collar: TerrainTransitionCollar,
) -> SceneGround {
    let half_size = Vec2::new(ground.width(), ground.depth()) * 0.5;
    let mut samples = ground.samples().to_vec();
    for z in 0..ground.grid_depth() {
        for x in 0..ground.grid_width() {
            let point = Vec2::new(x as f32, z as f32) * ground.grid_scale() - half_size;
            if collar.contains(point) {
                samples[z * ground.grid_width() + x] = GroundSurface {
                    substrate: GroundSubstrate::Water,
                    cover: GroundCover::Bare,
                    cover_density_bps: 0,
                    cover_height_cm: 0,
                };
            }
        }
    }
    SceneGround::from_samples(
        ground.grid_width(),
        ground.grid_depth(),
        ground.grid_scale(),
        samples,
    )
    .expect("masking scatter preserves the validated ground grid")
}
