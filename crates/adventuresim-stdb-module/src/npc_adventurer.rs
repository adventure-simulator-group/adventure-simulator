//! Persistent NPC adventuring companies.
//!
//! Companies and recruitment remain strategic authority. Automatic quest
//! investigation/intervention was intentionally removed: unresolved hostile
//! cases now escalate and spread through public awareness until players act.

use spacetimedb::table;

#[derive(Clone, Debug)]
#[table(accessor = npc_adventuring_party_authority)]
pub struct NpcAdventuringPartyAuthority {
    #[primary_key]
    pub id: String,
    #[index(btree)]
    pub settlement_id: String,
    pub name: String,
    pub member_resident_character_ids_json: String,
    pub capability: u16,
    pub available_at: u64,
}
