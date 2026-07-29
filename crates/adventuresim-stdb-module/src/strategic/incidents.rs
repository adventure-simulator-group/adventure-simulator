struct IncidentSpec<'a> {
    kind: IncidentKind,
    title: &'a str,
    description: String,
    enemy_type: &'a str,
    difficulty: i32,
}

fn create_strategic_incident(
    ctx: &ReducerContext,
    party_id: &str,
    settlement: &Settlement,
    instigator_id: u64,
    source_id: IncidentSourceId,
    spec: IncidentSpec<'_>,
) -> Result<Option<IncidentId>, String> {
    parse_threat(spec.enemy_type)?;
    let Some(mut party) = ctx.db.party_authority().id().find(&party_id.to_string()) else {
        return Ok(None);
    };
    if party.current_settlement_id.as_deref() != Some(&settlement.id) {
        return Ok(None);
    }
    if ctx
        .db
        .strategic_incident()
        .party_id()
        .filter(party_id)
        .any(|incident| incident.status == IncidentStatus::Pending)
    {
        return Ok(None);
    }
    if let Some(existing) = ctx
        .db
        .strategic_incident()
        .iter()
        .find(|incident| incident.source_id == source_id)
    {
        return Ok(Some(existing.id));
    }
    let incident_key = format!("incident:{}", source_id.value);
    let incident_id = IncidentId {
        value: incident_key.clone(),
    };
    let case_site_id = format!("case-site:{incident_key}");
    let enemy_count = living_party_member_ids(ctx, party_id).len().max(2) as i32;
    let site = CaseSiteAuthority {
        id_key: case_site_id.clone(),
        id: CaseSiteId::from(case_site_id.clone()),
        case_id: incident_id.value.clone(),
        origin_settlement_id: settlement.id.clone(),
        name: spec.title.into(),
        description: spec.description,
        scene_key: settlement.scene_key.clone(),
        longitude_e7: (settlement.coord_x * 10_000_000.0).round() as i32,
        latitude_e7: (settlement.coord_y * 10_000_000.0).round() as i32,
        coordinates_are_geographic: settlement.source_node_id.is_some(),
        distance_m: 0,
    };
    ctx.db.case_site_authority().insert(site.clone());
    let hostile_group_id = format!("hostile-group:{}", incident_id.value);
    let hostile_group = materialize_hostile_group(
        ctx,
        &hostile_group_id,
        &site,
        spec.enemy_type.into(),
        enemy_count as u32,
        spec.difficulty,
    )?;
    ctx.db.strategic_incident().insert(StrategicIncident {
        id_key: incident_id.value.clone(),
        id: incident_id.clone(),
        source_id,
        party_id: party_id.into(),
        settlement_id: settlement.id.clone(),
        instigator_id,
        kind: spec.kind,
        status: IncidentStatus::Pending,
        case_site_id: site.id.clone(),
        hostile_group_id: hostile_group.id,
        created_at_minute: crate::time::refresh_clock(ctx)?,
    });

    for member_id in living_party_member_ids(ctx, party_id) {
        if let Some(mut member) = ctx.db.character().id().find(member_id) {
            member.current_settlement_id = None;
            crate::investigation::set_character_case_site(
                ctx,
                member.id,
                Some(case_site_id.clone()),
            );
            ctx.db.character().id().update(member);
        }
    }
    party.current_settlement_id = None;
    party.current_case_site_id = Some(CaseSiteId::from(case_site_id));
    ctx.db.party_authority().id().update(party);
    Ok(Some(incident_id))
}

fn maybe_trigger_religious_incident(
    ctx: &ReducerContext,
    party_id: &str,
    settlement: &Settlement,
) -> Result<Option<IncidentId>, String> {
    if ctx
        .db
        .strategic_incident()
        .party_id()
        .filter(party_id)
        .any(|incident| {
            incident.kind == IncidentKind::Religious && incident.settlement_id == settlement.id
        })
    {
        return Ok(None);
    }
    let mut instigator = None;
    for member_id in living_party_member_ids(ctx, party_id) {
        crate::condition::initialize_character_condition(ctx, member_id)?;
        let religion = ctx
            .db
            .character_condition()
            .character_id()
            .find(member_id)
            .and_then(|condition| condition.religion_id);
        if religion
            .as_deref()
            .is_none_or(|faith| faith == settlement.religion_id)
        {
            continue;
        }
        let condition = crate::condition::refresh_character_strategic_condition(ctx, member_id)?;
        if instigator
            .as_ref()
            .is_none_or(|(_, fervor)| condition.fervor > *fervor)
        {
            instigator = Some((member_id, condition.fervor));
        }
    }
    let Some((instigator_id, instigator_fervor)) = instigator else {
        return Ok(None);
    };
    let roll = (ctx.random::<u64>() >> 40) as f32 / ((1_u32 << 24) as f32);
    if !fervor_event_occurs(instigator_fervor, roll) {
        return Ok(None);
    }
    let source_id = IncidentSourceId {
        value: format!("religious:{party_id}:{}", settlement.id),
    };
    create_strategic_incident(
        ctx,
        party_id,
        settlement,
        instigator_id,
        source_id,
        IncidentSpec {
            kind: IncidentKind::Religious,
            title: "A Quarrel at the Gate",
            description: format!(
                "At the gate of {}, a loud insult against the local faith has drawn an angry crowd. Combat is imminent, but the party can still withdraw and travel away.",
                settlement.name
            ),
            enemy_type: "angry_mob",
            difficulty: 1,
        },
    )
}

pub(crate) fn maybe_trigger_activity_incident(
    ctx: &ReducerContext,
    character_id: u64,
    risks: crate::time::ActivityRisks,
) -> Result<Option<IncidentId>, String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let Some(party_id) = character.party_id.as_deref() else {
        return Ok(None);
    };
    let Some(settlement_id) = character.current_settlement_id.as_ref() else {
        return Ok(None);
    };
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(settlement_id)
        .ok_or("Character's settlement not found")?;
    let occurrence_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |time| time.minutes);
    let entropy_id = format!(
        "activity-entropy:{party_id}:{}:{character_id}:{occurrence_minute}",
        settlement.id
    );
    let private_seed = if let Some(row) = ctx.db.activity_incident_entropy().id().find(&entropy_id)
    {
        row.seed
    } else {
        let seed = ctx.random::<u64>();
        ctx.db
            .activity_incident_entropy()
            .insert(ActivityIncidentEntropy {
                id: entropy_id,
                character_id,
                seed,
            });
        seed
    };
    let roll = |kind: &str| {
        let mut hasher = Sha256::new();
        hasher.update(b"adventuresim.activity-incident.v1\0");
        hasher.update(private_seed.to_le_bytes());
        hasher.update(kind.as_bytes());
        let digest = hasher.finalize();
        let sample = u32::from_le_bytes(digest[..4].try_into().unwrap());
        sample as f32 / u32::MAX as f32
    };
    let outcome = if fervor_event_occurs(risks.raiding_retaliation, roll("raiding")) {
        Some((
            "raiding",
            "Retaliation at Dawn",
            "The people raided from the surrounding countryside have tracked the party back to town. An armed band closes in; fight them or flee by road.",
            "armed_retainer",
            2,
        ))
    } else if fervor_event_occurs(risks.thievery_discovery, roll("thievery")) {
        Some((
            "thievery",
            "Caught Red-Handed",
            "A theft has been discovered and the watch has cornered the party near the market. Fight through them or abandon the settlement.",
            "town_watch",
            1,
        ))
    } else if fervor_event_occurs(risks.carousing_disorder, roll("carousing_disorder")) {
        Some((
            "carousing_disorder",
            "A Night Gone Wrong",
            "A drunken scandal has drawn the town watch. The party can answer for the disorder or flee the settlement.",
            "town_watch",
            1,
        ))
    } else {
        let (fame, infamy) = crate::reputation::local_reputation(ctx, character_id, &settlement.id);
        let has_charge =
            !crate::reputation::unsettled_local_offenses(ctx, character_id, &settlement.id)
                .is_empty();
        (has_charge && infamy > fame && infamy.saturating_sub(fame) >= 1_000).then_some((
            "authority_arrest",
            "Wanted by the Watch",
            "The local watch recognizes the party's wanted member and moves to make an arrest.",
            "town_watch",
            1,
        ))
    };
    let Some((kind, title, description, enemy_type, difficulty)) = outcome else {
        return Ok(None);
    };
    let (incident_kind, kind_key) = match kind {
        "raiding" => (IncidentKind::RaidingRetaliation, "raiding"),
        "thievery" => (IncidentKind::ThieveryDiscovery, "thievery"),
        "carousing_disorder" => (IncidentKind::CarousingDisorder, "carousing_disorder"),
        _ => (IncidentKind::AuthorityArrest, "authority_arrest"),
    };
    let source_id = if incident_kind == IncidentKind::AuthorityArrest {
        authority_arrest_source_id(
            party_id,
            &settlement.id,
            character_id,
            &crate::reputation::unsettled_local_offenses(ctx, character_id, &settlement.id),
        )
    } else {
        activity_incident_source_id(
            kind_key,
            party_id,
            &settlement.id,
            character_id,
            occurrence_minute,
        )
    };
    let incident = create_strategic_incident(
        ctx,
        party_id,
        &settlement,
        character_id,
        source_id.clone(),
        IncidentSpec {
            kind: incident_kind,
            title,
            description: description.into(),
            enemy_type,
            difficulty,
        },
    )?;
    if let Some(incident_id) = incident.as_ref()
        && incident_kind == IncidentKind::AuthorityArrest
    {
        if crate::reputation::snapshot_arrest_charges(
            ctx,
            &incident_id.value,
            character_id,
            &settlement.id,
        ) == 0
        {
            return Err("Authority arrest has no unsettled offense provenance".into());
        }
    } else if incident.is_some() {
        let infamy = match incident_kind {
            IncidentKind::RaidingRetaliation => 500,
            IncidentKind::ThieveryDiscovery => 300,
            IncidentKind::CarousingDisorder => 200,
            _ => 0,
        };
        crate::reputation::record_event(
            ctx,
            format!("incident-reputation:{}", source_id.value),
            character_id,
            &settlement.id,
            kind_key,
            &source_id.value,
            0,
            infamy,
            occurrence_minute,
        )?;
        crate::reputation::record_discovered_offense(
            ctx,
            format!("offense:{}", source_id.value),
            character_id,
            &settlement.id,
            kind_key,
            u8::try_from(difficulty).unwrap_or(u8::MAX),
            occurrence_minute,
        );
    }
    Ok(incident)
}

/// Surrender to a local arrest and pay the bounded fine atomically. An
/// insufficient balance leaves both money and incident untouched.
#[reducer]
pub fn surrender_to_authority(
    ctx: &ReducerContext,
    character_id: u64,
    incident_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    let incident = ctx
        .db
        .strategic_incident()
        .id_key()
        .find(&incident_id)
        .ok_or("Authority incident not found")?;
    if incident.status != IncidentStatus::Pending
        || incident.kind != IncidentKind::AuthorityArrest
        || incident.instigator_id != character_id
    {
        return Err("This character cannot surrender for that incident".into());
    }
    let offenses = crate::reputation::unsettled_arrest_charges(
        ctx,
        &incident.id.value,
        character_id,
        &incident.settlement_id,
    );
    let charges = offenses
        .iter()
        .map(|offense| (offense.severity, offense.settled))
        .collect::<Vec<_>>();
    let fine = adventuresim_core::reputation::authority_fine_for_charges(&charges)
        .ok_or("This arrest has no unsettled charges")?;
    crate::item::consume_personal_currency(ctx, character_id, fine)?;
    crate::reputation::settle_offenses(ctx, offenses);
    finish_strategic_incident(ctx, &incident.id, IncidentStatus::Resolved)
}

fn finish_strategic_incident(
    ctx: &ReducerContext,
    incident_id: &IncidentId,
    status: IncidentStatus,
) -> Result<(), String> {
    let Some(mut incident) = ctx
        .db
        .strategic_incident()
        .id_key()
        .find(&incident_id.value)
    else {
        return Ok(());
    };
    if incident.status != IncidentStatus::Pending {
        return Ok(());
    }
    incident.status = status;
    ctx.db.strategic_incident().id_key().update(incident);
    Ok(())
}

pub(crate) fn finish_incident_for_hostile_group(
    ctx: &ReducerContext,
    hostile_group_id: &str,
) -> Result<bool, String> {
    let incident = ctx.db.strategic_incident().iter().find(|incident| {
        incident_group_matches(
            incident.status,
            &incident.hostile_group_id,
            hostile_group_id,
        )
    });
    let Some(incident) = incident else {
        return Ok(false);
    };
    if incident.kind == IncidentKind::AuthorityArrest {
        let minute = ctx
            .db
            .character_time()
            .character_id()
            .find(incident.instigator_id)
            .map_or(0, |time| time.minutes);
        crate::reputation::record_event(
            ctx,
            format!("resist-authority:{}", incident.id.value),
            incident.instigator_id,
            &incident.settlement_id,
            "resisting_authority",
            &incident.id.value,
            0,
            500,
            minute,
        )?;
        crate::reputation::record_discovered_offense(
            ctx,
            format!("offense:resist-authority:{}", incident.id.value),
            incident.instigator_id,
            &incident.settlement_id,
            "resisting_authority",
            3,
            minute,
        );
    }
    finish_strategic_incident(ctx, &incident.id, IncidentStatus::Resolved)?;
    Ok(true)
}

fn incident_group_matches(
    status: IncidentStatus,
    incident_hostile_group_id: &str,
    completed_hostile_group_id: &str,
) -> bool {
    status == IncidentStatus::Pending && incident_hostile_group_id == completed_hostile_group_id
}

fn activity_incident_source_id(
    kind: &str,
    party_id: &str,
    settlement_id: &str,
    character_id: u64,
    occurrence_minute: u64,
) -> IncidentSourceId {
    IncidentSourceId {
        value: format!(
            "activity:{kind}:{party_id}:{settlement_id}:{character_id}:{occurrence_minute}"
        ),
    }
}

fn authority_arrest_source_id(
    party_id: &str,
    settlement_id: &str,
    character_id: u64,
    offenses: &[crate::reputation::DiscoveredOffense],
) -> IncidentSourceId {
    let mut hasher = Sha256::new();
    hasher.update(b"adventuresim.authority-arrest.v1\0");
    for offense in offenses {
        hasher.update(offense.id.as_bytes());
        hasher.update([0]);
    }
    let fingerprint = format!("{:x}", hasher.finalize());
    IncidentSourceId {
        value: format!(
            "authority-arrest:{party_id}:{settlement_id}:{character_id}:{}",
            &fingerprint[..16]
        ),
    }
}

pub(crate) fn delete_activity_incident_entropy(ctx: &ReducerContext, character_id: u64) {
    for row in ctx
        .db
        .activity_incident_entropy()
        .character_id()
        .filter(character_id)
        .collect::<Vec<_>>()
    {
        ctx.db.activity_incident_entropy().id().delete(&row.id);
    }
}
