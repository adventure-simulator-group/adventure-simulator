use adventuresim_tactical_core::{
    inventory::{ItemQuery, ItemQueryItem},
    player::{CharacterId, Player},
    prelude::*,
};
use adventuresim_tactical_netcode::{
    aeronet::io::connection::{LocalAddr, PeerAddr},
    bevy_replicon::prelude::{ClientState, ClientStats},
    client::WeaponGuardInputState,
    client::normalize_server_url,
    message::{SuccessfulAttackResponse, TacticalOutcome, TacticalOutcomeResponse},
    prelude::*,
};
use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    ecs::schedule::common_conditions::any_with_component,
    prelude::*,
};
use bevy_flair::prelude::*;

#[cfg(feature = "debug")]
use crate::animation::TerrainIkEnabled;
#[cfg(feature = "debug")]
use crate::camera::{CameraDebugEnabled, CameraRigDebugState};
#[cfg(feature = "debug")]
use crate::debug::DebugGameSpeed;
use crate::{
    Args,
    camera::CameraAimState,
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
                    update_incapacitation_ui.run_if(any_with_component::<ClientPlayer>),
                    update_combat_state_ui.run_if(any_with_component::<ClientPlayer>),
                    update_items_ui.run_if(any_with_component::<ClientPlayer>),
                    update_attack_timer_ui.run_if(any_with_component::<ClientPlayer>),
                    update_weapon_guard_ui,
                    update_camera_ui,
                    #[cfg(feature = "debug")]
                    update_terrain_ik_debug_ui,
                    #[cfg(feature = "debug")]
                    update_game_speed_debug_ui,
                ),
            )
            .add_observer(on_new_player_added_hook)
            .add_observer(on_successful_attack_display)
            .add_observer(on_tactical_outcome_display)
            .add_systems(Update, update_attack_result_ui);
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

/// Which incapacitation factor a meter-bar fill node visualizes.
#[derive(Clone, Copy)]
enum IncapacitationFactor {
    Pain,
    BloodLoss,
    Imbalance,
}

#[derive(Component)]
struct IncapacitationBarFill(IncapacitationFactor);

#[derive(Component)]
struct IncapacitationTotalSpan;

#[derive(Component)]
struct IncapacitationStatusSpan;

#[derive(Component)]
struct AttackTimerSpan;

#[derive(Component)]
struct CombatStateSpan;

#[derive(Component)]
struct WeaponGuardSpan;

#[derive(Component)]
struct RaisedReticle;

#[derive(Component)]
struct AimBlockedSpan;

#[cfg(feature = "debug")]
#[derive(Component)]
struct CameraDebugSpan;

#[cfg(feature = "debug")]
#[derive(Component)]
struct TerrainIkDebugSpan;

#[cfg(feature = "debug")]
#[derive(Component)]
struct GameSpeedDebugSpan;

#[derive(Component)]
struct TacticalOutcomeBanner;

#[derive(Component)]
struct AttackResultText {
    timer: Timer,
}

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
            (
                Name::new("terminal-outcome"),
                TacticalOutcomeBanner,
                Visibility::Hidden,
                ClassList::new("primary"),
                Text::default(),
            ),
            (
                Name::new("crosshair"),
                RaisedReticle,
                Visibility::Hidden,
                Node::default(),
                children![(
                    Name::new("aim-blocked"),
                    AimBlockedSpan,
                    Visibility::Hidden,
                    Text::new("MUZZLE BLOCKED"),
                )],
            ),
            (
                Name::new("attack-info"),
                Node::default(),
                children![
                    (Name::new("attack-timer"), Text::default(), AttackTimerSpan),
                    (
                        Name::new("attack-result"),
                        Text::default(),
                        AttackResultText {
                            timer: Timer::from_seconds(4.0, TimerMode::Once)
                        }
                    )
                ]
            ),
            (
                Name::new("weapon-guard"),
                Text::new("Guard: "),
                children![(WeaponGuardSpan, TextSpan::new("Lowered"))],
            ),
            (
                Name::new("controls"),
                Text::new(
                    "WASD to move | Space to jump | Mouse to look around | F9 to toggle camera\n",
                ),
                #[cfg(feature = "debug")]
                children![
                    TextSpan::new(
                        "DEBUG: F2 to toggle body | F3 to toggle hitbox | F4 to toggle hitscan"
                    ),
                    (GameSpeedDebugSpan, TextSpan::new(" | F7 game speed: 1x")),
                    (TerrainIkDebugSpan, TextSpan::new(" | F8 terrain IK: OFF")),
                    (CameraDebugSpan, TextSpan::new(" | F6 camera rig: OFF"))
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
                    (
                        Name::new("combat-state"),
                        Text::new("Combat: "),
                        children![(CombatStateSpan, TextSpan::default())]
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
                                Name::new("sword"),
                                Text::new("Sword hours:\n"),
                                children![(SkillSpan(Skill::Sword), TextSpan::default())]
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
                                Name::new("bow"),
                                Text::new("Bow hours:\n"),
                                children![(SkillSpan(Skill::Bow), TextSpan::default())]
                            ),
                            (
                                Name::new("will"),
                                Text::new("Will hours:\n"),
                                children![(SkillSpan(Skill::Will), TextSpan::default())]
                            ),
                            (
                                Name::new("command"),
                                Text::new("Command hours:\n"),
                                children![(SkillSpan(Skill::Command), TextSpan::default())]
                            ),
                            (
                                Name::new("physiology"),
                                Text::new("Physiology hours:\n"),
                                children![(SkillSpan(Skill::Physiology), TextSpan::default())]
                            ),
                            (
                                Name::new("religion"),
                                Text::new("Religion hours:\n"),
                                children![(SkillSpan(Skill::Religion), TextSpan::default())]
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
                        Name::new("incapacitation"),
                        Node::default(),
                        children![
                            Text::new("Incapacitation"),
                            (
                                Name::new("incap-meter"),
                                Node::default(),
                                children![
                                    (
                                        IncapacitationBarFill(IncapacitationFactor::Pain),
                                        ClassList::new("incap-pain"),
                                        Node::default()
                                    ),
                                    (
                                        IncapacitationBarFill(IncapacitationFactor::BloodLoss),
                                        ClassList::new("incap-blood"),
                                        Node::default()
                                    ),
                                    (
                                        IncapacitationBarFill(IncapacitationFactor::Imbalance),
                                        ClassList::new("incap-imbalance"),
                                        Node::default()
                                    ),
                                ]
                            ),
                            (
                                Name::new("incap-total"),
                                Text::new("Total: "),
                                children![(IncapacitationTotalSpan, TextSpan::default())]
                            ),
                            (
                                Name::new("incap-status"),
                                Text::new("Status: "),
                                children![(
                                    IncapacitationStatusSpan,
                                    ClassList::default(),
                                    TextSpan::default()
                                )]
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

fn update_weapon_guard_ui(
    guard: Res<WeaponGuardInputState>,
    mut spans: Query<&mut TextSpan, With<WeaponGuardSpan>>,
) {
    if !guard.is_changed() {
        return;
    }
    for mut span in &mut spans {
        **span = match guard.desired {
            WeaponGuardState::Lowered => "Lowered".to_owned(),
            WeaponGuardState::Raised => "Raised".to_owned(),
        };
    }
}

fn update_camera_ui(
    aim: Res<CameraAimState>,
    mut reticle: Single<&mut Visibility, (With<RaisedReticle>, Without<AimBlockedSpan>)>,
    mut blocked: Single<&mut Visibility, (With<AimBlockedSpan>, Without<RaisedReticle>)>,
    #[cfg(feature = "debug")] enabled: Res<CameraDebugEnabled>,
    #[cfg(feature = "debug")] debug: Res<CameraRigDebugState>,
    #[cfg(feature = "debug")] mut debug_span: Single<&mut TextSpan, With<CameraDebugSpan>>,
) {
    **reticle = if aim.active {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    **blocked = if aim.active && aim.blocked {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    #[cfg(feature = "debug")]
    {
        debug_span.0 = if enabled.0 && debug.active {
            format!(
                " | F6 camera: {:.0}% | boom {:.2}/{:.2}m v{:.2} | focus v{:.2} | screen ({:.2},{:.2})/({:.2},{:.2}) | hard:{} soft:{}",
                debug.raised_blend * 100.0,
                debug.limited_distance,
                debug.desired_distance,
                debug.boom_velocity,
                debug.focus_velocity.length(),
                debug.screen_error.x,
                debug.screen_error.y,
                debug.sweet_spot.x,
                debug.sweet_spot.y,
                debug.collision_entity.is_some(),
                debug.soft_occluder.is_some(),
            )
        } else {
            " | F6 camera rig: OFF".to_owned()
        };
    }
}

#[cfg(feature = "debug")]
fn update_terrain_ik_debug_ui(
    enabled: Res<TerrainIkEnabled>,
    mut span: Single<&mut TextSpan, With<TerrainIkDebugSpan>>,
) {
    if enabled.is_changed() {
        span.0 = if enabled.0 {
            " | F8 terrain IK: ON".to_owned()
        } else {
            " | F8 terrain IK: OFF".to_owned()
        };
    }
}

#[cfg(feature = "debug")]
fn update_game_speed_debug_ui(
    speed: Res<DebugGameSpeed>,
    mut span: Single<&mut TextSpan, With<GameSpeedDebugSpan>>,
) {
    if speed.is_changed() {
        span.0 = if speed.quarter_speed {
            " | F7 game speed: 1/4x".to_owned()
        } else {
            " | F7 game speed: 1x".to_owned()
        };
    }
}

fn combat_state_label(state: &TacticalCombatState) -> String {
    if state.is_incapacitated() {
        format!(
            "INCAPACITATED | Blood loss {:.0}% | Imbalance {:.0}%",
            state.blood_loss_fraction * 100.0,
            state.imbalance * 100.0
        )
    } else {
        format!(
            "Active | Blood loss {:.0}% | Imbalance {:.0}%",
            state.blood_loss_fraction * 100.0,
            state.imbalance * 100.0
        )
    }
}

fn update_combat_state_ui(
    player: Single<&TacticalCombatState, With<ClientPlayer>>,
    mut span: Single<&mut TextSpan, With<CombatStateSpan>>,
) {
    span.0 = combat_state_label(&player);
}

fn tactical_outcome_label(outcome: TacticalOutcome) -> (&'static str, &'static str) {
    match outcome {
        TacticalOutcome::Victory => ("VICTORY", "success"),
        TacticalOutcome::Defeat => ("DEFEAT", "error"),
    }
}

fn on_tactical_outcome_display(
    event: On<TacticalOutcomeResponse>,
    mut banner: Single<(&mut Text, &mut Visibility, &mut ClassList), With<TacticalOutcomeBanner>>,
) {
    let (label, class) = tactical_outcome_label(event.outcome);
    banner.0.0 = label.to_owned();
    *banner.1 = Visibility::Visible;
    *banner.2 = ClassList::new(class);
}

fn update_stats_ui(
    diagnostics: Res<DiagnosticsStore>,
    player: Single<(Ref<Transform>, &CharacterId), With<ClientPlayer>>,
    mut spans: ParamSet<(
        Single<&mut TextSpan, With<PositionSpan>>,
        Single<&mut TextSpan, With<FpsSpan>>,
    )>,
) {
    let (transform, &CharacterId(_player_id)) = player.into_inner();

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
    player: Single<(&Skills, &CharacterId), (With<ClientPlayer>, Changed<Skills>)>,
    mut spans: Query<(&mut TextSpan, &SkillSpan)>,
) {
    let (skills, _player_id) = player.into_inner();

    for (mut text, skill_span) in &mut spans {
        text.0 = match skill_span.0 {
            Skill::Polearm => format!("{:.2}", skills.polearm_hours),
            Skill::Axe => format!("{:.2}", skills.axe_hours),
            Skill::Bludgeon => format!("{:.2}", skills.bludgeon_hours),
            Skill::Sword => format!("{:.2}", skills.sword_hours),
            Skill::Knife => format!("{:.2}", skills.knife_hours),
            Skill::Dodge => format!("{:.2}", skills.dodge_hours),
            Skill::Block => format!("{:.2}", skills.block_hours),
            Skill::Bow => format!("{:.2}", skills.bow_hours),
            Skill::Crossbow => format!("{:.2}", skills.crossbow_hours),
            Skill::Firearm => format!("{:.2}", skills.firearm_hours),
            Skill::Throw => format!("{:.2}", skills.throw_hours),
            Skill::Will => format!("{:.2}", skills.will_hours),
            Skill::Insight => format!("{:.2}", skills.insight_hours),
            Skill::Charm => format!("{:.2}", skills.charm_hours),
            Skill::Command => format!("{:.2}", skills.command_hours),
            Skill::Deception => format!("{:.2}", skills.deception_hours),
            Skill::Physiology => format!("{:.2}", skills.physiology_hours),
            // Cooking is strategic-only and is not carried in tactical snapshots.
            Skill::Cooking => "0.00".to_owned(),
            Skill::Herbalism => "0.00".to_owned(),
            Skill::Religion => format!("{:.2}", skills.religion_hours),
            Skill::Bestiary => "0.00".to_string(),
            Skill::Surgery => format!("{:.2}", skills.surgery_hours),
            Skill::Stealth => format!("{:.2}", skills.stealth_hours),
            Skill::Balance => format!("{:.2}", skills.balance_hours),
            Skill::TerrainPlains
            | Skill::TerrainForest
            | Skill::TerrainHills
            | Skill::TerrainWetlands
            | Skill::TerrainUrban
            | Skill::TerrainSnow => "0.00".into(),
            Skill::Tailoring => format!("{:.2}", skills.tailoring_hours),
            Skill::Smithing => format!("{:.2}", skills.smithing_hours),
        };
    }
}

fn update_limbs_ui(
    player: Single<(&Limbs, &CharacterId), (With<ClientPlayer>, Changed<Limbs>)>,
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

/// Mirrors the "wheel" incapacitation meter from `wiki/tactical/combat.md` as
/// a segmented bar (pain/blood loss/imbalance) plus a total/status readout,
/// since the current HUD has no radial-gauge rendering path.
fn update_incapacitation_ui(
    player: Single<(&CombatState, &CharacterId), (With<ClientPlayer>, Changed<CombatState>)>,
    mut bars: Query<(&IncapacitationBarFill, &mut Node)>,
    mut spans: ParamSet<(
        Single<&mut TextSpan, With<IncapacitationTotalSpan>>,
        Single<(&mut TextSpan, &mut ClassList), With<IncapacitationStatusSpan>>,
    )>,
) {
    let (state, _player_id) = player.into_inner();

    for (fill, mut node) in &mut bars {
        let value = match fill.0 {
            IncapacitationFactor::Pain => state.pain,
            IncapacitationFactor::BloodLoss => state.blood_loss,
            IncapacitationFactor::Imbalance => state.imbalance,
        };
        node.width = Val::Percent((value * 100.0).clamp(0.0, 100.0));
    }

    spans.p0().0 = format!("{:.0}%", state.incapacitation() * 100.0);

    let (status_text, status_class) = match state.status() {
        IncapacitationStatus::Ready => ("Ready", "success"),
        IncapacitationStatus::Staggered => ("Staggered", "primary"),
        IncapacitationStatus::Incapacitated => ("Incapacitated", "error"),
    };
    let mut status_span = spans.p1();
    if status_span.0.0 != status_text {
        status_span.0.0 = status_text.to_string();
    }
    if !status_span.1.contains(status_class) {
        *status_span.1 = ClassList::new(status_class);
    }
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

fn on_successful_attack_display(
    event: On<SuccessfulAttackResponse>,
    mut cmd: Commands,
    q_player: Query<(&Player, &CharacterId)>,
    mut span: Single<(Entity, &mut AttackResultText, &mut Text)>,
) {
    let Some((player, id)) = event.hit.first().and_then(|e| q_player.get(*e).ok()) else {
        return;
    };

    span.1.timer.reset();
    span.1.timer.unpause();
    span.2.clear();
    cmd.entity(span.0)
        .despawn_children()
        .with_children(|children| {
            children.spawn(TextSpan::new("Attacking "));
            children.spawn((
                ClassList::new("player-id"),
                TextSpan::new(player.name.clone()),
                InlineStyle::new(&format!(
                    "--player-color: {}",
                    id.color().to_srgba().to_hex()
                )),
            ));
            children.spawn(TextSpan::new(": "));
            match event.result {
                AttackResult::ToAttacker { balance_damage, .. } => {
                    let reason = match event.defender_response {
                        DefenderResponse::None => "Missed",
                        DefenderResponse::Dodge { .. } => "Dodged",
                        DefenderResponse::Parry { .. } => "Parried",
                    };
                    children.spawn((ClassList::new("error"), TextSpan::new("fail")));
                    children.spawn(TextSpan::new(format!(
                        "\n\n{reason}! Got {balance_damage:.1} balance damage\n\n[part: {}]",
                        event.body_part
                    )));
                }
                AttackResult::ToDefender {
                    cut_damage,
                    blunt_damage,
                    balance_damage,
                    ..
                } => {
                    children.spawn((ClassList::new("success"), TextSpan::new("success")));
                    children.spawn(TextSpan::new(format!(
                        "\n\nDealt..\n{:.1} damage ({cut_damage:.1}C + {blunt_damage:.1}B)\n{balance_damage:.1} balance damage\n\n[part: {} | flanking: {:.1}]",
                        cut_damage + blunt_damage,
                        event.body_part,
                        event.flanking
                    )));
                }
            }
        });
}

fn update_attack_result_ui(
    time: Res<Time>,
    mut cmd: Commands,
    mut span: Single<(Entity, &mut AttackResultText, &mut Text)>,
) {
    span.1.timer.tick(time.delta());
    if span.1.timer.just_finished() {
        cmd.entity(span.0).despawn_children();
        span.2.clear();
        span.1.timer.pause();
    }
}

fn on_new_player_added_hook(
    event: On<Add, Player>,
    mut commands: Commands,
    query: Query<(&CharacterId, &Player)>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combat_label_surfaces_live_and_incapacitated_state() {
        let active = TacticalCombatState {
            blood_loss_fraction: 0.25,
            imbalance: 0.5,
            ..default()
        };
        assert_eq!(
            combat_state_label(&active),
            "Active | Blood loss 25% | Imbalance 50%"
        );
        let incapacitated = TacticalCombatState {
            incapacitation: 1.0,
            ..active
        };
        assert!(combat_state_label(&incapacitated).starts_with("INCAPACITATED"));
    }

    #[test]
    fn terminal_outcome_labels_are_unambiguous() {
        assert_eq!(
            tactical_outcome_label(TacticalOutcome::Victory),
            ("VICTORY", "success")
        );
        assert_eq!(
            tactical_outcome_label(TacticalOutcome::Defeat),
            ("DEFEAT", "error")
        );
    }
}
