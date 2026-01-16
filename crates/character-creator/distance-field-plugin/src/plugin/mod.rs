use bevy::prelude::*;
use crate::components::DistanceFieldComponent;
pub struct DistanceFieldPlugin;

impl Plugin for DistanceFieldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (DistanceFieldComponent::update, DistanceFieldComponent::debug));
    }
}
