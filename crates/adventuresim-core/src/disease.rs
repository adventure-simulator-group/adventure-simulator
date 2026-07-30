//! Deterministic, framework-neutral strategic disease simulation.
//!
//! An infection is completely described by its identity, associations and two
//! character-local timestamps. Everything else in this module is derived.

use crate::physiology::{self, BodyRegion, Humour, Meter, MeterVector};
use serde::{Deserialize, Serialize};

pub const DISEASE_RULESET_VERSION: u16 = 1;
pub const PHYSIOLOGY_VITALS_THRESHOLD: f32 = 2.0;
const SEED_DOMAIN: &[u8] = b"adventuresim/disease/severity/v1\0";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiseaseId {
    Influenza,
    Dysentery,
    Typhus,
    Tetanus,
    Erysipelas,
    Smallpox,
    Plague,
    Consumption,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiseaseStage {
    Incubating,
    Early,
    Established,
    Critical,
    Convalescent,
    Resolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Symptom {
    Coughing,
    Sneezing,
    Feverish,
    Fatigued,
    Vomiting,
    BloodyStool,
    Rash,
    Spasms,
    Lockjaw,
    Buboes,
    Trembling,
}

impl Symptom {
    pub const fn period_label(self) -> &'static str {
        match self {
            Self::Coughing => "coughing",
            Self::Sneezing => "sneezing",
            Self::Feverish => "feverish",
            Self::Fatigued => "fatigued",
            Self::Vomiting => "vomiting",
            Self::BloodyStool => "bloody flux",
            Self::Rash => "visible rash",
            Self::Spasms => "muscle spasms",
            Self::Lockjaw => "locked jaw",
            Self::Buboes => "swellings",
            Self::Trembling => "trembling",
        }
    }

    /// Every externally visible finding is forced through the same historical,
    /// deliberately lossy humour vocabulary as the private meter state.
    pub const fn humour(self) -> Humour {
        match self {
            Self::Coughing | Self::Sneezing => Humour::Phlegmatic,
            Self::Feverish | Self::Vomiting | Self::BloodyStool => Humour::Choleric,
            Self::Rash | Self::Buboes => Humour::Sanguine,
            Self::Fatigued | Self::Spasms | Self::Lockjaw | Self::Trembling => Humour::Melancholic,
        }
    }

    pub const fn observation_regions(self) -> &'static [BodyRegion] {
        use BodyRegion::{Abdomen, Chest, Head, LeftArm, LeftLeg, RightArm, RightLeg};
        match self {
            Self::Coughing => &[Chest],
            Self::Sneezing => &[Head, Chest],
            Self::Feverish | Self::Fatigued => {
                &[Head, Chest, Abdomen, LeftArm, RightArm, LeftLeg, RightLeg]
            }
            Self::Vomiting | Self::BloodyStool => &[Abdomen],
            Self::Rash => &[Head, Chest, Abdomen, LeftArm, RightArm, LeftLeg, RightLeg],
            Self::Spasms | Self::Trembling => &[LeftArm, RightArm, LeftLeg, RightLeg],
            Self::Lockjaw => &[Head],
            Self::Buboes => &[Chest, Abdomen, LeftArm, RightArm, LeftLeg, RightLeg],
        }
    }

    pub const fn humour_deviation(self) -> f32 {
        match self {
            Self::Coughing | Self::BloodyStool | Self::Spasms | Self::Lockjaw | Self::Buboes => {
                0.055
            }
            _ => 0.035,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AttributeImpairment {
    pub endurance: f32,
    pub immunity: f32,
    pub gut: f32,
    pub intelligence: f32,
    pub instinct: f32,
    pub limb_agility: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VitalImpairment {
    pub sanguine: f32,
    pub phlegmatic: f32,
    pub choleric: f32,
    pub melancholic: f32,
}

impl VitalImpairment {
    pub fn terminal_failure(self) -> Option<TerminalFailure> {
        [
            (self.phlegmatic, TerminalFailure::Respiratory),
            (self.sanguine, TerminalFailure::Circulatory),
            (self.choleric, TerminalFailure::Homeostatic),
            (self.melancholic, TerminalFailure::Neurologic),
        ]
        .into_iter()
        .filter(|(v, _)| *v >= 1.0)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, cause)| cause)
    }
    fn scaled(self, n: f32) -> Self {
        Self {
            sanguine: self.sanguine * n,
            phlegmatic: self.phlegmatic * n,
            choleric: self.choleric * n,
            melancholic: self.melancholic * n,
        }
    }
    fn add(&mut self, rhs: Self) {
        self.sanguine += rhs.sanguine;
        self.phlegmatic += rhs.phlegmatic;
        self.choleric += rhs.choleric;
        self.melancholic += rhs.melancholic;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminalFailure {
    Respiratory,
    Circulatory,
    Homeostatic,
    Neurologic,
}

#[derive(Clone, Copy, Debug)]
pub struct DiseaseDefinition {
    pub id: DiseaseId,
    pub period_name: &'static str,
    pub contagion: &'static str,
    pub incubation_minutes: u64,
    pub rise_minutes: u64,
    pub peak_minutes: u64,
    pub recovery_minutes: u64,
    pub base_acquisition: f32,
    pub peak_vitals: VitalImpairment,
    pub peak_attributes: AttributeImpairment,
    pub symptoms: &'static [Symptom],
    pub acquired_immunity: f32,
    pub transmission_vectors: &'static [TransmissionVector],
    /// Route represented by abstract settlement/outbreak intensity. Direct
    /// exposures such as food, wounds, and blood always supply their route.
    pub primary_community_vector: TransmissionVector,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransmissionVector {
    CloseContact,
    FoodWater,
    Vermin,
    Wound,
    Blood,
}

impl DiseaseDefinition {
    pub fn supports(&self, vector: TransmissionVector) -> bool {
        self.transmission_vectors.contains(&vector)
    }
}

/// Maximum fraction of route exposure that perfect Physiology practice can
/// prevent. Every route retains residual risk. Wounds remain Surgery's domain;
/// blood protection is deliberately modest because cleaning and wound closure
/// are the stronger modeled controls.
pub fn maximum_preventable_fraction(vector: TransmissionVector) -> f32 {
    match vector {
        TransmissionVector::CloseContact => 0.90,
        TransmissionVector::FoodWater => 0.94,
        TransmissionVector::Vermin => 0.65,
        TransmissionVector::Wound => 0.0,
        TransmissionVector::Blood => 0.30,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExposureComponents {
    pub unavoidable: f32,
    pub preventable: f32,
}

/// Split a route's total dose into unavoidable/environmental exposure and the
/// party-behavior share that Physiology can mitigate. `affordance` represents
/// how much of the route's ordinary preventive behavior is physically
/// possible with the current space, sanitation, and supplies.
pub fn exposure_components(
    exposure: f32,
    vector: TransmissionVector,
    affordance: f32,
) -> ExposureComponents {
    let exposure = exposure.max(0.0);
    let preventable = exposure * maximum_preventable_fraction(vector) * affordance.clamp(0.0, 1.0);
    ExposureComponents {
        unavoidable: exposure - preventable,
        preventable,
    }
}

pub fn residual_exposure_with_affordance(
    exposure: f32,
    vector: TransmissionVector,
    physiology_check: f32,
    affordance: f32,
) -> f32 {
    let components = exposure_components(exposure, vector, affordance);
    let competence = (physiology_check.clamp(0.0, 5.0) / 5.0).powf(1.25);
    components.unavoidable + components.preventable * (1.0 - competence)
}

pub const BLOOD_BASIC_HANDLING_AFFORDANCE: f32 = 0.25;
pub const BLOOD_CLEAN_HANDLING_BONUS: f32 = 0.45;
pub const BLOOD_CLOSED_WOUND_BONUS: f32 = 0.30;

/// Circumstance cap for blood/caregiving prevention. Basic avoidance remains
/// possible without supplies, but clean handling and a protected wound must
/// come from real sanitation and wound-care state.
pub fn blood_caregiving_affordance(clean_handling: bool, cut_route: f32) -> f32 {
    (BLOOD_BASIC_HANDLING_AFFORDANCE
        + if clean_handling {
            BLOOD_CLEAN_HANDLING_BONUS
        } else {
            0.0
        }
        + BLOOD_CLOSED_WOUND_BONUS * (1.0 - cut_route.clamp(0.0, 1.0)))
    .clamp(0.0, 1.0)
}

pub fn residual_exposure(exposure: f32, vector: TransmissionVector, physiology_check: f32) -> f32 {
    residual_exposure_with_affordance(exposure, vector, physiology_check, 1.0)
}

/// Aggregate distinct practitioners whose pinned capability covers `minute`.
/// Duplicate spans for one practitioner use the strongest matching pinned
/// value, which makes adjacent join/rejoin and band-boundary records harmless.
pub fn historical_physiology_check_at(
    spans: impl IntoIterator<Item = (u64, u64, u64, f32)>,
    minute: u64,
) -> f32 {
    let mut contributors = std::collections::BTreeMap::<u64, f32>::new();
    for (contributor_id, start, end, check) in spans {
        if minute < start || minute > end {
            continue;
        }
        contributors
            .entry(contributor_id)
            .and_modify(|value| *value = (*value).max(check))
            .or_insert(check);
    }
    crate::capability::aggregate_bounded_party_check(contributors.into_values())
}

pub fn elapsed_presence_window(
    span_start: u64,
    span_end: Option<u64>,
    joint_now: u64,
    from: u64,
    to: u64,
) -> Option<(u64, u64)> {
    let start = span_start.max(from.saturating_add(1));
    let end = span_end.unwrap_or(joint_now).min(joint_now).min(to);
    (start <= end).then_some((start, end))
}

/// End of a recorded presence span for an interval plan. Explicitly
/// co-advancing participants may project to their shared horizon. A solo
/// participant may consume already-recorded overlap only through the peer's
/// current clock.
pub fn projected_presence_end(
    ended_at: Option<u64>,
    low_horizon: Option<u64>,
    high_horizon: Option<u64>,
    low_clock: u64,
    high_clock: u64,
) -> Option<u64> {
    ended_at.or_else(|| match (low_horizon, high_horizon) {
        (Some(low), Some(high)) => Some(low.min(high)),
        (Some(low), None) => Some(low.min(high_clock)),
        (None, Some(high)) => Some(high.min(low_clock)),
        (None, None) => None,
    })
}

/// Disease-agnostic infectiousness for close company. Contact risk begins
/// during incubation, peaks through the acute phases, and declines during
/// recovery. It does not depend on whether symptoms are publicly visible.
pub fn close_contact_infectiousness(episode: InfectionEpisode, at: u64) -> f32 {
    if at < episode.contracted_at {
        return 0.0;
    }
    let definition = definition(episode.disease_id);
    if !definition.supports(TransmissionVector::CloseContact) {
        return 0.0;
    }
    let age = at.saturating_sub(episode.contracted_at) as f32;
    let incubation = definition.incubation_minutes.max(1) as f32;
    if age < incubation {
        return 0.15 + 0.45 * age / incubation;
    }
    let acute_end =
        (definition.incubation_minutes + definition.rise_minutes + definition.peak_minutes) as f32;
    if age < acute_end {
        return 1.0;
    }
    let resolved = acute_end + definition.recovery_minutes.max(1) as f32;
    if age < resolved {
        return ((resolved - age) / definition.recovery_minutes.max(1) as f32).clamp(0.0, 1.0);
    }
    0.0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContactWindow {
    pub low_id: u64,
    pub high_id: u64,
    pub start: u64,
    pub end: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AcquisitionTimeline {
    pub proposals: std::collections::BTreeMap<u64, Vec<InfectionEpisode>>,
    pub work_units: u64,
}

pub fn insert_unique_bounded<K: Ord, V>(
    values: &mut std::collections::BTreeMap<K, V>,
    key: K,
    value: V,
    limit: usize,
) -> Result<bool, &'static str> {
    use std::collections::btree_map::Entry;
    if values.len() >= limit && !values.contains_key(&key) {
        return Err("Disease interval has too many raw presence spans");
    }
    Ok(match values.entry(key) {
        Entry::Vacant(entry) => {
            entry.insert(value);
            true
        }
        Entry::Occupied(_) => false,
    })
}

pub fn add_bounded_work(work: &mut u64, amount: u64, max: u64) -> Result<(), &'static str> {
    *work = work.saturating_add(amount);
    (*work <= max)
        .then_some(())
        .ok_or("Disease interval exceeds bounded exposure work")
}

/// Resolve already-assembled route candidates and contact transmission in one
/// absolute-minute timeline. Acquisitions at a minute are simultaneous and
/// become eligible contact sources only on the following minute.
pub fn resolve_acquisition_timeline(
    target_ids: &std::collections::BTreeSet<u64>,
    initial: &std::collections::BTreeMap<u64, Vec<InfectionEpisode>>,
    scheduled: impl IntoIterator<Item = InfectionEpisode>,
    windows: &[ContactWindow],
    immunity: &std::collections::BTreeMap<u64, f32>,
    from: u64,
    to: u64,
    initial_work: u64,
    max_work: u64,
    physiology_check_at: impl Fn(u64, u64) -> f32,
) -> Result<AcquisitionTimeline, &'static str> {
    if to <= from || initial_work > max_work {
        return (initial_work <= max_work)
            .then_some(AcquisitionTimeline {
                work_units: initial_work,
                ..Default::default()
            })
            .ok_or("Disease interval exceeds bounded exposure work");
    }
    let mut scheduled_by_minute = std::collections::BTreeMap::<u64, Vec<InfectionEpisode>>::new();
    let mut work_units = initial_work;
    for episode in scheduled {
        if target_ids.contains(&episode.character_id)
            && episode.contracted_at > from
            && episode.contracted_at <= to
        {
            work_units = work_units.saturating_add(1);
            if work_units > max_work {
                return Err("Disease interval exceeds bounded exposure work");
            }
            scheduled_by_minute
                .entry(episode.contracted_at)
                .or_default()
                .push(episode);
        }
    }
    let mut state = initial.clone();
    let mut result = AcquisitionTimeline {
        work_units,
        ..Default::default()
    };
    let first_scheduled = scheduled_by_minute.keys().next().copied();
    let first_contact = windows
        .iter()
        .filter_map(|window| {
            let start = window.start.max(from.saturating_add(1));
            (start <= window.end && start <= to).then_some(start)
        })
        .min();
    let Some(mut minute) = first_scheduled.into_iter().chain(first_contact).min() else {
        return Ok(result);
    };
    while minute <= to {
        let mut candidates = scheduled_by_minute.remove(&minute).unwrap_or_default();
        for window in windows {
            result.work_units = result.work_units.saturating_add(1);
            if result.work_units > max_work {
                return Err("Disease interval exceeds bounded exposure work");
            }
            if minute < window.start || minute > window.end {
                continue;
            }
            for (source_id, target_id) in [
                (window.low_id, window.high_id),
                (window.high_id, window.low_id),
            ] {
                if !target_ids.contains(&target_id) {
                    continue;
                }
                let target_immunity = immunity.get(&target_id).copied().unwrap_or(3.0);
                let target_episodes = state.get(&target_id).map_or(&[][..], Vec::as_slice);
                for source_episode in state.get(&source_id).into_iter().flatten() {
                    result.work_units = result.work_units.saturating_add(1);
                    if result.work_units > max_work {
                        return Err("Disease interval exceeds bounded exposure work");
                    }
                    if source_episode.contracted_at >= minute
                        || !definition(source_episode.disease_id)
                            .supports(TransmissionVector::CloseContact)
                        || has_unresolved_disease(
                            target_episodes,
                            source_episode.disease_id,
                            minute,
                            target_immunity,
                        )
                    {
                        continue;
                    }
                    let exposure = residual_exposure(
                        close_contact_infectiousness(*source_episode, minute) / 1_440.0,
                        TransmissionVector::CloseContact,
                        physiology_check_at(target_id, minute),
                    );
                    let prior = acquired_immunity(
                        target_episodes,
                        source_episode.disease_id,
                        minute,
                        target_immunity,
                    );
                    let seed =
                        contact_exposure_seed(target_id, source_id, source_episode.id, minute);
                    if acquisition_succeeds(
                        seed,
                        definition(source_episode.disease_id),
                        target_immunity,
                        prior,
                        exposure,
                    ) {
                        candidates.push(InfectionEpisode {
                            id: seed,
                            character_id: target_id,
                            disease_id: source_episode.disease_id,
                            contracted_at: minute,
                            ruleset_version: source_episode.ruleset_version,
                            phenotype_key_version: source_episode.phenotype_key_version,
                        });
                    }
                }
            }
        }
        candidates.sort_by_key(|episode| {
            (
                episode.character_id,
                episode.disease_id as u8,
                episode.contracted_at,
                episode.id,
            )
        });
        for candidate in candidates {
            let current = state.entry(candidate.character_id).or_default();
            let target_immunity = immunity
                .get(&candidate.character_id)
                .copied()
                .unwrap_or(3.0);
            if has_unresolved_disease(current, candidate.disease_id, minute, target_immunity) {
                continue;
            }
            current.push(candidate);
            current.sort_by_key(|episode| (episode.contracted_at, episode.id));
            result
                .proposals
                .entry(candidate.character_id)
                .or_default()
                .push(candidate);
        }
        let next_scheduled = scheduled_by_minute
            .range(minute.saturating_add(1)..)
            .next()
            .map(|(next, _)| *next);
        let next_contact = windows
            .iter()
            .filter_map(|window| {
                let next = minute.saturating_add(1).max(window.start);
                (next <= window.end && next <= to).then_some(next)
            })
            .min();
        let Some(next) = next_scheduled.into_iter().chain(next_contact).min() else {
            break;
        };
        if next <= minute {
            break;
        }
        minute = next;
    }
    Ok(result)
}

const DAY: u64 = 1_440;
const RESP: VitalImpairment = VitalImpairment {
    sanguine: 0.0,
    phlegmatic: 0.55,
    choleric: 0.0,
    melancholic: 0.0,
};
const GUT: VitalImpairment = VitalImpairment {
    sanguine: 0.10,
    phlegmatic: 0.0,
    choleric: 0.70,
    melancholic: 0.0,
};
const SEPTIC: VitalImpairment = VitalImpairment {
    sanguine: 0.45,
    phlegmatic: 0.0,
    choleric: 0.25,
    melancholic: 0.10,
};
const COUGH: &[Symptom] = &[
    Symptom::Coughing,
    Symptom::Sneezing,
    Symptom::Feverish,
    Symptom::Fatigued,
];
const FLUX: &[Symptom] = &[
    Symptom::BloodyStool,
    Symptom::Vomiting,
    Symptom::Feverish,
    Symptom::Fatigued,
];
const SPOTTED: &[Symptom] = &[Symptom::Feverish, Symptom::Rash, Symptom::Trembling];
const LOCKJAW: &[Symptom] = &[Symptom::Lockjaw, Symptom::Spasms, Symptom::Feverish];
const RASH: &[Symptom] = &[Symptom::Rash, Symptom::Feverish];
const POX: &[Symptom] = &[Symptom::Rash, Symptom::Feverish, Symptom::Fatigued];
const BUBOES: &[Symptom] = &[Symptom::Buboes, Symptom::Feverish, Symptom::Trembling];
const CONSUMPTION: &[Symptom] = &[Symptom::Coughing, Symptom::Fatigued, Symptom::Feverish];

pub const STARTER_DISEASES: [DiseaseDefinition; 8] = [
    d(
        DiseaseId::Influenza,
        "Catarrhal fever",
        "Spreads readily among close company.",
        DAY,
        2 * DAY,
        2 * DAY,
        4 * DAY,
        0.65,
        RESP,
        AttributeImpairment {
            endurance: 0.55,
            ..Z
        },
        COUGH,
        0.30,
        &[TransmissionVector::CloseContact],
        TransmissionVector::CloseContact,
    ),
    d(
        DiseaseId::Dysentery,
        "Bloody flux",
        "Often follows foul water or provisions.",
        DAY,
        DAY,
        3 * DAY,
        5 * DAY,
        0.55,
        GUT,
        AttributeImpairment {
            gut: 0.75,
            endurance: 0.25,
            ..Z
        },
        FLUX,
        0.15,
        &[TransmissionVector::FoodWater],
        TransmissionVector::FoodWater,
    ),
    d(
        DiseaseId::Typhus,
        "Spotted fever",
        "Spreads where people and vermin crowd together.",
        6 * DAY,
        3 * DAY,
        5 * DAY,
        7 * DAY,
        0.35,
        SEPTIC,
        AttributeImpairment {
            endurance: 0.55,
            intelligence: 0.25,
            ..Z
        },
        SPOTTED,
        0.55,
        &[TransmissionVector::CloseContact, TransmissionVector::Vermin],
        TransmissionVector::Vermin,
    ),
    d(
        DiseaseId::Tetanus,
        "Lockjaw",
        "May follow a befouled open wound.",
        4 * DAY,
        4 * DAY,
        7 * DAY,
        10 * DAY,
        0.12,
        VitalImpairment {
            phlegmatic: 0.75,
            melancholic: 0.2,
            ..VZ
        },
        AttributeImpairment {
            limb_agility: 0.8,
            instinct: 0.3,
            ..Z
        },
        LOCKJAW,
        0.05,
        &[TransmissionVector::Wound],
        TransmissionVector::Wound,
    ),
    d(
        DiseaseId::Erysipelas,
        "Erysipelas",
        "May follow an inflamed open wound.",
        2 * DAY,
        2 * DAY,
        3 * DAY,
        6 * DAY,
        0.25,
        SEPTIC,
        AttributeImpairment {
            endurance: 0.4,
            ..Z
        },
        RASH,
        0.15,
        &[TransmissionVector::Wound],
        TransmissionVector::Wound,
    ),
    d(
        DiseaseId::Smallpox,
        "Smallpox",
        "Spreads readily through close company and belongings.",
        8 * DAY,
        3 * DAY,
        7 * DAY,
        12 * DAY,
        0.45,
        VitalImpairment {
            sanguine: 0.25,
            phlegmatic: 0.2,
            choleric: 0.2,
            melancholic: 0.1,
        },
        AttributeImpairment {
            endurance: 0.65,
            ..Z
        },
        POX,
        0.90,
        &[TransmissionVector::CloseContact],
        TransmissionVector::CloseContact,
    ),
    d(
        DiseaseId::Plague,
        "Plague",
        "Spreads amid afflicted settlements, people, and vermin.",
        3 * DAY,
        2 * DAY,
        5 * DAY,
        8 * DAY,
        0.30,
        VitalImpairment {
            sanguine: 0.75,
            choleric: 0.25,
            ..VZ
        },
        AttributeImpairment {
            endurance: 0.8,
            instinct: 0.3,
            ..Z
        },
        BUBOES,
        0.50,
        &[
            TransmissionVector::CloseContact,
            TransmissionVector::Vermin,
            TransmissionVector::Blood,
        ],
        TransmissionVector::Vermin,
    ),
    d(
        DiseaseId::Consumption,
        "Consumption",
        "Long company with the afflicted is suspected.",
        20 * DAY,
        40 * DAY,
        80 * DAY,
        120 * DAY,
        0.18,
        VitalImpairment {
            phlegmatic: 0.65,
            choleric: 0.15,
            ..VZ
        },
        AttributeImpairment {
            endurance: 0.7,
            ..Z
        },
        CONSUMPTION,
        0.20,
        &[TransmissionVector::CloseContact],
        TransmissionVector::CloseContact,
    ),
];
const Z: AttributeImpairment = AttributeImpairment {
    endurance: 0.,
    immunity: 0.,
    gut: 0.,
    intelligence: 0.,
    instinct: 0.,
    limb_agility: 0.,
};
const VZ: VitalImpairment = VitalImpairment {
    sanguine: 0.,
    phlegmatic: 0.,
    choleric: 0.,
    melancholic: 0.,
};
const fn d(
    id: DiseaseId,
    period_name: &'static str,
    contagion: &'static str,
    incubation_minutes: u64,
    rise_minutes: u64,
    peak_minutes: u64,
    recovery_minutes: u64,
    base_acquisition: f32,
    peak_vitals: VitalImpairment,
    peak_attributes: AttributeImpairment,
    symptoms: &'static [Symptom],
    acquired_immunity: f32,
    transmission_vectors: &'static [TransmissionVector],
    primary_community_vector: TransmissionVector,
) -> DiseaseDefinition {
    DiseaseDefinition {
        id,
        period_name,
        contagion,
        incubation_minutes,
        rise_minutes,
        peak_minutes,
        recovery_minutes,
        base_acquisition,
        peak_vitals,
        peak_attributes,
        symptoms,
        acquired_immunity,
        transmission_vectors,
        primary_community_vector,
    }
}

pub fn definition(id: DiseaseId) -> &'static DiseaseDefinition {
    &STARTER_DISEASES[id as usize]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InfectionEpisode {
    pub id: u64,
    pub character_id: u64,
    pub disease_id: DiseaseId,
    pub contracted_at: u64,
    pub ruleset_version: u16,
    pub phenotype_key_version: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiseaseState {
    pub stage: DiseaseStage,
    pub severity: f32,
    pub progress: f32,
    pub symptoms: Vec<Symptom>,
    pub attributes: AttributeImpairment,
    pub vitals: VitalImpairment,
    pub terminal_failure: Option<TerminalFailure>,
}

fn fnv(bytes: impl IntoIterator<Item = u8>) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
pub fn severity_seed(e: InfectionEpisode) -> u64 {
    fnv(SEED_DOMAIN
        .iter()
        .copied()
        .chain(e.id.to_le_bytes())
        .chain(e.character_id.to_le_bytes())
        .chain([e.disease_id as u8])
        .chain(e.contracted_at.to_le_bytes()))
}
pub fn outbreak_exposure_seed(character_id: u64, outbreak_id: &str) -> u64 {
    fnv(b"adventuresim/disease/exposure/v1\0"
        .iter()
        .copied()
        .chain(character_id.to_le_bytes())
        .chain(outbreak_id.bytes()))
}

/// Contact candidates vary at minute granularity. FNV supplies stable domain
/// identity and this finalizer avalanches adjacent minute suffixes so the high
/// 53 bits consumed by acquisition rolls are not correlated.
pub fn contact_exposure_seed(
    target_id: u64,
    source_id: u64,
    source_episode_id: u64,
    minute: u64,
) -> u64 {
    let mut value = outbreak_exposure_seed(
        target_id,
        &format!("party:{source_id}:{source_episode_id}:{minute}"),
    );
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// True while an episode of the same disease remains unresolved at the
/// interval boundary. Outbreak acquisition uses this to make partitioned rest
/// equivalent to a single interval and avoid duplicate active infections.
pub fn has_unresolved_disease(
    episodes: &[InfectionEpisode],
    disease_id: DiseaseId,
    at: u64,
    immunity: f32,
) -> bool {
    episodes.iter().any(|episode| {
        episode.disease_id == disease_id
            && episode.contracted_at <= at
            && !matches!(
                evaluate(*episode, at, immunity).stage,
                DiseaseStage::Resolved
            )
    })
}

/// Terminal interval boundaries are inclusive. Side effects strictly after
/// `through` must not be committed when elapsed time is clipped by death.
pub fn infection_occurs_through(episode: InfectionEpisode, through: u64) -> bool {
    episode.contracted_at <= through
}
pub fn first_presence_exposure_minute(
    character_id: u64,
    outbreak_id: &str,
    from: u64,
    to: u64,
    intensity: f32,
    base_acquisition: f32,
    immunity: f32,
    prior_immunity: f32,
) -> Option<u64> {
    if to <= from {
        return None;
    }
    ((from + 1)..=to).find(|minute| {
        let key = format!("{outbreak_id}:{minute}");
        acquisition_succeeds(
            outbreak_exposure_seed(character_id, &key),
            &DiseaseDefinition {
                base_acquisition: base_acquisition / 1_440.0,
                ..*definition(DiseaseId::Influenza)
            },
            immunity,
            prior_immunity,
            intensity,
        )
    })
}

/// First exposure minute at which the character is eligible to contract this
/// disease. Eligibility and acquired immunity are evaluated at each candidate
/// minute so a long interval behaves like equivalent smaller updates when an
/// earlier episode resolves partway through it.
pub fn first_eligible_presence_exposure_minute(
    episodes: &[InfectionEpisode],
    disease_id: DiseaseId,
    character_id: u64,
    outbreak_id: &str,
    from: u64,
    to: u64,
    intensity: f32,
    base_acquisition: f32,
    immunity: f32,
) -> Option<u64> {
    if to <= from {
        return None;
    }
    ((from + 1)..=to).find(|minute| {
        if has_unresolved_disease(episodes, disease_id, *minute, immunity) {
            return false;
        }
        let prior_immunity = acquired_immunity(episodes, disease_id, *minute, immunity);
        let key = format!("{outbreak_id}:{minute}");
        acquisition_succeeds(
            outbreak_exposure_seed(character_id, &key),
            &DiseaseDefinition {
                base_acquisition: base_acquisition / 1_440.0,
                ..*definition(DiseaseId::Influenza)
            },
            immunity,
            prior_immunity,
            intensity,
        )
    })
}

#[allow(clippy::too_many_arguments)]
pub fn first_eligible_protected_presence_exposure_minute(
    episodes: &[InfectionEpisode],
    disease_id: DiseaseId,
    character_id: u64,
    exposure_id: &str,
    from: u64,
    to: u64,
    intensity: f32,
    base_acquisition: f32,
    immunity: f32,
    vector: TransmissionVector,
    physiology_check_at: impl Fn(u64) -> f32,
) -> Option<u64> {
    if to <= from || intensity <= 0.0 {
        return None;
    }
    ((from + 1)..=to).find(|minute| {
        if has_unresolved_disease(episodes, disease_id, *minute, immunity) {
            return false;
        }
        let prior_immunity = acquired_immunity(episodes, disease_id, *minute, immunity);
        let key = format!("{exposure_id}:{minute}");
        acquisition_succeeds(
            outbreak_exposure_seed(character_id, &key),
            &DiseaseDefinition {
                base_acquisition: base_acquisition / 1_440.0,
                ..*definition(disease_id)
            },
            immunity,
            prior_immunity,
            residual_exposure(intensity, vector, physiology_check_at(*minute)),
        )
    })
}

#[allow(clippy::too_many_arguments)]
pub fn eligible_protected_presence_exposure_attempt_minutes(
    episodes: &[InfectionEpisode],
    disease_id: DiseaseId,
    character_id: u64,
    exposure_id: &str,
    from: u64,
    to: u64,
    intensity: f32,
    base_acquisition: f32,
    immunity: f32,
    vector: TransmissionVector,
    max_attempts: usize,
    physiology_check_at: impl Fn(u64) -> f32,
) -> Result<Vec<u64>, &'static str> {
    if to <= from || intensity <= 0.0 {
        return Ok(Vec::new());
    }
    let mut attempts = Vec::new();
    for minute in (from + 1)..=to {
        if has_unresolved_disease(episodes, disease_id, minute, immunity) {
            continue;
        }
        let prior_immunity = acquired_immunity(episodes, disease_id, minute, immunity);
        let key = format!("{exposure_id}:{minute}");
        if acquisition_succeeds(
            outbreak_exposure_seed(character_id, &key),
            &DiseaseDefinition {
                base_acquisition: base_acquisition / 1_440.0,
                ..*definition(disease_id)
            },
            immunity,
            prior_immunity,
            residual_exposure(intensity, vector, physiology_check_at(minute)),
        ) {
            if attempts.len() >= max_attempts {
                return Err("Disease interval exceeds bounded acquisition candidates");
            }
            attempts.push(minute);
        }
    }
    Ok(attempts)
}

pub fn severity(e: InfectionEpisode, immunity: f32) -> f32 {
    let unit = (severity_seed(e) >> 11) as f64 / (1u64 << 53) as f64;
    let innate = (immunity / 5.0).clamp(0.0, 1.0);
    ((0.72 + unit as f32 * 0.56) * (1.0 - innate * 0.62)).max(if immunity <= 0.0 {
        1.85
    } else {
        0.20
    })
}
pub fn acquisition_succeeds(
    seed: u64,
    definition: &DiseaseDefinition,
    immunity: f32,
    prior_immunity: f32,
    exposure: f32,
) -> bool {
    let resistance = (immunity / 5.0).clamp(0.0, 1.0) * 0.72 + prior_immunity.clamp(0.0, 0.95);
    let chance =
        (definition.base_acquisition * exposure.max(0.0) * (1.0 - resistance)).clamp(0.0, 1.0);
    (seed >> 11) as f64 / ((1u64 << 53) as f64) < chance as f64
}

/// Absolute exposure minute at which an outbreak infects this character. Using
/// a threshold on cumulative continuous exposure makes one 24-hour update
/// identical to twenty-four one-hour updates.
pub fn exposure_threshold_minute(
    seed: u64,
    outbreak_start: u64,
    intensity: f32,
    base_acquisition: f32,
    immunity: f32,
    prior_immunity: f32,
) -> Option<u64> {
    let resistance = ((immunity / 5.0).clamp(0.0, 1.0) * 0.72 + prior_immunity).clamp(0.0, 0.98);
    let hazard = (intensity.max(0.0) * base_acquisition.max(0.0) * (1.0 - resistance)) / 1_440.0;
    if hazard <= 0.0 {
        return None;
    }
    let unit =
        (((seed >> 11) as f64 + 0.5) / (1u64 << 53) as f64).clamp(f64::EPSILON, 1.0 - f64::EPSILON);
    let minutes = (-unit.ln() / (hazard as f64)).ceil() as u64;
    Some(outbreak_start.saturating_add(minutes))
}

fn disease_age(e: InfectionEpisode, now: u64, _d: &DiseaseDefinition) -> f32 {
    now.saturating_sub(e.contracted_at) as f32
}

fn wall_minute_for_age(e: InfectionEpisode, target_age: f32) -> u64 {
    e.contracted_at.saturating_add(target_age.ceil() as u64)
}
fn curve(d: &DiseaseDefinition, age: f32) -> (DiseaseStage, f32) {
    let i = d.incubation_minutes as f32;
    let rise = d.rise_minutes as f32;
    let peak = d.peak_minutes as f32;
    let recovery = d.recovery_minutes as f32;
    if age < i {
        (DiseaseStage::Incubating, 0.0)
    } else if age < i + rise {
        (DiseaseStage::Early, (age - i) / rise)
    } else if age < i + rise + peak {
        (DiseaseStage::Established, 1.0)
    } else if age < i + rise + peak + recovery {
        (
            DiseaseStage::Convalescent,
            1.0 - (age - i - rise - peak) / recovery,
        )
    } else {
        (DiseaseStage::Resolved, 0.0)
    }
}

pub fn evaluate(e: InfectionEpisode, now: u64, immunity: f32) -> DiseaseState {
    let d = definition(e.disease_id);
    let age = disease_age(e, now, d);
    let (mut stage, progress) = curve(d, age);
    let sev = severity(e, immunity);
    let scale = progress * sev;
    let vitals = d.peak_vitals.scaled(scale);
    let terminal = vitals.terminal_failure();
    if terminal.is_some() {
        stage = DiseaseStage::Critical
    }
    let symptom_count = if progress <= 0.0 {
        0
    } else if progress < 0.45 {
        1
    } else {
        d.symptoms.len()
    };
    DiseaseState {
        stage,
        severity: sev,
        progress,
        symptoms: d.symptoms[..symptom_count].to_vec(),
        attributes: AttributeImpairment {
            endurance: d.peak_attributes.endurance * scale,
            immunity: d.peak_attributes.immunity * scale,
            gut: d.peak_attributes.gut * scale,
            intelligence: d.peak_attributes.intelligence * scale,
            instinct: d.peak_attributes.instinct * scale,
            limb_agility: d.peak_attributes.limb_agility * scale,
        },
        vitals,
        terminal_failure: terminal,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiseaseEventKind {
    SymptomOnset,
    Peak,
    Critical(TerminalFailure),
    Resolution,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiseaseEvent {
    pub minute: u64,
    pub infection_id: u64,
    pub kind: DiseaseEventKind,
}
pub fn interval_events(
    e: InfectionEpisode,
    from: u64,
    to: u64,
    immunity: f32,
) -> Vec<DiseaseEvent> {
    if to <= from {
        return vec![];
    }
    let d = definition(e.disease_id);
    let mut points = vec![
        (
            wall_minute_for_age(e, d.incubation_minutes as f32),
            DiseaseEventKind::SymptomOnset,
        ),
        (
            wall_minute_for_age(e, (d.incubation_minutes + d.rise_minutes) as f32),
            DiseaseEventKind::Peak,
        ),
    ];
    let end = wall_minute_for_age(
        e,
        (d.incubation_minutes + d.rise_minutes + d.peak_minutes + d.recovery_minutes) as f32,
    );
    points.push((end, DiseaseEventKind::Resolution));
    for (v, cause) in [
        (d.peak_vitals.phlegmatic, TerminalFailure::Respiratory),
        (d.peak_vitals.sanguine, TerminalFailure::Circulatory),
        (d.peak_vitals.choleric, TerminalFailure::Homeostatic),
        (d.peak_vitals.melancholic, TerminalFailure::Neurologic),
    ] {
        let s = severity(e, immunity);
        if v * s >= 1. {
            let fraction = 1. / (v * s);
            let minute = wall_minute_for_age(
                e,
                d.incubation_minutes as f32 + d.rise_minutes as f32 * fraction,
            );
            points.push((minute, DiseaseEventKind::Critical(cause)));
        }
    }
    points
        .into_iter()
        .filter(|(m, _)| *m > from && *m <= to)
        .map(|(minute, kind)| DiseaseEvent {
            minute,
            infection_id: e.id,
            kind,
        })
        .collect()
}

/// Every minute where the authored disease curve can change slope. Consumers
/// doing combined-state crossing searches must include these points even when
/// no standalone disease event occurs there.
pub fn structural_minutes(e: InfectionEpisode, from: u64, to: u64) -> Vec<u64> {
    let d = definition(e.disease_id);
    [
        d.incubation_minutes,
        d.incubation_minutes.saturating_add(d.rise_minutes),
        d.incubation_minutes
            .saturating_add(d.rise_minutes)
            .saturating_add(d.peak_minutes),
        d.incubation_minutes
            .saturating_add(d.rise_minutes)
            .saturating_add(d.peak_minutes)
            .saturating_add(d.recovery_minutes),
    ]
    .into_iter()
    .map(|age| wall_minute_for_age(e, age as f32))
    .filter(|minute| *minute >= from && *minute <= to)
    .collect()
}

pub fn combined_state(
    episodes: &[InfectionEpisode],
    now: u64,
    immunity: f32,
) -> (
    AttributeImpairment,
    VitalImpairment,
    Vec<Symptom>,
    Option<TerminalFailure>,
) {
    let mut a = AttributeImpairment::default();
    let mut v = VitalImpairment::default();
    let mut s = Vec::new();
    for e in episodes {
        let x = evaluate(*e, now, immunity);
        a.endurance += x.attributes.endurance;
        a.immunity += x.attributes.immunity;
        a.gut += x.attributes.gut;
        a.intelligence += x.attributes.intelligence;
        a.instinct += x.attributes.instinct;
        a.limb_agility += x.attributes.limb_agility;
        v.add(x.vitals);
        for symptom in x.symptoms {
            if !s.contains(&symptom) {
                s.push(symptom)
            }
        }
    }
    s.sort();
    let terminal = v.terminal_failure();
    (a, v, s, terminal)
}

/// Derives where a patient experiences each humour imbalance. The disease's
/// global vital impairment still determines terminal failure; this regional
/// projection exists so a patient can perceive *where* they feel unwell while
/// only an examined physician can distinguish *why*.
pub fn regional_vitals(
    episodes: &[InfectionEpisode],
    now: u64,
    immunity: f32,
) -> [VitalImpairment; 7] {
    let mut regions = [VitalImpairment::default(); 7];
    for episode in episodes {
        let state = evaluate(*episode, now, immunity);
        for (region, weight) in disease_region_weights(*episode) {
            regions[region.index()].add(state.vitals.scaled(weight));
        }
    }
    regions
}

/// Private functional-loss state used by the authoritative server. Disease
/// identity never reaches the browser; the key changes the relative meter
/// involvement for each episode rather than applying a scalar severity tweak.
pub fn private_meter_state(
    episode: InfectionEpisode,
    now: u64,
    immunity: f32,
    phenotype_secret: &[u8],
) -> MeterVector {
    let state = evaluate(episode, now, immunity);
    let phenotype = physiology::phenotype_multipliers(
        phenotype_secret,
        episode.phenotype_key_version,
        episode.character_id,
        episode.id,
    );
    let peak = disease_peak_meters(episode.disease_id);
    let mut meters = MeterVector::ZERO;
    for (meter, loss) in peak {
        meters.0[meter.index()] = (meters.0[meter.index()]
            + loss * state.progress * state.severity * phenotype[meter.index()])
        .clamp(-1.0, 2.0);
    }
    meters
}

pub fn disease_peak_meters(disease_id: DiseaseId) -> &'static [(Meter, f32)] {
    match disease_id {
        DiseaseId::Influenza => &[
            (Meter::Oxygenation, 0.55),
            (Meter::Temperature, 0.20),
            (Meter::Inflammation, 0.20),
        ][..],
        DiseaseId::Dysentery => &[
            (Meter::Hydration, 0.70),
            (Meter::RenalClearance, 0.30),
            (Meter::Perfusion, 0.10),
        ],
        DiseaseId::Typhus => &[
            (Meter::Temperature, 0.35),
            (Meter::Inflammation, 0.45),
            (Meter::Perfusion, 0.25),
            (Meter::Neurologic, 0.10),
        ],
        DiseaseId::Tetanus => &[
            (Meter::Neurologic, 0.75),
            (Meter::Oxygenation, 0.20),
            (Meter::TissueIntegrity, 0.10),
        ],
        DiseaseId::Erysipelas => &[
            (Meter::TissueIntegrity, 0.45),
            (Meter::Inflammation, 0.55),
            (Meter::Temperature, 0.20),
        ],
        DiseaseId::Smallpox => &[
            (Meter::TissueIntegrity, 0.40),
            (Meter::Inflammation, 0.45),
            (Meter::Hydration, 0.20),
            (Meter::Temperature, 0.30),
        ],
        DiseaseId::Plague => &[
            (Meter::Perfusion, 0.75),
            (Meter::Coagulation, 0.35),
            (Meter::Inflammation, 0.50),
        ],
        DiseaseId::Consumption => &[
            (Meter::Oxygenation, 0.65),
            (Meter::Nutrition, 0.25),
            (Meter::TissueIntegrity, 0.20),
        ],
    }
}

pub fn private_regional_meter_state(
    patient_id: u64,
    episodes: &[InfectionEpisode],
    now: u64,
    immunity: f32,
    phenotype_secret: &[u8],
    key_version: u16,
) -> [MeterVector; physiology::REGION_COUNT] {
    let baseline = physiology::baseline_meters(phenotype_secret, key_version, patient_id);
    let mut regions = [baseline; physiology::REGION_COUNT];
    for episode in episodes {
        let meters = private_meter_state(*episode, now, immunity, phenotype_secret);
        let weights = disease_region_weights(*episode);
        let total = weights
            .iter()
            .map(|(_, weight)| *weight)
            .sum::<f32>()
            .max(1.0);
        for (region, weight) in weights {
            regions[region.index()].add_bounded(meters.scaled(weight / total));
        }
    }
    regions
}

fn disease_region_weights(episode: InfectionEpisode) -> Vec<(BodyRegion, f32)> {
    use BodyRegion::{Abdomen, Chest, Head, LeftArm, LeftLeg, RightArm, RightLeg};
    match episode.disease_id {
        DiseaseId::Influenza => vec![(Chest, 1.0), (Head, 0.35)],
        DiseaseId::Dysentery => vec![(Abdomen, 1.0), (Chest, 0.25)],
        DiseaseId::Typhus => vec![(Chest, 0.8), (Head, 1.0), (Abdomen, 0.45)],
        DiseaseId::Tetanus => vec![
            (Chest, 1.0),
            (Head, 0.7),
            (LeftArm, 0.65),
            (RightArm, 0.65),
            (LeftLeg, 0.65),
            (RightLeg, 0.65),
        ],
        DiseaseId::Erysipelas => {
            let limb =
                [LeftArm, RightArm, LeftLeg, RightLeg][(severity_seed(episode) as usize) % 4];
            vec![(limb, 1.0), (Chest, 0.35)]
        }
        DiseaseId::Smallpox => vec![
            (Head, 1.0),
            (Chest, 0.8),
            (Abdomen, 0.6),
            (LeftArm, 0.45),
            (RightArm, 0.45),
            (LeftLeg, 0.45),
            (RightLeg, 0.45),
        ],
        DiseaseId::Plague => {
            let limb =
                [LeftArm, RightArm, LeftLeg, RightLeg][(severity_seed(episode) as usize) % 4];
            vec![(Chest, 1.0), (Abdomen, 0.75), (Head, 0.55), (limb, 0.8)]
        }
        DiseaseId::Consumption => vec![(Chest, 1.0), (Abdomen, 0.3)],
    }
}

/// Findings visible during an examination. Incidental complaints are derived
/// from the infection identity, so they remain stable without additional
/// per-character disease state.
pub fn observed_symptoms(episodes: &[InfectionEpisode], now: u64, immunity: f32) -> Vec<Symptom> {
    let (_, _, mut symptoms, _) = combined_state(episodes, now, immunity);
    let Some(seed_episode) = episodes
        .iter()
        .filter(|episode| evaluate(**episode, now, immunity).progress > 0.0)
        .min_by_key(|episode| episode.id)
    else {
        return symptoms;
    };
    const INCIDENTAL: [Symptom; 7] = [
        Symptom::Coughing,
        Symptom::Sneezing,
        Symptom::Feverish,
        Symptom::Fatigued,
        Symptom::Vomiting,
        Symptom::Rash,
        Symptom::Trembling,
    ];
    let seed = severity_seed(*seed_episode);
    for shift in [0, 11] {
        let finding = INCIDENTAL[((seed >> shift) as usize) % INCIDENTAL.len()];
        if !symptoms.contains(&finding) {
            symptoms.push(finding);
        }
    }
    symptoms.sort();
    symptoms
}

/// Earliest combined terminal failure in an interval. Structural boundaries
/// split every deterministic curve into monotonic spans; binary search prevents
/// two individually survivable infections from hiding a fatal combined peak.
pub fn first_combined_terminal(
    episodes: &[InfectionEpisode],
    from: u64,
    to: u64,
    immunity: f32,
) -> Option<(u64, TerminalFailure)> {
    if to <= from {
        return None;
    }
    let mut points = vec![from, to];
    for e in episodes {
        points.extend(
            interval_events(*e, from, to, immunity)
                .into_iter()
                .map(|event| event.minute),
        );
    }
    points.sort_unstable();
    points.dedup();
    for window in points.windows(2) {
        let lo = window[0];
        let hi = window[1];
        if let Some(cause) = combined_state(episodes, lo, immunity).3 {
            return Some((lo, cause));
        }
        if combined_state(episodes, hi, immunity).3.is_some() {
            let (mut left, mut right) = (lo, hi);
            while left + 1 < right {
                let mid = left + (right - left) / 2;
                if combined_state(episodes, mid, immunity).3.is_some() {
                    right = mid
                } else {
                    left = mid
                }
            }
            if let Some(cause) = combined_state(episodes, right, immunity).3 {
                return Some((right, cause));
            }
        }
    }
    None
}
pub fn acquired_immunity(
    episodes: &[InfectionEpisode],
    disease: DiseaseId,
    now: u64,
    immunity: f32,
) -> f32 {
    episodes
        .iter()
        .filter(|e| {
            e.disease_id == disease && evaluate(**e, now, immunity).stage == DiseaseStage::Resolved
        })
        .map(|_| definition(disease).acquired_immunity)
        .fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn e(id: u64, disease_id: DiseaseId) -> InfectionEpisode {
        InfectionEpisode {
            id,
            character_id: 7,
            disease_id,
            contracted_at: 100,
            ruleset_version: physiology::PHYSIOLOGY_RULESET_VERSION,
            phenotype_key_version: physiology::PHENOTYPE_KEY_VERSION,
        }
    }
    #[test]
    fn seed_is_domain_stable_and_uses_identity() {
        assert_eq!(
            severity_seed(e(1, DiseaseId::Influenza)),
            17281431899313043311
        );
        assert_ne!(
            severity_seed(e(1, DiseaseId::Influenza)),
            severity_seed(e(2, DiseaseId::Influenza))
        );
    }
    #[test]
    fn interval_reports_fatal_crossing_even_past_recovery() {
        let x = e(1, DiseaseId::Influenza);
        let events = interval_events(x, 0, 100 + 20 * DAY, 0.);
        assert!(events.iter().any(|x| matches!(
            x.kind,
            DiseaseEventKind::Critical(TerminalFailure::Respiratory)
        )));
        assert!(
            events
                .iter()
                .any(|x| x.kind == DiseaseEventKind::Resolution)
        );
    }

    #[test]
    fn structural_minutes_include_the_start_of_recovery() {
        let infection = e(1, DiseaseId::Influenza);
        let definition = definition(infection.disease_id);
        let recovery_start = wall_minute_for_age(
            infection,
            definition
                .incubation_minutes
                .saturating_add(definition.rise_minutes)
                .saturating_add(definition.peak_minutes) as f32,
        );
        assert!(structural_minutes(infection, 0, u64::MAX).contains(&recovery_start));
    }
    #[test]
    fn zero_immunity_can_make_mild_disease_fatal() {
        let x = e(4, DiseaseId::Influenza);
        assert!(evaluate(x, 100 + 4 * DAY, 0.0).terminal_failure.is_some());
        assert!(evaluate(x, 100 + 4 * DAY, 5.0).terminal_failure.is_none());
    }
    #[test]
    fn composition_deduplicates_symptoms_and_combines_vitals() {
        let a = e(1, DiseaseId::Influenza);
        let b = e(2, DiseaseId::Influenza);
        let (_, v, s, _) = combined_state(&[a, b], 100 + 4 * DAY, 3.0);
        assert!(v.phlegmatic > evaluate(a, 100 + 4 * DAY, 3.0).vitals.phlegmatic);
        assert_eq!(s.iter().filter(|x| **x == Symptom::Coughing).count(), 1);
    }
    #[test]
    fn regional_projection_localizes_complaints_without_new_episode_state() {
        let influenza = e(1, DiseaseId::Influenza);
        let regions = regional_vitals(&[influenza], 100 + 4 * DAY, 3.0);
        assert!(regions[BodyRegion::Chest.index()].phlegmatic > 0.0);
        assert!(regions[BodyRegion::Head.index()].phlegmatic > 0.0);
        assert_eq!(
            regions[BodyRegion::Abdomen.index()],
            VitalImpairment::default()
        );

        let erysipelas = e(2, DiseaseId::Erysipelas);
        assert_eq!(
            regional_vitals(&[erysipelas], 100 + 4 * DAY, 3.0),
            regional_vitals(&[erysipelas], 100 + 4 * DAY, 3.0)
        );
    }

    #[test]
    fn private_regional_state_includes_patient_baseline_without_infections() {
        let regions = private_regional_meter_state(73, &[], 10_000, 3.0, b"test key", 1);
        assert!(regions.iter().all(|region| *region == regions[0]));
        assert_ne!(regions[0], MeterVector::ZERO);
        assert_ne!(
            regions[0],
            private_regional_meter_state(74, &[], 10_000, 3.0, b"test key", 1)[0]
        );
    }
    #[test]
    fn incidental_findings_are_stable_and_non_distinctive() {
        let infection = e(1, DiseaseId::Influenza);
        let first = observed_symptoms(&[infection], 100 + 4 * DAY, 3.0);
        let second = observed_symptoms(&[infection], 100 + 4 * DAY, 3.0);
        assert_eq!(first, second);
        assert!(!first.contains(&Symptom::Buboes));
        assert!(!first.contains(&Symptom::Lockjaw));
    }

    #[test]
    fn visible_findings_are_forced_through_humours_and_regions() {
        assert_eq!(Symptom::Coughing.humour(), Humour::Phlegmatic);
        assert_eq!(Symptom::Vomiting.humour(), Humour::Choleric);
        assert_eq!(Symptom::Rash.humour(), Humour::Sanguine);
        assert_eq!(Symptom::Trembling.humour(), Humour::Melancholic);
        assert_eq!(
            Symptom::Coughing.observation_regions(),
            &[BodyRegion::Chest]
        );
        assert!(
            Symptom::Rash
                .observation_regions()
                .contains(&BodyRegion::LeftArm)
        );
    }

    #[test]
    fn continuous_exposure_is_chunk_invariant() {
        let at = exposure_threshold_minute(42, 100, 0.8, 0.65, 2.0, 0.0).unwrap();
        let whole = at > 100 && at <= 100 + 30 * DAY;
        let chunks = (0..30).any(|day| at > 100 + day * DAY && at <= 100 + (day + 1) * DAY);
        assert_eq!(whole, chunks);
    }
    #[test]
    fn presence_exposure_handles_late_arrival_reentry_and_chunking() {
        let whole = first_presence_exposure_minute(4, "x", 10_000, 30_000, 1.0, 0.65, 0.0, 0.0);
        let chunks = (10_000..30_000).step_by(500).find_map(|from| {
            first_presence_exposure_minute(
                4,
                "x",
                from,
                (from + 500).min(30_000),
                1.0,
                0.65,
                0.0,
                0.0,
            )
        });
        assert_eq!(whole, chunks);
        let reentry = first_presence_exposure_minute(
            9,
            "reentry",
            20_000,
            21_000,
            1_000_000.0,
            0.65,
            0.0,
            0.0,
        );
        assert!(
            reentry.is_some(),
            "late re-entry receives a fresh presence interval"
        );
    }
    #[test]
    fn attribute_impairment_declines_and_recovers_without_mutating_baseline() {
        let x = e(77, DiseaseId::Dysentery);
        let baseline = 3.0;
        let peak = evaluate(x, 100 + 3 * DAY, 2.0).attributes.gut;
        let resolved = evaluate(x, 100 + 30 * DAY, 2.0).attributes.gut;
        assert!(peak > 0.0);
        assert_eq!(resolved, 0.0);
        assert_eq!(baseline, 3.0);
    }
    #[test]
    fn combined_subcritical_infections_can_cross_terminal_boundary() {
        let a = e(1, DiseaseId::Influenza);
        let b = e(2, DiseaseId::Influenza);
        let c = e(3, DiseaseId::Influenza);
        let d = e(4, DiseaseId::Influenza);
        assert!(evaluate(a, 100 + 4 * DAY, 3.0).terminal_failure.is_none());
        let at = first_combined_terminal(&[a, b, c, d], 100, 100 + 8 * DAY, 3.0);
        assert!(at.is_some());
    }

    #[test]
    fn clipped_interval_commits_the_terminal_boundary_but_not_later_infections() {
        let mut at_boundary = e(10, DiseaseId::Typhus);
        at_boundary.contracted_at = 500;
        let mut after_boundary = e(11, DiseaseId::Plague);
        after_boundary.contracted_at = 501;
        assert!(infection_occurs_through(at_boundary, 500));
        assert!(!infection_occurs_through(after_boundary, 500));
    }

    #[test]
    fn unresolved_disease_blocks_chunked_reinfection_until_resolution() {
        let episode = e(12, DiseaseId::Influenza);
        assert!(has_unresolved_disease(
            &[episode],
            DiseaseId::Influenza,
            episode.contracted_at + DAY,
            3.0,
        ));
        assert!(!has_unresolved_disease(
            &[episode],
            DiseaseId::Influenza,
            episode.contracted_at + 60 * DAY,
            3.0,
        ));
    }

    #[test]
    fn future_episode_is_not_unresolved_before_contraction() {
        let mut future = e(13, DiseaseId::Influenza);
        future.contracted_at = 10 * DAY;
        assert!(!has_unresolved_disease(
            &[future],
            DiseaseId::Influenza,
            DAY,
            3.0,
        ));
    }

    #[test]
    fn eligible_exposure_is_chunk_invariant_across_resolution() {
        let episode = e(14, DiseaseId::Influenza);
        let from = episode.contracted_at + DAY;
        let to = episode.contracted_at + 60 * DAY;
        let whole = first_eligible_presence_exposure_minute(
            &[episode],
            DiseaseId::Influenza,
            episode.character_id,
            "continuous",
            from,
            to,
            1_000_000.0,
            definition(DiseaseId::Influenza).base_acquisition,
            3.0,
        );
        let chunked = (from..to).step_by(DAY as usize).find_map(|chunk_from| {
            first_eligible_presence_exposure_minute(
                &[episode],
                DiseaseId::Influenza,
                episode.character_id,
                "continuous",
                chunk_from,
                chunk_from.saturating_add(DAY).min(to),
                1_000_000.0,
                definition(DiseaseId::Influenza).base_acquisition,
                3.0,
            )
        });
        assert!(whole.is_some());
        assert_eq!(whole, chunked);
    }

    #[test]
    fn physiology_prevention_is_route_ordered_bounded_and_monotonic() {
        let exposure = 1.0;
        for vector in [
            TransmissionVector::FoodWater,
            TransmissionVector::CloseContact,
            TransmissionVector::Vermin,
            TransmissionVector::Blood,
        ] {
            assert_eq!(residual_exposure(exposure, vector, 0.0), exposure);
            assert!(residual_exposure(exposure, vector, 5.0) > 0.0);
            assert!(
                residual_exposure(exposure, vector, 5.0) < residual_exposure(exposure, vector, 2.5)
            );
        }
        assert!(
            residual_exposure(exposure, TransmissionVector::FoodWater, 5.0)
                < residual_exposure(exposure, TransmissionVector::CloseContact, 5.0)
        );
        assert!(
            residual_exposure(exposure, TransmissionVector::CloseContact, 5.0)
                < residual_exposure(exposure, TransmissionVector::Vermin, 5.0)
        );
        assert!(
            residual_exposure(exposure, TransmissionVector::Vermin, 5.0)
                < residual_exposure(exposure, TransmissionVector::Blood, 5.0)
        );
        assert_eq!(
            residual_exposure(exposure, TransmissionVector::Wound, 5.0),
            exposure
        );
    }

    #[test]
    fn prevention_separates_unavoidable_dose_and_caps_missing_affordances() {
        let fully_possible = exposure_components(1.0, TransmissionVector::FoodWater, 1.0);
        let impossible = exposure_components(1.0, TransmissionVector::FoodWater, 0.0);
        assert!(fully_possible.preventable > fully_possible.unavoidable);
        assert_eq!(impossible.preventable, 0.0);
        assert_eq!(impossible.unavoidable, 1.0);

        let missing_supplies_open_wound = blood_caregiving_affordance(false, 1.0);
        let clean_handling_protected_wound = blood_caregiving_affordance(true, 0.18);
        assert_eq!(missing_supplies_open_wound, BLOOD_BASIC_HANDLING_AFFORDANCE);
        assert!(clean_handling_protected_wound > missing_supplies_open_wound);
        assert!(
            residual_exposure_with_affordance(
                1.0,
                TransmissionVector::Blood,
                5.0,
                missing_supplies_open_wound,
            ) > residual_exposure_with_affordance(
                1.0,
                TransmissionVector::Blood,
                5.0,
                clean_handling_protected_wound,
            )
        );
        assert_eq!(
            residual_exposure_with_affordance(
                0.0,
                TransmissionVector::Blood,
                5.0,
                clean_handling_protected_wound,
            ),
            0.0,
            "successful physical cleaning removes the dose before Physiology"
        );
    }

    #[test]
    fn strategic_prevention_fixture_reduces_contraction_and_secondary_attack_by_level() {
        let simulated_rate = |vector, physiology_check| {
            let definition = match vector {
                TransmissionVector::FoodWater => definition(DiseaseId::Dysentery),
                TransmissionVector::CloseContact => definition(DiseaseId::Influenza),
                _ => unreachable!("fixture covers authored high-preventability routes"),
            };
            let exposure = residual_exposure(0.20, vector, physiology_check);
            (0..20_000_u64)
                .map(|character_id| outbreak_exposure_seed(character_id, "prevention-fixture"))
                .filter(|seed| acquisition_succeeds(*seed, definition, 2.5, 0.0, exposure))
                .count()
        };
        for vector in [
            TransmissionVector::FoodWater,
            TransmissionVector::CloseContact,
        ] {
            let rates = [0.0, 1.0, 3.0, 5.0].map(|level| simulated_rate(vector, level));
            assert!(rates.windows(2).all(|pair| pair[1] < pair[0]), "{rates:?}");
            assert!(rates[3] * 2 < rates[0], "{rates:?}");
            assert!(rates[3] > 0, "mastery must retain residual risk");
        }
    }

    #[test]
    fn shared_sleep_is_close_contact_presence_and_is_chunk_invariant() {
        let overnight_start = 18 * 60;
        let overnight_end = overnight_start + 12 * 60;
        let physician_span = [(9, overnight_start + 1, overnight_end, 5.0)];
        let evaluate = |from, to| {
            first_eligible_protected_presence_exposure_minute(
                &[],
                DiseaseId::Influenza,
                7,
                "shared-sleep:source-8",
                from,
                to,
                10_000.0,
                definition(DiseaseId::Influenza).base_acquisition,
                2.5,
                TransmissionVector::CloseContact,
                |minute| historical_physiology_check_at(physician_span, minute),
            )
        };
        let whole = evaluate(overnight_start, overnight_end);
        let chunked = (overnight_start..overnight_end)
            .step_by(60)
            .find_map(|from| evaluate(from, (from + 60).min(overnight_end)));
        assert_eq!(whole, chunked);
    }

    #[test]
    fn every_community_route_is_authored_and_supported() {
        for definition in STARTER_DISEASES {
            assert!(definition.supports(definition.primary_community_vector));
        }
        assert_eq!(
            definition(DiseaseId::Dysentery).primary_community_vector,
            TransmissionVector::FoodWater
        );
        assert_eq!(
            definition(DiseaseId::Plague).primary_community_vector,
            TransmissionVector::Vermin
        );
    }

    #[test]
    fn maximum_physician_protection_retains_acquisition_risk() {
        let definition = definition(DiseaseId::Influenza);
        let residual = residual_exposure(1.0, TransmissionVector::CloseContact, 5.0);
        assert!(residual > 0.0);
        assert!(
            (0..10_000).any(|seed| { acquisition_succeeds(seed, definition, 0.0, 0.0, residual) })
        );
    }

    #[test]
    fn historical_party_support_respects_join_leave_and_band_boundaries() {
        let spans = [
            (7, 100, 199, 1.0),
            (8, 100, 199, 4.0),
            (7, 200, 300, 3.0),
            (8, 200, 300, 4.0),
            (9, 225, 250, 2.0),
        ];
        assert_eq!(historical_physiology_check_at(spans, 99), 0.0);
        let before_training = historical_physiology_check_at(spans, 150);
        let after_training = historical_physiology_check_at(spans, 210);
        let with_supporter = historical_physiology_check_at(spans, 240);
        assert!(after_training > before_training);
        assert!(with_supporter > after_training);
        assert_eq!(historical_physiology_check_at(spans, 301), 0.0);
    }

    #[test]
    fn duplicate_coverage_does_not_count_one_supporter_twice() {
        let duplicate = historical_physiology_check_at([(7, 0, 100, 4.0), (7, 50, 150, 4.0)], 75);
        assert!((duplicate - 4.0).abs() < 0.001);
    }

    #[test]
    fn changing_historical_protection_is_chunk_invariant() {
        let spans = [(7, 101, 500, 1.0), (7, 501, 1_000, 5.0)];
        let evaluate = |from, to| {
            first_eligible_protected_presence_exposure_minute(
                &[],
                DiseaseId::Influenza,
                77,
                "protected",
                from,
                to,
                10_000.0,
                definition(DiseaseId::Influenza).base_acquisition,
                0.0,
                TransmissionVector::CloseContact,
                |minute| historical_physiology_check_at(spans, minute),
            )
        };
        let whole = evaluate(100, 1_000);
        let chunked = (100..1_000)
            .step_by(100)
            .find_map(|from| evaluate(from, (from + 100).min(1_000)));
        assert!(whole.is_some());
        assert_eq!(whole, chunked);
    }

    #[test]
    fn presence_window_never_advances_beyond_the_lagging_source_clock() {
        assert_eq!(
            elapsed_presence_window(100, None, 250, 150, 500),
            Some((151, 250))
        );
        assert_eq!(elapsed_presence_window(100, None, 150, 150, 500), None);
        assert_eq!(
            elapsed_presence_window(100, Some(220), 300, 150, 500),
            Some((151, 220))
        );
    }

    #[test]
    fn close_contact_infectiousness_precedes_symptoms_and_resolves() {
        let episode = e(20, DiseaseId::Influenza);
        let definition = definition(episode.disease_id);
        assert!(close_contact_infectiousness(episode, episode.contracted_at) > 0.0);
        assert_eq!(
            close_contact_infectiousness(
                episode,
                episode.contracted_at + definition.incubation_minutes + definition.rise_minutes
            ),
            1.0
        );
        assert_eq!(
            close_contact_infectiousness(
                episode,
                episode.contracted_at
                    + definition.incubation_minutes
                    + definition.rise_minutes
                    + definition.peak_minutes
                    + definition.recovery_minutes
            ),
            0.0
        );
        assert_eq!(
            close_contact_infectiousness(e(21, DiseaseId::Dysentery), 100),
            0.0
        );
    }

    fn timeline_episode(id: u64, character_id: u64, contracted_at: u64) -> InfectionEpisode {
        InfectionEpisode {
            id,
            character_id,
            disease_id: DiseaseId::Influenza,
            contracted_at,
            ruleset_version: physiology::PHYSIOLOGY_RULESET_VERSION,
            phenotype_key_version: physiology::PHENOTYPE_KEY_VERSION,
        }
    }

    #[test]
    fn chronological_contact_chain_is_order_independent_and_chunk_invariant() {
        let targets = [1, 2, 3].into_iter().collect();
        let initial = [(1, vec![timeline_episode(11, 1, 0)])]
            .into_iter()
            .collect();
        let immunity = [(1, 0.0), (2, 0.0), (3, 0.0)].into_iter().collect();
        let windows = [
            ContactWindow {
                low_id: 1,
                high_id: 2,
                start: 1,
                end: 30_000,
            },
            ContactWindow {
                low_id: 2,
                high_id: 3,
                start: 1,
                end: 30_000,
            },
        ];
        let whole = resolve_acquisition_timeline(
            &targets,
            &initial,
            [],
            &windows,
            &immunity,
            0,
            30_000,
            0,
            2_000_000,
            |_, _| 0.0,
        )
        .unwrap();
        let b_at = whole.proposals[&2][0].contracted_at;
        let c_at = whole.proposals[&3][0].contracted_at;
        assert!(
            c_at > b_at,
            "new sources become eligible on the next minute"
        );

        let reversed = resolve_acquisition_timeline(
            &targets,
            &initial,
            [],
            &[windows[1], windows[0]],
            &immunity,
            0,
            30_000,
            0,
            2_000_000,
            |_, _| 0.0,
        )
        .unwrap();
        assert_eq!(whole.proposals, reversed.proposals);

        let first = resolve_acquisition_timeline(
            &targets,
            &initial,
            [],
            &windows,
            &immunity,
            0,
            b_at,
            0,
            2_000_000,
            |_, _| 0.0,
        )
        .unwrap();
        let mut split_initial = initial.clone();
        for (id, episodes) in &first.proposals {
            split_initial.entry(*id).or_default().extend(episodes);
        }
        let second = resolve_acquisition_timeline(
            &targets,
            &split_initial,
            [],
            &windows,
            &immunity,
            b_at,
            30_000,
            first.work_units,
            2_000_000,
            |_, _| 0.0,
        )
        .unwrap();
        let mut split = first.proposals;
        for (id, episodes) in second.proposals {
            split.entry(id).or_default().extend(episodes);
        }
        assert_eq!(whole.proposals, split);
    }

    #[test]
    fn blood_scheduled_acquisition_becomes_contact_source_next_minute() {
        let targets = [2, 3].into_iter().collect();
        let immunity = [(2, 0.0), (3, 0.0)].into_iter().collect();
        let blood = timeline_episode(22, 2, 100);
        let result = resolve_acquisition_timeline(
            &targets,
            &Default::default(),
            [blood],
            &[ContactWindow {
                low_id: 2,
                high_id: 3,
                start: 1,
                end: 30_000,
            }],
            &immunity,
            0,
            30_000,
            0,
            1_000_000,
            |_, _| 0.0,
        )
        .unwrap();
        assert_eq!(result.proposals[&2], vec![blood]);
        assert!(result.proposals[&3][0].contracted_at > blood.contracted_at);
    }

    #[test]
    fn synchronized_prevention_and_clipped_timeline_are_executable() {
        let targets = [1, 2].into_iter().collect();
        let initial = [(1, vec![timeline_episode(31, 1, 0)])]
            .into_iter()
            .collect();
        let immunity = [(1, 0.0), (2, 0.0)].into_iter().collect();
        let window = [ContactWindow {
            low_id: 1,
            high_id: 2,
            start: 1,
            end: 30_000,
        }];
        let unprotected = resolve_acquisition_timeline(
            &targets,
            &initial,
            [],
            &window,
            &immunity,
            0,
            30_000,
            0,
            1_000_000,
            |_, _| 0.0,
        )
        .unwrap();
        let protected = resolve_acquisition_timeline(
            &targets,
            &initial,
            [],
            &window,
            &immunity,
            0,
            30_000,
            0,
            1_000_000,
            |_, _| 5.0,
        )
        .unwrap();
        assert!(
            protected
                .proposals
                .get(&2)
                .and_then(|episodes| episodes.first())
                .map(|episode| episode.contracted_at)
                .unwrap_or(u64::MAX)
                > unprotected.proposals[&2][0].contracted_at
        );

        let scheduled_after_death_clip = timeline_episode(32, 2, 501);
        let clipped = resolve_acquisition_timeline(
            &targets,
            &Default::default(),
            [scheduled_after_death_clip],
            &[],
            &immunity,
            0,
            500,
            0,
            1_000,
            |_, _| 0.0,
        )
        .unwrap();
        assert!(clipped.proposals.is_empty());
        assert_eq!(
            resolve_acquisition_timeline(
                &targets,
                &initial,
                [],
                &window,
                &immunity,
                0,
                30_000,
                0,
                1,
                |_, _| 0.0,
            ),
            Err("Disease interval exceeds bounded exposure work")
        );
    }

    #[test]
    fn solo_catchup_uses_only_already_elapsed_peer_overlap() {
        assert_eq!(
            projected_presence_end(None, Some(300), None, 100, 250),
            Some(250)
        );
        assert_eq!(
            projected_presence_end(None, Some(200), None, 100, 250),
            Some(200)
        );
        assert_eq!(
            projected_presence_end(None, Some(300), Some(280), 100, 250),
            Some(280)
        );
        assert_eq!(
            projected_presence_end(Some(175), Some(300), None, 100, 250),
            Some(175)
        );

        let target_horizon = 20_000;
        let peer_clock = 10_000;
        let overlap_end =
            projected_presence_end(None, Some(target_horizon), None, 100, peer_clock).unwrap();
        let targets = [2].into_iter().collect();
        let initial = [(1, vec![timeline_episode(41, 1, 0)])]
            .into_iter()
            .collect();
        let result = resolve_acquisition_timeline(
            &targets,
            &initial,
            [],
            &[ContactWindow {
                low_id: 1,
                high_id: 2,
                start: 101,
                end: overlap_end,
            }],
            &[(2, 0.0)].into_iter().collect(),
            100,
            target_horizon,
            0,
            1_000_000,
            |_, _| 0.0,
        )
        .unwrap();
        assert!(result.proposals[&2][0].contracted_at <= peer_clock);
    }

    #[test]
    fn long_environmental_source_reattempts_after_resolution_whole_or_split() {
        let initial_episode = timeline_episode(51, 7, 0);
        let initial = [(7, vec![initial_episode])].into_iter().collect();
        let immunity = [(7, 0.0)].into_iter().collect();
        let targets = [7].into_iter().collect();
        let resolution = (1..60 * DAY)
            .find(|minute| evaluate(initial_episode, *minute, 0.0).stage == DiseaseStage::Resolved)
            .unwrap();
        let split_at = resolution.saturating_sub(1);
        let to = resolution + 30 * DAY;
        let scheduled = |from, to| {
            eligible_protected_presence_exposure_attempt_minutes(
                &[initial_episode],
                DiseaseId::Influenza,
                7,
                "long-running-outbreak",
                from,
                to,
                1_000.0,
                definition(DiseaseId::Influenza).base_acquisition,
                0.0,
                TransmissionVector::CloseContact,
                100_000,
                |_| 0.0,
            )
            .unwrap()
            .into_iter()
            .map(|at| InfectionEpisode {
                id: outbreak_exposure_seed(7, &format!("long-running-outbreak:{at}")),
                character_id: 7,
                disease_id: DiseaseId::Influenza,
                contracted_at: at,
                ruleset_version: physiology::PHYSIOLOGY_RULESET_VERSION,
                phenotype_key_version: physiology::PHENOTYPE_KEY_VERSION,
            })
            .collect::<Vec<_>>()
        };
        let whole = resolve_acquisition_timeline(
            &targets,
            &initial,
            scheduled(0, to),
            &[],
            &immunity,
            0,
            to,
            0,
            1_000_000,
            |_, _| 0.0,
        )
        .unwrap();
        assert!(whole.proposals[&7][0].contracted_at >= resolution);

        let first = resolve_acquisition_timeline(
            &targets,
            &initial,
            scheduled(0, split_at),
            &[],
            &immunity,
            0,
            split_at,
            0,
            1_000_000,
            |_, _| 0.0,
        )
        .unwrap();
        let mut split_initial = initial;
        for (id, episodes) in &first.proposals {
            split_initial.entry(*id).or_default().extend(episodes);
        }
        let second = resolve_acquisition_timeline(
            &targets,
            &split_initial,
            scheduled(split_at, to),
            &[],
            &immunity,
            split_at,
            to,
            first.work_units,
            1_000_000,
            |_, _| 0.0,
        )
        .unwrap();
        let mut split = first.proposals;
        for (id, episodes) in second.proposals {
            split.entry(id).or_default().extend(episodes);
        }
        assert_eq!(whole.proposals, split);
    }

    #[test]
    fn raw_presence_span_and_checkpoint_work_caps_fail_closed() {
        let mut spans = std::collections::BTreeMap::new();
        assert_eq!(insert_unique_bounded(&mut spans, 1, "a", 2), Ok(true));
        assert_eq!(
            insert_unique_bounded(&mut spans, 1, "duplicate", 2),
            Ok(false)
        );
        assert_eq!(insert_unique_bounded(&mut spans, 2, "b", 2), Ok(true));
        assert_eq!(
            insert_unique_bounded(&mut spans, 3, "excess", 2),
            Err("Disease interval has too many raw presence spans")
        );

        let mut work = 0;
        assert_eq!(add_bounded_work(&mut work, 4, 5), Ok(()));
        assert_eq!(
            add_bounded_work(&mut work, 2, 5),
            Err("Disease interval exceeds bounded exposure work")
        );
    }
}
