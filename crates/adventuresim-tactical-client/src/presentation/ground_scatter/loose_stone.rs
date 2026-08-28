use adventuresim_tactical_core::prelude::{GroundCover, RockLithology, SceneGround, SceneTerrain};
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::VisibilityRange,
    color::ColorToComponents,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    pbr::Material,
    prelude::{
        Asset, Assets, Color, Commands, Component, Mesh, Mesh3d, MeshMaterial3d, Name, Quat,
        Reflect, Transform, Vec2, Vec3, Vec4,
    },
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};
use fabelgeist_determinism::splitmix64;

use crate::presentation::obstacles::rock::rock_color;
use crate::presentation::unit_hash;

use super::GroundScatterLayer;

const PEBBLE_BILLBOARD_SHADER: &str = "shaders/tactical_pebble_billboard.wgsl";
const PEBBLE_SHADER: &str = "shaders/tactical_pebble.wgsl";
// Flat-ambient scale standing in for the sky IBL the old StandardMaterial path
// evaluated. Solid rock carries no diffuse transmission, so the lit face is
// energy-neutral with PBR; only this ambient term is an approximation.
const PEBBLE_AMBIENT_SCALE: f32 = 0.85;
const MESH_VARIANTS: u64 = 8;
const PEBBLE_CANDIDATES_PER_PATCH: usize = 49;
const PEBBLE_PATCH_COLUMNS: usize = 7;
const PEBBLE_PATCH_ROWS: usize = PEBBLE_CANDIDATES_PER_PATCH / PEBBLE_PATCH_COLUMNS;
const STANDARD_PEBBLE_RADIAL_SEGMENTS: usize = 6;
#[cfg(test)]
const STANDARD_PEBBLE_VERTICES: usize = STANDARD_PEBBLE_RADIAL_SEGMENTS * 2 + 2;
#[cfg(test)]
const STANDARD_PEBBLE_TRIANGLES: usize = STANDARD_PEBBLE_RADIAL_SEGMENTS * 4;
const HERO_PEBBLE_RADIAL_SEGMENTS: usize = 16;
const HERO_PEBBLE_RING_COUNT: usize = 3;
const HERO_PEBBLE_VERTICES: usize = HERO_PEBBLE_RADIAL_SEGMENTS * HERO_PEBBLE_RING_COUNT + 2;
#[cfg(test)]
const HERO_PEBBLE_TRIANGLES: usize = HERO_PEBBLE_RADIAL_SEGMENTS * HERO_PEBBLE_RING_COUNT * 2;
const BILLBOARD_VERTICES: usize = 4;
const BILLBOARD_TRIANGLES: usize = 2;
const MIN_PEBBLE_RADIUS_METRES: f32 = 0.03;
const MAX_PEBBLE_RADIUS_METRES: f32 = 0.08;
const STONE_NOISE_X_STRIDE: u64 = 0x9e37_79b9_7f4a_7c15;
const STONE_NOISE_Y_STRIDE: u64 = 0xbf58_476d_1ce4_e5b9;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(crate) struct TacticalPebbleBillboardMaterial {
    #[uniform(0)]
    color: Vec4,
    #[uniform(0)]
    pub(super) lighting: Vec4,
    #[uniform(0)]
    pub(super) ambient: Vec4,
}

impl Material for TacticalPebbleBillboardMaterial {
    fn vertex_shader() -> ShaderRef {
        PEBBLE_BILLBOARD_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        PEBBLE_BILLBOARD_SHADER.into()
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &bevy::pbr::MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &bevy::mesh::MeshVertexBufferLayoutRef,
        _key: bevy::pbr::MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// Cheap fragment path for the hero and near pebble LODs. Reuses the shared
/// patch meshes and their automatic batching; only the shading model changes,
/// dropping the per-fragment image-based lighting and specular the previous
/// `StandardMaterial` path evaluated for foreground rock.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub(crate) struct TacticalPebbleMaterial {
    /// Linear albedo in rgb, flat-ambient scale in w.
    #[uniform(0)]
    surface: Vec4,
}

impl TacticalPebbleMaterial {
    fn new(color: Color) -> Self {
        Self {
            surface: Vec3::from_array(color.to_linear().to_f32_array_no_alpha())
                .extend(PEBBLE_AMBIENT_SCALE),
        }
    }
}

impl Material for TacticalPebbleMaterial {
    fn vertex_shader() -> ShaderRef {
        PEBBLE_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        PEBBLE_SHADER.into()
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

#[derive(Component)]
pub(crate) struct LooseStonePebblePatch {
    pub(crate) physical_pebbles: usize,
}

impl LooseStonePebblePatch {
    fn hero(physical_pebbles: usize) -> Self {
        let patch = Self { physical_pebbles };
        debug_assert!((1..=PEBBLE_CANDIDATES_PER_PATCH).contains(&patch.physical_pebbles));
        patch
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PebbleMeshLod {
    Hero,
    Near,
    Billboard,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PebbleDensity {
    Woodland,
    Sparse,
    Dense,
}

impl PebbleDensity {
    const ALL: [Self; 3] = [Self::Woodland, Self::Sparse, Self::Dense];

    const fn asset_offset(self) -> usize {
        match self {
            Self::Woodland => 0,
            Self::Sparse => MESH_VARIANTS as usize,
            Self::Dense => MESH_VARIANTS as usize * 2,
        }
    }
}

pub(super) fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    pebble_materials: &mut Assets<TacticalPebbleMaterial>,
    billboard_materials: &mut Assets<TacticalPebbleBillboardMaterial>,
    terrain: &SceneTerrain,
    ground: &SceneGround,
    base_seed: u64,
) {
    let half_extent = ground.grid_scale() * 0.5;
    let mut hero_meshes = Vec::new();
    let mut near_meshes = Vec::new();
    let mut billboard_meshes = Vec::new();
    let mut pebble_counts = Vec::new();
    for density in PebbleDensity::ALL {
        for variant in 0..MESH_VARIANTS {
            let seed = splitmix64(0x7065_6262_6c65_0000 ^ variant);
            let hero = pebble_patch_mesh(seed, PebbleMeshLod::Hero, half_extent, density);
            pebble_counts.push(hero.count_vertices() / HERO_PEBBLE_VERTICES);
            hero_meshes.push(meshes.add(hero));
            near_meshes.push(meshes.add(pebble_patch_mesh(
                seed,
                PebbleMeshLod::Near,
                half_extent,
                density,
            )));
            billboard_meshes.push(meshes.add(pebble_billboard_patch_mesh(
                seed,
                half_extent,
                density,
            )));
        }
    }
    let stone_material = pebble_materials.add(TacticalPebbleMaterial::new(rock_color(
        RockLithology::Granite,
    )));
    let woodland_stone_material =
        pebble_materials.add(TacticalPebbleMaterial::new(Color::srgb_u8(104, 91, 70)));
    let billboard_material = billboard_materials.add(TacticalPebbleBillboardMaterial {
        color: Vec4::from_array(
            rock_color(RockLithology::Granite)
                .to_linear()
                .to_f32_array(),
        ),
        lighting: Vec3::new(0.35, 0.86, 0.25).normalize().extend(1.0),
        ambient: Vec4::new(1.0, 1.0, 1.0, 0.28),
    });

    for (index, sample) in ground.samples().iter().enumerate() {
        if !matches!(
            sample.cover,
            GroundCover::LooseStone | GroundCover::LeafLitter
        ) {
            continue;
        }
        let grid_x = index % ground.grid_width();
        let grid_z = index / ground.grid_width();
        let position = Vec2::new(
            grid_x as f32 * ground.grid_scale() - ground.width() * 0.5,
            grid_z as f32 * ground.grid_scale() - ground.depth() * 0.5,
        );
        let (Some(height), Some(normal)) =
            (terrain.height_at(position), terrain.normal_at(position))
        else {
            continue;
        };
        if normal.y < 0.72 {
            continue;
        }
        let hash = splitmix64(base_seed ^ index as u64 ^ 0x7374_6f6e_655f_7363);
        let woodland = sample.cover == GroundCover::LeafLitter;
        let density = if woodland {
            // Every woodland cell gets a sparse candidate patch. Individual
            // survival still leaves irregular gaps, but keeping the patches
            // continuous yields roughly one visible 3--8 cm stone per square
            // metre instead of making rocks disappear from review frames.
            PebbleDensity::Woodland
        } else {
            let coverage = scree_patch_coverage(base_seed, position, normal);
            if coverage >= 0.61 {
                PebbleDensity::Dense
            } else if coverage >= 0.34 {
                PebbleDensity::Sparse
            } else {
                continue;
            }
        };
        let variant = density.asset_offset() + (hash % MESH_VARIANTS) as usize;
        let yaw = Quat::from_rotation_y(
            unit_hash(splitmix64(hash ^ 0x55d8_093b)) * core::f32::consts::TAU,
        );
        let transform = Transform::from_xyz(
            position.x,
            height + if woodland { -0.006 } else { 0.006 },
            position.y,
        )
        .with_rotation(Quat::from_rotation_arc(Vec3::Y, normal) * yaw)
        .with_scale(Vec3::splat(if woodland { 0.58 } else { 1.0 }));

        commands.spawn((
            Name::new(if woodland {
                "Tactical woodland hero pebble patch"
            } else {
                "Tactical loose-stone hero pebble patch"
            }),
            GroundScatterLayer::LooseStone,
            LooseStonePebblePatch::hero(pebble_counts[variant]),
            NotShadowCaster,
            Mesh3d(hero_meshes[variant].clone()),
            MeshMaterial3d(if woodland {
                woodland_stone_material.clone()
            } else {
                stone_material.clone()
            }),
            pebble_lod_visibility(PebbleMeshLod::Hero),
            transform,
        ));
        commands.spawn((
            Name::new(if woodland {
                "Tactical woodland near pebble patch"
            } else {
                "Tactical loose-stone near pebble patch"
            }),
            GroundScatterLayer::LooseStone,
            LooseStonePebblePatch {
                physical_pebbles: 0,
            },
            NotShadowCaster,
            Mesh3d(near_meshes[variant].clone()),
            MeshMaterial3d(if woodland {
                woodland_stone_material.clone()
            } else {
                stone_material.clone()
            }),
            pebble_lod_visibility(PebbleMeshLod::Near),
            transform,
        ));
        if !woodland {
            commands.spawn((
                Name::new("Tactical loose-stone billboard pebble patch"),
                GroundScatterLayer::LooseStone,
                LooseStonePebblePatch {
                    physical_pebbles: 0,
                },
                // The shader yaws each pebble quad toward the camera, so the
                // mesh's static bounds would mis-cull; this rotation-safe box
                // restores frustum culling for off-screen patches.
                bevy::camera::primitives::Aabb {
                    center: bevy::math::Vec3A::new(0.0, 0.1, 0.0),
                    half_extents: bevy::math::Vec3A::new(half_extent + 0.3, 0.4, half_extent + 0.3),
                },
                NotShadowCaster,
                Mesh3d(billboard_meshes[variant].clone()),
                MeshMaterial3d(billboard_material.clone()),
                pebble_lod_visibility(PebbleMeshLod::Billboard),
                transform,
            ));
        }
    }
}

fn pebble_lod_visibility(lod: PebbleMeshLod) -> VisibilityRange {
    match lod {
        PebbleMeshLod::Hero => VisibilityRange {
            start_margin: 0.0..0.0,
            end_margin: 5.0..6.0,
            use_aabb: false,
        },
        // Even the six-centimetre minimum spans at least five QHD pixels at
        // close range; hand off only once its full diameter approaches two.
        PebbleMeshLod::Near => VisibilityRange {
            start_margin: 5.0..6.0,
            end_margin: 20.0..24.0,
            use_aabb: false,
        },
        PebbleMeshLod::Billboard => VisibilityRange {
            start_margin: 20.0..24.0,
            // The shader independently screen-fades each pebble around one
            // pixel; this range is only a conservative entity-level cutoff.
            end_margin: 160.0..180.0,
            use_aabb: false,
        },
    }
}

fn scree_patch_coverage(seed: u64, point: Vec2, normal: Vec3) -> f32 {
    let downhill = Vec2::new(normal.x, normal.z).normalize_or_zero();
    let downhill = if downhill.length_squared() > 0.001 {
        downhill
    } else {
        Vec2::X
    };
    let across = Vec2::new(-downhill.y, downhill.x);
    let broad = scree_noise(seed ^ 0x7363_7265_655f_6272, point / 4.8);
    // Stretch the second field along the fall line so loose material gathers
    // into downhill trains rather than evenly stippling the whole substrate.
    let streak_point = Vec2::new(point.dot(across) / 2.2, point.dot(downhill) / 8.5);
    let streak = scree_noise(seed ^ 0x7363_7265_655f_7374, streak_point);
    broad * 0.58 + streak * 0.42
}

fn scree_noise(seed: u64, point: Vec2) -> f32 {
    let cell = point.floor();
    let local = point - cell;
    let curve = local * local * (Vec2::splat(3.0) - local * 2.0);
    let hash = |offset: Vec2| {
        let coordinate = cell + offset;
        let x = i64::from(coordinate.x as i32) as u64;
        let y = i64::from(coordinate.y as i32) as u64;
        unit_hash(splitmix64(
            seed ^ x.wrapping_mul(STONE_NOISE_X_STRIDE) ^ y.wrapping_mul(STONE_NOISE_Y_STRIDE),
        ))
    };
    let bottom_left = hash(Vec2::ZERO);
    let bottom = bottom_left + (hash(Vec2::X) - bottom_left) * curve.x;
    let top_left = hash(Vec2::Y);
    let top = top_left + (hash(Vec2::ONE) - top_left) * curve.x;
    bottom + (top - bottom) * curve.y
}

fn pebble_survives(
    seed: u64,
    hash: u64,
    centre: Vec2,
    half_extent: f32,
    density: PebbleDensity,
) -> bool {
    let cluster = |salt: u64| {
        Vec2::new(
            unit_hash(splitmix64(seed ^ salt)) * 2.0 - 1.0,
            unit_hash(splitmix64(seed ^ salt.rotate_left(19))) * 2.0 - 1.0,
        ) * half_extent
            * 0.7
    };
    let radius = (half_extent * 0.72).max(0.18);
    let influence = [
        cluster(0x636c_7573_7465_7201),
        cluster(0x636c_7573_7465_7202),
    ]
    .into_iter()
    .map(|cluster| (-(centre.distance_squared(cluster) / radius.powi(2)) * 1.4).exp())
    .fold(0.0_f32, f32::max);
    let chance = match density {
        PebbleDensity::Woodland => 0.045 + influence * 0.16,
        PebbleDensity::Sparse => 0.035 + influence * 0.42,
        PebbleDensity::Dense => 0.09 + influence * 0.76,
    };
    unit_hash(splitmix64(hash ^ 0x7065_6262_6c65_6b70)) < chance
}

fn pebble_patch_mesh(
    seed: u64,
    lod: PebbleMeshLod,
    half_extent: f32,
    density: PebbleDensity,
) -> Mesh {
    let (radial_segments, ring_profiles): (usize, &[(f32, f32, f32)]) = match lod {
        PebbleMeshLod::Hero => (
            HERO_PEBBLE_RADIAL_SEGMENTS,
            &[(-0.05, 0.72, -0.38), (0.42, 1.00, 0.06), (0.76, 0.76, 0.46)],
        ),
        PebbleMeshLod::Near => (
            STANDARD_PEBBLE_RADIAL_SEGMENTS,
            &[(-0.03, 0.84, -0.28), (0.58, 1.0, 0.28)],
        ),
        PebbleMeshLod::Billboard => unreachable!("billboards use their dedicated quad mesh"),
    };
    let vertices_per_pebble = radial_segments * ring_profiles.len() + 2;
    let triangles_per_pebble = radial_segments * ring_profiles.len() * 2;
    let mut positions = Vec::with_capacity(PEBBLE_CANDIDATES_PER_PATCH * vertices_per_pebble);
    let mut normals = Vec::with_capacity(PEBBLE_CANDIDATES_PER_PATCH * vertices_per_pebble);
    let mut uvs = Vec::with_capacity(PEBBLE_CANDIDATES_PER_PATCH * vertices_per_pebble);
    let mut indices = Vec::with_capacity(PEBBLE_CANDIDATES_PER_PATCH * triangles_per_pebble * 3);

    for pebble in 0..PEBBLE_CANDIDATES_PER_PATCH {
        let hash = splitmix64(seed ^ pebble as u64 ^ 0x6772_6176_656c_0001);
        let radius = MIN_PEBBLE_RADIUS_METRES
            + unit_hash(splitmix64(hash ^ 0x9137_b22c))
                * (MAX_PEBBLE_RADIUS_METRES - MIN_PEBBLE_RADIUS_METRES);
        // Jittered low-discrepancy points avoid overlap and the large random
        // holes which previously exposed the repeated shared mesh tiles.
        let column = pebble % PEBBLE_PATCH_COLUMNS;
        let row = pebble / PEBBLE_PATCH_COLUMNS;
        let jitter_x = unit_hash(splitmix64(hash ^ 0x80c4_3f12)) - 0.5;
        let jitter_z = unit_hash(splitmix64(hash ^ 0xc21a_63d4)) - 0.5;
        let centre = Vec3::new(
            ((column as f32 + 0.5 + jitter_x * 0.88) / PEBBLE_PATCH_COLUMNS as f32 * 2.0 - 1.0)
                * half_extent,
            0.0,
            ((row as f32 + 0.5 + jitter_z * 0.88) / PEBBLE_PATCH_ROWS as f32 * 2.0 - 1.0)
                * half_extent,
        );
        if !pebble_survives(
            seed,
            hash,
            Vec2::new(centre.x, centre.z),
            half_extent,
            density,
        ) {
            continue;
        }
        let height_scale = if density == PebbleDensity::Woodland {
            0.52 + unit_hash(splitmix64(hash ^ 0x4f08_d119)) * 0.30
        } else {
            0.85 + unit_hash(splitmix64(hash ^ 0x4f08_d119)) * 0.45
        };
        let height = radius * height_scale;
        let yaw = unit_hash(splitmix64(hash ^ 0x5ca1_0f77)) * core::f32::consts::TAU;
        let direction = Vec3::new(yaw.cos(), 0.0, yaw.sin());
        let tangent = Vec3::new(-direction.z, 0.0, direction.x);
        let lateral_scale = if density == PebbleDensity::Woodland {
            0.52 + unit_hash(splitmix64(hash ^ 0xd71c_820e)) * 0.34
        } else {
            0.72 + unit_hash(splitmix64(hash ^ 0xd71c_820e)) * 0.26
        };
        let base = positions.len() as u32;
        positions.push((centre - Vec3::Y * radius * 0.18).to_array());
        normals.push(Vec3::NEG_Y.to_array());
        uvs.push([0.5, 0.5]);

        for &(height_fraction, radius_scale, normal_y) in ring_profiles {
            for segment in 0..radial_segments {
                let angle = segment as f32 / radial_segments as f32 * core::f32::consts::TAU;
                let facet_scale = if density == PebbleDensity::Woodland {
                    0.82 + unit_hash(splitmix64(hash ^ segment as u64 ^ 0xa94f_3b21)) * 0.28
                } else {
                    1.0
                };
                let horizontal = direction * angle.cos() + tangent * angle.sin() * lateral_scale;
                let vertex = centre
                    + direction * angle.cos() * radius * radius_scale * facet_scale
                    + tangent * angle.sin() * radius * radius_scale * lateral_scale * facet_scale
                    + Vec3::Y * height * height_fraction;
                positions.push(vertex.to_array());
                normals.push(
                    (horizontal + Vec3::Y * normal_y)
                        .normalize_or_zero()
                        .to_array(),
                );
                uvs.push([segment as f32 / radial_segments as f32, height_fraction]);
            }
        }

        positions.push((centre + Vec3::Y * height).to_array());
        normals.push(Vec3::Y.to_array());
        uvs.push([0.5, 1.0]);

        let first_ring = base + 1;
        let top = first_ring + (radial_segments * ring_profiles.len()) as u32;
        for segment in 0..radial_segments as u32 {
            let next = (segment + 1) % radial_segments as u32;
            indices.extend_from_slice(&[base, first_ring + segment, first_ring + next]);
        }
        for ring in 0..ring_profiles.len() - 1 {
            let lower = first_ring + (ring * radial_segments) as u32;
            let upper = lower + radial_segments as u32;
            for segment in 0..radial_segments as u32 {
                let next = (segment + 1) % radial_segments as u32;
                indices.extend_from_slice(&[
                    lower + segment,
                    upper + next,
                    lower + next,
                    lower + segment,
                    upper + segment,
                    upper + next,
                ]);
            }
        }
        let last_ring = top - radial_segments as u32;
        for segment in 0..radial_segments as u32 {
            let next = (segment + 1) % radial_segments as u32;
            indices.extend_from_slice(&[last_ring + segment, top, last_ring + next]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn pebble_billboard_patch_mesh(seed: u64, half_extent: f32, density: PebbleDensity) -> Mesh {
    let mut positions = Vec::with_capacity(PEBBLE_CANDIDATES_PER_PATCH * BILLBOARD_VERTICES);
    let mut centres = Vec::with_capacity(PEBBLE_CANDIDATES_PER_PATCH * BILLBOARD_VERTICES);
    let mut uvs = Vec::with_capacity(PEBBLE_CANDIDATES_PER_PATCH * BILLBOARD_VERTICES);
    let mut indices = Vec::with_capacity(PEBBLE_CANDIDATES_PER_PATCH * BILLBOARD_TRIANGLES * 3);

    for pebble in 0..PEBBLE_CANDIDATES_PER_PATCH {
        let hash = splitmix64(seed ^ pebble as u64 ^ 0x6772_6176_656c_0001);
        let radius = MIN_PEBBLE_RADIUS_METRES
            + unit_hash(splitmix64(hash ^ 0x9137_b22c))
                * (MAX_PEBBLE_RADIUS_METRES - MIN_PEBBLE_RADIUS_METRES);
        let column = pebble % PEBBLE_PATCH_COLUMNS;
        let row = pebble / PEBBLE_PATCH_COLUMNS;
        let jitter_x = unit_hash(splitmix64(hash ^ 0x80c4_3f12)) - 0.5;
        let jitter_z = unit_hash(splitmix64(hash ^ 0xc21a_63d4)) - 0.5;
        let centre = Vec3::new(
            ((column as f32 + 0.5 + jitter_x * 0.88) / PEBBLE_PATCH_COLUMNS as f32 * 2.0 - 1.0)
                * half_extent,
            0.0,
            ((row as f32 + 0.5 + jitter_z * 0.88) / PEBBLE_PATCH_ROWS as f32 * 2.0 - 1.0)
                * half_extent,
        );
        if !pebble_survives(
            seed,
            hash,
            Vec2::new(centre.x, centre.z),
            half_extent,
            density,
        ) {
            continue;
        }
        let height = radius * (0.85 + unit_hash(splitmix64(hash ^ 0x4f08_d119)) * 0.45);
        let sprite_centre = centre + Vec3::Y * height * 0.5;
        let base = positions.len() as u32;

        for (offset, uv) in [
            ([-radius, -height * 0.5, 0.0], [0.0, 0.0]),
            ([radius, -height * 0.5, 0.0], [1.0, 0.0]),
            ([radius, height * 0.5, 0.0], [1.0, 1.0]),
            ([-radius, height * 0.5, 0.0], [0.0, 1.0]),
        ] {
            positions.push(offset);
            // The billboard shader repurposes this otherwise unused vertex
            // channel as the per-quad local centre within the shared patch.
            centres.push(sprite_centre.to_array());
            uvs.push(uv);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, centres);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pebble_lods_use_rounded_meshes_and_one_quad_per_distant_stone() {
        let hero = pebble_patch_mesh(7, PebbleMeshLod::Hero, 1.0, PebbleDensity::Dense);
        let near = pebble_patch_mesh(7, PebbleMeshLod::Near, 1.0, PebbleDensity::Dense);
        let billboard = pebble_billboard_patch_mesh(7, 1.0, PebbleDensity::Dense);
        let pebble_count = hero.count_vertices() / HERO_PEBBLE_VERTICES;
        assert!((8..=32).contains(&pebble_count), "{pebble_count}");
        assert_eq!(hero.count_vertices(), pebble_count * HERO_PEBBLE_VERTICES);
        assert_eq!(
            hero.indices().unwrap().len(),
            pebble_count * HERO_PEBBLE_TRIANGLES * 3
        );
        assert_eq!(
            near.count_vertices(),
            pebble_count * STANDARD_PEBBLE_VERTICES
        );
        assert_eq!(
            near.indices().unwrap().len(),
            pebble_count * STANDARD_PEBBLE_TRIANGLES * 3
        );
        assert_eq!(
            billboard.count_vertices(),
            pebble_count * BILLBOARD_VERTICES
        );
        assert_eq!(
            billboard.indices().unwrap().len(),
            pebble_count * BILLBOARD_TRIANGLES * 3
        );

        let positions = match hero.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
            bevy::mesh::VertexAttributeValues::Float32x3(values) => values,
            other => panic!("unexpected pebble positions {other:?}"),
        };
        let normals = match hero.attribute(Mesh::ATTRIBUTE_NORMAL).unwrap() {
            bevy::mesh::VertexAttributeValues::Float32x3(values) => values,
            other => panic!("unexpected pebble normals {other:?}"),
        };
        let Indices::U32(indices) = hero.indices().unwrap() else {
            panic!("pebble mesh should use u32 indices");
        };
        for triangle in indices.as_chunks::<3>().0 {
            let [a, b, c] = *triangle;
            let a = Vec3::from_array(positions[a as usize]);
            let b = Vec3::from_array(positions[b as usize]);
            let c = Vec3::from_array(positions[c as usize]);
            let face = (b - a).cross(c - a).normalize_or_zero();
            let expected = triangle
                .iter()
                .map(|index| Vec3::from_array(normals[*index as usize]))
                .sum::<Vec3>()
                .normalize_or_zero();
            assert!(
                face.dot(expected) > 0.05,
                "outward normals and front-face winding disagree: {face:?} versus {expected:?}"
            );
        }
    }

    #[test]
    fn scree_density_is_clustered_and_preserved_across_lods() {
        let woodland = pebble_patch_mesh(19, PebbleMeshLod::Hero, 1.0, PebbleDensity::Woodland)
            .count_vertices()
            / HERO_PEBBLE_VERTICES;
        let sparse = pebble_patch_mesh(19, PebbleMeshLod::Hero, 1.0, PebbleDensity::Sparse)
            .count_vertices()
            / HERO_PEBBLE_VERTICES;
        let dense = pebble_patch_mesh(19, PebbleMeshLod::Hero, 1.0, PebbleDensity::Dense)
            .count_vertices()
            / HERO_PEBBLE_VERTICES;
        assert!(woodland > 0);
        assert!(sparse > woodland, "woodland {woodland}, sparse {sparse}");
        assert!(dense > sparse, "sparse {sparse}, dense {dense}");

        let normal = Vec3::new(0.25, 0.9, -0.15).normalize();
        let samples = (0..80)
            .map(|step| scree_patch_coverage(77, Vec2::new(step as f32 * 0.5, 3.0), normal))
            .collect::<Vec<_>>();
        assert!(
            samples
                .windows(2)
                .all(|pair| (pair[1] - pair[0]).abs() < 0.22)
        );
        assert!(
            samples.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                - samples.iter().copied().fold(f32::INFINITY, f32::min)
                > 0.25,
            "world-space scree field needs broad occupied and exposed bands"
        );
    }

    #[test]
    fn woodland_pebbles_average_half_to_one_and_a_half_stones_per_square_metre() {
        let half_extent = 1.0_f32;
        let patch_area = (half_extent * 2.0).powi(2);
        let stone_count = (0..MESH_VARIANTS)
            .map(|variant| {
                let seed = splitmix64(0x7065_6262_6c65_0000 ^ variant);
                pebble_patch_mesh(
                    seed,
                    PebbleMeshLod::Hero,
                    half_extent,
                    PebbleDensity::Woodland,
                )
                .count_vertices()
                    / HERO_PEBBLE_VERTICES
            })
            .sum::<usize>();
        let stones_per_square_metre = stone_count as f32 / (MESH_VARIANTS as f32 * patch_area);
        assert!(
            (0.5..=1.5).contains(&stones_per_square_metre),
            "woodland density was {stones_per_square_metre} stones/m2"
        );
    }

    #[test]
    fn pebble_lod_cutoffs_keep_minimum_diameters_multi_pixel_at_qhd() {
        let qhd_pixels_per_metre =
            |distance: f32| 1_440.0 / (2.0 * (40.0_f32.to_radians()).tan() * distance);
        let near_minimum_pixels = MIN_PEBBLE_RADIUS_METRES * 2.0 * qhd_pixels_per_metre(24.0);
        let hero_minimum_pixels = MIN_PEBBLE_RADIUS_METRES * 2.0 * qhd_pixels_per_metre(6.0);
        let largest_billboard_pixels = MAX_PEBBLE_RADIUS_METRES * 2.0 * qhd_pixels_per_metre(180.0);
        assert!(near_minimum_pixels >= 2.0);
        // The hero ring has sixteen segments, keeping its silhouette facets
        // short enough to appear round while remaining multi-pixel.
        let hero_chord_pixels = hero_minimum_pixels
            * (core::f32::consts::PI / HERO_PEBBLE_RADIAL_SEGMENTS as f32).sin();
        assert!(hero_chord_pixels >= 1.5);
        // A six-sided ring's chord is one radius wide, so its silhouette
        // edges remain at least one pixel until the billboard handoff.
        assert!(near_minimum_pixels * 0.5 >= 1.0);
        // Even the largest pebble is below one pixel by the conservative
        // entity cutoff; the shader screen-fades individual sizes sooner.
        assert!(largest_billboard_pixels < 1.0);
        assert_eq!(
            pebble_lod_visibility(PebbleMeshLod::Hero).end_margin,
            pebble_lod_visibility(PebbleMeshLod::Near).start_margin
        );
        assert_eq!(
            pebble_lod_visibility(PebbleMeshLod::Near).end_margin,
            pebble_lod_visibility(PebbleMeshLod::Billboard).start_margin
        );
    }

    #[test]
    fn hero_and_near_pebbles_use_the_cheap_lit_material() {
        let shader = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/shaders/tactical_pebble.wgsl"
        ));
        // Fast model: flat ambient plus a single clamped cascade fetch, the
        // same shape the distant grass tiers use.
        assert!(shader.contains("lights.ambient_color"));
        assert!(shader.contains("shadows::fetch_directional_shadow"));
        assert!(shader.contains("visibility_range_dither"));
        // Must NOT drag the foreground rock back through the full PBR path
        // that this material exists to avoid.
        assert!(!shader.contains("apply_pbr_lighting"));
        assert!(!shader.contains("pbr_input_from_standard_material"));

        // Foreground LODs render on the cheap material; only the distant
        // stochastic billboard keeps its dedicated camera-facing shader.
        let granite = TacticalPebbleMaterial::new(rock_color(RockLithology::Granite));
        assert_eq!(granite.surface.w, PEBBLE_AMBIENT_SCALE);
    }

    #[test]
    fn billboard_shader_faces_the_camera_and_screen_fades_at_one_pixel() {
        let shader = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/shaders/tactical_pebble_billboard.wgsl"
        ));
        assert!(shader.contains("view.world_position.xz - centre_world.xz"));
        assert!(shader.contains("smoothstep(0.75, 1.5, diameter_pixels)"));
        assert!(shader.contains("visibility_range_dither"));
    }
}
