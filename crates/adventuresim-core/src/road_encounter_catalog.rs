//! Strict, build-time compiled definitions for goal-neutral road and rest encounters.
//!
//! Content declares presentation and closed, typed intentions. Strategic reducers
//! remain the sole authority for checks and state mutation.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::OnceLock};

#[cfg(runtime_catalog)]
include!(concat!(env!("OUT_DIR"), "/road_encounter_catalog.rs"));

pub const CATALOG_REVISION: u32 = 3;
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
    pub backing: SpeakerBacking,
}

/// Declares whether a visible cast entry owns ordinary Character authority.
/// This is deliberately mandatory in content: a mortal name is never allowed
/// to silently fall back to a prose-only surrogate.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SpeakerBacking {
    Character {
        role: CharacterCastRole,
        contact_decision: AuthoredInteractionDecision,
        treatment_decision: AuthoredInteractionDecision,
    },
    NarrativeOnly {
        reason: String,
    },
    Blocked {
        issue: String,
        reason: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthoredInteractionDecision {
    Allowed,
    Refused,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CharacterCastRole {
    Counterparty,
    Patient,
    Bystander,
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
    pub interaction: InteractionBacking,
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

/// Machine-readable audit of how a choice reaches game authority. Narrative
/// text can explain a deed, but it cannot be the authority for a character
/// interaction.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InteractionBacking {
    SharedSystems {
        #[serde(default)]
        character_actions: Vec<SharedCharacterInteraction>,
    },
    NarrativeOnly {
        reason: String,
    },
    Blocked {
        issue: String,
        reason: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SharedCharacterInteraction {
    ConversationContact,
    Treatment,
    InvestigationInformation,
    ItemInventoryTransfer,
    RelationshipPersonality,
    Combat,
    IgnoreTravel,
}

impl EncounterChoice {
    pub fn requires_treatment(&self) -> bool {
        matches!(
            &self.interaction,
            InteractionBacking::SharedSystems { character_actions }
                if character_actions.contains(&SharedCharacterInteraction::Treatment)
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EncounterTransition {
    Noop,
    StartCombat {
        archetype: RoadCombatArchetype,
        count: u16,
        outcomes: CombatOutcomeSet,
    },
    TravelDelay {
        minutes: u16,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct CombatOutcomeSet {
    pub victory: CombatOutcomePayload,
    pub defeat: CombatOutcomePayload,
    pub escape: CombatOutcomePayload,
    pub surrender: CombatOutcomePayload,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct CombatOutcomePayload {
    pub result: String,
    #[serde(default)]
    pub effects: Vec<Effect>,
    #[serde(default)]
    pub personality: Vec<PersonalityDevelopment>,
    #[serde(default)]
    pub quest_reward_tags: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CombatOutcomeKind {
    Victory,
    Defeat,
    Escape,
    Surrender,
}

pub fn resolved_combat_outcome(outcome: &str) -> Result<CombatOutcomeKind, String> {
    match outcome {
        "victory" => Ok(CombatOutcomeKind::Victory),
        "defeat" | "stalemate" => Ok(CombatOutcomeKind::Defeat),
        "avoided" => Ok(CombatOutcomeKind::Escape),
        "surrendered" => Ok(CombatOutcomeKind::Surrender),
        _ => Err(format!("Unknown narrative combat outcome {outcome}")),
    }
}

impl CombatOutcomeSet {
    pub fn payload(&self, kind: CombatOutcomeKind) -> &CombatOutcomePayload {
        match kind {
            CombatOutcomeKind::Victory => &self.victory,
            CombatOutcomeKind::Defeat => &self.defeat,
            CombatOutcomeKind::Escape => &self.escape,
            CombatOutcomeKind::Surrender => &self.surrender,
        }
    }
}

pub fn exemplified_virtue(developments: &[PersonalityDevelopment]) -> Option<VirtueId> {
    developments
        .iter()
        .find(|development| development.delta > 0)
        .map(|development| development.virtue)
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RoadCombatArchetype {
    Bandits,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EncounterPresentation {
    pub cast: Vec<PresentationCastMember>,
    pub opening: Vec<PresentationLine>,
    pub choices: Vec<PresentationChoice>,
    pub response: Vec<PresentationLine>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationCastMember {
    pub character_id: u64,
    pub name: String,
    pub role: CharacterCastRole,
    pub contact_decision: InteractionPresentationDecision,
    pub treatment_decision: InteractionPresentationDecision,
    pub contact_revision: u32,
    pub membership_revision: u32,
    pub treatment_limb_slug: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InteractionPresentationDecision {
    Request,
    Refused,
    Unavailable,
    EmergencyTreatment,
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
    Currency {
        amount: u32,
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
            match &speaker.backing {
                SpeakerBacking::Character { .. } if speaker.nature != SpeakerNature::Mortal => {
                    return Err(format!(
                        "{}:{}: supernatural cast cannot silently materialize as a Character",
                        definition.id, speaker.id
                    ));
                }
                SpeakerBacking::NarrativeOnly { reason } if reason.trim().is_empty() => {
                    return Err(format!(
                        "{}:{}: narrative-only cast requires an audit reason",
                        definition.id, speaker.id
                    ));
                }
                SpeakerBacking::Blocked { issue, reason }
                    if !valid_follow_up_issue(issue) || reason.trim().is_empty() =>
                {
                    return Err(format!(
                        "{}:{}: blocked cast requires a named follow-up issue and reason",
                        definition.id, speaker.id
                    ));
                }
                _ => {}
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
            // Ignore is a universal escape hatch. Report violations of that
            // invariant before validating its declared backing so content
            // authors get the actionable error even when the dirty mechanic
            // would also require another typed system declaration.
            if choice.id == "ignore"
                && (!choice.requirements.is_empty()
                    || !choice.checks.is_empty()
                    || !choice.effects.is_empty()
                    || !choice.personality.is_empty()
                    || !choice.quest_reward_tags.is_empty()
                    || !choice.outcome_tags.is_empty()
                    || !matches!(
                        choice.transition.as_ref(),
                        None | Some(EncounterTransition::Noop)
                            | Some(EncounterTransition::TravelDelay { .. })
                    ))
            {
                return Err(format!(
                    "{}: ignore choice must be consequence-free",
                    definition.id
                ));
            }
            match &choice.interaction {
                InteractionBacking::SharedSystems { character_actions } => {
                    if character_actions.is_empty() {
                        return Err(format!(
                            "{}:{} shared-system interaction requires a typed character action",
                            definition.id, choice.id
                        ));
                    }
                    if character_actions.contains(&SharedCharacterInteraction::Treatment)
                        && !definition.cast.iter().any(|speaker| {
                            matches!(
                                &speaker.backing,
                                SpeakerBacking::Character {
                                    role: CharacterCastRole::Patient,
                                    treatment_decision: AuthoredInteractionDecision::Allowed
                                        | AuthoredInteractionDecision::Unavailable,
                                    ..
                                }
                            )
                        })
                    {
                        return Err(format!(
                            "{}:{} treatment requires a non-refusing Patient",
                            definition.id, choice.id
                        ));
                    }
                    if choice.id == "ignore"
                        && character_actions.as_slice()
                            != [SharedCharacterInteraction::IgnoreTravel]
                    {
                        return Err(format!(
                            "{}:{} ignore must declare only ignore_travel",
                            definition.id, choice.id
                        ));
                    }
                    let starts_combat = matches!(
                        choice.transition.as_ref(),
                        Some(EncounterTransition::StartCombat { .. })
                    );
                    if starts_combat
                        != character_actions.contains(&SharedCharacterInteraction::Combat)
                    {
                        return Err(format!(
                            "{}:{} combat backing does not match its transition",
                            definition.id, choice.id
                        ));
                    }
                    let has_information = choice
                        .effects
                        .iter()
                        .any(|effect| matches!(effect, Effect::Information { .. }));
                    let has_inventory_transfer = choice.effects.iter().any(|effect| {
                        matches!(
                            effect,
                            Effect::GrantItem { .. }
                                | Effect::ConsumeItem { .. }
                                | Effect::Currency { .. }
                        )
                    });
                    for (mechanic_present, action, label) in [
                        (
                            has_information,
                            SharedCharacterInteraction::InvestigationInformation,
                            "investigation information",
                        ),
                        (
                            has_inventory_transfer,
                            SharedCharacterInteraction::ItemInventoryTransfer,
                            "inventory transfer",
                        ),
                        (
                            !choice.personality.is_empty(),
                            SharedCharacterInteraction::RelationshipPersonality,
                            "relationship personality",
                        ),
                    ] {
                        if mechanic_present != character_actions.contains(&action) {
                            return Err(format!(
                                "{}:{} {label} backing does not match its mechanics",
                                definition.id, choice.id
                            ));
                        }
                    }
                }
                InteractionBacking::NarrativeOnly { reason } if reason.trim().is_empty() => {
                    return Err(format!(
                        "{}:{} narrative-only interaction requires an audit reason",
                        definition.id, choice.id
                    ));
                }
                InteractionBacking::Blocked { issue, reason }
                    if !valid_follow_up_issue(issue) || reason.trim().is_empty() =>
                {
                    return Err(format!(
                        "{}:{} blocked interaction requires a named follow-up issue and reason",
                        definition.id, choice.id
                    ));
                }
                _ => {}
            }
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
                    Requirement::Currency { amount } if *amount == 0 => {
                        return Err(format!(
                            "{}:{} has a zero currency requirement",
                            definition.id, choice.id
                        ));
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
                validate_effect(effect)?;
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
            if let Some(EncounterTransition::StartCombat {
                count, outcomes, ..
            }) = &choice.transition
            {
                if !(1..=8).contains(count) {
                    return Err(format!(
                        "{}:{} has an unsafe combat count",
                        definition.id, choice.id
                    ));
                }
                validate_combat_outcome_payload(
                    &definition.id,
                    &choice.id,
                    "victory",
                    &outcomes.victory,
                    true,
                )?;
                for (name, payload) in [
                    ("defeat", &outcomes.defeat),
                    ("escape", &outcomes.escape),
                    ("surrender", &outcomes.surrender),
                ] {
                    validate_combat_outcome_payload(
                        &definition.id,
                        &choice.id,
                        name,
                        payload,
                        false,
                    )?;
                }
                if !choice.effects.is_empty() || !choice.quest_reward_tags.is_empty() {
                    return Err(format!(
                        "{}:{} combat rewards must be victory-scoped",
                        definition.id, choice.id
                    ));
                }
            }
            if let Some(EncounterTransition::TravelDelay { minutes }) = &choice.transition
                && !(1..=720).contains(minutes)
            {
                return Err(format!(
                    "{}:{} has an unsafe travel delay",
                    definition.id, choice.id
                ));
            }
            if choice.id == "ignore"
                && (!choice.requirements.is_empty()
                    || !choice.checks.is_empty()
                    || !choice.effects.is_empty()
                    || !choice.personality.is_empty()
                    || !choice.quest_reward_tags.is_empty()
                    || !choice.outcome_tags.is_empty()
                    || !matches!(
                        choice.transition.as_ref(),
                        None | Some(EncounterTransition::Noop)
                            | Some(EncounterTransition::TravelDelay { .. })
                    ))
            {
                return Err(format!(
                    "{}: ignore choice must be consequence-free",
                    definition.id
                ));
            }
            if choice.id != "ignore" {
                if matches!(
                    &choice.interaction,
                    InteractionBacking::NarrativeOnly { .. }
                ) && (!choice.requirements.is_empty()
                    || !choice.checks.is_empty()
                    || !choice.effects.is_empty()
                    || !choice.personality.is_empty()
                    || !matches!(
                        choice.transition.as_ref(),
                        None | Some(EncounterTransition::Noop)
                    ))
                {
                    return Err(format!(
                        "{}:{} narrative-only interaction declares authoritative mechanics",
                        definition.id, choice.id
                    ));
                }
                material_routes.insert(
                    serde_json::to_string(&(
                        choice.requirements.as_slice(),
                        choice.checks.as_slice(),
                        choice.effects.as_slice(),
                        choice.transition.as_ref(),
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

fn valid_follow_up_issue(issue: &str) -> bool {
    issue.strip_prefix('#').is_some_and(|number| {
        !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn validate_combat_outcome_payload(
    definition_id: &str,
    choice_id: &str,
    outcome: &str,
    payload: &CombatOutcomePayload,
    allow_material_rewards: bool,
) -> Result<(), String> {
    if payload.result.trim().is_empty() || payload.result.len() > MAX_TEXT_BYTES {
        return Err(format!(
            "{definition_id}:{choice_id} has invalid {outcome} combat outcome text"
        ));
    }
    validate_goal_neutral(&payload.result)?;
    if payload.effects.len() > 8
        || payload.personality.len() > 8
        || payload.quest_reward_tags.len() > 8
    {
        return Err(format!(
            "{definition_id}:{choice_id} has an oversized {outcome} combat payload"
        ));
    }
    for effect in &payload.effects {
        validate_effect(effect)?;
        if !allow_material_rewards
            && matches!(
                effect,
                Effect::GrantItem { .. }
                    | Effect::Currency { amount: 1.., .. }
                    | Effect::Information { .. }
            )
        {
            return Err(format!(
                "{definition_id}:{choice_id} grants a material reward on {outcome}"
            ));
        }
    }
    let mut axes = BTreeSet::new();
    let mut virtues = BTreeSet::new();
    for development in &payload.personality {
        if development.delta == 0
            || development.delta.unsigned_abs() > 10_000
            || !axes.insert(development.axis)
        {
            return Err(format!(
                "{definition_id}:{choice_id} has invalid {outcome} personality"
            ));
        }
        virtues.insert(development.virtue);
    }
    if virtues.len() > 1 {
        return Err(format!(
            "{definition_id}:{choice_id} names multiple {outcome} virtues"
        ));
    }
    let mut tags = BTreeSet::new();
    for tag in &payload.quest_reward_tags {
        validate_id(tag, "combat_outcome.quest_reward_tag")?;
        if !tags.insert(tag) {
            return Err(format!(
                "{definition_id}:{choice_id} repeats a {outcome} quest reward tag"
            ));
        }
    }
    if !allow_material_rewards && !payload.quest_reward_tags.is_empty() {
        return Err(format!(
            "{definition_id}:{choice_id} grants quest rewards on {outcome}"
        ));
    }
    Ok(())
}

fn validate_effect(effect: &Effect) -> Result<(), String> {
    match effect {
        Effect::GrantItem { item_id, quantity } | Effect::ConsumeItem { item_id, quantity } => {
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
            if let Some(EncounterTransition::StartCombat { outcomes, .. }) = &choice.transition {
                for item_id in [
                    &outcomes.victory,
                    &outcomes.defeat,
                    &outcomes.escape,
                    &outcomes.surrender,
                ]
                .into_iter()
                .flat_map(|payload| payload.effects.iter())
                .filter_map(|effect| match effect {
                    Effect::GrantItem { item_id, .. } | Effect::ConsumeItem { item_id, .. } => {
                        Some(item_id.as_str())
                    }
                    Effect::Currency { currency_id, .. } => Some(currency_id.as_str()),
                    Effect::Information { .. } => None,
                }) {
                    if !exists(item_id) {
                        return Err(format!(
                            "{}:{} references unknown item {item_id}",
                            definition.id, choice.id
                        ));
                    }
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
    fn every_cast_member_and_choice_has_machine_readable_backing() {
        for definition in definitions() {
            assert!(
                definition
                    .cast
                    .iter()
                    .all(|speaker| match &speaker.backing {
                        SpeakerBacking::Character { .. } => speaker.nature == SpeakerNature::Mortal,
                        SpeakerBacking::NarrativeOnly { reason } => !reason.trim().is_empty(),
                        SpeakerBacking::Blocked { issue, reason } => {
                            valid_follow_up_issue(issue) && !reason.trim().is_empty()
                        }
                    })
            );
            assert!(
                definition
                    .choices
                    .iter()
                    .all(|choice| match &choice.interaction {
                        InteractionBacking::SharedSystems { character_actions } => {
                            !character_actions.is_empty()
                        }
                        InteractionBacking::NarrativeOnly { reason } => !reason.trim().is_empty(),
                        InteractionBacking::Blocked { issue, reason } => {
                            valid_follow_up_issue(issue) && !reason.trim().is_empty()
                        }
                    })
            );
        }
    }

    #[test]
    fn treatment_verbs_are_typed_and_bound_to_non_refusing_patients() {
        let treatment_routes = definitions()
            .iter()
            .flat_map(|definition| {
                definition
                    .choices
                    .iter()
                    .filter(|choice| choice.requires_treatment())
                    .map(move |choice| (definition, choice))
            })
            .collect::<Vec<_>>();
        assert_eq!(treatment_routes.len(), 2);
        for (definition, _) in treatment_routes {
            assert!(definition.cast.iter().any(|speaker| matches!(
                &speaker.backing,
                SpeakerBacking::Character {
                    role: CharacterCastRole::Patient,
                    contact_decision: _,
                    treatment_decision: AuthoredInteractionDecision::Allowed
                        | AuthoredInteractionDecision::Unavailable,
                }
            )));
        }
    }

    #[test]
    fn validator_rejects_unsupported_character_and_scripted_backing() {
        let mut supernatural = encounter("enchanted_fog_lost_forester_v1").unwrap().clone();
        supernatural.cast[0].backing = SpeakerBacking::Character {
            role: CharacterCastRole::Counterparty,
            contact_decision: AuthoredInteractionDecision::Allowed,
            treatment_decision: AuthoredInteractionDecision::Unavailable,
        };
        assert!(
            validate_definitions(&[supernatural])
                .unwrap_err()
                .contains("supernatural cast")
        );

        let mut blocked = encounter("proud_traveler_errands_v1").unwrap().clone();
        blocked.choices[0].interaction = InteractionBacking::Blocked {
            issue: "later".into(),
            reason: "Needs a shared authority".into(),
        };
        assert!(
            validate_definitions(&[blocked])
                .unwrap_err()
                .contains("named follow-up issue")
        );
    }

    #[test]
    fn validator_rejects_empty_and_mismatched_shared_system_claims() {
        let mut empty = encounter("proud_traveler_errands_v1").unwrap().clone();
        empty.choices[0].interaction = InteractionBacking::SharedSystems {
            character_actions: Vec::new(),
        };
        assert!(
            validate_definitions(&[empty])
                .unwrap_err()
                .contains("requires a typed character action")
        );

        let mut false_combat = encounter("proud_traveler_errands_v1").unwrap().clone();
        false_combat.choices[0].interaction = InteractionBacking::SharedSystems {
            character_actions: vec![SharedCharacterInteraction::Combat],
        };
        assert!(
            validate_definitions(&[false_combat])
                .unwrap_err()
                .contains("combat backing does not match")
        );

        let mut false_inventory = encounter("proud_traveler_errands_v1").unwrap().clone();
        if let InteractionBacking::SharedSystems { character_actions } =
            &mut false_inventory.choices[0].interaction
        {
            character_actions
                .retain(|action| *action != SharedCharacterInteraction::ItemInventoryTransfer);
        }
        assert!(
            validate_definitions(&[false_inventory])
                .unwrap_err()
                .contains("inventory transfer backing does not match")
        );
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
        let mut dirty = encounter("wounded_order_courier_v1").unwrap().clone();
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

        let mut duplicate = encounter("wounded_order_courier_v1").unwrap().clone();
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
        duplicate.choices[second].transition = duplicate.choices[first].transition.clone();
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
        let opening = definition.opening[0].text.to_ascii_lowercase();
        for withheld_fact in ["mail", "mid-shin", "lightly burdened", "movement"] {
            assert!(!opening.contains(withheld_fact));
        }
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

    #[test]
    fn unlawful_bridge_uses_bounded_authored_combat_and_honest_rewards() {
        let definition = encounter("unlawful_bridge_custom_v1").unwrap();
        assert!(definition.triggers.travel && !definition.triggers.rest);
        assert!(
            definition
                .cast
                .iter()
                .all(|speaker| speaker.nature == SpeakerNature::Mortal)
        );
        let choice = |id| {
            definition
                .choices
                .iter()
                .find(|choice| choice.id == id)
                .unwrap()
        };
        assert!(matches!(
            choice("pay_toll").effects[0],
            Effect::Currency { amount: -12, .. }
        ));
        assert!(matches!(
            choice("pay_toll").requirements[0],
            Requirement::Currency { amount: 12 }
        ));
        assert!(
            matches!(choice("barter_rations").effects[0], Effect::ConsumeItem { ref item_id, quantity: 4 } if item_id == "travel_ration")
        );
        assert!(
            matches!(choice("expose_charter").effects[0], Effect::Information { ref information_id }
            if information_id == "unlawful_bridge_false_charter_marks")
        );
        assert!(matches!(
            choice("join_watch").effects[0],
            Effect::Currency { amount: 40, .. }
        ));
        assert!(choice("join_watch").personality[0].delta < 0);
        assert_eq!(exemplified_virtue(&choice("join_watch").personality), None);
        let EncounterTransition::StartCombat {
            archetype,
            count,
            outcomes,
            ..
        } = choice("challenge_to_arms").transition.as_ref().unwrap()
        else {
            panic!("combat transition")
        };
        assert_eq!(*archetype, RoadCombatArchetype::Bandits);
        assert_eq!(*count, 2);
        assert!(choice("challenge_to_arms").effects.is_empty());
        assert!(choice("challenge_to_arms").quest_reward_tags.is_empty());
        assert!(
            outcomes
                .victory
                .effects
                .iter()
                .any(|effect| matches!(effect, Effect::Currency { amount: 12, .. }))
        );
        assert!(outcomes.victory.effects.iter().any(|effect| matches!(effect,
            Effect::Information { information_id } if information_id == "unlawful_bridge_keeper_fighting_method")));
        assert_eq!(outcomes.victory.personality[0].virtue, VirtueId::Courage);
        assert_eq!(
            outcomes.victory.quest_reward_tags,
            &["bridge_keeper_fighting_method"]
        );
        assert!(outcomes.escape.personality[0].delta < 0);
        assert!(outcomes.surrender.personality[0].delta < 0);
        assert_eq!(exemplified_virtue(&outcomes.escape.personality), None);
        assert_eq!(exemplified_virtue(&outcomes.surrender.personality), None);
        assert_eq!(
            exemplified_virtue(&outcomes.victory.personality),
            Some(VirtueId::Courage)
        );
        assert_ne!(outcomes.escape.result, outcomes.surrender.result);
        assert!(matches!(
            choice("ignore").transition.as_ref(),
            Some(EncounterTransition::TravelDelay { minutes: 120 })
        ));
    }

    #[test]
    fn proud_traveler_routes_are_mortal_distinct_grounded_and_balanced() {
        let definition = encounter("proud_traveler_errands_v1").unwrap();
        assert!(definition.triggers.travel && !definition.triggers.rest);
        assert_eq!(definition.weight, 70);
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
        let non_ignore = definition
            .choices
            .iter()
            .filter(|choice| choice.id != "ignore")
            .collect::<Vec<_>>();
        assert_eq!(non_ignore.len(), 5);
        let route_signatures = non_ignore
            .iter()
            .map(|choice| {
                serde_json::to_string(&(
                    &choice.requirements,
                    &choice.checks,
                    &choice.effects,
                    &choice.personality,
                    &choice.transition,
                ))
                .unwrap()
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(route_signatures.len(), 5);
        assert!(non_ignore.iter().all(|choice| {
            choice.effects.iter().any(|effect| {
                matches!(
                    effect,
                    Effect::Information { information_id }
                        if information_id == "mid_shin_mud_checks_mail_burdened_movement"
                )
            }) && choice.quest_reward_tags == ["heavy_attackers_slow_in_mid_shin_mud"]
                && choice
                    .outcome_tags
                    .iter()
                    .any(|tag| tag == "physical_observation")
        }));
        let delays = non_ignore
            .iter()
            .map(|choice| match choice.transition.as_ref() {
                Some(EncounterTransition::TravelDelay { minutes }) => *minutes,
                _ => panic!("non-ignore route lacks a travel delay"),
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(delays, BTreeSet::from([20, 30, 40, 60]));
        for (route, expected_minutes) in [
            ("render_service", 60),
            ("bargain_as_equals", 30),
            ("order_household", 20),
            ("rebuke_pride", 40),
            ("keep_entrusted_purse", 20),
        ] {
            assert!(matches!(
                choice(route).transition.as_ref(),
                Some(EncounterTransition::TravelDelay { minutes })
                    if *minutes == expected_minutes
            ));
        }
        assert!(matches!(
            choice("bargain_as_equals").checks[0],
            Check::Skill {
                skill: SkillId::Charm,
                difficulty_milli: 1000
            }
        ));
        assert!(matches!(
            choice("order_household").checks[0],
            Check::Skill {
                skill: SkillId::Command,
                difficulty_milli: 1000
            }
        ));
        assert!(matches!(
            choice("rebuke_pride").requirements[0],
            Requirement::Religion {
                religion: ReligionId::RomanCatholic
            }
        ));
        assert!(
            choice("rebuke_pride").response[0]
                .text
                .contains("duty unto the exhausted servant")
        );
        assert!(choice("rebuke_pride").effects.iter().any(|effect| matches!(
            effect,
            Effect::GrantItem { item_id, quantity: 2 } if item_id == "table_wine"
        )));
        assert!(
            choice("render_service")
                .effects
                .iter()
                .any(|effect| matches!(effect, Effect::Currency { amount: 8, .. }))
        );
        assert!(
            choice("bargain_as_equals")
                .effects
                .iter()
                .any(|effect| matches!(effect, Effect::Currency { amount: 16, .. }))
        );
        for (route, virtue) in [
            ("render_service", VirtueId::Courtesy),
            ("bargain_as_equals", VirtueId::Justice),
            ("order_household", VirtueId::Prudence),
            ("rebuke_pride", VirtueId::Faith),
        ] {
            assert_eq!(exemplified_virtue(&choice(route).personality), Some(virtue));
        }
        let theft = choice("keep_entrusted_purse");
        assert!(theft.response[0].text.contains("served a noble household"));
        assert!(theft.response[0].text.contains("forty-eight groschen"));
        assert!(!theft.response[0].text.contains("road note"));
        assert!(theft.result.contains("twenty minutes staging haulers"));
        assert!(theft.result.contains("mid-shin mud"));
        assert!(theft.result.contains("abscondest"));
        assert!(matches!(
            theft.effects[0],
            Effect::Currency { amount: 48, .. }
        ));
        assert!(theft.personality[0].delta < 0);
        assert_eq!(exemplified_virtue(&theft.personality), None);
        let largest_honest_coin = non_ignore
            .iter()
            .filter(|choice| choice.id != "keep_entrusted_purse")
            .flat_map(|choice| &choice.effects)
            .filter_map(|effect| match effect {
                Effect::Currency { amount, .. } if *amount > 0 => Some(*amount),
                _ => None,
            })
            .max()
            .unwrap();
        assert!(48 > largest_honest_coin);
        assert!(choice("ignore").transition.is_none());
        assert!(choice("ignore").effects.is_empty());
        assert!(choice("ignore").personality.is_empty());
    }

    #[test]
    fn ferryman_disputed_tribute_routes_are_grounded_distinct_and_balanced() {
        let definition = encounter("ferryman_disputed_tribute_v1").unwrap();
        assert!(definition.triggers.travel && !definition.triggers.rest);
        assert_eq!(definition.weight, 70);
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
        let opening = definition
            .opening
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        assert!(opening.contains("mixed company waiteth upon either bank"));
        assert!(opening.contains("mail-clad riders"));
        assert!(opening.contains("their horses"));
        assert!(opening.contains("blade of my west oar lieth split"));
        for withheld_observation in ["shallow", "gravel", "deep current", "forceth both back"] {
            assert!(!opening.contains(withheld_observation));
        }
        assert!(opening.contains("twelve groschen"));
        assert!(opening.contains("elder marks"));
        let authored_prose = serde_json::to_string(definition).unwrap().to_lowercase();
        assert!(!authored_prose.contains("spare oar"));
        assert!(!authored_prose.contains("hidden oar"));

        let choice = |id| {
            definition
                .choices
                .iter()
                .find(|choice| choice.id == id)
                .unwrap()
        };
        let active = definition
            .choices
            .iter()
            .filter(|choice| choice.id != "ignore")
            .collect::<Vec<_>>();
        assert_eq!(definition.choices.len(), 7);
        let signatures = active
            .iter()
            .map(|choice| {
                serde_json::to_string(&(
                    &choice.requirements,
                    &choice.checks,
                    &choice.effects,
                    &choice.personality,
                    &choice.transition,
                ))
                .unwrap()
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(signatures.len(), 6);
        assert!(active.iter().all(|choice| {
            choice.effects.iter().any(|effect| {
                matches!(
                    effect,
                    Effect::Information { information_id }
                        if information_id == "deep_current_confines_mail_riders_to_shallows"
                )
            }) && choice.quest_reward_tags == ["heavy_attackers_confined_to_shallow_gravel"]
                && choice
                    .outcome_tags
                    .iter()
                    .any(|tag| tag == "physical_observation")
        }));
        assert!(active.iter().all(|choice| {
            choice.result.contains("witnessest a mail-clad rider")
                && choice
                    .result
                    .contains("urge his horse into the deep current")
                && choice.result.contains("forceth both back")
                && choice.result.contains("shallow gravel margin")
        }));

        for (route, minutes) in [
            ("pay_tribute", 15),
            ("barter_provisions", 20),
            ("row_in_lieu", 45),
            ("expose_altered_tally", 30),
            ("organize_repair", 60),
            ("steal_till", 90),
        ] {
            assert!(matches!(
                choice(route).transition.as_ref(),
                Some(EncounterTransition::TravelDelay { minutes: actual }) if *actual == minutes
            ));
        }
        assert!(matches!(
            choice("ignore").transition.as_ref(),
            Some(EncounterTransition::TravelDelay { minutes: 180 })
        ));
        assert!(choice("ignore").effects.is_empty());
        assert!(choice("ignore").personality.is_empty());
        assert!(choice("ignore").quest_reward_tags.is_empty());

        assert!(matches!(
            choice("pay_tribute").requirements[0],
            Requirement::Currency { amount: 12 }
        ));
        assert!(matches!(
            choice("pay_tribute").effects[0],
            Effect::Currency { amount: -12, .. }
        ));
        assert!(matches!(
            choice("barter_provisions").requirements[0],
            Requirement::Item {
                ref item_id,
                minimum_quantity: 4
            } if item_id == "travel_ration"
        ));
        assert_eq!(
            crate::item_catalog::definition("travel_ration")
                .unwrap()
                .base_value
                * 4,
            12
        );
        assert!(matches!(
            choice("row_in_lieu").checks[0],
            Check::Attribute {
                attribute: AttributeId::Endurance,
                difficulty_milli: 1200
            }
        ));
        assert!(choice("row_in_lieu").label.contains("sound sculling oar"));
        assert!(
            choice("row_in_lieu").response[0]
                .text
                .contains("tend the landing cable and call thy stroke")
        );
        for route in ["pay_tribute", "barter_provisions"] {
            assert!(choice(route).result.contains("slow sculling"));
        }
        let lawful_fare = choice("expose_altered_tally");
        assert!(lawful_fare.response[0].text.contains("Four groschen"));
        assert!(lawful_fare.response[0].text.contains("witness-mark"));
        assert!(matches!(
            lawful_fare.requirements[0],
            Requirement::Currency { amount: 4 }
        ));
        assert!(matches!(
            lawful_fare.effects[0],
            Effect::Currency { amount: -4, .. }
        ));
        assert!(matches!(
            choice("organize_repair").checks[0],
            Check::Skill {
                skill: SkillId::Command,
                difficulty_milli: 1000
            }
        ));
        assert!(
            choice("organize_repair").response[0]
                .text
                .contains("raw ash, cord, and iron")
        );
        assert!(
            choice("organize_repair")
                .result
                .contains("temporary splint")
        );
        assert!(
            choice("organize_repair")
                .result
                .contains("sufficient only for the immediate queued crossings")
        );

        for (route, virtue) in [
            ("pay_tribute", VirtueId::Prudence),
            ("barter_provisions", VirtueId::Courtesy),
            ("row_in_lieu", VirtueId::Courage),
            ("expose_altered_tally", VirtueId::Justice),
            ("organize_repair", VirtueId::Prudence),
        ] {
            assert_eq!(exemplified_virtue(&choice(route).personality), Some(virtue));
        }
        let theft = choice("steal_till");
        assert!(theft.response[0].text.contains("trust the charge"));
        assert!(theft.response[0].text.contains("till ashore"));
        assert!(theft.result.contains("keeping the shore tally"));
        assert!(
            theft
                .result
                .contains("repeatedly sculleth queued crossings")
        );
        assert!(
            theft
                .result
                .contains("abscondest with forty-eight groschen")
        );
        assert!(matches!(
            theft.effects[0],
            Effect::Currency { amount: 48, .. }
        ));
        assert!(theft.personality[0].delta < 0);
        assert_eq!(exemplified_virtue(&theft.personality), None);
    }

    #[test]
    fn rash_cliff_hunt_routes_are_mortal_observational_and_balanced() {
        let definition = encounter("rash_cliff_hunt_v1").unwrap();
        assert!(definition.triggers.travel && !definition.triggers.rest);
        assert_eq!(definition.cast[0].nature, SpeakerNature::Mortal);

        let opening = definition.opening[0].text.to_lowercase();
        assert!(opening.contains("chief beater"));
        assert!(opening.contains("safe anchors and a belayed order"));
        assert!(opening.contains("lacketh rank to command"));
        for withheld in ["lower spur", "bow", "ranged", "melee", "across the gap"] {
            assert!(!opening.contains(withheld));
        }

        let choice = |id| {
            definition
                .choices
                .iter()
                .find(|choice| choice.id == id)
                .unwrap()
        };
        let active = definition
            .choices
            .iter()
            .filter(|choice| choice.id != "ignore")
            .collect::<Vec<_>>();
        assert_eq!(active.len(), 6);
        assert!(active.iter().all(|choice| {
            choice.effects.iter().any(|effect| {
                matches!(
                    effect,
                    Effect::Information { information_id }
                        if information_id == "melee_only_opposition_cannot_answer_prepared_bow_lane"
                )
            }) && choice.quest_reward_tags == ["prepare_bows_against_melee_only_opposition"]
        }));
        assert!(active.iter().all(|choice| {
            let result = choice.result.to_lowercase().replace('-', " ");
            result.contains("lower spur")
                && result.contains("bow")
                && result.contains("exposed")
                && result.contains("mail clad squire")
                && result.contains("melee")
                && result.contains("cannot answer")
                && result.contains("gap")
        }));

        let spear = choice("climb_with_hunting_spear");
        assert!(matches!(
            spear.requirements.as_slice(),
            [Requirement::Item {
                item_id,
                minimum_quantity: 1
            }] if item_id == "hunting_spear"
        ));
        assert!(!spear.effects.iter().any(|effect| matches!(
            effect,
            Effect::ConsumeItem { item_id, .. } if item_id == "hunting_spear"
        )));
        assert!(spear.result.contains("keepest possession of it"));

        let command = choice("order_belayed_drive");
        assert!(
            command.response[0]
                .text
                .contains("chief beater knoweth the safe anchors")
        );
        assert!(
            command.response[0]
                .text
                .contains("Lend him thine authority")
        );
        assert!(
            command
                .result
                .contains("enforcing and coordinating the chief beater's belay plan")
        );
        assert!(command.result.contains("he chooseth the safe anchors"));

        assert!(
            choice("rebuke_vainglory").response[0]
                .text
                .starts_with("Thy rebuke doth recall me")
        );
        let theft = choice("rig_station_and_steal_stakes");
        assert!(matches!(
            theft.effects[0],
            Effect::Currency { amount: 48, .. }
        ));
        let largest_honest_coin = active
            .iter()
            .filter(|choice| choice.id != theft.id)
            .flat_map(|choice| &choice.effects)
            .filter_map(|effect| match effect {
                Effect::Currency { amount, .. } if *amount > 0 => Some(*amount),
                _ => None,
            })
            .max()
            .unwrap();
        assert!(48 > largest_honest_coin);
        assert!(theft.personality.iter().all(|change| change.delta < 0));
        assert_eq!(exemplified_virtue(&theft.personality), None);

        let ignore = choice("ignore");
        assert!(ignore.transition.is_none());
        assert!(ignore.effects.is_empty());
        assert!(ignore.personality.is_empty());
        let authored = serde_json::to_string(definition).unwrap().to_lowercase();
        for prohibited in [
            "buck is slain",
            "buck dieth",
            "quarry item",
            "equipment damage",
            "injureth",
        ] {
            assert!(!authored.contains(prohibited));
        }
    }

    #[test]
    fn envious_captain_routes_ground_oath_trust_and_portable_bow_lesson() {
        let definition = encounter("envious_captain_false_directions_v1").unwrap();
        let opening = format!(
            "{} {}",
            definition.opening[0].text, definition.opening[1].text
        )
        .to_lowercase();
        assert!(opening.contains("ridge-wise advance guard"));
        assert!(opening.contains("hold first passage"));
        assert!(opening.contains("saint christopher's road-brotherhood mark"));
        for withheld in ["bow", "missile", "sword", "spear", "cannot answer"] {
            assert!(!opening.contains(withheld));
        }

        let choice = |id| {
            definition
                .choices
                .iter()
                .find(|choice| choice.id == id)
                .unwrap()
        };
        let active = definition
            .choices
            .iter()
            .filter(|choice| choice.id != "ignore")
            .collect::<Vec<_>>();
        for route in &active {
            assert!(route.effects.iter().any(|effect| matches!(
                effect,
                Effect::Information { information_id }
                    if information_id == "melee_only_opposition_cannot_answer_prepared_bow_lane"
            )));
            assert!(route.quest_reward_tags == ["prepare_bows_against_melee_only_opposition"]);
            assert!(route.result.contains("prepared bow"));
            assert!(route.result.contains("cannot answer across the gap"));
            assert!(route.result.contains("yield"));
        }

        for id in [
            "read_false_waymarks",
            "scout_both_roads",
            "expose_before_companies",
            "organize_ridge_passage",
        ] {
            let route = choice(id);
            assert_eq!(route.response[0].speaker, "merchant_factor");
            assert!(route.result.contains("factor payeth"));
        }
        let oath = choice("demand_oath_at_cross");
        let oath_prose = format!("{} {}", oath.response[0].text, oath.result).to_lowercase();
        for mortal_consequence in [
            "road-brotherhood rule",
            "wages and standing",
            "publicly recant",
            "without supernatural judgment",
        ] {
            assert!(oath_prose.contains(mortal_consequence));
        }

        let theft = choice("collude_and_split_payroll");
        let mut cursor = 0;
        for event in [
            "advance guard scouteth ahead",
            "abandoning direct seizure",
            "lower-road deceit",
            "boggeth upright without overturn or injury",
            "stealest its ninety-six-groschen purse",
        ] {
            let found = theft.result[cursor..].find(event).unwrap();
            cursor += found + event.len();
        }
        assert!(matches!(
            theft.effects[0],
            Effect::Currency { amount: 48, .. }
        ));
        let honest_max = active
            .iter()
            .filter(|choice| choice.id != theft.id)
            .flat_map(|choice| &choice.effects)
            .filter_map(|effect| match effect {
                Effect::Currency { amount, .. } if *amount > 0 => Some(*amount),
                _ => None,
            })
            .max()
            .unwrap();
        assert!(48 > honest_max);
        assert!(theft.personality.iter().all(|change| change.delta < 0));
        assert_eq!(exemplified_virtue(&theft.personality), None);

        let ignore = choice("ignore");
        assert!(matches!(
            ignore.transition.as_ref(),
            Some(EncounterTransition::TravelDelay { minutes: 60 })
        ));
        assert!(ignore.response.is_empty() && ignore.effects.is_empty());
        assert!(ignore.outcome_tags.is_empty() && ignore.personality.is_empty());
    }

    #[test]
    fn damsel_and_steward_routes_reward_grounded_counsel_without_identity_abuse() {
        let definition = encounter("insulting_damsel_and_dwarf_v1").unwrap();
        let choice = |id| {
            definition
                .choices
                .iter()
                .find(|choice| choice.id == id)
                .unwrap()
        };
        let active = definition
            .choices
            .iter()
            .filter(|choice| choice.id != "ignore")
            .collect::<Vec<_>>();
        for route in &active {
            assert!(route.effects.iter().any(|effect| matches!(
                effect,
                Effect::Information { information_id }
                    if information_id == "melee_only_opposition_cannot_answer_prepared_bow_lane"
            )));
            assert!(route.effects.iter().any(|effect| matches!(
                effect,
                Effect::GrantItem { item_id, quantity: 4 } if item_id == "arrow"
            )));
            assert!(route.quest_reward_tags == ["prepare_bows_against_melee_only_opposition"]);
            assert!(route.result.contains("steward") && route.result.contains("planted by hand"));
        }
        assert!(
            !serde_json::to_string(definition)
                .unwrap()
                .contains("household archers")
        );

        assert!(matches!(
            choice("endure_and_verify").checks[0],
            Check::Skill {
                skill: SkillId::Will,
                difficulty_milli: 1200
            }
        ));
        let command = choice("lend_steward_authority");
        assert!(
            command
                .result
                .contains("beneath the steward's own terrain plan")
        );
        let theft = choice("invent_forester_toll_and_keep_purse");
        assert!(matches!(
            theft.effects[0],
            Effect::Currency { amount: 48, .. }
        ));
        assert!(theft.personality.iter().all(|change| change.delta < 0));
        assert_eq!(exemplified_virtue(&theft.personality), None);

        let ignore = choice("ignore");
        assert!(ignore.transition.is_none());
        assert!(ignore.effects.is_empty() && ignore.personality.is_empty());
    }

    #[test]
    fn stolen_animals_routes_share_real_bow_preparation_without_persisting_animals() {
        let definition = encounter("stolen_lapdog_prize_horse_v1").unwrap();
        let choice = |id| {
            definition
                .choices
                .iter()
                .find(|choice| choice.id == id)
                .unwrap()
        };
        let active = definition
            .choices
            .iter()
            .filter(|choice| choice.id != "ignore")
            .collect::<Vec<_>>();
        for route in &active {
            assert!(route.effects.iter().any(|effect| matches!(
                effect,
                Effect::Information { information_id }
                    if information_id == "melee_only_opposition_cannot_answer_prepared_bow_lane"
            )));
            assert!(route.effects.iter().any(|effect| matches!(
                effect,
                Effect::GrantItem { item_id, quantity: 1 } if item_id == "self_bow"
            )));
            assert!(route.effects.iter().any(|effect| matches!(
                effect,
                Effect::GrantItem { item_id, quantity: 8 } if item_id == "arrow"
            )));
            assert!(route.quest_reward_tags == ["prepare_bows_against_melee_only_opposition"]);
        }
        let bow = crate::item_catalog::definition("self_bow").unwrap();
        let arrow = crate::item_catalog::definition("arrow").unwrap();
        assert_eq!(bow.base_value + arrow.base_value * 8, 16);
        let command = choice("coordinate_bloodless_recovery");
        let command_prose = format!("{} {}", command.response[0].text, command.result);
        assert!(command_prose.contains("I shall choose the ditch, recall, and animal handling"));
        assert!(command_prose.contains("coordinating witnesses and servants"));
        let ration = choice("lure_dog_for_recall_proof");
        assert!(matches!(
            ration.requirements[0],
            Requirement::Item { ref item_id, minimum_quantity: 1 } if item_id == "travel_ration"
        ));
        assert!(matches!(
            ration.effects[0],
            Effect::ConsumeItem { ref item_id, quantity: 1 } if item_id == "travel_ration"
        ));
        let theft = choice("validate_tally_and_take_commission");
        assert!(matches!(
            theft.effects[0],
            Effect::Currency { amount: 48, .. }
        ));
        assert!(theft.personality.iter().all(|change| change.delta < 0));
        assert_eq!(exemplified_virtue(&theft.personality), None);
        let mut cursor = 0;
        for event in [
            "Under color of neutral custody",
            "ordereth her ditch bowman away",
            "draweth thee aside beyond the chatelaine's hearing and quietly offereth forty-eight",
            "falsely rulest",
            "discreetly payeth the promised forty-eight",
            "stealest the entrusted bow and arrows and departest. Dog and bay pass away with the coper",
        ] {
            let found = theft.result[cursor..].find(event).unwrap();
            cursor += found + event.len();
        }
        let effects = active
            .iter()
            .flat_map(|route| &route.effects)
            .collect::<Vec<_>>();
        let effects = serde_json::to_string(&effects).unwrap();
        assert!(!effects.contains("dog") && !effects.contains("horse") && !effects.contains("bay"));
        let ignore = choice("ignore");
        assert!(ignore.transition.is_none());
        assert!(ignore.effects.is_empty() && ignore.personality.is_empty());
    }

    #[test]
    fn enchanted_fog_routes_are_local_grounded_and_share_bow_preparation() {
        let definition = encounter("enchanted_fog_lost_forester_v1").unwrap();
        assert!(definition.triggers.travel && definition.triggers.rest);
        let lady = definition
            .opening
            .iter()
            .filter(|line| line.speaker == "white_mere_lady")
            .collect::<Vec<_>>();
        assert_eq!(lady.len(), 3);
        assert!(lady.iter().all(|line| line.reviewed_iambic_pentameter));
        assert!(
            definition
                .choices
                .iter()
                .flat_map(|choice| &choice.response)
                .all(|line| line.speaker != "white_mere_lady")
        );
        let choice = |id| {
            definition
                .choices
                .iter()
                .find(|choice| choice.id == id)
                .unwrap()
        };
        for route in definition
            .choices
            .iter()
            .filter(|choice| choice.id != "ignore")
        {
            let effects = serde_json::to_string(&route.effects).unwrap();
            assert!(effects.contains(r#""item_id":"self_bow","quantity":1"#));
            assert!(effects.contains(r#""item_id":"arrow","quantity":8"#));
            assert!(effects.contains("melee_only_opposition_cannot_answer_prepared_bow_lane"));
            assert_eq!(
                route.quest_reward_tags,
                ["prepare_bows_against_melee_only_opposition"]
            );
        }
        assert!(crate::item_catalog::definition("self_bow").is_some());
        assert!(crate::item_catalog::definition("arrow").is_some());
        let command = choice("coordinate_reeves_mist_line");
        assert!(
            command
                .result
                .contains("reeve alone chooseth drainage, stakes, route, and order")
        );
        let faith = choice("keep_shared_litany");
        assert!(
            faith.effects.len() == 3
                && faith.effects.iter().all(|effect| matches!(
                    effect,
                    Effect::GrantItem { .. } | Effect::Information { .. }
                ))
        );
        let evil = choice("steal_reeves_wages_and_hunt_gear");
        assert!(
            evil.effects
                .iter()
                .any(|effect| matches!(effect, Effect::Currency { amount: 48, .. }))
        );
        assert!(evil.personality.len() == 1 && evil.personality[0].delta < 0);
        let ignore = choice("ignore");
        assert!(matches!(
            ignore.transition,
            Some(EncounterTransition::TravelDelay { minutes: 180 })
        ));
    }

    #[test]
    fn maiden_roadside_court_routes_share_real_shield_preparation() {
        let definition = encounter("maiden_roadside_court_v1").unwrap();
        assert!(definition.triggers.travel && definition.triggers.rest);
        assert!(definition.opening[0].text.contains("sergeant accuseth me"));
        let choice = |id| {
            definition
                .choices
                .iter()
                .find(|choice| choice.id == id)
                .unwrap()
        };
        let active = definition
            .choices
            .iter()
            .filter(|choice| choice.id != "ignore")
            .collect::<Vec<_>>();
        for route in &active {
            let effects = serde_json::to_string(&route.effects).unwrap();
            assert!(effects.contains(r#""item_id":"heater_shield","quantity":1"#));
            assert!(effects.contains("heater_shield_holds_blade_only_assault"));
            assert_eq!(
                route.quest_reward_tags,
                ["prepare_heater_shield_against_blade_only_opposition"]
            );
        }
        let shield = crate::item_catalog::definition("heater_shield").unwrap();
        assert!(matches!(
            &shield.kind,
            crate::item_catalog::ItemKind::Shield { block, .. } if *block > 0.0
        ));
        assert_eq!(shield.base_value, 12);
        let retainer = crate::bestiary::ThreatId::ArmedRetainer.profile();
        assert_eq!(retainer.combat.attack, crate::bestiary::AttackStyle::Blade);
        assert!(!retainer.combat.ranged);
        let command = choice("order_hearing_under_clerks_judgment");
        assert!(command.response[0].text.contains("I alone shall judge"));
        let religion = choice("authenticate_maidens_alms_safe_conduct");
        assert!(religion.result.contains("ordinary seal"));
        assert_eq!(religion.effects.len(), 2);
        let surety = choice("champion_maiden_in_controlled_blade_test");
        assert!(matches!(
            surety.checks[0],
            Check::Skill {
                skill: SkillId::Will,
                difficulty_milli: 1200
            }
        ));
        let evil = choice("suppress_verge_proof_against_maiden");
        assert!(
            evil.effects
                .iter()
                .any(|effect| matches!(effect, Effect::Currency { amount: 48, .. }))
        );
        assert!(evil.personality.len() == 1 && evil.personality[0].delta < 0);
        assert_eq!(48 + shield.base_value, 5 * shield.base_value);
        let response = evil.response[0].text.to_lowercase();
        assert!(
            !["wedge", "purse", "suppress", "under color"]
                .iter()
                .any(|term| response.contains(term))
        );
        for tag in [
            "exclusive_physical_evidence",
            "private_personal_bribe",
            "clerk_applied_corrupted_record",
            "material_harm_to_accused_maiden",
        ] {
            assert!(evil.outcome_tags.iter().any(|found| found == tag));
        }
        let evil_effects = serde_json::to_string(&evil.effects).unwrap();
        assert!(!evil_effects.contains("wagon") && !evil_effects.contains("person"));
        let ignore = choice("ignore");
        assert!(ignore.transition.is_none());
        assert!(ignore.effects.is_empty() && ignore.personality.is_empty());
    }

    #[test]
    fn brachet_routes_share_grounded_doublet_preparation_and_real_loot() {
        let definition = encounter("brachet_leads_to_slain_knight_v1").unwrap();
        assert!(definition.triggers.travel && definition.triggers.rest);
        assert_eq!(definition.cast.len(), 1);
        assert_eq!(definition.cast[0].nature, SpeakerNature::Mortal);
        let authored = serde_json::to_string(definition).unwrap();
        assert!(!authored.contains(r#""reviewed_shakespearean":false"#));
        assert!(!authored.contains(r#""reviewed_iambic_pentameter":true"#));
        let choice = |id| {
            definition
                .choices
                .iter()
                .find(|choice| choice.id == id)
                .unwrap()
        };
        let active = definition
            .choices
            .iter()
            .filter(|choice| choice.id != "ignore")
            .collect::<Vec<_>>();
        for route in &active {
            let effects = serde_json::to_string(&route.effects).unwrap();
            assert!(effects.contains(r#""item_id":"arming_doublet","quantity":1"#));
            assert!(effects.contains("arming_doublet_resists_close_blade_hits_on_covered_chest"));
            assert!(
                !["dog", "brachet", "corpse", "body"]
                    .iter()
                    .any(|id| effects.contains(id))
            );
            assert_eq!(
                route.quest_reward_tags,
                ["prepare_arming_doublet_against_blade_only_opposition"]
            );
        }
        let doublet = crate::item_catalog::definition("arming_doublet").unwrap();
        assert_eq!(doublet.base_value, 12);
        assert!(matches!(&doublet.kind,
            crate::item_catalog::ItemKind::Armor { coverage, resistance, padding, .. }
                if *coverage > 0.0 && *resistance > 0.0 && *padding > 0.0));
        let topology = serde_json::to_string(&doublet.equipment).unwrap();
        assert!(topology.contains(r#""location":"chest","channel":"padding"#));
        assert!(topology.contains(r#""protection":["chest"]"#));
        let retainer = crate::bestiary::ThreatId::ArmedRetainer.profile();
        assert_eq!(retainer.combat.attack, crate::bestiary::AttackStyle::Blade);
        assert!(!retainer.combat.ranged);
        let command = choice("rally_orderly_recovery");
        assert!(command.response[0].text.contains("I alone shall choose"));
        let faith = choice("keep_ordinary_christian_vigil");
        assert_eq!(faith.effects.len(), 2);
        assert!(
            faith
                .outcome_tags
                .iter()
                .any(|tag| tag == "ordinary_body_office")
        );
        let evil = choice("misdirect_master_and_strip_dead");
        assert!(matches!(
            evil.effects[0],
            Effect::Currency { amount: 96, .. }
        ));
        let evil_effects = serde_json::to_string(&evil.effects).unwrap();
        assert!(evil_effects.contains("mail_shirt") && evil_effects.contains("war_hammer"));
        assert!(evil.personality.len() == 1 && evil.personality[0].delta < 0);
        assert_eq!(exemplified_virtue(&evil.personality), None);
        let mail = crate::item_catalog::definition("mail_shirt").unwrap();
        let hammer = crate::item_catalog::definition("war_hammer").unwrap();
        assert_eq!(
            96 + mail.base_value + hammer.base_value + doublet.base_value,
            177
        );
        assert!(177 > 14 * doublet.base_value);
        let ignore = choice("ignore");
        assert!(ignore.transition.is_none());
        assert!(ignore.effects.is_empty() && ignore.personality.is_empty());
    }

    #[test]
    fn hawthorn_sleep_routes_share_real_halberd_working_reach_without_sleep_state() {
        let definition = encounter("enchanted_sleep_beneath_hawthorn_v1").unwrap();
        assert!(definition.triggers.travel && definition.triggers.rest);
        assert_eq!(
            definition
                .opening
                .iter()
                .filter(|line| line.speaker == "lady_of_hawthorn")
                .count(),
            3
        );
        assert!(
            definition
                .choices
                .iter()
                .flat_map(|choice| &choice.response)
                .all(|line| line.speaker != "lady_of_hawthorn")
        );
        let choice = |id| {
            definition
                .choices
                .iter()
                .find(|choice| choice.id == id)
                .unwrap()
        };
        let active = definition
            .choices
            .iter()
            .filter(|choice| choice.id != "ignore")
            .collect::<Vec<_>>();
        assert!(active.len() == 7 && active.iter().all(|route| route.checks.len() == 1));
        for route in &active {
            let effects = serde_json::to_string(&route.effects).unwrap();
            assert!(effects.contains(r#""item_id":"halberd","quantity":1"#));
            assert!(effects.contains("halberd_provides_two_meter_working_reach"));
            assert_eq!(route.quest_reward_tags, ["prepare_halberd_for_long_reach"]);
            assert!(
                !["sleep", "stake", "cart", "weather"]
                    .iter()
                    .any(|id| effects.contains(id))
            );
        }
        let halberd = crate::item_catalog::definition("halberd").unwrap();
        let weapon = serde_json::to_string(&halberd.kind).unwrap();
        assert!(weapon.contains("\"reach_m\":2.0") && weapon.contains("\"penetration\":2.0"));
        assert!(
            ["blunt", "slash", "pierce"]
                .iter()
                .all(|damage| weapon.contains(damage))
        );
        let tagged =
            |route: &EncounterChoice, tag| route.outcome_tags.iter().any(|found| found == tag);
        let command = choice("coordinate_silent_rescue_relay");
        assert!(tagged(command, "mistress_owned_triage_and_lifting"));
        let faith = choice("keep_litany_carry_cadence");
        assert!(tagged(faith, "ordinary_shared_cadence"));
        let evil = choice("steal_watch_cart_stores_and_release_rig");
        let evil_effects = serde_json::to_string(&evil.effects).unwrap();
        assert!(evil_effects.contains("\"amount\":96") && evil_effects.contains("jack_of_plates"));
        assert!(evil.personality.len() == 1 && evil.personality[0].delta < 0);
        assert_eq!(exemplified_virtue(&evil.personality), None);
        let jack = crate::item_catalog::definition("jack_of_plates").unwrap();
        assert_eq!(jack.base_value, 35);
        assert!(96 + jack.base_value + halberd.base_value == 155 && 155 > 6 * halberd.base_value);
        assert!(tagged(evil, "loot_secured_before_release"));
        assert!(
            tagged(evil, "sleepers_woke_safely")
                && tagged(evil, "departed_before_mistress_returned")
        );
        let ignore = choice("ignore");
        assert!(ignore.transition.is_none());
        assert!(ignore.effects.is_empty() && ignore.personality.is_empty());
    }

    #[test]
    fn ransom_convoy_routes_gate_mace_lesson_and_preserve_expert_authority() {
        let definition = encounter("halted_ransom_convoy_v1").unwrap();
        assert!(definition.triggers.travel && definition.triggers.rest);
        assert!(
            definition
                .provenance
                .works
                .iter()
                .any(|work| work.contains("Book IV") && work.contains("Chapter VII"))
        );
        assert!(
            definition
                .provenance
                .works
                .iter()
                .any(|work| work.contains("Book IV") && work.contains("Chapter XII"))
        );
        assert!(
            definition
                .cast
                .iter()
                .all(|speaker| speaker.nature == SpeakerNature::Mortal)
        );
        let mut dialogue = definition.opening.iter().chain(
            definition
                .choices
                .iter()
                .flat_map(|choice| &choice.response),
        );
        assert!(
            dialogue.all(|line| line.reviewed_shakespearean && !line.reviewed_iambic_pentameter)
        );
        let choice = |id| {
            definition
                .choices
                .iter()
                .find(|choice| choice.id == id)
                .unwrap()
        };
        let active = definition
            .choices
            .iter()
            .filter(|choice| choice.id != "ignore")
            .collect::<Vec<_>>();
        assert!(active.len() == 7 && active.iter().all(|route| route.checks.len() == 1));
        for route in &active {
            let effects = serde_json::to_string(&route.effects).unwrap();
            assert!(effects.contains(r#""item_id":"flanged_mace","quantity":1"#));
            assert!(effects.contains("pure_blunt_mace_bypasses_worn_armor_edge_resistance"));
            assert_eq!(
                route.quest_reward_tags,
                ["prepare_blunt_weapon_against_armored_blade_opposition"]
            );
            assert!(
                !["prisoner", "chain", "axle", "wagon", "roll", "key"]
                    .iter()
                    .any(|id| effects.contains(id))
            );
        }
        let mace = crate::item_catalog::definition("flanged_mace").unwrap();
        let weapon = serde_json::to_string(&mace.kind).unwrap();
        assert!(mace.base_value == 10 && weapon.contains(r#""damage_types":["blunt"]"#));
        assert!(weapon.contains(r#""melee":true"#) && weapon.contains(r#""ranged":false"#));
        let retainer = crate::bestiary::ThreatId::ArmedRetainer.profile();
        assert!(retainer.combat.protection == crate::bestiary::Protection::Armored);
        assert!(
            retainer.combat.attack == crate::bestiary::AttackStyle::Blade
                && !retainer.combat.ranged
        );
        let tagged = |route: &EncounterChoice, tag: &str| {
            route.outcome_tags.iter().any(|found| found == tag)
        };
        for spec in [
            "choose_dry_repair_shoulder terrain_plains sergeant_owned_repair ordinary_repair_completed",
            "identify_dangerous_binding_swelling physiology sergeant_owned_custody ordinary_repair_completed",
            "negotiate_immediate_sworn_parole charm clerk_owned_release_terms immediate_parole",
            "coordinate_guarded_axle_turns command clerk_owned_release_terms sergeant_owned_repair",
            "hold_neutral_line_for_prisoner_lift will sergeant_owned_custody ordinary_repair_completed",
            "swear_cross_marked_work_truce roman_catholic shared_confraternity_oath temporary_work_truce",
            "steal_custody_pay_and_release_escape deception stolen_custody_pay necessary_work_created_custody_gap",
        ] {
            let mut fields = spec.split_ascii_whitespace();
            let route = choice(fields.next().unwrap());
            assert!(
                serde_json::to_string(&route.checks)
                    .unwrap()
                    .contains(fields.next().unwrap())
            );
            assert!(fields.all(|tag| tagged(route, tag)));
        }
        let faith = choice("swear_cross_marked_work_truce");
        assert!(faith.effects.len() == 2 && tagged(faith, "clerk_owned_release_terms"));
        assert!(tagged(faith, "sergeant_owned_custody"));
        let evil = choice("steal_custody_pay_and_release_escape");
        let evil_effects = serde_json::to_string(&evil.effects).unwrap();
        assert!(
            evil_effects.contains(r#""amount":48"#) && 48 + mace.base_value > 5 * mace.base_value
        );
        assert!(evil.personality.len() == 1 && evil.personality[0].delta < 0);
        assert!(exemplified_virtue(&evil.personality).is_none());
        assert!(tagged(evil, "scene_local_prisoner_escape"));
        let ignore = choice("ignore");
        assert!(ignore.transition.is_none() && ignore.effects.is_empty());
        assert!(ignore.personality.is_empty() && ignore.outcome_tags.is_empty());
        assert!(ignore.quest_reward_tags.is_empty());
    }

    #[test]
    fn combat_transition_rejects_unsafe_counts() {
        let mut definition = encounter("unlawful_bridge_custom_v1").unwrap().clone();
        let choice = definition
            .choices
            .iter_mut()
            .find(|choice| choice.id == "challenge_to_arms")
            .unwrap();
        let Some(EncounterTransition::StartCombat { count, .. }) = &mut choice.transition else {
            panic!("combat transition")
        };
        *count = 0;
        assert!(
            validate_definitions(&[definition])
                .unwrap_err()
                .contains("unsafe combat count")
        );
    }

    #[test]
    fn non_victory_combat_payloads_reject_material_rewards() {
        let mut definition = encounter("unlawful_bridge_custom_v1").unwrap().clone();
        let choice = definition
            .choices
            .iter_mut()
            .find(|choice| choice.id == "challenge_to_arms")
            .unwrap();
        let Some(EncounterTransition::StartCombat { outcomes, .. }) = &mut choice.transition else {
            panic!("combat transition")
        };
        outcomes.surrender.effects.push(Effect::Currency {
            currency_id: "brandenburg_groschen".into(),
            amount: 1,
        });
        assert!(
            validate_definitions(&[definition])
                .unwrap_err()
                .contains("material reward on surrender")
        );
    }

    #[test]
    fn resolved_combat_outcomes_are_closed_and_distinguish_surrender() {
        assert_eq!(
            resolved_combat_outcome("surrendered"),
            Ok(CombatOutcomeKind::Surrender)
        );
        assert_eq!(
            resolved_combat_outcome("avoided"),
            Ok(CombatOutcomeKind::Escape)
        );
        assert!(resolved_combat_outcome("mysterious").is_err());
    }

    #[test]
    fn travel_delay_and_currency_requirements_are_bounded() {
        let mut delayed = encounter("unlawful_bridge_custom_v1").unwrap().clone();
        let ignore = delayed
            .choices
            .iter_mut()
            .find(|choice| choice.id == "ignore")
            .unwrap();
        ignore.transition = Some(EncounterTransition::TravelDelay { minutes: 0 });
        assert!(
            validate_definitions(&[delayed])
                .unwrap_err()
                .contains("unsafe travel delay")
        );

        let mut unpaid = encounter("unlawful_bridge_custom_v1").unwrap().clone();
        let toll = unpaid
            .choices
            .iter_mut()
            .find(|choice| choice.id == "pay_toll")
            .unwrap();
        toll.requirements = vec![Requirement::Currency { amount: 0 }];
        assert!(
            validate_definitions(&[unpaid])
                .unwrap_err()
                .contains("zero currency requirement")
        );
    }
}
