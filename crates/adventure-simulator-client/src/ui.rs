use adventure_simulator_core::{player::PlayerId, prelude::CharacterController};
use bevy::prelude::*;
use bevy_flair::prelude::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FlairPlugin)
            .add_systems(Startup, setup_ui)
            .add_systems(Update, update_ui);
    }
}

#[derive(Component)]
struct PositionSpan;

fn setup_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        Node::default(),
        NodeStyleSheet::new(asset_server.load("ui.css")),
        children![
            (
                Name::new("controls-text"),
                Text::new("WASD to move | Space to jump | Mouse to look around"),
            ),
            (
                Name::new("position"),
                Text::new("Position: "),
                children![(PositionSpan, TextSpan::default())]
            )
        ],
    ));
}

fn update_ui(
    player: Single<(&Transform, &PlayerId), With<CharacterController>>,
    mut text_query: Query<&mut TextSpan, With<PositionSpan>>,
) {
    let (Transform { translation, .. }, &PlayerId(player_id)) = player.into_inner();

    for mut text in &mut text_query {
        text.0 = format!(
            "{:.1} {:.1} {:.1}",
            translation.x, translation.y, translation.z
        );
    }
}
