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
/// Skills are trained abilities. Each has a governing aptitude which controls
/// learning speed and the highest currently effective rank, but never adds to
/// the final check value.
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
    /// Physical. Intuitive. Quiet, unobtrusive movement. (8000h)
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
    use super::{
        PlayerSkills, Skill, apply_direct_training, apply_language_training,
        aptitude_training_multiplier,
    };
    use crate::{
        body::{BodyPart, BodySide, LimbWeights, PlayerBody},
        prelude::{LimbAttribute, PlayerAttributes, SimpleAttribute},
        stub::{StubEquipment, StubEssentials},
    };

    #[derive(Clone, Copy)]
    struct Aptitudes {
        intelligence: f32,
        instinct: f32,
        limbs: [f32; 4],
    }

    impl PlayerAttributes for Aptitudes {
        fn raw_limb_attr(&self, attr: LimbAttribute, limb: BodyPart) -> f32 {
            if attr != LimbAttribute::Agility {
                return 3.0;
            }
            self.limbs[match limb {
                BodyPart::LeftArm => 0,
                BodyPart::RightArm => 1,
                BodyPart::LeftLeg => 2,
                BodyPart::RightLeg => 3,
                _ => return 0.0,
            }]
        }

        fn raw_single_body_part_attr(&self, attr: SimpleAttribute) -> f32 {
            match attr {
                SimpleAttribute::Intelligence => self.intelligence,
                SimpleAttribute::Instinct => self.instinct,
                _ => 3.0,
            }
        }
    }

    struct ArmHealthBody(f32);

    impl PlayerBody for ArmHealthBody {
        fn body_part_health(&self, part: BodyPart) -> f32 {
            if matches!(part, BodyPart::LeftArm | BodyPart::RightArm) {
                self.0
            } else {
                1.0
            }
        }

        fn body_weight(&self) -> f32 {
            70.0
        }

        fn primary_side(&self) -> BodySide {
            BodySide::Right
        }
    }

    #[test]
    fn only_family_skills_are_meta_skills() {
        assert!(Skill::Religion.is_meta_skill());
        assert!(Skill::Bestiary.is_meta_skill());
        assert!(!Skill::Anatomy.is_meta_skill());
        assert!(Skill::Bestiary.is_mental());
        assert!(!Skill::Bestiary.is_upper_body());
    }

    #[test]
    fn skill_and_aptitude_labels_are_canonical() {
        assert_eq!(Skill::SelfAwareness.label(), "Self-awareness");
        assert_eq!(
            Skill::Anatomy.governing_aptitude_kind().label(),
            "Intelligence"
        );
        assert_eq!(
            Skill::Physiology.governing_aptitude_kind().label(),
            "Intelligence"
        );
        assert_eq!(Skill::Knife.governing_aptitude_kind().label(), "Agility");
    }

    #[test]
    fn smithing_uses_its_documented_shared_training_curve() {
        assert!((Skill::Smithing.training_rank(5_000.0) - 2.5).abs() < 0.001);
        assert_eq!(Skill::Smithing.training_rank(f32::NAN), 0.0);
    }

    #[test]
    fn aptitude_multiplier_is_linear_at_every_authored_point() {
        for (attribute, expected) in [
            (0.0, 0.0),
            (1.0, 0.25),
            (2.5, 1.0),
            (4.0, 1.75),
            (5.0, 2.25),
        ] {
            assert!((aptitude_training_multiplier(attribute) - expected).abs() < f32::EPSILON);
        }
        let values: Vec<_> = (0..=50)
            .map(|step| aptitude_training_multiplier(step as f32 / 10.0))
            .collect();
        assert!(values.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn inverse_cap_and_partial_crossing_preserve_latent_hours() {
        let attributes = Aptitudes {
            intelligence: 3.0,
            instinct: 3.0,
            limbs: [3.0; 4],
        };
        for rank in 0..=5 {
            let hours = Skill::Physiology.hours_for_rank(rank as f32);
            if rank == 5 {
                assert!(hours.is_infinite());
            } else {
                assert!((Skill::Physiology.training_rank(hours) - rank as f32).abs() < 0.001);
            }
        }
        let cap = Skill::Physiology.hours_for_rank(3.0);
        let mut stored = cap - 0.5;
        let result = apply_direct_training(Skill::Physiology, &mut stored, 1.0, &attributes);
        assert!((stored - cap).abs() < 0.001);
        assert!((result.accepted_effective_hours - 0.5).abs() < 0.001);
        assert!((result.excess_effective_hours - 0.75).abs() < 0.001);

        let latent = Skill::Physiology.hours_for_rank(4.0);
        stored = latent;
        let low = Skill::Physiology.capped_training_rank(stored, &attributes);
        let restored = Skill::Physiology.capped_training_rank(
            stored,
            &Aptitudes {
                intelligence: 4.0,
                ..attributes
            },
        );
        assert_eq!(stored, latent);
        assert!((low - 3.0).abs() < 0.001);
        assert!((restored - 4.0).abs() < 0.001);
    }

    #[test]
    fn fixed_physical_aptitudes_use_healthy_limb_weights() {
        let attributes = Aptitudes {
            intelligence: 2.0,
            instinct: 4.0,
            limbs: [1.0, 3.0, 4.0, 2.0],
        };
        assert_eq!(Skill::Sword.governing_aptitude(&attributes), 2.0);
        assert_eq!(Skill::Dodge.governing_aptitude(&attributes), 3.0);
        assert_eq!(Skill::Stealth.governing_aptitude(&attributes), 2.5);
        assert_eq!(Skill::Deception.governing_aptitude(&attributes), 4.0);
        assert_eq!(Skill::SelfAwareness.governing_aptitude(&attributes), 4.0);
        assert_eq!(Skill::Physiology.governing_aptitude(&attributes), 2.0);
    }

    #[test]
    fn aptitude_caps_but_does_not_add_to_a_skill_check() {
        let hours = Skill::Physiology.hours_for_rank(2.0);
        struct PhysiologySkills(f32);
        impl PlayerSkills for PhysiologySkills {
            fn skill_hours_trained(&self, skill: Skill) -> f32 {
                (skill == Skill::Physiology)
                    .then_some(self.0)
                    .unwrap_or(0.0)
            }
        }
        let low = Aptitudes {
            intelligence: 3.0,
            instinct: 1.0,
            limbs: [1.0; 4],
        };
        let high = Aptitudes {
            intelligence: 5.0,
            ..low
        };
        let check = |attributes: &Aptitudes| {
            PhysiologySkills(hours).skill_check_by_parts(
                Skill::Physiology,
                attributes,
                &ArmHealthBody(1.0),
                &StubEssentials,
                &StubEquipment,
                LimbWeights::all_equal(),
            )
        };
        assert!((check(&low) - 2.0).abs() < 0.001);
        assert_eq!(check(&low), check(&high));
    }

    #[test]
    fn zero_aptitude_produces_neither_training_nor_mastery_excess() {
        let attributes = Aptitudes {
            intelligence: 0.0,
            instinct: 0.0,
            limbs: [0.0; 4],
        };
        let mut stored = 0.0;
        assert_eq!(
            apply_direct_training(Skill::Physiology, &mut stored, 100.0, &attributes),
            super::TrainingGain::default()
        );
        assert_eq!(stored, 0.0);
    }

    #[test]
    fn language_training_uses_aptitude_speed_and_partial_cap_crossing() {
        let mut hours = 2_999.5;
        let gain = apply_language_training(&mut hours, 1.0, 3.0);
        assert_eq!(hours, 3_000.0);
        assert_eq!(gain.accepted_effective_hours, 0.5);
        assert_eq!(gain.excess_effective_hours, 0.75);
    }

    #[test]
    fn language_training_zero_neutral_and_multi_leaf_excess_are_conserved() {
        let mut zero = 0.0;
        assert_eq!(
            apply_language_training(&mut zero, 10.0, 0.0),
            super::TrainingGain::default()
        );
        let mut neutral = 0.0;
        assert_eq!(
            apply_language_training(&mut neutral, 10.0, 2.5).accepted_effective_hours,
            10.0
        );
        let mut first = 2_999.5;
        let mut second = 3_000.0;
        let excess = apply_language_training(&mut first, 1.0, 3.0).excess_effective_hours
            + apply_language_training(&mut second, 1.0, 3.0).excess_effective_hours;
        assert_eq!(excess, 2.0);
    }

    #[test]
    fn partial_cap_crossing_mastery_is_bulk_chunk_invariant() {
        let duration = 7 * 24 * 60;
        let mut bulk_hours = 2_999.0;
        let bulk = apply_language_training(&mut bulk_hours, 2.0, 3.0);
        let bulk_morale = crate::morale::mastery_enjoyment_after_interval(
            0.0,
            bulk.excess_effective_hours,
            120,
            duration,
        );
        let mut chunked_hours = 2_999.0;
        // Split exactly at the cap crossing: the accepted prefix produces no
        // enjoyment, while the same rejected suffix reaches the endpoint.
        let first = apply_language_training(&mut chunked_hours, 0.8, 3.0);
        let second = apply_language_training(&mut chunked_hours, 1.2, 3.0);
        let chunked_morale = crate::morale::mastery_enjoyment_after_interval(
            crate::morale::mastery_enjoyment_after_interval(
                0.0,
                first.excess_effective_hours,
                48,
                duration,
            ),
            second.excess_effective_hours,
            72,
            duration,
        );
        assert_eq!(bulk_hours, chunked_hours);
        assert!((bulk_morale - chunked_morale).abs() < 0.0001);
    }
}

impl Skill {
    /// Canonical player-facing name used anywhere a skill is described.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Will => "Will",
            Self::Insight => "Insight",
            Self::SelfAwareness => "Self-awareness",
            Self::Humor => "Humor",
            Self::Command => "Command",
            Self::Deception => "Deception",
            Self::Seduction => "Seduction",
            Self::Physiology => "Physiology",
            Self::Cooking => "Cooking",
            Self::Religion => "Religion",
            Self::Bestiary => "Bestiary",
            Self::Polearm => "Polearm",
            Self::Axe => "Axe",
            Self::Bludgeon => "Bludgeon",
            Self::Sword => "Sword",
            Self::Knife => "Knife",
            Self::Bow => "Bow",
            Self::Crossbow => "Crossbow",
            Self::Firearm => "Firearm",
            Self::Throw => "Throw",
            Self::Block => "Block",
            Self::Dodge => "Dodge",
            Self::Stealth => "Stealth",
            Self::Balance => "Balance",
            Self::TerrainPlains => "Plains",
            Self::TerrainForest => "Forest",
            Self::TerrainHills => "Hills",
            Self::TerrainUrban => "Urban",
            Self::Anatomy => "Anatomy",
            Self::Tailoring => "Tailoring",
            Self::Smithing => "Smithing",
        }
    }

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

    /// Effective hours at which the shared training curve reaches `rank`.
    ///
    /// Rank five is the curve's asymptote and therefore has no finite hours
    /// boundary.
    pub fn hours_for_rank(self, rank: f32) -> f32 {
        let rank = if rank.is_finite() {
            rank.clamp(0.0, MAX_CHECK)
        } else {
            0.0
        };
        if rank >= MAX_CHECK {
            f32::INFINITY
        } else {
            rank * self.max_hours() * 0.5 / (MAX_CHECK - rank)
        }
    }

    /// Governing healthy conditioned aptitude. This intentionally reads raw
    /// attributes so injury changes performance, not learning speed or mastery.
    pub fn governing_aptitude(self, attr: &impl PlayerAttributes) -> f32 {
        let value = match self.governing_aptitude_kind() {
            GoverningAptitude::Intelligence => {
                attr.raw_single_body_part_attr(SimpleAttribute::Intelligence)
            }
            GoverningAptitude::Instinct => {
                attr.raw_single_body_part_attr(SimpleAttribute::Instinct)
            }
            GoverningAptitude::Agility(weights) => BodyPart::LIMBS.iter().fold(0.0, |sum, part| {
                sum + attr.raw_limb_attr(LimbAttribute::Agility, part) * weights.by_part(part)
            }),
        };
        if value.is_finite() {
            value.clamp(0.0, MAX_CHECK)
        } else {
            0.0
        }
    }

    pub const fn governing_aptitude_kind(self) -> GoverningAptitude {
        match self {
            Self::Will
            | Self::Insight
            | Self::SelfAwareness
            | Self::Humor
            | Self::Command
            | Self::Deception
            | Self::Seduction => GoverningAptitude::Instinct,
            Self::Physiology
            | Self::Anatomy
            | Self::Cooking
            | Self::Religion
            | Self::Bestiary
            | Self::TerrainPlains
            | Self::TerrainForest
            | Self::TerrainHills
            | Self::TerrainUrban => GoverningAptitude::Intelligence,
            Self::Dodge | Self::Balance => GoverningAptitude::Agility(LimbWeights::both_legs()),
            Self::Stealth => GoverningAptitude::Agility(LimbWeights::all_equal()),
            Self::Polearm
            | Self::Axe
            | Self::Bludgeon
            | Self::Sword
            | Self::Knife
            | Self::Bow
            | Self::Crossbow
            | Self::Firearm
            | Self::Throw
            | Self::Block
            | Self::Tailoring
            | Self::Smithing => GoverningAptitude::Agility(LimbWeights::both_arms()),
        }
    }

    pub fn capped_training_rank(self, hours: f32, attr: &impl PlayerAttributes) -> f32 {
        self.capped_rank_for_aptitude(hours, self.governing_aptitude(attr))
    }

    pub fn capped_rank_for_aptitude(self, hours: f32, aptitude: f32) -> f32 {
        let aptitude = if aptitude.is_finite() {
            aptitude.clamp(0.0, MAX_CHECK)
        } else {
            0.0
        };
        self.training_rank(hours).min(aptitude)
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GoverningAptitude {
    Intelligence,
    Instinct,
    Agility(LimbWeights),
}

impl GoverningAptitude {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Intelligence => "Intelligence",
            Self::Instinct => "Instinct",
            Self::Agility(_) => "Agility",
        }
    }
}

/// Positive effective learning produced by one real training-hour budget.
pub fn aptitude_training_multiplier(attribute: f32) -> f32 {
    if !attribute.is_finite() {
        return 0.0;
    }
    (1.0 + 0.5 * (attribute.clamp(0.0, MAX_CHECK) - 2.5)).max(0.0)
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct TrainingGain {
    pub accepted_effective_hours: f32,
    pub excess_effective_hours: f32,
}

/// Apply direct language study on the shared 1,000-effective-hours-per-rank
/// scale. Callers supply Instinct for oral languages and Intelligence for
/// written languages.
pub fn apply_language_training(
    stored_effective_hours: &mut f32,
    real_hours: f32,
    aptitude: f32,
) -> TrainingGain {
    if !real_hours.is_finite() || real_hours <= 0.0 {
        return TrainingGain::default();
    }
    let aptitude = if aptitude.is_finite() {
        aptitude.clamp(0.0, MAX_CHECK)
    } else {
        0.0
    };
    let gain = real_hours * aptitude_training_multiplier(aptitude);
    let current = if stored_effective_hours.is_finite() {
        (*stored_effective_hours).max(0.0)
    } else {
        0.0
    };
    let accepted = gain.min((aptitude * 1_000.0 - current).max(0.0));
    *stored_effective_hours = current + accepted;
    TrainingGain {
        accepted_effective_hours: accepted,
        excess_effective_hours: gain - accepted,
    }
}

/// Apply positive direct training without destroying hours which are latent
/// above a currently lowered aptitude cap.
pub fn apply_direct_training(
    skill: Skill,
    stored_effective_hours: &mut f32,
    real_hours: f32,
    attr: &impl PlayerAttributes,
) -> TrainingGain {
    if !real_hours.is_finite() || real_hours <= 0.0 {
        return TrainingGain::default();
    }
    let aptitude = skill.governing_aptitude(attr);
    let effective_gain = real_hours * aptitude_training_multiplier(aptitude);
    if effective_gain <= 0.0 {
        return TrainingGain::default();
    }
    let current = if stored_effective_hours.is_finite() {
        (*stored_effective_hours).max(0.0)
    } else {
        0.0
    };
    let available = (skill.hours_for_rank(aptitude) - current).max(0.0);
    let accepted = effective_gain.min(available);
    *stored_effective_hours = current + accepted;
    TrainingGain {
        accepted_effective_hours: accepted,
        excess_effective_hours: effective_gain - accepted,
    }
}

/// Skill category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillKind {
    /// Physical: governed by a fixed healthy-limb Agility weighting.
    Physical,
    /// Mental: governed by either Instinct or Intelligence.
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
        let mut check = skill.capped_training_rank(hours_trained, attr);
        // Injury remains an explicit performance penalty. Aptitude itself is
        // healthy/raw and contributes nothing to the final check value.
        if skill.is_mental() {
            check *= body.body_part_health(BodyPart::Head).clamp(0.0, 1.0);
        }
        if skill.is_physical() {
            let usable_limbs = BodyPart::LIMBS.iter().fold(0.0, |sum, part| {
                sum + weights.by_part(part).clamp(0.0, 1.0)
                    * body.body_part_health(part).clamp(0.0, 1.0)
            });
            check *= usable_limbs;
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
