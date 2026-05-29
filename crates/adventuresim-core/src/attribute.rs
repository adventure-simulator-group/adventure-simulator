use crate::body::{BodyPart, LimbWeights, PlayerBody};

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
    /// Deep thinking, mental skill bonus requiring focus.
    #[assoc(body_part = BodyPart::Head)]
    Intelligence,
    /// Quick decisions, tactical morale without focus.
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
