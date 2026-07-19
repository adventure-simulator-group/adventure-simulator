use crate::{
    body::{BodyPart, BodyParts, BodySide, LimbWeights, PlayerBody},
    prelude::{LimbAttribute, PlayerAttributes},
};

pub const LOWER_MUSCLE_MASS_PER_LEG_STRENGTH: f32 = 5.0;
pub const WEIGHT_CAPACITY_PER_LOWER_MUSCLE_MASS: f32 = 30.0;
const ARMOR_PENALTY_EXPONENT: i32 = 3;

/// The carried burden and injury-adjusted carrying capacity used by the
/// shared linear encumbrance rule.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EncumbranceSummary {
    pub burden_kg: f32,
    pub capacity_kg: f32,
}

impl EncumbranceSummary {
    pub fn new(burden_kg: f32, capacity_kg: f32) -> Self {
        Self {
            burden_kg: finite_nonnegative(burden_kg),
            capacity_kg: finite_nonnegative(capacity_kg),
        }
    }

    pub fn remaining_multiplier(self) -> f32 {
        encumbrance_remaining_multiplier(self.burden_kg, self.capacity_kg)
    }

    pub fn penalty_fraction(self) -> f32 {
        1.0 - self.remaining_multiplier()
    }

    pub fn combined(self, other: Self) -> Self {
        Self::new(
            self.burden_kg + other.burden_kg,
            self.capacity_kg + other.capacity_kg,
        )
    }
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

pub fn encumbrance_capacity_kg(average_injury_adjusted_leg_strength: f32) -> f32 {
    finite_nonnegative(average_injury_adjusted_leg_strength)
        * LOWER_MUSCLE_MASS_PER_LEG_STRENGTH
        * WEIGHT_CAPACITY_PER_LOWER_MUSCLE_MASS
}

/// Returns the multiplier left after encumbrance. A character with no usable
/// capacity is fully penalized, including when the reported burden is zero.
pub fn encumbrance_remaining_multiplier(burden_kg: f32, capacity_kg: f32) -> f32 {
    let burden_kg = finite_nonnegative(burden_kg);
    let capacity_kg = finite_nonnegative(capacity_kg);
    if capacity_kg <= f32::EPSILON {
        return 0.0;
    }
    (1.0 - burden_kg / capacity_kg).clamp(0.0, 1.0)
}

#[blanket::blanket(derive(Ref, Rc, Arc, Mut, Box, Cow))]
#[ambassador::delegatable_trait]
pub trait PlayerEquipment {
    fn weapon_is_melee(&self) -> bool {
        false
    }
    fn weapon_is_ranged(&self) -> bool {
        false
    }
    fn weapon_does_blunt(&self) -> bool {
        false
    }
    fn weapon_does_slash(&self) -> bool {
        false
    }
    fn weapon_does_pierce(&self) -> bool {
        false
    }
    fn weapon_accuracy(&self) -> f32;
    fn weapon_weight(&self) -> f32;
    fn weapon_penetration(&self) -> f32;
    fn weapon_reach(&self) -> f32;
    fn weapon_holding_side(&self) -> Option<BodySide>;
    fn weapon_is_precise(&self) -> bool;
    fn weapon_balance(&self) -> f32;
    /// Kinetic energy delivered by a projectile. Forty joules is a useful
    /// short-bow baseline; implementations with richer item data can override
    /// it per weapon.
    fn weapon_ranged_force_joules(&self) -> f32 {
        40.0 * self.weapon_weight().max(0.5)
    }
    fn shield_block_bonus(&self) -> f32;

    fn armor_resistance(&self, part: BodyPart) -> f32;
    fn armor_padding(&self, part: BodyPart) -> f32;
    fn armor_flexibility(&self, part: BodyPart) -> f32;
    fn armor_range_of_motion(&self, part: BodyPart) -> f32;
    fn armor_coverage(&self, part: BodyPart) -> f32;

    fn inventory_weight(&self) -> f32;

    // TODO: probably should count in limbs agility/strength for this ?
    fn armor_penalty(&self, parts: BodyParts) -> f32 {
        if parts.is_empty() {
            return 1.0;
        }

        let average_range_of_motion = parts
            .iter()
            .fold(0.0, |acc, part| acc + self.armor_range_of_motion(part))
            / parts.len() as f32;

        let penalty = 1.0 - (1.0 - average_range_of_motion).powi(ARMOR_PENALTY_EXPONENT);
        penalty.clamp(0.0, 1.0)
    }
    fn encumbrance_penalty_by_parts(
        &self,
        attrs: &impl PlayerAttributes,
        body: &impl PlayerBody,
    ) -> f32 {
        let average_leg_strength = attrs.limb_attr_by_weight_by_parts(
            LimbAttribute::Strength,
            body,
            LimbWeights::both_legs(),
        );
        let weight_capacity = encumbrance_capacity_kg(average_leg_strength);
        encumbrance_remaining_multiplier(
            body.body_weight() + self.inventory_weight(),
            weight_capacity,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_encumbrance_covers_key_points_and_overload() {
        assert_eq!(encumbrance_remaining_multiplier(0.0, 100.0), 1.0);
        assert_eq!(encumbrance_remaining_multiplier(50.0, 100.0), 0.5);
        assert_eq!(encumbrance_remaining_multiplier(100.0, 100.0), 0.0);
        assert_eq!(encumbrance_remaining_multiplier(125.0, 100.0), 0.0);
        assert_eq!(encumbrance_remaining_multiplier(0.0, 0.0), 0.0);
    }

    #[test]
    fn injury_adjusted_strength_maps_to_capacity() {
        assert_eq!(encumbrance_capacity_kg(0.0), 0.0);
        assert_eq!(encumbrance_capacity_kg(0.75), 112.5);
        assert_eq!(encumbrance_capacity_kg(3.0), 450.0);
    }

    #[test]
    fn summaries_combine_burdens_and_capacities_before_penalty() {
        let party =
            EncumbranceSummary::new(60.0, 100.0).combined(EncumbranceSummary::new(30.0, 200.0));
        assert_eq!(party, EncumbranceSummary::new(90.0, 300.0));
        assert!((party.penalty_fraction() - 0.3).abs() < f32::EPSILON);
    }
}
