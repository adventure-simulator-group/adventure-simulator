use super::*;

#[derive(Component)]
pub(crate) struct ImplicitTerrainPatchVisual;

#[derive(Component)]
pub(crate) struct ImplicitTerrainPatchSurface;

#[derive(Component)]
pub(crate) struct ImplicitTerrainTileGround {
    source_terrain: Entity,
}

#[derive(Component)]
pub(crate) struct PendingImplicitTerrainTile;

fn unit_smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn course_color_weight(relative_height: f32, center: f32, warp: f32) -> f32 {
    1.0 - unit_smoothstep(
        (((relative_height - center - warp).abs() - 0.047) / 0.111).clamp(0.0, 1.0),
    )
}

const UNDERCUT_WARM_SHADOW: [f32; 4] = [0.62, 0.34, 0.21, 1.0];

fn implicit_tile_terrace_height_local(
    recipe: RiverBluffRecipe,
    terrain: &SceneTerrain,
    convergence_start: f32,
    local_x: f32,
    local_z: f32,
) -> Option<f32> {
    let world = recipe.local_to_world(Vec3::new(local_x, 0.0, local_z));
    let terrain_height_local = terrain.height_at(world.xz())? - recipe.center_metres().y;
    let rear_blend = recipe.rear_terrace_inheritance(local_z, convergence_start);
    Some(
        recipe.local_crest_height(local_x) * (1.0 - rear_blend) + terrain_height_local * rear_blend,
    )
}

fn implicit_tile_landform_influence(recipe: RiverBluffRecipe, local: Vec3) -> f32 {
    let [minimum_x, maximum_x, minimum_z, maximum_z] = recipe.implicit_tile_bounds_local();
    let spacing = f32::from(recipe.sample_spacing_cm) / 100.0;
    let half_face_width = recipe.dimensions_metres().x * 0.5;

    // Keep the authored multi-valued face intact through the active central bluff, then let the
    // returned shoulders become ordinary terrain over several metres. The zero-weight contour is
    // deliberately well inside the distant tile perimeter, so the scalar is exactly the terrain
    // field at every tile edge rather than relying on a clipped finite closure.
    let lateral_full = half_face_width * 0.66;
    let lateral_zero =
        (half_face_width + 4.0).min(maximum_x.abs().min(minimum_x.abs()) - spacing * 3.0);
    let lateral = 1.0
        - unit_smoothstep(
            ((local.x.abs() - lateral_full) / (lateral_zero - lateral_full)).clamp(0.0, 1.0),
        );

    let face_front = recipe.minimum_face_local_z(local.x);
    let face_back = recipe.maximum_face_local_z(local.x);
    let front_zero = (face_front - 5.0).max(minimum_z + spacing * 3.0);
    let front_full = face_front - 0.8;
    let rear_full = face_back + 3.0;
    let rear_zero = (face_back + 10.0).min(maximum_z - spacing * 3.0);
    let front = unit_smoothstep(
        ((local.z - front_zero) / (front_full - front_zero).max(spacing)).clamp(0.0, 1.0),
    );
    let rear = 1.0
        - unit_smoothstep(
            ((local.z - rear_full) / (rear_zero - rear_full).max(spacing)).clamp(0.0, 1.0),
        );
    lateral * front * rear
}

fn implicit_tile_field(
    recipe: RiverBluffRecipe,
    terrain: &SceneTerrain,
    convergence_start: f32,
    local: Vec3,
) -> f32 {
    let world = recipe.local_to_world(local);
    let Some(height) = terrain.height_at(world.xz()) else {
        return recipe.signed_distance(world);
    };
    let terrain_surface = world.y - height;
    let terrace_height =
        implicit_tile_terrace_height_local(recipe, terrain, convergence_start, local.x, local.z)
            .expect("terrain was sampled at the same point above");
    let authored_bluff =
        (local.y - terrace_height).max(recipe.face_surface_local_z(local) - local.z);
    let horizontal_influence = implicit_tile_landform_influence(recipe, local);
    // A linear blend against the face term must itself return to terrain below the toe. Without
    // this smooth terrain-relative gate, a partially weighted vertical-face value can create an
    // arbitrarily deep zero crossing in the shoulder transition. This is not a finite bottom
    // closure: below the shallow weathered toe zone the scalar becomes the ordinary terrain field.
    let terrain_height_local = height - recipe.center_metres().y;
    let below_toe =
        unit_smoothstep(((local.y - (terrain_height_local - 1.35)) / 1.05).clamp(0.0, 1.0));
    let influence = horizontal_influence * below_toe;
    terrain_surface + (authored_bluff - terrain_surface) * influence
}

fn finish_implicit_tile_boundary(
    surface: &mut ExtractedSurface,
    recipe: RiverBluffRecipe,
    terrain: &SceneTerrain,
) {
    let [minimum_x, maximum_x, minimum_z, maximum_z] = recipe.implicit_tile_bounds_local();
    surface.indices = surface
        .indices
        .chunks_exact(3)
        .filter(|triangle| {
            let center = triangle
                .iter()
                .map(|index| Vec3::from_array(surface.positions[*index as usize]))
                .sum::<Vec3>()
                / 3.0;
            (minimum_x..=maximum_x).contains(&center.x)
                && (minimum_z..=maximum_z).contains(&center.z)
        })
        .flat_map(|triangle| triangle.iter().copied())
        .collect();

    let mut edge_counts = std::collections::HashMap::<(u32, u32), u8>::new();
    for triangle in surface.indices.chunks_exact(3) {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let edge = if a < b { (a, b) } else { (b, a) };
            *edge_counts.entry(edge).or_default() += 1;
        }
    }
    let boundary_vertices = edge_counts
        .into_iter()
        .filter(|(_, count)| *count == 1)
        .flat_map(|((a, b), _)| [a, b])
        .collect::<std::collections::HashSet<_>>();
    let tolerance = f32::from(recipe.sample_spacing_cm) / 100.0 * 2.5;
    for index in boundary_vertices {
        let mut local = Vec3::from_array(surface.positions[index as usize]);
        let candidates = [
            ((local.x - minimum_x).abs(), 0_u8),
            ((local.x - maximum_x).abs(), 1_u8),
            ((local.z - minimum_z).abs(), 2_u8),
            ((local.z - maximum_z).abs(), 3_u8),
        ];
        let (distance, edge) = candidates
            .into_iter()
            .min_by(|left, right| left.0.total_cmp(&right.0))
            .expect("tile has four perimeter edges");
        if distance > tolerance {
            continue;
        }
        match edge {
            0 => local.x = minimum_x,
            1 => local.x = maximum_x,
            2 => local.z = minimum_z,
            3 => local.z = maximum_z,
            _ => unreachable!(),
        }
        let boundary_world = recipe.local_to_world(Vec3::new(local.x, 0.0, local.z));
        let Some(height) = terrain.height_at(boundary_world.xz()) else {
            continue;
        };
        local.y = height - recipe.center_metres().y;
        surface.positions[index as usize] = local.to_array();
    }

    let mut accumulated = vec![Vec3::ZERO; surface.positions.len()];
    for triangle in surface.indices.chunks_exact(3) {
        let [a, b, c] = [triangle[0], triangle[1], triangle[2]]
            .map(|index| Vec3::from_array(surface.positions[index as usize]));
        let normal = (b - a).cross(c - a);
        for index in triangle {
            accumulated[*index as usize] += normal;
        }
    }
    for (index, normal) in accumulated.into_iter().enumerate() {
        if let Some(normal) = normal.try_normalize() {
            surface.normals[index] = normal.to_array();
        }
    }
}

fn partition_implicit_tile_triangles(
    surface: &ExtractedSurface,
    recipe: RiverBluffRecipe,
    terrain: &SceneTerrain,
) -> (Vec<u32>, Vec<u32>) {
    let mut ground = Vec::with_capacity(surface.indices.len());
    let mut rock = Vec::with_capacity(surface.indices.len());
    for triangle in surface.indices.chunks_exact(3) {
        if implicit_tile_triangle_is_rock(surface, triangle, recipe, terrain) {
            rock.extend_from_slice(triangle);
        } else {
            ground.extend_from_slice(triangle);
        }
    }
    (ground, rock)
}

fn implicit_tile_triangle_is_rock(
    surface: &ExtractedSurface,
    triangle: &[u32],
    recipe: RiverBluffRecipe,
    _terrain: &SceneTerrain,
) -> bool {
    let spacing = f32::from(recipe.sample_spacing_cm) / 100.0;
    let vertices = [triangle[0], triangle[1], triangle[2]]
        .map(|index| Vec3::from_array(surface.positions[index as usize]));
    let center = vertices.into_iter().sum::<Vec3>() / 3.0;
    let upward = (vertices[1] - vertices[0])
        .cross(vertices[2] - vertices[0])
        .normalize_or_zero()
        .y
        .abs();
    let crest = recipe.local_crest_height(center.x);
    let face = recipe.face_surface_local_z(center);
    let [minimum_x, maximum_x, minimum_z, maximum_z] = recipe.implicit_tile_bounds_local();
    let distant_perimeter = (center.x - minimum_x).abs() < 1.4
        || (center.x - maximum_x).abs() < 1.4
        || (center.z - minimum_z).abs() < 1.4
        || (center.z - maximum_z).abs() < 1.4;
    let landform_influence = implicit_tile_landform_influence(recipe, center);
    // The authored scarp semantic, not equality with the coarse authoritative
    // heightfield, owns sandstone. This includes the low toe and undercut but
    // excludes the crest plane, upper terrace, returned ground, and distant
    // tile perimeter even when those surfaces differ by a quantization cell.
    !distant_perimeter
        && ((upward < 0.62 && landform_influence > 0.01)
            || (upward < 0.62 && (center.z - face).abs() <= 8.0)
            || (crest >= spacing * 1.5
                && center.y <= crest - spacing * 0.45
                && (center.z - face).abs() <= spacing * 3.5))
}

fn tile_mesh(surface: &ExtractedSurface, indices: Vec<u32>) -> Mesh {
    ExtractedSurface {
        positions: surface.positions.clone(),
        normals: surface.normals.clone(),
        indices,
    }
    .into_mesh()
}

fn tile_ground_mesh(
    surface: &ExtractedSurface,
    indices: Vec<u32>,
    recipe: RiverBluffRecipe,
    terrain: &SceneTerrain,
) -> Mesh {
    let uvs = surface
        .positions
        .iter()
        .map(|position| {
            let world = recipe.local_to_world(Vec3::from_array(*position));
            [
                (world.x / terrain.width() + 0.5).clamp(0.0, 1.0),
                (world.z / terrain.depth() + 0.5).clamp(0.0, 1.0),
            ]
        })
        .collect::<Vec<_>>();
    let mut mesh = tile_mesh(surface, indices);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh
}

pub(super) fn on_terrain_patch_added(event: On<Add, TerrainPatchRecipe>, mut commands: Commands) {
    commands
        .entity(event.entity)
        .insert(PendingImplicitTerrainTile);
}

pub(super) fn present_pending_implicit_tiles(
    pending: Query<(Entity, &TerrainPatchRecipe), With<PendingImplicitTerrainTile>>,
    terrains: Query<(Entity, &SceneTerrain)>,
    terrain_materials: Query<
        (
            &ScenePresentationOf,
            &MeshMaterial3d<TacticalTerrainMaterial>,
        ),
        With<TerrainMaterialPresentation>,
    >,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut tactical_terrain_materials: ResMut<Assets<TacticalTerrainMaterial>>,
) -> Result {
    let Ok((terrain_entity, terrain)) = terrains.single() else {
        return Ok(());
    };
    let Some((_, ground_material)) = terrain_materials
        .iter()
        .find(|(source, _)| source.0 == terrain_entity)
    else {
        return Ok(());
    };
    let Some(tile_ground_material) = tactical_terrain_materials
        .get(&ground_material.0)
        .map(super::terrain::implicit_tile_ground_material)
    else {
        return Ok(());
    };
    let tile_ground_material = tactical_terrain_materials.add(tile_ground_material);
    for (entity, patch) in &pending {
        let TerrainPatchRecipe::RiverBluff(recipe) = *patch;
        let report = recipe
            .representability()
            .expect("validated terrain patch has a bounded sampling report");
        if report.representation != TerrainRepresentation::ImplicitSurface {
            commands
                .entity(entity)
                .remove::<PendingImplicitTerrainTile>();
            continue;
        }
        let dimensions = recipe.dimensions_metres();
        // One authored sample outside the finite mass is sufficient to close the scalar field. Using
        // a fixed two-metre pad with unchanged sample counts silently coarsened the nominal spacing
        // and produced visible triangular bedding steps.
        let padding = f32::from(recipe.sample_spacing_cm) / 100.0;
        let convergence_start =
            recipe.rear_terrace_convergence_start_local_z() + terrain.grid_scale() * 1.1;
        let [minimum_x, maximum_x, minimum_z, maximum_z] = recipe.implicit_tile_bounds_local();
        let grid = SurfaceNetsGrid {
            sample_counts: report.sample_counts.map(usize::from),
            minimum: Vec3::new(
                minimum_x - padding * 2.0,
                terrain.minimum_height()
                    - recipe.center_metres().y
                    - recipe.dimensions_metres().y * 0.28,
                minimum_z - padding * 2.0,
            ),
            maximum: Vec3::new(
                maximum_x + padding * 2.0,
                (terrain.maximum_height() - recipe.center_metres().y).max(dimensions.y)
                    + padding * 2.0,
                maximum_z + padding * 2.0,
            ),
        };
        let mut sandstone = extract_surface_nets(grid, |local| {
            implicit_tile_field(recipe, terrain, convergence_start, local)
        })
        .expect("validated river bluff tile produces a finite scalar field");
        finish_implicit_tile_boundary(&mut sandstone, recipe, terrain);
        let triangle_count = sandstone.indices.len() / 3;
        let (ground_indices, rock_indices) =
            partition_implicit_tile_triangles(&sandstone, recipe, terrain);
        assert_eq!(
            ground_indices.len() + rock_indices.len(),
            sandstone.indices.len(),
            "every implicit tile triangle must have exactly one material owner"
        );
        let colors = sandstone
            .positions
            .iter()
            .map(|position| {
                let position = Vec3::from_array(*position);
                let relative_height = position[1] / dimensions.y;
                // Resistant sandstone beds project between recessed clay/silt
                // interbeds. Both expressions ease through the broad failure
                // plane and fade into the buried returned ends.
                let bed_warp = (position[0] * 0.08).sin() * 0.014
                    + (position[0] * 0.17 + position[2] * 0.05).sin() * 0.005;
                let collapse_x = f32::from(recipe.collapse_offset_cm) / 100.0;
                let collapse_radius = f32::from(recipe.collapse_radius_cm) / 100.0;
                let release_blend = unit_smoothstep(
                    ((position.x - (collapse_x - collapse_radius * 0.45))
                        / (collapse_radius * 0.90))
                        .clamp(0.0, 1.0),
                );
                let release_offset = -0.006 + release_blend * 0.016;
                let end_fade = 1.0
                    - unit_smoothstep(
                        (((position[0] / (dimensions.x * 0.5)).abs() - 0.46) / 0.26)
                            .clamp(0.0, 1.0),
                    );
                let scar_weight = recipe.failure_scar_weight(position);
                let scar_color_fade = 1.0 - scar_weight * 0.82;
                let weak_weight =
                    course_color_weight(relative_height, 0.46, bed_warp + release_offset)
                        * end_fade
                        * scar_color_fade.clamp(0.0, 1.0);
                let resistant_weight =
                    course_color_weight(relative_height, 0.78, bed_warp + release_offset)
                        * end_fade
                        * scar_color_fade.clamp(0.0, 1.0);
                let undercut_recess = recipe.undercut_weight_local(position);
                let undercut_lateral =
                    recipe.undercut_weight_local(Vec3::new(position.x, 0.45, 0.0));
                let resistant_toe_lip =
                    undercut_lateral > 0.12 && (position.y - 1.42).abs() <= 0.34;
                let undercut_shadow =
                    undercut_recess * (-((position.y - 0.72) / 0.48).powi(2)).exp();
                let base_color = [0.78_f32, 0.45, 0.25, 1.0];
                let weak_color = [0.42_f32, 0.21, 0.14, 1.0];
                let resistant_color = [0.94_f32, 0.62, 0.38, 1.0];
                let toe_weight = if resistant_toe_lip { 0.75 } else { 0.0 };
                let resistant_mix = resistant_weight.max(toe_weight);
                let intact_color: [f32; 4] = core::array::from_fn(|channel| {
                    let weak_blend = base_color[channel] * (1.0 - weak_weight)
                        + weak_color[channel] * weak_weight;
                    weak_blend * (1.0 - resistant_mix) + resistant_color[channel] * resistant_mix
                });
                let variation = (position.x * 0.31 + position.y * 0.19).sin() * 0.018
                    + (position.x * 0.13 - position.y * 0.27).sin() * 0.010;
                let fresh_color = [
                    0.85 + variation,
                    0.53 + variation * 0.7,
                    0.32 + variation * 0.4,
                    1.0,
                ];
                // Fresh material appears only well inside the physically recessed
                // wedge; the broad transition/rim keeps the ordinary sandstone
                // palette so the failure cannot read as a painted panel.
                let fresh_blend =
                    unit_smoothstep(((scar_weight - 0.72) / 0.24).clamp(0.0, 1.0)) * 0.60;
                let surface_color = core::array::from_fn(|channel| {
                    intact_color[channel] * (1.0 - fresh_blend) + fresh_color[channel] * fresh_blend
                });
                let rock_color = if undercut_shadow > 0.10 {
                    UNDERCUT_WARM_SHADOW
                } else {
                    surface_color
                };
                let local_crest = recipe.local_crest_height(position.x);
                let crest_contact = (1.0
                    - unit_smoothstep(((local_crest - position.y) / 0.9).clamp(0.0, 1.0)))
                    * 0.22;
                // Only the immediate brink receives a restrained ground-color
                // handoff. Upward terrain is rendered by the terrain material;
                // painting returned shoulders and toes olive produced the broad
                // dark wedges seen in grazing evidence.
                let contact = crest_contact;
                let ground_contact = [0.26_f32, 0.30, 0.15, 1.0];
                core::array::from_fn(|channel| {
                    rock_color[channel] * (1.0 - contact) + ground_contact[channel] * contact
                })
            })
            .collect::<Vec<[f32; 4]>>();
        let root = commands
            .spawn((
                Name::new("Implicit Buntsandstein river bluff visual"),
                ImplicitTerrainPatchVisual,
                Transform::from_translation(recipe.center_metres())
                    .with_rotation(Quat::from_rotation_y(recipe.yaw_radians())),
                Visibility::default(),
            ))
            .id();
        let sandstone_material = materials.add(StandardMaterial {
            base_color: Color::WHITE,
            // A small warm ambient lift keeps the shallow undercut readable
            // without flattening the production directional shadow.
            emissive: LinearRgba::new(0.025, 0.012, 0.006, 1.0),
            perceptual_roughness: 0.9,
            metallic: 0.0,
            ..default()
        });
        let ground_mesh = tile_ground_mesh(&sandstone, ground_indices, recipe, terrain);
        let mut sandstone_mesh = tile_mesh(&sandstone, rock_indices);
        sandstone_mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        commands.entity(root).with_child((
            Name::new("Authoritative terrain portion of implicit tile"),
            ImplicitTerrainPatchSurface,
            ImplicitTerrainTileGround {
                source_terrain: terrain_entity,
            },
            Mesh3d(meshes.add(ground_mesh)),
            MeshMaterial3d(tile_ground_material.clone()),
            Transform::default(),
        ));
        commands.entity(root).with_child((
            Name::new("Molded sandstone cliff mass"),
            ImplicitTerrainPatchSurface,
            Mesh3d(meshes.add(sandstone_mesh)),
            MeshMaterial3d(sandstone_material.clone()),
            Transform::default(),
        ));
        assert!(
            triangle_count <= MAX_TERRAIN_PATCH_TRIANGLES,
            "implicit terrain patch exceeded its render triangle budget"
        );
        commands
            .entity(entity)
            .remove::<PendingImplicitTerrainTile>();
    }
    Ok(())
}

pub(super) fn synchronize_implicit_tile_ground_materials(
    tiles: Query<(
        &ImplicitTerrainTileGround,
        &MeshMaterial3d<TacticalTerrainMaterial>,
    )>,
    terrain_materials: Query<
        (
            &ScenePresentationOf,
            &MeshMaterial3d<TacticalTerrainMaterial>,
        ),
        With<TerrainMaterialPresentation>,
    >,
    mut materials: ResMut<Assets<TacticalTerrainMaterial>>,
) {
    for (tile, tile_handle) in &tiles {
        let Some((_, source_handle)) = terrain_materials
            .iter()
            .find(|(source, _)| source.0 == tile.source_terrain)
        else {
            continue;
        };
        let Some(source) = materials.get(&source_handle.0).cloned() else {
            continue;
        };
        let Some(mut tile_material) = materials.get_mut(&tile_handle.0) else {
            continue;
        };
        *tile_material = super::terrain::implicit_tile_ground_material(&source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undercut_shadow_stays_warm_and_never_reads_as_black() {
        assert!(UNDERCUT_WARM_SHADOW[0] >= 0.55);
        assert!(UNDERCUT_WARM_SHADOW[1] >= 0.30);
        assert!(UNDERCUT_WARM_SHADOW[2] >= 0.18);
    }

    fn test_recipe() -> RiverBluffRecipe {
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

    fn test_terrain(recipe: RiverBluffRecipe) -> SceneTerrain {
        let width = 41_usize;
        let spacing = 2.0_f32;
        let half = (width - 1) as f32 * spacing * 0.5;
        let heights = (0..width)
            .flat_map(|z| {
                (0..width).map(move |x| {
                    let world_x = x as f32 * spacing - half;
                    let world_z = z as f32 * spacing - half;
                    let terrace = unit_smoothstep(((world_z - 11.0) / 7.0).clamp(0.0, 1.0))
                        * recipe.local_crest_height(world_x);
                    terrace + recipe.debris_fan_height_local(world_x, world_z)
                })
            })
            .collect();
        SceneTerrain::from_heightmap(width, width, spacing, heights).unwrap()
    }

    fn extract_tile(recipe: RiverBluffRecipe, terrain: &SceneTerrain) -> ExtractedSurface {
        let report = recipe.representability().unwrap();
        let padding = f32::from(recipe.sample_spacing_cm) / 100.0;
        let convergence_start =
            recipe.rear_terrace_convergence_start_local_z() + terrain.grid_scale() * 1.1;
        let [minimum_x, maximum_x, minimum_z, maximum_z] = recipe.implicit_tile_bounds_local();
        let grid = SurfaceNetsGrid {
            sample_counts: report.sample_counts.map(usize::from),
            minimum: Vec3::new(
                minimum_x - padding * 2.0,
                terrain.minimum_height()
                    - recipe.center_metres().y
                    - recipe.dimensions_metres().y * 0.28,
                minimum_z - padding * 2.0,
            ),
            maximum: Vec3::new(
                maximum_x + padding * 2.0,
                (terrain.maximum_height() - recipe.center_metres().y)
                    .max(recipe.dimensions_metres().y)
                    + padding * 2.0,
                maximum_z + padding * 2.0,
            ),
        };
        let mut surface = extract_surface_nets(grid, |local| {
            implicit_tile_field(recipe, terrain, convergence_start, local)
        })
        .unwrap();
        finish_implicit_tile_boundary(&mut surface, recipe, terrain);
        surface
    }

    #[test]
    fn implicit_tile_boundary_matches_authoritative_heightfield_exactly() {
        let recipe = test_recipe();
        let terrain = test_terrain(recipe);
        let tile = extract_tile(recipe, &terrain);
        let [minimum_x, maximum_x, minimum_z, maximum_z] = recipe.implicit_tile_bounds_local();
        let mut edge_counts = std::collections::HashMap::<(u32, u32), u8>::new();
        for triangle in tile.indices.chunks_exact(3) {
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                let edge = if a < b { (a, b) } else { (b, a) };
                *edge_counts.entry(edge).or_default() += 1;
            }
        }
        let boundary_edges = edge_counts
            .into_iter()
            .filter_map(|(edge, count)| (count == 1).then_some(edge))
            .collect::<Vec<_>>();
        assert!(boundary_edges.len() >= 64);
        for (a, b) in boundary_edges {
            let positions = [a, b].map(|index| Vec3::from_array(tile.positions[index as usize]));
            for local in positions {
                let edge_distance = (local.x - minimum_x)
                    .abs()
                    .min((local.x - maximum_x).abs())
                    .min((local.z - minimum_z).abs())
                    .min((local.z - maximum_z).abs());
                assert!(
                    edge_distance <= 1.0e-4,
                    "implicit tile retained a non-perimeter open edge at {local:?}"
                );
                let world = recipe.local_to_world(local);
                let expected = terrain.height_at(world.xz()).unwrap();
                assert!(
                    (world.y - expected).abs() <= 1.0e-4,
                    "tile boundary did not match authoritative heightfield: local={local:?}, world_y={}, expected={expected}",
                    world.y,
                );
                let normal = Vec3::from_array(
                    tile.normals[tile
                        .positions
                        .iter()
                        .position(|p| *p == local.to_array())
                        .unwrap()],
                );
                assert!(normal.is_finite() && (0.75..=1.25).contains(&normal.length()));
            }
            assert!(
                positions[0].distance(positions[1]) <= 1.6,
                "distant tile boundary developed a pathological edge"
            );
        }
    }

    #[test]
    fn blended_landform_field_is_exactly_terrain_at_every_tile_boundary() {
        let recipe = test_recipe();
        let terrain = test_terrain(recipe);
        let convergence_start =
            recipe.rear_terrace_convergence_start_local_z() + terrain.grid_scale() * 1.1;
        let [minimum_x, maximum_x, minimum_z, maximum_z] = recipe.implicit_tile_bounds_local();
        for local in [
            Vec3::new(minimum_x, 0.0, 0.0),
            Vec3::new(maximum_x, 4.0, 0.0),
            Vec3::new(0.0, -1.0, minimum_z),
            Vec3::new(0.0, 8.0, maximum_z),
            Vec3::new(minimum_x, 3.0, minimum_z),
            Vec3::new(maximum_x, 3.0, maximum_z),
        ] {
            let world = recipe.local_to_world(local);
            let terrain_field = world.y - terrain.height_at(world.xz()).unwrap();
            assert_eq!(implicit_tile_landform_influence(recipe, local), 0.0);
            assert_eq!(
                implicit_tile_field(recipe, &terrain, convergence_start, local),
                terrain_field,
                "tile boundary drifted from the authoritative terrain field at {local:?}"
            );
        }

        let active = Vec3::new(
            f32::from(recipe.collapse_offset_cm) / 100.0,
            0.7,
            recipe.face_surface_local_z(Vec3::new(
                f32::from(recipe.collapse_offset_cm) / 100.0,
                0.7,
                0.0,
            )),
        );
        assert_eq!(implicit_tile_landform_influence(recipe, active), 1.0);
    }

    #[test]
    fn blended_landform_extracts_one_surface_and_localizes_down_facing_area() {
        let recipe = test_recipe();
        let terrain = test_terrain(recipe);
        let tile = extract_tile(recipe, &terrain);
        let mut adjacency = vec![Vec::<usize>::new(); tile.positions.len()];
        for triangle in tile.indices.chunks_exact(3) {
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                adjacency[a as usize].push(b as usize);
                adjacency[b as usize].push(a as usize);
            }
        }
        let first = tile.indices[0] as usize;
        let mut visited = vec![false; tile.positions.len()];
        let mut pending = vec![first];
        while let Some(index) = pending.pop() {
            if std::mem::replace(&mut visited[index], true) {
                continue;
            }
            pending.extend(adjacency[index].iter().copied());
        }
        assert!(
            tile.indices.iter().all(|index| visited[*index as usize]),
            "implicit landform extracted more than one connected surface component"
        );

        let overhang = tile
            .indices
            .iter()
            .map(|index| Vec3::from_array(tile.positions[*index as usize]))
            .filter(|position| {
                recipe.undercut_weight_local(*position) > 0.08
                    && (position.z - recipe.face_surface_local_z(*position)).abs() <= 1.5
            })
            .collect::<Vec<_>>();
        assert!(
            overhang.len() >= 12,
            "localized authored undercut lost its multi-valued surface"
        );
        let bounds = overhang.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(minimum, maximum), point| (minimum.min(*point), maximum.max(*point)),
        );
        assert!(
            bounds.1.x - bounds.0.x <= 5.0 && bounds.1.y <= 1.8 && bounds.1.z - bounds.0.z <= 2.0,
            "overhang area escaped the authored undercut: {bounds:?}"
        );
    }

    #[test]
    fn terrain_solid_has_no_obsolete_finite_side_or_back_planes() {
        let recipe = test_recipe();
        let size = recipe.dimensions_metres();
        let central_back = recipe.signed_distance(recipe.local_to_world(Vec3::new(
            0.0,
            size.y * 0.5,
            size.z + 20.0,
        )));
        assert!(
            central_back < 0.0,
            "terrace solid must continue rearward rather than closing at a finite back plane"
        );
        let outer_below_ground = recipe.signed_distance(recipe.local_to_world(Vec3::new(
            size.x * 0.5 + 4.0,
            -0.5,
            size.z + 4.0,
        )));
        let outer_above_ground = recipe.signed_distance(recipe.local_to_world(Vec3::new(
            size.x * 0.5 + 4.0,
            0.5,
            size.z + 4.0,
        )));
        assert!(outer_below_ground < 0.0 && outer_above_ground > 0.0);

        let terrain = test_terrain(recipe);
        let tile = extract_tile(recipe, &terrain);
        let sample = f32::from(recipe.sample_spacing_cm) / 100.0;
        for triangle in tile.indices.chunks_exact(3) {
            let positions = [triangle[0], triangle[1], triangle[2]]
                .map(|index| Vec3::from_array(tile.positions[index as usize]));
            let vertical_span = positions.iter().map(|position| position.y).fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(minimum, maximum), y| (minimum.min(y), maximum.max(y)),
            );
            let follows_old_side = positions
                .iter()
                .all(|position| (position.x.abs() - size.x * 0.5).abs() <= sample * 0.55);
            let follows_old_back = positions
                .iter()
                .all(|position| (position.z - size.z).abs() <= sample * 0.55);
            assert!(
                !(follows_old_side || follows_old_back)
                    || vertical_span.1 - vertical_span.0 <= sample * 1.5,
                "continuous tile exposed an obsolete finite recipe closure: {positions:?}"
            );
        }
    }

    #[test]
    fn terrain_relative_terrace_keeps_the_brink_and_converges_before_the_tile_back() {
        let recipe = test_recipe();
        let terrain = test_terrain(recipe);
        let spacing = f32::from(recipe.sample_spacing_cm) / 100.0;
        let convergence_start =
            recipe.rear_terrace_convergence_start_local_z() + terrain.grid_scale() * 1.1;
        let [_, _, _, tile_back] = recipe.implicit_tile_bounds_local();
        for local_x in [-10.0, -6.0, 0.0, 6.0, 10.0] {
            let brink_z = recipe.crest_brink_local_z(local_x);
            let brink_height = implicit_tile_terrace_height_local(
                recipe,
                &terrain,
                convergence_start,
                local_x,
                brink_z,
            )
            .unwrap();
            assert!(
                (brink_height - recipe.local_crest_height(local_x)).abs() <= 1.0e-5,
                "terrain-relative terrace changed the authored brink at x={local_x}: {brink_height}"
            );

            let mut previous_error = f32::INFINITY;
            for offset in [0.0, 2.0, 4.0, 6.0, 8.0, 10.0, 11.5] {
                let local_z = convergence_start + offset;
                let world = recipe.local_to_world(Vec3::new(local_x, 0.0, local_z));
                let terrain_height =
                    terrain.height_at(world.xz()).unwrap() - recipe.center_metres().y;
                let terrace_height = implicit_tile_terrace_height_local(
                    recipe,
                    &terrain,
                    convergence_start,
                    local_x,
                    local_z,
                )
                .unwrap();
                let error = (terrace_height - terrain_height).abs();
                assert!(
                    error <= previous_error + 1.0e-4,
                    "rear terrace failed to converge monotonically at x={local_x}, z={local_z}: {error} > {previous_error}"
                );
                previous_error = error;
            }
            assert!(previous_error <= 1.0e-5);
            assert!(
                convergence_start + 11.5 <= tile_back - spacing,
                "terrace did not converge before the distant tile perimeter at x={local_x}"
            );
        }
    }

    #[test]
    fn implicit_tile_is_deterministic_bounded_and_preserves_undercut() {
        let recipe = test_recipe();
        let terrain = test_terrain(recipe);
        let first = extract_tile(recipe, &terrain);
        assert_eq!(first, extract_tile(recipe, &terrain));
        assert!(!first.positions.is_empty() && !first.indices.is_empty());
        assert!(
            first
                .positions
                .iter()
                .flatten()
                .all(|value| value.is_finite())
                && first
                    .normals
                    .iter()
                    .flatten()
                    .all(|value| value.is_finite())
        );
        assert!(
            recipe.representability().unwrap().sample_count as usize <= MAX_TERRAIN_PATCH_SAMPLES
        );
        assert!(first.indices.len() / 3 <= MAX_TERRAIN_PATCH_TRIANGLES);
        for triangle in first.indices.chunks_exact(3) {
            let positions = [triangle[0], triangle[1], triangle[2]]
                .map(|index| Vec3::from_array(first.positions[index as usize]));
            for edge in [[0, 1], [1, 2], [2, 0]] {
                assert!(
                    positions[edge[0]].distance(positions[edge[1]]) <= 1.8,
                    "implicit tile contains a pathological triangle span"
                );
            }
        }
        let collapse_x = f32::from(recipe.collapse_offset_cm) / 100.0;
        let undercut = first
            .indices
            .iter()
            .map(|index| Vec3::from_array(first.positions[*index as usize]))
            .filter(|position| {
                (position.x - collapse_x).abs() <= 2.3 && (0.15..=1.30).contains(&position.y)
            })
            .collect::<Vec<_>>();
        assert!(undercut.len() >= 12);
        let z_span = undercut.iter().map(|position| position.z).fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(minimum, maximum), z| (minimum.min(z), maximum.max(z)),
        );
        assert!(
            z_span.1 - z_span.0 >= 0.50,
            "continuous terrain tile lost the localized shallow undercut"
        );
    }

    #[test]
    fn implicit_tile_material_partition_is_complete_disjoint_and_reuses_vertices() {
        let recipe = test_recipe();
        let terrain = test_terrain(recipe);
        let tile = extract_tile(recipe, &terrain);
        let (ground, rock) = partition_implicit_tile_triangles(&tile, recipe, &terrain);
        assert!(!ground.is_empty() && !rock.is_empty());
        assert_eq!(ground.len() + rock.len(), tile.indices.len());
        let ground_triangles = ground
            .chunks_exact(3)
            .map(|triangle| [triangle[0], triangle[1], triangle[2]])
            .collect::<std::collections::HashSet<_>>();
        let rock_triangles = rock
            .chunks_exact(3)
            .map(|triangle| [triangle[0], triangle[1], triangle[2]])
            .collect::<std::collections::HashSet<_>>();
        assert!(ground_triangles.is_disjoint(&rock_triangles));
        assert_eq!(
            ground_triangles.len() + rock_triangles.len(),
            tile.indices.len() / 3
        );
        for mesh in [tile_mesh(&tile, ground.clone()), tile_mesh(&tile, rock)] {
            assert_eq!(
                mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().len(),
                tile.positions.len()
            );
        }
        let ground_mesh = tile_ground_mesh(&tile, ground, recipe, &terrain);
        assert_eq!(
            ground_mesh.attribute(Mesh::ATTRIBUTE_UV_0).unwrap().len(),
            tile.positions.len()
        );
        let center = |triangle: &[u32; 3]| {
            triangle
                .iter()
                .map(|index| Vec3::from_array(tile.positions[*index as usize]))
                .sum::<Vec3>()
                / 3.0
        };
        let [minimum_x, maximum_x, minimum_z, maximum_z] = recipe.implicit_tile_bounds_local();
        let perimeter_ground = ground_triangles
            .iter()
            .map(center)
            .filter(|point| {
                (point.x - minimum_x).abs() < 0.8
                    || (point.x - maximum_x).abs() < 0.8
                    || (point.z - minimum_z).abs() < 0.8
                    || (point.z - maximum_z).abs() < 0.8
            })
            .count();
        assert!(perimeter_ground >= 32);
        assert!(rock_triangles.iter().map(center).all(|point| {
            (point.x - minimum_x).abs() >= 0.8
                && (point.x - maximum_x).abs() >= 0.8
                && (point.z - minimum_z).abs() >= 0.8
                && (point.z - maximum_z).abs() >= 0.8
        }));
        let upper_ground = ground_triangles
            .iter()
            .map(center)
            .filter(|point| {
                point.x.abs() <= 8.0
                    && point.y >= recipe.local_crest_height(point.x) - 0.20
                    && point.z >= recipe.maximum_face_local_z(point.x)
            })
            .count();
        assert!(
            upper_ground >= 64,
            "broad upper terrace was not ground-owned"
        );
        let toe_rock = rock_triangles
            .iter()
            .map(center)
            .filter(|point| point.y <= 1.5 && recipe.undercut_weight_local(*point) > 0.05)
            .count();
        let scarp_rock = rock_triangles
            .iter()
            .map(center)
            .filter(|point| {
                point.y >= 2.0
                    && point.y <= recipe.local_crest_height(point.x) - 0.5
                    && (point.z - recipe.face_surface_local_z(*point)).abs() <= 0.9
            })
            .count();
        assert!(
            toe_rock >= 8,
            "toe and undercut lost rock material ownership"
        );
        assert!(
            scarp_rock >= 64,
            "authored scarp lost rock material ownership"
        );
        let rock_bounds = rock_triangles.iter().map(center).fold(
            (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
            |(minimum, maximum), point| (minimum.min(point.xz()), maximum.max(point.xz())),
        );
        let tile_extent = Vec2::new(maximum_x - minimum_x, maximum_z - minimum_z);
        let rock_extent = rock_bounds.1 - rock_bounds.0;
        assert!(
            rock_extent.x < tile_extent.x * 0.80 && rock_extent.y < tile_extent.y * 0.65,
            "rock material must remain a scarp semantic, not a tile-shaped rectangle: ground_triangles={}, rock_triangles={}, rock_bounds={rock_bounds:?}, tile_extent={tile_extent:?}",
            ground_triangles.len(),
            rock_triangles.len(),
        );
    }

    #[test]
    fn fixture_tile_ground_uv_semantics_and_slopes_are_bounded() {
        let input: TacticalSceneInput = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/tactical-scenes/river-bluff-cliff.json"
        )))
        .unwrap();
        let generated = input.generate().unwrap();
        let TerrainPatchRecipe::RiverBluff(recipe) = generated.terrain_patches[0];
        let tile = extract_tile(recipe, &generated.terrain);
        let (ground, _) = partition_implicit_tile_triangles(&tile, recipe, &generated.terrain);
        let mut referenced = std::collections::BTreeSet::new();
        referenced.extend(ground.iter().map(|index| *index as usize));
        let (uv_min, uv_max) = referenced.iter().fold(
            (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
            |(minimum, maximum), index| {
                let world = recipe.local_to_world(Vec3::from_array(tile.positions[*index]));
                let uv = Vec2::new(
                    world.x / generated.terrain.width() + 0.5,
                    world.z / generated.terrain.depth() + 0.5,
                );
                (minimum.min(uv), maximum.max(uv))
            },
        );
        let [minimum_x, maximum_x, minimum_z, maximum_z] = recipe.implicit_tile_bounds_local();
        let convergence_start =
            recipe.rear_terrace_convergence_start_local_z() + generated.terrain.grid_scale() * 1.1;
        let mut steep_ground_before_rear = 0_usize;
        let (normal_y_min, normal_y_max) = ground
            .chunks_exact(3)
            .filter_map(|triangle| {
                let [a, b, c] = [triangle[0], triangle[1], triangle[2]]
                    .map(|index| Vec3::from_array(tile.positions[index as usize]));
                let center = (a + b + c) / 3.0;
                let normal_y = (b - a).cross(c - a).normalize_or_zero().y.abs();
                if normal_y < 0.62
                    && center.z <= convergence_start
                    && implicit_tile_landform_influence(recipe, center) > 0.01
                {
                    steep_ground_before_rear += 1;
                }
                ((center.x - minimum_x).abs() >= 1.4
                    && (center.x - maximum_x).abs() >= 1.4
                    && (center.z - minimum_z).abs() >= 1.4
                    && (center.z - maximum_z).abs() >= 1.4
                    && implicit_tile_landform_influence(recipe, center) > 0.01)
                    .then_some(normal_y)
            })
            .fold(
                (f32::INFINITY, f32::NEG_INFINITY),
                |(minimum, maximum), y| (minimum.min(y), maximum.max(y)),
            );
        let points = [
            recipe.local_to_world(Vec3::new(0.0, 0.0, recipe.maximum_face_local_z(0.0) + 4.0)),
            recipe.local_to_world(Vec3::new(0.0, 0.0, recipe.minimum_face_local_z(0.0) - 4.0)),
            recipe.local_to_world(Vec3::new(minimum_x + 1.0, 0.0, minimum_z + 1.0)),
        ];
        let surfaces = points.map(|point| generated.ground.ground_at(point.xz()).unwrap());
        eprintln!(
            "implicit tile diagnostic: uv={uv_min:?}..{uv_max:?}, blended ground normal.y={normal_y_min:.4}..{normal_y_max:.4}, samples={surfaces:?}"
        );
        assert!(uv_min.cmpge(Vec2::ZERO).all() && uv_max.cmple(Vec2::ONE).all());
        assert!(
            normal_y_min >= 0.50 && normal_y_max <= 1.0001,
            "tile-ground material developed a wall rather than the bounded rear terrain slope: normal.y={normal_y_min:.4}..{normal_y_max:.4}"
        );
        assert_eq!(
            steep_ground_before_rear, 0,
            "tile-ground material owned steep scarp/toe triangles before rear convergence"
        );
    }

    #[test]
    fn pending_tile_waits_for_terrain_regardless_of_insertion_order() {
        let recipe = test_recipe();
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<StandardMaterial>>()
            .init_resource::<Assets<TacticalTerrainMaterial>>()
            .add_systems(Update, present_pending_implicit_tiles);
        let patch = app
            .world_mut()
            .spawn((
                TerrainPatchRecipe::RiverBluff(recipe),
                PendingImplicitTerrainTile,
            ))
            .id();
        app.update();
        assert!(
            app.world()
                .entity(patch)
                .contains::<PendingImplicitTerrainTile>()
        );
        let terrain_component = test_terrain(recipe);
        let mut images = Assets::<Image>::default();
        let environment = legacy_scene_environment(&SceneId("implicit-tile-order".into()));
        let material =
            super::terrain::terrain_material(&terrain_component, &environment, None, &mut images);
        let terrain = app.world_mut().spawn(terrain_component).id();
        app.update();
        assert!(
            app.world()
                .entity(patch)
                .contains::<PendingImplicitTerrainTile>()
        );
        let material = app
            .world_mut()
            .resource_mut::<Assets<TacticalTerrainMaterial>>()
            .add(material);
        app.world_mut().spawn((
            ScenePresentationOf(terrain),
            TerrainMaterialPresentation,
            MeshMaterial3d::<TacticalTerrainMaterial>(material),
        ));
        app.update();
        assert!(
            !app.world()
                .entity(patch)
                .contains::<PendingImplicitTerrainTile>()
        );
        assert_eq!(
            app.world_mut()
                .query::<&ImplicitTerrainPatchVisual>()
                .iter(app.world())
                .count(),
            1
        );
    }

    #[test]
    fn tile_ground_material_tracks_the_finalized_source_material() {
        let recipe = test_recipe();
        let terrain = test_terrain(recipe);
        let mut images = Assets::<Image>::default();
        let first_environment = legacy_scene_environment(&SceneId("tile-material-first".into()));
        let first =
            super::terrain::terrain_material(&terrain, &first_environment, None, &mut images);
        let initial_tile = super::terrain::implicit_tile_ground_material(&first);

        let mut app = App::new();
        app.init_resource::<Assets<TacticalTerrainMaterial>>()
            .add_systems(Update, synchronize_implicit_tile_ground_materials);
        let source_terrain = app.world_mut().spawn_empty().id();
        let source_handle = app
            .world_mut()
            .resource_mut::<Assets<TacticalTerrainMaterial>>()
            .add(first);
        app.world_mut().spawn((
            ScenePresentationOf(source_terrain),
            TerrainMaterialPresentation,
            MeshMaterial3d(source_handle.clone()),
        ));
        let tile_handle = app
            .world_mut()
            .resource_mut::<Assets<TacticalTerrainMaterial>>()
            .add(initial_tile);
        app.world_mut().spawn((
            ImplicitTerrainTileGround { source_terrain },
            MeshMaterial3d(tile_handle.clone()),
        ));

        let mut finalized_environment = first_environment;
        finalized_environment.canopy_bps = 8_900;
        finalized_environment.hilly_bps = 7_100;
        finalized_environment.weather.ground_moisture_bps = 8_200;
        let finalized =
            super::terrain::terrain_material(&terrain, &finalized_environment, None, &mut images);
        let expected = super::terrain::implicit_tile_ground_material(&finalized);
        *app.world_mut()
            .resource_mut::<Assets<TacticalTerrainMaterial>>()
            .get_mut(&source_handle)
            .unwrap() = finalized;
        app.update();
        let synchronized = app
            .world()
            .resource::<Assets<TacticalTerrainMaterial>>()
            .get(&tile_handle)
            .unwrap();
        assert_eq!(
            format!("{synchronized:?}"),
            format!("{expected:?}"),
            "tile material must mirror every finalized source field except the intentional no-cutout override"
        );
    }
}
