use bevy::prelude::*;
use crate::plugin::components::InCharacterScene;

#[derive(Component)]
pub struct CharacterModel;

impl CharacterModel {
    pub fn spawn(mut commands: Commands) {
        commands.spawn((
            Transform::from_xyz(0.0, 1.0, -2.0),
            CharacterModel,
            InCharacterScene,
            Visibility::default(),
        ));
    }

    pub fn update(mut characters: Query<&mut Transform, With<CharacterModel>>, time: Res<Time>) {
        for mut transform in &mut characters {
            transform.rotation = Quat::from_rotation_y(time.elapsed_secs() as f32);
        }
    }
}
