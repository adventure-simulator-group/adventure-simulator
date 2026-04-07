use crate::body::BodyPart;

#[blanket::blanket(derive(Ref, Rc, Arc, Mut, Box, Cow))]
#[ambassador::delegatable_trait]
pub trait PlayerEquipment {
    fn weapon_accuracy(&self) -> f32;
    fn armor_dodge(&self, part: BodyPart) -> f32;
    fn inventory_weight(&self) -> f32;
    fn shield_block_bonus(&self) -> f32;

    fn encumbrance_penalty(&self) -> f32 {
        // TODO: implement
        1.0
    }
}
