//! Route handlers

pub mod characters;
mod data;
pub mod developer_quests;
pub mod dialogue;
pub mod evidence;
pub mod home;
mod inventory_forms;
pub mod investigation;
pub mod local_chat;
pub mod missions;
pub mod parties;
mod party_actions;
pub mod quests;
pub mod settlements;
pub(crate) mod travel;

use axum::{
    Router,
    extract::{Request, State},
    http::Uri,
    middleware::{self, Next},
    response::{IntoResponse, Json, Redirect, Response},
    routing::get,
};
use axum_extra::extract::CookieJar;
use serde::Serialize;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::live::LiveState;
use crate::session::{CHARACTER_COOKIE, Session};
use crate::spacetimedb::sql_string_literal;
use crate::spacetimedb::{
    BackendCaseSitePin, BackendCharacterCaseSiteLocation, Character, CharacterAttributes,
    CharacterLimbs, CharacterSkills, CharacterStats, CharacterStrategicCondition, CharacterTime,
    Party, PartyActionRequest, PartyJourney, PartyJourneyRoute, PartyMember, Settlement,
    SpacetimeClient, WorldClock,
};

/// Application state shared across routes
#[derive(Clone)]
pub struct AppState {
    pub db: SpacetimeClient,
    pub live: LiveState,
    pub strategic_map: Option<std::sync::Arc<crate::strategic_map::StrategicMap>>,
    pub terrain: Option<std::sync::Arc<travel::TerrainPlanner>>,
}

pub(crate) use party_actions::PartyAction;

pub(crate) enum PartyActionOutcome {
    Executed,
    Requested,
}

/// Accept a return destination only when it is a local absolute-path URL.
///
/// Workflows may carry this value through query strings and hidden form fields,
/// but every completion handler must validate it before emitting a redirect.
pub(crate) fn local_return_url(value: &str) -> Option<&str> {
    if !value.starts_with('/')
        || value.starts_with("//")
        || value.contains('\\')
        || value.chars().any(char::is_control)
    {
        return None;
    }
    let uri = value.parse::<Uri>().ok()?;
    (uri.scheme().is_none() && uri.authority().is_none() && uri.path().starts_with('/'))
        .then_some(value)
}

pub(crate) fn redirect_to_local(return_to: &str, fallback: &str) -> Redirect {
    Redirect::to(local_return_url(return_to).unwrap_or(fallback))
}

#[cfg(test)]
mod return_url_tests {
    use super::{local_return_url, terrain_mental_check};

    #[test]
    fn return_urls_are_local_paths_with_optional_query_and_fragment() {
        assert_eq!(
            local_return_url(
                "/locations/settlement/riverdale/map?destination=quest-1&target_surplus=1.5#plan"
            ),
            Some("/locations/settlement/riverdale/map?destination=quest-1&target_surplus=1.5#plan")
        );
        assert_eq!(local_return_url("https://example.com/steal"), None);
        assert_eq!(local_return_url("//example.com/steal"), None);
        assert_eq!(local_return_url("/\\example.com/steal"), None);
        assert_eq!(local_return_url("merchants"), None);
        assert_eq!(local_return_url("/safe\nLocation: /unsafe"), None);
    }

    #[test]
    fn party_purchase_funding_uses_shared_coin_before_personal_coin() {
        use adventuresim_core::strategic_economy::split_party_purchase_payment;

        assert_eq!(split_party_purchase_payment(8, 20, 15), Some((8, 7)));
        assert_eq!(split_party_purchase_payment(20, 8, 15), Some((15, 0)));
        assert_eq!(split_party_purchase_payment(4, 5, 10), None);
    }

    #[test]
    fn terrain_mental_check_applies_authoritative_head_health() {
        let healthy = terrain_mental_check(2.0, 2.0, 2.0, 1.0, 1.0);
        let injured = terrain_mental_check(2.0, 2.0, 2.0, 1.0, 0.5);
        let destroyed = terrain_mental_check(2.0, 2.0, 2.0, 1.0, 0.0);
        assert_eq!(healthy, 3.0);
        assert_eq!(injured, 2.0);
        assert_eq!(destroyed, 1.0);
    }
}

/// Corpses remain party members for rendering and history, but do not
/// participate in readiness checks that gate survivor actions.
pub(crate) fn participates_in_party_readiness(alive: bool) -> bool {
    alive
}

pub(crate) async fn character_case_site_id(
    state: &AppState,
    character_id: u64,
) -> Result<Option<String>, String> {
    state
        .db
        .query_one::<BackendCharacterCaseSiteLocation>(&format!(
            "SELECT * FROM backend_character_case_site_locations WHERE character_id = {character_id}"
        ))
        .await
        .map(|row| row.map(|location| location.case_site_id.value))
        .map_err(|error| error.to_string())
}

fn action_requires_ready_party(
    action: &PartyAction,
    character_case_site_id: Option<&str>,
    party: &Party,
) -> bool {
    match action {
        PartyAction::TravelToSettlement { .. } => !character_case_site_id.is_some_and(|site_id| {
            party
                .current_case_site_id
                .as_ref()
                .is_some_and(|party_site| party_site.value == site_id)
        }),
        _ => action.requires_ready_party(),
    }
}

/// Execute a leader action immediately, or persist the same validated intent for
/// the party leader when a member attempts it.
pub(crate) async fn execute_or_request_party_action(
    state: &AppState,
    actor_id: u64,
    action: PartyAction,
) -> Result<PartyActionOutcome, String> {
    let character = state
        .db
        .query_one::<Character>(&format!("SELECT * FROM character WHERE id = {actor_id}"))
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Character has no party")?;
    let party = state
        .db
        .query_one::<Party>(&format!(
            "SELECT * FROM party WHERE id = {}",
            sql_string_literal(&party_id)
        ))
        .await
        .map_err(|e| e.to_string())?
        .ok_or("Party not found")?;
    let actor_case_site_id = if matches!(&action, PartyAction::TravelToSettlement { .. }) {
        character_case_site_id(state, actor_id).await?
    } else {
        None
    };
    if action_requires_ready_party(&action, actor_case_site_id.as_deref(), &party) {
        let members = state
            .db
            .query::<PartyMember>(&format!(
                "SELECT * FROM party_member WHERE party_id = {}",
                sql_string_literal(&party.id)
            ))
            .await
            .map_err(|error| error.to_string())?;
        for membership in members {
            let member = state
                .db
                .query_one::<Character>(&format!(
                    "SELECT * FROM character WHERE id = {}",
                    membership.character_id
                ))
                .await
                .map_err(|error| error.to_string())?
                .ok_or("Party member not found")?;
            if !participates_in_party_readiness(member.alive) {
                continue;
            }
            state
                .db
                .call("refresh_strategic_condition", &[json!(member.id)])
                .await
                .map_err(|error| error.to_string())?;
            let condition = state
                .db
                .query_one::<CharacterStrategicCondition>(&format!(
                    "SELECT * FROM character_strategic_condition WHERE character_id = {}",
                    member.id
                ))
                .await
                .map_err(|error| error.to_string())?
                .ok_or("Party member condition not found")?;
            if condition.status == "incapacitated" {
                return Err(
                    "An incapacitated party member must recover before the party can act".into(),
                );
            }
        }
    }
    if party.leader_id == actor_id {
        let planned = planned_travel_call(state, actor_id, &action).await?;
        let (reducer, args) = planned.unwrap_or_else(|| action.reducer_call(actor_id));
        state
            .db
            .call(reducer, &args)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(PartyActionOutcome::Executed);
    }
    let kind = action.kind();
    let summary = action.summary();
    let payload = serde_json::to_string(&action).map_err(|e| e.to_string())?;
    state
        .db
        .call(
            "request_party_action",
            &[
                json!(actor_id),
                json!(&kind),
                json!(summary),
                json!(payload),
            ],
        )
        .await
        .map_err(|e| e.to_string())?;

    // Temporary NPC captains always approve after a short, visible delay.
    let leader = state
        .db
        .query_one::<Character>(&format!(
            "SELECT * FROM character WHERE id = {}",
            party.leader_id
        ))
        .await
        .map_err(|e| e.to_string())?;
    if leader.is_some_and(|leader| leader.temporary) {
        let state = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let requests = state
                .db
                .query::<PartyActionRequest>(&format!(
                    "SELECT * FROM party_action_request WHERE party_id = {}",
                    sql_string_literal(&party_id)
                ))
                .await
                .unwrap_or_default();
            for request in requests
                .into_iter()
                .filter(|request| request.requester_id == actor_id && request.action_kind == kind)
            {
                if let Err(error) = approve_party_action(&state, party.leader_id, &request).await {
                    tracing::warn!(%error, "temporary captain could not approve party action");
                }
            }
        });
    }
    Ok(PartyActionOutcome::Requested)
}

pub(crate) async fn party_terrain_profile(
    state: &AppState,
    actor: &Character,
) -> Result<adventuresim_terrain::TerrainSkillProfile, String> {
    let member_ids = if let Some(party_id) = actor.party_id.as_deref() {
        state
            .db
            .query::<PartyMember>(&format!(
                "SELECT * FROM party_member WHERE party_id = {}",
                sql_string_literal(party_id)
            ))
            .await
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|member| member.character_id)
            .collect::<Vec<_>>()
    } else {
        vec![actor.id]
    };
    let mut checks = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for id in member_ids {
        let Some(character) = state
            .db
            .query_one::<Character>(&format!("SELECT * FROM character WHERE id = {id}"))
            .await
            .map_err(|e| e.to_string())?
        else {
            continue;
        };
        if !character.alive {
            continue;
        }
        let Some(attributes) = state
            .db
            .query_one::<CharacterAttributes>(&format!(
                "SELECT * FROM character_attributes WHERE character_id = {id}"
            ))
            .await
            .map_err(|e| e.to_string())?
        else {
            continue;
        };
        let Some(stats) = state
            .db
            .query_one::<CharacterStats>(&format!(
                "SELECT * FROM character_stats WHERE character_id = {id}"
            ))
            .await
            .map_err(|e| e.to_string())?
        else {
            continue;
        };
        let Some(limbs) = state
            .db
            .query_one::<CharacterLimbs>(&format!(
                "SELECT * FROM character_limbs WHERE character_id = {id}"
            ))
            .await
            .map_err(|e| e.to_string())?
        else {
            continue;
        };
        let Some(skills) = state
            .db
            .query_one::<CharacterSkills>(&format!(
                "SELECT * FROM character_skills WHERE character_id = {id}"
            ))
            .await
            .map_err(|e| e.to_string())?
        else {
            continue;
        };
        for (index, (skill, hours)) in [
            (
                adventuresim_core::skill::Skill::TerrainPlains,
                skills.terrain_plains_hours,
            ),
            (
                adventuresim_core::skill::Skill::TerrainForest,
                skills.terrain_forest_hours,
            ),
            (
                adventuresim_core::skill::Skill::TerrainHills,
                skills.terrain_hills_hours,
            ),
            (
                adventuresim_core::skill::Skill::TerrainUrban,
                skills.terrain_urban_hours,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            checks[index].push(terrain_mental_check(
                skill.training_rank(hours),
                attributes.instinct,
                attributes.intelligence,
                stats.focus,
                limbs.head_health,
            ));
        }
    }
    let aggregate = |values: &[f32]| {
        (adventuresim_core::capability::aggregate_bounded_party_check(values.iter().copied())
            .clamp(0.0, 5.0)
            * 1_000.0)
            .round() as u16
    };
    Ok(adventuresim_terrain::TerrainSkillProfile {
        plains: aggregate(&checks[0]),
        forest: aggregate(&checks[1]),
        hills: aggregate(&checks[2]),
        urban: aggregate(&checks[3]),
    })
}

fn terrain_mental_check(
    training_rank: f32,
    instinct: f32,
    intelligence: f32,
    focus: f32,
    head_health: f32,
) -> f32 {
    let head_health = head_health.clamp(0.0, 1.0);
    let attribute_check =
        instinct * head_health + intelligence * head_health * focus.clamp(0.0, 1.0);
    ((training_rank + attribute_check) * 0.5).clamp(0.0, 5.0)
}

async fn planned_travel_call(
    state: &AppState,
    actor_id: u64,
    action: &PartyAction,
) -> Result<Option<(&'static str, Vec<serde_json::Value>)>, String> {
    let Some(terrain) = state.terrain.as_deref() else {
        return Ok(None);
    };
    let character = state
        .db
        .query_one::<Character>(&format!("SELECT * FROM character WHERE id = {actor_id}"))
        .await
        .map_err(|error| error.to_string())?
        .ok_or("Character not found")?;
    let terrain_profile = party_terrain_profile(state, &character).await?;
    let (reducer, destination) = match action {
        PartyAction::TravelToSettlement { settlement_id } => {
            let destination = state
                .db
                .query_one::<Settlement>(&format!(
                    "SELECT * FROM settlement WHERE id = {}",
                    sql_string_literal(settlement_id)
                ))
                .await
                .map_err(|error| error.to_string())?
                .ok_or("Settlement not found")?;
            (
                "travel_to_settlement_planned",
                (destination.coord_y, destination.coord_x),
            )
        }
        PartyAction::TravelToCaseSite { case_site_id } => {
            let destination = state
                .db
                .query_one::<BackendCaseSitePin>(&format!(
                    "SELECT * FROM backend_case_site_pins WHERE owner_character_id = {actor_id} AND case_site_id = {}",
                    sql_string_literal(case_site_id)
                ))
                .await
                .map_err(|error| error.to_string())?
                .ok_or("Known exact case site not found")?;
            (
                "travel_to_case_site_planned",
                (
                    f64::from(destination.latitude_e7) / 10_000_000.0,
                    f64::from(destination.longitude_e7) / 10_000_000.0,
                ),
            )
        }
        _ => return Ok(None),
    };
    let origin = if let Some(id) = character.current_settlement_id.as_deref() {
        let settlement = state
            .db
            .query_one::<Settlement>(&format!(
                "SELECT * FROM settlement WHERE id = {}",
                sql_string_literal(id)
            ))
            .await
            .map_err(|error| error.to_string())?
            .ok_or("Origin settlement not found")?;
        (settlement.coord_y, settlement.coord_x)
    } else if let Some(id) = character_case_site_id(state, actor_id).await? {
        let site = state
            .db
            .query_one::<BackendCaseSitePin>(&format!(
                "SELECT * FROM backend_case_site_pins WHERE owner_character_id = {actor_id} AND case_site_id = {}",
                sql_string_literal(&id)
            ))
            .await
            .map_err(|error| error.to_string())?
            .ok_or("Known exact origin case site not found")?;
        (
            f64::from(site.latitude_e7) / 10_000_000.0,
            f64::from(site.longitude_e7) / 10_000_000.0,
        )
    } else if let Some(party_id) = character.party_id.as_deref() {
        let journey = state
            .db
            .query_one::<PartyJourney>(&format!(
                "SELECT * FROM party_journey WHERE party_id = {}",
                sql_string_literal(party_id)
            ))
            .await
            .map_err(|error| error.to_string())?
            .ok_or("Camp journey not found")?;
        let route = state
            .db
            .query_one::<PartyJourneyRoute>(&format!(
                "SELECT * FROM party_journey_route WHERE party_id = {}",
                sql_string_literal(party_id)
            ))
            .await
            .map_err(|error| error.to_string())?
            .ok_or("Camp terrain route not found")?;
        persisted_route_position(&route, journey.completed_minutes)
            .ok_or("Camp terrain route position is unavailable")?
    } else {
        return Ok(None);
    };
    let plan = match terrain
        .plan_with_profile(origin, destination, terrain_profile)
        .await
    {
        Ok(plan) => plan,
        Err(error) => {
            tracing::warn!(%error, actor_id, "terrain route unavailable at execution; using unplanned travel reducer");
            return Ok(None);
        }
    };
    let return_plan = if matches!(action, PartyAction::TravelToCaseSite { .. }) {
        match terrain
            .plan_with_profile(destination, origin, terrain_profile)
            .await
        {
            Ok(plan) => Some(plan),
            Err(error) => {
                tracing::warn!(%error, actor_id, "case-site return terrain route unavailable at execution; using unplanned travel reducer");
                return Ok(None);
            }
        }
    } else {
        None
    };
    let route_json = terrain_route_json(terrain.digest(), &plan, return_plan.as_ref());
    let destination_id = match action {
        PartyAction::TravelToSettlement { settlement_id } => json!(settlement_id),
        PartyAction::TravelToCaseSite { case_site_id } => {
            json!({ "value": case_site_id })
        }
        _ => unreachable!(),
    };
    Ok(Some((
        reducer,
        vec![json!(actor_id), destination_id, route_json],
    )))
}

fn persisted_route_position(route: &PartyJourneyRoute, minute: u64) -> Option<(f64, f64)> {
    let coordinate = |point: &crate::spacetimedb::JourneyRoutePoint| {
        (
            f64::from(point.latitude_e7) / 10_000_000.0,
            f64::from(point.longitude_e7) / 10_000_000.0,
        )
    };
    let distance = |from: (f64, f64), to: (f64, f64)| {
        let earth_radius_m = 6_371_000.0_f64;
        let lat1 = from.0.to_radians();
        let lat2 = to.0.to_radians();
        let delta_lat = (to.0 - from.0).to_radians();
        let delta_lon = (to.1 - from.1).to_radians();
        let a = (delta_lat / 2.0).sin().powi(2)
            + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2);
        (earth_radius_m * 2.0 * a.sqrt().atan2((1.0 - a).sqrt())).round() as u64
    };
    let lengths = route
        .points
        .windows(2)
        .map(|pair| distance(coordinate(&pair[0]), coordinate(&pair[1])))
        .collect::<Vec<_>>();
    let total = lengths.iter().sum::<u64>();
    if total == 0 || route.minutes == 0 {
        return route.points.first().map(coordinate);
    }
    let target = total.saturating_mul(minute.min(route.minutes)) / route.minutes;
    let mut traversed = 0_u64;
    for (index, length) in lengths.into_iter().enumerate() {
        if traversed.saturating_add(length) >= target {
            let from = coordinate(&route.points[index]);
            let to = coordinate(&route.points[index + 1]);
            let fraction = if length == 0 {
                0.0
            } else {
                target.saturating_sub(traversed) as f64 / length as f64
            };
            return Some((
                from.0 + (to.0 - from.0) * fraction,
                from.1 + (to.1 - from.1) * fraction,
            ));
        }
        traversed = traversed.saturating_add(length);
    }
    route.points.last().map(coordinate)
}

fn terrain_route_json(
    digest: &str,
    plan: &adventuresim_terrain::RoutePlan,
    return_plan: Option<&adventuresim_terrain::RoutePlan>,
) -> serde_json::Value {
    let leg_json = |plan: &adventuresim_terrain::RoutePlan| {
        json!({
            "distance_m": plan.distance_m,
            "minutes": plan.minutes,
            "points": plan.points.iter().map(|point| json!({"latitude_e7":(point.latitude*10_000_000.0).round() as i32,"longitude_e7":(point.longitude*10_000_000.0).round() as i32})).collect::<Vec<_>>(),
            "spans": plan.spans.iter().filter_map(|span| { let kind=match span.surface { adventuresim_terrain::Surface::Road=>"Road",adventuresim_terrain::Surface::Open=>"Open",adventuresim_terrain::Surface::SparseWoods=>"SparseWoods",adventuresim_terrain::Surface::DeepWoods=>"DeepWoods",adventuresim_terrain::Surface::Wetland=>"Wetland",adventuresim_terrain::Surface::Water=>return None};Some(json!({"kind":kind,"terrain":span.terrain,"training_multiplier_permille":span.training_multiplier_permille,"check_millirank":span.check_millirank,"start_minute":span.start_minute,"duration_minutes":span.duration_minutes})) }).collect::<Vec<_>>()
        })
    };
    json!({
        "package_digest": digest,
        "distance_m": plan.distance_m,
        "minutes": plan.minutes,
        "points": plan.points.iter().map(|point| json!({"latitude_e7":(point.latitude*10_000_000.0).round() as i32,"longitude_e7":(point.longitude*10_000_000.0).round() as i32})).collect::<Vec<_>>(),
        "spans": plan.spans.iter().filter_map(|span| { let kind=match span.surface { adventuresim_terrain::Surface::Road=>"Road",adventuresim_terrain::Surface::Open=>"Open",adventuresim_terrain::Surface::SparseWoods=>"SparseWoods",adventuresim_terrain::Surface::DeepWoods=>"DeepWoods",adventuresim_terrain::Surface::Wetland=>"Wetland",adventuresim_terrain::Surface::Water=>return None};Some(json!({"kind":kind,"terrain":span.terrain,"training_multiplier_permille":span.training_multiplier_permille,"check_millirank":span.check_millirank,"start_minute":span.start_minute,"duration_minutes":span.duration_minutes})) }).collect::<Vec<_>>(),
        "return_route": return_plan.map(leg_json)
    })
}

#[cfg(test)]
mod readiness_tests {
    use super::{PartyAction, action_requires_ready_party, participates_in_party_readiness};
    use crate::spacetimedb::{CampDurationMode, CaseSiteId, Party};

    fn party(case_site_id: Option<&str>) -> Party {
        Party {
            id: "party".into(),
            name: "Party".into(),
            leader_id: 7,
            current_settlement_id: case_site_id.is_none().then(|| "ironforge".into()),
            current_case_site_id: case_site_id.map(|value| CaseSiteId {
                value: value.into(),
            }),
            active_contract_id: None,
            is_solo: true,
            camp_fatigue_percent: 50,
            walking_minutes_per_day: 480,
            travel_at_night: false,
            camp_duration_mode: CampDurationMode::Auto,
            fixed_camp_minutes: 0,
            camp_destination: None,
            camp_remaining_minutes: 0,
            pooled_water_ml: 0.0,
            medicine_target: 0.0,
            command_target: 0.0,
            religion_target: 0.0,
        }
    }

    #[test]
    fn corpses_do_not_participate_in_party_readiness() {
        assert!(participates_in_party_readiness(true));
        assert!(!participates_in_party_readiness(false));
    }

    #[test]
    fn only_exact_case_site_settlement_withdrawal_bypasses_web_readiness() {
        let withdrawal = PartyAction::TravelToSettlement {
            settlement_id: "ironforge".into(),
        };
        let onsite_party = party(Some("site:old-graveyard"));
        assert!(!action_requires_ready_party(
            &withdrawal,
            Some("site:old-graveyard"),
            &onsite_party
        ));
        assert!(action_requires_ready_party(
            &withdrawal,
            Some("site:other"),
            &onsite_party
        ));
        assert!(action_requires_ready_party(
            &withdrawal,
            None,
            &onsite_party
        ));
        assert!(action_requires_ready_party(&withdrawal, None, &party(None)));

        let investigate = PartyAction::PerformInvestigation {
            action_id: "action:inspect".into(),
            method: "inspect_site".into(),
            expected_version: 1,
        };
        assert!(action_requires_ready_party(
            &investigate,
            Some("site:old-graveyard"),
            &onsite_party
        ));
    }
}

pub(crate) async fn approve_party_action(
    state: &AppState,
    leader_id: u64,
    request: &PartyActionRequest,
) -> Result<(), String> {
    if let Ok(action) = serde_json::from_str::<PartyAction>(&request.payload)
        && let Some((_, args)) = planned_travel_call(state, leader_id, &action).await?
    {
        return state
            .db
            .call(
                "approve_party_action_request_planned",
                &[json!(leader_id), json!(request.id), args[2].clone()],
            )
            .await
            .map_err(|error| error.to_string());
    }
    state
        .db
        .call(
            "approve_party_action_request",
            &[json!(leader_id), json!(request.id)],
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Build the complete router
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route(
            crate::strategic_map::DATA_LICENSE_PATH,
            get(crate::strategic_map::data_license),
        )
        .route(
            "/map/tiles/{theme}/{zoom}/{x}/{tile}",
            get(crate::strategic_map::world_tile),
        )
        .merge(characters::routes())
        .merge(
            Router::new()
                .merge(home::routes())
                .merge(investigation::routes())
                .merge(dialogue::routes())
                .merge(developer_quests::routes())
                .merge(evidence::routes())
                .merge(local_chat::routes())
                .merge(settlements::routes())
                .merge(parties::routes())
                .merge(quests::routes())
                .merge(missions::routes())
                .merge(crate::live::routes())
                .route("/time", get(current_time))
                .layer(middleware::from_fn(require_active_character)),
        )
        .with_state(state)
}

#[derive(Serialize)]
struct CurrentTime {
    character_minutes: u64,
    official_minutes: u64,
}

async fn current_time(State(state): State<AppState>, session: Session) -> Response {
    let Some(character_id) = session.character_id_u64() else {
        return Json(CurrentTime {
            character_minutes: 0,
            official_minutes: 0,
        })
        .into_response();
    };
    let character_time_sql =
        format!("SELECT * FROM character_time WHERE character_id = {character_id}");
    let (character_time, world_clock) = tokio::join!(
        state.db.query::<CharacterTime>(&character_time_sql),
        state
            .db
            .query::<WorldClock>("SELECT * FROM world_clock WHERE id = 0"),
    );
    let character_time = match character_time {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%error, "failed to load character time");
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "Strategic time is unavailable",
            )
                .into_response();
        }
    };
    let world_clock = match world_clock {
        Ok(value) => value,
        Err(error) => {
            tracing::error!(%error, "failed to load world clock");
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "Strategic time is unavailable",
            )
                .into_response();
        }
    };
    let official_minutes = world_clock.first().map_or(0, |clock| {
        let now_micros = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros();
        let elapsed_micros = now_micros.saturating_sub(clock.epoch_micros.max(0) as u128);
        (elapsed_micros.saturating_mul(73) / 84_000_000) as u64
    });
    Json(CurrentTime {
        character_minutes: character_time.first().map_or(0, |time| time.minutes),
        official_minutes,
    })
    .into_response()
}

/// Strategic screens have no anonymous mode. Character creation and selection
/// remain public entry screens; every other route requires a selected character.
async fn require_active_character(request: Request, next: Next) -> Response {
    let cookies = CookieJar::from_headers(request.headers());
    if cookies.get(CHARACTER_COOKIE).is_none() {
        return Redirect::to("/characters").into_response();
    }
    next.run(request).await
}
