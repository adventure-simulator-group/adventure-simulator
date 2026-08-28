//! Durable strategic disease facts and authoritative treatment.

use std::collections::{BTreeMap, BTreeSet};

use adventuresim_core::disease::{
    self, DiseaseEventKind, DiseaseId, InfectionEpisode, TerminalFailure, TransmissionVector,
};
use adventuresim_core::physiology::{self, BodyRegion, InterventionRoute};
use adventuresim_core::strategic_time::MINUTES_PER_DAY;
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::capability::{character_capability, character_capability__view};
use crate::character::{character as _, character__view, character_attributes__view};
use crate::local_problem::local_problem_authority;
use crate::social::{
    PhysiologyPresenceSpan, physiology_presence_span, physiology_presence_span__view,
};
use crate::time::character_time__view;
use crate::{
    character_attributes, character_skills, character_time,
    item::{inventory_item, item},
    strategic::{settlement, strategic_gateway_authority__view},
};

/// The complete per-character disease state. This table is deliberately
/// private: strategic-web derives a viewer-specific presentation instead of
/// forwarding these rows to browsers.
#[derive(Clone, Debug)]
#[table(accessor = infection_episode)]
pub struct InfectionEpisodeRow {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub disease_id: String,
    pub contracted_at: u64,
    pub ruleset_version: u16,
    pub phenotype_key_version: u16,
}

/// Runtime-generated and immutable for the lifetime of this pre-launch
/// database. Changing the version requires recreating the disposable database,
/// so historical phenotypes fail closed instead of silently drifting.
#[derive(Clone, Debug)]
#[table(accessor = physiology_key_material)]
pub struct PhysiologyKeyMaterial {
    #[primary_key]
    pub id: u8,
    pub version: u16,
    pub key: Vec<u8>,
}

/// Public world fact, not private medical information. Overlap with a
/// character's exact local clock is evaluated continuously and deterministically.
#[derive(Clone, Debug)]
#[table(accessor = settlement_outbreak, public)]
pub struct SettlementOutbreak {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub settlement_id: String,
    pub disease_id: String,
    pub start_minute: u64,
    pub end_minute: u64,
    pub intensity: f32,
}

/// Narrow durable provenance for committed cuts. No tactical tick state crosses
/// this boundary.
#[derive(Clone, Debug)]
#[table(accessor = committed_cut)]
pub struct CommittedCut {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub committed_at: u64,
    pub severity: f32,
    pub surgery_check: f32,
}

/// Delivery dedupe is explicitly separate from infection state and contains no
/// undiagnosed disease identity.
#[derive(Clone, Debug)]
#[table(accessor = disease_notice)]
pub struct DiseaseNotice {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    pub minute: u64,
    pub kind: String,
    pub message: String,
}

/// Public agent knowledge distilled from symptom notices. It deliberately
/// carries no infection ID, disease identity, differential, or private vitals.
#[derive(Clone, Debug)]
#[table(accessor = character_illness_status, public)]
pub struct CharacterIllnessStatus {
    #[primary_key]
    pub character_id: u64,
    pub symptomatic: bool,
    pub critical: bool,
    pub updated_at_minute: u64,
}

/// Durable administration/start-stop history. Effects are selected solely by
/// the versioned preparation profile, never by a disease key.
#[derive(Clone, Debug)]
#[table(
    accessor = physiology_administration,
    index(accessor = administration_patient_id, btree(columns = [patient_id])),
)]
pub struct PhysiologyAdministration {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub patient_id: u64,
    pub preparation_id: String,
    pub profile_version: u16,
    pub route: InterventionRoute,
    pub dose_milliunits: u32,
    pub region: Option<BodyRegion>,
    pub administered_at: u64,
    pub stopped_at: Option<u64>,
    pub sensitivity_bps: i16,
    pub adverse_bps: u16,
    pub ruleset_version: u16,
    pub phenotype_key_version: u16,
}

/// Ephemeral observer-safe row derived from bounded causal history.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendPhysiologyDifferential {
    pub disease_id: String,
    pub label: String,
    pub likelihood_bps: u16,
}

/// Ephemeral observer-safe row derived from bounded causal history.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendPhysiologyChart {
    pub id: String,
    pub observer_id: u64,
    pub patient_id: u64,
    pub observed_at: u64,
    pub physiology_band: u8,
    pub observation_minutes: u64,
    pub sanguine_bps: Vec<i16>,
    pub phlegmatic_bps: Vec<i16>,
    pub choleric_bps: Vec<i16>,
    pub melancholic_bps: Vec<i16>,
    pub possible_diseases: Vec<BackendPhysiologyDifferential>,
    pub known_interventions: Vec<String>,
    pub confidence_bps: u16,
    pub gap_from: Option<u64>,
    pub gap_to: Option<u64>,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendPhysiologyAdministration {
    pub id: u64,
    pub patient_id: u64,
    pub preparation_id: String,
    pub profile_version: u16,
    pub route: InterventionRoute,
    pub dose_milliunits: u32,
    pub region: Option<BodyRegion>,
    pub administered_at: u64,
    pub stopped_at: Option<u64>,
}

#[view(accessor = backend_committed_cuts, public)]
pub fn backend_committed_cuts(ctx: &ViewContext) -> Vec<CommittedCut> {
    ctx.db
        .committed_cut()
        .character_id()
        .filter(0u64..)
        .collect()
}

fn is_strategic_gateway(ctx: &ViewContext) -> bool {
    ctx.db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|authority| authority.identity == ctx.sender())
}

const PHYSIOLOGY_CHART_MAX_RANGE_MINUTES: u64 = 30 * MINUTES_PER_DAY;
const PHYSIOLOGY_CHART_MAX_ROWS: usize = 1_024;
const PHYSIOLOGY_CHART_MAX_CAUSAL_SPANS: usize = 1_024;
const PHYSIOLOGY_CHART_MAX_PATIENT_CAUSES: usize = 256;

/// Trusted-gateway projection derived solely from bounded causal history.
#[view(accessor = backend_physiology_charts, public)]
pub fn backend_physiology_charts(ctx: &ViewContext) -> Vec<BackendPhysiologyChart> {
    if !is_strategic_gateway(ctx) {
        return Vec::new();
    }
    derive_physiology_chart(ctx)
}

#[view(accessor = backend_physiology_administrations, public)]
pub fn backend_physiology_administrations(
    ctx: &ViewContext,
) -> Vec<BackendPhysiologyAdministration> {
    if !is_strategic_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .physiology_administration()
        .administration_patient_id()
        .filter(0u64..)
        .map(|row| BackendPhysiologyAdministration {
            id: row.id,
            patient_id: row.patient_id,
            preparation_id: row.preparation_id,
            profile_version: row.profile_version,
            route: row.route,
            dose_milliunits: row.dose_milliunits,
            region: row.region,
            administered_at: row.administered_at,
            stopped_at: row.stopped_at,
        })
        .collect()
}

pub(crate) fn initialize_physiology_key(ctx: &ReducerContext) {
    if ctx.db.physiology_key_material().id().find(0).is_some() {
        return;
    }
    let mut key = Vec::with_capacity(32);
    for _ in 0..4 {
        key.extend_from_slice(&ctx.random::<u64>().to_le_bytes());
    }
    ctx.db
        .physiology_key_material()
        .insert(PhysiologyKeyMaterial {
            id: 0,
            version: physiology::PHENOTYPE_KEY_VERSION,
            key,
        });
}

fn derive_physiology_chart(ctx: &ViewContext) -> Vec<BackendPhysiologyChart> {
    let Some(key) = ctx.db.physiology_key_material().id().find(0) else {
        return Vec::new();
    };
    let clock = |id| {
        ctx.db
            .character_time()
            .character_id()
            .find(id)
            .map_or(0, |row| row.minutes)
    };
    let mut spans = ctx
        .db
        .physiology_presence_span()
        .presence_low_id()
        .filter(0u64..)
        .take(PHYSIOLOGY_CHART_MAX_CAUSAL_SPANS)
        .collect::<Vec<_>>();
    spans.sort_by_key(|span| (span.low_id, span.high_id, span.started_at, span.id));
    let mut result = Vec::new();
    let mut previous = BTreeMap::<(u64, u64), u64>::new();
    let mut continuous_starts = BTreeMap::<(u64, u64), u64>::new();
    let mut visible_history = BTreeMap::<(u64, u64), Vec<VisibleHumourReading>>::new();
    for span in spans {
        let joint_now = clock(span.low_id).min(clock(span.high_id));
        let end = span.ended_at.unwrap_or(joint_now).min(joint_now);
        if end < span.started_at {
            continue;
        }
        for (observer_id, patient_id, band) in [
            (span.low_id, span.high_id, span.low_observer_band),
            (span.high_id, span.low_id, span.high_observer_band),
        ] {
            let pair = (observer_id, patient_id);
            if result.len() >= PHYSIOLOGY_CHART_MAX_ROWS {
                return result;
            }
            let start = span
                .started_at
                .max(end.saturating_sub(PHYSIOLOGY_CHART_MAX_RANGE_MINUTES));
            if let Some(previous_end) = previous.get(&(observer_id, patient_id)).copied() {
                if previous_end < start {
                    result.push(BackendPhysiologyChart {
                        id: format!("gap:{observer_id}:{patient_id}:{previous_end}:{start}"),
                        observer_id,
                        patient_id,
                        observed_at: start,
                        physiology_band: band,
                        observation_minutes: 0,
                        sanguine_bps: Vec::new(),
                        phlegmatic_bps: Vec::new(),
                        choleric_bps: Vec::new(),
                        melancholic_bps: Vec::new(),
                        possible_diseases: Vec::new(),
                        known_interventions: Vec::new(),
                        confidence_bps: 0,
                        gap_from: Some(previous_end),
                        gap_to: Some(start),
                    });
                    continuous_starts.insert(pair, start);
                    visible_history.remove(&pair);
                }
            } else {
                continuous_starts.insert(pair, start);
            }
            let continuous_start = continuous_starts.get(&pair).copied().unwrap_or(start);
            let cadence = physiology::observation_cadence_minutes(band).max(1);
            let available = PHYSIOLOGY_CHART_MAX_ROWS
                .saturating_sub(result.len())
                .max(1);
            let natural_count = ((end.saturating_sub(start)) / cadence + 1) as usize;
            let stride = cadence.saturating_mul(
                u64::try_from(natural_count.div_ceil(available))
                    .unwrap_or(1)
                    .max(1),
            );
            let patient_state = chart_patient_state(ctx, patient_id);
            let mut minute = start;
            loop {
                if result.len() >= PHYSIOLOGY_CHART_MAX_ROWS {
                    return result;
                }
                let history = visible_history.entry(pair).or_default();
                let (reading, visible) = derive_chart_reading(
                    &key.key,
                    key.version,
                    observer_id,
                    patient_id,
                    band,
                    minute,
                    minute.saturating_sub(continuous_start),
                    &patient_state,
                    history,
                );
                if history.last().map(|row| row.observed_at) != Some(visible.observed_at) {
                    history.push(visible);
                }
                result.push(reading);
                if end.saturating_sub(minute) < stride {
                    break;
                }
                minute = minute.saturating_add(stride);
            }
            previous.insert(pair, end);
        }
    }
    for patient_time in ctx.db.character_time().minutes().filter(0u64..) {
        if result.len() >= PHYSIOLOGY_CHART_MAX_ROWS {
            break;
        }
        let Some(patient) = ctx
            .db
            .character()
            .id()
            .find(patient_time.character_id)
            .filter(|character| character.alive)
        else {
            continue;
        };
        let band = ctx
            .db
            .character_capability()
            .character_id()
            .find(patient.id)
            .map_or(0, |capability| {
                capability.physiology.round().clamp(0.0, 5.0) as u8
            });
        let patient_state = chart_patient_state(ctx, patient.id);
        let (reading, _) = derive_chart_reading(
            &key.key,
            key.version,
            patient.id,
            patient.id,
            band,
            patient_time.minutes,
            0,
            &patient_state,
            &[],
        );
        result.push(reading);
    }
    result
}

struct ChartPatientState {
    episodes: Vec<InfectionEpisode>,
    immunity: f32,
    interventions: Vec<physiology::Administration>,
}

fn chart_patient_state(ctx: &ViewContext, patient_id: u64) -> ChartPatientState {
    ChartPatientState {
        episodes: ctx
            .db
            .infection_episode()
            .character_id()
            .filter(patient_id)
            .take(PHYSIOLOGY_CHART_MAX_PATIENT_CAUSES)
            .filter_map(|row| episode(&row).ok())
            .collect(),
        immunity: ctx
            .db
            .character_attributes()
            .character_id()
            .find(patient_id)
            .map_or(3.0, |attributes| attributes.immunity),
        interventions: ctx
            .db
            .physiology_administration()
            .administration_patient_id()
            .filter(patient_id)
            .take(PHYSIOLOGY_CHART_MAX_PATIENT_CAUSES)
            .filter_map(|row| administration(&row).ok())
            .collect(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct VisibleHumourReading {
    observed_at: u64,
    mix: [f32; physiology::HUMOUR_COUNT],
}

#[expect(
    clippy::too_many_arguments,
    reason = "chart derivation keeps its keyed observation inputs explicit"
)]
fn derive_chart_reading(
    key: &[u8],
    key_version: u16,
    observer_id: u64,
    patient_id: u64,
    band: u8,
    minute: u64,
    observation_minutes: u64,
    patient_state: &ChartPatientState,
    prior_visible: &[VisibleHumourReading],
) -> (BackendPhysiologyChart, VisibleHumourReading) {
    let regional = chart_regional_state(key, key_version, patient_id, minute, patient_state);
    let mut public = physiology::regional_humours(&regional);
    for symptom in
        disease::observed_symptoms(&patient_state.episodes, minute, patient_state.immunity)
    {
        for region in symptom.observation_regions() {
            public[region.index()][symptom.humour().index()] += symptom.humour_deviation();
        }
    }
    for region in physiology::BodyRegion::ALL {
        for humour in physiology::Humour::ALL {
            public[region.index()][humour.index()] = (public[region.index()][humour.index()]
                + physiology::observation_noise(
                    key,
                    key_version,
                    observer_id,
                    patient_id,
                    region,
                    humour,
                    minute,
                    band,
                ))
            .clamp(-1.0, 1.0);
        }
    }
    let quantized = public.map(|values| physiology::quantize_humours(values, band));
    let visible = VisibleHumourReading {
        observed_at: minute,
        mix: visible_mix_from_quantized(&quantized),
    };
    let mut visible_sequence = prior_visible.to_vec();
    visible_sequence.push(visible);
    let possible_diseases = derive_possible_diseases(observation_minutes, band, &visible_sequence);
    let mut known_interventions = if band == 0 {
        Vec::new()
    } else {
        patient_state
            .interventions
            .iter()
            .filter(|value| {
                value.administered_at <= minute
                    && value.stopped_at.is_none_or(|stopped| minute <= stopped)
            })
            .map(|value| {
                if band >= 3 {
                    format!(
                        "{} v{} ({:?})",
                        value.preparation_id, value.profile_version, value.route
                    )
                } else {
                    value.preparation_id.clone()
                }
            })
            .collect::<Vec<_>>()
    };
    known_interventions.sort();
    known_interventions.dedup();
    (
        BackendPhysiologyChart {
            id: format!("reading:{observer_id}:{patient_id}:{minute}"),
            observer_id,
            patient_id,
            observed_at: minute,
            physiology_band: band,
            observation_minutes,
            sanguine_bps: quantized.iter().map(|values| values[0]).collect(),
            phlegmatic_bps: quantized.iter().map(|values| values[1]).collect(),
            choleric_bps: quantized.iter().map(|values| values[2]).collect(),
            melancholic_bps: quantized.iter().map(|values| values[3]).collect(),
            possible_diseases,
            known_interventions,
            confidence_bps: [1_500, 3_000, 5_000, 7_000, 8_500, 9_500][usize::from(band.min(5))],
            gap_from: None,
            gap_to: None,
        },
        visible,
    )
}

fn chart_regional_state(
    key: &[u8],
    key_version: u16,
    patient_id: u64,
    minute: u64,
    patient_state: &ChartPatientState,
) -> [physiology::MeterVector; physiology::REGION_COUNT] {
    let mut regional = disease::private_regional_meter_state(
        patient_id,
        &patient_state.episodes,
        minute,
        patient_state.immunity,
        key,
        key_version,
    );
    for administration in &patient_state.interventions {
        let effect = administration.effect_at(minute);
        if let Some(region) = administration.region {
            regional[region.index()].add_bounded(effect);
        } else {
            for values in &mut regional {
                values.add_bounded(effect.scaled(1.0 / physiology::REGION_COUNT as f32));
            }
        }
    }
    regional
}

fn derive_possible_diseases(
    observation_minutes: u64,
    band: u8,
    visible_sequence: &[VisibleHumourReading],
) -> Vec<BackendPhysiologyDifferential> {
    let observed_mix = visible_sequence
        .last()
        .map_or([0.25; physiology::HUMOUR_COUNT], |reading| reading.mix);
    let reliability_by_band = [0.12, 0.24, 0.42, 0.62, 0.80, 0.94];
    let time_factor = (observation_minutes as f32 / (8.0 * MINUTES_PER_DAY as f32)).clamp(0.0, 1.0);
    let reliability = reliability_by_band[usize::from(band.min(5))] * (0.25 + 0.75 * time_factor);

    let mut fits = adventuresim_core::disease::STARTER_DISEASES
        .iter()
        .map(|definition| {
            let mut meters = physiology::MeterVector::ZERO;
            for (meter, value) in disease::disease_peak_meters(definition.id) {
                meters.0[meter.index()] = *value;
            }
            let mut expected = physiology::humours(meters).map(f32::abs);
            for symptom in definition.symptoms {
                expected[symptom.humour().index()] += symptom.humour_deviation();
            }
            normalize_mix(&mut expected);
            let shape_fit = observed_mix
                .iter()
                .zip(expected)
                .map(|(actual, candidate)| (actual - candidate).abs())
                .sum::<f32>();
            let longitudinal = longitudinal_sequence_bonus(
                visible_sequence,
                observation_minutes,
                band,
                definition.id,
            );
            (
                definition,
                (1.0 - shape_fit * 0.5 + longitudinal).clamp(0.0, 1.0),
            )
        })
        .collect::<Vec<_>>();
    let mean = fits.iter().map(|(_, fit)| *fit).sum::<f32>() / fits.len() as f32;
    let mut result = fits
        .drain(..)
        .map(|(definition, fit)| {
            let differentiated = (0.5 + (fit - mean) * 2.2).clamp(0.0, 1.0);
            let likelihood = 0.5 + (differentiated - 0.5) * reliability;
            BackendPhysiologyDifferential {
                disease_id: disease_key(definition.id).to_owned(),
                label: definition.period_name.to_owned(),
                likelihood_bps: (likelihood
                    * f32::from(adventuresim_world_schema::BASIS_POINTS_PER_WHOLE))
                .round() as u16,
            }
        })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| {
        right
            .likelihood_bps
            .cmp(&left.likelihood_bps)
            .then_with(|| left.disease_id.cmp(&right.disease_id))
    });
    result
}

fn longitudinal_sequence_bonus(
    visible_sequence: &[VisibleHumourReading],
    observation_minutes: u64,
    band: u8,
    candidate: DiseaseId,
) -> f32 {
    let pattern = disease::diagnostic_pattern(candidate);
    let history_span = visible_sequence
        .first()
        .zip(visible_sequence.last())
        .map_or(0, |(first, last)| {
            last.observed_at.saturating_sub(first.observed_at)
        });
    if band < 3
        || observation_minutes < pattern.minimum_observation_minutes
        || history_span < pattern.minimum_observation_minutes
        || visible_sequence.len() < 3
    {
        return 0.0;
    }
    visible_sequence_fit(visible_sequence, candidate) * pattern.longitudinal_weight
}

fn visible_mix_from_quantized(
    quantized: &[[i16; physiology::HUMOUR_COUNT]; physiology::REGION_COUNT],
) -> [f32; physiology::HUMOUR_COUNT] {
    let mut mix = [0.0; physiology::HUMOUR_COUNT];
    for region in quantized {
        for (humour, value) in mix.iter_mut().zip(region) {
            *humour += f32::from(value.abs()) / physiology::REGION_COUNT as f32;
        }
    }
    normalize_mix(&mut mix);
    mix
}

/// Scores only a bounded sequence of public Humour mixes against public
/// disease definitions. Supplying the same visible sequence always produces
/// the same result, regardless of its hidden cause.
fn visible_sequence_fit(visible: &[VisibleHumourReading], candidate: DiseaseId) -> f32 {
    if visible.len() < 2 {
        return 0.0;
    }
    let definition = disease::definition(candidate);
    let total = definition
        .incubation_minutes
        .saturating_add(definition.rise_minutes)
        .saturating_add(definition.peak_minutes)
        .saturating_add(definition.recovery_minutes);
    let first_minute = visible.first().map_or(0, |reading| reading.observed_at);
    let last_minute = visible
        .last()
        .map_or(first_minute, |reading| reading.observed_at);
    let window = last_minute.saturating_sub(first_minute);
    if window == 0 {
        return 0.0;
    }
    let mut best = f32::INFINITY;
    for step in 0..=24_u64 {
        let latest_age = window.saturating_add(total.saturating_sub(window) * step / 24);
        let mut error = 0.0;
        for reading in visible {
            let age = latest_age.saturating_sub(last_minute.saturating_sub(reading.observed_at));
            let expected = expected_public_mix(candidate, age);
            error += reading
                .mix
                .iter()
                .zip(expected)
                .map(|(left, right)| (left - right).abs())
                .sum::<f32>();
        }
        best = best.min(error / visible.len() as f32);
    }
    (1.0 - best * 0.5).clamp(0.0, 1.0)
}

fn expected_public_mix(candidate: DiseaseId, age: u64) -> [f32; physiology::HUMOUR_COUNT] {
    let definition = disease::definition(candidate);
    let mut meters = physiology::MeterVector::ZERO;
    if let Some(curves) = disease::fantastic_meter_curves(candidate) {
        for curve in curves {
            meters.0[curve.meter.index()] = physiology::piecewise(curve, age);
        }
    } else {
        let (_, progress) = {
            let i = definition.incubation_minutes;
            let rise_end = i.saturating_add(definition.rise_minutes);
            let peak_end = rise_end.saturating_add(definition.peak_minutes);
            let end = peak_end.saturating_add(definition.recovery_minutes);
            if age < i {
                (0, 0.0)
            } else if age < rise_end {
                (1, (age - i) as f32 / definition.rise_minutes.max(1) as f32)
            } else if age < peak_end {
                (2, 1.0)
            } else if age < end {
                (
                    3,
                    1.0 - (age - peak_end) as f32 / definition.recovery_minutes.max(1) as f32,
                )
            } else {
                (4, 0.0)
            }
        };
        for (meter, peak) in disease::disease_peak_meters(candidate) {
            meters.0[meter.index()] = peak * progress;
        }
    }
    let mut expected = physiology::humours(meters).map(f32::abs);
    normalize_mix(&mut expected);
    expected
}

fn normalize_mix(values: &mut [f32; physiology::HUMOUR_COUNT]) {
    let total = values.iter().sum::<f32>();
    if total > f32::EPSILON {
        for value in values {
            *value /= total;
        }
    } else {
        values.fill(1.0 / physiology::HUMOUR_COUNT as f32);
    }
}

fn notice(
    ctx: &ReducerContext,
    character_id: u64,
    infection_id: u64,
    minute: u64,
    kind: &str,
    message: &str,
) -> Result<(), String> {
    let id = format!("disease-{infection_id}-{minute}-{kind}");
    if ctx.db.disease_notice().id().find(&id).is_none() {
        ctx.db.disease_notice().insert(DiseaseNotice {
            id,
            character_id,
            minute,
            kind: kind.into(),
            message: message.into(),
        });
    }
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(3.0, |attributes| attributes.immunity);
    let states = character_episodes(ctx, character_id)?
        .into_iter()
        .map(|episode| disease::evaluate(episode, minute, immunity))
        .collect::<Vec<_>>();
    let symptomatic = states.iter().any(|state| {
        !matches!(
            state.stage,
            disease::DiseaseStage::Incubating | disease::DiseaseStage::Resolved
        )
    });
    let critical = states
        .iter()
        .any(|state| state.stage == disease::DiseaseStage::Critical);
    let row = CharacterIllnessStatus {
        character_id,
        symptomatic,
        critical,
        updated_at_minute: minute,
    };
    if ctx
        .db
        .character_illness_status()
        .character_id()
        .find(character_id)
        .is_some()
    {
        ctx.db.character_illness_status().character_id().update(row);
    } else {
        ctx.db.character_illness_status().insert(row);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, SpacetimeType)]
pub enum DiseaseTerminalCause {
    Respiratory,
    Circulatory,
    Homeostatic,
    Neurologic,
}

pub(crate) fn parse_id(value: &str) -> Result<DiseaseId, String> {
    match value {
        "influenza" => Ok(DiseaseId::Influenza),
        "dysentery" => Ok(DiseaseId::Dysentery),
        "typhus" => Ok(DiseaseId::Typhus),
        "tetanus" => Ok(DiseaseId::Tetanus),
        "erysipelas" => Ok(DiseaseId::Erysipelas),
        "smallpox" => Ok(DiseaseId::Smallpox),
        "plague" => Ok(DiseaseId::Plague),
        "consumption" => Ok(DiseaseId::Consumption),
        "mahrdruck" => Ok(DiseaseId::Mahrdruck),
        "shroud_fever" => Ok(DiseaseId::ShroudFever),
        "bilwisschuss" => Ok(DiseaseId::Bilwisschuss),
        "kobeldunst" => Ok(DiseaseId::Kobeldunst),
        _ => Err("Unknown disease".into()),
    }
}

fn episode(row: &InfectionEpisodeRow) -> Result<InfectionEpisode, String> {
    if row.ruleset_version != physiology::PHYSIOLOGY_RULESET_VERSION {
        return Err(format!(
            "Unsupported physiology ruleset version {}",
            row.ruleset_version
        ));
    }
    if row.phenotype_key_version != physiology::PHENOTYPE_KEY_VERSION {
        return Err(format!(
            "Unsupported immutable physiology key version {}",
            row.phenotype_key_version
        ));
    }
    Ok(InfectionEpisode {
        id: row.id,
        character_id: row.character_id,
        disease_id: parse_id(&row.disease_id)?,
        contracted_at: row.contracted_at,
        ruleset_version: row.ruleset_version,
        phenotype_key_version: row.phenotype_key_version,
    })
}

pub fn character_episodes(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<Vec<InfectionEpisode>, String> {
    ctx.db
        .infection_episode()
        .character_id()
        .filter(character_id)
        .map(|row| episode(&row))
        .collect()
}

/// Historical, private party protection at one personal-clock minute. Presence
/// spans pin capability bands when they open and split whenever a band changes,
/// so later training cannot rewrite earlier prevention. Open spans are clamped
/// to both characters' clocks.
fn bounded_physiology_spans(
    ctx: &ReducerContext,
    ids: &[u64],
) -> Result<Vec<PhysiologyPresenceSpan>, String> {
    let mut spans_by_id = BTreeMap::new();
    let mut raw_rows = 0usize;
    for id in ids {
        for span in ctx
            .db
            .physiology_presence_span()
            .presence_low_id()
            .filter(*id)
            .chain(
                ctx.db
                    .physiology_presence_span()
                    .presence_high_id()
                    .filter(*id),
            )
        {
            raw_rows = raw_rows.saturating_add(1);
            if raw_rows > MAX_PARTY_INTERVAL_SPANS.saturating_mul(2) {
                return Err("Disease interval has too many raw presence spans".into());
            }
            disease::insert_unique_bounded(
                &mut spans_by_id,
                span.id,
                span,
                MAX_PARTY_INTERVAL_SPANS,
            )
            .map_err(str::to_string)?;
        }
    }
    Ok(spans_by_id.into_values().collect())
}

pub(crate) fn party_physiology_check_at(
    ctx: &ReducerContext,
    character_id: u64,
    minute: u64,
) -> f32 {
    try_party_physiology_check_at(ctx, character_id, minute).unwrap_or(0.0)
}

fn try_party_physiology_check_at(
    ctx: &ReducerContext,
    character_id: u64,
    minute: u64,
) -> Result<f32, String> {
    let clock = |id| {
        ctx.db
            .character_time()
            .character_id()
            .find(id)
            .map_or(0, |row| row.minutes)
    };
    let mut coverage = Vec::new();
    for span in bounded_physiology_spans(ctx, &[character_id])? {
        let joint_now = clock(span.low_id).min(clock(span.high_id));
        let end = span.ended_at.unwrap_or(joint_now).min(joint_now);
        coverage.push((
            span.low_id,
            span.started_at,
            end,
            f32::from(span.low_observer_band),
        ));
        coverage.push((
            span.high_id,
            span.started_at,
            end,
            f32::from(span.high_observer_band),
        ));
    }
    Ok(disease::historical_physiology_check_at(coverage, minute))
}

#[cfg(test)]
#[expect(
    dead_code,
    reason = "focused disease fixtures share this protected-exposure helper"
)]
pub(crate) fn protected_exposure_at(
    ctx: &ReducerContext,
    character_id: u64,
    minute: u64,
    vector: TransmissionVector,
    exposure: f32,
) -> f32 {
    disease::residual_exposure(
        exposure,
        vector,
        party_physiology_check_at(ctx, character_id, minute),
    )
}

/// Point actions have no interval-splitting ambiguity, so a solo character may
/// use their current capability. Party members still use pinned presence
/// history whenever it covers the action minute.
pub(crate) fn protected_point_exposure(
    ctx: &ReducerContext,
    character_id: u64,
    minute: u64,
    vector: TransmissionVector,
    exposure: f32,
) -> Result<f32, String> {
    let historical = try_party_physiology_check_at(ctx, character_id, minute)?;
    let check = if historical > 0.0 {
        historical
    } else {
        crate::capability::evaluate_character(ctx, character_id)?.physiology
    };
    Ok(disease::residual_exposure(exposure, vector, check))
}

const MAX_PARTY_INTERVAL_SPANS: usize = 4_096;
const MAX_PARTY_INTERVAL_WORK: u64 = 2_000_000;
const MAX_PARTY_INTERVAL_CANDIDATES: usize = 100_000;

fn blood_interval_work_budget(minutes: u64) -> u64 {
    let routes = disease::STARTER_DISEASES
        .iter()
        .filter(|definition| definition.supports(TransmissionVector::Blood))
        .count()
        .max(1) as u64;
    minutes.saturating_mul(routes)
}

#[derive(Clone, Debug)]
struct CachedCoverage {
    contributor_id: u64,
    start: u64,
    end: u64,
    check: f32,
}

#[derive(Clone, Debug)]
struct CachedCheckSegment {
    start: u64,
    end: u64,
    check: f32,
}

#[derive(Clone, Debug)]
struct CachedPairPresence {
    low_id: u64,
    high_id: u64,
    start: u64,
    end: u64,
}

/// Reducer-local immutable acquisition snapshot for members explicitly known
/// to co-advance. Preview and commit consume the same proposals, so database
/// mutation and member iteration order cannot alter the interval.
#[derive(Clone, Debug, Default)]
pub struct PartyDiseaseIntervalPlan {
    proposals: BTreeMap<u64, Vec<InfectionEpisode>>,
    coverage: BTreeMap<u64, Vec<CachedCheckSegment>>,
    work_units: u64,
}

impl PartyDiseaseIntervalPlan {
    pub(crate) fn check_at(&self, character_id: u64, minute: u64) -> f32 {
        let Some(segments) = self.coverage.get(&character_id) else {
            return 0.0;
        };
        let index = segments.partition_point(|segment| segment.end < minute);
        segments
            .get(index)
            .filter(|segment| segment.start <= minute)
            .map_or(0.0, |segment| segment.check)
    }

    fn proposals_for(&self, character_id: u64, from: u64, to: u64) -> Vec<InfectionEpisode> {
        self.proposals
            .get(&character_id)
            .into_iter()
            .flatten()
            .copied()
            .filter(|episode| episode.contracted_at > from && episode.contracted_at <= to)
            .collect()
    }

    pub fn work_units(&self) -> u64 {
        self.work_units
    }
}

pub fn plan_party_disease_interval(
    ctx: &ReducerContext,
    member_ids: &[u64],
    requested: u64,
    allow_healing: bool,
) -> Result<PartyDiseaseIntervalPlan, String> {
    let mut ids = member_ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let starts = ids
        .iter()
        .map(|id| {
            ctx.db
                .character_time()
                .character_id()
                .find(*id)
                .map(|row| (*id, row.minutes))
                .ok_or_else(|| "Party member has no strategic clock".to_string())
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let horizons = starts
        .iter()
        .map(|(id, start)| (*id, start.saturating_add(requested)))
        .collect::<BTreeMap<_, _>>();
    let mut coverage = BTreeMap::<u64, Vec<CachedCoverage>>::new();
    let mut pairs = Vec::new();
    let id_set = ids.iter().copied().collect::<BTreeSet<_>>();
    let spans_by_id = bounded_physiology_spans(ctx, &ids)?
        .into_iter()
        .map(|span| (span.id, span))
        .collect::<BTreeMap<_, _>>();
    let peer_ids = spans_by_id
        .values()
        .flat_map(|span| [span.low_id, span.high_id])
        .collect::<BTreeSet<_>>();
    let clocks = peer_ids
        .into_iter()
        .map(|id| {
            (
                id,
                starts.get(&id).copied().unwrap_or_else(|| {
                    ctx.db
                        .character_time()
                        .character_id()
                        .find(id)
                        .map_or(0, |row| row.minutes)
                }),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for span in spans_by_id.into_values() {
        let Some(end) = disease::projected_presence_end(
            span.ended_at,
            horizons.get(&span.low_id).copied(),
            horizons.get(&span.high_id).copied(),
            clocks[&span.low_id],
            clocks[&span.high_id],
        ) else {
            continue;
        };
        if end < span.started_at {
            continue;
        }
        if id_set.contains(&span.low_id) {
            coverage.entry(span.low_id).or_default().extend([
                CachedCoverage {
                    contributor_id: span.low_id,
                    start: span.started_at,
                    end,
                    check: f32::from(span.low_observer_band),
                },
                CachedCoverage {
                    contributor_id: span.high_id,
                    start: span.started_at,
                    end,
                    check: f32::from(span.high_observer_band),
                },
            ]);
        }
        if id_set.contains(&span.high_id) {
            coverage.entry(span.high_id).or_default().extend([
                CachedCoverage {
                    contributor_id: span.low_id,
                    start: span.started_at,
                    end,
                    check: f32::from(span.low_observer_band),
                },
                CachedCoverage {
                    contributor_id: span.high_id,
                    start: span.started_at,
                    end,
                    check: f32::from(span.high_observer_band),
                },
            ]);
        }
        if id_set.contains(&span.low_id) || id_set.contains(&span.high_id) {
            pairs.push(CachedPairPresence {
                low_id: span.low_id,
                high_id: span.high_id,
                start: span.started_at,
                end,
            });
        }
    }
    let span_count = coverage.values().map(Vec::len).sum::<usize>() + pairs.len();
    if span_count > MAX_PARTY_INTERVAL_SPANS {
        return Err("Party disease interval has too many relevant presence spans".into());
    }
    let coverage = coverage
        .into_iter()
        .map(|(character_id, spans)| {
            let mut boundaries = spans
                .iter()
                .flat_map(|span| [span.start, span.end.saturating_add(1)])
                .collect::<Vec<_>>();
            boundaries.sort_unstable();
            boundaries.dedup();
            let segments = boundaries
                .windows(2)
                .filter_map(|window| {
                    let start = window[0];
                    let end = window[1].saturating_sub(1);
                    let check = disease::historical_physiology_check_at(
                        spans
                            .iter()
                            .map(|span| (span.contributor_id, span.start, span.end, span.check)),
                        start,
                    );
                    (check > 0.0).then_some(CachedCheckSegment { start, end, check })
                })
                .collect::<Vec<_>>();
            (character_id, segments)
        })
        .collect();
    let base_work = ids.len() as u64;
    if base_work > MAX_PARTY_INTERVAL_WORK {
        return Err("Party disease interval exceeds bounded exposure work".into());
    }
    let mut plan = PartyDiseaseIntervalPlan {
        coverage,
        work_units: base_work,
        ..Default::default()
    };
    let source_ids = pairs
        .iter()
        .flat_map(|pair| [pair.low_id, pair.high_id])
        .chain(ids.iter().copied())
        .collect::<BTreeSet<_>>();
    let initial = source_ids
        .iter()
        .map(|id| character_episodes(ctx, *id).map(|episodes| (*id, episodes)))
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let immunities = ids
        .iter()
        .map(|id| {
            (
                *id,
                ctx.db
                    .character_attributes()
                    .character_id()
                    .find(*id)
                    .map_or(3.0, |row| row.immunity),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut scheduled = Vec::new();
    let mut environmental = Vec::new();
    for id in &ids {
        let start = starts[id];
        let end = horizons[id];
        let Some(character) = ctx.db.character().id().find(*id) else {
            continue;
        };
        if let Some(settlement_id) = character.current_settlement_id {
            let mut outbreaks = ctx
                .db
                .settlement_outbreak()
                .settlement_id()
                .filter(&settlement_id)
                .map(|row| {
                    (
                        row.id,
                        row.disease_id,
                        row.start_minute,
                        row.end_minute,
                        row.intensity,
                        false,
                    )
                })
                .collect::<Vec<_>>();
            outbreaks
                .sort_by(|left, right| (left.2, left.0.as_str()).cmp(&(right.2, right.0.as_str())));
            let scope_key = format!("settlement:{settlement_id}");
            let mut problems = ctx
                .db
                .local_problem_authority()
                .scope_key()
                .filter(&scope_key)
                .filter(|row| {
                    !row.disease_id.is_empty()
                        && row.disease_intensity > 0
                        && row.mitigation_bps < adventuresim_world_schema::BASIS_POINTS_PER_WHOLE
                })
                .collect::<Vec<_>>();
            problems.sort_by(|left, right| left.id.cmp(&right.id));
            problems.truncate(adventuresim_core::local_problem::MAX_ACTIVE_PER_SCOPE);
            let sources = outbreaks.into_iter().chain(problems.into_iter().map(|row| {
                (
                    row.id,
                    row.disease_id,
                    row.starts_at,
                    row.ends_at.min(row.resolved_at.unwrap_or(u64::MAX)),
                    f32::from(row.disease_intensity)
                        * f32::from(10_000_u16.saturating_sub(row.mitigation_bps))
                        / 10_000_000.0,
                    true,
                )
            }));
            for (source_id, disease_key, source_start, source_end, intensity, scoped) in sources {
                let disease_id = parse_id(&disease_key)?;
                let from = start.max(source_start);
                let to = end.min(source_end);
                let definition = disease::definition(disease_id);
                let windows = if scoped {
                    crate::outbreak::exposure_windows(ctx, &source_id, *id, from, to)
                } else {
                    vec![(source_id.clone(), from, to)]
                };
                for (window_source_id, window_from, window_to) in windows {
                    if let Some(source) = disease::protected_presence_exposure_source(
                        disease_id,
                        *id,
                        &window_source_id,
                        window_from,
                        window_to,
                        intensity,
                        definition.base_acquisition,
                        definition.primary_community_vector,
                    ) {
                        if environmental.len()
                            >= MAX_PARTY_INTERVAL_CANDIDATES.saturating_sub(scheduled.len())
                        {
                            return Err(
                                "Disease interval exceeds bounded acquisition candidates".into()
                            );
                        }
                        environmental.push(source);
                    }
                }
            }
        }
        let blood_attempts = crate::filth::blood_exposure_attempts_through(
            ctx,
            *id,
            start,
            end,
            false,
            allow_healing,
            Some(&plan),
            blood_interval_work_budget(end.saturating_sub(start)),
        )?;
        if blood_attempts.len()
            > MAX_PARTY_INTERVAL_CANDIDATES
                .saturating_sub(scheduled.len())
                .saturating_sub(environmental.len())
        {
            return Err("Disease interval exceeds bounded acquisition candidates".into());
        }
        scheduled.extend(blood_attempts);
    }
    let windows = pairs
        .into_iter()
        .filter_map(|pair| {
            let participant_starts = [pair.low_id, pair.high_id]
                .into_iter()
                .filter_map(|id| starts.get(&id).copied())
                .collect::<Vec<_>>();
            let participant_horizons = [pair.low_id, pair.high_id]
                .into_iter()
                .filter_map(|id| horizons.get(&id).copied())
                .collect::<Vec<_>>();
            let start = participant_starts
                .into_iter()
                .max()
                .unwrap_or(pair.start)
                .saturating_add(1)
                .max(pair.start);
            let end = participant_horizons
                .into_iter()
                .min()
                .unwrap_or(pair.end)
                .min(pair.end);
            (start <= end).then_some(disease::ContactWindow {
                low_id: pair.low_id,
                high_id: pair.high_id,
                start,
                end,
            })
        })
        .collect::<Vec<_>>();
    let from = starts.values().copied().min().unwrap_or(0);
    let to = horizons.values().copied().max().unwrap_or(from);
    let resolved = disease::resolve_acquisition_timeline(
        &id_set,
        &initial,
        scheduled,
        &environmental,
        &windows,
        &immunities,
        from,
        to,
        plan.work_units,
        MAX_PARTY_INTERVAL_WORK,
        |character_id, minute| plan.check_at(character_id, minute),
    )
    .map_err(str::to_string)?;
    plan.proposals = resolved.proposals;
    plan.work_units = resolved.work_units;
    Ok(plan)
}

#[cfg(test)]
#[expect(
    dead_code,
    reason = "focused disease fixtures build party-contact episodes through this helper"
)]
fn party_contact_episodes_through(
    ctx: &ReducerContext,
    character_id: u64,
    from: u64,
    to: u64,
) -> Result<Vec<InfectionEpisode>, String> {
    if to <= from {
        return Ok(Vec::new());
    }
    let clock = |id| {
        ctx.db
            .character_time()
            .character_id()
            .find(id)
            .map_or(0, |row| row.minutes)
    };
    let mut source_windows = BTreeMap::<u64, Vec<(u64, u64)>>::new();
    for span in ctx
        .db
        .physiology_presence_span()
        .iter()
        .filter(|span| span.low_id == character_id || span.high_id == character_id)
    {
        let source_id = if span.low_id == character_id {
            span.high_id
        } else {
            span.low_id
        };
        let joint_now = clock(span.low_id).min(clock(span.high_id));
        if let Some((start, end)) =
            disease::elapsed_presence_window(span.started_at, span.ended_at, joint_now, from, to)
        {
            source_windows
                .entry(source_id)
                .or_default()
                .push((start, end));
        }
    }
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(3.0, |row| row.immunity);
    let target_episodes = character_episodes(ctx, character_id)?;
    let mut proposals = Vec::new();
    let mut evaluated = BTreeSet::new();
    for (source_id, windows) in source_windows {
        let Some(_source) = ctx.db.character().id().find(source_id) else {
            continue;
        };
        let source_immunity = ctx
            .db
            .character_attributes()
            .character_id()
            .find(source_id)
            .map_or(3.0, |row| row.immunity);
        for source_row in ctx.db.infection_episode().character_id().filter(source_id) {
            let source_episode = episode(&source_row)?;
            let definition = disease::definition(source_episode.disease_id);
            if !definition.supports(TransmissionVector::CloseContact) {
                continue;
            }
            for (start, end) in &windows {
                for minute in *start..=*end {
                    if !evaluated.insert((source_episode.id, minute))
                        || disease::has_unresolved_disease(
                            &target_episodes,
                            source_episode.disease_id,
                            minute,
                            immunity,
                        )
                    {
                        continue;
                    }
                    let infectiousness =
                        disease::close_contact_infectiousness(source_episode, minute);
                    if infectiousness <= 0.0
                        || matches!(
                            disease::evaluate(source_episode, minute, source_immunity).stage,
                            disease::DiseaseStage::Resolved
                        )
                    {
                        continue;
                    }
                    let exposure = protected_exposure_at(
                        ctx,
                        character_id,
                        minute,
                        TransmissionVector::CloseContact,
                        infectiousness / MINUTES_PER_DAY as f32,
                    );
                    let prior = disease::acquired_immunity(
                        &target_episodes,
                        source_episode.disease_id,
                        minute,
                        immunity,
                    );
                    let seed = disease::contact_exposure_seed(
                        character_id,
                        source_id,
                        source_episode.id,
                        minute,
                    );
                    if disease::acquisition_succeeds(seed, definition, immunity, prior, exposure) {
                        proposals.push(InfectionEpisode {
                            id: seed,
                            character_id,
                            disease_id: source_episode.disease_id,
                            contracted_at: minute,
                            ruleset_version: physiology::PHYSIOLOGY_RULESET_VERSION,
                            phenotype_key_version: physiology::PHENOTYPE_KEY_VERSION,
                        });
                        break;
                    }
                }
            }
        }
    }
    Ok(merge_acquisition_proposals(Vec::new(), proposals))
}

#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
#[cfg(test)]
#[expect(
    dead_code,
    reason = "focused disease fixtures search protected presence exposure boundaries through this helper"
)]
fn first_protected_presence_exposure_minute(
    ctx: &ReducerContext,
    episodes: &[InfectionEpisode],
    disease_id: DiseaseId,
    character_id: u64,
    exposure_id: &str,
    from: u64,
    to: u64,
    intensity: f32,
    immunity: f32,
) -> Option<u64> {
    let definition = disease::definition(disease_id);
    disease::first_eligible_protected_presence_exposure_minute(
        episodes,
        disease_id,
        character_id,
        exposure_id,
        from,
        to,
        intensity,
        definition.base_acquisition,
        immunity,
        definition.primary_community_vector,
        |minute| party_physiology_check_at(ctx, character_id, minute),
    )
}

pub fn effective_attributes(
    ctx: &ReducerContext,
    character_id: u64,
    mut attributes: crate::CharacterAttributes,
) -> Result<crate::CharacterAttributes, String> {
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |t| t.minutes);
    let (penalty, _, _, _) = disease::combined_state(
        &character_episodes(ctx, character_id)?,
        now,
        attributes.immunity,
    );
    attributes.endurance = (attributes.endurance - penalty.endurance).max(0.0);
    attributes.immunity = (attributes.immunity - penalty.immunity).max(0.0);
    attributes.gut = (attributes.gut - penalty.gut).max(0.0);
    attributes.intelligence = (attributes.intelligence - penalty.intelligence).max(0.0);
    attributes.instinct = (attributes.instinct - penalty.instinct).max(0.0);
    for value in [
        &mut attributes.left_arm_agility,
        &mut attributes.right_arm_agility,
        &mut attributes.left_leg_agility,
        &mut attributes.right_leg_agility,
    ] {
        *value = (*value - penalty.limb_agility).max(0.0)
    }
    Ok(attributes)
}

#[cfg(test)]
#[expect(
    dead_code,
    reason = "focused disease fixtures build outbreak episodes through this helper"
)]
fn outbreak_episodes_through(
    ctx: &ReducerContext,
    character_id: u64,
    from: u64,
    to: u64,
) -> Result<Vec<InfectionEpisode>, String> {
    let Some(character) = ctx.db.character().id().find(character_id) else {
        return Ok(Vec::new());
    };
    let Some(settlement_id) = character.current_settlement_id else {
        return Ok(Vec::new());
    };
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(3.0, |a| a.immunity);
    let mut episodes = character_episodes(ctx, character_id)?;
    let existing_len = episodes.len();
    let mut outbreaks = ctx
        .db
        .settlement_outbreak()
        .settlement_id()
        .filter(&settlement_id)
        .collect::<Vec<_>>();
    outbreaks.sort_by(|left, right| {
        (left.start_minute, left.id.as_str()).cmp(&(right.start_minute, right.id.as_str()))
    });
    for outbreak in outbreaks {
        let disease_id = parse_id(&outbreak.disease_id)?;
        let overlap_from = from.max(outbreak.start_minute);
        let overlap_to = to.min(outbreak.end_minute);
        if overlap_to <= overlap_from {
            continue;
        }
        let Some(at) = first_protected_presence_exposure_minute(
            ctx,
            &episodes,
            disease_id,
            character_id,
            &outbreak.id,
            overlap_from,
            overlap_to,
            outbreak.intensity,
            immunity,
        ) else {
            continue;
        };
        if !episodes
            .iter()
            .any(|episode| episode.disease_id == disease_id && episode.contracted_at == at)
        {
            episodes.push(InfectionEpisode {
                id: disease::outbreak_exposure_seed(character_id, &format!("{}:{at}", outbreak.id)),
                character_id,
                disease_id,
                contracted_at: at,
                ruleset_version: physiology::PHYSIOLOGY_RULESET_VERSION,
                phenotype_key_version: physiology::PHENOTYPE_KEY_VERSION,
            });
        }
    }
    // Local-problem exposure uses the same minute-domain evaluator as normal
    // outbreaks. Stable problem IDs make split and single time advances agree.
    let scope_key = format!("settlement:{settlement_id}");
    let mut problems = ctx
        .db
        .local_problem_authority()
        .scope_key()
        .filter(&scope_key)
        .filter(|row| !row.disease_id.is_empty() && row.disease_intensity > 0)
        .collect::<Vec<_>>();
    problems.sort_by(|left, right| left.id.cmp(&right.id));
    problems.truncate(adventuresim_core::local_problem::MAX_ACTIVE_PER_SCOPE);
    for problem in problems {
        let overlap_from = from.max(problem.starts_at);
        let overlap_to = to
            .min(problem.ends_at)
            .min(problem.resolved_at.unwrap_or(u64::MAX));
        if overlap_to <= overlap_from
            || problem.mitigation_bps >= adventuresim_world_schema::BASIS_POINTS_PER_WHOLE
        {
            continue;
        }
        let disease_id = parse_id(&problem.disease_id)?;
        let intensity = f32::from(problem.disease_intensity)
            * f32::from(10_000_u16.saturating_sub(problem.mitigation_bps))
            / 10_000_000.0;
        for (source_id, window_from, window_to) in crate::outbreak::exposure_windows(
            ctx,
            &problem.id,
            character_id,
            overlap_from,
            overlap_to,
        ) {
            let Some(at) = first_protected_presence_exposure_minute(
                ctx,
                &episodes,
                disease_id,
                character_id,
                &source_id,
                window_from,
                window_to,
                intensity,
                immunity,
            ) else {
                continue;
            };
            if !episodes
                .iter()
                .any(|episode| episode.disease_id == disease_id && episode.contracted_at == at)
            {
                episodes.push(InfectionEpisode {
                    id: disease::outbreak_exposure_seed(
                        character_id,
                        &format!("{}:{at}", problem.id),
                    ),
                    character_id,
                    disease_id,
                    contracted_at: at,
                    ruleset_version: physiology::PHYSIOLOGY_RULESET_VERSION,
                    phenotype_key_version: physiology::PHENOTYPE_KEY_VERSION,
                });
            }
        }
    }
    Ok(episodes.split_off(existing_len))
}

fn persist_acquisition_episodes(
    ctx: &ReducerContext,
    character_id: u64,
    episodes: impl IntoIterator<Item = InfectionEpisode>,
) -> Result<(), String> {
    for episode in episodes {
        let disease_id = disease_key(episode.disease_id);
        if !ctx
            .db
            .infection_episode()
            .character_id()
            .filter(character_id)
            .any(|row| row.disease_id == disease_id && row.contracted_at == episode.contracted_at)
        {
            ctx.db.infection_episode().insert(InfectionEpisodeRow {
                id: episode.id,
                character_id,
                disease_id: disease_id.into(),
                contracted_at: episode.contracted_at,
                ruleset_version: episode.ruleset_version,
                phenotype_key_version: episode.phenotype_key_version,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
#[expect(
    dead_code,
    reason = "focused disease fixtures merge independently generated acquisition proposals"
)]
fn merge_acquisition_proposals(
    mut proposals: Vec<InfectionEpisode>,
    additional: impl IntoIterator<Item = InfectionEpisode>,
) -> Vec<InfectionEpisode> {
    for candidate in additional {
        if let Some(existing) = proposals
            .iter_mut()
            .find(|episode| episode.disease_id == candidate.disease_id)
        {
            if (candidate.contracted_at, candidate.id) < (existing.contracted_at, existing.id) {
                *existing = candidate;
            }
        } else {
            proposals.push(candidate);
        }
    }
    proposals.sort_by_key(|episode| (episode.contracted_at, episode.id));
    proposals
}

pub(crate) fn physiology_key(ctx: &ReducerContext) -> Result<PhysiologyKeyMaterial, String> {
    let key = ctx
        .db
        .physiology_key_material()
        .id()
        .find(0)
        .ok_or_else(|| "Private physiology key material is not initialized".to_string())?;
    if key.version != physiology::PHENOTYPE_KEY_VERSION {
        return Err(format!(
            "Immutable physiology key version {} does not match ruleset version {}",
            key.version,
            physiology::PHENOTYPE_KEY_VERSION
        ));
    }
    Ok(key)
}

fn administration(row: &PhysiologyAdministration) -> Result<physiology::Administration, String> {
    if row.ruleset_version != physiology::PHYSIOLOGY_RULESET_VERSION {
        return Err(format!(
            "Unsupported intervention ruleset version {}",
            row.ruleset_version
        ));
    }
    Ok(physiology::Administration {
        id: row.id,
        patient_id: row.patient_id,
        preparation_id: row.preparation_id.clone(),
        profile_version: row.profile_version,
        route: row.route,
        dose: physiology::DoseMilliunits::try_new(row.dose_milliunits)
            .map_err(|_| "Persisted intervention dose exceeds the supported maximum")?,
        region: row.region,
        administered_at: row.administered_at,
        stopped_at: row.stopped_at,
        sensitivity_bps: row.sensitivity_bps,
        adverse_bps: row.adverse_bps,
    })
}

fn intervention_rows(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<Vec<physiology::Administration>, String> {
    ctx.db
        .physiology_administration()
        .administration_patient_id()
        .filter(character_id)
        .map(|row| administration(&row))
        .collect()
}

fn private_combined_at(
    patient_id: u64,
    episodes: &[InfectionEpisode],
    interventions: &[physiology::Administration],
    minute: u64,
    immunity: f32,
    key: &PhysiologyKeyMaterial,
) -> physiology::MeterVector {
    let baseline = physiology::baseline_meters(&key.key, key.version, patient_id);
    physiology::combined_meter_state(
        [baseline].into_iter().chain(
            episodes
                .iter()
                .map(|episode| disease::private_meter_state(*episode, minute, immunity, &key.key)),
        ),
        interventions,
        minute,
    )
}

fn terminal_failure_for_meter(meter: physiology::Meter) -> TerminalFailure {
    match meter {
        physiology::Meter::Oxygenation => TerminalFailure::Respiratory,
        physiology::Meter::Perfusion
        | physiology::Meter::Coagulation
        | physiology::Meter::TissueIntegrity => TerminalFailure::Circulatory,
        physiology::Meter::Neurologic => TerminalFailure::Neurologic,
        physiology::Meter::Hydration
        | physiology::Meter::Temperature
        | physiology::Meter::Inflammation
        | physiology::Meter::Nutrition
        | physiology::Meter::RenalClearance => TerminalFailure::Homeostatic,
    }
}

pub(crate) fn first_private_terminal(
    ctx: &ReducerContext,
    character_id: u64,
    episodes: &[InfectionEpisode],
    from: u64,
    to: u64,
    immunity: f32,
) -> Result<Option<(u64, TerminalFailure)>, String> {
    let interventions = intervention_rows(ctx, character_id)?;
    let key = physiology_key(ctx)?;
    let mut structural = vec![from, to];
    for episode in episodes {
        structural.extend(disease::structural_minutes(*episode, from, to));
        structural.extend(
            disease::interval_events(*episode, from, to, immunity)
                .into_iter()
                .map(|event| event.minute),
        );
    }
    for administration in &interventions {
        let profile_end = physiology::intervention_profile(
            &administration.preparation_id,
            administration.profile_version,
        )
        .map(|profile| {
            administration
                .administered_at
                .saturating_add(profile.duration_minutes)
        });
        structural.extend(
            [
                Some(administration.administered_at),
                administration.stopped_at,
                profile_end,
            ]
            .into_iter()
            .flatten()
            .filter(|minute| *minute >= from && *minute <= to),
        );
    }
    Ok(physiology::first_terminal_crossing(&structural, |minute| {
        private_combined_at(
            character_id,
            episodes,
            &interventions,
            minute,
            immunity,
            &key,
        )
    })
    .map(|(minute, meter)| (minute, terminal_failure_for_meter(meter))))
}

/// Returns the safe prefix of an interval and a terminal mechanism, if any.
/// All boundary events at the earliest minute are considered together.
fn clip_elapsed_for_disease_planned(
    ctx: &ReducerContext,
    character_id: u64,
    requested: u64,
    allow_healing: bool,
    plan: Option<&PartyDiseaseIntervalPlan>,
) -> Result<(u64, Option<TerminalFailure>), String> {
    if requested == 0 {
        return Ok((0, None));
    }
    let owned_plan;
    let plan = if let Some(plan) = plan {
        plan
    } else {
        owned_plan = plan_party_disease_interval(ctx, &[character_id], requested, allow_healing)?;
        &owned_plan
    };
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |t| t.minutes);
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(3.0, |a| a.immunity);
    let mut episodes = character_episodes(ctx, character_id)?;
    let interval_end = now.saturating_add(requested);
    let proposed = plan.proposals_for(character_id, now, interval_end);
    episodes.extend(proposed.iter().copied());
    let mut events = episodes
        .iter()
        .copied()
        .flat_map(|e| disease::interval_events(e, now, now.saturating_add(requested), immunity))
        .collect::<Vec<_>>();
    events.sort_by_key(|e| e.minute);
    let terminal = first_private_terminal(
        ctx,
        character_id,
        &episodes,
        now,
        now.saturating_add(requested),
        immunity,
    )?;
    let death_minute = terminal.map(|value| value.0);
    let through = death_minute.unwrap_or_else(|| now.saturating_add(requested));
    // The terminal minute is inclusive: infections and notices occurring at
    // that boundary are committed; later effects from the requested interval
    // are never persisted.
    persist_acquisition_episodes(
        ctx,
        character_id,
        proposed
            .into_iter()
            .filter(|episode| disease::infection_occurs_through(*episode, through)),
    )?;
    // Re-evaluate only the committed prefix and advance the private cursor.
    // Absolute-minute seeds guarantee the same proposal as preview/full evaluation.
    let _ = crate::filth::blood_exposure_attempts_through(
        ctx,
        character_id,
        now,
        through,
        true,
        allow_healing,
        Some(plan),
        blood_interval_work_budget(through.saturating_sub(now)),
    )?;
    for event in events.iter().filter(|event| event.minute <= through) {
        match event.kind {
            DiseaseEventKind::SymptomOnset => notice(
                ctx,
                character_id,
                event.infection_id,
                event.minute,
                "symptom-onset",
                "New symptoms have appeared.",
            )?,
            DiseaseEventKind::Peak => {}
            DiseaseEventKind::Critical(_) => notice(
                ctx,
                character_id,
                event.infection_id,
                event.minute,
                "critical",
                "A vital humour is failing.",
            )?,
            DiseaseEventKind::Resolution => notice(
                ctx,
                character_id,
                event.infection_id,
                event.minute,
                "resolution",
                "The illness's visible effects have resolved.",
            )?,
        }
    }
    let Some(death_minute) = death_minute else {
        return Ok((requested, None));
    };
    notice(
        ctx,
        character_id,
        0,
        death_minute,
        "critical",
        "A vital humour is failing.",
    )?;
    Ok((
        death_minute.saturating_sub(now),
        terminal.map(|value| value.1),
    ))
}

pub fn clip_elapsed_for_disease(
    ctx: &ReducerContext,
    character_id: u64,
    requested: u64,
    allow_healing: bool,
) -> Result<(u64, Option<TerminalFailure>), String> {
    clip_elapsed_for_disease_planned(ctx, character_id, requested, allow_healing, None)
}

pub fn clip_elapsed_for_disease_in_plan(
    ctx: &ReducerContext,
    character_id: u64,
    requested: u64,
    allow_healing: bool,
    plan: &PartyDiseaseIntervalPlan,
) -> Result<(u64, Option<TerminalFailure>), String> {
    clip_elapsed_for_disease_planned(ctx, character_id, requested, allow_healing, Some(plan))
}

/// Side-effect-free party preflight. Acquisition and notice delivery happen
/// only in the subsequent committed interval.
fn preview_disease_boundary_planned(
    ctx: &ReducerContext,
    character_id: u64,
    requested: u64,
    allow_healing: bool,
    plan: Option<&PartyDiseaseIntervalPlan>,
) -> Result<(u64, bool), String> {
    let owned_plan;
    let plan = if let Some(plan) = plan {
        plan
    } else {
        owned_plan = plan_party_disease_interval(ctx, &[character_id], requested, allow_healing)?;
        &owned_plan
    };
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |t| t.minutes);
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(3.0, |a| a.immunity);
    let mut episodes = character_episodes(ctx, character_id)?;
    episodes.extend(plan.proposals_for(character_id, now, now.saturating_add(requested)));
    let terminal = first_private_terminal(
        ctx,
        character_id,
        &episodes,
        now,
        now.saturating_add(requested),
        immunity,
    )?;
    Ok((
        terminal.map_or(requested, |(minute, _)| minute.saturating_sub(now)),
        terminal.is_some(),
    ))
}

/// Side-effect-free terminal preview. The boolean is necessary because a
/// terminal crossing exactly at the requested endpoint has the same elapsed
/// duration as ordinary completion but must still suppress completion effects.
pub fn preview_disease_terminal_boundary(
    ctx: &ReducerContext,
    character_id: u64,
    requested: u64,
    allow_healing: bool,
) -> Result<(u64, bool), String> {
    preview_disease_boundary_planned(ctx, character_id, requested, allow_healing, None)
}

pub fn preview_elapsed_for_disease(
    ctx: &ReducerContext,
    character_id: u64,
    requested: u64,
    allow_healing: bool,
) -> Result<u64, String> {
    preview_disease_boundary_planned(ctx, character_id, requested, allow_healing, None)
        .map(|preview| preview.0)
}

pub fn preview_elapsed_for_disease_in_plan(
    ctx: &ReducerContext,
    character_id: u64,
    requested: u64,
    allow_healing: bool,
    plan: &PartyDiseaseIntervalPlan,
) -> Result<u64, String> {
    preview_disease_boundary_planned(ctx, character_id, requested, allow_healing, Some(plan))
        .map(|preview| preview.0)
}

pub fn finish_disease_interval(
    ctx: &ReducerContext,
    character_id: u64,
    cause: Option<TerminalFailure>,
) -> Result<(), String> {
    let Some(cause) = cause else { return Ok(()) };
    crate::transition_character_to_dead(
        ctx,
        character_id,
        match cause {
            TerminalFailure::Respiratory => crate::DeathCause::RespiratoryFailure,
            TerminalFailure::Circulatory => crate::DeathCause::CirculatoryFailure,
            TerminalFailure::Homeostatic => crate::DeathCause::HomeostaticFailure,
            TerminalFailure::Neurologic => crate::DeathCause::NeurologicFailure,
        },
        crate::DeathSource::Disease,
        Some(
            match cause {
                TerminalFailure::Respiratory => "respiratory-failure",
                TerminalFailure::Circulatory => "circulatory-failure",
                TerminalFailure::Homeostatic => "homeostatic-failure",
                TerminalFailure::Neurologic => "neurologic-failure",
            }
            .into(),
        ),
    )?;
    Ok(())
}

pub(crate) fn disease_key(id: DiseaseId) -> &'static str {
    match id {
        DiseaseId::Influenza => "influenza",
        DiseaseId::Dysentery => "dysentery",
        DiseaseId::Typhus => "typhus",
        DiseaseId::Tetanus => "tetanus",
        DiseaseId::Erysipelas => "erysipelas",
        DiseaseId::Smallpox => "smallpox",
        DiseaseId::Plague => "plague",
        DiseaseId::Consumption => "consumption",
        DiseaseId::Mahrdruck => "mahrdruck",
        DiseaseId::ShroudFever => "shroud_fever",
        DiseaseId::Bilwisschuss => "bilwisschuss",
        DiseaseId::Kobeldunst => "kobeldunst",
    }
}

fn private_variation(
    ctx: &ReducerContext,
    patient_id: u64,
    administration_minute: u64,
    preparation_id: &str,
) -> Result<(i16, u16), String> {
    use sha2::{Digest, Sha256};
    let key = physiology_key(ctx)?;
    let mut input = Sha256::new();
    input.update(b"adventuresim/physiology/administration");
    input.update(administration_minute.to_le_bytes());
    input.update(preparation_id.as_bytes());
    let discriminator =
        u64::from_le_bytes(input.finalize()[..8].try_into().expect("SHA-256 prefix"));
    let phenotype =
        physiology::phenotype_multipliers(&key.key, key.version, patient_id, discriminator);
    let sensitivity = ((phenotype[0] - 1.0) * 5_000.0).round() as i16;
    let adverse = ((phenotype[1] - 0.72) / 0.56 * 1_500.0)
        .round()
        .clamp(0.0, 1_500.0) as u16;
    Ok((sensitivity.clamp(-2_500, 2_500), adverse))
}

pub(crate) fn require_intervention_relationship(
    ctx: &ReducerContext,
    actor_id: u64,
    patient_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, actor_id)?;
    crate::require_living_character(ctx, actor_id)?;
    crate::require_living_character(ctx, patient_id)?;
    if actor_id == patient_id {
        return Ok(());
    }
    match crate::world_actor::contextual_nonemergency_treatment_decision(ctx, actor_id, patient_id)
    {
        adventuresim_core::strategic_action::ContextualActionDecision::Allowed(_) => Ok(()),
        adventuresim_core::strategic_action::ContextualActionDecision::Refused => {
            Err("The patient refused this intervention".into())
        }
        adventuresim_core::strategic_action::ContextualActionDecision::Unavailable => Err(
            "An intervention actor and patient must share an available treatment context".into(),
        ),
    }
}

fn commit_terminal_at_boundary(
    ctx: &ReducerContext,
    patient_id: u64,
    minute: u64,
) -> Result<(), String> {
    let episodes = character_episodes(ctx, patient_id)?;
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(patient_id)
        .map_or(3.0, |attributes| attributes.immunity);
    let interventions = intervention_rows(ctx, patient_id)?;
    let key = physiology_key(ctx)?;
    if let Some(meter) = private_combined_at(
        patient_id,
        &episodes,
        &interventions,
        minute,
        immunity,
        &key,
    )
    .terminal()
    {
        finish_disease_interval(ctx, patient_id, Some(terminal_failure_for_meter(meter)))?;
    }
    Ok(())
}

#[reducer]
#[expect(
    clippy::too_many_arguments,
    reason = "the reducer ABI exposes each validated administration input"
)]
pub fn administer_preparation(
    ctx: &ReducerContext,
    actor_id: u64,
    patient_id: u64,
    inventory_item_id: u64,
    profile_version: u16,
    route: InterventionRoute,
    dose_milliunits: u32,
    region: Option<BodyRegion>,
) -> Result<(), String> {
    require_intervention_relationship(ctx, actor_id, patient_id)?;
    administer_preparation_inner(
        ctx,
        patient_id,
        inventory_item_id,
        profile_version,
        route,
        dose_milliunits,
        region,
    )
}

fn administer_preparation_inner(
    ctx: &ReducerContext,
    patient_id: u64,
    inventory_item_id: u64,
    profile_version: u16,
    route: InterventionRoute,
    dose_milliunits: u32,
    region: Option<BodyRegion>,
) -> Result<(), String> {
    let dose = physiology::DoseMilliunits::try_new(dose_milliunits)
        .map_err(|_| "Intervention dose must be between 1 and 8000 milliunits")?;
    if dose.is_zero() {
        return Err("Intervention dose must be between 1 and 8000 milliunits".into());
    }
    let inventory = ctx
        .db
        .inventory_item()
        .id()
        .find(inventory_item_id)
        .ok_or("Preparation is not in the patient's inventory")?;
    if inventory.character_id != patient_id || inventory.quantity != 1 {
        return Err("Preparation must be an individual item in the patient's inventory".into());
    }
    let profile = physiology::intervention_profile(&inventory.item_id, profile_version)
        .ok_or("Unknown preparation profile version")?;
    if profile.route != route {
        return Err("Preparation does not support that route".into());
    }
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(patient_id)
        .ok_or("Patient time not found")?
        .minutes;
    let key = physiology_key(ctx)?;
    let (sensitivity_bps, adverse_bps) =
        private_variation(ctx, patient_id, now, &inventory.item_id)?;
    ctx.db
        .physiology_administration()
        .insert(PhysiologyAdministration {
            id: 0,
            patient_id,
            preparation_id: inventory.item_id,
            profile_version,
            route,
            dose_milliunits: dose.get(),
            region,
            administered_at: now,
            stopped_at: None,
            sensitivity_bps,
            adverse_bps,
            ruleset_version: physiology::PHYSIOLOGY_RULESET_VERSION,
            phenotype_key_version: key.version,
        });
    ctx.db.inventory_item().id().delete(inventory_item_id);
    commit_terminal_at_boundary(ctx, patient_id, now)?;
    Ok(())
}

/// Applies a pinned generic intervention component without consuming a
/// standalone medication item. Shared carriers (notably food lots) call this with the
/// exact proportional amount they consumed.
pub(crate) fn administer_intervention_component(
    ctx: &ReducerContext,
    patient_id: u64,
    preparation_id: &str,
    profile_version: u16,
    dose: physiology::DoseMilliunits,
) -> Result<(), String> {
    if dose.is_zero() {
        return Ok(());
    }
    let profile = physiology::intervention_profile(preparation_id, profile_version)
        .ok_or("Unknown medicinal component profile version")?;
    if profile.route != physiology::InterventionRoute::Oral {
        return Err("Only oral medicinal components can be consumed with food".into());
    }
    let now = ctx
        .db
        .character_time()
        .character_id()
        .find(patient_id)
        .ok_or("Patient time not found")?
        .minutes;
    let key = physiology_key(ctx)?;
    let (sensitivity_bps, adverse_bps) = private_variation(ctx, patient_id, now, preparation_id)?;
    ctx.db
        .physiology_administration()
        .insert(PhysiologyAdministration {
            id: 0,
            patient_id,
            preparation_id: preparation_id.into(),
            profile_version,
            route: InterventionRoute::Oral,
            dose_milliunits: dose.get(),
            region: None,
            administered_at: now,
            stopped_at: None,
            sensitivity_bps,
            adverse_bps,
            ruleset_version: physiology::PHYSIOLOGY_RULESET_VERSION,
            phenotype_key_version: key.version,
        });
    commit_terminal_at_boundary(ctx, patient_id, now)
}

#[reducer]
pub fn stop_preparation(
    ctx: &ReducerContext,
    actor_id: u64,
    administration_id: u64,
) -> Result<(), String> {
    let mut administration = ctx
        .db
        .physiology_administration()
        .id()
        .find(administration_id)
        .ok_or("Administration not found")?;
    require_intervention_relationship(ctx, actor_id, administration.patient_id)?;
    if administration.stopped_at.is_none() {
        let patient_id = administration.patient_id;
        let administered_at = administration.administered_at;
        administration.stopped_at = Some(
            ctx.db
                .character_time()
                .character_id()
                .find(administration.patient_id)
                .map_or(administration.administered_at, |time| time.minutes),
        );
        ctx.db
            .physiology_administration()
            .id()
            .update(administration);
        let minute = ctx
            .db
            .character_time()
            .character_id()
            .find(patient_id)
            .map_or(administered_at, |time| time.minutes);
        commit_terminal_at_boundary(ctx, patient_id, minute)?;
    }
    Ok(())
}

pub fn record_committed_cut(
    ctx: &ReducerContext,
    character_id: u64,
    severity: f32,
    surgery_check: f32,
) -> Result<(), String> {
    if severity <= 0.0 {
        return Ok(());
    }
    let at = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map_or(0, |t| t.minutes);
    let cut = ctx.db.committed_cut().insert(CommittedCut {
        id: 0,
        character_id,
        committed_at: at,
        severity,
        surgery_check,
    });
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(3.0, |a| a.immunity);
    let residual = (1.0 - (surgery_check / 5.0).clamp(0.0, 1.0) * 0.8) * severity.clamp(0.0, 1.0);
    for disease_id in [DiseaseId::Tetanus, DiseaseId::Erysipelas] {
        let d = disease::definition(disease_id);
        let seed = disease::outbreak_exposure_seed(
            character_id,
            &format!("cut-{}-{disease_id:?}", cut.id),
        );
        if disease::acquisition_succeeds(seed, d, immunity, 0.0, residual) {
            ctx.db.infection_episode().insert(InfectionEpisodeRow {
                id: 0,
                character_id,
                disease_id: disease_key(disease_id).to_owned(),
                contracted_at: at,
                ruleset_version: physiology::PHYSIOLOGY_RULESET_VERSION,
                phenotype_key_version: physiology::PHENOTYPE_KEY_VERSION,
            });
        }
    }
    Ok(())
}

/// Deterministic standing-wound exposure. The stable token is derived from a
/// limb key and monotonically increasing exposure checkpoint, so changing the
/// size of elapsed-time chunks cannot change acquisition outcomes.
pub fn record_standing_cut_exposure(
    ctx: &ReducerContext,
    character_id: u64,
    severity: f32,
    surgery_check: f32,
    token: &str,
    contracted_at: u64,
) -> Result<(), String> {
    if severity <= 0.0 {
        return Ok(());
    }
    let immunity = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(3.0, |a| a.immunity);
    let residual = (1.0 - (surgery_check / 5.0).clamp(0.0, 1.0) * 0.8) * severity.clamp(0.0, 1.0);
    for disease_id in [DiseaseId::Tetanus, DiseaseId::Erysipelas] {
        let definition = disease::definition(disease_id);
        let seed = disease::outbreak_exposure_seed(
            character_id,
            &format!("standing-cut-{token}-{disease_id:?}"),
        );
        if disease::acquisition_succeeds(seed, definition, immunity, 0.0, residual) {
            ctx.db.infection_episode().insert(InfectionEpisodeRow {
                id: 0,
                character_id,
                disease_id: disease_key(disease_id).to_owned(),
                contracted_at,
                ruleset_version: physiology::PHYSIOLOGY_RULESET_VERSION,
                phenotype_key_version: physiology::PHENOTYPE_KEY_VERSION,
            });
        }
    }
    Ok(())
}

/// Create or reset the diagnostic party used by the default development seed.
/// It includes a healthy physician and patients with staggered disease ages.
pub(crate) fn seed_sick_character(ctx: &ReducerContext) -> Result<(), String> {
    const SICK_CHARACTER_ID: u64 = 9_999_999_999_999_998;
    const PHYSICIAN_ID: u64 = 9_999_999_999_999_997;
    const AMBIGUOUS_PHYSICIAN_ID: u64 = 9_999_999_999_999_989;
    const DAY: u64 = 1_440;
    const FIXTURE_NOW: u64 = 60 * DAY;
    const PATIENTS: [(u64, &str, DiseaseId, u64); 8] = [
        (
            SICK_CHARACTER_ID,
            "Sick Demo",
            DiseaseId::Influenza,
            2 * DAY,
        ),
        (
            9_999_999_999_999_996,
            "Patient B",
            DiseaseId::Dysentery,
            3 * DAY,
        ),
        (
            9_999_999_999_999_995,
            "Patient C",
            DiseaseId::Typhus,
            8 * DAY,
        ),
        (
            9_999_999_999_999_994,
            "Patient D",
            DiseaseId::Tetanus,
            10 * DAY,
        ),
        (
            9_999_999_999_999_993,
            "Patient E",
            DiseaseId::Erysipelas,
            5 * DAY,
        ),
        (
            9_999_999_999_999_992,
            "Patient F",
            DiseaseId::Smallpox,
            12 * DAY,
        ),
        (
            9_999_999_999_999_991,
            "Patient G",
            DiseaseId::Plague,
            6 * DAY,
        ),
        (
            9_999_999_999_999_990,
            "Patient H",
            DiseaseId::Consumption,
            50 * DAY,
        ),
    ];

    for (id, name) in [
        (PHYSICIAN_ID, "Physician Demo"),
        (AMBIGUOUS_PHYSICIAN_ID, "Physician Demo (Physiology 3)"),
    ]
    .into_iter()
    .chain(PATIENTS.iter().map(|(id, name, _, _)| (*id, *name)))
    {
        if ctx.db.character().id().find(id).is_none() {
            crate::character::insert_new_character(ctx, name.into(), id, false)?;
        }
        let mut character_time = ctx
            .db
            .character_time()
            .character_id()
            .find(id)
            .ok_or_else(|| format!("{name} is missing time data"))?;
        character_time.minutes = character_time.minutes.max(FIXTURE_NOW);
        ctx.db
            .character_time()
            .character_id()
            .update(character_time);
    }

    let fixture_ids = [PHYSICIAN_ID, AMBIGUOUS_PHYSICIAN_ID]
        .into_iter()
        .chain(PATIENTS.iter().map(|(id, _, _, _)| *id))
        .collect::<Vec<_>>();
    for administration in ctx
        .db
        .physiology_administration()
        .iter()
        .filter(|administration| fixture_ids.contains(&administration.patient_id))
        .collect::<Vec<_>>()
    {
        ctx.db
            .physiology_administration()
            .id()
            .delete(administration.id);
    }

    for (id, _, _, _) in PATIENTS.iter().skip(1) {
        crate::strategic::attach_seeded_party_member(ctx, SICK_CHARACTER_ID, *id, "Patient")?;
    }
    crate::strategic::attach_seeded_party_member(
        ctx,
        SICK_CHARACTER_ID,
        PHYSICIAN_ID,
        "Physician",
    )?;
    crate::strategic::attach_seeded_party_member(
        ctx,
        SICK_CHARACTER_ID,
        AMBIGUOUS_PHYSICIAN_ID,
        "Physician",
    )?;

    let mut physician_skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(PHYSICIAN_ID)
        .ok_or_else(|| "Physician Demo is missing skill data".to_string())?;
    physician_skills.physiology_hours = 1_000_000.0;
    physician_skills.surgery_hours = 0.0;
    physician_skills.knife_hours = 1_000_000.0;
    physician_skills.tailoring_hours = 1_000_000.0;
    ctx.db
        .character_skills()
        .character_id()
        .update(physician_skills);
    let mut physician_attributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(PHYSICIAN_ID)
        .ok_or_else(|| "Physician Demo is missing attributes".to_string())?;
    physician_attributes.intelligence = 5.0;
    physician_attributes.instinct = 5.0;
    ctx.db
        .character_attributes()
        .character_id()
        .update(physician_attributes);

    let mut ambiguous_physician_skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(AMBIGUOUS_PHYSICIAN_ID)
        .ok_or_else(|| "Physiology 3 physician is missing skill data".to_string())?;
    ambiguous_physician_skills.physiology_hours = 7_500.0;
    ctx.db
        .character_skills()
        .character_id()
        .update(ambiguous_physician_skills);

    for (id, _, disease_id, age) in PATIENTS {
        for episode in ctx
            .db
            .infection_episode()
            .character_id()
            .filter(id)
            .collect::<Vec<_>>()
        {
            ctx.db.infection_episode().id().delete(episode.id);
        }
        ctx.db.infection_episode().insert(InfectionEpisodeRow {
            id: 0,
            character_id: id,
            disease_id: disease_key(disease_id).to_owned(),
            contracted_at: FIXTURE_NOW - age,
            ruleset_version: physiology::PHYSIOLOGY_RULESET_VERSION,
            phenotype_key_version: physiology::PHENOTYPE_KEY_VERSION,
        });
        crate::capability::refresh_character_capability(ctx, id)?;
    }
    crate::capability::refresh_character_capability(ctx, PHYSICIAN_ID)?;
    crate::capability::refresh_character_capability(ctx, AMBIGUOUS_PHYSICIAN_ID)?;

    // Patient H is the longitudinal notebook fixture: a week of shared
    // observation, a one-day party absence, and three distinct courses make
    // cadence, gaps, treatment windows, and response visible after a fresh seed.
    let patient_h = 9_999_999_999_999_990;
    let history_start = FIXTURE_NOW - 7 * DAY;
    let absence_start = FIXTURE_NOW - 5 * DAY;
    let absence_end = FIXTURE_NOW - 4 * DAY;
    let observer_band = |character_id| {
        ctx.db
            .character_capability()
            .character_id()
            .find(character_id)
            .map_or(0, |capability| {
                capability.physiology.round().clamp(0.0, 5.0) as u8
            })
    };
    let fixture_spans = ctx
        .db
        .physiology_presence_span()
        .iter()
        .filter(|span| {
            (span.low_id == patient_h
                && matches!(span.high_id, PHYSICIAN_ID | AMBIGUOUS_PHYSICIAN_ID))
                || (span.high_id == patient_h
                    && matches!(span.low_id, PHYSICIAN_ID | AMBIGUOUS_PHYSICIAN_ID))
        })
        .collect::<Vec<_>>();
    for span in fixture_spans {
        ctx.db.physiology_presence_span().id().delete(span.id);
    }
    for physician_id in [PHYSICIAN_ID, AMBIGUOUS_PHYSICIAN_ID] {
        let low_id = physician_id.min(patient_h);
        let high_id = physician_id.max(patient_h);
        let low_observer_band = observer_band(low_id);
        let high_observer_band = observer_band(high_id);
        for (started_at, ended_at) in [(history_start, Some(absence_start)), (absence_end, None)] {
            ctx.db
                .physiology_presence_span()
                .insert(crate::social::PhysiologyPresenceSpan {
                    id: 0,
                    low_id,
                    high_id,
                    started_at,
                    ended_at,
                    low_observer_band,
                    high_observer_band,
                });
        }
    }
    let key_version = physiology_key(ctx)?.version;
    for (administered_at, stopped_at, dose) in [
        (
            FIXTURE_NOW - 6 * DAY,
            Some(FIXTURE_NOW - 5 * DAY),
            physiology::DoseMilliunits::try_new(750).unwrap(),
        ),
        (
            FIXTURE_NOW - 4 * DAY,
            Some(FIXTURE_NOW - 3 * DAY),
            physiology::DoseMilliunits::STANDARD,
        ),
        (
            FIXTURE_NOW - 2 * DAY,
            None,
            physiology::DoseMilliunits::try_new(1_250).unwrap(),
        ),
    ] {
        let (sensitivity_bps, adverse_bps) =
            private_variation(ctx, patient_h, administered_at, "oral_rehydration_draught")?;
        ctx.db
            .physiology_administration()
            .insert(PhysiologyAdministration {
                id: 0,
                patient_id: patient_h,
                preparation_id: "oral_rehydration_draught".into(),
                profile_version: 1,
                route: InterventionRoute::Oral,
                dose_milliunits: dose.get(),
                region: None,
                administered_at,
                stopped_at,
                sensitivity_bps,
                adverse_bps,
                ruleset_version: physiology::PHYSIOLOGY_RULESET_VERSION,
                phenotype_key_version: key_version,
            });
    }
    crate::filth::seed_demo(ctx, SICK_CHARACTER_ID, 9_999_999_999_999_996)?;
    Ok(())
}

/// Typed purchasing for existing, concrete preparations. This reducer does not
/// craft, compose, diagnose, or choose an effect from disease identity.
#[reducer]
pub fn purchase_from_herbalist(
    ctx: &ReducerContext,
    patient_id: u64,
    settlement_id: String,
    item_ids: Vec<String>,
    quantities: Vec<u32>,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, patient_id)?;
    crate::strategic::require_settlement_service(
        ctx,
        &settlement_id,
        adventuresim_world_schema::SettlementService::Herbalist,
    )?;
    let patient = crate::require_living_character(ctx, patient_id)?;
    if patient.current_settlement_id.as_deref() != Some(&settlement_id) {
        return Err("Patient must be at this herbalist's settlement".into());
    }
    if item_ids.len() != quantities.len() || item_ids.is_empty() {
        return Err("Preparation purchase entries must be aligned".into());
    }
    let economy = ctx
        .db
        .settlement()
        .id()
        .find(settlement_id.clone())
        .ok_or("Settlement not found")?
        .economy;
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(patient_id)
        .map_or(0, |row| row.minutes);
    let problem_effects = crate::local_problem::settlement_effects(ctx, &settlement_id, minute);
    let mut cost = 0u64;
    for (item_id, quantity) in item_ids.iter().zip(&quantities) {
        if *quantity == 0 || *quantity > 100 {
            return Err("Preparation purchase quantity must be between 1 and 100".into());
        }
        let definition = ctx
            .db
            .item()
            .id()
            .find(item_id)
            .ok_or("Herbalist item not found")?;
        let permitted = match definition.kind {
            crate::ItemKind::Ingredient => true,
            crate::ItemKind::Medication => physiology::intervention_profile(item_id, 1).is_some(),
            _ => false,
        };
        if !permitted
            || !adventuresim_core::settlement_economy::storefront_stocks(
                &economy,
                adventuresim_core::settlement_economy::Storefront::Herbalist,
                item_id,
                crate::item::economy_catalog_kind(definition.kind),
            )
        {
            return Err("This herbalist does not stock that item".into());
        }
        let base = adventuresim_core::strategic_economy::merchant_buy_price(
            definition.base_value.unwrap_or(1),
        );
        let unit = adventuresim_core::local_problem::adjust_price(base, problem_effects.buy_bps);
        cost = cost.saturating_add(u64::from(unit) * u64::from(*quantity));
    }
    crate::strategic::consume_personal_gold(ctx, patient_id, cost)?;
    for (item_id, quantity) in item_ids.iter().zip(&quantities) {
        // The shared helper keeps medication individual while creating one
        // fungible ingredient stack with the requested quantity.
        crate::item::add_inventory_item_checked(ctx, patient_id, item_id, *quantity)?
            .ok_or("Herbalist purchase did not create inventory")?;
    }
    Ok(())
}

#[cfg(test)]
mod herbalist_purchase_source_tests {
    #[test]
    fn purchase_adds_requested_quantity_once_through_kind_aware_helper() {
        let source = crate::production_source(include_str!("disease.rs"));
        let purchase = source
            .split("pub fn purchase_from_herbalist")
            .nth(1)
            .unwrap();
        let body = purchase.split("#[cfg(test)]").next().unwrap();
        assert!(body.contains("add_inventory_item_checked(ctx, patient_id, item_id, *quantity)"));
        assert!(!body.contains("for _ in 0..*quantity"));
    }

    #[test]
    fn interval_authority_assembles_community_contact_and_blood_exposure() {
        let source = crate::production_source(include_str!("disease.rs"));
        let plan = source
            .split("pub fn plan_party_disease_interval")
            .nth(1)
            .unwrap()
            .split("fn party_contact_episodes_through")
            .next()
            .unwrap();
        assert!(plan.contains("settlement_outbreak"));
        assert!(plan.contains("local_problem_authority"));
        assert!(plan.contains("blood_exposure_attempts_through"));
        assert!(plan.contains("resolve_acquisition_timeline"));
        let clip = source
            .split("fn clip_elapsed_for_disease_planned")
            .nth(1)
            .unwrap()
            .split("pub fn clip_elapsed_for_disease")
            .next()
            .unwrap();
        assert!(clip.contains("infection_occurs_through"));
    }

    #[test]
    fn interval_plan_prefetches_private_presence_through_both_indexes() {
        let source = crate::production_source(include_str!("disease.rs"));
        let helper = source
            .split("fn bounded_physiology_spans")
            .nth(1)
            .unwrap()
            .split("pub(crate) fn party_physiology_check_at")
            .next()
            .unwrap();
        assert!(helper.contains("presence_low_id()"));
        assert!(helper.contains("presence_high_id()"));
        assert!(helper.contains("insert_unique_bounded"));
        assert!(helper.contains("raw_rows > MAX_PARTY_INTERVAL_SPANS.saturating_mul(2)"));
        assert!(!helper.contains("physiology_presence_span().iter()"));

        let point = source
            .split("fn try_party_physiology_check_at")
            .nth(1)
            .unwrap()
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(point.contains("bounded_physiology_spans(ctx, &[character_id])?"));
    }

    #[test]
    fn shared_interval_plan_is_bounded_order_stable_and_used_by_all_party_time_paths() {
        let disease = crate::production_source(include_str!("disease.rs"));
        let plan = disease
            .split("pub fn plan_party_disease_interval")
            .nth(1)
            .unwrap()
            .split("fn party_contact_episodes_through")
            .next()
            .unwrap();
        assert!(plan.contains("ids.sort_unstable()"));
        assert!(plan.contains("ids.dedup()"));
        assert!(plan.contains("MAX_PARTY_INTERVAL_SPANS"));
        assert!(plan.contains("MAX_PARTY_INTERVAL_WORK"));
        assert!(plan.contains("horizons"));
        assert!(plan.contains("resolve_acquisition_timeline"));

        let time = crate::production_source(include_str!("time.rs"));
        let rest = time.split("pub fn rest_at_camp").nth(1).unwrap();
        assert!(rest.contains("plan_party_disease_interval"));
        assert!(rest.contains("preview_elapsed_for_disease_in_plan"));
        assert!(rest.contains("clip_elapsed_for_disease_in_plan"));

        let journey = crate::production_source(include_str!("strategic/journey_camp.rs"));
        let movement = journey.split("fn advance_party_movement").nth(1).unwrap();
        assert!(movement.contains("plan_party_disease_interval"));
        assert!(movement.contains("preview_travel_time_in_plan"));
        assert!(movement.contains("advance_travel_time_in_plan"));

        let surgery = crate::production_source(include_str!("surgery.rs"));
        let treatment = surgery.split("fn align_and_advance").nth(1).unwrap();
        assert!(treatment.contains("plan_party_disease_interval"));
        assert!(treatment.contains("preview_elapsed_for_disease_in_plan"));
        assert!(treatment.contains("advance_character_wait_time_in_plan"));
    }
}

#[cfg(test)]
mod fantastic_differential_tests {
    use super::*;

    #[test]
    fn visible_sequence_score_is_independent_of_hidden_cause() {
        let visible = vec![
            VisibleHumourReading {
                observed_at: 100,
                mix: [0.12, 0.45, 0.18, 0.25],
            },
            VisibleHumourReading {
                observed_at: 1_540,
                mix: [0.10, 0.38, 0.16, 0.36],
            },
            VisibleHumourReading {
                observed_at: 2_980,
                mix: [0.08, 0.28, 0.14, 0.50],
            },
        ];
        let first = visible_sequence_fit(&visible, DiseaseId::Mahrdruck);
        let copied_from_an_unrelated_patient = visible.clone();
        let second = visible_sequence_fit(&copied_from_an_unrelated_patient, DiseaseId::Mahrdruck);
        assert_eq!(first, second);
    }

    #[test]
    fn one_or_too_short_visible_sequence_has_no_longitudinal_bonus() {
        let one = [VisibleHumourReading {
            observed_at: 5_000,
            mix: expected_public_mix(DiseaseId::Mahrdruck, 4 * 1_440),
        }];
        assert_eq!(visible_sequence_fit(&one, DiseaseId::Mahrdruck), 0.0);

        let too_short = [
            VisibleHumourReading {
                observed_at: 5_000,
                mix: expected_public_mix(DiseaseId::Mahrdruck, 4 * 1_440),
            },
            VisibleHumourReading {
                observed_at: 5_030,
                mix: expected_public_mix(DiseaseId::Mahrdruck, 4 * 1_440 + 30),
            },
            VisibleHumourReading {
                observed_at: 5_060,
                mix: expected_public_mix(DiseaseId::Mahrdruck, 4 * 1_440 + 60),
            },
        ];
        assert_eq!(
            longitudinal_sequence_bonus(&too_short, 60, 5, DiseaseId::Mahrdruck),
            0.0
        );

        for id in [
            DiseaseId::Mahrdruck,
            DiseaseId::ShroudFever,
            DiseaseId::Bilwisschuss,
            DiseaseId::Kobeldunst,
        ] {
            let pattern = disease::diagnostic_pattern(id);
            assert!(pattern.minimum_observation_minutes >= 1_440);
            assert!(pattern.longitudinal_weight > 0.0);
        }
        assert_eq!(
            disease::diagnostic_pattern(DiseaseId::Influenza).longitudinal_weight,
            0.0
        );
    }

    #[test]
    fn coordinated_public_sequences_rank_their_authored_pattern() {
        let id = DiseaseId::Kobeldunst;
        let visible = [6 * 60, 21 * 60, 36 * 60]
            .map(|age| VisibleHumourReading {
                observed_at: age,
                mix: expected_public_mix(id, age),
            })
            .to_vec();
        let own = visible_sequence_fit(&visible, id);
        let ordinary = visible_sequence_fit(&visible, DiseaseId::Dysentery);
        assert!(own > ordinary, "{own} <= {ordinary}");
    }

    #[test]
    fn canonical_disease_keys_round_trip_for_every_authored_disease() {
        for definition in adventuresim_core::disease::STARTER_DISEASES {
            let key = disease_key(definition.id);
            assert_eq!(parse_id(key), Ok(definition.id), "{key}");
        }
        assert_eq!(disease_key(DiseaseId::ShroudFever), "shroud_fever");
    }

    #[test]
    fn fantastic_evidence_hooks_reference_authored_evidence_and_sites() {
        let source = include_str!("../../../content/quests/investigation.yaml");
        for definition in adventuresim_core::disease::STARTER_DISEASES {
            for hook in disease::evidence_hooks(definition.id) {
                assert!(
                    source.contains(&format!(r#""id":"{}""#, hook.evidence_id)),
                    "{}",
                    hook.evidence_id
                );
                if let Some(site) = hook.required_site {
                    assert!(source.contains(&format!(r#""id":"{site}""#)), "{site}");
                }
            }
        }
    }

    #[test]
    fn disease_terminal_preview_is_side_effect_free() {
        let source = crate::production_source(include_str!("disease.rs"));
        let preview = source
            .split("fn preview_disease_boundary_planned")
            .nth(1)
            .and_then(|tail| {
                tail.split("pub fn preview_disease_terminal_boundary")
                    .next()
            })
            .expect("disease terminal preview");
        for forbidden in [
            "persist_acquisition_episodes",
            "persist_",
            "blood_exposure_attempts_through",
            "cursor",
            "notice(",
            ".insert(",
            ".update(",
            ".delete(",
        ] {
            assert!(!preview.contains(forbidden), "preview contains {forbidden}");
        }
    }

    #[test]
    fn medicinal_interventions_use_nonemergency_treatment_decisions() {
        let source = crate::production_source(include_str!("disease.rs"));
        let relationship = source
            .split("pub(crate) fn require_intervention_relationship")
            .nth(1)
            .and_then(|tail| tail.split("fn commit_terminal_at_boundary").next())
            .expect("intervention relationship");
        assert!(relationship.contains("contextual_nonemergency_treatment_decision"));
        assert!(relationship.contains("ContextualActionDecision::Refused"));
        assert!(relationship.contains("ContextualActionDecision::Unavailable"));
        assert!(!relationship.contains("incapacitat"));
        assert!(!relationship.contains("party_id =="));
    }
}
