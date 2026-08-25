//! Deterministic binary glTF export for an identity-shaped MHR character.

use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use fabelgeist_mhr::math::{Transform, quat_from_matrix, quat_normalize};
use serde_json::{Value, json};

const GLB_MAGIC: &[u8; 4] = b"glTF";
const GLB_VERSION: u32 = 2;
const JSON_CHUNK: u32 = 0x4E4F_534A;
const BIN_CHUNK: u32 = 0x004E_4942;

pub const LEFT_WEAPON_JOINT: &str = "l_weapon";
pub const RIGHT_WEAPON_JOINT: &str = "r_weapon";
pub const FIRST_PERSON_CAMERA_JOINT: &str = "c_camera";
pub const EQUIPMENT_SOCKET_NODE_PREFIX: &str = "equipment_socket_";
pub const MHR_ANATOMICAL_UV_DOMAIN: &str = "mhr_body_v1";

const PALMAR_SOCKET_CLEARANCE_METERS: f64 = 0.002;
const EQUIPMENT_SOCKET_CLEARANCE_METERS: f64 = 0.003;
const EQUIPMENT_SOCKET_WEIGHT_THRESHOLD: f32 = 0.35;
const RAY_INTERSECTION_EPSILON: f64 = 1e-9;
// Socket-local correction authored against assets_src/grip.glb. Keeping this
// on the bone makes every weapon share the same hand-relative default pose.
const WEAPON_SOCKET_CALIBRATION: Transform = Transform {
    translation: [
        -0.012158611279745317,
        -0.0014950778497500217,
        0.0023301551882869073,
    ],
    rotation: [
        -0.04224712194103828,
        0.1219730997753762,
        0.06256993654774025,
        0.9896578937487928,
    ],
    scale: 1.0,
};

pub struct RiggedMesh<'a> {
    pub positions: &'a [[f32; 3]],
    pub normals: &'a [[f32; 3]],
    pub faces: &'a [[u32; 3]],
    /// Whether the source body faces are emitted as a rendered primitive.
    /// Shell-only equipment still supplies them to locate authored sockets.
    pub export_body: bool,
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
    pub normals: &'a [[f32; 3]],
    pub faces: &'a [[u32; 3]],
    /// Artist-facing sRGB color. glTF factors are converted to linear RGB.
    pub base_color: [f32; 4],
    pub metallic: f32,
    pub roughness: f32,
}

pub struct RiggedSocket<'a> {
    pub attachment_point_id: &'a str,
    pub surface_uv_domain: &'a str,
    pub surface_uv: [f32; 2],
    /// Pelvis-local transform in the generated character's neutral pose.
    pub transform: Transform,
}

pub struct SurfaceUvLayout<'a> {
    pub domain: &'a str,
    pub texcoords: &'a [[f32; 2]],
    pub texcoord_faces: &'a [[u32; 3]],
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

fn subtract(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(vector: [f64; 3]) -> f64 {
    dot(vector, vector).sqrt()
}

fn normalize(vector: [f64; 3], side: &str, description: &str) -> Result<[f64; 3]> {
    let length = length(vector);
    if !length.is_finite() || length < 1e-9 {
        bail!("MHR {side} hand landmarks do not define a stable {description}");
    }
    Ok(vector.map(|component| component / length))
}

fn ray_triangle_distance(
    origin: [f64; 3],
    direction: [f64; 3],
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
) -> Option<f64> {
    ray_triangle_hit(origin, direction, a, b, c).map(|(distance, _, _)| distance)
}

fn ray_triangle_hit(
    origin: [f64; 3],
    direction: [f64; 3],
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
) -> Option<(f64, f64, f64)> {
    let edge_ab = subtract(b, a);
    let edge_ac = subtract(c, a);
    let perpendicular = cross(direction, edge_ac);
    let determinant = dot(edge_ab, perpendicular);
    if determinant.abs() < RAY_INTERSECTION_EPSILON {
        return None;
    }

    let inverse_determinant = determinant.recip();
    let from_a = subtract(origin, a);
    let u = inverse_determinant * dot(from_a, perpendicular);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }

    let cross_from_a = cross(from_a, edge_ab);
    let v = inverse_determinant * dot(direction, cross_from_a);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }

    let distance = inverse_determinant * dot(edge_ac, cross_from_a);
    (distance > RAY_INTERSECTION_EPSILON).then_some((distance, u, v))
}

fn attachment_region_joint(name: &str) -> bool {
    matches!(name, "root" | "c_spine0" | "c_spine1" | "l_upleg" | "r_upleg")
        || name.starts_with("l_upleg_twist")
        || name.starts_with("r_upleg_twist")
}

fn attachment_region_weight(mesh: &RiggedMesh<'_>, vertex: usize) -> f32 {
    mesh.joint_indices[vertex]
        .iter()
        .zip(mesh.joint_weights[vertex])
        .filter(|(joint, _)| {
            mesh.joint_names
                .get(**joint as usize)
                .is_some_and(|name| attachment_region_joint(name))
        })
        .map(|(_, weight)| weight)
        .sum()
}

fn face_key(mut face: [u32; 3]) -> [u32; 3] {
    face.sort_unstable();
    face
}

fn uv_barycentric(point: [f64; 2], a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> Option<[f64; 3]> {
    let determinant = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
    if determinant.abs() <= RAY_INTERSECTION_EPSILON {
        return None;
    }
    let first = ((b[1] - c[1]) * (point[0] - c[0])
        + (c[0] - b[0]) * (point[1] - c[1]))
        / determinant;
    let second = ((c[1] - a[1]) * (point[0] - c[0])
        + (a[0] - c[0]) * (point[1] - c[1]))
        / determinant;
    let third = 1.0 - first - second;
    const UV_EDGE_EPSILON: f64 = 1e-6;
    [first, second, third]
        .iter()
        .all(|weight| *weight >= -UV_EDGE_EPSILON && *weight <= 1.0 + UV_EDGE_EPSILON)
        .then_some([first, second, third])
}

fn interpolate3(values: [[f64; 3]; 3], weights: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|axis| {
        values[0][axis] * weights[0]
            + values[1][axis] * weights[1]
            + values[2][axis] * weights[2]
    })
}

fn equipment_surface_socket(
    mesh: &RiggedMesh<'_>,
    translation: [f64; 3],
    normal: [f64; 3],
    tangent: [f32; 3],
) -> Result<Transform> {
    let pelvis = landmark(mesh, "root")?;
    let pelvis_transform = state(mesh.global_joint_states[pelvis]);
    let direction = normalize(normal, "equipment", "surface normal")?;
    let translation = std::array::from_fn(|axis| {
        translation[axis] + direction[axis] * EQUIPMENT_SOCKET_CLEARANCE_METERS
    });
    let mut local_y = tangent.map(f64::from);
    let tangent_normal_component = dot(local_y, direction);
    for axis in 0..3 {
        local_y[axis] -= direction[axis] * tangent_normal_component;
    }
    let local_y = normalize(local_y, "equipment", "authored socket tangent")?;
    let local_z = direction;
    let local_x = normalize(cross(local_y, local_z), "equipment", "socket frame")?;
    let rotation = quat_normalize(quat_from_matrix([
        [local_x[0], local_y[0], local_z[0]],
        [local_x[1], local_y[1], local_z[1]],
        [local_x[2], local_y[2], local_z[2]],
    ]));
    let model_space_socket = Transform {
        translation,
        rotation,
        scale: 1.0,
    };
    Ok(pelvis_transform.inverse().compose(&model_space_socket))
}

/// Resolves one canonical anatomical UV coordinate against this LOD's body
/// atlas and its fitted garment shell, then constructs a pelvis-local socket.
pub fn fitted_equipment_socket_from_uv(
    mesh: &RiggedMesh<'_>,
    shell: &RiggedShell<'_>,
    layout: &SurfaceUvLayout<'_>,
    uv: [f32; 2],
    outward: [f64; 3],
    tangent: [f32; 3],
) -> Result<Transform> {
    if layout.domain != MHR_ANATOMICAL_UV_DOMAIN {
        bail!("unsupported anatomical UV domain {}", layout.domain);
    }
    if layout.texcoord_faces.len() != mesh.faces.len() {
        bail!("anatomical UV faces do not match the MHR body topology");
    }
    let selected_faces = shell
        .faces
        .iter()
        .copied()
        .map(face_key)
        .collect::<std::collections::BTreeSet<_>>();
    let expected_outward = normalize(outward, "equipment", "expected outward direction")?;
    let point = uv.map(f64::from);
    let mut maximum_attachment_weight = 0.0_f64;
    let candidate = mesh
        .faces
        .iter()
        .copied()
        .zip(layout.texcoord_faces.iter().copied())
        .enumerate()
        .filter(|(_, (face, _))| selected_faces.contains(&face_key(*face)))
        .filter_map(|(face_index, (face, uv_face))| {
            let uv_triangle = uv_face.map(|index| layout.texcoords.get(index as usize).copied())
                .into_iter()
                .collect::<Option<Vec<_>>>()?;
            let barycentric = uv_barycentric(
                point,
                uv_triangle[0].map(f64::from),
                uv_triangle[1].map(f64::from),
                uv_triangle[2].map(f64::from),
            )?;
            let weights = face.map(|vertex| attachment_region_weight(mesh, vertex as usize) as f64);
            let attachment_weight = weights[0] * barycentric[0]
                + weights[1] * barycentric[1]
                + weights[2] * barycentric[2];
            maximum_attachment_weight = maximum_attachment_weight.max(attachment_weight);
            if attachment_weight < EQUIPMENT_SOCKET_WEIGHT_THRESHOLD as f64 {
                return None;
            }
            let normals = face.map(|vertex| shell.normals[vertex as usize].map(f64::from));
            let normal = normalize(
                interpolate3(normals, barycentric),
                "equipment",
                "interpolated UV surface normal",
            )
            .ok()?;
            let alignment = dot(normal, expected_outward);
            (alignment > 0.25).then_some((alignment, attachment_weight, face_index, face, barycentric, normal))
        })
        .max_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.total_cmp(&right.1))
                .then_with(|| right.2.cmp(&left.2))
        });
    let Some((_, _, _, face, barycentric, normal)) = candidate else {
        bail!(
            "anatomical UV {:?} in domain {} did not resolve on fitted shell {} (maximum attachment weight {:.3})",
            uv,
            layout.domain,
            shell.name,
            maximum_attachment_weight,
        );
    };
    let positions = face.map(|vertex| shell.positions[vertex as usize].map(f64::from));
    equipment_surface_socket(mesh, interpolate3(positions, barycentric), normal, tangent)
}

/// Authoring utility that converts the previous outward-ray convention into a
/// canonical anatomical UV coordinate. Routine asset generation resolves the
/// stored UV and does not call this function.
pub fn bootstrap_equipment_surface_uv(
    mesh: &RiggedMesh<'_>,
    shell: &RiggedShell<'_>,
    layout: &SurfaceUvLayout<'_>,
    outward: [f64; 3],
) -> Result<[f32; 2]> {
    if layout.texcoord_faces.len() != mesh.faces.len() {
        bail!("anatomical UV faces do not match the MHR body topology");
    }
    let body_faces = mesh
        .faces
        .iter()
        .copied()
        .enumerate()
        .map(|(index, face)| (face_key(face), index))
        .collect::<std::collections::BTreeMap<_, _>>();
    let selected_vertices = shell
        .faces
        .iter()
        .flat_map(|face| face.iter().copied())
        .map(|vertex| vertex as usize)
        .collect::<std::collections::BTreeSet<_>>();
    let (minimum, maximum) = selected_vertices.iter().fold(
        ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]),
        |(mut minimum, mut maximum), vertex| {
            for axis in 0..3 {
                let coordinate = shell.positions[*vertex][axis] as f64;
                minimum[axis] = minimum[axis].min(coordinate);
                maximum[axis] = maximum[axis].max(coordinate);
            }
            (minimum, maximum)
        },
    );
    let minimum_y = minimum[1];
    let maximum_y = maximum[1];
    if !minimum_y.is_finite() || maximum_y <= minimum_y {
        bail!("fitted equipment shell has no usable vertical surface extent");
    }

    let direction = normalize(outward, "equipment", "outward ray")?;
    let position = |index: u32| shell.positions[index as usize].map(f64::from);
    let mut surface_uv = None;
    let mut intersection_count = 0_usize;
    let mut outward_intersection_count = 0_usize;
    let mut maximum_attachment_weight = 0.0_f64;
    // Prefer the belt centre, with nearby height samples providing
    // deterministic recovery when the centre lies exactly on a triangle edge.
    for height_fraction in [0.5_f64, 0.4, 0.6, 0.3, 0.7] {
        let origin = [
            (minimum[0] + maximum[0]) * 0.5,
            minimum_y + (maximum_y - minimum_y) * height_fraction,
            (minimum[2] + maximum[2]) * 0.5,
        ];
        let candidate = shell
            .faces
            .iter()
            .copied()
            .filter_map(|face| {
                let a = position(face[0]);
                let b = position(face[1]);
                let c = position(face[2]);
                let face_normal = cross(subtract(b, a), subtract(c, a));
                let (distance, u, v) = ray_triangle_hit(origin, direction, a, b, c)?;
                intersection_count += 1;
                // From an interior origin, an outward-facing exit is a
                // rendering back-face: its normal points with the ray.
                if dot(face_normal, direction) <= RAY_INTERSECTION_EPSILON {
                    return None;
                }
                outward_intersection_count += 1;
                let weights = face.map(|vertex| attachment_region_weight(mesh, vertex as usize));
                let weight = weights[0] as f64 * (1.0 - u - v)
                    + weights[1] as f64 * u
                    + weights[2] as f64 * v;
                maximum_attachment_weight = maximum_attachment_weight.max(weight);
                (weight >= EQUIPMENT_SOCKET_WEIGHT_THRESHOLD as f64)
                    .then_some((distance, face, [1.0 - u - v, u, v]))
            })
            .max_by(|left, right| left.0.total_cmp(&right.0));
        if let Some((_, face, barycentric)) = candidate {
            let body_face_index = body_faces
                .get(&face_key(face))
                .copied()
                .context("fitted shell face is absent from the MHR body topology")?;
            let body_face = mesh.faces[body_face_index];
            let uv_face = layout.texcoord_faces[body_face_index];
            let mut resolved = [0.0_f64; 2];
            for (shell_corner, vertex) in face.into_iter().enumerate() {
                let body_corner = body_face
                    .iter()
                    .position(|candidate| *candidate == vertex)
                    .context("fitted shell face does not preserve body vertices")?;
                let texcoord = layout
                    .texcoords
                    .get(uv_face[body_corner] as usize)
                    .context("MHR UV face references a missing coordinate")?;
                for axis in 0..2 {
                    resolved[axis] += barycentric[shell_corner] * texcoord[axis] as f64;
                }
            }
            surface_uv = Some(resolved.map(|component| component as f32));
            break;
        }
    }
    surface_uv.with_context(|| {
        format!(
            "fitted equipment shell has no eligible outward surface for UV bootstrapping ({} intersections, {} outward, maximum attachment weight {:.3}); shell bounds {minimum:?}..{maximum:?}",
            intersection_count,
            outward_intersection_count,
            maximum_attachment_weight,
        )
    })
}

fn palmar_surface_distance(
    mesh: &RiggedMesh<'_>,
    side: &str,
    palm_center: [f64; 3],
    palmar_normal: [f64; 3],
    maximum_distance: f64,
) -> Result<f64> {
    let position = |index: u32| mesh.positions[index as usize].map(f64::from);
    mesh.faces
        .iter()
        .filter_map(|face| {
            ray_triangle_distance(
                palm_center,
                palmar_normal,
                position(face[0]),
                position(face[1]),
                position(face[2]),
            )
        })
        .filter(|distance| *distance <= maximum_distance)
        .reduce(f64::min)
        .with_context(|| format!("MHR {side} palm mesh has no surface beneath the socket"))
}

fn weapon_attachment(
    mesh: &RiggedMesh<'_>,
    side: &str,
    wrist: Transform,
    middle: Transform,
    index: Transform,
    ring: Transform,
    thumb: [Transform; 2],
) -> Result<Transform> {
    let palm_length = normalize(
        subtract(middle.translation, wrist.translation),
        side,
        "palm length",
    )?;
    let palm_width = subtract(index.translation, ring.translation);
    let palm_width_length = length(palm_width);
    let mut palmar_normal = normalize(cross(palm_length, palm_width), side, "palm plane")?;
    let knuckle_center = [
        (index.translation[0] + middle.translation[0] + ring.translation[0]) / 3.0,
        (index.translation[1] + middle.translation[1] + ring.translation[1]) / 3.0,
        (index.translation[2] + middle.translation[2] + ring.translation[2]) / 3.0,
    ];
    let palm_center = [
        (wrist.translation[0] + knuckle_center[0]) * 0.5,
        (wrist.translation[1] + knuckle_center[1]) * 0.5,
        (wrist.translation[2] + knuckle_center[2]) * 0.5,
    ];

    // The thumb base disambiguates the palmar side of the skeletal hand plane
    // on both mirrored hands. Offset the socket to that side rather than
    // leaving it embedded halfway through the palm.
    if dot(palmar_normal, subtract(thumb[0].translation, palm_center)) < 0.0 {
        palmar_normal = palmar_normal.map(|component| -component);
    }
    // The weapon's +Y axis is the palm-width axis: exactly perpendicular to
    // the wrist-to-fingers direction. The thumb landmark only chooses which of
    // the two possible width directions is outward for each mirrored hand.
    let mut weapon_y = normalize(cross(palmar_normal, palm_length), side, "palm width")?;
    if dot(weapon_y, subtract(thumb[1].translation, palm_center)) < 0.0 {
        weapon_y = weapon_y.map(|component| -component);
    }
    let weapon_z = palmar_normal;
    let weapon_x = normalize(cross(weapon_y, weapon_z), side, "weapon frame")?;
    let rotation = quat_normalize(quat_from_matrix([
        [weapon_x[0], weapon_y[0], weapon_z[0]],
        [weapon_x[1], weapon_y[1], weapon_z[1]],
        [weapon_x[2], weapon_y[2], weapon_z[2]],
    ]));
    let depth = palmar_surface_distance(mesh, side, palm_center, palmar_normal, palm_width_length)?
        + PALMAR_SOCKET_CLEARANCE_METERS;

    let surface_socket = Transform {
        translation: [
            palm_center[0] + palmar_normal[0] * depth,
            palm_center[1] + palmar_normal[1] * depth,
            palm_center[2] + palmar_normal[2] * depth,
        ],
        rotation,
        scale: wrist.scale,
    };
    Ok(surface_socket.compose(&WEAPON_SOCKET_CALIBRATION))
}

fn append_attachment(
    names: &mut Vec<String>,
    parents: &mut Vec<i32>,
    globals: &mut Vec<Transform>,
    name: &str,
    parent: usize,
    transform: Transform,
) {
    names.push(name.to_owned());
    parents.push(parent as i32);
    globals.push(transform);
}

fn validate(
    mesh: &RiggedMesh<'_>,
    shells: &[RiggedShell<'_>],
    sockets: &[RiggedSocket<'_>],
) -> Result<()> {
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
        if shell.positions.len() != vertices
            || shell.normals.len() != vertices
            || shell.faces.is_empty()
        {
            bail!(
                "clothing shell '{}' must have body-sized positions and normals and at least one face",
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
            || shell
                .normals
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
    let mut socket_ids = std::collections::BTreeSet::new();
    for socket in sockets {
        let state = socket.transform.to_skel_state();
        if socket.attachment_point_id.trim().is_empty()
            || !socket_ids.insert(socket.attachment_point_id)
        {
            bail!("equipment socket attachment-point IDs must be non-empty and unique");
        }
        if state.iter().any(|value| !value.is_finite()) {
            bail!(
                "equipment socket '{}' contains a non-finite transform",
                socket.attachment_point_id
            );
        }
        if socket.surface_uv_domain.trim().is_empty()
            || socket.surface_uv.iter().any(|value| !value.is_finite())
        {
            bail!(
                "equipment socket '{}' has an invalid anatomical surface UV",
                socket.attachment_point_id
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
    sockets: &[RiggedSocket<'_>],
) -> Result<()> {
    validate(mesh, shells, sockets)?;
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
    let exported_faces = if mesh.export_body { mesh.faces } else { &[] };
    let indices = buffer.push(
        &u32_bytes(exported_faces.iter().flatten().copied()),
        Some(34_963),
    );
    let shell_views = shells
        .iter()
        .map(|shell| {
            let positions = buffer.push(
                &f32_bytes(shell.positions.iter().flatten().copied()),
                Some(34_962),
            );
            let normals = buffer.push(
                &f32_bytes(shell.normals.iter().flatten().copied()),
                Some(34_962),
            );
            let indices = buffer.push(
                &u32_bytes(shell.faces.iter().flatten().copied()),
                Some(34_963),
            );
            (positions, normals, indices)
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
    let mut attachments = Vec::new();
    if !mesh.faces.is_empty() {
        let left_wrist = landmark(mesh, "l_wrist")?;
        let left_middle = landmark(mesh, "l_middle1")?;
        let left_index = landmark(mesh, "l_index1")?;
        let left_ring = landmark(mesh, "l_ring1")?;
        let left_thumb_base = landmark(mesh, "l_thumb0")?;
        let left_thumb = landmark(mesh, "l_thumb1")?;
        let right_wrist = landmark(mesh, "r_wrist")?;
        let right_middle = landmark(mesh, "r_middle1")?;
        let right_index = landmark(mesh, "r_index1")?;
        let right_ring = landmark(mesh, "r_ring1")?;
        let right_thumb_base = landmark(mesh, "r_thumb0")?;
        let right_thumb = landmark(mesh, "r_thumb1")?;
        let head = landmark(mesh, "c_head")?;
        let left_eye = landmark(mesh, "l_eye")?;
        let right_eye = landmark(mesh, "r_eye")?;
        let left_grip = weapon_attachment(
            mesh,
            "left",
            globals[left_wrist],
            globals[left_middle],
            globals[left_index],
            globals[left_ring],
            [globals[left_thumb_base], globals[left_thumb]],
        )?;
        let right_grip = weapon_attachment(
            mesh,
            "right",
            globals[right_wrist],
            globals[right_middle],
            globals[right_index],
            globals[right_ring],
            [globals[right_thumb_base], globals[right_thumb]],
        )?;
        let camera_position = between(globals[left_eye], globals[right_eye], 0.5);
        let camera = Transform {
            translation: camera_position,
            rotation: globals[head].rotation,
            scale: globals[head].scale,
        };
        append_attachment(
            &mut joint_names,
            &mut joint_parents,
            &mut globals,
            LEFT_WEAPON_JOINT,
            left_wrist,
            left_grip,
        );
        append_attachment(
            &mut joint_names,
            &mut joint_parents,
            &mut globals,
            RIGHT_WEAPON_JOINT,
            right_wrist,
            right_grip,
        );
        append_attachment(
            &mut joint_names,
            &mut joint_parents,
            &mut globals,
            FIRST_PERSON_CAMERA_JOINT,
            head,
            camera,
        );
        attachments = vec![
            json!({"name": LEFT_WEAPON_JOINT, "parent": "l_wrist", "role": "left_weapon_grip"}),
            json!({"name": RIGHT_WEAPON_JOINT, "parent": "r_wrist", "role": "right_weapon_grip"}),
            json!({"name": FIRST_PERSON_CAMERA_JOINT, "parent": "c_head", "role": "first_person_camera"}),
        ];
    }
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
    let index_accessor = accessor(indices, 5_125, exported_faces.len() * 3, "SCALAR", None);
    let inverse_bind_accessor = accessor(
        inverse_bind_matrices,
        5_126,
        joint_names.len(),
        "MAT4",
        None,
    );
    let mut primitives = Vec::new();
    let mut material_values = Vec::new();
    if mesh.export_body {
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
    for (shell, (position_view, normal_view, index_view)) in shells.iter().zip(shell_views) {
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
        let shell_normal_accessor = accessor(normal_view, 5_126, shell.normals.len(), "VEC3", None);
        let material = material_values.len();
        primitives.push(json!({
            "attributes": {
                "POSITION": shell_position_accessor,
                "NORMAL": shell_normal_accessor,
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
    let mut nodes = Vec::with_capacity(joint_names.len() + 2 + sockets.len());
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
    let socket_parent = if sockets.is_empty() {
        None
    } else {
        Some(
            mesh.joint_names
                .iter()
                .position(|name| name == "root")
                .context("MHR skeleton has no anatomical pelvis joint")?,
        )
    };
    let socket_nodes = sockets
        .iter()
        .map(|socket| {
            let node = nodes.len();
            let state = socket.transform.to_skel_state();
            nodes.push(json!({
                "name": format!("{EQUIPMENT_SOCKET_NODE_PREFIX}{}", socket.attachment_point_id),
                "translation": [state[0], state[1], state[2]],
                "rotation": [state[3], state[4], state[5], state[6]],
                "scale": [state[7], state[7], state[7]],
                "extras": {
                    "adventuresim_equipment_socket": {
                        "attachment_point_id": socket.attachment_point_id,
                        "space": "pelvis_local",
                        "surface_uv": {
                            "domain": socket.surface_uv_domain,
                            "uv": socket.surface_uv
                        },
                        "tangent_axis": "+Y",
                        "normal_axis": "+Z"
                    }
                }
            }));
            node
        })
        .collect::<Vec<_>>();
    if let Some(socket_parent) = socket_parent {
        let root_children = nodes[socket_parent]
            .as_object_mut()
            .context("MHR anatomical pelvis node is not an object")?
            .entry("children")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .context("MHR anatomical pelvis children are not an array")?;
        root_children.extend(socket_nodes.iter().copied().map(Value::from));
    }
    let scene_nodes = vec![skeleton_node, mesh_node];
    let mut extras = json!({
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
            "attachments": attachments,
        },
    });
    if !sockets.is_empty() {
        extras["adventuresim_equipment"] = json!({
            "attachment_sockets": sockets.iter().map(|socket| json!({
                "attachment_point_id": socket.attachment_point_id,
                "node": format!("{EQUIPMENT_SOCKET_NODE_PREFIX}{}", socket.attachment_point_id),
                "space": "pelvis_local",
                "surface_uv": {
                    "domain": socket.surface_uv_domain,
                    "uv": socket.surface_uv
                },
                "tangent_axis": "+Y",
                "normal_axis": "+Z"
            })).collect::<Vec<_>>(),
        });
    }
    let document = json!({
        "asset": {"version": "2.0", "generator": "Fabelgeist MHR character creator"},
        "scene": 0,
        "scenes": [{"name": "Character", "nodes": scene_nodes}],
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
        "extras": extras
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
            "l_index1",
            "l_ring1",
            "l_thumb0",
            "l_thumb1",
            "r_wrist",
            "r_middle1",
            "r_index1",
            "r_ring1",
            "r_thumb0",
            "r_thumb1",
            "c_head",
            "l_eye",
            "r_eye",
        ]
        .map(str::to_owned)
        .to_vec();
        let parents = vec![-1, 0, 1, 1, 1, 1, 5, 0, 7, 7, 7, 7, 11, 0, 13, 13];
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
            transform([-0.45, 1.0, 0.2]),
            transform([-0.55, 1.0, 0.2]),
            transform([-0.44, 0.98, 0.06]),
            transform([-0.4, 0.98, 0.08]),
            transform([0.5, 1.0, 0.0]),
            transform([0.5, 1.0, 0.2]),
            transform([0.45, 1.0, 0.2]),
            transform([0.55, 1.0, 0.2]),
            transform([0.44, 0.98, 0.06]),
            transform([0.4, 0.98, 0.08]),
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
        let positions = [[-2.0, 0.98, -1.0], [2.0, 0.98, -1.0], [0.0, 0.98, 2.0]];
        let normals = [[0.0, -1.0, 0.0]; 3];
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
                export_body: true,
                joint_indices: &joint_indices,
                joint_weights: &joint_weights,
                joint_names: &joint_names,
                joint_parents: &joint_parents,
                global_joint_states: &global_joint_states,
            },
            &[],
            &[],
        )
        .unwrap();
        let bytes = fs::read(&path).unwrap();
        let document = read_document(&bytes);
        let parsed = gltf::Gltf::from_slice(&bytes).unwrap();
        assert_eq!(&bytes[..4], GLB_MAGIC);
        assert_eq!(parsed.skins().count(), 1);
        assert_eq!(parsed.meshes().count(), 1);
        assert_eq!(document["skins"][0]["joints"].as_array().unwrap().len(), 19);
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
        for (attachment, thumb_sign) in [(left_weapon, 1.0), (right_weapon, -1.0)] {
            let translation = nodes[attachment]["translation"].as_array().unwrap();
            let rotation = nodes[attachment]["rotation"]
                .as_array()
                .unwrap()
                .iter()
                .map(|component| component.as_f64().unwrap())
                .collect::<Vec<_>>();
            let rotation: [f64; 4] = rotation.try_into().unwrap();
            let weapon_y = [thumb_sign, 0.0, 0.0];
            let weapon_z = [0.0, -1.0, 0.0];
            let weapon_x = cross(weapon_y, weapon_z);
            let surface_rotation = quat_normalize(quat_from_matrix([
                [weapon_x[0], weapon_y[0], weapon_z[0]],
                [weapon_x[1], weapon_y[1], weapon_z[1]],
                [weapon_x[2], weapon_y[2], weapon_z[2]],
            ]));
            let wrist_translation = [-thumb_sign * 0.5, 1.0, 0.0];
            let expected_global = Transform {
                translation: [wrist_translation[0], 0.978, 0.1],
                rotation: surface_rotation,
                scale: 1.0,
            }
            .compose(&WEAPON_SOCKET_CALIBRATION);
            let expected_local = Transform {
                translation: wrist_translation,
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: 1.0,
            }
            .inverse()
            .compose(&expected_global);
            for (actual, expected) in translation.iter().zip(expected_local.translation) {
                assert!((actual.as_f64().unwrap() - expected).abs() < 1e-6);
            }
            let rotation_alignment = rotation
                .iter()
                .zip(expected_local.rotation)
                .map(|(actual, expected)| actual * expected)
                .sum::<f64>();
            assert!(rotation_alignment.abs() > 1.0 - 1e-6);
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
                export_body: true,
                joint_indices: &joint_indices,
                joint_weights: &joint_weights,
                joint_names: &joint_names,
                joint_parents: &joint_parents,
                global_joint_states: &global_joint_states,
            },
            &[],
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
        let positions = [[-2.0, 0.98, -1.0], [2.0, 0.98, -1.0], [0.0, 0.98, 2.0]];
        let shell_positions = [[-2.0, 0.99, -1.0], [2.0, 0.99, -1.0], [0.0, 0.99, 2.0]];
        let normals = [[0.0, -1.0, 0.0]; 3];
        let faces = [[0, 1, 2]];
        let joint_indices = [[0; 8]; 3];
        let joint_weights = [[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]; 3];
        let (joint_names, joint_parents, global_joint_states) = attachment_test_skeleton();
        let shell = RiggedShell {
            name: "Tunic",
            positions: &shell_positions,
            normals: &normals,
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
                export_body: true,
                joint_indices: &joint_indices,
                joint_weights: &joint_weights,
                joint_names: &joint_names,
                joint_parents: &joint_parents,
                global_joint_states: &global_joint_states,
            },
            &[shell],
            &[],
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
        assert_ne!(
            document["meshes"][0]["primitives"][1]["attributes"]["NORMAL"],
            document["meshes"][0]["primitives"][0]["attributes"]["NORMAL"]
        );
        assert_eq!(
            document["meshes"][0]["primitives"][1]["attributes"]["JOINTS_1"],
            document["meshes"][0]["primitives"][0]["attributes"]["JOINTS_1"]
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn exports_shell_only_equipment_without_character_attachment_geometry() {
        let directory = std::env::temp_dir().join(format!(
            "fabelgeist-mhr-equipment-export-{}",
            std::process::id()
        ));
        let path = directory.join("belt.glb");
        let positions = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let normals = [[0.0, 0.0, 1.0]; 3];
        let joint_indices = [[1; 8]; 3];
        let joint_weights = [[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]; 3];
        let joint_names = ["body_world".to_owned(), "root".to_owned()];
        let joint_parents = [-1, 0];
        let global_joint_states = [
            [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0],
            [0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0],
        ];
        let shell_faces = [[0, 1, 2]];
        let shell = RiggedShell {
            name: "Leather belt",
            positions: &positions,
            normals: &normals,
            faces: &shell_faces,
            base_color: [0.5, 0.35, 0.23, 1.0],
            metallic: 0.0,
            roughness: 0.58,
        };
        let socket = RiggedSocket {
            attachment_point_id: "left",
            surface_uv_domain: MHR_ANATOMICAL_UV_DOMAIN,
            surface_uv: [0.37, 0.71],
            transform: Transform {
                translation: [-0.25, 0.75, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: 1.0,
            },
        };

        export_rigged_glb(
            &path,
            "leather_belt",
            1,
            1,
            &RiggedMesh {
                positions: &positions,
                normals: &normals,
                faces: &[],
                export_body: false,
                joint_indices: &joint_indices,
                joint_weights: &joint_weights,
                joint_names: &joint_names,
                joint_parents: &joint_parents,
                global_joint_states: &global_joint_states,
            },
            &[shell],
            &[socket],
        )
        .unwrap();

        let bytes = fs::read(&path).unwrap();
        let document = read_document(&bytes);
        assert_eq!(
            document["meshes"][0]["primitives"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(document["skins"][0]["joints"].as_array().unwrap().len(), 2);
        assert_eq!(
            document["extras"]["adventuresim_equipment"]["attachment_sockets"][0]
                ["attachment_point_id"],
            "left"
        );
        let exported_surface = &document["extras"]["adventuresim_equipment"]
            ["attachment_sockets"][0]["surface_uv"];
        assert_eq!(exported_surface["domain"], MHR_ANATOMICAL_UV_DOMAIN);
        for (actual, expected) in exported_surface["uv"]
            .as_array()
            .unwrap()
            .iter()
            .zip([0.37, 0.71])
        {
            assert!((actual.as_f64().unwrap() - expected).abs() < 1e-6);
        }
        let nodes = document["nodes"].as_array().unwrap();
        let socket_node = nodes
            .iter()
            .position(|node| node["name"] == format!("{EQUIPMENT_SOCKET_NODE_PREFIX}left"))
            .expect("exported pelvis-local socket node");
        assert_eq!(nodes[socket_node]["translation"], json!([-0.25, 0.75, 0.0]));
        assert!(nodes[1]["children"]
            .as_array()
            .unwrap()
            .contains(&json!(socket_node)));
        assert!(!document["scenes"][0]["nodes"]
            .as_array()
            .unwrap()
            .contains(&json!(socket_node)));
        assert_eq!(
            document["extras"]["adventuresim_rig"]["attachments"],
            json!([])
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn fitted_socket_resolves_uv_per_lod_and_bootstrap_recovers_the_anchor() {
        let positions = [
            [-2.0, 0.0, -1.0],
            [-2.0, 2.0, -1.0],
            [-2.0, 2.0, 1.0],
            [-2.0, 0.0, 1.0],
            [2.0, 0.0, -1.0],
            [2.0, 2.0, -1.0],
            [2.0, 2.0, 1.0],
            [2.0, 0.0, 1.0],
            // A farther low hand-like surface must not steal the hip socket.
            [-4.0, 0.0, -1.0],
            [-4.0, 2.0, 1.0],
            [-4.0, 2.0, -1.0],
            [-4.0, 0.0, 1.0],
        ];
        let faces = [
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [3, 7, 6],
            [3, 6, 2],
            [8, 9, 10],
            [8, 11, 9],
        ];
        let normals = [
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
        ];
        let texcoords = [
            [0.0, 0.0],
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0],
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
            [1.0, 0.0],
        ];
        let mut joint_indices = [[0; 8]; 12];
        for indices in &mut joint_indices[8..] {
            indices[0] = 1;
        }
        let joint_weights = [[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]; 12];
        let joint_names = ["root".to_owned(), "l_wrist".to_owned()];
        let joint_parents = [-1, 0];
        let global_joint_states = [
            [0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0],
            [-4.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0],
        ];
        let mesh = RiggedMesh {
            positions: &positions,
            normals: &normals,
            faces: &faces,
            export_body: false,
            joint_indices: &joint_indices,
            joint_weights: &joint_weights,
            joint_names: &joint_names,
            joint_parents: &joint_parents,
            global_joint_states: &global_joint_states,
        };
        let shell = RiggedShell {
            name: "Belt",
            positions: &positions,
            normals: &normals,
            faces: &faces,
            base_color: [0.5, 0.3, 0.2, 1.0],
            metallic: 0.0,
            roughness: 0.8,
        };
        let layout = SurfaceUvLayout {
            domain: MHR_ANATOMICAL_UV_DOMAIN,
            texcoords: &texcoords,
            texcoord_faces: &faces,
        };
        let tangent = [
            0.0,
            -35.0_f32.to_radians().cos(),
            -35.0_f32.to_radians().sin(),
        ];

        let socket = fitted_equipment_socket_from_uv(
            &mesh,
            &shell,
            &layout,
            [0.5, 0.5],
            [-1.0, 0.0, 0.0],
            tangent,
        )
        .expect("weighted left hip UV should resolve");
        assert!((socket.translation[0] + 2.003).abs() < 1e-6);
        assert!((socket.translation[1] - 0.5).abs() < 1e-6);
        let tangent_tip = socket.compose(&Transform {
            translation: [0.0, 1.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: 1.0,
        });
        let resolved_tangent = subtract(tangent_tip.translation, socket.translation);
        for (actual, expected) in resolved_tangent.into_iter().zip(tangent.map(f64::from)) {
            assert!((actual - expected).abs() < 1e-6);
        }
        // The opposite end of the same axis is the hilt: forward (+Z) and up
        // in the tactical character frame.
        assert!(-resolved_tangent[1] > 0.0);
        assert!(-resolved_tangent[2] > 0.0);
        let bootstrapped =
            bootstrap_equipment_surface_uv(&mesh, &shell, &layout, [-1.0, 0.0, 0.0])
                .expect("legacy ray should bootstrap the same UV anchor");
        assert!((bootstrapped[0] - 0.5).abs() < 1e-6);
        assert!((bootstrapped[1] - 0.5).abs() < 1e-6);

        let open_shell = RiggedShell {
            name: "Invalid open belt",
            positions: &positions,
            normals: &normals,
            faces: &faces[..4],
            base_color: [0.5, 0.3, 0.2, 1.0],
            metallic: 0.0,
            roughness: 0.8,
        };
        assert!(
            fitted_equipment_socket_from_uv(
                &mesh,
                &open_shell,
                &layout,
                [0.5, 0.5],
                [0.0, 0.0, -1.0],
                [0.0, 1.0, 0.0],
            )
            .unwrap_err()
            .to_string()
            .contains("did not resolve on fitted shell")
        );
    }
}
