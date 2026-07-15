use crate::CodexError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;

pub const REQUIRED_MODEL: &str = "gpt-5.6-sol";
pub const REQUIRED_REASONING_EFFORT: &str = "high";
pub const MAX_PROMPT_BYTES: usize = 16 * 1024;
pub const MAX_SOURCE_REFS: usize = 64;
pub const MAX_SOURCE_REF_BYTES: usize = 128;
pub const MAX_MODELS: usize = 512;
pub const MAX_MODEL_PAGES: usize = 16;
pub const MAX_MODEL_ID_BYTES: usize = 128;
pub const MAX_MODEL_DISPLAY_NAME_BYTES: usize = 256;
pub const MAX_REASONING_EFFORTS: usize = 16;
pub const MAX_REASONING_EFFORT_BYTES: usize = 32;
pub const MAX_MODEL_CURSOR_BYTES: usize = 512;
pub const MAX_MODEL_CATALOG_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatGptLogin {
    pub auth_url: String,
    pub login_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state", deny_unknown_fields)]
pub enum AccountState {
    NotConnected,
    ChatGpt { email: String, plan_type: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GptModel {
    pub id: String,
    pub display_name: String,
    pub supported_reasoning_efforts: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutcomeRequest {
    pub prompt: String,
    pub allowed_source_refs: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StructuredOutcome {
    pub title: String,
    pub why_now: String,
    pub proposed_steps: Vec<String>,
    pub source_refs: Vec<String>,
}

impl OutcomeRequest {
    /// Validates the host-owned prompt and source-reference bounds before any
    /// app-server process or model operation is started.
    ///
    /// # Errors
    ///
    /// Returns an error for empty or oversized prompts and malformed refs.
    pub fn validate(&self) -> Result<(), CodexError> {
        if self.prompt.trim().is_empty() || self.prompt.len() > MAX_PROMPT_BYTES {
            return Err(CodexError::InvalidContract("invalid prompt length"));
        }
        if self.allowed_source_refs.len() > MAX_SOURCE_REFS {
            return Err(CodexError::InvalidContract("too many source refs"));
        }
        let mut unique = HashSet::new();
        for source_ref in &self.allowed_source_refs {
            if source_ref.is_empty()
                || source_ref.len() > MAX_SOURCE_REF_BYTES
                || !source_ref.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'-' | b'_' | b':' | b'.')
                })
                || !unique.insert(source_ref)
            {
                return Err(CodexError::InvalidContract("invalid source ref"));
            }
        }
        Ok(())
    }

    pub(crate) fn output_schema(&self) -> Value {
        let source_items = if self.allowed_source_refs.is_empty() {
            Value::Bool(false)
        } else {
            json!({"enum": self.allowed_source_refs, "type": "string"})
        };
        json!({
            "additionalProperties": false,
            "properties": {
                "proposedSteps": {
                    "items": {"maxLength": 240, "minLength": 1, "type": "string"},
                    "maxItems": 8,
                    "minItems": 1,
                    "type": "array"
                },
                "sourceRefs": {
                    "items": source_items,
                    "maxItems": self.allowed_source_refs.len(),
                    "type": "array",
                    "uniqueItems": true
                },
                "title": {"maxLength": 120, "minLength": 1, "type": "string"},
                "whyNow": {"maxLength": 300, "minLength": 1, "type": "string"}
            },
            "required": ["title", "whyNow", "proposedSteps", "sourceRefs"],
            "type": "object"
        })
    }
}

impl StructuredOutcome {
    pub(crate) fn parse_and_validate(
        text: &str,
        allowed_source_refs: &[String],
    ) -> Result<Self, CodexError> {
        let value: Self = serde_json::from_str(text)
            .map_err(|_| CodexError::InvalidContract("model output did not match schema"))?;
        if value.title.trim().is_empty()
            || value.title.chars().count() > 120
            || value.why_now.trim().is_empty()
            || value.why_now.chars().count() > 300
            || value.proposed_steps.is_empty()
            || value.proposed_steps.len() > 8
            || value
                .proposed_steps
                .iter()
                .any(|step| step.trim().is_empty() || step.chars().count() > 240)
        {
            return Err(CodexError::InvalidContract("invalid model output bounds"));
        }
        let allowed = allowed_source_refs.iter().collect::<HashSet<_>>();
        let mut seen = HashSet::new();
        if value
            .source_refs
            .iter()
            .any(|source_ref| !allowed.contains(source_ref) || !seen.insert(source_ref))
        {
            return Err(CodexError::InvalidContract("model forged a source ref"));
        }
        Ok(value)
    }
}

pub(crate) fn model_from_value(value: &Value) -> Result<Option<GptModel>, CodexError> {
    let hidden = value
        .get("hidden")
        .and_then(Value::as_bool)
        .ok_or(CodexError::Protocol("model.hidden missing"))?;
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .ok_or(CodexError::Protocol("model.model missing"))?;
    if hidden {
        return Ok(None);
    }
    if !model.starts_with("gpt-")
        || !model.is_ascii()
        || model.len() > MAX_MODEL_ID_BYTES
        || !model
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(CodexError::Protocol("invalid model id"));
    }
    let display_name = value
        .get("displayName")
        .and_then(Value::as_str)
        .ok_or(CodexError::Protocol("model.displayName missing"))?;
    if display_name.trim().is_empty() || display_name.len() > MAX_MODEL_DISPLAY_NAME_BYTES {
        return Err(CodexError::Protocol("invalid model display name"));
    }
    let raw_efforts = value
        .get("supportedReasoningEfforts")
        .and_then(Value::as_array)
        .ok_or(CodexError::Protocol(
            "model.supportedReasoningEfforts missing",
        ))?;
    if raw_efforts.len() > MAX_REASONING_EFFORTS {
        return Err(CodexError::Protocol("too many reasoning efforts"));
    }
    let mut unique = HashSet::new();
    let efforts = raw_efforts
        .iter()
        .map(|entry| {
            let effort = entry
                .get("reasoningEffort")
                .and_then(Value::as_str)
                .ok_or(CodexError::Protocol("reasoning effort missing"))?;
            if effort.is_empty()
                || effort.len() > MAX_REASONING_EFFORT_BYTES
                || !effort
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
                || !unique.insert(effort)
            {
                return Err(CodexError::Protocol("invalid reasoning effort"));
            }
            Ok(effort.to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(GptModel {
        id: model.to_owned(),
        display_name: display_name.to_owned(),
        supported_reasoning_efforts: efforts,
    }))
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_MODEL_DISPLAY_NAME_BYTES, OutcomeRequest, StructuredOutcome, model_from_value,
    };
    use serde_json::json;

    #[test]
    fn forged_or_duplicate_source_refs_fail_closed() {
        let allowed = vec!["typed:1".to_owned()];
        for output in [
            r#"{"title":"Do it","whyNow":"Now","proposedSteps":["One"],"sourceRefs":["typed:2"]}"#,
            r#"{"title":"Do it","whyNow":"Now","proposedSteps":["One"],"sourceRefs":["typed:1","typed:1"]}"#,
        ] {
            assert!(StructuredOutcome::parse_and_validate(output, &allowed).is_err());
        }
    }

    #[test]
    fn request_schema_enumerates_only_host_refs() {
        let request = OutcomeRequest {
            prompt: "Help me plan today".into(),
            allowed_source_refs: vec!["typed:1".into()],
        };
        request.validate().unwrap();
        assert_eq!(
            request.output_schema()["properties"]["sourceRefs"]["items"]["enum"][0],
            "typed:1"
        );
    }

    #[test]
    fn request_schema_forbids_items_when_no_source_refs_are_allowed() {
        let request = OutcomeRequest {
            prompt: "Help me plan today".into(),
            allowed_source_refs: Vec::new(),
        };
        request.validate().unwrap();
        assert_eq!(
            request.output_schema()["properties"]["sourceRefs"]["items"],
            false
        );
        assert_eq!(
            request.output_schema()["properties"]["sourceRefs"]["maxItems"],
            0
        );
    }

    #[test]
    fn model_fields_and_effort_catalog_are_strictly_bounded() {
        let oversized = json!({
            "displayName": "x".repeat(MAX_MODEL_DISPLAY_NAME_BYTES + 1),
            "hidden": false,
            "model": "gpt-5.6-sol",
            "supportedReasoningEfforts": [{"reasoningEffort": "high"}]
        });
        assert!(model_from_value(&oversized).is_err());

        let duplicate_effort = json!({
            "displayName": "GPT",
            "hidden": false,
            "model": "gpt-5.6-sol",
            "supportedReasoningEfforts": [
                {"reasoningEffort": "high"},
                {"reasoningEffort": "high"}
            ]
        });
        assert!(model_from_value(&duplicate_effort).is_err());
    }
}
