use super::*;

pub(super) fn sandstone() -> Fixture {
    Fixture {
        name: "sandstone-alcove",
        scene_key: "sandstone-alcove",
        seed: 47_115,
        terrain: |_, z| -z * 0.45,
        environment: rocky_open,
        weather: clear(),
        vista: VistaKind::Ordinary,
        buildings: BuildingFixture::Empty,
        landform: Some(TerrainLandformRecipe {
            kind: TerrainLandformKind::SandstoneAlcove,
            seed: 47_115,
            origin_cm: [0, 0],
            tangent_permyriad: [10_000, 0],
            relief_cm: 600,
            half_length_cm: 1200,
            half_width_cm: 1000,
            collar_cm: 250,
            lod: TerrainLandformLod::Detail,
        }),
    }
}
