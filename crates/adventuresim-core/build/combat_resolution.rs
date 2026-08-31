use std::{env, fs, path::Path};

pub fn compile(root: &Path) {
    let path = root.join("content/tactical/combat.yaml");
    println!("cargo:rerun-if-changed={}", path.display());
    let text = fs::read_to_string(&path).expect("content/tactical/combat.yaml must exist");
    let document: serde_json::Value =
        serde_saphyr::from_str(&text).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let resolution = document
        .get("resolution")
        .unwrap_or_else(|| panic!("{}: missing resolution", path.display()));
    let armed_attack_energy_transfer = resolution
        .get("armed_attack_energy_transfer")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or_else(|| {
            panic!(
                "{}: resolution.armed_attack_energy_transfer must be a number",
                path.display()
            )
        });
    let stagger_resistance_joules_per_kg = resolution
        .get("stagger_resistance_joules_per_kg")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or_else(|| {
            panic!(
                "{}: resolution.stagger_resistance_joules_per_kg must be a number",
                path.display()
            )
        });
    assert!(
        armed_attack_energy_transfer.is_finite()
            && armed_attack_energy_transfer > 0.0
            && armed_attack_energy_transfer <= 1.0,
        "{}: resolution.armed_attack_energy_transfer must be in (0, 1]",
        path.display()
    );
    assert!(
        stagger_resistance_joules_per_kg.is_finite() && stagger_resistance_joules_per_kg > 0.0,
        "{}: resolution.stagger_resistance_joules_per_kg must be positive",
        path.display()
    );
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("combat_resolution_config.rs"),
        format!(
            "pub const EMBEDDED_COMBAT_RESOLUTION_PARAMETERS: CombatResolutionParameters = \
             CombatResolutionParameters {{ armed_attack_energy_transfer: \
             {armed_attack_energy_transfer:?}_f32, stagger_resistance_joules_per_kg: \
             {stagger_resistance_joules_per_kg:?}_f32 }};\n"
        ),
    )
    .unwrap();
}
