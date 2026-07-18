//! Server-side medical privacy boundary.

use adventuresim_core::disease::{
    self, DiseaseId, DiseaseStage, InfectionEpisode, MEDICINE_VITALS_THRESHOLD, Symptom,
};

use crate::spacetimedb::InfectionEpisodeRow;

#[derive(Clone, Debug, Default)]
pub struct MedicalPresentation {
    pub symptoms: Vec<&'static str>,
    pub vitals: Option<HumourVitals>,
    pub diagnoses: Vec<DiagnosisPresentation>,
}
#[derive(Clone, Copy, Debug)]
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
    pub stage: &'static str,
    pub treatable: bool,
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
fn stage_label(stage: DiseaseStage) -> &'static str {
    match stage {
        DiseaseStage::Incubating => "hidden",
        DiseaseStage::Early => "early",
        DiseaseStage::Established => "established",
        DiseaseStage::Critical => "critical",
        DiseaseStage::Convalescent => "recovering",
        DiseaseStage::Resolved => "resolved",
    }
}

pub fn sanitize(
    rows: &[InfectionEpisodeRow],
    target_minute: u64,
    target_immunity: f32,
    viewer_medicine: f32,
    blood_fraction: f32,
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
    let (_, disease_vitals, symptoms, _) =
        disease::combined_state(&episodes, target_minute, target_immunity);
    let symptoms = symptoms.into_iter().map(Symptom::period_label).collect();
    if viewer_medicine < MEDICINE_VITALS_THRESHOLD {
        return MedicalPresentation {
            symptoms,
            ..MedicalPresentation::default()
        };
    }
    let mut diagnoses = Vec::new();
    for episode in &episodes {
        let state = disease::evaluate(*episode, target_minute, target_immunity);
        if viewer_medicine >= state.diagnosis_dc
            && !matches!(
                state.stage,
                DiseaseStage::Incubating | DiseaseStage::Resolved
            )
        {
            let d = disease::definition(episode.disease_id);
            diagnoses.push(DiagnosisPresentation {
                infection_id: episode.id,
                period_name: d.period_name,
                contagion: d.contagion,
                stage: stage_label(state.stage),
                treatable: episode.treated_at.is_none(),
            });
        }
    }
    MedicalPresentation {
        symptoms,
        vitals: Some(HumourVitals {
            sanguine: (blood_fraction.clamp(0.0, 1.0) - disease_vitals.sanguine).clamp(0.0, 1.0),
            phlegmatic: (1.0 - disease_vitals.phlegmatic).clamp(0.0, 1.0),
            choleric: (1.0 - disease_vitals.choleric).clamp(0.0, 1.0),
            melancholic: (1.0 - disease_vitals.melancholic).clamp(0.0, 1.0),
        }),
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
    #[test]
    fn below_two_gets_symptoms_but_no_vitals_or_identifiers() {
        let p = sanitize(&[row()], 4 * 1_440, 0.0, 1.99, 1.0);
        assert!(!p.symptoms.is_empty());
        assert!(p.vitals.is_none());
        assert!(p.diagnoses.is_empty());
    }
    #[test]
    fn exactly_two_gets_humours_but_only_diagnosable_disease() {
        let p = sanitize(&[row()], 4 * 1_440, 3.0, 2.0, 1.0);
        assert!(p.vitals.is_some());
        assert_eq!(p.diagnoses.len(), 1);
        assert_eq!(p.diagnoses[0].period_name, "Catarrhal fever");
    }
    #[test]
    fn high_dc_disease_name_remains_absent() {
        let mut r = row();
        r.disease_id = "consumption".into();
        let p = sanitize(&[r], 100 * 1_440, 3.0, 2.0, 1.0);
        assert!(p.diagnoses.is_empty());
    }
}
