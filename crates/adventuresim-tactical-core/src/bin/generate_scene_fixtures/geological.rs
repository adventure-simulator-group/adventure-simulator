use super::*;

pub(super) fn sandstone() -> Fixture {
    fixture(
        "sandstone-alcove",
        TerrainLandformKind::SandstoneAlcove,
        47_115,
    )
}

pub(super) fn carbonate() -> Fixture {
    fixture(
        "carbonate-dissolution",
        TerrainLandformKind::CarbonateDissolution,
        47_116,
    )
}

pub(super) fn granite() -> Fixture {
    fixture(
        "granite-joint-rockfall",
        TerrainLandformKind::GraniteJointRockfall,
        47_117,
    )
}

fn fixture(name: &'static str, kind: TerrainLandformKind, seed: u64) -> Fixture {
    Fixture {
        name,
        scene_key: name,
        seed,
        terrain: |_, z| -z * 0.45,
        environment: rocky_open,
        weather: clear(),
        vista: VistaKind::Ordinary,
        buildings: BuildingFixture::Empty,
        landform: Some(TerrainLandformRecipe {
            kind,
            seed,
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
