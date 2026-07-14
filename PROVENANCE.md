# Provenance Map

## Friday

- Upstream: `https://github.com/thesongzhu/Friday`
- Source commit: `4870f31fa088bef7eb9f4f256ec62993b02eda80`
- License: MIT, copyright Friday contributors

Only the smallest tested concepts or implementation portions needed by
OpenOpen may be adapted. Every adapted file must be added to this table before
staging.

| OpenOpen module | Friday source | Adaptation | Verification |
| --- | --- | --- | --- |
| `crates/openopen-core/src/mission.rs` | `rust-core/crates/friday-core/src/mission.rs` | Independent OpenOpen lifecycle adapted from Friday's legal-transition and proof-before-completion invariants | local Rust tests; reviewer rerun pending |
| `crates/openopen-core/src/store.rs` | `rust-core/crates/friday-storage/src/mission.rs`, `rust-core/crates/friday-storage/src/audit.rs` | Independent encrypted lifecycle persistence and signed/tail-anchored audit design | local Rust tests; reviewer rerun pending |

Workflow and Skill adaptations remain planned and are not yet present or
claimed.

No Friday Hub, provider, TypeScript execution route, mobile application, or UI
is imported.

The local Friday working tree was observed detached at `25329515…` during this
phase. Its `origin/main` later advanced to
`0871c37a5f88f30371ad86fa9583fe24c09ef253`; the required pin remains an
ancestor. All provenance inspection and adaptation uses the immutable form
`git show 4870f31fa088bef7eb9f4f256ec62993b02eda80:<path>` and does not import
later upstream changes.

## Other sources

`imsg`, Codex, serenity, rust_xlsxwriter, SQLite, and all transitive
dependencies must be locked and represented in `THIRD_PARTY_NOTICES.md` before
the signed distribution gate passes.
