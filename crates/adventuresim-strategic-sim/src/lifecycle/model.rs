use serde::{Deserialize, Serialize};

pub const LIFECYCLE_REPORT_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleCadence {
    Whole,
    Daily,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HousingMetrics {
    pub catalog_valid: bool,
    pub offer_count: u8,
    pub tier_names: Vec<String>,
    pub tiers_strictly_ordered: bool,
    pub renter_periods_paid: u64,
    pub renter_partial_funds_retained: bool,
    pub renter_has_unpaid_period: bool,
    pub owner_periods_paid: u64,
    pub owner_partial_funds_retained: bool,
    pub owner_has_unpaid_period: bool,
    pub ownership_recurring_cost_is_lower: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoraleMetrics {
    pub residence_source_capped: bool,
    pub spouse_source_capped: bool,
    pub combined_source_capped: bool,
    pub residence_refresh_days: u64,
    pub spouse_refresh_days: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SocializingMetrics {
    pub selected_role: String,
    pub priority_fallback_roles: Vec<String>,
    pub priority_order_verified: bool,
    pub stable_ambiguous_choice: bool,
    pub personality_training_budget_basis_points: u16,
    pub trains_charm: bool,
    pub trains_insight: bool,
    pub trains_deception: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CourtshipMetrics {
    pub formal_route_threshold_verified: bool,
    pub formal_route_requires_opposite_sexes: bool,
    pub formal_route_requires_father_approval: bool,
    pub informal_personality_order_verified: bool,
    pub informal_route_covers_father_disapproval: bool,
    pub informal_route_covers_same_sex_couple: bool,
    pub secrecy_checks_required: bool,
    pub secrecy_attempts: u64,
    pub secrecy_successes: u64,
    pub secrecy_failures: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarriageMetrics {
    pub notice_days: u64,
    pub ceremonies: u64,
    pub dowry_payments: u64,
    pub duplicate_wedding_processing_ignored: bool,
    pub duplicate_dowry_processing_ignored: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FamilyMetrics {
    pub conserved_joint_leisure_minutes: u64,
    pub conception_trials: u64,
    pub conception_probability_per_ten_thousand: u16,
    pub pregnancies: u64,
    pub gestation_days: u64,
    pub births: u64,
    pub newborn_is_dependent: bool,
    pub child_identity_is_deterministic: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CausalMetrics {
    pub queued_characters: u64,
    pub processed_characters: u64,
    pub max_batch_size: u64,
    pub batches: u64,
    pub bounded_batching_verified: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleMetrics {
    pub housing: HousingMetrics,
    pub morale: MoraleMetrics,
    pub socializing: SocializingMetrics,
    pub courtship: CourtshipMetrics,
    pub marriage: MarriageMetrics,
    pub family: FamilyMetrics,
    pub causal: CausalMetrics,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleReport {
    pub format_version: u32,
    pub evidence_tier: String,
    pub cadence: LifecycleCadence,
    pub seed: u64,
    pub elapsed_days: u64,
    pub passed: bool,
    pub normalized_digest: String,
    pub metrics: LifecycleMetrics,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleComparison {
    pub format_version: u32,
    pub passed: bool,
    pub normalized_digest: String,
    pub whole_report_digest: String,
    pub daily_report_digest: String,
    pub differences: Vec<String>,
    pub privacy_canary_absent: bool,
    pub privacy_findings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LifecycleBundle {
    pub whole: LifecycleReport,
    pub daily: LifecycleReport,
    pub comparison: LifecycleComparison,
}
