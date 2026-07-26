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
    Masculine,
    Ambiguous,
    Feminine,
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
    pub conversation_id: String,
}

#[view(accessor = backend_settlement_npcs, public)]
pub fn backend_settlement_npcs(ctx: &ViewContext) -> Vec<SettlementNpc> {
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
        (NpcSex::Female, 4) => NpcPresentation::Masculine,
        (NpcSex::Male, 4) => NpcPresentation::Feminine,
        (NpcSex::Female, _) => NpcPresentation::Feminine,
        (NpcSex::Male, _) => NpcPresentation::Masculine,
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
    if ctx.db.settlement_npc().id().find(&id).is_some() {
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
    ctx.db.settlement_npc().insert(SettlementNpc {
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
    Ok(())
}
pub fn npc_is_present(presence: &SettlementNpcPresence, minute: u64) -> bool {
    let minute = (minute % 1440) as u16;
    presence.start_minute <= minute && minute < presence.end_minute
}

#[cfg(test)]
mod tests {
    use super::*;
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
                NpcPresentation::Feminine
            ));
            male_cross_or_ambiguous += usize::from(!matches!(
                npc_presentation(&id, NpcSex::Male),
                NpcPresentation::Masculine
            ));
        }
        assert!((40..160).contains(&female_cross_or_ambiguous));
        assert!((40..160).contains(&male_cross_or_ambiguous));
    }
}
