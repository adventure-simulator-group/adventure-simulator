use super::*;
use adventuresim_tactical_core::combat_config::IncapacitationForecastConfig;

/// Server-local sampled history. Only its presentation forecast is replicated.
#[derive(Default)]
pub(crate) struct ConditionTrend {
    previous: Option<TacticalIncapacitationSources>,
    rates: TacticalIncapacitationSources,
}

impl ConditionTrend {
    pub(super) fn update(
        &mut self,
        current: TacticalIncapacitationSources,
        elapsed: f32,
        config: IncapacitationForecastConfig,
    ) -> TacticalIncapacitationSources {
        if elapsed <= 0.0 {
            return TacticalIncapacitationSources::default();
        }
        let Some(previous) = self.previous.replace(current) else {
            return TacticalIncapacitationSources::default();
        };
        let blend = -(-elapsed / config.trend_response_seconds).exp_m1();
        let mut forecast = TacticalIncapacitationSources::default();
        macro_rules! source {
            ($field:ident) => {
                self.rates.$field +=
                    ((current.$field - previous.$field) / elapsed - self.rates.$field) * blend;
                forecast.$field = (self.rates.$field * config.horizon_seconds).max(0.0);
            };
        }
        source!(pain);
        source!(acute_trauma);
        source!(blood_loss);
        source!(fear);
        source!(fatigue);
        source!(hunger);
        source!(thirst);
        source!(thermal);
        source!(imbalance);
        source!(encumbrance);
        forecast
    }
}

/// Bleeding is known ongoing work, not an inferred trend. Show its consequence
/// immediately, including on the tick an open or internal wound first appears.
pub(super) fn project_bleeding(
    state: &mut TacticalCombatState,
    sources: TacticalIncapacitationSources,
    wounds: Option<&TacticalWounds>,
    config: IncapacitationForecastConfig,
) {
    let future_loss = advance_combat_bleeding(
        state.blood_loss_fraction,
        wounds.map_or(&[], |wounds| wounds.0.as_slice()),
        config.horizon_seconds,
    );
    let remaining = (state.starting_blood_fraction - future_loss).clamp(0.0, 1.0);
    state.projected_increase.blood_loss =
        (blood_loss_incapacitation(remaining, 1.0) - sources.blood_loss).max(0.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_source_forecasts_growth_but_not_recovery_or_enrollment() {
        let mut trend = ConditionTrend::default();
        let config = IncapacitationForecastConfig::default();
        let initial = TacticalIncapacitationSources::default();
        assert_eq!(trend.update(initial, 0.1, config).total(), 0.0);
        let increased = TacticalIncapacitationSources {
            pain: 0.01,
            acute_trauma: 0.01,
            blood_loss: 0.01,
            fear: 0.01,
            fatigue: 0.01,
            hunger: 0.01,
            thirst: 0.01,
            thermal: 0.01,
            imbalance: 0.01,
            encumbrance: 0.01,
        };
        let predicted = trend.update(increased, 0.1, config);
        assert!(predicted.pain > 0.0 && predicted.encumbrance > 0.0);
        assert!((predicted.total() - predicted.pain * 10.0).abs() < 0.0001);
        assert_eq!(trend.update(initial, 0.1, config).total(), 0.0);
    }

    #[test]
    fn wound_forecast_is_immediate_and_does_not_change_actual_condition() {
        let mut state = TacticalCombatState::default();
        let sources = state.incapacitation_sources(0.0, 3.0);
        let wounds = TacticalWounds(vec![CombatWound {
            body_part: BodyPart::Chest,
            kind: CombatWoundKind::Internal,
            blood_fraction_per_second: 0.003,
        }]);
        project_bleeding(
            &mut state,
            sources,
            Some(&wounds),
            IncapacitationForecastConfig::default(),
        );
        assert!((state.projected_increase.blood_loss - 0.02).abs() < 0.0001);
        assert_eq!(state.incapacitation, 0.0);
        assert_eq!(state.blood_loss_fraction, 0.0);
    }

    #[test]
    fn steady_rates_use_simulation_time_and_the_authored_horizon() {
        let simulate = |step: f32, ticks: usize| {
            let mut trend = ConditionTrend::default();
            let config = IncapacitationForecastConfig::default();
            trend.update(default(), step, config);
            let mut forecast = TacticalIncapacitationSources::default();
            for tick in 1..=ticks {
                forecast = trend.update(
                    TacticalIncapacitationSources {
                        fatigue: tick as f32 * step * 0.01,
                        ..default()
                    },
                    step,
                    config,
                );
            }
            forecast.fatigue
        };
        let coarse = simulate(0.1, 100);
        let fine = simulate(0.01, 1000);
        assert!((coarse - fine).abs() < 0.00001);
        assert!((fine - 0.02).abs() < 0.00001);
        let mut invalid = IncapacitationForecastConfig {
            horizon_seconds: 0.0,
            ..default()
        };
        assert!(invalid.validate().is_err());
        invalid.horizon_seconds = f32::NAN;
        assert!(invalid.validate().is_err());
    }
}
