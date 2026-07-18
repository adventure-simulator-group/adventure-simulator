//! Deterministic, framework-neutral strategic disease simulation.
//!
//! An infection is completely described by its identity, associations and two
//! character-local timestamps. Everything else in this module is derived.

use serde::{Deserialize, Serialize};

pub const DISEASE_RULESET_VERSION: u16 = 1;
pub const MEDICINE_VITALS_THRESHOLD: f32 = 2.0;
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
    pub innate_detection_dc: f32,
    pub base_acquisition: f32,
    pub peak_vitals: VitalImpairment,
    pub peak_attributes: AttributeImpairment,
    pub symptoms: &'static [Symptom],
    pub acquired_immunity: f32,
    pub wound_borne: bool,
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
        2.0,
        0.65,
        RESP,
        AttributeImpairment {
            endurance: 0.55,
            ..Z
        },
        COUGH,
        0.30,
        false,
    ),
    d(
        DiseaseId::Dysentery,
        "Bloody flux",
        "Often follows foul water or provisions.",
        DAY,
        DAY,
        3 * DAY,
        5 * DAY,
        2.0,
        0.55,
        GUT,
        AttributeImpairment {
            gut: 0.75,
            endurance: 0.25,
            ..Z
        },
        FLUX,
        0.15,
        false,
    ),
    d(
        DiseaseId::Typhus,
        "Spotted fever",
        "Spreads where people and vermin crowd together.",
        6 * DAY,
        3 * DAY,
        5 * DAY,
        7 * DAY,
        2.8,
        0.35,
        SEPTIC,
        AttributeImpairment {
            endurance: 0.55,
            intelligence: 0.25,
            ..Z
        },
        SPOTTED,
        0.55,
        false,
    ),
    d(
        DiseaseId::Tetanus,
        "Lockjaw",
        "May follow a befouled open wound.",
        4 * DAY,
        4 * DAY,
        7 * DAY,
        10 * DAY,
        3.2,
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
        true,
    ),
    d(
        DiseaseId::Erysipelas,
        "Erysipelas",
        "May follow an inflamed open wound.",
        2 * DAY,
        2 * DAY,
        3 * DAY,
        6 * DAY,
        2.3,
        0.25,
        SEPTIC,
        AttributeImpairment {
            endurance: 0.4,
            ..Z
        },
        RASH,
        0.15,
        true,
    ),
    d(
        DiseaseId::Smallpox,
        "Smallpox",
        "Spreads readily through close company and belongings.",
        8 * DAY,
        3 * DAY,
        7 * DAY,
        12 * DAY,
        2.4,
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
        false,
    ),
    d(
        DiseaseId::Plague,
        "Plague",
        "Spreads amid afflicted settlements, people, and vermin.",
        3 * DAY,
        2 * DAY,
        5 * DAY,
        8 * DAY,
        3.0,
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
        false,
    ),
    d(
        DiseaseId::Consumption,
        "Consumption",
        "Long company with the afflicted is suspected.",
        20 * DAY,
        40 * DAY,
        80 * DAY,
        120 * DAY,
        3.4,
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
        false,
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
    innate_detection_dc: f32,
    base_acquisition: f32,
    peak_vitals: VitalImpairment,
    peak_attributes: AttributeImpairment,
    symptoms: &'static [Symptom],
    acquired_immunity: f32,
    wound_borne: bool,
) -> DiseaseDefinition {
    DiseaseDefinition {
        id,
        period_name,
        contagion,
        incubation_minutes,
        rise_minutes,
        peak_minutes,
        recovery_minutes,
        innate_detection_dc,
        base_acquisition,
        peak_vitals,
        peak_attributes,
        symptoms,
        acquired_immunity,
        wound_borne,
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
    pub treated_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiseaseState {
    pub stage: DiseaseStage,
    pub severity: f32,
    pub progress: f32,
    pub diagnosis_dc: f32,
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
    let age = now.saturating_sub(e.contracted_at) as f32;
    if let Some(t) = e.treated_at.filter(|t| *t <= now) {
        let before = t.saturating_sub(e.contracted_at) as f32;
        before + (age - before) * 1.5
    } else {
        age
    }
}

fn wall_minute_for_age(e: InfectionEpisode, target_age: f32) -> u64 {
    let untreated = e.contracted_at.saturating_add(target_age.ceil() as u64);
    let Some(treated_at) = e.treated_at else {
        return untreated;
    };
    let age_at_treatment = treated_at.saturating_sub(e.contracted_at) as f32;
    if target_age <= age_at_treatment {
        untreated
    } else {
        treated_at.saturating_add(((target_age - age_at_treatment) / 1.5).ceil() as u64)
    }
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
    let mitigation = e.treated_at.filter(|t| *t <= now).map_or(1.0, |t| {
        1.0 - 0.28 * (now.saturating_sub(t) as f32 / (2.0 * DAY as f32)).clamp(0.0, 1.0)
    });
    let scale = progress * sev * mitigation;
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
    let dc = (d.innate_detection_dc
        + match stage {
            DiseaseStage::Incubating => 2.0,
            DiseaseStage::Early => 1.0 - progress,
            DiseaseStage::Established | DiseaseStage::Critical => 0.0,
            DiseaseStage::Convalescent => 0.5,
            DiseaseStage::Resolved => 2.0,
        })
    .max(2.0);
    DiseaseState {
        stage,
        severity: sev,
        progress,
        diagnosis_dc: dc,
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
        if let Some(t) = e.treated_at.filter(|t| *t > from && *t < to) {
            points.push(t);
            points.push(t.saturating_add(2 * DAY).min(to));
        }
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
            treated_at: None,
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
    fn treatment_is_continuous_and_accelerates_recovery() {
        let mut x = e(9, DiseaseId::Dysentery);
        let t = 100 + 2 * DAY;
        let before = evaluate(x, t, 3.0).progress;
        x.treated_at = Some(t);
        assert_eq!(evaluate(x, t, 3.0).progress, before);
        let untreated = evaluate(e(9, DiseaseId::Dysentery), t, 3.0).vitals.choleric;
        assert_eq!(evaluate(x, t, 3.0).vitals.choleric, untreated);
        assert!(
            evaluate(x, t + 5 * DAY, 3.0).progress
                < evaluate(e(9, DiseaseId::Dysentery), t + 5 * DAY, 3.0).progress
        );
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
    fn diagnosis_has_floor_and_gets_easier() {
        let x = e(1, DiseaseId::Influenza);
        assert!(evaluate(x, 100, 3.0).diagnosis_dc > evaluate(x, 100 + 4 * DAY, 3.0).diagnosis_dc);
        assert!(evaluate(x, 100 + 4 * DAY, 3.0).diagnosis_dc >= 2.0);
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
}
