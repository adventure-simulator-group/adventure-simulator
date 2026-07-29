//! Gateway-attested, server-authoritative personal foraging.

use adventuresim_core::{
    foraging::{self, ForageEnvironment, ILLEGAL_FORAGE_INFAMY, LocalTerrainMixture},
    prelude::*,
};
use sha2::{Digest, Sha256};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::{
    capability::StrategicEquipment,
    character::{
        character, character_attributes, character_limbs, character_skills, character_stats,
    },
    investigation::{character_case_site_id, exact_case_site_for_observer},
    strategic::{
        party_authority, party_journey_authority, party_journey_route_authority,
        route_position_at_minute, settlement, strategic_gateway_authority,
        strategic_gateway_authority__view,
    },
    time::character_time,
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
    pub wetlands: u16,
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
    pub source_ids: Vec<String>,
    pub yielded_item_ids: Vec<String>,
    pub yielded_quantities: Vec<u16>,
    pub interrupted: bool,
    pub illegal: bool,
    pub stealth_dc_millirank: Option<u16>,
    pub stealth_succeeded: Option<bool>,
    pub infamy_gained: f32,
    pub context_kind: String,
    pub context_id: String,
    pub latitude_e7: i32,
    pub longitude_e7: i32,
    pub cultivated: bool,
}

/// Player-safe result projection. Exact location, context, roll/DC, private
/// entropy, and authoritative reputation mutation are intentionally omitted.
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

fn canonical_sources(mut sources: Vec<String>) -> Result<Vec<String>, String> {
    sources.sort();
    if sources.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("Forage sources must be unique".into());
    }
    if sources
        .iter()
        .any(|id| foraging::ForageSource::from_id(id).is_none())
    {
        Err("Unknown forage source".into())
    } else {
        Ok(sources)
    }
}

fn source_privilege(
    source: foraging::ForageSource,
) -> Option<adventuresim_core::organization::Privilege> {
    use adventuresim_core::organization::Privilege;
    Some(match source {
        foraging::ForageSource::HighGame => Privilege::ForageHighGame,
        foraging::ForageSource::LowGame => Privilege::ForageLowGame,
        foraging::ForageSource::Fish => Privilege::ForageFish,
        foraging::ForageSource::Plants => Privilege::ForagePlants,
        foraging::ForageSource::HarmfulBeasts => return None,
    })
}

fn license_violation_for_sources(
    source_ids: &[String],
    mut has_privilege: impl FnMut(adventuresim_core::organization::Privilege) -> bool,
) -> bool {
    source_ids.iter().any(|id| {
        let Some(source) = foraging::ForageSource::from_id(id) else {
            return true;
        };
        source_privilege(source).is_some_and(|privilege| !has_privilege(privilege))
    })
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
    if gateway.terrain_schema != 3
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
        wetlands: attestation.wetlands,
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
        license_violation: false,
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
    let stats = ctx
        .db
        .character_stats()
        .character_id()
        .find(character_id)
        .ok_or("Character stats not found")?;
    let equipment = StrategicEquipment::load(ctx, character_id);
    let check = |skill| {
        skills.skill_check_by_parts(
            skill,
            &attributes,
            &limbs,
            &stats,
            &equipment,
            LimbWeights::all_equal(),
        )
    };
    let terrain_training = check(Skill::TerrainPlains) * f32::from(mixture.plains) / 1_000.0
        + check(Skill::TerrainForest) * f32::from(mixture.forest) / 1_000.0
        + check(Skill::TerrainHills) * f32::from(mixture.hills) / 1_000.0
        + check(Skill::TerrainWetlands) * f32::from(mixture.wetlands) / 1_000.0;
    let stealth_training = check(Skill::Stealth);
    Ok((
        (terrain_training.clamp(0.0, 5.0) * 1_000.0).round() as u16,
        (stealth_training.clamp(0.0, 5.0) * 1_000.0).round() as u16,
    ))
}

fn resolution_seed(
    private_entropy: u64,
    character_id: u64,
    started_at: u64,
    attestation: &ForageEnvironmentAttestation,
    sources: &[String],
) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(b"forage-resolution-v2");
    hasher.update(private_entropy.to_le_bytes());
    hasher.update(character_id.to_le_bytes());
    hasher.update(started_at.to_le_bytes());
    hasher.update(attestation.latitude_e7.to_le_bytes());
    hasher.update(attestation.longitude_e7.to_le_bytes());
    for source in sources {
        hasher.update((source.len() as u64).to_le_bytes());
        hasher.update(source.as_bytes());
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
    source_ids: Vec<String>,
    requested_minutes: u64,
    attestation: ForageEnvironmentAttestation,
) -> Result<(), String> {
    crate::condition::require_character_ready(ctx, character_id)?;
    if !valid_request_id(&request_id) {
        return Err("Forage request id is invalid".into());
    }
    let mut environment = validate_attestation(ctx, character_id, &attestation)?;
    foraging::validate_duration(requested_minutes).map_err(str::to_owned)?;
    let source_ids = canonical_sources(source_ids)?;
    environment.license_violation = license_violation_for_sources(&source_ids, |privilege| {
        crate::organization::global_presented_privilege(ctx, character_id, privilege)
    });
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
        &source_ids,
    );
    let planned = foraging::resolve(
        seed,
        environment,
        &source_ids,
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
        let attributes = ctx
            .db
            .character_attributes()
            .character_id()
            .find(character_id)
            .ok_or("Character attributes not found")?;
        let mut excess = 0.0;
        for (stored, skill, real_hours) in [
            (
                &mut skills.terrain_plains_hours,
                Skill::TerrainPlains,
                gains[0],
            ),
            (
                &mut skills.terrain_forest_hours,
                Skill::TerrainForest,
                gains[1],
            ),
            (
                &mut skills.terrain_hills_hours,
                Skill::TerrainHills,
                gains[2],
            ),
            (
                &mut skills.terrain_wetlands_hours,
                Skill::TerrainWetlands,
                gains[3],
            ),
        ] {
            excess += adventuresim_core::skill::apply_direct_training(
                skill,
                stored,
                real_hours,
                &attributes,
            )
            .excess_effective_hours;
        }
        ctx.db.character_skills().character_id().update(skills);
        crate::condition::record_mastery_training_morale(ctx, character_id, elapsed, excess);
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
    let infamy_gained = resolution
        .as_ref()
        .and_then(|result| result.stealth_succeeded)
        .is_some_and(|success| !success)
        .then_some(ILLEGAL_FORAGE_INFAMY)
        .unwrap_or(0.0);
    if infamy_gained > 0.0 {
        if let Some(settlement_id) = ctx
            .db
            .character()
            .id()
            .find(character_id)
            .and_then(|character| character.current_settlement_id)
        {
            crate::reputation::record_event(
                ctx,
                format!("forage:{character_id}:{request_id}"),
                character_id,
                &settlement_id,
                "illegal_foraging",
                &request_id,
                0,
                (infamy_gained * 100.0).round() as i32,
                completed_at,
            )?;
            crate::reputation::record_discovered_offense(
                ctx,
                format!("offense:forage:{character_id}:{request_id}"),
                character_id,
                &settlement_id,
                "illegal_foraging",
                1,
                completed_at,
            );
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
        source_ids,
        yielded_item_ids,
        yielded_quantities,
        interrupted,
        illegal: environment.settlement || environment.cultivated || environment.license_violation,
        stealth_dc_millirank: resolution.as_ref().and_then(|row| row.stealth_dc_millirank),
        stealth_succeeded: resolution.as_ref().and_then(|row| row.stealth_succeeded),
        infamy_gained,
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
    fn canonical_sources_make_permutations_identical_and_reject_bad_input() {
        let first = canonical_sources(vec!["plants".into(), "fish".into()]).unwrap();
        let second = canonical_sources(vec!["fish".into(), "plants".into()]).unwrap();
        assert_eq!(first, second);
        assert!(canonical_sources(vec!["fish".into(), "fish".into()]).is_err());
        assert!(canonical_sources(vec!["raw_fish".into()]).is_err());
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

    #[test]
    fn source_privileges_cover_licensed_categories_and_exempt_harmful_beasts() {
        use adventuresim_core::organization::Privilege;
        assert_eq!(
            source_privilege(foraging::ForageSource::HighGame),
            Some(Privilege::ForageHighGame)
        );
        assert_eq!(
            source_privilege(foraging::ForageSource::LowGame),
            Some(Privilege::ForageLowGame)
        );
        assert_eq!(
            source_privilege(foraging::ForageSource::Fish),
            Some(Privilege::ForageFish)
        );
        assert_eq!(
            source_privilege(foraging::ForageSource::Plants),
            Some(Privilege::ForagePlants)
        );
        assert_eq!(
            source_privilege(foraging::ForageSource::HarmfulBeasts),
            None
        );
        assert!(!license_violation_for_sources(
            &["harmful_beasts".into()],
            |_| false
        ));
        assert!(!license_violation_for_sources(
            &["plants".into(), "low_game".into()],
            |_| true
        ));
        assert!(license_violation_for_sources(
            &["harmful_beasts".into(), "plants".into()],
            |_| false
        ));
    }
}
