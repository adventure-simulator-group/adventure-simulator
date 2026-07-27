fn main() {
    let target = std::env::args().nth(1).unwrap_or_else(|| "all".to_owned());
    if !matches!(target.as_str(), "all" | "items") {
        eprintln!("unsupported content target {target:?}; expected all or items");
        std::process::exit(2);
    }

    let catalog = adventuresim_core::item_catalog::catalog();
    let mut references = adventuresim_core::item_references::REQUIRED_GAMEPLAY_ITEM_IDS.to_vec();
    references.extend(adventuresim_core::strategic_currency::CURRENCY_IDS);
    references.extend(
        adventuresim_core::physiology::INTERVENTION_PROFILES
            .iter()
            .map(|profile| profile.preparation_id),
    );
    references.extend(
        adventuresim_core::bestiary::ALL_THREATS
            .iter()
            .filter_map(|id| {
                adventuresim_core::bestiary::profile(*id)
                    .combat
                    .loot_item_id
            }),
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
