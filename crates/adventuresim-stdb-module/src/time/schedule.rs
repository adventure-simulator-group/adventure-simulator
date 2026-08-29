// Owns schedule validation, location eligibility, training, and reading policy.
#[derive(Clone, Debug)]
struct ActivityExecutionLocation {
    policy: ActivityLocation,
    origin_settlement_id: Option<String>,
}

fn activity_execution_location(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<ActivityExecutionLocation, String> {
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if let Some(settlement_id) = character.current_settlement_id {
        let settlement = ctx
            .db
            .settlement()
            .id()
            .find(&settlement_id)
            .ok_or("Character's settlement not found")?;
        return Ok(ActivityExecutionLocation {
            policy: ActivityLocation::Settlement {
                has_inn: settlement
                    .economy
                    .has_service(adventuresim_world_schema::SettlementService::Inn),
            },
            origin_settlement_id: Some(settlement_id),
        });
    }
    if let Some(occupancy) =
        crate::investigation::current_character_case_site_occupancy(ctx, character_id)
    {
        let site = ctx
            .db
            .case_site_authority()
            .id_key()
            .find(occupancy.case_site_id.to_string())
            .ok_or("Character's case site not found")?;
        let is_incident_site = ctx
            .db
            .strategic_incident()
            .id_key()
            .find(site.case_id.clone())
            .is_some();
        return Ok(ActivityExecutionLocation {
            policy: if site.distance_m > 0 && !is_incident_site {
                ActivityLocation::NamedOutdoorLocation
            } else {
                ActivityLocation::IneligibleNamedLocation
            },
            origin_settlement_id: Some(site.origin_settlement_id),
        });
    }
    Ok(ActivityExecutionLocation {
        policy: ActivityLocation::JourneyCamp,
        origin_settlement_id: None,
    })
}

fn location_activity(activity: ImmediateActivity) -> Option<LocationActivity> {
    match activity {
        ImmediateActivity::Carousing => Some(LocationActivity::Carousing),
        ImmediateActivity::Thievery => Some(LocationActivity::Thievery),
        ImmediateActivity::Raiding => Some(LocationActivity::Raiding),
        _ => None,
    }
}

pub(crate) fn effective_location_schedule(
    schedule: &ScheduleAllocation,
    location: ActivityLocation,
    redistribution_seed: u64,
) -> ScheduleAllocation {
    let mut effective = schedule.clone();
    let redistributed = adventuresim_core::activity::redistribute_unavailable_segments(
        [
            schedule.combat_training_minutes,
            schedule.carousing_minutes,
            schedule.socializing_minutes,
            schedule.apprenticeship_minutes,
            schedule.profession_practice_minutes,
            schedule.labor_minutes,
            schedule.prayer_minutes,
            schedule.thievery_minutes,
            schedule.raiding_minutes,
        ],
        [
            true,
            location.allows(LocationActivity::Carousing),
            true,
            true,
            true,
            true,
            true,
            location.allows(LocationActivity::Thievery),
            location.allows(LocationActivity::Raiding),
        ],
        redistribution_seed,
    );
    effective.combat_training_minutes = redistributed[0];
    effective.carousing_minutes = redistributed[1];
    effective.socializing_minutes = redistributed[2];
    effective.apprenticeship_minutes = redistributed[3];
    effective.profession_practice_minutes = redistributed[4];
    effective.labor_minutes = redistributed[5];
    effective.prayer_minutes = redistributed[6];
    effective.thievery_minutes = redistributed[7];
    effective.raiding_minutes = redistributed[8];
    effective
}

fn default_schedule(character_id: u64) -> CharacterTrainingSchedule {
    CharacterTrainingSchedule {
        character_id,
        downtime: ScheduleAllocation::default(),
    }
}

fn ensure_character_time(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    let official_minutes = refresh_clock(ctx)?;
    if ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .is_none()
    {
        ctx.db.character_time().insert(CharacterTime {
            character_id,
            minutes: official_minutes,
        });
    }
    if ctx
        .db
        .character_training_schedule()
        .character_id()
        .find(character_id)
        .is_none()
    {
        ctx.db
            .character_training_schedule()
            .insert(default_schedule(character_id));
    }
    Ok(())
}

fn validate_organization_schedule(
    ctx: &ReducerContext,
    character_id: u64,
    schedule: &ScheduleAllocation,
) -> Result<(), String> {
    if schedule.apprenticeship_minutes > 0 {
        let organization_id = schedule
            .apprenticeship_organization_id
            .as_deref()
            .ok_or("Apprenticeship time requires an organization")?;
        crate::organization::require_activity_membership(ctx, character_id, organization_id)?;
    }
    if schedule.profession_practice_minutes > 0 {
        let organization_id = schedule
            .practice_organization_id
            .as_deref()
            .ok_or("Professional practice time requires an organization")?;
        let row =
            crate::organization::require_activity_membership(ctx, character_id, organization_id)?;
        let role = crate::organization::membership_role(ctx, &row)?;
        if !role.practice_allowed {
            return Err("This organization role does not permit independent practice".into());
        }
    }
    Ok(())
}

/// Sample organization eligibility at the beginning of an interval. Invalid
/// saved allocations become leisure without mutating the player's saved plan.
fn effective_organization_schedule(
    ctx: &ReducerContext,
    character_id: u64,
    schedule: &ScheduleAllocation,
) -> ScheduleAllocation {
    let mut effective = schedule.clone();
    if effective.apprenticeship_minutes > 0
        && effective
            .apprenticeship_organization_id
            .as_deref()
            .is_none_or(|organization_id| {
                crate::organization::require_activity_membership(ctx, character_id, organization_id)
                    .is_err()
            })
    {
        effective.apprenticeship_minutes = 0;
    }
    if effective.profession_practice_minutes > 0 {
        let eligible = effective
            .practice_organization_id
            .as_deref()
            .and_then(|organization_id| {
                let membership = crate::organization::require_activity_membership(
                    ctx,
                    character_id,
                    organization_id,
                )
                .ok()?;
                crate::organization::membership_role(ctx, &membership).ok()
            })
            .is_some_and(|role| role.practice_allowed);
        if !eligible {
            effective.profession_practice_minutes = 0;
        }
    }
    effective
}

fn activity_training_profile(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<adventuresim_core::strategic_schedule::ActivityTrainingProfile, String> {
    let equipment = StrategicEquipment::load(ctx, character_id);
    Ok(
        adventuresim_core::strategic_schedule::ActivityTrainingProfile {
            combat: equipment.combat_training_profile(),
        },
    )
}

fn apply_oral_language_training(
    ctx: &ReducerContext,
    character_id: u64,
    languages: &mut adventuresim_world_schema::OralLanguageHours,
    language: OralLanguage,
    real_hours: f32,
) -> f32 {
    let instinct = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .map_or(0.0, |attributes| attributes.instinct);
    adventuresim_core::skill::apply_language_training(
        languages.direct_mut(language),
        real_hours,
        instinct,
    )
    .excess_effective_hours
}

fn apply_training(
    ctx: &ReducerContext,
    character_id: u64,
    skills: &mut CharacterSkills,
    schedule: &ScheduleAllocation,
    elapsed: u64,
    activities: adventuresim_core::strategic_schedule::ActivityTrainingProfile,
) -> Result<f32, String> {
    let attributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .ok_or("Character attributes not found while applying training")?;
    let mut hours = SkillHours {
        polearm: skills.polearm_hours,
        axe: skills.axe_hours,
        bludgeon: skills.bludgeon_hours,
        sword: skills.sword_hours,
        knife: skills.knife_hours,
        dodge: skills.dodge_hours,
        block: skills.block_hours,
        bow: skills.bow_hours,
        crossbow: skills.crossbow_hours,
        firearm: skills.firearm_hours,
        throw: skills.throw_hours,
        will: skills.will_hours,
        insight: skills.insight_hours,
        charm: skills.charm_hours,
        command: skills.command_hours,
        deception: skills.deception_hours,
        physiology: skills.physiology_hours,
        cooking: skills.cooking_hours,
        herbalism: skills.herbalism_hours,
        religion: skills.religion_hours,
        bestiary: skills.bestiary_hours,
        surgery: skills.surgery_hours,
        stealth: skills.stealth_hours,
        balance: skills.balance_hours,
        terrain_plains: skills.terrain_plains_hours,
        terrain_forest: skills.terrain_forest_hours,
        terrain_hills: skills.terrain_hills_hours,
        terrain_wetlands: skills.terrain_wetlands_hours,
        terrain_urban: skills.terrain_urban_hours,
        terrain_snow: skills.terrain_snow_hours,
        tailoring: skills.tailoring_hours,
        smithing: skills.smithing_hours,
    };
    let personality = ctx
        .db
        .character_personality()
        .character_id()
        .find(character_id);
    let sociability =
        personality.as_ref().map_or(
            SocializingSociability::Neutral,
            |personality| match personality.sociability {
                CharacterSociability::Neutral => SocializingSociability::Neutral,
                CharacterSociability::Gregarious => SocializingSociability::Gregarious,
                CharacterSociability::Solitary => SocializingSociability::Solitary,
            },
        );
    let transparency = personality
        .as_ref()
        .map_or(Transparency::Neutral, |value| value.transparency);
    let mut excess = apply_schedule_training(
        &mut hours,
        core_schedule(schedule),
        elapsed,
        activities,
        sociability,
        transparency,
        &attributes,
    );
    let prayer_religion = ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .and_then(|condition| condition.religion_id)
        .as_deref()
        .and_then(OfficialReligion::from_id);
    excess += apply_religion_training(
        &mut hours.religion,
        elapsed,
        prayer_religion,
        schedule.prayer_minutes,
        &attributes,
    );
    if let Some(character) = ctx.db.character().id().find(character_id)
        && let Some(settlement_id) = character.current_settlement_id
        && let Some(settlement) = ctx.db.settlement().id().find(&settlement_id)
    {
        // Ordinary life supplies bounded ambient exposure during the
        // waking two-thirds of actual elapsed settlement time.
        let exposure = elapsed as f32 / 60.0 * (2.0 / 3.0);
        for (language, coefficient) in [
            (
                OralLanguage::EastCentral,
                settlement.languages.east_central_bp,
            ),
            (
                OralLanguage::WestCentral,
                settlement.languages.west_central_bp,
            ),
            (OralLanguage::Low, settlement.languages.low_bp),
        ] {
            excess += adventuresim_core::skill::apply_language_training(
                skills.oral_languages.direct_mut(language),
                exposure * f32::from(coefficient)
                    / f32::from(adventuresim_world_schema::BASIS_POINTS_PER_WHOLE),
                attributes.instinct,
            )
            .excess_effective_hours;
        }
    }
    for (minutes, organization_id) in [
        (
            schedule.apprenticeship_minutes,
            schedule.apprenticeship_organization_id.as_deref(),
        ),
        (
            schedule.profession_practice_minutes,
            schedule.practice_organization_id.as_deref(),
        ),
    ] {
        if minutes == 0 {
            continue;
        }
        if let Some(definition) =
            organization_id.and_then(adventuresim_core::organization::organization)
        {
            let work_hours = elapsed as f32 / MINUTES_PER_DAY as f32 * f32::from(minutes) / 60.0;
            let (organization_excess, written) = apply_organization_training(
                &mut hours,
                work_hours,
                definition,
                activities,
                &attributes,
            );
            excess += organization_excess;
            for (language, award) in written {
                excess += adventuresim_core::skill::apply_language_training(
                    skills.written_languages.direct_mut(language),
                    award,
                    attributes.intelligence,
                )
                .excess_effective_hours;
            }
        }
    }
    skills.polearm_hours = hours.polearm;
    skills.axe_hours = hours.axe;
    skills.bludgeon_hours = hours.bludgeon;
    skills.sword_hours = hours.sword;
    skills.knife_hours = hours.knife;
    skills.dodge_hours = hours.dodge;
    skills.block_hours = hours.block;
    skills.bow_hours = hours.bow;
    skills.crossbow_hours = hours.crossbow;
    skills.firearm_hours = hours.firearm;
    skills.throw_hours = hours.throw;
    skills.will_hours = hours.will;
    skills.insight_hours = hours.insight;
    skills.charm_hours = hours.charm;
    skills.command_hours = hours.command;
    skills.deception_hours = hours.deception;
    skills.physiology_hours = hours.physiology;
    skills.cooking_hours = hours.cooking;
    skills.herbalism_hours = hours.herbalism;
    skills.religion_hours = hours.religion;
    skills.bestiary_hours = hours.bestiary;
    skills.surgery_hours = hours.surgery;
    skills.stealth_hours = hours.stealth;
    skills.balance_hours = hours.balance;
    skills.terrain_plains_hours = hours.terrain_plains;
    skills.terrain_forest_hours = hours.terrain_forest;
    skills.terrain_hills_hours = hours.terrain_hills;
    skills.terrain_wetlands_hours = hours.terrain_wetlands;
    skills.terrain_urban_hours = hours.terrain_urban;
    skills.terrain_snow_hours = hours.terrain_snow;
    skills.tailoring_hours = hours.tailoring;
    skills.smithing_hours = hours.smithing;
    if schedule.reading_minutes > 0 {
        let reading_hours =
            elapsed as f32 / MINUTES_PER_DAY as f32 * f32::from(schedule.reading_minutes) / 60.0;
        excess += apply_reading_training(ctx, character_id, skills, reading_hours, &attributes)?;
    }
    Ok(excess)
}

fn total_socializing_receipt_minutes(ctx: &ReducerContext, actor_id: u64) -> u64 {
    ctx.db
        .socializing_receipt()
        .actor_id()
        .filter(actor_id)
        .fold(0_u64, |total, receipt| {
            total.saturating_add(receipt.minutes)
        })
}

fn terrain_book_skill(terrain: &str) -> Option<Skill> {
    Some(match terrain {
        "plains" => Skill::TerrainPlains,
        "forest" => Skill::TerrainForest,
        "hills" => Skill::TerrainHills,
        "wetlands" => Skill::TerrainWetlands,
        "urban" => Skill::TerrainUrban,
        "snow" => Skill::TerrainSnow,
        _ => return None,
    })
}

fn direct_skill_hours_mut(skills: &mut CharacterSkills, skill: Skill) -> &mut f32 {
    match skill {
        Skill::Polearm => &mut skills.polearm_hours,
        Skill::Axe => &mut skills.axe_hours,
        Skill::Bludgeon => &mut skills.bludgeon_hours,
        Skill::Sword => &mut skills.sword_hours,
        Skill::Knife => &mut skills.knife_hours,
        Skill::Dodge => &mut skills.dodge_hours,
        Skill::Block => &mut skills.block_hours,
        Skill::Bow => &mut skills.bow_hours,
        Skill::Crossbow => &mut skills.crossbow_hours,
        Skill::Firearm => &mut skills.firearm_hours,
        Skill::Throw => &mut skills.throw_hours,
        Skill::Will => &mut skills.will_hours,
        Skill::Insight => &mut skills.insight_hours,
        Skill::Charm => &mut skills.charm_hours,
        Skill::Command => &mut skills.command_hours,
        Skill::Deception => &mut skills.deception_hours,
        Skill::Physiology => &mut skills.physiology_hours,
        Skill::Cooking => &mut skills.cooking_hours,
        Skill::Herbalism => &mut skills.herbalism_hours,
        Skill::Stealth => &mut skills.stealth_hours,
        Skill::Balance => &mut skills.balance_hours,
        Skill::Surgery => &mut skills.surgery_hours,
        Skill::TerrainPlains => &mut skills.terrain_plains_hours,
        Skill::TerrainForest => &mut skills.terrain_forest_hours,
        Skill::TerrainHills => &mut skills.terrain_hills_hours,
        Skill::TerrainWetlands => &mut skills.terrain_wetlands_hours,
        Skill::TerrainUrban => &mut skills.terrain_urban_hours,
        Skill::TerrainSnow => &mut skills.terrain_snow_hours,
        Skill::Tailoring => &mut skills.tailoring_hours,
        Skill::Smithing => &mut skills.smithing_hours,
        Skill::Religion | Skill::Bestiary => unreachable!("family leaves have separate storage"),
    }
}

fn target_snapshot(
    skills: &CharacterSkills,
    target: &adventuresim_core::item_catalog_schema::BookTarget,
    attributes: &CharacterAttributes,
) -> Option<(f32, f32)> {
    use adventuresim_core::item_catalog_schema::BookTarget;
    match target {
        BookTarget::Written { language } => Some((
            adventuresim_core::book::written_rank(
                skills.written_languages.effective(*language),
                attributes.intelligence,
            ),
            attributes.intelligence,
        )),
        BookTarget::Religion { religion } => Some((
            Skill::Religion
                .capped_training_rank(skills.religion_hours.effective(*religion), attributes),
            Skill::Religion.governing_aptitude(attributes),
        )),
        BookTarget::Bestiary { category } => Some((
            Skill::Bestiary
                .capped_training_rank(skills.bestiary_hours.effective(*category), attributes),
            Skill::Bestiary.governing_aptitude(attributes),
        )),
        BookTarget::Terrain { terrain } => {
            let skill = terrain_book_skill(terrain)?;
            Some((
                skill.capped_training_rank(skills.effective_skill_hours(skill), attributes),
                skill.governing_aptitude(attributes),
            ))
        }
        BookTarget::Skill { .. } => {
            let skill = adventuresim_core::book::ordinary_skill(target)?;
            Some((
                skill.capped_training_rank(skills.effective_skill_hours(skill), attributes),
                skill.governing_aptitude(attributes),
            ))
        }
    }
}

fn apply_selected_book(
    skills: &mut CharacterSkills,
    book: &adventuresim_core::item_catalog_schema::Book,
    real_hours: f32,
    attributes: &CharacterAttributes,
) -> Result<adventuresim_core::book::BoundedBookGain, String> {
    use adventuresim_core::item_catalog_schema::BookTarget;
    let medium_rank = adventuresim_core::book::written_rank(
        skills.written_languages.effective(book.medium),
        attributes.intelligence,
    );
    let (rank, aptitude) = target_snapshot(skills, &book.target, attributes)
        .ok_or("Book catalog contains an unsupported training target")?;
    let (lower, upper) = adventuresim_core::book::rank_band(book);
    match &book.target {
        BookTarget::Written { language } => {
            Ok(adventuresim_core::book::apply_written_book_training(
                &mut skills.written_languages,
                book.medium,
                *language,
                rank,
                attributes.intelligence,
                lower,
                upper,
                real_hours,
            ))
        }
        BookTarget::Religion { religion } => {
            let baseline = skills.religion_hours;
            let direct = skills.religion_hours.direct_mut(*religion);
            Ok(adventuresim_core::book::apply_bounded_book_training(
                direct,
                rank,
                aptitude,
                lower,
                upper,
                real_hours,
                medium_rank,
                |rank| Skill::Religion.hours_for_rank(rank),
                |candidate| {
                    let mut projected = baseline;
                    *projected.direct_mut(*religion) = candidate;
                    projected.effective(*religion)
                },
            ))
        }
        BookTarget::Bestiary { category } => {
            let baseline = skills.bestiary_hours;
            let direct = skills.bestiary_hours.direct_mut(*category);
            Ok(adventuresim_core::book::apply_bounded_book_training(
                direct,
                rank,
                aptitude,
                lower,
                upper,
                real_hours,
                medium_rank,
                |rank| Skill::Bestiary.hours_for_rank(rank),
                |candidate| {
                    let mut projected = baseline;
                    *projected.direct_mut(*category) = candidate;
                    projected.effective(*category)
                },
            ))
        }
        BookTarget::Terrain { terrain } => {
            let skill = terrain_book_skill(terrain)
                .ok_or("Book catalog contains an unsupported terrain target")?;
            let transferred = skill
                .ordinary_correlations()
                .iter()
                .map(|(source, coefficient)| {
                    skills.skill_hours_trained(*source).max(0.0) * coefficient
                })
                .sum::<f32>()
                .max(0.0);
            Ok(adventuresim_core::book::apply_bounded_book_training(
                direct_skill_hours_mut(skills, skill),
                rank,
                aptitude,
                lower,
                upper,
                real_hours,
                medium_rank,
                |rank| skill.hours_for_rank(rank),
                |candidate| candidate.max(0.0) + transferred,
            ))
        }
        BookTarget::Skill { .. } => {
            let skill = adventuresim_core::book::ordinary_skill(&book.target)
                .ok_or("Book catalog contains an unsupported skill target")?;
            let transferred = skill
                .ordinary_correlations()
                .iter()
                .map(|(source, coefficient)| {
                    skills.skill_hours_trained(*source).max(0.0) * coefficient
                })
                .sum::<f32>()
                .max(0.0);
            Ok(adventuresim_core::book::apply_bounded_book_training(
                direct_skill_hours_mut(skills, skill),
                rank,
                aptitude,
                lower,
                upper,
                real_hours,
                medium_rank,
                |rank| skill.hours_for_rank(rank),
                |candidate| {
                    let candidate = candidate.max(0.0);
                    if skill.is_trained() && candidate <= 0.0 {
                        0.0
                    } else if skill.is_trained() {
                        candidate + transferred.min(candidate)
                    } else {
                        candidate + transferred
                    }
                },
            ))
        }
    }
}

fn apply_reading_training(
    ctx: &ReducerContext,
    character_id: u64,
    skills: &mut CharacterSkills,
    mut real_hours: f32,
    attributes: &CharacterAttributes,
) -> Result<f32, String> {
    use crate::item::inventory_item;
    use adventuresim_core::book::{BookCandidate, select_candidate};
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character disappeared while applying reading training")?;
    let personal = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(character_id)
        .filter(|row| row.quantity > 0)
        .map(|row| row.item_id)
        .collect::<std::collections::BTreeSet<_>>();
    let bookstore = character
        .current_settlement_id
        .as_ref()
        .and_then(|id| ctx.db.settlement().id().find(id))
        .filter(|settlement| {
            settlement
                .economy
                .has_service(adventuresim_world_schema::SettlementService::Bookstore)
        });
    let mut excess = 0.0;
    let mut unusable = std::collections::BTreeSet::new();
    // A bounded title can finish mid-interval; immediately continue with the
    // next eligible title. The catalog and item IDs give a stable order.
    while real_hours > 0.000_01 {
        let candidates = adventuresim_core::item_catalog::catalog()
            .iter()
            .filter_map(|item| {
                if unusable.contains(&item.id) {
                    return None;
                }
                let book = item.capabilities.book.as_ref()?;
                let owned = personal.contains(&item.id);
                let on_site = bookstore.as_ref().is_some_and(|settlement| {
                    book.settlement_allowlist.is_empty()
                        || book
                            .settlement_allowlist
                            .iter()
                            .any(|id| id == &settlement.id)
                });
                (owned || on_site).then_some(BookCandidate {
                    item_id: &item.id,
                    book,
                    personal: owned,
                })
            });
        let Some(selected) = select_candidate(candidates, |book| {
            let medium_rank = adventuresim_core::book::written_rank(
                skills.written_languages.effective(book.medium),
                attributes.intelligence,
            );
            target_snapshot(skills, &book.target, attributes).is_some_and(|(rank, aptitude)| {
                let (lower, upper) = adventuresim_core::book::rank_band(book);
                medium_rank >= adventuresim_core::book::READABLE_WRITTEN_RANK
                    && rank + 0.000_01 >= f32::from(lower)
                    && rank < f32::from(upper).min(aptitude)
                    && aptitude > f32::from(lower)
            })
        }) else {
            break;
        };
        let gain = apply_selected_book(skills, selected.book, real_hours, attributes)?;
        if gain.accepted_effective_hours <= 0.0 {
            unusable.insert(selected.item_id.to_owned());
            continue;
        }
        excess += 0.0;
        if gain.unused_real_hours >= real_hours - 0.000_01 {
            break;
        }
        real_hours = gain.unused_real_hours;
    }
    Ok(excess)
}

pub(crate) fn core_schedule(schedule: &ScheduleAllocation) -> DailySchedule {
    DailySchedule {
        reading_minutes: schedule.reading_minutes,
        combat_training_minutes: schedule.combat_training_minutes,
        carousing_minutes: schedule.carousing_minutes,
        socializing_minutes: schedule.socializing_minutes,
        apprenticeship_minutes: schedule.apprenticeship_minutes,
        profession_practice_minutes: schedule.profession_practice_minutes,
        labor: schedule.labor_minutes,
        prayer: schedule.prayer_minutes,
        thievery: schedule.thievery_minutes,
        raiding: schedule.raiding_minutes,
    }
}
