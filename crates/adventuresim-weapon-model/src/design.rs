use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct Millimeters(pub u32);

impl Millimeters {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
    pub fn meters(self) -> f32 {
        self.0 as f32 / 1_000.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct SignedMillimeters(pub i32);

impl SignedMillimeters {
    pub fn meters(self) -> f32 {
        self.0 as f32 / 1_000.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Permille(pub u16);

impl Permille {
    pub fn unit(self) -> f32 {
        self.0 as f32 / 1_000.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct SignedPermille(pub i16);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Segments(pub u16);

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct MilliRadians(pub i32);

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct OffsetMm {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl OffsetMm {
    pub fn meters(self) -> [f32; 3] {
        [
            self.x as f32 / 1_000.0,
            self.y as f32 / 1_000.0,
            self.z as f32 / 1_000.0,
        ]
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum MaterialClass {
    Wood,
    Leather,
    DarkLeather,
    Brass,
    Steel,
    DarkSteel,
}

/// Render-only carry fixture derived from the complete weapon recipe.
///
/// Blade weapons receive a fitted, full-length sheath or scabbard. Compact
/// hafted weapons receive a leather frog/loop around the grip. Long polearms
/// deliberately have no body-mounted holder.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum WeaponHolderKind {
    BladeSheath,
    HaftLoop,
}

/// A durable, smithable holder recipe. The fitted weapon recipe is captured
/// at fitting time so the holder remains independently reproducible even when
/// it is empty or the weapon later changes custody.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct WeaponHolderDesign {
    pub catalog_id: String,
    pub kind: WeaponHolderKind,
    pub fitted_weapon: WeaponDesign,
    pub body_material: MaterialClass,
    pub fitting_material: MaterialClass,
    pub clearance: Millimeters,
    pub throat_length: Millimeters,
    pub chape_length: Millimeters,
    pub loop_position: Permille,
    pub loop_bar_radius: Millimeters,
    pub hanger_width: Millimeters,
    pub hanger_height: Millimeters,
}

impl MaterialClass {
    pub const fn density_kg_m3(self) -> f32 {
        match self {
            Self::Wood => 720.0,
            Self::Leather | Self::DarkLeather => 920.0,
            Self::Brass => 8_500.0,
            Self::Steel | Self::DarkSteel => 7_850.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ComponentRole {
    Structure,
    Grip,
    Guard,
    Socket,
    Head,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum Attachment {
    Root,
    TopOf {
        component: String,
        insertion: Millimeters,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum BladeProfile {
    Straight,
    Spear,
    Cleaver,
    Curved,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CylinderSpec {
    pub length: Millimeters,
    pub radius: Millimeters,
    pub bottom_scale: Permille,
    pub top_scale: Permille,
    pub segments: Segments,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct OvalGripSpec {
    pub length: Millimeters,
    pub width: Millimeters,
    pub thickness: Millimeters,
    pub bottom_scale: Permille,
    pub top_scale: Permille,
    pub segments: Segments,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct BladeSpec {
    pub length: Millimeters,
    pub width: Millimeters,
    pub thickness: Millimeters,
    pub curvature: SignedMillimeters,
    pub profile: BladeProfile,
    pub section: BladeSection,
    pub samples: Segments,
    pub taper: Permille,
    pub single_edge: Permille,
    pub belly: SignedPermille,
    pub ricasso: Millimeters,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct GuardSpec {
    pub span: Millimeters,
    pub radius: Millimeters,
    pub sweep: SignedMillimeters,
    pub samples: Segments,
    pub radial_segments: Segments,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct MaceSpec {
    pub length: Millimeters,
    pub core_radius: Millimeters,
    pub cusp_radius: Millimeters,
    pub flanges: u8,
    pub flange_thickness: Millimeters,
    pub segments: Segments,
    pub cusp_height: Permille,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum BladeSection {
    Flat,
    Diamond,
    Fullered,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct SocketSpec {
    pub length: Millimeters,
    pub outer_radius: Millimeters,
    pub top_radius: Millimeters,
    pub wall: Millimeters,
    pub segments: Segments,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct LangetSpec {
    pub length: Millimeters,
    pub width: Millimeters,
    pub thickness: Millimeters,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct AxeSpec {
    pub reach: Millimeters,
    pub height: Millimeters,
    pub thickness: Millimeters,
    pub root_width: Millimeters,
    pub beard: Permille,
    pub curvature: Permille,
    pub side: i8,
    pub upper_shoulder: Permille,
    pub lower_shoulder: Permille,
    pub flare: SignedPermille,
    pub toe: SignedPermille,
    pub heel: SignedPermille,
    pub beard_drop: Permille,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct HammerPollSpec {
    pub length: Millimeters,
    pub face: Millimeters,
    pub neck: Millimeters,
    pub thickness: Millimeters,
    pub direction: i8,
    pub crown: Permille,
    pub neck_ratio: Permille,
    pub face_flare: Permille,
    pub crown_length: Millimeters,
    pub face_thickness: Millimeters,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CurvedBeakSpec {
    pub length: Millimeters,
    pub root_section: Millimeters,
    pub tip_section: Millimeters,
    pub thickness: Millimeters,
    pub curvature: SignedMillimeters,
    pub direction: i8,
    pub samples: Segments,
    pub bend_position: Permille,
    pub droop: SignedMillimeters,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct FacetedBeakSpec {
    pub length: Millimeters,
    pub root: Millimeters,
    pub tip: Millimeters,
    pub thickness: Millimeters,
    pub set: SignedMillimeters,
    pub direction: i8,
    pub bend_position: Permille,
    pub tip_thickness: Millimeters,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct GlaiveSpec {
    pub length: Millimeters,
    pub width: Millimeters,
    pub thickness: Millimeters,
    pub curvature: SignedMillimeters,
    pub root: Millimeters,
    pub edge_curvature: Permille,
    pub spine_curvature: Permille,
    pub point_length: Permille,
    pub samples: Segments,
    pub belly_position: Permille,
    pub root_length: Millimeters,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct BillSpec {
    pub length: Millimeters,
    pub width: Millimeters,
    pub hook: Millimeters,
    pub thickness: Millimeters,
    pub root: Millimeters,
    pub hook_depth: Permille,
    pub hook_curvature: Permille,
    pub samples: Segments,
    pub belly_position: Permille,
    pub point_length: Permille,
    pub root_length: Millimeters,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ForkSpec {
    pub length: Millimeters,
    pub width: Millimeters,
    pub base_width: Millimeters,
    pub thickness: Millimeters,
    pub tine_width: Millimeters,
    pub crotch: Permille,
    pub taper: Permille,
    pub shoulder_blend: Permille,
    pub crotch_round: Permille,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct PartisanSpec {
    pub length: Millimeters,
    pub width: Millimeters,
    pub lug_width: Millimeters,
    pub thickness: Millimeters,
    pub belly: Permille,
    pub root_width: Millimeters,
    pub lug_drop: Permille,
    pub belly_position: Permille,
    pub lug_sweep: Permille,
    pub acuteness: Permille,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct SlabGripSpec {
    pub length: Millimeters,
    pub width: Millimeters,
    pub thickness: Millimeters,
    pub scale_thickness: Millimeters,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct KnuckleBowSpec {
    pub width: Millimeters,
    pub length: Millimeters,
    pub bar: Millimeters,
    pub side: i8,
    pub bulge: Permille,
    pub samples: Segments,
    pub radial_segments: Segments,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct CollarSpec {
    pub width: Millimeters,
    pub radius: Millimeters,
    pub segments: Segments,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct SleeveSpec {
    pub length: Millimeters,
    pub radius: Millimeters,
    pub top_radius: Millimeters,
    pub wall: Millimeters,
    pub segments: Segments,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct BossSpec {
    pub radius: Millimeters,
    pub thickness: Millimeters,
    pub segments: Segments,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct SpearSpec {
    pub length: Millimeters,
    pub width: Millimeters,
    pub thickness: Millimeters,
    pub root_width: Millimeters,
    pub belly_position: Permille,
    pub acuteness: Permille,
    pub samples: Segments,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ProfilePointMm {
    pub y: Millimeters,
    pub radius: Millimeters,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ProfiledPommelSpec {
    pub profile: Vec<ProfilePointMm>,
    pub segments: Segments,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct TubePathSpec {
    pub points: Vec<OffsetMm>,
    pub radius: Millimeters,
    pub radial_segments: Segments,
    pub closed: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct RingGuardSpec {
    pub radius: Millimeters,
    pub bar: Millimeters,
    pub arc_start: MilliRadians,
    pub arc_end: MilliRadians,
    pub samples: Segments,
    pub radial_segments: Segments,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct FigureEightSpec {
    pub width: Millimeters,
    pub height: Millimeters,
    pub bar: Millimeters,
    pub samples: Segments,
    pub radial_segments: Segments,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct FanPommelSpec {
    pub width: Millimeters,
    pub height: Millimeters,
    pub thickness: Millimeters,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct RondelSpec {
    pub radius: Millimeters,
    pub thickness: Millimeters,
    pub segments: Segments,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct GothicMaceSpec {
    pub length: Millimeters,
    pub root_radius: Millimeters,
    pub shoulder_radius: Millimeters,
    pub cusp_radius: Millimeters,
    pub cusp_height: Permille,
    pub concavity: Permille,
    pub crown_length: Millimeters,
    pub flanges: u8,
    pub flange_thickness: Millimeters,
    pub profile_samples: Segments,
    pub radial_segments: Segments,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ComponentShape {
    Cylinder(CylinderSpec),
    OvalGrip(OvalGripSpec),
    Blade(BladeSpec),
    Guard(GuardSpec),
    Mace(MaceSpec),
    Socket(SocketSpec),
    Langet(LangetSpec),
    Axe(AxeSpec),
    HammerPoll(HammerPollSpec),
    CurvedBeak(CurvedBeakSpec),
    FacetedBeak(FacetedBeakSpec),
    Glaive(GlaiveSpec),
    Bill(BillSpec),
    Fork(ForkSpec),
    Partisan(PartisanSpec),
    TubePath(TubePathSpec),
    RingGuard(RingGuardSpec),
    FigureEight(FigureEightSpec),
    FanPommel(FanPommelSpec),
    Rondel(RondelSpec),
    GothicMace(GothicMaceSpec),
    SlabGrip(SlabGripSpec),
    KnuckleBow(KnuckleBowSpec),
    Collar(CollarSpec),
    Sleeve(SleeveSpec),
    Boss(BossSpec),
    Spear(SpearSpec),
    ProfiledPommel(ProfiledPommelSpec),
}

impl ComponentShape {
    pub fn axial_length(&self) -> Millimeters {
        match self {
            Self::Cylinder(value) => value.length,
            Self::OvalGrip(value) => value.length,
            Self::Blade(value) => value.length,
            Self::Guard(_) => Millimeters(0),
            Self::Mace(value) => value.length,
            Self::Socket(value) => value.length,
            Self::Langet(value) => value.length,
            Self::Axe(_)
            | Self::HammerPoll(_)
            | Self::CurvedBeak(_)
            | Self::FacetedBeak(_)
            | Self::TubePath(_)
            | Self::RingGuard(_)
            | Self::FigureEight(_) => Millimeters(0),
            Self::Glaive(value) => value.length,
            Self::Bill(value) => value.length,
            Self::Fork(value) => value.length,
            Self::Partisan(value) => value.length,
            Self::FanPommel(value) => value.height,
            Self::Rondel(value) => value.thickness,
            Self::GothicMace(value) => {
                Millimeters(value.length.0.saturating_add(value.crown_length.0))
            }
            Self::SlabGrip(value) => value.length,
            Self::KnuckleBow(_) => Millimeters(0),
            Self::Collar(value) => value.width,
            Self::Sleeve(value) => value.length,
            Self::Boss(_) => Millimeters(0),
            Self::Spear(value) => value.length,
            Self::ProfiledPommel(value) => {
                value.profile.last().map_or(Millimeters(0), |point| point.y)
            }
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ComponentDesign {
    pub id: String,
    pub role: ComponentRole,
    pub attachment: Attachment,
    pub offset: OffsetMm,
    pub material: MaterialClass,
    pub shape: ComponentShape,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct WeaponDesign {
    pub catalog_id: String,
    pub components: Vec<ComponentDesign>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Anchor {
    pub name: String,
    pub position: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeshPart {
    pub component_id: String,
    pub material: MaterialClass,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub bounds: Bounds,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DerivedProperties {
    pub mass_kg: f32,
    pub length_m: f32,
    pub grip_to_tip_m: f32,
    /// Signed longitudinal center of mass relative to the controlling hand.
    /// Positive values lie toward the weapon head.
    pub center_of_mass_from_grip_m: f32,
    /// Mean transverse rotational inertia about the controlling hand.
    pub moment_of_inertia_kg_m2: f32,
    /// Radius of gyration divided by grip-to-tip length. Lower is easier to redirect.
    pub balance: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DerivedMaterialMass {
    pub material: MaterialClass,
    pub mass_kg: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedWeapon {
    pub design_hash: crate::DesignHash,
    pub parts: Vec<MeshPart>,
    pub bounds: Bounds,
    pub anchors: Vec<Anchor>,
    pub derived: DerivedProperties,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedWeaponHolder {
    pub design_hash: crate::DesignHash,
    pub kind: WeaponHolderKind,
    /// Holder coordinates use the same recipe-local frame as the weapon.
    pub grip: [f32; 3],
    pub parts: Vec<MeshPart>,
    pub bounds: Bounds,
    pub derived: DerivedProperties,
}
