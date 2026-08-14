struct IncidentSpec<'a> {
    kind: IncidentKind,
    title: &'a str,
    description: String,
    enemy_type: &'a str,
    difficulty: i32,
}

/// Minimal trusted-gateway action surface for a pending local arrest. The
/// private incident kind/source, charge identities, offense kinds, severities,
/// and hostile-group authority never cross this boundary.
#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
pub struct BackendAuthorityArrestAction {
    pub action_token: String,
    pub party_id: String,
    pub case_site_id: String,
    pub origin_settlement_id: String,
    pub instigator_id: u64,
    pub fine: u64,
    pub affordable: bool,
}

#[view(accessor = backend_authority_arrest_actions, public)]
pub fn backend_authority_arrest_actions(
    ctx: &ViewContext,
) -> Vec<BackendAuthorityArrestAction> {
    use crate::{
        character::character__view as _,
        item::{inventory_item__view as _, item__view as _},
        reputation::{authority_arrest_charge__view as _, discovered_offense__view as _},
    };

    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    let mut actions = Vec::new();
    for party in ctx.db.party_authority().gateway_bucket().filter(0u8) {
        for incident in ctx
            .db
            .strategic_incident()
            .party_id()
            .filter(&party.id)
            .filter(|incident| {
                incident.kind == IncidentKind::AuthorityArrest
                    && incident.status == IncidentStatus::Pending
            })
        {
            if party.current_case_site_id.as_ref() != Some(&incident.case_site_id)
                || !ctx
                    .db
                    .party_member()
                    .party_id()
                    .filter(&incident.party_id)
                    .any(|member| member.character_id == incident.instigator_id)
            {
                continue;
            }
            let Some(character) = ctx.db.character().id().find(incident.instigator_id) else {
                continue;
            };
            if !character.alive || character.party_id.as_deref() != Some(&incident.party_id) {
                continue;
            }
            let charges = ctx
                .db
                .authority_arrest_charge()
                .incident_id()
                .filter(&incident.id.value)
                .filter(|charge| {
                    charge.character_id == incident.instigator_id
                        && charge.settlement_id == incident.settlement_id
                })
                .filter_map(|charge| ctx.db.discovered_offense().id().find(&charge.offense_id))
                .filter(|offense| {
                    offense.character_id == incident.instigator_id
                        && offense.settlement_id == incident.settlement_id
                        && !offense.settled
                })
                .map(|offense| (offense.severity, offense.settled))
                .collect::<Vec<_>>();
            let Some(fine) =
                adventuresim_core::reputation::authority_fine_for_charges(&charges)
            else {
                continue;
            };
            let funds = ctx
                .db
                .inventory_item()
                .character_id()
                .filter(incident.instigator_id)
                .filter(|stack| {
                    ctx.db.item().id().find(&stack.item_id).is_some_and(|item| {
                        item.kind == crate::item::ItemKind::Currency
                    })
                })
                .map(|stack| u64::from(stack.quantity))
                .sum::<u64>();
            actions.push(BackendAuthorityArrestAction {
                action_token: incident.action_token,
                party_id: incident.party_id,
                case_site_id: incident.case_site_id.value,
                origin_settlement_id: incident.settlement_id,
                instigator_id: incident.instigator_id,
                fine,
                affordable: funds >= fine,
            });
        }
    }
    actions.sort_by(|left, right| {
        (&left.party_id, &left.case_site_id, &left.action_token).cmp(&(
            &right.party_id,
            &right.case_site_id,
            &right.action_token,
        ))
    });
    actions
}

fn create_strategic_incident(
    ctx: &ReducerContext,
    party_id: &str,
    settlement: &Settlement,
    instigator_id: u64,
    current_case_site_id: Option<&str>,
    source_id: IncidentSourceId,
    spec: IncidentSpec<'_>,
) -> Result<Option<IncidentId>, String> {
    parse_threat(spec.enemy_type)?;
    let Some(mut party) = ctx.db.party_authority().id().find(party_id.to_string()) else {
        return Ok(None);
    };
    let at_expected_location = current_case_site_id.map_or_else(
        || party.current_settlement_id.as_deref() == Some(&settlement.id),
        |case_site_id| {
            party
                .current_case_site_id
                .as_ref()
                .is_some_and(|id| id.value == case_site_id)
        },
    );
    if !at_expected_location {
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
    let current_site = current_case_site_id
        .and_then(|id| ctx.db.case_site_authority().id_key().find(id.to_string()));
    let (scene_key, longitude_e7, latitude_e7, coordinates_are_geographic, distance_m) =
        current_site.map_or_else(
            || {
                (
                    settlement.scene_key.clone(),
                    (settlement.coord_x * 10_000_000.0).round() as i32,
                    (settlement.coord_y * 10_000_000.0).round() as i32,
                    settlement.source_node_id.is_some(),
                    0,
                )
            },
            |site| {
                (
                    site.scene_key,
                    site.longitude_e7,
                    site.latitude_e7,
                    site.coordinates_are_geographic,
                    site.distance_m,
                )
            },
        );
    let site = CaseSiteAuthority {
        id_key: case_site_id.clone(),
        id: CaseSiteId::from(case_site_id.clone()),
        case_id: incident_id.value.clone(),
        origin_settlement_id: settlement.id.clone(),
        name: spec.title.into(),
        description: spec.description,
        scene_key,
        longitude_e7,
        latitude_e7,
        coordinates_are_geographic,
        distance_m,
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
        action_token: format!("{:016x}{:016x}", ctx.random::<u64>(), ctx.random::<u64>()),
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
            )?;
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
        None,
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
    let current_case_site_id =
        crate::investigation::character_case_site_id(ctx, character_id);
    let settlement_id = if let Some(settlement_id) = character.current_settlement_id.as_ref() {
        settlement_id.clone()
    } else if let Some(case_site_id) = current_case_site_id.as_ref() {
        ctx.db
            .case_site_authority()
            .id_key()
            .find(case_site_id)
            .ok_or("Character's case site not found")?
            .origin_settlement_id
    } else {
        return Ok(None);
    };
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(&settlement_id)
        .ok_or("Activity origin settlement not found")?;
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
            "The people raided in the surrounding countryside have rallied an armed band. They close in on the party; fight them or flee by road.",
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
        (current_case_site_id.is_none()
            && has_charge
            && infamy > fame
            && infamy.saturating_sub(fame) >= 1_000).then_some((
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
        current_case_site_id.as_deref(),
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
    action_token: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    let incident = ctx
        .db
        .strategic_incident()
        .action_token()
        .find(&action_token)
        .ok_or("Authority incident not found")?;
    if incident.status != IncidentStatus::Pending
        || incident.kind != IncidentKind::AuthorityArrest
        || incident.instigator_id != character_id
    {
        return Err("This character cannot surrender for that incident".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character
        .party_id
        .as_deref()
        .ok_or("Character is not in the incident party")?;
    let membership_matches = ctx
        .db
        .party_member()
        .party_id()
        .filter(party_id)
        .any(|membership| membership.character_id == character_id);
    let party_site_matches = ctx
        .db
        .party_authority()
        .id()
        .find(party_id.to_owned())
        .and_then(|party| party.current_case_site_id)
        .is_some_and(|site| site == incident.case_site_id);
    if incident.party_id != party_id || !membership_matches || !party_site_matches {
        return Err("Character is not at the incident site with its owning party".into());
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
