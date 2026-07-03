//! SpacetimeDB HTTP client wrapper

use reqwest::Client;
use serde::de::DeserializeOwned;
use serde_json::json;
use serde_json::Value;
use std::time::Duration;

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
        return payload.clone();
    }

    // Most strategic layer enums are unit variants, encoded as empty tuple payload.
    if payload
        .as_array()
        .map(|elements| elements.is_empty())
        .unwrap_or(false)
    {
        return Value::String(variant_name(variant).unwrap_or("Unknown").to_string());
    }

    Value::Object(
        [(
            variant_name(variant).unwrap_or("Unknown").to_string(),
            json!(payload),
        )]
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

    /// Run a SQL query and return typed rows
    pub async fn query<T: DeserializeOwned>(&self, sql: &str) -> Result<Vec<T>> {
        let url = format!("{}/v1/database/{}/sql", self.base_url, self.database);

        let mut request = self.http.post(&url).body(sql.to_string());

        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request.send().await?;

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
            let rows: Vec<T> = first
                .rows
                .iter()
                .filter_map(|row| {
                    if let Value::Array(values) = row {
                        let mut obj = serde_json::Map::new();
                        for (i, value) in values.iter().enumerate() {
                            if let Some(&(name, algebraic_type)) = columns.get(i) {
                                let converted = convert_spacetime_value(value, algebraic_type);
                                obj.insert(name.to_string(), converted);
                            }
                        }
                        serde_json::from_value(Value::Object(obj)).ok()
                    } else {
                        None
                    }
                })
                .collect();
            Ok(rows)
        } else {
            Ok(vec![])
        }
    }

    /// Run a SQL query and return raw JSON values
    pub async fn query_raw(&self, sql: &str) -> Result<Vec<Value>> {
        let url = format!("{}/v1/database/{}/sql", self.base_url, self.database);

        let mut request = self.http.post(&url).body(sql.to_string());

        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request.send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SpacetimeError::Spacetime(error_text));
        }

        let text = response.text().await?;
        let query_response: QueryResponse = serde_json::from_str(&text)?;

        if let Some(first) = query_response.first() {
            Ok(first.rows.clone())
        } else {
            Ok(vec![])
        }
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

        let response = request.send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(SpacetimeError::Spacetime(error_text));
        }

        Ok(())
    }
}
