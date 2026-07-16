//! SpacetimeDB HTTP client module

mod client;
mod types;

pub use client::{Result, SpacetimeClient};
pub use types::*;

pub(crate) fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::sql_string_literal;

    #[test]
    fn sql_string_literals_escape_quotes() {
        assert_eq!(sql_string_literal("St. John's"), "'St. John''s'");
    }
}
