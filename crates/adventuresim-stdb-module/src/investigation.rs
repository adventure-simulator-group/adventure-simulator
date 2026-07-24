//! Private investigation authority and observer-safe gateway projections.

use crate::{
    character::{character, character__view, character_skills},
    condition::character_strategic_condition__view,
    local_problem::local_problem_receipt,
    settlement_population::{settlement_npc, settlement_npc_presence},
    strategic::{
        CustodyHolderKind, CustodyObjectKind, case_authority, case_custody,
        coordinate_distance_e7_m, living_party_member_ids, party_authority, party_authority__view,
        party_journey_authority, party_member__view, quest_generation_authority,
        require_no_unresolved_encounter, require_party_ready,
        require_strategic_character_authority, require_strategic_gateway, settlement,
        strategic_gateway_authority__view,
    },
    time::{
        advance_investigation_time, character_time, synchronize_party_activity_time, world_clock,
    },
};
use adventuresim_core::investigation as inv;
use adventuresim_core::investigation_action as action;
use adventuresim_core::skill::Skill;
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};
use std::collections::BTreeMap;

const MAX_TEXT: usize = 512;
const AREA_RADIUS_TOLERANCE_M: u64 = 1;

fn coordinate_area_contains_e7(
    center_longitude_e7: i32,
    center_latitude_e7: i32,
    radius_m: u32,
    area_coordinates_are_geographic: bool,
    longitude_e7: i32,
    latitude_e7: i32,
    point_coordinates_are_geographic: bool,
) -> bool {
    if area_coordinates_are_geographic != point_coordinates_are_geographic {
        return false;
    }
    coordinate_distance_e7_m(
        center_longitude_e7,
        center_latitude_e7,
        longitude_e7,
        latitude_e7,
        area_coordinates_are_geographic,
    )
    .is_some_and(|distance_m| {
        distance_m <= u64::from(radius_m).saturating_add(AREA_RADIUS_TOLERANCE_M)
    })
}

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
    pub proposition_id: String,
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

/// Private geometry for an imprecise lead. Its canonical center and radius
/// never cross the gateway boundary as a map pin.
#[derive(Clone, Debug)]
#[table(accessor = investigation_area_authority)]
pub struct InvestigationAreaAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub case_id: String,
    pub origin_settlement_id: String,
    pub safe_label: String,
    pub center_longitude_e7: i32,
    pub center_latitude_e7: i32,
    pub radius_m: u32,
    pub coordinates_are_geographic: bool,
    pub terrain: String,
}

/// Observer-bound private action authority. `target_*`, the resolution seed,
/// and the server-authored consequence are intentionally absent from public
/// projections.
#[derive(Clone, Debug)]
#[table(accessor = investigation_action_capability)]
pub struct InvestigationActionCapability {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    #[index(btree)]
    pub case_id: String,
    pub method: String,
    pub version: u32,
    pub target_kind: String,
    pub target_id: String,
    pub target_terrain: String,
    pub seed: u64,
    pub evidence_age_origin_minute: u64,
    pub uncertainty_bps: u16,
    pub safe_summary: String,
    pub known_prerequisites: String,
    pub safe_result_on_success: String,
    pub consequence_json: String,
    pub required_action_id: String,
    pub alternate_route_action_id: String,
    pub active: bool,
}

/// Private typed output blueprint for generated capabilities. Free-form
/// result wording never grants evidence, custody, or destination knowledge.
#[derive(Clone, Debug)]
#[table(accessor = investigation_generated_action_output)]
pub struct InvestigationGeneratedActionOutput {
    #[primary_key]
    pub capability_id: String,
    pub outputs_json: String,
}

/// Private binding from an opaque learned cohort to one persistent NPC and
/// the exact demographic/presence facts authored at generation time.
#[derive(Clone, Debug)]
#[table(accessor = investigation_pattern_target_authority)]
pub struct InvestigationPatternTargetAuthority {
    #[primary_key]
    pub cohort_id: String,
    #[index(btree)]
    pub case_id: String,
    pub npc_id: String,
    pub demographic: String,
    pub age_band: String,
    pub sex: String,
    pub profession: String,
    pub expected_settlement_id: String,
    pub expected_location: String,
    pub presence_version: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_action_attempt)]
pub struct InvestigationActionAttempt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub capability_id: String,
    pub owner_character_id: u64,
    pub expected_version: u32,
    pub method: String,
    pub started_at: u64,
    pub completed_at: u64,
    pub duration_minutes: u32,
    pub success: bool,
    pub resulting_uncertainty_bps: u16,
    pub private_resolution_json: String,
}

#[derive(Clone, Debug)]
#[table(accessor = investigation_action_outcome)]
pub struct InvestigationActionOutcome {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    pub case_id: String,
    pub capability_id: String,
    pub safe_wording: String,
    pub recorded_at: u64,
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

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendInvestigationAction {
    pub owner_character_id: u64,
    pub action_id: String,
    pub method: String,
    pub expected_version: u32,
    pub summary: String,
    pub known_prerequisites: String,
    pub duration_min_minutes: u32,
    pub duration_max_minutes: u32,
    pub uncertainty_bps: u16,
    pub skill_contributions: String,
    pub weather_available: bool,
    pub required_case_site_id: String,
    pub available: bool,
    pub can_travel_to_required_site: bool,
    pub unavailable_reason: String,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendInvestigationActionOutcome {
    pub owner_character_id: u64,
    pub outcome_id: String,
    pub action_id: String,
    pub wording: String,
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

#[view(accessor = backend_investigation_actions, public)]
pub fn backend_investigation_actions(ctx: &ViewContext) -> Vec<BackendInvestigationAction> {
    if !is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .investigation_action_capability()
        .owner_character_id()
        .filter(0u64..)
        .filter(|capability| capability.active)
        .filter_map(|capability| {
            let kind = parse_action_kind(&capability.method).ok()?;
            if capability_has_successful_attempt_view(ctx, &capability.id)
                || !capability_has_live_support_view(ctx, &capability, kind)
            {
                return None;
            }
            let cost = action::base_cost(kind);
            let required_case_site_id =
                exact_action_site_for_observer(ctx, &capability, kind).unwrap_or_default();
            let availability =
                action_unavailable_reason_view(ctx, &capability, &required_case_site_id);
            Some(BackendInvestigationAction {
                owner_character_id: capability.owner_character_id,
                action_id: capability.id,
                method: capability.method,
                expected_version: capability.version,
                summary: capability.safe_summary,
                known_prerequisites: capability.known_prerequisites,
                duration_min_minutes: (cost.minutes / 2).max(15),
                duration_max_minutes: cost.minutes.saturating_mul(3) / 2,
                uncertainty_bps: capability.uncertainty_bps,
                skill_contributions:
                    "terrain, awareness, stealth, local familiarity, and bounded party assistance"
                        .into(),
                weather_available: false,
                required_case_site_id,
                available: availability.unavailable_reason.is_none(),
                can_travel_to_required_site: availability.can_travel_to_required_site,
                unavailable_reason: availability.unavailable_reason.unwrap_or_default(),
            })
        })
        .collect()
}

fn capability_has_successful_attempt_view(ctx: &ViewContext, capability_id: &str) -> bool {
    ctx.db
        .investigation_action_attempt()
        .capability_id()
        .filter(capability_id)
        .any(|attempt| attempt.success)
}

fn lead_is_live_contact_referral(
    lead: &InvestigationLead,
    owner_character_id: u64,
    case_id: &str,
) -> bool {
    lead.owner_character_id == owner_character_id
        && lead.case_id == case_id
        && !lead.witness_name.is_empty()
        && lead.corrected_by.is_empty()
}

fn capability_has_live_support_view(
    ctx: &ViewContext,
    capability: &InvestigationActionCapability,
    kind: action::InvestigationActionKind,
) -> bool {
    if !capability.required_action_id.is_empty()
        && !capability_has_successful_attempt_view(ctx, &capability.required_action_id)
    {
        return false;
    }
    if kind == action::InvestigationActionKind::InspectSite
        && capability.target_kind == "site"
        && exact_action_site_for_observer(ctx, capability, kind).is_none()
    {
        return false;
    }
    let prerequisites = action::prerequisites(kind);
    if prerequisites.requires_contact_referral
        && !ctx
            .db
            .investigation_lead()
            .owner_character_id()
            .filter(capability.owner_character_id)
            .any(|lead| {
                lead_is_live_contact_referral(
                    &lead,
                    capability.owner_character_id,
                    &capability.case_id,
                )
            })
    {
        return false;
    }
    if prerequisites.requires_approximate_destination
        && capability.target_kind != "area"
        && !ctx
            .db
            .investigation_lead()
            .owner_character_id()
            .filter(capability.owner_character_id)
            .any(|lead| {
                lead.case_id == capability.case_id
                    && lead.destination_stage == "approximate_area"
                    && lead.corrected_by.is_empty()
            })
    {
        return false;
    }
    !prerequisites.requires_tracks || !capability.required_action_id.is_empty()
}

fn exact_action_site_for_observer(
    ctx: &ViewContext,
    capability: &InvestigationActionCapability,
    kind: action::InvestigationActionKind,
) -> Option<String> {
    if kind != action::InvestigationActionKind::InspectSite || capability.target_kind != "site" {
        return None;
    }
    let lead = ctx
        .db
        .investigation_lead()
        .owner_character_id()
        .filter(capability.owner_character_id)
        .find(|lead| {
            lead.case_id == capability.case_id
                && lead.exact_location_id == capability.target_id
                && lead.corrected_by.is_empty()
                && matches!(
                    lead.destination_stage.as_str(),
                    "exact_believed" | "visited"
                )
        })?;
    let site = ctx
        .db
        .case_site_authority()
        .id_key()
        .find(&lead.exact_location_id)?;
    exact_site_knowledge_is_live(
        &capability.case_id,
        &capability.target_id,
        &lead.case_id,
        &lead.exact_location_id,
        &lead.destination_stage,
        &lead.corrected_by,
        &site.case_id,
        &site.id.value,
        lead.latitude_e7 == site.latitude_e7 && lead.longitude_e7 == site.longitude_e7,
    )
    .then_some(lead.exact_location_id)
}

fn exact_site_knowledge_is_live(
    capability_case_id: &str,
    capability_target_id: &str,
    lead_case_id: &str,
    lead_exact_location_id: &str,
    lead_destination_stage: &str,
    lead_corrected_by: &str,
    authority_case_id: &str,
    authority_site_id: &str,
    coordinates_match: bool,
) -> bool {
    capability_case_id == lead_case_id
        && capability_case_id == authority_case_id
        && capability_target_id == lead_exact_location_id
        && capability_target_id == authority_site_id
        && matches!(lead_destination_stage, "exact_believed" | "visited")
        && lead_corrected_by.is_empty()
        && coordinates_match
}

struct ProjectedActionAvailability {
    unavailable_reason: Option<String>,
    can_travel_to_required_site: bool,
}

fn projected_action_availability(
    party_ready: bool,
    required_case_site_id: &str,
    occupying_required_site: bool,
) -> ProjectedActionAvailability {
    if !party_ready {
        return ProjectedActionAvailability {
            unavailable_reason: Some(
                "An incapacitated party member must recover before the party can investigate."
                    .into(),
            ),
            can_travel_to_required_site: false,
        };
    }
    if !required_case_site_id.is_empty() && !occupying_required_site {
        return ProjectedActionAvailability {
            unavailable_reason: Some(
                "Travel to the known investigation site before inspecting it.".into(),
            ),
            can_travel_to_required_site: true,
        };
    }
    ProjectedActionAvailability {
        unavailable_reason: None,
        can_travel_to_required_site: false,
    }
}

fn action_unavailable_reason_view(
    ctx: &ViewContext,
    capability: &InvestigationActionCapability,
    required_case_site_id: &str,
) -> ProjectedActionAvailability {
    let Some(character) = ctx.db.character().id().find(capability.owner_character_id) else {
        return ProjectedActionAvailability {
            unavailable_reason: Some(
                "The investigating character is currently unavailable.".into(),
            ),
            can_travel_to_required_site: false,
        };
    };
    let Some(party_id) = character.party_id else {
        return ProjectedActionAvailability {
            unavailable_reason: Some(
                "Join or form a party before attempting this investigation.".into(),
            ),
            can_travel_to_required_site: false,
        };
    };
    let party_ready = !ctx
        .db
        .party_member()
        .party_id()
        .filter(&party_id)
        .filter_map(|membership| ctx.db.character().id().find(membership.character_id))
        .filter(|member| member.alive)
        .any(|member| {
            ctx.db
                .character_strategic_condition()
                .character_id()
                .find(member.id)
                .is_some_and(|condition| condition.status == "incapacitated")
        });
    let occupying_required_site = !required_case_site_id.is_empty()
        && ctx
            .db
            .party_authority()
            .id()
            .find(&party_id)
            .and_then(|party| party.current_case_site_id)
            .is_some_and(|site| site.value == required_case_site_id);
    projected_action_availability(party_ready, required_case_site_id, occupying_required_site)
}

#[view(accessor = backend_investigation_action_outcomes, public)]
pub fn backend_investigation_action_outcomes(
    ctx: &ViewContext,
) -> Vec<BackendInvestigationActionOutcome> {
    if !is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .investigation_action_outcome()
        .owner_character_id()
        .filter(0u64..)
        .map(|outcome| BackendInvestigationActionOutcome {
            owner_character_id: outcome.owner_character_id,
            outcome_id: outcome.id,
            action_id: outcome.capability_id,
            wording: outcome.safe_wording,
            recorded_at: outcome.recorded_at,
        })
        .collect()
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum InvestigationActionConsequence {
    None,
    RetrieveAsset { asset_id: String, version: u32 },
    RescueSubject { subject_id: String, version: u32 },
}

fn parse_action_kind(value: &str) -> Result<action::InvestigationActionKind, String> {
    use action::InvestigationActionKind as K;
    match value {
        "inspect_site" => Ok(K::InspectSite),
        "search_area" => Ok(K::SearchArea),
        "follow_tracks" => Ok(K::FollowTracks),
        "reacquire_tracks" => Ok(K::ReacquireTracks),
        "locate_contact" => Ok(K::LocateContact),
        "watch" => Ok(K::Watch),
        "patrol" => Ok(K::Patrol),
        "lay_ambush" => Ok(K::LayAmbush),
        "approach_lead" => Ok(K::ApproachLead),
        _ => Err("Unknown investigation action method".into()),
    }
}

fn action_method(kind: action::InvestigationActionKind) -> &'static str {
    use action::InvestigationActionKind as K;
    match kind {
        K::InspectSite => "inspect_site",
        K::SearchArea => "search_area",
        K::FollowTracks => "follow_tracks",
        K::ReacquireTracks => "reacquire_tracks",
        K::LocateContact => "locate_contact",
        K::Watch => "watch",
        K::Patrol => "patrol",
        K::LayAmbush => "lay_ambush",
        K::ApproachLead => "approach_lead",
    }
}

fn parse_action_terrain(value: &str) -> Result<action::Terrain, String> {
    use action::Terrain as T;
    match value {
        "road" => Ok(T::Road),
        "settlement" => Ok(T::Settlement),
        "plains" => Ok(T::Plains),
        "forest" => Ok(T::Forest),
        "hills" => Ok(T::Hills),
        "marsh" => Ok(T::Marsh),
        "ruins" => Ok(T::Ruins),
        "underground" => Ok(T::Underground),
        _ => Err("Unknown investigation terrain".into()),
    }
}

/// Trusted generator seam. The opaque id is the only authority returned to a
/// browser. Hidden targets, seeds, and consequences remain private.
#[allow(clippy::too_many_arguments)]
fn validate_investigation_action_text(
    id: &str,
    case_id: &str,
    target_kind: &str,
    target_id: &str,
    safe_summary: &str,
    known_prerequisites: &str,
    safe_result_on_success: &str,
    required_action_id: &str,
    alternate_route_action_id: &str,
) -> Result<(), String> {
    for text in [
        id,
        case_id,
        target_kind,
        target_id,
        safe_summary,
        known_prerequisites,
        safe_result_on_success,
        alternate_route_action_id,
    ] {
        bounded(text)?;
    }
    // Root actions have no predecessor. Successor actions still carry the
    // observer-scoped prerequisite id and are validated as ordinary text.
    bounded_optional(required_action_id)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn issue_investigation_action_capability(
    ctx: &ReducerContext,
    id: String,
    owner_character_id: u64,
    case_id: String,
    kind: action::InvestigationActionKind,
    target_kind: String,
    target_id: String,
    target_terrain: action::Terrain,
    seed: u64,
    uncertainty_bps: u16,
    safe_summary: String,
    known_prerequisites: String,
    safe_result_on_success: String,
    consequence: InvestigationActionConsequence,
    required_action_id: String,
    alternate_route_action_id: String,
) -> Result<(), String> {
    validate_investigation_action_text(
        &id,
        &case_id,
        &target_kind,
        &target_id,
        &safe_summary,
        &known_prerequisites,
        &safe_result_on_success,
        &required_action_id,
        &alternate_route_action_id,
    )?;
    bps(uncertainty_bps)?;
    if alternate_route_action_id == id {
        return Err("A critical action needs a distinct recovery route".into());
    }
    if ctx
        .db
        .investigation_action_capability()
        .id()
        .find(&id)
        .is_some()
    {
        return Err("Investigation action capability already exists".into());
    }
    let target_exists = match target_kind.as_str() {
        "site" => ctx
            .db
            .case_site_authority()
            .id_key()
            .find(&target_id)
            .is_some(),
        "area" => ctx
            .db
            .investigation_area_authority()
            .id()
            .find(&target_id)
            .is_some(),
        "cohort" => ctx
            .db
            .investigation_pattern_target_authority()
            .cohort_id()
            .find(&target_id)
            .is_some_and(|target| target.case_id == case_id),
        "contact" | "route" | "tracks" => true,
        _ => false,
    };
    if !target_exists {
        return Err("Investigation action target is not authoritative".into());
    }
    ctx.db
        .investigation_action_capability()
        .insert(InvestigationActionCapability {
            id,
            owner_character_id,
            case_id,
            method: action_method(kind).into(),
            version: 0,
            target_kind,
            target_id,
            target_terrain: format!("{target_terrain:?}").to_ascii_lowercase(),
            seed,
            evidence_age_origin_minute: character_strategic_minute(ctx, owner_character_id),
            uncertainty_bps,
            safe_summary,
            known_prerequisites,
            safe_result_on_success,
            consequence_json: serde_json::to_string(&consequence)
                .map_err(|_| "Investigation action consequence is invalid")?,
            required_action_id,
            alternate_route_action_id,
            active: false,
        });
    Ok(())
}

fn character_strategic_minute(ctx: &ReducerContext, character_id: u64) -> u64 {
    ctx.db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or_else(|| official_minute(ctx), |time| time.minutes)
}

fn generated_observer_id(
    ctx: &ReducerContext,
    case_id: &str,
    kind: &str,
    name: &str,
) -> Option<String> {
    ctx.db
        .quest_generation_authority()
        .case_id()
        .find(&case_id.to_string())
        .and_then(|authority| {
            serde_json::from_str::<adventuresim_core::quest_generation::GenerationContext>(
                &authority.context_snapshot_json,
            )
            .ok()
        })
        .map(|context| {
            adventuresim_core::quest_generation::observer_scoped_id(&context, kind, name)
        })
}

fn set_action_active(ctx: &ReducerContext, action_id: &str, active: bool) -> Result<(), String> {
    let mut capability = ctx
        .db
        .investigation_action_capability()
        .id()
        .find(&action_id.to_string())
        .ok_or("Investigation route capability is missing")?;
    capability.active = active;
    ctx.db
        .investigation_action_capability()
        .id()
        .update(capability);
    Ok(())
}

fn validate_action_route_graph(
    ctx: &ReducerContext,
    owner_character_id: u64,
    case_id: &str,
) -> Result<(), String> {
    let capabilities: Vec<_> = ctx
        .db
        .investigation_action_capability()
        .owner_character_id()
        .filter(owner_character_id)
        .filter(|capability| capability.case_id == case_id)
        .collect();
    if capabilities.len() < 2 {
        return Err("Investigation needs at least two playable routes".into());
    }
    for capability in &capabilities {
        let alternate = capabilities
            .iter()
            .find(|candidate| candidate.id == capability.alternate_route_action_id)
            .ok_or("Investigation alternate route is missing")?;
        if alternate.id == capability.id
            || alternate.owner_character_id != capability.owner_character_id
            || alternate.case_id != capability.case_id
        {
            return Err("Investigation alternate route crosses authority boundaries".into());
        }
    }
    if capabilities
        .iter()
        .filter(|capability| capability.active)
        .count()
        < 2
    {
        return Err("Investigation needs two immediately playable routes".into());
    }
    Ok(())
}

fn activate_action_successors(
    ctx: &ReducerContext,
    capability: &InvestigationActionCapability,
    succeeded: bool,
) -> Result<bool, String> {
    let mut activate = Vec::new();
    if succeeded {
        activate.extend(
            ctx.db
                .investigation_action_capability()
                .owner_character_id()
                .filter(capability.owner_character_id)
                .filter(|candidate| {
                    candidate.case_id == capability.case_id
                        && candidate.required_action_id == capability.id
                })
                .map(|candidate| candidate.id),
        );
    } else {
        use adventuresim_core::quest_generation::{
            FailedActionAlternateTransition, ReferredContactActionState,
            transition_failed_action_alternate,
        };
        let capabilities: Vec<_> = ctx
            .db
            .investigation_action_capability()
            .owner_character_id()
            .filter(capability.owner_character_id)
            .filter(|candidate| candidate.case_id == capability.case_id)
            .collect();
        let mut states = capabilities
            .iter()
            .map(|candidate| ReferredContactActionState {
                id: candidate.id.clone(),
                owner_character_id: candidate.owner_character_id,
                case_id: candidate.case_id.clone(),
                method: candidate.method.clone(),
                target_kind: candidate.target_kind.clone(),
                target_id: candidate.target_id.clone(),
                required_action_id: candidate.required_action_id.clone(),
                active: candidate.active,
                version: candidate.version,
                successful_attempt: ctx
                    .db
                    .investigation_action_attempt()
                    .capability_id()
                    .filter(&candidate.id)
                    .any(|attempt| attempt.success),
            })
            .collect::<Vec<_>>();
        match transition_failed_action_alternate(
            &mut states,
            capability.owner_character_id,
            &capability.case_id,
            &capability.alternate_route_action_id,
        )? {
            FailedActionAlternateTransition::Activated { alternate_id } => {
                let alternate = capabilities
                    .iter()
                    .find(|candidate| candidate.id == alternate_id)
                    .ok_or("Investigation recovery route no longer exists")?;
                let kind = parse_action_kind(&alternate.method)?;
                if capability_has_live_support_reducer(ctx, alternate, kind) {
                    activate.push(alternate_id);
                }
            }
            FailedActionAlternateTransition::Unavailable => {}
        }
    }
    let alternate_available = !succeeded && !activate.is_empty();
    for id in activate {
        set_action_active(ctx, &id, true)?;
    }
    Ok(alternate_available)
}

fn capability_has_live_support_reducer(
    ctx: &ReducerContext,
    capability: &InvestigationActionCapability,
    kind: action::InvestigationActionKind,
) -> bool {
    if !capability.required_action_id.is_empty()
        && !ctx
            .db
            .investigation_action_attempt()
            .capability_id()
            .filter(&capability.required_action_id)
            .any(|attempt| attempt.success)
    {
        return false;
    }
    if kind == action::InvestigationActionKind::InspectSite && capability.target_kind == "site" {
        let Some((site, lead)) =
            exact_case_site_for_observer(ctx, capability.owner_character_id, &capability.target_id)
        else {
            return false;
        };
        if !exact_site_knowledge_is_live(
            &capability.case_id,
            &capability.target_id,
            &lead.case_id,
            &lead.exact_location_id,
            &lead.destination_stage,
            &lead.corrected_by,
            &site.case_id,
            &site.id.value,
            lead.latitude_e7 == site.latitude_e7 && lead.longitude_e7 == site.longitude_e7,
        ) {
            return false;
        }
    }
    let prerequisites = action::prerequisites(kind);
    if prerequisites.requires_contact_referral
        && !ctx
            .db
            .investigation_lead()
            .owner_character_id()
            .filter(capability.owner_character_id)
            .any(|lead| {
                lead_is_live_contact_referral(
                    &lead,
                    capability.owner_character_id,
                    &capability.case_id,
                )
            })
    {
        return false;
    }
    if prerequisites.requires_approximate_destination
        && capability.target_kind != "area"
        && !ctx
            .db
            .investigation_lead()
            .owner_character_id()
            .filter(capability.owner_character_id)
            .any(|lead| {
                lead.case_id == capability.case_id
                    && lead.destination_stage == "approximate_area"
                    && lead.corrected_by.is_empty()
            })
    {
        return false;
    }
    !prerequisites.requires_tracks || !capability.required_action_id.is_empty()
}

fn complete_referred_contact_action(
    ctx: &ReducerContext,
    owner_character_id: u64,
    canonical_case_id: &str,
    witness_npc_id: &str,
    dialogue_action_id: &str,
) -> Result<(), String> {
    use adventuresim_core::quest_generation::{
        ReferredContactActionState, ReferredContactTransition, transition_referred_contact_action,
    };
    let capabilities: Vec<_> = ctx
        .db
        .investigation_action_capability()
        .owner_character_id()
        .filter(owner_character_id)
        .filter(|capability| capability.case_id == canonical_case_id)
        .collect();
    let mut states: Vec<_> = capabilities
        .iter()
        .map(|capability| ReferredContactActionState {
            id: capability.id.clone(),
            owner_character_id: capability.owner_character_id,
            case_id: capability.case_id.clone(),
            method: capability.method.clone(),
            target_kind: capability.target_kind.clone(),
            target_id: capability.target_id.clone(),
            required_action_id: capability.required_action_id.clone(),
            active: capability.active,
            version: capability.version,
            successful_attempt: ctx
                .db
                .investigation_action_attempt()
                .capability_id()
                .filter(&capability.id)
                .any(|attempt| attempt.success),
        })
        .collect();
    let transition = transition_referred_contact_action(
        &mut states,
        owner_character_id,
        canonical_case_id,
        witness_npc_id,
    )?;
    let ReferredContactTransition::Applied {
        root_id,
        expected_version,
        next_version,
        activated_successor_ids,
        attempt_success,
        outcome_wording,
    } = transition
    else {
        return Ok(());
    };
    let mut capability = capabilities
        .into_iter()
        .find(|capability| capability.id == root_id)
        .ok_or("Referred contact action disappeared")?;
    if capability.version != expected_version || !capability.active {
        return Err("Referred contact action changed during transition planning".into());
    }
    let completed_at = character_strategic_minute(ctx, owner_character_id);
    let attempt_id = generated_observer_id(
        ctx,
        canonical_case_id,
        "attempt",
        &format!("dialogue:{dialogue_action_id}:{}", capability.id),
    )
    .ok_or("Generated contact action lacks observer-id authority")?;
    ctx.db
        .investigation_action_attempt()
        .insert(InvestigationActionAttempt {
            id: attempt_id.clone(),
            capability_id: capability.id.clone(),
            owner_character_id,
            expected_version: capability.version,
            method: capability.method.clone(),
            started_at: completed_at,
            completed_at,
            duration_minutes: 0,
            success: attempt_success,
            resulting_uncertainty_bps: capability.uncertainty_bps,
            private_resolution_json: serde_json::json!({
                "source": "exact_referred_witness_dialogue"
            })
            .to_string(),
        });
    capability.active = false;
    capability.version = next_version;
    ctx.db
        .investigation_action_capability()
        .id()
        .update(capability.clone());
    let outcome_id = generated_observer_id(ctx, canonical_case_id, "outcome", &attempt_id)
        .ok_or("Generated contact action lacks observer-id authority")?;
    if ctx
        .db
        .investigation_action_outcome()
        .id()
        .find(&outcome_id)
        .is_none()
    {
        ctx.db
            .investigation_action_outcome()
            .insert(InvestigationActionOutcome {
                id: outcome_id,
                owner_character_id,
                case_id: canonical_case_id.into(),
                capability_id: capability.id.clone(),
                safe_wording: outcome_wording,
                recorded_at: official_minute(ctx),
            });
    }
    for successor_id in activated_successor_ids {
        let successor = ctx
            .db
            .investigation_action_capability()
            .id()
            .find(&successor_id)
            .ok_or("Referred contact successor disappeared")?;
        if successor.owner_character_id != owner_character_id
            || successor.case_id != canonical_case_id
            || successor.required_action_id != capability.id
        {
            return Err("Referred contact successor changed during transition planning".into());
        }
        set_action_active(ctx, &successor_id, true)?;
    }
    Ok(())
}

fn issue_rumor_action_graph(
    ctx: &ReducerContext,
    owner_character_id: u64,
    case_id: &str,
    lead_id: &str,
    settlement_id: &str,
    contact_id: &str,
    safe_summary: &str,
) -> Result<(), String> {
    if let Some(authority) = ctx
        .db
        .quest_generation_authority()
        .case_id()
        .find(&case_id.to_string())
    {
        let manifest: adventuresim_core::quest_generation::GeneratedCase =
            serde_json::from_str(&authority.manifest_json)
                .map_err(|_| "Generated action blueprint is invalid")?;
        let generation_context: adventuresim_core::quest_generation::GenerationContext =
            serde_json::from_str(&authority.context_snapshot_json)
                .map_err(|_| "Generated observer-id authority is invalid")?;
        for target in &manifest.pattern_targets {
            let row = InvestigationPatternTargetAuthority {
                cohort_id: target.cohort_id.clone(),
                case_id: case_id.to_string(),
                npc_id: target.npc_id.clone(),
                demographic: format!("{:?}", target.demographic).to_ascii_lowercase(),
                age_band: target.age_band.clone(),
                sex: target.sex.clone(),
                profession: target.profession.clone(),
                expected_settlement_id: target.expected_settlement_id.clone(),
                expected_location: target.expected_location.clone(),
                presence_version: target.presence_version,
            };
            if let Some(existing) = ctx
                .db
                .investigation_pattern_target_authority()
                .cohort_id()
                .find(&row.cohort_id)
            {
                if existing.case_id != row.case_id
                    || existing.npc_id != row.npc_id
                    || existing.demographic != row.demographic
                    || existing.age_band != row.age_band
                    || existing.sex != row.sex
                    || existing.profession != row.profession
                    || existing.expected_settlement_id != row.expected_settlement_id
                    || existing.expected_location != row.expected_location
                    || existing.presence_version != row.presence_version
                {
                    return Err("Generated pattern target authority conflicts".into());
                }
            } else {
                ctx.db.investigation_pattern_target_authority().insert(row);
            }
        }
        for generated in &manifest.actions {
            let capability_id = adventuresim_core::quest_generation::observer_scoped_id(
                &generation_context,
                "capability",
                &format!("{owner_character_id}:{}", generated.id.0),
            );
            if ctx
                .db
                .investigation_action_capability()
                .id()
                .find(&capability_id)
                .is_some()
            {
                continue;
            }
            let remap = |id: &adventuresim_core::quest_generation::ActionId| {
                adventuresim_core::quest_generation::observer_scoped_id(
                    &generation_context,
                    "capability",
                    &format!("{owner_character_id}:{}", id.0),
                )
            };
            let site_terrain = manifest
                .sites
                .iter()
                .find(|site| site.id.0 == generated.target_id)
                .map(|site| site.terrain);
            let area_terrain = manifest
                .areas
                .iter()
                .find(|area| area.id == generated.target_id)
                .map(|area| area.terrain);
            let consequence = generated
                .outputs
                .iter()
                .find_map(|output| match output {
                    adventuresim_core::quest_generation::GeneratedActionOutput::Consequence {
                        consequence:
                            adventuresim_core::quest_generation::GeneratedActionConsequence::RetrieveAsset {
                                asset_id,
                                next_version,
                            },
                    } => Some(InvestigationActionConsequence::RetrieveAsset {
                        asset_id: asset_id.clone(),
                        version: *next_version,
                    }),
                    adventuresim_core::quest_generation::GeneratedActionOutput::Consequence {
                        consequence:
                            adventuresim_core::quest_generation::GeneratedActionConsequence::RescueSubject {
                                subject_id,
                                next_version,
                            },
                    } => Some(InvestigationActionConsequence::RescueSubject {
                        subject_id: subject_id.clone(),
                        version: *next_version,
                    }),
                    _ => None,
                })
                .unwrap_or(InvestigationActionConsequence::None);
            let evidence_summary = |evidence_id: &str| {
                manifest
                    .evidence
                    .iter()
                    .find(|evidence| evidence.id.0 == evidence_id)
                    .map(|evidence| evidence.safe_description.clone())
            };
            let learned_condition = generated.outputs.iter().find_map(|output| match output {
                adventuresim_core::quest_generation::GeneratedActionOutput::PatternCondition {
                    evidence_id,
                    ..
                } => evidence_summary(&evidence_id.0),
                _ => None,
            });
            let earned_clue = generated.outputs.iter().find_map(|output| match output {
                adventuresim_core::quest_generation::GeneratedActionOutput::Evidence {
                    evidence_id,
                } => evidence_summary(&evidence_id.0),
                _ => None,
            });
            issue_investigation_action_capability(
                ctx,
                capability_id,
                owner_character_id,
                case_id.to_string(),
                generated.kind,
                generated.target_kind.clone(),
                generated.target_id.clone(),
                site_terrain.or(area_terrain).unwrap_or(action::Terrain::Settlement),
                ctx.random::<u64>(),
                7_000,
                generated.safe_summary.clone(),
                learned_condition.map_or_else(
                    || "Complete the preceding generated lead and remain with your ready, co-located party.".into(),
                    |clue| format!("First learn and retain this corroborated clue: {clue}"),
                ),
                earned_clue.unwrap_or_else(|| {
                    "The investigation produces a new, source-attributed lead.".into()
                }),
                consequence,
                generated.prerequisite.as_ref().map_or_else(String::new, remap),
                remap(&generated.alternate),
            )?;
            ctx.db.investigation_generated_action_output().insert(
                InvestigationGeneratedActionOutput {
                    capability_id: adventuresim_core::quest_generation::observer_scoped_id(
                        &generation_context,
                        "capability",
                        &format!("{owner_character_id}:{}", generated.id.0),
                    ),
                    outputs_json: serde_json::to_string(&generated.outputs)
                        .map_err(|_| "Could not encode generated action outputs")?,
                },
            );
        }
        for generated in manifest
            .actions
            .iter()
            .filter(|action| action.active_initially)
        {
            set_action_active(
                ctx,
                &adventuresim_core::quest_generation::observer_scoped_id(
                    &generation_context,
                    "capability",
                    &format!("{owner_character_id}:{}", generated.id.0),
                ),
                true,
            )?;
        }
        return validate_action_route_graph(ctx, owner_character_id, case_id);
    }
    let area_id = inv::compound_id(&["area", lead_id]);
    if ctx
        .db
        .investigation_area_authority()
        .id()
        .find(&area_id)
        .is_none()
    {
        let settlement = ctx
            .db
            .settlement()
            .id()
            .find(&settlement_id.to_string())
            .ok_or("Rumor settlement no longer exists")?;
        ctx.db
            .investigation_area_authority()
            .insert(InvestigationAreaAuthority {
                id: area_id.clone(),
                case_id: case_id.to_string(),
                origin_settlement_id: settlement_id.to_string(),
                safe_label: "the area described by local accounts".into(),
                center_longitude_e7: (settlement.coord_x * 10_000_000.0) as i32,
                center_latitude_e7: (settlement.coord_y * 10_000_000.0) as i32,
                radius_m: 5_000,
                coordinates_are_geographic: settlement.source_node_id.is_some(),
                terrain: "settlement".into(),
            });
    }
    let canonical_case = ctx
        .db
        .case_authority()
        .iter()
        .find(|case| case.id == case_id || case.investigation_case_id == case_id);
    let site = canonical_case.as_ref().and_then(|case| {
        ctx.db
            .case_site_authority()
            .case_id()
            .filter(&case.id)
            .next()
    });
    let target_id = site
        .as_ref()
        .map_or_else(|| area_id.clone(), |site| site.id.value.clone());
    let target_kind = if site.is_some() { "site" } else { "area" };
    let terrain = site
        .as_ref()
        .and_then(|site| parse_action_terrain(&site.scene_key).ok())
        .unwrap_or(action::Terrain::Settlement);
    let ids = |method: &str| inv::compound_id(&["investigate", lead_id, method]);
    let locate = ids("locate_contact");
    let watch = ids("watch");
    let approach = ids("approach_lead");
    let patrol = ids("patrol");
    let search = ids("search_area");
    let reacquire = ids("reacquire_tracks");
    let follow = ids("follow_tracks");
    let ambush = ids("lay_ambush");
    let inspect = ids("inspect_site");
    if ctx
        .db
        .investigation_action_capability()
        .id()
        .find(&locate)
        .is_some()
    {
        return validate_action_route_graph(ctx, owner_character_id, case_id);
    }
    let none = InvestigationActionConsequence::None;
    let specs = [
        (
            locate.clone(),
            action::InvestigationActionKind::LocateContact,
            "contact",
            contact_id.to_string(),
            action::Terrain::Settlement,
            "",
            watch.clone(),
            format!("Look for {safe_summary}"),
            "You locate someone who can clarify the report.".to_string(),
            none.clone(),
        ),
        (
            watch.clone(),
            action::InvestigationActionKind::Watch,
            "contact",
            contact_id.to_string(),
            action::Terrain::Settlement,
            "",
            locate.clone(),
            "Watch the public area for a corroborating account.".into(),
            "A local observation reveals another route.".into(),
            none.clone(),
        ),
        (
            approach.clone(),
            action::InvestigationActionKind::ApproachLead,
            "area",
            area_id.clone(),
            terrain,
            locate.as_str(),
            patrol.clone(),
            "Approach the lead described by the witness.".into(),
            "The witness's directions narrow the search.".into(),
            none.clone(),
        ),
        (
            patrol.clone(),
            action::InvestigationActionKind::Patrol,
            "area",
            area_id.clone(),
            terrain,
            watch.as_str(),
            approach.clone(),
            "Patrol the area implicated by the reports.".into(),
            "The patrol reveals a repeatable pattern.".into(),
            none.clone(),
        ),
        (
            search.clone(),
            action::InvestigationActionKind::SearchArea,
            "area",
            area_id.clone(),
            terrain,
            approach.as_str(),
            reacquire.clone(),
            "Search the narrowed area for physical evidence.".into(),
            "The search reveals a trail worth following.".into(),
            none.clone(),
        ),
        (
            reacquire.clone(),
            action::InvestigationActionKind::ReacquireTracks,
            target_kind,
            target_id.clone(),
            terrain,
            patrol.as_str(),
            search.clone(),
            "Reacquire a trail from the observed pattern.".into(),
            "The party picks up the trail again.".into(),
            none.clone(),
        ),
        (
            follow.clone(),
            action::InvestigationActionKind::FollowTracks,
            target_kind,
            target_id.clone(),
            terrain,
            search.as_str(),
            ambush.clone(),
            "Follow the physical trail toward its source.".into(),
            "The trail identifies where the threat is based.".into(),
            none.clone(),
        ),
        (
            ambush.clone(),
            action::InvestigationActionKind::LayAmbush,
            target_kind,
            target_id.clone(),
            terrain,
            reacquire.as_str(),
            follow.clone(),
            "Lay an ambush along the threat's established route.".into(),
            "The ambush is prepared at the threat's likely approach.".into(),
            none.clone(),
        ),
        (
            inspect.clone(),
            action::InvestigationActionKind::InspectSite,
            target_kind,
            target_id,
            terrain,
            follow.as_str(),
            ambush.clone(),
            "Inspect the identified site directly.".into(),
            "The site yields decisive evidence.".into(),
            none,
        ),
    ];
    for (
        id,
        kind,
        kind_name,
        target,
        terrain,
        required,
        alternate,
        summary,
        success,
        consequence,
    ) in specs
    {
        issue_investigation_action_capability(
            ctx,
            id,
            owner_character_id,
            case_id.to_string(),
            kind,
            kind_name.into(),
            target,
            terrain,
            ctx.random::<u64>(),
            if matches!(
                kind,
                action::InvestigationActionKind::FollowTracks
                    | action::InvestigationActionKind::ReacquireTracks
                    | action::InvestigationActionKind::InspectSite
            ) {
                2_500
            } else {
                7_000
            },
            summary,
            "Complete the preceding lead and remain with your ready, co-located party.".into(),
            success,
            consequence,
            required.into(),
            alternate,
        )?;
    }
    set_action_active(ctx, &locate, true)?;
    set_action_active(ctx, &watch, true)?;
    validate_action_route_graph(ctx, owner_character_id, case_id)
}

fn skill_bps(skill: Skill, hours: f32) -> u16 {
    (skill.training_rank(hours) * 2_000.0)
        .round()
        .clamp(0.0, 10_000.0) as u16
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
    let terrain_bps = match terrain {
        action::Terrain::Forest => skill_bps(Skill::TerrainForest, actor.terrain_forest_hours),
        action::Terrain::Hills => skill_bps(Skill::TerrainHills, actor.terrain_hills_hours),
        action::Terrain::Settlement | action::Terrain::Ruins => {
            skill_bps(Skill::TerrainUrban, actor.terrain_urban_hours)
        }
        _ => skill_bps(Skill::TerrainPlains, actor.terrain_plains_hours),
    };
    let mut assistance = 0u16;
    for member_id in living_party_member_ids(ctx, party_id) {
        if member_id == actor_id {
            continue;
        }
        if let Some(skills) = ctx.db.character_skills().character_id().find(member_id) {
            let contribution = match terrain {
                action::Terrain::Forest => {
                    skill_bps(Skill::TerrainForest, skills.terrain_forest_hours)
                }
                action::Terrain::Hills => {
                    skill_bps(Skill::TerrainHills, skills.terrain_hills_hours)
                }
                action::Terrain::Settlement | action::Terrain::Ruins => {
                    skill_bps(Skill::TerrainUrban, skills.terrain_urban_hours)
                }
                _ => skill_bps(Skill::TerrainPlains, skills.terrain_plains_hours),
            } / 4;
            assistance = assistance.saturating_add(contribution).min(2_000);
        }
    }
    Ok(action::SkillContribution {
        terrain_bps,
        awareness_bps: skill_bps(Skill::Insight, actor.insight_hours),
        stealth_bps: skill_bps(Skill::Stealth, actor.stealth_hours),
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

fn persist_action_result_lead(
    ctx: &ReducerContext,
    capability: &InvestigationActionCapability,
    attempt_id: &str,
    resolution: &action::Resolution,
) -> Result<(), String> {
    let public_case_id = ctx
        .db
        .quest_generation_authority()
        .case_id()
        .find(&capability.case_id)
        .and_then(|authority| {
            serde_json::from_str::<adventuresim_core::quest_generation::GeneratedCase>(
                &authority.manifest_json,
            )
            .ok()
        })
        .map_or_else(
            || capability.case_id.clone(),
            |generated| generated.public_case_id,
        );
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
        current_learned_location: String::new(),
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
            if character_case_site_id(ctx, actor.id).as_deref()
                == Some(capability.target_id.as_str())
            {
                return Ok(());
            }
            if matches!(
                kind,
                action::InvestigationActionKind::FollowTracks
                    | action::InvestigationActionKind::ReacquireTracks
            ) {
                let predecessor = ctx
                    .db
                    .investigation_action_capability()
                    .id()
                    .find(&capability.required_action_id)
                    .ok_or("Track predecessor no longer exists")?;
                if predecessor.owner_character_id != capability.owner_character_id
                    || predecessor.case_id != capability.case_id
                    || predecessor.target_kind != "area"
                {
                    return Err("Track origin no longer matches this investigation".into());
                }
                return validate_action_position(
                    ctx,
                    actor,
                    &predecessor,
                    parse_action_kind(&predecessor.method)?,
                );
            }
            Err("The party must occupy the action's authoritative site".into())
        }
        "tracks" | "route" => {
            let predecessor = ctx
                .db
                .investigation_action_capability()
                .id()
                .find(&capability.required_action_id)
                .ok_or("Route predecessor no longer exists")?;
            if predecessor.owner_character_id != capability.owner_character_id
                || predecessor.case_id != capability.case_id
            {
                return Err("Route origin no longer matches this investigation".into());
            }
            validate_action_position(
                ctx,
                actor,
                &predecessor,
                parse_action_kind(&predecessor.method)?,
            )
        }
        _ => Err("Investigation action has no authoritative position binding".into()),
    }
}

fn validate_generated_pattern_condition(
    ctx: &ReducerContext,
    capability: &InvestigationActionCapability,
    kind: action::InvestigationActionKind,
    started_at: u64,
) -> Result<(), String> {
    let Some(outputs) = ctx
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
        .transpose()?
    else {
        return Ok(());
    };
    let Some((evidence_id, condition)) = outputs.iter().find_map(|output| match output {
        adventuresim_core::quest_generation::GeneratedActionOutput::PatternCondition {
            evidence_id,
            condition,
        } => Some((&evidence_id.0, condition)),
        _ => None,
    }) else {
        return Ok(());
    };
    if !ctx
        .db
        .investigation_evidence_knowledge()
        .owner_character_id()
        .filter(capability.owner_character_id)
        .any(|knowledge| {
            knowledge.case_id == capability.case_id
                && knowledge.evidence_id.as_str() == evidence_id.as_str()
        })
    {
        return Err("The selected pattern has not been corroborated yet".into());
    }
    use adventuresim_core::quest_generation::GeneratedPatternCondition as C;
    match condition {
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
            let current_demographic = crate::strategic::generated_npc_demographic(&npc);
            let current_version = crate::strategic::generated_npc_presence_version(&npc, &presence);
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
            let current = adventuresim_core::quest_generation::WitnessCandidate {
                npc_id: npc.id.clone(),
                demographic: current_demographic,
                age_band: format!("{:?}", npc.age_band).to_ascii_lowercase(),
                sex: format!("{:?}", npc.sex).to_ascii_lowercase(),
                profession: npc.profession.clone(),
                visible_description: String::new(),
                expected_location: presence.location_id.clone(),
                expected_location_label: presence.location_id.clone(),
                presence_version: current_version,
                allowed_circumstances: Default::default(),
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
        safe_wording:
            "The situation changed before you acted; the lead was refreshed without spending time."
                .into(),
        recorded_at: character_strategic_minute(ctx, capability.owner_character_id),
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
    let resolution = action::resolve(action::ResolutionInput {
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
        skills: party_action_skills(ctx, &party_id, actor_id, target_terrain)?,
        weather: action::WeatherAuthority::Unavailable,
    });
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
            private_resolution_json: serde_json::to_string(&resolution)
                .map_err(|_| "Investigation resolution could not be recorded")?,
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
            safe_wording: if resolution.success {
                if resolution.risk_triggered {
                    format!(
                        "{} The party was exposed to danger during the attempt.",
                        safe_result_on_success
                    )
                } else {
                    safe_result_on_success
                }
            } else {
                adventuresim_core::quest_generation::failed_action_outcome_wording(
                    alternate_available,
                )
                .into()
            },
            recorded_at: completed_at,
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
            "{}, {}, {}, with {}; {}",
            npc.height, npc.build, npc.complexion, npc.hair, npc.visible_features
        )
    });
    // Never expose the private opaque case seam. This observer-facing stable ID
    // derives only from the already-public problem identifier.
    let canonical_case_id = receipt.opaque_case_ref.clone();
    let generation = ctx
        .db
        .quest_generation_authority()
        .case_id()
        .find(&canonical_case_id)
        .ok_or("Rumor is not linked to a real generated case")?;
    let generated: adventuresim_core::quest_generation::GeneratedCase =
        serde_json::from_str(&generation.manifest_json)
            .map_err(|_| "Generated rumor manifest is invalid")?;
    let referral_location_label = generated
        .witnesses
        .iter()
        .find(|witness| {
            witness.npc_id == receipt.contact_npc_id
                && witness.expected_location == receipt.expected_location_id
        })
        .map(adventuresim_core::quest_generation::referral_display_location)
        .map(str::to_owned)
        .filter(|label| !label.is_empty())
        .ok_or("Generated rumor referral has no player-visible tab label")?;
    let case_id = generated.public_case_id;
    let lead_id = inv::compound_id(&["lead", "rumor", &receipt.id]);
    if ctx.db.investigation_lead().id().find(&lead_id).is_none() {
        ctx.db.investigation_lead().insert(InvestigationLead {
            id: lead_id.clone(),
            owner_character_id: character_id,
            case_id: case_id.clone(),
            proposition_id: String::new(),
            summary: receipt.safe_summary.clone(),
            source_label: "local rumor".into(),
            confidence_bps: 5_000,
            destination_stage: if receipt.expected_location_id.is_empty() {
                "unknown"
            } else {
                "textual"
            }
            .into(),
            directions: referral_location_label.clone(),
            exact_location_id: String::new(),
            latitude_e7: 0,
            longitude_e7: 0,
            witness_name: contact
                .as_ref()
                .map_or_else(String::new, |npc| npc.name.clone()),
            witness_description: visible_description,
            witness_occupation_or_relationship: contact
                .map_or_else(String::new, |npc| npc.profession),
            expected_location: referral_location_label,
            current_learned_location: String::new(),
            contradiction_group: String::new(),
            corrected_by: String::new(),
            recorded_at: receipt.learned_at,
        });
    }
    issue_rumor_action_graph(
        ctx,
        character_id,
        &canonical_case_id,
        &lead_id,
        &receipt.settlement_id,
        &receipt.contact_npc_id,
        &receipt.safe_summary,
    )?;
    record_action(ctx, action_id, character_id, "receive_rumor", payload);
    Ok(())
}

/// Trusted authority seam for #184/generation. `pipeline_json` is private
/// server-authored material and must never originate in or be projected to a
/// browser; only the registered SSR gateway can invoke this temporary seam.
fn process_investigation_pipeline(
    pipeline: inv::PipelineInput,
) -> Result<(inv::Observation, inv::Recollection, Option<inv::Claim>), String> {
    inv::process_report(pipeline)
        .map_err(|error| format!("Invalid investigation pipeline at report processing: {error:?}"))
}

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
    let pipeline: inv::PipelineInput = serde_json::from_str(&pipeline_json)
        .map_err(|_| "Invalid investigation pipeline at payload decoding")?;
    let proposition = pipeline.proposition.clone();
    let (observation, recollection, claim) = process_investigation_pipeline(pipeline)?;
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
    Ok(())
}

pub(crate) fn persist_generated_testimony(
    ctx: &ReducerContext,
    character_id: u64,
    generated: &adventuresim_core::quest_generation::GeneratedCase,
    witness: &adventuresim_core::quest_generation::WitnessBinding,
    dialogue_action_id: &str,
) -> Result<(), String> {
    use adventuresim_core::quest_generation::Reliability;
    let projection_plan =
        adventuresim_core::quest_generation::generated_testimony_projection_plan(witness)
            .map_err(str::to_string)?;
    let authority = ctx
        .db
        .quest_generation_authority()
        .case_id()
        .find(&generated.canonical_case_id)
        .ok_or("Generated testimony case authority is missing")?;
    let generation_context = serde_json::from_str::<
        adventuresim_core::quest_generation::GenerationContext,
    >(&authority.context_snapshot_json)
    .map_err(|_| "Generated testimony observer-id authority is invalid")?;
    for (index, draft) in projection_plan.iter().enumerate() {
        let (receipt_id, pipeline) =
            adventuresim_core::quest_generation::generated_testimony_pipeline(
                &generation_context,
                character_id,
                generated,
                witness,
                index,
                official_minute(ctx),
            )
            .map_err(|error| format!("Invalid generated testimony pipeline: {error:?}"))?;
        if ctx
            .db
            .investigation_safe_claim_receipt()
            .id()
            .find(&receipt_id)
            .is_some()
        {
            continue;
        }
        let correction_belief_id = draft
            .corrects_proposition_id
            .as_ref()
            .and_then(|proposition_id| {
                ctx.db
                    .investigation_belief()
                    .owner_character_id()
                    .filter(character_id)
                    .find(|belief| {
                        belief.case_id == generated.public_case_id
                            && belief.proposition_id == *proposition_id
                    })
                    .map(|belief| belief.id)
            })
            .unwrap_or_default();
        stage_investigation_claim(
            ctx,
            character_id,
            receipt_id.clone(),
            serde_json::to_string(&pipeline)
                .map_err(|_| "Could not encode generated testimony pipeline")?,
            generated.public_case_id.clone(),
            "the referred local witness".into(),
            inv::compound_id(&["conflict", &generated.public_case_id, &draft.proposition_id]),
            correction_belief_id,
        )?;
        receive_investigation_claim(
            ctx,
            character_id,
            inv::compound_id(&["receive-generated-testimony", &receipt_id]),
            receipt_id.clone(),
        )?;

        let site = draft
            .site_id
            .as_ref()
            .and_then(|site_id| ctx.db.case_site_authority().id_key().find(&site_id.0));
        let exact = draft.destination_stage == "exact_believed";
        let lead_id = inv::compound_id(&[
            "lead",
            "generated-testimony",
            &character_id.to_string(),
            &witness.id.0,
            &index.to_string(),
        ]);
        if ctx.db.investigation_lead().id().find(&lead_id).is_none() {
            let npc = ctx
                .db
                .settlement_npc()
                .id()
                .find(&witness.npc_id)
                .ok_or("Generated witness is no longer persistent")?;
            ctx.db.investigation_lead().insert(InvestigationLead {
                id: lead_id.clone(),
                owner_character_id: character_id,
                case_id: generated.public_case_id.clone(),
                proposition_id: draft.proposition_id.clone(),
                summary: draft.spoken_text.clone(),
                source_label: "the referred local witness".into(),
                confidence_bps: if draft.reliability == Reliability::Truthful {
                    8_000
                } else {
                    5_000
                },
                destination_stage: draft.destination_stage.clone(),
                directions: if exact {
                    String::new()
                } else {
                    draft.spoken_text.clone()
                },
                exact_location_id: site
                    .as_ref()
                    .filter(|_| exact)
                    .map_or_else(String::new, |site| site.id.value.clone()),
                latitude_e7: site
                    .as_ref()
                    .filter(|_| exact)
                    .map_or(0, |site| site.latitude_e7),
                longitude_e7: site
                    .as_ref()
                    .filter(|_| exact)
                    .map_or(0, |site| site.longitude_e7),
                witness_name: npc.name,
                witness_description: witness.visible_description.clone(),
                witness_occupation_or_relationship: npc.profession,
                expected_location: adventuresim_core::quest_generation::referral_display_location(
                    witness,
                )
                .to_owned(),
                current_learned_location:
                    adventuresim_core::quest_generation::referral_display_location(witness)
                        .to_owned(),
                contradiction_group: format!("generated-location:{}", generated.public_case_id),
                corrected_by: String::new(),
                recorded_at: official_minute(ctx),
            });
            if let Some(corrected_proposition) = &draft.corrects_proposition_id {
                for mut prior in ctx
                    .db
                    .investigation_lead()
                    .owner_character_id()
                    .filter(character_id)
                    .filter(|prior| {
                        prior.case_id == generated.public_case_id
                            && prior.proposition_id == *corrected_proposition
                            && prior.id != lead_id
                            && prior.corrected_by.is_empty()
                    })
                    .collect::<Vec<_>>()
                {
                    prior.corrected_by = lead_id.clone();
                    ctx.db.investigation_lead().id().update(prior);
                }
            }
        }
    }
    complete_referred_contact_action(
        ctx,
        character_id,
        &generated.canonical_case_id,
        &witness.npc_id,
        dialogue_action_id,
    )
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
        proposition_id: String::new(),
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
    fn both_generated_families_issue_root_and_successor_action_text() {
        use adventuresim_core::{
            local_problem::Scope,
            quest_generation::{
                GeneratedActionOutput, GenerationContext, TemplateFamily, generate,
                observer_scoped_id, test_witnesses,
            },
        };

        for (seed, family) in [
            (7, TemplateFamily::RecurringDepredation),
            (11, TemplateFamily::DisappearanceOrLoss),
        ] {
            let context = GenerationContext {
                seed,
                observer_entropy_hi: seed ^ 0x6f62_7365_7276_6572,
                observer_entropy_lo: seed.rotate_left(23) ^ 0x7175_6573_742d_7631,
                settlement_id: "lubeck".into(),
                settlement_name: "Lubeck".into(),
                scope: Scope::Settlement {
                    settlement_id: "lubeck".into(),
                },
                ordinal: 0,
                now_minute: 50_000,
                requested_family: Some(family),
                witness_candidates: test_witnesses(),
            };
            let manifest = generate(&context).expect("family should generate");
            let mut saw_root = false;
            let mut saw_successor = false;
            for action in &manifest.actions {
                let remap = |id: &adventuresim_core::quest_generation::ActionId| {
                    observer_scoped_id(&context, "capability", &format!("1:{}", id.0))
                };
                let required_action_id =
                    action.prerequisite.as_ref().map_or_else(String::new, remap);
                saw_root |= required_action_id.is_empty();
                saw_successor |= !required_action_id.is_empty();
                let earned_clue = action.outputs.iter().find_map(|output| match output {
                    GeneratedActionOutput::Evidence { evidence_id } => manifest
                        .evidence
                        .iter()
                        .find(|evidence| evidence.id == *evidence_id)
                        .map(|evidence| evidence.safe_description.clone()),
                    _ => None,
                });
                validate_investigation_action_text(
                    &remap(&action.id),
                    &manifest.public_case_id,
                    &action.target_kind,
                    &action.target_id,
                    &action.safe_summary,
                    "Complete the preceding generated lead and remain with your ready, co-located party.",
                    earned_clue.as_deref().unwrap_or(
                        "The investigation produces a new, source-attributed lead.",
                    ),
                    &required_action_id,
                    &remap(&action.alternate),
                )
                .expect("generated issuance text should accept an absent root prerequisite");
            }
            assert!(saw_root, "{family:?} did not generate a root action");
            assert!(
                saw_successor,
                "{family:?} did not generate a successor action"
            );
        }
    }

    #[test]
    fn root_rumor_then_every_referred_witness_pipeline_is_valid_in_both_families() {
        use adventuresim_core::{
            investigation::ValidationError,
            local_problem::Scope,
            quest_generation::{
                GenerationContext, TemplateFamily, generate, test_witnesses, validate,
            },
        };

        for (seed, family) in [
            (7, TemplateFamily::RecurringDepredation),
            (11, TemplateFamily::DisappearanceOrLoss),
        ] {
            let mut context = GenerationContext {
                seed,
                observer_entropy_hi: seed ^ 0x6f62_7365_7276_6572,
                observer_entropy_lo: seed.rotate_left(23) ^ 0x7175_6573_742d_7631,
                settlement_id: "lubeck".into(),
                settlement_name: "Lubeck".into(),
                scope: Scope::Settlement {
                    settlement_id: "lubeck".into(),
                },
                ordinal: 0,
                now_minute: 50_000,
                requested_family: Some(family),
                witness_candidates: test_witnesses(),
            };
            for (index, witness) in context.witness_candidates.iter_mut().enumerate() {
                witness.npc_id = format!("npc:riverdale:residences:{index}");
            }
            let generated = generate(&context).expect("root rumor should materialize a case");
            validate(&generated).expect("generated action graph should remain valid");
            assert_ne!(generated.canonical_case_id, generated.public_case_id);
            assert!(
                generated.witnesses.len() >= 2,
                "the referral transition needs another authored local account"
            );

            let character_id = 17_849_106_825_763_413_937;
            let mut authored_claims = 0;
            for witness in &generated.witnesses {
                for index in 0..witness.testimony.len() {
                    let (receipt_id, pipeline) =
                        adventuresim_core::quest_generation::generated_testimony_pipeline(
                            &context,
                            character_id,
                            &generated,
                            witness,
                            index,
                            50_000,
                        )
                        .expect("referred witness should produce a pipeline");
                    assert!(receipt_id.starts_with("testimony:"));
                    assert!(receipt_id.len() <= 256);
                    let (observation, recollection, claim) =
                        process_investigation_pipeline(pipeline.clone())
                            .expect("every authored witness claim should persist");
                    let claim = claim.expect("generated testimony is never omitted");
                    for id in [
                        observation.id.as_str(),
                        recollection.id.as_str(),
                        claim.id.as_str(),
                    ] {
                        assert!(id.len() <= 256, "pipeline id exceeds stable-id budget");
                        assert!(id.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric()
                                || matches!(byte, b'-' | b'_' | b':' | b'.')
                        }));
                    }
                    authored_claims += 1;

                    if authored_claims == 1 {
                        let mut invalid = pipeline;
                        invalid.receipt_identity = inv::compound_id(&[
                            "generated-testimony",
                            &character_id.to_string(),
                            &witness.id.0,
                            &index.to_string(),
                        ]);
                        assert_eq!(
                            inv::process_report(invalid.clone()).unwrap_err(),
                            ValidationError::InvalidId
                        );
                        assert_eq!(
                            process_investigation_pipeline(invalid).unwrap_err(),
                            "Invalid investigation pipeline at report processing: InvalidId"
                        );
                    }
                }
            }
            assert!(authored_claims >= generated.witnesses.len());
        }
    }

    #[test]
    fn non_exact_rows_are_sanitized_without_coordinates() {
        let row = InvestigationLead {
            id: "lead".into(),
            owner_character_id: 1,
            case_id: "case".into(),
            proposition_id: String::new(),
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
            "investigation_area_authority",
            "investigation_action_capability",
            "investigation_pattern_target_authority",
            "investigation_action_attempt",
            "investigation_action_outcome",
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

    #[test]
    fn action_projection_and_reducer_keep_hidden_authority_server_side() {
        let source = include_str!("investigation.rs");
        let projected_type = source
            .split("pub struct BackendInvestigationAction")
            .nth(1)
            .and_then(|tail| tail.split("#[derive").next())
            .expect("action projection type");
        let projection = source
            .split("pub fn backend_investigation_actions")
            .nth(1)
            .and_then(|tail| {
                tail.split("pub fn backend_investigation_action_outcomes")
                    .next()
            })
            .expect("action projection body");
        for hidden in [
            "case_id",
            "target_id",
            "resolution_seed",
            "success_threshold",
        ] {
            assert!(
                !projected_type.contains(hidden),
                "{hidden} leaked into projection"
            );
        }
        assert!(projected_type.contains("required_case_site_id"));
        assert!(projected_type.contains("available"));
        assert!(projection.contains("capability_has_successful_attempt_view"));
        assert!(projection.contains("capability_has_live_support_view"));
        assert!(projection.contains("action_unavailable_reason_view"));

        let reducer = source
            .split("pub fn perform_investigation_action")
            .nth(1)
            .expect("action reducer body");
        assert!(reducer.contains("expected_version"));
        assert!(reducer.contains("perform_investigation_action_authorized"));
        assert!(!reducer.contains("stage_investigation_lead"));
    }

    #[test]
    fn corrected_exact_site_knowledge_is_not_live_action_support() {
        assert!(exact_site_knowledge_is_live(
            "case",
            "site",
            "case",
            "site",
            "exact_believed",
            "",
            "case",
            "site",
            true,
        ));
        assert!(!exact_site_knowledge_is_live(
            "case",
            "site",
            "case",
            "site",
            "exact_believed",
            "newer-lead",
            "case",
            "site",
            true,
        ));
        let source = include_str!("investigation.rs");
        let projection = source
            .split("fn capability_has_live_support_view")
            .nth(1)
            .and_then(|tail| tail.split("fn exact_action_site_for_observer").next())
            .expect("projection live-support predicate");
        assert!(projection.contains("exact_action_site_for_observer"));
        let recovery = source
            .split("fn activate_action_successors")
            .nth(1)
            .and_then(|tail| tail.split("fn complete_referred_contact_action").next())
            .expect("failed-alternate live-support predicate");
        assert!(recovery.contains("capability_has_live_support_reducer"));
        assert!(recovery.contains("exact_site_knowledge_is_live"));
    }

    #[test]
    fn corrected_contact_referral_is_not_live_at_any_action_boundary() {
        let referral = |owner_character_id, case_id: &str, corrected_by: &str| InvestigationLead {
            id: "lead".into(),
            owner_character_id,
            case_id: case_id.into(),
            proposition_id: String::new(),
            summary: "Ask the cooper what she saw.".into(),
            source_label: "local rumor".into(),
            confidence_bps: 5_000,
            destination_stage: "textual".into(),
            directions: "Public square".into(),
            exact_location_id: String::new(),
            latitude_e7: 0,
            longitude_e7: 0,
            witness_name: "Greta".into(),
            witness_description: "A tall cooper.".into(),
            witness_occupation_or_relationship: "cooper".into(),
            expected_location: "Public square".into(),
            current_learned_location: String::new(),
            contradiction_group: String::new(),
            corrected_by: corrected_by.into(),
            recorded_at: 50_000,
        };
        let live = referral(7, "case", "");
        assert!(lead_is_live_contact_referral(&live, 7, "case"));
        let corrected = referral(7, "case", "replacement-lead");
        assert!(!lead_is_live_contact_referral(&corrected, 7, "case"));
        let mut retracted = referral(7, "case", "");
        retracted.witness_name.clear();
        assert!(!lead_is_live_contact_referral(&retracted, 7, "case"));
        assert!(!lead_is_live_contact_referral(&live, 8, "case"));
        assert!(!lead_is_live_contact_referral(&live, 7, "other-case"));

        let source = include_str!("investigation.rs");
        let projection = source
            .split("fn capability_has_live_support_view")
            .nth(1)
            .and_then(|tail| tail.split("fn exact_action_site_for_observer").next())
            .expect("projection contact support");
        assert!(projection.contains("lead_is_live_contact_referral"));
        let recovery = source
            .split("fn capability_has_live_support_reducer")
            .nth(1)
            .and_then(|tail| tail.split("fn complete_referred_contact_action").next())
            .expect("recovery contact support");
        assert!(recovery.contains("lead_is_live_contact_referral"));
        let execution = source
            .split("fn validate_live_action_prerequisites")
            .nth(1)
            .and_then(|tail| {
                tail.split("fn case_objective_contains_custody_target")
                    .next()
            })
            .expect("execution contact support");
        assert!(execution.contains("lead_is_live_contact_referral"));
        assert!(execution.contains("No live witness referral supports this action"));
    }

    #[test]
    fn inspect_site_travel_requires_ready_off_site_party() {
        let ready_off_site = projected_action_availability(true, "site", false);
        assert!(ready_off_site.unavailable_reason.is_some());
        assert!(ready_off_site.can_travel_to_required_site);

        let incapacitated_off_site = projected_action_availability(false, "site", false);
        assert!(incapacitated_off_site.unavailable_reason.is_some());
        assert!(!incapacitated_off_site.can_travel_to_required_site);

        let incapacitated_on_site = projected_action_availability(false, "site", true);
        assert!(incapacitated_on_site.unavailable_reason.is_some());
        assert!(!incapacitated_on_site.can_travel_to_required_site);

        let ready_on_site = projected_action_availability(true, "site", true);
        assert!(ready_on_site.unavailable_reason.is_none());
        assert!(!ready_on_site.can_travel_to_required_site);
    }

    #[test]
    fn action_graph_covers_all_methods_and_enforces_authoritative_boundaries() {
        let source = include_str!("investigation.rs");
        let graph = source
            .split("fn issue_rumor_action_graph")
            .nth(1)
            .and_then(|tail| tail.split("fn skill_bps").next())
            .expect("action graph");
        for method in [
            "InspectSite",
            "SearchArea",
            "FollowTracks",
            "ReacquireTracks",
            "LocateContact",
            "Watch",
            "Patrol",
            "LayAmbush",
            "ApproachLead",
        ] {
            assert!(graph.contains(method), "missing action method {method}");
        }
        assert!(graph.contains("validate_action_route_graph"));
        assert!(source.contains("require_party_ready(ctx, party_id)?"));
        assert!(source.contains("require_no_unresolved_encounter(ctx, party_id)?"));
        assert!(source.contains("synchronize_party_activity_time"));
        assert!(source.contains("started_at % 1_440 < 360"));
        assert!(source.contains("started_at % 1_440 >= 1_200"));
        assert!(source.contains("validate_pickup_custody"));
        assert!(source.contains("current.holder_kind != CustodyHolderKind::Site"));
        assert!(source.contains("resolution.risk_triggered"));
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source");
        assert!(!production.contains("ResolveHostileGroup"));
        assert!(!production.contains("commit_hostile_battle_resolution"));
        assert!(!production.contains("ensure_bound_mission_authority"));
        assert!(!production.contains("HostileResolutionKind::DrivenOff"));
        assert!(!production.contains("HostileResolutionKind::Captured"));
        let position = production
            .split("fn validate_action_position")
            .nth(1)
            .and_then(|tail| tail.split("fn validate_live_action_prerequisites").next())
            .expect("position authority");
        assert!(position.contains("settlement_npc_presence()"));
        assert!(position.contains("actor.current_settlement_id.as_deref()"));
        assert!(position.contains("presence.settlement_id.as_str()"));
        assert!(position.contains("predecessor.target_kind != \"area\""));
        assert!(position.contains("validate_action_position("));
        assert!(position.contains("coordinate_area_contains_e7("));
        assert!(position.contains("area.coordinates_are_geographic"));
        assert!(position.contains("site.coordinates_are_geographic"));
        assert!(position.contains("site.case_id == area.case_id"));
        assert!(position.contains("The party must occupy the action's authoritative site"));
        let reducer = production
            .split("pub(crate) fn perform_investigation_action_authorized")
            .nth(1)
            .expect("action reducer");
        let position_check = reducer
            .find("validate_live_action_prerequisites")
            .expect("position check");
        let time_advance = reducer
            .find("advance_investigation_time")
            .expect("time advance");
        let lead_write = reducer
            .find("persist_action_result_lead")
            .expect("lead write");
        assert!(position_check < time_advance);
        assert!(position_check < lead_write);
    }

    #[test]
    fn generated_physical_and_social_reveals_execute_from_known_origins() {
        let source = include_str!("investigation.rs");
        let position = source
            .split("fn validate_action_position")
            .nth(1)
            .and_then(|tail| tail.split("fn validate_live_action_prerequisites").next())
            .expect("real position validator");
        assert!(position.contains("\"site\" =>"));
        assert!(position.contains("InvestigationActionKind::FollowTracks"));
        assert!(position.contains("InvestigationActionKind::ReacquireTracks"));
        assert!(position.contains("predecessor.target_kind != \"area\""));
        assert!(position.contains("\"tracks\" | \"route\" =>"));
        assert!(position.contains("validate_action_position("));
        let generated = include_str!("../../adventuresim-core/src/quest_generation.rs");
        let disappearance = generated
            .split("TemplateFamily::DisappearanceOrLoss => vec![")
            .nth(1)
            .and_then(|tail| tail.split("pub fn generate").next())
            .expect("generated disappearance graph");
        assert!(disappearance.contains("\"locate_contact\""));
        assert!(disappearance.contains("GeneratedDestinationStage::ApproximateArea"));
        assert!(disappearance.contains("\"approach_social\""));
        assert!(disappearance.contains("\"route\""));
        assert!(disappearance.contains("GeneratedDestinationStage::Exact"));
        assert!(disappearance.contains("\"resolve_social\""));
        assert!(disappearance.contains("\"site\""));
    }

    #[test]
    fn generated_pattern_actions_require_the_exact_earned_clue() {
        let source = include_str!("investigation.rs");
        let validator = source
            .split("fn validate_generated_pattern_condition")
            .nth(1)
            .and_then(|tail| tail.split("fn validate_live_action_prerequisites").next())
            .expect("pattern-condition validator");
        assert!(validator.contains("GeneratedActionOutput::PatternCondition"));
        assert!(validator.contains("investigation_evidence_knowledge()"));
        assert!(validator.contains("knowledge.evidence_id.as_str() == evidence_id.as_str()"));
        assert!(validator.contains("started_at % 1_440"));
        assert!(validator.contains("capability.target_kind != \"route\""));
        assert!(validator.contains("InvestigationActionKind::SearchArea"));
        assert!(validator.contains("investigation_pattern_target_authority()"));
        assert!(validator.contains("pattern_target_matches"));
        assert!(validator.contains("generated_npc_presence_version"));
        assert!(validator.contains("npc_is_present"));
        assert!(validator.contains("capability.target_id != *cohort_id"));
        assert!(
            !source.contains("#[table(accessor = investigation_pattern_target_authority, public)]")
        );
        let generated_client = include_str!("../../adventuresim-stdb-client/src/mod.rs");
        assert!(!generated_client.contains("investigation_pattern_target_authority_table"));
        let performer = source
            .split("pub(crate) fn perform_investigation_action_authorized")
            .nth(1)
            .and_then(|tail| tail.split("#[reducer]").next())
            .expect("authorized action performer");
        assert_eq!(
            performer
                .matches("validate_generated_pattern_condition")
                .count(),
            2,
            "pattern authority is checked before resolution and at the mutation boundary"
        );
    }

    #[test]
    fn capability_randomness_is_private_persisted_and_attempt_domain_separated() {
        let source = include_str!("investigation.rs");
        let issuer = source
            .split("pub(crate) fn issue_investigation_action_capability")
            .nth(1)
            .and_then(|tail| tail.split("fn character_strategic_minute").next())
            .expect("capability issuer");
        assert!(issuer.contains("seed,"));
        assert!(issuer.contains("InvestigationActionCapability"));
        let generated_issuer = source
            .split("fn issue_rumor_action_graph")
            .nth(1)
            .and_then(|tail| tail.split("let area_id =").next())
            .expect("generated capability issuer");
        assert!(generated_issuer.contains("ctx.random::<u64>()"));
        let performer = source
            .split("pub(crate) fn perform_investigation_action_authorized")
            .nth(1)
            .and_then(|tail| tail.split("#[reducer]").next())
            .expect("authorized performer");
        assert!(performer.contains("&expected_version.to_string()"));
        assert!(performer.contains("if let Some(attempt)"));
        assert!(performer.contains("seed: capability.seed"));
        assert!(performer.contains("attempt_index: expected_version"));
        assert!(!source.contains("stable_action_seed"));
    }

    #[test]
    fn generated_testimony_persists_every_proposition_and_corrections_gate_pins() {
        let source = include_str!("investigation.rs");
        let generated = source
            .split("pub(crate) fn persist_generated_testimony")
            .nth(1)
            .and_then(|tail| {
                tail.split("#[reducer]\npub fn receive_investigation_claim")
                    .next()
            })
            .unwrap();
        assert!(generated.contains("for (index, draft) in witness.testimony.iter().enumerate()"));
        assert!(generated.contains("draft.proposition_id.clone()"));
        assert!(generated.contains("draft.corrects_proposition_id"));
        assert!(generated.contains("belief.proposition_id == *proposition_id"));
        assert!(generated.contains("let exact = draft.destination_stage == \"exact_believed\""));
        assert!(generated.contains(".filter(|_| exact)"));
        assert!(generated.contains("prior.corrected_by = lead_id.clone()"));
        assert!(generated.contains("prior.proposition_id == *corrected_proposition"));
    }

    #[test]
    fn coordinate_area_handles_both_modes_boundaries_and_invalid_geography() {
        // Geographic E7: roughly 500 m, 1,000 m, and 1,112 m at the equator.
        assert!(coordinate_area_contains_e7(
            0, 0, 1_000, true, 45_000, 0, true
        ));
        assert!(coordinate_area_contains_e7(
            0, 0, 1_000, true, 89_932, 0, true
        ));
        assert!(!coordinate_area_contains_e7(
            0, 0, 1_000, true, 100_000, 0, true
        ));
        // Abstract E7: one coordinate unit is one kilometer.
        assert!(coordinate_area_contains_e7(
            0, 0, 1_000, false, 5_000_000, 0, false
        ));
        assert!(coordinate_area_contains_e7(
            0, 0, 1_000, false, 10_000_000, 0, false
        ));
        assert!(!coordinate_area_contains_e7(
            0, 0, 1_000, false, 10_020_000, 0, false
        ));
        assert!(!coordinate_area_contains_e7(
            0, 0, 1_000, true, 45_000, 0, false
        ));
        assert!(!coordinate_area_contains_e7(
            0,
            0,
            1_000,
            true,
            i32::MAX,
            0,
            true
        ));
        assert!(!coordinate_area_contains_e7(
            0,
            0,
            1_000,
            true,
            0,
            i32::MAX,
            true
        ));
        // Valid near-antipodal geography must remain about 20,000 km away,
        // never wrap through NaN-to-integer conversion and appear as zero.
        assert!(!coordinate_area_contains_e7(
            0,
            0,
            5_000,
            true,
            1_799_999_999,
            0,
            true
        ));
    }
}
