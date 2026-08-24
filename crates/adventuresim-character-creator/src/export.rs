//! Deterministic binary glTF export for an identity-shaped MHR character.

use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use fabelgeist_mhr::math::Transform;
use serde_json::{Value, json};

const GLB_MAGIC: &[u8; 4] = b"glTF";
const GLB_VERSION: u32 = 2;
const JSON_CHUNK: u32 = 0x4E4F_534A;
const BIN_CHUNK: u32 = 0x004E_4942;

pub const LEFT_WEAPON_JOINT: &str = "l_weapon";
pub const RIGHT_WEAPON_JOINT: &str = "r_weapon";
pub const FIRST_PERSON_CAMERA_JOINT: &str = "c_camera";

pub struct RiggedMesh<'a> {
    pub positions: &'a [[f32; 3]],
    pub normals: &'a [[f32; 3]],
    pub faces: &'a [[u32; 3]],
    pub joint_indices: &'a [[u32; 8]],
    pub joint_weights: &'a [[f32; 8]],
    pub joint_names: &'a [String],
    pub joint_parents: &'a [i32],
    /// Identity-shaped global MHR transforms, in metres.
    pub global_joint_states: &'a [[f32; 8]],
}

pub struct RiggedShell<'a> {
    pub name: &'a str,
    pub positions: &'a [[f32; 3]],
    pub faces: &'a [[u32; 3]],
    /// Artist-facing sRGB color. glTF factors are converted to linear RGB.
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
}

#[derive(Default)]
struct BufferBuilder {
    bytes: Vec<u8>,
    views: Vec<Value>,
}

impl BufferBuilder {
    fn push(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        while !self.bytes.len().is_multiple_of(4) {
            self.bytes.push(0);
        }
        let offset = self.bytes.len();
        self.bytes.extend_from_slice(bytes);
        let mut view = json!({
            "buffer": 0,
            "byteOffset": offset,
            "byteLength": bytes.len(),
        });
        if let Some(target) = target {
            view["target"] = target.into();
        }
        self.views.push(view);
        self.views.len() - 1
    }
}

fn f32_bytes(values: impl IntoIterator<Item = f32>) -> Vec<u8> {
    values
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>()
}

fn u16_bytes(values: impl IntoIterator<Item = u16>) -> Vec<u8> {
    values
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>()
}

fn u32_bytes(values: impl IntoIterator<Item = u32>) -> Vec<u8> {
    values
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>()
}

fn state(value: [f32; 8]) -> Transform {
    Transform {
        translation: [value[0] as f64, value[1] as f64, value[2] as f64],
        rotation: [
            value[3] as f64,
            value[4] as f64,
            value[5] as f64,
            value[6] as f64,
        ],
        scale: value[7] as f64,
    }
}

fn transform_matrix(transform: Transform) -> [f32; 16] {
    let rotation = fabelgeist_mhr::math::quat_to_matrix(transform.rotation);
    let scale = transform.scale;
    // glTF stores matrices as column-major arrays.
    [
        (rotation[0][0] * scale) as f32,
        (rotation[1][0] * scale) as f32,
        (rotation[2][0] * scale) as f32,
        0.0,
        (rotation[0][1] * scale) as f32,
        (rotation[1][1] * scale) as f32,
        (rotation[2][1] * scale) as f32,
        0.0,
        (rotation[0][2] * scale) as f32,
        (rotation[1][2] * scale) as f32,
        (rotation[2][2] * scale) as f32,
        0.0,
        transform.translation[0] as f32,
        transform.translation[1] as f32,
        transform.translation[2] as f32,
        1.0,
    ]
}

fn landmark(mesh: &RiggedMesh<'_>, name: &str) -> Result<usize> {
    mesh.joint_names
        .iter()
        .position(|candidate| candidate == name)
        .with_context(|| format!("MHR rig is missing attachment landmark {name}"))
}

fn between(a: Transform, b: Transform, amount: f64) -> [f64; 3] {
    [
        a.translation[0] + (b.translation[0] - a.translation[0]) * amount,
        a.translation[1] + (b.translation[1] - a.translation[1]) * amount,
        a.translation[2] + (b.translation[2] - a.translation[2]) * amount,
    ]
}

fn append_attachment(
    names: &mut Vec<String>,
    parents: &mut Vec<i32>,
    globals: &mut Vec<Transform>,
    name: &str,
    parent: usize,
    translation: [f64; 3],
) {
    let parent_state = globals[parent];
    names.push(name.to_owned());
    parents.push(parent as i32);
    globals.push(Transform {
        translation,
        rotation: parent_state.rotation,
        scale: parent_state.scale,
    });
}

fn validate(mesh: &RiggedMesh<'_>, shells: &[RiggedShell<'_>]) -> Result<()> {
    let vertices = mesh.positions.len();
    if vertices == 0 || mesh.normals.len() != vertices {
        bail!("positions and normals must contain the same non-zero vertex count");
    }
    if mesh.joint_indices.len() != vertices || mesh.joint_weights.len() != vertices {
        bail!("skinning arrays must match the vertex count");
    }
    let joints = mesh.joint_names.len();
    if joints == 0 || mesh.joint_parents.len() != joints || mesh.global_joint_states.len() != joints
    {
        bail!("joint names, parents, and transforms must have the same non-zero length");
    }
    if joints > u16::MAX as usize {
        bail!("glTF export supports at most 65535 joints");
    }
    for (index, parent) in mesh.joint_parents.iter().copied().enumerate() {
        if parent >= index as i32 || parent < -1 {
            bail!("joint {index} has invalid parent {parent}");
        }
    }
    if mesh
        .faces
        .iter()
        .flatten()
        .any(|index| *index as usize >= vertices)
    {
        bail!("a face references a missing vertex");
    }
    for (vertex, (indices, weights)) in mesh
        .joint_indices
        .iter()
        .zip(mesh.joint_weights)
        .enumerate()
    {
        if indices
            .iter()
            .zip(weights)
            .any(|(joint, weight)| *weight > 0.0 && *joint as usize >= joints)
        {
            bail!("vertex {vertex} references a missing joint");
        }
        let sum: f32 = weights.iter().sum();
        if !sum.is_finite() || (sum - 1.0).abs() > 1e-4 {
            bail!("vertex {vertex} skin weights sum to {sum}, not 1");
        }
    }
    for shell in shells {
        if shell.name.trim().is_empty() {
            bail!("clothing shell name cannot be empty");
        }
        if shell.positions.len() != vertices || shell.faces.is_empty() {
            bail!(
                "clothing shell '{}' must have the body vertex count and at least one face",
                shell.name
            );
        }
        if shell
            .faces
            .iter()
            .flatten()
            .any(|index| *index as usize >= vertices)
        {
            bail!(
                "clothing shell '{}' references a missing vertex",
                shell.name
            );
        }
        if shell
            .positions
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
            || !shell.metallic.is_finite()
            || !shell.roughness.is_finite()
            || shell.base_color.iter().any(|value| !value.is_finite())
        {
            bail!(
                "clothing shell '{}' contains a non-finite value",
                shell.name
            );
        }
    }
    Ok(())
}

fn position_bounds(positions: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    positions.iter().fold(
        ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]),
        |(mut minimum, mut maximum), position| {
            for axis in 0..3 {
                minimum[axis] = minimum[axis].min(position[axis]);
                maximum[axis] = maximum[axis].max(position[axis]);
            }
            (minimum, maximum)
        },
    )
}

fn linear_base_color([red, green, blue, alpha]: [f32; 4]) -> [f32; 4] {
    fn channel(value: f32) -> f32 {
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }
    [channel(red), channel(green), channel(blue), alpha]
}

/// Writes a self-contained, zero-animation GLB suitable for Cascadeur and the
/// runtime base-rig preparation pipeline.
pub fn export_rigged_glb(
    path: &Path,
    character_name: &str,
    recipe_version: u8,
    lod: u8,
    mesh: &RiggedMesh<'_>,
    shells: &[RiggedShell<'_>],
) -> Result<()> {
    validate(mesh, shells)?;
    let mut buffer = BufferBuilder::default();
    let positions = buffer.push(
        &f32_bytes(mesh.positions.iter().flatten().copied()),
        Some(34_962),
    );
    let normals = buffer.push(
        &f32_bytes(mesh.normals.iter().flatten().copied()),
        Some(34_962),
    );
    let joints_0 = buffer.push(
        &u16_bytes(
            mesh.joint_indices
                .iter()
                .flat_map(|indices| indices[..4].iter().map(|value| *value as u16)),
        ),
        Some(34_962),
    );
    let weights_0 = buffer.push(
        &f32_bytes(
            mesh.joint_weights
                .iter()
                .flat_map(|weights| weights[..4].iter().copied()),
        ),
        Some(34_962),
    );
    let joints_1 = buffer.push(
        &u16_bytes(
            mesh.joint_indices
                .iter()
                .flat_map(|indices| indices[4..].iter().map(|value| *value as u16)),
        ),
        Some(34_962),
    );
    let weights_1 = buffer.push(
        &f32_bytes(
            mesh.joint_weights
                .iter()
                .flat_map(|weights| weights[4..].iter().copied()),
        ),
        Some(34_962),
    );
    let indices = buffer.push(
        &u32_bytes(mesh.faces.iter().flatten().copied()),
        Some(34_963),
    );
    let shell_views = shells
        .iter()
        .map(|shell| {
            let positions = buffer.push(
                &f32_bytes(shell.positions.iter().flatten().copied()),
                Some(34_962),
            );
            let indices = buffer.push(
                &u32_bytes(shell.faces.iter().flatten().copied()),
                Some(34_963),
            );
            (positions, indices)
        })
        .collect::<Vec<_>>();

    let mut globals = mesh
        .global_joint_states
        .iter()
        .copied()
        .map(state)
        .collect::<Vec<_>>();
    let mut joint_names = mesh.joint_names.to_vec();
    let mut joint_parents = mesh.joint_parents.to_vec();
    let left_wrist = landmark(mesh, "l_wrist")?;
    let left_middle = landmark(mesh, "l_middle1")?;
    let right_wrist = landmark(mesh, "r_wrist")?;
    let right_middle = landmark(mesh, "r_middle1")?;
    let head = landmark(mesh, "c_head")?;
    let left_eye = landmark(mesh, "l_eye")?;
    let right_eye = landmark(mesh, "r_eye")?;
    let left_grip_position = between(globals[left_wrist], globals[left_middle], 0.5);
    let right_grip_position = between(globals[right_wrist], globals[right_middle], 0.5);
    let camera_position = between(globals[left_eye], globals[right_eye], 0.5);
    append_attachment(
        &mut joint_names,
        &mut joint_parents,
        &mut globals,
        LEFT_WEAPON_JOINT,
        left_wrist,
        left_grip_position,
    );
    append_attachment(
        &mut joint_names,
        &mut joint_parents,
        &mut globals,
        RIGHT_WEAPON_JOINT,
        right_wrist,
        right_grip_position,
    );
    append_attachment(
        &mut joint_names,
        &mut joint_parents,
        &mut globals,
        FIRST_PERSON_CAMERA_JOINT,
        head,
        camera_position,
    );
    let inverse_bind_matrices = buffer.push(
        &f32_bytes(
            globals
                .iter()
                .flat_map(|global| transform_matrix(global.inverse())),
        ),
        None,
    );

    let (minimum, maximum) = position_bounds(mesh.positions);

    let mut accessors = Vec::new();
    let mut accessor =
        |view, component_type, count, kind: &str, bounds: Option<([f32; 3], [f32; 3])>| {
            let mut value = json!({
                "bufferView": view,
                "componentType": component_type,
                "count": count,
                "type": kind,
            });
            if let Some((minimum, maximum)) = bounds {
                value["min"] = json!(minimum);
                value["max"] = json!(maximum);
            }
            accessors.push(value);
            accessors.len() - 1
        };
    let position_accessor = accessor(
        positions,
        5_126,
        mesh.positions.len(),
        "VEC3",
        Some((minimum, maximum)),
    );
    let normal_accessor = accessor(normals, 5_126, mesh.normals.len(), "VEC3", None);
    let joints_0_accessor = accessor(joints_0, 5_123, mesh.positions.len(), "VEC4", None);
    let weights_0_accessor = accessor(weights_0, 5_126, mesh.positions.len(), "VEC4", None);
    let joints_1_accessor = accessor(joints_1, 5_123, mesh.positions.len(), "VEC4", None);
    let weights_1_accessor = accessor(weights_1, 5_126, mesh.positions.len(), "VEC4", None);
    let index_accessor = accessor(indices, 5_125, mesh.faces.len() * 3, "SCALAR", None);
    let inverse_bind_accessor = accessor(
        inverse_bind_matrices,
        5_126,
        joint_names.len(),
        "MAT4",
        None,
    );
    let mut primitives = Vec::new();
    let mut material_values = Vec::new();
    if !mesh.faces.is_empty() {
        primitives.push(json!({
            "attributes": {
                "POSITION": position_accessor,
                "NORMAL": normal_accessor,
                "JOINTS_0": joints_0_accessor,
                "WEIGHTS_0": weights_0_accessor,
                "JOINTS_1": joints_1_accessor,
                "WEIGHTS_1": weights_1_accessor,
            },
            "indices": index_accessor,
            "material": 0,
        }));
        material_values.push(json!({
            "name": "Skin",
            "pbrMetallicRoughness": {
                "baseColorFactor": [0.64, 0.39, 0.30, 1.0],
                "metallicFactor": 0.0,
                "roughnessFactor": 0.52,
            }
        }));
    }
    for (shell, (position_view, index_view)) in shells.iter().zip(shell_views) {
        let (minimum, maximum) = position_bounds(shell.positions);
        let shell_position_accessor = accessor(
            position_view,
            5_126,
            shell.positions.len(),
            "VEC3",
            Some((minimum, maximum)),
        );
        let shell_index_accessor =
            accessor(index_view, 5_125, shell.faces.len() * 3, "SCALAR", None);
        let material = material_values.len();
        primitives.push(json!({
            "attributes": {
                "POSITION": shell_position_accessor,
                "NORMAL": normal_accessor,
                "JOINTS_0": joints_0_accessor,
                "WEIGHTS_0": weights_0_accessor,
                "JOINTS_1": joints_1_accessor,
                "WEIGHTS_1": weights_1_accessor,
            },
            "indices": shell_index_accessor,
            "material": material,
        }));
        material_values.push(json!({
            "name": shell.name,
            "pbrMetallicRoughness": {
                "baseColorFactor": linear_base_color(shell.base_color),
                "metallicFactor": shell.metallic,
                "roughnessFactor": shell.roughness,
            }
        }));
    }

    let mut children = vec![Vec::<usize>::new(); joint_names.len()];
    let mut roots = Vec::new();
    for (joint, parent) in joint_parents.iter().copied().enumerate() {
        if parent < 0 {
            roots.push(joint);
        } else {
            children[parent as usize].push(joint);
        }
    }
    let mut nodes = Vec::with_capacity(joint_names.len() + 2);
    for joint in 0..joint_names.len() {
        let local = if joint_parents[joint] < 0 {
            globals[joint]
        } else {
            globals[joint_parents[joint] as usize]
                .inverse()
                .compose(&globals[joint])
        };
        let state = local.to_skel_state();
        let mut node = json!({
            "name": joint_names[joint],
            "translation": [state[0], state[1], state[2]],
            "rotation": [state[3], state[4], state[5], state[6]],
            "scale": [state[7], state[7], state[7]],
        });
        if !children[joint].is_empty() {
            node["children"] = json!(children[joint]);
        }
        nodes.push(node);
    }
    // Keep one stable, non-joint hierarchy root. Bevy uses this node as the
    // beginning of every animation target path, and Cascadeur preserves it
    // when the base rig is used to author motion files.
    let skeleton_node = nodes.len();
    nodes.push(json!({"name": "Skeleton", "children": roots}));
    let mesh_node = nodes.len();
    nodes.push(json!({"name": character_name, "mesh": 0, "skin": 0}));

    let skeleton_root = mesh
        .joint_parents
        .iter()
        .position(|parent| *parent < 0)
        .context("MHR skeleton has no root")?;
    let document = json!({
        "asset": {"version": "2.0", "generator": "Fabelgeist MHR character creator"},
        "scene": 0,
        "scenes": [{"name": "Character", "nodes": [skeleton_node, mesh_node]}],
        "nodes": nodes,
        "meshes": [{
            "name": character_name,
            "primitives": primitives,
        }],
        "materials": material_values,
        "skins": [{
            "name": "MHR",
            "inverseBindMatrices": inverse_bind_accessor,
            "skeleton": skeleton_root,
            "joints": (0..joint_names.len()).collect::<Vec<_>>(),
        }],
        "accessors": accessors,
        "bufferViews": buffer.views,
        "buffers": [{"byteLength": buffer.bytes.len()}],
        "extras": {
            "adventuresim_character": {
                "name": character_name,
                "recipe_version": recipe_version,
                "mhr_release": "v1.0.1",
                "lod": lod,
                "placeholder_clothing": shells.iter().map(|shell| shell.name).collect::<Vec<_>>(),
            },
            "adventuresim_rig": {
                "family": "mhr",
                "neutral_pose": "T-pose",
                "units": "metres",
                "up_axis": "+Y",
                "forward_axis": "-Z",
                "attachments": [
                    {"name": LEFT_WEAPON_JOINT, "parent": "l_wrist", "role": "left_weapon_grip"},
                    {"name": RIGHT_WEAPON_JOINT, "parent": "r_wrist", "role": "right_weapon_grip"},
                    {"name": FIRST_PERSON_CAMERA_JOINT, "parent": "c_head", "role": "first_person_camera"},
                ],
            }
        }
    });

    let mut json_bytes = serde_json::to_vec(&document)?;
    json_bytes.resize(json_bytes.len().next_multiple_of(4), b' ');
    buffer
        .bytes
        .resize(buffer.bytes.len().next_multiple_of(4), 0);
    let total_length = 12 + 8 + json_bytes.len() + 8 + buffer.bytes.len();
    let mut glb = Vec::with_capacity(total_length);
    glb.extend_from_slice(GLB_MAGIC);
    glb.extend_from_slice(&GLB_VERSION.to_le_bytes());
    glb.extend_from_slice(&(total_length as u32).to_le_bytes());
    glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    glb.extend_from_slice(&JSON_CHUNK.to_le_bytes());
    glb.extend_from_slice(&json_bytes);
    glb.extend_from_slice(&(buffer.bytes.len() as u32).to_le_bytes());
    glb.extend_from_slice(&BIN_CHUNK.to_le_bytes());
    glb.extend_from_slice(&buffer.bytes);

    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating export directory {}", parent.display()))?;
    }
    fs::write(path, glb).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_document(bytes: &[u8]) -> Value {
        let json_length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        serde_json::from_slice(&bytes[20..20 + json_length]).unwrap()
    }

    fn attachment_test_skeleton() -> (Vec<String>, Vec<i32>, Vec<[f32; 8]>) {
        let names = [
            "body_world",
            "l_wrist",
            "l_middle1",
            "r_wrist",
            "r_middle1",
            "c_head",
            "l_eye",
            "r_eye",
        ]
        .map(str::to_owned)
        .to_vec();
        let parents = vec![-1, 0, 1, 0, 3, 0, 5, 5];
        let transform = |translation: [f32; 3]| {
            [
                translation[0],
                translation[1],
                translation[2],
                0.0,
                0.0,
                0.0,
                1.0,
                1.0,
            ]
        };
        let states = vec![
            transform([0.0, 0.0, 0.0]),
            transform([-0.5, 1.0, 0.0]),
            transform([-0.5, 1.0, 0.2]),
            transform([0.5, 1.0, 0.0]),
            transform([0.5, 1.0, 0.2]),
            transform([0.0, 1.5, 0.0]),
            transform([-0.03, 1.6, -0.08]),
            transform([0.03, 1.6, -0.08]),
        ];
        (names, parents, states)
    }

    #[test]
    fn exports_two_sets_of_skin_influences_and_a_zero_animation_rig() {
        let directory =
            std::env::temp_dir().join(format!("fabelgeist-mhr-export-{}", std::process::id()));
        let path = directory.join("character.glb");
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let normals = [[0.0, 0.0, 1.0]; 3];
        let faces = [[0, 1, 2]];
        let joint_indices = [[0, 0, 0, 0, 0, 0, 0, 0]; 3];
        let joint_weights = [[
            0.5, 0.25, 0.125, 0.0625, 0.03125, 0.015625, 0.0078125, 0.0078125,
        ]; 3];
        let (joint_names, joint_parents, global_joint_states) = attachment_test_skeleton();
        export_rigged_glb(
            &path,
            "Test",
            1,
            1,
            &RiggedMesh {
                positions: &positions,
                normals: &normals,
                faces: &faces,
                joint_indices: &joint_indices,
                joint_weights: &joint_weights,
                joint_names: &joint_names,
                joint_parents: &joint_parents,
                global_joint_states: &global_joint_states,
            },
            &[],
        )
        .unwrap();
        let bytes = fs::read(&path).unwrap();
        let document = read_document(&bytes);
        let parsed = gltf::Gltf::from_slice(&bytes).unwrap();
        assert_eq!(&bytes[..4], GLB_MAGIC);
        assert_eq!(parsed.skins().count(), 1);
        assert_eq!(parsed.meshes().count(), 1);
        assert_eq!(document["skins"][0]["joints"].as_array().unwrap().len(), 11);
        let nodes = document["nodes"].as_array().unwrap();
        let index_of = |name: &str| nodes.iter().position(|node| node["name"] == name).unwrap();
        let left_weapon = index_of(LEFT_WEAPON_JOINT);
        let right_weapon = index_of(RIGHT_WEAPON_JOINT);
        let camera = index_of(FIRST_PERSON_CAMERA_JOINT);
        assert!(
            nodes[index_of("l_wrist")]["children"]
                .as_array()
                .unwrap()
                .contains(&json!(left_weapon))
        );
        assert!(
            nodes[index_of("r_wrist")]["children"]
                .as_array()
                .unwrap()
                .contains(&json!(right_weapon))
        );
        assert!(
            nodes[index_of("c_head")]["children"]
                .as_array()
                .unwrap()
                .contains(&json!(camera))
        );
        for attachment in [left_weapon, right_weapon] {
            let translation = nodes[attachment]["translation"].as_array().unwrap();
            assert!(translation[0].as_f64().unwrap().abs() < 1e-6);
            assert!(translation[1].as_f64().unwrap().abs() < 1e-6);
            assert!((translation[2].as_f64().unwrap() - 0.1).abs() < 1e-6);
        }
        assert_eq!(
            document["meshes"][0]["primitives"][0]["attributes"]["JOINTS_1"],
            4
        );
        assert!(document.get("animations").is_none());
        assert_eq!(document["extras"]["adventuresim_rig"]["family"], "mhr");
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn rejects_non_normalized_weights() {
        let positions = [[0.0, 0.0, 0.0]];
        let normals = [[0.0, 1.0, 0.0]];
        let faces = [];
        let joint_indices = [[0; 8]];
        let joint_weights = [[0.0; 8]];
        let joint_names = ["root".to_owned()];
        let joint_parents = [-1];
        let global_joint_states = [[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0]];
        let result = export_rigged_glb(
            Path::new("unused.glb"),
            "Test",
            1,
            1,
            &RiggedMesh {
                positions: &positions,
                normals: &normals,
                faces: &faces,
                joint_indices: &joint_indices,
                joint_weights: &joint_weights,
                joint_names: &joint_names,
                joint_parents: &joint_parents,
                global_joint_states: &global_joint_states,
            },
            &[],
        );
        assert!(result.unwrap_err().to_string().contains("skin weights sum"));
    }

    #[test]
    fn exports_clothing_as_a_separately_materialed_skinned_primitive() {
        let directory = std::env::temp_dir().join(format!(
            "fabelgeist-mhr-clothing-export-{}",
            std::process::id()
        ));
        let path = directory.join("character.glb");
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let shell_positions = [[0.0, 0.0, 0.01], [1.0, 0.0, 0.01], [0.0, 1.0, 0.01]];
        let normals = [[0.0, 0.0, 1.0]; 3];
        let faces = [[0, 1, 2]];
        let joint_indices = [[0; 8]; 3];
        let joint_weights = [[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]; 3];
        let (joint_names, joint_parents, global_joint_states) = attachment_test_skeleton();
        let shell = RiggedShell {
            name: "Tunic",
            positions: &shell_positions,
            faces: &faces,
            base_color: [0.1, 0.2, 0.3, 1.0],
            metallic: 0.0,
            roughness: 0.9,
        };
        export_rigged_glb(
            &path,
            "Test",
            2,
            1,
            &RiggedMesh {
                positions: &positions,
                normals: &normals,
                faces: &faces,
                joint_indices: &joint_indices,
                joint_weights: &joint_weights,
                joint_names: &joint_names,
                joint_parents: &joint_parents,
                global_joint_states: &global_joint_states,
            },
            &[shell],
        )
        .unwrap();
        let bytes = fs::read(&path).unwrap();
        let document = read_document(&bytes);
        let parsed = gltf::Gltf::from_slice(&bytes).unwrap();
        assert_eq!(parsed.meshes().next().unwrap().primitives().count(), 2);
        assert_eq!(document["materials"][1]["name"], "Tunic");
        let red = document["materials"][1]["pbrMetallicRoughness"]["baseColorFactor"][0]
            .as_f64()
            .unwrap();
        assert!((red - 0.010_022_8).abs() < 1e-6);
        assert_eq!(document["meshes"][0]["primitives"][1]["material"], 1);
        assert_eq!(
            document["meshes"][0]["primitives"][1]["attributes"]["JOINTS_1"],
            document["meshes"][0]["primitives"][0]["attributes"]["JOINTS_1"]
        );
        let _ = fs::remove_dir_all(directory);
    }
}
