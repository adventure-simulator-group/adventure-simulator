use super::{
    ChoiceId, ChoiceKind, DecisionArguments, EVAL_FORMAT_VERSION, MAX_PROVIDER_RESPONSE_BYTES,
    PlayerFrame, PolicyClassification, PolicyDecision, PolicyRunMetadata,
};
use serde::Deserialize;
use std::time::Instant;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderDecisionError {
    ResponseTooLarge,
    MalformedJson(String),
    UnsupportedVersion,
    MalformedChoiceId,
    MalformedEnvelope,
    MissingChoice,
}

impl std::fmt::Display for ProviderDecisionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ResponseTooLarge => formatter.write_str("provider response exceeds byte budget"),
            Self::MalformedJson(error) => write!(formatter, "invalid provider JSON: {error}"),
            Self::UnsupportedVersion => {
                formatter.write_str("provider selected unsupported schema version")
            }
            Self::MalformedChoiceId => formatter.write_str("provider choice ID is malformed"),
            Self::MalformedEnvelope => {
                formatter.write_str("provider returned malformed response envelope")
            }
            Self::MissingChoice => formatter.write_str("provider response contained no choice"),
        }
    }
}

impl std::error::Error for ProviderDecisionError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicyError {
    DeadlineExceeded,
    InvalidResponse(ProviderDecisionError),
    Failure(String),
    RepairFailed {
        initial: ProviderDecisionError,
        repair: Box<PolicyError>,
    },
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeadlineExceeded => {
                formatter.write_str("quest evaluator wall-time budget exceeded")
            }
            Self::InvalidResponse(error) => error.fmt(formatter),
            Self::Failure(error) => formatter.write_str(error),
            Self::RepairFailed { initial, repair } => {
                write!(formatter, "{initial}; bounded repair failed: {repair}")
            }
        }
    }
}

impl std::error::Error for PolicyError {}

impl PolicyError {
    pub(crate) fn is_deadline_exceeded(&self) -> bool {
        match self {
            Self::DeadlineExceeded => true,
            Self::RepairFailed { repair, .. } => repair.is_deadline_exceeded(),
            Self::InvalidResponse(_) | Self::Failure(_) => false,
        }
    }
}

impl From<String> for PolicyError {
    fn from(error: String) -> Self {
        Self::Failure(error)
    }
}

impl From<&str> for PolicyError {
    fn from(error: &str) -> Self {
        Self::Failure(error.to_owned())
    }
}

pub trait QuestPolicy {
    fn name(&self) -> &str;
    fn decide(&mut self, frame: &PlayerFrame) -> Result<PolicyDecision, PolicyError>;
    fn classify(&mut self, _frame: &PlayerFrame) -> Result<PolicyClassification, PolicyError> {
        Ok(PolicyClassification::default())
    }
    fn run_metadata(&self) -> PolicyRunMetadata {
        PolicyRunMetadata {
            policy_name: self.name().into(),
            policy_kind: "local".into(),
            provider: None,
            model: None,
            prompt_revision: "investigation-policy-v4".into(),
            requests: 0,
            estimated_prompt_tokens: 0,
            estimated_completion_tokens: 0,
            estimated_cost_microusd: 0,
        }
    }
    /// Providers that can block on I/O override this to respect the evaluator's
    /// absolute deadline. Deterministic policies remain immediate.
    fn decide_before(
        &mut self,
        frame: &PlayerFrame,
        deadline: Instant,
    ) -> Result<PolicyDecision, PolicyError> {
        if Instant::now() >= deadline {
            return Err(PolicyError::DeadlineExceeded);
        }
        let decision = self.decide(frame)?;
        if Instant::now() >= deadline {
            return Err(PolicyError::DeadlineExceeded);
        }
        Ok(decision)
    }
}

#[derive(Default)]
pub struct ScriptedPolicy {
    pub prefer_alternate_route: bool,
}

impl QuestPolicy for ScriptedPolicy {
    fn name(&self) -> &str {
        "scripted-evidence-seeking-v1"
    }

    fn decide(&mut self, frame: &PlayerFrame) -> Result<PolicyDecision, PolicyError> {
        // Evidence and travel are preferred over a finale; this keeps the
        // baseline from succeeding merely because it was offered a conclusion.
        let priorities = [
            ChoiceKind::EnterTavern,
            ChoiceKind::InterviewWitness,
            ChoiceKind::Wait,
            ChoiceKind::Investigate,
            ChoiceKind::Prepare,
            ChoiceKind::Travel,
            ChoiceKind::Conclude,
        ];
        let choice = priorities
            .iter()
            .find_map(|kind| {
                let mut matching = frame
                    .legal_choices
                    .iter()
                    .filter(|choice| choice.kind == *kind);
                if self.prefer_alternate_route && *kind == ChoiceKind::Investigate {
                    matching.next_back()
                } else {
                    matching.next()
                }
            })
            .ok_or_else(|| PolicyError::Failure("no legal action".into()))?;
        Ok(PolicyDecision {
            version: EVAL_FORMAT_VERSION,
            choice_id: choice.choice_id.clone(),
            arguments: DecisionArguments::default(),
        })
    }

    fn classify(&mut self, frame: &PlayerFrame) -> Result<PolicyClassification, PolicyError> {
        let summary = frame.discovery.problem_summary.to_ascii_lowercase();
        let template_guess = if summary.contains("missing") || summary.contains("disappear") {
            Some("disappearance_or_loss".into())
        } else if summary.contains("attack") || summary.contains("livestock") {
            Some("recurring_depredation".into())
        } else {
            Some("unknown".into())
        };
        Ok(PolicyClassification {
            template_guess,
            threat_guess: Some("unknown".into()),
            confidence_percent: Some(25),
        })
    }
}

/// Credential-free fixture that exercises the exact strict JSON boundary used
/// by a remote model, while deterministically selecting a sensible legal move.
#[derive(Default)]
pub struct MockLlmPolicy;

impl QuestPolicy for MockLlmPolicy {
    fn name(&self) -> &str {
        "mock-llm-json-v1"
    }

    fn decide(&mut self, frame: &PlayerFrame) -> Result<PolicyDecision, PolicyError> {
        let mut baseline = ScriptedPolicy::default();
        let intended = baseline.decide(frame)?;
        let fixture_reply = serde_json::to_vec(&intended)
            .map_err(|error| PolicyError::Failure(error.to_string()))?;
        parse_provider_decision(&fixture_reply).map_err(PolicyError::InvalidResponse)
    }

    fn classify(&mut self, frame: &PlayerFrame) -> Result<PolicyClassification, PolicyError> {
        ScriptedPolicy::default().classify(frame)
    }

    fn run_metadata(&self) -> PolicyRunMetadata {
        PolicyRunMetadata {
            policy_name: self.name().into(),
            policy_kind: "mock_provider".into(),
            provider: Some("strict-json-fixture".into()),
            model: Some("deterministic-mock".into()),
            prompt_revision: "investigation-policy-v4".into(),
            requests: 0,
            estimated_prompt_tokens: 0,
            estimated_completion_tokens: 0,
            estimated_cost_microusd: 0,
        }
    }
}

pub fn parse_provider_decision(bytes: &[u8]) -> Result<PolicyDecision, ProviderDecisionError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ProviderDecisionWire {
        version: u32,
        choice_id: String,
        #[serde(default)]
        arguments: DecisionArguments,
    }

    if bytes.len() > MAX_PROVIDER_RESPONSE_BYTES {
        return Err(ProviderDecisionError::ResponseTooLarge);
    }
    let decision: ProviderDecisionWire = serde_json::from_slice(bytes)
        .map_err(|error| ProviderDecisionError::MalformedJson(error.to_string()))?;
    if decision.version != EVAL_FORMAT_VERSION {
        return Err(ProviderDecisionError::UnsupportedVersion);
    }
    let choice_id = ChoiceId::parse(decision.choice_id)
        .map_err(|_| ProviderDecisionError::MalformedChoiceId)?;
    Ok(PolicyDecision {
        version: decision.version,
        choice_id,
        arguments: decision.arguments,
    })
}

pub fn policy_prompt(frame: &PlayerFrame) -> Result<String, PolicyError> {
    let payload = serde_json::json!({
        "instruction": format!(
            "Select exactly one currently legal opaque choice_id. Return only strict JSON {{\"version\":{EVAL_FORMAT_VERSION},\"choice_id\":\"choice:<24 lowercase hex digits>\",\"arguments\":{{\"selection\":null}}}}."
        ),
        "untrusted_player_frame": frame,
    });
    serde_json::to_string(&payload).map_err(|error| PolicyError::Failure(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_rejects_unknown_fields_and_oversized_data() {
        assert!(matches!(
            parse_provider_decision(
                br#"{"version":4,"choice_id":"choice:x","arguments":{},"reducer":"admin"}"#
            ),
            Err(ProviderDecisionError::MalformedJson(_))
        ));
        assert_eq!(
            parse_provider_decision(&vec![b'x'; MAX_PROVIDER_RESPONSE_BYTES + 1]),
            Err(ProviderDecisionError::ResponseTooLarge)
        );
    }

    #[test]
    fn parser_rejects_arbitrary_object_ids() {
        assert_eq!(
            parse_provider_decision(
                br#"{"version":4,"choice_id":"case:canonical-secret","arguments":{}}"#
            ),
            Err(ProviderDecisionError::MalformedChoiceId)
        );
        assert_eq!(
            parse_provider_decision(br#"{"version":3,"choice_id":"choice:legacy","arguments":{}}"#),
            Err(ProviderDecisionError::UnsupportedVersion)
        );
    }

    #[test]
    fn repair_deadlines_remain_typed_budget_outcomes() {
        let error = PolicyError::RepairFailed {
            initial: ProviderDecisionError::MalformedChoiceId,
            repair: Box::new(PolicyError::DeadlineExceeded),
        };
        assert!(error.is_deadline_exceeded());
        assert!(!PolicyError::Failure("deadline wording".into()).is_deadline_exceeded());
    }

    #[test]
    fn prompt_has_no_raw_closing_delimiter() {
        let frame = crate::investigation_eval::InvestigationEnvironment::generate(
            crate::investigation_eval::EvalCaseConfig::fixture(
                3,
                adventuresim_core::quest_generation::TemplateFamily::RecurringDepredation,
            ),
        )
        .unwrap();
        let prompt = policy_prompt(frame.frame()).unwrap();
        assert!(!prompt.contains("<UNTRUSTED_GAME_DATA>"));
        assert!(prompt.contains("untrusted_player_frame"));
    }
}
