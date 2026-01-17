use bevy::prelude::*;

#[derive(States, Debug, Clone, Copy, Default, Eq, PartialEq, Hash)]
pub enum SceneState {
    #[default]
    Character,
    Debug,
}

#[derive(Component)]
pub struct InCharacterScene;

#[derive(Component)]
pub struct InDebugScene;
