//! Private browser-session ownership authority.
//!
//! A browser cookie never contains character identifiers. The trusted
//! strategic gateway derives a pseudonymous owner key from the signed opaque
//! cookie and uses these rows to resolve the browser's roster and selection.

use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::{
    character::{character, character__view, starting_character_claim},
    continuity::lineage_control_claim,
    relationship::{character_birth, character_birth__view, effective_age_years},
    strategic::{require_strategic_gateway, strategic_gateway_authority__view},
    time::{character_time, character_time__view},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, SpacetimeType)]
pub enum BrowserCharacterGrantOrigin {
    StartingCandidate,
    AdultDescendant,
    Recruitment,
}

#[derive(Clone, Debug)]
#[table(accessor = browser_character_grant)]
pub struct BrowserCharacterGrant {
    /// A character can belong to only one browser owner.
    #[primary_key]
    pub character_id: u64,
    /// Numeric scan key for the trusted gateway projection. SpacetimeDB
    /// views cannot range-scan string indexes.
    #[index(btree)]
    pub character_scan_id: u64,
    #[index(btree)]
    pub owner_key: String,
    pub origin: BrowserCharacterGrantOrigin,
    /// Exactly one provenance arm is populated, as selected by `origin`.
    /// Keeping the arms structural prevents parsing security-sensitive IDs
    /// out of ad-hoc strings.
    pub starting_claim_request_key: Option<String>,
    pub lineage_source_parent_id: Option<u64>,
    pub recruitment_source_id: Option<String>,
    pub granted_micros: i64,
}

fn valid_grant_provenance(grant: &BrowserCharacterGrant) -> bool {
    match grant.origin {
        BrowserCharacterGrantOrigin::StartingCandidate => {
            grant.starting_claim_request_key.is_some()
                && grant.lineage_source_parent_id.is_none()
                && grant.recruitment_source_id.is_none()
        }
        BrowserCharacterGrantOrigin::AdultDescendant => {
            grant.starting_claim_request_key.is_none()
                && grant.lineage_source_parent_id.is_some()
                && grant.recruitment_source_id.is_none()
        }
        BrowserCharacterGrantOrigin::Recruitment => {
            grant.starting_claim_request_key.is_none()
                && grant.lineage_source_parent_id.is_none()
                && grant.recruitment_source_id.is_some()
        }
    }
}

#[derive(Clone, Debug)]
#[table(accessor = browser_character_selection)]
pub struct BrowserCharacterSelection {
    #[primary_key]
    pub owner_key: String,
    #[unique]
    pub character_id: u64,
    /// Numeric scan key for owner-scoped trusted projections.
    #[index(btree)]
    pub character_scan_id: u64,
    pub selected_micros: i64,
}

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendBrowserCharacterAccess {
    pub owner_key: String,
    pub character_id: u64,
    pub selected: bool,
}

fn valid_owner_key(owner_key: &str) -> bool {
    owner_key.len() == 64
        && owner_key
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) fn grant_browser_character_internal(
    ctx: &ReducerContext,
    owner_key: &str,
    character_id: u64,
    starting_request_key: &str,
) -> Result<(), String> {
    if !valid_owner_key(owner_key) {
        return Err("Browser owner key is malformed".into());
    }
    let claim = ctx
        .db
        .starting_character_claim()
        .request_key()
        .find(starting_request_key.to_owned())
        .ok_or("Starting-character claim not found")?;
    if claim.character_id != character_id || claim.owner_key != owner_key {
        return Err("Starting-character claim belongs to a different browser owner".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if character.temporary {
        return Err("Temporary characters cannot be granted to browser sessions".into());
    }
    if let Some(existing) = ctx
        .db
        .browser_character_grant()
        .character_id()
        .find(character_id)
    {
        return if existing.owner_key == owner_key
            && existing.origin == BrowserCharacterGrantOrigin::StartingCandidate
            && existing.starting_claim_request_key.as_deref() == Some(starting_request_key)
            && existing.lineage_source_parent_id.is_none()
        {
            Ok(())
        } else {
            Err("Character is already owned by a different browser session".into())
        };
    }
    ctx.db
        .browser_character_grant()
        .insert(BrowserCharacterGrant {
            character_id,
            character_scan_id: character_id,
            owner_key: owner_key.to_owned(),
            origin: BrowserCharacterGrantOrigin::StartingCandidate,
            starting_claim_request_key: Some(starting_request_key.to_owned()),
            lineage_source_parent_id: None,
            recruitment_source_id: None,
            granted_micros: ctx.timestamp.to_micros_since_unix_epoch(),
        });
    Ok(())
}

/// Internal adult-descendant grant. Birth froze the owner and source parent,
/// so adulthood needs no browser-supplied provenance.
pub(crate) fn grant_adult_descendant_internal(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<(), String> {
    let claim = ctx
        .db
        .lineage_control_claim()
        .child_id()
        .find(character_id)
        .ok_or("Adult descendant lineage claim not found")?;
    let owner_key = claim.owner_key.as_str();
    let source_parent_id = claim.source_parent_id;
    if !valid_owner_key(owner_key) {
        return Err("Browser owner key is malformed".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Adult descendant not found")?;
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or("Adult descendant time not found")?
        .minutes;
    if claim.established_minute > minute || source_parent_id == character_id {
        return Err("Adult descendant lineage claim is not yet effective or is cyclic".into());
    }
    if character.temporary
        || !character.alive
        || effective_age_years(ctx, character_id, minute).unwrap_or(0)
            < adventuresim_core::courtship::ADULT_AGE_YEARS
    {
        return Err("Only a living adult descendant can receive a browser grant".into());
    }
    let source_grant = ctx
        .db
        .browser_character_grant()
        .character_id()
        .find(source_parent_id)
        .ok_or("Lineage source parent has no browser grant")?;
    if source_grant.owner_key.as_str() != owner_key || !valid_grant_provenance(&source_grant) {
        return Err("Lineage claim does not match its source parent's browser owner".into());
    }
    if let Some(existing) = ctx
        .db
        .browser_character_grant()
        .character_id()
        .find(character_id)
    {
        return if existing.owner_key == owner_key
            && existing.origin == BrowserCharacterGrantOrigin::AdultDescendant
            && existing.starting_claim_request_key.is_none()
            && existing.lineage_source_parent_id == Some(source_parent_id)
        {
            Ok(())
        } else {
            Err("Character is already owned by a different browser session".into())
        };
    }
    ctx.db
        .browser_character_grant()
        .insert(BrowserCharacterGrant {
            character_id,
            character_scan_id: character_id,
            owner_key: owner_key.to_owned(),
            origin: BrowserCharacterGrantOrigin::AdultDescendant,
            starting_claim_request_key: None,
            lineage_source_parent_id: Some(source_parent_id),
            recruitment_source_id: None,
            granted_micros: ctx.timestamp.to_micros_since_unix_epoch(),
        });
    Ok(())
}

/// Canonical exclusive browser ownership grant for systemic recruitment.
/// The caller derives `owner_key` from the recruiting actor's existing grant.
pub(crate) fn grant_recruited_character_internal(
    ctx: &ReducerContext,
    owner_key: &str,
    character_id: u64,
    source_id: &str,
) -> Result<(), String> {
    if !valid_owner_key(owner_key) || source_id.is_empty() {
        return Err("Recruitment grant provenance is malformed".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if character.temporary || !character.alive {
        return Err("Only a living persistent character can be recruited".into());
    }
    if let Some(existing) = ctx
        .db
        .browser_character_grant()
        .character_id()
        .find(character_id)
    {
        return if existing.owner_key == owner_key
            && existing.origin == BrowserCharacterGrantOrigin::Recruitment
            && existing.recruitment_source_id.as_deref() == Some(source_id)
            && valid_grant_provenance(&existing)
        {
            Ok(())
        } else {
            Err("Character already has an exclusive browser owner".into())
        };
    }
    ctx.db
        .browser_character_grant()
        .insert(BrowserCharacterGrant {
            character_id,
            character_scan_id: character_id,
            owner_key: owner_key.to_owned(),
            origin: BrowserCharacterGrantOrigin::Recruitment,
            starting_claim_request_key: None,
            lineage_source_parent_id: None,
            recruitment_source_id: Some(source_id.to_owned()),
            granted_micros: ctx.timestamp.to_micros_since_unix_epoch(),
        });
    Ok(())
}

pub(crate) fn clear_dead_character_selection(ctx: &ReducerContext, character_id: u64) {
    if let Some(selection) = ctx
        .db
        .browser_character_selection()
        .character_id()
        .find(character_id)
    {
        ctx.db
            .browser_character_selection()
            .owner_key()
            .delete(&selection.owner_key);
    }
}

#[reducer]
pub fn grant_browser_character(
    ctx: &ReducerContext,
    owner_key: String,
    character_id: u64,
    starting_request_key: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    grant_browser_character_internal(ctx, &owner_key, character_id, &starting_request_key)
}

#[reducer]
pub fn select_browser_character(
    ctx: &ReducerContext,
    owner_key: String,
    character_id: u64,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    if !valid_owner_key(&owner_key) {
        return Err("Browser owner key is malformed".into());
    }
    let grant = ctx
        .db
        .browser_character_grant()
        .character_id()
        .find(character_id)
        .ok_or("Character is not granted to this browser session")?;
    if grant.owner_key != owner_key || !valid_grant_provenance(&grant) {
        return Err("Character is not granted to this browser session".into());
    }
    let character = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    let minute = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or("Character time record not found")?
        .minutes;
    if !character.alive
        || effective_age_years(ctx, character_id, minute).unwrap_or(character.age_years)
            < adventuresim_core::courtship::ADULT_AGE_YEARS
    {
        return Err("Only living adult characters can be selected".into());
    }
    if grant.origin == BrowserCharacterGrantOrigin::AdultDescendant {
        let birth = ctx
            .db
            .character_birth()
            .character_id()
            .find(character_id)
            .ok_or("Adult descendant birth coordinate not found")?;
        let adulthood_minute = u64::try_from(i128::from(birth.birth_minute).saturating_add(
            i128::from(adventuresim_core::courtship::ADULT_AGE_YEARS)
                * i128::from(adventuresim_core::strategic_time::MINUTES_PER_YEAR),
        ))
        .map_err(|_| "Adult descendant adulthood coordinate is invalid")?;
        let selected_living_observer_minute = ctx
            .db
            .browser_character_selection()
            .owner_key()
            .find(&owner_key)
            .and_then(|selection| {
                ctx.db
                    .character()
                    .id()
                    .find(selection.character_id)
                    .filter(|selected| selected.alive)?;
                ctx.db
                    .character_time()
                    .character_id()
                    .find(selection.character_id)
                    .map(|time| time.minutes)
            });
        if !descendant_grant_visible_at(adulthood_minute, selected_living_observer_minute, minute) {
            return Err(
                "Adult descendant is not yet visible at the selected character's date".into(),
            );
        }
    }
    let selection = BrowserCharacterSelection {
        owner_key: owner_key.clone(),
        character_id,
        character_scan_id: character_id,
        selected_micros: ctx.timestamp.to_micros_since_unix_epoch(),
    };
    if ctx
        .db
        .browser_character_selection()
        .owner_key()
        .find(&owner_key)
        .is_some()
    {
        ctx.db
            .browser_character_selection()
            .owner_key()
            .update(selection);
    } else {
        ctx.db.browser_character_selection().insert(selection);
    }
    Ok(())
}

#[reducer]
pub fn clear_browser_character_selection(
    ctx: &ReducerContext,
    owner_key: String,
) -> Result<(), String> {
    require_strategic_gateway(ctx)?;
    if !valid_owner_key(&owner_key) {
        return Err("Browser owner key is malformed".into());
    }
    ctx.db
        .browser_character_selection()
        .owner_key()
        .delete(&owner_key);
    Ok(())
}

fn is_strategic_gateway(ctx: &ViewContext) -> bool {
    ctx.db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|authority| authority.identity == ctx.sender())
}

fn descendant_grant_visible_at(
    adulthood_minute: u64,
    selected_living_observer_minute: Option<u64>,
    descendant_frontier: u64,
) -> bool {
    adulthood_minute <= selected_living_observer_minute.unwrap_or(descendant_frontier)
}

fn adulthood_minute_for_view(ctx: &ViewContext, character_id: u64) -> Option<u64> {
    let birth = ctx.db.character_birth().character_id().find(character_id)?;
    let minute = i128::from(birth.birth_minute).saturating_add(
        i128::from(adventuresim_core::courtship::ADULT_AGE_YEARS)
            * i128::from(adventuresim_core::strategic_time::MINUTES_PER_YEAR),
    );
    u64::try_from(minute).ok()
}

#[view(accessor = backend_browser_character_access, public)]
pub fn backend_browser_character_access(ctx: &ViewContext) -> Vec<BackendBrowserCharacterAccess> {
    if !is_strategic_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .browser_character_grant()
        .character_scan_id()
        .filter(0u64..)
        .filter(|grant| {
            if !valid_grant_provenance(grant) {
                return false;
            }
            let Some(character) = ctx.db.character().id().find(grant.character_id) else {
                return false;
            };
            let Some(minute) = ctx
                .db
                .character_time()
                .character_id()
                .find(grant.character_id)
                .map(|time| time.minutes)
            else {
                return false;
            };
            if !character.alive
                || !effective_age_years_for_view(ctx, grant.character_id, minute)
                    .is_some_and(|age| age >= adventuresim_core::courtship::ADULT_AGE_YEARS)
            {
                return false;
            }
            if grant.origin != BrowserCharacterGrantOrigin::AdultDescendant {
                return true;
            }
            let Some(adulthood_minute) = adulthood_minute_for_view(ctx, grant.character_id) else {
                return false;
            };
            let selected_living_observer_minute = ctx
                .db
                .browser_character_selection()
                .owner_key()
                .find(&grant.owner_key)
                .and_then(|selection| {
                    let selected_grant = ctx
                        .db
                        .browser_character_grant()
                        .character_id()
                        .find(selection.character_id)?;
                    let selected_character =
                        ctx.db.character().id().find(selection.character_id)?;
                    (selected_grant.owner_key == grant.owner_key && selected_character.alive)
                        .then(|| {
                            ctx.db
                                .character_time()
                                .character_id()
                                .find(selection.character_id)
                                .map(|time| time.minutes)
                        })
                        .flatten()
                });
            descendant_grant_visible_at(adulthood_minute, selected_living_observer_minute, minute)
        })
        .map(|grant| {
            let selected = ctx
                .db
                .browser_character_selection()
                .owner_key()
                .find(&grant.owner_key)
                .is_some_and(|selection| selection.character_id == grant.character_id);
            BackendBrowserCharacterAccess {
                owner_key: grant.owner_key,
                character_id: grant.character_id,
                selected,
            }
        })
        .collect()
}

fn effective_age_years_for_view(ctx: &ViewContext, character_id: u64, minute: u64) -> Option<u16> {
    let character = ctx.db.character().id().find(character_id)?;
    let Some(birth) = ctx.db.character_birth().character_id().find(character_id) else {
        return Some(character.age_years);
    };
    let elapsed = i128::from(minute).saturating_sub(i128::from(birth.birth_minute));
    Some(
        (elapsed.max(0) as u128 / u128::from(adventuresim_core::strategic_time::MINUTES_PER_YEAR))
            .min(u128::from(u16::MAX)) as u16,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grant_provenance_has_exactly_one_typed_arm() {
        let mut grant = BrowserCharacterGrant {
            character_id: 1,
            character_scan_id: 1,
            owner_key: "a".repeat(64),
            origin: BrowserCharacterGrantOrigin::StartingCandidate,
            starting_claim_request_key: Some("claim".into()),
            lineage_source_parent_id: None,
            recruitment_source_id: None,
            granted_micros: 0,
        };
        assert!(valid_grant_provenance(&grant));
        grant.lineage_source_parent_id = Some(2);
        assert!(!valid_grant_provenance(&grant));
        grant.origin = BrowserCharacterGrantOrigin::AdultDescendant;
        grant.starting_claim_request_key = None;
        assert!(valid_grant_provenance(&grant));
        grant.origin = BrowserCharacterGrantOrigin::Recruitment;
        grant.lineage_source_parent_id = None;
        grant.recruitment_source_id = Some("recruit:1".into());
        assert!(valid_grant_provenance(&grant));
    }

    #[test]
    fn selected_observer_frontier_hides_future_adulthood() {
        assert!(!descendant_grant_visible_at(1_000, Some(999), 2_000));
        assert!(descendant_grant_visible_at(1_000, Some(1_000), 2_000));
    }

    #[test]
    fn absent_or_dead_selection_allows_successor_recovery() {
        assert!(descendant_grant_visible_at(1_000, None, 1_000));
        assert!(!descendant_grant_visible_at(1_000, None, 999));
    }
}
