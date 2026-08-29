use super::*;

pub(super) fn build(
    package: &Package,
    wetlands: Vec<Vec<Vec<[f64; 2]>>>,
    wetland_source_sha256: String,
) -> adventuresim_terrain::builder::Features {
    adventuresim_terrain::builder::Features {
        roads: package
            .routing_roads
            .iter()
            .map(|line| line.iter().map(|point| point.0).collect())
            .collect(),
        water: package
            .water
            .iter()
            .map(|polygon| {
                polygon
                    .rings
                    .iter()
                    .map(|ring| ring.iter().map(|point| point.0).collect())
                    .collect()
            })
            .collect(),
        wetlands,
        wetland_source_sha256,
        cultivated: Vec::new(),
        cultivation_source_sha256: format!("{:x}", Sha256::digest(b"no-cultivation")),
        cultivation_rules_version:
            adventuresim_world_import::cultivation::CULTIVATION_RULES_VERSION,
        terrain_features: Vec::new(),
    }
}

pub(super) fn finalize(
    package: &mut Package,
    wetlands: Vec<Vec<Vec<[f64; 2]>>>,
    wetland_source_sha256: String,
    cultivated: CultivatedLand,
    world: &CompiledWorld,
) -> adventuresim_terrain::builder::Features {
    package.cultivated = cultivated
        .polygons
        .iter()
        .map(|rings| WaterPolygon {
            rings: rings
                .iter()
                .map(|ring| ring.iter().copied().map(Point).collect())
                .collect(),
        })
        .collect();
    let mut features = build(package, wetlands, wetland_source_sha256);
    features.terrain_features = world.terrain_features.clone();
    features.cultivated = cultivated.polygons;
    features.cultivation_source_sha256 = cultivated.source_sha256;
    features
}
