//! Opt-in reducer-backed core-loop simulation.
//!
//! Storage, policy, failure projection, travel, settlement, expedition,
//! generated-case, cycle, and bootstrap behavior are kept in focused
//! source units while retaining the established crate-root API.

#[cfg(test)]
pub(crate) const LIVE_CORE_SOURCE: &str = concat!(
    include_str!("model.rs"),
    include_str!("policy.rs"),
    include_str!("survival.rs"),
    include_str!("failure.rs"),
    include_str!("travel.rs"),
    include_str!("settlement.rs"),
    include_str!("expedition.rs"),
    include_str!("generated_cases.rs"),
    include_str!("cycle.rs"),
    include_str!("bootstrap.rs"),
    include_str!("tests.rs"),
);

include!("model.rs");
include!("policy.rs");
include!("survival.rs");

mod failure {
    use super::*;
    include!("failure.rs");
}

mod travel {
    use super::*;
    include!("travel.rs");
}

mod settlement {
    use super::*;
    include!("settlement.rs");
}

mod expedition {
    use super::*;
    include!("expedition.rs");
}

mod generated_cases {
    use super::*;
    include!("generated_cases.rs");
}

mod cycle {
    use super::*;
    include!("cycle.rs");
}

mod bootstrap {
    use super::*;
    include!("bootstrap.rs");
}

pub use bootstrap::run_core_loop;
#[cfg(test)]
use bootstrap::select_public_quest_fixture;
use bootstrap::{equipment_utility, leader_is_actionable, root_requirement_matches_slot};
include!("tests.rs");
