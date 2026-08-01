pub const ERRANTRY_ISSUER_ORGANIZATION_ID: &str = "order_saint_george";
pub const ERRANTRY_FINALE_THREAT_ID: &str = "armed_retainer";
pub const COURIER_REST_DELAY_MINUTES: u64 = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ChallengePresenterCatalogId {
    LadyBeneathThornV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ErrantryPuzzleKind {
    OrderedSigils,
    TruthfulWitnesses,
    RuneTransformation,
    LogicGrid,
    ResourceAllocation,
}

impl ErrantryPuzzleKind {
    fn core(self) -> adventuresim_core::errantry::PuzzleKind {
        match self {
            Self::OrderedSigils => adventuresim_core::errantry::PuzzleKind::OrderedSigils,
            Self::TruthfulWitnesses => adventuresim_core::errantry::PuzzleKind::TruthfulWitnesses,
            Self::RuneTransformation => adventuresim_core::errantry::PuzzleKind::RuneTransformation,
            Self::LogicGrid => adventuresim_core::errantry::PuzzleKind::LogicGrid,
            Self::ResourceAllocation => adventuresim_core::errantry::PuzzleKind::ResourceAllocation,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum NarrativeEncounterOrigin { ChanceTravel, ChanceRest, Errantry, DeveloperDemo }

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum NarrativeEncounterTrigger { Travel, Rest }

fn narrative_skill(skill: adventuresim_core::road_encounter_catalog::SkillId) -> adventuresim_core::skill::Skill {
    use adventuresim_core::{road_encounter_catalog::SkillId as Id, skill::Skill};
    match skill {
        Id::Will => Skill::Will, Id::Insight => Skill::Insight, Id::Charm => Skill::Charm,
        Id::Command => Skill::Command, Id::Deception => Skill::Deception,
        Id::Physiology => Skill::Physiology, Id::Stealth => Skill::Stealth,
        Id::TerrainPlains => Skill::TerrainPlains, Id::TerrainForest => Skill::TerrainForest,
        Id::TerrainHills => Skill::TerrainHills, Id::TerrainWetlands => Skill::TerrainWetlands,
        Id::Surgery => Skill::Surgery,
    }
}

fn narrative_skill_hours(skills: &crate::CharacterSkills, skill: adventuresim_core::road_encounter_catalog::SkillId) -> f32 {
    use adventuresim_core::road_encounter_catalog::SkillId as Id;
    match skill {
        Id::Will => skills.will_hours, Id::Insight => skills.insight_hours,
        Id::Charm => skills.charm_hours, Id::Command => skills.command_hours,
        Id::Deception => skills.deception_hours, Id::Physiology => skills.physiology_hours,
        Id::Stealth => skills.stealth_hours,
        Id::TerrainPlains => skills.terrain_plains_hours, Id::TerrainForest => skills.terrain_forest_hours,
        Id::TerrainHills => skills.terrain_hills_hours, Id::TerrainWetlands => skills.terrain_wetlands_hours,
        Id::Surgery => skills.surgery_hours,
    }
}

fn narrative_religion(_: adventuresim_core::road_encounter_catalog::ReligionId) -> adventuresim_world_schema::OfficialReligion {
    adventuresim_world_schema::OfficialReligion::RomanCatholic
}

fn narrative_axis(axis: adventuresim_core::road_encounter_catalog::PersonalityAxisId) -> crate::personality::PersonalityAxis {
    use adventuresim_core::road_encounter_catalog::PersonalityAxisId as Id;
    use crate::personality::PersonalityAxis as Axis;
    match axis { Id::Nerve => Axis::Nerve, Id::Drive => Axis::Drive, Id::Sociability => Axis::Sociability,
        Id::Conscience => Axis::Conscience, Id::SelfRegard => Axis::SelfRegard, Id::Conviction => Axis::Conviction,
        Id::Courtship => Axis::Courtship, Id::Transparency => Axis::Transparency }
}

fn narrative_virtue(virtue: adventuresim_core::road_encounter_catalog::VirtueId) -> crate::personality::ChivalricVirtue {
    use adventuresim_core::road_encounter_catalog::VirtueId as Id;
    use crate::personality::ChivalricVirtue as Virtue;
    match virtue { Id::Courage => Virtue::Courage, Id::Mercy => Virtue::Mercy, Id::Faith => Virtue::Faith,
        Id::Justice => Virtue::Justice, Id::Courtesy => Virtue::Courtesy, Id::Loyalty => Virtue::Loyalty,
        Id::Prudence => Virtue::Prudence, Id::Honesty => Virtue::Honesty }
}

fn narrative_attribute_check(ctx: &ReducerContext, character_id: u64, attribute: adventuresim_core::road_encounter_catalog::AttributeId) -> Result<f32, String> {
    use adventuresim_core::road_encounter_catalog::AttributeId as Id;
    let values = ctx.db.character_attributes().character_id().find(character_id).ok_or("Character attributes not found")?;
    Ok(match attribute { Id::Endurance => values.endurance, Id::Immunity => values.immunity,
        Id::Gut => values.gut, Id::Intelligence => values.intelligence, Id::Instinct => values.instinct,
        Id::Eyesight => values.eyesight, Id::Hearing => values.hearing })
}

fn apply_narrative_effect(ctx: &ReducerContext, occurrence_id: &str, party_id: &str, now: u64, effect: &adventuresim_core::road_encounter_catalog::Effect) -> Result<(), String> {
    use adventuresim_core::road_encounter_catalog::Effect;
    match effect {
        Effect::GrantItem { item_id, quantity } => add_to_party_inventory_checked(ctx, party_id, item_id, u32::from(*quantity)),
        Effect::Currency { currency_id, amount } if *amount > 0 && adventuresim_core::strategic_currency::is_currency_id(currency_id) =>
            add_to_party_inventory_checked(ctx, party_id, currency_id, *amount as u32),
        Effect::Currency { currency_id, amount } if *amount < 0 && adventuresim_core::strategic_currency::is_currency_id(currency_id) =>
            consume_party_currency(ctx, party_id, u64::from(amount.unsigned_abs())),
        Effect::Currency { .. } => Err("Encounter currency declaration is invalid".into()),
        Effect::ConsumeItem { item_id, quantity } => consume_narrative_party_item(ctx, party_id, item_id, u32::from(*quantity)),
        Effect::Information { information_id } => {
            let source_id = format!("narrative-information:{occurrence_id}:{information_id}");
            if ctx.db.narrative_encounter_information().source_id().find(&source_id).is_none() {
                ctx.db.narrative_encounter_information().insert(NarrativeEncounterInformation {
                    source_id, occurrence_id: occurrence_id.into(), party_id: party_id.into(),
                    information_id: information_id.clone(), learned_at_minute: now,
                });
            }
            Ok(())
        }
    }
}

fn consume_narrative_party_item(ctx: &ReducerContext, party_id: &str, item_id: &str, quantity: u32) -> Result<(), String> {
    if crate::inventory_amount::is_measured_item(ctx, item_id) || item_is_durable(ctx, item_id) {
        return Err("Narrative costs currently require an ordinary stackable item".into());
    }
    let mut stacks: Vec<_> = ctx.db.party_inventory_item().party_id().filter(party_id)
        .filter(|stack| stack.item_id == item_id).collect();
    stacks.sort_by_key(|stack| stack.id);
    if stacks.iter().map(|stack| u64::from(stack.quantity)).sum::<u64>() < u64::from(quantity) {
        return Err("The party lacks the declared encounter cost".into());
    }
    let mut remaining = quantity;
    for mut stack in stacks {
        if remaining == 0 { break; }
        let taken = remaining.min(stack.quantity);
        stack.quantity -= taken;
        remaining -= taken;
        if stack.quantity == 0 { ctx.db.party_inventory_item().id().delete(stack.id); }
        else { ctx.db.party_inventory_item().id().update(stack); }
    }
    Ok(())
}

/// Private case-level authority for an Order-issued errantry.
#[derive(Clone, Debug)]
#[table(accessor = errantry_authority)]
pub struct ErrantryAuthority {
    #[primary_key]
    pub case_id: String,
    pub contract_id: String,
    pub issuer_organization_id: String,
    pub issuer_resident_character_id: u64,
    pub issuer_settlement_id: String,
    pub issuer_location_id: String,
    pub finale_case_site_id: String,
    pub finale_hostile_group_id: String,
    pub preliminary_challenge_ids: Vec<String>,
    pub finale_defenses_json: String,
}

/// A chat-native, non-puzzle interruption that becomes available only after
/// resting at its bound road camp. Ignoring it requires no mutation; choosing
/// either response closes it without affecting the finale objective.
#[derive(Clone, Debug)]
#[table(accessor = road_challenge_authority)]
pub struct RoadChallengeAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub party_id: String,
    pub case_id: String,
    pub finale_case_site_id: String,
    pub finale_hostile_group_id: String,
    pub journey_departure_minute: u64,
    pub camp_movement_minute: u64,
    pub available_at_elapsed_minute: u64,
    pub catalog_id: String,
    pub catalog_revision: u32,
    pub catalog_digest: String,
    pub absolute_minute: u64,
    pub longitude_e7: i32,
    pub latitude_e7: i32,
    pub trigger: NarrativeEncounterTrigger,
    pub revision: u32,
    pub open: bool,
    pub resolved_choice: Option<String>,
    pub resolved_deed: Option<String>,
    pub virtue_exemplified: Option<crate::personality::ChivalricVirtue>,
    pub result_transcript: Option<String>,
}

/// Private origin and optional quest overlay. It is intentionally absent from
/// the public projection until a post-resolution reward addendum is emitted.
#[derive(Clone, Debug)]
#[table(accessor = narrative_encounter_private_authority)]
pub struct NarrativeEncounterPrivateAuthority {
    #[primary_key]
    pub occurrence_id: String,
    pub origin: NarrativeEncounterOrigin,
    pub case_id: Option<String>,
    pub finale_case_site_id: Option<String>,
    pub finale_hostile_group_id: Option<String>,
    pub reward_eligible: bool,
    pub reward_addendum: Option<String>,
}

#[derive(Clone, Debug)]
#[table(accessor = road_challenge_resolution_receipt)]
pub struct RoadChallengeResolutionReceipt {
    #[primary_key]
    pub id: String,
    pub challenge_id: String,
    pub party_id: String,
    pub character_id: u64,
    pub action_id: String,
    pub choice: String,
    pub deed: String,
    pub virtue_exemplified: Option<crate::personality::ChivalricVirtue>,
    pub catalog_revision: u32,
    pub catalog_digest: String,
    pub result_transcript: String,
    pub effects_json: String,
    pub resolved_at_minute: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = narrative_encounter_information)]
pub struct NarrativeEncounterInformation {
    #[primary_key]
    pub source_id: String,
    #[index(btree)]
    pub occurrence_id: String,
    pub party_id: String,
    pub information_id: String,
    pub learned_at_minute: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendRoadChallenge {
    pub id: String,
    pub owner_character_id: u64,
    pub absolute_minute: u64,
    pub presentation_json: String,
    pub revision: u32,
    pub open: bool,
    pub active: bool,
    pub result_transcript: Option<String>,
    pub quest_reward_addendum: Option<String>,
}

#[derive(Clone, Debug)]
#[table(accessor = order_errantry_acceptance_receipt)]
pub struct OrderErrantryAcceptanceReceipt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub dialogue_session_id: String,
    pub action_id: String,
    pub character_id: u64,
    pub case_id: String,
    pub contract_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ErrantryLaunch {
    NormalTravel,
    DirectDemoCamp(ErrantryPuzzleKind),
}

struct MaterializedErrantry {
    case_id: String,
    contract_id: String,
}

/// Private deterministic challenge authority. `puzzle_json` contains the seed
/// and canonical ordering and must never appear in a public table or view.
#[derive(Clone, Debug)]
#[table(accessor = challenge_authority)]
pub struct ChallengeAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub gateway_bucket: u8,
    #[index(btree)]
    pub case_id: String,
    #[index(btree)]
    pub party_id: String,
    pub finale_case_site_id: String,
    pub finale_hostile_group_id: String,
    pub journey_departure_minute: u64,
    pub camp_movement_minute: u64,
    pub camp_elapsed_minute: u64,
    pub errantry_frame_json: String,
    pub puzzle_json: String,
    pub presenter_catalog_id: ChallengePresenterCatalogId,
    pub revision: u32,
    pub open: bool,
    pub solved_at_minute: Option<u64>,
}

/// Durable source/revision receipt. Wrong attempts are retained and retryable;
/// a receipt is immutable and an exact reducer retry is idempotent.
#[derive(Clone, Debug)]
#[table(accessor = challenge_attempt_receipt)]
pub struct ChallengeAttemptReceipt {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub challenge_id: String,
    pub case_id: String,
    pub party_id: String,
    pub character_id: u64,
    pub submitted_revision: u32,
    pub submission_json: String,
    pub correct: bool,
    pub resulting_revision: u32,
    pub attempted_at_minute: u64,
}

/// Trusted-gateway, observer-bound projection. Puzzle seed and canonical
/// assignment are absent by construction.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendChallenge {
    pub id: String,
    pub case_id: String,
    pub party_id: String,
    pub owner_character_id: u64,
    pub finale_case_site_id: String,
    pub puzzle_projection_json: String,
    pub presenter_catalog_id: ChallengePresenterCatalogId,
    pub revision: u32,
    pub open: bool,
    pub solved: bool,
    pub active: bool,
    pub last_attempt_correct: Option<bool>,
    pub last_submission_json: Option<String>,
    pub tactical_insight_text: Option<String>,
    pub tactical_preparation_text: Option<String>,
}

fn bound_tactical_insight(
    ctx: &ViewContext,
    challenge: &ChallengeAuthority,
) -> Option<adventuresim_core::errantry::TacticalInsight> {
    challenge.solved_at_minute?;
    let hostile_group = ctx
        .db
        .hostile_group_authority()
        .id()
        .find(&challenge.finale_hostile_group_id)?;
    let threat = hostile_group
        .enemy_type
        .parse::<adventuresim_core::bestiary::ThreatId>()
        .ok()?;
    adventuresim_core::errantry::tactical_insight_for(threat)
}

#[view(accessor = backend_challenges, public)]
pub fn backend_challenges(ctx: &ViewContext) -> Vec<BackendChallenge> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .challenge_authority()
        .gateway_bucket()
        .filter(0u8)
        .filter_map(|challenge| {
            let party = ctx.db.party_authority().id().find(&challenge.party_id)?;
            let puzzle: adventuresim_core::errantry::PuzzleAuthority =
                serde_json::from_str(&challenge.puzzle_json).ok()?;
            puzzle.validate().ok()?;
            let projection = serde_json::to_string(&puzzle.projection()).ok()?;
            let accepted = party
                .active_contract_id
                .as_ref()
                .and_then(|id| ctx.db.contract_authority().id().find(id))
                .is_some_and(|contract| {
                    contract.case_id == challenge.case_id
                        && contract.status == ContractStatus::Accepted
                        && contract.accepted_by.as_deref() == Some(&challenge.party_id)
                });
            let active = accepted && party_at_bound_trial_camp_view(ctx, &party, &challenge);
            let last_attempt = ctx
                .db
                .challenge_attempt_receipt()
                .challenge_id()
                .filter(&challenge.id)
                .max_by_key(|receipt| receipt.submitted_revision);
            let tactical_insight = bound_tactical_insight(ctx, &challenge);
            Some(BackendChallenge {
                id: challenge.id.clone(),
                case_id: challenge.case_id,
                party_id: challenge.party_id,
                owner_character_id: party.leader_id,
                finale_case_site_id: challenge.finale_case_site_id,
                puzzle_projection_json: projection,
                presenter_catalog_id: challenge.presenter_catalog_id,
                revision: challenge.revision,
                open: challenge.open,
                solved: challenge.solved_at_minute.is_some(),
                active,
                last_attempt_correct: last_attempt.as_ref().map(|receipt| receipt.correct),
                last_submission_json: last_attempt.map(|receipt| receipt.submission_json),
                tactical_insight_text: tactical_insight
                    .as_ref()
                    .map(|insight| insight.finding.clone()),
                tactical_preparation_text: tactical_insight
                    .map(|insight| insight.preparation),
            })
        })
        .collect()
}

#[view(accessor = backend_road_challenges, public)]
pub fn backend_road_challenges(ctx: &ViewContext) -> Vec<BackendRoadChallenge> {
    if !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .road_challenge_authority()
        .gateway_bucket()
        .filter(0u8)
        .filter_map(|challenge| {
            let party = ctx.db.party_authority().id().find(&challenge.party_id)?;
            let definition = adventuresim_core::road_encounter_catalog::encounter(&challenge.catalog_id)?;
            if definition.version != challenge.catalog_revision
                || challenge.catalog_digest != adventuresim_core::road_encounter_catalog::digest()
            { return None; }
            let private = ctx.db.narrative_encounter_private_authority()
                .occurrence_id().find(&challenge.id)?;
            let active = party_at_bound_road_challenge_view(ctx, &party, &challenge);
            let skills = ctx.db.character_skills().character_id().find(party.leader_id);
            let available = |requirement: &adventuresim_core::road_encounter_catalog::Requirement| match requirement {
                adventuresim_core::road_encounter_catalog::Requirement::Skill { skill, minimum_hours } =>
                    skills.as_ref().is_some_and(|skills| narrative_skill_hours(skills, *skill).is_finite()
                        && narrative_skill_hours(skills, *skill) >= *minimum_hours as f32),
                adventuresim_core::road_encounter_catalog::Requirement::Religion { religion } =>
                    skills.as_ref().is_some_and(|skills| {
                        let hours = skills.religion_hours.direct(narrative_religion(*religion));
                        hours.is_finite() && hours > 0.0
                    }),
                adventuresim_core::road_encounter_catalog::Requirement::Item { item_id, minimum_quantity } =>
                    ctx.db.party_inventory_item().party_id().filter(&party.id)
                        .filter(|stack| stack.item_id == *item_id)
                        .map(|stack| stack.quantity).sum::<u32>() >= u32::from(*minimum_quantity),
            };
            let line = |line: &adventuresim_core::road_encounter_catalog::SpokenLine| {
                let speaker = definition.cast.iter().find(|speaker| speaker.id == line.speaker)?;
                Some(adventuresim_core::road_encounter_catalog::PresentationLine {
                    speaker_name: speaker.name.clone(), text: line.text.clone(),
                    supernatural: speaker.nature == adventuresim_core::road_encounter_catalog::SpeakerNature::Supernatural,
                })
            };
            let selected = challenge.resolved_choice.as_deref()
                .and_then(|id| definition.choices.iter().find(|choice| choice.id == id));
            let presentation = adventuresim_core::road_encounter_catalog::EncounterPresentation {
                opening: definition.opening.iter().filter_map(line).collect(),
                choices: if challenge.open { definition.choices.iter().map(|choice| adventuresim_core::road_encounter_catalog::PresentationChoice {
                    id: choice.id.clone(), label: choice.label.clone(),
                    available: choice.requirements.iter().all(|requirement| available(requirement)),
                }).collect() } else { Vec::new() },
                response: selected.into_iter().flat_map(|choice| choice.response.iter()).filter_map(line).collect(),
            };
            let presentation_json = serde_json::to_string(&presentation).ok()?;
            Some(BackendRoadChallenge {
                id: challenge.id.clone(),
                owner_character_id: party.leader_id,
                absolute_minute: challenge.absolute_minute,
                presentation_json,
                revision: challenge.revision,
                open: challenge.open,
                active,
                result_transcript: challenge.result_transcript,
                quest_reward_addendum: private.reward_addendum,
            })
        })
        .collect()
}

fn journey_destination_matches(endpoint: &JourneyEndpoint, case_site_id: &str) -> bool {
    matches!(
        endpoint,
        JourneyEndpoint::CaseSite(site) if site.id.value == case_site_id
    )
}

fn journey_at_bound_trial_camp(
    journey: &PartyJourney,
    challenge: &ChallengeAuthority,
) -> bool {
    adventuresim_core::errantry::journey_camp_identity_matches(
        journey.departure_minute,
        journey.completed_minutes,
        &journey.camp_stop_minutes,
        challenge.journey_departure_minute,
        challenge.camp_movement_minute,
    )
        && journey_destination_matches(&journey.destination, &challenge.finale_case_site_id)
}

fn journey_at_bound_road_challenge(
    journey: &PartyJourney,
    challenge: &RoadChallengeAuthority,
    origin: NarrativeEncounterOrigin,
) -> bool {
    match origin {
        NarrativeEncounterOrigin::ChanceTravel => journey.departure_minute == challenge.journey_departure_minute
            && journey.completed_minutes == challenge.camp_movement_minute
            && journey.completed_elapsed_minutes == challenge.available_at_elapsed_minute,
        NarrativeEncounterOrigin::ChanceRest | NarrativeEncounterOrigin::Errantry | NarrativeEncounterOrigin::DeveloperDemo =>
            adventuresim_core::errantry::rested_road_trial_camp_matches(
                journey.departure_minute, journey.completed_minutes, journey.completed_elapsed_minutes,
                &journey.camp_stop_minutes, challenge.journey_departure_minute,
                challenge.camp_movement_minute, challenge.available_at_elapsed_minute),
    }
}

fn party_at_bound_trial_camp_view(
    ctx: &ViewContext,
    party: &Party,
    challenge: &ChallengeAuthority,
) -> bool {
    party.current_settlement_id.is_none()
        && party.current_case_site_id.is_none()
        && party.camp_destination.is_some()
        && ctx
            .db
            .party_journey_authority()
            .party_id()
            .find(&party.id)
            .is_some_and(|journey| journey_at_bound_trial_camp(&journey, challenge))
        && !ctx
            .db
            .strategic_encounter()
            .party_id()
            .find(&party.id)
            .is_some_and(|encounter| encounter.status == "awaiting_choice")
}

fn party_at_bound_trial_camp(
    ctx: &ReducerContext,
    party: &Party,
    challenge: &ChallengeAuthority,
) -> bool {
    party.current_settlement_id.is_none()
        && party.current_case_site_id.is_none()
        && party.camp_destination.is_some()
        && ctx
            .db
            .party_journey_authority()
            .party_id()
            .find(&party.id)
            .is_some_and(|journey| journey_at_bound_trial_camp(&journey, challenge))
        && !ctx
            .db
            .strategic_encounter()
            .party_id()
            .find(&party.id)
            .is_some_and(|encounter| encounter.status == "awaiting_choice")
}

fn party_at_bound_road_challenge_view(
    ctx: &ViewContext,
    party: &Party,
    challenge: &RoadChallengeAuthority,
) -> bool {
    let Some(private) = ctx.db.narrative_encounter_private_authority().occurrence_id().find(&challenge.id) else { return false; };
    party.current_settlement_id.is_none()
        && party.current_case_site_id.is_none()
        && party.camp_destination.is_some()
        && ctx
            .db
            .party_journey_authority()
            .party_id()
            .find(&party.id)
            .is_some_and(|journey| journey_at_bound_road_challenge(&journey, challenge, private.origin))
        && !ctx
            .db
            .strategic_encounter()
            .party_id()
            .find(&party.id)
            .is_some_and(|encounter| encounter.status == "awaiting_choice")
}

fn party_at_bound_road_challenge(
    ctx: &ReducerContext,
    party: &Party,
    challenge: &RoadChallengeAuthority,
) -> bool {
    let Some(private) = ctx.db.narrative_encounter_private_authority().occurrence_id().find(&challenge.id) else { return false; };
    party.current_settlement_id.is_none()
        && party.current_case_site_id.is_none()
        && party.camp_destination.is_some()
        && ctx
            .db
            .party_journey_authority()
            .party_id()
            .find(&party.id)
            .is_some_and(|journey| journey_at_bound_road_challenge(&journey, challenge, private.origin))
        && !ctx
            .db
            .strategic_encounter()
            .party_id()
            .find(&party.id)
            .is_some_and(|encounter| encounter.status == "awaiting_choice")
}

pub(crate) fn materialize_chance_narrative_encounter(
    ctx: &ReducerContext,
    party_id: &str,
    selection: &adventuresim_core::encounter::NarrativeSelection,
    origin: NarrativeEncounterOrigin,
) -> Result<(), String> {
    require_no_unresolved_encounter(ctx, party_id)?;
    let journey = ctx.db.party_journey_authority().party_id().find(&party_id.to_string())
        .ok_or("Narrative encounter requires a durable journey")?;
    let definition = adventuresim_core::road_encounter_catalog::encounter(&selection.catalog_id)
        .ok_or("Narrative encounter selection has an unknown catalog ID")?;
    let route = ctx.db.party_journey_route_authority().party_id().find(&party_id.to_string());
    let position = route.as_ref().and_then(|route| route_position_at_minute(route, journey.completed_minutes))
        .unwrap_or_else(|| journey_fallback_position(ctx, &journey, journey.completed_minutes));
    let seed = ctx.db.party_journey_encounter_authority().party_id().find(&party_id.to_string())
        .ok_or("Narrative encounter requires durable encounter entropy")?.seed;
    let origin_slug = match origin { NarrativeEncounterOrigin::ChanceTravel => "travel", NarrativeEncounterOrigin::ChanceRest => "rest", NarrativeEncounterOrigin::Errantry => "errantry", NarrativeEncounterOrigin::DeveloperDemo => "developer-demo" };
    let id = format!("narrative:{party_id}:{seed:016x}:{origin_slug}:{}:{}:{}:{}:{}",
        journey.departure_minute, selection.boundary_minute, journey.completed_minutes,
        journey.completed_elapsed_minutes, selection.roll_index);
    if let Some(existing) = ctx.db.road_challenge_authority().id().find(&id) {
        let private = ctx.db.narrative_encounter_private_authority().occurrence_id().find(&id)
            .ok_or("Narrative encounter identity collision lacks private authority")?;
        if existing.party_id == party_id && existing.catalog_id == definition.id
            && existing.journey_departure_minute == journey.departure_minute
            && existing.camp_movement_minute == journey.completed_minutes
            && existing.available_at_elapsed_minute == journey.completed_elapsed_minutes
            && private.origin == origin { return Ok(()); }
        return Err("Narrative encounter identity collision".into());
    }
    ctx.db.road_challenge_authority().insert(RoadChallengeAuthority {
        id: id.clone(), gateway_bucket: 0, party_id: party_id.into(), case_id: String::new(),
        finale_case_site_id: String::new(), finale_hostile_group_id: String::new(),
        journey_departure_minute: journey.departure_minute,
        camp_movement_minute: journey.completed_minutes,
        available_at_elapsed_minute: journey.completed_elapsed_minutes,
        catalog_id: definition.id.clone(), catalog_revision: definition.version,
        catalog_digest: adventuresim_core::road_encounter_catalog::digest().into(),
        absolute_minute: journey.departure_minute.saturating_add(journey.completed_elapsed_minutes),
        longitude_e7: (position.0 * 10_000_000.0).round() as i32,
        latitude_e7: (position.1 * 10_000_000.0).round() as i32,
        trigger: match origin { NarrativeEncounterOrigin::ChanceTravel => NarrativeEncounterTrigger::Travel, NarrativeEncounterOrigin::ChanceRest | NarrativeEncounterOrigin::Errantry | NarrativeEncounterOrigin::DeveloperDemo => NarrativeEncounterTrigger::Rest },
        revision: 0, open: true, resolved_choice: None, resolved_deed: None,
        virtue_exemplified: None, result_transcript: None,
    });
    ctx.db.narrative_encounter_private_authority().insert(NarrativeEncounterPrivateAuthority {
        occurrence_id: id, origin, case_id: None, finale_case_site_id: None,
        finale_hostile_group_id: None, reward_eligible: false, reward_addendum: None,
    });
    Ok(())
}

/// Bind optional preliminary trials to the first real camp reached on their
/// accepted finale journey. Issuance never predicts camp coordinates.
pub(crate) fn bind_errantry_trials_to_current_camp(
    ctx: &ReducerContext,
    party_id: &str,
) -> Result<(), String> {
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .ok_or("Party not found")?;
    if party.camp_destination.is_none() {
        return Ok(());
    }
    let journey = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party_id.to_string())
        .ok_or("Camp has no journey authority")?;
    let JourneyEndpoint::CaseSite(destination) = &journey.destination else {
        return Ok(());
    };
    let Some(contract_id) = party.active_contract_id.as_deref() else {
        return Ok(());
    };
    let Some(contract) = ctx
        .db
        .contract_authority()
        .id()
        .find(contract_id.to_string())
    else {
        return Ok(());
    };
    if contract.status != ContractStatus::Accepted
        || contract.accepted_by.as_deref() != Some(party_id)
    {
        return Ok(());
    }
    let mut candidates = ctx
        .db
        .challenge_authority()
        .party_id()
        .filter(&party_id.to_string())
        .filter(|challenge| {
            challenge.open
                && challenge.solved_at_minute.is_none()
                && challenge.case_id == contract.case_id
                && challenge.finale_case_site_id == destination.id.value
                && challenge.journey_departure_minute == 0
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.id.cmp(&right.id));
    let site_id = format!(
        "journey-camp:{}:{}",
        journey.departure_minute, journey.completed_minutes
    );
    if let Some(mut challenge) = candidates.into_iter().next() {
        let mut frame: adventuresim_core::errantry::ErrantryFrame =
            serde_json::from_str(&challenge.errantry_frame_json)
                .map_err(|_| "Errantry frame authority is invalid")?;
        if !frame
            .trials
            .iter()
            .any(|trial| trial.challenge_id.as_deref() == Some(&challenge.id))
        {
            return Err("Challenge is not bound to its errantry frame".into());
        }
        for trial in &mut frame.trials {
            if trial.site_id == "journey-camp:unbound" {
                trial.site_id = site_id.clone();
            }
        }
        challenge.errantry_frame_json =
            serde_json::to_string(&frame).map_err(|_| "Could not encode errantry frame")?;
        challenge.journey_departure_minute = journey.departure_minute;
        challenge.camp_movement_minute = journey.completed_minutes;
        challenge.camp_elapsed_minute = journey.completed_elapsed_minutes;
        ctx.db.challenge_authority().id().update(challenge);
    }
    let mut road_candidates = ctx
        .db
        .road_challenge_authority()
        .party_id()
        .filter(&party_id.to_string())
        .filter(|challenge| {
            challenge.open
                && challenge.case_id == contract.case_id
                && challenge.finale_case_site_id == destination.id.value
                && challenge.journey_departure_minute == 0
        })
        .collect::<Vec<_>>();
    road_candidates.sort_by(|left, right| left.id.cmp(&right.id));
    for mut challenge in road_candidates {
        challenge.journey_departure_minute = journey.departure_minute;
        challenge.camp_movement_minute = journey.completed_minutes;
        challenge.available_at_elapsed_minute = journey
            .completed_elapsed_minutes
            .saturating_add(COURIER_REST_DELAY_MINUTES);
        challenge.absolute_minute = journey.departure_minute
            .saturating_add(challenge.available_at_elapsed_minute);
        ctx.db.road_challenge_authority().id().update(challenge);
    }
    Ok(())
}

fn validate_challenge_retry(
    existing: &ChallengeAttemptReceipt,
    case_id: &str,
    challenge_id: &str,
    party_id: &str,
    character_id: u64,
    normalized_submission: &str,
) -> Result<(), String> {
    if existing.case_id == case_id
        && existing.challenge_id == challenge_id
        && existing.party_id == party_id
        && existing.character_id == character_id
        && existing.submission_json == normalized_submission
    {
        Ok(())
    } else {
        Err("Conflicting retry for challenge revision".into())
    }
}

/// Submit one complete puzzle answer. Every authority coordinate is derived again:
/// selected character, party leadership, active accepted contract, case,
/// challenge, exact journey camp coordinates, pending encounter state, open
/// state, and expected revision.
#[reducer]
pub fn submit_puzzle_challenge(
    ctx: &ReducerContext,
    character_id: u64,
    case_id: String,
    challenge_id: String,
    expected_revision: u32,
    submission_json: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    let character = crate::character::require_living_character(ctx, character_id)?;
    let party_id = character.party_id.ok_or("Must be in a party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can answer this challenge".into());
    }
    let mut challenge = ctx
        .db
        .challenge_authority()
        .id()
        .find(&challenge_id)
        .ok_or("Challenge not found")?;
    if challenge.case_id != case_id || challenge.party_id != party_id {
        return Err("Challenge authority does not match this party and case".into());
    }
    let submission: adventuresim_core::errantry::PuzzleSubmission =
        serde_json::from_str(&submission_json).map_err(|_| "Malformed puzzle answer")?;
    let normalized_submission =
        serde_json::to_string(&submission).map_err(|_| "Could not encode puzzle answer")?;
    let receipt_id = format!(
        "challenge-attempt:{}:{}:{}",
        challenge.id, party_id, expected_revision
    );
    // Lost-response retries remain idempotent after a successful attempt has
    // closed the challenge, resolved the case, paid the demo contract, and
    // cleared the party's active contract. New attempts continue below and
    // must satisfy every live authority check.
    if let Some(existing) = ctx.db.challenge_attempt_receipt().id().find(&receipt_id) {
        return validate_challenge_retry(
            &existing,
            &case_id,
            &challenge_id,
            &party_id,
            character_id,
            &normalized_submission,
        );
    }
    let active_contract_id = party
        .active_contract_id
        .as_ref()
        .ok_or("Party has no active quest")?;
    let contract = ctx
        .db
        .contract_authority()
        .id()
        .find(active_contract_id)
        .ok_or("Active quest not found")?;
    if contract.case_id != case_id
        || contract.accepted_by.as_deref() != Some(&party_id)
        || contract.status != ContractStatus::Accepted
    {
        return Err("Challenge does not belong to the active accepted quest".into());
    }
    let errantry = ctx
        .db
        .errantry_authority()
        .case_id()
        .find(&case_id)
        .ok_or("Case has no errantry issuance authority")?;
    if errantry.contract_id != contract.id
        || !errantry.preliminary_challenge_ids.contains(&challenge.id)
        || errantry.finale_case_site_id != challenge.finale_case_site_id
        || errantry.finale_hostile_group_id != challenge.finale_hostile_group_id
        || errantry.issuer_organization_id != ERRANTRY_ISSUER_ORGANIZATION_ID
    {
        return Err("Errantry issuance does not match the accepted quest".into());
    }
    let issuer = ctx
        .db
        .settlement_resident_profile()
        .character_id()
        .find(errantry.issuer_resident_character_id)
        .ok_or("Errantry issuer is no longer a live representative")?;
    let issuer_organization = adventuresim_core::organization::organization(
        &errantry.issuer_organization_id,
    )
    .filter(|organization| organization.errantry_issuance)
    .ok_or("Issuer organization lacks errantry authority")?;
    if issuer_organization.id != ERRANTRY_ISSUER_ORGANIZATION_ID
        || exact_organization_representative(
            ctx,
            &issuer,
            &errantry.issuer_settlement_id,
            &errantry.issuer_location_id,
        )
        .as_deref()
            != Some(ERRANTRY_ISSUER_ORGANIZATION_ID)
    {
        return Err("Errantry issuer is not the exact live Order chapter representative".into());
    }
    let case = ctx
        .db
        .case_authority()
        .id()
        .find(&case_id)
        .ok_or("Case not found")?;
    if case.resolution_status != CaseResolutionStatus::Open {
        return Err("Case is no longer open".into());
    }
    if !party_at_bound_trial_camp(ctx, &party, &challenge) {
        return Err(
            "The fey trial is available only at its bound camp, after pending encounters".into(),
        );
    }
    if !challenge.open || challenge.solved_at_minute.is_some() {
        return Err("Challenge is closed".into());
    }
    if challenge.revision != expected_revision {
        return Err("Challenge revision is stale".into());
    }
    let puzzle: adventuresim_core::errantry::PuzzleAuthority =
        serde_json::from_str(&challenge.puzzle_json)
            .map_err(|_| "Challenge authority is invalid")?;
    let frame: adventuresim_core::errantry::ErrantryFrame =
        serde_json::from_str(&challenge.errantry_frame_json)
            .map_err(|_| "Errantry frame authority is invalid")?;
    let bound_trial = frame
        .trials
        .iter()
        .find(|trial| trial.challenge_id.as_deref() == Some(&challenge.id))
        .ok_or("Challenge is not bound to its errantry frame")?;
    if bound_trial.site_id
        != format!(
            "journey-camp:{}:{}",
            challenge.journey_departure_minute, challenge.camp_movement_minute
        )
        || bound_trial.kind != adventuresim_core::errantry::TrialKind::Puzzle
    {
        return Err("Errantry trial binding does not match challenge authority".into());
    }
    puzzle
        .validate()
        .map_err(|_| "Challenge authority failed deterministic replay")?;
    let replay = puzzle.replay().map_err(str::to_string)?;
    if replay != puzzle {
        return Err("Challenge deterministic replay does not match authority".into());
    }
    let correct = puzzle
        .check(&submission)
        .map_err(|_| "Puzzle answer does not match this challenge")?;
    let now = crate::time::refresh_clock(ctx)?;
    let resulting_revision = expected_revision.saturating_add(1);
    ctx.db
        .challenge_attempt_receipt()
        .insert(ChallengeAttemptReceipt {
            id: receipt_id,
            challenge_id: challenge.id.clone(),
            case_id: case_id.clone(),
            party_id: party_id.clone(),
            character_id,
            submitted_revision: expected_revision,
            submission_json: normalized_submission,
            correct,
            resulting_revision,
            attempted_at_minute: now,
        });
    challenge.revision = resulting_revision;
    if correct {
        challenge.open = false;
        challenge.solved_at_minute = Some(now);
    }
    ctx.db.challenge_authority().id().update(challenge.clone());
    if correct {
        let solved_challenge_id = challenge.id.clone();
        ingest_case_outcome_fact(
            ctx,
            &format!("challenge-solved:{solved_challenge_id}"),
            &case_id,
            &party_id,
            adventuresim_core::case::OutcomeFactKind::ChallengeSolved {
                challenge_id: solved_challenge_id,
            },
        )?;
    }
    Ok(())
}

/// Resolve the mortal roadside courtesy trial. It becomes available only
/// after resting at its exact bound camp. Simply continuing travel ignores it
/// without blocking the finale objective.
#[reducer]
pub fn resolve_errantry_road_challenge(
    ctx: &ReducerContext,
    character_id: u64,
    challenge_id: String,
    expected_revision: u32,
    choice: String,
    action_id: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    if action_id.is_empty() || action_id.len() > 160 {
        return Err("Road challenge action ID is invalid".into());
    }
    let receipt_id = format!("road-challenge:{challenge_id}:{action_id}");
    if let Some(existing) = ctx
        .db
        .road_challenge_resolution_receipt()
        .id()
        .find(&receipt_id)
    {
        return if existing.challenge_id == challenge_id
            && existing.character_id == character_id
            && existing.choice == choice
        {
            Ok(())
        } else {
            Err("Conflicting road challenge retry".into())
        };
    }
    let character = crate::character::require_living_character(ctx, character_id)?;
    let party_id = character.party_id.ok_or("Must be in a party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can answer this road challenge".into());
    }
    let mut challenge = ctx
        .db
        .road_challenge_authority()
        .id()
        .find(&challenge_id)
        .ok_or("Road challenge not found")?;
    if challenge.party_id != party_id {
        return Err("Road challenge does not belong to this party".into());
    }
    let overlay = ctx.db.narrative_encounter_private_authority()
        .occurrence_id().find(&challenge.id)
        .ok_or("Narrative encounter private authority is missing")?;
    if overlay.origin == NarrativeEncounterOrigin::Errantry {
        let contract_id = party.active_contract_id.as_deref().ok_or("No active errantry contract")?;
        let contract = ctx.db.contract_authority().id().find(contract_id.to_string())
            .ok_or("Active errantry contract not found")?;
        if contract.status != ContractStatus::Accepted
            || contract.accepted_by.as_deref() != Some(&party_id)
            || overlay.case_id.as_deref() != Some(contract.case_id.as_str())
        { return Err("Road challenge is not bound to the accepted errantry".into()); }
        let errantry = ctx.db.errantry_authority().case_id().find(&contract.case_id)
            .ok_or("Errantry authority not found")?;
        if !errantry.preliminary_challenge_ids.contains(&challenge.id)
            || overlay.finale_case_site_id.as_deref() != Some(errantry.finale_case_site_id.as_str())
            || overlay.finale_hostile_group_id.as_deref() != Some(errantry.finale_hostile_group_id.as_str())
        { return Err("Road challenge authority does not match errantry".into()); }
    }
    if !party_at_bound_road_challenge(ctx, &party, &challenge) {
        return Err("Road challenge is not active at this camp".into());
    }
    if !challenge.open || challenge.resolved_choice.is_some() {
        return Err("Road challenge is already closed".into());
    }
    if challenge.revision != expected_revision {
        return Err("Road challenge is stale".into());
    }
    if challenge.catalog_digest != adventuresim_core::road_encounter_catalog::digest() {
        return Err("Road challenge catalog authority is stale".into());
    }
    let definition = adventuresim_core::road_encounter_catalog::encounter(&challenge.catalog_id)
        .ok_or("Road challenge catalog ID is unknown")?;
    if definition.version != challenge.catalog_revision {
        return Err("Road challenge definition revision is stale".into());
    }
    let selected = definition.choices.iter().find(|candidate| candidate.id == choice)
        .ok_or("Road challenge choice is invalid")?;
    if !matches!(selected.transition, None | Some(adventuresim_core::road_encounter_catalog::EncounterTransition::Noop)) {
        return Err("This encounter transition is not supported by the strategic runtime".into());
    }
    let now = crate::time::refresh_clock(ctx)?;
    for requirement in &selected.requirements {
        match requirement {
        adventuresim_core::road_encounter_catalog::Requirement::Skill { skill, minimum_hours } => {
            let skills = ctx
                .db
                .character_skills()
                .character_id()
                .find(character_id)
                .ok_or("Character skills not found")?;
            let hours = narrative_skill_hours(&skills, *skill);
            if !hours.is_finite() || hours < *minimum_hours as f32 {
                return Err("The chosen encounter action requires more training".into());
            }
        }
        adventuresim_core::road_encounter_catalog::Requirement::Religion { religion } => {
            let religion = narrative_religion(*religion);
            let check = crate::social::target_religion_check(
                ctx,
                character_id,
                religion,
            )?;
            if !check.is_finite() || check <= 0.0 {
                return Err("The chosen encounter action requires knowledge of this faith".into());
            }
        }
        adventuresim_core::road_encounter_catalog::Requirement::Item { item_id, minimum_quantity } => {
            let held: u32 = ctx.db.party_inventory_item().party_id().filter(&party_id)
                .filter(|stack| stack.item_id == *item_id).map(|stack| stack.quantity).sum();
            if held < u32::from(*minimum_quantity) { return Err("The chosen encounter action requires an item the party lacks".into()); }
        }
        }
    }
    for check in &selected.checks {
        let (value, difficulty_milli) = match check {
            adventuresim_core::road_encounter_catalog::Check::Skill { skill, difficulty_milli } =>
                (crate::condition::mental_check(ctx, character_id, narrative_skill(*skill))?, *difficulty_milli),
            adventuresim_core::road_encounter_catalog::Check::Religion { religion, difficulty_milli } =>
                (crate::social::target_religion_check(ctx, character_id, narrative_religion(*religion))?, *difficulty_milli),
            adventuresim_core::road_encounter_catalog::Check::Attribute { attribute, difficulty_milli } =>
                (narrative_attribute_check(ctx, character_id, *attribute)?, *difficulty_milli),
        };
        if !value.is_finite() || value * 1_000.0 < f32::from(difficulty_milli) {
            return Err("The chosen encounter check failed".into());
        }
    }
    let effects_json = serde_json::to_string(&selected.effects).map_err(|_| "Could not encode encounter effects")?;
    for effect in &selected.effects { apply_narrative_effect(ctx, &challenge.id, &party_id, now, effect)?; }
    let mut recognized_virtue = None;
    for development in &selected.personality {
        let virtue = narrative_virtue(development.virtue);
        recognized_virtue = Some(virtue);
        crate::personality::apply_personality_development(
            ctx,
            &format!("road-challenge:{}:{}", challenge.id, development.axis as u8),
            character_id,
            narrative_axis(development.axis),
            development.delta,
            &selected.deed,
            virtue,
            now,
        )?;
    }
    // Origin is private. Only a bound errantry may append reward context, and
    // it does so after generic effects and personality have succeeded.
    if let Some(mut overlay) = ctx.db.narrative_encounter_private_authority()
        .occurrence_id().find(&challenge.id)
        && overlay.origin == NarrativeEncounterOrigin::Errantry && overlay.reward_eligible
        && !selected.quest_reward_tags.is_empty()
    {
        let has_material_token = selected.effects.iter().any(|effect| matches!(effect,
            adventuresim_core::road_encounter_catalog::Effect::GrantItem { .. }));
        overlay.reward_addendum = Some(if has_material_token {
            "Thy deed hath also yielded practical knowledge for the danger ahead; consult the material token before battle."
        } else {
            "Thy deed hath also yielded practical knowledge for the danger ahead; remember this observation when preparing for battle."
        }.into());
        ctx.db.narrative_encounter_private_authority().occurrence_id().update(overlay);
    }
    challenge.open = false;
    challenge.resolved_choice = Some(choice.clone());
    challenge.resolved_deed = Some(selected.deed.clone());
    challenge.virtue_exemplified = recognized_virtue;
    challenge.result_transcript = Some(selected.result.clone());
    challenge.revision = challenge.revision.saturating_add(1);
    ctx.db.road_challenge_authority().id().update(challenge.clone());
    ctx.db
        .road_challenge_resolution_receipt()
        .insert(RoadChallengeResolutionReceipt {
            id: receipt_id,
            challenge_id,
            party_id,
            character_id,
            action_id,
            choice,
            deed: selected.deed.clone(),
            virtue_exemplified: recognized_virtue,
            catalog_revision: challenge.catalog_revision,
            catalog_digest: challenge.catalog_digest.clone(),
            result_transcript: selected.result.clone(),
            effects_json,
            resolved_at_minute: now,
        });
    Ok(())
}

fn puzzle_demo_enabled() -> bool {
    COMPILED_DEV_BOOTSTRAP_TOKEN.is_some_and(|token| {
        adventuresim_core::simulation_security::simulation_bootstrap_authorized(
            COMPILED_DEV_BOOTSTRAP_TOKEN,
            token,
        )
    })
}

fn active_puzzle_demo(
    ctx: &ReducerContext,
    party_id: &str,
    character_id: u64,
    puzzle_kind: ErrantryPuzzleKind,
) -> Option<(ChallengeAuthority, Contract)> {
    let party_key = party_id.to_string();
    let demo_prefix = format!("challenge:{}:demo:{character_id}:", puzzle_kind.core().slug());
    let mut challenges = ctx
        .db
        .challenge_authority()
        .party_id()
        .filter(&party_key)
        .filter(|challenge| {
            challenge.open
                && challenge.solved_at_minute.is_none()
                && challenge.id.starts_with(&demo_prefix)
        })
        .collect::<Vec<_>>();
    challenges.sort_by(|left, right| left.id.cmp(&right.id));
    challenges.into_iter().find_map(|challenge| {
        let contract = ctx
            .db
            .contract_authority()
            .case_id()
            .filter(&challenge.case_id)
            .find(|contract| {
                contract.service_id == "errantry:order_saint_george"
                    && contract.status == ContractStatus::Accepted
                    && contract.accepted_by.as_deref() == Some(party_id)
            })?;
        Some((challenge, contract))
    })
}

fn puzzle_demo_suffix(character_id: u64, ordinal: u64) -> String {
    format!("demo:{character_id}:{ordinal}")
}

fn errantry_suffix(character_id: u64, ordinal: u64, launch: ErrantryLaunch) -> String {
    match launch {
        ErrantryLaunch::DirectDemoCamp(_) => puzzle_demo_suffix(character_id, ordinal),
        ErrantryLaunch::NormalTravel => format!("order:{character_id}:{ordinal}"),
    }
}

fn order_errantry_issuer(
    ctx: &ReducerContext,
) -> Option<(
    crate::settlement_population::SettlementResidentProfile,
    String,
    String,
)> {
    let order = adventuresim_core::organization::organization(ERRANTRY_ISSUER_ORGANIZATION_ID)
        .filter(|organization| organization.errantry_issuance)?;
    order.chapters.iter().find_map(|chapter| {
        let settlement = ctx.db.settlement().id().find(&chapter.settlement_id)?;
        let location_id = adventuresim_core::organization::chapter_effective_location_id(
            order,
            chapter,
            &settlement.economy,
        )
        .to_string();
        let expected_id = adventuresim_core::organization::organization_representative_id(
            &chapter.settlement_id,
            ERRANTRY_ISSUER_ORGANIZATION_ID,
        );
        let npc = ctx
            .db
            .settlement_resident_profile()
            .character_id()
            .find(expected_id)?;
        (exact_organization_representative(
            ctx,
            &npc,
            &chapter.settlement_id,
            &location_id,
        )
        .as_deref()
            == Some(ERRANTRY_ISSUER_ORGANIZATION_ID))
        .then(|| (npc, chapter.settlement_id.clone(), location_id))
    })
}

/// Creates or reuses an accepted, immediately playable errantry quest.
#[reducer]
pub fn load_puzzle_demo(
    ctx: &ReducerContext,
    character_id: u64,
    puzzle_kind: ErrantryPuzzleKind,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    if !puzzle_demo_enabled() {
        return Err("Puzzle demo loading is disabled in this module build".into());
    }
    materialize_order_errantry(
        ctx,
        character_id,
        None,
        ErrantryLaunch::DirectDemoCamp(puzzle_kind),
    )
    .map(|_| ())
}

/// Developer-only catalog harness for iterating on any compiled road encounter.
/// It uses the ordinary persisted occurrence and reducer path, but binds the
/// requested definition to the party's current journey camp immediately.
#[reducer]
pub fn load_road_encounter_demo(
    ctx: &ReducerContext,
    character_id: u64,
    catalog_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    if !puzzle_demo_enabled() {
        return Err("Road encounter demo loading is disabled in this module build".into());
    }
    if catalog_id.is_empty() || catalog_id.len() > 96 {
        return Err("Road encounter catalog ID is invalid".into());
    }
    let definition = adventuresim_core::road_encounter_catalog::encounter(&catalog_id)
        .ok_or("Unknown road encounter catalog ID")?;
    let character = crate::character::require_living_character(ctx, character_id)?;
    let party_id = character.party_id.ok_or("Must be in a party")?;
    let party = ctx.db.party_authority().id().find(&party_id).ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can load a road encounter demo".into());
    }
    if party.current_settlement_id.is_some()
        || party.current_case_site_id.is_some()
        || party.camp_destination.is_none()
    {
        return Err("Load a road encounter demo from a journey camp".into());
    }
    let journey = ctx.db.party_journey_authority().party_id().find(&party_id)
        .ok_or("Road encounter demo requires a durable journey camp")?;
    if !journey.camp_stop_minutes.contains(&journey.completed_minutes) {
        return Err("Road encounter demo requires a reached journey camp".into());
    }
    let ordinal = ctx.db.road_challenge_authority().party_id().filter(&party_id)
        .filter(|challenge| challenge.catalog_id == definition.id).count() as u64;
    let catalog_hash = catalog_id.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x1000_0000_01b3)
    });
    materialize_chance_narrative_encounter(
        ctx,
        &party_id,
        &adventuresim_core::encounter::NarrativeSelection {
            boundary_minute: journey.completed_elapsed_minutes,
            roll_index: 0xd000_0000_0000_0000 ^ catalog_hash ^ ordinal.rotate_left(17),
            catalog_id,
        },
        NarrativeEncounterOrigin::DeveloperDemo,
    )
}

/// Narrow production issuance seam: the client identifies only its live
/// dialogue session. The reducer derives and verifies the NPC, chapter,
/// organization capability, settlement, and location before materializing.
#[reducer]
pub fn accept_order_errantry(
    ctx: &ReducerContext,
    character_id: u64,
    dialogue_session_id: String,
    action_id: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    if action_id.is_empty() || action_id.len() > 160 {
        return Err("Errantry acceptance action ID is invalid".into());
    }
    let receipt_id = format!("order-errantry-accept:{dialogue_session_id}:{action_id}");
    // Exact lost-response retries return before physical dialogue presence is
    // revalidated. The immutable receipt is accepted only while its sourced
    // case and contract still agree with current errantry authority.
    if let Some(receipt) = ctx
        .db
        .order_errantry_acceptance_receipt()
        .id()
        .find(&receipt_id)
    {
        let current = ctx
            .db
            .errantry_authority()
            .case_id()
            .find(&receipt.case_id);
        return if receipt.dialogue_session_id == dialogue_session_id
            && receipt.action_id == action_id
            && receipt.character_id == character_id
            && current.is_some_and(|errantry| errantry.contract_id == receipt.contract_id)
        {
            Ok(())
        } else {
            Err("Conflicting Order errantry acceptance retry".into())
        };
    }
    let session = ctx
        .db
        .dialogue_session()
        .id()
        .find(&dialogue_session_id)
        .ok_or("Dialogue session not found")?;
    if session.owner_character_id != character_id
        || session.conversation_id != "organization-representative"
    {
        return Err("Errantry must be accepted in the owning organization dialogue".into());
    }
    let npc = require_live_dialogue_presence(ctx, &session, character_id)?;
    let organization_id =
        exact_organization_representative(ctx, &npc, &session.settlement_id, &session.location_id)
            .ok_or("Dialogue NPC is not an exact chapter representative")?;
    let organization = adventuresim_core::organization::organization(&organization_id)
        .filter(|organization| organization.errantry_issuance)
        .ok_or("This organization cannot issue errantry")?;
    if organization.id != ERRANTRY_ISSUER_ORGANIZATION_ID {
        return Err("Only the Order of St. George can issue this errantry".into());
    }
    let materialized = materialize_order_errantry(
        ctx,
        character_id,
        Some((npc, session.settlement_id, session.location_id)),
        ErrantryLaunch::NormalTravel,
    )?;
    ctx.db
        .order_errantry_acceptance_receipt()
        .insert(OrderErrantryAcceptanceReceipt {
            id: receipt_id,
            dialogue_session_id,
            action_id,
            character_id,
            case_id: materialized.case_id,
            contract_id: materialized.contract_id,
        });
    Ok(())
}

fn materialize_order_errantry(
    ctx: &ReducerContext,
    character_id: u64,
    issuer_override: Option<(
        crate::settlement_population::SettlementResidentProfile,
        String,
        String,
    )>,
    launch: ErrantryLaunch,
) -> Result<MaterializedErrantry, String> {
    let character = crate::character::require_living_character(ctx, character_id)?;
    let party_id = character.party_id.clone().ok_or("Must be in a party")?;
    let mut party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    if party.leader_id != character_id {
        return Err("Only the party leader can accept or load this errantry".into());
    }
    if let ErrantryLaunch::DirectDemoCamp(puzzle_kind) = launch
        && let Some((challenge, contract)) =
            active_puzzle_demo(ctx, &party_id, character_id, puzzle_kind)
    {
        if let Some(active) = party.active_contract_id.as_deref()
            && active != contract.id
        {
            return Err("Finish or abandon the active quest before loading the puzzle demo".into());
        }
        party.active_contract_id = Some(contract.id.clone());
        ctx.db.party_authority().id().update(party);
        return Ok(MaterializedErrantry {
            case_id: challenge.case_id,
            contract_id: contract.id,
        });
    }
    let origin_settlement_id = character
        .current_settlement_id
        .clone()
        .ok_or("Load a fresh puzzle demo while in a settlement")?;
    let origin_settlement = ctx
        .db
        .settlement()
        .id()
        .find(&origin_settlement_id)
        .ok_or("Current settlement not found")?;
    let (issuer, issuer_settlement_id, issuer_location_id) = issuer_override
        .or_else(|| order_errantry_issuer(ctx))
        .ok_or("No live Order of St. George chapter representative")?;
    if let Some(active) = party.active_contract_id.clone() {
        let active_contract = ctx
            .db
            .contract_authority()
            .id()
            .find(&active)
            .ok_or("Active quest not found")?;
        return Err(format!(
            "Finish or abandon the active quest '{}' before accepting errantry",
            active_contract.title
        ));
    }
    let namespace = match launch {
        ErrantryLaunch::DirectDemoCamp(_) => "demo",
        ErrantryLaunch::NormalTravel => "order",
    };
    let puzzle_kind = match launch {
        ErrantryLaunch::NormalTravel => adventuresim_core::errantry::PuzzleKind::OrderedSigils,
        ErrantryLaunch::DirectDemoCamp(kind) => kind.core(),
    };
    let challenge_prefix = format!(
        "challenge:{}:{namespace}:{character_id}:",
        puzzle_kind.slug()
    );
    let ordinal = ctx
        .db
        .challenge_authority()
        .party_id()
        .filter(&party_id)
        .filter(|challenge| match launch {
            ErrantryLaunch::NormalTravel => challenge.id.starts_with(&challenge_prefix),
            ErrantryLaunch::DirectDemoCamp(_) => {
                challenge.id.starts_with("challenge:")
                    && challenge.id.contains(&format!(":demo:{character_id}:"))
            }
        })
        .count() as u64;
    let suffix = errantry_suffix(character_id, ordinal, launch);
    let seed = 0x4b4e_4947_4854_4c59 ^ character_id ^ ordinal.rotate_left(23);
    let road_definition = adventuresim_core::road_encounter_catalog::select_quest_eligible(seed, ordinal)
        .ok_or("No quest-eligible road encounter is available")?;
    let case_id = format!("case:errantry-puzzle:{suffix}");
    let contract_id = format!("contract:errantry-puzzle:{suffix}");
    let challenge_id = format!("challenge:{}:{suffix}", puzzle_kind.slug());
    let courier_challenge_id = format!("challenge:road-encounter:{suffix}");
    let case_site_id = format!("case-site:errantry-finale:{suffix}");
    let hostile_group_id = format!("hostile-group:errantry-finale:{suffix}");
    // The preliminary trial is a true optional boon: defeating the finale
    // resolves the case whether or not ChallengeSolved was emitted.
    let objective = adventuresim_core::case::ObjectiveExpression::new(vec![
        adventuresim_core::case::ObjectivePath {
            objectives: vec![adventuresim_core::case::Objective {
                id: adventuresim_core::case::ObjectiveId::new(format!(
                    "objective:defeat-finale:{suffix}"
                ))
                .map_err(|_| "Finale objective ID is invalid")?,
                requirement: adventuresim_core::case::ObjectiveRequirement::Defeat {
                    hostile_group_id: hostile_group_id.clone(),
                    count: 4,
                },
            }],
        },
    ])
    .map_err(|_| "Puzzle objective is invalid")?;
    let objective_expression_json =
        serde_json::to_string(&objective).map_err(|_| "Could not encode puzzle objective")?;
    ctx.db.case_authority().insert(CaseAuthority {
        id: case_id.clone(),
        investigation_case_id: format!("errantry:{suffix}"),
        provenance_kind: "manual".into(),
        generated_case_id: String::new(),
        local_problem_id: None,
        objective_expression_json,
        resolution_status: CaseResolutionStatus::Open,
        resolved_by_party_id: None,
    });
    let (title, description, frame_name, charge) = match puzzle_kind {
        adventuresim_core::errantry::PuzzleKind::OrderedSigils => (
            "The Trial of Five Signs",
            "A knightly errand leads directly to a trial of ordered signs.",
            "five-signs",
            "Keep faith upon the road and answer the trial with discernment.",
        ),
        adventuresim_core::errantry::PuzzleKind::TruthfulWitnesses => (
            "The Trial of Three Witnesses",
            "A knightly errand leads directly to a trial of testimony and judgment.",
            "three-witnesses",
            "Hear each witness upon the road and judge the one safe path.",
        ),
        adventuresim_core::errantry::PuzzleKind::RuneTransformation => (
            "The Trial of the Changing Runes",
            "A knightly errand leads directly to a trial of a hidden rune law.",
            "changing-runes",
            "Study what enters and emerges, then name the rune the law must yield.",
        ),
        adventuresim_core::errantry::PuzzleKind::LogicGrid => (
            "The Trial of the Pilgrims' Bonds",
            "A knightly errand leads directly to a trial matching pilgrims, tokens, and roads.",
            "pilgrims-bonds",
            "Restore each pilgrim's token and road from the Lady's formal clues.",
        ),
        adventuresim_core::errantry::PuzzleKind::ResourceAllocation => (
            "The Trial of the Measured Pack",
            "A knightly errand leads directly to a trial of burdens and preparation.",
            "measured-pack",
            "Choose the most ready pack that meets every named hazard within its burden.",
        ),
    };
    ctx.db.contract_authority().insert(Contract {
        id: contract_id.clone(),
        gateway_bucket: 0,
        case_id: case_id.clone(),
        title: title.into(),
        description: description.into(),
        difficulty: 1,
        gold_reward: 0,
        xp_reward: 0,
        settlement_id: issuer_settlement_id.clone(),
        service_id: "errantry:order_saint_george".into(),
        issuer_resident_character_id: issuer.character_id,
        status: ContractStatus::Accepted,
        accepted_by: Some(party_id.clone()),
        opposition_wording: "an enchanted gate".into(),
        opposition_count_wording: "a household guard".into(),
        accepted_at_minute: Some(crate::time::refresh_clock(ctx)?),
        paid_at_minute: None,
    });
    let puzzle = adventuresim_core::errantry::PuzzleAuthority::generate(puzzle_kind, seed);
    let now = crate::time::refresh_clock(ctx)?;
    let site = CaseSiteAuthority {
        id_key: case_site_id.clone(),
        id: CaseSiteId::from(case_site_id.clone()),
        case_id: case_id.clone(),
        origin_settlement_id: origin_settlement_id.clone(),
        name: "The Black Knight's Ford".into(),
        description: "A ford held by a knight who preys upon travelers.".into(),
        scene_key: "forest-clearing".into(),
        longitude_e7: ((origin_settlement.coord_x + 0.02) * 10_000_000.0) as i32,
        latitude_e7: ((origin_settlement.coord_y + 0.02) * 10_000_000.0) as i32,
        coordinates_are_geographic: true,
        distance_m: 2_000,
    };
    ctx.db.case_site_authority().insert(site.clone());
    materialize_hostile_group(
        ctx,
        &hostile_group_id,
        &site,
        ERRANTRY_FINALE_THREAT_ID.into(),
        4,
        6,
    )?;
    let frame = adventuresim_core::errantry::ErrantryFrame {
        id: format!("errantry:{frame_name}:{suffix}"),
        purpose: adventuresim_core::errantry::ErrantryPurpose::ProveWorth,
        charge: charge.into(),
        trials: vec![
            adventuresim_core::errantry::TrialBinding {
                order: 0,
                trial_id: format!("trial:{frame_name}:{suffix}"),
                challenge_id: Some(challenge_id.clone()),
                site_id: "journey-camp:unbound".into(),
                kind: adventuresim_core::errantry::TrialKind::Puzzle,
            },
            adventuresim_core::errantry::TrialBinding {
                order: 1,
                trial_id: format!("trial:wounded-courier:{suffix}"),
                challenge_id: Some(courier_challenge_id.clone()),
                site_id: "journey-camp:unbound".into(),
                kind: adventuresim_core::errantry::TrialKind::Social,
            },
        ],
    };
    ctx.db.challenge_authority().insert(ChallengeAuthority {
        id: challenge_id.clone(),
        gateway_bucket: 0,
        case_id: case_id.clone(),
        party_id: party_id.clone(),
        finale_case_site_id: case_site_id.clone(),
        finale_hostile_group_id: hostile_group_id.clone(),
        journey_departure_minute: 0,
        camp_movement_minute: 0,
        camp_elapsed_minute: 0,
        errantry_frame_json: serde_json::to_string(&frame)
            .map_err(|_| "Could not encode errantry frame authority")?,
        puzzle_json: serde_json::to_string(&puzzle)
            .map_err(|_| "Could not encode puzzle authority")?,
        presenter_catalog_id: ChallengePresenterCatalogId::LadyBeneathThornV1,
        revision: 0,
        open: true,
        solved_at_minute: None,
    });
    ctx.db
        .road_challenge_authority()
        .insert(RoadChallengeAuthority {
            id: courier_challenge_id.clone(),
            gateway_bucket: 0,
            party_id: party_id.clone(),
            case_id: case_id.clone(),
            finale_case_site_id: case_site_id.clone(),
            finale_hostile_group_id: hostile_group_id.clone(),
            journey_departure_minute: 0,
            camp_movement_minute: 0,
            available_at_elapsed_minute: 0,
            catalog_id: road_definition.id.clone(),
            catalog_revision: road_definition.version,
            catalog_digest: adventuresim_core::road_encounter_catalog::digest().into(),
            absolute_minute: 0,
            longitude_e7: 0,
            latitude_e7: 0,
            trigger: NarrativeEncounterTrigger::Rest,
            revision: 0,
            open: true,
            resolved_choice: None,
            resolved_deed: None,
            virtue_exemplified: None,
            result_transcript: None,
        });
    ctx.db.narrative_encounter_private_authority().insert(
        NarrativeEncounterPrivateAuthority {
            occurrence_id: courier_challenge_id.clone(),
            origin: NarrativeEncounterOrigin::Errantry,
            case_id: Some(case_id.clone()),
            finale_case_site_id: Some(case_site_id.clone()),
            finale_hostile_group_id: Some(hostile_group_id.clone()),
            reward_eligible: true,
            reward_addendum: None,
        },
    );
    let finale_defenses = vec![
        adventuresim_core::errantry::FinaleDefenseKind::UnnaturalProwess,
        adventuresim_core::errantry::FinaleDefenseKind::Reinforcements,
        adventuresim_core::errantry::FinaleDefenseKind::SupernaturalArmor,
    ];
    ctx.db.errantry_authority().insert(ErrantryAuthority {
        case_id: case_id.clone(),
        contract_id: contract_id.clone(),
        issuer_organization_id: ERRANTRY_ISSUER_ORGANIZATION_ID.into(),
        issuer_resident_character_id: issuer.character_id,
        issuer_settlement_id,
        issuer_location_id,
        finale_case_site_id: case_site_id.clone(),
        finale_hostile_group_id: hostile_group_id,
        preliminary_challenge_ids: vec![challenge_id.clone(), courier_challenge_id],
        finale_defenses_json: serde_json::to_string(&finale_defenses)
            .map_err(|_| "Could not encode finale defenses")?,
    });
    crate::investigation::disclose_exact_case_site(
        ctx,
        character_id,
        &case_id,
        &site,
        "the Order of St. George",
    )?;
    let destination = JourneyEndpoint::CaseSite(JourneyCaseSiteEndpoint {
        id: CaseSiteId::from(case_site_id.clone()),
        name: site.name.clone(),
    });
    party.active_contract_id = Some(contract_id.clone());
    if matches!(launch, ErrantryLaunch::DirectDemoCamp(_)) {
        let camp_movement_minute = 60;
        let camp_elapsed_minute = 60;
        ctx.db.party_journey_authority().insert(PartyJourney {
            party_id: party_id.clone(),
            gateway_bucket: 0,
            origin: JourneyEndpoint::Settlement(JourneySettlementEndpoint {
                id: origin_settlement_id,
                name: origin_settlement.name,
            }),
            destination: destination.clone(),
            total_minutes: 120,
            completed_minutes: camp_movement_minute,
            camp_stop_minutes: vec![camp_movement_minute],
            forecast_camp_stop_minutes: Vec::new(),
            fatigue_percent: party.camp_fatigue_percent,
            plan_version: 2,
            departure_minute: now,
            total_elapsed_minutes: 120,
            completed_elapsed_minutes: camp_elapsed_minute,
            walking_minutes_per_day: party.walking_minutes_per_day,
            travel_at_night: party.travel_at_night,
            camp_duration_mode: party.camp_duration_mode,
            fixed_camp_minutes: party.fixed_camp_minutes,
        });
        ctx.db
            .party_journey_encounter_authority()
            .insert(PartyJourneyEncounterAuthority {
                party_id: party_id.clone(),
                seed: ctx.random(),
                next_roll: 1,
                narrative_rest_elapsed_minutes: 0,
            });
        party.current_settlement_id = None;
        party.current_case_site_id = None;
        party.camp_destination = Some(destination);
        party.camp_remaining_minutes = 60;
        ctx.db.party_authority().id().update(party);
        for member_id in living_party_member_ids(ctx, &party_id) {
            let mut member = ctx
                .db
                .character()
                .id()
                .find(member_id)
                .ok_or("Party member not found")?;
            member.current_settlement_id = None;
            crate::investigation::set_character_case_site(ctx, member_id, None);
            ctx.db.character().id().update(member);
        }
        bind_errantry_trials_to_current_camp(ctx, &party_id)?;
    } else {
        ctx.db.party_authority().id().update(party);
    }
    Ok(MaterializedErrantry {
        case_id,
        contract_id,
    })
}

#[cfg(test)]
mod challenge_source_boundary_tests {
    use super::{
        ChallengeAttemptReceipt, ERRANTRY_FINALE_THREAT_ID,
        puzzle_demo_suffix, validate_challenge_retry,
    };

    #[test]
    fn public_projection_omits_private_truth_fields() {
        let source = include_str!("challenges.rs");
        let projection = source
            .split("pub struct BackendChallenge")
            .nth(1)
            .unwrap()
            .split("#[view")
            .next()
            .unwrap();
        assert!(!projection.contains("seed:"));
        assert!(!projection.contains("solution"));
        assert!(!projection.contains("puzzle_json:"));
        assert!(source.contains("ChallengeSolved"));
        assert!(source.contains("challenge.revision != expected_revision"));
        assert!(source.contains("contract.accepted_by.as_deref() != Some(&party_id)"));
        let road_projection = source.split("pub struct BackendRoadChallenge").nth(1).unwrap()
            .split("pub struct OrderErrantryAcceptanceReceipt").next().unwrap();
        assert!(road_projection.contains("presentation_json"));
        assert!(!road_projection.contains("catalog_id"));
        assert!(!road_projection.contains("catalog_digest"));
        assert!(!road_projection.contains("origin:"));
        assert!(!road_projection.contains("case_id:"));
        assert!(!road_projection.contains("finale_hostile_group_id:"));
    }

    #[test]
    fn lost_success_response_retries_exactly_but_conflicts_fail() {
        let receipt = ChallengeAttemptReceipt {
            id: "challenge-attempt:challenge:test:party:test:0".into(),
            challenge_id: "challenge:test".into(),
            case_id: "case:test".into(),
            party_id: "party:test".into(),
            character_id: 7,
            submitted_revision: 0,
            submission_json: r#"{"kind":"ordered_sigils","answer":{"ordering":["Crown","Hart","Moon","Rose","Sword"]}}"#.into(),
            correct: true,
            resulting_revision: 1,
            attempted_at_minute: 42,
        };
        validate_challenge_retry(
            &receipt,
            "case:test",
            "challenge:test",
            "party:test",
            7,
            r#"{"kind":"ordered_sigils","answer":{"ordering":["Crown","Hart","Moon","Rose","Sword"]}}"#,
        )
        .unwrap();
        assert!(
            validate_challenge_retry(
                &receipt,
                "case:test",
                "challenge:test",
                "party:test",
                7,
                r#"{"kind":"ordered_sigils","answer":{"ordering":["Sword","Hart","Moon","Rose","Crown"]}}"#,
            )
            .unwrap_err()
            .contains("Conflicting retry")
        );

        let source = include_str!("challenges.rs");
        let receipt_check = source
            .find("if let Some(existing) = ctx.db.challenge_attempt_receipt()")
            .unwrap();
        let active_contract = source.find("let active_contract_id = party").unwrap();
        let case_open = source.find("case.resolution_status !=").unwrap();
        assert!(receipt_check < active_contract && receipt_check < case_open);
    }

    #[test]
    fn demo_is_reused_and_materializes_a_real_bound_camp() {
        assert_ne!(
            puzzle_demo_suffix(7, 0),
            puzzle_demo_suffix(7, 1)
        );
        let source = include_str!("challenges.rs");
        let loader = source.split("fn materialize_order_errantry").nth(1).unwrap();
        let reuse = loader.find("active_puzzle_demo").unwrap();
        let fresh_ordinal = loader.find("let ordinal =").unwrap();
        assert!(reuse < fresh_ordinal);
        assert!(loader.contains("return Ok(())"));
        assert!(loader.contains("ordinal.rotate_left(23)"));
        let reuse_lookup = source
            .split("fn active_puzzle_demo")
            .nth(1)
            .unwrap()
            .split("fn puzzle_demo_suffix")
            .next()
            .unwrap();
        assert!(reuse_lookup.contains("puzzle_kind.core().slug()"));
        assert!(loader.contains("PartyJourney"));
        assert!(loader.contains("camp_destination = Some(destination)"));
        assert!(loader.contains("ErrantryAuthority"));
        assert!(source.contains("accept_order_errantry"));
        assert!(source.contains("organization.errantry_issuance"));
    }

    #[test]
    fn road_encounter_demo_is_dev_authorized_catalog_driven_and_camp_bound() {
        let source = include_str!("challenges.rs");
        let loader = source.split("pub fn load_road_encounter_demo").nth(1)
            .and_then(|tail| tail.split("pub fn accept_order_errantry").next()).unwrap();
        assert!(loader.contains("require_strategic_character_authority"));
        assert!(loader.contains("puzzle_demo_enabled"));
        assert!(loader.contains("road_encounter_catalog::encounter(&catalog_id)"));
        assert!(loader.contains("party.leader_id != character_id"));
        assert!(loader.contains("camp_stop_minutes.contains"));
        assert!(loader.contains("materialize_chance_narrative_encounter"));
        assert!(loader.contains("NarrativeEncounterOrigin::DeveloperDemo"));
        assert!(!loader.contains("provenance"));
    }

    #[test]
    fn trial_is_optional_camp_authority_and_physical_rewards_are_separate() {
        let source = include_str!("challenges.rs");
        let camp_match = source
            .split("fn journey_at_bound_trial_camp")
            .nth(1)
            .unwrap()
            .split("fn party_at_bound_trial_camp_view")
            .next()
            .unwrap();
        assert!(camp_match.contains("journey_camp_identity_matches"));
        assert!(!camp_match.contains("completed_elapsed_minutes"));
        assert!(source.contains("encounter.status == \"awaiting_choice\""));
        assert!(!source.contains("insert(StrategicEncounter"));
        assert!(!source.contains("struct ErrantryCountermeasure"));
        assert!(!source.contains("ERRANTRY_COUNTERMEASURE_SCALE_FLOOR_BPS"));
        let puzzle_submission = source
            .split("pub fn submit_puzzle_challenge")
            .nth(1)
            .unwrap()
            .split("pub fn resolve_errantry_road_challenge")
            .next()
            .unwrap();
        assert!(!puzzle_submission.contains("award_errantry_countermeasure"));
        assert!(source.contains("bound_tactical_insight"));

        let mission = include_str!("custody_objectives.rs");
        assert!(!mission.contains("errantry_mission_scale_snapshot"));
        assert!(!mission.contains("base_enemy_combat_scale_bps"));
        assert!(!mission.contains("countermeasure_source_challenge_id"));
        let tactical = include_str!("../tactical.rs");
        assert!(tactical.contains("mission.enemy_combat_scale_bps"));
        let autoresolve = include_str!("autoresolve.rs");
        assert!(autoresolve.contains("mission.enemy_combat_scale_bps"));
    }

    #[test]
    fn rested_courier_trial_is_optional_authorized_and_material() {
        let source = include_str!("challenges.rs");
        let reducer = source
            .split("pub fn resolve_errantry_road_challenge")
            .nth(1)
            .and_then(|tail| tail.split("fn puzzle_demo_enabled").next())
            .unwrap();
        assert!(reducer.contains("require_strategic_gateway"));
        assert!(reducer.contains("party.leader_id != character_id"));
        assert!(reducer.contains("party_at_bound_road_challenge"));
        assert!(source.contains("encounter.status == \"awaiting_choice\""));
        assert!(source.contains("narrative_skill"));
        assert!(reducer.contains("target_religion_check"));
        assert!(reducer.contains("OfficialReligion::RomanCatholic"));
        assert!(reducer.contains("apply_personality_development"));
        assert!(!reducer.contains("ingest_case_outcome_fact"));
        assert!(source.contains("COURIER_REST_DELAY_MINUTES"));
        assert!(source.contains("preliminary_challenge_ids"));
        assert!(source.contains("finale_defenses_json"));
        assert!(reducer.contains("overlay.origin == NarrativeEncounterOrigin::Errantry"));
        assert!(reducer.contains("apply_narrative_effect"));
        assert!(reducer.find("apply_narrative_effect").unwrap() < reducer.find("reward_addendum").unwrap());
        assert!(reducer.find("road_challenge_resolution_receipt()").unwrap()
            < reducer.find("apply_narrative_effect").unwrap());
        assert!(reducer.contains("Conflicting road challenge retry"));
    }

    #[test]
    fn courier_catalog_binds_distinct_material_and_personality_routes() {
        use adventuresim_core::road_encounter_catalog::{Effect, Requirement, VirtueId};
        let definition = adventuresim_core::road_encounter_catalog::encounter("wounded_order_courier_v1").unwrap();
        let choice = |id| definition.choices.iter().find(|choice| choice.id == id).unwrap();
        assert!(matches!(choice("aid").effects[0], Effect::GrantItem { ref item_id, .. } if item_id == adventuresim_core::item_references::CAPTURED_DISPATCH_ITEM_ID));
        assert_eq!(choice("aid").personality[0].virtue, VirtueId::Mercy);
        assert!(matches!(choice("rally").requirements[0], Requirement::Skill { .. }));
        assert_eq!(choice("rally").personality[0].virtue, VirtueId::Courage);
        assert!(matches!(choice("consecrate").requirements[0], Requirement::Religion { .. }));
        assert_eq!(choice("consecrate").personality[0].virtue, VirtueId::Faith);
        assert!(choice("rob").personality[0].delta < 0);
        assert!(choice("ignore").effects.is_empty() && choice("ignore").personality.is_empty());
    }

    #[test]
    fn missions_do_not_snapshot_hidden_errantry_modifiers() {
        let mission = include_str!("authority_model.rs");
        assert!(!mission.contains("errantry_approach_snapshot_json"));
        assert!(!mission.contains("countermeasure_source_challenge_id"));
        assert!(!mission.contains("base_enemy_combat_scale_bps"));
    }

    #[test]
    fn authored_finale_threat_yields_only_a_modeled_physical_insight() {
        let threat = ERRANTRY_FINALE_THREAT_ID
            .parse::<adventuresim_core::bestiary::ThreatId>()
            .unwrap();
        assert_eq!(threat, adventuresim_core::bestiary::ThreatId::ArmedRetainer);
        let profile_before = threat.profile().combat;
        let insight = adventuresim_core::errantry::tactical_insight_for(threat).unwrap();
        assert!(insight.finding.contains("no missile weapons"));
        assert!(insight.preparation.contains("bows and arrows"));
        assert_eq!(
            format!("{profile_before:?}"),
            format!("{:?}", threat.profile().combat)
        );

        let source = include_str!("challenges.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(!production.contains("FavorOfTheThornLady"));
        assert!(!production.contains("FEY_COUNTERMEASURE_ITEM_ID"));
        assert!(source.contains("ERRANTRY_FINALE_THREAT_ID.into()"));
        assert!(source.contains("4,\n        6,"));
        assert!(source.contains("ErrantryLaunch::NormalTravel"));
        assert!(source.contains("ErrantryLaunch::DirectDemoCamp"));
    }

    #[test]
    fn committed_acceptance_retry_precedes_live_presence_validation() {
        let source = include_str!("challenges.rs");
        let reducer = source
            .split("pub fn accept_order_errantry")
            .nth(1)
            .and_then(|tail| tail.split("fn materialize_order_errantry").next())
            .unwrap();
        assert!(
            reducer
                .find("order_errantry_acceptance_receipt()")
                .unwrap()
                < reducer.find("require_live_dialogue_presence").unwrap()
        );
        assert!(reducer.contains("errantry.contract_id == receipt.contract_id"));
        assert!(reducer.contains("Conflicting Order errantry acceptance retry"));
    }
}
