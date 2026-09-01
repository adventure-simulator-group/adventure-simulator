use adventuresim_tactical_core::prelude::*;
use bevy_egui::egui::Color32;

pub(super) fn incapacitation_wheel_segments(
    sources: TacticalIncapacitationSources,
) -> [(f32, Color32); 9] {
    [
        (sources.pain, Color32::from_rgb(0xd9, 0x73, 0xa2)),
        (sources.blood_loss, Color32::from_rgb(0xc8, 0x47, 0x47)),
        (sources.fear, Color32::from_rgb(0x4f, 0x83, 0xcc)),
        (sources.fatigue, Color32::from_rgb(0x20, 0x20, 0x20)),
        (sources.hunger, Color32::from_rgb(0xb5, 0x7a, 0x35)),
        (sources.thirst, Color32::from_rgb(0x3f, 0x9f, 0xa8)),
        (sources.thermal, Color32::from_rgb(0x7d, 0x8e, 0xe8)),
        (sources.oxygen_debt, Color32::from_rgb(0x80, 0x80, 0x80)),
        (sources.imbalance, Color32::WHITE),
    ]
}

pub(super) fn combat_state_label(state: &TacticalCombatState, exhaustion: f32) -> String {
    if state.is_incapacitated() {
        format!(
            "INCAPACITATED | Blood loss {:.0}% | Exhaustion {:.0}% | Imbalance {:.0}%",
            state.blood_loss_fraction * 100.0,
            exhaustion * 100.0,
            state.imbalance * 100.0
        )
    } else {
        format!(
            "Active | Blood loss {:.0}% | Exhaustion {:.0}% | Imbalance {:.0}%",
            state.blood_loss_fraction * 100.0,
            exhaustion * 100.0,
            state.imbalance * 100.0
        )
    }
}
