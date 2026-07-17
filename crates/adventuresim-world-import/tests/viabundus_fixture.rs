use std::path::PathBuf;

use adventuresim_world_import::WorldBuilder;
use adventuresim_world_schema::{GeographicCoordinate, SettlementDescriptionKind, WorldBounds};

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
fn world_bounds_limit_the_viabundus_seed_graph() {
    let outside_lubeck = WorldBounds::new(
        GeographicCoordinate::new(50.0, 8.0).unwrap(),
        GeographicCoordinate::new(51.0, 9.0).unwrap(),
    )
    .unwrap();
    let world = WorldBuilder::new(1544)
        .with_world_bounds(outside_lubeck)
        .build_from_viabundus(&fixture_directory())
        .unwrap();

    assert_eq!(world.report.nodes, 0);
    assert_eq!(world.report.edges, 0);
    assert_eq!(world.report.settlement_aliases, 0);
}
