struct LiveRunner {
    connection: DbConnection,
    profiles: Vec<AgentProfile>,
    character_ids: Vec<u64>,
    metrics: CoreLoopMetrics,
    trace: Vec<CoreLoopEvent>,
    sequence: u64,
    dialogue_nonce: u64,
    last_semantic_event: Option<String>,
    recorded_deaths: HashSet<u64>,
    medically_paused_schedules: HashSet<u64>,
    generated_seen_cases: HashSet<(u64, String)>,
    generated_terminal_cases: HashSet<(u64, String)>,
    generated_exact_site_cases: HashSet<(u64, String)>,
    generated_traveled_cases: HashSet<(u64, String)>,
    generated_finance_blocks: HashMap<(String, u64, String), (u64, u64)>,
    generated_discovery_backoff: HashMap<u64, PublicDiscoveryBackoff>,
    generated_defeat_fingerprints: HashMap<(u64, String), PublicCombatFingerprint>,
    failure_recorder: FailureRecorder,
}

const SMITHING_DECISION_SCALE: f32 = 1_000.0;

fn quantize_smithing_condition(value: f32) -> u32 {
    (value.clamp(0.0, 1.0) * SMITHING_DECISION_SCALE).round() as u32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MedicalChoice {
    Ready,
    SuppressQuest,
    RestNaturally,
    BuyAndRest,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PublicInterventionOffer {
    preparation_id: String,
    profile_version: u16,
    route: String,
    public_score_micropoints: i64,
    storefront_quote: u64,
    inventory_item_id: Option<u64>,
}

fn public_disease_id(value: &str) -> Option<adventuresim_core::disease::DiseaseId> {
    use adventuresim_core::disease::DiseaseId;
    match value {
        "influenza" => Some(DiseaseId::Influenza),
        "dysentery" => Some(DiseaseId::Dysentery),
        "typhus" => Some(DiseaseId::Typhus),
        "tetanus" => Some(DiseaseId::Tetanus),
        "erysipelas" => Some(DiseaseId::Erysipelas),
        "smallpox" => Some(DiseaseId::Smallpox),
        "plague" => Some(DiseaseId::Plague),
        "consumption" => Some(DiseaseId::Consumption),
        "mahrdruck" => Some(DiseaseId::Mahrdruck),
        "shroud_fever" => Some(DiseaseId::ShroudFever),
        "bilwisschuss" => Some(DiseaseId::Bilwisschuss),
        "kobeldunst" => Some(DiseaseId::Kobeldunst),
        _ => None,
    }
}

/// Score only the observer-safe weighted differential and public generic
/// preparation profiles. Positive values mean expected meter relief exceeds
/// direct and adverse meter burden. Quantization makes tie-breaking replayable.
fn public_intervention_score(
    differential: &[BackendPhysiologyDifferential],
    profile: &adventuresim_core::physiology::InterventionProfile,
) -> i64 {
    use adventuresim_core::physiology::{METER_COUNT, Meter};
    let total_likelihood = differential
        .iter()
        .filter(|row| public_disease_id(&row.disease_id).is_some())
        .map(|row| u64::from(row.likelihood_bps))
        .sum::<u64>();
    if total_likelihood == 0 {
        return 0;
    }
    let mut expected_loss = [0.0_f64; METER_COUNT];
    for row in differential {
        let Some(disease_id) = public_disease_id(&row.disease_id) else {
            continue;
        };
        let weight = f64::from(row.likelihood_bps) / total_likelihood as f64;
        for (meter, peak_loss) in adventuresim_core::disease::disease_peak_meters(disease_id) {
            expected_loss[meter.index()] += weight * f64::from(*peak_loss).max(0.0);
        }
    }
    let mut benefit = 0.0_f64;
    let mut burden = 0.0_f64;
    for meter in Meter::ALL {
        let expected = expected_loss[meter.index()];
        let direct = f64::from(profile.loss_delta_per_unit.get(meter));
        benefit += expected * (-direct).max(0.0);
        burden += expected * direct.max(0.0);
        let adverse = f64::from(profile.adverse_delta_per_unit.get(meter)).max(0.0);
        burden += adverse * (0.5 + expected);
    }
    ((benefit - burden) * 1_000_000.0).round() as i64
}

fn intervention_route_name(
    route: adventuresim_core::physiology::InterventionRoute,
) -> &'static str {
    use adventuresim_core::physiology::InterventionRoute;
    match route {
        InterventionRoute::Oral => "oral",
        InterventionRoute::Topical => "topical",
        InterventionRoute::Inhaled => "inhaled",
        InterventionRoute::Injected => "injected",
    }
}

fn public_confidence_band(confidence_bps: u16) -> &'static str {
    match confidence_bps {
        0..=2_999 => "low",
        3_000..=6_999 => "moderate",
        _ => "high",
    }
}

fn choose_medical_action(
    condition_status: &str,
    symptomatic: bool,
    at_settlement: bool,
    herbalist_available: bool,
    purse: u64,
    observable_quote: Option<u64>,
    natural_rest_venue: Option<bool>,
    medicated_rest_venue: Option<bool>,
) -> (MedicalChoice, &'static str) {
    if condition_status == "ready" && !symptomatic {
        return (MedicalChoice::Ready, "ready_without_symptoms");
    }
    if !at_settlement {
        return (MedicalChoice::SuppressQuest, "not_at_settlement");
    }
    if natural_rest_venue.is_none() {
        return (
            MedicalChoice::SuppressQuest,
            "rest_venue_unavailable_or_unaffordable",
        );
    }
    if !symptomatic {
        return (
            MedicalChoice::RestNaturally,
            "convalescing_without_symptoms",
        );
    }
    if !herbalist_available {
        return (MedicalChoice::RestNaturally, "herbalist_unavailable");
    }
    let Some(quote) = observable_quote else {
        return (MedicalChoice::RestNaturally, "observable_quote_unavailable");
    };
    if purse < quote || medicated_rest_venue.is_none() {
        return (MedicalChoice::RestNaturally, "observable_care_unaffordable");
    }
    (MedicalChoice::BuyAndRest, "symptomatic_and_affordable")
}

fn affordable_medical_rest_venue(
    inn_available: bool,
    temple_available: bool,
    temple_food_covers_day: bool,
    purse: u64,
    committed_cost: u64,
) -> Option<bool> {
    if temple_available && temple_food_covers_day && purse >= committed_cost {
        return Some(false);
    }
    let inn_cost = adventuresim_core::strategic_economy::inn_full_board_cost(1_440)?;
    (inn_available && purse >= committed_cost.saturating_add(inn_cost)).then_some(true)
}

fn temple_food_covers_one_day(visible_food_kcal: f32) -> bool {
    visible_food_kcal >= adventuresim_core::provisioning::STRATEGIC_TRAVEL_KCAL_PER_DAY
}

fn observable_herbalist_stocks_medication(
    herbalist_available: bool,
    medication_kind: bool,
    herbs_stocked: bool,
) -> bool {
    herbalist_available && medication_kind && herbs_stocked
}

/// Keep one player-visible course of local treatment available while making
/// discretionary equipment decisions. This is a concrete emergency reserve,
/// not an arbitrary wealth target.
fn spending_budget_after_medical_reserve(purse: u64, observable_quote: Option<u64>) -> u64 {
    purse.saturating_sub(observable_quote.unwrap_or(0))
}

fn equipment_spend_is_still_affordable(
    purse: u64,
    observable_medical_quote: Option<u64>,
    equipment_cost: u64,
) -> bool {
    equipment_cost <= spending_budget_after_medical_reserve(purse, observable_medical_quote)
}

fn live_attributes(character_id: u64, profile: &AgentProfile) -> CharacterAttributes {
    let a = &profile.attributes;
    CharacterAttributes {
        character_id,
        endurance: a.endurance,
        immunity: a.immunity,
        gut: a.gut,
        intelligence: a.intelligence,
        instinct: a.instinct,
        eyesight: a.eyesight,
        hearing: a.hearing,
        left_arm_strength: a.left_arm_strength,
        right_arm_strength: a.right_arm_strength,
        left_leg_strength: a.left_leg_strength,
        right_leg_strength: a.right_leg_strength,
        left_arm_agility: a.left_arm_agility,
        right_arm_agility: a.right_arm_agility,
        left_leg_agility: a.left_leg_agility,
        right_leg_agility: a.right_leg_agility,
    }
}

fn live_skills(character_id: u64, profile: &AgentProfile) -> CharacterSkills {
    let s = profile.initial_skills;
    CharacterSkills {
        character_id,
        polearm_hours: s.polearm,
        axe_hours: s.axe,
        bludgeon_hours: s.bludgeon,
        sword_hours: s.sword,
        knife_hours: s.knife,
        dodge_hours: s.dodge,
        block_hours: s.block,
        bow_hours: s.bow,
        crossbow_hours: s.crossbow,
        firearm_hours: s.firearm,
        throw_hours: s.throw,
        will_hours: s.will,
        insight_hours: s.insight,
        charm_hours: s.charm,
        command_hours: s.command,
        deception_hours: s.deception,
        physiology_hours: s.physiology,
        cooking_hours: s.cooking,
        herbalism_hours: s.herbalism,
        religion_hours: adventuresim_stdb_client::ReligionHours {
            roman_catholic: s.religion.roman_catholic,
            lutheran: s.religion.lutheran,
            reformed: s.religion.reformed,
            anglican: s.religion.anglican,
            eastern_orthodox: s.religion.eastern_orthodox,
            islamic: s.religion.islamic,
            judaism: s.religion.judaism,
        },
        bestiary_hours: adventuresim_stdb_client::BestiaryHours {
            beast: s.bestiary.beast,
            undead: s.bestiary.undead,
            human: s.bestiary.human,
            werekin: s.bestiary.werekin,
            elf: s.bestiary.elf,
            dwarf: s.bestiary.dwarf,
            fey: s.bestiary.fey,
            spirit: s.bestiary.spirit,
            greenskin: s.bestiary.greenskin,
            insectoid: s.bestiary.insectoid,
            draconid: s.bestiary.draconid,
            construct: s.bestiary.construct,
            wildmen: s.bestiary.wildmen,
        },
        surgery_hours: s.surgery,
        oral_languages: adventuresim_stdb_client::OralLanguageHours {
            east_central: 5_000.0,
            west_central: 0.0,
            low: 0.0,
            yiddish: 0.0,
            latin: 0.0,
            romani: 0.0,
            elven: 0.0,
            dwarfish: 0.0,
        },
        written_languages: adventuresim_stdb_client::WrittenLanguageHours {
            german: 0.0,
            low: 0.0,
            latin: 0.0,
            hebrew: 0.0,
            yiddish: 0.0,
            elven: 0.0,
            dwarfish: 0.0,
        },
        stealth_hours: s.stealth,
        balance_hours: s.balance,
        terrain_plains_hours: 0.0,
        terrain_forest_hours: 0.0,
        terrain_hills_hours: 0.0,
        terrain_wetlands_hours: 0.0,
        terrain_urban_hours: 0.0,
        terrain_snow_hours: s.terrain_snow,
        tailoring_hours: s.tailoring,
        smithing_hours: s.smithing,
    }
}

fn reallocate_disabled_crime_to_labor(mut schedule: ScheduleAllocation) -> ScheduleAllocation {
    let disabled_crime_minutes = schedule
        .thievery_minutes
        .checked_add(schedule.raiding_minutes)
        .expect("valid daily schedule crime allocation");
    schedule.labor_minutes = schedule
        .labor_minutes
        .checked_add(disabled_crime_minutes)
        .expect("valid daily schedule labor allocation");
    schedule.thievery_minutes = 0;
    schedule.raiding_minutes = 0;
    schedule
}

fn live_schedule(profile: &AgentProfile) -> ScheduleAllocation {
    let s = profile.schedule;
    // The live reducer accepts quarter-hour allocations. Native profiles are
    // intentionally more granular, so use the conservative lower notch and
    // leave the remainder as leisure instead of failing after medical rest.
    let quarter_hour = |minutes: u16| minutes / 15 * 15;
    reallocate_disabled_crime_to_labor(ScheduleAllocation {
        reading_minutes: 0,
        combat_training_minutes: quarter_hour(s.combat_training_minutes),
        carousing_minutes: quarter_hour(s.carousing_minutes),
        socializing_minutes: 0,
        // Simulation profiles may express future profession preferences that
        // the disposable character has not learned yet. Do not submit those
        // locked activities to the authoritative schedule reducer.
        apprenticeship_minutes: 0,
        apprenticeship_organization_id: None,
        profession_practice_minutes: 0,
        practice_organization_id: None,
        labor_minutes: quarter_hour(s.labor),
        prayer_minutes: quarter_hour(s.prayer),
        // Crime activities can open a tactical incident and move the party to
        // its case site. This authoritative evaluator deliberately leaves the
        // tactical layer untouched. Preserve the authored time allocation by
        // assigning those minutes to legal subsistence labor instead.
        thievery_minutes: quarter_hour(s.thievery),
        raiding_minutes: quarter_hour(s.raiding),
    })
}

fn schedule_allocated_minutes(schedule: &ScheduleAllocation) -> u16 {
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
        schedule.reading_minutes,
    ]
    .into_iter()
    .sum()
}

fn activity_schedule_plan(
    profile: &AgentProfile,
    temple_food_covers_day: bool,
    purse: u64,
    committed_reserve: u64,
    inn_cost: Option<u64>,
) -> (ScheduleAllocation, &'static str, &'static str) {
    let mut schedule = live_schedule(profile);
    let crime_fallback = matches!(
        profile.preferred_activity,
        ActivityPreference::Thievery | ActivityPreference::Raiding
    );
    let reserve_pressure =
        inn_cost.is_some_and(|cost| purse <= committed_reserve.saturating_add(cost));
    if schedule.labor_minutes == 0 && !temple_food_covers_day && reserve_pressure {
        let prayer_minutes = schedule.prayer_minutes;
        schedule.prayer_minutes = 0;
        if prayer_minutes > 0 {
            schedule.labor_minutes = prayer_minutes;
        } else {
            let discretionary_minutes =
                1_440_u16.saturating_sub(schedule_allocated_minutes(&schedule));
            schedule.labor_minutes = discretionary_minutes.min(480);
        }
        if schedule.labor_minutes > 0 {
            return (schedule, "Labor", "subsistence_reserve_to_labor");
        }
    }
    if crime_fallback {
        (schedule, "Labor", "crime_disabled_to_labor")
    } else {
        (
            schedule,
            match profile.preferred_activity {
                ActivityPreference::Labor => "Labor",
                ActivityPreference::Prayer => "Prayer",
                ActivityPreference::Thievery | ActivityPreference::Raiding => {
                    unreachable!("crime preferences are handled above")
                }
            },
            "none",
        )
    }
}

fn medical_rest_schedule() -> ScheduleAllocation {
    ScheduleAllocation {
        reading_minutes: 0,
        combat_training_minutes: 0,
        carousing_minutes: 0,
        socializing_minutes: 0,
        apprenticeship_minutes: 0,
        apprenticeship_organization_id: None,
        profession_practice_minutes: 0,
        practice_organization_id: None,
        labor_minutes: 0,
        prayer_minutes: 0,
        thievery_minutes: 0,
        raiding_minutes: 0,
    }
}

fn live_personality(character_id: u64, p: &crate::Personality) -> CharacterPersonality {
    CharacterPersonality {
        character_id,
        projection_character_id: character_id,
        nerve: match p.nerve {
            crate::Nerve::Neutral => adventuresim_stdb_client::Nerve::Neutral,
            crate::Nerve::Brave => adventuresim_stdb_client::Nerve::Brave,
            crate::Nerve::Fearful => adventuresim_stdb_client::Nerve::Fearful,
        },
        drive: match p.drive {
            crate::Drive::Neutral => adventuresim_stdb_client::Drive::Neutral,
            crate::Drive::Ambitious => adventuresim_stdb_client::Drive::Ambitious,
            crate::Drive::Content => adventuresim_stdb_client::Drive::Content,
        },
        outlook: match p.outlook {
            crate::Outlook::Neutral => adventuresim_stdb_client::Outlook::Neutral,
            crate::Outlook::Sanguine => adventuresim_stdb_client::Outlook::Sanguine,
            crate::Outlook::Brooding => adventuresim_stdb_client::Outlook::Brooding,
        },
        sociability: match p.sociability {
            crate::Sociability::Neutral => adventuresim_stdb_client::Sociability::Neutral,
            crate::Sociability::Gregarious => adventuresim_stdb_client::Sociability::Gregarious,
            crate::Sociability::Solitary => adventuresim_stdb_client::Sociability::Solitary,
        },
        conscience: match p.conscience {
            crate::Conscience::Neutral => adventuresim_stdb_client::Conscience::Neutral,
            crate::Conscience::Compassionate => adventuresim_stdb_client::Conscience::Compassionate,
            crate::Conscience::Callous => adventuresim_stdb_client::Conscience::Callous,
            crate::Conscience::Cruel => adventuresim_stdb_client::Conscience::Cruel,
        },
        self_regard: match p.self_regard {
            crate::SelfRegard::Neutral => adventuresim_stdb_client::SelfRegard::Neutral,
            crate::SelfRegard::Proud => adventuresim_stdb_client::SelfRegard::Proud,
            crate::SelfRegard::Humble => adventuresim_stdb_client::SelfRegard::Humble,
        },
        conviction: match p.conviction {
            crate::Conviction::Neutral => adventuresim_stdb_client::Conviction::Neutral,
            crate::Conviction::Zealous => adventuresim_stdb_client::Conviction::Zealous,
            crate::Conviction::Irreverent => adventuresim_stdb_client::Conviction::Irreverent,
        },
        hygiene: match p.hygiene {
            crate::Hygiene::Neutral => adventuresim_stdb_client::Hygiene::Neutral,
            crate::Hygiene::Slovenly => adventuresim_stdb_client::Hygiene::Slovenly,
            crate::Hygiene::Cleanly => adventuresim_stdb_client::Hygiene::Cleanly,
        },
        temperance: match p.temperance {
            crate::Temperance::Neutral => adventuresim_stdb_client::Temperance::Neutral,
            crate::Temperance::Temperate => adventuresim_stdb_client::Temperance::Temperate,
            crate::Temperance::Drunkard => adventuresim_stdb_client::Temperance::Drunkard,
        },
        mirth: match p.mirth {
            crate::Mirth::Neutral => adventuresim_stdb_client::Mirth::Neutral,
            crate::Mirth::Merry => adventuresim_stdb_client::Mirth::Merry,
            crate::Mirth::Grave => adventuresim_stdb_client::Mirth::Grave,
        },
        courtship: match p.courtship {
            crate::Courtship::Neutral => adventuresim_stdb_client::Courtship::Neutral,
            crate::Courtship::Amorous => adventuresim_stdb_client::Courtship::Amorous,
            crate::Courtship::Proper => adventuresim_stdb_client::Courtship::Proper,
        },
        transparency: match p.transparency {
            crate::Transparency::Neutral => adventuresim_stdb_client::Transparency::Neutral,
            crate::Transparency::Open => adventuresim_stdb_client::Transparency::Open,
            crate::Transparency::Guarded => adventuresim_stdb_client::Transparency::Guarded,
        },
        self_knowledge: match p.self_knowledge {
            crate::SelfKnowledge::Neutral => adventuresim_stdb_client::SelfKnowledge::Neutral,
            crate::SelfKnowledge::Introspective => {
                adventuresim_stdb_client::SelfKnowledge::Introspective
            }
            crate::SelfKnowledge::SelfDeceiving => {
                adventuresim_stdb_client::SelfKnowledge::SelfDeceiving
            }
        },
        inclination: match p.inclination {
            crate::Inclination::Men => adventuresim_stdb_client::Inclination::Men,
            crate::Inclination::Either => adventuresim_stdb_client::Inclination::Either,
            crate::Inclination::Women => adventuresim_stdb_client::Inclination::Women,
            crate::Inclination::Neither => adventuresim_stdb_client::Inclination::Neither,
        },
        presentation: match p.presentation {
            crate::Presentation::Man => adventuresim_stdb_client::Presentation::Man,
            crate::Presentation::Ambiguous => adventuresim_stdb_client::Presentation::Ambiguous,
            crate::Presentation::Woman => adventuresim_stdb_client::Presentation::Woman,
        },
        sex: match p.sex {
            crate::Sex::Female => adventuresim_stdb_client::Sex::Female,
            crate::Sex::Male => adventuresim_stdb_client::Sex::Male,
        },
    }
}

macro_rules! reducer_call {
    ($runner:expr, $label:expr, $invoke:expr) => {{
        let (tx, rx) = mpsc::sync_channel(1);
        ($invoke)(
            move |_: &ReducerEventContext,
                  result: Result<
                Result<(), String>,
                adventuresim_stdb_client::spacetimedb_sdk::__codegen::InternalError,
            >| {
                let normalized = result
                    .map_err(|error| error.to_string())
                    .and_then(|module_result| module_result);
                let _ = tx.send(normalized);
            },
        )
        .map_err(|error| format!("could not send {}: {error}", $label))?;
        match rx.recv_timeout(ACTION_TIMEOUT) {
            Ok(result) => result.map_err(|error| format!("{} failed: {error}", $label)),
            Err(_) => Err(format!("{} timed out after {:?}", $label, ACTION_TIMEOUT)),
        }
    }};
}
