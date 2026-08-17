//! The MHR body model: identity, pose and expression parameters in, posed
//! mesh vertices and a skeleton state out.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use burn::tensor::ops::IndexingUpdateOp;
use burn::tensor::{Device, Int, Tensor, TensorData};

use crate::character::{Character, PARAMETERS_PER_JOINT};
use crate::model::mhr_config::MhrConfig;
use crate::model::mhr_output::MhrOutput;
use crate::model_def::{
    ParameterTransform, append_blend_shape_parameters, parse_model_definition,
};
use crate::pose_correctives::PoseCorrectives;
use crate::skel_state;

/// Shape coefficients: 20 body, 20 head, 5 hands.
pub const NUM_IDENTITY_BLEND_SHAPES: usize = 45;
/// Facial expression coefficients.
pub const NUM_FACE_EXPRESSION_BLEND_SHAPES: usize = 72;
/// Total blend shapes carried by an MHR rig.
pub const NUM_BLEND_SHAPES: usize = NUM_IDENTITY_BLEND_SHAPES + NUM_FACE_EXPRESSION_BLEND_SHAPES;

/// Level of detail, 0 (densest) through 6.
pub const MAX_LOD: u8 = 6;

const MODEL_DEFINITION: &str = "compact_v6_1.model";
const CORRECTIVE_ACTIVATION: &str = "corrective_activation.npz";

/// The MHR body model.
pub struct Mhr {
    /// Topology and names, kept on the host for export and lookups.
    pub character: Character,
    pub parameter_transform: ParameterTransform,
    device: Device,

    /// `[model parameters, joints * 7]`, transposed for a right-hand matmul.
    transform: Tensor<2>,
    /// Constant joint-parameter offsets, or `None` when the rig has none.
    offsets: Option<Tensor<2>>,
    num_model_parameters: usize,

    /// `[1, joints, 3]` and `[1, joints, 4]`.
    joint_translation_offsets: Tensor<3>,
    joint_prerotations: Tensor<3>,
    /// `[1, joints, 8]`.
    inverse_bind_pose: Tensor<3>,
    /// Levels of the forward-kinematics prefix scan.
    fk_levels: Vec<(Tensor<1, Int>, Tensor<1, Int>)>,

    /// `[45, vertices * 3]` and `[72, vertices * 3]`.
    identity_basis: Tensor<2>,
    expression_basis: Tensor<2>,
    /// `[1, vertices * 3]`.
    base_shape: Tensor<2>,

    /// The three corners of every triangle, flattened, `[faces * 3]`.
    face_corners: Tensor<1, Int>,

    /// Flattened non-zero skinning influences.
    skin_joint_indices: Tensor<1, Int>,
    skin_vertex_indices: Tensor<1, Int>,
    /// `[1, influences, 1]`.
    skin_weights: Tensor<3>,

    correctives: Option<PoseCorrectives>,
}

/// Accepts either the asset directory itself or its parent.
fn resolve_asset_dir(path: &Path) -> Result<PathBuf> {
    if path.join(MODEL_DEFINITION).is_file() {
        return Ok(path.to_path_buf());
    }
    let nested = path.join("assets");
    if nested.join(MODEL_DEFINITION).is_file() {
        return Ok(nested);
    }
    bail!(
        "no MHR assets in {}: expected {MODEL_DEFINITION} there or under assets/",
        path.display()
    )
}

impl Mhr {
    /// Loads a model from an MHR asset directory (the unpacked `assets.zip`).
    pub fn from_files(
        asset_dir: impl AsRef<Path>,
        config: MhrConfig,
        device: &Device,
    ) -> Result<Self> {
        if config.lod > MAX_LOD {
            bail!("LOD {} is out of range 0..={MAX_LOD}", config.lod);
        }
        let dir = resolve_asset_dir(asset_dir.as_ref())?;

        let fbx_path = dir.join(format!("lod{}.fbx", config.lod));
        let fbx =
            std::fs::read(&fbx_path).with_context(|| format!("reading {}", fbx_path.display()))?;
        let character = Character::from_fbx_bytes(&fbx, true)
            .with_context(|| format!("loading {}", fbx_path.display()))?;
        let definition_path = dir.join(MODEL_DEFINITION);
        let definition = std::fs::read_to_string(&definition_path)
            .with_context(|| format!("reading {}", definition_path.display()))?;
        let correctives = if config.pose_correctives {
            PoseCorrectives::load(
                &dir.join(CORRECTIVE_ACTIVATION),
                &dir.join(format!("corrective_blendshapes_lod{}.npz", config.lod)),
                character.skeleton.len(),
                character.mesh.vertices.len(),
                device,
            )
            .context("loading the pose-corrective network")?
        } else {
            None
        };
        Self::from_loaded_assets(character, &definition, correctives, device)
    }

    /// Loads MHR from asset bytes, allowing virtual filesystems to
    /// provide the same FBX/model/NPZ inputs as the native filesystem loader.
    pub fn from_asset_bytes(
        fbx: &[u8],
        definition: &str,
        corrective_archives: Option<(Vec<u8>, Vec<u8>)>,
        config: MhrConfig,
        device: &Device,
    ) -> Result<Self> {
        if config.lod > MAX_LOD {
            bail!("LOD {} is out of range 0..={MAX_LOD}", config.lod);
        }
        let character = Character::from_fbx_bytes(fbx, true).context("loading the MHR FBX")?;
        let correctives = if config.pose_correctives {
            let (activation, basis) = corrective_archives
                .context("pose correctives requested but their NPZ archives were not provided")?;
            PoseCorrectives::from_bytes(
                activation,
                basis,
                character.skeleton.len(),
                character.mesh.vertices.len(),
                device,
            )
            .context("loading the pose-corrective network")?
        } else {
            None
        };

        Self::from_loaded_assets(character, definition, correctives, device)
    }

    fn from_loaded_assets(
        character: Character,
        definition: &str,
        correctives: Option<PoseCorrectives>,
        device: &Device,
    ) -> Result<Self> {
        let mut parameter_transform = parse_model_definition(definition, &character.skeleton)
            .context("parsing the MHR model definition")?;
        let num_model_parameters = parameter_transform.num_parameters();
        // momentum appends one model parameter per identity blend shape when a
        // blend shape is attached to the character.
        append_blend_shape_parameters(&mut parameter_transform, NUM_IDENTITY_BLEND_SHAPES);
        Self::new(
            character,
            parameter_transform,
            num_model_parameters,
            correctives,
            device,
        )
    }

    fn new(
        character: Character,
        parameter_transform: ParameterTransform,
        num_model_parameters: usize,
        correctives: Option<PoseCorrectives>,
        device: &Device,
    ) -> Result<Self> {
        let joints = character.skeleton.len();
        let vertices = character.mesh.vertices.len();

        if character.blend_shapes.len() != NUM_BLEND_SHAPES {
            bail!(
                "rig has {} blend shapes, expected {NUM_IDENTITY_BLEND_SHAPES} identity plus \
                 {NUM_FACE_EXPRESSION_BLEND_SHAPES} expression",
                character.blend_shapes.len()
            );
        }

        // Transposed so a forward pass is `parameters @ transform`.
        let columns = parameter_transform.num_parameters();
        let mut transform = vec![0.0f32; num_model_parameters * joints * PARAMETERS_PER_JOINT];
        for row in 0..joints * PARAMETERS_PER_JOINT {
            for column in 0..num_model_parameters {
                transform[column * joints * PARAMETERS_PER_JOINT + row] =
                    parameter_transform.transform[row * columns + column];
            }
        }

        let offsets = parameter_transform
            .offsets
            .iter()
            .any(|offset| *offset != 0.0)
            .then(|| {
                Tensor::from_data(
                    TensorData::new(
                        parameter_transform.offsets.clone(),
                        [1, joints * PARAMETERS_PER_JOINT],
                    ),
                    device,
                )
            });

        let fk_levels = skel_state::prefix_multiplication_levels(&character.skeleton.parents)
            .into_iter()
            .map(|(source, target)| {
                let len = source.len();
                (
                    Tensor::from_data(TensorData::new(source, [len]), device),
                    Tensor::from_data(TensorData::new(target, [len]), device),
                )
            })
            .collect();

        let basis = &character.blend_shapes.vectors;
        let identity_basis = Tensor::from_data(
            TensorData::new(
                basis[..NUM_IDENTITY_BLEND_SHAPES * vertices * 3].to_vec(),
                [NUM_IDENTITY_BLEND_SHAPES, vertices * 3],
            ),
            device,
        );
        let expression_basis = Tensor::from_data(
            TensorData::new(
                basis[NUM_IDENTITY_BLEND_SHAPES * vertices * 3..].to_vec(),
                [NUM_FACE_EXPRESSION_BLEND_SHAPES, vertices * 3],
            ),
            device,
        );

        // Drop zero influences: MHR averages under three joints per vertex.
        let mut skin_joint_indices = Vec::new();
        let mut skin_vertex_indices = Vec::new();
        let mut skin_weights = Vec::new();
        for (vertex, (indices, weights)) in character
            .skin_weights
            .index
            .iter()
            .zip(&character.skin_weights.weight)
            .enumerate()
        {
            for (joint, weight) in indices.iter().zip(weights) {
                if *weight > 1e-5 {
                    skin_joint_indices.push(*joint as i32);
                    skin_vertex_indices.push(vertex as i32);
                    skin_weights.push(*weight);
                }
            }
        }
        let influences = skin_weights.len();

        let face_corners: Vec<i32> = character
            .mesh
            .faces
            .iter()
            .flatten()
            .map(|corner| *corner as i32)
            .collect();
        let num_corners = face_corners.len();

        Ok(Self {
            device: device.clone(),
            transform: Tensor::from_data(
                TensorData::new(
                    transform,
                    [num_model_parameters, joints * PARAMETERS_PER_JOINT],
                ),
                device,
            ),
            offsets,
            num_model_parameters,
            joint_translation_offsets: Tensor::from_data(
                TensorData::new(
                    character.skeleton.translation_offsets.concat(),
                    [1, joints, 3],
                ),
                device,
            ),
            joint_prerotations: Tensor::from_data(
                TensorData::new(character.skeleton.prerotations.concat(), [1, joints, 4]),
                device,
            ),
            inverse_bind_pose: Tensor::from_data(
                TensorData::new(character.inverse_bind_pose.concat(), [1, joints, 8]),
                device,
            ),
            fk_levels,
            identity_basis,
            expression_basis,
            base_shape: Tensor::from_data(
                TensorData::new(character.mesh.vertices.concat(), [1, vertices * 3]),
                device,
            ),
            face_corners: Tensor::from_data(TensorData::new(face_corners, [num_corners]), device),
            skin_joint_indices: Tensor::from_data(
                TensorData::new(skin_joint_indices, [influences]),
                device,
            ),
            skin_vertex_indices: Tensor::from_data(
                TensorData::new(skin_vertex_indices, [influences]),
                device,
            ),
            skin_weights: Tensor::from_data(
                TensorData::new(skin_weights, [1, influences, 1]),
                device,
            ),
            correctives,
            character,
            parameter_transform,
        })
    }

    pub fn num_joints(&self) -> usize {
        self.character.skeleton.len()
    }

    pub fn num_vertices(&self) -> usize {
        self.character.mesh.vertices.len()
    }

    /// Number of pose/scale parameters the model takes, excluding the blend
    /// shape coefficients momentum appends to the parameter vector.
    pub fn num_model_parameters(&self) -> usize {
        self.num_model_parameters
    }

    pub fn has_pose_correctives(&self) -> bool {
        self.correctives.is_some()
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    /// Zero parameters for a batch, i.e. the rest pose.
    pub fn zero_parameters(&self, batch: usize) -> Tensor<2> {
        Tensor::zeros([batch, self.num_model_parameters], &self.device)
    }

    /// Runs the model.
    ///
    /// * `identity` — `[batch, 45]` shape coefficients (a single row is broadcast).
    /// * `model_parameters` — `[batch, 204]` pose and scale parameters.
    /// * `expression` — `[batch, 72]` facial expression coefficients, optional.
    pub fn forward(
        &self,
        identity: Tensor<2>,
        model_parameters: Tensor<2>,
        expression: Option<Tensor<2>>,
    ) -> Result<MhrOutput> {
        self.forward_with(identity, model_parameters, expression, true)
    }

    /// As [`Mhr::forward`], but able to skip the pose correctives.
    pub fn forward_with(
        &self,
        identity: Tensor<2>,
        model_parameters: Tensor<2>,
        expression: Option<Tensor<2>>,
        apply_correctives: bool,
    ) -> Result<MhrOutput> {
        let batch = model_parameters.dims()[0];
        let joints = self.num_joints();
        let vertices = self.num_vertices();

        let [identity_rows, identity_columns] = identity.dims();
        if identity_columns != NUM_IDENTITY_BLEND_SHAPES {
            bail!(
                "identity coefficients have {identity_columns} columns, expected {NUM_IDENTITY_BLEND_SHAPES}"
            );
        }
        if identity_rows != batch && identity_rows != 1 {
            bail!("identity coefficients have {identity_rows} rows, expected {batch} or 1");
        }
        if model_parameters.dims()[1] != self.num_model_parameters {
            bail!(
                "model parameters have {} columns, expected {}",
                model_parameters.dims()[1],
                self.num_model_parameters
            );
        }

        // Rest shape: mean plus identity and expression offsets.
        let identity = if identity_rows == batch {
            identity
        } else {
            identity.expand([batch, NUM_IDENTITY_BLEND_SHAPES])
        };
        let mut rest = self.base_shape.clone() + identity.matmul(self.identity_basis.clone());
        if let Some(expression) = expression {
            let [rows, columns] = expression.dims();
            if columns != NUM_FACE_EXPRESSION_BLEND_SHAPES {
                bail!(
                    "expression coefficients have {columns} columns, expected {NUM_FACE_EXPRESSION_BLEND_SHAPES}"
                );
            }
            if rows != batch {
                bail!("expression coefficients have {rows} rows, expected {batch}");
            }
            rest = rest + expression.matmul(self.expression_basis.clone());
        }

        let mut joint_parameters = (model_parameters.unsqueeze_dim::<3>(2)
            * self.transform.clone().unsqueeze::<3>())
        .sum_dim(1)
        .reshape([batch, joints * PARAMETERS_PER_JOINT]);
        if let Some(offsets) = &self.offsets {
            joint_parameters = joint_parameters + offsets.clone();
        }
        let joint_parameters = joint_parameters.reshape([batch, joints, PARAMETERS_PER_JOINT]);

        let skeleton_state = self.skeleton_state(joint_parameters.clone());

        let mut rest = rest.reshape([batch, vertices, 3]);
        if apply_correctives && let Some(correctives) = &self.correctives {
            rest = rest + correctives.forward(joint_parameters);
        }

        let vertices = self.skin(skeleton_state.clone(), rest);

        Ok(MhrOutput {
            normals: self.vertex_normals(vertices.clone()),
            vertices,
            skeleton_state,
        })
    }

    /// Joint parameters `[batch, joints, 7]` to global skeleton states.
    fn skeleton_state(&self, joint_parameters: Tensor<3>) -> Tensor<3> {
        let translation =
            joint_parameters.clone().narrow(2, 0, 3) + self.joint_translation_offsets.clone();
        let rotation = skel_state::quaternion_multiply(
            self.joint_prerotations.clone(),
            skel_state::euler_xyz_to_quaternion(joint_parameters.clone().narrow(2, 3, 3)),
        );
        // momentum stores scale as a power of two.
        let scale = (joint_parameters.narrow(2, 6, 1) * std::f32::consts::LN_2).exp();

        let mut state = Tensor::cat(vec![translation, rotation, scale], 2);
        for (source, target) in &self.fk_levels {
            let parent = state.clone().select(1, target.clone());
            let child = state.clone().select(1, source.clone());
            let delta = skel_state::multiply(parent, child.clone()) - child;
            state = state.select_assign(1, source.clone(), delta, IndexingUpdateOp::Add);
        }
        state
    }

    /// Area-weighted vertex normals of a posed mesh, `[batch, vertices, 3]`.
    ///
    /// Each triangle contributes its unnormalized cross product, whose length
    /// is twice the triangle's area, to all three of its corners. That is the
    /// weighting `THREE.BufferGeometry.computeVertexNormals` uses, which is
    /// what the reference web viewer shades MHR with.
    fn vertex_normals(&self, vertices: Tensor<3>) -> Tensor<3> {
        let batch = vertices.dims()[0];
        let faces = self.character.mesh.faces.len();

        let corners = vertices
            .select(1, self.face_corners.clone())
            .reshape([batch, faces, 3, 3]);
        let corner = |index: usize| {
            corners
                .clone()
                .narrow(2, index, 1)
                .reshape([batch, faces, 3])
        };
        let (first, second, third) = (corner(0), corner(1), corner(2));
        let (u, v) = (second - first.clone(), third - first);

        let axis = |t: &Tensor<3>, index: usize| t.clone().narrow(2, index, 1);
        let cross = Tensor::cat(
            vec![
                axis(&u, 1) * axis(&v, 2) - axis(&u, 2) * axis(&v, 1),
                axis(&u, 2) * axis(&v, 0) - axis(&u, 0) * axis(&v, 2),
                axis(&u, 0) * axis(&v, 1) - axis(&u, 1) * axis(&v, 0),
            ],
            2,
        );

        // The same face normal lands on each of the face's three corners.
        let contribution = cross
            .unsqueeze_dim::<4>(2)
            .expand([batch, faces, 3, 3])
            .reshape([batch, faces * 3, 3]);
        let accumulated = Tensor::zeros([batch, self.num_vertices(), 3], &self.device)
            .select_assign(
                1,
                self.face_corners.clone(),
                contribution,
                IndexingUpdateOp::Add,
            );

        // A vertex no triangle references keeps a zero normal, as it does in
        // three.js; nothing rasterizes it, so there is no direction to invent.
        let length = accumulated
            .clone()
            .powi_scalar(2)
            .sum_dim(2)
            .sqrt()
            .clamp_min(1e-12);
        accumulated / length
    }

    /// Linear blend skinning of `rest` `[batch, vertices, 3]`.
    fn skin(&self, skeleton_state: Tensor<3>, rest: Tensor<3>) -> Tensor<3> {
        let batch = rest.dims()[0];
        let joint_state = skel_state::multiply(skeleton_state, self.inverse_bind_pose.clone());

        let transforms = joint_state.select(1, self.skin_joint_indices.clone());
        let points = rest.clone().select(1, self.skin_vertex_indices.clone());
        let deformed =
            skel_state::transform_points(&transforms, points) * self.skin_weights.clone();

        Tensor::zeros(rest.dims(), &self.device)
            .select_assign(
                1,
                self.skin_vertex_indices.clone(),
                deformed,
                IndexingUpdateOp::Add,
            )
            .reshape([batch, self.num_vertices(), 3])
    }
}
