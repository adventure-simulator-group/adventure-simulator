//! SpacetimeDB response types

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredBestiaryResult {
    pub category: adventuresim_world_schema::BestiaryCategory,
    pub support_bps: u16,
    pub interpretation: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct BestiaryEnemyLoreView {
    pub id: String,
    pub name: String,
    pub is_primary: bool,
    pub strengths: Vec<String>,
    pub weaknesses: Vec<String>,
}

pub fn bestiary_enemy_lore(
    category: adventuresim_world_schema::BestiaryCategory,
) -> Vec<BestiaryEnemyLoreView> {
    adventuresim_core::bestiary::profiles_for_category(category)
        .into_iter()
        .map(|categorized| {
            let profile = categorized.profile;
            let lore = adventuresim_core::bestiary::implemented_combat_lore(profile);
            BestiaryEnemyLoreView {
                id: profile.id.as_str().into(),
                name: profile.display_name.into(),
                is_primary: categorized.is_primary,
                strengths: lore.strengths,
                weaknesses: lore.weaknesses,
            }
        })
        .collect()
}

#[derive(Clone, Debug, Serialize)]
pub struct BestiaryResultView {
    pub category: String,
    pub label: String,
    pub support_bps: u16,
    pub support_label: &'static str,
    pub interpretation: String,
    pub enemies: Vec<BestiaryEnemyLoreView>,
}

pub fn bestiary_support_label(support_bps: u16) -> &'static str {
    match support_bps {
        0 => "rules out",
        1..=2_500 => "argues against",
        2_501..=4_999 => "weakly argues against",
        5_000 => "inconclusive",
        5_001..=7_499 => "supports",
        7_500..=9_999 => "strongly supports",
        _ => "conclusive support",
    }
}

pub fn bestiary_support_color(support_bps: u16) -> String {
    let bounded = support_bps.min(10_000);
    if bounded <= 5_000 {
        format!("rgb(255 {} 0)", (255 * u32::from(bounded) + 2_500) / 5_000)
    } else {
        format!(
            "rgb({} 255 0)",
            (255 * u32::from(10_000 - bounded) + 2_500) / 5_000
        )
    }
}

impl From<StoredBestiaryResult> for BestiaryResultView {
    fn from(result: StoredBestiaryResult) -> Self {
        let category = result.category;
        Self {
            category: category.id().into(),
            label: category.label().into(),
            support_bps: result.support_bps,
            support_label: bestiary_support_label(result.support_bps),
            interpretation: result.interpretation,
            enemies: bestiary_enemy_lore(category),
        }
    }
}

pub fn bestiary_result_views(payload: &str) -> Vec<BestiaryResultView> {
    let mut categories = std::collections::BTreeSet::new();
    serde_json::from_str::<Vec<StoredBestiaryResult>>(payload)
        .unwrap_or_default()
        .into_iter()
        .filter(|result| {
            categories.insert(result.category)
                && result.support_bps <= 10_000
                && !result.interpretation.trim().is_empty()
                && result.interpretation.len() <= 1_024
        })
        .map(Into::into)
        .collect()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BackendLocalProblemTradeEffect {
    pub character_id: u64,
    pub settlement_id: String,
    pub buy_bps: i32,
    pub sell_penalty_bps: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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
    pub bestiary_results_json: String,
    pub recorded_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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
    #[serde(rename = "latitude_e_7")]
    pub latitude_e7: i32,
    #[serde(rename = "longitude_e_7")]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BackendInvestigationCaseSummary {
    pub owner_character_id: u64,
    pub case_id: String,
    pub title: String,
    pub status: String,
    pub latest_update_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BackendCaseBattle {
    pub case_id: String,
    pub party_id: String,
    pub battle_id: String,
    pub mission_id: String,
    pub case_site_id: CaseSiteId,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BackendCaseSitePin {
    pub owner_character_id: u64,
    pub case_id: String,
    pub case_site_id: String,
    pub origin_settlement_id: String,
    pub name: String,
    pub description: String,
    pub scene_key: String,
    #[serde(rename = "longitude_e_7")]
    pub longitude_e7: i32,
    #[serde(rename = "latitude_e_7")]
    pub latitude_e7: i32,
    pub coordinates_are_geographic: bool,
    pub distance_m: u64,
    pub knowledge_stage: String,
    pub tracked: bool,
    pub display_title: String,
    pub generated_case: bool,
    pub case_resolved: bool,
    pub combat_available: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BackendPhysicalEvidence {
    pub owner_character_id: u64,
    pub evidence_id: String,
    pub case_id: String,
    pub case_site_id: String,
    pub label: String,
    pub portrait_icon: String,
    pub description: String,
    pub topics_json: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BackendPhysicalEvidenceInspection {
    pub attempt_id: String,
    pub owner_character_id: u64,
    pub evidence_id: String,
    pub topic_id: String,
    pub stat_label: String,
    pub passed: bool,
    pub narration: String,
    pub bestiary_results_json: String,
    pub attempted_at: u64,
}
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use serde_json::Value;

/// Response from SpacetimeDB SQL query (array of result sets)
pub type QueryResponse = Vec<QueryResult>;

#[derive(Debug, Deserialize)]
pub struct QueryResult {
    pub schema: QuerySchema,
    pub rows: Vec<Value>,
}

#[derive(Debug, Deserialize)]
pub struct QuerySchema {
    pub elements: Vec<SchemaElement>,
}

#[derive(Debug, Deserialize)]
pub struct SchemaElement {
    pub name: Option<AlgebraicTypeRef>,
    pub algebraic_type: AlgebraicType,
}

#[derive(Debug, Deserialize)]
pub struct AlgebraicTypeRef {
    pub some: String,
}

// AlgebraicType can be many forms - we just need to accept any valid JSON
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum AlgebraicType {
    Value(serde_json::Value),
}

// Domain types matching strategic-db schema

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub id: u64,
    pub name: String,
    pub xp: u32,
    pub level: u32,
    pub gold: u32,
    pub current_settlement_id: Option<String>,
    #[serde(default)]
    pub current_case_site_id: Option<String>,
    pub party_id: Option<String>,
    pub age_years: u16,
    pub alive: bool,
    pub temporary: bool,
    /// SSR-only observer-specific count, populated after database decoding.
    #[serde(default, skip_serializing)]
    pub social_notification_count: usize,
    /// SSR-only actor/target preference used to decide portrait-action visibility.
    #[serde(default, skip_serializing)]
    pub automatic_social_chat_enabled: bool,
}

macro_rules! personality_axis {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
        pub enum $name { $($variant),+ }
    };
}
personality_axis!(Nerve {
    Neutral,
    Brave,
    Fearful
});
personality_axis!(Drive {
    Neutral,
    Ambitious,
    Content
});
personality_axis!(Outlook {
    Neutral,
    Sanguine,
    Brooding
});
personality_axis!(Sociability {
    Neutral,
    Gregarious,
    Solitary
});
personality_axis!(Conscience {
    Neutral,
    Compassionate,
    Callous,
    Cruel
});
personality_axis!(SelfRegard {
    Neutral,
    Proud,
    Humble
});
personality_axis!(Conviction {
    Neutral,
    Zealous,
    Irreverent
});
personality_axis!(Hygiene {
    Neutral,
    Slovenly,
    Cleanly
});
personality_axis!(Temperance {
    Neutral,
    Temperate,
    Drunkard
});

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterPersonality {
    pub character_id: u64,
    pub nerve: Nerve,
    pub drive: Drive,
    pub outlook: Outlook,
    pub sociability: Sociability,
    pub conscience: Conscience,
    pub self_regard: SelfRegard,
    pub conviction: Conviction,
    pub hygiene: Hygiene,
    pub temperance: Temperance,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FilthSubstance {
    Dirt,
    Blood,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FilthOrigin {
    Own,
    Foreign,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterFilth {
    pub id: u64,
    pub character_id: u64,
    pub substance: FilthSubstance,
    pub origin: FilthOrigin,
    pub amount: u16,
    pub deposited_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SettlementCategory {
    Unknown,
    Hamlet,
    Village,
    Town,
    City,
    Capital,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settlement {
    pub id: String,
    pub name: String,
    pub coord_x: f64,
    pub coord_y: f64,
    pub population_level: i32,
    pub population_estimate: u32,
    pub category: SettlementCategory,
    pub languages: adventuresim_world_schema::SettlementLanguageProfile,
    pub industries: adventuresim_world_schema::InferredIndustryProfile,
    pub economy: adventuresim_world_schema::SettlementEconomyProfile,
    #[serde(deserialize_with = "deserialize_settlement_religious_status")]
    pub religious_status: adventuresim_world_schema::SettlementReligiousStatus,
    pub scene_key: String,
    pub religion_id: String,
    pub currency_id: String,
    pub source_node_id: Option<u64>,
}

fn deserialize_settlement_religious_status<'de, D>(
    deserializer: D,
) -> Result<adventuresim_world_schema::SettlementReligiousStatus, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    serde_json::from_value(normalize_religious_status(value)).map_err(D::Error::custom)
}

fn normalize_religious_status(value: Value) -> Value {
    let Value::Object(mut status) = value else {
        return value;
    };
    if status.len() != 1 {
        return Value::Object(status);
    }

    let (variant, payload) = status.into_iter().next().expect("checked one variant");
    let payload = match variant.as_str() {
        "Established" => wrap_single_field(payload, "religion"),
        "LocallyDetermined" => wrap_single_field(payload, "church"),
        "Parity" | "MultiConfessional" => {
            wrap_single_field(normalize_western_arrangement(payload), "arrangement")
        }
        _ => payload,
    };
    status = [(variant, payload)].into_iter().collect();
    Value::Object(status)
}

fn normalize_western_arrangement(value: Value) -> Value {
    let Value::Object(mut arrangement) = value else {
        return value;
    };
    if arrangement.len() != 1 || arrangement.contains_key("arrangement") {
        return Value::Object(arrangement);
    }

    let (variant, payload) = arrangement
        .into_iter()
        .next()
        .expect("checked one arrangement variant");
    arrangement = [(variant, wrap_single_field(payload, "church"))]
        .into_iter()
        .collect();
    Value::Object(arrangement)
}

fn wrap_single_field(value: Value, field: &str) -> Value {
    if value
        .as_object()
        .is_some_and(|object| object.contains_key(field))
    {
        value
    } else {
        Value::Object([(field.to_string(), value)].into_iter().collect())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementAlias {
    pub id: String,
    pub settlement_id: String,
    pub name: String,
    pub prefix: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementDescription {
    pub id: String,
    pub settlement_id: String,
    pub kind: SettlementDescriptionKind,
    pub language: Option<String>,
    pub body: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SettlementDescriptionKind {
    #[serde(alias = "settlement")]
    Settlement,
    #[serde(alias = "city")]
    City,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TravelEdge {
    pub id: u64,
    pub from_node_id: u64,
    pub to_node_id: u64,
    pub route: adventuresim_world_schema::TravelRoute,
    pub length_m: u32,
    pub slope_multiplier: f32,
    pub terrain: adventuresim_world_schema::RouteTerrain,
    pub certainty: u8,
    pub section: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractPresentation {
    pub id: String,
    pub case_id: String,
    pub title: String,
    pub description: String,
    pub difficulty: i32,
    pub gold_reward: i32,
    pub xp_reward: i32,
    pub settlement_id: String,
    pub service_id: String,
    pub issuer_npc_id: String,
    pub status: ContractPresentationStatus,
    pub accepted_by: Option<String>,
    pub opposition_wording: String,
    pub opposition_count_wording: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContractPresentationStatus {
    Offered,
    Accepted,
    ReadyToReport,
    Paid,
    Withdrawn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecruitmentOfferId {
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecruitmentSourceId {
    pub value: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RecruitmentOfferStatus {
    Open,
    Closed,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecruitmentOffer {
    pub id_key: String,
    pub id: RecruitmentOfferId,
    pub source_id: RecruitmentSourceId,
    pub recruiting_party_id: String,
    pub settlement_id: String,
    pub settlement_npc_id: String,
    pub location_id: String,
    pub leader_id: u64,
    pub status: RecruitmentOfferStatus,
    pub created_at_minute: u64,
    pub expires_at_minute: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Party {
    pub id: String,
    pub name: String,
    pub leader_id: u64,
    pub current_settlement_id: Option<String>,
    pub current_case_site_id: Option<CaseSiteId>,
    pub active_contract_id: Option<String>,
    pub is_solo: bool,
    pub camp_fatigue_percent: u8,
    pub walking_minutes_per_day: u16,
    pub travel_at_night: bool,
    pub camp_duration_mode: CampDurationMode,
    pub fixed_camp_minutes: u16,
    pub camp_destination: Option<JourneyEndpoint>,
    pub camp_remaining_minutes: u64,
    pub pooled_water_ml: f32,
    pub medicine_target: f32,
    pub command_target: f32,
    pub religion_target: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum CampDurationMode {
    Auto,
    Fixed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyJourney {
    pub party_id: String,
    pub gateway_bucket: u8,
    pub origin: JourneyEndpoint,
    pub destination: JourneyEndpoint,
    pub total_minutes: u64,
    pub completed_minutes: u64,
    pub camp_stop_minutes: Vec<u64>,
    pub forecast_camp_stop_minutes: Vec<u64>,
    pub fatigue_percent: u8,
    pub plan_version: u8,
    pub departure_minute: u64,
    pub total_elapsed_minutes: u64,
    pub completed_elapsed_minutes: u64,
    pub walking_minutes_per_day: u16,
    pub travel_at_night: bool,
    pub camp_duration_mode: CampDurationMode,
    pub fixed_camp_minutes: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CaseSiteId {
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JourneySettlementEndpoint {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JourneyCaseSiteEndpoint {
    pub id: CaseSiteId,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JourneyEndpoint {
    Settlement(JourneySettlementEndpoint),
    CaseSite(JourneyCaseSiteEndpoint),
    Camp(String),
}

impl JourneyEndpoint {
    pub fn settlement_id(&self) -> Option<&str> {
        match self {
            Self::Settlement(endpoint) => Some(&endpoint.id),
            _ => None,
        }
    }

    pub fn case_site_id(&self) -> Option<&str> {
        match self {
            Self::CaseSite(endpoint) => Some(&endpoint.id.value),
            _ => None,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Settlement(endpoint) => &endpoint.name,
            Self::CaseSite(endpoint) => &endpoint.name,
            Self::Camp(_) => "Camp",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendCharacterCaseSiteLocation {
    pub character_id: u64,
    pub case_site_id: CaseSiteId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StrategicEncounterLoss {
    pub owner_kind: String,
    pub owner_id: u64,
    pub inventory_id: u64,
    pub item_id: String,
    pub quantity: u32,
    pub value_each: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategicEncounter {
    pub party_id: String,
    pub encounter_id: String,
    pub archetype: String,
    pub enemy_count: u16,
    pub roll_index: u64,
    pub journey_movement_minute: u64,
    pub journey_elapsed_minute: u64,
    pub absolute_minute: u64,
    #[serde(rename = "longitude_e_7")]
    pub longitude_e7: i32,
    #[serde(rename = "latitude_e_7")]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JourneyCampInterval {
    pub movement_minute: u64,
    pub elapsed_start_minute: u64,
    pub elapsed_minutes: u64,
    pub average_fatigue_start: f32,
    pub average_fatigue_end: f32,
    pub maximum_fatigue_end: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyJourneyItinerary {
    pub party_id: String,
    pub actual_camp_intervals: Vec<JourneyCampInterval>,
    pub forecast_camp_intervals: Vec<JourneyCampInterval>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JourneyTerrainKind {
    Road,
    Open,
    SparseWoods,
    DeepWoods,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JourneyRoutePoint {
    pub latitude_e7: i32,
    pub longitude_e7: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JourneyTerrainSpan {
    pub kind: JourneyTerrainKind,
    pub terrain: JourneyTerrainWeights,
    pub training_multiplier_permille: u16,
    pub check_millirank: u16,
    pub start_minute: u64,
    pub duration_minutes: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct JourneyTerrainWeights {
    pub plains: u16,
    pub forest: u16,
    pub hills: u16,
    pub urban: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JourneyRouteLeg {
    pub distance_m: u64,
    pub minutes: u64,
    pub points: Vec<JourneyRoutePoint>,
    pub spans: Vec<JourneyTerrainSpan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyJourneyRoute {
    pub party_id: String,
    pub package_digest: String,
    pub distance_m: u64,
    pub minutes: u64,
    pub points: Vec<JourneyRoutePoint>,
    pub spans: Vec<JourneyTerrainSpan>,
    pub return_route: Option<JourneyRouteLeg>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyMember {
    pub id: u64,
    pub party_id: String,
    pub character_id: u64,
    pub role: Option<String>,
    pub recruitment_role_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyActionRequest {
    pub id: u64,
    pub party_id: String,
    pub requester_id: u64,
    pub action_kind: String,
    pub summary: String,
    pub payload: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyLeaderVote {
    pub id: String,
    pub party_id: String,
    pub voter_id: u64,
    pub candidate_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendLocalChatMessage {
    pub id: u64,
    pub owner_character_id: u64,
    pub conversation_kind: String,
    pub subject_party_id: String,
    pub subject_npc_id: String,
    pub sender_id: u64,
    pub sender_name: String,
    pub body: String,
    pub created_micros: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyInventoryItem {
    pub id: u64,
    pub party_id: String,
    pub item_id: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryQuantityTarget {
    pub id: String,
    pub owner_character_id: u64,
    pub party_scope: bool,
    pub item_id: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyStake {
    pub id: u64,
    pub party_id: String,
    pub character_id: u64,
    pub value: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BattleResult {
    pub battle_id: String,
    pub party_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoresolveReport {
    pub battle_id: String,
    pub party_id: String,
    pub seed: u64,
    pub victor: String,
    pub rounds: u32,
    pub summary: String,
    pub log: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BattleLootItem {
    pub id: u64,
    pub loot_battle_id: String,
    pub item_id: String,
    pub quantity: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyJoinRequest {
    pub id: u64,
    pub party_id: String,
    pub recruitment_role_id: u64,
    pub character_id: u64,
    pub meets_requirements: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
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
    pub medicine: u8,
    pub surgery: u8,
    pub command: u8,
    pub religion: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartyRecruitmentRole {
    pub id: u64,
    pub party_id: String,
    pub name: String,
    pub requirements: RecruitmentRequirements,
    pub quantity: u32,
    pub weapon_precision: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedRecruitmentRole {
    pub id: u64,
    pub owner_character_id: u64,
    pub name: String,
    pub requirements: RecruitmentRequirements,
    pub weapon_precision: f32,
}

impl PartyRecruitmentRole {
    pub fn effective_weapon_precision(&self) -> f32 {
        self.weapon_precision
            .max(legacy_weapon_precision(self.requirements))
    }
}

impl SavedRecruitmentRole {
    pub fn effective_weapon_precision(&self) -> f32 {
        self.weapon_precision
            .max(legacy_weapon_precision(self.requirements))
    }
}

fn legacy_weapon_precision(requirements: RecruitmentRequirements) -> f32 {
    adventuresim_core::capability::legacy_weapon_precision(
        requirements.precise,
        requirements.blunt,
        requirements.slash,
        requirements.pierce,
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterCapability {
    pub character_id: u64,
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
    pub athletics: f32,
    pub endurance: f32,
    pub medicine: f32,
    pub human_lore: f32,
    pub knife: f32,
    pub tailoring: f32,
    pub surgery: f32,
    pub command: f32,
    pub religion: f32,
    pub weapon_precision: f32,
}

impl CharacterCapability {
    pub fn summary_tags(&self) -> Vec<String> {
        let mut tags = Vec::new();
        for (enabled, label) in [
            (self.melee, "Melee"),
            (self.ranged, "Ranged"),
            (self.heavy, "Heavy"),
        ] {
            if enabled {
                tags.push(label.into());
            }
        }
        if let Some(label) =
            adventuresim_core::capability::weapon_precision_tier_label(self.weapon_precision)
        {
            tags.push(label.into());
        }
        if self.full_armor {
            tags.push("Full armor".into());
        } else if self.three_quarter_armor {
            tags.push("3/4 armor".into());
        } else if self.half_armor {
            tags.push("1/2 armor".into());
        } else if self.quarter_armor {
            tags.push("1/4 armor".into());
        }
        for (value, label) in [
            (self.athletics, "Athletics"),
            (self.endurance, "Endurance"),
            (self.medicine, "Medicine"),
            (self.surgery, "Surgery"),
            (self.command, "Command"),
            (self.religion, "Religion"),
        ] {
            if adventuresim_core::capability::rating(value)
                >= adventuresim_core::capability::DEFAULT_NUMERIC_REQUIREMENT
            {
                tags.push(label.into());
            }
        }
        tags
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryItem {
    pub id: u64,
    pub character_id: u64,
    pub item_id: String,
    #[serde(alias = "quantity")]
    pub qty: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FoodPreparation {
    Raw,
    Preserved,
    PanFried,
    Stewed,
    Roasted,
    Baked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoodLot {
    pub id: u64,
    pub inventory_item_id: Option<u64>,
    pub party_inventory_item_id: Option<u64>,
    pub display_name: String,
    pub preparation: FoodPreparation,
    pub ingredient_item_ids: Vec<String>,
    pub ingredient_quantities: Vec<f32>,
    pub mass_kg: f32,
    pub nutrition_kcal: f32,
    pub total_value: f32,
    pub created_at_minute: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterNeeds {
    pub character_id: u64,
    pub food_balance_kcal: f32,
    pub water_balance_ml: f32,
    pub carried_water_ml: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CharacterEquip {
    #[serde(rename = "character_id")]
    pub _character_id: u64,
    pub left_hand_item_id: Option<u64>,
    pub right_hand_item_id: Option<u64>,
    pub left_arm_armor_id: Option<u64>,
    pub right_arm_armor_id: Option<u64>,
    pub left_leg_armor_id: Option<u64>,
    pub right_leg_armor_id: Option<u64>,
    pub head_armor_id: Option<u64>,
    pub chest_armor_id: Option<u64>,
    pub stomach_armor_id: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct EquippedMedication {
    pub inventory_item_id: u64,
    pub character_id: u64,
    pub disease_id: String,
    pub equipped_at: u64,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct ItemDefinition {
    pub id: String,
    pub weight: f32,
    #[serde(default)]
    pub slot: ItemSlot,
    pub kind: ItemKind,
    #[serde(default)]
    pub accuracy: f32,
    #[serde(default)]
    pub reach: f32,
    #[serde(default)]
    pub block: f32,
    #[serde(default)]
    pub coverage: f32,
    #[serde(default)]
    pub penetration: f32,
    #[serde(default)]
    pub resistance: f32,
    #[serde(default)]
    pub padding: f32,
    #[serde(default)]
    pub flexibility: f32,
    #[serde(default)]
    pub range_of_motion: f32,
    #[serde(default)]
    pub precise: bool,
    #[serde(default)]
    pub balance: f32,
    #[serde(default)]
    pub melee: bool,
    #[serde(default)]
    pub ranged: bool,
    #[serde(default)]
    pub weapon_skills: WeaponSkillDistribution,
    #[serde(default)]
    pub blunt: bool,
    #[serde(default)]
    pub slash: bool,
    #[serde(default)]
    pub pierce: bool,
    #[serde(default)]
    pub base_value: Option<u32>,
    #[serde(default)]
    pub nutrition_kcal: f32,
    #[serde(default)]
    pub water_capacity_ml: u32,
    #[serde(default)]
    pub alcohol_serving_ml: u32,
    #[serde(default)]
    pub alcohol_abv_basis_points: u16,
    #[serde(default)]
    pub alcohol_net_hydration_ml: u32,
    #[serde(default)]
    pub alcohol_disinfectant_effectiveness: u16,
    #[serde(default)]
    pub alcohol_disinfectant_focused: bool,
    #[serde(default)]
    pub alcohol_potable: bool,
    #[serde(default)]
    pub quality: u8,
    #[serde(default)]
    pub durability_yield: f32,
    #[serde(default)]
    pub durability_fracture: f32,
    #[serde(default)]
    pub durability_wear: f32,
    #[serde(default)]
    pub durability_failure_share: f32,
    #[serde(default)]
    pub edge_sensitivity: f32,
    #[serde(default)]
    pub handling_sensitivity: f32,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct WeaponSkillDistribution {
    pub polearm: f32,
    pub axe: f32,
    pub bludgeon: f32,
    pub sword: f32,
    pub knife: f32,
    pub bow: f32,
    pub crossbow: f32,
    pub firearm: f32,
    #[serde(alias = "throw")]
    pub throw_skill: f32,
}

impl WeaponSkillDistribution {
    pub fn core(self) -> adventuresim_core::equipment::WeaponSkillDistribution {
        adventuresim_core::equipment::WeaponSkillDistribution {
            polearm: self.polearm,
            axe: self.axe,
            bludgeon: self.bludgeon,
            sword: self.sword,
            knife: self.knife,
            bow: self.bow,
            crossbow: self.crossbow,
            firearm: self.firearm,
            throw: self.throw_skill,
        }
    }
}

impl Default for ItemDefinition {
    fn default() -> Self {
        Self {
            id: String::new(),
            weight: 0.0,
            slot: ItemSlot::None,
            kind: ItemKind::Simple,
            accuracy: 0.0,
            reach: 0.0,
            block: 0.0,
            coverage: 0.0,
            penetration: 0.0,
            resistance: 0.0,
            padding: 0.0,
            flexibility: 0.0,
            range_of_motion: 0.0,
            precise: false,
            balance: 0.0,
            melee: false,
            ranged: false,
            weapon_skills: WeaponSkillDistribution::default(),
            blunt: false,
            slash: false,
            pierce: false,
            base_value: None,
            nutrition_kcal: 0.0,
            water_capacity_ml: 0,
            alcohol_serving_ml: 0,
            alcohol_abv_basis_points: 0,
            alcohol_net_hydration_ml: 0,
            alcohol_disinfectant_effectiveness: 0,
            alcohol_disinfectant_focused: false,
            alcohol_potable: false,
            quality: 0,
            durability_yield: 0.0,
            durability_fracture: 0.0,
            durability_wear: 0.0,
            durability_failure_share: 0.0,
            edge_sensitivity: 0.0,
            handling_sensitivity: 0.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ItemCondition {
    pub inventory_item_id: u64,
    pub tier_1: f32,
    pub tier_2: f32,
    pub tier_3: f32,
    pub tier_4: f32,
    pub tier_5: f32,
}

impl ItemCondition {
    pub fn bins(&self) -> [f32; 5] {
        [
            self.tier_1,
            self.tier_2,
            self.tier_3,
            self.tier_4,
            self.tier_5,
        ]
    }
    pub fn total(&self) -> f32 {
        self.bins().iter().sum::<f32>().clamp(0.0, 1.0)
    }
    pub fn repairable(&self, skill: u8) -> f32 {
        self.bins().iter().take(skill.min(5) as usize).sum()
    }
    pub fn residual(&self, skill: u8) -> f32 {
        self.bins().iter().skip(skill.min(5) as usize).sum()
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct SettlementSmith {
    pub settlement_id: String,
    pub weaponsmith_skill: u8,
    pub armourer_skill: u8,
    pub tailor_skill: u8,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct RepairOrder {
    pub id: u64,
    pub owner_character_id: u64,
    pub inventory_item_id: u64,
    pub item_id: String,
    pub settlement_id: String,
    pub smith_skill: u8,
    pub submitted_at_minutes: u64,
    pub ready_at_minutes: u64,
    pub target_condition: f32,
    pub quoted_cost: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ItemSlot {
    #[default]
    None,
    LeftHolding,
    RightHolding,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
    Chest,
    Stomach,
    Head,
    AnyHolding,
    AnyArm,
    AnyLeg,
}

impl ItemSlot {
    pub fn sats_json(self) -> serde_json::Value {
        let tag = match self {
            Self::None => "none",
            Self::LeftHolding => "leftHolding",
            Self::RightHolding => "rightHolding",
            Self::LeftArm => "leftArm",
            Self::RightArm => "rightArm",
            Self::LeftLeg => "leftLeg",
            Self::RightLeg => "rightLeg",
            Self::Chest => "chest",
            Self::Stomach => "stomach",
            Self::Head => "head",
            Self::AnyHolding => "anyHolding",
            Self::AnyArm => "anyArm",
            Self::AnyLeg => "anyLeg",
        };
        serde_json::json!({ (tag): {} })
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum ItemKind {
    #[serde(alias = "Simple", alias = "simple")]
    Simple,
    #[serde(alias = "Weapon", alias = "weapon")]
    Weapon,
    #[serde(alias = "Armor", alias = "armor")]
    Armor,
    #[serde(alias = "Shield", alias = "shield")]
    Shield,
    #[serde(alias = "Clothing", alias = "clothing")]
    Clothing,
    #[serde(alias = "Currency", alias = "currency")]
    Currency,
    #[serde(alias = "Ingredient", alias = "ingredient")]
    Ingredient,
    #[serde(alias = "Medication", alias = "medication")]
    Medication,
    #[serde(alias = "Food", alias = "food")]
    Food,
}

/// Attribute values for a character. These mirror the public strategic tables
/// and are rendered as the base values on the character sheet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterAttributes {
    pub character_id: u64,
    pub endurance: f32,
    pub immunity: f32,
    pub gut: f32,
    pub precision: f32,
    pub intelligence: f32,
    pub instinct: f32,
    pub eyesight: f32,
    pub hearing: f32,
    pub left_arm_strength: f32,
    pub right_arm_strength: f32,
    pub left_leg_strength: f32,
    pub right_leg_strength: f32,
    pub left_arm_agility: f32,
    pub right_arm_agility: f32,
    pub left_leg_agility: f32,
    pub right_leg_agility: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CharacterSkills {
    pub character_id: u64,
    pub polearm_hours: f32,
    pub axe_hours: f32,
    pub bludgeon_hours: f32,
    pub sword_hours: f32,
    pub knife_hours: f32,
    pub dodge_hours: f32,
    pub block_hours: f32,
    pub bow_hours: f32,
    pub crossbow_hours: f32,
    pub firearm_hours: f32,
    pub throw_hours: f32,
    pub will_hours: f32,
    pub insight_hours: f32,
    pub self_awareness_hours: f32,
    pub humor_hours: f32,
    pub command_hours: f32,
    pub deception_hours: f32,
    pub seduction_hours: f32,
    pub medicine_hours: f32,
    pub cooking_hours: f32,
    pub religion_hours: adventuresim_world_schema::ReligionHours,
    pub bestiary_hours: adventuresim_world_schema::BestiaryHours,
    pub surgery_hours: f32,
    pub oral_languages: adventuresim_world_schema::OralLanguageHours,
    pub written_languages: adventuresim_world_schema::WrittenLanguageHours,
    pub stealth_hours: f32,
    pub balance_hours: f32,
    pub terrain_plains_hours: f32,
    pub terrain_forest_hours: f32,
    pub terrain_hills_hours: f32,
    pub terrain_urban_hours: f32,
    pub tailoring_hours: f32,
    pub smithing_hours: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterTime {
    pub character_id: u64,
    pub minutes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterApprenticeship {
    pub id: u64,
    pub character_id: u64,
    pub service_id: String,
    pub religion_id: Option<String>,
    pub started_minute: u64,
    pub apprenticeship_minutes_accrued: u64,
    pub practice_minutes_accrued: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlcoholConsumption {
    pub id: String,
    pub character_id: u64,
    pub evening_id: u64,
    pub ethanol_ml: u32,
    pub morale_evaluated: bool,
}

/// Queried only by strategic-web and immediately sanitized. Browser responses
/// never serialize this private disease row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfectionEpisodeRow {
    pub id: u64,
    pub character_id: u64,
    pub disease_id: String,
    pub contracted_at: u64,
    pub treated_at: Option<u64>,
}
/// Queried only by strategic-web and filtered to the authenticated examining
/// character before any medical knowledge is rendered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedicalExaminationRow {
    pub id: u64,
    pub doctor_id: u64,
    pub target_id: u64,
    pub examined_at: u64,
    pub findings: Vec<String>,
    pub reveals_vitals: bool,
    pub sanguine: f32,
    pub phlegmatic: f32,
    pub choleric: f32,
    pub melancholic: f32,
    pub possible_disease_ids: Vec<String>,
    pub confirmed_infection_ids: Vec<u64>,
    pub confirmed_disease_ids: Vec<String>,
    pub confirmed_stages: Vec<String>,
}

/// Queried only by strategic-web long enough to deliver the authenticated
/// patient's name-only NPC result, then dismissed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HerbalistExaminationRow {
    pub id: u64,
    pub patient_id: u64,
    pub settlement_id: String,
    pub disease_names: Vec<String>,
    pub medication_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterStats {
    pub character_id: u64,
    pub calories_used: f32,
    pub focus: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScheduleAllocation {
    pub combat_training_minutes: u16,
    pub carousing_minutes: u16,
    pub apprenticeship_minutes: u16,
    pub apprenticeship_service_id: Option<String>,
    pub profession_practice_minutes: u16,
    pub profession_service_id: Option<String>,
    pub labor_minutes: u16,
    pub prayer_minutes: u16,
    pub thievery_minutes: u16,
    pub raiding_minutes: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterTrainingSchedule {
    pub character_id: u64,
    pub downtime: ScheduleAllocation,
    /// Legacy compatibility field; strategic travel no longer trains skills.
    pub travel: ScheduleAllocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterNotoriety {
    pub character_id: u64,
    pub value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterVirtue {
    pub character_id: u64,
    pub value: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendForageReceipt {
    pub character_id: u64,
    pub request_id: String,
    pub elapsed_minutes: u64,
    pub yielded_item_ids: Vec<String>,
    pub yielded_quantities: Vec<u16>,
    pub interrupted: bool,
    pub legal_outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldClock {
    pub id: u64,
    pub official_minutes: u64,
    pub epoch_micros: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterLimbs {
    pub character_id: u64,
    pub left_arm_health: f32,
    pub right_arm_health: f32,
    pub left_leg_health: f32,
    pub right_leg_health: f32,
    pub head_health: f32,
    pub chest_health: f32,
    pub stomach_health: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LimbRegion {
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
    Chest,
    Stomach,
    Head,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimbInjury {
    pub id: String,
    pub character_id: u64,
    pub limb: LimbRegion,
    pub cut_damage: f32,
    pub bruise_damage: f32,
    pub fracture_damage: f32,
    pub bandaged: bool,
    pub stitched: bool,
    pub stitch_quality: f32,
    pub splint_owner_id: Option<u64>,
    pub splint_inventory_item_id: Option<u64>,
    pub infection_exposure: f32,
    pub infection_checks: u32,
    pub infection_origin_minute: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProjectileKind {
    Arrowhead,
    Ball,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetainedProjectile {
    pub id: u64,
    pub character_id: u64,
    pub limb: LimbRegion,
    pub kind: ProjectileKind,
    pub extraction_dc: f32,
    pub source_damage: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterCondition {
    pub character_id: u64,
    pub body_weight_kg: f32,
    pub current_blood_ml: f32,
    pub maximum_blood_ml: f32,
    pub religion_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterStrategicCondition {
    pub character_id: u64,
    pub morale: f32,
    pub morale_bonus: f32,
    pub morale_bonus_cap: f32,
    pub fervor: f32,
    pub pain: f32,
    pub blood_loss: f32,
    pub fear: f32,
    pub fatigue: f32,
    pub hunger: f32,
    pub thirst: f32,
    /// Positive physiological food reserve in travel days; excludes inventory.
    pub food_days: f32,
    /// Positive physiological hydration reserve in travel days; excludes carried water.
    pub water_days: f32,
    pub water_capacity_ml: u32,
    pub incapacitation: f32,
    pub check_multiplier: f32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterMoraleSource {
    pub id: String,
    pub character_id: u64,
    pub kind: String,
    pub label: String,
    pub magnitude: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterAffinity {
    pub id: String,
    pub subject_id: u64,
    pub actor_id: u64,
    pub anchor: f32,
    pub anchor_minute: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterFamiliarity {
    pub id: String,
    pub low_id: u64,
    pub high_id: u64,
    pub shared_minutes: u64,
    pub joint_minute_anchor: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialBelief {
    pub id: String,
    pub observer_id: u64,
    pub subject_id: u64,
    pub axis: String,
    pub perceived_value: i8,
    pub confidence: f32,
    pub observed_at_minute: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialAddress {
    pub id: String,
    pub actor_id: u64,
    pub target_id: u64,
    pub source_id: String,
    pub addressed_at_minute: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomaticSocialChat {
    pub id: String,
    pub actor_id: u64,
    pub target_id: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReligiousDemand {
    pub id: u64,
    pub character_id: u64,
    pub kind: String,
    pub title: String,
    pub description: String,
    pub fervor: f32,
    pub status: String,
    pub created_at_minute: u64,
    pub resolved_at_minute: Option<u64>,
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TacticalServer {
    #[serde(default)]
    pub identity: Option<String>,
    pub mission_id: String,
    pub scene_key: String,
    pub party_id: String,
    #[serde(default)]
    pub status: MissionStatus,
    #[serde(default)]
    pub addr: String,
    #[serde(default)]
    pub cert_digest: String,
    #[serde(default)]
    pub character_id: Option<u64>,
}

impl TacticalServer {
    pub fn pending(mission_id: String, scene_key: String, party_id: String) -> Self {
        Self {
            identity: None,
            mission_id,
            scene_key,
            status: MissionStatus::Pending,
            addr: String::new(),
            cert_digest: String::new(),
            character_id: None,
            party_id,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum MissionStatus {
    #[default]
    #[serde(alias = "Ready", alias = "ready")]
    Ready,
    #[serde(
        alias = "Pending",
        alias = "pending",
        alias = "Requested",
        alias = "requested",
        alias = "Starting",
        alias = "starting"
    )]
    Pending,
    #[serde(alias = "Failed", alias = "failed", alias = "Error", alias = "error")]
    Failed,
    #[serde(alias = "Ended", alias = "ended", alias = "Stopped", alias = "stopped")]
    Ended,
}

impl MissionStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Pending => "Pending",
            Self::Failed => "Failed",
            Self::Ended => "Ended",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TacticalServerRequest {
    pub mission_id: String,
    pub scene_key: String,
    pub party_id: String,
    pub requested_by: u64,
    pub required_enemy_kills: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_core::{
        local_problem::Scope,
        quest_generation::{
            GenerationContext, TemplateFamily, generate, observer_scoped_id, test_witnesses,
        },
    };
    use std::collections::BTreeSet;

    #[test]
    fn bestiary_probability_colors_have_exact_anchors_and_safe_payloads() {
        assert_eq!(bestiary_support_color(0), "rgb(255 0 0)");
        assert_eq!(bestiary_support_color(5_000), "rgb(255 255 0)");
        assert_eq!(bestiary_support_color(10_000), "rgb(0 255 0)");
        assert_eq!(
            bestiary_result_views(
                r#"[{"category":"beast","support_bps":10000,"interpretation":"Canine print"}]"#
            )
            .len(),
            1
        );
        assert!(
            bestiary_result_views(
                r#"[{"category":"beast","support_bps":10000,"interpretation":"Canine print","difficulty_milli":1}]"#
            )
            .is_empty()
        );
    }

    #[test]
    fn strategic_statuses_reject_unknown_values() {
        assert_eq!(
            serde_json::from_str::<ContractPresentationStatus>("\"Accepted\"").unwrap(),
            ContractPresentationStatus::Accepted
        );
        assert!(serde_json::from_str::<ContractPresentationStatus>("\"accepted\"").is_err());
        assert!(serde_json::from_str::<ContractPresentationStatus>("\"mystery\"").is_err());
        assert_eq!(
            serde_json::from_str::<MissionStatus>("\"Starting\"").unwrap(),
            MissionStatus::Pending
        );
    }

    #[test]
    fn party_case_site_decodes_the_typed_spacetimedb_sql_shape() {
        let row = serde_json::json!({
            "id": "party-7",
            "gateway_bucket": 0,
            "name": "Ada's party",
            "leader_id": 7,
            "current_settlement_id": null,
            "current_case_site_id": { "value": "site:known" },
            "active_contract_id": null,
            "is_solo": true,
            "camp_fatigue_percent": 50,
            "walking_minutes_per_day": 480,
            "travel_at_night": false,
            "camp_duration_mode": "Auto",
            "fixed_camp_minutes": 0,
            "camp_destination": null,
            "camp_remaining_minutes": 0,
            "pooled_water_ml": 0.0,
            "medicine_target": 0.0,
            "command_target": 0.0,
            "religion_target": 0.0
        });
        let decoded: Party = serde_json::from_value(row).unwrap();
        assert_eq!(
            decoded.current_case_site_id,
            Some(CaseSiteId {
                value: "site:known".into()
            })
        );
    }

    #[test]
    fn investigation_projection_coordinates_match_spacetimedb_sql_schema() {
        let lead = serde_json::json!({
            "owner_character_id": 7,
            "case_id": "case-public",
            "lead_id": "lead-public",
            "summary": "Tracks cross the north road.",
            "source_label": "witness",
            "confidence_bps": 6500,
            "destination_stage": "exact_believed",
            "directions": "Beyond the old milestone.",
            "exact_location_id": "site-public",
            "latitude_e_7": 521234567,
            "longitude_e_7": 134567890,
            "witness_name": "Greta",
            "witness_description": "A tall cooper with grey hair.",
            "witness_occupation_or_relationship": "cooper",
            "expected_location": "Public square",
            "current_learned_location": "Public square",
            "contradiction_group": "creature-shape",
            "corrected_by": "",
            "recorded_at": 50000
        });
        let decoded: BackendInvestigationLead = serde_json::from_value(lead).unwrap();
        assert_eq!(decoded.latitude_e7, 521_234_567);
        assert_eq!(decoded.longitude_e7, 134_567_890);

        let pin = serde_json::json!({
            "owner_character_id": 7,
            "case_id": "case-public",
            "case_site_id": "site-public",
            "origin_settlement_id": "settlement-public",
            "name": "Old milestone",
            "description": "A weathered stone beside the north road.",
            "scene_key": "roadside",
            "longitude_e_7": 134567890,
            "latitude_e_7": 521234567,
            "coordinates_are_geographic": true,
            "distance_m": 1800,
            "knowledge_stage": "exact_believed",
            "tracked": true,
            "display_title": "Something preys on travellers",
            "generated_case": true,
            "combat_available": true,
            "case_resolved": false
        });
        let decoded: BackendCaseSitePin = serde_json::from_value(pin).unwrap();
        assert_eq!(decoded.latitude_e7, 521_234_567);
        assert_eq!(decoded.longitude_e7, 134_567_890);
        assert!(decoded.generated_case);
        assert!(decoded.combat_available);
        assert!(!decoded.case_resolved);
    }

    #[test]
    fn investigation_action_projection_decodes_eligibility_contract() {
        let action = serde_json::json!({
            "owner_character_id": 7,
            "action_id": "action-public",
            "method": "inspect_site",
            "expected_version": 2,
            "summary": "Inspect the abandoned croft.",
            "known_prerequisites": "Reach the croft.",
            "duration_min_minutes": 30,
            "duration_max_minutes": 90,
            "uncertainty_bps": 2000,
            "skill_contributions": "awareness",
            "weather_available": false,
            "required_case_site_id": "site-public",
            "available": false,
            "can_travel_to_required_site": true,
            "unavailable_reason": "Travel to the known investigation site before inspecting it."
        });
        let decoded: BackendInvestigationAction = serde_json::from_value(action).unwrap();
        assert_eq!(decoded.required_case_site_id, "site-public");
        assert!(!decoded.available);
        assert!(decoded.can_travel_to_required_site);
        assert!(decoded.unavailable_reason.contains("Travel"));
    }

    #[test]
    fn strategic_encounter_decodes_the_spacetimedb_sql_shape() {
        let row = serde_json::json!({
            "party_id": "party-7",
            "encounter_id": "party-7:3",
            "archetype": "bandits",
            "enemy_count": 4,
            "roll_index": 3,
            "journey_movement_minute": 540,
            "journey_elapsed_minute": 700,
            "absolute_minute": 1700,
            "longitude_e_7": 134567890,
            "latitude_e_7": 521234567,
            "terrain": "road",
            "party_aware": false,
            "enemy_aware": true,
            "available_choices": ["attack", "surrender"],
            "status": "awaiting_choice",
            "selected_choice": null,
            "selection_explanation": "The enemy surprised the party.",
            "party_speed_m_per_minute": 60,
            "enemy_speed_m_per_minute": 80,
            "run_ineligibility": "The enemy is faster.",
            "penalty_minutes": 0,
            "loss_preview": [],
            "outcome": null
        });

        let decoded: StrategicEncounter = serde_json::from_value(row).unwrap();
        assert_eq!(decoded.longitude_e7, 134_567_890);
        assert_eq!(decoded.latitude_e7, 521_234_567);
        assert_eq!(decoded.status, "awaiting_choice");
        assert_eq!(
            decoded.available_choices,
            vec!["attack".to_string(), "surrender".to_string()]
        );
    }

    #[test]
    fn investigation_projection_rejects_noncanonical_coordinate_names() {
        let lead = serde_json::json!({
            "owner_character_id": 7,
            "case_id": "case-public",
            "lead_id": "lead-public",
            "summary": "Tracks cross the north road.",
            "source_label": "witness",
            "confidence_bps": 6500,
            "destination_stage": "exact_believed",
            "directions": "Beyond the old milestone.",
            "exact_location_id": "site-public",
            "latitude_e7": 521234567,
            "longitude_e7": 134567890,
            "witness_name": "Greta",
            "witness_description": "A tall cooper with grey hair.",
            "witness_occupation_or_relationship": "cooper",
            "expected_location": "Public square",
            "current_learned_location": "Public square",
            "contradiction_group": "creature-shape",
            "corrected_by": "",
            "recorded_at": 50000
        });
        assert!(serde_json::from_value::<BackendInvestigationLead>(lead).is_err());
    }

    #[test]
    fn serialized_investigation_dtos_use_independent_opaque_ids() {
        let context = |entropy: u64, ordinal: u16| GenerationContext {
            seed: 0x4341_4e4f_4e49_4341,
            observer_entropy_hi: entropy,
            observer_entropy_lo: entropy.rotate_left(29) ^ 0x4c2d_5345_4e54_494e,
            settlement_id: "sentinel-settlement".into(),
            settlement_name: "Sentinel".into(),
            scope: Scope::Settlement {
                settlement_id: "sentinel-settlement".into(),
            },
            ordinal,
            now_minute: 50_000,
            requested_family: Some(TemplateFamily::RecurringDepredation),
            witness_candidates: test_witnesses(),
        };
        let first_context = context(11, 0);
        let second_context = context(12, 1);
        let first = generate(&first_context).unwrap();
        let second = generate(&second_context).unwrap();
        let capability_id = observer_scoped_id(
            &first_context,
            "capability",
            &format!("7:{}", first.actions[0].id.0),
        );
        let lead_id = observer_scoped_id(&first_context, "lead", "attempt-private-sentinel");
        let action = BackendInvestigationAction {
            owner_character_id: 7,
            action_id: capability_id.clone(),
            method: "search_area".into(),
            expected_version: 0,
            summary: "Search the reported area.".into(),
            known_prerequisites: "A local account.".into(),
            duration_min_minutes: 30,
            duration_max_minutes: 180,
            uncertainty_bps: 7_000,
            skill_contributions: "terrain".into(),
            weather_available: false,
            required_case_site_id: String::new(),
            available: true,
            can_travel_to_required_site: false,
            unavailable_reason: String::new(),
        };
        let lead = BackendInvestigationLead {
            owner_character_id: 7,
            case_id: first.public_case_id.clone(),
            lead_id: lead_id.clone(),
            summary: "A bounded lead.".into(),
            source_label: "witness".into(),
            confidence_bps: 5_000,
            destination_stage: "textual".into(),
            directions: String::new(),
            exact_location_id: String::new(),
            latitude_e7: 0,
            longitude_e7: 0,
            witness_name: String::new(),
            witness_description: String::new(),
            witness_occupation_or_relationship: String::new(),
            expected_location: String::new(),
            current_learned_location: String::new(),
            contradiction_group: String::new(),
            corrected_by: String::new(),
            recorded_at: 50_000,
        };
        let proposition_ids = first
            .evidence
            .iter()
            .map(|evidence| evidence.proposition_id.clone())
            .collect::<Vec<_>>();
        let witness_ids = first
            .witnesses
            .iter()
            .map(|witness| witness.id.0.clone())
            .collect::<Vec<_>>();
        let json = serde_json::to_string(&(
            &action,
            &lead,
            &first.actions,
            &witness_ids,
            &proposition_ids,
        ))
        .unwrap();
        assert!(!json.contains(&first.canonical_case_id));
        assert!(!json.contains("CANONICAL-SENTINEL"));
        assert!(!json.contains("attempt-private-sentinel"));
        let first_ids = first
            .actions
            .iter()
            .map(|item| item.id.0.clone())
            .chain(witness_ids.iter().cloned())
            .chain(proposition_ids.iter().cloned())
            .chain([capability_id, lead_id])
            .collect::<BTreeSet<_>>();
        let second_ids = second
            .actions
            .iter()
            .map(|item| item.id.0.clone())
            .chain(second.witnesses.iter().map(|item| item.id.0.clone()))
            .chain(
                second
                    .evidence
                    .iter()
                    .map(|item| item.proposition_id.clone()),
            )
            .chain([
                observer_scoped_id(&second_context, "capability", "same-logical-action"),
                observer_scoped_id(&second_context, "lead", "same-logical-attempt"),
            ])
            .collect::<BTreeSet<_>>();
        assert!(first_ids.is_disjoint(&second_ids));
        assert_eq!(
            first_ids.len(),
            first.actions.len() + witness_ids.len() + proposition_ids.len() + 2
        );
    }

    #[test]
    fn settlement_description_kind_is_a_closed_set() {
        assert_eq!(
            serde_json::from_str::<SettlementDescriptionKind>("\"city\"").unwrap(),
            SettlementDescriptionKind::City
        );
        assert!(serde_json::from_str::<SettlementDescriptionKind>("\"bridge\"").is_err());
    }

    #[test]
    fn item_slots_use_sats_tagged_sum_arguments() {
        assert_eq!(
            ItemSlot::None.sats_json(),
            serde_json::json!({ "none": {} })
        );
        assert_eq!(
            ItemSlot::AnyHolding.sats_json(),
            serde_json::json!({ "anyHolding": {} })
        );
    }

    #[test]
    fn settlement_religion_normalizes_single_field_sats_variants() {
        use adventuresim_world_schema::{
            CatholicLutheranChurch, OfficialReligion, SettlementReligiousStatus,
            WesternChristianArrangement,
        };

        let established: SettlementReligiousStatus = serde_json::from_value(
            normalize_religious_status(serde_json::json!({ "Established": "Lutheran" })),
        )
        .unwrap();
        assert_eq!(
            established,
            SettlementReligiousStatus::Established {
                religion: OfficialReligion::Lutheran,
            }
        );

        let parity: SettlementReligiousStatus =
            serde_json::from_value(normalize_religious_status(serde_json::json!({
                "Parity": { "CatholicLutheran": "RomanCatholic" }
            })))
            .unwrap();
        assert_eq!(
            parity,
            SettlementReligiousStatus::Parity {
                arrangement: WesternChristianArrangement::CatholicLutheran {
                    church: CatholicLutheranChurch::RomanCatholic,
                },
            }
        );
    }
}
