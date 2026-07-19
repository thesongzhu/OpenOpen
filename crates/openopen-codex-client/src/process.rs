use crate::{CodexError, wire::Transport};
use serde::Deserialize;
use serde_json::to_string;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const CODEX_VERSION: &str = "0.144.0";
pub const CODEX_BINARY_SHA256: &str =
    "978740e6bcbd9af2f850823b723fb74f16d8d1e44de05f7dd6737ae631f72017";
pub const CODEX_PACKAGE_SHA256: &str =
    "c1b3af67fd28bbf768765357251f7b29d315150068cb41101dc77eb8a42bc7eb";
pub const CODEX_CODE_MODE_HOST_SHA256: &str =
    "067ee7894c49489ca72fc2ca6093f408302241bd22097fcd12d785c9ba40fd43";
pub const CODEX_RG_SHA256: &str =
    "4fdf1d8365af224bc70e3c1490d8461d859c37cc70e739a11e987af0215f3e94";
const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";
const CODEX_RUNTIME_RECEIPT: &str = "CODEX-RUNTIME-RECEIPT.json";
const CODEX_SYSTEM_CONFIG_PATHS: [&str; 3] = [
    "/private/etc/codex/requirements.toml",
    "/private/etc/codex/managed_config.toml",
    "/private/etc/codex/config.toml",
];
const CONFIG: &str = "forced_login_method = \"chatgpt\"\n\
cli_auth_credentials_store = \"keyring\"\n\
mcp_oauth_credentials_store = \"keyring\"\n\
approval_policy = \"never\"\n\
sandbox_mode = \"read-only\"\n\
web_search = \"disabled\"\n\
\n\
[analytics]\n\
enabled = false\n\
\n\
[features]\n\
secret_auth_storage = false\n";
const PREVIOUS_CONFIG: &str = "forced_login_method = \"chatgpt\"\n\
cli_auth_credentials_store = \"keyring\"\n\
mcp_oauth_credentials_store = \"keyring\"\n\
approval_policy = \"never\"\n\
sandbox_mode = \"read-only\"\n\
web_search = \"disabled\"\n\
\n\
[analytics]\n\
enabled = false\n";

#[derive(Clone, Debug)]
pub struct CodexRuntimeConfig {
    pub runtime: PathBuf,
    pub codex_home: PathBuf,
    pub synthetic_home: PathBuf,
    pub model_workspace: PathBuf,
    pub user_home: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LoginKeychainAccess {
    ReadOnly,
    LoginWriteOnly,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CodexRuntimeReceipt {
    schema_version: u32,
    component: String,
    version: String,
    upstream_rg_sha256: String,
    runtime_rg_sha256: String,
    signing_identifier: String,
    team_identifier: String,
    cdhash: String,
}

pub(crate) fn spawn(
    config: &CodexRuntimeConfig,
    login_keychain_access: LoginKeychainAccess,
) -> Result<(Transport, i32), CodexError> {
    let runtime = canonical_regular_file(&config.runtime)?;
    if file_sha256(&runtime)? != CODEX_BINARY_SHA256 {
        return Err(CodexError::RuntimeMismatch);
    }
    let package_root = runtime
        .parent()
        .and_then(Path::parent)
        .ok_or(CodexError::InvalidPath)?;
    if runtime.file_name().and_then(|name| name.to_str()) != Some("codex")
        || runtime
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            != Some("bin")
        || file_sha256(&canonical_regular_file(
            &package_root.join("codex-package.json"),
        )?)? != CODEX_PACKAGE_SHA256
        || file_sha256(&canonical_regular_file(
            &package_root.join("bin/codex-code-mode-host"),
        )?)? != CODEX_CODE_MODE_HOST_SHA256
        || verify_rg_runtime(package_root).is_err()
    {
        return Err(CodexError::RuntimeMismatch);
    }
    let user_home = canonical_user_home(&config.user_home)?;
    let login_keychain = canonical_regular_file_beneath(
        &user_home,
        Path::new("Library/Keychains/login.keychain-db"),
    )?;
    let codex_home = canonical_runtime_home(&config.codex_home, &login_keychain)?;
    let synthetic_home = canonical_directory(&config.synthetic_home, true)?;
    if !synthetic_home.starts_with(&codex_home) {
        return Err(CodexError::InvalidPath);
    }
    let model_workspace = canonical_directory(&config.model_workspace, false)?;
    require_disjoint_paths(&login_keychain, &[&codex_home, &synthetic_home])?;
    let sandbox_exec = canonical_regular_file(Path::new(SANDBOX_EXEC))?;
    if sandbox_exec != Path::new(SANDBOX_EXEC) {
        return Err(CodexError::SandboxUnavailable);
    }
    ensure_credential_file_absent(&codex_home)?;
    let temp = codex_home.join("tmp");
    fs::create_dir_all(&temp).map_err(CodexError::Io)?;
    let temp = canonical_directory(&temp, false)?;
    ensure_exact_config(&codex_home.join("config.toml"))?;
    let profile = sandbox_profile(
        &runtime,
        package_root,
        &codex_home,
        &synthetic_home,
        &model_workspace,
        &login_keychain,
        login_keychain_access,
    )?;

    let mut child = Command::new(sandbox_exec)
        .arg("-p")
        .arg(profile)
        .arg(&runtime)
        .arg("app-server")
        .env_clear()
        .env("CODEX_HOME", &codex_home)
        .env("CFFIXED_USER_HOME", &synthetic_home)
        .env("HOME", user_home)
        .env("PATH", "/usr/bin:/bin")
        .env("TMPDIR", &temp)
        .current_dir(&model_workspace)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| CodexError::SandboxUnavailable)?;
    require_inherited_process_group(&mut child)?;
    let process_identifier =
        i32::try_from(child.id()).map_err(|_| CodexError::SandboxUnavailable)?;
    Ok((Transport::new(child)?, process_identifier))
}

pub(crate) fn ensure_credential_file_absent(codex_home: &Path) -> Result<(), CodexError> {
    match fs::symlink_metadata(codex_home.join("auth.json")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CodexError::Io(error)),
        Ok(_) => Err(CodexError::CredentialFilePresent),
    }
}

fn verify_rg_runtime(package_root: &Path) -> Result<(), CodexError> {
    let rg = canonical_regular_file(&package_root.join("codex-path/rg"))?;
    let runtime_sha = file_sha256(&rg)?;
    if runtime_sha == CODEX_RG_SHA256 {
        return Ok(());
    }

    let codex_root = package_root.parent().ok_or(CodexError::InvalidPath)?;
    let resources_root = codex_root.parent().ok_or(CodexError::InvalidPath)?;
    if package_root.file_name().and_then(|value| value.to_str()) != Some(CODEX_VERSION)
        || codex_root.file_name().and_then(|value| value.to_str()) != Some("Codex")
        || resources_root.file_name().and_then(|value| value.to_str()) != Some("Resources")
    {
        return Err(CodexError::InvalidPath);
    }
    let receipt_path =
        canonical_regular_file(&resources_root.join("Notices").join(CODEX_RUNTIME_RECEIPT))?;
    let receipt: CodexRuntimeReceipt =
        serde_json::from_slice(&fs::read(receipt_path).map_err(CodexError::Io)?)
            .map_err(|_| CodexError::RuntimeMismatch)?;
    let valid_lower_hex = |value: &str, length: usize| {
        value.len() == length
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    };
    if receipt.schema_version != 1
        || receipt.component != "openai/codex"
        || receipt.version != CODEX_VERSION
        || receipt.upstream_rg_sha256 != CODEX_RG_SHA256
        || receipt.runtime_rg_sha256 != runtime_sha
        || receipt.signing_identifier != "rg"
        || receipt.team_identifier.len() != 10
        || !receipt
            .team_identifier
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte.is_ascii_uppercase())
        || !valid_lower_hex(&receipt.cdhash, 40)
    {
        return Err(CodexError::RuntimeMismatch);
    }
    Ok(())
}

fn require_inherited_process_group(child: &mut std::process::Child) -> Result<(), CodexError> {
    let child_pid = i32::try_from(child.id())
        .ok()
        .and_then(rustix::process::Pid::from_raw);
    let inherited_group = child_pid.and_then(|pid| rustix::process::getpgid(Some(pid)).ok());
    if inherited_group == Some(rustix::process::getpgrp()) {
        return Ok(());
    }
    let _ = child.kill();
    let _ = child.wait();
    Err(CodexError::SandboxUnavailable)
}

fn canonical_regular_file(path: &Path) -> Result<PathBuf, CodexError> {
    if !path.is_absolute() {
        return Err(CodexError::InvalidPath);
    }
    let metadata = fs::symlink_metadata(path).map_err(CodexError::Io)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CodexError::InvalidPath);
    }
    fs::canonicalize(path).map_err(CodexError::Io)
}

fn canonical_regular_file_beneath(root: &Path, relative: &Path) -> Result<PathBuf, CodexError> {
    if !root.is_absolute()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(CodexError::InvalidPath);
    }
    let canonical_root = fs::canonicalize(root).map_err(CodexError::Io)?;
    if canonical_root != root {
        return Err(CodexError::InvalidPath);
    }

    let mut candidate = root.to_path_buf();
    let component_count = relative.components().count();
    for (index, component) in relative.components().enumerate() {
        candidate.push(component.as_os_str());
        let metadata = fs::symlink_metadata(&candidate).map_err(CodexError::Io)?;
        if metadata.file_type().is_symlink()
            || (index + 1 == component_count && !metadata.is_file())
            || (index + 1 != component_count && !metadata.is_dir())
        {
            return Err(CodexError::InvalidPath);
        }
    }
    let canonical = fs::canonicalize(&candidate).map_err(CodexError::Io)?;
    if canonical != candidate {
        return Err(CodexError::InvalidPath);
    }
    Ok(canonical)
}

fn require_disjoint_paths(path: &Path, writable_roots: &[&Path]) -> Result<(), CodexError> {
    if writable_roots
        .iter()
        .any(|root| path.starts_with(root) || root.starts_with(path))
    {
        return Err(CodexError::InvalidPath);
    }
    Ok(())
}

fn canonical_runtime_home(path: &Path, login_keychain: &Path) -> Result<PathBuf, CodexError> {
    let expected =
        PathBuf::from("/Library/Application Support/com.thesongzhu.OpenOpenRuntime/users")
            .join(rustix::process::geteuid().as_raw().to_string())
            .join("CodexHome");
    if path != expected {
        return Err(CodexError::InvalidPath);
    }
    let home = canonical_directory(path, false)?;
    let metadata = fs::symlink_metadata(&home).map_err(CodexError::Io)?;
    let login_metadata = fs::symlink_metadata(login_keychain).map_err(CodexError::Io)?;
    if metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o700
        || metadata.dev() == login_metadata.dev()
    {
        return Err(CodexError::InvalidPath);
    }
    #[cfg(target_os = "macos")]
    {
        let file_system = rustix::fs::statfs(&home).map_err(|error| {
            CodexError::Io(std::io::Error::from_raw_os_error(error.raw_os_error()))
        })?;
        let name = |bytes: &[std::ffi::c_char]| {
            bytes
                .iter()
                .take_while(|byte| **byte != 0)
                .map(|byte| (*byte).cast_unsigned())
                .collect::<Vec<_>>()
        };
        if name(&file_system.f_fstypename) != b"tmpfs"
            || name(&file_system.f_mntonname) != home.as_os_str().as_encoded_bytes()
        {
            return Err(CodexError::InvalidPath);
        }
    }
    Ok(home)
}

fn canonical_user_home(path: &Path) -> Result<PathBuf, CodexError> {
    let home = canonical_directory(path, false)?;
    let parent = home.parent().ok_or(CodexError::InvalidPath)?;
    if parent != Path::new("/Users")
        || home
            .file_name()
            .and_then(|value| value.to_str())
            .is_none_or(str::is_empty)
        || fs::symlink_metadata(&home).map_err(CodexError::Io)?.uid()
            != rustix::process::geteuid().as_raw()
    {
        return Err(CodexError::InvalidPath);
    }
    Ok(home)
}

fn canonical_directory(path: &Path, create: bool) -> Result<PathBuf, CodexError> {
    if !path.is_absolute() {
        return Err(CodexError::InvalidPath);
    }
    if create {
        fs::create_dir_all(path).map_err(CodexError::Io)?;
    }
    let metadata = fs::symlink_metadata(path).map_err(CodexError::Io)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(CodexError::InvalidPath);
    }
    let canonical = fs::canonicalize(path).map_err(CodexError::Io)?;
    if canonical != path {
        return Err(CodexError::InvalidPath);
    }
    Ok(canonical)
}

fn ensure_exact_config(path: &Path) -> Result<(), CodexError> {
    match fs::read_to_string(path) {
        Ok(existing) if existing == CONFIG => Ok(()),
        Ok(existing) if existing == PREVIOUS_CONFIG => replace_previous_config(path),
        Ok(_) => Err(CodexError::ConfigMismatch),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(path)
                .map_err(CodexError::Io)?;
            file.write_all(CONFIG.as_bytes()).map_err(CodexError::Io)?;
            file.sync_all().map_err(CodexError::Io)
        }
        Err(error) => Err(CodexError::Io(error)),
    }
}

fn replace_previous_config(path: &Path) -> Result<(), CodexError> {
    let metadata = fs::symlink_metadata(path).map_err(CodexError::Io)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CodexError::InvalidPath);
    }
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(CodexError::Io)?;
    file.write_all(CONFIG.as_bytes()).map_err(CodexError::Io)?;
    file.sync_all().map_err(CodexError::Io)
}

fn file_sha256(path: &Path) -> Result<String, CodexError> {
    let mut file = File::open(path).map_err(CodexError::Io)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(CodexError::Io)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn sandbox_regex_path(value: &str) -> Result<String, CodexError> {
    value
        .chars()
        .try_fold(String::new(), |mut escaped, character| {
            if character == '.' {
                escaped.push_str("[.]");
            } else if character.is_alphanumeric() || matches!(character, '/' | '_' | '-' | ' ') {
                escaped.push(character);
            } else {
                return Err(CodexError::InvalidPath);
            }
            Ok(escaped)
        })
}

fn login_keychain_sidecar_rules(
    login_keychain: &Path,
    access: LoginKeychainAccess,
) -> Result<String, CodexError> {
    if access == LoginKeychainAccess::ReadOnly {
        return Ok(String::new());
    }
    let login_keychain = login_keychain.to_str().ok_or(CodexError::InvalidPath)?;
    let hex_suffix = "[0-9A-Fa-f]".repeat(8);
    let random_suffix = "[A-Za-z0-9]".repeat(6);
    let pattern = format!(
        "^{}[.]sb-{hex_suffix}-{random_suffix}$",
        sandbox_regex_path(login_keychain)?
    );
    Ok(format!(
        "(allow file-write-create file-write-mode file-write-owner \
         file-write-flags file-write-times file-write-unlink \
         (regex #\"{pattern}\"))\n"
    ))
}

fn login_keychain_lock_pattern(parent: &Path) -> Result<String, CodexError> {
    let parent = parent.to_str().ok_or(CodexError::InvalidPath)?;
    Ok(format!(
        "^{}/[.]fl[0-9A-Fa-f]+$",
        sandbox_regex_path(parent)?
    ))
}

fn sandbox_profile(
    runtime: &Path,
    package_root: &Path,
    codex_home: &Path,
    synthetic_home: &Path,
    model_workspace: &Path,
    login_keychain: &Path,
    login_keychain_access: LoginKeychainAccess,
) -> Result<String, CodexError> {
    let quote = |path: &Path| {
        path.to_str()
            .ok_or(CodexError::InvalidPath)
            .and_then(|value| to_string(value).map_err(|_| CodexError::InvalidPath))
    };
    let login_keychain_parent = login_keychain.parent().ok_or(CodexError::InvalidPath)?;
    let login_keychain_lock_pattern = login_keychain_lock_pattern(login_keychain_parent)?;
    let login_keychain_sidecar_rules =
        login_keychain_sidecar_rules(login_keychain, login_keychain_access)?;
    let runtime = quote(runtime)?;
    let package_root = quote(package_root)?;
    let codex_home = quote(codex_home)?;
    let synthetic_home = quote(synthetic_home)?;
    let model_workspace = quote(model_workspace)?;
    let login_keychain = quote(login_keychain)?;
    let login_keychain_parent = quote(login_keychain_parent)?;
    let mut ancestor_paths = BTreeSet::new();
    for path in [
        runtime.as_str(),
        package_root.as_str(),
        codex_home.as_str(),
        synthetic_home.as_str(),
        model_workspace.as_str(),
    ] {
        let decoded: String = serde_json::from_str(path).map_err(|_| CodexError::InvalidPath)?;
        for ancestor in Path::new(&decoded).ancestors().skip(1) {
            if ancestor != Path::new("/") {
                ancestor_paths.insert(quote(ancestor)?);
            }
        }
    }
    let ancestor_literals = ancestor_paths
        .into_iter()
        .fold(String::new(), |mut output, path| {
            let _ = write!(output, " (literal {path})");
            output
        });
    let system_config_literals =
        CODEX_SYSTEM_CONFIG_PATHS
            .iter()
            .fold(String::new(), |mut output, path| {
                let _ = write!(output, " (literal \"{path}\")");
                output
            });
    let (login_keychain_protocol_read, login_keychain_parent_metadata, login_keychain_write) =
        match login_keychain_access {
            LoginKeychainAccess::ReadOnly => (String::new(), String::new(), String::new()),
            LoginKeychainAccess::LoginWriteOnly => (
                format!(" (regex #\"{login_keychain_lock_pattern}\")"),
                format!(" (literal {login_keychain_parent})"),
                format!(" (literal {login_keychain})"),
            ),
        };
    Ok(format!(
        "(version 1)\n\
         (deny default)\n\
         (allow process-info*)\n\
         (allow process-exec (literal {runtime}))\n\
         (allow file-read*\n\
           (literal \"/\")\n\
           (literal {runtime})\n\
           (literal \"/dev/null\")\n\
           (literal \"/dev/random\")\n\
           (literal \"/dev/urandom\")\n\
           (subpath \"/System\")\n\
           (subpath \"/usr/lib\")\n\
           (subpath \"/private/var/db/dyld\")\n\
           {system_config_literals}\n\
           (subpath {package_root})\n\
           (subpath {codex_home})\n\
           (subpath {synthetic_home})\n\
           (subpath {model_workspace})\n\
           (literal {login_keychain})\n\
           {login_keychain_protocol_read})\n\
         (allow file-read-metadata\n\
           (literal \"/var\")\n\
           (literal \"/etc\")\n\
           (literal \"/private/etc\")\n\
           (literal \"/private/etc/codex\")\n\
           (literal {login_keychain})\n\
           {login_keychain_parent_metadata}\n\
           {ancestor_literals})\n\
         (allow file-write*\n\
           (literal \"/dev/null\")\n\
           (subpath {codex_home})\n\
           {login_keychain_write})\n\
         {login_keychain_sidecar_rules}\
         (allow network-outbound)\n\
         (allow network-inbound\n\
           (local tcp \"localhost:1455\")\n\
           (local tcp \"localhost:1457\"))\n\
         (allow system-socket)\n\
         (allow mach-lookup)\n\
         (allow ipc-posix*)\n\
         (allow sysctl-read)\n\
         (allow signal (target self))\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        CODEX_BINARY_SHA256, CODEX_RG_SHA256, CODEX_SYSTEM_CONFIG_PATHS, CODEX_VERSION,
        LoginKeychainAccess, ensure_credential_file_absent, file_sha256, sandbox_profile,
        verify_rg_runtime,
    };
    use serde_json::json;
    use std::path::{Path, PathBuf};

    #[test]
    fn runtime_pin_is_exact() {
        assert_eq!(CODEX_VERSION, "0.144.0");
        assert_eq!(CODEX_BINARY_SHA256.len(), 64);
    }

    #[test]
    fn any_auth_json_filesystem_entry_is_rejected_without_reading_it() {
        let root = tempfile::tempdir().unwrap();
        ensure_credential_file_absent(root.path()).unwrap();
        std::fs::write(root.path().join("auth.json"), b"not inspected").unwrap();
        assert!(ensure_credential_file_absent(root.path()).is_err());
    }

    #[test]
    fn exact_previous_config_migrates_only_to_direct_keyring_policy() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        std::fs::write(&path, super::PREVIOUS_CONFIG).unwrap();
        super::ensure_exact_config(&path).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), super::CONFIG);
        assert!(super::CONFIG.contains("secret_auth_storage = false"));

        std::fs::write(&path, "cli_auth_credentials_store = \"auto\"\n").unwrap();
        assert!(super::ensure_exact_config(&path).is_err());
    }

    #[test]
    fn developer_id_rg_requires_an_exact_runtime_receipt() {
        let root = tempfile::tempdir().unwrap();
        let resources = root.path().join("Resources");
        let package = resources.join("Codex").join(CODEX_VERSION);
        let rg = package.join("codex-path/rg");
        let notices = resources.join("Notices");
        std::fs::create_dir_all(rg.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&notices).unwrap();
        std::fs::write(&rg, b"owner-signed-rg-fixture").unwrap();
        let runtime_sha = file_sha256(&rg).unwrap();
        write_runtime_receipt(&notices, &runtime_sha, CODEX_RG_SHA256);
        let package = std::fs::canonicalize(package).unwrap();
        verify_rg_runtime(&package).unwrap();

        std::fs::write(&rg, b"tampered-after-receipt").unwrap();
        assert!(verify_rg_runtime(&package).is_err());
    }

    #[test]
    fn developer_id_rg_rejects_a_receipt_with_a_false_upstream_pin() {
        let root = tempfile::tempdir().unwrap();
        let resources = root.path().join("Resources");
        let package = resources.join("Codex").join(CODEX_VERSION);
        let rg = package.join("codex-path/rg");
        let notices = resources.join("Notices");
        std::fs::create_dir_all(rg.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&notices).unwrap();
        std::fs::write(&rg, b"owner-signed-rg-fixture").unwrap();
        let runtime_sha = file_sha256(&rg).unwrap();
        write_runtime_receipt(&notices, &runtime_sha, &"0".repeat(64));
        let package = std::fs::canonicalize(package).unwrap();
        assert!(verify_rg_runtime(&package).is_err());
    }

    fn write_runtime_receipt(notices: &Path, runtime_sha: &str, upstream_sha: &str) {
        let receipt = json!({
            "schemaVersion": 1,
            "component": "openai/codex",
            "version": CODEX_VERSION,
            "upstreamRgSha256": upstream_sha,
            "runtimeRgSha256": runtime_sha,
            "signingIdentifier": "rg",
            "teamIdentifier": "UHDY2275L5",
            "cdhash": "ab".repeat(20),
        });
        std::fs::write(
            notices.join("CODEX-RUNTIME-RECEIPT.json"),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn sandbox_allows_only_exact_model_workspace_and_runtime_exec() {
        let profile = sandbox_profile(
            Path::new("/Bundle/OpenOpenCodex"),
            Path::new("/Bundle"),
            Path::new("/App/CodexHome"),
            Path::new("/App/SyntheticHome"),
            Path::new("/App/ModelInput/turn-1"),
            Path::new("/Keychains/login.keychain-db"),
            LoginKeychainAccess::ReadOnly,
        )
        .unwrap();
        assert!(profile.contains("(allow process-exec (literal \"/Bundle/OpenOpenCodex\"))"));
        assert!(profile.contains("(subpath \"/Bundle\")"));
        assert!(profile.contains("(subpath \"/App/ModelInput/turn-1\")"));
        assert!(profile.contains("(local tcp \"localhost:1455\")"));
        assert!(profile.contains("(local tcp \"localhost:1457\")"));
        assert!(!profile.contains("(allow network-inbound)"));
        assert!(!profile.contains("(local tcp \"*:*\")"));
        assert!(!profile.contains("(allow process-exec)"));
        assert!(!profile.contains("/Users/"));
        for path in CODEX_SYSTEM_CONFIG_PATHS {
            assert!(profile.contains(&format!("(literal \"{path}\")")));
        }
        assert!(!profile.contains("(subpath \"/etc\")"));
        assert!(!profile.contains("(subpath \"/private/etc\")"));
    }

    #[test]
    fn user_home_is_canonical_owned_and_under_users() {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap();
        if home.starts_with("/Users") {
            assert_eq!(super::canonical_user_home(&home).unwrap(), home);
        }
        assert!(super::canonical_user_home(Path::new("/private/tmp")).is_err());
        assert!(super::canonical_user_home(Path::new("/Users")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn keychain_path_rejects_symlinked_ancestors_and_final_entry() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(root.path()).unwrap();
        let outside = root.join("outside");
        std::fs::create_dir(&outside).unwrap();

        let home_with_library_link = root.join("home-library-link");
        std::fs::create_dir(&home_with_library_link).unwrap();
        let outside_library = outside.join("Library");
        std::fs::create_dir_all(outside_library.join("Keychains")).unwrap();
        std::fs::write(
            outside_library.join("Keychains/login.keychain-db"),
            b"encrypted",
        )
        .unwrap();
        symlink(&outside_library, home_with_library_link.join("Library")).unwrap();
        assert!(
            super::canonical_regular_file_beneath(
                &home_with_library_link,
                Path::new("Library/Keychains/login.keychain-db")
            )
            .is_err()
        );

        let home_with_keychains_link = root.join("home-keychains-link");
        std::fs::create_dir_all(home_with_keychains_link.join("Library")).unwrap();
        symlink(
            outside_library.join("Keychains"),
            home_with_keychains_link.join("Library/Keychains"),
        )
        .unwrap();
        assert!(
            super::canonical_regular_file_beneath(
                &home_with_keychains_link,
                Path::new("Library/Keychains/login.keychain-db")
            )
            .is_err()
        );

        let ordinary_home = root.join("ordinary-home");
        std::fs::create_dir_all(ordinary_home.join("Library/Keychains")).unwrap();
        let ordinary_keychain = ordinary_home.join("Library/Keychains/login.keychain-db");
        std::fs::write(&ordinary_keychain, b"encrypted").unwrap();
        assert_eq!(
            super::canonical_regular_file_beneath(
                &ordinary_home,
                Path::new("Library/Keychains/login.keychain-db")
            )
            .unwrap(),
            ordinary_keychain
        );
        std::fs::remove_file(&ordinary_keychain).unwrap();
        symlink(
            outside_library.join("Keychains/login.keychain-db"),
            &ordinary_keychain,
        )
        .unwrap();
        assert!(
            super::canonical_regular_file_beneath(
                &ordinary_home,
                Path::new("Library/Keychains/login.keychain-db")
            )
            .is_err()
        );
    }

    #[test]
    fn keychain_path_must_be_disjoint_from_every_writable_root() {
        let keychain = Path::new("/Users/owner/Library/Keychains/login.keychain-db");
        assert!(
            super::require_disjoint_paths(keychain, &[Path::new("/Users/owner/Library")]).is_err()
        );
        assert!(
            super::require_disjoint_paths(
                keychain,
                &[Path::new(
                    "/Users/owner/Library/Keychains/login.keychain-db/child"
                )]
            )
            .is_err()
        );
        super::require_disjoint_paths(
            keychain,
            &[Path::new(
                "/Users/owner/Library/Application Support/OpenOpen",
            )],
        )
        .unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn outer_sandbox_reads_exact_workspace_but_denies_sibling_canary() {
        let root = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(root.path()).unwrap();
        let codex_home = root.join("codex-home");
        let synthetic_home = root.join("synthetic-home");
        let workspace = root.join("model-input");
        for directory in [&codex_home, &synthetic_home, &workspace] {
            std::fs::create_dir(directory).unwrap();
        }
        let allowed = workspace.join("allowed.txt");
        let canary = root.join("outside-canary.txt");
        std::fs::write(&allowed, b"allowed").unwrap();
        std::fs::write(&canary, b"must stay unreadable").unwrap();
        let profile = sandbox_profile(
            Path::new("/bin/cat"),
            Path::new("/bin"),
            &codex_home,
            &synthetic_home,
            &workspace,
            &root.join("login.keychain-db"),
            LoginKeychainAccess::ReadOnly,
        )
        .unwrap();
        let allowed_result = std::process::Command::new("/usr/bin/sandbox-exec")
            .args(["-p", &profile, "/bin/cat"])
            .arg(&allowed)
            .env_clear()
            .env("CFFIXED_USER_HOME", &synthetic_home)
            .env("HOME", &synthetic_home)
            .env("TMPDIR", &codex_home)
            .current_dir(&workspace)
            .output()
            .unwrap();
        assert!(
            allowed_result.status.success(),
            "sandboxed allowed read failed: {}",
            String::from_utf8_lossy(&allowed_result.stderr)
        );
        assert_eq!(allowed_result.stdout, b"allowed");
        let denied_result = std::process::Command::new("/usr/bin/sandbox-exec")
            .args(["-p", &profile, "/bin/cat"])
            .arg(&canary)
            .env_clear()
            .env("CFFIXED_USER_HOME", &synthetic_home)
            .env("HOME", &synthetic_home)
            .env("TMPDIR", &codex_home)
            .current_dir(&workspace)
            .output()
            .unwrap();
        assert!(!denied_result.status.success());
        assert!(
            !denied_result
                .stdout
                .windows(4)
                .any(|bytes| bytes == b"must")
        );
        let system_canary_result = std::process::Command::new("/usr/bin/sandbox-exec")
            .args(["-p", &profile, "/bin/cat", "/etc/passwd"])
            .env_clear()
            .env("CFFIXED_USER_HOME", &synthetic_home)
            .env("HOME", &synthetic_home)
            .env("TMPDIR", &codex_home)
            .current_dir(&workspace)
            .output()
            .unwrap();
        assert!(!system_canary_result.status.success());
        assert!(system_canary_result.stdout.is_empty());
    }

    #[test]
    fn outer_sandbox_denies_process_fork() {
        let root = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(root.path()).unwrap();
        let codex_home = root.join("codex-home");
        let synthetic_home = root.join("synthetic-home");
        let workspace = root.join("model-input");
        for directory in [&codex_home, &synthetic_home, &workspace] {
            std::fs::create_dir(directory).unwrap();
        }
        let profile = sandbox_profile(
            Path::new("/bin/bash"),
            Path::new("/bin"),
            &codex_home,
            &synthetic_home,
            &workspace,
            &root.join("login.keychain-db"),
            LoginKeychainAccess::ReadOnly,
        )
        .unwrap();
        let login_keychain = root.join("login.keychain-db");
        std::fs::write(&login_keychain, b"encrypted-canary").unwrap();
        let denied_keychain_write = std::process::Command::new("/usr/bin/sandbox-exec")
            .args([
                "-p",
                &profile,
                "/bin/bash",
                "-c",
                "printf changed >> \"$1\"",
                "_",
            ])
            .arg(&login_keychain)
            .env_clear()
            .env("CFFIXED_USER_HOME", &synthetic_home)
            .env("HOME", &root)
            .env("TMPDIR", &codex_home)
            .current_dir(&workspace)
            .output()
            .unwrap();
        assert!(!denied_keychain_write.status.success());
        assert_eq!(std::fs::read(&login_keychain).unwrap(), b"encrypted-canary");
        let result = std::process::Command::new("/usr/bin/sandbox-exec")
            .args(["-p", &profile, "/bin/bash", "-c", ": | :"])
            .env_clear()
            .env("CFFIXED_USER_HOME", &synthetic_home)
            .env("HOME", &synthetic_home)
            .env("TMPDIR", &codex_home)
            .current_dir(&workspace)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(String::from_utf8_lossy(&result.stderr).contains("fork: Operation not permitted"));
    }

    #[cfg(target_os = "macos")]
    fn run_sandbox_bash(
        profile: &str,
        script: &str,
        argument: &Path,
        context: (&Path, &Path, &Path, &Path),
    ) -> std::process::Output {
        let (root, codex_home, synthetic_home, workspace) = context;
        std::process::Command::new("/usr/bin/sandbox-exec")
            .args(["-p", profile, "/bin/bash", "-c", script, "_"])
            .arg(argument)
            .env_clear()
            .env("CFFIXED_USER_HOME", synthetic_home)
            .env("HOME", root)
            .env("TMPDIR", codex_home)
            .current_dir(workspace)
            .output()
            .unwrap()
    }

    #[cfg(target_os = "macos")]
    fn run_sandbox_perl(
        profile: &str,
        script: &str,
        arguments: &[&Path],
        context: (&Path, &Path, &Path, &Path),
    ) -> std::process::Output {
        let (root, codex_home, synthetic_home, workspace) = context;
        std::process::Command::new("/usr/bin/sandbox-exec")
            .args(["-p", profile, "/usr/bin/perl", "-e", script])
            .args(arguments)
            .env_clear()
            .env("CFFIXED_USER_HOME", synthetic_home)
            .env("HOME", root)
            .env("TMPDIR", codex_home)
            .current_dir(workspace)
            .output()
            .unwrap()
    }

    #[cfg(target_os = "macos")]
    fn assert_login_sidecar_create_boundary(
        model_profile: &str,
        login_profile: &str,
        login_perl_profile: &str,
        keychain_directory: &Path,
        root: &Path,
        context: (&Path, &Path, &Path, &Path),
    ) {
        let sidecar = keychain_directory.join("login.keychain-db.sb-33f2115a-53phqP");
        let invalid_sidecar = keychain_directory.join("login.keychain-db.sb-33f2115-53phqP");
        let hardlink_target = root.join("hardlink-target");
        let attempt_create =
            |profile| run_sandbox_bash(profile, "printf x > \"$1\"", &sidecar, context);

        assert!(!attempt_create(model_profile).status.success());
        assert!(!sidecar.exists());
        let creation = attempt_create(login_profile);
        assert!(
            creation.status.success(),
            "exact sidecar create failed: {}",
            String::from_utf8_lossy(&creation.stderr)
        );
        assert_eq!(std::fs::read(&sidecar).unwrap(), b"x");
        let existing_write =
            run_sandbox_bash(login_profile, "printf changed > \"$1\"", &sidecar, context);
        assert!(!existing_write.status.success());
        assert_eq!(std::fs::read(&sidecar).unwrap(), b"x");
        std::fs::remove_file(&sidecar).unwrap();

        let invalid_creation = run_sandbox_bash(
            login_profile,
            "printf x > \"$1\"",
            &invalid_sidecar,
            context,
        );
        assert!(!invalid_creation.status.success());
        assert!(!invalid_sidecar.exists());

        std::fs::write(&hardlink_target, b"protected").unwrap();
        std::fs::hard_link(&hardlink_target, &sidecar).unwrap();
        assert!(!attempt_create(login_profile).status.success());
        let hardlink_write = run_sandbox_perl(
            login_perl_profile,
            r#"open(my $file, ">", $ARGV[0]) or exit 41; print $file "changed";"#,
            &[&sidecar],
            context,
        );
        assert!(!hardlink_write.status.success());
        let metadata_only = run_sandbox_perl(
            login_perl_profile,
            "exit(chmod(0600, $ARGV[0]) == 1 ? 0 : 42);",
            &[&sidecar],
            context,
        );
        assert!(metadata_only.status.success());
        assert_eq!(std::fs::read(&hardlink_target).unwrap(), b"protected");
        assert_eq!(std::fs::read(&sidecar).unwrap(), b"protected");
        let rename = run_sandbox_perl(
            login_perl_profile,
            "exit(rename($ARGV[0], $ARGV[1]) ? 0 : 43);",
            &[&sidecar, &keychain_directory.join("login.keychain-db")],
            context,
        );
        assert!(!rename.status.success());
        assert_eq!(std::fs::read(&hardlink_target).unwrap(), b"protected");
        assert_eq!(std::fs::read(&sidecar).unwrap(), b"protected");
        std::fs::remove_file(&sidecar).unwrap();
    }

    #[cfg(target_os = "macos")]
    fn run_security(
        arguments: &[&str],
        profile: Option<&str>,
        context: (&Path, &Path, &Path, &Path),
    ) -> std::process::Output {
        let (root, codex_home, synthetic_home, workspace) = context;
        let mut command = if let Some(profile) = profile {
            let mut command = std::process::Command::new("/usr/bin/sandbox-exec");
            command.args(["-p", profile, "/usr/bin/security"]);
            command
        } else {
            std::process::Command::new("/usr/bin/security")
        };
        command
            .args(arguments)
            .env_clear()
            .env("CFFIXED_USER_HOME", synthetic_home)
            .env("HOME", root)
            .env("TMPDIR", codex_home)
            .current_dir(workspace)
            .output()
            .unwrap()
    }

    #[cfg(target_os = "macos")]
    fn assert_security_framework_atomic_save(
        model_profile: &str,
        login_profile: &str,
        login_keychain: &Path,
        context: (&Path, &Path, &Path, &Path),
    ) {
        let keychain = login_keychain.to_str().unwrap();
        let password = "openopen-disposable-keychain-fixture";
        let create = run_security(
            &["create-keychain", "-p", password, keychain],
            None,
            context,
        );
        assert!(create.status.success());
        let unlock = run_security(
            &["unlock-keychain", "-p", password, keychain],
            None,
            context,
        );
        assert!(unlock.status.success());

        let denied = run_security(
            &[
                "add-generic-password",
                "-a",
                "model-denied",
                "-s",
                "openopen-model-denied",
                "-w",
                "fixture",
                keychain,
            ],
            Some(model_profile),
            context,
        );
        assert!(!denied.status.success());
        let added = run_security(
            &[
                "add-generic-password",
                "-a",
                "login-allowed",
                "-s",
                "openopen-login-allowed",
                "-w",
                "fixture",
                keychain,
            ],
            Some(login_profile),
            context,
        );
        assert!(
            added.status.success(),
            "Security.framework atomic save failed: {}",
            String::from_utf8_lossy(&added.stderr)
        );
        let found = run_security(
            &[
                "find-generic-password",
                "-a",
                "login-allowed",
                "-s",
                "openopen-login-allowed",
                "-w",
                keychain,
            ],
            None,
            context,
        );
        assert!(found.status.success());
        assert_eq!(found.stdout, b"fixture\n");
        assert!(
            std::fs::read_dir(login_keychain.parent().unwrap())
                .unwrap()
                .all(|entry| !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains(".sb-"))
        );
    }

    #[cfg(target_os = "macos")]
    fn assert_login_lock_read_boundary(
        model_profile: &str,
        login_profile: &str,
        keychain_directory: &Path,
        sibling: &Path,
        keychain_lock: &Path,
        non_keychain_lock: &Path,
        context: (&Path, &Path, &Path, &Path),
    ) {
        let attempt_read = |profile, path| {
            run_sandbox_bash(
                profile,
                "read -r value < \"$1\" || [[ -n \"$value\" ]]",
                path,
                context,
            )
        };
        assert!(!attempt_read(model_profile, keychain_lock).status.success());
        assert!(attempt_read(login_profile, keychain_lock).status.success());
        assert!(!attempt_read(login_profile, sibling).status.success());
        assert!(
            !attempt_read(login_profile, non_keychain_lock)
                .status
                .success()
        );
        let metadata =
            |profile| run_sandbox_bash(profile, "[[ -d \"$1\" ]]", keychain_directory, context);
        assert!(!metadata(model_profile).status.success());
        assert!(metadata(login_profile).status.success());
        assert_eq!(std::fs::read(sibling).unwrap(), b"sibling");
        assert_eq!(std::fs::read(keychain_lock).unwrap(), b"lock");
        assert_eq!(std::fs::read(non_keychain_lock).unwrap(), b"not-lock");
    }

    #[cfg(target_os = "macos")]
    fn keychain_test_profiles(
        codex_home: &Path,
        synthetic_home: &Path,
        workspace: &Path,
        login_keychain: &Path,
    ) -> (String, String, String, String, String) {
        let profile = |runtime, package_root, access| {
            sandbox_profile(
                Path::new(runtime),
                Path::new(package_root),
                codex_home,
                synthetic_home,
                workspace,
                login_keychain,
                access,
            )
            .unwrap()
        };
        (
            profile("/bin/bash", "/bin", LoginKeychainAccess::ReadOnly),
            profile("/bin/bash", "/bin", LoginKeychainAccess::LoginWriteOnly),
            profile(
                "/usr/bin/security",
                "/usr/bin",
                LoginKeychainAccess::ReadOnly,
            ),
            profile(
                "/usr/bin/security",
                "/usr/bin",
                LoginKeychainAccess::LoginWriteOnly,
            ),
            profile(
                "/usr/bin/perl",
                "/usr/bin",
                LoginKeychainAccess::LoginWriteOnly,
            ),
        )
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn login_keychain_write_is_exact_and_never_added_to_model_profile() {
        let root = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(root.path()).unwrap();
        let codex_home = root.join("codex-home");
        let synthetic_home = root.join("synthetic-home");
        let workspace = root.join("model-input");
        let keychain_directory = root.join("keychains");
        let login_keychain = keychain_directory.join("login.keychain-db");
        let sibling = keychain_directory.join("other.keychain-db");
        let keychain_lock = keychain_directory.join(".fl34AC2A0A");
        let non_keychain_lock = keychain_directory.join(".fl-not-hex");
        for directory in [
            &codex_home,
            &synthetic_home,
            &workspace,
            &keychain_directory,
        ] {
            std::fs::create_dir(directory).unwrap();
        }
        std::fs::write(&login_keychain, b"login").unwrap();
        std::fs::write(&sibling, b"sibling").unwrap();
        std::fs::write(&keychain_lock, b"lock").unwrap();
        std::fs::write(&non_keychain_lock, b"not-lock").unwrap();

        let (
            model_profile,
            login_profile,
            model_security_profile,
            login_security_profile,
            login_perl_profile,
        ) = keychain_test_profiles(&codex_home, &synthetic_home, &workspace, &login_keychain);
        let context = (
            root.as_path(),
            codex_home.as_path(),
            synthetic_home.as_path(),
            workspace.as_path(),
        );
        let attempt_write =
            |profile, path| run_sandbox_bash(profile, "printf x >> \"$1\"", path, context);

        assert_login_sidecar_create_boundary(
            &model_profile,
            &login_profile,
            &login_perl_profile,
            &keychain_directory,
            &root,
            context,
        );
        std::fs::remove_file(&login_keychain).unwrap();
        assert_security_framework_atomic_save(
            &model_security_profile,
            &login_security_profile,
            &login_keychain,
            context,
        );

        assert!(
            !attempt_write(&model_profile, &login_keychain)
                .status
                .success()
        );
        assert!(
            attempt_write(&login_profile, &login_keychain)
                .status
                .success()
        );
        assert!(!attempt_write(&login_profile, &sibling).status.success());
        assert!(
            !attempt_write(&login_profile, &keychain_lock)
                .status
                .success()
        );
        assert_login_lock_read_boundary(
            &model_profile,
            &login_profile,
            &keychain_directory,
            &sibling,
            &keychain_lock,
            &non_keychain_lock,
            context,
        );
    }
}
