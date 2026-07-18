// Handwritten facade over the generated SpacetimeDB client bindings.
// Regenerate `mod.rs` and its sibling binding files with: just generate-db-client

#[path = "mod.rs"]
mod bindings;

pub use bindings::*;
pub use spacetimedb_sdk;
