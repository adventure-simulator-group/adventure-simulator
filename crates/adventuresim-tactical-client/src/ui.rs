use adventuresim_tactical_core::{
    inventory::{ItemQuery, ItemQueryItem},
    player::{CharacterId, Player},
    prelude::*,
};
use adventuresim_tactical_netcode::{
    aeronet::io::connection::{LocalAddr, PeerAddr},
    bevy_replicon::prelude::{ClientState, ClientStats},
    client::{WeaponGuardInputState, normalize_server_url},
    message::{SuccessfulAttackResponse, TacticalOutcome, TacticalOutcomeResponse},
    prelude::*,
};
use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    ecs::schedule::common_conditions::any_with_component,
    prelude::*,
};
use bevy_egui::{
    EguiContext, EguiContexts, EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass,
    PrimaryEguiContext,
    egui::{self, Color32, Pos2, Stroke},
};
use bevy_flair::prelude::*;

mod combat_state;

#[cfg(test)]
use combat_state::incapacitation_wheel_segments;
use combat_state::{combat_state_label, forecast_wheel_segments};

#[cfg(feature = "debug")]
use crate::animation::TerrainIkEnabled;
#[cfg(feature = "debug")]
use crate::camera::{CameraDebugEnabled, CameraRigDebugState};
#[cfg(feature = "debug")]
use crate::debug::DebugGameSpeed;
use crate::{
    Args,
    animation::{BoneRole, HumanoidRig},
    camera::CameraAimState,
    player::{AttackState, ClientPlayer},
    presentation::TacticalGameplayCamera,
};

pub struct UiPlugin;

/// Root of the tactical-only HUD. The persistent browser runtime keeps the UI
/// instantiated but hides it while the shared canvas presents strategic scenes.
#[derive(Component)]
pub(crate) struct TacticalUiRoot;

/// Pins the primary egui context to the gameplay camera. The automatic
/// first-camera adoption is disabled in `UiPlugin::build`, so cameras that
/// render offscreen never receive UI passes.
#[expect(
    clippy::type_complexity,
    reason = "the Bevy camera filter selects newly added gameplay cameras without cloud or existing egui contexts"
)]
fn attach_primary_egui_context(
    mut commands: Commands,
    cameras: Query<
        Entity,
        (
            Added<Camera3d>,
            Without<crate::presentation::TacticalCloudOffscreenCamera>,
            Without<EguiContext>,
        ),
    >,
) {
    for camera in &cameras {
        commands.entity(camera).insert(PrimaryEguiContext);
    }
}

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((FlairPlugin, EguiPlugin::default()));
        app.world_mut()
            .resource_mut::<EguiGlobalSettings>()
            .auto_create_primary_context = false;
        app.add_systems(Startup, setup_ui)
            // bevy_egui would otherwise adopt the first camera it sees as
            // the primary context, which can be the offscreen cloud camera
            // whose Rgba16Float target the egui pipeline cannot render to.
            .add_systems(PreUpdate, attach_primary_egui_context)
            .add_systems(
                EguiPrimaryContextPass,
                draw_incapacitation_wheel.run_if(any_with_component::<ClientPlayer>),
            )
            .add_systems(
                Update,
                (
                    update_stats_ui.run_if(any_with_component::<ClientPlayer>),
                    update_connection_ui,
                    update_limbs_ui.run_if(any_with_component::<ClientPlayer>),
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

const INCAPACITATION_WHEEL_RADIUS: f32 = 26.0;
const INCAPACITATION_WHEEL_WIDTH: f32 = 8.0;
const ENEMY_INCAPACITATION_WHEEL_RADIUS: f32 = 18.0;
const ENEMY_INCAPACITATION_WHEEL_WIDTH: f32 = 6.0;
const ENEMY_WHEEL_HEAD_CLEARANCE_METRES: f32 = 0.25;
const ENEMY_WHEEL_ROOT_FALLBACK_HEIGHT_METRES: f32 = 1.1;
const INCAPACITATION_WHEEL_RESOLUTION: f32 = 96.0;
const MIN_VISIBLE_INCAPACITATION_SEGMENT: f32 = 0.005;

fn visible_incapacitation_wheel_amount(raw_amount: f32, remaining: f32) -> Option<f32> {
    let amount = raw_amount.max(0.0).min(remaining);
    (amount >= MIN_VISIBLE_INCAPACITATION_SEGMENT).then_some(amount)
}

fn enemy_incapacitation_wheel_visible(side: TacticalCombatSide, incapacitation: f32) -> bool {
    side == TacticalCombatSide::Enemy && incapacitation > 0.0
}

#[expect(
    clippy::type_complexity,
    reason = "the Bevy enemy query selects every combat and presentation input needed by the incapacitation wheel"
)]
fn draw_incapacitation_wheel(
    mut contexts: EguiContexts,
    player: Single<(Entity, &TacticalCombatState, &Limbs), With<ClientPlayer>>,
    enemies: Query<
        (
            Entity,
            &TacticalCombatSide,
            &TacticalCombatState,
            &Limbs,
            &GlobalTransform,
            Option<&HumanoidRig>,
        ),
        (With<Player>, Without<ClientPlayer>),
    >,
    bone_transforms: Query<&GlobalTransform, Without<Player>>,
    camera: Single<(&Camera, &GlobalTransform), With<TacticalGameplayCamera>>,
    viewer: TacticalPlayerViewer,
) -> Result {
    let (entity, state, limbs) = player.into_inner();
    let view = viewer.get(entity)?;
    let will = view.skill_check(Skill::Will, LimbWeights::all_equal());
    let sources = state.incapacitation_sources(limbs.total_damage(), will);

    let context = contexts.ctx_mut()?;
    let painter = context.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("incapacitation-wheel"),
    ));
    let center = context.content_rect().center();

    if state.incapacitation > 0.0 {
        paint_incapacitation_wheel(
            &painter,
            center,
            state,
            sources,
            INCAPACITATION_WHEEL_RADIUS,
            INCAPACITATION_WHEEL_WIDTH,
        );
    }

    let content_rect = context.content_rect();
    let (camera, camera_transform) = camera.into_inner();
    for (entity, side, state, limbs, root_transform, rig) in &enemies {
        if !enemy_incapacitation_wheel_visible(*side, state.incapacitation) {
            continue;
        }
        let Ok(view) = viewer.get(entity) else {
            continue;
        };
        let will = view.skill_check(Skill::Will, LimbWeights::all_equal());
        let sources = state.incapacitation_sources(limbs.total_damage(), will);
        let head_position = rig
            .and_then(|rig| rig.get(&BoneRole::Head))
            .and_then(|head| bone_transforms.get(*head).ok())
            .map_or_else(
                || root_transform.translation() + Vec3::Y * ENEMY_WHEEL_ROOT_FALLBACK_HEIGHT_METRES,
                |head| head.translation() + Vec3::Y * ENEMY_WHEEL_HEAD_CLEARANCE_METRES,
            );
        let Ok(viewport_position) = camera.world_to_viewport(camera_transform, head_position)
        else {
            continue;
        };
        let center = Pos2::new(viewport_position.x, viewport_position.y);
        if !content_rect
            .expand(ENEMY_INCAPACITATION_WHEEL_RADIUS)
            .contains(center)
        {
            continue;
        }
        paint_incapacitation_wheel(
            &painter,
            center,
            state,
            sources,
            ENEMY_INCAPACITATION_WHEEL_RADIUS,
            ENEMY_INCAPACITATION_WHEEL_WIDTH,
        );
    }

    Ok(())
}

fn paint_incapacitation_wheel(
    painter: &egui::Painter,
    center: Pos2,
    state: &TacticalCombatState,
    sources: TacticalIncapacitationSources,
    radius: f32,
    width: f32,
) {
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(
            width,
            Color32::from_rgba_unmultiplied(0x10, 0x12, 0x16, 150),
        ),
    );

    if state.is_incapacitated() {
        painter.circle_stroke(
            center,
            radius,
            Stroke::new(
                width + 6.0,
                Color32::from_rgba_unmultiplied(0xc8, 0x47, 0x47, 70),
            ),
        );
    }

    let mut cursor = -std::f32::consts::FRAC_PI_2;
    let mut remaining = 1.0_f32;
    for (raw_amount, color) in forecast_wheel_segments(sources, state.projected_increase) {
        let Some(amount) = visible_incapacitation_wheel_amount(raw_amount, remaining) else {
            continue;
        };
        let end = cursor + amount * std::f32::consts::TAU;
        let steps = (amount * INCAPACITATION_WHEEL_RESOLUTION).ceil().max(2.0) as usize;
        let points = (0..=steps)
            .map(|step| {
                let angle = cursor + (end - cursor) * step as f32 / steps as f32;
                Pos2::new(
                    center.x + radius * angle.cos(),
                    center.y + radius * angle.sin(),
                )
            })
            .collect();
        painter.add(egui::Shape::line(points, Stroke::new(width, color)));
        cursor = end;
        remaining -= amount;
        if remaining <= 0.0 {
            break;
        }
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
        TacticalUiRoot,
        Visibility::Inherited,
        Node::default(),
        Styled::new(asset_server.load("ui.css")),
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
                    "WASD to move | Caps Lock: jog | Shift: sprint | Space to jump | Aim + Space + WASD: quickstep | Release Left Alt without WASD: prone/get up | Left Alt + WASD: dive (slide while sprinting) | Downed WASD: tank controls | Hold Space: align with camera | Hold Space + A/D: keep rolling | Mouse to look around | F9 to toggle camera\n",
                ),
                #[cfg(feature = "debug")]
                children![
                    TextSpan::new(
                        "DEBUG: F2 to toggle body | F3 to toggle hitbox | F4 to toggle hitscan"
                    ),
                    (GameSpeedDebugSpan, TextSpan::new(" | F7 game speed: 1x")),
                    (TerrainIkDebugSpan, TextSpan::new(" | F8 terrain IK: ON")),
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

fn update_weapon_guard_ui(
    guard: Res<WeaponGuardInputState>,
    players: Query<Entity, With<ControlledPlayer>>,
    viewer: TacticalPlayerViewer,
    mut spans: Query<&mut TextSpan, With<WeaponGuardSpan>>,
) {
    let ranged = players
        .iter()
        .next()
        .and_then(|entity| viewer.get(entity).ok())
        .is_some_and(|player| player.weapon_is_ranged());
    for mut span in &mut spans {
        **span = match guard.desired {
            WeaponGuardState::Lowered => "Lowered".to_owned(),
            WeaponGuardState::Raised if ranged => "Aiming".to_owned(),
            WeaponGuardState::Raised => "Blocking".to_owned(),
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

fn update_combat_state_ui(
    player: Single<&TacticalCombatState, With<ClientPlayer>>,
    mut span: Single<&mut TextSpan, With<CombatStateSpan>>,
) {
    span.0 = combat_state_label(player.into_inner());
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

#[expect(
    clippy::type_complexity,
    reason = "the Bevy ParamSet borrows distinct HUD text spans without aliasing mutable UI components"
)]
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

#[expect(
    clippy::type_complexity,
    reason = "the Bevy ParamSet borrows each independently addressed limb HUD span"
)]
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

#[expect(
    clippy::type_complexity,
    reason = "the Bevy ParamSet borrows independent server, client, and status HUD spans"
)]
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
                        DefenderResponse::Block { .. } => "Blocked",
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
            fatigue: 0.2,
            imbalance: 0.5,
            ..default()
        };
        assert_eq!(
            combat_state_label(&active),
            "Active | Blood loss 25% | Fatigue 20% | Imbalance 50%"
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

    #[test]
    fn incapacitation_wheel_uses_strategic_order_then_white_imbalance() {
        let segments = incapacitation_wheel_segments(TacticalIncapacitationSources {
            pain: 0.1,
            acute_trauma: 0.05,
            blood_loss: 0.2,
            fear: 0.3,
            fatigue: 0.4,
            hunger: 0.5,
            thirst: 0.6,
            thermal: 0.7,
            imbalance: 0.9,
            encumbrance: 0.1,
        });

        assert_eq!(
            segments.map(|(amount, _)| amount),
            [0.15, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.9, 0.1]
        );
        assert_eq!(segments[0].1, Color32::from_rgb(0xd9, 0x73, 0xa2));
        assert_eq!(segments[1].1, Color32::from_rgb(0xc8, 0x47, 0x47));
        assert_eq!(segments[3].1, Color32::from_rgb(0x20, 0x20, 0x20));
        assert_eq!(segments[7].1, Color32::WHITE);
    }

    #[test]
    fn incapacitation_wheel_hides_subpixel_segments_without_changing_state() {
        assert_eq!(visible_incapacitation_wheel_amount(0.0049, 1.0), None);
        assert_eq!(visible_incapacitation_wheel_amount(0.005, 1.0), Some(0.005));
    }

    #[test]
    fn enemy_incapacitation_wheel_is_absent_at_zero() {
        assert!(!enemy_incapacitation_wheel_visible(
            TacticalCombatSide::Enemy,
            0.0
        ));
        assert!(enemy_incapacitation_wheel_visible(
            TacticalCombatSide::Enemy,
            f32::EPSILON
        ));
        assert!(!enemy_incapacitation_wheel_visible(
            TacticalCombatSide::Party,
            0.5
        ));
    }
}
