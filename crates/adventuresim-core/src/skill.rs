use crate::{
    attribute::PlayerAttributes,
    body::{BodyPart, LimbWeights, PlayerBody},
    equipment::PlayerEquipment,
    essential::PlayerEssentials,
    prelude::{LimbAttribute, SimpleAttribute},
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
    /// Mental. Intuitive. Reading another character's motives and temperament.
    #[assoc(max_hours = 10000.0, kind = SkillKind::Mental, is_trained = false)]
    Insight,
    /// Mental. Intuitive. Recognizing one's own motives and temperament.
    #[assoc(max_hours = 10000.0, kind = SkillKind::Mental, is_trained = false)]
    SelfAwareness,
    /// Mental. Intuitive. Relieving tension through levity.
    #[assoc(max_hours = 12000.0, kind = SkillKind::Mental, is_trained = false)]
    Humor,
    /// Mental. Intuitive. Rallying and coordinating others.
    #[assoc(max_hours = 20000.0, kind = SkillKind::Mental, is_trained = false)]
    Command,
    /// Mental. Intuitive. Sustaining a plausible false impression.
    #[assoc(max_hours = 15000.0, kind = SkillKind::Mental, is_trained = false)]
    Deception,
    /// Mental. Intuitive. Reading and expressing romantic interest.
    #[assoc(max_hours = 15000.0, kind = SkillKind::Mental, is_trained = false)]
    Seduction,
    /// Mental. Trained. Party health recovery bonus. (10000h)
    #[assoc(max_hours = 10000.0, kind = SkillKind::Mental, is_trained = true)]
    Physiology,
    /// Mental. Trained. Food preparation, safety, and kitchen technique. (10000h)
    #[assoc(max_hours = 10000.0, kind = SkillKind::Mental, is_trained = true)]
    Cooking,
    /// Mental. Trained. Meta-skill for knowledge of religious traditions. (5000h each)
    #[assoc(max_hours = 5000.0, kind = SkillKind::Mental, is_trained = true)]
    Religion,
    /// Mental. Trained. Meta-skill for knowledge of creature categories. (5000h each)
    #[assoc(max_hours = 5000.0, kind = SkillKind::Mental, is_trained = true)]
    Bestiary,
    /// Physical. Intuitive. Long hafted weapons. (8000h)
    #[assoc(max_hours = 8000.0, kind = SkillKind::Physical, is_trained = false)]
    Polearm,
    /// Physical. Intuitive. Axes and cleaving weapons. (8000h)
    #[assoc(max_hours = 8000.0, kind = SkillKind::Physical, is_trained = false)]
    Axe,
    /// Physical. Intuitive. Hammers, maces, and other impact weapons. (8000h)
    #[assoc(max_hours = 8000.0, kind = SkillKind::Physical, is_trained = false)]
    Bludgeon,
    /// Physical. Intuitive. Long blades. (8000h)
    #[assoc(max_hours = 8000.0, kind = SkillKind::Physical, is_trained = false)]
    Sword,
    /// Physical. Intuitive. Short weapons, including knives and short blades. (8000h)
    #[assoc(max_hours = 8000.0, kind = SkillKind::Physical, is_trained = false)]
    Knife,
    /// Physical. Intuitive. Bows. (15000h)
    #[assoc(max_hours = 15000.0, kind = SkillKind::Physical, is_trained = false)]
    Bow,
    /// Physical. Intuitive. Crossbows. (15000h)
    #[assoc(max_hours = 15000.0, kind = SkillKind::Physical, is_trained = false)]
    Crossbow,
    /// Physical. Intuitive. Firearms. (15000h)
    #[assoc(max_hours = 15000.0, kind = SkillKind::Physical, is_trained = false)]
    Firearm,
    /// Physical. Intuitive. Thrown weapons. (15000h)
    #[assoc(max_hours = 15000.0, kind = SkillKind::Physical, is_trained = false)]
    Throw,
    /// Physical. Intuitive. Shield defense, poise damage on block. (12000h)
    #[assoc(max_hours = 12000.0, kind = SkillKind::Physical, is_trained = false)]
    Block,
    /// Physical. Intuitive. Avoiding hits. (20000h)
    #[assoc(max_hours = 20000.0, kind = SkillKind::Physical, is_trained = false)]
    Dodge,
    /// Physical. Intuitive. Movement noise (agility), detection radius (precision). (8000h)
    #[assoc(max_hours = 8000.0, kind = SkillKind::Physical, is_trained = false)]
    Stealth,
    /// Physical. Intuitive. Poise in melee. (30000h)
    #[assoc(max_hours = 30000.0, kind = SkillKind::Physical, is_trained = false)]
    Balance,
    /// Mental. Intuitive. Movement through open country. (30000h)
    #[assoc(max_hours = 30000.0, kind = SkillKind::Mental, is_trained = false)]
    TerrainPlains,
    /// Mental. Intuitive. Movement through woodland. (30000h)
    #[assoc(max_hours = 30000.0, kind = SkillKind::Mental, is_trained = false)]
    TerrainForest,
    /// Mental. Intuitive. Movement through hilly ground. (30000h)
    #[assoc(max_hours = 30000.0, kind = SkillKind::Mental, is_trained = false)]
    TerrainHills,
    /// Mental. Intuitive. Movement through built-up ground. (30000h)
    #[assoc(max_hours = 30000.0, kind = SkillKind::Mental, is_trained = false)]
    TerrainUrban,
    /// Mental. Trained. Knowledge of bodies and wounds. (10000h)
    #[assoc(max_hours = 10000.0, kind = SkillKind::Mental, is_trained = true)]
    Anatomy,
    /// Physical. Trained. Sewing, clothing repair, and wound stitching. (10000h)
    #[assoc(max_hours = 10000.0, kind = SkillKind::Physical, is_trained = true)]
    Tailoring,
    /// Physical. Trained. Field maintenance and equipment repair. (10000h)
    #[assoc(max_hours = 10000.0, kind = SkillKind::Physical, is_trained = true)]
    Smithing,
}

#[cfg(test)]
mod tests {
    use super::Skill;

    #[test]
    fn only_family_skills_are_meta_skills() {
        assert!(Skill::Religion.is_meta_skill());
        assert!(Skill::Bestiary.is_meta_skill());
        assert!(!Skill::Anatomy.is_meta_skill());
        assert!(Skill::Bestiary.is_mental());
        assert!(!Skill::Bestiary.is_upper_body());
    }

    #[test]
    fn smithing_uses_its_documented_shared_training_curve() {
        assert!((Skill::Smithing.training_rank(5_000.0) - 2.5).abs() < 0.001);
        assert_eq!(Skill::Smithing.training_rank(f32::NAN), 0.0);
    }
}

impl Skill {
    /// Training-only contribution on the shared five-point curve. `max_hours`
    /// is the documented asymptotic calibration; half of it is the half-rank
    /// point used consistently by simulation, persistence, and UI.
    pub fn training_rank(self, hours: f32) -> f32 {
        let hours = if hours.is_finite() {
            hours.max(0.0)
        } else {
            0.0
        };
        MAX_CHECK * (hours / (hours + self.max_hours() * 0.5))
    }

    pub const fn is_intuitive(&self) -> bool {
        !self.is_trained()
    }

    pub const fn is_mental(&self) -> bool {
        matches!(self.kind(), SkillKind::Mental)
    }

    pub const fn is_physical(&self) -> bool {
        matches!(self.kind(), SkillKind::Physical)
    }

    /// Whether this value names a family whose trained hours live on separate,
    /// correlated subskills rather than on the parent itself.
    pub const fn is_meta_skill(&self) -> bool {
        matches!(self, Skill::Religion | Skill::Bestiary)
    }

    pub const fn is_upper_body(&self) -> bool {
        matches!(
            self,
            Skill::Polearm
                | Skill::Axe
                | Skill::Bludgeon
                | Skill::Sword
                | Skill::Knife
                | Skill::Bow
                | Skill::Crossbow
                | Skill::Firearm
                | Skill::Throw
                | Skill::Block
                | Skill::Stealth
                | Skill::Tailoring
                | Skill::Smithing
        )
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

    fn bestiary_hours_for(&self, _category: adventuresim_world_schema::BestiaryCategory) -> f32 {
        self.skill_hours_trained(Skill::Bestiary)
    }

    fn skill_check_by_parts(
        &self,
        skill: Skill,
        attr: &impl PlayerAttributes,
        body: &impl PlayerBody,
        essentials: &impl PlayerEssentials,
        equipment: &impl PlayerEquipment,
        weights: LimbWeights,
    ) -> f32 {
        let hours_trained = self.skill_hours_trained(skill);
        let training = skill.training_rank(hours_trained);

        let (reflex, focus) = match skill.kind() {
            SkillKind::Mental => (
                attr.attr_by_parts(SimpleAttribute::Instinct, body),
                attr.attr_by_parts(SimpleAttribute::Intelligence, body),
            ),
            SkillKind::Physical => (
                attr.limb_attr_by_weight_by_parts(LimbAttribute::Agility, body, weights),
                if skill == Skill::Block {
                    // Precision improves the poise damage dealt by a parry,
                    // not the chance of successfully blocking an attack.
                    0.0
                } else {
                    attr.precision_by_parts(body, weights)
                },
            ),
        };
        let attribute_check = reflex + focus * essentials.focus_level();

        let mut check = if skill.is_intuitive() {
            (training + attribute_check) * 0.5
        } else {
            training.min(attribute_check)
        };
        if skill.is_physical() {
            let armor_penalty = if skill.is_upper_body() {
                equipment.armor_penalty(BodyPart::UPPER_BODY)
            } else {
                equipment.armor_penalty(BodyPart::LOWER_BODY)
            };
            check *= armor_penalty;
            check *= equipment.encumbrance_penalty_by_parts(attr, body);
            check *= essentials.fatigue_penalty_by_parts(attr, body);
        }

        check
    }
}
