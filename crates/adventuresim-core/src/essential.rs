use crate::{attribute::PlayerAttributes, body::PlayerBody, prelude::SimpleAttribute};

const CALORIES_PER_ENDURANCE: f32 = 1000.0;
const FATIGUE_EXPONENT: i32 = 5;

#[blanket::blanket(derive(Ref, Rc, Arc, Mut, Box, Cow))]
#[ambassador::delegatable_trait]
pub trait PlayerEssentials {
    fn calories_used_today(&self) -> f32;
    fn focus_level(&self) -> f32;

    fn fatigue_by_parts(&self, attr: &impl PlayerAttributes, body: &impl PlayerBody) -> f32 {
        self.calories_used_today()
            / (attr.attr_by_parts(SimpleAttribute::Endurance, body) * CALORIES_PER_ENDURANCE)
    }

    fn fatigue_penalty_by_parts(
        &self,
        attr: &impl PlayerAttributes,
        body: &impl PlayerBody,
    ) -> f32 {
        let fatigue = self.fatigue_by_parts(attr, body);
        (1.0 - fatigue.powi(FATIGUE_EXPONENT)).max(0.0)
    }
}
