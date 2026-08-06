// Private, capability-gated catalog for isolated strategic development scenarios.
//
// Scenario metadata is deliberately separate from player and quest models. A
// normal module build cannot project, adopt, or mutate this authority.

#[derive(Clone, Debug)]
#[table(accessor = development_scenario)]
pub struct DevelopmentScenario {
    #[primary_key]
    pub slug: String,
    #[index(btree)]
    pub scan_id: u64,
    pub revision: u16,
    #[index(btree)]
    pub category: String,
    pub label: String,
    pub description: String,
    #[unique]
    pub primary_character_id: u64,
    pub entry_route: String,
}

#[derive(Clone, Debug)]
#[table(accessor = development_scenario_subject)]
pub struct DevelopmentScenarioSubject {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub scan_id: u64,
    #[index(btree)]
    pub scenario_slug: String,
    pub subject_kind: String,
    pub subject_id: String,
}

#[derive(Clone, Debug)]
#[table(accessor = development_scenario_update_receipt)]
pub struct DevelopmentScenarioUpdateReceipt {
    #[primary_key]
    pub request_id: String,
    pub scenario_slug: String,
    pub problem_id: String,
    pub resulting_incident_ordinal: u16,
    pub occurred_at_minute: u64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendDevelopmentScenario {
    pub slug: String,
    pub revision: u16,
    pub category: String,
    pub label: String,
    pub description: String,
    pub primary_character_id: u64,
    pub entry_route: String,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendDevelopmentQuest {
    pub scenario_slug: String,
    pub quest_kind: String,
    pub subject_id: String,
    pub canonical_case_id: String,
    pub title: String,
    pub status: String,
    pub incident_count: u16,
    pub public_awareness_bps: u16,
    pub supports_incident_action: bool,
    pub player_safe_summary: String,
}

pub(crate) fn development_capability_enabled() -> bool {
    COMPILED_DEV_BOOTSTRAP_TOKEN.is_some_and(|token| {
        adventuresim_core::simulation_security::simulation_bootstrap_authorized(
            COMPILED_DEV_BOOTSTRAP_TOKEN,
            token,
        )
    })
}

pub(crate) fn require_development_gateway(ctx: &ReducerContext) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    development_capability_enabled()
        .then_some(())
        .ok_or_else(|| "Development scenarios are disabled in this module build".into())
}

pub(crate) fn register_development_scenario(
    ctx: &ReducerContext,
    slug: &str,
    category: &str,
    label: &str,
    description: &str,
    primary_character_id: u64,
    entry_route: &str,
) -> Result<(), String> {
    if slug.is_empty()
        || slug.len() > 96
        || !slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
        || !entry_route.starts_with('/')
        || entry_route.len() > 256
    {
        return Err("Development scenario metadata is invalid".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(primary_character_id)
        .ok_or("Development scenario primary character is missing")?;
    if character.temporary {
        return Err("Development scenario primary character cannot be temporary".into());
    }
    let row = DevelopmentScenario {
        slug: slug.into(),
        scan_id: adventuresim_core::settlement_population::stable_hash(&format!(
            "development-scenario:{slug}"
        )),
        revision: 1,
        category: category.into(),
        label: label.into(),
        description: description.into(),
        primary_character_id,
        entry_route: entry_route.into(),
    };
    if let Some(existing) = ctx
        .db
        .development_scenario()
        .slug()
        .find(slug.to_owned())
    {
        if existing.primary_character_id != primary_character_id {
            return Err("Development scenario slug conflicts with another primary".into());
        }
        ctx.db.development_scenario().slug().update(row);
    } else {
        ctx.db.development_scenario().insert(row);
    }
    Ok(())
}

pub(crate) fn register_development_subject(
    ctx: &ReducerContext,
    scenario_slug: &str,
    subject_kind: &str,
    subject_id: &str,
) -> Result<(), String> {
    if ctx
        .db
        .development_scenario()
        .slug()
        .find(scenario_slug.to_owned())
        .is_none()
    {
        return Err("Development scenario subject has no registered scenario".into());
    }
    let row = DevelopmentScenarioSubject {
        id: format!("{scenario_slug}:{subject_kind}:{subject_id}"),
        scan_id: adventuresim_core::settlement_population::stable_hash(&format!(
            "development-scenario-subject:{scenario_slug}:{subject_kind}:{subject_id}"
        )),
        scenario_slug: scenario_slug.into(),
        subject_kind: subject_kind.into(),
        subject_id: subject_id.into(),
    };
    if ctx
        .db
        .development_scenario_subject()
        .id()
        .find(row.id.clone())
        .is_none()
    {
        ctx.db.development_scenario_subject().insert(row);
    }
    Ok(())
}

fn ensure_scenario_character(
    ctx: &ReducerContext,
    character_id: u64,
    name: &str,
) -> Result<(), String> {
    if ctx.db.character().id().find(character_id).is_none() {
        crate::character::insert_new_character(ctx, name.into(), character_id, false)?;
    }
    Ok(())
}

fn ensure_scenario_settlement(ctx: &ReducerContext, id: &str, name: &str) -> Result<(), String> {
    if let Some(existing) = ctx.db.settlement().id().find(&id.to_owned()) {
        return (existing.name == name)
            .then_some(())
            .ok_or_else(|| "Development scenario settlement identity conflicts".into());
    }
    let mut settlement = ctx
        .db
        .settlement()
        .id()
        .find(&"riverdale".to_owned())
        .ok_or("Development scenario settlement template is missing")?;
    settlement.id = id.into();
    settlement.name = name.into();
    settlement.source_node_id = None;
    ctx.db.settlement().insert(settlement);
    ensure_settlement_activity(ctx, id.into())
}

pub(crate) fn ensure_foraging_demo_settlement(ctx: &ReducerContext) -> Result<(), String> {
    const ID: &str = "dev-scenario-foraging";
    let (mut settlement, exists) = if let Some(existing) = ctx.db.settlement().id().find(&ID.to_owned()) {
        (existing, true)
    } else {
        (
            ctx.db
                .settlement()
                .id()
                .find(&"riverdale".to_owned())
                .ok_or("Foraging demo settlement template is missing")?,
            false,
        )
    };
    // Empirically sampled from the pinned final terrain pack: uncultivated
    // deep woods with no crossing or wetland fraction.
    settlement.id = ID.into();
    settlement.name = "Foraging Demo Woods".into();
    settlement.coord_x = 9.75;
    settlement.coord_y = 51.75;
    settlement.source_node_id = None;
    if exists {
        ctx.db.settlement().id().update(settlement);
    } else {
        ctx.db.settlement().insert(settlement);
    }
    ensure_settlement_activity(ctx, ID.into())
}

fn ensure_scenario_character_at(
    ctx: &ReducerContext,
    character_id: u64,
    name: &str,
    settlement_id: &str,
) -> Result<(), String> {
    if let Some(character) = ctx.db.character().id().find(character_id) {
        return (character.current_settlement_id.as_deref() == Some(settlement_id))
            .then_some(())
            .ok_or_else(|| "Development scenario character is in the wrong settlement".into());
    }
    crate::character::insert_character_with_origin(
        ctx,
        name.into(),
        character_id,
        crate::character::CharacterCreationOptions {
            origin_settlement_id: Some(settlement_id),
            mode: crate::character::CharacterCreationMode::Player,
            create_solo_party: true,
            stable_seed: character_id,
            initial_time_minute: None,
            field_actor: false,
        },
        None,
        None,
    )
}

const RECURRING_THREAT_RATIONS: u32 = 10;
const RECURRING_THREAT_WATERSKINS: u32 = 4;
const RECURRING_THREAT_FIELD_TENTS: u32 = 1;

fn ensure_recurring_threat_provisions(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<(), String> {
    let party_id = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .and_then(|character| character.party_id)
        .ok_or("Recurring-threat scenario character has no party")?;
    for (item_id, target_quantity) in [
        (
            adventuresim_core::provisioning::STANDARD_TRAVEL_RATION_ID,
            RECURRING_THREAT_RATIONS,
        ),
        (
            adventuresim_core::provisioning::STANDARD_WATERSKIN_ID,
            RECURRING_THREAT_WATERSKINS,
        ),
        (
            adventuresim_core::item_references::FIELD_TENT_ID,
            RECURRING_THREAT_FIELD_TENTS,
        ),
    ] {
        let current_quantity = ctx
            .db
            .party_inventory_item()
            .party_id()
            .filter(&party_id)
            .filter(|row| row.item_id == item_id)
            .map(|row| row.quantity)
            .sum::<u32>();
        add_to_party_inventory_checked(
            ctx,
            &party_id,
            item_id,
            target_quantity.saturating_sub(current_quantity),
        )?;
    }
    Ok(())
}

/// Register every fixture from the one strategic bootstrap and materialize
/// feature states against distinct primary characters.
pub(crate) fn materialize_development_scenario_gallery(
    ctx: &ReducerContext,
) -> Result<(), String> {
    let static_scenarios = [
        ("health-disease-party", "Health", "Disease diagnosis party", "Diagnose a party containing every authored disease stage.", 9_999_999_999_999_998, "/characters"),
        ("health-wounded-party", "Health", "Wounds and surgery", "Treat wounds, retained projectiles, splints, and damaged equipment.", 9_999_999_999_999_999, "/characters"),
        ("knowledge-religion", "Knowledge", "Religion knowledge", "Inspect the complete bounded religion skill presentation.", 9_999_999_999_999_988, "/characters"),
        ("knowledge-bestiary", "Knowledge", "Bestiary knowledge", "Inspect broad bestiary category knowledge and evidence.", 9_999_999_999_999_987, "/characters"),
        ("knowledge-herbalism", "Knowledge", "Herbalism and foraging", "Exercise every bounded herbalism method, public grade, and the terrain-backed foraging flow.", 9_999_999_999_999_986, "/characters"),
        ("social-affinity", "Social", "Affinity and courtship", "Exercise visible affinity, belief, morale, and social actions.", 9_999_999_999_999_977, "/characters"),
        ("social-prayer", "Social", "Zealous prayer", "Exercise conviction-sensitive prayer interactions.", 9_999_999_999_999_975, "/characters"),
    ];
    for (slug, category, label, description, character_id, route) in static_scenarios {
        register_development_scenario(ctx, slug, category, label, description, character_id, route)?;
    }

    const AUTOPSY_ID: u64 = 9_999_999_999_999_960;
    ensure_scenario_character(ctx, AUTOPSY_ID, "Anatomist Demo")?;
    crate::corpse::seed_autopsy_demo(ctx, AUTOPSY_ID)?;
    register_development_scenario(ctx, "health-autopsy", "Health", "Autopsy", "Examine a deterministic corpse through the ordinary settlement UI.", AUTOPSY_ID, "/characters")?;

    const OUTBREAK_ID: u64 = 9_999_999_999_999_959;
    const OUTBREAK_SETTLEMENT: &str = "dev-scenario-outbreak";
    ensure_scenario_settlement(ctx, OUTBREAK_SETTLEMENT, "Outbreak Scenario Hamlet")?;
    ensure_scenario_character_at(ctx, OUTBREAK_ID, "Outbreak Investigator", OUTBREAK_SETTLEMENT)?;
    let outbreak_problem_id = seed_outbreak_demo(ctx, OUTBREAK_ID)?;
    crate::local_problem::discover_development_problem(
        ctx,
        OUTBREAK_ID,
        &outbreak_problem_id,
        "quest-outbreak",
    )?;
    register_development_scenario(ctx, "quest-outbreak", "Quests", "Discovered outbreak", "Continue a deterministic outbreak from its ordinary rumor-derived journal entry, referral, and investigation actions.", OUTBREAK_ID, "/quests")?;
    register_development_subject(ctx, "quest-outbreak", "generated_problem", &outbreak_problem_id)?;

    const THREAT_ID: u64 = 9_999_999_999_999_958;
    const THREAT_SETTLEMENT: &str = "dev-scenario-recurring-threat";
    ensure_scenario_settlement(ctx, THREAT_SETTLEMENT, "Threat Scenario Hamlet")?;
    ensure_scenario_character_at(ctx, THREAT_ID, "Threat Investigator", THREAT_SETTLEMENT)?;
    if let Some(mut attributes) = ctx.db.character_attributes().character_id().find(THREAT_ID) {
        attributes.instinct = 5.0;
        ctx.db
            .character_attributes()
            .character_id()
            .update(attributes);
    }
    if let Some(mut skills) = ctx.db.character_skills().character_id().find(THREAT_ID) {
        skills.charm_hours = skills.charm_hours.max(10_000.0);
        skills.command_hours = skills.command_hours.max(10_000.0);
        skills.oral_languages.east_central = 10_000.0;
        skills.oral_languages.west_central = 10_000.0;
        skills.oral_languages.low = 10_000.0;
        skills.oral_languages.yiddish = 10_000.0;
        skills.oral_languages.latin = 10_000.0;
        skills.oral_languages.romani = 10_000.0;
        skills.oral_languages.elven = 10_000.0;
        skills.oral_languages.dwarfish = 10_000.0;
        ctx.db.character_skills().character_id().update(skills);
    }
    ensure_recurring_threat_provisions(ctx, THREAT_ID)?;
    debug_assert!(
        adventuresim_core::strategic_action::assess_negotiated_withdrawal(
            5.0, 1.0, 0.0, 50,
        )
        .accepted
    );
    let threat_problem_id = materialize_preferred_generated_fixture(
        ctx,
        THREAT_ID,
        adventuresim_core::quest_generation::TemplateFamily::RecurringDepredation,
        0x5448_5245_4154_0001,
    )?;
    crate::local_problem::discover_development_problem(
        ctx,
        THREAT_ID,
        &threat_problem_id,
        "quest-recurring-threat",
    )?;
    register_development_scenario(ctx, "quest-recurring-threat", "Quests", "Combat or negotiated withdrawal", "Follow a deterministic sapient hostile threat to its case site, then resolve it through ordinary combat or a pre-combat demand to withdraw.", THREAT_ID, "/quests")?;
    register_development_subject(ctx, "quest-recurring-threat", "generated_problem", &threat_problem_id)?;

    for (offset, kind) in [
        ErrantryPuzzleKind::OrderedSigils,
        ErrantryPuzzleKind::TruthfulWitnesses,
        ErrantryPuzzleKind::RuneTransformation,
        ErrantryPuzzleKind::LogicGrid,
        ErrantryPuzzleKind::ResourceAllocation,
    ]
    .into_iter()
    .enumerate()
    {
        let slug = format!("puzzle-{}", kind.core().slug());
        let character_id = 9_999_999_999_999_950 - offset as u64;
        ensure_scenario_character(ctx, character_id, &format!("{} Puzzle Tester", kind.core().slug()))?;
        let materialized = materialize_order_errantry(
            ctx,
            character_id,
            None,
            ErrantryLaunch::DirectDemoCamp(kind),
        )?;
        register_development_scenario(ctx, &slug, "Puzzles", &format!("{} puzzle", kind.core().slug().replace('-', " ")), "Solve this puzzle from its ordinary journey-camp entry state.", character_id, "/camp")?;
        register_development_subject(ctx, &slug, "case", &materialized.case_id)?;
    }

    for (ordinal, definition) in adventuresim_core::road_encounter_catalog::definitions()
        .iter()
        .enumerate()
    {
        let scenario_slug = format!("road-{}", definition.id);
        let character_id = 9_999_999_999_990_000_u64.saturating_sub(ordinal as u64);
        ensure_scenario_character(ctx, character_id, &format!("Road Tester {}", ordinal + 1))?;
        let occurrence_id = materialize_development_road_encounter(ctx, character_id, &definition.id)?;
        let label = definition
            .cast
            .first()
            .map_or_else(|| definition.id.replace('-', " ").replace('_', " "), |speaker| format!("Encounter with {}", speaker.name));
        register_development_scenario(ctx, &scenario_slug, "Road encounters", &label, "Play this compiled encounter through its ordinary journey-camp presentation.", character_id, "/camp")?;
        register_development_subject(ctx, &scenario_slug, "road_encounter", &occurrence_id)?;
    }

    // Postcondition validation makes a partial gallery abort transactionally.
    for scenario in ctx.db.development_scenario().iter() {
        if ctx.db.character().id().find(scenario.primary_character_id).is_none() {
            return Err(format!("Scenario {} has no primary character", scenario.slug));
        }
    }
    Ok(())
}

#[view(accessor = backend_development_scenarios, public)]
pub fn backend_development_scenarios(ctx: &ViewContext) -> Vec<BackendDevelopmentScenario> {
    if !development_capability_enabled() || !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    let mut rows = ctx
        .db
        .development_scenario()
        .scan_id()
        .filter(0u64..)
        .map(|row| BackendDevelopmentScenario {
            slug: row.slug,
            revision: row.revision,
            category: row.category,
            label: row.label,
            description: row.description,
            primary_character_id: row.primary_character_id,
            entry_route: row.entry_route,
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        left.category
            .cmp(&right.category)
            .then_with(|| left.label.cmp(&right.label))
    });
    rows.truncate(256);
    rows
}

#[view(accessor = backend_development_quests, public)]
pub fn backend_development_quests(ctx: &ViewContext) -> Vec<BackendDevelopmentQuest> {
    if !development_capability_enabled() || !strategic_view_is_gateway(ctx) {
        return Vec::new();
    }
    const MAX_SUBJECT_INPUTS: usize = 512;
    const MAX_KIND_INPUTS: usize = 256;
    const MAX_OUTPUTS: usize = 256;

    let mut subjects = ctx
        .db
        .development_scenario_subject()
        .scan_id()
        .filter(0u64..)
        .take(MAX_SUBJECT_INPUTS)
        .collect::<Vec<_>>();
    subjects.sort_by(|left, right| left.id.cmp(&right.id));
    let subject_scenarios = subjects
        .into_iter()
        .map(|subject| {
            (
                (subject.subject_kind, subject.subject_id),
                subject.scenario_slug,
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut rows = Vec::new();
    let mut problems = ctx
        .db
        .local_problem_authority()
        .gateway_bucket()
        .filter(0u8)
        .take(MAX_KIND_INPUTS)
        .collect::<Vec<_>>();
    problems.sort_by(|left, right| left.id.cmp(&right.id));
    for problem in problems {
        let scenario_slug = subject_scenarios
            .get(&("generated_problem".into(), problem.id.clone()))
            .cloned()
            .unwrap_or_default();
        let player_safe_summary = ctx
            .db
            .local_problem_symptom()
            .problem_id()
            .find(problem.id.clone())
            .map_or_else(|| "Not publicly described".into(), |row| row.public_summary);
        let supports_incident_action = !scenario_slug.is_empty()
            && problem.recurring_hostile
            && problem.resolved_at.is_none();
        rows.push(BackendDevelopmentQuest {
            scenario_slug,
            quest_kind: "generated problem".into(),
            subject_id: problem.id,
            canonical_case_id: problem.opaque_case_ref,
            title: problem.symptom,
            status: if problem.resolved_at.is_some() {
                "resolved".into()
            } else {
                "active".into()
            },
            incident_count: problem.incident_count,
            public_awareness_bps: problem.public_awareness_bps,
            supports_incident_action,
            player_safe_summary,
        });
    }

    let mut contracts = ctx
        .db
        .contract_authority()
        .gateway_bucket()
        .filter(0u8)
        .take(MAX_KIND_INPUTS)
        .filter(|contract| {
            !matches!(
                contract.status,
                ContractStatus::Paid | ContractStatus::Withdrawn
            )
        })
        .collect::<Vec<_>>();
    contracts.sort_by(|left, right| left.id.cmp(&right.id));
    for contract in contracts {
        let scenario_slug = subject_scenarios
            .get(&("case".into(), contract.case_id.clone()))
            .cloned()
            .unwrap_or_default();
        rows.push(BackendDevelopmentQuest {
            scenario_slug,
            quest_kind: if ctx
                .db
                .errantry_authority()
                .case_id()
                .find(&contract.case_id)
                .is_some()
            {
                "errantry contract".into()
            } else {
                "contract".into()
            },
            subject_id: contract.id,
            canonical_case_id: contract.case_id,
            title: contract.title,
            status: format!("{:?}", contract.status).to_ascii_lowercase(),
            incident_count: 0,
            public_awareness_bps: 0,
            supports_incident_action: false,
            player_safe_summary: contract.description,
        });
    }

    for ((kind, occurrence_id), scenario_slug) in &subject_scenarios {
        if kind != "road_encounter" {
            continue;
        }
        let Some(challenge) = ctx.db.road_challenge_authority().id().find(occurrence_id) else {
            continue;
        };
        rows.push(BackendDevelopmentQuest {
            scenario_slug: scenario_slug.clone(),
            quest_kind: "road encounter".into(),
            subject_id: challenge.id,
            canonical_case_id: challenge.case_id,
            title: challenge.catalog_id,
            status: if challenge.open { "open".into() } else { "resolved".into() },
            incident_count: 0,
            public_awareness_bps: 0,
            supports_incident_action: false,
            player_safe_summary: "Catalog-authored encounter bound to this scenario's ordinary journey camp.".into(),
        });
    }
    rows.sort_by(|left, right| {
        left.scenario_slug
            .cmp(&right.scenario_slug)
            .then_with(|| left.quest_kind.cmp(&right.quest_kind))
            .then_with(|| left.subject_id.cmp(&right.subject_id))
    });
    rows.truncate(MAX_OUTPUTS);
    rows
}

#[reducer]
pub fn trigger_development_scenario_incident(
    ctx: &ReducerContext,
    scenario_slug: String,
    problem_id: String,
    request_id: String,
) -> Result<(), String> {
    require_development_gateway(ctx)?;
    if request_id.is_empty() || request_id.len() > 160 {
        return Err("Development scenario request ID is invalid".into());
    }
    if let Some(receipt) = ctx
        .db
        .development_scenario_update_receipt()
        .request_id()
        .find(&request_id)
    {
        return if receipt.scenario_slug == scenario_slug && receipt.problem_id == problem_id {
            Ok(())
        } else {
            Err("Conflicting development scenario update retry".into())
        };
    }
    let registered = ctx
        .db
        .development_scenario_subject()
        .scenario_slug()
        .filter(&scenario_slug)
        .any(|subject| subject.subject_kind == "generated_problem" && subject.subject_id == problem_id);
    if !registered {
        return Err("Generated problem is not a subject of this development scenario".into());
    }
    let occurred_at = crate::local_problem::official_minute(ctx);
    let resulting_incident_ordinal =
        crate::local_problem::trigger_next_generated_incident(ctx, &problem_id, occurred_at)?;
    ctx.db
        .development_scenario_update_receipt()
        .insert(DevelopmentScenarioUpdateReceipt {
            request_id,
            scenario_slug,
            problem_id,
            resulting_incident_ordinal,
            occurred_at_minute: occurred_at,
        });
    Ok(())
}

#[cfg(test)]
mod development_scenario_source_tests {
    #[test]
    fn scenario_projection_is_bounded_gateway_only_and_metadata_only() {
        let source = include_str!("development_scenarios.rs");
        assert!(source.contains("!strategic_view_is_gateway(ctx)"));
        assert!(source.contains("MAX_SUBJECT_INPUTS"));
        assert!(source.contains("MAX_KIND_INPUTS"));
        assert!(source.contains("rows.truncate(MAX_OUTPUTS)"));
        assert!(source.contains("subject_scenarios"));
        assert!(source.contains("\"errantry contract\""));
        assert!(source.contains("\"road encounter\""));
        assert!(!source.contains("authority_json"));
        assert!(!source.contains("manifest_json"));
    }

    #[test]
    fn mutable_scenarios_bind_exact_stable_subjects() {
        let source = include_str!("development_scenarios.rs");
        assert!(source.contains("dev-scenario-outbreak"));
        assert!(source.contains("dev-scenario-recurring-threat"));
        assert!(source.contains("let outbreak_problem_id = seed_outbreak_demo"));
        assert!(source.contains("let threat_problem_id = materialize_preferred_generated_fixture"));
        assert_eq!(source.matches("discover_development_problem(").count(), 2);
        assert!(source.contains("&outbreak_problem_id,\n        \"quest-outbreak\""));
        assert!(source.contains("&threat_problem_id,\n        \"quest-recurring-threat\""));
        assert!(source.contains("\"Discovered outbreak\""));
        assert!(source.contains("\"Combat or negotiated withdrawal\""));
        assert!(source.contains("ensure_recurring_threat_provisions(ctx, THREAT_ID)"));
        assert!(source.contains("STANDARD_TRAVEL_RATION_ID"));
        assert!(source.contains("STANDARD_WATERSKIN_ID"));
        assert!(source.contains("FIELD_TENT_ID"));
        assert!(source.contains("add_to_party_inventory_checked"));
        assert!(source.contains("let occurrence_id = materialize_development_road_encounter"));
        assert!(!source.contains(".find(|problem| problem.recurring_hostile"));
    }
}
