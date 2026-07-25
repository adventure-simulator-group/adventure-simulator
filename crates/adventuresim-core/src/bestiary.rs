//! Shared, deterministic authority for threat identity, combat profiles, and
//! investigation-facing evidence. Stable IDs, never display text, drive rules.

use core::str::FromStr;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ThreatId {
    len: u8,
    bytes: [u8; 63],
}

pub const ALL_THREATS: &[ThreatId] = &[
    ThreatId::Bandit,
    ThreatId::Deserter,
    ThreatId::Poacher,
    ThreatId::Smuggler,
    ThreatId::Cultist,
    ThreatId::GraveRobber,
    ThreatId::TownWatch,
    ThreatId::ArmedRetainer,
    ThreatId::AngryMob,
    ThreatId::Wolf,
    ThreatId::Boar,
    ThreatId::Bear,
    ThreatId::FeralDog,
    ThreatId::TrainedDog,
    ThreatId::Goblin,
    ThreatId::Orc,
    ThreatId::Skeleton,
    ThreatId::Ghoul,
    ThreatId::Revenant,
    ThreatId::Werewolf,
    ThreatId::Alp,
    ThreatId::Kobold,
    ThreatId::WildMan,
    ThreatId::SpectralHound,
    ThreatId::Nachzehrer,
];

fn catalog_threats() -> Vec<ThreatId> {
    crate::quest_catalog::catalog()
        .monsters()
        .map(|monster| ThreatId::try_new(&monster.id).expect("validated monster ID"))
        .collect()
}

impl ThreatId {
    const fn from_static(value: &str) -> Self {
        let source = value.as_bytes();
        let mut bytes = [0; 63];
        let mut index = 0;
        while index < source.len() {
            bytes[index] = source[index];
            index += 1;
        }
        Self {
            len: source.len() as u8,
            bytes,
        }
    }
    pub fn try_new(value: &str) -> Result<Self, UnknownThreatId> {
        if value.is_empty()
            || value.len() > 63
            || !value.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'_' | b'-' | b'.' | b':')
            })
        {
            return Err(UnknownThreatId);
        }
        let mut bytes = [0; 63];
        bytes[..value.len()].copy_from_slice(value.as_bytes());
        Ok(Self {
            len: value.len() as u8,
            bytes,
        })
    }
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..usize::from(self.len)]).expect("validated ASCII ID")
    }

    #[allow(non_upper_case_globals)]
    pub const Bandit: Self = Self::from_static("bandit");
    #[allow(non_upper_case_globals)]
    pub const Deserter: Self = Self::from_static("deserter");
    #[allow(non_upper_case_globals)]
    pub const Poacher: Self = Self::from_static("poacher");
    #[allow(non_upper_case_globals)]
    pub const Smuggler: Self = Self::from_static("smuggler");
    #[allow(non_upper_case_globals)]
    pub const Cultist: Self = Self::from_static("cultist");
    #[allow(non_upper_case_globals)]
    pub const GraveRobber: Self = Self::from_static("grave_robber");
    #[allow(non_upper_case_globals)]
    pub const TownWatch: Self = Self::from_static("town_watch");
    #[allow(non_upper_case_globals)]
    pub const ArmedRetainer: Self = Self::from_static("armed_retainer");
    #[allow(non_upper_case_globals)]
    pub const AngryMob: Self = Self::from_static("angry_mob");
    #[allow(non_upper_case_globals)]
    pub const Wolf: Self = Self::from_static("wolf");
    #[allow(non_upper_case_globals)]
    pub const Boar: Self = Self::from_static("boar");
    #[allow(non_upper_case_globals)]
    pub const Bear: Self = Self::from_static("bear");
    #[allow(non_upper_case_globals)]
    pub const FeralDog: Self = Self::from_static("feral_dog");
    #[allow(non_upper_case_globals)]
    pub const TrainedDog: Self = Self::from_static("trained_dog");
    #[allow(non_upper_case_globals)]
    pub const Goblin: Self = Self::from_static("goblin");
    #[allow(non_upper_case_globals)]
    pub const Orc: Self = Self::from_static("orc");
    #[allow(non_upper_case_globals)]
    pub const Skeleton: Self = Self::from_static("skeleton");
    #[allow(non_upper_case_globals)]
    pub const Ghoul: Self = Self::from_static("ghoul");
    #[allow(non_upper_case_globals)]
    pub const Revenant: Self = Self::from_static("revenant");
    #[allow(non_upper_case_globals)]
    pub const Werewolf: Self = Self::from_static("werewolf");
    #[allow(non_upper_case_globals)]
    pub const Alp: Self = Self::from_static("alp");
    #[allow(non_upper_case_globals)]
    pub const Kobold: Self = Self::from_static("kobold");
    #[allow(non_upper_case_globals)]
    pub const WildMan: Self = Self::from_static("wild_man");
    #[allow(non_upper_case_globals)]
    pub const SpectralHound: Self = Self::from_static("spectral_hound");
    #[allow(non_upper_case_globals)]
    pub const Nachzehrer: Self = Self::from_static("nachzehrer");

    pub fn profile(self) -> ThreatProfile {
        profile(self)
    }

    pub fn display_name(self, count: u32) -> String {
        let profile = self.profile();
        if count == 1 {
            profile.singular_name
        } else {
            profile.plural_name
        }
        .to_string()
    }
}

impl FromStr for ThreatId {
    type Err = UnknownThreatId;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let id = Self::try_new(value)?;
        crate::quest_catalog::catalog()
            .monster(value)
            .map(|_| id)
            .ok_or(UnknownThreatId)
    }
}

impl core::fmt::Debug for ThreatId {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.as_str())
    }
}
impl Serialize for ThreatId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
impl<'de> Deserialize<'de> for ThreatId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        value
            .parse()
            .map_err(|_| serde::de::Error::custom("unknown threat ID"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownThreatId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RigTopology {
    Humanoid,
    Quadruped,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackStyle {
    Blade,
    Blunt,
    Knife,
    Spear,
    Bow,
    Bite,
    Claw,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protection {
    Unarmored,
    Hide,
    Shielded,
    Armored,
    Bone,
    Supernatural,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Temperament {
    Cowardly,
    Cautious,
    Disciplined,
    Aggressive,
    Relentless,
    Elusive,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActivityTime {
    Day,
    Night,
    Any,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Habitat {
    Road,
    Open,
    SparseWoods,
    DeepWoods,
    Cave,
    Crypt,
    Ruin,
    Camp,
    Mine,
    Graveyard,
    OccupiedHouse,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationVisibility {
    Clear,
    Dim,
    Dark,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservationDistance {
    Close,
    Medium,
    Far,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WitnessCapability {
    Poor,
    Ordinary,
    Trained,
}
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReportDescription(ThreatId);
impl ReportDescription {
    #[allow(non_upper_case_globals)]
    pub const ArmedPeople: Self = Self(ThreatId::from_static("armed_people"));
    #[allow(non_upper_case_globals)]
    pub const SmallUprightFigures: Self = Self(ThreatId::from_static("small_upright_figures"));
    #[allow(non_upper_case_globals)]
    pub const LargeUprightBeast: Self = Self(ThreatId::from_static("large_upright_beast"));
    #[allow(non_upper_case_globals)]
    pub const GauntHuman: Self = Self(ThreatId::from_static("gaunt_human"));
    #[allow(non_upper_case_globals)]
    pub const WalkingDead: Self = Self(ThreatId::from_static("walking_dead"));
    #[allow(non_upper_case_globals)]
    pub const LargeAnimal: Self = Self(ThreatId::from_static("large_animal"));
    #[allow(non_upper_case_globals)]
    pub const DoglikeBeast: Self = Self(ThreatId::from_static("doglike_beast"));
    #[allow(non_upper_case_globals)]
    pub const UnseenNightVisitor: Self = Self(ThreatId::from_static("unseen_night_visitor"));
    pub fn try_new(value: &str) -> Result<Self, UnknownThreatId> {
        ThreatId::try_new(value).map(Self)
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
impl core::fmt::Debug for ReportDescription {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}
impl Serialize for ReportDescription {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}
impl<'de> Deserialize<'de> for ReportDescription {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::try_new(&String::deserialize(deserializer)?)
            .map_err(|_| serde::de::Error::custom("invalid description ID"))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EvidenceKind {
    BootPrints,
    SmallBareTracks,
    Hoofprints,
    Pawprints,
    ClawMarks,
    GnawedBones,
    GraveSoil,
    NoBreath,
    WeaponCuts,
    ArrowShafts,
    CorpseOdor,
    SulfurOdor,
    ColdPatch,
    MissingBlood,
    DisturbedGoods,
    HumanSpeech,
    AnimalOdor,
    BrokenFoliage,
    BiteWounds,
    BluntDamage,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CatalogCountermeasure {
    ShatteringBlow,
    AntiArmor,
    Fire,
    Silver,
    Daylight,
    Courage,
    NoSpecial,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CountermeasureHypothesis {
    Fire,
    Silver,
    Daylight,
    Courage,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionalContext {
    NorthernGermany1544,
    GenericFantasy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CausalBridge {
    CellarCrypt,
    GraveyardTunnel,
    ResidentController,
    AbandonedMine,
}

/// Full-body material protection contributed by a threat's anatomy rather than
/// worn equipment. These values use the same joule-based resistance and
/// padding model as [`crate::autoresolve::CombatArmor`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InnateProtection {
    pub resistance_joules: f32,
    pub padding_joules: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct CombatProfile {
    pub rig: RigTopology,
    pub speed_m_per_minute: u32,
    pub weight_kg: f32,
    pub attack: AttackStyle,
    pub ranged: bool,
    pub precision_bonus: f32,
    pub training_multiplier: f32,
    pub perception: u8,
    pub stealth: u8,
    pub morale: u8,
    pub protection: Protection,
    pub innate_protection: InnateProtection,
    pub disease_risk: u8,
    pub fear: u8,
    pub temperament: Temperament,
    pub encounter_scale_basis_points: u16,
    pub loot_item_id: Option<&'static str>,
}

fn has_unsupported_layered_protection(combat: CombatProfile) -> bool {
    (combat.innate_protection.resistance_joules > 0.0
        || combat.innate_protection.padding_joules > 0.0)
        && matches!(combat.protection, Protection::Armored)
}

#[derive(Clone, Copy, Debug)]
pub struct InvestigationProfile {
    pub habitats: &'static [Habitat],
    pub activity: ActivityTime,
    pub victim_tags: &'static [&'static str],
    pub tracks: &'static [EvidenceKind],
    pub wounds: &'static [EvidenceKind],
    pub disturbances: &'static [EvidenceKind],
    pub sounds: &'static [&'static str],
    pub silhouettes: &'static [ReportDescription],
    pub odors: &'static [EvidenceKind],
    pub mistaken_for: &'static [ThreatId],
    pub distinguishing_clues: &'static [EvidenceKind],
    pub preparation_advice: &'static str,
    pub evidence_visibility: u8,
    pub identification_challenge: bool,
    pub location_challenge: bool,
    /// Leads worth investigating; these do not claim an implemented combat modifier.
    pub countermeasure_hypotheses: &'static [CountermeasureHypothesis],
}

#[derive(Clone, Copy, Debug)]
pub struct ThreatProfile {
    pub id: ThreatId,
    pub display_name: &'static str,
    pub singular_name: &'static str,
    pub plural_name: &'static str,
    pub aliases: &'static [&'static str],
    pub base_weight: u16,
    pub curation_weight: u16,
    pub combat: CombatProfile,
    pub investigation: InvestigationProfile,
}

const HUMANS: &[ThreatId] = &[
    ThreatId::Bandit,
    ThreatId::Deserter,
    ThreatId::Poacher,
    ThreatId::Smuggler,
    ThreatId::Cultist,
    ThreatId::GraveRobber,
    ThreatId::TownWatch,
    ThreatId::ArmedRetainer,
    ThreatId::AngryMob,
];
const DOGS: &[ThreatId] = &[
    ThreatId::Wolf,
    ThreatId::FeralDog,
    ThreatId::TrainedDog,
    ThreatId::SpectralHound,
    ThreatId::Werewolf,
];
const UPRIGHT: &[ThreatId] = &[
    ThreatId::Werewolf,
    ThreatId::WildMan,
    ThreatId::Orc,
    ThreatId::Bear,
];
const UNDEAD: &[ThreatId] = &[
    ThreatId::Skeleton,
    ThreatId::Ghoul,
    ThreatId::Revenant,
    ThreatId::Nachzehrer,
];

const fn combat(
    rig: RigTopology,
    speed: u32,
    weight: f32,
    attack: AttackStyle,
    ranged: bool,
    protection: Protection,
    countermeasure: CatalogCountermeasure,
    temperament: Temperament,
    loot: Option<&'static str>,
) -> CombatProfile {
    CombatProfile {
        rig,
        speed_m_per_minute: speed,
        weight_kg: weight,
        attack,
        ranged,
        precision_bonus: if ranged { 0.5 } else { 0.0 },
        training_multiplier: 1.0,
        perception: if ranged { 65 } else { 50 },
        stealth: 50,
        morale: 50,
        protection,
        innate_protection: if matches!(countermeasure, CatalogCountermeasure::ShatteringBlow) {
            // Bone resists an edge but provides no soft layer to dissipate a
            // crushing impact. Penetration still acts through the ordinary
            // resistance calculation rather than a species damage modifier.
            InnateProtection {
                resistance_joules: 150.0,
                padding_joules: 0.0,
            }
        } else {
            InnateProtection {
                resistance_joules: 0.0,
                padding_joules: 0.0,
            }
        },
        disease_risk: 0,
        fear: 0,
        temperament,
        encounter_scale_basis_points: 10_000,
        loot_item_id: loot,
    }
}

const fn investigation(
    habitats: &'static [Habitat],
    activity: ActivityTime,
    silhouettes: &'static [ReportDescription],
    mistaken_for: &'static [ThreatId],
    clues: &'static [EvidenceKind],
    advice: &'static str,
) -> InvestigationProfile {
    InvestigationProfile {
        habitats,
        activity,
        victim_tags: &["travelers", "livestock"],
        tracks: &[EvidenceKind::BootPrints],
        wounds: &[EvidenceKind::WeaponCuts],
        disturbances: &[EvidenceKind::DisturbedGoods],
        sounds: &["movement", "voices"],
        silhouettes,
        odors: &[],
        mistaken_for,
        distinguishing_clues: clues,
        preparation_advice: advice,
        evidence_visibility: 60,
        identification_challenge: false,
        location_challenge: false,
        countermeasure_hypotheses: &[],
    }
}

fn legacy_profile(id: ThreatId) -> ThreatProfile {
    use ActivityTime::*;
    use AttackStyle::*;
    use CatalogCountermeasure::*;
    use Habitat::*;
    use Protection::*;
    #[allow(non_upper_case_globals)]
    const ArmedPeople: ReportDescription = ReportDescription::ArmedPeople;
    #[allow(non_upper_case_globals)]
    const SmallUprightFigures: ReportDescription = ReportDescription::SmallUprightFigures;
    #[allow(non_upper_case_globals)]
    const LargeUprightBeast: ReportDescription = ReportDescription::LargeUprightBeast;
    #[allow(non_upper_case_globals)]
    const GauntHuman: ReportDescription = ReportDescription::GauntHuman;
    #[allow(non_upper_case_globals)]
    const WalkingDead: ReportDescription = ReportDescription::WalkingDead;
    #[allow(non_upper_case_globals)]
    const LargeAnimal: ReportDescription = ReportDescription::LargeAnimal;
    #[allow(non_upper_case_globals)]
    const DoglikeBeast: ReportDescription = ReportDescription::DoglikeBeast;
    #[allow(non_upper_case_globals)]
    const UnseenNightVisitor: ReportDescription = ReportDescription::UnseenNightVisitor;
    use RigTopology::*;
    use Temperament::*;
    let (name, aliases, base, curate, mut c, mut i): (
        &'static str,
        &'static [&'static str],
        u16,
        u16,
        CombatProfile,
        InvestigationProfile,
    ) = match id {
        ThreatId::Bandit => (
            "Bandit",
            &["brigand"] as &[_],
            80,
            70,
            combat(
                Humanoid,
                80,
                70.0,
                Blade,
                false,
                Armored,
                AntiArmor,
                Cautious,
                Some("katzbalger"),
            ),
            investigation(
                &[Road, Open, SparseWoods, Camp, Ruin],
                Any,
                &[ArmedPeople],
                HUMANS,
                &[EvidenceKind::BootPrints, EvidenceKind::WeaponCuts],
                "Bring armor and an anti-armor weapon; expect an organized group.",
            ),
        ),
        ThreatId::Deserter => (
            "Deserter",
            &["runaway soldier"],
            30,
            35,
            combat(
                Humanoid,
                82,
                72.0,
                Spear,
                false,
                Armored,
                AntiArmor,
                Disciplined,
                Some("spear"),
            ),
            investigation(
                &[Road, Camp, Ruin],
                Any,
                &[ArmedPeople],
                HUMANS,
                &[EvidenceKind::WeaponCuts, EvidenceKind::HumanSpeech],
                "Expect military weapons, formation discipline, and armor.",
            ),
        ),
        ThreatId::Poacher => (
            "Poacher",
            &["illegal hunter"],
            35,
            40,
            combat(
                Humanoid,
                84,
                68.0,
                Bow,
                true,
                Hide,
                NoSpecial,
                Elusive,
                Some("self_bow"),
            ),
            investigation(
                &[SparseWoods, DeepWoods, Camp],
                Any,
                &[ArmedPeople],
                HUMANS,
                &[EvidenceKind::ArrowShafts, EvidenceKind::BootPrints],
                "Use cover and close quickly against skilled bow fire.",
            ),
        ),
        ThreatId::Smuggler => (
            "Smuggler",
            &["contraband runner"],
            25,
            35,
            combat(
                Humanoid,
                83,
                70.0,
                Blade,
                false,
                Hide,
                NoSpecial,
                Cautious,
                Some("knife"),
            ),
            investigation(
                &[Road, Cave, OccupiedHouse],
                Night,
                &[ArmedPeople],
                HUMANS,
                &[EvidenceKind::DisturbedGoods, EvidenceKind::HumanSpeech],
                "Watch exits and bring enough people to prevent escape.",
            ),
        ),
        ThreatId::Cultist => (
            "Cultist",
            &["secret worshipper"],
            16,
            30,
            combat(
                Humanoid,
                78,
                67.0,
                Knife,
                false,
                Unarmored,
                Courage,
                Aggressive,
                Some("knife"),
            ),
            investigation(
                &[Ruin, Cave, OccupiedHouse],
                Night,
                &[ArmedPeople],
                HUMANS,
                &[EvidenceKind::SulfurOdor, EvidenceKind::HumanSpeech],
                "Resolve and religious knowledge help against frightening rites.",
            ),
        ),
        ThreatId::GraveRobber => (
            "Grave robber",
            &["resurrectionist"],
            18,
            30,
            combat(
                Humanoid,
                80,
                69.0,
                Blunt,
                false,
                Hide,
                NoSpecial,
                Cowardly,
                Some("club"),
            ),
            investigation(
                &[Graveyard, Crypt, OccupiedHouse],
                Night,
                &[GauntHuman],
                HUMANS,
                &[EvidenceKind::GraveSoil, EvidenceKind::BootPrints],
                "They are lightly equipped but likely to flee through prepared routes.",
            ),
        ),
        ThreatId::TownWatch => (
            "Town watch",
            &["watchman"],
            5,
            5,
            combat(
                Humanoid,
                80,
                72.0,
                Spear,
                false,
                Armored,
                AntiArmor,
                Disciplined,
                Some("spear"),
            ),
            investigation(
                &[Road, OccupiedHouse],
                Any,
                &[ArmedPeople],
                HUMANS,
                &[EvidenceKind::HumanSpeech],
                "Their armor and formation reward anti-armor weapons or withdrawal.",
            ),
        ),
        ThreatId::ArmedRetainer => (
            "Armed retainer",
            &["household soldier"],
            5,
            5,
            combat(
                Humanoid,
                82,
                75.0,
                Spear,
                false,
                Armored,
                AntiArmor,
                Disciplined,
                Some("spear"),
            ),
            investigation(
                &[Road, Camp, OccupiedHouse],
                Any,
                &[ArmedPeople],
                HUMANS,
                &[EvidenceKind::HumanSpeech],
                "Expect trained, armored opponents working in formation.",
            ),
        ),
        ThreatId::AngryMob => (
            "Angry townsfolk",
            &["angry mob"],
            5,
            5,
            combat(
                Humanoid,
                76,
                68.0,
                Blunt,
                false,
                Unarmored,
                Courage,
                Aggressive,
                Some("club"),
            ),
            investigation(
                &[Road, Open, OccupiedHouse],
                Any,
                &[ArmedPeople],
                HUMANS,
                &[EvidenceKind::HumanSpeech],
                "Withdrawal is safer than escalating a frightened crowd.",
            ),
        ),
        ThreatId::Wolf => (
            "Wolf",
            &["grey wolf"],
            65,
            65,
            combat(
                Quadruped, 92, 45.0, Bite, false, Hide, NoSpecial, Aggressive, None,
            ),
            investigation(
                &[Open, SparseWoods, DeepWoods],
                Any,
                &[DoglikeBeast],
                DOGS,
                &[EvidenceKind::Pawprints, EvidenceKind::GnawedBones],
                "Spears and a tight formation keep the pack at reach.",
            ),
        ),
        ThreatId::Boar => (
            "Boar",
            &["wild boar"],
            50,
            50,
            combat(
                Quadruped, 86, 90.0, Bite, false, Hide, NoSpecial, Aggressive, None,
            ),
            investigation(
                &[Open, SparseWoods, DeepWoods],
                Day,
                &[LargeAnimal],
                &[ThreatId::Bear],
                &[EvidenceKind::Hoofprints],
                "Use reach and avoid its initial charge.",
            ),
        ),
        ThreatId::Bear => (
            "Bear",
            &["brown bear"],
            30,
            45,
            combat(
                Quadruped, 80, 260.0, Claw, false, Hide, NoSpecial, Aggressive, None,
            ),
            investigation(
                &[SparseWoods, DeepWoods, Cave],
                Any,
                &[LargeAnimal, LargeUprightBeast],
                UPRIGHT,
                &[EvidenceKind::ClawMarks, EvidenceKind::Pawprints],
                "Bring heavy spears and do not fight it alone.",
            ),
        ),
        ThreatId::FeralDog => (
            "Feral dog",
            &["stray dog"],
            45,
            35,
            combat(
                Quadruped, 90, 30.0, Bite, false, Hide, NoSpecial, Aggressive, None,
            ),
            investigation(
                &[Road, Open, OccupiedHouse],
                Any,
                &[DoglikeBeast],
                DOGS,
                &[EvidenceKind::Pawprints],
                "A shield and spear blunt a pack's rush.",
            ),
        ),
        ThreatId::TrainedDog => (
            "Trained attack dog",
            &["guard dog"],
            20,
            30,
            combat(
                Quadruped,
                94,
                38.0,
                Bite,
                false,
                Hide,
                NoSpecial,
                Disciplined,
                None,
            ),
            investigation(
                &[Road, Camp, OccupiedHouse],
                Any,
                &[DoglikeBeast],
                DOGS,
                &[EvidenceKind::Pawprints, EvidenceKind::HumanSpeech],
                "Expect handlers: isolate the dogs instead of chasing them.",
            ),
        ),
        ThreatId::Goblin => (
            "Goblin",
            &["goblin raider"],
            45,
            65,
            combat(
                Humanoid,
                88,
                42.0,
                Bow,
                true,
                Hide,
                NoSpecial,
                Cowardly,
                Some("self_bow"),
            ),
            investigation(
                &[Cave, Mine, Ruin, SparseWoods, DeepWoods],
                Night,
                &[SmallUprightFigures],
                &[ThreatId::Kobold],
                &[EvidenceKind::SmallBareTracks, EvidenceKind::ArrowShafts],
                "Carry shields and close before their archers can scatter.",
            ),
        ),
        ThreatId::Orc => (
            "Orc",
            &["orc raider"],
            20,
            50,
            combat(
                Humanoid,
                82,
                105.0,
                Blunt,
                false,
                Armored,
                AntiArmor,
                Aggressive,
                Some("club"),
            ),
            investigation(
                &[Camp, Ruin, Cave],
                Any,
                &[LargeUprightBeast],
                UPRIGHT,
                &[EvidenceKind::BootPrints, EvidenceKind::WeaponCuts],
                "Use anti-armor weapons and avoid trading blows.",
            ),
        ),
        ThreatId::Skeleton => (
            "Skeleton",
            &["animated skeleton"],
            20,
            60,
            combat(
                Humanoid,
                58,
                35.0,
                Blade,
                false,
                Bone,
                ShatteringBlow,
                Relentless,
                None,
            ),
            investigation(
                &[Crypt, Graveyard, Ruin, Cave, DeepWoods, OccupiedHouse],
                Night,
                &[WalkingDead],
                UNDEAD,
                &[EvidenceKind::NoBreath, EvidenceKind::GraveSoil],
                "Blunt weapons shatter bone; cutting weapons are inefficient.",
            ),
        ),
        ThreatId::Ghoul => (
            "Ghoul",
            &["grave eater"],
            16,
            55,
            combat(
                Humanoid, 74, 55.0, Claw, false, Hide, Fire, Aggressive, None,
            ),
            investigation(
                &[Crypt, Graveyard, Cave],
                Night,
                &[GauntHuman, WalkingDead],
                UNDEAD,
                &[EvidenceKind::GnawedBones, EvidenceKind::CorpseOdor],
                "Protect wounds and avoid diseased remains. Fire is an unverified lead, not an autoresolve modifier.",
            ),
        ),
        ThreatId::Revenant => (
            "Revenant",
            &["returned dead"],
            8,
            35,
            combat(
                Humanoid,
                64,
                75.0,
                Blunt,
                false,
                Supernatural,
                Fire,
                Relentless,
                None,
            ),
            investigation(
                &[Crypt, Graveyard, Ruin, OccupiedHouse],
                Night,
                &[WalkingDead, GauntHuman],
                UNDEAD,
                &[EvidenceKind::NoBreath, EvidenceKind::ColdPatch],
                "Expect supernatural endurance. Fire is an unverified lead, not an autoresolve modifier.",
            ),
        ),
        ThreatId::Werewolf => (
            "Werewolf",
            &["therianthrope"],
            5,
            40,
            combat(
                Humanoid,
                96,
                95.0,
                Claw,
                false,
                Supernatural,
                Silver,
                Aggressive,
                None,
            ),
            investigation(
                &[DeepWoods, Cave, OccupiedHouse],
                Night,
                &[LargeUprightBeast, DoglikeBeast],
                UPRIGHT,
                &[EvidenceKind::BootPrints, EvidenceKind::Pawprints],
                "Contain it before it reaches isolated victims. Silver is an unverified lead, not an autoresolve modifier.",
            ),
        ),
        ThreatId::Alp => (
            "Alp",
            &["night-mare spirit"],
            3,
            30,
            combat(
                Humanoid,
                72,
                55.0,
                Claw,
                false,
                Supernatural,
                Daylight,
                Elusive,
                None,
            ),
            investigation(
                &[OccupiedHouse, Ruin],
                Night,
                &[UnseenNightVisitor, GauntHuman],
                &[ThreatId::Cultist, ThreatId::Nachzehrer],
                &[EvidenceKind::ColdPatch, EvidenceKind::NoBreath],
                "Identify its access. Daylight is an investigative lead, not an implemented combat modifier.",
            ),
        ),
        ThreatId::Kobold => (
            "Kobold",
            &["house spirit"],
            7,
            35,
            combat(
                Humanoid, 76, 35.0, Blunt, false, Unarmored, Courage, Elusive, None,
            ),
            investigation(
                &[Mine, OccupiedHouse, Cave],
                Night,
                &[SmallUprightFigures, UnseenNightVisitor],
                &[ThreatId::Goblin],
                &[EvidenceKind::DisturbedGoods, EvidenceKind::SmallBareTracks],
                "Once located it is frail; secure exits and distinguish pranks from theft.",
            ),
        ),
        ThreatId::WildMan => (
            "Wild man",
            &["woodwose"],
            8,
            35,
            combat(
                Humanoid,
                86,
                95.0,
                Blunt,
                false,
                Hide,
                NoSpecial,
                Cautious,
                Some("club"),
            ),
            investigation(
                &[DeepWoods, Cave, Ruin],
                Any,
                &[LargeUprightBeast],
                UPRIGHT,
                &[EvidenceKind::BootPrints, EvidenceKind::HumanSpeech],
                "Approach carefully: it may be reasoned with and knows the terrain.",
            ),
        ),
        ThreatId::SpectralHound => (
            "Spectral hound",
            &["black dog"],
            4,
            35,
            combat(
                Quadruped,
                100,
                45.0,
                Bite,
                false,
                Supernatural,
                Courage,
                Elusive,
                None,
            ),
            investigation(
                &[Road, Graveyard, Ruin],
                Night,
                &[DoglikeBeast],
                DOGS,
                &[EvidenceKind::ColdPatch, EvidenceKind::NoBreath],
                "Expect an elusive, frightening opponent. Ritual courage is an unverified lead, not an autoresolve modifier.",
            ),
        ),
        ThreatId::Nachzehrer => (
            "Nachzehrer",
            &["shroud eater"],
            3,
            35,
            combat(
                Humanoid,
                56,
                70.0,
                Bite,
                false,
                Supernatural,
                Fire,
                Relentless,
                None,
            ),
            investigation(
                &[Crypt, Graveyard, OccupiedHouse],
                Night,
                &[WalkingDead, UnseenNightVisitor],
                UNDEAD,
                &[EvidenceKind::MissingBlood, EvidenceKind::CorpseOdor],
                "Locate and contain the corpse. Fire is an unverified lead, not an autoresolve modifier.",
            ),
        ),
        _ => return legacy_profile(ThreatId::Bandit),
    };
    if matches!(id, ThreatId::Ghoul | ThreatId::Nachzehrer) {
        c.disease_risk = 70;
        c.fear = 45;
    }
    if matches!(
        id,
        ThreatId::Werewolf | ThreatId::SpectralHound | ThreatId::Revenant | ThreatId::Alp
    ) {
        c.fear = 70;
    }
    if matches!(id, ThreatId::Goblin | ThreatId::Kobold) {
        c.encounter_scale_basis_points = 13_000;
        c.morale = 30;
    }
    if matches!(id, ThreatId::Bear | ThreatId::Werewolf | ThreatId::Revenant) {
        c.encounter_scale_basis_points = 5_000;
        c.morale = 80;
    }
    if matches!(id, ThreatId::Poacher | ThreatId::Smuggler | ThreatId::Alp) {
        c.stealth = 75;
    }
    if matches!(
        id,
        ThreatId::Alp | ThreatId::Kobold | ThreatId::SpectralHound
    ) {
        i.identification_challenge = true;
        i.location_challenge = true;
    }
    // Replace generic authoring defaults with threat-coherent physical traces.
    match c.rig {
        RigTopology::Humanoid => {
            i.tracks = &[EvidenceKind::BootPrints];
            i.wounds = &[EvidenceKind::WeaponCuts];
            i.disturbances = &[EvidenceKind::DisturbedGoods];
            i.sounds = &["footsteps", "voices"];
            i.victim_tags = &["travelers", "settlement residents"];
        }
        RigTopology::Quadruped => {
            i.tracks = &[EvidenceKind::Pawprints];
            i.wounds = &[EvidenceKind::BiteWounds, EvidenceKind::ClawMarks];
            i.disturbances = &[EvidenceKind::BrokenFoliage];
            i.sounds = &["growls", "running paws"];
            i.odors = &[EvidenceKind::AnimalOdor];
            i.victim_tags = &["livestock", "isolated travelers"];
        }
    }
    match c.attack {
        AttackStyle::Blunt => i.wounds = &[EvidenceKind::BluntDamage],
        AttackStyle::Bow => i.wounds = &[EvidenceKind::ArrowShafts],
        AttackStyle::Bite => i.wounds = &[EvidenceKind::BiteWounds],
        AttackStyle::Claw => i.wounds = &[EvidenceKind::ClawMarks],
        AttackStyle::Blade | AttackStyle::Knife | AttackStyle::Spear => {
            i.wounds = &[EvidenceKind::WeaponCuts]
        }
    }
    match id {
        ThreatId::Boar => {
            i.tracks = &[EvidenceKind::Hoofprints];
            i.wounds = &[EvidenceKind::BiteWounds];
            i.victim_tags = &["crops", "foresters"];
        }
        ThreatId::Bear => {
            i.tracks = &[EvidenceKind::Pawprints];
            i.wounds = &[EvidenceKind::ClawMarks, EvidenceKind::BiteWounds];
            i.victim_tags = &["livestock", "foragers"];
        }
        ThreatId::Wolf | ThreatId::FeralDog | ThreatId::TrainedDog => {
            i.tracks = &[EvidenceKind::Pawprints];
            i.wounds = &[EvidenceKind::BiteWounds];
        }
        ThreatId::Skeleton => {
            i.tracks = &[EvidenceKind::GraveSoil];
            i.wounds = &[EvidenceKind::WeaponCuts];
            i.sounds = &["rattling bone", "scraping metal"];
            i.odors = &[];
            i.victim_tags = &["graveyard visitors", "travelers"];
            i.evidence_visibility = 85;
        }
        ThreatId::Ghoul => {
            i.tracks = &[EvidenceKind::GraveSoil, EvidenceKind::SmallBareTracks];
            i.wounds = &[EvidenceKind::BiteWounds];
            i.disturbances = &[EvidenceKind::GnawedBones, EvidenceKind::GraveSoil];
            i.sounds = &["scratching", "wet chewing"];
            i.odors = &[EvidenceKind::CorpseOdor];
            i.victim_tags = &["recently buried dead", "mourners"];
            i.evidence_visibility = 90;
            i.countermeasure_hypotheses = &[CountermeasureHypothesis::Fire];
        }
        ThreatId::Revenant => {
            i.tracks = &[EvidenceKind::GraveSoil, EvidenceKind::BootPrints];
            i.wounds = &[EvidenceKind::BluntDamage];
            i.disturbances = &[EvidenceKind::ColdPatch];
            i.sounds = &["slow footsteps"];
            i.odors = &[EvidenceKind::CorpseOdor];
            i.victim_tags = &["former associates", "graveyard visitors"];
            i.countermeasure_hypotheses = &[CountermeasureHypothesis::Fire];
        }
        ThreatId::Nachzehrer => {
            i.tracks = &[EvidenceKind::GraveSoil];
            i.wounds = &[EvidenceKind::BiteWounds, EvidenceKind::MissingBlood];
            i.disturbances = &[EvidenceKind::GnawedBones];
            i.sounds = &["chewing beneath earth"];
            i.odors = &[EvidenceKind::CorpseOdor];
            i.victim_tags = &["recently buried dead", "bereaved families"];
            i.evidence_visibility = 35;
            i.countermeasure_hypotheses = &[CountermeasureHypothesis::Fire];
        }
        ThreatId::Werewolf => {
            i.tracks = &[EvidenceKind::Pawprints, EvidenceKind::BootPrints];
            i.wounds = &[EvidenceKind::ClawMarks, EvidenceKind::BiteWounds];
            i.disturbances = &[EvidenceKind::BrokenFoliage];
            i.sounds = &["howling", "running paws"];
            i.odors = &[EvidenceKind::AnimalOdor];
            i.victim_tags = &["isolated people", "livestock"];
            i.countermeasure_hypotheses = &[CountermeasureHypothesis::Silver];
        }
        ThreatId::SpectralHound => {
            i.tracks = &[];
            i.wounds = &[EvidenceKind::BiteWounds];
            i.disturbances = &[EvidenceKind::ColdPatch];
            i.sounds = &["distant baying"];
            i.odors = &[];
            i.victim_tags = &["night travelers"];
            i.evidence_visibility = 20;
            i.countermeasure_hypotheses = &[CountermeasureHypothesis::Courage];
        }
        ThreatId::Alp => {
            i.tracks = &[];
            i.wounds = &[];
            i.disturbances = &[EvidenceKind::ColdPatch];
            i.sounds = &["breathing", "roof noises"];
            i.odors = &[];
            i.victim_tags = &["sleepers"];
            i.evidence_visibility = 15;
            i.countermeasure_hypotheses = &[CountermeasureHypothesis::Daylight];
        }
        ThreatId::Kobold => {
            i.tracks = &[EvidenceKind::SmallBareTracks];
            i.wounds = &[];
            i.disturbances = &[EvidenceKind::DisturbedGoods];
            i.sounds = &["knocking", "small footsteps"];
            i.odors = &[];
            i.victim_tags = &["households", "miners"];
            i.evidence_visibility = 40;
        }
        _ => {}
    }
    let (singular_name, plural_name) = display_forms(id, name);
    ThreatProfile {
        id,
        display_name: name,
        singular_name,
        plural_name,
        aliases,
        base_weight: base,
        curation_weight: curate,
        combat: c,
        investigation: i,
    }
}

/// Returns mechanics from the startup-compiled authoring catalog. The legacy
/// typed profile supplies the closed enum arrays still consumed by tactical
/// code; scalar combat values and player-facing identity are authoritative in
/// YAML and use ordinary resistance/padding rather than damage multipliers.
pub fn profile(id: ThreatId) -> ThreatProfile {
    static PROFILES: OnceLock<BTreeMap<ThreatId, ThreatProfile>> = OnceLock::new();
    *PROFILES
        .get_or_init(|| {
            crate::quest_catalog::catalog()
                .monsters()
                .map(|monster| {
                    let id = ThreatId::try_new(&monster.id).expect("validated monster ID");
                    (id, compile_profile(id, monster))
                })
                .collect()
        })
        .get(&id)
        .expect("threat ID was validated against startup catalog")
}

fn compile_profile(
    id: ThreatId,
    authored: &'static crate::quest_catalog::Monster,
) -> ThreatProfile {
    let mut profile = legacy_profile(id);
    profile.display_name = authored.name.as_str();
    profile.singular_name = authored.singular.as_str();
    profile.plural_name = authored.plural.as_str();
    profile.id = id;
    profile.aliases = Box::leak(
        authored
            .aliases
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    profile.base_weight = authored.base_weight;
    profile.curation_weight = authored.curation_weight;
    profile.combat.rig = match authored.combat.rig.as_str() {
        "humanoid" => RigTopology::Humanoid,
        "quadruped" => RigTopology::Quadruped,
        _ => unreachable!("validated combat rig"),
    };
    profile.combat.speed_m_per_minute = authored.combat.speed_m_per_minute;
    profile.combat.weight_kg = authored.combat.weight_kg;
    profile.combat.attack = match authored.combat.attack.as_str() {
        "blade" => AttackStyle::Blade,
        "blunt" | "tusk" => AttackStyle::Blunt,
        "knife" => AttackStyle::Knife,
        "spear" => AttackStyle::Spear,
        "bow" => AttackStyle::Bow,
        "bite" => AttackStyle::Bite,
        "claw" => AttackStyle::Claw,
        _ => unreachable!("validated attack style"),
    };
    profile.combat.ranged = authored.combat.ranged;
    profile.combat.precision_bonus = authored.combat.precision_bonus_milli as f32 / 1_000.0;
    profile.combat.training_multiplier =
        f32::from(authored.combat.training_multiplier_milli) / 1_000.0;
    profile.combat.perception = authored.combat.perception;
    profile.combat.stealth = authored.combat.stealth;
    profile.combat.morale = authored.combat.morale;
    profile.combat.protection = match authored.combat.protection.as_str() {
        "unarmored" => Protection::Unarmored,
        "hide" => Protection::Hide,
        "shielded" => Protection::Shielded,
        "armored" => Protection::Armored,
        "bone" => Protection::Bone,
        "supernatural" => Protection::Supernatural,
        _ => unreachable!("validated protection"),
    };
    profile.combat.innate_protection = InnateProtection {
        resistance_joules: authored.combat.resistance_joules as f32,
        padding_joules: authored.combat.padding_joules as f32,
    };
    profile.combat.disease_risk = authored.combat.disease_risk;
    profile.combat.fear = authored.combat.fear;
    profile.combat.temperament = match authored.combat.temperament.as_str() {
        "cowardly" => Temperament::Cowardly,
        "cautious" => Temperament::Cautious,
        "disciplined" => Temperament::Disciplined,
        "aggressive" => Temperament::Aggressive,
        "relentless" => Temperament::Relentless,
        "elusive" => Temperament::Elusive,
        _ => unreachable!("validated temperament"),
    };
    profile.combat.encounter_scale_basis_points = authored.combat.encounter_scale_basis_points;
    profile.combat.loot_item_id = authored.combat.loot_item_id.as_deref();
    profile.investigation.preparation_advice = authored.investigation.preparation_advice.as_str();
    profile.investigation.habitats = Box::leak(
        authored
            .investigation
            .habitats
            .iter()
            .map(|value| catalog_habitat(value))
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    profile.investigation.activity = match authored.investigation.activity.as_str() {
        "day" => ActivityTime::Day,
        "night" => ActivityTime::Night,
        "any" => ActivityTime::Any,
        _ => unreachable!("validated activity"),
    };
    profile.investigation.victim_tags = Box::leak(
        authored
            .investigation
            .victim_tags
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    profile.investigation.tracks = leak_evidence(&authored.investigation.tracks);
    profile.investigation.wounds = leak_evidence(&authored.investigation.wounds);
    profile.investigation.disturbances = leak_evidence(&authored.investigation.disturbances);
    profile.investigation.odors = leak_evidence(&authored.investigation.odors);
    profile.investigation.sounds = Box::leak(
        authored
            .investigation
            .sounds
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    profile.investigation.silhouettes = Box::leak(
        authored
            .investigation
            .silhouettes
            .iter()
            .map(|value| catalog_report(value))
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    profile.investigation.mistaken_for = Box::leak(
        authored
            .investigation
            .mistaken_for
            .iter()
            .map(|value| ThreatId::try_new(value).expect("validated monster reference"))
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    profile.investigation.distinguishing_clues =
        leak_evidence(&authored.investigation.distinguishing_clues);
    profile.investigation.countermeasure_hypotheses = Box::leak(
        authored
            .investigation
            .countermeasure_hypotheses
            .iter()
            .filter_map(|value| match value.as_str() {
                "fire" => Some(CountermeasureHypothesis::Fire),
                "silver" => Some(CountermeasureHypothesis::Silver),
                "daylight" => Some(CountermeasureHypothesis::Daylight),
                "courage" => Some(CountermeasureHypothesis::Courage),
                "shattering_blow" => None,
                _ => unreachable!("validated countermeasure"),
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    );
    profile.investigation.evidence_visibility = authored.investigation.evidence_visibility;
    profile.investigation.identification_challenge =
        authored.investigation.identification_challenge;
    profile.investigation.location_challenge = authored.investigation.location_challenge;
    profile
}

fn catalog_habitat(value: &str) -> Habitat {
    match value {
        "road" => Habitat::Road,
        "open" => Habitat::Open,
        "sparse_woods" => Habitat::SparseWoods,
        "deep_woods" => Habitat::DeepWoods,
        "cave" => Habitat::Cave,
        "crypt" => Habitat::Crypt,
        "ruin" => Habitat::Ruin,
        "camp" => Habitat::Camp,
        "mine" => Habitat::Mine,
        "graveyard" => Habitat::Graveyard,
        "occupied_house" => Habitat::OccupiedHouse,
        _ => unreachable!("validated habitat"),
    }
}

fn catalog_evidence(value: &str) -> EvidenceKind {
    match value {
        "boot_prints" | "large_boot_prints" | "bare_tracks" | "large_bare_tracks" => {
            EvidenceKind::BootPrints
        }
        "small_bare_tracks" => EvidenceKind::SmallBareTracks,
        "hoofprints" => EvidenceKind::Hoofprints,
        "pawprints" | "large_pawprints" | "pack_tracks" | "changing_tracks" => {
            EvidenceKind::Pawprints
        }
        "claw_marks" => EvidenceKind::ClawMarks,
        "gnawed_bones" | "bone_dust" => EvidenceKind::GnawedBones,
        "grave_soil" | "opened_graves" => EvidenceKind::GraveSoil,
        "no_breath" => EvidenceKind::NoBreath,
        "weapon_cuts" | "knife_wounds" | "heavy_weapon_cuts" | "tusk_wounds" => {
            EvidenceKind::WeaponCuts
        }
        "arrow_shafts" => EvidenceKind::ArrowShafts,
        "corpse_odor" => EvidenceKind::CorpseOdor,
        "sulfur_odor" => EvidenceKind::SulfurOdor,
        "cold_patch" => EvidenceKind::ColdPatch,
        "missing_blood" => EvidenceKind::MissingBlood,
        "disturbed_goods" | "camp_debris" | "snares" | "contraband" | "ritual_marks"
        | "tool_marks" | "official_seals" | "heraldry" | "broken_goods" | "rooted_soil"
        | "human_refuse" | "collar_marks" | "military_kit" | "local_slogans"
        | "personal_grudge" | "disturbed_bedding" | "moved_goods" | "helpful_mischief"
        | "woven_branches" | "damaged_shroud" | "eaten_shroud" => EvidenceKind::DisturbedGoods,
        "human_speech" => EvidenceKind::HumanSpeech,
        "animal_odor" | "woodsmoke" | "smoke" => EvidenceKind::AnimalOdor,
        "broken_foliage" => EvidenceKind::BrokenFoliage,
        "bite_wounds" => EvidenceKind::BiteWounds,
        "blunt_damage" => EvidenceKind::BluntDamage,
        _ => EvidenceKind::DisturbedGoods,
    }
}
fn leak_evidence(values: &[String]) -> &'static [EvidenceKind] {
    Box::leak(
        values
            .iter()
            .map(|value| catalog_evidence(value))
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    )
}
fn catalog_report(value: &str) -> ReportDescription {
    ReportDescription::try_new(value).expect("validated report description ID")
}

const fn display_forms(id: ThreatId, fallback: &'static str) -> (&'static str, &'static str) {
    match id {
        ThreatId::TownWatch => ("Town watch", "Town watch"),
        ThreatId::AngryMob => ("Angry townsman", "Angry townsfolk"),
        ThreatId::WildMan => ("Wild man", "Wild men"),
        ThreatId::GraveRobber => ("Grave robber", "Grave robbers"),
        ThreatId::FeralDog => ("Feral dog", "Feral dogs"),
        ThreatId::TrainedDog => ("Trained attack dog", "Trained attack dogs"),
        ThreatId::ArmedRetainer => ("Armed retainer", "Armed retainers"),
        ThreatId::SpectralHound => ("Spectral hound", "Spectral hounds"),
        ThreatId::Nachzehrer => ("Nachzehrer", "Nachzehrer"),
        _ => (
            fallback,
            match id {
                ThreatId::Bandit => "Bandits",
                ThreatId::Deserter => "Deserters",
                ThreatId::Poacher => "Poachers",
                ThreatId::Smuggler => "Smugglers",
                ThreatId::Cultist => "Cultists",
                ThreatId::Wolf => "Wolves",
                ThreatId::Boar => "Boars",
                ThreatId::Bear => "Bears",
                ThreatId::Goblin => "Goblins",
                ThreatId::Orc => "Orcs",
                ThreatId::Skeleton => "Skeletons",
                ThreatId::Ghoul => "Ghouls",
                ThreatId::Revenant => "Revenants",
                ThreatId::Werewolf => "Werewolves",
                ThreatId::Alp => "Alps",
                ThreatId::Kobold => "Kobolds",
                _ => fallback,
            },
        ),
    }
}

pub(crate) fn habitat_weight(id: ThreatId, habitat: Habitat) -> u16 {
    let p = profile(id);
    if !contains_habitat(p.investigation.habitats, habitat) {
        return 0;
    }
    match (id, habitat) {
        (ThreatId::Skeleton, Habitat::Crypt | Habitat::Graveyard) => 900,
        (ThreatId::Skeleton, Habitat::OccupiedHouse) => 10,
        (ThreatId::Werewolf, Habitat::OccupiedHouse) => 350,
        (ThreatId::Smuggler | ThreatId::Cultist, Habitat::OccupiedHouse) => 500,
        (_, Habitat::Road | Habitat::Open | Habitat::SparseWoods | Habitat::DeepWoods) => 300,
        _ => 200,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HabitatRelation {
    pub weight: u16,
    pub bridge: Option<CausalBridge>,
    pub explanatory_evidence: &'static [EvidenceKind],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HabitatRelationError {
    Impossible,
    MissingBridge,
    InvalidBridge,
    UnexpectedBridge,
}

/// Validated public selection surface for threat/habitat relations.
pub fn select_habitat_relation(
    id: ThreatId,
    habitat: Habitat,
    bridge: Option<CausalBridge>,
) -> Result<HabitatRelation, HabitatRelationError> {
    let weight = habitat_weight(id, habitat);
    if weight == 0 {
        return Err(HabitatRelationError::Impossible);
    }
    match (required_bridge(id, habitat), bridge) {
        (Some(_), None) => Err(HabitatRelationError::MissingBridge),
        (Some(allowed), Some(value)) if !allowed.contains(&value) => {
            Err(HabitatRelationError::InvalidBridge)
        }
        (Some(_), Some(value)) => Ok(HabitatRelation {
            weight,
            bridge: Some(value),
            explanatory_evidence: bridge_evidence(value),
        }),
        (None, Some(_)) => Err(HabitatRelationError::UnexpectedBridge),
        (None, None) => Ok(HabitatRelation {
            weight,
            bridge: None,
            explanatory_evidence: &[],
        }),
    }
}

const fn contains_habitat(values: &[Habitat], needle: Habitat) -> bool {
    let mut i = 0;
    while i < values.len() {
        if values[i] as u8 == needle as u8 {
            return true;
        }
        i += 1;
    }
    false
}

pub const fn required_bridge(id: ThreatId, habitat: Habitat) -> Option<&'static [CausalBridge]> {
    match (id, habitat) {
        (ThreatId::Skeleton, Habitat::OccupiedHouse) => Some(&[
            CausalBridge::CellarCrypt,
            CausalBridge::GraveyardTunnel,
            CausalBridge::ResidentController,
        ]),
        _ => None,
    }
}

pub const fn bridge_evidence(bridge: CausalBridge) -> &'static [EvidenceKind] {
    match bridge {
        CausalBridge::CellarCrypt => &[EvidenceKind::GraveSoil, EvidenceKind::ColdPatch],
        CausalBridge::GraveyardTunnel => &[EvidenceKind::GraveSoil, EvidenceKind::BootPrints],
        CausalBridge::ResidentController => {
            &[EvidenceKind::HumanSpeech, EvidenceKind::DisturbedGoods]
        }
        CausalBridge::AbandonedMine => &[EvidenceKind::BrokenFoliage],
    }
}

pub fn report_likelihood(
    id: ThreatId,
    report: ReportDescription,
    visibility: ObservationVisibility,
    distance: ObservationDistance,
    capability: WitnessCapability,
) -> u32 {
    let p = profile(id);
    let base = description_likelihood(id, report);
    if base == 0 {
        return 0;
    }
    let visibility = match visibility {
        ObservationVisibility::Clear => 100,
        ObservationVisibility::Dim => 55,
        ObservationVisibility::Dark => 20,
    };
    let distance = match distance {
        ObservationDistance::Close => 100,
        ObservationDistance::Medium => 60,
        ObservationDistance::Far => 25,
    };
    let capability = match capability {
        WitnessCapability::Poor => 35,
        WitnessCapability::Ordinary => 65,
        WitnessCapability::Trained => 100,
    };
    let clarity = (visibility + distance + capability) / 3;
    let evidence_visibility = u32::from(p.investigation.evidence_visibility);
    // Clear observations favor conspicuous threats; poor observations favor
    // elusive threats that plausibly yield only a vague report.
    let observation = if clarity >= 60 {
        50 + evidence_visibility * clarity / 100
    } else {
        50 + (100 - evidence_visibility) * (100 - clarity) / 100
    };
    base.saturating_mul(observation).max(1)
}

pub fn description_likelihood(id: ThreatId, report: ReportDescription) -> u32 {
    crate::quest_catalog::catalog()
        .relation(&format!("description.{}", report.as_str()))
        .and_then(|relation| {
            relation
                .candidates
                .iter()
                .find(|candidate| candidate.id == id.as_str())
        })
        .map_or(0, |candidate| candidate.plausibility)
}

pub fn regional_prior(id: ThreatId, region: RegionalContext) -> u16 {
    if matches!(region, RegionalContext::GenericFantasy) {
        return 100;
    }
    crate::quest_catalog::catalog()
        .monster(id.as_str())
        .expect("validated threat identity")
        .northern_germany_prior
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateScore {
    pub id: ThreatId,
    pub score: u64,
}

pub fn rank_candidates(
    report: ReportDescription,
    evidence: &[EvidenceKind],
    visibility: ObservationVisibility,
    distance: ObservationDistance,
    capability: WitnessCapability,
) -> Vec<CandidateScore> {
    rank_candidates_in_region(
        report,
        evidence,
        visibility,
        distance,
        capability,
        RegionalContext::NorthernGermany1544,
    )
}

pub fn rank_candidates_in_region(
    report: ReportDescription,
    evidence: &[EvidenceKind],
    visibility: ObservationVisibility,
    distance: ObservationDistance,
    capability: WitnessCapability,
    region: RegionalContext,
) -> Vec<CandidateScore> {
    let mut unique_evidence = Vec::new();
    for clue in evidence.iter().copied() {
        if !unique_evidence.contains(&clue) {
            unique_evidence.push(clue);
        }
        if unique_evidence.len() == 32 {
            break;
        }
    }
    let mut ranked: Vec<_> = catalog_threats()
        .into_iter()
        .filter_map(|id| {
            let likelihood = report_likelihood(id, report, visibility, distance, capability);
            if likelihood == 0 {
                return None;
            }
            let p = profile(id);
            let evidence_factor = unique_evidence.iter().fold(100_u64, |score, clue| {
                if p.investigation.distinguishing_clues.contains(clue) {
                    score.saturating_mul(100)
                } else if p.investigation.tracks.contains(clue)
                    || p.investigation.wounds.contains(clue)
                {
                    score.saturating_mul(2)
                } else {
                    score
                }
            });
            Some(CandidateScore {
                id,
                score: u64::from(p.base_weight)
                    .saturating_mul(u64::from(regional_prior(id, region)))
                    .saturating_mul(u64::from(likelihood))
                    .saturating_mul(evidence_factor),
            })
        })
        .collect();
    ranked.sort_by_key(|item| (core::cmp::Reverse(item.score), item.id));
    ranked
}

pub const ALL_REPORTS: &[ReportDescription] = &[
    ReportDescription::ArmedPeople,
    ReportDescription::SmallUprightFigures,
    ReportDescription::LargeUprightBeast,
    ReportDescription::GauntHuman,
    ReportDescription::WalkingDead,
    ReportDescription::LargeAnimal,
    ReportDescription::DoglikeBeast,
    ReportDescription::UnseenNightVisitor,
];
const ALL_HABITATS: &[Habitat] = &[
    Habitat::Road,
    Habitat::Open,
    Habitat::SparseWoods,
    Habitat::DeepWoods,
    Habitat::Cave,
    Habitat::Crypt,
    Habitat::Ruin,
    Habitat::Camp,
    Habitat::Mine,
    Habitat::Graveyard,
    Habitat::OccupiedHouse,
];

pub fn ambiguous_description_cardinality(report: ReportDescription) -> usize {
    catalog_threats()
        .into_iter()
        .filter(|id| description_likelihood(*id, report) > 0)
        .count()
}

pub fn distinguishing_clue_set_count(report: ReportDescription) -> usize {
    let mut sets: Vec<&'static [EvidenceKind]> = Vec::new();
    for id in catalog_threats()
        .into_iter()
        .filter(|id| description_likelihood(*id, report) > 0)
    {
        let clues = profile(id).investigation.distinguishing_clues;
        if !sets.contains(&clues) {
            sets.push(clues);
        }
    }
    sets.len()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CandidateMarginal {
    pub id: ThreatId,
    pub plausibility_basis_points: u16,
    pub curated_basis_points: u16,
}

pub fn distribution_summary(
    report: ReportDescription,
    region: RegionalContext,
) -> Vec<CandidateMarginal> {
    let ranked = rank_candidates_in_region(
        report,
        &[],
        ObservationVisibility::Dim,
        ObservationDistance::Medium,
        WitnessCapability::Ordinary,
        region,
    );
    let plausibility_total = ranked
        .iter()
        .fold(0_u64, |sum, item| sum.saturating_add(item.score))
        .max(1);
    let curated_total = ranked
        .iter()
        .fold(0_u64, |sum, item| {
            sum.saturating_add(
                item.score
                    .saturating_mul(u64::from(profile(item.id).curation_weight)),
            )
        })
        .max(1);
    ranked
        .into_iter()
        .map(|item| CandidateMarginal {
            id: item.id,
            plausibility_basis_points: ((item.score.saturating_mul(10_000) / plausibility_total)
                .min(10_000)) as u16,
            curated_basis_points: ((item
                .score
                .saturating_mul(u64::from(profile(item.id).curation_weight))
                .saturating_mul(10_000)
                / curated_total)
                .min(10_000)) as u16,
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogDiagnostic {
    pub message: String,
}

pub fn validate_catalog() -> Vec<CatalogDiagnostic> {
    let mut errors = Vec::new();
    let threats = catalog_threats();
    if has_duplicates(&threats) {
        errors.push(CatalogDiagnostic {
            message: "duplicate stable threat ID".into(),
        });
    }
    for id in &threats {
        let p = profile(*id);
        if id.as_str().parse::<ThreatId>() != Ok(*id) {
            errors.push(CatalogDiagnostic {
                message: format!("strict ID coverage failed for {}", id.as_str()),
            });
        }
        if p.id != *id || p.display_name.is_empty() || p.base_weight == 0 || p.curation_weight == 0
        {
            errors.push(CatalogDiagnostic {
                message: format!("invalid profile {}", id.as_str()),
            });
        }
        let combat = p.combat;
        if combat.speed_m_per_minute == 0
            || combat.weight_kg <= 0.0
            || !combat.weight_kg.is_finite()
            || combat.innate_protection.resistance_joules < 0.0
            || !combat.innate_protection.resistance_joules.is_finite()
            || combat.innate_protection.padding_joules < 0.0
            || !combat.innate_protection.padding_joules.is_finite()
            || combat.perception > 100
            || combat.stealth > 100
            || combat.morale > 100
            || combat.evidence_invalid()
        {
            errors.push(CatalogDiagnostic {
                message: format!("invalid numeric combat profile {}", id.as_str()),
            });
        }
        if has_unsupported_layered_protection(combat) {
            errors.push(CatalogDiagnostic {
                message: format!(
                    "innate and worn armor composition is unsupported for {}",
                    id.as_str()
                ),
            });
        }
        if has_duplicates(p.aliases)
            || has_duplicates(p.investigation.tracks)
            || has_duplicates(p.investigation.wounds)
            || has_duplicates(p.investigation.disturbances)
            || has_duplicates(p.investigation.odors)
            || p.investigation.evidence_visibility > 100
        {
            errors.push(CatalogDiagnostic {
                message: format!("duplicate or invalid investigation values {}", id.as_str()),
            });
        }
        if p.investigation.habitats.is_empty()
            || p.investigation.silhouettes.is_empty()
            || p.investigation.distinguishing_clues.is_empty()
        {
            errors.push(CatalogDiagnostic {
                message: format!("incomplete investigation profile {}", id.as_str()),
            });
        }
        for habitat in p.investigation.habitats {
            if habitat_weight(*id, *habitat) == 0 {
                errors.push(CatalogDiagnostic {
                    message: format!("unreachable habitat for {}", id.as_str()),
                });
            }
        }
        if ALL_HABITATS
            .iter()
            .all(|habitat| habitat_weight(*id, *habitat) == 0)
        {
            errors.push(CatalogDiagnostic {
                message: format!("threat {} is unreachable", id.as_str()),
            });
        }
        for habitat in ALL_HABITATS {
            let weight = habitat_weight(*id, *habitat);
            if weight > 0 && weight < 25 && required_bridge(*id, *habitat).is_none() {
                errors.push(CatalogDiagnostic {
                    message: format!("rare relation lacks bridge for {}", id.as_str()),
                });
            }
        }
    }
    for report in ALL_REPORTS {
        let cardinality = ambiguous_description_cardinality(*report);
        if cardinality < 2 {
            errors.push(CatalogDiagnostic {
                message: format!("description {report:?} is not ambiguous"),
            });
        }
        let marginals = distribution_summary(*report, RegionalContext::NorthernGermany1544);
        if marginals
            .iter()
            .any(|item| item.curated_basis_points > 9_500)
        {
            errors.push(CatalogDiagnostic {
                message: format!("description {report:?} is over-dominant"),
            });
        }
        if cardinality > 1 && distinguishing_clue_set_count(*report) < 2 {
            errors.push(CatalogDiagnostic {
                message: format!("description {report:?} lacks distinguishing clues"),
            });
        }
    }
    errors
}

impl CombatProfile {
    fn evidence_invalid(self) -> bool {
        self.encounter_scale_basis_points == 0
            || self.perception > 100
            || self.stealth > 100
            || self.morale > 100
    }
}

fn has_duplicates<T: PartialEq>(values: &[T]) -> bool {
    values
        .iter()
        .enumerate()
        .any(|(index, value)| values[..index].contains(value))
}

pub fn diagnose_scores(scores: &[CandidateScore]) -> Vec<CatalogDiagnostic> {
    let total = scores
        .iter()
        .fold(0_u64, |sum, item| sum.saturating_add(item.score));
    let mut out = Vec::new();
    if total == 0 {
        out.push(CatalogDiagnostic {
            message: "unreachable distribution".into(),
        });
    }
    if total > 0
        && scores
            .iter()
            .any(|item| item.score.saturating_mul(10_000) / total > 9_500)
    {
        out.push(CatalogDiagnostic {
            message: "over-dominant distribution".into(),
        });
    }
    out
}

/// Preparation derived only from evidence. It deliberately cannot accept a
/// hidden threat ID, preventing future investigation UI from leaking truth.
pub fn evidence_limited_preparation(
    report: ReportDescription,
    evidence: &[EvidenceKind],
) -> Vec<&'static str> {
    let ranked = rank_candidates(
        report,
        evidence,
        ObservationVisibility::Dim,
        ObservationDistance::Medium,
        WitnessCapability::Ordinary,
    );
    let mut advice = Vec::new();
    for candidate in ranked.into_iter().take(3) {
        let value = profile(candidate.id).investigation.preparation_advice;
        if !advice.contains(&value) {
            advice.push(value);
        }
    }
    advice
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_is_valid_and_ids_are_strict() {
        let diagnostics = validate_catalog();
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        assert_eq!("skeleton".parse(), Ok(ThreatId::Skeleton));
        assert!("restless skeletons".parse::<ThreatId>().is_err());
    }
    #[test]
    fn an_early_report_supports_multiple_truths() {
        let ranked = rank_candidates(
            ReportDescription::LargeUprightBeast,
            &[],
            ObservationVisibility::Dark,
            ObservationDistance::Far,
            WitnessCapability::Poor,
        );
        assert!(ranked.len() >= 3);
    }
    #[test]
    fn distinguishing_evidence_changes_ranking() {
        let before = rank_candidates(
            ReportDescription::WalkingDead,
            &[],
            ObservationVisibility::Dim,
            ObservationDistance::Medium,
            WitnessCapability::Ordinary,
        );
        let after = rank_candidates(
            ReportDescription::WalkingDead,
            &[EvidenceKind::MissingBlood],
            ObservationVisibility::Dim,
            ObservationDistance::Medium,
            WitnessCapability::Ordinary,
        );
        assert_ne!(before[0].id, after[0].id);
        assert_eq!(after[0].id, ThreatId::Nachzehrer);
    }
    #[test]
    fn inverse_conclusions_are_computed_from_forward_likelihood_and_prior() {
        let ranked = rank_candidates(
            ReportDescription::DoglikeBeast,
            &[EvidenceKind::ColdPatch],
            ObservationVisibility::Dark,
            ObservationDistance::Far,
            WitnessCapability::Ordinary,
        );
        assert_eq!(ranked[0].id, ThreatId::SpectralHound);
    }
    #[test]
    fn rare_combinations_require_typed_bridges() {
        assert!(habitat_weight(ThreatId::Skeleton, Habitat::OccupiedHouse) > 0);
        let bridges = required_bridge(ThreatId::Skeleton, Habitat::OccupiedHouse).unwrap();
        assert!(!bridge_evidence(bridges[0]).is_empty());
        assert_eq!(
            select_habitat_relation(ThreatId::Skeleton, Habitat::OccupiedHouse, None),
            Err(HabitatRelationError::MissingBridge)
        );
        assert_eq!(
            select_habitat_relation(
                ThreatId::Skeleton,
                Habitat::OccupiedHouse,
                Some(CausalBridge::CellarCrypt)
            )
            .unwrap()
            .explanatory_evidence,
            bridge_evidence(CausalBridge::CellarCrypt)
        );
        assert_eq!(
            select_habitat_relation(
                ThreatId::Skeleton,
                Habitat::OccupiedHouse,
                Some(CausalBridge::ResidentController)
            )
            .unwrap()
            .bridge,
            Some(CausalBridge::ResidentController)
        );
        assert_eq!(
            select_habitat_relation(
                ThreatId::Skeleton,
                Habitat::OccupiedHouse,
                Some(CausalBridge::AbandonedMine)
            ),
            Err(HabitatRelationError::InvalidBridge)
        );
        assert_eq!(
            select_habitat_relation(ThreatId::Skeleton, Habitat::Road, None),
            Err(HabitatRelationError::Impossible)
        );
        assert_eq!(
            select_habitat_relation(
                ThreatId::Wolf,
                Habitat::Open,
                Some(CausalBridge::CellarCrypt)
            ),
            Err(HabitatRelationError::UnexpectedBridge)
        );
    }
    #[test]
    fn identification_and_location_challenges_exist() {
        assert!(
            ALL_THREATS
                .iter()
                .filter(|id| {
                    let i = profile(**id).investigation;
                    i.identification_challenge && i.location_challenge
                })
                .count()
                >= 2
        );
    }

    #[test]
    fn observation_context_changes_relative_ranking_without_removing_ambiguity() {
        let clear = rank_candidates(
            ReportDescription::DoglikeBeast,
            &[],
            ObservationVisibility::Clear,
            ObservationDistance::Close,
            WitnessCapability::Trained,
        );
        let dark = rank_candidates(
            ReportDescription::DoglikeBeast,
            &[],
            ObservationVisibility::Dark,
            ObservationDistance::Far,
            WitnessCapability::Poor,
        );
        assert!(clear.len() >= 3 && dark.len() >= 3);
        let score =
            |items: &[CandidateScore], id| items.iter().find(|item| item.id == id).unwrap().score;
        assert_ne!(
            score(&clear, ThreatId::Wolf) / score(&clear, ThreatId::SpectralHound).max(1),
            score(&dark, ThreatId::Wolf) / score(&dark, ThreatId::SpectralHound).max(1)
        );
    }

    #[test]
    fn regional_priors_change_plausibility_without_changing_curation_authority() {
        let north = distribution_summary(
            ReportDescription::SmallUprightFigures,
            RegionalContext::NorthernGermany1544,
        );
        let generic = distribution_summary(
            ReportDescription::SmallUprightFigures,
            RegionalContext::GenericFantasy,
        );
        assert_ne!(north, generic);
        assert_eq!(profile(ThreatId::Goblin).curation_weight, 65);
    }

    #[test]
    fn repeated_and_unbounded_evidence_is_deduplicated_and_saturating() {
        let once = rank_candidates(
            ReportDescription::WalkingDead,
            &[EvidenceKind::MissingBlood],
            ObservationVisibility::Dim,
            ObservationDistance::Medium,
            WitnessCapability::Ordinary,
        );
        let repeated = rank_candidates(
            ReportDescription::WalkingDead,
            &vec![EvidenceKind::MissingBlood; 10_000],
            ObservationVisibility::Dim,
            ObservationDistance::Medium,
            WitnessCapability::Ordinary,
        );
        assert_eq!(once, repeated);
        assert!(repeated.iter().all(|item| item.score > 0));
    }

    #[test]
    fn diagnostics_report_unreachable_and_over_dominant_fixtures() {
        assert!(
            diagnose_scores(&[])
                .iter()
                .any(|item| item.message.contains("unreachable"))
        );
        assert!(
            diagnose_scores(&[
                CandidateScore {
                    id: ThreatId::Wolf,
                    score: 1000
                },
                CandidateScore {
                    id: ThreatId::Bear,
                    score: 1
                }
            ])
            .iter()
            .any(|item| item.message.contains("over-dominant"))
        );
        assert!(
            ALL_REPORTS
                .iter()
                .all(|report| ambiguous_description_cardinality(*report) >= 2)
        );
    }

    #[test]
    fn explicit_display_forms_handle_collectives_and_irregulars() {
        assert_eq!(ThreatId::TownWatch.display_name(4), "Town watch");
        assert_eq!(ThreatId::AngryMob.display_name(4), "Angry townsfolk");
        assert_eq!(ThreatId::WildMan.display_name(2), "Wild men");
        assert_eq!(ThreatId::Wolf.display_name(2), "Wolves");
        assert_eq!(ThreatId::Bandit.display_name(1), "Bandit");
    }

    #[test]
    fn representative_profiles_expose_multiple_supported_preparation_choices() {
        let skeleton = profile(ThreatId::Skeleton).combat;
        assert!(skeleton.innate_protection.resistance_joules > 0.0);
        assert_eq!(skeleton.innate_protection.padding_joules, 0.0);
        assert_eq!(
            profile(ThreatId::Orc).combat.protection,
            Protection::Armored
        );
        assert!(profile(ThreatId::Goblin).combat.ranged);
        assert!(
            profile(ThreatId::Bear).combat.encounter_scale_basis_points
                < profile(ThreatId::Goblin)
                    .combat
                    .encounter_scale_basis_points
        );
        assert!(
            profile(ThreatId::Werewolf)
                .investigation
                .countermeasure_hypotheses
                .contains(&CountermeasureHypothesis::Silver)
        );
    }

    #[test]
    fn catalog_rejects_combined_innate_and_worn_armor_until_layers_are_modeled() {
        let mut unsupported = profile(ThreatId::Skeleton).combat;
        unsupported.protection = Protection::Armored;
        assert!(has_unsupported_layered_protection(unsupported));
        assert!(
            ALL_THREATS
                .iter()
                .all(|id| !has_unsupported_layered_protection(profile(*id).combat))
        );
    }
}
