mod prelude;

use bevy::prelude::*;

pub mod components;
mod systems;

pub use components::*;

pub struct DistanceFieldPlugin;

impl Plugin for DistanceFieldPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<SdfShape>()
            .register_type::<SdfConfig>()
            .add_systems(Update, (DistanceField::update, DistanceField::debug));
    }
}
