/// Converts a trusted mission outcome into a typed strategic fact. This is the
/// only battle-to-case seam: tactical code cannot resolve a case or pay a
/// contract directly.
fn ingest_hostile_group_defeat_fact(
    ctx: &ReducerContext,
    outcome_source_id: &str,
    party_id: &str,
    group: &HostileGroupAuthority,
    count: u32,
) -> Result<(), String> {
    let site = ctx
        .db
        .case_site_authority()
        .id_key()
        .find(&group.case_site_id.value)
        .ok_or("Hostile group has no case site")?;
    let Some(_) = ctx.db.case_authority().id().find(&site.case_id) else {
        // Incidents and random encounters intentionally have no case.
        return Ok(());
    };
    ingest_case_outcome_fact(
        ctx,
        outcome_source_id,
        &site.case_id,
        party_id,
        adventuresim_core::case::OutcomeFactKind::HostilesDefeated {
            hostile_group_id: group.id.clone(),
            count,
        },
    )
}

fn custody_object(
    kind: CustodyObjectKind,
    object_id: &str,
) -> Result<adventuresim_core::case::CustodyObject, String> {
    match kind {
        CustodyObjectKind::Asset => adventuresim_core::case::AssetId::new(object_id)
            .map(adventuresim_core::case::CustodyObject::Asset)
            .map_err(|_| "Custody asset ID is invalid".into()),
        CustodyObjectKind::Subject => adventuresim_core::case::SubjectId::new(object_id)
            .map(adventuresim_core::case::CustodyObject::Subject)
            .map_err(|_| "Custody subject ID is invalid".into()),
    }
}

fn custody_holder(
    kind: CustodyHolderKind,
    holder_id: &str,
) -> Result<adventuresim_core::case::CustodyHolder, String> {
    if holder_id.len() > 160 {
        return Err("Custody holder ID is too long".into());
    }
    match kind {
        CustodyHolderKind::Site if !holder_id.is_empty() => Ok(
            adventuresim_core::case::CustodyHolder::Site(holder_id.into()),
        ),
        CustodyHolderKind::Party if !holder_id.is_empty() => Ok(
            adventuresim_core::case::CustodyHolder::Party(holder_id.into()),
        ),
        CustodyHolderKind::Character => holder_id
            .parse()
            .map(adventuresim_core::case::CustodyHolder::Character)
            .map_err(|_| "Custody character ID is invalid".into()),
        CustodyHolderKind::Npc if !holder_id.is_empty() => Ok(
            adventuresim_core::case::CustodyHolder::Npc(holder_id.into()),
        ),
        CustodyHolderKind::Destroyed if holder_id.is_empty() => {
            Ok(adventuresim_core::case::CustodyHolder::Destroyed)
        }
        CustodyHolderKind::Released if holder_id.is_empty() => {
            Ok(adventuresim_core::case::CustodyHolder::Released)
        }
        _ => Err("Custody holder ID does not match its typed holder".into()),
    }
}

/// Sole typed custody transition. Domain adapters provide the corresponding
/// outcome fact only after the core custody state machine accepts the move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CustodyPartyDispatch {
    Unattributed,
    OrdinaryPartyContinuity,
    ResidentNpcAuthority,
}

fn custody_party_dispatch(
    party_id: &str,
    ordinary_party_exists: bool,
    resident_npc_party_exists: bool,
) -> Result<CustodyPartyDispatch, String> {
    match (
        party_id.is_empty(),
        ordinary_party_exists,
        resident_npc_party_exists,
    ) {
        (true, false, false) => Ok(CustodyPartyDispatch::Unattributed),
        (true, _, _) => {
            Err("Empty custody outcome party ID unexpectedly has party authority".into())
        }
        (false, true, false) => Ok(CustodyPartyDispatch::OrdinaryPartyContinuity),
        (false, false, true) => Ok(CustodyPartyDispatch::ResidentNpcAuthority),
        (false, false, false) => Err(format!(
            "Custody outcome party ID is not an ordinary or resident NPC party: {party_id}"
        )),
        (false, true, true) => Err(format!(
            "Custody outcome party ID is ambiguous across ordinary and resident NPC authority: {party_id}"
        )),
    }
}

fn apply_custody_party_continuity(
    dispatch: CustodyPartyDispatch,
    ordinary_party_continuity: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if dispatch == CustodyPartyDispatch::OrdinaryPartyContinuity {
        ordinary_party_continuity()
    } else {
        Ok(())
    }
}

fn validate_custody_fact_retry_attribution(
    expected_case_id: &str,
    expected_party_id: &str,
    receipt_attribution: Option<(&str, &str)>,
) -> Result<(), String> {
    let Some((receipt_case_id, receipt_party_id)) = receipt_attribution else {
        return Err("Exact custody retry is missing paired outcome fact attribution".into());
    };
    if receipt_case_id != expected_case_id || receipt_party_id != expected_party_id {
        return Err("Conflicting custody retry outcome fact attribution".into());
    }
    Ok(())
}

#[cfg(test)]
mod custody_party_dispatch_tests {
    use super::{
        CustodyPartyDispatch, apply_custody_party_continuity, custody_party_dispatch,
        validate_custody_fact_retry_attribution,
    };
    use std::cell::Cell;

    #[test]
    fn ordinary_party_dispatch_preserves_player_continuity() {
        let dispatch = custody_party_dispatch("party:player", true, false).unwrap();
        assert_eq!(dispatch, CustodyPartyDispatch::OrdinaryPartyContinuity);

        let continuity_ran = Cell::new(false);
        apply_custody_party_continuity(dispatch, || {
            continuity_ran.set(true);
            Ok(())
        })
        .unwrap();
        assert!(continuity_ran.get());
    }

    #[test]
    fn resident_npc_custody_dispatch_cannot_run_player_continuity() {
        let dispatch = custody_party_dispatch("npc-party:resident-company", false, true).unwrap();
        assert_eq!(dispatch, CustodyPartyDispatch::ResidentNpcAuthority);

        apply_custody_party_continuity(dispatch, || -> Result<(), String> {
            panic!("resident NPC Retrieve/Return path reached player continuity")
        })
        .unwrap();
    }

    #[test]
    fn unknown_and_ambiguous_nonempty_party_ids_fail_closed() {
        let unknown = custody_party_dispatch("party:unknown", false, false).unwrap_err();
        assert!(unknown.contains("not an ordinary or resident NPC party"));
        assert!(unknown.contains("party:unknown"));

        let ambiguous = custody_party_dispatch("party:ambiguous", true, true).unwrap_err();
        assert!(ambiguous.contains("ambiguous"));
        assert!(ambiguous.contains("party:ambiguous"));
    }

    #[test]
    fn empty_party_id_retains_unattributed_custody_seeding() {
        assert_eq!(
            custody_party_dispatch("", false, false).unwrap(),
            CustodyPartyDispatch::Unattributed
        );
    }

    #[test]
    fn exact_fact_retry_requires_matching_durable_case_and_party_attribution() {
        validate_custody_fact_retry_attribution(
            "case:test",
            "npc-party:retired",
            Some(("case:test", "npc-party:retired")),
        )
        .unwrap();

        let missing =
            validate_custody_fact_retry_attribution("case:test", "npc-party:retired", None)
                .unwrap_err();
        assert!(missing.contains("missing paired outcome fact attribution"));

        for attribution in [
            ("case:other", "npc-party:retired"),
            ("case:test", "npc-party:other"),
        ] {
            assert!(
                validate_custody_fact_retry_attribution(
                    "case:test",
                    "npc-party:retired",
                    Some(attribution),
                )
                .unwrap_err()
                .contains("Conflicting custody retry outcome fact attribution")
            );
        }
    }
}

fn transition_case_custody(
    ctx: &ReducerContext,
    source_id: &str,
    case_id: &str,
    party_id: &str,
    object_kind: CustodyObjectKind,
    object_id: &str,
    holder_kind: CustodyHolderKind,
    holder_id: &str,
    version: u32,
    fact: Option<adventuresim_core::case::OutcomeFactKind>,
) -> Result<bool, String> {
    let object = custody_object(object_kind, object_id)?;
    let next = adventuresim_core::case::CustodyRecord {
        case_id: adventuresim_core::case::CaseId::new(case_id)
            .map_err(|_| "Custody case ID is invalid")?,
        object: object.clone(),
        holder: custody_holder(holder_kind, holder_id)?,
        version,
        source_id: source_id.to_string(),
    };
    if let Some(existing) = ctx
        .db
        .case_custody()
        .source_id()
        .find(&source_id.to_string())
    {
        return if existing.case_id == case_id
            && existing.object_id == object_id
            && existing.object_kind == object_kind
            && existing.holder_kind == holder_kind
            && existing.holder_id == holder_id
            && existing.version == version
        {
            if fact.is_some() {
                let fact_source_id = format!("custody:{source_id}");
                let paired_fact = ctx.db.case_outcome_fact().source_id().find(&fact_source_id);
                validate_custody_fact_retry_attribution(
                    case_id,
                    party_id,
                    paired_fact
                        .as_ref()
                        .map(|receipt| (receipt.case_id.as_str(), receipt.party_id.as_str())),
                )?;
            }
            Ok(false)
        } else {
            Err("Conflicting retry for custody source".into())
        };
    }
    let party_key = party_id.to_string();
    let party_dispatch = custody_party_dispatch(
        party_id,
        !party_id.is_empty() && ctx.db.party_authority().id().find(&party_key).is_some(),
        !party_id.is_empty()
            && ctx
                .db
                .npc_adventuring_party_authority()
                .id()
                .find(&party_key)
                .is_some(),
    )?;
    let mut records = BTreeMap::new();
    if let Some(current) = ctx
        .db
        .case_custody()
        .object_id()
        .find(&object_id.to_string())
    {
        records.insert(
            custody_object(current.object_kind, &current.object_id)?,
            adventuresim_core::case::CustodyRecord {
                case_id: adventuresim_core::case::CaseId::new(current.case_id)
                    .map_err(|_| "Stored custody case ID is invalid")?,
                object: object.clone(),
                holder: custody_holder(current.holder_kind, &current.holder_id)?,
                version: current.version,
                source_id: current.source_id,
            },
        );
    }
    if !adventuresim_core::case::apply_custody(&mut records, next).map_err(str::to_string)? {
        return Ok(false);
    }
    let row = CaseCustody {
        object_id: object_id.to_string(),
        case_id: case_id.to_string(),
        object_kind,
        holder_kind,
        holder_id: holder_id.to_string(),
        version,
        source_id: source_id.to_string(),
    };
    if ctx
        .db
        .case_custody()
        .object_id()
        .find(&row.object_id)
        .is_some()
    {
        ctx.db.case_custody().object_id().update(row);
    } else {
        ctx.db.case_custody().insert(row);
    }
    if let Some(fact) = fact {
        ingest_case_outcome_fact(
            ctx,
            &format!("custody:{source_id}"),
            case_id,
            party_id,
            fact,
        )?;
    }
    apply_custody_party_continuity(party_dispatch, || {
        ensure_objective_continuity_guards(ctx, party_id, case_id)?;
        reconcile_party_objective_continuity(ctx, party_id)
    })?;
    if matches!(
        holder_kind,
        CustodyHolderKind::Destroyed | CustodyHolderKind::Released
    ) {
        emit_terminal_custody_impossibility(
            ctx,
            source_id,
            case_id,
            party_id,
            object_kind,
            object_id,
            holder_kind,
        )?;
    }
    Ok(true)
}

fn seed_case_custody(
    ctx: &ReducerContext,
    case_id: &str,
    expression: &adventuresim_core::case::ObjectiveExpression,
    authored_custody: &[(String, adventuresim_core::quest_generation::SiteId)],
) -> Result<(), String> {
    use adventuresim_core::case::ObjectiveRequirement as R;
    let mut kinds = BTreeMap::new();
    for requirement in expression
        .alternatives
        .iter()
        .flat_map(|path| &path.objectives)
        .map(|objective| &objective.requirement)
    {
        let (kind, object_id) = match requirement {
            R::Retrieve { asset_id }
            | R::Return { asset_id, .. }
            | R::Exchange { asset_id, .. } => (CustodyObjectKind::Asset, asset_id.as_str()),
            R::Capture { subject_id }
            | R::Rescue { subject_id }
            | R::EscortTo { subject_id, .. }
            | R::Protect { subject_id, .. }
            | R::Release { subject_id } => (CustodyObjectKind::Subject, subject_id.as_str()),
            _ => continue,
        };
        if let Some(existing) = kinds.insert(object_id.to_owned(), kind)
            && existing != kind
        {
            return Err("Generated custody object has ambiguous objective kind".into());
        }
    }
    if kinds.len() != authored_custody.len() {
        return Err("Generated custody does not exactly cover objective custody objects".into());
    }
    for (object_id, site_id) in authored_custody {
        let kind = kinds
            .get(object_id)
            .copied()
            .ok_or("Generated custody object has no typed objective leaf")?;
        transition_case_custody(
            ctx,
            &format!("spawn:{case_id}:{object_id}"),
            case_id,
            "",
            kind,
            object_id,
            CustodyHolderKind::Site,
            &site_id.0,
            0,
            None,
        )?;
    }
    Ok(())
}

fn party_strategic_minute(ctx: &ReducerContext, party_id: &str) -> Result<u64, String> {
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .ok_or("Party not found")?;
    Ok(ctx
        .db
        .character_time()
        .character_id()
        .find(party.leader_id)
        .map_or(0, |time| time.minutes))
}

fn open_case_expression(
    ctx: &ReducerContext,
    case_id: &str,
) -> Result<Option<adventuresim_core::case::ObjectiveExpression>, String> {
    let Some(case) = ctx.db.case_authority().id().find(&case_id.to_string()) else {
        return Ok(None);
    };
    if case.resolution_status != CaseResolutionStatus::Open {
        return Ok(None);
    }
    serde_json::from_str(&case.objective_expression_json)
        .map(Some)
        .map_err(|_| "Case objective authority is invalid".into())
}

fn ensure_objective_continuity_guards(
    ctx: &ReducerContext,
    party_id: &str,
    case_id: &str,
) -> Result<(), String> {
    let Some(expression) = open_case_expression(ctx, case_id)? else {
        return Ok(());
    };
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .ok_or("Party not found")?;
    let now = party_strategic_minute(ctx, party_id)?;
    use adventuresim_core::case::ObjectiveRequirement as R;
    for objective in expression
        .alternatives
        .iter()
        .flat_map(|path| &path.objectives)
    {
        let (kind, site_id, subject_id, custody_version, through, valid_now) =
            match &objective.requirement {
                R::SurviveWindow {
                    site_id,
                    through_minute,
                } => (
                    ObjectiveContinuityKind::SurviveAtSite,
                    site_id.clone(),
                    String::new(),
                    None,
                    *through_minute,
                    party.current_case_site_id.as_deref() == Some(site_id),
                ),
                R::Protect {
                    subject_id,
                    through_minute,
                } => {
                    let custody = ctx
                        .db
                        .case_custody()
                        .object_id()
                        .find(&subject_id.as_str().to_string());
                    let valid = custody.as_ref().is_some_and(|row| {
                        row.case_id == case_id
                            && row.holder_kind == CustodyHolderKind::Party
                            && row.holder_id == party_id
                    });
                    (
                        ObjectiveContinuityKind::ProtectSubject,
                        String::new(),
                        subject_id.as_str().to_string(),
                        custody.map(|row| row.version),
                        *through_minute,
                        valid,
                    )
                }
                _ => continue,
            };
        if !valid_now || now > through {
            continue;
        }
        let has_active = ctx
            .db
            .objective_continuity_guard()
            .party_id()
            .filter(party_id)
            .any(|guard| {
                guard.case_id == case_id
                    && guard.objective_id == objective.id.as_str()
                    && guard.broken_at_minute.is_none()
                    && !guard.completed
            });
        if has_active {
            continue;
        }
        let id = format!(
            "continuity:{case_id}:{}:{party_id}:{now}",
            objective.id.as_str()
        );
        ctx.db
            .objective_continuity_guard()
            .insert(ObjectiveContinuityGuard {
                id,
                party_id: party_id.to_string(),
                case_id: case_id.to_string(),
                objective_id: objective.id.as_str().to_string(),
                kind,
                site_id,
                subject_id,
                custody_version,
                started_at_minute: now,
                through_minute: through,
                broken_at_minute: None,
                completed: false,
            });
    }
    Ok(())
}

pub(crate) fn reconcile_party_objective_continuity(
    ctx: &ReducerContext,
    party_id: &str,
) -> Result<(), String> {
    let Some(party) = ctx.db.party_authority().id().find(&party_id.to_string()) else {
        return Ok(());
    };
    let now = party_strategic_minute(ctx, party_id)?;
    let mut touched_cases = HashSet::new();
    let guards: Vec<_> = ctx
        .db
        .objective_continuity_guard()
        .party_id()
        .filter(party_id)
        .filter(|guard| guard.broken_at_minute.is_none() && !guard.completed)
        .collect();
    for mut guard in guards {
        touched_cases.insert(guard.case_id.clone());
        let valid = match guard.kind {
            ObjectiveContinuityKind::SurviveAtSite => {
                party.current_case_site_id.as_deref() == Some(&guard.site_id)
            }
            ObjectiveContinuityKind::ProtectSubject => ctx
                .db
                .case_custody()
                .object_id()
                .find(&guard.subject_id)
                .is_some_and(|custody| {
                    custody.case_id == guard.case_id
                        && custody.holder_kind == CustodyHolderKind::Party
                        && custody.holder_id == party_id
                        && Some(custody.version) == guard.custody_version
                }),
        };
        if !valid {
            guard.broken_at_minute = Some(now);
            ctx.db.objective_continuity_guard().id().update(guard);
            continue;
        }
        if now < guard.through_minute {
            continue;
        }
        let Some(expression) = open_case_expression(ctx, &guard.case_id)? else {
            continue;
        };
        let objective = expression
            .alternatives
            .iter()
            .flat_map(|path| &path.objectives)
            .find(|objective| objective.id.as_str() == guard.objective_id)
            .ok_or("Continuity objective no longer exists")?;
        use adventuresim_core::case::{ObjectiveRequirement as R, OutcomeFactKind as F};
        let fact = match (&guard.kind, &objective.requirement) {
            (
                ObjectiveContinuityKind::SurviveAtSite,
                R::SurviveWindow {
                    site_id,
                    through_minute,
                },
            ) if site_id == &guard.site_id && *through_minute == guard.through_minute => {
                F::WindowSurvived {
                    site_id: site_id.clone(),
                    through_minute: *through_minute,
                }
            }
            (
                ObjectiveContinuityKind::ProtectSubject,
                R::Protect {
                    subject_id,
                    through_minute,
                },
            ) if subject_id.as_str() == guard.subject_id
                && *through_minute == guard.through_minute =>
            {
                F::SubjectProtected {
                    subject_id: subject_id.clone(),
                    through_minute: *through_minute,
                }
            }
            _ => return Err("Continuity guard no longer matches objective authority".into()),
        };
        ingest_case_outcome_fact(
            ctx,
            &format!("timed-objective:{}", guard.id),
            &guard.case_id,
            party_id,
            fact,
        )?;
        guard.completed = true;
        ctx.db.objective_continuity_guard().id().update(guard);
    }
    for case_id in touched_cases {
        ensure_objective_continuity_guards(ctx, party_id, &case_id)?;
    }
    Ok(())
}

fn break_party_objective_continuity(ctx: &ReducerContext, party_id: &str) -> Result<(), String> {
    let now = party_strategic_minute(ctx, party_id)?;
    let guards: Vec<_> = ctx
        .db
        .objective_continuity_guard()
        .party_id()
        .filter(party_id)
        .filter(|guard| guard.broken_at_minute.is_none() && !guard.completed)
        .collect();
    for mut guard in guards {
        guard.broken_at_minute = Some(now);
        ctx.db.objective_continuity_guard().id().update(guard);
    }
    Ok(())
}

fn commit_case_site_arrival_objectives(
    ctx: &ReducerContext,
    party_id: &str,
    site: &CaseSiteAuthority,
) -> Result<(), String> {
    let Some(expression) = open_case_expression(ctx, &site.case_id)? else {
        return Ok(());
    };
    use adventuresim_core::case::{ObjectiveRequirement as R, OutcomeFactKind as F};
    for objective in expression
        .alternatives
        .iter()
        .flat_map(|path| &path.objectives)
    {
        let R::EscortTo {
            subject_id,
            site_id,
        } = &objective.requirement
        else {
            continue;
        };
        if site_id != &site.id.value {
            continue;
        }
        let current = ctx
            .db
            .case_custody()
            .object_id()
            .find(&subject_id.as_str().to_string())
            .ok_or("Escorted subject has no custody authority")?;
        if current.case_id != site.case_id
            || current.holder_kind != CustodyHolderKind::Party
            || current.holder_id != party_id
        {
            continue;
        }
        transition_case_custody(
            ctx,
            &format!(
                "arrival:{}:{party_id}:{}:{}",
                site.case_id,
                site.id.value,
                objective.id.as_str()
            ),
            &site.case_id,
            party_id,
            CustodyObjectKind::Subject,
            subject_id.as_str(),
            CustodyHolderKind::Site,
            &site.id.value,
            current.version.saturating_add(1),
            Some(F::SubjectEscorted {
                subject_id: subject_id.clone(),
                site_id: site_id.clone(),
            }),
        )?;
    }
    ensure_objective_continuity_guards(ctx, party_id, &site.case_id)?;
    reconcile_party_objective_continuity(ctx, party_id)
}

fn emit_terminal_custody_impossibility(
    ctx: &ReducerContext,
    source_id: &str,
    case_id: &str,
    party_id: &str,
    object_kind: CustodyObjectKind,
    object_id: &str,
    terminal_holder: CustodyHolderKind,
) -> Result<(), String> {
    let Some(expression) = open_case_expression(ctx, case_id)? else {
        return Ok(());
    };
    use adventuresim_core::case::{ObjectiveRequirement as R, OutcomeFactKind as F};
    for objective in expression
        .alternatives
        .iter()
        .flat_map(|path| &path.objectives)
    {
        let affected = match (&objective.requirement, object_kind, terminal_holder) {
            (R::Retrieve { asset_id }, CustodyObjectKind::Asset, CustodyHolderKind::Destroyed)
            | (
                R::Return { asset_id, .. },
                CustodyObjectKind::Asset,
                CustodyHolderKind::Destroyed,
            )
            | (
                R::Exchange { asset_id, .. },
                CustodyObjectKind::Asset,
                CustodyHolderKind::Destroyed,
            ) => asset_id.as_str() == object_id,
            (R::Capture { subject_id }, CustodyObjectKind::Subject, _)
            | (R::Rescue { subject_id }, CustodyObjectKind::Subject, _)
            | (R::EscortTo { subject_id, .. }, CustodyObjectKind::Subject, _)
            | (R::Protect { subject_id, .. }, CustodyObjectKind::Subject, _) => {
                subject_id.as_str() == object_id
            }
            (
                R::Release { subject_id },
                CustodyObjectKind::Subject,
                CustodyHolderKind::Destroyed,
            ) => subject_id.as_str() == object_id,
            _ => false,
        };
        if !affected {
            continue;
        }
        if ctx
            .db
            .case_authority()
            .id()
            .find(&case_id.to_string())
            .is_none_or(|case| case.resolution_status != CaseResolutionStatus::Open)
        {
            break;
        }
        ingest_case_outcome_fact(
            ctx,
            &format!("{source_id}:impossible:{}", objective.id.as_str()),
            case_id,
            party_id,
            F::ObjectiveImpossible {
                objective_id: objective.id.clone(),
            },
        )?;
    }
    Ok(())
}

pub(crate) fn record_asset_retrieved(
    ctx: &ReducerContext,
    source_id: &str,
    case_id: &str,
    party_id: &str,
    asset_id: &str,
    version: u32,
) -> Result<bool, String> {
    let asset = adventuresim_core::case::AssetId::new(asset_id)
        .map_err(|_| "Custody asset ID is invalid")?;
    transition_case_custody(
        ctx,
        source_id,
        case_id,
        party_id,
        CustodyObjectKind::Asset,
        asset_id,
        CustodyHolderKind::Party,
        party_id,
        version,
        Some(adventuresim_core::case::OutcomeFactKind::AssetRetrieved { asset_id: asset }),
    )
}

pub(crate) fn record_asset_returned_or_exchanged(
    ctx: &ReducerContext,
    source_id: &str,
    case_id: &str,
    party_id: &str,
    asset_id: &str,
    recipient_id: &str,
    version: u32,
    exchange: bool,
) -> Result<bool, String> {
    let asset = adventuresim_core::case::AssetId::new(asset_id)
        .map_err(|_| "Custody asset ID is invalid")?;
    let fact = if exchange {
        adventuresim_core::case::OutcomeFactKind::AssetExchanged {
            asset_id: asset,
            recipient_id: recipient_id.into(),
        }
    } else {
        adventuresim_core::case::OutcomeFactKind::AssetReturned {
            asset_id: asset,
            custodian_id: recipient_id.into(),
        }
    };
    transition_case_custody(
        ctx,
        source_id,
        case_id,
        party_id,
        CustodyObjectKind::Asset,
        asset_id,
        CustodyHolderKind::Npc,
        recipient_id,
        version,
        Some(fact),
    )
}

pub(crate) fn record_subject_rescued_or_released(
    ctx: &ReducerContext,
    source_id: &str,
    case_id: &str,
    party_id: &str,
    subject_id: &str,
    version: u32,
    release: bool,
) -> Result<bool, String> {
    let subject = adventuresim_core::case::SubjectId::new(subject_id)
        .map_err(|_| "Custody subject ID is invalid")?;
    let fact = if release {
        adventuresim_core::case::OutcomeFactKind::SubjectReleased {
            subject_id: subject,
        }
    } else {
        adventuresim_core::case::OutcomeFactKind::SubjectRescued {
            subject_id: subject,
        }
    };
    transition_case_custody(
        ctx,
        source_id,
        case_id,
        party_id,
        CustodyObjectKind::Subject,
        subject_id,
        if release {
            CustodyHolderKind::Released
        } else {
            CustodyHolderKind::Party
        },
        if release { "" } else { party_id },
        version,
        Some(fact),
    )
}

/// Sole trusted fact ingestion and evaluation seam. Callers must already have
/// authenticated their strategic authority and bind `source_id` to a durable
/// domain receipt; no public reducer exposes this function.
fn validated_case_outcome_provenance(
    ctx: &ReducerContext,
    case: &CaseAuthority,
) -> Result<Option<ValidatedQuestGenerationAuthority>, String> {
    let mut authorities: Vec<_> = ctx
        .db
        .quest_generation_authority()
        .iter()
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
        "manual" if case.generated_case_id.is_empty() => {
            if authorities.is_empty() {
                Ok(None)
            } else {
                Err("Manual case collides with generated quest authority".into())
            }
        }
        "generated" if case.generated_case_id == case.id && !case.generated_case_id.is_empty() => {
            if authorities.len() != 1 {
                return Err("Generated case must have exactly one quest authority".into());
            }
            let validated = validate_quest_generation_authority(&authorities[0])?;
            if validated.manifest.canonical_case_id != case.id {
                return Err("Generated case authority does not match case provenance".into());
            }
            let objectives: adventuresim_core::case::ObjectiveExpression =
                serde_json::from_str(&case.objective_expression_json)
                    .map_err(|_| "Case objective authority is invalid")?;
            if objectives != validated.manifest.objectives {
                return Err("Generated case objectives do not match its manifest".into());
            }
            Ok(Some(validated))
        }
        _ => Err("Case provenance tuple is invalid".into()),
    }
}

pub(crate) fn ingest_case_outcome_fact(
    ctx: &ReducerContext,
    source_id: &str,
    case_id: &str,
    party_id: &str,
    kind: adventuresim_core::case::OutcomeFactKind,
) -> Result<(), String> {
    let mut case = ctx
        .db
        .case_authority()
        .id()
        .find(&case_id.to_string())
        .ok_or("Case not found")?;
    let generated_provenance = validated_case_outcome_provenance(ctx, &case)?;
    let expression: adventuresim_core::case::ObjectiveExpression =
        serde_json::from_str(&case.objective_expression_json)
            .map_err(|_| "Case objective authority is invalid")?;
    let fact_id = format!(
        "fact:{}",
        source_id.strip_prefix("outcome:").unwrap_or(source_id)
    );
    let fact = adventuresim_core::case::OutcomeFact {
        id: adventuresim_core::case::OutcomeFactId::new(fact_id.clone())
            .map_err(|_| "Outcome fact ID is invalid")?,
        case_id: adventuresim_core::case::CaseId::new(case.id.clone())
            .map_err(|_| "Case ID is invalid")?,
        party_id: party_id.to_string(),
        source_id: source_id.to_string(),
        happened_at: crate::time::refresh_clock(ctx)?,
        kind,
    };
    let encoded = serde_json::to_string(&fact).map_err(|_| "Could not encode outcome fact")?;
    if let Some(existing) = ctx
        .db
        .case_outcome_fact()
        .source_id()
        .find(&source_id.to_string())
    {
        return if existing.case_id == case.id
            && existing.party_id == party_id
            && existing.fact_json == encoded
        {
            Ok(())
        } else {
            Err("Conflicting retry for case outcome source".into())
        };
    }
    if case.resolution_status != CaseResolutionStatus::Open {
        return Err("Case is no longer open".into());
    }
    ctx.db.case_outcome_fact().insert(CaseOutcomeFact {
        id: fact_id,
        case_id: case.id.clone(),
        party_id: party_id.to_string(),
        source_id: source_id.to_string(),
        fact_json: encoded,
        happened_at_minute: fact.happened_at,
    });

    let facts = ctx
        .db
        .case_outcome_fact()
        .case_id()
        .filter(&case.id)
        .map(|row| {
            serde_json::from_str::<adventuresim_core::case::OutcomeFact>(&row.fact_json)
                .map_err(|_| "Stored outcome fact is invalid".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let core_case_id =
        adventuresim_core::case::CaseId::new(case.id.clone()).map_err(|_| "Case ID is invalid")?;
    let evaluation = expression.evaluate(&core_case_id, party_id, &facts);
    if evaluation.state == adventuresim_core::case::EvaluationState::Pending {
        return Ok(());
    }

    let now = crate::time::refresh_clock(ctx)?;
    let winning_path_index = (evaluation.state
        == adventuresim_core::case::EvaluationState::Satisfied)
        .then(|| {
            evaluation.alternatives.iter().position(|path| {
                path.iter().all(|progress| {
                    progress.state == adventuresim_core::case::EvaluationState::Satisfied
                })
            })
        })
        .flatten()
        .and_then(|index| u16::try_from(index).ok());
    case.resolution_status =
        if evaluation.state == adventuresim_core::case::EvaluationState::Satisfied {
            CaseResolutionStatus::Resolved
        } else {
            CaseResolutionStatus::Failed
        };
    case.resolved_by_party_id = Some(party_id.to_string());
    ctx.db.case_authority().id().update(case.clone());

    for mut contract in ctx
        .db
        .contract_authority()
        .case_id()
        .filter(&case.id)
        .collect::<Vec<_>>()
    {
        if case.resolution_status == CaseResolutionStatus::Resolved
            && contract.status == ContractStatus::Accepted
            && contract.accepted_by.as_deref() == Some(party_id)
        {
            contract.status = ContractStatus::ReadyToReport;
        } else if matches!(
            contract.status,
            ContractStatus::Offered | ContractStatus::Accepted
        ) {
            contract.status = ContractStatus::Withdrawn;
        }
        ctx.db.contract_authority().id().update(contract);
    }

    let selected_finale_id =
        select_case_finale(ctx, &case.id, case.resolution_status, winning_path_index)?
            .unwrap_or_default();
    ctx.db.case_outcome().insert(CaseOutcome {
        case_id: case.id.clone(),
        party_id: party_id.to_string(),
        status: case.resolution_status,
        winning_path_index,
        resolved_at_minute: now,
        selected_finale_id: selected_finale_id.clone(),
        finale_executed: false,
    });
    if !selected_finale_id.is_empty() {
        execute_case_finale(
            ctx,
            &selected_finale_id,
            &format!("finale:{source_id}"),
            party_id,
        )?;
    }
    if let Some(validated) = generated_provenance {
        ensure_settlement_activity_inner(ctx, &validated.context.settlement_id)?;
    }
    Ok(())
}

fn select_case_finale(
    ctx: &ReducerContext,
    case_id: &str,
    resolution_status: CaseResolutionStatus,
    winning_path_index: Option<u16>,
) -> Result<Option<String>, String> {
    let mut finales = ctx
        .db
        .case_finale_authority()
        .case_id()
        .filter(&case_id.to_string())
        .collect::<Vec<_>>();
    finales.sort_by_key(|finale| (finale.priority, finale.id.clone()));
    let selected = finales
        .iter()
        .find(|finale| {
            finale.status == FinaleStatus::Available
                && finale.resolution_status == resolution_status
                && finale
                    .eligible_path_index
                    .is_none_or(|path| Some(path) == winning_path_index)
        })
        .map(|finale| finale.id.clone());
    for mut finale in finales {
        finale.status = if selected.as_deref() == Some(&finale.id) {
            FinaleStatus::Selected
        } else {
            FinaleStatus::Ineligible
        };
        ctx.db.case_finale_authority().id().update(finale);
    }
    Ok(selected)
}

fn execute_case_finale(
    ctx: &ReducerContext,
    finale_id: &str,
    source_id: &str,
    party_id: &str,
) -> Result<(), String> {
    if let Some(existing) = ctx
        .db
        .case_finale_execution()
        .finale_id()
        .find(&finale_id.to_string())
    {
        return if existing.source_id == source_id && existing.party_id == party_id {
            Ok(())
        } else {
            Err("Finale already executed by different authority".into())
        };
    }
    let mut finale = ctx
        .db
        .case_finale_authority()
        .id()
        .find(&finale_id.to_string())
        .ok_or("Finale not found")?;
    if finale.status != FinaleStatus::Selected {
        return Err("Finale is not selected".into());
    }
    let case = ctx
        .db
        .case_authority()
        .id()
        .find(&finale.case_id)
        .ok_or("Finale case not found")?;
    let now = crate::time::refresh_clock(ctx)?;
    if finale.kind == FinaleKind::ResolveLocalProblem
        && case.resolution_status == CaseResolutionStatus::Resolved
        && let Some(problem_id) = case.local_problem_id.as_ref()
    {
        crate::local_problem::apply_outcome(
            ctx,
            problem_id,
            &crate::local_problem::LocalProblemOutcomeInput {
                source_outcome_id: source_id.to_string(),
                at_minute: now,
                mitigation_bps: 10_000,
                resolve: true,
            },
        )?;
    }
    if case.resolution_status == CaseResolutionStatus::Resolved {
        let quest_authority = ctx
            .db
            .quest_generation_authority()
            .case_id()
            .find(&case.id)
            .or_else(|| {
                ctx.db
                    .quest_generation_authority()
                    .iter()
                    .find(|authority| {
                        authority.public_case_id == case.id
                            || authority.public_case_id == case.generated_case_id
                            || authority.case_id == case.generated_case_id
                    })
            });
        if let Some(authority) = quest_authority {
            crate::reputation::award_case_resolution(
                ctx,
                &case.id,
                &authority.public_case_id,
                party_id,
                &authority.settlement_id,
                500,
                now,
            )?;
        }
    }
    finale.status = FinaleStatus::Executed;
    ctx.db.case_finale_authority().id().update(finale.clone());
    ctx.db.case_finale_execution().insert(CaseFinaleExecution {
        finale_id: finale.id.clone(),
        source_id: source_id.to_string(),
        case_id: finale.case_id,
        party_id: party_id.to_string(),
        executed_at_minute: now,
    });
    if let Some(mut outcome) = ctx.db.case_outcome().case_id().find(&case.id) {
        outcome.finale_executed = true;
        ctx.db.case_outcome().case_id().update(outcome);
    }
    Ok(())
}

fn hostile_resolution_for_objective(
    requirement: &adventuresim_core::case::ObjectiveRequirement,
    hostile_group_id: &str,
) -> Option<(HostileResolutionKind, u32)> {
    use adventuresim_core::case::ObjectiveRequirement as R;
    match requirement {
        R::Defeat {
            hostile_group_id: id,
            ..
        } if id == hostile_group_id => Some((HostileResolutionKind::Defeated, 50)),
        R::DriveOff {
            hostile_group_id: id,
        } if id == hostile_group_id => Some((HostileResolutionKind::DrivenOff, 30)),
        _ => None,
    }
}

fn mission_candidate_from_capability(
    mission_id: &str,
    index: usize,
    capability: MissionApproachCapability,
) -> MissionOutcomeCandidate {
    MissionOutcomeCandidate {
        id: format!("{mission_id}:candidate:{index:03}"),
        mission_id: mission_id.to_string(),
        capability_id: capability.id,
        case_id: capability.case_id,
        case_site_id: capability.case_site_id,
        hostile_group_id: capability.hostile_group_id,
        path_index: capability.path_index,
        objective_id: capability.objective_id,
        resolution: capability.resolution,
        weight: capability.weight,
        capture_subject_id: capability.capture_subject_id,
        capture_custody_version: capability.capture_custody_version,
    }
}

pub(crate) fn generated_case_site_combat_group_id<'a>(
    generated: &'a adventuresim_core::quest_generation::GeneratedCase,
    case_site: &CaseSiteAuthority,
) -> Option<&'a str> {
    let mut finale_group_ids: BTreeSet<&str> = generated
        .finales
        .iter()
        .filter(|finale| {
            finale.site_id.0 == case_site.id.value && finale.strategic_outcome_compatible
        })
        .filter_map(|finale| finale.hostile_group_id.as_deref())
        .collect();
    let hostile_group_id = finale_group_ids.pop_first()?;
    (finale_group_ids.is_empty()
        && generated
            .hostile_groups
            .iter()
            .any(|(group_id, site_id, _, _)| {
                group_id == hostile_group_id && site_id.0 == case_site.id.value
            }))
    .then_some(hostile_group_id)
}

pub(crate) fn generated_case_site_combat_eligible<'a>(
    generated: &adventuresim_core::quest_generation::GeneratedCase,
    case: &CaseAuthority,
    case_site: &CaseSiteAuthority,
    hostile_groups: &'a [HostileGroupAuthority],
    finales: &[CaseFinaleAuthority],
    facts: &[adventuresim_core::case::OutcomeFact],
    party_id: &str,
) -> Option<&'a HostileGroupAuthority> {
    if case.provenance_kind != "generated"
        || case.generated_case_id != case.id
        || case.id != generated.canonical_case_id
        || case_site.case_id != case.id
        || case.resolution_status != CaseResolutionStatus::Open
    {
        return None;
    }
    let Some(generated_site) = generated
        .sites
        .iter()
        .find(|site| site.id.0 == case_site.id.value)
    else {
        return None;
    };
    if generated_site.safe_label != case_site.name {
        return None;
    }
    let hostile_group_id = generated_case_site_combat_group_id(generated, case_site)?;
    let site_groups: Vec<_> = hostile_groups
        .iter()
        .filter(|group| group.case_site_id == case_site.id)
        .collect();
    let [hostile_group] = site_groups.as_slice() else {
        return None;
    };
    if hostile_group.id != hostile_group_id
        || hostile_group.disposition != HostileGroupDisposition::Active
    {
        return None;
    }
    let Ok(expression) = serde_json::from_str::<adventuresim_core::case::ObjectiveExpression>(
        &case.objective_expression_json,
    ) else {
        return None;
    };
    if expression != generated.objectives {
        return None;
    }
    let Ok(core_case_id) = adventuresim_core::case::CaseId::new(case.id.clone()) else {
        return None;
    };
    let evaluation = expression.evaluate(&core_case_id, party_id, facts);
    expression
        .alternatives
        .iter()
        .enumerate()
        .any(|(path_index, path)| {
            let hostile_pending =
                path.objectives
                    .iter()
                    .enumerate()
                    .any(|(objective_index, objective)| {
                        evaluation
                            .alternatives
                            .get(path_index)
                            .and_then(|progress| progress.get(objective_index))
                            .is_some_and(|progress| {
                                progress.state == adventuresim_core::case::EvaluationState::Pending
                            })
                            && hostile_resolution_for_objective(
                                &objective.requirement,
                                hostile_group_id,
                            )
                            .is_some()
                    });
            hostile_pending
                && finales.iter().any(|finale| {
                    finale.case_id == case.id
                        && finale.kind == FinaleKind::RecordResolution
                        && finale.resolution_status == CaseResolutionStatus::Resolved
                        && finale.status == FinaleStatus::Available
                        && finale.eligible_path_index == u16::try_from(path_index).ok()
                })
        })
        .then_some(*hostile_group)
}

pub(crate) fn ensure_bound_mission_authority(
    ctx: &ReducerContext,
    mission_id: &str,
    party_id: &str,
    observer_character_id: u64,
    case_site: &CaseSiteAuthority,
    scene_key: &str,
) -> Result<MissionAuthority, String> {
    if let Some(existing) = ctx
        .db
        .mission_authority()
        .id()
        .find(&mission_id.to_string())
    {
        return if existing.party_id == party_id
            && existing.observer_character_id == observer_character_id
            && existing.case_site_id.as_ref() == Some(&case_site.id)
            && existing.scene_key == scene_key
            && existing.status == MissionAttemptStatus::Bound
        {
            Ok(existing)
        } else if existing.status != MissionAttemptStatus::Bound {
            Err("Mission ID is already terminal and cannot be reused".into())
        } else {
            Err("Mission ID is already bound to different authority".into())
        };
    }
    if exact_case_site_for_observer(ctx, observer_character_id, &case_site.id.value).is_none() {
        return Err("Mission observer does not know or have a visited exact case site".into());
    }
    let case = ctx
        .db
        .case_authority()
        .id()
        .find(&case_site.case_id)
        .ok_or("Case authority no longer exists")?;
    let expression: adventuresim_core::case::ObjectiveExpression =
        serde_json::from_str(&case.objective_expression_json)
            .map_err(|_| "Case objective authority is invalid")?;
    if case.resolution_status != CaseResolutionStatus::Open {
        return Err("Case is no longer open".into());
    }
    let facts = ctx
        .db
        .case_outcome_fact()
        .case_id()
        .filter(&case.id)
        .map(|row| {
            serde_json::from_str::<adventuresim_core::case::OutcomeFact>(&row.fact_json)
                .map_err(|_| "Stored outcome fact is invalid".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let hostile_group_id = match case_site_provenance_reducer(ctx, case_site) {
        Some(Some((canonical_case_id, _))) => {
            let authority = ctx
                .db
                .quest_generation_authority()
                .case_id()
                .find(&canonical_case_id)
                .ok_or("Generated combat authority is unavailable")?;
            let validated = validate_quest_generation_authority(&authority)
                .map_err(|_| "Generated combat authority is invalid")?;
            let hostile_group_id =
                generated_case_site_combat_group_id(&validated.manifest, case_site)
                    .ok_or("Generated combat authority has no exact hostile group")?;
            let hostile_groups: Vec<_> = ctx
                .db
                .hostile_group_authority()
                .id()
                .find(&hostile_group_id.to_string())
                .into_iter()
                .collect();
            let finales: Vec<_> = ctx
                .db
                .case_finale_authority()
                .case_id()
                .filter(&canonical_case_id)
                .collect();
            generated_case_site_combat_eligible(
                &validated.manifest,
                &case,
                case_site,
                &hostile_groups,
                &finales,
                &facts,
                party_id,
            )
            .map(|group| group.id.clone())
            .ok_or("Generated strategic combat is not available at this site")?
        }
        Some(None) => {
            let group = ctx
                .db
                .hostile_group_authority()
                .iter()
                .find(|group| group.case_site_id == case_site.id)
                .ok_or("Case site has no materialized hostile group")?;
            if group.disposition != HostileGroupDisposition::Active {
                return Err("Hostile group is already resolved".into());
            }
            let accepted_contract = ctx
                .db
                .contract_authority()
                .case_id()
                .filter(&case.id)
                .find(|contract| {
                    contract.status == ContractStatus::Accepted
                        && contract.accepted_by.as_deref() == Some(party_id)
                })
                .ok_or("This quest requires an accepted active contract")?;
            let party = ctx
                .db
                .party_authority()
                .id()
                .find(&party_id.to_string())
                .ok_or("Party not found")?;
            if party.active_contract_id.as_deref() != Some(&accepted_contract.id) {
                return Err("This quest requires an accepted active contract".into());
            }
            group.id
        }
        None => return Err("Case-site combat provenance is invalid or ambiguous".into()),
    };
    let core_case_id =
        adventuresim_core::case::CaseId::new(case.id.clone()).map_err(|_| "Case ID is invalid")?;
    let evaluation = expression.evaluate(&core_case_id, party_id, &facts);
    use adventuresim_core::case::ObjectiveRequirement as R;
    let mut capabilities = Vec::new();
    for (path_index, path) in expression.alternatives.iter().enumerate() {
        let Some(progress) = evaluation.alternatives.get(path_index) else {
            return Err("Case objective evaluation shape is invalid".into());
        };
        for (objective_index, objective) in path.objectives.iter().enumerate() {
            if progress.get(objective_index).is_none_or(|progress| {
                progress.state != adventuresim_core::case::EvaluationState::Pending
            }) {
                continue;
            }
            let (resolution, weight, capture_subject_id, capture_custody_version) =
                if let Some((resolution, weight)) =
                    hostile_resolution_for_objective(&objective.requirement, &hostile_group_id)
                {
                    (resolution, weight, None, None)
                } else {
                    match &objective.requirement {
                        R::Capture { subject_id } => {
                            let subject = subject_id.as_str().to_string();
                            let Some(custody) = ctx.db.case_custody().object_id().find(&subject)
                            else {
                                continue;
                            };
                            if custody.case_id != case.id
                                || custody.object_kind != CustodyObjectKind::Subject
                                || custody.holder_kind != CustodyHolderKind::Site
                                || custody.holder_id != case_site.id.value
                            {
                                continue;
                            }
                            (
                                HostileResolutionKind::Captured,
                                20u32,
                                Some(subject),
                                Some(custody.version),
                            )
                        }
                        _ => continue,
                    }
                };
            let path_index =
                u16::try_from(path_index).map_err(|_| "Case has too many objective paths")?;
            let id = mission_approach_capability_id(
                observer_character_id,
                &case.id,
                &case_site.id.value,
                &hostile_group_id,
                path_index,
                objective.id.as_str(),
                resolution,
                capture_subject_id.as_deref(),
                capture_custody_version,
            );
            let capability = MissionApproachCapability {
                id: id.clone(),
                observer_character_id,
                hostile_group_id: hostile_group_id.clone(),
                case_id: case.id.clone(),
                case_site_id: case_site.id.clone(),
                path_index,
                objective_id: objective.id.as_str().to_string(),
                resolution,
                weight,
                capture_subject_id,
                capture_custody_version,
                active: true,
            };
            if let Some(existing) = ctx.db.mission_approach_capability().id().find(&id) {
                if existing.observer_character_id != capability.observer_character_id
                    || existing.hostile_group_id != capability.hostile_group_id
                    || existing.case_id != capability.case_id
                    || existing.case_site_id != capability.case_site_id
                    || existing.path_index != capability.path_index
                    || existing.objective_id != capability.objective_id
                    || existing.resolution != capability.resolution
                    || existing.weight != capability.weight
                    || existing.capture_subject_id != capability.capture_subject_id
                    || existing.capture_custody_version != capability.capture_custody_version
                {
                    return Err(
                        "Mission approach capability conflicts with existing authority".into(),
                    );
                }
                if !existing.active {
                    continue;
                }
                capabilities.push(existing);
            } else {
                ctx.db
                    .mission_approach_capability()
                    .insert(capability.clone());
                capabilities.push(capability);
            }
        }
    }
    capabilities.sort_by(|left, right| left.id.cmp(&right.id));
    if capabilities.is_empty() {
        return Err("Case site has no unresolved observer-authorized combat approach".into());
    }
    let hostile_group = ctx
        .db
        .hostile_group_authority()
        .id()
        .find(&hostile_group_id)
        .ok_or("Bound hostile group disappeared")?;
    let authority = MissionAuthority {
        id: mission_id.to_string(),
        party_id: party_id.to_string(),
        case_site_id: Some(case_site.id.clone()),
        hostile_group_id: Some(hostile_group_id.clone()),
        observer_character_id,
        case_id: case.id.clone(),
        outcome_entropy: ctx.random(),
        status: MissionAttemptStatus::Bound,
        committed_resolution: None,
        committed_capture_subject_id: None,
        scene_key: scene_key.to_string(),
        hostile_version: hostile_group.escalation_incident_ordinal,
        enemy_count: hostile_group.enemy_count,
        enemy_difficulty: hostile_group.base_difficulty,
        enemy_combat_scale_bps: hostile_group.combat_scale_bps,
        normalized_combat_power: hostile_group.normalized_combat_power,
        drop_item_id: hostile_group.drop_item_id.clone(),
        drop_quantity: hostile_group.drop_quantity,
    };
    ctx.db.mission_authority().insert(authority.clone());
    for (index, capability) in capabilities.into_iter().enumerate() {
        ctx.db
            .mission_outcome_candidate()
            .insert(mission_candidate_from_capability(
                mission_id, index, capability,
            ));
    }
    Ok(authority)
}
