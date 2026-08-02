//! Persistent strategic authority, organized by gameplay domain.

//! Implementation is partitioned by behavior domain below. The fragments
//! intentionally share this module scope because SpacetimeDB macro discovery
//! and generated accessor names are scope-sensitive. Non-macro services use
//! ordinary child modules elsewhere; these ordered files preserve the exact
//! reducer/view/table ABI while keeping each gameplay domain navigable.

#[cfg(test)]
pub(crate) const STRATEGIC_SOURCE: &str = concat!(
    include_str!("autoresolve.rs"),
    include_str!("tests/combat_party.rs"),
    include_str!("tests/authority_trade_dialogue.rs"),
    include_str!("tests/generated_world.rs"),
    include_str!("party_readiness.rs"),
    include_str!("world_import.rs"),
    include_str!("authority_model.rs"),
    include_str!("dialogue_schema.rs"),
    include_str!("dialogue_sessions.rs"),
    include_str!("dialogue_provenance.rs"),
    include_str!("dialogue_bindings.rs"),
    include_str!("dialogue_prompts.rs"),
    include_str!("dialogue_effects.rs"),
    include_str!("governance.rs"),
    include_str!("inventory_trade.rs"),
    include_str!("contracts.rs"),
    include_str!("travel_planning.rs"),
    include_str!("incidents.rs"),
    include_str!("journey_camp.rs"),
    include_str!("encounters.rs"),
    include_str!("travel_tests.rs"),
    include_str!("travel_reducers.rs"),
    include_str!("custody_objectives.rs"),
    include_str!("systemic_interactions.rs"),
    include_str!("mission_bootstrap.rs"),
    include_str!("challenges.rs"),
);

include!("autoresolve.rs");
include!("party_readiness.rs");
include!("world_import.rs");
include!("authority_model.rs");
include!("dialogue_schema.rs");
include!("dialogue_sessions.rs");
include!("dialogue_provenance.rs");
include!("dialogue_bindings.rs");
include!("dialogue_prompts.rs");
include!("dialogue_effects.rs");
include!("governance.rs");
include!("inventory_trade.rs");
include!("contracts.rs");
include!("travel_planning.rs");
include!("incidents.rs");
include!("journey_camp.rs");
include!("encounters.rs");
include!("travel_tests.rs");
include!("travel_reducers.rs");
include!("custody_objectives.rs");
include!("systemic_interactions.rs");
include!("mission_bootstrap.rs");
include!("challenges.rs");
include!("tests.rs");
