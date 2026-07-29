//! Authoritative settlement-scoped fame and infamy.
//!
//! Only immutable action events spread. Aggregate rows are terminal
//! projections, which prevents cyclic roads from amplifying reputation.

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

/// Immutable charge snapshot for one arrest. A later offense requires a later
/// arrest and cannot be silently folded into an existing fine.
#[derive(Clone, Debug)]
#[table(accessor = authority_arrest_charge)]
pub struct AuthorityArrestCharge {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub incident_id: String,
    #[index(btree)]
    pub character_id: u64,
    pub settlement_id: String,
    pub offense_id: String,
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
    canonical_case_id: &str,
    public_case_id: &str,
    party_id: &str,
    settlement_id: &str,
    fame: i32,
    minute: u64,
) -> Result<(), String> {
    let mut character_ids = ctx
        .db
        .backend_case_battle_authority()
        .iter()
        .filter(|battle| battle.public_case_id == public_case_id && battle.party_id == party_id)
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
        let snapshot_id = format!("{canonical_case_id}:{character_id}");
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
                    case_id: canonical_case_id.to_owned(),
                    character_id,
                    party_id: party_id.to_owned(),
                    captured_at_minute: minute,
                });
        }
        record_event(
            ctx,
            format!("case-resolution:{canonical_case_id}:{character_id}"),
            character_id,
            settlement_id,
            "case_resolution",
            canonical_case_id,
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

pub fn unsettled_local_offenses(
    ctx: &ReducerContext,
    character_id: u64,
    settlement_id: &str,
) -> Vec<DiscoveredOffense> {
    let mut offenses = ctx
        .db
        .discovered_offense()
        .character_id()
        .filter(character_id)
        .filter(|offense| offense.settlement_id == settlement_id && !offense.settled)
        .collect::<Vec<_>>();
    offenses.sort_by(|left, right| {
        (left.occurred_at_minute, &left.id).cmp(&(right.occurred_at_minute, &right.id))
    });
    offenses
}

pub fn snapshot_arrest_charges(
    ctx: &ReducerContext,
    incident_id: &str,
    character_id: u64,
    settlement_id: &str,
) -> usize {
    let offenses = unsettled_local_offenses(ctx, character_id, settlement_id);
    for offense in &offenses {
        let id = format!("{incident_id}:{}", offense.id);
        if ctx.db.authority_arrest_charge().id().find(&id).is_none() {
            ctx.db
                .authority_arrest_charge()
                .insert(AuthorityArrestCharge {
                    id,
                    incident_id: incident_id.to_owned(),
                    character_id,
                    settlement_id: settlement_id.to_owned(),
                    offense_id: offense.id.clone(),
                });
        }
    }
    offenses.len()
}

pub fn unsettled_arrest_charges(
    ctx: &ReducerContext,
    incident_id: &str,
    character_id: u64,
    settlement_id: &str,
) -> Vec<DiscoveredOffense> {
    let mut offenses = ctx
        .db
        .authority_arrest_charge()
        .incident_id()
        .filter(incident_id)
        .filter(|charge| {
            charge.character_id == character_id && charge.settlement_id == settlement_id
        })
        .filter_map(|charge| ctx.db.discovered_offense().id().find(&charge.offense_id))
        .filter(|offense| {
            offense.character_id == character_id
                && offense.settlement_id == settlement_id
                && !offense.settled
        })
        .collect::<Vec<_>>();
    offenses.sort_by(|left, right| left.id.cmp(&right.id));
    offenses
}

pub fn settle_offenses(ctx: &ReducerContext, offenses: Vec<DiscoveredOffense>) {
    for mut offense in offenses {
        offense.settled = true;
        ctx.db.discovered_offense().id().update(offense);
    }
}

pub fn aggregate_id(character_id: u64, settlement_id: &str) -> String {
    format!("{character_id}:{settlement_id}")
}

/// Record and immediately project one immutable action event. Repeating an
/// event ID is a successful no-op so reducer retries cannot double-award it.
/// SpacetimeDB reducers commit the event and every aggregate mutation in one
/// transaction, so per-destination idempotency rows are unnecessary.
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
    for charge in ctx
        .db
        .authority_arrest_charge()
        .character_id()
        .filter(character_id)
        .collect::<Vec<_>>()
    {
        ctx.db.authority_arrest_charge().id().delete(&charge.id);
    }
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

#[cfg(test)]
mod tests {
    #[test]
    fn case_battles_use_public_identity_but_events_keep_canonical_identity() {
        let source = include_str!("reputation.rs");
        let award = source
            .split("pub fn award_case_resolution")
            .nth(1)
            .and_then(|tail| tail.split("pub fn record_discovered_offense").next())
            .expect("case reputation award");
        assert!(award.contains("battle.public_case_id == public_case_id"));
        assert!(award.contains("case-resolution:{canonical_case_id}:{character_id}"));
        assert!(award.contains("case_id: canonical_case_id.to_owned()"));
    }

    #[test]
    fn event_is_the_only_projection_idempotency_marker() {
        let source = include_str!("reputation.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(!source.contains("ReputationContributionReceipt"));
        assert!(!source.contains("reputation_contribution()"));
        assert!(source.contains("reputation_event().id().find(&event_id)"));
    }
}
