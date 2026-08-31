use spacetimedb::{ReducerContext, SpacetimeType};

use crate::strategic::settlement;

#[derive(Clone, Debug, PartialEq, Eq, SpacetimeType)]
pub struct TacticalSettlementSnapshot {
    pub id: String,
    pub population_level: i32,
    pub population_estimate: u32,
}

pub(crate) fn tactical_settlement_snapshot(
    ctx: &ReducerContext,
    origin_settlement_id: &str,
    scene_key: &str,
) -> Option<TacticalSettlementSnapshot> {
    let settlement = ctx
        .db
        .settlement()
        .id()
        .find(origin_settlement_id.to_owned())?;
    (settlement.scene_key == scene_key).then(|| TacticalSettlementSnapshot {
        id: settlement.id,
        population_level: settlement.population_level,
        population_estimate: settlement.population_estimate,
    })
}

pub(crate) fn tactical_party_roster(
    ctx: &ReducerContext,
    party_id: &str,
) -> Result<(Vec<u64>, u32), String> {
    let members = crate::strategic::living_party_member_ids(ctx, party_id);
    let count =
        u32::try_from(members.len()).map_err(|_| "Party is too large for tactical enrollment")?;
    if count == 0 {
        return Err("A tactical mission requires at least one living party member".into());
    }
    if count as usize > adventuresim_core::mission::MAX_TACTICAL_RECEIPT_PARTICIPANTS {
        return Err("Party exceeds the tactical receipt participant limit".into());
    }
    Ok((members, count))
}
