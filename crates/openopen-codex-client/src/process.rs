use crate::{CodexError, wire::Transport};
use serde::Deserialize;
use serde_json::to_string;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
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
const CONFIG: &str = "forced_login_method = \"chatgpt\"\n\
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

pub(crate) fn spawn(config: &CodexRuntimeConfig) -> Result<(Transport, i32), CodexError> {
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
    let codex_home = canonical_directory(&config.codex_home, true)?;
    let synthetic_home = canonical_directory(&config.synthetic_home, true)?;
    let model_workspace = canonical_directory(&config.model_workspace, false)?;
    let sandbox_exec = canonical_regular_file(Path::new(SANDBOX_EXEC))?;
    if sandbox_exec != Path::new(SANDBOX_EXEC) {
        return Err(CodexError::SandboxUnavailable);
    }
    if codex_home
        .join("auth.json")
        .try_exists()
        .map_err(CodexError::Io)?
    {
        return Err(CodexError::CredentialFilePresent);
    }
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
    )?;

    let mut child = Command::new(sandbox_exec)
        .arg("-p")
        .arg(profile)
        .arg(&runtime)
        .arg("app-server")
        .env_clear()
        .env("CODEX_HOME", &codex_home)
        .env("CFFIXED_USER_HOME", &synthetic_home)
        .env("HOME", &synthetic_home)
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

fn sandbox_profile(
    runtime: &Path,
    package_root: &Path,
    codex_home: &Path,
    synthetic_home: &Path,
    model_workspace: &Path,
) -> Result<String, CodexError> {
    let quote = |path: &Path| {
        path.to_str()
            .ok_or(CodexError::InvalidPath)
            .and_then(|value| to_string(value).map_err(|_| CodexError::InvalidPath))
    };
    let runtime = quote(runtime)?;
    let package_root = quote(package_root)?;
    let codex_home = quote(codex_home)?;
    let synthetic_home = quote(synthetic_home)?;
    let model_workspace = quote(model_workspace)?;
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
           (subpath {package_root})\n\
           (subpath {codex_home})\n\
           (subpath {synthetic_home})\n\
           (subpath {model_workspace}))\n\
         (allow file-read-metadata (literal \"/var\"){ancestor_literals})\n\
         (allow file-write*\n\
           (literal \"/dev/null\")\n\
           (subpath {codex_home})\n\
           (subpath {synthetic_home}))\n\
         (allow network-outbound)\n\
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
        CODEX_BINARY_SHA256, CODEX_RG_SHA256, CODEX_VERSION, file_sha256, sandbox_profile,
        verify_rg_runtime,
    };
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn runtime_pin_is_exact() {
        assert_eq!(CODEX_VERSION, "0.144.0");
        assert_eq!(CODEX_BINARY_SHA256.len(), 64);
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
        )
        .unwrap();
        assert!(profile.contains("(allow process-exec (literal \"/Bundle/OpenOpenCodex\"))"));
        assert!(profile.contains("(subpath \"/Bundle\")"));
        assert!(profile.contains("(subpath \"/App/ModelInput/turn-1\")"));
        assert!(!profile.contains("(allow process-exec)"));
        assert!(!profile.contains("/Users/"));
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
        )
        .unwrap();
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
}
