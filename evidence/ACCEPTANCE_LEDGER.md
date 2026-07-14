# Acceptance Ledger

Product state: `IMPLEMENTATION_IN_PROGRESS`

No row may be marked PASS from a mock, fixture, screenshot, CI status, or a
different commit/build.

| Gate | Evidence required | Current result |
| --- | --- | --- |
| Rust domain and security tests | Current-SHA test log | 95 local uncommitted tests pass across Core, protocol, host, broker engine, persistent worker, concurrent Store→broker ordering, reconciliation, lost-response/session-rotation recovery, crash recovery, and outcome rollback/tamper coverage; release build, strict workspace Clippy, and fmt pass; fresh reviewers and commit-SHA proof remain pending |
| SwiftUI build and tests | Current-SHA build/test log | EffectBrokerBridge and real daemon executable pass 25 local tests including typed reconciliation routing; warnings-as-errors test/release build, strict format, and plist lint pass locally; commit-SHA proof and the product SwiftUI app remain pending |
| Codex stable protocol contract | Generated schema and live handshake | pending |
| Voice → Reminders → Receipt | Signed-build real runtime record | pending |
| iMessage bidirectional route | Signed-build real message IDs | pending |
| Discord bidirectional route | Signed-build bot/channel message IDs | pending |
| Receipt image → XLSX | Input/output hashes and workbook verification | pending |
| Restart recovery and dedupe | Same-SHA runtime record | pending |
| Skill lifecycle and containment | Security test report | pending |
| Stress suite | Nonzero all-pass scenario artifact | pending |
| Signed/notarized clean install | codesign/notary/staple/Gatekeeper evidence | pending |
| Three external users | Consent-safe aggregate and 48-hour reuse | pending |
| Two isolated reviewers | PASS reports for release SHA | pending |

## Review history

- 2026-07-14 foundation pass 1: Reviewer A FAIL; Reviewer B FAIL. Shared
  blockers were `NeedsMe` bypass, non-specific action approval, forgeable
  Evidence, unanchored audit tail, incomplete recovery persistence, lint
  failure, and disclosure/provenance overclaim. Repair and fresh verification
  are in progress; this is not a PASS artifact.
- 2026-07-14 lint fix-loop supervisor: `NOT_STUCK: different_cause_per_attempt`
  after 11 → 6 → 2 distinct finite Clippy findings; authorized a narrow fourth
  correction and reset the same-root counter. This is process evidence, not a
  product PASS.
- 2026-07-14 foundation reviewer rerun: Reviewer A FAIL; Reviewer B FAIL.
  Blockers were empty-Mission completion, Evidence scope replay, duplicate
  approval IDs, untrusted workspace roots/symlink escape, non-atomic state and
  audit writes, audit re-anchoring, missing WorkItem approval/recovery coverage,
  JSON-RPC parse error identity, and documentation overclaim. The repair now
  passes local verification; fresh reviewer reports remain pending.
- 2026-07-14 second verification supervisor:
  `NOT_STUCK: different_cause_per_attempt` for rustfmt, RPC type propagation,
  and a Clippy API-shape warning. The final typed Evidence input correction
  retains Mission/WorkItem claims in the signed payload. This is process
  evidence, not release proof.
- 2026-07-14 next foundation reviewer rerun: Reviewer A FAIL; Reviewer B FAIL.
  Reproduced persistence bypass, WorkItem approval replay, dangling symlink,
  unbound state/audit rows, middle-row deletion followed by a successful write,
  optional disclosure payloads, parent Mission state drift, and JSON-RPC
  invalid-request misclassification. That repair was superseded by another
  fresh reviewer cycle; no prior local green result is treated as closure.
- 2026-07-14 third verification supervisor:
  `NOT_STUCK: different_cause_per_attempt` for formatting, production-code
  lint, and a mechanically distinct test lint. The narrow correction retained
  the same assertions and reset the same-root counter. This is process
  evidence, not product proof.
- 2026-07-14 latest foundation reviewer rerun: Reviewer A FAIL; Reviewer B
  FAIL. Both reproduced missing bidirectional ledger/state reconciliation,
  mutable Receipt IDs, and pre-approved Approval injection. Reviewer B also
  reproduced post-confirmation WorkItem injection and primitive JSON-RPC
  params. The repaired tree now passes 36 local tests, workspace check/build,
  strict Clippy, and runtime stdio probes; fresh reviewer reports remain
  pending, and there is still no commit-SHA or release proof.
- 2026-07-14 follow-up foundation reviewer rerun: Reviewer A FAIL; Reviewer B
  FAIL. Both reproduced direct persisted Approval decisions outside the exact
  confirmation boundary. Reviewer B also reproduced Pending Mission and
  WorkItem resume plus a Mission-declared `/` workspace trust root; Reviewer A
  identified free-standing genesis approvals and untracked-file whitespace
  that ordinary `git diff --check` did not inspect. The repaired tree now
  passes 39 local tests and the full local verification set. A third fresh
  reviewer cycle remains required; this is still not commit-SHA or release
  proof.
- 2026-07-14 first command-owned persistence/workspace reviewer cycle:
  Reviewer A FAIL; Reviewer B FAIL. Reproduced invalid Mission path IDs,
  ordinary workspace replacement, and an unsigned command-result hash that
  could turn a conflict into a retry. The repair bound `command_hash` into the
  signed audit payload, rejected path-like IDs, and pinned root/workspace file
  identities; 43 local tests and the Rust/stdio verification set passed before
  the required fresh rerun. This was not a foundation PASS.
- 2026-07-14 second command-owned persistence/workspace reviewer cycle:
  Reviewer A FAIL; Reviewer B FAIL. Reproduced NUL Mission ID persistence,
  case-insensitive Mission directory aliasing, and an unbound redundant
  `mission_command_result.mission_id`. The narrow repair now rejects
  non-POSIX Mission IDs, enforces one Mission ID per workspace file identity,
  and reconciles command-result Mission ownership against the signed Mission
  audit row. All 45 local tests and the Rust/stdio verification set pass;
  another two fresh reviewer PASS reports are still required. This is not
  commit-SHA or release proof.
- 2026-07-14 third command-owned persistence/workspace reviewer cycle:
  Reviewer A FAIL; Reviewer B FAIL. Reproduced jointly mutable encrypted
  command results and hashes, normalized path-like Mission IDs, fresh-Gate
  case aliases, hard-link truncation outside the workspace, and future
  Evidence accepted before its observation time. The three-cycle supervisor
  returned `STUCK: same_root_cause`; the owner explicitly approved “方案
  1：完整不变量重构（推荐）”. This approval did not weaken any acceptance gate.
- 2026-07-14 full invariant repair: one canonical lowercase ASCII Mission ID
  parser is shared by domain, Store, and Gate; descriptor-derived exact names
  and atomic temporary-file replacement close case aliases and hard-link
  truncation; a detached signature binds the entire command-result record and
  global reconciliation decrypts every result; Evidence observation,
  attachment, completion, and Receipt times are causal and monotonic. The tree
  passes 49 local tests plus fmt/check/build/strict Clippy, host stdio,
  forbidden-route, credential-pattern, cleanup, and touched-file whitespace
  checks. Two fresh isolated reviewer PASS reports remain required; there is
  no commit-SHA or release proof.
- 2026-07-14 full invariant reviewer cycle: Reviewer A FAIL; Reviewer B FAIL.
  Both reproduced a Mission workspace moved outside its trusted root during a
  long write and a visible streaming temporary inode that could be hard-linked
  outside the workspace. The repair streams into a non-enumerable private
  staging directory, re-opens and revalidates the exact destination at the
  effect boundary, and removes staged or relocated output on failure. All 51
  local tests plus fmt/check/build/strict Clippy, host stdio, forbidden-route,
  credential-pattern, cleanup, and all-file whitespace checks pass. Two new
  isolated reviewer PASS reports remain required; this is not foundation PASS,
  commit-SHA proof, or release proof.
- 2026-07-14 private-staging reviewer cycle: Reviewer A FAIL; Reviewer B FAIL.
  Both reproduced the fact that same-UID code could regain read permission on
  the staging directory; Reviewer A also held a directory FD during the former
  `0700` creation window and reproduced a failed unchanged command-envelope
  retry. Staging now starts search-only through macOS `O_SEARCH`, payload
  inodes remain mode `0000` during streaming, permission/link-count tampering
  fails closed and scrubs the inode, and exact duplicates are fully verified
  before applying a new-command anchor check. All 52 local tests plus the full
  Rust/stdio/security scan set pass. Two fresh isolated reviewer PASS reports
  remain required; this is not foundation PASS, commit-SHA proof, or release
  proof.
- 2026-07-14 search-only staging reviewer cycle: Reviewer A FAIL; Reviewer B
  FAIL. One moved the renamed final file outside and one moved the entire
  Mission workspace after the last destination check; both adversarial long
  writes returned success while the full output survived outside the trusted
  root. The supervisor classified this as the third consecutive manifestation
  of the same missing linearizable effect-boundary invariant and returned
  `STUCK: same_root_cause`. The owner approved “新的方案 1：独立 effect broker
  安全边界” and all recommended implementation details that preserve a
  distinct security principal, broker-exclusive root, typed Core commands,
  and fail-closed behavior. No test or proof gate was weakened.
- 2026-07-14 protected effect-broker implementation: Core now refuses missing
  or caller-selected broker trust, signs only Store-derived typed permits, and
  persists broker-signed Receipts with a bound audit row. The root worker
  derives the sole Mission root from authenticated EUID, persists root-only
  Core enrollment/broker seed/session/journal state, and supports fresh
  cross-session attestations over immutable committed output. The Swift
  LaunchDaemon executable enforces mutual code requirements, exact canonical
  DTOs, and a signed worker copied into root-only storage. 74 Rust and 21 Swift
  local tests plus strict builds pass. Two fresh combined reviewers remain
  required, and no unsigned local result is cross-UID, admin-install, signing,
  notarization, current-SHA, or release proof.
- 2026-07-14 first combined protected-broker reviewer cycle: Reviewer A FAIL;
  Reviewer B FAIL. Findings were an unwired/caller-selectable trust enrollment,
  an unbounded payload/worker lock, a deletable effect authorization, mutable
  recovery commit time, non-durable cross-session attestation, and divergent
  Rust/Swift permit bounds. The repair now persists a Keychain trust anchor
  only through an admin-enabled exact-code-requirement XPC flow, signs the
  install record with Core's independently derived Keychain authority, audits
  every authorization, bounds and reaps worker I/O, preserves pre-rename
  commit intent, durably replaces session attestations, accepts only verified
  immutable reattestations, and aligns all permit bounds. 77 Rust and 23 Swift
  local tests plus strict builds pass. Two new isolated reviewer PASS reports
  remain required; signed/admin/cross-UID evidence is still absent.
- 2026-07-14 second combined protected-broker reviewer cycle: Reviewer A FAIL;
  Reviewer B FAIL. They reproduced a write reaching rename after permit/session
  expiry, an already-committed broker result becoming unrecordable after the
  Store audit advanced, Receipt reuse across distinct exact permits, committed
  retries returning without consuming or validating payload, and duplicate
  approval IDs accepted by Swift but rejected by Rust. This supersedes the
  prior 77-test local green result; it was not a foundation PASS.
- 2026-07-14 exact-permit/recovery repair: the pre-rename callback now rejects
  an expired permit or daemon session, every Receipt signs the SHA-256 binding
  of the complete signed permit, every committed/recovery retry drains and
  hashes its payload, and Core can append a verified immutable result from the
  latest audit tail without reissuing a stale write authorization. Swift and
  Rust both reject duplicate approval IDs. All 82 Rust and 23 Swift local
  tests, Rust fmt/workspace tests/release/strict Clippy, Swift
  warnings-as-errors tests/debug/release, strict format, plist lint, host stdio
  probes, credential scan, and all-file whitespace scan pass. Two new isolated
  reviewer PASS reports remain required. There is no remote, commit-SHA,
  signed/admin, cross-UID, GitHub CI, or release proof.
- 2026-07-14 third combined protected-broker reviewer cycle: Reviewer A FAIL;
  Reviewer B FAIL. Reviewer A showed that a still-live execute permit could
  commit after pause/cancel and the former recovery route would accept it.
  Reviewer B showed that rename-before-response-loss became unrecoverable once
  the old permit/session expired, and that committed retries used the entry
  time even when payload validation completed after expiry. This supersedes
  the prior 82-test local green result; it was not a foundation PASS.
- 2026-07-14 recovery-authority repair: permits now bind an `execute` or
  `reattestOnly` purpose. After any audit advancement Core can issue only
  recovery authority, and the broker requires a matching existing journal,
  pinned workspace, and committed output without creating a workspace, stage,
  or file. Every signed audit row includes the Store's observed wall-clock
  time; a recovered commit must be strictly earlier than the first intervening
  observation, and a nonempty legacy ledger without that proof fails closed.
  The broker rereads and validates completion time only after payload and
  output hashing. All 87 Rust and 23 Swift tests plus strict Rust/Swift builds,
  format, Clippy, and plist lint pass locally. Two new isolated reviewer PASS
  reports remain required; there is still no remote, commit-SHA, GitHub CI,
  signed/admin, cross-UID, notarization, or release proof.
- 2026-07-14 fourth combined protected-broker reviewer cycle: Reviewer A FAIL;
  Reviewer B FAIL. They reproduced that the journal's former commit timestamp
  still preceded rename, `reattestOnly` could mutate stage/journal state, and
  an old live Execute permit could write after pause/cancel before Core rejected
  the Receipt. This was the third consecutive manifestation of the same
  missing effect/audit linearization invariant. The isolated supervisor
  returned `STUCK: same_root_cause`; the prior 87 Rust / 23 Swift green run is
  superseded and was not foundation closure.
- 2026-07-14 linearizable effect fence/reconciliation repair: effect
  authorization, a single global unresolved fence, and the authorization audit
  row share one Store transaction. Every later Mission audit is blocked until
  a verified Receipt or signed definitive noncommit writes its own audit row
  and clears the fence in one transaction. The protected broker now serializes
  independent workers with a root-owned file lock, persists the staged inode
  before rename, distinguishes intent from post-fsync completion, recovers only
  that inode, persists a permanent noncommit tombstone, rejects old Execute
  permits after noncommit, and keeps `reattestOnly` read-only. A separate typed
  `reconcile` route is wired through the Rust worker and Swift XPC DTO/backend.
  All 94 Rust and 25 Swift local tests, Rust release/fmt/strict Clippy and Swift
  warnings-as-errors test/release/strict-format/plist checks pass;
  the focused suite includes streaming effect versus pause ordering, two-worker
  exclusion, post-rename recovery, wrong-inode same-hash rejection, reattest
  zero-mutation, cleanup-before-tombstone crash recovery, and atomic outcome
  rollback/tamper. Two fresh isolated reviewer PASS reports remain required.
  There is still no remote, commit SHA, GitHub CI, signed/admin, cross-UID,
  notarization, or release proof.
- 2026-07-14 first linearizable-repair reviewer cycle: Reviewer B FAIL; Reviewer
  A was interrupted once the reviewed tree became obsolete. Reviewer B
  reproduced permanent global-fence deadlock after the broker persisted a
  terminal noncommit but its first permit-bound response was lost; a later
  reconciliation permit/session could not obtain a matching attestation. The
  prior 94-test local green run is superseded and was not foundation closure.
- 2026-07-14 lost-noncommit-response repair: `NotCommitted` remains a permanent
  terminal classification, while a new valid matching Reconcile permit can
  atomically replace only the cached signed attestation for its exact permit
  and session. A persistent end-to-end test loses the first response, restarts
  Store and broker, rotates the session, records the fresh attestation, clears
  the fence, advances the Mission, and proves the old Execute stays rejected.
  All 95 Rust tests plus release/fmt/strict Clippy pass locally; 25 Swift tests
  and their prior warnings-as-errors/strict-format/release checks remain green
  because the Swift protocol surface did not change. Two new isolated reviewer
  PASS reports remain required; there is still no remote, commit SHA, GitHub
  CI, signed/admin, cross-UID, notarization, or release proof.
- 2026-07-14 second linearizable-repair reviewer cycle: Reviewer C PASS;
  Reviewer D PASS. Reviewer C independently traced Store fence/outcome
  atomicity, cross-process serialization, post-fsync completion, staged-inode
  recovery, ReattestOnly zero-mutation, cleanup crash recovery, and the full
  lost-noncommit-response/session-rotation sequence. Reviewer D independently
  traced terminal classification versus refreshable attestation, exact retry,
  tamper/session/permit conflict, persistence shape, every Store fence route,
  Rust↔Swift purpose parity, no raw Mission snapshot replacement, and evidence
  claims. Disposable reruns passed 95 Rust and 25 Swift tests and left no
  artifacts. This closes the foundation Stage 5 reviewer gate only; commit
  SHA, remote/GitHub CI, signed/admin install, cross-UID/XPC adversarial proof,
  notarization, real product E2E, and release proof remain absent.
