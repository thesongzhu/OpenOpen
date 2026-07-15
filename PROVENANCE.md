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
| `crates/openopen-core/src/mission.rs` | `rust-core/crates/friday-core/src/mission.rs` | Independent OpenOpen lifecycle adapted from Friday's legal-transition and proof-before-completion invariants | bootstrap `19ecdd9…`; two isolated foundation reviewers PASS; local 95-test verification; follow-up PR run `29370433505` PASS on synthesized merge `d502b3d…`, whose tree equals head `923c88a…`; exact-head and release proof pending |
| `crates/openopen-core/src/store.rs` | `rust-core/crates/friday-storage/src/mission.rs`, `rust-core/crates/friday-storage/src/audit.rs` | Independent encrypted lifecycle persistence and signed/tail-anchored audit design | bootstrap `19ecdd9…`; two isolated foundation reviewers PASS; local 95-test verification; follow-up PR run `29370433505` PASS on synthesized merge `d502b3d…`, whose tree equals head `923c88a…`; product-shell `e2313fe…` passes local 134-Rust/67-Swift verification, two fresh Repair20 reviewers PASS fingerprint `29a00413…`, and PR #2 run `29386477267` PASS on synthesized merge `487dae1…`, whose tree equals the head tree; the first Hero A candidate passed 139-Rust/70-Swift locally but failed both closure reviews; Repair1 passed governance but failed functional review on physical EventKit target drift; both Repair2 reviewers failed its first-write/cancellation/all-markers-missing recovery boundary; both Repair3 reviewers then failed frozen `fa9d905e…` because volatile `createOnce` authority survived possible EventKit commit; Repair4 added durable signed dispatch Evidence but governance rejected fingerprint `4cabaeb4…` because its lower-level writer still accepted reusable Mission copies; Repair5 consumes an internal one-shot dispatch start before any external boundary, passes 146 ordinary Rust plus 83 Swift tests and ad-hoc staging locally, and receives two fresh reviewer PASS reports on frozen fingerprint `4b41a04f…`; current CI, exact-head, signed/admin, cross-UID, provider, and release proof remain pending |

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

Codex app-server schemas are generated from pinned runtime `0.144.0`; the
tracked manifest binds the runtime package components and all 267 schemas. The
runtime binary itself is not stored in Git. Local staging accepts only an
explicit package root whose four component hashes match the manifest. This is
implementation provenance, not redistribution, signed-package, or provider
proof.

`imsg`, serenity, rust_xlsxwriter, SQLite, Codex, and all transitive
dependencies must have complete locked notices before the signed distribution
gate passes.
