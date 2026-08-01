#[derive(Deserialize)]
pub(super) struct CookFoodForm {
    method: String,
    inventory_item_ids: String,
    amounts_milliunits: String,
}

pub(super) async fn cook_food(
    State(state): State<AppState>,
    Path((kind, id, character_id)): Path<(String, String, u64)>,
    Query(building): Query<BuildingQuery>,
    session: Session,
    Form(form): Form<CookFoodForm>,
) -> Response {
    if session.character_id_u64() != Some(character_id) {
        return (
            StatusCode::FORBIDDEN,
            "Only the selected character can cook",
        )
            .into_response();
    }
    let parse = |value: &str| -> Result<Vec<u64>, _> {
        value
            .split(',')
            .filter(|value| !value.is_empty())
            .map(str::parse)
            .collect()
    };
    let ids = match parse(&form.inventory_item_ids) {
        Ok(value) => value,
        Err(_) => return (StatusCode::BAD_REQUEST, "Invalid ingredient selection").into_response(),
    };
    let amounts_milliunits = match form
        .amounts_milliunits
        .split(',')
        .filter(|value| !value.is_empty())
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(value) => value,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "Invalid ingredient amounts").into_response();
        }
    };
    let method = match form.method.as_str() {
        "pan-fry" => json!({ "panFry": {} }),
        "stew" => json!({ "stew": {} }),
        "roast" => json!({ "roast": {} }),
        "bake" => json!({ "bake": {} }),
        _ => return (StatusCode::BAD_REQUEST, "Invalid cooking method").into_response(),
    };
    if let Err(error) = state
        .db
        .call(
            "cook_food",
            &[
                json!(character_id),
                method,
                json!(ids),
                json!(amounts_milliunits),
            ],
        )
        .await
    {
        tracing::warn!(%error, character_id, "cooking failed");
        return (StatusCode::BAD_REQUEST, error.to_string()).into_response();
    }
    Redirect::to(&building.append_to(&state, &kind, &id, format!(
        "/locations/{kind}/{id}/party/{character_id}?cook=true"
    )).await)
    .into_response()
}

pub(super) async fn party_religion_knowledge_check(
    state: &AppState,
    party_members: &[Character],
    religion_id: &str,
) -> f32 {
    let Some(religion) = OfficialReligion::from_id(religion_id) else {
        return 0.0;
    };
    let mut checks = Vec::with_capacity(party_members.len());
    for member in living_party_member_refs(party_members) {
        let skills = query_single::<CharacterSkills>(state, "backend_character_skills", member.id).await;
        let attributes =
            query_single::<CharacterAttributes>(state, "backend_character_attributes", member.id).await;
        let limbs = query_single::<CharacterLimbs>(state, "backend_character_limbs", member.id).await;
        let stats = query_single::<CharacterStats>(state, "backend_character_stats", member.id).await;
        if let (Some(skills), Some(attributes), Some(limbs), Some(stats)) =
            (skills, attributes, limbs, stats)
        {
            checks.push(adventuresim_core::capability::religion_knowledge_check(
                skills.religion_hours.effective(religion),
                attributes.instinct,
                attributes.intelligence,
                stats.focus,
                limbs.head_health,
            ));
        }
    }
    adventuresim_core::capability::aggregate_party_check(checks).clamp(0.0, 5.0)
}

pub(super) fn living_party_member_refs(party_members: &[Character]) -> impl Iterator<Item = &Character> {
    party_members.iter().filter(|member| member.alive)
}

#[cfg(test)]
mod party_religion_knowledge_tests {
    use super::living_party_member_refs;
    use crate::spacetimedb::Character;

    fn party_member(id: u64, alive: bool) -> Character {
        Character {
            id,
            name: format!("Member {id}"),
            xp: 0,
            level: 1,
            gold: 0,
            current_settlement_id: None,
            current_case_site_id: None,
            party_id: Some("party".into()),
            age_years: 20,
            alive,
            temporary: false,
            social_notification_count: 0,
            automatic_social_chat_enabled: false,
        }
    }

    #[test]
    fn prayer_preview_knowledge_excludes_dead_party_members() {
        let members = [party_member(1, true), party_member(2, false)];
        let ids = living_party_member_refs(&members)
            .map(|member| member.id)
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![1]);
    }
}
