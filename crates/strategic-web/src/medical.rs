//! Server-side medical privacy boundary.

use adventuresim_core::disease::{self, DiseaseId, InfectionEpisode, Symptom};

use crate::spacetimedb::{InfectionEpisodeRow, MedicalExaminationRow};

#[derive(Clone, Debug, Default)]
pub struct MedicalPresentation {
    pub unavailable: bool,
    pub obvious_cut: f32,
    pub symptoms: Vec<&'static str>,
    pub medications: Vec<&'static str>,
    pub findings: Vec<String>,
    pub examination_id: Option<u64>,
    pub examined_at: Option<u64>,
    /// Disease impairment by body region. Lay views collapse each row into
    /// `concealed_other`; an examination lets the physician see the channels.
    pub regional_humours: Option<[HumourVitals; 7]>,
    pub concealed_other: [f32; 7],
    pub possible_diagnoses: Vec<&'static str>,
    pub diagnoses: Vec<DiagnosisPresentation>,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct HumourVitals {
    pub sanguine: f32,
    pub phlegmatic: f32,
    pub choleric: f32,
    pub melancholic: f32,
}
#[derive(Clone, Debug)]
pub struct DiagnosisPresentation {
    pub infection_id: u64,
    pub period_name: &'static str,
    pub contagion: &'static str,
    pub stage: String,
    pub treatable: bool,
    pub treatment_gold: u32,
    pub treatment_minutes: u64,
}

fn parse(value: &str) -> Option<DiseaseId> {
    match value {
        "influenza" => Some(DiseaseId::Influenza),
        "dysentery" => Some(DiseaseId::Dysentery),
        "typhus" => Some(DiseaseId::Typhus),
        "tetanus" => Some(DiseaseId::Tetanus),
        "erysipelas" => Some(DiseaseId::Erysipelas),
        "smallpox" => Some(DiseaseId::Smallpox),
        "plague" => Some(DiseaseId::Plague),
        "consumption" => Some(DiseaseId::Consumption),
        _ => None,
    }
}
pub fn sanitize(
    rows: &[InfectionEpisodeRow],
    examination: Option<&MedicalExaminationRow>,
    target_minute: u64,
    target_immunity: f32,
    viewer_medicine: f32,
) -> MedicalPresentation {
    let episodes = rows
        .iter()
        .filter_map(|r| {
            Some(InfectionEpisode {
                id: r.id,
                character_id: r.character_id,
                disease_id: parse(&r.disease_id)?,
                contracted_at: r.contracted_at,
                treated_at: r.treated_at,
            })
        })
        .collect::<Vec<_>>();
    let (_, _, outward_symptoms, _) =
        disease::combined_state(&episodes, target_minute, target_immunity);
    let current_regional = disease::regional_vitals(&episodes, target_minute, target_immunity);
    let concealed_other = current_regional.map(|vitals| {
        (vitals.sanguine + vitals.phlegmatic + vitals.choleric + vitals.melancholic).clamp(0.0, 1.0)
    });
    let symptoms = outward_symptoms
        .into_iter()
        .map(Symptom::period_label)
        .collect();
    let mut medications = episodes
        .iter()
        .filter(|episode| episode.treated_at.is_some_and(|at| at <= target_minute))
        .filter(|episode| {
            !matches!(
                disease::evaluate(**episode, target_minute, target_immunity).stage,
                disease::DiseaseStage::Incubating | disease::DiseaseStage::Resolved
            )
        })
        .map(|episode| disease::definition(episode.disease_id).period_name)
        .collect::<Vec<_>>();
    medications.sort_unstable();
    medications.dedup();
    let Some(examination) = examination else {
        return MedicalPresentation {
            symptoms,
            medications,
            concealed_other,
            ..MedicalPresentation::default()
        };
    };
    let mut diagnoses = Vec::new();
    for ((infection_id, disease_id), stage) in examination
        .confirmed_infection_ids
        .iter()
        .zip(&examination.confirmed_disease_ids)
        .zip(&examination.confirmed_stages)
    {
        if let Some(id) = parse(disease_id) {
            let d = disease::definition(id);
            diagnoses.push(DiagnosisPresentation {
                infection_id: *infection_id,
                period_name: d.period_name,
                contagion: d.contagion,
                stage: stage.clone(),
                treatment_gold: disease::treatment_gold(id),
                treatment_minutes: disease::treatment_minutes(id),
                treatable: rows
                    .iter()
                    .find(|row| row.id == *infection_id)
                    .is_some_and(|row| {
                        let Some(disease_id) = parse(&row.disease_id) else {
                            return false;
                        };
                        let state = disease::evaluate(
                            InfectionEpisode {
                                id: row.id,
                                character_id: row.character_id,
                                disease_id,
                                contracted_at: row.contracted_at,
                                treated_at: row.treated_at,
                            },
                            target_minute,
                            target_immunity,
                        );
                        row.treated_at.is_none()
                            && !matches!(
                                state.stage,
                                disease::DiseaseStage::Incubating | disease::DiseaseStage::Resolved
                            )
                    }),
            });
        }
    }
    let possible_diagnoses = examination
        .possible_disease_ids
        .iter()
        .filter_map(|id| parse(id).map(|id| disease::definition(id).period_name))
        .collect();
    let reveals_humours =
        examination.reveals_vitals && viewer_medicine >= disease::MEDICINE_VITALS_THRESHOLD;
    let regional_humours = reveals_humours.then(|| {
        disease::regional_vitals(&episodes, examination.examined_at, target_immunity).map(
            |vitals| HumourVitals {
                sanguine: vitals.sanguine,
                phlegmatic: vitals.phlegmatic,
                choleric: vitals.choleric,
                melancholic: vitals.melancholic,
            },
        )
    });
    MedicalPresentation {
        unavailable: false,
        obvious_cut: 0.0,
        symptoms,
        medications,
        findings: examination.findings.clone(),
        examination_id: Some(examination.id),
        examined_at: Some(examination.examined_at),
        regional_humours,
        concealed_other,
        possible_diagnoses,
        diagnoses,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row() -> InfectionEpisodeRow {
        InfectionEpisodeRow {
            id: 91,
            character_id: 4,
            disease_id: "influenza".into(),
            contracted_at: 0,
            treated_at: None,
        }
    }
    fn examination() -> MedicalExaminationRow {
        MedicalExaminationRow {
            id: 7,
            doctor_id: 3,
            target_id: 4,
            examined_at: 4 * 1_440,
            findings: vec!["coughing".into(), "fatigued".into()],
            reveals_vitals: true,
            sanguine: 1.0,
            phlegmatic: 0.6,
            choleric: 1.0,
            melancholic: 1.0,
            possible_disease_ids: Vec::new(),
            confirmed_infection_ids: vec![91],
            confirmed_disease_ids: vec!["influenza".into()],
            confirmed_stages: vec!["established".into()],
        }
    }
    #[test]
    fn unexamined_patient_has_signs_but_no_vitals_or_identifiers() {
        let p = sanitize(&[row()], None, 4 * 1_440, 0.0, 0.0);
        assert!(!p.symptoms.is_empty());
        assert!(p.regional_humours.is_none());
        assert!(p.concealed_other.iter().any(|value| *value > 0.0));
        assert!(p.diagnoses.is_empty());
    }
    #[test]
    fn completed_examination_reveals_its_snapshot() {
        let exam = examination();
        let p = sanitize(&[row()], Some(&exam), 4 * 1_440, 3.0, 2.0);
        assert!(p.regional_humours.is_some());
        assert!(p.regional_humours.unwrap()[4].phlegmatic > 0.0);
        assert_eq!(p.diagnoses.len(), 1);
        assert_eq!(p.diagnoses[0].period_name, "Catarrhal fever");
        assert_eq!(p.diagnoses[0].treatment_gold, 2);
        assert_eq!(p.diagnoses[0].treatment_minutes, 30);
        assert_eq!(p.examined_at, Some(4 * 1_440));
    }
    #[test]
    fn low_medicine_examination_keeps_vitals_hidden() {
        let mut exam = examination();
        exam.reveals_vitals = false;
        let p = sanitize(&[row()], Some(&exam), 4 * 1_440, 3.0, 1.99);
        assert!(p.regional_humours.is_none());
        assert_eq!(p.examined_at, Some(4 * 1_440));
    }
    #[test]
    fn uncertain_examination_exposes_only_period_differential() {
        let mut exam = examination();
        exam.confirmed_infection_ids.clear();
        exam.confirmed_disease_ids.clear();
        exam.confirmed_stages.clear();
        exam.possible_disease_ids = vec!["influenza".into(), "consumption".into()];
        let p = sanitize(&[row()], Some(&exam), 4 * 1_440, 3.0, 2.0);
        assert!(p.diagnoses.is_empty());
        assert_eq!(p.possible_diagnoses, ["Catarrhal fever", "Consumption"]);
    }
    #[test]
    fn active_treatment_is_public_without_revealing_other_diagnoses() {
        let mut treated = row();
        treated.treated_at = Some(3 * 1_440);
        let p = sanitize(&[treated.clone()], None, 4 * 1_440, 3.0, 0.0);
        assert_eq!(p.medications, ["Catarrhal fever"]);
        assert!(p.diagnoses.is_empty());

        let resolved = sanitize(&[treated], None, 20 * 1_440, 3.0, 0.0);
        assert!(resolved.medications.is_empty());
    }
}
