use adventuresim_world_schema::TerrainFeature;

#[derive(Default)]
pub struct Features {
    pub roads: Vec<Vec<[f64; 2]>>,
    pub water: Vec<Vec<Vec<[f64; 2]>>>,
    pub wetlands: Vec<Vec<Vec<[f64; 2]>>>,
    pub wetland_source_sha256: String,
    pub cultivated: Vec<Vec<Vec<[f64; 2]>>>,
    pub cultivation_source_sha256: String,
    pub cultivation_rules_version: u16,
    pub terrain_features: Vec<TerrainFeature>,
}
