use crate::{
    attribute::{Attribute, PlayerAttributes},
    equipment::PlayerEquipment,
    essential::PlayerEssentials,
};

const MAX_CHECK: f32 = 5.0;

/// Player skills.
///
/// Skills are trained abilities, either mental (intuitive) or physical
/// (training-based). Each has a half-life in hours for the training curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, enum_assoc::Assoc)]
#[func(pub const fn max_hours(&self) -> f32)]
#[func(pub const fn kind(&self) -> SkillKind)]
#[func(pub const fn is_trained(&self) -> bool)]
pub enum Skill {
    /// Mental. Intuitive. Resist pain and morale penalties. (5000h)
    #[assoc(max_hours = 5000.0, kind = SkillKind::Mental, is_trained = false)]
    Will,
    /// Mental. Intuitive. Party morale buff, traveling focus. (20000h)
    #[assoc(max_hours = 20000.0, kind = SkillKind::Mental, is_trained = false)]
    Charisma,
    /// Mental. Trained. Party health recovery bonus. (10000h)
    #[assoc(max_hours = 10000.0, kind = SkillKind::Mental, is_trained = true)]
    Medicine,
    /// Mental. Trained. Faith multiplies same-religion morale. (5000h)
    #[assoc(max_hours = 5000.0, kind = SkillKind::Mental, is_trained = true)]
    Faith,
    /// Physical. Intuitive. Attack damage, agility for dodges, precision for hits. (8000h)
    #[assoc(max_hours = 8000.0, kind = SkillKind::Physical, is_trained = false)]
    Melee,
    /// Physical. Intuitive. Ranged accuracy, aim time. (15000h)
    #[assoc(max_hours = 15000.0, kind = SkillKind::Physical, is_trained = false)]
    Ranged,
    /// Physical. Intuitive. Shield defense, poise damage on block. (12000h)
    #[assoc(max_hours = 12000.0, kind = SkillKind::Physical, is_trained = false)]
    Block,
    /// Physical. Intuitive. Avoiding hits. (20000h)
    #[assoc(max_hours = 20000.0, kind = SkillKind::Physical, is_trained = false)]
    Dodge,
    /// Physical. Intuitive. Movement noise (agility), detection radius (precision). (8000h)
    #[assoc(max_hours = 8000.0, kind = SkillKind::Physical, is_trained = false)]
    Stealth,
    /// Physical. Intuitive. Poise in melee, terrain speed. (30000h)
    #[assoc(max_hours = 30000.0, kind = SkillKind::Physical, is_trained = false)]
    Balance,
    /// Physical. Trained. Bandage/splint speed, healing rate. (10000h)
    #[assoc(max_hours = 10000.0, kind = SkillKind::Physical, is_trained = true)]
    Surgeon,
}

impl Skill {
    pub const fn is_intuitive(&self) -> bool {
        !self.is_trained()
    }

    pub const fn is_mental(&self) -> bool {
        matches!(self.kind(), SkillKind::Mental)
    }

    pub const fn is_physical(&self) -> bool {
        matches!(self.kind(), SkillKind::Physical)
    }
}

/// Skill category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillKind {
    /// Physical: governed by agility, requires training.
    Physical,
    /// Mental: governed by instinct/intelligence, intuitive.
    Mental,
}

/// Trait for accessing player skill data.
#[blanket::blanket(derive(Ref, Rc, Arc, Mut, Box, Cow))]
#[ambassador::delegatable_trait]
pub trait PlayerSkills {
    fn skill_hours_trained(&self, skill: Skill) -> f32;

    fn skill_check_by_parts(
        &self,
        skill: Skill,
        attr: &impl PlayerAttributes,
        essentials: &impl PlayerEssentials,
        equipment: &impl PlayerEquipment,
    ) -> f32 {
        let hours_trained = self.skill_hours_trained(skill);
        let training = MAX_CHECK * (hours_trained / (hours_trained + skill.max_hours() * 0.5));
        let (reflex, focus) = match skill.kind() {
            SkillKind::Mental => (
                attr.attr(Attribute::Instinct),
                attr.attr(Attribute::Intelligence),
            ),
            SkillKind::Physical => (
                attr.attr(Attribute::Agility),
                attr.attr(Attribute::Precision),
            ),
        };
        let attribute = reflex + focus * essentials.focus_level();

        let mut check = if skill.is_intuitive() {
            (training + attribute) * 0.5
        } else {
            training.min(attribute)
        };
        if skill.is_physical() {
            //         # armor penalty ranges from 0-0.4, with full-plate being 0.4
            //         if skill.is_upper_body():
            //             check *= 1 - player.upper_body_armor_penalty()
            //         else:
            //             check *= 1 - player.lower_body_armor_penalty()
            //         check *= player.encumbrance_penalty()
            check *= equipment.encumbrance_penalty();
        }
        // TODO: modify by fatigue penalty here ?

        check
    }
}
