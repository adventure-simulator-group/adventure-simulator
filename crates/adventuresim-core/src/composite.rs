use crate::{attribute::*, body::*, combat::*, equipment::*, essential::*, skill::*};

/// A composite type that holds all aspects of a player's state.
///
/// ## Composite Nature
///
/// `PlayerInfo<At, Bd, Es, Eq, Sl>` is generic over five type parameters:
/// - `At` - Player attributes (implements [`PlayerAttributes`])
/// - `Bd` - Player body (implements [`PlayerBody`])
/// - `Es` - Player essentials like calories and focus (implements [`PlayerEssentials`])
/// - `Eq` - Player equipment (implements [`PlayerEquipment`])
/// - `Sl` - Player skills (implements [`PlayerSkills`])
///
/// By default, each part is `()`, meaning it's not present. When a part is
/// present (not `()`), methods from the corresponding trait become available.
///
/// ## Example: Building a Player
///
/// ```
/// # use adventuresim_core::prelude::*;
/// # use adventuresim_core::stub::{StubAttributes, StubBody, StubEssentials, StubEquipment, StubSkills};
/// // Start with an empty player
/// let player = PlayerInfo::empty();
///
/// // Add attributes
/// let player = player.with_attributes(StubAttributes);
///
/// // Add body
/// let player = player.with_body(StubBody);
///
/// // Add essentials
/// let player = player.with_essentials(StubEssentials);
///
/// // Add equipment
/// let player = player.with_equipment(StubEquipment);
///
/// // Add skills
/// let player = player.with_skills(StubSkills);
///
/// // The builder records every component in the resulting composite type.
/// let _: PlayerInfo<StubAttributes, StubBody, StubEssentials, StubEquipment, StubSkills> = player;
/// ```
///
/// ## Trait Methods Available Based on Parts
///
/// When a part is present, you can use methods from the corresponding trait:
/// - If `At: PlayerAttributes`:  use [`PlayerAttributes::attr`] to get attribute values
/// - If `Bd: PlayerBody`:  use [`PlayerBody::body_part_health`] and [`PlayerBody::body_weight`]
/// - If `Es: PlayerEssentials`:  use [`PlayerEssentials::calories_used_today`] and [`PlayerEssentials::focus_level`]
/// - If `Eq: PlayerEquipment`:  use [`PlayerEquipment::armor_dodgerm`] to get equipment penalties
/// - If `Sl: PlayerSkills`:  use [`PlayerSkills::skill_hours_trained`] to get training hours
///
/// ```
/// # use adventuresim_core::prelude::*;
/// # use adventuresim_core::stub::{StubAttributes, StubBody, StubSkills};
/// // Create a player with attributes and skills
/// let player = PlayerInfo::empty()
///     .with_attributes(StubAttributes)
///     .with_body(StubBody)
///     .with_skills(StubSkills);
///
/// // We can call PlayerAttributes methods via the trait
/// let _strength = player.attr(LimbAttribute::Strength);
///
/// // We can call PlayerSkills methods via the trait
/// let _hours = player.skill_hours_trained(Skill::Melee);
/// ```
///
/// ## Combined/Shorthand Methods
///
/// When multiple parts are present, additional convenience methods become available
/// that combine data from multiple parts. For example, [`PlayerInfo::skill_check`] combines
/// skills, attributes, essentials, and equipment to compute a skill check result.
///
/// This is more convenient than calling [`PlayerSkills::skill_check_by_parts`]
/// directly, which requires passing all parts as separate arguments.
///
/// ```
/// # use adventuresim_core::prelude::*;
/// # use adventuresim_core::stub::{StubAttributes, StubBody, StubEssentials, StubEquipment, StubSkills};
/// let attributes = StubAttributes;
/// let body = StubBody;
/// let essentials = StubEssentials;
/// let equipment = StubEquipment;
/// let skills = StubSkills;
/// let weights = LimbWeights::all_equal();
///
/// // Calling skill_check_by_parts explicitly uses the same component values.
/// let check_explicit = skills.skill_check_by_parts(
///     Skill::Melee,
///     &attributes,
///     &body,
///     &essentials,
///     &equipment,
///     weights,
/// );
///
/// // Build player with all required parts for skill_check
/// let player = PlayerInfo::empty()
///     .with_attributes(attributes)
///     .with_body(body)
///     .with_essentials(essentials)
///     .with_equipment(equipment)
///     .with_skills(skills);
///
/// // Use the shorthand - combines skills, attributes, essentials, and equipment internally
/// let check = player.skill_check(Skill::Melee, weights);
///
/// assert_eq!(check, check_explicit);
/// ```
///
/// Another example: [`PlayerInfo::fatigue`] combines essentials and attributes:
///
/// ```
/// # use adventuresim_core::prelude::*;
/// # use adventuresim_core::stub::{StubAttributes, StubBody, StubEssentials};
/// let attributes = StubAttributes;
/// let body = StubBody;
/// let essentials = StubEssentials;
///
/// // Calling fatigue_by_parts explicitly uses the same component values.
/// let fatigue_explicit = essentials.fatigue_by_parts(&attributes, &body);
///
/// let player = PlayerInfo::empty()
///     .with_attributes(attributes)
///     .with_body(body)
///     .with_essentials(essentials);
///
/// // Shorthand method
/// let fatigue = player.fatigue();
///
/// assert_eq!(fatigue, fatigue_explicit);
/// ```
#[derive(Default, Debug, ambassador::Delegate)]
#[delegate(
    PlayerAttributes,
    where = "At: PlayerAttributes",
    target = "attributes"
)]
#[delegate(PlayerBody, where = "Bd: PlayerBody", target = "body")]
#[delegate(
    PlayerEssentials,
    where = "Es: PlayerEssentials",
    target = "essentials"
)]
#[delegate(PlayerEquipment, where = "Eq: PlayerEquipment", target = "equipment")]
#[delegate(PlayerSkills, where = "Sl: PlayerSkills", target = "skills")]
pub struct PlayerInfo<At = (), Bd = (), Es = (), Eq = (), Sl = ()> {
    attributes: At,
    body: Bd,
    essentials: Es,
    equipment: Eq,
    skills: Sl,
}

/// Empty builder block.
impl PlayerInfo<(), (), (), (), ()> {
    /// Create a new empty `PlayerInfo`.
    ///
    /// This is equivalent to calling [`PlayerInfo::empty`].
    pub fn new() -> Self {
        Self::empty()
    }

    /// Create a new empty `PlayerInfo`.
    pub fn empty() -> Self {
        Self::default()
    }
}

/// [`PlayerAttributes`] builder block.
impl<Bd, Es, Eq, Sl> PlayerInfo<(), Bd, Es, Eq, Sl> {
    /// Set the attributes part of the player info.
    pub fn with_attributes<T: PlayerAttributes>(
        self,
        attributes: T,
    ) -> PlayerInfo<T, Bd, Es, Eq, Sl> {
        PlayerInfo {
            attributes,
            body: self.body,
            essentials: self.essentials,
            equipment: self.equipment,
            skills: self.skills,
        }
    }
}

/// [`PlayerBody`] builder block.
impl<At, Es, Eq, Sl> PlayerInfo<At, (), Es, Eq, Sl> {
    /// Set the body part of the player info.
    pub fn with_body<T: PlayerBody>(self, body: T) -> PlayerInfo<At, T, Es, Eq, Sl> {
        PlayerInfo {
            body,
            attributes: self.attributes,
            essentials: self.essentials,
            equipment: self.equipment,
            skills: self.skills,
        }
    }
}

/// [`PlayerEssentials`] builder block.
impl<At, Bd, Eq, Sl> PlayerInfo<At, Bd, (), Eq, Sl> {
    /// Set the essentials part of the player info.
    pub fn with_essentials<T: PlayerEssentials>(
        self,
        essentials: T,
    ) -> PlayerInfo<At, Bd, T, Eq, Sl> {
        PlayerInfo {
            essentials,
            attributes: self.attributes,
            body: self.body,
            equipment: self.equipment,
            skills: self.skills,
        }
    }
}

/// [`PlayerEquipment`] builder block.
impl<At, Bd, Es, Sl> PlayerInfo<At, Bd, Es, (), Sl> {
    /// Set the equipment part of the player info.
    pub fn with_equipment<T: PlayerEquipment>(self, equipment: T) -> PlayerInfo<At, Bd, Es, T, Sl> {
        PlayerInfo {
            equipment,
            essentials: self.essentials,
            attributes: self.attributes,
            body: self.body,
            skills: self.skills,
        }
    }
}

/// [`PlayerSkills`] builder block.
impl<At, Bd, Es, Eq> PlayerInfo<At, Bd, Es, Eq, ()> {
    /// Set the skills part of the player info.
    pub fn with_skills<T: PlayerSkills>(self, skills: T) -> PlayerInfo<At, Bd, Es, Eq, T> {
        PlayerInfo {
            skills,
            essentials: self.essentials,
            attributes: self.attributes,
            body: self.body,
            equipment: self.equipment,
        }
    }
}

impl<At, Bd, Es, Eq, Sl> PlayerInfo<At, Bd, Es, Eq, Sl>
where
    At: PlayerAttributes,
    Bd: PlayerBody,
    Es: PlayerEssentials,
{
    pub fn fatigue(&self) -> f32 {
        self.fatigue_by_parts(&self.attributes, &self.body)
    }
}

impl<At, Bd, Es, Eq, Sl> PlayerInfo<At, Bd, Es, Eq, Sl>
where
    At: PlayerAttributes,
    Bd: PlayerBody,
{
    pub fn limb_attr_by_weight(&self, attr: LimbAttribute, weights: LimbWeights) -> f32 {
        self.attributes
            .limb_attr_by_weight_by_parts(attr, &self.body, weights)
    }

    pub fn attr(&self, attr: impl Into<Attribute>) -> f32 {
        self.attributes.attr_by_parts(attr, &self.body)
    }
}

impl<At, Bd, Es, Eq, Sl> PlayerInfo<At, Bd, Es, Eq, Sl>
where
    At: PlayerAttributes,
    Bd: PlayerBody,
    Es: PlayerEssentials,
    Eq: PlayerEquipment,
    Sl: PlayerSkills,
{
    pub fn skill_check(&self, skill: Skill, weights: LimbWeights) -> f32 {
        self.skills.skill_check_by_parts(
            skill,
            &self.attributes,
            &self.body,
            &self.essentials,
            &self.equipment,
            weights,
        )
    }

    pub fn resolve_melee_attack(
        &self,
        side: BodySide,
        defender: &Self,
        defender_response: DefenderResponse,
        hit_precision: f32,
        flanking: f32,
        body_part: BodyPart,
    ) -> AttackResult {
        resolve_melee_attack_by_parts(
            &self.skills,
            &self.attributes,
            &self.body,
            &self.essentials,
            &self.equipment,
            side,
            hit_precision,
            flanking,
            body_part,
            defender_response,
            &defender.skills,
            &defender.attributes,
            &defender.body,
            &defender.essentials,
            &defender.equipment,
        )
    }

    pub fn resolve_ranged_attack(
        &self,
        defender: &Self,
        defender_response: DefenderResponse,
        hit_precision: f32,
        flanking: f32,
        body_part: BodyPart,
    ) -> AttackResult {
        resolve_ranged_attack_by_parts(
            &self.skills,
            &self.attributes,
            &self.body,
            &self.essentials,
            &self.equipment,
            hit_precision,
            flanking,
            body_part,
            defender_response,
            &defender.skills,
            &defender.attributes,
            &defender.body,
            &defender.essentials,
            &defender.equipment,
        )
    }
}
