use adventuresim_tactical_core::{
    inventory::{ItemQuery, ItemQueryItem},
    player::{Player, PlayerId},
    prelude::*,
};
use adventuresim_tactical_netcode::{
    aeronet::io::connection::{LocalAddr, PeerAddr},
    bevy_replicon::prelude::{ClientState, ClientStats},
    client::normalize_server_url,
    prelude::*,
};
use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    ecs::schedule::common_conditions::any_with_component,
    prelude::*,
};
use bevy_flair::prelude::*;

use crate::{
    Args,
    player::{AttackState, ClientPlayer},
};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FlairPlugin)
            .add_systems(Startup, setup_ui)
            .add_systems(
                Update,
                (
                    update_stats_ui.run_if(any_with_component::<ClientPlayer>),
                    update_connection_ui,
                    update_skills_ui.run_if(any_with_component::<ClientPlayer>),
                    update_limbs_ui.run_if(any_with_component::<ClientPlayer>),
                    update_items_ui.run_if(any_with_component::<ClientPlayer>),
                    update_attack_timer_ui.run_if(any_with_component::<ClientPlayer>),
                ),
            )
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
struct SkillSpan(Skill);

#[derive(Component)]
struct LeftArmSpan;

#[derive(Component)]
struct RightArmSpan;

#[derive(Component)]
struct LeftLegSpan;

#[derive(Component)]
struct RightLegSpan;

#[derive(Component)]
struct ChestSpan;

#[derive(Component)]
struct StomachSpan;

#[derive(Component)]
struct HeadSpan;

#[derive(Component)]
struct AttackTimerSpan;

#[derive(Component)]
struct EquippedItemsList;

#[derive(Component)]
struct InventoryItemsList;

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
            (Name::new("attack-timer"), AttackTimerSpan, Text::default(),),
            (
                Name::new("controls"),
                Text::new(""),
                children![
                    TextSpan::new("WASD to move | Space to jump | Mouse to look around\n"),
                    #[cfg(feature = "debug")]
                    TextSpan::new(
                        "DEBUG: F2 to toggle body | F3 to toggle hitbox | F4 to toggle hitscan"
                    )
                ],
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
                Name::new("player"),
                Node::default(),
                children![
                    (
                        Name::new("skills"),
                        Node::default(),
                        children![
                            Text::new("Skills"),
                            (
                                Name::new("melee"),
                                Text::new("Melee hours:\n"),
                                children![(SkillSpan(Skill::Melee), TextSpan::default())]
                            ),
                            (
                                Name::new("dodge"),
                                Text::new("Dodge hours:\n"),
                                children![(SkillSpan(Skill::Dodge), TextSpan::default())]
                            ),
                            (
                                Name::new("block"),
                                Text::new("Block hours:\n"),
                                children![(SkillSpan(Skill::Block), TextSpan::default())]
                            ),
                            (
                                Name::new("ranged"),
                                Text::new("Ranged hours:\n"),
                                children![(SkillSpan(Skill::Ranged), TextSpan::default())]
                            ),
                            (
                                Name::new("will"),
                                Text::new("Will hours:\n"),
                                children![(SkillSpan(Skill::Will), TextSpan::default())]
                            ),
                            (
                                Name::new("charisma"),
                                Text::new("Charisma hours:\n"),
                                children![(SkillSpan(Skill::Charisma), TextSpan::default())]
                            ),
                            (
                                Name::new("medicine"),
                                Text::new("Medicine hours:\n"),
                                children![(SkillSpan(Skill::Medicine), TextSpan::default())]
                            ),
                            (
                                Name::new("faith"),
                                Text::new("Faith hours:\n"),
                                children![(SkillSpan(Skill::Faith), TextSpan::default())]
                            ),
                            (
                                Name::new("stealth"),
                                Text::new("Stealth hours:\n"),
                                children![(SkillSpan(Skill::Stealth), TextSpan::default())]
                            ),
                            (
                                Name::new("balance"),
                                Text::new("Balance hours:\n"),
                                children![(SkillSpan(Skill::Balance), TextSpan::default())]
                            ),
                            (
                                Name::new("surgeon"),
                                Text::new("Surgeon hours:\n"),
                                children![(SkillSpan(Skill::Surgeon), TextSpan::default())]
                            ),
                        ]
                    ),
                    (
                        Name::new("limbs"),
                        Node::default(),
                        children![
                            Text::new("Limbs"),
                            (
                                Name::new("head"),
                                Text::new("Head: "),
                                children![(HeadSpan, TextSpan::default())]
                            ),
                            (
                                Name::new("chest"),
                                Text::new("Chest: "),
                                children![(ChestSpan, TextSpan::default())]
                            ),
                            (
                                Name::new("stomach"),
                                Text::new("Stomach: "),
                                children![(StomachSpan, TextSpan::default())]
                            ),
                            (
                                Name::new("left-arm"),
                                Text::new("Left Arm: "),
                                children![(LeftArmSpan, TextSpan::default())]
                            ),
                            (
                                Name::new("right-arm"),
                                Text::new("Right Arm: "),
                                children![(RightArmSpan, TextSpan::default())]
                            ),
                            (
                                Name::new("left-leg"),
                                Text::new("Left Leg: "),
                                children![(LeftLegSpan, TextSpan::default())]
                            ),
                            (
                                Name::new("right-leg"),
                                Text::new("Right Leg: "),
                                children![(RightLegSpan, TextSpan::default())]
                            ),
                        ]
                    ),
                    (
                        Name::new("equipped-items"),
                        Node::default(),
                        children![
                            Text::new("Equipped"),
                            (
                                EquippedItemsList,
                                Name::new("equipped-list"),
                                Node::default()
                            )
                        ]
                    ),
                    (
                        Name::new("inventory-items"),
                        Node::default(),
                        children![
                            Text::new("Inventory"),
                            (
                                InventoryItemsList,
                                Name::new("inventory-list"),
                                Node::default()
                            )
                        ]
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
    player: Single<(Ref<Transform>, &PlayerId), With<ClientPlayer>>,
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

fn update_skills_ui(
    player: Single<(&Skills, &PlayerId), (With<ClientPlayer>, Changed<Skills>)>,
    mut spans: Query<(&mut TextSpan, &SkillSpan)>,
) {
    let (skills, _player_id) = player.into_inner();

    for (mut text, skill_span) in &mut spans {
        text.0 = match skill_span.0 {
            Skill::Melee => format!("{:.2}", skills.melee_hours),
            Skill::Dodge => format!("{:.2}", skills.dodge_hours),
            Skill::Block => format!("{:.2}", skills.block_hours),
            Skill::Ranged => format!("{:.2}", skills.ranged_hours),
            Skill::Will => format!("{:.2}", skills.will_hours),
            Skill::Charisma => format!("{:.2}", skills.charisma_hours),
            Skill::Medicine => format!("{:.2}", skills.medicine_hours),
            Skill::Faith => format!("{:.2}", skills.faith_hours),
            Skill::Stealth => format!("{:.2}", skills.stealth_hours),
            Skill::Balance => format!("{:.2}", skills.balance_hours),
            Skill::Surgeon => format!("{:.2}", skills.surgeon_hours),
        };
    }
}

fn update_limbs_ui(
    player: Single<(&Limbs, &PlayerId), (With<ClientPlayer>, Changed<Limbs>)>,
    mut spans: ParamSet<(
        Single<&mut TextSpan, With<HeadSpan>>,
        Single<&mut TextSpan, With<ChestSpan>>,
        Single<&mut TextSpan, With<StomachSpan>>,
        Single<&mut TextSpan, With<LeftArmSpan>>,
        Single<&mut TextSpan, With<RightArmSpan>>,
        Single<&mut TextSpan, With<LeftLegSpan>>,
        Single<&mut TextSpan, With<RightLegSpan>>,
    )>,
) {
    let (limbs, _player_id) = player.into_inner();

    spans.p0().0 = format!("{:.0}%", limbs.head * 100.0);
    spans.p1().0 = format!("{:.0}%", limbs.chest * 100.0);
    spans.p2().0 = format!("{:.0}%", limbs.stomach * 100.0);
    spans.p3().0 = format!("{:.0}%", limbs.left_arm * 100.0);
    spans.p4().0 = format!("{:.0}%", limbs.right_arm * 100.0);
    spans.p5().0 = format!("{:.0}%", limbs.left_leg * 100.0);
    spans.p6().0 = format!("{:.0}%", limbs.right_leg * 100.0);
}

fn item_display_name(item: &ItemQueryItem) -> String {
    let qty = if item.quantity.get() > 1 {
        format!(" x{}", item.quantity.get())
    } else {
        String::new()
    };
    let slot = if let Some(slot) = item.slot {
        format!("{slot}: ")
    } else {
        String::new()
    };
    let name = if item.properties.id.is_empty() {
        "unknown"
    } else {
        item.properties.id.as_str()
    };

    if let Some(weapon) = item.weapon {
        format!("{slot}{name}{qty}\naccuracy: {:.1}", weapon.accuracy)
    } else if let Some(armor) = item.armor {
        format!(
            "{slot}{name}{qty}\ncoverage: {:.1} | padding: {:.1}\nrange_of_motion: {:.1} | flexibility: {:.1}",
            armor.coverage, armor.padding, armor.range_of_motion, armor.flexibility
        )
    } else if let Some(shield) = item.shield {
        format!("{slot}{name}{qty}\nblock: {:.1}", shield.block)
    } else {
        format!("{slot}{name}{qty}")
    }
}

fn update_items_ui(
    mut cmd: Commands,
    player: Single<Option<&InventoryItems>, (With<ClientPlayer>, Changed<InventoryItems>)>,
    equipped_list: Single<Entity, With<EquippedItemsList>>,
    inventory_list: Single<Entity, With<InventoryItemsList>>,
    q_items: Query<ItemQuery>,
) {
    let equipped_list_entity = equipped_list.into_inner();
    cmd.entity(equipped_list_entity).despawn_children();
    let inventory_list_entity = inventory_list.into_inner();
    cmd.entity(inventory_list_entity).despawn_children();

    let Some(items) = player.into_inner() else {
        return;
    };

    for item in q_items.iter_many(items.iter()) {
        let list = if item.slot.is_some() {
            equipped_list_entity
        } else {
            inventory_list_entity
        };
        cmd.spawn((Text::new(item_display_name(&item)), ChildOf(list)));
    }
}

fn update_connection_ui(
    player: Single<(
        &AdventureSimulatorClient,
        Option<&PeerAddr>,
        Option<&LocalAddr>,
    )>,
    client_state: Res<State<ClientState>>,
    client_stats: Res<ClientStats>,
    mut spans: ParamSet<(
        Single<&mut TextSpan, With<ServerInfoSpan>>,
        Single<&mut TextSpan, With<ClientInfoSpan>>,
        Single<(&mut TextSpan, &mut ClassList), With<ClientStatusSpan>>,
    )>,
) {
    let (client, peer, local) = player.into_inner();

    spans.p0().0 = peer
        .map(|addr| addr.0.to_string())
        .unwrap_or_else(|| normalize_server_url(&client.server_url));
    spans.p1().0 = local
        .map(|addr| addr.0.to_string())
        .unwrap_or_else(|| "browser session".to_string());

    let (status_text, status_class) = match client_state.get() {
        ClientState::Connected => (
            format!(
                "\nConnected\nRTT {:.0}ms | Loss {:.1}% | Rx {:.0}bps | Tx {:.0}bps",
                client_stats.rtt * 1000.0,
                client_stats.packet_loss * 100.0,
                client_stats.received_bps,
                client_stats.sent_bps,
            ),
            "success",
        ),
        ClientState::Connecting => ("\nConnecting...".to_string(), "primary"),
        ClientState::Disconnected => ("\nDisconnected".to_string(), "error"),
    };

    if spans.p2().0.0 != status_text {
        spans.p2().0.0 = status_text;
    }
    if !spans.p2().1.contains(status_class) {
        *spans.p2().1 = ClassList::new(status_class);
    }
}

fn update_attack_timer_ui(
    player: Single<Option<Ref<AttackState>>, With<ClientPlayer>>,
    mut span: Single<&mut Text, With<AttackTimerSpan>>,
) {
    let state = player.into_inner();

    if let Some(state) = state.as_ref()
        && state.is_attacking()
    {
        let remaining = state.pre_hit_timer.remaining().as_secs_f32();
        span.0 = format!("{:.1}s", remaining);
    } else if !span.0.is_empty() {
        span.0.clear();
    }
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
