fn resolved_solid_bounds(solid: &ResolvedSolid) -> (Vec3, Vec3) {
    let cosine = solid.yaw_radians.cos().abs();
    let sine = solid.yaw_radians.sin().abs();
    let half = Vec3::new(
        (solid.size.x * cosine + solid.size.z * sine) * 0.5,
        solid.size.y * 0.5,
        (solid.size.x * sine + solid.size.z * cosine) * 0.5,
    );
    (solid.centre - half, solid.centre + half)
}

fn tower_chord_void_separates(
    plan: &BuildingPlan,
    shell: &ResolvedSolid,
    other: &ResolvedSolid,
) -> bool {
    let Some((tower_index, tower)) = plan
        .wall_assemblies
        .iter()
        .find(|wall| wall.owner == shell.owner)
        .and_then(|wall| match wall.source {
            crate::WallSourceId::RoundTower { tower_index } => {
                Some((tower_index, plan.towers.get(tower_index)?))
            }
            _ => None,
        })
    else {
        return false;
    };
    let _ = tower_index;
    let (min, max) = resolved_solid_bounds(other);
    let centre = Vec2::new((min.x + max.x) * 0.5, (min.z + max.z) * 0.5);
    let half = Vec2::new((max.x - min.x) * 0.5, (max.z - min.z) * 0.5);
    tower.chord_interfaces().any(|interface| {
        let toward = match interface.toward_gate {
            crate::Direction::North => Vec2::Y,
            crate::Direction::East => Vec2::X,
            crate::Direction::South => -Vec2::Y,
            crate::Direction::West => -Vec2::X,
        };
        let minimum_projection = (centre - tower.centre_metres()).dot(toward)
            - half.x * toward.x.abs()
            - half.y * toward.y.abs();
        minimum_projection >= tower.radius_metres() - interface.bearing_depth.metres() - 0.025
    })
}

fn point_is_inside_tower_chord_void(
    plan: &BuildingPlan,
    shell: &ResolvedSolid,
    point: Vec3,
) -> bool {
    let Some(tower) = plan
        .wall_assemblies
        .iter()
        .find(|wall| wall.owner == shell.owner)
        .and_then(|wall| match wall.source {
            crate::WallSourceId::RoundTower { tower_index } => plan.towers.get(tower_index),
            _ => None,
        })
    else {
        return false;
    };
    tower.chord_interfaces().any(|interface| {
        let toward = match interface.toward_gate {
            crate::Direction::North => Vec2::Y,
            crate::Direction::East => Vec2::X,
            crate::Direction::South => -Vec2::Y,
            crate::Direction::West => -Vec2::X,
        };
        (Vec2::new(point.x, point.z) - tower.centre_metres()).dot(toward)
            >= tower.radius_metres() - interface.bearing_depth.metres() - 0.025
    })
}

fn segment_is_inside_tower_chord_void(
    plan: &BuildingPlan,
    shell: &ResolvedSolid,
    start: Vec3,
    end: Vec3,
) -> bool {
    point_is_inside_tower_chord_void(plan, shell, start)
        && point_is_inside_tower_chord_void(plan, shell, end)
}

fn valid_tower_chord_bond(plan: &BuildingPlan, bond: &crate::JunctionBond) -> bool {
    for (shell_owner, target_owner) in [
        (bond.owners[0], bond.owners[1]),
        (bond.owners[1], bond.owners[0]),
    ] {
        let Some(shell) = plan.resolved_geometry.solids.iter().find(|solid| {
            solid.owner == shell_owner
                && matches!(
                    solid.shape,
                    crate::ResolvedSolidShape::RoundTowerShell { .. }
                )
        }) else {
            continue;
        };
        if plan
            .resolved_geometry
            .solids
            .iter()
            .filter(|solid| solid.owner == target_owner)
            .any(|solid| tower_chord_void_separates(plan, shell, solid))
            && bond.minimum_interface_area_square_metres >= 0.08
            && bond.maximum_penetration_metres <= 0.08
        {
            return true;
        }
    }
    false
}

fn resolved_solid_contains_point(solid: &ResolvedSolid, point: Vec3, tolerance: f32) -> bool {
    let relative = point - solid.centre;
    let (sine, cosine) = solid.yaw_radians.sin_cos();
    let local = Vec3::new(
        relative.x * cosine - relative.z * sine,
        relative.y,
        relative.x * sine + relative.z * cosine,
    );
    let half = solid.size * 0.5 + Vec3::splat(tolerance);
    if !local.abs().cmple(half).all() {
        return false;
    }
    match solid.shape {
        crate::ResolvedSolidShape::AnnularPrism {
            inner_radius_metres,
            outer_radius_metres,
            ..
        } => {
            let radius = Vec2::new(local.x, local.z).length();
            radius >= inner_radius_metres - tolerance && radius <= outer_radius_metres + tolerance
        }
        crate::ResolvedSolidShape::AnnularSectorPrism {
            inner_radius_metres,
            outer_radius_metres,
            start_angle_radians,
            end_angle_radians,
            ..
        } => {
            let radius = Vec2::new(local.x, local.z).length();
            let angle = local.z.atan2(local.x).rem_euclid(std::f32::consts::TAU);
            let start = start_angle_radians.rem_euclid(std::f32::consts::TAU);
            let sweep = (end_angle_radians - start_angle_radians)
                .rem_euclid(std::f32::consts::TAU)
                .max(0.0001);
            radius >= inner_radius_metres - tolerance
                && radius <= outer_radius_metres + tolerance
                && (angle - start).rem_euclid(std::f32::consts::TAU) <= sweep + 0.0001
        }
        crate::ResolvedSolidShape::RoundTowerShell {
            inner_radius_metres,
            outer_radius_metres,
            ..
        } => {
            let radius = Vec2::new(local.x, local.z).length();
            radius >= inner_radius_metres - tolerance && radius <= outer_radius_metres + tolerance
        }
        _ => true,
    }
}

fn artillery_route_solid_contains(solid: &ResolvedSolid, point: Vec3, tolerance: f32) -> bool {
    if let crate::ResolvedSolidShape::RoundTowerShell {
        outer_radius_metres,
        chord_interfaces,
        ..
    } = solid.shape
    {
        let radial =
            Vec2::new(point.x - solid.centre.x, point.z - solid.centre.z).normalize_or_zero();
        if chord_interfaces.into_iter().flatten().any(|interface| {
            radial.dot(direction_vector(interface.toward_gate))
                > (outer_radius_metres - interface.bearing_depth.metres()) / outer_radius_metres
        }) {
            return false;
        }
    }
    resolved_solid_contains_point(solid, point, tolerance)
}

fn opening_host_contains_point(
    opening: &crate::OpeningAssembly,
    wall: &crate::WallAssembly,
    solid: &ResolvedSolid,
    point: Vec3,
) -> bool {
    if !resolved_solid_contains_point(solid, point, 0.001) {
        return false;
    }
    let plan = Vec2::new(point.x, point.z);
    let along = (plan - opening.frame.origin).dot(opening.frame.tangent);
    let depth = (plan - opening.frame.origin).dot(opening.frame.outward);
    let depth_fraction = (0.5 - depth / wall.thickness_metres).clamp(0.0, 1.0);
    match solid.shape {
        crate::ResolvedSolidShape::SplayedReveal {
            exterior_width_metres,
            interior_width_metres,
            side,
            ..
        } => {
            let clear_width = exterior_width_metres
                + (interior_width_metres - exterior_width_metres) * depth_fraction;
            if side < 0 {
                along <= -clear_width * 0.5 + 0.001
            } else {
                along >= clear_width * 0.5 - 0.001
            }
        }
        crate::ResolvedSolidShape::SplayedHead {
            exterior_clear_height_metres,
            interior_clear_height_metres,
            ..
        } => {
            let clear_height = exterior_clear_height_metres
                + (interior_clear_height_metres - exterior_clear_height_metres) * depth_fraction;
            point.y + 0.001 >= opening.sill_elevation_metres + clear_height
        }
        _ => true,
    }
}

fn bounds_overlap_3d(a: (Vec3, Vec3), b: (Vec3, Vec3), tolerance: f32) -> bool {
    a.1.x.min(b.1.x) - a.0.x.max(b.0.x) > tolerance
        && a.1.y.min(b.1.y) - a.0.y.max(b.0.y) > tolerance
        && a.1.z.min(b.1.z) - a.0.z.max(b.0.z) > tolerance
}

fn resolved_shape_overlap(a: &ResolvedSolid, b: &ResolvedSolid, tolerance: f32) -> bool {
    if !bounds_overlap_3d(
        resolved_solid_bounds(a),
        resolved_solid_bounds(b),
        tolerance,
    ) {
        return false;
    }
    if !matches!(
        a.shape,
        crate::ResolvedSolidShape::AnnularPrism { .. }
            | crate::ResolvedSolidShape::AnnularSectorPrism { .. }
    ) && !matches!(
        b.shape,
        crate::ResolvedSolidShape::AnnularPrism { .. }
            | crate::ResolvedSolidShape::AnnularSectorPrism { .. }
    ) {
        return true;
    }
    let (amin, amax) = resolved_solid_bounds(a);
    let (bmin, bmax) = resolved_solid_bounds(b);
    let min = amin.max(bmin);
    let max = amax.min(bmax);
    (0..=8).any(|x| {
        (0..=4).any(|y| {
            (0..=8).any(|z| {
                let point = Vec3::new(
                    min.x + (max.x - min.x) * x as f32 / 8.0,
                    min.y + (max.y - min.y) * y as f32 / 4.0,
                    min.z + (max.z - min.z) * z as f32 / 8.0,
                );
                resolved_solid_contains_point(a, point, -tolerance)
                    && resolved_solid_contains_point(b, point, -tolerance)
            })
        })
    })
}

fn resolved_shape_overlaps_bounds(
    solid: &ResolvedSolid,
    bounds: (Vec3, Vec3),
    tolerance: f32,
) -> bool {
    if !matches!(
        solid.shape,
        crate::ResolvedSolidShape::AnnularPrism { .. }
            | crate::ResolvedSolidShape::AnnularSectorPrism { .. }
    ) {
        return resolved_solid_overlaps_bounds(solid, bounds, tolerance);
    }
    (0..=8).any(|x| {
        (0..=4).any(|y| {
            (0..=8).any(|z| {
                let point = Vec3::new(
                    bounds.0.x + (bounds.1.x - bounds.0.x) * x as f32 / 8.0,
                    bounds.0.y + (bounds.1.y - bounds.0.y) * y as f32 / 4.0,
                    bounds.0.z + (bounds.1.z - bounds.0.z) * z as f32 / 8.0,
                );
                resolved_solid_contains_point(solid, point, -tolerance)
            })
        })
    })
}

fn oriented_occupant_overlaps_solid(
    foot: Vec3,
    along: Vec2,
    across: Vec2,
    solid: &ResolvedSolid,
    tolerance: f32,
) -> bool {
    let (solid_min, solid_max) = resolved_solid_bounds(solid);
    if solid_max.y.min(foot.y + 1.90) - solid_min.y.max(foot.y) <= tolerance {
        return false;
    }
    let cosine = solid.yaw_radians.cos();
    let sine = solid.yaw_radians.sin();
    let solid_x = Vec2::new(cosine, -sine);
    let solid_z = Vec2::new(sine, cosine);
    let delta = Vec2::new(solid.centre.x - foot.x, solid.centre.z - foot.z);
    [along, across, solid_x, solid_z].into_iter().all(|axis| {
        let occupant_radius = 0.10 * along.dot(axis).abs() + 0.45 * across.dot(axis).abs();
        let solid_radius = solid.size.x * 0.5 * solid_x.dot(axis).abs()
            + solid.size.z * 0.5 * solid_z.dot(axis).abs();
        occupant_radius + solid_radius - delta.dot(axis).abs() > tolerance
    })
}

fn resolved_solid_overlaps_bounds(
    solid: &ResolvedSolid,
    bounds: (Vec3, Vec3),
    tolerance: f32,
) -> bool {
    let bounds_centre = (bounds.0 + bounds.1) * 0.5;
    let bounds_half = (bounds.1 - bounds.0) * 0.5;
    let rotation = Quat::from_rotation_y(solid.yaw_radians)
        * Quat::from_rotation_x(solid.crossfall_radians)
        * Quat::from_rotation_z(solid.longfall_radians);
    let solid_axes = [rotation * Vec3::X, rotation * Vec3::Y, rotation * Vec3::Z];
    let world_axes = [Vec3::X, Vec3::Y, Vec3::Z];
    let solid_half = solid.size * 0.5;
    let delta = bounds_centre - solid.centre;
    let mut axes = Vec::with_capacity(15);
    axes.extend(world_axes);
    axes.extend(solid_axes);
    for world in world_axes {
        for local in solid_axes {
            let cross = world.cross(local);
            if cross.length_squared() > 0.000_001 {
                axes.push(cross.normalize());
            }
        }
    }
    axes.into_iter().all(|axis| {
        let solid_radius = solid_half.x * solid_axes[0].dot(axis).abs()
            + solid_half.y * solid_axes[1].dot(axis).abs()
            + solid_half.z * solid_axes[2].dot(axis).abs();
        let bounds_radius = bounds_half.x * axis.x.abs()
            + bounds_half.y * axis.y.abs()
            + bounds_half.z * axis.z.abs();
        solid_radius + bounds_radius - delta.dot(axis).abs() > tolerance
    })
}

fn oriented_cuboids_overlap(a: &ResolvedSolid, b: &ResolvedSolid, tolerance: f32) -> bool {
    let rotation = |solid: &ResolvedSolid| {
        Quat::from_rotation_y(solid.yaw_radians)
            * Quat::from_rotation_x(solid.crossfall_radians)
            * Quat::from_rotation_z(solid.longfall_radians)
    };
    let a_rotation = rotation(a);
    let b_rotation = rotation(b);
    let a_axes = [
        a_rotation * Vec3::X,
        a_rotation * Vec3::Y,
        a_rotation * Vec3::Z,
    ];
    let b_axes = [
        b_rotation * Vec3::X,
        b_rotation * Vec3::Y,
        b_rotation * Vec3::Z,
    ];
    let delta = b.centre - a.centre;
    let a_half = a.size * 0.5;
    let b_half = b.size * 0.5;
    let radius = |half: Vec3, axes: [Vec3; 3], axis: Vec3| {
        half.x * axes[0].dot(axis).abs()
            + half.y * axes[1].dot(axis).abs()
            + half.z * axes[2].dot(axis).abs()
    };
    let mut axes = Vec::with_capacity(15);
    axes.extend(a_axes);
    axes.extend(b_axes);
    for left in a_axes {
        for right in b_axes {
            let cross = left.cross(right);
            if cross.length_squared() > 0.000_001 {
                axes.push(cross.normalize());
            }
        }
    }
    axes.into_iter().all(|axis| {
        radius(a_half, a_axes, axis) + radius(b_half, b_axes, axis) - delta.dot(axis).abs()
            > tolerance
    })
}

fn resolved_solids_overlap_positive_volume(
    left: &ResolvedSolid,
    right: &ResolvedSolid,
    tolerance: f32,
) -> bool {
    let left_vertical = (
        left.centre.y - left.size.y * 0.5,
        left.centre.y + left.size.y * 0.5,
    );
    let right_vertical = (
        right.centre.y - right.size.y * 0.5,
        right.centre.y + right.size.y * 0.5,
    );
    if left_vertical.1.min(right_vertical.1) - left_vertical.0.max(right_vertical.0) <= tolerance {
        return false;
    }
    let left_x = Vec2::new(left.yaw_radians.cos(), -left.yaw_radians.sin());
    let left_z = Vec2::new(left.yaw_radians.sin(), left.yaw_radians.cos());
    let right_x = Vec2::new(right.yaw_radians.cos(), -right.yaw_radians.sin());
    let right_z = Vec2::new(right.yaw_radians.sin(), right.yaw_radians.cos());
    let delta = Vec2::new(
        right.centre.x - left.centre.x,
        right.centre.z - left.centre.z,
    );
    [left_x, left_z, right_x, right_z].into_iter().all(|axis| {
        let left_radius =
            left.size.x * 0.5 * left_x.dot(axis).abs() + left.size.z * 0.5 * left_z.dot(axis).abs();
        let right_radius = right.size.x * 0.5 * right_x.dot(axis).abs()
            + right.size.z * 0.5 * right_z.dot(axis).abs();
        left_radius + right_radius - delta.dot(axis).abs() > tolerance
    })
}

fn resolved_plan_overlap_area(left: &ResolvedSolid, right: &ResolvedSolid) -> f32 {
    let local_x = Vec2::new(left.yaw_radians.cos(), -left.yaw_radians.sin());
    let local_z = Vec2::new(left.yaw_radians.sin(), left.yaw_radians.cos());
    let delta = Vec2::new(
        right.centre.x - left.centre.x,
        right.centre.z - left.centre.z,
    );
    let right_x = Vec2::new(right.yaw_radians.cos(), -right.yaw_radians.sin());
    let right_z = Vec2::new(right.yaw_radians.sin(), right.yaw_radians.cos());
    let overlap = |axis: Vec2, left_extent: f32| {
        let right_extent = right.size.x * 0.5 * right_x.dot(axis).abs()
            + right.size.z * 0.5 * right_z.dot(axis).abs();
        (left_extent + right_extent - delta.dot(axis).abs()).max(0.0)
    };
    overlap(local_x, left.size.x * 0.5) * overlap(local_z, left.size.z * 0.5)
}

fn bonded_interface_metrics(
    a: &ResolvedSolid,
    b: &ResolvedSolid,
) -> Option<(Vec3, Vec3, f32, f32)> {
    let (a_min, a_max) = resolved_solid_bounds(a);
    let (b_min, b_max) = resolved_solid_bounds(b);
    let signed = a_max.min(b_max) - a_min.max(b_min);
    let mut axes = [(signed.x, 0_usize), (signed.y, 1), (signed.z, 2)];
    axes.sort_by(|left, right| left.0.total_cmp(&right.0));
    if axes[0].0 < -0.025 || axes[1].0 <= 0.0 || axes[2].0 <= 0.0 {
        return None;
    }
    let contact_min = a_min.max(b_min);
    let mut contact_max = a_max.min(b_max);
    if axes[0].0 < 0.0 {
        let axis = axes[0].1;
        let midpoint = (contact_min[axis] + contact_max[axis]) * 0.5;
        contact_max[axis] = midpoint;
    }
    Some((
        contact_min.min(contact_max),
        contact_min.max(contact_max),
        axes[1].0 * axes[2].0,
        axes[0].0.max(0.0),
    ))
}

/// Conservative cuboid-in-cavity test for resolved round shells.  This keeps
/// the generic AABB overlap sweep from treating a gun mount or casemate fitting
/// wholly inside the hollow cylinder as masonry penetration.
fn round_shell_clears_inner_solid(shell: &ResolvedSolid, inner: &ResolvedSolid) -> bool {
    let crate::ResolvedSolidShape::RoundTowerShell {
        inner_radius_metres,
        ..
    } = shell.shape
    else {
        return false;
    };
    let (min, max) = resolved_solid_bounds(inner);
    [
        Vec2::new(min.x, min.z),
        Vec2::new(min.x, max.z),
        Vec2::new(max.x, min.z),
        Vec2::new(max.x, max.z),
    ]
    .into_iter()
    .all(|corner| {
        corner.distance(Vec2::new(shell.centre.x, shell.centre.z)) <= inner_radius_metres - 0.005
    })
}
