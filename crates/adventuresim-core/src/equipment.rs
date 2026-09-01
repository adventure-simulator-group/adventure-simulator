#![allow(clippy::crate_in_macro_def)]

use crate::skill::Skill;
use crate::{
    body::{BodyPart, BodyParts, BodySide, LimbWeights, PlayerBody},
    prelude::{LimbAttribute, PlayerAttributes},
};
use crate::{
    combat_style::MeleeAttackStyle,
    item_catalog_schema::{EquipmentChannel, EquipmentLocation},
};
use std::collections::{BTreeMap, BTreeSet};

mod armor;
mod parametric_weapon;

pub use armor::*;
pub use parametric_weapon::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputAddressMapping {
    pub input: &'static str,
    /// Position in the compact QWERTY slot map shown by clients.
    pub keyboard_row: u8,
    pub keyboard_column: u8,
    /// One input may address several physical anchors (for example a belt
    /// spans four authored locations).
    pub locations: &'static [EquipmentLocation],
    /// Repeated selection traverses attached descendants deepest-first in
    /// authored point/capacity order, then their body-root item.
    pub channel_order: &'static [EquipmentChannel],
}

const OUTSIDE_TO_INSIDE: &[EquipmentChannel] = &[
    EquipmentChannel::Mount,
    EquipmentChannel::Accessory,
    EquipmentChannel::Outerwear,
    EquipmentChannel::RigidArmor,
    EquipmentChannel::FlexibleArmor,
    EquipmentChannel::Padding,
    EquipmentChannel::BaseClothing,
    EquipmentChannel::Containment,
];

pub const INPUT_ADDRESS_MAPPINGS: &[InputAddressMapping] = &[
    InputAddressMapping {
        input: "q",
        keyboard_row: 1,
        keyboard_column: 1,
        locations: &[EquipmentLocation::LeftBelt],
        channel_order: OUTSIDE_TO_INSIDE,
    },
    InputAddressMapping {
        input: "e",
        keyboard_row: 1,
        keyboard_column: 3,
        locations: &[EquipmentLocation::RightBelt],
        channel_order: OUTSIDE_TO_INSIDE,
    },
    InputAddressMapping {
        input: "f",
        keyboard_row: 2,
        keyboard_column: 4,
        locations: &[EquipmentLocation::FrontBelt],
        channel_order: OUTSIDE_TO_INSIDE,
    },
    InputAddressMapping {
        input: "x",
        keyboard_row: 3,
        keyboard_column: 2,
        locations: &[EquipmentLocation::BackBelt],
        channel_order: OUTSIDE_TO_INSIDE,
    },
    InputAddressMapping {
        input: "tab",
        keyboard_row: 1,
        keyboard_column: 0,
        locations: &[EquipmentLocation::LeftShoulder],
        channel_order: OUTSIDE_TO_INSIDE,
    },
    InputAddressMapping {
        input: "r",
        keyboard_row: 1,
        keyboard_column: 4,
        locations: &[EquipmentLocation::RightShoulder],
        channel_order: OUTSIDE_TO_INSIDE,
    },
    InputAddressMapping {
        input: "2",
        keyboard_row: 0,
        keyboard_column: 2,
        locations: &[EquipmentLocation::LeftPocket],
        channel_order: OUTSIDE_TO_INSIDE,
    },
    InputAddressMapping {
        input: "3",
        keyboard_row: 0,
        keyboard_column: 3,
        locations: &[EquipmentLocation::RightPocket],
        channel_order: OUTSIDE_TO_INSIDE,
    },
    InputAddressMapping {
        input: "1",
        keyboard_row: 0,
        keyboard_column: 1,
        locations: &[EquipmentLocation::BackLeftPocket],
        channel_order: OUTSIDE_TO_INSIDE,
    },
    InputAddressMapping {
        input: "4",
        keyboard_row: 0,
        keyboard_column: 4,
        locations: &[EquipmentLocation::BackRightPocket],
        channel_order: OUTSIDE_TO_INSIDE,
    },
    InputAddressMapping {
        input: "t",
        keyboard_row: 1,
        keyboard_column: 5,
        locations: &[
            EquipmentLocation::Head,
            EquipmentLocation::Face,
            EquipmentLocation::Neck,
        ],
        channel_order: OUTSIDE_TO_INSIDE,
    },
    InputAddressMapping {
        input: "z",
        keyboard_row: 3,
        keyboard_column: 1,
        locations: &[EquipmentLocation::LeftFoot],
        channel_order: OUTSIDE_TO_INSIDE,
    },
    InputAddressMapping {
        input: "c",
        keyboard_row: 3,
        keyboard_column: 3,
        locations: &[EquipmentLocation::RightFoot],
        channel_order: OUTSIDE_TO_INSIDE,
    },
    InputAddressMapping {
        input: "`",
        keyboard_row: 0,
        keyboard_column: 0,
        locations: &[EquipmentLocation::LeftArm, EquipmentLocation::LeftHand],
        channel_order: OUTSIDE_TO_INSIDE,
    },
    InputAddressMapping {
        input: "5",
        keyboard_row: 0,
        keyboard_column: 5,
        locations: &[EquipmentLocation::RightArm, EquipmentLocation::RightHand],
        channel_order: OUTSIDE_TO_INSIDE,
    },
    InputAddressMapping {
        input: "g",
        keyboard_row: 2,
        keyboard_column: 5,
        locations: &[EquipmentLocation::Chest],
        channel_order: OUTSIDE_TO_INSIDE,
    },
    InputAddressMapping {
        input: "y",
        keyboard_row: 1,
        keyboard_column: 6,
        locations: &[EquipmentLocation::Stomach],
        channel_order: OUTSIDE_TO_INSIDE,
    },
    InputAddressMapping {
        input: "h",
        keyboard_row: 2,
        keyboard_column: 6,
        locations: &[EquipmentLocation::Back],
        channel_order: OUTSIDE_TO_INSIDE,
    },
    InputAddressMapping {
        input: "v",
        keyboard_row: 3,
        keyboard_column: 4,
        locations: &[EquipmentLocation::LeftLeg],
        channel_order: OUTSIDE_TO_INSIDE,
    },
    InputAddressMapping {
        input: "b",
        keyboard_row: 3,
        keyboard_column: 5,
        locations: &[EquipmentLocation::RightLeg],
        channel_order: OUTSIDE_TO_INSIDE,
    },
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EquipmentGraph {
    pub nodes: BTreeMap<u64, EquipmentGraphPlacement>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EquipmentGraphPlacement {
    pub body: Vec<(EquipmentLocation, EquipmentChannel, u16)>,
    pub parents: Vec<EquipmentGraphEdge>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EquipmentGraphEdge {
    pub parent_inventory_item_id: u64,
    pub attachment_point_id: String,
    pub capacity_index: u16,
}

impl EquipmentGraph {
    pub fn equip(
        &mut self,
        inventory_item_id: u64,
        mut placement: EquipmentGraphPlacement,
    ) -> Result<(), &'static str> {
        if self.has_children(inventory_item_id) {
            return Err("item has equipped children");
        }
        for (_, channel, order) in &mut placement.body {
            if channel.singleton_per_location() {
                *order = 0;
            }
        }
        let body_keys = placement.body.iter().copied().collect::<BTreeSet<_>>();
        if body_keys.len() != placement.body.len() {
            return Err("duplicate body occupancy");
        }
        let edge_keys = placement.parents.iter().cloned().collect::<BTreeSet<_>>();
        if edge_keys.len() != placement.parents.len() {
            return Err("duplicate attachment capacity");
        }
        for (other_id, other) in &self.nodes {
            if *other_id == inventory_item_id {
                continue;
            }
            if other.body.iter().any(|cell| body_keys.contains(cell)) {
                return Err("body occupancy conflict");
            }
            if other.parents.iter().any(|edge| edge_keys.contains(edge)) {
                return Err("attachment capacity conflict");
            }
        }
        if placement
            .parents
            .iter()
            .any(|edge| !self.nodes.contains_key(&edge.parent_inventory_item_id))
        {
            return Err("parent is not equipped");
        }
        if self.would_cycle(
            inventory_item_id,
            placement
                .parents
                .iter()
                .map(|edge| edge.parent_inventory_item_id),
        ) {
            return Err("attachment cycle");
        }
        self.nodes.insert(inventory_item_id, placement);
        Ok(())
    }

    pub fn unequip(&mut self, inventory_item_id: u64) -> Result<(), &'static str> {
        if self.has_children(inventory_item_id) {
            return Err("item has equipped children");
        }
        self.nodes.remove(&inventory_item_id);
        Ok(())
    }

    pub fn has_children(&self, inventory_item_id: u64) -> bool {
        self.nodes.values().any(|placement| {
            placement
                .parents
                .iter()
                .any(|edge| edge.parent_inventory_item_id == inventory_item_id)
        })
    }

    fn would_cycle(&self, inventory_item_id: u64, parents: impl IntoIterator<Item = u64>) -> bool {
        let mut ancestors = parents.into_iter().collect::<Vec<_>>();
        let mut visited = BTreeSet::new();
        while let Some(ancestor) = ancestors.pop() {
            if ancestor == inventory_item_id {
                return true;
            }
            if visited.insert(ancestor)
                && let Some(placement) = self.nodes.get(&ancestor)
            {
                ancestors.extend(
                    placement
                        .parents
                        .iter()
                        .map(|edge| edge.parent_inventory_item_id),
                );
            }
        }
        false
    }
}

/// SpacetimeDB-friendly weights for the nine weapon leaf skills. A weapon may
/// combine melee and ranged families; callers normalize the positive entries.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "spacetimedb", derive(spacetimedb::SpacetimeType))]
pub struct WeaponSkillDistribution {
    pub polearm: f32,
    pub axe: f32,
    pub bludgeon: f32,
    pub sword: f32,
    pub knife: f32,
    pub bow: f32,
    pub crossbow: f32,
    pub firearm: f32,
    pub throw: f32,
}

impl From<crate::item_catalog_schema::WeaponSkills> for WeaponSkillDistribution {
    fn from(skills: crate::item_catalog_schema::WeaponSkills) -> Self {
        Self {
            polearm: skills.polearm,
            axe: skills.axe,
            bludgeon: skills.bludgeon,
            sword: skills.sword,
            knife: skills.knife,
            bow: skills.bow,
            crossbow: skills.crossbow,
            firearm: skills.firearm,
            throw: skills.throw,
        }
    }
}

impl WeaponSkillDistribution {
    pub const UNARMED: Self = Self {
        polearm: 0.0,
        axe: 0.0,
        bludgeon: 1.0,
        sword: 0.0,
        knife: 0.0,
        bow: 0.0,
        crossbow: 0.0,
        firearm: 0.0,
        throw: 0.0,
    };

    pub const SKILLS: [Skill; 9] = [
        Skill::Polearm,
        Skill::Axe,
        Skill::Bludgeon,
        Skill::Sword,
        Skill::Knife,
        Skill::Bow,
        Skill::Crossbow,
        Skill::Firearm,
        Skill::Throw,
    ];

    pub fn weights(self) -> [f32; 9] {
        [
            self.polearm,
            self.axe,
            self.bludgeon,
            self.sword,
            self.knife,
            self.bow,
            self.crossbow,
            self.firearm,
            self.throw,
        ]
        .map(|v| if v.is_finite() { v.max(0.0) } else { 0.0 })
    }

    pub fn total(self) -> f32 {
        self.weights().into_iter().sum()
    }
    pub fn melee_total(self) -> f32 {
        self.weights()[..5].iter().sum()
    }
    pub fn ranged_total(self) -> f32 {
        self.weights()[5..].iter().sum()
    }

    pub fn validate(self, melee: bool, ranged: bool) -> bool {
        let raw = [
            self.polearm,
            self.axe,
            self.bludgeon,
            self.sword,
            self.knife,
            self.bow,
            self.crossbow,
            self.firearm,
            self.throw,
        ];
        raw.into_iter().all(|v| v.is_finite() && v >= 0.0)
            && (!melee || self.melee_total() > 0.0)
            && (!ranged || self.ranged_total() > 0.0)
            && (!(melee || ranged) || self.total() > 0.0)
    }

    pub fn weighted_check(self, mut check: impl FnMut(Skill) -> f32) -> f32 {
        let weights = self.weights();
        let total: f32 = weights.into_iter().sum();
        if total <= f32::EPSILON {
            return 0.0;
        }
        Self::SKILLS
            .into_iter()
            .zip(weights)
            .map(|(skill, weight)| check(skill) * weight)
            .sum::<f32>()
            / total
    }
}

/// Real-time attack schedule for one ordinary melee strike. Preparation ends
/// at contact; recovery ends when another non-continuation attack may begin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeleeAttackTiming {
    pub preparation_secs: f32,
    pub recovery_secs: f32,
}

/// Converts physical rotational inertia into the compact handling coefficient
/// shown to players. The characteristic length is measured from the grip to
/// the tip, so this is the radius of gyration as a fraction of usable length.
/// Lower values are easier to redirect.
pub fn weapon_balance_from_moment(
    moment_of_inertia_kg_m2: f32,
    mass_kg: f32,
    grip_to_tip_m: f32,
) -> f32 {
    if !moment_of_inertia_kg_m2.is_finite()
        || !mass_kg.is_finite()
        || !grip_to_tip_m.is_finite()
        || moment_of_inertia_kg_m2 < 0.0
        || mass_kg <= f32::EPSILON
        || grip_to_tip_m <= f32::EPSILON
    {
        return 1.0;
    }
    (moment_of_inertia_kg_m2 / mass_kg).sqrt() / grip_to_tip_m
}

/// Initial tactical cadence synthesized from physical rotational inertia and
/// attack style. Weapon family is intentionally absent: two weapons with the
/// same inertia take the same baseline time to redirect.
pub fn melee_attack_timing(
    style: MeleeAttackStyle,
    moment_of_inertia_kg_m2: f32,
    unarmed: bool,
) -> MeleeAttackTiming {
    let inertia = if moment_of_inertia_kg_m2.is_finite() {
        moment_of_inertia_kg_m2.max(0.0)
    } else {
        0.3
    };
    let base_cycle = if unarmed {
        0.36
    } else {
        // Saturation prevents long weapons from becoming unusably slow while
        // retaining a steep distinction among knives, swords, and polearms.
        0.24 + 0.40 * (inertia / (inertia + 0.45)).sqrt()
    };
    let (style_scale, contact_share) = if style == MeleeAttackStyle::Stab {
        (1.18, 0.56)
    } else {
        (1.0, 0.55)
    };
    let cycle = base_cycle * style_scale;
    MeleeAttackTiming {
        preparation_secs: cycle * contact_share,
        recovery_secs: cycle * (1.0 - contact_share),
    }
}

/// Canonical weapon-leaf distribution from the embedded authored catalog.
///
/// Unknown and non-weapon identifiers deliberately return an empty
/// distribution. There is no ID-shaped sword fallback.
pub fn weapon_skill_distribution_for_item(item_id: &str) -> WeaponSkillDistribution {
    crate::item_catalog::weapon_skills(item_id)
        .map_or_else(WeaponSkillDistribution::default, Into::into)
}

pub const LOWER_MUSCLE_MASS_PER_LEG_STRENGTH: f32 = 5.0;
pub const WEIGHT_CAPACITY_PER_LOWER_MUSCLE_MASS: f32 = 30.0;
const ARMOR_PENALTY_EXPONENT: i32 = 3;

/// The carried burden and injury-adjusted carrying capacity used by the
/// shared linear encumbrance rule.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EncumbranceSummary {
    pub burden_kg: f32,
    pub capacity_kg: f32,
}

impl EncumbranceSummary {
    pub fn new(burden_kg: f32, capacity_kg: f32) -> Self {
        Self {
            burden_kg: finite_nonnegative(burden_kg),
            capacity_kg: finite_nonnegative(capacity_kg),
        }
    }

    pub fn remaining_multiplier(self) -> f32 {
        encumbrance_remaining_multiplier(self.burden_kg, self.capacity_kg)
    }

    pub fn penalty_fraction(self) -> f32 {
        1.0 - self.remaining_multiplier()
    }

    pub fn combined(self, other: Self) -> Self {
        Self::new(
            self.burden_kg + other.burden_kg,
            self.capacity_kg + other.capacity_kg,
        )
    }
}

fn finite_nonnegative(value: f32) -> f32 {
    if value.is_nan() || value <= 0.0 {
        0.0
    } else if value.is_infinite() {
        f32::MAX
    } else {
        value
    }
}

pub fn encumbrance_capacity_kg(average_injury_adjusted_leg_strength: f32) -> f32 {
    finite_nonnegative(
        finite_nonnegative(average_injury_adjusted_leg_strength)
            * LOWER_MUSCLE_MASS_PER_LEG_STRENGTH
            * WEIGHT_CAPACITY_PER_LOWER_MUSCLE_MASS,
    )
}

/// Returns the multiplier left after encumbrance. A character with no usable
/// capacity is fully penalized, including when the reported burden is zero.
pub fn encumbrance_remaining_multiplier(burden_kg: f32, capacity_kg: f32) -> f32 {
    let burden_kg = finite_nonnegative(burden_kg);
    let capacity_kg = finite_nonnegative(capacity_kg);
    if capacity_kg <= f32::EPSILON {
        return 0.0;
    }
    (1.0 - burden_kg / capacity_kg).clamp(0.0, 1.0)
}

#[blanket::blanket(derive(Ref, Rc, Arc, Mut, Box, Cow))]
#[ambassador::delegatable_trait]
pub trait PlayerEquipment {
    fn weapon_skill_distribution(&self) -> WeaponSkillDistribution {
        WeaponSkillDistribution::default()
    }
    fn weapon_is_melee(&self) -> bool {
        false
    }
    fn weapon_is_ranged(&self) -> bool {
        false
    }
    /// Whether the selected attack uses the character's empty hands.
    ///
    /// This is intentionally explicit: zero weapon weight is not a reliable
    /// proxy because an item may be exceptionally light, while empty-hand
    /// impacts have their own injury and stagger calibration.
    fn weapon_is_unarmed(&self) -> bool {
        false
    }
    fn weapon_does_blunt(&self) -> bool {
        false
    }
    fn weapon_does_slash(&self) -> bool {
        false
    }
    fn weapon_does_pierce(&self) -> bool {
        false
    }
    /// Precision for ranged attacks. Melee attacks use the style-specific
    /// values below so a weapon can be easy to thrust accurately but hard to
    /// place precisely during a swing (or vice versa).
    fn weapon_accuracy(&self) -> f32;
    fn weapon_swing_precision(&self) -> f32 {
        self.weapon_accuracy()
    }
    fn weapon_stab_precision(&self) -> f32 {
        self.weapon_accuracy()
    }
    fn weapon_preferred_melee_style(&self) -> MeleeAttackStyle {
        MeleeAttackStyle::Swing
    }
    fn weapon_melee_precision(&self, style: MeleeAttackStyle) -> f32 {
        match style {
            MeleeAttackStyle::Swing => self.weapon_swing_precision(),
            MeleeAttackStyle::Stab => self.weapon_stab_precision(),
        }
    }
    fn weapon_weight(&self) -> f32;
    fn weapon_penetration(&self) -> f32;
    fn weapon_reach(&self) -> f32;
    /// Distance from the controlling grip to the distal striking tip, in metres.
    fn weapon_grip_to_tip(&self) -> f32 {
        self.weapon_reach()
    }
    /// Full authored end-to-end weapon length, in metres.
    fn weapon_total_length(&self) -> f32 {
        self.weapon_grip_to_tip()
    }
    /// Longitudinal extent of a distinct distal striking head, in metres.
    fn weapon_striking_head_length(&self) -> f32 {
        0.0
    }
    fn weapon_body_material(&self) -> Option<crate::item_catalog_schema::EquipmentMaterial> {
        None
    }
    fn weapon_striking_material(&self) -> Option<crate::item_catalog_schema::EquipmentMaterial> {
        None
    }
    fn weapon_holding_side(&self) -> Option<BodySide>;
    fn weapon_is_precise(&self) -> bool;
    fn weapon_balance(&self) -> f32;
    /// Rotational inertia around the controlling hand, in kg*m^2.
    fn weapon_moment_of_inertia(&self) -> f32 {
        0.0
    }
    /// Real-time seconds between committing to a swing and the hit actually
    /// landing - the authoritative source both the tactical client (to pace
    /// its own local windup before raycasting) and the tactical server (to
    /// validate that pacing, see `CLIENT_MELEE_WINDUP`/`CLIENT_RANGED_WINDUP`
    /// in `adventuresim-tactical-server::combat`) read, so the two can never
    /// independently drift out of sync the way two separately-hardcoded
    /// constants can. Defaulted for implementations - autoresolve/strategic
    /// combat has no real-time pacing concept at all - that don't drive
    /// real-time tactical combat.
    fn weapon_windup_secs(&self) -> f32 {
        0.3
    }
    fn weapon_windup_secs_for(&self, _style: MeleeAttackStyle) -> f32 {
        self.weapon_windup_secs()
    }
    fn weapon_recovery_secs(&self) -> f32 {
        0.3
    }
    fn weapon_recovery_secs_for(&self, _style: MeleeAttackStyle) -> f32 {
        self.weapon_recovery_secs()
    }
    fn weapon_ranged_windup_secs(&self) -> f32 {
        0.3
    }
    /// Kinetic energy delivered by a projectile. Forty joules is a useful
    /// short-bow baseline; implementations with richer item data can override
    /// it per weapon.
    fn weapon_ranged_force_joules(&self) -> f32 {
        40.0 * self.weapon_weight().max(0.5)
    }
    fn shield_block_bonus(&self) -> f32;
    fn shield_holding_side(&self) -> Option<BodySide> {
        None
    }

    fn armor_resistance(&self, part: BodyPart) -> f32;
    fn armor_padding(&self, part: BodyPart) -> f32;
    fn armor_flexibility(&self, part: BodyPart) -> f32;
    fn armor_range_of_motion(&self, part: BodyPart) -> f32;
    fn armor_coverage(&self, part: BodyPart) -> f32;

    /// Samples coverage at one already-selected anatomical destination and
    /// returns the actual engaged layer. Implementations with only aggregate
    /// strategic data return that authored aggregate as one surface.
    fn armor_surface(&self, part: BodyPart, sample: f32) -> Option<ArmorSurface> {
        (sample.clamp(0.0, 1.0 - f32::EPSILON) < self.armor_coverage(part).clamp(0.0, 1.0)).then(
            || ArmorSurface {
                inventory_item_id: None,
                material: None,
                resistance: self.armor_resistance(part),
                padding: self.armor_padding(part),
                flexibility: self.armor_flexibility(part),
            },
        )
    }

    fn inventory_weight(&self) -> f32;

    // TODO: probably should count in limbs agility/strength for this ?
    fn armor_penalty(&self, parts: BodyParts) -> f32 {
        if parts.is_empty() {
            return 1.0;
        }

        let average_range_of_motion = parts
            .iter()
            .fold(0.0, |acc, part| acc + self.armor_range_of_motion(part))
            / parts.len() as f32;

        let penalty = 1.0 - (1.0 - average_range_of_motion).powi(ARMOR_PENALTY_EXPONENT);
        penalty.clamp(0.0, 1.0)
    }
    fn encumbrance_penalty_by_parts(
        &self,
        attrs: &impl PlayerAttributes,
        body: &impl PlayerBody,
    ) -> f32 {
        let average_leg_strength = attrs.limb_attr_by_weight_by_parts(
            LimbAttribute::Strength,
            body,
            LimbWeights::both_legs(),
        );
        let weight_capacity = encumbrance_capacity_kg(average_leg_strength);
        encumbrance_remaining_multiplier(
            body.body_weight() + self.inventory_weight(),
            weight_capacity,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weapon_skill_distribution_uses_the_shared_boundary_shape() {
        let skills = WeaponSkillDistribution {
            sword: 0.75,
            throw: 0.25,
            ..Default::default()
        };
        let encoded = serde_json::to_value(skills).unwrap();
        assert_eq!(encoded["sword"], serde_json::json!(0.75));
        assert_eq!(encoded["throw"], serde_json::json!(0.25));
        assert!(encoded.get("throw_skill").is_none());
        assert_eq!(
            serde_json::from_value::<WeaponSkillDistribution>(encoded).unwrap(),
            skills
        );
    }

    #[test]
    fn hybrid_weapon_distribution_uses_normalized_leaf_checks() {
        let halberd = WeaponSkillDistribution {
            polearm: 1.0,
            axe: 1.0,
            bludgeon: 1.0,
            ..Default::default()
        };
        let check = halberd.weighted_check(|skill| match skill {
            Skill::Polearm => 2.0,
            Skill::Axe => 3.0,
            Skill::Bludgeon => 4.0,
            _ => 100.0,
        });
        assert_eq!(check, 3.0);
        assert!(halberd.validate(true, false));
    }

    #[test]
    fn melee_timing_is_inferred_from_moment_and_style() {
        let cases = [
            ("fists", MeleeAttackStyle::Swing, 0.0, true, 0.36),
            (
                "knife cut",
                MeleeAttackStyle::Swing,
                0.009216,
                false,
                0.24 + 0.40 * f32::sqrt(0.009216 / (0.009216 + 0.45)),
            ),
            (
                "knife thrust",
                MeleeAttackStyle::Stab,
                0.009216,
                false,
                (0.24 + 0.40 * f32::sqrt(0.009216 / (0.009216 + 0.45))) * 1.18,
            ),
            (
                "sword",
                MeleeAttackStyle::Swing,
                0.26325,
                false,
                0.24 + 0.40 * f32::sqrt(0.26325 / (0.26325 + 0.45)),
            ),
            (
                "mace",
                MeleeAttackStyle::Swing,
                0.170555,
                false,
                0.24 + 0.40 * f32::sqrt(0.170555 / (0.170555 + 0.45)),
            ),
        ];
        for (label, style, inertia, unarmed, expected_cycle) in cases {
            let timing = melee_attack_timing(style, inertia, unarmed);
            let cycle = timing.preparation_secs + timing.recovery_secs;
            assert!((cycle - expected_cycle).abs() < 1.0e-5, "{label}: {cycle}");
        }
    }

    #[test]
    fn greater_moment_of_inertia_slows_cadence() {
        let low_inertia = melee_attack_timing(MeleeAttackStyle::Swing, 0.05, false);
        let high_inertia = melee_attack_timing(MeleeAttackStyle::Swing, 1.0, false);
        assert!(
            high_inertia.preparation_secs + high_inertia.recovery_secs
                > low_inertia.preparation_secs + low_inertia.recovery_secs
        );
    }

    #[test]
    fn instance_geometry_preserves_attack_allowance_and_tracks_recipe_length() {
        let short =
            ParametricWeaponCombatGeometry::new(2.0, 2.0, 1.8, 0.25, 2.5, 0.46, 2.0, 1.8).unwrap();
        let long =
            ParametricWeaponCombatGeometry::new(2.2, 2.3, 2.1, 0.25, 3.4, 0.44, 2.0, 1.8).unwrap();

        assert!((short.melee_reach_m() - 2.0).abs() < 1.0e-6);
        assert!((long.melee_reach_m() - 2.3).abs() < 1.0e-6);
        assert!((long.melee_reach_m() - short.melee_reach_m() - 0.3).abs() < 1.0e-6);
        assert!(long.moment_of_inertia_kg_m2 > short.moment_of_inertia_kg_m2);

        let preferred = |geometry: ParametricWeaponCombatGeometry| {
            crate::combat::preferred_melee_striking_measure(
                geometry.melee_reach_m(),
                geometry.grip_to_tip_m,
                geometry.striking_head_length_m,
                true,
                0.7,
            )
        };
        assert!(preferred(long) > preferred(short));

        let contact = |geometry: ParametricWeaponCombatGeometry| {
            crate::combat::resolve_melee_contact_at_time(crate::combat::MeleeContactAtTimeFacts {
                scheduled_measure_metres: 2.1,
                actual_measure_metres: 2.1,
                effective_reach_metres: geometry.melee_reach_m(),
                grip_to_tip_metres: geometry.grip_to_tip_m,
                total_length_metres: geometry.total_length_m,
                striking_head_length_metres: geometry.striking_head_length_m,
                distal_headed: true,
                attack_style: MeleeAttackStyle::Swing,
                body_material: Some(crate::item_catalog_schema::EquipmentMaterial::Hardwood),
                striking_material: Some(crate::item_catalog_schema::EquipmentMaterial::RoughSteel),
            })
        };
        assert_eq!(
            contact(short).classification,
            crate::combat::MeleeContactClassification::InvalidatedMiss
        );
        assert_eq!(
            contact(long).classification,
            crate::combat::MeleeContactClassification::IntendedSurface
        );
    }

    #[test]
    fn displayed_balance_round_trips_authored_moment() {
        let mass: f32 = 1.3;
        let length: f32 = 0.9;
        let old_balance: f32 = 0.5;
        let moment = mass * (old_balance * length).powi(2);
        assert!((weapon_balance_from_moment(moment, mass, length) - old_balance).abs() < 1.0e-6);
    }

    #[test]
    fn weapon_distribution_rejects_missing_family_and_invalid_weights() {
        let bow = WeaponSkillDistribution {
            bow: 1.0,
            ..Default::default()
        };
        assert!(bow.validate(false, true));
        assert!(!bow.validate(true, false));
        assert!(
            !WeaponSkillDistribution {
                sword: f32::NAN,
                ..Default::default()
            }
            .validate(true, false)
        );
    }

    #[test]
    fn seeded_item_weapon_distributions_are_normalized_and_canonical() {
        assert_eq!(
            weapon_skill_distribution_for_item("longbow"),
            WeaponSkillDistribution {
                bow: 1.0,
                ..Default::default()
            }
        );
        assert_eq!(
            weapon_skill_distribution_for_item("hand_axe"),
            WeaponSkillDistribution {
                axe: 0.5,
                knife: 0.5,
                ..Default::default()
            }
        );
        let halberd = weapon_skill_distribution_for_item("halberd");
        assert!((halberd.polearm - 1.0 / 3.0).abs() < f32::EPSILON);
        assert_eq!(halberd.polearm, halberd.axe);
        assert_eq!(halberd.axe, halberd.bludgeon);
        assert!((halberd.total() - 1.0).abs() < f32::EPSILON);
        assert_eq!(
            weapon_skill_distribution_for_item("unknown_seeded_weapon"),
            WeaponSkillDistribution::default()
        );
    }

    #[test]
    fn linear_encumbrance_covers_key_points_and_overload() {
        assert_eq!(encumbrance_remaining_multiplier(0.0, 100.0), 1.0);
        assert_eq!(encumbrance_remaining_multiplier(50.0, 100.0), 0.5);
        assert_eq!(encumbrance_remaining_multiplier(100.0, 100.0), 0.0);
        assert_eq!(encumbrance_remaining_multiplier(125.0, 100.0), 0.0);
        assert_eq!(encumbrance_remaining_multiplier(0.0, 0.0), 0.0);
    }

    #[test]
    fn injury_adjusted_strength_maps_to_capacity() {
        assert_eq!(encumbrance_capacity_kg(0.0), 0.0);
        assert_eq!(encumbrance_capacity_kg(0.75), 112.5);
        assert_eq!(encumbrance_capacity_kg(3.0), 450.0);
    }

    #[test]
    fn summaries_combine_burdens_and_capacities_before_penalty() {
        let party =
            EncumbranceSummary::new(60.0, 100.0).combined(EncumbranceSummary::new(30.0, 200.0));
        assert_eq!(party, EncumbranceSummary::new(90.0, 300.0));
        assert!((party.penalty_fraction() - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn positive_infinite_burden_is_saturated_and_fully_penalizing() {
        let summary = EncumbranceSummary::new(f32::INFINITY, 100.0);
        assert_eq!(summary.burden_kg, f32::MAX);
        assert_eq!(summary.penalty_fraction(), 1.0);
    }

    #[test]
    fn positive_infinite_capacity_is_saturated_without_becoming_zero() {
        let summary = EncumbranceSummary::new(100.0, f32::INFINITY);
        assert_eq!(summary.capacity_kg, f32::MAX);
        assert_eq!(summary.remaining_multiplier(), 1.0);
    }

    #[test]
    fn finite_sum_overflow_cannot_erase_an_overload() {
        let combined = EncumbranceSummary::new(f32::MAX, 100.0)
            .combined(EncumbranceSummary::new(f32::MAX, 100.0));
        assert_eq!(combined.burden_kg, f32::MAX);
        assert_eq!(combined.capacity_kg, 200.0);
        assert_eq!(combined.penalty_fraction(), 1.0);
    }

    #[test]
    fn layered_armor_combines_stats_without_expanding_body_parts() {
        use crate::item_catalog::EquipmentChannel as C;
        let pieces = [
            WearableProtection {
                inventory_item_id: 1,
                body_part: BodyPart::Chest,
                channel: C::Padding,
                order: 0,
                coverage: 0.5,
                resistance: 20.0,
                padding: 30.0,
                flexibility: 0.8,
                range_of_motion: 0.9,
            },
            WearableProtection {
                inventory_item_id: 2,
                body_part: BodyPart::Chest,
                channel: C::RigidArmor,
                order: 0,
                coverage: 0.8,
                resistance: 100.0,
                padding: 10.0,
                flexibility: 0.2,
                range_of_motion: 0.7,
            },
        ];
        let armor = aggregate_layered_armor(BodyPart::Chest, pieces);
        assert!((armor.coverage - 0.9).abs() < 0.0001);
        assert_eq!(armor.resistance, 120.0);
        assert_eq!(armor.padding, 40.0);
        assert!((armor.flexibility - 0.3).abs() < 0.0001);
        assert_eq!(armor.range_of_motion, 0.7);
        assert_eq!(
            outermost_wearable(BodyPart::Chest, pieces)
                .expect("outer layer")
                .inventory_item_id,
            2
        );
    }

    #[test]
    fn zero_resistance_layers_have_defined_flexibility() {
        use crate::item_catalog::EquipmentChannel as C;
        let armor = aggregate_layered_armor(
            BodyPart::LeftArm,
            [WearableProtection {
                inventory_item_id: 1,
                body_part: BodyPart::LeftArm,
                channel: C::BaseClothing,
                order: 0,
                coverage: 1.0,
                resistance: 0.0,
                padding: 2.0,
                flexibility: 1.0,
                range_of_motion: 1.0,
            }],
        );
        assert_eq!(armor.flexibility, 0.0);
    }

    #[test]
    fn input_addresses_are_explicit_many_to_many_and_outside_in() {
        let head = INPUT_ADDRESS_MAPPINGS
            .iter()
            .find(|mapping| mapping.input == "t")
            .expect("head input");
        assert_eq!(
            head.locations,
            &[
                EquipmentLocation::Head,
                EquipmentLocation::Face,
                EquipmentLocation::Neck,
            ]
        );
        let left_arm = INPUT_ADDRESS_MAPPINGS
            .iter()
            .find(|mapping| mapping.input == "`")
            .expect("left arm input");
        assert_eq!(
            left_arm.locations,
            &[EquipmentLocation::LeftArm, EquipmentLocation::LeftHand]
        );
        assert_eq!(
            &head.channel_order[..4],
            &[
                EquipmentChannel::Mount,
                EquipmentChannel::Accessory,
                EquipmentChannel::Outerwear,
                EquipmentChannel::RigidArmor,
            ]
        );
        for (input, location) in [
            ("g", EquipmentLocation::Chest),
            ("y", EquipmentLocation::Stomach),
            ("h", EquipmentLocation::Back),
            ("v", EquipmentLocation::LeftLeg),
            ("b", EquipmentLocation::RightLeg),
        ] {
            assert!(
                INPUT_ADDRESS_MAPPINGS
                    .iter()
                    .find(|mapping| mapping.input == input)
                    .is_some_and(|mapping| mapping.locations.contains(&location)),
                "{location:?} must have a QWERTY equipment input"
            );
        }
        assert!(
            ["w", "a", "s", "d"].iter().all(|input| {
                INPUT_ADDRESS_MAPPINGS
                    .iter()
                    .all(|mapping| mapping.input != *input)
            }),
            "tactical movement inputs must remain free"
        );
        let keyboard_cells = INPUT_ADDRESS_MAPPINGS
            .iter()
            .map(|mapping| (mapping.keyboard_row, mapping.keyboard_column))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            keyboard_cells.len(),
            INPUT_ADDRESS_MAPPINGS.len(),
            "slot inputs must occupy distinct cells in the QWERTY map"
        );
    }

    #[test]
    fn graph_supports_belt_sheath_sword_and_body_bag_contents() {
        let edge = |parent, point: &str, capacity_index| EquipmentGraphEdge {
            parent_inventory_item_id: parent,
            attachment_point_id: point.into(),
            capacity_index,
        };
        let mut graph = EquipmentGraph::default();
        graph
            .equip(
                1,
                EquipmentGraphPlacement {
                    body: vec![(EquipmentLocation::LeftBelt, EquipmentChannel::Accessory, 0)],
                    parents: vec![],
                },
            )
            .unwrap();
        graph
            .equip(
                2,
                EquipmentGraphPlacement {
                    body: vec![],
                    parents: vec![edge(1, "left", 0), edge(1, "right", 0)],
                },
            )
            .unwrap();
        graph
            .equip(
                3,
                EquipmentGraphPlacement {
                    body: vec![],
                    parents: vec![edge(2, "blade", 0)],
                },
            )
            .unwrap();
        graph
            .equip(
                4,
                EquipmentGraphPlacement {
                    body: vec![(
                        EquipmentLocation::LeftShoulder,
                        EquipmentChannel::Accessory,
                        0,
                    )],
                    parents: vec![],
                },
            )
            .unwrap();
        graph
            .equip(
                5,
                EquipmentGraphPlacement {
                    body: vec![],
                    parents: vec![edge(4, "contents", 0)],
                },
            )
            .unwrap();
        assert_eq!(graph.nodes.len(), 5);
        assert!(graph.unequip(1).is_err());
        graph.unequip(3).unwrap();
        graph.unequip(2).unwrap();
        graph.unequip(1).unwrap();
    }

    #[test]
    fn graph_multi_point_move_is_atomic_and_cycle_safe() {
        let edge = |parent, point: &str, capacity_index| EquipmentGraphEdge {
            parent_inventory_item_id: parent,
            attachment_point_id: point.into(),
            capacity_index,
        };
        let mut graph = EquipmentGraph::default();
        for id in [10, 11] {
            graph
                .equip(
                    id,
                    EquipmentGraphPlacement {
                        body: vec![(
                            if id == 10 {
                                EquipmentLocation::LeftShoulder
                            } else {
                                EquipmentLocation::RightShoulder
                            },
                            EquipmentChannel::Mount,
                            0,
                        )],
                        parents: vec![],
                    },
                )
                .unwrap();
        }
        graph
            .equip(
                20,
                EquipmentGraphPlacement {
                    body: vec![],
                    parents: vec![edge(10, "strap", 0), edge(11, "strap", 0)],
                },
            )
            .unwrap();
        let before = graph.clone();
        assert_eq!(
            graph.equip(
                21,
                EquipmentGraphPlacement {
                    body: vec![],
                    parents: vec![edge(10, "strap", 0), edge(11, "strap", 1)],
                },
            ),
            Err("attachment capacity conflict")
        );
        assert_eq!(graph, before, "failed preflight never partially mutates");
        assert_eq!(
            graph.equip(
                10,
                EquipmentGraphPlacement {
                    body: vec![],
                    parents: vec![edge(20, "loop", 0)],
                },
            ),
            Err("item has equipped children")
        );
    }

    #[test]
    fn singleton_wearable_orders_conflict_but_accessory_coexists() {
        let mut graph = EquipmentGraph::default();
        graph
            .equip(
                1,
                EquipmentGraphPlacement {
                    body: vec![(EquipmentLocation::Chest, EquipmentChannel::RigidArmor, 0)],
                    parents: vec![],
                },
            )
            .unwrap();
        assert_eq!(
            graph.equip(
                2,
                EquipmentGraphPlacement {
                    body: vec![(EquipmentLocation::Chest, EquipmentChannel::RigidArmor, 1,)],
                    parents: vec![],
                },
            ),
            Err("body occupancy conflict")
        );
        graph
            .equip(
                3,
                EquipmentGraphPlacement {
                    body: vec![(EquipmentLocation::Chest, EquipmentChannel::Accessory, 0)],
                    parents: vec![],
                },
            )
            .unwrap();
    }
}
