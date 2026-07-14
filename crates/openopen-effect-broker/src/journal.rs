use crate::{BrokerError, sha256_hex};
use openopen_protocol::{EffectCommand, EffectNonCommit, EffectReceipt, PayloadDescriptor};
use rusqlite::{Connection, OptionalExtension, params};
use std::fs::Permissions;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

const JOURNAL_NAME: &str = ".openopen-effect-journal.sqlite3";

type JournalRow = (
    String,
    u32,
    String,
    String,
    String,
    String,
    i64,
    String,
    i64,
    String,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<String>,
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EffectState {
    Accepted,
    FilesystemCommitted,
    ReceiptCommitted,
    NotCommitted,
}

impl EffectState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::FilesystemCommitted => "filesystem_committed",
            Self::ReceiptCommitted => "receipt_committed",
            Self::NotCommitted => "not_committed",
        }
    }

    fn parse(value: &str) -> Result<Self, BrokerError> {
        match value {
            "accepted" => Ok(Self::Accepted),
            "filesystem_committed" => Ok(Self::FilesystemCommitted),
            "receipt_committed" => Ok(Self::ReceiptCommitted),
            "not_committed" => Ok(Self::NotCommitted),
            _ => Err(BrokerError::JournalMismatch),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct JournalEntry {
    pub effect_id: String,
    pub stable_effect_hash: String,
    pub mission_id: String,
    pub path_components: Vec<String>,
    pub payload_sha256: String,
    pub payload_byte_len: u64,
    pub stage_name: String,
    pub write_started: bool,
    pub state: EffectState,
    pub stage_device: Option<u64>,
    pub stage_inode: Option<u64>,
    pub commit_intent_at_ms: Option<i64>,
    pub completed_at_ms: Option<i64>,
    pub committed_session_nonce: Option<String>,
    pub receipt: Option<EffectReceipt>,
    pub reconciled_at_ms: Option<i64>,
    pub noncommit: Option<EffectNonCommit>,
}

pub(crate) struct Journal {
    connection: Connection,
    audit_euid: u32,
}

impl Journal {
    pub(crate) fn open(
        root: &Path,
        audit_euid: u32,
        core_key_id: &str,
        broker_key_id: &str,
    ) -> Result<Self, BrokerError> {
        let path = root.join(JOURNAL_NAME);
        if let Ok(metadata) = std::fs::symlink_metadata(&path)
            && (metadata.file_type().is_symlink()
                || !metadata.is_file()
                || metadata.uid() != rustix::process::geteuid().as_raw())
        {
            return Err(BrokerError::InvalidRoot);
        }
        let connection = Connection::open(&path)?;
        std::fs::set_permissions(&path, Permissions::from_mode(0o600))?;
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.permissions().mode() & 0o777 != 0o600
        {
            return Err(BrokerError::InvalidRoot);
        }
        connection.execute_batch(
            "PRAGMA journal_mode = DELETE;
             PRAGMA synchronous = FULL;
             PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS broker_meta (
                singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                audit_euid INTEGER NOT NULL,
                core_key_id TEXT NOT NULL,
                broker_key_id TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS mission_workspace (
                mission_id TEXT PRIMARY KEY,
                device INTEGER NOT NULL,
                inode INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS effect_journal (
                effect_id TEXT PRIMARY KEY,
                audit_euid INTEGER NOT NULL,
                stable_effect_hash TEXT NOT NULL,
                mission_id TEXT NOT NULL,
                path_components_json TEXT NOT NULL,
                payload_sha256 TEXT NOT NULL,
                payload_byte_len INTEGER NOT NULL,
                stage_name TEXT NOT NULL,
                write_started INTEGER NOT NULL DEFAULT 0,
                state TEXT NOT NULL,
                stage_device INTEGER,
                stage_inode INTEGER,
                commit_intent_at_ms INTEGER,
                completed_at_ms INTEGER,
                committed_session_nonce TEXT,
                receipt_json TEXT,
                reconciled_at_ms INTEGER,
                noncommit_json TEXT
             );",
        )?;
        add_column_if_missing(&connection, "stage_device", "INTEGER")?;
        add_column_if_missing(&connection, "stage_inode", "INTEGER")?;
        add_column_if_missing(&connection, "commit_intent_at_ms", "INTEGER")?;
        add_column_if_missing(&connection, "completed_at_ms", "INTEGER")?;
        add_column_if_missing(&connection, "reconciled_at_ms", "INTEGER")?;
        add_column_if_missing(&connection, "noncommit_json", "TEXT")?;
        let stored_meta: Option<(u32, String, String)> = connection
            .query_row(
                "SELECT audit_euid, core_key_id, broker_key_id
                 FROM broker_meta WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        match stored_meta {
            Some((stored_euid, stored_core, stored_broker))
                if stored_euid != audit_euid
                    || stored_core != core_key_id
                    || stored_broker != broker_key_id =>
            {
                return Err(BrokerError::JournalMismatch);
            }
            Some(_) => {}
            None => {
                connection.execute(
                    "INSERT INTO broker_meta
                        (singleton, audit_euid, core_key_id, broker_key_id)
                     VALUES (1, ?1, ?2, ?3)",
                    params![audit_euid, core_key_id, broker_key_id],
                )?;
            }
        }
        Ok(Self {
            connection,
            audit_euid,
        })
    }

    pub(crate) fn accept(
        &mut self,
        command: &EffectCommand,
        stable_effect_hash: &str,
        payload: &PayloadDescriptor,
        path_components: &[String],
    ) -> Result<JournalEntry, BrokerError> {
        if let Some(existing) = self.load(&command.effect_id)? {
            if existing.stable_effect_hash != stable_effect_hash
                || existing.mission_id != command.mission_id
                || existing.path_components != path_components
                || existing.payload_sha256 != payload.sha256
                || existing.payload_byte_len != payload.byte_len
            {
                return Err(BrokerError::EffectConflict);
            }
            if existing.state == EffectState::NotCommitted {
                return Err(BrokerError::EffectNotCommitted);
            }
            return Ok(existing);
        }
        let stage_name = random_stage_name()?;
        let path_json =
            serde_json::to_string(path_components).map_err(|_| BrokerError::JournalMismatch)?;
        let payload_byte_len =
            i64::try_from(payload.byte_len).map_err(|_| BrokerError::InvalidCommand)?;
        self.connection.execute(
            "INSERT INTO effect_journal
                (effect_id, audit_euid, stable_effect_hash, mission_id,
                 path_components_json, payload_sha256, payload_byte_len,
                 stage_name, write_started, state)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9)",
            params![
                command.effect_id,
                self.audit_euid,
                stable_effect_hash,
                command.mission_id,
                path_json,
                payload.sha256,
                payload_byte_len,
                stage_name,
                EffectState::Accepted.as_str(),
            ],
        )?;
        self.load(&command.effect_id)?
            .ok_or(BrokerError::JournalMismatch)
    }

    pub(crate) fn load(&self, effect_id: &str) -> Result<Option<JournalEntry>, BrokerError> {
        let row: Option<JournalRow> = self
            .connection
            .query_row(
                "SELECT effect_id, audit_euid, stable_effect_hash, mission_id,
                        path_components_json, payload_sha256, payload_byte_len,
                        stage_name, write_started, state, stage_device, stage_inode,
                        commit_intent_at_ms, completed_at_ms, committed_session_nonce,
                        receipt_json, reconciled_at_ms, noncommit_json
                 FROM effect_journal WHERE effect_id = ?1",
                [effect_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                        row.get(10)?,
                        row.get(11)?,
                        row.get(12)?,
                        row.get(13)?,
                        row.get(14)?,
                        row.get(15)?,
                        row.get(16)?,
                        row.get(17)?,
                    ))
                },
            )
            .optional()?;
        row.map(|row| self.decode_row(row)).transpose()
    }

    fn decode_row(&self, row: JournalRow) -> Result<JournalEntry, BrokerError> {
        let (
            effect_id,
            audit_euid,
            stable_effect_hash,
            mission_id,
            path_json,
            payload_sha256,
            payload_byte_len,
            stage_name,
            write_started,
            state,
            stage_device,
            stage_inode,
            commit_intent_at_ms,
            completed_at_ms,
            committed_session_nonce,
            receipt_json,
            reconciled_at_ms,
            noncommit_json,
        ) = row;
        if audit_euid != self.audit_euid {
            return Err(BrokerError::JournalMismatch);
        }
        let state = EffectState::parse(&state)?;
        if !(JournalShape {
            state,
            write_started,
            stage_device,
            stage_inode,
            commit_intent_at_ms,
            completed_at_ms,
            committed_session_nonce: committed_session_nonce.as_deref(),
            receipt_json: receipt_json.as_deref(),
            reconciled_at_ms,
            noncommit_json: noncommit_json.as_deref(),
        })
        .is_valid()
        {
            return Err(BrokerError::JournalMismatch);
        }
        Ok(JournalEntry {
            effect_id,
            stable_effect_hash,
            mission_id,
            path_components: parse_json(&path_json)?,
            payload_sha256,
            payload_byte_len: u64::try_from(payload_byte_len)
                .map_err(|_| BrokerError::JournalMismatch)?,
            stage_name,
            write_started: write_started == 1,
            state,
            stage_device: to_optional_u64(stage_device)?,
            stage_inode: to_optional_u64(stage_inode)?,
            commit_intent_at_ms,
            completed_at_ms,
            committed_session_nonce,
            receipt: receipt_json.as_deref().map(parse_json).transpose()?,
            reconciled_at_ms,
            noncommit: noncommit_json.as_deref().map(parse_json).transpose()?,
        })
    }

    pub(crate) fn bind_workspace(
        &self,
        mission_id: &str,
        device: u64,
        inode: u64,
    ) -> Result<(), BrokerError> {
        let device = i64::try_from(device).map_err(|_| BrokerError::WorkspaceBoundary)?;
        let inode = i64::try_from(inode).map_err(|_| BrokerError::WorkspaceBoundary)?;
        let existing: Option<(i64, i64)> = self
            .connection
            .query_row(
                "SELECT device, inode FROM mission_workspace WHERE mission_id = ?1",
                [mission_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match existing {
            Some(expected) if expected != (device, inode) => Err(BrokerError::WorkspaceBoundary),
            Some(_) => Ok(()),
            None => {
                self.connection.execute(
                    "INSERT INTO mission_workspace(mission_id, device, inode)
                     VALUES (?1, ?2, ?3)",
                    params![mission_id, device, inode],
                )?;
                Ok(())
            }
        }
    }

    pub(crate) fn workspace_identity(
        &self,
        mission_id: &str,
    ) -> Result<Option<(u64, u64)>, BrokerError> {
        self.connection
            .query_row(
                "SELECT device, inode FROM mission_workspace WHERE mission_id = ?1",
                [mission_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?
            .map(|(device, inode)| {
                Ok((
                    u64::try_from(device).map_err(|_| BrokerError::JournalMismatch)?,
                    u64::try_from(inode).map_err(|_| BrokerError::JournalMismatch)?,
                ))
            })
            .transpose()
    }

    pub(crate) fn mark_stage_identity(
        &self,
        effect_id: &str,
        stable_effect_hash: &str,
        device: u64,
        inode: u64,
    ) -> Result<(), BrokerError> {
        let device = i64::try_from(device).map_err(|_| BrokerError::WorkspaceBoundary)?;
        let inode = i64::try_from(inode).map_err(|_| BrokerError::WorkspaceBoundary)?;
        Self::require_one(self.connection.execute(
            "UPDATE effect_journal
             SET write_started = 1, stage_device = ?4, stage_inode = ?5
             WHERE effect_id = ?1 AND stable_effect_hash = ?2 AND state = ?3
               AND write_started = 0 AND stage_device IS NULL AND stage_inode IS NULL",
            params![
                effect_id,
                stable_effect_hash,
                EffectState::Accepted.as_str(),
                device,
                inode,
            ],
        )?)
    }

    pub(crate) fn mark_commit_intent(
        &self,
        effect_id: &str,
        stable_effect_hash: &str,
        commit_intent_at_ms: i64,
        session_nonce: &str,
    ) -> Result<(), BrokerError> {
        Self::require_one(self.connection.execute(
            "UPDATE effect_journal
             SET commit_intent_at_ms = ?4, committed_session_nonce = ?5
             WHERE effect_id = ?1 AND stable_effect_hash = ?2 AND state = ?3
               AND write_started = 1 AND stage_device IS NOT NULL AND stage_inode IS NOT NULL
               AND commit_intent_at_ms IS NULL
               AND committed_session_nonce IS NULL",
            params![
                effect_id,
                stable_effect_hash,
                EffectState::Accepted.as_str(),
                commit_intent_at_ms,
                session_nonce,
            ],
        )?)
    }

    pub(crate) fn mark_filesystem_committed(
        &self,
        effect_id: &str,
        stable_effect_hash: &str,
        completed_at_ms: i64,
        session_nonce: &str,
    ) -> Result<(), BrokerError> {
        Self::require_one(self.connection.execute(
            "UPDATE effect_journal
             SET state = ?3, completed_at_ms = ?4, committed_session_nonce = ?5
             WHERE effect_id = ?1 AND stable_effect_hash = ?2 AND state = ?6
               AND write_started = 1 AND stage_device IS NOT NULL AND stage_inode IS NOT NULL
               AND commit_intent_at_ms IS NOT NULL AND completed_at_ms IS NULL
               AND committed_session_nonce IS NOT NULL",
            params![
                effect_id,
                stable_effect_hash,
                EffectState::FilesystemCommitted.as_str(),
                completed_at_ms,
                session_nonce,
                EffectState::Accepted.as_str(),
            ],
        )?)
    }

    pub(crate) fn mark_not_committed(
        &self,
        effect_id: &str,
        stable_effect_hash: &str,
        attestation: &EffectNonCommit,
    ) -> Result<(), BrokerError> {
        let noncommit_json =
            serde_json::to_string(attestation).map_err(|_| BrokerError::JournalMismatch)?;
        Self::require_one(self.connection.execute(
            "UPDATE effect_journal
             SET state = ?3, write_started = 0, stage_device = NULL, stage_inode = NULL,
                 commit_intent_at_ms = NULL, completed_at_ms = NULL,
                 committed_session_nonce = NULL, receipt_json = NULL,
                 reconciled_at_ms = ?4, noncommit_json = ?5
             WHERE effect_id = ?1 AND stable_effect_hash = ?2 AND state = ?6",
            params![
                effect_id,
                stable_effect_hash,
                EffectState::NotCommitted.as_str(),
                attestation.reconciled_at_ms,
                noncommit_json,
                EffectState::Accepted.as_str(),
            ],
        )?)
    }

    pub(crate) fn refresh_not_committed(
        &self,
        effect_id: &str,
        stable_effect_hash: &str,
        attestation: &EffectNonCommit,
    ) -> Result<(), BrokerError> {
        let noncommit_json =
            serde_json::to_string(attestation).map_err(|_| BrokerError::JournalMismatch)?;
        Self::require_one(self.connection.execute(
            "UPDATE effect_journal
             SET reconciled_at_ms = ?4, noncommit_json = ?5
             WHERE effect_id = ?1 AND stable_effect_hash = ?2 AND state = ?3
               AND write_started = 0 AND stage_device IS NULL AND stage_inode IS NULL
               AND commit_intent_at_ms IS NULL AND completed_at_ms IS NULL
               AND committed_session_nonce IS NULL AND receipt_json IS NULL
               AND reconciled_at_ms IS NOT NULL AND noncommit_json IS NOT NULL",
            params![
                effect_id,
                stable_effect_hash,
                EffectState::NotCommitted.as_str(),
                attestation.reconciled_at_ms,
                noncommit_json,
            ],
        )?)
    }

    pub(crate) fn commit_receipt(
        &self,
        effect_id: &str,
        stable_effect_hash: &str,
        receipt: &EffectReceipt,
    ) -> Result<(), BrokerError> {
        let receipt_json =
            serde_json::to_string(receipt).map_err(|_| BrokerError::JournalMismatch)?;
        Self::require_one(self.connection.execute(
            "UPDATE effect_journal
             SET state = ?3, receipt_json = ?4, committed_session_nonce = ?6
             WHERE effect_id = ?1 AND stable_effect_hash = ?2 AND state = ?5",
            params![
                effect_id,
                stable_effect_hash,
                EffectState::ReceiptCommitted.as_str(),
                receipt_json,
                EffectState::FilesystemCommitted.as_str(),
                receipt.broker_session_nonce,
            ],
        )?)
    }

    fn require_one(changed: usize) -> Result<(), BrokerError> {
        if changed == 1 {
            Ok(())
        } else {
            Err(BrokerError::JournalMismatch)
        }
    }
}

struct JournalShape<'a> {
    state: EffectState,
    write_started: i64,
    stage_device: Option<i64>,
    stage_inode: Option<i64>,
    commit_intent_at_ms: Option<i64>,
    completed_at_ms: Option<i64>,
    committed_session_nonce: Option<&'a str>,
    receipt_json: Option<&'a str>,
    reconciled_at_ms: Option<i64>,
    noncommit_json: Option<&'a str>,
}

impl JournalShape<'_> {
    fn is_valid(&self) -> bool {
        let stage_present = self.stage_device.is_some() && self.stage_inode.is_some();
        let stage_absent = self.stage_device.is_none() && self.stage_inode.is_none();
        if !matches!(self.write_started, 0 | 1) {
            return false;
        }
        match self.state {
            EffectState::Accepted => {
                self.receipt_json.is_none()
                    && self.noncommit_json.is_none()
                    && self.completed_at_ms.is_none()
                    && self.reconciled_at_ms.is_none()
                    && if self.write_started == 1 {
                        stage_present
                            && self.commit_intent_at_ms.is_some()
                                == self.committed_session_nonce.is_some()
                    } else {
                        stage_absent
                            && self.commit_intent_at_ms.is_none()
                            && self.committed_session_nonce.is_none()
                    }
            }
            EffectState::FilesystemCommitted | EffectState::ReceiptCommitted => {
                self.write_started == 1
                    && stage_present
                    && self.commit_intent_at_ms.is_some()
                    && self.completed_at_ms.is_some()
                    && self.committed_session_nonce.is_some()
                    && (self.receipt_json.is_some()
                        == (self.state == EffectState::ReceiptCommitted))
                    && self.reconciled_at_ms.is_none()
                    && self.noncommit_json.is_none()
            }
            EffectState::NotCommitted => {
                self.write_started == 0
                    && stage_absent
                    && self.commit_intent_at_ms.is_none()
                    && self.completed_at_ms.is_none()
                    && self.committed_session_nonce.is_none()
                    && self.receipt_json.is_none()
                    && self.reconciled_at_ms.is_some()
                    && self.noncommit_json.is_some()
            }
        }
    }
}

fn to_optional_u64(value: Option<i64>) -> Result<Option<u64>, BrokerError> {
    value
        .map(|value| u64::try_from(value).map_err(|_| BrokerError::JournalMismatch))
        .transpose()
}

fn parse_json<T: serde::de::DeserializeOwned>(value: &str) -> Result<T, BrokerError> {
    serde_json::from_str(value).map_err(|_| BrokerError::JournalMismatch)
}

fn random_stage_name() -> Result<String, BrokerError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| BrokerError::Crypto)?;
    let name = format!(".openopen-stage-{}", hex::encode(bytes));
    debug_assert_eq!(sha256_hex(name.as_bytes()).len(), 64);
    Ok(name)
}

fn add_column_if_missing(
    connection: &Connection,
    column: &str,
    column_type: &str,
) -> Result<(), BrokerError> {
    let mut statement = connection.prepare("PRAGMA table_info(effect_journal)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|existing| existing == column) {
        connection.execute(
            &format!("ALTER TABLE effect_journal ADD COLUMN {column} {column_type}"),
            [],
        )?;
    }
    Ok(())
}
