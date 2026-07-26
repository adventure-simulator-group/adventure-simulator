//! Gateway-attested, server-authoritative personal foraging.

use adventuresim_core::{
    foraging::{self, ForageEnvironment, ILLEGAL_FORAGE_VIRTUE_LOSS, LocalTerrainMixture},
    prelude::Skill,
};
use sha2::{Digest, Sha256};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::{
    character::{character, character_attributes, character_limbs, character_skills},
    investigation::{character_case_site_id, exact_case_site_for_observer},
    strategic::{
        party_authority, party_journey_authority, party_journey_route_authority,
        route_position_at_minute, settlement, strategic_gateway_authority,
        strategic_gateway_authority__view,
    },
    time::{CharacterVirtue, character_time, character_virtue},
};

#[derive(Clone, Debug, SpacetimeType)]
pub struct ForageEnvironmentAttestation {
    pub package_digest: String,
    pub latitude_e7: i32,
    pub longitude_e7: i32,
    pub context_kind: String,
    pub context_id: String,
    pub plains: u16,
    pub forest: u16,
    pub hills: u16,
    pub river_or_wet_ground: bool,
    pub sea_or_coast: bool,
    pub cultivated: bool,
}

/// Private, bounded replay authority. There is at most one row per character.
#[derive(Clone, Debug)]
#[table(accessor = forage_attempt_authority)]
pub struct ForageAttemptAuthority {
    #[primary_key]
    pub character_id: u64,
    #[index(btree)]
    pub gateway_bucket: u8,
    pub request_id: String,
    pub resolution_seed: u64,
    pub started_at: u64,
    pub completed_at: u64,
    pub requested_minutes: u64,
    pub elapsed_minutes: u64,
    pub target_item_ids: Vec<String>,
    pub yielded_item_ids: Vec<String>,
    pub yielded_quantities: Vec<u16>,
    pub interrupted: bool,
    pub illegal: bool,
    pub stealth_dc_millirank: Option<u16>,
    pub stealth_succeeded: Option<bool>,
    pub virtue_lost: f32,
    pub context_kind: String,
    pub context_id: String,
    pub latitude_e7: i32,
    pub longitude_e7: i32,
    pub cultivated: bool,
}

/// Player-safe result projection. Exact location, context, roll/DC, private
/// entropy, and authoritative virtue mutation are intentionally omitted.
#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendForageReceipt {
    pub character_id: u64,
    pub request_id: String,
    pub elapsed_minutes: u64,
    pub yielded_item_ids: Vec<String>,
    pub yielded_quantities: Vec<u16>,
    pub interrupted: bool,
    pub legal_outcome: String,
}

fn view_is_gateway(ctx: &ViewContext) -> bool {
    ctx.db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .is_some_and(|row| row.identity == ctx.sender())
}

fn safe_legal_outcome(illegal: bool, stealth_succeeded: Option<bool>) -> &'static str {
    if !illegal {
        "legal"
    } else if stealth_succeeded == Some(true) {
        "unnoticed"
    } else {
        "noticed"
    }
}

fn valid_request_id(request_id: &str) -> bool {
    request_id.len() == 64
        && request_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn canonical_targets(mut targets: Vec<String>) -> Result<Vec<String>, String> {
    targets.sort();
    if targets.windows(2).any(|pair| pair[0] == pair[1]) {
        Err("Forage targets must be unique".into())
    } else {
        Ok(targets)
    }
}

#[view(accessor = backend_forage_receipts, public)]
pub fn backend_forage_receipts(ctx: &ViewContext) -> Vec<BackendForageReceipt> {
    if !view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .forage_attempt_authority()
        .gateway_bucket()
        .filter(0u8)
        .map(|attempt| BackendForageReceipt {
            character_id: attempt.character_id,
            request_id: attempt.request_id,
            elapsed_minutes: attempt.elapsed_minutes,
            yielded_item_ids: attempt.yielded_item_ids,
            yielded_quantities: attempt.yielded_quantities,
            interrupted: attempt.interrupted,
            legal_outcome: safe_legal_outcome(attempt.illegal, attempt.stealth_succeeded).into(),
        })
        .collect()
}

fn expected_location(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<(String, String, i32, i32, bool), String> {
    let actor = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if actor.in_server {
        return Err("Foraging is unavailable during a tactical encounter".into());
    }
    if let Some(settlement_id) = actor.current_settlement_id.as_deref() {
        let location = ctx
            .db
            .settlement()
            .id()
            .find(settlement_id.to_owned())
            .ok_or("Character settlement not found")?;
        return Ok((
            "settlement".into(),
            settlement_id.into(),
            (location.coord_y * 10_000_000.0).round() as i32,
            (location.coord_x * 10_000_000.0).round() as i32,
            true,
        ));
    }
    if let Some(site_id) = character_case_site_id(ctx, character_id) {
        let (site, _) = exact_case_site_for_observer(ctx, character_id, &site_id)
            .ok_or("Current case site is not exact for this character")?;
        return Ok((
            "case_site".into(),
            site_id,
            site.latitude_e7,
            site.longitude_e7,
            false,
        ));
    }
    let party_id = actor
        .party_id
        .as_deref()
        .ok_or("Character has no stationary vicinity")?;
    let party = ctx
        .db
        .party_authority()
        .id()
        .find(party_id.to_owned())
        .ok_or("Party not found")?;
    if party.current_settlement_id.is_some()
        || party.current_case_site_id.is_some()
        || party.camp_destination.is_none()
    {
        return Err(
            "Foraging requires a stationary settlement, case site, or en-route camp".into(),
        );
    }
    crate::strategic::require_character_no_unresolved_encounter(ctx, character_id)?;
    let journey = ctx
        .db
        .party_journey_authority()
        .party_id()
        .find(party_id.to_owned())
        .ok_or("Camp journey not found")?;
    let route = ctx
        .db
        .party_journey_route_authority()
        .party_id()
        .find(party_id.to_owned())
        .ok_or("Camp terrain route not found")?;
    let (longitude, latitude) = route_position_at_minute(&route, journey.completed_minutes)
        .ok_or("Camp location is unavailable")?;
    Ok((
        "camp".into(),
        party_id.into(),
        (latitude * 10_000_000.0).round() as i32,
        (longitude * 10_000_000.0).round() as i32,
        false,
    ))
}

fn validate_attestation(
    ctx: &ReducerContext,
    character_id: u64,
    attestation: &ForageEnvironmentAttestation,
) -> Result<ForageEnvironment, String> {
    crate::strategic::require_strategic_gateway(ctx)?;
    let gateway = ctx
        .db
        .strategic_gateway_authority()
        .id()
        .find(0)
        .ok_or("Strategic gateway is not registered")?;
    if gateway.terrain_schema != 2
        || gateway.terrain_package_digest.as_deref() != Some(&attestation.package_digest)
    {
        return Err("Forage environment uses a stale terrain package".into());
    }
    let (kind, id, latitude_e7, longitude_e7, settlement) = expected_location(ctx, character_id)?;
    if attestation.context_kind != kind
        || attestation.context_id != id
        || attestation.latitude_e7 != latitude_e7
        || attestation.longitude_e7 != longitude_e7
    {
        return Err("Forage environment does not match the authoritative location".into());
    }
    let terrain = LocalTerrainMixture {
        plains: attestation.plains,
        forest: attestation.forest,
        hills: attestation.hills,
    };
    if !terrain.is_normalized() {
        return Err("Forage environment terrain mixture is invalid".into());
    }
    Ok(ForageEnvironment {
        terrain,
        river_or_wet_ground: attestation.river_or_wet_ground,
        sea_or_coast: attestation.sea_or_coast,
        cultivated: attestation.cultivated,
        settlement,
    })
}

fn acting_checks(
    ctx: &ReducerContext,
    character_id: u64,
    mixture: LocalTerrainMixture,
) -> Result<(u16, u16), String> {
    let attributes = ctx
        .db
        .character_attributes()
        .character_id()
        .find(character_id)
        .ok_or("Character attributes not found")?;
    let limbs = ctx
        .db
        .character_limbs()
        .character_id()
        .find(character_id)
        .ok_or("Character limbs not found")?;
    let skills = ctx
        .db
        .character_skills()
        .character_id()
        .find(character_id)
        .ok_or("Character skills not found")?;
    let head = limbs.head_health.clamp(0.0, 1.0);
    let mental = ((attributes.instinct + attributes.intelligence) * 0.5 * head).clamp(0.0, 5.0);
    let terrain_training = Skill::TerrainPlains.training_rank(skills.terrain_plains_hours)
        * f32::from(mixture.plains)
        / 1_000.0
        + Skill::TerrainForest.training_rank(skills.terrain_forest_hours)
            * f32::from(mixture.forest)
            / 1_000.0
        + Skill::TerrainHills.training_rank(skills.terrain_hills_hours) * f32::from(mixture.hills)
            / 1_000.0;
    let arms = ((limbs.left_arm_health + limbs.right_arm_health) * 0.5).clamp(0.0, 1.0);
    let agility =
        ((attributes.left_arm_agility + attributes.right_arm_agility) * 0.5 * arms).clamp(0.0, 5.0);
    let stealth_training = Skill::Stealth.training_rank(skills.stealth_hours);
    Ok((
        (((terrain_training + mental) * 0.5).clamp(0.0, 5.0) * 1_000.0).round() as u16,
        (((stealth_training + agility) * 0.5).clamp(0.0, 5.0) * 1_000.0).round() as u16,
    ))
}

fn resolution_seed(
    private_entropy: u64,
    character_id: u64,
    started_at: u64,
    attestation: &ForageEnvironmentAttestation,
    targets: &[String],
) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(b"forage-resolution-v2");
    hasher.update(private_entropy.to_le_bytes());
    hasher.update(character_id.to_le_bytes());
    hasher.update(started_at.to_le_bytes());
    hasher.update(attestation.latitude_e7.to_le_bytes());
    hasher.update(attestation.longitude_e7.to_le_bytes());
    for target in targets {
        hasher.update((target.len() as u64).to_le_bytes());
        hasher.update(target.as_bytes());
    }
    u64::from_le_bytes(
        hasher.finalize()[..8]
            .try_into()
            .expect("eight digest bytes"),
    )
}

#[reducer]
pub fn forage_current_vicinity(
    ctx: &ReducerContext,
    character_id: u64,
    request_id: String,
    target_item_ids: Vec<String>,
    requested_minutes: u64,
    attestation: ForageEnvironmentAttestation,
) -> Result<(), String> {
    crate::condition::require_character_ready(ctx, character_id)?;
    if !valid_request_id(&request_id) {
        return Err("Forage request id is invalid".into());
    }
    let environment = validate_attestation(ctx, character_id, &attestation)?;
    foraging::validate_duration(requested_minutes).map_err(str::to_owned)?;
    let target_item_ids = canonical_targets(target_item_ids)?;
    let (terrain_check, stealth_check) = acting_checks(ctx, character_id, environment.terrain)?;
    crate::time::initialize_character_time(ctx, character_id)?;
    let started_at = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or("Character time record not found")?
        .minutes;
    let seed = resolution_seed(
        ctx.random::<u64>(),
        character_id,
        started_at,
        &attestation,
        &target_item_ids,
    );
    let planned = foraging::resolve(
        seed,
        environment,
        &target_item_ids,
        requested_minutes,
        terrain_check,
        stealth_check,
    )
    .map_err(str::to_owned)?;
    let completed = crate::time::advance_investigation_time(ctx, character_id, requested_minutes)?;
    let completed_at = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or("Character time record not found after foraging")?
        .minutes;
    let elapsed = completed_at.saturating_sub(started_at);
    if elapsed > 0
        && let Some(mut skills) = ctx.db.character_skills().character_id().find(character_id)
    {
        let gains = foraging::training_hours(environment.terrain, elapsed);
        skills.terrain_plains_hours =
            (skills.terrain_plains_hours + gains[0]).min(Skill::TerrainPlains.max_hours());
        skills.terrain_forest_hours =
            (skills.terrain_forest_hours + gains[1]).min(Skill::TerrainForest.max_hours());
        skills.terrain_hills_hours =
            (skills.terrain_hills_hours + gains[2]).min(Skill::TerrainHills.max_hours());
        ctx.db.character_skills().character_id().update(skills);
    }
    let interrupted = !completed || elapsed < requested_minutes;
    let mut resolution = (!interrupted).then_some(planned);
    if interrupted && elapsed > 0 {
        let (stealth_dc_millirank, stealth_succeeded) =
            foraging::resolve_stealth(seed, environment, elapsed, stealth_check);
        resolution = Some(foraging::ForageResolution {
            yields: Vec::new(),
            stealth_dc_millirank,
            stealth_succeeded,
        });
    }
    let mut yielded_item_ids = Vec::new();
    let mut yielded_quantities = Vec::new();
    if let Some(resolution) = resolution.as_ref() {
        for found in &resolution.yields {
            crate::item::add_inventory_item_checked(
                ctx,
                character_id,
                found.item_id,
                u32::from(found.quantity),
            )?;
            yielded_item_ids.push(found.item_id.into());
            yielded_quantities.push(found.quantity);
        }
    }
    let virtue_lost = resolution
        .as_ref()
        .and_then(|result| result.stealth_succeeded)
        .is_some_and(|success| !success)
        .then_some(ILLEGAL_FORAGE_VIRTUE_LOSS)
        .unwrap_or(0.0);
    if virtue_lost > 0.0 {
        let existing = ctx.db.character_virtue().character_id().find(character_id);
        let mut virtue = existing.clone().unwrap_or(CharacterVirtue {
            character_id,
            value: 0.0,
        });
        virtue.value -= virtue_lost;
        if existing.is_some() {
            ctx.db.character_virtue().character_id().update(virtue);
        } else {
            ctx.db.character_virtue().insert(virtue);
        }
    }
    let attempt = ForageAttemptAuthority {
        character_id,
        gateway_bucket: 0,
        request_id,
        resolution_seed: seed,
        started_at,
        completed_at,
        requested_minutes,
        elapsed_minutes: elapsed,
        target_item_ids,
        yielded_item_ids,
        yielded_quantities,
        interrupted,
        illegal: environment.settlement || environment.cultivated,
        stealth_dc_millirank: resolution.as_ref().and_then(|row| row.stealth_dc_millirank),
        stealth_succeeded: resolution.as_ref().and_then(|row| row.stealth_succeeded),
        virtue_lost,
        context_kind: attestation.context_kind,
        context_id: attestation.context_id,
        latitude_e7: attestation.latitude_e7,
        longitude_e7: attestation.longitude_e7,
        cultivated: attestation.cultivated,
    };
    if ctx
        .db
        .forage_attempt_authority()
        .character_id()
        .find(character_id)
        .is_some()
    {
        ctx.db
            .forage_attempt_authority()
            .character_id()
            .update(attempt);
    } else {
        ctx.db.forage_attempt_authority().insert(attempt);
    }
    crate::capability::refresh_character_capability(ctx, character_id)?;
    crate::condition::refresh_character_strategic_condition(ctx, character_id)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_targets_make_permutations_identical_and_reject_duplicates() {
        let first = canonical_targets(vec!["sage".into(), "berries".into()]).unwrap();
        let second = canonical_targets(vec!["berries".into(), "sage".into()]).unwrap();
        assert_eq!(first, second);
        assert!(canonical_targets(vec!["sage".into(), "sage".into()]).is_err());
    }

    #[test]
    fn receipt_projection_uses_only_safe_legality_wording() {
        assert_eq!(safe_legal_outcome(false, None), "legal");
        assert_eq!(safe_legal_outcome(true, Some(true)), "unnoticed");
        assert_eq!(safe_legal_outcome(true, Some(false)), "noticed");
    }

    #[test]
    fn request_ids_are_fixed_lowercase_hex() {
        assert!(valid_request_id(&"a".repeat(64)));
        assert!(!valid_request_id(&"A".repeat(64)));
        assert!(!valid_request_id(&"a".repeat(63)));
    }
}
