mod prelude;

use bevy::prelude::*;

pub mod components;
mod systems;

pub use components::*;

pub struct DistanceFieldPlugin;

impl Plugin for DistanceFieldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (DistanceField::update, DistanceField::debug));
    }
}
