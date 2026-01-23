use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Id of the scene in which the game takes place.
#[derive(Component, Serialize, Deserialize, Default, Debug, Reflect, Clone, PartialEq, Eq)]
#[component(immutable)]
pub struct GameSceneId(pub String);
