use std::path::PathBuf;

use adventuresim_world_import::{WorldBuilder, renderer_artifacts};
use adventuresim_world_schema::{
    CURRENT_INFERENCE_RULES_VERSION, CompiledWorld, SettlementDescriptionKind, SpatialGridSpec,
    WORLD_SCHEMA_VERSION, WorldBuildReport, WorldMetadata,
};

fn fixture_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/viabundus")
}

#[test]
fn renderer_artifacts_are_deterministic_and_bound_to_world_identity() {
    let world = CompiledWorld {
        metadata: WorldMetadata {
            schema_version: WORLD_SCHEMA_VERSION,
            inference_rules_version: CURRENT_INFERENCE_RULES_VERSION,
            spatial_grid: SpatialGridSpec::default(),
            world_year: 1544,
            manifest_digest: "1".repeat(64),
            sources: Vec::new(),
            road_types: Vec::new(),
        },
        nodes: Vec::new(),
        edges: Vec::new(),
        settlements: Vec::new(),
        settlement_aliases: Vec::new(),
        settlement_descriptions: Vec::new(),
        report: WorldBuildReport::default(),
    };
    let world_bytes = serde_json::to_vec(&world).unwrap();
    let artifact_id = blake3::hash(&world_bytes).to_hex().to_string();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("renderer-fixture-{}", std::process::id()));
    let first = renderer_artifacts::build(&world, &artifact_id, &root.join("first")).unwrap();
    let second = renderer_artifacts::build(&world, &artifact_id, &root.join("second")).unwrap();

    assert_eq!(
        std::fs::read(first.package).unwrap(),
        std::fs::read(second.package).unwrap()
    );
    assert_eq!(
        std::fs::read(first.paper_map).unwrap(),
        std::fs::read(second.paper_map).unwrap()
    );
    let manifest: adventuresim_render_contracts::MapManifest =
        serde_json::from_slice(&std::fs::read(first.manifest).unwrap()).unwrap();
    assert_eq!(manifest.artifact_id, artifact_id);
    assert_eq!(manifest.manifest_digest, world.metadata.manifest_digest);
    assert_eq!(manifest.world_schema, world.metadata.schema_version);
    assert!(manifest.bounds.min.x <= manifest.bounds.max.x);
    assert!(manifest.package_url.contains(&manifest.package_hash));
    manifest.validate().unwrap();
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
