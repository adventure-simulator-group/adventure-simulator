//! Public discovery contacts, backoff, intake, and dialogue topic policy.

use super::*;

pub(super) const PUBLIC_DISCOVERY_BACKOFF_MINUTES: u64 = 2 * MINUTES_PER_DAY;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct PublicDiscoveryContactIdentity {
    pub(super) resident_character_id: u64,
    pub(super) conversation_id: String,
    pub(super) location_id: String,
}

pub(super) fn public_discovery_contact_identity(
    candidate: &PublicNpcCandidate,
) -> PublicDiscoveryContactIdentity {
    PublicDiscoveryContactIdentity {
        resident_character_id: candidate.resident_character_id,
        conversation_id: candidate.conversation_id.clone(),
        location_id: candidate.location_id.clone(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PublicDiscoveryFingerprint {
    pub(super) settlement_id: String,
    pub(super) contacts: Vec<PublicDiscoveryContactIdentity>,
    pub(super) active_symptoms: Vec<(String, String, u64, u64)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PublicDiscoveryBackoff {
    pub(super) fingerprint: PublicDiscoveryFingerprint,
    pub(super) last_contact: PublicDiscoveryContactIdentity,
    pub(super) retry_at: u64,
}

pub(super) fn public_discovery_backoff_active(
    backoff: &PublicDiscoveryBackoff,
    fingerprint: &PublicDiscoveryFingerprint,
    official_minute: u64,
) -> bool {
    backoff.fingerprint == *fingerprint && official_minute < backoff.retry_at
}

pub(super) fn public_discovery_previous_contact<'a>(
    backoff: Option<&'a PublicDiscoveryBackoff>,
    fingerprint: &PublicDiscoveryFingerprint,
) -> Option<&'a PublicDiscoveryContactIdentity> {
    backoff
        .filter(|backoff| backoff.fingerprint == *fingerprint)
        .map(|backoff| &backoff.last_contact)
}

pub(super) fn public_symptom_age_bucket(oldest_age_minutes: Option<u64>) -> &'static str {
    match oldest_age_minutes {
        None => "none",
        Some(age) if age < MINUTES_PER_DAY => "under_1_day",
        Some(age) if age < 4_320 => "1_to_2_days",
        Some(age) if age < 11_520 => "3_to_7_days",
        Some(_) => "8_plus_days",
    }
}

pub(super) fn public_count_bucket(count: usize) -> &'static str {
    match count {
        0 => "0",
        1 => "1",
        2..=3 => "2_to_3",
        _ => "4_plus",
    }
}

pub(super) fn discovery_location_class(candidate: Option<&PublicNpcCandidate>) -> &'static str {
    match candidate.map(|candidate| candidate.location_id.as_str()) {
        Some("inn") => "inn",
        Some("overview") => "overview",
        Some(_) => "other",
        None => "none",
    }
}

pub(super) fn stable_discovery_action_candidate(
    candidates: Vec<PublicNpcCandidate>,
    previous_contact: Option<&PublicDiscoveryContactIdentity>,
) -> Option<PublicNpcCandidate> {
    let mut candidates = stable_public_npc_candidates(candidates, None, Some("inn"));
    if candidates
        .iter()
        .any(|candidate| candidate.location_id == "inn")
    {
        candidates.retain(|candidate| candidate.location_id == "inn");
    } else {
        candidates.retain(|candidate| candidate.location_id == "overview");
    }
    let next_index = previous_contact
        .and_then(|previous| {
            candidates
                .iter()
                .position(|candidate| public_discovery_contact_identity(candidate) == *previous)
        })
        .map_or(0, |index| (index + 1) % candidates.len());
    candidates.into_iter().nth(next_index)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PublicDiscoveryReferral {
    pub(super) owner_character_id: u64,
    pub(super) case_id: String,
    pub(super) lead_id: String,
    pub(super) summary: String,
    pub(super) witness_name: String,
    pub(super) expected_location: String,
    pub(super) current_learned_location: String,
    pub(super) corrected_by: String,
    pub(super) recorded_at: u64,
}

impl From<BackendInvestigationLead> for PublicDiscoveryReferral {
    fn from(lead: BackendInvestigationLead) -> Self {
        Self {
            owner_character_id: lead.owner_character_id,
            case_id: lead.case_id,
            lead_id: lead.lead_id,
            summary: lead.summary,
            witness_name: lead.witness_name,
            expected_location: lead.expected_location,
            current_learned_location: lead.current_learned_location,
            corrected_by: lead.corrected_by,
            recorded_at: lead.recorded_at,
        }
    }
}

pub(super) fn public_discovery_referral_to_follow(
    owner_character_id: u64,
    before: &HashMap<String, PublicDiscoveryReferral>,
    open_cases: &HashSet<String>,
    after: impl IntoIterator<Item = PublicDiscoveryReferral>,
) -> Option<PublicDiscoveryReferral> {
    let mut newest_changed: Option<PublicDiscoveryReferral> = None;
    let mut newest_unresolved: Option<PublicDiscoveryReferral> = None;
    for lead in after.into_iter().filter(|lead| {
        lead.owner_character_id == owner_character_id
            && !lead.case_id.is_empty()
            && !lead.witness_name.is_empty()
            && lead.corrected_by.is_empty()
    }) {
        let later_than_changed = newest_changed.as_ref().is_none_or(|current| {
            (lead.recorded_at, &lead.lead_id) > (current.recorded_at, &current.lead_id)
        });
        let later_than_unresolved = newest_unresolved.as_ref().is_none_or(|current| {
            (lead.recorded_at, &lead.lead_id) > (current.recorded_at, &current.lead_id)
        });
        if before.get(&lead.lead_id) != Some(&lead) && later_than_changed {
            newest_changed = Some(lead.clone());
        }
        if !open_cases.contains(&lead.case_id) && later_than_unresolved {
            newest_unresolved = Some(lead);
        }
    }
    newest_changed.or(newest_unresolved)
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PublicDialogueProgressFingerprint {
    pub(super) cases: Vec<(String, String)>,
    pub(super) leads: Vec<PublicDialogueLeadSemantic>,
    pub(super) actions: Vec<PublicDialogueActionSemantic>,
    pub(super) outcomes: Vec<(String, String)>,
    pub(super) sites: Vec<(String, CoreDestinationKnowledgeStage, bool, bool, bool)>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct PublicDialogueLeadSemantic {
    pub(super) summary: String,
    pub(super) source_label: String,
    pub(super) confidence_bps: u16,
    pub(super) destination_stage: CoreDestinationKnowledgeStage,
    pub(super) directions: String,
    pub(super) exact_location_id: String,
    pub(super) latitude_e7: i32,
    pub(super) longitude_e7: i32,
    pub(super) witness_name: String,
    pub(super) witness_description: String,
    pub(super) witness_occupation_or_relationship: String,
    pub(super) expected_location: String,
    pub(super) current_learned_location: String,
    pub(super) contradiction_group: String,
    pub(super) corrected_by: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PublicDialogueActionSemantic {
    pub(super) action_id: String,
    pub(super) method: String,
    pub(super) summary: String,
    pub(super) known_prerequisites: String,
    pub(super) duration_min_minutes: u32,
    pub(super) duration_max_minutes: u32,
    pub(super) uncertainty_bps: u16,
    pub(super) skill_contributions: String,
    pub(super) weather_available: bool,
    pub(super) required_case_site_id: Option<CaseSiteId>,
    pub(super) availability: InvestigationActionAvailability,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct PublicDialogueAttemptKey {
    pub(super) owner_character_id: u64,
    pub(super) case_id: String,
    pub(super) topic_id: String,
    pub(super) contact: PublicDiscoveryContactIdentity,
}

pub(super) fn public_dialogue_topic_attempt_allowed(
    last_no_progress: Option<&PublicDialogueProgressFingerprint>,
    current: &PublicDialogueProgressFingerprint,
) -> bool {
    last_no_progress != Some(current)
}

pub(super) fn public_dialogue_topic_made_progress(
    before: &PublicDialogueProgressFingerprint,
    after: &PublicDialogueProgressFingerprint,
) -> bool {
    before != after
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GeneratedDiscoveryOutcome {
    Discovered,
    NoVisibleContacts,
    NoPublicRumor,
    PublicBackoff,
}

impl GeneratedDiscoveryOutcome {
    pub(super) fn case_discovered(self) -> bool {
        self == Self::Discovered
    }
}

pub(super) fn npc_is_publicly_present(
    start_minute: u16,
    end_minute: u16,
    context_suppressed: bool,
    health_suppressed: bool,
    minute: u64,
) -> bool {
    if context_suppressed || health_suppressed {
        return false;
    }
    let minute = minute % MINUTES_PER_DAY;
    let start = u64::from(start_minute);
    let end = u64::from(end_minute);
    start != end
        && if start < end {
            start <= minute && minute < end
        } else {
            minute >= start || minute < end
        }
}

pub(super) fn stable_public_npc_candidates(
    mut candidates: Vec<PublicNpcCandidate>,
    preferred_name: Option<&str>,
    preferred_location: Option<&str>,
) -> Vec<PublicNpcCandidate> {
    candidates.sort_by_key(|candidate| {
        (
            !preferred_name.is_some_and(|name| candidate.name.eq_ignore_ascii_case(name)),
            !preferred_location.is_some_and(|location| candidate.location_id == location),
            candidate.location_id != "inn",
            candidate.name.to_ascii_lowercase(),
            candidate.profession.to_ascii_lowercase(),
            candidate.resident_character_id,
        )
    });
    candidates
}

pub(super) fn stable_owned_open_cases(
    owner_character_id: u64,
    rows: impl IntoIterator<Item = (u64, String, String, DomainCaseStatus, u64)>,
) -> Vec<(String, String)> {
    let mut cases = rows
        .into_iter()
        .filter(|(owner, _, _, status, _)| {
            *owner == owner_character_id && *status == DomainCaseStatus::Open
        })
        .map(|(_, case_id, title, _, latest_update_at)| (latest_update_at, case_id, title))
        .collect::<Vec<_>>();
    cases.sort();
    cases
        .into_iter()
        .map(|(_, case_id, title)| (case_id, title))
        .collect()
}

pub(super) fn fair_open_case_index(
    cases: &[(String, String)],
    active_case_id: Option<&str>,
    active_is_actionable: bool,
    cursor_case_id: Option<&str>,
) -> usize {
    if active_is_actionable
        && let Some(index) = active_case_id
            .and_then(|active| cases.iter().position(|(case_id, _)| case_id == active))
    {
        return index;
    }
    cursor_case_id
        .and_then(|cursor| cases.iter().position(|(case_id, _)| case_id == cursor))
        .map_or(0, |index| (index + 1) % cases.len())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GeneratedClosureAttribution {
    StillOpen,
    OwnImmediateTransition,
    ExternalTransition,
}

pub(super) fn generated_closure_attribution(
    before_status: DomainCaseStatus,
    after_status: Option<DomainCaseStatus>,
    immediately_after_own_action: bool,
) -> GeneratedClosureAttribution {
    if before_status == DomainCaseStatus::Open && after_status == Some(DomainCaseStatus::Resolved) {
        if immediately_after_own_action {
            GeneratedClosureAttribution::OwnImmediateTransition
        } else {
            GeneratedClosureAttribution::ExternalTransition
        }
    } else {
        GeneratedClosureAttribution::StillOpen
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GeneratedCaseIntakeSource {
    OwnerProjectionContinuation,
    DialogueRumor,
}

impl GeneratedCaseIntakeSource {
    pub(super) const fn stable_id(self) -> &'static str {
        match self {
            Self::OwnerProjectionContinuation => "owner_projection_continuation",
            Self::DialogueRumor => "dialogue_rumor",
        }
    }

    pub(super) const fn is_continuation(self) -> bool {
        matches!(self, Self::OwnerProjectionContinuation)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GeneratedDialoguePurpose {
    Discovery,
    Case,
}

impl GeneratedDialoguePurpose {
    pub(super) const fn stable_id(self) -> &'static str {
        match self {
            Self::Discovery => "discover",
            Self::Case => "case",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GeneratedDialogueTopic {
    ReferredTestimony,
    ReturnRecoveredProperty,
    ExposeFalseAccount,
}

impl GeneratedDialogueTopic {
    pub(super) const fn stable_id(self) -> &'static str {
        match self {
            Self::ReferredTestimony => "referred-testimony",
            Self::ReturnRecoveredProperty => "return-recovered-property",
            Self::ExposeFalseAccount => "expose-false-account",
        }
    }

    pub(super) fn from_stable_id(value: &str) -> Option<Self> {
        match value {
            "referred-testimony" => Some(Self::ReferredTestimony),
            "return-recovered-property" => Some(Self::ReturnRecoveredProperty),
            "expose-false-account" => Some(Self::ExposeFalseAccount),
            _ => None,
        }
    }
}
