use bevy::prelude::*;

pub mod components;
pub mod systems;

pub use components::*;
pub use systems::*;

pub struct DistanceFieldPlugin;

impl Plugin for DistanceFieldPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<SdfShape>()
            .register_type::<SdfConfig>()
            .add_systems(Update, (update_distance_field, debug_sdf));
    }
}
