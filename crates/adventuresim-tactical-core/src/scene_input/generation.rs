use super::*;

impl TacticalSceneInput {
    pub fn generate(&self) -> Result<GeneratedTacticalScene, SceneInputError> {
        self.validate()?;
        let (grid_width, grid_depth, grid_spacing, mut heights, mut environment) =
            upsample_playable_grid(&self.playable);
        let mut repairs = prepare_terrain(
            self,
            grid_width,
            grid_depth,
            grid_spacing,
            &mut heights,
            &mut environment,
        );
        let mut buildings = buildings::prepare_buildings(&self.buildings)?;
        buildings::validate_building_pads(&buildings)?;
        let (building_pads, levelled_building_samples) = buildings::level_building_pads(
            grid_width,
            grid_depth,
            grid_spacing,
            &mut heights,
            &mut buildings,
        );
        repairs.levelled_building_samples = levelled_building_samples;
        let coarse_terrain =
            SceneTerrain::from_heightmap(grid_width, grid_depth, grid_spacing, heights)
                .ok_or_else(|| {
                    SceneInputError::Validation("playable heightmap is invalid".into())
                })?;
        let mut obstacles = generated_obstacles(self);
        remove_reserved_obstacles(self, &mut obstacles, &mut repairs);
        remove_building_obstacles(
            self,
            &coarse_terrain,
            &building_pads,
            &mut obstacles,
            &mut repairs,
        );
        let ground = build_scene_ground(
            grid_width,
            grid_depth,
            grid_spacing,
            &environment,
            &coarse_terrain,
            &obstacles,
            self.playable.spacing_metres,
            &building_pads,
            &self.streets,
            &self.yards,
        )?;
        let terrain = refine_authoritative_terrain(
            self.seed,
            &coarse_terrain,
            &ground,
            &obstacles,
            self.playable.spacing_metres,
            self.weather.ground_moisture_bps,
            &building_pads,
        )?;
        let terrain_patch = crate::scene_fault::generate(self.fault_scarp, &terrain)?;
        Ok(GeneratedTacticalScene {
            digest: self.digest()?,
            terrain,
            ground,
            obstacles,
            terrain_patch,
            buildings,
            repairs,
        })
    }
}

fn prepare_terrain(
    input: &TacticalSceneInput,
    width: usize,
    depth: usize,
    spacing: f32,
    heights: &mut [f32],
    environment: &mut [EnvironmentalSample],
) -> SceneRepairReport {
    let upsampled_height_samples = heights
        .len()
        .saturating_sub(input.playable.heights_metres.len())
        as u32;
    let microrelief_adjusted_samples =
        add_authoritative_microrelief(input.seed, width, depth, spacing, heights, environment);
    let mut repairs = repair_playable_terrain(width, depth, spacing, heights, environment);
    repairs.upsampled_height_samples = upsampled_height_samples;
    repairs.microrelief_adjusted_samples = microrelief_adjusted_samples;
    repairs
}

fn generated_obstacles(input: &TacticalSceneInput) -> Vec<GeneratedObstacle> {
    input
        .playable
        .environment
        .iter()
        .enumerate()
        .filter_map(|(index, sample)| {
            let x = (index % usize::from(input.playable.width)) as u16;
            let z = (index / usize::from(input.playable.width)) as u16;
            let coordinate = ((x as u64) << 32) ^ z as u64;
            let tree_roll = splitmix64(input.seed ^ coordinate) % u64::from(BASIS_POINTS_PER_WHOLE);
            let rock_seed = splitmix64(input.seed ^ coordinate ^ ROCK_PLACEMENT_DOMAIN);
            let rock_roll = rock_seed % u64::from(BASIS_POINTS_PER_WHOLE);
            if tree_roll < u64::from(sample.canopy_bps) / 12 {
                Some(GeneratedObstacle::Tree { x, z })
            } else if rock_roll < u64::from(sample.hilly_bps) / 20 && sample.water_bps < 5_000 {
                Some(GeneratedObstacle::Rock {
                    x,
                    z,
                    recipe: rock_recipe(rock_seed),
                })
            } else {
                None
            }
        })
        .collect()
}

fn remove_reserved_obstacles(
    input: &TacticalSceneInput,
    obstacles: &mut Vec<GeneratedObstacle>,
    repairs: &mut SceneRepairReport,
) {
    let before = obstacles.len();
    obstacles.retain(|obstacle| {
        let width = usize::from(input.playable.width);
        let depth = usize::from(input.playable.depth);
        match *obstacle {
            GeneratedObstacle::Tree { x, z } => {
                !is_tree_camera_clearance_cell(usize::from(x), usize::from(z), depth)
            }
            GeneratedObstacle::Rock { x, z, .. } => {
                !is_reserved_playability_cell(usize::from(x), usize::from(z), width, depth)
            }
        }
    });
    repairs.removed_corridor_obstacles = (before - obstacles.len()) as u32;
}

fn remove_building_obstacles(
    input: &TacticalSceneInput,
    terrain: &SceneTerrain,
    building_pads: &[buildings::BuildingPad],
    obstacles: &mut Vec<GeneratedObstacle>,
    repairs: &mut SceneRepairReport,
) {
    let before = obstacles.len();
    let terrain_extent = bevy::math::Vec2::new(terrain.width(), terrain.depth());
    obstacles.retain(|obstacle| {
        !buildings::obstacle_intersects_building(
            *obstacle,
            input.playable.spacing_metres,
            terrain_extent,
            building_pads,
        )
    });
    repairs.removed_building_obstacles = (before - obstacles.len()) as u32;
}
