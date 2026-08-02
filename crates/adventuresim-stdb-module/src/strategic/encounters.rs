fn encounter_terrain_at(route: Option<&PartyJourneyRoute>, minute: u64) -> JourneyTerrainKind {
    route
        .and_then(|route| {
            route.spans.iter().find(|span| {
                minute >= span.start_minute
                    && minute < span.start_minute.saturating_add(span.duration_minutes)
            })
        })
        .map_or(JourneyTerrainKind::Open, |span| span.kind)
}

fn core_encounter_terrain(
    kind: JourneyTerrainKind,
) -> adventuresim_core::encounter::EncounterTerrain {
    use adventuresim_core::encounter::EncounterTerrain;
    match kind {
        JourneyTerrainKind::Road => EncounterTerrain::Road,
        JourneyTerrainKind::Open => EncounterTerrain::Open,
        JourneyTerrainKind::SparseWoods => EncounterTerrain::SparseWoods,
        JourneyTerrainKind::DeepWoods => EncounterTerrain::DeepWoods,
        // Encounter placement still has a four-surface tactical vocabulary.
        JourneyTerrainKind::Wetland => EncounterTerrain::Open,
    }
}

fn journey_fallback_position(
    ctx: &ReducerContext,
    journey: &PartyJourney,
    minute: u64,
) -> (f64, f64) {
    let endpoint = |endpoint: &JourneyEndpoint| -> Option<(f64, f64)> {
        match endpoint {
            JourneyEndpoint::Settlement(endpoint) => ctx
                .db
                .settlement()
                .id()
                .find(&endpoint.id)
                .map(|v| (v.coord_x, v.coord_y)),
            JourneyEndpoint::CaseSite(endpoint) => ctx
                .db
                .case_site_authority()
                .id_key()
                .find(&endpoint.id.value)
                .map(|v| {
                    (
                        f64::from(v.longitude_e7) / 10_000_000.0,
                        f64::from(v.latitude_e7) / 10_000_000.0,
                    )
                }),
            JourneyEndpoint::Camp(_) => None,
        }
    };
    let start = endpoint(&journey.origin).unwrap_or((0.0, 0.0));
    let end = endpoint(&journey.destination).unwrap_or(start);
    let progress = minute.min(journey.total_minutes) as f64 / journey.total_minutes.max(1) as f64;
    (
        start.0 + (end.0 - start.0) * progress,
        start.1 + (end.1 - start.1) * progress,
    )
}

fn party_encumbrance_remaining_basis_points(
    ctx: &ReducerContext,
    party_id: &str,
    member_ids: &[u64],
) -> u32 {
    let personal_burden: f32 = member_ids
        .iter()
        .flat_map(|member_id| ctx.db.inventory_item().character_id().filter(*member_id))
        .map(|row| {
            if let Some(lot) = crate::food::personal_lot(ctx, row.id) {
                return lot.mass_kg.max(0.0);
            }
            ctx.db.item().id().find(&row.item_id).map_or(0.0, |item| {
                let quantity = crate::inventory_amount::personal_amount(ctx, row.id)
                    .map_or(row.quantity as f32, |amount| {
                        amount as f32 / crate::inventory_amount::FULL_AMOUNT_MILLIUNITS as f32
                    });
                item.weight * quantity
            })
        })
        .sum();
    let party_burden: f32 = ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(party_id)
        .map(|row| {
            if let Some(lot) = crate::food::party_lot(ctx, row.id) {
                return lot.mass_kg.max(0.0);
            }
            ctx.db.item().id().find(&row.item_id).map_or(0.0, |item| {
                let quantity = crate::inventory_amount::party_amount(ctx, row.id)
                    .map_or(row.quantity as f32, |amount| {
                        amount as f32 / crate::inventory_amount::FULL_AMOUNT_MILLIUNITS as f32
                    });
                item.weight * quantity
            })
        })
        .sum();
    let capacity: f32 = member_ids
        .iter()
        .map(|member_id| {
            let Some(attributes) = ctx
                .db
                .character_attributes()
                .character_id()
                .find(*member_id)
            else {
                return 0.0;
            };
            let Some(limbs) = ctx.db.character_limbs().character_id().find(*member_id) else {
                return 0.0;
            };
            let adjusted_leg_strength = (attributes.left_leg_strength * limbs.left_leg_health
                + attributes.right_leg_strength * limbs.right_leg_health)
                * 0.5;
            let condition_multiplier = ctx
                .db
                .character_strategic_condition()
                .character_id()
                .find(*member_id)
                .map_or(0.0, |condition| {
                    carrying_capacity_multiplier_for_condition(&condition.status)
                });
            adventuresim_core::equipment::encumbrance_capacity_kg(adjusted_leg_strength)
                * condition_multiplier
        })
        .sum();
    let body_burden: f32 = member_ids
        .iter()
        .map(|member_id| {
            ctx.db
                .character_condition()
                .character_id()
                .find(*member_id)
                .map_or(70.0, |condition| {
                    sanitized_encounter_body_weight(condition.body_weight_kg)
                })
        })
        .sum();
    let remaining = adventuresim_core::equipment::encumbrance_remaining_multiplier(
        body_burden + personal_burden + party_burden,
        capacity,
    );
    (remaining.clamp(0.0, 1.0) * 10_000.0).round() as u32
}

fn carrying_capacity_multiplier_for_condition(status: &str) -> f32 {
    match status {
        "ready" => 1.0,
        "staggered" => 0.5,
        _ => 0.0,
    }
}

fn sanitized_encounter_body_weight(weight_kg: f32) -> f32 {
    if weight_kg.is_finite() && (20.0..=300.0).contains(&weight_kg) {
        weight_kg
    } else {
        70.0
    }
}

fn current_party_fatigue_percent(ctx: &ReducerContext, member_ids: &[u64]) -> u8 {
    member_ids
        .iter()
        .filter_map(|member_id| {
            let attributes = ctx
                .db
                .character_attributes()
                .character_id()
                .find(*member_id)?;
            let limbs = ctx.db.character_limbs().character_id().find(*member_id)?;
            let stats = ctx.db.character_stats().character_id().find(*member_id)?;
            let capacity = attributes
                .attr_by_parts(SimpleAttribute::Endurance, &limbs)
                .max(0.01)
                * 1_000.0;
            Some(((stats.calories_used.max(0.0) / capacity) * 100.0).round() as u16)
        })
        .max()
        .unwrap_or(0)
        .min(100) as u8
}

pub(crate) fn opaque_strategic_encounter_id(seed: u64, roll_index: u64) -> String {
    adventuresim_core::encounter::opaque_strategic_encounter_id(seed, roll_index)
}

pub(crate) fn advance_party_journey_delay(
    ctx: &ReducerContext,
    party_id: &str,
    minutes: u64,
) -> Result<(), String> {
    for member_id in living_party_member_ids(ctx, party_id) {
        if !advance_travel_time(ctx, member_id, minutes)? {
            return Err(
                "Every living party member must be able to complete the travel delay".into(),
            );
        }
    }
    let mut journey = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party_id.to_string())
        .ok_or("Travel delay requires a durable journey")?;
    journey.completed_elapsed_minutes = journey.completed_elapsed_minutes.saturating_add(minutes);
    ctx.db.party_journey_authority().party_id().update(journey);
    Ok(())
}

pub(crate) fn build_strategic_encounter(
    ctx: &ReducerContext,
    party_id: &str,
    encounter_id: String,
    archetype: adventuresim_core::encounter::EncounterArchetype,
    enemy_count: u16,
    roll_index: u64,
    movement_minute: u64,
    elapsed_minute: u64,
    absolute_minute: u64,
    longitude_e7: i32,
    latitude_e7: i32,
    fatigue_percent: u8,
    terrain_kind: JourneyTerrainKind,
    awareness: adventuresim_core::encounter::Awareness,
    explanation: String,
) -> Result<StrategicEncounter, String> {
    let member_ids = living_party_member_ids(ctx, party_id);
    if member_ids.is_empty() {
        return Err("A party with no living members cannot enter an encounter".into());
    }
    let terrain = core_encounter_terrain(terrain_kind);
    let party_speed = adventuresim_core::encounter::sustainable_speed_m_per_minute(
        fatigue_percent,
        party_encumbrance_remaining_basis_points(ctx, party_id, &member_ids),
        member_ids.len().min(u16::MAX as usize) as u16,
        terrain,
    );
    let enemy_speed = archetype.enemy_speed_m_per_minute();
    let choices =
        adventuresim_core::encounter::available_choices(awareness, archetype, party_speed)
            .into_iter()
            .map(|choice| match choice {
                adventuresim_core::encounter::EncounterChoice::Sneak => "sneak",
                adventuresim_core::encounter::EncounterChoice::Detour => "detour",
                adventuresim_core::encounter::EncounterChoice::Attack => "attack",
                adventuresim_core::encounter::EncounterChoice::Run => "run",
                adventuresim_core::encounter::EncounterChoice::Surrender => "surrender",
            })
            .map(str::to_string)
            .collect::<Vec<_>>();
    let archetype_name = match archetype {
        adventuresim_core::encounter::EncounterArchetype::Bandits => "bandit",
        adventuresim_core::encounter::EncounterArchetype::Goblins => "goblin",
        adventuresim_core::encounter::EncounterArchetype::Undead => "skeleton",
    };
    let mut encounter = StrategicEncounter {
        party_id: party_id.into(),
        encounter_id,
        archetype: archetype_name.into(),
        enemy_count,
        roll_index,
        journey_movement_minute: movement_minute,
        journey_elapsed_minute: elapsed_minute,
        absolute_minute,
        longitude_e7,
        latitude_e7,
        terrain: format!("{terrain_kind:?}").to_ascii_lowercase(),
        party_aware: matches!(
            awareness,
            adventuresim_core::encounter::Awareness::PartyOnly
                | adventuresim_core::encounter::Awareness::Both
        ),
        enemy_aware: matches!(
            awareness,
            adventuresim_core::encounter::Awareness::EnemyOnly
                | adventuresim_core::encounter::Awareness::Both
        ),
        available_choices: choices,
        status: "awaiting_choice".into(),
        revision: 1,
        selected_choice: None,
        selection_explanation: explanation,
        party_speed_m_per_minute: party_speed,
        enemy_speed_m_per_minute: enemy_speed,
        run_ineligibility: (party_speed <= enemy_speed).then(|| {
            format!(
                "Party speed {party_speed} m/min does not exceed enemy speed {enemy_speed} m/min"
            )
        }),
        penalty_minutes: 0,
        loss_preview: Vec::new(),
        outcome: None,
    };
    if encounter
        .available_choices
        .iter()
        .any(|choice| choice == "surrender")
    {
        encounter.loss_preview = encounter_loss_preview(ctx, party_id);
    }
    Ok(encounter)
}

fn whole_party_sneak_score(ctx: &ReducerContext, member_ids: &[u64]) -> u16 {
    member_ids
        .iter()
        .filter_map(|member_id| {
            let skills = ctx.db.character_skills().character_id().find(*member_id)?;
            let attributes = ctx
                .db
                .character_attributes()
                .character_id()
                .find(*member_id)?;
            let training = adventuresim_core::prelude::Skill::Stealth
                .capped_training_rank(skills.stealth_hours, &attributes);
            Some((training.max(0.0) * 100.0).round() as u16)
        })
        .min()
        .unwrap_or(0)
}

fn quest_influence_case_site_id(destination: &JourneyEndpoint) -> Option<&str> {
    destination.case_site_id()
}

fn destination_hostile_archetype(
    destination_case_site_id: &str,
    groups: impl IntoIterator<Item = (String, String, String)>,
) -> Option<adventuresim_core::encounter::EncounterArchetype> {
    groups
        .into_iter()
        .filter(|(_, case_site_id, _)| case_site_id == destination_case_site_id)
        .min_by(|(left_id, _, _), (right_id, _, _)| left_id.cmp(right_id))
        .and_then(|(_, _, enemy_type)| quest_encounter_archetype(&enemy_type))
}

/// Truncates a walking leg at its first canonical random-encounter boundary.
/// The caller advances ordinary time/needs/fatigue by the returned duration.
fn maybe_interrupt_travel(
    ctx: &ReducerContext,
    party_id: &str,
    requested_minutes: u64,
) -> Result<
    (
        u64,
        Option<StrategicEncounter>,
        Option<adventuresim_core::encounter::NarrativeSelection>,
        u64,
    ),
    String,
> {
    require_no_unresolved_encounter(ctx, party_id)?;
    let Some(journey) = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party_id.to_string())
    else {
        return Ok((requested_minutes, None, None, 1));
    };
    let absolute_start = journey
        .departure_minute
        .saturating_add(journey.completed_elapsed_minutes);
    if let (Some(origin_id), Some(destination_id)) = (
        journey.origin.settlement_id(),
        journey.destination.settlement_id(),
    ) {
        crate::local_problem::ensure_route_problem(ctx, origin_id, destination_id, absolute_start)?;
    }
    let authority = ctx
        .db
        .party_journey_encounter_authority()
        .party_id()
        .find(&party_id.to_string())
        .ok_or("Journey encounter authority is missing")?;
    let route = ctx
        .db
        .party_journey_route_authority()
        .party_id()
        .find(&party_id.to_string());
    let active_contract_archetype =
        quest_influence_case_site_id(&journey.destination).and_then(|destination_case_site_id| {
            let contract = ctx
                .db
                .party_authority()
                .id()
                .find(&party_id.to_string())
                .and_then(|party| party.active_contract_id)
                .and_then(|contract_id| ctx.db.contract_authority().id().find(&contract_id))?;
            let destination_site = ctx
                .db
                .case_site_authority()
                .id_key()
                .find(&destination_case_site_id.to_string())?;
            if destination_site.case_id != contract.case_id {
                return None;
            }
            destination_hostile_archetype(
                destination_case_site_id,
                ctx.db
                    .hostile_group_authority()
                    .iter()
                    .map(|group| (group.id, group.case_site_id.value, group.enemy_type)),
            )
        });
    let member_ids = living_party_member_ids(ctx, party_id);
    let capable = member_ids
        .iter()
        .filter(|id| {
            ctx.db
                .character_capability()
                .character_id()
                .find(**id)
                .is_some_and(|capability| capability.melee || capability.ranged)
        })
        .count()
        .max(1) as u16;
    let completed = journey.completed_minutes;
    let selection = adventuresim_core::encounter::first_encounter_with_problem(
        authority.seed,
        completed,
        requested_minutes,
        |minute| {
            let terrain = core_encounter_terrain(encounter_terrain_at(route.as_ref(), minute));
            let absolute_minute = absolute_start.saturating_add(minute.saturating_sub(completed));
            let night = absolute_minute % 1_440 < 360 || absolute_minute % 1_440 >= 1_200;
            adventuresim_core::encounter::EncounterContext {
                terrain,
                night,
                accepted_active_quest: active_contract_archetype.map(|archetype| {
                    adventuresim_core::encounter::AcceptedQuestInfluence {
                        archetype,
                        distance_minutes: journey.total_minutes.saturating_sub(minute),
                    }
                }),
                combat_capable_members: capable,
                party_awareness: 250,
                enemy_awareness: 250
                    + if night {
                        adventuresim_core::encounter::NIGHT_ENEMY_AWARENESS_BONUS
                    } else {
                        0
                    },
                party_speed_m_per_minute:
                    adventuresim_core::encounter::PARTY_WALKING_SPEED_M_PER_MINUTE,
            }
        },
        |minute| {
            let absolute_minute = absolute_start.saturating_add(minute.saturating_sub(completed));
            match (
                journey.origin.settlement_id(),
                journey.destination.settlement_id(),
            ) {
                (Some(origin_id), Some(destination_id)) => {
                    crate::local_problem::route_encounter_influence(
                        ctx,
                        origin_id,
                        destination_id,
                        absolute_minute,
                    )
                }
                _ => None,
            }
        },
    );
    let narrative = adventuresim_core::encounter::first_narrative_encounter(
        authority.seed,
        completed,
        requested_minutes,
        adventuresim_core::encounter::NarrativeContext {
            kind: adventuresim_core::encounter::NarrativeBoundaryKind::Travel,
            in_settlement: false,
            another_interruption_pending: false,
        },
    );
    let crossed_end = completed.saturating_add(requested_minutes);
    let next_roll = crossed_end / adventuresim_core::encounter::ENCOUNTER_ROLL_INTERVAL_MINUTES + 1;
    if let Some(narrative) = narrative
        && selection
            .as_ref()
            .is_none_or(|combat| narrative.boundary_minute <= combat.boundary_minute)
    {
        let reached_next_roll =
            adventuresim_core::encounter::next_combat_roll_after_reached_boundary(
                narrative.boundary_minute,
            );
        return Ok((
            narrative.boundary_minute.saturating_sub(completed),
            None,
            Some(narrative),
            reached_next_roll,
        ));
    }
    let Some(selection) = selection else {
        return Ok((requested_minutes, None, None, next_roll));
    };

    if selection.awareness == adventuresim_core::encounter::Awareness::Neither {
        return Ok((requested_minutes, None, None, next_roll));
    }
    let position = route
        .as_ref()
        .and_then(|route| route_position_at_minute(route, selection.boundary_minute))
        .unwrap_or_else(|| journey_fallback_position(ctx, &journey, selection.boundary_minute));
    let terrain = encounter_terrain_at(route.as_ref(), selection.boundary_minute);
    let encounter = build_strategic_encounter(
        ctx,
        party_id,
        opaque_strategic_encounter_id(authority.seed, selection.roll_index),
        selection.archetype,
        selection.count,
        selection.roll_index,
        selection.boundary_minute,
        journey
            .completed_elapsed_minutes
            .saturating_add(selection.boundary_minute.saturating_sub(completed)),
        absolute_start.saturating_add(selection.boundary_minute.saturating_sub(completed)),
        (position.0 * 10_000_000.0).round() as i32,
        (position.1 * 10_000_000.0).round() as i32,
        journey.fatigue_percent,
        terrain,
        selection.awareness,
        format!(
            "Canonical journey roll {} in {:?}; party awareness {} vs enemy awareness {}",
            selection.roll_index, terrain, selection.party_roll, selection.enemy_roll
        ),
    )?;
    Ok((
        selection.boundary_minute.saturating_sub(completed),
        Some(encounter),
        None,
        selection.roll_index.saturating_add(1),
    ))
}

fn advance_party_movement_until_encounter(
    ctx: &ReducerContext,
    party_id: &str,
    traveler_ids: &[u64],
    proposed_leg_minutes: u64,
) -> Result<
    (
        u64,
        Option<StrategicEncounter>,
        Option<adventuresim_core::encounter::NarrativeSelection>,
        u64,
    ),
    String,
> {
    let (requested_leg_minutes, mut encounter, mut narrative, mut next_roll) =
        maybe_interrupt_travel(ctx, party_id, proposed_leg_minutes)?;
    let (actual_minutes, _) =
        advance_party_movement(ctx, party_id, traveler_ids, requested_leg_minutes)?;
    if actual_minutes < requested_leg_minutes {
        let (rescanned_minutes, rescanned_encounter, rescanned_narrative, rescanned_next_roll) =
            maybe_interrupt_travel(ctx, party_id, actual_minutes)?;
        debug_assert_eq!(rescanned_minutes, actual_minutes);
        encounter = rescanned_encounter;
        narrative = rescanned_narrative;
        next_roll = rescanned_next_roll;
    }
    Ok((actual_minutes, encounter, narrative, next_roll))
}

/// Commits the scan cursor and, when one was found, materializes the encounter
/// only after every traveler has reached the same canonical boundary.
fn commit_encounter_scan(
    ctx: &ReducerContext,
    party_id: &str,
    next_roll: u64,
    encounter: Option<StrategicEncounter>,
    narrative: Option<adventuresim_core::encounter::NarrativeSelection>,
) -> Result<(), String> {
    let mut authority = ctx
        .db
        .party_journey_encounter_authority()
        .party_id()
        .find(&party_id.to_string())
        .ok_or("Journey encounter authority is missing")?;
    authority.next_roll = next_roll;
    let seed = authority.seed;
    ctx.db
        .party_journey_encounter_authority()
        .party_id()
        .update(authority);

    if let Some(selection) = narrative {
        return materialize_chance_narrative_encounter(
            ctx,
            party_id,
            &selection,
            NarrativeEncounterOrigin::ChanceTravel,
        );
    }

    let Some(mut encounter) = encounter else {
        return Ok(());
    };
    let member_ids = living_party_member_ids(ctx, party_id);
    if member_ids.is_empty() {
        return Err("A party with no living members cannot enter an encounter".into());
    }
    let capable = member_ids
        .iter()
        .filter(|id| {
            ctx.db
                .character_capability()
                .character_id()
                .find(**id)
                .is_some_and(|capability| capability.melee || capability.ranged)
        })
        .count()
        .max(1) as u16;
    let archetype = match encounter.archetype.as_str() {
        "bandit" => adventuresim_core::encounter::EncounterArchetype::Bandits,
        "goblin" => adventuresim_core::encounter::EncounterArchetype::Goblins,
        "skeleton" => adventuresim_core::encounter::EncounterArchetype::Undead,
        _ => return Err("Encounter has an unknown archetype".into()),
    };
    let awareness = match (encounter.party_aware, encounter.enemy_aware) {
        (true, false) => adventuresim_core::encounter::Awareness::PartyOnly,
        (false, true) => adventuresim_core::encounter::Awareness::EnemyOnly,
        (true, true) => adventuresim_core::encounter::Awareness::Both,
        (false, false) => return Err("Encounter has no aware participants".into()),
    };
    let terrain = core_encounter_terrain(match encounter.terrain.as_str() {
        "road" => JourneyTerrainKind::Road,
        "open" => JourneyTerrainKind::Open,
        "sparsewoods" | "sparse_woods" => JourneyTerrainKind::SparseWoods,
        "deepwoods" | "deep_woods" => JourneyTerrainKind::DeepWoods,
        "wetland" => JourneyTerrainKind::Wetland,
        _ => JourneyTerrainKind::Open,
    });
    encounter.enemy_count = adventuresim_core::encounter::scale_enemy_count(
        adventuresim_core::encounter::enemy_count(seed, encounter.roll_index, capable),
        archetype,
    );
    encounter.party_speed_m_per_minute =
        adventuresim_core::encounter::sustainable_speed_m_per_minute(
            current_party_fatigue_percent(ctx, &member_ids),
            party_encumbrance_remaining_basis_points(ctx, party_id, &member_ids),
            member_ids.len().min(u16::MAX as usize) as u16,
            terrain,
        );
    let run_eligible = adventuresim_core::encounter::run_is_eligible(
        encounter.party_speed_m_per_minute,
        archetype,
    );
    encounter.available_choices = adventuresim_core::encounter::available_choices(
        awareness,
        archetype,
        encounter.party_speed_m_per_minute,
    )
    .into_iter()
    .map(|choice| match choice {
        adventuresim_core::encounter::EncounterChoice::Sneak => "sneak",
        adventuresim_core::encounter::EncounterChoice::Detour => "detour",
        adventuresim_core::encounter::EncounterChoice::Attack => "attack",
        adventuresim_core::encounter::EncounterChoice::Run => "run",
        adventuresim_core::encounter::EncounterChoice::Surrender => "surrender",
    })
    .map(str::to_string)
    .collect();
    encounter.run_ineligibility = (!run_eligible).then(|| {
        format!(
            "Party speed {} m/min does not exceed enemy speed {} m/min",
            encounter.party_speed_m_per_minute, encounter.enemy_speed_m_per_minute
        )
    });
    encounter.loss_preview = if encounter
        .available_choices
        .iter()
        .any(|choice| choice == "surrender")
    {
        encounter_loss_preview(ctx, party_id)
    } else {
        Vec::new()
    };
    let previous = ctx
        .db
        .strategic_encounter()
        .party_id()
        .find(&party_id.to_string());
    if let Some(previous) = previous.as_ref()
        && previous.encounter_id != encounter.encounter_id
    {
        crate::world_actor::deactivate_context_roster(ctx, &previous.encounter_id);
    }
    let roster = crate::world_actor::materialize_context_roster(
        ctx,
        crate::world_actor::CharacterContextKind::StrategicEncounter,
        &encounter.encounter_id,
        &encounter.encounter_id,
        &encounter.archetype,
        u32::from(encounter.enemy_count),
    )?;
    if roster.len() != usize::from(encounter.enemy_count) {
        return Err("Committed encounter roster does not match its enemy count".into());
    }
    if previous.is_some() {
        ctx.db.strategic_encounter().party_id().update(encounter);
    } else {
        ctx.db.strategic_encounter().insert(encounter);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParsedEncounterChoice {
    Sneak,
    Detour,
    Attack,
    Run,
    Surrender,
}

impl ParsedEncounterChoice {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "sneak" => Ok(Self::Sneak),
            "detour" => Ok(Self::Detour),
            "attack" => Ok(Self::Attack),
            "run" => Ok(Self::Run),
            "surrender" => Ok(Self::Surrender),
            _ => Err("Unknown encounter choice".into()),
        }
    }
    const fn label(self) -> &'static str {
        match self {
            Self::Sneak => "sneak",
            Self::Detour => "detour",
            Self::Attack => "attack",
            Self::Run => "run",
            Self::Surrender => "surrender",
        }
    }
}

fn encounter_loss_preview(ctx: &ReducerContext, party_id: &str) -> Vec<StrategicEncounterLoss> {
    let minimum = adventuresim_core::encounter::SURRENDER_MINIMUM_ITEM_VALUE;
    let mut losses = Vec::new();
    for row in ctx.db.party_inventory_item().party_id().filter(party_id) {
        let currency = crate::item::is_currency(ctx, &row.item_id);
        let value = ctx
            .db
            .item()
            .id()
            .find(&row.item_id)
            .and_then(|item| item.base_value)
            .unwrap_or(0);
        if currency || value >= minimum {
            losses.push(StrategicEncounterLoss {
                owner_kind: "party".into(),
                owner_id: 0,
                inventory_id: row.id,
                item_id: row.item_id,
                quantity: row.quantity,
                value_each: value,
            });
        }
    }
    let mut member_ids: Vec<_> = ctx
        .db
        .party_member()
        .party_id()
        .filter(party_id)
        .map(|membership| membership.character_id)
        .collect();
    member_ids.sort_unstable();
    member_ids.dedup();
    for member_id in member_ids {
        for row in ctx.db.inventory_item().character_id().filter(member_id) {
            let currency = crate::item::is_currency(ctx, &row.item_id);
            let value = ctx
                .db
                .item()
                .id()
                .find(&row.item_id)
                .and_then(|item| item.base_value)
                .unwrap_or(0);
            if currency || value >= minimum {
                losses.push(StrategicEncounterLoss {
                    owner_kind: "member".into(),
                    owner_id: member_id,
                    inventory_id: row.id,
                    item_id: row.item_id,
                    quantity: row.quantity,
                    value_each: value,
                });
            }
        }
    }
    losses.sort_by(|a, b| {
        (&a.owner_kind, a.owner_id, a.inventory_id).cmp(&(
            &b.owner_kind,
            b.owner_id,
            b.inventory_id,
        ))
    });
    losses
}

fn commit_encounter_surrender(
    ctx: &ReducerContext,
    party_id: &str,
    encounter_id: &str,
    current: &[StrategicEncounterLoss],
) -> Result<(), String> {
    for loss in current {
        let property_id = format!(
            "property:surrender:{encounter_id}:{}:{}",
            loss.owner_id, loss.inventory_id
        );
        if ctx.db.legal_property().id().find(&property_id).is_none() {
            let escrow_id = format!(
                "surrender:{encounter_id}:{}:{}",
                loss.owner_id, loss.inventory_id
            );
            let owner_id = format!("counterparty:{encounter_id}");
            ctx.db.systemic_escrow_lot().insert(SystemicEscrowLot {
                id: escrow_id.clone(),
                holder_id: owner_id.clone(),
                context_id: encounter_id.into(),
                item_id: loss.item_id.clone(),
                quantity: u64::from(loss.quantity),
            });
            ctx.db.legal_property().insert(LegalProperty {
                id: property_id,
                scope_owner_key: format!("faction:{owner_id}"),
                kind: if crate::item::is_currency(ctx, &loss.item_id) {
                    PropertyKind::Currency
                } else {
                    PropertyKind::Item
                },
                item_id: loss.item_id.clone(),
                quantity: u64::from(loss.quantity),
                owner_kind: LegalOwnerKind::Faction,
                owner_id: owner_id.clone(),
                physical_holder_id: owner_id,
                physical_binding_id: format!("escrow:{escrow_id}"),
                version: 0,
                provenance: format!("surrender:{party_id}:{}", loss.inventory_id),
                metadata: format!("value_each={}", loss.value_each),
                case_id: None,
            });
        }
        if loss.owner_kind == "party" {
            ctx.db
                .party_item_condition()
                .party_inventory_item_id()
                .delete(loss.inventory_id);
            ctx.db.party_inventory_item().id().delete(loss.inventory_id);
        } else {
            crate::character::unequip_wearable(ctx, loss.inventory_id);
            ctx.db
                .item_condition()
                .inventory_item_id()
                .delete(loss.inventory_id);
            ctx.db.inventory_item().id().delete(loss.inventory_id);
        }
    }
    reconcile_party_pool_ledger(ctx, party_id)?;
    for member_id in living_party_member_ids(ctx, party_id) {
        crate::capability::refresh_character_capability(ctx, member_id)?;
        crate::condition::refresh_character_strategic_condition(ctx, member_id)?;
    }
    Ok(())
}

fn reconcile_party_pool_ledger(ctx: &ReducerContext, party_id: &str) -> Result<(), String> {
    let remaining_value = ctx
        .db
        .party_inventory_item()
        .party_id()
        .filter(party_id)
        .try_fold(0_u64, |total, row| {
            Ok::<_, String>(total.saturating_add(
                objective_item_value(ctx, &row.item_id)?.saturating_mul(u64::from(row.quantity)),
            ))
        })?;
    let mut stakes: Vec<_> = ctx.db.party_stake().party_id().filter(party_id).collect();
    stakes.sort_by_key(|stake| stake.id);
    let prior_reserve = ctx
        .db
        .party_inventory_state()
        .party_id()
        .find(&party_id.to_string())
        .map_or(0, |state| state.reserve_value);
    let total_claims = stakes.iter().fold(prior_reserve, |total, stake| {
        total.saturating_add(stake.value)
    });
    let mut allocated = 0_u64;
    for mut stake in stakes {
        stake.value = if total_claims == 0 {
            0
        } else {
            ((u128::from(stake.value) * u128::from(remaining_value)) / u128::from(total_claims))
                as u64
        };
        allocated = allocated.saturating_add(stake.value);
        ctx.db.party_stake().id().update(stake);
    }
    let reserve_value = remaining_value.saturating_sub(allocated);
    if let Some(mut state) = ctx
        .db
        .party_inventory_state()
        .party_id()
        .find(&party_id.to_string())
    {
        state.reserve_value = reserve_value;
        ctx.db.party_inventory_state().party_id().update(state);
    } else {
        ctx.db.party_inventory_state().insert(PartyInventoryState {
            party_id: party_id.to_string(),
            reserve_value,
        });
    }
    Ok(())
}

fn encounter_core_terrain(value: &str) -> adventuresim_core::encounter::EncounterTerrain {
    use adventuresim_core::encounter::EncounterTerrain;
    match value {
        "road" => EncounterTerrain::Road,
        "sparsewoods" => EncounterTerrain::SparseWoods,
        "deepwoods" => EncounterTerrain::DeepWoods,
        _ => EncounterTerrain::Open,
    }
}

fn advance_encounter_penalty(
    ctx: &ReducerContext,
    encounter: &mut StrategicEncounter,
    choice: ParsedEncounterChoice,
) -> Result<(), String> {
    use adventuresim_core::encounter::EncounterChoice;
    let core_choice = match choice {
        ParsedEncounterChoice::Detour => EncounterChoice::Detour,
        ParsedEncounterChoice::Run => EncounterChoice::Run,
        _ => return Ok(()),
    };
    let minutes = adventuresim_core::encounter::penalty_minutes(
        encounter_core_terrain(&encounter.terrain),
        core_choice,
    );
    advance_party_journey_delay(ctx, &encounter.party_id, minutes)?;
    encounter.penalty_minutes = minutes;
    Ok(())
}

/// Commit every persistent consequence shared by quest and random autoresolve.
/// The battle itself remains transient; only its bounded summary and strategic
/// condition effects cross into SpacetimeDB.
fn commit_autoresolve_outcome(
    ctx: &ReducerContext,
    source_id: &str,
    party_id: &str,
    member_ids: &[u64],
    defeat_morale_penalty: f32,
    outcome: &adventuresim_core::autoresolve::BattleOutcome,
) -> Result<(), String> {
    record_autoresolve_report(ctx, source_id, party_id, outcome);
    for member_id in member_ids {
        crate::filth::deposit_now(
            ctx,
            *member_id,
            crate::filth::FilthSubstance::Dirt,
            None,
            adventuresim_core::filth::COMBAT_DIRT,
        )?;
    }
    for exchange in &outcome.log {
        if exchange.cut_damage > 0.0 && member_ids.contains(&exchange.attacker_id) {
            crate::filth::deposit_now(
                ctx,
                exchange.attacker_id,
                crate::filth::FilthSubstance::Blood,
                member_ids
                    .contains(&exchange.defender_id)
                    .then_some(exchange.defender_id),
                (exchange.cut_damage * 35.0).ceil().clamp(1.0, 15.0) as u16,
            )?;
        }
        if let Some(id) = exchange.weapon_inventory_item_id {
            crate::repair::apply_impact(ctx, id, exchange.contact_stress);
        }
        if let Some(id) = exchange.defender_contact_item_id {
            crate::repair::apply_impact(ctx, id, exchange.contact_stress);
        }
        if exchange.armor_contact && exchange.contact_stress > 0.0 {
            if let Some(id) = crate::character::outermost_wearable_for_body_part(
                ctx,
                exchange.defender_id,
                exchange.body_part,
            ) {
                crate::repair::apply_impact(ctx, id, exchange.contact_stress);
            }
        }
    }
    for member in &outcome.allies {
        consume_autoresolve_ammunition(ctx, member.id, member.ammunition_used);
        for exchange in outcome
            .log
            .iter()
            .filter(|exchange| exchange.defender_id == member.id && exchange.health_damage > 0.0)
        {
            let limb = match exchange.body_part {
                BodyPart::LeftArm => crate::surgery::LimbRegion::LeftArm,
                BodyPart::RightArm => crate::surgery::LimbRegion::RightArm,
                BodyPart::LeftLeg => crate::surgery::LimbRegion::LeftLeg,
                BodyPart::RightLeg => crate::surgery::LimbRegion::RightLeg,
                BodyPart::Chest => crate::surgery::LimbRegion::Chest,
                BodyPart::Stomach => crate::surgery::LimbRegion::Stomach,
                BodyPart::Head => crate::surgery::LimbRegion::Head,
            };
            let projectile = exchange.projectile_kind.map(|kind| match kind {
                adventuresim_core::autoresolve::CombatProjectileKind::Arrowhead => {
                    crate::surgery::ProjectileKind::Arrowhead
                }
                adventuresim_core::autoresolve::CombatProjectileKind::Ball => {
                    crate::surgery::ProjectileKind::Ball
                }
            });
            crate::surgery::commit_hit_injury(
                ctx,
                member.id,
                limb,
                exchange.cut_damage,
                exchange.blunt_damage,
                projectile,
            )?;
        }
        crate::condition::apply_blood_loss(ctx, member.id, member.blood_loss_fraction)?;
        crate::capability::refresh_character_capability(ctx, member.id)?;
    }
    if outcome.victor != BattleVictor::Allies {
        for member_id in member_ids {
            crate::condition::record_morale_event(
                ctx,
                *member_id,
                "defeat",
                -defeat_morale_penalty,
                Some(source_id.to_string()),
            )?;
        }
    }
    Ok(())
}

fn resolve_random_encounter_battle(
    ctx: &ReducerContext,
    encounter: &StrategicEncounter,
    seed: u64,
    opening: BattleOpening,
) -> Result<String, String> {
    let member_ids = living_party_member_ids(ctx, &encounter.party_id);
    let allies = member_ids
        .iter()
        .map(|id| {
            let condition = crate::condition::refresh_character_strategic_condition(ctx, *id)?;
            crate::capability::load_combatant(
                ctx,
                *id,
                condition.incapacitation,
                condition.pain,
                condition.blood_loss,
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    let difficulty = i32::from(encounter.enemy_count.max(1));
    let enemy_ids = crate::world_actor::context_character_ids(ctx, &encounter.encounter_id);
    if enemy_ids.len() != encounter.enemy_count as usize {
        return Err("Encounter counterparty roster does not match encounter authority".into());
    }
    let enemies = enemy_ids
        .into_iter()
        .map(|enemy_id| autoresolve_enemy(enemy_id, &encounter.archetype, difficulty, 10_000))
        .collect::<Result<Vec<_>, String>>()?;
    let outcome = resolve_battle(allies, enemies, seed ^ encounter.roll_index, opening);
    commit_autoresolve_outcome(
        ctx,
        &encounter.encounter_id,
        &encounter.party_id,
        &member_ids,
        5.0 + f32::from(encounter.enemy_count),
        &outcome,
    )?;
    crate::corpse::persist_autoresolve_enemy_corpses(
        ctx,
        &encounter.encounter_id,
        &encounter.party_id,
        "",
        "",
        &encounter.archetype,
        &outcome,
    )?;
    if outcome.victor == BattleVictor::Allies {
        let authored_followup = ctx
            .db
            .narrative_combat_followup_authority()
            .encounter_id()
            .find(&encounter.encounter_id)
            .is_some();
        if !authored_followup && let Some(item_id) = autoresolve_drop(&encounter.archetype)? {
            add_to_party_inventory(
                ctx,
                &encounter.party_id,
                item_id,
                u32::from(encounter.enemy_count),
            );
        }
        for member_id in &member_ids {
            crate::condition::record_morale_event(
                ctx,
                *member_id,
                "victory",
                5.0 + f32::from(encounter.enemy_count),
                Some(encounter.encounter_id.clone()),
            )?;
        }
    }
    Ok(match outcome.victor {
        BattleVictor::Allies => "victory",
        BattleVictor::Enemies => "defeat",
        BattleVictor::Stalemate => "stalemate",
    }
    .into())
}

#[reducer]
pub fn resolve_strategic_encounter(
    ctx: &ReducerContext,
    character_id: u64,
    encounter_id: String,
    choice: String,
    expected_revision: u32,
    action_id: String,
) -> Result<(), String> {
    require_strategic_character_authority(ctx, character_id)?;
    if action_id.is_empty() || action_id.len() > 160 {
        return Err("Strategic encounter action ID is invalid".into());
    }
    let receipt_id = format!("strategic-encounter-action:{character_id}:{action_id}");
    if let Some(receipt) = ctx
        .db
        .strategic_encounter_resolution_receipt()
        .id()
        .find(&receipt_id)
    {
        return if adventuresim_core::encounter::strategic_encounter_retry_matches(
            &receipt.encounter_id,
            receipt.character_id,
            &receipt.choice,
            receipt.expected_revision,
            &encounter_id,
            character_id,
            &choice,
            expected_revision,
        ) {
            Ok(())
        } else {
            Err("Conflicting strategic encounter retry".into())
        };
    }
    crate::character::require_living_character(ctx, character_id)?;
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let party_id = character.party_id.ok_or("Character is not in a party")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id)
        .ok_or("Party not found")?;
    let delegated_recovery = ready_companion_may_continue_evacuation(ctx, &party, character_id);
    if party.leader_id != character_id && !delegated_recovery {
        return Err(
            "Only the party leader, or a ready companion protecting an unready leader, can resolve an encounter"
                .into(),
        );
    }
    let parsed = ParsedEncounterChoice::parse(&choice)?;
    if delegated_recovery && parsed == ParsedEncounterChoice::Attack {
        return Err(
            "A delegated evacuation actor may choose only a protective encounter response".into(),
        );
    }
    let mut encounter = unresolved_encounter(ctx, &party_id).ok_or("No unresolved encounter")?;
    if encounter.encounter_id != encounter_id {
        return Err("Strategic encounter identity is stale".into());
    }
    if encounter.revision != expected_revision {
        return Err("Strategic encounter revision is stale".into());
    }
    let seed = ctx
        .db
        .party_journey_encounter_authority()
        .party_id()
        .find(&party_id)
        .ok_or("Journey encounter authority is missing")?
        .seed;
    if !encounter
        .available_choices
        .iter()
        .any(|available| available == parsed.label())
    {
        return Err("That choice is not available for this encounter".into());
    }
    encounter.selected_choice = Some(parsed.label().into());
    match parsed {
        ParsedEncounterChoice::Sneak => {
            let enemy_stealth =
                u16::from(parse_threat(&encounter.archetype)?.profile().combat.stealth);
            if adventuresim_core::encounter::sneak_succeeds(
                seed,
                encounter.roll_index,
                whole_party_sneak_score(ctx, &living_party_member_ids(ctx, &party_id)),
                200_u16.saturating_add(enemy_stealth),
            ) {
                encounter.outcome = Some("avoided".into());
            } else {
                encounter.outcome = Some(resolve_random_encounter_battle(
                    ctx,
                    &encounter,
                    seed,
                    BattleOpening::Normal,
                )?);
            }
        }
        ParsedEncounterChoice::Detour | ParsedEncounterChoice::Run => {
            if parsed == ParsedEncounterChoice::Run
                && encounter.party_speed_m_per_minute <= encounter.enemy_speed_m_per_minute
            {
                return Err("The party is not fast enough to run".into());
            }
            advance_encounter_penalty(ctx, &mut encounter, parsed)?;
            encounter.outcome = Some("avoided".into());
        }
        ParsedEncounterChoice::Attack => {
            let opening = match (encounter.party_aware, encounter.enemy_aware) {
                (true, false) => BattleOpening::AlliesSurprise,
                (false, true) => BattleOpening::EnemiesSurprise,
                _ => BattleOpening::Normal,
            };
            encounter.outcome = Some(resolve_random_encounter_battle(
                ctx, &encounter, seed, opening,
            )?);
        }
        ParsedEncounterChoice::Surrender => {
            let current = encounter_loss_preview(ctx, &party_id);
            if current != encounter.loss_preview {
                encounter.selected_choice = None;
                encounter.loss_preview = current;
                encounter.revision = encounter.revision.saturating_add(1);
                ctx.db.strategic_encounter().party_id().update(encounter);
                ctx.db.strategic_encounter_resolution_receipt().insert(
                    StrategicEncounterResolutionReceipt {
                        id: receipt_id,
                        encounter_id,
                        party_id,
                        character_id,
                        action_id,
                        choice,
                        expected_revision,
                        resulting_revision: expected_revision.saturating_add(1),
                        outcome: "preview_refreshed".into(),
                    },
                );
                return Ok(());
            }
            commit_encounter_surrender(ctx, &party_id, &encounter.encounter_id, &current)?;
            encounter.outcome = Some("surrendered".into());
        }
    }
    encounter.status = "resolved".into();
    encounter.revision = encounter.revision.saturating_add(1);
    resolve_narrative_combat_followup(ctx, &encounter)?;
    ctx.db
        .strategic_encounter()
        .party_id()
        .update(encounter.clone());
    crate::world_actor::deactivate_context_roster(ctx, &encounter.encounter_id);
    ctx.db
        .strategic_encounter_resolution_receipt()
        .insert(StrategicEncounterResolutionReceipt {
            id: receipt_id,
            encounter_id,
            party_id,
            character_id,
            action_id,
            choice,
            expected_revision,
            resulting_revision: encounter.revision,
            outcome: encounter.outcome.unwrap_or_else(|| "resolved".into()),
        });
    Ok(())
}

fn redirect_camped_party_to_settlement(
    ctx: &ReducerContext,
    party: &mut Party,
    destination: &Settlement,
    route: Option<JourneyRoutePlan>,
) -> Result<(), String> {
    let mut journey = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(&party.id)
        .ok_or("Camp journey not found")?;
    let redirect_departure_minute = living_party_member_ids(ctx, &party.id)
        .into_iter()
        .filter_map(|member_id| ctx.db.character_time().character_id().find(member_id))
        .map(|time| time.minutes)
        .max()
        .unwrap_or(journey.departure_minute);
    let travel_minutes = if let Some(route) = route.as_ref() {
        validate_camp_redirect_weather_interval(route, redirect_departure_minute)?;
        let current_route = ctx
            .db
            .party_journey_route_authority()
            .party_id()
            .find(&party.id)
            .ok_or("Camp has no persisted terrain route")?;
        let origin = route_position_at_minute(&current_route, journey.completed_minutes)
            .ok_or("Camp route position is unavailable")?;
        validate_journey_route(
            ctx,
            route,
            origin,
            (destination.coord_x, destination.coord_y),
        )?;
        route.minutes
    } else {
        camp_redirect_minutes(&journey, &destination.id)
            .ok_or("That settlement is not an endpoint of this camp journey")?
    };
    if travel_minutes == 0 {
        return Err("The party is already at that journey endpoint".into());
    }

    journey.origin = JourneyEndpoint::Camp(party.id.clone());
    journey.destination = JourneyEndpoint::Settlement(JourneySettlementEndpoint {
        id: destination.id.clone(),
        name: destination.name.clone(),
    });
    journey.total_minutes = travel_minutes;
    journey.completed_minutes = 0;
    journey.departure_minute = redirect_departure_minute;
    journey.completed_elapsed_minutes = 0;
    journey.camp_stop_minutes.clear();
    if let Some(mut typed) = ctx.db.party_journey_itinerary().party_id().find(&party.id) {
        typed.actual_camp_intervals.clear();
        typed.forecast_camp_intervals.clear();
        ctx.db.party_journey_itinerary().party_id().update(typed);
    }
    journey.forecast_camp_stop_minutes =
        forecast_camp_stop_minutes(ctx, &party.id, travel_minutes, 0, journey.fatigue_percent)?;
    ctx.db.party_journey_authority().party_id().update(journey);
    if ctx
        .db
        .party_journey_route_authority()
        .party_id()
        .find(&party.id)
        .is_some()
    {
        ctx.db
            .party_journey_route_authority()
            .party_id()
            .delete(&party.id);
    }
    if let Some(route) = route {
        ctx.db
            .party_journey_route_authority()
            .insert(PartyJourneyRoute {
                party_id: party.id.clone(),
                gateway_bucket: 0,
                package_digest: route.package_digest,
                weather_rules_version: route.weather_rules_version,
                weather_interval_start: route.weather_interval_start,
                precipitation: route.precipitation,
                intensity_bps: route.intensity_bps,
                ground_moisture_bps: route.ground_moisture_bps,
                snow_cover_bps: route.snow_cover_bps,
                distance_m: route.distance_m,
                minutes: route.minutes,
                points: route.points,
                spans: route.spans,
                return_route: route.return_route,
            });
    }

    party.current_settlement_id = None;
    party.current_case_site_id = None;
    party.camp_destination = Some(JourneyEndpoint::Settlement(JourneySettlementEndpoint {
        id: destination.id.clone(),
        name: destination.name.clone(),
    }));
    party.camp_remaining_minutes = travel_minutes;
    ctx.db.party_authority().id().update(party.clone());
    refresh_party_journey_forecast(ctx, &party.id)?;
    Ok(())
}

fn revalidate_party_after_departure_sync(
    ctx: &ReducerContext,
    party_id: &str,
    leader_id: u64,
    expected_settlement_id: Option<&str>,
    expected_quest_location_id: Option<&str>,
    expected_active_contract_id: Option<&str>,
    allow_incapacitated_case_site_withdrawal: bool,
) -> Result<Party, String> {
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(&party_id.to_string())
        .ok_or("Party changed during departure synchronization")?;
    let party_matches = party.leader_id == leader_id
        && party.camp_destination.is_none()
        && party.current_settlement_id.as_deref() == expected_settlement_id
        && party.current_case_site_id.as_deref() == expected_quest_location_id
        && !expected_active_contract_id
            .is_some_and(|id| party.active_contract_id.as_deref() != Some(id));
    let pending_incident_sites: Vec<_> = ctx
        .db
        .strategic_incident()
        .party_id()
        .filter(party_id)
        .filter(|incident| incident.status == IncidentStatus::Pending)
        .map(|incident| incident.case_site_id.value)
        .collect();
    if !departure_snapshot_allows_travel(
        party_matches,
        true,
        pending_incident_allows_departure(
            expected_quest_location_id,
            pending_incident_sites.iter().map(String::as_str),
        ),
    ) {
        return Err("Travel was interrupted while the party synchronized its clocks".into());
    }
    let members = living_party_member_ids(ctx, party_id);
    let members_match = !members.is_empty()
        && !members.iter().any(|id| {
            ctx.db.character().id().find(*id).is_none_or(|member| {
                member.current_settlement_id.as_deref() != expected_settlement_id
                    || crate::investigation::character_case_site_id(ctx, member.id).as_deref()
                        != expected_quest_location_id
            })
        });
    if !departure_snapshot_allows_travel(true, members_match, true) {
        return Err("A party member changed location during departure synchronization".into());
    }
    if departure_requires_ready_party(
        expected_settlement_id,
        expected_quest_location_id,
        allow_incapacitated_case_site_withdrawal,
    ) {
        require_party_ready(ctx, party_id)?;
    }
    Ok(party)
}

fn departure_requires_ready_party(
    expected_settlement_id: Option<&str>,
    expected_case_site_id: Option<&str>,
    allow_incapacitated_case_site_withdrawal: bool,
) -> bool {
    !(allow_incapacitated_case_site_withdrawal
        && expected_settlement_id.is_none()
        && expected_case_site_id.is_some())
}

fn departure_snapshot_allows_travel(
    party_matches: bool,
    members_match: bool,
    incident_snapshot_allows_departure: bool,
) -> bool {
    party_matches && members_match && incident_snapshot_allows_departure
}

fn pending_incident_allows_departure<'a>(
    expected_case_site_id: Option<&str>,
    pending_case_site_ids: impl Iterator<Item = &'a str>,
) -> bool {
    let mut pending = pending_case_site_ids;
    match (pending.next(), pending.next()) {
        (None, _) => true,
        (Some(site), None) => expected_case_site_id == Some(site),
        (Some(_), Some(_)) => false,
    }
}

fn reconstruct_legacy_journey_coordinates(
    current_minute: u64,
    completed_movement: u64,
) -> (u64, u64) {
    (
        current_minute.saturating_sub(completed_movement),
        completed_movement,
    )
}
