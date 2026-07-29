//! Authoritative settlement-scoped fame and infamy.
//!
//! Only immutable action events spread. Aggregate rows and imported receipts
//! are terminal projections, which prevents cyclic roads from amplifying
//! reputation.

use adventuresim_core::reputation::{
    ReputationEdge, ReputationSettlement, apply_delta, contributions,
};
use spacetimedb::{ReducerContext, Table, table};

use crate::{backend_case_battle_authority, battle_participant, settlement, travel_edge};

#[derive(Clone, Debug)]
#[table(accessor = character_settlement_reputation, public)]
pub struct CharacterSettlementReputation {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    #[index(btree)]
    pub settlement_id: String,
    /// Nonnegative centipoints, capped by `REPUTATION_CAP`.
    pub fame: i32,
    /// Nonnegative centipoints, capped by `REPUTATION_CAP`.
    pub infamy: i32,
}

#[derive(Clone, Debug)]
#[table(accessor = reputation_event)]
pub struct ReputationEvent {
    /// Retry-stable source identity, unique for the authoritative action.
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    pub origin_settlement_id: String,
    pub source_kind: String,
    pub source_id: String,
    pub raw_fame: i32,
    pub raw_infamy: i32,
    pub occurred_at_minute: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = reputation_contribution)]
pub struct ReputationContributionReceipt {
    /// `{event_id}:{settlement_id}`; makes each projection idempotent.
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub event_id: String,
    #[index(btree)]
    pub character_id: u64,
    pub settlement_id: String,
    pub fame: i32,
    pub infamy: i32,
    pub distance_m: u64,
}

#[derive(Clone, Debug)]
#[table(accessor = discovered_offense)]
pub struct DiscoveredOffense {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub character_id: u64,
    #[index(btree)]
    pub settlement_id: String,
    pub kind: String,
    pub severity: u8,
    /// Current implemented offenses are fine/arrest eligible, never capital.
    pub execution_eligible: bool,
    pub occurred_at_minute: u64,
    pub settled: bool,
}

#[derive(Clone, Debug)]
#[table(accessor = case_reputation_participant)]
pub struct CaseReputationParticipant {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub case_id: String,
    #[index(btree)]
    pub character_id: u64,
    pub party_id: String,
    pub captured_at_minute: u64,
}

pub fn award_case_resolution(
    ctx: &ReducerContext,
    case_id: &str,
    party_id: &str,
    settlement_id: &str,
    fame: i32,
    minute: u64,
) -> Result<(), String> {
    let mut character_ids = ctx
        .db
        .backend_case_battle_authority()
        .iter()
        .filter(|battle| battle.public_case_id == case_id && battle.party_id == party_id)
        .flat_map(|battle| {
            ctx.db
                .battle_participant()
                .participant_battle_id()
                .filter(&battle.battle_id)
                .map(|participant| participant.character_id)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    character_ids.sort_unstable();
    character_ids.dedup();
    if character_ids.is_empty() {
        character_ids = crate::strategic::living_party_member_ids(ctx, party_id);
    }
    for character_id in character_ids {
        let snapshot_id = format!("{case_id}:{character_id}");
        if ctx
            .db
            .case_reputation_participant()
            .id()
            .find(&snapshot_id)
            .is_none()
        {
            ctx.db
                .case_reputation_participant()
                .insert(CaseReputationParticipant {
                    id: snapshot_id,
                    case_id: case_id.to_owned(),
                    character_id,
                    party_id: party_id.to_owned(),
                    captured_at_minute: minute,
                });
        }
        record_event(
            ctx,
            format!("case-resolution:{case_id}:{character_id}"),
            character_id,
            settlement_id,
            "case_resolution",
            case_id,
            fame,
            0,
            minute,
        )?;
    }
    Ok(())
}

pub fn record_discovered_offense(
    ctx: &ReducerContext,
    id: String,
    character_id: u64,
    settlement_id: &str,
    kind: &str,
    severity: u8,
    occurred_at_minute: u64,
) {
    if ctx.db.discovered_offense().id().find(&id).is_none() {
        ctx.db.discovered_offense().insert(DiscoveredOffense {
            id,
            character_id,
            settlement_id: settlement_id.to_owned(),
            kind: kind.to_owned(),
            severity: severity.clamp(1, 5),
            execution_eligible: false,
            occurred_at_minute,
            settled: false,
        });
    }
}

pub fn aggregate_id(character_id: u64, settlement_id: &str) -> String {
    format!("{character_id}:{settlement_id}")
}

/// Record and immediately project one immutable action event. Repeating an
/// event ID is a successful no-op so reducer retries cannot double-award it.
pub fn record_event(
    ctx: &ReducerContext,
    event_id: String,
    character_id: u64,
    origin_settlement_id: &str,
    source_kind: &str,
    source_id: &str,
    raw_fame: i32,
    raw_infamy: i32,
    occurred_at_minute: u64,
) -> Result<bool, String> {
    if raw_fame < 0 || raw_infamy < 0 {
        return Err("Reputation event deltas must be nonnegative".into());
    }
    if ctx.db.reputation_event().id().find(&event_id).is_some() {
        return Ok(false);
    }
    if ctx
        .db
        .settlement()
        .id()
        .find(&origin_settlement_id.to_owned())
        .is_none()
    {
        return Err("Reputation origin settlement not found".into());
    }
    let settlements = ctx
        .db
        .settlement()
        .iter()
        .map(|value| ReputationSettlement {
            id: value.id,
            node_id: value.source_node_id,
            population_level: value.population_level,
            population_estimate: value.population_estimate,
        })
        .collect::<Vec<_>>();
    let edges = ctx
        .db
        .travel_edge()
        .iter()
        .map(|value| ReputationEdge {
            from: value.from_node_id,
            to: value.to_node_id,
            length_m: value.length_m,
        })
        .collect::<Vec<_>>();
    let projected = contributions(
        origin_settlement_id,
        raw_fame,
        raw_infamy,
        &settlements,
        &edges,
    );
    ctx.db.reputation_event().insert(ReputationEvent {
        id: event_id.clone(),
        character_id,
        origin_settlement_id: origin_settlement_id.to_owned(),
        source_kind: source_kind.to_owned(),
        source_id: source_id.to_owned(),
        raw_fame,
        raw_infamy,
        occurred_at_minute,
    });
    for contribution in projected {
        let receipt_id = format!("{event_id}:{}", contribution.settlement_id);
        if ctx
            .db
            .reputation_contribution()
            .id()
            .find(&receipt_id)
            .is_some()
        {
            continue;
        }
        let aggregate_id = aggregate_id(character_id, &contribution.settlement_id);
        let existing = ctx
            .db
            .character_settlement_reputation()
            .id()
            .find(&aggregate_id);
        let mut aggregate = existing.clone().unwrap_or(CharacterSettlementReputation {
            id: aggregate_id,
            character_id,
            settlement_id: contribution.settlement_id.clone(),
            fame: 0,
            infamy: 0,
        });
        aggregate.fame = apply_delta(aggregate.fame, contribution.fame);
        aggregate.infamy = apply_delta(aggregate.infamy, contribution.infamy);
        if existing.is_some() {
            ctx.db
                .character_settlement_reputation()
                .id()
                .update(aggregate);
        } else {
            ctx.db.character_settlement_reputation().insert(aggregate);
        }
        ctx.db
            .reputation_contribution()
            .insert(ReputationContributionReceipt {
                id: receipt_id,
                event_id: event_id.clone(),
                character_id,
                settlement_id: contribution.settlement_id,
                fame: contribution.fame,
                infamy: contribution.infamy,
                distance_m: contribution.distance_m,
            });
    }
    Ok(true)
}

pub fn local_reputation(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: &str,
) -> (i32, i32) {
    ctx.db
        .character_settlement_reputation()
        .id()
        .find(aggregate_id(character_id, settlement_id))
        .map_or((0, 0), |row| (row.fame, row.infamy))
}

pub fn delete_character_reputation(ctx: &ReducerContext, character_id: u64) {
    for participant in ctx
        .db
        .case_reputation_participant()
        .character_id()
        .filter(character_id)
        .collect::<Vec<_>>()
    {
        ctx.db
            .case_reputation_participant()
            .id()
            .delete(&participant.id);
    }
    for offense in ctx
        .db
        .discovered_offense()
        .character_id()
        .filter(character_id)
        .collect::<Vec<_>>()
    {
        ctx.db.discovered_offense().id().delete(&offense.id);
    }
    for receipt in ctx
        .db
        .reputation_contribution()
        .character_id()
        .filter(character_id)
        .collect::<Vec<_>>()
    {
        ctx.db.reputation_contribution().id().delete(&receipt.id);
    }
    for event in ctx
        .db
        .reputation_event()
        .character_id()
        .filter(character_id)
        .collect::<Vec<_>>()
    {
        ctx.db.reputation_event().id().delete(&event.id);
    }
    for aggregate in ctx
        .db
        .character_settlement_reputation()
        .character_id()
        .filter(character_id)
        .collect::<Vec<_>>()
    {
        ctx.db
            .character_settlement_reputation()
            .id()
            .delete(&aggregate.id);
    }
}
