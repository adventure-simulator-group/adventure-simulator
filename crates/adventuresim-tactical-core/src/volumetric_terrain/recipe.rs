use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerrainLandformKind {
    FaultScarp,
    SandstoneAlcove,
    CarbonateDissolution,
    GraniteJointRockfall,
}

#[derive(Component, Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[component(immutable)]
#[serde(deny_unknown_fields)]
pub struct TerrainLandformRecipe {
    pub kind: TerrainLandformKind,
    pub seed: u64,
    pub origin_cm: [i32; 2],
    /// Unit tangent encoded in ten-thousandths.
    pub tangent_permyriad: [i16; 2],
    pub relief_cm: u16,
    pub half_length_cm: u16,
    pub half_width_cm: u16,
    pub collar_cm: u16,
    pub lod: TerrainLandformLod,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerrainLandformLod {
    Detail,
    Fringe,
}

impl TerrainLandformLod {
    pub(crate) const fn voxel_cm(self) -> u16 {
        match self {
            Self::Detail => 50,
            Self::Fringe => 100,
        }
    }
}

impl TerrainLandformRecipe {
    pub fn validate(self, terrain: &SceneTerrain) -> Result<(), &'static str> {
        let tangent = Vec2::new(
            f32::from(self.tangent_permyriad[0]),
            f32::from(self.tangent_permyriad[1]),
        ) / 10_000.0;
        if !(0.98..=1.02).contains(&tangent.length()) {
            return Err("landform tangent is not normalized");
        }
        if !(100..=2_000).contains(&self.relief_cm)
            || !(400..=5_000).contains(&self.half_length_cm)
            || !(300..=2_000).contains(&self.half_width_cm)
            || self.collar_cm < 100
            || self.collar_cm >= self.half_length_cm
            || u32::from(self.collar_cm) * 2 >= u32::from(self.half_width_cm)
        {
            return Err("landform dimensions are outside their bounds");
        }
        let origin = Vec2::new(self.origin_cm[0] as f32, self.origin_cm[1] as f32) / 100.0;
        let half = Vec2::new(terrain.width(), terrain.depth()) * 0.5;
        let half_length = f32::from(self.half_length_cm) / 100.0;
        let half_width = f32::from(self.half_width_cm) / 100.0 + SCARP_RUPTURE_WANDER_METRES;
        let normal = Vec2::new(-tangent.y, tangent.x);
        let extent = tangent.abs() * half_length + normal.abs() * half_width;
        if origin.x.abs() > half.x + extent.x || origin.y.abs() > half.y + extent.y {
            return Err("landform does not overlap the playable terrain");
        }
        Ok(())
    }

    pub fn transition_collar(self) -> TerrainTransitionCollar {
        let origin = Vec2::new(self.origin_cm[0] as f32, self.origin_cm[1] as f32) / 100.0;
        let tangent = Vec2::new(
            f32::from(self.tangent_permyriad[0]),
            f32::from(self.tangent_permyriad[1]),
        ) / 10_000.0;
        let half_length = f32::from(self.half_length_cm) / 100.0;
        let half_width = f32::from(self.half_width_cm) / 100.0;
        TerrainTransitionCollar::irregular_ellipse(
            origin,
            tangent,
            half_length,
            half_width,
            f32::from(self.collar_cm) / 100.0,
            self.seed,
            SCARP_RUPTURE_WANDER_METRES,
            SCARP_WIDTH_VARIATION_BPS,
        )
        .expect("validated landform dimensions produce a transition collar")
    }
}
