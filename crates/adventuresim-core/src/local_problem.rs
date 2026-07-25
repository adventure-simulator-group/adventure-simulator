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
/// The initial offence plus four follow-up incidents. This bounded ceiling is
/// temporary until other adventuring parties can retire neglected cases.
pub const MAX_INCIDENTS_PER_PROBLEM: u16 = 5;
pub const INCIDENT_INTERVAL_MINUTES: u64 = 2 * 1_440;
/// Each follow-up incident makes the unresolved consequences 25% more severe.
pub const INCIDENT_SEVERITY_STEP_BPS: u32 = 2_500;

pub fn due_incident_count(starts_at: u64, minute: u64) -> u16 {
    due_incident_count_configured(
        starts_at,
        minute,
        INCIDENT_INTERVAL_MINUTES,
        MAX_INCIDENTS_PER_PROBLEM,
    )
}

pub fn due_incident_count_configured(
    starts_at: u64,
    minute: u64,
    interval_minutes: u64,
    maximum_incidents: u16,
) -> u16 {
    if minute < starts_at {
        return 0;
    }
    assert!(interval_minutes > 0 && maximum_incidents > 0);
    let follow_ups = minute.saturating_sub(starts_at) / interval_minutes;
    u16::try_from(follow_ups.saturating_add(1))
        .unwrap_or(u16::MAX)
        .min(maximum_incidents)
}

pub fn incident_severity_bps(incident_count: u16) -> u32 {
    if incident_count == 0 {
        return 0;
    }
    10_000u32.saturating_add(
        u32::from(incident_count.saturating_sub(1)).saturating_mul(INCIDENT_SEVERITY_STEP_BPS),
    )
}

fn scale_i32_for_incidents(value: i32, incident_count: u16) -> i32 {
    (i64::from(value) * i64::from(incident_severity_bps(incident_count)) / 10_000)
        .clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn scale_u16_for_incidents(value: u16, incident_count: u16) -> u16 {
    (u64::from(value) * u64::from(incident_severity_bps(incident_count)) / 10_000)
        .min(u64::from(u16::MAX)) as u16
}

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
        cause: Cause::Bandits,
        symptom: Symptom::VanishedLivestock,
        plausibility: 10,
        curation: 7,
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
        cause: Cause::Bandits,
        symptom: Symptom::NightScreams,
        plausibility: 12,
        curation: 8,
        required_bridge: None,
    },
    Candidate {
        cause: Cause::Goblins,
        symptom: Symptom::NightScreams,
        plausibility: 20,
        curation: 9,
        required_bridge: None,
    },
    Candidate {
        cause: Cause::Ghouls,
        symptom: Symptom::SickLocals,
        plausibility: 15,
        curation: 8,
        required_bridge: None,
    },
    Candidate {
        cause: Cause::Smugglers,
        symptom: Symptom::MissingCaravans,
        plausibility: 18,
        curation: 7,
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
                && (!matches!(context.scope, Scope::Route { .. })
                    || effects_for(c.cause).encounter_frequency_bps > 0)
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
    pub incident_count: u16,
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
        let buy_bps = scale_i32_for_incidents(r.buy_bps, r.incident_count);
        let sell_penalty_bps = scale_i32_for_incidents(r.sell_penalty_bps, r.incident_count);
        let encounter_frequency_bps =
            scale_u16_for_incidents(r.encounter_frequency_bps, r.incident_count);
        let disease_intensity = scale_u16_for_incidents(r.disease_intensity, r.incident_count);
        out.buy_bps = (i64::from(out.buy_bps) + i64::from(buy_bps) * f / 10_000)
            .clamp(i64::from(-MAX_TRADE_BPS), i64::from(MAX_TRADE_BPS))
            as i32;
        out.sell_penalty_bps = (i64::from(out.sell_penalty_bps)
            + i64::from(sell_penalty_bps) * f / 10_000)
            .clamp(0, i64::from(MAX_TRADE_BPS)) as i32;
        out.encounter_frequency_bps = (u64::from(out.encounter_frequency_bps)
            + u64::from(encounter_frequency_bps) * f as u64 / 10_000)
            .min(u64::from(MAX_ENCOUNTER_BPS)) as u16;
        out.disease_intensity = (u64::from(out.disease_intensity)
            + u64::from(disease_intensity) * f as u64 / 10_000)
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
pub enum DiscoveryAction {
    NewRumor,
    KnownRedirect,
    None,
}

pub fn discovery_action(
    location: &str,
    inn_available: bool,
    has_known_unresolved: bool,
) -> DiscoveryAction {
    if has_known_unresolved {
        DiscoveryAction::KnownRedirect
    } else if location == "inn" || (location == "overview" && !inn_available) {
        DiscoveryAction::NewRumor
    } else {
        DiscoveryAction::None
    }
}

#[allow(clippy::too_many_arguments)]
/// Formats a safe referral, including an inline topic when the speaker is the
/// contact, and explicitly distinguishes a same-named contact from the speaker.
pub fn referral_presentation(
    summary: &str,
    source: Option<(&str, &str)>,
    contact_id: &str,
    contact_name: &str,
    contact_profession: &str,
    contact_height: &str,
    contact_build: &str,
    contact_hair: &str,
    tab: &str,
) -> ReferralPresentation {
    let description = format!("{contact_height}, {contact_build}, with {contact_hair}");
    if source.is_some_and(|(source_id, _)| source_id == contact_id) {
        return ReferralPresentation {
            lead: format!("{summary} I am the person you were sent to. Ask me about "),
            topic: Some(("what I saw".into(), "referred-testimony".into())),
            trailing: ".".into(),
        };
    }
    if source.is_some_and(|(source_id, source_name)| {
        source_id != contact_id && source_name.eq_ignore_ascii_case(contact_name)
    }) {
        return ReferralPresentation::plain(format!(
            "{summary} Ask the other {contact_name}—not me. The one you want is the {contact_profession}: {description}, usually found at the {tab}."
        ));
    }
    ReferralPresentation::plain(format!(
        "{summary} Ask {contact_name}—the {contact_profession}, {description}, usually found at the {tab}."
    ))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferralPresentation {
    pub lead: String,
    /// `(visible label, dialogue topic id)`.
    pub topic: Option<(String, String)>,
    pub trailing: String,
}

impl ReferralPresentation {
    fn plain(lead: String) -> Self {
        Self {
            lead,
            topic: None,
            trailing: String::new(),
        }
    }

    pub fn text(&self) -> String {
        let mut text = self.lead.clone();
        if let Some((label, _)) = &self.topic {
            text.push_str(label);
        }
        text.push_str(&self.trailing);
        text
    }
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
    fn referral_explicitly_disambiguates_a_same_named_other_person() {
        let text = referral_presentation(
            "Livestock have been disappearing.",
            Some(("npc:riverdale:inn:0", "Hans Wagner")),
            "npc:riverdale:residences:0",
            "Hans Wagner",
            "householder",
            "average height",
            "slender",
            "brown hair",
            "Residences",
        );

        assert_eq!(
            text.text(),
            "Livestock have been disappearing. Ask the other Hans Wagner—not me. The one you want is the householder: average height, slender, with brown hair, usually found at the Residences."
        );

        let self_referral = referral_presentation(
            "I saw it myself.",
            Some(("npc:riverdale:inn:0", "Hans Wagner")),
            "npc:riverdale:inn:0",
            "Hans Wagner",
            "innkeeper",
            "average height",
            "sturdy",
            "black hair",
            "Inn",
        );
        assert_eq!(
            self_referral.text(),
            "I saw it myself. I am the person you were sent to. Ask me about what I saw."
        );
        assert_eq!(
            self_referral.topic,
            Some(("what I saw".into(), "referred-testimony".into()))
        );
    }
    #[test]
    fn generator_is_deterministic_and_explains_separate_weights() {
        let a = generate(&ctx("x"), 0, 12).unwrap();
        assert_eq!(a, generate(&ctx("x"), 0, 12).unwrap());
        assert!(a.1.plausibility > 0);
        assert!(a.1.curation > 0);
    }
    #[test]
    fn unresolved_incidents_arrive_periodically_and_stop_at_the_cap() {
        let start = 10_000;
        assert_eq!(due_incident_count(start, start - 1), 0);
        assert_eq!(due_incident_count(start, start), 1);
        assert_eq!(
            due_incident_count(start, start + INCIDENT_INTERVAL_MINUTES),
            2
        );
        assert_eq!(
            due_incident_count(start, u64::MAX),
            MAX_INCIDENTS_PER_PROBLEM
        );
        assert_eq!(incident_severity_bps(1), 10_000);
        assert_eq!(incident_severity_bps(MAX_INCIDENTS_PER_PROBLEM), 20_000);
    }
    #[test]
    fn accumulated_incidents_scale_every_consequence_before_global_caps() {
        let row = ConsequenceInput {
            id: "problem".into(),
            buy_bps: 500,
            sell_penalty_bps: 200,
            encounter_frequency_bps: 300,
            disease_intensity: 100,
            starts_at: 0,
            ends_at: u64::MAX,
            mitigation_bps: 0,
            resolved_at: None,
            incident_count: 3,
        };
        let effects = aggregate_consequences([&row], 1);
        assert_eq!(effects.buy_bps, 750);
        assert_eq!(effects.sell_penalty_bps, 300);
        assert_eq!(effects.encounter_frequency_bps, 450);
        assert_eq!(effects.disease_intensity, 150);
    }
    #[test]
    fn every_generatable_public_symptom_has_multiple_possible_causes() {
        let symptoms: BTreeSet<_> = CANDIDATES
            .iter()
            .filter(|candidate| candidate.plausibility > 0 && candidate.curation > 0)
            .map(|candidate| candidate.symptom)
            .collect();
        for symptom in symptoms {
            let causes: BTreeSet<_> = CANDIDATES
                .iter()
                .filter(|candidate| {
                    candidate.symptom == symptom
                        && candidate.plausibility > 0
                        && candidate.curation > 0
                })
                .map(|candidate| candidate.cause)
                .collect();
            assert!(
                causes.len() >= 2,
                "public symptom {symptom:?} identifies {causes:?}"
            );
        }
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
        assert_eq!(
            discovery_action("inn", true, false),
            DiscoveryAction::NewRumor
        );
        assert_eq!(
            discovery_action("overview", true, false),
            DiscoveryAction::None
        );
        assert_eq!(
            discovery_action("overview", false, false),
            DiscoveryAction::NewRumor
        );
        assert_eq!(
            discovery_action("market", true, true),
            DiscoveryAction::KnownRedirect
        );
    }
    #[test]
    fn route_generation_never_selects_settlement_only_or_noop_causes() {
        for n in 0..500 {
            let c = GenerationContext {
                seed: format!("route-private-{n}"),
                scope: Scope::route("a", "b"),
                allowed_bridges: BTreeSet::new(),
            };
            let (p, _) = generate(&c, 0, 50_000).unwrap();
            assert_ne!(p.cause, Cause::ContaminatedWell);
            assert!(p.effects.encounter_frequency_bps > 0);
        }
    }
    #[test]
    fn symptoms_are_ambiguous_and_public_scope_does_not_recover_private_selection() {
        let mut causes = BTreeSet::new();
        for n in 0..2_000 {
            let (p, _) = generate(&ctx(&format!("private-{n}")), 0, 0).unwrap();
            if p.symptom == Symptom::NightScreams {
                causes.insert(p.cause);
            }
        }
        assert!(causes.len() >= 3);
        let public = generate(&ctx("local-problems:lubeck"), 0, 0)
            .unwrap()
            .0
            .cause;
        assert!((0..100).any(|n| {
            generate(&ctx(&format!("private-entropy-{n}")), 0, 0)
                .unwrap()
                .0
                .cause
                != public
        }));
    }
    #[test]
    fn expired_or_resolved_history_does_not_count_active_at_late_cycle() {
        let (mut old, _) = generate(&ctx("old"), 0, 0).unwrap();
        assert_eq!(old.active_fraction_bps(43_201), 0);
        old.resolve(10);
        assert_eq!(old.active_fraction_bps(20), 0);
        let (new, _) = generate(&ctx("new-private"), 0, 43_201).unwrap();
        assert!(new.active_fraction_bps(43_201) > 0);
    }
}
