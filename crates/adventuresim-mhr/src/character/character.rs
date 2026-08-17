use std::collections::HashMap;

use adventuresim_fbx::{Object, Scene};
use anyhow::{Context, Result, bail};

use crate::character::blend_shapes::BlendShapes;
use crate::character::mesh::Mesh;
use crate::character::skeleton::Skeleton;
use crate::character::skin_weights::{MAX_SKIN_JOINTS, SkinWeights};
use crate::math::{
    Mat4, Quat, Transform, affine_inverse, mat4_from_column_major, quat_from_euler_degrees,
    quat_mul, rotation_order,
};

/// Everything the MHR forward pass needs from the rig file.
pub struct Character {
    pub skeleton: Skeleton,
    pub mesh: Mesh,
    pub skin_weights: SkinWeights,
    /// Inverse bind pose per joint as a skeleton state `[t, q, s]`.
    pub inverse_bind_pose: Vec<[f32; 8]>,
    pub blend_shapes: BlendShapes,
}

struct SkeletonBuilder<'a> {
    scene: &'a Scene,
    skeleton: Skeleton,
    /// FBX object id per joint index, used to resolve skin clusters back to joints.
    joint_ids: Vec<i64>,
}

impl<'a> SkeletonBuilder<'a> {
    fn visit(&mut self, object: &Object, parent: Option<usize>) {
        if !object.is_node() {
            return;
        }

        if object.is_null_node() {
            if parent.is_none() && object.node.property70("col_type").is_none() {
                for child in self.scene.children(object.id).collect::<Vec<_>>() {
                    self.visit(child, None);
                }
            }
            return;
        }

        if !object.is_limb() {
            return;
        }

        let order = rotation_order(object.node.property70_i64("RotationOrder", 0));
        let local_rotation =
            quat_from_euler_degrees(object.node.property70_vec3("Lcl Rotation", [0.0; 3]), order);
        let pre_rotation = quat_from_euler_degrees(
            object.node.property70_vec3("PreRotation", [0.0; 3]),
            [0, 1, 2],
        );
        let prerotation: Quat = quat_mul(pre_rotation, local_rotation);
        let offset = object.node.property70_vec3("Lcl Translation", [0.0; 3]);

        let index = self.skeleton.len();
        self.skeleton.names.push(object.name.clone());
        self.skeleton
            .parents
            .push(parent.map(|p| p as i32).unwrap_or(-1));
        self.skeleton.translation_offsets.push([
            offset[0] as f32,
            offset[1] as f32,
            offset[2] as f32,
        ]);
        self.skeleton.prerotations.push([
            prerotation[0] as f32,
            prerotation[1] as f32,
            prerotation[2] as f32,
            prerotation[3] as f32,
        ]);
        self.joint_ids.push(object.id);

        for child in self.scene.children(object.id).collect::<Vec<_>>() {
            self.visit(child, Some(index));
        }
    }
}

fn parse_skeleton(scene: &Scene) -> (Skeleton, Vec<i64>) {
    let mut builder = SkeletonBuilder {
        scene,
        skeleton: Skeleton::default(),
        joint_ids: Vec::new(),
    };
    for root in scene.children(0).collect::<Vec<_>>() {
        builder.visit(root, None);
    }
    (builder.skeleton, builder.joint_ids)
}

/// Splits an FBX `PolygonVertexIndex` stream into fan-triangulated triangles.
fn triangulate(polygon_vertex_index: &[i64]) -> Result<Vec<[u32; 3]>> {
    let mut faces = Vec::new();
    let mut polygon: Vec<u32> = Vec::with_capacity(4);
    for raw in polygon_vertex_index {
        let (index, last) = if *raw < 0 {
            ((-(*raw + 1)) as u32, true)
        } else {
            (*raw as u32, false)
        };
        polygon.push(index);
        if !last {
            continue;
        }
        if polygon.len() < 3 {
            bail!(
                "invalid face with {} indices; expected at least 3",
                polygon.len()
            );
        }
        for j in 1..polygon.len() - 1 {
            faces.push([polygon[0], polygon[j], polygon[j + 1]]);
        }
        polygon.clear();
    }
    if !polygon.is_empty() {
        bail!("trailing polygon indices without a terminator");
    }
    Ok(faces)
}

fn parse_uvs(geometry: &Object, polygon_count: usize) -> (Vec<[f32; 2]>, Vec<i64>) {
    let Some(layer) = geometry.node.child("LayerElementUV") else {
        return (Vec::new(), Vec::new());
    };
    let Some(values) = layer.child("UV").and_then(|n| n.f64_array()) else {
        return (Vec::new(), Vec::new());
    };
    let texcoords: Vec<[f32; 2]> = values
        .chunks_exact(2)
        .map(|uv| [uv[0] as f32, 1.0 - uv[1] as f32])
        .collect();

    let reference = layer
        .child("ReferenceInformationType")
        .and_then(|n| n.str_prop(0))
        .map(|v| v.to_vec())
        .unwrap_or_default();
    let indices = if reference == b"IndexToDirect" {
        layer
            .child("UVIndex")
            .and_then(|n| n.i64_array())
            .unwrap_or_default()
    } else {
        (0..polygon_count as i64).collect()
    };
    (texcoords, indices)
}

fn triangulate_texcoords(polygon_vertex_index: &[i64], texcoord_indices: &[i64]) -> Vec<[u32; 3]> {
    if texcoord_indices.len() != polygon_vertex_index.len() {
        return Vec::new();
    }
    let mut faces = Vec::new();
    let mut polygon: Vec<u32> = Vec::with_capacity(4);
    for (raw, tex) in polygon_vertex_index.iter().zip(texcoord_indices) {
        polygon.push(*tex as u32);
        if *raw >= 0 {
            continue;
        }
        for j in 1..polygon.len().saturating_sub(1) {
            faces.push([polygon[0], polygon[j], polygon[j + 1]]);
        }
        polygon.clear();
    }
    faces
}

fn parse_blend_shapes(scene: &Scene, geometry: &Object, num_vertices: usize) -> BlendShapes {
    let mut names = Vec::new();
    let mut vectors: Vec<f32> = Vec::new();

    let Some(blend_shape) = scene
        .children(geometry.id)
        .find(|o| o.kind == "Deformer" && o.class == "BlendShape")
    else {
        return BlendShapes::default();
    };

    for channel in scene
        .children(blend_shape.id)
        .filter(|o| o.kind == "Deformer" && o.class == "BlendShapeChannel")
        .collect::<Vec<_>>()
    {
        for shape in scene
            .children(channel.id)
            .filter(|o| o.kind == "Geometry" && o.class == "Shape")
            .collect::<Vec<_>>()
        {
            let offsets = shape.node.child("Vertices").and_then(|n| n.f64_array());
            let indices = shape.node.child("Indexes").and_then(|n| n.i64_array());
            let base = vectors.len();
            vectors.resize(base + num_vertices * 3, 0.0);
            if let (Some(offsets), Some(indices)) = (offsets, indices) {
                for (slot, vertex) in indices.iter().enumerate() {
                    let vertex = *vertex as usize;
                    if vertex >= num_vertices || slot * 3 + 2 >= offsets.len() {
                        continue;
                    }
                    for axis in 0..3 {
                        vectors[base + vertex * 3 + axis] = offsets[slot * 3 + axis] as f32;
                    }
                }
            }
            names.push(shape.name.clone());
        }
    }

    BlendShapes {
        names,
        vectors,
        num_vertices,
    }
}

fn parse_skin(
    scene: &Scene,
    geometry: &Object,
    joint_of_object: &HashMap<i64, usize>,
    num_vertices: usize,
    inverse_bind_pose: &mut [Mat4],
) -> Result<SkinWeights> {
    let mut per_vertex: Vec<Vec<(usize, f64)>> = vec![Vec::new(); num_vertices];

    let Some(skin) = scene
        .children(geometry.id)
        .find(|o| o.kind == "Deformer" && o.class == "Skin")
    else {
        bail!("geometry '{}' has no skin deformer", geometry.name);
    };

    for cluster in scene
        .children(skin.id)
        .filter(|o| o.kind == "Deformer" && o.class == "Cluster")
        .collect::<Vec<_>>()
    {
        let Some(bone) = scene
            .children(cluster.id)
            .find(|o| joint_of_object.contains_key(&o.id))
        else {
            bail!("cluster '{}' references an unknown bone", cluster.name);
        };
        let joint = joint_of_object[&bone.id];

        if let Some(link) = cluster
            .node
            .child("TransformLink")
            .and_then(|n| n.f64_array())
            && link.len() == 16
        {
            inverse_bind_pose[joint] = affine_inverse(&mat4_from_column_major(&link));
        }

        let (Some(indices), Some(weights)) = (
            cluster.node.child("Indexes").and_then(|n| n.i64_array()),
            cluster.node.child("Weights").and_then(|n| n.f64_array()),
        ) else {
            continue;
        };
        if indices.len() != weights.len() {
            bail!(
                "cluster '{}' has mismatched indices and weights",
                cluster.name
            );
        }

        for (vertex, weight) in indices.iter().zip(&weights) {
            let vertex = *vertex as usize;
            if vertex >= num_vertices {
                bail!("cluster '{}' references vertex {vertex}", cluster.name);
            }
            if *weight <= 0.0 {
                continue;
            }
            per_vertex[vertex].push((joint, weight.clamp(0.0, 1.0)));
        }
    }

    let mut skin_weights = SkinWeights {
        index: vec![[0; MAX_SKIN_JOINTS]; num_vertices],
        weight: vec![[0.0; MAX_SKIN_JOINTS]; num_vertices],
    };
    for (vertex, influences) in per_vertex.iter_mut().enumerate() {
        if influences.is_empty() {
            bail!("no skinning weights for vertex {vertex}");
        }
        influences.sort_by(|a, b| b.1.total_cmp(&a.1));
        influences.truncate(MAX_SKIN_JOINTS);
        let total: f64 = influences.iter().map(|(_, w)| *w).sum();
        if total <= 0.0 {
            bail!("empty weight sum for vertex {vertex}");
        }
        for (slot, (joint, weight)) in influences.iter().enumerate() {
            skin_weights.index[vertex][slot] = *joint as u32;
            skin_weights.weight[vertex][slot] = (*weight / total) as f32;
        }
    }

    Ok(skin_weights)
}

impl Character {
    /// Loads a character from the bytes of a binary FBX rig.
    pub fn from_fbx_bytes(data: &[u8], load_blend_shapes: bool) -> Result<Self> {
        let scene = Scene::parse(data)?;
        let (skeleton, joint_ids) = parse_skeleton(&scene);
        if skeleton.is_empty() {
            bail!("no joints found in FBX rig");
        }

        let joint_of_object: HashMap<i64, usize> = joint_ids
            .iter()
            .enumerate()
            .map(|(index, id)| (*id, index))
            .collect();

        let bind_pose = skeleton.bind_pose();
        let mut inverse_bind_pose: Vec<Mat4> = bind_pose
            .iter()
            .map(|transform| {
                let inverse = transform.inverse();
                let rotation = crate::math::quat_to_matrix(inverse.rotation);
                let mut m = [[0.0; 4]; 4];
                for row in 0..3 {
                    for col in 0..3 {
                        m[row][col] = rotation[row][col] * inverse.scale;
                    }
                    m[row][3] = inverse.translation[row];
                }
                m[3][3] = 1.0;
                m
            })
            .collect();

        let mesh_model = scene
            .objects_of_kind("Model", "Mesh")
            .next()
            .context("FBX rig has no mesh")?;
        let geometry = scene
            .child_of_kind(mesh_model.id, "Geometry", "Mesh")
            .context("mesh model has no geometry")?;

        let vertices: Vec<[f32; 3]> = geometry
            .node
            .child("Vertices")
            .and_then(|n| n.f64_array())
            .context("mesh geometry has no vertices")?
            .chunks_exact(3)
            .map(|v| [v[0] as f32, v[1] as f32, v[2] as f32])
            .collect();
        let polygon_vertex_index = geometry
            .node
            .child("PolygonVertexIndex")
            .and_then(|n| n.i64_array())
            .context("mesh geometry has no polygons")?;

        let faces = triangulate(&polygon_vertex_index)?;
        let (texcoords, texcoord_indices) = parse_uvs(geometry, polygon_vertex_index.len());
        let texcoord_faces = triangulate_texcoords(&polygon_vertex_index, &texcoord_indices);

        let blend_shapes = if load_blend_shapes {
            parse_blend_shapes(&scene, geometry, vertices.len())
        } else {
            BlendShapes::default()
        };

        let skin_weights = parse_skin(
            &scene,
            geometry,
            &joint_of_object,
            vertices.len(),
            &mut inverse_bind_pose,
        )?;

        Ok(Self {
            skeleton,
            mesh: Mesh {
                vertices,
                faces,
                texcoords,
                texcoord_faces,
            },
            skin_weights,
            inverse_bind_pose: inverse_bind_pose
                .iter()
                .map(|m| Transform::from_matrix(m).to_skel_state())
                .collect(),
            blend_shapes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangulates_polygon_fans() {
        let stream = [0, 1, 2, -4, 4, 5, -7];
        let faces = triangulate(&stream).unwrap();
        assert_eq!(faces, vec![[0, 1, 2], [0, 2, 3], [4, 5, 6]]);
    }

    #[test]
    fn rejects_degenerate_polygons() {
        assert!(triangulate(&[0, -2]).is_err());
        assert!(triangulate(&[0, 1, 2]).is_err());
    }
}
