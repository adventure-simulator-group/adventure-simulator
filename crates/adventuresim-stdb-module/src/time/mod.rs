use adventuresim_core::activity::{ActivityLocation, LocationActivity};
use adventuresim_core::strategic_schedule::{
    ActivityOutcomeInputs, DailySchedule, SkillHours, SocializingSociability,
    apply_organization_training, apply_religion_training, apply_schedule_training,
    settlement_activity_outcome,
};
use adventuresim_core::strategic_time::{
    MAX_SETTLEMENT_REST_MINUTES, MINUTES_PER_DAY, WORLD_START_MINUTE, allocated_schedule_minutes,
    official_minutes as calculate_official_minutes,
};
use adventuresim_core::survival::{ExposureShelter, FieldShelter};
use adventuresim_core::{capability::aggregate_bounded_party_check, prelude::*};
use spacetimedb::{ReducerContext, SpacetimeType, Table, reducer, table};

use crate::capability::StrategicEquipment;
use crate::character::character;
use crate::condition::{character_condition as _, character_strategic_condition as _};
use crate::disease::character_illness_status as _;
use crate::investigation::case_site_authority as _;
use crate::organization::organization_membership as _;
use crate::personality::{
    Sociability as CharacterSociability, Transparency, character_personality,
};
use crate::relationship::{
    MarriageStatus, character_kinship as _, household_member as _, marriage as _, npc_policy as _,
    socializing_receipt as _,
};
use crate::strategic::{
    party_authority, party_inventory_item as _, party_journey_encounter_authority as _,
    party_member as _, road_challenge_authority as _, strategic_incident as _,
};
use crate::surgery::InjuryRecoveryMinutes;
use crate::{
    CharacterAttributes, CharacterSkills, CharacterStats, character_attributes, character_limbs,
    character_skills, character_stats, settlement,
};
use adventuresim_world_schema::{OfficialReligion, OralLanguage, SettlementActionService};
use std::collections::BTreeMap;

// Coordinates lifecycle settlement after authoritative writes and assembles
// the ordered time-policy fragments in their existing SpacetimeDB module scope.
include!("model.rs");
include!("clock.rs");
include!("advancement.rs");
include!("schedule.rs");
include!("activities.rs");
include!("settlement_rest.rs");
include!("camp_rest.rs");
include!("stationary.rs");

/// The single lifecycle boundary for an authoritative personal-clock write.
/// Callers finish injury/disease settlement first so wedding cancellation and
/// widowhood observe the final alive state at this frontier.
pub(crate) fn settle_lifecycle_after_character_time_write(
    ctx: &ReducerContext,
    character_id: u64,
    minute: u64,
) -> Result<(), String> {
    crate::relationship::settle_character_age(ctx, character_id, minute);
    crate::continuity::settle_continuity_for_character(ctx, character_id, minute)?;
    crate::residence::settle_residence_billing(ctx, character_id)?;
    crate::relationship::settle_due_weddings(ctx, character_id, minute)?;
    crate::relationship::settle_due_births(ctx, character_id, minute)?;
    crate::relationship::settle_secret_courtship_discovery_for_character(
        ctx,
        character_id,
        minute,
    )?;
    crate::relationship::settle_marriage_lifecycle_for_character(ctx, character_id, minute);
    crate::outbreak::refresh_patient_context_after_time_write(ctx, character_id, minute);
    Ok(())
}

#[cfg(test)]
pub(crate) const TIME_SOURCE: &str = concat!(
    include_str!("model.rs"),
    include_str!("clock.rs"),
    include_str!("advancement.rs"),
    include_str!("schedule.rs"),
    include_str!("activities.rs"),
    include_str!("settlement_rest.rs"),
    include_str!("camp_rest.rs"),
    include_str!("stationary.rs"),
    include_str!("mod.rs"),
);

#[cfg(test)]
mod tests {
    use super::*;

    mod clock_and_advancement {
        include!("tests/clock_and_advancement.rs");
    }

    mod schedule_and_activity {
        use super::*;
        include!("tests/schedule_and_activity.rs");
    }

    mod rest {
        use super::*;
        include!("tests/rest.rs");
    }
}
