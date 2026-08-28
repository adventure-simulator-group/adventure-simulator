//! SpacetimeDB HTTP client module

mod client;
mod queries;
mod types;

pub(crate) use client::{Result, SpacetimeClient};
pub(crate) use queries::{party_by_id, settlement_by_id};
pub use types::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SqlQuery(String);

impl SqlQuery {
    fn new(query: String) -> Self {
        Self(query)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for SqlQuery {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

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

/// Supplies the exact schema name for a unit variant at the raw SATS boundary.
pub(crate) trait SatsUnitVariant {
    fn sats_name(self) -> &'static str;
}

impl SatsUnitVariant for adventuresim_core::physiology::BodyRegion {
    fn sats_name(self) -> &'static str {
        use adventuresim_core::physiology::BodyRegion;

        match self {
            BodyRegion::LeftArm => "leftArm",
            BodyRegion::RightArm => "rightArm",
            BodyRegion::LeftLeg => "leftLeg",
            BodyRegion::RightLeg => "rightLeg",
            BodyRegion::Chest => "chest",
            BodyRegion::Abdomen => "abdomen",
            BodyRegion::Head => "head",
        }
    }
}

impl SatsUnitVariant for adventuresim_core::physiology::InterventionRoute {
    fn sats_name(self) -> &'static str {
        use adventuresim_core::physiology::InterventionRoute;

        match self {
            InterventionRoute::Oral => "oral",
            InterventionRoute::Topical => "topical",
            InterventionRoute::Inhaled => "inhaled",
            InterventionRoute::Injected => "injected",
        }
    }
}

impl SatsUnitVariant for adventuresim_core::surgery::SurgeryProcedure {
    fn sats_name(self) -> &'static str {
        use adventuresim_core::surgery::SurgeryProcedure;

        match self {
            SurgeryProcedure::Bandage => "bandage",
            SurgeryProcedure::Stitch => "stitch",
            SurgeryProcedure::Splint => "splint",
            SurgeryProcedure::RemoveSplint => "removeSplint",
            SurgeryProcedure::Extract => "extract",
            SurgeryProcedure::OpenBody => "openBody",
        }
    }
}

/// SpacetimeDB's raw HTTP reducer API represents unit enum variants as a
/// single-key sum object. Domain types own the exact schema-name mapping above.
pub(crate) fn sats_unit_variant(variant: impl SatsUnitVariant) -> serde_json::Value {
    serde_json::json!({ (variant.sats_name()): {} })
}

#[cfg(test)]
mod tests {
    use super::{sats_option, sats_unit_variant, sql_string_literal};

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

    #[test]
    fn reducer_unit_variants_use_spacetimedb_sum_encoding() {
        assert_eq!(
            sats_unit_variant(adventuresim_core::physiology::BodyRegion::LeftArm),
            serde_json::json!({ "leftArm": {} })
        );
        assert_eq!(
            sats_unit_variant(adventuresim_core::physiology::InterventionRoute::Oral),
            serde_json::json!({ "oral": {} })
        );
        assert_eq!(
            sats_unit_variant(adventuresim_core::surgery::SurgeryProcedure::RemoveSplint),
            serde_json::json!({ "removeSplint": {} })
        );
        assert_eq!(
            sats_unit_variant(adventuresim_core::surgery::SurgeryProcedure::OpenBody),
            serde_json::json!({ "openBody": {} })
        );
    }
}
