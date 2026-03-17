use adventuresim_tactical_core::{
    player::{Player, PlayerId},
    prelude::CharacterController,
};
use adventuresim_tactical_netcode::lightyear::{connection::client::ClientState, prelude::*};
use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};
use bevy_flair::prelude::*;

use crate::Args;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FlairPlugin)
            .add_systems(Startup, setup_ui)
            .add_systems(Update, (update_stats_ui, update_connection_ui))
            .add_observer(on_new_player_added_hook);
    }
}

#[derive(Component)]
struct PositionSpan;

#[derive(Component)]
struct FpsSpan;

#[derive(Component)]
struct ServerInfoSpan;

#[derive(Component)]
struct ClientInfoSpan;

#[derive(Component)]
struct ClientStatusSpan;

#[derive(Component)]
struct PlayersList;

#[derive(Component)]
#[relationship(relationship_target = PlayerSpan)]
struct PlayerSpanOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = PlayerSpanOf, linked_spawn)]
struct PlayerSpan(Vec<Entity>);

fn setup_ui(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.spawn((
        Node::default(),
        NodeStyleSheet::new(asset_server.load("ui.css")),
        children![
            (Name::new("crosshair"), Node::default()),
            (
                Name::new("controls"),
                Text::new("WASD to move | Space to jump | Mouse to look around"),
            ),
            (
                Name::new("stats"),
                Node::default(),
                children![
                    (
                        Name::new("position"),
                        Text::new("Position: "),
                        children![(PositionSpan, TextSpan::default())]
                    ),
                    (
                        Name::new("fps"),
                        Text::new("FPS: "),
                        children![(FpsSpan, TextSpan::default())]
                    ),
                ]
            ),
            (
                Name::new("info"),
                Node::default(),
                children![
                    (
                        Name::new("connection"),
                        Node::default(),
                        children![
                            (
                                Text::new("Server: "),
                                children![(ServerInfoSpan, TextSpan::default())]
                            ),
                            (
                                Text::new("Client: "),
                                children![
                                    (ClientInfoSpan, TextSpan::default()),
                                    (ClientStatusSpan, ClassList::default(), TextSpan::default())
                                ]
                            ),
                        ]
                    ),
                    (
                        PlayersList,
                        Name::new("players"),
                        Node::default(),
                        children![Text::new("Players:")]
                    )
                ]
            ),
        ],
    ));
}

fn update_stats_ui(
    diagnostics: Res<DiagnosticsStore>,
    player: Single<(Ref<Transform>, &PlayerId), With<CharacterController>>,
    mut spans: ParamSet<(
        Single<&mut TextSpan, With<PositionSpan>>,
        Single<&mut TextSpan, With<FpsSpan>>,
    )>,
) {
    let (transform, &PlayerId(_player_id)) = player.into_inner();

    if transform.is_changed() {
        let translation = transform.translation;
        spans.p0().0 = format!(
            "{:.1}  {:.1}  {:.1}",
            translation.x, translation.y, translation.z
        );
    }

    if let Some(fps) = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|fps| fps.smoothed())
    {
        spans.p1().0 = format!("{fps:.2}",);
    }
}

fn update_connection_ui(
    player: Single<(&PeerAddr, &LocalAddr, &Client, Option<&Disconnected>), Changed<Client>>,
    mut spans: ParamSet<(
        Single<&mut TextSpan, With<ServerInfoSpan>>,
        Single<&mut TextSpan, With<ClientInfoSpan>>,
        Single<(&mut TextSpan, &mut ClassList), With<ClientStatusSpan>>,
    )>,
) {
    let (peer, local, client, disconnected) = player.into_inner();

    spans.p0().0 = peer.0.to_string();
    spans.p1().0 = local.0.to_string();
    let (status_text, status_class) = match (client.state, disconnected) {
        (ClientState::Connected, _) => ("\nConnected".to_string(), "success"),
        (ClientState::Connecting, _) => ("\nConnecting...".to_string(), "primary"),
        (ClientState::Disconnecting, _) => ("\nDisconnecting..".to_string(), "primary"),
        (
            ClientState::Disconnected,
            Some(Disconnected {
                reason: Some(reason),
            }),
        ) => (format!("\nDisconnected:\n{reason}"), "error"),
        (ClientState::Disconnected, _) => ("\nDisconnected (no reason)".to_string(), "error"),
    };

    spans.p2().0 .0 = status_text;
    *spans.p2().1 = ClassList::new(status_class);
}

fn on_new_player_added_hook(
    event: On<Add, Player>,
    mut commands: Commands,
    query: Query<(&PlayerId, &Player)>,
    args: Res<Args>,
    players_list: Single<Entity, With<PlayersList>>,
) -> Result {
    let (id, player) = query.get(event.entity)?;

    let class_list = if args.id == id.0 {
        "player controlled"
    } else {
        "player"
    };
    commands.spawn((
        ClassList::new(class_list),
        Node::default(),
        InlineStyle::new(&format!(
            "--player-color: {}",
            id.color().to_srgba().to_hex()
        )),
        children![
            (ClassList::new("player-icon"), Node::default()),
            (
                ClassList::new("player-name"),
                Text::new(player.name.clone()),
            ),
            (ClassList::new("player-id"), Text::new(format!("#{}", id.0)))
        ],
        ChildOf(players_list.into_inner()),
        PlayerSpanOf(event.entity),
    ));

    Ok(())
}
