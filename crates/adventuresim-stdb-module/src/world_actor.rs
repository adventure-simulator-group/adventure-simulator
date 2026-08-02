//! Unified strategic presence and contextual role authority for every Character.

use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::{
    character::{character, character__view},
    condition::character_strategic_condition,
    investigation::character_case_site_id,
    strategic::{
        hostile_group_authority, party_authority__view, road_challenge_authority,
        road_challenge_authority__view, strategic_encounter, strategic_encounter__view,
        strategic_gateway_authority__view,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum CharacterContextKind {
    HostileGroup,
    StrategicEncounter,
    RoadEncounter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum CharacterContextRole {
    Counterparty,
    Patient,
    Bystander,
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
    pub revision: u32,
    /// Explicit permission for ordinary medical treatment. Incapacitation is
    /// evaluated live and is not copied into this authority.
    pub treatment_consent: bool,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendContextCharacter {
    pub party_id: String,
    /// Public encounter ID for random encounters, or the already-visible case
    /// site / road-challenge ID. Private hostile-group IDs never cross this view.
    pub contact_ref: String,
    pub context_kind: CharacterContextKind,
    pub location_id: String,
    pub character_id: u64,
    pub role: CharacterContextRole,
    pub ordinal: u16,
    pub alive: bool,
    pub revision: u32,
    pub treatment_consent: bool,
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
        .filter(|row| row.active)
    {
        let Some(character) = ctx.db.character().id().find(row.character_id) else {
            continue;
        };
        let parties = match row.context_kind {
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
                        .map(|encounter| (encounter.party_id, row.context_id.clone()))
                })
                .collect::<Vec<_>>(),
            CharacterContextKind::HostileGroup => ctx
                .db
                .party_authority()
                .gateway_bucket()
                .filter(0u8)
                .filter(|party| {
                    party
                        .current_case_site_id
                        .as_ref()
                        .is_some_and(|site| site.value == row.location_id)
                })
                .map(|party| (party.id, row.location_id.clone()))
                .collect(),
            CharacterContextKind::RoadEncounter => ctx
                .db
                .road_challenge_authority()
                .gateway_bucket()
                .filter(0u8)
                .filter(|challenge| challenge.id == row.context_id && challenge.open)
                .map(|challenge| (challenge.party_id, challenge.id))
                .collect(),
        };
        for (party_id, contact_ref) in parties {
            let contact_id = party_context_contact_id(&party_id, &row.context_id);
            let revision = ctx
                .db
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
                );
            result.push(BackendContextCharacter {
                party_id,
                contact_ref,
                context_kind: row.context_kind,
                location_id: row.location_id.clone(),
                character_id: row.character_id,
                role: row.role,
                ordinal: row.ordinal,
                alive: character.alive,
                revision,
                treatment_consent: row.treatment_consent,
            });
        }
    }
    result
}

fn party_context_contact_id(party_id: &str, context_id: &str) -> String {
    format!("party-context-contact:{party_id}:{context_id}")
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
        .filter(|row| row.active)
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
                revision: 1,
                treatment_consent: false,
            });
        ids.push(id);
    }
    Ok(ids)
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_else(|| "Unknown".into())
}

pub(crate) fn deactivate_context_roster(ctx: &ReducerContext, context_id: &str) {
    for mut row in context_members(ctx, context_id) {
        row.active = false;
        row.revision = row.revision.saturating_add(1);
        ctx.db.character_context_membership().id().update(row);
    }
}

pub(crate) fn materialize_wounded_road_actor(
    ctx: &ReducerContext,
    context_id: &str,
    catalog_id: &str,
    absolute_minute: u64,
) -> Result<Option<u64>, String> {
    let name = match catalog_id {
        "wounded_order_courier_v1" => "Wounded Order courier",
        "wounded_knight_linden_v1" => "Wounded knight",
        _ => return Ok(None),
    };
    if let Some(existing) = context_members(ctx, context_id).into_iter().next() {
        return Ok(Some(existing.character_id));
    }
    let id = field_character_id(context_id, 0);
    if ctx.db.character().id().find(id).is_some() {
        return Err("Deterministic road-character identity collision".into());
    }
    crate::character::insert_persistent_field_character(
        ctx,
        name.into(),
        id,
        id,
        Some(absolute_minute),
    )?;
    ctx.db
        .character_context_membership()
        .insert(CharacterContextMembership {
            id: format!("context:{context_id}:0"),
            context_id: context_id.into(),
            location_id: context_id.into(),
            character_id: id,
            context_kind: CharacterContextKind::RoadEncounter,
            role: CharacterContextRole::Patient,
            ordinal: 0,
            active: true,
            revision: 1,
            treatment_consent: true,
        });
    crate::surgery::seed_field_cut(
        ctx,
        id,
        crate::surgery::LimbRegion::LeftArm,
        0.35,
        absolute_minute,
    );
    Ok(Some(id))
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
    let actor_site = character_case_site_id(ctx, actor_id);
    if actor_site.is_some() && actor_site == character_case_site_id(ctx, target_id) {
        return true;
    }
    ctx.db
        .character_context_membership()
        .character_id()
        .filter(target_id)
        .filter(|row| row.active)
        .any(|row| match row.context_kind {
            CharacterContextKind::HostileGroup => actor_site.as_ref().is_some_and(|site| {
                ctx.db
                    .hostile_group_authority()
                    .id()
                    .find(&row.context_id)
                    .is_some_and(|group| group.case_site_id.value == *site)
            }),
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
                        .road_challenge_authority()
                        .id()
                        .find(&row.context_id)
                        .is_some_and(|challenge| challenge.party_id == *party_id && challenge.open)
                })
            }
        })
}

pub(crate) fn treatment_is_authorized(ctx: &ReducerContext, patient_id: u64) -> bool {
    ctx.db
        .character_context_membership()
        .character_id()
        .filter(patient_id)
        .any(|row| row.active && row.treatment_consent)
        || ctx
            .db
            .character_strategic_condition()
            .character_id()
            .find(patient_id)
            .is_some_and(|row| row.incapacitation >= 1.0 || row.status == "incapacitated")
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
    let membership =
        ctx.db
            .character_context_membership()
            .character_id()
            .filter(target_id)
            .find(|row| {
                row.active
                    && match row.context_kind {
                        CharacterContextKind::StrategicEncounter => row.context_id == contact_ref,
                        CharacterContextKind::HostileGroup
                        | CharacterContextKind::RoadEncounter => row.location_id == contact_ref,
                    }
            })
            .ok_or("Target is not present in that context")?;
    if membership.role != CharacterContextRole::Counterparty {
        return Err("That context actor is not available for conversation".into());
    }
    if !characters_are_contextually_present(ctx, actor_id, target_id) {
        return Err("Characters are not co-present".into());
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
    let current_revision = existing_contact.as_ref().map_or_else(
        || encounter.as_ref().map_or(1, |encounter| encounter.revision),
        |contact| contact.revision,
    );
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
