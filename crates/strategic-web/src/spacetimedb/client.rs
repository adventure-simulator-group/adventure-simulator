//! SpacetimeDB HTTP client wrapper

use reqwest::Client;
use serde::de::DeserializeOwned;
use serde_json::Value;
use serde_json::json;
use std::time::{Duration, Instant};

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

/// Convert SpacetimeDB encoded values to normal JSON using schema type info.
/// Handles:
/// - Option<T>: [0, value] => value, [1, []] => null
/// - Unit enums: [tag, []] => "VariantName"
/// - Identity: ["0x..."] => "0x..."
fn convert_spacetime_value(value: &Value, algebraic_type: &AlgebraicType) -> Value {
    if let Some(element_type) = array_element_type(algebraic_type)
        && let Value::Array(values) = value
    {
        let element_type = AlgebraicType::Value(element_type.clone());
        return Value::Array(
            values
                .iter()
                .map(|value| convert_spacetime_value(value, &element_type))
                .collect(),
        );
    }

    if let Some(elements) = product_elements(algebraic_type) {
        if is_identity_product(elements) {
            if let Some(identity) = value
                .as_array()
                .and_then(|items| items.first())
                .and_then(Value::as_str)
            {
                return Value::String(identity.to_string());
            }
        }

        if let Value::Array(values) = value {
            let mut object = serde_json::Map::new();
            for (element, value) in elements.iter().zip(values) {
                let Some(name) = element
                    .get("name")
                    .and_then(|name| name.get("some"))
                    .and_then(Value::as_str)
                else {
                    continue;
                };
                let nested_type = AlgebraicType::Value(
                    element
                        .get("algebraic_type")
                        .cloned()
                        .unwrap_or(Value::Null),
                );
                object.insert(
                    name.to_string(),
                    convert_spacetime_value(value, &nested_type),
                );
            }
            if !object.is_empty() {
                return Value::Object(object);
            }
        }
    }

    let Some(variants) = sum_variants(algebraic_type) else {
        return value.clone();
    };

    let Value::Array(arr) = value else {
        return value.clone();
    };

    if arr.len() != 2 {
        return value.clone();
    }

    let Some(tag) = arr[0].as_u64() else {
        return value.clone();
    };
    let Some(variant) = variants.get(tag as usize) else {
        return value.clone();
    };
    let payload = &arr[1];

    if is_option_sum(variants) {
        if variant_name(variant)
            .map(|n| n.eq_ignore_ascii_case("none"))
            .unwrap_or(false)
        {
            return Value::Null;
        }
        let payload_type = AlgebraicType::Value(
            variant
                .get("algebraic_type")
                .cloned()
                .unwrap_or(Value::Null),
        );
        return convert_spacetime_value(payload, &payload_type);
    }

    // Most strategic layer enums are unit variants, encoded as empty tuple payload.
    if payload
        .as_array()
        .map(|elements| elements.is_empty())
        .unwrap_or(false)
    {
        return Value::String(serde_variant_name(variant));
    }

    let payload_type = AlgebraicType::Value(
        variant
            .get("algebraic_type")
            .cloned()
            .unwrap_or(Value::Null),
    );
    let payload = convert_spacetime_value(payload, &payload_type);

    Value::Object(
        [(serde_variant_name(variant), json!(payload))]
            .into_iter()
            .collect(),
    )
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

pub type Result<T> = std::result::Result<T, SpacetimeError>;

/// HTTP client for SpacetimeDB
#[derive(Clone)]
pub struct SpacetimeClient {
    http: Client,
    base_url: String,
    database: String,
    token: Option<String>,
}

impl SpacetimeClient {
    /// Create a new SpacetimeDB client
    pub fn new(base_url: impl Into<String>, database: impl Into<String>) -> Self {
        Self {
            http: Client::builder()
                .timeout(Duration::from_secs(10))
                .connect_timeout(Duration::from_secs(3))
                .build()
                .expect("failed to build SpacetimeDB HTTP client"),
            base_url: base_url.into(),
            database: database.into(),
            token: None,
        }
    }

    /// Set the auth token
    pub fn with_token(mut self, token: Option<String>) -> Self {
        self.token = token;
        self
    }

    /// Whether this client points at the local development database.
    pub fn is_local(&self) -> bool {
        let base = self.base_url.to_ascii_lowercase();
        base.contains("localhost") || base.contains("127.0.0.1") || base.contains("[::1]")
    }

    /// Run a SQL query and return typed rows
    pub async fn query<T: DeserializeOwned>(&self, sql: &str) -> Result<Vec<T>> {
        let url = format!("{}/v1/database/{}/sql", self.base_url, self.database);

        let mut request = self.http.post(&url).body(sql.to_string());

        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let started = Instant::now();
        let response = request.send().await;
        let elapsed = started.elapsed();
        if elapsed >= Duration::from_millis(250) {
            tracing::warn!(?elapsed, query = %sql, "slow SpacetimeDB query");
        }
        let response = response?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SpacetimeError::Spacetime(error_text));
        }

        let text = response.text().await?;
        let query_response: QueryResponse = serde_json::from_str(&text)?;

        // Extract rows from the first result set
        if let Some(first) = query_response.first() {
            // Get column metadata from schema
            let columns: Vec<(&str, &AlgebraicType)> = first
                .schema
                .elements
                .iter()
                .filter_map(|e| {
                    e.name
                        .as_ref()
                        .map(|n| (n.some.as_str(), &e.algebraic_type))
                })
                .collect();

            // Convert array rows to objects using column names
            let rows: Result<Vec<T>> = first
                .rows
                .iter()
                .map(|row| {
                    if let Value::Array(values) = row {
                        let mut obj = serde_json::Map::new();
                        for (i, value) in values.iter().enumerate() {
                            if let Some(&(name, algebraic_type)) = columns.get(i) {
                                let converted = convert_spacetime_value(value, algebraic_type);
                                obj.insert(name.to_string(), converted);
                            }
                        }
                        serde_json::from_value(Value::Object(obj)).map_err(Into::into)
                    } else {
                        Err(SpacetimeError::Spacetime(
                            "SpacetimeDB returned a non-array SQL row".into(),
                        ))
                    }
                })
                .collect();
            rows
        } else {
            Ok(vec![])
        }
    }

    /// Run a query that should return at most one row without conflating an
    /// empty result with a transport or decoding failure.
    pub async fn query_one<T: DeserializeOwned>(&self, sql: &str) -> Result<Option<T>> {
        let mut rows = self.query(sql).await?;
        if rows.len() > 1 {
            return Err(SpacetimeError::Spacetime(format!(
                "query expected at most one row but returned {}: {sql}",
                rows.len()
            )));
        }
        Ok(rows.pop())
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

    #[test]
    fn converts_named_nested_products_to_objects() {
        let ty = AlgebraicType::Value(json!({
            "Product": { "elements": [
                { "name": { "some": "melee" }, "algebraic_type": { "Bool": [] } },
                { "name": { "some": "endurance" }, "algebraic_type": { "U8": [] } }
            ] }
        }));
        assert_eq!(
            convert_spacetime_value(&json!([true, 3]), &ty),
            json!({ "melee": true, "endurance": 3 })
        );
    }

    #[test]
    fn converts_arrays_of_nested_sum_variants() {
        let ty = AlgebraicType::Value(json!({
            "Product": { "elements": [
                {
                    "name": { "some": "outputs" },
                    "algebraic_type": { "Array": { "Sum": { "variants": [
                        {
                            "name": { "some": "derived" },
                            "algebraic_type": { "Product": { "elements": [] } }
                        },
                        {
                            "name": { "some": "fallback" },
                            "algebraic_type": { "Sum": { "variants": [
                                {
                                    "name": { "some": "woodlandFuelwood" },
                                    "algebraic_type": { "Product": { "elements": [] } }
                                }
                            ] } }
                        }
                    ] } } }
                }
            ] }
        }));
        let converted = convert_spacetime_value(&json!([[[1, [0, []]]]]), &ty);
        assert_eq!(
            converted,
            json!({ "outputs": [{ "Fallback": "WoodlandFuelwood" }] })
        );
        serde_json::from_value::<adventuresim_world_schema::InferredIndustryProfile>(converted)
            .expect("converted industry profile should deserialize");
    }
}
