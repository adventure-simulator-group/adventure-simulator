//! Party route handlers

use axum::{
    Form, Router,
    extract::{Path, State},
    response::{Html, Json, Redirect},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::{AppState, approve_party_action, execute_or_request_party_action};
use crate::session::Session;
use crate::spacetimedb::{
    Character, CharacterAttributes, CharacterCapability, CharacterLimbs, CharacterSkills, Party,
    PartyActionRequest, PartyJoinRequest, PartyLeaderVote, PartyMember, PartyRecruitmentRole,
    RecruitmentRequirements, SavedRecruitmentRole,
};
use crate::templates::recruitment::{
    PartyCheckSummary, RecruitmentApplicant, RecruitmentRolePanel, recruitment_panel,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/party-roles/{id}/join", post(join_party))
        .route("/parties/{id}/join-general", post(join_general_party))
        .route("/party-recruitment/panel", get(recruitment_panel_fragment))
        .route("/party-recruitment/roles", post(create_recruitment_role))
        .route(
            "/party-recruitment/roles/{id}",
            post(update_recruitment_role),
        )
        .route(
            "/party-recruitment/roles/{id}/delete",
            post(delete_recruitment_role),
        )
        .route("/party-recruitment/saved", post(save_recruitment_role))
        .route(
            "/party-recruitment/check-targets",
            post(update_party_check_targets),
        )
        .route(
            "/party-recruitment/saved/{id}/delete",
            post(delete_saved_role),
        )
        .route(
            "/party-recruitment/saved/{id}/rename",
            post(rename_saved_role),
        )
        .route("/characters/{id}/capabilities", get(character_capabilities))
        .route(
            "/parties/{id}/requests/{request_id}/accept",
            post(accept_join_request),
        )
        .route(
            "/parties/{id}/requests/{request_id}/reject",
            post(reject_join_request),
        )
        .route("/party-notifications", get(party_notifications))
        .route(
            "/party-action-requests/{id}/approve",
            post(approve_action_request),
        )
        .route(
            "/party-action-requests/{id}/deny",
            post(deny_action_request),
        )
        .route("/party-leader-votes/{candidate_id}", post(vote_for_leader))
        .route("/parties/{id}/leave", post(leave_party))
        .route("/parties/{id}/disband", post(disband_party))
}

#[derive(Default, Deserialize)]
struct RecruitmentRoleForm {
    #[serde(default)]
    name: String,
    quantity: u32,
    #[serde(default)]
    save_role: bool,
    #[serde(default)]
    melee: bool,
    #[serde(default)]
    ranged: bool,
    #[serde(default)]
    weapon_precision: f32,
    #[serde(default)]
    heavy: bool,
    #[serde(default)]
    armor_tier: u8,
    #[serde(default)]
    athletics: u8,
    #[serde(default)]
    endurance: u8,
}

impl RecruitmentRoleForm {
    fn requirements(&self) -> RecruitmentRequirements {
        RecruitmentRequirements {
            melee: self.melee,
            ranged: self.ranged,
            precise: false,
            heavy: self.heavy,
            quarter_armor: self.armor_tier == 1,
            half_armor: self.armor_tier == 2,
            three_quarter_armor: self.armor_tier == 3,
            full_armor: self.armor_tier == 4,
            blunt: false,
            slash: false,
            pierce: false,
            athletics: self.athletics,
            endurance: self.endurance,
            medicine: 0,
            surgery: 0,
            charisma: 0,
            faith: 0,
        }
    }

    fn weapon_precision(&self) -> f32 {
        ((self.weapon_precision * 2.0).round() / 2.0)
            .clamp(0.0, adventuresim_core::capability::WEAPON_PRECISION_RAPIER)
    }
}

#[derive(Deserialize)]
struct PartyCheckTargetsForm {
    medicine: f32,
    surgery: f32,
    charisma: f32,
    faith: f32,
}

async fn update_party_check_targets(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<PartyCheckTargetsForm>,
) -> Redirect {
    if let Some(actor_id) = session.character_id_u64() {
        let _ = execute_or_request_party_action(
            &state,
            actor_id,
            "party_checks",
            "Change party skill targets",
            "update_party_check_targets",
            vec![
                json!(actor_id),
                json!(form.medicine),
                json!(form.surgery),
                json!(form.charisma),
                json!(form.faith),
            ],
        )
        .await;
    }
    Redirect::to("/")
}

async fn create_recruitment_role(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<RecruitmentRoleForm>,
) -> Redirect {
    let Some(actor_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    let requirements = form.requirements();
    let outcome = execute_or_request_party_action(
        &state,
        actor_id,
        "add_role",
        &format!("Add {} {} slot(s)", form.quantity, form.name),
        "create_recruitment_role",
        vec![
            json!(actor_id),
            json!(form.name),
            json!(form.quantity),
            json!(requirements),
            json!(form.weapon_precision()),
            json!(form.save_role),
        ],
    )
    .await;
    if let Err(error) = outcome {
        tracing::warn!("Failed to create recruitment role: {error:?}");
        return Redirect::to("/");
    }
    if state.db.is_local() && matches!(outcome, Ok(super::PartyActionOutcome::Executed)) {
        if let Some(character) = get_character(&state, actor_id).await {
            if let Some(party_id) = character.party_id {
                let roles: Vec<PartyRecruitmentRole> = state
                    .db
                    .query(&format!(
                        "SELECT * FROM party_recruitment_role WHERE party_id = '{}'",
                        party_id
                    ))
                    .await
                    .unwrap_or_default();
                if let Some(role) = roles.into_iter().max_by_key(|role| role.id) {
                    let _ = state
                        .db
                        .call("seed_bot_join_requests", &[json!(role.id)])
                        .await;
                }
            }
        }
    }
    Redirect::to("/")
}

async fn update_recruitment_role(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    session: Session,
    Form(form): Form<RecruitmentRoleForm>,
) -> Redirect {
    let Some(actor_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    let _ = execute_or_request_party_action(
        &state,
        actor_id,
        &format!("edit_role:{id}"),
        &format!("Edit recruitment role {}", form.name),
        "update_recruitment_role",
        vec![
            json!(actor_id),
            json!(id),
            json!(form.name),
            json!(form.quantity),
            json!(form.requirements()),
            json!(form.weapon_precision()),
        ],
    )
    .await;
    Redirect::to("/")
}

async fn delete_recruitment_role(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    session: Session,
) -> Redirect {
    let Some(actor_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    let _ = execute_or_request_party_action(
        &state,
        actor_id,
        &format!("delete_role:{id}"),
        "Delete a recruitment role",
        "delete_recruitment_role",
        vec![json!(actor_id), json!(id)],
    )
    .await;
    Redirect::to("/")
}

async fn delete_saved_role(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    session: Session,
) -> Redirect {
    if let Some(owner_id) = session.character_id_u64() {
        let _ = state
            .db
            .call(
                "delete_saved_recruitment_role",
                &[json!(owner_id), json!(id)],
            )
            .await;
    }
    Redirect::to("/")
}

async fn save_recruitment_role(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<RecruitmentRoleForm>,
) -> Redirect {
    if let Some(owner_id) = session.character_id_u64() {
        let _ = state
            .db
            .call(
                "save_recruitment_role",
                &[
                    json!(owner_id),
                    json!(form.name),
                    json!(form.requirements()),
                    json!(form.weapon_precision()),
                ],
            )
            .await;
    }
    Redirect::to("/")
}

#[derive(Deserialize)]
struct RenameSavedRoleForm {
    name: String,
}

async fn rename_saved_role(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    session: Session,
    Form(form): Form<RenameSavedRoleForm>,
) -> Redirect {
    if let Some(owner_id) = session.character_id_u64() {
        let _ = state
            .db
            .call(
                "rename_saved_recruitment_role",
                &[json!(owner_id), json!(id), json!(form.name)],
            )
            .await;
    }
    Redirect::to("/")
}

async fn join_party(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    session: Session,
) -> Redirect {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };

    let _ = state
        .db
        .call("request_to_join_party", &[json!(character_id), json!(id)])
        .await;

    let role = state
        .db
        .query::<PartyRecruitmentRole>(&format!(
            "SELECT * FROM party_recruitment_role WHERE id = {id}"
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    if let Some(role) = role {
        let party = state
            .db
            .query::<Party>(&format!(
                "SELECT * FROM party WHERE id = '{}'",
                role.party_id
            ))
            .await
            .unwrap_or_default()
            .into_iter()
            .next();
        if let Some(party) = party {
            let leader = get_character(&state, party.leader_id).await;
            if leader.is_some_and(|leader| leader.temporary) {
                let request = state
                    .db
                    .query::<PartyJoinRequest>(&format!(
                        "SELECT * FROM party_join_request WHERE character_id = {character_id}"
                    ))
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .find(|request| request.recruitment_role_id == id);
                if let Some(request) = request {
                    let _ = state
                        .db
                        .call(
                            "accept_party_join_request",
                            &[json!(party.leader_id), json!(request.id)],
                        )
                        .await;
                }
            }
        }
    }

    Redirect::to("/")
}

async fn join_general_party(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Redirect {
    if let Some(character_id) = session.character_id_u64() {
        let _ = state
            .db
            .call(
                "request_general_party_join",
                &[json!(character_id), json!(id)],
            )
            .await;
    }
    Redirect::to("/")
}

async fn recruitment_panel_fragment(
    State(state): State<AppState>,
    session: Session,
) -> Html<String> {
    let Some(character_id) = session.character_id_u64() else {
        return Html(String::new());
    };
    let Some(character) = get_character(&state, character_id).await else {
        return Html(String::new());
    };
    let Some(party_id) = character.party_id else {
        return Html(String::new());
    };
    let Some(party) = state
        .db
        .query::<Party>(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
        .await
        .unwrap_or_default()
        .into_iter()
        .next()
    else {
        return Html(String::new());
    };
    let roles: Vec<PartyRecruitmentRole> = state
        .db
        .query(&format!(
            "SELECT * FROM party_recruitment_role WHERE party_id = '{}'",
            party_id
        ))
        .await
        .unwrap_or_default();
    let memberships: Vec<PartyMember> = state
        .db
        .query(&format!(
            "SELECT * FROM party_member WHERE party_id = '{}'",
            party_id
        ))
        .await
        .unwrap_or_default();
    let requests: Vec<PartyJoinRequest> = state
        .db
        .query(&format!(
            "SELECT * FROM party_join_request WHERE party_id = '{}'",
            party_id
        ))
        .await
        .unwrap_or_default();
    let saved: Vec<SavedRecruitmentRole> = state
        .db
        .query(&format!(
            "SELECT * FROM saved_recruitment_role WHERE owner_character_id = {}",
            character_id
        ))
        .await
        .unwrap_or_default();
    let mut member_capabilities = Vec::new();
    for membership in &memberships {
        let _ = state
            .db
            .call("refresh_capabilities", &[json!(membership.character_id)])
            .await;
        if let Some(capability) = state
            .db
            .query::<CharacterCapability>(&format!(
                "SELECT * FROM character_capability WHERE character_id = {}",
                membership.character_id
            ))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
        {
            member_capabilities.push(capability);
        }
    }
    let medicine: Vec<f32> = member_capabilities
        .iter()
        .map(|value| value.medicine)
        .collect();
    let surgery: Vec<f32> = member_capabilities
        .iter()
        .map(|value| value.surgery)
        .collect();
    let charisma: Vec<f32> = member_capabilities
        .iter()
        .map(|value| value.charisma)
        .collect();
    let faith: Vec<f32> = member_capabilities
        .iter()
        .map(|value| value.faith)
        .collect();
    let checks = PartyCheckSummary {
        medicine: adventuresim_core::capability::aggregate_party_check(medicine.iter().copied()),
        surgery: adventuresim_core::capability::aggregate_party_check(surgery.iter().copied()),
        charisma: adventuresim_core::capability::aggregate_party_check(charisma.iter().copied()),
        faith: adventuresim_core::capability::aggregate_party_check(faith.iter().copied()),
    };
    let mut panels = Vec::new();
    for role in roles {
        let mut filled = Vec::new();
        for membership in memberships
            .iter()
            .filter(|member| member.recruitment_role_id == Some(role.id))
        {
            if let Some(character) = get_character(&state, membership.character_id).await {
                filled.push(character);
            }
        }
        let mut applicants = Vec::new();
        for request in requests
            .iter()
            .filter(|request| request.recruitment_role_id == role.id)
        {
            if let Some(character) = get_character(&state, request.character_id).await {
                let _ = state
                    .db
                    .call("refresh_capabilities", &[json!(request.character_id)])
                    .await;
                let capability = state
                    .db
                    .query::<CharacterCapability>(&format!(
                        "SELECT * FROM character_capability WHERE character_id = {}",
                        request.character_id
                    ))
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .next();
                let attributes = state
                    .db
                    .query::<CharacterAttributes>(&format!(
                        "SELECT * FROM character_attributes WHERE character_id = {}",
                        request.character_id
                    ))
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .next();
                let skills = state
                    .db
                    .query::<CharacterSkills>(&format!(
                        "SELECT * FROM character_skills WHERE character_id = {}",
                        request.character_id
                    ))
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .next();
                let limbs = state
                    .db
                    .query::<CharacterLimbs>(&format!(
                        "SELECT * FROM character_limbs WHERE character_id = {}",
                        request.character_id
                    ))
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .next();
                let contribution =
                    capability
                        .as_ref()
                        .map_or_default(|candidate| PartyCheckSummary {
                            medicine: adventuresim_core::capability::aggregate_party_contribution(
                                &medicine,
                                candidate.medicine,
                            ),
                            surgery: adventuresim_core::capability::aggregate_party_contribution(
                                &surgery,
                                candidate.surgery,
                            ),
                            charisma: adventuresim_core::capability::aggregate_party_contribution(
                                &charisma,
                                candidate.charisma,
                            ),
                            faith: adventuresim_core::capability::aggregate_party_contribution(
                                &faith,
                                candidate.faith,
                            ),
                        });
                applicants.push(RecruitmentApplicant {
                    request: request.clone(),
                    character,
                    capability,
                    attributes,
                    skills,
                    limbs,
                    contribution,
                });
            }
        }
        panels.push(RecruitmentRolePanel {
            role,
            filled,
            requests: applicants,
        });
    }
    Html(recruitment_panel(&party, character_id, &panels, &saved, checks).into_string())
}

#[derive(Serialize)]
struct CapabilityResponse {
    tags: Vec<String>,
}

async fn character_capabilities(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Json<CapabilityResponse> {
    let _ = state.db.call("refresh_capabilities", &[json!(id)]).await;
    let capability = state
        .db
        .query::<CharacterCapability>(&format!(
            "SELECT * FROM character_capability WHERE character_id = {id}"
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next();
    Json(CapabilityResponse {
        tags: capability
            .as_ref()
            .map(CharacterCapability::summary_tags)
            .unwrap_or_default(),
    })
}

async fn accept_join_request(
    State(state): State<AppState>,
    Path((id, request_id)): Path<(String, u64)>,
    session: Session,
) -> Redirect {
    let Some(actor_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    let _ = execute_or_request_party_action(
        &state,
        actor_id,
        "accept_join",
        &format!("Accept join request {request_id}"),
        "accept_party_join_request",
        vec![json!(actor_id), json!(request_id)],
    )
    .await;
    Redirect::to(&party_location_url(&state, &id).await)
}

async fn reject_join_request(
    State(state): State<AppState>,
    Path((id, request_id)): Path<(String, u64)>,
    session: Session,
) -> Redirect {
    let Some(actor_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    let _ = execute_or_request_party_action(
        &state,
        actor_id,
        "reject_join",
        &format!("Reject join request {request_id}"),
        "reject_party_join_request",
        vec![json!(actor_id), json!(request_id)],
    )
    .await;
    Redirect::to(&party_location_url(&state, &id).await)
}

async fn party_location_url(state: &AppState, party_id: &str) -> String {
    let parties: Vec<Party> = state
        .db
        .query(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
        .await
        .unwrap_or_default();
    let Some(party) = parties.first() else {
        return "/".to_string();
    };
    match &party.current_settlement_id {
        Some(settlement) => format!("/settlements/{settlement}"),
        None => "/".to_string(),
    }
}

#[derive(Serialize)]
struct PartyNotifications {
    pending_join_requests: usize,
    role_join_requests: Vec<RoleNotification>,
    action_requests: Vec<PartyActionRequest>,
    succession_required: bool,
    leader_votes: Vec<PartyLeaderVote>,
}

#[derive(Serialize)]
struct RoleNotification {
    role_id: u64,
    count: usize,
}

async fn party_notifications(
    State(state): State<AppState>,
    session: Session,
) -> Json<PartyNotifications> {
    let Some(character_id) = session.character_id_u64() else {
        return Json(PartyNotifications {
            pending_join_requests: 0,
            role_join_requests: Vec::new(),
            action_requests: Vec::new(),
            succession_required: false,
            leader_votes: Vec::new(),
        });
    };
    let Some(character) = get_character(&state, character_id).await else {
        return Json(PartyNotifications {
            pending_join_requests: 0,
            role_join_requests: Vec::new(),
            action_requests: Vec::new(),
            succession_required: false,
            leader_votes: Vec::new(),
        });
    };
    let Some(party_id) = character.party_id else {
        return Json(PartyNotifications {
            pending_join_requests: 0,
            role_join_requests: Vec::new(),
            action_requests: Vec::new(),
            succession_required: false,
            leader_votes: Vec::new(),
        });
    };
    let parties: Vec<Party> = state
        .db
        .query(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
        .await
        .unwrap_or_default();
    let is_leader = parties
        .first()
        .is_some_and(|party| party.leader_id == character_id);
    let requests: Vec<PartyJoinRequest> = state
        .db
        .query(&format!(
            "SELECT * FROM party_join_request WHERE party_id = '{}'",
            party_id
        ))
        .await
        .unwrap_or_default();
    let mut counts = std::collections::BTreeMap::new();
    for request in &requests {
        *counts.entry(request.recruitment_role_id).or_insert(0) += 1;
    }
    let action_requests = if is_leader {
        state
            .db
            .query::<PartyActionRequest>(&format!(
                "SELECT * FROM party_action_request WHERE party_id = '{}'",
                party_id
            ))
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let actual_leader_alive = match parties.first() {
        Some(party) => get_character(&state, party.leader_id)
            .await
            .is_some_and(|leader| leader.alive),
        None => true,
    };
    let leader_votes = state
        .db
        .query::<PartyLeaderVote>(&format!(
            "SELECT * FROM party_leader_vote WHERE party_id = '{}'",
            party_id
        ))
        .await
        .unwrap_or_default();
    Json(PartyNotifications {
        pending_join_requests: if is_leader { requests.len() } else { 0 },
        role_join_requests: counts
            .into_iter()
            .map(|(role_id, count)| RoleNotification { role_id, count })
            .collect(),
        action_requests,
        succession_required: !actual_leader_alive,
        leader_votes,
    })
}

async fn approve_action_request(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    session: Session,
) -> Redirect {
    let Some(leader_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    if let Some(request) = state
        .db
        .query::<PartyActionRequest>(&format!(
            "SELECT * FROM party_action_request WHERE id = {id}"
        ))
        .await
        .unwrap_or_default()
        .into_iter()
        .next()
    {
        let _ = approve_party_action(&state, leader_id, &request).await;
    }
    Redirect::to("/")
}

async fn deny_action_request(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    session: Session,
) -> Redirect {
    if let Some(leader_id) = session.character_id_u64() {
        let _ = state
            .db
            .call(
                "dismiss_party_action_request",
                &[json!(leader_id), json!(id)],
            )
            .await;
    }
    Redirect::to("/")
}

async fn vote_for_leader(
    State(state): State<AppState>,
    Path(candidate_id): Path<u64>,
    session: Session,
) -> Redirect {
    if let Some(voter_id) = session.character_id_u64() {
        let _ = state
            .db
            .call(
                "vote_for_party_leader",
                &[json!(voter_id), json!(candidate_id)],
            )
            .await;
    }
    Redirect::to("/")
}

async fn leave_party(
    State(state): State<AppState>,
    Path(_id): Path<String>,
    session: Session,
) -> Redirect {
    let Some(character_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };

    let _ = state.db.call("leave_party", &[json!(character_id)]).await;

    Redirect::to("/")
}

async fn disband_party(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Redirect {
    if let Some(actor_id) = session.character_id_u64() {
        let _ = execute_or_request_party_action(
            &state,
            actor_id,
            "disband_party",
            "Disband the party",
            "disband_party",
            vec![json!(actor_id), json!(id)],
        )
        .await;
    }

    Redirect::to("/")
}

async fn get_character(state: &AppState, character_id: u64) -> Option<Character> {
    let characters: Vec<Character> = state
        .db
        .query(&format!(
            "SELECT * FROM character WHERE id = {character_id}"
        ))
        .await
        .unwrap_or_default();
    characters.into_iter().next()
}
