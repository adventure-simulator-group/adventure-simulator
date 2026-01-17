use bevy::prelude::*;

#[derive(States, Debug, Clone, Copy, Default, Eq, PartialEq, Hash)]
pub enum SceneState {
    #[default]
    Character,
    MarchingCubes,
    SphereTracing,
}

#[derive(Component)]
pub struct InCharacterScene;

#[derive(Component)]
pub struct InMarchingCubesScene;

#[derive(Component)]
pub struct InSphereTracingScene;
