use openopen_deep_zip_worker::{
    CandidateDraft, CatalogEntry, DeepZipCatalog, PreviewError, PreviewSelection, PreviewSession,
    PreviewSessionState,
};
use sha2::{Digest, Sha256};

fn hash(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn catalog() -> DeepZipCatalog {
    DeepZipCatalog {
        archive_bytes: 200,
        archive_sha256: hash(b"synthetic archive"),
        total_compressed_bytes: 80,
        total_expanded_bytes: 120,
        entries: vec![
            CatalogEntry {
                path: "conversations-000.json".to_owned(),
                compressed_bytes: 60,
                expanded_bytes: 90,
                sha256: hash(b"synthetic conversations"),
                directory: false,
            },
            CatalogEntry {
                path: "profile.json".to_owned(),
                compressed_bytes: 20,
                expanded_bytes: 30,
                sha256: hash(b"synthetic profile"),
                directory: false,
            },
        ],
    }
}

fn candidate(id: &str, title: &str, path: &str, line: &str, private: &[u8]) -> CandidateDraft {
    let source = match path {
        "conversations-000.json" => b"synthetic conversations".as_slice(),
        "profile.json" => b"synthetic profile".as_slice(),
        _ => b"unknown".as_slice(),
    };
    CandidateDraft {
        id: id.to_owned(),
        title: title.to_owned(),
        rationale: format!("Why {title} may help"),
        source_entry_path: path.to_owned(),
        source_entry_sha256: hash(source),
        proposed_markdown_line: line.to_owned(),
        private_derived_bytes: private.to_vec(),
    }
}

fn candidates() -> Vec<CandidateDraft> {
    vec![
        candidate(
            "candidate-a",
            "Keep the opening concise",
            "conversations-000.json",
            "- Keep the opening concise.",
            b"private-a",
        ),
        candidate(
            "candidate-b",
            "Prepare one fallback",
            "profile.json",
            "- Prepare one fallback.",
            b"private-bb",
        ),
        candidate(
            "candidate-c",
            "Rehearse once",
            "conversations-000.json",
            "- Rehearse once.",
            b"private-ccc",
        ),
    ]
}

#[test]
fn preview_is_deterministic_bounded_and_source_bound() {
    let first = PreviewSession::new(&catalog(), candidates()).expect("first preview");
    let second = PreviewSession::new(&catalog(), candidates()).expect("second preview");
    let first_set = first.choice_set().expect("first set");
    let second_set = second.choice_set().expect("second set");
    assert_eq!(first_set, second_set);
    assert_eq!(first_set.cards.len(), 3);
    assert_eq!(first_set.free_description.key, "D");
    assert_eq!(first_set.free_description.title, "Something else");

    let too_many = vec![
        candidate("a", "A", "profile.json", "- A", b"a"),
        candidate("b", "B", "profile.json", "- B", b"b"),
        candidate("c", "C", "profile.json", "- C", b"c"),
        candidate("d", "D", "profile.json", "- D", b"d"),
    ];
    assert_eq!(
        PreviewSession::new(&catalog(), too_many).err(),
        Some(PreviewError::InvalidCandidates)
    );

    let mut wrong_source = candidates();
    wrong_source[0].source_entry_sha256 = hash(b"not the catalog member");
    assert_eq!(
        PreviewSession::new(&catalog(), wrong_source).err(),
        Some(PreviewError::SourceBindingMismatch)
    );
}

#[test]
fn candidate_selection_is_single_and_disposes_all_working_data() {
    let mut preview = PreviewSession::new(&catalog(), candidates()).expect("preview");
    let diff = preview
        .select(PreviewSelection::Candidate {
            candidate_id: "candidate-b".to_owned(),
        })
        .expect("selection");
    assert_eq!(diff.selection_id, "candidate-b");
    assert_eq!(diff.proposed_line, "- Prepare one fallback.");
    assert_eq!(preview.state(), PreviewSessionState::Selected);
    assert_eq!(
        preview.disposal_summary().candidates_disposed,
        candidates().len()
    );
    assert_eq!(preview.disposal_summary().private_bytes_disposed, 30);
    assert!(preview.disposal_summary().complete);
    assert_eq!(
        preview.choice_set().err(),
        Some(PreviewError::StateConflict)
    );
    assert_eq!(
        preview
            .select(PreviewSelection::Candidate {
                candidate_id: "candidate-a".to_owned(),
            })
            .err(),
        Some(PreviewError::StateConflict)
    );
    let serialized = serde_json::to_string(&diff).expect("serialize diff");
    assert!(!serialized.contains("private-a"));
    assert!(!serialized.contains("private-bb"));
    assert!(!serialized.contains("private-ccc"));
    assert!(!serialized.contains("conversations-000.json"));
    assert!(!serialized.contains("profile.json"));
}

#[test]
fn free_description_edit_is_digest_bound_and_preserves_one_selection() {
    let mut preview = PreviewSession::new(&catalog(), candidates()).expect("preview");
    let selected = preview
        .select(PreviewSelection::FreeDescription {
            markdown_line: "- Ask one clarifying question.".to_owned(),
        })
        .expect("D selection");
    assert_eq!(selected.selection_id, "D");
    assert_eq!(
        preview
            .edit_markdown_line("stale", "- Keep it shorter.".to_owned())
            .err(),
        Some(PreviewError::DigestMismatch)
    );
    let edited = preview
        .edit_markdown_line(&selected.diff_digest, "- Keep it shorter.".to_owned())
        .expect("edit");
    assert_eq!(edited.revision, 2);
    assert_ne!(edited.diff_digest, selected.diff_digest);
    assert_eq!(
        preview
            .edit_markdown_line(&edited.diff_digest, "two\nlines".to_owned())
            .err(),
        Some(PreviewError::StateConflict)
    );
}

#[test]
fn confirmation_and_readback_are_exact_and_idempotent() {
    let mut preview = PreviewSession::new(&catalog(), candidates()).expect("preview");
    let diff = preview
        .select(PreviewSelection::Candidate {
            candidate_id: "candidate-a".to_owned(),
        })
        .expect("selection");
    assert_eq!(
        preview.confirm("stale").err(),
        Some(PreviewError::DigestMismatch)
    );
    let confirmation = preview.confirm(&diff.diff_digest).expect("confirmation");
    assert_eq!(
        preview
            .confirm(&diff.diff_digest)
            .expect("confirmation replay"),
        confirmation
    );
    assert_eq!(
        preview
            .verify_readback(&confirmation.confirmation_digest, "- Different.")
            .err(),
        Some(PreviewError::ReadbackMismatch)
    );
    let receipt = preview
        .verify_readback(&confirmation.confirmation_digest, &confirmation.edited_line)
        .expect("readback");
    assert_eq!(preview.state(), PreviewSessionState::ReadBack);
    assert_eq!(
        preview
            .verify_readback(&confirmation.confirmation_digest, &confirmation.edited_line)
            .expect("readback replay"),
        receipt
    );
}

#[test]
fn cancel_disposes_candidates_without_creating_a_selection() {
    let mut preview = PreviewSession::new(&catalog(), candidates()).expect("preview");
    let disposal = preview.cancel().expect("cancel");
    assert!(disposal.complete);
    assert_eq!(disposal.candidates_disposed, 3);
    assert_eq!(preview.state(), PreviewSessionState::Cancelled);
    assert_eq!(preview.cancel().err(), Some(PreviewError::StateConflict));
}
