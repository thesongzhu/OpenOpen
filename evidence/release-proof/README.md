# OpenOpen Choice Loop proof

Current proof authority comes only from the 2026-07-19 private-agent contract
at the top of the Master Plan. The former Passport/ZIP→proactivity→Skill→
Workflow Candidate competition story is historical and is not the current
critical-path proof.

A proof record binds the exact commit/tree, document fingerprint, selected
account model and actual effort/catalog/protocol provenance, tested build,
scenario count, results, reviewers, CI, and blockers. Scenario count must be
nonzero, every claimed scenario must pass, and blockers must be empty. Private
bodies, credentials, tokens, raw Markdown/imports, message content, and new
Choice Loop user identifiers must not enter new committed proof. Historical
evidence below the Master Plan's non-normative boundary is quarantined history,
may contain legacy raw provider identifiers, and is not a privacy precedent.

## Current source truth

Repair24 source ca26036809609deb381f901b04329328aefa04cb merged as
c86e5903e72dd693d6e3cec6cd455ebd581116e7, and CI run 29707715009 passed
Rust and Swift. This is source/CI identity only. It is not package, installed
runtime, provider, Mission, Reminder, Receipt, or release proof.

## PR1 required implementation scenarios

- Host-owned `choice.begin` must be the sole public first-local-question
  intake/create RPC. It must validate the bounded question, idempotent request
  ID, and exact persisted model/catalog/protocol references; derive the trusted
  Mac source/delivery binding; and commit the initial interpreting session plus
  audit in one SQLite `IMMEDIATE` transaction before model work. Exact replay
  returns the existing operation; changed replay, Off, missing selection,
  unresolved-session conflict, and catalog/protocol drift fail closed.
- The first-question body must be a Keychain-master-key encrypted Store-private
  blob with AAD bound to request/session/envelope/batch identity. Restart may
  retain it only while its operation is recoverable. Cancel or the accepted
  typed-state Markdown render receipt must delete every raw encrypted or
  derived body representation, leaving only accepted typed state plus body-free
  request-digest/audit tombstones. Its plaintext must never enter logs,
  evidence, Receipts, or remote; bounded transient buffers are zeroized where
  supported.
- The first ChoiceSet must be accepted only by a private operation/generation/
  revision/model/catalog/protocol/source-manifest-bound result commit. Stale or
  late output cannot commit, and no public raw snapshot writer may exist.
- English-only first launch must scan the account, require explicit compatible
  model and supported-effort selection, ask one simple question, and only then
  generate the first dynamic ChoiceSet. No model work may occur before
  selection; a model without effort control must use `not_applicable`.
- Natural input must produce one validated InterpretationFrame and exactly three
  materially distinct dynamic choices plus product-owned D.
- D must support multi-turn ordinary conversation through the command-owned
  `choice.select` D variant. The caller supplies bounded untrusted text plus an
  idempotent request ID and never a batch ID; Host derives/seals the exact
  authenticated batch. Exact replay is idempotent and changed/stale/binding-
  drift replay fails closed. A/B/C/D selection alone must never create a
  Mission or effect.
- Every A/B/C/D `choice.select` variant must use one SQLite `IMMEDIATE`
  transaction to persist Selection, create its pending refinement operation,
  advance the exact session revision/state, and append audit. Crash injection
  must prove that no committed Selection can exist without its operation.
- Every post-selection model result must enter only through the private
  Selection/operation/generation/session/interpretation/model/catalog/protocol/
  manifest/audit-bound refinement commit. Exact retry is idempotent; late, Off,
  cancel, Selection, revision, or provenance drift must fail closed. One SQLite
  `IMMEDIATE` transaction must complete the pending operation, persist result
  digest plus encrypted InterpretationFrame/new ChoiceSet, advance session
  state/revision, and append audit; no intermediate result may become model- or
  UI-visible.
- Explicit complete Mac D intake must atomically persist encrypted body,
  Host-derived envelope/sealed batch, Selection, pending operation, session,
  digest-bound request registry, and audit. Any future quiet-window collection
  must persist encrypted envelopes/open batch transactionally and seal only
  with Selection/operation/session/audit. Collect/seal/commit crash recovery
  must pass. After cancel or an accepted typed-state render receipt, every raw
  encrypted or derived body representation must be deleted; only the accepted
  typed state plus body-free request digest/audit tombstones may remain. No
  plaintext may enter logs, evidence, Receipts, or remote.
- Same-surface burst batching and one global ChoiceSession must pass
  deterministic tests. Host-owned persisted-deadline soft-idle/stale-review
  transitions must bind expected revision/generation; the private Host derives
  target/time from same-boot continuous monotonic or safe reboot clock evidence.
  Timer hints alone start no model/effect work; sleep, backward/ambiguous clock,
  exact replay, restart, late timer, input, cancel, and Off races fail closed.
- The complete compatible account catalog must be shown and the user must
  explicitly select a model and supported effort. Selection, requested/actual
  effort, catalog/protocol revision, turn,
  Mission, and Receipt provenance remain exact. Missing/removed/incompatible
  selection produces Need you; no fixed Sol, Auto, or fallback route exists.
- Bounded Markdown manifest/render/diff must prove Store render intent →
  descriptor-safe staging → staged-file sync → no-clobber creation or atomic
  same-directory swap/CAS retaining the displaced base → parent-directory sync
  → exact final and displaced-base digest/inode verification → Store receipt.
  Replacement must atomically retain and validate the displaced base so
  concurrent Owner edits cannot be lost.
  Intent/rename/sync/receipt crash points plus traversal, symlink, hardlink,
  special file, ownership/mode, path/case/Unicode collision, size, digest drift,
  conflicting edit, secret material, prompt injection, restart, and Off must
  pass. Concurrent-edit/partial-manifest cases must fail closed or preserve
  both versions in typed reconciliation.
- One exact consolidated confirmation must drive the typed Reminders/Evidence/
  Receipt/Markdown/next-ChoiceSet implementation path. Partial write,
  duplicate, changed time/list/count, permission denial, Evidence mismatch,
  false Done, and restart must fail closed. A real Reminder write/readback is
  separate `REAL_PRODUCT_PROOF` and remains action-time Owner gated.
- Visible/editable Reminder date/time/timezone proposals derive only from
  explicit user temporal information, must resolve to a future instant, require
  user selection when time is absent, never use a fixed/question-time default,
  and bind exact date/time/timezone/list/count to a new confirmation revision
  after every edit. Confirmation never substitutes for the real-write permit.
- Mac must keep setup, model choice, Dashboard, Settings, confirmation, recovery,
  and Off reachable without requiring channel setup first.

## PR2 required implementation scenarios

- Same-account iMessage self-chat must be an interactive private inbox with exact
  user-versus-OpenOpen classification, echo marker, cursor, dedupe, restart,
  and loop failure closure.
- The one additional read-only source remains unavailable inside PR2. Groups
  are rejected before persistence/model access.
- No wake word may be required to address the dedicated inbox; an optional summon
  phrase refreshes choices only.
- Permission deny/cancel/revoke/regrant/restart must remain deterministic, avoid
  repeated modals and false On, keep Off reachable, and allow zero provider/
  model/effect work until revalidation.

## Minimal B+ scenarios after the Hero checkpoint

- The additional one-to-one iMessage source is post-B+. When later built it is individually selected,
  revision-bound, revocable, and read-only. A second source, groups, ambiguous/
  stale identity, any outbound route, recipient derivation, or reply mirroring
  fails before persistence/model/effect work.
- B+ B2 accepts exactly one real import (singleton or contiguous split) and
  fails closed on immutable-snapshot, path, collision, corruption, limits,
  cancellation, and partial-catalog violations. The supplied export diagnostic
  is local/read-only/no-network/no-retention and emits only redacted PASS/FAIL
  plus bounded failure class.
- B2 preview exposes at most three dynamic candidate cards, uses no fixed
  categories, disposes unselected content, makes no provider request before
  exact Owner consent, and binds selected cards to a revisioned semantic
  Markdown diff plus consolidated confirmation; exactly one Owner-selected
  card and only its confirmed diff may persist.
- B+ C2 binds exactly one public instruction-only Skill's canonical GitHub identity, immutable commit/package/permission
  digests, structural/license limits, Candidate → Staged → Promoted → Runnable,
  exact update/rollback, and zero instruction/script execution. Synthetic
  fixtures cannot satisfy a real Skill lifecycle action. Its one B+ use has
  no external effect and is separately confirmed.
- Each staged phase earns its own Scout, two-reviewer, full-matrix, exact-head
  CI, ordinary merge, and rollback receipt; owned-path preparation is not
  merge or product proof.

## PR3 Discord scenarios — post-B+

- Authenticated owner, expected Bot/application, exact DM, and install binding
  must be verified. The Bot token must be at rest only in Keychain, transient
  only in authenticated local Gateway memory, never logged/persisted, redacted
  from diagnostics, and released/zeroized on stop where supported.
- Only the exact owner-bound personal Bot DM may be interactive. Unrelated
  events must be discarded before body persistence/model access. No OpenOpen
  cloud/shared Bot, normal-user token, or remote queue may exist.
- Gateway loss/reconnect, cursor, restart, token remove/rotate, identity/intent
  drift, and Off must pass. The pre-consent offline recap must use deterministic
  metadata only; only explicit English `Continue` may admit bounded owner-bound
  bodies. No old message may start model or effect work automatically.

## Cross-cutting proof gates

- Pre-freeze and post-freeze Product Scouts find no P0/P1 modal loop,
  unreachable control/state, focus stealing, repeated alert, failure coupling,
  false Done/On, retry/replay, restart, permission, or Off dead end.
- Two fresh reviewers report P0/P1/P2=0/0/0 on the same fingerprint.
- Focused and required full Rust/Swift matrices, strict lint/format, diff,
  bounded secret checks, and exact-head CI pass with content parity.
- Normal PR merge succeeds without admin/owner bypass.
- After normal PR2 merge, a same-main Core+iMessage Hero checkpoint App/DMG offline
  verification receipt binds the exact merged source/tree and content parity,
  then records the
  applicable deterministic structural, signature, and manifest checks. It does
  not install, contact a provider, cross a permission/effect boundary, or reuse
  an older Repair package/receipt.
- After all staged merges, a final same-main integrated App/DMG receipt repeats
  the applicable offline checks and still claims no install/provider/effect.
- Real install, macOS permissions, Discord token, provider send, Mission
  confirmation, Reminder write/manual Evidence, any newly different ZIP
  disclosure, consent to send bounded history excerpts to the selected model,
  Memory-card commit, real Skill selection/promotion/update/enable/rollback/
  first use, and release proof remain exact action-time gates and are never
  inferred from tests. The supplied export is limited to the local/no-network/
  no-retention B2 diagnostic and is not product proof.

Each active integration must earn `IMPLEMENTATION_MERGE_READY` independently
before its shared successor lands. A later gate cannot replace an earlier one.
`REAL_PRODUCT_PROOF` is a
separate exact-SHA receipt after the applicable Owner-authorized install,
permission, channel, Mission, Reminder, Evidence, or installed-runtime action.

## Ineligible substitutes

Mocks, fixtures, screenshots, CI alone, component probes, schema tests,
ad-hoc packages, signatures, historical packages, or old-SHA receipts are
supporting evidence only. They cannot satisfy a real provider, permission,
install, Reminder, or runtime path that is claimed.
