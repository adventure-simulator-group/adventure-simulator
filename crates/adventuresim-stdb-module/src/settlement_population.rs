//! Persistent strategic settlement population and authoritative observable presences.
use crate::strategic::{settlement, strategic_gateway_authority__view};
use adventuresim_core::settlement_population::{
    self as population, AgeBand, GenerationInput, LocationContext, PresenceBridge, Profession,
    Schedule,
};
use serde::{Deserialize, Serialize};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, table, view};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, SpacetimeType)]
pub enum NpcAgeBand {
    Child,
    Adolescent,
    Adult,
    Elder,
}
#[derive(Clone, Copy, Debug, SpacetimeType)]
pub enum NpcSex {
    Female,
    Male,
}
#[derive(Clone, Copy, Debug, SpacetimeType)]
pub enum NpcPresentation {
    Man,
    Ambiguous,
    Woman,
}

#[derive(Clone, Debug)]
#[table(accessor = settlement_npc)]
pub struct SettlementNpc {
    #[primary_key]
    pub id: String,
    /// Numeric bounded traversal key for the fail-closed gateway view.
    #[index(btree)]
    pub projection_id: u64,
    #[index(btree)]
    pub home_settlement_id: String,
    pub name: String,
    pub age_band: NpcAgeBand,
    /// Private demographic truth used by generated quest predicates.
    pub sex: NpcSex,
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
    /// Explicit institution authority. Empty for NPCs not representing an organization.
    pub organization_id: String,
    pub conversation_id: String,
}

/// Settlement NPC facts that the registered gateway may project to players.
///
/// Keep this row explicit: `SettlementNpc` also contains private demographic
/// and traversal authority that must never become subscription data merely
/// because the authoritative table gains another field.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendSettlementNpc {
    pub id: String,
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

fn project_backend_settlement_npc(npc: SettlementNpc) -> BackendSettlementNpc {
    BackendSettlementNpc {
        id: npc.id,
        home_settlement_id: npc.home_settlement_id,
        name: npc.name,
        age_band: npc.age_band,
        presentation: npc.presentation,
        height: npc.height,
        build: npc.build,
        hair: npc.hair,
        facial_hair: npc.facial_hair,
        complexion: npc.complexion,
        visible_features: npc.visible_features,
        clothing: npc.clothing,
        profession: npc.profession,
        household: npc.household,
        local_role: npc.local_role,
        service_id: npc.service_id,
        organization_id: npc.organization_id,
        conversation_id: npc.conversation_id,
    }
}

pub(crate) fn npc_is_dialogue_capable(npc: &SettlementNpc) -> bool {
    adventuresim_dialogue::find_conversation(&npc.conversation_id).is_some_and(|conversation| {
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

#[view(accessor = backend_settlement_npcs, public)]
pub fn backend_settlement_npcs(ctx: &ViewContext) -> Vec<BackendSettlementNpc> {
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
        .settlement_npc()
        .projection_id()
        .filter(0u64..)
        .filter(npc_is_dialogue_capable)
        .map(project_backend_settlement_npc)
        .collect()
}

/// Public presence contains only directly observable scheduling and location facts.
#[derive(Clone, Debug)]
#[table(accessor = settlement_npc_presence, public)]
pub struct SettlementNpcPresence {
    #[primary_key]
    pub npc_id: String,
    #[index(btree)]
    pub settlement_id: String,
    #[index(btree)]
    pub location_id: String,
    pub start_minute: u16,
    pub end_minute: u16,
    pub is_default: bool,
}

#[derive(Clone, Debug)]
#[table(accessor = settlement_npc_seed_explanation)]
pub struct SettlementNpcSeedExplanation {
    #[primary_key]
    pub npc_id: String,
    pub seed: String,
    pub relations_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PersistedGenerationExplanation {
    input: GenerationInput,
    profile: population::GeneratedPopulationProfile,
}

const SERVICES: [(&str, &str, &str, &str); 7] = [
    ("merchants", "market", "merchant", "market steward"),
    ("weapons", "forge", "weaponsmith", "master weaponsmith"),
    ("armor", "armoury", "armourer", "master armourer"),
    ("clothing", "tailor", "tailor", "master tailor"),
    ("herbalist", "herbalist", "herbalist", "local healer"),
    ("inn", "inn", "innkeeper", "innkeeper"),
    ("religion", "church", "cleric", "parish priest"),
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
fn npc_name(seed: &str, sex: NpcSex) -> String {
    let hash = population::stable_hash(seed);
    let given = match sex {
        NpcSex::Female => FEMALE_NAMES[hash as usize % FEMALE_NAMES.len()],
        NpcSex::Male => MALE_NAMES[hash as usize % MALE_NAMES.len()],
    };
    format!(
        "{} {}",
        given,
        SURNAMES[hash.rotate_left(17) as usize % SURNAMES.len()]
    )
}

fn npc_presentation(id: &str, sex: NpcSex) -> NpcPresentation {
    match (
        sex,
        population::stable_hash(&format!("{id}:presentation")) % 100,
    ) {
        (_, 0..=3) => NpcPresentation::Ambiguous,
        (NpcSex::Female, 4) => NpcPresentation::Man,
        (NpcSex::Male, 4) => NpcPresentation::Woman,
        (NpcSex::Female, _) => NpcPresentation::Woman,
        (NpcSex::Male, _) => NpcPresentation::Man,
    }
}

fn insert_npc(
    ctx: &ReducerContext,
    settlement_id: &str,
    location: &str,
    service: &str,
    provider_profession: &str,
    supplied_role: &str,
    ordinal: usize,
    is_default: bool,
) -> Result<(), String> {
    let id = format!("npc:{settlement_id}:{location}:{ordinal}");
    insert_npc_with_id(
        ctx,
        id,
        settlement_id,
        location,
        service,
        provider_profession,
        supplied_role,
        is_default,
    )
}

fn insert_npc_with_id(
    ctx: &ReducerContext,
    id: String,
    settlement_id: &str,
    location: &str,
    service: &str,
    provider_profession: &str,
    supplied_role: &str,
    is_default: bool,
) -> Result<(), String> {
    if let Some(existing) = ctx.db.settlement_npc().id().find(&id) {
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
        crate::social_estate::ensure_settlement_npc_social_roles(
            ctx,
            &id,
            settlement_id,
            urban,
            &existing.profession,
            &settlement.religion_id,
        )?;
        return Ok(());
    }
    let input = GenerationInput {
        seed: id.clone(),
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
    let local_role = if service.is_empty() && profile.profession == Profession::Retainer {
        "lord's household retainer"
    } else {
        supplied_role
    };
    let sex = if population::stable_hash(&format!("{id}:sex")) % 2 == 0 {
        NpcSex::Female
    } else {
        NpcSex::Male
    };
    let age_band = age(profile.age);
    let household = format!(
        "the {} {}",
        SURNAMES[population::stable_hash(&format!("{id}:house")) as usize % SURNAMES.len()],
        profile.household_kind
    );
    let npc = ctx.db.settlement_npc().insert(SettlementNpc {
        id: id.clone(),
        projection_id: population::stable_hash(&id),
        home_settlement_id: settlement_id.into(),
        name: npc_name(&id, sex),
        age_band,
        sex,
        presentation: npc_presentation(&id, sex),
        height: profile.height.clone(),
        build: profile.build.clone(),
        hair: profile.hair.clone(),
        facial_hair: if matches!(sex, NpcSex::Male)
            && population::stable_hash(&id) % 3 == 0
            && !matches!(age_band, NpcAgeBand::Child)
        {
            "a neatly kept beard".into()
        } else {
            "none visible".into()
        },
        complexion: ["fair", "ruddy", "weathered", "olive"]
            [population::stable_hash(&format!("{id}:complexion")) as usize % 4]
            .into(),
        visible_features: [
            "a small scar at one brow",
            "freckles",
            "work-worn hands",
            "no especially notable marks",
        ][population::stable_hash(&format!("{id}:feature")) as usize % 4]
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
    crate::social_estate::ensure_settlement_npc_social_roles(
        ctx,
        &npc.id,
        settlement_id,
        urban,
        &npc.profession,
        &settlement.religion_id,
    )?;
    let (start_minute, end_minute) = match profile.schedule {
        Schedule::Day => (360, 1200),
        Schedule::Evening => (720, 1380),
        Schedule::Early => (240, 960),
        Schedule::Provider => (0, 1440),
    };
    ctx.db
        .settlement_npc_presence()
        .insert(SettlementNpcPresence {
            npc_id: id.clone(),
            settlement_id: settlement_id.into(),
            location_id: location.into(),
            start_minute,
            end_minute,
            is_default,
        });
    let explanation = PersistedGenerationExplanation { input, profile };
    let relations_json = serde_json::to_string(&explanation)
        .map_err(|error| format!("Could not serialize population explanation: {error}"))?;
    ctx.db
        .settlement_npc_seed_explanation()
        .insert(SettlementNpcSeedExplanation {
            npc_id: id.clone(),
            seed: id,
            relations_json,
        });
    Ok(())
}

pub fn ensure_settlement_population(
    ctx: &ReducerContext,
    settlement_id: &str,
) -> Result<(), String> {
    crate::social_estate::ensure_settlement_social_organizations(ctx, settlement_id)?;
    for (service, location, profession, role) in SERVICES {
        insert_npc(
            ctx,
            settlement_id,
            location,
            service,
            profession,
            role,
            0,
            true,
        )?;
        insert_npc(
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
        insert_npc(
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
    insert_npc(
        ctx,
        settlement_id,
        "residences",
        "",
        "householder",
        "resident",
        0,
        true,
    )?;
    insert_npc(
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
        insert_npc(
            ctx,
            settlement_id,
            "keep",
            "",
            "retainer",
            "lord's household retainer",
            0,
            true,
        )?;
        insert_npc(
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
        let representative_id = adventuresim_core::organization::organization_representative_id(
            settlement_id,
            &organization.id,
        );
        insert_npc_with_id(
            ctx,
            representative_id.clone(),
            settlement_id,
            physical_location,
            "organization",
            &chapter.representative_profession,
            &chapter.representative_title,
            physical_location == chapter.location_id.as_str(),
        )?;
        let mut representative = ctx
            .db
            .settlement_npc()
            .id()
            .find(&representative_id)
            .ok_or("Organization representative was not seeded")?;
        representative.service_id.clear();
        representative.organization_id = organization.id.clone();
        representative.conversation_id = "organization-representative".into();
        representative.clothing =
            "well-kept clothing bearing the institution's public insignia".into();
        ctx.db.settlement_npc().id().update(representative);
    }
    Ok(())
}
pub fn npc_is_present(presence: &SettlementNpcPresence, minute: u64) -> bool {
    npc_presence_remaining_minutes(presence, minute).is_some()
}

/// Remaining contiguous minutes in the NPC's current daily presence window.
/// Wrapped schedules (for example 20:00–02:00) remain one continuous window.
pub fn npc_presence_remaining_minutes(
    presence: &SettlementNpcPresence,
    minute: u64,
) -> Option<u64> {
    let minute = minute % 1_440;
    let start = u64::from(presence.start_minute);
    let end = u64::from(presence.end_minute);
    if start == end {
        return None;
    }
    if start < end {
        (start <= minute && minute < end).then_some(end - minute)
    } else if minute >= start {
        Some((1_440 - minute) + end)
    } else {
        (minute < end).then_some(end - minute)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settlement_npc() -> SettlementNpc {
        SettlementNpc {
            id: "npc:test".into(),
            projection_id: 42,
            home_settlement_id: "settlement:test".into(),
            name: "Klara Example".into(),
            age_band: NpcAgeBand::Adult,
            sex: NpcSex::Female,
            presentation: NpcPresentation::Ambiguous,
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

    fn presence(start_minute: u16, end_minute: u16) -> SettlementNpcPresence {
        SettlementNpcPresence {
            npc_id: "npc".into(),
            settlement_id: "settlement".into(),
            location_id: "inn".into(),
            start_minute,
            end_minute,
            is_default: true,
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
    fn presentation_is_correlated_with_but_does_not_encode_private_sex() {
        let mut female_cross_or_ambiguous = 0;
        let mut male_cross_or_ambiguous = 0;
        for index in 0..2_000 {
            let id = format!("npc:test:{index}");
            female_cross_or_ambiguous += usize::from(!matches!(
                npc_presentation(&id, NpcSex::Female),
                NpcPresentation::Woman
            ));
            male_cross_or_ambiguous += usize::from(!matches!(
                npc_presentation(&id, NpcSex::Male),
                NpcPresentation::Man
            ));
        }
        assert!((40..160).contains(&female_cross_or_ambiguous));
        assert!((40..160).contains(&male_cross_or_ambiguous));
    }

    #[test]
    fn backend_settlement_npc_projection_contains_only_visible_fields() {
        let row = project_backend_settlement_npc(settlement_npc());
        assert_eq!(row.id, "npc:test");
        assert_eq!(row.home_settlement_id, "settlement:test");
        assert_eq!(row.name, "Klara Example");
        assert!(matches!(row.age_band, NpcAgeBand::Adult));
        assert!(matches!(row.presentation, NpcPresentation::Ambiguous));
        assert_eq!(row.height, "average height");
        assert_eq!(row.build, "sturdy");
        assert_eq!(row.hair, "braided brown hair");
        assert_eq!(row.facial_hair, "none visible");
        assert_eq!(row.complexion, "weathered");
        assert_eq!(row.visible_features, "a scar over one eyebrow");
        assert_eq!(row.clothing, "a wool coat");
        assert_eq!(row.profession, "merchant");
        assert_eq!(row.household, "market household");
        assert_eq!(row.local_role, "market steward");
        assert_eq!(row.service_id, "merchants");
        assert!(row.organization_id.is_empty());
        assert_eq!(row.conversation_id, "service-professions");
    }

    #[test]
    fn backend_settlement_npc_view_is_an_explicit_fail_closed_projection() {
        let source = include_str!("settlement_population.rs");
        let row = source
            .split("pub struct BackendSettlementNpc {")
            .nth(1)
            .and_then(|tail| tail.split_once('}').map(|(body, _)| body))
            .expect("backend settlement NPC row");
        for field in [
            "id",
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
            .split("pub fn backend_settlement_npcs")
            .nth(1)
            .and_then(|tail| tail.split("/// Public presence contains").next())
            .expect("backend settlement NPC view");
        assert!(view.contains("-> Vec<BackendSettlementNpc>"));
        assert!(view.contains(".filter(npc_is_dialogue_capable)"));
        assert!(view.contains(".map(project_backend_settlement_npc)"));
        assert!(!view.contains("-> Vec<SettlementNpc>"));
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
}
