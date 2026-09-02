const INTERNAL_PARTITION_POST_WIDTH_METRES: f32 = 0.16;
const INTERNAL_PARTITION_SILL_HEIGHT_METRES: f32 = 0.12;
const INTERNAL_PARTITION_PLATE_HEIGHT_METRES: f32 = 0.14;
const INTERNAL_PARTITION_RAIL_HEIGHT_METRES: f32 = 0.10;
const INTERNAL_PARTITION_RAIL_ELEVATION_METRES: f32 = 1.12;
const INTERNAL_PARTITION_PLASTER_RECESS_METRES: f32 = 0.010;
const TIMBER_UPPER_FLOOR_SURFACE_OFFSET_METRES: f32 = 0.08;
const TIMBER_CEILING_JOIST_ZONE_METRES: f32 = 0.50;

fn resolved_wall_vertical_span(
    material: crate::WallMaterialClass,
    level: u16,
    base: f32,
    height: f32,
) -> (f32, f32) {
    if material != crate::WallMaterialClass::InternalTimber {
        return (base, height);
    }
    let floor_offset = if level > 0 {
        TIMBER_UPPER_FLOOR_SURFACE_OFFSET_METRES
    } else {
        0.0
    };
    (
        base + floor_offset,
        height - floor_offset - TIMBER_CEILING_JOIST_ZONE_METRES,
    )
}

fn wall_endpoint(wall: crate::WallSegment, sign: f32) -> Vec2 {
    let tangent = if wall.is_horizontal() { Vec2::X } else { Vec2::Y };
    wall.centre() + tangent * sign * CELL_SIZE_METRES * 0.5
}

fn wall_contains_plan_point(wall: crate::WallSegment, point: Vec2) -> bool {
    let tangent = if wall.is_horizontal() { Vec2::X } else { Vec2::Y };
    let delta = point - wall.centre();
    delta.dot(direction_vector(wall.direction)).abs() <= 0.001
        && delta.dot(tangent).abs() <= CELL_SIZE_METRES * 0.5 + 0.001
}

fn wall_has_opening(storey: &StoreyPlan, wall_index: usize) -> bool {
    storey.openings.iter().any(|opening| opening.wall == wall_index)
}

fn partition_endpoint_has_structural_connector(
    storey: &StoreyPlan,
    wall_index: usize,
    endpoint: Vec2,
) -> bool {
    storey.walls.iter().enumerate().any(|(other_index, other)| {
        other_index != wall_index
            && wall_contains_plan_point(*other, endpoint)
            && (other.exterior() || wall_has_opening(storey, other_index))
    })
}

fn has_collinear_partition_beyond(
    storey: &StoreyPlan,
    wall_index: usize,
    endpoint: Vec2,
) -> bool {
    let wall = storey.walls[wall_index];
    storey.walls.iter().enumerate().any(|(other_index, other)| {
        other_index != wall_index
            && !other.exterior()
            && other.is_horizontal() == wall.is_horizontal()
            && [-1.0_f32, 1.0]
                .into_iter()
                .any(|sign| wall_endpoint(*other, sign).distance(endpoint) <= 0.001)
    })
}

fn partition_endpoint_is_post_candidate(
    storey: &StoreyPlan,
    wall_index: usize,
    sign: f32,
) -> bool {
    let endpoint = wall_endpoint(storey.walls[wall_index], sign);
    !partition_endpoint_has_structural_connector(storey, wall_index, endpoint)
        && (sign < 0.0 || !has_collinear_partition_beyond(storey, wall_index, endpoint))
}

fn partition_post_owner(storey: &StoreyPlan, endpoint: Vec2) -> Option<usize> {
    storey
        .walls
        .iter()
        .enumerate()
        .filter(|(_, wall)| !wall.exterior())
        .flat_map(|(wall_index, wall)| {
            [-1.0_f32, 1.0]
                .into_iter()
                .map(move |sign| (wall_index, *wall, sign))
        })
        .filter(|(wall_index, wall, sign)| {
            partition_endpoint_is_post_candidate(storey, *wall_index, *sign)
                && wall_endpoint(*wall, *sign).distance(endpoint) <= 0.001
        })
        .map(|(wall_index, _, _)| wall_index)
        .min()
}

fn partition_endpoint_trim(
    storey: &StoreyPlan,
    wall_index: usize,
    endpoint: Vec2,
    archetype: BuildingArchetype,
) -> f32 {
    let wall = storey.walls[wall_index];
    let perpendicular_depth = storey
        .walls
        .iter()
        .enumerate()
        .filter(|(other_index, other)| {
            *other_index != wall_index
                && wall_contains_plan_point(**other, endpoint)
                && other.is_horizontal() != wall.is_horizontal()
                && (other.exterior() || wall_has_opening(storey, *other_index))
        })
        .map(|(_, other)| {
            wall_material_and_thickness(archetype, other.exterior(), storey.level).2 * 0.5
        })
        .fold(0.0_f32, f32::max);
    if perpendicular_depth > 0.0 {
        perpendicular_depth
    } else if partition_endpoint_has_structural_connector(storey, wall_index, endpoint) {
        0.0
    } else {
        INTERNAL_PARTITION_POST_WIDTH_METRES * 0.5
    }
}

#[derive(Clone, Copy)]
struct InternalPartitionRun {
    centre: Vec2,
    length: f32,
    base: f32,
    height: f32,
    infill_depth: f32,
}

fn internal_partition_run(
    program: &BuildingProgram,
    storey: &StoreyPlan,
    wall_index: usize,
    wall: crate::WallSegment,
    origin: Vec2,
    base: f32,
    height: f32,
    thickness: f32,
) -> InternalPartitionRun {
    let tangent = if wall.is_horizontal() { Vec2::X } else { Vec2::Y };
    let negative = partition_endpoint_trim(
        storey,
        wall_index,
        wall_endpoint(wall, -1.0),
        program.archetype,
    );
    let positive = partition_endpoint_trim(
        storey,
        wall_index,
        wall_endpoint(wall, 1.0),
        program.archetype,
    );
    InternalPartitionRun {
        centre: origin + tangent * (negative - positive) * 0.5,
        length: CELL_SIZE_METRES - negative - positive,
        base,
        height,
        infill_depth: thickness - INTERNAL_PARTITION_PLASTER_RECESS_METRES * 2.0,
    }
}

fn partition_aligned_size(wall: crate::WallSegment, length: f32, height: f32, depth: f32) -> Vec3 {
    if wall.is_horizontal() {
        Vec3::new(length, height, depth)
    } else {
        Vec3::new(depth, height, length)
    }
}

#[expect(clippy::too_many_arguments, reason = "resolved partition solid identity")]
fn push_partition_solid(
    geometry: &mut ResolvedGeometry,
    host_solids: &mut Vec<ResolvedItemId>,
    owner: GeometryOwnerId,
    wall_node: StructuralNodeId,
    slot: u64,
    centre: Vec3,
    size: Vec3,
    role: SolidRole,
) {
    host_solids.push(wall_solid(
        geometry,
        owner,
        slot,
        centre,
        size,
        role,
        crate::ResolvedSolidShape::Cuboid,
        wall_node,
    ));
}

fn append_partition_field(
    wall: crate::WallSegment,
    run: InternalPartitionRun,
    thickness: f32,
    geometry: &mut ResolvedGeometry,
    host_solids: &mut Vec<ResolvedItemId>,
    owner: GeometryOwnerId,
    wall_node: StructuralNodeId,
) {
    let lower_top = INTERNAL_PARTITION_RAIL_ELEVATION_METRES
        - INTERNAL_PARTITION_RAIL_HEIGHT_METRES * 0.5;
    let upper_bottom = INTERNAL_PARTITION_RAIL_ELEVATION_METRES
        + INTERNAL_PARTITION_RAIL_HEIGHT_METRES * 0.5;
    let upper_top = run.height - INTERNAL_PARTITION_PLATE_HEIGHT_METRES;
    for (slot, bottom, top) in [
        (0_u64, INTERNAL_PARTITION_SILL_HEIGHT_METRES, lower_top),
        (1, upper_bottom, upper_top),
    ] {
        let height = (top - bottom).max(0.05);
        push_partition_solid(
            geometry,
            host_solids,
            owner,
            wall_node,
            slot,
            Vec3::new(run.centre.x, run.base + bottom + height * 0.5, run.centre.y),
            partition_aligned_size(wall, run.length, height, run.infill_depth),
            SolidRole::FrameInfill,
        );
    }
    for (slot, elevation, height, role) in [
        (2_u64, INTERNAL_PARTITION_SILL_HEIGHT_METRES * 0.5, INTERNAL_PARTITION_SILL_HEIGHT_METRES, SolidRole::FrameSill),
        (3, run.height - INTERNAL_PARTITION_PLATE_HEIGHT_METRES * 0.5, INTERNAL_PARTITION_PLATE_HEIGHT_METRES, SolidRole::FramePlate),
        (4, INTERNAL_PARTITION_RAIL_ELEVATION_METRES, INTERNAL_PARTITION_RAIL_HEIGHT_METRES, SolidRole::FrameRail),
    ] {
        push_partition_solid(
            geometry,
            host_solids,
            owner,
            wall_node,
            slot,
            Vec3::new(run.centre.x, run.base + elevation, run.centre.y),
            partition_aligned_size(wall, run.length, height, thickness),
            role,
        );
    }
}

#[expect(clippy::too_many_arguments, reason = "resolved partition post identity")]
fn append_partition_posts(
    storey: &StoreyPlan,
    wall_index: usize,
    wall: crate::WallSegment,
    run: InternalPartitionRun,
    thickness: f32,
    geometry: &mut ResolvedGeometry,
    host_solids: &mut Vec<ResolvedItemId>,
    owner: GeometryOwnerId,
    wall_node: StructuralNodeId,
) {
    for (slot, sign) in [(5_u64, -1.0_f32), (6, 1.0)] {
        let endpoint = wall_endpoint(wall, sign);
        if partition_endpoint_is_post_candidate(storey, wall_index, sign)
            && partition_post_owner(storey, endpoint) == Some(wall_index)
        {
            let width = INTERNAL_PARTITION_POST_WIDTH_METRES.max(thickness);
            push_partition_solid(
                geometry,
                host_solids,
                owner,
                wall_node,
                slot,
                Vec3::new(endpoint.x, run.base + run.height * 0.5, endpoint.y),
                Vec3::new(width, run.height, width),
                SolidRole::FramePost,
            );
        }
    }
}

#[expect(clippy::too_many_arguments, reason = "authoritative partition assembly inputs")]
fn append_internal_timber_partition(
    program: &BuildingProgram,
    storey: &StoreyPlan,
    wall_index: usize,
    wall: crate::WallSegment,
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    wall_node: StructuralNodeId,
    origin: Vec2,
    base: f32,
    height: f32,
    thickness: f32,
    host_solids: &mut Vec<ResolvedItemId>,
) {
    let run = internal_partition_run(
        program, storey, wall_index, wall, origin, base, height, thickness,
    );
    append_partition_field(wall, run, thickness, geometry, host_solids, owner, wall_node);
    append_partition_posts(
        storey, wall_index, wall, run, thickness, geometry, host_solids, owner, wall_node,
    );
}

#[expect(clippy::too_many_arguments, reason = "closed wall assembly inputs")]
fn append_closed_wall_assembly(
    program: &BuildingProgram,
    storey: &StoreyPlan,
    wall_index: usize,
    wall: crate::WallSegment,
    material: crate::WallMaterialClass,
    geometry: &mut ResolvedGeometry,
    owner: GeometryOwnerId,
    wall_node: StructuralNodeId,
    origin: Vec2,
    outward: Vec2,
    tangent: Vec2,
    base: f32,
    height: f32,
    thickness: f32,
    host_solids: &mut Vec<ResolvedItemId>,
) {
    if material == crate::WallMaterialClass::InternalTimber {
        append_internal_timber_partition(
            program, storey, wall_index, wall, geometry, owner, wall_node, origin, base, height,
            thickness, host_solids,
        );
        return;
    }
    for (slot, side) in [(0_u64, -1.0_f32), (1, 1.0)] {
        let centre = origin + tangent * side * CELL_SIZE_METRES * 0.25;
        push_partition_solid(
            geometry,
            host_solids,
            owner,
            wall_node,
            slot,
            Vec3::new(centre.x, base + height * 0.5, centre.y),
            partition_aligned_size(wall, CELL_SIZE_METRES * 0.5, height, thickness),
            SolidRole::WallHost,
        );
    }
    if material != crate::WallMaterialClass::CathedralMasonry || !wall.exterior() {
        return;
    }
    let buttress_depth = 0.78;
    for (slot, side) in [(80_u64, -1.0_f32), (81, 1.0)] {
        let plan = origin
            + tangent * side * 0.12
            + outward * (thickness * 0.5 + buttress_depth * 0.5);
        push_partition_solid(
            geometry,
            host_solids,
            owner,
            wall_node,
            slot,
            Vec3::new(plan.x, base + height * 0.44, plan.y),
            partition_aligned_size(wall, 0.24, height * 0.88, buttress_depth),
            SolidRole::WallButtress,
        );
    }
}
