use adventuresim_tactical_core::prelude::{GroundCover, RockLithology, SceneGround, SceneTerrain};
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::{NoFrustumCulling, VisibilityRange},
    color::ColorToComponents,
    light::NotShadowCaster,
    mesh::{Indices, PrimitiveTopology},
    pbr::Material,
    prelude::{
        Asset, Assets, Commands, Component, Mesh, Mesh3d, MeshMaterial3d, Name, Quat, Reflect,
        StandardMaterial, Transform, Vec2, Vec3, Vec4,
    },
    render::render_resource::{
        AsBindGroup, RenderPipelineDescriptor, SpecializedMeshPipelineError,
    },
    shader::ShaderRef,
};

use crate::presentation::obstacles::rock::rock_color;
use crate::presentation::{splitmix64, unit_hash};

use super::GroundScatterLayer;

const PEBBLE_BILLBOARD_SHADER: &str = "shaders/tactical_pebble_billboard.wgsl";
const MESH_VARIANTS: u64 = 8;
const PHYSICAL_PEBBLES_PER_PATCH: usize = 64;
const PEBBLE_PATCH_COLUMNS: usize = 8;
const PEBBLE_PATCH_ROWS: usize = PHYSICAL_PEBBLES_PER_PATCH / PEBBLE_PATCH_COLUMNS;
const STANDARD_PEBBLE_RADIAL_SEGMENTS: usize = 6;
const STANDARD_PEBBLE_VERTICES: usize = STANDARD_PEBBLE_RADIAL_SEGMENTS * 2 + 2;
const STANDARD_PEBBLE_TRIANGLES: usize = STANDARD_PEBBLE_RADIAL_SEGMENTS * 4;
const HERO_PEBBLE_RADIAL_SEGMENTS: usize = 16;
const HERO_PEBBLE_RING_COUNT: usize = 3;
const HERO_PEBBLE_VERTICES: usize = HERO_PEBBLE_RADIAL_SEGMENTS * HERO_PEBBLE_RING_COUNT + 2;
const HERO_PEBBLE_TRIANGLES: usize = HERO_PEBBLE_RADIAL_SEGMENTS * HERO_PEBBLE_RING_COUNT * 2;
const BILLBOARD_VERTICES: usize = 4;
const BILLBOARD_TRIANGLES: usize = 2;
const MIN_PEBBLE_RADIUS_METRES: f32 = 0.03;
const MAX_PEBBLE_RADIUS_METRES: f32 = 0.09;

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

#[derive(Component)]
pub(crate) struct LooseStonePebblePatch {
    pub(crate) physical_pebbles: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PebbleMeshLod {
    Hero,
    Near,
    Billboard,
}

pub(super) fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    billboard_materials: &mut Assets<TacticalPebbleBillboardMaterial>,
    terrain: &SceneTerrain,
    ground: &SceneGround,
    base_seed: u64,
) {
    let half_extent = ground.grid_scale() * 0.5;
    let hero_meshes = (0..MESH_VARIANTS)
        .map(|variant| {
            meshes.add(pebble_patch_mesh(
                splitmix64(0x7065_6262_6c65_0000 ^ variant),
                PebbleMeshLod::Hero,
                half_extent,
            ))
        })
        .collect::<Vec<_>>();
    let near_meshes = (0..MESH_VARIANTS)
        .map(|variant| {
            meshes.add(pebble_patch_mesh(
                splitmix64(0x7065_6262_6c65_0000 ^ variant),
                PebbleMeshLod::Near,
                half_extent,
            ))
        })
        .collect::<Vec<_>>();
    let billboard_meshes = (0..MESH_VARIANTS)
        .map(|variant| {
            meshes.add(pebble_billboard_patch_mesh(
                splitmix64(0x7065_6262_6c65_0000 ^ variant),
                half_extent,
            ))
        })
        .collect::<Vec<_>>();
    let stone_material = materials.add(StandardMaterial {
        base_color: rock_color(RockLithology::Granite),
        perceptual_roughness: 1.0,
        ..Default::default()
    });
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
        if sample.cover != GroundCover::LooseStone {
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
        let variant = (hash % MESH_VARIANTS) as usize;
        let yaw = Quat::from_rotation_y(
            unit_hash(splitmix64(hash ^ 0x55d8_093b)) * core::f32::consts::TAU,
        );
        let transform = Transform::from_xyz(position.x, height + 0.018, position.y)
            .with_rotation(Quat::from_rotation_arc(Vec3::Y, normal) * yaw);

        commands.spawn((
            Name::new("Tactical loose-stone hero pebble patch"),
            GroundScatterLayer::LooseStone,
            LooseStonePebblePatch {
                physical_pebbles: PHYSICAL_PEBBLES_PER_PATCH,
            },
            NotShadowCaster,
            Mesh3d(hero_meshes[variant].clone()),
            MeshMaterial3d(stone_material.clone()),
            pebble_lod_visibility(PebbleMeshLod::Hero),
            transform,
        ));
        commands.spawn((
            Name::new("Tactical loose-stone near pebble patch"),
            GroundScatterLayer::LooseStone,
            LooseStonePebblePatch {
                physical_pebbles: 0,
            },
            NotShadowCaster,
            Mesh3d(near_meshes[variant].clone()),
            MeshMaterial3d(stone_material.clone()),
            pebble_lod_visibility(PebbleMeshLod::Near),
            transform,
        ));
        commands.spawn((
            Name::new("Tactical loose-stone billboard pebble patch"),
            GroundScatterLayer::LooseStone,
            LooseStonePebblePatch {
                physical_pebbles: 0,
            },
            NoFrustumCulling,
            NotShadowCaster,
            Mesh3d(billboard_meshes[variant].clone()),
            MeshMaterial3d(billboard_material.clone()),
            pebble_lod_visibility(PebbleMeshLod::Billboard),
            transform,
        ));
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

fn pebble_patch_mesh(seed: u64, lod: PebbleMeshLod, half_extent: f32) -> Mesh {
    let (radial_segments, ring_profiles): (usize, &[(f32, f32, f32)]) = match lod {
        PebbleMeshLod::Hero => (
            HERO_PEBBLE_RADIAL_SEGMENTS,
            &[(0.12, 0.68, -0.52), (0.46, 1.00, 0.02), (0.76, 0.76, 0.46)],
        ),
        PebbleMeshLod::Near => (
            STANDARD_PEBBLE_RADIAL_SEGMENTS,
            &[(0.18, 0.82, -0.34), (0.58, 1.0, 0.28)],
        ),
        PebbleMeshLod::Billboard => unreachable!("billboards use their dedicated quad mesh"),
    };
    let vertices_per_pebble = radial_segments * ring_profiles.len() + 2;
    let triangles_per_pebble = radial_segments * ring_profiles.len() * 2;
    let mut positions = Vec::with_capacity(PHYSICAL_PEBBLES_PER_PATCH * vertices_per_pebble);
    let mut normals = Vec::with_capacity(PHYSICAL_PEBBLES_PER_PATCH * vertices_per_pebble);
    let mut uvs = Vec::with_capacity(PHYSICAL_PEBBLES_PER_PATCH * vertices_per_pebble);
    let mut indices = Vec::with_capacity(PHYSICAL_PEBBLES_PER_PATCH * triangles_per_pebble * 3);

    for pebble in 0..PHYSICAL_PEBBLES_PER_PATCH {
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
        let height = radius * (0.85 + unit_hash(splitmix64(hash ^ 0x4f08_d119)) * 0.45);
        let yaw = unit_hash(splitmix64(hash ^ 0x5ca1_0f77)) * core::f32::consts::TAU;
        let direction = Vec3::new(yaw.cos(), 0.0, yaw.sin());
        let tangent = Vec3::new(-direction.z, 0.0, direction.x);
        let lateral_scale = 0.72 + unit_hash(splitmix64(hash ^ 0xd71c_820e)) * 0.26;
        let base = positions.len() as u32;
        positions.push(centre.to_array());
        normals.push(Vec3::NEG_Y.to_array());
        uvs.push([0.5, 0.5]);

        for &(height_fraction, radius_scale, normal_y) in ring_profiles {
            for segment in 0..radial_segments {
                let angle = segment as f32 / radial_segments as f32 * core::f32::consts::TAU;
                let horizontal = direction * angle.cos() + tangent * angle.sin() * lateral_scale;
                let vertex = centre
                    + direction * angle.cos() * radius * radius_scale
                    + tangent * angle.sin() * radius * radius_scale * lateral_scale
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
            indices.extend_from_slice(&[base, first_ring + next, first_ring + segment]);
        }
        for ring in 0..ring_profiles.len() - 1 {
            let lower = first_ring + (ring * radial_segments) as u32;
            let upper = lower + radial_segments as u32;
            for segment in 0..radial_segments as u32 {
                let next = (segment + 1) % radial_segments as u32;
                indices.extend_from_slice(&[
                    lower + segment,
                    lower + next,
                    upper + next,
                    lower + segment,
                    upper + next,
                    upper + segment,
                ]);
            }
        }
        let last_ring = top - radial_segments as u32;
        for segment in 0..radial_segments as u32 {
            let next = (segment + 1) % radial_segments as u32;
            indices.extend_from_slice(&[last_ring + segment, last_ring + next, top]);
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

fn pebble_billboard_patch_mesh(seed: u64, half_extent: f32) -> Mesh {
    let mut positions = Vec::with_capacity(PHYSICAL_PEBBLES_PER_PATCH * BILLBOARD_VERTICES);
    let mut centres = Vec::with_capacity(PHYSICAL_PEBBLES_PER_PATCH * BILLBOARD_VERTICES);
    let mut uvs = Vec::with_capacity(PHYSICAL_PEBBLES_PER_PATCH * BILLBOARD_VERTICES);
    let mut indices = Vec::with_capacity(PHYSICAL_PEBBLES_PER_PATCH * BILLBOARD_TRIANGLES * 3);

    for pebble in 0..PHYSICAL_PEBBLES_PER_PATCH {
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
        let hero = pebble_patch_mesh(7, PebbleMeshLod::Hero, 1.0);
        let near = pebble_patch_mesh(7, PebbleMeshLod::Near, 1.0);
        let billboard = pebble_billboard_patch_mesh(7, 1.0);
        assert_eq!(
            hero.count_vertices(),
            PHYSICAL_PEBBLES_PER_PATCH * HERO_PEBBLE_VERTICES
        );
        assert_eq!(
            hero.indices().unwrap().len(),
            PHYSICAL_PEBBLES_PER_PATCH * HERO_PEBBLE_TRIANGLES * 3
        );
        assert_eq!(
            near.count_vertices(),
            PHYSICAL_PEBBLES_PER_PATCH * STANDARD_PEBBLE_VERTICES
        );
        assert_eq!(
            near.indices().unwrap().len(),
            PHYSICAL_PEBBLES_PER_PATCH * STANDARD_PEBBLE_TRIANGLES * 3
        );
        assert_eq!(
            billboard.count_vertices(),
            PHYSICAL_PEBBLES_PER_PATCH * BILLBOARD_VERTICES
        );
        assert_eq!(
            billboard.indices().unwrap().len(),
            PHYSICAL_PEBBLES_PER_PATCH * BILLBOARD_TRIANGLES * 3
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
