use super::*;
use adventuresim_world_schema::{
    IgneousRock, SedimentaryRock, SurfaceLithology, UnconsolidatedDeposit,
};

pub(super) fn sandstone() -> Fixture {
    fixture(
        "sandstone-alcove",
        TerrainLandformKind::SandstoneAlcove,
        SurfaceLithology::Sedimentary(SedimentaryRock::Sandstone),
        47_115,
    )
}

pub(super) fn carbonate() -> Fixture {
    fixture(
        "carbonate-dissolution",
        TerrainLandformKind::CarbonateDissolution,
        SurfaceLithology::Sedimentary(SedimentaryRock::Limestone),
        47_116,
    )
}

pub(super) fn granite() -> Fixture {
    fixture(
        "granite-joint-rockfall",
        TerrainLandformKind::GraniteJointRockfall,
        SurfaceLithology::Igneous(IgneousRock::Granite),
        47_117,
    )
}

pub(super) fn basalt() -> Fixture {
    fixture(
        "basalt-cooling-columns",
        TerrainLandformKind::BasaltCoolingColumns,
        SurfaceLithology::Igneous(IgneousRock::Basalt),
        47_118,
    )
}

pub(super) fn slump() -> Fixture {
    fixture(
        "cohesive-slump-headscarp",
        TerrainLandformKind::CohesiveSlumpHeadscarp,
        SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Clay),
        47_119,
    )
}

fn fixture(
    name: &'static str,
    kind: TerrainLandformKind,
    lithology: SurfaceLithology,
    seed: u64,
) -> Fixture {
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
            surface: TerrainSurfaceRecipe::new(
                lithology,
                TerrainSurfaceSource::AuthoredFixture,
                seed,
                [10_000, 0],
            ),
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
