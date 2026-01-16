use bevy::prelude::*;

use character_creator_plugin::CharacterCreatorPlugin;

pub fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(CharacterCreatorPlugin)
        .run();
}
