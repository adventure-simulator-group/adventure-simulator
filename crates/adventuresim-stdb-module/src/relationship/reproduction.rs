// Owns pregnancy establishment, spouse-leisure conception, and birth settlement.
pub fn establish_pregnancy(
    ctx: &ReducerContext,
    mother_id: u64,
    father_id: u64,
    conceived_minute: u64,
    birth_settlement_id: &str,
) -> Result<Pregnancy, String> {
    if let Some(existing) = ctx
        .db
        .active_pregnancy()
        .mother_id()
        .find(mother_id)
        .and_then(|active| ctx.db.pregnancy().id().find(&active.pregnancy_id))
    {
        return Ok(existing);
    }
    let ordinal = ctx.db.pregnancy().mother_id().filter(mother_id).count() as u64;
    let due_minute = conceived_minute.saturating_add(GESTATION_MINUTES);
    if ctx
        .db
        .settlement()
        .id()
        .find(birth_settlement_id.to_owned())
        .is_none()
    {
        return Err("Pregnancy requires a valid conception settlement".into());
    }
    let birth_residence_holding_id = [mother_id, father_id].into_iter().find_map(|parent_id| {
        ctx.db
            .residence_transition()
            .iter()
            .filter(|transition| {
                transition.affected_character_id == parent_id
                    && transition.minute <= conceived_minute
                    && matches!(
                        transition.kind,
                        ResidenceTransitionKind::OccupantAdmitted
                            | ResidenceTransitionKind::OccupantRemoved
                    )
            })
            .max_by_key(|transition| {
                (
                    transition.minute,
                    matches!(transition.kind, ResidenceTransitionKind::OccupantAdmitted),
                )
            })
            .filter(|transition| transition.kind == ResidenceTransitionKind::OccupantAdmitted)
            .map(|transition| transition.holding_id)
    });
    let seeds = deterministic_child_seeds(
        &mother_id.to_string(),
        &father_id.to_string(),
        ordinal,
        due_minute,
        birth_settlement_id,
    );
    let mut reserved_child_id = seeds.identity;
    while ctx.db.character().id().find(reserved_child_id).is_some()
        || ctx
            .db
            .pregnancy()
            .iter()
            .any(|row| row.reserved_child_id == reserved_child_id)
    {
        reserved_child_id = reserved_child_id.wrapping_add(1);
    }
    let id = format!("pregnancy:{mother_id}:{ordinal}");
    let pregnancy = Pregnancy {
        id: id.clone(),
        mother_id,
        father_id,
        ordinal,
        conceived_minute,
        due_minute,
        reserved_child_id,
        child_name_seed: seeds.name,
        child_female: seeds.female,
        child_home_seed: seeds.home,
        birth_settlement_id: birth_settlement_id.to_owned(),
        birth_residence_holding_id,
        status: PregnancyStatus::Active,
        birth_character_id: None,
        resolved_minute: None,
    };
    ctx.db.pregnancy().insert(pregnancy.clone());
    ctx.db
        .child_identity_reservation()
        .insert(ChildIdentityReservation {
            character_id: reserved_child_id,
            pregnancy_id: id.clone(),
            reserved_minute: conceived_minute,
        });
    ctx.db.active_pregnancy().insert(ActivePregnancy {
        mother_id,
        pregnancy_id: id,
    });
    Ok(pregnancy)
}

fn conception_parents(
    ctx: &ReducerContext,
    first_id: u64,
    second_id: u64,
    trial_minute: u64,
) -> Result<Option<(u64, u64)>, String> {
    let first = ctx
        .db
        .character()
        .id()
        .find(first_id)
        .ok_or("First spouse not found")?;
    let second = ctx
        .db
        .character()
        .id()
        .find(second_id)
        .ok_or("Second spouse not found")?;
    let alive_at = |character_id: u64, alive_now: bool| {
        alive_now
            || ctx.db.strategic_corpse().iter().any(|corpse| {
                corpse.subject_character_id == Some(character_id)
                    && corpse.death_minute > trial_minute
            })
    };
    let adult_at = |character_id: u64, age_years: u16| {
        age_years >= ADULT_AGE_YEARS
            && ctx
                .db
                .pregnancy()
                .iter()
                .find(|pregnancy| pregnancy.birth_character_id == Some(character_id))
                .is_none_or(|birth| {
                    birth.due_minute.saturating_add(
                        u64::from(ADULT_AGE_YEARS)
                            * adventuresim_core::strategic_time::MINUTES_PER_YEAR,
                    ) <= trial_minute
                })
    };
    let married_at_trial = ctx.db.marriage().iter().any(|marriage| {
        ((marriage.first_character_id == first_id && marriage.second_character_id == second_id)
            || (marriage.first_character_id == second_id
                && marriage.second_character_id == first_id))
            && marriage.married_minute <= trial_minute
            && marriage
                .resolved_minute
                .is_none_or(|resolved| resolved > trial_minute)
    });
    if !alive_at(first_id, first.alive)
        || !alive_at(second_id, second.alive)
        || !adult_at(first_id, first.age_years)
        || !adult_at(second_id, second.age_years)
        || !married_at_trial
    {
        return Ok(None);
    }
    let first_personality = ctx
        .db
        .character_personality()
        .character_id()
        .find(first_id)
        .ok_or("First spouse personality not found")?;
    let second_personality = ctx
        .db
        .character_personality()
        .character_id()
        .find(second_id)
        .ok_or("Second spouse personality not found")?;
    let (mother_id, father_id) = match (first_personality.sex, second_personality.sex) {
        (Sex::Female, Sex::Male) => (first_id, second_id),
        (Sex::Male, Sex::Female) => (second_id, first_id),
        _ => return Ok(None),
    };
    Ok(Some((mother_id, father_id)))
}

fn refresh_spouse_pair_morale(
    ctx: &ReducerContext,
    first_id: u64,
    second_id: u64,
    joint_minutes: u64,
    minute: u64,
) -> Result<(), String> {
    let earned = spouse_leisure_earned_milli(joint_minutes);
    if earned == 0 {
        return Ok(());
    }
    let source = format!(
        "spouse-leisure:{}:{}",
        first_id.min(second_id),
        first_id.max(second_id)
    );
    for character_id in [first_id, second_id] {
        let existing = ctx
            .db
            .morale_event()
            .character_id()
            .filter(character_id)
            .find(|event| event.source_id.as_deref() == Some(&source));
        let residence_source = format!("residence-leisure:{character_id}");
        let residence = ctx
            .db
            .morale_event()
            .character_id()
            .filter(character_id)
            .find(|event| event.source_id.as_deref() == Some(&residence_source))
            .map_or(Default::default(), |event| {
                adventuresim_core::courtship::RefreshableMorale {
                    milli_points: (event.magnitude.max(0.0) * 1_000.0).round() as u32,
                    expires_at_minute: event.expires_at_minute,
                }
            });
        let refreshed = refresh_bounded_leisure_morale(
            existing.as_ref().map_or(Default::default(), |event| {
                adventuresim_core::courtship::RefreshableMorale {
                    milli_points: (event.magnitude.max(0.0) * 1_000.0).round() as u32,
                    expires_at_minute: event.expires_at_minute,
                }
            }),
            residence,
            minute,
            earned,
            SPOUSE_LEISURE_MORALE_SPEC,
        );
        crate::condition::upsert_fixed_morale_event_without_refresh(
            ctx,
            character_id,
            adventuresim_core::morale::MoraleEventKind::SpouseLeisure,
            refreshed.milli_points as f32 / 1_000.0,
            minute,
            refreshed.expires_at_minute,
            &source,
        );
        crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    }
    Ok(())
}

fn settle_spouse_leisure_pair(
    ctx: &ReducerContext,
    first_id: u64,
    second_id: u64,
) -> Result<(), String> {
    let (first_id, second_id) = canonical_pair(first_id, second_id);
    let pair_id = format!("spouse-leisure:{first_id}:{second_id}");
    let mut overlaps = Vec::new();
    for first in ctx
        .db
        .spouse_leisure_slice()
        .character_id()
        .filter(first_id)
    {
        for second in ctx
            .db
            .spouse_leisure_slice()
            .character_id()
            .filter(second_id)
        {
            let id = format!("spouse-overlap:{}:{}", first.id, second.id);
            if ctx.db.spouse_leisure_overlap().id().find(&id).is_some() {
                continue;
            }
            let joint = joint_leisure_minutes(
                LeisureInterval {
                    start_minute: first.start_minute,
                    end_minute: first.end_minute,
                    location_id: &first.location_id,
                },
                LeisureInterval {
                    start_minute: second.start_minute,
                    end_minute: second.end_minute,
                    location_id: &second.location_id,
                },
            );
            if joint > 0 {
                let start = first.start_minute.max(second.start_minute);
                let end = first.end_minute.min(second.end_minute);
                overlaps.push((
                    start,
                    end,
                    first.location_id.clone(),
                    id,
                    first.id.clone(),
                    second.id.clone(),
                    joint,
                ));
            }
        }
    }
    overlaps.sort_by(|left, right| (left.0, &left.1).cmp(&(right.0, &right.1)));
    for (overlap_start, overlap_end, location_id, id, first_slice_id, second_slice_id, joint) in
        overlaps
    {
        let mut accrual = ctx
            .db
            .spouse_leisure_accrual()
            .pair_id()
            .find(&pair_id)
            .unwrap_or(SpouseLeisureAccrual {
                pair_id: pair_id.clone(),
                first_character_id: first_id,
                second_character_id: second_id,
                conserved_joint_minutes: 0,
                next_trial_ordinal: 0,
                total_joint_minutes: 0,
            });
        let plan = conception_quantum_plan(
            ConceptionQuantumState {
                conserved_joint_minutes: accrual.conserved_joint_minutes,
                next_trial_ordinal: accrual.next_trial_ordinal,
            },
            joint,
        );
        for trial in &plan.trials {
            let receipt_id = format!("conception-trial:{pair_id}:{}", trial.ordinal);
            if ctx
                .db
                .conception_trial_receipt()
                .id()
                .find(&receipt_id)
                .is_some()
            {
                continue;
            }
            let minute = overlap_start.saturating_add(trial.crossing_offset_minutes);
            let ordinal = trial.ordinal.to_string();
            let entropy = (stable_lifecycle_hash(
                "spouse-conception",
                &[&first_id.to_string(), &second_id.to_string(), &ordinal],
            ) % u64::from(adventuresim_world_schema::BASIS_POINTS_PER_WHOLE))
                as u16;
            let parents = conception_parents(ctx, first_id, second_id, minute)?;
            let succeeded = parents.is_some()
                && succeeds_daily_trial(entropy, CONCEPTION_CHANCE_PER_TEN_THOUSAND)
                && parents.is_some_and(|(mother_id, _)| {
                    !ctx.db
                        .pregnancy()
                        .mother_id()
                        .filter(mother_id)
                        .any(|pregnancy| {
                            pregnancy.conceived_minute <= minute && minute < pregnancy.due_minute
                        })
                });
            ctx.db
                .conception_trial_receipt()
                .insert(ConceptionTrialReceipt {
                    id: receipt_id,
                    pair_id: pair_id.clone(),
                    ordinal: trial.ordinal,
                    minute,
                    succeeded,
                });
            if succeeded && let Some((mother_id, father_id)) = parents {
                establish_pregnancy(ctx, mother_id, father_id, minute, &location_id)?;
            }
        }
        accrual.conserved_joint_minutes = plan.state.conserved_joint_minutes;
        accrual.next_trial_ordinal = plan.state.next_trial_ordinal;
        accrual.total_joint_minutes = accrual.total_joint_minutes.saturating_add(joint);
        if ctx
            .db
            .spouse_leisure_accrual()
            .pair_id()
            .find(&pair_id)
            .is_some()
        {
            ctx.db.spouse_leisure_accrual().pair_id().update(accrual);
        } else {
            ctx.db.spouse_leisure_accrual().insert(accrual);
        }
        let resolved_minute = overlap_end;
        ctx.db
            .spouse_leisure_overlap()
            .insert(SpouseLeisureOverlap {
                id,
                first_slice_id,
                second_slice_id,
                joint_minutes: joint,
                resolved_minute,
            });
        refresh_spouse_pair_morale(ctx, first_id, second_id, joint, resolved_minute)?;
    }
    Ok(())
}

pub fn apply_spouse_leisure_conception(
    ctx: &ReducerContext,
    character_id: u64,
    interval_start: u64,
    interval_end: u64,
    schedule: DailySchedule,
) -> Result<(), String> {
    if interval_end <= interval_start {
        return Ok(());
    }
    let Some(marriage) = ctx.db.marriage().iter().find(|row| {
        (row.first_character_id == character_id || row.second_character_id == character_id)
            && row.married_minute < interval_end
            && row
                .resolved_minute
                .is_none_or(|resolved| resolved > interval_start)
    }) else {
        return Ok(());
    };
    let spouse_id = if marriage.first_character_id == character_id {
        marriage.second_character_id
    } else {
        marriage.first_character_id
    };
    let interval_start = interval_start.max(marriage.married_minute);
    let interval_end = interval_end.min(marriage.resolved_minute.unwrap_or(u64::MAX));
    if interval_end <= interval_start {
        return Ok(());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let Some(location_id) = character.current_settlement_id else {
        return Ok(());
    };
    for realized in restorative_leisure_spans(
        schedule,
        interval_start,
        interval_end.saturating_sub(interval_start),
    ) {
        let existing: Vec<_> = ctx
            .db
            .spouse_leisure_slice()
            .character_id()
            .filter(character_id)
            .filter(|slice| slice.location_id == location_id)
            .map(|slice| MinuteSpan {
                start_minute: slice.start_minute,
                end_minute: slice.end_minute,
            })
            .collect();
        for uncovered in uncovered_minute_spans(
            MinuteSpan {
                start_minute: realized.start_minute,
                end_minute: realized.end_minute,
            },
            existing,
        ) {
            let id = format!(
                "spouse-leisure-slice:{character_id}:{}:{}:{location_id}",
                uncovered.start_minute, uncovered.end_minute
            );
            ctx.db.spouse_leisure_slice().insert(SpouseLeisureSlice {
                id,
                character_id,
                start_minute: uncovered.start_minute,
                end_minute: uncovered.end_minute,
                location_id: location_id.clone(),
            });
        }
    }
    settle_spouse_leisure_pair(ctx, character_id, spouse_id)
}

/// Materialize due children as ordinary full Characters under NPC policy.
/// Age-restricted behavior remains elsewhere, but the child already has the
/// complete data/skills/needs surface and canonical family edges.
pub fn settle_due_births(ctx: &ReducerContext, mother_id: u64, now: u64) -> Result<(), String> {
    if let Some(pregnancy) = ctx
        .db
        .active_pregnancy()
        .mother_id()
        .find(mother_id)
        .and_then(|active| ctx.db.pregnancy().id().find(&active.pregnancy_id))
        .filter(|pregnancy| {
            pregnancy.status == PregnancyStatus::Active && pregnancy.due_minute <= now
        })
    {
        let mother_frontier = canonical_now(ctx, mother_id)?;
        if mother_frontier < pregnancy.due_minute {
            // Normal causal advancement must reach the due minute. Do not
            // jump NPCs past daily needs, disease, training, or socializing.
            return Ok(());
        }
    }
    let due: Vec<_> = ctx
        .db
        .active_pregnancy()
        .mother_id()
        .find(mother_id)
        .and_then(|active| ctx.db.pregnancy().id().find(&active.pregnancy_id))
        .filter(|pregnancy| {
            pregnancy.status == PregnancyStatus::Active && pregnancy.due_minute <= now
        })
        .into_iter()
        .collect();
    for mut pregnancy in due {
        let mother = ctx
            .db
            .character()
            .id()
            .find(pregnancy.mother_id)
            .ok_or("Pregnant mother not found")?;
        let father = ctx
            .db
            .character()
            .id()
            .find(pregnancy.father_id)
            .ok_or("Pregnant father not found")?;
        let child_id = pregnancy.reserved_child_id;
        if ctx.db.character().id().find(child_id).is_some() {
            return Err("Reserved child identity is no longer available".into());
        }
        let settlement_id = pregnancy.birth_settlement_id.clone();
        let newborn_life = crate::character::NpcLifeFacts {
            age_years: 0,
            organization_id: None,
            literacy: None,
        };
        crate::character::insert_character_with_origin(
            ctx,
            format!("Child-{:08x}", pregnancy.child_name_seed as u32),
            child_id,
            crate::character::CharacterCreationOptions {
                origin_settlement_id: Some(&settlement_id),
                mode: crate::character::CharacterCreationMode::Newborn,
                create_solo_party: false,
                materialize_generated_carry: false,
                stable_seed: pregnancy.child_name_seed,
                initial_time_minute: Some(pregnancy.due_minute),
                field_actor: false,
            },
            None,
            Some(&newborn_life),
        )?;
        crate::social_roles::copy_birth_family_roles(ctx, mother.id, child_id)?;
        record_character_birth(
            ctx,
            child_id,
            i64::try_from(pregnancy.due_minute).unwrap_or(i64::MAX),
        );
        if let Some(mut personality) = ctx.db.character_personality().character_id().find(child_id)
        {
            personality.sex = if pregnancy.child_female {
                Sex::Female
            } else {
                Sex::Male
            };
            ctx.db
                .character_personality()
                .character_id()
                .update(personality);
        }
        initialize_npc_policy(
            ctx,
            child_id,
            settlement_id.clone(),
            pregnancy.child_home_seed,
        )?;
        crate::continuity::initialize_child_continuity(
            ctx,
            child_id,
            mother.id,
            father.id,
            pregnancy.due_minute,
            pregnancy.child_home_seed,
        );
        ctx.db
            .child_identity_reservation()
            .character_id()
            .delete(child_id);
        ensure_kinship(
            ctx,
            child_id,
            mother.id,
            KinshipKind::Parent,
            pregnancy.due_minute,
        );
        ensure_kinship(
            ctx,
            child_id,
            father.id,
            KinshipKind::Parent,
            pregnancy.due_minute,
        );
        ensure_kinship(
            ctx,
            mother.id,
            child_id,
            KinshipKind::Child,
            pregnancy.due_minute,
        );
        ensure_kinship(
            ctx,
            father.id,
            child_id,
            KinshipKind::Child,
            pregnancy.due_minute,
        );
        if let Some(household_id) = household_id_at(ctx, mother.id, pregnancy.due_minute)
            .or_else(|| household_id_at(ctx, father.id, pregnancy.due_minute))
        {
            join_household(
                ctx,
                &household_id,
                child_id,
                pregnancy.due_minute,
                HouseholdRole::Dependent,
            );
        }
        if let Some(residence_holding_id) = [mother.id, father.id]
            .into_iter()
            .filter_map(|parent_id| {
                crate::residence::occupant_holding_id_at(ctx, parent_id, pregnancy.due_minute)
            })
            .find(|holding_id| {
                ctx.db
                    .residence_holding()
                    .id()
                    .find(holding_id.to_owned())
                    .is_some_and(|holding| {
                        crate::residence::holding_active_at(ctx, &holding.id, pregnancy.due_minute)
                            && holding.settlement_id == settlement_id
                    })
            })
        {
            // Housing is ancillary to an uncomplicated birth. If household
            // or occupancy authority changed during the pregnancy, the child
            // is still born and simply remains without this residence link.
            let _ = crate::residence::move_residence_occupant_effective(
                ctx,
                &residence_holding_id,
                child_id,
                pregnancy.due_minute,
            );
        }
        pregnancy.status = PregnancyStatus::Born;
        pregnancy.birth_character_id = Some(child_id);
        pregnancy.resolved_minute = Some(pregnancy.due_minute);
        ctx.db.pregnancy().id().update(pregnancy.clone());
        if ctx
            .db
            .active_pregnancy()
            .mother_id()
            .find(pregnancy.mother_id)
            .is_some_and(|active| active.pregnancy_id == pregnancy.id)
        {
            ctx.db
                .active_pregnancy()
                .mother_id()
                .delete(pregnancy.mother_id);
        }
    }
    Ok(())
}

/// Resolve a parent's household at an effective minute. Marriage history is
/// authoritative even when a later divorce or household move has replaced the
/// mutable active membership row.
fn household_id_at(ctx: &ReducerContext, character_id: u64, minute: u64) -> Option<String> {
    let marriage_household = ctx
        .db
        .marriage()
        .iter()
        .filter(|marriage| {
            (marriage.first_character_id == character_id
                || marriage.second_character_id == character_id)
                && marriage.married_minute <= minute
                && marriage
                    .resolved_minute
                    .is_none_or(|resolved| resolved > minute)
        })
        .max_by(|left, right| {
            (left.married_minute, left.id.as_str()).cmp(&(right.married_minute, right.id.as_str()))
        })
        .map(|marriage| marriage.household_id);
    marriage_household.or_else(|| {
        ctx.db
            .household_member()
            .character_id()
            .find(character_id)
            .filter(|member| member.joined_minute <= minute)
            .map(|member| member.household_id)
    })
}

fn validate_due_birth(ctx: &ReducerContext, pregnancy: &Pregnancy) -> Result<(), String> {
    pregnancy.parsed_state()?;
    if pregnancy.status != PregnancyStatus::Active {
        return Err("Pregnancy is not active".into());
    }
    if ctx.db.character().id().find(pregnancy.mother_id).is_none()
        || ctx.db.character().id().find(pregnancy.father_id).is_none()
    {
        return Err("Birth parents are unavailable".into());
    }
    if ctx
        .db
        .settlement()
        .id()
        .find(&pregnancy.birth_settlement_id)
        .is_none()
    {
        return Err("Birth settlement is unavailable".into());
    }
    if ctx
        .db
        .child_identity_reservation()
        .character_id()
        .find(pregnancy.reserved_child_id)
        .is_none_or(|row| row.pregnancy_id != pregnancy.id)
        || ctx
            .db
            .character()
            .id()
            .find(pregnancy.reserved_child_id)
            .is_some()
    {
        return Err("Reserved child identity is unavailable".into());
    }
    Ok(())
}

/// Materialize a stable, bounded slice of due pregnancies independently of
/// parent access. Each mother can have only one active pregnancy, so the
/// per-mother settlement call cannot exceed this selected batch.
pub fn settle_due_births_global(
    ctx: &ReducerContext,
    now: u64,
    limit: usize,
) -> Result<usize, String> {
    let mut due: Vec<_> = ctx
        .db
        .pregnancy()
        .iter()
        .filter(|row| row.status == PregnancyStatus::Active && row.due_minute <= now)
        .collect();
    due.sort_by(|left, right| {
        (left.due_minute, left.id.as_str()).cmp(&(right.due_minute, right.id.as_str()))
    });
    due.truncate(limit);
    let count = due.len();
    for pregnancy in due {
        settle_due_births(ctx, pregnancy.mother_id, now)?;
    }
    Ok(count)
}
