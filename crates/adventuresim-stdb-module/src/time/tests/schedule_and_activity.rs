#[test]
fn effective_schedule_redistributes_location_activities_without_mutating_saved_plan() {
    let saved = ScheduleAllocation {
        carousing_minutes: 60,
        thievery_minutes: 90,
        raiding_minutes: 120,
        labor_minutes: 180,
        ..ScheduleAllocation::default()
    };
    let settlement = effective_location_schedule(
        &saved,
        ActivityLocation::Settlement { has_inn: false },
        42,
    );
    assert_eq!(settlement.carousing_minutes, 0);
    assert_eq!(settlement.raiding_minutes, 0);
    assert!(settlement.thievery_minutes >= 90);
    assert!(settlement.labor_minutes >= 180);
    assert_eq!(settlement.allocated_minutes(), saved.allocated_minutes());
    let saved_recovery = adventuresim_core::strategic_schedule::restorative_leisure_minutes(
        core_schedule(&saved),
        0,
        MINUTES_PER_DAY,
    );
    let effective_recovery = adventuresim_core::strategic_schedule::restorative_leisure_minutes(
        core_schedule(&settlement),
        0,
        MINUTES_PER_DAY,
    );
    assert_eq!(effective_recovery, saved_recovery);

    let outdoors =
        effective_location_schedule(&saved, ActivityLocation::NamedOutdoorLocation, 42);
    assert_eq!(outdoors.carousing_minutes, 0);
    assert_eq!(outdoors.thievery_minutes, 0);
    assert!(outdoors.raiding_minutes >= 120);
    assert!(outdoors.labor_minutes >= 180);
    assert_eq!(outdoors.allocated_minutes(), saved.allocated_minutes());
    assert_eq!(saved.carousing_minutes, 60);
    assert_eq!(saved.thievery_minutes, 90);
    assert_eq!(saved.raiding_minutes, 120);
}

#[test]
fn effective_schedule_uses_leisure_when_every_planned_activity_is_unavailable() {
    let saved = ScheduleAllocation {
        carousing_minutes: 60,
        raiding_minutes: 120,
        ..ScheduleAllocation::default()
    };
    let effective = effective_location_schedule(
        &saved,
        ActivityLocation::Settlement { has_inn: false },
        42,
    );
    assert_eq!(effective.allocated_minutes(), 0);
    assert_eq!(saved.allocated_minutes(), 180);
}

#[test]
fn immediate_activity_schedule_contains_only_the_selected_interval() {
    let schedule = immediate_activity_schedule(
        ImmediateActivity::ProfessionPractice,
        180,
        Some("weaponsmith_guild"),
    );
    assert_eq!(schedule.profession_practice_minutes, 180);
    assert_eq!(
        schedule.practice_organization_id.as_deref(),
        Some("weaponsmith_guild")
    );
    assert_eq!(schedule.allocated_minutes(), 180);
    let prayer = immediate_activity_schedule(ImmediateActivity::Prayer, 60, None);
    assert_eq!(prayer.prayer_minutes, 60);
    assert_eq!(prayer.allocated_minutes(), 60);
}

#[test]
fn stale_saved_organization_allocations_are_converted_to_leisure() {
    let source = crate::production_source(crate::time::TIME_SOURCE);
    let effective = source
        .split("fn effective_organization_schedule")
        .nth(1)
        .and_then(|tail| tail.split("fn activity_training_profile").next())
        .expect("effective organization schedule");
    assert!(effective.contains("effective.apprenticeship_minutes = 0"));
    assert!(effective.contains("effective.profession_practice_minutes = 0"));
    assert!(effective.contains("membership_role(ctx, &membership)"));
    assert!(effective.contains("role.practice_allowed"));
    assert!(!effective.contains("return Err"));
}

#[test]
fn organization_interval_samples_eligibility_before_advancing_and_settles_after_outcomes() {
    let source = crate::production_source(crate::time::TIME_SOURCE);
    for (start, end) in [
        ("fn rest_for_minutes", "fn inn_stay_cost"),
        (
            "pub(crate) fn advance_stationary_character_to(",
            "/// Advance through elapsed wall-clock time",
        ),
    ] {
        let interval = source
            .split(start)
            .nth(1)
            .and_then(|tail| tail.split(end).next())
            .expect("organization time interval");
        let sample = interval
            .find("effective_organization_schedule")
            .expect("start eligibility sample");
        let advance = interval
            .find(".update(character_time)")
            .expect("clock advance");
        let outcomes = interval
            .rfind("apply_activity_outcomes")
            .expect("activity outcomes");
        let settle = interval
            .rfind("settle_membership_dues")
            .expect("post-interval dues settlement");
        assert!(sample < advance);
        assert!(outcomes < settle);
    }
    let immediate = source
        .split("pub fn perform_immediate_activity")
        .nth(1)
        .and_then(|tail| tail.split("const ACTIVITY_MINUTE_SCALE").next())
        .expect("immediate organization interval");
    let availability = immediate.find("unavailable_reason").unwrap();
    let clock_initialization = immediate.find("ensure_character_time").unwrap();
    assert!(
        availability < clock_initialization,
        "location availability must reject before clock or outcome mutation"
    );
    assert!(immediate.contains("require_character_no_unresolved_encounter"));
    assert!(immediate.contains("IncidentStatus::Pending"));
    assert!(source.contains("site.distance_m > 0"));
    assert!(source.contains(".strategic_incident()"));
    assert!(source.contains(".id_key()"));
    assert!(source.contains(".find(site.case_id.clone())"));
    assert!(source.contains("ActivityLocation::IneligibleNamedLocation"));
    assert!(
        immediate.find("validate_organization_schedule").unwrap()
            < immediate.find(".update(character_time)").unwrap()
    );
    assert!(
        immediate.rfind("apply_activity_outcomes").unwrap()
            < immediate.rfind("settle_membership_dues").unwrap()
    );
}

#[test]
fn activity_training_uses_the_daily_minute_allocation() {
    let allocation = ScheduleAllocation {
        combat_training_minutes: 90,
        labor_minutes: 480,
        prayer_minutes: 60,
        ..Default::default()
    };
    let trained_for = |elapsed_minutes| {
        let mut hours = SkillHours::default();
        adventuresim_core::strategic_schedule::apply_schedule_training(
            &mut hours,
            core_schedule(&allocation),
            elapsed_minutes,
            ActivityTrainingProfile {
                combat: adventuresim_core::strategic_schedule::CombatTrainingProfile {
                    weapons: adventuresim_core::equipment::WeaponSkillDistribution {
                        sword: 1.0,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            },
            SocializingSociability::Neutral,
            Transparency::Neutral,
            &adventuresim_core::stub::StubAttributes,
        );
        hours
    };
    let one_day = trained_for(MINUTES_PER_DAY);
    let two_days = trained_for(MINUTES_PER_DAY * 2);
    for (daily, doubled) in [
        (one_day.sword, two_days.sword),
        (one_day.dodge, two_days.dodge),
        (one_day.balance, two_days.balance),
        (one_day.will, two_days.will),
    ] {
        assert!(daily > 0.0);
        assert!((doubled - daily * 2.0).abs() < f32::EPSILON);
    }
    assert!((one_day.sword - one_day.dodge).abs() < f32::EPSILON);
    assert!((one_day.sword - one_day.balance).abs() < f32::EPSILON);
    assert!(one_day.will > one_day.sword);
    assert_eq!(
        allocation.allocated_minutes(),
        u64::from(allocation.combat_training_minutes)
            + u64::from(allocation.labor_minutes)
            + u64::from(allocation.prayer_minutes)
    );
}

#[test]
fn stationary_social_training_requires_realized_conversation_time() {
    let source = crate::production_source(crate::time::TIME_SOURCE);
    let stationary = source
        .split("pub(crate) fn advance_stationary_character_to")
        .nth(1)
        .expect("stationary advancement");
    let socializing = stationary
        .find("apply_scheduled_socializing")
        .expect("scheduled Socializing");
    let training = stationary.find("apply_training").expect("training");
    assert!(socializing < training);
    assert!(stationary.contains("realized_socializing_minutes == 0"));
    assert!(stationary.contains("realized_training_schedule.socializing_minutes = 0"));
}
