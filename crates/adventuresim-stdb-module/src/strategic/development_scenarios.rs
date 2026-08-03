//! Private, capability-gated catalog for isolated strategic development scenarios.
//!
//! Scenario metadata is deliberately separate from player and quest models. A
//! normal module build cannot project, adopt, or mutate this authority.

#[derive(Clone, Debug)]
#[table(accessor = development_scenario)]
pub struct DevelopmentScenario {
    #[primary_key]
    pub slug: String,
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
    pub problem_id: String,
    pub canonical_case_id: String,
    pub symptom: String,
    pub incident_count: u16,
    pub public_awareness_bps: u16,
    pub recurring_hostile: bool,
    pub resolved: bool,
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
        revision: 1,
        category: category.into(),
        label: label.into(),
        description: description.into(),
        primary_character_id,
        entry_route: entry_route.into(),
    };
    if let Some(existing) = ctx.db.development_scenario().slug().find(slug) {
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
    if ctx.db.development_scenario().slug().find(scenario_slug).is_none() {
        return Err("Development scenario subject has no registered scenario".into());
    }
    let row = DevelopmentScenarioSubject {
        id: format!("{scenario_slug}:{subject_kind}:{subject_id}"),
        scenario_slug: scenario_slug.into(),
        subject_kind: subject_kind.into(),
        subject_id: subject_id.into(),
    };
    if ctx.db.development_scenario_subject().id().find(&row.id).is_none() {
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
        ("knowledge-herbalism", "Knowledge", "Herbalism methods", "Exercise every bounded herbalism method and public grade.", 9_999_999_999_999_986, "/characters"),
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
    ensure_scenario_character(ctx, OUTBREAK_ID, "Outbreak Investigator")?;
    seed_outbreak_demo(ctx, OUTBREAK_ID)?;
    register_development_scenario(ctx, "quest-outbreak", "Quests", "Undiscovered outbreak", "Follow an outbreak through ordinary rumor and journal discovery.", OUTBREAK_ID, "/quests")?;
    if let Some(problem) = ctx.db.local_problem_authority().iter().find(|problem| {
        problem.scope_key == "settlement:riverdale" && problem.disease_intensity > 0
    }) {
        register_development_subject(ctx, "quest-outbreak", "generated_problem", &problem.id)?;
    }

    const THREAT_ID: u64 = 9_999_999_999_999_958;
    ensure_scenario_character(ctx, THREAT_ID, "Threat Investigator")?;
    register_development_scenario(ctx, "quest-recurring-threat", "Quests", "Recurring hostile threat", "Inspect a generated threat and trigger one isolated follow-up attack.", THREAT_ID, "/developer/scenarios")?;
    let threat_problem = ctx
        .db
        .local_problem_authority()
        .iter()
        .find(|problem| problem.recurring_hostile && problem.resolved_at.is_none())
        .ok_or("Development bootstrap did not create a recurring hostile problem")?;
    register_development_subject(ctx, "quest-recurring-threat", "generated_problem", &threat_problem.id)?;

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
        materialize_development_road_encounter(ctx, character_id, &definition.id)?;
        let label = definition
            .cast
            .first()
            .map_or_else(|| definition.id.replace('-', " ").replace('_', " "), |speaker| format!("Encounter with {}", speaker.name));
        register_development_scenario(ctx, &scenario_slug, "Road encounters", &label, "Play this compiled encounter through its ordinary journey-camp presentation.", character_id, "/camp")?;
        register_development_subject(ctx, &scenario_slug, "road_encounter", &definition.id)?;
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
        .iter()
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
    let mut rows = Vec::new();
    for problem in ctx.db.local_problem_authority().iter() {
        let scenario_slug = ctx
            .db
            .development_scenario_subject()
            .iter()
            .find(|row| row.subject_kind == "generated_problem" && row.subject_id == problem.id)
            .map_or_else(String::new, |row| row.scenario_slug);
        let player_safe_summary = ctx
            .db
            .local_problem_symptom()
            .problem_id()
            .filter(&problem.id)
            .next()
            .map_or_else(|| "Not publicly described".into(), |row| row.public_summary);
        rows.push(BackendDevelopmentQuest {
            scenario_slug,
            problem_id: problem.id,
            canonical_case_id: problem.opaque_case_ref,
            symptom: problem.symptom,
            incident_count: problem.incident_count,
            public_awareness_bps: problem.public_awareness_bps,
            recurring_hostile: problem.recurring_hostile,
            resolved: problem.resolved_at.is_some(),
            player_safe_summary,
        });
    }
    rows.sort_by(|left, right| left.scenario_slug.cmp(&right.scenario_slug));
    rows.truncate(256);
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
        assert!(source.contains("rows.truncate(256)"));
        assert!(!source.contains("authority_json"));
        assert!(!source.contains("manifest_json"));
    }
}
