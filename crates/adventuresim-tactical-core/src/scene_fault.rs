use crate::{
    scene::SceneTerrain,
    scene_input::{SceneInputError, TerrainSampleGrid},
    volumetric_terrain::{SceneTerrainPatch, TerrainLandformRecipe, terrain_landform_patch},
};

pub(crate) fn validate(
    recipe: Option<TerrainLandformRecipe>,
    playable: &TerrainSampleGrid,
) -> Result<(), SceneInputError> {
    let Some(recipe) = recipe else {
        return Ok(());
    };
    let terrain = SceneTerrain::from_heightmap(
        usize::from(playable.width),
        usize::from(playable.depth),
        playable.spacing_metres,
        playable.heights_metres.clone(),
    )
    .ok_or_else(|| SceneInputError::Validation("playable heightmap is invalid".into()))?;
    recipe
        .validate(&terrain)
        .map_err(|reason| SceneInputError::Validation(reason.into()))
}

pub(crate) fn generate(
    recipe: Option<TerrainLandformRecipe>,
    terrain: &SceneTerrain,
) -> Result<Option<SceneTerrainPatch>, SceneInputError> {
    recipe
        .map(|recipe| terrain_landform_patch(terrain, recipe))
        .transpose()
        .map_err(|reason| SceneInputError::Validation(reason.into()))
}
