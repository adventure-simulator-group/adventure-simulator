//! SpacetimeDB HTTP client wrapper

use reqwest::Client;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::types::QueryResponse;

/// Convert SpacetimeDB value encoding to standard JSON
/// SpacetimeDB encodes Option as: [0, value] = Some(value), [1, []] = None
fn convert_spacetime_value(value: &Value) -> Value {
    match value {
        Value::Array(arr) if arr.len() == 2 => {
            // Check if this is a SpacetimeDB Option encoding
            if let Some(tag) = arr[0].as_i64() {
                match tag {
                    0 => convert_spacetime_value(&arr[1]), // Some(value)
                    1 => Value::Null,                       // None
                    _ => value.clone(),
                }
            } else {
                value.clone()
            }
        }
        _ => value.clone(),
    }
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
            http: Client::new(),
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
        let url = format!(
            "{}/v1/database/{}/sql",
            self.base_url, self.database
        );

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
            // Get column names from schema
            let column_names: Vec<&str> = first
                .schema
                .elements
                .iter()
                .filter_map(|e| e.name.as_ref().map(|n| n.some.as_str()))
                .collect();

            // Convert array rows to objects using column names
            let rows: Vec<T> = first
                .rows
                .iter()
                .filter_map(|row| {
                    if let Value::Array(values) = row {
                        let mut obj = serde_json::Map::new();
                        for (i, value) in values.iter().enumerate() {
                            if let Some(&name) = column_names.get(i) {
                                // Handle SpacetimeDB Option encoding: [0, value] = Some, [1, []] = None
                                let converted = convert_spacetime_value(value);
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
        let url = format!(
            "{}/v1/database/{}/sql",
            self.base_url, self.database
        );

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
