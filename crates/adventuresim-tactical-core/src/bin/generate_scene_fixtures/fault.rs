use super::*;

pub(super) fn fixture() -> Fixture {
    Fixture {
        name: "fault-scarp-cliff",
        scene_key: "fault-scarp",
        seed: 47_114,
        terrain: rolling,
        environment: rocky_open,
        weather: clear(),
        vista: VistaKind::Ordinary,
        fault_scarp: Some(FaultScarpRecipe {
            seed: 47_114,
            origin_cm: [0, 0],
            tangent_permyriad: [10_000, 0],
            throw_cm: 800,
            half_length_cm: 4_500,
            half_width_cm: 1_800,
            collar_cm: 400,
            lod: FaultScarpLod::Detail,
        }),
    }
}

pub(super) fn flat_fixture() -> Fixture {
    super::fixture(
        "flat-dry-grassland",
        "grassland",
        47_101,
        super::flat,
        super::dry_open,
        super::clear(),
    )
}
