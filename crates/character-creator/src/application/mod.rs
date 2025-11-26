use bevy::prelude::*;

use crate::plugins::animation_player::AnimationPlayerPlugin;

pub fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(AnimationPlayerPlugin)
        .run();
}
