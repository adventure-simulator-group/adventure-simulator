//! Strict, build-time compiled definitions for goal-neutral road and rest encounters.
//!
//! Content declares presentation and closed, typed intentions. Strategic reducers
//! remain the sole authority for checks and state mutation.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::OnceLock};

#[cfg(runtime_catalog)]
include!(concat!(env!("OUT_DIR"), "/road_encounter_catalog.rs"));

pub const CATALOG_REVISION: u32 = 1;
pub const MAX_WEIGHT: u16 = 10_000;
pub const MAX_CHOICES: usize = 8;
pub const MAX_TEXT_BYTES: usize = 2_048;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogDocument {
    pub encounters: Vec<EncounterDefinition>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EncounterDefinition {
    pub id: String,
    pub version: u32,
    pub weight: u16,
    pub triggers: TriggerEligibility,
    pub provenance: Provenance,
    pub cast: Vec<Speaker>,
    pub opening: Vec<SpokenLine>,
    pub choices: Vec<EncounterChoice>,
    #[serde(default)]
    pub quest_reward_eligibility: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerEligibility {
    pub travel: bool,
    pub rest: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub works: Vec<String>,
    pub motifs: Vec<String>,
    pub adaptation_note: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Speaker {
    pub id: String,
    pub name: String,
    pub nature: SpeakerNature,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerNature {
    Mortal,
    Supernatural,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SpokenLine {
    pub speaker: String,
    pub text: String,
    pub reviewed_shakespearean: bool,
    #[serde(default)]
    pub reviewed_iambic_pentameter: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EncounterChoice {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub response: Vec<SpokenLine>,
    pub result: String,
    pub deed: String,
    pub outcome_tags: Vec<String>,
    #[serde(default)]
    pub requirements: Vec<Requirement>,
    #[serde(default)]
    pub checks: Vec<Check>,
    #[serde(default)]
    pub effects: Vec<Effect>,
    #[serde(default)]
    pub personality: Vec<PersonalityDevelopment>,
    #[serde(default)]
    pub quest_reward_tags: Vec<String>,
    #[serde(default)]
    pub transition: Option<EncounterTransition>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EncounterTransition {
    Noop,
    Combat,
    TravelDelay,
    NpcState,
    CaptiveState,
    Injury,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EncounterPresentation {
    pub opening: Vec<PresentationLine>,
    pub choices: Vec<PresentationChoice>,
    pub response: Vec<PresentationLine>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationLine {
    pub speaker_name: String,
    pub text: String,
    pub supernatural: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationChoice {
    pub id: String,
    pub label: String,
    pub available: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Requirement {
    Skill {
        skill: SkillId,
        minimum_hours: u32,
    },
    Religion {
        religion: ReligionId,
    },
    Item {
        item_id: String,
        minimum_quantity: u16,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Check {
    Skill {
        skill: SkillId,
        difficulty_milli: u16,
    },
    Religion {
        religion: ReligionId,
        difficulty_milli: u16,
    },
    Attribute {
        attribute: AttributeId,
        difficulty_milli: u16,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    GrantItem { item_id: String, quantity: u16 },
    ConsumeItem { item_id: String, quantity: u16 },
    Currency { currency_id: String, amount: i32 },
    Information { information_id: String },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct PersonalityDevelopment {
    pub axis: PersonalityAxisId,
    pub delta: i16,
    pub virtue: VirtueId,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SkillId {
    Will,
    Insight,
    Charm,
    Command,
    Deception,
    Physiology,
    Stealth,
    TerrainPlains,
    TerrainForest,
    TerrainHills,
    TerrainWetlands,
    Surgery,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ReligionId {
    RomanCatholic,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AttributeId {
    Endurance,
    Immunity,
    Gut,
    Intelligence,
    Instinct,
    Eyesight,
    Hearing,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum PersonalityAxisId {
    Nerve,
    Drive,
    Sociability,
    Conscience,
    SelfRegard,
    Conviction,
    Courtship,
    Transparency,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum VirtueId {
    Courage,
    Mercy,
    Faith,
    Justice,
    Courtesy,
    Loyalty,
    Prudence,
    Honesty,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EncounterSource {
    pub id: String,
    pub file: String,
    pub line: u32,
}

pub fn validate_definitions(definitions: &[EncounterDefinition]) -> Result<(), String> {
    let mut ids = BTreeSet::new();
    for definition in definitions {
        validate_id(&definition.id, "encounter.id")?;
        if !ids.insert(definition.id.as_str()) {
            return Err(format!("duplicate encounter ID {}", definition.id));
        }
        if definition.version == 0 {
            return Err(format!("{}: version must be positive", definition.id));
        }
        if definition.weight == 0 || definition.weight > MAX_WEIGHT {
            return Err(format!(
                "{}: weight outside 1..={MAX_WEIGHT}",
                definition.id
            ));
        }
        if !definition.triggers.travel && !definition.triggers.rest {
            return Err(format!("{}: no eligible trigger", definition.id));
        }
        if definition.cast.is_empty()
            && (!definition.opening.is_empty()
                || definition
                    .choices
                    .iter()
                    .any(|choice| !choice.response.is_empty()))
        {
            return Err(format!("{}: cast is empty", definition.id));
        }
        let mut speakers = BTreeSet::new();
        for speaker in &definition.cast {
            validate_id(&speaker.id, "speaker.id")?;
            if speaker.name.trim().is_empty() || !speakers.insert(speaker.id.as_str()) {
                return Err(format!("{}: empty or duplicate speaker", definition.id));
            }
        }
        validate_lines(&definition.id, &definition.cast, &definition.opening)?;
        if !(3..=MAX_CHOICES).contains(&definition.choices.len()) {
            return Err(format!(
                "{}: expected 2+ resolutions and ignore",
                definition.id
            ));
        }
        let mut choices = BTreeSet::new();
        let mut material_routes = BTreeSet::new();
        for choice in &definition.choices {
            validate_id(&choice.id, "choice.id")?;
            if choice.label.trim().is_empty()
                || choice.result.trim().is_empty()
                || choice.deed.trim().is_empty()
                || !choices.insert(choice.id.as_str())
            {
                return Err(format!("{}: empty or duplicate choice", definition.id));
            }
            validate_goal_neutral(&choice.label)?;
            validate_goal_neutral(&choice.result)?;
            validate_lines(&definition.id, &definition.cast, &choice.response)?;
            for requirement in &choice.requirements {
                match requirement {
                    Requirement::Skill { minimum_hours, .. } if *minimum_hours == 0 => {
                        return Err(format!(
                            "{}:{} has a zero skill requirement",
                            definition.id, choice.id
                        ));
                    }
                    Requirement::Item {
                        item_id,
                        minimum_quantity,
                    } => {
                        validate_id(item_id, "requirement.item_id")?;
                        if *minimum_quantity == 0 {
                            return Err(format!(
                                "{}:{} has a zero item requirement",
                                definition.id, choice.id
                            ));
                        }
                    }
                    _ => {}
                }
            }
            for check in &choice.checks {
                let difficulty = match check {
                    Check::Skill {
                        difficulty_milli, ..
                    }
                    | Check::Religion {
                        difficulty_milli, ..
                    }
                    | Check::Attribute {
                        difficulty_milli, ..
                    } => *difficulty_milli,
                };
                if !(1..=10_000).contains(&difficulty) {
                    return Err(format!(
                        "{}:{} invalid check difficulty",
                        definition.id, choice.id
                    ));
                }
            }
            for effect in &choice.effects {
                match effect {
                    Effect::GrantItem { item_id, quantity }
                    | Effect::ConsumeItem { item_id, quantity } => {
                        validate_id(item_id, "effect.item_id")?;
                        if *quantity == 0 {
                            return Err("zero item quantity".into());
                        }
                    }
                    Effect::Currency {
                        currency_id,
                        amount,
                    } => {
                        validate_id(currency_id, "effect.currency_id")?;
                        if *amount == 0 {
                            return Err("zero currency effect".into());
                        }
                    }
                    Effect::Information { information_id } => {
                        validate_id(information_id, "effect.information_id")?
                    }
                }
            }
            let mut axes = BTreeSet::new();
            let mut virtues = BTreeSet::new();
            for development in &choice.personality {
                if development.delta == 0 || development.delta.unsigned_abs() > 10_000 {
                    return Err(format!(
                        "{}:{} has an invalid personality delta",
                        definition.id, choice.id
                    ));
                }
                if !axes.insert(development.axis) {
                    return Err(format!(
                        "{}:{} repeats a personality axis",
                        definition.id, choice.id
                    ));
                }
                virtues.insert(development.virtue);
            }
            if virtues.len() > 1 {
                return Err(format!(
                    "{}:{} names more than one virtue",
                    definition.id, choice.id
                ));
            }
            if !matches!(choice.transition, None | Some(EncounterTransition::Noop)) {
                return Err(format!(
                    "{}:{} uses an unsupported encounter transition",
                    definition.id, choice.id
                ));
            }
            if choice.id == "ignore"
                && (!choice.requirements.is_empty()
                    || !choice.checks.is_empty()
                    || !choice.effects.is_empty()
                    || !choice.personality.is_empty()
                    || !choice.quest_reward_tags.is_empty()
                    || !choice.outcome_tags.is_empty())
            {
                return Err(format!(
                    "{}: ignore choice must be consequence-free",
                    definition.id
                ));
            }
            if choice.id != "ignore" {
                material_routes.insert(
                    serde_json::to_string(&(
                        choice.requirements.as_slice(),
                        choice.checks.as_slice(),
                        choice.effects.as_slice(),
                        choice.transition,
                    ))
                    .unwrap(),
                );
            }
        }
        if !choices.contains("ignore") {
            return Err(format!("{}: ignore choice is required", definition.id));
        }
        if material_routes.len() < 2 {
            return Err(format!(
                "{}: fewer than two materially distinct non-ignore resolutions",
                definition.id
            ));
        }
        if definition.provenance.works.is_empty()
            || definition.provenance.motifs.is_empty()
            || definition.provenance.adaptation_note.trim().is_empty()
        {
            return Err(format!("{}: incomplete provenance", definition.id));
        }
    }
    Ok(())
}

pub fn validate_item_references(
    definitions: &[EncounterDefinition],
    mut exists: impl FnMut(&str) -> bool,
) -> Result<(), String> {
    for definition in definitions {
        for choice in &definition.choices {
            for item_id in choice
                .requirements
                .iter()
                .filter_map(|requirement| match requirement {
                    Requirement::Item { item_id, .. } => Some(item_id.as_str()),
                    _ => None,
                })
                .chain(choice.effects.iter().filter_map(|effect| match effect {
                    Effect::GrantItem { item_id, .. } | Effect::ConsumeItem { item_id, .. } => {
                        Some(item_id.as_str())
                    }
                    Effect::Currency { currency_id, .. } => Some(currency_id.as_str()),
                    _ => None,
                }))
            {
                if !exists(item_id) {
                    return Err(format!(
                        "{}:{} references unknown item {item_id}",
                        definition.id, choice.id
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_lines(encounter: &str, cast: &[Speaker], lines: &[SpokenLine]) -> Result<(), String> {
    for line in lines {
        if line.text.trim().is_empty() || line.text.len() > MAX_TEXT_BYTES {
            return Err(format!("{encounter}: invalid line length"));
        }
        validate_goal_neutral(&line.text)?;
        let speaker = cast
            .iter()
            .find(|speaker| speaker.id == line.speaker)
            .ok_or_else(|| format!("{encounter}: dangling speaker {}", line.speaker))?;
        if !line.reviewed_shakespearean {
            return Err(format!(
                "{encounter}: line lacks Shakespearean-English review"
            ));
        }
        if speaker.nature == SpeakerNature::Supernatural && !line.reviewed_iambic_pentameter {
            return Err(format!(
                "{encounter}: supernatural line lacks reviewed-iambic marker"
            ));
        }
    }
    Ok(())
}

fn validate_goal_neutral(text: &str) -> Result<(), String> {
    let lower = text.to_ascii_lowercase();
    const FORBIDDEN: &[&str] = &[
        "{quest",
        "{case",
        "{finale",
        "your quest",
        "thine errantry",
        "thy goal",
        "your goal",
        "quest destination",
    ];
    if FORBIDDEN.iter().any(|needle| lower.contains(needle))
        || text.contains("{{")
        || text.contains("}}")
    {
        Err(format!(
            "generic prose contains quest goal or runtime slot: {text:?}"
        ))
    } else {
        Ok(())
    }
}

fn validate_id(value: &str, at: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 96
        || !value.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-' | b'.' | b':')
        })
    {
        Err(format!("{at}: invalid ID {value:?}"))
    } else {
        Ok(())
    }
}

#[cfg(runtime_catalog)]
pub fn definitions() -> &'static [EncounterDefinition] {
    static VALUE: OnceLock<Vec<EncounterDefinition>> = OnceLock::new();
    VALUE.get_or_init(|| {
        let value: Vec<EncounterDefinition> = serde_json::from_str(ROAD_ENCOUNTER_CATALOG_JSON)
            .expect("embedded road encounter catalog");
        validate_definitions(&value).expect("validated road encounter catalog");
        validate_item_references(&value, |id| crate::item_catalog::definition(id).is_some())
            .expect("validated road encounter item references");
        value
    })
}

#[cfg(runtime_catalog)]
pub fn encounter(id: &str) -> Option<&'static EncounterDefinition> {
    definitions().iter().find(|definition| definition.id == id)
}

#[cfg(runtime_catalog)]
pub fn select_quest_eligible(seed: u64, draw: u64) -> Option<&'static EncounterDefinition> {
    let eligible = definitions()
        .iter()
        .filter(|definition| !definition.quest_reward_eligibility.is_empty())
        .collect::<Vec<_>>();
    let total = eligible
        .iter()
        .map(|definition| u64::from(definition.weight))
        .sum::<u64>();
    if total == 0 {
        return None;
    }
    let mut mixed = seed ^ draw.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ 0x726f_6164_7175_6573;
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    let mut roll = (mixed ^ (mixed >> 31)) % total;
    for definition in eligible {
        if roll < u64::from(definition.weight) {
            return Some(definition);
        }
        roll -= u64::from(definition.weight);
    }
    None
}
#[cfg(runtime_catalog)]
pub fn digest() -> &'static str {
    ROAD_ENCOUNTER_CATALOG_DIGEST
}
#[cfg(runtime_catalog)]
pub fn sources() -> Vec<EncounterSource> {
    serde_json::from_str(ROAD_ENCOUNTER_SOURCE_MAP_JSON).expect("embedded encounter sources")
}

#[cfg(all(test, runtime_catalog))]
mod tests {
    use super::*;
    #[test]
    fn embedded_catalog_is_valid_and_provenanced() {
        validate_definitions(definitions()).unwrap();
        assert_eq!(sources().len(), definitions().len());
        assert_eq!(digest().len(), 64);
        assert!(
            sources()
                .iter()
                .all(|source| source.file.starts_with("content/encounters/"))
        );
    }
    #[test]
    fn supernatural_review_inventory_is_closed() {
        for definition in definitions() {
            for line in definition.opening.iter().chain(
                definition
                    .choices
                    .iter()
                    .flat_map(|choice| &choice.response),
            ) {
                let nature = definition
                    .cast
                    .iter()
                    .find(|speaker| speaker.id == line.speaker)
                    .unwrap()
                    .nature;
                assert!(line.reviewed_shakespearean);
                if nature == SpeakerNature::Supernatural {
                    assert!(line.reviewed_iambic_pentameter);
                }
            }
        }
    }
    #[test]
    fn semantic_validator_rejects_goal_slots_duplicate_routes_and_missing_review() {
        let mut value = definitions()[0].clone();
        value.opening[0].text = "Thy goal is {{quest}}".into();
        assert!(
            validate_definitions(&[value])
                .unwrap_err()
                .contains("quest goal")
        );
        let mut value = definitions()[0].clone();
        value.choices[1].id = value.choices[0].id.clone();
        assert!(
            validate_definitions(&[value])
                .unwrap_err()
                .contains("duplicate choice")
        );
        let mut value = definitions()[0].clone();
        value.opening[0].reviewed_shakespearean = false;
        assert!(
            validate_definitions(&[value])
                .unwrap_err()
                .contains("Shakespearean")
        );
    }
    #[test]
    fn item_references_are_checked_independently_of_prose() {
        let error =
            validate_item_references(definitions(), |id| id != "captured_black_knight_dispatch")
                .unwrap_err();
        assert!(error.contains("captured_black_knight_dispatch"));
    }

    #[test]
    fn quest_selection_is_deterministic_weighted_and_eligible() {
        let first = select_quest_eligible(42, 7).unwrap();
        let second = select_quest_eligible(42, 7).unwrap();
        assert_eq!(first.id, second.id);
        assert!(!first.quest_reward_eligibility.is_empty());
    }

    #[test]
    fn validator_rejects_dirty_ignore_and_non_mechanical_route_variants() {
        let mut dirty = definitions()[0].clone();
        dirty
            .choices
            .iter_mut()
            .find(|choice| choice.id == "ignore")
            .unwrap()
            .personality
            .push(PersonalityDevelopment {
                axis: PersonalityAxisId::Nerve,
                delta: 1,
                virtue: VirtueId::Courage,
            });
        assert!(
            validate_definitions(&[dirty])
                .unwrap_err()
                .contains("consequence-free")
        );

        let mut duplicate = definitions()[0].clone();
        let first = duplicate
            .choices
            .iter()
            .position(|choice| choice.id == "aid")
            .unwrap();
        let second = duplicate
            .choices
            .iter()
            .position(|choice| choice.id == "rally")
            .unwrap();
        duplicate.choices[second].requirements = duplicate.choices[first].requirements.clone();
        duplicate.choices[second].checks = duplicate.choices[first].checks.clone();
        duplicate.choices[second].effects = duplicate.choices[first].effects.clone();
        duplicate.choices[second].transition = duplicate.choices[first].transition;
        duplicate
            .choices
            .retain(|choice| matches!(choice.id.as_str(), "aid" | "rally" | "ignore"));
        assert!(
            validate_definitions(&[duplicate])
                .unwrap_err()
                .contains("materially distinct")
        );
    }

    #[test]
    fn wounded_knight_routes_are_physical_distinct_and_mortal() {
        let definition = encounter("wounded_knight_linden_v1").unwrap();
        assert!(definition.triggers.travel && definition.triggers.rest);
        assert!(
            definition
                .cast
                .iter()
                .all(|speaker| speaker.nature == SpeakerNature::Mortal)
        );
        assert!(
            definition
                .opening
                .iter()
                .chain(
                    definition
                        .choices
                        .iter()
                        .flat_map(|choice| &choice.response)
                )
                .all(|line| line.reviewed_shakespearean && !line.reviewed_iambic_pentameter)
        );
        let choice = |id| {
            definition
                .choices
                .iter()
                .find(|choice| choice.id == id)
                .unwrap()
        };
        assert!(
            matches!(choice("treat").requirements[0], Requirement::Item { ref item_id, minimum_quantity: 1 } if item_id == "bandage")
        );
        assert!(
            matches!(choice("treat").effects[0], Effect::ConsumeItem { ref item_id, quantity: 1 } if item_id == "bandage")
        );
        assert!(choice("treat").effects.iter().any(|effect| matches!(effect,
            Effect::Information { information_id } if information_id == "linden_assailants_close_ambush_method")));
        assert!(choice("track").effects.iter().any(|effect| matches!(effect,
            Effect::GrantItem { item_id, quantity: 1 } if item_id == "captured_black_knight_dispatch")));
        assert!(
            choice("track")
                .effects
                .iter()
                .any(|effect| matches!(effect, Effect::Information { .. }))
        );
        assert!(matches!(
            choice("organize_aid").checks[0],
            Check::Skill {
                skill: SkillId::Command,
                ..
            }
        ));
        assert!(matches!(
            choice("plunder").effects[0],
            Effect::Currency { amount: 48, .. }
        ));
        assert!(
            choice("organize_aid")
                .effects
                .iter()
                .any(|effect| matches!(effect, Effect::Currency { amount: 16, .. }))
        );
        assert!(
            choice("plunder")
                .effects
                .iter()
                .any(|effect| matches!(effect,
            Effect::GrantItem { item_id, .. } if item_id == "arming_sword"))
        );
        assert!(
            choice("plunder")
                .effects
                .iter()
                .any(|effect| matches!(effect,
            Effect::GrantItem { item_id, .. } if item_id == "heater_shield"))
        );
        assert!(
            definition
                .choices
                .iter()
                .filter(|choice| choice.id != "ignore")
                .all(|choice| !choice.effects.is_empty())
        );
        assert!(
            choice("ignore").requirements.is_empty()
                && choice("ignore").checks.is_empty()
                && choice("ignore").effects.is_empty()
                && choice("ignore").personality.is_empty()
        );
    }
}
