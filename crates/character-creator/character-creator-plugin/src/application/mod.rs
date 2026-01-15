use bevy::prelude::*;

use crate::plugins::AnimationPlayerPlugin;
use marching_cubes_plugin::MarchingCubesPlugin;
use distance_field_plugin::DistanceFieldPlugin;
use sphere_tracing_plugin::SphereTracingPlugin;

pub fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(AnimationPlayerPlugin)
        .add_plugins(DistanceFieldPlugin)
        .add_plugins(MarchingCubesPlugin)
        .add_plugins(SphereTracingPlugin)
        .run();
}
