//! Party route handlers

use axum::{
    Form, Router,
    extract::{Path, State},
    response::{Html, Json, Redirect},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::AppState;
use crate::session::Session;
use crate::spacetimedb::{
    Character, CharacterCapability, Party, PartyJoinRequest, PartyMember, PartyRecruitmentRole,
    Quest, RecruitmentRequirements, SavedRecruitmentRole,
};
use crate::templates::party::{parties_list_page, party_detail_page};
use crate::templates::recruitment::{RecruitmentRolePanel, recruitment_panel};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/parties", get(list_parties))
        .route("/parties/{id}", get(show_party))
        .route("/party-roles/{id}/join", post(join_party))
        .route("/party-recruitment/panel", get(recruitment_panel_fragment))
        .route("/party-recruitment/roles", post(create_recruitment_role))
        .route(
            "/party-recruitment/saved/{id}/delete",
            post(delete_saved_role),
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
        .route("/parties/{id}/leave", post(leave_party))
        .route("/parties/{id}/disband", post(disband_party))
}

async fn list_parties(State(state): State<AppState>, session: Session) -> Html<String> {
    let parties: Vec<Party> = state
        .db
        .query("SELECT * FROM party")
        .await
        .unwrap_or_default();

    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(parties_list_page(&parties, None, logged_in_as.as_deref(), session.theme()).into_string())
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
    precise: bool,
    #[serde(default)]
    heavy: bool,
    #[serde(default)]
    armored: bool,
    #[serde(default)]
    shield: bool,
    #[serde(default)]
    blunt: bool,
    #[serde(default)]
    slash: bool,
    #[serde(default)]
    pierce: bool,
    #[serde(default)]
    climb: u8,
    #[serde(default)]
    swim: u8,
    #[serde(default)]
    endurance: u8,
    #[serde(default)]
    medicine: u8,
    #[serde(default)]
    surgery: u8,
    #[serde(default)]
    charisma: u8,
    #[serde(default)]
    faith: u8,
}

impl RecruitmentRoleForm {
    fn requirements(&self) -> RecruitmentRequirements {
        RecruitmentRequirements {
            melee: self.melee,
            ranged: self.ranged,
            precise: self.precise,
            heavy: self.heavy,
            armored: self.armored,
            shield: self.shield,
            blunt: self.blunt,
            slash: self.slash,
            pierce: self.pierce,
            climb: self.climb,
            swim: self.swim,
            endurance: self.endurance,
            medicine: self.medicine,
            surgery: self.surgery,
            charisma: self.charisma,
            faith: self.faith,
        }
    }
}

async fn create_recruitment_role(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<RecruitmentRoleForm>,
) -> Redirect {
    let Some(leader_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    let requirements = form.requirements();
    if let Err(error) = state
        .db
        .call(
            "create_recruitment_role",
            &[
                json!(leader_id),
                json!(form.name),
                json!(form.quantity),
                json!(requirements),
                json!(form.save_role),
            ],
        )
        .await
    {
        tracing::warn!("Failed to create recruitment role: {error:?}");
        return Redirect::to("/");
    }
    if state.db.is_local() {
        if let Some(character) = get_character(&state, leader_id).await {
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

async fn show_party(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Html<String> {
    let parties: Vec<Party> = state
        .db
        .query(&format!("SELECT * FROM party WHERE id = '{}'", id))
        .await
        .unwrap_or_default();

    let party = match parties.first() {
        Some(p) => p,
        None => return Html("<h1>Party not found</h1>".to_string()),
    };

    // Get party members
    let members: Vec<PartyMember> = state
        .db
        .query(&format!(
            "SELECT * FROM party_member WHERE party_id = '{}'",
            id
        ))
        .await
        .unwrap_or_default();

    // Get character info for each member
    let mut members_with_chars: Vec<(PartyMember, Option<Character>)> = Vec::new();
    for member in members {
        let chars: Vec<Character> = state
            .db
            .query(&format!(
                "SELECT * FROM character WHERE id = {}",
                member.character_id
            ))
            .await
            .unwrap_or_default();
        members_with_chars.push((member, chars.into_iter().next()));
    }

    // Get active quest if any
    let active_quest: Option<Quest> = if let Some(quest_id) = &party.active_quest_id {
        let quests: Vec<Quest> = state
            .db
            .query(&format!("SELECT * FROM quest WHERE id = '{}'", quest_id))
            .await
            .unwrap_or_default();
        quests.into_iter().next()
    } else {
        None
    };

    // Check if current user is the leader
    let is_leader = session.character_id_u64() == Some(party.leader_id);

    let logged_in_as = get_character_name(&state, session.character_id()).await;
    Html(
        party_detail_page(
            party,
            &members_with_chars,
            active_quest.as_ref(),
            is_leader,
            logged_in_as.as_deref(),
            session.theme(),
        )
        .into_string(),
    )
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
                applicants.push((request.clone(), character));
            }
        }
        panels.push(RecruitmentRolePanel {
            role,
            filled,
            requests: applicants,
        });
    }
    Html(recruitment_panel(&party, character_id, &panels, &saved).into_string())
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
        tags: capability.map(capability_tags).unwrap_or_default(),
    })
}

fn capability_tags(c: CharacterCapability) -> Vec<String> {
    let mut tags = Vec::new();
    for (enabled, label) in [
        (c.melee, "Melee"),
        (c.ranged, "Ranged"),
        (c.precise, "Precise"),
        (c.heavy, "Heavy"),
        (c.armored, "Armored"),
        (c.shield, "Shield"),
        (c.blunt, "Blunt"),
        (c.slash, "Slash"),
        (c.pierce, "Pierce"),
    ] {
        if enabled {
            tags.push(label.into());
        }
    }
    for (value, label) in [
        (c.climb, "Climb"),
        (c.swim, "Swim"),
        (c.endurance, "Endurance"),
        (c.medicine, "Medicine"),
        (c.surgery, "Surgery"),
        (c.charisma, "Charisma"),
        (c.faith, "Faith"),
    ] {
        tags.push(format!("{label} {}", value.round().clamp(0.0, 5.0) as u8));
    }
    tags
}

async fn accept_join_request(
    State(state): State<AppState>,
    Path((id, request_id)): Path<(String, u64)>,
    session: Session,
) -> Redirect {
    let Some(leader_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    let _ = state
        .db
        .call(
            "accept_party_join_request",
            &[json!(leader_id), json!(request_id)],
        )
        .await;
    Redirect::to(&party_noticeboard_url(&state, &id).await)
}

async fn reject_join_request(
    State(state): State<AppState>,
    Path((id, request_id)): Path<(String, u64)>,
    session: Session,
) -> Redirect {
    let Some(leader_id) = session.character_id_u64() else {
        return Redirect::to("/characters");
    };
    let _ = state
        .db
        .call(
            "reject_party_join_request",
            &[json!(leader_id), json!(request_id)],
        )
        .await;
    Redirect::to(&party_noticeboard_url(&state, &id).await)
}

async fn party_noticeboard_url(state: &AppState, party_id: &str) -> String {
    let parties: Vec<Party> = state
        .db
        .query(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
        .await
        .unwrap_or_default();
    let Some(party) = parties.first() else {
        return "/parties".to_string();
    };
    match &party.current_settlement_id {
        Some(settlement) => format!("/settlements/{settlement}/noticeboard"),
        None => format!("/parties/{party_id}"),
    }
}

#[derive(Serialize)]
struct PartyNotifications {
    pending_join_requests: usize,
}

async fn party_notifications(
    State(state): State<AppState>,
    session: Session,
) -> Json<PartyNotifications> {
    let Some(character_id) = session.character_id_u64() else {
        return Json(PartyNotifications {
            pending_join_requests: 0,
        });
    };
    let Some(character) = get_character(&state, character_id).await else {
        return Json(PartyNotifications {
            pending_join_requests: 0,
        });
    };
    let Some(party_id) = character.party_id else {
        return Json(PartyNotifications {
            pending_join_requests: 0,
        });
    };
    let parties: Vec<Party> = state
        .db
        .query(&format!("SELECT * FROM party WHERE id = '{}'", party_id))
        .await
        .unwrap_or_default();
    if parties
        .first()
        .is_none_or(|party| party.leader_id != character_id)
    {
        return Json(PartyNotifications {
            pending_join_requests: 0,
        });
    }
    let requests: Vec<PartyJoinRequest> = state
        .db
        .query(&format!(
            "SELECT * FROM party_join_request WHERE party_id = '{}'",
            party_id
        ))
        .await
        .unwrap_or_default();
    Json(PartyNotifications {
        pending_join_requests: requests.len(),
    })
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

    Redirect::to("/parties")
}

async fn disband_party(State(state): State<AppState>, Path(id): Path<String>) -> Redirect {
    let _ = state.db.call("disband_party", &[json!(id)]).await;

    Redirect::to("/parties")
}

/// Helper to get character name for session display
async fn get_character_name(state: &AppState, character_id: Option<&str>) -> Option<String> {
    let Some(id) = character_id else {
        return None;
    };
    let characters: Vec<Character> = state
        .db
        .query(&format!("SELECT * FROM character WHERE id = {}", id))
        .await
        .unwrap_or_default();
    characters.first().map(|c| c.name.clone())
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
