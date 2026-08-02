// Systemic Character interactions: contextual surrender, individual defection,
// global custody, and legal property. Tactical state remains transient.

fn valid_systemic_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 150
        && value.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b':' | b'-' | b'_' | b'.')
        })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum DispositionKind {
    Neutral,
    Hostile,
    OfferPending,
    DemandPending,
    Refused,
    Surrendered,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum SurrenderActionKind {
    Offer,
    Demand,
    Accept,
    Refuse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum SurrenderObligationKind {
    Disarm,
    LeaveSite,
    PayRansom,
    EnterCustody,
    Testify,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
pub struct SurrenderObligation {
    pub kind: SurrenderObligationKind,
    pub beneficiary_id: String,
    pub amount_minor: u64,
}

#[derive(Clone, Debug)]
#[table(accessor=surrender_obligation_fulfillment)]
pub struct SurrenderObligationFulfillment {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub source_id: String,
    pub context_id: String,
    pub character_id: u64,
    pub kind: SurrenderObligationKind,
    pub beneficiary_id: String,
    pub amount_minor: u64,
}
#[derive(Clone, Debug)]
#[table(accessor=surrender_testimony_commitment)]
pub struct SurrenderTestimonyCommitment {
    #[primary_key]
    pub id: String,
    pub context_id: String,
    pub character_id: u64,
    pub beneficiary_id: String,
    pub fulfilled: bool,
}

/// Private per-context state. Hostility is never copied onto Character.
#[derive(Clone, Debug)]
#[table(accessor = character_context_disposition_authority)]
pub struct CharacterContextDispositionAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub context_id: String,
    #[index(btree)]
    pub character_id: u64,
    pub disposition: DispositionKind,
    pub revision: u32,
    pub obligations: Vec<SurrenderObligation>,
    pub refusal_count: u16,
    pub last_source_id: String,
}

#[derive(Clone, Debug)]
#[table(accessor = character_disposition_transition_receipt)]
pub struct CharacterDispositionTransitionReceipt {
    #[primary_key]
    pub source_id: String,
    pub context_id: String,
    pub character_id: u64,
    pub action: SurrenderActionKind,
    pub expected_revision: u32,
    pub resulting_revision: u32,
    pub resulting_disposition: DispositionKind,
}

pub(crate) fn ensure_context_disposition(
    ctx: &ReducerContext,
    context_id: &str,
    character_id: u64,
    hostile: bool,
) -> Result<(), String> {
    let id = format!("{context_id}:{character_id}");
    if ctx
        .db
        .character_context_disposition_authority()
        .id()
        .find(&id)
        .is_none()
    {
        ctx.db.character_context_disposition_authority().insert(
            CharacterContextDispositionAuthority {
                id,
                context_id: context_id.into(),
                character_id,
                disposition: if hostile {
                    DispositionKind::Hostile
                } else {
                    DispositionKind::Neutral
                },
                revision: 0,
                obligations: Vec::new(),
                refusal_count: 0,
                last_source_id: String::new(),
            },
        );
        if hostile {
            author_context_surrender_obligations(
                ctx,
                context_id,
                character_id,
                vec![
                    SurrenderObligation {
                        kind: SurrenderObligationKind::Disarm,
                        beneficiary_id: context_id.into(),
                        amount_minor: 0,
                    },
                    SurrenderObligation {
                        kind: SurrenderObligationKind::LeaveSite,
                        beneficiary_id: String::new(),
                        amount_minor: 0,
                    },
                ],
            )?;
        }
    }
    Ok(())
}

/// Typed trusted adapter for an owning quest/encounter producer. There is no
/// public reducer: browsers cannot author terms or private decision inputs.
pub(crate) fn author_context_surrender_obligations(
    ctx: &ReducerContext,
    context_id: &str,
    character_id: u64,
    obligations: Vec<SurrenderObligation>,
) -> Result<(), String> {
    let id = format!("{context_id}:{character_id}");
    let mut row = ctx
        .db
        .character_context_disposition_authority()
        .id()
        .find(&id)
        .ok_or("Context disposition not found")?;
    let core: Vec<_> = obligations
        .iter()
        .map(
            |obligation| adventuresim_core::systemic_character::AuthoredObligation {
                kind: match obligation.kind {
                    SurrenderObligationKind::Disarm => {
                        adventuresim_core::systemic_character::ObligationKind::Disarm
                    }
                    SurrenderObligationKind::LeaveSite => {
                        adventuresim_core::systemic_character::ObligationKind::LeaveSite
                    }
                    SurrenderObligationKind::PayRansom => {
                        adventuresim_core::systemic_character::ObligationKind::PayRansom
                    }
                    SurrenderObligationKind::EnterCustody => {
                        adventuresim_core::systemic_character::ObligationKind::EnterCustody
                    }
                    SurrenderObligationKind::Testify => {
                        adventuresim_core::systemic_character::ObligationKind::Testify
                    }
                },
                beneficiary_id: obligation.beneficiary_id.clone(),
                amount_minor: obligation.amount_minor,
            },
        )
        .collect();
    adventuresim_core::systemic_character::obligation_effects(&core)
        .map_err(|_| "Authored surrender obligations are invalid")?;
    row.obligations = obligations;
    ctx.db
        .character_context_disposition_authority()
        .id()
        .update(row);
    Ok(())
}

fn authored_custodian(
    ctx: &ReducerContext,
    beneficiary_id: &str,
) -> Result<(CharacterCustodianKind, String), String> {
    if let Some(id) = beneficiary_id.strip_prefix("party:") {
        ctx.db
            .party_authority()
            .id()
            .find(&id.to_owned())
            .ok_or("Authored custodian party does not exist")?;
        Ok((CharacterCustodianKind::Party, id.into()))
    } else if let Some(id) = beneficiary_id.strip_prefix("character:") {
        let character_id = id
            .parse::<u64>()
            .map_err(|_| "Authored character custodian is invalid")?;
        ctx.db
            .character()
            .id()
            .find(character_id)
            .filter(|character| character.alive)
            .ok_or("Authored character custodian does not exist")?;
        Ok((CharacterCustodianKind::Character, id.into()))
    } else if let Some(id) = beneficiary_id.strip_prefix("site:") {
        ctx.db
            .case_site_authority()
            .id_key()
            .find(&id.to_owned())
            .ok_or("Authored site custodian does not exist")?;
        Ok((CharacterCustodianKind::Site, id.into()))
    } else {
        Err("Authored custody beneficiary must be a typed party, character, or site".into())
    }
}
fn apply_authored_surrender_obligations(
    ctx: &ReducerContext,
    character_id: u64,
    context_id: &str,
    source_id: &str,
    obligations: &[SurrenderObligation],
) -> Result<(), String> {
    let core: Vec<_> = obligations
        .iter()
        .map(
            |obligation| adventuresim_core::systemic_character::AuthoredObligation {
                kind: match obligation.kind {
                    SurrenderObligationKind::Disarm => {
                        adventuresim_core::systemic_character::ObligationKind::Disarm
                    }
                    SurrenderObligationKind::LeaveSite => {
                        adventuresim_core::systemic_character::ObligationKind::LeaveSite
                    }
                    SurrenderObligationKind::PayRansom => {
                        adventuresim_core::systemic_character::ObligationKind::PayRansom
                    }
                    SurrenderObligationKind::EnterCustody => {
                        adventuresim_core::systemic_character::ObligationKind::EnterCustody
                    }
                    SurrenderObligationKind::Testify => {
                        adventuresim_core::systemic_character::ObligationKind::Testify
                    }
                },
                beneficiary_id: obligation.beneficiary_id.clone(),
                amount_minor: obligation.amount_minor,
            },
        )
        .collect();
    let effects = adventuresim_core::systemic_character::obligation_effects(&core)
        .map_err(|_| "Stored surrender obligations are invalid")?;
    for (ordinal, (obligation, effect)) in obligations.iter().zip(effects).enumerate() {
        let id = format!("{source_id}:obligation:{ordinal}");
        if ctx
            .db
            .surrender_obligation_fulfillment()
            .id()
            .find(&id)
            .is_some()
        {
            continue;
        }
        match effect {
            adventuresim_core::systemic_character::ObligationEffect::Disarm { .. } => {
                let equipped: Vec<u64> = ctx
                    .db
                    .character_equipped_item()
                    .character_id()
                    .filter(character_id)
                    .map(|row| row.inventory_item_id)
                    .collect();
                for inventory_id in equipped {
                    crate::character::unequip_wearable(ctx, inventory_id);
                }
            }
            adventuresim_core::systemic_character::ObligationEffect::LeaveSite => {
                if let Some(mut membership) = ctx
                    .db
                    .character_context_membership()
                    .context_id()
                    .filter(context_id)
                    .find(|membership| membership.character_id == character_id)
                {
                    membership.active = false;
                    membership.revision = membership
                        .revision
                        .checked_add(1)
                        .ok_or("Context membership revision overflow")?;
                    ctx.db
                        .character_context_membership()
                        .id()
                        .update(membership);
                }
            }
            adventuresim_core::systemic_character::ObligationEffect::PayRansom {
                amount_minor,
                ..
            } => {
                let mut custody = ctx
                    .db
                    .character_custody()
                    .character_id()
                    .find(character_id)
                    .filter(|custody| custody.status == CharacterCustodyStatus::Captive)
                    .ok_or("Pay-ransom obligation requires an authored custody obligation first")?;
                custody.ransom_minor = Some(amount_minor);
                ctx.db.character_custody().character_id().update(custody);
            }
            adventuresim_core::systemic_character::ObligationEffect::EnterCustody {
                custodian_id,
            } => {
                let (kind, custodian_id) = authored_custodian(ctx, &custodian_id)?;
                let current = ctx.db.character_custody().character_id().find(character_id);
                let expected = current.as_ref().map_or(0, |row| row.version);
                let case_id = validated_case_provenance(ctx, context_id, character_id)?;
                transition_character_custody(
                    ctx,
                    character_id,
                    expected,
                    CharacterCustodyStatus::Captive,
                    kind,
                    custodian_id,
                    None,
                    context_id.into(),
                    case_id,
                    format!("{id}:custody"),
                )?;
            }
            adventuresim_core::systemic_character::ObligationEffect::Testify { beneficiary_id } => {
                ctx.db
                    .surrender_testimony_commitment()
                    .insert(SurrenderTestimonyCommitment {
                        id: format!("{id}:testify"),
                        context_id: context_id.into(),
                        character_id,
                        beneficiary_id,
                        fulfilled: false,
                    });
            }
        }
        ctx.db
            .surrender_obligation_fulfillment()
            .insert(SurrenderObligationFulfillment {
                id,
                source_id: source_id.into(),
                context_id: context_id.into(),
                character_id,
                kind: obligation.kind,
                beneficiary_id: obligation.beneficiary_id.clone(),
                amount_minor: obligation.amount_minor,
            });
    }
    Ok(())
}

/// Observer-safe result: offered terms and committed outcome only. Morale,
/// fear, affinity, familiarity, leverage and decision scores stay private.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendContextDisposition {
    pub observer_party_id: String,
    pub contact_ref: String,
    pub character_id: u64,
    pub disposition: DispositionKind,
    pub revision: u32,
    pub offered_terms: Vec<SurrenderObligation>,
}

#[view(accessor = backend_context_dispositions, public)]
pub fn backend_context_dispositions(ctx: &ViewContext) -> Vec<BackendContextDisposition> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for contact in ctx
        .db
        .party_context_contact_authority()
        .scan_id()
        .filter(0u8)
        .filter(|contact| contact.mutual_awareness)
    {
        if ctx
            .db
            .party_authority()
            .id()
            .find(&contact.party_id)
            .is_none()
        {
            continue;
        }
        for state in ctx
            .db
            .character_context_disposition_authority()
            .context_id()
            .filter(&contact.context_id)
        {
            out.push(BackendContextDisposition {
                observer_party_id: contact.party_id.clone(),
                contact_ref: contact.location_id.clone(),
                character_id: state.character_id,
                disposition: state.disposition,
                revision: state.revision,
                offered_terms: state.obligations.clone(),
            });
        }
    }
    out
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct TacticalParticipantExclusion {
    pub mission_id: String,
    pub character_id: u64,
    pub reason: String,
}
#[view(accessor=tactical_participant_exclusions,public)]
pub fn tactical_participant_exclusions(ctx: &ViewContext) -> Vec<TacticalParticipantExclusion> {
    let Some(server) = ctx
        .db
        .tactical_server_authority()
        .identity()
        .find(ctx.sender())
    else {
        return Vec::new();
    };
    let Some(mission) = ctx.db.mission_authority().id().find(&server.mission_id) else {
        return Vec::new();
    };
    let Some(context_id) = mission.hostile_group_id else {
        return Vec::new();
    };
    server
        .enemy_character_ids
        .into_iter()
        .filter(|id| {
            ctx.db
                .character_context_disposition_authority()
                .id()
                .find(&format!("{context_id}:{id}"))
                .is_some_and(|r| r.disposition == DispositionKind::Surrendered)
        })
        .map(|character_id| TacticalParticipantExclusion {
            mission_id: server.mission_id.clone(),
            character_id,
            reason: "surrendered".into(),
        })
        .collect()
}

fn core_disposition(
    value: DispositionKind,
) -> adventuresim_core::systemic_character::ContextDisposition {
    use adventuresim_core::systemic_character::ContextDisposition as C;
    match value {
        DispositionKind::Neutral => C::Neutral,
        DispositionKind::Hostile => C::Hostile,
        DispositionKind::OfferPending => C::OfferPending,
        DispositionKind::DemandPending => C::DemandPending,
        DispositionKind::Refused => C::Refused,
        DispositionKind::Surrendered => C::Surrendered,
    }
}
fn stored_disposition(
    value: adventuresim_core::systemic_character::ContextDisposition,
) -> DispositionKind {
    use adventuresim_core::systemic_character::ContextDisposition as C;
    match value {
        C::Neutral => DispositionKind::Neutral,
        C::Hostile => DispositionKind::Hostile,
        C::OfferPending => DispositionKind::OfferPending,
        C::DemandPending => DispositionKind::DemandPending,
        C::Refused => DispositionKind::Refused,
        C::Surrendered => DispositionKind::Surrendered,
    }
}
fn core_action(
    value: SurrenderActionKind,
) -> adventuresim_core::systemic_character::SurrenderAction {
    use adventuresim_core::systemic_character::SurrenderAction as A;
    match value {
        SurrenderActionKind::Offer => A::Offer,
        SurrenderActionKind::Demand => A::Demand,
        SurrenderActionKind::Accept => A::Accept,
        SurrenderActionKind::Refuse => A::Refuse,
    }
}

fn ensure_transition_retry(
    ctx: &ReducerContext,
    source_id: &str,
    context_id: &str,
    character_id: u64,
    action: SurrenderActionKind,
    expected_revision: u32,
) -> Result<Option<u32>, String> {
    if !valid_systemic_id(source_id) {
        return Err("Invalid surrender source ID".into());
    }
    if let Some(receipt) = ctx
        .db
        .character_disposition_transition_receipt()
        .source_id()
        .find(&source_id.to_owned())
    {
        if receipt.context_id == context_id
            && receipt.character_id == character_id
            && receipt.action == action
            && receipt.expected_revision == expected_revision
        {
            return Ok(Some(receipt.resulting_revision));
        }
        return Err("Conflicting surrender source ID reuse".into());
    }
    Ok(None)
}

fn authoritative_surrender_inputs(
    ctx: &ReducerContext,
    actor_id: u64,
    target_id: u64,
    party_id: &str,
    context_id: &str,
    obligations: &[SurrenderObligation],
) -> Result<adventuresim_core::systemic_character::SurrenderInputs, String> {
    let contact = ctx
        .db
        .party_context_contact_authority()
        .party_id()
        .filter(party_id)
        .find(|c| c.context_id == context_id && c.mutual_awareness)
        .ok_or("Surrender requires mutual awareness")?;
    let condition = ctx
        .db
        .character_strategic_condition()
        .character_id()
        .find(target_id);
    let morale = condition
        .as_ref()
        .map_or(0.0, |c| c.morale)
        .round()
        .clamp(-100.0, 100.0) as i16;
    let fear = condition
        .as_ref()
        .map_or(0.0, |c| c.fear * 100.0)
        .round()
        .clamp(0.0, 100.0) as u16;
    let incap = condition
        .as_ref()
        .map_or(0.0, |c| c.incapacitation * 10_000.0)
        .round()
        .clamp(0.0, 10_000.0) as u16;
    let affinity = crate::social::current_affinity(ctx, target_id, actor_id)
        .round()
        .clamp(-100.0, 100.0) as i16;
    let (lo, hi) = if actor_id < target_id {
        (actor_id, target_id)
    } else {
        (target_id, actor_id)
    };
    let familiarity = ctx
        .db
        .character_familiarity()
        .id()
        .find(&format!("{lo}:{hi}"))
        .map_or(0, |f| (f.shared_minutes.min(6000) * 10_000 / 6000) as u16);
    let leverage = if incap >= 5_000 {
        45
    } else if fear >= 50 {
        25
    } else {
        0
    };
    Ok(adventuresim_core::systemic_character::SurrenderInputs {
        morale,
        fear,
        incapacitation_bps: incap,
        affinity,
        familiarity_bps: familiarity,
        leverage,
        mutual_awareness: contact.mutual_awareness,
        obligations: obligations
            .iter()
            .map(
                |o| adventuresim_core::systemic_character::AuthoredObligation {
                    kind: match o.kind {
                        SurrenderObligationKind::Disarm => {
                            adventuresim_core::systemic_character::ObligationKind::Disarm
                        }
                        SurrenderObligationKind::LeaveSite => {
                            adventuresim_core::systemic_character::ObligationKind::LeaveSite
                        }
                        SurrenderObligationKind::PayRansom => {
                            adventuresim_core::systemic_character::ObligationKind::PayRansom
                        }
                        SurrenderObligationKind::EnterCustody => {
                            adventuresim_core::systemic_character::ObligationKind::EnterCustody
                        }
                        SurrenderObligationKind::Testify => {
                            adventuresim_core::systemic_character::ObligationKind::Testify
                        }
                    },
                    beneficiary_id: o.beneficiary_id.clone(),
                    amount_minor: o.amount_minor,
                },
            )
            .collect(),
    })
}

fn apply_surrender_transition(
    ctx: &ReducerContext,
    actor_id: u64,
    target_id: u64,
    party_id: &str,
    context_id: &str,
    action: SurrenderActionKind,
    expected_revision: u32,
    source_id: &str,
    tactical_evidence: bool,
) -> Result<(), String> {
    if ensure_transition_retry(
        ctx,
        source_id,
        context_id,
        target_id,
        action,
        expected_revision,
    )?
    .is_some()
    {
        return Ok(());
    }
    let id = format!("{context_id}:{target_id}");
    let mut row = ctx
        .db
        .character_context_disposition_authority()
        .id()
        .find(&id)
        .ok_or("Context disposition not found")?;
    if row.revision != expected_revision {
        return Err("Stale disposition revision".into());
    }
    let requested = core_action(action);
    let mut next = adventuresim_core::systemic_character::resolve_surrender_transition(
        core_disposition(row.disposition),
        requested,
        tactical_evidence,
    )
    .map_err(|_| "Invalid surrender transition")?;
    if matches!(
        action,
        SurrenderActionKind::Offer | SurrenderActionKind::Demand
    ) {
        let decision = adventuresim_core::systemic_character::decide_surrender(
            requested,
            &authoritative_surrender_inputs(
                ctx,
                actor_id,
                target_id,
                party_id,
                context_id,
                &row.obligations,
            )?,
        )
        .map_err(|_| "Surrender decision inputs are invalid")?;
        next = match decision {
            adventuresim_core::systemic_character::Decision::Accept => {
                adventuresim_core::systemic_character::ContextDisposition::Surrendered
            }
            adventuresim_core::systemic_character::Decision::Refuse => {
                adventuresim_core::systemic_character::ContextDisposition::Refused
            }
        };
    } else if tactical_evidence && action != SurrenderActionKind::Accept {
        return Err("Tactical evidence may only accept surrender".into());
    }
    row.revision = row
        .revision
        .checked_add(1)
        .ok_or("Disposition revision overflow")?;
    row.disposition = stored_disposition(next);
    row.last_source_id = source_id.into();
    if row.disposition == DispositionKind::Refused {
        row.refusal_count = row
            .refusal_count
            .checked_add(1)
            .ok_or("Refusal count overflow")?;
    }
    ctx.db
        .character_context_disposition_authority()
        .id()
        .update(row.clone());
    if row.disposition == DispositionKind::Surrendered {
        apply_authored_surrender_obligations(
            ctx,
            target_id,
            context_id,
            source_id,
            &row.obligations,
        )?;
        if let Some(mut membership) = ctx
            .db
            .character_context_membership()
            .context_id()
            .filter(context_id)
            .find(|m| m.character_id == target_id && m.active)
        {
            membership.active = false;
            membership.revision = membership
                .revision
                .checked_add(1)
                .ok_or("Context membership revision overflow")?;
            ctx.db
                .character_context_membership()
                .id()
                .update(membership);
        }
        if let Some(mut group) = ctx
            .db
            .hostile_group_authority()
            .id()
            .find(&context_id.to_owned())
        {
            group.enemy_count = u32::try_from(
                ctx.db
                    .character_context_membership()
                    .context_id()
                    .filter(context_id)
                    .filter(|m| {
                        m.active && m.role == crate::world_actor::CharacterContextRole::Counterparty
                    })
                    .count(),
            )
            .map_err(|_| "Hostile count exceeds supported range")?;
            if group.enemy_count == 0 {
                group.disposition = HostileGroupDisposition::DrivenOff;
            }
            ctx.db.hostile_group_authority().id().update(group);
        }
        if !ctx
            .db
            .character_context_membership()
            .context_id()
            .filter(context_id)
            .any(|m| m.active && m.role == crate::world_actor::CharacterContextRole::Counterparty)
            && let Some(mut encounter) = ctx
                .db
                .strategic_encounter()
                .iter()
                .find(|e| e.encounter_id == context_id)
        {
            encounter.status = "resolved".into();
            encounter.outcome = Some("surrendered".into());
            encounter.available_choices.clear();
            encounter.revision = encounter
                .revision
                .checked_add(1)
                .ok_or("Encounter revision overflow")?;
            ctx.db.strategic_encounter().party_id().update(encounter);
        }
        if let Some(case_id) = case_id_for_context(ctx, context_id) {
            ingest_case_outcome_fact(
                ctx,
                source_id,
                &case_id,
                party_id,
                adventuresim_core::case::OutcomeFactKind::CharacterSurrendered {
                    character_id: target_id,
                    context_id: context_id.into(),
                },
            )?;
        }
    }
    ctx.db.character_disposition_transition_receipt().insert(
        CharacterDispositionTransitionReceipt {
            source_id: source_id.into(),
            context_id: context_id.into(),
            character_id: target_id,
            action,
            expected_revision,
            resulting_revision: row.revision,
            resulting_disposition: row.disposition,
        },
    );
    Ok(())
}

fn case_id_for_context(ctx: &ReducerContext, context_id: &str) -> Option<String> {
    let group = ctx
        .db
        .hostile_group_authority()
        .id()
        .find(&context_id.to_owned())?;
    ctx.db
        .case_site_authority()
        .id_key()
        .find(&group.case_site_id.value)
        .map(|site| site.case_id)
}
fn validated_case_provenance(
    ctx: &ReducerContext,
    context_id: &str,
    character_id: u64,
) -> Result<Option<String>, String> {
    if !ctx
        .db
        .character_context_membership()
        .context_id()
        .filter(context_id)
        .any(|membership| membership.character_id == character_id)
    {
        return Err("Character is not a member of the explicit context".into());
    }
    let case_id = case_id_for_context(ctx, context_id);
    let conflicting = ctx
        .db
        .character_context_membership()
        .character_id()
        .filter(character_id)
        .filter_map(|membership| case_id_for_context(ctx, &membership.context_id))
        .any(|candidate| {
            case_id
                .as_ref()
                .is_some_and(|expected| expected != &candidate)
        });
    if conflicting {
        return Err("Character has ambiguous case provenance".into());
    }
    Ok(case_id)
}

/// Shared precombat contextual Character action. The client supplies no morale,
/// affinity, leverage, obligations, acceptance roll, or quest fact.
#[reducer]
pub fn resolve_context_surrender(
    ctx: &ReducerContext,
    actor_id: u64,
    target_id: u64,
    contact_ref: String,
    action: SurrenderActionKind,
    expected_revision: u32,
    source_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, actor_id)?;
    if !valid_systemic_id(&contact_ref) {
        return Err("Invalid surrender context reference".into());
    }
    if !matches!(
        action,
        SurrenderActionKind::Offer | SurrenderActionKind::Demand
    ) {
        return Err("Clients may only offer or demand surrender; acceptance and refusal are authoritative outcomes".into());
    }
    let party_id = ctx
        .db
        .character()
        .id()
        .find(actor_id)
        .and_then(|c| c.party_id)
        .ok_or("Actor has no party")?;
    let context_id = ctx
        .db
        .party_context_contact_authority()
        .party_id()
        .filter(&party_id)
        .find(|c| {
            c.mutual_awareness && (c.location_id == contact_ref || c.context_id == contact_ref)
        })
        .map(|c| c.context_id)
        .ok_or("Context reference is not an aware party contact")?;
    apply_surrender_transition(
        ctx,
        actor_id,
        target_id,
        &party_id,
        &context_id,
        action,
        expected_revision,
        &source_id,
        false,
    )
}

/// Authenticated tactical result adapter. The tactical server attests only that
/// this participant yielded; strategic authority derives exclusion and truth.
#[reducer]
pub fn commit_tactical_participant_surrender(
    ctx: &ReducerContext,
    mission_id: String,
    character_id: u64,
    expected_revision: u32,
    source_id: String,
) -> Result<(), String> {
    let server = ctx
        .db
        .tactical_server_authority()
        .identity()
        .find(ctx.sender())
        .ok_or("Authenticated tactical server required")?;
    if server.mission_id != mission_id || !server.enemy_character_ids.contains(&character_id) {
        return Err("Character is not an authenticated tactical participant".into());
    }
    let mission = ctx
        .db
        .mission_authority()
        .id()
        .find(&mission_id)
        .ok_or("Mission not found")?;
    let context_id = mission
        .hostile_group_id
        .ok_or("Mission has no hostile context")?;
    apply_surrender_transition(
        ctx,
        mission.observer_character_id,
        character_id,
        &mission.party_id,
        &context_id,
        SurrenderActionKind::Accept,
        expected_revision,
        &source_id,
        true,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum CharacterCustodyStatus {
    Captive,
    Released,
    Escaped,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum CharacterCustodianKind {
    None,
    Party,
    Character,
    Faction,
    Site,
}
#[derive(Clone, Debug)]
#[table(accessor=character_custody)]
pub struct CharacterCustody {
    #[primary_key]
    pub character_id: u64,
    #[index(btree)]
    pub observer_party_id: String,
    pub status: CharacterCustodyStatus,
    pub custodian_kind: CharacterCustodianKind,
    pub custodian_id: String,
    pub version: u32,
    pub source_id: String,
    pub context_id: String,
    pub case_id: Option<String>,
    pub ransom_minor: Option<u64>,
}
#[derive(Clone, Debug)]
#[table(accessor=character_custody_receipt)]
pub struct CharacterCustodyReceipt {
    #[primary_key]
    pub source_id: String,
    pub character_id: u64,
    pub expected_version: u32,
    pub resulting_version: u32,
    pub status: CharacterCustodyStatus,
    pub custodian_kind: CharacterCustodianKind,
    pub custodian_id: String,
    pub context_id: String,
}
#[derive(Clone, Debug)]
#[table(accessor=ransom_payment_receipt)]
pub struct RansomPaymentReceipt {
    #[primary_key]
    pub source_id: String,
    pub actor_id: u64,
    pub character_id: u64,
    pub expected_version: u32,
    pub payer_party_id: String,
    pub recipient_kind: CharacterCustodianKind,
    pub recipient_id: String,
    pub amount_minor: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendCharacterCustody {
    pub observer_party_id: String,
    pub character_id: u64,
    pub status: CharacterCustodyStatus,
    pub custodian_kind: CharacterCustodianKind,
    pub custodian_id: String,
    pub version: u32,
    pub ransom_minor: Option<u64>,
}
#[view(accessor=backend_character_custodies,public)]
pub fn backend_character_custodies(ctx: &ViewContext) -> Vec<BackendCharacterCustody> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for party in ctx.db.party_authority().gateway_bucket().filter(0u8) {
        for row in ctx
            .db
            .character_custody()
            .observer_party_id()
            .filter(&party.id)
        {
            out.push(BackendCharacterCustody {
                observer_party_id: party.id.clone(),
                character_id: row.character_id,
                status: row.status,
                custodian_kind: row.custodian_kind,
                custodian_id: row.custodian_id.clone(),
                version: row.version,
                ransom_minor: row.ransom_minor,
            });
        }
        for member in ctx.db.party_member().party_id().filter(&party.id) {
            if let Some(row) = ctx
                .db
                .character_custody()
                .character_id()
                .find(member.character_id)
            {
                if row.observer_party_id != party.id {
                    out.push(BackendCharacterCustody {
                        observer_party_id: party.id.clone(),
                        character_id: row.character_id,
                        status: row.status,
                        custodian_kind: row.custodian_kind,
                        custodian_id: row.custodian_id.clone(),
                        version: row.version,
                        ransom_minor: row.ransom_minor,
                    });
                }
            }
        }
    }
    out
}

fn stored_custodian(
    kind: CharacterCustodianKind,
    id: &str,
) -> adventuresim_core::systemic_character::Custodian {
    match kind {
        CharacterCustodianKind::None => adventuresim_core::systemic_character::Custodian::None,
        CharacterCustodianKind::Party => {
            adventuresim_core::systemic_character::Custodian::Party(id.into())
        }
        CharacterCustodianKind::Character => {
            adventuresim_core::systemic_character::Custodian::Character(
                id.parse().unwrap_or(u64::MAX),
            )
        }
        CharacterCustodianKind::Faction => {
            adventuresim_core::systemic_character::Custodian::Faction(id.into())
        }
        CharacterCustodianKind::Site => {
            adventuresim_core::systemic_character::Custodian::Site(id.into())
        }
    }
}
fn core_custody_status(
    status: CharacterCustodyStatus,
) -> adventuresim_core::systemic_character::CustodyStatus {
    match status {
        CharacterCustodyStatus::Captive => {
            adventuresim_core::systemic_character::CustodyStatus::Captive
        }
        CharacterCustodyStatus::Released => {
            adventuresim_core::systemic_character::CustodyStatus::Released
        }
        CharacterCustodyStatus::Escaped => {
            adventuresim_core::systemic_character::CustodyStatus::Escaped
        }
    }
}
fn custodian_exists_and_controlled(
    ctx: &ReducerContext,
    actor_id: u64,
    kind: CharacterCustodianKind,
    id: &str,
) -> bool {
    match kind {
        CharacterCustodianKind::Character => id.parse::<u64>().ok().is_some_and(|candidate| {
            candidate == actor_id
                && ctx
                    .db
                    .character()
                    .id()
                    .find(candidate)
                    .is_some_and(|character| character.alive)
        }),
        CharacterCustodianKind::Party => ctx
            .db
            .party_authority()
            .id()
            .find(&id.to_owned())
            .is_some_and(|party| party.leader_id == actor_id),
        CharacterCustodianKind::Site => ctx
            .db
            .character()
            .id()
            .find(actor_id)
            .and_then(|character| character.party_id)
            .and_then(|party_id| ctx.db.party_authority().id().find(&party_id))
            .is_some_and(|party| {
                party.leader_id == actor_id
                    && party
                        .current_case_site_id
                        .is_some_and(|site| site.value == id)
                    && ctx
                        .db
                        .case_site_authority()
                        .id_key()
                        .find(&id.to_owned())
                        .is_some()
            }),
        CharacterCustodianKind::Faction => false,
        CharacterCustodianKind::None => false,
    }
}
fn actor_controls_custody(ctx: &ReducerContext, actor_id: u64, row: &CharacterCustody) -> bool {
    custodian_exists_and_controlled(ctx, actor_id, row.custodian_kind, &row.custodian_id)
}
fn actor_target_colocated(
    ctx: &ReducerContext,
    actor_id: u64,
    target_id: u64,
    context_id: &str,
) -> bool {
    let Some(actor) = ctx.db.character().id().find(actor_id) else {
        return false;
    };
    let Some(target) = ctx.db.character().id().find(target_id) else {
        return false;
    };
    if actor.current_settlement_id.is_some()
        && actor.current_settlement_id == target.current_settlement_id
    {
        return true;
    }
    let Some(party_id) = actor.party_id else {
        return false;
    };
    let aware = ctx
        .db
        .party_context_contact_authority()
        .party_id()
        .filter(&party_id)
        .any(|contact| contact.context_id == context_id && contact.mutual_awareness);
    let present = ctx
        .db
        .character_context_membership()
        .context_id()
        .filter(context_id)
        .any(|membership| {
            membership.character_id == target_id
                && (membership.active
                    || ctx
                        .db
                        .character_context_disposition_authority()
                        .id()
                        .find(&format!("{context_id}:{target_id}"))
                        .is_some_and(|row| row.disposition == DispositionKind::Surrendered))
        });
    aware && present
}
fn custody_observer_party(
    ctx: &ReducerContext,
    character_id: u64,
    kind: CharacterCustodianKind,
    id: &str,
) -> String {
    if kind == CharacterCustodianKind::Party {
        return id.into();
    }
    ctx.db
        .character()
        .id()
        .find(character_id)
        .and_then(|character| character.party_id)
        .unwrap_or_default()
}

fn transition_character_custody(
    ctx: &ReducerContext,
    character_id: u64,
    expected_version: u32,
    status: CharacterCustodyStatus,
    custodian_kind: CharacterCustodianKind,
    custodian_id: String,
    ransom_minor: Option<u64>,
    context_id: String,
    case_id: Option<String>,
    source_id: String,
) -> Result<(), String> {
    if let Some(r) = ctx
        .db
        .character_custody_receipt()
        .source_id()
        .find(&source_id)
    {
        if r.character_id == character_id
            && r.expected_version == expected_version
            && r.status == status
            && r.custodian_kind == custodian_kind
            && r.custodian_id == custodian_id
            && r.context_id == context_id
        {
            return Ok(());
        }
        return Err("Conflicting custody source ID reuse".into());
    }
    if !valid_systemic_id(&source_id) {
        return Err("Invalid custody source ID".into());
    }
    let current = ctx.db.character_custody().character_id().find(character_id);
    let actual = current.as_ref().map_or(0, |row| row.version);
    if actual != expected_version {
        return Err("Stale custody version".into());
    }
    if (status == CharacterCustodyStatus::Captive)
        != (custodian_kind != CharacterCustodianKind::None && !custodian_id.is_empty())
    {
        return Err("Custody state and typed custodian disagree".into());
    }
    let version = actual.checked_add(1).ok_or("Custody version overflow")?;
    let observer_party_id =
        custody_observer_party(ctx, character_id, custodian_kind, &custodian_id);
    let row = CharacterCustody {
        character_id,
        observer_party_id,
        status,
        custodian_kind,
        custodian_id: custodian_id.clone(),
        version,
        source_id: source_id.clone(),
        context_id: context_id.clone(),
        case_id,
        ransom_minor,
    };
    if current.is_some() {
        ctx.db.character_custody().character_id().update(row);
    } else {
        ctx.db.character_custody().insert(row);
    }
    ctx.db
        .character_custody_receipt()
        .insert(CharacterCustodyReceipt {
            source_id,
            character_id,
            expected_version,
            resulting_version: version,
            status,
            custodian_kind,
            custodian_id,
            context_id,
        });
    Ok(())
}

fn ensure_custody_retry(
    ctx: &ReducerContext,
    source_id: &str,
    character_id: u64,
    expected_version: u32,
    status: CharacterCustodyStatus,
    custodian_kind: CharacterCustodianKind,
    custodian_id: &str,
    context_id: Option<&str>,
) -> Result<bool, String> {
    let Some(receipt) = ctx
        .db
        .character_custody_receipt()
        .source_id()
        .find(&source_id.to_owned())
    else {
        return Ok(false);
    };
    if receipt.character_id == character_id
        && receipt.expected_version == expected_version
        && receipt.status == status
        && receipt.custodian_kind == custodian_kind
        && receipt.custodian_id == custodian_id
        && context_id.is_none_or(|context| receipt.context_id == context)
    {
        Ok(true)
    } else {
        Err("Conflicting custody source ID reuse".into())
    }
}

#[reducer]
pub fn capture_character(
    ctx: &ReducerContext,
    actor_id: u64,
    character_id: u64,
    context_id: String,
    expected_version: u32,
    custodian_kind: CharacterCustodianKind,
    custodian_id: String,
    ransom_minor: Option<u64>,
    source_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, actor_id)?;
    if ensure_custody_retry(
        ctx,
        &source_id,
        character_id,
        expected_version,
        CharacterCustodyStatus::Captive,
        custodian_kind,
        &custodian_id,
        Some(&context_id),
    )? {
        return Ok(());
    }
    let current = ctx.db.character_custody().character_id().find(character_id);
    let yielded = ctx
        .db
        .character_context_disposition_authority()
        .id()
        .find(&format!("{context_id}:{character_id}"))
        .is_some_and(|row| row.disposition == DispositionKind::Surrendered);
    let incapacitated = ctx
        .db
        .character_strategic_condition()
        .character_id()
        .find(character_id)
        .is_some_and(|condition| condition.incapacitation >= 1.0);
    let destination_controlled =
        custodian_exists_and_controlled(ctx, actor_id, custodian_kind, &custodian_id);
    let colocated = actor_target_colocated(ctx, actor_id, character_id, &context_id);
    adventuresim_core::systemic_character::validate_custody_transition(
        &adventuresim_core::systemic_character::CustodyTransition {
            current: current.as_ref().map(|row| {
                (
                    core_custody_status(row.status),
                    stored_custodian(row.custodian_kind, &row.custodian_id),
                )
            }),
            action: adventuresim_core::systemic_character::CustodyAction::Capture,
            destination: stored_custodian(custodian_kind, &custodian_id),
            target_surrendered_or_incapacitated: yielded || incapacitated,
            actor_controls_current: false,
            actor_controls_destination: destination_controlled,
            actor_is_captive: false,
            co_located: colocated,
        },
    )
    .map_err(|_| "Invalid or unauthorized capture transition")?;
    let case_id = validated_case_provenance(ctx, &context_id, character_id)?;
    transition_character_custody(
        ctx,
        character_id,
        expected_version,
        CharacterCustodyStatus::Captive,
        custodian_kind,
        custodian_id,
        ransom_minor,
        context_id,
        case_id,
        source_id,
    )
}
#[reducer]
pub fn handoff_character_custody(
    ctx: &ReducerContext,
    actor_id: u64,
    character_id: u64,
    expected_version: u32,
    custodian_kind: CharacterCustodianKind,
    custodian_id: String,
    source_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, actor_id)?;
    if ensure_custody_retry(
        ctx,
        &source_id,
        character_id,
        expected_version,
        CharacterCustodyStatus::Captive,
        custodian_kind,
        &custodian_id,
        None,
    )? {
        return Ok(());
    }
    let cur = ctx
        .db
        .character_custody()
        .character_id()
        .find(character_id)
        .ok_or("Character is not in custody")?;
    adventuresim_core::systemic_character::validate_custody_transition(
        &adventuresim_core::systemic_character::CustodyTransition {
            current: Some((
                core_custody_status(cur.status),
                stored_custodian(cur.custodian_kind, &cur.custodian_id),
            )),
            action: adventuresim_core::systemic_character::CustodyAction::Handoff,
            destination: stored_custodian(custodian_kind, &custodian_id),
            target_surrendered_or_incapacitated: true,
            actor_controls_current: actor_controls_custody(ctx, actor_id, &cur),
            actor_controls_destination: custodian_exists_and_controlled(
                ctx,
                actor_id,
                custodian_kind,
                &custodian_id,
            ),
            actor_is_captive: false,
            co_located: actor_target_colocated(ctx, actor_id, character_id, &cur.context_id),
        },
    )
    .map_err(|_| "Invalid or unauthorized custody handoff")?;
    let case = cur.case_id.clone();
    let context = cur.context_id.clone();
    transition_character_custody(
        ctx,
        character_id,
        expected_version,
        CharacterCustodyStatus::Captive,
        custodian_kind,
        custodian_id.clone(),
        cur.ransom_minor,
        context,
        cur.case_id,
        source_id.clone(),
    )?;
    if let Some(case_id) = case {
        let party = ctx
            .db
            .character()
            .id()
            .find(actor_id)
            .and_then(|character| character.party_id)
            .ok_or("Custody fact actor has no party")?;
        ingest_case_outcome_fact(
            ctx,
            &source_id,
            &case_id,
            &party,
            adventuresim_core::case::OutcomeFactKind::CustodyHandedOff {
                character_id,
                custodian_id,
            },
        )?;
    }
    Ok(())
}
#[reducer]
pub fn release_character(
    ctx: &ReducerContext,
    actor_id: u64,
    character_id: u64,
    expected_version: u32,
    source_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, actor_id)?;
    if ensure_custody_retry(
        ctx,
        &source_id,
        character_id,
        expected_version,
        CharacterCustodyStatus::Released,
        CharacterCustodianKind::None,
        "",
        None,
    )? {
        return Ok(());
    }
    let cur = ctx
        .db
        .character_custody()
        .character_id()
        .find(character_id)
        .ok_or("Character is not in custody")?;
    adventuresim_core::systemic_character::validate_custody_transition(
        &adventuresim_core::systemic_character::CustodyTransition {
            current: Some((
                core_custody_status(cur.status),
                stored_custodian(cur.custodian_kind, &cur.custodian_id),
            )),
            action: adventuresim_core::systemic_character::CustodyAction::Release,
            destination: adventuresim_core::systemic_character::Custodian::None,
            target_surrendered_or_incapacitated: true,
            actor_controls_current: actor_controls_custody(ctx, actor_id, &cur),
            actor_controls_destination: false,
            actor_is_captive: false,
            co_located: actor_target_colocated(ctx, actor_id, character_id, &cur.context_id),
        },
    )
    .map_err(|_| "Invalid or unauthorized custody release")?;
    transition_character_custody(
        ctx,
        character_id,
        expected_version,
        CharacterCustodyStatus::Released,
        CharacterCustodianKind::None,
        String::new(),
        None,
        cur.context_id,
        cur.case_id,
        source_id,
    )
}
#[reducer]
pub fn escape_character_custody(
    ctx: &ReducerContext,
    actor_id: u64,
    character_id: u64,
    expected_version: u32,
    source_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, actor_id)?;
    if ensure_custody_retry(
        ctx,
        &source_id,
        character_id,
        expected_version,
        CharacterCustodyStatus::Escaped,
        CharacterCustodianKind::None,
        "",
        None,
    )? {
        return Ok(());
    }
    let cur = ctx
        .db
        .character_custody()
        .character_id()
        .find(character_id)
        .ok_or("Character is not in custody")?;
    adventuresim_core::systemic_character::validate_custody_transition(
        &adventuresim_core::systemic_character::CustodyTransition {
            current: Some((
                core_custody_status(cur.status),
                stored_custodian(cur.custodian_kind, &cur.custodian_id),
            )),
            action: adventuresim_core::systemic_character::CustodyAction::Escape,
            destination: adventuresim_core::systemic_character::Custodian::None,
            target_surrendered_or_incapacitated: false,
            actor_controls_current: false,
            actor_controls_destination: false,
            actor_is_captive: actor_id == character_id,
            co_located: false,
        },
    )
    .map_err(|_| "Only the current captive may escape captivity")?;
    let case = cur.case_id.clone();
    transition_character_custody(
        ctx,
        character_id,
        expected_version,
        CharacterCustodyStatus::Escaped,
        CharacterCustodianKind::None,
        String::new(),
        None,
        cur.context_id,
        cur.case_id,
        source_id.clone(),
    )?;
    if let Some(case_id) = case {
        let party = ctx
            .db
            .character()
            .id()
            .find(actor_id)
            .and_then(|character| character.party_id)
            .ok_or("Escape fact actor has no party")?;
        ingest_case_outcome_fact(
            ctx,
            &source_id,
            &case_id,
            &party,
            adventuresim_core::case::OutcomeFactKind::CharacterEscaped { character_id },
        )?;
    }
    Ok(())
}

fn transfer_ransom_currency(
    ctx: &ReducerContext,
    actor_id: u64,
    payer_party_id: &str,
    recipient_kind: CharacterCustodianKind,
    recipient_id: &str,
    amount: u64,
    source_id: &str,
) -> Result<(), String> {
    let to_owner_kind = match recipient_kind {
        CharacterCustodianKind::Party => {
            ctx.db
                .party_authority()
                .id()
                .find(&recipient_id.to_owned())
                .ok_or("Ransom recipient party does not exist")?;
            LegalOwnerKind::Party
        }
        CharacterCustodianKind::Character => {
            let id = recipient_id
                .parse::<u64>()
                .map_err(|_| "Ransom character recipient is invalid")?;
            ctx.db
                .character()
                .id()
                .find(id)
                .filter(|character| character.alive)
                .ok_or("Ransom character recipient does not exist")?;
            LegalOwnerKind::Personal
        }
        _ => return Err("Ransom custodian has no concrete currency recipient".into()),
    };
    let stacks: Vec<_> = ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(payer_party_id)
        .filter(|stack| crate::item::is_currency(ctx, &stack.item_id))
        .collect();
    let mut remaining = amount;
    for stack in stacks {
        if remaining == 0 {
            break;
        }
        let moved = u64::from(stack.quantity).min(remaining);
        let property_id = format!("party-inventory:{}", stack.id);
        materialize_inventory_property(ctx, &property_id, LegalOwnerKind::Party, payer_party_id)?;
        let version = ctx
            .db
            .legal_property()
            .id()
            .find(&property_id)
            .ok_or("Ransom currency property disappeared")?
            .version;
        transfer_legal_property(
            ctx,
            actor_id,
            property_id,
            moved,
            LegalOwnerKind::Party,
            payer_party_id.into(),
            version,
            to_owner_kind,
            recipient_id.into(),
            format!("{source_id}:currency:{}", stack.id),
        )?;
        remaining -= moved;
    }
    if remaining != 0 {
        return Err("Insufficient party currency for ransom".into());
    }
    Ok(())
}
#[reducer]
pub fn pay_character_ransom(
    ctx: &ReducerContext,
    actor_id: u64,
    character_id: u64,
    expected_version: u32,
    source_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, actor_id)?;
    if !valid_systemic_id(&source_id) || source_id.len() > 96 {
        return Err("Invalid ransom source ID".into());
    }
    let party = ctx
        .db
        .character()
        .id()
        .find(actor_id)
        .and_then(|character| character.party_id)
        .ok_or("Actor has no party")?;
    if let Some(receipt) = ctx.db.ransom_payment_receipt().source_id().find(&source_id) {
        return if receipt.actor_id == actor_id
            && receipt.character_id == character_id
            && receipt.expected_version == expected_version
            && receipt.payer_party_id == party
        {
            Ok(())
        } else {
            Err("Conflicting ransom source ID reuse".into())
        };
    }
    if !ctx
        .db
        .party_authority()
        .id()
        .find(&party)
        .is_some_and(|row| row.leader_id == actor_id)
    {
        return Err("Only the payer party leader may authorize ransom payment".into());
    }
    let cur = ctx
        .db
        .character_custody()
        .character_id()
        .find(character_id)
        .ok_or("Character is not in custody")?;
    let amount = cur.ransom_minor.ok_or("No ransom terms were offered")?;
    adventuresim_core::systemic_character::validate_custody_transition(
        &adventuresim_core::systemic_character::CustodyTransition {
            current: Some((
                core_custody_status(cur.status),
                stored_custodian(cur.custodian_kind, &cur.custodian_id),
            )),
            action: adventuresim_core::systemic_character::CustodyAction::RansomRelease,
            destination: adventuresim_core::systemic_character::Custodian::None,
            target_surrendered_or_incapacitated: true,
            actor_controls_current: false,
            actor_controls_destination: false,
            actor_is_captive: false,
            co_located: actor_target_colocated(ctx, actor_id, character_id, &cur.context_id),
        },
    )
    .map_err(|_| "Ransom release terms are no longer valid")?;
    let recipient_kind = cur.custodian_kind;
    let recipient_id = cur.custodian_id.clone();
    transfer_ransom_currency(
        ctx,
        actor_id,
        &party,
        recipient_kind,
        &recipient_id,
        amount,
        &source_id,
    )?;
    let case = cur.case_id.clone();
    transition_character_custody(
        ctx,
        character_id,
        expected_version,
        CharacterCustodyStatus::Released,
        CharacterCustodianKind::None,
        String::new(),
        None,
        cur.context_id,
        cur.case_id,
        format!("{source_id}:release"),
    )?;
    ctx.db
        .ransom_payment_receipt()
        .insert(RansomPaymentReceipt {
            source_id: source_id.clone(),
            actor_id,
            character_id,
            expected_version,
            payer_party_id: party.clone(),
            recipient_kind,
            recipient_id: recipient_id.clone(),
            amount_minor: amount,
        });
    if let Some(case_id) = case {
        ingest_case_outcome_fact(
            ctx,
            &source_id,
            &case_id,
            &party,
            adventuresim_core::case::OutcomeFactKind::RansomPaid {
                character_id,
                recipient_id,
            },
        )?;
    }
    Ok(())
}

/// Sole party-membership transfer primitive used by recruitment and defection.
/// Reducer atomicity rolls back every mutation on any subsequent error.
fn transfer_character_party_membership(
    ctx: &ReducerContext,
    actor_id: u64,
    character_id: u64,
    destination_party_id: &str,
    source_id: &str,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, actor_id)?;
    let mut character = crate::character::require_living_character(ctx, character_id)?;
    let destination = ctx
        .db
        .party_authority()
        .id()
        .find(&destination_party_id.to_owned())
        .ok_or("Destination party not found")?;
    if ctx
        .db
        .party_member()
        .party_id()
        .filter(destination_party_id)
        .count()
        >= 8
    {
        return Err("Destination party is at capacity".into());
    }
    let actor = ctx
        .db
        .character()
        .id()
        .find(actor_id)
        .ok_or("Actor not found")?;
    let colocated = match (
        actor.current_settlement_id.as_deref(),
        character.current_settlement_id.as_deref(),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => actor.party_id.as_deref().is_some_and(|party_id| {
            ctx.db
                .character_context_membership()
                .character_id()
                .filter(character_id)
                .any(|membership| {
                    ctx.db
                        .party_context_contact_authority()
                        .party_id()
                        .filter(party_id)
                        .any(|contact| {
                            contact.context_id == membership.context_id && contact.mutual_awareness
                        })
                })
        }),
    };
    if !colocated {
        return Err("Recruit and party must share an authoritative location".into());
    }
    let memberships: Vec<_> = ctx
        .db
        .party_member()
        .character_id()
        .filter(character_id)
        .collect();
    if memberships.len() > 1 {
        return Err("Character has conflicting party memberships".into());
    }
    if let Some(old) = memberships.first() {
        ctx.db.party_member().id().delete(old.id);
    }
    character.party_id = Some(destination_party_id.into());
    ctx.db.character().id().update(character);
    ctx.db.party_member().insert(PartyMember {
        id: 0,
        party_id: destination.id,
        character_id,
        role: Some(format!("recruited:{source_id}")),
        recruitment_role_id: None,
    });
    Ok(())
}

#[derive(Clone, Debug)]
#[table(accessor=recruitment_transfer_receipt)]
pub struct RecruitmentTransferReceipt {
    #[primary_key]
    pub source_id: String,
    pub character_id: u64,
    pub from_party_id: Option<String>,
    pub to_party_id: String,
    pub context_id: String,
    pub contact_ref: String,
    pub expected_disposition_revision: u32,
}
#[reducer]
pub fn recruit_context_character(
    ctx: &ReducerContext,
    actor_id: u64,
    character_id: u64,
    destination_party_id: String,
    contact_ref: String,
    expected_disposition_revision: u32,
    source_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, actor_id)?;
    if !valid_systemic_id(&source_id)
        || !valid_systemic_id(&destination_party_id)
        || !valid_systemic_id(&contact_ref)
    {
        return Err("Invalid recruitment coordinates".into());
    }
    if let Some(receipt) = ctx
        .db
        .recruitment_transfer_receipt()
        .source_id()
        .find(&source_id)
    {
        return if receipt.character_id == character_id
            && receipt.to_party_id == destination_party_id
            && receipt.contact_ref == contact_ref
            && receipt.expected_disposition_revision == expected_disposition_revision
        {
            Ok(())
        } else {
            Err("Conflicting recruitment source ID reuse".into())
        };
    }
    let actor = ctx
        .db
        .character()
        .id()
        .find(actor_id)
        .ok_or("Actor not found")?;
    let actor_party_id = actor.party_id.as_deref().ok_or("Actor has no party")?;
    let destination = ctx
        .db
        .party_authority()
        .id()
        .find(&destination_party_id)
        .ok_or("Destination party not found")?;
    let contact = ctx
        .db
        .party_context_contact_authority()
        .party_id()
        .filter(actor_party_id)
        .find(|contact| {
            (contact.location_id == contact_ref || contact.context_id == contact_ref)
                && contact.mutual_awareness
        })
        .ok_or("Recruitment requires an active mutually aware contact")?;
    let context_id = contact.context_id.clone();
    let _membership = ctx
        .db
        .character_context_membership()
        .context_id()
        .filter(&context_id)
        .find(|membership| membership.character_id == character_id)
        .ok_or("Recruit is not in the explicit active context")?;
    let disposition = ctx
        .db
        .character_context_disposition_authority()
        .id()
        .find(&format!("{context_id}:{character_id}"))
        .ok_or("Recruit has no disposition in this context")?;
    let existing_grant = ctx
        .db
        .browser_character_grant()
        .character_id()
        .find(character_id)
        .is_some();
    let captive = ctx
        .db
        .character_custody()
        .character_id()
        .find(character_id)
        .is_some_and(|row| row.status == CharacterCustodyStatus::Captive);
    adventuresim_core::systemic_character::validate_recruitment(
        &adventuresim_core::systemic_character::RecruitmentPreflight {
            destination_exists: true,
            actor_leads_destination: destination.leader_id == actor_id
                && actor_party_id == destination_party_id,
            active_contact: true,
            mutual_awareness: contact.mutual_awareness,
            co_located: actor_target_colocated(ctx, actor_id, character_id, &context_id),
            disposition: core_disposition(disposition.disposition),
            expected_revision: expected_disposition_revision,
            actual_revision: disposition.revision,
            captive,
            existing_control_grant: existing_grant,
        },
    )
    .map_err(|_| "Recruitment consent, authority, or context preflight failed")?;
    let owner_key = ctx
        .db
        .browser_character_grant()
        .character_id()
        .find(actor_id)
        .ok_or("Recruiting actor has no canonical browser owner")?
        .owner_key;
    let case_id = validated_case_provenance(ctx, &context_id, character_id)?;
    let from = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?
        .party_id;
    crate::browser_session::grant_recruited_character_internal(
        ctx,
        &owner_key,
        character_id,
        &source_id,
    )?;
    transfer_character_party_membership(
        ctx,
        actor_id,
        character_id,
        &destination_party_id,
        &source_id,
    )?;
    ctx.db
        .recruitment_transfer_receipt()
        .insert(RecruitmentTransferReceipt {
            source_id: source_id.clone(),
            character_id,
            from_party_id: from,
            to_party_id: destination_party_id.clone(),
            context_id,
            contact_ref,
            expected_disposition_revision,
        });
    if let Some(case_id) = case_id {
        ingest_case_outcome_fact(
            ctx,
            &source_id,
            &case_id,
            &destination_party_id,
            adventuresim_core::case::OutcomeFactKind::CharacterRecruited {
                character_id,
                party_id: destination_party_id.clone(),
            },
        )?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum LegalOwnerKind {
    Personal,
    Party,
    Faction,
    Abandoned,
    Corpse,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum PropertyKind {
    Item,
    Currency,
}
#[derive(Clone, Debug)]
#[table(accessor=systemic_escrow_lot)]
pub struct SystemicEscrowLot {
    #[primary_key]
    pub id: String,
    pub holder_id: String,
    pub context_id: String,
    pub item_id: String,
    pub quantity: u64,
}
#[derive(Clone, Debug)]
#[table(accessor=legal_property)]
pub struct LegalProperty {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub scope_owner_key: String,
    pub kind: PropertyKind,
    pub item_id: String,
    pub quantity: u64,
    pub owner_kind: LegalOwnerKind,
    pub owner_id: String,
    pub physical_holder_id: String,
    pub physical_binding_id: String,
    pub version: u32,
    pub provenance: String,
    pub metadata: String,
    pub case_id: Option<String>,
}
#[derive(Clone, Debug)]
#[table(accessor=property_transfer_receipt)]
pub struct PropertyTransferReceipt {
    #[primary_key]
    pub source_id: String,
    pub property_id: String,
    pub physical_binding_id: String,
    pub quantity: u64,
    pub expected_version: u32,
    pub from_owner_kind: LegalOwnerKind,
    pub from_owner_id: String,
    pub to_owner_kind: LegalOwnerKind,
    pub to_owner_id: String,
    pub resulting_version: u32,
}
#[derive(Clone, Debug)]
#[table(accessor=property_transfer_event)]
pub struct PropertyTransferEvent {
    #[primary_key]
    pub source_id: String,
    pub actor_id: u64,
    pub victim_id: String,
    pub property_id: String,
    pub location_id: String,
    pub happened_micros: i64,
    pub theft: bool,
    pub witness_character_ids: Vec<u64>,
}
#[derive(Clone, Debug)]
#[table(accessor=property_event_observer_scope)]
pub struct PropertyEventObserverScope {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub observer_party_id: String,
    pub source_id: String,
    pub happened_micros: i64,
    pub witnessed: bool,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendLegalProperty {
    pub observer_party_id: String,
    pub property_id: String,
    pub kind: PropertyKind,
    pub item_id: String,
    pub quantity: u64,
    pub owner_kind: LegalOwnerKind,
    pub owner_id: String,
    pub version: u32,
}
#[view(accessor=backend_legal_properties,public)]
pub fn backend_legal_properties(ctx: &ViewContext) -> Vec<BackendLegalProperty> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for party in ctx.db.party_authority().gateway_bucket().filter(0u8) {
        let mut rows: Vec<_> = ctx
            .db
            .legal_property()
            .scope_owner_key()
            .filter(&format!("party:{}", party.id))
            .collect();
        for member in ctx.db.party_member().party_id().filter(&party.id) {
            rows.extend(
                ctx.db
                    .legal_property()
                    .scope_owner_key()
                    .filter(&format!("personal:{}", member.character_id)),
            );
        }
        for row in rows {
            out.push(BackendLegalProperty {
                observer_party_id: party.id.clone(),
                property_id: row.id,
                item_id: row.item_id,
                kind: row.kind,
                quantity: row.quantity,
                owner_kind: row.owner_kind,
                owner_id: row.owner_id,
                version: row.version,
            });
        }
    }
    out
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendPropertyEvent {
    pub observer_party_id: String,
    pub source_id: String,
    pub actor_id: u64,
    pub victim_id: String,
    pub property_id: String,
    pub location_id: String,
    pub theft: bool,
    pub witnessed: bool,
}
#[view(accessor=backend_property_events,public)]
pub fn backend_property_events(ctx: &ViewContext) -> Vec<BackendPropertyEvent> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for party in ctx.db.party_authority().gateway_bucket().filter(0u8) {
        let mut scopes: Vec<_> = ctx
            .db
            .property_event_observer_scope()
            .observer_party_id()
            .filter(&party.id)
            .collect();
        scopes.sort_by_key(|scope| std::cmp::Reverse(scope.happened_micros));
        for scope in scopes.into_iter().take(256) {
            if let Some(event) = ctx
                .db
                .property_transfer_event()
                .source_id()
                .find(&scope.source_id)
            {
                out.push(BackendPropertyEvent {
                    observer_party_id: party.id.clone(),
                    source_id: event.source_id.clone(),
                    actor_id: event.actor_id,
                    victim_id: event.victim_id.clone(),
                    property_id: event.property_id.clone(),
                    location_id: event.location_id.clone(),
                    theft: event.theft,
                    witnessed: scope.witnessed,
                });
            }
        }
    }
    out
}

fn legal_scope(owner_kind: LegalOwnerKind, owner_id: &str) -> String {
    match owner_kind {
        LegalOwnerKind::Personal => format!("personal:{owner_id}"),
        LegalOwnerKind::Party => format!("party:{owner_id}"),
        LegalOwnerKind::Faction => format!("faction:{owner_id}"),
        LegalOwnerKind::Abandoned => format!("abandoned:{owner_id}"),
        LegalOwnerKind::Corpse => format!("corpse:{owner_id}"),
    }
}

fn core_owner(k: LegalOwnerKind) -> adventuresim_core::systemic_character::PropertyOwnerKind {
    match k {
        LegalOwnerKind::Personal => {
            adventuresim_core::systemic_character::PropertyOwnerKind::Personal
        }
        LegalOwnerKind::Party => adventuresim_core::systemic_character::PropertyOwnerKind::Party,
        LegalOwnerKind::Faction => {
            adventuresim_core::systemic_character::PropertyOwnerKind::Faction
        }
        LegalOwnerKind::Abandoned => {
            adventuresim_core::systemic_character::PropertyOwnerKind::Abandoned
        }
        LegalOwnerKind::Corpse => adventuresim_core::systemic_character::PropertyOwnerKind::Corpse,
    }
}

fn materialize_inventory_property(
    ctx: &ReducerContext,
    property_id: &str,
    owner_kind: LegalOwnerKind,
    owner_id: &str,
) -> Result<(), String> {
    if ctx
        .db
        .legal_property()
        .id()
        .find(&property_id.to_owned())
        .is_some()
    {
        return Ok(());
    }
    let (item_id, quantity, holder, binding, metadata) = if let Some(id) = property_id
        .strip_prefix("inventory:")
        .and_then(|id| id.parse::<u64>().ok())
    {
        let row = ctx
            .db
            .inventory_item()
            .id()
            .find(id)
            .ok_or("Personal inventory property not found")?;
        if owner_kind != LegalOwnerKind::Personal
            || owner_id.parse::<u64>().ok() != Some(row.character_id)
        {
            return Err("Personal inventory owner mismatch".into());
        }
        let metadata = ctx
            .db
            .item_condition()
            .inventory_item_id()
            .find(id)
            .map_or_else(String::new, |c| {
                format!(
                    "condition:{},{},{},{},{}",
                    c.tier_1, c.tier_2, c.tier_3, c.tier_4, c.tier_5
                )
            });
        (
            row.item_id,
            u64::from(row.quantity),
            format!("character:{}", row.character_id),
            format!("inventory:{id}"),
            metadata,
        )
    } else if let Some(id) = property_id
        .strip_prefix("party-inventory:")
        .and_then(|id| id.parse::<u64>().ok())
    {
        let row = ctx
            .db
            .party_inventory_item()
            .id()
            .find(id)
            .ok_or("Party inventory property not found")?;
        if owner_kind != LegalOwnerKind::Party || owner_id != row.party_id {
            return Err("Party inventory owner mismatch".into());
        }
        let metadata = ctx
            .db
            .party_item_condition()
            .party_inventory_item_id()
            .find(id)
            .map_or_else(String::new, |c| {
                format!(
                    "condition:{},{},{},{},{}",
                    c.tier_1, c.tier_2, c.tier_3, c.tier_4, c.tier_5
                )
            });
        (
            row.item_id,
            u64::from(row.quantity),
            format!("party:{}", row.party_id),
            format!("party-inventory:{id}"),
            metadata,
        )
    } else {
        return Err("Property has no trusted inventory or owning-system authority".into());
    };
    let kind = if crate::item::is_currency(ctx, &item_id) {
        PropertyKind::Currency
    } else {
        PropertyKind::Item
    };
    ctx.db.legal_property().insert(LegalProperty {
        id: property_id.into(),
        scope_owner_key: legal_scope(owner_kind, owner_id),
        kind,
        item_id,
        quantity,
        owner_kind,
        owner_id: owner_id.into(),
        physical_holder_id: holder,
        physical_binding_id: binding,
        version: 0,
        provenance: "ordinary-inventory".into(),
        metadata,
        case_id: None,
    });
    Ok(())
}

fn move_physical_inventory(
    ctx: &ReducerContext,
    binding_id: &str,
    item_id: &str,
    quantity: u64,
    to_kind: LegalOwnerKind,
    to_id: &str,
) -> Result<String, String> {
    let quantity =
        u32::try_from(quantity).map_err(|_| "Transfer quantity exceeds inventory range")?;
    if let Some(id) = binding_id
        .strip_prefix("inventory:")
        .and_then(|id| id.parse::<u64>().ok())
    {
        let mut row = ctx
            .db
            .inventory_item()
            .id()
            .find(id)
            .ok_or("Physical personal inventory changed")?;
        if row.item_id != item_id {
            return Err("Physical personal inventory binding changed".into());
        }
        row.quantity = row
            .quantity
            .checked_sub(quantity)
            .ok_or("Physical personal inventory underflow")?;
        if row.quantity == 0 {
            crate::character::unequip_wearable(ctx, id);
            ctx.db.item_condition().inventory_item_id().delete(id);
            ctx.db.inventory_item().id().delete(id);
        } else {
            ctx.db.inventory_item().id().update(row);
        }
    } else if let Some(id) = binding_id
        .strip_prefix("party-inventory:")
        .and_then(|id| id.parse::<u64>().ok())
    {
        let mut row = ctx
            .db
            .party_inventory_item()
            .id()
            .find(id)
            .ok_or("Physical party inventory changed")?;
        if row.item_id != item_id {
            return Err("Physical party inventory binding changed".into());
        }
        row.quantity = row
            .quantity
            .checked_sub(quantity)
            .ok_or("Physical party inventory underflow")?;
        if row.quantity == 0 {
            ctx.db
                .party_item_condition()
                .party_inventory_item_id()
                .delete(id);
            ctx.db.party_inventory_item().id().delete(id);
        } else {
            ctx.db.party_inventory_item().id().update(row);
        }
    } else if let Some(id) = binding_id.strip_prefix("escrow:") {
        let mut row = ctx
            .db
            .systemic_escrow_lot()
            .id()
            .find(&id.to_owned())
            .ok_or("Physical escrow lot changed")?;
        if row.item_id != item_id {
            return Err("Physical escrow binding changed".into());
        }
        row.quantity = row
            .quantity
            .checked_sub(u64::from(quantity))
            .ok_or("Physical escrow underflow")?;
        if row.quantity == 0 {
            ctx.db.systemic_escrow_lot().id().delete(&id.to_owned());
        } else {
            ctx.db.systemic_escrow_lot().id().update(row);
        }
    } else {
        return Err("Legal property has no exact physical inventory binding".into());
    }
    match to_kind {
        LegalOwnerKind::Personal => {
            let character_id = to_id
                .parse::<u64>()
                .map_err(|_| "Personal destination ID is invalid")?;
            let id = crate::item::add_inventory_item_checked(ctx, character_id, item_id, quantity)?
                .ok_or("Personal destination lot was not created")?;
            Ok(format!("inventory:{id}"))
        }
        LegalOwnerKind::Party => {
            let kind = ctx
                .db
                .item()
                .id()
                .find(&item_id.to_owned())
                .map(|item| item.kind);
            let individual = kind.is_some_and(|kind| {
                matches!(kind, crate::ItemKind::Food | crate::ItemKind::Medication)
            }) || ctx
                .db
                .item()
                .id()
                .find(&item_id.to_owned())
                .is_some_and(|item| item.repairable)
                || crate::inventory_amount::is_measured_item(ctx, item_id);
            if individual {
                if quantity != 1 {
                    return Err(
                        "Individual inventory lots must transfer one physical row at a time".into(),
                    );
                }
                let before: Vec<u64> = ctx
                    .db
                    .party_inventory_item()
                    .party_id()
                    .filter(to_id)
                    .map(|row| row.id)
                    .collect();
                add_to_party_inventory_checked(ctx, to_id, item_id, quantity)?;
                let id = ctx
                    .db
                    .party_inventory_item()
                    .party_id()
                    .filter(to_id)
                    .find(|row| row.item_id == item_id && !before.contains(&row.id))
                    .map(|row| row.id)
                    .ok_or("Party destination lot was not created")?;
                Ok(format!("party-inventory:{id}"))
            } else {
                let row = ctx.db.party_inventory_item().insert(PartyInventoryItem {
                    id: 0,
                    party_id: to_id.into(),
                    item_id: item_id.into(),
                    quantity,
                });
                Ok(format!("party-inventory:{}", row.id))
            }
        }
        LegalOwnerKind::Faction | LegalOwnerKind::Abandoned | LegalOwnerKind::Corpse => {
            Err("Destination owner has no supported physical inventory adapter".into())
        }
    }
}

fn unique_context_for_character(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<Option<String>, String> {
    let mut contexts: Vec<String> = ctx
        .db
        .character_context_membership()
        .character_id()
        .filter(character_id)
        .filter(|membership| {
            membership.active
                || ctx
                    .db
                    .character_context_disposition_authority()
                    .id()
                    .find(&format!("{}:{character_id}", membership.context_id))
                    .is_some_and(|row| row.disposition == DispositionKind::Surrendered)
        })
        .map(|membership| membership.context_id)
        .collect();
    contexts.sort();
    contexts.dedup();
    if contexts.len() > 1 {
        return Err("Physical holder has ambiguous active context location".into());
    }
    Ok(contexts.pop())
}
fn authoritative_holder_location(ctx: &ReducerContext, binding_id: &str) -> Result<String, String> {
    if let Some(id) = binding_id
        .strip_prefix("inventory:")
        .and_then(|id| id.parse::<u64>().ok())
    {
        let row = ctx
            .db
            .inventory_item()
            .id()
            .find(id)
            .ok_or("Physical holder row not found")?;
        let character = ctx
            .db
            .character()
            .id()
            .find(row.character_id)
            .ok_or("Physical holder character not found")?;
        if let Some(settlement) = character.current_settlement_id {
            return Ok(format!("settlement:{settlement}"));
        }
        return unique_context_for_character(ctx, character.id)?
            .map(|context| format!("context:{context}"))
            .ok_or("Physical holder has no authoritative location".into());
    }
    if let Some(id) = binding_id
        .strip_prefix("party-inventory:")
        .and_then(|id| id.parse::<u64>().ok())
    {
        let row = ctx
            .db
            .party_inventory_item()
            .id()
            .find(id)
            .ok_or("Physical party holder row not found")?;
        let party = ctx
            .db
            .party_authority()
            .id()
            .find(&row.party_id)
            .ok_or("Physical holder party not found")?;
        if let Some(settlement) = party.current_settlement_id {
            return Ok(format!("settlement:{settlement}"));
        }
        let mut contexts: Vec<String> = ctx
            .db
            .party_context_contact_authority()
            .party_id()
            .filter(&row.party_id)
            .filter(|contact| contact.mutual_awareness)
            .map(|contact| contact.context_id)
            .collect();
        contexts.sort();
        contexts.dedup();
        if contexts.len() != 1 {
            return Err("Physical holder party has no unique active context location".into());
        }
        return Ok(format!("context:{}", contexts.remove(0)));
    }
    if let Some(id) = binding_id.strip_prefix("escrow:") {
        let row = ctx
            .db
            .systemic_escrow_lot()
            .id()
            .find(&id.to_owned())
            .ok_or("Physical escrow holder row not found")?;
        return Ok(format!("context:{}", row.context_id));
    }
    Err("Property physical binding is unsupported".into())
}
fn actor_at_location(ctx: &ReducerContext, actor_id: u64, location: &str) -> bool {
    let Some(actor) = ctx.db.character().id().find(actor_id) else {
        return false;
    };
    if let Some(settlement) = location.strip_prefix("settlement:") {
        return actor.current_settlement_id.as_deref() == Some(settlement);
    }
    if let Some(context_id) = location.strip_prefix("context:") {
        return actor.party_id.as_deref().is_some_and(|party_id| {
            ctx.db
                .party_context_contact_authority()
                .party_id()
                .filter(party_id)
                .any(|contact| contact.context_id == context_id && contact.mutual_awareness)
        });
    }
    false
}
fn aware_witnesses_at_location(ctx: &ReducerContext, actor_id: u64, location: &str) -> Vec<u64> {
    let candidates = if let Some(settlement) = location.strip_prefix("settlement:") {
        ctx.db
            .character()
            .scan_id()
            .filter(0u64..)
            .filter(|character| {
                character.alive && character.current_settlement_id.as_deref() == Some(settlement)
            })
            .map(
                |character| adventuresim_core::systemic_character::WitnessCandidate {
                    character_id: character.id,
                    co_present: true,
                    aware: true,
                },
            )
            .collect()
    } else if let Some(context_id) = location.strip_prefix("context:") {
        ctx.db
            .character_context_membership()
            .context_id()
            .filter(context_id)
            .filter(|membership| membership.active)
            .filter_map(|membership| ctx.db.character().id().find(membership.character_id))
            .filter(|character| character.alive)
            .map(
                |character| adventuresim_core::systemic_character::WitnessCandidate {
                    character_id: character.id,
                    co_present: true,
                    aware: true,
                },
            )
            .collect()
    } else {
        Vec::new()
    };
    adventuresim_core::systemic_character::derive_theft_witnesses(actor_id, &candidates)
}
fn scope_property_event(
    ctx: &ReducerContext,
    event: &PropertyTransferEvent,
    actor_party_id: Option<&str>,
    owner_kind: LegalOwnerKind,
    owner_id: &str,
) {
    let mut scopes: Vec<(String, bool)> = Vec::new();
    if let Some(party_id) = actor_party_id {
        scopes.push((party_id.into(), false));
    }
    if owner_kind == LegalOwnerKind::Party {
        scopes.push((owner_id.into(), false));
    } else if owner_kind == LegalOwnerKind::Personal {
        if let Some(party_id) = owner_id
            .parse::<u64>()
            .ok()
            .and_then(|id| ctx.db.character().id().find(id))
            .and_then(|character| character.party_id)
        {
            scopes.push((party_id, false));
        }
    }
    for witness in &event.witness_character_ids {
        if let Some(party_id) = ctx
            .db
            .character()
            .id()
            .find(*witness)
            .and_then(|character| character.party_id)
        {
            scopes.push((party_id, true));
        }
    }
    scopes.sort_by(|a, b| a.0.cmp(&b.0));
    scopes.dedup_by(|a, b| {
        if a.0 == b.0 {
            b.1 |= a.1;
            true
        } else {
            false
        }
    });
    for (party_id, witnessed) in scopes {
        ctx.db
            .property_event_observer_scope()
            .insert(PropertyEventObserverScope {
                id: format!("{}:{party_id}", event.source_id),
                observer_party_id: party_id,
                source_id: event.source_id.clone(),
                happened_micros: event.happened_micros,
                witnessed,
            });
    }
}

/// Atomic exact-version transfer for item lots and currency lots. Splits retain
/// item ID, provenance, metadata and physical holder; totals are checked.
#[reducer]
pub fn transfer_legal_property(
    ctx: &ReducerContext,
    actor_id: u64,
    property_id: String,
    quantity: u64,
    expected_owner_kind: LegalOwnerKind,
    expected_owner_id: String,
    expected_version: u32,
    to_owner_kind: LegalOwnerKind,
    to_owner_id: String,
    source_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, actor_id)?;
    if !valid_systemic_id(&source_id)
        || !valid_systemic_id(&property_id)
        || expected_owner_id.len() > 150
        || to_owner_id.len() > 150
    {
        return Err("Invalid property transfer coordinates".into());
    }
    if let Some(r) = ctx
        .db
        .property_transfer_receipt()
        .source_id()
        .find(&source_id)
    {
        if r.property_id == property_id
            && r.quantity == quantity
            && r.expected_version == expected_version
            && r.from_owner_kind == expected_owner_kind
            && r.from_owner_id == expected_owner_id
            && r.to_owner_kind == to_owner_kind
            && r.to_owner_id == to_owner_id
        {
            return Ok(());
        }
        return Err("Conflicting property transfer source ID reuse".into());
    }
    materialize_inventory_property(ctx, &property_id, expected_owner_kind, &expected_owner_id)?;
    let mut source = ctx
        .db
        .legal_property()
        .id()
        .find(&property_id)
        .ok_or("Property not found")?;
    if source.owner_kind != expected_owner_kind
        || source.owner_id != expected_owner_id
        || source.version != expected_version
    {
        return Err("Stale property owner or version".into());
    }
    let (remaining, moved) = adventuresim_core::systemic_character::transfer_balances(
        source.quantity,
        0,
        quantity,
        expected_version,
        source.version,
    )
    .map_err(|_| "Property transfer preflight failed")?;
    let resulting_version = source
        .version
        .checked_add(1)
        .ok_or("Property version overflow")?;
    let source_binding = source.physical_binding_id.clone();
    let location = authoritative_holder_location(ctx, &source_binding)?;
    if !actor_at_location(ctx, actor_id, &location) {
        return Err(
            "Property transfer actor is not co-present with its authoritative physical holder"
                .into(),
        );
    }
    source.quantity = remaining;
    source.version = resulting_version;
    ctx.db.legal_property().id().update(source.clone());
    let destination_id = if remaining == 0 {
        property_id.clone()
    } else {
        format!("{property_id}:transfer:{source_id}")
    };
    let actor = ctx
        .db
        .character()
        .id()
        .find(actor_id)
        .ok_or("Actor not found")?;
    let authorized = match expected_owner_kind {
        LegalOwnerKind::Personal => expected_owner_id.parse::<u64>().ok() == Some(actor_id),
        LegalOwnerKind::Party => {
            actor.party_id.as_deref() == Some(expected_owner_id.as_str())
                && ctx
                    .db
                    .party_authority()
                    .id()
                    .find(&expected_owner_id)
                    .is_some_and(|p| p.leader_id == actor_id)
        }
        LegalOwnerKind::Abandoned => true,
        LegalOwnerKind::Corpse => true,
        LegalOwnerKind::Faction => false,
    };
    let theft = adventuresim_core::systemic_character::transfer_is_theft(
        core_owner(expected_owner_kind),
        authorized,
    );
    let destination_binding = move_physical_inventory(
        ctx,
        &source_binding,
        &source.item_id,
        quantity,
        to_owner_kind,
        &to_owner_id,
    )?;
    let legal_owner_kind = if theft {
        expected_owner_kind
    } else {
        to_owner_kind
    };
    let legal_owner_id = if theft {
        expected_owner_id.clone()
    } else {
        to_owner_id.clone()
    };
    let destination = LegalProperty {
        id: destination_id.clone(),
        scope_owner_key: legal_scope(legal_owner_kind, &legal_owner_id),
        kind: source.kind,
        item_id: source.item_id.clone(),
        quantity: moved,
        owner_kind: legal_owner_kind,
        owner_id: legal_owner_id,
        physical_holder_id: to_owner_id.clone(),
        physical_binding_id: destination_binding,
        version: if remaining == 0 { resulting_version } else { 0 },
        provenance: source.provenance.clone(),
        metadata: source.metadata.clone(),
        case_id: source.case_id.clone(),
    };
    if remaining == 0 {
        ctx.db.legal_property().id().update(destination);
    } else {
        ctx.db.legal_property().insert(destination);
    }
    let witnesses = aware_witnesses_at_location(ctx, actor_id, &location);
    let event = PropertyTransferEvent {
        source_id: source_id.clone(),
        actor_id,
        victim_id: expected_owner_id.clone(),
        property_id: property_id.clone(),
        location_id: location.clone(),
        happened_micros: ctx.timestamp.to_micros_since_unix_epoch(),
        theft,
        witness_character_ids: witnesses.clone(),
    };
    ctx.db.property_transfer_event().insert(event.clone());
    scope_property_event(
        ctx,
        &event,
        actor.party_id.as_deref(),
        expected_owner_kind,
        &expected_owner_id,
    );
    if theft && !witnesses.is_empty() {
        let minute =
            u64::try_from(ctx.timestamp.to_micros_since_unix_epoch()).unwrap_or(0) / 60_000_000;
        crate::reputation::record_discovered_offense(
            ctx,
            format!("theft:{source_id}"),
            actor_id,
            &location,
            "theft",
            2,
            minute,
        );
    }
    if let Some(case_id) = source.case_id.as_deref() {
        let party_id = actor.party_id.as_deref().unwrap_or("");
        let fact = if theft {
            adventuresim_core::case::OutcomeFactKind::TheftCommitted {
                property_id: property_id.clone(),
                victim_id: expected_owner_id.clone(),
            }
        } else {
            adventuresim_core::case::OutcomeFactKind::OwnershipTransferred {
                property_id: property_id.clone(),
                owner_id: to_owner_id.clone(),
            }
        };
        ingest_case_outcome_fact(ctx, &source_id, case_id, party_id, fact)?;
    }
    ctx.db
        .property_transfer_receipt()
        .insert(PropertyTransferReceipt {
            source_id,
            property_id,
            physical_binding_id: source_binding,
            quantity,
            expected_version,
            from_owner_kind: expected_owner_kind,
            from_owner_id: expected_owner_id,
            to_owner_kind,
            to_owner_id,
            resulting_version,
        });
    Ok(())
}

#[cfg(test)]
mod systemic_interaction_contract_tests {
    const SOURCE: &str = include_str!("systemic_interactions.rs");
    #[test]
    fn clients_never_choose_private_surrender_inputs_or_quest_truth() {
        let reducer = SOURCE
            .split("pub fn resolve_context_surrender")
            .nth(1)
            .unwrap();
        assert!(!reducer.split('}').next().unwrap().contains("morale:"));
        assert!(reducer.contains("authoritative outcomes"));
        assert!(SOURCE.contains("ingest_case_outcome_fact"));
    }
    #[test]
    fn accepted_surrender_retains_identity_and_excludes_participation() {
        assert!(SOURCE.contains("membership.active=false"));
        assert!(SOURCE.contains("TacticalParticipantExclusion"));
        assert!(!SOURCE.contains("character().id().delete"));
    }
    #[test]
    fn recruitment_has_one_atomic_membership_primitive_and_exclusive_grant() {
        assert_eq!(
            SOURCE
                .matches("fn transfer_character_party_membership")
                .count(),
            1
        );
        assert!(SOURCE.contains("grant_recruited_character_internal"));
        assert!(SOURCE.contains("memberships.len()>1"));
        assert!(SOURCE.contains("expected_disposition_revision"));
    }
    #[test]
    fn property_transfer_is_checked_receipted_and_witnessed() {
        assert!(SOURCE.contains("transfer_balances"));
        assert!(SOURCE.contains("Conflicting property transfer source ID reuse"));
        assert!(SOURCE.contains("derive_theft_witnesses"));
        assert!(SOURCE.contains("physical_binding_id"));
        assert!(SOURCE.contains("record_discovered_offense"));
    }
}
