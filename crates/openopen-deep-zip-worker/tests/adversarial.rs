use openopen_deep_zip_worker::{
    CancellationToken, DeepZipSupervisor, EntryKindViolation, FrozenLimits, PathViolation,
    ScanError,
};
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

#[derive(Clone, Copy)]
enum FixtureKind {
    File,
    Directory,
}

struct Fixture<'a> {
    name: &'a str,
    body: &'a [u8],
    compression: CompressionMethod,
    kind: FixtureKind,
}

struct ArchiveFixture {
    _directory: TempDir,
    path: PathBuf,
}

fn write_archive(entries: &[Fixture<'_>]) -> ArchiveFixture {
    let directory = tempfile::tempdir().expect("fixture directory");
    let path = directory.path().join("fixture.zip");
    let file = File::create(&path).expect("create fixture");
    let mut writer = zip::ZipWriter::new(file);
    for entry in entries {
        let options = SimpleFileOptions::default().compression_method(entry.compression);
        match entry.kind {
            FixtureKind::File => {
                writer.start_file(entry.name, options).expect("start file");
                writer.write_all(entry.body).expect("write body");
            }
            FixtureKind::Directory => writer
                .add_directory(entry.name, options)
                .expect("add directory"),
        }
    }
    writer.finish().expect("finish fixture");
    ArchiveFixture {
        _directory: directory,
        path,
    }
}

fn nested_zip_bytes() -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("nested.json", SimpleFileOptions::default())
        .expect("nested entry");
    writer.write_all(b"{}").expect("nested body");
    writer.finish().expect("nested finish").into_inner()
}

fn scan(path: &Path) -> Result<openopen_deep_zip_worker::DeepZipCatalog, ScanError> {
    DeepZipSupervisor::new(env!("CARGO_BIN_EXE_openopen-deep-zip-worker")).scan(path)
}

#[test]
fn valid_catalog_is_sorted_and_content_bound() {
    let fixture = write_archive(&[
        Fixture {
            name: "conversations.json",
            body: br#"[{"title":"bounded"}]"#,
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        },
        Fixture {
            name: "data/",
            body: b"",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::Directory,
        },
        Fixture {
            name: "data/profile.json",
            body: br#"{"kind":"profile"}"#,
            compression: CompressionMethod::Deflated,
            kind: FixtureKind::File,
        },
    ]);

    let catalog = scan(&fixture.path).expect("catalog");
    assert_eq!(catalog.entries.len(), 3);
    assert_eq!(catalog.entries[0].path, "conversations.json");
    assert_eq!(catalog.entries[1].path, "data/");
    assert_eq!(catalog.entries[2].path, "data/profile.json");
    assert!(catalog.archive_bytes > 0);
    assert_ne!(catalog.archive_sha256, [0; 32]);
    assert_ne!(catalog.entries[0].sha256, [0; 32]);
}

#[test]
fn valid_streaming_data_descriptor_archive_is_authenticated() {
    for compression in [CompressionMethod::Stored, CompressionMethod::Deflated] {
        let mut writer = zip::ZipWriter::new_stream(Vec::new());
        writer
            .start_file(
                "conversations.json",
                SimpleFileOptions::default().compression_method(compression),
            )
            .expect("start streaming entry");
        writer
            .write_all(br#"[{"title":"streaming"}]"#)
            .expect("write streaming entry");
        let bytes = writer.finish().expect("finish streaming zip").into_inner();
        let directory = tempfile::tempdir().expect("streaming directory");
        let path = directory.path().join("streaming.zip");
        fs::write(&path, bytes).expect("write streaming zip");

        let catalog = scan(&path).expect("streaming catalog");
        assert_eq!(catalog.entries.len(), 1);
        assert_eq!(catalog.entries[0].path, "conversations.json");
    }
}

#[test]
fn split_conversation_members_are_contiguous_zero_based_and_array_shaped() {
    let valid = write_archive(&[
        Fixture {
            name: "conversations-001.json",
            body: br#"[{"title":"second"}]"#,
            compression: CompressionMethod::Deflated,
            kind: FixtureKind::File,
        },
        Fixture {
            name: "conversations-000.json",
            body: br#"[{"title":"first"}]"#,
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        },
    ]);
    let catalog = scan(&valid.path).expect("split conversation catalog");
    assert_eq!(catalog.entries[0].path, "conversations-000.json");
    assert_eq!(catalog.entries[1].path, "conversations-001.json");

    for entries in [
        vec![
            Fixture {
                name: "conversations.json",
                body: b"[]",
                compression: CompressionMethod::Stored,
                kind: FixtureKind::File,
            },
            Fixture {
                name: "conversations-000.json",
                body: b"[]",
                compression: CompressionMethod::Stored,
                kind: FixtureKind::File,
            },
        ],
        vec![Fixture {
            name: "conversations-001.json",
            body: b"[]",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        }],
        vec![
            Fixture {
                name: "conversations-000.json",
                body: b"[]",
                compression: CompressionMethod::Stored,
                kind: FixtureKind::File,
            },
            Fixture {
                name: "conversations-002.json",
                body: b"[]",
                compression: CompressionMethod::Stored,
                kind: FixtureKind::File,
            },
        ],
        vec![Fixture {
            name: "conversations-00.json",
            body: b"[]",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        }],
    ] {
        let invalid = write_archive(&entries);
        assert_eq!(
            scan(&invalid.path),
            Err(ScanError::UnsupportedArchiveLayout)
        );
    }

    let object_part = write_archive(&[Fixture {
        name: "conversations-000.json",
        body: br#"{"title":"not an array"}"#,
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    assert_eq!(
        scan(&object_part.path),
        Err(ScanError::UnsupportedMemberContent { index: 0 })
    );
}

#[test]
fn traversal_absolute_backslash_and_duplicate_paths_fail_closed() {
    for (name, violation) in [
        ("../escape.json", PathViolation::Traversal),
        ("/absolute.json", PathViolation::Absolute),
        ("C:/drive.json", PathViolation::Absolute),
        ("folder\\escape.json", PathViolation::Backslash),
    ] {
        let fixture = write_archive(&[Fixture {
            name,
            body: b"x",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        }]);
        assert_eq!(
            scan(&fixture.path),
            Err(ScanError::InvalidPath {
                index: 0,
                violation,
            })
        );
    }

    let duplicate = write_archive(&[
        Fixture {
            name: "same.json",
            body: b"first",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        },
        Fixture {
            name: "same.json/",
            body: b"",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::Directory,
        },
    ]);
    assert_eq!(
        scan(&duplicate.path),
        Err(ScanError::InvalidPath {
            index: 1,
            violation: PathViolation::Duplicate,
        })
    );
}

#[test]
fn unicode_casefold_nfc_and_canonical_separator_collisions_fail_closed() {
    for (first, second) in [
        ("A.json", "a.json"),
        ("\u{c4}.json", "\u{e4}.json"),
        ("Stra\u{df}e.json", "STRASSE.json"),
        ("caf\u{e9}.json", "cafe\u{301}.json"),
        ("a/b.json", "a//b.json"),
    ] {
        let fixture = write_archive(&[
            Fixture {
                name: first,
                body: b"{}",
                compression: CompressionMethod::Stored,
                kind: FixtureKind::File,
            },
            Fixture {
                name: second,
                body: b"{}",
                compression: CompressionMethod::Stored,
                kind: FixtureKind::File,
            },
        ]);
        assert_eq!(
            scan(&fixture.path),
            Err(ScanError::InvalidPath {
                index: 1,
                violation: PathViolation::Duplicate,
            })
        );
    }

    let normalized = write_archive(&[
        Fixture {
            name: "conversations.json",
            body: b"[]",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        },
        Fixture {
            name: "folder//cafe\u{301}.json",
            body: b"{}",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        },
    ]);
    let catalog = scan(&normalized.path).expect("normalized catalog");
    assert_eq!(catalog.entries[1].path, "folder/caf\u{e9}.json");
}

#[test]
fn file_ancestor_collisions_use_canonical_component_identity() {
    for (ancestor, descendant) in [
        ("Folder.json", "Folder.json/child.json"),
        ("Folder.json", "folder.JSON/child.json"),
        ("caf\u{e9}.json", "cafe\u{301}.JSON/child.json"),
    ] {
        for file_first in [true, false] {
            let (first, second) = if file_first {
                (ancestor, descendant)
            } else {
                (descendant, ancestor)
            };
            let fixture = write_archive(&[
                Fixture {
                    name: "conversations.json",
                    body: b"[]",
                    compression: CompressionMethod::Stored,
                    kind: FixtureKind::File,
                },
                Fixture {
                    name: first,
                    body: b"{}",
                    compression: CompressionMethod::Stored,
                    kind: FixtureKind::File,
                },
                Fixture {
                    name: second,
                    body: b"{}",
                    compression: CompressionMethod::Stored,
                    kind: FixtureKind::File,
                },
            ]);
            assert_eq!(
                scan(&fixture.path),
                Err(ScanError::InvalidPath {
                    index: 2,
                    violation: PathViolation::Duplicate,
                })
            );
        }
    }
}

#[test]
fn explicit_and_implicit_canonical_directories_agree() {
    for (directory, descendant) in [
        ("Folder/", "folder/child.json"),
        ("caf\u{e9}/", "cafe\u{301}/child.json"),
    ] {
        for directory_first in [true, false] {
            let (first_name, first_kind, first_body, second_name, second_kind, second_body) =
                if directory_first {
                    (
                        directory,
                        FixtureKind::Directory,
                        b"" as &[u8],
                        descendant,
                        FixtureKind::File,
                        b"{}" as &[u8],
                    )
                } else {
                    (
                        descendant,
                        FixtureKind::File,
                        b"{}" as &[u8],
                        directory,
                        FixtureKind::Directory,
                        b"" as &[u8],
                    )
                };
            let fixture = write_archive(&[
                Fixture {
                    name: "conversations.json",
                    body: b"[]",
                    compression: CompressionMethod::Stored,
                    kind: FixtureKind::File,
                },
                Fixture {
                    name: first_name,
                    body: first_body,
                    compression: CompressionMethod::Stored,
                    kind: first_kind,
                },
                Fixture {
                    name: second_name,
                    body: second_body,
                    compression: CompressionMethod::Stored,
                    kind: second_kind,
                },
            ]);
            let catalog = scan(&fixture.path).expect("explicit and implicit directory agreement");
            assert_eq!(catalog.entries.len(), 3);
        }
    }
}

#[test]
fn controls_line_separators_and_bidi_formats_fail_closed() {
    for name in [
        "nul\0.json",
        "line\nfeed.json",
        "delete\u{7f}.json",
        "c1\u{85}.json",
        "line\u{2028}separator.json",
        "paragraph\u{2029}separator.json",
        "override\u{202e}gpj.json",
        "isolate\u{2066}name.json",
        "mark\u{200f}name.json",
    ] {
        let fixture = write_archive(&[Fixture {
            name,
            body: b"{}",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        }]);
        assert_eq!(
            scan(&fixture.path),
            Err(ScanError::InvalidPath {
                index: 0,
                violation: PathViolation::Control,
            })
        );
    }
}

#[test]
fn invalid_utf8_path_fails_closed() {
    let fixture = write_archive(&[Fixture {
        name: "entry.json",
        body: b"body",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    patch_first_name_as_invalid_utf8(&fixture.path);
    assert_eq!(
        scan(&fixture.path),
        Err(ScanError::InvalidPath {
            index: 0,
            violation: PathViolation::InvalidUtf8,
        })
    );
}

#[test]
fn path_byte_and_depth_limits_are_exact() {
    let accepted_name = format!(
        "{}.json",
        "a".repeat(FrozenLimits::MAX_PATH_BYTES - ".json".len())
    );
    let accepted = write_archive(&[
        Fixture {
            name: "conversations.json",
            body: b"[]",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        },
        Fixture {
            name: &accepted_name,
            body: b"{}",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        },
    ]);
    scan(&accepted.path).expect("512-byte path");

    let rejected_name = "a".repeat(FrozenLimits::MAX_PATH_BYTES + 1);
    let rejected = write_archive(&[Fixture {
        name: &rejected_name,
        body: b"x",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    assert_eq!(
        scan(&rejected.path),
        Err(ScanError::InvalidPath {
            index: 0,
            violation: PathViolation::TooLong,
        })
    );

    let accepted_depth = format!("{}/value.json", vec!["a"; 15].join("/"));
    let accepted = write_archive(&[
        Fixture {
            name: "conversations.json",
            body: b"[]",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        },
        Fixture {
            name: &accepted_depth,
            body: b"{}",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        },
    ]);
    scan(&accepted.path).expect("depth 16");

    let rejected_depth = format!("{}/value.json", vec!["a"; 16].join("/"));
    let rejected = write_archive(&[Fixture {
        name: &rejected_depth,
        body: b"x",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    assert_eq!(
        scan(&rejected.path),
        Err(ScanError::InvalidPath {
            index: 0,
            violation: PathViolation::TooDeep,
        })
    );
}

#[test]
fn nested_archive_extension_and_magic_fail_closed() {
    let extension = write_archive(&[Fixture {
        name: "nested.ZIP",
        body: b"not even parsed",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    assert_eq!(
        scan(&extension.path),
        Err(ScanError::NestedArchive { index: 0 })
    );

    let nested = nested_zip_bytes();
    let mut disguised = b"<!doctype html><html><body>".to_vec();
    disguised.extend_from_slice(&nested);
    disguised.extend_from_slice(b"</body></html>");
    let magic = write_archive(&[Fixture {
        name: "opaque.html",
        body: &disguised,
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    assert_eq!(
        scan(&magic.path),
        Err(ScanError::NestedArchive { index: 0 })
    );
}

#[test]
fn padded_and_neutral_extension_nested_archives_fail_closed() {
    let nested = nested_zip_bytes();
    let mut padded = b"<!doctype html>".to_vec();
    padded.extend(std::iter::repeat_n(b'x', 2_048));
    padded.extend_from_slice(&nested);
    let fixture = write_archive(&[Fixture {
        name: "attachment.html",
        body: &padded,
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    assert_eq!(
        scan(&fixture.path),
        Err(ScanError::NestedArchive { index: 0 })
    );

    for name in [
        "nested.docx",
        "nested.iso",
        "nested.zst",
        "nested.apk",
        "nested.epub",
    ] {
        let extension = write_archive(&[Fixture {
            name,
            body: b"payload",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        }]);
        assert_eq!(
            scan(&extension.path),
            Err(ScanError::NestedArchive { index: 0 })
        );
    }
}

#[test]
fn structural_archive_detection_does_not_reject_chatgpt_json_words() {
    let fixture = write_archive(&[Fixture {
        name: "conversations.json",
        body: br#"[{"title":"mustard ustar CD001 koly MSCF BZh xar!"}]"#,
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    let catalog = scan(&fixture.path).expect("ordinary JSON text");
    assert_eq!(catalog.entries.len(), 1);
}

#[test]
fn structurally_valid_tar_under_a_text_extension_fails_closed() {
    let tar = minimal_tar_bytes();
    let fixture = write_archive(&[Fixture {
        name: "opaque.json",
        body: &tar,
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    assert_eq!(
        scan(&fixture.path),
        Err(ScanError::NestedArchive { index: 0 })
    );
}

#[test]
fn bounded_chatgpt_member_allowlist_rejects_unknown_binary_and_bad_content() {
    let supported = write_archive(&[
        Fixture {
            name: "conversations.json",
            body: b"[]",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        },
        Fixture {
            name: "chat.html",
            body: b"<!doctype html><html><body>history</body></html>\n",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        },
    ]);
    assert_eq!(scan(&supported.path).expect("supported").entries.len(), 2);

    let unknown = write_archive(&[Fixture {
        name: "opaque.bin",
        body: b"PK\x03\x04 archive-like unknown binary",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    assert_eq!(
        scan(&unknown.path),
        Err(ScanError::UnsupportedMember { index: 0 })
    );

    let bad_magic = write_archive(&[Fixture {
        name: "chat.html",
        body: b"not html",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    assert_eq!(
        scan(&bad_magic.path),
        Err(ScanError::UnsupportedMemberContent { index: 0 })
    );

    let control_html = write_archive(&[Fixture {
        name: "chat.html",
        body: b"<!doctype html><html>\0</html>",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    assert_eq!(
        scan(&control_html.path),
        Err(ScanError::UnsupportedMemberContent { index: 0 })
    );

    let malformed_json = write_archive(&[Fixture {
        name: "conversations.json",
        body: b"[",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    assert_eq!(
        scan(&malformed_json.path),
        Err(ScanError::UnsupportedMemberContent { index: 0 })
    );

    let missing_core = write_archive(&[Fixture {
        name: "user.json",
        body: b"{}",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    assert_eq!(
        scan(&missing_core.path),
        Err(ScanError::UnsupportedArchiveLayout)
    );
}

#[test]
fn outer_zip_preamble_and_trailer_polyglots_fail_closed() {
    let fixture = write_archive(&[Fixture {
        name: "conversations.json",
        body: b"[]",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    let original = fs::read(&fixture.path).expect("read fixture");
    let directory = tempfile::tempdir().expect("polyglot directory");

    let preamble_path = directory.path().join("preamble.zip");
    let mut preamble = b"#!/bin/sh\n".to_vec();
    preamble.extend_from_slice(&original);
    fs::write(&preamble_path, preamble).expect("write preamble fixture");
    assert_eq!(scan(&preamble_path), Err(ScanError::InvalidArchive));

    let trailer_path = directory.path().join("trailer.zip");
    let mut trailer = original;
    trailer.extend_from_slice(b"trailing-polyglot");
    fs::write(&trailer_path, trailer).expect("write trailer fixture");
    assert_eq!(scan(&trailer_path), Err(ScanError::InvalidArchive));
}

#[test]
fn duplicate_decoded_central_members_are_rejected_before_library_collapse() {
    let fixture = write_archive(&[
        Fixture {
            name: "a.json",
            body: b"{}",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        },
        Fixture {
            name: "b.json",
            body: b"{}",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        },
    ]);
    patch_entry_name(&fixture.path, 1, b"a.json");
    assert_eq!(
        scan(&fixture.path),
        Err(ScanError::InvalidPath {
            index: 1,
            violation: PathViolation::Duplicate,
        })
    );
}

#[test]
fn dual_eocd_differing_mode_library_fallback_fails_closed() {
    let fixture = write_archive(&[Fixture {
        name: "conversations.json",
        body: b"[]",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    let (embedded_central, final_central) = forge_dual_eocd_symlink_fallback(&fixture.path);

    let mut library =
        zip::ZipArchive::new(File::open(&fixture.path).expect("open fallback fixture"))
            .expect("zip 6.0.0 falls back to embedded central directory");
    assert_eq!(library.offset(), 0);
    assert_eq!(library.central_directory_start(), embedded_central);
    assert_ne!(library.central_directory_start(), final_central);
    assert!(!library.by_index(0).expect("fallback entry").is_symlink());
    drop(library);

    assert_eq!(scan(&fixture.path), Err(ScanError::InvalidArchive));
}

#[test]
fn unsupported_and_method_incompatible_zip_flags_fail_closed() {
    for bit in [0_u16, 4, 5, 6, 7, 8, 9, 10, 12, 13, 14, 15] {
        let fixture = write_archive(&[Fixture {
            name: "conversations.json",
            body: b"[]",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        }]);
        add_entry_flags(&fixture.path, 0, 1 << bit);
        assert_eq!(scan(&fixture.path), Err(ScanError::InvalidArchive));
    }

    for bit in [1_u16, 2] {
        let fixture = write_archive(&[Fixture {
            name: "conversations.json",
            body: b"[]",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        }]);
        add_entry_flags(&fixture.path, 0, 1 << bit);
        assert_eq!(scan(&fixture.path), Err(ScanError::InvalidArchive));
    }

    let missing_descriptor = write_archive(&[Fixture {
        name: "conversations.json",
        body: b"[]",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    add_entry_flags(&missing_descriptor.path, 0, 1 << 3);
    assert_eq!(
        scan(&missing_descriptor.path),
        Err(ScanError::InvalidArchive)
    );

    for deflate_options in [1 << 1, 1 << 2, (1 << 1) | (1 << 2)] {
        let fixture = write_archive(&[Fixture {
            name: "conversations.json",
            body: br#"[{"title":"flags"}]"#,
            compression: CompressionMethod::Deflated,
            kind: FixtureKind::File,
        }]);
        add_entry_flags(&fixture.path, 0, deflate_options);
        scan(&fixture.path).expect("deflate option bits are method-compatible");
    }
}

#[test]
fn extra_fields_are_semantically_allowlisted() {
    let extended_timestamp = [0x55, 0x54, 0x05, 0x00, 0x01, 0, 0, 0, 0];
    let valid = write_archive(&[Fixture {
        name: "conversations.json",
        body: b"[]",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    add_single_entry_extras(&valid.path, &extended_timestamp, &extended_timestamp);
    scan(&valid.path).expect("semantically valid timestamp extra field");

    let mut ntfs_timestamp = [0_u8; 36];
    ntfs_timestamp[..2].copy_from_slice(&0x000a_u16.to_le_bytes());
    ntfs_timestamp[2..4].copy_from_slice(&32_u16.to_le_bytes());
    ntfs_timestamp[8..10].copy_from_slice(&0x0001_u16.to_le_bytes());
    ntfs_timestamp[10..12].copy_from_slice(&24_u16.to_le_bytes());
    let valid_ntfs = write_archive(&[Fixture {
        name: "conversations.json",
        body: b"[]",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    add_single_entry_extras(&valid_ntfs.path, &ntfs_timestamp, &ntfs_timestamp);
    scan(&valid_ntfs.path).expect("semantically valid NTFS timestamp extra field");

    let unknown = write_archive(&[Fixture {
        name: "conversations.json",
        body: b"[]",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    let unknown_field = [0xfe, 0xca, 0x00, 0x00];
    add_single_entry_extras(&unknown.path, &unknown_field, &unknown_field);
    assert_eq!(scan(&unknown.path), Err(ScanError::InvalidArchive));

    let malformed_local = write_archive(&[Fixture {
        name: "conversations.json",
        body: b"[]",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    let malformed_timestamp = [0x55, 0x54, 0x05, 0x00, 0x03, 0, 0, 0, 0];
    add_single_entry_extras(
        &malformed_local.path,
        &malformed_timestamp,
        &extended_timestamp,
    );
    assert_eq!(scan(&malformed_local.path), Err(ScanError::InvalidArchive));
}

#[test]
fn central_file_comments_are_rejected_instead_of_skipped() {
    let fixture = write_archive(&[Fixture {
        name: "conversations.json",
        body: b"[]",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    add_single_entry_comment(&fixture.path, b"opaque central metadata");

    let mut library =
        zip::ZipArchive::new(File::open(&fixture.path).expect("open comment fixture"))
            .expect("zip accepts a central file comment");
    assert_eq!(
        library.by_index(0).expect("commented entry").comment(),
        "opaque central metadata"
    );
    drop(library);
    assert_eq!(scan(&fixture.path), Err(ScanError::InvalidArchive));
}

#[test]
fn forged_central_and_local_sizes_and_reused_ranges_fail_closed() {
    let central_only = write_archive(&[Fixture {
        name: "entry.json",
        body: b"{}",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    patch_central_sizes(&central_only.path, 0, 1, 1);
    assert_eq!(scan(&central_only.path), Err(ScanError::InvalidArchive));

    let central_and_local = write_archive(&[Fixture {
        name: "entry.json",
        body: b"{}",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    patch_central_and_local_sizes(&central_and_local.path, 0, 1, 1);
    assert_eq!(
        scan(&central_and_local.path),
        Err(ScanError::InvalidArchive)
    );

    let padded_deflate = write_archive(&[Fixture {
        name: "conversations.json",
        body: br#"[{"title":"compressed"}]"#,
        compression: CompressionMethod::Deflated,
        kind: FixtureKind::File,
    }]);
    pad_first_deflate_stream(&padded_deflate.path, b"hidden-padding");
    assert_eq!(scan(&padded_deflate.path), Err(ScanError::InvalidArchive));

    let reused = write_archive(&[
        Fixture {
            name: "first.json",
            body: b"{}",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        },
        Fixture {
            name: "other.json",
            body: b"{}",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        },
    ]);
    reuse_local_record(&reused.path, 0, 1);
    assert_eq!(scan(&reused.path), Err(ScanError::InvalidArchive));
}

#[test]
fn crc_corruption_and_truncation_fail_closed_without_a_catalog() {
    let corrupted = write_archive(&[
        Fixture {
            name: "conversations.json",
            body: b"[]",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        },
        Fixture {
            name: "profile.json",
            body: b"{}",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        },
    ]);
    patch_stored_entry_body(&corrupted.path, 1, b"[]");
    assert_eq!(scan(&corrupted.path), Err(ScanError::InvalidArchive));

    let truncated = write_archive(&[Fixture {
        name: "conversations.json",
        body: b"[]",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    let mut bytes = fs::read(&truncated.path).expect("read truncation fixture");
    bytes.pop();
    fs::write(&truncated.path, bytes).expect("truncate fixture");
    assert_eq!(scan(&truncated.path), Err(ScanError::InvalidArchive));
}

#[test]
fn source_open_rejects_symlink_directory_and_fifo_without_blocking() {
    let fixture = write_archive(&[Fixture {
        name: "conversations.json",
        body: b"[]",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    let directory = tempfile::tempdir().expect("directory");
    let link = directory.path().join("source-link.zip");
    symlink(&fixture.path, &link).expect("symlink");
    assert_eq!(scan(&link), Err(ScanError::ArchiveRead));
    assert_eq!(scan(directory.path()), Err(ScanError::ArchiveNotRegular));

    let fifo = directory.path().join("source.fifo");
    nix::unistd::mkfifo(
        &fifo,
        nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
    )
    .expect("fifo");
    let started = Instant::now();
    assert_eq!(scan(&fifo), Err(ScanError::ArchiveNotRegular));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn compression_ratio_above_one_hundred_to_one_fails_closed() {
    let body = vec![b'x'; 1_000_000];
    let fixture = write_archive(&[Fixture {
        name: "compressed.json",
        body: &body,
        compression: CompressionMethod::Deflated,
        kind: FixtureKind::File,
    }]);
    assert_eq!(
        scan(&fixture.path),
        Err(ScanError::EntryCompressionRatio { index: 0 })
    );
}

#[test]
fn archive_size_limit_rejects_sparse_over_limit_file_before_parse() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("oversized.zip");
    let file = File::create(&path).expect("create sparse file");
    file.set_len(FrozenLimits::MAX_ARCHIVE_BYTES + 1)
        .expect("extend sparse file");
    assert_eq!(scan(&path), Err(ScanError::ArchiveTooLarge));
}

#[test]
fn entry_count_limit_rejects_twenty_five_thousand_and_one() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("many.zip");
    let file = File::create(&path).expect("create archive");
    let mut writer = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for index in 0..=FrozenLimits::MAX_ENTRIES {
        writer
            .start_file(format!("entry-{index:05}.json"), options)
            .expect("start entry");
    }
    writer.finish().expect("finish archive");
    assert_eq!(scan(&path), Err(ScanError::TooManyEntries));
}

#[test]
fn symlink_and_special_file_modes_fail_closed() {
    for (mode, violation) in [
        (0o120_777, EntryKindViolation::Symlink),
        (0o010_644, EntryKindViolation::Special),
        (0o040_755, EntryKindViolation::ContradictoryDirectory),
    ] {
        let fixture = write_archive(&[Fixture {
            name: "entry",
            body: b"target",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        }]);
        patch_first_central_unix_mode(&fixture.path, mode);
        assert_eq!(
            scan(&fixture.path),
            Err(ScanError::UnsupportedEntryKind {
                index: 0,
                violation,
            })
        );
    }
}

#[test]
fn declared_entry_and_total_expansion_limits_fail_before_body_read() {
    let one = write_archive(&[Fixture {
        name: "huge.json",
        body: b"",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    patch_central_sizes(
        &one.path,
        0,
        FrozenLimits::MAX_ENTRY_EXPANDED_BYTES + 1,
        FrozenLimits::MAX_ENTRY_EXPANDED_BYTES + 1,
    );
    assert_eq!(scan(&one.path), Err(ScanError::EntryTooLarge { index: 0 }));

    let names = [
        "part-0.json",
        "part-1.json",
        "part-2.json",
        "part-3.json",
        "part-4.json",
        "part-5.json",
        "part-6.json",
        "part-7.json",
        "part-8.json",
    ];
    let entries = names.map(|name| Fixture {
        name,
        body: b"" as &[u8],
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    });
    let total = write_archive(&entries);
    for index in 0..9 {
        patch_central_sizes(
            &total.path,
            index,
            FrozenLimits::MAX_ENTRY_EXPANDED_BYTES,
            FrozenLimits::MAX_ENTRY_EXPANDED_BYTES,
        );
    }
    assert_eq!(scan(&total.path), Err(ScanError::TotalExpandedTooLarge));
}

#[test]
fn public_worker_cancellation_is_sticky_and_returns_no_catalog() {
    let fixture = write_archive(&[Fixture {
        name: "data.json",
        body: b"content",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    let worker = DeepZipSupervisor::new(env!("CARGO_BIN_EXE_openopen-deep-zip-worker"));
    let token: CancellationToken = worker.cancellation_token();
    token.cancel();
    assert_eq!(worker.scan(&fixture.path), Err(ScanError::Cancelled));
}

#[test]
fn valid_first_entry_plus_invalid_second_returns_no_partial_catalog() {
    let fixture = write_archive(&[
        Fixture {
            name: "valid.json",
            body: b"valid",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        },
        Fixture {
            name: "../invalid.json",
            body: b"invalid",
            compression: CompressionMethod::Stored,
            kind: FixtureKind::File,
        },
    ]);
    assert_eq!(
        scan(&fixture.path),
        Err(ScanError::InvalidPath {
            index: 1,
            violation: PathViolation::Traversal,
        })
    );
}

fn minimal_tar_bytes() -> Vec<u8> {
    let mut header = [0_u8; 512];
    header[..10].copy_from_slice(b"nested.txt");
    write_tar_octal(&mut header[100..108], 0o644);
    write_tar_octal(&mut header[108..116], 0);
    write_tar_octal(&mut header[116..124], 0);
    write_tar_octal(&mut header[124..136], 0);
    write_tar_octal(&mut header[136..148], 0);
    header[148..156].fill(b' ');
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let checksum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
    header[148..156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());

    let mut tar = header.to_vec();
    tar.extend_from_slice(&[0_u8; 1_024]);
    tar
}

fn write_tar_octal(field: &mut [u8], value: u64) {
    let digits_bytes = field.len() - 1;
    let digits = format!("{value:0digits_bytes$o}");
    field[..digits_bytes].copy_from_slice(digits.as_bytes());
    field[digits_bytes] = 0;
}

fn patch_first_central_unix_mode(path: &Path, mode: u32) {
    let mut bytes = fs::read(path).expect("read archive");
    let offset = find_signatures(&bytes, b"PK\x01\x02")[0];
    bytes[offset + 4] = 20;
    bytes[offset + 5] = 3;
    bytes[offset + 38..offset + 42].copy_from_slice(&(mode << 16).to_le_bytes());
    fs::write(path, bytes).expect("patch mode");
}

fn add_entry_flags(path: &Path, index: usize, flags: u16) {
    let mut bytes = fs::read(path).expect("read archive");
    let local = find_signatures(&bytes, b"PK\x03\x04")[index];
    let central = find_signatures(&bytes, b"PK\x01\x02")[index];
    let local_flags = u16::from_le_bytes([bytes[local + 6], bytes[local + 7]]) | flags;
    let central_flags = u16::from_le_bytes([bytes[central + 8], bytes[central + 9]]) | flags;
    bytes[local + 6..local + 8].copy_from_slice(&local_flags.to_le_bytes());
    bytes[central + 8..central + 10].copy_from_slice(&central_flags.to_le_bytes());
    fs::write(path, bytes).expect("patch flags");
}

fn add_single_entry_extras(path: &Path, local_extra: &[u8], central_extra: &[u8]) {
    let mut bytes = fs::read(path).expect("read archive");
    let local = find_signatures(&bytes, b"PK\x03\x04")[0];
    let original_central = find_signatures(&bytes, b"PK\x01\x02")[0];
    let original_eocd = find_signatures(&bytes, b"PK\x05\x06")[0];
    assert_eq!(
        u16::from_le_bytes([bytes[local + 28], bytes[local + 29]]),
        0
    );
    assert_eq!(
        u16::from_le_bytes([bytes[original_central + 30], bytes[original_central + 31]]),
        0
    );
    let local_name_bytes = usize::from(u16::from_le_bytes([bytes[local + 26], bytes[local + 27]]));
    let local_extra_bytes = u16::try_from(local_extra.len()).expect("local extra fits u16");
    bytes[local + 28..local + 30].copy_from_slice(&local_extra_bytes.to_le_bytes());
    let local_insert = local + 30 + local_name_bytes;
    bytes.splice(local_insert..local_insert, local_extra.iter().copied());

    let central = original_central + local_extra.len();
    let central_name_bytes = usize::from(u16::from_le_bytes([
        bytes[central + 28],
        bytes[central + 29],
    ]));
    let central_extra_bytes = u16::try_from(central_extra.len()).expect("central extra fits u16");
    bytes[central + 30..central + 32].copy_from_slice(&central_extra_bytes.to_le_bytes());
    let central_insert = central + 46 + central_name_bytes;
    bytes.splice(
        central_insert..central_insert,
        central_extra.iter().copied(),
    );

    let eocd = original_eocd + local_extra.len() + central_extra.len();
    let central_size = u32::from_le_bytes([
        bytes[eocd + 12],
        bytes[eocd + 13],
        bytes[eocd + 14],
        bytes[eocd + 15],
    ])
    .checked_add(u32::try_from(central_extra.len()).expect("central extra fits u32"))
    .expect("central size");
    let central_offset = u32::from_le_bytes([
        bytes[eocd + 16],
        bytes[eocd + 17],
        bytes[eocd + 18],
        bytes[eocd + 19],
    ])
    .checked_add(u32::try_from(local_extra.len()).expect("local extra fits u32"))
    .expect("central offset");
    bytes[eocd + 12..eocd + 16].copy_from_slice(&central_size.to_le_bytes());
    bytes[eocd + 16..eocd + 20].copy_from_slice(&central_offset.to_le_bytes());
    fs::write(path, bytes).expect("add extra fields");
}

fn add_single_entry_comment(path: &Path, comment: &[u8]) {
    let mut bytes = fs::read(path).expect("read archive");
    let central = find_signatures(&bytes, b"PK\x01\x02")[0];
    let original_eocd = find_signatures(&bytes, b"PK\x05\x06")[0];
    assert_eq!(
        u16::from_le_bytes([bytes[central + 32], bytes[central + 33]]),
        0
    );
    let name_bytes = usize::from(u16::from_le_bytes([
        bytes[central + 28],
        bytes[central + 29],
    ]));
    let extra_bytes = usize::from(u16::from_le_bytes([
        bytes[central + 30],
        bytes[central + 31],
    ]));
    bytes[central + 32..central + 34].copy_from_slice(
        &u16::try_from(comment.len())
            .expect("comment fits u16")
            .to_le_bytes(),
    );
    let comment_start = central + 46 + name_bytes + extra_bytes;
    bytes.splice(comment_start..comment_start, comment.iter().copied());
    let eocd = original_eocd + comment.len();
    let central_size = u32::from_le_bytes([
        bytes[eocd + 12],
        bytes[eocd + 13],
        bytes[eocd + 14],
        bytes[eocd + 15],
    ])
    .checked_add(u32::try_from(comment.len()).expect("comment fits u32"))
    .expect("central size");
    bytes[eocd + 12..eocd + 16].copy_from_slice(&central_size.to_le_bytes());
    fs::write(path, bytes).expect("add central comment");
}

fn forge_dual_eocd_symlink_fallback(path: &Path) -> (u64, u64) {
    let bytes = fs::read(path).expect("read archive");
    let final_central = find_signatures(&bytes, b"PK\x01\x02")[0];
    let original_eocd = *find_signatures(&bytes, b"PK\x05\x06").last().expect("EOCD");
    assert_eq!(original_eocd + 22, bytes.len());
    assert_eq!(
        usize::try_from(u32::from_le_bytes([
            bytes[original_eocd + 16],
            bytes[original_eocd + 17],
            bytes[original_eocd + 18],
            bytes[original_eocd + 19],
        ]))
        .expect("central offset"),
        final_central
    );
    let original_central = bytes[final_central..original_eocd].to_vec();
    let name_bytes = usize::from(u16::from_le_bytes([
        original_central[28],
        original_central[29],
    ]));
    assert_eq!(
        u16::from_le_bytes([original_central[30], original_central[31]]),
        0
    );
    assert_eq!(
        u16::from_le_bytes([original_central[32], original_central[33]]),
        0
    );
    assert_eq!(original_central.len(), 46 + name_bytes);

    let malformed_aes_extra = [0x01, 0x99, 0x06, 0x00, 0, 0, 0, 0, 0, 0];
    let embedded_central = final_central + 46 + name_bytes + malformed_aes_extra.len();
    let embedded_eocd = single_entry_eocd(embedded_central, original_central.len());
    let comment_bytes = original_central
        .len()
        .checked_add(embedded_eocd.len())
        .expect("comment size");

    let mut forged_central = original_central[..46 + name_bytes].to_vec();
    forged_central[4] = 20;
    forged_central[5] = 3;
    forged_central[30..32].copy_from_slice(
        &u16::try_from(malformed_aes_extra.len())
            .expect("extra size")
            .to_le_bytes(),
    );
    forged_central[32..34].copy_from_slice(
        &u16::try_from(comment_bytes)
            .expect("comment size")
            .to_le_bytes(),
    );
    forged_central[38..42].copy_from_slice(&(0o120_777_u32 << 16).to_le_bytes());
    forged_central.extend_from_slice(&malformed_aes_extra);
    forged_central.extend_from_slice(&original_central);
    forged_central.extend_from_slice(&embedded_eocd);
    let final_eocd = single_entry_eocd(final_central, forged_central.len());

    let mut forged = bytes[..final_central].to_vec();
    forged.extend_from_slice(&forged_central);
    forged.extend_from_slice(&final_eocd);
    fs::write(path, forged).expect("write dual EOCD fixture");
    (
        u64::try_from(embedded_central).expect("embedded central fits u64"),
        u64::try_from(final_central).expect("final central fits u64"),
    )
}

fn single_entry_eocd(central_start: usize, central_bytes: usize) -> [u8; 22] {
    let mut eocd = [0_u8; 22];
    eocd[..4].copy_from_slice(b"PK\x05\x06");
    eocd[8..10].copy_from_slice(&1_u16.to_le_bytes());
    eocd[10..12].copy_from_slice(&1_u16.to_le_bytes());
    eocd[12..16].copy_from_slice(
        &u32::try_from(central_bytes)
            .expect("central bytes fit u32")
            .to_le_bytes(),
    );
    eocd[16..20].copy_from_slice(
        &u32::try_from(central_start)
            .expect("central start fits u32")
            .to_le_bytes(),
    );
    eocd
}

fn patch_first_name_as_invalid_utf8(path: &Path) {
    let mut bytes = fs::read(path).expect("read archive");
    let local = find_signatures(&bytes, b"PK\x03\x04")[0];
    let central = find_signatures(&bytes, b"PK\x01\x02")[0];

    let local_flags = u16::from_le_bytes([bytes[local + 6], bytes[local + 7]]) & !(1 << 11);
    bytes[local + 6..local + 8].copy_from_slice(&local_flags.to_le_bytes());
    bytes[local + 30] = 0xff;

    let central_flags = u16::from_le_bytes([bytes[central + 8], bytes[central + 9]]) & !(1 << 11);
    bytes[central + 8..central + 10].copy_from_slice(&central_flags.to_le_bytes());
    bytes[central + 46] = 0xff;
    fs::write(path, bytes).expect("patch name");
}

fn patch_central_sizes(path: &Path, index: usize, compressed: u64, expanded: u64) {
    let compressed = u32::try_from(compressed).expect("fixture compressed size fits u32");
    let expanded = u32::try_from(expanded).expect("fixture expanded size fits u32");
    let mut bytes = fs::read(path).expect("read archive");
    let offsets = find_signatures(&bytes, b"PK\x01\x02");
    let offset = offsets[index];
    bytes[offset + 20..offset + 24].copy_from_slice(&compressed.to_le_bytes());
    bytes[offset + 24..offset + 28].copy_from_slice(&expanded.to_le_bytes());
    fs::write(path, bytes).expect("patch sizes");
}

fn patch_central_and_local_sizes(path: &Path, index: usize, compressed: u64, expanded: u64) {
    let compressed = u32::try_from(compressed).expect("fixture compressed size fits u32");
    let expanded = u32::try_from(expanded).expect("fixture expanded size fits u32");
    let mut bytes = fs::read(path).expect("read archive");
    let central = find_signatures(&bytes, b"PK\x01\x02")[index];
    let local = find_signatures(&bytes, b"PK\x03\x04")[index];
    bytes[central + 20..central + 24].copy_from_slice(&compressed.to_le_bytes());
    bytes[central + 24..central + 28].copy_from_slice(&expanded.to_le_bytes());
    bytes[local + 18..local + 22].copy_from_slice(&compressed.to_le_bytes());
    bytes[local + 22..local + 26].copy_from_slice(&expanded.to_le_bytes());
    fs::write(path, bytes).expect("patch local and central sizes");
}

fn patch_entry_name(path: &Path, index: usize, replacement: &[u8]) {
    let mut bytes = fs::read(path).expect("read archive");
    let central = find_signatures(&bytes, b"PK\x01\x02")[index];
    let local = find_signatures(&bytes, b"PK\x03\x04")[index];
    let central_name_bytes = usize::from(u16::from_le_bytes([
        bytes[central + 28],
        bytes[central + 29],
    ]));
    let local_name_bytes = usize::from(u16::from_le_bytes([bytes[local + 26], bytes[local + 27]]));
    assert_eq!(replacement.len(), central_name_bytes);
    assert_eq!(replacement.len(), local_name_bytes);
    bytes[central + 46..central + 46 + replacement.len()].copy_from_slice(replacement);
    bytes[local + 30..local + 30 + replacement.len()].copy_from_slice(replacement);
    fs::write(path, bytes).expect("patch entry name");
}

fn patch_stored_entry_body(path: &Path, index: usize, replacement: &[u8]) {
    let mut bytes = fs::read(path).expect("read archive");
    let local = find_signatures(&bytes, b"PK\x03\x04")[index];
    let name_bytes = usize::from(u16::from_le_bytes([bytes[local + 26], bytes[local + 27]]));
    let extra_bytes = usize::from(u16::from_le_bytes([bytes[local + 28], bytes[local + 29]]));
    let body_start = local + 30 + name_bytes + extra_bytes;
    let body_bytes = usize::try_from(u32::from_le_bytes([
        bytes[local + 18],
        bytes[local + 19],
        bytes[local + 20],
        bytes[local + 21],
    ]))
    .expect("stored body size fits usize");
    assert_eq!(replacement.len(), body_bytes);
    bytes[body_start..body_start + body_bytes].copy_from_slice(replacement);
    fs::write(path, bytes).expect("patch stored body");
}

fn pad_first_deflate_stream(path: &Path, padding: &[u8]) {
    let mut bytes = fs::read(path).expect("read archive");
    let local = find_signatures(&bytes, b"PK\x03\x04")[0];
    let central = find_signatures(&bytes, b"PK\x01\x02")[0];
    let eocd = find_signatures(&bytes, b"PK\x05\x06")[0];
    let compressed = u32::from_le_bytes([
        bytes[central + 20],
        bytes[central + 21],
        bytes[central + 22],
        bytes[central + 23],
    ]);
    let padding_bytes = u32::try_from(padding.len()).expect("padding fits u32");
    let padded_compressed = compressed.checked_add(padding_bytes).expect("padded size");
    bytes.splice(central..central, padding.iter().copied());
    let shifted_central = central + padding.len();
    let shifted_eocd = eocd + padding.len();
    bytes[local + 18..local + 22].copy_from_slice(&padded_compressed.to_le_bytes());
    bytes[shifted_central + 20..shifted_central + 24]
        .copy_from_slice(&padded_compressed.to_le_bytes());
    let central_offset = u32::try_from(shifted_central).expect("central offset fits u32");
    bytes[shifted_eocd + 16..shifted_eocd + 20].copy_from_slice(&central_offset.to_le_bytes());
    fs::write(path, bytes).expect("pad deflate stream");
}

fn reuse_local_record(path: &Path, source: usize, target: usize) {
    let mut bytes = fs::read(path).expect("read archive");
    let central = find_signatures(&bytes, b"PK\x01\x02");
    let source_offset = bytes[central[source] + 42..central[source] + 46].to_vec();
    bytes[central[target] + 42..central[target] + 46].copy_from_slice(&source_offset);
    fs::write(path, bytes).expect("reuse local record");
}

fn find_signatures(bytes: &[u8], signature: &[u8]) -> Vec<usize> {
    bytes
        .windows(signature.len())
        .enumerate()
        .filter_map(|(index, window)| (window == signature).then_some(index))
        .collect()
}

#[test]
fn fixture_patch_helpers_do_not_modify_local_entry_bodies() {
    let fixture = write_archive(&[Fixture {
        name: "entry",
        body: b"body",
        compression: CompressionMethod::Stored,
        kind: FixtureKind::File,
    }]);
    patch_first_central_unix_mode(&fixture.path, 0o100_644);
    let mut archive = zip::ZipArchive::new(File::open(&fixture.path).expect("open")).expect("zip");
    let mut body = String::new();
    archive
        .by_index(0)
        .expect("entry")
        .read_to_string(&mut body)
        .expect("read");
    assert_eq!(body, "body");
}
