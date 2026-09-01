use std::{env, fs, path::Path};

use serde::Deserialize;

#[derive(Deserialize)]
struct CombatBuildConfig {
    resolution: ResolutionValues,
    autoresolve: AutoresolveValues,
}

#[derive(Deserialize)]
struct ResolutionValues {
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
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("combat_resolution_config.rs"),
        format!(
            "pub const EMBEDDED_COMBAT_RESOLUTION_PARAMETERS: CombatResolutionParameters = \
             CombatResolutionParameters {{ armed_attack_energy_transfer: {:?}_f32, \
             stagger_resistance_joules_per_kg: {:?}_f32 }};\n\
             pub const EMBEDDED_AUTORESOLVE_PARAMETERS: AutoresolveParameters = \
             AutoresolveParameters {{ combat_round_seconds: {:?}_f32, formation_spacing_metres: \
             {:?}_f32, reference_melee_attack_seconds: {:?}_f32, \
             minimum_movement_speed_metres_per_second: {:?}_f32, \
             minimum_attack_interval_seconds: {:?}_f32, minimum_melee_input_reflex: {:?}_f32, \
             minimum_hit_precision: {:?}_f32, maximum_hit_precision: {:?}_f32, \
             outnumbered_flanking: {:?}_f32, ranged_defense_input_reflex: {:?}_f32 }};\n",
            resolution.armed_attack_energy_transfer,
            resolution.stagger_resistance_joules_per_kg,
            autoresolve.combat_round_seconds,
            autoresolve.formation_spacing_metres,
            autoresolve.reference_melee_attack_seconds,
            autoresolve.minimum_movement_speed_metres_per_second,
            autoresolve.minimum_attack_interval_seconds,
            autoresolve.minimum_melee_input_reflex,
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
            self.autoresolve.minimum_attack_interval_seconds,
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
            self.autoresolve.minimum_hit_precision,
            self.autoresolve.maximum_hit_precision,
            self.autoresolve.outnumbered_flanking,
            self.autoresolve.ranged_defense_input_reflex,
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
    }
}
