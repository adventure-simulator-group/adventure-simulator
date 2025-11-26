use bevy::prelude::*;

#[derive(Resource)]
pub struct SceneHandle {
    pub scene: Handle<Scene>,
}
