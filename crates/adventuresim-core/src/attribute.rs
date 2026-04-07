/// Player attributes.
///
/// Attributes represent a character's physical and mental capabilities.
/// They are grouped by body region: chest, stomach, head, and limbs.
/// Each attribute is a value from 0-5 representing conditioning level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Attribute {
    /// Chest. Heart strength, lung capacity, endurance for traveling.
    Endurance,
    /// Stomach. Liver, spleen, immune system, toxin filtering.
    Immunity,
    /// Stomach. Digestive system, food tolerance.
    Gut,
    /// Limbs. Muscle mass, damage and climbing ability.
    Strength,
    /// Limbs. Fine motor control, accuracy.
    Precision,
    /// Limbs. Reflex speed, dodging and stealth.
    Agility,
    /// Head. Deep thinking, mental skill bonus requiring focus.
    Intelligence,
    /// Head. Quick decisions, tactical morale without focus.
    Instinct,
    /// Head. Visual acuity.
    Eyesight,
    /// Head. Auditory perception.
    Hearing,
}

/// Trait for accessing player attribute values.
#[blanket::blanket(derive(Ref, Rc, Arc, Mut, Box, Cow))]
#[ambassador::delegatable_trait]
pub trait PlayerAttributes {
    fn attr(&self, attr: Attribute) -> f32;
}
