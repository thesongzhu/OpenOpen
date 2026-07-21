use crate::CodexError;
use openopen_protocol::PersonaRevisionRef;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

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
pub const MAX_DEVELOPER_INSTRUCTIONS_BYTES: usize = 16 * 1024;

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
    ChatGpt {
        email: String,
        #[serde(rename = "planType")]
        plan_type: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GptModel {
    pub id: String,
    pub display_name: String,
    pub supported_reasoning_efforts: Vec<String>,
}

/// An explicit user selection bound by Host to one verified model catalog.
/// `None` means the selected model has no user-configurable effort and is
/// persisted by the caller as `not_applicable`; it never asks the runtime to
/// choose an effort implicitly.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectedModel {
    pub model_id: String,
    pub reasoning_effort: Option<String>,
    pub catalog_fingerprint: String,
    pub catalog_revision: u64,
}

impl SelectedModel {
    /// Validates only the bounded wire representation. Catalog membership is
    /// checked immediately before the model thread starts.
    ///
    /// # Errors
    ///
    /// Returns an error when the selected model, effort, or catalog binding
    /// is not a bounded protocol value.
    pub fn validate(&self) -> Result<(), CodexError> {
        if self.model_id.is_empty()
            || self.model_id.len() > MAX_MODEL_ID_BYTES
            || !self
                .model_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(CodexError::InvalidContract("invalid selected model"));
        }
        if let Some(effort) = &self.reasoning_effort
            && (effort.is_empty()
                || effort.len() > MAX_REASONING_EFFORT_BYTES
                || !effort
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'-'))
        {
            return Err(CodexError::InvalidContract("invalid selected effort"));
        }
        if !is_sha256_hex(&self.catalog_fingerprint) || self.catalog_revision == 0 {
            return Err(CodexError::InvalidContract(
                "invalid selected model catalog",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutcomeRequest {
    pub prompt: String,
    pub allowed_source_refs: Vec<String>,
    pub selected_model: Option<SelectedModel>,
    pub persona_revision: PersonaRevisionRef,
    pub developer_instructions: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StructuredOutcome {
    pub title: String,
    pub why_now: String,
    pub proposed_steps: Vec<String>,
    pub source_refs: Vec<String>,
}

/// Host-owned request for the first dynamic Choice Loop result. The model may
/// describe bounded understanding and three directions, but it cannot supply
/// session, delivery, audit, provenance, or effect authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChoiceGenerationRequest {
    pub prompt: String,
    pub allowed_source_refs: Vec<String>,
    pub selected_model: Option<SelectedModel>,
    pub persona_revision: PersonaRevisionRef,
    pub developer_instructions: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StructuredChoiceOption {
    pub direction: String,
    pub rationale: String,
    pub expected_result: String,
    pub information_needed: Vec<String>,
    pub external_effects_preview: Vec<String>,
    pub source_categories: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StructuredChoiceGeneration {
    pub understood_goal: String,
    pub current_context: String,
    pub assumptions: Vec<String>,
    pub constraints: Vec<String>,
    pub uncertainties: Vec<String>,
    pub what_to_avoid: Vec<String>,
    pub options: Vec<StructuredChoiceOption>,
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
        if !self.persona_revision.is_valid()
            || self.developer_instructions.trim().is_empty()
            || self.developer_instructions.len() > MAX_DEVELOPER_INSTRUCTIONS_BYTES
            || !self
                .developer_instructions
                .contains(&self.persona_revision.persona_id)
            || !self
                .developer_instructions
                .contains(&self.persona_revision.revision)
            || !self
                .developer_instructions
                .contains(&self.persona_revision.aggregate_digest)
            || format!(
                "{:x}",
                Sha256::digest(self.developer_instructions.as_bytes())
            ) != self.persona_revision.instructions_digest
        {
            return Err(CodexError::InvalidContract("invalid persona binding"));
        }
        if let Some(selected_model) = &self.selected_model {
            selected_model.validate()?;
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
                    "type": "array"
                },
                "title": {"maxLength": 120, "minLength": 1, "type": "string"},
                "whyNow": {"maxLength": 300, "minLength": 1, "type": "string"}
            },
            "required": ["title", "whyNow", "proposedSteps", "sourceRefs"],
            "type": "object"
        })
    }
}

impl ChoiceGenerationRequest {
    /// # Errors
    ///
    /// Returns an error when the host-owned prompt, source-reference list, or
    /// exact selected-model binding exceeds the sealed contract.
    pub fn validate(&self) -> Result<(), CodexError> {
        OutcomeRequest {
            prompt: self.prompt.clone(),
            allowed_source_refs: self.allowed_source_refs.clone(),
            selected_model: self.selected_model.clone(),
            persona_revision: self.persona_revision.clone(),
            developer_instructions: self.developer_instructions.clone(),
        }
        .validate()
    }

    pub(crate) fn output_schema(&self) -> Value {
        let source_items = if self.allowed_source_refs.is_empty() {
            Value::Bool(false)
        } else {
            json!({"enum": self.allowed_source_refs, "type": "string"})
        };
        let text = |maximum| json!({"maxLength": maximum, "minLength": 1, "type": "string"});
        let string_list = |maximum_items, maximum_length| json!({"items": {"maxLength": maximum_length, "minLength": 1, "type": "string"}, "maxItems": maximum_items, "type": "array"});
        json!({
            "additionalProperties": false,
            "properties": {
                "understoodGoal": text(1024), "currentContext": text(2048),
                "assumptions": string_list(64, 1024), "constraints": string_list(64, 1024),
                "uncertainties": string_list(64, 1024), "whatToAvoid": string_list(64, 1024),
                "options": {"type": "array", "minItems": 3, "maxItems": 3, "items": {"type": "object", "additionalProperties": false, "required": ["direction", "rationale", "expectedResult", "informationNeeded", "externalEffectsPreview", "sourceCategories"], "properties": {"direction": text(512), "rationale": text(1024), "expectedResult": text(1024), "informationNeeded": string_list(16, 512), "externalEffectsPreview": string_list(16, 512), "sourceCategories": string_list(16, 128)}}},
                "sourceRefs": {"items": source_items, "maxItems": self.allowed_source_refs.len(), "type": "array"}
            },
            "required": ["understoodGoal", "currentContext", "assumptions", "constraints", "uncertainties", "whatToAvoid", "options", "sourceRefs"], "type": "object"
        })
    }
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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

impl StructuredChoiceGeneration {
    pub(crate) fn parse_and_validate(
        text: &str,
        allowed_source_refs: &[String],
    ) -> Result<Self, CodexError> {
        let value: Self = serde_json::from_str(text)
            .map_err(|_| CodexError::InvalidContract("model output did not match choice schema"))?;
        let valid_text = |text: &str, maximum: usize| {
            !text.trim().is_empty() && text.is_ascii() && text.chars().count() <= maximum
        };
        let valid_list = |values: &[String], maximum_items: usize, maximum: usize| {
            values.len() <= maximum_items && values.iter().all(|value| valid_text(value, maximum))
        };
        if !valid_text(&value.understood_goal, 1024)
            || !valid_text(&value.current_context, 2048)
            || !valid_list(&value.assumptions, 64, 1024)
            || !valid_list(&value.constraints, 64, 1024)
            || !valid_list(&value.uncertainties, 64, 1024)
            || !valid_list(&value.what_to_avoid, 64, 1024)
            || value.options.len() != 3
            || value.options.iter().any(|option| {
                !valid_text(&option.direction, 512)
                    || !valid_text(&option.rationale, 1024)
                    || !valid_text(&option.expected_result, 1024)
                    || !valid_list(&option.information_needed, 16, 512)
                    || !valid_list(&option.external_effects_preview, 16, 512)
                    || !valid_list(&option.source_categories, 16, 128)
            })
        {
            return Err(CodexError::InvalidContract("invalid choice output bounds"));
        }
        let directions = value
            .options
            .iter()
            .map(|option| option.direction.as_str())
            .collect::<HashSet<_>>();
        if directions.len() != 3 {
            return Err(CodexError::InvalidContract(
                "choice directions were not distinct",
            ));
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
        AccountState, ChoiceGenerationRequest, MAX_MODEL_DISPLAY_NAME_BYTES, OutcomeRequest,
        SelectedModel, StructuredChoiceGeneration, StructuredOutcome, is_sha256_hex,
        model_from_value,
    };
    use openopen_protocol::PersonaRevisionRef;
    use serde_json::json;
    use sha2::{Digest, Sha256};

    fn persona_revision() -> PersonaRevisionRef {
        PersonaRevisionRef {
            persona_id: "openopen.nondev.default".into(),
            revision: "draft-03-en".into(),
            aggregate_digest: "b".repeat(64),
            instructions_digest: format!("{:x}", Sha256::digest(persona_instructions())),
        }
    }

    fn persona_instructions() -> String {
        format!(
            "OpenOpen persona openopen.nondev.default / draft-03-en; aggregate={}. Return only the requested JSON.",
            "b".repeat(64)
        )
    }

    #[test]
    fn connected_account_uses_the_swift_camel_case_contract() {
        let value = serde_json::to_value(AccountState::ChatGpt {
            email: "owner@example.invalid".to_owned(),
            plan_type: "pro".to_owned(),
        })
        .unwrap();

        assert_eq!(
            value,
            json!({
                "email": "owner@example.invalid",
                "planType": "pro",
                "state": "chatGpt"
            })
        );
        assert!(value.get("plan_type").is_none());
    }

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
    fn choice_generation_requires_three_distinct_bounded_options_and_host_refs() {
        let request = ChoiceGenerationRequest {
            prompt: "Plan one bounded local task".into(),
            allowed_source_refs: vec!["local:intake".into()],
            selected_model: Some(SelectedModel {
                model_id: "gpt-example".into(),
                reasoning_effort: Some("high".into()),
                catalog_fingerprint: "a".repeat(64),
                catalog_revision: 1,
            }),
            persona_revision: persona_revision(),
            developer_instructions: persona_instructions(),
        };
        request.validate().unwrap();
        assert_eq!(
            request.output_schema()["properties"]["options"]["minItems"],
            3
        );
        let output = r#"{"understoodGoal":"Plan safely","currentContext":"One local question","assumptions":[],"constraints":[],"uncertainties":[],"whatToAvoid":[],"options":[{"direction":"Review","rationale":"Bound scope","expectedResult":"A plan","informationNeeded":[],"externalEffectsPreview":[],"sourceCategories":["ownerInput"]},{"direction":"Narrow","rationale":"Reduce uncertainty","expectedResult":"A smaller plan","informationNeeded":[],"externalEffectsPreview":[],"sourceCategories":["ownerInput"]},{"direction":"Prepare backup","rationale":"Keep an alternative","expectedResult":"A safe alternative","informationNeeded":[],"externalEffectsPreview":[],"sourceCategories":["ownerInput"]}],"sourceRefs":["local:intake"]}"#;
        assert!(
            StructuredChoiceGeneration::parse_and_validate(output, &request.allowed_source_refs)
                .is_ok()
        );
        assert!(
            StructuredChoiceGeneration::parse_and_validate(
                &output.replace("\"Narrow\"", "\"Review\""),
                &request.allowed_source_refs
            )
            .is_err()
        );
        assert!(
            StructuredChoiceGeneration::parse_and_validate(
                &output.replace("local:intake", "forged:source"),
                &request.allowed_source_refs
            )
            .is_err()
        );
        assert!(
            StructuredChoiceGeneration::parse_and_validate(
                &output.replace("Plan safely", "计划"),
                &request.allowed_source_refs
            )
            .is_err()
        );
    }

    #[test]
    fn model_request_rejects_a_persona_digest_not_bound_into_instructions() {
        let mut request = ChoiceGenerationRequest {
            prompt: "Plan one bounded local task".into(),
            allowed_source_refs: vec!["local:intake".into()],
            selected_model: None,
            persona_revision: persona_revision(),
            developer_instructions: persona_instructions(),
        };
        request.validate().unwrap();
        request.persona_revision.aggregate_digest = "c".repeat(64);
        assert!(request.validate().is_err());
    }

    #[test]
    fn model_request_rejects_compiler_output_that_differs_from_the_bound_digest() {
        let mut request = ChoiceGenerationRequest {
            prompt: "Plan one bounded local task".into(),
            allowed_source_refs: vec!["local:intake".into()],
            selected_model: None,
            persona_revision: persona_revision(),
            developer_instructions: persona_instructions(),
        };
        request.validate().unwrap();
        request
            .developer_instructions
            .push_str(" Changed compiler output.");
        assert!(request.validate().is_err());
    }

    #[test]
    fn request_schema_enumerates_only_host_refs() {
        let request = OutcomeRequest {
            prompt: "Help me plan today".into(),
            allowed_source_refs: vec!["typed:1".into()],
            selected_model: Some(SelectedModel {
                model_id: "gpt-example".into(),
                reasoning_effort: None,
                catalog_fingerprint: "a".repeat(64),
                catalog_revision: 1,
            }),
            persona_revision: persona_revision(),
            developer_instructions: persona_instructions(),
        };
        request.validate().unwrap();
        assert_eq!(
            request.output_schema()["properties"]["sourceRefs"]["items"]["enum"][0],
            "typed:1"
        );
        assert!(
            request.output_schema()["properties"]["sourceRefs"]
                .get("uniqueItems")
                .is_none(),
            "Codex structured output supports only its documented JSON Schema subset; duplicate refs remain rejected after parsing"
        );
    }

    #[test]
    fn request_schema_forbids_items_when_no_source_refs_are_allowed() {
        let request = OutcomeRequest {
            prompt: "Help me plan today".into(),
            allowed_source_refs: Vec::new(),
            selected_model: Some(SelectedModel {
                model_id: "gpt-example".into(),
                reasoning_effort: None,
                catalog_fingerprint: "a".repeat(64),
                catalog_revision: 1,
            }),
            persona_revision: persona_revision(),
            developer_instructions: persona_instructions(),
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
    fn selected_catalog_digest_rejects_non_hex_lowercase_letters() {
        assert!(is_sha256_hex(&"abcdef0123456789".repeat(4)));
        assert!(!is_sha256_hex(&format!("g{}", "a".repeat(63))));
    }

    #[test]
    fn model_fields_and_effort_catalog_are_strictly_bounded() {
        let oversized = json!({
            "displayName": "x".repeat(MAX_MODEL_DISPLAY_NAME_BYTES + 1),
            "hidden": false,
            "model": "gpt-test-model",
            "supportedReasoningEfforts": [{"reasoningEffort": "high"}]
        });
        assert!(model_from_value(&oversized).is_err());

        let duplicate_effort = json!({
            "displayName": "GPT",
            "hidden": false,
            "model": "gpt-test-model",
            "supportedReasoningEfforts": [
                {"reasoningEffort": "high"},
                {"reasoningEffort": "high"}
            ]
        });
        assert!(model_from_value(&duplicate_effort).is_err());
    }
}
