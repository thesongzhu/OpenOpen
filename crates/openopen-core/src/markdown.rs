//! Descriptor-bound access to the user's Markdown continuity root.
//!
//! Reads are manifest-bound. Renders are command-owned and can only publish a
//! Store-created intent through a same-directory no-clobber commit. A target
//! that already exists is never overwritten here: it enters reconciliation.

use openopen_protocol::{
    DocumentManifest, DocumentManifestEntry, MarkdownBaseIdentity, MarkdownRenderIntent,
    MarkdownRenderReceipt,
};
use rustix::fs::{
    AtFlags, Dir, FileType, Mode, OFlags, RenameFlags, fstat, fsync, getpath, linkat, open, openat,
    renameat_with, unlinkat,
};
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Maximum plaintext byte count accepted for an individual manifest entry.
pub const MAX_MARKDOWN_DOCUMENT_BYTES: u64 = 512 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

impl FileIdentity {
    fn from_fd(fd: &impl AsFd) -> Result<Self, MarkdownBoundaryError> {
        let stat = fstat(fd).map_err(|_| MarkdownBoundaryError::RootBoundary)?;
        Ok(Self {
            device: u64::try_from(stat.st_dev).map_err(|_| MarkdownBoundaryError::RootBoundary)?,
            inode: stat.st_ino,
        })
    }
}

/// A bounded plaintext document read through the immutable manifest identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VerifiedMarkdownDocument {
    pub relative_path: String,
    pub text: String,
}

/// Fail-closed filesystem boundary failures. These values are intentionally
/// coarse so a caller cannot expose filesystem internals in product UI.
#[allow(dead_code)] // The private reader is retained for manifest-scoped recovery validation.
#[derive(Debug, Error)]
pub(crate) enum MarkdownBoundaryError {
    #[error("Markdown root boundary failed")]
    RootBoundary,
    #[error("Markdown manifest is invalid")]
    InvalidManifest,
    #[error("Markdown entry boundary failed")]
    EntryBoundary,
    #[error("Markdown entry content does not match its manifest")]
    ContentMismatch,
    #[error("Markdown entry is not plaintext")]
    NonPlaintext,
    #[error("Markdown render intent is invalid")]
    InvalidRenderIntent,
    #[error("Markdown render requires reconciliation")]
    ReconciliationRequired,
    #[error("Markdown render persistence failed")]
    RenderPersistence,
}

/// The Host never overwrites a Markdown path. A conflict leaves the owner's
/// existing file untouched and is deliberately not a receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MarkdownRenderOutcome {
    Committed(MarkdownRenderReceipt),
    ReconciliationRequired,
}

/// Descriptor-pinned, read-only access to one exact private Markdown root.
///
/// The root is opened only when it is an absolute, canonical, current-user
/// directory with exact `0700` mode. Every component and file is later opened
/// relative to that descriptor with `NOFOLLOW`, checked against its directory
/// entry inode, and bound back to the expected path before content is read.
pub(crate) struct MarkdownRoot {
    root: PathBuf,
    identity: FileIdentity,
}

impl MarkdownRoot {
    /// Opens the exact user-owned `~/Documents/OpenOpen` root. Creation and
    /// repair are deliberately outside this read-only boundary.
    ///
    /// # Errors
    ///
    /// Returns an error unless the root is an exact private directory whose
    /// descriptor and canonical path remain stable.
    pub(crate) fn open(root: &Path) -> Result<Self, MarkdownBoundaryError> {
        if !root.is_absolute() {
            return Err(MarkdownBoundaryError::RootBoundary);
        }
        let metadata =
            std::fs::symlink_metadata(root).map_err(|_| MarkdownBoundaryError::RootBoundary)?;
        let canonical =
            std::fs::canonicalize(root).map_err(|_| MarkdownBoundaryError::RootBoundary)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_dir()
            || canonical != root
            || !metadata_is_private_directory(&metadata)
        {
            return Err(MarkdownBoundaryError::RootBoundary);
        }
        let descriptor = open_exact_directory_path(root)?;
        Ok(Self {
            root: root.to_path_buf(),
            identity: FileIdentity::from_fd(&descriptor)?,
        })
    }

    /// Validates and reads exactly the manifest-listed plaintext files.
    ///
    /// A malformed manifest, path replacement, symlink, hardlink, permission
    /// change, size mismatch, binary payload, or digest mismatch fails closed.
    ///
    /// # Errors
    ///
    /// Returns an error when the manifest or any filesystem boundary cannot
    /// be verified without reading outside the exact private root.
    #[allow(dead_code)] // Used by private recovery paths and adversarial unit coverage.
    pub(crate) fn read_manifest(
        &self,
        manifest: &DocumentManifest,
    ) -> Result<Vec<VerifiedMarkdownDocument>, MarkdownBoundaryError> {
        if !manifest.is_valid() || manifest.entries.len() > 256 {
            return Err(MarkdownBoundaryError::InvalidManifest);
        }
        let mut entries = manifest.entries.iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        let documents = entries
            .into_iter()
            .map(|entry| self.read_entry(entry))
            .collect::<Result<Vec<_>, _>>()?;
        // A one-entry manifest would otherwise have no subsequent per-entry
        // root-open check to detect a replacement racing its read.
        self.open_root()?;
        Ok(documents)
    }

    /// Returns the descriptor-pinned identity of the current target, if one
    /// exists. This is deliberately Host-only input to a render intent: it
    /// prevents a production replacement from falling back to a test-only
    /// `expected_base` or treating an owner edit as an overwrite opportunity.
    ///
    /// # Errors
    ///
    /// Returns an error when the exact private root or existing entry cannot
    /// be proven regular, private, descriptor-pinned, and manifest-safe.
    pub(crate) fn observe_existing_entry(
        &self,
        relative_path: &str,
    ) -> Result<Option<MarkdownBaseIdentity>, MarkdownBoundaryError> {
        if !is_safe_relative_path(relative_path) {
            return Err(MarkdownBoundaryError::EntryBoundary);
        }
        let provisional = DocumentManifestEntry {
            relative_path: relative_path.to_owned(),
            sha256: "0".repeat(64),
            byte_length: 0,
            mode: 0o600,
        };
        let (directory, file_name, expected_path) = self.open_parent_for_entry(&provisional)?;
        let Some(expected_inode) = exact_entry_inode(&directory, OsStr::new(file_name))? else {
            return Ok(None);
        };
        let file = openat(
            &directory,
            file_name,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(|_| MarkdownBoundaryError::EntryBoundary)?;
        let identity = FileIdentity::from_fd(&file)?;
        if identity.inode != expected_inode
            || !fd_path_is_exact(&file, &expected_path)?
            || !private_regular_file_matches_manifest(&file)?
        {
            return Err(MarkdownBoundaryError::EntryBoundary);
        }
        let mut file = File::from(file);
        let mut bytes = Vec::with_capacity(4096);
        (&mut file)
            .take(MAX_MARKDOWN_DOCUMENT_BYTES.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|_| MarkdownBoundaryError::ContentMismatch)?;
        if bytes.len() as u64 > MAX_MARKDOWN_DOCUMENT_BYTES
            || FileIdentity::from_fd(&file)? != identity
            || exact_entry_inode(&directory, OsStr::new(file_name))? != Some(identity.inode)
            || !fd_path_is_exact(&file, &expected_path)?
            || !private_regular_file_matches_manifest(&file)?
            || String::from_utf8(bytes.clone()).is_err()
            || bytes.contains(&0)
        {
            return Err(MarkdownBoundaryError::ContentMismatch);
        }
        Ok(Some(MarkdownBaseIdentity {
            entry: DocumentManifestEntry {
                relative_path: relative_path.to_owned(),
                sha256: sha256_hex(&bytes),
                byte_length: bytes.len() as u64,
                mode: 0o600,
            },
            device: identity.device,
            inode: identity.inode,
        }))
    }

    /// Publishes a single Store-owned Markdown render intent without replacing
    /// an existing path. The encrypted body is supplied only by the private
    /// Host command that already authenticated the intent; callers cannot use
    /// this function as a raw Markdown writer because every byte is bound to
    /// the exact manifest digest in `intent`.
    ///
    /// If a prior exact receipt is supplied, the final descriptor is re-read
    /// and returned idempotently. Any existing path without that exact receipt
    /// becomes typed reconciliation rather than an overwrite.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid intent/body data, descriptor/path safety
    /// failure, durability failure, or a final inode/digest mismatch. Existing
    /// nonmatching content returns typed reconciliation instead of permitting
    /// an overwrite retry.
    #[allow(clippy::too_many_lines)] // The descriptor/open/sync/publication checks must remain visibly ordered.
    pub(crate) fn render_no_clobber(
        &self,
        intent: &MarkdownRenderIntent,
        content: &[u8],
        prior_receipt: Option<&MarkdownRenderReceipt>,
        committed_at_ms: i64,
    ) -> Result<MarkdownRenderOutcome, MarkdownBoundaryError> {
        if !intent.is_valid()
            || committed_at_ms < intent.created_at_ms
            || content.len() as u64 != intent.entry.byte_length
            || u64::try_from(content.len())
                .map_or(true, |length| length > MAX_MARKDOWN_DOCUMENT_BYTES)
            || sha256_hex(content) != intent.content_digest
            || String::from_utf8(content.to_vec()).is_err()
            || content.contains(&0)
        {
            return Err(MarkdownBoundaryError::InvalidRenderIntent);
        }
        let (directory, file_name, expected_path) = self.open_parent_for_entry(&intent.entry)?;
        let replacing_base = if let Some(existing_inode) =
            exact_entry_inode(&directory, OsStr::new(file_name))?
        {
            if let Some(receipt) = prior_receipt
                && receipt_matches_intent(receipt, intent, existing_inode, &directory, file_name)?
                && self.read_entry(&intent.entry).is_ok()
            {
                return Ok(MarkdownRenderOutcome::Committed(receipt.clone()));
            }
            let Some(expected_base) = intent.expected_base.as_ref() else {
                return Ok(MarkdownRenderOutcome::ReconciliationRequired);
            };
            // Re-open and validate the exact declared base before the atomic
            // exchange. A changed, partial, or concurrent owner edit never
            // becomes an overwrite target.
            if self.read_entry(&expected_base.entry).is_err()
                || !private_entry_matches(&directory, file_name, expected_base)?
            {
                return Ok(MarkdownRenderOutcome::ReconciliationRequired);
            }
            Some(expected_base.clone())
        } else {
            if intent.expected_base.is_some() {
                return Ok(MarkdownRenderOutcome::ReconciliationRequired);
            }
            None
        };

        let stage_name = stage_name(intent)?;
        if exact_entry_inode(&directory, OsStr::new(&stage_name))?.is_some() {
            return Ok(MarkdownRenderOutcome::ReconciliationRequired);
        }
        let stage = openat(
            &directory,
            &stage_name,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|_| MarkdownBoundaryError::RenderPersistence)?;
        let stage_identity = FileIdentity::from_fd(&stage)?;
        if !private_regular_file_matches_manifest(&stage)? {
            let _ = unlinkat(&directory, &stage_name, AtFlags::empty());
            return Err(MarkdownBoundaryError::RenderPersistence);
        }
        let mut stage_file = File::from(stage);
        if stage_file.write_all(content).is_err() || stage_file.sync_all().is_err() {
            drop(stage_file);
            let _ = unlinkat(&directory, &stage_name, AtFlags::empty());
            return Err(MarkdownBoundaryError::RenderPersistence);
        }
        if FileIdentity::from_fd(&stage_file)? != stage_identity {
            drop(stage_file);
            let _ = unlinkat(&directory, &stage_name, AtFlags::empty());
            return Err(MarkdownBoundaryError::RenderPersistence);
        }
        drop(stage_file);

        let displaced_base = if let Some(expected_base) = replacing_base {
            // macOS `RENAME_SWAP` is one directory-relative atomic exchange:
            // the current verified base moves to the private staging name and
            // remains there until the Store receipt is durable. A failed
            // exchange never produces an authoritative result.
            if renameat_with(
                &directory,
                &stage_name,
                &directory,
                file_name,
                RenameFlags::EXCHANGE,
            )
            .is_err()
                || !private_entry_matches(&directory, &stage_name, &expected_base)?
                || fsync(&directory).is_err()
            {
                return Ok(MarkdownRenderOutcome::ReconciliationRequired);
            }
            Some(expected_base)
        } else {
            // linkat is an atomic no-clobber publication: it fails if an owner
            // created the final name since preflight.
            if linkat(
                &directory,
                &stage_name,
                &directory,
                file_name,
                AtFlags::empty(),
            )
            .is_err()
            {
                let _ = unlinkat(&directory, &stage_name, AtFlags::empty());
                return Ok(MarkdownRenderOutcome::ReconciliationRequired);
            }
            if unlinkat(&directory, &stage_name, AtFlags::empty()).is_err()
                || fsync(&directory).is_err()
            {
                return Err(MarkdownBoundaryError::RenderPersistence);
            }
            None
        };
        let final_file = openat(
            &directory,
            file_name,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(|_| MarkdownBoundaryError::RenderPersistence)?;
        let final_identity = FileIdentity::from_fd(&final_file)?;
        if final_identity != stage_identity
            || exact_entry_inode(&directory, OsStr::new(file_name))? != Some(final_identity.inode)
            || !fd_path_is_exact(&final_file, &expected_path)?
            || !private_regular_file_matches_manifest(&final_file)?
            || self.read_entry(&intent.entry).is_err()
        {
            return Err(MarkdownBoundaryError::RenderPersistence);
        }
        Ok(MarkdownRenderOutcome::Committed(MarkdownRenderReceipt {
            intent_id: intent.id.clone(),
            final_entry: intent.entry.clone(),
            final_device: final_identity.device,
            final_inode: final_identity.inode,
            displaced_base,
            committed_at_ms,
        }))
    }

    /// Removes the private retained base only after its exact receipt is
    /// durable in the Store.  The caller supplies no path: the deterministic
    /// stage name, final entry, and displaced-base identity all come from the
    /// command-owned intent and receipt. A missing retained base is an
    /// idempotent success only after this exact receipt has committed: a
    /// crash can occur after unlink+fsync and before Store body retirement.
    /// A changed or recreated base is reconciliation, never best-effort
    /// cleanup.
    ///
    /// # Errors
    ///
    /// Returns typed reconciliation for an ambiguous or externally changed
    /// retained base and persistence failure for an unlink or directory-sync
    /// failure.
    pub(crate) fn cleanup_displaced_base(
        &self,
        intent: &MarkdownRenderIntent,
        receipt: &MarkdownRenderReceipt,
    ) -> Result<(), MarkdownBoundaryError> {
        let Some(expected_base) = receipt.displaced_base.as_ref() else {
            return Ok(());
        };
        if !intent.is_valid()
            || !receipt.is_valid()
            || receipt.intent_id != intent.id
            || receipt.final_entry != intent.entry
            || intent.expected_base.as_ref() != Some(expected_base)
        {
            return Err(MarkdownBoundaryError::InvalidRenderIntent);
        }
        let (directory, _file_name, _expected_path) = self.open_parent_for_entry(&intent.entry)?;
        let retained_name = stage_name(intent)?;
        match exact_entry_inode(&directory, OsStr::new(&retained_name))? {
            None => return Ok(()),
            Some(_) if !private_entry_matches(&directory, &retained_name, expected_base)? => {
                return Err(MarkdownBoundaryError::ReconciliationRequired);
            }
            Some(_) => {}
        }
        unlinkat(&directory, &retained_name, AtFlags::empty())
            .map_err(|_| MarkdownBoundaryError::RenderPersistence)?;
        fsync(&directory).map_err(|_| MarkdownBoundaryError::RenderPersistence)
    }

    /// Re-opens the exact published entry before a receipt-recovery cleanup.
    /// A durable receipt is not permission to retire encrypted bodies if an
    /// Owner edit, deletion, or replacement happened after publication.
    ///
    /// # Errors
    ///
    /// Returns an error if the intent, receipt, final entry, or retained base
    /// cannot be re-opened and verified without ambiguity.
    pub(crate) fn verify_committed_receipt(
        &self,
        intent: &MarkdownRenderIntent,
        receipt: &MarkdownRenderReceipt,
    ) -> Result<(), MarkdownBoundaryError> {
        if !intent.is_valid()
            || !receipt.is_valid()
            || receipt.intent_id != intent.id
            || receipt.final_entry != intent.entry
        {
            return Err(MarkdownBoundaryError::InvalidRenderIntent);
        }
        let (directory, file_name, _) = self.open_parent_for_entry(&intent.entry)?;
        let Some(_) = exact_entry_inode(&directory, OsStr::new(file_name))? else {
            return Err(MarkdownBoundaryError::ReconciliationRequired);
        };
        let final_file = openat(
            &directory,
            OsStr::new(file_name),
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(|_| MarkdownBoundaryError::ReconciliationRequired)?;
        let identity = FileIdentity::from_fd(&final_file)?;
        if identity.device != receipt.final_device || identity.inode != receipt.final_inode {
            return Err(MarkdownBoundaryError::ReconciliationRequired);
        }
        self.read_entry(&intent.entry)?;
        Ok(())
    }

    /// Recovers only the exact fresh-file publication whose bytes already
    /// match the command-owned intent. This closes the crash window after the
    /// directory fsync and before the encrypted Store receipt commits. It is
    /// deliberately unavailable for replacement/CAS renders because those
    /// also require the displaced-base identity from the original exchange.
    pub(crate) fn recover_exact_fresh_publication(
        &self,
        intent: &MarkdownRenderIntent,
        committed_at_ms: i64,
    ) -> Result<Option<MarkdownRenderReceipt>, MarkdownBoundaryError> {
        if !intent.is_valid()
            || intent.expected_base.is_some()
            || committed_at_ms < intent.created_at_ms
        {
            return Err(MarkdownBoundaryError::InvalidRenderIntent);
        }
        let (directory, file_name, expected_path) = self.open_parent_for_entry(&intent.entry)?;
        let Some(expected_inode) = exact_entry_inode(&directory, OsStr::new(file_name))? else {
            return Ok(None);
        };
        let file = openat(
            &directory,
            file_name,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(|_| MarkdownBoundaryError::ReconciliationRequired)?;
        let identity = FileIdentity::from_fd(&file)?;
        if identity.inode != expected_inode
            || !fd_path_is_exact(&file, &expected_path)?
            || !private_regular_file_matches_manifest(&file)?
            || self.read_entry(&intent.entry).is_err()
        {
            return Ok(None);
        }
        Ok(Some(MarkdownRenderReceipt {
            intent_id: intent.id.clone(),
            final_entry: intent.entry.clone(),
            final_device: identity.device,
            final_inode: identity.inode,
            displaced_base: None,
            committed_at_ms,
        }))
    }

    fn open_root(&self) -> Result<OwnedFd, MarkdownBoundaryError> {
        let descriptor = open_exact_directory_path(&self.root)?;
        if FileIdentity::from_fd(&descriptor)? != self.identity
            || !fd_path_is_exact(&descriptor, &self.root)?
        {
            return Err(MarkdownBoundaryError::RootBoundary);
        }
        Ok(descriptor)
    }

    fn read_entry(
        &self,
        entry: &DocumentManifestEntry,
    ) -> Result<VerifiedMarkdownDocument, MarkdownBoundaryError> {
        let components = entry.relative_path.split('/').collect::<Vec<_>>();
        let (file_name, parents) = components
            .split_last()
            .ok_or(MarkdownBoundaryError::EntryBoundary)?;
        let mut directory = self.open_root()?;
        let mut expected_path = self.root.clone();
        for component in parents {
            expected_path.push(*component);
            directory = open_exact_child_directory(&directory, component, &expected_path)?;
        }

        expected_path.push(*file_name);
        let expected_inode = exact_entry_inode(&directory, OsStr::new(*file_name))?
            .ok_or(MarkdownBoundaryError::EntryBoundary)?;
        let file = openat(
            &directory,
            *file_name,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(|_| MarkdownBoundaryError::EntryBoundary)?;
        let identity = FileIdentity::from_fd(&file)?;
        if identity.inode != expected_inode
            || !fd_path_is_exact(&file, &expected_path)?
            || !private_regular_file_matches_manifest(&file)?
        {
            return Err(MarkdownBoundaryError::EntryBoundary);
        }

        let mut bytes = Vec::with_capacity(
            usize::try_from(entry.byte_length)
                .map_err(|_| MarkdownBoundaryError::ContentMismatch)?,
        );
        let mut file = File::from(file);
        // A file can grow after descriptor and manifest checks. Limit this
        // read to one byte beyond the declared length so that race cannot
        // turn a bounded Markdown scan into an unbounded allocation.
        let read_limit = entry
            .byte_length
            .checked_add(1)
            .ok_or(MarkdownBoundaryError::ContentMismatch)?;
        (&mut file)
            .take(read_limit)
            .read_to_end(&mut bytes)
            .map_err(|_| MarkdownBoundaryError::ContentMismatch)?;
        if u64::try_from(bytes.len()).map_err(|_| MarkdownBoundaryError::ContentMismatch)?
            != entry.byte_length
            || entry.byte_length > MAX_MARKDOWN_DOCUMENT_BYTES
            || FileIdentity::from_fd(&file)? != identity
            || exact_entry_inode(&directory, OsStr::new(*file_name))? != Some(identity.inode)
            || !fd_path_is_exact(&file, &expected_path)?
            || !private_regular_file_matches_manifest(&file)?
            || sha256_hex(&bytes) != entry.sha256
        {
            return Err(MarkdownBoundaryError::ContentMismatch);
        }
        let text = String::from_utf8(bytes).map_err(|_| MarkdownBoundaryError::NonPlaintext)?;
        if text.contains('\0') {
            return Err(MarkdownBoundaryError::NonPlaintext);
        }
        Ok(VerifiedMarkdownDocument {
            relative_path: entry.relative_path.clone(),
            text,
        })
    }

    fn open_parent_for_entry<'a>(
        &self,
        entry: &'a DocumentManifestEntry,
    ) -> Result<(OwnedFd, &'a str, PathBuf), MarkdownBoundaryError> {
        let components = entry.relative_path.split('/').collect::<Vec<_>>();
        let (file_name, parents) = components
            .split_last()
            .ok_or(MarkdownBoundaryError::EntryBoundary)?;
        let mut directory = self.open_root()?;
        let mut expected_path = self.root.clone();
        for component in parents {
            expected_path.push(*component);
            directory = open_exact_child_directory(&directory, component, &expected_path)?;
        }
        expected_path.push(*file_name);
        Ok((directory, file_name, expected_path))
    }
}

fn is_safe_relative_path(relative_path: &str) -> bool {
    !relative_path.is_empty()
        && relative_path.len() <= 512
        && relative_path.split('/').all(|component| {
            !component.is_empty()
                && component != "."
                && component != ".."
                && component.len() <= 255
                && !component.contains('\0')
        })
}

fn private_entry_matches(
    directory: &impl AsFd,
    name: &str,
    expected: &MarkdownBaseIdentity,
) -> Result<bool, MarkdownBoundaryError> {
    let file = openat(
        directory,
        name,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(|_| MarkdownBoundaryError::EntryBoundary)?;
    if FileIdentity::from_fd(&file)?
        != (FileIdentity {
            device: expected.device,
            inode: expected.inode,
        })
        || !private_regular_file_matches_manifest(&file)?
    {
        return Ok(false);
    }
    let mut file = File::from(file);
    let mut bytes = Vec::with_capacity(
        usize::try_from(expected.entry.byte_length)
            .map_err(|_| MarkdownBoundaryError::ContentMismatch)?,
    );
    (&mut file)
        .take(
            expected
                .entry
                .byte_length
                .checked_add(1)
                .ok_or(MarkdownBoundaryError::ContentMismatch)?,
        )
        .read_to_end(&mut bytes)
        .map_err(|_| MarkdownBoundaryError::ContentMismatch)?;
    Ok(
        u64::try_from(bytes.len()).ok() == Some(expected.entry.byte_length)
            && sha256_hex(&bytes) == expected.entry.sha256,
    )
}

fn stage_name(intent: &MarkdownRenderIntent) -> Result<String, MarkdownBoundaryError> {
    let name = format!(".openopen-stage-{}", intent.id);
    (name.len() <= 255 && !name.contains('/') && !name.contains('\0'))
        .then_some(name)
        .ok_or(MarkdownBoundaryError::InvalidRenderIntent)
}

fn receipt_matches_intent(
    receipt: &MarkdownRenderReceipt,
    intent: &MarkdownRenderIntent,
    inode: u64,
    directory: &impl AsFd,
    file_name: &str,
) -> Result<bool, MarkdownBoundaryError> {
    if !receipt.is_valid()
        || receipt.intent_id != intent.id
        || receipt.final_entry != intent.entry
        || receipt.final_inode != inode
    {
        return Ok(false);
    }
    let file = openat(
        directory,
        file_name,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(|_| MarkdownBoundaryError::EntryBoundary)?;
    if FileIdentity::from_fd(&file)?.device != receipt.final_device
        || FileIdentity::from_fd(&file)?.inode != receipt.final_inode
        || !private_regular_file_matches_manifest(&file)?
    {
        return Ok(false);
    }
    if let Some(expected_base) = receipt.displaced_base.as_ref() {
        let retained_name = stage_name(intent)?;
        return private_entry_matches(directory, &retained_name, expected_base);
    }
    Ok(true)
}

fn metadata_is_private_directory(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    metadata.uid() == rustix::process::geteuid().as_raw()
        && metadata.permissions().mode() & 0o777 == 0o700
}

fn open_exact_directory_path(path: &Path) -> Result<OwnedFd, MarkdownBoundaryError> {
    let descriptor = open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|_| MarkdownBoundaryError::RootBoundary)?;
    if !fd_path_is_exact(&descriptor, path)? || !private_directory(&descriptor)? {
        return Err(MarkdownBoundaryError::RootBoundary);
    }
    Ok(descriptor)
}

fn open_exact_child_directory(
    parent: &impl AsFd,
    name: &str,
    expected_path: &Path,
) -> Result<OwnedFd, MarkdownBoundaryError> {
    let expected_inode =
        exact_entry_inode(parent, OsStr::new(name))?.ok_or(MarkdownBoundaryError::EntryBoundary)?;
    let directory = openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|_| MarkdownBoundaryError::EntryBoundary)?;
    if FileIdentity::from_fd(&directory)?.inode != expected_inode
        || !fd_path_is_exact(&directory, expected_path)?
        || !private_directory(&directory)?
    {
        return Err(MarkdownBoundaryError::EntryBoundary);
    }
    Ok(directory)
}

fn exact_entry_inode(
    directory: &impl AsFd,
    name: &OsStr,
) -> Result<Option<u64>, MarkdownBoundaryError> {
    let mut entries =
        Dir::read_from(directory).map_err(|_| MarkdownBoundaryError::EntryBoundary)?;
    for entry in &mut entries {
        let entry = entry.map_err(|_| MarkdownBoundaryError::EntryBoundary)?;
        if entry.file_name().to_bytes() == name.as_bytes() {
            return Ok(Some(entry.ino()));
        }
    }
    Ok(None)
}

fn private_directory(directory: &impl AsFd) -> Result<bool, MarkdownBoundaryError> {
    let stat = fstat(directory).map_err(|_| MarkdownBoundaryError::EntryBoundary)?;
    Ok(FileType::from_raw_mode(stat.st_mode) == FileType::Directory
        && stat.st_uid == rustix::process::geteuid().as_raw()
        && stat.st_mode & 0o777 == 0o700)
}

fn private_regular_file_matches_manifest(file: &impl AsFd) -> Result<bool, MarkdownBoundaryError> {
    let stat = fstat(file).map_err(|_| MarkdownBoundaryError::EntryBoundary)?;
    Ok(
        FileType::from_raw_mode(stat.st_mode) == FileType::RegularFile
            && stat.st_uid == rustix::process::geteuid().as_raw()
            && stat.st_mode & 0o777 == 0o600
            && stat.st_nlink == 1,
    )
}

fn fd_path_is_exact(fd: &impl AsFd, expected_path: &Path) -> Result<bool, MarkdownBoundaryError> {
    let actual = getpath(fd).map_err(|_| MarkdownBoundaryError::EntryBoundary)?;
    Ok(Path::new(OsStr::from_bytes(actual.to_bytes())) == expected_path)
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::{MarkdownBoundaryError, MarkdownRenderOutcome, MarkdownRoot};
    use openopen_protocol::{
        DocumentManifest, DocumentManifestEntry, MarkdownRenderIntent,
        canonical_document_manifest_digest,
    };
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use std::path::Path;
    use std::process::Command;

    fn private_directory(path: &Path) {
        fs::create_dir_all(path).expect("create private directory");
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .expect("set private directory mode");
    }

    fn canonical_temporary_root(temporary: &tempfile::TempDir) -> std::path::PathBuf {
        fs::canonicalize(temporary.path())
            .expect("canonicalize temporary root")
            .join("OpenOpen")
    }

    fn manifest_for(root: &Path, relative_path: &str) -> DocumentManifest {
        let path = root.join(relative_path);
        let bytes = fs::read(path).expect("read fixture");
        let entry = DocumentManifestEntry {
            relative_path: relative_path.to_owned(),
            sha256: super::sha256_hex(&bytes),
            byte_length: u64::try_from(bytes.len()).expect("fixture length"),
            mode: 0o600,
        };
        DocumentManifest {
            root_version: 1,
            aggregate_digest: canonical_document_manifest_digest(std::slice::from_ref(&entry))
                .expect("canonical digest"),
            entries: vec![entry],
            generated_at_ms: 1,
        }
    }

    fn write_document(root: &Path, relative_path: &str, content: &[u8]) {
        let path = root.join(relative_path);
        let mut directory = root.to_path_buf();
        for component in Path::new(relative_path)
            .parent()
            .expect("parent")
            .components()
        {
            directory.push(component.as_os_str());
            private_directory(&directory);
        }
        fs::write(&path, content).expect("write document");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("set document mode");
    }

    fn render_intent(relative_path: &str, content: &[u8]) -> MarkdownRenderIntent {
        MarkdownRenderIntent {
            id: "markdown-render-1".to_owned(),
            choice_session_id: "choice-session-1".to_owned(),
            expected_session_revision: 1,
            expected_generation: 1,
            entry: DocumentManifestEntry {
                relative_path: relative_path.to_owned(),
                sha256: super::sha256_hex(content),
                byte_length: content.len() as u64,
                mode: 0o600,
            },
            expected_base: None,
            content_digest: super::sha256_hex(content),
            created_at_ms: 1,
        }
    }

    #[test]
    fn reads_only_exact_private_manifest_file() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = canonical_temporary_root(&temporary);
        private_directory(&root);
        write_document(&root, "tasks/demo/OVERVIEW.md", b"# Demo\n");
        let manifest = manifest_for(&root, "tasks/demo/OVERVIEW.md");

        let documents = MarkdownRoot::open(&root)
            .expect("open root")
            .read_manifest(&manifest)
            .expect("read exact manifest");
        assert_eq!(documents.len(), 1);
        assert_eq!(documents[0].text, "# Demo\n");
    }

    #[test]
    fn reads_manifest_documents_in_canonical_path_order() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = canonical_temporary_root(&temporary);
        private_directory(&root);
        write_document(&root, "tasks/demo/OVERVIEW.md", b"# Overview\n");
        write_document(&root, "tasks/demo/STATE.md", b"# State\n");
        let mut overview_manifest = manifest_for(&root, "tasks/demo/OVERVIEW.md");
        let overview = overview_manifest.entries.remove(0);
        let mut state_manifest = manifest_for(&root, "tasks/demo/STATE.md");
        let state = state_manifest.entries.remove(0);
        let entries = vec![state, overview];
        let manifest = DocumentManifest {
            root_version: 1,
            aggregate_digest: canonical_document_manifest_digest(&entries)
                .expect("canonical digest"),
            entries,
            generated_at_ms: 1,
        };

        let documents = MarkdownRoot::open(&root)
            .expect("open root")
            .read_manifest(&manifest)
            .expect("read deterministic manifest");
        assert_eq!(
            documents
                .iter()
                .map(|document| document.relative_path.as_str())
                .collect::<Vec<_>>(),
            ["tasks/demo/OVERVIEW.md", "tasks/demo/STATE.md"]
        );
    }

    #[test]
    fn rejects_symlink_hardlink_permissions_and_manifest_mismatch() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = canonical_temporary_root(&temporary);
        private_directory(&root);
        write_document(&root, "tasks/demo/OVERVIEW.md", b"# Demo\n");
        let manifest = manifest_for(&root, "tasks/demo/OVERVIEW.md");
        let file = root.join("tasks/demo/OVERVIEW.md");

        fs::remove_file(&file).expect("remove fixture");
        symlink("/tmp/not-openopen", &file).expect("make symlink");
        assert!(matches!(
            MarkdownRoot::open(&root)
                .expect("open root")
                .read_manifest(&manifest),
            Err(MarkdownBoundaryError::EntryBoundary)
        ));

        fs::remove_file(&file).expect("remove symlink");
        write_document(&root, "tasks/demo/OVERVIEW.md", b"# Demo\n");
        fs::hard_link(&file, root.join("tasks/demo/other.md")).expect("hard link");
        assert!(matches!(
            MarkdownRoot::open(&root)
                .expect("open root")
                .read_manifest(&manifest),
            Err(MarkdownBoundaryError::EntryBoundary)
        ));

        fs::remove_file(root.join("tasks/demo/other.md")).expect("remove hard link");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).expect("change file mode");
        assert!(matches!(
            MarkdownRoot::open(&root)
                .expect("open root")
                .read_manifest(&manifest),
            Err(MarkdownBoundaryError::EntryBoundary)
        ));
    }

    #[test]
    fn rejects_a_root_escape_through_a_symlinked_documents_root() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = canonical_temporary_root(&temporary);
        private_directory(&root);
        let alias = temporary.path().join("OpenOpen-alias");
        symlink(&root, &alias).expect("make root symlink");
        assert!(matches!(
            MarkdownRoot::open(&alias),
            Err(MarkdownBoundaryError::RootBoundary)
        ));
    }

    #[test]
    fn rejects_a_named_pipe_without_blocking_the_host_read_path() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = canonical_temporary_root(&temporary);
        private_directory(&root);
        write_document(&root, "tasks/demo/OVERVIEW.md", b"# Demo\n");
        let manifest = manifest_for(&root, "tasks/demo/OVERVIEW.md");
        let file = root.join("tasks/demo/OVERVIEW.md");
        fs::remove_file(&file).expect("remove regular fixture");
        let status = Command::new("/usr/bin/mkfifo")
            .arg(&file)
            .status()
            .expect("launch mkfifo fixture helper");
        assert!(status.success(), "create named pipe");

        assert!(matches!(
            MarkdownRoot::open(&root)
                .expect("open root")
                .read_manifest(&manifest),
            Err(MarkdownBoundaryError::EntryBoundary)
        ));
    }

    #[test]
    fn rejects_root_replacement_and_content_or_digest_mismatch() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = canonical_temporary_root(&temporary);
        private_directory(&root);
        write_document(&root, "tasks/demo/OVERVIEW.md", b"# Demo\n");
        let manifest = manifest_for(&root, "tasks/demo/OVERVIEW.md");
        let scanner = MarkdownRoot::open(&root).expect("open root");

        fs::write(root.join("tasks/demo/OVERVIEW.md"), b"# Changed\n").expect("change fixture");
        fs::set_permissions(
            root.join("tasks/demo/OVERVIEW.md"),
            fs::Permissions::from_mode(0o600),
        )
        .expect("restore file mode");
        assert!(matches!(
            scanner.read_manifest(&manifest),
            Err(MarkdownBoundaryError::ContentMismatch)
        ));

        fs::remove_dir_all(&root).expect("remove root");
        private_directory(&root);
        write_document(&root, "tasks/demo/OVERVIEW.md", b"# Demo\n");
        assert!(matches!(
            scanner.read_manifest(&manifest),
            Err(MarkdownBoundaryError::RootBoundary)
        ));
    }

    #[test]
    fn rejects_binary_markdown_and_oversize_manifest_entries_before_context_use() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = canonical_temporary_root(&temporary);
        private_directory(&root);
        write_document(&root, "tasks/demo/OVERVIEW.md", &[0xff, 0xfe]);
        let binary_manifest = manifest_for(&root, "tasks/demo/OVERVIEW.md");
        assert!(matches!(
            MarkdownRoot::open(&root)
                .expect("open root")
                .read_manifest(&binary_manifest),
            Err(MarkdownBoundaryError::NonPlaintext)
        ));

        let mut oversize_manifest = binary_manifest;
        oversize_manifest.entries[0].byte_length = super::MAX_MARKDOWN_DOCUMENT_BYTES + 1;
        assert!(matches!(
            MarkdownRoot::open(&root)
                .expect("open root")
                .read_manifest(&oversize_manifest),
            Err(MarkdownBoundaryError::InvalidManifest)
        ));
    }

    #[test]
    fn rejects_a_file_extended_after_its_bounded_manifest_was_created() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = canonical_temporary_root(&temporary);
        private_directory(&root);
        write_document(&root, "tasks/demo/OVERVIEW.md", b"# Demo\n");
        let manifest = manifest_for(&root, "tasks/demo/OVERVIEW.md");
        let expanded = vec![
            b'a';
            usize::try_from(super::MAX_MARKDOWN_DOCUMENT_BYTES + 1)
                .expect("bounded test length")
        ];
        write_document(&root, "tasks/demo/OVERVIEW.md", &expanded);
        assert!(matches!(
            MarkdownRoot::open(&root)
                .expect("open root")
                .read_manifest(&manifest),
            Err(MarkdownBoundaryError::ContentMismatch)
        ));
    }

    #[test]
    fn render_is_same_directory_private_no_clobber_and_exactly_replayable() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = canonical_temporary_root(&temporary);
        private_directory(&root);
        private_directory(&root.join("tasks"));
        private_directory(&root.join("tasks/demo"));
        let content = b"# Rendered\n";
        let intent = render_intent("tasks/demo/STATE.md", content);
        let scanner = MarkdownRoot::open(&root).expect("open root");

        let receipt = match scanner
            .render_no_clobber(&intent, content, None, 2)
            .expect("publish render")
        {
            MarkdownRenderOutcome::Committed(receipt) => receipt,
            MarkdownRenderOutcome::ReconciliationRequired => panic!("unexpected reconciliation"),
        };
        assert_eq!(fs::read(root.join("tasks/demo/STATE.md")).unwrap(), content);
        let metadata = fs::metadata(root.join("tasks/demo/STATE.md")).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        assert!(matches!(
            scanner
                .render_no_clobber(&intent, content, Some(&receipt), 3)
                .expect("exact replay"),
            MarkdownRenderOutcome::Committed(replayed) if replayed == receipt
        ));
        scanner
            .verify_committed_receipt(&intent, &receipt)
            .expect("the published receipt pins the exact final descriptor and digest");
        write_document(&root, "tasks/demo/STATE.md", b"# Owner edit\n");
        assert!(
            scanner.verify_committed_receipt(&intent, &receipt).is_err(),
            "an Owner edit after publication invalidates receipt recovery before cleanup"
        );
    }

    #[test]
    fn exact_fresh_publication_is_recoverable_after_receipt_commit_crash() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = canonical_temporary_root(&temporary);
        private_directory(&root);
        private_directory(&root.join("tasks"));
        private_directory(&root.join("tasks/demo"));
        let content = b"# Rendered\n";
        let intent = render_intent("tasks/demo/STATE.md", content);
        let scanner = MarkdownRoot::open(&root).expect("open root");

        let first = match scanner
            .render_no_clobber(&intent, content, None, 2)
            .expect("publish before simulated Store crash")
        {
            MarkdownRenderOutcome::Committed(receipt) => receipt,
            MarkdownRenderOutcome::ReconciliationRequired => panic!("unexpected reconciliation"),
        };
        let recovered = scanner
            .recover_exact_fresh_publication(&intent, 3)
            .expect("recover exact descriptor")
            .expect("exact final is recoverable");
        assert_eq!(recovered.intent_id, first.intent_id);
        assert_eq!(recovered.final_entry, first.final_entry);
        assert_eq!(recovered.final_device, first.final_device);
        assert_eq!(recovered.final_inode, first.final_inode);
        scanner
            .verify_committed_receipt(&intent, &recovered)
            .expect("recovered receipt is exact");

        write_document(&root, "tasks/demo/STATE.md", b"# Owner edit\n");
        assert!(
            scanner
                .recover_exact_fresh_publication(&intent, 4)
                .expect("typed mismatch")
                .is_none()
        );
    }

    #[test]
    fn host_observed_existing_entry_is_descriptor_bound_and_ready_for_cas() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = canonical_temporary_root(&temporary);
        private_directory(&root);
        private_directory(&root.join("tasks"));
        private_directory(&root.join("tasks/demo"));
        write_document(&root, "tasks/demo/STATE.md", b"# Existing\n");
        let scanner = MarkdownRoot::open(&root).expect("open root");
        let observed = scanner
            .observe_existing_entry("tasks/demo/STATE.md")
            .expect("observe exact base")
            .expect("existing entry");
        let manifest = manifest_for(&root, "tasks/demo/STATE.md");
        assert_eq!(observed.entry, manifest.entries[0]);
        assert!(
            scanner
                .observe_existing_entry("tasks/demo/MISSING.md")
                .expect("bounded absent observation")
                .is_none()
        );
        assert!(matches!(
            scanner.observe_existing_entry("../STATE.md"),
            Err(MarkdownBoundaryError::EntryBoundary)
        ));
    }

    #[test]
    fn render_exchanges_only_the_exact_verified_base_and_retains_it_for_receipt() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = canonical_temporary_root(&temporary);
        private_directory(&root);
        private_directory(&root.join("tasks"));
        private_directory(&root.join("tasks/demo"));
        let relative = "tasks/demo/STATE.md";
        let old = b"# Old\n";
        let replacement = b"# Replacement\n";
        write_document(&root, relative, old);
        let scanner = MarkdownRoot::open(&root).expect("open root");
        let base = scanner
            .observe_existing_entry(relative)
            .expect("observe exact base")
            .expect("existing base");
        let mut intent = render_intent(relative, replacement);
        intent.expected_base = Some(base.clone());

        let receipt = match scanner
            .render_no_clobber(&intent, replacement, None, 2)
            .expect("exchange exact base")
        {
            MarkdownRenderOutcome::Committed(receipt) => receipt,
            MarkdownRenderOutcome::ReconciliationRequired => panic!("unexpected reconciliation"),
        };
        assert_eq!(fs::read(root.join(relative)).unwrap(), replacement);
        assert_eq!(receipt.displaced_base, Some(base));
        assert!(
            root.join("tasks/demo")
                .join(format!(".openopen-stage-{}", intent.id))
                .is_file()
        );
        assert!(matches!(
            scanner
                .render_no_clobber(&intent, replacement, Some(&receipt), 3)
                .expect("exact exchanged replay"),
            MarkdownRenderOutcome::Committed(replayed) if replayed == receipt
        ));
        scanner
            .cleanup_displaced_base(&intent, &receipt)
            .expect("durable receipt permits cleanup");
        assert!(
            !root
                .join("tasks/demo")
                .join(format!(".openopen-stage-{}", intent.id))
                .exists()
        );

        write_document(&root, relative, old);
        let changed = b"# Changed\n";
        let mut stale = render_intent(relative, replacement);
        stale.expected_base = scanner
            .observe_existing_entry(relative)
            .expect("observe stale base");
        write_document(&root, relative, changed);
        assert!(matches!(
            scanner.render_no_clobber(&stale, replacement, None, 3),
            Ok(MarkdownRenderOutcome::ReconciliationRequired)
        ));
        assert_eq!(fs::read(root.join(relative)).unwrap(), changed);

        // Equal bytes are not an equal CAS base: an owner can replace the
        // inode without changing the visible text, and that still must not be
        // swapped away by the Host.
        write_document(&root, relative, old);
        let same_content_base = scanner
            .observe_existing_entry(relative)
            .expect("observe inode-pinned base")
            .expect("existing base");
        let target = root.join(relative);
        let displaced = root.join("tasks/demo/owner-copy.md");
        fs::rename(&target, &displaced).expect("move observed owner file");
        write_document(&root, relative, old);
        let mut same_content_replacement = render_intent(relative, replacement);
        same_content_replacement.expected_base = Some(same_content_base);
        assert!(matches!(
            scanner.render_no_clobber(&same_content_replacement, replacement, None, 4),
            Ok(MarkdownRenderOutcome::ReconciliationRequired)
        ));
        assert_eq!(fs::read(&target).unwrap(), old);
    }

    #[test]
    fn render_preserves_external_edit_and_partial_or_ambiguous_state_for_reconciliation() {
        let temporary = tempfile::tempdir().expect("tempdir");
        let root = canonical_temporary_root(&temporary);
        private_directory(&root);
        private_directory(&root.join("tasks"));
        private_directory(&root.join("tasks/demo"));
        let scanner = MarkdownRoot::open(&root).expect("open root");
        let intent = render_intent("tasks/demo/STATE.md", b"# New\n");
        write_document(&root, "tasks/demo/STATE.md", b"# Owner edit\n");
        assert!(matches!(
            scanner.render_no_clobber(&intent, b"# New\n", None, 2),
            Ok(MarkdownRenderOutcome::ReconciliationRequired)
        ));
        assert_eq!(
            fs::read(root.join("tasks/demo/STATE.md")).unwrap(),
            b"# Owner edit\n"
        );

        let mut partial = intent.clone();
        partial.entry.mode = 0o644;
        assert!(matches!(
            scanner.render_no_clobber(&partial, b"# New\n", None, 2),
            Err(MarkdownBoundaryError::InvalidRenderIntent)
        ));
    }
}
