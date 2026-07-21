//! Shared deterministic surgery rules used by reducers and server rendering.

pub const SELF_TREATMENT_PENALTY: f32 = 2.5;
pub const MINUTES_PER_DAY: u64 = 1_440;
pub const UNTREATED_CUT_DETERIORATION_PER_DAY: f32 = 0.025;
pub const UNTREATED_CUT_BLOOD_LOSS_PER_DAY: f32 = 0.08;
pub const PROJECTILE_KIT_DC_THRESHOLD: f32 = 1.0;

pub fn effective_skill(skill: f32, self_treatment: bool) -> f32 {
    (skill
        - if self_treatment {
            SELF_TREATMENT_PENALTY
        } else {
            0.0
        })
    .max(0.0)
}

/// Compose the procedure's complete leaf checks and apply self-treatment once.
pub fn procedure_skill(
    procedure: &str,
    anatomy: f32,
    knife: f32,
    tailoring: f32,
    self_treatment: bool,
) -> f32 {
    let composite = match procedure {
        "extract" => (anatomy + knife) * 0.5,
        "stitch" => (anatomy + tailoring) * 0.5,
        _ => anatomy,
    };
    effective_skill(composite, self_treatment)
}

pub fn procedure_duration_minutes(procedure: &str, skill: f32, dc: f32) -> u64 {
    let base = match procedure {
        "bandage" => 30.0,
        "stitch" => 60.0,
        "splint" | "remove-splint" => 45.0,
        "extract" => 30.0 + dc.max(0.0) * 10.0,
        _ => 30.0,
    };
    (base - skill.max(0.0) * 5.0).max(10.0).ceil() as u64
}

/// Extraction difficulty is correlated with the complete applied hit. A very
/// shallow low-energy projectile can be DC 0, while the scale remains uncapped.
pub fn projectile_extraction_dc(total_hit_damage: f32, depth: f32) -> f32 {
    ((total_hit_damage.max(0.0) - 0.05) * 8.0 + depth.max(0.0) - 0.5).max(0.0)
}

pub fn extraction_requires_surgery_kit(dc: f32) -> bool {
    dc > PROJECTILE_KIT_DC_THRESHOLD
}

pub fn standing_infection_multiplier(bandaged: bool, stitched: bool, stitch_quality: f32) -> f32 {
    (if bandaged { 0.40 } else { 1.0 })
        * if stitched {
            (1.0 - stitch_quality.clamp(0.0, 5.0) * 0.12).max(0.20)
        } else {
            1.0
        }
}

pub fn untreated_cut_progress(starting_cut: f32, days: f32) -> (f32, f32) {
    let starting_cut = starting_cut.clamp(0.0, 1.0);
    let days = days.max(0.0);
    let days_to_cap = (1.0 - starting_cut) / UNTREATED_CUT_DETERIORATION_PER_DAY;
    let growing_days = days.min(days_to_cap.max(0.0));
    let cut_days = starting_cut * growing_days
        + 0.5 * UNTREATED_CUT_DETERIORATION_PER_DAY * growing_days * growing_days
        + (days - growing_days).max(0.0);
    (
        (starting_cut + UNTREATED_CUT_DETERIORATION_PER_DAY * days).min(1.0),
        cut_days,
    )
}

#[derive(Clone, Debug)]
pub struct BloodInterval {
    pub elapsed: u64,
    pub blood_fraction: f32,
    pub open_cuts: Vec<f32>,
    pub cut_days: Vec<f32>,
    pub terminal: bool,
}

pub fn simulate_blood_interval(
    starting_blood: f32,
    open_cuts: &[f32],
    requested: u64,
    recovery_per_day: f32,
) -> BloodInterval {
    let mut blood = starting_blood.clamp(0.0, 1.0);
    let mut cuts = open_cuts.to_vec();
    let mut cut_days = vec![0.0; cuts.len()];
    let minute_days = 1.0 / MINUTES_PER_DAY as f32;
    let mut elapsed = 0;
    for _ in 0..requested {
        let mut loss = 0.0;
        for (index, cut) in cuts.iter_mut().enumerate() {
            if *cut <= 0.0 {
                continue;
            }
            let (next, exposure) = untreated_cut_progress(*cut, minute_days);
            *cut = next;
            cut_days[index] += exposure;
            loss += UNTREATED_CUT_BLOOD_LOSS_PER_DAY * exposure;
        }
        blood = (blood + recovery_per_day.max(0.0) * minute_days - loss).clamp(0.0, 1.0);
        elapsed += 1;
        if blood <= 0.10 {
            return BloodInterval {
                elapsed,
                blood_fraction: blood,
                open_cuts: cuts,
                cut_days,
                terminal: true,
            };
        }
    }
    BloodInterval {
        elapsed,
        blood_fraction: blood,
        open_cuts: cuts,
        cut_days,
        terminal: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_dc_reaches_zero_and_remains_uncapped() {
        assert_eq!(projectile_extraction_dc(0.02, 0.0), 0.0);
        assert!(projectile_extraction_dc(0.30, 0.8) > projectile_extraction_dc(0.10, 0.8));
        assert!(projectile_extraction_dc(1.0, 2.0) > 5.0);
    }

    #[test]
    fn only_projectiles_above_dc_one_require_a_kit() {
        assert!(!extraction_requires_surgery_kit(0.0));
        assert!(!extraction_requires_surgery_kit(1.0));
        assert!(extraction_requires_surgery_kit(1.000_1));
        assert!(extraction_requires_surgery_kit(8.0));
    }

    #[test]
    fn self_treatment_penalty_is_shared() {
        assert!(effective_skill(2.0, false) > effective_skill(4.0, true));
    }

    #[test]
    fn procedure_composition_distinguishes_extraction_from_stitching() {
        assert_eq!(procedure_skill("extract", 5.0, 5.0, 0.0, false), 5.0);
        assert_eq!(procedure_skill("stitch", 5.0, 5.0, 0.0, false), 2.5);
        assert!(procedure_skill("extract", 5.0, 5.0, 0.0, false) >= 4.0);
        assert!(procedure_skill("stitch", 5.0, 5.0, 0.0, false) < 4.0);
        assert_eq!(procedure_skill("extract", 5.0, 5.0, 0.0, true), 2.5);
    }

    #[test]
    fn bleeding_recovery_is_chunk_invariant() {
        let whole = simulate_blood_interval(0.82, &[0.10], 3_000, 0.01);
        let first = simulate_blood_interval(0.82, &[0.10], 1_000, 0.01);
        let second = simulate_blood_interval(first.blood_fraction, &first.open_cuts, 2_000, 0.01);
        assert!((whole.blood_fraction - second.blood_fraction).abs() < 0.000_001);
        assert!((whole.open_cuts[0] - second.open_cuts[0]).abs() < 0.000_001);
        assert!((whole.cut_days[0] - first.cut_days[0] - second.cut_days[0]).abs() < 0.000_001);
    }

    #[test]
    fn terminal_boundary_is_chunk_invariant() {
        let whole = simulate_blood_interval(0.13, &[0.45], MINUTES_PER_DAY, 0.0);
        let split = whole.elapsed / 2;
        let first = simulate_blood_interval(0.13, &[0.45], split, 0.0);
        let second = simulate_blood_interval(
            first.blood_fraction,
            &first.open_cuts,
            MINUTES_PER_DAY - split,
            0.0,
        );
        assert!(whole.terminal);
        assert_eq!(whole.elapsed, first.elapsed + second.elapsed);
        assert!(second.terminal);
    }

    #[test]
    fn wound_protection_reduces_standing_exposure() {
        let open = standing_infection_multiplier(false, false, 0.0);
        let bandaged = standing_infection_multiplier(true, false, 0.0);
        let stitched = standing_infection_multiplier(true, true, 4.0);
        assert!(open > bandaged && bandaged > stitched);
    }
}
