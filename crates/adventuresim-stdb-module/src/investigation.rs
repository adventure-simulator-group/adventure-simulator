//! Private investigation authority and observer-safe gateway projections.

use crate::{
    character::{character, character__view},
    local_problem::local_problem_receipt,
    settlement_population::settlement_npc,
    strategic::{require_strategic_gateway, strategic_gateway_authority__view},
    time::world_clock,
};
use adventuresim_core::investigation as inv;
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};
use std::collections::BTreeMap;

const MAX_TEXT: usize = 512;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, SpacetimeType)]
pub struct CaseSiteId {
    pub value: String,
}

impl CaseSiteId {
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl From<String> for CaseSiteId {
    fn from(value: String) -> Self {
        Self { value }
    }
}

impl std::ops::Deref for CaseSiteId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl std::fmt::Display for CaseSiteId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(formatter)
    }
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_case_authority)]
pub struct InvestigationCaseAuthority {
    #[primary_key]
    pub id: String,
    pub problem_id: String,
    pub hidden_target_json: String,
    pub generation_explanation_json: String,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_event_authority)]
pub struct InvestigationEventAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub case_id: String,
    pub canonical_propositions_json: String,
    pub occurred_at: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_observation)]
pub struct InvestigationObservation {
    #[primary_key]
    pub id: String,
    pub event_id: String,
    pub observer_ref: String,
    pub proposition_id: String,
    pub stage_json: String,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_recollection)]
pub struct InvestigationRecollection {
    #[primary_key]
    pub id: String,
    pub observation_id: String,
    pub witness_ref: String,
    pub proposition_id: String,
    pub stage_json: String,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_claim)]
pub struct InvestigationClaim {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub case_id: String,
    pub proposition_id: String,
    pub hidden_speaker_ref: String,
    pub statement: String,
    pub confidence_bps: u16,
    pub disclosure_stage: String,
    pub transmission_stage: String,
    pub received_at: u64,
    pub public_case_id: String,
    pub safe_source_label: String,
    pub conflict_group: String,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_evidence_authority)]
pub struct InvestigationEvidenceAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub case_id: String,
    pub proposition_id: String,
    pub presentation_kind: EvidencePresentationKind,
    pub authority_json: String,
    pub hidden_coordinates_json: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum EvidencePresentationKind {
    Physical,
    Informational,
}

/// Private, source-attributed proof custody/knowledge. Merely having a hidden
/// evidence-authority row is never enough to present that proof.
#[derive(Clone, Debug)]
#[table(accessor = investigation_evidence_knowledge)]
pub struct InvestigationEvidenceKnowledge {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    #[index(btree)]
    pub case_id: String,
    pub evidence_id: String,
    pub source_id: String,
    pub learned_at: u64,
}

#[allow(dead_code)] // Owning investigation actions call this as evidence types are added.
pub(crate) fn record_evidence_knowledge(
    ctx: &ReducerContext,
    owner_character_id: u64,
    case_id: &str,
    evidence_id: &str,
    source_id: &str,
) -> Result<(), String> {
    let evidence = ctx
        .db
        .investigation_evidence_authority()
        .id()
        .find(&evidence_id.to_string())
        .ok_or("Evidence does not exist")?;
    if evidence.case_id != case_id {
        return Err("Evidence belongs to another case".into());
    }
    let id = inv::compound_id(&[
        "evidence-knowledge",
        &owner_character_id.to_string(),
        case_id,
        evidence_id,
    ]);
    if let Some(existing) = ctx.db.investigation_evidence_knowledge().id().find(&id) {
        return if existing.source_id == source_id {
            Ok(())
        } else {
            Err("Evidence knowledge has conflicting provenance".into())
        };
    }
    ctx.db
        .investigation_evidence_knowledge()
        .insert(InvestigationEvidenceKnowledge {
            id,
            owner_character_id,
            case_id: case_id.into(),
            evidence_id: evidence_id.into(),
            source_id: source_id.into(),
            learned_at: official_minute(ctx),
        });
    Ok(())
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_belief)]
pub struct InvestigationBelief {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    #[index(btree)]
    pub case_id: String,
    pub proposition_id: String,
    pub current_revision_id: String,
    pub statement: String,
    pub confidence_bps: u16,
    pub conflict_group: String,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_belief_revision)]
pub struct InvestigationBeliefRevision {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    pub belief_id: String,
    pub revision: u16,
    pub statement: String,
    pub confidence_bps: u16,
    pub provenance_kind: String,
    pub provenance_label: String,
    pub supersedes: String,
    pub recorded_at: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_lead)]
pub struct InvestigationLead {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    #[index(btree)]
    pub case_id: String,
    pub summary: String,
    pub source_label: String,
    pub confidence_bps: u16,
    pub destination_stage: String,
    pub directions: String,
    pub exact_location_id: String,
    pub latitude_e7: i32,
    pub longitude_e7: i32,
    pub witness_name: String,
    pub witness_description: String,
    pub witness_occupation_or_relationship: String,
    pub expected_location: String,
    pub current_learned_location: String,
    pub contradiction_group: String,
    pub corrected_by: String,
    pub recorded_at: u64,
}

/// Private physical authority for a strategic investigation site. Coordinates
/// never appear in a public table; observer-safe exact pins are projected by
/// the gateway view below only after an explicit exact disclosure.
#[derive(Clone, Debug)]
#[table(accessor = case_site_authority)]
pub struct CaseSiteAuthority {
    #[primary_key]
    pub id_key: String,
    pub id: CaseSiteId,
    #[index(btree)]
    pub case_id: String,
    #[index(btree)]
    pub origin_settlement_id: String,
    pub name: String,
    pub description: String,
    pub scene_key: String,
    pub longitude_e7: i32,
    pub latitude_e7: i32,
    pub coordinates_are_geographic: bool,
    pub distance_m: u64,
}

/// Private per-party presentation choice. Tracking does not accept a contract,
/// disclose knowledge, move a party, satisfy an objective, or award anything.
#[derive(Clone, Debug)]
#[table(accessor = party_case_site_tracking)]
pub struct PartyCaseSiteTracking {
    #[primary_key]
    pub party_id: String,
    pub observer_character_id: u64,
    pub case_site_id: CaseSiteId,
    pub tracked_at: u64,
}

/// Private physical occupancy. Public character rows deliberately contain no
/// case-site identifier.
#[derive(Clone, Debug)]
#[table(accessor = character_case_site_occupancy)]
pub struct CharacterCaseSiteOccupancy {
    #[primary_key]
    pub character_id: u64,
    #[index(btree)]
    pub gateway_bucket: u8,
    pub case_site_id: CaseSiteId,
}

pub(crate) fn character_case_site_id(ctx: &ReducerContext, character_id: u64) -> Option<String> {
    ctx.db
        .character_case_site_occupancy()
        .character_id()
        .find(character_id)
        .map(|row| row.case_site_id.value)
}

pub(crate) fn set_character_case_site(
    ctx: &ReducerContext,
    character_id: u64,
    case_site_id: Option<String>,
) {
    if ctx
        .db
        .character_case_site_occupancy()
        .character_id()
        .find(character_id)
        .is_some()
    {
        ctx.db
            .character_case_site_occupancy()
            .character_id()
            .delete(character_id);
    }
    if let Some(value) = case_site_id {
        ctx.db
            .character_case_site_occupancy()
            .insert(CharacterCaseSiteOccupancy {
                character_id,
                gateway_bucket: 0,
                case_site_id: CaseSiteId { value },
            });
    }
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_sharing_receipt)]
pub struct InvestigationSharingReceipt {
    #[primary_key]
    pub id: String,
    pub sender_id: u64,
    pub recipient_id: u64,
    pub source_record_id: String,
    pub payload_fingerprint: String,
    pub shared_at: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_action_receipt)]
pub struct InvestigationActionReceipt {
    #[primary_key]
    pub id: String,
    pub actor_id: u64,
    pub action_kind: String,
    pub canonical_payload: String,
    pub applied_at: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_safe_claim_receipt)]
pub struct InvestigationSafeClaimReceipt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    pub claim_id: String,
    pub public_case_id: String,
    pub proposition_id: String,
    pub statement: String,
    pub safe_source_label: String,
    pub confidence_bps: u16,
    pub conflict_group: String,
    pub correction_of_belief_id: String,
    pub consumed_by: String,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_received_testimony)]
pub struct InvestigationReceivedTestimony {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    #[index(btree)]
    pub public_case_id: String,
    pub claim_id: String,
    pub witness_ref: String,
    pub source_receipt_id: String,
    pub received_at: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_testimony_bundle)]
pub struct InvestigationTestimonyBundle {
    #[primary_key]
    pub id: String,
    pub case_id: String,
    pub witness_ref: String,
    pub reliability_json: String,
    pub stages_json: String,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_safe_lead_receipt)]
pub struct InvestigationSafeLeadReceipt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    pub public_case_id: String,
    pub summary: String,
    pub safe_source_label: String,
    pub confidence_bps: u16,
    pub destination_stage: String,
    pub directions: String,
    pub exact_location_id: String,
    pub latitude_e7: i32,
    pub longitude_e7: i32,
    pub conflict_group: String,
    pub correction_of_lead_id: String,
    pub consumed_by: String,
}

/// Sanitized journal row. It contains no hidden threat, sincerity, coordinates
/// below exact knowledge, private NPC identifiers, likelihoods, or bridges.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendInvestigationJournalEntry {
    pub owner_character_id: u64,
    pub case_id: String,
    pub record_id: String,
    pub kind: String,
    pub summary: String,
    pub source_label: String,
    pub confidence_bps: u16,
    pub contradiction_group: String,
    pub corrected_by: String,
    pub supersedes: String,
    pub recorded_at: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendInvestigationLead {
    pub owner_character_id: u64,
    pub case_id: String,
    pub lead_id: String,
    pub summary: String,
    pub source_label: String,
    pub confidence_bps: u16,
    pub destination_stage: String,
    pub directions: String,
    pub exact_location_id: String,
    pub latitude_e7: i32,
    pub longitude_e7: i32,
    pub witness_name: String,
    pub witness_description: String,
    pub witness_occupation_or_relationship: String,
    pub expected_location: String,
    pub current_learned_location: String,
    pub contradiction_group: String,
    pub corrected_by: String,
    pub recorded_at: u64,
}

/// Dedicated observer-safe map/travel projection. Unlike a raw lead, every
/// row has been joined to a server-issued site and is currently exact for the
/// named observer. The strategic web must additionally filter by session
/// owner before rendering it.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendCaseSitePin {
    pub owner_character_id: u64,
    pub case_id: String,
    pub case_site_id: String,
    pub origin_settlement_id: String,
    pub name: String,
    pub description: String,
    pub scene_key: String,
    pub longitude_e7: i32,
    pub latitude_e7: i32,
    pub coordinates_are_geographic: bool,
    pub distance_m: u64,
    pub knowledge_stage: String,
    pub tracked: bool,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendCharacterCaseSiteLocation {
    pub character_id: u64,
    pub case_site_id: CaseSiteId,
}

fn is_gateway(ctx: &ViewContext) -> bool {
    ctx.db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|row| row.identity == ctx.sender())
}

#[view(accessor = backend_investigation_journal, public)]
pub fn backend_investigation_journal(ctx: &ViewContext) -> Vec<BackendInvestigationJournalEntry> {
    if !is_gateway(ctx) {
        return Vec::new();
    }
    let mut rows = Vec::new();
    rows.extend(
        ctx.db
            .investigation_belief_revision()
            .owner_character_id()
            .filter(0u64..)
            .filter_map(|r| {
                let belief = ctx.db.investigation_belief().id().find(&r.belief_id)?;
                if belief.owner_character_id != r.owner_character_id {
                    return None;
                }
                Some(BackendInvestigationJournalEntry {
                    owner_character_id: r.owner_character_id,
                    case_id: belief.case_id,
                    record_id: r.id,
                    kind: "belief_revision".into(),
                    summary: r.statement,
                    source_label: r.provenance_label,
                    confidence_bps: r.confidence_bps,
                    contradiction_group: belief.conflict_group,
                    corrected_by: String::new(),
                    supersedes: r.supersedes,
                    recorded_at: r.recorded_at,
                })
            }),
    );
    rows.sort_by_key(|row| {
        (
            row.owner_character_id,
            row.recorded_at,
            row.record_id.clone(),
        )
    });
    rows
}

#[view(accessor = backend_investigation_leads, public)]
pub fn backend_investigation_leads(ctx: &ViewContext) -> Vec<BackendInvestigationLead> {
    if !is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .investigation_lead()
        .owner_character_id()
        .filter(0u64..)
        .map(sanitize_lead)
        .collect()
}

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
            if site.case_id != lead.case_id
                || site.latitude_e7 != lead.latitude_e7
                || site.longitude_e7 != lead.longitude_e7
            {
                return None;
            }
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
                case_id: lead.case_id,
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
    ctx.db
        .investigation_lead()
        .owner_character_id()
        .filter(observer_character_id)
        .find(|lead| {
            lead.exact_location_id == case_site_id
                && lead.case_id == site.case_id
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
    if site.case_id != case_id {
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
        current_learned_location: String::new(),
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

fn sanitize_lead(row: InvestigationLead) -> BackendInvestigationLead {
    let exact = matches!(row.destination_stage.as_str(), "exact_believed" | "visited");
    BackendInvestigationLead {
        owner_character_id: row.owner_character_id,
        case_id: row.case_id,
        lead_id: row.id,
        summary: row.summary,
        source_label: row.source_label,
        confidence_bps: row.confidence_bps,
        destination_stage: row.destination_stage,
        directions: row.directions,
        exact_location_id: if exact {
            row.exact_location_id
        } else {
            String::new()
        },
        latitude_e7: if exact { row.latitude_e7 } else { 0 },
        longitude_e7: if exact { row.longitude_e7 } else { 0 },
        witness_name: row.witness_name,
        witness_description: row.witness_description,
        witness_occupation_or_relationship: row.witness_occupation_or_relationship,
        expected_location: row.expected_location,
        current_learned_location: row.current_learned_location,
        contradiction_group: row.contradiction_group,
        corrected_by: row.corrected_by,
        recorded_at: row.recorded_at,
    }
}

fn bounded(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_TEXT || value.chars().any(char::is_control) {
        Err("Investigation text must be non-empty, bounded, and printable".into())
    } else {
        Ok(())
    }
}
fn bounded_optional(value: &str) -> Result<(), String> {
    if value.len() > MAX_TEXT || value.chars().any(char::is_control) {
        Err("Investigation text must be bounded and printable".into())
    } else {
        Ok(())
    }
}
fn bps(value: u16) -> Result<(), String> {
    (value <= 10_000)
        .then_some(())
        .ok_or_else(|| "Confidence must be at most 10000 basis points".into())
}
fn validate_destination(stage: &str, id: &str, lat: i32, lon: i32) -> Result<(), String> {
    let exact = matches!(stage, "exact_believed" | "visited");
    if exact
        && (id.is_empty()
            || !(-900_000_000..=900_000_000).contains(&lat)
            || !(-1_800_000_000..=1_800_000_000).contains(&lon))
    {
        return Err("Exact destination requires an id and valid E7 coordinates".into());
    }
    if !exact && (!id.is_empty() || lat != 0 || lon != 0) {
        return Err("Non-exact destination knowledge may not carry a pin".into());
    }
    Ok(())
}
fn official_minute(ctx: &ReducerContext) -> u64 {
    ctx.db
        .world_clock()
        .id()
        .find(0)
        .map_or(0, |clock| clock.official_minutes)
}
fn require_actor(ctx: &ReducerContext, actor_id: u64) -> Result<crate::Character, String> {
    require_strategic_gateway(ctx)?;
    let actor = ctx
        .db
        .character()
        .id()
        .find(actor_id)
        .ok_or("Character not found")?;
    if !actor.alive {
        return Err("Dead characters cannot update investigation knowledge".into());
    }
    Ok(actor)
}
fn canonical_payload(parts: &[&str]) -> Result<String, String> {
    let payload = inv::compound_id(parts);
    if payload.len() > 4_096 {
        Err("Canonical investigation payload is too large".into())
    } else {
        Ok(payload)
    }
}
fn idempotent(
    ctx: &ReducerContext,
    action_id: &str,
    actor_id: u64,
    kind: &str,
    payload: &str,
) -> Result<bool, String> {
    bounded(action_id)?;
    if let Some(existing) = ctx
        .db
        .investigation_action_receipt()
        .id()
        .find(action_id.to_string())
    {
        if existing.actor_id != actor_id
            || existing.action_kind != kind
            || existing.canonical_payload != payload
        {
            return Err("Investigation action id was reused with a different payload".into());
        }
        return Ok(true);
    }
    Ok(false)
}
fn record_action(
    ctx: &ReducerContext,
    action_id: String,
    actor_id: u64,
    kind: &str,
    payload: String,
) {
    ctx.db
        .investigation_action_receipt()
        .insert(InvestigationActionReceipt {
            id: action_id,
            actor_id,
            action_kind: kind.into(),
            canonical_payload: payload,
            applied_at: official_minute(ctx),
        });
}

/// Converts #182's private safe receipt to owner knowledge without consulting
/// or exposing the local problem's hidden cause.
#[reducer]
pub fn receive_local_problem_rumor(
    ctx: &ReducerContext,
    character_id: u64,
    receipt_id: String,
    action_id: String,
) -> Result<(), String> {
    require_actor(ctx, character_id)?;
    bounded(&receipt_id)?;
    let payload = canonical_payload(&[&receipt_id])?;
    if idempotent(ctx, &action_id, character_id, "receive_rumor", &payload)? {
        return Ok(());
    }
    let receipt = ctx
        .db
        .local_problem_receipt()
        .id()
        .find(&receipt_id)
        .ok_or("Rumor receipt not found")?;
    if receipt.character_id != character_id {
        return Err("Rumor receipt belongs to another observer".into());
    }
    let contact = ctx.db.settlement_npc().id().find(&receipt.contact_npc_id);
    let visible_description = contact.as_ref().map_or_else(String::new, |npc| {
        format!(
            "{}, {}, {}, with {} hair; {}",
            npc.height, npc.build, npc.complexion, npc.hair, npc.visible_features
        )
    });
    // Never expose the private opaque case seam. This observer-facing stable ID
    // derives only from the already-public problem identifier.
    let case_id = inv::compound_id(&["case", "problem", &receipt.problem_id]);
    let lead_id = inv::compound_id(&["lead", "rumor", &receipt.id]);
    if ctx.db.investigation_lead().id().find(&lead_id).is_none() {
        ctx.db.investigation_lead().insert(InvestigationLead {
            id: lead_id,
            owner_character_id: character_id,
            case_id,
            summary: receipt.safe_summary,
            source_label: "local rumor".into(),
            confidence_bps: 5_000,
            destination_stage: if receipt.expected_location_id.is_empty() {
                "unknown"
            } else {
                "textual"
            }
            .into(),
            directions: receipt.expected_location_id.clone(),
            exact_location_id: String::new(),
            latitude_e7: 0,
            longitude_e7: 0,
            witness_name: contact
                .as_ref()
                .map_or_else(String::new, |npc| npc.name.clone()),
            witness_description: visible_description,
            witness_occupation_or_relationship: contact
                .map_or_else(String::new, |npc| npc.profession),
            expected_location: receipt.expected_location_id,
            current_learned_location: String::new(),
            contradiction_group: String::new(),
            corrected_by: String::new(),
            recorded_at: receipt.learned_at,
        });
    }
    record_action(ctx, action_id, character_id, "receive_rumor", payload);
    Ok(())
}

/// Trusted authority seam for #184/generation. `pipeline_json` is private
/// server-authored material and must never originate in or be projected to a
/// browser; only the registered SSR gateway can invoke this temporary seam.
pub(crate) fn stage_investigation_claim(
    ctx: &ReducerContext,
    character_id: u64,
    receipt_id: String,
    pipeline_json: String,
    public_case_id: String,
    safe_source_label: String,
    conflict_group: String,
    correction_of_belief_id: String,
) -> Result<(), String> {
    require_actor(ctx, character_id)?;
    for value in [&receipt_id, &public_case_id, &safe_source_label] {
        bounded(value)?;
    }
    bounded_optional(&conflict_group)?;
    bounded_optional(&correction_of_belief_id)?;
    if pipeline_json.len() > 8_192 {
        return Err("Pipeline payload is too large".into());
    }
    let pipeline: inv::PipelineInput =
        serde_json::from_str(&pipeline_json).map_err(|_| "Invalid investigation pipeline")?;
    let proposition = pipeline.proposition.clone();
    let public_claim_id = pipeline.receipt_identity.clone();
    let (observation, recollection, claim) =
        inv::process_report(pipeline).map_err(|_| "Invalid investigation pipeline")?;
    let claim = claim.ok_or("An omitted proposition cannot create a receivable claim")?;
    if ctx
        .db
        .investigation_safe_claim_receipt()
        .id()
        .find(&receipt_id)
        .is_some()
    {
        return Err("Safe claim receipt already exists".into());
    }
    let event_id = observation.event_id.as_str().to_string();
    let event_payload = serde_json::to_string(&proposition).map_err(|e| e.to_string())?;
    if let Some(existing) = ctx.db.investigation_event_authority().id().find(&event_id) {
        if existing.case_id != claim.case_id.as_str()
            || existing.canonical_propositions_json != event_payload
        {
            return Err("Event id does not match its existing authority payload".into());
        }
    } else {
        ctx.db
            .investigation_event_authority()
            .insert(InvestigationEventAuthority {
                id: event_id,
                case_id: claim.case_id.as_str().into(),
                canonical_propositions_json: event_payload,
                occurred_at: claim.received_at,
            });
    }
    let observation_row = InvestigationObservation {
        id: observation.id.as_str().into(),
        event_id: observation.event_id.as_str().into(),
        observer_ref: observation.observer_ref.clone(),
        proposition_id: observation.proposition_id.as_str().into(),
        stage_json: serde_json::to_string(&observation).map_err(|e| e.to_string())?,
    };
    if let Some(existing) = ctx
        .db
        .investigation_observation()
        .id()
        .find(&observation_row.id)
    {
        if existing.event_id != observation_row.event_id
            || existing.observer_ref != observation_row.observer_ref
            || existing.proposition_id != observation_row.proposition_id
            || existing.stage_json != observation_row.stage_json
        {
            return Err("Observation id does not match existing authority".into());
        }
    } else {
        ctx.db.investigation_observation().insert(observation_row);
    }
    let recollection_row = InvestigationRecollection {
        id: recollection.id.as_str().into(),
        observation_id: recollection.observation_id.as_str().into(),
        witness_ref: claim.speaker_ref.clone(),
        proposition_id: claim.proposition_id.as_str().into(),
        stage_json: serde_json::to_string(&recollection).map_err(|e| e.to_string())?,
    };
    if let Some(existing) = ctx
        .db
        .investigation_recollection()
        .id()
        .find(&recollection_row.id)
    {
        if existing.observation_id != recollection_row.observation_id
            || existing.witness_ref != recollection_row.witness_ref
            || existing.proposition_id != recollection_row.proposition_id
            || existing.stage_json != recollection_row.stage_json
        {
            return Err("Recollection id does not match existing authority".into());
        }
    } else {
        ctx.db.investigation_recollection().insert(recollection_row);
    }
    let claim_row = InvestigationClaim {
        id: claim.id.as_str().into(),
        case_id: claim.case_id.as_str().into(),
        proposition_id: claim.proposition_id.as_str().into(),
        hidden_speaker_ref: claim.speaker_ref,
        statement: claim.statement.clone(),
        confidence_bps: claim.confidence.get(),
        disclosure_stage: format!("{:?}", claim.disclosure),
        transmission_stage: format!("{:?}", claim.transmission),
        received_at: claim.received_at,
        public_case_id: public_case_id.clone(),
        safe_source_label: safe_source_label.clone(),
        conflict_group: conflict_group.clone(),
    };
    if let Some(existing) = ctx.db.investigation_claim().id().find(&claim_row.id) {
        if existing.case_id != claim_row.case_id
            || existing.proposition_id != claim_row.proposition_id
            || existing.statement != claim_row.statement
            || existing.public_case_id != claim_row.public_case_id
        {
            return Err("Claim id does not match existing authority".into());
        }
    } else {
        ctx.db.investigation_claim().insert(claim_row);
    }
    ctx.db
        .investigation_safe_claim_receipt()
        .insert(InvestigationSafeClaimReceipt {
            id: receipt_id,
            owner_character_id: character_id,
            claim_id: claim.id.as_str().into(),
            public_case_id,
            proposition_id: claim.proposition_id.as_str().into(),
            statement: claim.statement,
            safe_source_label,
            confidence_bps: claim.confidence.get(),
            conflict_group,
            correction_of_belief_id,
            consumed_by: String::new(),
        });
    let _ = public_claim_id;
    Ok(())
}

pub(crate) fn persist_runtime_testimony(
    ctx: &ReducerContext,
    character_id: u64,
    private_case_id: &str,
    public_case_id: &str,
    witness_ref: &str,
    safe_source_label: &str,
    reliability: adventuresim_dialogue::TestimonyReliability,
    event_statement: &str,
    circumstance_statement: &str,
) -> Result<(), String> {
    use adventuresim_core::investigation::{
        AtomicProposition, CaseId, DisclosureMode, EventId, MemoryCondition, PerceptionCondition,
        PipelineInput, PropositionId, TransmissionCondition,
    };
    let event = adventuresim_dialogue::PropositionTestimony {
        proposition_id: inv::compound_id(&["proposition", private_case_id, "event"]),
        statement: event_statement.into(),
        confidence_bps: 7_000,
        disclosed: true,
    };
    let circumstance = adventuresim_dialogue::PropositionTestimony {
        proposition_id: inv::compound_id(&["proposition", private_case_id, "circumstance"]),
        statement: circumstance_statement.into(),
        confidence_bps: 6_000,
        disclosed: true,
    };
    let stages = adventuresim_dialogue::build_testimony_bundle(
        reliability.clone(),
        event,
        circumstance,
        "I remember a smaller, stooped figure instead.",
        "I saw nothing unusual there.",
    );
    let bundle_id = inv::compound_id(&["testimony", private_case_id, witness_ref]);
    let stages_json = serde_json::to_string(&stages).map_err(|e| e.to_string())?;
    let reliability_json = serde_json::to_string(&reliability).map_err(|e| e.to_string())?;
    if let Some(existing) = ctx
        .db
        .investigation_testimony_bundle()
        .id()
        .find(&bundle_id)
    {
        if existing.stages_json != stages_json || existing.reliability_json != reliability_json {
            return Err("Testimony bundle identity conflicts with authority".into());
        }
    } else {
        ctx.db
            .investigation_testimony_bundle()
            .insert(InvestigationTestimonyBundle {
                id: bundle_id.clone(),
                case_id: private_case_id.into(),
                witness_ref: witness_ref.into(),
                reliability_json,
                stages_json,
            });
    }
    for (index, stage) in stages.into_iter().enumerate() {
        let Some(disclosed) = stage.disclosed_text.clone() else {
            continue;
        };
        let receipt_id = inv::compound_id(&[
            "safe-testimony",
            &character_id.to_string(),
            &bundle_id,
            &index.to_string(),
        ]);
        if ctx
            .db
            .investigation_safe_claim_receipt()
            .id()
            .find(&receipt_id)
            .is_some()
        {
            continue;
        }
        let proposition_id =
            PropositionId::new(stage.proposition_id.clone()).map_err(|_| "Invalid proposition")?;
        let proposition = AtomicProposition::new(
            proposition_id,
            witness_ref,
            "reported",
            &stage.perceived_text,
        )
        .map_err(|_| "Invalid testimony proposition")?;
        let pipeline = PipelineInput {
            case_id: CaseId::new(private_case_id).map_err(|_| "Invalid case ID")?,
            event_id: EventId::new(inv::compound_id(&["event", &bundle_id, &index.to_string()]))
                .map_err(|_| "Invalid event ID")?,
            proposition,
            observer_ref: witness_ref.into(),
            speaker_ref: witness_ref.into(),
            receipt_identity: receipt_id.clone(),
            recollection_revision: 1,
            perceived_text: stage.perceived_text,
            recalled_text: stage.recalled_text,
            disclosed_text: Some(disclosed),
            transmitted_text: stage.transmitted_text,
            perception: PerceptionCondition::Clear,
            memory: if matches!(
                reliability,
                adventuresim_dialogue::TestimonyReliability::Mistaken
            ) {
                MemoryCondition::Confused
            } else {
                MemoryCondition::Accurate
            },
            disclosure: if matches!(
                reliability,
                adventuresim_dialogue::TestimonyReliability::Deceptive
            ) {
                DisclosureMode::Distort
            } else {
                DisclosureMode::Disclose
            },
            transmission: TransmissionCondition::Clear,
            received_at: official_minute(ctx),
        };
        stage_investigation_claim(
            ctx,
            character_id,
            receipt_id.clone(),
            serde_json::to_string(&pipeline).map_err(|e| e.to_string())?,
            public_case_id.into(),
            safe_source_label.into(),
            inv::compound_id(&["conflict", public_case_id, &stage.proposition_id]),
            String::new(),
        )?;
        receive_investigation_claim(
            ctx,
            character_id,
            inv::compound_id(&["receive-testimony", &receipt_id]),
            receipt_id,
        )?;
    }
    Ok(())
}

#[reducer]
pub fn receive_investigation_claim(
    ctx: &ReducerContext,
    character_id: u64,
    action_id: String,
    receipt_id: String,
) -> Result<(), String> {
    require_actor(ctx, character_id)?;
    bounded(&receipt_id)?;
    let payload = canonical_payload(&[&receipt_id])?;
    if idempotent(ctx, &action_id, character_id, "receive_claim", &payload)? {
        return Ok(());
    }
    let mut receipt = ctx
        .db
        .investigation_safe_claim_receipt()
        .id()
        .find(&receipt_id)
        .ok_or("Safe claim receipt not found")?;
    if receipt.owner_character_id != character_id || !receipt.consumed_by.is_empty() {
        return Err("Safe claim receipt is stale or belongs to another observer".into());
    }
    let authority = ctx
        .db
        .investigation_claim()
        .id()
        .find(&receipt.claim_id)
        .ok_or("Claim authority missing")?;
    if authority.public_case_id != receipt.public_case_id
        || authority.proposition_id != receipt.proposition_id
        || authority.statement != receipt.statement
        || authority.safe_source_label != receipt.safe_source_label
        || authority.confidence_bps != receipt.confidence_bps
    {
        return Err("Safe claim receipt no longer matches authority".into());
    }
    let previous = if receipt.correction_of_belief_id.is_empty() {
        None
    } else {
        let belief = ctx
            .db
            .investigation_belief()
            .id()
            .find(&receipt.correction_of_belief_id)
            .ok_or("Correction target belief not found")?;
        if belief.owner_character_id != character_id
            || belief.case_id != receipt.public_case_id
            || belief.proposition_id != receipt.proposition_id
        {
            return Err("Correction target does not match observer and proposition".into());
        }
        Some(belief)
    };
    let belief_id = previous.as_ref().map_or_else(
        || inv::compound_id(&["belief", &character_id.to_string(), &receipt.claim_id]),
        |belief| belief.id.clone(),
    );
    let now = official_minute(ctx);
    let revision = previous.as_ref().map_or(1, |_| {
        ctx.db
            .investigation_belief_revision()
            .owner_character_id()
            .filter(character_id)
            .filter(|r| r.belief_id == belief_id)
            .count()
            .saturating_add(1) as u16
    });
    let revision_id = inv::compound_id(&["revision", &belief_id, &revision.to_string()]);
    ctx.db
        .investigation_belief_revision()
        .insert(InvestigationBeliefRevision {
            id: revision_id.clone(),
            owner_character_id: character_id,
            belief_id: belief_id.clone(),
            revision,
            statement: receipt.statement.clone(),
            confidence_bps: receipt.confidence_bps,
            provenance_kind: "received_claim".into(),
            provenance_label: receipt.safe_source_label.clone(),
            supersedes: previous
                .as_ref()
                .map_or_else(String::new, |b| b.current_revision_id.clone()),
            recorded_at: now,
        });
    let belief = InvestigationBelief {
        id: belief_id.clone(),
        owner_character_id: character_id,
        case_id: receipt.public_case_id.clone(),
        proposition_id: receipt.proposition_id.clone(),
        current_revision_id: revision_id,
        statement: receipt.statement.clone(),
        confidence_bps: receipt.confidence_bps,
        conflict_group: receipt.conflict_group.clone(),
    };
    if previous.is_some() {
        ctx.db.investigation_belief().id().update(belief);
    } else {
        ctx.db.investigation_belief().insert(belief);
    }
    let testimony_id = inv::compound_id(&[
        "received-testimony",
        &character_id.to_string(),
        &receipt.claim_id,
        &authority.hidden_speaker_ref,
    ]);
    if ctx
        .db
        .investigation_received_testimony()
        .id()
        .find(&testimony_id)
        .is_none()
    {
        ctx.db
            .investigation_received_testimony()
            .insert(InvestigationReceivedTestimony {
                id: testimony_id,
                owner_character_id: character_id,
                public_case_id: receipt.public_case_id.clone(),
                claim_id: receipt.claim_id.clone(),
                witness_ref: authority.hidden_speaker_ref,
                source_receipt_id: receipt.id.clone(),
                received_at: now,
            });
    }
    receipt.consumed_by = action_id.clone();
    ctx.db
        .investigation_safe_claim_receipt()
        .id()
        .update(receipt);
    record_action(ctx, action_id, character_id, "receive_claim", payload);
    Ok(())
}

#[reducer]
pub fn stage_investigation_lead(
    ctx: &ReducerContext,
    character_id: u64,
    receipt_id: String,
    public_case_id: String,
    summary: String,
    safe_source_label: String,
    confidence_bps: u16,
    destination_stage: String,
    directions: String,
    exact_location_id: String,
    latitude_e7: i32,
    longitude_e7: i32,
    conflict_group: String,
    correction_of_lead_id: String,
) -> Result<(), String> {
    require_actor(ctx, character_id)?;
    for value in [
        &receipt_id,
        &public_case_id,
        &summary,
        &safe_source_label,
        &destination_stage,
    ] {
        bounded(value)?;
    }
    bps(confidence_bps)?;
    bounded_optional(&directions)?;
    bounded_optional(&exact_location_id)?;
    bounded_optional(&conflict_group)?;
    bounded_optional(&correction_of_lead_id)?;
    if !matches!(
        destination_stage.as_str(),
        "unknown"
            | "textual"
            | "landmark"
            | "approximate_area"
            | "route_segment"
            | "exact_believed"
            | "visited"
    ) {
        return Err("Unknown destination knowledge stage".into());
    }
    validate_destination(
        &destination_stage,
        &exact_location_id,
        latitude_e7,
        longitude_e7,
    )?;
    if matches!(destination_stage.as_str(), "exact_believed" | "visited") {
        let site = ctx
            .db
            .case_site_authority()
            .id_key()
            .find(&exact_location_id)
            .ok_or("Exact lead must name a server-issued case site")?;
        if site.case_id != public_case_id
            || site.latitude_e7 != latitude_e7
            || site.longitude_e7 != longitude_e7
        {
            return Err("Exact lead does not match the case-site authority".into());
        }
    }
    if ctx
        .db
        .investigation_safe_lead_receipt()
        .id()
        .find(&receipt_id)
        .is_some()
    {
        return Err("Safe lead receipt already exists".into());
    }
    ctx.db
        .investigation_safe_lead_receipt()
        .insert(InvestigationSafeLeadReceipt {
            id: receipt_id,
            owner_character_id: character_id,
            public_case_id,
            summary,
            safe_source_label,
            confidence_bps,
            destination_stage,
            directions,
            exact_location_id,
            latitude_e7,
            longitude_e7,
            conflict_group,
            correction_of_lead_id,
            consumed_by: String::new(),
        });
    Ok(())
}

#[reducer]
pub fn discover_investigation_lead(
    ctx: &ReducerContext,
    character_id: u64,
    action_id: String,
    receipt_id: String,
) -> Result<(), String> {
    require_actor(ctx, character_id)?;
    bounded(&receipt_id)?;
    let payload = canonical_payload(&[&receipt_id])?;
    if idempotent(ctx, &action_id, character_id, "discover_lead", &payload)? {
        return Ok(());
    }
    let mut receipt = ctx
        .db
        .investigation_safe_lead_receipt()
        .id()
        .find(&receipt_id)
        .ok_or("Safe lead receipt not found")?;
    if receipt.owner_character_id != character_id || !receipt.consumed_by.is_empty() {
        return Err("Safe lead receipt is stale or belongs to another observer".into());
    }
    if matches!(
        receipt.destination_stage.as_str(),
        "exact_believed" | "visited"
    ) {
        let site = ctx
            .db
            .case_site_authority()
            .id_key()
            .find(&receipt.exact_location_id)
            .ok_or("Exact lead must name a server-issued case site")?;
        if site.case_id != receipt.public_case_id
            || site.latitude_e7 != receipt.latitude_e7
            || site.longitude_e7 != receipt.longitude_e7
        {
            return Err("Exact lead no longer matches the case-site authority".into());
        }
    }
    let lead_id = inv::compound_id(&["lead", &character_id.to_string(), &receipt_id]);
    if !receipt.correction_of_lead_id.is_empty() {
        let mut prior = ctx
            .db
            .investigation_lead()
            .id()
            .find(&receipt.correction_of_lead_id)
            .ok_or("Correction target lead not found")?;
        if prior.owner_character_id != character_id || prior.case_id != receipt.public_case_id {
            return Err("Correction target does not match observer and case".into());
        }
        prior.corrected_by = lead_id.clone();
        ctx.db.investigation_lead().id().update(prior);
    }
    ctx.db.investigation_lead().insert(InvestigationLead {
        id: lead_id,
        owner_character_id: character_id,
        case_id: receipt.public_case_id.clone(),
        summary: receipt.summary.clone(),
        source_label: receipt.safe_source_label.clone(),
        confidence_bps: receipt.confidence_bps,
        destination_stage: receipt.destination_stage.clone(),
        directions: receipt.directions.clone(),
        exact_location_id: receipt.exact_location_id.clone(),
        latitude_e7: receipt.latitude_e7,
        longitude_e7: receipt.longitude_e7,
        witness_name: String::new(),
        witness_description: String::new(),
        witness_occupation_or_relationship: String::new(),
        expected_location: String::new(),
        current_learned_location: String::new(),
        contradiction_group: receipt.conflict_group.clone(),
        corrected_by: String::new(),
        recorded_at: official_minute(ctx),
    });
    receipt.consumed_by = action_id.clone();
    ctx.db
        .investigation_safe_lead_receipt()
        .id()
        .update(receipt);
    record_action(ctx, action_id, character_id, "discover_lead", payload);
    Ok(())
}

fn same_place(ctx: &ReducerContext, left: &crate::Character, right: &crate::Character) -> bool {
    let left_site = character_case_site_id(ctx, left.id);
    let right_site = character_case_site_id(ctx, right.id);
    (left.current_settlement_id.is_some()
        && left.current_settlement_id == right.current_settlement_id)
        || (left_site.is_some() && left_site == right_site)
}

#[reducer]
pub fn share_investigation_lead(
    ctx: &ReducerContext,
    sender_id: u64,
    recipient_id: u64,
    source_lead_id: String,
    action_id: String,
) -> Result<(), String> {
    let sender = require_actor(ctx, sender_id)?;
    bounded(&source_lead_id)?;
    let recipient = ctx
        .db
        .character()
        .id()
        .find(recipient_id)
        .ok_or("Recipient not found")?;
    if !recipient.alive
        || sender.party_id.is_none()
        || sender.party_id != recipient.party_id
        || !same_place(ctx, &sender, &recipient)
    {
        return Err("Recipient must be a living, co-located member of the sender's party".into());
    }
    let source = ctx
        .db
        .investigation_lead()
        .id()
        .find(&source_lead_id)
        .ok_or("Source lead not found")?;
    if source.owner_character_id != sender_id {
        return Err("Cannot share another observer's lead".into());
    }
    let payload = canonical_payload(&[
        &recipient_id.to_string(),
        &source_lead_id,
        &source.summary,
        &source.source_label,
        &source.confidence_bps.to_string(),
        &source.destination_stage,
        &source.directions,
        &source.exact_location_id,
        &source.latitude_e7.to_string(),
        &source.longitude_e7.to_string(),
        &source.witness_name,
        &source.witness_description,
        &source.witness_occupation_or_relationship,
        &source.expected_location,
        &source.current_learned_location,
        &source.contradiction_group,
        &source.corrected_by,
    ])?;
    if idempotent(ctx, &action_id, sender_id, "share_lead", &payload)? {
        return Ok(());
    }
    let receipt_id = inv::compound_id(&[
        "share-lead",
        &sender_id.to_string(),
        &recipient_id.to_string(),
        &source_lead_id,
        &payload,
    ]);
    if let Some(existing) = ctx
        .db
        .investigation_sharing_receipt()
        .id()
        .find(&receipt_id)
    {
        if existing.payload_fingerprint != payload {
            return Err("Semantic share receipt payload mismatch".into());
        }
        record_action(ctx, action_id, sender_id, "share_lead", payload);
        return Ok(());
    }
    let copy_id = inv::compound_id(&["shared-lead", &recipient_id.to_string(), &receipt_id]);
    ctx.db.investigation_lead().insert(InvestigationLead {
        id: copy_id,
        owner_character_id: recipient_id,
        source_label: format!("shared by character {sender_id}"),
        ..source
    });
    ctx.db
        .investigation_sharing_receipt()
        .insert(InvestigationSharingReceipt {
            id: receipt_id,
            sender_id,
            recipient_id,
            source_record_id: source_lead_id,
            payload_fingerprint: payload.clone(),
            shared_at: official_minute(ctx),
        });
    record_action(ctx, action_id, sender_id, "share_lead", payload);
    Ok(())
}

#[reducer]
pub fn share_investigation_belief(
    ctx: &ReducerContext,
    sender_id: u64,
    recipient_id: u64,
    source_belief_id: String,
    action_id: String,
) -> Result<(), String> {
    let sender = require_actor(ctx, sender_id)?;
    bounded(&source_belief_id)?;
    let recipient = ctx
        .db
        .character()
        .id()
        .find(recipient_id)
        .ok_or("Recipient not found")?;
    if !recipient.alive
        || sender.party_id.is_none()
        || sender.party_id != recipient.party_id
        || !same_place(ctx, &sender, &recipient)
    {
        return Err("Recipient must be a living, co-located member of the sender's party".into());
    }
    let source = ctx
        .db
        .investigation_belief()
        .id()
        .find(&source_belief_id)
        .ok_or("Source belief not found")?;
    if source.owner_character_id != sender_id {
        return Err("Cannot share another observer's belief".into());
    }
    let payload = canonical_payload(&[
        &recipient_id.to_string(),
        &source_belief_id,
        &source.current_revision_id,
        &source.case_id,
        &source.proposition_id,
        &source.statement,
        &source.confidence_bps.to_string(),
    ])?;
    if idempotent(ctx, &action_id, sender_id, "share_belief", &payload)? {
        return Ok(());
    }
    let recipient_belief_id = inv::compound_id(&[
        "belief",
        &recipient_id.to_string(),
        &source_belief_id,
        &source.current_revision_id,
    ]);
    let existing = ctx
        .db
        .investigation_belief()
        .id()
        .find(&recipient_belief_id);
    let revision = existing.as_ref().map_or(1, |_| {
        ctx.db
            .investigation_belief_revision()
            .owner_character_id()
            .filter(recipient_id)
            .filter(|r| r.belief_id == recipient_belief_id)
            .count()
            .saturating_add(1) as u16
    });
    let revision_id = inv::compound_id(&["revision", &recipient_belief_id, &revision.to_string()]);
    let receipt_id = inv::compound_id(&[
        "share-belief",
        &sender_id.to_string(),
        &recipient_id.to_string(),
        &source_belief_id,
        &source.current_revision_id,
        &payload,
    ]);
    if let Some(existing_share) = ctx
        .db
        .investigation_sharing_receipt()
        .id()
        .find(&receipt_id)
    {
        if existing_share.payload_fingerprint != payload {
            return Err("Semantic share receipt payload mismatch".into());
        }
        record_action(ctx, action_id, sender_id, "share_belief", payload);
        return Ok(());
    }
    ctx.db
        .investigation_belief_revision()
        .insert(InvestigationBeliefRevision {
            id: revision_id.clone(),
            owner_character_id: recipient_id,
            belief_id: recipient_belief_id.clone(),
            revision,
            statement: source.statement.clone(),
            confidence_bps: source.confidence_bps,
            provenance_kind: "shared_by".into(),
            provenance_label: format!("shared by character {sender_id}"),
            supersedes: existing
                .as_ref()
                .map_or_else(String::new, |belief| belief.current_revision_id.clone()),
            recorded_at: official_minute(ctx),
        });
    let copy = InvestigationBelief {
        id: recipient_belief_id,
        owner_character_id: recipient_id,
        case_id: source.case_id,
        proposition_id: source.proposition_id,
        current_revision_id: revision_id,
        statement: source.statement,
        confidence_bps: source.confidence_bps,
        conflict_group: source.conflict_group,
    };
    if existing.is_some() {
        ctx.db.investigation_belief().id().update(copy);
    } else {
        ctx.db.investigation_belief().insert(copy);
    }
    ctx.db
        .investigation_sharing_receipt()
        .insert(InvestigationSharingReceipt {
            id: receipt_id,
            sender_id,
            recipient_id,
            source_record_id: source_belief_id,
            payload_fingerprint: payload.clone(),
            shared_at: official_minute(ctx),
        });
    record_action(ctx, action_id, sender_id, "share_belief", payload);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_exact_rows_are_sanitized_without_coordinates() {
        let row = InvestigationLead {
            id: "lead".into(),
            owner_character_id: 1,
            case_id: "case".into(),
            summary: "somewhere north".into(),
            source_label: "witness".into(),
            confidence_bps: 5000,
            destination_stage: "approximate_area".into(),
            directions: "north wood".into(),
            exact_location_id: "hidden-cave".into(),
            latitude_e7: 12,
            longitude_e7: 34,
            witness_name: String::new(),
            witness_description: String::new(),
            witness_occupation_or_relationship: String::new(),
            expected_location: String::new(),
            current_learned_location: String::new(),
            contradiction_group: String::new(),
            corrected_by: String::new(),
            recorded_at: 1,
        };
        let safe = sanitize_lead(row);
        assert!(safe.exact_location_id.is_empty());
        assert_eq!((safe.latitude_e7, safe.longitude_e7), (0, 0));
    }

    #[test]
    fn destination_validation_is_bidirectional_and_bounded() {
        assert!(
            validate_destination("exact_believed", "cave", 900_000_000, -1_800_000_000).is_ok()
        );
        assert!(validate_destination("visited", "", 1, 2).is_err());
        assert!(validate_destination("exact_believed", "cave", 900_000_001, 0).is_err());
        assert!(validate_destination("exact_believed", "cave", 0, -1_800_000_001).is_err());
        assert!(validate_destination("approximate_area", "hidden", 0, 0).is_err());
        assert!(validate_destination("textual", "", 0, 0).is_ok());
    }

    #[test]
    fn raw_tables_are_private_and_views_fail_closed() {
        let source = include_str!("investigation.rs");
        for table in [
            "case_site_authority",
            "party_case_site_tracking",
            "investigation_case_authority",
            "investigation_event_authority",
            "investigation_observation",
            "investigation_recollection",
            "investigation_claim",
            "investigation_evidence_authority",
            "investigation_evidence_knowledge",
            "investigation_belief",
            "investigation_belief_revision",
            "investigation_lead",
            "investigation_sharing_receipt",
        ] {
            let declaration = format!("#[table(accessor = {table})]");
            assert!(source.contains(&declaration));
            assert!(!source.contains(&format!("#[table(accessor = {table}, public)]")));
        }
        assert_eq!(source.matches("if !is_gateway(ctx)").count(), 3);
        assert!(!source.contains("pub hidden_target"));
    }

    #[test]
    fn case_site_projection_requires_exact_unrevised_observer_knowledge() {
        let source = include_str!("investigation.rs");
        let projection = source
            .split("pub fn backend_case_site_pins")
            .nth(1)
            .and_then(|tail| {
                tail.split("pub(crate) fn exact_case_site_for_observer")
                    .next()
            })
            .expect("case-site projection body");
        assert!(projection.contains("lead.corrected_by.is_empty()"));
        assert!(projection.contains("\"exact_believed\" | \"visited\""));
        assert!(projection.contains("owner_character_id: lead.owner_character_id"));
        assert!(projection.contains("case_site_authority()"));
        assert!(!projection.contains("destination_stage.as_str(), \"textual\""));
        assert!(!projection.contains("destination_stage.as_str(), \"approximate_area\""));
    }

    #[test]
    fn source_has_authorization_idempotency_and_no_implicit_sharing() {
        let source = include_str!("investigation.rs");
        assert!(source.contains("require_strategic_gateway(ctx)?"));
        assert!(source.contains("different payload"));
        assert!(source.contains("co-located member"));
        assert!(source.contains("share_investigation_belief"));
        assert!(!source.contains("on_party_join"));
        assert!(source.contains("compound_id(&[\"case\", \"problem\""));
        assert!(!source.contains("case_id = receipt.opaque_case_ref"));
        assert!(source.contains("local_problem_receipt().id().find(&receipt_id)"));
        assert!(source.contains("Evidence knowledge has conflicting provenance"));
        assert!(!source.contains("#[table(accessor = investigation_evidence_knowledge, public)]"));
    }
}
