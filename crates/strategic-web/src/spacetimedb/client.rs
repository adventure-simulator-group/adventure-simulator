//! SpacetimeDB HTTP client wrapper

use reqwest::Client;
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
use spacetimedb_sats::{de::DeserializeOwned as SatsDeserializeOwned, serde::SerdeWrapper};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use super::types::{AlgebraicType, QueryResponse};

fn sum_variants(algebraic_type: &AlgebraicType) -> Option<&Vec<Value>> {
    let AlgebraicType::Value(ty) = algebraic_type;
    ty.get("Sum")?.get("variants")?.as_array()
}

fn variant_name(variant: &Value) -> Option<&str> {
    variant.get("name")?.get("some")?.as_str()
}

fn is_option_sum(variants: &[Value]) -> bool {
    if variants.len() != 2 {
        return false;
    }

    let names: Vec<_> = variants
        .iter()
        .filter_map(variant_name)
        .map(|s| s.to_ascii_lowercase())
        .collect();

    names.iter().any(|n| n == "some") && names.iter().any(|n| n == "none")
}

fn product_elements(algebraic_type: &AlgebraicType) -> Option<&Vec<Value>> {
    let AlgebraicType::Value(ty) = algebraic_type;
    ty.get("Product")?.get("elements")?.as_array()
}

fn array_element_type(algebraic_type: &AlgebraicType) -> Option<&Value> {
    let AlgebraicType::Value(ty) = algebraic_type;
    ty.get("Array")
}

fn serde_variant_name(variant: &Value) -> String {
    let name = variant_name(variant).unwrap_or("Unknown");
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "Unknown".into(),
    }
}

fn is_identity_product(elements: &[Value]) -> bool {
    elements.len() == 1
        && elements[0]
            .get("name")
            .and_then(|name| name.get("some"))
            .and_then(Value::as_str)
            == Some("__identity__")
}

fn malformed_sats_value(message: impl Into<String>) -> SpacetimeError {
    SpacetimeError::Spacetime(format!(
        "malformed SpacetimeDB SQL value: {}",
        message.into()
    ))
}

/// Convert the SQL endpoint's positional wire values to the human-readable
/// representation accepted by SATS' serde bridge. Unlike the presentation
/// conversion above, sums remain explicit one-key objects so their tags never
/// depend on serde enum conventions.
fn convert_spacetime_value_sats(value: &Value, algebraic_type: &AlgebraicType) -> Result<Value> {
    if let Some(element_type) = array_element_type(algebraic_type) {
        let values = value
            .as_array()
            .ok_or_else(|| malformed_sats_value("expected an array"))?;
        let element_type = AlgebraicType::Value(element_type.clone());
        return values
            .iter()
            .map(|value| convert_spacetime_value_sats(value, &element_type))
            .collect::<Result<Vec<_>>>()
            .map(Value::Array);
    }

    if let Some(elements) = product_elements(algebraic_type) {
        let values = value
            .as_array()
            .ok_or_else(|| malformed_sats_value("expected a product array"))?;
        if values.len() != elements.len() {
            return Err(malformed_sats_value(format!(
                "product expected {} fields but received {}",
                elements.len(),
                values.len()
            )));
        }

        if is_identity_product(elements) {
            let raw = values
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| malformed_sats_value("identity was not a hexadecimal string"))?;
            let digits = raw.strip_prefix("0x").unwrap_or(raw);
            if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(malformed_sats_value(
                    "identity contained invalid hexadecimal digits",
                ));
            }
            let significant = digits.trim_start_matches('0');
            let quantity = if significant.is_empty() {
                "0x0".to_owned()
            } else {
                format!("0x{significant}")
            };
            return Ok(Value::Array(vec![Value::String(quantity)]));
        }

        if elements.is_empty() {
            return Ok(Value::Array(Vec::new()));
        }

        if elements.iter().all(|element| {
            element
                .get("name")
                .and_then(|name| name.get("some"))
                .and_then(Value::as_str)
                .is_some()
        }) {
            let mut object = serde_json::Map::new();
            for (element, value) in elements.iter().zip(values) {
                let name = element
                    .get("name")
                    .and_then(|name| name.get("some"))
                    .and_then(Value::as_str)
                    .expect("all product elements were checked as named");
                let nested_type = AlgebraicType::Value(
                    element
                        .get("algebraic_type")
                        .cloned()
                        .unwrap_or(Value::Null),
                );
                object.insert(
                    name.to_owned(),
                    convert_spacetime_value_sats(value, &nested_type)?,
                );
            }
            return Ok(Value::Object(object));
        }

        let values = elements
            .iter()
            .zip(values)
            .map(|(element, value)| {
                let nested_type = AlgebraicType::Value(
                    element
                        .get("algebraic_type")
                        .cloned()
                        .unwrap_or(Value::Null),
                );
                convert_spacetime_value_sats(value, &nested_type)
            })
            .collect::<Result<Vec<_>>>()?;
        return Ok(Value::Array(values));
    }

    let Some(variants) = sum_variants(algebraic_type) else {
        return Ok(value.clone());
    };
    let encoded = value
        .as_array()
        .ok_or_else(|| malformed_sats_value("expected a tagged sum array"))?;
    if encoded.len() != 2 {
        return Err(malformed_sats_value(format!(
            "sum expected a tag and payload but received {} values",
            encoded.len()
        )));
    }
    let tag = encoded[0]
        .as_u64()
        .ok_or_else(|| malformed_sats_value("sum tag was not an unsigned integer"))?;
    let variant = variants
        .get(tag as usize)
        .ok_or_else(|| malformed_sats_value(format!("unknown sum tag {tag}")))?;
    let name = if is_option_sum(variants) {
        variant_name(variant)
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| malformed_sats_value("option variant had no name"))?
    } else {
        serde_variant_name(variant)
    };
    let payload_type = AlgebraicType::Value(
        variant
            .get("algebraic_type")
            .cloned()
            .unwrap_or(Value::Null),
    );
    let payload = convert_spacetime_value_sats(&encoded[1], &payload_type)?;
    Ok(Value::Object([(name, payload)].into_iter().collect()))
}

fn decode_sats_query_response<T: SatsDeserializeOwned>(
    query_response: &QueryResponse,
) -> Result<Vec<T>> {
    let Some(first) = query_response.first() else {
        return Ok(Vec::new());
    };
    if first
        .schema
        .elements
        .iter()
        .any(|element| element.name.is_none())
    {
        return Err(malformed_sats_value(
            "generated-row query returned an unnamed column",
        ));
    }

    first
        .rows
        .iter()
        .map(|row| {
            let values = row
                .as_array()
                .ok_or_else(|| malformed_sats_value("SpacetimeDB returned a non-array SQL row"))?;
            if values.len() != first.schema.elements.len() {
                return Err(malformed_sats_value(format!(
                    "row expected {} columns but received {}",
                    first.schema.elements.len(),
                    values.len()
                )));
            }
            let mut object = serde_json::Map::new();
            for (element, value) in first.schema.elements.iter().zip(values) {
                let name = element
                    .name
                    .as_ref()
                    .expect("all SQL columns were checked as named");
                object.insert(
                    name.some.clone(),
                    convert_spacetime_value_sats(value, &element.algebraic_type)?,
                );
            }
            serde_json::from_value::<SerdeWrapper<T>>(Value::Object(object))
                .map(|wrapped| wrapped.0)
                .map_err(Into::into)
        })
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum SpacetimeError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("SpacetimeDB error: {0}")]
    Spacetime(String),
}

impl SpacetimeError {
    pub(crate) fn reducer_code(
        &self,
    ) -> Option<adventuresim_core::reducer_error::ReducerErrorCode> {
        let Self::Spacetime(message) = self else {
            return None;
        };
        adventuresim_core::reducer_error::parse_reducer_error(message)
    }
}

pub type Result<T> = std::result::Result<T, SpacetimeError>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QueryMetricsSnapshot {
    pub requests: u64,
    pub elapsed_micros: u64,
}

impl QueryMetricsSnapshot {
    pub fn delta(self, before: Self) -> Self {
        Self {
            requests: self.requests.saturating_sub(before.requests),
            elapsed_micros: self.elapsed_micros.saturating_sub(before.elapsed_micros),
        }
    }
}

#[derive(Default)]
struct QueryMetrics {
    requests: AtomicU64,
    elapsed_micros: AtomicU64,
}

/// HTTP client for SpacetimeDB
#[derive(Clone)]
pub struct SpacetimeClient {
    http: Client,
    base_url: String,
    database: String,
    token: Option<String>,
    metrics: Arc<QueryMetrics>,
}

impl SpacetimeClient {
    /// Create a new SpacetimeDB client
    pub fn new(base_url: impl Into<String>, database: impl Into<String>) -> Result<Self> {
        Ok(Self {
            http: Client::builder()
                .timeout(Duration::from_secs(10))
                .connect_timeout(Duration::from_secs(3))
                .build()?,
            base_url: base_url.into(),
            database: database.into(),
            token: None,
            metrics: Arc::new(QueryMetrics::default()),
        })
    }

    /// Set the auth token
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token;
        self
    }

    /// Return monotonic SQL counters. Take a snapshot before and after one
    /// controlled request, then call `after.delta(before)`. This is safe for
    /// concurrent requests; it deliberately does not destructively reset a
    /// process-global counter.
    pub fn query_metrics(&self) -> QueryMetricsSnapshot {
        QueryMetricsSnapshot {
            requests: self.metrics.requests.load(Ordering::Acquire),
            elapsed_micros: self.metrics.elapsed_micros.load(Ordering::Acquire),
        }
    }

    async fn query_response(&self, sql: &str) -> Result<QueryResponse> {
        self.metrics.requests.fetch_add(1, Ordering::Relaxed);
        let url = format!("{}/v1/database/{}/sql", self.base_url, self.database);
        let mut request = self.http.post(&url).body(sql.to_owned());
        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }

        let started = Instant::now();
        let response = request.send().await;
        let elapsed = started.elapsed();
        self.metrics
            .elapsed_micros
            .fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);
        if elapsed >= Duration::from_millis(250) {
            tracing::warn!(?elapsed, query = %sql, "slow SpacetimeDB query");
        }
        let response = response?;
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SpacetimeError::Spacetime(error_text));
        }

        let text = response.text().await?;
        serde_json::from_str(&text).map_err(Into::into)
    }

    /// Run a SQL query and decode exact generated rows through SATS rather
    /// than through hand-maintained serde mirrors.
    pub async fn query_sats<T: SatsDeserializeOwned>(&self, sql: &str) -> Result<Vec<T>> {
        let query_response = self.query_response(sql).await?;
        decode_sats_query_response(&query_response)
    }

    /// Run a generated-row query that should return at most one row.
    pub async fn query_one_sats<T: SatsDeserializeOwned>(&self, sql: &str) -> Result<Option<T>> {
        let mut rows = self.query_sats(sql).await?;
        if rows.len() > 1 {
            return Err(SpacetimeError::Spacetime(format!(
                "query expected at most one row but returned {}: {sql}",
                rows.len()
            )));
        }
        Ok(rows.pop())
    }

    /// Decode exact generated rows first, then apply an explicit presentation
    /// projection. This keeps schema validation on the generated SATS owner.
    pub async fn query_sats_into<T, U>(&self, sql: &str) -> Result<Vec<U>>
    where
        T: SatsDeserializeOwned,
        U: TryFrom<T>,
        U::Error: std::fmt::Display,
    {
        self.query_sats(sql)
            .await?
            .into_iter()
            .map(|row| {
                U::try_from(row).map_err(|error| {
                    SpacetimeError::Spacetime(format!(
                        "generated-row presentation conversion failed: {error}"
                    ))
                })
            })
            .collect()
    }

    /// Decode and project a generated query that should return at most one row.
    pub async fn query_one_sats_into<T, U>(&self, sql: &str) -> Result<Option<U>>
    where
        T: SatsDeserializeOwned,
        U: TryFrom<T>,
        U::Error: std::fmt::Display,
    {
        self.query_one_sats(sql)
            .await?
            .map(U::try_from)
            .transpose()
            .map_err(|error| {
                SpacetimeError::Spacetime(format!(
                    "generated-row presentation conversion failed: {error}"
                ))
            })
    }

    /// Call a reducer with JSON arguments
    pub async fn call(&self, reducer: &str, args: &[Value]) -> Result<()> {
        let url = format!(
            "{}/v1/database/{}/call/{}",
            self.base_url, self.database, reducer
        );

        let mut request = self.http.post(&url).json(args);

        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let started = Instant::now();
        let response = request.send().await;
        let elapsed = started.elapsed();
        if elapsed >= Duration::from_millis(250) {
            tracing::warn!(?elapsed, reducer, "slow SpacetimeDB reducer call");
        }
        let response = response?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SpacetimeError::Spacetime(error_text));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adventuresim_stdb_client::{
        BackendForageReceipt, CaseStatus, InvestigationActionAvailability,
        InvestigationActionUnavailableReason, SettlementAlias, StrategicGatewayAuthority,
    };

    fn query_fixture(columns: &[(&str, Value)], row: Value) -> QueryResponse {
        vec![super::super::types::QueryResult {
            schema: super::super::types::QuerySchema {
                elements: columns
                    .iter()
                    .map(
                        |(name, algebraic_type)| super::super::types::SchemaElement {
                            name: Some(super::super::types::AlgebraicTypeRef {
                                some: (*name).to_owned(),
                            }),
                            algebraic_type: AlgebraicType::Value(algebraic_type.clone()),
                        },
                    )
                    .collect(),
            },
            rows: vec![row],
        }]
    }

    #[test]
    fn sats_conversion_preserves_named_products_options_and_one_key_sums() {
        let option_string = json!({ "Sum": { "variants": [
            {
                "name": { "some": "some" },
                "algebraic_type": { "String": [] }
            },
            {
                "name": { "some": "none" },
                "algebraic_type": { "Product": { "elements": [] } }
            }
        ] } });
        let ty = AlgebraicType::Value(json!({
            "Product": { "elements": [
                { "name": { "some": "id" }, "algebraic_type": { "String": [] } },
                { "name": { "some": "settlement_id" }, "algebraic_type": { "String": [] } },
                { "name": { "some": "name" }, "algebraic_type": { "String": [] } },
                { "name": { "some": "prefix" }, "algebraic_type": option_string },
                { "name": { "some": "language" }, "algebraic_type": option_string }
            ] }
        }));
        let converted = convert_spacetime_value_sats(
            &json!(["alias-1", "settlement-1", "Harbour", [0, "Old"], [1, []]]),
            &ty,
        )
        .unwrap();
        assert_eq!(
            converted,
            json!({
                "id": "alias-1",
                "settlement_id": "settlement-1",
                "name": "Harbour",
                "prefix": { "some": "Old" },
                "language": { "none": [] }
            })
        );
        let decoded = serde_json::from_value::<SerdeWrapper<SettlementAlias>>(converted)
            .unwrap()
            .0;
        assert_eq!(decoded.prefix.as_deref(), Some("Old"));
        assert_eq!(decoded.language, None);

        let sum = AlgebraicType::Value(json!({ "Sum": { "variants": [
            {
                "name": { "some": "Open" },
                "algebraic_type": { "Product": { "elements": [] } }
            },
            {
                "name": { "some": "Resolved" },
                "algebraic_type": { "Product": { "elements": [] } }
            },
            {
                "name": { "some": "Failed" },
                "algebraic_type": { "Product": { "elements": [] } }
            }
        ] } }));
        let converted = convert_spacetime_value_sats(&json!([1, []]), &sum).unwrap();
        assert_eq!(converted, json!({ "Resolved": [] }));
        assert_eq!(
            serde_json::from_value::<SerdeWrapper<CaseStatus>>(converted)
                .unwrap()
                .0,
            CaseStatus::Resolved
        );
    }

    #[test]
    fn sats_generated_products_reject_missing_and_unknown_fields() {
        let complete = json!({
            "id": "alias-1",
            "settlement_id": "settlement-1",
            "name": "Harbour",
            "prefix": { "none": [] },
            "language": { "none": [] }
        });
        let mut missing = complete.clone();
        missing.as_object_mut().unwrap().remove("name");
        assert!(serde_json::from_value::<SerdeWrapper<SettlementAlias>>(missing).is_err());

        let mut unknown = complete;
        unknown["display_name"] = json!("Harbour");
        assert!(serde_json::from_value::<SerdeWrapper<SettlementAlias>>(unknown).is_err());
    }

    #[test]
    fn sats_sql_fixture_decodes_generated_identity_row() {
        let option_string = json!({ "Sum": { "variants": [
            {
                "name": { "some": "some" },
                "algebraic_type": { "String": [] }
            },
            {
                "name": { "some": "none" },
                "algebraic_type": { "Product": { "elements": [] } }
            }
        ] } });
        let identity = json!({ "Product": { "elements": [{
            "name": { "some": "__identity__" },
            "algebraic_type": { "U256": [] }
        }] } });
        let response = query_fixture(
            &[
                ("id", json!({ "U8": [] })),
                ("identity", identity),
                ("terrain_package_digest", option_string),
                ("terrain_schema", json!({ "U32": [] })),
            ],
            json!([
                1,
                ["0000000000000000000000000000000000000000000000000000000000000000"],
                [1, []],
                3
            ]),
        );
        let decoded = decode_sats_query_response::<StrategicGatewayAuthority>(&response).unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(
            decoded[0].identity,
            adventuresim_stdb_client::spacetimedb_sdk::Identity::ZERO
        );
        assert_eq!(decoded[0].terrain_package_digest, None);
    }

    #[test]
    fn sats_sql_fixture_decodes_generated_array_row() {
        let response = query_fixture(
            &[
                ("character_id", json!({ "U64": [] })),
                ("request_id", json!({ "String": [] })),
                ("elapsed_minutes", json!({ "U64": [] })),
                ("yielded_item_ids", json!({ "Array": { "String": [] } })),
                ("yielded_quantities", json!({ "Array": { "U16": [] } })),
                ("interrupted", json!({ "Bool": [] })),
                ("legal_outcome", json!({ "String": [] })),
            ],
            json!([
                7,
                "forage:7:1",
                45,
                ["yarrow", "nettle"],
                [2, 1],
                false,
                "lawful"
            ]),
        );
        let decoded = decode_sats_query_response::<BackendForageReceipt>(&response).unwrap();
        assert_eq!(decoded[0].yielded_item_ids, ["yarrow", "nettle"]);
        assert_eq!(decoded[0].yielded_quantities, [2, 1]);
    }

    #[test]
    fn sats_fixture_decodes_generated_data_carrying_sum() {
        let reason = json!({ "Sum": { "variants": [
            { "name": { "some": "PartyNotReady" }, "algebraic_type": { "Product": { "elements": [] } } },
            { "name": { "some": "TravelRequired" }, "algebraic_type": { "Product": { "elements": [] } } },
            { "name": { "some": "NightWindow" }, "algebraic_type": { "Product": { "elements": [] } } },
            { "name": { "some": "TargetChanged" }, "algebraic_type": { "Product": { "elements": [] } } },
            { "name": { "some": "ContactScheduleWindow" }, "algebraic_type": { "Product": { "elements": [] } } },
            { "name": { "some": "ContactNotPresent" }, "algebraic_type": { "Product": { "elements": [] } } },
            { "name": { "some": "CharacterUnavailable" }, "algebraic_type": { "Product": { "elements": [] } } },
            { "name": { "some": "PartyRequired" }, "algebraic_type": { "Product": { "elements": [] } } }
        ] } });
        let unavailable = json!({ "Product": { "elements": [
            { "name": { "some": "reason" }, "algebraic_type": reason },
            { "name": { "some": "can_travel_to_required_site" }, "algebraic_type": { "Bool": [] } },
            { "name": { "some": "wait_minutes" }, "algebraic_type": { "U32": [] } }
        ] } });
        let availability = AlgebraicType::Value(json!({ "Sum": { "variants": [
            { "name": { "some": "Available" }, "algebraic_type": { "Product": { "elements": [] } } },
            { "name": { "some": "Unavailable" }, "algebraic_type": unavailable }
        ] } }));
        let converted =
            convert_spacetime_value_sats(&json!([1, [[1, []], true, 45]]), &availability).unwrap();
        let decoded =
            serde_json::from_value::<SerdeWrapper<InvestigationActionAvailability>>(converted)
                .unwrap()
                .0;
        let InvestigationActionAvailability::Unavailable(details) = decoded else {
            panic!("expected data-carrying unavailable variant");
        };
        assert_eq!(
            details.reason,
            InvestigationActionUnavailableReason::TravelRequired
        );
        assert!(details.can_travel_to_required_site);
        assert_eq!(details.wait_minutes, 45);
    }

    #[test]
    fn sats_conversion_rejects_malformed_products_and_variants() {
        let product = AlgebraicType::Value(json!({
            "Product": { "elements": [
                { "name": { "some": "value" }, "algebraic_type": { "U64": [] } }
            ] }
        }));
        assert!(convert_spacetime_value_sats(&json!([]), &product).is_err());

        let sum = AlgebraicType::Value(json!({ "Sum": { "variants": [
            {
                "name": { "some": "Only" },
                "algebraic_type": { "Product": { "elements": [] } }
            }
        ] } }));
        assert!(convert_spacetime_value_sats(&json!([1, []]), &sum).is_err());
        assert!(convert_spacetime_value_sats(&json!([0]), &sum).is_err());
    }

    #[test]
    fn query_metrics_are_monotonic_and_clone_safe() {
        let client = SpacetimeClient::new("http://localhost:3000", "test").unwrap();
        client.metrics.requests.store(3, Ordering::Relaxed);
        client.metrics.elapsed_micros.store(125, Ordering::Relaxed);
        assert_eq!(
            client.query_metrics(),
            QueryMetricsSnapshot {
                requests: 3,
                elapsed_micros: 125
            }
        );
        assert_eq!(
            client.query_metrics().delta(QueryMetricsSnapshot {
                requests: 3,
                elapsed_micros: 125
            }),
            QueryMetricsSnapshot::default()
        );
        let clone = client.clone();
        clone.metrics.requests.fetch_add(1, Ordering::Relaxed);
        assert_eq!(
            client
                .query_metrics()
                .delta(QueryMetricsSnapshot {
                    requests: 3,
                    elapsed_micros: 125
                })
                .requests,
            1
        );
    }

    #[test]
    fn injected_latency_measurement_delta_is_deterministic() {
        let before = QueryMetricsSnapshot::default();
        let after = QueryMetricsSnapshot {
            requests: 3,
            elapsed_micros: 425_000,
        };
        assert_eq!(
            after.delta(before),
            QueryMetricsSnapshot {
                requests: 3,
                elapsed_micros: 425_000
            }
        );
    }
}
