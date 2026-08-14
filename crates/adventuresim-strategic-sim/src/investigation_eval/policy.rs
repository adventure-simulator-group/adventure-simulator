use super::{
    ChoiceKind, DecisionArguments, EVAL_FORMAT_VERSION, MAX_PROVIDER_RESPONSE_BYTES, PlayerFrame,
    PolicyClassification, PolicyDecision, PolicyRunMetadata,
};
use std::time::Instant;

pub trait QuestPolicy {
    fn name(&self) -> &str;
    fn decide(&mut self, frame: &PlayerFrame) -> Result<PolicyDecision, String>;
    fn classify(&mut self, _frame: &PlayerFrame) -> Result<PolicyClassification, String> {
        Ok(PolicyClassification::default())
    }
    fn run_metadata(&self) -> PolicyRunMetadata {
        PolicyRunMetadata {
            policy_name: self.name().into(),
            policy_kind: "local".into(),
            provider: None,
            model: None,
            prompt_revision: "investigation-policy-v3".into(),
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
    ) -> Result<PolicyDecision, String> {
        if Instant::now() >= deadline {
            return Err("quest evaluator wall-time budget exceeded".into());
        }
        let decision = self.decide(frame)?;
        if Instant::now() >= deadline {
            return Err("quest evaluator wall-time budget exceeded".into());
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

    fn decide(&mut self, frame: &PlayerFrame) -> Result<PolicyDecision, String> {
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
            .ok_or("no legal action")?;
        Ok(PolicyDecision {
            version: EVAL_FORMAT_VERSION,
            choice_id: choice.choice_id.clone(),
            arguments: DecisionArguments::default(),
        })
    }

    fn classify(&mut self, frame: &PlayerFrame) -> Result<PolicyClassification, String> {
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

    fn decide(&mut self, frame: &PlayerFrame) -> Result<PolicyDecision, String> {
        let mut baseline = ScriptedPolicy::default();
        let intended = baseline.decide(frame)?;
        let fixture_reply = serde_json::to_vec(&intended).map_err(|error| error.to_string())?;
        parse_provider_decision(&fixture_reply)
    }

    fn classify(&mut self, frame: &PlayerFrame) -> Result<PolicyClassification, String> {
        ScriptedPolicy::default().classify(frame)
    }

    fn run_metadata(&self) -> PolicyRunMetadata {
        PolicyRunMetadata {
            policy_name: self.name().into(),
            policy_kind: "mock_provider".into(),
            provider: Some("strict-json-fixture".into()),
            model: Some("deterministic-mock".into()),
            prompt_revision: "investigation-policy-v3".into(),
            requests: 0,
            estimated_prompt_tokens: 0,
            estimated_completion_tokens: 0,
            estimated_cost_microusd: 0,
        }
    }
}

pub fn parse_provider_decision(bytes: &[u8]) -> Result<PolicyDecision, String> {
    if bytes.len() > MAX_PROVIDER_RESPONSE_BYTES {
        return Err("provider response exceeds byte budget".into());
    }
    let decision: PolicyDecision =
        serde_json::from_slice(bytes).map_err(|error| format!("invalid provider JSON: {error}"))?;
    if decision.version != EVAL_FORMAT_VERSION {
        return Err("provider selected unsupported schema version".into());
    }
    if decision.choice_id.len() > 128 || !decision.choice_id.starts_with("choice:") {
        return Err("provider choice ID is malformed".into());
    }
    Ok(decision)
}

pub fn policy_prompt(frame: &PlayerFrame) -> Result<String, String> {
    let payload = serde_json::json!({
        "instruction": format!(
            "Select exactly one currently legal opaque choice_id. Return only strict JSON {{\"version\":{EVAL_FORMAT_VERSION},\"choice_id\":\"choice:...\",\"arguments\":{{\"selection\":null}}}}."
        ),
        "untrusted_player_frame": frame,
    });
    serde_json::to_string(&payload).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_rejects_unknown_fields_and_oversized_data() {
        assert!(
            parse_provider_decision(
                br#"{"version":1,"choice_id":"choice:x","arguments":{},"reducer":"admin"}"#
            )
            .is_err()
        );
        assert!(parse_provider_decision(&vec![b'x'; MAX_PROVIDER_RESPONSE_BYTES + 1]).is_err());
    }

    #[test]
    fn parser_rejects_arbitrary_object_ids() {
        assert!(
            parse_provider_decision(
                br#"{"version":1,"choice_id":"case:canonical-secret","arguments":{}}"#
            )
            .is_err()
        );
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
