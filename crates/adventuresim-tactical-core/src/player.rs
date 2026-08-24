use adventuresim_core::prelude::*;
use bevy::{ecs::system::SystemParam, prelude::*};
use bevy_enhanced_input::prelude::Actions;
use serde::{Deserialize, Serialize};

use crate::{
    animation::{AttackCurve, AttackHand, AttackSpec},
    inventory::{InventoryView, InventoryViewer},
};

/// BEI Component alias to mark players that are controlled by the present client.
pub type ControlledPlayer = Actions<Player>;

/// Component for a player entity, for both client-controlled
/// active player and other players.
#[derive(Component, Serialize, Deserialize, Debug, Reflect, Clone, PartialEq, Eq)]
#[reflect(Component)]
#[require(
    CharacterId,
    Limbs,
    Skills,
    Attributes,
    Stats,
    crate::animation::SkeletonState
)]
#[component(immutable)]
pub struct Player {
    pub name: String,
}

/// Transient tactical allegiance. This is authoritative on the tactical
/// server and replicated so clients can present enemy-only combat UI without
/// inferring allegiance from connectivity or local control.
#[derive(Component, Serialize, Deserialize, Debug, Reflect, Clone, Copy, PartialEq, Eq)]
#[reflect(Component)]
#[component(immutable)]
pub enum TacticalCombatSide {
    Party,
    Enemy,
}

pub fn default_tactical_character_id() -> u64 {
    adventuresim_core::starting_character::default_character("tactical").id
}

impl Default for Player {
    fn default() -> Self {
        Self {
            name: adventuresim_core::starting_character::DEFAULT_CHARACTER_NAME.into(),
        }
    }
}

/// Strategic character identity projected into the transient tactical world.
/// Network client identity remains a separate transport concern.
#[derive(
    Component, Serialize, Deserialize, Default, Debug, Reflect, Clone, Copy, PartialEq, Eq, Hash,
)]
#[reflect(Component)]
#[component(immutable)]
pub struct CharacterId(pub u64);

impl CharacterId {
    /// Get associated color of this player.
    pub fn color(&self) -> Color {
        // SplitMix64-style mixing for good bit diffusion
        let mut x = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^= x >> 31;

        let hue = (x % 360) as f32;
        let saturation = 0.28 + ((x >> 8) & 0xFF) as f32 / 255.0 * 0.18;
        let value = 0.90 + ((x >> 16) & 0xFF) as f32 / 255.0 * 0.08;

        Color::hsv(hue, saturation, value)
    }
}

/// Creature families used to select the attacker's anatomical lore.
///
/// Current tactical characters are Human by default. Multi-category enemies
/// can provide every applicable category without changing combat resolution.
#[derive(Component, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[component(immutable)]
pub struct BestiaryCategories(pub Vec<BestiaryCategory>);

impl Default for BestiaryCategories {
    fn default() -> Self {
        Self(vec![BestiaryCategory::Human])
    }
}

/// General player stats.
#[derive(Component, Serialize, Deserialize, Debug, Reflect, Clone, PartialEq)]
#[reflect(Component)]
pub struct Stats {
    pub calories_used: f32,
    pub focus: f32,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            calories_used: 0.0,
            focus: 1.0,
        }
    }
}

impl PlayerEssentials for Stats {
    fn calories_used_today(&self) -> f32 {
        self.calories_used
    }

    fn focus_level(&self) -> f32 {
        self.focus
    }
}

/// Live, server-authoritative combat effects. This component is replicated for
/// presentation but remains transient and is never written to SpacetimeDB.
#[derive(Component, Serialize, Deserialize, Debug, Reflect, Clone, PartialEq)]
#[reflect(Component)]
pub struct TacticalCombatState {
    pub starting_incapacitation: f32,
    pub starting_blood_fraction: f32,
    pub starting_fear: f32,
    pub starting_fatigue: f32,
    pub starting_hunger: f32,
    pub starting_thirst: f32,
    pub starting_thermal: f32,
    pub blood_loss_fraction: f32,
    pub exhaustion: f32,
    pub imbalance: f32,
    pub incapacitation: f32,
}

impl Default for TacticalCombatState {
    fn default() -> Self {
        Self {
            starting_incapacitation: 0.0,
            starting_blood_fraction: 1.0,
            starting_fear: 0.0,
            starting_fatigue: 0.0,
            starting_hunger: 0.0,
            starting_thirst: 0.0,
            starting_thermal: 0.0,
            blood_loss_fraction: 0.0,
            exhaustion: 0.0,
            imbalance: 0.0,
            incapacitation: 0.0,
        }
    }
}

impl TacticalCombatState {
    /// Returns the source values represented by the tactical incapacitation
    /// wheel. Pain and blood loss are recomputed live in combat; the remaining
    /// strategic sources retain their enrollment-time breakdown.
    pub fn incapacitation_sources(
        &self,
        total_limb_damage: f32,
        will_check: f32,
    ) -> TacticalIncapacitationSources {
        let remaining_blood =
            (self.starting_blood_fraction - self.blood_loss_fraction).clamp(0.0, 1.0);
        TacticalIncapacitationSources {
            pain: pain_incapacitation(total_limb_damage, will_check),
            blood_loss: blood_loss_incapacitation(remaining_blood, 1.0),
            fear: self.starting_fear.max(0.0),
            fatigue: self.starting_fatigue.max(0.0),
            hunger: self.starting_hunger.max(0.0),
            thirst: self.starting_thirst.max(0.0),
            thermal: self.starting_thermal.max(0.0),
            exhaustion: self.exhaustion.max(0.0),
            imbalance: self.imbalance.max(0.0),
        }
    }

    /// Derives readiness from the one replicated incapacitation value.
    ///
    /// Readiness is intentionally not stored separately: clients, authority
    /// checks, AI, and mission resolution therefore cannot observe divergent
    /// boolean/component copies of the same state.
    pub fn incapacitation_status(&self) -> IncapacitationStatus {
        match self.incapacitation {
            total if total >= 1.0 => IncapacitationStatus::Incapacitated,
            total if total > 0.5 => IncapacitationStatus::Staggered,
            _ => IncapacitationStatus::Ready,
        }
    }

    pub fn is_incapacitated(&self) -> bool {
        self.incapacitation_status() == IncapacitationStatus::Incapacitated
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TacticalIncapacitationSources {
    pub pain: f32,
    pub blood_loss: f32,
    pub fear: f32,
    pub fatigue: f32,
    pub hunger: f32,
    pub thirst: f32,
    pub thermal: f32,
    pub exhaustion: f32,
    pub imbalance: f32,
}

impl TacticalIncapacitationSources {
    pub fn total(self) -> f32 {
        self.pain
            + self.blood_loss
            + self.fear
            + self.fatigue
            + self.hunger
            + self.thirst
            + self.thermal
            + self.exhaustion
            + self.imbalance
    }
}

/// Limb health status.
#[derive(Component, Serialize, Deserialize, Debug, Reflect, Clone, PartialEq)]
#[reflect(Component)]
pub struct Limbs {
    pub body_weight_kg: f32,
    pub left_arm: f32,
    pub right_arm: f32,
    pub left_leg: f32,
    pub right_leg: f32,
    pub chest: f32,
    pub stomach: f32,
    pub head: f32,
}

impl Default for Limbs {
    fn default() -> Self {
        Self {
            body_weight_kg: 70.0,
            left_arm: 1.0,
            right_arm: 1.0,
            left_leg: 1.0,
            right_leg: 1.0,
            chest: 1.0,
            stomach: 1.0,
            head: 1.0,
        }
    }
}

impl Limbs {
    pub fn health_mut(&mut self, part: BodyPart) -> &mut f32 {
        match part {
            BodyPart::LeftArm => &mut self.left_arm,
            BodyPart::RightArm => &mut self.right_arm,
            BodyPart::LeftLeg => &mut self.left_leg,
            BodyPart::RightLeg => &mut self.right_leg,
            BodyPart::Chest => &mut self.chest,
            BodyPart::Stomach => &mut self.stomach,
            BodyPart::Head => &mut self.head,
        }
    }
}

impl PlayerBody for Limbs {
    fn body_part_health(&self, part: BodyPart) -> f32 {
        match part {
            BodyPart::LeftArm => self.left_arm,
            BodyPart::RightArm => self.right_arm,
            BodyPart::LeftLeg => self.left_leg,
            BodyPart::RightLeg => self.right_leg,
            BodyPart::Chest => self.chest,
            BodyPart::Stomach => self.stomach,
            BodyPart::Head => self.head,
        }
    }

    fn body_weight(&self) -> f32 {
        self.body_weight_kg
    }

    fn primary_side(&self) -> BodySide {
        // TODO: this should be stored in DB ?
        BodySide::Right
    }
}

impl Limbs {
    /// Applies up to `damage` (0-1 scale) to `part`'s health, clamped to what
    /// remains, and returns how much was actually applied. Mirrors
    /// `CombatBody::apply_damage` in `adventuresim_core::autoresolve`.
    pub fn apply_damage(&mut self, part: BodyPart, damage: f32) -> f32 {
        let health = match part {
            BodyPart::LeftArm => &mut self.left_arm,
            BodyPart::RightArm => &mut self.right_arm,
            BodyPart::LeftLeg => &mut self.left_leg,
            BodyPart::RightLeg => &mut self.right_leg,
            BodyPart::Chest => &mut self.chest,
            BodyPart::Stomach => &mut self.stomach,
            BodyPart::Head => &mut self.head,
        };
        let applied = damage.max(0.0).min(health.max(0.0));
        *health = (*health - applied).max(0.0);
        applied
    }

    /// Aggregate health deficit across all body parts, used as the `pain`
    /// input to [`TacticalCombatState::incapacitation_sources`].
    pub fn total_damage(&self) -> f32 {
        [
            self.left_arm,
            self.right_arm,
            self.left_leg,
            self.right_leg,
            self.chest,
            self.stomach,
            self.head,
        ]
        .into_iter()
        .map(|health| (1.0 - health).max(0.0))
        .sum()
    }
}

/// Physical and mental skills of a [`Player`].
#[derive(Component, Serialize, Deserialize, Debug, Reflect, Clone, PartialEq)]
#[reflect(Component)]
#[component(immutable)]
pub struct Skills {
    pub polearm_hours: f32,
    pub axe_hours: f32,
    pub bludgeon_hours: f32,
    pub sword_hours: f32,
    pub knife_hours: f32,
    pub dodge_hours: f32,
    pub block_hours: f32,
    pub bow_hours: f32,
    pub crossbow_hours: f32,
    pub firearm_hours: f32,
    pub throw_hours: f32,
    pub will_hours: f32,
    pub insight_hours: f32,
    pub charm_hours: f32,
    pub command_hours: f32,
    pub deception_hours: f32,
    pub physiology_hours: f32,
    pub religion_hours: f32,
    pub bestiary_beast_hours: f32,
    pub bestiary_undead_hours: f32,
    pub bestiary_human_hours: f32,
    pub bestiary_werekin_hours: f32,
    pub bestiary_elf_hours: f32,
    pub bestiary_dwarf_hours: f32,
    pub bestiary_fey_hours: f32,
    pub bestiary_spirit_hours: f32,
    pub bestiary_greenskin_hours: f32,
    pub bestiary_insectoid_hours: f32,
    pub bestiary_draconid_hours: f32,
    pub bestiary_construct_hours: f32,
    pub bestiary_wildmen_hours: f32,
    pub surgery_hours: f32,
    pub stealth_hours: f32,
    pub balance_hours: f32,
    pub tailoring_hours: f32,
    pub smithing_hours: f32,
}

impl Default for Skills {
    fn default() -> Self {
        let default = adventuresim_core::starting_character::default_character("tactical");
        let skills = default.skills;
        Self {
            polearm_hours: skills.polearm,
            axe_hours: skills.axe,
            bludgeon_hours: skills.bludgeon,
            sword_hours: skills.sword,
            knife_hours: skills.knife,
            dodge_hours: skills.dodge,
            block_hours: skills.block,
            bow_hours: skills.bow,
            crossbow_hours: skills.crossbow,
            firearm_hours: skills.firearm,
            throw_hours: skills.throw,
            will_hours: skills.will,
            insight_hours: skills.insight,
            charm_hours: skills.charm,
            command_hours: skills.command,
            deception_hours: skills.deception,
            physiology_hours: skills.physiology,
            religion_hours: [
                skills.religion.roman_catholic,
                skills.religion.lutheran,
                skills.religion.reformed,
                skills.religion.anglican,
                skills.religion.eastern_orthodox,
                skills.religion.islamic,
                skills.religion.judaism,
            ]
            .into_iter()
            .sum(),
            bestiary_beast_hours: skills.bestiary.beast,
            bestiary_undead_hours: skills.bestiary.undead,
            bestiary_human_hours: skills.bestiary.human,
            bestiary_werekin_hours: skills.bestiary.werekin,
            bestiary_elf_hours: skills.bestiary.elf,
            bestiary_dwarf_hours: skills.bestiary.dwarf,
            bestiary_fey_hours: skills.bestiary.fey,
            bestiary_spirit_hours: skills.bestiary.spirit,
            bestiary_greenskin_hours: skills.bestiary.greenskin,
            bestiary_insectoid_hours: skills.bestiary.insectoid,
            bestiary_draconid_hours: skills.bestiary.draconid,
            bestiary_construct_hours: skills.bestiary.construct,
            bestiary_wildmen_hours: skills.bestiary.wildmen,
            surgery_hours: skills.surgery,
            stealth_hours: skills.stealth,
            balance_hours: skills.balance,
            tailoring_hours: skills.tailoring,
            smithing_hours: skills.smithing,
        }
    }
}

impl Skills {
    fn bestiary_hours(&self) -> BestiaryHours {
        BestiaryHours {
            beast: self.bestiary_beast_hours,
            undead: self.bestiary_undead_hours,
            human: self.bestiary_human_hours,
            werekin: self.bestiary_werekin_hours,
            elf: self.bestiary_elf_hours,
            dwarf: self.bestiary_dwarf_hours,
            fey: self.bestiary_fey_hours,
            spirit: self.bestiary_spirit_hours,
            greenskin: self.bestiary_greenskin_hours,
            insectoid: self.bestiary_insectoid_hours,
            draconid: self.bestiary_draconid_hours,
            construct: self.bestiary_construct_hours,
            wildmen: self.bestiary_wildmen_hours,
        }
    }
}

impl PlayerSkills for Skills {
    fn skill_hours_trained(&self, skill: Skill) -> f32 {
        match skill {
            Skill::Polearm => self.polearm_hours,
            Skill::Axe => self.axe_hours,
            Skill::Bludgeon => self.bludgeon_hours,
            Skill::Sword => self.sword_hours,
            Skill::Knife => self.knife_hours,
            Skill::Block => self.block_hours,
            Skill::Dodge => self.dodge_hours,
            Skill::Bow => self.bow_hours,
            Skill::Crossbow => self.crossbow_hours,
            Skill::Firearm => self.firearm_hours,
            Skill::Throw => self.throw_hours,
            Skill::Will => self.will_hours,
            Skill::Insight => self.insight_hours,
            Skill::Charm => self.charm_hours,
            Skill::Command => self.command_hours,
            Skill::Deception => self.deception_hours,
            Skill::Physiology => self.physiology_hours,
            Skill::Cooking => 0.0,
            Skill::Herbalism => 0.0,
            Skill::Religion => self.religion_hours,
            Skill::Bestiary => self.bestiary_hours().aggregate_effective(),
            Skill::Surgery => self.surgery_hours,
            Skill::Stealth => self.stealth_hours,
            Skill::Balance => self.balance_hours,
            Skill::TerrainPlains
            | Skill::TerrainForest
            | Skill::TerrainHills
            | Skill::TerrainWetlands
            | Skill::TerrainUrban
            | Skill::TerrainSnow => 0.0,
            Skill::Tailoring => self.tailoring_hours,
            Skill::Smithing => self.smithing_hours,
        }
    }

    fn bestiary_hours_for(&self, category: BestiaryCategory) -> f32 {
        self.bestiary_hours().effective(category)
    }
}

/// Genetic attributes of a [`Player`].
#[derive(Component, Serialize, Deserialize, Debug, Reflect, Clone, PartialEq)]
#[reflect(Component)]
#[component(immutable)]
pub struct Attributes {
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

impl Default for Attributes {
    fn default() -> Self {
        let default = adventuresim_core::starting_character::default_character("tactical");
        let attributes = default.attributes;
        Self {
            endurance: attributes.endurance,
            immunity: attributes.immunity,
            gut: attributes.gut,
            intelligence: attributes.intelligence,
            instinct: attributes.instinct,
            eyesight: attributes.eyesight,
            hearing: attributes.hearing,
            left_arm_strength: attributes.strength,
            right_arm_strength: attributes.strength,
            left_leg_strength: attributes.strength,
            right_leg_strength: attributes.strength,
            left_arm_agility: attributes.agility,
            right_arm_agility: attributes.agility,
            left_leg_agility: attributes.agility,
            right_leg_agility: attributes.agility,
        }
    }
}

impl PlayerAttributes for Attributes {
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
            _ => unimplemented!(),
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

pub type TacticalPlayerView<'v, 'w, 's> =
    PlayerInfo<&'v Attributes, &'v Limbs, &'v Stats, InventoryView<'v, 'w, 's>, &'v Skills>;

#[derive(SystemParam)]
pub struct TacticalPlayerViewer<'w, 's> {
    pub inventory: InventoryViewer<'w, 's>,
    pub q_player: Query<
        'w,
        's,
        (
            &'static Limbs,
            &'static Skills,
            &'static Stats,
            &'static Attributes,
        ),
    >,
}

impl TacticalPlayerViewer<'_, '_> {
    pub fn get(&self, entity: Entity) -> Result<TacticalPlayerView<'_, '_, '_>> {
        let (limbs, skills, stats, attributes) = self.q_player.get(entity)?;
        let inventory = self.inventory.get(entity);
        Ok(PlayerInfo::empty()
            .with_attributes(attributes)
            .with_body(limbs)
            .with_essentials(stats)
            .with_equipment(inventory)
            .with_skills(skills))
    }

    pub fn get_for_attack(
        &self,
        entity: Entity,
        hand: AttackHand,
    ) -> Result<TacticalPlayerView<'_, '_, '_>> {
        let (limbs, skills, stats, attributes) = self.q_player.get(entity)?;
        let inventory = self.inventory.get_for_attack(entity, hand);
        Ok(PlayerInfo::empty()
            .with_attributes(attributes)
            .with_body(limbs)
            .with_essentials(stats)
            .with_equipment(inventory)
            .with_skills(skills))
    }
}

/// Effective leaf-skill check used by attack handling. The weapon's authored
/// skill distribution is the single selector, including the shared unarmed
/// bludgeon fallback.
pub fn effective_weapon_handling_skill(view: &TacticalPlayerView<'_, '_, '_>) -> f32 {
    view.weapon_skill_distribution()
        .weighted_check(|skill| view.skill_check(skill, LimbWeights::all_equal()))
}

/// Applies the attacker's current equipment and skill to the replicated pose
/// curve. Player input and NPC behavior call this same function.
pub fn configure_attack_curve(
    mut spec: AttackSpec,
    view: &TacticalPlayerView<'_, '_, '_>,
) -> AttackSpec {
    spec.curve = AttackCurve::from_handling(
        view.weapon_moment_of_inertia(),
        effective_weapon_handling_skill(view),
    );
    spec
}

/// Contact timing combines authored rotational inertia with the attacker's arm
/// strength. The correction is deliberately bounded: strength helps control a
/// difficult weapon but does not erase the physical distinction between it
/// and a knife.
pub fn attack_preparation_secs(
    view: &TacticalPlayerView<'_, '_, '_>,
    style: MeleeAttackStyle,
) -> f32 {
    let inertia = view.weapon_moment_of_inertia().max(0.0);
    let inertia_difficulty = if view.weapon_is_unarmed() {
        0.35
    } else {
        (inertia / (inertia + 0.45)).sqrt()
    };
    let strength = view.limb_attr_by_weight(LimbAttribute::Strength, LimbWeights::both_arms());
    let strength_scale = (1.0 + (3.0 - strength) * 0.08 * inertia_difficulty).clamp(0.85, 1.20);
    (view.weapon_windup_secs_for(style) * strength_scale).clamp(0.08, 0.75)
}

/// Skill mostly improves braking and redirection rather than raw peak weapon
/// speed. Continuations save additional recovery but remain bounded.
pub fn attack_recovery_secs(
    view: &TacticalPlayerView<'_, '_, '_>,
    style: MeleeAttackStyle,
    continuation: bool,
) -> f32 {
    let control = (effective_weapon_handling_skill(view) / 5.0).clamp(0.0, 1.0);
    let skill_scale = 1.12 - 0.28 * control;
    let continuation_scale = if continuation { 0.78 } else { 1.0 };
    (view.weapon_recovery_secs_for(style) * skill_scale * continuation_scale).clamp(0.08, 0.55)
}

#[cfg(test)]
mod tactical_combat_state_tests {
    use super::*;

    #[test]
    fn component_defaults_project_john_fabelgeist() {
        let player = Player::default();
        let attributes = Attributes::default();
        let skills = Skills::default();
        assert_eq!(player.name, "John Fabelgeist");
        assert_eq!(attributes.endurance, 4.0);
        assert_eq!(attributes.left_arm_strength, 4.0);
        assert_eq!(attributes.right_leg_agility, 4.0);
        assert_eq!(attributes.intelligence, 3.0);
        assert_eq!(attributes.instinct, 3.0);
        assert_eq!(
            Skill::Insight.capped_training_rank(skills.insight_hours, &attributes),
            3.0
        );
        assert_eq!(
            Skill::Command.capped_training_rank(skills.command_hours, &attributes),
            3.0
        );
    }

    #[test]
    fn wheel_sources_preserve_strategic_breakdown_and_recompute_live_values() {
        let state = TacticalCombatState {
            starting_incapacitation: 0.35,
            starting_blood_fraction: 0.85,
            starting_fear: 0.1,
            starting_fatigue: 0.05,
            starting_hunger: 0.08,
            starting_thirst: 0.07,
            starting_thermal: 0.05,
            blood_loss_fraction: 0.15,
            exhaustion: 0.12,
            imbalance: 0.2,
            ..default()
        };

        let sources = state.incapacitation_sources(0.0, 4.0);
        assert_eq!(sources.pain, 0.0);
        assert!((sources.blood_loss - 1.0).abs() < 0.0001);
        assert_eq!(sources.fear, 0.1);
        assert_eq!(sources.fatigue, 0.05);
        assert_eq!(sources.hunger, 0.08);
        assert_eq!(sources.thirst, 0.07);
        assert_eq!(sources.thermal, 0.05);
        assert_eq!(sources.exhaustion, 0.12);
        assert_eq!(sources.imbalance, 0.2);
        assert!((sources.total() - 1.67).abs() < 0.0001);
        assert!(
            (sources.total()
                - combat_incapacitation(
                    state.starting_incapacitation,
                    state.starting_blood_fraction,
                    state.blood_loss_fraction,
                    0.0,
                    4.0,
                    state.imbalance,
                )
                - state.exhaustion)
                .abs()
                < 0.0001
        );
    }
}
