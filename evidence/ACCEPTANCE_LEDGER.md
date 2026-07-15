# Acceptance Ledger

Product state: `IMPLEMENTATION_IN_PROGRESS`

No row may be marked PASS from a mock, fixture, screenshot, CI status, or a
different commit/build.

| Gate | Evidence required | Current result |
| --- | --- | --- |
| Rust domain and security tests | Current-SHA test log | Local Friday-alpha Repair3 tree based on pushed Hero A commit `774789c…` passes 190 ordinary Rust tests with one explicit environment-gated Codex test, release build, workspace strict Clippy, fmt, focused Host/imsg regressions, five pinned upstream imsg tests, and two explicit pinned-runtime sandbox diagnostics. Two fresh Repair3 reviewers and current pushed-SHA CI remain pending; this is not provider or release proof |
| SwiftUI build and tests | Current-SHA build/test log | Local Friday-alpha Repair3 tree passes 40 broker/signing tests plus 53 App tests, warnings-as-errors debug/release build, strict format, both plists, script syntax, and diff checks. New tests cover product chat selection plus activation and prepare-response-loss cleanup/retry. Corrected-provenance `/private/tmp/OpenOpen-FridayAlpha-Repair3-Final2.app` and DMG pass deep ad-hoc and read-only mount/copy/signature install verification at DMG SHA `bff4d18b…`; Team is `not set`, so signed/admin and cross-UID release proof remain pending |
| Codex stable protocol contract | Generated schema and live handshake | Pinned `0.144.0` manifest binds four runtime components and 267 generated schemas; the exact pinned outer-sandbox initialize diagnostic passes twice on the Repair3 tree. Hero A PR #2 CI remains historical integration-tree plumbing. Real managed login, required-model catalog, structured turn, signed-build proof, and release proof remain pending |
| Voice → Reminders → Receipt | Signed-build real runtime record | pending |
| iMessage bidirectional route | Signed-build real message IDs | pending |
| Discord bidirectional route | Signed-build bot/channel message IDs | pending |
| Receipt image → XLSX | Input/output hashes and workbook verification | pending |
| Restart recovery and dedupe | Same-SHA runtime record | pending |
| Skill lifecycle and containment | Security test report | pending |
| Stress suite | Nonzero all-pass scenario artifact | pending |
| Signed/notarized clean install | codesign/notary/staple/Gatekeeper evidence | pending |
| Three external users | Consent-safe aggregate and 48-hour reuse | pending |
| Two isolated reviewers | PASS reports for release SHA | Two entirely fresh Repair3 replacement reviewers PASS local fingerprint `3e201547…` with zero P0/P1/P2. Exact commit SHA, current CI, provider proof, and final release-SHA review remain pending |

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
- 2026-07-14 repository bootstrap: public `thesongzhu/OpenOpen` was created and
  reviewed commit `19ecdd9c290dd685f1e79ff525c71b8d38504db8` was pushed as the
  initial `main`; `git ls-remote` confirms parity. The exact SHA then passed 95
  Rust and 25 Swift tests plus strict local format/lint/release checks. No
  Actions workflow existed on that SHA, so no remote test result is claimed.
- 2026-07-14 CI slice: a minimal read-only `macos-26` workflow pins the official
  checkout action to a full commit and Rust to 1.96.0, records runner/Xcode/
  Swift versions, runs the complete strict Rust and Swift suites, checks the
  LaunchDaemon plist, and rejects tracked-file rewrites. Local verification,
  two isolated reviewers, push, draft PR, and the first real Actions run are
  pending.
- 2026-07-14 first CI reviewer cycle: technical Reviewer A PASS; governance
  Reviewer B FAIL. The workflow, action pin, runner/tool availability,
  permission floor, event semantics, and local command suite passed review.
  Reviewer B found only contradictory evidence state: the plan still called
  commit SHA absent after `19ecdd9…` existed, and README used present tense
  before any remote run. Those statements are corrected without changing the
  workflow or proof floor. Two fresh isolated reviewer PASS reports remain
  required before push.
- 2026-07-14 second CI reviewer cycle: Reviewer C PASS; Reviewer D FAIL. The
  workflow and corrected master-plan/README evidence state passed, but
  `BUILD_WEEK.md` and `PROVENANCE.md` still called the foundation pre-commit or
  reviewer-pending. A full Markdown current-state scan found no other live
  occurrence outside chronological history. Both current provenance surfaces
  now record bootstrap `19ecdd9…`, local 95/25 tests, foundation reviewer PASS,
  and the still-missing CI/signed/admin/cross-UID/product/release proof. A third
  fresh reviewer cycle is required before push.
- 2026-07-14 third CI reviewer cycle: Reviewer E PASS; Reviewer F PASS. Both
  independently accepted the least-privilege workflow, pinned action/toolchain,
  runner/tool availability, local 95/25 disposable reruns, live `19ecdd9…`
  remote facts, and repository-wide current evidence wording. This closes the
  CI slice Stage 5 reviewer gate only; the branch, draft PR, and first inspected
  GitHub Actions run remain pending, and no signed/admin/cross-UID/product or
  release proof is implied.
- 2026-07-14 first GitHub Actions proof: commit
  `b61766b5f6cb5f208583633cc0d8244b8cfd2ea8` was pushed to
  `agent/foundation-ci` and draft PR #1 triggered run `29369643001`. GitHub
  associated the run with head `b61766b…`, while checkout used synthesized
  merge `e0fed49af5ff7f65f579f6f94f509d1f7e253ff8`; their tree SHA is the same.
  The `Rust and Swift verification` job passed all steps in 1m47s: 95 Rust
  tests, Rust release build/fmt/strict Clippy, 25 Swift tests,
  warnings-as-errors release build, strict format, LaunchDaemon plist lint,
  and clean tracked diff. The runner reported macOS 26.4, Xcode 26.5, Swift
  6.3.2, and Rust 1.96.0. This is inspected PR integration-tree CI plumbing,
  not exact-head/current-SHA release proof. The evidence-only follow-up tree
  must pass its own PR check; signed/admin installation, cross-UID/XPC
  adversarial proof, notarization, real product E2E, external-user validation,
  and release proof remain absent.
- 2026-07-14 first post-run evidence reviewer cycle: factual Reviewer A PASS;
  governance Reviewer B FAIL. Reviewer B confirmed that GitHub's run API
  associates `29369643001` with head `b61766b…`, but checkout logs prove the
  workflow actually executed synthesized merge `e0fed49…`. Direct Git object
  inspection proves both commits have tree
  `56083d29024a5f389beeeaae7c5b925ee7d531d0`. Current evidence wording now
  distinguishes PR integration-tree CI from exact-head/current-SHA release
  proof. Two fresh isolated reviewer PASS reports remain required before the
  evidence-only follow-up commit.
- 2026-07-14 second post-run evidence reviewer cycle: Reviewer C PASS;
  Reviewer D PASS. Both independently verified live PR/base/head state,
  GitHub's merge-ref checkout, exact head/merge tree parity, the 95 Rust and 25
  Swift log totals, runner versions, the prior FAIL history, and the explicit
  distinction between PR integration-tree plumbing and exact-current-SHA or
  release proof. This closes the evidence-update Stage 5 gate only; its exact
  commit, push, and follow-up PR check remain pending.
- 2026-07-14 CI evidence follow-up: exact commit `923c88a…` is the remote head
  of draft PR #1. Actions run `29370433505` passed on synthesized merge
  `d502b3d…`; head and merge share tree `ecc50fa…`. The PR remains draft and
  unmerged. This closes only that evidence-follow-up integration-tree check.
- 2026-07-14 product-shell local implementation: Store runtime control is
  signed, persistent, and default-Off; Off blocks new/unresolved effects,
  permits read-only reattestation, and cancels an active Codex operation. The
  host uses a binary private Keychain bootstrap, production bundle-derived
  paths, managed ChatGPT-only stable calls, one active cancellable operation,
  strict structured outcomes, and short-lived model workspaces under an outer
  sandbox. SwiftUI provides one window, menu bar, same-window Account/Models/
  Connections/Skills/Privacy Settings, one suggestion slot, at most three
  active cards, an honestly disabled microphone, and `SMAppService.mainApp`
  Login Item registration after first successful enable. Local checks pass 112
  Rust and 28 Swift tests plus strict build/lint/format; the exact pinned
  runtime passes sandboxed initialize/account-read, and one hash-verified app
  stages with explicit `STAGED_AD_HOC_NOT_RELEASE_PROOF`. Two fresh reviewers,
  commit/push/Actions, real ChatGPT login/model output, signed/notarized clean
  install, product E2E, and external users remain pending.
- 2026-07-14 first product-shell reviewer cycle: both isolated reviewers FAIL.
  They reproduced stale Execute authority after Off, signed runtime rollback,
  local RPC memory/deadline gaps, old-process callback interference,
  out-of-order switch writes, login-item/UI divergence, missing broker bundle
  artifacts, floating IDs, closed-window restoration failure, and staging
  TOCTOU. The prior 112/28 local green result is superseded and was never
  pushed.
- 2026-07-14 product-shell repair: Core signs the next runtime revision, the
  protected broker durably applies it and returns a broker-signed Receipt, and
  only then may Core commit. Broker/effect serialization plus exact revision
  matching revokes old Execute permits before Off returns; append-only signed
  runtime history rejects valid-row rollback. Core frames are bounded in both
  directions, requests have cancellation/deadline cleanup, process generations
  are isolated, and the Codex version probe is forcibly bounded. Swift switch
  writes are serialized to last intent, login-item failure cannot misreport
  Core state, response IDs reject floating representations, and menu actions
  reopen the window. Staging now includes the daemon, worker, and LaunchDaemon
  plist, verifies embedded pins after copy, and atomically claims a new output
  directory before population. Local verification passes 116 Rust and 33 Swift
  tests, strict release/lint/format checks, the real pinned-runtime sandbox
  diagnostic, and deep ad-hoc verification of
  `/private/tmp/OpenOpen-Stage-Repaired.app`. This is not signed/admin,
  cross-UID, GitHub CI, real-provider, notarization, product-E2E, or release
  proof; two fresh reviewers remain required.
- 2026-07-14 second product-shell reviewer cycle: Reviewer C FAIL; Reviewer D
  FAIL. They found that the Swift daemon's in-process lock did not cover a
  legacy worker after daemon restart, whole-database/prefix rollback could
  restore an old signed On state and prevent Core from catching up to the
  broker high-water mark, model calls lacked live broker freshness, accepted
  Off could revert the UI to On on Core failure, refresh could overwrite a
  newer toggle, model catalog/Host output remained incompletely bounded, and
  Keychain-derived buffers lacked guaranteed zeroization. Their isolated
  116/33 reruns passed but did not close these findings; the branch stayed
  unpushed.
- 2026-07-14 second product-shell repair: runtime transitions and effect
  commits now share one root-owned cross-process lock, and the exact
  pre-rename callback rechecks the protected revision. The broker persists a
  nonce-bound checkpoint; Core records a verified recovery jump after a full
  Store rollback and can continue at the next revision. A one-time Core
  challenge is included in the broker's current-state Receipt and consumed by
  every account/model/outcome route, so a replayed old On proof cannot start a
  model process. Swift keeps the UI/model routes Off until broker and Core
  converge, retries recovery after broker acceptance, and generation-guards
  refresh. Explicit field/catalog/frame limits, a bounded Host response queue,
  and `zeroize` cover the remaining resource/secret findings. All 121 Rust and
  35 Swift tests, release builds, strict Clippy/format/plist/diff checks, the
  real pinned-runtime diagnostic, and deep ad-hoc verification of
  `/private/tmp/OpenOpen-Stage-Repair2.app` pass locally. Two entirely fresh
  isolated reviewer PASS reports remain required before commit/push; no
  signed/admin, cross-UID, real-provider, notarization, product-E2E, or release
  proof is claimed.
- 2026-07-14 third product-shell reviewer cycle: Reviewer E FAIL; Reviewer F
  FAIL. They reproduced a delayed old On proof crossing a newer Off generation,
  account and model reads incorrectly sharing one consumed challenge, a failed
  accepted-Off transition that could prevent a later On convergence, stale
  refresh failure overwrite, Swift enrollment deriving/copying the effect
  private key, an unbounded Codex stdout queue and turn accumulation, and a
  legacy-worker test that proved lock waiting without exercising the protected
  runtime/fence state. The prior 121/35 green result is superseded; the branch
  remained local and unpushed.
- 2026-07-14 third product-shell repair: Rust Core is now the only effect-key
  derivation and enrollment-signing authority; Swift supplies only a pinned
  public broker trust anchor and continues to wipe the one bootstrap master
  buffer after transfer. Preparing Off clears the outstanding challenge;
  every model entry is generation-bound, account and model reads obtain
  distinct proofs, convergence includes desired/UI/Core/protected state, and
  stale refresh success or failure cannot overwrite a newer toggle. Codex uses
  a termination-safe bounded stdout queue and explicit per-turn item/text
  ceilings. The legacy-worker test now updates a persistent SQLite protected
  runtime from On revision 1 to Off revision 2 under the cross-process guard
  and proves a later old-revision write reaches the exact commit fence and is
  rejected before rename. All 125 Rust and 38 Swift tests, release builds,
  warnings-as-errors, strict Clippy/format/plist/script/diff checks, the exact
  pinned-runtime diagnostic, and deep ad-hoc verification of
  `/private/tmp/OpenOpen-Stage-Repair5.app` pass locally. One cold first run
  exposed that the former two-second version-probe bound was too short; the
  probe remains force-bounded at five seconds and the final staged build passed
  the complete exact diagnostic twice consecutively. Two entirely fresh
  isolated reviewer PASS reports remain required before commit/push; no
  product-shell GitHub CI, signed/admin, cross-UID, real-provider,
  notarization, product-E2E, or release proof is claimed.
- 2026-07-14 fourth product-shell reviewer cycle: security Reviewer G FAIL;
  governance Reviewer H FAIL. Reviewer G reproduced that two independently
  launched App/Core processes could hold separate challenges and cancellation
  tokens against one Store, allowing a model call to cross another process's
  broker-accepted Off-to-Core-commit window. Reviewer H found the live
  `BUILD_WEEK.md` and `PROVENANCE.md` disclosures still described the obsolete
  second 121/35 repair. The frozen 125/38 tree remained unpushed.
- 2026-07-14 fourth product-shell repair: Core holds a private user-scoped
  SQLite exclusive instance lock for its full lifetime, and Launch Services is
  also told that multiple app instances are prohibited. A deterministic test
  launches a second independent host test process against the same production-
  shaped support directory, proves it fails with `AlreadyRunning`, releases
  the first process, and proves clean takeover. Because only one Host can own
  the Store/Codex authority, global Off's process-local challenge invalidation
  and cancellation token cover the only running model process. All live
  disclosure surfaces now report the fourth repair and 126 Rust/38 Swift local
  tests. Release/lint/format/plist/script/diff checks, deep ad-hoc verification
  of `/private/tmp/OpenOpen-Stage-Repair6.app`, and two consecutive exact
  pinned-runtime diagnostics pass. Two fresh isolated reviewer PASS reports
  remain required before any product-shell commit or push; no signed/admin,
  cross-UID, real-provider, notarization, product-E2E, GitHub CI, or release
  proof is claimed.
- 2026-07-14 fifth product-shell reviewer cycle: fresh security Reviewers I and
  J both FAIL the frozen fourth repair. They independently found that Host
  released the user lock before detached model/Codex work was proven dead and
  that any same-EUID process could unlink/recreate the user-owned SQLite lock
  path, splitting exclusion. The frozen `5c1b663…` tree remained unpushed.
- 2026-07-14 fifth product-shell repair: the user-owned lock is no longer a
  security authority. The root effect broker now persists one signed lease per
  audit EUID in its protected SQLite state, binding authenticated App and Core
  PIDs plus start times, exact signed bundle layout, and a fresh Host nonce.
  Core verifies the enrolled broker signature and requires the lease for model
  routes and every On prepare/commit/recover path. Core is a private process-
  group leader, and every spawned pinned Codex process is actively checked to
  inherit that exact PGID; broker Off first durably stores
  protected Off while retaining the old lease, sends TERM/KILL to the exact
  leased group, waits until the complete PGID is empty, exact-CAS clears the
  lease, and only then returns acceptance to App. Tests prove
  daemon-restart persistence, exactly one concurrent acquire winner, exact
  release, caller-authority rejection, process-incarnation binding, stale
  group retirement, Off-before-acceptance reaping, and no-lease fail-closed
  behavior. All 129 Rust tests and 43 Swift tests pass, together with Rust
  release/fmt/strict Clippy, Swift warnings-as-errors test/release/strict
  format, plist/script checks, deep ad-hoc verification of
  `/private/tmp/OpenOpen-Stage-Repair10.app`, and two consecutive exact pinned-
  runtime diagnostics. Two entirely fresh reviewer PASS reports remain
  required before commit/push; signed/admin, cross-UID, real-provider,
  notarization, product-E2E, product-shell GitHub CI, and release proof remain
  unclaimed.
- 2026-07-14 sixth product-shell reviewer cycle: fresh security Reviewer M
  FAILS frozen fingerprint `6ce2ef2…`. It finds that a daemon crash between
  protected Off persistence and PGID reaping can leave the old model group
  running, and that an unrelated process reusing the old Core PID can wedge
  lease retirement. Governance Reviewer N was immediately canceled because
  the frozen tree was invalidated; no partial result is counted as PASS.
- 2026-07-14 sixth product-shell repair: Global Off now retains the old lease,
  validates the exact Core incarnation, delivers SIGKILL to its exact PGID,
  and proves the group empty before protected Off persistence. A failed signal
  therefore causes rejection without a protected-state write; after successful
  signal delivery the old group cannot finish even if the daemon exits. Only
  then does the broker persist Off, exact-CAS release the lease, and return
  acceptance. A different start time proves PID reuse: the unrelated process
  receives no signal while the signed stale lease is exactly released. The
  focused reused-PID, kill-before-persistence, and failed-kill/no-persistence
  tests pass; full evidence is 129 Rust and 45 Swift tests. Two entirely fresh
  reviewers remain required before commit/push; signed/admin, cross-UID,
  real-provider, product-shell GitHub CI, notarization, product E2E, and release
  proof remain unclaimed.
- 2026-07-14 seventh product-shell reviewer cycle: fresh governance Reviewer P
  FAILS frozen fingerprint `8905784…` because two canonical master-plan status
  paragraphs still described the completed foundation/fifth repair as current.
  Security Reviewer O is canceled immediately after invalidation, and no
  partial result is counted. The resume point and blocker are corrected to the
  sixth code repair, seven issue-finding cycles, two fresh reviews, and then
  product-shell commit/push/current CI. No product code or test evidence changes.
- 2026-07-14 GitHub identity audit: `gh auth status` lists only `thesongzhu`,
  `gh api user` returns `thesongzhu`, and local Git author configuration uses
  `thesongzhu` with the account-ID noreply address. Public
  `thesongzhu/OpenOpen` already exists with the reviewed bootstrap and draft
  foundation-CI PR facts recorded below; the obsolete `mxclip` authentication
  blocker is closed. This does not authorize an early product-shell push.
- 2026-07-14 eighth product-shell reviewer cycle: fresh security Reviewer Q
  and governance Reviewer R both FAIL frozen fingerprint `1dda502…`. Reviewer
  Q finds the mutable-PGID escape, numeric killpg PID-reuse TOCTOU, and missing
  exact running-Core identifier binding. Reviewer R finds the direct
  `zeroize 1.9.0` notice missing. No partial result is counted as PASS.
- 2026-07-14 seventh product-shell repair: the root broker now snapshots and
  signs exact Core and persistent-Codex Mach audit tokens, rejects an identity
  if its token changes across inspection, validates exact running Core and
  Codex signing requirements, and terminates Codex then Core by audit token
  before persisting Off and exact-CAS releasing the durable lease. The outer
  sandbox denies fork, eliminating unregistered descendants; PGID is no
  longer security authority. Direct `zeroize 1.9.0` disclosure is present.
  All 130 ordinary Rust tests and 37 broker/signing plus 12 App Swift tests,
  strict release/lint/format/plist/script/diff checks pass; the explicit real
  runtime test passes twice. Repair12 was rejected when deep signing rewrote
  the Core identifier; the corrected exclusive Repair14 stage preserves every
  exact identifier/hash and passes deep ad-hoc verification. Two entirely
  fresh reviewer PASS reports remain required before commit/push; no
  signed/admin, cross-UID, real-provider, product-shell CI, notarization,
  product-E2E, or release proof is claimed.
- 2026-07-14 ninth product-shell reviewer cycle: fresh security Reviewer S and
  governance Reviewer T both FAIL frozen fingerprint `81b20d6…`. Reviewer S
  finds that persistent Codex initialization preceded the durable broker lease,
  App/Core shutdown and cancellation still held numeric PID/PGID signal
  authority, and the root worker timeout path could signal a reused numeric
  PID. Reviewer T finds that `BUILD_WEEK.md` disclosed seven rather than eight
  earlier issue-finding cycles. No partial result is counted as PASS.
- 2026-07-14 eighth product-shell repair: the exact pinned Codex child now
  starts uninitialized and accepts only the initialization handshake; no
  account, model, or other request can precede the full broker-persisted and
  Core-installed exact audit-token lease. A pre-lease failure aborts through
  the unreaped Rust-owned `Child` handle, while a post-lease failure remains
  fail-closed under durable broker authority. App/Core cleanup closes pipes and
  waits without numeric signals. The root broker snapshots each worker audit
  token before sending request bytes and reaps timeout/error paths only by that
  exact token. All 131 ordinary Rust tests and 39 broker/signing plus 14 App
  Swift tests, strict release/lint/format/plist/script/diff checks pass locally.
  Exclusive `/private/tmp/OpenOpen-Stage-Repair15.app` passes deep strict
  ad-hoc verification with the exact App/Core/broker/worker identifiers and
  pinned Codex identifier, Team, CDHash, and four manifest hashes; the real
  sandbox initialize/account-read diagnostic passes twice. Two entirely new
  reviewer PASS reports remain required before commit/push. No product-shell
  CI, signed/admin, cross-UID, real-provider, notarization, clean-install,
  product-E2E, external-user, or release proof is claimed.
- 2026-07-14 tenth product-shell reviewer cycle: fresh security Reviewer U
  FAILS frozen fingerprint `dd3b1cea…`; governance Reviewer V PASSES that same
  tree, but the PASS cannot be reused after the security-invalidating edit. U
  proves repeated provisioning fails on duplicate Codex initialization before
  Off can cancel Core work or reach the broker, and proves the first worker
  audit token can bind a PID-reused unrelated root process. V independently
  validates the then-current counts, Repair15, remote facts, and unclaimed
  tiers. The eighth repair is superseded; no partial gate is carried forward.
- 2026-07-14 ninth product-shell repair: broker trust and Codex readiness are
  separate. Only On/model paths prepare the exact Codex lease, readiness is
  cached for one Core instance nonce, and Host initialization is idempotent
  under the same immutable lease. Off never spawns, reacquires, or initializes
  Codex; it first clears challenges/cancels active Core work, then applies Off
  against the durable broker lease, so dead Codex or future acquire failure
  cannot block it. Worker authority now requires no observed termination and a
  stable token→identity→token snapshot bound to exact PID, daemon parent, root
  EUID, nonzero start time, and canonical protected executable before any
  request bytes. All 131 ordinary Rust tests and 40 broker/signing plus 15 App
  Swift tests and the full strict verification pass. Exclusive Repair16 passes
  deep strict ad-hoc verification with exact identities/hashes, and its real
  sandbox diagnostic passes twice. Two entirely fresh reviewer PASS reports
  remain required before commit/push; no product-shell CI, signed/admin,
  cross-UID, real-provider, notarization, clean-install, product-E2E,
  external-user, or release proof is claimed.
- 2026-07-14 eleventh product-shell reviewer cycle: fresh security Reviewer W
  FAILS frozen fingerprint `dd9ad888…`; governance Reviewer X PASSES the same
  tree, but its PASS cannot be reused after the security-invalidating edit. W
  proves that Off was published before Core cancellation or broker proof, so a
  repeated provisioning failure could leave protected On and active work behind
  a false-Off UI; dashboard failure had the same false-Off result. X validates
  the prior lifecycle repair, counts, Repair16, remote facts, and honest proof
  boundaries. The ninth repair is superseded; no partial gate carries forward.
- 2026-07-14 tenth product-shell repair: authoritative protected state, desired
  state, model-entry permission, and transition/unknown presentation are now
  separate. Off intent advances generation and immediately blocks new model
  entry, then clears the Core challenge and cancels active work before any
  fallible broker-trust call. A known-On runtime reports Off only after broker
  acceptance or fresh matching protected proof; a fresh Core with no protected
  history may report its explicit default-Off state. Pre-apply failure preserves
  the last certain state, while response loss, dashboard failure, and mismatches
  show Unknown. All 131 ordinary Rust tests and 40 broker/signing plus 21 App Swift
  tests pass together with the strict verification set. Exclusive Repair17
  passes deep strict ad-hoc verification with exact identities/hashes, and its
  real sandbox diagnostic passes twice. Two entirely fresh reviewer PASS
  reports remain required before commit/push; no product-shell CI,
  signed/admin, cross-UID, real-provider, notarization, clean-install,
  product-E2E, external-user, or release proof is claimed.
- 2026-07-14 isolated pre-freeze security audit: Repair17 is rejected before
  formal Stage 5. The audit reproduces explicit Off intent erased by refresh,
  reusable cancellation identity and early active-slot removal, initial and
  nondefault false-Off presentation, and a stale await stranding newer intent.
  A first repair re-audit finds the narrower login install/cancel lock race.
  This is issue-finding process evidence, not a required formal reviewer PASS.
- 2026-07-14 eleventh product-shell repair: explicit user intent remains pending
  until convergence; startup is Unknown; only exact Core default Off is accepted
  without broker history; protected/Core revision and timestamp must match; a
  pending Off never prepares Codex; stale generation failure continues to the
  latest intent. Host uses unique per-operation cancellation tokens, retains
  canceled active work until exact finish, and serializes login install/cancel
  under one exact-token `active → login` boundary. All 133 ordinary Rust tests
  and 40 broker/signing plus 24 App Swift tests pass with strict build/lint/
  format checks. Repair18 is freshly staged and explicitly ad-hoc; deep exact
  identity/four-hash verification and two real-runtime diagnostics pass. Two entirely
  fresh formal reviewer PASS reports remain required before commit/push; no
  product-shell CI, signed/admin, cross-UID, real-provider, notarization,
  clean-install, product-E2E, external-user, or release proof is claimed.
- 2026-07-14 twelfth product-shell reviewer cycle: fresh security Reviewer Y
  FAILS frozen fingerprint `2426b866…`; governance Reviewer Z PASSES that same
  tree, but its PASS cannot be reused after the security-invalidating edit. Y
  proves a canceled pending login could clear the active slot and let a new
  route reset the shared Codex cancellation flag while the protected broker
  still reported the prior On state. Y also finds that App model authorization
  compared recovered enabled/revision but omitted `updatedAtMs`. Z validates
  the then-current counts, Repair18, remote facts, and honest unclaimed tiers.
  No partial gate carries forward.
- 2026-07-14 twelfth product-shell repair: one locked Host operation gate now
  owns startup-unknown, enabled, and revision-bound pending-Off authority with
  the exact active token. Off cancellation clears pending login state and
  cannot be released by replaying an older On commit or recovery; only a
  sufficiently new broker-protected On revision can reopen Codex work. App
  model authorization now requires recovered enabled, revision, and
  `updatedAtMs` to equal the protected authorization. A deterministic test
  covers canceled-login slot release plus old-On commit/recovery replay and
  fresh-revision recovery. All 134 ordinary Rust tests and 40 broker/signing
  plus 25 App Swift tests pass with the strict build/lint/format/plist/script/
  diff and credential scans. Repair19 is freshly staged and explicitly ad-hoc;
  deep exact identity/four-hash verification and two correctly selected real
  pinned-runtime diagnostics pass. Two entirely fresh reviewer PASS reports
  remain required before commit/push; no product-shell CI, signed/admin,
  cross-UID, real-provider, notarization, clean-install, product-E2E,
  external-user, or release proof is claimed.
- 2026-07-14 thirteenth product-shell reviewer cycle: fresh security Reviewer
  AA FAILS frozen fingerprint `b0d9e514…`; governance Reviewer AB PASSES that
  same tree, but its PASS cannot be reused after the security-invalidating
  edit. AA proves a same-UID regular-file replacement of bundled
  `OpenOpenCore` could receive the Keychain master because App checked only
  path shape before launch and broker code identity only later. AB validates
  the then-current Repair19 counts, stage, remote facts, provenance, and
  unclaimed tiers. No partial gate carries forward.
- 2026-07-14 thirteenth product-shell repair: App now requires the current host
  identity, exact Core signing identifier, and identical Team in a strict
  static-code check before process launch. It then derives the running Core's
  Mach audit token and checks the same requirement against that exact process
  incarnation before invoking the Keychain master loader or writing bootstrap
  bytes. Tests prove a regular unsigned replacement and a running-auth failure
  both leave the master-loader count at zero; fake-Core lifecycle tests inject
  only explicit test validators. All 134 ordinary Rust tests and 40 broker/
  signing plus 27 App Swift tests pass with strict build/lint/format/plist/
  script/diff and credential scans. Repair20 is freshly staged and explicitly
  ad-hoc; deep exact identity/four-hash verification and two correctly selected
  real pinned-runtime diagnostics pass. No signed/admin, cross-UID,
  real-provider, notarization, clean-install, product-E2E, external-user, or
  release proof is claimed.
- 2026-07-14 Repair20 Stage 5: fresh security Reviewer AC and governance
  Reviewer AD both PASS frozen fingerprint `29a004131a5987713eb4500de5060312ab954fde5c6bf0cbbd7efa2d4bb142ac`
  with no P0/P1/P2 findings. AC verifies static exact Core identifier/App-Team
  validation, running Mach audit-token validation, Keychain ordering, failure
  cleanup, and protected Off/timestamp invariants. AD independently verifies
  reviewer accounting, test counts, live remote facts, staging/provenance, and
  no-overclaim boundaries. Both confirm the fingerprint is unchanged.
- 2026-07-14 product-shell Stage 6/7: reviewed commit
  `e2313fe8b28cbdb8aac4bc41661394d8e39806cd` was pushed to
  `agent/product-shell` and draft PR #2 was opened against `main`. Actions run
  `29386477267` passed every strict Rust/Swift/build/lint/format/plist/script/
  clean-diff step. GitHub's pull-request run used synthesized merge
  `487dae18c6c3030e78d0200698981f9b6f33f4f1`; its tree
  `2cae9eb80e7bb07a988565999c29579abbf21a3d` equals the PR head tree. This is
  integration-tree plumbing evidence only, not exact-head, signed/admin,
  cross-UID, real-provider, notarized, clean-install, product-E2E,
  external-user, or release proof. PR #2 remains draft and unmerged.
- 2026-07-14 Hero A local implementation candidate: explicit text input uses
  the existing protected GPT-5.6 structured Outcome route; Core consumes the
  exact in-memory suggestion and persists typed Create, confirmation, owner
  approval, and activation commands; EventKit requests full Reminders access,
  creates an exact OpenOpen mirror in the default Reminders source, and returns
  stable calendar-item identifiers; exact completed-item readback becomes
  signed `ReminderCompleted` Evidence before Core can issue a Receipt. Invalid,
  partial, duplicate, changed, missing, or future completion input fails
  closed, and permission failure retains the Mission for explicit retry.
  All 139 ordinary Rust tests and 40 broker/signing plus 30 App Swift tests,
  release builds, strict lint/format/plist/script/diff/credential checks pass.
  `/private/tmp/OpenOpen-Stage-HeroA.app` is freshly staged and explicitly
  `STAGED_AD_HOC_NOT_RELEASE_PROOF`; two fresh isolated closure reviews remain
  pending. No real provider output, user Reminders write/readback, signed/admin,
  cross-UID, current GitHub CI, notarization, clean-install, product-E2E,
  external-user, or release proof is claimed.
- 2026-07-14 first Hero A closure review: both fresh reviewers FAIL frozen
  fingerprint `1711864f1e5af30f6c7ea6a3ee85630b2c098ddb15274dfe2bcf359e671ca935`.
  They reproduce nonrecoverable partial/response-loss confirmation and
  completion, completed-Mission reuse by a second Outcome, direct EventKit
  write without the required exact `NewExternalWrite` approval, fabricated
  approval time copied from suggestion creation, and the one-test count error.
  The candidate is superseded; repair preserves the frozen shell and adds no
  optional feature or proof claim.
- 2026-07-14 Hero A closure repair: each composite confirm/complete operation
  is one atomic Store transaction over typed commands, audit rows, encrypted
  results, Evidence, and Receipt. Exact response-loss retries reopen the same
  Store and return the same authorized Mission or Receipt; changed completion
  input is rejected without audit movement. Dashboard recovery returns separate
  max-three cards, the newest exact active Mission authorization, and latest
  Receipt. The confirmation click uses its observed time and explicitly binds
  the exact logical Reminders list plus ordered Mission/work-item payload to an
  owner-approved `NewExternalWrite`; App and EventKit both reject an invalid
  authorization before any external writer call. Sequential Outcome tests prove
  a completed Mission cannot be reused. The full 143-Rust/75-Swift suite,
  release builds, strict Clippy/format, shared payload vector, plist/script/diff,
  and credential checks pass locally. Fresh
  `/private/tmp/OpenOpen-Stage-HeroA-Repair1.app` passes deep ad-hoc staging and
  is explicitly `STAGED_AD_HOC_NOT_RELEASE_PROOF`. Two fresh reviewers are
  pending; no real provider/Reminders, signed/admin, cross-UID, CI, notarized,
  clean-install, product-E2E, external-user, or release proof is claimed.
- 2026-07-14 Hero A Repair1 replacement review: governance Reviewer E PASS and
  functional Reviewer F FAIL the unchanged fingerprint `3e839145…`. Reviewer F
  proves that restarting after a successful EventKit write, then renaming the
  original list or changing the default account, could resolve the same logical
  approval against another physical calendar and duplicate the mirror. Both
  reviewers independently confirm the 143-Rust/75-Swift suites and unchanged
  fingerprint; the governance PASS is historical only after the repair.
- 2026-07-14 Hero A Repair2: Swift resolves the EventKit source/calendar before
  Core confirmation without writing. Core persists that exact target in the
  audited `NewExternalWrite` approval and V2 payload digest; a response-loss
  retry with a changed source/calendar is rejected without audit movement.
  EventKit recovery is restricted to the approved source/calendar and recovers
  exact Mission markers even after a list rename; missing or ambiguous physical
  targets fail closed. The shared Rust/Swift V2 vector is `188605fc…`.
  All 144 ordinary Rust tests and 40 broker/signing plus 37 App Swift tests,
  release builds, strict Clippy/format, plist/script/diff checks pass. Fresh
  `/private/tmp/OpenOpen-Stage-HeroA-Repair2.app` passes deep ad-hoc staging and
  is explicitly `STAGED_AD_HOC_NOT_RELEASE_PROOF`. Two fresh reviewers remain
  pending; no real provider/Reminders, signed/admin, cross-UID, CI, notarized,
  clean-install, product-E2E, external-user, or release proof is claimed.
- 2026-07-14 Hero A Repair2 replacement review: fresh functional and governance
  reviewers both FAIL frozen fingerprint `76ca9834…`. The functional reviewer
  proves that a restart after every Mission marker is deleted, moved, or made
  unrecognizable produces zero recovered markers and Repair2 recreates the full
  mirror. The governance reviewer proves that an approval with no initial
  calendar identifier could bind to a newly appearing same-name list later,
  and that cancellation after awaited discovery could still enter calendar
  persistence. Both rerun the 144-Rust/77-Swift suites and verify the unchanged
  fingerprint. Repair2 is superseded; neither report is a PASS.
- 2026-07-14 Hero A Repair3: the user must pre-create one uniquely selectable
  OpenOpen Reminders list; OpenOpen does not create or silently choose a
  calendar. Core issues `createOnce` only in the original confirmation response
  and `recoverOnly` for response-loss recovery, dashboard, and restart. Exact
  EventKit readback is recorded atomically as signed `ReminderMirrored`
  Evidence for every WorkItem. Persisted links restore without another EventKit
  write; missing or partial markers without persisted links fail closed.
  Cancellation is checked after awaited work and around the final EventKit
  commit/readback boundary. All 145 ordinary Rust tests pass with one exact-
  runtime test ignored unless its pinned binary is supplied; 40 broker/signing
  and 40 App Swift tests pass. Release builds, strict Clippy, Rust/Swift format,
  plist/script, and diff checks pass locally. Fresh
  `/private/tmp/OpenOpen-Stage-HeroA-Repair3.app` reports
  `STAGED_AD_HOC_NOT_RELEASE_PROOF` and passes deep exact-identity and pinned-
  hash verification. Two entirely fresh reviewers remain pending. No real
  ChatGPT output, user
  Reminders mutation/readback, signed/admin, cross-UID, current CI,
  notarization, clean install, product E2E, external-user, or release proof is
  claimed.
- 2026-07-14 Hero A Repair3 replacement review: both fresh reviewers FAIL
  frozen fingerprint
  `fa9d905ec85907719c98c4f968fff497261677a2e175e6631b6f34ccebad1417`.
  They independently prove the same route: EventKit may commit, then readback
  may fail or Off may intervene while the App still retains volatile
  `createOnce` authority. If every marker is later deleted, moved, or mutated,
  retry can issue a second batch. The three repairs share this missing durable
  dispatch invariant; the supervisor returns `STUCK: same_root_cause`. The
  owner's standing approval of recommended in-direction fixes selects strict
  at-most-once dispatch. Repair3's green tests and stage are historical only.
- 2026-07-14 Hero A Repair4: Core route `mission.reminders.begin` atomically
  persists signed deterministic `ReminderDispatchStarted` Evidence for every
  WorkItem before EventKit. The first committed call alone returns
  `executeNow=true`; exact response-loss/restart retries return false.
  App caches the resulting recovery-only Mission before EventKit and splits
  external execution from strictly read-only recovery. Marker v2, readback
  links, and signed `ReminderMirrored` Evidence bind the exact per-item dispatch
  token. Deterministic tests cover begin response loss/restart, precommit
  failure, post-commit readback failure, Off after commit, missing recovery,
  exact persisted links, and zero second writes. All 146 ordinary Rust tests
  pass with one pinned-runtime test skipped in the ordinary run; 40
  broker/signing and 42 App Swift tests pass. Release builds, strict Clippy,
  warnings-as-errors, Rust/Swift format, plist/script/diff checks, and two
  explicit pinned-runtime sandbox diagnostics pass. Fresh
  `/private/tmp/OpenOpen-Stage-HeroA-Repair4.app` reports
  `STAGED_AD_HOC_NOT_RELEASE_PROOF` and passes exact identity/four-hash
  staging. Two entirely fresh reviewers remain required. No real ChatGPT
  output, user Reminders mutation/readback, signed/admin, cross-UID, current CI,
  notarization, clean install, product E2E, external-user, or release proof is
  claimed.
- 2026-07-14 Hero A Repair4 replacement review: functional Reviewer A PASS and
  governance Reviewer B FAIL frozen fingerprint
  `4cabaeb4c041ef383cca8ca64a4f9bf1e9cf8fe0b3ebd36284e9abe983e40b58`.
  Governance proves the lower-level public EventKit writer accepted a reusable
  `ConfirmedMission` rather than consuming the one-shot start: retain the first
  Mission, execute, delete every marker, call the writer again, and
  `allowMissingAll` can commit a second batch. Both reviewers rerun the complete
  suites and keep the fingerprint unchanged. Repair4 is superseded; its
  functional PASS cannot carry across the safety edit.
- 2026-07-14 Hero A Repair5: the EventKit writer is no longer public and now
  accepts the full `ReminderDispatchStart`. A process-local execution gate
  consumes the Mission claim before permission requests, marker discovery, or
  EventKit; replaying the retained first start fails before any external
  boundary. Restart cannot recreate that start because durable Core dispatch
  returns only `executeNow=false` and App selects strictly read-only recovery.
  A direct retained-start regression test is added. All 146 ordinary Rust
  tests pass with one pinned-runtime test skipped in the ordinary run; 40
  broker/signing and 43 App Swift tests pass. Release builds, strict Clippy,
  warnings-as-errors, Rust/Swift format, plist/script/diff checks, and two
  explicit pinned-runtime sandbox diagnostics pass. Fresh
  `/private/tmp/OpenOpen-Stage-HeroA-Repair5.app` reports
  `STAGED_AD_HOC_NOT_RELEASE_PROOF` and passes exact identity/four-hash
  staging.
- 2026-07-14 Hero A Repair5 closure review: two entirely fresh isolated
  reviewers PASS frozen fingerprint
  `4b41a04f7b28573e1a04cb19c79f499b497a2240efbcc236f003f4feb97971cf`
  before and after, with zero P0/P1/P2 findings. Functional review traces the
  sole internal EventKit writer, complete one-shot start consumption, durable
  restart/Off recovery-only behavior, target/token/link bindings, completion
  Evidence/Receipt, and sequential Mission isolation. Governance independently
  verifies the same live routes, Store/audit/ActionGate/global-Off boundaries,
  disclosure history, remote facts, and no alternate client/writer path. Each
  reruns all 146 ordinary Rust tests and 40 broker/signing plus 43 App Swift
  tests, strict release/lint/format/plist/script/diff checks, ad-hoc staging,
  and two pinned-runtime sandbox diagnostics. Hero A Stage 5 passes; reviewed
  commit/push and current Actions remain pending. No real ChatGPT output, user
  Reminders mutation/readback, signed/admin, cross-UID, notarization, clean
  install, product E2E, external-user, or release proof is claimed.
- 2026-07-14 Hero A Stage 6/7: reviewed commit
  `774789ca4a5eeadb8fa57688e79f823dec4da65b` is pushed to
  `agent/product-shell` and draft PR #2. Current Actions run `29393462659`
  passes all strict Rust/Swift test, release, lint, format, plist, script, and
  clean-diff steps. The pull-request run used synthesized merge
  `bccdf360d8ad1b97b56eaf4e9603007bd0584a01`; its tree
  `e8f3605e0644e23d6f2cd5f6557b2ca6d917077c` equals the exact head tree.
  This is integration-tree plumbing evidence, not real ChatGPT/Reminders,
  exact-head, signed/admin, cross-UID, notarized, clean-install, product-E2E,
  external-user, or release proof. PR #2 remains draft and unmerged.
- 2026-07-14 owner decision: the accelerated intermediate milestone is named
  `FRIDAY_ALPHA_READY` and targets July 16–17, 2026
  `America/Los_Angeles`. It requires the reviewed Hero A loop plus real
  bidirectional iMessage and Discord entry/readback for that same bounded
  Mission; allowlisting/pairing, durable dedupe/cursor recovery, restart without
  duplicate send, and global Off listener/model/outbound shutdown are
  mandatory. A sent message is never completion Evidence. Hero B and Hero C
  move after this milestone but remain in the final plan. Two similar
  Evidence-complete Hero A successes may then propose one Workflow Candidate,
  but that slice must not delay the first installable alpha.
- 2026-07-14 alpha provenance verification: `openclaw/imsg` annotated v0.13.0
  tag object `1677a9fe…` dereferences to exact commit `fa2f82d…`; its MIT text
  names copyright 2026 Peter Steinberger. `serenity-rs/serenity` v0.12.5
  resolves to exact commit `1809beb…`; its manifest and license declare ISC,
  copyright 2016 Serenity Contributors. Friday Discord adapter contract/test
  semantics remain pinned to MIT commit `4870f31…`; no Friday TypeScript/Node
  runtime is imported.
- 2026-07-15 Friday implementation checkpoint: shared channel records and the
  encrypted Store now own exact pairing, message-ID/cursor dedupe, once-only
  model/outbound start, Mission origin, delivery reconciliation, and Off
  behavior. The imsg adapter uses one child and a tracked exact-commit patch
  reducing CLI/RPC/send to the approved surface; `Package.resolved` locks its
  three Swift dependencies and the build receipt binds the produced artifact.
  The Discord adapter uses the exact serenity Git commit and official bot
  Gateway/HTTP only; Swift stores the token in a device-only Keychain item.
  Complete current-tree verification passes 175 Rust tests with one explicit
  environment-gated pinned-runtime test plus 40 broker/signing and 47 App Swift
  tests, release builds, strict Clippy/warnings/format, plist/script/diff, and
  two pinned imsg boundary tests. `/private/tmp/OpenOpen-FridayAlpha-Final.app`
  completes receipt validation, nested imsg ad-hoc signing, and deep
  verification with `STAGED_AD_HOC_NOT_RELEASE_PROOF`. Its ad-hoc, unnotarized
  DMG passes read-only mount, isolated copy, and signature verification with
  SHA-256 `0f9b7fd3ca54c27138c52fe42a0cb31a3a4a13260d0d945a954d608cab39bd15`.
  This is local implementation/package evidence only. Two fresh reviewers and
  real GPT-5.6/Reminders/iMessage/Discord message IDs remain pending;
  `FRIDAY_ALPHA_READY` is not claimed.
- 2026-07-15 first Friday-alpha closure review: functional and governance
  reviewers both FAIL unchanged fingerprint
  `136a42ba505d270f8d8ca3f26b99990d340deb0d8a48e3872cf9573f14df0d69`.
  The accepted observation/cursor could commit before model work was durably
  enqueued; Need-you and Receipt return kinds were unreachable; Discord still
  required manual owner/channel/bot IDs and lacked the approved official
  install, intent, permission, and attachment probe; imsg runtime validation
  followed symlinks and bound only the pre-sign hash; private IMCore/SIP/bridge
  sources still compiled into the Mach-O; the Connections status text was
  literal/stale; a public Store method could bind an existing Mission to an
  accepted message outside channel-origin genesis; and the DMG notices were
  incomplete. Both reviewers independently reran the complete 175-Rust and
  87-Swift suite and preserved the fingerprint, so the green suite does not
  close the findings. A subsequent read-only packaging audit also proves the
  staged imsg references a `PhoneNumberKit_PhoneNumberKit.bundle` under a
  deleted temporary build root while the App/DMG contains no such bundle.
  Therefore DMG `0f9b7fd3…` is retained only as historical ad-hoc mount/copy/
  signature evidence, not as a runnable alpha candidate. Repair attempt 1 is
  in progress; no Friday gate, push, provider proof, or release proof is
  claimed.
- 2026-07-15 Friday-alpha Repair1 local candidate closes the reported routes.
  Accepted inbound content, its cursor, and a durable queued model dispatch now
  share one Store transaction; restart claims only the oldest queued item.
  Channel-origin Mission binding exists only in atomic `CreateMission` genesis.
  Exact current Need-you and Evidence-backed Receipt payloads use typed
  outbound authorization/readback, while UI connection status is separate from
  poll event status. Discord now uses a token-derived official three-step bot
  wizard, exact permission bits `101376`, a random 128-bit pairing code, live
  intent/permission/history probes, explicit candidate confirmation, and
  persisted bot/application/guild/source identity. iMessage compiles only a
  positive source whitelist, carries the locked PhoneNumberKit resource tree,
  and uses prepare → running Mach-identity validation → activate so no RPC
  request bytes precede the signed-child check. The deterministic notice
  payload covers 190 OpenOpen and 924 Codex dependency identities through 597
  content-addressed texts. Current local verification passes 186 Rust tests
  with one explicit environment-gated Codex runtime test, plus 40 broker/signing
  and 49 App Swift tests. Fresh
  `/private/tmp/OpenOpen-FridayAlpha-Repair1-Final.app` contains exactly four Codex
  runtime files, the complete three-file imsg runtime, signed-runtime receipt
  (`binarySha256=72242c5c…`, resource tree `7a5cb869…`), all notice texts, and
  passes deep ad-hoc staging plus a real staged basic-RPC resource probe. Its
  ad-hoc, unnotarized DMG passes read-only mount/copy/signature install testing
  at SHA-256 `04f02c846f481b8a5604c260f600920f3e0b7660ab6829c45b77f356352f9091`.
  It is
  labeled `STAGED_AD_HOC_NOT_RELEASE_PROOF`; its Team is intentionally `not
  set`, so it is not a Developer-ID runnable alpha or release artifact. Two
  entirely fresh replacement reviewers remain pending, followed by real
  GPT/Reminders/iMessage/Discord proof. No push or
  `FRIDAY_ALPHA_READY` claim has occurred.
- 2026-07-15 Friday-alpha Repair1 replacement review: two entirely fresh
  isolated reviewers FAIL unchanged fingerprint
  `10160bb13293036008479241224cc2f34c842bd5433c5c44468346ef4ca7d01d`.
  Functional review proves the Host passed already-prefixed approved iMessage
  wire bytes into an adapter that accepts only unprefixed content, so every
  live iMessage send failed before RPC; it also proves the patched basic send
  result returned no real GUID for durable `Sent`. Governance review proves a
  failed iMessage activation/proof or repeated Discord setup could retain a
  prepared session and wedge retry. The ledger also prematurely said two
  reviewers had completed. Both reports preserve the fingerprint. Repair1 is
  superseded; no prior local green result or ad-hoc package closes these routes.
- 2026-07-15 Friday-alpha Repair2 preserves the exact Store-approved final
  `OpenOpen · AI` wire body while the Host strips exactly one authorized prefix
  before the adapter reconstructs it. The pinned basic sender records the
  pre-send database high-water, sends once, and returns only a unique exact
  same-chat/text local row's real GUID; zero or ambiguous results remain
  uncertain. Restart scans are bounded, read-only, and never resend. Swift now
  stops a prepared iMessage child after activation/proof failure and stops any
  prior Discord setup before a new setup attempt. The tracked patch applies
  cleanly with the 449-line server and 88-line test additions; four upstream
  OpenOpen tests pass. Full local verification passes 187 ordinary Rust tests
  with one explicitly environment-gated Codex runtime test, plus 40 broker/
  signing and 51 App Swift tests, release builds, strict Clippy/warnings/
  format, plist/script/notices/diff checks, and the fresh pinned imsg build.
  During the first heavily parallel full run, one old Host cancellation timing
  assertion failed once; it then passed 20/20 isolated repetitions and the
  complete exact suite rerun. Fresh
  `/private/tmp/OpenOpen-FridayAlpha-Repair2-Final.app` contains four Codex
  runtime files, 597 notice texts, build receipt SHA-256 `9d867a84…`, resource
  tree `7a5cb869…`, and signed imsg SHA-256 `0d109cbe…`; staged basic RPC and
  deep ad-hoc verification pass. Its ad-hoc, unnotarized DMG passes read-only
  mount/copy/signature install testing at SHA-256
  `15c1429b0b05564b890d5766cd9e1df2aa6d291b5248cc252a11bb7ee2dc02ec`.
  It remains `STAGED_AD_HOC_NOT_RELEASE_PROOF`, Team `not set`, not a
  Developer-ID runnable alpha or release artifact. Two entirely fresh
  replacement reviewers, real GPT/Reminders/iMessage/Discord traffic, signing,
  and notarization remain pending; no push or `FRIDAY_ALPHA_READY` claim has
  occurred.
- 2026-07-15 Friday-alpha Repair2 replacement review: both entirely fresh
  isolated reviewers FAIL unchanged fingerprint
  `1a983c72ad9f70e7cd321c9782e4e127e42e006ba190daec5f76947831064494`.
  Functional review proves a prior Mission's same-text GUID can be selected by
  the restart history scan and incorrectly persisted as the current outbound's
  delivery. Governance review proves prepare response loss can retain the
  child and wedge retry, the product lacks a usable `chats.list` selection
  route, and this ledger's top summary was stale. Both rerun the complete
  187-Rust/91-Swift suite and preserve the tree. Repair2 is superseded; its
  green suite and ad-hoc DMG are not closure proof.
- 2026-07-15 Friday-alpha Repair3 closes only those approved blockers. Every
  iMessage history recovery outcome remains `Uncertain`; only the exact
  synchronous send RPC may bind a provider GUID, after a send-once complete
  two-second candidate observation. Separate discovery prepare/list RPCs each
  require a fresh proof; no request bytes precede exact running Mach identity
  validation, and success, failure, stop, or Off clears the discovery child.
  Only bounded exact-iMessage chats enter the Swift conversation/participant
  pickers. App connection attempts pre-stop old state and all prepare failures
  best-effort stop, including committed-prepare response loss. Fresh pinned v4
  imsg tests pass 5/5 with binary SHA `635c9981…`, build receipt `c1769b40…`,
  and resource tree `7a5cb869…`. The full local tree passes 190 ordinary Rust
  tests plus one environment-gated test, 40 broker/signing plus 53 App Swift
  tests, release/strict lint/format/plist/script/diff checks, two pinned Codex
  diagnostics, and an independent notices check of 190 OpenOpen/924 Codex/
  1888 documents/597 texts. Fresh
  `/private/tmp/OpenOpen-FridayAlpha-Repair3-Final2.app` passes deep ad-hoc
  verification and staged RPC with signed imsg SHA `04736f58…`, build receipt
  `c1769b40…`, runtime receipt `de512e7b…`, resource tree `7a5cb869…`, and Team
  `not set`. Its ad-hoc, unnotarized DMG passes read-only mount/copy/signature
  install verification at SHA-256
  `bff4d18b49e5fa6c01ec365fc3b2676dcb155733be4b6493870e203f21df099d`.
  Two entirely fresh replacement reviewers remain pending. No push, provider,
  Developer-ID, notarization, release, or `FRIDAY_ALPHA_READY` proof is
  claimed.
- 2026-07-15 first Repair3 closure review: fresh functional Reviewer PASS and
  fresh governance Reviewer FAIL unchanged fingerprint
  `11d34c594ec1f1f2988d763a25a76244f477cc854254b390d20db5b88290499a`.
  Functional reports zero P0/P1/P2 after full Rust, focused Swift, patch,
  notices, and package checks. Governance reports one P2 only: the Discord
  provenance paragraph still described historical Repair2 187-Rust/91-Swift
  evidence as “current,” contradicting the Repair3 190-Rust/93-Swift top
  ledger. Its full Swift/focused Rust/static/package/remote checks find no
  product or security blocker. The evidence-only fix now labels Repair2
  historical and Repair3 current; Final2 embeds the corrected provenance and
  passes the same ad-hoc package checks. Two entirely fresh replacement
  reviewers remain required; no push or provider/release claim follows from
  the first functional PASS.
- 2026-07-15 Repair3 evidence-fix replacement review: two entirely fresh
  isolated reviewers PASS unchanged fingerprint
  `3e2015475d98b74d88a3de4c36e3a1aa4e8bcd1659a3356c5f36f7bd68103ae3`
  with zero P0/P1/P2. Functional review confirms only evidence wording and
  derived package signatures changed, while history-never-Sent, send-once
  full-window observation, two-proof discovery cleanup, and UI selection remain
  intact; focused Host/imsg/Swift and pinned upstream tests pass. Governance
  confirms the prior current/historical contradiction is closed across
  provenance, master plan, Build Week, and ledger; Final2 embeds byte-identical
  provenance; ad-hoc/Team-not-set and remote/CI/provider gates remain honest.
  This closes local Friday-alpha Repair3 Stage 5 only. Exact commit/push,
  current-SHA CI, real provider evidence, signing/notarization, and the final
  product gates remain pending.
