#[view(accessor = backend_case_site_pins, public)]
pub fn backend_case_site_pins(ctx: &ViewContext) -> Vec<BackendCaseSitePin> {
    if !is_gateway(ctx) {
        return Vec::new();
    }
    let mut pins: BTreeMap<(u64, String), BackendCaseSitePin> = BTreeMap::new();
    for lead in ctx
        .db
        .investigation_lead()
        .owner_character_id()
        .filter(0u64..)
        .filter(|lead| {
            lead.corrected_by.is_empty()
                && matches!(
                    lead.destination_stage.as_str(),
                    "exact_believed" | "visited"
                )
        })
        .filter_map(|lead| {
            let site = ctx
                .db
                .case_site_authority()
                .id_key()
                .find(&lead.exact_location_id)?;
            let aliases = case_site_provenance_view(ctx, &site)?;
            if !lead_projects_exact_case_site_pin(
                &lead,
                &site,
                aliases.as_ref().map(|aliases| aliases.1.as_str()),
            ) {
                return None;
            }
            let presentation =
                case_site_presentation_view(ctx, lead.owner_character_id, &site, aliases.as_ref())?;
            let tracked = ctx
                .db
                .character()
                .id()
                .find(lead.owner_character_id)
                .and_then(|character| character.party_id)
                .and_then(|party_id| ctx.db.party_case_site_tracking().party_id().find(&party_id))
                .is_some_and(|row| {
                    row.observer_character_id == lead.owner_character_id
                        && row.case_site_id == site.id
                });
            Some(BackendCaseSitePin {
                owner_character_id: lead.owner_character_id,
                case_id: aliases
                    .as_ref()
                    .map_or_else(|| site.case_id.clone(), |aliases| aliases.1.clone()),
                case_site_id: site.id.value,
                origin_settlement_id: site.origin_settlement_id,
                name: site.name,
                description: site.description,
                scene_key: site.scene_key,
                longitude_e7: lead.longitude_e7,
                latitude_e7: lead.latitude_e7,
                coordinates_are_geographic: site.coordinates_are_geographic,
                distance_m: site.distance_m,
                knowledge_stage: lead.destination_stage,
                tracked,
                display_title: presentation.display_title,
                generated_case: presentation.generated_case,
                case_resolved: presentation.case_resolved,
                combat_available: presentation.combat_available,
            })
        })
    {
        let key = (lead.owner_character_id, lead.case_site_id.clone());
        match pins.get(&key) {
            Some(existing)
                if existing.knowledge_stage == "visited" || lead.knowledge_stage != "visited" => {}
            _ => {
                pins.insert(key, lead);
            }
        }
    }
    pins.into_values().collect()
}

struct CaseSitePresentationView {
    display_title: String,
    generated_case: bool,
    case_resolved: bool,
    combat_available: bool,
}

fn case_site_presentation_view(
    ctx: &ViewContext,
    owner_character_id: u64,
    site: &CaseSiteAuthority,
    aliases: Option<&(String, String)>,
) -> Option<CaseSitePresentationView> {
    let Some((canonical_case_id, _public_case_id)) = aliases else {
        return Some(CaseSitePresentationView {
            display_title: site.name.clone(),
            generated_case: false,
            case_resolved: false,
            combat_available: false,
        });
    };
    let authority = ctx
        .db
        .quest_generation_authority()
        .case_id()
        .find(canonical_case_id)?;
    let validated = validate_quest_generation_authority(&authority).ok()?;
    let generated_site = validated
        .manifest
        .sites
        .iter()
        .find(|generated_site| generated_site.id.0 == site.id.value)?;
    if generated_site.safe_label != site.name {
        return None;
    }
    let case = ctx.db.case_authority().id().find(canonical_case_id)?;
    let party_id = ctx
        .db
        .character()
        .id()
        .find(owner_character_id)
        .and_then(|character| character.party_id);
    let hostile_groups: Vec<_> = generated_case_site_combat_group_id(&validated.manifest, site)
        .and_then(|group_id| {
            ctx.db
                .hostile_group_authority()
                .id()
                .find(&group_id.to_string())
        })
        .into_iter()
        .collect();
    let finales: Vec<_> = ctx
        .db
        .case_finale_authority()
        .case_id()
        .filter(canonical_case_id)
        .collect();
    let facts = ctx
        .db
        .case_outcome_fact()
        .case_id()
        .filter(canonical_case_id)
        .map(|row| serde_json::from_str(&row.fact_json))
        .collect::<Result<Vec<adventuresim_core::case::OutcomeFact>, _>>()
        .ok();
    let combat_available =
        party_id
            .as_deref()
            .zip(facts.as_deref())
            .is_some_and(|(party_id, facts)| {
                generated_case_site_combat_eligible(
                    &validated.manifest,
                    &case,
                    site,
                    &hostile_groups,
                    &finales,
                    facts,
                    party_id,
                )
                .is_some()
            });
    Some(CaseSitePresentationView {
        display_title: validated.manifest.consequence.public_summary,
        generated_case: true,
        case_resolved: case.resolution_status != crate::strategic::CaseResolutionStatus::Open,
        combat_available,
    })
}

fn lead_projects_exact_case_site_pin(
    lead: &InvestigationLead,
    site: &CaseSiteAuthority,
    generated_public_case_id: Option<&str>,
) -> bool {
    lead.corrected_by.is_empty()
        && matches!(
            lead.destination_stage.as_str(),
            "exact_believed" | "visited"
        )
        && (lead.case_id == site.case_id
            || generated_public_case_id.is_some_and(|public| lead.case_id == public))
        && lead.exact_location_id == site.id.value
        && lead.latitude_e7 == site.latitude_e7
        && lead.longitude_e7 == site.longitude_e7
}

fn validated_case_site_aliases(
    case: &crate::strategic::CaseAuthority,
    authorities: impl IntoIterator<Item = crate::strategic::QuestGenerationAuthority>,
) -> Option<Option<(String, String)>> {
    let mut authorities: Vec<_> = authorities
        .into_iter()
        .filter(|authority| {
            authority.case_id == case.id
                || authority.public_case_id == case.id
                || (!case.generated_case_id.is_empty()
                    && (authority.case_id == case.generated_case_id
                        || authority.public_case_id == case.generated_case_id))
        })
        .collect();
    authorities.sort_by(|left, right| left.case_id.cmp(&right.case_id));
    authorities.dedup_by(|left, right| left.case_id == right.case_id);
    match case.provenance_kind.as_str() {
        "manual" if case.generated_case_id.is_empty() && authorities.is_empty() => Some(None),
        "generated" if case.generated_case_id == case.id && authorities.len() == 1 => {
            let validated = validate_quest_generation_authority(&authorities[0]).ok()?;
            (validated.manifest.canonical_case_id == case.id).then_some(Some((
                validated.manifest.canonical_case_id,
                validated.manifest.public_case_id,
            )))
        }
        _ => None,
    }
}

fn case_site_provenance_view(
    ctx: &ViewContext,
    site: &CaseSiteAuthority,
) -> Option<Option<(String, String)>> {
    let case = ctx.db.case_authority().id().find(&site.case_id)?;
    let mut authorities = Vec::new();
    for alias in [&case.id, &case.generated_case_id] {
        if alias.is_empty()
            || authorities
                .iter()
                .any(|authority: &crate::strategic::QuestGenerationAuthority| {
                    authority.case_id == alias.as_str()
                })
        {
            continue;
        }
        if let Some(authority) = ctx.db.quest_generation_authority().case_id().find(alias) {
            authorities.push(authority);
        }
        authorities.extend(
            ctx.db
                .quest_generation_authority()
                .public_case_id()
                .filter(alias),
        );
    }
    validated_case_site_aliases(&case, authorities)
}

pub(crate) fn case_site_provenance_reducer(
    ctx: &ReducerContext,
    site: &CaseSiteAuthority,
) -> Option<Option<(String, String)>> {
    let case = ctx.db.case_authority().id().find(&site.case_id)?;
    let mut authorities = Vec::new();
    for alias in [&case.id, &case.generated_case_id] {
        if alias.is_empty()
            || authorities
                .iter()
                .any(|authority: &crate::strategic::QuestGenerationAuthority| {
                    authority.case_id == alias.as_str()
                })
        {
            continue;
        }
        if let Some(authority) = ctx.db.quest_generation_authority().case_id().find(alias) {
            authorities.push(authority);
        }
        authorities.extend(
            ctx.db
                .quest_generation_authority()
                .public_case_id()
                .filter(alias),
        );
    }
    validated_case_site_aliases(&case, authorities)
}

#[view(accessor = backend_character_case_site_locations, public)]
pub fn backend_character_case_site_locations(
    ctx: &ViewContext,
) -> Vec<BackendCharacterCaseSiteLocation> {
    if !is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .character_case_site_occupancy()
        .gateway_bucket()
        .filter(0u8)
        .map(|row| BackendCharacterCaseSiteLocation {
            character_id: row.character_id,
            case_site_id: row.case_site_id,
        })
        .collect()
}

fn exact_action_case_site_for_observer(
    ctx: &ReducerContext,
    capability: &InvestigationActionCapability,
) -> Option<(
    CaseSiteAuthority,
    InvestigationLead,
    Option<(String, String)>,
)> {
    let site = ctx
        .db
        .case_site_authority()
        .id_key()
        .find(&capability.target_id)?;
    let generated_aliases = case_site_provenance_reducer(ctx, &site)?;
    match (&generated_aliases, capability.provenance_kind.as_str()) {
        (None, "manual") if capability.generated_case_id.is_empty() => {}
        (Some((canonical, public)), "generated")
            if capability.generated_case_id == canonical.as_str()
                && (capability.case_id == canonical.as_str()
                    || capability.case_id == public.as_str()) => {}
        _ => return None,
    }
    ctx.db
        .investigation_lead()
        .owner_character_id()
        .filter(capability.owner_character_id)
        .find(|lead| {
            lead.exact_location_id == capability.target_id
                && (lead.case_id == capability.case_id
                    || generated_aliases.as_ref().is_some_and(|aliases| {
                        lead.case_id == aliases.0 || lead.case_id == aliases.1
                    }))
                && lead.latitude_e7 == site.latitude_e7
                && lead.longitude_e7 == site.longitude_e7
                && lead.corrected_by.is_empty()
                && matches!(
                    lead.destination_stage.as_str(),
                    "exact_believed" | "visited"
                )
        })
        .map(|lead| (site, lead, generated_aliases))
}

pub(crate) fn exact_case_site_for_observer(
    ctx: &ReducerContext,
    observer_character_id: u64,
    case_site_id: &str,
) -> Option<(CaseSiteAuthority, InvestigationLead)> {
    let site = ctx
        .db
        .case_site_authority()
        .id_key()
        .find(&case_site_id.to_string())?;
    let generated_aliases = case_site_provenance_reducer(ctx, &site)?;
    ctx.db
        .investigation_lead()
        .owner_character_id()
        .filter(observer_character_id)
        .find(|lead| {
            lead.exact_location_id == case_site_id
                && (lead.case_id == site.case_id
                    || generated_aliases
                        .as_ref()
                        .is_some_and(|aliases| lead.case_id == aliases.1.as_str()))
                && lead.latitude_e7 == site.latitude_e7
                && lead.longitude_e7 == site.longitude_e7
                && lead.corrected_by.is_empty()
                && matches!(
                    lead.destination_stage.as_str(),
                    "exact_believed" | "visited"
                )
        })
        .map(|lead| (site, lead))
}

pub(crate) fn disclose_exact_case_site(
    ctx: &ReducerContext,
    observer_character_id: u64,
    case_id: &str,
    site: &CaseSiteAuthority,
    source_label: &str,
) -> Result<(), String> {
    let aliases = case_site_provenance_reducer(ctx, site);
    if site.case_id != case_id
        && !aliases
            .flatten()
            .is_some_and(|(_, public_case_id)| public_case_id == case_id)
    {
        return Err("Case-site disclosure does not belong to the disclosed case".into());
    }
    let base_id = format!("case-site-disclosure:{observer_character_id}:{}", site.id);
    let recorded_at = crate::time::refresh_clock(ctx).unwrap_or(0);
    let mut disclosures: Vec<_> = ctx
        .db
        .investigation_lead()
        .owner_character_id()
        .filter(observer_character_id)
        .filter(|lead| lead.exact_location_id == site.id.value && lead.id.starts_with(&base_id))
        .collect();
    disclosures.sort_by(|left, right| left.id.cmp(&right.id));
    let active: Vec<_> = disclosures
        .iter()
        .filter(|lead| lead.corrected_by.is_empty())
        .cloned()
        .collect();
    if let Some(canonical_id) = active
        .iter()
        .find(|existing| {
            existing.case_id == case_id
                && existing.latitude_e7 == site.latitude_e7
                && existing.longitude_e7 == site.longitude_e7
                && matches!(
                    existing.destination_stage.as_str(),
                    "exact_believed" | "visited"
                )
        })
        .map(|lead| lead.id.clone())
    {
        for mut duplicate in active {
            if duplicate.id != canonical_id {
                duplicate.corrected_by = canonical_id.clone();
                ctx.db.investigation_lead().id().update(duplicate);
            }
        }
        return Ok(());
    }
    let id = if disclosures.is_empty() {
        base_id
    } else {
        format!("{base_id}:revision:{:08}", disclosures.len())
    };
    for mut stale in active {
        stale.corrected_by = id.clone();
        ctx.db.investigation_lead().id().update(stale);
    }
    ctx.db.investigation_lead().insert(InvestigationLead {
        id,
        owner_character_id: observer_character_id,
        case_id: case_id.into(),
        proposition_id: String::new(),
        summary: format!("Exact destination disclosed: {}", site.name),
        source_label: source_label.into(),
        confidence_bps: 10_000,
        destination_stage: "exact_believed".into(),
        directions: site.description.clone(),
        exact_location_id: site.id.value.clone(),
        latitude_e7: site.latitude_e7,
        longitude_e7: site.longitude_e7,
        witness_name: String::new(),
        witness_description: String::new(),
        witness_occupation_or_relationship: String::new(),
        expected_location: String::new(),
        current_learned_location: site.name.clone(),
        contradiction_group: format!("case-site:{}", site.case_id),
        corrected_by: String::new(),
        recorded_at,
    });
    Ok(())
}

/// Arrival is durable shared experience: every living traveler can navigate
/// back even if party leadership later changes.
pub(crate) fn mark_case_site_visited(
    ctx: &ReducerContext,
    observer_character_id: u64,
    site: &CaseSiteAuthority,
) -> Result<(), String> {
    disclose_exact_case_site(
        ctx,
        observer_character_id,
        &site.case_id,
        site,
        "visited with the party",
    )?;
    let active: Vec<_> = ctx
        .db
        .investigation_lead()
        .owner_character_id()
        .filter(observer_character_id)
        .filter(|lead| {
            lead.case_id == site.case_id
                && lead.exact_location_id == site.id.value
                && lead.latitude_e7 == site.latitude_e7
                && lead.longitude_e7 == site.longitude_e7
                && lead.corrected_by.is_empty()
                && matches!(
                    lead.destination_stage.as_str(),
                    "exact_believed" | "visited"
                )
        })
        .collect();
    for mut lead in active {
        if lead.destination_stage != "visited" {
            lead.destination_stage = "visited".into();
            ctx.db.investigation_lead().id().update(lead);
        }
    }
    Ok(())
}
