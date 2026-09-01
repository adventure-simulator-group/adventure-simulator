use super::*;

pub(super) fn initiative_delay_seconds(
    random: &mut CombatRandom,
    instinct: f32,
    config: &AiOffenseConfig,
) -> f32 {
    let sampled = random.range_f32(
        config.initiative_delay_min_seconds,
        config.initiative_delay_max_seconds,
    );
    sampled * (3.0 / instinct.max(0.5)).clamp(0.6, 2.0)
}

pub(super) fn committed_threat_recognition_probability(attack_phase: f32, instinct: f32) -> f32 {
    let visible_windup = (attack_phase.clamp(0.0, 0.5) * 2.0).sqrt();
    (visible_windup * instinct.max(0.0) / 5.0).clamp(0.0, 1.0)
}

pub(super) fn below_preferred_long_weapon_measure(
    reach_metres: f32,
    preferred_measure_metres: f32,
    distance_metres: f32,
    long_weapon_threshold_metres: f32,
) -> bool {
    reach_metres >= long_weapon_threshold_metres && distance_metres < preferred_measure_metres
}
