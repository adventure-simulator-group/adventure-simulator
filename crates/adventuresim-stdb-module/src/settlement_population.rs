//! Persistent strategic settlement population and authoritative presences.
use crate::strategic::settlement;
use spacetimedb::{ReducerContext, SpacetimeType, Table, table};

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

#[derive(Clone, Debug)]
#[table(accessor = settlement_npc, public)]
pub struct SettlementNpc {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub home_settlement_id: String,
    pub name: String,
    pub age_band: NpcAgeBand,
    pub sex: NpcSex,
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
    /// Empty for an ordinary local.
    pub service_id: String,
    pub conversation_id: String,
}

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
    pub circumstance: String,
}

/// Private audit boundary. Ordinary subscriptions and the web DTO never expose it.
#[derive(Clone, Debug)]
#[table(accessor = settlement_npc_seed_explanation)]
pub struct SettlementNpcSeedExplanation {
    #[primary_key]
    pub npc_id: String,
    pub seed: String,
    pub relations_json: String,
}

#[derive(Clone, Copy)]
struct Choice<'a> {
    value: &'a str,
    plausibility: u32,
    curation: u32,
    bridge: Option<&'a str>,
}

fn stable_hash(value: &str) -> u64 {
    value.bytes().fold(1_469_598_103_934_665_603, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(1_099_511_628_211)
    })
}

fn choose(seed: &str, choices: &[Choice<'static>]) -> Result<Choice<'static>, String> {
    let total: u64 = choices
        .iter()
        .filter(|c| c.plausibility > 0)
        .map(|c| u64::from(c.plausibility) * u64::from(c.curation))
        .sum();
    if total == 0 {
        return Err(format!("No valid weighted population choice for {seed}"));
    }
    let mut draw = stable_hash(seed) % total;
    for choice in choices.iter().copied().filter(|c| c.plausibility > 0) {
        let weight = u64::from(choice.plausibility) * u64::from(choice.curation);
        if draw < weight {
            return Ok(choice);
        }
        draw -= weight;
    }
    Err("Weighted population selection exhausted unexpectedly".into())
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

fn npc_name(seed: &str, sex: NpcSex) -> String {
    let hash = stable_hash(seed);
    let given = match sex {
        NpcSex::Female => FEMALE_NAMES[hash as usize % FEMALE_NAMES.len()],
        NpcSex::Male => MALE_NAMES[hash as usize % MALE_NAMES.len()],
    };
    format!(
        "{} {}",
        given,
        SURNAMES[(hash.rotate_left(17) as usize) % SURNAMES.len()]
    )
}

fn profile_choice(
    seed: &str,
    dimension: &str,
    values: &[Choice<'static>],
) -> Result<Choice<'static>, String> {
    choose(&format!("{seed}:{dimension}"), values)
}

fn insert_npc(
    ctx: &ReducerContext,
    settlement_id: &str,
    location: &str,
    service: &str,
    profession: &str,
    local_role: &str,
    ordinal: usize,
    is_default: bool,
) -> Result<(), String> {
    let id = format!("npc:{settlement_id}:{location}:{ordinal}");
    if ctx.db.settlement_npc().id().find(&id).is_some() {
        return Ok(());
    }
    let age = profile_choice(
        &id,
        "age",
        &[
            Choice {
                value: "adult",
                plausibility: if service.is_empty() { 62 } else { 85 },
                curation: 10,
                bridge: None,
            },
            Choice {
                value: "elder",
                plausibility: if service.is_empty() { 18 } else { 15 },
                curation: 10,
                bridge: None,
            },
            Choice {
                value: "adolescent",
                plausibility: if service.is_empty() { 15 } else { 0 },
                curation: 10,
                bridge: Some("household errand"),
            },
            Choice {
                value: "child",
                plausibility: if service.is_empty() { 5 } else { 0 },
                curation: 10,
                bridge: Some("nearby home"),
            },
        ],
    )?;
    if matches!(age.value, "child" | "adolescent") && age.bridge.is_none() {
        return Err("Rare young presence lacks a causal bridge".into());
    }
    let age_band = match age.value {
        "child" => NpcAgeBand::Child,
        "adolescent" => NpcAgeBand::Adolescent,
        "elder" => NpcAgeBand::Elder,
        _ => NpcAgeBand::Adult,
    };
    let sex = if stable_hash(&format!("{id}:sex")) % 2 == 0 {
        NpcSex::Female
    } else {
        NpcSex::Male
    };
    let height = profile_choice(
        &id,
        "height",
        &[
            Choice {
                value: "short",
                plausibility: 25,
                curation: 10,
                bridge: None,
            },
            Choice {
                value: "average height",
                plausibility: 55,
                curation: 10,
                bridge: None,
            },
            Choice {
                value: "tall",
                plausibility: 20,
                curation: 10,
                bridge: None,
            },
        ],
    )?;
    let build = profile_choice(
        &id,
        "build",
        &[
            Choice {
                value: "slender",
                plausibility: 30,
                curation: 10,
                bridge: None,
            },
            Choice {
                value: "sturdy",
                plausibility: if location == "forge" || location == "armoury" {
                    65
                } else {
                    35
                },
                curation: 10,
                bridge: None,
            },
            Choice {
                value: "broad",
                plausibility: 20,
                curation: 10,
                bridge: None,
            },
        ],
    )?;
    let hair = profile_choice(
        &id,
        "hair",
        &[
            Choice {
                value: "brown hair",
                plausibility: 45,
                curation: 10,
                bridge: None,
            },
            Choice {
                value: "fair hair",
                plausibility: 25,
                curation: 10,
                bridge: None,
            },
            Choice {
                value: "black hair",
                plausibility: 15,
                curation: 10,
                bridge: None,
            },
            Choice {
                value: "red hair",
                plausibility: 5,
                curation: 10,
                bridge: None,
            },
            Choice {
                value: "grey hair",
                plausibility: if age.value == "elder" { 60 } else { 5 },
                curation: 10,
                bridge: None,
            },
        ],
    )?;
    let (profession, local_role) = if service.is_empty() {
        let profession_choice = profile_choice(
            &id,
            "profession_at_location",
            &[
                Choice {
                    value: "artisan",
                    plausibility: if matches!(location, "forge" | "armoury" | "tailor" | "market") {
                        55
                    } else {
                        18
                    },
                    curation: 10,
                    bridge: None,
                },
                Choice {
                    value: "householder",
                    plausibility: if location == "overview" { 45 } else { 20 },
                    curation: 10,
                    bridge: None,
                },
                Choice {
                    value: "laborer",
                    plausibility: 30,
                    curation: 10,
                    bridge: None,
                },
                Choice {
                    value: "retainer",
                    plausibility: if location == "keep" { 70 } else { 2 },
                    curation: 10,
                    bridge: (location != "keep").then_some("errand"),
                },
            ],
        )?;
        if profession_choice.plausibility <= 2 && profession_choice.bridge.is_none() {
            return Err("Rare profession/location choice lacks a causal bridge".into());
        }
        (
            profession_choice.value,
            if profession_choice.value == "retainer" {
                "lord's household retainer"
            } else {
                local_role
            },
        )
    } else {
        (profession, local_role)
    };
    let clothing = if service.is_empty() {
        "practical local woolens"
    } else {
        "clean working clothes appropriate to the trade"
    };
    let household = format!(
        "the {} household",
        SURNAMES[(stable_hash(&format!("{id}:house")) as usize) % SURNAMES.len()]
    );
    let circumstance = age.bridge.unwrap_or(if service.is_empty() {
        "ordinary daily business"
    } else {
        "working hours"
    });
    ctx.db.settlement_npc().insert(SettlementNpc {
        id: id.clone(),
        home_settlement_id: settlement_id.into(),
        name: npc_name(&id, sex),
        age_band,
        sex,
        height: height.value.into(),
        build: build.value.into(),
        hair: hair.value.into(),
        facial_hair: if matches!(sex, NpcSex::Male)
            && stable_hash(&id) % 3 == 0
            && !matches!(age_band, NpcAgeBand::Child)
        {
            "a neatly kept beard".into()
        } else {
            "none visible".into()
        },
        complexion: ["fair", "ruddy", "weathered", "olive"]
            [(stable_hash(&format!("{id}:complexion")) as usize) % 4]
            .into(),
        visible_features: [
            "a small scar at one brow",
            "freckles",
            "work-worn hands",
            "no especially notable marks",
        ][(stable_hash(&format!("{id}:feature")) as usize) % 4]
            .into(),
        clothing: clothing.into(),
        profession: profession.into(),
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
    let schedule = if service.is_empty() {
        profile_choice(
            &id,
            "schedule_at_location",
            &[
                Choice {
                    value: "day",
                    plausibility: if location == "inn" { 35 } else { 75 },
                    curation: 10,
                    bridge: None,
                },
                Choice {
                    value: "evening",
                    plausibility: if location == "inn" { 65 } else { 15 },
                    curation: 10,
                    bridge: None,
                },
                Choice {
                    value: "early",
                    plausibility: if matches!(profession, "laborer" | "artisan") {
                        35
                    } else {
                        10
                    },
                    curation: 10,
                    bridge: None,
                },
            ],
        )?
        .value
    } else {
        "provider"
    };
    let (start_minute, end_minute) = match schedule {
        "day" => (360, 1_200),
        "evening" => (720, 1_380),
        "early" => (240, 960),
        _ => (0, 1_440),
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
            circumstance: circumstance.into(),
        });
    ctx.db.settlement_npc_seed_explanation().insert(SettlementNpcSeedExplanation { npc_id: id.clone(), seed: id, relations_json: format!(r#"{{"age":{{"choice":"{}","plausibility":{},"curation":{},"bridge":{:?}}},"location":"{}","profession":"{}"}}"#, age.value, age.plausibility, age.curation, age.bridge, location, profession) });
    Ok(())
}

pub fn ensure_settlement_population(
    ctx: &ReducerContext,
    settlement_id: &str,
) -> Result<(), String> {
    // Every service gets its persistent provider and another plausible occupant.
    for (service, location, profession, local_role) in SERVICES {
        insert_npc(
            ctx,
            settlement_id,
            location,
            service,
            profession,
            local_role,
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
    if let Some(settlement) = ctx.db.settlement().id().find(&settlement_id.to_string()) {
        if matches!(
            settlement.category,
            crate::strategic::SettlementCategory::Town
                | crate::strategic::SettlementCategory::City
                | crate::strategic::SettlementCategory::Capital
        ) {
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
    }
    Ok(())
}

pub fn npc_is_present(presence: &SettlementNpcPresence, minute: u64) -> bool {
    let minute = (minute % 1_440) as u16;
    presence.start_minute <= minute && minute < presence.end_minute
}
