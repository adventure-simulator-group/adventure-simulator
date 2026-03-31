use crate::attribute::{Attribute, PlayerAttributes};

const CALORIES_PER_ENDURANCE: f32 = 1000.0;

pub trait PlayerEssentials {
    fn calories_used_today(&self) -> f32;
    fn focus_level(&self) -> f32;

    fn fatigue_by_parts(&self, attr: &impl PlayerAttributes) -> f32 {
        self.calories_used_today() / (attr.attr(Attribute::Endurance) * CALORIES_PER_ENDURANCE)
    }
}
