//! Pure, versioned generation for the first-character candidate roster.

use serde::{Deserialize, Serialize};

pub const GENERATOR_VERSION: u16 = 1;
pub const ROSTER_SIZE: u8 = 5;

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
    pub precision: f32,
    pub intelligence: f32,
    pub instinct: f32,
    pub eyesight: f32,
    pub hearing: f32,
    pub strength: f32,
    pub agility: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StartingSkills {
    pub polearm: f32,
    pub axe: f32,
    pub bludgeon: f32,
    pub sword: f32,
    pub knife: f32,
    pub dodge: f32,
    pub block: f32,
    pub bow: f32,
    pub crossbow: f32,
    pub throw: f32,
    pub will: f32,
    pub insight: f32,
    pub command: f32,
    pub medicine: f32,
    pub stealth: f32,
    pub balance: f32,
    pub cooking: f32,
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
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartingPersonality {
    pub traits: Vec<StartingPersonalityTrait>,
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
}

pub fn validate_request(version: u16, seed: &str, slot: u8) -> Result<(), &'static str> {
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
    if slot >= ROSTER_SIZE {
        return Err("candidate slot is out of range");
    }
    Ok(())
}

fn hash(domain: &str, seed: &str, slot: u8) -> u64 {
    let mut value = 0xcbf29ce484222325_u64;
    for byte in b"adventuresim.starting-character.v1"
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
pub fn generate(version: u16, seed: &str, slot: u8) -> Result<StartingCharacterSpec, &'static str> {
    validate_request(version, seed, slot)?;
    let first = choose(
        "first-name",
        seed,
        slot,
        &[
            "Adela", "Anselm", "Beatrix", "Conrad", "Elsbeth", "Florian", "Greta", "Hugo", "Lina",
            "Matthias", "Oda", "Ruprecht",
        ],
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
    use StartingPersonalityTrait as Trait;
    let (background, personality, weapon, weapon_slot, armor, primary, defense, currency_base) =
        match slot {
            0 => (
                "Militia runner",
                vec![Trait::Brave, Trait::Gregarious],
                "katzbalger",
                StartingSlot::RightHand,
                "arming_doublet",
                "sword",
                "block",
                90,
            ),
            1 => (
                "Woodland hunter",
                vec![Trait::Solitary, Trait::Sanguine],
                "longbow",
                StartingSlot::RightHand,
                "quilted_sleeve",
                "bow",
                "dodge",
                65,
            ),
            2 => (
                "Caravan guard",
                vec![Trait::Content, Trait::Compassionate],
                "hunting_spear",
                StartingSlot::RightHand,
                "padded_chausses",
                "polearm",
                "dodge",
                125,
            ),
            3 => (
                "Town watch apprentice",
                vec![Trait::Ambitious, Trait::Proud],
                "light_crossbow",
                StartingSlot::RightHand,
                "arming_cap",
                "crossbow",
                "block",
                155,
            ),
            _ => (
                "Camp follower turned scout",
                vec![Trait::Sanguine, Trait::Humble],
                "bauernwehr",
                StartingSlot::RightHand,
                "padded_skirt",
                "knife",
                "dodge",
                105,
            ),
        };
    let primary_hours = 2600.0 + (hash("training", seed, slot) % 1800) as f32;
    let defense_hours = 1400.0 + (hash("defense", seed, slot) % 1400) as f32;
    let mut skills = StartingSkills {
        polearm: 120.0,
        axe: 120.0,
        bludgeon: 120.0,
        sword: 120.0,
        knife: 300.0,
        dodge: 600.0,
        block: 500.0,
        bow: 120.0,
        crossbow: 120.0,
        throw: 250.0,
        will: 700.0,
        insight: 500.0,
        command: 250.0,
        medicine: 250.0,
        stealth: 450.0,
        balance: 600.0,
        cooking: 300.0,
    };
    match primary {
        "sword" => skills.sword = primary_hours,
        "bow" => skills.bow = primary_hours,
        "polearm" => skills.polearm = primary_hours,
        "crossbow" => skills.crossbow = primary_hours,
        _ => skills.knife = primary_hours,
    }
    if defense == "block" {
        skills.block = defense_hours
    } else {
        skills.dodge = defense_hours
    }
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
    Ok(StartingCharacterSpec {
        id: hash("character-id", seed, slot) | 0x8000_0000_0000_0000,
        name: format!("{first} {byname}"),
        age_years: 16 + (hash("age", seed, slot) % 19) as u16,
        background: background.into(),
        personality: StartingPersonality {
            traits: personality,
        },
        attributes: StartingAttributes {
            endurance: variation("endurance"),
            immunity: variation("immunity"),
            gut: variation("gut"),
            precision: variation("precision"),
            intelligence: variation("intelligence"),
            instinct: variation("instinct"),
            eyesight: variation("eyesight"),
            hearing: variation("hearing"),
            strength: variation("strength"),
            agility: variation("agility"),
        },
        skills,
        currency: currency_base + (hash("currency", seed, slot) % 61) as u32,
        settlement_selector: hash("settlement", seed, slot),
        inventory,
    })
}

pub fn roster(version: u16, seed: &str) -> Result<Vec<StartingCharacterSpec>, &'static str> {
    (0..ROSTER_SIZE)
        .map(|slot| generate(version, seed, slot))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    const SEED: &str = "00112233445566778899aabbccddeeff";
    #[test]
    fn fixture_is_stable() {
        let c = generate(1, SEED, 0).unwrap();
        assert_eq!(
            (c.id, c.name.as_str(), c.age_years),
            (16765322260560672214, "Matthias Blackwood", 32)
        );
    }
    #[test]
    fn roster_is_viable_and_diverse() {
        let r = roster(1, SEED).unwrap();
        assert_eq!(r.len(), 5);
        assert!(r.iter().all(|c| {
            c.age_years >= 16
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
        let roster = roster(1, SEED).unwrap();
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
    fn ages_are_young_biased_and_bounded() {
        let ages: Vec<_> = (0..500)
            .map(|n| {
                generate(1, &format!("{n:032x}"), (n % 5) as u8)
                    .unwrap()
                    .age_years
            })
            .collect();
        assert!(ages.iter().all(|a| (16..=34).contains(a)));
        assert!(
            (ages.iter().map(|a| u64::from(*a)).sum::<u64>() as f64 / ages.len() as f64) < 26.0
        );
    }
    #[test]
    fn rejects_untrusted_coordinates() {
        assert!(generate(2, SEED, 0).is_err());
        assert!(generate(1, "ABC", 0).is_err());
        assert!(generate(1, SEED, 5).is_err());
    }
}
