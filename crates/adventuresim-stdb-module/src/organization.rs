//! Authoritative organization membership, rank, dues, and presentation state.

use adventuresim_core::organization::{
    OrganizationDefinition, Privilege, Requirement, organization,
};
use adventuresim_core::skill::Skill;
use adventuresim_core::strategic_time::MINUTES_PER_DAY;
use adventuresim_world_schema::{BestiaryCategory, OfficialReligion};
use spacetimedb::{ReducerContext, Table, reducer, table};

use crate::{
    CharacterSkills, character::character, character_skills, character_time,
    condition::character_condition,
};

pub const MEMBERSHIP_ACTIVE: &str = "active";
pub const MEMBERSHIP_SUSPENDED: &str = "suspended";

#[derive(Clone, Debug)]
#[table(accessor = organization_membership, public)]
pub struct OrganizationMembership {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    #[index(btree)]
    pub character_id: u64,
    pub organization_id: String,
    pub rank_id: String,
    pub joined_minute: u64,
    pub dues_paid_through_minute: u64,
    pub status: String,
    pub apprenticeship_minutes_accrued: u64,
    pub practice_minutes_accrued: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = organization_presentation, public)]
pub struct OrganizationPresentation {
    #[primary_key]
    pub character_id: u64,
    pub organization_id: String,
}

pub fn membership(
    ctx: &ReducerContext,
    character_id: u64,
    organization_id: &str,
) -> Option<OrganizationMembership> {
    ctx.db
        .organization_membership()
        .character_id()
        .filter(character_id)
        .find(|row| row.organization_id == organization_id)
}

fn current_minute(ctx: &ReducerContext, character_id: u64) -> Result<u64, String> {
    ctx.db
        .character_time()
        .character_id()
        .find(character_id)
        .map(|row| row.minutes)
        .ok_or_else(|| "Character time record not found".to_string())
}

pub fn membership_is_current(row: &OrganizationMembership, minute: u64) -> bool {
    row.status == MEMBERSHIP_ACTIVE && minute <= row.dues_paid_through_minute
}

pub fn active_membership(
    ctx: &ReducerContext,
    character_id: u64,
    organization_id: &str,
) -> Result<OrganizationMembership, String> {
    let row = membership(ctx, character_id, organization_id)
        .ok_or("Character is not a member of that organization")?;
    if !membership_is_current(&row, current_minute(ctx, character_id)?) {
        return Err("Organization membership is suspended until dues are paid".into());
    }
    Ok(row)
}

fn bestiary_category(id: &str) -> Option<BestiaryCategory> {
    BestiaryCategory::ALL
        .into_iter()
        .find(|category| format!("{category:?}").eq_ignore_ascii_case(id))
}

fn skill_hours(skills: &CharacterSkills, skill: &str, leaf: Option<&str>) -> Option<(Skill, f32)> {
    Some(match skill {
        "will" => (Skill::Will, skills.will_hours),
        "insight" => (Skill::Insight, skills.insight_hours),
        "charm" => (Skill::Charm, skills.charm_hours),
        "command" => (Skill::Command, skills.command_hours),
        "deception" => (Skill::Deception, skills.deception_hours),
        "physiology" => (Skill::Physiology, skills.physiology_hours),
        "cooking" => (Skill::Cooking, skills.cooking_hours),
        "herbalism" => (Skill::Herbalism, skills.herbalism_hours),
        "religion" => (
            Skill::Religion,
            skills
                .religion_hours
                .direct(OfficialReligion::from_id(leaf?)?),
        ),
        "bestiary" => (
            Skill::Bestiary,
            skills.bestiary_hours.direct(bestiary_category(leaf?)?),
        ),
        "surgery" => (Skill::Surgery, skills.surgery_hours),
        "polearm" => (Skill::Polearm, skills.polearm_hours),
        "axe" => (Skill::Axe, skills.axe_hours),
        "bludgeon" => (Skill::Bludgeon, skills.bludgeon_hours),
        "sword" => (Skill::Sword, skills.sword_hours),
        "knife" => (Skill::Knife, skills.knife_hours),
        "bow" => (Skill::Bow, skills.bow_hours),
        "crossbow" => (Skill::Crossbow, skills.crossbow_hours),
        "firearm" => (Skill::Firearm, skills.firearm_hours),
        "throw" => (Skill::Throw, skills.throw_hours),
        "block" => (Skill::Block, skills.block_hours),
        "dodge" => (Skill::Dodge, skills.dodge_hours),
        "stealth" => (Skill::Stealth, skills.stealth_hours),
        "balance" => (Skill::Balance, skills.balance_hours),
        "terrain_plains" => (Skill::TerrainPlains, skills.terrain_plains_hours),
        "terrain_forest" => (Skill::TerrainForest, skills.terrain_forest_hours),
        "terrain_hills" => (Skill::TerrainHills, skills.terrain_hills_hours),
        "terrain_wetlands" => (Skill::TerrainWetlands, skills.terrain_wetlands_hours),
        "terrain_urban" => (Skill::TerrainUrban, skills.terrain_urban_hours),
        "terrain_snow" => (Skill::TerrainSnow, skills.terrain_snow_hours),
        "tailoring" => (Skill::Tailoring, skills.tailoring_hours),
        "smithing" => (Skill::Smithing, skills.smithing_hours),
        _ => return None,
    })
}

fn requirements_met(
    ctx: &ReducerContext,
    character_id: u64,
    requirements: &[Requirement],
) -> Result<(), String> {
    let skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Character skills not found")?;
    let professed = ctx
        .db
        .character_condition()
        .character_id()
        .find(character_id)
        .and_then(|row| row.religion_id);
    for requirement in requirements {
        match requirement {
            Requirement::SkillRating {
                skill,
                minimum,
                leaf,
            } => {
                let (skill_kind, hours) = skill_hours(&skills, skill, leaf.as_deref())
                    .ok_or_else(|| format!("Unknown organization skill requirement {skill}"))?;
                if skill_kind.training_rank(hours) < *minimum {
                    return Err(format!(
                        "Requires {}{} rating {:.1}",
                        skill.replace('_', " "),
                        leaf.as_ref()
                            .map_or(String::new(), |leaf| format!(" ({leaf})")),
                        minimum
                    ));
                }
            }
            Requirement::ProfessedReligion { religion } => {
                if professed.as_deref() != Some(religion) {
                    return Err(format!("Requires profession of {religion}"));
                }
            }
        }
    }
    Ok(())
}

fn require_local_chapter<'a>(
    ctx: &ReducerContext,
    character_id: u64,
    organization_id: &str,
) -> Result<&'a OrganizationDefinition, String> {
    let definition = organization(organization_id).ok_or("Unknown organization")?;
    let character = crate::character::require_living_character(ctx, character_id)?;
    let settlement_id = character
        .current_settlement_id
        .ok_or("Organization business may only be conducted in a settlement")?;
    if !definition.has_chapter(&settlement_id) {
        return Err("That organization has no chapter in this settlement".into());
    }
    Ok(definition)
}

#[reducer]
pub fn join_organization(
    ctx: &ReducerContext,
    character_id: u64,
    organization_id: String,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    crate::time::initialize_character_time(ctx, character_id)?;
    crate::relationship::enforce_temporal_scope(
        ctx,
        character_id,
        None,
        crate::relationship::TemporalScope::Institutional,
    )?;
    let definition = require_local_chapter(ctx, character_id, &organization_id)?;
    if membership(ctx, character_id, &organization_id).is_some() {
        return Ok(());
    }
    requirements_met(ctx, character_id, &definition.admission.requirements)?;
    requirements_met(ctx, character_id, &definition.ranks[0].requirements)?;
    if definition.admission.joining_fee > 0 {
        crate::item::consume_personal_currency(
            ctx,
            character_id,
            u64::from(definition.admission.joining_fee),
        )?;
    }
    let minute = current_minute(ctx, character_id)?;
    let paid_through = definition.dues.as_ref().map_or(u64::MAX, |dues| {
        minute.saturating_add(u64::from(dues.interval_days) * MINUTES_PER_DAY)
    });
    ctx.db
        .organization_membership()
        .insert(OrganizationMembership {
            id: 0,
            character_id,
            organization_id,
            rank_id: definition.ranks[0].id.clone(),
            joined_minute: minute,
            dues_paid_through_minute: paid_through,
            status: MEMBERSHIP_ACTIVE.into(),
            apprenticeship_minutes_accrued: 0,
            practice_minutes_accrued: 0,
        });
    Ok(())
}

#[reducer]
pub fn promote_organization_membership(
    ctx: &ReducerContext,
    character_id: u64,
    organization_id: String,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    let definition = require_local_chapter(ctx, character_id, &organization_id)?;
    let mut row = active_membership(ctx, character_id, &organization_id)?;
    let next = definition
        .next_rank(&row.rank_id)
        .ok_or("Character already holds the organization's highest rank")?;
    requirements_met(ctx, character_id, &next.requirements)?;
    row.rank_id = next.id.clone();
    ctx.db.organization_membership().id().update(row);
    Ok(())
}

#[reducer]
pub fn pay_organization_dues(
    ctx: &ReducerContext,
    character_id: u64,
    organization_id: String,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    let definition = require_local_chapter(ctx, character_id, &organization_id)?;
    let dues = definition
        .dues
        .as_ref()
        .ok_or("This organization charges no dues")?;
    let mut row = membership(ctx, character_id, &organization_id)
        .ok_or("Character is not a member of that organization")?;
    crate::item::consume_personal_currency(ctx, character_id, u64::from(dues.amount))?;
    let now = current_minute(ctx, character_id)?;
    let base = if membership_is_current(&row, now) {
        row.dues_paid_through_minute
    } else {
        now
    };
    row.dues_paid_through_minute =
        base.saturating_add(u64::from(dues.interval_days) * MINUTES_PER_DAY);
    row.status = MEMBERSHIP_ACTIVE.into();
    ctx.db.organization_membership().id().update(row);
    Ok(())
}

#[reducer]
pub fn present_organization(
    ctx: &ReducerContext,
    character_id: u64,
    organization_id: String,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    let definition = organization(&organization_id).ok_or("Unknown organization")?;
    active_membership(ctx, character_id, &organization_id)?;
    let character = crate::character::require_living_character(ctx, character_id)?;
    let settlement_id = character
        .current_settlement_id
        .ok_or("An organization can only be presented in a settlement")?;
    if !definition.recognition.includes(&settlement_id) {
        return Err("This settlement does not recognize that organization".into());
    }
    let row = OrganizationPresentation {
        character_id,
        organization_id,
    };
    if ctx
        .db
        .organization_presentation()
        .character_id()
        .find(character_id)
        .is_some()
    {
        ctx.db
            .organization_presentation()
            .character_id()
            .update(row);
    } else {
        ctx.db.organization_presentation().insert(row);
    }
    crate::equipment_law::enforce_equipment_compliance(ctx, character_id)?;
    Ok(())
}

#[reducer]
pub fn clear_organization_presentation(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<(), String> {
    crate::strategic::require_strategic_character_authority(ctx, character_id)?;
    ctx.db
        .organization_presentation()
        .character_id()
        .delete(character_id);
    crate::equipment_law::enforce_equipment_compliance(ctx, character_id)?;
    Ok(())
}

pub fn settle_membership_dues(ctx: &ReducerContext, character_id: u64) -> Result<(), String> {
    let now = current_minute(ctx, character_id)?;
    let mut lapsed = Vec::new();
    for mut row in ctx
        .db
        .organization_membership()
        .character_id()
        .filter(character_id)
    {
        if row.status == MEMBERSHIP_ACTIVE && now > row.dues_paid_through_minute {
            row.status = MEMBERSHIP_SUSPENDED.into();
            lapsed.push(row.organization_id.clone());
            ctx.db.organization_membership().id().update(row);
        }
    }
    if !lapsed.is_empty() {
        reconcile_presentation(ctx, character_id)?;
    }
    Ok(())
}

pub fn effective_presented_organization(ctx: &ReducerContext, character_id: u64) -> Option<String> {
    let presentation = ctx
        .db
        .organization_presentation()
        .character_id()
        .find(character_id)?;
    let character = ctx.db.character().id().find(character_id)?;
    let settlement_id = character.current_settlement_id.as_deref()?;
    let definition = organization(&presentation.organization_id)?;
    let membership = membership(ctx, character_id, &presentation.organization_id)?;
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)?
        .minutes;
    (definition.recognition.includes(settlement_id) && membership_is_current(&membership, minute))
        .then_some(presentation.organization_id)
}

fn globally_current_presented_organization(
    ctx: &ReducerContext,
    character_id: u64,
) -> Option<String> {
    let presentation = ctx
        .db
        .organization_presentation()
        .character_id()
        .find(character_id)?;
    let membership = membership(ctx, character_id, &presentation.organization_id)?;
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)?
        .minutes;
    membership_is_current(&membership, minute).then_some(presentation.organization_id)
}

/// Remove a stale presentation, then apply locally recognized equipment law.
/// Merely leaving a recognizing settlement does not discard the persisted
/// profession, because global privileges (including forage licenses) still use
/// it.
pub fn reconcile_presentation(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<Option<String>, String> {
    let effective = effective_presented_organization(ctx, character_id);
    if globally_current_presented_organization(ctx, character_id).is_none()
        && ctx
            .db
            .organization_presentation()
            .character_id()
            .find(character_id)
            .is_some()
    {
        ctx.db
            .organization_presentation()
            .character_id()
            .delete(character_id);
    }
    crate::equipment_law::enforce_equipment_compliance(ctx, character_id)?;
    Ok(effective)
}

pub fn presented_privilege(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: &str,
    privilege: Privilege,
) -> bool {
    let Some(presentation) = ctx
        .db
        .organization_presentation()
        .character_id()
        .find(character_id)
    else {
        return false;
    };
    let Some(definition) = organization(&presentation.organization_id) else {
        return false;
    };
    definition.recognition.includes(settlement_id)
        && definition.has_privilege(privilege)
        && active_membership(ctx, character_id, &presentation.organization_id).is_ok()
}

fn current_membership_grants(
    definition: &OrganizationDefinition,
    membership: &OrganizationMembership,
    minute: u64,
    privilege: Privilege,
) -> bool {
    membership_is_current(membership, minute)
        && definition.has_privilege_at_rank(&membership.rank_id, privilege)
}

/// Global presented privileges deliberately ignore current settlement and
/// local recognition. Presentation, active dues-current membership, and rank
/// remain authoritative.
pub fn global_presented_privilege(
    ctx: &ReducerContext,
    character_id: u64,
    privilege: Privilege,
) -> bool {
    let Some(organization_id) = globally_current_presented_organization(ctx, character_id) else {
        return false;
    };
    let Some(definition) = organization(&organization_id) else {
        return false;
    };
    let Some(membership) = membership(ctx, character_id, &organization_id) else {
        return false;
    };
    let Some(minute) = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .map(|row| row.minutes)
    else {
        return false;
    };
    current_membership_grants(definition, &membership, minute, privilege)
}

pub fn require_activity_membership(
    ctx: &ReducerContext,
    character_id: u64,
    organization_id: &str,
) -> Result<OrganizationMembership, String> {
    require_local_chapter(ctx, character_id, organization_id)?;
    active_membership(ctx, character_id, organization_id)
}

pub fn increment_activity_accrual(
    ctx: &ReducerContext,
    character_id: u64,
    organization_id: &str,
    apprenticeship: u64,
    practice: u64,
) {
    if let Some(mut row) = membership(ctx, character_id, organization_id) {
        row.apprenticeship_minutes_accrued = row
            .apprenticeship_minutes_accrued
            .saturating_add(apprenticeship);
        row.practice_minutes_accrued = row.practice_minutes_accrued.saturating_add(practice);
        ctx.db.organization_membership().id().update(row);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn membership_at(rank_id: &str, status: &str, paid_through: u64) -> OrganizationMembership {
        OrganizationMembership {
            id: 1,
            character_id: 7,
            organization_id: "lodge_hart_king".into(),
            rank_id: rank_id.into(),
            joined_minute: 0,
            dues_paid_through_minute: paid_through,
            status: status.into(),
            apprenticeship_minutes_accrued: 0,
            practice_minutes_accrued: 0,
        }
    }

    #[test]
    fn global_license_requires_current_membership_and_right_rank() {
        let definition = organization("lodge_hart_king").unwrap();
        let warden = membership_at("warden", MEMBERSHIP_ACTIVE, 100);
        assert!(current_membership_grants(
            definition,
            &warden,
            100,
            Privilege::ForagePlants
        ));
        assert!(!current_membership_grants(
            definition,
            &warden,
            100,
            Privilege::ForageHighGame
        ));
        let master = membership_at("master", MEMBERSHIP_ACTIVE, 100);
        assert!(current_membership_grants(
            definition,
            &master,
            100,
            Privilege::ForageHighGame
        ));
        let lapsed = membership_at("master", MEMBERSHIP_ACTIVE, 99);
        assert!(!current_membership_grants(
            definition,
            &lapsed,
            100,
            Privilege::ForageHighGame
        ));
        let suspended = membership_at("master", MEMBERSHIP_SUSPENDED, 100);
        assert!(!current_membership_grants(
            definition,
            &suspended,
            100,
            Privilege::ForageHighGame
        ));
    }
}
