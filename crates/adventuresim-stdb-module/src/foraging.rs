//! Gateway-attested, server-authoritative personal foraging.

use adventuresim_core::{
    foraging::{
        self, ForageEnvironment, ForageLegality, ILLEGAL_FORAGE_INFAMY, LocalTerrainMixture,
    },
    physical_object::CustodyCharacterId,
    prelude::*,
    strategic_action::{
        ActionCoordinates, ActionDefinitionId, ActionEffect, ActionRequestId, ActionTarget,
        AuthoritativeSnapshot, AuthorityBinding, CommitAttempt, PlanProvenance, RequestedDuration,
        SnapshotDigest, SnapshotRevision,
    },
    strategic_place::StrategicPlaceId,
};
use sha2::{Digest, Sha256};
use spacetimedb::{ReducerContext, SpacetimeType, Table, ViewContext, reducer, table, view};

use crate::{
    capability::StrategicEquipment,
    character::{
        character, character_attributes, character_limbs, character_skills, character_stats,
    },
    food::food_lot,
    investigation::{case_site_authority, character_case_site_id, exact_case_site_for_observer},
    strategic::{
        IncidentStatus, party_authority, party_journey_authority, party_journey_route_authority,
        party_member, route_position_at_minute, settlement, strategic_gateway_authority,
        strategic_gateway_authority__view, strategic_incident,
    },
    time::character_time,
};

#[derive(Clone, Debug, Eq, PartialEq, SpacetimeType)]
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

/// Immutable private receipt and replay authority. Rows are request-keyed so
/// later actions never destroy the proof needed to make an old retry inert.
#[derive(Clone, Debug)]
#[table(accessor = forage_attempt_authority)]
pub struct ForageAttemptAuthority {
    #[primary_key]
    pub request_id: String,
    #[index(btree)]
    pub character_id: u64,
    #[index(btree)]
    pub gateway_bucket: u8,
    pub attempt_generation: u64,
    pub authority_input_digest: String,
    pub environment_digest: String,
    pub canonical_place: String,
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
    pub terrain_package_digest: String,
    pub latitude_e7: i32,
    pub longitude_e7: i32,
    pub plains: u16,
    pub forest: u16,
    pub hills: u16,
    pub wetlands: u16,
    pub river_or_wet_ground: bool,
    pub sea_or_coast: bool,
    pub cultivated: bool,
    pub license_violation: bool,
    pub output_inventory_item_ids: Vec<u64>,
    pub output_object_ids: Vec<u64>,
    pub output_food_lot_ids: Vec<u64>,
    pub output_material_revisions: Vec<u64>,
}

/// Per-actor cursor used by the gateway to mint the next independent attempt.
#[derive(Clone, Debug)]
#[table(accessor = forage_attempt_state)]
pub struct ForageAttemptState {
    #[primary_key]
    pub character_id: u64,
    #[index(btree)]
    pub gateway_bucket: u8,
    pub next_generation: u64,
}

/// Private source provenance for every concrete harvested unit.
#[derive(Clone, Debug)]
#[table(accessor = forage_harvest_material)]
pub struct ForageHarvestMaterial {
    #[primary_key]
    pub inventory_item_id: u64,
    pub request_id: String,
    pub actor_character_id: u64,
    pub item_id: String,
    pub material_object_id: u64,
    pub food_lot_id: u64,
    pub material_revision: u64,
    pub canonical_place: String,
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

#[derive(Clone, Debug, SpacetimeType)]
pub struct BackendForageAttemptState {
    pub character_id: u64,
    pub next_generation: u64,
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

#[view(accessor = backend_forage_attempt_states, public)]
pub fn backend_forage_attempt_states(ctx: &ViewContext) -> Vec<BackendForageAttemptState> {
    if !view_is_gateway(ctx) {
        return Vec::new();
    }
    ctx.db
        .forage_attempt_state()
        .gateway_bucket()
        .filter(0u8)
        .map(|state| BackendForageAttemptState {
            character_id: state.character_id,
            next_generation: state.next_generation,
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ForageVicinityAuthority {
    place: StrategicPlaceId,
    context_kind: String,
    context_id: String,
    latitude_e7: i32,
    longitude_e7: i32,
    settlement: bool,
}

fn actor_party_owns_incident_site(
    ctx: &ReducerContext,
    character_id: u64,
    party_id: &str,
    case_site_id: &str,
) -> bool {
    let party_membership_matches = ctx
        .db
        .party_member()
        .party_id()
        .filter(party_id)
        .any(|membership| membership.character_id == character_id);
    let party_site_matches = ctx
        .db
        .party_authority()
        .id()
        .find(party_id.to_owned())
        .and_then(|party| party.current_case_site_id)
        .is_some_and(|site| site.value == case_site_id);
    party_membership_matches
        && party_site_matches
        && ctx
            .db
            .strategic_incident()
            .party_id()
            .filter(party_id)
            .any(|incident| incident.case_site_id.value == case_site_id)
}

fn actor_party_has_pending_incident_at_current_site(
    ctx: &ReducerContext,
    character_id: u64,
) -> bool {
    let Some(actor) = ctx.db.character().id().find(character_id) else {
        return false;
    };
    let (Some(party_id), Some(case_site_id)) = (
        actor.party_id.as_deref(),
        character_case_site_id(ctx, character_id),
    ) else {
        return false;
    };
    actor_party_owns_incident_site(ctx, character_id, party_id, &case_site_id)
        && ctx
            .db
            .strategic_incident()
            .party_id()
            .filter(party_id)
            .any(|incident| {
                incident.status == IncidentStatus::Pending
                    && incident.case_site_id.value == case_site_id
            })
}

fn expected_location(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<ForageVicinityAuthority, String> {
    let actor = ctx
        .db
        .character()
        .id()
        .find(character_id)
        .ok_or("Character not found")?;
    if actor.in_server {
        return Err("Foraging is unavailable during a tactical encounter".into());
    }
    crate::strategic::require_character_no_unresolved_encounter(ctx, character_id)?;
    if let Some(settlement_id) = actor.current_settlement_id.as_deref() {
        let location = ctx
            .db
            .settlement()
            .id()
            .find(settlement_id.to_owned())
            .ok_or("Character settlement not found")?;
        return Ok(ForageVicinityAuthority {
            place: StrategicPlaceId::settlement(settlement_id)
                .map_err(|_| "Settlement has an invalid canonical identity")?,
            context_kind: "settlement".into(),
            context_id: settlement_id.into(),
            latitude_e7: (location.coord_y * 10_000_000.0).round() as i32,
            longitude_e7: (location.coord_x * 10_000_000.0).round() as i32,
            settlement: true,
        });
    }
    if let Some(site_id) = character_case_site_id(ctx, character_id) {
        let exact_investigation_site =
            exact_case_site_for_observer(ctx, character_id, &site_id).map(|(site, _)| site);
        let exact_incident_site = actor.party_id.as_deref().and_then(|party_id| {
            actor_party_owns_incident_site(ctx, character_id, party_id, &site_id)
                .then(|| ctx.db.case_site_authority().id_key().find(site_id.clone()))
                .flatten()
        });
        let site = exact_investigation_site
            .or(exact_incident_site)
            .ok_or("Current case site is not exact for this character")?;
        let place = crate::investigation::canonical_case_site_place(&site_id)
            .ok_or("Case site has an invalid canonical identity")?;
        return Ok(ForageVicinityAuthority {
            place,
            context_kind: "case_site".into(),
            context_id: site_id,
            latitude_e7: site.latitude_e7,
            longitude_e7: site.longitude_e7,
            settlement: false,
        });
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
    let (longitude, latitude) =
        route_position_at_minute(&route, journey.completed_movement_minutes)
            .ok_or("Camp location is unavailable")?;
    Ok(ForageVicinityAuthority {
        place: crate::strategic::current_journey_camp_place(ctx, party_id)?,
        context_kind: "camp".into(),
        context_id: party_id.into(),
        latitude_e7: (latitude * 10_000_000.0).round() as i32,
        longitude_e7: (longitude * 10_000_000.0).round() as i32,
        settlement: false,
    })
}

pub(crate) fn current_strategic_place(
    ctx: &ReducerContext,
    character_id: u64,
) -> Result<StrategicPlaceId, String> {
    Ok(expected_location(ctx, character_id)?.place)
}

fn validate_attestation(
    ctx: &ReducerContext,
    character_id: u64,
    attestation: &ForageEnvironmentAttestation,
) -> Result<(ForageVicinityAuthority, ForageEnvironment), String> {
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
    let vicinity = expected_location(ctx, character_id)?;
    if attestation.context_kind != vicinity.context_kind
        || attestation.context_id != vicinity.context_id
        || attestation.latitude_e7 != vicinity.latitude_e7
        || attestation.longitude_e7 != vicinity.longitude_e7
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
    Ok((
        vicinity.clone(),
        ForageEnvironment {
            terrain,
            river_or_wet_ground: attestation.river_or_wet_ground,
            sea_or_coast: attestation.sea_or_coast,
            cultivated: attestation.cultivated,
            settlement: vicinity.settlement,
            license_violation: false,
        },
    ))
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

fn encode_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn forage_terminal_minute(
    ctx: &ReducerContext,
    character_id: u64,
    current_minute: u64,
    duration: u64,
) -> Result<Option<u64>, String> {
    let injury = crate::surgery::preview_injury_boundary(
        ctx,
        character_id,
        duration,
        crate::surgery::InjuryRecoveryMinutes::NONE,
    )?;
    let (disease_safe, disease_terminal) = crate::disease::preview_disease_terminal_boundary(
        ctx,
        character_id,
        injury.elapsed,
        false,
    )?;
    let safe = injury.elapsed.min(disease_safe);
    if safe < duration || injury.terminal || disease_terminal {
        Ok(Some(current_minute.checked_add(safe).ok_or(
            "Foraging terminal time exceeds the strategic clock",
        )?))
    } else {
        Ok(None)
    }
}

fn environment_digest(
    attestation: &ForageEnvironmentAttestation,
    environment: ForageEnvironment,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    let mut frame = |bytes: &[u8]| {
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    };
    frame(b"forage-private-environment-v1");
    frame(attestation.package_digest.as_bytes());
    frame(attestation.context_kind.as_bytes());
    frame(attestation.context_id.as_bytes());
    frame(&attestation.latitude_e7.to_le_bytes());
    frame(&attestation.longitude_e7.to_le_bytes());
    for value in [
        attestation.plains,
        attestation.forest,
        attestation.hills,
        attestation.wetlands,
    ] {
        frame(&value.to_le_bytes());
    }
    frame(&[
        environment.river_or_wet_ground as u8,
        environment.sea_or_coast as u8,
        environment.cultivated as u8,
        environment.settlement as u8,
        environment.license_violation as u8,
    ]);
    hash.finalize().into()
}

#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
fn forage_authority_digest(
    character_id: u64,
    place: &StrategicPlaceId,
    environment_digest: [u8; 32],
    source_ids: &[String],
    requested_minutes: u64,
    current_minute: u64,
    terminal_minute: Option<u64>,
    attempt_generation: u64,
    terrain_check: u16,
    stealth_check: u16,
    resolution_seed: u64,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    let mut frame = |bytes: &[u8]| {
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    };
    frame(b"forage-plan-authority-v1");
    frame(&character_id.to_le_bytes());
    frame(place.to_string().as_bytes());
    frame(&environment_digest);
    for source in source_ids {
        frame(source.as_bytes());
    }
    frame(&requested_minutes.to_le_bytes());
    frame(&current_minute.to_le_bytes());
    frame(&terminal_minute.unwrap_or(u64::MAX).to_le_bytes());
    frame(&attempt_generation.to_le_bytes());
    frame(&terrain_check.to_le_bytes());
    frame(&stealth_check.to_le_bytes());
    frame(&resolution_seed.to_le_bytes());
    hash.finalize().into()
}

fn license_decisions(
    ctx: &ReducerContext,
    actor: CustodyCharacterId,
    source_ids: &[String],
    evidence_revision: u64,
) -> Result<Vec<foraging::ForageLicenseDecision>, String> {
    source_ids
        .iter()
        .map(|id| {
            let source = foraging::ForageSource::from_id(id).ok_or("Unknown forage source")?;
            let question = foraging::forage_license_question(actor, source)
                .map_err(|_| "Foraging rights question is inconsistent")?;
            let allowed = source_privilege(source).is_none_or(|privilege| {
                crate::organization::global_presented_privilege(ctx, actor.get(), privilege)
            });
            let decision = foraging::decide_forage_license(&question, allowed, evidence_revision);
            foraging::ForageLicenseDecision::try_new(&question, decision).map_err(str::to_owned)
        })
        .collect()
}

#[expect(
    clippy::too_many_arguments,
    reason = "this domain boundary names each independent input explicitly"
)]
fn build_forage_planner(
    ctx: &ReducerContext,
    character_id: u64,
    request_id: &str,
    vicinity: &ForageVicinityAuthority,
    attestation: &ForageEnvironmentAttestation,
    environment: ForageEnvironment,
    source_ids: &[String],
    requested_minutes: u64,
    current_minute: u64,
    terminal_minute: Option<u64>,
    attempt_generation: u64,
    terrain_check: u16,
    stealth_check: u16,
    seed: u64,
) -> Result<foraging::ForagePlanningOutcome, String> {
    let actor = CustodyCharacterId::try_new(character_id).map_err(|error| error.to_string())?;
    let coordinates = ActionCoordinates::try_new(
        actor,
        ActionTarget::Place(vicinity.place.clone()),
        vicinity.place.clone(),
        None,
        Vec::new(),
    )
    .map_err(|_| "Foraging coordinates are inconsistent")?;
    let duration = RequestedDuration::try_new(requested_minutes)
        .map_err(|_| "Foraging duration must be positive")?;
    let time = adventuresim_core::strategic_action::resolve_time(
        current_minute,
        duration,
        &adventuresim_core::strategic_action::TimeBoundaries::<foraging::ForagePlanInterruption> {
            terminal_minute,
            interruption: None,
        },
    );
    // Full resolution validates source availability. Partial terminal attempts
    // retain only their exposure check; zero-time attempts resolve nothing.
    let full = foraging::resolve(
        seed,
        environment,
        source_ids,
        requested_minutes,
        terrain_check,
        stealth_check,
    )
    .map_err(str::to_owned)?;
    let resolution = if time.elapsed_minutes == 0 {
        None
    } else if time.permits_completion_effects() {
        Some(full)
    } else {
        let (stealth_dc_millirank, stealth_succeeded) =
            foraging::resolve_stealth(seed, environment, time.elapsed_minutes, stealth_check);
        Some(foraging::ForageResolution {
            yields: Vec::new(),
            stealth_dc_millirank,
            stealth_succeeded,
        })
    };
    let env_digest = environment_digest(attestation, environment);
    let digest = forage_authority_digest(
        character_id,
        &vicinity.place,
        env_digest,
        source_ids,
        requested_minutes,
        current_minute,
        terminal_minute,
        attempt_generation,
        terrain_check,
        stealth_check,
        seed,
    );
    let decisions = license_decisions(ctx, actor, source_ids, current_minute)?;
    Ok(foraging::build_forage_plan(foraging::ForagePlanAuthority {
        coordinates,
        provenance: PlanProvenance {
            request_id: ActionRequestId::try_new(request_id)
                .map_err(|_| "Forage request is malformed")?,
            action_id: ActionDefinitionId::try_new("forage:current-vicinity")
                .map_err(|_| "Forage definition is malformed")?,
            input_digest: SnapshotDigest(digest),
            authority_binding: AuthorityBinding(digest),
        },
        snapshot: AuthoritativeSnapshot {
            revision: SnapshotRevision(attempt_generation),
            digest: SnapshotDigest(digest),
        },
        current_minute,
        duration,
        terminal_minute,
        exact_presence: true,
        encounter_clear: true,
        environment_current: true,
        capability_current: true,
        sources_available: true,
        license_decisions: decisions,
        local_restriction: environment.settlement || environment.cultivated,
        legality: if environment.settlement
            || environment.cultivated
            || environment.license_violation
        {
            ForageLegality::IllegalAttempt
        } else {
            ForageLegality::Legal
        },
        source_ids: source_ids.to_vec(),
        resolution,
    }))
}

fn replay_matches(
    receipt: &ForageAttemptAuthority,
    character_id: u64,
    source_ids: &[String],
    requested_minutes: u64,
    attempt_generation: u64,
    attestation: &ForageEnvironmentAttestation,
) -> bool {
    receipt.character_id == character_id
        && receipt.source_ids == source_ids
        && receipt.requested_minutes == requested_minutes
        && receipt.attempt_generation == attempt_generation
        && receipt.context_kind == attestation.context_kind
        && receipt.context_id == attestation.context_id
        && receipt.terrain_package_digest == attestation.package_digest
        && receipt.latitude_e7 == attestation.latitude_e7
        && receipt.longitude_e7 == attestation.longitude_e7
        && receipt.plains == attestation.plains
        && receipt.forest == attestation.forest
        && receipt.hills == attestation.hills
        && receipt.wetlands == attestation.wetlands
        && receipt.river_or_wet_ground == attestation.river_or_wet_ground
        && receipt.sea_or_coast == attestation.sea_or_coast
        && receipt.cultivated == attestation.cultivated
}

#[reducer]
pub fn forage_current_vicinity(
    ctx: &ReducerContext,
    character_id: u64,
    request_id: String,
    source_ids: Vec<String>,
    requested_minutes: u64,
    attempt_generation: u64,
    attestation: ForageEnvironmentAttestation,
) -> Result<(), String> {
    // Authentication must precede even replay lookup: otherwise a caller can
    // use receipt/readiness differences as a character-state oracle.
    crate::strategic::require_strategic_gateway(ctx)?;
    if !valid_request_id(&request_id) {
        return Err("Forage request id is invalid".into());
    }
    foraging::validate_duration(requested_minutes).map_err(str::to_owned)?;
    let source_ids = canonical_sources(source_ids)?;
    // Exact immutable retry is resolved before any mutable character, place,
    // condition, or ecology authority is consulted.
    if let Some(receipt) = ctx
        .db
        .forage_attempt_authority()
        .request_id()
        .find(&request_id)
    {
        return if replay_matches(
            &receipt,
            character_id,
            &source_ids,
            requested_minutes,
            attempt_generation,
            &attestation,
        ) {
            Ok(())
        } else {
            Err("Forage request id collides with a different attempt".into())
        };
    }

    if actor_party_has_pending_incident_at_current_site(ctx, character_id) {
        return Err("Foraging is unavailable during a pending strategic incident".into());
    }

    crate::condition::require_character_ready(ctx, character_id)?;
    let expected_generation = ctx
        .db
        .forage_attempt_state()
        .character_id()
        .find(character_id)
        .map_or(0, |state| state.next_generation);
    if attempt_generation != expected_generation {
        return Err("Forage attempt generation is stale".into());
    }
    let next_attempt_generation = attempt_generation
        .checked_add(1)
        .ok_or("Forage attempt generation is exhausted")?;
    let (vicinity, mut environment) = validate_attestation(ctx, character_id, &attestation)?;
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
    let terminal_minute = forage_terminal_minute(ctx, character_id, started_at, requested_minutes)?;
    let planned = match build_forage_planner(
        ctx,
        character_id,
        &request_id,
        &vicinity,
        &attestation,
        environment,
        &source_ids,
        requested_minutes,
        started_at,
        terminal_minute,
        attempt_generation,
        terrain_check,
        stealth_check,
        seed,
    )? {
        adventuresim_core::strategic_action::PlanningOutcome::Ready(plan) => plan,
        adventuresim_core::strategic_action::PlanningOutcome::Rejected(_) => {
            return Err("Foraging is unavailable".into());
        }
    };
    // Rehydrate and replan before mutation; plans are evidence, not authority.
    let (fresh_vicinity, mut fresh_environment) =
        validate_attestation(ctx, character_id, &attestation)?;
    fresh_environment.license_violation = license_violation_for_sources(&source_ids, |privilege| {
        crate::organization::global_presented_privilege(ctx, character_id, privilege)
    });
    let (fresh_terrain_check, fresh_stealth_check) =
        acting_checks(ctx, character_id, fresh_environment.terrain)?;
    let fresh_terminal = forage_terminal_minute(ctx, character_id, started_at, requested_minutes)?;
    let replanned = build_forage_planner(
        ctx,
        character_id,
        &request_id,
        &fresh_vicinity,
        &attestation,
        fresh_environment,
        &source_ids,
        requested_minutes,
        started_at,
        fresh_terminal,
        attempt_generation,
        fresh_terrain_check,
        fresh_stealth_check,
        seed,
    )?;
    let fresh_snapshot = match &replanned {
        adventuresim_core::strategic_action::PlanningOutcome::Ready(plan) => plan.snapshot(),
        adventuresim_core::strategic_action::PlanningOutcome::Rejected(_) => {
            return Err("Foraging prerequisites changed before commit".into());
        }
    };
    let provenance = planned.provenance();
    adventuresim_core::strategic_action::validate_commit(
        &planned,
        &replanned,
        fresh_snapshot,
        &CommitAttempt {
            request_id: provenance.request_id.clone(),
            action_id: provenance.action_id.clone(),
            authority_binding: provenance.authority_binding,
        },
        None,
    )
    .map_err(|_| "Foraging authority changed before commit")?;

    let mut effect_duration = None;
    let mut effect_resolution = None;
    for effect in planned.effects() {
        match effect {
            ActionEffect::Domain(foraging::ForagePlanEffect::AttemptSearch {
                actor,
                requested_minutes,
            }) => effect_duration = Some((actor.get(), *requested_minutes)),
            ActionEffect::Domain(foraging::ForagePlanEffect::CommitResolution {
                resolution,
                permits_yield,
            }) => effect_resolution = Some((resolution.clone(), *permits_yield)),
            _ => return Err("Foraging planner emitted an unsupported effect".into()),
        }
    }
    let (effect_actor, effect_minutes) =
        effect_duration.ok_or("Foraging planner omitted its search effect")?;
    if effect_actor != character_id || effect_minutes != requested_minutes {
        return Err("Foraging planner effects do not match authority".into());
    }
    let completed = crate::time::advance_investigation_time(ctx, character_id, requested_minutes)?;
    let completed_at = ctx
        .db
        .character_time()
        .character_id()
        .find(character_id)
        .ok_or("Character time record not found after foraging")?
        .minutes;
    let elapsed = completed_at.saturating_sub(started_at);
    let interrupted = !planned.time().permits_completion_effects();
    if elapsed != planned.time().elapsed_minutes
        || completed != planned.time().permits_completion_effects()
        || effect_resolution
            .as_ref()
            .is_some_and(|(_, permits_yield)| *permits_yield != !interrupted)
    {
        return Err("Foraging time diverged from its authoritative plan".into());
    }
    // Time effects are allowed to change physiology. Place, environmental
    // authority, source rights, and capability inputs must still be identical
    // before material/consequence effects are committed.
    let (post_vicinity, mut post_environment) =
        validate_attestation(ctx, character_id, &attestation)?;
    post_environment.license_violation = license_violation_for_sources(&source_ids, |privilege| {
        crate::organization::global_presented_privilege(ctx, character_id, privilege)
    });
    let (post_terrain_check, post_stealth_check) =
        acting_checks(ctx, character_id, post_environment.terrain)?;
    if post_vicinity != vicinity
        || post_environment != environment
        || post_terrain_check != terrain_check
        || post_stealth_check != stealth_check
    {
        return Err("Foraging authority changed during the search".into());
    }
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
    let resolution = effect_resolution.map(|(resolution, _)| resolution);
    let mut yielded_item_ids = Vec::new();
    let mut yielded_quantities = Vec::new();
    let mut output_inventory_item_ids = Vec::new();
    let mut output_object_ids = Vec::new();
    let mut output_food_lot_ids = Vec::new();
    let mut output_material_revisions = Vec::new();
    if let Some(resolution) = resolution.as_ref() {
        for found in &resolution.yields {
            let rows = crate::item::add_foraged_inventory_item_checked_rows(
                ctx,
                character_id,
                found.item_id,
                u32::from(found.quantity),
            )?;
            for row_id in rows {
                let object = crate::inventory_container::object_for_row(
                    ctx,
                    adventuresim_core::physical_object::CarriedInventoryScope::Personal,
                    row_id,
                )?
                .ok_or("Foraged material has no stable object identity")?;
                let lot = ctx
                    .db
                    .food_lot()
                    .iter()
                    .find(|lot| lot.inventory_item_id == Some(row_id))
                    .ok_or("Foraged material has no food lot")?;
                if lot.material_revision != 1
                    || lot.ingredient_item_ids != vec![found.item_id.to_string()]
                    || lot.ingredient_quantities != vec![1.0]
                {
                    return Err("Foraged material provenance is inconsistent".into());
                }
                ctx.db
                    .forage_harvest_material()
                    .insert(ForageHarvestMaterial {
                        inventory_item_id: row_id,
                        request_id: request_id.clone(),
                        actor_character_id: character_id,
                        item_id: found.item_id.into(),
                        material_object_id: object.id,
                        food_lot_id: lot.id,
                        material_revision: lot.material_revision,
                        canonical_place: vicinity.place.to_string(),
                    });
                output_inventory_item_ids.push(row_id);
                output_object_ids.push(object.id);
                output_food_lot_ids.push(lot.id);
                output_material_revisions.push(lot.material_revision);
            }
            yielded_item_ids.push(found.item_id.into());
            yielded_quantities.push(found.quantity);
        }
    }
    let infamy_gained = if resolution
        .as_ref()
        .and_then(|result| result.stealth_succeeded)
        .is_some_and(|success| !success)
    {
        ILLEGAL_FORAGE_INFAMY
    } else {
        0.0
    };
    if infamy_gained > 0.0
        && let Some(settlement_id) = ctx
            .db
            .character()
            .id()
            .find(character_id)
            .and_then(|character| character.current_settlement_id)
    {
        crate::world_event::commit_noticed_illegal_foraging(
            ctx,
            character_id,
            &settlement_id,
            &request_id,
            (infamy_gained * 100.0).round() as i32,
            completed_at,
        )?;
    }
    let attempt = ForageAttemptAuthority {
        request_id: request_id.clone(),
        character_id,
        gateway_bucket: 0,
        attempt_generation,
        authority_input_digest: encode_digest(&planned.provenance().input_digest.0),
        environment_digest: encode_digest(&environment_digest(&attestation, environment)),
        canonical_place: vicinity.place.to_string(),
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
        terrain_package_digest: attestation.package_digest,
        latitude_e7: attestation.latitude_e7,
        longitude_e7: attestation.longitude_e7,
        plains: attestation.plains,
        forest: attestation.forest,
        hills: attestation.hills,
        wetlands: attestation.wetlands,
        river_or_wet_ground: attestation.river_or_wet_ground,
        sea_or_coast: attestation.sea_or_coast,
        cultivated: attestation.cultivated,
        license_violation: environment.license_violation,
        output_inventory_item_ids,
        output_object_ids,
        output_food_lot_ids,
        output_material_revisions,
    };
    ctx.db.forage_attempt_authority().insert(attempt);
    let state = ForageAttemptState {
        character_id,
        gateway_bucket: 0,
        next_generation: next_attempt_generation,
    };
    if ctx
        .db
        .forage_attempt_state()
        .character_id()
        .find(character_id)
        .is_some()
    {
        ctx.db.forage_attempt_state().character_id().update(state);
    } else {
        ctx.db.forage_attempt_state().insert(state);
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

    #[test]
    fn gateway_and_immutable_replay_precede_live_character_reads() {
        let source = crate::production_source(include_str!("foraging.rs"));
        let reducer = source
            .split("pub fn forage_current_vicinity")
            .nth(1)
            .unwrap()
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let gateway = reducer.find("require_strategic_gateway(ctx)").unwrap();
        let replay = reducer
            .find("forage_attempt_authority()\n        .request_id()")
            .unwrap();
        let readiness = reducer
            .find("require_character_ready(ctx, character_id)")
            .unwrap();
        assert!(gateway < replay && replay < readiness);
        assert!(reducer.contains("ctx.db.forage_attempt_authority().insert(attempt)"));
        assert!(!reducer.contains(
            "forage_attempt_authority()\n            .character_id()\n            .update"
        ));
    }

    #[test]
    fn planner_revalidation_and_material_receipts_are_mandatory() {
        let source = crate::production_source(include_str!("foraging.rs"));
        let reducer = source
            .split("pub fn forage_current_vicinity")
            .nth(1)
            .unwrap()
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(reducer.matches("build_forage_planner(").count() >= 2);
        assert!(reducer.contains("validate_commit("));
        assert!(reducer.contains("post_vicinity != vicinity"));
        assert!(reducer.contains("ForagePlanEffect::CommitResolution"));
        assert!(reducer.contains(".forage_harvest_material()"));
        assert!(reducer.contains(".insert(ForageHarvestMaterial {"));
        assert!(reducer.contains("lot.material_revision != 1"));
        assert!(reducer.contains("lot.ingredient_quantities != vec![1.0]"));
    }

    #[test]
    fn planner_uses_committing_time_policy_and_checked_attempt_generation() {
        let source = crate::production_source(include_str!("foraging.rs"));
        let preview = source
            .split("fn forage_terminal_minute")
            .nth(1)
            .and_then(|tail| tail.split("fn environment_digest").next())
            .expect("foraging terminal preview");
        assert!(preview.contains("preview_injury_boundary("));
        assert!(preview.contains("InjuryRecoveryMinutes::NONE"));
        assert!(preview.contains("preview_disease_terminal_boundary("));
        assert!(preview.contains("injury.elapsed"));
        assert!(preview.contains("current_minute.checked_add(safe)"));

        let reducer = source
            .split("pub fn forage_current_vicinity")
            .nth(1)
            .unwrap()
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(reducer.contains("attempt_generation\n        .checked_add(1)"));
        assert!(!reducer.contains("attempt_generation.saturating_add(1)"));
    }

    #[test]
    fn public_receipt_omits_private_environment_entropy_and_material_ids() {
        let source = crate::production_source(include_str!("foraging.rs"));
        let projection = source
            .split("pub struct BackendForageReceipt")
            .nth(1)
            .unwrap()
            .split('}')
            .next()
            .unwrap();
        for private in [
            "resolution_seed",
            "environment_digest",
            "canonical_place",
            "latitude_e7",
            "stealth_dc_millirank",
            "output_object_ids",
            "material_revision",
        ] {
            assert!(!projection.contains(private), "leaked {private}");
        }
    }

    #[test]
    fn incident_site_provenance_survives_resolution_but_remains_exact() {
        let source = crate::production_source(include_str!("foraging.rs"));
        let authority = source
            .split("fn actor_party_owns_incident_site")
            .nth(1)
            .and_then(|tail| {
                tail.split("fn actor_party_has_pending_incident_at_current_site")
                    .next()
            })
            .expect("incident site provenance authority");
        for exact_boundary in [
            ".party_member()",
            "membership.character_id == character_id",
            ".party_authority()",
            "party.current_case_site_id",
            ".strategic_incident()",
            "incident.case_site_id.value == case_site_id",
        ] {
            assert!(authority.contains(exact_boundary), "{exact_boundary}");
        }
        assert!(!authority.contains("IncidentStatus::Pending"));

        let location = source
            .split("fn expected_location")
            .nth(1)
            .and_then(|tail| tail.split("pub(crate) fn current_strategic_place").next())
            .expect("shared expected location resolver");
        let investigation = location.find("exact_case_site_for_observer").unwrap();
        let incident = location.find("actor_party_owns_incident_site").unwrap();
        let authority_site = location
            .find("case_site_authority().id_key().find")
            .unwrap();
        assert!(investigation < incident && incident < authority_site);
        assert!(location.contains("actor.party_id.as_deref()"));
        assert!(location.contains(".or(exact_incident_site)"));
        assert!(!location.contains("starts_with(\"case-site:incident:"));
        assert!(!location.contains("split(\"incident:"));
    }

    #[test]
    fn fresh_forage_rejects_pending_exact_incident_after_immutable_replay() {
        let source = crate::production_source(include_str!("foraging.rs"));
        let pending = source
            .split("fn actor_party_has_pending_incident_at_current_site")
            .nth(1)
            .and_then(|tail| tail.split("fn expected_location").next())
            .expect("pending incident forage gate");
        assert!(pending.contains("actor_party_owns_incident_site"));
        assert!(pending.contains("incident.status == IncidentStatus::Pending"));
        assert!(pending.contains("incident.case_site_id.value == case_site_id"));

        let reducer = source
            .split("pub fn forage_current_vicinity")
            .nth(1)
            .and_then(|tail| tail.split("#[cfg(test)]").next())
            .expect("forage reducer");
        let replay = reducer.find("forage_attempt_authority()").unwrap();
        let pending_gate = reducer
            .find("actor_party_has_pending_incident_at_current_site")
            .unwrap();
        let readiness = reducer.find("require_character_ready").unwrap();
        assert!(replay < pending_gate && pending_gate < readiness);
        assert!(reducer.contains("Foraging is unavailable during a pending strategic incident"));
    }
}
