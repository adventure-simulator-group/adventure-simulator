use bevy::prelude::*;

use crate::plugins::AnimationPlayerPlugin;
use marching_cubes_plugin::MarchingCubesPlugin;
use distance_field_plugin::DistanceFieldPlugin;

pub fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(AnimationPlayerPlugin)
        .add_plugins(DistanceFieldPlugin)
        .add_plugins(MarchingCubesPlugin)
        .run();
}
