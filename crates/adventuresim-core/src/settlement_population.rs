//! Canonical deterministic weighted relations for persistent settlement residents.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgeBand {
    Child,
    Adolescent,
    Adult,
    Elder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresenceBridge {
    NearbyHome,
    HouseholdErrand,
    RetainerErrand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profession {
    Artisan,
    Householder,
    Laborer,
    Retainer,
    ServiceProvider,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Schedule {
    Provider,
    Day,
    Evening,
    Early,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationContext {
    Overview,
    Market,
    Forge,
    Armoury,
    Tailor,
    Herbalist,
    Inn,
    Church,
    Residences,
    Keep,
    Organization,
    AdultVenue,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationInput {
    pub seed: String,
    pub location: LocationContext,
    pub is_service_provider: bool,
    pub service_id: Option<String>,
    pub profession_override: Option<String>,
    pub local_role: String,
    pub available_bridges: BTreeSet<PresenceBridge>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationCandidate<T> {
    pub value: T,
    pub plausibility: u32,
    pub curation: u32,
    pub bridge: Option<PresenceBridge>,
    /// Set for outcomes whose causal bridge is part of their validity, not flavor.
    pub requires_bridge: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationDecision {
    pub relation: String,
    pub context: String,
    pub decision: String,
    pub plausibility: u32,
    pub curation: u32,
    pub bridge: Option<PresenceBridge>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedPopulationProfile {
    pub age: AgeBand,
    pub profession: Profession,
    pub schedule: Schedule,
    pub height: String,
    pub build: String,
    pub hair: String,
    pub household_kind: String,
    pub decisions: Vec<RelationDecision>,
}

/// Stable FNV-1a rather than `DefaultHasher`, whose algorithm is not a persistence contract.
pub fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(1_469_598_103_934_665_603, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(1_099_511_628_211)
    })
}

pub fn choose<T: Copy + std::fmt::Debug>(
    seed: &str,
    relation: &str,
    context: &str,
    available_bridges: &BTreeSet<PresenceBridge>,
    candidates: &[RelationCandidate<T>],
) -> Result<(T, RelationDecision), String> {
    let valid: Vec<_> = candidates
        .iter()
        .copied()
        .filter(|candidate| {
            candidate.plausibility > 0
                && candidate.curation > 0
                && (!candidate.requires_bridge
                    || candidate
                        .bridge
                        .is_some_and(|bridge| available_bridges.contains(&bridge)))
        })
        .collect();
    let total: u64 = valid
        .iter()
        .map(|candidate| u64::from(candidate.plausibility) * u64::from(candidate.curation))
        .sum();
    if total == 0 {
        return Err(format!(
            "No valid choice for relation {relation} in {context}"
        ));
    }
    let mut draw = stable_hash(&format!("{seed}:{relation}")) % total;
    for candidate in valid {
        let weight = u64::from(candidate.plausibility) * u64::from(candidate.curation);
        if draw < weight {
            return Ok((
                candidate.value,
                RelationDecision {
                    relation: relation.into(),
                    context: context.into(),
                    decision: format!("{:?}", candidate.value).to_ascii_lowercase(),
                    plausibility: candidate.plausibility,
                    curation: candidate.curation,
                    bridge: candidate.bridge,
                },
            ));
        }
        draw -= weight;
    }
    Err(format!("Weighted choice exhausted for relation {relation}"))
}

fn candidate<T>(value: T, plausibility: u32) -> RelationCandidate<T> {
    RelationCandidate {
        value,
        plausibility,
        curation: 10,
        bridge: None,
        requires_bridge: false,
    }
}
fn bridged<T>(value: T, plausibility: u32, bridge: PresenceBridge) -> RelationCandidate<T> {
    RelationCandidate {
        value,
        plausibility,
        curation: 10,
        bridge: Some(bridge),
        requires_bridge: true,
    }
}

pub fn generate(input: &GenerationInput) -> Result<GeneratedPopulationProfile, String> {
    let context = format!("{:?}", input.location).to_ascii_lowercase();
    let adult_only = input.is_service_provider || input.location == LocationContext::AdultVenue;
    let age_candidates = [
        candidate(AgeBand::Adult, if adult_only { 85 } else { 62 }),
        candidate(AgeBand::Elder, if adult_only { 15 } else { 18 }),
        bridged(
            AgeBand::Adolescent,
            if input.is_service_provider {
                0
            } else if input.location == LocationContext::AdultVenue {
                3
            } else {
                15
            },
            PresenceBridge::HouseholdErrand,
        ),
        bridged(
            AgeBand::Child,
            if input.is_service_provider {
                0
            } else if input.location == LocationContext::AdultVenue {
                1
            } else {
                5
            },
            PresenceBridge::NearbyHome,
        ),
    ];
    let profession_candidates = [
        candidate(
            Profession::Artisan,
            if input.is_service_provider {
                0
            } else if matches!(
                input.location,
                LocationContext::Forge
                    | LocationContext::Armoury
                    | LocationContext::Tailor
                    | LocationContext::Market
            ) {
                55
            } else {
                18
            },
        ),
        candidate(
            Profession::Householder,
            if input.is_service_provider {
                0
            } else if matches!(
                input.location,
                LocationContext::Overview | LocationContext::Residences
            ) {
                45
            } else {
                20
            },
        ),
        candidate(
            Profession::Laborer,
            if input.is_service_provider { 0 } else { 30 },
        ),
        if input.is_service_provider {
            candidate(Profession::Retainer, 0)
        } else if input.location == LocationContext::Keep {
            candidate(Profession::Retainer, 70)
        } else {
            bridged(Profession::Retainer, 2, PresenceBridge::RetainerErrand)
        },
        candidate(
            Profession::ServiceProvider,
            if input.is_service_provider { 100 } else { 0 },
        ),
    ];
    let (age, age_decision) = choose(
        &input.seed,
        "age_at_location",
        &context,
        &input.available_bridges,
        &age_candidates,
    )?;
    let (profession, mut profession_decision) = choose(
        &input.seed,
        "profession_at_location",
        &context,
        &input.available_bridges,
        &profession_candidates,
    )?;
    if profession == Profession::ServiceProvider {
        let override_name = input
            .profession_override
            .as_deref()
            .ok_or("Service provider generation requires a profession override")?;
        profession_decision.decision = override_name.into();
        profession_decision.context = format!(
            "service:{};role:{}",
            input.service_id.as_deref().unwrap_or("unknown"),
            input.local_role
        );
    }
    let schedule_candidates = if input.is_service_provider {
        [
            candidate(Schedule::Provider, 100),
            candidate(Schedule::Day, 0),
            candidate(Schedule::Evening, 0),
            candidate(Schedule::Early, 0),
        ]
    } else {
        [
            candidate(Schedule::Provider, 0),
            candidate(
                Schedule::Day,
                if input.location == LocationContext::Inn {
                    35
                } else {
                    75
                },
            ),
            candidate(
                Schedule::Evening,
                if input.location == LocationContext::Inn {
                    65
                } else {
                    15
                },
            ),
            candidate(
                Schedule::Early,
                if matches!(profession, Profession::Laborer | Profession::Artisan) {
                    35
                } else {
                    10
                },
            ),
        ]
    };
    let (schedule, schedule_decision) = choose(
        &input.seed,
        "schedule_at_location",
        &context,
        &input.available_bridges,
        &schedule_candidates,
    )?;
    let (height, height_decision) = choose(
        &input.seed,
        "height",
        "demographic",
        &input.available_bridges,
        &[
            candidate("short", 25),
            candidate("average height", 55),
            candidate("tall", 20),
        ],
    )?;
    let (build, build_decision) = choose(
        &input.seed,
        "build_for_profession",
        &format!("{profession:?}"),
        &input.available_bridges,
        &[
            candidate("slender", 30),
            candidate(
                "sturdy",
                if matches!(
                    profession,
                    Profession::Artisan | Profession::Laborer | Profession::ServiceProvider
                ) {
                    60
                } else {
                    35
                },
            ),
            candidate("broad", 20),
        ],
    )?;
    let (hair, hair_decision) = choose(
        &input.seed,
        "hair_for_age",
        &format!("{age:?}"),
        &input.available_bridges,
        &[
            candidate("brown hair", 45),
            candidate("fair hair", 25),
            candidate("black hair", 15),
            candidate("red hair", 5),
            candidate("grey hair", if age == AgeBand::Elder { 60 } else { 5 }),
        ],
    )?;
    let (household_kind, household_decision) = choose(
        &input.seed,
        "household_for_age_profession",
        &format!("{age:?}:{profession:?}"),
        &input.available_bridges,
        &[
            candidate(
                "independent",
                if matches!(age, AgeBand::Adult | AgeBand::Elder) {
                    60
                } else {
                    0
                },
            ),
            candidate(
                "family",
                if matches!(age, AgeBand::Child | AgeBand::Adolescent) {
                    80
                } else {
                    35
                },
            ),
            candidate(
                "employer",
                if profession == Profession::Retainer || profession == Profession::ServiceProvider {
                    70
                } else {
                    5
                },
            ),
        ],
    )?;
    Ok(GeneratedPopulationProfile {
        age,
        profession,
        schedule,
        height: height.into(),
        build: build.into(),
        hair: hair.into(),
        household_kind: household_kind.into(),
        decisions: vec![
            age_decision,
            profession_decision,
            schedule_decision,
            height_decision,
            build_decision,
            hair_decision,
            household_decision,
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(seed: &str, location: LocationContext) -> GenerationInput {
        GenerationInput {
            seed: seed.into(),
            location,
            is_service_provider: false,
            service_id: None,
            profession_override: None,
            local_role: "witness".into(),
            available_bridges: BTreeSet::from([
                PresenceBridge::NearbyHome,
                PresenceBridge::HouseholdErrand,
                PresenceBridge::RetainerErrand,
            ]),
        }
    }
    #[test]
    fn production_profile_is_deterministic_and_explanation_round_trips() {
        let value = generate(&input("same", LocationContext::Market)).unwrap();
        assert_eq!(
            value,
            generate(&input("same", LocationContext::Market)).unwrap()
        );
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(value, serde_json::from_str(&json).unwrap());
    }
    #[test]
    fn hard_zero_and_missing_required_bridges_are_impossible() {
        let mut i = input("x", LocationContext::AdultVenue);
        i.available_bridges.clear();
        for n in 0..500 {
            i.seed = format!("x-{n}");
            let p = generate(&i).unwrap();
            assert!(matches!(p.age, AgeBand::Adult | AgeBand::Elder));
            assert_ne!(p.profession, Profession::Retainer);
        }
    }
    #[test]
    fn adult_venue_young_people_are_rare_and_always_bridged() {
        let mut young = 0;
        for n in 0..4000 {
            let p = generate(&input(&format!("venue-{n}"), LocationContext::AdultVenue)).unwrap();
            if matches!(p.age, AgeBand::Child | AgeBand::Adolescent) {
                young += 1;
                let d = p
                    .decisions
                    .iter()
                    .find(|d| d.relation == "age_at_location")
                    .unwrap();
                assert!(matches!(
                    d.bridge,
                    Some(PresenceBridge::NearbyHome | PresenceBridge::HouseholdErrand)
                ));
            }
        }
        assert!(young > 0 && young < 400);
    }
    #[test]
    fn rare_retainer_outside_keep_preserves_bridge() {
        let mut found = None;
        for n in 0..20000 {
            let p = generate(&input(&format!("retainer-{n}"), LocationContext::Overview)).unwrap();
            if p.profession == Profession::Retainer {
                found = Some(p);
                break;
            }
        }
        let p = found.expect("deterministic corpus should include rare retainer");
        assert_eq!(
            p.decisions
                .iter()
                .find(|d| d.relation == "profession_at_location")
                .unwrap()
                .bridge,
            Some(PresenceBridge::RetainerErrand)
        );
    }
}
