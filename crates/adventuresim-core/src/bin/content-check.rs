fn main() {
    let target = std::env::args().nth(1).unwrap_or_else(|| "all".to_owned());
    if !matches!(target.as_str(), "all" | "items") {
        eprintln!("unsupported content target {target:?}; expected all or items");
        std::process::exit(2);
    }

    let catalog = adventuresim_core::item_catalog::catalog();
    let mut references = vec![
        "arrow",
        "bandage",
        "cooked_meal",
        "cooking_pan",
        "cooking_pot",
        "portable_oven",
        "soft_soap",
        "splint",
        "surgery_kit",
        "torch",
        "travel_ration",
        "waterskin",
    ];
    references.extend(adventuresim_core::strategic_currency::CURRENCY_IDS);
    references.extend(
        adventuresim_core::physiology::INTERVENTION_PROFILES
            .iter()
            .map(|profile| profile.preparation_id),
    );
    if let Err(missing) = adventuresim_core::item_catalog::validate_references(references) {
        panic!("missing required gameplay item references: {missing:?}");
    }
    println!(
        "items: {} definitions, revision {}",
        catalog.len(),
        adventuresim_core::item_catalog::revision()
    );
}
