use crate::{BrokerError, PayloadDescriptor};
use rustix::fs::{
    AtFlags, Dir, FileType, Mode, OFlags, fchmod, fstat, fsync, getpath, mkdirat, open, openat,
    renameat, statat, unlinkat,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs::{File, Metadata};
use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const PAYLOAD_NAME: &str = ".payload";
const STREAM_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FileIdentity {
    pub device: u64,
    pub inode: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommittedFile {
    pub sha256: String,
    pub byte_len: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InspectedFile {
    pub identity: FileIdentity,
    pub content: CommittedFile,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EffectFilesystemState {
    pub stage_directory_exists: bool,
    pub stage_payload: Option<InspectedFile>,
    pub final_file: Option<InspectedFile>,
}

pub(crate) struct Workspace {
    root: PathBuf,
    root_identity: FileIdentity,
    mission_identities: Mutex<HashMap<String, FileIdentity>>,
}

impl Workspace {
    pub(crate) fn open(root: &Path) -> Result<Self, BrokerError> {
        if !root.is_absolute() {
            return Err(BrokerError::InvalidRoot);
        }
        let metadata = std::fs::symlink_metadata(root).map_err(|_| BrokerError::InvalidRoot)?;
        let canonical = std::fs::canonicalize(root).map_err(|_| BrokerError::InvalidRoot)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_dir()
            || canonical != root
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.permissions().mode() & 0o777 != 0o700
        {
            return Err(BrokerError::InvalidRoot);
        }
        Ok(Self {
            root: root.to_path_buf(),
            root_identity: FileIdentity::from_metadata(&metadata),
            mission_identities: Mutex::new(HashMap::new()),
        })
    }

    pub(crate) fn ensure_workspace(&self, mission_id: &str) -> Result<FileIdentity, BrokerError> {
        let root = self.open_root()?;
        let path = self.root.join(mission_id);
        if exact_entry_inode(&root, OsStr::new(mission_id))?.is_none() {
            mkdirat(&root, mission_id, Mode::RUSR | Mode::WUSR | Mode::XUSR)
                .map_err(|_| BrokerError::WorkspaceBoundary)?;
            fsync(&root).map_err(|_| BrokerError::WorkspaceBoundary)?;
        }
        let directory = open_exact_directory(&root, OsStr::new(mission_id), &path)?;
        let identity = FileIdentity::from_fd(&directory)?;
        let stat = fstat(&directory).map_err(|_| BrokerError::WorkspaceBoundary)?;
        if stat.st_mode & 0o777 != 0o700 {
            return Err(BrokerError::WorkspaceBoundary);
        }
        let mut identities = self
            .mission_identities
            .lock()
            .map_err(|_| BrokerError::WorkspaceBoundary)?;
        match identities.get(mission_id) {
            Some(expected) if *expected != identity => Err(BrokerError::WorkspaceBoundary),
            Some(_) => Ok(identity),
            None => {
                identities.insert(mission_id.to_owned(), identity);
                Ok(identity)
            }
        }
    }

    pub(crate) fn require_workspace_identity(
        &self,
        mission_id: &str,
        expected: FileIdentity,
    ) -> Result<(), BrokerError> {
        let root = self.open_root()?;
        let path = self.root.join(mission_id);
        if exact_entry_inode(&root, OsStr::new(mission_id))?.is_none() {
            return Err(BrokerError::WorkspaceBoundary);
        }
        let directory = open_exact_directory(&root, OsStr::new(mission_id), &path)?;
        let actual = FileIdentity::from_fd(&directory)?;
        let stat = fstat(&directory).map_err(|_| BrokerError::WorkspaceBoundary)?;
        if actual != expected || stat.st_mode & 0o777 != 0o700 {
            return Err(BrokerError::WorkspaceBoundary);
        }
        let mut identities = self
            .mission_identities
            .lock()
            .map_err(|_| BrokerError::WorkspaceBoundary)?;
        match identities.get(mission_id) {
            Some(pinned) if *pinned != expected => Err(BrokerError::WorkspaceBoundary),
            Some(_) => Ok(()),
            None => {
                identities.insert(mission_id.to_owned(), expected);
                Ok(())
            }
        }
    }

    pub(crate) fn prepare_stage(
        &self,
        mission_id: &str,
        path_components: &[String],
        stage_name: &str,
    ) -> Result<FileIdentity, BrokerError> {
        let (parent, _, parent_path) = self.open_parent(mission_id, path_components, true)?;
        if exact_entry_inode(&parent, OsStr::new(stage_name))?.is_some() {
            return Err(BrokerError::WorkspaceBoundary);
        }
        mkdirat(&parent, stage_name, Mode::RUSR | Mode::WUSR | Mode::XUSR)
            .map_err(|_| BrokerError::WorkspaceBoundary)?;
        let stage_path = parent_path.join(stage_name);
        let result = (|| -> Result<FileIdentity, BrokerError> {
            let stage = open_exact_directory(&parent, OsStr::new(stage_name), &stage_path)?;
            let payload = openat(
                &stage,
                PAYLOAD_NAME,
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC | OFlags::NOFOLLOW,
                Mode::RUSR | Mode::WUSR,
            )
            .map_err(|_| BrokerError::WorkspaceBoundary)?;
            fsync(&payload).map_err(|_| BrokerError::WorkspaceBoundary)?;
            let identity = FileIdentity::from_fd(&payload)?;
            fsync(&stage).map_err(|_| BrokerError::WorkspaceBoundary)?;
            fsync(&parent).map_err(|_| BrokerError::WorkspaceBoundary)?;
            Ok(identity)
        })();
        if result.is_err() {
            if let Ok(stage) = openat(
                &parent,
                stage_name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
                Mode::empty(),
            ) {
                let _ = unlinkat(&stage, PAYLOAD_NAME, AtFlags::empty());
            }
            let _ = unlinkat(&parent, stage_name, AtFlags::REMOVEDIR);
        }
        result
    }

    /// Inspects stage and final state without creating, deleting, renaming, or
    /// rewriting anything. This is the only workspace path used by a
    /// `reattestOnly` permit.
    pub(crate) fn inspect_effect_read_only(
        &self,
        mission_id: &str,
        path_components: &[String],
        stage_name: &str,
    ) -> Result<EffectFilesystemState, BrokerError> {
        let Some((parent, final_name, parent_path)) =
            self.open_parent_optional(mission_id, path_components, false)?
        else {
            return Ok(EffectFilesystemState {
                stage_directory_exists: false,
                stage_payload: None,
                final_file: None,
            });
        };
        let stage_directory_exists = exact_entry_inode(&parent, OsStr::new(stage_name))?.is_some();
        let stage_payload = if stage_directory_exists {
            let stage_path = parent_path.join(stage_name);
            let stage = open_exact_directory(&parent, OsStr::new(stage_name), &stage_path)?;
            require_directory_mode(&stage, 0o700)?;
            if exact_entry_inode(&stage, OsStr::new(PAYLOAD_NAME))?.is_some() {
                let file = openat(
                    &stage,
                    PAYLOAD_NAME,
                    OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
                    Mode::empty(),
                )
                .map_err(|_| BrokerError::WorkspaceBoundary)?;
                require_staged_file(&file)?;
                let identity = FileIdentity::from_fd(&file)?;
                let content = hash_reader(File::from(file))?;
                Some(InspectedFile { identity, content })
            } else {
                if !directory_is_empty(&stage)? {
                    return Err(BrokerError::WorkspaceBoundary);
                }
                None
            }
        } else {
            None
        };
        let final_file = if exact_entry_inode(&parent, &final_name)?.is_some() {
            let file = openat(
                &parent,
                &final_name,
                OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
                Mode::empty(),
            )
            .map_err(|_| BrokerError::WorkspaceBoundary)?;
            if !fd_path_is_exact(&file, &parent_path.join(&final_name))?
                || !is_exact_final_file(&file)?
            {
                return Err(BrokerError::WorkspaceBoundary);
            }
            let identity = FileIdentity::from_fd(&file)?;
            let content = hash_reader(File::from(file))?;
            Some(InspectedFile { identity, content })
        } else {
            None
        };
        Ok(EffectFilesystemState {
            stage_directory_exists,
            stage_payload,
            final_file,
        })
    }

    pub(crate) fn finalize_owned_commit(
        &self,
        mission_id: &str,
        path_components: &[String],
        expected: &PayloadDescriptor,
        stage_name: &str,
        staged_identity: FileIdentity,
    ) -> Result<bool, BrokerError> {
        let state = self.inspect_effect_read_only(mission_id, path_components, stage_name)?;
        let Some(final_file) = state.final_file else {
            return Ok(false);
        };
        if final_file.identity != staged_identity
            || final_file.content.sha256 != expected.sha256
            || final_file.content.byte_len != expected.byte_len
            || state.stage_payload.is_some()
        {
            return Err(BrokerError::WorkspaceBoundary);
        }
        let (parent, final_name, _) = self.open_parent(mission_id, path_components, false)?;
        let file = openat(
            &parent,
            &final_name,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::empty(),
        )
        .map_err(|_| BrokerError::WorkspaceBoundary)?;
        if FileIdentity::from_fd(&file)? != staged_identity {
            return Err(BrokerError::WorkspaceBoundary);
        }
        fsync(&file).map_err(|_| BrokerError::WorkspaceBoundary)?;
        if state.stage_directory_exists {
            unlinkat(&parent, stage_name, AtFlags::REMOVEDIR)
                .map_err(|_| BrokerError::WorkspaceBoundary)?;
        }
        fsync(&parent).map_err(|_| BrokerError::WorkspaceBoundary)?;
        Ok(true)
    }

    pub(crate) fn discard_owned_stage(
        &self,
        mission_id: &str,
        path_components: &[String],
        stage_name: &str,
        staged_identity: Option<FileIdentity>,
    ) -> Result<(), BrokerError> {
        let state = self.inspect_effect_read_only(mission_id, path_components, stage_name)?;
        if state.final_file.is_some() {
            return Err(BrokerError::WorkspaceBoundary);
        }
        match (staged_identity, state.stage_payload.as_ref()) {
            (Some(expected), Some(actual)) if expected == actual.identity => {}
            (None, Some(_)) | (_, None) => {}
            (Some(_), Some(_)) => return Err(BrokerError::WorkspaceBoundary),
        }
        self.cleanup_stage(mission_id, path_components, stage_name)
    }

    pub(crate) fn cleanup_stage(
        &self,
        mission_id: &str,
        path_components: &[String],
        stage_name: &str,
    ) -> Result<(), BrokerError> {
        let Some((parent, _, parent_path)) =
            self.open_parent_optional(mission_id, path_components, false)?
        else {
            return Ok(());
        };
        if exact_entry_inode(&parent, OsStr::new(stage_name))?.is_none() {
            return Ok(());
        }
        let stage_path = parent_path.join(stage_name);
        let stage = open_exact_directory(&parent, OsStr::new(stage_name), &stage_path)?;
        if let Ok(file) = openat(
            &stage,
            PAYLOAD_NAME,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::empty(),
        ) {
            let file = File::from(file);
            scrub_file(&file);
            unlinkat(&stage, PAYLOAD_NAME, AtFlags::empty())
                .map_err(|_| BrokerError::WorkspaceBoundary)?;
        }
        if !directory_is_empty(&stage)? {
            return Err(BrokerError::WorkspaceBoundary);
        }
        unlinkat(&parent, stage_name, AtFlags::REMOVEDIR)
            .map_err(|_| BrokerError::WorkspaceBoundary)?;
        fsync(&parent).map_err(|_| BrokerError::WorkspaceBoundary)?;
        Ok(())
    }

    pub(crate) fn write_atomically(
        &self,
        mission_id: &str,
        path_components: &[String],
        expected: &PayloadDescriptor,
        stage_name: &str,
        mut payload: impl Read,
        before_rename: impl FnOnce() -> Result<(), BrokerError>,
    ) -> Result<CommittedFile, BrokerError> {
        let (parent, final_name, parent_path) =
            self.open_parent(mission_id, path_components, false)?;
        if !target_slot_is_safe(&parent, &final_name)? {
            return Err(BrokerError::WorkspaceBoundary);
        }
        let parent_identity = FileIdentity::from_fd(&parent)?;
        let stage_path = parent_path.join(stage_name);
        let stage = open_exact_directory(&parent, OsStr::new(stage_name), &stage_path)?;
        require_directory_mode(&stage, 0o700)?;
        let temporary = openat(
            &stage,
            PAYLOAD_NAME,
            OFlags::WRONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::empty(),
        )
        .map_err(|_| BrokerError::WorkspaceBoundary)?;
        let mut file = File::from(temporary);
        let mut renamed = false;
        let result = (|| -> Result<CommittedFile, BrokerError> {
            let mut hasher = Sha256::new();
            let mut byte_len = 0_u64;
            let mut buffer = vec![0_u8; STREAM_CHUNK_BYTES].into_boxed_slice();
            loop {
                let count = payload.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                require_staged_file(&file)?;
                file.write_all(&buffer[..count])?;
                hasher.update(&buffer[..count]);
                byte_len = byte_len
                    .checked_add(u64::try_from(count).map_err(|_| BrokerError::PayloadMismatch)?)
                    .ok_or(BrokerError::PayloadMismatch)?;
            }
            let actual = CommittedFile {
                sha256: hex::encode(hasher.finalize()),
                byte_len,
            };
            if actual.sha256 != expected.sha256 || actual.byte_len != expected.byte_len {
                return Err(BrokerError::PayloadMismatch);
            }
            require_staged_file(&file)?;
            file.sync_all()?;
            require_directory_mode(&stage, 0o700)?;
            let (destination, destination_name, destination_path) =
                self.open_parent(mission_id, path_components, false)?;
            if destination_name != final_name
                || destination_path != parent_path
                || FileIdentity::from_fd(&destination)? != parent_identity
                || !target_slot_is_safe(&destination, &destination_name)?
            {
                return Err(BrokerError::WorkspaceBoundary);
            }
            before_rename()?;
            renameat(&stage, PAYLOAD_NAME, &destination, &destination_name)
                .map_err(|_| BrokerError::WorkspaceBoundary)?;
            renamed = true;
            fchmod(&file, Mode::RUSR | Mode::WUSR).map_err(|_| BrokerError::WorkspaceBoundary)?;
            file.sync_all()?;
            if !fd_path_is_exact(&destination, &destination_path)?
                || !fd_path_is_exact(&file, &destination_path.join(&destination_name))?
                || !is_exact_final_file(&file)?
            {
                return Err(BrokerError::WorkspaceBoundary);
            }
            fsync(&destination).map_err(|_| BrokerError::WorkspaceBoundary)?;
            Ok(actual)
        })();
        if result.is_err() {
            scrub_file(&file);
            if renamed {
                let _ = unlinkat(&parent, &final_name, AtFlags::empty());
            } else {
                let _ = unlinkat(&stage, PAYLOAD_NAME, AtFlags::empty());
            }
        }
        let cleanup = unlinkat(&parent, stage_name, AtFlags::REMOVEDIR);
        if cleanup.is_err() && result.is_ok() {
            scrub_file(&file);
            let _ = unlinkat(&parent, &final_name, AtFlags::empty());
            return Err(BrokerError::WorkspaceBoundary);
        }
        fsync(&parent).map_err(|_| BrokerError::WorkspaceBoundary)?;
        result
    }

    fn open_root(&self) -> Result<OwnedFd, BrokerError> {
        let root = open(
            &self.root,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::empty(),
        )
        .map_err(|_| BrokerError::WorkspaceBoundary)?;
        if FileIdentity::from_fd(&root)? != self.root_identity
            || !fd_path_is_exact(&root, &self.root)?
        {
            return Err(BrokerError::WorkspaceBoundary);
        }
        Ok(root)
    }

    fn open_parent(
        &self,
        mission_id: &str,
        path_components: &[String],
        create_parents: bool,
    ) -> Result<(OwnedFd, OsString, PathBuf), BrokerError> {
        self.open_parent_optional(mission_id, path_components, create_parents)?
            .ok_or(BrokerError::WorkspaceBoundary)
    }

    fn open_parent_optional(
        &self,
        mission_id: &str,
        path_components: &[String],
        create_parents: bool,
    ) -> Result<Option<(OwnedFd, OsString, PathBuf)>, BrokerError> {
        let (final_name, parents) = path_components
            .split_last()
            .ok_or(BrokerError::WorkspaceBoundary)?;
        let root = self.open_root()?;
        let mut expected_path = self.root.join(mission_id);
        let mut directory = open_exact_directory(&root, OsStr::new(mission_id), &expected_path)?;
        let identity = FileIdentity::from_fd(&directory)?;
        let identities = self
            .mission_identities
            .lock()
            .map_err(|_| BrokerError::WorkspaceBoundary)?;
        if identities.get(mission_id) != Some(&identity) {
            return Err(BrokerError::WorkspaceBoundary);
        }
        drop(identities);
        for component in parents {
            expected_path.push(component);
            if exact_entry_inode(&directory, OsStr::new(component))?.is_none() {
                if !create_parents {
                    return Ok(None);
                }
                mkdirat(
                    &directory,
                    component.as_str(),
                    Mode::RUSR | Mode::WUSR | Mode::XUSR,
                )
                .map_err(|_| BrokerError::WorkspaceBoundary)?;
                fsync(&directory).map_err(|_| BrokerError::WorkspaceBoundary)?;
            }
            directory = open_exact_directory(&directory, OsStr::new(component), &expected_path)?;
            require_directory_mode(&directory, 0o700)?;
        }
        Ok(Some((directory, OsString::from(final_name), expected_path)))
    }
}

impl FileIdentity {
    fn from_metadata(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }

    fn from_fd(fd: &impl AsFd) -> Result<Self, BrokerError> {
        let stat = fstat(fd).map_err(|_| BrokerError::WorkspaceBoundary)?;
        Ok(Self {
            device: u64::try_from(stat.st_dev).map_err(|_| BrokerError::WorkspaceBoundary)?,
            inode: stat.st_ino,
        })
    }
}

fn exact_entry_inode(directory: &impl AsFd, name: &OsStr) -> Result<Option<u64>, BrokerError> {
    let mut entries = Dir::read_from(directory).map_err(|_| BrokerError::WorkspaceBoundary)?;
    for entry in &mut entries {
        let entry = entry.map_err(|_| BrokerError::WorkspaceBoundary)?;
        if entry.file_name().to_bytes() == name.as_bytes() {
            return Ok(Some(entry.ino()));
        }
    }
    Ok(None)
}

fn directory_is_empty(directory: &impl AsFd) -> Result<bool, BrokerError> {
    let mut entries = Dir::read_from(directory).map_err(|_| BrokerError::WorkspaceBoundary)?;
    for entry in &mut entries {
        let entry = entry.map_err(|_| BrokerError::WorkspaceBoundary)?;
        if !matches!(entry.file_name().to_bytes(), b"." | b"..") {
            return Ok(false);
        }
    }
    Ok(true)
}

fn open_exact_directory(
    parent: &impl AsFd,
    name: &OsStr,
    expected_path: &Path,
) -> Result<OwnedFd, BrokerError> {
    let expected_inode = exact_entry_inode(parent, name)?.ok_or(BrokerError::WorkspaceBoundary)?;
    let directory = openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|_| BrokerError::WorkspaceBoundary)?;
    let identity = FileIdentity::from_fd(&directory)?;
    if identity.inode != expected_inode || !fd_path_is_exact(&directory, expected_path)? {
        return Err(BrokerError::WorkspaceBoundary);
    }
    Ok(directory)
}

fn require_directory_mode(directory: &impl AsFd, expected_mode: u16) -> Result<(), BrokerError> {
    let stat = fstat(directory).map_err(|_| BrokerError::WorkspaceBoundary)?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::Directory
        || stat.st_uid != rustix::process::geteuid().as_raw()
        || stat.st_mode & 0o777 != expected_mode
    {
        return Err(BrokerError::WorkspaceBoundary);
    }
    Ok(())
}

fn require_staged_file(file: &impl AsFd) -> Result<(), BrokerError> {
    let stat = fstat(file).map_err(|_| BrokerError::WorkspaceBoundary)?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
        || stat.st_uid != rustix::process::geteuid().as_raw()
        || stat.st_mode & 0o777 != 0o600
        || stat.st_nlink != 1
    {
        return Err(BrokerError::WorkspaceBoundary);
    }
    Ok(())
}

fn is_exact_final_file(file: &impl AsFd) -> Result<bool, BrokerError> {
    let stat = fstat(file).map_err(|_| BrokerError::WorkspaceBoundary)?;
    Ok(
        FileType::from_raw_mode(stat.st_mode) == FileType::RegularFile
            && stat.st_uid == rustix::process::geteuid().as_raw()
            && stat.st_mode & 0o777 == 0o600
            && stat.st_nlink == 1,
    )
}

fn fd_path_is_exact(fd: &impl AsFd, expected_path: &Path) -> Result<bool, BrokerError> {
    let actual = getpath(fd).map_err(|_| BrokerError::WorkspaceBoundary)?;
    Ok(Path::new(OsStr::from_bytes(actual.to_bytes())) == expected_path)
}

fn target_slot_is_safe(parent: &impl AsFd, final_name: &OsStr) -> Result<bool, BrokerError> {
    let exact_inode = exact_entry_inode(parent, final_name)?;
    match statat(parent, final_name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => Ok(exact_inode == Some(stat.st_ino)
            && FileType::from_raw_mode(stat.st_mode) == FileType::RegularFile),
        Err(rustix::io::Errno::NOENT) => Ok(exact_inode.is_none()),
        Err(_) => Err(BrokerError::WorkspaceBoundary),
    }
}

fn hash_reader(mut reader: impl Read) -> Result<CommittedFile, BrokerError> {
    let mut hasher = Sha256::new();
    let mut byte_len = 0_u64;
    let mut buffer = vec![0_u8; STREAM_CHUNK_BYTES].into_boxed_slice();
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
        byte_len = byte_len
            .checked_add(u64::try_from(count).map_err(|_| BrokerError::PayloadMismatch)?)
            .ok_or(BrokerError::PayloadMismatch)?;
    }
    Ok(CommittedFile {
        sha256: hex::encode(hasher.finalize()),
        byte_len,
    })
}

fn scrub_file(file: &File) {
    let _ = file.set_len(0);
    let _ = file.sync_all();
}
