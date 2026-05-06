//! Edgegap Deployment API HTTP client
//!
//! References:
//! - POST /v2/deployments
//! - GET /v1/status/{request_id}
//! - DELETE /v1/stop/{request_id}

use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum EdgegapError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Edgegap API error ({status}): {body}")]
    Api { status: u16, body: String },
    #[error("Edgegap request failed: {0}")]
    Invalid(String),
}

pub type Result<T> = std::result::Result<T, EdgegapError>;

#[derive(Debug, Clone, Serialize)]
struct CreateDeploymentRequest {
    pub application: String,
    pub version: String,
    pub users: Vec<DeploymentUser>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub environment_variables: Vec<EnvVar>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DeploymentUser {
    pub user_type: String,
    pub user_data: DeploymentUserData,
}

#[derive(Debug, Clone, Serialize)]
struct DeploymentUserData {
    pub ip_address: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnvVar {
    pub key: String,
    pub value: String,
    pub is_hidden: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct CreateDeploymentResponse {
    pub request_id: Option<String>,
    #[serde(default)]
    pub error: bool,
    pub message: Option<String>,
    #[serde(default)]
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeploymentStatus {
    pub request_id: Option<String>,
    pub current_status: Option<String>,
    pub fqdn: Option<String>,
    pub public_ip: Option<String>,
    #[serde(default)]
    pub ports: Option<serde_json::Value>,
    #[serde(default)]
    pub error: bool,
    pub message: Option<String>,
    #[serde(default)]
    pub errors: Vec<String>,
}

impl DeploymentStatus {
    pub fn status_label(&self) -> String {
        self.current_status
            .clone()
            .unwrap_or_else(|| "UNKNOWN".to_string())
    }

    pub fn last_error(&self) -> Option<String> {
        if !self.errors.is_empty() {
            return Some(self.errors.join("; "));
        }
        self.message.clone()
    }

    pub fn is_failed(&self) -> bool {
        if self.error {
            return true;
        }

        let status = self.status_label().to_ascii_uppercase();
        status.contains("ERROR")
            || status.contains("FAIL")
            || status.contains("CANCEL")
            || status.contains("TERMINAT")
    }
}

#[derive(Clone)]
pub struct EdgegapClient {
    http: Client,
    base_url: String,
    auth_header: String,
    pub application_name: String,
    pub version_name: String,
}

impl EdgegapClient {
    pub fn new(
        base_url: impl Into<String>,
        token: impl Into<String>,
        application_name: impl Into<String>,
        version_name: impl Into<String>,
    ) -> Self {
        let token = token.into();
        let auth_header = if token.starts_with("token ") || token.starts_with("Bearer ") {
            token
        } else {
            format!("token {}", token)
        };

        Self {
            http: Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            auth_header,
            application_name: application_name.into(),
            version_name: version_name.into(),
        }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}{}", self.base_url, path))
            .header("Authorization", &self.auth_header)
            .header("Content-Type", "application/json")
    }

    pub async fn create_deployment(
        &self,
        user_ips: Vec<String>,
        env_vars: Vec<EnvVar>,
        tags: Vec<String>,
    ) -> Result<String> {
        if user_ips.is_empty() {
            return Err(EdgegapError::Invalid(
                "no valid client IP found for deployment".to_string(),
            ));
        }

        let body = CreateDeploymentRequest {
            application: self.application_name.clone(),
            version: self.version_name.clone(),
            users: user_ips
                .into_iter()
                .map(|ip| DeploymentUser {
                    user_type: "ip_address".to_string(),
                    user_data: DeploymentUserData { ip_address: ip },
                })
                .collect(),
            environment_variables: env_vars,
            tags,
        };

        let response = self
            .request(reqwest::Method::POST, "/v2/deployments")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(EdgegapError::Api { status, body });
        }

        let payload: CreateDeploymentResponse = response.json().await?;
        if payload.error {
            return Err(EdgegapError::Invalid(
                payload
                    .message
                    .or_else(|| (!payload.errors.is_empty()).then_some(payload.errors.join("; ")))
                    .unwrap_or_else(|| "unknown Edgegap deployment error".to_string()),
            ));
        }

        payload
            .request_id
            .ok_or_else(|| EdgegapError::Invalid("missing request_id from Edgegap".to_string()))
    }

    pub async fn get_status(&self, request_id: &str) -> Result<DeploymentStatus> {
        let response = self
            .request(reqwest::Method::GET, &format!("/v1/status/{}", request_id))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(EdgegapError::Api { status, body });
        }

        Ok(response.json().await?)
    }

    pub async fn stop_deployment(&self, request_id: &str) -> Result<()> {
        let response = self
            .request(reqwest::Method::DELETE, &format!("/v1/stop/{}", request_id))
            .send()
            .await?;

        // Edgegap returns 404 when deployment is already gone; treat as idempotent success.
        if response.status().as_u16() == 404 {
            return Ok(());
        }

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(EdgegapError::Api { status, body });
        }

        Ok(())
    }
}
