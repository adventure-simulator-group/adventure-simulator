//! Exact render meshes derived from accepted resolved building geometry.
//!
//! Unlike the facade and shell LODs, this representation preserves interior
//! walls, opening assemblies, timber members, floors, and roof framing. It is
//! render-only; tactical collision remains independently compiled.

use std::collections::{BTreeMap, BTreeSet};

use bevy::math::{Quat, Vec2, Vec3};

use crate::{
    BuildingLodMaterial, BuildingPlan, LodMesh, ResolvedSolid, ResolvedSolidShape, RoofMaterial,
    RoofSurface, SolidRole, WallMaterialClass, WallStyle, compile_operable_doors,
    compile_operable_windows, compile_window_bars, tessellate_roof_enclosure, tessellate_roof_face,
};

/// Physical metres represented by one unit in exact-detail mesh UV space.
///
/// Renderers scale this common metric space to each texture recipe's authored
/// tile size. Keeping the geometry contract material-agnostic lets plaster,
/// timber, and floorboards share a mesh compiler without stretching.
pub const BUILDING_DETAIL_UV_METRES_PER_UNIT: f32 = 2.0;
const TIMBER_SEAM_COVER_METRES: f32 = 0.008;

/// Material-batched exact geometry for a playable building.
#[derive(Clone, Debug)]
pub struct BuildingDetail {
    pub meshes: Vec<LodMesh>,
}

impl BuildingDetail {
    fn mesh_mut(&mut self, material: BuildingLodMaterial) -> &mut LodMesh {
        if let Some(index) = self
            .meshes
            .iter()
            .position(|mesh| mesh.material == material)
        {
            return &mut self.meshes[index];
        }
        self.meshes.push(LodMesh::new(material));
        self.meshes.last_mut().expect("mesh was just inserted")
    }
}

/// Compiles the authoritative high-detail representation used in playable space.
pub fn compile_building_detail(plan: &BuildingPlan) -> BuildingDetail {
    compile_detail(plan, &BTreeSet::new())
}

/// Compiles high detail while reserving operable exterior leaves for dynamic entities.
pub fn compile_static_building_detail(plan: &BuildingPlan) -> BuildingDetail {
    let dynamic_closure_solids = compile_operable_doors(plan)
        .into_iter()
        .map(|door| door.source)
        .chain(
            compile_operable_windows(plan)
                .into_iter()
                .map(|window| window.source),
        )
        .collect::<BTreeSet<_>>();
    compile_detail(plan, &dynamic_closure_solids)
}

fn compile_detail(
    plan: &BuildingPlan,
    excluded_solids: &BTreeSet<crate::ResolvedItemId>,
) -> BuildingDetail {
    let mut detail = BuildingDetail { meshes: Vec::new() };
    let panel_boundary_edges = panel_boundary_edges(plan);

    for solid in &plan.resolved_geometry.solids {
        if excluded_solids.contains(&solid.id) {
            continue;
        }
        if matches!(solid.shape, ResolvedSolidShape::RoundTowerShell { .. }) {
            continue;
        }
        let material = material_for_solid(plan, solid);
        let wall = wall_for_solid(plan, solid);
        if matches!(
            solid.role,
            SolidRole::OpeningJamb
                | SolidRole::OpeningSill
                | SolidRole::OpeningHead
                | SolidRole::OpeningSpandrel
                | SolidRole::OpeningReveal
        ) {
            // These are recessed structural bearing solids, not a second
            // visible finish. The resolved infill and timber opening frame
            // already own the exposed Fachwerk surface.
            continue;
        }
        match solid.shape {
            ResolvedSolidShape::TimberPanelPrism {
                vertices,
                outward,
                depth_metres,
            } => append_timber_panel(
                &mut detail,
                material,
                vertices,
                outward,
                depth_metres,
                wall,
                std::array::from_fn(|edge| {
                    wall.is_none_or(|wall| {
                        panel_boundary_edges.contains(&(
                            wall.id,
                            panel_edge_key(vertices[edge], vertices[(edge + 1) % 3]),
                        ))
                    })
                }),
            ),
            _ => append_oriented_cuboid(&mut detail, material, solid, wall),
        }
    }
    for bar in compile_window_bars(plan) {
        append_cuboid_faces(
            &mut detail,
            BuildingLodMaterial::Iron,
            bar.centre,
            bar.size_metres,
            Quat::from_rotation_y(bar.yaw_radians),
            None,
        );
    }
    append_roofs(&mut detail, plan);
    detail
        .meshes
        .retain(|mesh| !mesh.vertices.is_empty() && !mesh.indices.is_empty());
    detail
}

fn material_for_solid(plan: &BuildingPlan, solid: &ResolvedSolid) -> BuildingLodMaterial {
    let wall_material = wall_for_solid(plan, solid).map(|wall| wall.material);
    material_for_solid_body(plan, solid, wall_material)
}

fn wall_for_solid<'plan>(
    plan: &'plan BuildingPlan,
    solid: &ResolvedSolid,
) -> Option<&'plan crate::WallAssembly> {
    if let Some(wall) = plan.wall_assemblies.iter().find(|wall| {
        wall.host_solids.contains(&solid.id) || wall.replaced_by_owner == Some(solid.owner)
    }) {
        return Some(wall);
    }
    if let Some(wall_id) = plan.timber_frame.as_ref().and_then(|frame| {
        let member = frame
            .members
            .iter()
            .find(|member| member.solid == solid.id)?;
        frame
            .bays
            .iter()
            .find(|bay| bay.member_ids.contains(&member.id))?
            .wall
    }) {
        return plan.wall_assemblies.iter().find(|wall| wall.id == wall_id);
    }
    plan.wall_assemblies
        .iter()
        .find(|wall| wall.owner == solid.owner || wall.replaced_by_owner == Some(solid.owner))
}

fn material_for_solid_body(
    plan: &BuildingPlan,
    solid: &ResolvedSolid,
    wall_material: Option<WallMaterialClass>,
) -> BuildingLodMaterial {
    let infill_material = wall_material.unwrap_or(match plan.wall_style {
        WallStyle::Brick => WallMaterialClass::CivilianMasonry,
        WallStyle::Stone => WallMaterialClass::FortifiedMasonry,
        WallStyle::TimberFrame | WallStyle::Plaster => WallMaterialClass::TimberInfill,
    });
    match solid.role {
        SolidRole::EdgeGuard
        | SolidRole::FrameMember
        | SolidRole::FrameSill
        | SolidRole::FramePost
        | SolidRole::FramePlate
        | SolidRole::FrameRail
        | SolidRole::FrameTie
        | SolidRole::FrameBrace
        | SolidRole::FrameJettyBeam
        | SolidRole::FrameKnagge
        | SolidRole::FrameGableMember
        | SolidRole::FrameDormerTrimmer
        | SolidRole::FrameOrnament
        | SolidRole::OpeningClosure
        | SolidRole::ChurchStairNewel
        | SolidRole::ChurchServiceLadder
        | SolidRole::ArtilleryBridgeBeam
        | SolidRole::ArtilleryBridgeDeck
        | SolidRole::ArtilleryGateMechanism => BuildingLodMaterial::Timber,
        SolidRole::FrameJoist
        | SolidRole::FrameGirder
        | SolidRole::BeamJoist
        | SolidRole::RoofFraming
        | SolidRole::RoofPlate => BuildingLodMaterial::InteriorTimber,
        SolidRole::LeadedGlazing => BuildingLodMaterial::Glass,
        SolidRole::FrameFloor
        | SolidRole::WalkSurface
        | SolidRole::DrainageChannel
        | SolidRole::DrainageFloor
        | SolidRole::GalleryFloor
        | SolidRole::Landing
        | SolidRole::CircuitWalk
        | SolidRole::ChurchFloor
        | SolidRole::ChurchBellFloor
        | SolidRole::ChurchVaultShell => BuildingLodMaterial::Floor,
        SolidRole::RoofFlashing
        | SolidRole::DefenseRoof
        | SolidRole::RoofEdgeTreatment
        | SolidRole::RoofGutter => BuildingLodMaterial::Roof(RoofMaterial::ClayTile),
        SolidRole::FrameInfill => BuildingLodMaterial::Wall(infill_material),
        SolidRole::OpeningJamb | SolidRole::OpeningHead
            if wall_material == Some(WallMaterialClass::InternalTimber) =>
        {
            BuildingLodMaterial::Timber
        }
        SolidRole::WallHost
        | SolidRole::OpeningJamb
        | SolidRole::OpeningSill
        | SolidRole::OpeningHead
        | SolidRole::OpeningSpandrel
        | SolidRole::OpeningReveal => {
            BuildingLodMaterial::Wall(wall_material.unwrap_or(WallMaterialClass::FortifiedMasonry))
        }
        _ => BuildingLodMaterial::Wall(WallMaterialClass::FortifiedMasonry),
    }
}

fn append_roofs(detail: &mut BuildingDetail, plan: &BuildingPlan) {
    for roof in &plan.roof_assemblies {
        for face in &roof.faces {
            for triangle in tessellate_roof_face(face) {
                let mesh = detail.mesh_mut(roof_surface_material(face.material, triangle.surface));
                mesh.push_triangle(
                    triangle.positions,
                    triangle.normal,
                    triangle.positions.map(|point| {
                        Vec2::new(point.x, point.z) / BUILDING_DETAIL_UV_METRES_PER_UNIT
                    }),
                );
            }
        }
        for enclosure in &roof.enclosure_faces {
            for triangle in tessellate_roof_enclosure(enclosure) {
                let mesh =
                    detail.mesh_mut(roof_surface_material(enclosure.material, triangle.surface));
                mesh.push_triangle(
                    triangle.positions,
                    triangle.normal,
                    triangle.positions.map(|point| {
                        Vec2::new(point.x, point.z) / BUILDING_DETAIL_UV_METRES_PER_UNIT
                    }),
                );
            }
        }
    }
}

fn roof_surface_material(exterior: RoofMaterial, surface: RoofSurface) -> BuildingLodMaterial {
    match surface {
        RoofSurface::Interior => BuildingLodMaterial::InteriorTimber,
        RoofSurface::Weather | RoofSurface::Boundary | RoofSurface::Enclosure => {
            BuildingLodMaterial::Roof(exterior)
        }
    }
}

fn append_oriented_cuboid(
    detail: &mut BuildingDetail,
    material: BuildingLodMaterial,
    solid: &ResolvedSolid,
    wall: Option<&crate::WallAssembly>,
) {
    let fachwerk_member = is_fachwerk_member_role(solid.role);
    let resolved_yaw = if matches!(
        solid.role,
        SolidRole::RoofFraming
            | SolidRole::RoofFlashing
            | SolidRole::RoofGutter
            | SolidRole::RoofEdgeTreatment
    ) {
        -solid.yaw_radians
    } else {
        solid.yaw_radians
    };
    let rotation = Quat::from_rotation_y(resolved_yaw)
        * Quat::from_rotation_x(solid.crossfall_radians)
        * Quat::from_rotation_z(solid.longfall_radians);
    let (render_centre, render_size) =
        render_cuboid_placement(solid, wall, fachwerk_member, rotation);
    append_cuboid_faces(detail, material, render_centre, render_size, rotation, wall);
}

fn is_fachwerk_member_role(role: SolidRole) -> bool {
    matches!(
        role,
        SolidRole::FrameMember
            | SolidRole::FrameSill
            | SolidRole::FramePost
            | SolidRole::FramePlate
            | SolidRole::FrameRail
            | SolidRole::FrameTie
            | SolidRole::FrameBrace
            | SolidRole::FrameJettyBeam
            | SolidRole::FrameKnagge
            | SolidRole::FrameGableMember
            | SolidRole::FrameDormerTrimmer
            | SolidRole::FrameOrnament
    )
}

fn render_cuboid_placement(
    solid: &ResolvedSolid,
    wall: Option<&crate::WallAssembly>,
    fachwerk_member: bool,
    rotation: Quat,
) -> (Vec3, Vec3) {
    let mut render_size = if fachwerk_member {
        solid.size + Vec3::splat(TIMBER_SEAM_COVER_METRES * 2.0)
    } else {
        solid.size
    };
    let mut render_centre = solid.centre;
    if let Some(wall) =
        wall.filter(|wall| fachwerk_member && wall.material == WallMaterialClass::TimberInfill)
    {
        let outward = Vec3::new(wall.frame.outward.x, 0.0, wall.frame.outward.y);
        let local_axes = [rotation * Vec3::X, rotation * Vec3::Y, rotation * Vec3::Z];
        let projected_half_extent = local_axes
            .into_iter()
            .zip(render_size.to_array())
            .map(|(axis, extent)| axis.dot(outward).abs() * extent)
            .sum::<f32>()
            * 0.5;
        let inner_plane = wall.frame.origin.dot(wall.frame.outward) - wall.thickness_metres * 0.5;
        let current_inner_extent = render_centre.dot(outward) - projected_half_extent;
        let missing_depth =
            (current_inner_extent - (inner_plane - TIMBER_SEAM_COVER_METRES)).max(0.0);

        if missing_depth > 0.0 {
            let x_alignment = local_axes[0].dot(outward).abs();
            let z_alignment = local_axes[2].dot(outward).abs();
            let (depth_axis, alignment) = if x_alignment >= z_alignment {
                (0, x_alignment)
            } else {
                (2, z_alignment)
            };
            if alignment > f32::EPSILON {
                render_size[depth_axis] += missing_depth / alignment;
                render_centre -= outward * missing_depth * 0.5;
            }
        }
    }
    (render_centre, render_size)
}

fn append_cuboid_faces(
    detail: &mut BuildingDetail,
    material: BuildingLodMaterial,
    centre: Vec3,
    size: Vec3,
    rotation: Quat,
    wall: Option<&crate::WallAssembly>,
) {
    let half = size * 0.5;
    let local = [
        Vec3::new(-half.x, -half.y, -half.z),
        Vec3::new(half.x, -half.y, -half.z),
        Vec3::new(half.x, half.y, -half.z),
        Vec3::new(-half.x, half.y, -half.z),
        Vec3::new(-half.x, -half.y, half.z),
        Vec3::new(half.x, -half.y, half.z),
        Vec3::new(half.x, half.y, half.z),
        Vec3::new(-half.x, half.y, half.z),
    ];
    let point = |index: usize| centre + rotation * local[index];
    for (indices, normal, u_axis, v_axis) in [
        ([0, 3, 2, 1], -Vec3::Z, Vec3::X, Vec3::Y),
        ([4, 5, 6, 7], Vec3::Z, Vec3::X, Vec3::Y),
        ([0, 4, 7, 3], -Vec3::X, Vec3::Z, Vec3::Y),
        ([1, 2, 6, 5], Vec3::X, Vec3::Z, Vec3::Y),
        ([0, 1, 5, 4], -Vec3::Y, Vec3::X, Vec3::Z),
        ([3, 7, 6, 2], Vec3::Y, Vec3::X, Vec3::Z),
    ] {
        let positions = indices.map(point);
        let world_normal = rotation * normal;
        let wall_uvs = wall.filter(|wall| {
            let outward = Vec3::new(wall.frame.outward.x, 0.0, wall.frame.outward.y);
            world_normal.dot(outward).abs() > 0.99
        });
        detail
            .mesh_mut(interior_face_material(material, wall, world_normal))
            .push_quad(
                positions,
                world_normal,
                if let Some(wall) = wall_uvs {
                    positions.map(|position| wall_surface_uv(wall, position))
                } else {
                    indices.map(|index| {
                        Vec2::new(local[index].dot(u_axis), local[index].dot(v_axis))
                            / BUILDING_DETAIL_UV_METRES_PER_UNIT
                    })
                },
            );
    }
}

fn interior_face_material(
    material: BuildingLodMaterial,
    wall: Option<&crate::WallAssembly>,
    face_normal: Vec3,
) -> BuildingLodMaterial {
    let Some(wall) = wall else {
        return material;
    };
    let outward = Vec3::new(wall.frame.outward.x, 0.0, wall.frame.outward.y);
    if matches!(material, BuildingLodMaterial::Wall(_))
        && wall.frame.inside_room.is_some()
        && wall.frame.outside_room.is_none()
        && face_normal.dot(outward) < -0.99
    {
        BuildingLodMaterial::InteriorPlaster
    } else {
        material
    }
}

fn wall_surface_uv(wall: &crate::WallAssembly, point: Vec3) -> Vec2 {
    let tangent = canonical_wall_texture_tangent(wall.frame.tangent);
    Vec2::new(Vec2::new(point.x, point.z).dot(tangent), point.y)
        / BUILDING_DETAIL_UV_METRES_PER_UNIT
}

fn canonical_wall_texture_tangent(tangent: Vec2) -> Vec2 {
    if tangent.x < -f32::EPSILON || (tangent.x.abs() <= f32::EPSILON && tangent.y < 0.0) {
        -tangent
    } else {
        tangent
    }
}

type PanelEdgeKey = ([i64; 3], [i64; 3]);

fn panel_edge_key(first: Vec3, second: Vec3) -> PanelEdgeKey {
    let quantize = |point: Vec3| {
        [
            (point.x * 100_000.0).round() as i64,
            (point.y * 100_000.0).round() as i64,
            (point.z * 100_000.0).round() as i64,
        ]
    };
    let first = quantize(first);
    let second = quantize(second);
    if first <= second {
        (first, second)
    } else {
        (second, first)
    }
}

fn panel_boundary_edges(plan: &BuildingPlan) -> BTreeSet<(crate::WallAssemblyId, PanelEdgeKey)> {
    let mut counts = BTreeMap::new();
    for wall in &plan.wall_assemblies {
        for solid in plan
            .resolved_geometry
            .solids
            .iter()
            .filter(|solid| wall.host_solids.contains(&solid.id))
        {
            let ResolvedSolidShape::TimberPanelPrism { vertices, .. } = solid.shape else {
                continue;
            };
            for edge in 0..3 {
                *counts
                    .entry((
                        wall.id,
                        panel_edge_key(vertices[edge], vertices[(edge + 1) % 3]),
                    ))
                    .or_insert(0_u8) += 1;
            }
        }
    }
    counts
        .into_iter()
        .filter_map(|(edge, count)| (count == 1).then_some(edge))
        .collect()
}

fn append_timber_panel(
    detail: &mut BuildingDetail,
    material: BuildingLodMaterial,
    vertices: [Vec3; 3],
    outward: Vec2,
    depth_metres: f32,
    wall: Option<&crate::WallAssembly>,
    boundary_edges: [bool; 3],
) {
    let normal = Vec3::new(outward.x, 0.0, outward.y).normalize_or_zero();
    let offset = normal * depth_metres * 0.5;
    let front = vertices.map(|point| point + offset);
    let back = vertices.map(|point| point - offset);
    let tangent = Vec3::new(-normal.z, 0.0, normal.x);
    let uv = |point: Vec3| {
        wall.map_or_else(
            || Vec2::new(point.dot(tangent), point.y) / BUILDING_DETAIL_UV_METRES_PER_UNIT,
            |wall| wall_surface_uv(wall, point),
        )
    };
    detail
        .mesh_mut(material)
        .push_triangle(front, normal, front.map(uv));
    let back_positions = [back[2], back[1], back[0]];
    detail
        .mesh_mut(interior_face_material(material, wall, -normal))
        .push_triangle(back_positions, -normal, back_positions.map(uv));
    for edge in 0..3 {
        if !boundary_edges[edge] {
            continue;
        }
        let next = (edge + 1) % 3;
        let positions = [front[edge], back[edge], back[next], front[next]];
        let side_normal = (back[edge] - front[edge])
            .cross(front[next] - front[edge])
            .normalize_or_zero();
        let edge_length = front[edge].distance(front[next]);
        detail.mesh_mut(material).push_quad(
            positions,
            side_normal,
            [
                Vec2::ZERO,
                Vec2::new(depth_metres, 0.0),
                Vec2::new(depth_metres, edge_length),
                Vec2::new(0.0, edge_length),
            ]
            .map(|uv| uv / BUILDING_DETAIL_UV_METRES_PER_UNIT),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuildingArchetype, BuildingProgram, CELL_SIZE_METRES, Direction, generate};

    #[derive(Clone, Copy)]
    struct AuditTriangle {
        points: [Vec3; 3],
        normal: Vec3,
    }

    fn projected(point: Vec3, dominant_axis: usize) -> Vec2 {
        match dominant_axis {
            0 => Vec2::new(point.y, point.z),
            1 => Vec2::new(point.x, point.z),
            _ => Vec2::new(point.x, point.y),
        }
    }

    fn projection_interval(points: [Vec2; 3], axis: Vec2) -> (f32, f32) {
        points
            .map(|point| point.dot(axis))
            .into_iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), value| {
                (min.min(value), max.max(value))
            })
    }

    fn triangles_overlap_with_area(left: [Vec2; 3], right: [Vec2; 3]) -> bool {
        [left, right].into_iter().all(|triangle| {
            (0..3).all(|index| {
                let start = triangle[index];
                let end = triangle[(index + 1) % 3];
                let edge = end - start;
                let axis = Vec2::new(-edge.y, edge.x).normalize_or_zero();
                let (left_min, left_max) = projection_interval(left, axis);
                let (right_min, right_max) = projection_interval(right, axis);
                left_max.min(right_max) - left_min.max(right_min) > 0.0001
            })
        })
    }

    fn coplanar_overlap_count(detail: &BuildingDetail) -> usize {
        let triangles = detail
            .meshes
            .iter()
            .filter(|mesh| matches!(mesh.material, BuildingLodMaterial::Wall(_)))
            .flat_map(|mesh| {
                mesh.indices
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .filter_map(|indices| {
                        let normal = mesh.vertices[indices[0] as usize]
                            .normal
                            .normalize_or_zero();
                        (normal.y.abs() < 0.5).then_some(AuditTriangle {
                            points: [
                                mesh.vertices[indices[0] as usize].position,
                                mesh.vertices[indices[1] as usize].position,
                                mesh.vertices[indices[2] as usize].position,
                            ],
                            normal,
                        })
                    })
            })
            .collect::<Vec<_>>();

        let mut overlaps = 0;
        for (index, left) in triangles.iter().enumerate() {
            for right in &triangles[index + 1..] {
                let plane_offset = left.normal.dot(left.points[0]);
                if left.normal.dot(right.normal) < 0.999_999
                    || right
                        .points
                        .iter()
                        .any(|point| (left.normal.dot(*point) - plane_offset).abs() > 0.0001)
                {
                    continue;
                }
                let absolute_normal = left.normal.abs();
                let dominant_axis = if absolute_normal.x >= absolute_normal.y
                    && absolute_normal.x >= absolute_normal.z
                {
                    0
                } else if absolute_normal.y >= absolute_normal.z {
                    1
                } else {
                    2
                };
                if triangles_overlap_with_area(
                    left.points.map(|point| projected(point, dominant_axis)),
                    right.points.map(|point| projected(point, dominant_axis)),
                ) {
                    overlaps += 1;
                }
            }
        }
        overlaps
    }

    #[test]
    fn playable_detail_contains_interior_wall_material() {
        let plan = generate(&BuildingProgram::fixture(
            BuildingArchetype::FachwerkMerchantHouse,
            42,
        ))
        .unwrap();
        let detail = compile_building_detail(&plan);

        assert!(detail.meshes.iter().any(|mesh| {
            matches!(
                mesh.material,
                BuildingLodMaterial::Wall(
                    WallMaterialClass::InternalTimber | WallMaterialClass::InternalMasonry
                )
            )
        }));
        assert!(
            detail
                .meshes
                .iter()
                .any(|mesh| mesh.material == BuildingLodMaterial::InteriorPlaster)
        );
        assert!(
            detail
                .meshes
                .iter()
                .any(|mesh| mesh.material == BuildingLodMaterial::Timber)
        );
        assert!(
            detail
                .meshes
                .iter()
                .any(|mesh| mesh.material == BuildingLodMaterial::InteriorTimber)
        );
        assert!(
            detail
                .meshes
                .iter()
                .any(|mesh| mesh.material == BuildingLodMaterial::Floor)
        );
    }

    #[test]
    fn settlement_internal_timber_partitions_are_framed_connected_and_ceiling_clipped() {
        let archetypes = [
            BuildingArchetype::TownHouse,
            BuildingArchetype::HallHouse,
            BuildingArchetype::FachwerkCottage,
            BuildingArchetype::FachwerkMerchantHouse,
        ];
        for archetype in archetypes {
            for seed in [42_u64, 47, 101] {
                let plan = generate(&BuildingProgram::fixture(archetype, seed)).unwrap();
                for wall in plan
                    .wall_assemblies
                    .iter()
                    .filter(|wall| wall.material == WallMaterialClass::InternalTimber)
                {
                    let solids = wall
                        .host_solids
                        .iter()
                        .map(|id| {
                            plan.resolved_geometry
                                .solids
                                .iter()
                                .find(|solid| solid.id == *id)
                                .expect("partition host solid is resolved")
                        })
                        .collect::<Vec<_>>();
                    assert!(
                        solids.iter().all(|solid| solid.role != SolidRole::WallHost),
                        "{archetype:?} retained a featureless InternalTimber wall host"
                    );
                    assert!(
                        solids.iter().any(|solid| matches!(
                            solid.role,
                            SolidRole::FrameSill
                                | SolidRole::FramePost
                                | SolidRole::FramePlate
                                | SolidRole::FrameRail
                                | SolidRole::OpeningJamb
                                | SolidRole::OpeningHead
                        )),
                        "{archetype:?} partition has no semantic timber frame"
                    );
                    let wall_top = wall.base_elevation_metres + wall.height_metres;
                    let maximum_solid_top = solids
                        .iter()
                        .map(|solid| solid.centre.y + solid.size.y * 0.5)
                        .fold(f32::NEG_INFINITY, f32::max);
                    assert!(
                        (maximum_solid_top - wall_top).abs() <= 0.001,
                        "{archetype:?} partition top {maximum_solid_top} steps away from ceiling {wall_top}"
                    );

                    if wall.opening_ids.is_empty() {
                        assert!(
                            solids
                                .iter()
                                .any(|solid| solid.role == SolidRole::FrameInfill),
                            "{archetype:?} closed partition has no plaster/daub infill"
                        );
                        assert!(
                            solids
                                .iter()
                                .filter(|solid| { solid.role == SolidRole::FrameInfill })
                                .all(|solid| {
                                    let depth = if wall.frame.tangent.x.abs() > 0.5 {
                                        solid.size.z
                                    } else {
                                        solid.size.x
                                    };
                                    depth < wall.thickness_metres
                                })
                        );
                    } else {
                        for opening in plan
                            .opening_assemblies
                            .iter()
                            .filter(|opening| opening.host_wall == wall.id)
                        {
                            for id in [
                                opening.jamb_solids[0],
                                opening.jamb_solids[1],
                                opening.head_solid,
                            ] {
                                let solid = plan
                                    .resolved_geometry
                                    .solids
                                    .iter()
                                    .find(|solid| solid.id == id)
                                    .unwrap();
                                assert_eq!(
                                    material_for_solid(&plan, solid),
                                    BuildingLodMaterial::Timber
                                );
                            }
                        }
                    }

                    let crate::WallSourceId::StoreyWall {
                        storey_level,
                        wall_index,
                    } = wall.source
                    else {
                        panic!("settlement partition uses a storey-wall source")
                    };
                    let storey = &plan.storeys[usize::from(storey_level)];
                    let source = storey.walls[wall_index];
                    let tangent = if source.is_horizontal() {
                        Vec2::X
                    } else {
                        Vec2::Y
                    };
                    for sign in [-1.0_f32, 1.0] {
                        let endpoint = source.centre() + tangent * sign * CELL_SIZE_METRES * 0.5;
                        let has_post = plan.resolved_geometry.solids.iter().any(|solid| {
                            solid.role == SolidRole::FramePost
                                && Vec2::new(solid.centre.x, solid.centre.z).distance(endpoint)
                                    <= 0.001
                        });
                        let has_perpendicular_or_exterior = storey.walls.iter().any(|other| {
                            let other_tangent = if other.is_horizontal() {
                                Vec2::X
                            } else {
                                Vec2::Y
                            };
                            let delta = endpoint - other.centre();
                            let outward = match other.direction {
                                Direction::North => Vec2::Y,
                                Direction::East => Vec2::X,
                                Direction::South => -Vec2::Y,
                                Direction::West => -Vec2::X,
                            };
                            let on_wall = delta.dot(outward).abs() <= 0.001
                                && delta.dot(other_tangent).abs() <= CELL_SIZE_METRES * 0.5 + 0.001;
                            on_wall
                                && (other.exterior()
                                    || other.is_horizontal() != source.is_horizontal())
                        });
                        let has_opening_jamb = plan.opening_assemblies.iter().any(|opening| {
                            let delta = endpoint - opening.frame.origin;
                            delta.dot(opening.frame.outward).abs() <= 0.001
                                && delta.dot(opening.frame.tangent).abs()
                                    <= CELL_SIZE_METRES * 0.5 + 0.001
                        });
                        assert!(
                            has_post || has_perpendicular_or_exterior || has_opening_jamb,
                            "{archetype:?} partition endpoint {endpoint:?} has no structural termination"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn exact_roof_routes_only_room_facing_slopes_to_interior_timber() {
        let plan = generate(&BuildingProgram::fixture(
            BuildingArchetype::FachwerkMerchantHouse,
            42,
        ))
        .unwrap();
        let mut inward_triangles = 0;
        for face in plan
            .roof_assemblies
            .iter()
            .flat_map(|roof| roof.faces.iter())
        {
            for triangle in tessellate_roof_face(face) {
                let material = roof_surface_material(face.material, triangle.surface);
                match triangle.surface {
                    RoofSurface::Interior => {
                        assert_eq!(material, BuildingLodMaterial::InteriorTimber);
                        inward_triangles += 1;
                    }
                    RoofSurface::Weather | RoofSurface::Boundary => {
                        assert_eq!(material, BuildingLodMaterial::Roof(face.material));
                    }
                    RoofSurface::Enclosure => unreachable!("weather faces are not enclosures"),
                }
            }
        }
        assert!(inward_triangles > 0);
    }

    #[test]
    fn static_playable_detail_reserves_operable_leaves_for_dynamic_entities() {
        let plan = generate(&BuildingProgram::fixture(BuildingArchetype::TownHouse, 42)).unwrap();
        let operable_doors = compile_operable_doors(&plan);
        let operable_windows = compile_operable_windows(&plan);
        assert!(!operable_windows.is_empty());
        let self_contained = compile_building_detail(&plan);
        let static_detail = compile_static_building_detail(&plan);
        let triangle_count = |detail: &BuildingDetail| {
            detail
                .meshes
                .iter()
                .map(|mesh| mesh.indices.len() / 3)
                .sum::<usize>()
        };

        assert_eq!(
            triangle_count(&self_contained) - triangle_count(&static_detail),
            (operable_doors.len() + operable_windows.len()) * 12
        );
    }

    #[test]
    fn settlement_archetypes_use_exactly_supported_resolved_shapes() {
        for archetype in [
            BuildingArchetype::FachwerkCottage,
            BuildingArchetype::TownHouse,
            BuildingArchetype::FachwerkMerchantHouse,
        ] {
            for seed in [42, 47, 101] {
                let plan = generate(&BuildingProgram::fixture(archetype, seed)).unwrap();
                assert!(plan.resolved_geometry.solids.iter().all(|solid| matches!(
                    solid.shape,
                    ResolvedSolidShape::Cuboid | ResolvedSolidShape::TimberPanelPrism { .. }
                )));
            }
        }
    }

    #[test]
    fn resolved_floor_mesh_does_not_reintroduce_tiles_over_stair_cuts() {
        let plan = generate(&BuildingProgram::fixture(
            BuildingArchetype::FachwerkMerchantHouse,
            42,
        ))
        .unwrap();
        let detail = compile_building_detail(&plan);
        let expected_cuboids = plan
            .resolved_geometry
            .solids
            .iter()
            .filter(|solid| material_for_solid(&plan, solid) == BuildingLodMaterial::Floor)
            .count();
        let floor_vertices = detail
            .meshes
            .iter()
            .filter(|mesh| mesh.material == BuildingLodMaterial::Floor)
            .map(|mesh| mesh.vertices.len())
            .sum::<usize>();

        assert!(
            !plan
                .timber_frame
                .as_ref()
                .unwrap()
                .circulation
                .floor_cut_voids
                .is_empty()
        );
        assert_eq!(floor_vertices, expected_cuboids * 24);
    }

    #[test]
    fn exact_wall_mesh_has_no_positive_area_coplanar_overlaps() {
        for archetype in [
            BuildingArchetype::FachwerkCottage,
            BuildingArchetype::TownHouse,
            BuildingArchetype::FachwerkMerchantHouse,
        ] {
            let plan = generate(&BuildingProgram::fixture(archetype, 42)).unwrap();
            let detail = compile_building_detail(&plan);
            assert_eq!(
                coplanar_overlap_count(&detail),
                0,
                "{archetype:?} compiled co-facing wall triangles overlap"
            );
        }
    }

    #[test]
    fn exact_wall_piece_uvs_are_non_degenerate_and_metric() {
        let expected_area_ratio = BUILDING_DETAIL_UV_METRES_PER_UNIT.powi(2);
        for archetype in [
            BuildingArchetype::FachwerkCottage,
            BuildingArchetype::TownHouse,
            BuildingArchetype::FachwerkMerchantHouse,
        ] {
            let plan = generate(&BuildingProgram::fixture(archetype, 42)).unwrap();
            let detail = compile_building_detail(&plan);
            for mesh in detail.meshes.iter().filter(|mesh| {
                matches!(
                    mesh.material,
                    BuildingLodMaterial::Wall(_) | BuildingLodMaterial::InteriorPlaster
                )
            }) {
                for triangle in mesh.indices.as_chunks::<3>().0 {
                    let vertices = triangle.map(|index| mesh.vertices[index as usize]);
                    let geometric_double_area = (vertices[1].position - vertices[0].position)
                        .cross(vertices[2].position - vertices[0].position)
                        .length();
                    let first_uv_edge = vertices[1].uv - vertices[0].uv;
                    let second_uv_edge = vertices[2].uv - vertices[0].uv;
                    let uv_double_area = first_uv_edge.perp_dot(second_uv_edge).abs();

                    assert!(
                        uv_double_area > 1.0e-7,
                        "{archetype:?} wall triangle has collapsed UVs: {vertices:?}"
                    );
                    let area_ratio = geometric_double_area / uv_double_area;
                    assert!(
                        (area_ratio - expected_area_ratio).abs() < 0.002,
                        "{archetype:?} wall triangle stretches texture: ratio {area_ratio}, expected {expected_area_ratio}"
                    );
                }
            }
        }
    }

    #[test]
    fn exterior_wall_back_faces_keep_positions_and_wall_local_uvs_paired() {
        let plan = generate(&BuildingProgram::fixture(
            BuildingArchetype::FachwerkMerchantHouse,
            42,
        ))
        .unwrap();
        let detail = compile_building_detail(&plan);
        let plaster = detail
            .meshes
            .iter()
            .find(|mesh| mesh.material == BuildingLodMaterial::InteriorPlaster)
            .expect("merchant house has room-facing exterior plaster");

        for vertex in &plaster.vertices {
            assert!(
                plan.wall_assemblies.iter().any(|wall| {
                    wall.frame.inside_room.is_some()
                        && wall.frame.outside_room.is_none()
                        && vertex.normal.dot(Vec3::new(
                            wall.frame.outward.x,
                            0.0,
                            wall.frame.outward.y,
                        )) < -0.99
                        && vertex
                            .uv
                            .abs_diff_eq(wall_surface_uv(wall, vertex.position), 0.000_01)
                }),
                "interior plaster vertex lost its wall-local UV pairing: {vertex:?}"
            );
        }
    }

    #[test]
    fn coplanar_wall_assemblies_share_plaster_tile_phase_at_edges() {
        let plan = generate(&BuildingProgram::fixture(
            BuildingArchetype::FachwerkMerchantHouse,
            42,
        ))
        .unwrap();
        let walls = plan
            .wall_assemblies
            .iter()
            .filter(|wall| wall.material == WallMaterialClass::TimberInfill)
            .collect::<Vec<_>>();
        let mut shared_edges = 0;
        for (index, left) in walls.iter().enumerate() {
            for right in &walls[index + 1..] {
                if left.frame.outward.dot(right.frame.outward).abs() < 0.999
                    || (left.base_elevation_metres - right.base_elevation_metres).abs() > 0.001
                {
                    continue;
                }
                let left_ends = [
                    left.frame.origin - left.frame.tangent * left.length_metres * 0.5,
                    left.frame.origin + left.frame.tangent * left.length_metres * 0.5,
                ];
                let right_ends = [
                    right.frame.origin - right.frame.tangent * right.length_metres * 0.5,
                    right.frame.origin + right.frame.tangent * right.length_metres * 0.5,
                ];
                let Some(shared) = left_ends.into_iter().find(|left_end| {
                    right_ends
                        .iter()
                        .any(|right_end| left_end.distance(*right_end) < 0.001)
                }) else {
                    continue;
                };
                let point = Vec3::new(shared.x, left.base_elevation_metres + 1.37, shared.y);
                let left_uv = wall_surface_uv(left, point);
                let right_uv = wall_surface_uv(right, point);
                let left_phase = left_uv.map(|component| component.rem_euclid(1.0));
                let right_phase = right_uv.map(|component| component.rem_euclid(1.0));
                assert!(
                    left_phase.abs_diff_eq(right_phase, 0.000_01),
                    "walls {:?} and {:?} disagree at {point:?}: {left_phase:?} vs {right_phase:?}",
                    left.id,
                    right.id
                );
                shared_edges += 1;
            }
        }
        assert!(
            shared_edges >= 4,
            "only {shared_edges} coplanar shared edges found"
        );
    }

    #[test]
    fn rendered_fachwerk_members_seal_the_full_wall_depth() {
        let plan = generate(&BuildingProgram::fixture(
            BuildingArchetype::FachwerkMerchantHouse,
            42,
        ))
        .unwrap();
        let frame = plan.timber_frame.as_ref().unwrap();
        let mut checked = 0;

        for member in &frame.members {
            let solid = plan
                .resolved_geometry
                .solids
                .iter()
                .find(|solid| solid.id == member.solid)
                .unwrap();
            if !is_fachwerk_member_role(solid.role) {
                continue;
            }
            let Some(wall) = wall_for_solid(&plan, solid)
                .filter(|wall| wall.material == WallMaterialClass::TimberInfill)
            else {
                continue;
            };
            let outward = Vec3::new(wall.frame.outward.x, 0.0, wall.frame.outward.y);
            let inner_plane =
                wall.frame.origin.dot(wall.frame.outward) - wall.thickness_metres * 0.5;
            let rotation = Quat::from_rotation_y(solid.yaw_radians)
                * Quat::from_rotation_x(solid.crossfall_radians)
                * Quat::from_rotation_z(solid.longfall_radians);
            let (centre, size) = render_cuboid_placement(solid, Some(wall), true, rotation);
            let projected_half_extent = [
                (rotation * Vec3::X).dot(outward).abs() * size.x,
                (rotation * Vec3::Y).dot(outward).abs() * size.y,
                (rotation * Vec3::Z).dot(outward).abs() * size.z,
            ]
            .into_iter()
            .sum::<f32>()
                * 0.5;

            assert!(
                centre.dot(outward) - projected_half_extent
                    <= inner_plane + TIMBER_SEAM_COVER_METRES,
                "fachwerk member {:?} ({:?}) stops before wall {:?}'s interior plane: centre={centre:?}, size={size:?}, yaw={}, outward={outward:?}, inner_extent={}, inner_plane={inner_plane}",
                member.id,
                solid.role,
                wall.id,
                solid.yaw_radians,
                centre.dot(outward) - projected_half_extent,
            );
            checked += 1;
        }
        assert!(checked > 20);
    }
}
