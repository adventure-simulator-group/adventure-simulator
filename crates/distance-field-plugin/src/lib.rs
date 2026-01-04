use bevy::prelude::*;

pub mod components;
pub mod field;
pub mod systems;

pub use components::*;
pub use field::*;
pub use systems::*;

pub struct DistanceFieldPlugin;

impl Plugin for DistanceFieldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SdfConfig>()
            // Initialize with default size
            .insert_resource(DistanceField::new_distance_field(36, 36, 36))
            .register_type::<SdfShape>()
            //.register_type::<SdfOperation>()
            .add_systems(Update, (update_distance_field, debug_sdf));
    }
}
