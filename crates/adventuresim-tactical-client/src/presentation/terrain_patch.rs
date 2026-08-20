use super::*;

#[derive(Component)]
pub(crate) struct ImplicitTerrainPatchVisual;

#[derive(Component)]
pub(crate) struct ImplicitTerrainPatchSurface;

fn unit_smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn course_color_weight(relative_height: f32, center: f32, warp: f32) -> f32 {
    1.0 - unit_smoothstep(
        (((relative_height - center - warp).abs() - 0.047) / 0.111).clamp(0.0, 1.0),
    )
}

const UNDERCUT_WARM_SHADOW: [f32; 4] = [0.62, 0.34, 0.21, 1.0];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RetainedTerrainSurface {
    Face,
}

fn classify_authored_surface_triangle(
    positions: [Vec3; 3],
    recipe: RiverBluffRecipe,
) -> Option<RetainedTerrainSurface> {
    let sample_spacing = f32::from(recipe.sample_spacing_cm) / 100.0;
    let face_tolerance = sample_spacing * 2.5;
    // Keep one authored sample between the retained face and the finite scalar side. The crest
    // has already converged into the lower bench there; cutting 2.5 samples inward exposed the
    // semantic triangle boundary as a visible tooth fringe on the returned shoulder.
    let lateral_margin = sample_spacing;
    let implicit_termination = recipe.dimensions_metres().x * 0.5 - lateral_margin;
    let center = positions.into_iter().sum::<Vec3>() / 3.0;
    let inside_lateral_overlap = positions
        .iter()
        .all(|position| position.x.abs() < implicit_termination);
    let inside_finite_rear = positions
        .iter()
        .all(|position| position.z < recipe.dimensions_metres().z - face_tolerance);
    if center.y <= 0.12 || !inside_lateral_overlap || !inside_finite_rear {
        return None;
    }
    let vertices_on_authored_face = positions
        .into_iter()
        .filter(|position| {
            position.y < recipe.local_crest_height(position.x) - sample_spacing * 0.10
                && (position.z - recipe.face_surface_local_z(*position)).abs() <= face_tolerance
        })
        .count();
    let center_crest = recipe.local_crest_height(center.x);
    let center_face_distance = (center.z - recipe.face_surface_local_z(center)).abs();
    if vertices_on_authored_face >= 1
        && center.y < center_crest + sample_spacing * 0.20
        && center_face_distance <= face_tolerance * 1.5
    {
        return Some(RetainedTerrainSurface::Face);
    }

    None
}

fn retain_exposed_scarp(surface: &mut ExtractedSurface, recipe: RiverBluffRecipe) {
    // Only the authored face is rendered. Horizontal upper/rear terrain belongs
    // exclusively to the ordinary heightfield; finite top, back, bottom, and
    // x-side scalar-field closures are all presentation-ineligible.
    surface.indices = surface
        .indices
        .chunks_exact(3)
        .filter(|triangle| {
            let positions = [triangle[0], triangle[1], triangle[2]]
                .map(|index| Vec3::from_array(surface.positions[index as usize]));
            classify_authored_surface_triangle(positions, recipe).is_some()
        })
        .flat_map(|triangle| triangle.iter().copied())
        .collect();
}

fn smooth_retained_crest_boundary(surface: &mut ExtractedSurface, recipe: RiverBluffRecipe) {
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
    let sample_spacing = f32::from(recipe.sample_spacing_cm) / 100.0;
    let crest_tolerance = sample_spacing * 2.5;
    let mut crest_vertices = edge_counts
        .into_iter()
        .filter(|(_, count)| *count == 1)
        .flat_map(|((a, b), _)| [a, b])
        .filter(|source_index| {
            let position = Vec3::from_array(surface.positions[*source_index as usize]);
            position.y >= 0.12
                && position.y >= recipe.local_crest_height(position.x) - crest_tolerance
                && (position.z - recipe.face_surface_local_z(position)).abs() <= crest_tolerance
        })
        .collect::<Vec<_>>();
    crest_vertices.sort_unstable();
    crest_vertices.dedup();
    for source_index in crest_vertices {
        let position = Vec3::from_array(surface.positions[source_index as usize]);
        let crest = recipe.local_crest_height(position.x);
        let snapped = Vec3::new(
            position.x,
            crest,
            recipe.face_surface_local_z(Vec3::new(position.x, crest, position.z)),
        );
        surface.positions[source_index as usize] = snapped.to_array();
    }

    // The retained semantic face has an open lateral edge one sample before the finite scalar
    // side. Collapse the final two-sample band into the lower bench so that edge has no vertical
    // extent to expose as a row of triangle teeth in grazing views.
    let half_width = recipe.dimensions_metres().x * 0.5;
    let lateral_termination = half_width - sample_spacing;
    let closure_start = lateral_termination - sample_spacing * 2.0;
    for position in &mut surface.positions {
        let mut local = Vec3::from_array(*position);
        if local.x.abs() > closure_start {
            let closure = unit_smoothstep(
                ((lateral_termination - local.x.abs()) / (lateral_termination - closure_start))
                    .clamp(0.0, 1.0),
            );
            local.y *= closure;
            local.z = recipe.face_surface_local_z(local);
        }
        // Outside central implicit collision ownership, the regular heightfield owns the lower
        // contact. Bury the low face gently so two nearly coplanar meshes cannot alternate as a
        // triangular fringe; keep the upper returned rock untouched.
        let return_weight = unit_smoothstep(
            ((local.x.abs() - recipe.implicit_collision_half_width()) / 2.0).clamp(0.0, 1.0),
        );
        let toe_weight = 1.0 - unit_smoothstep((local.y / 2.0).clamp(0.0, 1.0));
        local.y -= return_weight * toe_weight * 1.10;
        *position = local.to_array();
    }

    // Snapping is bounded to one sampled crest cell, but the lighting normals must follow the
    // resulting continuous brink rather than preserve the old staircase's faceted highlights.
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

pub(super) fn on_terrain_patch_added(
    event: On<Add, TerrainPatchRecipe>,
    patches: Query<&TerrainPatchRecipe>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) -> Result {
    let patch = *patches.get(event.entity)?;
    let TerrainPatchRecipe::RiverBluff(recipe) = patch;
    let report = recipe
        .representability()
        .expect("validated terrain patch has a bounded sampling report");
    if report.representation != TerrainRepresentation::ImplicitSurface {
        return Ok(());
    }
    let dimensions = recipe.dimensions_metres();
    // One authored sample outside the finite mass is sufficient to close the scalar field. Using
    // a fixed two-metre pad with unchanged sample counts silently coarsened the nominal spacing
    // and produced visible triangular bedding steps.
    let padding = f32::from(recipe.sample_spacing_cm) / 100.0;
    let grid = SurfaceNetsGrid {
        sample_counts: report.sample_counts.map(usize::from),
        minimum: Vec3::new(-dimensions.x * 0.5 - padding, -padding, -padding),
        maximum: Vec3::new(
            dimensions.x * 0.5 + padding,
            dimensions.y + padding,
            dimensions.z + padding,
        ),
    };
    let local_to_world = |local| recipe.local_to_world(local);
    let mut sandstone =
        extract_surface_nets(grid, |local| recipe.signed_distance(local_to_world(local)))
            .expect("validated river bluff produces a finite scalar field");
    retain_exposed_scarp(&mut sandstone, recipe);
    smooth_retained_crest_boundary(&mut sandstone, recipe);
    let triangle_count = sandstone.indices.len() / 3;
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
                ((position.x - (collapse_x - collapse_radius * 0.45)) / (collapse_radius * 0.90))
                    .clamp(0.0, 1.0),
            );
            let release_offset = -0.006 + release_blend * 0.016;
            let end_fade = 1.0
                - unit_smoothstep(
                    (((position[0] / (dimensions.x * 0.5)).abs() - 0.46) / 0.26).clamp(0.0, 1.0),
                );
            let scar_weight = recipe.failure_scar_weight(position);
            let scar_color_fade = 1.0 - scar_weight * 0.82;
            let weak_weight = course_color_weight(relative_height, 0.46, bed_warp + release_offset)
                * end_fade
                * scar_color_fade.clamp(0.0, 1.0);
            let resistant_weight =
                course_color_weight(relative_height, 0.78, bed_warp + release_offset)
                    * end_fade
                    * scar_color_fade.clamp(0.0, 1.0);
            let undercut_recess = recipe.undercut_weight_local(position);
            let undercut_lateral = recipe.undercut_weight_local(Vec3::new(position.x, 0.45, 0.0));
            let resistant_toe_lip = undercut_lateral > 0.12 && (position.y - 1.42).abs() <= 0.34;
            let undercut_shadow = undercut_recess * (-((position.y - 0.72) / 0.48).powi(2)).exp();
            let base_color = [0.78_f32, 0.45, 0.25, 1.0];
            let weak_color = [0.42_f32, 0.21, 0.14, 1.0];
            let resistant_color = [0.94_f32, 0.62, 0.38, 1.0];
            let toe_weight = if resistant_toe_lip { 0.75 } else { 0.0 };
            let resistant_mix = resistant_weight.max(toe_weight);
            let intact_color: [f32; 4] = core::array::from_fn(|channel| {
                let weak_blend =
                    base_color[channel] * (1.0 - weak_weight) + weak_color[channel] * weak_weight;
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
            let fresh_blend = unit_smoothstep(((scar_weight - 0.72) / 0.24).clamp(0.0, 1.0)) * 0.60;
            let surface_color = core::array::from_fn(|channel| {
                intact_color[channel] * (1.0 - fresh_blend) + fresh_color[channel] * fresh_blend
            });
            let rock_color = if undercut_shadow > 0.10 {
                UNDERCUT_WARM_SHADOW
            } else {
                surface_color
            };
            let local_crest = recipe.local_crest_height(position.x);
            let across = (position.x / (dimensions.x * 0.5)).abs();
            let returned_shoulder = unit_smoothstep(((across - 0.42) / 0.25).clamp(0.0, 1.0));
            let end_contact = 1.0 - unit_smoothstep(((local_crest - 3.0) / 1.6).clamp(0.0, 1.0));
            let crest_contact =
                (1.0 - unit_smoothstep(((local_crest - position.y) / 0.9).clamp(0.0, 1.0))) * 0.48;
            let toe_height = 0.58
                + (position.x * 0.53 + position.z * 0.31).sin() * 0.13
                + (position.x * 1.41 - position.z * 0.27).sin() * 0.07;
            let toe_contact =
                1.0 - unit_smoothstep((position.y / toe_height.max(0.32)).clamp(0.0, 1.0));
            let contact = end_contact
                .max(returned_shoulder)
                .max(crest_contact)
                .max(toe_contact);
            // Match the terrain shader's darker mapped response at returned contacts; the old
            // raw base color exposed upward-facing Surface Nets facets as pale stair teeth.
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
        perceptual_roughness: 0.9,
        metallic: 0.0,
        ..default()
    });
    let mut sandstone_mesh = sandstone.into_mesh();
    sandstone_mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
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
    Ok(())
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

    #[test]
    fn retained_crest_boundary_is_smooth_finite_and_bounded() {
        let recipe = RiverBluffRecipe {
            seed: 7_094_698_234_423_137_900,
            center_cm: [0, 0, 1_800],
            yaw_milliradians: 0,
            face_width_cm: 2_800,
            face_height_cm: 900,
            rock_depth_cm: 1_400,
            curvature_cm: 420,
            undercut_depth_cm: 130,
            collapse_offset_cm: 180,
            collapse_radius_cm: 300,
            talus_depth_cm: 700,
            heightfield_error_cm: 650,
            error_tolerance_cm: 75,
            vertical_intersections: 2,
            sample_spacing_cm: 28,
        };
        let dimensions = recipe.dimensions_metres();
        let report = recipe.representability().unwrap();
        let padding = f32::from(recipe.sample_spacing_cm) / 100.0;
        let grid = SurfaceNetsGrid {
            sample_counts: report.sample_counts.map(usize::from),
            minimum: Vec3::new(-dimensions.x * 0.5 - padding, -padding, -padding),
            maximum: Vec3::new(
                dimensions.x * 0.5 + padding,
                dimensions.y + padding,
                dimensions.z + padding,
            ),
        };
        let mut face = extract_surface_nets(grid, |local| {
            recipe.signed_distance(recipe.local_to_world(local))
        })
        .unwrap();
        retain_exposed_scarp(&mut face, recipe);
        smooth_retained_crest_boundary(&mut face, recipe);

        let mut edge_counts = std::collections::HashMap::<(u32, u32), u8>::new();
        for triangle in face.indices.chunks_exact(3) {
            for (a, b) in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                let edge = if a < b { (a, b) } else { (b, a) };
                *edge_counts.entry(edge).or_default() += 1;
            }
        }
        let spacing = f32::from(recipe.sample_spacing_cm) / 100.0;
        let mut crest_vertices = edge_counts
            .into_iter()
            .filter(|(_, count)| *count == 1)
            .flat_map(|((a, b), _)| [a, b])
            .filter(|index| {
                let position = Vec3::from_array(face.positions[*index as usize]);
                position.y >= 0.12
                    && (position.y - recipe.local_crest_height(position.x)).abs() <= 1.0e-4
                    && (position.z - recipe.face_surface_local_z(position)).abs() <= 1.0e-4
            })
            .collect::<Vec<_>>();
        crest_vertices.sort_unstable();
        crest_vertices.dedup();
        assert!(
            crest_vertices.len() >= 24,
            "retained face lost its authored open crest boundary"
        );
        for index in crest_vertices {
            let position = Vec3::from_array(face.positions[index as usize]);
            let normal = Vec3::from_array(face.normals[index as usize]);
            assert!(position.is_finite() && normal.is_finite());
            assert!(
                (0.80..=1.20).contains(&normal.length()),
                "smoothed crest normal was not unit-ish: index={index}, normal={normal:?}"
            );
        }
        let lateral_termination = dimensions.x * 0.5 - spacing;
        let outer_boundary_height = face
            .indices
            .iter()
            .map(|index| Vec3::from_array(face.positions[*index as usize]))
            .filter(|position| position.x.abs() >= lateral_termination - spacing * 0.5)
            .map(|position| position.y)
            .fold(0.0_f32, f32::max);
        assert!(
            outer_boundary_height <= 0.25,
            "open returned-face edge must be buried in the lower bench: height={outer_boundary_height}"
        );
        for triangle in face.indices.chunks_exact(3) {
            let positions = [triangle[0], triangle[1], triangle[2]]
                .map(|index| Vec3::from_array(face.positions[index as usize]));
            for edge in [[0, 1], [1, 2], [2, 0]] {
                let length = positions[edge[0]].distance(positions[edge[1]]);
                assert!(
                    length <= spacing * 5.0,
                    "retained face developed a pathological edge: a={:?}, b={:?}, length={length}",
                    positions[edge[0]],
                    positions[edge[1]],
                );
            }
        }
    }

    #[test]
    fn cliff_surface_is_finite_bounded_and_deterministic() {
        let recipe = RiverBluffRecipe {
            seed: 7,
            center_cm: [0, 0, 0],
            yaw_milliradians: 0,
            face_width_cm: 2_800,
            face_height_cm: 900,
            rock_depth_cm: 1_400,
            curvature_cm: 420,
            undercut_depth_cm: 130,
            collapse_offset_cm: 180,
            collapse_radius_cm: 300,
            talus_depth_cm: 600,
            heightfield_error_cm: 650,
            error_tolerance_cm: 75,
            vertical_intersections: 2,
            sample_spacing_cm: 50,
        };
        let size = recipe.dimensions_metres();
        let grid = SurfaceNetsGrid {
            sample_counts: recipe.sample_counts().map(usize::from),
            minimum: Vec3::new(-size.x * 0.5 - 1.0, -1.0, -1.0),
            maximum: Vec3::new(size.x * 0.5 + 1.0, size.y + 1.0, size.z + 1.0),
        };
        let extract = || {
            let mut surface = extract_surface_nets(grid, |local| {
                recipe.signed_distance(recipe.local_to_world(local))
            })
            .unwrap();
            retain_exposed_scarp(&mut surface, recipe);
            surface
        };
        let first = extract();
        assert_eq!(first, extract());
        assert!(!first.positions.is_empty());
        assert!(!first.indices.is_empty());
        assert!(
            first
                .positions
                .iter()
                .flatten()
                .all(|value| value.is_finite())
        );
        assert!(first.indices.len() / 3 <= MAX_TERRAIN_PATCH_TRIANGLES);
        let sample_spacing = f32::from(recipe.sample_spacing_cm) / 100.0;
        let face_tolerance = sample_spacing * 2.5;
        let lateral_margin = sample_spacing;
        let implicit_termination = size.x * 0.5 - lateral_margin;
        let mut referenced_positions = Vec::new();
        let mut face_positions = Vec::new();
        let mut _face_triangles = 0_usize;
        for triangle in first.indices.chunks_exact(3) {
            let positions = [triangle[0], triangle[1], triangle[2]]
                .map(|index| Vec3::from_array(first.positions[index as usize]));
            referenced_positions.extend(positions);
            let center = positions.into_iter().sum::<Vec3>() / 3.0;
            assert!(
                center.y > 0.12,
                "buried bottom closure leaked into render mesh"
            );
            assert_eq!(
                classify_authored_surface_triangle(positions, recipe),
                Some(RetainedTerrainSurface::Face),
                "retained Surface Nets triangle must belong to the authored face"
            );
            _face_triangles += 1;
            face_positions.extend(positions);
            assert!(
                positions
                    .into_iter()
                    .all(|position| position.x.abs() < implicit_termination),
                "finite scalar-field side closure leaked into the tapered face"
            );
            for edge in [[0, 1], [1, 2], [2, 0]] {
                assert!(
                    positions[edge[0]].distance(positions[edge[1]]) <= sample_spacing * 5.0,
                    "retained triangle contains a pathological span"
                );
            }
        }
        assert!(!face_positions.is_empty());

        // Every mid-face band owned by the implicit central sector has retained
        // geometry. Returned shoulders outside this interval are deliberately
        // heightfield-owned; the termination and finite-edge assertions above
        // prove that the narrowed mesh remains buried there.
        let dense_face_half_width = recipe.implicit_collision_half_width().floor() as i16;
        for x_step in -dense_face_half_width..=dense_face_half_width {
            let x = f32::from(x_step);
            let crest = recipe.local_crest_height(x);
            for height_fraction in [0.22_f32, 0.48, 0.72] {
                let expected_y = crest * height_fraction;
                let nearest_xy = face_positions
                    .iter()
                    .map(|position| Vec2::new(position.x - x, position.y - expected_y).length())
                    .fold(f32::INFINITY, f32::min);
                assert!(
                    nearest_xy <= sample_spacing * 2.5,
                    "missing retained face coverage near x={x}, y={expected_y}"
                );
            }
        }
        let face_x_bounds = face_positions.iter().fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(minimum, maximum), position| (minimum.min(position.x), maximum.max(position.x)),
        );
        for contact_x in [face_x_bounds.0, face_x_bounds.1] {
            let contact_distance = contact_x.abs();
            assert!(contact_distance < implicit_termination);
            assert!(
                contact_distance > recipe.implicit_collision_half_width()
                    && contact_distance <= implicit_termination
                    && implicit_termination - contact_distance <= face_tolerance * 1.5
                    && recipe.local_crest_height(contact_x) < 0.5,
                "each authored returned face must taper into the lower bench before the excluded finite side: x={contact_x}, finite contact={}, crest={}",
                implicit_termination,
                recipe.local_crest_height(contact_x),
            );
        }

        let collapse_x = f32::from(recipe.collapse_offset_cm) / 100.0;
        let undercut_positions = face_positions
            .iter()
            .filter(|position| {
                (position.x - collapse_x).abs() <= 2.3 && (0.20..=1.50).contains(&position.y)
            })
            .collect::<Vec<_>>();
        assert!(
            undercut_positions.len() >= 12,
            "localized undercut lost its retained overhang triangles"
        );
        let undercut_z_span = undercut_positions.iter().map(|position| position.z).fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            |(minimum, maximum), z| (minimum.min(z), maximum.max(z)),
        );
        assert!(
            undercut_z_span.1 - undercut_z_span.0 >= 0.9,
            "retained toe must show a material setback beneath the resistant bed"
        );
    }
}
