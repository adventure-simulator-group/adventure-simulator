use crate::body::{BodyPart, LimbWeights, PlayerBody};
use serde::{Deserialize, Serialize};

/// Complete, framework-neutral base attribute values for one character.
///
/// Persistence rows and ECS components may carry their own identity or
/// framework metadata, but calculations share this value rather than
/// redefining its fields.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
#[serde(deny_unknown_fields)]
pub struct PlayerAttributeValues {
    pub endurance: f32,
    pub immunity: f32,
    pub gut: f32,
    pub intelligence: f32,
    pub instinct: f32,
    pub eyesight: f32,
    pub hearing: f32,
    pub left_arm_strength: f32,
    pub right_arm_strength: f32,
    pub left_leg_strength: f32,
    pub right_leg_strength: f32,
    pub left_arm_agility: f32,
    pub right_arm_agility: f32,
    pub left_leg_agility: f32,
    pub right_leg_agility: f32,
}

impl PlayerAttributes for PlayerAttributeValues {
    fn raw_limb_attr(&self, attr: LimbAttribute, limb: BodyPart) -> f32 {
        match (attr, limb) {
            (LimbAttribute::Strength, BodyPart::LeftArm) => self.left_arm_strength,
            (LimbAttribute::Strength, BodyPart::RightArm) => self.right_arm_strength,
            (LimbAttribute::Strength, BodyPart::LeftLeg) => self.left_leg_strength,
            (LimbAttribute::Strength, BodyPart::RightLeg) => self.right_leg_strength,
            (LimbAttribute::Agility, BodyPart::LeftArm) => self.left_arm_agility,
            (LimbAttribute::Agility, BodyPart::RightArm) => self.right_arm_agility,
            (LimbAttribute::Agility, BodyPart::LeftLeg) => self.left_leg_agility,
            (LimbAttribute::Agility, BodyPart::RightLeg) => self.right_leg_agility,
            _ => 0.0,
        }
    }

    fn raw_single_body_part_attr(&self, attr: SimpleAttribute) -> f32 {
        match attr {
            SimpleAttribute::Endurance => self.endurance,
            SimpleAttribute::Immunity => self.immunity,
            SimpleAttribute::Gut => self.gut,
            SimpleAttribute::Intelligence => self.intelligence,
            SimpleAttribute::Instinct => self.instinct,
            SimpleAttribute::Eyesight => self.eyesight,
            SimpleAttribute::Hearing => self.hearing,
        }
    }
}

/// Player attributes.
///
/// Attributes represent a character's physical and mental capabilities.
/// They are grouped by body region: chest, stomach, head, and limbs.
/// Each attribute is a value from 0-5 representing conditioning level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Attribute {
    Limb(LimbAttribute),
    Simple(SimpleAttribute),
}

impl From<SimpleAttribute> for Attribute {
    fn from(value: SimpleAttribute) -> Self {
        Self::Simple(value)
    }
}

impl From<LimbAttribute> for Attribute {
    fn from(value: LimbAttribute) -> Self {
        Self::Limb(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LimbAttribute {
    /// Muscle mass, damage and climbing ability.
    Strength,
    /// Reflex speed, dodging and stealth.
    Agility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, enum_assoc::Assoc)]
#[func(pub fn body_part(&self) -> BodyPart)]
pub enum SimpleAttribute {
    /// Heart strength, lung capacity, endurance for traveling.
    #[assoc(body_part = BodyPart::Chest)]
    Endurance,
    /// Liver, spleen, immune system, toxin filtering.
    #[assoc(body_part = BodyPart::Stomach)]
    Immunity,
    /// Digestive system, food tolerance.
    #[assoc(body_part = BodyPart::Stomach)]
    Gut,
    /// Deep thinking; learning speed and mastery cap for intellectual skills.
    #[assoc(body_part = BodyPart::Head)]
    Intelligence,
    /// Quick decisions; learning speed and mastery cap for instinctive skills.
    #[assoc(body_part = BodyPart::Head)]
    Instinct,
    /// Visual acuity.
    #[assoc(body_part = BodyPart::Head)]
    Eyesight,
    /// Auditory perception.
    #[assoc(body_part = BodyPart::Head)]
    Hearing,
}

/// Trait for accessing player attribute values.
#[blanket::blanket(derive(Ref, Rc, Arc, Mut, Box, Cow))]
#[ambassador::delegatable_trait]
pub trait PlayerAttributes {
    // this can panic if attribute and limb isn't a valid limb
    fn raw_limb_attr(&self, attr: LimbAttribute, limb: BodyPart) -> f32;
    fn raw_single_body_part_attr(&self, attr: SimpleAttribute) -> f32;

    fn attr_by_parts(&self, attr: impl Into<Attribute>, body: &impl PlayerBody) -> f32 {
        let attr = attr.into();
        match attr {
            Attribute::Limb(attr) => {
                self.limb_attr_by_weight_by_parts(attr, body, LimbWeights::all_equal())
            }
            Attribute::Simple(attr) => {
                let health = body.body_part_health(attr.body_part());
                let raw = self.raw_single_body_part_attr(attr);
                raw * health
            }
        }
    }

    fn limb_attr_by_weight_by_parts(
        &self,
        attr: LimbAttribute,
        body: &impl PlayerBody,
        weights: LimbWeights,
    ) -> f32 {
        BodyPart::LIMBS.iter().fold(0.0, |sum, part| {
            let raw = self.raw_limb_attr(attr, part);
            let weight = weights.by_part(part).clamp(0.0, 1.0);
            let health = body.body_part_health(part).clamp(0.0, 1.0);

            sum + raw * weight * health
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_attribute_values_round_trip_without_accepting_schema_drift() {
        let values = PlayerAttributeValues {
            endurance: 3.0,
            instinct: 4.0,
            left_arm_strength: 2.5,
            ..Default::default()
        };
        let encoded = serde_json::to_value(&values).unwrap();
        assert_eq!(
            serde_json::from_value::<PlayerAttributeValues>(encoded.clone()).unwrap(),
            values
        );
        let mut drifted = encoded.as_object().unwrap().clone();
        drifted.insert("legacy_endurance".into(), serde_json::json!(3.0));
        assert!(
            serde_json::from_value::<PlayerAttributeValues>(serde_json::Value::Object(drifted))
                .is_err()
        );
    }
}
