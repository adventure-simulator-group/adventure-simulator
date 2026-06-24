use crate::{
    body::{BodyPart, BodyParts, BodySide, LimbWeights, PlayerBody},
    prelude::{LimbAttribute, PlayerAttributes},
};

const LOWER_MUSCLE_MASS_PER_LEG_STRENGTH: f32 = 5.0;
const WEIGHT_CAPACITY_PER_LOWER_MUSCLE_MASS: f32 = 30.0;
const ARMOR_PENALTY_EXPONENT: i32 = 3;

#[blanket::blanket(derive(Ref, Rc, Arc, Mut, Box, Cow))]
#[ambassador::delegatable_trait]
pub trait PlayerEquipment {
    fn weapon_accuracy(&self) -> f32;
    fn weapon_weight(&self) -> f32;
    fn weapon_penetration(&self) -> f32;
    fn weapon_reach(&self) -> f32;
    fn weapon_holding_side(&self) -> Option<BodySide>;
    fn shield_block_bonus(&self) -> f32;

    fn armor_resistance(&self, part: BodyPart) -> f32;
    fn armor_padding(&self, part: BodyPart) -> f32;
    fn armor_flexibility(&self, part: BodyPart) -> f32;
    fn armor_range_of_motion(&self, part: BodyPart) -> f32;

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
        let lower_muscle_mass = average_leg_strength * LOWER_MUSCLE_MASS_PER_LEG_STRENGTH;
        let weight_capacity = WEIGHT_CAPACITY_PER_LOWER_MUSCLE_MASS * lower_muscle_mass;

        let penalty = 1.0 - ((body.body_weight() + self.inventory_weight()) / weight_capacity);
        penalty.clamp(0.0, 1.0)
    }
}
