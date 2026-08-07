//! Unified strategic presence and contextual role authority for every Character.

use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::{
    character::{character, character__view, character_death__view},
    condition::{character_strategic_condition, character_strategic_condition__view},
    investigation::{
        canonical_case_site_place, case_context_presence_for_observer, case_site_authority__view,
        case_site_presence_for_observer, case_site_provenance_view,
        character_case_site_occupancy__view, investigation_lead__view,
    },
    relationship::character_birth__view,
    strategic::{
        party_authority, party_authority__view, road_challenge_authority,
        road_challenge_authority__view, strategic_encounter, strategic_encounter__view,
        strategic_gateway_authority__view,
    },
    surgery::limb_injury__view,
    time::{character_time, character_time__view},
};

const EXACT_CASE_CONTEXT_CONTACT_REF: &str = "exact_case_context";

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum CharacterContextKind {
    HostileGroup,
    CaseSite,
    StrategicEncounter,
    RoadEncounter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum CharacterContextRole {
    Counterparty,
    Patient,
    Bystander,
}

/// Sanitized authored answer for a contextual interaction. The authoritative
/// reducer may still return `Unavailable` when presence or privacy no longer
/// matches the projected row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ContextualDecisionState {
    Allowed,
    Refused,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum BackendContextualDecision {
    Request,
    Refused,
    Unavailable,
    EmergencyTreatment,
}

/// A Character's role and presence in a strategic context. Hostility is
/// deliberately contextual; it is never intrinsic Character state.
#[derive(Clone, Debug)]
#[table(accessor = character_context_membership)]
pub struct CharacterContextMembership {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub context_id: String,
    #[index(btree)]
    pub location_id: String,
    #[index(btree)]
    pub character_id: u64,
    pub context_kind: CharacterContextKind,
    pub role: CharacterContextRole,
    pub ordinal: u16,
    pub active: bool,
    pub entered_at: u64,
    pub left_at: Option<u64>,
    pub revision: u32,
    pub contact_decision: ContextualDecisionState,
    /// Explicit treatment answer. Narrow emergency bandaging is evaluated live
    /// and is not copied into contextual authority.
    pub treatment_decision: ContextualDecisionState,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendContextCharacter {
    pub party_id: String,
    /// Public encounter/road-challenge ID, or the fixed action/context
    /// discriminator for case-site and hostile actors. Private case and
    /// hostile-group IDs never cross this view.
    pub contact_ref: String,
    pub context_kind: CharacterContextKind,
    pub location_id: String,
    pub character_id: u64,
    pub role: CharacterContextRole,
    pub ordinal: u16,
    pub alive: bool,
    pub revision: u32,
    pub membership_revision: u32,
    pub contact_decision: BackendContextualDecision,
    pub treatment_decision: BackendContextualDecision,
    pub treatment_limb_slug: Option<String>,
}

/// Party-scoped awareness/contact authority. `context_id` remains private;
/// callers address it through a public context reference and target Character.
#[derive(Clone, Debug)]
#[table(accessor = party_context_contact_authority)]
pub struct PartyContextContactAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub party_id: String,
    pub context_id: String,
    pub location_id: String,
    pub revision: u32,
    pub contacted: bool,
    pub mutual_awareness: bool,
}

#[derive(Clone, Debug)]
#[table(accessor = contextual_contact_receipt)]
pub struct ContextualContactReceipt {
    #[primary_key]
    pub id: String,
    pub actor_id: u64,
    pub target_id: u64,
    pub context_id: String,
    pub action_id: String,
    pub expected_revision: u32,
    pub resulting_revision: u32,
}

/// Gateway-only, role-minimal projection. Callers must query by exact context;
/// no private group composition or future encounter is exposed to players.
#[view(accessor = backend_context_characters, public)]
pub fn backend_context_characters(ctx: &ViewContext) -> Vec<BackendContextCharacter> {
    let gateway = ctx
        .db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|row| row.identity == ctx.sender());
    if !gateway {
        return Vec::new();
    }
    let mut result = Vec::new();
    for row in ctx
        .db
        .character_context_membership()
        .character_id()
        .filter(0u64..)
    {
        if !context_membership_interval_is_well_formed(&row) {
            continue;
        }
        if !matches!(
            row.context_kind,
            CharacterContextKind::CaseSite | CharacterContextKind::HostileGroup
        ) && !row.active
        {
            continue;
        }
        let Some(character) = ctx.db.character().id().find(row.character_id) else {
            continue;
        };
        let parties = match row.context_kind {
            CharacterContextKind::CaseSite | CharacterContextKind::HostileGroup => ctx
                .db
                .party_authority()
                .gateway_bucket()
                .filter(0u8)
                .filter(|party| {
                    ctx.db
                        .character_time()
                        .character_id()
                        .find(party.leader_id)
                        .is_some_and(|time| {
                            context_membership_valid_at(&row, time.minutes)
                                && character_case_site_occupancy_at_view(
                                    ctx,
                                    party.leader_id,
                                    time.minutes,
                                )
                                .and_then(|occupancy| occupancy.case_site_id.to_place())
                                .zip(canonical_case_site_place(&row.location_id))
                                .is_some_and(
                                    |(party_place, context_place)| party_place == context_place,
                                )
                                && exact_case_site_visible_to_observer_view(
                                    ctx,
                                    party.leader_id,
                                    &row.location_id,
                                    time.minutes,
                                )
                                && (row.role != CharacterContextRole::Patient
                                    || crate::outbreak::case_patient_visible_to_character_view(
                                        ctx,
                                        party.leader_id,
                                        &row.context_id,
                                        time.minutes,
                                    ))
                        })
                })
                .filter_map(|party| {
                    let minute = ctx
                        .db
                        .character_time()
                        .character_id()
                        .find(party.leader_id)?
                        .minutes;
                    let party_id = party.id;
                    Some((
                        party_id,
                        EXACT_CASE_CONTEXT_CONTACT_REF.to_string(),
                        character_alive_at_for_view(ctx, row.character_id, minute),
                    ))
                })
                .collect(),
            CharacterContextKind::StrategicEncounter => ctx
                .db
                .party_authority()
                .gateway_bucket()
                .filter(0u8)
                .filter_map(|party| {
                    ctx.db
                        .strategic_encounter()
                        .party_id()
                        .find(&party.id)
                        .filter(|encounter| {
                            encounter.encounter_id == row.context_id
                                && encounter.status == "awaiting_choice"
                        })
                        .map(|encounter| {
                            (encounter.party_id, row.context_id.clone(), character.alive)
                        })
                })
                .collect::<Vec<_>>(),
            CharacterContextKind::RoadEncounter => ctx
                .db
                .road_challenge_authority()
                .gateway_bucket()
                .filter(0u8)
                .filter(|challenge| challenge.id == row.context_id && challenge.open)
                .map(|challenge| (challenge.party_id, challenge.id, character.alive))
                .collect(),
        };
        for (party_id, contact_ref, alive_at_frontier) in parties {
            let contact_id = party_context_contact_id(&party_id, &row.context_id);
            let revision = if matches!(
                row.context_kind,
                CharacterContextKind::CaseSite | CharacterContextKind::HostileGroup
            ) {
                row.revision
            } else {
                ctx.db
                    .party_context_contact_authority()
                    .id()
                    .find(&contact_id)
                    .map_or_else(
                        || {
                            if row.context_kind == CharacterContextKind::StrategicEncounter {
                                ctx.db
                                    .strategic_encounter()
                                    .party_id()
                                    .find(&party_id)
                                    .map_or(row.revision, |encounter| encounter.revision)
                            } else {
                                1
                            }
                        },
                        |contact| contact.revision,
                    )
            };
            let presented = |decision| match decision {
                ContextualDecisionState::Allowed => BackendContextualDecision::Request,
                ContextualDecisionState::Refused => BackendContextualDecision::Refused,
                ContextualDecisionState::Unavailable => BackendContextualDecision::Unavailable,
            };
            let treatment_limb = crate::surgery::LimbRegion::ALL.into_iter().find(|limb| {
                ctx.db
                    .limb_injury()
                    .character_id()
                    .filter(row.character_id)
                    .find(|injury| injury.limb == *limb)
                    .is_some_and(|injury| injury.cut_damage > 0.0 && !injury.bandaged)
            });
            let incapacitated = ctx
                .db
                .character_strategic_condition()
                .character_id()
                .find(row.character_id)
                .is_some_and(|condition| {
                    condition.incapacitation >= 1.0 || condition.status == "incapacitated"
                });
            let emergency_bandage = row.treatment_decision != ContextualDecisionState::Refused
                && treatment_limb.is_some_and(|limb| {
                    ctx.db
                        .limb_injury()
                        .character_id()
                        .filter(row.character_id)
                        .find(|injury| injury.limb == limb)
                        .is_some_and(|injury| {
                            adventuresim_core::strategic_action::emergency_bandage_is_necessary(
                                incapacitated,
                                "bandage",
                                injury.cut_damage,
                                injury.bandaged,
                            )
                        })
                });
            result.push(BackendContextCharacter {
                party_id,
                contact_ref,
                context_kind: row.context_kind,
                location_id: row.location_id.clone(),
                character_id: row.character_id,
                role: row.role,
                ordinal: row.ordinal,
                alive: alive_at_frontier,
                revision,
                membership_revision: row.revision,
                contact_decision: presented(row.contact_decision),
                treatment_decision: if row.treatment_decision
                    == ContextualDecisionState::Unavailable
                    && emergency_bandage
                {
                    BackendContextualDecision::EmergencyTreatment
                } else {
                    presented(row.treatment_decision)
                },
                treatment_limb_slug: treatment_limb.map(|limb| limb.slug().to_owned()),
            });
        }
    }
    result
}

fn party_context_contact_id(party_id: &str, context_id: &str) -> String {
    format!("party-context-contact:{party_id}:{context_id}")
}

pub(crate) fn context_contact_revision_view(
    ctx: &ViewContext,
    party_id: &str,
    context_id: &str,
    fallback: u32,
) -> u32 {
    ctx.db
        .party_context_contact_authority()
        .id()
        .find(&party_context_contact_id(party_id, context_id))
        .map_or(fallback, |contact| contact.revision)
}

pub(crate) fn party_contacted_context(
    ctx: &ReducerContext,
    party_id: &str,
    context_id: &str,
) -> bool {
    ctx.db
        .party_context_contact_authority()
        .id()
        .find(&party_context_contact_id(party_id, context_id))
        .is_some_and(|contact| contact.contacted && contact.mutual_awareness)
}

pub(crate) fn context_members(
    ctx: &ReducerContext,
    context_id: &str,
) -> Vec<CharacterContextMembership> {
    let mut rows = ctx
        .db
        .character_context_membership()
        .context_id()
        .filter(&context_id.to_string())
        .filter(|row| context_membership_interval_is_well_formed(row) && row.active)
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| row.ordinal);
    rows
}

pub(crate) fn context_character_ids(ctx: &ReducerContext, context_id: &str) -> Vec<u64> {
    context_members(ctx, context_id)
        .into_iter()
        .filter_map(|row| {
            ctx.db
                .character()
                .id()
                .find(row.character_id)
                .filter(|character| character.alive)
                .map(|character| character.id)
        })
        .collect()
}

fn field_character_id(context_id: &str, ordinal: u16) -> u64 {
    adventuresim_core::settlement_population::stable_hash(&format!(
        "field-character:{context_id}:{ordinal}"
    )) | (1u64 << 63)
}

pub(crate) fn materialize_context_roster(
    ctx: &ReducerContext,
    kind: CharacterContextKind,
    context_id: &str,
    location_id: &str,
    archetype: &str,
    count: u32,
) -> Result<Vec<u64>, String> {
    let entered_at = crate::time::refresh_clock(ctx)?;
    let expected = count.min(u32::from(u16::MAX));
    let existing = context_members(ctx, context_id);
    if !existing.is_empty() {
        if existing.len() > expected as usize
            || existing.iter().any(|row| {
                row.context_kind != kind
                    || row.location_id != location_id
                    || row.role != CharacterContextRole::Counterparty
            })
        {
            return Err("Context roster conflicts with its immutable materialization".into());
        }
        if existing.len() == expected as usize {
            return Ok(existing.into_iter().map(|row| row.character_id).collect());
        }
    }
    let mut ids = Vec::with_capacity(expected as usize);
    ids.extend(existing.iter().map(|row| row.character_id));
    for ordinal in existing.len() as u16..expected as u16 {
        let id = field_character_id(context_id, ordinal);
        if ctx.db.character().id().find(id).is_some() {
            return Err("Deterministic field-character identity collision".into());
        }
        let display = archetype.replace(['_', '-'], " ");
        crate::character::insert_persistent_field_character(
            ctx,
            format!("{} {}", title_case(&display), ordinal + 1),
            id,
            id,
            None,
        )?;
        ctx.db
            .character_context_membership()
            .insert(CharacterContextMembership {
                id: format!("context:{context_id}:{ordinal}"),
                context_id: context_id.to_string(),
                location_id: location_id.to_string(),
                character_id: id,
                context_kind: kind,
                role: CharacterContextRole::Counterparty,
                ordinal,
                active: true,
                entered_at,
                left_at: None,
                revision: 1,
                contact_decision: ContextualDecisionState::Allowed,
                treatment_decision: ContextualDecisionState::Unavailable,
            });
        ids.push(id);
    }
    Ok(ids)
}

/// Carry already-materialized mortal road counterparties into a combat
/// follow-up without replacing their Character identity or components.
pub(crate) fn rebind_road_cast_to_strategic_encounter(
    ctx: &ReducerContext,
    road_context_id: &str,
    encounter_id: &str,
    archetype: &str,
    count: u32,
) -> Result<Vec<u64>, String> {
    let entered_at = crate::time::refresh_clock(ctx)?;
    let mut eligible = context_members(ctx, road_context_id)
        .into_iter()
        .filter(|membership| {
            membership.context_kind == CharacterContextKind::RoadEncounter
                && membership.role == CharacterContextRole::Counterparty
                && ctx
                    .db
                    .character()
                    .id()
                    .find(membership.character_id)
                    .is_some_and(|character| character.alive)
        })
        .take(usize::try_from(count).unwrap_or(usize::MAX))
        .collect::<Vec<_>>();
    eligible.sort_by_key(|membership| membership.ordinal);
    for (ordinal, road_membership) in eligible.iter().enumerate() {
        let ordinal = u16::try_from(ordinal)
            .map_err(|_| "Strategic encounter roster exceeds the supported size")?;
        let id = format!("context:{encounter_id}:{ordinal}");
        let rebound = CharacterContextMembership {
            id: id.clone(),
            context_id: encounter_id.into(),
            location_id: encounter_id.into(),
            character_id: road_membership.character_id,
            context_kind: CharacterContextKind::StrategicEncounter,
            role: CharacterContextRole::Counterparty,
            ordinal,
            active: true,
            entered_at,
            left_at: None,
            revision: 1,
            contact_decision: ContextualDecisionState::Allowed,
            treatment_decision: ContextualDecisionState::Unavailable,
        };
        if let Some(existing) = ctx.db.character_context_membership().id().find(&id) {
            if existing.context_id != rebound.context_id
                || existing.character_id != rebound.character_id
                || existing.context_kind != rebound.context_kind
                || existing.role != rebound.role
            {
                return Err("Road-to-combat Character identity collision".into());
            }
        } else {
            ctx.db.character_context_membership().insert(rebound);
        }
    }
    materialize_context_roster(
        ctx,
        CharacterContextKind::StrategicEncounter,
        encounter_id,
        encounter_id,
        archetype,
        count,
    )
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_else(|| "Unknown".into())
}

fn context_interval_is_well_formed(active: bool, entered_at: u64, left_at: Option<u64>) -> bool {
    active == left_at.is_none() && left_at.is_none_or(|left_at| left_at >= entered_at)
}

pub(crate) fn context_membership_interval_is_well_formed(row: &CharacterContextMembership) -> bool {
    context_interval_is_well_formed(row.active, row.entered_at, row.left_at)
}

pub(crate) fn context_membership_valid_at(row: &CharacterContextMembership, minute: u64) -> bool {
    context_membership_interval_is_well_formed(row)
        && row.entered_at <= minute
        && row.left_at.is_none_or(|left_at| minute < left_at)
}

fn exact_context_claim_matches(
    kind: CharacterContextKind,
    contact_ref: &str,
    expected_revision: u32,
    actual_revision: u32,
) -> bool {
    matches!(
        kind,
        CharacterContextKind::CaseSite | CharacterContextKind::HostileGroup
    ) && contact_ref == EXACT_CASE_CONTEXT_CONTACT_REF
        && expected_revision == actual_revision
}

fn exactly_one<T>(mut values: impl Iterator<Item = T>) -> Option<T> {
    let value = values.next()?;
    values.next().is_none().then_some(value)
}

fn projected_case_context_claim(
    ctx: &ReducerContext,
    observer_character_id: u64,
    membership: &CharacterContextMembership,
    minute: u64,
) -> Option<(String, u32)> {
    context_membership_valid_at(membership, minute)
        .then(|| (membership.id.clone(), membership.revision))
        .filter(|_| {
            crate::investigation::exact_case_site_for_observer_at(
                ctx,
                observer_character_id,
                &membership.location_id,
                minute,
            )
            .is_some()
        })
}

fn character_case_site_occupancy_at_view(
    ctx: &ViewContext,
    character_id: u64,
    minute: u64,
) -> Option<crate::investigation::CharacterCaseSiteOccupancy> {
    let mut rows = ctx
        .db
        .character_case_site_occupancy()
        .character_id()
        .filter(character_id)
        .filter(|row| row.entered_at <= minute && row.left_at.is_none_or(|left| minute < left));
    let row = rows.next()?;
    rows.next().is_none().then_some(row)
}

pub(crate) fn character_alive_at_for_view(
    ctx: &ViewContext,
    character_id: u64,
    minute: u64,
) -> bool {
    ctx.db.character().id().find(character_id).is_some()
        && ctx
            .db
            .character_birth()
            .character_id()
            .find(character_id)
            .is_none_or(|birth| i128::from(birth.birth_minute) <= i128::from(minute))
        && ctx
            .db
            .character_death()
            .character_id()
            .find(character_id)
            .is_none_or(|death| death.strategic_minute > minute)
}

fn exact_case_site_visible_to_observer_view(
    ctx: &ViewContext,
    observer_character_id: u64,
    case_site_id: &str,
    minute: u64,
) -> bool {
    let Some(place) = canonical_case_site_place(case_site_id) else {
        return false;
    };
    let Some(site) = ctx
        .db
        .case_site_authority()
        .id_key()
        .find(&case_site_id.to_owned())
    else {
        return false;
    };
    if site.id.to_place().as_ref() != Some(&place) {
        return false;
    }
    let Some(generated_aliases) = case_site_provenance_view(ctx, &site) else {
        return false;
    };
    ctx.db
        .investigation_lead()
        .owner_character_id()
        .filter(observer_character_id)
        .any(|lead| {
            lead.recorded_at <= minute
                && canonical_case_site_place(&lead.exact_location_id).as_ref() == Some(&place)
                && (lead.case_id == site.case_id
                    || generated_aliases
                        .as_ref()
                        .is_some_and(|aliases| lead.case_id == aliases.1.as_str()))
                && lead.latitude_e7 == site.latitude_e7
                && lead.longitude_e7 == site.longitude_e7
                && matches!(
                    lead.destination_stage.as_str(),
                    "exact_believed" | "visited"
                )
                && (lead.corrected_by.is_empty()
                    || ctx
                        .db
                        .investigation_lead()
                        .id()
                        .find(&lead.corrected_by)
                        .is_some_and(|correction| {
                            correction.owner_character_id == lead.owner_character_id
                                && correction.recorded_at > minute
                        }))
        })
}

pub(crate) fn deactivate_context_roster_at(ctx: &ReducerContext, context_id: &str, minute: u64) {
    for mut row in context_members(ctx, context_id) {
        if !row.active {
            continue;
        }
        row.active = false;
        row.left_at = Some(minute.max(row.entered_at));
        row.revision = row.revision.saturating_add(1);
        ctx.db.character_context_membership().id().update(row);
    }
}

pub(crate) fn deactivate_context_roster(ctx: &ReducerContext, context_id: &str) {
    let minute = crate::time::refresh_clock(ctx).unwrap_or(0);
    deactivate_context_roster_at(ctx, context_id, minute);
}

/// Materialize every individualized mortal in a compiled road cast as an
/// ordinary, fully componentized Character. Cast order is the stable identity
/// coordinate; narrative collectives and explicitly blocked figures never
/// receive a surrogate Character row.
pub(crate) fn materialize_road_encounter_cast(
    ctx: &ReducerContext,
    context_id: &str,
    definition: &adventuresim_core::road_encounter_catalog::EncounterDefinition,
    absolute_minute: u64,
) -> Result<Vec<u64>, String> {
    use adventuresim_core::road_encounter_catalog::{
        AuthoredInteractionDecision, CharacterCastRole, SpeakerBacking,
    };

    let decision = |value: AuthoredInteractionDecision| match value {
        AuthoredInteractionDecision::Allowed => ContextualDecisionState::Allowed,
        AuthoredInteractionDecision::Refused => ContextualDecisionState::Refused,
        AuthoredInteractionDecision::Unavailable => ContextualDecisionState::Unavailable,
    };

    let mut materialized = Vec::new();
    for (cast_ordinal, speaker) in definition.cast.iter().enumerate() {
        let SpeakerBacking::Character {
            role,
            contact_decision,
            treatment_decision,
        } = &speaker.backing
        else {
            continue;
        };
        let ordinal = u16::try_from(cast_ordinal)
            .map_err(|_| "Road encounter cast exceeds the supported roster size")?;
        let membership_id = format!("context:{context_id}:{ordinal}");
        let character_id = field_character_id(context_id, ordinal);
        let expected_role = match role {
            CharacterCastRole::Counterparty => CharacterContextRole::Counterparty,
            CharacterCastRole::Patient => CharacterContextRole::Patient,
            CharacterCastRole::Bystander => CharacterContextRole::Bystander,
        };
        let existing_membership = ctx
            .db
            .character_context_membership()
            .id()
            .find(&membership_id);
        let existing_character = ctx.db.character().id().find(character_id);
        match (existing_membership, existing_character) {
            (Some(membership), Some(character)) => {
                if membership.context_id != context_id
                    || membership.location_id != context_id
                    || membership.character_id != character_id
                    || membership.context_kind != CharacterContextKind::RoadEncounter
                    || membership.role != expected_role
                    || membership.ordinal != ordinal
                    || !context_membership_interval_is_well_formed(&membership)
                    || !membership.active
                    || membership.contact_decision != decision(*contact_decision)
                    || membership.treatment_decision != decision(*treatment_decision)
                    || character.name != speaker.name
                {
                    return Err(
                        "Road cast retry conflicts with immutable Character authority".into(),
                    );
                }
                materialized.push(character_id);
                continue;
            }
            (Some(_), None) | (None, Some(_)) => {
                return Err("Road cast retry found partial Character authority".into());
            }
            (None, None) => {}
        }
        crate::character::insert_persistent_field_character(
            ctx,
            speaker.name.clone(),
            character_id,
            character_id,
            Some(absolute_minute),
        )?;
        ctx.db
            .character_context_membership()
            .insert(CharacterContextMembership {
                id: membership_id,
                context_id: context_id.into(),
                location_id: context_id.into(),
                character_id,
                context_kind: CharacterContextKind::RoadEncounter,
                role: expected_role,
                ordinal,
                active: true,
                entered_at: absolute_minute,
                left_at: None,
                revision: 1,
                contact_decision: decision(*contact_decision),
                treatment_decision: decision(*treatment_decision),
            });
        if expected_role == CharacterContextRole::Patient {
            crate::surgery::seed_field_cut(
                ctx,
                character_id,
                crate::surgery::LimbRegion::LeftArm,
                0.35,
                absolute_minute,
            );
            if *treatment_decision == AuthoredInteractionDecision::Unavailable {
                crate::condition::apply_blood_loss(ctx, character_id, 0.30)?;
            }
        }
        materialized.push(character_id);
    }
    Ok(materialized)
}

pub(crate) fn characters_are_contextually_present(
    ctx: &ReducerContext,
    actor_id: u64,
    target_id: u64,
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
    let actor_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(actor_id)
        .map(|row| row.minutes);
    let actor_case_presence = actor_minute.and_then(|minute| {
        case_site_presence_for_observer(ctx, actor_id, actor_id, minute)
            .map(|presence| (presence, minute))
    });
    if let Some((actor_presence, minute)) = actor_case_presence.as_ref()
        && case_site_presence_for_observer(ctx, actor_id, target_id, *minute).is_some_and(
            |target_presence| {
                adventuresim_core::strategic_presence::are_co_present(
                    actor_presence,
                    &target_presence,
                )
            },
        )
    {
        return true;
    }
    ctx.db
        .character_context_membership()
        .character_id()
        .filter(target_id)
        .filter(|row| {
            context_membership_interval_is_well_formed(row)
                && (row.active
                    || matches!(
                        row.context_kind,
                        CharacterContextKind::CaseSite | CharacterContextKind::HostileGroup
                    ))
        })
        .any(|row| match row.context_kind {
            CharacterContextKind::CaseSite | CharacterContextKind::HostileGroup => {
                actor_case_presence
                    .as_ref()
                    .is_some_and(|(actor_presence, minute)| {
                        let Some((projected_id, projected_revision)) =
                            projected_case_context_claim(ctx, actor_id, &row, *minute)
                        else {
                            return false;
                        };
                        case_context_presence_for_observer(
                            ctx,
                            actor_id,
                            &row,
                            &projected_id,
                            projected_revision,
                            *minute,
                        )
                        .is_some_and(|target_presence| {
                            adventuresim_core::strategic_presence::are_co_present(
                                actor_presence,
                                &target_presence,
                            )
                        })
                    })
            }
            CharacterContextKind::StrategicEncounter => {
                actor.party_id.as_ref().is_some_and(|party_id| {
                    ctx.db
                        .strategic_encounter()
                        .party_id()
                        .find(party_id)
                        .is_some_and(|encounter| {
                            encounter.encounter_id == row.context_id
                                && encounter.status == "awaiting_choice"
                        })
                })
            }
            CharacterContextKind::RoadEncounter => {
                actor.party_id.as_ref().is_some_and(|party_id| {
                    ctx.db
                        .party_authority()
                        .id()
                        .find(party_id)
                        .is_some_and(|party| {
                            ctx.db
                                .road_challenge_authority()
                                .id()
                                .find(&row.context_id)
                                .is_some_and(|challenge| {
                                    challenge.party_id == *party_id
                                        && challenge.open
                                        && crate::strategic::party_at_bound_road_challenge(
                                            ctx, &party, &challenge,
                                        )
                                })
                        })
                })
            }
        })
}

fn contextual_membership_is_visible(
    ctx: &ReducerContext,
    actor_id: u64,
    membership: &CharacterContextMembership,
) -> bool {
    let Some(actor) = ctx.db.character().id().find(actor_id) else {
        return false;
    };
    let Some(party_id) = actor.party_id.as_deref() else {
        return false;
    };
    let Some(actor_minute) = ctx
        .db
        .character_time()
        .character_id()
        .find(actor_id)
        .map(|row| row.minutes)
    else {
        return false;
    };
    (if matches!(
        membership.context_kind,
        CharacterContextKind::CaseSite | CharacterContextKind::HostileGroup
    ) {
        context_membership_valid_at(membership, actor_minute)
    } else {
        context_membership_interval_is_well_formed(membership) && membership.active
    }) && characters_are_contextually_present(ctx, actor_id, membership.character_id)
        && match membership.context_kind {
            CharacterContextKind::CaseSite => crate::outbreak::case_patient_visible_to_character(
                ctx,
                actor_id,
                &membership.context_id,
                actor_minute,
            ),
            CharacterContextKind::RoadEncounter => ctx
                .db
                .road_challenge_authority()
                .id()
                .find(&membership.context_id)
                .is_some_and(|challenge| challenge.party_id == party_id),
            CharacterContextKind::StrategicEncounter => ctx
                .db
                .strategic_encounter()
                .party_id()
                .find(&party_id.to_owned())
                .is_some_and(|encounter| encounter.encounter_id == membership.context_id),
            CharacterContextKind::HostileGroup => true,
        }
}

fn public_decision(
    state: ContextualDecisionState,
) -> adventuresim_core::strategic_action::ContextualActionDecision {
    use adventuresim_core::strategic_action::{ContextualActionDecision, ContextualActionReason};
    match state {
        ContextualDecisionState::Allowed => {
            ContextualActionDecision::Allowed(ContextualActionReason::TargetPermission)
        }
        ContextualDecisionState::Refused => ContextualActionDecision::Refused,
        ContextualDecisionState::Unavailable => ContextualActionDecision::Unavailable,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContextualTreatmentClaim {
    pub contact_ref: String,
    pub expected_membership_revision: u32,
}

fn contextual_treatment_claim_matches(
    membership: &CharacterContextMembership,
    claim: &ContextualTreatmentClaim,
) -> bool {
    let reference_matches = match membership.context_kind {
        CharacterContextKind::CaseSite | CharacterContextKind::HostileGroup => {
            claim.contact_ref == EXACT_CASE_CONTEXT_CONTACT_REF
        }
        CharacterContextKind::StrategicEncounter | CharacterContextKind::RoadEncounter => {
            claim.contact_ref == membership.context_id
        }
    };
    reference_matches && claim.expected_membership_revision == membership.revision
}

fn treatment_target_answer(
    contextual_answer: Option<ContextualDecisionState>,
    party_preference: Option<ContextualDecisionState>,
) -> adventuresim_core::strategic_action::ContextualActionDecision {
    contextual_answer.or(party_preference).map_or(
        adventuresim_core::strategic_action::ContextualActionDecision::Unavailable,
        public_decision,
    )
}

fn contextual_contact_decision(
    ctx: &ReducerContext,
    actor_id: u64,
    target_id: u64,
    membership: &CharacterContextMembership,
) -> adventuresim_core::strategic_action::ContextualActionDecision {
    use adventuresim_core::strategic_action::{ContextualActionDecision, ContextualActionReason};
    if actor_id == target_id {
        return ContextualActionDecision::Allowed(ContextualActionReason::SelfAction);
    }
    if membership.character_id != target_id
        || !ctx
            .db
            .character()
            .id()
            .find(target_id)
            .is_some_and(|target| target.alive)
        || !contextual_membership_is_visible(ctx, actor_id, membership)
    {
        return ContextualActionDecision::Unavailable;
    }
    public_decision(membership.contact_decision)
}

pub(crate) fn contextual_treatment_decision(
    ctx: &ReducerContext,
    actor_id: u64,
    patient_id: u64,
    limb: crate::surgery::LimbRegion,
    procedure: &str,
    claim: Option<&ContextualTreatmentClaim>,
) -> adventuresim_core::strategic_action::ContextualActionDecision {
    use adventuresim_core::strategic_action::ContextualActionDecision;
    let Some(actor) = ctx
        .db
        .character()
        .id()
        .find(actor_id)
        .filter(|row| row.alive)
    else {
        return ContextualActionDecision::Unavailable;
    };
    let Some(patient) = ctx
        .db
        .character()
        .id()
        .find(patient_id)
        .filter(|row| row.alive)
    else {
        return ContextualActionDecision::Unavailable;
    };
    if actor_id == patient_id {
        if claim.is_some() {
            return ContextualActionDecision::Unavailable;
        }
        return adventuresim_core::strategic_action::decide_contextual_action(
            true,
            ContextualActionDecision::Unavailable,
            false,
        );
    }
    if !characters_are_contextually_present(ctx, actor_id, patient_id) {
        return ContextualActionDecision::Unavailable;
    }

    let actor_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(actor_id)
        .map_or(0, |time| time.minutes);
    let contextual = ctx
        .db
        .character_context_membership()
        .character_id()
        .filter(patient_id)
        .filter(|membership| {
            if matches!(
                membership.context_kind,
                CharacterContextKind::CaseSite | CharacterContextKind::HostileGroup
            ) {
                context_membership_valid_at(membership, actor_minute)
            } else {
                context_membership_interval_is_well_formed(membership) && membership.active
            }
        })
        .collect::<Vec<_>>();
    let authored = if contextual.is_empty() {
        if claim.is_some() {
            return ContextualActionDecision::Unavailable;
        }
        None
    } else {
        let Some(claim) = claim else {
            return ContextualActionDecision::Unavailable;
        };
        let matching = contextual.iter().filter(|membership| {
            contextual_treatment_claim_matches(membership, claim)
                && contextual_membership_is_visible(ctx, actor_id, membership)
        });
        let Some(membership) = exactly_one(matching) else {
            return ContextualActionDecision::Unavailable;
        };
        Some(membership.treatment_decision)
    };
    let same_party_preference = (actor.party_id.is_some() && actor.party_id == patient.party_id)
        .then_some(patient.party_treatment_decision);
    let incapacitated = ctx
        .db
        .character_strategic_condition()
        .character_id()
        .find(patient_id)
        .is_some_and(|row| row.incapacitation >= 1.0 || row.status == "incapacitated");
    let injury = crate::surgery::injury_for(ctx, patient_id, limb);
    let emergency_bandage = adventuresim_core::strategic_action::emergency_bandage_is_necessary(
        incapacitated,
        procedure,
        injury.cut_damage,
        injury.bandaged,
    );
    adventuresim_core::strategic_action::decide_contextual_action(
        false,
        treatment_target_answer(authored, same_party_preference),
        emergency_bandage,
    )
}

/// Treatment decision for preparations and other interventions which never
/// receive the emergency-bandage exception.
pub(crate) fn contextual_nonemergency_treatment_decision(
    ctx: &ReducerContext,
    actor_id: u64,
    patient_id: u64,
) -> adventuresim_core::strategic_action::ContextualActionDecision {
    contextual_treatment_decision(
        ctx,
        actor_id,
        patient_id,
        crate::surgery::LimbRegion::LeftArm,
        "intervention",
        None,
    )
}

pub(crate) fn context_patient_is_treated(ctx: &ReducerContext, context_id: &str) -> bool {
    context_members(ctx, context_id)
        .into_iter()
        .find(|row| row.role == CharacterContextRole::Patient)
        .is_some_and(|row| {
            crate::surgery::LimbRegion::ALL.into_iter().any(|limb| {
                let injury = crate::surgery::injury_for(ctx, row.character_id, limb);
                injury.cut_damage > 0.0 && injury.bandaged
            })
        })
}

/// Initiate ordinary social contact with any living co-present Character.
/// Contact is intentionally not a full authored-dialogue session: it lays the
/// durable relationship edge and changes encounter awareness atomically.
#[reducer]
pub fn contact_context_character(
    ctx: &ReducerContext,
    actor_id: u64,
    target_id: u64,
    contact_ref: String,
    expected_revision: u32,
    action_id: String,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, actor_id)?;
    if action_id.is_empty() || action_id.len() > 160 {
        return Err("Contextual contact action ID is invalid".into());
    }
    let receipt_id = format!("context-contact:{actor_id}:{action_id}");
    if let Some(existing) = ctx.db.contextual_contact_receipt().id().find(&receipt_id) {
        return if existing.actor_id == actor_id
            && existing.target_id == target_id
            && existing.context_id == contact_ref
            && existing.expected_revision == expected_revision
        {
            Ok(())
        } else {
            Err("Conflicting contextual contact retry".into())
        };
    }
    crate::character::require_living_character(ctx, actor_id)?;
    crate::character::require_living_character(ctx, target_id)?;
    let actor = ctx
        .db
        .character()
        .id()
        .find(actor_id)
        .ok_or("Contact actor does not exist")?;
    let party_id = actor.party_id.ok_or("Contact requires an active party")?;
    let actor_minute = ctx
        .db
        .character_time()
        .character_id()
        .find(actor_id)
        .ok_or("Contact actor has no personal time")?
        .minutes;
    let actor_case_presence =
        case_site_presence_for_observer(ctx, actor_id, actor_id, actor_minute);
    let candidates = ctx
        .db
        .character_context_membership()
        .character_id()
        .filter(target_id)
        .filter(|row| {
            (if matches!(
                row.context_kind,
                CharacterContextKind::CaseSite | CharacterContextKind::HostileGroup
            ) {
                context_membership_valid_at(row, actor_minute)
            } else {
                context_membership_interval_is_well_formed(row) && row.active
            }) && match row.context_kind {
                CharacterContextKind::StrategicEncounter => row.context_id == contact_ref,
                CharacterContextKind::CaseSite | CharacterContextKind::HostileGroup => {
                    exact_context_claim_matches(
                        row.context_kind,
                        &contact_ref,
                        expected_revision,
                        row.revision,
                    ) && actor_case_presence.as_ref().is_some_and(|actor_presence| {
                        case_context_presence_for_observer(
                            ctx,
                            actor_id,
                            row,
                            &row.id,
                            expected_revision,
                            actor_minute,
                        )
                        .is_some_and(|target_presence| {
                            adventuresim_core::strategic_presence::are_co_present(
                                actor_presence,
                                &target_presence,
                            )
                        })
                    })
                }
                CharacterContextKind::RoadEncounter => row.location_id == contact_ref,
            }
        })
        .collect::<Vec<_>>();
    let membership = exactly_one(candidates.into_iter())
        .ok_or("Target context claim is unavailable or ambiguous")?;
    if membership.context_kind == CharacterContextKind::CaseSite
        && membership.role == CharacterContextRole::Patient
        && !crate::outbreak::case_patient_visible_to_character(
            ctx,
            actor_id,
            &membership.context_id,
            actor_minute,
        )
    {
        return Err("Patient context is not visible at the actor frontier".into());
    }
    match contextual_contact_decision(ctx, actor_id, target_id, &membership) {
        adventuresim_core::strategic_action::ContextualActionDecision::Allowed(_) => {}
        adventuresim_core::strategic_action::ContextualActionDecision::Refused => {
            return Err("Contact was refused".into());
        }
        adventuresim_core::strategic_action::ContextualActionDecision::Unavailable => {
            return Err("Contact is unavailable".into());
        }
    }
    let mut encounter = ctx
        .db
        .strategic_encounter()
        .party_id()
        .find(
            &ctx.db
                .character()
                .id()
                .find(actor_id)
                .and_then(|character| character.party_id)
                .ok_or("Contact requires an active party")?,
        )
        .filter(|encounter| {
            encounter.encounter_id == membership.context_id && encounter.status == "awaiting_choice"
        });
    let contact_id = party_context_contact_id(&party_id, &membership.context_id);
    let existing_contact = ctx
        .db
        .party_context_contact_authority()
        .id()
        .find(&contact_id);
    let current_revision = if matches!(
        membership.context_kind,
        CharacterContextKind::CaseSite | CharacterContextKind::HostileGroup
    ) {
        membership.revision
    } else {
        existing_contact.as_ref().map_or_else(
            || encounter.as_ref().map_or(1, |encounter| encounter.revision),
            |contact| contact.revision,
        )
    };
    if current_revision != expected_revision {
        return Err("Context contact revision is stale".into());
    }
    let resulting_revision = expected_revision.saturating_add(1);
    if let Some(encounter) = encounter.as_mut() {
        encounter.party_aware = true;
        encounter.enemy_aware = true;
        encounter
            .available_choices
            .retain(|choice| choice != "sneak");
        encounter.selection_explanation =
            "Contact established; both sides are aware and surprise is no longer possible.".into();
        encounter.revision = resulting_revision;
        ctx.db
            .strategic_encounter()
            .party_id()
            .update(encounter.clone());
    } else {
        if membership.context_kind == CharacterContextKind::StrategicEncounter {
            return Err("Strategic encounter is no longer active".into());
        }
    }
    let contact = PartyContextContactAuthority {
        id: contact_id,
        party_id,
        context_id: membership.context_id.clone(),
        location_id: membership.location_id.clone(),
        revision: resulting_revision,
        contacted: true,
        mutual_awareness: true,
    };
    if existing_contact.is_some() {
        ctx.db
            .party_context_contact_authority()
            .id()
            .update(contact);
    } else {
        ctx.db.party_context_contact_authority().insert(contact);
    }
    crate::social::begin_physiology_presence_on_contact(ctx, actor_id, target_id);
    crate::social::apply_async_socializing(ctx, actor_id, target_id, 10)?;
    ctx.db
        .contextual_contact_receipt()
        .insert(ContextualContactReceipt {
            id: receipt_id,
            actor_id,
            target_id,
            context_id: contact_ref,
            action_id,
            expected_revision,
            resulting_revision,
        });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        CharacterContextKind, CharacterContextMembership, CharacterContextRole,
        ContextualDecisionState, ContextualTreatmentClaim, EXACT_CASE_CONTEXT_CONTACT_REF,
        context_interval_is_well_formed, contextual_treatment_claim_matches,
        exact_context_claim_matches, exactly_one, treatment_target_answer,
    };

    fn treatment_membership(kind: CharacterContextKind) -> CharacterContextMembership {
        CharacterContextMembership {
            id: "membership:1".into(),
            context_id: "road:1".into(),
            location_id: "road:1".into(),
            character_id: 9,
            context_kind: kind,
            role: CharacterContextRole::Patient,
            ordinal: 0,
            active: true,
            entered_at: 1,
            left_at: None,
            revision: 4,
            contact_decision: ContextualDecisionState::Allowed,
            treatment_decision: ContextualDecisionState::Allowed,
        }
    }

    #[test]
    fn context_intervals_reject_malformed_active_and_chronology_shapes() {
        assert!(context_interval_is_well_formed(true, 10, None));
        assert!(context_interval_is_well_formed(false, 10, Some(10)));
        assert!(!context_interval_is_well_formed(true, 10, Some(11)));
        assert!(!context_interval_is_well_formed(false, 10, None));
        assert!(!context_interval_is_well_formed(false, 10, Some(9)));
    }

    #[test]
    fn exact_context_claim_rejects_forged_discriminators_and_stale_revisions() {
        assert!(exact_context_claim_matches(
            CharacterContextKind::CaseSite,
            EXACT_CASE_CONTEXT_CONTACT_REF,
            3,
            3,
        ));
        assert!(exact_context_claim_matches(
            CharacterContextKind::HostileGroup,
            EXACT_CASE_CONTEXT_CONTACT_REF,
            3,
            3,
        ));
        assert!(!exact_context_claim_matches(
            CharacterContextKind::CaseSite,
            "forged_private_context",
            3,
            3,
        ));
        assert!(!exact_context_claim_matches(
            CharacterContextKind::CaseSite,
            EXACT_CASE_CONTEXT_CONTACT_REF,
            2,
            3,
        ));
    }

    #[test]
    fn context_claim_resolution_fails_closed_on_zero_or_ambiguous_rows() {
        assert_eq!(exactly_one(std::iter::empty::<u8>()), None);
        assert_eq!(exactly_one([7].into_iter()), Some(7));
        assert_eq!(exactly_one([7, 8].into_iter()), None);
    }

    #[test]
    fn treatment_claims_bind_exact_context_and_membership_revision() {
        let road = treatment_membership(CharacterContextKind::RoadEncounter);
        assert!(contextual_treatment_claim_matches(
            &road,
            &ContextualTreatmentClaim {
                contact_ref: "road:1".into(),
                expected_membership_revision: 4,
            }
        ));
        assert!(!contextual_treatment_claim_matches(
            &road,
            &ContextualTreatmentClaim {
                contact_ref: "road:forged".into(),
                expected_membership_revision: 4,
            }
        ));
        assert!(!contextual_treatment_claim_matches(
            &road,
            &ContextualTreatmentClaim {
                contact_ref: "road:1".into(),
                expected_membership_revision: 3,
            }
        ));

        let case = treatment_membership(CharacterContextKind::CaseSite);
        assert!(contextual_treatment_claim_matches(
            &case,
            &ContextualTreatmentClaim {
                contact_ref: EXACT_CASE_CONTEXT_CONTACT_REF.into(),
                expected_membership_revision: 4,
            }
        ));
        assert!(!contextual_treatment_claim_matches(
            &case,
            &ContextualTreatmentClaim {
                contact_ref: case.context_id.clone(),
                expected_membership_revision: 4,
            }
        ));
    }

    #[test]
    fn contextual_refusal_overrides_ordinary_party_care_preference() {
        use adventuresim_core::strategic_action::{
            ContextualActionDecision, ContextualActionReason,
        };
        assert_eq!(
            treatment_target_answer(None, Some(ContextualDecisionState::Allowed)),
            ContextualActionDecision::Allowed(ContextualActionReason::TargetPermission)
        );
        assert_eq!(
            treatment_target_answer(
                Some(ContextualDecisionState::Refused),
                Some(ContextualDecisionState::Allowed)
            ),
            ContextualActionDecision::Refused
        );
    }

    #[test]
    fn contextual_actions_share_private_presence_decisions_and_physiology_authority() {
        let source = include_str!("world_actor.rs");
        let authorization = source
            .split("fn contextual_membership_is_visible")
            .nth(1)
            .and_then(|tail| tail.split("fn public_decision").next())
            .expect("contextual authorization");
        assert!(authorization.contains("case_patient_visible_to_character"));
        assert!(authorization.contains("challenge.party_id == party_id"));
        assert!(authorization.contains("characters_are_contextually_present"));

        let contact = source
            .split("pub fn contact_context_character")
            .nth(1)
            .expect("context contact reducer");
        assert!(contact.contains("contextual_contact_decision"));
        assert!(contact.contains("begin_physiology_presence_on_contact"));
        assert!(contact.contains("retain(|choice| choice != \"sneak\")"));
    }

    #[test]
    fn emergency_treatment_is_only_exact_limb_bandaging() {
        let source = include_str!("world_actor.rs");
        let treatment = source
            .split("pub(crate) fn contextual_treatment_decision")
            .nth(1)
            .and_then(|tail| {
                tail.split("pub(crate) fn context_patient_is_treated")
                    .next()
            })
            .expect("treatment decision");
        assert!(treatment.contains("emergency_bandage_is_necessary"));
        assert!(treatment.contains("injury_for(ctx, patient_id, limb)"));
        assert!(treatment.contains("incapacitation >= 1.0"));
        assert!(!treatment.contains("stitch\""));
        assert!(!treatment.contains("extract\""));
    }

    #[test]
    fn road_combat_reuses_cast_character_identity() {
        let source = include_str!("world_actor.rs");
        let rebound = source
            .split("pub(crate) fn rebind_road_cast_to_strategic_encounter")
            .nth(1)
            .and_then(|tail| tail.split("fn title_case").next())
            .expect("road cast rebound");
        assert!(rebound.contains("character_id: road_membership.character_id"));
        assert!(rebound.contains("CharacterContextKind::StrategicEncounter"));
        assert!(rebound.contains("materialize_context_roster"));
    }

    #[test]
    fn case_context_joins_use_typed_observer_relative_presence() {
        let source = include_str!("world_actor.rs");
        let presence = source
            .split("pub(crate) fn characters_are_contextually_present")
            .nth(1)
            .and_then(|tail| tail.split("fn contextual_membership_is_visible").next())
            .expect("contextual presence projection");
        assert!(presence.contains("case_site_presence_for_observer"));
        assert!(presence.contains("case_context_presence_for_observer"));
        assert!(presence.contains("are_co_present"));
        assert!(!presence.contains("actor_site == character_case_site_id"));
        assert!(!presence.contains("actor_site.as_ref() == Some(&row.location_id)"));
    }
}
