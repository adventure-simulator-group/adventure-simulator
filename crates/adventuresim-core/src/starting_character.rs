//! Pure, versioned generation for the first-character candidate roster.

use adventuresim_world_schema::{BestiaryHours, ReligionHours};
use serde::{Deserialize, Serialize};

use crate::organization::{Requirement, StartingProfession, catalog};
use crate::skill::Skill;

pub const GENERATOR_VERSION: u16 = 4;
pub const YOUNG_ROSTER_SIZE: u8 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StartingAgeTier {
    Young,
    Adult,
    Old,
}

impl StartingAgeTier {
    pub const ALL: [Self; 3] = [Self::Young, Self::Adult, Self::Old];

    pub const fn age_years(self) -> u16 {
        match self {
            Self::Young => 16,
            Self::Adult => 22,
            Self::Old => 40,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Young => "young",
            Self::Adult => "adult",
            Self::Old => "old",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tier| tier.as_str() == value)
    }

    pub const fn roster_size(self) -> u8 {
        match self {
            Self::Young => YOUNG_ROSTER_SIZE,
            Self::Adult | Self::Old => StartingProfession::ALL.len() as u8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartingSlot {
    LeftHand,
    RightHand,
    LeftArm,
    RightArm,
    LeftLeg,
    RightLeg,
    Head,
    Chest,
    Stomach,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartingItem {
    pub item_id: String,
    pub quantity: u32,
    pub equipped: Option<StartingSlot>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StartingAttributes {
    pub endurance: f32,
    pub immunity: f32,
    pub gut: f32,
    pub intelligence: f32,
    pub instinct: f32,
    pub eyesight: f32,
    pub hearing: f32,
    pub strength: f32,
    pub agility: f32,
}

impl crate::attribute::PlayerAttributes for StartingAttributes {
    fn raw_limb_attr(
        &self,
        attr: crate::attribute::LimbAttribute,
        _limb: crate::body::BodyPart,
    ) -> f32 {
        match attr {
            crate::attribute::LimbAttribute::Strength => self.strength,
            crate::attribute::LimbAttribute::Agility => self.agility,
        }
    }

    fn raw_single_body_part_attr(&self, attr: crate::attribute::SimpleAttribute) -> f32 {
        match attr {
            crate::attribute::SimpleAttribute::Endurance => self.endurance,
            crate::attribute::SimpleAttribute::Immunity => self.immunity,
            crate::attribute::SimpleAttribute::Gut => self.gut,
            crate::attribute::SimpleAttribute::Intelligence => self.intelligence,
            crate::attribute::SimpleAttribute::Instinct => self.instinct,
            crate::attribute::SimpleAttribute::Eyesight => self.eyesight,
            crate::attribute::SimpleAttribute::Hearing => self.hearing,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StartingSkills {
    pub written: adventuresim_world_schema::WrittenLanguageHours,
    pub polearm: f32,
    pub axe: f32,
    pub bludgeon: f32,
    pub sword: f32,
    pub knife: f32,
    pub dodge: f32,
    pub block: f32,
    pub bow: f32,
    pub crossbow: f32,
    pub firearm: f32,
    pub throw: f32,
    pub will: f32,
    pub insight: f32,
    pub charm: f32,
    pub command: f32,
    pub deception: f32,
    pub physiology: f32,
    pub bestiary: BestiaryHours,
    pub surgery: f32,
    pub stealth: f32,
    pub balance: f32,
    pub cooking: f32,
    pub herbalism: f32,
    pub religion: ReligionHours,
    pub terrain_plains: f32,
    pub terrain_forest: f32,
    pub terrain_hills: f32,
    pub terrain_wetlands: f32,
    pub terrain_urban: f32,
    pub terrain_snow: f32,
    pub tailoring: f32,
    pub smithing: f32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartingOrganization {
    pub organization_id: String,
    pub organization_name: String,
    pub rank_id: String,
    pub rank_name: String,
}

/// Exact non-neutral personality axes persisted for a generated character.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartingPersonalityTrait {
    Brave,
    Fearful,
    Ambitious,
    Content,
    Sanguine,
    Brooding,
    Gregarious,
    Solitary,
    Compassionate,
    Callous,
    Cruel,
    Proud,
    Humble,
    Zealous,
    Irreverent,
    Slovenly,
    Cleanly,
    Temperate,
    Drunkard,
    Merry,
    Grave,
    Amorous,
    Proper,
    Open,
    Guarded,
    Introspective,
    SelfDeceiving,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartingSex {
    Female,
    Male,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartingPresentation {
    Man,
    Ambiguous,
    Woman,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartingInclination {
    Men,
    Either,
    Women,
    Neither,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartingPersonality {
    pub traits: Vec<StartingPersonalityTrait>,
    pub sex: StartingSex,
    pub presentation: StartingPresentation,
    pub inclination: StartingInclination,
}

pub fn personality_description(personality: &StartingPersonality) -> String {
    let names: Vec<_> = personality
        .traits
        .iter()
        .map(|value| match value {
            StartingPersonalityTrait::Brave => "brave",
            StartingPersonalityTrait::Fearful => "fearful",
            StartingPersonalityTrait::Ambitious => "ambitious",
            StartingPersonalityTrait::Content => "content",
            StartingPersonalityTrait::Sanguine => "sanguine",
            StartingPersonalityTrait::Brooding => "brooding",
            StartingPersonalityTrait::Gregarious => "gregarious",
            StartingPersonalityTrait::Solitary => "solitary",
            StartingPersonalityTrait::Compassionate => "compassionate",
            StartingPersonalityTrait::Callous => "callous",
            StartingPersonalityTrait::Cruel => "cruel",
            StartingPersonalityTrait::Proud => "proud",
            StartingPersonalityTrait::Humble => "humble",
            StartingPersonalityTrait::Zealous => "zealous",
            StartingPersonalityTrait::Irreverent => "irreverent",
            StartingPersonalityTrait::Slovenly => "slovenly",
            StartingPersonalityTrait::Cleanly => "cleanly",
            StartingPersonalityTrait::Temperate => "temperate",
            StartingPersonalityTrait::Drunkard => "a drunkard",
            StartingPersonalityTrait::Merry => "merry",
            StartingPersonalityTrait::Grave => "grave",
            StartingPersonalityTrait::Amorous => "amorous",
            StartingPersonalityTrait::Proper => "proper",
            StartingPersonalityTrait::Open => "open",
            StartingPersonalityTrait::Guarded => "guarded",
            StartingPersonalityTrait::Introspective => "introspective",
            StartingPersonalityTrait::SelfDeceiving => "self-deceiving",
        })
        .collect();
    match names.as_slice() {
        [] => "Reserved".into(),
        [one] => capitalize(one),
        [left, right] => capitalize(&format!("{left} and {right}")),
        _ => capitalize(&format!(
            "{}, and {}",
            names[..names.len() - 1].join(", "),
            names[names.len() - 1]
        )),
    }
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StartingCharacterSpec {
    pub id: u64,
    pub name: String,
    pub age_years: u16,
    pub background: String,
    pub personality: StartingPersonality,
    pub attributes: StartingAttributes,
    pub skills: StartingSkills,
    pub currency: u32,
    pub settlement_selector: u64,
    pub inventory: Vec<StartingItem>,
    pub age_tier: StartingAgeTier,
    pub profession: Option<StartingProfession>,
    pub organization: Option<StartingOrganization>,
    pub religion_id: Option<String>,
}

pub fn validate_request(
    version: u16,
    seed: &str,
    age_tier: StartingAgeTier,
    slot: u8,
) -> Result<(), &'static str> {
    if version != GENERATOR_VERSION {
        return Err("unsupported candidate generator version");
    }
    if seed.len() != 32
        || !seed
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("candidate seed must be 32 lowercase hexadecimal characters");
    }
    if slot >= age_tier.roster_size() {
        return Err("candidate slot is out of range");
    }
    Ok(())
}

fn tier_hash(domain: &str, seed: &str, age_tier: StartingAgeTier, slot: u8) -> u64 {
    let mut value = hash(domain, seed, slot);
    for byte in age_tier.as_str().bytes() {
        value ^= u64::from(byte);
        value = value.wrapping_mul(0x100000001b3);
    }
    value ^ (value >> 32)
}

fn hash(domain: &str, seed: &str, slot: u8) -> u64 {
    let mut value = 0xcbf29ce484222325_u64;
    for byte in b"adventuresim.starting-character.v4"
        .iter()
        .chain(domain.as_bytes())
        .chain(seed.as_bytes())
        .chain([slot].iter())
    {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(0x100000001b3);
    }
    value ^ (value >> 32)
}

fn choose<'a>(domain: &str, seed: &str, slot: u8, choices: &'a [&'a str]) -> &'a str {
    choices[(hash(domain, seed, slot) % choices.len() as u64) as usize]
}

fn item(id: &str, quantity: u32, equipped: Option<StartingSlot>) -> StartingItem {
    StartingItem {
        item_id: id.into(),
        quantity,
        equipped,
    }
}

/// Generate one of five deliberately differentiated but viable candidates.
pub fn generate(
    version: u16,
    seed: &str,
    age_tier: StartingAgeTier,
    slot: u8,
) -> Result<StartingCharacterSpec, &'static str> {
    validate_request(version, seed, age_tier, slot)?;
    let sex = generated_sex(seed, age_tier, slot);
    let first = choose(
        "first-name",
        seed,
        slot,
        match sex {
            StartingSex::Female => &["Adela", "Beatrix", "Elsbeth", "Greta", "Lina", "Oda"],
            StartingSex::Male => &[
                "Anselm", "Conrad", "Florian", "Hugo", "Matthias", "Ruprecht",
            ],
        },
    );
    let byname = choose(
        "byname",
        seed,
        slot,
        &[
            "Ashbrook",
            "Blackwood",
            "Dawnward",
            "Falken",
            "Greyfield",
            "Hartmann",
            "Ironmere",
            "Rosen",
            "Stoneford",
            "Winter",
        ],
    );
    let (background, weapon, weapon_slot, armor, _primary, defense, currency_base) = match slot {
        0 => (
            "Militia runner",
            "katzbalger",
            StartingSlot::RightHand,
            "arming_doublet",
            "sword",
            "block",
            90,
        ),
        1 => (
            "Woodland hunter",
            "longbow",
            StartingSlot::RightHand,
            "quilted_sleeve",
            "bow",
            "dodge",
            65,
        ),
        2 => (
            "Caravan guard",
            "hunting_spear",
            StartingSlot::RightHand,
            "padded_chausses",
            "polearm",
            "dodge",
            125,
        ),
        3 => (
            "Town watch apprentice",
            "light_crossbow",
            StartingSlot::RightHand,
            "arming_cap",
            "crossbow",
            "block",
            155,
        ),
        _ => (
            "Camp follower turned scout",
            "bauernwehr",
            StartingSlot::RightHand,
            "padded_skirt",
            "knife",
            "dodge",
            105,
        ),
    };
    let variation = |domain: &str| 2.0 + (hash(domain, seed, slot) % 17) as f32 / 10.0;
    let mut inventory = vec![
        item(weapon, 1, Some(weapon_slot)),
        item(
            armor,
            1,
            Some(match armor {
                "arming_doublet" => StartingSlot::Chest,
                "quilted_sleeve" => StartingSlot::LeftArm,
                "padded_chausses" => StartingSlot::LeftLeg,
                "arming_cap" => StartingSlot::Head,
                _ => StartingSlot::Stomach,
            }),
        ),
        item("torch", 1, None),
        item(
            "bandage",
            2 + (hash("bandages", seed, slot) % 3) as u32,
            None,
        ),
    ];
    if defense == "block" {
        inventory.push(item("buckler", 1, Some(StartingSlot::LeftHand)));
    }
    if matches!(weapon, "longbow" | "light_crossbow") {
        inventory.push(item(
            "arrow",
            18 + (hash("arrows", seed, slot) % 13) as u32,
            None,
        ));
    }
    let mut spec = StartingCharacterSpec {
        id: tier_hash("character-id", seed, age_tier, slot) | 0x8000_0000_0000_0000,
        name: format!("{first} {byname}"),
        age_years: age_tier.age_years(),
        background: background.into(),
        personality: generated_personality(seed, age_tier, slot),
        attributes: StartingAttributes {
            endurance: variation("endurance"),
            immunity: variation("immunity"),
            gut: variation("gut"),
            intelligence: variation("intelligence"),
            instinct: variation("instinct"),
            eyesight: variation("eyesight"),
            hearing: variation("hearing"),
            strength: variation("strength"),
            agility: variation("agility"),
        },
        skills: StartingSkills::default(),
        currency: currency_base + (hash("currency", seed, slot) % 61) as u32,
        settlement_selector: hash("settlement", seed, slot),
        inventory,
        age_tier,
        profession: None,
        organization: None,
        religion_id: None,
    };
    if age_tier != StartingAgeTier::Young {
        apply_professional_start(&mut spec, seed, slot)?;
    }
    simulate_starting_life(&mut spec, seed, slot)?;
    Ok(spec)
}

fn generated_sex(seed: &str, tier: StartingAgeTier, slot: u8) -> StartingSex {
    if tier_hash("sex", seed, tier, slot) % 2 == 0 {
        StartingSex::Female
    } else {
        StartingSex::Male
    }
}

fn personality_with_demographics(
    traits: Vec<StartingPersonalityTrait>,
    seed: &str,
    tier: StartingAgeTier,
    slot: u8,
) -> StartingPersonality {
    let sex = generated_sex(seed, tier, slot);
    let presentation = match (sex, tier_hash("presentation", seed, tier, slot) % 100) {
        (_, 0..=3) => StartingPresentation::Ambiguous,
        (StartingSex::Female, 4) => StartingPresentation::Man,
        (StartingSex::Male, 4) => StartingPresentation::Woman,
        (StartingSex::Female, _) => StartingPresentation::Woman,
        (StartingSex::Male, _) => StartingPresentation::Man,
    };
    let inclination = match tier_hash("inclination", seed, tier, slot) % 100 {
        0 => StartingInclination::Neither,
        1..=4 => StartingInclination::Either,
        5..=9 => match sex {
            StartingSex::Female => StartingInclination::Women,
            StartingSex::Male => StartingInclination::Men,
        },
        _ => match sex {
            StartingSex::Female => StartingInclination::Men,
            StartingSex::Male => StartingInclination::Women,
        },
    };
    StartingPersonality {
        traits,
        sex,
        presentation,
        inclination,
    }
}

fn generated_personality(seed: &str, tier: StartingAgeTier, slot: u8) -> StartingPersonality {
    use StartingPersonalityTrait as Trait;
    let axes: &[&[Trait]] = &[
        &[Trait::Brave, Trait::Fearful],
        &[Trait::Ambitious, Trait::Content],
        &[Trait::Sanguine, Trait::Brooding],
        &[Trait::Gregarious, Trait::Solitary],
        &[Trait::Compassionate, Trait::Callous, Trait::Cruel],
        &[Trait::Proud, Trait::Humble],
        &[Trait::Zealous, Trait::Irreverent],
        &[Trait::Slovenly, Trait::Cleanly],
        &[Trait::Temperate, Trait::Drunkard],
        &[Trait::Merry, Trait::Grave],
        &[Trait::Amorous, Trait::Proper],
        &[Trait::Open, Trait::Guarded],
        &[Trait::Introspective, Trait::SelfDeceiving],
    ];
    let mut order: Vec<_> = (0..axes.len()).collect();
    order.sort_by_key(|axis| tier_hash(&format!("personality-axis-{axis}"), seed, tier, slot));
    let count = 2 + (tier_hash("personality-count", seed, tier, slot) % 3) as usize;
    let traits = order
        .into_iter()
        .take(count)
        .map(|axis| {
            let values = axes[axis];
            values[(tier_hash(&format!("personality-value-{axis}"), seed, tier, slot)
                % values.len() as u64) as usize]
        })
        .collect();
    personality_with_demographics(traits, seed, tier, slot)
}

fn fixed_skill_hours(skills: &StartingSkills, skill: &str) -> Option<(Skill, f32)> {
    Some(match skill {
        "will" => (Skill::Will, skills.will),
        "insight" => (Skill::Insight, skills.insight),
        "charm" => (Skill::Charm, skills.charm),
        "command" => (Skill::Command, skills.command),
        "deception" => (Skill::Deception, skills.deception),
        "physiology" => (Skill::Physiology, skills.physiology),
        "cooking" => (Skill::Cooking, skills.cooking),
        "herbalism" => (Skill::Herbalism, skills.herbalism),
        "surgery" => (Skill::Surgery, skills.surgery),
        "polearm" => (Skill::Polearm, skills.polearm),
        "axe" => (Skill::Axe, skills.axe),
        "bludgeon" => (Skill::Bludgeon, skills.bludgeon),
        "sword" => (Skill::Sword, skills.sword),
        "knife" => (Skill::Knife, skills.knife),
        "bow" => (Skill::Bow, skills.bow),
        "crossbow" => (Skill::Crossbow, skills.crossbow),
        "firearm" => (Skill::Firearm, skills.firearm),
        "throw" => (Skill::Throw, skills.throw),
        "block" => (Skill::Block, skills.block),
        "dodge" => (Skill::Dodge, skills.dodge),
        "stealth" => (Skill::Stealth, skills.stealth),
        "balance" => (Skill::Balance, skills.balance),
        "terrain_plains" => (Skill::TerrainPlains, skills.terrain_plains),
        "terrain_forest" => (Skill::TerrainForest, skills.terrain_forest),
        "terrain_hills" => (Skill::TerrainHills, skills.terrain_hills),
        "terrain_wetlands" => (Skill::TerrainWetlands, skills.terrain_wetlands),
        "terrain_urban" => (Skill::TerrainUrban, skills.terrain_urban),
        "terrain_snow" => (Skill::TerrainSnow, skills.terrain_snow),
        "tailoring" => (Skill::Tailoring, skills.tailoring),
        "smithing" => (Skill::Smithing, skills.smithing),
        _ => return None,
    })
}

fn leaf_hours(skills: &StartingSkills, skill: &str, leaf: &str) -> Option<(Skill, f32)> {
    Some(match skill {
        "religion" => (
            Skill::Religion,
            match leaf {
                "roman_catholic" => skills.religion.roman_catholic,
                "lutheran" => skills.religion.lutheran,
                "reformed" => skills.religion.reformed,
                "anglican" => skills.religion.anglican,
                "eastern_orthodox" => skills.religion.eastern_orthodox,
                "islamic" => skills.religion.islamic,
                "judaism" => skills.religion.judaism,
                _ => return None,
            },
        ),
        "bestiary" => (
            Skill::Bestiary,
            match leaf {
                "beast" => skills.bestiary.beast,
                "undead" => skills.bestiary.undead,
                "human" => skills.bestiary.human,
                "werekin" => skills.bestiary.werekin,
                "elf" => skills.bestiary.elf,
                "dwarf" => skills.bestiary.dwarf,
                "fey" => skills.bestiary.fey,
                "spirit" => skills.bestiary.spirit,
                "greenskin" => skills.bestiary.greenskin,
                "insectoid" => skills.bestiary.insectoid,
                "draconid" => skills.bestiary.draconid,
                "construct" => skills.bestiary.construct,
                "wildmen" => skills.bestiary.wildmen,
                _ => return None,
            },
        ),
        _ => return None,
    })
}

fn requirement_met(
    skills: &StartingSkills,
    requirement: &Requirement,
    religion: Option<&str>,
) -> bool {
    match requirement {
        Requirement::ProfessedReligion { religion: required } => {
            religion == Some(required.as_str())
        }
        Requirement::SkillRating {
            skill,
            minimum,
            leaf,
        } => {
            let value = leaf
                .as_deref()
                .and_then(|leaf| leaf_hours(skills, skill, leaf))
                .or_else(|| fixed_skill_hours(skills, skill));
            value.is_some_and(|(skill, hours)| skill.training_rank(hours) >= *minimum)
        }
    }
}

fn starting_activity_profile(
    inventory: &[StartingItem],
) -> crate::strategic_schedule::ActivityTrainingProfile {
    use crate::strategic_schedule::{
        ActivityTrainingProfile, CombatTrainingProfile, EquippedCombatItem,
    };
    let hands = inventory.iter().filter_map(|item| {
        matches!(
            item.equipped,
            Some(StartingSlot::LeftHand | StartingSlot::RightHand)
        )
        .then(|| {
            let definition = crate::item_catalog::definition(&item.item_id);
            let (shield, balance) = definition.map_or((false, 1.0), |definition| match &definition
                .kind
            {
                crate::item_catalog_schema::ItemKind::Shield { .. } => (true, 1.0),
                crate::item_catalog_schema::ItemKind::Weapon { balance, .. } => (false, *balance),
                _ => (false, 1.0),
            });
            EquippedCombatItem {
                weapons: crate::equipment::weapon_skill_distribution_for_item(&item.item_id),
                shield,
                balance,
            }
        })
    });
    ActivityTrainingProfile {
        combat: CombatTrainingProfile::from_equipped_hands(hands),
    }
}

fn simulate_starting_life(
    spec: &mut StartingCharacterSpec,
    seed: &str,
    slot: u8,
) -> Result<(), &'static str> {
    let organization = spec
        .organization
        .as_ref()
        .map(|starting| {
            crate::organization::organization(&starting.organization_id)
                .ok_or("starting organization is not in the catalog")
        })
        .transpose()?;
    let requirements = organization
        .into_iter()
        .flat_map(|definition| {
            let rank_id = &spec
                .organization
                .as_ref()
                .expect("paired organization")
                .rank_id;
            requirements_through_rank(definition, rank_id)
        })
        .cloned()
        .collect::<Vec<_>>();
    let religion = spec
        .religion_id
        .as_deref()
        .and_then(adventuresim_world_schema::OfficialReligion::from_id);
    let result =
        crate::life_simulation::simulate_life(crate::life_simulation::LifeSimulationInput {
            stable_seed: tier_hash("life-simulation", seed, spec.age_tier, slot),
            age_years: spec.age_years,
            attributes: &spec.attributes,
            organization,
            rank_requirements: &requirements,
            religion,
            activity_profile: starting_activity_profile(&spec.inventory),
            native_oral: Default::default(),
            literacy: None,
        });
    spec.skills = StartingSkills::from_life_simulation(result.skills, result.written);
    if !requirements
        .iter()
        .all(|requirement| requirement_met(&spec.skills, requirement, spec.religion_id.as_deref()))
    {
        return Err("simulated professional life does not meet its starting requirements");
    }
    Ok(())
}

impl StartingSkills {
    #[cfg(test)]
    fn as_skill_hours(&self) -> crate::strategic_schedule::SkillHours {
        crate::strategic_schedule::SkillHours {
            polearm: self.polearm,
            axe: self.axe,
            bludgeon: self.bludgeon,
            sword: self.sword,
            knife: self.knife,
            dodge: self.dodge,
            block: self.block,
            bow: self.bow,
            crossbow: self.crossbow,
            firearm: self.firearm,
            throw: self.throw,
            will: self.will,
            insight: self.insight,
            charm: self.charm,
            command: self.command,
            deception: self.deception,
            physiology: self.physiology,
            cooking: self.cooking,
            herbalism: self.herbalism,
            religion: self.religion,
            bestiary: self.bestiary,
            stealth: self.stealth,
            balance: self.balance,
            surgery: self.surgery,
            terrain_plains: self.terrain_plains,
            terrain_forest: self.terrain_forest,
            terrain_hills: self.terrain_hills,
            terrain_wetlands: self.terrain_wetlands,
            terrain_urban: self.terrain_urban,
            terrain_snow: self.terrain_snow,
            tailoring: self.tailoring,
            smithing: self.smithing,
        }
    }

    fn from_life_simulation(
        hours: crate::strategic_schedule::SkillHours,
        written: adventuresim_world_schema::WrittenLanguageHours,
    ) -> Self {
        Self {
            written,
            polearm: hours.polearm,
            axe: hours.axe,
            bludgeon: hours.bludgeon,
            sword: hours.sword,
            knife: hours.knife,
            dodge: hours.dodge,
            block: hours.block,
            bow: hours.bow,
            crossbow: hours.crossbow,
            firearm: hours.firearm,
            throw: hours.throw,
            will: hours.will,
            insight: hours.insight,
            charm: hours.charm,
            command: hours.command,
            deception: hours.deception,
            physiology: hours.physiology,
            bestiary: hours.bestiary,
            surgery: hours.surgery,
            stealth: hours.stealth,
            balance: hours.balance,
            cooking: hours.cooking,
            herbalism: hours.herbalism,
            religion: hours.religion,
            terrain_plains: hours.terrain_plains,
            terrain_forest: hours.terrain_forest,
            terrain_hills: hours.terrain_hills,
            terrain_wetlands: hours.terrain_wetlands,
            terrain_urban: hours.terrain_urban,
            terrain_snow: hours.terrain_snow,
            tailoring: hours.tailoring,
            smithing: hours.smithing,
        }
    }
}

fn required_religion<'a>(
    requirements: impl Iterator<Item = &'a Requirement>,
) -> Result<Option<String>, &'static str> {
    let mut selected: Option<String> = None;
    for religion in requirements.filter_map(|requirement| match requirement {
        Requirement::ProfessedReligion { religion } => Some(religion),
        _ => None,
    }) {
        if selected
            .as_deref()
            .is_some_and(|selected| selected != religion)
        {
            return Err("starting organization rank has conflicting religions");
        }
        selected = Some(religion.clone());
    }
    Ok(selected)
}

fn requirements_through_rank<'a>(
    organization: &'a crate::organization::OrganizationDefinition,
    rank_id: &str,
) -> Vec<&'a Requirement> {
    let mut requirements = organization
        .admission
        .requirements
        .iter()
        .collect::<Vec<_>>();
    for rank in &organization.ranks {
        requirements.extend(rank.requirements.iter());
        if rank.id == rank_id {
            break;
        }
    }
    requirements
}

fn professional_loadout(
    profession: StartingProfession,
    tier: StartingAgeTier,
) -> Vec<StartingItem> {
    let adult = tier == StartingAgeTier::Adult;
    let armor = |id, slot| item(id, 1, Some(slot));
    let held = |id, slot| item(id, 1, Some(slot));
    let supplies = |bandages| vec![item("torch", 1, None), item("bandage", bandages, None)];
    let mut inventory = match profession {
        StartingProfession::Merchant => vec![
            held(
                if adult { "bauernwehr" } else { "rapier" },
                StartingSlot::RightHand,
            ),
            armor(
                if adult {
                    "padded_skirt"
                } else {
                    "arming_doublet"
                },
                if adult {
                    StartingSlot::Stomach
                } else {
                    StartingSlot::Chest
                },
            ),
        ],
        StartingProfession::Weaponsmith => vec![
            held("war_hammer", StartingSlot::RightHand),
            armor(
                if adult {
                    "arming_doublet"
                } else {
                    "brigandine"
                },
                StartingSlot::Chest,
            ),
        ],
        StartingProfession::Armourer => vec![
            held(
                if adult { "flanged_mace" } else { "war_hammer" },
                StartingSlot::RightHand,
            ),
            armor(
                if adult {
                    "jack_of_plates"
                } else {
                    "breastplate"
                },
                StartingSlot::Chest,
            ),
        ],
        StartingProfession::Tailor => vec![
            held(
                if adult { "utility_knife" } else { "baselard" },
                StartingSlot::RightHand,
            ),
            armor("arming_doublet", StartingSlot::Chest),
        ],
        StartingProfession::Herbalist => vec![
            held("walking_staff", StartingSlot::RightHand),
            armor(
                if adult {
                    "padded_skirt"
                } else {
                    "arming_doublet"
                },
                if adult {
                    StartingSlot::Stomach
                } else {
                    StartingSlot::Chest
                },
            ),
        ],
        StartingProfession::Cook => vec![
            held(
                if adult { "utility_knife" } else { "bauernwehr" },
                StartingSlot::RightHand,
            ),
            armor(
                if adult {
                    "padded_skirt"
                } else {
                    "arming_doublet"
                },
                if adult {
                    StartingSlot::Stomach
                } else {
                    StartingSlot::Chest
                },
            ),
        ],
        StartingProfession::LearnedReligiousPractitioner => vec![
            held(
                if adult { "walking_staff" } else { "baselard" },
                StartingSlot::RightHand,
            ),
            armor(
                if adult {
                    "arming_cap"
                } else {
                    "arming_doublet"
                },
                if adult {
                    StartingSlot::Head
                } else {
                    StartingSlot::Chest
                },
            ),
        ],
        StartingProfession::WitchHunter => vec![
            held(
                if adult {
                    "light_crossbow"
                } else {
                    "heavy_crossbow"
                },
                StartingSlot::RightHand,
            ),
            armor(
                if adult {
                    "arming_doublet"
                } else {
                    "brigandine"
                },
                StartingSlot::Chest,
            ),
            item("arrow", if adult { 24 } else { 40 }, None),
            item("bauernwehr", 1, None),
        ],
        StartingProfession::Knight => {
            let mut kit = vec![
                held("arming_sword", StartingSlot::RightHand),
                held(
                    if adult { "buckler" } else { "heater_shield" },
                    StartingSlot::LeftHand,
                ),
                armor(
                    if adult { "brigandine" } else { "breastplate" },
                    StartingSlot::Chest,
                ),
                armor(
                    if adult { "sallet" } else { "visored_sallet" },
                    StartingSlot::Head,
                ),
            ];
            if !adult {
                kit.push(armor("vambrace", StartingSlot::LeftArm));
                kit.push(armor("greave", StartingSlot::LeftLeg));
            }
            kit
        }
        StartingProfession::Forester => vec![
            held("longbow", StartingSlot::RightHand),
            armor(
                if adult {
                    "quilted_sleeve"
                } else {
                    "arming_doublet"
                },
                if adult {
                    StartingSlot::LeftArm
                } else {
                    StartingSlot::Chest
                },
            ),
            item("arrow", if adult { 28 } else { 44 }, None),
            item("utility_knife", 1, None),
        ],
    };
    inventory.extend(supplies(match (profession, adult) {
        (StartingProfession::Herbalist, true) => 7,
        (StartingProfession::Herbalist, false) => 12,
        (_, true) => 3,
        (_, false) => 5,
    }));
    inventory
}

fn professional_personality(
    profession: StartingProfession,
    tier: StartingAgeTier,
    seed: &str,
    slot: u8,
) -> StartingPersonality {
    use StartingPersonalityTrait as Trait;
    let (first, alternate_a, alternate_b, veteran) = match profession {
        StartingProfession::Merchant => (
            Trait::Ambitious,
            Trait::Gregarious,
            Trait::Cleanly,
            Trait::Proud,
        ),
        StartingProfession::Weaponsmith => (
            Trait::Proud,
            Trait::Cleanly,
            Trait::Solitary,
            Trait::Ambitious,
        ),
        StartingProfession::Armourer => (
            Trait::Content,
            Trait::Cleanly,
            Trait::Solitary,
            Trait::Proud,
        ),
        StartingProfession::Tailor => (
            Trait::Gregarious,
            Trait::Ambitious,
            Trait::Sanguine,
            Trait::Cleanly,
        ),
        StartingProfession::Herbalist => (
            Trait::Compassionate,
            Trait::Cleanly,
            Trait::Solitary,
            Trait::Humble,
        ),
        StartingProfession::Cook => (
            Trait::Sanguine,
            Trait::Gregarious,
            Trait::Content,
            Trait::Temperate,
        ),
        StartingProfession::LearnedReligiousPractitioner => (
            Trait::Zealous,
            Trait::Humble,
            Trait::Compassionate,
            Trait::Cleanly,
        ),
        StartingProfession::WitchHunter => (
            Trait::Brave,
            Trait::Solitary,
            Trait::Brooding,
            Trait::Zealous,
        ),
        StartingProfession::Knight => (
            Trait::Brave,
            Trait::Proud,
            Trait::Gregarious,
            Trait::Ambitious,
        ),
        StartingProfession::Forester => (
            Trait::Solitary,
            Trait::Sanguine,
            Trait::Content,
            Trait::Humble,
        ),
    };
    let mut traits = vec![
        first,
        if tier_hash("professional-personality", seed, tier, slot) % 2 == 0 {
            alternate_a
        } else {
            alternate_b
        },
    ];
    if tier == StartingAgeTier::Old {
        traits.push(veteran);
    }
    personality_with_demographics(traits, seed, tier, slot)
}

fn raise_governing_attributes_for_requirement(attributes: &mut StartingAttributes, skill: Skill) {
    let governing = skill.governing_aptitudes();
    if governing.intelligence_percent() > 0 {
        attributes.intelligence = 5.0;
    }
    if governing.instinct_percent() > 0 {
        attributes.instinct = 5.0;
    }
    if governing.agility_percent() > 0 {
        attributes.agility = 5.0;
    }
}

fn apply_professional_start(
    spec: &mut StartingCharacterSpec,
    seed: &str,
    slot: u8,
) -> Result<(), &'static str> {
    let profession = StartingProfession::ALL[slot as usize];
    let eligible = catalog()
        .organizations
        .iter()
        .filter(|organization| {
            organization
                .starting_role
                .as_ref()
                .is_some_and(|role| role.profession == profession)
        })
        .collect::<Vec<_>>();
    if eligible.is_empty() {
        return Err("profession has no eligible starting organization");
    }
    let organization = eligible
        [(tier_hash("organization", seed, spec.age_tier, slot) % eligible.len() as u64) as usize];
    let role = organization
        .starting_role
        .as_ref()
        .expect("filtered starting role");
    let rank_id = match spec.age_tier {
        StartingAgeTier::Adult => &role.adult_rank_id,
        StartingAgeTier::Old => &role.old_rank_id,
        StartingAgeTier::Young => return Err("young characters cannot have a profession"),
    };
    let rank = organization
        .rank(rank_id)
        .ok_or("starting organization rank is missing")?;
    let starting_requirements = requirements_through_rank(organization, rank_id);
    let religion_id = required_religion(starting_requirements.iter().copied())?;
    let adult = spec.age_tier == StartingAgeTier::Adult;
    let purse_base = match profession {
        StartingProfession::Merchant => 360,
        StartingProfession::Weaponsmith | StartingProfession::Armourer => 180,
        StartingProfession::Tailor => 150,
        StartingProfession::Herbalist => 170,
        StartingProfession::Cook => 135,
        StartingProfession::LearnedReligiousPractitioner => 110,
        StartingProfession::WitchHunter => 210,
        StartingProfession::Knight => 260,
        StartingProfession::Forester => 145,
    };
    spec.currency = if adult {
        purse_base
    } else {
        purse_base * 2 + 160
    } + (tier_hash("professional-purse", seed, spec.age_tier, slot)
        % if adult { 61 } else { 141 }) as u32;
    spec.inventory = professional_loadout(profession, spec.age_tier);
    spec.personality = professional_personality(profession, spec.age_tier, seed, slot);
    let base_attribute =
        |domain: &str| 2.0 + (tier_hash(domain, seed, spec.age_tier, slot) % 13) as f32 / 10.0;
    spec.attributes = StartingAttributes {
        endurance: base_attribute("professional-endurance"),
        immunity: base_attribute("professional-immunity"),
        gut: base_attribute("professional-gut"),
        intelligence: base_attribute("professional-intelligence") + if adult { 0.0 } else { 0.4 },
        instinct: base_attribute("professional-instinct") + if adult { 0.0 } else { 0.3 },
        eyesight: base_attribute("professional-eyesight"),
        hearing: base_attribute("professional-hearing"),
        strength: base_attribute("professional-strength")
            + if matches!(
                profession,
                StartingProfession::Weaponsmith
                    | StartingProfession::Armourer
                    | StartingProfession::Knight
            ) {
                0.5
            } else {
                0.0
            },
        agility: base_attribute("professional-agility")
            + if matches!(
                profession,
                StartingProfession::Tailor | StartingProfession::Forester
            ) {
                0.4
            } else {
                0.0
            },
    };
    // Credentials are never patched onto the skill projection. Raise the
    // underlying aptitudes that govern authored requirements, then let the
    // life simulator earn every hour through curricula under normal caps.
    for skill in starting_requirements
        .iter()
        .copied()
        .filter_map(|requirement| match requirement {
            Requirement::SkillRating { skill, .. } => Skill::from_training_id(skill),
            Requirement::ProfessedReligion { .. } => None,
        })
    {
        raise_governing_attributes_for_requirement(&mut spec.attributes, skill);
    }
    spec.background = format!(
        "{} {} of {}",
        if adult { "Newly qualified" } else { "Veteran" },
        rank.name,
        organization.name
    );
    spec.profession = Some(profession);
    spec.organization = Some(StartingOrganization {
        organization_id: organization.id.clone(),
        organization_name: organization.name.clone(),
        rank_id: rank.id.clone(),
        rank_name: rank.name.clone(),
    });
    spec.religion_id = religion_id;
    spec.settlement_selector = tier_hash("settlement", seed, spec.age_tier, slot);
    Ok(())
}

pub fn roster(
    version: u16,
    seed: &str,
    age_tier: StartingAgeTier,
) -> Result<Vec<StartingCharacterSpec>, &'static str> {
    (0..age_tier.roster_size())
        .map(|slot| generate(version, seed, age_tier, slot))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    const SEED: &str = "00112233445566778899aabbccddeeff";
    #[test]
    fn fixture_is_stable() {
        let c = generate(GENERATOR_VERSION, SEED, StartingAgeTier::Young, 0).unwrap();
        assert!(!c.name.is_empty());
        assert_eq!(c.age_years, 16);
        assert_eq!(c.age_tier, StartingAgeTier::Young);
    }
    #[test]
    fn roster_is_viable_and_diverse() {
        let r = roster(GENERATOR_VERSION, SEED, StartingAgeTier::Young).unwrap();
        assert_eq!(r.len(), 5);
        assert!(r.iter().all(|c| {
            c.age_years == 16
                && c.currency >= 65
                && c.inventory
                    .iter()
                    .any(|i| i.equipped == Some(StartingSlot::RightHand))
        }));
        let ids: std::collections::HashSet<_> = r.iter().map(|c| c.id).collect();
        assert_eq!(ids.len(), 5);
    }
    #[test]
    fn ranged_candidates_have_ammunition_and_personality_copy_is_derived() {
        let roster = roster(GENERATOR_VERSION, SEED, StartingAgeTier::Young).unwrap();
        for candidate in &roster {
            let ranged = candidate
                .inventory
                .iter()
                .any(|item| matches!(item.item_id.as_str(), "longbow" | "light_crossbow"));
            if ranged {
                assert!(
                    candidate
                        .inventory
                        .iter()
                        .any(|item| item.item_id == "arrow" && item.quantity >= 18)
                );
            }
            let description = personality_description(&candidate.personality);
            assert!(!description.is_empty());
            assert_eq!(description.matches(" and ").count(), 1);
        }
    }
    #[test]
    fn tier_matrix_has_fixed_ages_and_profession_families() {
        let young = roster(GENERATOR_VERSION, SEED, StartingAgeTier::Young).unwrap();
        assert!(young.iter().all(|candidate| candidate.profession.is_none()));
        for (tier, age) in [(StartingAgeTier::Adult, 22), (StartingAgeTier::Old, 40)] {
            let roster = roster(GENERATOR_VERSION, SEED, tier).unwrap();
            assert_eq!(roster.len(), 10);
            assert_eq!(
                roster
                    .iter()
                    .map(|candidate| candidate.profession.unwrap())
                    .collect::<Vec<_>>(),
                StartingProfession::ALL.to_vec()
            );
            assert!(roster.iter().all(|candidate| {
                candidate.age_years == age && candidate.organization.is_some()
            }));
        }
        let adult = roster(GENERATOR_VERSION, SEED, StartingAgeTier::Adult).unwrap();
        let old = roster(GENERATOR_VERSION, SEED, StartingAgeTier::Old).unwrap();
        for (profession, id, name, adult_rank, old_rank) in [
            (
                StartingProfession::WitchHunter,
                "hunt_pale_lantern",
                "The Hunt of the Pale Lantern",
                "hunter",
                "huntmaster",
            ),
            (
                StartingProfession::Knight,
                "order_saint_george",
                "The Order of St. George",
                "knight",
                "commander",
            ),
            (
                StartingProfession::Forester,
                "lodge_hart_king",
                "The Lodge of the Hart King",
                "warden",
                "master",
            ),
        ] {
            let slot = StartingProfession::ALL
                .iter()
                .position(|candidate| *candidate == profession)
                .unwrap();
            let adult_organization = adult[slot].organization.as_ref().unwrap();
            let old_organization = old[slot].organization.as_ref().unwrap();
            assert_eq!(adult_organization.organization_id, id);
            assert_eq!(adult_organization.organization_name, name);
            assert_eq!(adult_organization.rank_id, adult_rank);
            assert_eq!(old_organization.organization_id, id);
            assert_eq!(old_organization.organization_name, name);
            assert_eq!(old_organization.rank_id, old_rank);
            assert!(adult[slot].religion_id.is_none());
            assert!(old[slot].religion_id.is_none());
        }
        assert!(adult.iter().zip(&old).all(|(adult, old)| {
            adult.id != old.id
                && adult.organization.as_ref().unwrap().rank_id
                    != old.organization.as_ref().unwrap().rank_id
        }));
        assert_eq!(
            adult,
            roster(GENERATOR_VERSION, SEED, StartingAgeTier::Adult).unwrap(),
            "the same authoritative coordinates must regenerate exactly"
        );
        assert!(adult.iter().all(|candidate| {
            let organization = candidate.organization.as_ref().unwrap();
            !organization.organization_id.starts_with("test_")
                && !candidate.inventory.is_empty()
                && candidate.currency > 0
        }));
        for candidate in adult.iter().chain(&old) {
            let organization = crate::organization::organization(
                &candidate.organization.as_ref().unwrap().organization_id,
            )
            .unwrap();
            let rank = organization
                .rank(&candidate.organization.as_ref().unwrap().rank_id)
                .unwrap();
            assert!(
                requirements_through_rank(organization, &rank.id)
                    .into_iter()
                    .all(|requirement| requirement_met(
                        &candidate.skills,
                        requirement,
                        candidate.religion_id.as_deref()
                    ))
            );
            assert!(candidate.skills.as_skill_hours().is_finite());
        }
        for (slot, profession) in StartingProfession::ALL.into_iter().enumerate() {
            let adult = &adult[slot];
            let old = &old[slot];
            assert_eq!(adult.profession, Some(profession));
            assert!(old.currency > adult.currency);
            assert_ne!(old.inventory, adult.inventory);
            assert_ne!(old.attributes, adult.attributes);
            assert!(
                old.skills
                    .as_skill_hours()
                    .values()
                    .into_iter()
                    .sum::<f32>()
                    > adult
                        .skills
                        .as_skill_hours()
                        .values()
                        .into_iter()
                        .sum::<f32>()
            );
            let adult_right = adult
                .inventory
                .iter()
                .find(|item| item.equipped == Some(StartingSlot::RightHand))
                .map(|item| item.item_id.as_str());
            let old_right = old
                .inventory
                .iter()
                .find(|item| item.equipped == Some(StartingSlot::RightHand))
                .map(|item| item.item_id.as_str());
            let expected = match profession {
                StartingProfession::Merchant => ("bauernwehr", "rapier"),
                StartingProfession::Weaponsmith => ("war_hammer", "war_hammer"),
                StartingProfession::Armourer => ("flanged_mace", "war_hammer"),
                StartingProfession::Tailor => ("utility_knife", "baselard"),
                StartingProfession::Herbalist => ("walking_staff", "walking_staff"),
                StartingProfession::Cook => ("utility_knife", "bauernwehr"),
                StartingProfession::LearnedReligiousPractitioner => ("walking_staff", "baselard"),
                StartingProfession::WitchHunter => ("light_crossbow", "heavy_crossbow"),
                StartingProfession::Knight => ("arming_sword", "arming_sword"),
                StartingProfession::Forester => ("longbow", "longbow"),
            };
            assert_eq!(adult_right, Some(expected.0));
            assert_eq!(old_right, Some(expected.1));
            assert!(old.personality.traits.len() > adult.personality.traits.len());
        }
        let alternate = roster(
            GENERATOR_VERSION,
            "ffeeddccbbaa99887766554433221100",
            StartingAgeTier::Adult,
        )
        .unwrap();
        assert!(adult.iter().zip(alternate).any(|(left, right)| {
            left.currency != right.currency
                || left.attributes != right.attributes
                || left.personality != right.personality
        }));
    }

    #[test]
    fn profession_of_faith_includes_rank_requirements_and_rejects_conflicts() {
        let admission = [Requirement::SkillRating {
            skill: "religion".into(),
            minimum: 1.0,
            leaf: Some("lutheran".into()),
        }];
        let rank = [Requirement::ProfessedReligion {
            religion: "lutheran".into(),
        }];
        assert_eq!(
            required_religion(admission.iter().chain(rank.iter())).unwrap(),
            Some("lutheran".into())
        );
        let conflict = [Requirement::ProfessedReligion {
            religion: "roman_catholic".into(),
        }];
        assert!(required_religion(rank.iter().chain(conflict.iter())).is_err());
    }

    #[test]
    fn rejects_untrusted_coordinates() {
        assert!(generate(1, SEED, StartingAgeTier::Young, 0).is_err());
        assert!(generate(3, SEED, StartingAgeTier::Young, 0).is_err());
        assert!(generate(GENERATOR_VERSION, "ABC", StartingAgeTier::Young, 0).is_err());
        assert!(generate(GENERATOR_VERSION, SEED, StartingAgeTier::Young, 5).is_err());
        assert!(generate(GENERATOR_VERSION, SEED, StartingAgeTier::Adult, 10).is_err());
    }

    #[test]
    fn intelligence_requirement_preserves_unrelated_attribute_variation() {
        let slot = StartingProfession::ALL
            .iter()
            .position(|profession| *profession == StartingProfession::Herbalist)
            .unwrap() as u8;
        let candidate = generate(GENERATOR_VERSION, SEED, StartingAgeTier::Adult, slot).unwrap();
        assert_eq!(candidate.attributes.intelligence, 5.0);
        let expected_instinct = 2.0
            + (tier_hash("professional-instinct", SEED, StartingAgeTier::Adult, slot) % 13) as f32
                / 10.0;
        let expected_agility = 2.0
            + (tier_hash("professional-agility", SEED, StartingAgeTier::Adult, slot) % 13) as f32
                / 10.0;
        assert_eq!(candidate.attributes.instinct, expected_instinct);
        assert_eq!(candidate.attributes.agility, expected_agility);
    }

    #[test]
    fn hybrid_skill_requirement_raises_every_governing_component() {
        let mut attributes = StartingAttributes {
            endurance: 1.1,
            immunity: 1.2,
            gut: 1.3,
            intelligence: 1.4,
            instinct: 2.6,
            eyesight: 1.5,
            hearing: 1.6,
            strength: 1.7,
            agility: 3.8,
        };
        raise_governing_attributes_for_requirement(&mut attributes, Skill::Surgery);
        assert_eq!(attributes.intelligence, 5.0);
        assert_eq!(attributes.instinct, 5.0);
        assert_eq!(attributes.agility, 5.0);
        assert_eq!(attributes.endurance, 1.1);
        assert_eq!(attributes.strength, 1.7);
    }

    #[test]
    fn generated_loadouts_use_canonical_weapon_profiles() {
        for tier in StartingAgeTier::ALL {
            for candidate in roster(GENERATOR_VERSION, SEED, tier).unwrap() {
                let profile = starting_activity_profile(&candidate.inventory);
                let mut expected = crate::equipment::WeaponSkillDistribution::default();
                for item in candidate.inventory.iter().filter(|item| {
                    matches!(
                        item.equipped,
                        Some(StartingSlot::LeftHand | StartingSlot::RightHand)
                    )
                }) {
                    let canonical =
                        crate::equipment::weapon_skill_distribution_for_item(&item.item_id);
                    for (target, weight) in [
                        &mut expected.polearm,
                        &mut expected.axe,
                        &mut expected.bludgeon,
                        &mut expected.sword,
                        &mut expected.knife,
                        &mut expected.bow,
                        &mut expected.crossbow,
                        &mut expected.firearm,
                        &mut expected.throw,
                    ]
                    .into_iter()
                    .zip(canonical.weights())
                    {
                        *target = target.max(weight);
                    }
                }
                assert_eq!(profile.combat.weapons, expected);
            }
        }
    }

    #[test]
    fn demographics_are_weighted_and_names_match_sex() {
        let mut same = 0;
        let mut either = 0;
        let mut neither = 0;
        let mut ambiguous = 0;
        for n in 0..2_000 {
            let c = generate(
                GENERATOR_VERSION,
                &format!("{n:032x}"),
                StartingAgeTier::Young,
                (n % 5) as u8,
            )
            .unwrap();
            assert!((2..=4).contains(&c.personality.traits.len()));
            ambiguous += usize::from(c.personality.presentation == StartingPresentation::Ambiguous);
            either += usize::from(c.personality.inclination == StartingInclination::Either);
            neither += usize::from(c.personality.inclination == StartingInclination::Neither);
            same += usize::from(matches!(
                (c.personality.sex, c.personality.inclination),
                (StartingSex::Female, StartingInclination::Women)
                    | (StartingSex::Male, StartingInclination::Men)
            ));
        }
        assert!((40..140).contains(&ambiguous));
        assert!((40..140).contains(&either));
        assert!((5..50).contains(&neither));
        assert!((50..170).contains(&same));
    }
}
