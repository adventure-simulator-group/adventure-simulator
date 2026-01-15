use bevy::prelude::*;

#[derive(Resource)]
pub struct Animations {
    pub animations: Vec<AnimationNodeIndex>,
    pub graph_handle: Handle<AnimationGraph>,
}
