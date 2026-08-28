use super::{SqlQuery, sql_string_literal};

/// Builds the canonical query for one settlement's primary-key row.
pub(crate) fn settlement_by_id(id: &str) -> SqlQuery {
    SqlQuery::new(format!(
        "SELECT * FROM settlement WHERE id = {}",
        sql_string_literal(id)
    ))
}

/// Builds the canonical query for one party's primary-key row.
pub(crate) fn party_by_id(id: &str) -> SqlQuery {
    SqlQuery::new(format!(
        "SELECT * FROM party WHERE id = {}",
        sql_string_literal(id)
    ))
}

#[cfg(test)]
mod tests {
    use super::{party_by_id, settlement_by_id};

    #[test]
    fn primary_key_queries_escape_ids() {
        assert_eq!(
            settlement_by_id("St. John's").as_str(),
            "SELECT * FROM settlement WHERE id = 'St. John''s'"
        );
        assert_eq!(
            party_by_id("pilgrims' guild").as_str(),
            "SELECT * FROM party WHERE id = 'pilgrims'' guild'"
        );
    }
}
