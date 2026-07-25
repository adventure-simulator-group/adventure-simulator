//! Shared deterministic surgery rules used by reducers and server rendering.

pub const SELF_TREATMENT_PENALTY: f32 = 2.5;
pub const MINUTES_PER_DAY: u64 = 1_440;
pub const UNTREATED_CUT_DETERIORATION_PER_DAY: f32 = 0.025;
pub const UNTREATED_CUT_BLOOD_LOSS_PER_DAY: f32 = 0.08;
pub const PROJECTILE_KIT_DC_THRESHOLD: f32 = 1.0;
/// Small visible contamination transferred by a successful bloody procedure.
pub const PROCEDURE_BLOOD_EXPOSURE_FILTH: u16 = 2;
/// Transfer from trained Knife hours into effective Surgery knowledge.
pub const KNIFE_SURGERY_CORRELATION: f32 = 0.25;
/// Transfer from trained Tailoring hours into effective Surgery knowledge.
pub const TAILORING_SURGERY_CORRELATION: f32 = 0.25;

/// Compute effective Surgery knowledge in one nonrecursive pass.
///
/// Only direct Surgery, Knife, and Tailoring hours are inputs. Transferred
/// knowledge never feeds back into any source skill.
pub fn effective_surgery_hours(
    direct_surgery_hours: f32,
    knife_hours: f32,
    tailoring_hours: f32,
) -> f32 {
    let valid_hours = |hours: f32| {
        if hours.is_finite() {
            hours.max(0.0)
        } else {
            0.0
        }
    };
    (valid_hours(direct_surgery_hours)
        + valid_hours(knife_hours) * KNIFE_SURGERY_CORRELATION
        + valid_hours(tailoring_hours) * TAILORING_SURGERY_CORRELATION)
        .min(crate::skill::Skill::Surgery.max_hours())
}

pub fn procedure_blood_exposure(procedure: &str, treating_other: bool) -> u16 {
    if treating_other && matches!(procedure, "bandage" | "stitch" | "extract") {
        PROCEDURE_BLOOD_EXPOSURE_FILTH
    } else {
        0
    }
}

pub fn effective_skill(skill: f32, self_treatment: bool) -> f32 {
    (skill
        - if self_treatment {
            SELF_TREATMENT_PENALTY
        } else {
            0.0
        })
    .max(0.0)
}

/// Cap an operative Surgery check by knowledge of the patient's species, then
/// apply the shared self-treatment penalty once.
pub fn procedure_skill(
    surgery_check: f32,
    species_knowledge_check: f32,
    self_treatment: bool,
) -> f32 {
    effective_skill(
        surgery_check.max(0.0).min(species_knowledge_check.max(0.0)),
        self_treatment,
    )
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
    fn only_blood_contact_procedures_contaminate_the_actor() {
        assert_eq!(
            procedure_blood_exposure("bandage", true),
            PROCEDURE_BLOOD_EXPOSURE_FILTH
        );
        assert_eq!(
            procedure_blood_exposure("stitch", true),
            PROCEDURE_BLOOD_EXPOSURE_FILTH
        );
        assert_eq!(
            procedure_blood_exposure("extract", true),
            PROCEDURE_BLOOD_EXPOSURE_FILTH
        );
        assert_eq!(procedure_blood_exposure("splint", true), 0);
        assert_eq!(procedure_blood_exposure("remove-splint", true), 0);
        assert_eq!(procedure_blood_exposure("bandage", false), 0);
    }

    #[test]
    fn self_treatment_penalty_is_shared() {
        assert!(effective_skill(2.0, false) > effective_skill(4.0, true));
    }

    #[test]
    fn surgery_hours_use_direct_and_one_pass_adjacent_skill_transfer() {
        assert_eq!(effective_surgery_hours(1_000.0, 800.0, 400.0), 1_300.0);
        assert_eq!(effective_surgery_hours(0.0, 1_000.0, 0.0), 250.0);
        assert_eq!(effective_surgery_hours(0.0, 0.0, 1_000.0), 250.0);
    }

    #[test]
    fn effective_surgery_hours_are_capped_at_mastery() {
        assert_eq!(
            effective_surgery_hours(4_500.0, 4_000.0, 4_000.0),
            crate::skill::Skill::Surgery.max_hours()
        );
        assert_eq!(effective_surgery_hours(f32::NAN, -10.0, 400.0), 100.0);
    }

    #[test]
    fn every_procedure_check_is_capped_by_species_knowledge() {
        assert_eq!(procedure_skill(4.5, 2.0, false), 2.0);
        assert_eq!(procedure_skill(1.5, 4.0, false), 1.5);
        assert_eq!(procedure_skill(5.0, 5.0, true), 2.5);
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
