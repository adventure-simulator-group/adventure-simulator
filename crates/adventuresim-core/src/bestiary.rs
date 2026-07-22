//! Shared, deterministic authority for threat identity, combat profiles, and
//! investigation-facing evidence. Stable IDs, never display text, drive rules.

use core::str::FromStr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ThreatId {
    Bandit,
    Deserter,
    Poacher,
    Smuggler,
    Cultist,
    GraveRobber,
    TownWatch,
    ArmedRetainer,
    AngryMob,
    Wolf,
    Boar,
    Bear,
    FeralDog,
    TrainedDog,
    Goblin,
    Orc,
    Skeleton,
    Ghoul,
    Revenant,
    Werewolf,
    Alp,
    Kobold,
    WildMan,
    SpectralHound,
    Nachzehrer,
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

impl ThreatId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bandit => "bandit",
            Self::Deserter => "deserter",
            Self::Poacher => "poacher",
            Self::Smuggler => "smuggler",
            Self::Cultist => "cultist",
            Self::GraveRobber => "grave_robber",
            Self::TownWatch => "town_watch",
            Self::ArmedRetainer => "armed_retainer",
            Self::AngryMob => "angry_mob",
            Self::Wolf => "wolf",
            Self::Boar => "boar",
            Self::Bear => "bear",
            Self::FeralDog => "feral_dog",
            Self::TrainedDog => "trained_dog",
            Self::Goblin => "goblin",
            Self::Orc => "orc",
            Self::Skeleton => "skeleton",
            Self::Ghoul => "ghoul",
            Self::Revenant => "revenant",
            Self::Werewolf => "werewolf",
            Self::Alp => "alp",
            Self::Kobold => "kobold",
            Self::WildMan => "wild_man",
            Self::SpectralHound => "spectral_hound",
            Self::Nachzehrer => "nachzehrer",
        }
    }

    pub const fn profile(self) -> ThreatProfile {
        profile(self)
    }

    pub fn display_name(self, count: u32) -> String {
        let name = self.profile().display_name;
        if count == 1 {
            name.to_string()
        } else if name.ends_with('s') {
            name.to_string()
        } else {
            format!("{name}s")
        }
    }
}

impl FromStr for ThreatId {
    type Err = UnknownThreatId;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        ALL_THREATS
            .iter()
            .copied()
            .find(|id| id.as_str() == value)
            .ok_or(UnknownThreatId)
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReportDescription {
    ArmedPeople,
    SmallUprightFigures,
    LargeUprightBeast,
    GauntHuman,
    WalkingDead,
    LargeAnimal,
    DoglikeBeast,
    UnseenNightVisitor,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vulnerability {
    ShatteringBlow,
    AntiArmor,
    Fire,
    Silver,
    Daylight,
    Courage,
    NoSpecial,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CausalBridge {
    CellarCrypt,
    GraveyardTunnel,
    ResidentController,
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
    pub vulnerability: Vulnerability,
    pub cut_damage_multiplier: f32,
    pub blunt_damage_multiplier: f32,
    pub disease_risk: u8,
    pub fear: u8,
    pub temperament: Temperament,
    pub encounter_scale_basis_points: u16,
    pub loot_item_id: Option<&'static str>,
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
}

#[derive(Clone, Copy, Debug)]
pub struct ThreatProfile {
    pub id: ThreatId,
    pub display_name: &'static str,
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
    vulnerability: Vulnerability,
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
        vulnerability,
        cut_damage_multiplier: if matches!(vulnerability, Vulnerability::ShatteringBlow) {
            0.35
        } else {
            1.0
        },
        blunt_damage_multiplier: if matches!(vulnerability, Vulnerability::ShatteringBlow) {
            1.8
        } else {
            1.0
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
    }
}

pub const fn profile(id: ThreatId) -> ThreatProfile {
    use ActivityTime::*;
    use AttackStyle::*;
    use Habitat::*;
    use Protection::*;
    use ReportDescription::*;
    use RigTopology::*;
    use Temperament::*;
    use Vulnerability::*;
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
                &[Road, Camp, Ruin],
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
                &[Cave, Mine, Ruin, DeepWoods],
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
                &[Crypt, Graveyard, Ruin, Cave, OccupiedHouse],
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
                "Use fire, protect wounds, and avoid diseased remains.",
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
                "Fire and religious support help against its supernatural endurance.",
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
                "Silver is decisive; contain it before it reaches isolated victims.",
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
                "Identify its access and confront it in daylight; combat is otherwise elusive.",
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
                "Courage and religious support matter more than ordinary armor.",
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
                "Locate and contain the corpse; fire prevents its return.",
            ),
        ),
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
    ThreatProfile {
        id,
        display_name: name,
        aliases,
        base_weight: base,
        curation_weight: curate,
        combat: c,
        investigation: i,
    }
}

pub const fn habitat_weight(id: ThreatId, habitat: Habitat) -> u16 {
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
    if !p.investigation.silhouettes.contains(&report) {
        return 0;
    }
    let visibility = match visibility {
        ObservationVisibility::Clear => 100,
        ObservationVisibility::Dim => 75,
        ObservationVisibility::Dark => 50,
    };
    let distance = match distance {
        ObservationDistance::Close => 100,
        ObservationDistance::Medium => 75,
        ObservationDistance::Far => 50,
    };
    let capability = match capability {
        WitnessCapability::Poor => 65,
        WitnessCapability::Ordinary => 100,
        WitnessCapability::Trained => 125,
    };
    (visibility * distance * capability / 10_000).max(1)
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
    let mut ranked: Vec<_> = ALL_THREATS
        .iter()
        .copied()
        .filter_map(|id| {
            let likelihood = report_likelihood(id, report, visibility, distance, capability);
            if likelihood == 0 {
                return None;
            }
            let p = profile(id);
            let evidence_factor = evidence.iter().fold(100_u64, |score, clue| {
                if p.investigation.distinguishing_clues.contains(clue) {
                    score * 100
                } else if p.investigation.tracks.contains(clue)
                    || p.investigation.wounds.contains(clue)
                {
                    score * 2
                } else {
                    score
                }
            });
            Some(CandidateScore {
                id,
                score: u64::from(p.base_weight) * u64::from(likelihood) * evidence_factor,
            })
        })
        .collect();
    ranked.sort_by_key(|item| (core::cmp::Reverse(item.score), item.id));
    ranked
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogDiagnostic {
    pub message: String,
}

pub fn validate_catalog() -> Vec<CatalogDiagnostic> {
    let mut errors = Vec::new();
    for id in ALL_THREATS {
        let p = profile(*id);
        if p.id != *id || p.display_name.is_empty() || p.base_weight == 0 || p.curation_weight == 0
        {
            errors.push(CatalogDiagnostic {
                message: format!("invalid profile {}", id.as_str()),
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
        if habitat_weight(*id, Habitat::OccupiedHouse) < 25
            && habitat_weight(*id, Habitat::OccupiedHouse) > 0
            && required_bridge(*id, Habitat::OccupiedHouse).is_none()
        {
            errors.push(CatalogDiagnostic {
                message: format!(
                    "rare occupied-house relation lacks bridge for {}",
                    id.as_str()
                ),
            });
        }
    }
    errors
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
        assert!(validate_catalog().is_empty());
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
}
