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
    let [_, _, minimum_z, maximum_z] = recipe.implicit_tile_bounds_local();
    let spacing = f32::from(recipe.sample_spacing_cm) / 100.0;
    // Only the active failed sector needs a multi-valued rock surface. Stable flanks and both
    // returned shoulders are exactly the authoritative terrain field; the shared C1 support
    // prevents the old full-width sandstone frontage from surviving presentation-only code.
    let lateral = recipe.rock_support_weight_local(local.x);

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
    _convergence_start: f32,
    local: Vec3,
) -> f32 {
    let world = recipe.local_to_world(local);
    let Some(height) = terrain.height_at(world.xz()) else {
        return recipe.signed_distance(world);
    };
    let terrain_surface = world.y - height;
    if implicit_tile_landform_influence(recipe, local) <= 0.0 {
        return terrain_surface;
    }
    // The heightfield supplies the complete bluff and both stable flanks. Union only the compact
    // toe ledge needed for the localized overhang; never interpolate toward a full-height wall.
    // The rock's rear closure is buried in the rising heightfield and its padded tile boundary is
    // exactly the ordinary terrain scalar.
    terrain_surface.min(recipe.implicit_rock_solid_local(local))
}

fn implicit_folded_terrain_field(
    recipe: RiverBluffRecipe,
    terrain: &SceneTerrain,
    local: Vec3,
) -> f32 {
    let world = recipe.local_to_world(local);
    let Some(base_height) = terrain.height_at(world.xz()) else {
        return world.y;
    };
    let forward_offset = recipe.implicit_fold_forward_offset_local(local);
    if forward_offset <= 0.0 {
        return world.y - base_height;
    }
    // Displayed z is the source heightfield coordinate shifted forward. Sampling the source at
    // z+offset makes the zero set fold without adding a separate closed object.
    let source_world = recipe.local_to_world(local + Vec3::Z * forward_offset);
    terrain
        .height_at(source_world.xz())
        .map_or(world.y - base_height, |height| world.y - height)
}

fn heightfield_scarp_local_z(
    recipe: RiverBluffRecipe,
    terrain: &SceneTerrain,
    local_x: f32,
    local_y: f32,
) -> Option<f32> {
    const SEARCH_STEPS: usize = 48;
    const BISECTION_STEPS: usize = 12;
    let [_, _, minimum_z, maximum_z] = recipe.implicit_tile_bounds_local();
    let target_height = recipe.center_metres().y + local_y;
    let height_at = |local_z: f32| {
        let world = recipe.local_to_world(Vec3::new(local_x, 0.0, local_z));
        terrain.height_at(world.xz()).unwrap_or(target_height)
    };
    let mut previous_z = minimum_z;
    let mut previous_delta = height_at(previous_z) - target_height;
    for step in 1..=SEARCH_STEPS {
        let current_z = minimum_z + (maximum_z - minimum_z) * step as f32 / SEARCH_STEPS as f32;
        let current_delta = height_at(current_z) - target_height;
        if previous_delta <= 0.0 && current_delta >= 0.0 {
            let mut low = previous_z;
            let mut high = current_z;
            for _ in 0..BISECTION_STEPS {
                let middle = (low + high) * 0.5;
                if height_at(middle) < target_height {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            return Some((low + high) * 0.5);
        }
        previous_z = current_z;
        previous_delta = current_delta;
    }
    None
}

fn implicit_scarp_sheet_top(recipe: RiverBluffRecipe) -> f32 {
    const SAMPLES: usize = 32;
    let [minimum_x, maximum_x] = recipe.rock_support_bounds_local();
    (0..=SAMPLES)
        .map(|index| {
            let x = minimum_x + (maximum_x - minimum_x) * index as f32 / SAMPLES as f32;
            recipe.local_crest_height(x)
        })
        .fold(f32::INFINITY, f32::min)
        - 0.25
}

fn extract_implicit_scarp_sheet(
    grid: SurfaceNetsGrid,
    recipe: RiverBluffRecipe,
    terrain: &SceneTerrain,
) -> Option<ExtractedSurface> {
    let [nx, ny, _] = grid.sample_counts;
    let sheet_top = implicit_scarp_sheet_top(recipe);
    let spacing = (grid.maximum - grid.minimum)
        / Vec3::new(
            (grid.sample_counts[0] - 1) as f32,
            (grid.sample_counts[1] - 1) as f32,
            (grid.sample_counts[2] - 1) as f32,
        );
    let mut surface_z = Vec::with_capacity(nx * ny);
    for y in 0..ny {
        for x in 0..nx {
            let local_x = grid.minimum.x + x as f32 * spacing.x;
            let local_y = grid.minimum.y + y as f32 * spacing.y;
            let heightfield_z = heightfield_scarp_local_z(recipe, terrain, local_x, local_y);
            let lower_contact = unit_smoothstep(((local_y - 0.30) / 0.35).clamp(0.0, 1.0));
            let upper_contact =
                1.0 - unit_smoothstep(((local_y - (sheet_top - 1.20)) / 1.20).clamp(0.0, 1.0));
            let authored_weight =
                recipe.implicit_scarp_blend_weight_local(local_x) * lower_contact * upper_contact;
            if let Some(heightfield_z) = heightfield_z {
                let authored_z = recipe.face_surface_local_z(Vec3::new(local_x, local_y, 0.0));
                surface_z
                    .push(heightfield_z * (1.0 - authored_weight) + authored_z * authored_weight);
            } else {
                surface_z.push(grid.maximum.z + spacing.z * 4.0);
            }
        }
    }
    extract_surface_nets(grid, move |local| {
        let x = (((local.x - grid.minimum.x) / spacing.x).round() as usize).min(nx - 1);
        let y = (((local.y - grid.minimum.y) / spacing.y).round() as usize).min(ny - 1);
        surface_z[y * nx + x] - local.z
    })
}

fn retain_implicit_scarp_sheet_domain(surface: &mut ExtractedSurface, recipe: RiverBluffRecipe) {
    let [minimum_x, maximum_x] = recipe.implicit_scarp_render_bounds_local();
    let sheet_top = implicit_scarp_sheet_top(recipe);
    surface.indices = surface
        .indices
        .chunks_exact(3)
        .filter(|triangle| {
            let center = triangle
                .iter()
                .map(|index| Vec3::from_array(surface.positions[*index as usize]))
                .sum::<Vec3>()
                / 3.0;
            center.x >= minimum_x - 0.55
                && center.x <= maximum_x + 0.55
                && center.y >= 0.10
                && center.y <= sheet_top - 0.05
        })
        .flat_map(|triangle| triangle.iter().copied())
        .collect();
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
    terrain: &SceneTerrain,
) -> bool {
    let spacing = f32::from(recipe.sample_spacing_cm) / 100.0;
    let vertices = [triangle[0], triangle[1], triangle[2]]
        .map(|index| Vec3::from_array(surface.positions[index as usize]));
    let center = vertices.into_iter().sum::<Vec3>() / 3.0;
    let [minimum_x, maximum_x, minimum_z, maximum_z] = recipe.implicit_tile_bounds_local();
    let distant_perimeter = (center.x - minimum_x).abs() < 1.4
        || (center.x - maximum_x).abs() < 1.4
        || (center.z - minimum_z).abs() < 1.4
        || (center.z - maximum_z).abs() < 1.4;
    let world = recipe.local_to_world(center);
    let terrain_field = terrain
        .height_at(world.xz())
        .map(|height| world.y - height)
        .unwrap_or(f32::INFINITY);
    let rock_field = recipe.implicit_rock_solid_local(center);
    // Sandstone belongs only to the compact rock lobe that wins the union. Stable bluff slopes
    // remain ordinary terrain even when their heightfield triangles are steep.
    !distant_perimeter && rock_field.abs() <= spacing * 2.0 && rock_field <= terrain_field + spacing
}

fn retain_largest_surface_component(surface: &mut ExtractedSurface) {
    let mut adjacency = vec![Vec::<usize>::new(); surface.positions.len()];
    let mut referenced = vec![false; surface.positions.len()];
    for triangle in surface.indices.chunks_exact(3) {
        for index in triangle {
            referenced[*index as usize] = true;
        }
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            adjacency[a as usize].push(b as usize);
            adjacency[b as usize].push(a as usize);
        }
    }
    let mut visited = vec![false; surface.positions.len()];
    let mut largest = Vec::new();
    for start in 0..surface.positions.len() {
        if !referenced[start] || visited[start] {
            continue;
        }
        let mut component = Vec::new();
        let mut pending = vec![start];
        while let Some(index) = pending.pop() {
            if std::mem::replace(&mut visited[index], true) {
                continue;
            }
            component.push(index);
            pending.extend(adjacency[index].iter().copied());
        }
        if component.len() > largest.len() {
            largest = component;
        }
    }
    let retained = largest
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    surface.indices = surface
        .indices
        .chunks_exact(3)
        .filter(|triangle| {
            triangle
                .iter()
                .all(|index| retained.contains(&(*index as usize)))
        })
        .flat_map(|triangle| triangle.iter().copied())
        .collect();
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

fn partition_folded_surface_triangles(
    surface: &ExtractedSurface,
    recipe: RiverBluffRecipe,
    _terrain: &SceneTerrain,
) -> (Vec<u32>, Vec<u32>) {
    let mut ground_indices = Vec::new();
    let mut rock_indices = Vec::new();
    for triangle in surface.indices.chunks_exact(3) {
        let indices = [triangle[0], triangle[1], triangle[2]];
        let support = indices.map(|index| {
            let local = Vec3::from_array(surface.positions[index as usize]);
            recipe.rock_support_weight_local(local.x)
        });
        let average_height = indices
            .into_iter()
            .map(|index| surface.positions[index as usize][1])
            .sum::<f32>()
            / 3.0;
        let average_crest = indices
            .into_iter()
            .map(|index| recipe.local_crest_height(surface.positions[index as usize][0]))
            .sum::<f32>()
            / 3.0;
        let central_scarp_band = support.into_iter().all(|weight| weight >= 0.22)
            && average_height >= 0.28
            && average_height <= average_crest - 0.24;
        if central_scarp_band {
            rock_indices.extend_from_slice(triangle);
        } else {
            ground_indices.extend_from_slice(triangle);
        }
    }
    (ground_indices, rock_indices)
}

fn folded_triangle_should_retain(
    surface: &ExtractedSurface,
    triangle: &[u32],
    recipe: RiverBluffRecipe,
    terrain: &SceneTerrain,
) -> bool {
    let departed = triangle.iter().any(|index| {
        let local = Vec3::from_array(surface.positions[*index as usize]);
        let world = recipe.local_to_world(local);
        terrain
            .height_at(world.xz())
            .is_some_and(|height| (world.y - height).abs() >= 0.04)
    });
    let central_scarp_band = triangle.iter().all(|index| {
        let local = Vec3::from_array(surface.positions[*index as usize]);
        recipe.rock_support_weight_local(local.x) >= 0.08
    }) && {
        let average_height = triangle
            .iter()
            .map(|index| surface.positions[*index as usize][1])
            .sum::<f32>()
            / 3.0;
        let average_crest = triangle
            .iter()
            .map(|index| recipe.local_crest_height(surface.positions[*index as usize][0]))
            .sum::<f32>()
            / 3.0;
        average_height >= 0.18 && average_height <= average_crest - 0.12
    };
    let shared_terrain_transition = triangle.iter().all(|index| {
        let local = Vec3::from_array(surface.positions[*index as usize]);
        recipe.rock_support_weight_local(local.x) > 0.0
            && local.z >= recipe.minimum_face_local_z(local.x) - 1.4
            && local.z <= recipe.rear_terrace_convergence_start_local_z() + 1.4
    });
    departed || central_scarp_band || shared_terrain_transition
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
        let [minimum_x, maximum_x, minimum_z, maximum_z] = recipe.implicit_tile_bounds_local();
        let grid = SurfaceNetsGrid {
            sample_counts: report.sample_counts.map(usize::from),
            minimum: Vec3::new(minimum_x - padding * 2.0, 0.0, minimum_z - padding * 2.0),
            maximum: Vec3::new(
                maximum_x + padding * 2.0,
                implicit_scarp_sheet_top(recipe),
                maximum_z + padding * 2.0,
            ),
        };
        let mut sandstone = extract_implicit_scarp_sheet(grid, recipe, terrain)
            .expect("validated river bluff sheet produces a finite scalar field");
        retain_implicit_scarp_sheet_domain(&mut sandstone, recipe);
        let triangle_count = sandstone.indices.len() / 3;
        let (ground_indices, rock_indices) =
            partition_folded_surface_triangles(&sandstone, recipe, terrain);
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
                let lateral_contact =
                    1.0 - recipe.rock_support_weight_local(position.x).clamp(0.0, 1.0);
                let contact = crest_contact.max(lateral_contact);
                let ground_contact = [101.0_f32 / 255.0, 82.0 / 255.0, 49.0 / 255.0, 1.0];
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
            depth_bias: 4.0,
            // A small warm ambient lift keeps the shallow undercut readable
            // without flattening the production directional shadow.
            emissive: LinearRgba::new(0.025, 0.012, 0.006, 1.0),
            perceptual_roughness: 0.9,
            metallic: 0.0,
            ..default()
        });
        let mut sandstone_mesh = tile_mesh(&sandstone, rock_indices);
        sandstone_mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
        if !ground_indices.is_empty() {
            commands.entity(root).with_child((
                Name::new("Terrain-matched implicit fold transition"),
                ImplicitTerrainPatchSurface,
                ImplicitTerrainTileGround {
                    source_terrain: terrain_entity,
                },
                Mesh3d(meshes.add(tile_ground_mesh(
                    &sandstone,
                    ground_indices,
                    recipe,
                    terrain,
                ))),
                MeshMaterial3d(tile_ground_material.clone()),
                Transform::default(),
            ));
        }
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
            // Unit extraction uses a coarser but still sub-feature spacing. The committed fixture
            // retains its production 28 cm sampling and is exercised only after this cheap
            // topology screen passes.
            sample_spacing_cm: 42,
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

    fn extract_localized_rock(recipe: RiverBluffRecipe) -> ExtractedSurface {
        let report = recipe.representability().unwrap();
        let padding = f32::from(recipe.sample_spacing_cm) / 100.0;
        let [minimum_x, maximum_x, minimum_z, maximum_z] = recipe.implicit_tile_bounds_local();
        extract_surface_nets(
            SurfaceNetsGrid {
                sample_counts: report.sample_counts.map(usize::from),
                minimum: Vec3::new(
                    minimum_x - padding * 2.0,
                    -padding * 2.0,
                    minimum_z - padding * 2.0,
                ),
                maximum: Vec3::new(
                    maximum_x + padding * 2.0,
                    2.2 + padding * 2.0,
                    maximum_z + padding * 2.0,
                ),
            },
            |local| recipe.implicit_rock_solid_local(local),
        )
        .unwrap()
    }

    #[allow(dead_code)]
    fn legacy_localized_rock_is_closed_connected_and_exposes_only_a_shallow_overhang() {
        let recipe = test_recipe();
        let terrain = test_terrain(recipe);
        let rock = extract_localized_rock(recipe);
        assert!(!rock.indices.is_empty());
        let mut edges = std::collections::HashMap::<(u32, u32), u8>::new();
        let mut adjacency = vec![Vec::<usize>::new(); rock.positions.len()];
        for triangle in rock.indices.chunks_exact(3) {
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                let edge = if a < b { (a, b) } else { (b, a) };
                *edges.entry(edge).or_default() += 1;
                adjacency[a as usize].push(b as usize);
                adjacency[b as usize].push(a as usize);
            }
        }
        assert!(edges.values().all(|count| *count == 2));
        let first = rock.indices[0] as usize;
        let mut visited = vec![false; rock.positions.len()];
        let mut pending = vec![first];
        while let Some(index) = pending.pop() {
            if std::mem::replace(&mut visited[index], true) {
                continue;
            }
            pending.extend(adjacency[index].iter().copied());
        }
        assert!(rock.indices.iter().all(|index| visited[*index as usize]));

        let collapse_x = f32::from(recipe.collapse_offset_cm) / 100.0;
        let bounds = rock.positions.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(minimum, maximum), point| {
                let point = Vec3::from_array(*point);
                (minimum.min(point), maximum.max(point))
            },
        );
        assert!((bounds.0.x - collapse_x).abs() <= 2.9);
        assert!((bounds.1.x - collapse_x).abs() <= 2.9);
        assert!(bounds.0.y >= 0.35 && bounds.1.y <= 2.05);

        let visible_underside = rock
            .positions
            .iter()
            .zip(&rock.normals)
            .filter(|(position, normal)| {
                let local = Vec3::from_array(**position);
                let world = recipe.local_to_world(local);
                let terrain_height = terrain.height_at(world.xz()).unwrap();
                normal[1] < -0.15 && world.y >= terrain_height + 0.08
            })
            .count();
        assert!(visible_underside >= 8);
    }

    #[test]
    fn implicit_scarp_sheet_is_connected_and_returns_to_heightfield() {
        let recipe = test_recipe();
        let terrain = test_terrain(recipe);
        let report = recipe.representability().unwrap();
        let dimensions = recipe.dimensions_metres();
        let padding = f32::from(recipe.sample_spacing_cm) / 100.0;
        let [minimum_x, maximum_x, minimum_z, maximum_z] = recipe.implicit_tile_bounds_local();
        let grid = SurfaceNetsGrid {
            sample_counts: report.sample_counts.map(usize::from),
            minimum: Vec3::new(minimum_x - padding * 2.0, 0.0, minimum_z - padding * 2.0),
            maximum: Vec3::new(
                maximum_x + padding * 2.0,
                implicit_scarp_sheet_top(recipe),
                maximum_z + padding * 2.0,
            ),
        };
        let mut fold = extract_implicit_scarp_sheet(grid, recipe, &terrain).unwrap();
        retain_implicit_scarp_sheet_domain(&mut fold, recipe);
        assert!(!fold.indices.is_empty());

        let mut edge_counts = std::collections::HashMap::<(u32, u32), u8>::new();
        let mut adjacency = vec![Vec::<usize>::new(); fold.positions.len()];
        for triangle in fold.indices.chunks_exact(3) {
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                let edge = if a < b { (a, b) } else { (b, a) };
                *edge_counts.entry(edge).or_default() += 1;
                adjacency[a as usize].push(b as usize);
                adjacency[b as usize].push(a as usize);
            }
        }
        let boundary = edge_counts
            .iter()
            .filter(|(_, count)| **count == 1)
            .flat_map(|((a, b), _)| [*a, *b])
            .collect::<std::collections::BTreeSet<_>>();
        assert!(!boundary.is_empty());
        for index in &boundary {
            let local = Vec3::from_array(fold.positions[*index as usize]);
            let world = recipe.local_to_world(local);
            let base = terrain.height_at(world.xz()).unwrap();
            assert!(
                (world.y - base).abs() <= 1.0,
                "implicit sheet boundary failed to converge into terrain: {local:?}, delta={}m",
                (world.y - base).abs()
            );
        }
        let first = fold.indices[0] as usize;
        let mut visited = vec![false; fold.positions.len()];
        let mut pending = vec![first];
        while let Some(index) = pending.pop() {
            if std::mem::replace(&mut visited[index], true) {
                continue;
            }
            pending.extend(adjacency[index].iter().copied());
        }
        assert!(fold.indices.iter().all(|index| visited[*index as usize]));
        let down_facing = fold
            .indices
            .iter()
            .filter(|index| fold.normals[**index as usize][1] < -0.10)
            .count();
        assert!(down_facing >= 8);

        let (ground_indices, rock_indices) =
            partition_folded_surface_triangles(&fold, recipe, &terrain);
        assert!(!ground_indices.is_empty());
        assert!(!rock_indices.is_empty());
        assert_eq!(
            ground_indices.len() + rock_indices.len(),
            fold.indices.len()
        );
        assert!(
            ground_indices
                .chunks_exact(3)
                .any(|triangle| { triangle.iter().any(|index| boundary.contains(index)) })
        );
        assert!(rock_indices.chunks_exact(3).all(|triangle| {
            let average_height = triangle
                .iter()
                .map(|index| fold.positions[*index as usize][1])
                .sum::<f32>()
                / 3.0;
            let average_crest = triangle
                .iter()
                .map(|index| recipe.local_crest_height(fold.positions[*index as usize][0]))
                .sum::<f32>()
                / 3.0;
            let supported = triangle.iter().all(|index| {
                recipe.rock_support_weight_local(fold.positions[*index as usize][0]) >= 0.22
            });
            let band =
                average_height >= 0.28 && average_height <= average_crest - 0.24 && supported;
            band
        }));
        let rock_height = rock_indices
            .iter()
            .map(|index| fold.positions[*index as usize][1])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(
            rock_height >= dimensions.y * 0.65,
            "central sandstone exposure stopped at {rock_height}m of {}m; tile_z=[{minimum_z},{maximum_z}], rear terrain={}m",
            dimensions.y,
            terrain
                .height_at(
                    recipe
                        .local_to_world(Vec3::new(
                            f32::from(recipe.collapse_offset_cm) / 100.0,
                            0.0,
                            maximum_z,
                        ))
                        .xz(),
                )
                .unwrap()
                - recipe.center_metres().y,
        );

        let mut rock_adjacency = std::collections::HashMap::<u32, Vec<u32>>::new();
        for triangle in rock_indices.chunks_exact(3) {
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                rock_adjacency.entry(a).or_default().push(b);
                rock_adjacency.entry(b).or_default().push(a);
            }
        }
        let mut remaining = rock_adjacency
            .keys()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let mut components = 0;
        while let Some(start) = remaining.pop_first() {
            components += 1;
            let mut pending = vec![start];
            while let Some(index) = pending.pop() {
                if let Some(neighbours) = rock_adjacency.get(&index) {
                    for neighbour in neighbours {
                        if remaining.remove(neighbour) {
                            pending.push(*neighbour);
                        }
                    }
                }
            }
        }
        assert_eq!(
            components, 1,
            "sandstone partition split into {components} components"
        );

        let mut rock_edges = std::collections::HashMap::<(u32, u32), u8>::new();
        for triangle in rock_indices.chunks_exact(3) {
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                let edge = if a < b { (a, b) } else { (b, a) };
                *rock_edges.entry(edge).or_default() += 1;
            }
        }
        let mut boundary_adjacency = std::collections::HashMap::<u32, Vec<u32>>::new();
        for ((a, b), count) in rock_edges {
            if count == 1 {
                boundary_adjacency.entry(a).or_default().push(b);
                boundary_adjacency.entry(b).or_default().push(a);
            }
        }
        let mut remaining = boundary_adjacency
            .keys()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let mut boundary_components = 0;
        while let Some(start) = remaining.pop_first() {
            boundary_components += 1;
            let mut pending = vec![start];
            while let Some(index) = pending.pop() {
                if let Some(neighbours) = boundary_adjacency.get(&index) {
                    for neighbour in neighbours {
                        if remaining.remove(neighbour) {
                            pending.push(*neighbour);
                        }
                    }
                }
            }
        }
        assert_eq!(
            boundary_components, 1,
            "sandstone partition contains {boundary_components} disconnected boundary loops"
        );
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
            bounds.1.x - bounds.0.x <= 5.0 && bounds.1.y <= 1.8 && bounds.1.z - bounds.0.z <= 4.0,
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

    #[allow(dead_code)]
    fn legacy_terrain_relative_terrace_keeps_the_brink_and_converges_before_the_tile_back() {
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
        let collapse_x = f32::from(recipe.collapse_offset_cm) / 100.0;
        let localized_rock = rock_triangles
            .iter()
            .map(center)
            .filter(|point| point.y <= 2.0 && (point.x - collapse_x).abs() <= 2.5)
            .count();
        assert!(
            localized_rock >= 16,
            "localized toe ledge lost rock material ownership"
        );
        assert!(
            rock_triangles
                .iter()
                .map(center)
                .all(|point| point.y <= 2.25 && (point.x - collapse_x).abs() <= 2.8),
            "stable heightfield bluff slope incorrectly became sandstone"
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
        let procedural_assets =
            super::procedural_assets::generate_procedural_environment_assets(&mut images);
        let environment = legacy_scene_environment(&SceneId("implicit-tile-order".into()));
        let material = super::terrain::terrain_material(
            &terrain_component,
            &environment,
            None,
            &procedural_assets,
            &mut images,
        );
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
        let procedural_assets =
            super::procedural_assets::generate_procedural_environment_assets(&mut images);
        let first_environment = legacy_scene_environment(&SceneId("tile-material-first".into()));
        let first = super::terrain::terrain_material(
            &terrain,
            &first_environment,
            None,
            &procedural_assets,
            &mut images,
        );
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
        let finalized = super::terrain::terrain_material(
            &terrain,
            &finalized_environment,
            None,
            &procedural_assets,
            &mut images,
        );
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
