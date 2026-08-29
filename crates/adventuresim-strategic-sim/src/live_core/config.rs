//! Core-loop configuration, shared limits, and stable domain mappings.

use super::*;

pub(super) const ACTION_TIMEOUT: Duration = Duration::from_secs(20);
/// Severe but non-incapacitating injuries can reduce overland pace enough for
/// a long quest leg to require many daily camps.
pub(super) const MAX_CAMPS_PER_LEG: u32 = 512;
/// Long but survivable illnesses and injuries can require many daily rests;
/// keep the policy bounded well beyond ordinary convalescence.
pub(super) const MAX_RECOVERY_ACTIONS: u32 = 128;
pub(super) const MAX_CORE_LOOP_WORK: u64 = 100_000;
pub(super) const MAX_CORE_TRACE_EVENTS: usize = 100_000;
pub(super) const DEFAULT_SIMULATION_DISEASE: &str = "influenza";
pub(super) const SIMULATION_DISEASE_SCENARIOS: [&str; 9] = [
    "influenza",
    "dysentery",
    "tetanus",
    "erysipelas",
    "consumption",
    "mahrdruck",
    "shroud_fever",
    "bilwisschuss",
    "kobeldunst",
];

pub(super) fn core_destination_knowledge_stage(
    stage: DestinationKnowledgeStage,
) -> CoreDestinationKnowledgeStage {
    match stage {
        DestinationKnowledgeStage::Unknown => CoreDestinationKnowledgeStage::Unknown,
        DestinationKnowledgeStage::Textual => CoreDestinationKnowledgeStage::Textual,
        DestinationKnowledgeStage::Landmark => CoreDestinationKnowledgeStage::Landmark,
        DestinationKnowledgeStage::ApproximateArea => {
            CoreDestinationKnowledgeStage::ApproximateArea
        }
        DestinationKnowledgeStage::RouteSegment => CoreDestinationKnowledgeStage::RouteSegment,
        DestinationKnowledgeStage::ExactBelieved => CoreDestinationKnowledgeStage::ExactBelieved,
        DestinationKnowledgeStage::Visited => CoreDestinationKnowledgeStage::Visited,
    }
}

pub(super) fn default_simulation_disease() -> String {
    DEFAULT_SIMULATION_DISEASE.to_owned()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoreLoopConfig {
    pub host: String,
    pub database: String,
    pub seed: u64,
    pub population: u32,
    pub cycles: u32,
    pub duration_days: u32,
    pub party_size: u32,
    pub run_nonce: String,
    /// Validated disease identity used only by the disposable fixture.
    #[serde(default = "default_simulation_disease")]
    pub fixture_disease: String,
    /// Install and require the deterministic two-party quest acceptance fixture.
    #[serde(default)]
    pub require_quest_coverage: bool,
    pub use_imported_world: bool,
    pub expected_world_manifest_digest: Option<String>,
    /// Immutable, public-safe diagnostic artifact written if the run fails.
    pub failure_output: Option<PathBuf>,
}

impl CoreLoopConfig {
    pub fn validate(&self) -> Result<(), String> {
        const MAX_DURATION_YEARS: u32 = 100;
        const MAX_DURATION_DAYS: u32 =
            adventuresim_core::strategic_time::DAYS_PER_YEAR as u32 * MAX_DURATION_YEARS;

        validate_loopback_url(&self.host)?;
        if !self.database.starts_with("adventuresim-sim-")
            || !self
                .database
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err("database must be a unique adventuresim-sim-* disposable name".into());
        }
        if !(2..=32).contains(&self.population)
            || !(1..=10_000).contains(&self.cycles)
            || !(1..=MAX_DURATION_DAYS).contains(&self.duration_days)
            || !(2..=8).contains(&self.party_size)
            || self.party_size > self.population
        {
            return Err(format!(
                "population 2..=32, party_size 2..=8, cycles 1..=10000, and duration_days 1..={MAX_DURATION_DAYS} are required"
            ));
        }
        let work = u64::from(self.population)
            .checked_mul(u64::from(self.cycles))
            .ok_or("core-loop work overflow")?;
        if work > MAX_CORE_LOOP_WORK {
            return Err(format!(
                "population * cycles must be <= {MAX_CORE_LOOP_WORK}"
            ));
        }
        if !(16..=96).contains(&self.run_nonce.len())
            || !self
                .run_nonce
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err("run_nonce must be 16..=96 ASCII alphanumeric/dash characters".into());
        }
        if !SIMULATION_DISEASE_SCENARIOS.contains(&self.fixture_disease.as_str()) {
            return Err(format!(
                "fixture_disease must be one of {}",
                SIMULATION_DISEASE_SCENARIOS.join(", ")
            ));
        }
        if self.use_imported_world {
            if self.require_quest_coverage {
                return Err("quest coverage fixture cannot use an imported world".into());
            }
            let digest = self
                .expected_world_manifest_digest
                .as_deref()
                .ok_or("imported-world mode requires an expected manifest digest")?;
            if digest.len() != 64
                || !digest
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                return Err("expected world manifest digest must be 64 lowercase hex".into());
            }
        } else if self.expected_world_manifest_digest.is_some() {
            return Err("fixture mode cannot claim an expected world manifest".into());
        }
        if self.require_quest_coverage && self.population <= self.party_size {
            return Err("quest coverage fixture requires at least two parties".into());
        }
        Ok(())
    }
}

pub(super) fn validate_loopback_url(host: &str) -> Result<(), String> {
    let parsed = Url::parse(host).map_err(|error| format!("invalid SpacetimeDB URL: {error}"))?;
    if parsed.scheme() != "http"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !matches!(parsed.host_str(), Some("localhost" | "127.0.0.1" | "::1"))
    {
        return Err("host must be a credential-free http://localhost, 127.0.0.1, or [::1] origin with no path/query/fragment".into());
    }
    Ok(())
}

pub(super) fn bootstrap_token_from_environment(value: Option<String>) -> Result<String, String> {
    let token = value.ok_or_else(|| {
        format!(
            "{BOOTSTRAP_TOKEN_ENV} is required; use the disposable strategic-sim-core-loop recipe"
        )
    })?;
    if token.len() != BOOTSTRAP_TOKEN_HEX_LEN || !token.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(format!(
            "{BOOTSTRAP_TOKEN_ENV} must contain exactly 32 random bytes encoded as hexadecimal"
        ));
    }
    Ok(token)
}
