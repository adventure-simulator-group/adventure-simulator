use adventuresim_tactical_core::prelude::{
    EnvironmentalSample, GroundCover, SceneEnvironment, SceneGround, SceneTerrain, TacticalSurface,
};
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::VisibilityRange,
    color::ColorToComponents,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::{
        Color, Commands, Component, Handle, Image, Mesh, Mesh3d, MeshMaterial3d, Name, Quat,
        Transform, Vec2, Vec3, Vec4,
    },
};

use crate::presentation::{bps, splitmix64, unit_hash};

use super::{GroundScatterLayer, TacticalFoliageMaterial, foliage_material};

pub(super) struct Assets {
    pub community_meshes: [CommunityMeshes; GrassCommunity::COUNT],
    pub near_materials: GrassMaterialHandles,
    pub far_materials: GrassMaterialHandles,
    pub vista_materials: GrassMaterialHandles,
}

/// The two immutable material variants used by one grass LOD.  The selected
/// handle makes the ground-mask path uniform for a whole draw, rather than
/// requiring divergent per-blade decisions in the shader.
#[derive(Clone)]
pub(super) struct GrassMaterialHandles {
    pub boundary: Handle<TacticalFoliageMaterial>,
    pub interior: Handle<TacticalFoliageMaterial>,
}

impl GrassMaterialHandles {
    pub(super) fn for_mask_mode(
        &self,
        mask_mode: GrassGroundMaskMode,
    ) -> Handle<TacticalFoliageMaterial> {
        match mask_mode {
            GrassGroundMaskMode::Boundary => self.boundary.clone(),
            GrassGroundMaskMode::Interior => self.interior.clone(),
        }
    }
}

/// Selects the material contract used by every grass distance tier.  Keeping
/// this on the entity makes scene diagnostics prove that only the close field
/// retains player-responsive curved-ribbon reconstruction.
#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::presentation) enum GrassMaterialPath {
    FullInteractive,
    CheapLod,
}

impl GrassMaterialPath {
    pub(in crate::presentation) const fn for_lod(lod: GrassMeshLod) -> Self {
        match lod {
            GrassMeshLod::Near => Self::FullInteractive,
            // The instanced near-edge ring shades cheaply like the reduced
            // tiers; the legacy patch renderer never spawns it.
            GrassMeshLod::NearEdge | GrassMeshLod::Far | GrassMeshLod::Vista => Self::CheapLod,
        }
    }
}

/// Records whether this patch's draw may skip the authored ground-cover mask.
/// Boundary is deliberately the default: only a conservative all-grass
/// footprint is eligible for the interior material.
#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::presentation) enum GrassGroundMaskMode {
    Boundary,
    Interior,
}

impl GrassGroundMaskMode {
    const fn shader_flag(self) -> f32 {
        match self {
            Self::Boundary => 0.0,
            Self::Interior => 1.0,
        }
    }
}

pub(in crate::presentation) struct CommunityMeshes {
    near: [Handle<Mesh>; GrassTopology::COUNT],
    far: [Handle<Mesh>; GrassTopology::COUNT],
    vista: [Handle<Mesh>; GrassTopology::COUNT],
}

impl CommunityMeshes {
    pub(in crate::presentation) fn new(
        mut build: impl FnMut(GrassMeshLod, GrassTopology) -> Handle<Mesh>,
    ) -> Self {
        Self {
            near: GrassTopology::ALL.map(|topology| build(GrassMeshLod::Near, topology)),
            far: GrassTopology::ALL.map(|topology| build(GrassMeshLod::Far, topology)),
            vista: GrassTopology::ALL.map(|topology| build(GrassMeshLod::Vista, topology)),
        }
    }

    pub(in crate::presentation) fn mesh(
        &self,
        lod: GrassMeshLod,
        topology: GrassTopology,
    ) -> &Handle<Mesh> {
        match lod {
            // `NearEdge` is an instanced-only sub-tier with no patch-level mesh
            // cache; it never reaches `CommunityMeshes`, so map it to the near
            // handle to keep the match exhaustive.
            GrassMeshLod::Near | GrassMeshLod::NearEdge => &self.near[topology.index()],
            GrassMeshLod::Far => &self.far[topology.index()],
            GrassMeshLod::Vista => &self.vista[topology.index()],
        }
    }
}

/// A deterministic subset of the shared grass patch topology. The continuous
/// ground mask still rejects individual blades at authored boundaries; this
/// only avoids submitting obviously unused blades for sparsely covered patches.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(in crate::presentation) enum GrassTopology {
    Quarter,
    Half,
    ThreeQuarters,
    Full,
}

impl GrassTopology {
    pub(in crate::presentation) const ALL: [Self; 4] =
        [Self::Quarter, Self::Half, Self::ThreeQuarters, Self::Full];
    pub(in crate::presentation) const COUNT: usize = Self::ALL.len();

    pub(in crate::presentation) const fn index(self) -> usize {
        self as usize
    }

    pub(in crate::presentation) const fn density(self) -> f32 {
        match self {
            Self::Quarter => 0.25,
            Self::Half => 0.50,
            Self::ThreeQuarters => 0.75,
            Self::Full => 1.0,
        }
    }

    pub(in crate::presentation) fn for_local_coverage(coverage: f32) -> Option<Self> {
        let coverage = coverage.clamp(0.0, 1.0);
        if coverage <= f32::EPSILON {
            None
        } else if coverage <= 0.25 {
            Some(Self::Quarter)
        } else if coverage <= 0.50 {
            Some(Self::Half)
        } else if coverage <= 0.75 {
            Some(Self::ThreeQuarters)
        } else {
            Some(Self::Full)
        }
    }
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
            let Some(mut transform) = grass_patch_placement(
                terrain,
                ground,
                Vec2::new(eligibility_world_x, eligibility_world_z),
                Vec2::new(world_x, world_z),
            ) else {
                continue;
            };
            transform.rotation *= grass_patch_yaw(hash);
            let Some(topology) = grass_patch_topology(ground, Vec2::new(world_x, world_z)) else {
                continue;
            };
            let mask_mode =
                grass_ground_mask_mode(ground, Vec2::new(world_x, world_z), GrassMeshLod::Near);
            let meshes = &assets.community_meshes
                [grass_community_at(Vec2::new(world_x, world_z), base_seed, profile).index()];
            commands.spawn((
                Name::new("Tactical grass near ribbons"),
                GroundScatterLayer::Grass,
                GrassMaterialPath::for_lod(GrassMeshLod::Near),
                mask_mode,
                NotShadowCaster,
                Mesh3d(meshes.mesh(GrassMeshLod::Near, topology).clone()),
                MeshMaterial3d(assets.near_materials.for_mask_mode(mask_mode)),
                grass_lod_visibility(GrassMeshLod::Near),
                transform,
            ));
            commands.spawn((
                Name::new("Tactical grass far ribbons"),
                GroundScatterLayer::Grass,
                GrassMaterialPath::for_lod(GrassMeshLod::Far),
                mask_mode,
                NotShadowCaster,
                Mesh3d(meshes.mesh(GrassMeshLod::Far, topology).clone()),
                MeshMaterial3d(assets.far_materials.for_mask_mode(mask_mode)),
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
            let Some(mut transform) = grass_patch_placement(terrain, ground, centre, centre) else {
                continue;
            };
            transform.rotation *= grass_patch_yaw(hash);
            let Some(topology) = grass_patch_topology(ground, centre) else {
                continue;
            };
            let mask_mode = grass_ground_mask_mode(ground, centre, GrassMeshLod::Vista);
            let meshes =
                &assets.community_meshes[grass_community_at(centre, base_seed, profile).index()];
            commands.spawn((
                Name::new("Tactical grass vista tufts"),
                GroundScatterLayer::Grass,
                GrassMaterialPath::for_lod(GrassMeshLod::Vista),
                mask_mode,
                NotShadowCaster,
                Mesh3d(meshes.mesh(GrassMeshLod::Vista, topology).clone()),
                MeshMaterial3d(assets.vista_materials.for_mask_mode(mask_mode)),
                grass_lod_visibility(GrassMeshLod::Vista),
                transform,
            ));
        }
    }
}

// A 32 x 32 grid preserves the established macro-patch footprint and overlap
// while reducing the close interactive sward to the density that still reads
// as individual grass at its short 10 m range. Density lives inside the shared
// mesh rather than in more ECS entities, so extraction and visibility costs
// stay bounded.
const GRASS_PATCH_GRID_SIDE: usize = 32;
pub(in crate::presentation) const GRASS_PATCH_SPACING: f32 = 3.2;
const GRASS_BLADE_SPACING: f32 = 3.51 / (GRASS_PATCH_GRID_SIDE - 1) as f32;
// Deliberately stylized for third-person readability: the former 19 mm body
// became too thin once centreline lean stopped being misread as shader width.
// Taper still converges to one zero-width terminal vertex.
const GRASS_BLADE_WIDTH_METRES: f32 = 0.076;
// Keep neighbouring near-flat macro patches inside the blade footprint even
// when their deterministic centre jitter diverges in opposite directions. This
// is just below the 3.51 / 3.2 overlap limit.
const GRASS_PATCH_JITTER_FRACTION: f32 = 0.09;
// Far and Vista retain one deterministic Near root per square stratum. The
// hashed root within each stratum prevents the reduced meshes from exposing a
// Cartesian row pattern, while the strata preserve uniform coverage and exact
// Near-root correspondence through the LOD crossfade.
const GRASS_FAR_STRATUM_SIDE: usize = 4;
const GRASS_VISTA_STRATUM_SIDE: usize = 8;
// Bevy 0.19's render-mesh slab allocator was stable with these allocation
// footprints. Keep the old per-LOD vertex-buffer size while the index buffer
// references only the reduced physical sward.
const LEGACY_GRASS_PATCH_GRID_SIDE: usize = 64;
const LEGACY_GRASS_FAR_GRID_COORDINATES: [usize; 16] =
    [0, 4, 8, 13, 17, 21, 25, 29, 34, 38, 42, 46, 50, 55, 59, 63];
const LEGACY_GRASS_VISTA_GRID_COORDINATES: [usize; 8] = [0, 9, 18, 27, 36, 45, 54, 63];
pub(in crate::presentation) const VISTA_GRASS_PATCH_SPACING: f32 = 6.4;
/// The terrain begins replacing the physical coverage removed by the Far LOD
/// during the same dithered interval that exchanges Near blades for Far
/// survivors. This is deliberately shared with [`grass_lod_visibility`] so
/// neither representation can expose a bare circular band.
pub(in crate::presentation) const NEAR_TO_FAR_SWARD_FADE_START_METRES: f32 = 7.0;
pub(in crate::presentation) const NEAR_TO_FAR_SWARD_FADE_END_METRES: f32 = 14.0;
// The reduced Lambert path is calibrated against a matched full-PBR capture
// after both paths share the same normal bias. It retains the cheap fragment
// contract while matching the shadow-aware tier's observed daylight luminance.
const REDUCED_GRASS_LIGHTING_SCALE: f32 = 0.58;
/// Far retains an evenly spaced 8-by-8 subset of Near's 32-by-32 roots. Dense
/// Near ribbons overlap heavily in screen space, so removed root count
/// overstates the optical coverage that the terrain must replace. This
/// calibrated fraction leaves the unchanged wide Far survivors carrying part
/// of the sward instead of double-darkening the transition with solid ground.
pub(in crate::presentation) const FAR_LOD_GAP_FILL_FRACTION: f32 = 0.75;
/// The final physical-grass fade overlaps the terrain's band-limited sward.
/// Keeping these bounds here makes the playable and vista lattices share the
/// same terminal representation contract.
pub(in crate::presentation) const TERMINAL_SWARD_FADE_START_METRES: f32 = 42.0;
pub(in crate::presentation) const TERMINAL_SWARD_FADE_END_METRES: f32 = 50.0;
// `grass_cover_mask_pixels` feathers authored non-grass cover over this
// radius. The organic source lookup can warp by one cell; 1.5 cells also
// covers its rounding and bilinear filtering. Any uncertainty stays on the
// boundary material, which retains the exact texture-driven collapse.
const GRASS_MASK_FEATHER_METRES: f32 = 4.8;
const GRASS_MASK_SOURCE_WARP_GUARD_CELLS: f32 = 1.5;
pub(super) fn grass_material(
    wind_scale: f32,
    lod: GrassMeshLod,
    grass_density: f32,
    grass_dryness: f32,
    ground_mask: Handle<Image>,
    ground: &SceneGround,
    mask_mode: GrassGroundMaskMode,
) -> TacticalFoliageMaterial {
    let mut material = foliage_material(wind_scale, true);
    // Grass uses this otherwise generic meadow-variation lane as a replicated
    // environmental dryness factor. Woodland shade and wet cover retain green
    // growth; exposed low-moisture swards develop coherent senescent cohorts.
    material.shading.y = grass_dryness;
    material.shading.w = if lod == GrassMeshLod::Near { 1.0 } else { 0.0 };
    TacticalFoliageMaterial {
        // Keep close blades at their botanical width. Reduced tiers receive
        // only bounded silhouette compensation; terrain sward supplies the
        // aggregate coverage through the crossfade.
        shape: Vec4::new(1.0, 0.88, 0.09, lod.width_compensation(grass_density)),
        ground_mask_transform: Vec4::new(1.0 / ground.width(), 1.0 / ground.depth(), 0.5, 0.5),
        ground_mask: Some(ground_mask),
        // A uniform material selector lets an entire Far/Vista draw bypass
        // player interaction and camera-facing curved-ribbon reconstruction.
        // `quality.z` independently selects reduced fragment lighting.
        quality: Vec4::new(
            if lod == GrassMeshLod::Near { 0.0 } else { 1.0 },
            mask_mode.shader_flag(),
            if lod == GrassMeshLod::Near { 0.0 } else { 1.0 },
            if lod == GrassMeshLod::Near {
                1.0
            } else {
                REDUCED_GRASS_LIGHTING_SCALE
            },
        ),
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
    material.shading.w = if lod == GrassMeshLod::Near { 1.0 } else { 0.0 };
    material.quality.x = if lod == GrassMeshLod::Near { 0.0 } else { 1.0 };
    material.quality.z = if lod == GrassMeshLod::Near { 0.0 } else { 1.0 };
    material.quality.w = if lod == GrassMeshLod::Near {
        1.0
    } else {
        REDUCED_GRASS_LIGHTING_SCALE
    };
    material.shape = match lod {
        // The close exterior field keeps the full interactive representation.
        // Far/Vista retain this authored footprint while their material's
        // quality selector uses the reduced wind and vertex path.
        GrassMeshLod::Near | GrassMeshLod::NearEdge | GrassMeshLod::Far => {
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

fn grass_patch_topology(ground: &SceneGround, centre: Vec2) -> Option<GrassTopology> {
    let half_extent = GRASS_PATCH_SPACING * 0.58;
    let mut total = 0.0;
    let mut samples = 0;
    for z in [-1.0, -0.5, 0.0, 0.5, 1.0] {
        for x in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            let coverage = ground
                .ground_at(centre + Vec2::new(x, z) * half_extent)
                .filter(|sample| sample.cover == GroundCover::TallGrass)
                .map_or(0.0, |sample| f32::from(sample.cover_density_bps) / 10_000.0);
            total += coverage;
            samples += 1;
        }
    }
    GrassTopology::for_local_coverage(total / samples as f32)
}

fn grass_ground_mask_mode(
    ground: &SceneGround,
    centre: Vec2,
    lod: GrassMeshLod,
) -> GrassGroundMaskMode {
    let half_extent = lod.masked_root_half_extent()
        + GRASS_MASK_FEATHER_METRES
        + ground.grid_scale() * GRASS_MASK_SOURCE_WARP_GUARD_CELLS;
    // A patch near the edge cannot establish the complete authoritative
    // footprint, so it always preserves the texture mask's edge behaviour.
    if centre.x.abs() + half_extent > ground.width() * 0.5
        || centre.y.abs() + half_extent > ground.depth() * 0.5
    {
        return GrassGroundMaskMode::Boundary;
    }
    let touches_non_grass = [
        GroundCover::Bare,
        GroundCover::LeafLitter,
        GroundCover::LooseStone,
        GroundCover::Reeds,
    ]
    .into_iter()
    .any(|cover| ground.cover_intersects_square(centre, half_extent, cover));
    if touches_non_grass {
        GrassGroundMaskMode::Boundary
    } else {
        GrassGroundMaskMode::Interior
    }
}
fn grass_patch_transform(terrain: &SceneTerrain, world_x: f32, world_z: f32) -> Option<Transform> {
    let sample = Vec2::new(world_x, world_z);
    let height = terrain.height_at(sample)?;
    let normal = terrain.normal_at(sample)?;
    if normal.y < 0.72 {
        return None;
    }
    Some(
        Transform::from_xyz(world_x, height, world_z)
            .with_rotation(Quat::from_rotation_arc(Vec3::Y, normal)),
    )
}

fn grass_patch_yaw(hash: u64) -> Quat {
    Quat::from_rotation_y(unit_hash(splitmix64(hash ^ 0x2f76_b694)) * core::f32::consts::TAU)
}

fn grass_patch_placement(
    terrain: &SceneTerrain,
    ground: &SceneGround,
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
    grass_patch_transform(terrain, render_centre.x, render_centre.y)
}

/// Reproduces the legacy jittered-cell eligibility gate for a single placement
/// cell. Used by the instanced renderer to keep tuft placement identical to the
/// legacy patch grid: same jitter, same legacy/render two-centre cover gate,
/// same slope rejection.
#[cfg(all(feature = "instanced-grass", not(target_family = "wasm")))]
pub(super) fn cell_allows_grass(
    terrain: &SceneTerrain,
    ground: &SceneGround,
    cell_hash: u64,
    x: i32,
    z: i32,
    cell_spacing: f32,
) -> bool {
    let jitter_x = unit_hash(splitmix64(cell_hash ^ 0x39bd_7f21)) - 0.5;
    let jitter_z = unit_hash(splitmix64(cell_hash ^ 0xe651_34aa)) - 0.5;
    let eligibility_centre = Vec2::new(
        (x as f32 + jitter_x * 0.24) * cell_spacing,
        (z as f32 + jitter_z * 0.24) * cell_spacing,
    );
    let render_centre = Vec2::new(
        (x as f32 + jitter_x * GRASS_PATCH_JITTER_FRACTION) * cell_spacing,
        (z as f32 + jitter_z * GRASS_PATCH_JITTER_FRACTION) * cell_spacing,
    );
    grass_patch_placement(terrain, ground, eligibility_centre, render_centre).is_some()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::presentation) enum GrassMeshLod {
    Near,
    /// Instanced-only outer ring of the near band. Same tuft placements as
    /// `Near`, but a thinner blade grid at reduced ribbon vertices: past ~9 m
    /// individual blades stop being countable while the near band's area is
    /// dominated by this ring (area grows with radius squared). The legacy
    /// patch renderer never spawns it, so it is dead on the wasm/legacy build.
    #[cfg_attr(
        any(not(feature = "instanced-grass"), target_family = "wasm"),
        allow(
            dead_code,
            reason = "instanced-only near sub-tier; legacy patch renderer never spawns it"
        )
    )]
    NearEdge,
    Far,
    /// Patch-level vista representation. Broad five-vertex tuft ribbons carry
    /// the field here instead of merely thinning the close-range blade mesh.
    Vista,
}

impl GrassMeshLod {
    fn masked_root_half_extent(self) -> f32 {
        let blade_spacing = GRASS_BLADE_SPACING
            * if self == Self::Vista {
                VISTA_GRASS_PATCH_SPACING / GRASS_PATCH_SPACING
            } else {
                1.0
            };
        // Patches receive arbitrary yaw, so the conservative axis-aligned mask
        // footprint is the square mesh's circumradius rather than its unrotated
        // half-width.
        blade_spacing * (GRASS_PATCH_GRID_SIDE - 1) as f32 * 0.5 * core::f32::consts::SQRT_2
    }

    fn row_heights(self) -> &'static [f32] {
        match self {
            // Five paired rows plus a shared tip: eleven vertices preserve the
            // original base, shoulder, tip, and monotonic UV progression while
            // removing two intermediate ribbon segments from ordinary Near
            // grass.
            Self::Near => &[0.0, 0.22, 0.45, 0.68, 0.9],
            // Instanced-only outer near ring. Four paired rows plus a shared
            // tip: nine vertices keep the ribbon bend readable while shedding
            // cost past the countable-blade distance.
            Self::NearEdge => &[0.0, 0.3, 0.58, 0.83],
            // Three paired rows plus a shared tip: seven vertices at distance.
            Self::Far => &[0.0, 0.45, 0.82],
            Self::Vista => &[0.0, 0.62],
        }
    }

    fn blade_grid_indices(self, grass_density: f32) -> impl Iterator<Item = usize> {
        (0..GRASS_PATCH_GRID_SIDE * GRASS_PATCH_GRID_SIDE).filter(move |index| {
            let row = index / GRASS_PATCH_GRID_SIDE;
            let column = index % GRASS_PATCH_GRID_SIDE;
            let selected_for_lod = self.selects_grid_root(row, column);
            selected_for_lod
                && (grass_density >= 1.0
                    || unit_hash(splitmix64(*index as u64 ^ 0x24e8_51c6_9a37_b40d)) < grass_density)
        })
    }

    fn selects_grid_root(self, row: usize, column: usize) -> bool {
        let (stratum_side, salt) = match self {
            // Both near tiers keep every grid root; `NearEdge` is instanced-only
            // and never reaches the legacy stratified selection.
            Self::Near | Self::NearEdge => return true,
            Self::Far => (GRASS_FAR_STRATUM_SIDE, 0x6661_725f_726f_6f74),
            Self::Vista => (GRASS_VISTA_STRATUM_SIDE, 0x7669_7374_726f_6f74),
        };
        let strata_per_side = GRASS_PATCH_GRID_SIDE / stratum_side;
        let stratum_row = row / stratum_side;
        let stratum_column = column / stratum_side;
        let stratum = stratum_row * strata_per_side + stratum_column;
        let hash = splitmix64(stratum as u64 ^ salt);
        let selected_row = if stratum_row == 0 {
            0
        } else if stratum_row + 1 == strata_per_side {
            GRASS_PATCH_GRID_SIDE - 1
        } else {
            stratum_row * stratum_side + (hash as usize % stratum_side)
        };
        let selected_column = if stratum_column == 0 {
            0
        } else if stratum_column + 1 == strata_per_side {
            GRASS_PATCH_GRID_SIDE - 1
        } else {
            stratum_column * stratum_side + (splitmix64(hash) as usize % stratum_side)
        };
        row == selected_row && column == selected_column
    }

    fn blade_count(self, grass_density: f32) -> usize {
        self.blade_grid_indices(grass_density).count()
    }

    pub(in crate::presentation) fn width_compensation(self, grass_density: f32) -> f32 {
        match self {
            Self::Near => return 1.0,
            // Compensates the thinned instanced edge tuft (6x6) against the near
            // tuft (8x8) so projected ground cover holds through the sub-tier
            // crossfade. Instanced-only; the legacy renderer never asks for it.
            Self::NearEdge => return (64.0_f32 / 36.0).sqrt(),
            // Vista represents small tufts, but must not become the broad
            // rectangular cards that were conspicuous in traversal views.
            Self::Vista => return 1.8,
            Self::Far => {}
        }
        // Compensate only for blades discarded by the Far LOD. The square-root
        // response retains clumped negative space while keeping projected
        // cover close to Near through the crossfade.
        let near_count =
            (GRASS_PATCH_GRID_SIDE * GRASS_PATCH_GRID_SIDE) as f32 * grass_density.clamp(0.0, 1.0);
        let lod_count = Self::Far.blade_count(grass_density).max(1) as f32;
        (near_count.max(1.0) / lod_count).sqrt().min(1.65)
    }
}

pub(in crate::presentation) fn grass_lod_visibility(lod: GrassMeshLod) -> VisibilityRange {
    match lod {
        GrassMeshLod::Near => VisibilityRange {
            start_margin: 0.0..0.0,
            end_margin: NEAR_TO_FAR_SWARD_FADE_START_METRES..NEAR_TO_FAR_SWARD_FADE_END_METRES,
            use_aabb: false,
        },
        // Instanced-only sub-tier; the legacy patch renderer never spawns it,
        // so this band only informs the instanced fade-continuity invariant.
        // The near field hands off to the edge ring at 4..6 m and the ring
        // fades out into the far tier across the legacy near band's end.
        GrassMeshLod::NearEdge => VisibilityRange {
            start_margin: 4.0..6.0,
            end_margin: NEAR_TO_FAR_SWARD_FADE_START_METRES..NEAR_TO_FAR_SWARD_FADE_END_METRES,
            use_aabb: false,
        },
        GrassMeshLod::Far => VisibilityRange {
            start_margin: NEAR_TO_FAR_SWARD_FADE_START_METRES..NEAR_TO_FAR_SWARD_FADE_END_METRES,
            end_margin: 36.0..44.0,
            use_aabb: false,
        },
        GrassMeshLod::Vista => VisibilityRange {
            start_margin: 34.0..42.0,
            end_margin: TERMINAL_SWARD_FADE_START_METRES..TERMINAL_SWARD_FADE_END_METRES,
            use_aabb: false,
        },
    }
}

/// Blade grid side of one instanced tuft, per LOD tier.
///
/// Near tufts subdivide the legacy macro patch so the instanced sward
/// reproduces the legacy per-cell shoot density; the near-edge ring deliberately
/// thins that count past the countable-blade distance. Far and vista tufts cover
/// larger footprints with the legacy per-cell shoot totals.
#[cfg(all(feature = "instanced-grass", not(target_family = "wasm")))]
pub(in crate::presentation) fn tuft_blade_side(lod: GrassMeshLod) -> usize {
    match lod {
        GrassMeshLod::Near => 8,
        GrassMeshLod::NearEdge => 6,
        GrassMeshLod::Far => 10,
        GrassMeshLod::Vista => 12,
    }
}

/// Footprint of one instanced tuft in metres, per LOD tier.
#[cfg(all(feature = "instanced-grass", not(target_family = "wasm")))]
pub(in crate::presentation) fn tuft_footprint_metres(lod: GrassMeshLod) -> f32 {
    match lod {
        GrassMeshLod::Near | GrassMeshLod::NearEdge => GRASS_PATCH_SPACING / 12.0,
        GrassMeshLod::Far => GRASS_PATCH_SPACING / 4.0,
        GrassMeshLod::Vista => VISTA_GRASS_PATCH_SPACING / 2.0,
    }
}

/// One instanced grass tuft: a small blade grid sharing the legacy blade,
/// pigment, and inflorescence construction so the instanced and patch renderers
/// stay visually comparable. Placement resolves one species per tuft with the
/// legacy community weights; `seed` decorrelates blade hashes between the shared
/// tuft meshes of different species. The tuft is built from the current
/// (#560) blade/ribbon geometry via `grass_ribbon_patch_mesh`.
#[cfg(all(feature = "instanced-grass", not(target_family = "wasm")))]
pub(in crate::presentation) fn grass_tuft_mesh(
    color: Color,
    lod: GrassMeshLod,
    grass_density: f32,
    species: GrassSpecies,
    seed: u64,
) -> Mesh {
    let grid_side = tuft_blade_side(lod);
    let centre = (grid_side - 1) as f32 * 0.5;
    let blade_spacing = tuft_footprint_metres(lod) / grid_side as f32;
    let blades = (0..grid_side * grid_side)
        .filter(|index| {
            grass_density >= 1.0
                || unit_hash(splitmix64((*index as u64) ^ seed ^ 0x24e8_51c6_9a37_b40d))
                    < grass_density
        })
        .map(|index| {
            let row = index / grid_side;
            let column = index % grid_side;
            let hash = splitmix64(index as u64 ^ seed ^ 0x8d12_6f4a_0bc3_7791);
            let jitter_x = (unit_hash(hash) - 0.5) * blade_spacing * 0.46;
            let jitter_z = (unit_hash(splitmix64(hash)) - 0.5) * blade_spacing * 0.46;
            let clump_vigor = 0.5 + 0.5 * (row as f32 * 0.31 + column as f32 * 0.17 + 0.8).sin();
            let height_scale =
                (0.50 + unit_hash(splitmix64(hash ^ 0x52a9_f131)) * 0.62 + clump_vigor * 0.20)
                    .clamp(0.50, 1.30);
            let width_scale = 0.62 + unit_hash(splitmix64(hash ^ 0x91e2_57a4)) * 0.76;
            GrassBlade {
                offset_x: (column as f32 - centre) * blade_spacing + jitter_x,
                offset_z: (row as f32 - centre) * blade_spacing + jitter_z,
                height_scale,
                width_scale,
                seed: splitmix64(index as u64 ^ seed),
                species,
            }
        })
        .collect::<Vec<_>>();
    grass_ribbon_patch_mesh(0.026, 0.82, color, lod, &blades)
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
    // The shared ribbon stays tall enough to read as meadow grass from the
    // third-person camera. Species width and the softer taper keep individual
    // blades separated without turning the sward into short triangular cards.
    let mut mesh = grass_ribbon_patch_mesh(GRASS_BLADE_WIDTH_METRES, 0.82, color, lod, &blades);
    let rendered_vertices = mesh.count_vertices();
    pad_grass_vertex_allocation(
        &mut mesh,
        legacy_grass_vertex_allocation(lod, grass_density, community).max(rendered_vertices),
    );
    mesh
}

/// Returns the vertex count allocated by the pre-density-reduction mesh for
/// this exact LOD variant. Indices deliberately retain the reduced topology;
/// this count is only a render-buffer allocation compatibility contract.
fn legacy_grass_vertex_allocation(
    lod: GrassMeshLod,
    grass_density: f32,
    community: GrassCommunity,
) -> usize {
    let coordinates: &[usize] = match lod {
        // `NearEdge` is instanced-only and never reaches this legacy allocation
        // contract; it keeps the near tier's full grid for exhaustiveness.
        GrassMeshLod::Near | GrassMeshLod::NearEdge => &[],
        GrassMeshLod::Far => &LEGACY_GRASS_FAR_GRID_COORDINATES,
        GrassMeshLod::Vista => &LEGACY_GRASS_VISTA_GRID_COORDINATES,
    };
    (0..LEGACY_GRASS_PATCH_GRID_SIDE * LEGACY_GRASS_PATCH_GRID_SIDE)
        .filter(|index| {
            let selected_for_lod = coordinates.is_empty()
                || coordinates.contains(&(index / LEGACY_GRASS_PATCH_GRID_SIDE))
                    && coordinates.contains(&(index % LEGACY_GRASS_PATCH_GRID_SIDE));
            selected_for_lod
                && (grass_density >= 1.0
                    || unit_hash(splitmix64(*index as u64 ^ 0x24e8_51c6_9a37_b40d)) < grass_density)
        })
        .map(|index| {
            let mut vertices = lod.row_heights().len() * 2 + 1;
            if lod != GrassMeshLod::Near {
                return vertices;
            }
            let row = index / LEGACY_GRASS_PATCH_GRID_SIDE;
            let column = index % LEGACY_GRASS_PATCH_GRID_SIDE;
            let species_cell = (((column / 8) as u64) << 32) | (row / 8) as u64;
            let species =
                grass_species(community, splitmix64(species_cell ^ 0x7475_6674_5f63_656c));
            let blade_hash = splitmix64(index as u64 ^ 0x6c8e_9cf5_701a_d30b);
            let branch_count = species.inflorescence_branch_count();
            if branch_count > 0 && unit_hash(splitmix64(blade_hash ^ 0x0070_616e_6963_6c65)) < 0.125
            {
                // Two stem quads, then one branch quad and two crossed quads
                // (eight vertices) for every spikelet on every branch.
                vertices += 8 + branch_count * (4 + species.spikelets_per_branch() * 8);
            }
            vertices
        })
        .sum()
}

/// Pads every vertex attribute with copies of an existing in-bounds vertex.
/// No index may reference the padding, so it consumes no vertex-shader work
/// and cannot expand the mesh AABB.
fn pad_grass_vertex_allocation(mesh: &mut Mesh, allocated_vertices: usize) {
    let rendered_vertices = mesh.count_vertices();
    assert!(allocated_vertices >= rendered_vertices);
    if allocated_vertices == rendered_vertices {
        return;
    }

    for attribute in [
        Mesh::ATTRIBUTE_POSITION,
        Mesh::ATTRIBUTE_NORMAL,
        Mesh::ATTRIBUTE_UV_0,
        Mesh::ATTRIBUTE_UV_1,
        Mesh::ATTRIBUTE_COLOR,
    ] {
        let values = mesh
            .attribute_mut(attribute)
            .expect("grass allocation padding requires every vertex attribute");
        match values {
            VertexAttributeValues::Float32x2(values) => {
                let padding = values.first().copied().unwrap_or([0.0; 2]);
                values.resize(allocated_vertices, padding);
            }
            VertexAttributeValues::Float32x3(values) => {
                let padding = values.first().copied().unwrap_or([0.0; 3]);
                values.resize(allocated_vertices, padding);
            }
            VertexAttributeValues::Float32x4(values) => {
                let padding = values.first().copied().unwrap_or([0.0; 4]);
                values.resize(allocated_vertices, padding);
            }
            _ => unreachable!("grass uses only float2, float3, and float4 attributes"),
        }
    }
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
pub(in crate::presentation) enum GrassSpecies {
    FalseOatGrass,
    Cocksfoot,
    RedFescue,
    CommonBent,
    TuftedHairGrass,
    YorkshireFog,
}

pub(in crate::presentation) fn grass_species(community: GrassCommunity, hash: u64) -> GrassSpecies {
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
    /// Enumerates every species so the instanced renderer can key one batch
    /// per species. Instanced-only; dead on the legacy/wasm build.
    #[cfg_attr(
        any(not(feature = "instanced-grass"), target_family = "wasm"),
        allow(dead_code, reason = "instanced grass renderer is native-only")
    )]
    pub(in crate::presentation) const ALL: [Self; 6] = [
        Self::FalseOatGrass,
        Self::Cocksfoot,
        Self::RedFescue,
        Self::CommonBent,
        Self::TuftedHairGrass,
        Self::YorkshireFog,
    ];

    #[cfg_attr(
        any(not(feature = "instanced-grass"), target_family = "wasm"),
        allow(dead_code, reason = "instanced grass renderer is native-only")
    )]
    pub(in crate::presentation) const fn index(self) -> usize {
        self as usize
    }

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
        let lean_direction = Vec3::new(-angle.sin(), 0.0, angle.cos());
        let lean_metres = height
            * height_scale
            * species.height_scale()
            * (0.008 + unit_hash(splitmix64(hash ^ 0x626c_6164_655f_6c65)) * 0.027);
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
            let taper = (1.0 - height_fraction).powf(0.72);
            let side = half_width * taper;
            let centre = root
                + Vec3::Y * height * height_scale * species.height_scale() * height_fraction
                + lean_direction * lean_metres * height_fraction.powf(1.65);
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
        positions.push(
            (root
                + Vec3::Y * height * height_scale * species.height_scale()
                + lean_direction * lean_metres)
                .to_array(),
        );
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

        // Seed heads are sparse Near-only geometry. Close cocksfoot reads as
        // compact offset clusters and oat/bent/hair-grass as open panicles;
        // ordinary ribbons remain smoothly pointed at every LOD. Only about
        // one shoot in eight bears a seed head, keeping the cost bounded and
        // avoiding sub-pixel triangles at distance.
        let branch_count = species.inflorescence_branch_count();
        if lod == GrassMeshLod::Near
            && branch_count > 0
            && unit_hash(splitmix64(hash ^ 0x0070_616e_6963_6c65)) < 0.125
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
    use adventuresim_tactical_core::prelude::GroundSurface;
    use bevy::{
        pbr::Material,
        prelude::{AlphaMode, default},
    };
    use std::collections::BTreeSet;

    fn indexed_vertex_count(mesh: &Mesh) -> usize {
        mesh.indices()
            .and_then(|indices| indices.iter().max())
            .map_or(0, |maximum| maximum as usize + 1)
    }

    fn vertex_bounds(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
        positions.iter().fold(
            ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]),
            |(mut minimum, mut maximum), position| {
                for axis in 0..3 {
                    minimum[axis] = minimum[axis].min(position[axis]);
                    maximum[axis] = maximum[axis].max(position[axis]);
                }
                (minimum, maximum)
            },
        )
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
        assert_eq!(GrassSpecies::RedFescue.inflorescence_branch_count(), 0);
        assert!(
            GrassSpecies::CommonBent.inflorescence_branch_count()
                > GrassSpecies::FalseOatGrass.inflorescence_branch_count()
        );
    }

    #[test]
    fn ordinary_blade_bodies_use_the_exact_four_times_authored_width() {
        assert_eq!(GRASS_BLADE_WIDTH_METRES, 0.019 * 4.0);
        for species in [
            GrassSpecies::FalseOatGrass,
            GrassSpecies::Cocksfoot,
            GrassSpecies::RedFescue,
            GrassSpecies::CommonBent,
            GrassSpecies::TuftedHairGrass,
            GrassSpecies::YorkshireFog,
        ] {
            let mesh = grass_ribbon_patch_mesh(
                GRASS_BLADE_WIDTH_METRES,
                0.82,
                Color::WHITE,
                GrassMeshLod::Far,
                &[GrassBlade {
                    offset_x: 0.0,
                    offset_z: 0.0,
                    height_scale: 1.0,
                    width_scale: 1.0,
                    seed: 0,
                    species,
                }],
            );
            let positions = mesh
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .and_then(VertexAttributeValues::as_float3)
                .expect("ordinary blades should retain authored positions");
            let base_width =
                Vec3::from_array(positions[0]).distance(Vec3::from_array(positions[1]));
            assert!(
                (base_width - GRASS_BLADE_WIDTH_METRES * species.width_scale()).abs()
                    < f32::EPSILON * 8.0,
                "{species:?} must apply its width scale to the exact 76 mm body"
            );
        }
    }

    #[test]
    fn near_seed_heads_are_crossed_clusters_with_rigid_attachment_metadata() {
        let flowering_seed = (0..4_096)
            .find(|seed| {
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
                        seed: *seed,
                        species: GrassSpecies::Cocksfoot,
                    }],
                );
                mesh.count_vertices() > 11
            })
            .expect("the bounded seed search should find a flowering cocksfoot shoot");
        let flowering_mesh = |species| {
            grass_ribbon_patch_mesh(
                0.026,
                0.82,
                Color::WHITE,
                GrassMeshLod::Near,
                &[GrassBlade {
                    offset_x: 0.0,
                    offset_z: 0.0,
                    height_scale: 1.0,
                    width_scale: 1.0,
                    seed: flowering_seed,
                    species,
                }],
            )
        };
        let mesh = flowering_mesh(GrassSpecies::Cocksfoot);

        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .expect("seed heads should retain authored positions");
        let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0)
        else {
            panic!("seed heads should carry attachment metadata");
        };
        let seed_head_positions = &positions[11..];
        let seed_head_uvs = &uvs[11..];

        assert!(seed_head_positions.len() >= 92);
        assert!(seed_head_uvs.iter().all(|uv| uv[0] > 0.0 && uv[1] < 0.0));
        assert!(
            seed_head_positions
                .iter()
                .any(|position| position[0].abs().max(position[2].abs()) > 0.03),
            "the nearest seed head must contain authored lateral branches"
        );
        assert_eq!(seed_head_positions.len() % 4, 0);

        let oat = flowering_mesh(GrassSpecies::FalseOatGrass);
        let oat_positions = oat
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .expect("open oat-grass panicles should retain authored positions");
        let lateral_extent = |positions: &[[f32; 3]]| {
            positions[11..]
                .iter()
                .map(|position| Vec2::new(position[0], position[2]).length())
                .fold(0.0_f32, f32::max)
        };
        assert!(
            lateral_extent(positions) < lateral_extent(oat_positions),
            "cocksfoot seed heads must remain more compact than open panicles"
        );
    }

    #[test]
    fn ordinary_blade_ribbons_reconstruct_to_pointed_nondegenerate_tips() {
        let species = [
            GrassSpecies::FalseOatGrass,
            GrassSpecies::Cocksfoot,
            GrassSpecies::RedFescue,
            GrassSpecies::CommonBent,
            GrassSpecies::TuftedHairGrass,
            GrassSpecies::YorkshireFog,
        ];

        for lod in [GrassMeshLod::Near, GrassMeshLod::Far, GrassMeshLod::Vista] {
            for species in species {
                let mesh = grass_ribbon_patch_mesh(
                    0.026,
                    0.82,
                    Color::WHITE,
                    lod,
                    &[GrassBlade {
                        offset_x: 0.0,
                        offset_z: 0.0,
                        height_scale: 1.0,
                        width_scale: 1.0,
                        seed: 0,
                        species,
                    }],
                );
                let positions = mesh
                    .attribute(Mesh::ATTRIBUTE_POSITION)
                    .and_then(VertexAttributeValues::as_float3)
                    .expect("ordinary blades should retain authored positions");
                let normals = mesh
                    .attribute(Mesh::ATTRIBUTE_NORMAL)
                    .and_then(VertexAttributeValues::as_float3)
                    .expect("ordinary blades should retain authored normals");
                let Some(VertexAttributeValues::Float32x2(uvs)) =
                    mesh.attribute(Mesh::ATTRIBUTE_UV_0)
                else {
                    panic!("ordinary blades should retain side and height metadata");
                };
                let paired_rows = lod.row_heights().len();
                let widths = positions[..paired_rows * 2]
                    .chunks_exact(2)
                    .map(|row| Vec3::from_array(row[0]).distance(Vec3::from_array(row[1])))
                    .collect::<Vec<_>>();
                assert!(
                    widths.windows(2).all(|pair| pair[1] < pair[0]),
                    "{species:?} {lod:?} ordinary blade must narrow at every row"
                );
                let tip_index = paired_rows * 2;
                let shoulder = tip_index - 2;
                assert!(
                    uvs[tip_index] == [0.5, 1.0],
                    "the shared tip must carry the centre-vertex UV contract"
                );
                let indices = mesh.indices().unwrap().iter().collect::<Vec<_>>();
                let ordinary_index_count = ((paired_rows - 1) * 2 + 1) * 3;
                assert_eq!(
                    &indices[ordinary_index_count - 3..ordinary_index_count],
                    &[shoulder as usize, shoulder + 1, tip_index],
                    "the final primitive must be one shoulder-to-tip triangle"
                );

                let normal = Vec3::from_array(normals[0]);
                let local_side = Vec2::new(-normal.z, normal.x).normalize();
                let reconstruct = |index: usize| {
                    let authored = Vec3::from_array(positions[index]);
                    let authored_half_width =
                        Vec2::new(authored.x, authored.z).dot(local_side).abs();
                    let half_width = if (uvs[index][0] - 0.5).abs() < 0.001 {
                        0.0
                    } else {
                        authored_half_width
                    };
                    let signed_side = if uvs[index][0] >= 0.5 { 1.0 } else { -1.0 };
                    Vec3::new(
                        local_side.x * half_width * signed_side,
                        authored.y,
                        local_side.y * half_width * signed_side,
                    )
                };
                let left = reconstruct(shoulder);
                let right = reconstruct(shoulder + 1);
                let tip = reconstruct(tip_index);
                assert!(
                    Vec2::new(positions[tip_index][0], positions[tip_index][2]).length() > 0.0,
                    "the fixture must exercise an authored centreline lean"
                );
                assert_eq!(Vec2::new(tip.x, tip.z), Vec2::ZERO);
                let terminal_area = (right - left).cross(tip - left).length() * 0.5;
                assert!(
                    terminal_area > 0.00001,
                    "{species:?} {lod:?} reconstructed terminal triangle must remain visible"
                );
            }
        }
    }

    #[test]
    fn grass_ribbon_lods_have_exact_per_blade_geometry_and_valid_meshes() {
        let expected = [
            (GrassMeshLod::Near, 11, 27),
            (GrassMeshLod::Far, 7, 15),
            (GrassMeshLod::Vista, 5, 9),
        ];

        for (lod, vertices_per_blade, indices_per_blade) in expected {
            let mesh = grass_ribbon_patch_mesh(
                0.026,
                0.82,
                Color::WHITE,
                lod,
                &[GrassBlade {
                    offset_x: 0.0,
                    offset_z: 0.0,
                    height_scale: 1.0,
                    width_scale: 1.0,
                    seed: 0,
                    species: GrassSpecies::RedFescue,
                }],
            );
            assert_eq!(mesh.count_vertices(), vertices_per_blade);
            let indices = mesh.indices().unwrap().iter().collect::<Vec<_>>();
            assert_eq!(indices.len(), indices_per_blade);

            for attribute in [
                Mesh::ATTRIBUTE_POSITION,
                Mesh::ATTRIBUTE_NORMAL,
                Mesh::ATTRIBUTE_UV_0,
                Mesh::ATTRIBUTE_UV_1,
                Mesh::ATTRIBUTE_COLOR,
            ] {
                assert_eq!(mesh.attribute(attribute).unwrap().len(), vertices_per_blade);
            }
            assert!(indices.iter().all(|index| *index < vertices_per_blade));
            assert_eq!(indices.len() % 3, 0);
            let expected_indices = match lod {
                GrassMeshLod::Near => vec![
                    0, 1, 3, 0, 3, 2, 2, 3, 5, 2, 5, 4, 4, 5, 7, 4, 7, 6, 6, 7, 9, 6, 9, 8, 8, 9,
                    10,
                ],
                GrassMeshLod::Far => vec![0, 1, 3, 0, 3, 2, 2, 3, 5, 2, 5, 4, 4, 5, 6],
                GrassMeshLod::Vista => vec![0, 1, 3, 0, 3, 2, 2, 3, 4],
                // Instanced-only sub-tier, excluded from this legacy geometry
                // table; the loop above never yields it.
                GrassMeshLod::NearEdge => {
                    unreachable!("near-edge tier is instanced-only")
                }
            };
            assert_eq!(indices, expected_indices);
        }
    }

    #[test]
    fn grass_allocation_padding_is_unindexed_and_preserves_bounds() {
        let mesh = grass_patch_mesh(
            Color::WHITE,
            GrassMeshLod::Far,
            1.0,
            GrassCommunity::MesicMeadow,
        );
        let rendered_vertices = indexed_vertex_count(&mesh);
        let allocated_vertices = mesh.count_vertices();
        assert_eq!(rendered_vertices, 64 * 7);
        assert_eq!(allocated_vertices, 256 * 7);
        assert_eq!(
            allocated_vertices,
            legacy_grass_vertex_allocation(GrassMeshLod::Far, 1.0, GrassCommunity::MesicMeadow)
        );
        assert!(
            mesh.indices()
                .unwrap()
                .iter()
                .all(|index| index < rendered_vertices)
        );

        for attribute in [
            Mesh::ATTRIBUTE_POSITION,
            Mesh::ATTRIBUTE_NORMAL,
            Mesh::ATTRIBUTE_UV_0,
            Mesh::ATTRIBUTE_UV_1,
            Mesh::ATTRIBUTE_COLOR,
        ] {
            assert_eq!(mesh.attribute(attribute).unwrap().len(), allocated_vertices);
        }
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .unwrap();
        assert_eq!(
            vertex_bounds(&positions[..rendered_vertices]),
            vertex_bounds(positions),
            "unreferenced allocation padding must not distort the mesh AABB"
        );

        let sparse_snow = grass_patch_mesh(
            Color::WHITE,
            GrassMeshLod::Near,
            0.12,
            GrassCommunity::MesicMeadow,
        );
        assert!(sparse_snow.count_vertices() >= indexed_vertex_count(&sparse_snow));
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
        assert_eq!(GrassMeshLod::Near.blade_count(1.0), 1_024);
        assert_eq!(GrassMeshLod::Far.blade_count(1.0), 64);
        assert_eq!(GrassMeshLod::Vista.blade_count(1.0), 16);
        assert_eq!(FAR_LOD_GAP_FILL_FRACTION, 0.75);
        assert_eq!(near_positions.len(), 75_308);
        let near_blade_positions = &near_positions[..1_024 * 11];
        assert_eq!(far_positions.len(), 256 * 7);
        assert_eq!(vista.count_vertices(), 64 * 5);
        assert_eq!(indexed_vertex_count(&near), 18_944);
        assert_eq!(indexed_vertex_count(&far), 64 * 7);
        assert_eq!(indexed_vertex_count(&vista), 16 * 5);
        assert!(16.0 / VISTA_GRASS_PATCH_SPACING.powi(2) >= 0.35);
        assert!(indexed_vertex_count(&near) > indexed_vertex_count(&far));
        assert!(indexed_vertex_count(&far) > indexed_vertex_count(&vista));
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
        let near_blade_roots = &near_roots[..1_024 * 11];
        assert_eq!(far_roots.len(), far_positions.len());
        let rendered_far_roots = &far_roots[..indexed_vertex_count(&far)];
        assert!(
            rendered_far_roots
                .iter()
                .all(|root| near_blade_roots.contains(root))
        );
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
        let rendered_far_colors = &far_colors[..indexed_vertex_count(&far)];
        for (far_root, far_color) in rendered_far_roots
            .chunks_exact(7)
            .zip(rendered_far_colors.chunks_exact(7))
        {
            let matching_near_blade = near_blade_roots
                .chunks_exact(11)
                .position(|near_root| near_root[0] == far_root[0])
                .expect("every far blade must retain its exact near-LOD root");
            assert_eq!(
                colors[matching_near_blade * 11][3],
                far_color[0][3],
                "near and far LODs must apply the same ground-mask threshold"
            );
            assert_eq!(
                colors[matching_near_blade * 11],
                far_color[0],
                "near and far LOD roots must retain the same base pigment and age"
            );
            assert_eq!(
                colors[matching_near_blade * 11 + 10],
                far_color[6],
                "near and far LOD tips must retain the same senescent pigment"
            );
        }

        let blade_heights = near_blade_positions
            .chunks_exact(11)
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
            minimum_height > 0.25,
            "even the shortest species must remain legible as grass"
        );
        assert!(
            minimum_height < 0.60,
            "short blades should still break the curtain silhouette"
        );
        assert!(
            maximum_height > 1.20,
            "mature blades should remain visibly taller"
        );
        assert!(maximum_height - minimum_height > 0.45);

        for (blade, roots) in near_blade_positions
            .chunks_exact(11)
            .zip(near_blade_roots.chunks_exact(11))
        {
            let tip = Vec3::from_array(blade[10]);
            let root = Vec3::new(roots[0][0], 0.0, roots[0][1]);
            let displacement = tip - root;
            let horizontal_displacement = Vec2::new(displacement.x, displacement.z).length();
            assert!(
                horizontal_displacement / tip.y <= 0.0351,
                "authored blade silhouettes must remain predominantly upright"
            );
        }

        let blade_widths = near_blade_positions
            .chunks_exact(11)
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

        let repeated_far = grass_patch_mesh(
            Color::WHITE,
            GrassMeshLod::Far,
            1.0,
            GrassCommunity::MesicMeadow,
        );
        let repeated_vista = grass_patch_mesh(
            Color::WHITE,
            GrassMeshLod::Vista,
            1.0,
            GrassCommunity::MesicMeadow,
        );
        assert_eq!(
            far.attribute(Mesh::ATTRIBUTE_POSITION),
            repeated_far.attribute(Mesh::ATTRIBUTE_POSITION)
        );
        assert_eq!(
            vista.attribute(Mesh::ATTRIBUTE_POSITION),
            repeated_vista.attribute(Mesh::ATTRIBUTE_POSITION)
        );

        let Some(VertexAttributeValues::Float32x2(vista_roots)) =
            vista.attribute(Mesh::ATTRIBUTE_UV_1)
        else {
            panic!("vista grass mesh must carry stable blade roots");
        };
        assert_eq!(vista_roots.len(), vista.count_vertices());
        assert!(vista_roots.iter().any(|root| root[0] < -1.7));
        assert!(vista_roots.iter().any(|root| root[0] > 1.7));
    }

    #[test]
    fn reduced_lods_use_stratified_irregular_roots() {
        for (lod, stratum_side, cartesian_row_count) in [
            (GrassMeshLod::Far, GRASS_FAR_STRATUM_SIDE, 8),
            (GrassMeshLod::Vista, GRASS_VISTA_STRATUM_SIDE, 4),
        ] {
            let indices = lod.blade_grid_indices(1.0).collect::<Vec<_>>();
            let rows = indices
                .iter()
                .map(|index| index / GRASS_PATCH_GRID_SIDE)
                .collect::<BTreeSet<_>>();
            let columns = indices
                .iter()
                .map(|index| index % GRASS_PATCH_GRID_SIDE)
                .collect::<BTreeSet<_>>();
            assert!(rows.len() > cartesian_row_count);
            assert!(columns.len() > cartesian_row_count);

            let strata_per_side = GRASS_PATCH_GRID_SIDE / stratum_side;
            for stratum_row in 0..strata_per_side {
                for stratum_column in 0..strata_per_side {
                    let retained = indices
                        .iter()
                        .filter(|index| {
                            let row = *index / GRASS_PATCH_GRID_SIDE;
                            let column = *index % GRASS_PATCH_GRID_SIDE;
                            row / stratum_side == stratum_row
                                && column / stratum_side == stratum_column
                        })
                        .count();
                    assert_eq!(retained, 1);
                }
            }
        }
    }

    #[test]
    fn patch_yaw_is_deterministic_and_breaks_lattice_alignment() {
        let first = grass_patch_yaw(1);
        assert_eq!(first, grass_patch_yaw(1));
        let distinct_rotations = (1..=16)
            .map(grass_patch_yaw)
            .map(|rotation| {
                let [x, y, z, w] = rotation.to_array();
                [x.to_bits(), y.to_bits(), z.to_bits(), w.to_bits()]
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(distinct_rotations.len(), 16);
    }

    #[test]
    fn near_blades_stay_natural_and_reduced_lod_widths_remain_bounded() {
        for density in [0.25, 0.5, 0.75, 1.0] {
            let near_width = GrassMeshLod::Near.width_compensation(density);
            let far_width = GrassMeshLod::Far.width_compensation(density);
            assert_eq!(near_width, 1.0);
            assert!((1.0..=1.65).contains(&far_width));
        }
        assert_eq!(GrassMeshLod::Vista.width_compensation(1.0), 1.8);

        let ground = SceneGround::from_samples(2, 2, 1.0, vec![GroundSurface::default(); 4])
            .expect("a flat ground mask is valid");
        for density in [0.25, 0.5, 0.75, 1.0] {
            let near = grass_material(
                0.3,
                GrassMeshLod::Near,
                density,
                0.2,
                Handle::default(),
                &ground,
                GrassGroundMaskMode::Interior,
            );
            let far = grass_material(
                0.3,
                GrassMeshLod::Far,
                density,
                0.2,
                Handle::default(),
                &ground,
                GrassGroundMaskMode::Interior,
            );
            assert_eq!(near.shape.w, 1.0);
            assert!((1.0..=1.65).contains(&far.shape.w));
        }
    }

    #[test]
    fn local_coverage_selects_monotonic_deterministic_topologies_and_skips_zero() {
        assert_eq!(GrassTopology::for_local_coverage(0.0), None);
        assert_eq!(
            GrassTopology::for_local_coverage(0.01),
            Some(GrassTopology::Quarter)
        );
        assert_eq!(
            GrassTopology::for_local_coverage(0.25),
            Some(GrassTopology::Quarter)
        );
        assert_eq!(
            GrassTopology::for_local_coverage(0.26),
            Some(GrassTopology::Half)
        );
        assert_eq!(
            GrassTopology::for_local_coverage(0.51),
            Some(GrassTopology::ThreeQuarters)
        );
        assert_eq!(
            GrassTopology::for_local_coverage(0.76),
            Some(GrassTopology::Full)
        );

        let zero = SceneGround::from_samples(41, 41, 0.1, vec![GroundSurface::default(); 41 * 41])
            .unwrap();
        assert_eq!(grass_patch_topology(&zero, Vec2::ZERO), None);

        let topology_for_density = |density| {
            let sample = GroundSurface {
                cover: GroundCover::TallGrass,
                cover_density_bps: density,
                ..default()
            };
            let ground = SceneGround::from_samples(41, 41, 0.1, vec![sample; 41 * 41]).unwrap();
            grass_patch_topology(&ground, Vec2::ZERO)
        };
        assert_eq!(topology_for_density(2_500), Some(GrassTopology::Quarter));
        assert_eq!(topology_for_density(5_000), Some(GrassTopology::Half));
        assert_eq!(
            topology_for_density(7_500),
            Some(GrassTopology::ThreeQuarters)
        );
        assert_eq!(topology_for_density(10_000), Some(GrassTopology::Full));

        let variants = GrassTopology::ALL.map(|topology| {
            grass_patch_mesh(
                Color::WHITE,
                GrassMeshLod::Far,
                topology.density(),
                GrassCommunity::MesicMeadow,
            )
        });
        let repeated = grass_patch_mesh(
            Color::WHITE,
            GrassMeshLod::Far,
            GrassTopology::Half.density(),
            GrassCommunity::MesicMeadow,
        );
        assert_eq!(
            variants[1].attribute(Mesh::ATTRIBUTE_POSITION),
            repeated.attribute(Mesh::ATTRIBUTE_POSITION)
        );
        for pair in variants.windows(2) {
            assert!(pair[0].count_vertices() <= pair[1].count_vertices());
        }
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
        let transform = grass_patch_transform(&terrain, 0.0, 0.0).unwrap();
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
        assert!(grass_patch_placement(&terrain, &ground, legacy, rendered).is_some());
    }

    #[test]
    fn invalid_render_anchor_is_skipped_without_legacy_fallback() {
        let terrain = SceneTerrain::from_heightmap(2, 2, 1.0, vec![0.0; 4]).unwrap();
        let ground =
            SceneGround::from_samples(81, 81, 0.1, vec![GroundSurface::default(); 81 * 81])
                .unwrap();
        assert!(grass_patch_transform(&terrain, 0.0, 0.0).is_some());
        assert!(
            grass_patch_placement(&terrain, &ground, Vec2::ZERO, Vec2::new(2.0, 0.0)).is_none()
        );
    }

    #[test]
    fn representative_slope_keeps_adjacent_boundary_rows_overlapping() {
        let heights = (0..3)
            .flat_map(|_| (0..9).map(|x| x as f32 * 0.25))
            .collect::<Vec<_>>();
        let terrain = SceneTerrain::from_heightmap(9, 3, 1.0, heights).unwrap();
        let left = grass_patch_transform(&terrain, -1.6, 0.0).unwrap();
        let right = grass_patch_transform(&terrain, 1.6, 0.0).unwrap();
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

        assert_eq!(near.start_margin, 0.0..0.0);
        assert_eq!(
            near.end_margin,
            NEAR_TO_FAR_SWARD_FADE_START_METRES..NEAR_TO_FAR_SWARD_FADE_END_METRES
        );
        assert_eq!(
            far.start_margin,
            NEAR_TO_FAR_SWARD_FADE_START_METRES..NEAR_TO_FAR_SWARD_FADE_END_METRES
        );
        assert_eq!(far.end_margin, 36.0..44.0);
        assert_eq!(vista.start_margin, 34.0..42.0);
        assert_eq!(vista.end_margin, 42.0..50.0);
        assert_eq!(
            vista.end_margin,
            TERMINAL_SWARD_FADE_START_METRES..TERMINAL_SWARD_FADE_END_METRES
        );

        // The next LOD begins fading before the previous one has finished,
        // so the range never exposes an uncovered distance interval.
        assert_eq!(near.end_margin, far.start_margin);
        assert!(far.end_margin.start >= vista.start_margin.start);
        assert!(far.end_margin.end >= vista.start_margin.end);
        // The terrain sward begins while the final physical tier remains
        // visible, and reaches full coverage exactly as that tier ends.
        assert_eq!(vista.end_margin.start, TERMINAL_SWARD_FADE_START_METRES);
        assert_eq!(vista.end_margin.end, TERMINAL_SWARD_FADE_END_METRES);
        assert!(!near.is_abrupt());
        assert!(!far.is_abrupt());
        assert!(!vista.is_abrupt());
    }

    #[test]
    fn grass_composition_guards_boundary_mask_fetches_and_preserves_topology() {
        let shader = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/shaders/tactical_foliage.wgsl"
        ));
        // The full Near and cheap Far/Vista paths each retain one boundary
        // fetch. Both are nested under the uniform interior selector, so an
        // interior material cannot sample or collapse a grass blade.
        assert_eq!(shader.matches("textureSampleLevel(").count(), 2);
        assert_eq!(shader.matches("if foliage.quality.y < 0.5 {").count(), 2);
        assert!(
            shader
                .contains("if foliage.quality.y < 0.5 {\n            let mask_uv = root_world.xz")
        );
        assert!(shader.contains("let effective_coverage = ground_coverage * clump_coverage"));
        assert!(shader.contains("let edge_growth = mix(0.26, 1.0"));
        assert!(!shader.contains("let tip_age"));
        assert!(shader.contains("* mix(1.0, 0.94, mature_age)"));
        assert!(shader.contains("lean_amount + 0.004 * mature_age"));
        assert!(shader.contains("let is_inflorescence = vertex.uv.y < 0.0"));
        assert!(shader.contains("let bent_offset = rotate_between"));
        assert!(shader.contains("let authored_half_width = abs(dot("));
        assert!(shader.contains("let is_centre_vertex = abs(vertex.uv.x - 0.5) < 0.001"));
        assert!(shader.contains("0.0,\n                is_centre_vertex,"));
        assert!(!shader.contains("let half_width = length(position.xz - root_local.xz)"));
        assert!(shader.contains("abs(f32(in.visibility_range_dither)) / 16.0"));
        assert!(!shader.contains("visibility_range_dither(in.position"));
        assert!(shader.contains("vec4<f32>(root_world, 1.0)"));
        assert!(shader.contains("0.60,"));
        assert!(shader.contains("foliage.shape.x > 0.5"));

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
        assert_eq!(near.count_vertices(), 75_308);
        assert_eq!(far.count_vertices(), 256 * 7);
        assert_eq!(indexed_vertex_count(&near), 18_944);
        assert_eq!(indexed_vertex_count(&far), 64 * 7);
    }

    #[test]
    fn mask_mode_selects_uniform_material_flags_for_every_grass_lod() {
        let ground = SceneGround::from_samples(2, 2, 1.0, vec![GroundSurface::default(); 4])
            .expect("a flat ground mask is valid");
        assert_eq!(
            GrassMaterialPath::for_lod(GrassMeshLod::Near),
            GrassMaterialPath::FullInteractive
        );
        assert_eq!(
            GrassMaterialPath::for_lod(GrassMeshLod::Far),
            GrassMaterialPath::CheapLod
        );
        assert_eq!(
            GrassMaterialPath::for_lod(GrassMeshLod::Vista),
            GrassMaterialPath::CheapLod
        );
        for lod in [GrassMeshLod::Near, GrassMeshLod::Far, GrassMeshLod::Vista] {
            let boundary = grass_material(
                0.3,
                lod,
                1.0,
                0.2,
                Handle::default(),
                &ground,
                GrassGroundMaskMode::Boundary,
            );
            let interior = grass_material(
                0.3,
                lod,
                1.0,
                0.2,
                Handle::default(),
                &ground,
                GrassGroundMaskMode::Interior,
            );
            assert_eq!(boundary.quality.y, 0.0);
            assert_eq!(interior.quality.y, 1.0);
            assert_eq!(
                boundary.quality.x,
                if lod == GrassMeshLod::Near { 0.0 } else { 1.0 }
            );
            assert_eq!(interior.quality.x, boundary.quality.x);
            assert_eq!(boundary.quality.z, boundary.quality.x);
            assert_eq!(interior.quality.z, boundary.quality.z);
            assert_eq!(
                boundary.quality.w,
                if lod == GrassMeshLod::Near {
                    1.0
                } else {
                    REDUCED_GRASS_LIGHTING_SCALE
                }
            );
            assert_eq!(interior.quality.w, boundary.quality.w);
            assert_eq!(boundary.alpha_mode(), AlphaMode::AlphaToCoverage);
            assert_eq!(interior.alpha_mode(), AlphaMode::AlphaToCoverage);
        }
        let exterior_vista =
            vista_grass_material(0.3, 0.2, Handle::default(), Vec4::ONE, GrassMeshLod::Vista);
        assert_eq!(exterior_vista.quality.y, 0.0);
    }

    #[test]
    fn cheap_grass_branch_pins_reduced_vertex_and_fragment_work() {
        let shader = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/shaders/tactical_foliage.wgsl"
        ));
        assert!(
            !shader.contains("world_position.xz +="),
            "WGSL vector swizzles are not valid compound-assignment targets"
        );
        let cheap_vertex_start = shader
            .find("if foliage.quality.x > 0.5 {")
            .expect("cheap vertex selector");
        let expensive_vertex_start = shader.find("let spatial_noise").expect("full vertex path");
        let cheap_vertex = &shader[cheap_vertex_start..expensive_vertex_start];
        assert_eq!(cheap_vertex.matches("textureSampleLevel(").count(), 1);
        assert!(cheap_vertex.contains("if foliage.quality.y < 0.5 {"));
        assert_eq!(cheap_vertex.matches("sin(").count(), 1);
        assert!(!cheap_vertex.contains("interaction_offset"));
        assert!(!cheap_vertex.contains("camera_side"));
        assert!(!cheap_vertex.contains("rotate_between"));

        let fragment = shader
            .split("fn fragment")
            .nth(1)
            .expect("foliage fragment");
        let cheap_fragment_start = fragment
            .find("if foliage.quality.z > 0.5 {")
            .expect("cheap fragment selector");
        let full_fragment_start = fragment
            .rfind("let height_fraction")
            .expect("full fragment path");
        let cheap_fragment = &fragment[cheap_fragment_start..full_fragment_start];
        assert!(cheap_fragment.contains("main_pass_post_lighting_processing"));
        assert!(!cheap_fragment.contains("apply_pbr_lighting"));
        assert!(!cheap_fragment.contains("diffuse_transmission"));
        assert!(fragment.contains("foliage.shape.x > 0.5"));
    }

    #[test]
    fn conservative_ground_mask_classification_only_marks_clear_tall_grass_as_interior() {
        const WIDTH: usize = 201;
        let mut samples = vec![GroundSurface::default(); WIDTH * WIDTH];
        // The central leaf-litter island is inside every LOD's conservative
        // root, feather, and source-warp footprint. Two otherwise identical
        // patches are far enough away to be genuine interior draws.
        samples[100 * WIDTH + 108].cover = GroundCover::LeafLitter;
        let ground = SceneGround::from_samples(WIDTH, WIDTH, 1.0, samples).unwrap();
        let centres = [Vec2::new(-24.0, 0.0), Vec2::ZERO, Vec2::new(24.0, 0.0)];
        for lod in [GrassMeshLod::Near, GrassMeshLod::Far, GrassMeshLod::Vista] {
            let modes = centres.map(|centre| grass_ground_mask_mode(&ground, centre, lod));
            let interiors = modes
                .iter()
                .filter(|&&mode| mode == GrassGroundMaskMode::Interior)
                .count();
            let boundaries = modes
                .iter()
                .filter(|&&mode| mode == GrassGroundMaskMode::Boundary)
                .count();
            assert_eq!((interiors, boundaries), (2, 1), "{lod:?}");
            assert_eq!(modes[1], GrassGroundMaskMode::Boundary, "{lod:?}");
        }
        assert_eq!(
            grass_ground_mask_mode(&ground, Vec2::new(96.0, 0.0), GrassMeshLod::Near),
            GrassGroundMaskMode::Boundary,
            "scene edges retain the texture path"
        );
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
            Vec4::new(1.0, 0.88, 0.09, 1.65)
        );
        assert_eq!(GrassMeshLod::Vista.width_compensation(1.0), 1.8);
    }
}
