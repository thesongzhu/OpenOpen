//! Fail-closed ZIP cataloging for bounded `ChatGPT` Deep ZIP imports.
//!
//! The public scanner is a hard child-process supervisor. The child first
//! copies the selected regular file through a no-follow open into an unnamed
//! private snapshot, then catalogs only that snapshot. A catalog is returned
//! only after every member has passed path, resource, structure, and content
//! validation. No API extracts files, invokes a model, or persists import
//! state.

#![forbid(unsafe_code)]

use flate2::{Decompress, FlushDecompress, Status};
use libproc::libproc::pid_rusage::{RUsageInfoV4, pidrusage};
use nix::fcntl::{OFlag, open};
use nix::sys::resource::{Resource, setrlimit};
use nix::sys::stat::Mode;
use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, TryRecvError},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use thiserror::Error;
use unicode_casefold::UnicodeCaseFold;
use unicode_normalization::UnicodeNormalization;
use zip::read::{ArchiveOffset, Config};
use zip::{CompressionMethod, HasZipMetadata, ZipArchive};

/// Immutable competition limits. Callers cannot weaken them.
pub struct FrozenLimits;

impl FrozenLimits {
    pub const MAX_ARCHIVE_BYTES: u64 = 1_073_741_824;
    pub const MAX_ENTRIES: usize = 25_000;
    pub const MAX_ENTRY_EXPANDED_BYTES: u64 = 536_870_912;
    pub const MAX_TOTAL_EXPANDED_BYTES: u64 = 4_294_967_296;
    pub const MAX_COMPRESSION_RATIO: u64 = 100;
    pub const MAX_PATH_BYTES: usize = 512;
    pub const MAX_PATH_DEPTH: usize = 16;
    pub const MAX_WALL_TIME: Duration = Duration::from_secs(600);
    pub const MAX_RSS_BYTES: u64 = 536_870_912;
    pub const MAX_PROTOCOL_BYTES: u64 = 40 * 1_048_576;
}

/// Bounded non-archive member formats admitted by this ChatGPT-only catalog.
/// All binary attachment formats are rejected; a real adapter must still
/// validate the provider schema before import.
pub const SUPPORTED_CHATGPT_MEMBER_EXTENSIONS: &[&str] = &["json", "html"];

const SUPERVISOR_POLL_INTERVAL: Duration = Duration::from_millis(5);
const RSS_DISCOVERY_TIME: Duration = Duration::from_secs(1);
const MAX_MEMBER_PREFIX_BYTES: usize = 1_024;
const MAX_MEMBER_SUFFIX_BYTES: usize = 64;
const ZIP_LOCAL_HEADER_BYTES: usize = 30;
const ZIP_CENTRAL_HEADER_BYTES: usize = 46;
const ZIP_EOCD_BYTES: usize = 22;
const ZIP_DATA_DESCRIPTOR_BYTES: u64 = 12;
const ZIP_DATA_DESCRIPTOR_WITH_SIGNATURE_BYTES: u64 = 16;
const ZIP_DATA_DESCRIPTOR_BUFFER_BYTES: usize = 16;
const NESTED_PREFIX_PROBE_BYTES: usize = (16 * 2_048) + 8;
const NESTED_SUFFIX_PROBE_BYTES: usize = 512;
const NESTED_ZIP_PROBE_BYTES: usize = ZIP_LOCAL_HEADER_BYTES;
const WORKER_READY: &[u8] = b"OPENOPEN_DEEP_ZIP_READY_V1\n";
const WORKER_GO: u8 = b'G';

/// A cancellation handle shared with the owning import session.
#[derive(Clone, Debug, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// The only public cataloging API. It owns child creation, monitoring, kill,
/// wait, pipe draining, and cleanup.
#[derive(Clone, Debug)]
pub struct DeepZipSupervisor {
    worker_executable: PathBuf,
    cancellation: CancellationToken,
}

impl DeepZipSupervisor {
    /// Binds the supervisor to the exact bundled worker executable selected by
    /// the host. Resource limits remain frozen.
    #[must_use]
    pub fn new(worker_executable: impl Into<PathBuf>) -> Self {
        Self {
            worker_executable: worker_executable.into(),
            cancellation: CancellationToken::new(),
        }
    }

    #[must_use]
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    /// Runs the complete open/snapshot/parse/stream operation in a monitored
    /// child. Every failure returns a bounded classification and no catalog.
    ///
    /// # Errors
    ///
    /// Returns [`ScanError`] when source validation, worker supervision, ZIP
    /// validation, a fixed resource boundary, or cancellation fails closed.
    pub fn scan(&self, path: &Path) -> Result<DeepZipCatalog, ScanError> {
        if self.cancellation.is_cancelled() {
            return Err(ScanError::Cancelled);
        }
        if !path.is_absolute() || !self.worker_executable.is_absolute() {
            return Err(ScanError::SupervisorUnavailable);
        }

        let mut command = Command::new(&self.worker_executable);
        command
            .arg("--isolated-scan-v1")
            .arg(path)
            .env_clear()
            .current_dir("/")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        let budget = SupervisorBudget::frozen();
        let mut probe = SystemRssProbe::new();
        supervise_command(&mut command, &self.cancellation, budget, &mut probe)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct CatalogEntry {
    pub path: String,
    pub compressed_bytes: u64,
    pub expanded_bytes: u64,
    pub sha256: [u8; 32],
    pub directory: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct DeepZipCatalog {
    pub archive_bytes: u64,
    pub archive_sha256: [u8; 32],
    pub total_compressed_bytes: u64,
    pub total_expanded_bytes: u64,
    pub entries: Vec<CatalogEntry>,
}

/// Maximum number of bounded import candidates shown beside the permanent D
/// free-description route.
pub const MAX_PREVIEW_CANDIDATES: usize = 3;

const MAX_PREVIEW_IDENTIFIER_BYTES: usize = 64;
const MAX_PREVIEW_TITLE_BYTES: usize = 160;
const MAX_PREVIEW_RATIONALE_BYTES: usize = 512;
const MAX_MARKDOWN_LINE_BYTES: usize = 4_096;

/// One synthetic-safe candidate derived outside this crate from a verified
/// catalog member. The optional private bytes are deliberately never exposed
/// by [`PreviewSession`]; they model bounded derived working data that must be
/// destroyed as soon as the owner selects or cancels.
pub struct CandidateDraft {
    pub id: String,
    pub title: String,
    pub rationale: String,
    pub source_entry_path: String,
    pub source_entry_sha256: [u8; 32],
    pub proposed_markdown_line: String,
    pub private_derived_bytes: Vec<u8>,
}

/// The bounded, display-safe projection of a candidate. The D route is
/// represented separately and never has fixed semantics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CandidateCard {
    pub id: String,
    pub position: u8,
    pub title: String,
    pub rationale: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FreeDescriptionCard {
    pub key: String,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PreviewChoiceSet {
    pub session_id: String,
    pub catalog_digest: String,
    pub cards: Vec<CandidateCard>,
    pub free_description: FreeDescriptionCard,
}

/// The single owner selection accepted by a preview session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreviewSelection {
    Candidate { candidate_id: String },
    FreeDescription { markdown_line: String },
}

/// One exact, editable Markdown line. The digest binds the verified catalog,
/// selection, original proposal, current edit, and revision without retaining
/// any unselected or raw derived candidate data.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MarkdownLineDiff {
    pub session_id: String,
    pub revision: u64,
    pub selection_id: String,
    pub source_binding_digest: String,
    pub proposed_line: String,
    pub edited_line: String,
    pub diff_digest: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ConfirmedMarkdownLine {
    pub session_id: String,
    pub revision: u64,
    pub diff_digest: String,
    pub edited_line: String,
    pub confirmation_digest: String,
}

/// Receipt for an exact in-memory readback comparison. This is not filesystem
/// or real-import proof; it records only that the supplied line exactly
/// matched the already confirmed synthetic-safe line.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MarkdownReadbackReceipt {
    pub session_id: String,
    pub confirmation_digest: String,
    pub readback_digest: String,
    pub receipt_digest: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum PreviewSessionState {
    AwaitingSelection,
    Selected,
    Confirmed,
    ReadBack,
    Cancelled,
}

/// Body-free evidence that the session disposed of every candidate working
/// buffer. Counts are safe to retain; candidate text, paths, and raw bytes are
/// not.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub struct DisposalSummary {
    pub candidates_disposed: usize,
    pub private_bytes_disposed: usize,
    pub complete: bool,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PreviewError {
    #[error("preview catalog is invalid")]
    InvalidCatalog,
    #[error("preview candidate set is invalid")]
    InvalidCandidates,
    #[error("preview candidate is not bound to the catalog")]
    SourceBindingMismatch,
    #[error("preview session is in the wrong state")]
    StateConflict,
    #[error("preview selection is invalid")]
    InvalidSelection,
    #[error("preview digest is stale or invalid")]
    DigestMismatch,
    #[error("preview readback does not match the confirmed line")]
    ReadbackMismatch,
}

struct PrivateBytes(Vec<u8>);

impl PrivateBytes {
    fn zeroize(&mut self) {
        self.0.fill(0);
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn text(&self) -> Result<&str, PreviewError> {
        std::str::from_utf8(&self.0).map_err(|_| PreviewError::InvalidCandidates)
    }
}

impl Drop for PrivateBytes {
    fn drop(&mut self) {
        self.zeroize();
    }
}

struct PrivateCandidate {
    id: PrivateBytes,
    title: PrivateBytes,
    rationale: PrivateBytes,
    source_entry_path: PrivateBytes,
    source_entry_sha256: [u8; 32],
    proposed_markdown_line: PrivateBytes,
    private_derived_bytes: PrivateBytes,
}

impl PrivateCandidate {
    fn zeroize(&mut self) {
        self.id.zeroize();
        self.title.zeroize();
        self.rationale.zeroize();
        self.source_entry_path.zeroize();
        self.source_entry_sha256.fill(0);
        self.proposed_markdown_line.zeroize();
        self.private_derived_bytes.zeroize();
    }
}

impl Drop for PrivateCandidate {
    fn drop(&mut self) {
        self.zeroize();
    }
}

struct SelectedMarkdown {
    selection_id: String,
    source_binding_digest: String,
    proposed_line: String,
    edited_line: String,
    revision: u64,
    diff_digest: String,
}

/// Deterministic, effect-free preview state layered on one verified catalog.
/// It never reads archive bytes, writes Markdown, invokes a model, or records a
/// real import. Every transition is local and digest-bound.
pub struct PreviewSession {
    session_id: String,
    catalog_digest: String,
    candidates: Vec<PrivateCandidate>,
    state: PreviewSessionState,
    selected: Option<SelectedMarkdown>,
    confirmed: Option<ConfirmedMarkdownLine>,
    receipt: Option<MarkdownReadbackReceipt>,
    disposal: DisposalSummary,
}

impl PreviewSession {
    /// Creates one bounded preview from candidates that exactly reference
    /// regular members in the supplied catalog.
    ///
    /// # Errors
    ///
    /// Rejects invalid catalogs, invalid candidate fields, duplicate IDs, or
    /// candidate source bindings that do not match a regular catalog member.
    pub fn new(
        catalog: &DeepZipCatalog,
        candidates: Vec<CandidateDraft>,
    ) -> Result<Self, PreviewError> {
        let catalog_digest = catalog_digest(catalog)?;
        if candidates.is_empty() || candidates.len() > MAX_PREVIEW_CANDIDATES {
            return Err(PreviewError::InvalidCandidates);
        }
        let mut ids = HashSet::with_capacity(candidates.len());
        let mut private_candidates = Vec::with_capacity(candidates.len());
        let mut session_manifest = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            if !valid_preview_identifier(&candidate.id)
                || !valid_single_line(&candidate.title, MAX_PREVIEW_TITLE_BYTES)
                || !valid_single_line(&candidate.rationale, MAX_PREVIEW_RATIONALE_BYTES)
                || !valid_markdown_line(&candidate.proposed_markdown_line)
                || !ids.insert(candidate.id.clone())
            {
                return Err(PreviewError::InvalidCandidates);
            }
            let entry = catalog
                .entries
                .iter()
                .find(|entry| entry.path == candidate.source_entry_path && !entry.directory)
                .ok_or(PreviewError::SourceBindingMismatch)?;
            if entry.sha256 != candidate.source_entry_sha256 {
                return Err(PreviewError::SourceBindingMismatch);
            }
            let source_binding_digest = digest_json(&serde_json::json!({
                "path": &candidate.source_entry_path,
                "sha256": hex_digest(&candidate.source_entry_sha256),
            }))?;
            session_manifest.push(serde_json::json!({
                "id": &candidate.id,
                "sourceBindingDigest": source_binding_digest,
                "proposedLineDigest": digest_bytes(candidate.proposed_markdown_line.as_bytes()),
            }));
            private_candidates.push(PrivateCandidate {
                id: PrivateBytes(candidate.id.into_bytes()),
                title: PrivateBytes(candidate.title.into_bytes()),
                rationale: PrivateBytes(candidate.rationale.into_bytes()),
                source_entry_path: PrivateBytes(candidate.source_entry_path.into_bytes()),
                source_entry_sha256: candidate.source_entry_sha256,
                proposed_markdown_line: PrivateBytes(candidate.proposed_markdown_line.into_bytes()),
                private_derived_bytes: PrivateBytes(candidate.private_derived_bytes),
            });
        }
        let session_id = format!(
            "deep-zip-preview-{}",
            &digest_json(&serde_json::json!({
                "catalogDigest": catalog_digest,
                "candidates": session_manifest,
            }))?[..24]
        );
        Ok(Self {
            session_id,
            catalog_digest,
            candidates: private_candidates,
            state: PreviewSessionState::AwaitingSelection,
            selected: None,
            confirmed: None,
            receipt: None,
            disposal: DisposalSummary::default(),
        })
    }

    /// Returns the bounded public projection while selection is still open.
    ///
    /// # Errors
    ///
    /// Returns [`PreviewError::StateConflict`] after selection or cancellation,
    /// or a validation error if private candidate storage is malformed.
    pub fn choice_set(&self) -> Result<PreviewChoiceSet, PreviewError> {
        if self.state != PreviewSessionState::AwaitingSelection {
            return Err(PreviewError::StateConflict);
        }
        let cards = self
            .candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| {
                Ok(CandidateCard {
                    id: candidate.id.text()?.to_owned(),
                    position: u8::try_from(index + 1)
                        .map_err(|_| PreviewError::InvalidCandidates)?,
                    title: candidate.title.text()?.to_owned(),
                    rationale: candidate.rationale.text()?.to_owned(),
                })
            })
            .collect::<Result<Vec<_>, PreviewError>>()?;
        Ok(PreviewChoiceSet {
            session_id: self.session_id.clone(),
            catalog_digest: self.catalog_digest.clone(),
            cards,
            free_description: FreeDescriptionCard {
                key: "D".to_owned(),
                title: "Something else".to_owned(),
            },
        })
    }

    /// Accepts exactly one candidate or D line and immediately destroys all
    /// candidate working buffers. A second selection is always rejected.
    ///
    /// # Errors
    ///
    /// Rejects an invalid or repeated selection and malformed private state.
    pub fn select(
        &mut self,
        selection: PreviewSelection,
    ) -> Result<MarkdownLineDiff, PreviewError> {
        if self.state != PreviewSessionState::AwaitingSelection {
            return Err(PreviewError::StateConflict);
        }
        let (selection_id, source_binding_digest, proposed_line) = match selection {
            PreviewSelection::Candidate { candidate_id } => {
                let candidate = self
                    .candidates
                    .iter()
                    .find(|candidate| {
                        candidate
                            .id
                            .text()
                            .is_ok_and(|stored_id| stored_id == candidate_id)
                    })
                    .ok_or(PreviewError::InvalidSelection)?;
                let path = candidate.source_entry_path.text()?;
                let source_binding_digest = digest_json(&serde_json::json!({
                    "path": path,
                    "sha256": hex_digest(&candidate.source_entry_sha256),
                }))?;
                (
                    candidate_id,
                    source_binding_digest,
                    candidate.proposed_markdown_line.text()?.to_owned(),
                )
            }
            PreviewSelection::FreeDescription { markdown_line } => {
                if !valid_markdown_line(&markdown_line) {
                    return Err(PreviewError::InvalidSelection);
                }
                (
                    "D".to_owned(),
                    digest_json(&serde_json::json!({
                        "catalogDigest": self.catalog_digest,
                        "selection": "D",
                    }))?,
                    markdown_line,
                )
            }
        };
        let private_bytes_disposed = self
            .candidates
            .iter()
            .map(|candidate| candidate.private_derived_bytes.len())
            .sum();
        let candidates_disposed = self.candidates.len();
        self.candidates
            .iter_mut()
            .for_each(PrivateCandidate::zeroize);
        self.candidates.clear();
        self.disposal = DisposalSummary {
            candidates_disposed,
            private_bytes_disposed,
            complete: true,
        };
        let revision = 1;
        let diff_digest = markdown_diff_digest(
            &self.session_id,
            revision,
            &selection_id,
            &source_binding_digest,
            &proposed_line,
            &proposed_line,
        )?;
        self.selected = Some(SelectedMarkdown {
            selection_id,
            source_binding_digest,
            proposed_line: proposed_line.clone(),
            edited_line: proposed_line,
            revision,
            diff_digest,
        });
        self.state = PreviewSessionState::Selected;
        self.current_diff()
    }

    /// Replaces the selected line when the caller presents the current digest.
    ///
    /// # Errors
    ///
    /// Fails for an invalid state or line, or for a stale digest.
    pub fn edit_markdown_line(
        &mut self,
        expected_diff_digest: &str,
        edited_line: String,
    ) -> Result<MarkdownLineDiff, PreviewError> {
        if self.state != PreviewSessionState::Selected || !valid_markdown_line(&edited_line) {
            return Err(PreviewError::StateConflict);
        }
        let session_id = self.session_id.clone();
        let selected = self.selected.as_mut().ok_or(PreviewError::StateConflict)?;
        if selected.diff_digest != expected_diff_digest {
            return Err(PreviewError::DigestMismatch);
        }
        if selected.edited_line == edited_line {
            return self.current_diff();
        }
        selected.revision = selected
            .revision
            .checked_add(1)
            .ok_or(PreviewError::StateConflict)?;
        selected.edited_line = edited_line;
        selected.diff_digest = markdown_diff_digest(
            &session_id,
            selected.revision,
            &selected.selection_id,
            &selected.source_binding_digest,
            &selected.proposed_line,
            &selected.edited_line,
        )?;
        self.current_diff()
    }

    /// Confirms the exact current diff and returns an idempotent confirmation.
    ///
    /// # Errors
    ///
    /// Fails unless a selection exists and the supplied digest is current.
    pub fn confirm(
        &mut self,
        expected_diff_digest: &str,
    ) -> Result<ConfirmedMarkdownLine, PreviewError> {
        if self.state == PreviewSessionState::Confirmed
            && self
                .confirmed
                .as_ref()
                .is_some_and(|value| value.diff_digest == expected_diff_digest)
        {
            return self.confirmed.clone().ok_or(PreviewError::StateConflict);
        }
        if self.state != PreviewSessionState::Selected {
            return Err(PreviewError::StateConflict);
        }
        let selected = self.selected.as_ref().ok_or(PreviewError::StateConflict)?;
        if selected.diff_digest != expected_diff_digest {
            return Err(PreviewError::DigestMismatch);
        }
        let confirmation_digest = digest_json(&serde_json::json!({
            "sessionId": self.session_id,
            "revision": selected.revision,
            "diffDigest": selected.diff_digest,
            "editedLineDigest": digest_bytes(selected.edited_line.as_bytes()),
        }))?;
        let confirmed = ConfirmedMarkdownLine {
            session_id: self.session_id.clone(),
            revision: selected.revision,
            diff_digest: selected.diff_digest.clone(),
            edited_line: selected.edited_line.clone(),
            confirmation_digest,
        };
        self.confirmed = Some(confirmed.clone());
        self.state = PreviewSessionState::Confirmed;
        Ok(confirmed)
    }

    /// Verifies an exact readback of the confirmed line without filesystem I/O.
    ///
    /// # Errors
    ///
    /// Fails for a stale confirmation, wrong state, or mismatched readback.
    pub fn verify_readback(
        &mut self,
        confirmation_digest: &str,
        readback_line: &str,
    ) -> Result<MarkdownReadbackReceipt, PreviewError> {
        if self.state == PreviewSessionState::ReadBack {
            return self
                .receipt
                .as_ref()
                .filter(|receipt| receipt.confirmation_digest == confirmation_digest)
                .cloned()
                .ok_or(PreviewError::DigestMismatch);
        }
        if self.state != PreviewSessionState::Confirmed {
            return Err(PreviewError::StateConflict);
        }
        let confirmed = self.confirmed.as_ref().ok_or(PreviewError::StateConflict)?;
        if confirmed.confirmation_digest != confirmation_digest {
            return Err(PreviewError::DigestMismatch);
        }
        if confirmed.edited_line != readback_line {
            return Err(PreviewError::ReadbackMismatch);
        }
        let readback_digest = digest_bytes(readback_line.as_bytes());
        let receipt_digest = digest_json(&serde_json::json!({
            "sessionId": self.session_id,
            "confirmationDigest": confirmation_digest,
            "readbackDigest": readback_digest,
        }))?;
        let receipt = MarkdownReadbackReceipt {
            session_id: self.session_id.clone(),
            confirmation_digest: confirmation_digest.to_owned(),
            readback_digest,
            receipt_digest,
        };
        self.receipt = Some(receipt.clone());
        self.state = PreviewSessionState::ReadBack;
        Ok(receipt)
    }

    /// Cancels before selection and destroys all private candidate buffers.
    ///
    /// # Errors
    ///
    /// Returns [`PreviewError::StateConflict`] once selection has started.
    pub fn cancel(&mut self) -> Result<DisposalSummary, PreviewError> {
        if self.state != PreviewSessionState::AwaitingSelection {
            return Err(PreviewError::StateConflict);
        }
        let private_bytes_disposed = self
            .candidates
            .iter()
            .map(|candidate| candidate.private_derived_bytes.len())
            .sum();
        let candidates_disposed = self.candidates.len();
        self.candidates
            .iter_mut()
            .for_each(PrivateCandidate::zeroize);
        self.candidates.clear();
        self.disposal = DisposalSummary {
            candidates_disposed,
            private_bytes_disposed,
            complete: true,
        };
        self.state = PreviewSessionState::Cancelled;
        Ok(self.disposal)
    }

    #[must_use]
    pub fn state(&self) -> PreviewSessionState {
        self.state
    }

    #[must_use]
    pub fn disposal_summary(&self) -> DisposalSummary {
        self.disposal
    }

    fn current_diff(&self) -> Result<MarkdownLineDiff, PreviewError> {
        let selected = self.selected.as_ref().ok_or(PreviewError::StateConflict)?;
        Ok(MarkdownLineDiff {
            session_id: self.session_id.clone(),
            revision: selected.revision,
            selection_id: selected.selection_id.clone(),
            source_binding_digest: selected.source_binding_digest.clone(),
            proposed_line: selected.proposed_line.clone(),
            edited_line: selected.edited_line.clone(),
            diff_digest: selected.diff_digest.clone(),
        })
    }
}

fn catalog_digest(catalog: &DeepZipCatalog) -> Result<String, PreviewError> {
    if catalog.entries.is_empty()
        || catalog.entries.len() > FrozenLimits::MAX_ENTRIES
        || catalog.archive_bytes > FrozenLimits::MAX_ARCHIVE_BYTES
        || catalog.total_expanded_bytes > FrozenLimits::MAX_TOTAL_EXPANDED_BYTES
        || catalog
            .entries
            .windows(2)
            .any(|pair| pair[0].path >= pair[1].path)
    {
        return Err(PreviewError::InvalidCatalog);
    }
    digest_json(&serde_json::json!({
        "archiveBytes": catalog.archive_bytes,
        "archiveSha256": hex_digest(&catalog.archive_sha256),
        "totalCompressedBytes": catalog.total_compressed_bytes,
        "totalExpandedBytes": catalog.total_expanded_bytes,
        "entries": catalog.entries.iter().map(|entry| serde_json::json!({
            "path": entry.path,
            "compressedBytes": entry.compressed_bytes,
            "expandedBytes": entry.expanded_bytes,
            "sha256": hex_digest(&entry.sha256),
            "directory": entry.directory,
        })).collect::<Vec<_>>(),
    }))
}

fn markdown_diff_digest(
    session_id: &str,
    revision: u64,
    selection_id: &str,
    source_binding_digest: &str,
    proposed_line: &str,
    edited_line: &str,
) -> Result<String, PreviewError> {
    digest_json(&serde_json::json!({
        "sessionId": session_id,
        "revision": revision,
        "selectionId": selection_id,
        "sourceBindingDigest": source_binding_digest,
        "proposedLine": proposed_line,
        "editedLine": edited_line,
    }))
}

fn digest_json(value: &serde_json::Value) -> Result<String, PreviewError> {
    serde_json::to_vec(value)
        .map(|bytes| digest_bytes(&bytes))
        .map_err(|_| PreviewError::InvalidCandidates)
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn hex_digest(bytes: &[u8; 32]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            write!(hex, "{byte:02x}").expect("writing to a String cannot fail");
            hex
        })
}

fn valid_preview_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PREVIEW_IDENTIFIER_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn valid_single_line(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.len() <= max_bytes
        && !value
            .chars()
            .any(|character| character.is_control() || matches!(character, '\n' | '\r'))
}

fn valid_markdown_line(value: &str) -> bool {
    valid_single_line(value, MAX_MARKDOWN_LINE_BYTES)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum PathViolation {
    Empty,
    TooLong,
    InvalidUtf8,
    Absolute,
    Traversal,
    Backslash,
    Control,
    TooDeep,
    Duplicate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum EntryKindViolation {
    Symlink,
    Special,
    ContradictoryDirectory,
}

/// Errors are bounded classifications and intentionally omit archive paths,
/// member names, child output, and underlying library messages.
#[derive(Clone, Debug, Error, Eq, PartialEq, Deserialize, Serialize)]
pub enum ScanError {
    #[error("worker supervisor is unavailable")]
    SupervisorUnavailable,
    #[error("worker process could not be started")]
    WorkerSpawn,
    #[error("worker process failed")]
    WorkerCrashed,
    #[error("worker response is invalid")]
    WorkerProtocol,
    #[error("worker response exceeds the fixed byte limit")]
    WorkerOutputExceeded,
    #[error("archive metadata is unavailable")]
    ArchiveMetadata,
    #[error("archive source is not a regular no-follow file")]
    ArchiveNotRegular,
    #[error("archive exceeds the fixed byte limit")]
    ArchiveTooLarge,
    #[error("archive could not be read")]
    ArchiveRead,
    #[error("private archive snapshot is unavailable")]
    SnapshotUnavailable,
    #[error("archive structure is invalid or unsupported")]
    InvalidArchive,
    #[error("archive does not match the bounded ChatGPT member layout")]
    UnsupportedArchiveLayout,
    #[error("archive contains too many entries")]
    TooManyEntries,
    #[error("entry {index} has an invalid path: {violation:?}")]
    InvalidPath {
        index: usize,
        violation: PathViolation,
    },
    #[error("entry {index} has an unsupported kind: {violation:?}")]
    UnsupportedEntryKind {
        index: usize,
        violation: EntryKindViolation,
    },
    #[error("entry {index} is a nested archive")]
    NestedArchive { index: usize },
    #[error("entry {index} has an unsupported ChatGPT member format")]
    UnsupportedMember { index: usize },
    #[error("entry {index} content does not match its allowed format")]
    UnsupportedMemberContent { index: usize },
    #[error("entry {index} exceeds the fixed expanded-byte limit")]
    EntryTooLarge { index: usize },
    #[error("archive exceeds the fixed total expanded-byte limit")]
    TotalExpandedTooLarge,
    #[error("entry {index} exceeds the fixed compression-ratio limit")]
    EntryCompressionRatio { index: usize },
    #[error("archive exceeds the fixed aggregate compression-ratio limit")]
    AggregateCompressionRatio,
    #[error("entry {index} expanded size does not match its authenticated stream")]
    ExpandedSizeMismatch { index: usize },
    #[error("scan was cancelled")]
    Cancelled,
    #[error("scan exceeded the fixed wall-clock limit")]
    WallClockExceeded,
    #[error("resident-memory measurement is unavailable")]
    RssUnavailable,
    #[error("scan exceeded the fixed resident-memory limit")]
    RssExceeded,
}

#[derive(Debug, Deserialize, Serialize)]
enum WorkerResponse {
    Catalog(DeepZipCatalog),
    Error(ScanError),
}

#[derive(Clone, Copy)]
struct SupervisorBudget {
    wall_time: Duration,
    rss_bytes: u64,
    protocol_bytes: u64,
    poll_interval: Duration,
}

impl SupervisorBudget {
    const fn frozen() -> Self {
        Self {
            wall_time: FrozenLimits::MAX_WALL_TIME,
            rss_bytes: FrozenLimits::MAX_RSS_BYTES,
            protocol_bytes: FrozenLimits::MAX_PROTOCOL_BYTES,
            poll_interval: SUPERVISOR_POLL_INTERVAL,
        }
    }
}

trait ChildRssProbe {
    fn rss_bytes(&mut self, pid: u32) -> Option<u64>;
}

struct SystemRssProbe;

impl SystemRssProbe {
    fn new() -> Self {
        Self
    }
}

impl ChildRssProbe for SystemRssProbe {
    fn rss_bytes(&mut self, pid: u32) -> Option<u64> {
        process_memory_high_water(pid)
    }
}

fn supervise_command<P: ChildRssProbe>(
    command: &mut Command,
    cancellation: &CancellationToken,
    budget: SupervisorBudget,
    probe: &mut P,
) -> Result<DeepZipCatalog, ScanError> {
    let started = Instant::now();
    let mut child = command.spawn().map_err(|_| ScanError::WorkerSpawn)?;
    let mut child_stdin = child.stdin.take().ok_or_else(|| {
        terminate_and_wait(&mut child);
        ScanError::WorkerSpawn
    })?;
    let stdout = child.stdout.take().ok_or_else(|| {
        terminate_and_wait(&mut child);
        ScanError::WorkerSpawn
    })?;
    let overflow = Arc::new(AtomicBool::new(false));
    let (ready_sender, ready_receiver) = mpsc::channel();
    let reader = spawn_protocol_reader(
        stdout,
        budget.protocol_bytes,
        overflow.clone(),
        ready_sender,
    );
    let mut worker_ready = false;
    let mut rss_observed = false;
    let mut worker_released = false;

    let status = loop {
        if cancellation.is_cancelled() {
            return fail_supervision(child, reader, ScanError::Cancelled);
        }
        if started.elapsed() > budget.wall_time {
            return fail_supervision(child, reader, ScanError::WallClockExceeded);
        }
        if overflow.load(Ordering::Acquire) {
            return fail_supervision(child, reader, ScanError::WorkerOutputExceeded);
        }
        match ready_receiver.try_recv() {
            Ok(Ok(())) => worker_ready = true,
            Ok(Err(())) => return fail_supervision(child, reader, ScanError::WorkerProtocol),
            Err(TryRecvError::Disconnected) if !worker_ready => {
                return fail_supervision(child, reader, ScanError::WorkerProtocol);
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => {}
        }

        let Ok(child_status) = child.try_wait() else {
            return fail_supervision(child, reader, ScanError::WorkerCrashed);
        };
        if let Some(status) = child_status {
            break status;
        }
        match probe.rss_bytes(child.id()) {
            Some(rss_bytes) => {
                rss_observed = true;
                if rss_bytes > budget.rss_bytes {
                    return fail_supervision(child, reader, ScanError::RssExceeded);
                }
            }
            None if worker_released => match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    return fail_supervision(child, reader, ScanError::RssUnavailable);
                }
                Err(_) => return fail_supervision(child, reader, ScanError::WorkerCrashed),
            },
            None if worker_ready && started.elapsed() > RSS_DISCOVERY_TIME => {
                return fail_supervision(child, reader, ScanError::RssUnavailable);
            }
            None => {}
        }
        if worker_ready && rss_observed && !worker_released {
            if child_stdin.write_all(&[WORKER_GO]).is_err() || child_stdin.flush().is_err() {
                return fail_supervision(child, reader, ScanError::WorkerProtocol);
            }
            worker_released = true;
        }
        thread::sleep(budget.poll_interval);
    };

    let result = finish_supervision(status, reader, budget.protocol_bytes);
    if cancellation.is_cancelled() {
        return Err(ScanError::Cancelled);
    }
    if started.elapsed() > budget.wall_time {
        return Err(ScanError::WallClockExceeded);
    }
    result
}

fn spawn_protocol_reader(
    mut stdout: ChildStdout,
    protocol_bytes: u64,
    overflow: Arc<AtomicBool>,
    ready: mpsc::Sender<Result<(), ()>>,
) -> JoinHandle<io::Result<Vec<u8>>> {
    thread::spawn(move || {
        let mut marker = [0_u8; WORKER_READY.len()];
        if stdout.read_exact(&mut marker).is_err() || marker != WORKER_READY {
            let _ = ready.send(Err(()));
            return Err(io::Error::other("invalid worker handshake"));
        }
        let _ = ready.send(Ok(()));
        let mut bytes = Vec::new();
        stdout
            .by_ref()
            .take(protocol_bytes.saturating_add(1))
            .read_to_end(&mut bytes)?;
        if u64::try_from(bytes.len()).is_ok_and(|len| len > protocol_bytes) {
            overflow.store(true, Ordering::Release);
        }
        Ok(bytes)
    })
}

fn fail_supervision(
    mut child: Child,
    reader: JoinHandle<io::Result<Vec<u8>>>,
    error: ScanError,
) -> Result<DeepZipCatalog, ScanError> {
    terminate_and_wait(&mut child);
    let _ = reader.join();
    Err(error)
}

fn terminate_and_wait(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn finish_supervision(
    status: ExitStatus,
    reader: JoinHandle<io::Result<Vec<u8>>>,
    protocol_bytes: u64,
) -> Result<DeepZipCatalog, ScanError> {
    let bytes = reader
        .join()
        .map_err(|_| ScanError::WorkerProtocol)?
        .map_err(|_| ScanError::WorkerProtocol)?;
    if u64::try_from(bytes.len()).map_or(true, |len| len > protocol_bytes) {
        return Err(ScanError::WorkerOutputExceeded);
    }
    if !status.success() {
        return Err(ScanError::WorkerCrashed);
    }
    match serde_json::from_slice(&bytes).map_err(|_| ScanError::WorkerProtocol)? {
        WorkerResponse::Catalog(catalog) => Ok(catalog),
        WorkerResponse::Error(error) => Err(error),
    }
}

/// Entrypoint used only by the crate's bundled worker executable. The public
/// catalog API remains [`DeepZipSupervisor::scan`].
#[doc(hidden)]
pub fn isolated_worker_entrypoint() -> i32 {
    let mut arguments = std::env::args_os();
    let _program = arguments.next();
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new("--isolated-scan-v1")) {
        return 64;
    }
    let Some(path) = arguments.next().map(PathBuf::from) else {
        return 64;
    };
    if arguments.next().is_some() {
        return 64;
    }

    if install_worker_limits().is_err() || worker_handshake().is_err() {
        return 74;
    }
    let result = {
        let monitor = SystemMonitor::new(CancellationToken::new());
        capture_snapshot(&path, &monitor)
            .and_then(|snapshot| scan_immutable_snapshot(snapshot, &monitor))
    };
    let response = match result {
        Ok(catalog) => WorkerResponse::Catalog(catalog),
        Err(error) => WorkerResponse::Error(error),
    };
    let stdout = io::stdout();
    let mut output = stdout.lock();
    if serde_json::to_writer(&mut output, &response).is_err() || output.flush().is_err() {
        74
    } else {
        0
    }
}

fn worker_handshake() -> io::Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    output.write_all(WORKER_READY)?;
    output.flush()?;
    drop(output);

    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut command = [0_u8; 1];
    input.read_exact(&mut command)?;
    if command[0] != WORKER_GO {
        return Err(io::Error::other("invalid supervisor command"));
    }
    Ok(())
}

fn install_worker_limits() -> Result<(), ScanError> {
    let cpu_limit = FrozenLimits::MAX_WALL_TIME.as_secs() as nix::libc::rlim_t;
    setrlimit(Resource::RLIMIT_CPU, cpu_limit, cpu_limit)
        .map_err(|_| ScanError::SupervisorUnavailable)
}

trait ResourceMonitor {
    fn cancelled(&self) -> bool;
    fn elapsed(&self) -> Duration;
    fn rss_bytes(&self) -> Option<u64>;
}

struct SystemMonitor {
    started: Instant,
    cancellation: CancellationToken,
}

impl SystemMonitor {
    fn new(cancellation: CancellationToken) -> Self {
        Self {
            started: Instant::now(),
            cancellation,
        }
    }
}

impl ResourceMonitor for SystemMonitor {
    fn cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    fn rss_bytes(&self) -> Option<u64> {
        process_memory_high_water(std::process::id())
    }
}

fn process_memory_high_water(pid: u32) -> Option<u64> {
    let pid = i32::try_from(pid).ok()?;
    let usage = pidrusage::<RUsageInfoV4>(pid).ok()?;
    Some(
        usage
            .ri_resident_size
            .max(usage.ri_phys_footprint)
            .max(usage.ri_lifetime_max_phys_footprint)
            .max(usage.ri_interval_max_phys_footprint),
    )
}

struct ImmutableSnapshot {
    file: File,
    bytes: u64,
    sha256: [u8; 32],
}

fn capture_snapshot<M: ResourceMonitor + ?Sized>(
    path: &Path,
    monitor: &M,
) -> Result<ImmutableSnapshot, ScanError> {
    check_budget(monitor)?;
    if !path.is_absolute() {
        return Err(ScanError::ArchiveRead);
    }
    let descriptor = open(
        path,
        OFlag::O_RDONLY | OFlag::O_CLOEXEC | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK,
        Mode::empty(),
    )
    .map_err(|_| ScanError::ArchiveRead)?;
    let mut source = File::from(descriptor);
    let source_metadata = source.metadata().map_err(|_| ScanError::ArchiveMetadata)?;
    if !source_metadata.file_type().is_file() {
        return Err(ScanError::ArchiveNotRegular);
    }
    if source_metadata.len() > FrozenLimits::MAX_ARCHIVE_BYTES {
        return Err(ScanError::ArchiveTooLarge);
    }

    let mut snapshot = tempfile::tempfile().map_err(|_| ScanError::SnapshotUnavailable)?;
    let snapshot_metadata = snapshot
        .metadata()
        .map_err(|_| ScanError::SnapshotUnavailable)?;
    if !snapshot_metadata.file_type().is_file() || snapshot_metadata.nlink() != 0 {
        return Err(ScanError::SnapshotUnavailable);
    }

    let mut digest = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1_024].into_boxed_slice();
    loop {
        check_budget(monitor)?;
        let read = source
            .read(&mut buffer)
            .map_err(|_| ScanError::ArchiveRead)?;
        if read == 0 {
            break;
        }
        bytes = bytes
            .checked_add(u64::try_from(read).map_err(|_| ScanError::ArchiveTooLarge)?)
            .ok_or(ScanError::ArchiveTooLarge)?;
        if bytes > FrozenLimits::MAX_ARCHIVE_BYTES {
            return Err(ScanError::ArchiveTooLarge);
        }
        snapshot
            .write_all(&buffer[..read])
            .map_err(|_| ScanError::SnapshotUnavailable)?;
        digest.update(&buffer[..read]);
    }
    check_budget(monitor)?;
    if snapshot
        .metadata()
        .map_err(|_| ScanError::SnapshotUnavailable)?
        .len()
        != bytes
    {
        return Err(ScanError::SnapshotUnavailable);
    }
    snapshot
        .seek(SeekFrom::Start(0))
        .map_err(|_| ScanError::SnapshotUnavailable)?;
    Ok(ImmutableSnapshot {
        file: snapshot,
        bytes,
        sha256: digest.finalize().into(),
    })
}

#[derive(Debug)]
struct AuthenticatedEntry {
    index: usize,
    raw_name: Vec<u8>,
    path: String,
    central_header_start: u64,
    creator_system: u8,
    creator_version: u8,
    flags: u16,
    compression_method: u16,
    modified_time: u16,
    modified_date: u16,
    crc32: u32,
    compressed_bytes: u64,
    expanded_bytes: u64,
    local_header_start: u64,
    data_start: u64,
    data_end: u64,
    local_record_end: u64,
    external_attributes: u32,
    directory: bool,
}

#[derive(Debug)]
struct AuthenticatedArchive {
    entries: Vec<AuthenticatedEntry>,
    central_directory_start: u64,
    total_physical_compressed_bytes: u64,
    total_expanded_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CanonicalEntryKind {
    File,
    Directory,
}

#[derive(Debug, Default)]
struct PathIdentityNode {
    kind: Option<CanonicalEntryKind>,
    children: HashMap<String, Self>,
}

#[derive(Debug, Default)]
struct PathIdentityTrie {
    root: PathIdentityNode,
}

impl PathIdentityTrie {
    fn insert(&mut self, path: &str, directory: bool) -> bool {
        let components: Vec<String> = path
            .trim_end_matches('/')
            .split('/')
            .map(canonical_component_key)
            .collect();
        let mut node = &mut self.root;
        for component in components {
            if node.kind == Some(CanonicalEntryKind::File) {
                return false;
            }
            node = node.children.entry(component).or_default();
        }

        let kind = if directory {
            CanonicalEntryKind::Directory
        } else {
            CanonicalEntryKind::File
        };
        if node.kind.is_some() || (kind == CanonicalEntryKind::File && !node.children.is_empty()) {
            return false;
        }
        node.kind = Some(kind);
        true
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExtraFieldLocation {
    Local,
    Central,
}

#[allow(clippy::too_many_lines)] // One ordered pass over the hostile central-directory grammar.
fn authenticate_zip_layout<M: ResourceMonitor + ?Sized>(
    file: &mut File,
    archive_bytes: u64,
    monitor: &M,
) -> Result<AuthenticatedArchive, ScanError> {
    if archive_bytes < ZIP_EOCD_BYTES as u64 {
        return Err(ScanError::InvalidArchive);
    }

    let eocd_start = archive_bytes
        .checked_sub(ZIP_EOCD_BYTES as u64)
        .ok_or(ScanError::InvalidArchive)?;
    let mut eocd = [0_u8; ZIP_EOCD_BYTES];
    read_exact_at(file, eocd_start, &mut eocd, monitor)?;
    if &eocd[..4] != b"PK\x05\x06"
        || le_u16(&eocd, 4) != 0
        || le_u16(&eocd, 6) != 0
        || le_u16(&eocd, 8) != le_u16(&eocd, 10)
        || le_u16(&eocd, 20) != 0
    {
        return Err(ScanError::InvalidArchive);
    }

    let entry_count = usize::from(le_u16(&eocd, 10));
    if entry_count == usize::from(u16::MAX)
        || le_u32(&eocd, 12) == u32::MAX
        || le_u32(&eocd, 16) == u32::MAX
    {
        return Err(ScanError::InvalidArchive);
    }
    if entry_count > FrozenLimits::MAX_ENTRIES {
        return Err(ScanError::TooManyEntries);
    }

    let central_bytes = u64::from(le_u32(&eocd, 12));
    let central_start = u64::from(le_u32(&eocd, 16));
    if central_start.checked_add(central_bytes) != Some(eocd_start) {
        return Err(ScanError::InvalidArchive);
    }

    let mut cursor = central_start;
    let mut entries = Vec::with_capacity(entry_count);
    let mut identities = PathIdentityTrie::default();
    let mut total_declared_compressed_bytes = 0_u64;
    let mut total_expanded_bytes = 0_u64;
    for index in 0..entry_count {
        check_budget(monitor)?;
        let mut header = [0_u8; ZIP_CENTRAL_HEADER_BYTES];
        read_exact_at(file, cursor, &mut header, monitor)?;
        if &header[..4] != b"PK\x01\x02" || le_u16(&header, 34) != 0 {
            return Err(ScanError::InvalidArchive);
        }

        let creator_version = header[4];
        let creator_system = header[5];
        if !matches!(creator_system, 0 | 3) {
            return Err(ScanError::InvalidArchive);
        }

        let compressed_size = le_u32(&header, 20);
        let expanded_size = le_u32(&header, 24);
        let local_header_offset = le_u32(&header, 42);
        if compressed_size == u32::MAX
            || expanded_size == u32::MAX
            || local_header_offset == u32::MAX
        {
            return Err(ScanError::InvalidArchive);
        }

        let name_bytes = usize::from(le_u16(&header, 28));
        if name_bytes > FrozenLimits::MAX_PATH_BYTES {
            return Err(invalid_path(index, PathViolation::TooLong));
        }
        let extra_bytes = u64::from(le_u16(&header, 30));
        let comment_bytes = u64::from(le_u16(&header, 32));
        if comment_bytes != 0 {
            return Err(ScanError::InvalidArchive);
        }
        let variable_start = cursor
            .checked_add(ZIP_CENTRAL_HEADER_BYTES as u64)
            .ok_or(ScanError::InvalidArchive)?;
        let record_end = variable_start
            .checked_add(u64::try_from(name_bytes).map_err(|_| ScanError::InvalidArchive)?)
            .and_then(|end| end.checked_add(extra_bytes))
            .and_then(|end| end.checked_add(comment_bytes))
            .ok_or(ScanError::InvalidArchive)?;
        if record_end > eocd_start {
            return Err(ScanError::InvalidArchive);
        }

        let mut raw_name = vec![0_u8; name_bytes];
        read_exact_at(file, variable_start, &mut raw_name, monitor)?;
        let flags = le_u16(&header, 8);
        let central_extra_start = variable_start
            .checked_add(u64::try_from(name_bytes).map_err(|_| ScanError::InvalidArchive)?)
            .ok_or(ScanError::InvalidArchive)?;
        validate_extra_fields(
            file,
            central_extra_start,
            extra_bytes,
            ExtraFieldLocation::Central,
            monitor,
        )?;
        let directory = raw_name.ends_with(b"/");
        let path = validate_path(index, &raw_name, directory)?;
        let compression_method = le_u16(&header, 10);
        validate_zip_flags(compression_method, flags)?;
        if raw_name.iter().any(|byte| !byte.is_ascii()) && flags & (1 << 11) == 0 {
            return Err(ScanError::InvalidArchive);
        }
        if !identities.insert(&path, directory) {
            return Err(invalid_path(index, PathViolation::Duplicate));
        }
        let external_attributes = le_u32(&header, 38);
        validate_entry_kind(
            index,
            raw_unix_mode(creator_system, external_attributes),
            directory,
        )?;

        let compressed_bytes = u64::from(compressed_size);
        let expanded_bytes = u64::from(expanded_size);
        if expanded_bytes > FrozenLimits::MAX_ENTRY_EXPANDED_BYTES {
            return Err(ScanError::EntryTooLarge { index });
        }
        if ratio_exceeded(expanded_bytes, compressed_bytes) {
            return Err(ScanError::EntryCompressionRatio { index });
        }
        total_declared_compressed_bytes = total_declared_compressed_bytes
            .checked_add(compressed_bytes)
            .ok_or(ScanError::AggregateCompressionRatio)?;
        total_expanded_bytes = total_expanded_bytes
            .checked_add(expanded_bytes)
            .ok_or(ScanError::TotalExpandedTooLarge)?;
        if total_expanded_bytes > FrozenLimits::MAX_TOTAL_EXPANDED_BYTES {
            return Err(ScanError::TotalExpandedTooLarge);
        }
        if ratio_exceeded(total_expanded_bytes, total_declared_compressed_bytes) {
            return Err(ScanError::AggregateCompressionRatio);
        }

        entries.push(AuthenticatedEntry {
            index,
            raw_name,
            path,
            central_header_start: cursor,
            creator_system,
            creator_version,
            flags,
            compression_method,
            modified_time: le_u16(&header, 12),
            modified_date: le_u16(&header, 14),
            crc32: le_u32(&header, 16),
            compressed_bytes,
            expanded_bytes,
            local_header_start: u64::from(local_header_offset),
            data_start: 0,
            data_end: 0,
            local_record_end: 0,
            external_attributes,
            directory,
        });
        cursor = record_end;
    }
    if cursor != eocd_start {
        return Err(ScanError::InvalidArchive);
    }

    for entry in &mut entries {
        authenticate_local_record(file, entry, central_start, monitor)?;
    }
    authenticate_physical_ranges(&entries, central_start)?;

    let mut total_physical_compressed_bytes = 0_u64;
    for entry in &entries {
        check_budget(monitor)?;
        if entry.compression_method == 8 {
            authenticate_deflate_stream(file, entry, monitor)?;
        }
        total_physical_compressed_bytes = total_physical_compressed_bytes
            .checked_add(entry.data_end - entry.data_start)
            .ok_or(ScanError::AggregateCompressionRatio)?;
    }
    if total_physical_compressed_bytes != total_declared_compressed_bytes {
        return Err(ScanError::InvalidArchive);
    }
    if ratio_exceeded(total_expanded_bytes, total_physical_compressed_bytes) {
        return Err(ScanError::AggregateCompressionRatio);
    }

    Ok(AuthenticatedArchive {
        entries,
        central_directory_start: central_start,
        total_physical_compressed_bytes,
        total_expanded_bytes,
    })
}

fn authenticate_local_record<M: ResourceMonitor + ?Sized>(
    file: &mut File,
    entry: &mut AuthenticatedEntry,
    central_start: u64,
    monitor: &M,
) -> Result<(), ScanError> {
    let mut local = [0_u8; ZIP_LOCAL_HEADER_BYTES];
    read_exact_at(file, entry.local_header_start, &mut local, monitor)?;
    if &local[..4] != b"PK\x03\x04"
        || le_u16(&local, 6) != entry.flags
        || le_u16(&local, 8) != entry.compression_method
        || le_u16(&local, 10) != entry.modified_time
        || le_u16(&local, 12) != entry.modified_date
    {
        return Err(ScanError::InvalidArchive);
    }

    let local_compressed = le_u32(&local, 18);
    let local_expanded = le_u32(&local, 22);
    if local_compressed == u32::MAX || local_expanded == u32::MAX {
        return Err(ScanError::InvalidArchive);
    }
    let using_descriptor = entry.flags & (1 << 3) != 0;
    if using_descriptor {
        let local_crc = le_u32(&local, 14);
        let local_compressed = u64::from(local_compressed);
        let local_expanded = u64::from(local_expanded);
        if !(local_crc == 0 || local_crc == entry.crc32)
            || !(local_compressed == 0 || local_compressed == entry.compressed_bytes)
            || !(local_expanded == 0 || local_expanded == entry.expanded_bytes)
        {
            return Err(ScanError::InvalidArchive);
        }
    } else if le_u32(&local, 14) != entry.crc32
        || u64::from(local_compressed) != entry.compressed_bytes
        || u64::from(local_expanded) != entry.expanded_bytes
    {
        return Err(ScanError::InvalidArchive);
    }
    if entry.compression_method == 0 && entry.compressed_bytes != entry.expanded_bytes {
        return Err(ScanError::InvalidArchive);
    }

    let local_name_bytes = usize::from(le_u16(&local, 26));
    let local_extra_bytes = u64::from(le_u16(&local, 28));
    if local_name_bytes != entry.raw_name.len() {
        return Err(ScanError::InvalidArchive);
    }
    let name_start = entry
        .local_header_start
        .checked_add(ZIP_LOCAL_HEADER_BYTES as u64)
        .ok_or(ScanError::InvalidArchive)?;
    let mut local_name = vec![0_u8; local_name_bytes];
    read_exact_at(file, name_start, &mut local_name, monitor)?;
    if local_name != entry.raw_name {
        return Err(ScanError::InvalidArchive);
    }
    let local_extra_start = name_start
        .checked_add(u64::try_from(local_name_bytes).map_err(|_| ScanError::InvalidArchive)?)
        .ok_or(ScanError::InvalidArchive)?;
    validate_extra_fields(
        file,
        local_extra_start,
        local_extra_bytes,
        ExtraFieldLocation::Local,
        monitor,
    )?;

    entry.data_start = name_start
        .checked_add(u64::try_from(local_name_bytes).map_err(|_| ScanError::InvalidArchive)?)
        .and_then(|offset| offset.checked_add(local_extra_bytes))
        .ok_or(ScanError::InvalidArchive)?;
    entry.data_end = entry
        .data_start
        .checked_add(entry.compressed_bytes)
        .ok_or(ScanError::InvalidArchive)?;
    if entry.data_end > central_start {
        return Err(ScanError::InvalidArchive);
    }
    entry.local_record_end = if using_descriptor {
        entry
            .data_end
            .checked_add(authenticate_data_descriptor(file, entry, monitor)?)
            .ok_or(ScanError::InvalidArchive)?
    } else {
        entry.data_end
    };
    if entry.local_record_end > central_start {
        return Err(ScanError::InvalidArchive);
    }
    Ok(())
}

fn authenticate_data_descriptor<M: ResourceMonitor + ?Sized>(
    file: &mut File,
    entry: &AuthenticatedEntry,
    monitor: &M,
) -> Result<u64, ScanError> {
    let mut descriptor = [0_u8; ZIP_DATA_DESCRIPTOR_BUFFER_BYTES];
    read_exact_at(file, entry.data_end, &mut descriptor, monitor)?;
    if &descriptor[..4] == b"PK\x07\x08"
        && le_u32(&descriptor, 4) == entry.crc32
        && u64::from(le_u32(&descriptor, 8)) == entry.compressed_bytes
        && u64::from(le_u32(&descriptor, 12)) == entry.expanded_bytes
    {
        return Ok(ZIP_DATA_DESCRIPTOR_WITH_SIGNATURE_BYTES);
    }
    if le_u32(&descriptor, 0) == entry.crc32
        && u64::from(le_u32(&descriptor, 4)) == entry.compressed_bytes
        && u64::from(le_u32(&descriptor, 8)) == entry.expanded_bytes
    {
        return Ok(ZIP_DATA_DESCRIPTOR_BYTES);
    }
    Err(ScanError::InvalidArchive)
}

fn validate_extra_fields<M: ResourceMonitor + ?Sized>(
    file: &mut File,
    offset: u64,
    length: u64,
    location: ExtraFieldLocation,
    monitor: &M,
) -> Result<(), ScanError> {
    let mut cursor = offset;
    let end = offset
        .checked_add(length)
        .ok_or(ScanError::InvalidArchive)?;
    let mut saw_extended_timestamp = false;
    let mut saw_ntfs_timestamp = false;
    while cursor < end {
        if end - cursor < 4 {
            return Err(ScanError::InvalidArchive);
        }
        let mut header = [0_u8; 4];
        read_exact_at(file, cursor, &mut header, monitor)?;
        let field_id = le_u16(&header, 0);
        let field_bytes = u64::from(le_u16(&header, 2));
        let payload_start = cursor.checked_add(4).ok_or(ScanError::InvalidArchive)?;
        cursor = payload_start
            .checked_add(field_bytes)
            .ok_or(ScanError::InvalidArchive)?;
        if cursor > end {
            return Err(ScanError::InvalidArchive);
        }
        match field_id {
            0x5455 if !saw_extended_timestamp => {
                validate_extended_timestamp(file, payload_start, field_bytes, location, monitor)?;
                saw_extended_timestamp = true;
            }
            0x000a if !saw_ntfs_timestamp => {
                validate_ntfs_timestamp(file, payload_start, field_bytes, monitor)?;
                saw_ntfs_timestamp = true;
            }
            _ => return Err(ScanError::InvalidArchive),
        }
    }
    Ok(())
}

fn validate_extended_timestamp<M: ResourceMonitor + ?Sized>(
    file: &mut File,
    offset: u64,
    length: u64,
    location: ExtraFieldLocation,
    monitor: &M,
) -> Result<(), ScanError> {
    if !(5..=13).contains(&length) {
        return Err(ScanError::InvalidArchive);
    }
    let mut payload = [0_u8; 13];
    let length = usize::try_from(length).map_err(|_| ScanError::InvalidArchive)?;
    read_exact_at(file, offset, &mut payload[..length], monitor)?;
    let flags = payload[0];
    let local_expected_length = usize::try_from(flags.count_ones())
        .ok()
        .and_then(|count| count.checked_mul(4))
        .and_then(|bytes| bytes.checked_add(1));
    let valid = match location {
        ExtraFieldLocation::Central => length == 5 && flags == 0x01,
        ExtraFieldLocation::Local => {
            flags & !0x07 == 0 && flags & 0x01 != 0 && local_expected_length == Some(length)
        }
    };
    if !valid {
        return Err(ScanError::InvalidArchive);
    }
    Ok(())
}

fn validate_ntfs_timestamp<M: ResourceMonitor + ?Sized>(
    file: &mut File,
    offset: u64,
    length: u64,
    monitor: &M,
) -> Result<(), ScanError> {
    if length != 32 {
        return Err(ScanError::InvalidArchive);
    }
    let mut payload = [0_u8; 32];
    read_exact_at(file, offset, &mut payload, monitor)?;
    if le_u32(&payload, 0) != 0 || le_u16(&payload, 4) != 0x0001 || le_u16(&payload, 6) != 24 {
        return Err(ScanError::InvalidArchive);
    }
    Ok(())
}

fn validate_zip_flags(compression_method: u16, flags: u16) -> Result<(), ScanError> {
    const DATA_DESCRIPTOR: u16 = 1 << 3;
    const UTF8_NAMES: u16 = 1 << 11;
    const DEFLATE_OPTIONS: u16 = (1 << 1) | (1 << 2);

    let allowed = match compression_method {
        0 => DATA_DESCRIPTOR | UTF8_NAMES,
        8 => DATA_DESCRIPTOR | UTF8_NAMES | DEFLATE_OPTIONS,
        _ => return Err(ScanError::InvalidArchive),
    };
    if flags & !allowed != 0 {
        return Err(ScanError::InvalidArchive);
    }
    Ok(())
}

fn authenticate_physical_ranges(
    entries: &[AuthenticatedEntry],
    central_start: u64,
) -> Result<(), ScanError> {
    let mut by_offset: Vec<&AuthenticatedEntry> = entries.iter().collect();
    by_offset.sort_unstable_by_key(|entry| entry.local_header_start);
    let mut expected_start = 0_u64;
    for entry in by_offset {
        if entry.local_header_start != expected_start
            || entry.data_start < entry.local_header_start
            || entry.data_end < entry.data_start
            || entry.local_record_end < entry.data_end
        {
            return Err(ScanError::InvalidArchive);
        }
        expected_start = entry.local_record_end;
    }
    if expected_start != central_start {
        return Err(ScanError::InvalidArchive);
    }
    Ok(())
}

fn authenticate_deflate_stream<M: ResourceMonitor + ?Sized>(
    file: &mut File,
    entry: &AuthenticatedEntry,
    monitor: &M,
) -> Result<(), ScanError> {
    file.seek(SeekFrom::Start(entry.data_start))
        .map_err(|_| ScanError::InvalidArchive)?;
    let mut decompressor = Decompress::new(false);
    let mut input = vec![0_u8; 64 * 1_024].into_boxed_slice();
    let mut output = vec![0_u8; 64 * 1_024].into_boxed_slice();
    let mut remaining = entry.compressed_bytes;
    let mut stream_ended = false;

    while remaining > 0 && !stream_ended {
        check_budget(monitor)?;
        let requested = usize::try_from(remaining.min(input.len() as u64))
            .map_err(|_| ScanError::InvalidArchive)?;
        file.read_exact(&mut input[..requested])
            .map_err(|_| ScanError::InvalidArchive)?;
        remaining -= u64::try_from(requested).map_err(|_| ScanError::InvalidArchive)?;
        let mut consumed = 0_usize;
        while consumed < requested {
            let before_in = decompressor.total_in();
            let before_out = decompressor.total_out();
            let status = decompressor
                .decompress(
                    &input[consumed..requested],
                    &mut output,
                    FlushDecompress::None,
                )
                .map_err(|_| ScanError::InvalidArchive)?;
            let used = usize::try_from(decompressor.total_in() - before_in)
                .map_err(|_| ScanError::InvalidArchive)?;
            let produced = decompressor.total_out() - before_out;
            consumed = consumed
                .checked_add(used)
                .ok_or(ScanError::InvalidArchive)?;
            if decompressor.total_out() > entry.expanded_bytes {
                return Err(ScanError::ExpandedSizeMismatch { index: entry.index });
            }
            if status == Status::StreamEnd {
                if consumed != requested || remaining != 0 {
                    return Err(ScanError::InvalidArchive);
                }
                stream_ended = true;
                break;
            }
            if used == 0 && produced == 0 {
                return Err(ScanError::InvalidArchive);
            }
        }
    }

    while !stream_ended {
        check_budget(monitor)?;
        let before_in = decompressor.total_in();
        let before_out = decompressor.total_out();
        let status = decompressor
            .decompress(&[], &mut output, FlushDecompress::Finish)
            .map_err(|_| ScanError::InvalidArchive)?;
        if decompressor.total_out() > entry.expanded_bytes {
            return Err(ScanError::ExpandedSizeMismatch { index: entry.index });
        }
        if status == Status::StreamEnd {
            stream_ended = true;
        } else if decompressor.total_in() == before_in && decompressor.total_out() == before_out {
            return Err(ScanError::InvalidArchive);
        }
    }

    if decompressor.total_in() != entry.compressed_bytes
        || decompressor.total_out() != entry.expanded_bytes
    {
        return Err(ScanError::ExpandedSizeMismatch { index: entry.index });
    }
    Ok(())
}

fn read_exact_at<M: ResourceMonitor + ?Sized>(
    file: &mut File,
    offset: u64,
    buffer: &mut [u8],
    monitor: &M,
) -> Result<(), ScanError> {
    check_budget(monitor)?;
    file.seek(SeekFrom::Start(offset))
        .and_then(|_| file.read_exact(buffer))
        .map_err(|_| ScanError::InvalidArchive)?;
    check_budget(monitor)
}

fn le_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn le_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

#[derive(Debug)]
struct EntryPlan {
    index: usize,
    path: String,
    compressed_bytes: u64,
    expanded_bytes: u64,
    directory: bool,
    format: MemberFormat,
}

#[derive(Debug)]
struct ArchivePlan {
    entries: Vec<EntryPlan>,
    total_compressed_bytes: u64,
    total_expanded_bytes: u64,
    has_conversations: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MemberFormat {
    Directory,
    Json,
    Html,
}

impl MemberFormat {
    fn textual(self) -> bool {
        matches!(self, Self::Json | Self::Html)
    }
}

fn scan_immutable_snapshot<M: ResourceMonitor + ?Sized>(
    snapshot: ImmutableSnapshot,
    monitor: &M,
) -> Result<DeepZipCatalog, ScanError> {
    check_budget(monitor)?;
    let ImmutableSnapshot {
        mut file,
        bytes: archive_bytes,
        sha256: archive_sha256,
    } = snapshot;
    let authenticated = authenticate_zip_layout(&mut file, archive_bytes, monitor)?;
    file.seek(SeekFrom::Start(0))
        .map_err(|_| ScanError::InvalidArchive)?;
    let config = Config {
        archive_offset: ArchiveOffset::Known(0),
    };
    let mut archive =
        ZipArchive::with_config(config, file).map_err(|_| ScanError::InvalidArchive)?;
    check_budget(monitor)?;
    if archive.offset() != 0
        || archive.central_directory_start() != authenticated.central_directory_start
        || !archive.comment().is_empty()
        || archive.zip64_comment().is_some()
        || archive.len() != authenticated.entries.len()
    {
        return Err(ScanError::InvalidArchive);
    }
    if archive.len() > FrozenLimits::MAX_ENTRIES {
        return Err(ScanError::TooManyEntries);
    }

    let plan = plan_archive(&mut archive, &authenticated, monitor)?;
    let entries = stream_entries(&mut archive, plan.entries, monitor)?;
    if !plan.has_conversations {
        return Err(ScanError::UnsupportedArchiveLayout);
    }
    check_budget(monitor)?;
    Ok(DeepZipCatalog {
        archive_bytes,
        archive_sha256,
        total_compressed_bytes: plan.total_compressed_bytes,
        total_expanded_bytes: plan.total_expanded_bytes,
        entries,
    })
}

fn plan_archive<M: ResourceMonitor + ?Sized>(
    archive: &mut ZipArchive<File>,
    authenticated: &AuthenticatedArchive,
    monitor: &M,
) -> Result<ArchivePlan, ScanError> {
    let mut plans = Vec::with_capacity(archive.len());
    let mut has_conversations_singleton = false;
    let mut conversation_parts = Vec::new();

    for index in 0..archive.len() {
        check_budget(monitor)?;
        let entry = archive
            .by_index(index)
            .map_err(|_| ScanError::InvalidArchive)?;
        let authenticated_entry = authenticated
            .entries
            .get(index)
            .ok_or(ScanError::InvalidArchive)?;
        let metadata = entry.get_metadata();
        let selected_creator_system = u8::from(metadata.system);
        if authenticated_entry.index != index
            || entry.name_raw() != authenticated_entry.raw_name
            || entry.central_header_start() != authenticated_entry.central_header_start
            || selected_creator_system != authenticated_entry.creator_system
            || metadata.version_made_by != authenticated_entry.creator_version
            || metadata.flags != authenticated_entry.flags
            || metadata.external_attributes != authenticated_entry.external_attributes
            || entry.compressed_size() != authenticated_entry.compressed_bytes
            || entry.size() != authenticated_entry.expanded_bytes
            || entry.crc32() != authenticated_entry.crc32
            || entry.header_start() != authenticated_entry.local_header_start
            || entry.data_start() != authenticated_entry.data_start
            || !compression_method_matches(
                authenticated_entry.compression_method,
                entry.compression(),
            )
            || entry.unix_mode()
                != raw_unix_mode(
                    authenticated_entry.creator_system,
                    authenticated_entry.external_attributes,
                )
            || !entry.comment().is_empty()
            || entry.is_dir() != authenticated_entry.directory
        {
            return Err(ScanError::InvalidArchive);
        }
        let path = authenticated_entry.path.clone();
        validate_entry_kind(index, entry.unix_mode(), entry.is_dir())?;
        if entry.encrypted()
            || !matches!(
                entry.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            )
        {
            return Err(ScanError::InvalidArchive);
        }
        if entry.is_dir() && (entry.size() != 0 || entry.compressed_size() != 0) {
            return Err(ScanError::UnsupportedEntryKind {
                index,
                violation: EntryKindViolation::ContradictoryDirectory,
            });
        }
        if !entry.is_dir() && has_nested_archive_extension(&path) {
            return Err(ScanError::NestedArchive { index });
        }
        let format = if entry.is_dir() {
            MemberFormat::Directory
        } else {
            classify_member_format(&path).ok_or(ScanError::UnsupportedMember { index })?
        };
        match conversation_member(&path) {
            Some(ConversationMember::Singleton) => has_conversations_singleton = true,
            Some(ConversationMember::Part(part)) => conversation_parts.push(part),
            None => {}
        }

        plans.push(EntryPlan {
            index,
            path,
            compressed_bytes: authenticated_entry.compressed_bytes,
            expanded_bytes: authenticated_entry.expanded_bytes,
            directory: entry.is_dir(),
            format,
        });
    }

    conversation_parts.sort_unstable();
    let has_conversations = match (has_conversations_singleton, conversation_parts.is_empty()) {
        (true, true) => true,
        (false, false) => conversation_parts
            .iter()
            .copied()
            .eq(0..conversation_parts.len()),
        (true, false) | (false, true) => false,
    };

    Ok(ArchivePlan {
        entries: plans,
        total_compressed_bytes: authenticated.total_physical_compressed_bytes,
        total_expanded_bytes: authenticated.total_expanded_bytes,
        has_conversations,
    })
}

fn stream_entries<M: ResourceMonitor + ?Sized>(
    archive: &mut ZipArchive<File>,
    plans: Vec<EntryPlan>,
    monitor: &M,
) -> Result<Vec<CatalogEntry>, ScanError> {
    let mut entries = Vec::with_capacity(plans.len());
    for plan in plans {
        entries.push(stream_entry(archive, plan, monitor)?);
    }
    entries.sort_unstable_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
}

fn stream_entry<M: ResourceMonitor + ?Sized>(
    archive: &mut ZipArchive<File>,
    plan: EntryPlan,
    monitor: &M,
) -> Result<CatalogEntry, ScanError> {
    check_budget(monitor)?;
    let entry = archive
        .by_index(plan.index)
        .map_err(|_| ScanError::InvalidArchive)?;
    let mut inspected = InspectingReader::new(entry, plan.index, plan.format, monitor);

    let content_result = if plan.format == MemberFormat::Json {
        let mut deserializer = serde_json::Deserializer::from_reader(&mut inspected);
        IgnoredAny::deserialize(&mut deserializer).and_then(|_| deserializer.end())
    } else {
        io::copy(&mut inspected, &mut io::sink())
            .map(|_| ())
            .map_err(serde_json::Error::io)
    };
    if content_result.is_err() {
        let _ = io::copy(&mut inspected, &mut io::sink());
        return match inspected.finish(plan.expanded_bytes) {
            Ok(_) => Err(ScanError::UnsupportedMemberContent { index: plan.index }),
            Err(error) => Err(error),
        };
    }
    let inspected = inspected.finish(plan.expanded_bytes)?;
    if !member_signature_matches(
        plan.format,
        &plan.path,
        &inspected.prefix,
        &inspected.suffix,
    ) {
        return Err(ScanError::UnsupportedMemberContent { index: plan.index });
    }

    Ok(CatalogEntry {
        path: plan.path,
        compressed_bytes: plan.compressed_bytes,
        expanded_bytes: inspected.expanded_bytes,
        sha256: inspected.sha256,
        directory: plan.directory,
    })
}

struct InspectedEntry {
    expanded_bytes: u64,
    sha256: [u8; 32],
    prefix: Vec<u8>,
    suffix: Vec<u8>,
}

struct InspectingReader<'a, R, M: ResourceMonitor + ?Sized> {
    inner: R,
    index: usize,
    monitor: &'a M,
    digest: Sha256,
    expanded_bytes: u64,
    prefix: Vec<u8>,
    suffix: Vec<u8>,
    archive_detector: NestedArchiveDetector,
    utf8: Option<Utf8Validator>,
    failure: Option<ScanError>,
}

impl<'a, R: Read, M: ResourceMonitor + ?Sized> InspectingReader<'a, R, M> {
    fn new(inner: R, index: usize, format: MemberFormat, monitor: &'a M) -> Self {
        Self {
            inner,
            index,
            monitor,
            digest: Sha256::new(),
            expanded_bytes: 0,
            prefix: Vec::with_capacity(MAX_MEMBER_PREFIX_BYTES),
            suffix: Vec::with_capacity(MAX_MEMBER_SUFFIX_BYTES),
            archive_detector: NestedArchiveDetector::default(),
            utf8: format.textual().then(Utf8Validator::default),
            failure: None,
        }
    }

    fn finish(self, expected_expanded_bytes: u64) -> Result<InspectedEntry, ScanError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if self.archive_detector.finish() {
            return Err(ScanError::NestedArchive { index: self.index });
        }
        if self.expanded_bytes != expected_expanded_bytes {
            return Err(ScanError::ExpandedSizeMismatch { index: self.index });
        }
        if self.utf8.is_some_and(|validator| !validator.complete()) {
            return Err(ScanError::UnsupportedMemberContent { index: self.index });
        }
        Ok(InspectedEntry {
            expanded_bytes: self.expanded_bytes,
            sha256: self.digest.finalize().into(),
            prefix: self.prefix,
            suffix: self.suffix,
        })
    }

    fn reject(&mut self, error: ScanError) -> io::Error {
        self.failure = Some(error);
        io::Error::other("entry rejected")
    }
}

impl<R: Read, M: ResourceMonitor + ?Sized> Read for InspectingReader<'_, R, M> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.failure.is_some() {
            return Err(io::Error::other("entry rejected"));
        }
        if let Err(error) = check_budget(self.monitor) {
            return Err(self.reject(error));
        }
        let Ok(read) = self.inner.read(buffer) else {
            return Err(self.reject(ScanError::InvalidArchive));
        };
        if read == 0 {
            return Ok(0);
        }
        let chunk = &buffer[..read];
        let Ok(read_u64) = u64::try_from(read) else {
            return Err(self.reject(ScanError::EntryTooLarge { index: self.index }));
        };
        self.expanded_bytes = match self.expanded_bytes.checked_add(read_u64) {
            Some(expanded) => expanded,
            None => return Err(self.reject(ScanError::EntryTooLarge { index: self.index })),
        };
        if self.expanded_bytes > FrozenLimits::MAX_ENTRY_EXPANDED_BYTES {
            return Err(self.reject(ScanError::EntryTooLarge { index: self.index }));
        }
        if self.archive_detector.push(chunk) {
            return Err(self.reject(ScanError::NestedArchive { index: self.index }));
        }
        if let Some(validator) = self.utf8.as_mut()
            && !validator.push(chunk)
        {
            return Err(self.reject(ScanError::UnsupportedMemberContent { index: self.index }));
        }
        let prefix_remaining = MAX_MEMBER_PREFIX_BYTES.saturating_sub(self.prefix.len());
        self.prefix
            .extend_from_slice(&chunk[..read.min(prefix_remaining)]);
        if chunk.len() >= MAX_MEMBER_SUFFIX_BYTES {
            self.suffix.clear();
            self.suffix
                .extend_from_slice(&chunk[chunk.len() - MAX_MEMBER_SUFFIX_BYTES..]);
        } else {
            self.suffix.extend_from_slice(chunk);
            if self.suffix.len() > MAX_MEMBER_SUFFIX_BYTES {
                self.suffix
                    .drain(..self.suffix.len() - MAX_MEMBER_SUFFIX_BYTES);
            }
        }
        self.digest.update(chunk);
        Ok(read)
    }
}

#[derive(Default)]
struct Utf8Validator {
    incomplete: Vec<u8>,
    invalid: bool,
}

impl Utf8Validator {
    fn push(&mut self, bytes: &[u8]) -> bool {
        if self.invalid {
            return false;
        }
        if self.incomplete.is_empty() {
            self.validate(bytes)
        } else {
            let mut combined = Vec::with_capacity(self.incomplete.len() + bytes.len());
            combined.extend_from_slice(&self.incomplete);
            combined.extend_from_slice(bytes);
            self.incomplete.clear();
            self.validate(&combined)
        }
    }

    fn validate(&mut self, bytes: &[u8]) -> bool {
        match std::str::from_utf8(bytes) {
            Ok(text) if allowed_text_controls(text) => true,
            Ok(_) => {
                self.invalid = true;
                false
            }
            Err(error) if error.error_len().is_some() => {
                self.invalid = true;
                false
            }
            Err(error) => {
                let valid = std::str::from_utf8(&bytes[..error.valid_up_to()])
                    .is_ok_and(allowed_text_controls);
                if !valid {
                    self.invalid = true;
                    return false;
                }
                self.incomplete
                    .extend_from_slice(&bytes[error.valid_up_to()..]);
                true
            }
        }
    }

    fn complete(self) -> bool {
        !self.invalid && self.incomplete.is_empty()
    }
}

fn allowed_text_controls(text: &str) -> bool {
    text.chars()
        .all(|character| !character.is_control() || matches!(character, '\t' | '\n' | '\r'))
}

#[derive(Default)]
struct NestedArchiveDetector {
    prefix: Vec<u8>,
    zip_tail: Vec<u8>,
    suffix: Vec<u8>,
    rejected: bool,
}

impl NestedArchiveDetector {
    fn push(&mut self, bytes: &[u8]) -> bool {
        if self.rejected {
            return true;
        }

        let mut zip_probe = Vec::with_capacity(self.zip_tail.len() + bytes.len());
        zip_probe.extend_from_slice(&self.zip_tail);
        zip_probe.extend_from_slice(bytes);
        let zip_detected = contains_structural_zip_record(&zip_probe);
        if zip_probe.len() >= NESTED_ZIP_PROBE_BYTES - 1 {
            self.zip_tail.clear();
            self.zip_tail
                .extend_from_slice(&zip_probe[zip_probe.len() - (NESTED_ZIP_PROBE_BYTES - 1)..]);
        } else {
            self.zip_tail = zip_probe;
        }

        let prefix_remaining = NESTED_PREFIX_PROBE_BYTES.saturating_sub(self.prefix.len());
        self.prefix
            .extend_from_slice(&bytes[..bytes.len().min(prefix_remaining)]);
        update_suffix(&mut self.suffix, bytes, NESTED_SUFFIX_PROBE_BYTES);
        self.rejected = zip_detected || has_anchored_archive_format(&self.prefix);
        self.rejected
    }

    fn finish(&self) -> bool {
        self.rejected || is_structural_dmg_trailer(&self.suffix)
    }
}

fn update_suffix(suffix: &mut Vec<u8>, bytes: &[u8], limit: usize) {
    if bytes.len() >= limit {
        suffix.clear();
        suffix.extend_from_slice(&bytes[bytes.len() - limit..]);
    } else {
        suffix.extend_from_slice(bytes);
        if suffix.len() > limit {
            suffix.drain(..suffix.len() - limit);
        }
    }
}

fn contains_structural_zip_record(bytes: &[u8]) -> bool {
    bytes.windows(4).enumerate().any(|(index, signature)| {
        (signature == b"PK\x03\x04" && is_structural_zip_local_header(&bytes[index..]))
            || (signature == b"PK\x05\x06" && is_structural_empty_zip_eocd(&bytes[index..]))
    })
}

fn is_structural_zip_local_header(bytes: &[u8]) -> bool {
    if bytes.len() < ZIP_LOCAL_HEADER_BYTES || &bytes[..4] != b"PK\x03\x04" {
        return false;
    }
    let name_bytes = le_u16(bytes, 26);
    name_bytes > 0
}

fn is_structural_empty_zip_eocd(bytes: &[u8]) -> bool {
    bytes.len() >= ZIP_EOCD_BYTES
        && &bytes[..4] == b"PK\x05\x06"
        && le_u16(bytes, 4) == 0
        && le_u16(bytes, 6) == 0
        && le_u16(bytes, 8) == 0
        && le_u16(bytes, 10) == 0
        && le_u32(bytes, 12) == 0
        && le_u32(bytes, 16) == 0
}

fn has_anchored_archive_format(prefix: &[u8]) -> bool {
    is_structural_gzip(prefix)
        || is_structural_compress(prefix)
        || is_structural_bzip2(prefix)
        || prefix.starts_with(b"\xfd7zXZ\0")
        || is_structural_7z(prefix)
        || prefix.starts_with(b"Rar!\x1a\x07\x00")
        || prefix.starts_with(b"Rar!\x1a\x07\x01\x00")
        || prefix.starts_with(&[0x28, 0xb5, 0x2f, 0xfd])
        || prefix.starts_with(&[0x04, 0x22, 0x4d, 0x18])
        || is_structural_cab(prefix)
        || prefix.starts_with(b"!<arch>\n")
        || is_structural_cpio(prefix)
        || is_structural_xar(prefix)
        || is_structural_rpm(prefix)
        || is_structural_tar(prefix)
        || is_structural_iso(prefix)
}

fn is_structural_gzip(bytes: &[u8]) -> bool {
    bytes.len() >= 10 && bytes.starts_with(&[0x1f, 0x8b, 8]) && bytes[3] & 0xe0 == 0
}

fn is_structural_compress(bytes: &[u8]) -> bool {
    bytes.len() >= 3
        && bytes.starts_with(&[0x1f, 0x9d])
        && bytes[2] & 0x60 == 0
        && (9..=16).contains(&(bytes[2] & 0x1f))
}

fn is_structural_bzip2(bytes: &[u8]) -> bool {
    bytes.len() >= 10
        && bytes.starts_with(b"BZh")
        && matches!(bytes[3], b'1'..=b'9')
        && (&bytes[4..10] == b"1AY&SY" || &bytes[4..10] == b"\x17rE8P\x90")
}

fn is_structural_7z(bytes: &[u8]) -> bool {
    bytes.len() >= 32 && bytes.starts_with(&[0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c]) && bytes[6] == 0
}

fn is_structural_cab(bytes: &[u8]) -> bool {
    if bytes.len() < 36 || !bytes.starts_with(b"MSCF") {
        return false;
    }
    let cabinet_bytes = le_u32(bytes, 8);
    let files_offset = le_u32(bytes, 16);
    bytes[4..8] == [0; 4]
        && bytes[12..16] == [0; 4]
        && bytes[20..24] == [0; 4]
        && cabinet_bytes >= 36
        && (36..=cabinet_bytes).contains(&files_offset)
}

fn is_structural_cpio(bytes: &[u8]) -> bool {
    if bytes.starts_with(b"070701") || bytes.starts_with(b"070702") {
        bytes.len() >= 110 && bytes[6..110].iter().all(u8::is_ascii_hexdigit)
    } else if bytes.starts_with(b"070707") {
        bytes.len() >= 76 && bytes[6..76].iter().all(|byte| matches!(*byte, b'0'..=b'7'))
    } else {
        false
    }
}

fn is_structural_xar(bytes: &[u8]) -> bool {
    bytes.len() >= 28
        && bytes.starts_with(b"xar!")
        && be_u16(bytes, 4) >= 28
        && be_u16(bytes, 6) == 1
}

fn is_structural_rpm(bytes: &[u8]) -> bool {
    bytes.len() >= 96 && bytes.starts_with(&[0xed, 0xab, 0xee, 0xdb]) && matches!(bytes[4], 3 | 4)
}

fn is_structural_tar(bytes: &[u8]) -> bool {
    if bytes.len() < 512
        || (&bytes[257..263] != b"ustar\0" && &bytes[257..263] != b"ustar ")
        || !bytes[148..156]
            .iter()
            .all(|byte| matches!(*byte, 0 | b' ' | b'0'..=b'7'))
    {
        return false;
    }
    let stored = bytes[148..156]
        .iter()
        .copied()
        .filter(|byte| matches!(*byte, b'0'..=b'7'))
        .fold(0_u64, |value, digit| {
            value
                .saturating_mul(8)
                .saturating_add(u64::from(digit - b'0'))
        });
    let actual = bytes[..512]
        .iter()
        .enumerate()
        .map(|(index, byte)| {
            if (148..156).contains(&index) {
                u64::from(b' ')
            } else {
                u64::from(*byte)
            }
        })
        .sum::<u64>();
    stored == actual
}

fn is_structural_iso(bytes: &[u8]) -> bool {
    const VOLUME_DESCRIPTOR: usize = 16 * 2_048;
    bytes.len() >= VOLUME_DESCRIPTOR + 7
        && matches!(bytes[VOLUME_DESCRIPTOR], 0..=3 | 255)
        && &bytes[VOLUME_DESCRIPTOR + 1..VOLUME_DESCRIPTOR + 6] == b"CD001"
        && bytes[VOLUME_DESCRIPTOR + 6] == 1
}

fn is_structural_dmg_trailer(bytes: &[u8]) -> bool {
    bytes.len() == NESTED_SUFFIX_PROBE_BYTES
        && bytes.starts_with(b"koly")
        && be_u32(bytes, 4) == 4
        && be_u32(bytes, 8) == 512
}

fn be_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([bytes[offset], bytes[offset + 1]])
}

fn be_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn member_signature_matches(
    format: MemberFormat,
    path: &str,
    prefix: &[u8],
    suffix: &[u8],
) -> bool {
    let text = trim_text_prefix(prefix);
    match format {
        MemberFormat::Directory => prefix.is_empty() && suffix.is_empty(),
        MemberFormat::Json => {
            if conversation_member(path).is_some() {
                text.starts_with(b"[")
            } else {
                text.starts_with(b"[") || text.starts_with(b"{")
            }
        }
        MemberFormat::Html => {
            (starts_with_ascii_case_insensitive(text, b"<!doctype html")
                || starts_with_ascii_case_insensitive(text, b"<html"))
                && ends_with_ascii_case_insensitive(trim_text_suffix(suffix), b"</html>")
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConversationMember {
    Singleton,
    Part(usize),
}

fn conversation_member(path: &str) -> Option<ConversationMember> {
    if path == "conversations.json" {
        return Some(ConversationMember::Singleton);
    }
    let digits = path.strip_prefix("conversations-")?.strip_suffix(".json")?;
    if digits.len() != 3 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let part = digits.parse::<usize>().ok()?;
    Some(ConversationMember::Part(part))
}

fn trim_text_prefix(mut bytes: &[u8]) -> &[u8] {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        bytes = &bytes[3..];
    }
    while bytes.first().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[1..];
    }
    bytes
}

fn trim_text_suffix(mut bytes: &[u8]) -> &[u8] {
    while bytes.last().is_some_and(u8::is_ascii_whitespace) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn starts_with_ascii_case_insensitive(bytes: &[u8], expected: &[u8]) -> bool {
    bytes
        .get(..expected.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(expected))
}

fn ends_with_ascii_case_insensitive(bytes: &[u8], expected: &[u8]) -> bool {
    bytes
        .get(bytes.len().saturating_sub(expected.len())..)
        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(expected))
}

fn classify_member_format(path: &str) -> Option<MemberFormat> {
    let extension = path.rsplit_once('.')?.1.to_ascii_lowercase();
    match extension.as_str() {
        "json" => Some(MemberFormat::Json),
        "html" => Some(MemberFormat::Html),
        _ => None,
    }
}

fn validate_path(index: usize, raw: &[u8], directory: bool) -> Result<String, ScanError> {
    if raw.is_empty() {
        return Err(invalid_path(index, PathViolation::Empty));
    }
    if raw.len() > FrozenLimits::MAX_PATH_BYTES {
        return Err(invalid_path(index, PathViolation::TooLong));
    }
    let path =
        std::str::from_utf8(raw).map_err(|_| invalid_path(index, PathViolation::InvalidUtf8))?;
    if path.contains('\\') {
        return Err(invalid_path(index, PathViolation::Backslash));
    }
    if path.chars().any(is_ambiguous_path_character) {
        return Err(invalid_path(index, PathViolation::Control));
    }
    let bytes = path.as_bytes();
    if path.starts_with('/')
        || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':')
    {
        return Err(invalid_path(index, PathViolation::Absolute));
    }
    if directory != path.ends_with('/') {
        return Err(ScanError::UnsupportedEntryKind {
            index,
            violation: EntryKindViolation::ContradictoryDirectory,
        });
    }

    let without_trailing = if directory {
        path.trim_end_matches('/')
    } else {
        path
    };
    let mut components = Vec::new();
    for component in without_trailing.split('/') {
        if component.is_empty() {
            continue;
        }
        if component == "." || component == ".." {
            return Err(invalid_path(index, PathViolation::Traversal));
        }
        components.push(component.nfc().collect::<String>());
    }
    if components.is_empty() {
        return Err(invalid_path(index, PathViolation::Empty));
    }
    if components.len() > FrozenLimits::MAX_PATH_DEPTH {
        return Err(invalid_path(index, PathViolation::TooDeep));
    }
    let mut canonical = components.join("/");
    if directory {
        canonical.push('/');
    }
    if canonical.len() > FrozenLimits::MAX_PATH_BYTES {
        return Err(invalid_path(index, PathViolation::TooLong));
    }
    Ok(canonical)
}

fn invalid_path(index: usize, violation: PathViolation) -> ScanError {
    ScanError::InvalidPath { index, violation }
}

fn is_ambiguous_path_character(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{206f}'
        )
}

#[cfg(test)]
fn path_collision_key(path: &str) -> String {
    path.trim_end_matches('/')
        .split('/')
        .map(canonical_component_key)
        .collect::<Vec<_>>()
        .join("/")
}

fn canonical_component_key(component: &str) -> String {
    component.case_fold().nfc().collect()
}

fn raw_unix_mode(creator_system: u8, external_attributes: u32) -> Option<u32> {
    const DOS_READ_ONLY: u32 = 0x01;
    const DOS_DIRECTORY: u32 = 0x10;
    const DIRECTORY: u32 = 0o040_000;
    const REGULAR: u32 = 0o100_000;

    if external_attributes == 0 {
        return None;
    }
    match creator_system {
        3 => Some(external_attributes >> 16),
        0 => {
            let mut mode = if external_attributes & DOS_DIRECTORY != 0 {
                DIRECTORY | 0o775
            } else {
                REGULAR | 0o664
            };
            if external_attributes & DOS_READ_ONLY != 0 {
                mode &= 0o0555;
            }
            Some(mode)
        }
        _ => None,
    }
}

fn compression_method_matches(raw: u16, selected: CompressionMethod) -> bool {
    matches!(
        (raw, selected),
        (0, CompressionMethod::Stored) | (8, CompressionMethod::Deflated)
    )
}

fn validate_entry_kind(
    index: usize,
    unix_mode: Option<u32>,
    directory: bool,
) -> Result<(), ScanError> {
    const TYPE_MASK: u32 = 0o170_000;
    const DIRECTORY: u32 = 0o040_000;
    const REGULAR: u32 = 0o100_000;
    const SYMLINK: u32 = 0o120_000;

    let Some(mode) = unix_mode else {
        return Ok(());
    };
    match mode & TYPE_MASK {
        0 => Ok(()),
        SYMLINK => Err(ScanError::UnsupportedEntryKind {
            index,
            violation: EntryKindViolation::Symlink,
        }),
        DIRECTORY if directory => Ok(()),
        REGULAR if !directory => Ok(()),
        DIRECTORY | REGULAR => Err(ScanError::UnsupportedEntryKind {
            index,
            violation: EntryKindViolation::ContradictoryDirectory,
        }),
        _ => Err(ScanError::UnsupportedEntryKind {
            index,
            violation: EntryKindViolation::Special,
        }),
    }
}

fn ratio_exceeded(expanded: u64, compressed: u64) -> bool {
    if expanded == 0 {
        return false;
    }
    compressed
        .checked_mul(FrozenLimits::MAX_COMPRESSION_RATIO)
        .is_none_or(|limit| expanded > limit)
}

fn has_nested_archive_extension(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [
        ".zip", ".zipx", ".tar", ".tgz", ".gz", ".bz2", ".xz", ".7z", ".rar", ".zst", ".lz4",
        ".lzh", ".lha", ".arj", ".ace", ".cab", ".cpio", ".xar", ".iso", ".dmg", ".deb", ".rpm",
        ".jar", ".war", ".ear", ".apk", ".aab", ".whl", ".docx", ".xlsx", ".pptx", ".odt", ".ods",
        ".odp", ".epub",
    ]
    .iter()
    .any(|extension| lower.ends_with(extension))
}

fn check_budget<M: ResourceMonitor + ?Sized>(monitor: &M) -> Result<(), ScanError> {
    if monitor.cancelled() {
        return Err(ScanError::Cancelled);
    }
    if monitor.elapsed() > FrozenLimits::MAX_WALL_TIME {
        return Err(ScanError::WallClockExceeded);
    }
    let rss_bytes = monitor.rss_bytes().ok_or(ScanError::RssUnavailable)?;
    if rss_bytes > FrozenLimits::MAX_RSS_BYTES {
        return Err(ScanError::RssExceeded);
    }
    Ok(())
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use zip::write::SimpleFileOptions;

    struct FixedMonitor {
        cancelled: bool,
        elapsed: Duration,
        rss_bytes: Option<u64>,
    }

    impl ResourceMonitor for FixedMonitor {
        fn cancelled(&self) -> bool {
            self.cancelled
        }

        fn elapsed(&self) -> Duration {
            self.elapsed
        }

        fn rss_bytes(&self) -> Option<u64> {
            self.rss_bytes
        }
    }

    struct FixedProbe(Option<u64>);

    impl ChildRssProbe for FixedProbe {
        fn rss_bytes(&mut self, _pid: u32) -> Option<u64> {
            self.0
        }
    }

    #[test]
    fn fixed_limits_match_the_competition_contract() {
        assert_eq!(FrozenLimits::MAX_ARCHIVE_BYTES, 1 << 30);
        assert_eq!(FrozenLimits::MAX_ENTRIES, 25_000);
        assert_eq!(FrozenLimits::MAX_ENTRY_EXPANDED_BYTES, 512 << 20);
        assert_eq!(FrozenLimits::MAX_TOTAL_EXPANDED_BYTES, 4 << 30);
        assert_eq!(FrozenLimits::MAX_COMPRESSION_RATIO, 100);
        assert_eq!(FrozenLimits::MAX_PATH_BYTES, 512);
        assert_eq!(FrozenLimits::MAX_PATH_DEPTH, 16);
        assert_eq!(FrozenLimits::MAX_WALL_TIME, Duration::from_secs(600));
        assert_eq!(FrozenLimits::MAX_RSS_BYTES, 512 << 20);
        assert_eq!(FrozenLimits::MAX_PROTOCOL_BYTES, 40 << 20);
    }

    #[test]
    fn ratio_boundary_is_closed_above_one_hundred_to_one() {
        assert!(!ratio_exceeded(10_000, 100));
        assert!(ratio_exceeded(10_001, 100));
        assert!(ratio_exceeded(1, 0));
        assert!(!ratio_exceeded(0, 0));
    }

    #[test]
    fn archive_detector_is_structural_and_covers_preamble_zip() {
        let mut local_header = [0_u8; ZIP_LOCAL_HEADER_BYTES];
        local_header[..4].copy_from_slice(b"PK\x03\x04");
        local_header[4..6].copy_from_slice(&20_u16.to_le_bytes());
        local_header[26..28].copy_from_slice(&1_u16.to_le_bytes());
        for split_at in 1..local_header.len() {
            let mut detector = NestedArchiveDetector::default();
            let mut first = b"arbitrary-preamble".to_vec();
            first.extend_from_slice(&local_header[..split_at]);
            assert!(!detector.push(&first));
            assert!(detector.push(&local_header[split_at..]));
        }

        assert!(is_structural_gzip(&[0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 0]));
        assert!(is_structural_compress(&[0x1f, 0x9d, 0x90]));
        assert!(is_structural_bzip2(b"BZh91AY&SY"));
        assert!(has_anchored_archive_format(b"\xfd7zXZ\0"));
        let mut seven_zip = [0_u8; 32];
        seven_zip[..6].copy_from_slice(&[0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c]);
        assert!(is_structural_7z(&seven_zip));
        assert!(has_anchored_archive_format(b"Rar!\x1a\x07\x00"));
        assert!(has_anchored_archive_format(&[0x28, 0xb5, 0x2f, 0xfd]));
        assert!(has_anchored_archive_format(&[0x04, 0x22, 0x4d, 0x18]));
        let mut cab = [0_u8; 36];
        cab[..4].copy_from_slice(b"MSCF");
        cab[8..12].copy_from_slice(&36_u32.to_le_bytes());
        cab[16..20].copy_from_slice(&36_u32.to_le_bytes());
        assert!(is_structural_cab(&cab));
        assert!(has_anchored_archive_format(b"!<arch>\n"));
        let mut cpio = [b'0'; 110];
        cpio[..6].copy_from_slice(b"070701");
        assert!(is_structural_cpio(&cpio));
        let mut xar = [0_u8; 28];
        xar[..4].copy_from_slice(b"xar!");
        xar[4..6].copy_from_slice(&28_u16.to_be_bytes());
        xar[6..8].copy_from_slice(&1_u16.to_be_bytes());
        assert!(is_structural_xar(&xar));
        let mut rpm = [0_u8; 96];
        rpm[..4].copy_from_slice(&[0xed, 0xab, 0xee, 0xdb]);
        rpm[4] = 4;
        assert!(is_structural_rpm(&rpm));
        let mut iso = vec![0_u8; NESTED_PREFIX_PROBE_BYTES];
        iso[16 * 2_048] = 1;
        iso[(16 * 2_048) + 1..(16 * 2_048) + 6].copy_from_slice(b"CD001");
        iso[(16 * 2_048) + 6] = 1;
        assert!(is_structural_iso(&iso));
        let mut dmg = [0_u8; NESTED_SUFFIX_PROBE_BYTES];
        dmg[..4].copy_from_slice(b"koly");
        dmg[4..8].copy_from_slice(&4_u32.to_be_bytes());
        dmg[8..12].copy_from_slice(&512_u32.to_be_bytes());
        assert!(is_structural_dmg_trailer(&dmg));

        let mut ordinary = NestedArchiveDetector::default();
        assert!(!ordinary.push(b"ordinary mustard ustar CD001 koly export data"));
        assert!(!ordinary.finish());
    }

    #[test]
    fn canonical_path_policy_is_nfc_unicode_casefold_and_separator_stable() {
        assert_eq!(
            validate_path(0, "folder//cafe\u{301}.json".as_bytes(), false).expect("path"),
            "folder/caf\u{e9}.json"
        );
        assert_eq!(path_collision_key("Folder/A.JSON"), "folder/a.json");
        assert_eq!(
            path_collision_key("\u{c4}.JSON"),
            path_collision_key("\u{e4}.json")
        );
        assert_eq!(path_collision_key("Stra\u{df}e.json"), "strasse.json");
        assert_eq!(
            validate_path(0, b"bad\0.json", false),
            Err(ScanError::InvalidPath {
                index: 0,
                violation: PathViolation::Control,
            })
        );
    }

    #[test]
    fn cancellation_timeout_and_rss_boundaries_fail_closed() {
        for (monitor, expected) in [
            (
                FixedMonitor {
                    cancelled: true,
                    elapsed: Duration::ZERO,
                    rss_bytes: Some(1),
                },
                ScanError::Cancelled,
            ),
            (
                FixedMonitor {
                    cancelled: false,
                    elapsed: FrozenLimits::MAX_WALL_TIME + Duration::from_nanos(1),
                    rss_bytes: Some(1),
                },
                ScanError::WallClockExceeded,
            ),
            (
                FixedMonitor {
                    cancelled: false,
                    elapsed: Duration::ZERO,
                    rss_bytes: Some(FrozenLimits::MAX_RSS_BYTES + 1),
                },
                ScanError::RssExceeded,
            ),
            (
                FixedMonitor {
                    cancelled: false,
                    elapsed: Duration::ZERO,
                    rss_bytes: None,
                },
                ScanError::RssUnavailable,
            ),
        ] {
            assert_eq!(check_budget(&monitor), Err(expected));
        }
    }

    #[test]
    fn darwin_rss_probe_reports_monotonic_high_water() {
        let before = process_memory_high_water(std::process::id()).expect("RSS");
        let mut allocation = vec![0_u8; 4 * 1_048_576];
        allocation
            .iter_mut()
            .step_by(4_096)
            .for_each(|byte| *byte = 1);
        let during = process_memory_high_water(std::process::id()).expect("RSS");
        drop(allocation);
        let after = process_memory_high_water(std::process::id()).expect("RSS");
        assert!(before > 0);
        assert!(during >= before);
        assert!(after >= during);
    }

    #[test]
    fn hard_supervisor_kills_on_wall_and_rss_boundaries() {
        let cancellation = CancellationToken::new();
        let mut wall_command = Command::new("/bin/sleep");
        wall_command
            .arg("60")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let wall_budget = SupervisorBudget {
            wall_time: Duration::from_millis(20),
            rss_bytes: FrozenLimits::MAX_RSS_BYTES,
            protocol_bytes: 1_024,
            poll_interval: Duration::from_millis(1),
        };
        assert_eq!(
            supervise_command(
                &mut wall_command,
                &cancellation,
                wall_budget,
                &mut FixedProbe(Some(1))
            ),
            Err(ScanError::WallClockExceeded)
        );

        let mut rss_command = Command::new("/bin/sleep");
        rss_command
            .arg("60")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        assert_eq!(
            supervise_command(
                &mut rss_command,
                &cancellation,
                wall_budget,
                &mut FixedProbe(Some(FrozenLimits::MAX_RSS_BYTES + 1))
            ),
            Err(ScanError::RssExceeded)
        );

        let live_cancellation = CancellationToken::new();
        let cancellation_handle = live_cancellation.clone();
        let canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            cancellation_handle.cancel();
        });
        let mut cancellation_command = Command::new("/bin/sleep");
        cancellation_command
            .arg("60")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        assert_eq!(
            supervise_command(
                &mut cancellation_command,
                &live_cancellation,
                SupervisorBudget {
                    wall_time: Duration::from_secs(1),
                    ..wall_budget
                },
                &mut FixedProbe(Some(1))
            ),
            Err(ScanError::Cancelled)
        );
        canceller.join().expect("canceller");

        let mut overflow_command = Command::new("/usr/bin/printf");
        let overflow_payload = format!(
            "{}{}",
            std::str::from_utf8(WORKER_READY).expect("ready is UTF-8"),
            "x".repeat(1_024)
        );
        overflow_command
            .arg(overflow_payload)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        assert_eq!(
            supervise_command(
                &mut overflow_command,
                &cancellation,
                SupervisorBudget {
                    wall_time: Duration::from_secs(1),
                    protocol_bytes: 128,
                    ..wall_budget
                },
                &mut FixedProbe(Some(1))
            ),
            Err(ScanError::WorkerOutputExceeded)
        );
    }

    #[test]
    fn unnamed_snapshot_remains_content_bound_after_source_replacement() {
        let directory = tempfile::tempdir().expect("directory");
        let path = directory.path().join("source.zip");
        let file = File::create(&path).expect("archive");
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("conversations.json", SimpleFileOptions::default())
            .expect("entry");
        writer.write_all(b"[]").expect("body");
        writer.finish().expect("finish");
        let original = fs::read(&path).expect("original");
        let expected_hash: [u8; 32] = Sha256::digest(&original).into();
        let monitor = FixedMonitor {
            cancelled: false,
            elapsed: Duration::ZERO,
            rss_bytes: Some(1),
        };
        let snapshot = capture_snapshot(&path, &monitor).expect("snapshot");
        assert_eq!(snapshot.file.metadata().expect("metadata").nlink(), 0);
        fs::write(&path, b"replacement").expect("replace source");

        let catalog = scan_immutable_snapshot(snapshot, &monitor).expect("catalog");
        assert_eq!(catalog.archive_sha256, expected_hash);
        assert_eq!(catalog.entries[0].path, "conversations.json");
    }
}
