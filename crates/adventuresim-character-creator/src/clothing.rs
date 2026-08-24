//! Fitted placeholder garments generated from catalog-authored anatomical spans.

use std::collections::{HashMap, HashSet};

use crate::item_catalog_schema::{
    EquipmentAnatomicalRegion as Region, EquipmentChannel, EquipmentMaterial, EquipmentPlacement,
    SurfaceAnchor,
};

#[derive(Debug, Clone)]
pub struct GarmentSpecification {
    pub name: String,
    pub material: EquipmentMaterial,
    pub placement: EquipmentPlacement,
    pub weight_threshold: f32,
    pub expansion_rings: usize,
    pub normal_offset_metres: f32,
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
    pub occludes_body: bool,
}

impl GarmentSpecification {
    pub fn from_catalog(
        name: impl Into<String>,
        placement: &EquipmentPlacement,
        material: EquipmentMaterial,
    ) -> Self {
        let channel = placement
            .occupancy
            .iter()
            .map(|requirement| requirement.channel)
            .max_by_key(|channel| channel.order())
            .unwrap_or(EquipmentChannel::BaseClothing);
        let normal_offset_metres = match channel {
            EquipmentChannel::BaseClothing | EquipmentChannel::Outerwear => 0.007,
            EquipmentChannel::Padding => 0.012,
            EquipmentChannel::FlexibleArmor => 0.017,
            EquipmentChannel::RigidArmor => 0.024,
            _ => 0.008,
        };
        let (base_color, metallic, roughness) = match material {
            EquipmentMaterial::PolishedSteel => ([0.769, 0.776, 0.776, 1.0], 1.0, 0.20),
            EquipmentMaterial::RoughSteel => ([0.769, 0.776, 0.776, 1.0], 1.0, 0.58),
            EquipmentMaterial::OxidizedSteel => ([0.420, 0.275, 0.196, 1.0], 0.0, 0.82),
            EquipmentMaterial::MailSteel => ([0.769, 0.776, 0.776, 1.0], 1.0, 0.42),
            EquipmentMaterial::VegetableTannedLeather => ([0.502, 0.353, 0.231, 1.0], 0.0, 0.58),
            EquipmentMaterial::Linen => ([0.722, 0.663, 0.510, 1.0], 0.0, 0.88),
            EquipmentMaterial::Wool => ([0.561, 0.510, 0.408, 1.0], 0.0, 0.92),
            EquipmentMaterial::QuiltedTextile => ([0.459, 0.416, 0.314, 1.0], 0.0, 0.90),
        };
        Self {
            name: name.into(),
            material,
            placement: placement.clone(),
            weight_threshold: 0.35,
            expansion_rings: 1,
            normal_offset_metres,
            base_color,
            metallic,
            roughness,
            occludes_body: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClothingShell {
    pub specification: GarmentSpecification,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub faces: Vec<[u32; 3]>,
}

#[derive(Debug, Clone)]
pub struct ClothedMesh {
    pub visible_body_faces: Vec<[u32; 3]>,
    pub shells: Vec<ClothingShell>,
}

#[derive(Clone, Copy)]
struct RegionRig {
    proximal: &'static str,
    distal: &'static str,
    joint: fn(&str) -> bool,
}

fn region_rig(region: Region) -> RegionRig {
    match region {
        Region::Head => RegionRig {
            proximal: "c_neck",
            distal: "c_head",
            joint: |name| {
                matches!(name, "c_head" | "c_jaw" | "l_eye" | "r_eye")
                    || name.starts_with("c_tongue")
            },
        },
        Region::Neck => RegionRig {
            proximal: "c_spine3",
            distal: "c_neck",
            joint: |name| name == "c_neck" || name.starts_with("c_neck_twist"),
        },
        Region::Chest => RegionRig {
            proximal: "c_spine2",
            distal: "c_neck",
            joint: |name| matches!(name, "c_spine2" | "c_spine3" | "l_clavicle" | "r_clavicle"),
        },
        Region::Stomach => RegionRig {
            proximal: "c_spine0",
            distal: "c_spine2",
            joint: |name| matches!(name, "c_spine0" | "c_spine1"),
        },
        Region::LeftUpperArm => limb_rig("l_uparm", "l_lowarm", left_upper_arm),
        Region::LeftForearm => limb_rig("l_lowarm", "l_wrist", left_forearm),
        Region::RightUpperArm => limb_rig("r_uparm", "r_lowarm", right_upper_arm),
        Region::RightForearm => limb_rig("r_lowarm", "r_wrist", right_forearm),
        Region::LeftThigh => limb_rig("l_upleg", "l_lowleg", left_thigh),
        Region::LeftLowerLeg => limb_rig("l_lowleg", "l_foot", left_lower_leg),
        Region::RightThigh => limb_rig("r_upleg", "r_lowleg", right_thigh),
        Region::RightLowerLeg => limb_rig("r_lowleg", "r_foot", right_lower_leg),
    }
}

const fn limb_rig(
    proximal: &'static str,
    distal: &'static str,
    joint: fn(&str) -> bool,
) -> RegionRig {
    RegionRig {
        proximal,
        distal,
        joint,
    }
}

fn left_upper_arm(name: &str) -> bool {
    name == "l_uparm" || name.starts_with("l_uparm_twist")
}
fn left_forearm(name: &str) -> bool {
    name == "l_lowarm" || name.starts_with("l_lowarm_twist")
}
fn right_upper_arm(name: &str) -> bool {
    name == "r_uparm" || name.starts_with("r_uparm_twist")
}
fn right_forearm(name: &str) -> bool {
    name == "r_lowarm" || name.starts_with("r_lowarm_twist")
}
fn left_thigh(name: &str) -> bool {
    name == "l_upleg" || name.starts_with("l_upleg_twist")
}
fn left_lower_leg(name: &str) -> bool {
    name == "l_lowleg" || name.starts_with("l_lowleg_twist")
}
fn right_thigh(name: &str) -> bool {
    name == "r_upleg" || name.starts_with("r_upleg_twist")
}
fn right_lower_leg(name: &str) -> bool {
    name == "r_lowleg" || name.starts_with("r_lowleg_twist")
}

fn squared_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

const CONCAVITY_RELAXATION_PASSES: usize = 3;
const CONCAVITY_RELAXATION_STRENGTH: f32 = 0.5;

fn subtract(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalized(value: [f32; 3]) -> Option<[f32; 3]> {
    let length = dot(value, value).sqrt();
    (length > f32::EPSILON).then(|| [value[0] / length, value[1] / length, value[2] / length])
}

/// Raises local valleys toward their one-ring average while leaving convex
/// points and garment openings in place. The source normals define "outward"
/// for every pass so the relaxation cannot shrink the shell into the body.
fn relax_concavities(
    positions: &[[f32; 3]],
    source_normals: &[[f32; 3]],
    faces: &[[u32; 3]],
) -> Vec<[f32; 3]> {
    let mut neighbors = vec![Vec::new(); positions.len()];
    let mut edge_counts = HashMap::<(u32, u32), usize>::new();
    for face in faces {
        for (a, b) in [(face[0], face[1]), (face[1], face[2]), (face[2], face[0])] {
            neighbors[a as usize].push(b as usize);
            neighbors[b as usize].push(a as usize);
            *edge_counts.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let boundary_vertices = edge_counts
        .into_iter()
        .filter(|(_, count)| *count == 1)
        .flat_map(|((a, b), _)| [a as usize, b as usize])
        .collect::<HashSet<_>>();
    for adjacent in &mut neighbors {
        adjacent.sort_unstable();
        adjacent.dedup();
    }

    let mut relaxed = positions.to_vec();
    for _ in 0..CONCAVITY_RELAXATION_PASSES {
        let previous = relaxed.clone();
        for (vertex, adjacent) in neighbors.iter().enumerate() {
            if adjacent.is_empty() || boundary_vertices.contains(&vertex) {
                continue;
            }
            let Some(normal) = normalized(source_normals[vertex]) else {
                continue;
            };
            let inverse_count = 1.0 / adjacent.len() as f32;
            let average = adjacent.iter().fold([0.0; 3], |mut average, neighbor| {
                for axis in 0..3 {
                    average[axis] += previous[*neighbor][axis] * inverse_count;
                }
                average
            });
            let outward_lift = dot(subtract(average, previous[vertex]), normal).max(0.0)
                * CONCAVITY_RELAXATION_STRENGTH;
            for axis in 0..3 {
                relaxed[vertex][axis] = previous[vertex][axis] + normal[axis] * outward_lift;
            }
        }
    }
    relaxed
}

fn surface_normals(
    positions: &[[f32; 3]],
    source_normals: &[[f32; 3]],
    faces: &[[u32; 3]],
) -> Vec<[f32; 3]> {
    let mut accumulated = vec![[0.0; 3]; positions.len()];
    for face in faces {
        let [a, b, c] = face.map(|vertex| positions[vertex as usize]);
        let face_normal = cross(subtract(b, a), subtract(c, a));
        for vertex in face {
            for axis in 0..3 {
                accumulated[*vertex as usize][axis] += face_normal[axis];
            }
        }
    }
    accumulated
        .into_iter()
        .zip(source_normals)
        .map(|(normal, fallback)| {
            normalized(normal)
                .map(|normal| {
                    if dot(normal, *fallback) < 0.0 {
                        [-normal[0], -normal[1], -normal[2]]
                    } else {
                        normal
                    }
                })
                .unwrap_or(*fallback)
        })
        .collect()
}

fn chain_coordinate(point: [f32; 3], segments: &[([f32; 3], [f32; 3])]) -> Option<f32> {
    let lengths = segments
        .iter()
        .map(|(start, end)| squared_distance(*start, *end).sqrt())
        .collect::<Vec<_>>();
    let total = lengths.iter().sum::<f32>();
    if total <= f32::EPSILON {
        return None;
    }
    let mut best = None::<(f32, f32)>;
    let mut distance_along = 0.0;
    for (index, ((start, end), length)) in segments.iter().zip(lengths).enumerate() {
        if length <= f32::EPSILON {
            continue;
        }
        let axis = [
            (end[0] - start[0]) / length,
            (end[1] - start[1]) / length,
            (end[2] - start[2]) / length,
        ];
        let raw_projection = (point[0] - start[0]) * axis[0]
            + (point[1] - start[1]) * axis[1]
            + (point[2] - start[2]) * axis[2];
        let projection = if (index == 0 && raw_projection < 0.0)
            || (index + 1 == segments.len() && raw_projection > length)
        {
            raw_projection
        } else {
            raw_projection.clamp(0.0, length)
        };
        let nearest = [
            start[0] + axis[0] * projection,
            start[1] + axis[1] * projection,
            start[2] + axis[2] * projection,
        ];
        let candidate = (
            squared_distance(point, nearest),
            (distance_along + projection) / total,
        );
        if best.is_none_or(|current| candidate.0 < current.0) {
            best = Some(candidate);
        }
        distance_along += length;
    }
    best.map(|(_, coordinate)| coordinate)
}

fn anchor_interval(anchor: SurfaceAnchor, coverage: f32) -> (f32, f32) {
    match anchor {
        SurfaceAnchor::Proximal => (0.0, coverage),
        SurfaceAnchor::Distal => (1.0 - coverage, 1.0),
        SurfaceAnchor::Center => ((1.0 - coverage) * 0.5, (1.0 + coverage) * 0.5),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn generate_clothing_shells(
    garments: &[GarmentSpecification],
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    faces: &[[u32; 3]],
    joint_indices: &[[u32; 8]],
    joint_weights: &[[f32; 8]],
    joint_names: &[String],
    global_joint_states: &[[f32; 8]],
) -> Result<ClothedMesh, String> {
    let vertices = positions.len();
    if normals.len() != vertices
        || joint_indices.len() != vertices
        || joint_weights.len() != vertices
        || global_joint_states.len() != joint_names.len()
    {
        return Err("clothing inputs have inconsistent vertex or joint counts".into());
    }
    if faces
        .iter()
        .flatten()
        .any(|vertex| *vertex as usize >= vertices)
    {
        return Err("clothing topology references a missing vertex".into());
    }
    let joint_position = |name: &str| {
        joint_names
            .iter()
            .position(|candidate| candidate == name)
            .map(|index| {
                let state = global_joint_states[index];
                [state[0], state[1], state[2]]
            })
            .ok_or_else(|| format!("MHR rig is missing anatomical landmark {name}"))
    };

    let mut shells = Vec::with_capacity(garments.len());
    let mut occluded_faces = HashSet::new();
    for specification in garments {
        let mut span_masks = Vec::new();
        for span in &specification.placement.surface {
            let rigs = span
                .regions
                .iter()
                .copied()
                .map(region_rig)
                .collect::<Vec<_>>();
            let selected_joints = joint_names
                .iter()
                .enumerate()
                .filter_map(|(index, name)| {
                    rigs.iter().any(|rig| (rig.joint)(name)).then_some(index)
                })
                .collect::<HashSet<_>>();
            let segments = rigs
                .iter()
                .map(|rig| Ok((joint_position(rig.proximal)?, joint_position(rig.distal)?)))
                .collect::<Result<Vec<_>, String>>()?;
            let coordinate_bounds = positions
                .iter()
                .enumerate()
                .filter_map(|(vertex, position)| {
                    let weight = joint_indices[vertex]
                        .iter()
                        .zip(&joint_weights[vertex])
                        .filter(|(joint, _)| selected_joints.contains(&(**joint as usize)))
                        .map(|(_, weight)| *weight)
                        .sum::<f32>();
                    (weight >= specification.weight_threshold)
                        .then(|| chain_coordinate(*position, &segments))
                        .flatten()
                })
                .fold(None::<(f32, f32)>, |bounds, coordinate| {
                    Some(
                        bounds.map_or((coordinate, coordinate), |(minimum, maximum)| {
                            (minimum.min(coordinate), maximum.max(coordinate))
                        }),
                    )
                })
                .ok_or_else(|| format!("{} selected no weighted vertices", specification.name))?;
            span_masks.push((
                selected_joints,
                segments,
                coordinate_bounds,
                anchor_interval(span.anchor, span.coverage),
            ));
        }
        if span_masks.is_empty() {
            return Err(format!(
                "{} has no anatomical surface spans",
                specification.name
            ));
        }
        let covered = positions
            .iter()
            .enumerate()
            .map(|(vertex, position)| {
                span_masks.iter().any(
                    |(selected_joints, segments, (minimum, maximum), (start, end))| {
                        let weight = joint_indices[vertex]
                            .iter()
                            .zip(&joint_weights[vertex])
                            .filter(|(joint, _)| selected_joints.contains(&(**joint as usize)))
                            .map(|(_, weight)| *weight)
                            .sum::<f32>();
                        weight >= specification.weight_threshold
                            && chain_coordinate(*position, segments).is_some_and(|coordinate| {
                                let extent = maximum - minimum;
                                extent > f32::EPSILON
                                    && (coordinate - minimum) / extent >= *start
                                    && (coordinate - minimum) / extent <= *end
                            })
                    },
                )
            })
            .collect::<Vec<_>>();
        let mut selected_faces = faces
            .iter()
            .map(|face| {
                face.iter()
                    .filter(|vertex| covered[**vertex as usize])
                    .count()
                    >= 2
            })
            .collect::<Vec<_>>();
        for _ in 0..specification.expansion_rings {
            let mut selected_vertices = vec![false; vertices];
            for (face, selected) in faces.iter().zip(&selected_faces) {
                if *selected {
                    for vertex in face {
                        selected_vertices[*vertex as usize] = true;
                    }
                }
            }
            let previous = selected_faces.clone();
            for (index, face) in faces.iter().enumerate() {
                selected_faces[index] |= face
                    .iter()
                    .filter(|vertex| selected_vertices[**vertex as usize])
                    .count()
                    >= 2;
            }
            if selected_faces == previous {
                break;
            }
        }
        let shell_faces = faces
            .iter()
            .copied()
            .zip(&selected_faces)
            .filter_map(|(face, selected)| selected.then_some(face))
            .collect::<Vec<_>>();
        if shell_faces.is_empty() {
            return Err(format!("{} selected no MHR triangles", specification.name));
        }
        if specification.occludes_body {
            occluded_faces.extend(
                selected_faces
                    .iter()
                    .enumerate()
                    .filter_map(|(index, selected)| selected.then_some(index)),
            );
        }
        let relaxed_positions = relax_concavities(positions, normals, &shell_faces);
        let shell_normals = surface_normals(&relaxed_positions, normals, &shell_faces);
        let shell_positions = relaxed_positions
            .iter()
            .zip(&shell_normals)
            .map(|(position, normal)| {
                [
                    position[0] + normal[0] * specification.normal_offset_metres,
                    position[1] + normal[1] * specification.normal_offset_metres,
                    position[2] + normal[2] * specification.normal_offset_metres,
                ]
            })
            .collect();
        shells.push(ClothingShell {
            specification: specification.clone(),
            positions: shell_positions,
            normals: shell_normals,
            faces: shell_faces,
        });
    }
    Ok(ClothedMesh {
        visible_body_faces: faces
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(index, face)| (!occluded_faces.contains(&index)).then_some(face))
            .collect(),
        shells,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item_catalog_schema::{EquipmentBodyPart, EquipmentSurfaceSpan};

    #[test]
    fn anchors_clip_the_requested_fraction() {
        assert_eq!(anchor_interval(SurfaceAnchor::Proximal, 0.3), (0.0, 0.3));
        assert_eq!(anchor_interval(SurfaceAnchor::Distal, 0.3), (0.7, 1.0));
        assert_eq!(anchor_interval(SurfaceAnchor::Center, 0.3), (0.35, 0.65));
    }

    #[test]
    fn coordinate_continues_across_multiple_bones() {
        let segments = [
            ([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            ([0.0, 1.0, 0.0], [0.0, 3.0, 0.0]),
        ];
        assert!((chain_coordinate([0.0, 2.0, 0.0], &segments).unwrap() - 2.0 / 3.0).abs() < 1e-6);
    }

    fn center_fan(center_height: f32) -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
        (
            vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
                [0.0, 0.0, center_height],
            ],
            vec![[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]],
        )
    }

    #[test]
    fn concavity_relaxation_raises_a_valley_without_moving_its_opening() {
        let (positions, faces) = center_fan(-1.0);
        let normals = vec![[0.0, 0.0, 1.0]; positions.len()];

        let relaxed = relax_concavities(&positions, &normals, &faces);

        assert!(relaxed[4][2] > positions[4][2]);
        assert_eq!(&relaxed[..4], &positions[..4]);
    }

    #[test]
    fn concavity_relaxation_does_not_flatten_a_convex_peak() {
        let (positions, faces) = center_fan(1.0);
        let normals = vec![[0.0, 0.0, 1.0]; positions.len()];

        let relaxed = relax_concavities(&positions, &normals, &faces);

        assert_eq!(relaxed, positions);
    }

    #[test]
    fn recomputed_surface_normals_are_unit_length() {
        let (positions, faces) = center_fan(0.25);
        let fallback = vec![[0.0, 0.0, 1.0]; positions.len()];

        let normals = surface_normals(&positions, &fallback, &faces);

        for normal in normals {
            assert!((dot(normal, normal) - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn catalog_material_class_controls_shell_offset() {
        let placement = EquipmentPlacement {
            id: "left".into(),
            occupancy: vec![crate::item_catalog_schema::OccupancyRequirement {
                location: crate::item_catalog_schema::EquipmentLocation::LeftArm,
                channel: EquipmentChannel::RigidArmor,
                order: 0,
            }],
            parents: Vec::new(),
            protection: vec![EquipmentBodyPart::LeftArm],
            surface: vec![EquipmentSurfaceSpan {
                regions: vec![Region::LeftForearm],
                anchor: SurfaceAnchor::Distal,
                coverage: 0.3,
            }],
        };
        let specification = GarmentSpecification::from_catalog(
            "bracer",
            &placement,
            EquipmentMaterial::PolishedSteel,
        );
        assert_eq!(specification.normal_offset_metres, 0.024);
        assert_eq!(specification.metallic, 1.0);
        assert_eq!(specification.roughness, 0.20);
        assert_eq!(specification.placement.surface[0].coverage, 0.3);
    }
}
