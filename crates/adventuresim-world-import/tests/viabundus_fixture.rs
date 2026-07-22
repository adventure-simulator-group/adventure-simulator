use std::path::PathBuf;

use adventuresim_world_import::WorldBuilder;
use adventuresim_world_schema::SettlementDescriptionKind;

fn fixture_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/viabundus")
}

#[test]
fn parses_settlement_enrichment_into_domain_types() {
    let world = WorldBuilder::new(1544)
        .build_from_viabundus(&fixture_directory())
        .unwrap();

    assert_eq!(world.settlement_aliases.len(), 1);
    let alias = &world.settlement_aliases[0];
    assert_eq!(alias.name, "Lubeke");
    assert_eq!(alias.language.as_ref().unwrap().as_str(), "deu");

    assert_eq!(world.settlement_descriptions.len(), 2);
    assert_eq!(world.settlement_descriptions[0].body, "Eine Stadt & Burg.");
    assert_eq!(
        world.settlement_descriptions[0].kind,
        SettlementDescriptionKind::Settlement
    );
    assert_eq!(
        world.settlement_descriptions[1].kind,
        SettlementDescriptionKind::City
    );
    assert_eq!(world.report.settlement_aliases, 1);
    assert_eq!(world.report.settlement_descriptions, 2);
    assert_eq!(world.report.deferred_settlement_descriptions["bridge"], 1);
}

#[test]
fn playable_bounds_filter_topology_before_enrichment() {
    let world = WorldBuilder::new(1544)
        .with_bounds([10.68, 53.86, 10.69, 53.87])
        .build_from_viabundus(&fixture_directory())
        .unwrap();

    assert_eq!(world.report.settlements, 1);
    assert_eq!(world.report.nodes, 1);
    assert_eq!(world.report.edges, 0);
    assert_eq!(world.report.excluded_edges["outside-playable-bounds"], 1);
    assert_eq!(world.settlement_aliases.len(), 1);
}
