use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use serde::Deserialize;
use sha1::{Digest as _, Sha1};
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;
use url::Url;

use crate::{
    EntryKind, GitHubProvenance, GitHubRequest, MAX_ENTRIES, MAX_FILE_BYTES, MAX_PATH_DEPTH,
    MAX_TOTAL_BYTES, PackageEntry, ResolvedPackage, SkillSource, is_immutable_commit,
    normalize_package_path, valid_github_ref,
};

const API_ORIGIN: &str = "https://api.github.com";
const API_VERSION: &str = "2022-11-28";
const MAX_API_BODY_BYTES: usize = 8 * 1024 * 1024;
const MAX_BLOB_API_BODY_BYTES: usize = 768 * 1024;
const MAX_TREE_RESPONSE_ENTRIES: usize = 4_096;
const MAX_TREE_OBJECTS: usize = MAX_ENTRIES + MAX_PATH_DEPTH + 1;
const MAX_API_REQUESTS: usize = (MAX_ENTRIES * 2) + MAX_PATH_DEPTH + 8;

/// Concrete, credential-free GitHub acquirer. It sends only fixed public API
/// requests to `api.github.com`, follows no redirects, and is the sole public
/// constructor of [`ResolvedPackage`].
#[derive(Clone)]
pub struct GitHubAcquirer {
    api: LiveGitHubApi,
}

impl std::fmt::Debug for GitHubAcquirer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GitHubAcquirer")
            .finish_non_exhaustive()
    }
}

impl Default for GitHubAcquirer {
    fn default() -> Self {
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .max_redirects_will_error(true)
            .timeout_global(Some(Duration::from_secs(30)))
            .user_agent("OpenOpen-Skill-Acquirer/1")
            .build();
        Self {
            api: LiveGitHubApi {
                agent: config.new_agent(),
            },
        }
    }
}

impl GitHubAcquirer {
    /// Resolves the requested public repository ref, verifies the exact
    /// repository/commit/tree/blob chain and all acquisition limits, and
    /// returns a sealed immutable package.
    ///
    /// # Errors
    ///
    /// Returns a typed, body-free failure for any network, identity,
    /// truncation, Git-object, mode, path, or fixed-limit violation.
    pub fn acquire(&self, request: &GitHubRequest) -> Result<ResolvedPackage, AcquisitionError> {
        acquire_with_api(&self.api, request)
    }
}

trait GitHubApi {
    fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        byte_limit: usize,
    ) -> Result<T, AcquisitionError>;
}

#[derive(Clone)]
struct LiveGitHubApi {
    agent: ureq::Agent,
}

impl GitHubApi for LiveGitHubApi {
    fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        byte_limit: usize,
    ) -> Result<T, AcquisitionError> {
        validate_api_url(url)?;
        let mut response = self
            .agent
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", API_VERSION)
            .call()
            .map_err(|_| AcquisitionError::Transport)?;
        if response.status().as_u16() != 200 {
            return Err(AcquisitionError::UnexpectedStatus);
        }
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if !content_type
            .to_ascii_lowercase()
            .starts_with("application/json")
        {
            return Err(AcquisitionError::UnexpectedContentType);
        }
        response
            .body_mut()
            .with_config()
            .limit(u64::try_from(byte_limit).map_err(|_| AcquisitionError::InvalidResponse)?)
            .read_json()
            .map_err(|_| AcquisitionError::InvalidResponse)
    }
}

fn validate_api_url(value: &str) -> Result<(), AcquisitionError> {
    let url = Url::parse(value).map_err(|_| AcquisitionError::InvalidEndpoint)?;
    if url.scheme() != "https"
        || url.host_str() != Some("api.github.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AcquisitionError::InvalidEndpoint);
    }
    Ok(())
}

#[derive(Deserialize)]
struct RepositoryResponse {
    full_name: String,
    default_branch: String,
}

#[derive(Deserialize)]
struct CommitResponse {
    sha: String,
    commit: CommitDetails,
}

#[derive(Deserialize)]
struct CommitDetails {
    tree: ObjectReference,
}

#[derive(Deserialize)]
struct ObjectReference {
    sha: String,
}

#[derive(Deserialize)]
struct TreeResponse {
    sha: String,
    truncated: Option<bool>,
    tree: Vec<TreeResponseEntry>,
}

#[derive(Deserialize)]
struct TreeResponseEntry {
    path: String,
    mode: String,
    #[serde(rename = "type")]
    object_type: String,
    sha: String,
    size: Option<u64>,
}

#[derive(Deserialize)]
struct BlobResponse {
    sha: String,
    size: u64,
    encoding: String,
    content: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GitEntryType {
    Blob,
    Tree,
    Commit,
}

#[derive(Clone, Debug)]
struct VerifiedTreeEntry {
    name: String,
    mode: u32,
    raw_mode: &'static str,
    object_type: GitEntryType,
    sha: String,
    size: Option<u64>,
}

#[derive(Clone, Debug)]
struct VerifiedTree {
    sha: String,
    entries: Vec<VerifiedTreeEntry>,
}

struct AcquisitionState<'a, A> {
    api: &'a A,
    repository_base: String,
    requests: usize,
    tree_objects: usize,
    blob_objects: usize,
    package_nodes: usize,
    package_files: usize,
    total_bytes: u64,
    fetched_trees: BTreeMap<String, VerifiedTree>,
    fetched_blobs: BTreeMap<String, Vec<u8>>,
}

impl<A: GitHubApi> AcquisitionState<'_, A> {
    fn get_json<T: serde::de::DeserializeOwned>(
        &mut self,
        suffix: &str,
        byte_limit: usize,
    ) -> Result<T, AcquisitionError> {
        self.requests = self
            .requests
            .checked_add(1)
            .ok_or(AcquisitionError::RequestLimitExceeded)?;
        if self.requests > MAX_API_REQUESTS {
            return Err(AcquisitionError::RequestLimitExceeded);
        }
        self.api
            .get_json(&format!("{}{suffix}", self.repository_base), byte_limit)
    }

    fn fetch_tree(&mut self, expected_sha: &str) -> Result<VerifiedTree, AcquisitionError> {
        if let Some(tree) = self.fetched_trees.get(expected_sha) {
            return Ok(tree.clone());
        }
        require_git_id(expected_sha)?;
        self.tree_objects = self
            .tree_objects
            .checked_add(1)
            .ok_or(AcquisitionError::TreeLimitExceeded)?;
        if self.tree_objects > MAX_TREE_OBJECTS {
            return Err(AcquisitionError::TreeLimitExceeded);
        }
        let response: TreeResponse =
            self.get_json(&format!("/git/trees/{expected_sha}"), MAX_API_BODY_BYTES)?;
        let tree = verify_tree_response(expected_sha, response)?;
        self.fetched_trees
            .insert(expected_sha.to_owned(), tree.clone());
        Ok(tree)
    }

    fn fetch_blob(
        &mut self,
        expected_sha: &str,
        expected_size: u64,
    ) -> Result<Vec<u8>, AcquisitionError> {
        if expected_size > MAX_FILE_BYTES {
            return Err(AcquisitionError::FileTooLarge);
        }
        if let Some(bytes) = self.fetched_blobs.get(expected_sha) {
            if u64::try_from(bytes.len()).ok() != Some(expected_size) {
                return Err(AcquisitionError::BlobSizeMismatch);
            }
            return Ok(bytes.clone());
        }
        require_git_id(expected_sha)?;
        let response: BlobResponse = self.get_json(
            &format!("/git/blobs/{expected_sha}"),
            MAX_BLOB_API_BODY_BYTES,
        )?;
        if response.sha != expected_sha
            || response.size != expected_size
            || response.encoding != "base64"
        {
            return Err(AcquisitionError::BlobIdentityMismatch);
        }
        if response
            .content
            .chars()
            .any(|character| character.is_ascii_whitespace() && character != '\n')
        {
            return Err(AcquisitionError::InvalidBlobEncoding);
        }
        let encoded: String = response
            .content
            .chars()
            .filter(|character| *character != '\n')
            .collect();
        let bytes = BASE64_STANDARD
            .decode(encoded.as_bytes())
            .map_err(|_| AcquisitionError::InvalidBlobEncoding)?;
        if u64::try_from(bytes.len()).ok() != Some(expected_size) {
            return Err(AcquisitionError::BlobSizeMismatch);
        }
        if git_object_id("blob", &bytes) != expected_sha {
            return Err(AcquisitionError::BlobIdentityMismatch);
        }
        self.blob_objects = self
            .blob_objects
            .checked_add(1)
            .ok_or(AcquisitionError::BlobLimitExceeded)?;
        if self.blob_objects > MAX_ENTRIES {
            return Err(AcquisitionError::BlobLimitExceeded);
        }
        self.fetched_blobs
            .insert(expected_sha.to_owned(), bytes.clone());
        Ok(bytes)
    }

    fn collect_package_tree(
        &mut self,
        tree: &VerifiedTree,
        prefix: &str,
        depth: usize,
        output: &mut Vec<PackageEntry>,
    ) -> Result<(), AcquisitionError> {
        if depth > MAX_PATH_DEPTH {
            return Err(AcquisitionError::InvalidPackagePath);
        }
        for entry in &tree.entries {
            self.package_nodes = self
                .package_nodes
                .checked_add(1)
                .ok_or(AcquisitionError::TooManyEntries)?;
            if self.package_nodes > MAX_ENTRIES {
                return Err(AcquisitionError::TooManyEntries);
            }
            let path = if prefix.is_empty() {
                entry.name.clone()
            } else {
                format!("{prefix}/{}", entry.name)
            };
            normalize_package_path(&path).map_err(|_| AcquisitionError::InvalidPackagePath)?;
            match entry.object_type {
                GitEntryType::Tree => {
                    if entry.mode != 0o040_000 {
                        return Err(AcquisitionError::ModeTypeMismatch);
                    }
                    let child = self.fetch_tree(&entry.sha)?;
                    self.collect_package_tree(&child, &path, depth + 1, output)?;
                }
                GitEntryType::Commit => return Err(AcquisitionError::Submodule),
                GitEntryType::Blob => {
                    let kind = match entry.mode {
                        0o100_644 => EntryKind::File,
                        0o100_755 => return Err(AcquisitionError::Executable),
                        0o120_000 => return Err(AcquisitionError::Symlink),
                        _ => return Err(AcquisitionError::UnsupportedMode),
                    };
                    let size = entry.size.ok_or(AcquisitionError::MissingBlobSize)?;
                    self.account_package_file(size)?;
                    let bytes = self.fetch_blob(&entry.sha, size)?;
                    output.push(PackageEntry::with_metadata(path, entry.mode, kind, bytes));
                }
            }
        }
        Ok(())
    }

    fn account_package_file(&mut self, size: u64) -> Result<(), AcquisitionError> {
        self.package_files = self
            .package_files
            .checked_add(1)
            .ok_or(AcquisitionError::TooManyEntries)?;
        if self.package_files > MAX_ENTRIES {
            return Err(AcquisitionError::TooManyEntries);
        }
        self.total_bytes = self
            .total_bytes
            .checked_add(size)
            .ok_or(AcquisitionError::TotalBytesExceeded)?;
        if self.total_bytes > MAX_TOTAL_BYTES {
            return Err(AcquisitionError::TotalBytesExceeded);
        }
        Ok(())
    }
}

fn acquire_with_api<A: GitHubApi>(
    api: &A,
    request: &GitHubRequest,
) -> Result<ResolvedPackage, AcquisitionError> {
    let repository_base = format!("{API_ORIGIN}/repos/{}/{}", request.owner, request.repo);
    let repository: RepositoryResponse = api.get_json(&repository_base, MAX_API_BODY_BYTES)?;
    let expected_full_name = format!("{}/{}", request.owner, request.repo);
    if repository.full_name != expected_full_name {
        return Err(AcquisitionError::RepositoryIdentityMismatch);
    }
    let requested_ref = request
        .requested_ref
        .clone()
        .unwrap_or(repository.default_branch);
    if !valid_github_ref(&requested_ref) {
        return Err(AcquisitionError::InvalidResolvedRef);
    }

    let mut state = AcquisitionState {
        api,
        repository_base,
        requests: 1,
        tree_objects: 0,
        blob_objects: 0,
        package_nodes: 0,
        package_files: 0,
        total_bytes: 0,
        fetched_trees: BTreeMap::new(),
        fetched_blobs: BTreeMap::new(),
    };
    let commit: CommitResponse =
        state.get_json(&format!("/commits/{requested_ref}"), MAX_API_BODY_BYTES)?;
    require_git_id(&commit.sha)?;
    require_git_id(&commit.commit.tree.sha)?;
    if is_immutable_commit(&requested_ref) && requested_ref != commit.sha {
        return Err(AcquisitionError::CommitMismatch);
    }
    let source = SkillSource::resolve(request.clone(), &commit.sha)
        .map_err(|_| AcquisitionError::CommitMismatch)?;

    let root_tree = state.fetch_tree(&commit.commit.tree.sha)?;
    let package_tree = resolve_package_tree(&mut state, &root_tree, request.package_path())?;
    let mut entries = Vec::new();
    state.collect_package_tree(&package_tree, "", 1, &mut entries)?;

    if !request.package_path().is_empty() {
        let root_license_entries: Vec<&VerifiedTreeEntry> = root_tree
            .entries
            .iter()
            .filter(|entry| matches!(entry.name.as_str(), "LICENSE" | "LICENSE.txt" | "COPYING"))
            .collect();
        for entry in root_license_entries {
            if entry.object_type != GitEntryType::Blob || entry.mode != 0o100_644 {
                return Err(AcquisitionError::UnsupportedMode);
            }
            let size = entry.size.ok_or(AcquisitionError::MissingBlobSize)?;
            state.account_package_file(size)?;
            let bytes = state.fetch_blob(&entry.sha, size)?;
            entries.push(PackageEntry::with_metadata(
                entry.name.clone(),
                entry.mode,
                EntryKind::File,
                bytes,
            ));
        }
    }

    Ok(ResolvedPackage {
        source,
        entries,
        provenance: GitHubProvenance {
            repository_identity: expected_full_name,
            commit_tree: commit.commit.tree.sha,
            package_tree: package_tree.sha,
            verified_tree_objects: state.tree_objects,
            verified_blob_objects: state.blob_objects,
        },
    })
}

fn resolve_package_tree<A: GitHubApi>(
    state: &mut AcquisitionState<'_, A>,
    root: &VerifiedTree,
    package_path: &str,
) -> Result<VerifiedTree, AcquisitionError> {
    let mut current = root.clone();
    if package_path.is_empty() {
        return Ok(current);
    }
    for component in package_path.split('/') {
        let entry = current
            .entries
            .iter()
            .find(|entry| entry.name == component)
            .ok_or(AcquisitionError::PackagePathNotFound)?;
        if entry.object_type != GitEntryType::Tree || entry.mode != 0o040_000 {
            return Err(AcquisitionError::PackagePathNotTree);
        }
        current = state.fetch_tree(&entry.sha)?;
    }
    Ok(current)
}

fn verify_tree_response(
    expected_sha: &str,
    response: TreeResponse,
) -> Result<VerifiedTree, AcquisitionError> {
    if response.sha != expected_sha {
        return Err(AcquisitionError::TreeIdentityMismatch);
    }
    if response.truncated != Some(false) {
        return Err(AcquisitionError::TruncatedTree);
    }
    if response.tree.len() > MAX_TREE_RESPONSE_ENTRIES {
        return Err(AcquisitionError::TreeLimitExceeded);
    }
    let mut names = BTreeSet::new();
    let mut entries = Vec::with_capacity(response.tree.len());
    for entry in response.tree {
        if entry.path.is_empty()
            || entry.path.contains('/')
            || entry.path.contains('\0')
            || entry.path.nfc().collect::<String>() != entry.path
            || entry.path.chars().any(crate::is_unsafe_path_character)
            || !names.insert(entry.path.clone())
        {
            return Err(AcquisitionError::InvalidTreeEntry);
        }
        require_git_id(&entry.sha)?;
        let (mode, raw_mode, object_type) = match (entry.mode.as_str(), entry.object_type.as_str())
        {
            ("040000", "tree") => (0o040_000, "40000", GitEntryType::Tree),
            ("100644", "blob") => (0o100_644, "100644", GitEntryType::Blob),
            ("100755", "blob") => (0o100_755, "100755", GitEntryType::Blob),
            ("120000", "blob") => (0o120_000, "120000", GitEntryType::Blob),
            ("160000", "commit") => (0o160_000, "160000", GitEntryType::Commit),
            _ => return Err(AcquisitionError::ModeTypeMismatch),
        };
        if object_type != GitEntryType::Blob && entry.size.is_some() {
            return Err(AcquisitionError::ModeTypeMismatch);
        }
        entries.push(VerifiedTreeEntry {
            name: entry.path,
            mode,
            raw_mode,
            object_type,
            sha: entry.sha,
            size: entry.size,
        });
    }
    entries.sort_by(git_tree_entry_order);
    if git_tree_object_id(&entries)? != expected_sha {
        return Err(AcquisitionError::TreeIdentityMismatch);
    }
    Ok(VerifiedTree {
        sha: response.sha,
        entries,
    })
}

fn git_tree_entry_order(left: &VerifiedTreeEntry, right: &VerifiedTreeEntry) -> Ordering {
    let mut left_key = left.name.as_bytes().to_vec();
    let mut right_key = right.name.as_bytes().to_vec();
    if left.object_type == GitEntryType::Tree {
        left_key.push(b'/');
    }
    if right.object_type == GitEntryType::Tree {
        right_key.push(b'/');
    }
    left_key.cmp(&right_key)
}

fn git_tree_object_id(entries: &[VerifiedTreeEntry]) -> Result<String, AcquisitionError> {
    let mut body = Vec::new();
    for entry in entries {
        body.extend_from_slice(entry.raw_mode.as_bytes());
        body.push(b' ');
        body.extend_from_slice(entry.name.as_bytes());
        body.push(0);
        let object_id =
            hex::decode(&entry.sha).map_err(|_| AcquisitionError::InvalidGitObjectId)?;
        if object_id.len() != 20 {
            return Err(AcquisitionError::InvalidGitObjectId);
        }
        body.extend_from_slice(&object_id);
    }
    Ok(git_object_id("tree", &body))
}

fn git_object_id(kind: &str, bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(kind.as_bytes());
    hasher.update(b" ");
    hasher.update(bytes.len().to_string().as_bytes());
    hasher.update([0]);
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn require_git_id(value: &str) -> Result<(), AcquisitionError> {
    if is_immutable_commit(value) {
        Ok(())
    } else {
        Err(AcquisitionError::InvalidGitObjectId)
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AcquisitionError {
    #[error("GitHub API endpoint is outside the fixed allowlist")]
    InvalidEndpoint,
    #[error("GitHub acquisition transport failed")]
    Transport,
    #[error("GitHub returned an unexpected status")]
    UnexpectedStatus,
    #[error("GitHub returned an unexpected content type")]
    UnexpectedContentType,
    #[error("GitHub returned an invalid or oversized response")]
    InvalidResponse,
    #[error("GitHub repository identity does not exactly match the request")]
    RepositoryIdentityMismatch,
    #[error("GitHub resolved an invalid ref")]
    InvalidResolvedRef,
    #[error("GitHub resolved a different immutable commit")]
    CommitMismatch,
    #[error("Git object id is not lowercase 40-hex")]
    InvalidGitObjectId,
    #[error("Git tree response was truncated")]
    TruncatedTree,
    #[error("Git tree identity does not match its complete members")]
    TreeIdentityMismatch,
    #[error("Git tree entry is invalid, duplicated, non-NFC, or undisplayable")]
    InvalidTreeEntry,
    #[error("Git tree mode and object type disagree")]
    ModeTypeMismatch,
    #[error("Git package path does not exist at the immutable commit")]
    PackagePathNotFound,
    #[error("Git package path is not a tree")]
    PackagePathNotTree,
    #[error("package path is invalid")]
    InvalidPackagePath,
    #[error("package contains too many entries")]
    TooManyEntries,
    #[error("tree acquisition exceeds its bounded object limit")]
    TreeLimitExceeded,
    #[error("blob acquisition exceeds its bounded object limit")]
    BlobLimitExceeded,
    #[error("GitHub acquisition exceeds its request limit")]
    RequestLimitExceeded,
    #[error("package file exceeds its fixed byte limit")]
    FileTooLarge,
    #[error("package total bytes exceed the fixed limit")]
    TotalBytesExceeded,
    #[error("Git tree omitted a blob size")]
    MissingBlobSize,
    #[error("Git blob identity does not match the tree member")]
    BlobIdentityMismatch,
    #[error("Git blob size does not match the tree member")]
    BlobSizeMismatch,
    #[error("Git blob encoding is invalid")]
    InvalidBlobEncoding,
    #[error("package contains a symlink")]
    Symlink,
    #[error("package contains a submodule")]
    Submodule,
    #[error("package contains an executable file")]
    Executable,
    #[error("package contains an unsupported Git mode")]
    UnsupportedMode,
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use serde::de::DeserializeOwned;
    use serde_json::Value;

    #[derive(Default)]
    pub(crate) struct FakeGitHubApi {
        responses: BTreeMap<String, Value>,
    }

    impl FakeGitHubApi {
        pub(crate) fn insert(&mut self, url: impl Into<String>, value: Value) {
            self.responses.insert(url.into(), value);
        }
    }

    impl GitHubApi for FakeGitHubApi {
        fn get_json<T: DeserializeOwned>(
            &self,
            url: &str,
            _byte_limit: usize,
        ) -> Result<T, AcquisitionError> {
            let value = self
                .responses
                .get(url)
                .cloned()
                .ok_or(AcquisitionError::Transport)?;
            serde_json::from_value(value).map_err(|_| AcquisitionError::InvalidResponse)
        }
    }

    pub(crate) fn acquire_fixture(
        api: &FakeGitHubApi,
        request: &GitHubRequest,
    ) -> Result<ResolvedPackage, AcquisitionError> {
        acquire_with_api(api, request)
    }

    pub(crate) fn blob_id(bytes: &[u8]) -> String {
        git_object_id("blob", bytes)
    }

    pub(crate) fn tree_id(entries: &[(&str, &str, &str, Option<u64>)]) -> String {
        let mut verified = entries
            .iter()
            .map(|(name, mode, sha, size)| {
                let (numeric_mode, raw_mode, object_type) = match *mode {
                    "040000" => (0o040_000, "40000", GitEntryType::Tree),
                    "100644" => (0o100_644, "100644", GitEntryType::Blob),
                    "100755" => (0o100_755, "100755", GitEntryType::Blob),
                    "120000" => (0o120_000, "120000", GitEntryType::Blob),
                    "160000" => (0o160_000, "160000", GitEntryType::Commit),
                    _ => panic!("unsupported fixture mode"),
                };
                VerifiedTreeEntry {
                    name: (*name).to_owned(),
                    mode: numeric_mode,
                    raw_mode,
                    object_type,
                    sha: (*sha).to_owned(),
                    size: *size,
                }
            })
            .collect::<Vec<_>>();
        verified.sort_by(git_tree_entry_order);
        git_tree_object_id(&verified).expect("valid fixture tree")
    }
}
