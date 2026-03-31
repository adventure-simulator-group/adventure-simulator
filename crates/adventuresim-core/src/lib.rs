use crate::{
    attribute::{Attribute, PlayerAttributes},
    body::{BodyPart, PlayerBody},
    equipment::PlayerEquipment,
    essential::PlayerEssentials,
    skill::{PlayerSkills, Skill},
};

pub mod attribute;
pub mod body;
pub mod equipment;
pub mod essential;
pub mod skill;
#[doc(hidden)]
pub mod stub;

pub mod prelude {
    pub use crate::attribute::*;
    pub use crate::body::*;
    pub use crate::equipment::*;
    pub use crate::essential::*;
    pub use crate::skill::*;
    pub use crate::PlayerInfo;
}

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
/// assert!(
///     matches!(player, PlayerInfo {
///         attributes: StubAttributes,
///         body: StubBody,
///         essentials: StubEssentials,
///         equipment: StubEquipment,
///         skills: StubSkills,
///     })
/// );
/// ```
///
/// ## Trait Methods Available Based on Parts
///
/// When a part is present, you can use methods from the corresponding trait:
/// - If `At: PlayerAttributes`:  use [`PlayerAttributes::attr`] to get attribute values
/// - If `Bd: PlayerBody`:  use [`PlayerBody::body_part_health`] and [`PlayerBody::body_weight`]
/// - If `Es: PlayerEssentials`:  use [`PlayerEssentials::calories_used_today`] and [`PlayerEssentials::focus_level`]
/// - If `Eq: PlayerEquipment`:  use [`PlayerEquipment::encumbrance_penalty`] to get equipment penalties
/// - If `Sl: PlayerSkills`:  use [`PlayerSkills::skill_hours_trained`] to get training hours
///
/// ```
/// # use adventuresim_core::prelude::*;
/// # use adventuresim_core::stub::{StubAttributes, StubSkills};
/// // Create a player with attributes and skills
/// let player = PlayerInfo::empty()
///     .with_attributes(StubAttributes)
///     .with_skills(StubSkills);
///
/// // We can call PlayerAttributes methods via the trait
/// let _strength = player.attr(Attribute::Strength);
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
/// # use adventuresim_core::stub::{StubAttributes, StubEssentials, StubEquipment, StubSkills};
/// // Build player with all required parts for skill_check
/// let player = PlayerInfo::empty()
///     .with_attributes(StubAttributes)
///     .with_essentials(StubEssentials)
///     .with_equipment(StubEquipment)
///     .with_skills(StubSkills);
///
/// // Use the shorthand - combines skills, attributes, essentials, and equipment internally
/// let check = player.skill_check(Skill::Melee);
///
/// // This is equivalent to calling skill_check_by_parts with explicit parts:
/// let check_explicit = player.skill_check_by_parts(
///     Skill::Melee,
///     &player.attributes,
///     &player.essentials,
///     &player.equipment,
/// );
///
/// assert_eq!(check, check_explicit);
/// ```
///
/// Another example: [`PlayerInfo::fatigue`] combines essentials and attributes:
///
/// ```
/// # use adventuresim_core::prelude::*;
/// # use adventuresim_core::stub::{StubAttributes, StubEssentials};
/// let player = PlayerInfo::empty()
///     .with_attributes(StubAttributes)
///     .with_essentials(StubEssentials);
///
/// // Shorthand method
/// let fatigue = player.fatigue();
///
/// // Equivalent explicit call
/// let fatigue_explicit= player.fatigue_by_parts(&player.attributes);
///
/// assert_eq!(fatigue, fatigue_explicit);
/// ```
#[derive(Default, Debug)]
pub struct PlayerInfo<At = (), Bd = (), Es = (), Eq = (), Sl = ()> {
    pub attributes: At,
    pub body: Bd,
    pub essentials: Es,
    pub equipment: Eq,
    pub skills: Sl,
}

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

impl<At, Bd, Es, Eq, Sl> PlayerInfo<At, Bd, Es, Eq, Sl> {
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

impl<A, Bd, E, Eq, S> PlayerAttributes for PlayerInfo<A, Bd, E, Eq, S>
where
    A: PlayerAttributes,
{
    fn attr(&self, attr: Attribute) -> f32 {
        self.attributes.attr(attr)
    }
}

impl<A, Bd, E, Eq, S> PlayerBody for PlayerInfo<A, Bd, E, Eq, S>
where
    Bd: PlayerBody,
{
    fn body_part_health(&self, part: BodyPart) -> f32 {
        self.body.body_part_health(part)
    }

    fn body_weight(&self) -> f32 {
        self.body.body_weight()
    }
}

impl<A, Bd, E, Eq, S> PlayerEssentials for PlayerInfo<A, Bd, E, Eq, S>
where
    E: PlayerEssentials,
{
    fn calories_used_today(&self) -> f32 {
        self.essentials.calories_used_today()
    }

    fn focus_level(&self) -> f32 {
        self.essentials.focus_level()
    }
}

impl<A, Bd, E, Eq, S> PlayerEquipment for PlayerInfo<A, Bd, E, Eq, S>
where
    Eq: PlayerEquipment,
{
    fn weapon_accuracy(&self) -> f32 {
        self.equipment.weapon_accuracy()
    }

    fn armor_dodge(&self) -> f32 {
        self.equipment.armor_dodge()
    }

    fn inventory_weight(&self) -> f32 {
        self.equipment.inventory_weight()
    }

    fn shield_block_bonus(&self) -> f32 {
        self.equipment.shield_block_bonus()
    }
}

impl<A, Bd, E, Eq, S> PlayerSkills for PlayerInfo<A, Bd, E, Eq, S>
where
    S: PlayerSkills,
{
    fn skill_hours_trained(&self, skill: Skill) -> f32 {
        self.skills.skill_hours_trained(skill)
    }
}

impl<A, Bd, E, Eq, S> PlayerInfo<A, Bd, E, Eq, S>
where
    A: PlayerAttributes,
    E: PlayerEssentials,
{
    pub fn fatigue(&self) -> f32 {
        self.fatigue_by_parts(&self.attributes)
    }
}

impl<A, Bd, E, Eq, S> PlayerInfo<A, Bd, E, Eq, S>
where
    A: PlayerAttributes,
    E: PlayerEssentials,
    Eq: PlayerEquipment,
    S: PlayerSkills,
{
    pub fn skill_check(&self, skill: Skill) -> f32 {
        self.skills
            .skill_check_by_parts(skill, &self.attributes, &self.essentials, &self.equipment)
    }
}
