use bevy::prelude::*;

use crate::plugins::{AnimationPlayerPlugin, MarchingCubesPlugin};

pub fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(AnimationPlayerPlugin)
        .add_plugins(MarchingCubesPlugin)
        .run();
}
