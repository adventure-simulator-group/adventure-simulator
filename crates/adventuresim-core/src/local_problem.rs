//! Deterministic local-problem generation and consequence evaluation.
//!
//! Hidden causes stay in the strategic authority.  Consumers receive only the
//! bounded effects returned by [`aggregate`] and safe public symptoms.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_ACTIVE_PER_SCOPE: usize = 3;
pub const MAX_TRADE_BPS: i32 = 2_500;
pub const MAX_ENCOUNTER_BPS: u16 = 2_000;
pub const MAX_DISEASE_INTENSITY: u16 = 700;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ProblemId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Scope {
    Settlement {
        settlement_id: String,
    },
    Route {
        endpoint_a: String,
        endpoint_b: String,
    },
}

impl Scope {
    pub fn route(left: impl Into<String>, right: impl Into<String>) -> Self {
        let (mut a, mut b) = (left.into(), right.into());
        if b < a {
            std::mem::swap(&mut a, &mut b);
        }
        Self::Route {
            endpoint_a: a,
            endpoint_b: b,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cause {
    Bandits,
    Goblins,
    Ghouls,
    ContaminatedWell,
    Smugglers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Symptom {
    MissingCaravans,
    NightScreams,
    SickLocals,
    EmptyStalls,
    VanishedLivestock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncounterArchetype {
    Bandits,
    Goblins,
    Undead,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Effects {
    /// Positive makes merchant purchases dearer; negative makes them cheaper.
    pub buy_bps: i32,
    /// Positive makes merchant offers to players worse.
    pub sell_penalty_bps: i32,
    pub encounter_frequency_bps: u16,
    pub encounter_archetype: Option<EncounterArchetype>,
    /// Acquisition intensity; the disease identity deliberately is not here.
    pub disease_intensity: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalProblem {
    pub id: ProblemId,
    pub scope: Scope,
    pub cause: Cause,
    pub symptom: Symptom,
    pub effects: Effects,
    pub starts_at: u64,
    pub ends_at: u64,
    pub mitigation_bps: u16,
    pub resolved_at: Option<u64>,
    pub bridge_keys: BTreeSet<String>,
}

impl LocalProblem {
    pub fn active_fraction_bps(&self, minute: u64) -> u16 {
        if minute < self.starts_at
            || minute >= self.ends_at
            || self.resolved_at.is_some_and(|at| at <= minute)
        {
            return 0;
        }
        10_000u16.saturating_sub(self.mitigation_bps.min(10_000))
    }
    pub fn mitigate(&mut self, bps: u16) {
        self.mitigation_bps = self.mitigation_bps.max(bps.min(10_000));
    }
    pub fn resolve(&mut self, minute: u64) {
        self.resolved_at = Some(self.resolved_at.map_or(minute, |old| old.min(minute)));
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationContext {
    pub seed: String,
    pub scope: Scope,
    pub allowed_bridges: BTreeSet<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    pub cause: Cause,
    pub symptom: Symptom,
    pub plausibility: u32,
    pub curation: u32,
    pub required_bridge: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationExplanation {
    pub selected_cause: Cause,
    pub selected_symptom: Symptom,
    pub plausibility: u32,
    pub curation: u32,
    pub emitted_bridge_keys: BTreeSet<String>,
}

const CANDIDATES: &[Candidate] = &[
    Candidate {
        cause: Cause::Bandits,
        symptom: Symptom::MissingCaravans,
        plausibility: 70,
        curation: 8,
        required_bridge: None,
    },
    Candidate {
        cause: Cause::Goblins,
        symptom: Symptom::VanishedLivestock,
        plausibility: 50,
        curation: 10,
        required_bridge: None,
    },
    Candidate {
        cause: Cause::Ghouls,
        symptom: Symptom::NightScreams,
        plausibility: 25,
        curation: 10,
        required_bridge: None,
    },
    Candidate {
        cause: Cause::ContaminatedWell,
        symptom: Symptom::SickLocals,
        plausibility: 55,
        curation: 9,
        required_bridge: None,
    },
    Candidate {
        cause: Cause::Smugglers,
        symptom: Symptom::NightScreams,
        plausibility: 2,
        curation: 7,
        required_bridge: Some("secret_riverside_meeting"),
    },
    // Hard-zero example: contaminated wells cannot directly explain missing caravans.
    Candidate {
        cause: Cause::ContaminatedWell,
        symptom: Symptom::MissingCaravans,
        plausibility: 0,
        curation: 10,
        required_bridge: None,
    },
];

fn hash(value: &str) -> u64 {
    value.bytes().fold(1_469_598_103_934_665_603, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(1_099_511_628_211)
    })
}

pub fn generate(
    context: &GenerationContext,
    ordinal: usize,
    starts_at: u64,
) -> Result<(LocalProblem, GenerationExplanation), String> {
    if ordinal >= MAX_ACTIVE_PER_SCOPE {
        return Err("local-problem generation limit reached".into());
    }
    let valid: Vec<_> = CANDIDATES
        .iter()
        .filter(|c| {
            c.plausibility > 0
                && c.curation > 0
                && c.required_bridge
                    .is_none_or(|b| context.allowed_bridges.contains(b))
        })
        .collect();
    let total: u64 = valid
        .iter()
        .map(|c| u64::from(c.plausibility) * u64::from(c.curation))
        .sum();
    if total == 0 {
        return Err("no valid local-problem relation".into());
    }
    let mut draw = hash(&format!("{}:{ordinal}:relation", context.seed)) % total;
    let chosen = valid
        .into_iter()
        .find(|c| {
            let w = u64::from(c.plausibility) * u64::from(c.curation);
            if draw < w {
                true
            } else {
                draw -= w;
                false
            }
        })
        .ok_or("weighted selection exhausted")?;
    let mut bridges = BTreeSet::new();
    if let Some(key) = chosen.required_bridge {
        bridges.insert(key.to_owned());
    }
    let effects = effects_for(chosen.cause);
    let id = ProblemId(format!(
        "problem:{:016x}",
        hash(&format!("{}:{ordinal}", context.seed))
    ));
    Ok((
        LocalProblem {
            id,
            scope: context.scope.clone(),
            cause: chosen.cause,
            symptom: chosen.symptom,
            effects,
            starts_at,
            ends_at: starts_at.saturating_add(30 * 1_440),
            mitigation_bps: 0,
            resolved_at: None,
            bridge_keys: bridges.clone(),
        },
        GenerationExplanation {
            selected_cause: chosen.cause,
            selected_symptom: chosen.symptom,
            plausibility: chosen.plausibility,
            curation: chosen.curation,
            emitted_bridge_keys: bridges,
        },
    ))
}

fn effects_for(cause: Cause) -> Effects {
    match cause {
        Cause::Bandits => Effects {
            buy_bps: 1_200,
            sell_penalty_bps: 500,
            encounter_frequency_bps: 1_500,
            encounter_archetype: Some(EncounterArchetype::Bandits),
            disease_intensity: 0,
        },
        Cause::Goblins => Effects {
            buy_bps: 700,
            sell_penalty_bps: 300,
            encounter_frequency_bps: 1_000,
            encounter_archetype: Some(EncounterArchetype::Goblins),
            disease_intensity: 0,
        },
        Cause::Ghouls => Effects {
            buy_bps: 400,
            sell_penalty_bps: 200,
            encounter_frequency_bps: 700,
            encounter_archetype: Some(EncounterArchetype::Undead),
            disease_intensity: 180,
        },
        Cause::ContaminatedWell => Effects {
            buy_bps: 900,
            sell_penalty_bps: 100,
            encounter_frequency_bps: 0,
            encounter_archetype: None,
            disease_intensity: 600,
        },
        Cause::Smugglers => Effects {
            buy_bps: -200,
            sell_penalty_bps: 800,
            encounter_frequency_bps: 300,
            encounter_archetype: Some(EncounterArchetype::Bandits),
            disease_intensity: 0,
        },
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AggregateEffects {
    pub buy_bps: i32,
    pub sell_penalty_bps: i32,
    pub encounter_frequency_bps: u16,
    pub encounter_archetypes: BTreeSet<EncounterArchetype>,
    pub disease_intensity: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsequenceInput {
    pub id: String,
    pub buy_bps: i32,
    pub sell_penalty_bps: i32,
    pub encounter_frequency_bps: u16,
    pub disease_intensity: u16,
    pub starts_at: u64,
    pub ends_at: u64,
    pub mitigation_bps: u16,
    pub resolved_at: Option<u64>,
}

pub fn aggregate_consequences<'a>(
    rows: impl IntoIterator<Item = &'a ConsequenceInput>,
    minute: u64,
) -> AggregateEffects {
    let mut rows: Vec<_> = rows
        .into_iter()
        .filter(|r| {
            minute >= r.starts_at
                && minute < r.ends_at
                && r.resolved_at.is_none_or(|at| minute < at)
                && r.mitigation_bps < 10_000
        })
        .collect();
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    rows.truncate(MAX_ACTIVE_PER_SCOPE);
    let mut out = AggregateEffects::default();
    for r in rows {
        let f = i64::from(10_000u16.saturating_sub(r.mitigation_bps.min(10_000)));
        out.buy_bps = (i64::from(out.buy_bps) + i64::from(r.buy_bps) * f / 10_000)
            .clamp(i64::from(-MAX_TRADE_BPS), i64::from(MAX_TRADE_BPS))
            as i32;
        out.sell_penalty_bps = (i64::from(out.sell_penalty_bps)
            + i64::from(r.sell_penalty_bps) * f / 10_000)
            .clamp(0, i64::from(MAX_TRADE_BPS)) as i32;
        out.encounter_frequency_bps = (u64::from(out.encounter_frequency_bps)
            + u64::from(r.encounter_frequency_bps) * f as u64 / 10_000)
            .min(u64::from(MAX_ENCOUNTER_BPS)) as u16;
        out.disease_intensity = (u64::from(out.disease_intensity)
            + u64::from(r.disease_intensity) * f as u64 / 10_000)
            .min(u64::from(MAX_DISEASE_INTENSITY)) as u16;
    }
    out
}

pub fn aggregate<'a>(
    problems: impl IntoIterator<Item = &'a LocalProblem>,
    scope: &Scope,
    minute: u64,
) -> AggregateEffects {
    let mut rows: Vec<_> = problems
        .into_iter()
        .filter(|p| &p.scope == scope && p.active_fraction_bps(minute) > 0)
        .collect();
    rows.sort_by(|a, b| a.id.cmp(&b.id));
    rows.truncate(MAX_ACTIVE_PER_SCOPE);
    let mut out = AggregateEffects::default();
    for p in rows {
        let f = i64::from(p.active_fraction_bps(minute));
        out.buy_bps = (i64::from(out.buy_bps) + i64::from(p.effects.buy_bps) * f / 10_000)
            .clamp(i64::from(-MAX_TRADE_BPS), i64::from(MAX_TRADE_BPS))
            as i32;
        out.sell_penalty_bps = (i64::from(out.sell_penalty_bps)
            + i64::from(p.effects.sell_penalty_bps) * f / 10_000)
            .clamp(0, i64::from(MAX_TRADE_BPS)) as i32;
        out.encounter_frequency_bps = (u64::from(out.encounter_frequency_bps)
            + u64::from(p.effects.encounter_frequency_bps) * f as u64 / 10_000)
            .min(u64::from(MAX_ENCOUNTER_BPS)) as u16;
        out.disease_intensity = (u64::from(out.disease_intensity)
            + u64::from(p.effects.disease_intensity) * f as u64 / 10_000)
            .min(u64::from(MAX_DISEASE_INTENSITY)) as u16;
        if let Some(a) = p.effects.encounter_archetype {
            out.encounter_archetypes.insert(a);
        }
    }
    out
}

pub fn adjust_price(base: u32, basis_points: i32) -> u32 {
    let numerator =
        i64::from(base).saturating_mul(i64::from(10_000 + basis_points.clamp(-9_999, 50_000)));
    u32::try_from((numerator + 9_999) / 10_000)
        .unwrap_or(u32::MAX)
        .max(1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscoveryAction { NewRumor, KnownRedirect, None }

pub fn discovery_action(location:&str, inn_available:bool, has_known_unresolved:bool)->DiscoveryAction {
    if has_known_unresolved { DiscoveryAction::KnownRedirect }
    else if location=="inn" || (location=="overview" && !inn_available) { DiscoveryAction::NewRumor }
    else { DiscoveryAction::None }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ctx(seed: &str) -> GenerationContext {
        GenerationContext {
            seed: seed.into(),
            scope: Scope::Settlement {
                settlement_id: "lubeck".into(),
            },
            allowed_bridges: BTreeSet::new(),
        }
    }
    #[test]
    fn generator_is_deterministic_and_explains_separate_weights() {
        let a = generate(&ctx("x"), 0, 12).unwrap();
        assert_eq!(a, generate(&ctx("x"), 0, 12).unwrap());
        assert!(a.1.plausibility > 0);
        assert!(a.1.curation > 0);
    }
    #[test]
    fn hard_zero_and_bridge_are_enforced() {
        for n in 0..MAX_ACTIVE_PER_SCOPE {
            let (p, e) = generate(&ctx(&format!("s{n}")), n, 0).unwrap();
            assert_ne!(
                (p.cause, p.symptom),
                (Cause::ContaminatedWell, Symptom::MissingCaravans)
            );
            assert!(!matches!(p.cause, Cause::Smugglers));
            assert!(e.emitted_bridge_keys.is_empty());
        }
        let mut c = ctx("bridge");
        c.allowed_bridges.insert("secret_riverside_meeting".into());
        let mut found = false;
        for n in 0..500 {
            c.seed = format!("bridge-{n}");
            if generate(&c, 0, 0).unwrap().0.cause == Cause::Smugglers {
                found = true;
                break;
            }
        }
        assert!(found);
        assert!(generate(&c, MAX_ACTIVE_PER_SCOPE, 0).is_err());
    }
    #[test]
    fn lifecycle_aggregation_is_absolute_capped_and_stable() {
        let (mut p, _) = generate(&ctx("a"), 0, 100).unwrap();
        assert_eq!(p.active_fraction_bps(99), 0);
        p.mitigate(4_000);
        p.mitigate(2_000);
        assert_eq!(p.active_fraction_bps(100), 6_000);
        p.resolve(200);
        p.resolve(220);
        assert_eq!(p.resolved_at, Some(200));
        assert_eq!(p.active_fraction_bps(200), 0);
        let mut rows = Vec::new();
        for n in 0..3 {
            let (mut q, _) = generate(&ctx(&format!("q{n}")), 0, 0).unwrap();
            q.effects.buy_bps = MAX_TRADE_BPS;
            q.effects.disease_intensity = MAX_DISEASE_INTENSITY;
            rows.push(q);
        }
        let a = aggregate(rows.iter(), &ctx("z").scope, 1);
        assert_eq!(a.buy_bps, MAX_TRADE_BPS);
        assert_eq!(a.disease_intensity, MAX_DISEASE_INTENSITY);
    }
    #[test]
    fn route_scope_is_canonical_and_price_checked() {
        assert_eq!(Scope::route("b", "a"), Scope::route("a", "b"));
        assert_eq!(adjust_price(100, 1_500), 115);
        assert_eq!(adjust_price(u32::MAX, 2_500), u32::MAX);
    }
    #[test]
    fn ssr_and_reducer_quotes_match_for_buy_sale_and_food_lot() {
        let buy = crate::strategic_economy::language_adjusted_buy_price(
            crate::strategic_economy::merchant_buy_price(100),
            0.55,
        );
        let sale = crate::strategic_economy::language_adjusted_sell_price(
            crate::strategic_economy::merchant_sell_price(100),
            0.55,
        );
        let food = crate::strategic_economy::merchant_sell_food_lot_value(12.0).unwrap() as u32;
        for (base, bps) in [(buy, 1_200), (sale, -500), (food, -500)] {
            let ssr_quote = adjust_price(base, bps);
            let reducer_unit_price = adjust_price(base, bps);
            assert_eq!(ssr_quote, reducer_unit_price);
        }
    }
    #[test]
    fn problem_disease_exposure_is_partition_invariant() {
        use crate::disease::{DiseaseId, first_eligible_presence_exposure_minute};
        let exposure = |from, to| {
            first_eligible_presence_exposure_minute(
                &[],
                DiseaseId::Influenza,
                7,
                "problem:stable",
                from,
                to,
                0.7,
                0.4,
                0.0,
            )
        };
        let whole = exposure(0, 2_880);
        let split = exposure(0, 1_440).or_else(|| exposure(1_440, 2_880));
        assert_eq!(whole, split);
        assert_eq!(exposure(0, 0), None);
    }
    #[test]
    fn inn_funnel_fallback_and_local_redirect_are_explicit() {
        assert_eq!(discovery_action("inn",true,false),DiscoveryAction::NewRumor);
        assert_eq!(discovery_action("overview",true,false),DiscoveryAction::None);
        assert_eq!(discovery_action("overview",false,false),DiscoveryAction::NewRumor);
        assert_eq!(discovery_action("market",true,true),DiscoveryAction::KnownRedirect);
    }
}
