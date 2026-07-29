fn skill_bps(skill: Skill, hours: f32, attributes: &crate::CharacterAttributes) -> u16 {
    (skill.capped_training_rank(hours, attributes) * 2_000.0)
        .round()
        .clamp(0.0, 10_000.0) as u16
}

fn investigation_terrain_skill(terrain: action::Terrain) -> Skill {
    match terrain {
        action::Terrain::Forest => Skill::TerrainForest,
        action::Terrain::Hills | action::Terrain::Underground => Skill::TerrainHills,
        action::Terrain::Settlement | action::Terrain::Ruins => Skill::TerrainUrban,
        action::Terrain::Marsh => Skill::TerrainWetlands,
        action::Terrain::Plains | action::Terrain::Road => Skill::TerrainPlains,
    }
}

#[cfg(test)]
mod terrain_skill_tests {
    use super::*;

    #[test]
    fn marsh_investigation_uses_wetlands() {
        assert_eq!(
            investigation_terrain_skill(action::Terrain::Marsh),
            Skill::TerrainWetlands
        );
        assert_eq!(
            investigation_terrain_skill(action::Terrain::Road),
            Skill::TerrainPlains
        );
    }
}

fn party_action_skills(
    ctx: &ReducerContext,
    party_id: &str,
    actor_id: u64,
    terrain: action::Terrain,
) -> Result<action::SkillContribution, String> {
    let actor = ctx
        .db
        .character_skills()
        .character_id()
        .find(actor_id)
        .ok_or("Character skills not found")?;
    let actor_attributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(actor_id)
        .ok_or("Character attributes not found")?;
    let terrain_skill = investigation_terrain_skill(terrain);
    let terrain_bps = skill_bps(
        terrain_skill,
        actor.effective_skill_hours(terrain_skill),
        &actor_attributes,
    );
    let mut assistance = 0u16;
    for member_id in living_party_member_ids(ctx, party_id) {
        if member_id == actor_id {
            continue;
        }
        if let Some(skills) = ctx.db.character_skills().character_id().find(member_id) {
            let Some(attributes) = ctx.db.character_attributes().character_id().find(member_id)
            else {
                continue;
            };
            let contribution = skill_bps(
                terrain_skill,
                skills.effective_skill_hours(terrain_skill),
                &attributes,
            ) / 4;
            assistance = assistance.saturating_add(contribution).min(2_000);
        }
    }
    Ok(action::SkillContribution {
        terrain_bps,
        awareness_bps: skill_bps(Skill::Insight, actor.insight_hours, &actor_attributes),
        stealth_bps: skill_bps(Skill::Stealth, actor.stealth_hours, &actor_attributes),
        assistance_bps: assistance,
        // No authoritative locality-familiarity source exists yet.
        familiarity_bps: 0,
    })
}

fn actor_action_terrain(ctx: &ReducerContext, actor: &crate::Character) -> action::Terrain {
    if actor.current_settlement_id.is_some() {
        return action::Terrain::Settlement;
    }
    character_case_site_id(ctx, actor.id)
        .and_then(|id| ctx.db.case_site_authority().id_key().find(&id))
        .and_then(|site| parse_action_terrain(&site.scene_key).ok())
        .unwrap_or(action::Terrain::Road)
}

fn actor_action_weather(
    ctx: &ReducerContext,
    actor: &crate::Character,
    started_at: u64,
) -> action::WeatherAuthority {
    let coordinates = actor
        .current_settlement_id
        .as_ref()
        .and_then(|id| ctx.db.settlement().id().find(id))
        .map(|settlement| {
            (
                (settlement.coord_y * 1_000_000.0).round() as i32,
                (settlement.coord_x * 1_000_000.0).round() as i32,
            )
        })
        .or_else(|| {
            character_case_site_id(ctx, actor.id)
                .and_then(|id| ctx.db.case_site_authority().id_key().find(&id))
                .map(|site| (site.latitude_e7 / 10, site.longitude_e7 / 10))
        })
        .unwrap_or((0, 0));
    let weather = adventuresim_core::weather::weather_at(
        adventuresim_core::weather::WORLD_WEATHER_SEED,
        started_at,
        coordinates.0,
        coordinates.1,
        0,
    );
    match weather.precipitation {
        adventuresim_core::weather::Precipitation::Clear => action::WeatherAuthority::Clear {
            snow_cover_bps: weather.snow_cover_bps,
        },
        adventuresim_core::weather::Precipitation::Rain => action::WeatherAuthority::Rain {
            intensity_bps: weather.intensity_bps,
            snow_cover_bps: weather.snow_cover_bps,
        },
        adventuresim_core::weather::Precipitation::Snow => action::WeatherAuthority::Snow {
            intensity_bps: weather.intensity_bps,
            snow_cover_bps: weather.snow_cover_bps,
        },
    }
}

fn persist_action_result_lead(
    ctx: &ReducerContext,
    capability: &InvestigationActionCapability,
    attempt_id: &str,
    resolution: &action::Resolution,
) -> Result<(), String> {
    let public_case_id = generated_authority_reducer(ctx, capability)
        .map_err(|()| "Generated action authority is invalid")?
        .map(|(manifest, _)| {
            serde_json::from_str::<adventuresim_core::quest_generation::GeneratedCase>(&manifest)
                .map(|generated| generated.public_case_id)
                .map_err(|_| "Validated generated manifest became invalid")
        })
        .transpose()?
        .unwrap_or_else(|| capability.case_id.clone());
    let kind = parse_action_kind(&capability.method)?;
    let generated_outputs = ctx
        .db
        .investigation_generated_action_output()
        .capability_id()
        .find(&capability.id)
        .map(|row| {
            serde_json::from_str::<Vec<adventuresim_core::quest_generation::GeneratedActionOutput>>(
                &row.outputs_json,
            )
            .map_err(|_| "Generated action output authority is invalid")
        })
        .transpose()?;
    let typed_destination = generated_outputs.as_ref().and_then(|outputs| {
        outputs.iter().find_map(|output| match output {
            adventuresim_core::quest_generation::GeneratedActionOutput::Destination {
                stage,
                site_id,
            } => Some((*stage, site_id.as_ref())),
            _ => None,
        })
    });
    let exact_site_id = typed_destination.and_then(|(stage, site_id)| {
        (stage == adventuresim_core::quest_generation::GeneratedDestinationStage::Exact)
            .then_some(site_id)
            .flatten()
    });
    let exact = resolution.success
        && if generated_outputs.is_some() {
            exact_site_id.is_some()
        } else {
            capability.target_kind == "site"
                && (kind == action::InvestigationActionKind::InspectSite
                    || resolution.resulting_uncertainty_bps <= 1_500)
        };
    let site = if exact {
        let site_id =
            exact_site_id.map_or_else(|| capability.target_id.clone(), |site_id| site_id.0.clone());
        ctx.db.case_site_authority().id_key().find(&site_id)
    } else {
        None
    };
    let lead_id = generated_observer_id(ctx, &capability.case_id, "lead", attempt_id)
        .unwrap_or_else(|| inv::compound_id(&["lead", "action", attempt_id]));
    if ctx.db.investigation_lead().id().find(&lead_id).is_some() {
        return Ok(());
    }
    let typed_stage = typed_destination.map(|(stage, _)| match stage {
        adventuresim_core::quest_generation::GeneratedDestinationStage::Unknown => "unknown",
        adventuresim_core::quest_generation::GeneratedDestinationStage::Textual => "textual",
        adventuresim_core::quest_generation::GeneratedDestinationStage::Landmark => "landmark",
        adventuresim_core::quest_generation::GeneratedDestinationStage::ApproximateArea => {
            "approximate_area"
        }
        adventuresim_core::quest_generation::GeneratedDestinationStage::RouteSegment => {
            "route_segment"
        }
        adventuresim_core::quest_generation::GeneratedDestinationStage::Exact => "exact_believed",
    });
    let exact_location_label = site
        .as_ref()
        .map(|site| site.name.clone())
        .unwrap_or_default();
    let (stage, exact_location_id, latitude_e7, longitude_e7) = if let Some(site) = site {
        (
            "exact_believed",
            site.id.value,
            site.latitude_e7,
            site.longitude_e7,
        )
    } else if resolution.success {
        (
            typed_stage.unwrap_or("approximate_area"),
            String::new(),
            0,
            0,
        )
    } else {
        ("unknown", String::new(), 0, 0)
    };
    ctx.db.investigation_lead().insert(InvestigationLead {
        id: lead_id,
        owner_character_id: capability.owner_character_id,
        case_id: public_case_id,
        proposition_id: String::new(),
        summary: if resolution.success {
            capability.safe_result_on_success.clone()
        } else {
            "The attempt found nothing conclusive; the lead remains open through another approach."
                .into()
        },
        source_label: "your party's investigation".into(),
        confidence_bps: if resolution.success { 8_000 } else { 3_000 },
        destination_stage: stage.into(),
        directions: if exact {
            String::new()
        } else {
            capability.safe_summary.clone()
        },
        exact_location_id,
        latitude_e7,
        longitude_e7,
        witness_name: String::new(),
        witness_description: String::new(),
        witness_occupation_or_relationship: String::new(),
        expected_location: String::new(),
        current_learned_location: exact_location_label,
        contradiction_group: format!("action-location:{}", capability.case_id),
        corrected_by: String::new(),
        recorded_at: official_minute(ctx),
    });
    if resolution.success
        && let Some(outputs) = generated_outputs
    {
        for evidence_id in outputs.iter().filter_map(|output| match output {
            adventuresim_core::quest_generation::GeneratedActionOutput::Evidence {
                evidence_id,
            } => Some(&evidence_id.0),
            _ => None,
        }) {
            record_evidence_knowledge(
                ctx,
                capability.owner_character_id,
                &capability.case_id,
                evidence_id,
                attempt_id,
            )?;
        }
    }
    Ok(())
}

const INVALID_INVESTIGATION_ROUTE_ERROR: &str =
    "Investigation track origin no longer matches the projected route";

fn validate_tracking_action_origin(
    ctx: &ReducerContext,
    actor: &crate::Character,
    capability: &InvestigationActionCapability,
    kind: action::InvestigationActionKind,
) -> Result<(), String> {
    if !tracking_capability_chain_is_coherent(
        capability,
        kind,
        |id| {
            ctx.db
                .investigation_action_capability()
                .id()
                .find(&id.to_owned())
        },
        |id| {
            ctx.db
                .investigation_action_attempt()
                .capability_id()
                .filter(id)
                .any(|attempt| attempt.success)
        },
    ) {
        return Err(INVALID_INVESTIGATION_ROUTE_ERROR.into());
    }
    let predecessor = ctx
        .db
        .investigation_action_capability()
        .id()
        .find(&capability.required_action_id)
        .ok_or(INVALID_INVESTIGATION_ROUTE_ERROR)?;
    validate_action_position(
        ctx,
        actor,
        &predecessor,
        parse_action_kind(&predecessor.method)?,
    )
}

fn validate_action_position(
    ctx: &ReducerContext,
    actor: &crate::Character,
    capability: &InvestigationActionCapability,
    kind: action::InvestigationActionKind,
) -> Result<(), String> {
    match capability.target_kind.as_str() {
        "contact" => {
            let presence = ctx
                .db
                .settlement_npc_presence()
                .npc_id()
                .find(&capability.target_id)
                .ok_or("Referred contact no longer has an authoritative presence")?;
            if actor.current_settlement_id.as_deref() != Some(presence.settlement_id.as_str()) {
                return Err("The referred contact is in another settlement".into());
            }
            if kind == action::InvestigationActionKind::LocateContact {
                let minute = character_strategic_minute(ctx, actor.id) % 1_440;
                let present = if presence.start_minute <= presence.end_minute {
                    minute >= u64::from(presence.start_minute)
                        && minute < u64::from(presence.end_minute)
                } else {
                    minute >= u64::from(presence.start_minute)
                        || minute < u64::from(presence.end_minute)
                };
                if !present {
                    return Err("The referred contact is not currently present".into());
                }
            }
            Ok(())
        }
        "cohort" => {
            let target = ctx
                .db
                .investigation_pattern_target_authority()
                .cohort_id()
                .find(&capability.target_id)
                .ok_or("Victim cohort authority no longer exists")?;
            if target.case_id != capability.case_id {
                return Err("Victim cohort belongs to another case".into());
            }
            let presence = ctx
                .db
                .settlement_npc_presence()
                .npc_id()
                .find(&target.npc_id)
                .ok_or("Victim cohort target is unavailable")?;
            if actor.current_settlement_id.as_deref() != Some(presence.settlement_id.as_str())
                || presence.settlement_id != target.expected_settlement_id
                || presence.location_id != target.expected_location
                || presence.settlement_id != target.expected_settlement_id
            {
                return Err("Victim cohort target moved from the learned location".into());
            }
            Ok(())
        }
        "area" => {
            let area = ctx
                .db
                .investigation_area_authority()
                .id()
                .find(&capability.target_id)
                .ok_or("Investigation area no longer exists")?;
            let in_origin =
                actor.current_settlement_id.as_deref() == Some(&area.origin_settlement_id);
            let at_case_site = character_case_site_id(ctx, actor.id)
                .and_then(|id| ctx.db.case_site_authority().id_key().find(&id))
                .is_some_and(|site| {
                    site.case_id == area.case_id
                        && coordinate_area_contains_e7(
                            area.center_longitude_e7,
                            area.center_latitude_e7,
                            area.radius_m,
                            area.coordinates_are_geographic,
                            site.longitude_e7,
                            site.latitude_e7,
                            site.coordinates_are_geographic,
                        )
                });
            if !in_origin && !at_case_site {
                return Err("The party is not near the approximate search area".into());
            }
            Ok(())
        }
        "site" => {
            if matches!(
                kind,
                action::InvestigationActionKind::FollowTracks
                    | action::InvestigationActionKind::ReacquireTracks
            ) {
                return validate_tracking_action_origin(ctx, actor, capability, kind);
            }
            if character_case_site_id(ctx, actor.id).as_deref()
                == Some(capability.target_id.as_str())
            {
                return Ok(());
            }
            Err("The party must occupy the action's authoritative site".into())
        }
        "tracks" | "route" => validate_tracking_action_origin(ctx, actor, capability, kind),
        _ => Err("Investigation action has no authoritative position binding".into()),
    }
}

fn validate_generated_pattern_condition(
    ctx: &ReducerContext,
    capability: &InvestigationActionCapability,
    kind: action::InvestigationActionKind,
    started_at: u64,
) -> Result<(), String> {
    let output = ctx
        .db
        .investigation_generated_action_output()
        .capability_id()
        .find(&capability.id);
    let authority = generated_authority_reducer(ctx, capability)
        .map_err(|()| "Generated action authority is ambiguous")?;
    let (evidence_id, condition) = match generated_pattern_authority(
        capability,
        authority
            .as_ref()
            .map(|(manifest, context)| (manifest.as_str(), context.as_str())),
        output.as_ref().map(|output| output.outputs_json.as_str()),
    ) {
        GeneratedPatternAuthority::Manual | GeneratedPatternAuthority::GeneratedWithoutPattern => {
            return Ok(());
        }
        GeneratedPatternAuthority::Pattern {
            evidence_id,
            condition,
        } => (evidence_id, condition),
        GeneratedPatternAuthority::Invalid => {
            return Err("Generated action output authority is invalid".into());
        }
    };
    if !observer_pattern_route_has_live_corroborated_clue(
        capability.owner_character_id,
        &capability.case_id,
        &evidence_id,
        ctx.db
            .investigation_evidence_knowledge()
            .owner_character_id()
            .filter(capability.owner_character_id),
    ) {
        return Err("The selected pattern has not been corroborated yet".into());
    }
    use adventuresim_core::quest_generation::GeneratedPatternCondition as C;
    match &condition {
        C::NightWindow if started_at % 1_440 >= 360 && started_at % 1_440 < 1_200 => {
            Err("The learned pattern requires acting during the nighttime window".into())
        }
        C::RoadRoute if capability.target_kind != "route" => {
            Err("The learned roadside pattern is not bound to route geography".into())
        }
        C::VictimProfile {
            cohort_id,
            demographic,
            age_band,
            sex,
            profession,
        } => {
            if kind != action::InvestigationActionKind::Patrol
                || capability.target_kind != "cohort"
                || capability.target_id != *cohort_id
            {
                return Err("The learned victim profile targets another cohort".into());
            }
            let target = ctx
                .db
                .investigation_pattern_target_authority()
                .cohort_id()
                .find(cohort_id)
                .ok_or("Victim cohort authority no longer exists")?;
            let expected_demographic = format!("{demographic:?}").to_ascii_lowercase();
            if target.case_id != capability.case_id
                || target.demographic != expected_demographic
                || target.age_band != *age_band
                || target.sex != *sex
                || target.profession != *profession
            {
                return Err("Victim cohort profile no longer matches its authority".into());
            }
            let npc = ctx
                .db
                .settlement_npc()
                .id()
                .find(&target.npc_id)
                .ok_or("Victim cohort NPC no longer exists")?;
            let presence = ctx
                .db
                .settlement_npc_presence()
                .npc_id()
                .find(&target.npc_id)
                .ok_or("Victim cohort target is unavailable")?;
            let expected = adventuresim_core::quest_generation::GeneratedPatternTarget {
                cohort_id: target.cohort_id.clone(),
                npc_id: target.npc_id.clone(),
                demographic: *demographic,
                age_band: target.age_band.clone(),
                sex: target.sex.clone(),
                profession: target.profession.clone(),
                expected_settlement_id: target.expected_settlement_id.clone(),
                expected_location: target.expected_location.clone(),
                expected_location_label: String::new(),
                presence_version: target.presence_version,
            };
            let current = if target.sex.is_empty() {
                crate::strategic::developer_npc_witness_candidate(&npc, &presence)
                    .ok_or("Victim cohort NPC no longer has a visible demographic")?
            } else {
                adventuresim_core::quest_generation::WitnessCandidate {
                    npc_id: npc.id.clone(),
                    display_name: npc.name.clone(),
                    demographic: crate::strategic::generated_npc_demographic(&npc),
                    age_band: format!("{:?}", npc.age_band).to_ascii_lowercase(),
                    sex: format!("{:?}", npc.sex).to_ascii_lowercase(),
                    profession: npc.profession.clone(),
                    visible_description: String::new(),
                    expected_location: presence.location_id.clone(),
                    expected_location_label: presence.location_id.clone(),
                    presence_version: crate::strategic::generated_npc_presence_version(
                        &npc, &presence,
                    ),
                    allowed_circumstances: Default::default(),
                }
            };
            if !adventuresim_core::quest_generation::pattern_target_matches(
                &expected,
                &current,
                &presence.settlement_id,
            ) || !crate::settlement_population::npc_is_present(&presence, started_at)
            {
                return Err("Victim cohort target moved, changed, or is unavailable".into());
            }
            Ok(())
        }
        C::BroadSurvey
            if kind != action::InvestigationActionKind::SearchArea
                || capability.target_kind != "area" =>
        {
            Err("An irregular pattern requires a broad area search".into())
        }
        _ => Ok(()),
    }
}

fn validate_live_action_prerequisites(
    ctx: &ReducerContext,
    actor: &crate::Character,
    party_id: &str,
    capability: &InvestigationActionCapability,
    kind: action::InvestigationActionKind,
) -> Result<Vec<u64>, String> {
    if !tracking_capability_chain_is_coherent(
        capability,
        kind,
        |id| {
            ctx.db
                .investigation_action_capability()
                .id()
                .find(&id.to_owned())
        },
        |id| {
            ctx.db
                .investigation_action_attempt()
                .capability_id()
                .filter(id)
                .any(|attempt| attempt.success)
        },
    ) {
        return Err(INVALID_INVESTIGATION_ROUTE_ERROR.into());
    }
    if !capability_has_live_support_reducer(ctx, capability, kind) {
        return Err("The current journal no longer supports this investigation route".into());
    }
    require_party_ready(ctx, party_id)?;
    require_no_unresolved_encounter(ctx, party_id)?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .ok_or("Party not found")?;
    if party.camp_destination.is_some()
        || party.camp_remaining_minutes > 0
        || ctx
            .db
            .party_journey_authority()
            .party_id()
            .find(&party_id.to_string())
            .is_some()
    {
        return Err("Investigation cannot begin during a journey or camp".into());
    }
    let members = living_party_member_ids(ctx, party_id);
    if members.len() < usize::from(action::prerequisites(kind).minimum_party_members) {
        return Err("Not enough living party members for this action".into());
    }
    let actor_site = character_case_site_id(ctx, actor.id);
    for member_id in &members {
        let member = ctx
            .db
            .character()
            .id()
            .find(*member_id)
            .ok_or("Party member no longer exists")?;
        if member.current_settlement_id != actor.current_settlement_id
            || character_case_site_id(ctx, *member_id) != actor_site
        {
            return Err("Every living party member must be co-located".into());
        }
    }
    if !capability.required_action_id.is_empty() {
        let predecessor = ctx
            .db
            .investigation_action_capability()
            .id()
            .find(&capability.required_action_id)
            .ok_or("Required investigation lead no longer exists")?;
        if predecessor.owner_character_id != capability.owner_character_id
            || predecessor.case_id != capability.case_id
            || !ctx
                .db
                .investigation_action_attempt()
                .capability_id()
                .filter(&predecessor.id)
                .any(|attempt| attempt.success)
        {
            return Err("The preceding investigation lead is not complete".into());
        }
    }
    let prereqs = action::prerequisites(kind);
    if prereqs.requires_contact_referral
        && !ctx
            .db
            .investigation_lead()
            .owner_character_id()
            .filter(actor.id)
            .any(|lead| lead_is_live_contact_referral(&lead, actor.id, &capability.case_id))
    {
        return Err("No live witness referral supports this action".into());
    }
    if prereqs.requires_approximate_destination
        && capability.target_kind != "area"
        && !ctx
            .db
            .investigation_lead()
            .owner_character_id()
            .filter(actor.id)
            .any(|lead| {
                lead.case_id == capability.case_id
                    && lead.destination_stage == "approximate_area"
                    && lead.corrected_by.is_empty()
            })
    {
        return Err("No current approximate destination supports this action".into());
    }
    if prereqs.requires_tracks && capability.required_action_id.is_empty() {
        return Err("No authoritative track source supports this action".into());
    }
    validate_action_position(ctx, actor, capability, kind)?;
    Ok(members)
}

fn case_objective_contains_custody_target(
    ctx: &ReducerContext,
    case_id: &str,
    object_kind: CustodyObjectKind,
    object_id: &str,
) -> Result<bool, String> {
    let case = ctx
        .db
        .case_authority()
        .id()
        .find(&case_id.to_string())
        .ok_or("Investigation case no longer exists")?;
    let expression: adventuresim_core::case::ObjectiveExpression =
        serde_json::from_str(&case.objective_expression_json)
            .map_err(|_| "Case objective authority is invalid")?;
    use adventuresim_core::case::ObjectiveRequirement as R;
    Ok(expression
        .alternatives
        .iter()
        .flat_map(|path| &path.objectives)
        .any(|objective| match (&objective.requirement, object_kind) {
            (R::Retrieve { asset_id }, CustodyObjectKind::Asset) => asset_id.as_str() == object_id,
            (R::Rescue { subject_id }, CustodyObjectKind::Subject) => {
                subject_id.as_str() == object_id
            }
            _ => false,
        }))
}

fn validate_pickup_custody(
    ctx: &ReducerContext,
    capability: &InvestigationActionCapability,
    party_id: &str,
    object_kind: CustodyObjectKind,
    object_id: &str,
    expected_next_version: u32,
) -> Result<u32, String> {
    if !case_objective_contains_custody_target(ctx, &capability.case_id, object_kind, object_id)? {
        return Err("Capability target is not an unresolved objective of this case".into());
    }
    let current = ctx
        .db
        .case_custody()
        .object_id()
        .find(&object_id.to_string())
        .ok_or("Capability target has no custody authority")?;
    if current.case_id != capability.case_id
        || current.object_kind != object_kind
        || current.holder_kind != CustodyHolderKind::Site
        || capability.target_kind != "site"
        || current.holder_id != capability.target_id
    {
        return Err("Capability target is not legally present at this investigation site".into());
    }
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .ok_or("Party not found")?;
    if party.current_case_site_id.as_deref() != Some(current.holder_id.as_str()) {
        return Err("Party is not at the custody site".into());
    }
    let next = current.version.saturating_add(1);
    if expected_next_version != next {
        return Err("Capability custody version is stale and must be reissued".into());
    }
    Ok(next)
}

fn reissue_stale_custody_capability(
    ctx: &ReducerContext,
    capability: &mut InvestigationActionCapability,
    party_id: &str,
) -> Result<bool, String> {
    let consequence: InvestigationActionConsequence =
        serde_json::from_str(&capability.consequence_json)
            .map_err(|_| "Investigation action consequence authority is invalid")?;
    let (object_kind, object_id, expected) = match &consequence {
        InvestigationActionConsequence::RetrieveAsset { asset_id, version } => {
            (CustodyObjectKind::Asset, asset_id.as_str(), *version)
        }
        InvestigationActionConsequence::RescueSubject {
            subject_id,
            version,
        } => (CustodyObjectKind::Subject, subject_id.as_str(), *version),
        _ => return Ok(false),
    };
    let current = ctx
        .db
        .case_custody()
        .object_id()
        .find(&object_id.to_string())
        .ok_or("Capability target has no custody authority")?;
    let next = current.version.saturating_add(1);
    if expected == next {
        return Ok(false);
    }
    // A changed version is recoverable only while every semantic binding is
    // still identical. Holder/site/case changes are authority failures.
    validate_pickup_custody(ctx, capability, party_id, object_kind, object_id, next)?;
    let refreshed = match consequence {
        InvestigationActionConsequence::RetrieveAsset { asset_id, .. } => {
            InvestigationActionConsequence::RetrieveAsset {
                asset_id,
                version: next,
            }
        }
        InvestigationActionConsequence::RescueSubject { subject_id, .. } => {
            InvestigationActionConsequence::RescueSubject {
                subject_id,
                version: next,
            }
        }
        _ => unreachable!(),
    };
    capability.consequence_json = serde_json::to_string(&refreshed)
        .map_err(|_| "Refreshed investigation consequence is invalid")?;
    capability.version = capability.version.saturating_add(1);
    capability.seed = ctx.random::<u64>();
    ctx.db
        .investigation_action_capability()
        .id()
        .update(capability.clone());
    ctx.db
        .investigation_action_outcome()
        .insert(InvestigationActionOutcome {
        id: generated_observer_id(
            ctx,
            &capability.case_id,
            "outcome",
            &format!("reissue:{}:{}", capability.id, capability.version),
        )
        .unwrap_or_else(|| {
            inv::compound_id(&[
                "outcome",
                "reissue",
                &capability.id,
                &capability.version.to_string(),
            ])
        }),
        owner_character_id: capability.owner_character_id,
        case_id: capability.case_id.clone(),
        capability_id: capability.id.clone(),
        attempt_id: String::new(),
        safe_wording:
            "The situation changed before you acted; the lead was refreshed without spending time."
                .into(),
        recorded_at: character_strategic_minute(ctx, capability.owner_character_id),
        official_recorded_at: official_minute(ctx),
    });
    Ok(true)
}

fn commit_action_consequence(
    ctx: &ReducerContext,
    capability: &InvestigationActionCapability,
    party_id: &str,
    attempt_id: &str,
) -> Result<(), String> {
    let consequence: InvestigationActionConsequence =
        serde_json::from_str(&capability.consequence_json)
            .map_err(|_| "Investigation action consequence authority is invalid")?;
    match consequence {
        InvestigationActionConsequence::None => Ok(()),
        InvestigationActionConsequence::RetrieveAsset { asset_id, version } => {
            let version = validate_pickup_custody(
                ctx,
                capability,
                party_id,
                CustodyObjectKind::Asset,
                &asset_id,
                version,
            )?;
            crate::strategic::record_asset_retrieved(
                ctx,
                attempt_id,
                &capability.case_id,
                party_id,
                &asset_id,
                version,
            )
            .map(|_| ())
        }
        InvestigationActionConsequence::RescueSubject {
            subject_id,
            version,
        } => {
            let version = validate_pickup_custody(
                ctx,
                capability,
                party_id,
                CustodyObjectKind::Subject,
                &subject_id,
                version,
            )?;
            crate::strategic::record_subject_rescued_or_released(
                ctx,
                attempt_id,
                &capability.case_id,
                party_id,
                &subject_id,
                version,
                false,
            )
            .map(|_| ())
        }
    }
}

fn generated_progress_kind(kind: action::InvestigationActionKind) -> bool {
    use action::InvestigationActionKind as K;
    match kind {
        K::InspectSite
        | K::SearchArea
        | K::FollowTracks
        | K::ReacquireTracks
        | K::LocateContact
        | K::Watch
        | K::Patrol
        | K::LayAmbush
        | K::ApproachLead => true,
    }
}

fn capability_uses_bounded_progress(
    provenance_kind: &str,
    kind: action::InvestigationActionKind,
) -> bool {
    provenance_kind == "generated" && generated_progress_kind(kind)
}

fn contiguous_failed_attempts(
    capability_id: &str,
    owner_character_id: u64,
    method: &str,
    current_version: u32,
    attempts: impl IntoIterator<Item = InvestigationActionAttempt>,
) -> u32 {
    let attempts = attempts
        .into_iter()
        .filter(|attempt| {
            attempt.capability_id == capability_id
                && attempt.owner_character_id == owner_character_id
                && attempt.method == method
                && !attempt.success
                && attempt.expected_version < current_version
        })
        .map(|attempt| (attempt.expected_version, attempt))
        .collect::<BTreeMap<_, _>>();
    let mut cursor = current_version;
    let mut failures = 0;
    while cursor > 0 {
        cursor -= 1;
        if attempts.get(&cursor).is_none() {
            break;
        }
        failures += 1;
    }
    failures
}

fn bounded_failure_wording(
    progress: action::BoundedProgressResolution,
    alternate_available: bool,
) -> String {
    let threshold_whole = progress.success_threshold_bps / 100;
    let threshold_fraction = progress.success_threshold_bps % 100;
    let progress_whole = progress.persistent_progress_bps / 100;
    let progress_fraction = progress.persistent_progress_bps % 100;
    let alternate = if alternate_available {
        " Another currently supported route is also available."
    } else {
        " No alternate route is currently supported by the leads in your journal."
    };
    format!(
        "No conclusive result. Persistent fieldwork advanced this exact route to attempt {} of {}; accumulated progress added {progress_whole}.{progress_fraction:02}% to this attempt's bounded success threshold of {threshold_whole}.{threshold_fraction:02}%, uncertainty fell to {}.{:02}%, and contiguous work guarantees success by attempt {}.{alternate}",
        progress.attempt_number,
        progress.guaranteed_by_attempt,
        progress.resolution.resulting_uncertainty_bps / 100,
        progress.resolution.resulting_uncertainty_bps % 100,
        progress.guaranteed_by_attempt,
    )
}

fn private_action_resolution_json(
    resolution: action::Resolution,
    bounded_progress: Option<action::BoundedProgressResolution>,
) -> Result<String, String> {
    if let Some(progress) = bounded_progress {
        serde_json::to_string(&serde_json::json!({
            "resolution": progress.resolution,
            "attempt_number": progress.attempt_number,
            "persistent_progress_bps": progress.persistent_progress_bps,
            "success_threshold_bps": progress.success_threshold_bps,
            "guaranteed_by_attempt": progress.guaranteed_by_attempt,
        }))
    } else {
        // Preserve the historical bare Resolution audit shape for manual and
        // otherwise unbounded actions.
        serde_json::to_string(&resolution)
    }
    .map_err(|_| "Investigation resolution could not be recorded".into())
}

fn capability_progress_depends_on_exact_lead(
    capability: &InvestigationActionCapability,
    lead: &InvestigationLead,
    generated_case_aliases: Option<(&str, &str)>,
) -> bool {
    let case_matches = match (capability.provenance_kind.as_str(), generated_case_aliases) {
        ("manual", None) => capability.case_id == lead.case_id,
        ("generated", Some((canonical, public))) => {
            capability.generated_case_id == canonical
                && (capability.case_id == canonical || capability.case_id == public)
                && (lead.case_id == canonical || lead.case_id == public)
        }
        _ => false,
    };
    capability.provenance_kind == "generated"
        && capability.active
        && capability.owner_character_id == lead.owner_character_id
        && case_matches
        && capability.target_kind == "site"
        && capability.target_id == lead.exact_location_id
        && matches!(
            lead.destination_stage.as_str(),
            "exact_believed" | "visited"
        )
        && parse_action_kind(&capability.method).is_ok_and(generated_progress_kind)
}

fn dependent_capability_ids_for_exact_lead(
    ctx: &ReducerContext,
    lead: &InvestigationLead,
) -> BTreeSet<String> {
    if lead.exact_location_id.is_empty() {
        return BTreeSet::new();
    }
    ctx.db
        .investigation_action_capability()
        .owner_character_id()
        .filter(lead.owner_character_id)
        .filter(|capability| {
            let aliases = generated_authority_reducer(ctx, capability)
                .ok()
                .flatten()
                .and_then(|(manifest, _)| {
                    serde_json::from_str::<adventuresim_core::quest_generation::GeneratedCase>(
                        &manifest,
                    )
                    .ok()
                    .map(|generated| (generated.canonical_case_id, generated.public_case_id))
                });
            capability_progress_depends_on_exact_lead(
                capability,
                lead,
                aliases
                    .as_ref()
                    .map(|(canonical, public)| (canonical.as_str(), public.as_str())),
            )
        })
        .map(|capability| capability.id)
        .collect()
}

fn unique_capability_ids(capability_ids: impl IntoIterator<Item = String>) -> BTreeSet<String> {
    capability_ids.into_iter().collect()
}

fn correction_requires_progress_reset(has_live_replacement_support: bool) -> bool {
    !has_live_replacement_support
}

fn reset_capability_progress_if_unsupported(
    capability: &mut InvestigationActionCapability,
    has_live_replacement_support: bool,
    replacement_seed: impl FnOnce() -> u64,
) -> bool {
    if !correction_requires_progress_reset(has_live_replacement_support) {
        return false;
    }
    capability.version = capability.version.saturating_add(1);
    capability.seed = replacement_seed();
    true
}

fn reset_unsupported_capability_progress(
    ctx: &ReducerContext,
    capability_ids: impl IntoIterator<Item = String>,
) -> Result<(), String> {
    for capability_id in unique_capability_ids(capability_ids) {
        let Some(mut capability) = ctx
            .db
            .investigation_action_capability()
            .id()
            .find(&capability_id)
        else {
            continue;
        };
        let has_live_replacement_support =
            exact_action_case_site_for_observer(ctx, &capability).is_some();
        if !reset_capability_progress_if_unsupported(
            &mut capability,
            has_live_replacement_support,
            || ctx.random::<u64>(),
        ) {
            continue;
        }
        ctx.db
            .investigation_action_capability()
            .id()
            .update(capability);
    }
    Ok(())
}

pub(crate) fn perform_investigation_action_authorized(
    ctx: &ReducerContext,
    actor_id: u64,
    action_id: String,
    method: String,
    expected_version: u32,
    leader_approved: bool,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, actor_id)?;
    let actor = crate::character::require_living_character(ctx, actor_id)?;
    let party_id = actor.party_id.clone().ok_or("Must be in a party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != actor_id && !leader_approved {
        return Err("Party leader approval is required".into());
    }
    let attempt_id = inv::compound_id(&[
        "attempt",
        &action_id,
        &actor_id.to_string(),
        &expected_version.to_string(),
    ]);
    if let Some(attempt) = ctx.db.investigation_action_attempt().id().find(&attempt_id) {
        return if attempt.owner_character_id == actor_id
            && attempt.capability_id == action_id
            && attempt.method == method
            && attempt.expected_version == expected_version
        {
            Ok(())
        } else {
            Err("Investigation attempt id conflicts with an earlier action".into())
        };
    }
    let mut capability = ctx
        .db
        .investigation_action_capability()
        .id()
        .find(&action_id)
        .ok_or("Investigation action is unavailable")?;
    if capability.owner_character_id != actor_id
        || !capability.active
        || capability.method != method
        || capability.version != expected_version
    {
        return Err("Investigation action is stale or belongs to another observer".into());
    }
    if reissue_stale_custody_capability(ctx, &mut capability, &party_id)? {
        return Ok(());
    }
    let kind = parse_action_kind(&method)?;
    let target_terrain = parse_action_terrain(&capability.target_terrain)?;
    validate_action_route_graph(ctx, actor_id, &capability.case_id)?;
    let members = validate_live_action_prerequisites(ctx, &actor, &party_id, &capability, kind)?;
    let started_at = synchronize_party_activity_time(ctx, &members, party.leader_id)?;
    validate_generated_pattern_condition(ctx, &capability, kind, started_at)?;
    let mut route_skills = party_action_skills(ctx, &party_id, actor_id, target_terrain)?;
    if let Some(investigability) = generated_investigability(ctx, &capability) {
        route_skills = apply_investigability_to_route_skills(route_skills, investigability);
    }
    let resolution_input = action::ResolutionInput {
        seed: capability.seed,
        attempt_index: expected_version,
        kind,
        terrain: actor_action_terrain(ctx, &actor),
        target_terrain,
        time_of_day: if started_at % 1_440 < 360 || started_at % 1_440 >= 1_200 {
            action::TimeOfDay::Night
        } else {
            action::TimeOfDay::Day
        },
        evidence_age_minutes: started_at.saturating_sub(capability.evidence_age_origin_minute),
        current_uncertainty_bps: capability.uncertainty_bps,
        skills: route_skills,
        weather: actor_action_weather(ctx, &actor, started_at),
    };
    let bounded_progress = capability_uses_bounded_progress(&capability.provenance_kind, kind)
        .then(|| {
            let prior_failures = contiguous_failed_attempts(
                &capability.id,
                capability.owner_character_id,
                &capability.method,
                capability.version,
                ctx.db
                    .investigation_action_attempt()
                    .capability_id()
                    .filter(&capability.id),
            );
            action::resolve_with_bounded_progress(resolution_input, prior_failures)
        });
    let resolution = bounded_progress
        .map(|progress| progress.resolution)
        .unwrap_or_else(|| action::resolve(resolution_input));
    // This is the final mutation-boundary validation. Browser previews and
    // party votes are UX; only this transaction authorizes the shared time.
    validate_live_action_prerequisites(ctx, &actor, &party_id, &capability, kind)?;
    validate_generated_pattern_condition(ctx, &capability, kind, started_at)?;
    for member_id in &members {
        if !advance_investigation_time(ctx, *member_id, u64::from(resolution.cost.minutes))? {
            return Err("Every living party member must survive the investigation interval".into());
        }
    }
    crate::strategic::reconcile_party_objective_continuity(ctx, &party_id)?;
    if resolution.success {
        commit_action_consequence(ctx, &capability, &party_id, &attempt_id)?;
    }
    persist_action_result_lead(ctx, &capability, &attempt_id, &resolution)?;
    let completed_at = ctx
        .db
        .character_time()
        .character_id()
        .find(party.leader_id)
        .ok_or("Party leader strategic clock disappeared")?
        .minutes;
    ctx.db
        .investigation_action_attempt()
        .insert(InvestigationActionAttempt {
            id: attempt_id.clone(),
            capability_id: action_id.clone(),
            owner_character_id: actor_id,
            expected_version,
            method,
            started_at,
            completed_at,
            duration_minutes: resolution.cost.minutes,
            success: resolution.success,
            resulting_uncertainty_bps: resolution.resulting_uncertainty_bps,
            private_resolution_json: private_action_resolution_json(resolution, bounded_progress)?,
        });
    let outcome_case_id = capability.case_id.clone();
    let safe_result_on_success = capability.safe_result_on_success.clone();
    capability.version = capability.version.saturating_add(1);
    capability.seed = ctx.random::<u64>();
    capability.uncertainty_bps = resolution.resulting_uncertainty_bps;
    capability.active = !resolution.success;
    ctx.db
        .investigation_action_capability()
        .id()
        .update(capability);
    let alternate_available = activate_action_successors(
        ctx,
        &ctx.db
            .investigation_action_capability()
            .id()
            .find(&action_id)
            .ok_or("Investigation action disappeared")?,
        resolution.success,
    )?;
    ctx.db
        .investigation_action_outcome()
        .insert(InvestigationActionOutcome {
            id: generated_observer_id(ctx, &outcome_case_id, "outcome", &attempt_id)
                .unwrap_or_else(|| inv::compound_id(&["outcome", &attempt_id])),
            owner_character_id: actor_id,
            case_id: outcome_case_id,
            capability_id: action_id.clone(),
            attempt_id: attempt_id.clone(),
            safe_wording: if resolution.success {
                if resolution.risk_triggered {
                    format!(
                        "{} The party was exposed to danger during the attempt.",
                        safe_result_on_success
                    )
                } else {
                    safe_result_on_success
                }
            } else if let Some(progress) = bounded_progress {
                bounded_failure_wording(progress, alternate_available)
            } else {
                adventuresim_core::quest_generation::failed_action_outcome_wording(
                    alternate_available,
                )
                .into()
            },
            recorded_at: completed_at,
            official_recorded_at: official_minute(ctx),
        });
    Ok(())
}

#[reducer]
pub fn perform_investigation_action(
    ctx: &ReducerContext,
    actor_id: u64,
    action_id: String,
    method: String,
    expected_version: u32,
) -> Result<(), String> {
    perform_investigation_action_authorized(
        ctx,
        actor_id,
        action_id,
        method,
        expected_version,
        false,
    )
}
