//! Verified, non-executable conversation behavior for `OpenOpen`.
//!
//! Persona data may shape language and routing presentation only. This crate
//! deliberately has no tool, provider, model-selection, permission, memory,
//! recipient, retention, or effect interface.

use openopen_protocol::{
    ConversationAmbiguityClass, ConversationChoice, ConversationContext, ConversationDecision,
    ConversationRiskClass, ConversationSurface, ConversationTurnKind, PersonaRevisionRef,
    RenderedMessage,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
#[cfg(test)]
use std::fs::{File, OpenOptions};
use std::io;
#[cfg(test)]
use std::io::Write;
#[cfg(test)]
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
#[cfg(test)]
use std::process::Command;
use thiserror::Error;

pub const DEFAULT_PERSONA_ID: &str = "openopen.nondev.default";
pub const DEFAULT_REVISION: &str = "draft-03-en";
pub const PERSONA_SCHEMA_VERSION: u32 = 1;
pub const MAX_BUNDLE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 32 * 1024;
const MAX_POLICY_BYTES: u64 = 128 * 1024;
const MAX_MESSAGES_BYTES: u64 = 128 * 1024;
const MAX_SCENARIOS_BYTES: u64 = 256 * 1024;
const MAX_EVALS_BYTES: u64 = 1024 * 1024;
const ALLOWED_CONTENT_FILES: [&str; 5] = [
    "manifest.json",
    "persona.json",
    "messages.en.json",
    "scenarios.json",
    "evals.jsonl",
];

const EMBEDDED_MANIFEST: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../resources/persona/openopen.nondev.default/draft-03-en/manifest.json"
));
const EMBEDDED_POLICY: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../resources/persona/openopen.nondev.default/draft-03-en/persona.json"
));
const EMBEDDED_MESSAGES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../resources/persona/openopen.nondev.default/draft-03-en/messages.en.json"
));
const EMBEDDED_SCENARIOS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../resources/persona/openopen.nondev.default/draft-03-en/scenarios.json"
));
const EMBEDDED_EVALS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../resources/persona/openopen.nondev.default/draft-03-en/evals.jsonl"
));

#[derive(Debug, Error)]
pub enum PersonaError {
    #[error("persona bundle is not canonical")]
    NonCanonicalManifest,
    #[error("persona bundle layout is invalid")]
    InvalidLayout,
    #[error("persona bundle contains an invalid field")]
    InvalidContract,
    #[error("persona bundle digest does not match")]
    DigestMismatch,
    #[error("persona bundle is too large")]
    Oversized,
    #[error("persona bundle is incompatible with this OpenOpen Host")]
    Incompatible,
    #[error("persona bundle is not signed by the approved Developer ID team")]
    InvalidSignature,
    #[error("persona revision would downgrade the active behavior")]
    Downgrade,
    #[error("persona activation confirmation is invalid or already used")]
    InvalidConfirmation,
    #[error("persona revision changed after staging")]
    ChangedAfterStaging,
    #[error("persona revision was not previously verified")]
    UnknownRevision,
    #[error("persona storage failed")]
    Io(#[from] io::Error),
    #[error("persona data could not be decoded")]
    Json(#[from] serde_json::Error),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonaManifest {
    pub schema_version: u32,
    pub persona_id: String,
    pub revision: String,
    pub locale: String,
    pub minimum_host_protocol: u64,
    pub files: BTreeMap<String, String>,
    pub change_summary: String,
    pub signer_identity: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonaVoice {
    pub warmth: String,
    pub recommendation: String,
    pub humor: String,
    pub greeting: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
// These independently persisted safety boundaries intentionally remain
// separate booleans in the signed JSON contract.
#[allow(clippy::struct_excessive_bools)]
pub struct PersonaBoundaries {
    pub attachment_claims: bool,
    pub relationship_maintenance: bool,
    pub fabricated_experience: bool,
    pub inferred_durable_preferences: bool,
    pub technical_details_by_default: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResponseShape {
    pub minimum_sentences: u8,
    pub maximum_sentences: u8,
    pub maximum_focused_questions: u8,
    pub one_useful_next_step: bool,
    pub answer_first: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AmbiguityPolicy {
    pub small_reversible: String,
    pub material_preview: String,
    pub material_outcome: String,
    pub external_effect: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AdaptationPolicy {
    pub per_turn: bool,
    pub session_preference_immediate: bool,
    pub durable_preference_requires_confirmation: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProactivePolicy {
    pub task_grounded_only: bool,
    pub allowed_events: Vec<String>,
    pub relationship_checkins: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProgressPolicy {
    pub initial_working_after_seconds: u64,
    pub first_unchanged_liveness_minutes: u64,
    pub subsequent_unchanged_liveness_minutes: u64,
    pub fake_percentages: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonaPolicy {
    pub voice: PersonaVoice,
    pub boundaries: PersonaBoundaries,
    pub response_shape: ResponseShape,
    pub ambiguity_policy: AmbiguityPolicy,
    pub adaptation: AdaptationPolicy,
    pub proactive_policy: ProactivePolicy,
    pub progress_policy: ProgressPolicy,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageCatalog(pub BTreeMap<String, String>);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScenarioDefinition {
    pub id: String,
    pub decision: ConversationDecision,
    pub hard_constraints: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvalCase {
    pub id: String,
    pub category: String,
    #[serde(default)]
    pub scenario: Option<String>,
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub suite: Option<String>,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub turns: Option<Vec<String>>,
    #[serde(default)]
    pub expected_decision: Option<ConversationDecision>,
    pub hard_constraints: Vec<String>,
}

#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedSigner {
    pub team_identifier: String,
    pub signing_identifier: String,
}

#[cfg(test)]
pub trait CodeSignatureVerifier {
    /// # Errors
    ///
    /// Returns an error when the bundle cannot be verified by the expected
    /// Developer ID identity.
    fn verify(&self, bundle_path: &Path) -> Result<VerifiedSigner, PersonaError>;
}

/// Verifies a resource bundle against an exact Developer ID Application team.
#[cfg(test)]
pub struct MacDeveloperIdVerifier {
    expected_team_identifier: String,
}

#[cfg(test)]
impl MacDeveloperIdVerifier {
    /// # Errors
    ///
    /// Returns an error when `expected_team_identifier` is not a bounded
    /// Developer ID team identifier.
    pub fn new(expected_team_identifier: String) -> Result<Self, PersonaError> {
        if !valid_team_identifier(&expected_team_identifier) {
            return Err(PersonaError::InvalidSignature);
        }
        Ok(Self {
            expected_team_identifier,
        })
    }
}

#[cfg(test)]
impl CodeSignatureVerifier for MacDeveloperIdVerifier {
    fn verify(&self, bundle_path: &Path) -> Result<VerifiedSigner, PersonaError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = bundle_path;
            return Err(PersonaError::InvalidSignature);
        }
        #[cfg(target_os = "macos")]
        {
            let requirement = format!(
                "=anchor apple generic and certificate leaf[field.1.2.840.113635.100.6.1.13] /* exists */ and certificate leaf[subject.OU] = \"{}\"",
                self.expected_team_identifier
            );
            let status = Command::new("/usr/bin/codesign")
                .args(["--verify", "--strict", "-R", &requirement])
                .arg(bundle_path)
                .status()
                .map_err(|_| PersonaError::InvalidSignature)?;
            if !status.success() {
                return Err(PersonaError::InvalidSignature);
            }
            let output = Command::new("/usr/bin/codesign")
                .args(["-d", "--verbose=4"])
                .arg(bundle_path)
                .output()
                .map_err(|_| PersonaError::InvalidSignature)?;
            if !output.status.success() {
                return Err(PersonaError::InvalidSignature);
            }
            let details = String::from_utf8_lossy(&output.stderr);
            let team_identifier =
                signing_value(&details, "TeamIdentifier").ok_or(PersonaError::InvalidSignature)?;
            let signing_identifier =
                signing_value(&details, "Identifier").ok_or(PersonaError::InvalidSignature)?;
            if team_identifier != self.expected_team_identifier {
                return Err(PersonaError::InvalidSignature);
            }
            Ok(VerifiedSigner {
                team_identifier,
                signing_identifier,
            })
        }
    }
}

#[derive(Clone, Debug)]
pub struct PersonaBundle {
    pub manifest: PersonaManifest,
    pub policy: PersonaPolicy,
    pub messages: MessageCatalog,
    pub scenarios: Vec<ScenarioDefinition>,
    pub evals: Vec<EvalCase>,
    pub revision_ref: PersonaRevisionRef,
    #[cfg(test)]
    raw_files: BTreeMap<String, Vec<u8>>,
}

impl PersonaBundle {
    /// # Errors
    ///
    /// Returns an error when the embedded bundle is malformed, exceeds its
    /// bounds, or does not match its signed content digests.
    pub fn embedded_default(host_protocol: u64) -> Result<Self, PersonaError> {
        let files = BTreeMap::from([
            ("manifest.json".to_owned(), EMBEDDED_MANIFEST.to_vec()),
            ("persona.json".to_owned(), EMBEDDED_POLICY.to_vec()),
            ("messages.en.json".to_owned(), EMBEDDED_MESSAGES.to_vec()),
            ("scenarios.json".to_owned(), EMBEDDED_SCENARIOS.to_vec()),
            ("evals.jsonl".to_owned(), EMBEDDED_EVALS.to_vec()),
        ]);
        Self::from_files(files, host_protocol)
    }

    /// # Errors
    ///
    /// Returns an error when the directory is not an exact regular bundle or
    /// its contents do not satisfy the Persona contract.
    pub fn load_embedded(path: &Path, host_protocol: u64) -> Result<Self, PersonaError> {
        validate_bundle_layout(path, false)?;
        Self::from_files(read_content_files(path)?, host_protocol)
    }

    /// # Errors
    ///
    /// Returns an error when the bundle layout, signature, identity, or
    /// content contract is invalid.
    #[cfg(test)]
    pub fn load_update(
        path: &Path,
        host_protocol: u64,
        verifier: &dyn CodeSignatureVerifier,
    ) -> Result<(Self, VerifiedSigner), PersonaError> {
        validate_bundle_layout(path, true)?;
        let signer = verifier.verify(path)?;
        let bundle = Self::from_files(read_content_files(path)?, host_protocol)?;
        if signer.signing_identifier != bundle.manifest.signer_identity {
            return Err(PersonaError::InvalidSignature);
        }
        Ok((bundle, signer))
    }

    #[allow(clippy::needless_pass_by_value)] // test-only verified-bundle materialization owns the exact bytes.
    fn from_files(
        raw_files: BTreeMap<String, Vec<u8>>,
        host_protocol: u64,
    ) -> Result<Self, PersonaError> {
        let total = raw_files.values().map(Vec::len).sum::<usize>();
        if total as u64 > MAX_BUNDLE_BYTES || raw_files.len() != ALLOWED_CONTENT_FILES.len() {
            return Err(PersonaError::Oversized);
        }
        let manifest_bytes = required_file(&raw_files, "manifest.json", MAX_MANIFEST_BYTES)?;
        let manifest: PersonaManifest = serde_json::from_slice(manifest_bytes)?;
        let canonical_manifest = serde_json::to_vec(&manifest)?;
        if manifest_bytes != canonical_manifest.as_slice()
            && manifest_bytes != [canonical_manifest.as_slice(), b"\n"].concat().as_slice()
        {
            return Err(PersonaError::NonCanonicalManifest);
        }
        validate_manifest(&manifest, host_protocol)?;
        for (name, expected_digest) in &manifest.files {
            let maximum = file_limit(name).ok_or(PersonaError::InvalidLayout)?;
            let content = required_file(&raw_files, name, maximum)?;
            if sha256_hex(content) != *expected_digest {
                return Err(PersonaError::DigestMismatch);
            }
        }
        let policy: PersonaPolicy =
            serde_json::from_slice(required_file(&raw_files, "persona.json", MAX_POLICY_BYTES)?)?;
        validate_policy(&policy)?;
        let messages: BTreeMap<String, String> = serde_json::from_slice(required_file(
            &raw_files,
            "messages.en.json",
            MAX_MESSAGES_BYTES,
        )?)?;
        validate_messages(&messages)?;
        let scenarios: Vec<ScenarioDefinition> = serde_json::from_slice(required_file(
            &raw_files,
            "scenarios.json",
            MAX_SCENARIOS_BYTES,
        )?)?;
        validate_scenarios(&scenarios)?;
        let evals = parse_evals(required_file(&raw_files, "evals.jsonl", MAX_EVALS_BYTES)?)?;
        validate_evals(&evals, &scenarios)?;
        let aggregate_digest = sha256_hex(&canonical_manifest);
        let revision_ref = PersonaRevisionRef {
            persona_id: manifest.persona_id.clone(),
            revision: manifest.revision.clone(),
            aggregate_digest,
            // The deterministic compiler below does not read this field, so
            // it can be sealed from its exact output after the bundle exists.
            instructions_digest: String::new(),
        };
        let mut bundle = Self {
            manifest,
            policy,
            messages: MessageCatalog(messages),
            scenarios,
            evals,
            revision_ref,
            #[cfg(test)]
            raw_files,
        };
        bundle.revision_ref.instructions_digest =
            sha256_hex(bundle.developer_instructions(true)?.as_bytes());
        bundle
            .revision_ref
            .is_valid()
            .then_some(bundle)
            .ok_or(PersonaError::InvalidContract)
    }

    /// Compiles the complete verified behavior bundle into the only Persona
    /// instruction payload allowed to reach the model. The format is
    /// deliberately deterministic: policy, message catalog, scenarios, and
    /// evaluation commitments are all represented, so a verified revision
    /// cannot be audit-only metadata that leaves model behavior unchanged.
    ///
    /// # Errors
    ///
    /// Returns an error if the verified bundle cannot fit within the bounded
    /// Codex instruction contract.
    pub fn developer_instructions(
        &self,
        structured_output_only: bool,
    ) -> Result<String, PersonaError> {
        const MAX_DEVELOPER_INSTRUCTIONS_BYTES: usize = 16 * 1024;
        let policy = &self.policy;
        let mut instructions = format!(
            "OpenOpen Persona Contract\nPersona id: {}\nPersona revision: {}\nPersona aggregate digest: {}\nLocale: {}\nPolicy voice: warmth={}; recommendation={}; humor={}; greeting={}.\nPolicy response shape: minimumSentences={}; maximumSentences={}; maximumFocusedQuestions={}; oneUsefulNextStep={}; answerFirst={}.\nPolicy boundaries: attachmentClaims={}; relationshipMaintenance={}; fabricatedExperience={}; inferredDurablePreferences={}; technicalDetailsByDefault={}.\nPolicy ambiguity: smallReversible={}; materialPreview={}; materialOutcome={}; externalEffect={}.\nPolicy adaptation: perTurn={}; sessionPreferenceImmediate={}; durablePreferenceRequiresConfirmation={}.\nPolicy proactive: taskGroundedOnly={}; allowedEvents={}; relationshipCheckins={}.\nPolicy progress: initialWorkingAfterSeconds={}; firstUnchangedLivenessMinutes={}; subsequentUnchangedLivenessMinutes={}; fakePercentages={}.\nThe contract never grants tools, permissions, recipients, memory, model selection, retention, or effects. Never infer a missing Reminder time; an external or irreversible action requires an exact editable preview and separate action-time confirmation.\nVerified message catalog (use only when the corresponding Host-owned state applies):\n",
            self.revision_ref.persona_id,
            self.revision_ref.revision,
            self.revision_ref.aggregate_digest,
            self.manifest.locale,
            policy.voice.warmth,
            policy.voice.recommendation,
            policy.voice.humor,
            policy.voice.greeting,
            policy.response_shape.minimum_sentences,
            policy.response_shape.maximum_sentences,
            policy.response_shape.maximum_focused_questions,
            policy.response_shape.one_useful_next_step,
            policy.response_shape.answer_first,
            policy.boundaries.attachment_claims,
            policy.boundaries.relationship_maintenance,
            policy.boundaries.fabricated_experience,
            policy.boundaries.inferred_durable_preferences,
            policy.boundaries.technical_details_by_default,
            policy.ambiguity_policy.small_reversible,
            policy.ambiguity_policy.material_preview,
            policy.ambiguity_policy.material_outcome,
            policy.ambiguity_policy.external_effect,
            policy.adaptation.per_turn,
            policy.adaptation.session_preference_immediate,
            policy.adaptation.durable_preference_requires_confirmation,
            policy.proactive_policy.task_grounded_only,
            policy.proactive_policy.allowed_events.join(","),
            policy.proactive_policy.relationship_checkins,
            policy.progress_policy.initial_working_after_seconds,
            policy.progress_policy.first_unchanged_liveness_minutes,
            policy.progress_policy.subsequent_unchanged_liveness_minutes,
            policy.progress_policy.fake_percentages,
        );
        for (key, value) in &self.messages.0 {
            instructions.push_str(key);
            instructions.push_str(" = ");
            instructions.push_str(value);
            instructions.push('\n');
        }
        instructions.push_str("Verified scenario obligations:\n");
        let mut scenarios = self.scenarios.iter().collect::<Vec<_>>();
        scenarios.sort_by(|left, right| left.id.cmp(&right.id));
        for scenario in scenarios {
            instructions.push_str(&scenario.id);
            instructions.push_str(": decision=");
            instructions.push_str(&serde_json::to_string(&scenario.decision)?);
            instructions.push_str("; constraints=");
            instructions.push_str(&scenario.hard_constraints.join(","));
            instructions.push('\n');
        }
        instructions
            .push_str("Verified complete evaluation commitments (ordered, no deduplication): ");
        instructions.push_str(&evaluation_commitments(&self.evals)?);
        instructions.push('\n');
        if structured_output_only {
            instructions.push_str(" Return only the requested JSON. Do not invoke tools.");
        }
        (instructions.len() <= MAX_DEVELOPER_INSTRUCTIONS_BYTES)
            .then_some(instructions)
            .ok_or(PersonaError::Oversized)
    }

    #[cfg(test)]
    fn write_to(&self, path: &Path) -> Result<(), PersonaError> {
        create_private_directory(path)?;
        for (name, content) in &self.raw_files {
            write_private_file(&path.join(name), content)?;
        }
        sync_directory(path)?;
        Ok(())
    }

    pub fn message(&self, key: &str) -> Option<&str> {
        self.messages.0.get(key).map(String::as_str)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonaStatus {
    pub active: PersonaRevisionRef,
    pub staged: Option<PersonaRevisionRef>,
    pub warning: Option<String>,
    pub change_note_pending: bool,
}

#[cfg(test)]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonaDiff {
    pub active: PersonaRevisionRef,
    pub staged: PersonaRevisionRef,
    pub change_summary: String,
    pub changed_files: Vec<String>,
}

#[cfg(test)]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersonaActivationConfirmation {
    pub expected_digest: String,
    pub confirmation_nonce: String,
    pub confirmed_at_ms: i64,
}

#[cfg(test)]
impl PersonaActivationConfirmation {
    fn is_valid(&self) -> bool {
        is_sha256_string(&self.expected_digest)
            && valid_identifier(&self.confirmation_nonce, 128)
            && self.confirmed_at_ms >= 0
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredRevision {
    revision_ref: PersonaRevisionRef,
    change_summary: String,
    signer_team_identifier: String,
    signer_identity: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RegistryState {
    active_digest: String,
    staged_digest: Option<String>,
    verified: BTreeMap<String, StoredRevision>,
    used_confirmation_nonces: BTreeSet<String>,
    warning: Option<String>,
    change_note_pending: bool,
}

pub struct PersonaManager {
    #[cfg(test)]
    root: PathBuf,
    #[cfg(test)]
    host_protocol: u64,
    #[cfg(test)]
    expected_team_identifier: String,
    state: RegistryState,
    active_bundle: PersonaBundle,
}

impl PersonaManager {
    /// Opens the embedded default-only Persona view used by PR1.
    ///
    /// This deliberately does not read or rewrite a mutable lifecycle
    /// registry. A later reviewed lifecycle can retain its staged data, but
    /// it cannot alter the PR1 Host's conversation behavior merely by leaving
    /// state on disk.
    ///
    /// # Errors
    ///
    /// Returns an error when private storage or the embedded Persona bundle
    /// cannot be initialized safely.
    // The test-only lifecycle checks retain ownership of this value while the
    // production default-only build deliberately has no mutable lifecycle.
    #[allow(clippy::needless_pass_by_value)]
    pub fn open_default_only(
        root: &Path,
        host_protocol: u64,
        expected_team_identifier: String,
    ) -> Result<Self, PersonaError> {
        #[cfg(not(test))]
        let _ = root;
        #[cfg(not(test))]
        let _ = expected_team_identifier;
        let embedded = PersonaBundle::embedded_default(host_protocol)?;
        let mut verified = BTreeMap::new();
        verified.insert(
            embedded.revision_ref.aggregate_digest.clone(),
            StoredRevision {
                revision_ref: embedded.revision_ref.clone(),
                change_summary: embedded.manifest.change_summary.clone(),
                signer_team_identifier: "embedded-app-signature".to_owned(),
                signer_identity: embedded.manifest.signer_identity.clone(),
            },
        );
        Ok(Self {
            #[cfg(test)]
            root: root.to_path_buf(),
            #[cfg(test)]
            host_protocol,
            #[cfg(test)]
            expected_team_identifier,
            state: RegistryState {
                active_digest: embedded.revision_ref.aggregate_digest.clone(),
                staged_digest: None,
                verified,
                used_confirmation_nonces: BTreeSet::new(),
                warning: None,
                change_note_pending: false,
            },
            active_bundle: embedded,
        })
    }

    /// # Errors
    ///
    /// Returns an error when private storage or the embedded Persona bundle
    /// cannot be initialized safely.
    #[cfg(test)]
    pub fn open(
        root: &Path,
        host_protocol: u64,
        expected_team_identifier: String,
    ) -> Result<Self, PersonaError> {
        create_private_directory(root)?;
        let embedded = PersonaBundle::embedded_default(host_protocol)?;
        let state_path = root.join("state.json");
        let mut state = if state_path.exists() {
            let bytes = read_regular_file(&state_path, MAX_BUNDLE_BYTES)?;
            serde_json::from_slice::<RegistryState>(&bytes).unwrap_or_else(|_| RegistryState {
                active_digest: embedded.revision_ref.aggregate_digest.clone(),
                staged_digest: None,
                verified: BTreeMap::new(),
                used_confirmation_nonces: BTreeSet::new(),
                warning: Some(
                    "The saved conversation style could not be verified, so OpenOpen kept the built-in style."
                        .to_owned(),
                ),
                change_note_pending: false,
            })
        } else {
            RegistryState {
                active_digest: embedded.revision_ref.aggregate_digest.clone(),
                staged_digest: None,
                verified: BTreeMap::new(),
                used_confirmation_nonces: BTreeSet::new(),
                warning: None,
                change_note_pending: false,
            }
        };
        let embedded_record = StoredRevision {
            revision_ref: embedded.revision_ref.clone(),
            change_summary: embedded.manifest.change_summary.clone(),
            signer_team_identifier: "embedded-app-signature".to_owned(),
            signer_identity: embedded.manifest.signer_identity.clone(),
        };
        materialize_verified(root, &embedded, host_protocol)?;
        state
            .verified
            .entry(embedded.revision_ref.aggregate_digest.clone())
            .or_insert(embedded_record);
        let active_bundle = load_verified_bundle(root, &state.active_digest, host_protocol)
            .unwrap_or_else(|_| embedded.clone());
        if active_bundle.revision_ref.aggregate_digest != state.active_digest {
            state
                .active_digest
                .clone_from(&embedded.revision_ref.aggregate_digest);
            state.staged_digest = None;
            state.warning = Some(
                "The saved conversation style could not be verified, so OpenOpen kept the built-in style."
                    .to_owned(),
            );
        }
        let mut manager = Self {
            root: root.to_path_buf(),
            host_protocol,
            expected_team_identifier,
            state,
            active_bundle,
        };
        manager.persist()?;
        Ok(manager)
    }

    #[must_use]
    pub fn status(&self) -> PersonaStatus {
        PersonaStatus {
            active: self.active_bundle.revision_ref.clone(),
            staged: self.state.staged_digest.as_ref().and_then(|digest| {
                self.state
                    .verified
                    .get(digest)
                    .map(|value| value.revision_ref.clone())
            }),
            warning: self.state.warning.clone(),
            change_note_pending: self.state.change_note_pending,
        }
    }

    #[must_use]
    pub fn accept_turn(&self) -> PersonaRevisionRef {
        self.active_bundle.revision_ref.clone()
    }

    #[must_use]
    pub fn active_bundle(&self) -> &PersonaBundle {
        &self.active_bundle
    }

    /// # Errors
    ///
    /// Returns an error when the referenced revision is unknown, altered, or
    /// no longer verified in private storage.
    pub fn bundle_for_ref(
        &self,
        revision: &PersonaRevisionRef,
    ) -> Result<PersonaBundle, PersonaError> {
        if !revision.is_valid()
            || !self
                .state
                .verified
                .get(&revision.aggregate_digest)
                .is_some_and(|stored| stored.revision_ref == *revision)
        {
            return Err(PersonaError::UnknownRevision);
        }
        (self.active_bundle.revision_ref == *revision)
            .then_some(self.active_bundle.clone())
            .ok_or(PersonaError::ChangedAfterStaging)
    }

    /// # Errors
    ///
    /// Returns an error when signature verification or staging fails.
    #[cfg(test)]
    pub fn stage(&mut self, path: &Path) -> Result<PersonaDiff, PersonaError> {
        let verifier = MacDeveloperIdVerifier::new(self.expected_team_identifier.clone())?;
        self.stage_with_verifier(path, &verifier)
    }

    /// # Errors
    ///
    /// Returns an error when the bundle is not a valid, newer, signed Persona
    /// revision for this Host.
    #[cfg(test)]
    pub fn stage_with_verifier(
        &mut self,
        path: &Path,
        verifier: &dyn CodeSignatureVerifier,
    ) -> Result<PersonaDiff, PersonaError> {
        let (bundle, signer) = PersonaBundle::load_update(path, self.host_protocol, verifier)?;
        let staged_revision =
            revision_number(&bundle.revision_ref.revision).ok_or(PersonaError::InvalidContract)?;
        let active_revision = revision_number(&self.active_bundle.revision_ref.revision)
            .ok_or(PersonaError::InvalidContract)?;
        if bundle.revision_ref.persona_id != self.active_bundle.revision_ref.persona_id
            || staged_revision <= active_revision
        {
            return Err(PersonaError::Downgrade);
        }
        if signer.team_identifier != self.expected_team_identifier {
            return Err(PersonaError::InvalidSignature);
        }
        let changed_files = changed_files(&self.active_bundle, &bundle);
        materialize_verified(&self.root, &bundle, self.host_protocol)?;
        self.state.verified.insert(
            bundle.revision_ref.aggregate_digest.clone(),
            StoredRevision {
                revision_ref: bundle.revision_ref.clone(),
                change_summary: bundle.manifest.change_summary.clone(),
                signer_team_identifier: signer.team_identifier,
                signer_identity: signer.signing_identifier,
            },
        );
        self.state.staged_digest = Some(bundle.revision_ref.aggregate_digest.clone());
        self.state.warning = None;
        self.persist()?;
        Ok(PersonaDiff {
            active: self.active_bundle.revision_ref.clone(),
            staged: bundle.revision_ref,
            change_summary: bundle.manifest.change_summary,
            changed_files,
        })
    }

    /// # Errors
    ///
    /// Returns an error when the one-time confirmation does not exactly bind
    /// the currently staged verified revision.
    #[cfg(test)]
    pub fn activate(
        &mut self,
        confirmation: &PersonaActivationConfirmation,
    ) -> Result<PersonaStatus, PersonaError> {
        if !confirmation.is_valid()
            || self
                .state
                .used_confirmation_nonces
                .contains(&confirmation.confirmation_nonce)
            || self.state.staged_digest.as_deref() != Some(&confirmation.expected_digest)
        {
            return Err(PersonaError::InvalidConfirmation);
        }
        let bundle = load_verified_bundle(
            &self.root,
            &confirmation.expected_digest,
            self.host_protocol,
        )
        .map_err(|_| PersonaError::ChangedAfterStaging)?;
        let stored = self
            .state
            .verified
            .get(&confirmation.expected_digest)
            .ok_or(PersonaError::UnknownRevision)?;
        if stored.revision_ref != bundle.revision_ref {
            return Err(PersonaError::ChangedAfterStaging);
        }
        self.state
            .active_digest
            .clone_from(&confirmation.expected_digest);
        self.state.staged_digest = None;
        self.state
            .used_confirmation_nonces
            .insert(confirmation.confirmation_nonce.clone());
        self.state.change_note_pending = true;
        self.state.warning = None;
        self.active_bundle = bundle;
        self.persist()?;
        Ok(self.status())
    }

    /// # Errors
    ///
    /// Returns an error when the confirmation is reused, mismatched, or does
    /// not name an already verified revision.
    #[cfg(test)]
    pub fn rollback(
        &mut self,
        target_digest: &str,
        confirmation: &PersonaActivationConfirmation,
    ) -> Result<PersonaStatus, PersonaError> {
        if target_digest != confirmation.expected_digest
            || !confirmation.is_valid()
            || self
                .state
                .used_confirmation_nonces
                .contains(&confirmation.confirmation_nonce)
            || !self.state.verified.contains_key(target_digest)
        {
            return Err(PersonaError::InvalidConfirmation);
        }
        let bundle = load_verified_bundle(&self.root, target_digest, self.host_protocol)
            .map_err(|_| PersonaError::ChangedAfterStaging)?;
        target_digest.clone_into(&mut self.state.active_digest);
        self.state.staged_digest = None;
        self.state
            .used_confirmation_nonces
            .insert(confirmation.confirmation_nonce.clone());
        self.state.change_note_pending = true;
        self.state.warning = None;
        self.active_bundle = bundle;
        self.persist()?;
        Ok(self.status())
    }

    /// # Errors
    ///
    /// Returns an error when the durable change-note state cannot be updated.
    #[cfg(test)]
    pub fn take_change_note(&mut self) -> Result<Option<String>, PersonaError> {
        if !self.state.change_note_pending {
            return Ok(None);
        }
        self.state.change_note_pending = false;
        self.persist()?;
        Ok(Some(
            self.active_bundle
                .message("updated")
                .unwrap_or("OpenOpen’s conversation style was updated.")
                .to_owned(),
        ))
    }

    #[cfg(test)]
    fn persist(&mut self) -> Result<(), PersonaError> {
        let bytes = serde_json::to_vec(&self.state)?;
        write_private_file_atomic(&self.root.join("state.json"), &bytes)?;
        Ok(())
    }
}

pub struct ConversationRouter;

impl ConversationRouter {
    #[must_use]
    pub fn decide(context: &ConversationContext) -> ConversationDecision {
        match context.turn_kind {
            ConversationTurnKind::Progress => return ConversationDecision::Progress,
            ConversationTurnKind::Completion => return ConversationDecision::Receipt,
            ConversationTurnKind::Failure => return ConversationDecision::SafeFailure,
            ConversationTurnKind::Permission | ConversationTurnKind::Confirmation => {
                return ConversationDecision::NeedUser;
            }
            _ => {}
        }
        if context.risk_class == ConversationRiskClass::Irreversible
            || context.ambiguity_class == ConversationAmbiguityClass::ExternalEffect
        {
            return ConversationDecision::EditablePreview;
        }
        if context.ambiguity_class == ConversationAmbiguityClass::MissingRequiredValue {
            return ConversationDecision::Clarify;
        }
        match context.ambiguity_class {
            ConversationAmbiguityClass::None | ConversationAmbiguityClass::SmallReversible => {
                ConversationDecision::Direct
            }
            ConversationAmbiguityClass::MissingRequiredValue => ConversationDecision::Clarify,
            ConversationAmbiguityClass::MaterialPreview
            | ConversationAmbiguityClass::ExternalEffect => ConversationDecision::EditablePreview,
            ConversationAmbiguityClass::MaterialOutcome => ConversationDecision::Choice,
        }
    }
}

pub struct ConversationRenderer<'a> {
    bundle: &'a PersonaBundle,
}

pub struct RenderRequest {
    pub context: ConversationContext,
    pub decision: ConversationDecision,
    pub primary_answer: String,
    pub explanation: Option<String>,
    pub choices: Vec<ConversationChoice>,
    pub next_step: Option<String>,
    pub technical_details: Option<String>,
}

impl<'a> ConversationRenderer<'a> {
    #[must_use]
    pub const fn new(bundle: &'a PersonaBundle) -> Self {
        Self { bundle }
    }

    /// # Errors
    ///
    /// Returns an error when the rendered content contradicts the exact
    /// Host-owned context or the bounded response contract.
    pub fn render(&self, request: RenderRequest) -> Result<RenderedMessage, PersonaError> {
        if request.decision != ConversationRouter::decide(&request.context)
            || request.primary_answer.trim().is_empty()
            || request.choices.len() > 4
            || (request.decision == ConversationDecision::Choice && request.choices.len() != 4)
            || (request.decision != ConversationDecision::Choice && !request.choices.is_empty())
        {
            return Err(PersonaError::InvalidContract);
        }
        let message = RenderedMessage {
            primary_answer: request.primary_answer,
            explanation: request.explanation,
            choices: request.choices,
            next_step: request.next_step,
            technical_details: request.technical_details,
            persona: self.bundle.revision_ref.clone(),
        };
        message
            .is_valid()
            .then_some(message)
            .ok_or(PersonaError::InvalidContract)
    }

    #[must_use]
    pub fn surface_text(&self, surface: ConversationSurface, rendered: &RenderedMessage) -> String {
        let mut parts = Vec::new();
        parts.push(rendered.primary_answer.clone());
        if let Some(explanation) = &rendered.explanation {
            parts.push(explanation.clone());
        }
        if !rendered.choices.is_empty() {
            for choice in &rendered.choices {
                parts.push(format!(
                    "{} — {} ({})",
                    choice.label, choice.outcome, choice.tradeoff
                ));
            }
        }
        if let Some(next_step) = &rendered.next_step {
            parts.push(next_step.clone());
        }
        let body = parts.join("\n");
        match surface {
            ConversationSurface::MacApp => body,
            ConversationSurface::IMessageSelfChat => format!(
                "{}\n{body}",
                self.bundle
                    .message("identityPrefix")
                    .unwrap_or("OpenOpen · AI")
            ),
        }
    }

    /// # Errors
    ///
    /// Returns an error when the supplied next step is empty, oversized, or
    /// the verified bundle does not contain the required safe message.
    pub fn overwhelmed(&self, next_step: &str) -> Result<String, PersonaError> {
        if next_step.trim().is_empty() || next_step.len() > 1_024 {
            return Err(PersonaError::InvalidContract);
        }
        Ok(self
            .bundle
            .message("overwhelmed")
            .ok_or(PersonaError::InvalidContract)?
            .replace("{next_step}", next_step))
    }
}

fn validate_manifest(manifest: &PersonaManifest, host_protocol: u64) -> Result<(), PersonaError> {
    let expected_files = BTreeSet::from([
        "persona.json".to_owned(),
        "messages.en.json".to_owned(),
        "scenarios.json".to_owned(),
        "evals.jsonl".to_owned(),
    ]);
    if manifest.schema_version != PERSONA_SCHEMA_VERSION
        || manifest.persona_id != DEFAULT_PERSONA_ID
        || !valid_identifier(&manifest.revision, 64)
        || manifest.locale != "en"
        || manifest.minimum_host_protocol == 0
        || manifest.minimum_host_protocol > host_protocol
        || manifest.files.keys().cloned().collect::<BTreeSet<_>>() != expected_files
        || manifest
            .files
            .values()
            .any(|value| !is_sha256_string(value))
        || !valid_text(&manifest.change_summary, 1_024)
        || !valid_identifier(&manifest.signer_identity, 128)
    {
        return Err(PersonaError::Incompatible);
    }
    Ok(())
}

fn validate_policy(policy: &PersonaPolicy) -> Result<(), PersonaError> {
    let strings = [
        &policy.voice.warmth,
        &policy.voice.recommendation,
        &policy.voice.humor,
        &policy.voice.greeting,
        &policy.ambiguity_policy.small_reversible,
        &policy.ambiguity_policy.material_preview,
        &policy.ambiguity_policy.material_outcome,
        &policy.ambiguity_policy.external_effect,
    ];
    if strings.iter().any(|value| !valid_text(value, 512))
        || policy.voice.greeting != "Hi — what are you working through?"
        || policy.boundaries.attachment_claims
        || policy.boundaries.relationship_maintenance
        || policy.boundaries.fabricated_experience
        || policy.boundaries.inferred_durable_preferences
        || policy.boundaries.technical_details_by_default
        || policy.response_shape.minimum_sentences != 1
        || policy.response_shape.maximum_sentences != 5
        || policy.response_shape.maximum_focused_questions != 1
        || !policy.response_shape.one_useful_next_step
        || !policy.response_shape.answer_first
        || !policy.adaptation.per_turn
        || !policy.adaptation.session_preference_immediate
        || !policy.adaptation.durable_preference_requires_confirmation
        || !policy.proactive_policy.task_grounded_only
        || policy.proactive_policy.relationship_checkins
        || policy.progress_policy.initial_working_after_seconds != 30
        || policy.progress_policy.first_unchanged_liveness_minutes != 10
        || policy.progress_policy.subsequent_unchanged_liveness_minutes != 30
        || policy.progress_policy.fake_percentages
    {
        return Err(PersonaError::InvalidContract);
    }
    let allowed = BTreeSet::from([
        "selected-reminder",
        "task-change",
        "required-decision",
        "failure",
        "completion",
    ]);
    if policy
        .proactive_policy
        .allowed_events
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != allowed
    {
        return Err(PersonaError::InvalidContract);
    }
    Ok(())
}

fn validate_messages(messages: &BTreeMap<String, String>) -> Result<(), PersonaError> {
    let required = BTreeSet::from([
        "greeting",
        "overwhelmed",
        "working",
        "updated",
        "off",
        "duplicate",
        "failureBeforeEffect",
        "ambiguousEffect",
        "completion",
        "switchingTopic",
        "changedConfirmation",
        "permission",
        "needUser",
        "identityPrefix",
        "reminderTimeRequired",
        "preferenceCandidate",
        "memoryCorrection",
        "permissionDeclined",
        "liveness",
        "partialFailure",
        "modelUnavailable",
        "restartRecovery",
        "returnShort",
        "returnMedium",
        "returnLong",
        "pendingConfirmation",
        "cancelled",
        "unapprovedConversation",
    ]);
    if messages.keys().map(String::as_str).collect::<BTreeSet<_>>() != required
        || messages.values().any(|value| !valid_text(value, 2_048))
        || messages.get("greeting").map(String::as_str)
            != Some("Hi — what are you working through?")
        || messages.get("identityPrefix").map(String::as_str) != Some("OpenOpen · AI")
    {
        return Err(PersonaError::InvalidContract);
    }
    Ok(())
}

fn validate_scenarios(scenarios: &[ScenarioDefinition]) -> Result<(), PersonaError> {
    let mut ids = BTreeSet::new();
    if scenarios.len() < 32
        || scenarios.iter().any(|scenario| {
            !valid_identifier(&scenario.id, 128)
                || !ids.insert(scenario.id.as_str())
                || scenario.hard_constraints.is_empty()
                || scenario.hard_constraints.len() > 16
                || scenario
                    .hard_constraints
                    .iter()
                    .any(|value| !valid_identifier(value, 128))
        })
    {
        return Err(PersonaError::InvalidContract);
    }
    Ok(())
}

fn parse_evals(bytes: &[u8]) -> Result<Vec<EvalCase>, PersonaError> {
    let text = std::str::from_utf8(bytes).map_err(|_| PersonaError::InvalidContract)?;
    let mut cases = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() || line != line.trim() {
            return Err(PersonaError::InvalidContract);
        }
        let value: EvalCase = serde_json::from_str(line)?;
        cases.push(value);
    }
    Ok(cases)
}

fn validate_evals(
    evals: &[EvalCase],
    scenarios: &[ScenarioDefinition],
) -> Result<(), PersonaError> {
    let scenario_ids = scenarios
        .iter()
        .map(|scenario| scenario.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut ids = BTreeSet::new();
    let single_turn = evals
        .iter()
        .filter(|value| value.category == "single-turn")
        .count();
    let multi_turn = evals
        .iter()
        .filter(|value| value.category == "multi-turn")
        .count();
    let adversarial = evals
        .iter()
        .filter(|value| value.category == "adversarial")
        .count();
    if single_turn < 128 || multi_turn < 12 || adversarial < 8 {
        return Err(PersonaError::InvalidContract);
    }
    for value in evals {
        if !valid_identifier(&value.id, 160)
            || !ids.insert(value.id.as_str())
            || !matches!(
                value.category.as_str(),
                "single-turn" | "multi-turn" | "adversarial"
            )
            || value.hard_constraints.is_empty()
            || value
                .hard_constraints
                .iter()
                .any(|constraint| !valid_identifier(constraint, 128))
        {
            return Err(PersonaError::InvalidContract);
        }
        if value.category == "single-turn"
            && (!value
                .scenario
                .as_deref()
                .is_some_and(|scenario| scenario_ids.contains(scenario))
                || !matches!(
                    value.variant.as_deref(),
                    Some("concise" | "rambling" | "informal" | "emotional")
                )
                || value.expected_decision.is_none())
        {
            return Err(PersonaError::InvalidContract);
        }
    }
    Ok(())
}

fn validate_bundle_layout(root: &Path, allow_signature_metadata: bool) -> Result<(), PersonaError> {
    let metadata = fs::symlink_metadata(root)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(PersonaError::InvalidLayout);
    }
    let mut total = 0_u64;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| PersonaError::InvalidLayout)?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            return Err(PersonaError::InvalidLayout);
        }
        if ALLOWED_CONTENT_FILES.contains(&name.as_str()) {
            if !metadata.is_file() || metadata.permissions().mode() & 0o111 != 0 {
                return Err(PersonaError::InvalidLayout);
            }
            total = total.saturating_add(metadata.len());
            continue;
        }
        if allow_signature_metadata
            && name == "Info.plist"
            && metadata.is_file()
            && metadata.permissions().mode() & 0o111 == 0
        {
            total = total.saturating_add(metadata.len());
            continue;
        }
        if allow_signature_metadata && name == "_CodeSignature" && metadata.is_dir() {
            let children = fs::read_dir(entry.path())?.collect::<Result<Vec<_>, _>>()?;
            if children.len() != 1 || children[0].file_name() != "CodeResources" {
                return Err(PersonaError::InvalidLayout);
            }
            let child_metadata = fs::symlink_metadata(children[0].path())?;
            if !child_metadata.is_file()
                || child_metadata.file_type().is_symlink()
                || child_metadata.permissions().mode() & 0o111 != 0
            {
                return Err(PersonaError::InvalidLayout);
            }
            total = total.saturating_add(child_metadata.len());
            continue;
        }
        return Err(PersonaError::InvalidLayout);
    }
    if total > MAX_BUNDLE_BYTES {
        return Err(PersonaError::Oversized);
    }
    Ok(())
}

fn read_content_files(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, PersonaError> {
    ALLOWED_CONTENT_FILES
        .iter()
        .map(|name| {
            let maximum = file_limit(name).ok_or(PersonaError::InvalidLayout)?;
            read_regular_file(&root.join(name), maximum)
                .map(|content| ((*name).to_owned(), content))
        })
        .collect()
}

fn required_file<'a>(
    files: &'a BTreeMap<String, Vec<u8>>,
    name: &str,
    maximum: u64,
) -> Result<&'a [u8], PersonaError> {
    let value = files.get(name).ok_or(PersonaError::InvalidLayout)?;
    if value.len() as u64 > maximum {
        return Err(PersonaError::Oversized);
    }
    Ok(value)
}

const fn file_limit(name: &str) -> Option<u64> {
    match name.as_bytes() {
        b"manifest.json" => Some(MAX_MANIFEST_BYTES),
        b"persona.json" => Some(MAX_POLICY_BYTES),
        b"messages.en.json" => Some(MAX_MESSAGES_BYTES),
        b"scenarios.json" => Some(MAX_SCENARIOS_BYTES),
        b"evals.jsonl" => Some(MAX_EVALS_BYTES),
        _ => None,
    }
}

fn read_regular_file(path: &Path, maximum: u64) -> Result<Vec<u8>, PersonaError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(PersonaError::InvalidLayout);
    }
    if metadata.len() > maximum {
        return Err(PersonaError::Oversized);
    }
    let bytes = fs::read(path)?;
    if bytes.len() as u64 != metadata.len() {
        return Err(PersonaError::ChangedAfterStaging);
    }
    Ok(bytes)
}

#[cfg(test)]
fn materialize_verified(
    root: &Path,
    bundle: &PersonaBundle,
    host_protocol: u64,
) -> Result<(), PersonaError> {
    let verified_root = root.join("verified");
    create_private_directory(&verified_root)?;
    let final_path = verified_root.join(&bundle.revision_ref.aggregate_digest);
    if final_path.exists() {
        let existing = PersonaBundle::load_embedded(&final_path, host_protocol)?;
        if existing.revision_ref == bundle.revision_ref {
            return Ok(());
        }
        return Err(PersonaError::ChangedAfterStaging);
    }
    let temporary = verified_root.join(format!(
        ".{}.tmp-{}",
        bundle.revision_ref.aggregate_digest,
        std::process::id()
    ));
    if temporary.exists() {
        fs::remove_dir_all(&temporary)?;
    }
    bundle.write_to(&temporary)?;
    fs::rename(&temporary, &final_path)?;
    sync_directory(&verified_root)?;
    Ok(())
}

#[cfg(test)]
fn load_verified_bundle(
    root: &Path,
    digest: &str,
    host_protocol: u64,
) -> Result<PersonaBundle, PersonaError> {
    if !is_sha256_string(digest) {
        return Err(PersonaError::UnknownRevision);
    }
    let bundle = PersonaBundle::load_embedded(&root.join("verified").join(digest), host_protocol)?;
    if bundle.revision_ref.aggregate_digest != digest {
        return Err(PersonaError::DigestMismatch);
    }
    Ok(bundle)
}

#[cfg(test)]
fn changed_files(active: &PersonaBundle, staged: &PersonaBundle) -> Vec<String> {
    ALLOWED_CONTENT_FILES
        .iter()
        .filter(|name| active.raw_files.get(**name) != staged.raw_files.get(**name))
        .map(|name| (*name).to_owned())
        .collect()
}

#[cfg(test)]
fn revision_number(revision: &str) -> Option<u64> {
    let suffix = revision.strip_prefix("draft-")?.strip_suffix("-en")?;
    suffix.parse().ok()
}

fn valid_identifier(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.trim().is_empty()
        && value == value.trim()
        && value.len() <= maximum
        && !value.chars().any(char::is_control)
}

fn is_sha256_string(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Produces a compact, deterministic commitment to every field of every
/// evaluation record. The model-facing contract remains bounded, while a
/// change to an input, multi-turn sequence, suite, variant, decision, or even
/// a repeated otherwise-equal case changes the compiled instructions.
fn evaluation_commitments(evals: &[EvalCase]) -> Result<String, PersonaError> {
    let canonical = evals
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()?;
    let field_digest = |name: &str, values: Vec<String>| {
        format!("{name}={}", sha256_hex(values.join("\u{1f}").as_bytes()))
    };
    let field_digests = [
        field_digest("ids", evals.iter().map(|value| value.id.clone()).collect()),
        field_digest(
            "categories",
            evals.iter().map(|value| value.category.clone()).collect(),
        ),
        field_digest(
            "scenarios",
            evals
                .iter()
                .map(|value| value.scenario.clone().unwrap_or_else(|| "null".to_owned()))
                .collect(),
        ),
        field_digest(
            "variants",
            evals
                .iter()
                .map(|value| value.variant.clone().unwrap_or_else(|| "null".to_owned()))
                .collect(),
        ),
        field_digest(
            "suites",
            evals
                .iter()
                .map(|value| value.suite.clone().unwrap_or_else(|| "null".to_owned()))
                .collect(),
        ),
        field_digest(
            "inputs",
            evals
                .iter()
                .map(|value| value.input.clone().unwrap_or_else(|| "null".to_owned()))
                .collect(),
        ),
        field_digest(
            "turns",
            evals
                .iter()
                .map(|value| {
                    value
                        .turns
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()
                        .map(|value| value.unwrap_or_else(|| "null".to_owned()))
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
        field_digest(
            "decisions",
            evals
                .iter()
                .map(|value| {
                    value
                        .expected_decision
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()
                        .map(|value| value.unwrap_or_else(|| "null".to_owned()))
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
        field_digest(
            "constraints",
            evals
                .iter()
                .map(|value| serde_json::to_string(&value.hard_constraints))
                .collect::<Result<Vec<_>, _>>()?,
        ),
    ];
    Ok(format!(
        "caseCount={};fullRecords={};{}",
        canonical.len(),
        sha256_hex(canonical.join("\n").as_bytes()),
        field_digests.join(";")
    ))
}

#[cfg(test)]
fn valid_team_identifier(value: &str) -> bool {
    value.len() == 10
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

#[cfg(all(test, target_os = "macos"))]
fn signing_value(details: &str, field: &str) -> Option<String> {
    details.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key == field && !value.is_empty()).then(|| value.to_owned())
    })
}

#[cfg(test)]
fn create_private_directory(path: &Path) -> Result<(), PersonaError> {
    if path.exists() {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(PersonaError::InvalidLayout);
        }
    } else {
        fs::create_dir(path)?;
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(test)]
fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), PersonaError> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
fn write_private_file_atomic(path: &Path, bytes: &[u8]) -> Result<(), PersonaError> {
    let parent = path.parent().ok_or(PersonaError::InvalidLayout)?;
    create_private_directory(parent)?;
    let temporary = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|value| value.to_str())
            .ok_or(PersonaError::InvalidLayout)?,
        std::process::id()
    ));
    if temporary.exists() {
        fs::remove_file(&temporary)?;
    }
    write_private_file(&temporary, bytes)?;
    fs::rename(&temporary, path)?;
    sync_directory(parent)?;
    Ok(())
}

#[cfg(test)]
fn sync_directory(path: &Path) -> Result<(), PersonaError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openopen_protocol::{
        ActiveTaskState, ConversationUrgency, RequestedDetail, TransientSupportLevel,
    };
    use tempfile::TempDir;

    struct TestVerifier {
        team: String,
        identifier: String,
        pass: bool,
    }

    impl CodeSignatureVerifier for TestVerifier {
        fn verify(&self, _bundle_path: &Path) -> Result<VerifiedSigner, PersonaError> {
            self.pass
                .then(|| VerifiedSigner {
                    team_identifier: self.team.clone(),
                    signing_identifier: self.identifier.clone(),
                })
                .ok_or(PersonaError::InvalidSignature)
        }
    }

    fn context(
        surface: ConversationSurface,
        ambiguity_class: ConversationAmbiguityClass,
    ) -> ConversationContext {
        ConversationContext {
            surface,
            turn_kind: ConversationTurnKind::Request,
            ambiguity_class,
            risk_class: ConversationRiskClass::Ordinary,
            urgency: ConversationUrgency::Normal,
            requested_detail: RequestedDetail::Concise,
            active_task_state: ActiveTaskState::None,
            return_interval_ms: None,
            transient_support_level: TransientSupportLevel::Ordinary,
        }
    }

    fn update_bundle(root: &Path, revision: &str, change_summary: &str) -> PersonaBundle {
        let mut files = BTreeMap::from([
            ("persona.json".to_owned(), EMBEDDED_POLICY.to_vec()),
            ("messages.en.json".to_owned(), EMBEDDED_MESSAGES.to_vec()),
            ("scenarios.json".to_owned(), EMBEDDED_SCENARIOS.to_vec()),
            ("evals.jsonl".to_owned(), EMBEDDED_EVALS.to_vec()),
        ]);
        let manifest = PersonaManifest {
            schema_version: PERSONA_SCHEMA_VERSION,
            persona_id: DEFAULT_PERSONA_ID.to_owned(),
            revision: revision.to_owned(),
            locale: "en".to_owned(),
            minimum_host_protocol: 1,
            files: files
                .iter()
                .map(|(name, bytes)| (name.clone(), sha256_hex(bytes)))
                .collect(),
            change_summary: change_summary.to_owned(),
            signer_identity: "com.thesongzhu.OpenOpen.persona".to_owned(),
        };
        files.insert(
            "manifest.json".to_owned(),
            serde_json::to_vec(&manifest).unwrap(),
        );
        let bundle = PersonaBundle::from_files(files, 1).unwrap();
        bundle.write_to(root).unwrap();
        bundle
    }

    #[test]
    fn embedded_bundle_is_canonical_and_contains_complete_eval_floor() {
        let bundle = PersonaBundle::embedded_default(1).unwrap();
        assert_eq!(bundle.revision_ref.persona_id, DEFAULT_PERSONA_ID);
        assert_eq!(bundle.revision_ref.revision, DEFAULT_REVISION);
        assert_eq!(bundle.scenarios.len(), 50);
        assert_eq!(
            bundle
                .evals
                .iter()
                .filter(|value| value.category == "single-turn")
                .count(),
            200
        );
        assert_eq!(
            bundle
                .evals
                .iter()
                .filter(|value| value.category == "multi-turn")
                .count(),
            12
        );
        let instructions = bundle.developer_instructions(true).unwrap();
        assert!(instructions.contains("Do not invoke tools."));
        assert!(instructions.contains(&bundle.revision_ref.aggregate_digest));
        assert!(instructions.contains("Verified message catalog"));
        assert!(instructions.contains("Verified scenario obligations"));
        assert!(instructions.contains("Verified complete evaluation commitments"));
        assert_eq!(
            bundle.revision_ref.instructions_digest,
            sha256_hex(instructions.as_bytes())
        );
    }

    #[test]
    fn verified_bundle_content_changes_the_compiled_model_contract() {
        let bundle = PersonaBundle::embedded_default(1).unwrap();
        let baseline = bundle.developer_instructions(true).unwrap();

        let mut changed_message = bundle.clone();
        changed_message.messages.0.insert(
            "working".to_owned(),
            "A changed verified message.".to_owned(),
        );
        assert_ne!(
            baseline,
            changed_message.developer_instructions(true).unwrap()
        );

        let mut changed_scenario = bundle.clone();
        changed_scenario.scenarios[0]
            .hard_constraints
            .push("changed-contract".to_owned());
        assert_ne!(
            baseline,
            changed_scenario.developer_instructions(true).unwrap()
        );

        let mutations: Vec<fn(&mut EvalCase)> = vec![
            |value| value.id.push_str("-changed"),
            |value| value.category.push_str("-changed"),
            |value| value.scenario = Some("changed-scenario".to_owned()),
            |value| value.variant = Some("changed-variant".to_owned()),
            |value| value.suite = Some("changed-suite".to_owned()),
            |value| value.input = Some("changed input".to_owned()),
            |value| value.turns = Some(vec!["changed turn".to_owned()]),
            |value| value.expected_decision = Some(ConversationDecision::Clarify),
            |value| value.hard_constraints.push("changed-evaluation".to_owned()),
        ];
        for mutate in mutations {
            let mut changed_evaluation = bundle.clone();
            mutate(&mut changed_evaluation.evals[0]);
            assert_ne!(
                baseline,
                changed_evaluation.developer_instructions(true).unwrap()
            );
        }
    }

    #[test]
    fn policy_rejects_a_bundle_that_disables_immediate_session_preferences() {
        let bundle = PersonaBundle::embedded_default(1).unwrap();
        let mut policy = bundle.policy.clone();
        policy.adaptation.session_preference_immediate = false;
        assert!(matches!(
            validate_policy(&policy),
            Err(PersonaError::InvalidContract)
        ));
    }

    #[test]
    fn router_reserves_choices_for_material_outcome_branches() {
        let ordinary = context(
            ConversationSurface::MacApp,
            ConversationAmbiguityClass::SmallReversible,
        );
        assert_eq!(
            ConversationRouter::decide(&ordinary),
            ConversationDecision::Direct
        );
        let material = context(
            ConversationSurface::MacApp,
            ConversationAmbiguityClass::MaterialOutcome,
        );
        assert_eq!(
            ConversationRouter::decide(&material),
            ConversationDecision::Choice
        );
        let mut effect = ordinary;
        effect.risk_class = ConversationRiskClass::Irreversible;
        assert_eq!(
            ConversationRouter::decide(&effect),
            ConversationDecision::EditablePreview
        );
        let missing = context(
            ConversationSurface::MacApp,
            ConversationAmbiguityClass::MissingRequiredValue,
        );
        assert_eq!(
            ConversationRouter::decide(&missing),
            ConversationDecision::Clarify
        );
        let mut high_stakes = context(
            ConversationSurface::MacApp,
            ConversationAmbiguityClass::None,
        );
        high_stakes.risk_class = ConversationRiskClass::HighStakes;
        assert_eq!(
            ConversationRouter::decide(&high_stakes),
            ConversationDecision::Direct
        );
    }

    #[test]
    fn mac_and_imessage_share_message_and_revision_with_identity_only_on_imessage() {
        let bundle = PersonaBundle::embedded_default(1).unwrap();
        let renderer = ConversationRenderer::new(&bundle);
        let context = context(
            ConversationSurface::MacApp,
            ConversationAmbiguityClass::None,
        );
        let message = renderer
            .render(RenderRequest {
                context,
                decision: ConversationDecision::Direct,
                primary_answer: "Here is the answer.".to_owned(),
                explanation: Some("One short explanation.".to_owned()),
                choices: Vec::new(),
                next_step: None,
                technical_details: None,
            })
            .unwrap();
        let mac = renderer.surface_text(ConversationSurface::MacApp, &message);
        let imessage = renderer.surface_text(ConversationSurface::IMessageSelfChat, &message);
        assert_eq!(imessage, format!("OpenOpen · AI\n{mac}"));
        assert_eq!(message.persona, bundle.revision_ref);
    }

    #[test]
    fn invalid_or_changed_updates_never_replace_last_verified_revision() {
        let temp = TempDir::new().unwrap();
        let mut manager = PersonaManager::open(temp.path(), 1, "A1B2C3D4E5".to_owned()).unwrap();
        let original = manager.accept_turn();
        let unsigned = TestVerifier {
            team: "A1B2C3D4E5".to_owned(),
            identifier: "com.thesongzhu.OpenOpen.persona".to_owned(),
            pass: false,
        };
        let bundle_path = temp
            .path()
            .join("verified")
            .join(&original.aggregate_digest);
        assert!(
            manager
                .stage_with_verifier(&bundle_path, &unsigned)
                .is_err()
        );
        assert_eq!(manager.accept_turn(), original);
    }

    #[test]
    fn activation_is_one_shot_applies_to_next_turn_and_can_rollback() {
        let temp = TempDir::new().unwrap();
        let storage = temp.path().join("storage");
        let update = temp.path().join("update");
        let mut manager = PersonaManager::open(&storage, 1, "A1B2C3D4E5".to_owned()).unwrap();
        let accepted_turn = manager.accept_turn();
        let candidate = update_bundle(&update, "draft-04-en", "Makes corrections more concise.");
        let verifier = TestVerifier {
            team: "A1B2C3D4E5".to_owned(),
            identifier: "com.thesongzhu.OpenOpen.persona".to_owned(),
            pass: true,
        };
        let diff = manager.stage_with_verifier(&update, &verifier).unwrap();
        assert_eq!(diff.staged, candidate.revision_ref);
        assert_eq!(accepted_turn.revision, "draft-03-en");

        let activation = PersonaActivationConfirmation {
            expected_digest: candidate.revision_ref.aggregate_digest.clone(),
            confirmation_nonce: "activate-draft-04".to_owned(),
            confirmed_at_ms: 1,
        };
        manager.activate(&activation).unwrap();
        assert_eq!(accepted_turn.revision, "draft-03-en");
        assert_eq!(manager.accept_turn(), candidate.revision_ref);
        assert_eq!(
            manager.take_change_note().unwrap().as_deref(),
            Some("OpenOpen’s conversation style was updated.")
        );
        assert!(manager.take_change_note().unwrap().is_none());
        assert!(matches!(
            manager.activate(&activation),
            Err(PersonaError::InvalidConfirmation)
        ));

        let rollback = PersonaActivationConfirmation {
            expected_digest: accepted_turn.aggregate_digest.clone(),
            confirmation_nonce: "rollback-draft-03".to_owned(),
            confirmed_at_ms: 2,
        };
        manager
            .rollback(&accepted_turn.aggregate_digest, &rollback)
            .unwrap();
        assert_eq!(manager.accept_turn(), accepted_turn);
    }

    #[test]
    fn changed_staged_bundle_is_rejected_at_activation() {
        let temp = TempDir::new().unwrap();
        let storage = temp.path().join("storage");
        let update = temp.path().join("update");
        let mut manager = PersonaManager::open(&storage, 1, "A1B2C3D4E5".to_owned()).unwrap();
        let candidate = update_bundle(&update, "draft-04-en", "Makes corrections more concise.");
        let verifier = TestVerifier {
            team: "A1B2C3D4E5".to_owned(),
            identifier: "com.thesongzhu.OpenOpen.persona".to_owned(),
            pass: true,
        };
        manager.stage_with_verifier(&update, &verifier).unwrap();
        let verified_policy = storage
            .join("verified")
            .join(&candidate.revision_ref.aggregate_digest)
            .join("persona.json");
        fs::set_permissions(&verified_policy, fs::Permissions::from_mode(0o700)).unwrap();
        let activation = PersonaActivationConfirmation {
            expected_digest: candidate.revision_ref.aggregate_digest,
            confirmation_nonce: "activate-changed-bundle".to_owned(),
            confirmed_at_ms: 1,
        };
        assert!(matches!(
            manager.activate(&activation),
            Err(PersonaError::ChangedAfterStaging)
        ));
        assert_eq!(manager.accept_turn().revision, "draft-03-en");
    }

    #[test]
    fn symlink_and_unknown_file_are_rejected_before_signature_verification() {
        let temp = TempDir::new().unwrap();
        for (name, bytes) in [
            ("manifest.json", EMBEDDED_MANIFEST),
            ("persona.json", EMBEDDED_POLICY),
            ("messages.en.json", EMBEDDED_MESSAGES),
            ("scenarios.json", EMBEDDED_SCENARIOS),
            ("evals.jsonl", EMBEDDED_EVALS),
        ] {
            fs::write(temp.path().join(name), bytes).unwrap();
        }
        fs::write(temp.path().join("unknown.json"), b"{}").unwrap();
        assert!(matches!(
            validate_bundle_layout(temp.path(), true),
            Err(PersonaError::InvalidLayout)
        ));
    }

    #[test]
    fn executable_content_is_rejected_before_signature_verification() {
        let temp = TempDir::new().unwrap();
        for (name, bytes) in [
            ("manifest.json", EMBEDDED_MANIFEST),
            ("persona.json", EMBEDDED_POLICY),
            ("messages.en.json", EMBEDDED_MESSAGES),
            ("scenarios.json", EMBEDDED_SCENARIOS),
            ("evals.jsonl", EMBEDDED_EVALS),
        ] {
            fs::write(temp.path().join(name), bytes).unwrap();
        }
        fs::set_permissions(
            temp.path().join("persona.json"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        assert!(matches!(
            validate_bundle_layout(temp.path(), true),
            Err(PersonaError::InvalidLayout)
        ));
    }

    #[test]
    fn corrupt_registry_falls_back_to_embedded_without_losing_availability() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("state.json"), b"not-json").unwrap();
        let manager = PersonaManager::open(temp.path(), 1, "A1B2C3D4E5".to_owned()).unwrap();
        assert_eq!(manager.accept_turn().revision, DEFAULT_REVISION);
        assert!(manager.status().warning.is_some());
    }

    #[test]
    fn default_only_view_ignores_existing_lifecycle_state_without_destroying_it() {
        let temp = TempDir::new().unwrap();
        let storage = temp.path().join("storage");
        let update = temp.path().join("update");
        let mut lifecycle = PersonaManager::open(&storage, 1, "A1B2C3D4E5".to_owned()).unwrap();
        let candidate = update_bundle(&update, "draft-04-en", "Changes language policy.");
        let verifier = TestVerifier {
            team: "A1B2C3D4E5".to_owned(),
            identifier: "com.thesongzhu.OpenOpen.persona".to_owned(),
            pass: true,
        };
        lifecycle.stage_with_verifier(&update, &verifier).unwrap();
        lifecycle
            .activate(&PersonaActivationConfirmation {
                expected_digest: candidate.revision_ref.aggregate_digest,
                confirmation_nonce: "activate-draft-04".to_owned(),
                confirmed_at_ms: 1,
            })
            .unwrap();
        let state_path = storage.join("state.json");
        let before = fs::read(&state_path).unwrap();

        let pr1 = PersonaManager::open_default_only(&storage, 1, "A1B2C3D4E5".to_owned()).unwrap();
        assert_eq!(pr1.accept_turn().revision, DEFAULT_REVISION);
        assert!(pr1.status().staged.is_none());
        assert!(!pr1.status().change_note_pending);
        assert_eq!(fs::read(state_path).unwrap(), before);
    }

    #[test]
    fn default_only_view_never_materializes_or_reads_a_private_registry() {
        let temp = TempDir::new().unwrap();
        let absent_storage = temp.path().join("absent-storage");
        let manager =
            PersonaManager::open_default_only(&absent_storage, 1, "A1B2C3D4E5".to_owned()).unwrap();

        assert!(!absent_storage.exists());
        let active = manager.accept_turn();
        assert_eq!(
            manager.bundle_for_ref(&active).unwrap().revision_ref,
            active
        );
        assert_eq!(
            manager
                .bundle_for_ref(&PersonaRevisionRef {
                    persona_id: active.persona_id.clone(),
                    revision: active.revision.clone(),
                    aggregate_digest: active.aggregate_digest.clone(),
                    instructions_digest: "a".repeat(64),
                })
                .unwrap_err()
                .to_string(),
            PersonaError::UnknownRevision.to_string()
        );
        assert!(!absent_storage.exists());
    }

    #[test]
    fn overwhelmed_copy_has_one_manageable_step() {
        let bundle = PersonaBundle::embedded_default(1).unwrap();
        let renderer = ConversationRenderer::new(&bundle);
        assert_eq!(
            renderer
                .overwhelmed("write down the next deadline")
                .unwrap(),
            "That sounds like a lot. Let’s make this smaller: write down the next deadline"
        );
    }
}
