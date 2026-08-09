use bevy::prelude::*;
use bevy_animation_graph::core::{
    animation_node::{NodeLike, ReflectNodeLike},
    context::{new_context::NodeContext, spec_context::SpecContext},
    edge_data::{DataSpec, DataValue, bone_mask::BoneMask},
    errors::GraphError,
    interpolation::linear::LinearInterpolator,
};

/// Non-trivially composes two sparse semantic poses inside the dependency
/// runtime. Adventure Simulator supplies normalized factors; the graph-returned
/// pose is decoded into the authored FK sample weights.
#[derive(Reflect, Clone, Debug, Default)]
#[reflect(Default, NodeLike)]
#[type_path = "adventuresim_tactical_client::animation_graph_nodes"]
pub(crate) struct SparseSemanticBlendNode;

impl SparseSemanticBlendNode {
    pub(crate) const POSE_A: &'static str = "pose_a";
    pub(crate) const POSE_B: &'static str = "pose_b";
    pub(crate) const FACTOR: &'static str = "factor";
    pub(crate) const OUTPUT: &'static str = "pose";
}

impl NodeLike for SparseSemanticBlendNode {
    fn display_name(&self) -> String {
        "Adventure Simulator sparse semantic blend".into()
    }

    fn update(&self, mut ctx: NodeContext) -> Result<(), GraphError> {
        let mut pose_a = ctx.data_back(Self::POSE_A)?.into_pose()?;
        let pose_b = ctx.data_back(Self::POSE_B)?.into_pose()?;
        let factor = ctx.data_back(Self::FACTOR)?.as_f32()?.clamp(0.0, 1.0);
        LinearInterpolator {
            bone_mask: BoneMask::all(),
        }
        .interpolate_pose(&mut pose_a, &pose_b, factor);
        ctx.set_data_fwd(Self::OUTPUT, DataValue::Pose(pose_a));
        Ok(())
    }

    fn spec(&self, mut ctx: SpecContext) -> Result<(), GraphError> {
        ctx.add_input_data(Self::POSE_A, DataSpec::Pose)
            .add_input_data(Self::POSE_B, DataSpec::Pose)
            .add_input_data(Self::FACTOR, DataSpec::F32)
            .add_output_data(Self::OUTPUT, DataSpec::Pose);
        Ok(())
    }
}
