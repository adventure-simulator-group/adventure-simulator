//! SpacetimeDB HTTP client module

mod client;
mod types;

pub(crate) use client::{Result, SpacetimeClient};
pub use types::*;

pub(crate) fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// SpacetimeDB's raw HTTP reducer API represents algebraic `Option<T>` values
/// as sum variants rather than Serde's scalar-or-null representation.
pub(crate) fn sats_option<T: serde::Serialize>(value: Option<T>) -> serde_json::Value {
    match value {
        Some(value) => serde_json::json!({ "some": value }),
        None => serde_json::json!({ "none": [] }),
    }
}

#[cfg(test)]
mod tests {
    use super::{sats_option, sql_string_literal};

    #[test]
    fn sql_string_literals_escape_quotes() {
        assert_eq!(sql_string_literal("St. John's"), "'St. John''s'");
    }

    #[test]
    fn reducer_options_use_spacetimedb_sum_encoding() {
        assert_eq!(
            sats_option(Some("digest")),
            serde_json::json!({ "some": "digest" })
        );
        assert_eq!(sats_option(Some(73_u64)), serde_json::json!({ "some": 73 }));
        assert_eq!(sats_option::<u64>(None), serde_json::json!({ "none": [] }));
    }
}
