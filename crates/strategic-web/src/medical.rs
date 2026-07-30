//! Server-side observer-authorized Physiology chart presentation.
//!
//! Inputs are already quantized projection rows. This module deliberately has
//! no infection row, private meter, phenotype, diagnosis or recommendation
//! type available to serialize.

use crate::spacetimedb::{BackendPhysiologyAdministration, BackendPhysiologyChart};
use adventuresim_core::{
    disease::{DiseaseId, definition},
    physiology::{BodyRegion, Humour},
};

#[derive(Clone, Debug, Default)]
pub struct MedicalPresentation {
    pub unavailable: bool,
    pub regional_humours: Option<[HumourVitals; 7]>,
    pub concealed_other: [f32; 7],
    pub readings: Vec<ChartReadingPresentation>,
    pub gaps: Vec<ChartGapPresentation>,
    pub administrations: Vec<AdministrationPresentation>,
    pub active_administrations: Vec<AdministrationPresentation>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HumourVitals {
    pub sanguine: f32,
    pub phlegmatic: f32,
    pub choleric: f32,
    pub melancholic: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartReadingPresentation {
    pub minute: u64,
    pub physiology_band: u8,
    pub observation_minutes: u64,
    pub humour_deviations_bps: [[i16; 4]; 7],
    pub possible_diseases: Vec<DiseaseLikelihoodPresentation>,
    pub known_interventions: Vec<String>,
    pub confidence_bps: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiseaseLikelihoodPresentation {
    pub disease_id: String,
    pub label: String,
    pub likelihood_bps: u16,
    /// Observer-safe, disease-definition-derived examples for the differential
    /// tooltip. These never inspect the patient's private infection state.
    pub typical_effects: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartGapPresentation {
    pub from: u64,
    pub to: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdministrationPresentation {
    pub id: u64,
    pub preparation_id: String,
    pub display_name: String,
    pub profile_version: u16,
    pub route: String,
    pub amount_milliunits: u32,
    pub region: Option<String>,
    pub administered_at: u64,
    pub stopped_at: Option<u64>,
}

pub fn sanitize(
    rows: &[BackendPhysiologyChart],
    administrations: &[BackendPhysiologyAdministration],
    current_minute: u64,
) -> MedicalPresentation {
    let mut readings = rows
        .iter()
        .filter(|row| row.gap_from.is_none() && row.gap_to.is_none())
        .filter_map(|row| {
            let sanguine: [i16; 7] = row.sanguine_bps.clone().try_into().ok()?;
            let phlegmatic: [i16; 7] = row.phlegmatic_bps.clone().try_into().ok()?;
            let choleric: [i16; 7] = row.choleric_bps.clone().try_into().ok()?;
            let melancholic: [i16; 7] = row.melancholic_bps.clone().try_into().ok()?;
            Some(ChartReadingPresentation {
                minute: row.observed_at,
                physiology_band: row.physiology_band,
                observation_minutes: row.observation_minutes,
                humour_deviations_bps: std::array::from_fn(|region| {
                    [
                        sanguine[region],
                        phlegmatic[region],
                        choleric[region],
                        melancholic[region],
                    ]
                }),
                possible_diseases: row
                    .possible_diseases
                    .iter()
                    .map(|candidate| DiseaseLikelihoodPresentation {
                        disease_id: candidate.disease_id.clone(),
                        label: candidate.label.clone(),
                        likelihood_bps: candidate.likelihood_bps.min(10_000),
                        typical_effects: typical_disease_effects(&candidate.disease_id),
                    })
                    .collect(),
                known_interventions: row.known_interventions.clone(),
                confidence_bps: row.confidence_bps,
            })
        })
        .collect::<Vec<_>>();
    readings.sort_by_key(|reading| reading.minute);
    let mut gaps = rows
        .iter()
        .filter_map(|row| {
            Some(ChartGapPresentation {
                from: row.gap_from?,
                to: row.gap_to?,
            })
        })
        .collect::<Vec<_>>();
    gaps.sort_by_key(|gap| (gap.from, gap.to));
    gaps.dedup();

    let latest = readings.last();
    let regional_humours = latest.map(|reading| {
        reading.humour_deviations_bps.map(|values| HumourVitals {
            sanguine: values[0] as f32 / 10_000.0,
            phlegmatic: values[1] as f32 / 10_000.0,
            choleric: values[2] as f32 / 10_000.0,
            melancholic: values[3] as f32 / 10_000.0,
        })
    });
    let concealed_other = if regional_humours.is_some() {
        [0.0; 7]
    } else {
        let aggregate = latest.map_or(0.0, |reading| {
            reading.humour_deviations_bps[0]
                .iter()
                .map(|value| value.unsigned_abs() as f32 / 10_000.0)
                .sum::<f32>()
                .clamp(0.0, 1.0)
        });
        [aggregate; 7]
    };
    let administrations = administrations
        .iter()
        .map(|row| AdministrationPresentation {
            id: row.id,
            preparation_id: row.preparation_id.clone(),
            display_name: adventuresim_core::item_catalog::definition(&row.preparation_id)
                .map_or_else(
                    || {
                        let mut readable = row.preparation_id.replace('_', " ");
                        if let Some(first) = readable.get_mut(0..1) {
                            first.make_ascii_uppercase();
                        }
                        readable
                    },
                    |definition| definition.display_name.clone(),
                ),
            profile_version: row.profile_version,
            route: row.route.clone(),
            amount_milliunits: row.amount_milliunits,
            region: row.region.clone(),
            administered_at: row.administered_at,
            stopped_at: row.stopped_at,
        })
        .collect::<Vec<_>>();
    let active_administrations = administrations
        .iter()
        .filter(|row| {
            row.stopped_at.is_none()
                && adventuresim_core::physiology::intervention_profile(
                    &row.preparation_id,
                    row.profile_version,
                )
                .is_some_and(|profile| {
                    current_minute >= row.administered_at
                        && current_minute
                            < row.administered_at.saturating_add(profile.duration_minutes)
                })
        })
        .cloned()
        .collect();
    MedicalPresentation {
        regional_humours,
        concealed_other,
        readings,
        gaps,
        administrations,
        active_administrations,
        unavailable: false,
    }
}

fn disease_id_from_public_key(key: &str) -> Option<DiseaseId> {
    Some(match key {
        "influenza" => DiseaseId::Influenza,
        "dysentery" => DiseaseId::Dysentery,
        "typhus" => DiseaseId::Typhus,
        "tetanus" => DiseaseId::Tetanus,
        "erysipelas" => DiseaseId::Erysipelas,
        "smallpox" => DiseaseId::Smallpox,
        "plague" => DiseaseId::Plague,
        "consumption" => DiseaseId::Consumption,
        "mahrdruck" => DiseaseId::Mahrdruck,
        "nachzehrer_wasting" => DiseaseId::NachzehrerWasting,
        "bilwisschuss" => DiseaseId::Bilwisschuss,
        "kobeldunst" => DiseaseId::Kobeldunst,
        _ => return None,
    })
}

fn typical_disease_effects(public_disease_key: &str) -> Vec<String> {
    let Some(disease_id) = disease_id_from_public_key(public_disease_key) else {
        return Vec::new();
    };
    let mut focal_effects = [[0.0_f32; 4]; 7];
    let mut whole_body_effects = [0.0_f32; 4];
    for symptom in definition(disease_id).symptoms {
        let regions = symptom.observation_regions();
        // Broad visible findings should read as a systemic signature rather
        // than seven arbitrary limb entries. The observer still sees only the
        // public disease definition, never the patient's infection state.
        if regions.len() >= 4 {
            whole_body_effects[symptom.humour().index()] += symptom.humour_deviation();
        } else {
            for region in regions {
                focal_effects[region.index()][symptom.humour().index()] +=
                    symptom.humour_deviation();
            }
        }
    }

    let mut ranked = BodyRegion::ALL
        .into_iter()
        .flat_map(|region| {
            Humour::ALL.into_iter().filter_map(move |humour| {
                let weight = focal_effects[region.index()][humour.index()];
                (weight > 0.0).then_some((weight, Some(region), humour))
            })
        })
        .collect::<Vec<_>>();
    ranked.extend(Humour::ALL.into_iter().filter_map(|humour| {
        let weight = whole_body_effects[humour.index()];
        (weight > 0.0).then_some((weight, None, humour))
    }));
    ranked.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| a.1.is_none().cmp(&b.1.is_none()))
            .then_with(|| {
                a.1.map_or(usize::MAX, BodyRegion::index)
                    .cmp(&b.1.map_or(usize::MAX, BodyRegion::index))
            })
            .then_with(|| a.2.index().cmp(&b.2.index()))
    });
    ranked
        .into_iter()
        .take(5)
        .map(|(_, region, humour)| {
            let region = match region {
                Some(BodyRegion::LeftArm) => "left arm",
                Some(BodyRegion::RightArm) => "right arm",
                Some(BodyRegion::LeftLeg) => "left leg",
                Some(BodyRegion::RightLeg) => "right leg",
                Some(BodyRegion::Chest) => "chest",
                Some(BodyRegion::Abdomen) => "stomach",
                Some(BodyRegion::Head) => "head",
                None => "whole body",
            };
            let humour = match humour {
                Humour::Sanguine => "blood",
                Humour::Phlegmatic => "phlegm",
                Humour::Choleric => "yellow bile",
                Humour::Melancholic => "black bile",
            };
            format!("▲ {region} {humour}")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chart_contains_only_quantized_observations_and_explicit_gaps() {
        let rows = vec![
            BackendPhysiologyChart {
                id: "reading".into(),
                observer_id: 1,
                patient_id: 2,
                observed_at: 100,
                physiology_band: 3,
                observation_minutes: 100,
                sanguine_bps: vec![-1_200; 7],
                phlegmatic_bps: vec![2_300; 7],
                choleric_bps: vec![3_400; 7],
                melancholic_bps: vec![4_500; 7],
                possible_diseases: vec![crate::spacetimedb::BackendPhysiologyDifferential {
                    disease_id: "influenza".into(),
                    label: "Catarrhal fever".into(),
                    likelihood_bps: 7_500,
                }],
                known_interventions: vec!["cooling_willow_draught v1 (Oral)".into()],
                confidence_bps: 7_000,
                gap_from: None,
                gap_to: None,
            },
            BackendPhysiologyChart {
                id: "gap".into(),
                observer_id: 1,
                patient_id: 2,
                observed_at: 200,
                physiology_band: 3,
                observation_minutes: 0,
                sanguine_bps: Vec::new(),
                phlegmatic_bps: Vec::new(),
                choleric_bps: Vec::new(),
                melancholic_bps: Vec::new(),
                possible_diseases: Vec::new(),
                known_interventions: Vec::new(),
                confidence_bps: 0,
                gap_from: Some(150),
                gap_to: Some(200),
            },
        ];
        let presentation = sanitize(&rows, &[], 200);
        assert_eq!(presentation.readings.len(), 1);
        assert_eq!(presentation.gaps.len(), 1);
        let regions = presentation.regional_humours.expect("regional readings");
        assert_eq!(regions[4].sanguine, -0.12);
        assert_eq!(regions[0].phlegmatic, 0.23);
        assert_eq!(
            presentation.readings[0].possible_diseases[0].label,
            "Catarrhal fever"
        );
        assert_eq!(
            presentation.readings[0].possible_diseases[0].typical_effects[0],
            "▲ chest phlegm"
        );
        let encoded = format!("{presentation:?}");
        for forbidden in ["infection_id", "phenotype", "private_meter", "diagnosis"] {
            assert!(!encoded.contains(forbidden));
        }
    }

    #[test]
    fn administration_history_retains_boundaries_and_excludes_stopped_or_expired_courses() {
        let administrations = vec![
            BackendPhysiologyAdministration {
                id: 1,
                patient_id: 2,
                preparation_id: "first_course".into(),
                profile_version: 1,
                route: "oral".into(),
                amount_milliunits: 750,
                region: None,
                administered_at: 100,
                stopped_at: Some(200),
            },
            BackendPhysiologyAdministration {
                id: 2,
                patient_id: 2,
                preparation_id: "oral_rehydration_draught".into(),
                profile_version: 1,
                route: "oral".into(),
                amount_milliunits: 1_000,
                region: None,
                administered_at: 300,
                stopped_at: None,
            },
            BackendPhysiologyAdministration {
                id: 3,
                patient_id: 2,
                preparation_id: "cooling_willow_draught".into(),
                profile_version: 1,
                route: "oral".into(),
                amount_milliunits: 1_000,
                region: None,
                administered_at: 100,
                stopped_at: None,
            },
        ];
        let presentation = sanitize(&[], &administrations, 500);
        assert_eq!(presentation.administrations.len(), 3);
        assert_eq!(presentation.administrations[0].administered_at, 100);
        assert_eq!(presentation.administrations[0].stopped_at, Some(200));
        assert_eq!(presentation.active_administrations.len(), 1);
        assert_eq!(
            presentation.active_administrations[0].preparation_id,
            "oral_rehydration_draught"
        );
        assert_eq!(
            presentation.active_administrations[0].display_name,
            "Oral rehydration draught"
        );

        let expired = sanitize(&[], &administrations, 1_000);
        assert!(expired.active_administrations.is_empty());
        assert_eq!(expired.administrations.len(), 3);
    }

    #[test]
    fn public_disease_effects_preserve_focal_signatures_and_collapse_broad_findings() {
        let influenza = typical_disease_effects("influenza");
        assert_eq!(influenza[0], "▲ chest phlegm");
        assert!(influenza.contains(&"▲ head phlegm".to_owned()));
        assert!(influenza.contains(&"▲ whole body yellow bile".to_owned()));
        assert!(influenza.contains(&"▲ whole body black bile".to_owned()));
        assert!(!influenza.iter().any(|effect| effect.contains("arm")));

        let smallpox = typical_disease_effects("smallpox");
        assert!(smallpox.contains(&"▲ whole body blood".to_owned()));
        assert!(smallpox.contains(&"▲ whole body yellow bile".to_owned()));
        assert!(smallpox.contains(&"▲ whole body black bile".to_owned()));
        assert!(smallpox.iter().all(|effect| effect.contains("whole body")));

        for disease_key in [
            "influenza",
            "dysentery",
            "typhus",
            "tetanus",
            "erysipelas",
            "smallpox",
            "plague",
            "consumption",
        ] {
            let effects = typical_disease_effects(disease_key);
            assert!(!effects.is_empty(), "{disease_key}");
            assert!(effects.len() <= 5, "{disease_key}: {effects:?}");
        }
        assert!(typical_disease_effects("unknown").is_empty());
    }
}
