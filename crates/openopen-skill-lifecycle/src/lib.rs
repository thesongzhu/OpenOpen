//! Fail-closed GitHub acquisition, structural auditing, and lifecycle state
//! for instruction-only Skills.
//!
//! The concrete acquirer performs only bounded, credential-free reads from
//! the fixed GitHub API origin. Skill content is never executed and receives
//! no network, filesystem, channel, model, provider, or product authority. The
//! Integrator remains responsible for typed RPC, durable Store transactions,
//! Global Off, and the owner-decision audit authority.

mod acquisition;

pub use acquisition::{AcquisitionError, GitHubAcquirer};

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;
use std::str;

use sha2::{Digest, Sha256};
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;
use url::Url;

pub const MAX_ENTRIES: usize = 200;
pub const MAX_TOTAL_BYTES: u64 = 5 * 1024 * 1024;
pub const MAX_FILE_BYTES: u64 = 512 * 1024;
pub const MAX_PATH_BYTES: usize = 256;
pub const MAX_PATH_DEPTH: usize = 8;

const REGULAR_NON_EXECUTABLE_MODE: u32 = 0o100_644;
const MAX_APPROVAL_ID_BYTES: usize = 128;
const MAX_FRONT_MATTER_BYTES: usize = 2 * 1024;
const PERMISSION_PATTERNS: [&str; 11] = [
    "allowedtools",
    "allowtools",
    "externaleffects",
    "filesystemaccess",
    "networkaccess",
    "permissionmanifest",
    "permissions",
    "requiresnetwork",
    "requirestool",
    "toolpermissions",
    "external effects",
];
const SENSITIVE_TOKENS: [&str; 12] = [
    "apikey",
    "apikeys",
    "credential",
    "credentials",
    "password",
    "passwords",
    "secret",
    "secrets",
    "systemprompt",
    "token",
    "tokens",
    "privatekey",
];
const UNDECLARED_CAPABILITY_TOKENS: &[&str] = &[
    "browser",
    "browsers",
    "calendar",
    "channel",
    "channels",
    "credential",
    "credentials",
    "directory",
    "directories",
    "disk",
    "email",
    "emails",
    "file",
    "files",
    "filesystem",
    "folder",
    "folders",
    "internet",
    "message",
    "messages",
    "model",
    "network",
    "online",
    "password",
    "passwords",
    "provider",
    "recipient",
    "recipients",
    "secret",
    "secrets",
    "token",
    "tokens",
    "tool",
    "tools",
    "url",
    "urls",
    "web",
    "website",
    "websites",
    "workspace",
];
const SAFE_INSTRUCTION_TOKENS: &[&str] = &[
    "a",
    "bounded",
    "checklist",
    "confirm",
    "confirmed",
    "guide",
    "help",
    "inside",
    "item",
    "md",
    "mission",
    "name",
    "offer",
    "planning",
    "prepare",
    "prose",
    "references",
    "review",
    "safe",
    "skill",
    "study",
    "template",
    "the",
    "use",
    "x",
];
const AUTHORITY_PATTERNS: [&str; 39] = [
    "addrecipient",
    "anyrecipient",
    "bypassapproval",
    "bypassconfirmation",
    "bypassgate",
    "changeeffort",
    "changemodel",
    "changeprovider",
    "circumventapproval",
    "disregardearlierdirections",
    "disregardpreviousinstructions",
    "downloadfile",
    "executeaction",
    "executearbitrarycode",
    "ignoreallpriorinstructions",
    "ignoredeveloperinstructions",
    "ignorepreviousinstructions",
    "ignoresysteminstructions",
    "newrecipient",
    "overrideapproval",
    "overridedeveloper",
    "overrideinstructions",
    "overridesystem",
    "persistdata",
    "readcredential",
    "readpassword",
    "readsecret",
    "retaindata",
    "revealprompt",
    "sendemail",
    "sendmessage",
    "sendto",
    "skipapproval",
    "switchmodel",
    "switchprovider",
    "uploadfile",
    "withoutapproval",
    "withoutconfirmation",
    "withoutownerapproval",
];
const LEADING_COMMAND_TOKENS: &[&str] = &[
    "bash",
    "cat",
    "cd",
    "chmod",
    "chown",
    "cp",
    "curl",
    "dd",
    "echo",
    "env",
    "eval",
    "exec",
    "git",
    "grep",
    "ln",
    "ls",
    "make",
    "mkdir",
    "mktemp",
    "mv",
    "node",
    "npm",
    "npx",
    "perl",
    "php",
    "pip",
    "pip3",
    "powershell",
    "printf",
    "print",
    "python",
    "python3",
    "rm",
    "ruby",
    "sed",
    "sh",
    "source",
    "sudo",
    "test",
    "touch",
    "wget",
    "xargs",
    "zsh",
];
const SCRIPT_RUNTIME_TOKENS: &[&str] = &[
    "bash",
    "bun",
    "childprocess",
    "cmdexe",
    "csh",
    "curl",
    "dash",
    "deno",
    "eval",
    "exec",
    "fish",
    "javascript",
    "javascriptcore",
    "ksh",
    "lua",
    "node",
    "nodejs",
    "npm",
    "npx",
    "perl",
    "php",
    "pip",
    "pip3",
    "powershell",
    "python",
    "python3",
    "ruby",
    "shell",
    "subprocess",
    "tcsh",
    "typescript",
    "wget",
    "zsh",
];
const OBFUSCATED_SCRIPT_MARKERS: &[&str] = &[
    "javascript",
    "nodejs",
    "powershell",
    "python",
    "shell",
    "subprocess",
    "typescript",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitHubRequest {
    owner: String,
    repo: String,
    requested_ref: Option<String>,
    package_path: String,
}

impl GitHubRequest {
    /// Parses only canonical public GitHub repository/tree URLs. Encoded path
    /// segments are rejected instead of being normalized into another URL.
    ///
    /// # Errors
    ///
    /// Returns a typed source error when the URL, repository identity, ref, or
    /// package path is not canonical and bounded.
    pub fn parse(value: &str) -> Result<Self, SourceError> {
        if !value.is_ascii()
            || value.contains('%')
            || value
                .split('/')
                .any(|segment| matches!(segment, "." | ".."))
        {
            return Err(SourceError::NonCanonicalUrl);
        }
        let authority = value
            .strip_prefix("https://")
            .and_then(|remainder| remainder.split('/').next())
            .ok_or(SourceError::NonCanonicalUrl)?;
        if authority != "github.com" {
            return Err(SourceError::NonCanonicalUrl);
        }
        let url = Url::parse(value).map_err(|_| SourceError::NonCanonicalUrl)?;
        if url.scheme() != "https"
            || url.host_str() != Some("github.com")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
            || value.ends_with('/')
        {
            return Err(SourceError::NonCanonicalUrl);
        }

        let segments: Vec<&str> = url
            .path_segments()
            .ok_or(SourceError::NonCanonicalUrl)?
            .collect();
        if segments.len() < 2 {
            return Err(SourceError::NonCanonicalUrl);
        }
        let owner = segments[0];
        let repo = segments[1];
        if !valid_github_atom(owner)
            || !valid_github_atom(repo)
            || Path::new(repo)
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("git"))
            || owner.len() > 100
            || repo.len() > 100
        {
            return Err(SourceError::InvalidRepositoryIdentity);
        }

        let (requested_ref, package_path) = match segments.as_slice() {
            [_, _] => (None, String::new()),
            [_, _, "tree", requested_ref, rest @ ..] => {
                if !valid_github_ref(requested_ref) {
                    return Err(SourceError::InvalidRequestedRef);
                }
                let package_path = normalize_source_path(rest)?;
                (Some((*requested_ref).to_owned()), package_path)
            }
            _ => return Err(SourceError::NonCanonicalUrl),
        };

        Ok(Self {
            owner: owner.to_owned(),
            repo: repo.to_owned(),
            requested_ref,
            package_path,
        })
    }

    #[must_use]
    pub fn owner(&self) -> &str {
        &self.owner
    }

    #[must_use]
    pub fn repo(&self) -> &str {
        &self.repo
    }

    #[must_use]
    pub fn requested_ref(&self) -> Option<&str> {
        self.requested_ref.as_deref()
    }

    #[must_use]
    pub fn package_path(&self) -> &str {
        &self.package_path
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkillSource {
    owner: String,
    repo: String,
    package_path: String,
    commit: String,
}

impl SkillSource {
    /// Binds a parsed request to an immutable, lowercase 40-hex commit.
    ///
    /// # Errors
    ///
    /// Returns a typed source error when the resolved commit is not immutable
    /// or contradicts an immutable commit already present in the URL.
    pub fn resolve(request: GitHubRequest, commit: &str) -> Result<Self, SourceError> {
        if !is_immutable_commit(commit) {
            return Err(SourceError::InvalidResolvedCommit);
        }
        if request
            .requested_ref
            .as_deref()
            .is_some_and(is_immutable_commit)
            && request.requested_ref.as_deref() != Some(commit)
        {
            return Err(SourceError::ResolvedCommitMismatch);
        }
        Ok(Self {
            owner: request.owner,
            repo: request.repo,
            package_path: request.package_path,
            commit: commit.to_owned(),
        })
    }

    #[must_use]
    pub fn owner(&self) -> &str {
        &self.owner
    }

    #[must_use]
    pub fn repo(&self) -> &str {
        &self.repo
    }

    #[must_use]
    pub fn package_path(&self) -> &str {
        &self.package_path
    }

    #[must_use]
    pub fn commit(&self) -> &str {
        &self.commit
    }

    fn same_package_as(&self, other: &Self) -> bool {
        self.owner == other.owner
            && self.repo == other.repo
            && self.package_path == other.package_path
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryKind {
    File,
    Symlink,
    Submodule,
    Special,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageEntry {
    path: String,
    mode: u32,
    kind: EntryKind,
    bytes: Vec<u8>,
}

impl PackageEntry {
    #[must_use]
    pub fn file(path: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            path: path.into(),
            mode: REGULAR_NON_EXECUTABLE_MODE,
            kind: EntryKind::File,
            bytes: bytes.into(),
        }
    }

    #[must_use]
    pub fn with_metadata(
        path: impl Into<String>,
        mode: u32,
        kind: EntryKind,
        bytes: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            path: path.into(),
            mode,
            kind,
            bytes: bytes.into(),
        }
    }

    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    #[must_use]
    pub const fn mode(&self) -> u32 {
        self.mode
    }

    #[must_use]
    pub const fn kind(&self) -> EntryKind {
        self.kind
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedPackage {
    source: SkillSource,
    entries: Vec<PackageEntry>,
    provenance: GitHubProvenance,
}

impl ResolvedPackage {
    #[must_use]
    pub const fn source(&self) -> &SkillSource {
        &self.source
    }

    #[must_use]
    pub fn entries(&self) -> &[PackageEntry] {
        &self.entries
    }

    #[must_use]
    pub const fn provenance(&self) -> &GitHubProvenance {
        &self.provenance
    }
}

/// Immutable proof summary retained with the package constructed by the
/// concrete GitHub acquirer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitHubProvenance {
    repository_identity: String,
    commit_tree: String,
    package_tree: String,
    verified_tree_objects: usize,
    verified_blob_objects: usize,
}

impl GitHubProvenance {
    #[must_use]
    pub fn repository_identity(&self) -> &str {
        &self.repository_identity
    }

    #[must_use]
    pub fn commit_tree(&self) -> &str {
        &self.commit_tree
    }

    #[must_use]
    pub fn package_tree(&self) -> &str {
        &self.package_tree
    }

    #[must_use]
    pub const fn verified_tree_objects(&self) -> usize {
        self.verified_tree_objects
    }

    #[must_use]
    pub const fn verified_blob_objects(&self) -> usize {
        self.verified_blob_objects
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AcceptedLicense {
    Mit,
    Apache2,
    Bsd2Clause,
    Bsd3Clause,
    Isc,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Capability {
    Tools,
    Network,
    Filesystem,
    Channels,
    ExternalEffects,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionManifest {
    grants: BTreeSet<Capability>,
}

impl PermissionManifest {
    #[must_use]
    pub fn instruction_only() -> Self {
        Self {
            grants: BTreeSet::new(),
        }
    }

    #[must_use]
    pub fn allows(&self, capability: Capability) -> bool {
        self.grants.contains(&capability)
    }

    #[must_use]
    pub fn digest(&self) -> String {
        let canonical = format!(
            "tools={}\nnetwork={}\nfilesystem={}\nchannels={}\nexternal_effects={}\n",
            u8::from(self.allows(Capability::Tools)),
            u8::from(self.allows(Capability::Network)),
            u8::from(self.allows(Capability::Filesystem)),
            u8::from(self.allows(Capability::Channels)),
            u8::from(self.allows(Capability::ExternalEffects))
        );
        hex::encode(Sha256::digest(canonical.as_bytes()))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditedPackage {
    source: SkillSource,
    digest: String,
    license: AcceptedLicense,
    entry_count: usize,
    total_bytes: u64,
    permissions: PermissionManifest,
}

impl AuditedPackage {
    #[must_use]
    pub const fn source(&self) -> &SkillSource {
        &self.source
    }

    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }

    #[must_use]
    pub const fn license(&self) -> AcceptedLicense {
        self.license
    }

    #[must_use]
    pub const fn entry_count(&self) -> usize {
        self.entry_count
    }

    #[must_use]
    pub const fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    #[must_use]
    pub const fn permissions(&self) -> &PermissionManifest {
        &self.permissions
    }
}

#[derive(Clone, Debug)]
struct NormalizedEntry {
    path: String,
    bytes: Vec<u8>,
}

/// Audits exact supplied bytes without fetching, executing, or granting any
/// authority. Failure produces no staged package.
///
/// # Errors
///
/// Returns the first typed fail-closed package violation. No lifecycle state
/// is changed by this pure audit.
pub fn audit_package(package: &ResolvedPackage) -> Result<AuditedPackage, AuditError> {
    if package.entries.is_empty() {
        return Err(AuditError::EmptyPackage);
    }
    if package.entries.len() > MAX_ENTRIES {
        return Err(AuditError::TooManyEntries);
    }

    let (mut normalized_entries, exact_paths, total_bytes) =
        normalize_and_validate_entries(&package.entries)?;
    let (license, license_path) = validate_root_contract(&normalized_entries, &exact_paths)?;
    validate_dependencies(&normalized_entries, &exact_paths)?;
    validate_instruction_contract(&normalized_entries, &license_path)?;
    normalized_entries.sort_by(|left, right| left.path.cmp(&right.path));

    Ok(AuditedPackage {
        source: package.source.clone(),
        digest: canonical_digest(&normalized_entries)?,
        license,
        entry_count: normalized_entries.len(),
        total_bytes,
        permissions: PermissionManifest::instruction_only(),
    })
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct VersionId(u64);

impl VersionId {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for VersionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VersionState {
    Candidate,
    Staged,
    Promoted,
    Runnable,
    RolledBack,
    Rejected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkillVersion {
    id: VersionId,
    state: VersionState,
    package: ResolvedPackage,
    audit: Option<AuditedPackage>,
    staged_revision: Option<u64>,
    staged_audit_anchor: Option<AuditAnchor>,
    promotion_record: Option<PromotionRecord>,
}

impl SkillVersion {
    #[must_use]
    pub const fn id(&self) -> VersionId {
        self.id
    }

    #[must_use]
    pub const fn state(&self) -> VersionState {
        self.state
    }

    #[must_use]
    pub const fn source(&self) -> &SkillSource {
        self.package.source()
    }

    #[must_use]
    pub const fn audit(&self) -> Option<&AuditedPackage> {
        self.audit.as_ref()
    }

    #[must_use]
    pub const fn staged_revision(&self) -> Option<u64> {
        self.staged_revision
    }

    #[must_use]
    pub const fn staged_audit_anchor(&self) -> Option<&AuditAnchor> {
        self.staged_audit_anchor.as_ref()
    }

    #[must_use]
    pub const fn promotion_record(&self) -> Option<&PromotionRecord> {
        self.promotion_record.as_ref()
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct AuditAnchor(String);

impl AuditAnchor {
    /// Parses the exact lowercase SHA-256 audit-row anchor supplied by the
    /// durable Integrator transaction.
    ///
    /// # Errors
    ///
    /// Returns a typed error when the value is not lowercase 64-hex.
    pub fn parse(value: impl Into<String>) -> Result<Self, ApprovalError> {
        let value = value.into();
        if is_lower_hex(&value, 64) {
            Ok(Self(value))
        } else {
            Err(ApprovalError::InvalidAuditAnchor)
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromotionDecision {
    actor_id: String,
    decision_id: String,
    nonce: String,
}

impl PromotionDecision {
    /// Creates the identity supplied by one explicit owner decision. The
    /// nonce is a 32-byte lowercase hexadecimal value and must be globally
    /// unique in the durable Store.
    ///
    /// # Errors
    ///
    /// Returns a typed error for empty, non-canonical, control-bearing, or
    /// oversized identities and malformed nonces.
    pub fn new(
        actor_id: impl Into<String>,
        decision_id: impl Into<String>,
        nonce: impl Into<String>,
    ) -> Result<Self, ApprovalError> {
        let actor_id = actor_id.into();
        let decision_id = decision_id.into();
        let nonce = nonce.into();
        if !valid_approval_identity(&actor_id) {
            return Err(ApprovalError::InvalidActorIdentity);
        }
        if !valid_approval_identity(&decision_id) {
            return Err(ApprovalError::InvalidDecisionIdentity);
        }
        if !is_lower_hex(&nonce, 64) {
            return Err(ApprovalError::InvalidNonce);
        }
        Ok(Self {
            actor_id,
            decision_id,
            nonce,
        })
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct PromotionApproval {
    source: SkillSource,
    version_id: VersionId,
    staged_revision: u64,
    staged_audit_anchor: AuditAnchor,
    package_digest: String,
    permission_digest: String,
    actor_id: String,
    decision_id: String,
    nonce: String,
}

/// Immutable decision evidence retained on the exact version by the same
/// transition that consumes its one-use approval nonce.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromotionRecord {
    source: SkillSource,
    version_id: VersionId,
    staged_revision: u64,
    staged_audit_anchor: AuditAnchor,
    package_digest: String,
    permission_digest: String,
    actor_id: String,
    decision_id: String,
    nonce: String,
}

/// One explicit owner decision permitting exactly one instruction-only,
/// no-external-effect use of the current Runnable version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirstUseDecision {
    actor_id: String,
    decision_id: String,
    nonce: String,
}

impl FirstUseDecision {
    /// Creates a bounded decision identity. The nonce is consumed only by a
    /// successful first-use receipt transition.
    ///
    /// # Errors
    ///
    /// Returns a typed error for malformed identities or a non-canonical
    /// nonce.
    pub fn new(
        actor_id: impl Into<String>,
        decision_id: impl Into<String>,
        nonce: impl Into<String>,
    ) -> Result<Self, ApprovalError> {
        let actor_id = actor_id.into();
        let decision_id = decision_id.into();
        let nonce = nonce.into();
        if !valid_approval_identity(&actor_id) {
            return Err(ApprovalError::InvalidActorIdentity);
        }
        if !valid_approval_identity(&decision_id) {
            return Err(ApprovalError::InvalidDecisionIdentity);
        }
        if !is_lower_hex(&nonce, 64) {
            return Err(ApprovalError::InvalidNonce);
        }
        Ok(Self {
            actor_id,
            decision_id,
            nonce,
        })
    }
}

/// Immutable one-use approval bound to the exact Runnable version, package,
/// empty permission manifest, staged audit anchor, and lifecycle revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirstUseApproval {
    source: SkillSource,
    version_id: VersionId,
    lifecycle_revision: u64,
    staged_revision: u64,
    staged_audit_anchor: AuditAnchor,
    package_digest: String,
    permission_digest: String,
    actor_id: String,
    decision_id: String,
    nonce: String,
}

impl FirstUseApproval {
    /// Binds a decision to the exact currently Runnable, instruction-only
    /// version. Binding itself does not consume authority or execute content.
    ///
    /// # Errors
    ///
    /// Returns a typed lifecycle error unless the current pin and all audit
    /// facts are complete and capability grants are empty.
    pub fn bind(
        lifecycle: &SkillLifecycle,
        decision: FirstUseDecision,
    ) -> Result<Self, LifecycleError> {
        let version_id = lifecycle
            .current_runnable
            .ok_or(LifecycleError::NoRunnableVersion)?;
        let version = lifecycle
            .versions
            .get(&version_id)
            .ok_or(LifecycleError::UnknownVersion)?;
        require_state(version.state, VersionState::Runnable)?;
        let audit = version.audit.as_ref().ok_or(LifecycleError::MissingAudit)?;
        if !audit.permissions.grants.is_empty() {
            return Err(LifecycleError::FirstUseRequiresInstructionOnly);
        }
        Ok(Self {
            source: version.package.source.clone(),
            version_id,
            lifecycle_revision: lifecycle.revision,
            staged_revision: version
                .staged_revision
                .ok_or(LifecycleError::MissingStagedBinding)?,
            staged_audit_anchor: version
                .staged_audit_anchor
                .clone()
                .ok_or(LifecycleError::MissingStagedBinding)?,
            package_digest: audit.digest.clone(),
            permission_digest: audit.permissions.digest(),
            actor_id: decision.actor_id,
            decision_id: decision.decision_id,
            nonce: decision.nonce,
        })
    }
}

/// Durable evidence for the sole approved first use. It records only exact
/// identities and the caller's already-computed result digest; this crate
/// never interprets or executes package instructions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FirstUseReceipt {
    receipt_digest: String,
    source: SkillSource,
    version_id: VersionId,
    approved_lifecycle_revision: u64,
    committed_lifecycle_revision: u64,
    staged_revision: u64,
    staged_audit_anchor: AuditAnchor,
    package_digest: String,
    permission_digest: String,
    actor_id: String,
    decision_id: String,
    nonce: String,
    result_digest: String,
}

impl FirstUseReceipt {
    #[must_use]
    pub fn receipt_digest(&self) -> &str {
        &self.receipt_digest
    }

    #[must_use]
    pub const fn source(&self) -> &SkillSource {
        &self.source
    }

    #[must_use]
    pub const fn version_id(&self) -> VersionId {
        self.version_id
    }

    #[must_use]
    pub const fn approved_lifecycle_revision(&self) -> u64 {
        self.approved_lifecycle_revision
    }

    #[must_use]
    pub const fn committed_lifecycle_revision(&self) -> u64 {
        self.committed_lifecycle_revision
    }

    #[must_use]
    pub const fn staged_revision(&self) -> u64 {
        self.staged_revision
    }

    #[must_use]
    pub const fn staged_audit_anchor(&self) -> &AuditAnchor {
        &self.staged_audit_anchor
    }

    #[must_use]
    pub fn package_digest(&self) -> &str {
        &self.package_digest
    }

    #[must_use]
    pub fn permission_digest(&self) -> &str {
        &self.permission_digest
    }

    #[must_use]
    pub fn actor_id(&self) -> &str {
        &self.actor_id
    }

    #[must_use]
    pub fn decision_id(&self) -> &str {
        &self.decision_id
    }

    #[must_use]
    pub fn nonce(&self) -> &str {
        &self.nonce
    }

    #[must_use]
    pub fn result_digest(&self) -> &str {
        &self.result_digest
    }
}

impl PromotionRecord {
    #[must_use]
    pub const fn source(&self) -> &SkillSource {
        &self.source
    }

    #[must_use]
    pub const fn version_id(&self) -> VersionId {
        self.version_id
    }

    #[must_use]
    pub const fn staged_revision(&self) -> u64 {
        self.staged_revision
    }

    #[must_use]
    pub const fn staged_audit_anchor(&self) -> &AuditAnchor {
        &self.staged_audit_anchor
    }

    #[must_use]
    pub fn package_digest(&self) -> &str {
        &self.package_digest
    }

    #[must_use]
    pub fn permission_digest(&self) -> &str {
        &self.permission_digest
    }

    #[must_use]
    pub fn actor_id(&self) -> &str {
        &self.actor_id
    }

    #[must_use]
    pub fn decision_id(&self) -> &str {
        &self.decision_id
    }

    #[must_use]
    pub fn nonce(&self) -> &str {
        &self.nonce
    }
}

impl PromotionApproval {
    /// Binds one decision to the exact currently Staged version and audit
    /// anchor. The returned value is consumed by `promote`; any independently
    /// reconstructed decision with the same nonce fails because the lifecycle
    /// atomically records that nonce.
    ///
    /// # Errors
    ///
    /// Returns a lifecycle error when the version is missing, not Staged, or
    /// lacks its complete audit binding.
    pub fn bind(
        lifecycle: &SkillLifecycle,
        id: VersionId,
        decision: PromotionDecision,
    ) -> Result<Self, LifecycleError> {
        let version = lifecycle
            .versions
            .get(&id)
            .ok_or(LifecycleError::UnknownVersion)?;
        require_state(version.state, VersionState::Staged)?;
        let audit = version.audit.as_ref().ok_or(LifecycleError::MissingAudit)?;
        let staged_revision = version
            .staged_revision
            .ok_or(LifecycleError::MissingStagedBinding)?;
        let staged_audit_anchor = version
            .staged_audit_anchor
            .clone()
            .ok_or(LifecycleError::MissingStagedBinding)?;
        Ok(Self {
            source: version.package.source.clone(),
            version_id: id,
            staged_revision,
            staged_audit_anchor,
            package_digest: audit.digest.clone(),
            permission_digest: audit.permissions.digest(),
            actor_id: decision.actor_id,
            decision_id: decision.decision_id,
            nonce: decision.nonce,
        })
    }

    #[must_use]
    pub fn actor_id(&self) -> &str {
        &self.actor_id
    }

    #[must_use]
    pub fn decision_id(&self) -> &str {
        &self.decision_id
    }

    #[must_use]
    pub fn nonce(&self) -> &str {
        &self.nonce
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkillLifecycle {
    package_source: SkillSource,
    revision: u64,
    next_version_id: u64,
    versions: BTreeMap<VersionId, SkillVersion>,
    current_runnable: Option<VersionId>,
    rollback_target: Option<VersionId>,
    consumed_approval_nonces: BTreeSet<String>,
    first_use_receipts: BTreeMap<VersionId, FirstUseReceipt>,
}

impl SkillLifecycle {
    #[must_use]
    pub fn from_candidate(package: ResolvedPackage) -> Self {
        let source = package.source.clone();
        let first_id = VersionId(1);
        let first = SkillVersion {
            id: first_id,
            state: VersionState::Candidate,
            package,
            audit: None,
            staged_revision: None,
            staged_audit_anchor: None,
            promotion_record: None,
        };
        Self {
            package_source: source,
            revision: 1,
            next_version_id: 2,
            versions: BTreeMap::from([(first_id, first)]),
            current_runnable: None,
            rollback_target: None,
            consumed_approval_nonces: BTreeSet::new(),
            first_use_receipts: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn current_runnable(&self) -> Option<VersionId> {
        self.current_runnable
    }

    #[must_use]
    pub const fn rollback_target(&self) -> Option<VersionId> {
        self.rollback_target
    }

    #[must_use]
    pub fn version(&self, id: VersionId) -> Option<&SkillVersion> {
        self.versions.get(&id)
    }

    pub fn version_ids(&self) -> impl Iterator<Item = VersionId> + '_ {
        self.versions.keys().copied()
    }

    #[must_use]
    pub fn first_use_receipt(&self, id: VersionId) -> Option<&FirstUseReceipt> {
        self.first_use_receipts.get(&id)
    }

    /// Atomically records the sole instruction-only, no-external-effect use of
    /// one exact Runnable version. The result is represented only by a
    /// lowercase SHA-256 digest; no package bytes are read or executed here.
    /// Exact retries return the durable receipt without advancing revision.
    ///
    /// # Errors
    ///
    /// Returns a typed revision, state, approval, capability, digest, or
    /// replay-conflict error without partial state movement.
    pub fn record_first_no_effect_use(
        &mut self,
        expected_revision: u64,
        approval: &FirstUseApproval,
        result_digest: &str,
    ) -> Result<FirstUseReceipt, LifecycleError> {
        self.require_revision(expected_revision)?;
        if !is_lower_hex(result_digest, 64) {
            return Err(LifecycleError::InvalidFirstUseResultDigest);
        }
        if let Some(existing) = self.first_use_receipts.get(&approval.version_id) {
            return first_use_receipt_matches(existing, approval, result_digest)
                .then_some(existing.clone())
                .ok_or(LifecycleError::FirstUseAlreadyRecorded);
        }
        let current = self
            .current_runnable
            .ok_or(LifecycleError::NoRunnableVersion)?;
        if current != approval.version_id || approval.lifecycle_revision != self.revision {
            return Err(LifecycleError::FirstUseApprovalMismatch);
        }
        let version = self
            .versions
            .get(&current)
            .ok_or(LifecycleError::UnknownVersion)?;
        require_state(version.state, VersionState::Runnable)?;
        let audit = version.audit.as_ref().ok_or(LifecycleError::MissingAudit)?;
        let staged_revision = version
            .staged_revision
            .ok_or(LifecycleError::MissingStagedBinding)?;
        let staged_audit_anchor = version
            .staged_audit_anchor
            .as_ref()
            .ok_or(LifecycleError::MissingStagedBinding)?;
        if !audit.permissions.grants.is_empty() {
            return Err(LifecycleError::FirstUseRequiresInstructionOnly);
        }
        if approval.source != *version.package.source()
            || approval.staged_revision != staged_revision
            || approval.staged_audit_anchor != *staged_audit_anchor
            || approval.package_digest != audit.digest
            || approval.permission_digest != audit.permissions.digest()
        {
            return Err(LifecycleError::FirstUseApprovalMismatch);
        }
        if self.consumed_approval_nonces.contains(&approval.nonce) {
            return Err(LifecycleError::FirstUseApprovalAlreadyConsumed);
        }
        let next_revision = self.next_revision()?;
        let receipt_digest = first_use_receipt_digest(approval, next_revision, result_digest);
        let receipt = FirstUseReceipt {
            receipt_digest,
            source: approval.source.clone(),
            version_id: approval.version_id,
            approved_lifecycle_revision: approval.lifecycle_revision,
            committed_lifecycle_revision: next_revision,
            staged_revision: approval.staged_revision,
            staged_audit_anchor: approval.staged_audit_anchor.clone(),
            package_digest: approval.package_digest.clone(),
            permission_digest: approval.permission_digest.clone(),
            actor_id: approval.actor_id.clone(),
            decision_id: approval.decision_id.clone(),
            nonce: approval.nonce.clone(),
            result_digest: result_digest.to_owned(),
        };
        self.consumed_approval_nonces.insert(approval.nonce.clone());
        self.first_use_receipts.insert(current, receipt);
        self.revision = next_revision;
        self.first_use_receipts
            .get(&current)
            .cloned()
            .ok_or(LifecycleError::FirstUseAlreadyRecorded)
    }

    /// Adds a new immutable Candidate while the current Runnable pin remains
    /// unchanged.
    ///
    /// # Errors
    ///
    /// Returns a typed revision, missing-runnable, identity, or overflow
    /// error. Failure leaves the lifecycle unchanged.
    pub fn propose_update(
        &mut self,
        expected_revision: u64,
        package: ResolvedPackage,
    ) -> Result<VersionId, LifecycleError> {
        self.require_revision(expected_revision)?;
        if self.current_runnable.is_none() {
            return Err(LifecycleError::NoRunnableVersion);
        }
        if !self.package_source.same_package_as(package.source()) {
            return Err(LifecycleError::PackageIdentityChanged);
        }
        let next_revision = self.next_revision()?;
        let id = VersionId(self.next_version_id);
        let next_version_id = self
            .next_version_id
            .checked_add(1)
            .ok_or(LifecycleError::VersionIdOverflow)?;
        self.versions.insert(
            id,
            SkillVersion {
                id,
                state: VersionState::Candidate,
                package,
                audit: None,
                staged_revision: None,
                staged_audit_anchor: None,
                promotion_record: None,
            },
        );
        self.next_version_id = next_version_id;
        self.revision = next_revision;
        Ok(id)
    }

    /// Audits a Candidate and moves it to Staged without partial state.
    ///
    /// # Errors
    ///
    /// Returns a typed revision, state, identity, or package-audit error.
    pub fn stage(
        &mut self,
        expected_revision: u64,
        id: VersionId,
        audit_anchor: AuditAnchor,
    ) -> Result<&AuditedPackage, LifecycleError> {
        self.require_revision(expected_revision)?;
        let package = self
            .versions
            .get(&id)
            .ok_or(LifecycleError::UnknownVersion)?;
        require_state(package.state, VersionState::Candidate)?;
        let audit = audit_package(&package.package)?;
        let next_revision = self.next_revision()?;
        let version = self
            .versions
            .get_mut(&id)
            .ok_or(LifecycleError::UnknownVersion)?;
        version.state = VersionState::Staged;
        version.audit = Some(audit);
        version.staged_revision = Some(next_revision);
        version.staged_audit_anchor = Some(audit_anchor);
        self.revision = next_revision;
        self.versions
            .get(&id)
            .and_then(SkillVersion::audit)
            .ok_or(LifecycleError::MissingAudit)
    }

    /// Applies an exact digest/commit/permission-bound promotion approval.
    ///
    /// # Errors
    ///
    /// Returns a typed revision, state, missing-audit, or approval error.
    pub fn promote(
        &mut self,
        expected_revision: u64,
        id: VersionId,
        approval: PromotionApproval,
    ) -> Result<(), LifecycleError> {
        self.require_revision(expected_revision)?;
        let next_revision = self.next_revision()?;
        let version = self
            .versions
            .get(&id)
            .ok_or(LifecycleError::UnknownVersion)?;
        require_state(version.state, VersionState::Staged)?;
        let audit = version.audit.as_ref().ok_or(LifecycleError::MissingAudit)?;
        let staged_revision = version
            .staged_revision
            .ok_or(LifecycleError::MissingStagedBinding)?;
        let staged_audit_anchor = version
            .staged_audit_anchor
            .as_ref()
            .ok_or(LifecycleError::MissingStagedBinding)?;
        if approval.source != *version.package.source()
            || approval.version_id != id
            || approval.staged_revision != staged_revision
            || approval.staged_audit_anchor != *staged_audit_anchor
            || approval.package_digest != audit.digest
            || approval.permission_digest != audit.permissions.digest()
        {
            return Err(LifecycleError::ApprovalMismatch);
        }
        if self.consumed_approval_nonces.contains(&approval.nonce) {
            return Err(LifecycleError::ApprovalAlreadyConsumed);
        }
        let version = self
            .versions
            .get_mut(&id)
            .ok_or(LifecycleError::UnknownVersion)?;
        version.state = VersionState::Promoted;
        let nonce = approval.nonce.clone();
        version.promotion_record = Some(PromotionRecord {
            source: approval.source,
            version_id: approval.version_id,
            staged_revision: approval.staged_revision,
            staged_audit_anchor: approval.staged_audit_anchor,
            package_digest: approval.package_digest,
            permission_digest: approval.permission_digest,
            actor_id: approval.actor_id,
            decision_id: approval.decision_id,
            nonce: approval.nonce,
        });
        self.consumed_approval_nonces.insert(nonce);
        self.revision = next_revision;
        Ok(())
    }

    /// Makes one Promoted version the current Runnable version.
    ///
    /// # Errors
    ///
    /// Returns a typed revision, version, or state-transition error.
    pub fn enable(&mut self, expected_revision: u64, id: VersionId) -> Result<(), LifecycleError> {
        self.require_revision(expected_revision)?;
        let version = self
            .versions
            .get(&id)
            .ok_or(LifecycleError::UnknownVersion)?;
        require_state(version.state, VersionState::Promoted)?;
        let next_revision = self.next_revision()?;
        let version = self
            .versions
            .get_mut(&id)
            .ok_or(LifecycleError::UnknownVersion)?;
        version.state = VersionState::Runnable;
        self.rollback_target = self.current_runnable;
        self.current_runnable = Some(id);
        self.revision = next_revision;
        Ok(())
    }

    /// Rejects any pre-Runnable package version.
    ///
    /// # Errors
    ///
    /// Returns a typed revision, version, or state-transition error.
    pub fn reject(&mut self, expected_revision: u64, id: VersionId) -> Result<(), LifecycleError> {
        self.require_revision(expected_revision)?;
        let version = self
            .versions
            .get(&id)
            .ok_or(LifecycleError::UnknownVersion)?;
        if !matches!(
            version.state,
            VersionState::Candidate | VersionState::Staged | VersionState::Promoted
        ) {
            return Err(LifecycleError::InvalidTransition {
                actual: version.state,
                required: VersionState::Candidate,
            });
        }
        let next_revision = self.next_revision()?;
        let version = self
            .versions
            .get_mut(&id)
            .ok_or(LifecycleError::UnknownVersion)?;
        version.state = VersionState::Rejected;
        self.revision = next_revision;
        Ok(())
    }

    /// Rolls back the current Runnable version to its retained predecessor.
    ///
    /// # Errors
    ///
    /// Returns a typed revision, missing-pointer, version, or state error.
    pub fn rollback(&mut self, expected_revision: u64) -> Result<VersionId, LifecycleError> {
        self.require_revision(expected_revision)?;
        let current = self
            .current_runnable
            .ok_or(LifecycleError::NoRunnableVersion)?;
        let target = self
            .rollback_target
            .ok_or(LifecycleError::NoRollbackTarget)?;
        let current_version = self
            .versions
            .get(&current)
            .ok_or(LifecycleError::UnknownVersion)?;
        require_state(current_version.state, VersionState::Runnable)?;
        let target_version = self
            .versions
            .get(&target)
            .ok_or(LifecycleError::UnknownVersion)?;
        require_state(target_version.state, VersionState::Runnable)?;
        let next_revision = self.next_revision()?;
        let current_version = self
            .versions
            .get_mut(&current)
            .ok_or(LifecycleError::UnknownVersion)?;
        current_version.state = VersionState::RolledBack;
        self.current_runnable = Some(target);
        self.rollback_target = None;
        self.revision = next_revision;
        Ok(target)
    }

    fn require_revision(&self, expected: u64) -> Result<(), LifecycleError> {
        if self.revision == expected {
            Ok(())
        } else {
            Err(LifecycleError::RevisionConflict {
                expected,
                actual: self.revision,
            })
        }
    }

    fn next_revision(&self) -> Result<u64, LifecycleError> {
        self.revision
            .checked_add(1)
            .ok_or(LifecycleError::RevisionOverflow)
    }
}

fn require_state(actual: VersionState, required: VersionState) -> Result<(), LifecycleError> {
    if actual == required {
        Ok(())
    } else {
        Err(LifecycleError::InvalidTransition { actual, required })
    }
}

fn first_use_receipt_matches(
    receipt: &FirstUseReceipt,
    approval: &FirstUseApproval,
    result_digest: &str,
) -> bool {
    receipt.source == approval.source
        && receipt.version_id == approval.version_id
        && receipt.approved_lifecycle_revision == approval.lifecycle_revision
        && receipt.staged_revision == approval.staged_revision
        && receipt.staged_audit_anchor == approval.staged_audit_anchor
        && receipt.package_digest == approval.package_digest
        && receipt.permission_digest == approval.permission_digest
        && receipt.actor_id == approval.actor_id
        && receipt.decision_id == approval.decision_id
        && receipt.nonce == approval.nonce
        && receipt.result_digest == result_digest
        && receipt.receipt_digest
            == first_use_receipt_digest(
                approval,
                receipt.committed_lifecycle_revision,
                result_digest,
            )
}

fn first_use_receipt_digest(
    approval: &FirstUseApproval,
    committed_revision: u64,
    result_digest: &str,
) -> String {
    let fields = [
        approval.source.owner.as_bytes(),
        approval.source.repo.as_bytes(),
        approval.source.package_path.as_bytes(),
        approval.source.commit.as_bytes(),
        &approval.version_id.0.to_be_bytes(),
        &approval.lifecycle_revision.to_be_bytes(),
        &committed_revision.to_be_bytes(),
        &approval.staged_revision.to_be_bytes(),
        approval.staged_audit_anchor.as_str().as_bytes(),
        approval.package_digest.as_bytes(),
        approval.permission_digest.as_bytes(),
        approval.actor_id.as_bytes(),
        approval.decision_id.as_bytes(),
        approval.nonce.as_bytes(),
        result_digest.as_bytes(),
    ];
    let mut hasher = Sha256::new();
    hasher.update(b"openopen-skill-first-no-effect-use-v1");
    for field in fields {
        hasher.update(u64::try_from(field.len()).unwrap_or(u64::MAX).to_be_bytes());
        hasher.update(field);
    }
    hex::encode(hasher.finalize())
}

fn valid_github_atom(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && value != "."
        && value != ".."
}

fn valid_github_ref(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && value != "."
        && value != ".."
}

fn is_immutable_commit(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn normalize_source_path(segments: &[&str]) -> Result<String, SourceError> {
    if segments.is_empty() {
        return Ok(String::new());
    }
    if segments.len() > MAX_PATH_DEPTH || segments.iter().any(|segment| !valid_github_atom(segment))
    {
        return Err(SourceError::InvalidPackagePath);
    }
    let path = segments.join("/");
    if path.len() > MAX_PATH_BYTES {
        return Err(SourceError::InvalidPackagePath);
    }
    Ok(path)
}

fn normalize_package_path(value: &str) -> Result<String, AuditError> {
    if value.is_empty()
        || value.starts_with('/')
        || value.ends_with('/')
        || value.contains('\\')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'-' | b'_'))
        || value.chars().any(is_unsafe_path_character)
    {
        return Err(AuditError::InvalidPath);
    }
    let normalized: String = value.nfc().collect();
    if normalized.chars().any(is_unsafe_path_character) {
        return Err(AuditError::InvalidPath);
    }
    if normalized.len() > MAX_PATH_BYTES {
        return Err(AuditError::PathTooLong);
    }
    let components: Vec<&str> = normalized.split('/').collect();
    if components.len() > MAX_PATH_DEPTH
        || components
            .iter()
            .any(|component| component.is_empty() || matches!(*component, "." | ".."))
    {
        return Err(AuditError::InvalidPath);
    }
    Ok(normalized)
}

pub(crate) fn is_unsafe_path_character(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{061c}'
                | '\u{200b}'..='\u{200f}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2060}'
                | '\u{2066}'..='\u{2069}'
                | '\u{feff}'
        )
}

fn normalize_and_validate_entries(
    entries: &[PackageEntry],
) -> Result<(Vec<NormalizedEntry>, BTreeSet<String>, u64), AuditError> {
    let mut normalized_entries = Vec::with_capacity(entries.len());
    let mut exact_paths = BTreeSet::new();
    let mut casefold_paths = BTreeSet::new();
    let mut total_bytes = 0_u64;

    for entry in entries {
        let normalized_path = normalize_package_path(&entry.path)?;
        match entry.kind {
            EntryKind::File => {}
            EntryKind::Symlink => return Err(AuditError::Symlink),
            EntryKind::Submodule => return Err(AuditError::Submodule),
            EntryKind::Special => return Err(AuditError::SpecialFile),
        }
        if entry.mode != REGULAR_NON_EXECUTABLE_MODE {
            return Err(AuditError::ExecutableOrUnsupportedMode);
        }
        if !exact_paths.insert(normalized_path.clone()) {
            return Err(AuditError::DuplicateNormalizedPath);
        }
        if !casefold_paths.insert(normalized_path.to_ascii_lowercase()) {
            return Err(AuditError::AsciiCaseCollision);
        }

        let file_bytes = u64::try_from(entry.bytes.len()).map_err(|_| AuditError::FileTooLarge)?;
        if file_bytes > MAX_FILE_BYTES {
            return Err(AuditError::FileTooLarge);
        }
        total_bytes = total_bytes
            .checked_add(file_bytes)
            .ok_or(AuditError::TotalBytesExceeded)?;
        if total_bytes > MAX_TOTAL_BYTES {
            return Err(AuditError::TotalBytesExceeded);
        }

        if archive_magic(&entry.bytes) {
            return Err(AuditError::NestedArchive);
        }
        let text = str::from_utf8(&entry.bytes).map_err(|_| AuditError::NonUtf8OrBinary)?;
        if text.contains('\0') {
            return Err(AuditError::NonUtf8OrBinary);
        }
        if text
            .trim_start_matches(['\u{feff}', ' ', '\t', '\r', '\n'])
            .starts_with("version https://git-lfs.github.com/spec/v1")
        {
            return Err(AuditError::GitLfsPointer);
        }
        normalized_entries.push(NormalizedEntry {
            path: normalized_path,
            bytes: entry.bytes.clone(),
        });
    }
    Ok((normalized_entries, exact_paths, total_bytes))
}

fn validate_root_contract(
    entries: &[NormalizedEntry],
    exact_paths: &BTreeSet<String>,
) -> Result<(AcceptedLicense, String), AuditError> {
    if !exact_paths.contains("SKILL.md") {
        return Err(AuditError::MissingRootSkill);
    }
    let license_paths: Vec<&str> = exact_paths
        .iter()
        .map(String::as_str)
        .filter(|path| matches!(*path, "LICENSE" | "LICENSE.txt" | "COPYING"))
        .collect();
    if license_paths.len() != 1 {
        return Err(AuditError::MissingOrAmbiguousRootLicense);
    }
    let license_entry = entries
        .iter()
        .find(|entry| entry.path == license_paths[0])
        .ok_or(AuditError::MissingOrAmbiguousRootLicense)?;
    let license_text = str::from_utf8(&license_entry.bytes)
        .map_err(|_| AuditError::MissingOrAmbiguousRootLicense)?;
    let license = classify_license(license_text).ok_or(AuditError::UnsupportedLicense)?;
    Ok((license, license_paths[0].to_owned()))
}

fn validate_instruction_contract(
    entries: &[NormalizedEntry],
    license_path: &str,
) -> Result<(), AuditError> {
    for entry in entries {
        if entry.path == license_path {
            continue;
        }
        let lower_path = entry.path.to_ascii_lowercase();
        if forbidden_extension(&lower_path) {
            return Err(AuditError::ScriptOrExecutableContent);
        }
        if nested_archive_extension(&lower_path) {
            return Err(AuditError::NestedArchive);
        }
        let text = str::from_utf8(&entry.bytes).map_err(|_| AuditError::NonUtf8OrBinary)?;
        match entry.path.as_str() {
            "SKILL.md" => {
                let body = validate_skill_front_matter(text)?;
                validate_instruction_text(body, true)?;
            }
            "agents/openai.yaml" => validate_openai_manifest(text)?,
            _ if has_extension(&lower_path, "md") => {
                if text.starts_with("---\n") || text.starts_with("---\r\n") {
                    return Err(AuditError::UnsupportedManifest);
                }
                validate_instruction_text(text, true)?;
            }
            _ if has_extension(&lower_path, "txt") => validate_instruction_text(text, false)?,
            _ => return Err(AuditError::UnsupportedManifest),
        }
    }
    Ok(())
}

fn validate_dependencies(
    entries: &[NormalizedEntry],
    exact_paths: &BTreeSet<String>,
) -> Result<(), AuditError> {
    for entry in entries {
        if !entry.path.to_ascii_lowercase().ends_with(".md") {
            continue;
        }
        let text = str::from_utf8(&entry.bytes).map_err(|_| AuditError::NonUtf8OrBinary)?;
        let text = if entry.path == "SKILL.md" {
            skill_body_for_dependency_scan(text)
        } else {
            text
        };
        for target in obvious_inline_targets(text) {
            let resolved = resolve_package_dependency(&entry.path, &target)?;
            if !exact_paths.contains(&resolved) {
                return Err(AuditError::OutOfPathOrMissingDependency);
            }
        }
        for target in markdown_local_targets(text)? {
            let resolved = resolve_package_dependency(&entry.path, &target)?;
            if !exact_paths.contains(&resolved) {
                return Err(AuditError::OutOfPathOrMissingDependency);
            }
        }
    }
    Ok(())
}

fn skill_body_for_dependency_scan(text: &str) -> &str {
    let Some(remainder) = text.strip_prefix("---\n") else {
        return text;
    };
    let Some(end) = remainder.find("\n---\n") else {
        return text;
    };
    &remainder[end + "\n---\n".len()..]
}

fn obvious_inline_targets(text: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut remaining = text;
    while let Some(marker) = remaining.find("](") {
        let open = remaining[..marker].rfind('[');
        let is_image =
            open.is_some_and(|open| open > 0 && remaining.as_bytes().get(open - 1) == Some(&b'!'));
        let is_escaped =
            open.is_some_and(|open| open > 0 && remaining.as_bytes().get(open - 1) == Some(&b'\\'));
        remaining = &remaining[marker + 2..];
        let Some(end) = remaining.find(')') else {
            break;
        };
        let raw = remaining[..end].trim();
        remaining = &remaining[end + 1..];
        if is_image
            || is_escaped
            || raw.is_empty()
            || raw.starts_with('#')
            || raw.contains(['\\', '?', '%'])
            || raw.chars().any(char::is_whitespace)
        {
            continue;
        }
        let target = raw.split('#').next().unwrap_or_default();
        if !target.is_empty() && !target.contains(':') {
            targets.push(target.to_owned());
        }
    }
    targets
}

fn canonical_digest(entries: &[NormalizedEntry]) -> Result<String, AuditError> {
    let mut hasher = Sha256::new();
    for entry in entries {
        hasher.update(entry.path.as_bytes());
        hasher.update([0]);
        hasher.update(
            u64::try_from(entry.bytes.len())
                .map_err(|_| AuditError::FileTooLarge)?
                .to_be_bytes(),
        );
        hasher.update([0]);
        hasher.update(&entry.bytes);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn forbidden_extension(path: &str) -> bool {
    [
        ".sh", ".bash", ".zsh", ".fish", ".py", ".pyc", ".js", ".mjs", ".cjs", ".ts", ".tsx",
        ".jsx", ".rb", ".pl", ".ps1", ".command", ".exe", ".dll", ".dylib", ".so", ".wasm", ".jar",
        ".class", ".app", ".pkg", ".dmg", ".bin",
    ]
    .iter()
    .any(|extension| path.ends_with(extension))
}

fn nested_archive_extension(path: &str) -> bool {
    [
        ".zip", ".tar", ".tgz", ".tar.gz", ".gz", ".bz2", ".xz", ".7z", ".rar",
    ]
    .iter()
    .any(|extension| path.ends_with(extension))
}

fn has_extension(path: &str, expected: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
}

fn classify_license(text: &str) -> Option<AcceptedLicense> {
    let canonical = canonical_license_body(text)?;
    let digest = hex::encode(Sha256::digest(canonical.as_bytes()));
    match digest.as_str() {
        // SPDX license-list-data v3.27.0 canonical text, with only validated
        // copyright header rows removed and whitespace reflowed.
        "7c9b48b52decb9837c70f608678129e1ac79e056829c8d1e82e8cdd8aed562f8" => {
            Some(AcceptedLicense::Mit)
        }
        "0ffddef9e48f8a09aed5caf2d44f7ba1c1be2d9b8e0a6f693b1635b2d5566645" => {
            Some(AcceptedLicense::Apache2)
        }
        "971850f7f84e5dc88bd6b52e0d0c1fbffdaec4eae2fd311b1511d42bf7855c51" => {
            Some(AcceptedLicense::Bsd2Clause)
        }
        "70435d703f5e4bdca9a1fa6b9291501b0dd850d2ad5457b491ce936217b1e063" => {
            Some(AcceptedLicense::Bsd3Clause)
        }
        "6be198129e93c5876148c3ee8bcc8a146fea1d51f8963e3a0cd611d8bbb5f5e8" => {
            Some(AcceptedLicense::Isc)
        }
        _ => None,
    }
}

fn markdown_local_targets(text: &str) -> Result<Vec<String>, AuditError> {
    let mut targets = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\\'
                if bytes.get(index + 1).is_some_and(|next| {
                    matches!(*next, b'[' | b']' | b'(' | b')' | b'!' | b'<' | b'>' | b'%')
                }) =>
            {
                return Err(AuditError::UnsupportedMarkdown);
            }
            b'!' if bytes.get(index + 1) == Some(&b'[') => {
                return Err(AuditError::UnsupportedMarkdown);
            }
            b'<' | b'>' => return Err(AuditError::UnsupportedMarkdown),
            b'&' if looks_like_html_entity(&bytes[index..]) => {
                return Err(AuditError::ObfuscatedContent);
            }
            b'%' if bytes
                .get(index + 1..index + 3)
                .is_some_and(|pair| pair.iter().all(u8::is_ascii_hexdigit)) =>
            {
                return Err(AuditError::ObfuscatedContent);
            }
            b'[' => {
                let label_end = find_unescaped(bytes, index + 1, b']')
                    .ok_or(AuditError::MalformedMarkdownDependency)?;
                if bytes[index + 1..label_end]
                    .iter()
                    .any(|byte| matches!(*byte, b'[' | b']' | b'\n' | b'\r'))
                {
                    return Err(AuditError::UnsupportedMarkdown);
                }
                if bytes.get(label_end + 1) != Some(&b'(') {
                    if matches!(bytes.get(label_end + 1), Some(b'[' | b':'))
                        || !literal_bracket_label(&bytes[index + 1..label_end])
                    {
                        return Err(AuditError::UnsupportedMarkdown);
                    }
                    index = label_end + 1;
                    continue;
                }
                let target_start = label_end + 2;
                let target_end = find_unescaped(bytes, target_start, b')')
                    .ok_or(AuditError::MalformedMarkdownDependency)?;
                let raw = str::from_utf8(&bytes[target_start..target_end])
                    .map_err(|_| AuditError::MalformedMarkdownDependency)?;
                if raw.as_bytes().windows(3).any(|window| {
                    window[0] == b'%'
                        && window[1].is_ascii_hexdigit()
                        && window[2].is_ascii_hexdigit()
                }) {
                    return Err(AuditError::ObfuscatedContent);
                }
                if raw.is_empty()
                    || raw != raw.trim()
                    || raw.chars().any(|character| {
                        character.is_ascii_whitespace()
                            || matches!(character, '(' | '\\' | '?' | '<' | '>' | '"' | '\'')
                    })
                {
                    return Err(AuditError::UnsupportedMarkdown);
                }
                if raw.starts_with('#') {
                    index = target_end + 1;
                    continue;
                }
                let without_fragment = raw.split('#').next().unwrap_or_default();
                if without_fragment.is_empty()
                    || without_fragment.contains(':')
                    || without_fragment.starts_with('/')
                {
                    return Err(AuditError::OutOfPathOrMissingDependency);
                }
                targets.push(without_fragment.to_owned());
                index = target_end + 1;
                continue;
            }
            b']' => return Err(AuditError::MalformedMarkdownDependency),
            _ => {}
        }
        index += 1;
    }
    Ok(targets)
}

fn literal_bracket_label(label: &[u8]) -> bool {
    !label.is_empty()
        && label.len() <= 128
        && label.iter().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(*byte, b' ' | b'-' | b'_' | b'/' | b'.' | b',' | b'?' | b'!')
        })
}

fn find_unescaped(bytes: &[u8], start: usize, needle: u8) -> Option<usize> {
    let mut escaped = false;
    for (offset, byte) in bytes.iter().enumerate().skip(start) {
        if escaped {
            escaped = false;
            continue;
        }
        if *byte == b'\\' {
            escaped = true;
        } else if *byte == needle {
            return Some(offset);
        }
    }
    None
}

fn looks_like_html_entity(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .take(16)
        .position(|byte| *byte == b';')
        .is_some_and(|end| {
            end > 1
                && bytes[1..end]
                    .iter()
                    .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'#')
        })
}

fn resolve_package_dependency(from_path: &str, target: &str) -> Result<String, AuditError> {
    if target.starts_with('/') {
        return Err(AuditError::OutOfPathOrMissingDependency);
    }
    let mut resolved: Vec<&str> = from_path.split('/').collect();
    resolved.pop();
    for component in target.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if resolved.pop().is_none() {
                    return Err(AuditError::OutOfPathOrMissingDependency);
                }
            }
            other => resolved.push(other),
        }
    }
    normalize_package_path(&resolved.join("/"))
        .map_err(|_| AuditError::OutOfPathOrMissingDependency)
}

fn validate_skill_front_matter(text: &str) -> Result<&str, AuditError> {
    let remainder = text
        .strip_prefix("---\n")
        .ok_or(AuditError::UnsupportedManifest)?;
    let end = remainder
        .find("\n---\n")
        .ok_or(AuditError::UnsupportedManifest)?;
    if end > MAX_FRONT_MATTER_BYTES {
        return Err(AuditError::UnsupportedManifest);
    }
    let front_matter = &remainder[..end];
    let mut fields = BTreeMap::new();
    for line in front_matter.lines() {
        if line.is_empty()
            || line.starts_with(|character: char| character.is_ascii_whitespace())
            || line.contains('\t')
            || line.contains('#')
            || line.contains('&')
            || line.contains('*')
            || line.contains('|')
            || line.contains('>')
            || line.contains('!')
            || line.starts_with('%')
        {
            return Err(AuditError::UnsupportedManifest);
        }
        let (key, raw_value) = line
            .split_once(':')
            .ok_or(AuditError::UnsupportedManifest)?;
        if !matches!(key, "name" | "description") || fields.contains_key(key) {
            return Err(AuditError::UnsupportedManifest);
        }
        let value = parse_manifest_scalar(raw_value)?;
        if key == "name"
            && (value.len() > 64
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
                || value.starts_with('-')
                || value.ends_with('-'))
        {
            return Err(AuditError::UnsupportedManifest);
        }
        if key == "description" {
            validate_instruction_payload(value)?;
        }
        fields.insert(key, value);
    }
    if !fields.contains_key("name") || !fields.contains_key("description") {
        return Err(AuditError::UnsupportedManifest);
    }
    let body = &remainder[end + "\n---\n".len()..];
    if body.trim().is_empty() {
        return Err(AuditError::UnsupportedManifest);
    }
    Ok(body)
}

fn validate_openai_manifest(text: &str) -> Result<(), AuditError> {
    validate_instruction_characters(text)?;
    let mut section = None;
    let mut sections = BTreeSet::new();
    let mut fields = BTreeSet::new();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        if line.contains('\t')
            || line.contains('#')
            || line.contains('&')
            || line.contains('*')
            || line.contains('!')
            || line.contains('|')
            || line.contains('>')
            || line.starts_with('%')
        {
            return Err(AuditError::UnsupportedManifest);
        }
        if !line.starts_with(' ') {
            let parsed = match line {
                "interface:" => Some("interface"),
                "policy:" => Some("policy"),
                _ => return Err(AuditError::UnsupportedManifest),
            };
            if !sections.insert(parsed.ok_or(AuditError::UnsupportedManifest)?) {
                return Err(AuditError::UnsupportedManifest);
            }
            section = parsed;
            continue;
        }
        if !line.starts_with("  ") || line.starts_with("   ") {
            return Err(AuditError::UnsupportedManifest);
        }
        let current = section.ok_or(AuditError::UnsupportedManifest)?;
        let (key, raw_value) = line[2..]
            .split_once(':')
            .ok_or(AuditError::UnsupportedManifest)?;
        let qualified = format!("{current}.{key}");
        if !fields.insert(qualified.clone()) {
            return Err(AuditError::UnsupportedManifest);
        }
        match qualified.as_str() {
            "interface.display_name"
            | "interface.short_description"
            | "interface.default_prompt" => {
                let value = parse_manifest_scalar(raw_value)?;
                validate_instruction_payload(value)?;
            }
            "policy.allow_implicit_invocation" => {
                if raw_value.trim() != "false" {
                    return Err(AuditError::PermissionExpansion);
                }
            }
            _ => return Err(AuditError::UnsupportedManifest),
        }
    }
    let required = [
        "interface.display_name",
        "interface.short_description",
        "interface.default_prompt",
        "policy.allow_implicit_invocation",
    ];
    if required.iter().any(|field| !fields.contains(*field)) {
        return Err(AuditError::UnsupportedManifest);
    }
    Ok(())
}

fn parse_manifest_scalar(raw: &str) -> Result<&str, AuditError> {
    let value = raw.trim();
    if value.is_empty() || value.len() > 1_024 || value.contains(['\n', '\r', '\t']) {
        return Err(AuditError::UnsupportedManifest);
    }
    let (value, quoted) = if let Some(inner) = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    {
        if inner.contains(['"', '\\']) {
            return Err(AuditError::UnsupportedManifest);
        }
        (inner, true)
    } else {
        if value.contains(['"', '\'', '[', ']', '{', '}']) {
            return Err(AuditError::UnsupportedManifest);
        }
        (value, false)
    };
    if value.is_empty()
        || value.contains(['[', ']', '{', '}', '<', '>', '\\', '&', '*', '|', '`'])
        || (!quoted && (value.starts_with(['-', '?', ':', '!', '%', '@']) || value.contains(": ")))
    {
        return Err(AuditError::UnsupportedManifest);
    }
    Ok(value)
}

fn validate_instruction_text(text: &str, markdown: bool) -> Result<(), AuditError> {
    validate_instruction_payload(text)?;
    if markdown {
        markdown_local_targets(text)?;
    }
    Ok(())
}

fn validate_instruction_payload(text: &str) -> Result<(), AuditError> {
    validate_instruction_characters(text)?;
    let lower = text.to_ascii_lowercase();
    if [
        "http://",
        "https://",
        "ftp://",
        "ssh://",
        "git://",
        "mailto:",
        "file:",
        "data:",
        "javascript:",
        "www.",
        "../",
        "//",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
        || contains_bare_email(text)
    {
        return Err(AuditError::OutOfPathOrMissingDependency);
    }
    if text.contains(['<', '>']) {
        return Err(AuditError::UnsupportedMarkdown);
    }
    if contains_encoded_escape(text)
        || text
            .as_bytes()
            .iter()
            .enumerate()
            .any(|(index, byte)| *byte == b'&' && looks_like_html_entity(&text.as_bytes()[index..]))
    {
        return Err(AuditError::ObfuscatedContent);
    }
    validate_no_executable_forms(text)?;
    validate_no_authority_conflict(text)?;
    validate_bounded_instruction_grammar(text)
}

fn contains_bare_email(text: &str) -> bool {
    instruction_tokens_with_punctuation(text).any(|token| {
        token.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && domain
                    .split_once('.')
                    .is_some_and(|(host, suffix)| !host.is_empty() && !suffix.is_empty())
        })
    })
}

fn instruction_tokens_with_punctuation(text: &str) -> impl Iterator<Item = &str> {
    text.split(|character: char| character.is_ascii_whitespace() || matches!(character, '(' | ')'))
        .map(|token| {
            token.trim_matches(|character: char| matches!(character, ',' | ';' | '!' | '?'))
        })
        .filter(|token| !token.is_empty())
}

fn contains_encoded_escape(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.windows(3).any(|window| {
        (window[0] == b'%' && window[1].is_ascii_hexdigit() && window[2].is_ascii_hexdigit())
            || (window[0] == b'\\'
                && (matches!(window[1], b'x' | b'u' | b'U')
                    || (matches!(window[1], b'0'..=b'7') && window[2].is_ascii_hexdigit())))
    })
}

fn validate_instruction_characters(text: &str) -> Result<(), AuditError> {
    if text.contains('\r')
        || text.nfkc().collect::<String>() != text
        || text
            .chars()
            .any(|character| character != '\n' && (!character.is_ascii() || character.is_control()))
    {
        return Err(AuditError::ObfuscatedContent);
    }
    Ok(())
}

fn validate_no_executable_forms(text: &str) -> Result<(), AuditError> {
    if text.lines().any(line_has_executable_form) {
        return Err(AuditError::ScriptOrExecutableContent);
    }

    if text.contains('`') || text.contains("~~~") {
        return Err(AuditError::ScriptOrExecutableContent);
    }

    let tokens = instruction_tokens(text);
    let compact = tokens.join("");
    if tokens
        .iter()
        .any(|token| SCRIPT_RUNTIME_TOKENS.contains(&token.as_str()))
        || OBFUSCATED_SCRIPT_MARKERS
            .iter()
            .any(|marker| compact.contains(marker))
        || tokens
            .windows(2)
            .any(|window| matches!(window, [left, right] if left == "s" && right == "h"))
        || text.contains("$(")
        || text.contains("${")
        || text.contains("child_process")
        || text.contains("module.exports")
        || text.contains("os.system")
        || text.contains("process.")
        || text.contains("subprocess.")
        || text.contains("__import__")
    {
        return Err(AuditError::ScriptOrExecutableContent);
    }
    Ok(())
}

fn line_has_executable_form(line: &str) -> bool {
    let trimmed = line.trim_start();
    let line_tokens = instruction_tokens(trimmed);
    let first = line_tokens
        .iter()
        .find(|token| !token.bytes().all(|byte| byte.is_ascii_digit()))
        .map(String::as_str);
    trimmed.starts_with("#!")
        || line.starts_with("    ")
        || line.starts_with('\t')
        || trimmed.starts_with("```")
        || trimmed.starts_with("~~~")
        || trimmed.starts_with("/bin/")
        || trimmed.starts_with("/usr/bin/")
        || trimmed.starts_with("/usr/local/bin/")
        || trimmed.starts_with("./")
        || trimmed.starts_with("read ")
        || shell_assignment(trimmed)
        || spaced_language_assignment(trimmed)
        || shell_function_declaration(trimmed)
        || function_call_expression(trimmed)
        || contains_shell_pipeline(trimmed)
        || contains_shell_list_separator(trimmed)
        || contains_shell_background_operator(trimmed)
        || trimmed.contains("; do")
        || trimmed.contains("; then")
        || matches!(trimmed, "do" | "done" | "else" | "esac" | "fi" | "then")
        || first.is_some_and(|token| LEADING_COMMAND_TOKENS.contains(&token))
        || matches!(first, Some("import" | "export"))
        || (first == Some("from") && line_tokens.iter().any(|token| token == "import"))
        || (first == Some("def") && trimmed.contains(':'))
        || (first == Some("class") && trimmed.contains(':'))
        || (looks_executable(trimmed)
            && (trimmed.contains('=')
                || trimmed.contains(';')
                || trimmed.contains("()")
                || trimmed.contains("('")
                || trimmed.contains("(\"")))
}

fn shell_assignment(value: &str) -> bool {
    let value = value.trim_start_matches(['-', '*', '+', ' ']);
    let first = value.split_ascii_whitespace().next().unwrap_or_default();
    let Some((name, _assigned)) = first.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && name.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_alphabetic() || byte == b'_' || (index > 0 && byte.is_ascii_digit())
        })
}

fn spaced_language_assignment(value: &str) -> bool {
    let value = value.trim_start_matches(['-', '*', '+', ' ']);
    let Some(equal) = value.find('=') else {
        return false;
    };
    if value.as_bytes().get(equal + 1) == Some(&b'=') {
        return false;
    }
    let before = value[..equal].trim_end();
    if before.ends_with(['!', '='])
        || (before.ends_with('<') && !before.ends_with("<<"))
        || (before.ends_with('>') && !before.ends_with(">>"))
    {
        return false;
    }
    let target = before
        .trim_end_matches(['+', '-', '*', '/', '%', '&', '|', '^', '?', ':', '<', '>'])
        .trim_end();
    assignment_target(target)
}

fn assignment_target(value: &str) -> bool {
    let value = value.trim();
    if value.starts_with(['{', '[', '(']) {
        return true;
    }
    value.split(',').all(|component| {
        let component = component.trim();
        let target = component
            .split_once(':')
            .map_or(component, |(target, _annotation)| target.trim());
        !target.is_empty()
            && target.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(byte, b'_' | b'.' | b'[' | b']' | b'\'' | b'"')
            })
            && target
                .as_bytes()
                .first()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
    })
}

fn shell_function_declaration(value: &str) -> bool {
    let value = value.trim_start_matches(['-', '*', '+', ' ']);
    let first = value.split_ascii_whitespace().next().unwrap_or_default();
    (first.ends_with("()") && value.contains('{'))
        || ((first == "function" || value.starts_with("async function "))
            && (value.contains('(') || value.contains('{')))
}

fn function_call_expression(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.iter().enumerate().any(|(index, byte)| {
        *byte == b'('
            && index > 0
            && matches!(bytes[index - 1], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
            && bytes[index + 1..].contains(&b')')
    })
}

fn contains_shell_pipeline(value: &str) -> bool {
    value.contains('|')
}

fn contains_shell_list_separator(value: &str) -> bool {
    value.contains(';')
}

fn contains_shell_background_operator(value: &str) -> bool {
    value.contains('&')
}

fn looks_executable(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let tokens = instruction_tokens(&lower);
    let command_tokens = [
        "bash",
        "cat",
        "cd",
        "chmod",
        "const",
        "cp",
        "curl",
        "echo",
        "eval",
        "exec",
        "export",
        "function",
        "import",
        "javascript",
        "let",
        "mv",
        "node",
        "npm",
        "npx",
        "pip",
        "print",
        "python",
        "require",
        "return",
        "rm",
        "sh",
        "shell",
        "sudo",
        "var",
        "wget",
        "zsh",
    ];
    tokens
        .iter()
        .any(|token| command_tokens.contains(&token.as_str()))
        || lower.contains("&&")
        || lower.contains("||")
        || lower.contains("$(")
        || lower.contains("=>")
}

fn validate_no_authority_conflict(text: &str) -> Result<(), AuditError> {
    let tokens = instruction_tokens(text);
    let compact = tokens.join("");
    if PERMISSION_PATTERNS
        .iter()
        .any(|pattern| compact.contains(&pattern.replace(' ', "")))
    {
        return Err(AuditError::PermissionExpansion);
    }

    if contains_sensitive_authority(&tokens) {
        return Err(AuditError::InstructionConflict);
    }

    if contains_any_token(&tokens, UNDECLARED_CAPABILITY_TOKENS) {
        return Err(AuditError::PermissionExpansion);
    }

    let contains_any = |needles: &[&str]| contains_any_token(&tokens, needles);
    if (contains_any(&["bypass", "circumvent", "override", "skip", "without"])
        && contains_any(&[
            "approval",
            "asking",
            "confirmation",
            "consent",
            "gate",
            "owner",
            "review",
        ]))
        || (contains_any(&["change", "choose", "select", "switch"])
            && contains_any(&["effort", "model", "provider"]))
        || (contains_any(&[
            "add", "broaden", "contact", "email", "expand", "forward", "message", "post",
            "publish", "send", "transmit",
        ]) && contains_any(&["email", "message", "recipient", "recipients", "thirdparty"]))
        || (contains_any(&["persist", "retain", "save", "store"])
            && contains_any(&["data", "history", "information", "record", "records"]))
        || (contains_any(&["disregard", "ignore", "override"])
            && contains_any(&[
                "above",
                "contract",
                "developer",
                "directions",
                "earlier",
                "guardrail",
                "guardrails",
                "instruction",
                "instructions",
                "previous",
                "prior",
                "rule",
                "rules",
                "system",
            ]))
        || (contains_any(&[
            "collect", "expose", "extract", "obtain", "read", "reveal", "show",
        ]) && contains_any(&[
            "credential",
            "credentials",
            "key",
            "keys",
            "password",
            "passwords",
            "prompt",
            "secret",
            "secrets",
            "token",
            "tokens",
        ]))
        || (contains_any(&[
            "access", "call", "download", "invoke", "open", "upload", "write",
        ]) && contains_any(&[
            "channel",
            "channels",
            "file",
            "files",
            "filesystem",
            "network",
            "tool",
            "tools",
        ]))
        || (contains_any(&["private", "undeclared", "unapproved"])
            && contains_any(&["data", "file", "files", "tool", "tools"]))
        || (contains_any(&["add", "broaden", "create", "expand"])
            && contains_any(&["effect", "effects", "permission", "permissions"]))
    {
        return Err(AuditError::InstructionConflict);
    }

    if AUTHORITY_PATTERNS
        .iter()
        .any(|pattern| compact.contains(pattern))
    {
        return Err(AuditError::InstructionConflict);
    }
    Ok(())
}

fn contains_sensitive_authority(tokens: &[String]) -> bool {
    contains_any_token(tokens, &SENSITIVE_TOKENS)
        || (contains_any_token(tokens, &["api", "private"])
            && contains_any_token(tokens, &["key", "keys"]))
        || (contains_any_token(tokens, &["system"]) && contains_any_token(tokens, &["prompt"]))
}

fn contains_any_token(tokens: &[String], needles: &[&str]) -> bool {
    tokens.iter().any(|token| needles.contains(&token.as_str()))
}

fn validate_bounded_instruction_grammar(text: &str) -> Result<(), AuditError> {
    let tokens = instruction_tokens(text);
    if tokens.is_empty()
        || tokens.iter().any(|token| {
            !token.bytes().all(|byte| byte.is_ascii_digit())
                && !SAFE_INSTRUCTION_TOKENS.contains(&token.as_str())
        })
    {
        return Err(AuditError::InstructionConflict);
    }
    Ok(())
}

fn instruction_tokens(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect()
}

fn canonical_license_body(text: &str) -> Option<String> {
    const FIXED_SPDX_COPYRIGHT_LINES: [&str; 2] = [
        "Copyright (c) 2004-2010 by Internet Systems Consortium, Inc. (\"ISC\")",
        "Copyright (c) 1995-2003 by Internet Software Consortium",
    ];
    let normalized = text.replace("\r\n", "\n");
    let is_bsd3_template = normalized
        .lines()
        .any(|line| line.starts_with("3. Neither the name of the copyright holder"));
    if normalized.contains('\r')
        || normalized.contains('\u{feff}')
        || !normalized.is_ascii()
        || normalized
            .chars()
            .any(|character| character.is_control() && character != '\n' && character != '\t')
    {
        return None;
    }
    let mut copyright_lines = 0_usize;
    let mut retained = Vec::new();
    for (index, line) in normalized.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("Copyright (c) ") {
            let valid_variable_header = if is_bsd3_template {
                trimmed.strip_suffix('.').is_some_and(valid_copyright_line)
            } else {
                valid_copyright_line(trimmed)
            };
            if index > 6
                || !(FIXED_SPDX_COPYRIGHT_LINES.contains(&trimmed) || valid_variable_header)
            {
                return None;
            }
            copyright_lines += 1;
            if copyright_lines > 3 {
                return None;
            }
            continue;
        }
        retained.push(trimmed);
    }
    Some(
        retained
            .join("\n")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn valid_copyright_line(line: &str) -> bool {
    if line.len() > 256
        || !line.is_ascii()
        || !line
            .bytes()
            .all(|byte| byte.is_ascii_graphic() || byte == b' ')
    {
        return false;
    }
    let remainder = &line["Copyright (c) ".len()..];
    let year = remainder
        .split_ascii_whitespace()
        .next()
        .unwrap_or_default();
    let Some(holder) = remainder
        .strip_prefix(year)
        .and_then(|value| value.strip_prefix(' '))
    else {
        return false;
    };
    valid_copyright_year(year) && valid_copyright_holder(holder)
}

fn valid_copyright_year(value: &str) -> bool {
    let bytes = value.as_bytes();
    (bytes.len() == 4 && bytes.iter().all(u8::is_ascii_digit))
        || (bytes.len() == 9
            && bytes[4] == b'-'
            && bytes[..4].iter().all(u8::is_ascii_digit)
            && bytes[5..].iter().all(u8::is_ascii_digit)
            && bytes[..4] <= bytes[5..])
}

fn valid_copyright_holder(holder: &str) -> bool {
    !holder.is_empty()
        && holder.len() <= 80
        && holder
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && holder
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        && holder.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

fn archive_magic(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
        || bytes.starts_with(b"\x1f\x8b")
        || bytes.starts_with(b"BZh")
        || bytes.starts_with(b"\xfd7zXZ\0")
        || bytes.starts_with(b"7z\xbc\xaf'\x1c")
        || bytes.starts_with(b"Rar!\x1a\x07")
        || bytes.get(257..262) == Some(b"ustar")
}

fn valid_approval_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_APPROVAL_ID_BYTES
        && value.trim() == value
        && value
            .bytes()
            .all(|byte| byte.is_ascii_graphic() || byte == b' ')
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum SourceError {
    #[error("GitHub URL is not canonical")]
    NonCanonicalUrl,
    #[error("repository identity is invalid")]
    InvalidRepositoryIdentity,
    #[error("requested ref is invalid")]
    InvalidRequestedRef,
    #[error("package path is invalid")]
    InvalidPackagePath,
    #[error("resolved commit must be lowercase 40-hex")]
    InvalidResolvedCommit,
    #[error("immutable requested commit differs from resolved commit")]
    ResolvedCommitMismatch,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AuditError {
    #[error("package contains no entries")]
    EmptyPackage,
    #[error("package entry count exceeds the fixed limit")]
    TooManyEntries,
    #[error("package path is invalid")]
    InvalidPath,
    #[error("package path exceeds the fixed byte limit")]
    PathTooLong,
    #[error("package contains a duplicate normalized path")]
    DuplicateNormalizedPath,
    #[error("package contains an ASCII case-fold path collision")]
    AsciiCaseCollision,
    #[error("package contains a symlink")]
    Symlink,
    #[error("package contains a submodule")]
    Submodule,
    #[error("package contains a special file")]
    SpecialFile,
    #[error("package contains an executable or unsupported file mode")]
    ExecutableOrUnsupportedMode,
    #[error("package file exceeds the fixed byte limit")]
    FileTooLarge,
    #[error("package total bytes exceed the fixed limit")]
    TotalBytesExceeded,
    #[error("package contains non-UTF-8 or binary content")]
    NonUtf8OrBinary,
    #[error("package contains Unicode/control obfuscation")]
    ObfuscatedContent,
    #[error("package contains script or executable content")]
    ScriptOrExecutableContent,
    #[error("package contains a nested archive")]
    NestedArchive,
    #[error("package contains a Git LFS pointer")]
    GitLfsPointer,
    #[error("package instructions conflict with fixed authority")]
    InstructionConflict,
    #[error("package declares a permission expansion")]
    PermissionExpansion,
    #[error("package contains an unsupported instruction manifest or resource")]
    UnsupportedManifest,
    #[error("package contains unsupported Markdown syntax")]
    UnsupportedMarkdown,
    #[error("package is missing root SKILL.md")]
    MissingRootSkill,
    #[error("package must contain exactly one root license file")]
    MissingOrAmbiguousRootLicense,
    #[error("package license is not in the competition allowlist")]
    UnsupportedLicense,
    #[error("package markdown dependency is malformed")]
    MalformedMarkdownDependency,
    #[error("package references a missing or out-of-path dependency")]
    OutOfPathOrMissingDependency,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ApprovalError {
    #[error("promotion actor identity is invalid")]
    InvalidActorIdentity,
    #[error("promotion decision identity is invalid")]
    InvalidDecisionIdentity,
    #[error("promotion nonce must be lowercase 64-hex")]
    InvalidNonce,
    #[error("staged audit anchor must be lowercase 64-hex")]
    InvalidAuditAnchor,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum LifecycleError {
    #[error("expected revision {expected}, actual revision {actual}")]
    RevisionConflict { expected: u64, actual: u64 },
    #[error("revision overflow")]
    RevisionOverflow,
    #[error("version id overflow")]
    VersionIdOverflow,
    #[error("unknown skill version")]
    UnknownVersion,
    #[error("update changed owner, repo, or package path")]
    PackageIdentityChanged,
    #[error("no runnable version exists")]
    NoRunnableVersion,
    #[error("no rollback target exists")]
    NoRollbackTarget,
    #[error("version state {actual:?} does not satisfy required state {required:?}")]
    InvalidTransition {
        actual: VersionState,
        required: VersionState,
    },
    #[error("staged version is missing its audit")]
    MissingAudit,
    #[error("staged version is missing its revision or audit anchor")]
    MissingStagedBinding,
    #[error("promotion approval does not bind the exact package")]
    ApprovalMismatch,
    #[error("promotion approval nonce was already consumed")]
    ApprovalAlreadyConsumed,
    #[error("first use requires an instruction-only package with empty capability grants")]
    FirstUseRequiresInstructionOnly,
    #[error("first-use approval does not bind the exact runnable package and audit")]
    FirstUseApprovalMismatch,
    #[error("first-use approval nonce was already consumed")]
    FirstUseApprovalAlreadyConsumed,
    #[error("first use was already recorded with different evidence")]
    FirstUseAlreadyRecorded,
    #[error("first-use result digest must be lowercase 64-hex")]
    InvalidFirstUseResultDigest,
    #[error(transparent)]
    Audit(#[from] AuditError),
}

#[cfg(test)]
mod tests;
