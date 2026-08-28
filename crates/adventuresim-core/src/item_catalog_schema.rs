//! Exact typed authoring schema shared by the build compiler and runtime.

use crate::combat_style::MeleeAttackStyle;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemCatalogDocument {
    pub schema_version: u32,
    pub items: Vec<ItemDefinition>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ItemDefinition {
    pub id: String,
    pub display_name: String,
    pub weight_kg: f32,
    /// Exterior displacement used when this object is placed in a container.
    pub exterior_volume_ml: u32,
    pub base_value: u32,
    #[serde(default)]
    pub tags: Vec<String>,
    pub presentation: Presentation,
    /// Content-authored equipment topology. This is independent of item kind:
    /// armor, weapons, containers, and simple accessories may all participate.
    #[serde(default)]
    pub equipment: Option<EquipmentDefinition>,
    #[serde(flatten)]
    pub kind: ItemKind,
    #[serde(default)]
    pub capabilities: Capabilities,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentDefinition {
    /// Authored tactical placeholder geometry. Exterior container volume is
    /// deliberately not used to guess a shape.
    pub physical: EquipmentPhysical,
    /// Semantic surface material used by procedural placeholder rendering.
    /// Detailed authored assets may later override this with textures.
    #[serde(default)]
    pub material: Option<EquipmentMaterial>,
    /// Tags used by parent attachment points to accept this item. An empty
    /// list is never inferred from ItemKind.
    #[serde(default)]
    pub attachment_tags: Vec<String>,
    pub placements: Vec<EquipmentPlacement>,
    /// Shared projection stats for non-armor equipment such as protective
    /// clothing. Armor may continue to source these physical values from its
    /// kind payload; placement protection targets are always explicit.
    #[serde(default)]
    pub protection: Option<EquipmentProtection>,
    #[serde(default)]
    pub attachment_points: Vec<AttachmentPointDefinition>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentPhysical {
    /// Full box dimensions in metres: local X width, local Y length, local Z thickness.
    pub dimensions_m: [f32; 3],
    /// Distance from a weapon socket/grip to the gameplay tip along local +Y.
    /// Non-weapons author zero.
    #[serde(default)]
    pub grip_to_tip_m: f32,
    /// Local attachment anchor relative to the ordinary box-centre origin.
    /// Weapons attach this point to a hand socket; worn equipment attaches it
    /// to the bone selected by its primary occupancy location.
    #[serde(default)]
    pub anchor_offset_m: [f32; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentPlacement {
    /// Stable within the item definition; persisted instead of an array index.
    pub id: String,
    /// A placement may combine physical character anchors with any number of
    /// required item attachment points. The selected targets are runtime
    /// graph edges and are not inferred from item kind.
    #[serde(default)]
    pub occupancy: Vec<OccupancyRequirement>,
    #[serde(default)]
    pub parents: Vec<ParentRequirement>,
    /// Explicit many-to-many combat projection. Locations never imply
    /// protection.
    #[serde(default)]
    pub protection: Vec<EquipmentBodyPart>,
    /// Fundamental visible surface coverage used to generate fitted MHR
    /// geometry. Each span is measured along its ordered anatomical regions.
    #[serde(default)]
    pub surface: Vec<EquipmentSurfaceSpan>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentMaterial {
    PolishedSteel,
    RoughSteel,
    OxidizedSteel,
    MailSteel,
    VegetableTannedLeather,
    Linen,
    Wool,
    QuiltedTextile,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentSurfaceSpan {
    /// Ordered proximal-to-distal anatomical chain. More than one region makes
    /// a continuous span across multiple bones.
    pub regions: Vec<EquipmentAnatomicalRegion>,
    /// Which end remains fixed while the other end is clipped to `coverage`.
    pub anchor: SurfaceAnchor,
    /// Fraction of the combined region-chain length retained.
    pub coverage: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceAnchor {
    Proximal,
    Distal,
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentAnatomicalRegion {
    Head,
    Neck,
    Chest,
    Stomach,
    LeftUpperArm,
    LeftForearm,
    RightUpperArm,
    RightForearm,
    LeftThigh,
    LeftLowerLeg,
    RightThigh,
    RightLowerLeg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(
    all(feature = "spacetimedb", runtime_catalog),
    derive(spacetimedb::SpacetimeType)
)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentChannel {
    Held,
    BaseClothing,
    Padding,
    FlexibleArmor,
    RigidArmor,
    Outerwear,
    Accessory,
    Mount,
    Containment,
}

impl EquipmentChannel {
    pub const fn order(self) -> u8 {
        match self {
            Self::Held => 0,
            Self::BaseClothing => 10,
            Self::Padding => 20,
            Self::FlexibleArmor => 30,
            Self::RigidArmor => 40,
            Self::Outerwear => 50,
            Self::Accessory => 60,
            Self::Mount => 70,
            Self::Containment => 80,
        }
    }

    pub const fn singleton_per_location(self) -> bool {
        matches!(
            self,
            Self::Held
                | Self::BaseClothing
                | Self::Padding
                | Self::FlexibleArmor
                | Self::RigidArmor
                | Self::Outerwear
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OccupancyRequirement {
    pub location: EquipmentLocation,
    pub channel: EquipmentChannel,
    /// Ordering within a channel supports repeated deeper selection without
    /// making occupancy compatibility depend on enum declaration order.
    #[serde(default)]
    pub order: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParentRequirement {
    pub channel: EquipmentChannel,
    #[serde(default)]
    pub order: u16,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentPointDefinition {
    pub id: String,
    pub channel: EquipmentChannel,
    pub capacity: u16,
    #[serde(default)]
    pub order: u16,
    /// Body locations whose slot inputs may traverse this point. Empty means
    /// the point inherits every location through which its parent is reached.
    #[serde(default)]
    pub locations: Vec<EquipmentLocation>,
    /// Empty accepts any equipment-authored child; otherwise at least one
    /// child's attachment tag must match.
    #[serde(default)]
    pub accepts_tags: Vec<String>,
    /// Optional canonical anatomical-surface coordinate used to resolve this
    /// point independently on every generated mesh LOD.
    #[serde(default)]
    pub surface_uv: Option<SurfaceUvCoordinate>,
    /// Optional anatomical tangent for presentation sockets generated on the
    /// parent item's fitted surface. Local +Y (grip toward weapon tip) on an
    /// attached item follows this direction; gameplay topology does not depend
    /// on it. Surface coordinates and tangents are authored together.
    #[serde(default)]
    pub tangent_direction: Option<[f32; 3]>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceUvCoordinate {
    /// Versioned canonical surface parameterization shared by compatible LODs.
    pub domain: String,
    /// Coordinate in that domain's non-overlapping anatomical UV atlas.
    pub uv: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquipmentProtection {
    #[serde(default)]
    pub coverage: f32,
    #[serde(default)]
    pub padding: f32,
    #[serde(default)]
    pub resistance: f32,
    #[serde(default = "default_equipment_flexibility")]
    pub flexibility: f32,
    #[serde(default = "default_equipment_range_of_motion")]
    pub range_of_motion: f32,
}

fn default_equipment_flexibility() -> f32 {
    1.0
}

fn default_equipment_range_of_motion() -> f32 {
    1.0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(
    all(feature = "spacetimedb", runtime_catalog),
    derive(spacetimedb::SpacetimeType)
)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentBodyPart {
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
    Chest,
    Stomach,
    Head,
}

/// Fine-grained equipment topology. This is intentionally separate from the
/// seven-part combat/health `BodyPart` ABI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(
    all(feature = "spacetimedb", runtime_catalog),
    derive(spacetimedb::SpacetimeType)
)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentLocation {
    Head,
    Face,
    Neck,
    Chest,
    Stomach,
    Back,
    LeftShoulder,
    RightShoulder,
    LeftArm,
    RightArm,
    LeftHand,
    RightHand,
    LeftLeg,
    RightLeg,
    LeftFoot,
    RightFoot,
    LeftBelt,
    RightBelt,
    FrontBelt,
    BackBelt,
    LeftPocket,
    RightPocket,
    BackLeftPocket,
    BackRightPocket,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Presentation {
    pub icon: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ItemKind {
    Simple,
    Currency,
    Ingredient,
    Medication,
    Clothing,
    Container {
        slot: Slot,
    },
    Shield {
        slot: Slot,
        block: f32,
    },
    Armor {
        slot: Slot,
        /// Derived from `equipment.placements[].surface` when the catalog is
        /// loaded; it is not independently authored.
        #[serde(default)]
        coverage: f32,
        resistance: f32,
        padding: f32,
        flexibility: f32,
        range_of_motion: f32,
    },
    Weapon {
        slot: Slot,
        /// Whether the weapon fits an authored sheath/holster or must remain
        /// in a hand (or be dropped) when not otherwise carried.
        carry: WeaponCarry,
        #[serde(default)]
        preferred_attack: MeleeAttackStyle,
        #[serde(default)]
        swing_precision: f32,
        #[serde(default)]
        stab_precision: f32,
        accuracy: f32,
        reach_m: f32,
        penetration: f32,
        /// Rotational inertia around the controlling hand, in kg*m^2.
        moment_of_inertia_kg_m2: f32,
        precise: bool,
        melee: bool,
        ranged: bool,
        damage_types: Vec<DamageType>,
        skills: WeaponSkills,
    },
    Food,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaponCarry {
    Sheathable,
    HandOnly,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    all(feature = "spacetimedb", runtime_catalog),
    derive(spacetimedb::SpacetimeType)
)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    #[default]
    None,
    LeftHolding,
    RightHolding,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
    Chest,
    Stomach,
    Head,
    AnyHolding,
    AnyArm,
    AnyLeg,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DamageType {
    Blunt,
    Slash,
    Pierce,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponSkills {
    #[serde(default)]
    pub polearm: f32,
    #[serde(default)]
    pub axe: f32,
    #[serde(default)]
    pub bludgeon: f32,
    #[serde(default)]
    pub sword: f32,
    #[serde(default)]
    pub knife: f32,
    #[serde(default)]
    pub bow: f32,
    #[serde(default)]
    pub crossbow: f32,
    #[serde(default)]
    pub firearm: f32,
    #[serde(default)]
    pub throw: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capabilities {
    pub durability: Option<Durability>,
    pub food: Option<Food>,
    pub alcohol: Option<Alcohol>,
    pub container: Option<Container>,
    pub book: Option<Book>,
}

/// Authored teaching metadata. Books remain ordinary `simple` items; this
/// capability is resolved from the embedded catalog and is never flattened
/// into a persisted inventory row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Book {
    pub medium: adventuresim_world_schema::WrittenLanguage,
    pub target: BookTarget,
    /// Shared 1..=5 item quality. A quality-N book teaches rank N-1 to N.
    pub quality: u8,
    #[serde(default)]
    pub settlement_allowlist: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum BookTarget {
    Written {
        language: adventuresim_world_schema::WrittenLanguage,
    },
    Religion {
        religion: adventuresim_world_schema::OfficialReligion,
    },
    Bestiary {
        category: adventuresim_world_schema::BestiaryCategory,
    },
    Terrain {
        terrain: String,
    },
    Skill {
        skill: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Durability {
    pub quality: u8,
    pub yield_j: f32,
    pub fracture_j: f32,
    pub wear: f32,
    pub failure_share: f32,
    pub edge_sensitivity: f32,
    pub handling_sensitivity: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Food {
    pub class: String,
    pub nutrition_kcal: f32,
    pub value_per_unit: f32,
    pub growth_per_hour: f32,
    pub cooking_minutes: u32,
    pub flavors_kg: Flavors,
    pub culinary_fat: bool,
    pub quality: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Flavors {
    pub salty: f32,
    pub spicy: f32,
    pub sweet: f32,
    pub sour: f32,
    pub savory: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Alcohol {
    pub serving_ml: u32,
    pub abv_basis_points: u16,
    pub net_hydration_ml: u32,
    pub disinfectant_effectiveness: u16,
    pub disinfectant_focused: bool,
    pub potable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Container {
    pub capacity_ml: u32,
}
