/// Private world-state and objective authority. Presentation and rewards live
/// on a separate contract, and investigation truth remains in
/// `investigation_case_authority`.
#[derive(Clone, Debug)]
#[table(accessor = case_authority)]
pub struct CaseAuthority {
    #[primary_key]
    pub id: String,
    #[unique]
    pub investigation_case_id: String,
    /// Immutable private origin used by dialogue and objective authority.
    pub provenance_kind: String,
    pub generated_case_id: String,
    pub local_problem_id: Option<String>,
    pub objective_expression_json: String,
    pub resolution_status: CaseResolutionStatus,
    pub resolved_by_party_id: Option<String>,
}

/// Immutable private replay authority for one generated case. The manifest and
/// factor trace include canonical truth and must never be exposed by a public
/// table or view.
#[derive(Clone, Debug)]
#[table(accessor = quest_generation_authority)]
pub struct QuestGenerationAuthority {
    #[primary_key]
    pub case_id: String,
    #[index(btree)]
    pub public_case_id: String,
    #[index(btree)]
    pub settlement_id: String,
    pub settlement_name: String,
    #[index(btree)]
    pub seed: u64,
    pub catalog_revision: String,
    pub context_snapshot_json: String,
    pub context_commitment: String,
    pub manifest_json: String,
    pub factor_trace_json: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ValidatedQuestGenerationAuthority {
    pub context: adventuresim_core::quest_generation::GenerationContext,
    pub manifest: adventuresim_core::quest_generation::GeneratedCase,
}

pub(crate) fn quest_generation_context_commitment(context_json: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"adventuresim.quest-generation-context.v1\0");
    hasher.update(context_json.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) fn validate_quest_generation_authority(
    authority: &QuestGenerationAuthority,
) -> Result<ValidatedQuestGenerationAuthority, String> {
    use adventuresim_core::quest_generation as qg;
    if authority.context_commitment
        != quest_generation_context_commitment(&authority.context_snapshot_json)
    {
        return Err("Quest generation context commitment mismatch".into());
    }
    let developer_context = serde_json::from_str::<
        adventuresim_core::developer_quest::DeveloperGenerationContext,
    >(&authority.context_snapshot_json)
    .ok();
    let context: qg::GenerationContext = if let Some(developer) = &developer_context {
        developer.base.clone()
    } else {
        serde_json::from_str(&authority.context_snapshot_json)
            .map_err(|_| "Quest generation context is invalid")?
    };
    let manifest: qg::GeneratedCase = serde_json::from_str(&authority.manifest_json)
        .map_err(|_| "Quest generation manifest is invalid")?;
    let trace: Vec<qg::FactorTrace> = serde_json::from_str(&authority.factor_trace_json)
        .map_err(|_| "Quest generation factor trace is invalid")?;
    let scope_matches = matches!(
        &context.scope,
        adventuresim_core::local_problem::Scope::Settlement { settlement_id }
            if settlement_id == &context.settlement_id
    );
    if authority.case_id != manifest.canonical_case_id
        || authority.public_case_id != manifest.public_case_id
        || authority.seed != context.seed
        || authority.seed != manifest.generation_seed
        || authority.catalog_revision != qg::CATALOG_REVISION
        || authority.catalog_revision != manifest.catalog_revision
        || manifest.catalog_revision != qg::CATALOG_REVISION
        || trace != manifest.factor_trace
        || authority.settlement_id != context.settlement_id
        || authority.settlement_name != context.settlement_name
        || context.settlement_id.is_empty()
        || context.settlement_name.is_empty()
        || !scope_matches
    {
        return Err("Quest generation authority metadata is inconsistent".into());
    }
    qg::validate(&manifest).map_err(|errors| {
        format!(
            "Quest generation manifest failed validation: {}",
            errors.join("; ")
        )
    })?;
    let regenerated = if let Some(developer) = &developer_context {
        adventuresim_core::developer_quest::compile(developer).map_err(|diagnostics| {
            format!(
                "Developer quest replay failed: {}",
                serde_json::to_string(&diagnostics)
                    .unwrap_or_else(|_| "invalid diagnostics".into())
            )
        })?
    } else {
        qg::generate(&context)
            .map_err(|error| format!("Quest generation replay failed: {error:?}"))?
    };
    if regenerated != manifest {
        return Err("Quest generation replay does not match stored manifest".into());
    }
    Ok(ValidatedQuestGenerationAuthority { context, manifest })
}

/// A separately accepted agreement concerning a case. This row is private:
/// the web gateway builds observer-safe disclosures rather than subscribing
/// clients to undiscovered postings or acceptance state.
#[derive(Clone, Debug)]
#[table(accessor = contract_authority)]
pub struct Contract {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub case_id: String,
    pub title: String,
    pub description: String,
    pub difficulty: i32,
    pub gold_reward: i32,
    pub xp_reward: i32,
    #[index(btree)]
    pub settlement_id: String,
    #[index(btree)]
    pub service_id: String,
    pub issuer_resident_character_id: u64,
    pub status: ContractStatus,
    pub accepted_by: Option<String>,
    pub opposition_wording: String,
    pub opposition_count_wording: String,
    pub accepted_at_minute: Option<u64>,
    pub paid_at_minute: Option<u64>,
}

/// Trusted-gateway projection. This is not a direct player subscription; web
/// handlers still select only locally surfaced or party-accepted contracts.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendContract {
    pub id: String,
    pub case_id: String,
    pub title: String,
    pub description: String,
    pub difficulty: i32,
    pub gold_reward: i32,
    pub xp_reward: i32,
    pub settlement_id: String,
    pub service_id: String,
    pub issuer_resident_character_id: u64,
    pub status: ContractStatus,
    pub accepted_by: Option<String>,
    pub opposition_wording: String,
    pub opposition_count_wording: String,
    /// Observer-safe aggregate built from the exact enemy Combatants that
    /// autoresolve will construct: authored profile, base difficulty, incident
    /// scale, equipment, training, and current enemy count.
    pub opposition_combat_power: u64,
    pub accepted_at_minute: Option<u64>,
    pub paid_at_minute: Option<u64>,
    /// Conservative public one-way preflight distance: the greatest distance
    /// among this contract's possible case destinations. Site identity stays
    /// private until ordinary exact disclosure.
    pub distance_m: u64,
}

#[view(accessor = backend_contracts, public)]
pub fn backend_contracts(ctx: &ViewContext) -> Vec<BackendContract> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .contract_authority()
        .gateway_bucket()
        .filter(0u8)
        .filter_map(|row| {
            let sites = ctx
                .db
                .case_site_authority()
                .case_id()
                .filter(&row.case_id)
                .collect::<Vec<_>>();
            let distance_m = sites.iter().map(|site| site.distance_m).max()?;
            let opposition_combat_power = sites
                .iter()
                .filter_map(|site| {
                    ctx.db
                        .hostile_group_authority()
                        .case_site_id_key()
                        .find(&site.id.value)
                        .filter(|group| {
                            group.case_site_id_key == group.case_site_id.value
                                && group.case_site_id == site.id
                        })
                })
                .map(|group| {
                    autoresolve_enemy(
                        u64::MAX,
                        &group.enemy_type,
                        group.base_difficulty,
                        group.combat_scale_bps,
                    )
                    .ok()
                    .and_then(|enemy| {
                        adventuresim_core::autoresolve::autoresolve_combat_power(&enemy)
                            .checked_mul(u64::from(group.enemy_count))
                    })
                })
                .try_fold(0u64, |total, power| total.checked_add(power?))?;
            Some(BackendContract {
                id: row.id,
                case_id: row.case_id,
                title: row.title,
                description: row.description,
                difficulty: row.difficulty,
                gold_reward: row.gold_reward,
                xp_reward: row.xp_reward,
                settlement_id: row.settlement_id,
                service_id: row.service_id,
                issuer_resident_character_id: row.issuer_resident_character_id,
                status: row.status,
                accepted_by: row.accepted_by,
                opposition_wording: row.opposition_wording,
                opposition_count_wording: row.opposition_count_wording,
                opposition_combat_power,
                accepted_at_minute: row.accepted_at_minute,
                paid_at_minute: row.paid_at_minute,
                distance_m,
            })
        })
        .collect()
}

#[derive(Clone, Debug)]
#[table(accessor = case_outcome)]
pub struct CaseOutcome {
    #[primary_key]
    pub case_id: String,
    pub party_id: String,
    pub status: CaseResolutionStatus,
    pub winning_path_index: Option<u16>,
    pub resolved_at_minute: u64,
    pub selected_finale_id: String,
    pub finale_executed: bool,
}

#[derive(Clone, Debug)]
#[table(accessor = case_outcome_fact)]
pub struct CaseOutcomeFact {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub case_id: String,
    #[index(btree)]
    pub party_id: String,
    #[unique]
    pub source_id: String,
    pub fact_json: String,
    pub happened_at_minute: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = case_custody)]
pub struct CaseCustody {
    #[primary_key]
    pub object_id: String,
    #[index(btree)]
    pub case_id: String,
    pub object_kind: CustodyObjectKind,
    pub holder_kind: CustodyHolderKind,
    pub holder_id: String,
    pub version: u32,
    #[unique]
    pub source_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ObjectiveContinuityKind {
    SurviveAtSite,
    ProtectSubject,
}

/// Private continuous-history guard. A deadline is satisfiable only when this
/// row has remained unbroken from `started_at_minute` through the deadline.
#[derive(Clone, Debug)]
#[table(accessor = objective_continuity_guard)]
pub struct ObjectiveContinuityGuard {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub party_id: String,
    pub case_id: String,
    pub objective_id: String,
    pub kind: ObjectiveContinuityKind,
    pub site_id: String,
    pub subject_id: String,
    pub custody_version: Option<u32>,
    pub started_at_minute: u64,
    pub through_minute: u64,
    pub broken_at_minute: Option<u64>,
    pub completed: bool,
}

#[derive(Clone, Debug)]
#[table(accessor = case_finale_authority)]
pub struct CaseFinaleAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub case_id: String,
    pub kind: FinaleKind,
    pub resolution_status: CaseResolutionStatus,
    pub eligible_path_index: Option<u16>,
    pub priority: u16,
    pub status: FinaleStatus,
}

#[derive(Clone, Debug)]
#[table(accessor = case_finale_execution)]
pub struct CaseFinaleExecution {
    #[primary_key]
    pub finale_id: String,
    #[unique]
    pub source_id: String,
    pub case_id: String,
    pub party_id: String,
    pub executed_at_minute: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ContractInteractionStage {
    Accept,
    Report,
}

#[derive(Clone, Debug)]
#[table(accessor = contract_issuer_interaction_receipt)]
pub struct ContractIssuerInteractionReceipt {
    #[primary_key]
    pub id: String,
    pub contract_id: String,
    pub party_id: String,
    pub stage: ContractInteractionStage,
    pub issuer_resident_character_id: u64,
    pub interacting_character_id: u64,
    pub interacted_at_minute: u64,
    pub dialogue_session_id: String,
    pub dialogue_action_id: String,
    pub dialogue_revision: u64,
    pub location_id: String,
    pub consumed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
pub struct IncidentId {
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
pub struct IncidentSourceId {
    pub value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum IncidentKind {
    Religious,
    RaidingRetaliation,
    ThieveryDiscovery,
    CarousingDisorder,
    AuthorityArrest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum IncidentStatus {
    Pending,
    Resolved,
    Avoided,
}

/// Private strategic authority for an interruption. The source is the durable
/// dedupe key; its site and hostile group bind directly to mission authority.
#[derive(Clone, Debug)]
#[table(accessor = strategic_incident)]
pub struct StrategicIncident {
    #[primary_key]
    pub id_key: String,
    pub id: IncidentId,
    #[unique]
    pub source_id: IncidentSourceId,
    #[index(btree)]
    pub party_id: String,
    pub settlement_id: String,
    pub instigator_id: u64,
    pub kind: IncidentKind,
    pub status: IncidentStatus,
    #[unique]
    pub case_site_id: CaseSiteId,
    #[unique]
    pub hostile_group_id: String,
    pub created_at_minute: u64,
}

/// Private server entropy persisted for one activity interval. The public
/// source identity remains deterministic while rolls cannot be predicted from
/// player-controlled inputs.
#[derive(Clone, Debug)]
#[table(accessor = activity_incident_entropy)]
pub struct ActivityIncidentEntropy {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    pub seed: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
pub struct RecruitmentOfferId {
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
pub struct RecruitmentSourceId {
    pub value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum RecruitmentOfferStatus {
    Open,
    Closed,
    Expired,
}

/// Public, social-only projection for a persistent NPC company's recruiting
/// lifecycle. It intentionally contains no investigation or quest identity.
#[derive(Clone, Debug)]
#[table(accessor = recruitment_offer, public)]
pub struct RecruitmentOffer {
    #[primary_key]
    pub id_key: String,
    pub id: RecruitmentOfferId,
    #[unique]
    pub source_id: RecruitmentSourceId,
    #[unique]
    pub recruiting_party_id: String,
    #[index(btree)]
    pub settlement_id: String,
    #[unique]
    pub settlement_resident_id: u64,
    pub location_id: String,
    pub leader_id: u64,
    pub status: RecruitmentOfferStatus,
    pub created_at_minute: u64,
    pub expires_at_minute: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = party_authority)]
pub struct Party {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    pub name: String,
    pub leader_id: u64,
    pub current_settlement_id: Option<String>,
    pub current_case_site_id: Option<CaseSiteId>,
    pub active_contract_id: Option<String>,
    pub is_solo: bool,
    /// The fatigue level at which the first tiring party member makes camp.
    #[default(50u8)]
    pub camp_fatigue_percent: u8,
    /// Leader-selected daily walking budget. The itinerary centers it on noon.
    #[default(480u16)]
    pub walking_minutes_per_day: u16,
    /// False travels in the daylight window centered on noon; true travels in
    /// the night window centered on midnight.
    #[default(false)]
    pub travel_at_night: bool,
    /// Automatic camps clear every living member's carried fatigue. A fixed
    /// duration preserves the leader's deliberate shorter or longer override.
    #[default(CampDurationMode::Auto)]
    pub camp_duration_mode: CampDurationMode,
    #[default(0u16)]
    pub fixed_camp_minutes: u16,
    /// A non-empty destination means the party is currently camped en route.
    #[default(None::<JourneyEndpoint>)]
    pub camp_destination: Option<JourneyEndpoint>,
    #[default(0u64)]
    pub camp_remaining_minutes: u64,
    /// Water currently held in shared party-inventory waterskins.
    #[default(0.0)]
    pub pooled_water_ml: f32,
    #[default(0.0)]
    pub physiology_target: f32,
    #[default(0.0)]
    pub command_target: f32,
    #[default(0.0)]
    pub religion_target: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, SpacetimeType)]
pub enum CampDurationMode {
    #[default]
    Auto,
    Fixed,
}

/// Party movement and case-site occupancy are visible only through the trusted
/// gateway; direct subscribers cannot enumerate secret destinations.
#[view(accessor = party, public)]
pub fn party(ctx: &ViewContext) -> Vec<Party> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .party_authority()
        .gateway_bucket()
        .filter(0u8)
        .collect()
}

#[derive(Clone, Debug, Default, PartialEq, SpacetimeType)]
pub struct JourneyCampInterval {
    pub movement_minute: u64,
    pub elapsed_start_minute: u64,
    pub elapsed_minutes: u64,
    pub average_fatigue_start: f32,
    pub average_fatigue_end: f32,
    pub maximum_fatigue_end: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
pub struct JourneySettlementEndpoint {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
pub struct JourneyCaseSiteEndpoint {
    pub id: CaseSiteId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
pub enum JourneyEndpoint {
    Settlement(JourneySettlementEndpoint),
    CaseSite(JourneyCaseSiteEndpoint),
    Camp(String),
}

impl JourneyEndpoint {
    fn settlement_id(&self) -> Option<&str> {
        match self {
            Self::Settlement(endpoint) => Some(&endpoint.id),
            _ => None,
        }
    }

    fn case_site_id(&self) -> Option<&str> {
        match self {
            Self::CaseSite(endpoint) => Some(&endpoint.id.value),
            _ => None,
        }
    }
}

/// The durable strategic record behind the travel tracker. Party location
/// answers where the party is right now; this record retains the journey's
/// original endpoints, completed camp stops, and authoritative forecast.
#[derive(Clone, Debug)]
#[table(accessor = party_journey_authority)]
pub struct PartyJourney {
    #[primary_key]
    pub party_id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    pub origin: JourneyEndpoint,
    pub destination: JourneyEndpoint,
    pub total_minutes: u64,
    pub completed_minutes: u64,
    /// Cumulative journey minutes for camps the party has actually reached.
    pub camp_stop_minutes: Vec<u64>,
    /// Cumulative future camp estimates, recalculated after each camp rest.
    pub forecast_camp_stop_minutes: Vec<u64>,
    /// A journey keeps the leader's chosen threshold from departure.
    pub fatigue_percent: u8,
    /// Zero identifies a pre elapsed-itinerary row requiring conservative
    /// reconstruction from the party's current absolute time.
    #[default(0u8)]
    pub plan_version: u8,
    /// Additive v2 itinerary coordinates. Legacy minute fields above remain
    /// route-movement coordinates for compatibility.
    #[default(0u64)]
    pub departure_minute: u64,
    #[default(0u64)]
    pub total_elapsed_minutes: u64,
    #[default(0u64)]
    pub completed_elapsed_minutes: u64,
    #[default(480u16)]
    pub walking_minutes_per_day: u16,
    #[default(false)]
    pub travel_at_night: bool,
    #[default(CampDurationMode::Auto)]
    pub camp_duration_mode: CampDurationMode,
    #[default(0u16)]
    pub fixed_camp_minutes: u16,
}

/// Private encounter authority. Public journey and encounter projections never
/// reveal future-roll entropy to clients.
#[derive(Clone, Debug)]
#[table(accessor = party_journey_encounter_authority)]
pub struct PartyJourneyEncounterAuthority {
    #[primary_key]
    pub party_id: String,
    pub seed: u64,
    pub next_roll: u64,
}

#[derive(Clone, Debug, PartialEq, SpacetimeType)]
pub struct StrategicEncounterLoss {
    pub owner_kind: String,
    pub owner_id: u64,
    pub inventory_id: u64,
    pub item_id: String,
    pub quantity: u32,
    pub value_each: u32,
}

/// Durable strategic interruption only. Tactical exchanges, positions, HP,
/// and enemies remain transient and are committed only through final outcomes.
#[derive(Clone, Debug)]
#[table(accessor = strategic_encounter, public)]
pub struct StrategicEncounter {
    #[primary_key]
    pub party_id: String,
    pub encounter_id: String,
    pub archetype: String,
    pub enemy_count: u16,
    pub roll_index: u64,
    pub journey_movement_minute: u64,
    pub journey_elapsed_minute: u64,
    pub absolute_minute: u64,
    pub longitude_e7: i32,
    pub latitude_e7: i32,
    pub terrain: String,
    pub party_aware: bool,
    pub enemy_aware: bool,
    pub available_choices: Vec<String>,
    pub status: String,
    pub selected_choice: Option<String>,
    pub selection_explanation: String,
    pub party_speed_m_per_minute: u32,
    pub enemy_speed_m_per_minute: u32,
    pub run_ineligibility: Option<String>,
    pub penalty_minutes: u64,
    pub loss_preview: Vec<StrategicEncounterLoss>,
    pub outcome: Option<String>,
}

/// Typed elapsed-time camp coordinates for the journey tracker. Keeping these
/// in an additive table avoids changing the movement-coordinate legacy rows.
#[derive(Clone, Debug)]
#[table(accessor = party_journey_itinerary, public)]
pub struct PartyJourneyItinerary {
    #[primary_key]
    pub party_id: String,
    pub actual_camp_intervals: Vec<JourneyCampInterval>,
    pub forecast_camp_intervals: Vec<JourneyCampInterval>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum JourneyTerrainKind {
    Road,
    Open,
    SparseWoods,
    DeepWoods,
    Wetland,
}

#[derive(Clone, Debug, PartialEq, SpacetimeType)]
pub struct JourneyRoutePoint {
    pub latitude_e7: i32,
    pub longitude_e7: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub struct JourneyTerrainWeights {
    pub plains: u16,
    pub forest: u16,
    pub hills: u16,
    pub wetlands: u16,
    pub urban: u16,
}

#[derive(Clone, Debug, PartialEq, SpacetimeType)]
pub struct JourneyTerrainSpan {
    pub kind: JourneyTerrainKind,
    pub terrain: JourneyTerrainWeights,
    pub training_multiplier_permille: u16,
    pub check_millirank: u16,
    pub start_minute: u64,
    pub duration_minutes: u64,
}

#[derive(Clone, Debug, PartialEq, SpacetimeType)]
pub struct JourneyRouteLeg {
    pub distance_m: u64,
    pub minutes: u64,
    pub points: Vec<JourneyRoutePoint>,
    pub spans: Vec<JourneyTerrainSpan>,
}

#[derive(Clone, Debug, PartialEq, SpacetimeType)]
pub struct JourneyRoutePlan {
    pub package_digest: String,
    pub weather_rules_version: u16,
    pub weather_interval_start: u64,
    pub precipitation: JourneyPrecipitation,
    pub intensity_bps: u16,
    pub ground_moisture_bps: u16,
    pub snow_cover_bps: u16,
    pub distance_m: u64,
    pub minutes: u64,
    pub points: Vec<JourneyRoutePoint>,
    pub spans: Vec<JourneyTerrainSpan>,
    pub return_route: Option<JourneyRouteLeg>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum JourneyPrecipitation {
    Clear,
    Rain,
    Snow,
}

#[derive(Clone, Debug)]
#[table(accessor = party_journey_route_authority)]
pub struct PartyJourneyRoute {
    #[primary_key]
    pub party_id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    pub package_digest: String,
    pub weather_rules_version: u16,
    pub weather_interval_start: u64,
    pub precipitation: JourneyPrecipitation,
    pub intensity_bps: u16,
    pub ground_moisture_bps: u16,
    pub snow_cover_bps: u16,
    pub distance_m: u64,
    pub minutes: u64,
    pub points: Vec<JourneyRoutePoint>,
    pub spans: Vec<JourneyTerrainSpan>,
    pub return_route: Option<JourneyRouteLeg>,
}

fn strategic_view_is_gateway(ctx: &ViewContext) -> bool {
    ctx.db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|row| row.identity == ctx.sender())
}

/// Gateway-only projection of journey endpoints and progress. Case-site names
/// and identifiers never reside in a globally subscribable table.
#[view(accessor = party_journey, public)]
pub fn party_journey(ctx: &ViewContext) -> Vec<PartyJourney> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .party_journey_authority()
        .gateway_bucket()
        .filter(0u8)
        .collect()
}

/// Gateway-only projection of exact route geometry.
#[view(accessor = party_journey_route, public)]
pub fn party_journey_route(ctx: &ViewContext) -> Vec<PartyJourneyRoute> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .party_journey_route_authority()
        .gateway_bucket()
        .filter(0u8)
        .collect()
}

/// The authenticated strategic-web identity trusted to submit server-planned
/// travel. The singleton also pins the immutable terrain package digest.
#[derive(Clone, Debug)]
#[table(accessor = strategic_gateway_authority, public)]
pub struct StrategicGatewayAuthority {
    #[primary_key]
    pub id: u8,
    pub identity: Identity,
    pub terrain_package_digest: Option<String>,
    pub terrain_schema: u32,
}

fn valid_route_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// First authenticated registration claims the singleton. Subsequent package
/// rotations must be made by the same SpacetimeDB identity.
#[reducer]
pub fn register_strategic_gateway(
    ctx: &ReducerContext,
    terrain_package_digest: Option<String>,
    terrain_schema: u32,
) -> Result<(), String> {
    if ctx.sender() == Identity::ZERO {
        return Err("Strategic gateway registration requires authentication".into());
    }
    if terrain_package_digest
        .as_deref()
        .is_some_and(|digest| !valid_route_digest(digest))
        || (terrain_package_digest.is_some() && terrain_schema != 3)
        || (terrain_package_digest.is_none() && terrain_schema != 0)
    {
        return Err("Strategic gateway terrain package metadata is invalid".into());
    }
    let authority = StrategicGatewayAuthority {
        id: 0,
        identity: ctx.sender(),
        terrain_package_digest,
        terrain_schema,
    };
    if let Some(existing) = ctx.db.strategic_gateway_authority().id().find(0) {
        if existing.identity != ctx.sender() {
            return Err("A different authenticated identity owns the strategic gateway".into());
        }
        ctx.db.strategic_gateway_authority().id().update(authority);
    } else {
        ctx.db.strategic_gateway_authority().insert(authority);
    }
    Ok(())
}

pub(crate) fn require_strategic_gateway(
    ctx: &ReducerContext,
) -> Result<StrategicGatewayAuthority, String> {
    let authority = ctx
        .db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .ok_or("Strategic gateway is not registered")?;
    if authority.identity != ctx.sender() {
        return Err("This reducer may only be called by the strategic gateway".into());
    }
    Ok(authority)
}

pub(crate) fn require_strategic_character_authority(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<(), String> {
    if require_strategic_gateway(ctx).is_ok()
        || crate::simulation::sender_owns_simulation_character(ctx, character_id)
    {
        Ok(())
    } else {
        Err("Character-mutating strategic reducers may only be called by the strategic gateway or the owner of the target disposable simulation character".into())
    }
}

#[derive(Clone, Debug)]
#[table(accessor = party_member, public)]
pub struct PartyMember {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub party_id: String,
    #[index(btree)]
    pub character_id: u64,
    pub role: Option<String>,
    pub recruitment_role_id: Option<u64>,
}

#[derive(Clone, Debug)]
#[table(accessor = party_inventory_item, public)]
pub struct PartyInventoryItem {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub party_id: String,
    #[index(btree)]
    pub item_id: String,
    pub quantity: u32,
}

/// Condition follows a durable item while it is held in the shared party pool.
/// Durable party rows are always individual (`quantity == 1`) and never merge.
#[derive(Clone, Debug)]
#[table(accessor = party_item_condition, public)]
pub struct PartyItemCondition {
    #[primary_key]
    pub party_inventory_item_id: u64,
    pub tier_1: f32,
    pub tier_2: f32,
    pub tier_3: f32,
    pub tier_4: f32,
    pub tier_5: f32,
}

/// Desired retained quantity used by bulk inventory actions. Party targets are
/// owned by the leader character so they survive party disbanding/recreation.
#[derive(Clone, Debug)]
#[table(
    accessor = inventory_quantity_target, public,
    index(accessor = owner_and_scope, btree(columns = [owner_character_id, party_scope])),
)]
pub struct InventoryQuantityTarget {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub owner_character_id: u64,
    pub party_scope: bool,
    #[index(btree)]
    pub item_id: String,
    pub quantity: u32,
}

#[reducer]
pub fn set_inventory_quantity_target(
    ctx: &ReducerContext,
    character_id: u64,
    party_scope: bool,
    item_id: String,
    quantity: u32,
) -> Result<(), String> {
    crate::character::require_living_character(ctx, character_id)?;
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if ctx.db.item().id().find(&item_id).is_none() {
        return Err("Item not found".into());
    }
    let owner_character_id = if party_scope {
        let party_id = character.party_id.ok_or("Character has no party")?;
        let party = ctx
            .db
            .party_authority()
            .id()
            .find(&party_id)
            .ok_or("Party not found")?;
        if party.leader_id != character_id {
            return Err("Only the party leader can change party quantity targets".into());
        }
        party.leader_id
    } else {
        character_id
    };
    let id = format!(
        "{}:{owner_character_id}:{item_id}",
        if party_scope { "party" } else { "player" }
    );
    let row = InventoryQuantityTarget {
        id: id.clone(),
        owner_character_id,
        party_scope,
        item_id,
        quantity,
    };
    if ctx.db.inventory_quantity_target().id().find(&id).is_some() {
        ctx.db.inventory_quantity_target().id().update(row);
    } else {
        ctx.db.inventory_quantity_target().insert(row);
    }
    Ok(())
}

#[derive(Clone, Debug)]
#[table(accessor = party_stake, public)]
pub struct PartyStake {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub party_id: String,
    #[index(btree)]
    pub character_id: u64,
    pub value: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = party_inventory_state, public)]
pub struct PartyInventoryState {
    #[primary_key]
    pub party_id: String,
    pub reserve_value: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = battle_result, public)]
pub struct BattleResult {
    #[primary_key]
    pub battle_id: String,
    #[index(btree)]
    pub party_id: String,
}

/// Reproducible strategic-combat diagnostics retained whether the party wins
/// or loses. Clients can show `summary` immediately and expand `log` on demand.
#[derive(Clone, Debug)]
#[table(accessor = autoresolve_report, public)]
pub struct AutoresolveReport {
    #[primary_key]
    pub battle_id: String,
    #[index(btree)]
    pub party_id: String,
    pub seed: u64,
    pub victor: String,
    pub rounds: u32,
    pub summary: String,
    pub log: Vec<String>,
}

#[derive(Clone, Debug)]
#[table(accessor = battle_loot_item, public)]
pub struct BattleLootItem {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub loot_battle_id: String,
    pub item_id: String,
    pub quantity: u32,
}

#[derive(Clone, Debug)]
#[table(accessor = battle_participant, public)]
pub struct BattleParticipant {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub participant_battle_id: String,
    pub character_id: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = backend_case_battle_authority)]
pub struct BackendCaseBattle {
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub owner_character_id: u64,
    /// Observer-safe generated public case ID, or the ordinary manual case ID.
    pub public_case_id: String,
    pub party_id: String,
    #[primary_key]
    pub battle_id: String,
    pub mission_id: String,
    pub case_site_id: CaseSiteId,
}

fn mission_public_case_id(
    ctx: &ReducerContext,
    mission: &MissionAuthority,
) -> Result<String, String> {
    let case = ctx
        .db
        .case_authority()
        .id()
        .find(&mission.case_id)
        .ok_or("Mission case authority not found")?;
    let authority = (!case.generated_case_id.is_empty())
        .then(|| {
            ctx.db
                .quest_generation_authority()
                .case_id()
                .find(&case.generated_case_id)
        })
        .flatten();
    Ok(
        validated_generated_dialogue_manifest(&case, authority.as_ref())?
            .map_or(case.id, |manifest| manifest.public_case_id),
    )
}

#[view(accessor = backend_case_battles, public)]
pub fn backend_case_battles(ctx: &ViewContext) -> Vec<BackendCaseBattle> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .backend_case_battle_authority()
        .gateway_bucket()
        .filter(0u8)
        .collect()
}

/// Persistent strategic identity for a specific combat opportunity. A mission
/// may be unbound (random encounter) or bound to both a known case site and a
/// specific hostile group. Enemy similarity never creates a binding.
#[derive(Clone, Debug)]
#[table(accessor = mission_authority)]
pub struct MissionAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub party_id: String,
    pub case_site_id: Option<CaseSiteId>,
    pub hostile_group_id: Option<String>,
    pub observer_character_id: u64,
    pub case_id: String,
    pub outcome_entropy: u64,
    pub status: MissionAttemptStatus,
    pub committed_resolution: Option<HostileResolutionKind>,
    pub committed_capture_subject_id: Option<String>,
    pub scene_key: String,
    /// Immutable combat/loot snapshot captured when this mission binds.
    pub hostile_version: u16,
    pub enemy_count: u32,
    pub enemy_difficulty: i32,
    pub base_enemy_combat_scale_bps: u32,
    pub enemy_combat_scale_bps: u32,
    pub countermeasure_multiplier_bps: u32,
    pub countermeasure_source_challenge_id: Option<String>,
    pub errantry_approach_snapshot_json: String,
    pub normalized_combat_power: u32,
    pub drop_item_id: Option<String>,
    pub drop_quantity: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum MissionAttemptStatus {
    Bound,
    Committed,
    Failed,
    Cancelled,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, SpacetimeType, serde::Serialize, serde::Deserialize,
)]
pub enum HostileResolutionKind {
    Defeated,
    DrivenOff,
    Captured,
    CaptureTargetKilled,
}

/// Private observer-authorized approach authority. These rows are exact,
/// objective-scoped capabilities, never a broad public "can capture" flag.
#[derive(Clone, Debug)]
#[table(accessor = mission_approach_capability)]
pub struct MissionApproachCapability {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub observer_character_id: u64,
    #[index(btree)]
    pub hostile_group_id: String,
    pub case_id: String,
    pub case_site_id: CaseSiteId,
    pub path_index: u16,
    pub objective_id: String,
    pub resolution: HostileResolutionKind,
    pub weight: u32,
    pub capture_subject_id: Option<String>,
    pub capture_custody_version: Option<u32>,
    pub active: bool,
}

/// Immutable private snapshot of the exact candidates a mission may sample.
/// The tactical child cannot read, select, or amend this manifest.
#[derive(Clone, Debug)]
#[table(accessor = mission_outcome_candidate)]
pub struct MissionOutcomeCandidate {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub mission_id: String,
    pub capability_id: String,
    pub case_id: String,
    pub case_site_id: CaseSiteId,
    pub hostile_group_id: String,
    pub path_index: u16,
    pub objective_id: String,
    pub resolution: HostileResolutionKind,
    pub weight: u32,
    pub capture_subject_id: Option<String>,
    pub capture_custody_version: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum HostileGroupDisposition {
    Active,
    Defeated,
    DrivenOff,
    Captured,
}

fn validate_hostile_resolution_contract(
    expected: Option<HostileResolutionKind>,
    expected_capture_subject: Option<&str>,
    resolution: HostileResolutionKind,
    capture_subject: Option<&str>,
    has_loot: bool,
) -> Result<(), &'static str> {
    if resolution == HostileResolutionKind::CaptureTargetKilled {
        return Err("A killed capture target is not a successful tactical resolution");
    }
    if resolution != HostileResolutionKind::Defeated && has_loot {
        return Err("Only defeated hostiles can produce battle loot");
    }
    if let Some(expected) = expected {
        if expected != resolution {
            return Err("Battle result does not match the mission-selected objective");
        }
        if resolution == HostileResolutionKind::Captured
            && expected_capture_subject != capture_subject
        {
            return Err("Captured subject does not match mission authority");
        }
    }
    Ok(())
}

#[derive(Clone, Debug)]
#[table(accessor = hostile_group_authority)]
pub struct HostileGroupAuthority {
    #[primary_key]
    pub id: String,
    /// Primitive query key for view contexts; it must exactly mirror the typed
    /// authority ID below.
    #[unique]
    pub case_site_id_key: String,
    #[unique]
    pub case_site_id: CaseSiteId,
    pub enemy_type: String,
    /// Immutable combat snapshot from initial materialization.
    pub base_enemy_count: u32,
    pub base_difficulty: i32,
    pub baseline_enemy_power: u32,
    pub enemy_count: u32,
    pub difficulty: i32,
    /// Ordinal of the latest recurring incident incorporated into this group.
    pub escalation_incident_ordinal: u16,
    pub escalation_progress_bps: u16,
    pub combat_scale_bps: u32,
    pub normalized_combat_power: u32,
    pub drop_item_id: Option<String>,
    pub drop_quantity: u32,
    pub disposition: HostileGroupDisposition,
}

fn materialize_hostile_group(
    ctx: &ReducerContext,
    hostile_group_id: &str,
    site: &CaseSiteAuthority,
    enemy_type: String,
    enemy_count: u32,
    difficulty: i32,
) -> Result<HostileGroupAuthority, String> {
    let group =
        hostile_group_authority_row(hostile_group_id, site, enemy_type, enemy_count, difficulty)?;
    if let Some(existing) = ctx.db.hostile_group_authority().id().find(&group.id) {
        return if existing.case_site_id == group.case_site_id
            && existing.case_site_id_key == group.case_site_id_key
            && existing.case_site_id_key == existing.case_site_id.value
            && existing.enemy_type == group.enemy_type
            && existing.base_enemy_count == group.base_enemy_count
            && existing.base_difficulty == group.base_difficulty
            && existing.baseline_enemy_power == group.baseline_enemy_power
            && existing.drop_item_id == group.drop_item_id
        {
            Ok(existing)
        } else {
            Err("Hostile-group ID is already bound to different authority".into())
        };
    }
    ctx.db.hostile_group_authority().insert(group.clone());
    Ok(group)
}

fn hostile_group_authority_row(
    hostile_group_id: &str,
    site: &CaseSiteAuthority,
    enemy_type: String,
    enemy_count: u32,
    difficulty: i32,
) -> Result<HostileGroupAuthority, String> {
    if hostile_group_id.is_empty() {
        return Err("Hostile-group authority requires a canonical ID".into());
    }
    let base_enemy_count =
        enemy_count.clamp(1, adventuresim_core::threat_escalation::MAX_MOB_COUNT);
    let base_difficulty = difficulty.max(1);
    let threat = parse_threat(&enemy_type)?;
    let profile = threat.profile().combat.escalation;
    let base = adventuresim_core::threat_escalation::combat_for_incident(
        base_enemy_count,
        base_difficulty,
        1,
        profile,
    );
    Ok(HostileGroupAuthority {
        id: hostile_group_id.to_string(),
        case_site_id_key: site.id.value.clone(),
        case_site_id: site.id.clone(),
        drop_item_id: autoresolve_drop(&enemy_type)?.map(str::to_string),
        drop_quantity: base_enemy_count,
        enemy_type,
        base_enemy_count,
        base_difficulty,
        baseline_enemy_power: profile.baseline_enemy_power,
        enemy_count: base.enemy_count,
        difficulty: base.difficulty,
        escalation_incident_ordinal: 1,
        escalation_progress_bps: 0,
        combat_scale_bps: base.combat_scale_bps,
        normalized_combat_power: base.normalized_combat_power,
        disposition: HostileGroupDisposition::Active,
    })
}

/// Idempotency and attribution receipt for a persistent victorious outcome.
/// Its primary key is supplied by the trusted battle producer.
#[derive(Clone, Debug)]
#[table(accessor = outcome_source_authority)]
pub struct OutcomeSourceAuthority {
    #[primary_key]
    pub id: String,
    #[unique]
    pub battle_id: String,
    pub mission_id: Option<String>,
    pub hostile_group_id: Option<String>,
    pub resolution: HostileResolutionKind,
    pub party_id: String,
}

#[derive(SpacetimeType, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecruitmentRequirements {
    pub melee: bool,
    pub ranged: bool,
    pub precise: bool,
    pub heavy: bool,
    pub quarter_armor: bool,
    pub half_armor: bool,
    pub three_quarter_armor: bool,
    pub full_armor: bool,
    pub blunt: bool,
    pub slash: bool,
    pub pierce: bool,
    pub athletics: u8,
    pub endurance: u8,
    pub physiology: u8,
    pub surgery: u8,
    pub command: u8,
    pub religion: u8,
}

impl From<RecruitmentRequirements> for adventuresim_core::capability::RoleRequirements {
    fn from(value: RecruitmentRequirements) -> Self {
        Self {
            melee: value.melee,
            ranged: value.ranged,
            weapon_precision: adventuresim_core::capability::legacy_weapon_precision(
                value.precise,
                value.blunt,
                value.slash,
                value.pierce,
            ),
            heavy: value.heavy,
            quarter_armor: value.quarter_armor,
            half_armor: value.half_armor,
            three_quarter_armor: value.three_quarter_armor,
            full_armor: value.full_armor,
            athletics: value.athletics,
            endurance: value.endurance,
            physiology: value.physiology,
            surgery: value.surgery,
            command: value.command,
            religion: value.religion,
        }
    }
}

#[derive(Clone, Debug)]
#[table(accessor = party_recruitment_role, public)]
pub struct PartyRecruitmentRole {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub party_id: String,
    pub name: String,
    pub requirements: RecruitmentRequirements,
    pub quantity: u32,
    #[default(0.0)]
    pub weapon_precision: f32,
}

#[derive(Clone, Debug)]
#[table(accessor = saved_recruitment_role, public)]
pub struct SavedRecruitmentRole {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub owner_character_id: u64,
    pub name: String,
    pub requirements: RecruitmentRequirements,
    #[default(0.0)]
    pub weapon_precision: f32,
}

#[derive(Clone, Debug)]
#[table(accessor = party_join_request, public)]
pub struct PartyJoinRequest {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub party_id: String,
    #[index(btree)]
    pub recruitment_role_id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub meets_requirements: bool,
}

/// A party member's proposed use of authority normally reserved for the leader.
/// `payload` is JSON so approval can replay the original typed reducer call.
#[derive(Clone, Debug)]
#[table(accessor = party_action_request_authority)]
pub struct PartyActionRequest {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub party_id: String,
    #[index(btree)]
    pub requester_id: u64,
    pub action_kind: String,
    pub summary: String,
    pub payload: String,
}

/// Gateway-only projection: proposed case-site travel contains observer-secret
/// identifiers and must never be visible to ordinary subscribers.
#[view(accessor = party_action_request, public)]
pub fn party_action_request(ctx: &ViewContext) -> Vec<PartyActionRequest> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .party_action_request_authority()
        .gateway_bucket()
        .filter(0u8)
        .collect()
}

#[derive(Clone, Debug)]
#[table(accessor = resolved_party_action)]
struct ResolvedPartyAction {
    #[primary_key]
    id: u64,
    party_id: String,
    approved_by: u64,
}

#[derive(serde::Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum ApprovedPartyAction {
    TravelToSettlement {
        settlement_id: String,
    },
    TravelToCaseSite {
        case_site_id: String,
    },
    RemovePartyMember {
        character_id: u64,
    },
    CreateRecruitmentRole {
        name: String,
        quantity: u32,
        requirements: RecruitmentRequirements,
        weapon_precision: f32,
        save_role: bool,
    },
    UpdateRecruitmentRole {
        role_id: u64,
        name: String,
        quantity: u32,
        requirements: RecruitmentRequirements,
        weapon_precision: f32,
    },
    DeleteRecruitmentRole {
        role_id: u64,
    },
    AcceptJoinRequest {
        request_id: u64,
    },
    RejectJoinRequest {
        request_id: u64,
    },
    AcceptContract {
        contract_id: String,
    },
    AbandonContract {
        contract_id: String,
    },
    ReportContract {
        contract_id: String,
    },
    AutoresolveMission {
        mission_id: String,
    },
    UpdatePartyCheckTargets {
        physiology: f32,
        command: f32,
        religion: f32,
    },
    SetInventoryQuantityTarget {
        item_id: String,
        quantity: u32,
    },
    DisbandParty {
        party_id: String,
    },
    RequestTacticalServer {
        mission_id: String,
        scene_key: String,
    },
    CancelMission {
        mission_id: String,
    },
    PerformInvestigation {
        action_id: String,
        method: String,
        expected_version: u32,
    },
}

impl ApprovedPartyAction {
    fn kind(&self) -> &'static str {
        match self {
            Self::TravelToSettlement { .. } | Self::TravelToCaseSite { .. } => "travel",
            Self::RemovePartyMember { .. } => "kick",
            Self::CreateRecruitmentRole { .. } => "add_role",
            Self::UpdateRecruitmentRole { .. } => "edit_role",
            Self::DeleteRecruitmentRole { .. } => "delete_role",
            Self::AcceptJoinRequest { .. } => "accept_join",
            Self::RejectJoinRequest { .. } => "reject_join",
            Self::AcceptContract { .. } => "accept_contract",
            Self::AbandonContract { .. } => "abandon_contract",
            Self::ReportContract { .. } => "report_contract",
            Self::AutoresolveMission { .. } => "autoresolve",
            Self::UpdatePartyCheckTargets { .. } => "party_checks",
            Self::SetInventoryQuantityTarget { .. } => "party_inventory",
            Self::DisbandParty { .. } => "disband_party",
            Self::RequestTacticalServer { .. } => "initiate_combat",
            Self::CancelMission { .. } => "cancel_mission",
            Self::PerformInvestigation { .. } => "investigate",
        }
    }

    fn execute(
        self,
        ctx: &ReducerContext,
        leader_id: u64,
        requester_id: u64,
    ) -> Result<(), String> {
        match self {
            Self::TravelToSettlement { settlement_id } => {
                travel_to_settlement(ctx, leader_id, settlement_id)
            }
            Self::TravelToCaseSite { case_site_id } => {
                travel_to_case_site(ctx, leader_id, CaseSiteId::from(case_site_id))
            }
            Self::RemovePartyMember { character_id } => {
                remove_party_member(ctx, leader_id, character_id)
            }
            Self::CreateRecruitmentRole {
                name,
                quantity,
                requirements,
                weapon_precision,
                save_role,
            } => create_recruitment_role(
                ctx,
                leader_id,
                name,
                quantity,
                requirements,
                weapon_precision,
                save_role,
            ),
            Self::UpdateRecruitmentRole {
                role_id,
                name,
                quantity,
                requirements,
                weapon_precision,
            } => update_recruitment_role(
                ctx,
                leader_id,
                role_id,
                name,
                quantity,
                requirements,
                weapon_precision,
            ),
            Self::DeleteRecruitmentRole { role_id } => {
                delete_recruitment_role(ctx, leader_id, role_id)
            }
            Self::AcceptJoinRequest { request_id } => {
                accept_party_join_request(ctx, leader_id, request_id)
            }
            Self::RejectJoinRequest { request_id } => {
                reject_party_join_request(ctx, leader_id, request_id)
            }
            Self::AcceptContract { contract_id } => accept_contract(ctx, leader_id, contract_id),
            Self::AbandonContract { contract_id } => abandon_contract(ctx, leader_id, contract_id),
            Self::ReportContract { contract_id } => report_contract(ctx, leader_id, contract_id),
            Self::AutoresolveMission { mission_id } => {
                autoresolve_mission(ctx, leader_id, mission_id)
            }
            Self::UpdatePartyCheckTargets {
                physiology,
                command,
                religion,
            } => update_party_check_targets(ctx, leader_id, physiology, command, religion),
            Self::SetInventoryQuantityTarget { item_id, quantity } => {
                set_inventory_quantity_target(ctx, leader_id, true, item_id, quantity)
            }
            Self::DisbandParty { party_id } => disband_party(ctx, leader_id, party_id),
            Self::RequestTacticalServer {
                mission_id,
                scene_key,
            } => crate::tactical::request_tactical_server(ctx, leader_id, mission_id, scene_key),
            Self::CancelMission { mission_id } => {
                cancel_mission_request(ctx, leader_id, mission_id)
            }
            Self::PerformInvestigation {
                action_id,
                method,
                expected_version,
            } => crate::investigation::perform_investigation_action_authorized(
                ctx,
                requester_id,
                action_id,
                method,
                expected_version,
                true,
            ),
        }
    }
}

#[derive(Clone, Debug)]
#[table(accessor = party_leader_vote, public)]
pub struct PartyLeaderVote {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub party_id: String,
    #[index(btree)]
    pub voter_id: u64,
    pub candidate_id: u64,
}
