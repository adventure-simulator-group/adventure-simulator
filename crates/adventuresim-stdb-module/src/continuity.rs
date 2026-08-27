//! Generational continuity: childhood development, lineage control, adult
//! promotion, and personal-estate succession.
//!
//! These authorities are private. The gateway projections at the bottom of
//! this module are scoped to the browser's selected observer and that
//! observer's personal frontier.

use adventuresim_core::{
    courtship::ADULT_AGE_YEARS,
    prelude::Skill,
    skill::apply_direct_training,
    strategic_time::{MINUTES_PER_DAY, MINUTES_PER_YEAR},
};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, table, view};

use crate::{
    browser_session::{
        BrowserCharacterGrantOrigin, browser_character_grant, browser_character_grant__view,
        browser_character_selection__view, grant_adult_descendant_internal,
    },
    character::{
        character, character__view, character_attributes, character_death, character_death__view,
        character_equipped_item, character_skills, unequip_wearable,
    },
    inventory_container::inventory_object,
    item::inventory_item,
    relationship::{
        HouseholdRole, KinshipKind, character_alive_at, character_birth, character_birth__view,
        character_kinship, character_kinship__view, effective_age_years, household_member,
        marriage, npc_policy,
    },
    strategic::{create_solo_party_for_character, strategic_gateway_authority__view},
    time::{character_time, character_time__view},
};

/// Qualitative age stages used by both policy and presentation. Boundaries are
/// exact authoritative birthdays, never cached wall-clock approximations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ChildStage {
    EarlyChildhood,
    MiddleChildhood,
    Adolescence,
    Adult,
}

/// Safe, closed childhood activities. They deliberately cannot name adult
/// paid, criminal, combat, incident, or organization work.
#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum ChildActivityFocus {
    Play,
    Study,
    HouseholdHelp,
    SocialLearning,
}

pub fn child_stage(age_years: u16) -> ChildStage {
    match age_years {
        0..=5 => ChildStage::EarlyChildhood,
        6..=11 => ChildStage::MiddleChildhood,
        12..=15 => ChildStage::Adolescence,
        _ => ChildStage::Adult,
    }
}

fn deterministic_child_focus(seed: u64) -> ChildActivityFocus {
    match seed % 4 {
        0 => ChildActivityFocus::Play,
        1 => ChildActivityFocus::Study,
        2 => ChildActivityFocus::HouseholdHelp,
        _ => ChildActivityFocus::SocialLearning,
    }
}

/// One row exists only for naturally born characters. `trained_through_minute`
/// is a durable exactly-once cursor; the focus is frozen at birth.
#[derive(Clone, Debug)]
#[table(accessor = child_development)]
pub struct ChildDevelopment {
    #[primary_key]
    pub character_id: u64,
    pub focus: ChildActivityFocus,
    pub trained_through_minute: u64,
    /// Effective hours actually contributed by the two frozen curriculum
    /// tracks. These are audit/baseline values, not replacements for skill
    /// totals that may also grow through other systems.
    pub first_curriculum_effective_hours: f32,
    pub second_curriculum_effective_hours: f32,
}

/// Private ownership claim frozen when the child is born. It does not itself
/// make a child playable.
#[derive(Clone, Debug)]
#[table(accessor = lineage_control_claim)]
pub struct LineageControlClaim {
    #[primary_key]
    pub child_id: u64,
    #[index(btree)]
    pub owner_key: String,
    pub source_parent_id: u64,
    pub established_minute: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum EstateHeirKind {
    DirectChild,
    Spouse,
    Unclaimed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum EstateDispositionStatus {
    Pending,
    Transferred,
    Unclaimed,
    HeirPredeceased,
}

fn valid_estate_choice(heir_kind: EstateHeirKind, chosen_heir_id: u64) -> bool {
    matches!(heir_kind, EstateHeirKind::Unclaimed) == (chosen_heir_id == 0)
}

/// Immutable/effective-dated succession choice plus its retry-safe settlement
/// state. The chosen heir is frozen at the first death transition.
#[derive(Clone, Debug)]
#[table(accessor = estate_disposition)]
pub struct EstateDisposition {
    #[primary_key]
    pub decedent_id: u64,
    /// Zero is the typed no-heir sentinel and is valid only for an Unclaimed
    /// disposition. A non-optional key keeps heir settlement indexable.
    #[index(btree)]
    pub chosen_heir_id: u64,
    pub heir_kind: EstateHeirKind,
    pub effective_minute: u64,
    pub status: EstateDispositionStatus,
    pub settled_minute: Option<u64>,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendFamilyChild {
    pub owner_key: String,
    pub observer_character_id: u64,
    pub child_id: u64,
    pub child_name: String,
    pub stage: ChildStage,
    pub focus: ChildActivityFocus,
    pub maturity_basis_points: u16,
    pub adult_playable: bool,
    pub alive: bool,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendEstateDisposition {
    pub owner_key: String,
    pub observer_character_id: u64,
    pub decedent_id: u64,
    pub chosen_heir_id: Option<u64>,
    pub heir_kind: EstateHeirKind,
    pub status: EstateDispositionStatus,
    pub effective_minute: u64,
}

pub(crate) fn initialize_child_continuity(
    ctx: &ReducerContext,
    child_id: u64,
    mother_id: u64,
    father_id: u64,
    birth_minute: u64,
    policy_seed: u64,
) {
    if ctx
        .db
        .child_development()
        .character_id()
        .find(child_id)
        .is_none()
    {
        ctx.db.child_development().insert(ChildDevelopment {
            character_id: child_id,
            focus: deterministic_child_focus(policy_seed),
            trained_through_minute: birth_minute,
            first_curriculum_effective_hours: 0.0,
            second_curriculum_effective_hours: 0.0,
        });
    }
    if ctx
        .db
        .lineage_control_claim()
        .child_id()
        .find(child_id)
        .is_some()
    {
        return;
    }
    // Maternal control wins ties, including the common same-owner case.
    let selected = [mother_id, father_id].into_iter().find_map(|parent_id| {
        ctx.db
            .browser_character_grant()
            .character_id()
            .find(parent_id)
            .map(|grant| (parent_id, grant.owner_key))
    });
    if let Some((source_parent_id, owner_key)) = selected {
        ctx.db.lineage_control_claim().insert(LineageControlClaim {
            child_id,
            owner_key,
            source_parent_id,
            established_minute: birth_minute,
        });
    }
}

fn direct_skill_hours_mut(skills: &mut crate::CharacterSkills, skill: Skill) -> Option<&mut f32> {
    Some(match skill {
        Skill::Dodge => &mut skills.dodge_hours,
        Skill::Balance => &mut skills.balance_hours,
        Skill::Insight => &mut skills.insight_hours,
        Skill::Charm => &mut skills.charm_hours,
        Skill::Physiology => &mut skills.physiology_hours,
        Skill::Cooking => &mut skills.cooking_hours,
        Skill::Tailoring => &mut skills.tailoring_hours,
        _ => return None,
    })
}

fn focus_training(focus: ChildActivityFocus, adolescent: bool) -> [(Skill, f32); 2] {
    let daily_hours = if adolescent { 3.0 } else { 2.0 };
    match focus {
        ChildActivityFocus::Play => [
            (Skill::Balance, daily_hours * 0.55),
            (Skill::Dodge, daily_hours * 0.45),
        ],
        ChildActivityFocus::Study => [
            (Skill::Insight, daily_hours * 0.6),
            (Skill::Physiology, daily_hours * 0.4),
        ],
        ChildActivityFocus::HouseholdHelp => [
            (Skill::Cooking, daily_hours * 0.6),
            (Skill::Tailoring, daily_hours * 0.4),
        ],
        ChildActivityFocus::SocialLearning => [
            (Skill::Charm, daily_hours * 0.55),
            (Skill::Insight, daily_hours * 0.45),
        ],
    }
}

fn curriculum_real_hours(
    focus: ChildActivityFocus,
    track: usize,
    birth_minute: u64,
    start_minute: u64,
    end_minute: u64,
) -> (Skill, f32) {
    let six = birth_minute.saturating_add(6 * MINUTES_PER_YEAR);
    let twelve = birth_minute.saturating_add(12 * MINUTES_PER_YEAR);
    let sixteen = birth_minute.saturating_add(u64::from(ADULT_AGE_YEARS) * MINUTES_PER_YEAR);
    let middle_minutes = end_minute
        .min(twelve)
        .saturating_sub(start_minute.max(six).min(end_minute.min(twelve)));
    let adolescent_minutes = end_minute
        .min(sixteen)
        .saturating_sub(start_minute.max(twelve).min(end_minute.min(sixteen)));
    let middle = focus_training(focus, false)[track];
    let adolescent = focus_training(focus, true)[track];
    debug_assert_eq!(middle.0, adolescent.0);
    let hours = middle_minutes as f32 / MINUTES_PER_DAY as f32 * middle.1
        + adolescent_minutes as f32 / MINUTES_PER_DAY as f32 * adolescent.1;
    (middle.0, hours)
}

/// Advance only the safe childhood curriculum. The durable cursor means each
/// interval is evaluated once with the attributes that existed for that
/// interval. Curriculum gains are added to, rather than substituted for,
/// independently earned skill hours.
fn settle_child_training(
    ctx: &ReducerContext,
    character_id: u64,
    minute: u64,
) -> Result<(), String> {
    let Some(mut development) = ctx.db.child_development().character_id().find(character_id) else {
        return Ok(());
    };
    let birth = ctx
        .db
        .character_birth()
        .character_id()
        .find(character_id)
        .ok_or("Child birth coordinate not found")?;
    let Ok(birth_minute) = u64::try_from(birth.birth_minute) else {
        return Err("Natural child birth minute cannot be negative".into());
    };
    let sixteen = birth_minute.saturating_add(u64::from(ADULT_AGE_YEARS) * MINUTES_PER_YEAR);
    let end = minute.min(sixteen);
    let cursor = development
        .trained_through_minute
        .max(birth_minute)
        .min(end);
    if cursor < end {
        let attributes = ctx
            .db
            .character_attributes()
            .character_id()
            .find(character_id)
            .ok_or("Child attributes not found")?;
        let mut skills = ctx
            .db
            .character_skills()
            .character_id()
            .find(character_id)
            .ok_or("Child skills not found")?;
        for index in 0..2 {
            let (skill, real_hours) =
                curriculum_real_hours(development.focus, index, birth_minute, cursor, end);
            if let Some(stored) = direct_skill_hours_mut(&mut skills, skill) {
                let gain = apply_direct_training(skill, stored, real_hours, &attributes);
                if index == 0 {
                    development.first_curriculum_effective_hours += gain.accepted_effective_hours;
                } else {
                    development.second_curriculum_effective_hours += gain.accepted_effective_hours;
                }
            }
        }
        ctx.db.character_skills().character_id().update(skills);
    }
    if development.trained_through_minute < end {
        development.trained_through_minute = end;
        ctx.db
            .child_development()
            .character_id()
            .update(development);
    }
    Ok(())
}

fn promote_household_role(ctx: &ReducerContext, character_id: u64) {
    if let Some(mut member) = ctx.db.household_member().character_id().find(character_id)
        && member.role == HouseholdRole::Dependent
    {
        member.role = HouseholdRole::AdultChild;
        ctx.db.household_member().id().update(member);
    }
}

fn promote_adult_descendant(
    ctx: &ReducerContext,
    character_id: u64,
    minute: u64,
) -> Result<(), String> {
    if effective_age_years(ctx, character_id, minute).unwrap_or(0) < ADULT_AGE_YEARS {
        return Ok(());
    }
    promote_household_role(ctx, character_id);
    let Some(_claim) = ctx.db.lineage_control_claim().child_id().find(character_id) else {
        return Ok(());
    };
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Adult descendant character not found")?;
    if !character.alive {
        return Ok(());
    }
    grant_adult_descendant_internal(ctx, character_id)?;
    if character.party_id.is_none() {
        create_solo_party_for_character(ctx, character_id)?;
    }
    ctx.db.npc_policy().character_id().delete(character_id);
    Ok(())
}

pub(crate) fn settle_continuity_for_character(
    ctx: &ReducerContext,
    character_id: u64,
    minute: u64,
) -> Result<(), String> {
    settle_child_training(ctx, character_id, minute)?;
    promote_adult_descendant(ctx, character_id, minute)?;
    settle_pending_inheritances_for_heir(ctx, character_id, minute)?;
    Ok(())
}

fn eldest_living_child_at(ctx: &ReducerContext, parent_id: u64, minute: u64) -> Option<u64> {
    let mut children = ctx
        .db
        .character_kinship()
        .subject_id()
        .filter(parent_id)
        .filter(|edge| edge.kind == KinshipKind::Child && edge.established_minute <= minute)
        .filter_map(|edge| {
            let birth = ctx
                .db
                .character_birth()
                .character_id()
                .find(edge.related_id)?;
            (i128::from(birth.birth_minute) <= i128::from(minute)
                && character_alive_at(ctx, edge.related_id, minute))
            .then_some((birth.birth_minute, edge.related_id))
        })
        .collect::<Vec<_>>();
    children.sort_unstable();
    children.first().map(|(_, id)| *id)
}

fn living_spouse_at(ctx: &ReducerContext, character_id: u64, minute: u64) -> Option<u64> {
    ctx.db
        .marriage()
        .iter()
        .filter(|row| {
            (row.first_character_id == character_id || row.second_character_id == character_id)
                && row.married_minute <= minute
                && row.resolved_minute.is_none_or(|resolved| resolved > minute)
        })
        .map(|row| {
            if row.first_character_id == character_id {
                row.second_character_id
            } else {
                row.first_character_id
            }
        })
        .find(|spouse_id| character_alive_at(ctx, *spouse_id, minute))
}

pub(crate) fn record_estate_disposition_for_death(
    ctx: &ReducerContext,
    decedent_id: u64,
    death_minute: u64,
) -> Result<(), String> {
    if ctx
        .db
        .estate_disposition()
        .decedent_id()
        .find(decedent_id)
        .is_some()
    {
        return Ok(());
    }
    let (chosen_heir_id, heir_kind) =
        if let Some(child) = eldest_living_child_at(ctx, decedent_id, death_minute) {
            (child, EstateHeirKind::DirectChild)
        } else if let Some(spouse) = living_spouse_at(ctx, decedent_id, death_minute) {
            (spouse, EstateHeirKind::Spouse)
        } else {
            (0, EstateHeirKind::Unclaimed)
        };
    let status = if chosen_heir_id != 0 {
        EstateDispositionStatus::Pending
    } else {
        EstateDispositionStatus::Unclaimed
    };
    if !valid_estate_choice(heir_kind, chosen_heir_id) {
        return Err("Estate heir provenance is inconsistent".into());
    }
    unequip_personal_estate(ctx, decedent_id)?;
    ctx.db.estate_disposition().insert(EstateDisposition {
        decedent_id,
        chosen_heir_id,
        heir_kind,
        effective_minute: death_minute,
        status,
        settled_minute: (status == EstateDispositionStatus::Unclaimed).then_some(death_minute),
    });
    if chosen_heir_id != 0
        && ctx
            .db
            .character_time()
            .character_id()
            .find(chosen_heir_id)
            .is_some_and(|time| time.minutes >= death_minute)
    {
        settle_pending_inheritances_for_heir(ctx, chosen_heir_id, death_minute)?;
    }
    Ok(())
}

fn unequip_personal_estate(ctx: &ReducerContext, decedent_id: u64) -> Result<(), String> {
    let equipped = ctx
        .db
        .character_equipped_item()
        .character_id()
        .filter(decedent_id)
        .map(|row| row.inventory_item_id)
        .collect::<Vec<_>>();
    for inventory_item_id in equipped {
        unequip_wearable(ctx, inventory_item_id);
    }
    crate::capability::refresh_character_capability(ctx, decedent_id)?;
    Ok(())
}

fn transfer_personal_estate(
    ctx: &ReducerContext,
    decedent_id: u64,
    heir_id: u64,
) -> Result<(), String> {
    // Tear down every body and item-attachment anchor before ownership moves.
    // Item IDs remain stable, preserving amounts, food lots, and condition.
    unequip_personal_estate(ctx, decedent_id)?;
    let object_roots = ctx
        .db
        .inventory_object()
        .iter()
        .filter(|object| {
            matches!(
                &object.location,
                adventuresim_core::physical_object::InventoryLocation::Personal(location)
                    if location.character_id == decedent_id
            )
        })
        .filter(|object| !crate::inventory_container::object_is_nested(ctx, object.id))
        .map(|object| object.id)
        .collect::<Vec<_>>();
    for object_id in object_roots {
        let destination =
            adventuresim_core::physical_object::OperationalCustody::character(heir_id)
                .map_err(|error| error.to_string())?;
        crate::inventory_container::rehome_subtree(ctx, object_id, &destination)?;
    }
    let mut inventory = ctx
        .db
        .inventory_item()
        .character_id()
        .filter(decedent_id)
        .collect::<Vec<_>>();
    inventory.sort_by_key(|row| row.id);
    for mut row in inventory {
        row.character_id = heir_id;
        ctx.db.inventory_item().id().update(row);
    }
    crate::capability::refresh_character_capability(ctx, heir_id)?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EstateRouteState {
    Living,
    Dead {
        death_minute: u64,
        status: Option<EstateDispositionStatus>,
        chosen_heir_id: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EstateRouteOutcome {
    Destination(u64),
    InitialHeirPredeceased,
    UnclaimedAt(u64),
}

/// Follow already-materialized later estates without replaying their transfer.
/// A pending later estate remains an intentional staging point: when its heir
/// reaches the effective date it will transfer everything then present.
fn route_materialized_inheritance(
    initial_heir_id: u64,
    initial_effective_minute: u64,
    mut state_for: impl FnMut(u64) -> EstateRouteState,
) -> Result<EstateRouteOutcome, String> {
    let mut current = initial_heir_id;
    let mut effective = initial_effective_minute;
    let mut visited = Vec::new();
    loop {
        if visited.contains(&current) {
            return Err("Estate succession cycle detected".into());
        }
        visited.push(current);
        match state_for(current) {
            EstateRouteState::Living => return Ok(EstateRouteOutcome::Destination(current)),
            EstateRouteState::Dead {
                death_minute,
                status,
                chosen_heir_id,
            } => {
                if death_minute <= effective {
                    return Ok(EstateRouteOutcome::InitialHeirPredeceased);
                }
                match status {
                    Some(EstateDispositionStatus::Transferred) if chosen_heir_id != 0 => {
                        current = chosen_heir_id;
                        effective = death_minute;
                    }
                    Some(EstateDispositionStatus::Unclaimed)
                    | Some(EstateDispositionStatus::HeirPredeceased) => {
                        return Ok(EstateRouteOutcome::UnclaimedAt(current));
                    }
                    _ => return Ok(EstateRouteOutcome::Destination(current)),
                }
            }
        }
    }
}

fn materialized_inheritance_route(
    ctx: &ReducerContext,
    heir_id: u64,
    effective_minute: u64,
) -> Result<EstateRouteOutcome, String> {
    route_materialized_inheritance(heir_id, effective_minute, |character_id| {
        let Some(death) = ctx.db.character_death().character_id().find(character_id) else {
            return EstateRouteState::Living;
        };
        let disposition = ctx.db.estate_disposition().decedent_id().find(character_id);
        EstateRouteState::Dead {
            death_minute: death.strategic_minute,
            status: disposition.as_ref().map(|row| row.status),
            chosen_heir_id: disposition.map_or(0, |row| row.chosen_heir_id),
        }
    })
}

pub(crate) fn settle_pending_inheritances_for_heir(
    ctx: &ReducerContext,
    heir_id: u64,
    heir_frontier: u64,
) -> Result<(), String> {
    let heir_death_minute = ctx
        .db
        .character_death()
        .character_id()
        .find(heir_id)
        .map(|death| death.strategic_minute);
    let mut pending = ctx
        .db
        .estate_disposition()
        .chosen_heir_id()
        .filter(heir_id)
        .filter(|row| {
            row.status == EstateDispositionStatus::Pending
                && (row.effective_minute <= heir_frontier
                    || heir_death_minute.is_some_and(|death| death <= row.effective_minute))
        })
        .collect::<Vec<_>>();
    pending.sort_by_key(|row| (row.effective_minute, row.decedent_id));
    for mut disposition in pending {
        if heir_death_minute.is_some_and(|death| death <= disposition.effective_minute) {
            // The frozen heir is not reselected: a causally later discovery
            // that they predeceased the estate makes this estate unclaimed.
            disposition.status = EstateDispositionStatus::HeirPredeceased;
        } else {
            match materialized_inheritance_route(ctx, heir_id, disposition.effective_minute)? {
                EstateRouteOutcome::InitialHeirPredeceased => {
                    disposition.status = EstateDispositionStatus::HeirPredeceased;
                }
                EstateRouteOutcome::Destination(destination)
                | EstateRouteOutcome::UnclaimedAt(destination) => {
                    transfer_personal_estate(ctx, disposition.decedent_id, destination)?;
                    disposition.status = EstateDispositionStatus::Transferred;
                }
            }
        }
        disposition.settled_minute = Some(disposition.effective_minute);
        ctx.db
            .estate_disposition()
            .decedent_id()
            .update(disposition);
    }
    Ok(())
}

fn view_is_gateway(ctx: &ViewContext) -> bool {
    ctx.db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|row| row.identity == ctx.sender())
}

/// Selected-observer family projection. The claim supplies owner scope; the
/// observer's personal frontier prevents future births/adulthood from leaking.
#[view(accessor = backend_family_children, public)]
pub fn backend_family_children(ctx: &ViewContext) -> Vec<BackendFamilyChild> {
    if !view_is_gateway(ctx) {
        return Vec::new();
    }
    let mut rows = Vec::new();
    for selection in ctx
        .db
        .browser_character_selection()
        .character_scan_id()
        .filter(0u64..)
    {
        let Some(observer_grant) = ctx
            .db
            .browser_character_grant()
            .character_id()
            .find(selection.character_id)
            .filter(|grant| grant.owner_key == selection.owner_key)
        else {
            continue;
        };
        let Some(observer_minute) = ctx
            .db
            .character_time()
            .character_id()
            .find(selection.character_id)
            .map(|time| time.minutes)
        else {
            continue;
        };
        for edge in ctx
            .db
            .character_kinship()
            .subject_id()
            .filter(selection.character_id)
            .filter(|edge| {
                edge.kind == KinshipKind::Child && edge.established_minute <= observer_minute
            })
        {
            let Some(birth) = ctx
                .db
                .character_birth()
                .character_id()
                .find(edge.related_id)
                .filter(|birth| i128::from(birth.birth_minute) <= i128::from(observer_minute))
            else {
                continue;
            };
            let Some(child) = ctx.db.character().id().find(edge.related_id) else {
                continue;
            };
            let Some(development) = ctx.db.child_development().character_id().find(child.id) else {
                continue;
            };
            let age = effective_age_years_for_view(birth.birth_minute, observer_minute);
            let elapsed = i128::from(observer_minute)
                .saturating_sub(i128::from(birth.birth_minute))
                .max(0) as u128;
            let maturity = elapsed
                .saturating_mul(u128::from(
                    adventuresim_world_schema::BASIS_POINTS_PER_WHOLE,
                ))
                .checked_div(u128::from(ADULT_AGE_YEARS) * u128::from(MINUTES_PER_YEAR))
                .unwrap_or(0)
                .min(u128::from(
                    adventuresim_world_schema::BASIS_POINTS_PER_WHOLE,
                )) as u16;
            let adult_playable = age >= ADULT_AGE_YEARS
                && ctx
                    .db
                    .browser_character_grant()
                    .character_id()
                    .find(child.id)
                    .is_some_and(|grant| {
                        grant.owner_key == observer_grant.owner_key
                            && grant.origin == BrowserCharacterGrantOrigin::AdultDescendant
                    });
            rows.push(BackendFamilyChild {
                owner_key: observer_grant.owner_key.clone(),
                observer_character_id: selection.character_id,
                child_id: child.id,
                child_name: child.name,
                stage: child_stage(age),
                focus: development.focus,
                maturity_basis_points: maturity,
                adult_playable,
                alive: character_alive_at_for_view(ctx, child.id, observer_minute),
            });
        }
    }
    rows.sort_by_key(|row| {
        (
            row.owner_key.clone(),
            row.observer_character_id,
            row.child_id,
        )
    });
    rows
}

fn effective_age_years_for_view(birth_minute: i64, minute: u64) -> u16 {
    let elapsed = i128::from(minute).saturating_sub(i128::from(birth_minute));
    (elapsed.max(0) as u128 / u128::from(MINUTES_PER_YEAR)).min(u128::from(u16::MAX)) as u16
}

fn character_alive_at_for_view(ctx: &ViewContext, character_id: u64, minute: u64) -> bool {
    ctx.db.character().id().find(character_id).is_some()
        && ctx
            .db
            .character_death()
            .character_id()
            .find(character_id)
            .is_none_or(|death| death.strategic_minute > minute)
}

#[view(accessor = backend_estate_dispositions, public)]
pub fn backend_estate_dispositions(ctx: &ViewContext) -> Vec<BackendEstateDisposition> {
    if !view_is_gateway(ctx) {
        return Vec::new();
    }
    let mut rows = Vec::new();
    for selection in ctx
        .db
        .browser_character_selection()
        .character_scan_id()
        .filter(0u64..)
    {
        let Some(grant) = ctx
            .db
            .browser_character_grant()
            .character_id()
            .find(selection.character_id)
            .filter(|grant| grant.owner_key == selection.owner_key)
        else {
            continue;
        };
        let Some(frontier) = ctx
            .db
            .character_time()
            .character_id()
            .find(selection.character_id)
            .map(|time| time.minutes)
        else {
            continue;
        };
        let mut dispositions = ctx
            .db
            .estate_disposition()
            .decedent_id()
            .find(selection.character_id)
            .into_iter()
            .collect::<Vec<_>>();
        dispositions.extend(
            ctx.db
                .estate_disposition()
                .chosen_heir_id()
                .filter(selection.character_id)
                .filter(|row| row.decedent_id != selection.character_id),
        );
        for disposition in dispositions
            .into_iter()
            .filter(|row| row.effective_minute <= frontier)
        {
            rows.push(BackendEstateDisposition {
                owner_key: grant.owner_key.clone(),
                observer_character_id: selection.character_id,
                decedent_id: disposition.decedent_id,
                chosen_heir_id: (disposition.chosen_heir_id != 0)
                    .then_some(disposition.chosen_heir_id),
                heir_kind: disposition.heir_kind,
                status: disposition.status,
                effective_minute: disposition.effective_minute,
            });
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_boundaries_are_exact_and_closed() {
        assert_eq!(child_stage(0), ChildStage::EarlyChildhood);
        assert_eq!(child_stage(5), ChildStage::EarlyChildhood);
        assert_eq!(child_stage(6), ChildStage::MiddleChildhood);
        assert_eq!(child_stage(11), ChildStage::MiddleChildhood);
        assert_eq!(child_stage(12), ChildStage::Adolescence);
        assert_eq!(child_stage(15), ChildStage::Adolescence);
        assert_eq!(child_stage(16), ChildStage::Adult);
    }

    #[test]
    fn focuses_never_map_to_adult_or_illicit_activities() {
        for focus in [
            ChildActivityFocus::Play,
            ChildActivityFocus::Study,
            ChildActivityFocus::HouseholdHelp,
            ChildActivityFocus::SocialLearning,
        ] {
            for (skill, hours) in focus_training(focus, false) {
                assert!(hours > 0.0);
                assert!(matches!(
                    skill,
                    Skill::Balance
                        | Skill::Dodge
                        | Skill::Insight
                        | Skill::Charm
                        | Skill::Physiology
                        | Skill::Cooking
                        | Skill::Tailoring
                ));
            }
        }
    }

    #[test]
    fn curriculum_intervals_are_stage_split_and_chunk_invariant() {
        let birth = 1_000;
        let six = birth + 6 * MINUTES_PER_YEAR;
        let twelve = birth + 12 * MINUTES_PER_YEAR;
        let sixteen = birth + 16 * MINUTES_PER_YEAR;
        for track in 0..2 {
            let (_, whole) =
                curriculum_real_hours(ChildActivityFocus::Study, track, birth, birth, sixteen);
            let (_, early) =
                curriculum_real_hours(ChildActivityFocus::Study, track, birth, birth, six);
            let (_, middle) =
                curriculum_real_hours(ChildActivityFocus::Study, track, birth, six, twelve);
            let (_, adolescent) =
                curriculum_real_hours(ChildActivityFocus::Study, track, birth, twelve, sixteen);
            assert_eq!(early, 0.0);
            assert!((whole - middle - adolescent).abs() < 0.01);
        }
    }

    #[test]
    fn materialized_estates_cascade_through_three_generations() {
        let routed = route_materialized_inheritance(2, 100, |id| match id {
            2 => EstateRouteState::Dead {
                death_minute: 200,
                status: Some(EstateDispositionStatus::Transferred),
                chosen_heir_id: 3,
            },
            3 => EstateRouteState::Dead {
                death_minute: 300,
                status: Some(EstateDispositionStatus::Transferred),
                chosen_heir_id: 4,
            },
            _ => EstateRouteState::Living,
        });
        assert_eq!(routed, Ok(EstateRouteOutcome::Destination(4)));
    }

    #[test]
    fn out_of_order_assets_stop_at_a_pending_later_estate() {
        let routed = route_materialized_inheritance(2, 100, |id| match id {
            2 => EstateRouteState::Dead {
                death_minute: 200,
                status: Some(EstateDispositionStatus::Transferred),
                chosen_heir_id: 3,
            },
            3 => EstateRouteState::Dead {
                death_minute: 300,
                status: Some(EstateDispositionStatus::Pending),
                chosen_heir_id: 4,
            },
            _ => EstateRouteState::Living,
        });
        assert_eq!(routed, Ok(EstateRouteOutcome::Destination(3)));
    }

    #[test]
    fn estate_choice_requires_structural_heir_consistency() {
        assert!(valid_estate_choice(EstateHeirKind::DirectChild, 7));
        assert!(valid_estate_choice(EstateHeirKind::Unclaimed, 0));
        assert!(!valid_estate_choice(EstateHeirKind::Unclaimed, 7));
        assert!(!valid_estate_choice(EstateHeirKind::Spouse, 0));
    }
}
