use adventuresim_world_schema::{
    IgneousRock, MetamorphicRock, MixedLithology, SedimentaryRock, SurfaceLithology,
    UnconsolidatedDeposit,
};
use fabelgeist_determinism::{inclusive_unit_f32, splitmix64};
use serde::{Deserialize, Serialize};

/// Provenance for the compact surface truth carried into a tactical scene.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerrainSurfaceSource {
    /// Lithology comes from a containment-verified EGDI window.
    Mapped,
    /// A deterministic synthetic fixture explicitly authors its geology.
    AuthoredFixture,
}

/// Structural variation evaluated once from continuous scene position.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum TerrainGeologicStructure {
    Massive,
    Bedded {
        normal_permyriad: [i16; 3],
        bed_thickness_cm: u16,
        thickness_variation_bps: u16,
        warp_cm: u16,
        cross_bedding_bps: u16,
    },
    Foliated {
        normal_permyriad: [i16; 3],
        band_spacing_cm: u16,
        warp_cm: u16,
    },
}

/// Causal material families used by the renderer. The mapping from canonical
/// world lithology is exhaustive and intentionally has no fallback arm.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerrainSurfacePreset {
    Sandstone,
    FineClastic,
    Shale,
    Carbonate,
    Chalk,
    Evaporite,
    Organic,
    Siliceous,
    Granite,
    Plutonic,
    Basalt,
    Volcanic,
    MetamorphicFoliated,
    MetamorphicMassive,
    ConglomerateBreccia,
    MixedRock,
    CohesiveSediment,
    UnconsolidatedSediment,
}

/// Physical and palette inputs derived deterministically from a surface recipe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainSurfaceParameters {
    pub palette_srgb: [[u8; 3]; 2],
    pub roughness: [f32; 2],
    pub grain_tile_metres: f32,
    pub microrelief_metres: f32,
}

/// Required geological surface truth for every implicit terrain patch.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainSurfaceRecipe {
    pub lithology: SurfaceLithology,
    pub source: TerrainSurfaceSource,
    pub structure: TerrainGeologicStructure,
}

impl TerrainSurfaceRecipe {
    pub fn new(
        lithology: SurfaceLithology,
        source: TerrainSurfaceSource,
        seed: u64,
        tangent_permyriad: [i16; 2],
    ) -> Self {
        let preset = TerrainSurfacePreset::from_lithology(lithology);
        let structure = inferred_structure(preset, seed, tangent_permyriad);
        Self {
            lithology,
            source,
            structure,
        }
    }

    pub const fn preset(self) -> TerrainSurfacePreset {
        TerrainSurfacePreset::from_lithology(self.lithology)
    }

    pub const fn parameters(self) -> TerrainSurfaceParameters {
        self.preset().parameters()
    }

    pub fn validate(self) -> Result<(), &'static str> {
        let parameters = self.parameters();
        if !(0.18..=4.0).contains(&parameters.grain_tile_metres)
            || !(0.004..=0.055).contains(&parameters.microrelief_metres)
            || !(0.55..=1.0).contains(&parameters.roughness[0])
            || !(parameters.roughness[0]..=1.0).contains(&parameters.roughness[1])
        {
            return Err("terrain surface parameters are outside physical bounds");
        }
        match self.structure {
            TerrainGeologicStructure::Massive => {}
            TerrainGeologicStructure::Bedded {
                normal_permyriad,
                bed_thickness_cm,
                thickness_variation_bps,
                warp_cm,
                cross_bedding_bps,
            } => {
                validate_normal(normal_permyriad)?;
                if !(12..=600).contains(&bed_thickness_cm)
                    || !(500..=4_500).contains(&thickness_variation_bps)
                    || !(2..=120).contains(&warp_cm)
                    || cross_bedding_bps > 5_000
                {
                    return Err("bedded terrain structure is outside bounds");
                }
            }
            TerrainGeologicStructure::Foliated {
                normal_permyriad,
                band_spacing_cm,
                warp_cm,
            } => {
                validate_normal(normal_permyriad)?;
                if !(8..=350).contains(&band_spacing_cm) || !(2..=90).contains(&warp_cm) {
                    return Err("foliated terrain structure is outside bounds");
                }
            }
        }
        Ok(())
    }
}

fn validate_normal(normal: [i16; 3]) -> Result<(), &'static str> {
    let squared = normal
        .into_iter()
        .map(|component| i64::from(component).pow(2))
        .sum::<i64>();
    ((98_000_000..=102_000_000).contains(&squared))
        .then_some(())
        .ok_or("terrain geological structure normal is not normalized")
}

impl TerrainSurfacePreset {
    pub const fn from_lithology(lithology: SurfaceLithology) -> Self {
        match lithology {
            SurfaceLithology::Unconsolidated(deposit) => match deposit {
                UnconsolidatedDeposit::Clay | UnconsolidatedDeposit::Silt => Self::CohesiveSediment,
                UnconsolidatedDeposit::Sand
                | UnconsolidatedDeposit::Gravel
                | UnconsolidatedDeposit::Till
                | UnconsolidatedDeposit::Alluvium
                | UnconsolidatedDeposit::Loess
                | UnconsolidatedDeposit::VolcanicAsh
                | UnconsolidatedDeposit::MixedSediment => Self::UnconsolidatedSediment,
                UnconsolidatedDeposit::Peat => Self::Organic,
            },
            SurfaceLithology::Sedimentary(rock) => match rock {
                SedimentaryRock::Sandstone => Self::Sandstone,
                SedimentaryRock::Siltstone
                | SedimentaryRock::Mudstone
                | SedimentaryRock::Marl
                | SedimentaryRock::MixedSedimentary => Self::FineClastic,
                SedimentaryRock::Shale => Self::Shale,
                SedimentaryRock::Limestone | SedimentaryRock::Dolostone => Self::Carbonate,
                SedimentaryRock::Chalk => Self::Chalk,
                SedimentaryRock::Chert => Self::Siliceous,
                SedimentaryRock::Conglomerate => Self::ConglomerateBreccia,
                SedimentaryRock::Evaporite => Self::Evaporite,
                SedimentaryRock::Coal => Self::Organic,
            },
            SurfaceLithology::Igneous(rock) => match rock {
                IgneousRock::Granite | IgneousRock::Granitoid => Self::Granite,
                IgneousRock::Diorite
                | IgneousRock::Gabbro
                | IgneousRock::OtherPlutonic
                | IgneousRock::OtherIgneous => Self::Plutonic,
                IgneousRock::Basalt => Self::Basalt,
                IgneousRock::Andesite
                | IgneousRock::Rhyolite
                | IgneousRock::Tuff
                | IgneousRock::OtherVolcanic => Self::Volcanic,
            },
            SurfaceLithology::Metamorphic(rock) => match rock {
                MetamorphicRock::Slate
                | MetamorphicRock::Schist
                | MetamorphicRock::Gneiss
                | MetamorphicRock::Phyllite
                | MetamorphicRock::Amphibolite => Self::MetamorphicFoliated,
                MetamorphicRock::Quartzite
                | MetamorphicRock::Marble
                | MetamorphicRock::OtherMetamorphic => Self::MetamorphicMassive,
            },
            SurfaceLithology::Mixed(mixed) => match mixed {
                MixedLithology::Breccia => Self::ConglomerateBreccia,
                MixedLithology::Melange | MixedLithology::MixedRock => Self::MixedRock,
            },
        }
    }

    pub const fn parameters(self) -> TerrainSurfaceParameters {
        match self {
            Self::Sandstone => parameters([151, 104, 70], [194, 145, 98], 0.70, 0.90, 0.42, 0.032),
            Self::FineClastic => {
                parameters([112, 98, 82], [151, 136, 111], 0.76, 0.94, 0.28, 0.020)
            }
            Self::Shale => parameters([58, 61, 60], [91, 88, 79], 0.72, 0.91, 0.22, 0.014),
            Self::Carbonate => {
                parameters([145, 140, 118], [185, 180, 151], 0.68, 0.90, 0.36, 0.026)
            }
            Self::Chalk => parameters([184, 181, 157], [218, 215, 191], 0.78, 0.96, 0.24, 0.015),
            Self::Evaporite => parameters([71, 68, 61], [118, 109, 91], 0.72, 0.95, 0.30, 0.014),
            Self::Organic => parameters([35, 31, 27], [72, 62, 48], 0.78, 0.97, 0.24, 0.010),
            Self::Siliceous => parameters([101, 98, 91], [161, 154, 139], 0.57, 0.82, 0.31, 0.022),
            Self::Granite => parameters([105, 102, 99], [154, 148, 143], 0.61, 0.84, 0.55, 0.038),
            Self::Plutonic => parameters([79, 82, 82], [128, 129, 125], 0.60, 0.84, 0.48, 0.036),
            Self::Basalt => parameters([43, 47, 48], [75, 79, 77], 0.66, 0.88, 0.34, 0.030),
            Self::Volcanic => parameters([74, 68, 64], [119, 108, 99], 0.69, 0.91, 0.32, 0.029),
            Self::MetamorphicFoliated => {
                parameters([62, 65, 67], [130, 126, 118], 0.65, 0.88, 0.30, 0.024)
            }
            Self::MetamorphicMassive => {
                parameters([126, 124, 119], [174, 169, 158], 0.62, 0.86, 0.44, 0.030)
            }
            Self::ConglomerateBreccia => {
                parameters([102, 88, 75], [169, 148, 122], 0.67, 0.91, 0.72, 0.045)
            }
            Self::MixedRock => parameters([71, 68, 66], [145, 132, 116], 0.64, 0.90, 0.46, 0.032),
            Self::CohesiveSediment => {
                parameters([100, 79, 59], [126, 103, 78], 0.78, 0.96, 0.24, 0.010)
            }
            Self::UnconsolidatedSediment => {
                parameters([112, 100, 79], [158, 143, 112], 0.70, 0.95, 0.48, 0.018)
            }
        }
    }
}

const fn parameters(
    first: [u8; 3],
    second: [u8; 3],
    roughness_min: f32,
    roughness_max: f32,
    grain_tile_metres: f32,
    microrelief_metres: f32,
) -> TerrainSurfaceParameters {
    TerrainSurfaceParameters {
        palette_srgb: [first, second],
        roughness: [roughness_min, roughness_max],
        grain_tile_metres,
        microrelief_metres,
    }
}

fn inferred_structure(
    preset: TerrainSurfacePreset,
    seed: u64,
    tangent_permyriad: [i16; 2],
) -> TerrainGeologicStructure {
    // EGDI supplies unit lithology, not a local measured column, dip, or
    // strike. This stable seed/tangent construction is explicitly artistic
    // inference: it gives one scene-wide frame to bedding/foliation without
    // presenting it as observed field structure.
    let random = |salt| inclusive_unit_f32(splitmix64(seed ^ salt));
    let tangent = [
        f32::from(tangent_permyriad[0]) / 10_000.0,
        f32::from(tangent_permyriad[1]) / 10_000.0,
    ];
    let dip = match preset {
        TerrainSurfacePreset::MetamorphicFoliated | TerrainSurfacePreset::MixedRock => {
            0.28 + random(0x666f_6c69_6174_696f) * 0.58
        }
        _ => 0.06 + random(0x6265_6464_696e_6721) * 0.30,
    };
    let horizontal = (1.0 - dip * dip).sqrt();
    let normal = [
        (-tangent[1] * horizontal * 10_000.0).round() as i16,
        (dip * 10_000.0).round() as i16,
        (tangent[0] * horizontal * 10_000.0).round() as i16,
    ];
    match preset {
        TerrainSurfacePreset::Sandstone
        | TerrainSurfacePreset::FineClastic
        | TerrainSurfacePreset::Shale
        | TerrainSurfacePreset::Carbonate
        | TerrainSurfacePreset::Chalk
        | TerrainSurfacePreset::Evaporite
        | TerrainSurfacePreset::Organic
        | TerrainSurfacePreset::CohesiveSediment => TerrainGeologicStructure::Bedded {
            normal_permyriad: normal,
            bed_thickness_cm: (24.0 + random(0x7468_6963_6b6e_6573) * 210.0).round() as u16,
            thickness_variation_bps: (900.0 + random(0x7661_7269_6174_696f) * 2_800.0).round()
                as u16,
            warp_cm: (8.0 + random(0x7761_7270_5f63_6d21) * 54.0).round() as u16,
            cross_bedding_bps: if preset == TerrainSurfacePreset::Sandstone {
                (900.0 + random(0x6372_6f73_7362_6564) * 2_700.0).round() as u16
            } else {
                0
            },
        },
        TerrainSurfacePreset::MetamorphicFoliated | TerrainSurfacePreset::MixedRock => {
            TerrainGeologicStructure::Foliated {
                normal_permyriad: normal,
                band_spacing_cm: (16.0 + random(0x6261_6e64_5f63_6d21) * 150.0).round() as u16,
                warp_cm: (5.0 + random(0x666f_6c64_5f63_6d21) * 42.0).round() as u16,
            }
        }
        TerrainSurfacePreset::Granite
        | TerrainSurfacePreset::Plutonic
        | TerrainSurfacePreset::Basalt
        | TerrainSurfacePreset::Volcanic
        | TerrainSurfacePreset::Siliceous
        | TerrainSurfacePreset::MetamorphicMassive
        | TerrainSurfacePreset::ConglomerateBreccia
        | TerrainSurfacePreset::UnconsolidatedSediment => TerrainGeologicStructure::Massive,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_LITHOLOGIES: [SurfaceLithology; 45] = [
        SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Clay),
        SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Silt),
        SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Sand),
        SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Gravel),
        SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Till),
        SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Peat),
        SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Alluvium),
        SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::Loess),
        SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::VolcanicAsh),
        SurfaceLithology::Unconsolidated(UnconsolidatedDeposit::MixedSediment),
        SurfaceLithology::Sedimentary(SedimentaryRock::Limestone),
        SurfaceLithology::Sedimentary(SedimentaryRock::Dolostone),
        SurfaceLithology::Sedimentary(SedimentaryRock::Chalk),
        SurfaceLithology::Sedimentary(SedimentaryRock::Marl),
        SurfaceLithology::Sedimentary(SedimentaryRock::Sandstone),
        SurfaceLithology::Sedimentary(SedimentaryRock::Siltstone),
        SurfaceLithology::Sedimentary(SedimentaryRock::Mudstone),
        SurfaceLithology::Sedimentary(SedimentaryRock::Shale),
        SurfaceLithology::Sedimentary(SedimentaryRock::Conglomerate),
        SurfaceLithology::Sedimentary(SedimentaryRock::Evaporite),
        SurfaceLithology::Sedimentary(SedimentaryRock::Coal),
        SurfaceLithology::Sedimentary(SedimentaryRock::Chert),
        SurfaceLithology::Sedimentary(SedimentaryRock::MixedSedimentary),
        SurfaceLithology::Igneous(IgneousRock::Granite),
        SurfaceLithology::Igneous(IgneousRock::Granitoid),
        SurfaceLithology::Igneous(IgneousRock::Diorite),
        SurfaceLithology::Igneous(IgneousRock::Gabbro),
        SurfaceLithology::Igneous(IgneousRock::Basalt),
        SurfaceLithology::Igneous(IgneousRock::Andesite),
        SurfaceLithology::Igneous(IgneousRock::Rhyolite),
        SurfaceLithology::Igneous(IgneousRock::Tuff),
        SurfaceLithology::Igneous(IgneousRock::OtherPlutonic),
        SurfaceLithology::Igneous(IgneousRock::OtherVolcanic),
        SurfaceLithology::Igneous(IgneousRock::OtherIgneous),
        SurfaceLithology::Metamorphic(MetamorphicRock::Slate),
        SurfaceLithology::Metamorphic(MetamorphicRock::Schist),
        SurfaceLithology::Metamorphic(MetamorphicRock::Gneiss),
        SurfaceLithology::Metamorphic(MetamorphicRock::Quartzite),
        SurfaceLithology::Metamorphic(MetamorphicRock::Marble),
        SurfaceLithology::Metamorphic(MetamorphicRock::Phyllite),
        SurfaceLithology::Metamorphic(MetamorphicRock::Amphibolite),
        SurfaceLithology::Metamorphic(MetamorphicRock::OtherMetamorphic),
        SurfaceLithology::Mixed(MixedLithology::Breccia),
        SurfaceLithology::Mixed(MixedLithology::Melange),
        SurfaceLithology::Mixed(MixedLithology::MixedRock),
    ];

    #[test]
    fn every_canonical_lithology_has_deterministic_bounded_surface_parameters() {
        for lithology in ALL_LITHOLOGIES {
            let first = TerrainSurfaceRecipe::new(
                lithology,
                TerrainSurfaceSource::AuthoredFixture,
                42,
                [10_000, 0],
            );
            let second = TerrainSurfaceRecipe::new(
                lithology,
                TerrainSurfaceSource::AuthoredFixture,
                42,
                [10_000, 0],
            );
            assert_eq!(first, second, "{lithology:?}");
            first.validate().unwrap();
            let parameters = first.parameters();
            assert!(
                parameters
                    .palette_srgb
                    .into_iter()
                    .flatten()
                    .all(|channel| (24..=235).contains(&channel))
            );
        }
    }

    #[test]
    fn depositional_and_foliated_structure_never_leaks_to_massive_rock() {
        for lithology in [
            SurfaceLithology::Igneous(IgneousRock::Granite),
            SurfaceLithology::Igneous(IgneousRock::Basalt),
            SurfaceLithology::Mixed(MixedLithology::Breccia),
        ] {
            assert_eq!(
                TerrainSurfaceRecipe::new(
                    lithology,
                    TerrainSurfaceSource::AuthoredFixture,
                    7,
                    [10_000, 0]
                )
                .structure,
                TerrainGeologicStructure::Massive
            );
        }
        assert!(matches!(
            TerrainSurfaceRecipe::new(
                SurfaceLithology::Sedimentary(SedimentaryRock::Sandstone),
                TerrainSurfaceSource::AuthoredFixture,
                7,
                [10_000, 0]
            )
            .structure,
            TerrainGeologicStructure::Bedded {
                cross_bedding_bps: 1..,
                ..
            }
        ));
        assert!(matches!(
            TerrainSurfaceRecipe::new(
                SurfaceLithology::Metamorphic(MetamorphicRock::Schist),
                TerrainSurfaceSource::AuthoredFixture,
                7,
                [10_000, 0]
            )
            .structure,
            TerrainGeologicStructure::Foliated { .. }
        ));
    }
}
