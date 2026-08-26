//! GPU-instanced playable grass on `bevy_eidolon`.
//!
//! One instance is a multi-blade tuft whose mesh reuses the legacy blade,
//! species, and pigment construction (`grass_tuft_mesh`). Placement keeps the
//! legacy cell eligibility semantics - the jittered 3.2 m cell grid, the
//! legacy/render two-centre cover gate, and the slope rejection - then
//! subdivides each eligible cell into tufts. Ground-cover coverage is sampled
//! on the CPU at placement time and packed into each instance's seed byte, so
//! the shader needs no mask texture.
//!
//! Native-only for now: the browser build keeps the legacy patch renderer
//! until bevy_eidolon's `multi_draw_indexed_indirect` draw path gains a
//! baseline-WebGPU fallback.

use std::sync::Arc;

use adventuresim_tactical_core::prelude::{SceneEnvironment, SceneGround, SceneId, SceneTerrain};
use bevy::{
    camera::{primitives::Aabb, visibility::NoFrustumCulling},
    color::{Color, LinearRgba},
    prelude::*,
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};
use bevy_eidolon::{prelude::*, prepass::CullComputeCamera};

use crate::presentation::{bps, grass_cover_mask_pixels, splitmix64, stable_text_seed, unit_hash};

use super::{
    GrassInteractor, GroundScatterLayer,
    grass::{
        GRASS_PATCH_SPACING, GrassCommunityProfile, GrassMeshLod, GrassSpecies,
        VISTA_GRASS_PATCH_SPACING, cell_allows_grass, grass_community_at, grass_species,
        grass_tuft_mesh, tuft_footprint_metres,
    },
    grass_pigment, grass_scatter_density,
};

const GRASS_INSTANCED_SHADER: &str = "shaders/tactical_grass_instanced.wgsl";

/// Interaction envelope radius in metres, matching the legacy material.
const INTERACTION_RADIUS: f32 = 1.35;

pub(crate) struct InstancedGrassPlugin;

impl Plugin for InstancedGrassPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            InstancedMaterialCorePlugin,
            GpuComputeCullCorePlugin,
            InstancedMaterialPlugin::<TacticalGrassInstancedMaterial>::default(),
            GpuCullComputePlugin::<TacticalGrassInstancedMaterial>::default(),
            InstancedMaterialPlugin::<super::TacticalShrubBarkInstancedMaterial>::default(),
            GpuCullComputePlugin::<super::TacticalShrubBarkInstancedMaterial>::default(),
            InstancedMaterialPlugin::<super::TacticalShrubLeafInstancedMaterial>::default(),
            GpuCullComputePlugin::<super::TacticalShrubLeafInstancedMaterial>::default(),
        ))
        .init_resource::<InstancedGrassInteractionState>()
        .add_systems(
            Update,
            (
                enable_camera_cull_compute,
                present_instanced_grass,
                update_instanced_grass_interaction,
            ),
        );
    }
}

/// Marks scenes whose instanced sward has been generated.
#[derive(Component)]
struct InstancedGrassPresented;

/// Instanced counterpart of the grass portion of `present_ground_scatter`.
/// Runs independently so the legacy scatter path keeps its signature; the
/// legacy grass spawn itself is compiled out while this module is active.
fn present_instanced_grass(
    scenes: Query<
        (
            Entity,
            &SceneId,
            &SceneTerrain,
            &SceneGround,
            &SceneEnvironment,
        ),
        Without<InstancedGrassPresented>,
    >,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<TacticalGrassInstancedMaterial>>,
    settings: Res<crate::presentation::TacticalGraphicsSettings>,
) {
    for (entity, _scene_id, terrain, ground, environment) in &scenes {
        let started = web_time::Instant::now();
        let (grass_color, grass_dryness) = grass_pigment(environment);
        let grass_density = grass_scatter_density(
            bps(environment.canopy_bps),
            bps(environment.water_bps),
            bps(environment.cultivation_bps),
            bps(environment.weather.snow_cover_bps),
        ) * settings.grass_density_scale.clamp(0.0, 1.0);
        let wind_scale = 0.16 + bps(environment.weather.wind_speed_bps) * 0.36;
        spawn(
            &mut commands,
            &mut meshes,
            &mut materials,
            terrain,
            ground,
            environment,
            stable_text_seed(&environment.scene_digest) ^ 0x6772_6173_735f_6c6f,
            GrassCommunityProfile::from_environment(environment),
            grass_color,
            grass_density,
            grass_dryness,
            wind_scale,
            settings.grass_range_scale,
        );
        tracing::info!(
            elapsed_ms = started.elapsed().as_millis(),
            "Generated instanced tactical grass"
        );
        commands.entity(entity).insert(InstancedGrassPresented);
    }
}

/// Every gameplay/viewer camera drives eidolon's per-instance compute cull.
fn enable_camera_cull_compute(
    mut commands: Commands,
    cameras: Query<
        Entity,
        (
            With<Camera3d>,
            Without<CullComputeCamera>,
            // The grass never renders on the offscreen cloud layer, and the
            // compute cull must follow the gameplay camera.
            Without<crate::presentation::TacticalCloudOffscreenCamera>,
        ),
    >,
) {
    for camera in &cameras {
        commands.entity(camera).insert(CullComputeCamera);
    }
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[uniform(0, TacticalGrassInstancedUniform)]
pub(in crate::presentation) struct TacticalGrassInstancedMaterial {
    /// Wind direction xy, strength, time scale.
    pub wind: Vec4,
    /// Interactor position (xyz) and interaction radius (w; 0 disables).
    pub interaction: Vec4,
    /// Interactor smoothed velocity (xyz) and push strength.
    pub interaction_motion: Vec4,
    /// Root occlusion, dryness lane, authored lean, width compensation.
    pub params: Vec4,
    /// y scales the flat ambient term to approximate the skipped image-based
    /// lighting. Every tier now shades fast (ambient + wrapped Lambert + one
    /// clamped shadow fetch), so x/z/w are reserved.
    pub shading: Vec4,
}

#[derive(Clone, Default, ShaderType)]
pub(in crate::presentation) struct TacticalGrassInstancedUniform {
    wind: Vec4,
    interaction: Vec4,
    interaction_motion: Vec4,
    params: Vec4,
    shading: Vec4,
}

impl From<&TacticalGrassInstancedMaterial> for TacticalGrassInstancedUniform {
    fn from(material: &TacticalGrassInstancedMaterial) -> Self {
        Self {
            wind: material.wind,
            interaction: material.interaction,
            interaction_motion: material.interaction_motion,
            params: material.params,
            shading: material.shading,
        }
    }
}

impl InstancedMaterial for TacticalGrassInstancedMaterial {
    fn vertex_shader() -> ShaderRef {
        GRASS_INSTANCED_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        GRASS_INSTANCED_SHADER.into()
    }

    /// Legacy grass renders without a prepass; keep the instanced sward
    /// identical so the depth interplay with the terrain detail patch and
    /// transparent weather stays unchanged.
    fn disable_prepass(&self) -> bool {
        true
    }

    fn specialize(
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: Self::Data,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Thin ribbons are shaded double-sided, and the LOD dither converts
        // to hardware coverage samples under MSAA exactly like the legacy
        // AlphaToCoverage foliage material.
        descriptor.primitive.cull_mode = None;
        if descriptor.multisample.count > 1 {
            descriptor.multisample.alpha_to_coverage_enabled = true;
        }
        Ok(())
    }
}

/// Smoothing state for the instanced-grass interaction uniforms; the legacy
/// `GrassInteractionState` keeps serving the remaining foliage materials.
#[derive(Resource, Default)]
pub(in crate::presentation) struct InstancedGrassInteractionState {
    previous_position: Option<Vec3>,
    smoothed_velocity: Vec3,
    /// Last values written to the materials, to skip redundant asset writes
    /// (each write re-uploads the uniform and re-queues the batch).
    written: Option<(Vec3, Vec3)>,
}

fn update_instanced_grass_interaction(
    time: Res<Time>,
    interactors: Query<&GlobalTransform, With<GrassInteractor>>,
    mut state: ResMut<InstancedGrassInteractionState>,
    mut materials: ResMut<Assets<TacticalGrassInstancedMaterial>>,
) {
    let Some(position) = interactors.iter().next().map(GlobalTransform::translation) else {
        if state.written.take().is_some() {
            for (_, material) in materials.iter_mut() {
                material.interaction = Vec4::ZERO;
                material.interaction_motion = Vec4::ZERO;
            }
        }
        state.previous_position = None;
        state.smoothed_velocity = Vec3::ZERO;
        return;
    };
    let delta_seconds = time.delta_secs().max(1.0 / 240.0);
    let velocity = state
        .previous_position
        .map(|previous| ((position - previous) / delta_seconds).clamp_length_max(8.0))
        .unwrap_or_default();
    let response = 1.0 - (-delta_seconds * 10.0).exp();
    state.smoothed_velocity = state.smoothed_velocity.lerp(velocity, response);
    state.previous_position = Some(position);

    // Idle interactors converge to constants; stop dirtying material assets
    // once the written values are close enough that no motion is visible.
    if state
        .written
        .is_some_and(|(written_position, written_velocity)| {
            written_position.distance_squared(position) < 1e-6
                && written_velocity.distance_squared(state.smoothed_velocity) < 1e-6
        })
    {
        return;
    }

    let speed = state.smoothed_velocity.length();
    for (_, material) in materials.iter_mut() {
        material.interaction = position.extend(INTERACTION_RADIUS);
        material.interaction_motion = Vec4::new(
            state.smoothed_velocity.x,
            state.smoothed_velocity.y,
            state.smoothed_velocity.z,
            (0.7 + speed * 0.11).clamp(0.7, 1.35),
        );
    }
    state.written = Some((position, state.smoothed_velocity));
}

/// Number of tuft columns/rows that subdivide one placement cell, per tier.
fn tufts_per_cell_side(lod: GrassMeshLod) -> i32 {
    match lod {
        GrassMeshLod::Near | GrassMeshLod::NearEdge => 12,
        GrassMeshLod::Far => 4,
        GrassMeshLod::Vista => 2,
    }
}

/// Eidolon fade bands per tier: `xy` fade-in, `zw` fade-out, mirroring the
/// legacy `grass_lod_visibility` crossfade margins. The near tier's epsilon
/// fade-in keeps the dither-level math well-defined at zero distance.
///
/// `range_scale` contracts every band edge uniformly, so tier hand-offs stay
/// contiguous while the geometric sward trades reach for vertex throughput.
/// Near-tier cost scales with the square of its radius, making this the
/// single most effective grass performance lever.
fn tier_visibility_range(lod: GrassMeshLod, range_scale: f32) -> Vec4 {
    let scale = range_scale.clamp(0.35, 1.0);
    (match lod {
        // #560's legacy near band (0..14 m) splits into a full-detail field and
        // a slimmer edge ring; the ring covers most of the band's area, so most
        // near verts move to the nine-vertex 6x6 mesh. The instanced tiers
        // reproduce the legacy `grass_lod_visibility` bands so the native and
        // wasm swards fade out at the same distances (~50 m terminal).
        GrassMeshLod::Near => Vec4::new(0.0, 0.001, 4.0, 6.0),
        GrassMeshLod::NearEdge => Vec4::new(4.0, 6.0, 7.0, 14.0),
        GrassMeshLod::Far => Vec4::new(7.0, 14.0, 36.0, 44.0),
        // The vista fade-in shares the far tier's fade-out endpoints so the
        // complementary crossfade partition hands off exactly.
        GrassMeshLod::Vista => Vec4::new(34.0, 42.0, 42.0, 50.0),
    }) * scale
}

const TIERS: [GrassMeshLod; 4] = [
    GrassMeshLod::Near,
    GrassMeshLod::NearEdge,
    GrassMeshLod::Far,
    GrassMeshLod::Vista,
];

fn tier_index(lod: GrassMeshLod) -> usize {
    TIERS
        .iter()
        .position(|tier| *tier == lod)
        .expect("every grass tier is listed")
}

/// Maximum blade reach above a tuft root: authored ribbon height times the
/// largest height/species scaling the mesh generator produces.
const TUFT_HEIGHT_MARGIN_METRES: f32 = 1.5;

/// Tight bounds over a batch's actual instances. The initial implementation
/// used one whole-terrain slab per batch; a fitted box keeps Bevy's
/// batch-level frustum test and the compute cull's chunk test conservative
/// without extending the volume past the sward's real extent.
pub(super) fn fitted_batch_aabb(instances: &[InstanceData], footprint: f32) -> Aabb {
    let mut minimum = Vec3::splat(f32::INFINITY);
    let mut maximum = Vec3::splat(f32::NEG_INFINITY);
    for instance in instances {
        minimum = minimum.min(instance.position);
        maximum = maximum.max(instance.position);
    }
    let margin = Vec3::new(footprint, TUFT_HEIGHT_MARGIN_METRES, footprint);
    let minimum = minimum - margin * Vec3::new(1.0, 0.2, 1.0);
    let maximum = maximum + margin;
    Aabb {
        center: ((minimum + maximum) * 0.5).into(),
        half_extents: ((maximum - minimum) * 0.5).into(),
    }
}

/// CPU-side sampler over the same feathered cover mask the legacy renderer
/// binds as a texture.
struct CoverageMask {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
    ground_width: f32,
    ground_depth: f32,
}

impl CoverageMask {
    fn new(ground: &SceneGround, seed: u64) -> Self {
        let (width, height, pixels) = grass_cover_mask_pixels(ground, seed);
        Self {
            width: width as usize,
            height: height as usize,
            pixels,
            ground_width: ground.width(),
            ground_depth: ground.depth(),
        }
    }

    fn coverage_byte(&self, world: Vec2) -> u8 {
        let u = (world.x / self.ground_width + 0.5).clamp(0.0, 1.0);
        let v = (world.y / self.ground_depth + 0.5).clamp(0.0, 1.0);
        let x = ((u * self.width as f32) as usize).min(self.width - 1);
        let y = ((v * self.height as f32) as usize).min(self.height - 1);
        self.pixels[y * self.width + x]
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<TacticalGrassInstancedMaterial>,
    terrain: &SceneTerrain,
    ground: &SceneGround,
    environment: &SceneEnvironment,
    base_seed: u64,
    profile: GrassCommunityProfile,
    grass_color: Color,
    grass_density: f32,
    grass_dryness: f32,
    wind_scale: f32,
    range_scale: f32,
) {
    let mask = CoverageMask::new(ground, stable_text_seed(&environment.scene_digest));

    // One batch per (tier, species): 24 instance populations sharing four
    // materials and twenty-four small tuft meshes.
    let mut batches: [[Vec<InstanceData>; GrassSpecies::ALL.len()]; TIERS.len()] =
        Default::default();

    for lod in [GrassMeshLod::Near, GrassMeshLod::Far] {
        scatter_cell_tufts(
            &mut batches[tier_index(lod)],
            terrain,
            ground,
            &mask,
            base_seed,
            profile,
            lod,
            GRASS_PATCH_SPACING,
        );
    }
    // The near-edge ring reuses the exact near-field placements so the
    // 8-10 m dither crossfade morphs each tuft in place instead of moving it.
    for species in GrassSpecies::ALL {
        batches[tier_index(GrassMeshLod::NearEdge)][species.index()] =
            batches[tier_index(GrassMeshLod::Near)][species.index()].clone();
    }
    scatter_cell_tufts(
        &mut batches[tier_index(GrassMeshLod::Vista)],
        terrain,
        ground,
        &mask,
        base_seed ^ 0x7669_7374_615f_6c6f,
        profile,
        GrassMeshLod::Vista,
        VISTA_GRASS_PATCH_SPACING,
    );

    for lod in TIERS {
        let material = materials.add(TacticalGrassInstancedMaterial {
            wind: Vec4::new(0.74, 0.67, wind_scale, 1.35),
            interaction: Vec4::ZERO,
            interaction_motion: Vec4::ZERO,
            params: Vec4::new(
                0.52,
                grass_dryness,
                0.09,
                lod.width_compensation(grass_density),
            ),
            // Every tier - the near field included - shades fast. The near
            // field already crossfaded seamlessly into this model at 8-10 m,
            // so its former full PBR was pure per-fragment cost. y=0.72 scales
            // the flat ambient to stand in for the skipped sky IBL.
            shading: Vec4::new(1.0, 0.72, 0.0, 0.0),
        });
        for species in GrassSpecies::ALL {
            let instances = std::mem::take(&mut batches[tier_index(lod)][species.index()]);
            if instances.is_empty() {
                continue;
            }
            let mesh = meshes.add(grass_tuft_mesh(
                grass_color,
                lod,
                grass_density,
                species,
                splitmix64(base_seed ^ ((species.index() as u64) << 8 | tier_index(lod) as u64)),
            ));
            commands.spawn((
                Name::new(format!(
                    "Instanced grass {species:?} {lod:?} tufts ({})",
                    instances.len()
                )),
                GroundScatterLayer::Grass,
                GpuCullCompute,
                // Batches span the whole scene, so CPU frustum culling can
                // only ever hide them wholesale - and worse, a culled frame
                // drops the batch from `RenderMeshInstances`, which makes
                // eidolon free and re-upload the retained instance buffers
                // every time the camera pitch crosses the horizon. Culling
                // belongs solely to the GPU compute pass.
                NoFrustumCulling,
                Mesh3d(mesh),
                InstancedMeshMaterial(material.clone()),
                fitted_batch_aabb(&instances, tuft_footprint_metres(lod)),
                InstanceMaterialData {
                    instances: Arc::new(instances),
                    color: LinearRgba::WHITE,
                    visibility_range: tier_visibility_range(lod, range_scale),
                },
                Transform::default(),
                Visibility::Inherited,
            ));
        }
    }
}

/// Walks the legacy jittered placement cells and fills per-species instance
/// vectors with tuft placements.
#[allow(clippy::too_many_arguments)]
fn scatter_cell_tufts(
    species_batches: &mut [Vec<InstanceData>; GrassSpecies::ALL.len()],
    terrain: &SceneTerrain,
    ground: &SceneGround,
    mask: &CoverageMask,
    base_seed: u64,
    profile: GrassCommunityProfile,
    lod: GrassMeshLod,
    cell_spacing: f32,
) -> u32 {
    let half_x = terrain.width() * 0.5;
    let half_z = terrain.depth() * 0.5;
    let minimum_x = (-half_x / cell_spacing).floor() as i32;
    let maximum_x = (half_x / cell_spacing).ceil() as i32;
    let minimum_z = (-half_z / cell_spacing).floor() as i32;
    let maximum_z = (half_z / cell_spacing).ceil() as i32;
    let side = tufts_per_cell_side(lod);
    let footprint = tuft_footprint_metres(lod);
    let mut emitted = 0_u32;
    for z in minimum_z..=maximum_z {
        for x in minimum_x..=maximum_x {
            let cell = ((x as u32 as u64) << 32) | z as u32 as u64;
            let cell_hash = splitmix64(base_seed ^ cell);
            if !cell_allows_grass(terrain, ground, cell_hash, x, z, cell_spacing) {
                continue;
            }
            let cell_origin = Vec2::new(x as f32, z as f32) * cell_spacing
                - Vec2::splat((side - 1) as f32 * 0.5 * footprint);
            for tuft_z in 0..side {
                for tuft_x in 0..side {
                    let tuft_hash =
                        splitmix64(cell_hash ^ (((tuft_x as u64) << 17) | ((tuft_z as u64) << 3)));
                    let jitter = Vec2::new(
                        unit_hash(tuft_hash) - 0.5,
                        unit_hash(splitmix64(tuft_hash)) - 0.5,
                    ) * footprint
                        * 0.35;
                    let centre =
                        cell_origin + Vec2::new(tuft_x as f32, tuft_z as f32) * footprint + jitter;
                    let coverage = mask.coverage_byte(centre);
                    if coverage == 0 {
                        continue;
                    }
                    let Some(height) = terrain.height_at(centre) else {
                        continue;
                    };
                    if terrain
                        .normal_at(centre)
                        .is_none_or(|normal| normal.y < 0.72)
                    {
                        continue;
                    }
                    let community = grass_community_at(centre, base_seed, profile);
                    let species =
                        grass_species(community, splitmix64(tuft_hash ^ 0x7475_6674_5f63_656c));
                    let batch = &mut species_batches[species.index()];
                    batch.push(InstanceData {
                        position: Vec3::new(centre.x, height, centre.y),
                        scale: 1.0,
                        rotation: unit_hash(splitmix64(tuft_hash ^ 0x796177))
                            * core::f32::consts::TAU,
                        index: batch.len() as u32,
                        batch_id: 0,
                        seed: u32::from(coverage)
                            | ((splitmix64(tuft_hash ^ 0x736565_64) as u32) << 8),
                    });
                    emitted += 1;
                }
            }
        }
    }
    emitted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::ground_scatter::grass::{grass_lod_visibility, tuft_blade_side};

    #[test]
    fn instanced_tiers_reproduce_legacy_per_cell_shoot_totals() {
        // Legacy: 96x96 near shoots, 40x40 far shoots, 24x24 vista shoots
        // per placement cell. The near-edge ring deliberately thins the
        // legacy near total to 6x6 blades per tuft past ~9 m.
        for (lod, legacy_total) in [
            (GrassMeshLod::Near, 96 * 96),
            (GrassMeshLod::NearEdge, 12 * 12 * 36),
            (GrassMeshLod::Far, 1_600),
            (GrassMeshLod::Vista, 576),
        ] {
            let side = tufts_per_cell_side(lod) as usize;
            let blades = tuft_blade_side(lod) * tuft_blade_side(lod);
            assert_eq!(side * side * blades, legacy_total, "{lod:?}");
        }
    }

    #[test]
    fn instanced_fade_bands_match_the_legacy_visibility_ranges() {
        // The legacy near band's 18..26 fade-out is owned by the near-edge
        // sub-tier; the near field hands off to the edge ring at 8..10.
        for lod in [
            GrassMeshLod::NearEdge,
            GrassMeshLod::Far,
            GrassMeshLod::Vista,
        ] {
            let range = tier_visibility_range(lod, 1.0);
            let legacy = grass_lod_visibility(lod);
            assert_eq!(range.x, legacy.start_margin.start, "{lod:?}");
            assert_eq!(range.y, legacy.start_margin.end, "{lod:?}");
            assert_eq!(range.z, legacy.end_margin.start, "{lod:?}");
            assert_eq!(range.w, legacy.end_margin.end, "{lod:?}");
        }
        let near = tier_visibility_range(GrassMeshLod::Near, 1.0);
        let edge = tier_visibility_range(GrassMeshLod::NearEdge, 1.0);
        assert_eq!(near.z, edge.x);
        assert_eq!(near.w, edge.y);
        for lod in TIERS {
            let range = tier_visibility_range(lod, 1.0);
            assert!(range.x <= range.y && range.y <= range.z && range.z < range.w);
        }
    }

    #[test]
    fn contracted_fade_bands_stay_contiguous_across_tiers() {
        for scale in [0.35, 0.6, 0.75, 1.0] {
            let near = tier_visibility_range(GrassMeshLod::Near, scale);
            let edge = tier_visibility_range(GrassMeshLod::NearEdge, scale);
            let far = tier_visibility_range(GrassMeshLod::Far, scale);
            let vista = tier_visibility_range(GrassMeshLod::Vista, scale);
            assert_eq!(near.z, edge.x);
            assert_eq!(near.w, edge.y);
            assert_eq!(edge.z, far.x);
            assert_eq!(edge.w, far.y);
            assert!(far.z >= vista.x && far.w >= vista.y);
            for range in [near, edge, far, vista] {
                assert!(range.x <= range.y && range.y <= range.z && range.z < range.w);
            }
        }
        // Out-of-range requests clamp instead of collapsing the sward.
        assert_eq!(
            tier_visibility_range(GrassMeshLod::Near, 0.0),
            tier_visibility_range(GrassMeshLod::Near, 0.35)
        );
    }

    #[test]
    fn fitted_batch_aabb_bounds_all_instances_with_blade_headroom() {
        let instances = vec![
            InstanceData {
                position: Vec3::new(-4.0, 1.0, 6.0),
                scale: 1.0,
                ..Default::default()
            },
            InstanceData {
                position: Vec3::new(9.0, 3.0, -2.0),
                scale: 1.0,
                ..Default::default()
            },
        ];
        let aabb = fitted_batch_aabb(&instances, 0.5);
        let minimum = aabb.center - aabb.half_extents;
        let maximum = aabb.center + aabb.half_extents;
        assert!(minimum.x <= -4.5 && maximum.x >= 9.5);
        assert!(minimum.z <= -2.5 && maximum.z >= 6.5);
        assert!(maximum.y >= 3.0 + TUFT_HEIGHT_MARGIN_METRES);
        assert!(minimum.y <= 1.0);
    }

    #[test]
    fn instance_seed_low_byte_carries_placement_coverage() {
        let coverage = 173_u8;
        let seed = u32::from(coverage) | (0xdead_beef_u32 << 8);
        assert_eq!((seed & 0xff) as u8, coverage);
    }
}
