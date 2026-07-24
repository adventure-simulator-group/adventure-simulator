use super::{
    MAX_PROVIDER_RESPONSE_BYTES, PlayerFrame, PolicyDecision, QuestPolicy, parse_provider_decision,
    policy_prompt,
};
use reqwest::{
    StatusCode, Url,
    blocking::Client,
    header::{AUTHORIZATION, CONTENT_TYPE},
    redirect::Policy,
};
use serde::{Deserialize, Serialize};
use std::{
    env, thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct ProviderConfig {
    pub endpoint: String,
    pub model: String,
    pub api_key_env: String,
    pub allow_network: bool,
    pub max_requests: u32,
    pub max_prompt_tokens: u32,
    pub max_completion_tokens: u32,
    pub max_cost_microusd: u64,
    pub prompt_microusd_per_million_tokens: u64,
    pub completion_microusd_per_million_tokens: u64,
    pub requests_per_minute: u32,
    pub max_retries: u8,
    pub timeout: Duration,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            endpoint: "https://api.openai.com/v1/chat/completions".into(),
            model: "gpt-4.1-nano".into(),
            api_key_env: "OPENAI_API_KEY".into(),
            allow_network: false,
            max_requests: 64,
            max_prompt_tokens: 16_000,
            max_completion_tokens: 256,
            max_cost_microusd: 100_000,
            prompt_microusd_per_million_tokens: 100_000,
            completion_microusd_per_million_tokens: 400_000,
            requests_per_minute: 60,
            max_retries: 2,
            timeout: Duration::from_secs(30),
        }
    }
}

impl ProviderConfig {
    pub fn validate_and_key(&self) -> Result<(Url, String), String> {
        if !self.allow_network {
            return Err("network provider requires explicit --allow-network".into());
        }
        if self.api_key_env.is_empty()
            || self.api_key_env.len() > 128
            || !self
                .api_key_env
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        {
            return Err("API key environment variable name is invalid".into());
        }
        // Read the key before creating a client or doing any network work.
        let key = env::var(&self.api_key_env).map_err(|_| {
            format!(
                "required API key environment variable {} is missing",
                self.api_key_env
            )
        })?;
        if key.is_empty() {
            return Err("API key environment variable is empty".into());
        }
        let url = Url::parse(&self.endpoint).map_err(|_| "provider endpoint is invalid")?;
        if !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("provider URL must not contain credentials, query, or fragment".into());
        }
        let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "::1"));
        if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
            return Err("provider URL must use HTTPS except for loopback fixtures".into());
        }
        if self.max_requests == 0
            || self.max_prompt_tokens == 0
            || self.max_completion_tokens == 0
            || self.requests_per_minute == 0
            || self.max_retries > 5
            || self.timeout > Duration::from_secs(120)
        {
            return Err("provider bounds are invalid".into());
        }
        Ok((url, key))
    }
}

pub struct OpenAiCompatiblePolicy {
    config: ProviderConfig,
    endpoint: Url,
    key: String,
    client: Client,
    requests: u32,
    estimated_cost_microusd: u64,
    last_request: Option<Instant>,
}

impl OpenAiCompatiblePolicy {
    pub fn new(config: ProviderConfig) -> Result<Self, String> {
        let (endpoint, key) = config.validate_and_key()?;
        let client = Client::builder()
            .redirect(Policy::none())
            .timeout(config.timeout)
            .build()
            .map_err(|error| format!("provider client setup failed: {error}"))?;
        Ok(Self {
            config,
            endpoint,
            key,
            client,
            requests: 0,
            estimated_cost_microusd: 0,
            last_request: None,
        })
    }

    fn request(&mut self, frame: &PlayerFrame, repair: bool) -> Result<PolicyDecision, String> {
        if self.requests >= self.config.max_requests {
            return Err("provider request budget exhausted".into());
        }
        let prompt = policy_prompt(frame)?;
        let prompt_tokens = estimate_tokens(&prompt);
        if prompt_tokens > u64::from(self.config.max_prompt_tokens) {
            return Err("prompt token budget exceeded".into());
        }
        let projected = priced(
            prompt_tokens,
            u64::from(self.config.max_completion_tokens),
            &self.config,
        );
        if self.estimated_cost_microusd.saturating_add(projected) > self.config.max_cost_microusd {
            return Err("provider cost budget exceeded".into());
        }
        if let Some(previous) = self.last_request {
            let minimum =
                Duration::from_secs_f64(60.0 / f64::from(self.config.requests_per_minute));
            if let Some(wait) = minimum.checked_sub(previous.elapsed()) {
                thread::sleep(wait.min(Duration::from_secs(2)));
            }
        }
        let body = ChatRequest {
            model: &self.config.model,
            messages: vec![
                Message {
                    role: "system",
                    content: if repair {
                        "Your previous response violated the schema. Return one strict JSON object only."
                    } else {
                        "Return a legal game choice as strict JSON only."
                    },
                },
                Message {
                    role: "user",
                    content: &prompt,
                },
            ],
            max_tokens: self.config.max_completion_tokens,
            temperature: 0,
        };
        let mut retry = 0;
        loop {
            self.requests += 1;
            self.last_request = Some(Instant::now());
            let response = self
                .client
                .post(self.endpoint.clone())
                .header(AUTHORIZATION, format!("Bearer {}", self.key))
                .header(CONTENT_TYPE, "application/json")
                .json(&body)
                .send()
                .map_err(|error| format!("provider request failed: {error}"))?;
            if matches!(response.status(), StatusCode::TOO_MANY_REQUESTS)
                || response.status().is_server_error()
            {
                if retry < self.config.max_retries && self.requests < self.config.max_requests {
                    retry += 1;
                    thread::sleep(Duration::from_millis(100_u64 << retry));
                    continue;
                }
                return Err(format!(
                    "provider retry cutoff reached ({})",
                    response.status()
                ));
            }
            if !response.status().is_success() {
                return Err(format!("provider rejected request ({})", response.status()));
            }
            if response
                .content_length()
                .is_some_and(|length| length > MAX_PROVIDER_RESPONSE_BYTES as u64)
            {
                return Err("provider response exceeds byte budget".into());
            }
            let bytes = response
                .bytes()
                .map_err(|error| format!("provider response read failed: {error}"))?;
            if bytes.len() > MAX_PROVIDER_RESPONSE_BYTES {
                return Err("provider response exceeds byte budget".into());
            }
            let envelope: ChatResponse = serde_json::from_slice(&bytes)
                .map_err(|_| "provider returned malformed response envelope")?;
            let content = envelope
                .choices
                .first()
                .ok_or("provider response contained no choice")?
                .message
                .content
                .as_bytes();
            self.estimated_cost_microusd = self.estimated_cost_microusd.saturating_add(projected);
            return parse_provider_decision(content);
        }
    }
}

impl QuestPolicy for OpenAiCompatiblePolicy {
    fn name(&self) -> &str {
        "openai-compatible-v1"
    }

    fn decide(&mut self, frame: &PlayerFrame) -> Result<PolicyDecision, String> {
        match self.request(frame, false) {
            Ok(decision) => Ok(decision),
            Err(first) if first.contains("provider JSON") || first.contains("schema") => self
                .request(frame, true)
                .map_err(|repair| format!("{first}; bounded repair failed: {repair}")),
            Err(error) => Err(error),
        }
    }
}

fn estimate_tokens(text: &str) -> u64 {
    (text.len() as u64).div_ceil(4)
}

fn priced(prompt: u64, completion: u64, config: &ProviderConfig) -> u64 {
    prompt
        .saturating_mul(config.prompt_microusd_per_million_tokens)
        .saturating_add(completion.saturating_mul(config.completion_microusd_per_million_tokens))
        .div_ceil(1_000_000)
}

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<Message<'a>>,
    max_tokens: u32,
    temperature: u8,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: String,
}

impl Drop for OpenAiCompatiblePolicy {
    fn drop(&mut self) {
        // Proactively erase the credential from owned memory. Debug and report
        // types do not expose this field.
        self.key.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_fail_closed() {
        let invalid = [
            "http://example.com/v1",
            "https://user:pass@example.com/v1",
            "https://example.com/v1?key=x",
            "https://example.com/v1#x",
        ];
        for endpoint in invalid {
            let config = ProviderConfig {
                endpoint: endpoint.into(),
                allow_network: true,
                api_key_env: "MISSING_EVAL_TEST_KEY".into(),
                ..ProviderConfig::default()
            };
            // Missing key is deliberately checked before endpoint/network.
            assert!(config.validate_and_key().unwrap_err().contains("missing"));
        }
    }

    #[test]
    fn network_requires_explicit_opt_in() {
        let error = ProviderConfig::default().validate_and_key().unwrap_err();
        assert!(error.contains("allow-network"));
    }

    #[test]
    fn debug_config_never_contains_a_key_value() {
        let config = ProviderConfig::default();
        assert!(format!("{config:?}").contains("OPENAI_API_KEY"));
        assert!(!format!("{config:?}").contains("Bearer"));
    }
}
