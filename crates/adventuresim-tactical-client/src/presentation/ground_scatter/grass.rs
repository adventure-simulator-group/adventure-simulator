use adventuresim_tactical_core::prelude::{
    EnvironmentalSample, GroundCover, SceneEnvironment, SceneGround, SceneTerrain, TacticalSurface,
    TerrainPatchRecipe,
};
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::VisibilityRange,
    color::ColorToComponents,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    prelude::{
        Color, Commands, Handle, Image, Mesh, Mesh3d, MeshMaterial3d, Name, Quat, Transform, Vec2,
        Vec3, Vec4,
    },
};

use crate::presentation::{bps, splitmix64, unit_hash};

use super::{GroundScatterLayer, TacticalFoliageMaterial, foliage_material};

pub(super) struct Assets {
    pub community_meshes: [CommunityMeshes; GrassCommunity::COUNT],
    pub near_material: Handle<TacticalFoliageMaterial>,
    pub far_material: Handle<TacticalFoliageMaterial>,
    pub vista_material: Handle<TacticalFoliageMaterial>,
}

pub(super) struct CommunityMeshes {
    pub near: Handle<Mesh>,
    pub far: Handle<Mesh>,
    pub vista: Handle<Mesh>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::presentation) enum GrassCommunity {
    /// Fertile lowland hay meadow: tall false oat-grass with coarse cocksfoot clumps.
    MesicMeadow,
    /// Leaner or more exposed turf: fine red fescue with airy common bent.
    LeanSward,
    /// Damp meadow and wet woodland gap: tufted hair-grass with Yorkshire fog.
    WetTussock,
}

impl GrassCommunity {
    pub(in crate::presentation) const ALL: [Self; 3] =
        [Self::MesicMeadow, Self::LeanSward, Self::WetTussock];
    pub(in crate::presentation) const COUNT: usize = Self::ALL.len();

    const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug)]
pub(in crate::presentation) struct GrassCommunityProfile {
    weights: [f32; GrassCommunity::COUNT],
}

impl GrassCommunityProfile {
    pub(in crate::presentation) fn from_environment(environment: &SceneEnvironment) -> Self {
        let wet = (bps(environment.wetland_bps)
            + bps(environment.water_bps) * 0.55
            + bps(environment.weather.ground_moisture_bps) * 0.35)
            .clamp(0.0, 1.0);
        let exposed = (bps(environment.hilly_bps) * 0.72
            + (1.0 - bps(environment.cultivation_bps)) * 0.18)
            .clamp(0.0, 1.0);
        Self::from_site_drivers(wet, exposed)
    }

    fn from_site_drivers(wet: f32, exposed: f32) -> Self {
        let wet = wet.clamp(0.0, 1.0);
        let exposed = exposed.clamp(0.0, 1.0);
        let mesic = (1.0 - wet) * (1.0 - exposed * 0.55);
        let lean = exposed * (1.0 - wet * 0.65);
        // A dry scene has no token wet-tussock cells. Deschampsia and
        // Yorkshire fog appear only once the available moisture signal is
        // material, while mesic and lean communities trade off continuously.
        let wet_tussock = (wet - 0.18).max(0.0) * 1.25;
        Self {
            weights: [mesic.max(0.001), lean, wet_tussock],
        }
    }

    pub(in crate::presentation) fn localized(self, sample: EnvironmentalSample) -> Self {
        let surface_wet = match sample.surface {
            TacticalSurface::Water | TacticalSurface::Wetland => 1.0,
            _ => 0.0,
        };
        let wet = (bps(sample.wetland_bps) + bps(sample.water_bps) * 0.7 + surface_wet * 0.7)
            .clamp(0.0, 1.0);
        let exposed = (bps(sample.hilly_bps) * 0.8 + (1.0 - bps(sample.cultivation_bps)) * 0.12)
            .clamp(0.0, 1.0);
        let local = Self::from_site_drivers(wet, exposed);
        Self {
            weights: core::array::from_fn(|index| {
                self.weights[index] * 0.35 + local.weights[index] * 0.65
            }),
        }
    }

    fn select(self, roll: f32, site_hash: u64) -> GrassCommunity {
        // Stable low-frequency pseudo-fields stand in for finer soil data we
        // do not yet have. They modulate, but never invent, a habitat that the
        // scene/local environmental sample assigned zero weight.
        let moisture_field = 0.68 + unit_hash(splitmix64(site_hash ^ 0x6d6f_6973_7475)) * 0.64;
        let exposure_field = 0.68 + unit_hash(splitmix64(site_hash ^ 0x6578_706f_7375)) * 0.64;
        let fertility_field = 0.76 + unit_hash(splitmix64(site_hash ^ 0x6665_7274_696c)) * 0.48;
        let weights = [
            self.weights[0] * fertility_field,
            self.weights[1] * exposure_field,
            self.weights[2] * moisture_field,
        ];
        let total = weights.iter().sum::<f32>().max(f32::EPSILON);
        let target = roll * total;
        if target < weights[0] {
            GrassCommunity::MesicMeadow
        } else if target < weights[0] + weights[1] {
            GrassCommunity::LeanSward
        } else {
            GrassCommunity::WetTussock
        }
    }
}

pub(in crate::presentation) fn grass_community_at(
    point: Vec2,
    seed: u64,
    profile: GrassCommunityProfile,
) -> GrassCommunity {
    // Jittered Voronoi cells create coherent 12-40 m sward communities. A
    // patch selects one community; species vary within that community rather
    // than becoming independent blade-by-blade confetti.
    const CELL_SIZE: f32 = 24.0;
    let cell = (point / CELL_SIZE).floor().as_ivec2();
    let mut nearest_distance = f32::INFINITY;
    let mut nearest_hash = 0;
    for offset_z in -1..=1 {
        for offset_x in -1..=1 {
            let candidate = cell + bevy::math::IVec2::new(offset_x, offset_z);
            let key = ((candidate.x as u32 as u64) << 32) | candidate.y as u32 as u64;
            let hash = splitmix64(seed ^ key ^ 0x6772_6173_735f_636f);
            let site = (candidate.as_vec2()
                + Vec2::new(
                    0.18 + unit_hash(hash) * 0.64,
                    0.18 + unit_hash(splitmix64(hash)) * 0.64,
                ))
                * CELL_SIZE;
            let distance = point.distance_squared(site);
            if distance < nearest_distance {
                nearest_distance = distance;
                nearest_hash = hash;
            }
        }
    }
    profile.select(
        unit_hash(splitmix64(nearest_hash ^ 0x7377_6172_645f_7479)),
        nearest_hash,
    )
}

pub(super) fn spawn(
    commands: &mut Commands,
    terrain: &SceneTerrain,
    ground: &SceneGround,
    terrain_patches: &[TerrainPatchRecipe],
    base_seed: u64,
    profile: GrassCommunityProfile,
    assets: &Assets,
) {
    let half_x = terrain.width() * 0.5;
    let half_z = terrain.depth() * 0.5;
    let minimum_x = (-half_x / GRASS_PATCH_SPACING).floor() as i32;
    let maximum_x = (half_x / GRASS_PATCH_SPACING).ceil() as i32;
    let minimum_z = (-half_z / GRASS_PATCH_SPACING).floor() as i32;
    let maximum_z = (half_z / GRASS_PATCH_SPACING).ceil() as i32;
    for z in minimum_z..=maximum_z {
        for x in minimum_x..=maximum_x {
            let cell = ((x as u32 as u64) << 32) | z as u32 as u64;
            let hash = splitmix64(base_seed ^ cell);
            let jitter_x = unit_hash(splitmix64(hash ^ 0x39bd_7f21)) - 0.5;
            let jitter_z = unit_hash(splitmix64(hash ^ 0xe651_34aa)) - 0.5;
            let eligibility_world_x = (x as f32 + jitter_x * 0.24) * GRASS_PATCH_SPACING;
            let eligibility_world_z = (z as f32 + jitter_z * 0.24) * GRASS_PATCH_SPACING;
            let world_x = (x as f32 + jitter_x * GRASS_PATCH_JITTER_FRACTION) * GRASS_PATCH_SPACING;
            let world_z = (z as f32 + jitter_z * GRASS_PATCH_JITTER_FRACTION) * GRASS_PATCH_SPACING;
            let Some(transform) = grass_patch_placement(
                terrain,
                ground,
                terrain_patches,
                Vec2::new(eligibility_world_x, eligibility_world_z),
                Vec2::new(world_x, world_z),
            ) else {
                continue;
            };
            let meshes = &assets.community_meshes
                [grass_community_at(Vec2::new(world_x, world_z), base_seed, profile).index()];
            commands.spawn((
                Name::new("Tactical grass near ribbons"),
                GroundScatterLayer::Grass,
                NotShadowCaster,
                Mesh3d(meshes.near.clone()),
                MeshMaterial3d(assets.near_material.clone()),
                grass_lod_visibility(GrassMeshLod::Near),
                transform,
            ));
            commands.spawn((
                Name::new("Tactical grass far ribbons"),
                GroundScatterLayer::Grass,
                NotShadowCaster,
                Mesh3d(meshes.far.clone()),
                MeshMaterial3d(assets.far_material.clone()),
                grass_lod_visibility(GrassMeshLod::Far),
                transform,
            ));
        }
    }

    let minimum_x = (-half_x / VISTA_GRASS_PATCH_SPACING).floor() as i32;
    let maximum_x = (half_x / VISTA_GRASS_PATCH_SPACING).ceil() as i32;
    let minimum_z = (-half_z / VISTA_GRASS_PATCH_SPACING).floor() as i32;
    let maximum_z = (half_z / VISTA_GRASS_PATCH_SPACING).ceil() as i32;
    for z in minimum_z..=maximum_z {
        for x in minimum_x..=maximum_x {
            let cell = ((x as u32 as u64) << 32) | z as u32 as u64;
            let hash = splitmix64(base_seed ^ cell ^ 0x7669_7374_615f_6c6f);
            let jitter_x = unit_hash(splitmix64(hash ^ 0x39bd_7f21)) - 0.5;
            let jitter_z = unit_hash(splitmix64(hash ^ 0xe651_34aa)) - 0.5;
            let centre = Vec2::new(
                (x as f32 + jitter_x * GRASS_PATCH_JITTER_FRACTION) * VISTA_GRASS_PATCH_SPACING,
                (z as f32 + jitter_z * GRASS_PATCH_JITTER_FRACTION) * VISTA_GRASS_PATCH_SPACING,
            );
            let Some(transform) =
                grass_patch_placement(terrain, ground, terrain_patches, centre, centre)
            else {
                continue;
            };
            let meshes =
                &assets.community_meshes[grass_community_at(centre, base_seed, profile).index()];
            commands.spawn((
                Name::new("Tactical grass vista tufts"),
                GroundScatterLayer::Grass,
                NotShadowCaster,
                Mesh3d(meshes.vista.clone()),
                MeshMaterial3d(assets.vista_material.clone()),
                grass_lod_visibility(GrassMeshLod::Vista),
                transform,
            ));
        }
    }
}

// A 96 x 96 grid preserves the established macro-patch footprint while
// approaching the shoot density of a mature meadow. Density lives inside the
// shared mesh rather than in more ECS entities, so extraction and visibility
// costs remain bounded as the sward becomes substantially fuller.
const GRASS_PATCH_GRID_SIDE: usize = 96;
pub(in crate::presentation) const GRASS_PATCH_SPACING: f32 = 3.2;
const GRASS_BLADE_SPACING: f32 = 3.51 / (GRASS_PATCH_GRID_SIDE - 1) as f32;
// Keep neighbouring near-flat macro patches inside the blade footprint even
// when their deterministic centre jitter diverges in opposite directions.
const GRASS_PATCH_JITTER_FRACTION: f32 = 0.04;
const GRASS_FAR_GRID_COORDINATES: [usize; 40] = [
    0, 2, 5, 7, 10, 12, 15, 17, 19, 22, 24, 27, 29, 32, 34, 37, 39, 41, 44, 46, 49, 51, 54, 56, 58,
    61, 63, 66, 68, 71, 73, 76, 78, 80, 83, 85, 88, 90, 93, 95,
];
const GRASS_VISTA_GRID_COORDINATES: [usize; 24] = [
    0, 4, 8, 12, 17, 21, 25, 29, 33, 37, 41, 45, 50, 54, 58, 62, 66, 70, 74, 78, 83, 87, 91, 95,
];
pub(in crate::presentation) const VISTA_GRASS_PATCH_SPACING: f32 = 6.4;
pub(super) fn grass_material(
    wind_scale: f32,
    lod: GrassMeshLod,
    grass_density: f32,
    grass_dryness: f32,
    ground_mask: Handle<Image>,
    ground: &SceneGround,
) -> TacticalFoliageMaterial {
    let mut material = foliage_material(wind_scale, true);
    // Grass uses this otherwise generic meadow-variation lane as a replicated
    // environmental dryness factor. Woodland shade and wet cover retain green
    // growth; exposed low-moisture swards develop coherent senescent cohorts.
    material.shading.y = grass_dryness;
    TacticalFoliageMaterial {
        // The far mesh is still substantially reduced, but retains enough
        // shoots to keep a dense meadow from visually collapsing at distance.
        shape: Vec4::new(1.0, 0.88, 0.09, lod.width_compensation(grass_density)),
        ground_mask_transform: Vec4::new(1.0 / ground.width(), 1.0 / ground.depth(), 0.5, 0.5),
        ground_mask: Some(ground_mask),
        ..material
    }
}

pub(in crate::presentation) fn vista_grass_material(
    wind_scale: f32,
    grass_dryness: f32,
    ground_mask: Handle<Image>,
    ground_mask_transform: Vec4,
    lod: GrassMeshLod,
) -> TacticalFoliageMaterial {
    let mut material = foliage_material(wind_scale, true);
    material.shading.y = grass_dryness;
    material.shape = match lod {
        // Keep close and intermediate exterior grass optically identical to
        // the playable representation. Only its one-pixel regional coverage
        // mask differs from the playable ground-cover mask.
        GrassMeshLod::Near | GrassMeshLod::Far => {
            Vec4::new(1.0, 0.88, 0.09, lod.width_compensation(1.0))
        }
        GrassMeshLod::Vista => Vec4::new(1.0, 0.94, 0.055, lod.width_compensation(1.0)),
    };
    material.ground_mask_transform = ground_mask_transform;
    material.ground_mask = Some(ground_mask);
    material
}
fn ground_allows_grass_patch(ground: &SceneGround, centre: Vec2) -> bool {
    let half_extent = GRASS_PATCH_SPACING * 0.58;
    [-1.0, 0.0, 1.0].into_iter().any(|z| {
        [-1.0, 0.0, 1.0].into_iter().any(|x| {
            ground
                .ground_at(centre + Vec2::new(x, z) * half_extent)
                .is_some_and(|sample| sample.cover == GroundCover::TallGrass)
        })
    })
}
fn grass_patch_transform(
    terrain: &SceneTerrain,
    terrain_patches: &[TerrainPatchRecipe],
    world_x: f32,
    world_z: f32,
) -> Option<Transform> {
    let sample = Vec2::new(world_x, world_z);
    if !super::super::terrain::presented_ground_allows_scatter(
        terrain_patches,
        sample,
        GRASS_PATCH_SPACING * 0.58,
    ) {
        return None;
    }
    let height = super::super::terrain::presented_ground_height(terrain, terrain_patches, sample)?;
    let normal = super::super::terrain::presented_ground_normal(terrain, terrain_patches, sample)?;
    if normal.y < 0.72 {
        return None;
    }
    Some(
        Transform::from_xyz(world_x, height, world_z)
            .with_rotation(Quat::from_rotation_arc(Vec3::Y, normal)),
    )
}

fn grass_patch_placement(
    terrain: &SceneTerrain,
    ground: &SceneGround,
    terrain_patches: &[TerrainPatchRecipe],
    legacy_predicate_centre: Vec2,
    render_centre: Vec2,
) -> Option<Transform> {
    // The legacy centre remains a one-way count-invariance guard: a formerly
    // rejected patch stays rejected. The actual rendered centre must also be
    // legal, so reducing jitter cannot move grass into leaf litter or outside
    // a usable terrain anchor.
    if !ground_allows_grass_patch(ground, legacy_predicate_centre)
        || !ground_allows_grass_patch(ground, render_centre)
    {
        return None;
    }
    grass_patch_transform(terrain, terrain_patches, render_centre.x, render_centre.y)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::presentation) enum GrassMeshLod {
    Near,
    Far,
    /// Patch-level vista representation. Broad five-vertex tuft ribbons carry
    /// the field here instead of merely thinning the close-range blade mesh.
    Vista,
}

impl GrassMeshLod {
    fn row_heights(self) -> &'static [f32] {
        match self {
            // Seven paired rows plus a shared tip: the same fifteen-vertex
            // near ribbon used by Ghost of Tsushima's published grass design.
            Self::Near => &[0.0, 0.14, 0.29, 0.45, 0.61, 0.76, 0.9],
            // Three paired rows plus a shared tip: seven vertices at distance.
            Self::Far => &[0.0, 0.45, 0.82],
            Self::Vista => &[0.0, 0.62],
        }
    }

    fn blade_grid_indices(self, grass_density: f32) -> impl Iterator<Item = usize> {
        let coordinates: &[usize] = match self {
            Self::Near => &[],
            Self::Far => &GRASS_FAR_GRID_COORDINATES,
            Self::Vista => &GRASS_VISTA_GRID_COORDINATES,
        };
        (0..GRASS_PATCH_GRID_SIDE * GRASS_PATCH_GRID_SIDE).filter(move |index| {
            let selected_for_lod = if coordinates.is_empty() {
                true
            } else {
                let row = index / GRASS_PATCH_GRID_SIDE;
                let column = index % GRASS_PATCH_GRID_SIDE;
                coordinates.contains(&row) && coordinates.contains(&column)
            };
            selected_for_lod
                && (grass_density >= 1.0
                    || unit_hash(splitmix64(*index as u64 ^ 0x24e8_51c6_9a37_b40d)) < grass_density)
        })
    }

    fn blade_count(self, grass_density: f32) -> usize {
        self.blade_grid_indices(grass_density).count()
    }

    fn width_compensation(self, grass_density: f32) -> f32 {
        match self {
            Self::Near => return 1.0,
            // These intentionally read as broad clump silhouettes rather than
            // pretending that 576 survivors remain close-range blades.
            Self::Vista => return 2.4,
            Self::Far => {}
        }
        // Compensate only for the blades discarded by the far LOD. This keeps
        // projected cover stable through the crossfade without hiding the
        // deliberate increase in authored shoot density.
        let near_count =
            (GRASS_PATCH_GRID_SIDE * GRASS_PATCH_GRID_SIDE) as f32 * grass_density.clamp(0.0, 1.0);
        let lod_count = self.blade_count(grass_density).max(1) as f32;
        (near_count.max(1.0) / lod_count).sqrt()
    }
}

pub(in crate::presentation) fn grass_lod_visibility(lod: GrassMeshLod) -> VisibilityRange {
    match lod {
        GrassMeshLod::Near => VisibilityRange {
            start_margin: 0.0..0.0,
            end_margin: 18.0..26.0,
            use_aabb: false,
        },
        GrassMeshLod::Far => VisibilityRange {
            start_margin: 18.0..26.0,
            end_margin: 62.0..76.0,
            use_aabb: false,
        },
        GrassMeshLod::Vista => VisibilityRange {
            start_margin: 58.0..72.0,
            end_margin: 124.0..140.0,
            use_aabb: false,
        },
    }
}

pub(in crate::presentation) fn grass_patch_mesh(
    color: Color,
    lod: GrassMeshLod,
    grass_density: f32,
    community: GrassCommunity,
) -> Mesh {
    let grid_side = GRASS_PATCH_GRID_SIDE;
    let centre = (grid_side - 1) as f32 * 0.5;
    let blade_spacing = GRASS_BLADE_SPACING
        * if lod == GrassMeshLod::Vista {
            VISTA_GRASS_PATCH_SPACING / GRASS_PATCH_SPACING
        } else {
            1.0
        };
    let blades = lod
        .blade_grid_indices(grass_density)
        .map(|index| {
            let row = index / grid_side;
            let column = index % grid_side;
            let hash = splitmix64(index as u64 ^ 0x8d12_6f4a_0bc3_7791);
            let species_cell = (((column / 8) as u64) << 32) | (row / 8) as u64;
            let species_hash = splitmix64(species_cell ^ 0x7475_6674_5f63_656c);
            let clump_x = ((row as f32 * 0.47 + column as f32 * 0.19).sin()) * blade_spacing * 0.24;
            let clump_z = ((column as f32 * 0.41 - row as f32 * 0.23).sin()) * blade_spacing * 0.24;
            let jitter_x = (unit_hash(hash) - 0.5) * blade_spacing * 0.46;
            let jitter_z = (unit_hash(splitmix64(hash)) - 0.5) * blade_spacing * 0.46;
            let clump_vigor = 0.5 + 0.5 * (row as f32 * 0.31 + column as f32 * 0.17 + 0.8).sin();
            let height_scale =
                (0.50 + unit_hash(splitmix64(hash ^ 0x52a9_f131)) * 0.62 + clump_vigor * 0.20)
                    .clamp(0.50, 1.30);
            let width_scale = 0.62 + unit_hash(splitmix64(hash ^ 0x91e2_57a4)) * 0.76;
            let base_x = (column as f32 - centre) * blade_spacing;
            let base_z = (row as f32 - centre) * blade_spacing;
            let mut offset_x = base_x + jitter_x + clump_x;
            let mut offset_z = base_z + jitter_z + clump_z;
            // Boundary rows may wander outward but never inward. This retains
            // organic clumping inside the patch while mitigating gaps along
            // near-flat and ordinary sloped shared edges.
            if column == 0 {
                offset_x = offset_x.min(base_x);
            } else if column + 1 == grid_side {
                offset_x = offset_x.max(base_x);
            }
            if row == 0 {
                offset_z = offset_z.min(base_z);
            } else if row + 1 == grid_side {
                offset_z = offset_z.max(base_z);
            }
            GrassBlade {
                offset_x,
                offset_z,
                height_scale,
                width_scale,
                seed: index as u64,
                species: grass_species(community, species_hash),
            }
        })
        .collect::<Vec<_>>();
    grass_ribbon_patch_mesh(0.026, 0.82, color, lod, &blades)
}

#[derive(Clone, Copy)]
struct GrassBlade {
    offset_x: f32,
    offset_z: f32,
    height_scale: f32,
    width_scale: f32,
    seed: u64,
    species: GrassSpecies,
}

#[derive(Clone, Copy)]
struct GrassInflorescence {
    root: Vec3,
    angle: f32,
    total_height: f32,
    species: GrassSpecies,
    normal: [f32; 3],
    color: [f32; 4],
    blade_root: [f32; 2],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GrassSpecies {
    FalseOatGrass,
    Cocksfoot,
    RedFescue,
    CommonBent,
    TuftedHairGrass,
    YorkshireFog,
}

fn grass_species(community: GrassCommunity, hash: u64) -> GrassSpecies {
    let roll = unit_hash(splitmix64(hash ^ 0x7370_6563_6965_735f));
    match community {
        GrassCommunity::MesicMeadow if roll < 0.68 => GrassSpecies::FalseOatGrass,
        GrassCommunity::MesicMeadow => GrassSpecies::Cocksfoot,
        GrassCommunity::LeanSward if roll < 0.64 => GrassSpecies::RedFescue,
        GrassCommunity::LeanSward => GrassSpecies::CommonBent,
        GrassCommunity::WetTussock if roll < 0.66 => GrassSpecies::TuftedHairGrass,
        GrassCommunity::WetTussock => GrassSpecies::YorkshireFog,
    }
}

impl GrassSpecies {
    fn inflorescence_branch_count(self) -> usize {
        match self {
            Self::FalseOatGrass => 2,
            Self::Cocksfoot => 3,
            Self::CommonBent | Self::TuftedHairGrass => 4,
            Self::RedFescue | Self::YorkshireFog => 0,
        }
    }

    fn spikelets_per_branch(self) -> usize {
        match self {
            Self::Cocksfoot | Self::TuftedHairGrass => 3,
            Self::FalseOatGrass | Self::CommonBent => 2,
            Self::RedFescue | Self::YorkshireFog => 0,
        }
    }

    fn height_scale(self) -> f32 {
        match self {
            Self::FalseOatGrass => 1.24,
            Self::Cocksfoot => 0.94,
            Self::RedFescue => 0.64,
            Self::CommonBent => 0.78,
            Self::TuftedHairGrass => 1.08,
            Self::YorkshireFog => 0.82,
        }
    }

    fn width_scale(self) -> f32 {
        match self {
            Self::FalseOatGrass => 0.88,
            Self::Cocksfoot => 1.34,
            Self::RedFescue => 0.48,
            Self::CommonBent => 0.58,
            Self::TuftedHairGrass => 0.76,
            Self::YorkshireFog => 1.04,
        }
    }

    fn shoulder_scale(self, height_fraction: f32) -> f32 {
        if height_fraction < 0.76 {
            return 1.0;
        }
        match self {
            // Dense, lumpy cocksfoot panicles remain conspicuous at the last
            // paired ribbon row, while open panicles stay optically lighter.
            Self::Cocksfoot => 1.75,
            Self::FalseOatGrass | Self::TuftedHairGrass => 1.16,
            Self::CommonBent => 0.82,
            Self::RedFescue | Self::YorkshireFog => 1.0,
        }
    }

    fn pigment_scale(self) -> [f32; 3] {
        match self {
            Self::FalseOatGrass => [1.02, 1.0, 0.84],
            Self::Cocksfoot => [0.72, 0.86, 0.74],
            Self::RedFescue => [0.80, 0.86, 0.72],
            Self::CommonBent => [0.92, 0.94, 0.78],
            Self::TuftedHairGrass => [0.82, 0.90, 0.82],
            Self::YorkshireFog => [1.02, 0.94, 0.88],
        }
    }
}

fn grass_ribbon_patch_mesh(
    width: f32,
    height: f32,
    color: Color,
    lod: GrassMeshLod,
    blades: &[GrassBlade],
) -> Mesh {
    let rows = lod.row_heights();
    let vertices_per_blade = rows.len() * 2 + 1;
    let triangles_per_blade = (rows.len() - 1) * 2 + 1;
    let mut positions = Vec::with_capacity(blades.len() * vertices_per_blade);
    let mut normals = Vec::with_capacity(blades.len() * vertices_per_blade);
    let mut uvs = Vec::with_capacity(blades.len() * vertices_per_blade);
    let mut blade_roots = Vec::with_capacity(blades.len() * vertices_per_blade);
    let mut colors = Vec::with_capacity(blades.len() * vertices_per_blade);
    let mut indices = Vec::with_capacity(blades.len() * triangles_per_blade * 3);
    let mut inflorescences = Vec::new();
    let linear = color.to_linear().to_f32_array();

    for &GrassBlade {
        offset_x,
        offset_z,
        height_scale,
        width_scale,
        seed: blade_seed,
        species,
    } in blades
    {
        let root = Vec3::new(offset_x, 0.0, offset_z);
        let hash = splitmix64(blade_seed ^ 0x6c8e_9cf5_701a_d30b);
        let angle = unit_hash(hash) * core::f32::consts::TAU;
        let half_width = Vec3::new(angle.cos(), 0.0, angle.sin())
            * width
            * width_scale
            * species.width_scale()
            * 0.5;
        let normal = Vec3::Y.cross(half_width).normalize_or_zero().to_array();
        let blade_threshold = unit_hash(splitmix64(hash ^ 0x3d91_02ea_61b8_7c45));
        let age = unit_hash(splitmix64(hash ^ 0x1b47_c95a_622d_41e3));
        // Healthy blades share their species pigment. Senescent tips retain a
        // hard straw region, while blade separation comes from the material's
        // specular response rather than randomized albedo.
        let species_scale = species.pigment_scale();
        let blade_color = [
            (linear[0] * species_scale[0]).clamp(0.0, 1.0),
            (linear[1] * species_scale[1]).clamp(0.0, 1.0),
            (linear[2] * species_scale[2]).clamp(0.0, 1.0),
            blade_threshold,
        ];
        let luminance = blade_color[0] * 0.2126 + blade_color[1] * 0.7152 + blade_color[2] * 0.0722;
        let straw_color = [luminance * 1.12, luminance * 0.88, luminance * 0.42];
        let senescent = age > 0.82;
        let base = positions.len() as u32;

        for &height_fraction in rows {
            let taper =
                (1.0 - height_fraction).powf(0.72) * species.shoulder_scale(height_fraction);
            let side = half_width * taper;
            let centre =
                root + Vec3::Y * height * height_scale * species.height_scale() * height_fraction;
            positions.extend_from_slice(&[(centre - side).to_array(), (centre + side).to_array()]);
            normals.extend_from_slice(&[normal; 2]);
            uvs.extend_from_slice(&[[0.0, height_fraction], [1.0, height_fraction]]);
            blade_roots.extend_from_slice(&[[offset_x, offset_z]; 2]);
            let row_color = if senescent && height_fraction >= 0.72 {
                [
                    straw_color[0],
                    straw_color[1],
                    straw_color[2],
                    blade_threshold,
                ]
            } else {
                blade_color
            };
            colors.extend_from_slice(&[row_color; 2]);
        }
        positions
            .push((root + Vec3::Y * height * height_scale * species.height_scale()).to_array());
        normals.push(normal);
        uvs.push([0.5, 1.0]);
        blade_roots.push([offset_x, offset_z]);
        colors.push(if senescent {
            [
                straw_color[0],
                straw_color[1],
                straw_color[2],
                blade_threshold,
            ]
        } else {
            blade_color
        });

        for row in 0..rows.len() - 1 {
            let lower = base + (row * 2) as u32;
            let upper = lower + 2;
            indices.extend_from_slice(&[lower, lower + 1, upper + 1, lower, upper + 1, upper]);
        }
        let shoulder = base + ((rows.len() - 1) * 2) as u32;
        let tip = base + (vertices_per_blade - 1) as u32;
        indices.extend_from_slice(&[shoulder, shoulder + 1, tip]);

        // Seed heads are a sparse Near-only diagnostic. Far/Vista retain the
        // same optical mass with the ribbon shoulder, while close cocksfoot
        // reads as compact offset clusters and oat/bent/hair-grass as open
        // panicles. Only about one shoot in eight bears one, keeping the cost
        // bounded and avoiding sub-pixel triangles at distance.
        let branch_count = species.inflorescence_branch_count();
        if lod == GrassMeshLod::Near
            && branch_count > 0
            && unit_hash(splitmix64(hash ^ 0x7061_6e69_636c_65)) < 0.125
        {
            let total_height = height * height_scale * species.height_scale();
            inflorescences.push(GrassInflorescence {
                root,
                angle,
                total_height,
                species,
                normal,
                color: blade_color,
                blade_root: [offset_x, offset_z],
            });
        }
    }

    for GrassInflorescence {
        root,
        angle,
        total_height,
        species,
        normal,
        color,
        blade_root,
    } in inflorescences
    {
        let compact = species == GrassSpecies::Cocksfoot;
        let branch_count = species.inflorescence_branch_count();
        let stem_side = Vec3::new(angle.cos(), 0.0, angle.sin()) * 0.0025;
        for (start_fraction, end_fraction) in [(0.62, 0.82), (0.82, 1.03)] {
            let start = root + Vec3::Y * total_height * start_fraction;
            let end = root + Vec3::Y * total_height * end_fraction;
            append_attached_quad(
                &mut positions,
                &mut normals,
                &mut uvs,
                &mut blade_roots,
                &mut colors,
                &mut indices,
                start - stem_side,
                start + stem_side,
                end - stem_side * 0.62,
                end + stem_side * 0.62,
                normal,
                color,
                blade_root,
                start.y,
                start_fraction,
            );
        }

        for branch in 0..branch_count {
            let fraction = branch as f32 / branch_count as f32;
            let side_sign = if branch % 2 == 0 { 1.0 } else { -1.0 };
            let branch_angle = angle + side_sign * (0.72 + fraction * 0.38);
            let direction = Vec3::new(branch_angle.cos(), 0.0, branch_angle.sin());
            let length = if compact {
                0.035 + fraction * 0.022
            } else {
                0.075 + fraction * 0.055
            };
            let thickness = if compact { 0.010 } else { 0.005 };
            let start = root
                + Vec3::Y
                    * total_height
                    * (if compact {
                        0.78 + fraction * 0.045
                    } else {
                        0.70 + fraction * 0.055
                    });
            let end = start
                + direction * length
                + Vec3::Y * total_height * if compact { 0.018 } else { 0.045 };
            let side = Vec3::new(-direction.z, 0.0, direction.x) * thickness;
            let attachment_fraction = start.y / total_height;
            append_attached_quad(
                &mut positions,
                &mut normals,
                &mut uvs,
                &mut blade_roots,
                &mut colors,
                &mut indices,
                start - side,
                start + side,
                end - side * 0.55,
                end + side * 0.55,
                normal,
                color,
                blade_root,
                start.y,
                attachment_fraction,
            );

            for spikelet in 0..species.spikelets_per_branch() {
                let along = if compact {
                    0.52 + spikelet as f32 * 0.18
                } else {
                    0.64 + spikelet as f32 * 0.27
                };
                let centre = start.lerp(end, along.min(1.0));
                append_crossed_spikelet(
                    &mut positions,
                    &mut normals,
                    &mut uvs,
                    &mut blade_roots,
                    &mut colors,
                    &mut indices,
                    centre,
                    direction,
                    if compact { 0.008 } else { 0.005 },
                    if compact { 0.014 } else { 0.010 },
                    color,
                    blade_root,
                    start.y,
                    attachment_fraction,
                );
            }
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, blade_roots);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[allow(clippy::too_many_arguments)]
fn append_attached_quad(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    blade_roots: &mut Vec<[f32; 2]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
    lower_left: Vec3,
    lower_right: Vec3,
    upper_left: Vec3,
    upper_right: Vec3,
    normal: [f32; 3],
    color: [f32; 4],
    blade_root: [f32; 2],
    attachment_height: f32,
    attachment_fraction: f32,
) {
    let base = positions.len() as u32;
    positions.extend_from_slice(&[
        lower_left.to_array(),
        lower_right.to_array(),
        upper_left.to_array(),
        upper_right.to_array(),
    ]);
    normals.extend_from_slice(&[normal; 4]);
    // Negative V identifies rigid seed-head geometry. U carries its authored
    // stalk attachment height so the shader can preserve the mesh while
    // inheriting the parent shoot's deformation.
    uvs.extend_from_slice(&[[attachment_height, -attachment_fraction]; 4]);
    blade_roots.extend_from_slice(&[blade_root; 4]);
    colors.extend_from_slice(&[color; 4]);
    indices.extend_from_slice(&[base, base + 1, base + 3, base, base + 3, base + 2]);
}

#[allow(clippy::too_many_arguments)]
fn append_crossed_spikelet(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    blade_roots: &mut Vec<[f32; 2]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
    centre: Vec3,
    branch_direction: Vec3,
    half_width: f32,
    half_height: f32,
    color: [f32; 4],
    blade_root: [f32; 2],
    attachment_height: f32,
    attachment_fraction: f32,
) {
    let horizontal =
        Vec3::new(-branch_direction.z, 0.0, branch_direction.x).normalize_or_zero() * half_width;
    let along = branch_direction.normalize_or_zero() * half_width;
    for side in [horizontal, along] {
        let normal = Vec3::Y.cross(side).normalize_or_zero().to_array();
        append_attached_quad(
            positions,
            normals,
            uvs,
            blade_roots,
            colors,
            indices,
            centre - Vec3::Y * half_height,
            centre - side,
            centre + side,
            centre + Vec3::Y * half_height,
            normal,
            color,
            blade_root,
            attachment_height,
            attachment_fraction,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_tactical_core::prelude::{GroundSurface, RiverBluffRecipe};
    use bevy::{mesh::VertexAttributeValues, prelude::default};
    use std::collections::BTreeSet;

    fn patch_aware_test_recipe() -> RiverBluffRecipe {
        RiverBluffRecipe {
            seed: 7_094_698_234_423_137_900,
            center_cm: [0, 0, 0],
            yaw_milliradians: 0,
            face_width_cm: 2_800,
            face_height_cm: 900,
            rock_depth_cm: 1_400,
            curvature_cm: 420,
            undercut_depth_cm: 80,
            collapse_offset_cm: 180,
            collapse_radius_cm: 300,
            talus_depth_cm: 700,
            heightfield_error_cm: 650,
            error_tolerance_cm: 75,
            vertical_intersections: 2,
            sample_spacing_cm: 28,
        }
    }

    #[test]
    fn patch_aware_grass_uses_upper_terrace_and_omits_the_scarp() {
        let terrain = SceneTerrain::from_heightmap(41, 41, 2.0, vec![0.0; 41 * 41]).unwrap();
        let recipe = patch_aware_test_recipe();
        let patches = [TerrainPatchRecipe::RiverBluff(recipe)];
        let upper_z = recipe.maximum_face_local_z(0.0) + 2.0;
        let upper = grass_patch_transform(&terrain, &patches, 0.0, upper_z)
            .expect("upper terrace must receive patch-aware grass presentation");
        assert!(
            (upper.translation.y - recipe.local_crest_height(0.0)).abs() <= 0.05,
            "upper-terrace grass was not seated on the implicit top: {upper:?}"
        );
        let lower = grass_patch_transform(&terrain, &patches, 0.0, -10.0)
            .expect("ordinary lower ground must retain grass presentation");
        assert!(lower.translation.y.abs() <= 0.01);
        let scarp_z = recipe.face_surface_local_z(Vec3::new(0.0, 4.0, 0.0));
        assert!(
            grass_patch_transform(&terrain, &patches, 0.0, scarp_z).is_none(),
            "grass presentation must not bridge the multi-valued scarp"
        );
    }

    #[test]
    fn coherent_sward_selector_honors_habitat_profiles_deterministically() {
        let point = Vec2::new(17.0, -9.0);
        let mesic = GrassCommunityProfile {
            weights: [1.0, 0.0, 0.0],
        };
        let lean = GrassCommunityProfile {
            weights: [0.0, 1.0, 0.0],
        };
        let wet = GrassCommunityProfile {
            weights: [0.0, 0.0, 1.0],
        };
        assert_eq!(
            grass_community_at(point, 42, mesic),
            GrassCommunity::MesicMeadow
        );
        assert_eq!(
            grass_community_at(point, 42, lean),
            GrassCommunity::LeanSward
        );
        assert_eq!(
            grass_community_at(point, 42, wet),
            GrassCommunity::WetTussock
        );
        assert_eq!(
            grass_community_at(point, 42, wet),
            grass_community_at(point, 42, wet)
        );
    }

    #[test]
    fn dry_wet_and_exposed_site_drivers_change_community_availability() {
        let dry = GrassCommunityProfile::from_site_drivers(0.0, 0.15);
        let wet = GrassCommunityProfile::from_site_drivers(0.95, 0.05);
        let exposed = GrassCommunityProfile::from_site_drivers(0.05, 1.0);
        assert_eq!(dry.weights[GrassCommunity::WetTussock.index()], 0.0);
        assert!(
            wet.weights[GrassCommunity::WetTussock.index()]
                > wet.weights[GrassCommunity::MesicMeadow.index()]
        );
        assert!(
            exposed.weights[GrassCommunity::LeanSward.index()]
                > dry.weights[GrassCommunity::LeanSward.index()]
        );

        let locally_wet = dry.localized(EnvironmentalSample {
            wetland_bps: 10_000,
            surface: TacticalSurface::Wetland,
            ..EnvironmentalSample::default()
        });
        assert!(locally_wet.weights[GrassCommunity::WetTussock.index()] > 0.0);
    }

    #[test]
    fn near_far_and_vista_resolve_the_same_world_community() {
        let profile = GrassCommunityProfile::from_site_drivers(0.48, 0.42);
        let seed = 0x51a7_7eed;
        for point in [
            Vec2::new(-24.01, 11.8),
            Vec2::new(-23.99, 11.8),
            Vec2::new(0.0, 0.0),
            Vec2::new(23.99, -17.0),
            Vec2::new(24.01, -17.0),
            Vec2::new(71.5, 54.25),
        ] {
            let near = grass_community_at(point, seed, profile);
            let far = grass_community_at(point, seed, profile);
            let vista = grass_community_at(point, seed, profile);
            assert_eq!(near, far);
            assert_eq!(far, vista);
        }
    }

    #[test]
    fn grass_species_have_distinct_near_morphology_and_diagnostic_seed_heads() {
        let mesic = grass_patch_mesh(
            Color::WHITE,
            GrassMeshLod::Near,
            1.0,
            GrassCommunity::MesicMeadow,
        );
        let lean = grass_patch_mesh(
            Color::WHITE,
            GrassMeshLod::Near,
            1.0,
            GrassCommunity::LeanSward,
        );
        assert_ne!(mesic.count_vertices(), lean.count_vertices());
        assert_ne!(
            mesic.attribute(Mesh::ATTRIBUTE_POSITION),
            lean.attribute(Mesh::ATTRIBUTE_POSITION)
        );
        assert!(GrassSpecies::RedFescue.width_scale() < GrassSpecies::FalseOatGrass.width_scale());
        assert!(
            GrassSpecies::Cocksfoot.shoulder_scale(0.9)
                > GrassSpecies::FalseOatGrass.shoulder_scale(0.9)
        );
        assert_eq!(GrassSpecies::RedFescue.inflorescence_branch_count(), 0);
        assert!(
            GrassSpecies::CommonBent.inflorescence_branch_count()
                > GrassSpecies::FalseOatGrass.inflorescence_branch_count()
        );
    }

    #[test]
    fn near_seed_heads_are_crossed_clusters_with_rigid_attachment_metadata() {
        let mesh = (0..4_096)
            .find_map(|seed| {
                let mesh = grass_ribbon_patch_mesh(
                    0.026,
                    0.82,
                    Color::WHITE,
                    GrassMeshLod::Near,
                    &[GrassBlade {
                        offset_x: 0.0,
                        offset_z: 0.0,
                        height_scale: 1.0,
                        width_scale: 1.0,
                        seed,
                        species: GrassSpecies::Cocksfoot,
                    }],
                );
                (mesh.count_vertices() > 15).then_some(mesh)
            })
            .expect("the bounded seed search should find a flowering cocksfoot shoot");

        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .expect("seed heads should retain authored positions");
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("seed heads should carry attachment metadata");
        };
        let seed_head_positions = &positions[15..];
        let seed_head_uvs = &uvs[15..];

        assert!(seed_head_positions.len() >= 92);
        assert!(seed_head_uvs.iter().all(|uv| uv[0] > 0.0 && uv[1] < 0.0));
        assert!(
            seed_head_positions
                .iter()
                .any(|position| position[0].abs().max(position[2].abs()) > 0.03),
            "the nearest seed head must contain authored lateral branches"
        );
        assert_eq!(seed_head_positions.len() % 4, 0);
    }

    #[test]
    fn grass_patches_use_a_stable_reduced_far_subset() {
        let near = grass_patch_mesh(
            Color::WHITE,
            GrassMeshLod::Near,
            1.0,
            GrassCommunity::MesicMeadow,
        );
        let far = grass_patch_mesh(
            Color::WHITE,
            GrassMeshLod::Far,
            1.0,
            GrassCommunity::MesicMeadow,
        );
        let vista = grass_patch_mesh(
            Color::WHITE,
            GrassMeshLod::Vista,
            1.0,
            GrassCommunity::MesicMeadow,
        );
        let sparse = grass_patch_mesh(
            Color::WHITE,
            GrassMeshLod::Near,
            0.25,
            GrassCommunity::MesicMeadow,
        );
        let near_positions = near
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        let far_positions = far
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        assert!(near_positions.len() > 9_216 * 15);
        let near_blade_positions = &near_positions[..9_216 * 15];
        assert_eq!(far_positions.len(), 1_600 * 7);
        assert_eq!(vista.count_vertices(), 576 * 5);
        assert!(576.0 / VISTA_GRASS_PATCH_SPACING.powi(2) >= 14.0);
        assert!(near_blade_positions.len() > far_positions.len());
        assert!(far_positions.len() > vista.count_vertices());
        let sparse_positions = sparse
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        assert!(!sparse_positions.is_empty());
        assert!(sparse_positions.len() < near_positions.len());
        let Some(VertexAttributeValues::Float32x2(near_roots)) =
            near.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("grass mesh must carry stable blade roots");
        };
        let Some(VertexAttributeValues::Float32x2(far_roots)) = far.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("far grass mesh must carry stable blade roots");
        };
        assert_eq!(near_roots.len(), near_positions.len());
        let near_blade_roots = &near_roots[..9_216 * 15];
        assert_eq!(far_roots.len(), far_positions.len());
        assert!(far_roots.iter().all(|root| near_blade_roots.contains(root)));
        let Some(VertexAttributeValues::Float32x4(colors)) = near.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("grass mesh must carry stable blade thresholds");
        };
        let Some(VertexAttributeValues::Float32x4(far_colors)) =
            far.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("far grass mesh must carry stable blade thresholds");
        };
        assert!(colors.iter().all(|color| (0.0..1.0).contains(&color[3])));
        assert!(colors.iter().any(|color| color[3] < 0.25));
        assert!(colors.iter().any(|color| color[3] > 0.75));
        for (far_root, far_color) in far_roots.chunks_exact(7).zip(far_colors.chunks_exact(7)) {
            let matching_near_blade = near_blade_roots
                .chunks_exact(15)
                .position(|near_root| near_root[0] == far_root[0])
                .expect("every far blade must retain its exact near-LOD root");
            assert_eq!(
                colors[matching_near_blade * 15][3],
                far_color[0][3],
                "near and far LODs must apply the same ground-mask threshold"
            );
            assert_eq!(
                colors[matching_near_blade * 15],
                far_color[0],
                "near and far LOD roots must retain the same base pigment and age"
            );
            assert_eq!(
                colors[matching_near_blade * 15 + 14],
                far_color[6],
                "near and far LOD tips must retain the same senescent pigment"
            );
        }

        let blade_heights = near_blade_positions
            .chunks_exact(15)
            .map(|blade| {
                blade
                    .iter()
                    .map(|position| position[1])
                    .fold(0.0_f32, f32::max)
            })
            .collect::<Vec<_>>();
        let minimum_height = blade_heights.iter().copied().fold(f32::INFINITY, f32::min);
        let maximum_height = blade_heights
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            minimum_height < 0.52,
            "short blades should break the curtain silhouette"
        );
        assert!(
            maximum_height > 0.95,
            "mature blades should remain visibly taller"
        );
        assert!(maximum_height - minimum_height > 0.45);

        let blade_widths = near_blade_positions
            .chunks_exact(15)
            .map(|blade| Vec3::from_array(blade[0]).distance(Vec3::from_array(blade[1])))
            .collect::<Vec<_>>();
        let minimum_width = blade_widths.iter().copied().fold(f32::INFINITY, f32::min);
        let maximum_width = blade_widths
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(maximum_width / minimum_width > 2.0);

        let distinct_pigments = colors
            .iter()
            .map(|color| [color[0].to_bits(), color[1].to_bits(), color[2].to_bits()])
            .collect::<BTreeSet<_>>();
        assert!(distinct_pigments.len() <= 4);
        assert!(distinct_pigments.len() >= 3);
    }

    #[test]
    fn unit_scale_macro_patch_footprints_overlap_at_worst_case_near_flat_jitter() {
        let near = grass_patch_mesh(
            Color::WHITE,
            GrassMeshLod::Near,
            1.0,
            GrassCommunity::MesicMeadow,
        );
        let Some(VertexAttributeValues::Float32x2(roots)) = near.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("grass mesh must carry roots");
        };
        let min_x = roots
            .iter()
            .map(|root| root[0])
            .fold(f32::INFINITY, f32::min);
        let max_x = roots
            .iter()
            .map(|root| root[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let min_z = roots
            .iter()
            .map(|root| root[1])
            .fold(f32::INFINITY, f32::min);
        let max_z = roots
            .iter()
            .map(|root| root[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let worst_adjacent_centre_distance =
            GRASS_PATCH_SPACING * (1.0 + GRASS_PATCH_JITTER_FRACTION);
        assert!(max_x - min_x > worst_adjacent_centre_distance);
        assert!(max_z - min_z > worst_adjacent_centre_distance);

        let terrain = SceneTerrain::from_heightmap(2, 2, 1.0, vec![0.0; 4]).unwrap();
        let transform = grass_patch_transform(&terrain, &[], 0.0, 0.0).unwrap();
        assert_eq!(transform.scale, Vec3::ONE);
        assert_eq!(transform.rotation, Quat::IDENTITY);
    }

    #[test]
    fn boundary_patch_is_retained_for_per_blade_ground_masking() {
        let width = 81;
        let depth = 41;
        let mut samples = vec![GroundSurface::default(); width * depth];
        // x=1.9 m: outside the legacy footprint centred at -0.32 m, but
        // inside the actual footprint centred at 0.0 m.
        let leaf_x = 59;
        let leaf_z = 20;
        samples[leaf_z * width + leaf_x].cover = GroundCover::LeafLitter;
        let ground = SceneGround::from_samples(width, depth, 0.1, samples).unwrap();
        let terrain = SceneTerrain::from_heightmap(9, 9, 1.0, vec![0.0; 81]).unwrap();
        let legacy = Vec2::new(-0.32, 0.0);
        let rendered = Vec2::ZERO;
        assert!(ground_allows_grass_patch(&ground, legacy));
        assert!(ground_allows_grass_patch(&ground, rendered));
        assert!(grass_patch_placement(&terrain, &ground, &[], legacy, rendered).is_some());
    }

    #[test]
    fn invalid_render_anchor_is_skipped_without_legacy_fallback() {
        let terrain = SceneTerrain::from_heightmap(2, 2, 1.0, vec![0.0; 4]).unwrap();
        let ground =
            SceneGround::from_samples(81, 81, 0.1, vec![GroundSurface::default(); 81 * 81])
                .unwrap();
        assert!(grass_patch_transform(&terrain, &[], 0.0, 0.0).is_some());
        assert!(
            grass_patch_placement(&terrain, &ground, &[], Vec2::ZERO, Vec2::new(2.0, 0.0))
                .is_none()
        );
    }

    #[test]
    fn representative_slope_keeps_adjacent_boundary_rows_overlapping() {
        let heights = (0..3)
            .flat_map(|_| (0..9).map(|x| x as f32 * 0.25))
            .collect::<Vec<_>>();
        let terrain = SceneTerrain::from_heightmap(9, 3, 1.0, heights).unwrap();
        let left = grass_patch_transform(&terrain, &[], -1.6, 0.0).unwrap();
        let right = grass_patch_transform(&terrain, &[], 1.6, 0.0).unwrap();
        let near = grass_patch_mesh(
            Color::WHITE,
            GrassMeshLod::Near,
            1.0,
            GrassCommunity::MesicMeadow,
        );
        let Some(VertexAttributeValues::Float32x2(roots)) = near.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("grass mesh must carry roots");
        };
        let min_x = roots
            .iter()
            .map(|root| root[0])
            .fold(f32::INFINITY, f32::min);
        let max_x = roots
            .iter()
            .map(|root| root[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let direction = (right.translation - left.translation).normalize();
        let left_edge = left.transform_point(Vec3::new(max_x, 0.0, 0.0));
        let right_edge = right.transform_point(Vec3::new(min_x, 0.0, 0.0));
        assert!((right_edge - left_edge).dot(direction) <= 0.0);
    }

    #[test]
    fn grass_lods_crossfade_across_the_same_distance_interval() {
        let near = grass_lod_visibility(GrassMeshLod::Near);
        let far = grass_lod_visibility(GrassMeshLod::Far);
        let vista = grass_lod_visibility(GrassMeshLod::Vista);
        assert_eq!(near.end_margin, far.start_margin);
        assert!(far.end_margin.start >= vista.start_margin.start);
        assert!(far.end_margin.end >= vista.start_margin.end);
        assert!(!near.is_abrupt());
        assert!(!far.is_abrupt());
        assert!(!vista.is_abrupt());
    }

    #[test]
    fn grass_composition_reuses_existing_mask_fetch_and_preserves_topology() {
        let shader = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/shaders/tactical_foliage.wgsl"
        ));
        assert_eq!(shader.matches("textureSampleLevel(").count(), 1);
        assert!(shader.contains("let effective_coverage = ground_coverage * clump_coverage"));
        assert!(shader.contains("let edge_growth = mix(0.26, 1.0"));
        assert!(!shader.contains("let tip_age"));
        assert!(shader.contains("* mix(1.0, 0.94, mature_age)"));
        assert!(shader.contains("lean_amount + 0.012 * mature_age"));
        assert!(shader.contains("let is_inflorescence = vertex.uv.y < 0.0"));
        assert!(shader.contains("let bent_offset = rotate_between"));
        assert!(shader.contains("abs(f32(in.visibility_range_dither)) / 16.0"));
        assert!(!shader.contains("visibility_range_dither(in.position"));
        assert!(shader.contains("vec4<f32>(root_world, 1.0)"));
        assert!(shader.contains("0.60,"));
        assert!(shader.contains("foliage.shading.w > 0.5"));

        let near = grass_patch_mesh(
            Color::WHITE,
            GrassMeshLod::Near,
            1.0,
            GrassCommunity::MesicMeadow,
        );
        let far = grass_patch_mesh(
            Color::WHITE,
            GrassMeshLod::Far,
            1.0,
            GrassCommunity::MesicMeadow,
        );
        assert!(near.count_vertices() > 9_216 * 15);
        assert_eq!(far.count_vertices(), 1_600 * 7);
    }

    #[test]
    fn only_deep_leaf_litter_omits_a_grass_patch() {
        let mut samples = vec![GroundSurface::default(); 81];
        samples[40].cover = GroundCover::LeafLitter;
        let boundary = SceneGround::from_samples(9, 9, 1.0, samples).unwrap();
        assert!(ground_allows_grass_patch(&boundary, Vec2::ZERO));
        let litter = SceneGround::from_samples(
            9,
            9,
            1.0,
            vec![
                GroundSurface {
                    cover: GroundCover::LeafLitter,
                    ..default()
                };
                81
            ],
        )
        .unwrap();
        assert!(!ground_allows_grass_patch(&litter, Vec2::ZERO));
    }

    #[test]
    fn ground_foliage_enables_continuous_lod_and_interaction() {
        let grass = foliage_material(0.3, true);
        let crown = foliage_material(0.3, false);
        assert_eq!(grass.shading.w, 1.0);
        assert_eq!(crown.shading.w, 0.0);
        assert_eq!(grass.shape, Vec4::ZERO);
        assert_eq!(GrassMeshLod::Near.width_compensation(1.0), 1.0);
        assert_eq!(
            Vec4::new(1.0, 0.88, 0.09, GrassMeshLod::Near.width_compensation(1.0)),
            Vec4::new(1.0, 0.88, 0.09, 1.0)
        );
        assert_eq!(
            Vec4::new(1.0, 0.88, 0.09, GrassMeshLod::Far.width_compensation(1.0)),
            Vec4::new(1.0, 0.88, 0.09, 2.4)
        );
    }
}
