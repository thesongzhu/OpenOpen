use openopen_deep_zip_worker::{
    DeepZipMemoryContext, DeepZipSupervisor, MAX_MEMORY_CONTEXT_BYTES,
    MAX_MEMORY_CONTEXT_CONVERSATIONS, MAX_MEMORY_CONTEXT_MESSAGES,
    MAX_MEMORY_CONTEXT_RESPONSE_BYTES, ScanError,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

struct ArchiveFixture {
    directory: TempDir,
    path: PathBuf,
}

fn conversation(title: &str, messages: &[(&str, &str)]) -> Value {
    let mapping = messages
        .iter()
        .enumerate()
        .map(|(index, (role, text))| {
            let parent = (index > 0).then(|| format!("node-{:03}", index - 1));
            (
                format!("node-{index:03}"),
                json!({
                    "parent": parent,
                    "message": {
                        "author": {"role": role},
                        "create_time": index,
                        "content": {"content_type": "text", "parts": [text]},
                    }
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    let current_node = messages
        .len()
        .checked_sub(1)
        .map(|index| format!("node-{index:03}"));
    json!({"title": title, "current_node": current_node, "mapping": mapping})
}

fn write_archive(entries: &[(&str, Vec<u8>)]) -> ArchiveFixture {
    let directory = tempfile::tempdir().expect("fixture directory");
    let path = directory.path().join("fixture.zip");
    let file = File::create(&path).expect("create archive");
    let mut writer = zip::ZipWriter::new(file);
    for (name, body) in entries {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .expect("start member");
        writer.write_all(body).expect("write member");
    }
    writer.finish().expect("finish archive");
    ArchiveFixture { directory, path }
}

fn context(path: &Path) -> Result<DeepZipMemoryContext, ScanError> {
    DeepZipSupervisor::new(env!("CARGO_BIN_EXE_openopen-deep-zip-worker")).scan_memory_context(path)
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[test]
fn singleton_context_is_structured_bounded_and_source_bound() {
    let conversations = serde_json::to_vec(&vec![
        conversation(
            "Prepare the demo",
            &[
                ("user", "Keep the setup short."),
                ("assistant", "Use three steps."),
            ],
        ),
        conversation(
            "Ignore tool output",
            &[
                ("tool", "private tool text"),
                ("system", "private system text"),
            ],
        ),
    ])
    .expect("serialize conversations");
    let profile = br#"{"name":"Synthetic"}"#.to_vec();
    let fixture = write_archive(&[
        ("conversations.json", conversations.clone()),
        ("profile.json", profile),
    ]);

    let result = context(&fixture.path).expect("memory context");
    assert!(result.is_valid());
    assert_eq!(result.conversations.len(), 1);
    let first = &result.conversations[0];
    assert_eq!(first.source_path, "conversations.json");
    assert_eq!(first.source_sha256, sha256(&conversations));
    assert_eq!(first.conversation_index, 0);
    assert_eq!(first.title, "Prepare the demo");
    assert_eq!(first.messages.len(), 2);
    assert_eq!(first.messages[0].role, "user");
    assert_eq!(first.messages[0].text, "Keep the setup short.");
    assert_eq!(first.messages[1].role, "assistant");
    assert_eq!(first.messages[1].text, "Use three steps.");
    let encoded = serde_json::to_string(&result).expect("encode context");
    assert!(!encoded.contains("private tool text"));
    assert!(!encoded.contains("private system text"));
    assert!(!encoded.contains("Synthetic"));
}

#[test]
fn split_members_emit_only_digest_bound_conversation_context() {
    let first = serde_json::to_vec(&vec![conversation(
        "First part",
        &[("user", "First user text")],
    )])
    .expect("first part");
    let second = serde_json::to_vec(&vec![conversation(
        "Second part",
        &[("assistant", "Second assistant text")],
    )])
    .expect("second part");
    let fixture = write_archive(&[
        ("conversations-001.json", second.clone()),
        ("conversations-000.json", first.clone()),
        (
            "other.json",
            br#"[{"message":"not conversation context"}]"#.to_vec(),
        ),
    ]);

    let result = context(&fixture.path).expect("split context");
    assert!(result.is_valid());
    assert_eq!(result.conversations.len(), 2);
    for (path, body) in [
        ("conversations-000.json", first),
        ("conversations-001.json", second),
    ] {
        let item = result
            .conversations
            .iter()
            .find(|item| item.source_path == path)
            .expect("source context");
        assert_eq!(item.source_sha256, sha256(&body));
    }
    assert!(
        result
            .conversations
            .iter()
            .all(|item| item.source_path != "other.json")
    );
}

#[test]
fn context_and_catalog_tampering_breaks_digest_validation() {
    let body = serde_json::to_vec(&vec![conversation(
        "Bound title",
        &[("user", "Bound body")],
    )])
    .expect("body");
    let fixture = write_archive(&[("conversations.json", body)]);
    let result = context(&fixture.path).expect("context");

    let mut changed_text = result.clone();
    changed_text.conversations[0].messages[0].text.push('!');
    assert!(!changed_text.is_valid());

    let mut changed_source = result.clone();
    changed_source.conversations[0].source_sha256[0] ^= 1;
    assert!(!changed_source.is_valid());

    let mut changed_catalog = result;
    changed_catalog.catalog_digest[0] ^= 1;
    assert!(!changed_catalog.is_valid());
}

#[test]
fn only_the_current_conversation_branch_is_emitted_and_cycles_fail_closed() {
    let mut selected = conversation(
        "Branch selection",
        &[
            ("user", "Selected question"),
            ("assistant", "Selected answer"),
        ],
    );
    selected["mapping"]["orphan"] = json!({
        "parent": "node-000",
        "message": {
            "author": {"role": "assistant"},
            "content": {"parts": ["Unselected alternate branch"]}
        }
    });
    let body = serde_json::to_vec(&vec![selected]).expect("branch fixture");
    let fixture = write_archive(&[("conversations.json", body)]);
    let result = context(&fixture.path).expect("selected branch context");
    let encoded = serde_json::to_string(&result).expect("encode context");
    assert!(encoded.contains("Selected question"));
    assert!(encoded.contains("Selected answer"));
    assert!(!encoded.contains("Unselected alternate branch"));

    let cycle = serde_json::to_vec(&vec![json!({
        "title": "Cycle",
        "current_node": "a",
        "mapping": {
            "a": {"parent": "b", "message": {"author": {"role": "user"}, "content": {"parts": ["a"]}}},
            "b": {"parent": "a", "message": {"author": {"role": "assistant"}, "content": {"parts": ["b"]}}}
        }
    })])
    .expect("cycle fixture");
    let fixture = write_archive(&[("conversations.json", cycle)]);
    assert_eq!(
        context(&fixture.path),
        Err(ScanError::UnsupportedArchiveLayout)
    );
}

#[test]
fn output_caps_are_hard_and_complete_archive_is_still_validated() {
    let long = "x".repeat(3_000);
    let conversations = (0..40)
        .map(|conversation_index| {
            let messages = (0..24)
                .map(|message_index| {
                    let role = if message_index % 2 == 0 {
                        "user"
                    } else {
                        "assistant"
                    };
                    (role, long.as_str())
                })
                .collect::<Vec<_>>();
            conversation(&format!("Conversation {conversation_index}"), &messages)
        })
        .collect::<Vec<_>>();
    let body = serde_json::to_vec(&conversations).expect("large synthetic body");
    let fixture = write_archive(&[("conversations.json", body)]);

    let result = context(&fixture.path).expect("bounded context");
    assert!(result.is_valid());
    assert!(result.conversations.len() <= MAX_MEMORY_CONTEXT_CONVERSATIONS);
    assert!(
        result
            .conversations
            .iter()
            .map(|value| value.messages.len())
            .sum::<usize>()
            <= MAX_MEMORY_CONTEXT_MESSAGES
    );
    assert!(result.context_bytes <= MAX_MEMORY_CONTEXT_BYTES as u64);
    assert!(
        serde_json::to_vec(&result).expect("bounded response").len()
            <= MAX_MEMORY_CONTEXT_RESPONSE_BYTES
    );
}

#[test]
fn memory_context_creates_no_extracted_files_and_honors_cancellation() {
    let body = serde_json::to_vec(&vec![conversation(
        "No extraction",
        &[("user", "Keep this in memory only")],
    )])
    .expect("body");
    let fixture = write_archive(&[("conversations.json", body)]);
    let before = directory_entries(fixture.directory.path());
    let supervisor = DeepZipSupervisor::new(env!("CARGO_BIN_EXE_openopen-deep-zip-worker"));
    let result = supervisor
        .scan_memory_context(&fixture.path)
        .expect("context");
    assert!(result.is_valid());
    assert_eq!(directory_entries(fixture.directory.path()), before);

    let cancelled = DeepZipSupervisor::new(env!("CARGO_BIN_EXE_openopen-deep-zip-worker"));
    cancelled.cancel();
    assert_eq!(
        cancelled.scan_memory_context(&fixture.path),
        Err(ScanError::Cancelled)
    );
}

#[test]
fn split_layout_gaps_and_conversations_without_messages_fail_closed() {
    let gap = write_archive(&[
        ("conversations-000.json", b"[]".to_vec()),
        ("conversations-002.json", b"[]".to_vec()),
    ]);
    assert_eq!(context(&gap.path), Err(ScanError::UnsupportedArchiveLayout));

    let no_messages = serde_json::to_vec(&vec![json!({
        "title": "No accepted messages",
        "current_node": "node",
        "mapping": {"node": {"parent": null, "message": {"author": {"role": "tool"}, "content": {"parts": ["hidden"]}}}}
    })])
    .expect("no-message fixture");
    let fixture = write_archive(&[("conversations.json", no_messages)]);
    assert_eq!(
        context(&fixture.path),
        Err(ScanError::UnsupportedArchiveLayout)
    );
}

fn directory_entries(path: &Path) -> Vec<String> {
    let mut entries = fs::read_dir(path)
        .expect("read fixture directory")
        .map(|entry| {
            entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    entries.sort();
    entries
}
