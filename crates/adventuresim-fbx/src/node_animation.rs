use crate::TransformChannel;

/// Everything one take animates on one node.
#[derive(Debug, Clone)]
pub struct NodeAnimation {
    /// Object id of the animated `Model`.
    pub node: i64,
    /// The model's name, namespace intact.
    pub name: String,
    pub translation: Option<TransformChannel>,
    /// Euler angles in degrees, in the node's own rotation order.
    pub rotation: Option<TransformChannel>,
    pub scale: Option<TransformChannel>,
}

impl NodeAnimation {
    pub fn key_times(&self, into: &mut Vec<f64>) {
        for channel in [&self.translation, &self.rotation, &self.scale]
            .into_iter()
            .flatten()
        {
            channel.key_times(into);
        }
    }
}
