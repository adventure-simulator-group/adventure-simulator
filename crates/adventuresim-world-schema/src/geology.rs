use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct GeologicUnitId {
    value: String,
}

impl GeologicUnitId {
    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        (!value.is_empty()
            && value.len() <= 255
            && value == value.trim()
            && !value.chars().any(char::is_control))
        .then_some(Self { value })
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl<'de> Deserialize<'de> for GeologicUnitId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            value: String,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.value)
            .ok_or_else(|| serde::de::Error::custom("invalid EGDI geologic unit identifier"))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum UnconsolidatedDeposit {
    Clay,
    Silt,
    Sand,
    Gravel,
    Till,
    Peat,
    Alluvium,
    Loess,
    VolcanicAsh,
    MixedSediment,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum SedimentaryRock {
    Limestone,
    Dolostone,
    Chalk,
    Marl,
    Sandstone,
    Siltstone,
    Mudstone,
    Shale,
    Conglomerate,
    Evaporite,
    Coal,
    Chert,
    MixedSedimentary,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum IgneousRock {
    Granite,
    Granitoid,
    Diorite,
    Gabbro,
    Basalt,
    Andesite,
    Rhyolite,
    Tuff,
    OtherPlutonic,
    OtherVolcanic,
    OtherIgneous,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum MetamorphicRock {
    Slate,
    Schist,
    Gneiss,
    Quartzite,
    Marble,
    Phyllite,
    Amphibolite,
    OtherMetamorphic,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum MixedLithology {
    Breccia,
    Melange,
    MixedRock,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum SurfaceLithology {
    Unconsolidated(UnconsolidatedDeposit),
    Sedimentary(SedimentaryRock),
    Igneous(IgneousRock),
    Metamorphic(MetamorphicRock),
    Mixed(MixedLithology),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum GeologicEra {
    Quaternary,
    Neogene,
    Paleogene,
    Cenozoic,
    Cretaceous,
    Jurassic,
    Triassic,
    Mesozoic,
    Permian,
    Carboniferous,
    Devonian,
    Silurian,
    Ordovician,
    Cambrian,
    Paleozoic,
    Precambrian,
    Phanerozoic,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum GeologicAgeEvidence {
    Mapped(GeologicEra),
    Inferred(GeologicEra),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum GeologicLithologyEvidence {
    Mapped(SurfaceLithology),
    Inferred(SurfaceLithology),
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct GeologicSetting {
    pub lithology: GeologicLithologyEvidence,
    pub age: GeologicAgeEvidence,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct MappedSurfaceGeology {
    pub unit: GeologicUnitId,
    pub setting: GeologicSetting,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct InferredGeologicSetting {
    pub lithology: SurfaceLithology,
    pub age: GeologicEra,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub enum SurfaceGeology {
    Mapped(MappedSurfaceGeology),
    Inferred(InferredGeologicSetting),
}
