use adventuresim_tactical_core::prelude::*;
use bevy_egui::egui::Color32;

pub(super) fn incapacitation_wheel_segments(
    sources: TacticalIncapacitationSources,
) -> [(f32, Color32); 9] {
    [
        (
            sources.pain + sources.acute_trauma,
            Color32::from_rgb(0xd9, 0x73, 0xa2),
        ),
        (sources.blood_loss, Color32::from_rgb(0xc8, 0x47, 0x47)),
        (sources.fear, Color32::from_rgb(0x4f, 0x83, 0xcc)),
        (sources.fatigue, Color32::from_rgb(0x20, 0x20, 0x20)),
        (sources.hunger, Color32::from_rgb(0xb5, 0x7a, 0x35)),
        (sources.thirst, Color32::from_rgb(0x3f, 0x9f, 0xa8)),
        (sources.thermal, Color32::from_rgb(0x7d, 0x8e, 0xe8)),
        (sources.imbalance, Color32::WHITE),
        (
            sources.encumbrance,
            Color32::from_rgba_unmultiplied(160, 160, 160, 120),
        ),
    ]
}

/// Keep all current impairment visible before allocating remaining wheel
/// space to yellow forecasts. Forecasts never displace actual sources.
pub(super) fn forecast_wheel_segments(
    current: TacticalIncapacitationSources,
    forecast: TacticalIncapacitationSources,
) -> Vec<(f32, Color32)> {
    let available = (1.0 - current.total()).clamp(0.0, 1.0);
    let predicted = forecast.total().max(0.0);
    let scale = if predicted > 0.0 {
        (available / predicted).min(1.0)
    } else {
        0.0
    };
    incapacitation_wheel_segments(current)
        .into_iter()
        .zip(incapacitation_wheel_segments(forecast))
        .flat_map(|((actual, color), (increase, _))| {
            [
                (actual, color),
                (increase.max(0.0) * scale, Color32::YELLOW),
            ]
        })
        .collect()
}

pub(super) fn combat_state_label(state: &TacticalCombatState) -> String {
    if state.is_incapacitated() {
        format!(
            "INCAPACITATED | Blood loss {:.0}% | Fatigue {:.0}% | Imbalance {:.0}%",
            state.blood_loss_fraction * 100.0,
            state.fatigue * 100.0,
            state.imbalance * 100.0
        )
    } else {
        format!(
            "Active | Blood loss {:.0}% | Fatigue {:.0}% | Imbalance {:.0}%",
            state.blood_loss_fraction * 100.0,
            state.fatigue * 100.0,
            state.imbalance * 100.0
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::default;

    #[test]
    fn forecasts_are_yellow_and_cannot_hide_actual_impairment() {
        let current = TacticalIncapacitationSources {
            fatigue: 0.5,
            encumbrance: 0.3,
            ..default()
        };
        let forecast = TacticalIncapacitationSources {
            pain: 0.4,
            blood_loss: 0.2,
            ..default()
        };
        let segments = forecast_wheel_segments(current, forecast);
        let yellow: f32 = segments
            .iter()
            .filter(|(_, color)| *color == Color32::YELLOW)
            .map(|(v, _)| v)
            .sum();
        assert!((yellow - 0.2).abs() < 0.0001);
        let actual: f32 = segments.iter().step_by(2).map(|(v, _)| v).sum();
        assert!((actual - current.total()).abs() < 0.0001);
        assert!(
            incapacitation_wheel_segments(current)
                .iter()
                .all(|(_, color)| *color != Color32::YELLOW)
        );
        assert!(incapacitation_wheel_segments(current)[8].1.a() < 255);
        assert_ne!(
            incapacitation_wheel_segments(current)[8].1,
            incapacitation_wheel_segments(current)[3].1
        );
    }

    #[test]
    fn every_source_can_show_a_forecast_and_overflow_still_clips() {
        let forecast = TacticalIncapacitationSources {
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
        assert!(
            forecast_wheel_segments(default(), forecast)
                .iter()
                .skip(1)
                .step_by(2)
                .all(|(v, c)| *v > 0.0 && *c == Color32::YELLOW)
        );
        let unconscious = TacticalIncapacitationSources {
            blood_loss: 1.2,
            ..default()
        };
        assert!(
            forecast_wheel_segments(unconscious, forecast)
                .iter()
                .skip(1)
                .step_by(2)
                .all(|(v, _)| *v == 0.0)
        );
    }
}
