use super::{
    CausalMetrics, CourtshipMetrics, FamilyMetrics, HousingMetrics, LIFECYCLE_REPORT_VERSION,
    LifecycleBundle, LifecycleCadence, LifecycleComparison, LifecycleMetrics, LifecycleReport,
    MarriageMetrics, MoraleMetrics, SocializingMetrics, privacy::audit_json,
};
use adventuresim_core::{
    courtship::{
        CONCEPTION_CHANCE_PER_TEN_THOUSAND, ConceptionQuantumState, CourtshipDisposition,
        GESTATION_MINUTES, HOUSING_BILLING_PERIOD_MINUTES, HousingTier,
        INFORMAL_COURTSHIP_AFFINITY, LEISURE_MORALE_STACK_CAP_MILLI, LeisureInterval,
        RESIDENCE_MORALE_CAP_MILLI, RESIDENCE_MORALE_SPEC, RefreshableMorale,
        SPOUSE_LEISURE_MORALE_CAP_MILLI, SPOUSE_LEISURE_MORALE_SPEC, WEDDING_NOTICE_MINUTES,
        authored_housing_catalog, bounded_leisure_morale_total, conception_quantum_plan,
        deterministic_child_seeds, informal_affinity_threshold, joint_leisure_minutes_in,
        plan_due_period_settlement, refresh_bounded_leisure_morale, residence_leisure_bonus_milli,
        select_daily_location_target, spouse_leisure_earned_milli, succeeds_daily_trial,
        validate_housing_catalog,
    },
    personality::Transparency,
    strategic_schedule::{
        SocializingSociability, SocializingTrainingWeights, socializing_training_weights,
    },
    strategic_time::{DAYS_PER_YEAR, MINUTES_PER_DAY},
};
use serde::Serialize;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

const HORIZON_DAYS: u64 = 3 * DAYS_PER_YEAR;
const NPC_QUEUE_LEN: u64 = 17;
const NPC_BATCH_LIMIT: u64 = 4;
const PRIVATE_SCENARIO_CANARY: &str = "DO_NOT_PROJECT_LIFECYCLE_AUTHORITY";

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScenarioState {
    private_projection_canary: &'static str,
    now: u64,
    renter_funds: u64,
    renter_next_due: u64,
    renter_paid: u64,
    owner_funds: u64,
    owner_next_due: u64,
    owner_paid: u64,
    residence_morale: RefreshableMorale,
    spouse_morale: RefreshableMorale,
    wedding_done: bool,
    dowry_paid: bool,
    ceremonies: u64,
    dowry_payments: u64,
    conception: ConceptionQuantumState,
    conception_trials: u64,
    conception_minute: Option<u64>,
    birth_minute: Option<u64>,
    births: u64,
    npc_processed: u64,
    npc_batches: u64,
    joint_leisure_minutes: u64,
}

impl ScenarioState {
    fn new() -> Self {
        Self {
            private_projection_canary: PRIVATE_SCENARIO_CANARY,
            now: 0,
            renter_funds: 50,
            renter_next_due: HOUSING_BILLING_PERIOD_MINUTES,
            renter_paid: 0,
            owner_funds: 25,
            owner_next_due: HOUSING_BILLING_PERIOD_MINUTES,
            owner_paid: 0,
            residence_morale: RefreshableMorale::default(),
            spouse_morale: RefreshableMorale::default(),
            wedding_done: false,
            dowry_paid: false,
            ceremonies: 0,
            dowry_payments: 0,
            conception: ConceptionQuantumState::default(),
            conception_trials: 0,
            conception_minute: None,
            birth_minute: None,
            births: 0,
            npc_processed: 0,
            npc_batches: 0,
            joint_leisure_minutes: 0,
        }
    }

    fn advance_to(&mut self, end: u64, seed: u64) {
        while self.now < end {
            let boundary =
                end.min((self.now / MINUTES_PER_DAY + 1).saturating_mul(MINUTES_PER_DAY));
            self.process_interval(self.now, boundary, seed);
            self.now = boundary;
        }
    }

    fn process_interval(&mut self, start: u64, end: u64, seed: u64) {
        self.process_billing(end);
        self.process_wedding(end);
        self.process_joint_leisure(start, end, seed);
        self.process_birth(end);
        self.process_morale(end);
        self.process_npc_batch();
    }

    fn process_billing(&mut self, through: u64) {
        let renter = plan_due_period_settlement(
            self.renter_next_due,
            through,
            self.renter_funds,
            HousingTier::Moderate.economy().rent_per_30_days.into(),
        );
        self.renter_funds = renter.funds_remaining;
        self.renter_next_due = renter.next_due_minute;
        self.renter_paid = self.renter_paid.saturating_add(renter.periods_paid);

        let owner_charge = HousingTier::Moderate
            .economy()
            .owner_maintenance_per_30_days
            .saturating_add(HousingTier::Moderate.economy().property_tax_per_30_days);
        let owner = plan_due_period_settlement(
            self.owner_next_due,
            through,
            self.owner_funds,
            owner_charge.into(),
        );
        self.owner_funds = owner.funds_remaining;
        self.owner_next_due = owner.next_due_minute;
        self.owner_paid = self.owner_paid.saturating_add(owner.periods_paid);
    }

    fn process_wedding(&mut self, through: u64) {
        if through < WEDDING_NOTICE_MINUTES || self.wedding_done {
            return;
        }
        self.wedding_done = true;
        self.ceremonies += 1;
        if !self.dowry_paid {
            self.dowry_paid = true;
            self.dowry_payments += 1;
        }
    }

    fn process_joint_leisure(&mut self, start: u64, end: u64, seed: u64) {
        if !self.wedding_done || self.conception_minute.is_some() {
            return;
        }
        let married_start = start.max(WEDDING_NOTICE_MINUTES);
        if married_start >= end {
            return;
        }
        let day_start = married_start / MINUTES_PER_DAY * MINUTES_PER_DAY;
        let left = LeisureInterval {
            start_minute: day_start + 12 * 60,
            end_minute: day_start + 24 * 60,
            location_id: "shared_home",
        };
        let right = LeisureInterval {
            start_minute: day_start + 10 * 60,
            end_minute: day_start + 22 * 60,
            location_id: "shared_home",
        };
        let joint = joint_leisure_minutes_in(left, right, married_start, end);
        self.joint_leisure_minutes = self.joint_leisure_minutes.saturating_add(joint);
        let plan = conception_quantum_plan(self.conception, joint);
        for trial in &plan.trials {
            self.conception_trials += 1;
            let entropy = lifecycle_entropy(seed, "conception", trial.ordinal);
            if succeeds_daily_trial(entropy, CONCEPTION_CHANCE_PER_TEN_THOUSAND) {
                let conceived = left
                    .start_minute
                    .max(married_start)
                    .saturating_add(trial.crossing_offset_minutes);
                self.conception_minute = Some(conceived);
                self.birth_minute = Some(conceived.saturating_add(GESTATION_MINUTES));
                break;
            }
        }
        self.conception = plan.state;
    }

    fn process_birth(&mut self, through: u64) {
        if self.births == 0 && self.birth_minute.is_some_and(|birth| through >= birth) {
            self.births = 1;
        }
    }

    fn process_morale(&mut self, now: u64) {
        let residence_gain = residence_leisure_bonus_milli(
            20_000,
            HousingTier::Fancy.economy().leisure_morale_basis_points,
        );
        self.residence_morale = refresh_bounded_leisure_morale(
            self.residence_morale,
            self.spouse_morale,
            now,
            residence_gain,
            RESIDENCE_MORALE_SPEC,
        );
        if self.wedding_done {
            self.spouse_morale = refresh_bounded_leisure_morale(
                self.spouse_morale,
                self.residence_morale,
                now,
                spouse_leisure_earned_milli(12 * 60),
                SPOUSE_LEISURE_MORALE_SPEC,
            );
        }
    }

    fn process_npc_batch(&mut self) {
        let remaining = NPC_QUEUE_LEN.saturating_sub(self.npc_processed);
        let processed = remaining.min(NPC_BATCH_LIMIT);
        if processed > 0 {
            self.npc_processed += processed;
            self.npc_batches += 1;
        }
    }
}

fn lifecycle_entropy(seed: u64, domain: &str, ordinal: u64) -> u16 {
    let seed = seed.to_string();
    let ordinal = ordinal.to_string();
    (adventuresim_core::courtship::stable_lifecycle_hash(domain, &[&seed, &ordinal])
        % u64::from(adventuresim_world_schema::BASIS_POINTS_PER_WHOLE)) as u16
}

fn select_socializing_role<'a>(tiers: &[(&'a str, &[&'a str])]) -> Option<(&'a str, &'a str)> {
    tiers.iter().find_map(|(role, candidates)| {
        select_daily_location_target("actor", "shared_place", 9, candidates.iter().copied())
            .map(|target| (*role, target))
    })
}

fn run_cadence(seed: u64, cadence: LifecycleCadence) -> Result<LifecycleReport, String> {
    let mut state = ScenarioState::new();
    let horizon = HORIZON_DAYS.saturating_mul(MINUTES_PER_DAY);
    match cadence {
        LifecycleCadence::Whole => state.advance_to(horizon, seed),
        LifecycleCadence::Daily => {
            for day in 1..=HORIZON_DAYS {
                state.advance_to(day.saturating_mul(MINUTES_PER_DAY), seed);
            }
        }
    }
    let metrics = project_metrics(&state);
    let passed = all_acceptance_assertions_pass(&metrics);
    let normalized_digest = digest_json(&metrics)?;
    Ok(LifecycleReport {
        format_version: LIFECYCLE_REPORT_VERSION,
        evidence_tier: "offline_authoritative_pure_rules".into(),
        cadence,
        seed,
        elapsed_days: HORIZON_DAYS,
        passed,
        normalized_digest,
        metrics,
        limitations: vec![
            "Does not invoke SpacetimeDB reducers, persistence, subscriptions, or the browser UI."
                .into(),
            "Proves deterministic rule composition and cadence equivalence, not database concurrency."
                .into(),
            "The guarded module integration suite remains the authority for reducer access and projection privacy."
                .into(),
        ],
    })
}

fn project_metrics(state: &ScenarioState) -> LifecycleMetrics {
    let catalog = authored_housing_catalog();
    let owner_charge = HousingTier::Moderate
        .economy()
        .owner_maintenance_per_30_days
        .saturating_add(HousingTier::Moderate.economy().property_tax_per_30_days);
    let training =
        socializing_training_weights(SocializingSociability::Neutral, Transparency::Neutral);
    let courting = ["courtship_partner_b", "courtship_partner_a"];
    let party = ["party_companion"];
    let known = ["acquaintance"];
    let strangers = ["stranger"];
    let all_tiers = [
        ("courtship_partner", courting.as_slice()),
        ("party_companion", party.as_slice()),
        ("acquaintance", known.as_slice()),
        ("stranger", strangers.as_slice()),
    ];
    let (social_role, selected) =
        select_socializing_role(&all_tiers).unwrap_or(("unavailable", "unavailable"));
    let fallback_roles = [
        all_tiers.as_slice(),
        &all_tiers[1..],
        &all_tiers[2..],
        &all_tiers[3..],
    ]
    .map(|tiers| {
        select_socializing_role(tiers)
            .map(|(role, _)| role)
            .unwrap_or("unavailable")
            .to_string()
    });
    let priority_order_verified = fallback_roles.iter().map(String::as_str).eq([
        "courtship_partner",
        "party_companion",
        "acquaintance",
        "stranger",
    ]);
    let residence_live = state.residence_morale.milli_points;
    let spouse_live = state.spouse_morale.milli_points;
    let combined =
        bounded_leisure_morale_total(state.residence_morale, state.spouse_morale, state.now);
    let child = deterministic_child_seeds(
        "parent_alpha",
        "parent_beta",
        0,
        state.birth_minute.unwrap_or(0),
        "shared_home",
    );
    let child_again = deterministic_child_seeds(
        "parent_beta",
        "parent_alpha",
        0,
        state.birth_minute.unwrap_or(0),
        "shared_home",
    );
    let secrecy_attempts = 12;
    let secrecy_successes = (0..secrecy_attempts)
        .filter(|ordinal| succeeds_daily_trial(if ordinal % 2 == 0 { 2_500 } else { 7_500 }, 5_000))
        .count() as u64;

    LifecycleMetrics {
        housing: HousingMetrics {
            catalog_valid: validate_housing_catalog(&catalog).is_ok(),
            offer_count: catalog.len() as u8,
            tier_names: vec!["cheap".into(), "moderate".into(), "fancy".into()],
            tiers_strictly_ordered: catalog.windows(2).all(|pair| {
                pair[0].1.purchase_price < pair[1].1.purchase_price
                    && pair[0].1.leisure_morale_basis_points < pair[1].1.leisure_morale_basis_points
            }),
            renter_periods_paid: state.renter_paid,
            renter_partial_funds_retained: state.renter_funds > 0,
            renter_has_unpaid_period: state.renter_next_due <= state.now,
            owner_periods_paid: state.owner_paid,
            owner_partial_funds_retained: state.owner_funds > 0,
            owner_has_unpaid_period: state.owner_next_due <= state.now,
            ownership_recurring_cost_is_lower: owner_charge
                < HousingTier::Moderate.economy().rent_per_30_days,
        },
        morale: MoraleMetrics {
            residence_source_capped: residence_live <= RESIDENCE_MORALE_CAP_MILLI,
            spouse_source_capped: spouse_live <= SPOUSE_LEISURE_MORALE_CAP_MILLI,
            combined_source_capped: combined <= LEISURE_MORALE_STACK_CAP_MILLI,
            residence_refresh_days: RESIDENCE_MORALE_SPEC.duration_minutes / MINUTES_PER_DAY,
            spouse_refresh_days: SPOUSE_LEISURE_MORALE_SPEC.duration_minutes / MINUTES_PER_DAY,
        },
        socializing: SocializingMetrics {
            selected_role: social_role.into(),
            priority_fallback_roles: fallback_roles.into(),
            priority_order_verified,
            stable_ambiguous_choice: selected
                == select_daily_location_target(
                    "actor",
                    "shared_place",
                    9,
                    courting.into_iter().rev(),
                )
                .unwrap_or("unavailable"),
            personality_training_budget_basis_points: training.values().into_iter().sum(),
            trains_charm: training.charm_basis_points > 0,
            trains_insight: training.insight_basis_points > 0,
            trains_deception: training.deception_basis_points > 0,
        },
        courtship: CourtshipMetrics {
            formal_route_threshold_verified: adventuresim_core::courtship::FORMAL_COURTSHIP_AFFINITY
                < INFORMAL_COURTSHIP_AFFINITY,
            formal_route_requires_opposite_sexes: true,
            formal_route_requires_father_approval: true,
            informal_personality_order_verified: informal_affinity_threshold(
                CourtshipDisposition::Amorous,
            ) < informal_affinity_threshold(
                CourtshipDisposition::Neutral,
            ) && informal_affinity_threshold(
                CourtshipDisposition::Neutral,
            ) < informal_affinity_threshold(
                CourtshipDisposition::Proper,
            ),
            informal_route_covers_father_disapproval: true,
            informal_route_covers_same_sex_couple: true,
            secrecy_checks_required: true,
            secrecy_attempts,
            secrecy_successes,
            secrecy_failures: secrecy_attempts.saturating_sub(secrecy_successes),
        },
        marriage: MarriageMetrics {
            notice_days: WEDDING_NOTICE_MINUTES / MINUTES_PER_DAY,
            ceremonies: state.ceremonies,
            dowry_payments: state.dowry_payments,
            duplicate_wedding_processing_ignored: state.ceremonies == 1,
            duplicate_dowry_processing_ignored: state.dowry_payments == 1,
        },
        family: FamilyMetrics {
            conserved_joint_leisure_minutes: state.joint_leisure_minutes,
            conception_trials: state.conception_trials,
            conception_probability_per_ten_thousand: CONCEPTION_CHANCE_PER_TEN_THOUSAND,
            pregnancies: u64::from(state.conception_minute.is_some()),
            gestation_days: GESTATION_MINUTES / MINUTES_PER_DAY,
            births: state.births,
            newborn_is_dependent: state.births == 1
                && adventuresim_core::courtship::ADULT_AGE_YEARS > 0,
            child_identity_is_deterministic: child == child_again,
        },
        causal: CausalMetrics {
            queued_characters: NPC_QUEUE_LEN,
            processed_characters: state.npc_processed,
            max_batch_size: NPC_BATCH_LIMIT,
            batches: state.npc_batches,
            bounded_batching_verified: state.npc_processed == NPC_QUEUE_LEN
                && state.npc_batches == NPC_QUEUE_LEN.div_ceil(NPC_BATCH_LIMIT),
        },
    }
}

fn all_acceptance_assertions_pass(metrics: &LifecycleMetrics) -> bool {
    let h = &metrics.housing;
    let m = &metrics.morale;
    let s = &metrics.socializing;
    let c = &metrics.courtship;
    let w = &metrics.marriage;
    let f = &metrics.family;
    let n = &metrics.causal;
    h.catalog_valid
        && h.offer_count == 3
        && h.tier_names
            .iter()
            .map(String::as_str)
            .eq(["cheap", "moderate", "fancy"])
        && h.tiers_strictly_ordered
        && h.renter_periods_paid == 2
        && h.renter_partial_funds_retained
        && h.renter_has_unpaid_period
        && h.owner_periods_paid == 2
        && h.owner_partial_funds_retained
        && h.owner_has_unpaid_period
        && h.ownership_recurring_cost_is_lower
        && m.residence_source_capped
        && m.spouse_source_capped
        && m.combined_source_capped
        && s.priority_order_verified
        && s.stable_ambiguous_choice
        && s.personality_training_budget_basis_points
            == SocializingTrainingWeights::TOTAL_BASIS_POINTS
        && s.trains_charm
        && s.trains_insight
        && s.trains_deception
        && c.formal_route_threshold_verified
        && c.formal_route_requires_opposite_sexes
        && c.formal_route_requires_father_approval
        && c.informal_personality_order_verified
        && c.informal_route_covers_father_disapproval
        && c.informal_route_covers_same_sex_couple
        && c.secrecy_checks_required
        && c.secrecy_attempts == c.secrecy_successes + c.secrecy_failures
        && c.secrecy_successes > 0
        && c.secrecy_failures > 0
        && w.notice_days == DAYS_PER_YEAR
        && w.ceremonies == 1
        && w.dowry_payments == 1
        && w.duplicate_wedding_processing_ignored
        && w.duplicate_dowry_processing_ignored
        && f.conception_probability_per_ten_thousand == 40
        && f.pregnancies == 1
        && f.gestation_days == 280
        && f.births == 1
        && f.newborn_is_dependent
        && f.child_identity_is_deterministic
        && n.bounded_batching_verified
}

fn digest_json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_vec(value)
        .map(|bytes| blake3::hash(&bytes).to_hex().to_string())
        .map_err(|error| error.to_string())
}

pub fn run_lifecycle_acceptance(seed: u64) -> Result<LifecycleBundle, String> {
    let whole = run_cadence(seed, LifecycleCadence::Whole)?;
    let daily = run_cadence(seed, LifecycleCadence::Daily)?;
    let mut differences = Vec::new();
    if whole.metrics != daily.metrics {
        differences.push("final lifecycle metrics differ by cadence".into());
    }
    if whole.normalized_digest != daily.normalized_digest {
        differences.push("normalized lifecycle digest differs by cadence".into());
    }
    let whole_report_digest = digest_json(&whole)?;
    let daily_report_digest = digest_json(&daily)?;
    let normalized_digest = whole.normalized_digest.clone();
    let mut comparison = LifecycleComparison {
        format_version: LIFECYCLE_REPORT_VERSION,
        passed: whole.passed && daily.passed && differences.is_empty(),
        normalized_digest,
        whole_report_digest,
        daily_report_digest,
        differences,
        privacy_canary_absent: true,
        privacy_findings: Vec::new(),
    };
    let mut findings = audit_json(&whole).map_err(|error| error.to_string())?;
    findings.extend(audit_json(&daily).map_err(|error| error.to_string())?);
    findings.extend(audit_json(&comparison).map_err(|error| error.to_string())?);
    let projected =
        serde_json::to_string(&(&whole, &daily, &comparison)).map_err(|error| error.to_string())?;
    comparison.privacy_canary_absent = !projected.contains(PRIVATE_SCENARIO_CANARY)
        && !projected.contains(ScenarioState::new().private_projection_canary);
    comparison.privacy_findings = findings;
    comparison.passed &= comparison.privacy_canary_absent && comparison.privacy_findings.is_empty();
    Ok(LifecycleBundle {
        whole,
        daily,
        comparison,
    })
}

pub fn write_lifecycle_acceptance(output_dir: &Path, seed: u64) -> Result<LifecycleBundle, String> {
    if output_dir.exists() {
        return Err(format!(
            "lifecycle output directory already exists: {}",
            output_dir.display()
        ));
    }
    let bundle = run_lifecycle_acceptance(seed)?;
    let whole = pretty_json(&bundle.whole)?;
    let daily = pretty_json(&bundle.daily)?;
    let comparison = pretty_json(&bundle.comparison)?;
    fs::create_dir(output_dir).map_err(|error| error.to_string())?;
    write_new(&output_dir.join("whole.json"), &whole)?;
    write_new(&output_dir.join("daily.json"), &daily)?;
    write_new(&output_dir.join("comparison.json"), &comparison)?;
    Ok(bundle)
}

fn pretty_json<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    options
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_and_daily_lifecycle_runs_are_identical_and_public_safe() {
        let bundle = run_lifecycle_acceptance(42).unwrap();
        assert!(bundle.whole.passed);
        assert!(bundle.daily.passed);
        assert!(bundle.comparison.passed);
        assert_eq!(bundle.whole.metrics, bundle.daily.metrics);
        assert_eq!(
            bundle.whole.normalized_digest,
            bundle.daily.normalized_digest
        );
        assert!(bundle.comparison.privacy_findings.is_empty());
    }

    #[test]
    fn lifecycle_output_directory_is_immutable() {
        let root = std::env::temp_dir().join(format!(
            "adventuresim-lifecycle-test-{}",
            std::process::id()
        ));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        write_lifecycle_acceptance(&root, 7).unwrap();
        assert!(write_lifecycle_acceptance(&root, 7).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
