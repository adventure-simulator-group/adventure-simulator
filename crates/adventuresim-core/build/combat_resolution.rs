use std::{env, fs, path::Path};

use serde::Deserialize;

#[path = "../src/combat/fatigue_config.rs"]
mod fatigue_config;
use fatigue_config::CombatFatigueParameters;

#[derive(Deserialize)]
struct CombatBuildConfig {
    resolution: ResolutionValues,
    autoresolve: AutoresolveValues,
    ai: AiValues,
    movement: MovementValues,
}

#[derive(Deserialize)]
struct MovementValues {
    speeds_metres_per_second: MovementSpeedValues,
    motor: MovementMotorValues,
}

#[derive(Deserialize)]
struct MovementSpeedValues {
    raised_guard: f64,
    run: f64,
}

#[derive(Deserialize)]
struct MovementMotorValues {
    reference_ground_drive_force_newtons: f64,
    reference_quickstep_target_displacement_metres: f64,
    reference_leg_strength: f64,
    gravity_metres_per_second_squared: f64,
    traction_coefficient: f64,
}

#[derive(Deserialize)]
struct AiValues {
    ordinary: OrdinaryAiValues,
}

#[derive(Deserialize)]
struct OrdinaryAiValues {
    defense: AiDefenseValues,
    offense: AiOffenseValues,
}

#[derive(Deserialize)]
struct AiDefenseValues {
    dodge_chance: f64,
}

#[derive(Deserialize)]
struct AiOffenseValues {
    long_weapon_measure_threshold_metres: f64,
    melee_measure_reach_fraction: f64,
}

#[derive(Deserialize)]
struct ResolutionValues {
    fatigue: CombatFatigueParameters,
    armed_attack_energy_transfer: f64,
    stagger_resistance_joules_per_kg: f64,
}

#[derive(Deserialize)]
struct AutoresolveValues {
    combat_round_seconds: f64,
    formation_spacing_metres: f64,
    reference_melee_attack_seconds: f64,
    minimum_movement_speed_metres_per_second: f64,
    minimum_attack_interval_seconds: f64,
    minimum_melee_input_reflex: f64,
    melee_windup_seconds: f64,
    melee_reaction_delay_min_seconds: f64,
    melee_reaction_delay_max_seconds: f64,
    melee_reflex_window_seconds: f64,
    melee_initiative_delay_min_seconds: f64,
    melee_initiative_delay_max_seconds: f64,
    melee_cadence_jitter_seconds: f64,
    minimum_hit_precision: f64,
    maximum_hit_precision: f64,
    outnumbered_flanking: f64,
    ranged_defense_input_reflex: f64,
}

pub fn compile(root: &Path) {
    let path = root.join("content/tactical/combat.yaml");
    println!("cargo:rerun-if-changed={}", path.display());
    let text = fs::read_to_string(&path).expect("content/tactical/combat.yaml must exist");
    let values: CombatBuildConfig =
        serde_saphyr::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    values.validate(&path);
    let resolution = values.resolution;
    let autoresolve = values.autoresolve;
    let fatigue = format!("{:?}", resolution.fatigue);
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("combat_resolution_config.rs"),
        format!(
            "pub const EMBEDDED_COMBAT_RESOLUTION_PARAMETERS: CombatResolutionParameters = \
             CombatResolutionParameters {{ fatigue: {fatigue}, armed_attack_energy_transfer: {:?}_f32, \
             stagger_resistance_joules_per_kg: {:?}_f32 }};\n\
             pub const EMBEDDED_AUTORESOLVE_PARAMETERS: AutoresolveParameters = \
             AutoresolveParameters {{ combat_round_seconds: {:?}_f32, formation_spacing_metres: \
             {:?}_f32, reference_melee_attack_seconds: {:?}_f32, \
             minimum_movement_speed_metres_per_second: {:?}_f32, \
             guarded_movement_speed_metres_per_second: {:?}_f32, \
             melee_lunge_speed_metres_per_second: {:?}_f32, \
             melee_lunge_maximum_travel_metres: {:?}_f32, \
             reference_ground_drive_force_newtons: {:?}_f32, \
             reference_leg_strength: {:?}_f32, gravity_metres_per_second_squared: {:?}_f32, \
             traction_coefficient: {:?}_f32, \
             minimum_attack_interval_seconds: {:?}_f32, minimum_melee_input_reflex: {:?}_f32, \
             melee_windup_seconds: {:?}_f32, melee_reaction_delay_min_seconds: {:?}_f32, \
             melee_reaction_delay_max_seconds: {:?}_f32, melee_reflex_window_seconds: {:?}_f32, \
             melee_dodge_reaction_chance: {:?}_f32, \
             melee_initiative_delay_min_seconds: {:?}_f32, \
             melee_initiative_delay_max_seconds: {:?}_f32, \
             melee_cadence_jitter_seconds: {:?}_f32, \
             long_weapon_measure_threshold_metres: {:?}_f32, \
             melee_measure_reach_fraction: {:?}_f32, \
             minimum_hit_precision: {:?}_f32, maximum_hit_precision: {:?}_f32, \
             outnumbered_flanking: {:?}_f32, ranged_defense_input_reflex: {:?}_f32 }};\n",
            resolution.armed_attack_energy_transfer,
            resolution.stagger_resistance_joules_per_kg,
            autoresolve.combat_round_seconds,
            autoresolve.formation_spacing_metres,
            autoresolve.reference_melee_attack_seconds,
            autoresolve.minimum_movement_speed_metres_per_second,
            values.movement.speeds_metres_per_second.raised_guard,
            values.movement.speeds_metres_per_second.run,
            values
                .movement
                .motor
                .reference_quickstep_target_displacement_metres,
            values.movement.motor.reference_ground_drive_force_newtons,
            values.movement.motor.reference_leg_strength,
            values.movement.motor.gravity_metres_per_second_squared,
            values.movement.motor.traction_coefficient,
            autoresolve.minimum_attack_interval_seconds,
            autoresolve.minimum_melee_input_reflex,
            autoresolve.melee_windup_seconds,
            autoresolve.melee_reaction_delay_min_seconds,
            autoresolve.melee_reaction_delay_max_seconds,
            autoresolve.melee_reflex_window_seconds,
            values.ai.ordinary.defense.dodge_chance,
            autoresolve.melee_initiative_delay_min_seconds,
            autoresolve.melee_initiative_delay_max_seconds,
            autoresolve.melee_cadence_jitter_seconds,
            values
                .ai
                .ordinary
                .offense
                .long_weapon_measure_threshold_metres,
            values.ai.ordinary.offense.melee_measure_reach_fraction,
            autoresolve.minimum_hit_precision,
            autoresolve.maximum_hit_precision,
            autoresolve.outnumbered_flanking,
            autoresolve.ranged_defense_input_reflex,
        ),
    )
    .unwrap();
}

impl CombatBuildConfig {
    fn validate(&self, path: &Path) {
        self.resolution
            .fatigue
            .validate()
            .expect("invalid authored fatigue configuration");
        assert!(
            self.resolution.armed_attack_energy_transfer.is_finite()
                && (0.0..=1.0).contains(&self.resolution.armed_attack_energy_transfer)
                && self.resolution.armed_attack_energy_transfer > 0.0,
            "{}: resolution.armed_attack_energy_transfer must be in (0, 1]",
            path.display()
        );
        let positive = [
            self.resolution.stagger_resistance_joules_per_kg,
            self.autoresolve.combat_round_seconds,
            self.autoresolve.formation_spacing_metres,
            self.autoresolve.reference_melee_attack_seconds,
            self.autoresolve.minimum_movement_speed_metres_per_second,
            self.movement.speeds_metres_per_second.raised_guard,
            self.movement.speeds_metres_per_second.run,
            self.movement
                .motor
                .reference_quickstep_target_displacement_metres,
            self.movement.motor.reference_ground_drive_force_newtons,
            self.movement.motor.reference_leg_strength,
            self.movement.motor.gravity_metres_per_second_squared,
            self.movement.motor.traction_coefficient,
            self.autoresolve.minimum_attack_interval_seconds,
            self.autoresolve.melee_windup_seconds,
            self.autoresolve.melee_reaction_delay_min_seconds,
            self.autoresolve.melee_reaction_delay_max_seconds,
            self.autoresolve.melee_reflex_window_seconds,
            self.autoresolve.melee_initiative_delay_min_seconds,
            self.autoresolve.melee_initiative_delay_max_seconds,
            self.ai
                .ordinary
                .offense
                .long_weapon_measure_threshold_metres,
        ];
        assert!(
            positive
                .into_iter()
                .all(|value| value.is_finite() && value > 0.0),
            "{}: resolution and autoresolve physical values must be positive",
            path.display()
        );
        let fractions = [
            self.autoresolve.minimum_melee_input_reflex,
            self.ai.ordinary.defense.dodge_chance,
            self.autoresolve.minimum_hit_precision,
            self.autoresolve.maximum_hit_precision,
            self.autoresolve.outnumbered_flanking,
            self.autoresolve.ranged_defense_input_reflex,
            self.autoresolve.melee_cadence_jitter_seconds,
            self.ai.ordinary.offense.melee_measure_reach_fraction,
        ];
        assert!(
            fractions
                .into_iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(&value)),
            "{}: autoresolve fractions must be in [0, 1]",
            path.display()
        );
        assert!(
            self.autoresolve.minimum_hit_precision <= self.autoresolve.maximum_hit_precision,
            "{}: autoresolve hit precision bounds must be ordered",
            path.display()
        );
        assert!(
            self.autoresolve.melee_reaction_delay_min_seconds
                <= self.autoresolve.melee_reaction_delay_max_seconds,
            "{}: autoresolve melee reaction delay bounds must be ordered",
            path.display()
        );
        assert!(
            self.autoresolve.melee_initiative_delay_min_seconds
                <= self.autoresolve.melee_initiative_delay_max_seconds,
            "{}: autoresolve melee initiative delay bounds must be ordered",
            path.display()
        );
    }
}
