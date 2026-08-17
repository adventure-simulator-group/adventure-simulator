use std::collections::HashMap;

use crate::model_def::parameter_limit::ParameterLimit;

/// The linear map from model parameters to joint parameters.
///
/// `joint_parameters = transform * model_parameters + offsets`, with
/// `transform` stored row-major as `[num_joints * 7, names.len()]`.
#[derive(Debug, Default, Clone)]
pub struct ParameterTransform {
    /// Model parameter names, in the order the `.model` file introduces them.
    pub names: Vec<String>,
    pub transform: Vec<f32>,
    pub offsets: Vec<f32>,
    /// Joint channels that any parameter drives.
    pub active_joint_parameters: Vec<bool>,
    /// Named parameter subsets from `[ParameterSets]`.
    pub parameter_sets: HashMap<String, Vec<bool>>,
    pub limits: Vec<ParameterLimit>,
    pub num_joint_parameters: usize,
}

impl ParameterTransform {
    pub fn num_parameters(&self) -> usize {
        self.names.len()
    }

    pub fn parameter_index(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    /// Row `joint * 7 + channel` of the transform matrix.
    pub fn row(&self, joint_parameter: usize) -> &[f32] {
        let stride = self.num_parameters();
        &self.transform[joint_parameter * stride..(joint_parameter + 1) * stride]
    }

    /// Clamps model parameters to the `[Limits]` section's `minmax` bounds.
    pub fn apply_limits(&self, parameters: &mut [f32]) {
        for limit in &self.limits {
            if let Some(value) = parameters.get_mut(limit.parameter) {
                *value = value.clamp(limit.min, limit.max);
            }
        }
    }
}
