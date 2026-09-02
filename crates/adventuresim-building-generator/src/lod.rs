//! Render-only level-of-detail meshes derived from an accepted building plan.
//!
//! LOD compilation deliberately consumes semantic assemblies rather than
//! simplifying resolved solids. The accepted plan therefore remains the sole
//! authority for structure, openings, circulation, and tactical collision.

use bevy::math::{Vec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::{
    BuildingPlan, RoofMaterial, WallAssemblyId, WallMaterialClass, tessellate_roof_enclosure,
    tessellate_roof_face,
};

#[path = "lod/crowns.rs"]
mod crowns;
#[path = "lod/details.rs"]
mod details;
#[path = "lod/walls.rs"]
mod walls;

use crowns::append_crowns;
use details::{append_opening_details, append_timber_details};
use walls::append_wall_envelopes;

const JOIN_TOLERANCE_METRES: f32 = 0.02;
const FACADE_DETAIL_OFFSET_METRES: f32 = 0.012;
const TEXTURE_REPEAT_METRES: f32 = 2.0;
const ROUND_LOD_SEGMENTS: usize = 24;

/// Render representation selected by a screen-space LOD policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildingLodLevel {
    /// Joined wall runs, textured façade details, and geometric straight crowns.
    Facade,
    /// Joined shell surfaces with alpha-masked crown strips.
    Shell,
}

/// One material batch shared by exact-detail and LOD render representations.
/// A renderer may bind each variant to a texture-array layer or atlas region
/// while retaining one mesh per material class.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum BuildingLodMaterial {
    Wall(WallMaterialClass),
    Roof(RoofMaterial),
    /// Timber grid and braces baked into a plaster texture for shell LODs.
    FachwerkBaked,
    Timber,
    /// Hewn structural timber visible from the room side.
    InteriorTimber,
    Iron,
    /// Room-facing lime-plaster finish, including the inner face of an exterior wall.
    InteriorPlaster,
    Floor,
    Glass,
    FacadeDetails,
    CrownMasonry,
    CrownMask,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct LodVertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LodMesh {
    pub material: BuildingLodMaterial,
    pub vertices: Vec<LodVertex>,
    pub indices: Vec<u32>,
}

impl LodMesh {
    pub(crate) fn new(material: BuildingLodMaterial) -> Self {
        Self {
            material,
            vertices: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub(crate) fn push_quad(&mut self, positions: [Vec3; 4], normal: Vec3, uvs: [Vec2; 4]) {
        let base = self.vertices.len() as u32;
        let geometric_normal = (positions[1] - positions[0]).cross(positions[2] - positions[0]);
        self.vertices.extend(
            positions
                .into_iter()
                .zip(uvs)
                .map(|(position, uv)| LodVertex {
                    position,
                    normal,
                    uv,
                }),
        );
        let indices = if geometric_normal.dot(normal) < 0.0 {
            [base, base + 2, base + 1, base, base + 3, base + 2]
        } else {
            [base, base + 1, base + 2, base, base + 2, base + 3]
        };
        self.indices.extend_from_slice(&indices);
    }

    pub(crate) fn push_triangle(&mut self, positions: [Vec3; 3], normal: Vec3, uvs: [Vec2; 3]) {
        let base = self.vertices.len() as u32;
        let geometric_normal = (positions[1] - positions[0]).cross(positions[2] - positions[0]);
        self.vertices.extend(
            positions
                .into_iter()
                .zip(uvs)
                .map(|(position, uv)| LodVertex {
                    position,
                    normal,
                    uv,
                }),
        );
        let indices = if geometric_normal.dot(normal) < 0.0 {
            [base, base + 2, base + 1]
        } else {
            [base, base + 1, base + 2]
        };
        self.indices.extend_from_slice(&indices);
    }
}

/// A maximal collinear exterior wall interval sharing one render treatment.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum FacadeRunPath {
    Straight {
        start: Vec2,
        end: Vec2,
        outward: Vec2,
    },
    Round {
        centre: Vec2,
        radius_metres: f32,
        reference_outward: Vec2,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FacadeRun {
    pub material: WallMaterialClass,
    pub storey_level: u16,
    pub path: FacadeRunPath,
    pub base_elevation_metres: f32,
    pub height_metres: f32,
    pub thickness_metres: f32,
    pub source_walls: Vec<WallAssemblyId>,
}

impl FacadeRun {
    fn straight(&self) -> Option<(Vec2, Vec2, Vec2)> {
        match self.path {
            FacadeRunPath::Straight {
                start,
                end,
                outward,
            } => Some((start, end, outward)),
            FacadeRunPath::Round { .. } => None,
        }
    }

    fn can_join(&self, other: &Self) -> bool {
        let (Some((start, end, outward)), Some((other_start, other_end, other_outward))) =
            (self.straight(), other.straight())
        else {
            return false;
        };
        let tangent = (end - start).normalize_or_zero();
        let other_tangent = (other_end - other_start).normalize_or_zero();
        if self.material != other.material
            || self.storey_level != other.storey_level
            || (self.base_elevation_metres - other.base_elevation_metres).abs()
                > JOIN_TOLERANCE_METRES
            || (self.height_metres - other.height_metres).abs() > JOIN_TOLERANCE_METRES
            || (self.thickness_metres - other.thickness_metres).abs() > JOIN_TOLERANCE_METRES
            || outward.dot(other_outward) < 0.999
            || tangent.dot(other_tangent) < 0.999
        {
            return false;
        }
        let line_distance = (other_start - start).perp_dot(tangent).abs();
        let self_interval = ordered_interval(start.dot(tangent), end.dot(tangent));
        let other_interval = ordered_interval(other_start.dot(tangent), other_end.dot(tangent));
        line_distance <= JOIN_TOLERANCE_METRES
            && intervals_touch(self_interval, other_interval, JOIN_TOLERANCE_METRES)
    }

    fn join(&mut self, other: Self) {
        let (start, end, outward) = self.straight().expect("only straight runs join");
        let (other_start, other_end, _) = other.straight().expect("only straight runs join");
        let tangent = (end - start).normalize_or_zero();
        let origin = start - tangent * start.dot(tangent);
        let min = start
            .dot(tangent)
            .min(end.dot(tangent))
            .min(other_start.dot(tangent))
            .min(other_end.dot(tangent));
        let max = start
            .dot(tangent)
            .max(end.dot(tangent))
            .max(other_start.dot(tangent))
            .max(other_end.dot(tangent));
        self.path = FacadeRunPath::Straight {
            start: origin + tangent * min,
            end: origin + tangent * max,
            outward,
        };
        self.source_walls.extend(other.source_walls);
        self.source_walls.sort_unstable();
        self.source_walls.dedup();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildingLod {
    pub level: BuildingLodLevel,
    pub facade_runs: Vec<FacadeRun>,
    pub meshes: Vec<LodMesh>,
}

impl BuildingLod {
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

/// Compiles a render-only LOD from the accepted semantic plan.
pub fn compile_building_lod(plan: &BuildingPlan, level: BuildingLodLevel) -> BuildingLod {
    let facade_runs = extract_facade_runs(plan);
    let mut lod = BuildingLod {
        level,
        facade_runs,
        meshes: Vec::new(),
    };
    append_wall_envelopes(&mut lod);
    append_roofs(&mut lod, plan);
    if level == BuildingLodLevel::Facade {
        append_opening_details(&mut lod, plan);
        append_timber_details(&mut lod, plan);
    }
    append_crowns(&mut lod, plan);
    lod.meshes
        .retain(|mesh| !mesh.vertices.is_empty() && !mesh.indices.is_empty());
    lod
}

fn extract_facade_runs(plan: &BuildingPlan) -> Vec<FacadeRun> {
    let mut runs = plan
        .wall_assemblies
        .iter()
        .filter(|wall| {
            wall.replaced_by_owner.is_none()
                && wall.frame.outside_room.is_none()
                && !matches!(
                    wall.material,
                    WallMaterialClass::InternalTimber | WallMaterialClass::InternalMasonry
                )
        })
        .map(|wall| {
            if let Some(radial) = wall.radial_frame {
                let radius = wall.length_metres / std::f32::consts::TAU;
                return FacadeRun {
                    material: wall.material,
                    storey_level: wall.storey_level,
                    path: FacadeRunPath::Round {
                        centre: radial.centre,
                        radius_metres: radius,
                        reference_outward: radial.reference_outward,
                    },
                    base_elevation_metres: wall.base_elevation_metres,
                    height_metres: wall.height_metres,
                    thickness_metres: wall.thickness_metres,
                    source_walls: vec![wall.id],
                };
            }
            let mut tangent = wall.frame.tangent.normalize_or_zero();
            if tangent.x < -0.001 || (tangent.x.abs() <= 0.001 && tangent.y < 0.0) {
                tangent = -tangent;
            }
            FacadeRun {
                material: wall.material,
                storey_level: wall.storey_level,
                path: FacadeRunPath::Straight {
                    start: wall.frame.origin - tangent * wall.length_metres * 0.5,
                    end: wall.frame.origin + tangent * wall.length_metres * 0.5,
                    outward: wall.frame.outward,
                },
                base_elevation_metres: wall.base_elevation_metres,
                height_metres: wall.height_metres,
                thickness_metres: wall.thickness_metres,
                source_walls: vec![wall.id],
            }
        })
        .collect::<Vec<_>>();

    let mut changed = true;
    while changed {
        changed = false;
        'outer: for left in 0..runs.len() {
            for right in left + 1..runs.len() {
                if runs[left].can_join(&runs[right]) {
                    let other = runs.remove(right);
                    runs[left].join(other);
                    changed = true;
                    break 'outer;
                }
            }
        }
    }
    runs
}

fn append_facade_prism(mesh: &mut LodMesh, run: &FacadeRun) {
    let FacadeRunPath::Straight {
        start,
        end,
        outward,
    } = run.path
    else {
        append_round_wall(mesh, run);
        return;
    };
    let tangent = (end - start).normalize_or_zero();
    let outward = outward.normalize_or_zero();
    let depth = outward * run.thickness_metres * 0.5;
    let bottom = run.base_elevation_metres;
    let top = bottom + run.height_metres;
    let front = [
        plan_vertex(start + depth, bottom),
        plan_vertex(end + depth, bottom),
        plan_vertex(end + depth, top),
        plan_vertex(start + depth, top),
    ];
    let back = [
        plan_vertex(end - depth, bottom),
        plan_vertex(start - depth, bottom),
        plan_vertex(start - depth, top),
        plan_vertex(end - depth, top),
    ];
    let facade_uv = |point: Vec3| {
        Vec2::new(Vec2::new(point.x, point.z).dot(tangent), point.y) / TEXTURE_REPEAT_METRES
    };
    mesh.push_quad(
        front,
        Vec3::new(outward.x, 0.0, outward.y),
        front.map(facade_uv),
    );
    mesh.push_quad(
        back,
        Vec3::new(-outward.x, 0.0, -outward.y),
        back.map(facade_uv),
    );
    let top_face = [front[3], front[2], back[3], back[2]];
    mesh.push_quad(
        top_face,
        Vec3::Y,
        top_face.map(|point| Vec2::new(point.x, point.z) / TEXTURE_REPEAT_METRES),
    );
    let bottom_face = [back[1], back[0], front[1], front[0]];
    mesh.push_quad(
        bottom_face,
        -Vec3::Y,
        bottom_face.map(|point| Vec2::new(point.x, point.z) / TEXTURE_REPEAT_METRES),
    );
    for (positions, normal) in [
        ([back[1], front[0], front[3], back[2]], -tangent),
        ([front[1], back[0], back[3], front[2]], tangent),
    ] {
        mesh.push_quad(
            positions,
            Vec3::new(normal.x, 0.0, normal.y),
            positions.map(|point| Vec2::new(point.z, point.y) / TEXTURE_REPEAT_METRES),
        );
    }
}

fn append_round_wall(mesh: &mut LodMesh, run: &FacadeRun) {
    let FacadeRunPath::Round {
        centre,
        radius_metres: radius,
        ..
    } = run.path
    else {
        return;
    };
    let bottom = run.base_elevation_metres;
    let top = bottom + run.height_metres;
    for segment in 0..ROUND_LOD_SEGMENTS {
        let a = std::f32::consts::TAU * segment as f32 / ROUND_LOD_SEGMENTS as f32;
        let b = std::f32::consts::TAU * (segment + 1) as f32 / ROUND_LOD_SEGMENTS as f32;
        let radial_a = Vec2::from_angle(a);
        let radial_b = Vec2::from_angle(b);
        let positions = [
            plan_vertex(centre + radial_a * radius, bottom),
            plan_vertex(centre + radial_b * radius, bottom),
            plan_vertex(centre + radial_b * radius, top),
            plan_vertex(centre + radial_a * radius, top),
        ];
        let u0 = radius * a / TEXTURE_REPEAT_METRES;
        let u1 = radius * b / TEXTURE_REPEAT_METRES;
        mesh.push_quad(
            positions,
            {
                let normal = (radial_a + radial_b).normalize_or_zero();
                Vec3::new(normal.x, 0.0, normal.y)
            },
            [
                Vec2::new(u0, bottom / TEXTURE_REPEAT_METRES),
                Vec2::new(u1, bottom / TEXTURE_REPEAT_METRES),
                Vec2::new(u1, top / TEXTURE_REPEAT_METRES),
                Vec2::new(u0, top / TEXTURE_REPEAT_METRES),
            ],
        );
    }
}

fn append_roofs(lod: &mut BuildingLod, plan: &BuildingPlan) {
    for assembly in &plan.roof_assemblies {
        for face in &assembly.faces {
            let mesh = lod.mesh_mut(roof_lod_material(lod.level, face.material));
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
        for face in &assembly.enclosure_faces {
            let mesh = lod.mesh_mut(roof_lod_material(lod.level, face.material));
            for triangle in tessellate_roof_enclosure(face) {
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

fn roof_lod_material(level: BuildingLodLevel, material: RoofMaterial) -> BuildingLodMaterial {
    if level == BuildingLodLevel::Shell && material == RoofMaterial::TimberInfill {
        BuildingLodMaterial::FachwerkBaked
    } else {
        BuildingLodMaterial::Roof(material)
    }
}

fn plan_vertex(point: Vec2, y: f32) -> Vec3 {
    Vec3::new(point.x, y, point.y)
}

fn ordered_interval(left: f32, right: f32) -> (f32, f32) {
    (left.min(right), left.max(right))
}

fn intervals_touch(left: (f32, f32), right: (f32, f32), tolerance: f32) -> bool {
    left.0 <= right.1 + tolerance && right.0 <= left.1 + tolerance
}

fn direction_vector(direction: crate::Direction) -> Vec2 {
    match direction {
        crate::Direction::North => Vec2::Y,
        crate::Direction::East => Vec2::X,
        crate::Direction::South => -Vec2::Y,
        crate::Direction::West => -Vec2::X,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BuildingArchetype, BuildingProgram, CrownPath, generate};

    fn assert_triangle_winding_matches_normals(lod: &BuildingLod) {
        for mesh in &lod.meshes {
            for triangle in mesh.indices.as_chunks::<3>().0 {
                let vertices = [
                    mesh.vertices[triangle[0] as usize],
                    mesh.vertices[triangle[1] as usize],
                    mesh.vertices[triangle[2] as usize],
                ];
                let geometric_normal = (vertices[1].position - vertices[0].position)
                    .cross(vertices[2].position - vertices[0].position);
                assert!(
                    geometric_normal.dot(vertices[0].normal) > 0.0,
                    "{:?} triangle winding opposes its normal: {triangle:?}",
                    mesh.material
                );
            }
        }
    }

    #[test]
    fn fachwerk_lod_joins_wall_cells_and_emits_uv_mapped_details() {
        let plan = generate(&BuildingProgram::fixture(
            BuildingArchetype::FachwerkMerchantHouse,
            42,
        ))
        .unwrap();
        let exterior_wall_count = plan
            .wall_assemblies
            .iter()
            .filter(|wall| wall.frame.outside_room.is_none() && wall.radial_frame.is_none())
            .count();
        let lod = compile_building_lod(&plan, BuildingLodLevel::Facade);
        let exterior_ids = plan
            .wall_assemblies
            .iter()
            .filter(|wall| wall.frame.outside_room.is_none())
            .map(|wall| wall.id)
            .collect::<std::collections::HashSet<_>>();
        let exterior_run_count = lod
            .facade_runs
            .iter()
            .filter(|run| run.source_walls.iter().any(|id| exterior_ids.contains(id)))
            .count();

        assert!(exterior_run_count < exterior_wall_count);
        assert!(lod.facade_runs.iter().any(|run| run.source_walls.len() > 1));
        assert!(
            lod.meshes
                .iter()
                .any(|mesh| mesh.material == BuildingLodMaterial::FacadeDetails)
        );
        assert!(lod.meshes.iter().all(|mesh| {
            mesh.vertices.iter().all(|vertex| {
                vertex.position.is_finite() && vertex.normal.is_finite() && vertex.uv.is_finite()
            }) && mesh
                .indices
                .iter()
                .all(|index| *index < mesh.vertices.len() as u32)
        }));
        assert_triangle_winding_matches_normals(&lod);
    }

    #[test]
    fn facade_and_shell_lods_omit_interior_walls() {
        let plan = generate(&BuildingProgram::fixture(
            BuildingArchetype::FachwerkMerchantHouse,
            42,
        ))
        .unwrap();
        let interior_ids = plan
            .wall_assemblies
            .iter()
            .filter(|wall| wall.frame.outside_room.is_some())
            .map(|wall| wall.id)
            .collect::<std::collections::HashSet<_>>();
        assert!(!interior_ids.is_empty());

        for level in [BuildingLodLevel::Facade, BuildingLodLevel::Shell] {
            let lod = compile_building_lod(&plan, level);
            assert!(
                lod.facade_runs
                    .iter()
                    .all(|run| { run.source_walls.iter().all(|id| !interior_ids.contains(id)) })
            );
        }
    }

    #[test]
    fn fachwerk_shell_omits_facade_details_and_reduces_triangle_count() {
        let plan = generate(&BuildingProgram::fixture(
            BuildingArchetype::FachwerkMerchantHouse,
            42,
        ))
        .unwrap();
        let facade = compile_building_lod(&plan, BuildingLodLevel::Facade);
        let shell = compile_building_lod(&plan, BuildingLodLevel::Shell);
        let triangle_count = |lod: &BuildingLod| {
            lod.meshes
                .iter()
                .map(|mesh| mesh.indices.len() / 3)
                .sum::<usize>()
        };

        assert!(
            facade
                .meshes
                .iter()
                .any(|mesh| mesh.material == BuildingLodMaterial::FacadeDetails)
        );
        assert!(
            shell
                .meshes
                .iter()
                .all(|mesh| mesh.material != BuildingLodMaterial::FacadeDetails)
        );
        assert!(
            shell
                .meshes
                .iter()
                .any(|mesh| mesh.material == BuildingLodMaterial::FachwerkBaked)
        );
        assert!(triangle_count(&shell) < triangle_count(&facade));
        assert_triangle_winding_matches_normals(&shell);
    }

    #[test]
    fn fachwerk_detail_quads_lie_on_the_outer_member_faces() {
        let plan = generate(&BuildingProgram::fixture(
            BuildingArchetype::FachwerkMerchantHouse,
            42,
        ))
        .unwrap();
        let frame = plan.timber_frame.as_ref().unwrap();
        let mut lod = BuildingLod {
            level: BuildingLodLevel::Facade,
            facade_runs: Vec::new(),
            meshes: Vec::new(),
        };
        append_timber_details(&mut lod, &plan);
        let mesh = lod
            .meshes
            .iter()
            .find(|mesh| mesh.material == BuildingLodMaterial::FacadeDetails)
            .unwrap();
        for quad in mesh.vertices.as_chunks::<4>().0 {
            let outward = quad[0].normal;
            assert!(
                plan.wall_assemblies.iter().any(|wall| {
                    let wall_outward = Vec3::new(wall.frame.outward.x, 0.0, wall.frame.outward.y);
                    let expected_plane = wall.frame.origin.dot(wall.frame.outward)
                        + wall.thickness_metres * 0.5
                        + FACADE_DETAIL_OFFSET_METRES;
                    wall_outward.dot(outward) > 0.999
                        && quad.iter().all(|vertex| {
                            (vertex.position.dot(outward) - expected_plane).abs() < 0.0001
                        })
                }),
                "fachwerk quad does not lie on an authoritative exterior wall plane"
            );
        }

        let (brace, wall) = frame
            .bays
            .iter()
            .find_map(|bay| {
                let wall = plan
                    .wall_assemblies
                    .iter()
                    .find(|wall| Some(wall.id) == bay.wall)?;
                let brace = bay.member_ids.iter().find_map(|id| {
                    frame.members.iter().find(|member| {
                        member.id == *id && member.role == crate::TimberMemberRole::StoreyBrace
                    })
                })?;
                Some((brace, wall))
            })
            .expect("merchant fixture should contain a facade brace");
        let outward = Vec3::new(wall.frame.outward.x, 0.0, wall.frame.outward.y);
        let expected_plane = wall.frame.origin.dot(wall.frame.outward)
            + wall.thickness_metres * 0.5
            + FACADE_DETAIL_OFFSET_METRES;
        let midpoint = (brace.start + brace.end) * 0.5;
        let expected_midpoint = midpoint + outward * (expected_plane - midpoint.dot(outward));
        assert!(mesh.vertices.as_chunks::<4>().0.iter().any(|quad| {
            let centroid = quad.iter().map(|vertex| vertex.position).sum::<Vec3>() * 0.25;
            centroid.distance(expected_midpoint) < 0.0001
        }));
    }

    #[test]
    fn castle_shell_uses_alpha_mask_batches_for_straight_and_round_crowns() {
        let plan = generate(&BuildingProgram::fixture(
            BuildingArchetype::CourtyardCastle,
            42,
        ))
        .unwrap();
        assert!(
            plan.crowns
                .iter()
                .any(|crown| matches!(crown.path, CrownPath::Straight { .. }))
        );
        assert!(
            plan.crowns
                .iter()
                .any(|crown| matches!(crown.path, CrownPath::Round { .. }))
        );

        let facade = compile_building_lod(&plan, BuildingLodLevel::Facade);
        assert!(
            facade.meshes.iter().any(|mesh| {
                mesh.material == BuildingLodMaterial::CrownMasonry && !mesh.vertices.is_empty()
            }),
            "the facade representation must retain geometric straight crowns"
        );

        let lod = compile_building_lod(&plan, BuildingLodLevel::Shell);
        assert!(
            lod.meshes
                .iter()
                .all(|mesh| mesh.material != BuildingLodMaterial::CrownMasonry),
            "the shell representation substitutes crown geometry with the mask"
        );
        let mask = lod
            .meshes
            .iter()
            .find(|mesh| mesh.material == BuildingLodMaterial::CrownMask)
            .expect("castle shell should contain a crown mask batch");
        assert!(mask.vertices.len() > ROUND_LOD_SEGMENTS * 4);
        assert!(mask.vertices.iter().any(|vertex| vertex.uv.x > 1.0));
    }
}
