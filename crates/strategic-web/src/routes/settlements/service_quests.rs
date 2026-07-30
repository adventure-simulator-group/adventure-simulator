pub(crate) fn living_party_members(members: &[Character]) -> Vec<Character> {
    members
        .iter()
        .filter(|member| member.alive)
        .cloned()
        .collect()
}

#[derive(Serialize)]
pub(super) struct ServiceQuestOffer {
    id: String,
    title: String,
    description: String,
    service_id: String,
    npc_name: &'static str,
    greeting: String,
    problem: String,
    follow_up: String,
    details: String,
    acceptance: &'static str,
    state: &'static str,
    waiting: &'static str,
    turn_in_response: String,
    can_accept: bool,
    can_turn_in: bool,
}

#[derive(Serialize)]
pub(super) struct ServiceActivityResponse {
    quests: Vec<ServiceQuestOffer>,
    recruitment: Vec<ServiceQuestRecruitment>,
}

#[derive(Serialize)]
pub(super) struct ApprenticeshipResult {
    enrolled: bool,
    message: &'static str,
}

#[derive(Deserialize)]
struct ApprenticeshipRepresentativeRow {
    id: String,
    home_settlement_id: String,
    organization_id: String,
    conversation_id: String,
}

#[derive(Deserialize)]
struct ApprenticeshipRepresentativePresenceRow {
    npc_id: String,
    settlement_id: String,
    location_id: String,
}

fn exact_apprenticeship_representative_present(
    representative: Option<&ApprenticeshipRepresentativeRow>,
    presences: &[ApprenticeshipRepresentativePresenceRow],
    expected_id: &str,
    settlement_id: &str,
    organization_id: &str,
    effective_location_id: &str,
) -> bool {
    representative.is_some_and(|representative| {
        adventuresim_core::organization::exact_representative_fields_match(
            &representative.id,
            expected_id,
            &representative.home_settlement_id,
            settlement_id,
            &representative.organization_id,
            organization_id,
            &representative.conversation_id,
        ) && presences.iter().any(|presence| {
            presence.npc_id == representative.id
                && presence.settlement_id == settlement_id
                && presence.location_id == effective_location_id
        })
    })
}

pub(super) async fn begin_service_apprenticeship(
    State(state): State<AppState>,
    Path((id, service_id)): Path<(String, String)>,
    session: Session,
) -> Json<ApprenticeshipResult> {
    // This is deliberately narrower than organization business dialogue:
    // a local trainer's service resolves one catalog-linked apprenticeship.
    // The route cannot select arbitrary organizations or pay dues, promote,
    // or change presentation.
    let Some(organization) = adventuresim_core::organization::organizations_for_chapter(&id)
        .find(|organization| organization.service_id.as_deref() == Some(service_id.as_str()))
    else {
        return Json(ApprenticeshipResult {
            enrolled: false,
            message: "No local organization offers that professional activity.",
        });
    };
    let settlement = state
        .db
        .query_one::<Settlement>(&format!(
            "SELECT * FROM settlement WHERE id = {}",
            sql_string_literal(&id)
        ))
        .await
        .ok()
        .flatten();
    let Some(settlement) = settlement else {
        return Json(ApprenticeshipResult {
            enrolled: false,
            message: "The local training authority is unavailable.",
        });
    };
    let chapter = organization
        .chapter(&id)
        .expect("chapter iterator guarantees a local chapter");
    let effective_location_id =
        adventuresim_core::organization::chapter_effective_location_id(
            organization,
            chapter,
            &settlement.economy,
        );
    let representative_id = adventuresim_core::organization::organization_representative_id(
        &id,
        &organization.id,
    );
    let representative = match state
        .db
        .query_one::<ApprenticeshipRepresentativeRow>(&format!(
            "SELECT * FROM backend_settlement_npcs WHERE id = {}",
            sql_string_literal(&representative_id)
        ))
        .await
    {
        Ok(representative) => representative,
        Err(error) => {
            tracing::warn!(%error, %representative_id, "failed to verify apprenticeship representative");
            return Json(ApprenticeshipResult {
                enrolled: false,
                message: "The local training authority is unavailable.",
            });
        }
    };
    let presences = match state
        .db
        .query::<ApprenticeshipRepresentativePresenceRow>(&format!(
            "SELECT * FROM settlement_npc_presence WHERE npc_id = {}",
            sql_string_literal(&representative_id)
        ))
        .await
    {
        Ok(presences) => presences,
        Err(error) => {
            tracing::warn!(%error, %representative_id, "failed to verify apprenticeship representative presence");
            return Json(ApprenticeshipResult {
                enrolled: false,
                message: "The local training authority is unavailable.",
            });
        }
    };
    if exact_apprenticeship_representative_present(
        representative.as_ref(),
        &presences,
        &representative_id,
        &id,
        &organization.id,
        effective_location_id,
    ) {
        return Json(ApprenticeshipResult {
            enrolled: false,
            message: "Speak to the local organization representative about apprenticeship.",
        });
    }
    let Some((character, _)) = get_active_character(&state, session.character_id_u64()).await
    else {
        return Json(ApprenticeshipResult {
            enrolled: false,
            message: "Choose a character before asking to train.",
        });
    };
    if character.current_settlement_id.as_deref() != Some(id.as_str()) {
        return Json(ApprenticeshipResult {
            enrolled: false,
            message: "You must speak to the guild member in person.",
        });
    }
    match state
        .db
        .call(
            "join_organization",
            &[json!(character.id), json!(organization.id)],
        )
        .await
    {
        Ok(()) => Json(ApprenticeshipResult {
            enrolled: true,
            message: "Your membership begins today.",
        }),
        Err(error) => {
            tracing::warn!(%error, character_id = character.id, %service_id, "failed to begin apprenticeship");
            Json(ApprenticeshipResult {
                enrolled: false,
                message: "I cannot take you on just now.",
            })
        }
    }
}

#[cfg(test)]
mod apprenticeship_representative_tests {
    use super::*;

    fn representative(
        id: &str,
        settlement_id: &str,
        organization_id: &str,
    ) -> ApprenticeshipRepresentativeRow {
        ApprenticeshipRepresentativeRow {
            id: id.into(),
            home_settlement_id: settlement_id.into(),
            organization_id: organization_id.into(),
            conversation_id: "organization-representative".into(),
        }
    }

    fn presence(
        id: &str,
        settlement_id: &str,
        location_id: &str,
    ) -> ApprenticeshipRepresentativePresenceRow {
        ApprenticeshipRepresentativePresenceRow {
            npc_id: id.into(),
            settlement_id: settlement_id.into(),
            location_id: location_id.into(),
        }
    }

    #[test]
    fn standalone_exact_representative_blocks_direct_apprenticeship() {
        let id = adventuresim_core::organization::organization_representative_id(
            "viabundus-0",
            "physicians_college",
        );
        let representative = representative(&id, "viabundus-0", "physicians_college");
        let presences = [presence(
            &id,
            "viabundus-0",
            "organization-physicians-college",
        )];
        assert!(exact_apprenticeship_representative_present(
            Some(&representative),
            &presences,
            &id,
            "viabundus-0",
            "physicians_college",
            "organization-physicians-college",
        ));
    }

    #[test]
    fn missing_or_wrong_presence_keeps_the_direct_fallback_available() {
        let id = adventuresim_core::organization::organization_representative_id(
            "viabundus-0",
            "merchant_guild",
        );
        let representative = representative(&id, "viabundus-0", "merchant_guild");
        assert!(!exact_apprenticeship_representative_present(
            Some(&representative),
            &[],
            &id,
            "viabundus-0",
            "merchant_guild",
            "market",
        ));
        assert!(!exact_apprenticeship_representative_present(
            Some(&representative),
            &[presence(&id, "viabundus-0", "forge")],
            &id,
            "viabundus-0",
            "merchant_guild",
            "market",
        ));
    }
}

pub(super) async fn update_organization_presentation(
    State(state): State<AppState>,
    Path((id, character_id, organization_id)): Path<(String, u64, String)>,
    session: Session,
) -> Response {
    if session.character_id_u64() != Some(character_id) {
        return (StatusCode::FORBIDDEN, "Select this character first").into_response();
    }
    match state
        .db
        .call(
            "present_organization",
            &[json!(character_id), json!(organization_id)],
        )
        .await
    {
        Ok(()) => Redirect::to(&format!("/locations/settlement/{id}/party/{character_id}"))
            .into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

pub(super) async fn clear_presented_organization(
    State(state): State<AppState>,
    Path((id, character_id)): Path<(String, u64)>,
    session: Session,
) -> Response {
    if session.character_id_u64() != Some(character_id) {
        return (StatusCode::FORBIDDEN, "Select this character first").into_response();
    }
    match state
        .db
        .call("clear_organization_presentation", &[json!(character_id)])
        .await
    {
        Ok(()) => Redirect::to(&format!("/locations/settlement/{id}/party/{character_id}"))
            .into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    }
}

#[derive(Serialize)]
pub(super) struct ServiceQuestRecruitment {
    offer_id: String,
    service_id: &'static str,
    location_id: String,
    party_name: String,
    leader_id: String,
    leader_name: String,
    roles: Vec<ServiceQuestRole>,
}

#[derive(Serialize)]
pub(super) struct ServiceQuestRole {
    id: u64,
    name: String,
    remaining: u32,
    requirements: Vec<String>,
    requirements_summary: String,
    match_level: &'static str,
    match_summary: String,
    left_html: String,
    right_html: String,
}

pub(super) async fn service_quest_offers(
    State(state): State<AppState>,
    Path(id): Path<String>,
    session: Session,
) -> Json<ServiceActivityResponse> {
    let settlements: Vec<Settlement> = state
        .db
        .query("SELECT * FROM settlement")
        .await
        .unwrap_or_default();
    let Some(settlement) = settlements.iter().find(|settlement| settlement.id == id) else {
        return Json(ServiceActivityResponse {
            quests: Vec::new(),
            recruitment: Vec::new(),
        });
    };
    let quests: Vec<ContractPresentation> = state
        .db
        .query(&format!(
            "SELECT * FROM backend_contracts WHERE settlement_id = {}",
            sql_string_literal(&id)
        ))
        .await
        .unwrap_or_default();
    let edges: Vec<TravelEdge> = state
        .db
        .query("SELECT * FROM travel_edge")
        .await
        .unwrap_or_default();
    let neighboring_name = connected_destinations(settlement, &settlements, &edges)
        .first()
        .map(|destination| destination.name.clone())
        .unwrap_or_else(|| "the next settlement".to_string());
    let active_character = get_active_character(&state, session.character_id_u64()).await;
    let active_party = if let Some(party_id) = active_character
        .as_ref()
        .and_then(|(character, _)| character.party_id.as_ref())
    {
        state
            .db
            .query::<Party>(&format!(
                "SELECT * FROM party WHERE id = {}",
                sql_string_literal(party_id)
            ))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
    } else {
        None
    };
    let can_accept = active_character.as_ref().is_some_and(|(character, _)| {
        character.current_settlement_id.as_deref() == Some(id.as_str())
            && active_party
                .as_ref()
                .is_some_and(|party| party.active_contract_id.is_none())
    });
    let can_turn_in = active_character.as_ref().is_some_and(|(character, _)| {
        character.current_settlement_id.as_deref() == Some(id.as_str()) && active_party.is_some()
    });
    let parties: Vec<Party> = state
        .db
        .query("SELECT * FROM party")
        .await
        .unwrap_or_default();
    let party_memberships: Vec<PartyMember> = state
        .db
        .query("SELECT * FROM party_member")
        .await
        .unwrap_or_default();
    let recruitment_roles: Vec<PartyRecruitmentRole> = state
        .db
        .query("SELECT * FROM party_recruitment_role")
        .await
        .unwrap_or_default();
    let recruitment_offers: Vec<RecruitmentOffer> = state
        .db
        .query(&format!(
            "SELECT * FROM recruitment_offer WHERE settlement_id = {}",
            sql_string_literal(&id)
        ))
        .await
        .unwrap_or_default();
    let characters: Vec<Character> = state
        .db
        .query("SELECT * FROM character")
        .await
        .unwrap_or_default();
    let viewer_party_id = active_party.as_ref().map(|party| party.id.as_str());
    let viewer_member_ids: Vec<u64> = viewer_party_id
        .map(|party_id| {
            party_memberships
                .iter()
                .filter(|member| member.party_id == party_id)
                .map(|member| member.character_id)
                .collect()
        })
        .unwrap_or_default();
    let mut viewer_capabilities = Vec::new();
    for character_id in viewer_member_ids {
        let _ = state
            .db
            .call("refresh_capabilities", &[json!(character_id)])
            .await;
        if let Some(capability) = state
            .db
            .query::<CharacterCapability>(&format!(
                "SELECT * FROM character_capability WHERE character_id = {character_id}"
            ))
            .await
            .unwrap_or_default()
            .into_iter()
            .next()
        {
            viewer_capabilities.push(capability);
        }
    }

    let recruiting_companies = recruitment_offers
        .iter()
        .filter(|offer| {
            offer.status == RecruitmentOfferStatus::Open
                && viewer_party_id != Some(offer.recruiting_party_id.as_str())
        })
        .filter_map(|offer| {
            let party = parties
                .iter()
                .find(|party| party.id == offer.recruiting_party_id)?;
            let leader = characters
                .iter()
                .find(|character| character.id == offer.leader_id)?;
            if party.current_settlement_id.as_deref() != Some(id.as_str())
                || party.leader_id != leader.id
            {
                return None;
            }
            let roles = recruitment_roles
                .iter()
                .filter(|role| role.party_id == party.id)
                .filter_map(|role| {
                    let filled = party_memberships
                        .iter()
                        .filter(|member| member.recruitment_role_id == Some(role.id))
                        .count() as u32;
                    let remaining = role.quantity.saturating_sub(filled);
                    if remaining == 0 {
                        return None;
                    }
                    let requirements = role_requirement_labels(role);
                    let (match_level, match_summary) = party_role_match(&viewer_capabilities, role);
                    let (left_html, right_html) =
                        crate::templates::recruitment::service_role_inspection(
                            &role.name,
                            &requirements,
                            &party.name,
                            &leader.name,
                            remaining,
                            match_level,
                            &match_summary,
                            &format!("/party-roles/{}/join", role.id),
                            can_accept,
                        );
                    Some(ServiceQuestRole {
                        id: role.id,
                        name: role.name.clone(),
                        remaining,
                        requirements_summary: if requirements.is_empty() {
                            "No minimum recommendations".to_string()
                        } else {
                            requirements.join(" Â· ")
                        },
                        requirements,
                        match_level,
                        match_summary,
                        left_html,
                        right_html,
                    })
                })
                .collect::<Vec<_>>();
            (!roles.is_empty()).then(|| ServiceQuestRecruitment {
                offer_id: offer.id_key.clone(),
                service_id: "inn",
                location_id: offer.location_id.clone(),
                party_name: party.name.clone(),
                leader_id: leader.id.to_string(),
                leader_name: leader.name.clone(),
                roles,
            })
        })
        .collect();
    let quest_offers = quests
            .iter()
            .filter_map(|quest| {
                let is_current = active_party.as_ref().is_some_and(|party| {
                    party.active_contract_id.as_deref() == Some(quest.id.as_str())
                        && quest.accepted_by.as_deref() == Some(party.id.as_str())
                });
                let state = if quest.status == ContractPresentationStatus::Offered {
                    "available"
                } else if is_current
                    && quest.status == ContractPresentationStatus::ReadyToReport
                {
                    "ready"
                } else if is_current {
                    "underway"
                } else {
                    return None;
                };
                let problem = quest.description.trim_end_matches('.').to_lowercase();
                let (npc_name, greeting) = service_quest_greeting(&quest.service_id);
                Some(ServiceQuestOffer {
                    id: quest.id.clone(),
                    title: quest.title.clone(),
                    description: active_contract_tooltip(quest),
                    service_id: quest.service_id.clone(),
                    npc_name,
                    greeting: greeting.to_string(),
                    follow_up: format!("{problem}?"),
                    problem,
                    details: service_quest_details(
                        &quest.service_id,
                        quest,
                        &settlement.name,
                        &neighboring_name,
                    ),
                    acceptance: "Splendid! And please, do be careful! You wouldn't be the first men they've slain.",
                    state,
                    waiting: "Hello again, I eagerly await the results of your efforts.",
                    turn_in_response: format!(
                        "Excellent work. Here is the promised {} coin. You've earned it.",
                        quest.gold_reward
                    ),
                    can_accept,
                    can_turn_in: can_turn_in && state == "ready",
                })
            })
            .collect();
    Json(ServiceActivityResponse {
        quests: quest_offers,
        recruitment: recruiting_companies,
    })
}

pub(super) fn service_quest_greeting(service_id: &str) -> (&'static str, &'static str) {
    match service_id {
        "weapons" => (
            "Weaponsmith",
            "Welcome. Business would be better, were it not for how",
        ),
        "armor" => ("Armourer", "Welcome. Production has nearly stopped because"),
        "clothing" => (
            "Clothier",
            "Welcome, traveler. Cloth is scarce of late because",
        ),
        "herbalist" => (
            "Herbalist",
            "Welcome. My stores and preparation work have suffered because",
        ),
        "physician" => (
            "Physician",
            "Welcome. Patient care has been made more difficult because",
        ),
        "surgeon" => (
            "Surgeon",
            "Welcome. The guild has urgent need of assistance because",
        ),
        "inn" => (
            "Innkeeper",
            "Welcome. Travelers have been avoiding this road because",
        ),
        "religion" => (
            "Priest",
            "God give you peace. I must ask your aid concerning",
        ),
        _ => (
            "Merchant",
            "Welcome, traveler. You'll have to excuse the sorry state of my inventory;",
        ),
    }
}

pub(super) fn service_quest_details(
    _service_id: &str,
    quest: &ContractPresentation,
    _settlement_name: &str,
    _neighboring_name: &str,
) -> String {
    // The generated quest is authoritative. Service identifies the speaker,
    // never the threat or location; several templates intentionally share it.
    let situation = &quest.description;
    format!(
        "Yes, {situation}. I believe it involves {} {}, but that account may be wrong. I'd offer {} coin for a verified resolution. Learn more before committing to a fight. Are you",
        quest.opposition_count_wording, quest.opposition_wording, quest.gold_reward,
    )
}

#[cfg(test)]
mod bestiary_quest_presentation_tests {
    use super::*;

    fn quest(opposition_wording: &str, description: &str) -> ContractPresentation {
        ContractPresentation {
            id: "q".into(),
            case_id: "case:q".into(),
            title: "Problem".into(),
            description: description.into(),
            difficulty: 2,
            gold_reward: 40,
            xp_reward: 20,
            settlement_id: "s".into(),
            service_id: "inn".into(),
            issuer_npc_id: String::new(),
            status: ContractPresentationStatus::Offered,
            accepted_by: None,
            opposition_wording: opposition_wording.into(),
            opposition_count_wording: "perhaps several".into(),
        }
    }

    #[test]
    fn shared_service_never_substitutes_its_old_fixed_threat_or_location() {
        let alp = quest("alp", "Sleepers report an unseen visitor.");
        let hound = quest("spectral_hound", "A black hound haunts the road.");
        let alp_details = service_quest_details("inn", &alp, "A", "B");
        let hound_details = service_quest_details("inn", &hound, "A", "B");
        assert!(alp_details.contains("unseen visitor"));
        assert!(hound_details.contains("black hound"));
        assert!(!alp_details.contains("goblin") && !hound_details.contains("goblin"));
    }
}

pub(super) fn role_requirement_labels(role: &PartyRecruitmentRole) -> Vec<String> {
    let requirements = role.requirements;
    let mut labels = Vec::new();
    for (required, label) in [
        (requirements.melee, "Melee"),
        (requirements.ranged, "Ranged"),
        (requirements.heavy, "Heavy"),
        (requirements.quarter_armor, "1/4 armor"),
        (requirements.half_armor, "1/2 armor"),
        (requirements.three_quarter_armor, "3/4 armor"),
        (requirements.full_armor, "Full armor"),
    ] {
        if required {
            labels.push(label.to_string());
        }
    }
    let precision = role.effective_weapon_precision();
    if precision > 0.0 {
        labels.push(format!("Weapon precision {precision:.1}+"));
    }
    for (minimum, label) in [
        (requirements.athletics, "Athletics"),
        (requirements.endurance, "Endurance"),
    ] {
        if minimum > 0 {
            labels.push(format!("{label} {minimum}+"));
        }
    }
    labels
}

pub(super) fn party_role_match(
    capabilities: &[CharacterCapability],
    role: &PartyRecruitmentRole,
) -> (&'static str, String) {
    let total = role_requirement_labels(role).len();
    if total == 0 {
        return (
            "none",
            "This role has no minimum recommendations.".to_string(),
        );
    }
    let best = capabilities
        .iter()
        .map(|capability| matched_role_requirements(capability, role))
        .max()
        .unwrap_or(0);
    if best == total {
        (
            "all",
            "Someone in your party meets every recommendation.".to_string(),
        )
    } else if best > 0 {
        (
            "some",
            format!("Your best candidate meets {best} of {total} recommendations."),
        )
    } else {
        (
            "none-met",
            "No one in your party meets any recommendation.".to_string(),
        )
    }
}

pub(super) fn matched_role_requirements(
    capability: &CharacterCapability,
    role: &PartyRecruitmentRole,
) -> usize {
    let requirements: RecruitmentRequirements = role.requirements;
    let mut matched = 0;
    for (required, present) in [
        (requirements.melee, capability.melee),
        (requirements.ranged, capability.ranged),
        (requirements.heavy, capability.heavy),
        (requirements.quarter_armor, capability.quarter_armor),
        (requirements.half_armor, capability.half_armor),
        (
            requirements.three_quarter_armor,
            capability.three_quarter_armor,
        ),
        (requirements.full_armor, capability.full_armor),
    ] {
        if required && present {
            matched += 1;
        }
    }
    if role.effective_weapon_precision() > 0.0
        && capability.weapon_precision >= role.effective_weapon_precision()
    {
        matched += 1;
    }
    for (minimum, value) in [
        (requirements.athletics, capability.athletics),
        (requirements.endurance, capability.endurance),
    ] {
        if minimum > 0 && adventuresim_core::capability::rating(value) >= minimum {
            matched += 1;
        }
    }
    matched
}
