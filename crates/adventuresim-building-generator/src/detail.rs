//! Exact render meshes derived from accepted resolved building geometry.
//!
//! Unlike the facade and shell LODs, this representation preserves interior
//! walls, opening assemblies, timber members, floors, and roof framing. It is
//! render-only; tactical collision remains independently compiled.

use std::collections::BTreeSet;

use bevy::math::{Quat, Vec2, Vec3};

use crate::{
    BuildingLodMaterial, BuildingPlan, LodMesh, ResolvedSolid, ResolvedSolidShape, RoofMaterial,
    SolidRole, WallMaterialClass, WallStyle, compile_operable_doors, tessellate_roof_enclosure,
    tessellate_roof_face,
};

const TEXTURE_REPEAT_METRES: f32 = 2.0;
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
    let dynamic_door_solids = compile_operable_doors(plan)
        .into_iter()
        .map(|door| door.source)
        .collect::<BTreeSet<_>>();
    compile_detail(plan, &dynamic_door_solids)
}

fn compile_detail(
    plan: &BuildingPlan,
    excluded_solids: &BTreeSet<crate::ResolvedItemId>,
) -> BuildingDetail {
    let mut detail = BuildingDetail { meshes: Vec::new() };

    for solid in &plan.resolved_geometry.solids {
        if excluded_solids.contains(&solid.id) {
            continue;
        }
        if matches!(solid.shape, ResolvedSolidShape::RoundTowerShell { .. }) {
            continue;
        }
        let material = material_for_solid(plan, solid);
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
            } => append_timber_panel(detail.mesh_mut(material), vertices, outward, depth_metres),
            _ => append_oriented_cuboid(detail.mesh_mut(material), solid),
        }
    }
    append_roofs(&mut detail, plan);
    detail
        .meshes
        .retain(|mesh| !mesh.vertices.is_empty() && !mesh.indices.is_empty());
    detail
}

fn material_for_solid(plan: &BuildingPlan, solid: &ResolvedSolid) -> BuildingLodMaterial {
    let wall_material = plan
        .wall_assemblies
        .iter()
        .find(|wall| {
            wall.host_solids.contains(&solid.id)
                || wall.owner == solid.owner
                || wall.replaced_by_owner == Some(solid.owner)
        })
        .map(|wall| wall.material);
    match solid.role {
        SolidRole::EdgeGuard
        | SolidRole::FrameMember
        | SolidRole::FrameSill
        | SolidRole::FramePost
        | SolidRole::FramePlate
        | SolidRole::FrameRail
        | SolidRole::FrameJoist
        | SolidRole::FrameGirder
        | SolidRole::FrameTie
        | SolidRole::FrameBrace
        | SolidRole::FrameJettyBeam
        | SolidRole::FrameKnagge
        | SolidRole::FrameGableMember
        | SolidRole::FrameDormerTrimmer
        | SolidRole::FrameOrnament
        | SolidRole::BeamJoist
        | SolidRole::RoofFraming
        | SolidRole::RoofPlate
        | SolidRole::OpeningClosure
        | SolidRole::ChurchStairNewel
        | SolidRole::ChurchServiceLadder
        | SolidRole::ArtilleryBridgeBeam
        | SolidRole::ArtilleryBridgeDeck
        | SolidRole::ArtilleryGateMechanism => BuildingLodMaterial::Timber,
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
        SolidRole::FrameInfill => BuildingLodMaterial::Wall(match plan.wall_style {
            WallStyle::Brick => WallMaterialClass::CivilianMasonry,
            WallStyle::Stone => WallMaterialClass::FortifiedMasonry,
            WallStyle::TimberFrame | WallStyle::Plaster => WallMaterialClass::TimberInfill,
        }),
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
            let mesh = detail.mesh_mut(BuildingLodMaterial::Roof(face.material));
            for triangle in tessellate_roof_face(face) {
                mesh.push_triangle(
                    triangle.positions,
                    triangle.normal,
                    triangle
                        .positions
                        .map(|point| Vec2::new(point.x, point.z) / TEXTURE_REPEAT_METRES),
                );
            }
        }
        for enclosure in &roof.enclosure_faces {
            let mesh = detail.mesh_mut(BuildingLodMaterial::Roof(enclosure.material));
            for triangle in tessellate_roof_enclosure(enclosure) {
                mesh.push_triangle(
                    triangle.positions,
                    triangle.normal,
                    triangle
                        .positions
                        .map(|point| Vec2::new(point.x, point.z) / TEXTURE_REPEAT_METRES),
                );
            }
        }
    }
}

fn append_oriented_cuboid(mesh: &mut LodMesh, solid: &ResolvedSolid) {
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
    let render_size = if matches!(
        solid.role,
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
    ) {
        solid.size + Vec3::splat(TIMBER_SEAM_COVER_METRES * 2.0)
    } else {
        solid.size
    };
    append_cuboid(
        mesh,
        solid.centre,
        render_size,
        Quat::from_rotation_y(resolved_yaw)
            * Quat::from_rotation_x(solid.crossfall_radians)
            * Quat::from_rotation_z(solid.longfall_radians),
    );
}

fn append_cuboid(mesh: &mut LodMesh, centre: Vec3, size: Vec3, rotation: Quat) {
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
        let world_u = rotation * u_axis;
        let world_v = rotation * v_axis;
        mesh.push_quad(
            positions,
            rotation * normal,
            positions.map(|position| {
                Vec2::new(position.dot(world_u), position.dot(world_v)) / TEXTURE_REPEAT_METRES
            }),
        );
    }
}

fn append_timber_panel(mesh: &mut LodMesh, vertices: [Vec3; 3], outward: Vec2, depth_metres: f32) {
    let normal = Vec3::new(outward.x, 0.0, outward.y).normalize_or_zero();
    let offset = normal * depth_metres * 0.5;
    let front = vertices.map(|point| point + offset);
    let back = vertices.map(|point| point - offset);
    let tangent = Vec3::new(-normal.z, 0.0, normal.x);
    let uv = |point: Vec3| Vec2::new(point.dot(tangent), point.y) / TEXTURE_REPEAT_METRES;
    mesh.push_triangle(front, normal, front.map(uv));
    mesh.push_triangle([back[2], back[1], back[0]], -normal, back.map(uv));
    for edge in 0..3 {
        let next = (edge + 1) % 3;
        let positions = [front[edge], back[edge], back[next], front[next]];
        let side_normal = (back[edge] - front[edge])
            .cross(front[next] - front[edge])
            .normalize_or_zero();
        mesh.push_quad(positions, side_normal, positions.map(uv));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuildingArchetype, BuildingProgram, generate};

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
                mesh.indices.chunks_exact(3).filter_map(|indices| {
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
                .any(|mesh| mesh.material == BuildingLodMaterial::Timber)
        );
        assert!(
            detail
                .meshes
                .iter()
                .any(|mesh| mesh.material == BuildingLodMaterial::Floor)
        );
    }

    #[test]
    fn static_playable_detail_reserves_operable_leaves_for_dynamic_entities() {
        let plan = generate(&BuildingProgram::fixture(BuildingArchetype::TownHouse, 42)).unwrap();
        let self_contained = compile_building_detail(&plan);
        let static_detail = compile_static_building_detail(&plan);
        let triangle_count = |detail: &BuildingDetail| {
            detail
                .meshes
                .iter()
                .map(|mesh| mesh.indices.len() / 3)
                .sum::<usize>()
        };

        assert!(triangle_count(&static_detail) < triangle_count(&self_contained));
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
}
