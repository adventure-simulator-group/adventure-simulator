//! Persistent strategic settlement residents and authoritative observable presences.
use crate::{
    character::{character, character__view, insert_persistent_npc_character},
    personality::{Presentation, character_personality, character_personality__view},
    relationship::{NpcPolicy, npc_policy},
    strategic::{settlement, strategic_gateway_authority__view},
};
use adventuresim_core::settlement_population::{
    self as population, AgeBand, GenerationInput, LocationContext, PresenceBridge, Profession,
    Schedule,
};
use adventuresim_core::strategic_place::{SettlementVenueKind, StrategicPlaceId};
use adventuresim_core::strategic_presence::{
    DailyPresenceWindow, PresenceFrontier, ScheduledStrategicPresence, StrategicPresence,
};
use serde::{Deserialize, Serialize};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, table, view};
use std::collections::BTreeSet;
use std::ops::Deref;

#[derive(Clone, Copy, Debug, SpacetimeType)]
pub enum NpcAgeBand {
    Child,
    Adolescent,
    Adult,
    Elder,
}
#[derive(Clone, Copy, Debug, SpacetimeType)]
pub enum NpcPresentation {
    Man,
    Ambiguous,
    Woman,
}

#[derive(Clone, Debug)]
#[table(accessor = settlement_resident_profile)]
pub struct SettlementResidentProfile {
    #[primary_key]
    pub character_id: u64,
    /// Bounded traversal key for the fail-closed gateway view.
    #[index(btree)]
    pub projection_id: u64,
    #[index(btree)]
    pub home_settlement_id: String,
    pub height: String,
    pub build: String,
    pub hair: String,
    pub facial_hair: String,
    pub complexion: String,
    pub visible_features: String,
    pub clothing: String,
    pub profession: String,
    pub household: String,
    pub local_role: String,
    pub service_id: String,
    /// Explicit institution authority. Empty for NPCs not representing an organization.
    pub organization_id: String,
    pub conversation_id: String,
}

/// Reducer-local join of a resident's presentation metadata with the ordinary
/// Character and private personality components that own identity and
/// demographics. This is never persisted as a second person record.
#[derive(Clone, Debug)]
pub struct ResolvedSettlementResident {
    pub profile: SettlementResidentProfile,
    pub name: String,
    pub age_band: NpcAgeBand,
    pub sex: crate::personality::Sex,
    pub presentation: crate::personality::Presentation,
}

impl Deref for ResolvedSettlementResident {
    type Target = SettlementResidentProfile;

    fn deref(&self) -> &Self::Target {
        &self.profile
    }
}

pub fn resolve_settlement_resident(
    ctx: &ReducerContext,
    character_id: u64,
) -> Option<ResolvedSettlementResident> {
    let profile = ctx
        .db
        .settlement_resident_profile()
        .character_id()
        .find(character_id)?;
    let character = ctx.db.character().id().find(character_id)?;
    let personality = ctx
        .db
        .character_personality()
        .character_id()
        .find(character_id)?;
    Some(ResolvedSettlementResident {
        profile,
        name: character.name,
        age_band: match character.age_years {
            0..=12 => NpcAgeBand::Child,
            13..=17 => NpcAgeBand::Adolescent,
            18..=59 => NpcAgeBand::Adult,
            _ => NpcAgeBand::Elder,
        },
        sex: personality.sex,
        presentation: personality.presentation,
    })
}

pub fn resolve_settlement_resident_view(
    ctx: &ViewContext,
    character_id: u64,
) -> Option<ResolvedSettlementResident> {
    let profile = ctx
        .db
        .settlement_resident_profile()
        .character_id()
        .find(character_id)?;
    let character = ctx.db.character().id().find(character_id)?;
    let personality = ctx
        .db
        .character_personality()
        .character_id()
        .find(character_id)?;
    Some(ResolvedSettlementResident {
        profile,
        name: character.name,
        age_band: match character.age_years {
            0..=12 => NpcAgeBand::Child,
            13..=17 => NpcAgeBand::Adolescent,
            18..=59 => NpcAgeBand::Adult,
            _ => NpcAgeBand::Elder,
        },
        sex: personality.sex,
        presentation: personality.presentation,
    })
}

/// Settlement NPC facts that the registered gateway may project to players.
///
/// Keep this row explicit: `SettlementResidentProfile` also contains private demographic
/// and traversal authority that must never become subscription data merely
/// because the authoritative table gains another field.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendSettlementResident {
    pub character_id: u64,
    pub home_settlement_id: String,
    pub name: String,
    pub age_band: NpcAgeBand,
    pub presentation: NpcPresentation,
    pub height: String,
    pub build: String,
    pub hair: String,
    pub facial_hair: String,
    pub complexion: String,
    pub visible_features: String,
    pub clothing: String,
    pub profession: String,
    pub household: String,
    pub local_role: String,
    pub service_id: String,
    pub organization_id: String,
    pub conversation_id: String,
}

fn project_backend_settlement_resident(
    ctx: &ViewContext,
    profile: SettlementResidentProfile,
) -> Option<BackendSettlementResident> {
    let character = ctx.db.character().id().find(profile.character_id)?;
    let personality = ctx
        .db
        .character_personality()
        .character_id()
        .find(profile.character_id)?;
    let age_band = match character.age_years {
        0..=12 => NpcAgeBand::Child,
        13..=17 => NpcAgeBand::Adolescent,
        18..=59 => NpcAgeBand::Adult,
        _ => NpcAgeBand::Elder,
    };
    let presentation = match personality.presentation {
        Presentation::Man => NpcPresentation::Man,
        Presentation::Ambiguous => NpcPresentation::Ambiguous,
        Presentation::Woman => NpcPresentation::Woman,
    };
    Some(BackendSettlementResident {
        character_id: profile.character_id,
        home_settlement_id: profile.home_settlement_id,
        name: character.name,
        age_band,
        presentation,
        height: profile.height,
        build: profile.build,
        hair: profile.hair,
        facial_hair: profile.facial_hair,
        complexion: profile.complexion,
        visible_features: profile.visible_features,
        clothing: profile.clothing,
        profession: profile.profession,
        household: profile.household,
        local_role: profile.local_role,
        service_id: profile.service_id,
        organization_id: profile.organization_id,
        conversation_id: profile.conversation_id,
    })
}

pub(crate) fn resident_is_dialogue_capable(profile: &SettlementResidentProfile) -> bool {
    adventuresim_dialogue::find_conversation(&profile.conversation_id).is_some_and(|conversation| {
        conversation
            .roles
            .values()
            .any(|role| role.kind == adventuresim_dialogue::ParticipantKind::Player)
            && conversation
                .roles
                .values()
                .any(|role| role.kind == adventuresim_dialogue::ParticipantKind::Npc)
    })
}

#[view(accessor = backend_settlement_residents, public)]
pub fn backend_settlement_residents(ctx: &ViewContext) -> Vec<BackendSettlementResident> {
    let trusted = ctx
        .db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|authority| authority.identity == ctx.sender());
    if !trusted {
        return Vec::new();
    }
    ctx.db
        .settlement_resident_profile()
        .projection_id()
        .filter(0u64..)
        .filter(resident_is_dialogue_capable)
        .filter_map(|profile| project_backend_settlement_resident(ctx, profile))
        .collect()
}

/// Public presence contains only directly observable scheduling and location facts.
#[derive(Clone, Debug)]
#[table(accessor = settlement_resident_presence, public)]
pub struct SettlementResidentPresence {
    #[primary_key]
    pub character_id: u64,
    #[index(btree)]
    pub settlement_id: String,
    #[index(btree)]
    pub location_id: String,
    pub start_minute: u16,
    pub end_minute: u16,
    pub is_default: bool,
    /// Shared schedule/service suppression while another active context owns
    /// this Character's physical presence. The authored schedule is retained.
    pub context_suppressed: bool,
    /// Ordinary health availability, independent of quest/context lifecycle.
    pub health_suppressed: bool,
}

#[derive(Clone, Debug)]
#[table(accessor = settlement_resident_seed_explanation)]
pub struct SettlementResidentSeedExplanation {
    #[primary_key]
    pub character_id: u64,
    pub seed: String,
    pub relations_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedGenerationExplanation {
    input: GenerationInput,
    profile: population::GeneratedPopulationProfile,
}

const SERVICES: [(&str, &str, &str, &str); 8] = [
    ("merchants", "market", "merchant", "market steward"),
    ("weapons", "forge", "weaponsmith", "master weaponsmith"),
    ("armor", "armoury", "armourer", "master armourer"),
    ("clothing", "tailor", "tailor", "master tailor"),
    ("herbalist", "herbalist", "herbalist", "local healer"),
    ("inn", "inn", "innkeeper", "innkeeper"),
    ("religion", "church", "cleric", "parish priest"),
    ("books", "bookstore", "merchant", "bookseller"),
];
const FEMALE_NAMES: [&str; 10] = [
    "Anna",
    "Greta",
    "Elsbeth",
    "Klara",
    "Marta",
    "Ursula",
    "Agnes",
    "Ida",
    "Dorothea",
    "Margarete",
];
const MALE_NAMES: [&str; 10] = [
    "Johann", "Hans", "Konrad", "Martin", "Peter", "Nikolaus", "Otto", "Lukas", "Heinrich",
    "Wilhelm",
];
const SURNAMES: [&str; 12] = [
    "Bauer", "Fischer", "Weber", "Schmidt", "Kramer", "Wagner", "Hoffmann", "Schulz", "Klein",
    "Wolf", "Hartmann", "Vogel",
];

fn location_context(location: &str) -> Result<LocationContext, String> {
    Ok(match location {
        "overview" => LocationContext::Overview,
        "market" => LocationContext::Market,
        "forge" => LocationContext::Forge,
        "armoury" => LocationContext::Armoury,
        "tailor" => LocationContext::Tailor,
        "herbalist" => LocationContext::Herbalist,
        "bookstore" => LocationContext::Market,
        "inn" => LocationContext::Inn,
        "church" => LocationContext::Church,
        "residences" => LocationContext::Residences,
        "keep" => LocationContext::Keep,
        location if location.starts_with("organization-") => LocationContext::Organization,
        _ => return Err(format!("Unknown population location {location}")),
    })
}
fn age(value: AgeBand) -> NpcAgeBand {
    match value {
        AgeBand::Child => NpcAgeBand::Child,
        AgeBand::Adolescent => NpcAgeBand::Adolescent,
        AgeBand::Adult => NpcAgeBand::Adult,
        AgeBand::Elder => NpcAgeBand::Elder,
    }
}
fn profession(value: Profession) -> &'static str {
    match value {
        Profession::Artisan => "artisan",
        Profession::Householder => "householder",
        Profession::Laborer => "laborer",
        Profession::Retainer => "retainer",
        Profession::ServiceProvider => "service provider",
    }
}
fn resident_name(seed: &str, female: bool) -> String {
    let hash = population::stable_hash(seed);
    let given = if female {
        FEMALE_NAMES[hash as usize % FEMALE_NAMES.len()]
    } else {
        MALE_NAMES[hash as usize % MALE_NAMES.len()]
    };
    format!(
        "{} {}",
        given,
        SURNAMES[hash.rotate_left(17) as usize % SURNAMES.len()]
    )
}

fn resident_character_id(seed: &str) -> u64 {
    // Keep generated residents in the upper half of the identity space. The
    // stable source coordinate is the identity; there is no parallel string ID.
    population::stable_hash(seed) | (1u64 << 63)
}

fn insert_resident(
    ctx: &ReducerContext,
    settlement_id: &str,
    location: &str,
    service: &str,
    provider_profession: &str,
    supplied_role: &str,
    ordinal: usize,
    is_default: bool,
) -> Result<(), String> {
    let seed = format!("resident:{settlement_id}:{location}:{ordinal}");
    insert_resident_with_seed(
        ctx,
        seed,
        settlement_id,
        location,
        service,
        provider_profession,
        supplied_role,
        is_default,
    )
}

fn insert_resident_with_seed(
    ctx: &ReducerContext,
    seed: String,
    settlement_id: &str,
    location: &str,
    service: &str,
    provider_profession: &str,
    supplied_role: &str,
    is_default: bool,
) -> Result<(), String> {
    let character_id = resident_character_id(&seed);
    if let Some(existing) = ctx
        .db
        .settlement_resident_profile()
        .character_id()
        .find(character_id)
    {
        let settlement = ctx
            .db
            .settlement()
            .id()
            .find(settlement_id.to_owned())
            .ok_or("Settlement population references an unknown settlement")?;
        let urban = matches!(
            settlement.category,
            crate::strategic::SettlementCategory::Town
                | crate::strategic::SettlementCategory::City
                | crate::strategic::SettlementCategory::Capital
        );
        crate::social_roles::ensure_character_social_roles(
            ctx,
            character_id,
            settlement_id,
            urban,
        )?;
        if existing.profession == "cleric" {
            if let Some(organization_id) =
                crate::social_roles::religious_organization_for(&settlement.religion_id)
            {
                crate::social_roles::ensure_character_professional_role(
                    ctx,
                    character_id,
                    organization_id,
                    adventuresim_core::organization::organization(organization_id)
                        .and_then(|definition| definition.entry_role_ids.first())
                        .ok_or("Religious organization has no entry role")?,
                )?;
            }
        }
        return Ok(());
    }
    let input = GenerationInput {
        seed: seed.clone(),
        location: location_context(location)?,
        is_service_provider: !service.is_empty(),
        service_id: (!service.is_empty()).then(|| service.to_owned()),
        profession_override: (!service.is_empty()).then(|| provider_profession.to_owned()),
        local_role: supplied_role.to_owned(),
        available_bridges: BTreeSet::from([
            PresenceBridge::NearbyHome,
            PresenceBridge::HouseholdErrand,
            PresenceBridge::RetainerErrand,
        ]),
    };
    let profile = population::generate(&input)?;
    let selected_profession = if service.is_empty() {
        profession(profile.profession)
    } else {
        provider_profession
    };
    let local_role = if service.is_empty()
        && profile.profession == Profession::Retainer
        && supplied_role != "reeve"
    {
        "lord's household retainer"
    } else {
        supplied_role
    };
    let female = population::stable_hash(&format!("{seed}:sex")) % 2 == 0;
    let age_band = age(profile.age);
    let household = format!(
        "the {} {}",
        SURNAMES[population::stable_hash(&format!("{seed}:house")) as usize % SURNAMES.len()],
        profile.household_kind
    );
    insert_persistent_npc_character(
        ctx,
        resident_name(&seed, female),
        character_id,
        settlement_id,
        population::stable_hash(&seed),
        None,
    )?;
    let mut character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Resident character was not created")?;
    character.age_years = match age_band {
        NpcAgeBand::Child => 8,
        NpcAgeBand::Adolescent => 15,
        NpcAgeBand::Adult => 30,
        NpcAgeBand::Elder => 68,
    };
    ctx.db.character().id().update(character);
    ctx.db.npc_policy().insert(NpcPolicy {
        character_id,
        home_settlement_id: settlement_id.into(),
        policy_seed: population::stable_hash(&seed),
    });
    let resident = ctx
        .db
        .settlement_resident_profile()
        .insert(SettlementResidentProfile {
            character_id,
            projection_id: character_id,
            home_settlement_id: settlement_id.into(),
            height: profile.height.clone(),
            build: profile.build.clone(),
            hair: profile.hair.clone(),
            facial_hair: if !female
                && population::stable_hash(&seed) % 3 == 0
                && !matches!(age_band, NpcAgeBand::Child)
            {
                "a neatly kept beard".into()
            } else {
                "none visible".into()
            },
            complexion: ["fair", "ruddy", "weathered", "olive"]
                [population::stable_hash(&format!("{seed}:complexion")) as usize % 4]
                .into(),
            visible_features: [
                "a small scar at one brow",
                "freckles",
                "work-worn hands",
                "no especially notable marks",
            ][population::stable_hash(&format!("{seed}:feature")) as usize % 4]
                .into(),
            clothing: if service.is_empty() {
                "practical local woolens".into()
            } else {
                "clean working clothes appropriate to the trade".into()
            },
            profession: selected_profession.into(),
            household,
            local_role: local_role.into(),
            service_id: service.into(),
            organization_id: String::new(),
            conversation_id: if service.is_empty() {
                "local-resident".into()
            } else if service == "herbalist" {
                "herbalist-examination".into()
            } else if service == "religion" {
                "religion-service".into()
            } else {
                "service-professions".into()
            },
        });
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(&settlement_id.to_owned())
        .ok_or("Settlement population references an unknown settlement")?;
    let urban = matches!(
        settlement.category,
        crate::strategic::SettlementCategory::Town
            | crate::strategic::SettlementCategory::City
            | crate::strategic::SettlementCategory::Capital
    );
    crate::social_roles::ensure_character_social_roles(
        ctx,
        resident.character_id,
        settlement_id,
        urban,
    )?;
    if resident.profession == "cleric" {
        if let Some(organization_id) =
            crate::social_roles::religious_organization_for(&settlement.religion_id)
        {
            crate::social_roles::ensure_character_professional_role(
                ctx,
                resident.character_id,
                organization_id,
                adventuresim_core::organization::organization(organization_id)
                    .and_then(|definition| definition.entry_role_ids.first())
                    .ok_or("Religious organization has no entry role")?,
            )?;
        }
    }
    let (start_minute, end_minute) = match profile.schedule {
        Schedule::Day => (360, 1200),
        Schedule::Evening => (720, 1380),
        Schedule::Early => (240, 960),
        Schedule::Provider => (0, 1440),
    };
    ctx.db
        .settlement_resident_presence()
        .insert(SettlementResidentPresence {
            character_id,
            settlement_id: settlement_id.into(),
            location_id: location.into(),
            start_minute,
            end_minute,
            is_default,
            context_suppressed: false,
            health_suppressed: false,
        });
    let explanation = PersistedGenerationExplanation { input, profile };
    let relations_json = serde_json::to_string(&explanation)
        .map_err(|error| format!("Could not serialize population explanation: {error}"))?;
    ctx.db
        .settlement_resident_seed_explanation()
        .insert(SettlementResidentSeedExplanation {
            character_id,
            seed,
            relations_json,
        });
    Ok(())
}

pub fn ensure_settlement_population(
    ctx: &ReducerContext,
    settlement_id: &str,
) -> Result<(), String> {
    crate::social_roles::ensure_settlement_social_organizations(ctx, settlement_id)?;
    for (service, location, profession, role) in SERVICES {
        insert_resident(
            ctx,
            settlement_id,
            location,
            service,
            profession,
            role,
            0,
            true,
        )?;
        insert_resident(
            ctx,
            settlement_id,
            location,
            "",
            "local resident",
            "customer or visitor",
            1,
            false,
        )?;
    }
    for ordinal in 0..3 {
        insert_resident(
            ctx,
            settlement_id,
            "overview",
            "",
            ["laborer", "householder", "artisan"][ordinal],
            ["neighbor", "household representative", "local resident"][ordinal],
            ordinal,
            ordinal == 0,
        )?;
    }
    insert_resident(
        ctx,
        settlement_id,
        "residences",
        "",
        "householder",
        "resident",
        0,
        true,
    )?;
    insert_resident(
        ctx,
        settlement_id,
        "residences",
        "",
        "domestic worker",
        "neighbor",
        1,
        false,
    )?;
    if ctx
        .db
        .settlement()
        .id()
        .find(&settlement_id.to_string())
        .is_some_and(|settlement| {
            matches!(
                settlement.category,
                crate::strategic::SettlementCategory::Town
                    | crate::strategic::SettlementCategory::City
                    | crate::strategic::SettlementCategory::Capital
            )
        })
    {
        insert_resident(ctx, settlement_id, "keep", "", "retainer", "reeve", 0, true)?;
        insert_resident(
            ctx,
            settlement_id,
            "keep",
            "",
            "servant",
            "keep servant",
            1,
            false,
        )?;
    }
    for organization in adventuresim_core::organization::organizations_for_chapter(settlement_id) {
        let chapter = organization
            .chapter(settlement_id)
            .expect("chapter iterator guarantees a local chapter");
        let settlement = ctx
            .db
            .settlement()
            .id()
            .find(&settlement_id.to_owned())
            .ok_or("Organization chapter references an unknown settlement")?;
        let physical_location = adventuresim_core::organization::chapter_effective_location_id(
            organization,
            chapter,
            &settlement.economy,
        );
        let representative_character_id =
            adventuresim_core::organization::organization_representative_id(
                settlement_id,
                &organization.id,
            );
        let representative_seed = format!(
            "resident:organization-representative:{settlement_id}:{}",
            organization.id
        );
        insert_resident_with_seed(
            ctx,
            representative_seed,
            settlement_id,
            physical_location,
            "organization",
            &chapter.representative_profession,
            &chapter.representative_title,
            physical_location == chapter.location_id.as_str(),
        )?;
        let mut representative = ctx
            .db
            .settlement_resident_profile()
            .character_id()
            .find(representative_character_id)
            .ok_or("Organization representative was not seeded")?;
        representative.service_id.clear();
        representative.organization_id = organization.id.clone();
        representative.conversation_id = "organization-representative".into();
        representative.clothing =
            "well-kept clothing bearing the institution's public insignia".into();
        ctx.db
            .settlement_resident_profile()
            .character_id()
            .update(representative);
    }
    crate::relationship::ensure_seeded_family_households(ctx, settlement_id)?;
    Ok(())
}
pub fn npc_is_present(
    ctx: &ReducerContext,
    presence: &SettlementResidentPresence,
    minute: u64,
) -> bool {
    npc_presence_remaining_minutes_at(ctx, presence, minute).is_some()
}

/// Canonical exact place behind an authoritative settlement NPC location.
/// The `overview` route is the presentation alias for the public square.
pub fn canonical_npc_place(settlement_id: &str, location_id: &str) -> Option<StrategicPlaceId> {
    let venue = if matches!(location_id, "overview" | "public-square") {
        Some(SettlementVenueKind::PublicSquare)
    } else {
        SettlementVenueKind::from_id(location_id)
    };
    if let Some(kind) = venue {
        return StrategicPlaceId::settlement_venue(settlement_id, kind).ok();
    }
    let (organization, chapter) =
        adventuresim_core::organization::organization_chapter_at(settlement_id, location_id)?;
    StrategicPlaceId::chapter_venue(settlement_id, &organization.id, &chapter.location_id).ok()
}

/// Typed scheduled presence at the actor-relative personal minute. Historical
/// outbreak state is reconstructed without mutating or reading future state.
pub fn npc_strategic_presence_at(
    ctx: &ReducerContext,
    presence: &SettlementResidentPresence,
    observer_character_id: u64,
    minute: u64,
) -> Option<ScheduledStrategicPresence> {
    let suppression =
        crate::outbreak::patient_presence_suppression_at(ctx, presence.character_id, minute)?;
    let alive = crate::relationship::character_alive_at(ctx, presence.character_id, minute);
    StrategicPresence::scheduled_resident(
        presence.character_id,
        canonical_npc_place(&presence.settlement_id, &presence.location_id)?,
        PresenceFrontier {
            observer_character_id,
            personal_minute: minute,
        },
        DailyPresenceWindow {
            start_minute: presence.start_minute,
            end_minute: presence.end_minute,
        },
        alive,
        suppression.context_suppressed,
        suppression.health_suppressed,
    )
    .ok()
}

/// Compatibility projection for consumers that only need schedule duration.
/// It deliberately does not fabricate a typed observer frontier.
pub fn npc_presence_remaining_minutes_at(
    ctx: &ReducerContext,
    presence: &SettlementResidentPresence,
    minute: u64,
) -> Option<u64> {
    let suppression =
        crate::outbreak::patient_presence_suppression_at(ctx, presence.character_id, minute)?;
    DailyPresenceWindow {
        start_minute: presence.start_minute,
        end_minute: presence.end_minute,
    }
    .remaining_minutes(
        minute,
        suppression.context_suppressed,
        suppression.health_suppressed,
    )
    .ok()
}

/// Remaining contiguous minutes in the NPC's current daily presence window.
/// Wrapped schedules (for example 20:00–02:00) remain one continuous window.
pub fn npc_presence_remaining_minutes(
    presence: &SettlementResidentPresence,
    minute: u64,
) -> Option<u64> {
    DailyPresenceWindow {
        start_minute: presence.start_minute,
        end_minute: presence.end_minute,
    }
    .remaining_minutes(
        minute,
        presence.context_suppressed,
        presence.health_suppressed,
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_npc_places_use_physical_venue_identity() {
        let overview = canonical_npc_place("lubeck", "overview").unwrap();
        let square = canonical_npc_place("lubeck", "public-square").unwrap();
        let inn = canonical_npc_place("lubeck", "inn").unwrap();

        assert_eq!(overview, square);
        assert_ne!(overview, inn);
        assert!(canonical_npc_place("lubeck", "unknown-route-value").is_none());
    }

    fn settlement_resident_profile() -> SettlementResidentProfile {
        SettlementResidentProfile {
            character_id: 42,
            projection_id: 42,
            home_settlement_id: "settlement:test".into(),
            height: "average height".into(),
            build: "sturdy".into(),
            hair: "braided brown hair".into(),
            facial_hair: "none visible".into(),
            complexion: "weathered".into(),
            visible_features: "a scar over one eyebrow".into(),
            clothing: "a wool coat".into(),
            profession: "merchant".into(),
            household: "market household".into(),
            local_role: "market steward".into(),
            service_id: "merchants".into(),
            organization_id: String::new(),
            conversation_id: "service-professions".into(),
        }
    }

    fn presence(start_minute: u16, end_minute: u16) -> SettlementResidentPresence {
        SettlementResidentPresence {
            character_id: 42,
            settlement_id: "settlement".into(),
            location_id: "inn".into(),
            start_minute,
            end_minute,
            is_default: true,
            context_suppressed: false,
            health_suppressed: false,
        }
    }

    #[test]
    fn presence_remaining_handles_daytime_and_wrapped_schedules() {
        let daytime = presence(480, 1_020);
        assert_eq!(npc_presence_remaining_minutes(&daytime, 900), Some(120));
        assert_eq!(npc_presence_remaining_minutes(&daytime, 1_020), None);

        let overnight = presence(1_200, 120);
        assert_eq!(npc_presence_remaining_minutes(&overnight, 1_380), Some(180));
        assert_eq!(npc_presence_remaining_minutes(&overnight, 60), Some(60));
        assert_eq!(npc_presence_remaining_minutes(&overnight, 600), None);
    }

    #[test]
    fn contextual_membership_suppresses_without_rewriting_schedule() {
        let mut row = presence(480, 1_020);
        row.context_suppressed = true;
        assert_eq!(npc_presence_remaining_minutes(&row, 900), None);
        assert_eq!((row.start_minute, row.end_minute), (480, 1_020));

        row.context_suppressed = false;
        assert_eq!(npc_presence_remaining_minutes(&row, 900), Some(120));

        row.health_suppressed = true;
        assert_eq!(npc_presence_remaining_minutes(&row, 900), None);
        assert_eq!((row.start_minute, row.end_minute), (480, 1_020));
    }

    #[test]
    fn persisted_explanation_is_valid_complete_and_deterministic() {
        let input = GenerationInput {
            seed: "npc:test".into(),
            location: LocationContext::Overview,
            is_service_provider: false,
            service_id: None,
            profession_override: None,
            local_role: "resident".into(),
            available_bridges: BTreeSet::from([
                PresenceBridge::NearbyHome,
                PresenceBridge::HouseholdErrand,
                PresenceBridge::RetainerErrand,
            ]),
        };
        let profile = population::generate(&input).unwrap();
        let value = PersistedGenerationExplanation { input, profile };
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(value, serde_json::from_str(&json).unwrap());
        assert_eq!(json, serde_json::to_string(&value).unwrap());
        assert_eq!(value.profile.decisions.len(), 7);
    }

    #[test]
    fn backend_settlement_resident_view_is_an_explicit_fail_closed_projection() {
        let source = include_str!("settlement_population.rs");
        let row = source
            .split("pub struct BackendSettlementResident {")
            .nth(1)
            .and_then(|tail| tail.split_once('}').map(|(body, _)| body))
            .expect("backend settlement NPC row");
        for field in [
            "character_id",
            "home_settlement_id",
            "name",
            "age_band",
            "presentation",
            "height",
            "build",
            "hair",
            "facial_hair",
            "complexion",
            "visible_features",
            "clothing",
            "profession",
            "household",
            "local_role",
            "service_id",
            "organization_id",
            "conversation_id",
        ] {
            assert!(
                row.contains(&format!("pub {field}:")),
                "missing player-visible field {field}"
            );
        }
        assert!(!row.contains("sex:"));
        assert!(!row.contains("projection_id:"));

        let view = source
            .split("pub fn backend_settlement_residents")
            .nth(1)
            .and_then(|tail| tail.split("/// Public presence contains").next())
            .expect("backend settlement NPC view");
        assert!(view.contains("-> Vec<BackendSettlementResident>"));
        assert!(view.contains(".filter(resident_is_dialogue_capable)"));
        assert!(
            view.contains(
                ".filter_map(|profile| project_backend_settlement_resident(ctx, profile))"
            )
        );
        assert!(!view.contains("-> Vec<SettlementResidentProfile>"));
    }

    #[test]
    fn every_authored_chapter_seeds_one_bound_persistent_representative() {
        let source = include_str!("settlement_population.rs");
        let ensure = source
            .split("pub fn ensure_settlement_population")
            .nth(1)
            .and_then(|tail| tail.split("pub fn npc_is_present").next())
            .expect("population seeding body");
        assert!(ensure.contains("organizations_for_chapter(settlement_id)"));
        assert!(ensure.contains("chapter_effective_location_id"));
        assert!(ensure.contains("representative.organization_id = organization.id.clone()"));
        assert!(ensure.contains("\"organization-representative\""));
        assert!(ensure.contains("organization_representative_id"));
        assert!(source.contains("id = format!(\"npc:{settlement_id}:{location}:{ordinal}\")"));
        assert!(ensure.contains("physical_location == chapter.location_id.as_str()"));
    }

    #[test]
    fn authoritative_presence_reconstructs_historical_outbreak_state_without_mutation() {
        let source = include_str!("settlement_population.rs");
        let typed_projection = source
            .split("pub fn npc_strategic_presence_at")
            .nth(1)
            .and_then(|tail| {
                tail.split("pub fn npc_presence_remaining_minutes_at")
                    .next()
            })
            .expect("typed historical presence projection");
        assert!(typed_projection.contains("patient_presence_suppression_at"));
        assert!(typed_projection.contains("character_alive_at"));
        assert!(!typed_projection.contains("world_clock"));
        assert!(!typed_projection.contains("refresh_patient_context_after_time_write"));
        assert!(!typed_projection.contains("character.alive"));

        let projection = source
            .split("pub fn npc_presence_remaining_minutes_at")
            .nth(1)
            .and_then(|tail| tail.split("pub fn npc_presence_remaining_minutes").next())
            .expect("authoritative presence projection");
        assert!(projection.contains("patient_presence_suppression_at"));
        assert!(!projection.contains("PresenceFrontier"));
        assert!(!projection.contains("refresh_patient_context_after_time_write"));
    }
}
