use adventuresim_world_schema::BASIS_POINTS_PER_WHOLE;
use bevy::prelude::*;
use fabelgeist_determinism::{inclusive_unit_f32, splitmix64};
use serde::{Deserialize, Serialize};

/// Bounded implicit footprint where a volumetric terrain patch takes ownership
/// from the heightfield and blends back through its irregular outer collar.
#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect, Serialize, Deserialize)]
#[reflect(Component)]
#[component(immutable)]
pub struct TerrainTransitionCollar {
    origin: Vec2,
    tangent: Vec2,
    half_length_metres: f32,
    half_width_metres: f32,
    width_metres: f32,
    seed: u64,
    wander_metres: f32,
    width_variation_bps: u16,
}

impl TerrainTransitionCollar {
    #[expect(
        clippy::too_many_arguments,
        reason = "all footprint parameters are validated"
    )]
    pub fn irregular_ellipse(
        origin: Vec2,
        tangent: Vec2,
        half_length_metres: f32,
        half_width_metres: f32,
        width_metres: f32,
        seed: u64,
        wander_metres: f32,
        width_variation_bps: u16,
    ) -> Option<Self> {
        if !origin.is_finite()
            || !tangent.is_finite()
            || !(0.98..=1.02).contains(&tangent.length())
            || !half_length_metres.is_finite()
            || !half_width_metres.is_finite()
            || !width_metres.is_finite()
            || !wander_metres.is_finite()
            || half_length_metres <= width_metres
            || half_width_metres <= width_metres
            || width_metres <= 0.0
            || wander_metres < 0.0
            || width_variation_bps > 5_000
        {
            return None;
        }
        Some(Self {
            origin,
            tangent: tangent.normalize(),
            half_length_metres,
            half_width_metres,
            width_metres,
            seed,
            wander_metres,
            width_variation_bps,
        })
    }

    pub fn cuts_out(self, point: Vec2) -> bool {
        self.radial_coordinate(point).0 < 1.0
    }

    pub fn contains(self, point: Vec2) -> bool {
        self.radial_coordinate(point).0 <= 1.0
    }

    pub(crate) fn blend_weight(self, point: Vec2) -> f32 {
        let (radial, minimum_extent) = self.radial_coordinate(point);
        let inner = 1.0 - self.width_metres / minimum_extent;
        smoothstep01((1.0 - radial) / (1.0 - inner))
    }

    pub(crate) fn local_coordinates(self, point: Vec2) -> Vec2 {
        let normal = Vec2::new(-self.tangent.y, self.tangent.x);
        let relative = point - self.origin;
        let along = relative.dot(self.tangent);
        let clamped = along.clamp(-self.half_length_metres, self.half_length_metres);
        let wander = smooth_value_noise(self.seed ^ 0x7275_7074_7572_6501, clamped / 5.5) * 0.72
            + smooth_value_noise(self.seed ^ 0x7275_7074_7572_6502, clamped / 1.8) * 0.28;
        Vec2::new(along, relative.dot(normal) - wander * self.wander_metres)
    }

    fn radial_coordinate(self, point: Vec2) -> (f32, f32) {
        let local = self.local_coordinates(point);
        let variation = f32::from(self.width_variation_bps) / BASIS_POINTS_PER_WHOLE as f32;
        let width_noise =
            smooth_value_noise(self.seed ^ 0x7769_6474_6800_0001, local.x / 4.2) * 0.5 + 0.5;
        let local_half_width = self.half_width_metres * (1.0 - variation + width_noise * variation);
        let radial = Vec2::new(
            local.x / self.half_length_metres,
            local.y / local_half_width,
        )
        .length();
        (radial, self.half_length_metres.min(local_half_width))
    }
}

fn smooth_value_noise(seed: u64, coordinate: f32) -> f32 {
    let cell = coordinate.floor() as i64;
    let fraction = smoothstep01(coordinate - coordinate.floor());
    let sample = |offset: i64| {
        inclusive_unit_f32(splitmix64(seed ^ cell.wrapping_add(offset) as u64)) * 2.0 - 1.0
    };
    sample(0).lerp(sample(1), fraction)
}

fn smoothstep01(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}
