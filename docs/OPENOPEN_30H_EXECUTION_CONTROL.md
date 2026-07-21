# OpenOpen Choice Loop 10-Hour B+ Closure Control

Status: `OWNER_APPROVED_EXECUTION_CONTROL`

Canonical rules:
`/Users/jarvis/Desktop/agents-generic-phase-batch.md`

Product contract:
`docs/OPENOPEN_PRIVATE_AGENT_CHOICE_LOOP_DESIGN.md`

## 1. Frozen starting facts

Execution start date: 2026-07-19 PDT.

Repair24 source/CI identity:

```text
source_head = ca26036809609deb381f901b04329328aefa04cb
main_merge  = c86e5903e72dd693d6e3cec6cd455ebd581116e7
ci_run      = 29707715009
```

PR #2 is merged and both CI jobs passed. This triple proves source/CI identity
only. It is not installation, installed-runtime equality, package, provider,
Mission, Reminder, or release proof.

The original `agent/product-shell` worktree at `3fd5a9e…` is intentionally
preserved with 16 modified files and is behind its remote branch. It must not
be pulled, merged, reset, cleaned, staged, used as a writable fork source, or
treated as current proof. Repair23 Dashboard input is unconsumed/invalid for
Repair24 and must never be replayed.

The Choice Loop PR1 worktree started clean from `c86e590…` on branch
`agent/choice-loop-pr1`. It now contains the Implementation Task's preserved
in-progress PR1 diff plus the Advisor-owned canonical document patch. Neither
is discarded or treated as proof. Further implementation resumes only after
the updated document fingerprint passes review and receives the exact handoff.

Existing isolated support heads:

```text
B2 Deep ZIP  = 146305766398a37a1b66f89649b1908df6771e76
C2 Skills    = f4ec71c5f6ad9d7a4a852cec3803dc3a8192fb28
```

Both support heads are based on older history. They may prepare in isolated
owned paths while the protected PR1/PR2/Hero checkpoint path runs, but they do
not receive shared-file or merge authority. B2 head `1463057…` includes the
split `conversations-NNN.json` compatibility repair. B2 may run one isolated,
local-only, read-only, no-network, no-retention diagnostic and otherwise uses
newly generated synthetic fixtures; it integrates no Claude/Anthropic path and
cannot block or satisfy the protected path.

## 2. Scope and completion truth

Ten hours after the reviewed handoff is the latest-safe B+ delivery target and
execution deadline, never a gate bypass. The protected Hero order is
PR1 Choice Core+Mac, PR2 iMessage same-account self-chat, then a same-main
Core+iMessage checkpoint App/DMG proving the complete real outcome loop plus
Off, restart, and duplicate-effect prevention. The final B+ App/DMG then adds
minimal B2 and minimal C2 in that order. These are narrow proof chapters, not
co-equal product stories and not dependencies that redefine or delay the Hero
checkpoint. The extra iMessage read-only source, Discord, broader B2/C2/channel
expansion, and product-wide presentation are post-B+. Any exact Owner action,
external outage, or normal-merge rejection that threatens the deadline is
reported immediately rather than becoming a silent extension.

The prior twenty-four-hour closure schedule is superseded. Its useful control
discipline remains: prepare the complete non-sensitive Owner-return queue,
freeze only reached stateful children, and continue every unrelated READY path.
The ten-hour target cannot waive a review, CI, ordinary-merge, privacy,
permission, effect, or release boundary and cannot guarantee an unavailable
external dependency.

Advanced visual/final-copy/animation work is post-B+; the direct
Owner-approved reviewed default Persona bundle migration and audit is PR1
scope, without independently authoring a new Persona voice or final copy. B+
polish is limited to Core, B2, and C2 plus minimal iMessage setup/status.
PR1 exposes no mutable Persona stage, activation, or rollback route; a future
revision change is a separate reviewed Owner action-time node.
External waiting for install, credentials,
permissions, provider-processing consent, real sends, real Reminder Evidence,
real Skill lifecycle actions, and other action-time effects is excluded from
autonomous completion truth and reported separately.

## 3. Authority and stale-route quarantine

Every task/resume must fresh-read, in order:

1. `/Users/jarvis/Desktop/agents-generic-phase-batch.md` in full.
2. `AGENTS.md` in the exact worktree.
3. The current normative Master Plan section.
4. `docs/OPENOPEN_PRIVATE_AGENT_CHOICE_LOOP_DESIGN.md`.
5. This execution-control document.
6. Current Git status/diff, relevant source/tests, and current remote head.

No implementation task receives a broad “implement the old Master Plan”
handoff. Every handoff includes the reviewed document hashes and an explicit
MUST-NOT list covering fixed Sol, Auto, old Outcome UX, 15-minute sessions,
groups, shared/cloud Discord Bot, arbitrary iMessage routing, offline replay,
Claude/Anthropic, and Repair23 input reuse.

If a historical document conflicts with the current Choice Loop contract, the
task records the exact conflict and ignores the historical imperative. It does
not invent a compatibility path.

The implementation task's own runtime may remain `gpt-5.6-sol` at `high` for
speed. That is an execution-task setting only; it is not the OpenOpen product's
model contract and must never reintroduce a fixed-Sol or Auto route.

The Owner locked four PR1 interfaces on 2026-07-20 UTC:

- `OWNER-20260720-CHOICE-BEGIN`: Host-owned `choice.begin` is the sole public
  first-local-question intake/create RPC. It authenticates the Mac caller,
  derives the trusted source/delivery binding, and commits the initial
  interpreting ChoiceSession plus audit in one SQLite `IMMEDIATE` transaction
  before any model call. The first ChoiceSet is accepted only by a private
  operation/generation/revision/provenance/source-manifest-bound result commit.
  Exact request replay returns the existing operation; changed replay, missing
  model, catalog/protocol drift, active unresolved session, Off, and stale or
  late results fail closed. No public raw snapshot writer or external effect is
  exposed.

- `OWNER-20260720-CHOICE-SELECT`: Host-owned `choice.select` atomically commits
  Selection, the next ChoiceSession revision, and audit evidence; stale input
  fails closed and raw whole-snapshot writes are not a production route.
- `OWNER-20260720-CHOICE-CONFIRM`: dedicated `choice.confirm` owns the immutable
  consolidated Choice payload/session/audit transaction; legacy
  `mission.confirm` cannot alias or satisfy it.
- `OWNER-20260720-BATCH-BINDING`: ConversationTurnBatch requires the durable
  Host-derived binding of its first authenticated SourceEnvelope; all later
  envelopes must match and historical missing values become typed blocked.

The Owner additionally resolved the four reached PR1 architecture packets on
2026-07-20 UTC:

- `OWNER-20260720-CHOICE-D-SELECT`: extend the command-owned `choice.select`
  request with a D variant carrying bounded untrusted text and an idempotent
  request ID. Every A/B/C/D selection atomically commits Selection, pending
  refinement operation, session revision/state, and audit in one SQLite
  `IMMEDIATE` transaction; no Selection may commit without its operation. For D,
  Host also authenticates, derives and seals the envelope/batch in that same
  transaction. Callers never supply a batch ID. Explicit complete Mac D intake
  persists encrypted body, envelope,
  sealed batch, Selection, pending refinement operation, session, request
  registry and audit in one `IMMEDIATE` transaction. Any future quiet-window
  collection persists encrypted envelopes/open batch transactionally and seals
  only in the same transaction as Selection/operation/session/audit. Exact
  replay is idempotent and changed/stale/binding-drift replay fails closed.
- `OWNER-20260720-REFINEMENT-RESULT`: accept post-selection model output only
  through a private Selection/operation/generation/session/interpretation/model/
  catalog/protocol/manifest/audit-bound result commit. Exact retry is
  idempotent. One `IMMEDIATE` transaction completes the pending operation,
  persists result digest plus encrypted frame/set, advances session state/
  revision, and appends audit; late, Off, cancel, selection or provenance drift
  fails closed. Pending raw turn text is encrypted with the existing Keychain-
  derived Store boundary and deleted after accepted typed-state render receipt
  or cancel, leaving body-free digest/audit only.
- `OWNER-20260720-MARKDOWN-RENDER`: Store first commits an exact render intent;
  Host uses descriptor-safe staging, syncs the staged file, atomically renames
  it in the same directory, syncs the parent directory, and verifies the exact
  final digest; replacement uses an atomic swap/CAS-equivalent that retains and
  validates the displaced base inode/digest, while creation is no-clobber.
  Store only then commits the render receipt. Concurrent edits, partial multi-
  file swaps, or any ambiguous restart preserve both versions in typed
  reconciliation. Markdown and receipts grant no effect authority.
- `OWNER-20260720-IDLE-STALE`: Host-owned internal deterministic transitions
  use persisted deadlines plus expected session revision/generation in Store
  transactions. Timers are hints only; exact retry is idempotent, late timers
  fail after input/Off/cancel/restart generation change, and transitions alone
  trigger no model/effect work. The API is private: schedulers provide only a
  wake hint, Host derives time/state, same-boot continuous monotonic time is
  authoritative, and reboot/backward/ambiguous clock evidence blocks rather
  than guessing.

The Owner also locked five staged execution decisions on 2026-07-20 UTC:

- `OWNER-20260720-16H-FULL-FIRE`: retained Core-first, at-most-four-lane/two-
  heavy-job, and exact-node deferred-Owner safety; its schedule and broader
  integration order are superseded by the ten-hour B+ route.
- `OWNER-20260720-B2-DYNAMIC-CARDS-CONSENT`: B2 automatic work is local and
  no-network; preview offers at most three dynamic candidate cards plus D.
  Only later exact Owner consent may send bounded source excerpts to the
  explicitly selected OpenAI model, and only selected cards may form a
  confirmed Markdown diff.
- `OWNER-20260720-IMSG-ONE-READONLY`: V1 permits at most one additional
  individually selected/revocable one-to-one iMessage read-only source and no
  outbound route; groups remain rejected.
- `OWNER-20260720-DESIGN-AFTER-FUNCTION`: advanced UI, new Persona behavior,
  final copy, density, and animation stay Owner-open until the functional
  staged path is ready; the reviewed default Persona bundle's technical
  migration is the narrow PR1 exception.
- `OWNER-20260720-24H-CLOSURE-QUEUE`: its prior schedule is superseded, while
  every Owner-needed action or still-open design decision remains prepared as a deduplicated return-
  queue item; reaching it freezes only that node and descendants. Unrelated
  READY work must continue and the queue itself never grants action authority.

The Owner then narrowed the active route with four direct decisions:

- `OWNER-20260720-REMINDER-SCHEDULE-BG`: visible/editable Reminder schedule
  values derive only from explicit user temporal information. Missing time
  requires user selection; fixed defaults and question-time inference are
  forbidden. Exact future date/time/timezone/list/count bind confirmation,
  every edit creates a new revision/reconfirmation, and real Reminder write
  remains a separate action-time gate.
- `OWNER-20260720-14H-DEMO-CORE-B2-C2` and
  `OWNER-20260720-14H-DEMO-IMSG-INCLUDE`: retain their exact cardinality,
  iMessage scope, action-time gates, and narrow UI bounds; their schedule and
  co-equal Demo narrative are superseded by B+.
- `OWNER-20260720-10H-BPLUS-HERO`: PR1+PR2 and the same-main Core+iMessage checkpoint
  are the independent Hero gate for one complete real verified outcome loop.
  Minimal B2 then minimal C2 follow as narrow proof chapters in the final B+
  package. Additional read-only iMessage, Discord, broader expansion, persona,
  full visual system, complex animation, and product-wide final copy are post-
  B+. No inclusion grants permission, selection, install, send, write, use,
  release, or merge-bypass authority.
- `OWNER-20260720-10H-BPLUS-DEADLINE`: supersedes only the prior earliest-safe
  pacing language. Ten hours is the latest-safe delivery target and execution
  deadline. It never waives or combines a gate; a threatened deadline triggers
  immediate Owner notification with the exact action or external blocker.

### Document-freeze stage

Before PR1 implementation resumes, the Primary Advisor alone:

1. replaces the Master Plan's prior normative section instead of appending a
   second live contract;
2. synchronizes README, Build Week disclosure, Acceptance Ledger,
   release-proof, validation, and provenance current sections while preserving
   historical evidence;
3. runs current-scope legacy/conflict, link, diff, status, and bounded secret
   checks;
4. obtains two fresh read-only reviews on the same document fingerprint; and
5. sends one exact writable handoff to the existing Implementation Task.

Until all five complete, new staged scope remains `WAIT_STAGED_DOC_HANDOFF`. A Product Scout
may run read-only in parallel but cannot substitute for either document
reviewer.

## 4. Lane and file ownership

### Integrator / PR1 owner

The Integrator alone changes:

- `crates/openopen-protocol/src/lib.rs`
- `crates/openopen-core/src/{mission.rs,store.rs}` and shared migrations/tests
- `crates/openopen-host/src/lib.rs`
- `crates/openopen-codex-client/src/{contracts.rs,lib.rs}`
- shared effect-broker tests
- `macos/EffectBrokerBridge/Sources/OpenOpenAppSupport/{AppModel.swift,CoreContracts.swift,CoreProcessClient.swift,OpenOpenViews.swift}`
- primary Swift tests
- root `Cargo.toml`, `Cargo.lock`, CI, primary docs, provenance, notices, and
  acceptance/evidence indices

No other lane writes those paths.

### iMessage domain owner

The domain lane changes only `crates/openopen-imsg-adapter/**`. Integrator
serially lands protocol/Store/Host/Swift wiring after the domain handoff.

### Discord domain owner — post-B+

The domain lane changes only `crates/openopen-discord-adapter/**`. Integrator
serially lands protocol/Store/Host/Swift/Keychain wiring after the domain
handoff.

### B2 Dynamic Memory owner — minimal B+ proof chapter

The lane changes only `crates/openopen-deep-zip-worker/**`. One isolated local
diagnostic may run for at most three hours. It may commit only newly generated
synthetic fixtures. The Owner-supplied real export may additionally be selected
only in place, read-only, with networking disabled and no retained extracted,
temporary, derived, catalog, count, path, hash, member metadata, or content in
the repository, worktree, evidence, or logs. Only redacted PASS/FAIL and a
bounded failure class may be reported. Mainline preview/session/card/selection
work uses synthetic fixtures; no source excerpt reaches a model without later
exact Owner consent. Claude/Anthropic adapters/fixtures/docs are excluded. B2
cannot block or satisfy the protected path. Root workspace, lockfile, Choice
types, Store, Host, Swift, provenance, and notices remain Integrator-owned.

### C2 Skills owner — minimal B+ proof chapter

The lane changes only `crates/openopen-skill-lifecycle/**`. Scope is one
instruction-only public Skill lifecycle with immutable pin/digest, audit,
promotion, update, rollback, and malicious fixtures. No marketplace, arbitrary
scripts, silent update, self-promotion, or shared Choice contract changes.

### Scout and reviewers

Product Scout and reviewers are read-only. They report findings bound to exact
tree/document hashes. Only Integrator changes evidence/docs after a freeze.

## 5. Resource controls

- Maximum four active lanes including Integrator.
- Maximum two compile-heavy jobs.
- B2 may use heavy slot 2 for its bounded diagnostic or owned-path synthetic
  verification; it is non-gating and cannot delay the protected path.
- C2 and Discord may compile/test only in their isolated owned paths before the
  Hero checkpoint; no shared integration starts early.
- iMessage self-chat integration starts only after verified normal PR1 merge.
- Minimal B2 and then minimal C2 shared integration start only after the
  verified Hero checkpoint. The extra read-only iMessage source and Discord
  remain post-B+ and cannot enter or block that order.
- Do not create a third full target directory.
- Below 25 GiB free: start no new compile-heavy or package job.
- Below 15 GiB free: stop all compilation and packaging.
- Target 45–50 GiB free before final integrated full matrices.
- Repair22/23/24 rollback candidates, final artifacts, receipts, and current
  valid proof are never deleted. Cleanup requires a retention manifest and
  separate exact authorization.

## 6. Dependency graph and schedule

```text
Repair24 source/CI receipt
        |
        v
clean c86e590 baseline + canonical Choice docs
        |
        +------> pre-freeze Product Scout (read-only)
        +------> B2 owned-path preparation + local no-retention diagnostic
        +------> C2 owned-path preparation with synthetic fixtures
        +------> Channels gap matrix / owned-path preparation
        |
        v
PR1 Host-owned choice.begin + Core + Mac + model + Markdown + Reminders
        |
        v
PR1 Scout/review/matrix/CI → IMPLEMENTATION_MERGE_READY → normal merge SHA
        |
        v
PR2 iMessage same-account self-chat private inbox only
        |
        v
PR2 Scout/review/matrix/CI → IMPLEMENTATION_MERGE_READY → normal merge SHA
        |
        v
same-main Core+iMessage Hero checkpoint App/DMG offline verification receipt
        |
        v
minimal B2: one import → ≤3 cards → one selected card → confirmed diff
        |
        v
minimal C2: one public instruction-only Skill → audit → enable → no-effect use
        |
        v
final same-main B+ App/DMG offline verification receipt;
REAL_PRODUCT_PROOF remains separate
```

| Target time | Integrator | Independent lane |
| --- | --- | --- |
| T+0–3h | Close PR1 review repairs, verification/CI, and normal merge | Prepare narrow B2/C2 fixtures and action-time packets without shared writes |
| T+3–6h | PR2 same-account self-chat implementation/gates/normal merge | Continue narrow B2/C2 owned-path verification |
| T+6–7h | Core+iMessage Hero checkpoint App/DMG offline verification | Freeze exact B2/C2 reviewed handoffs |
| T+7–8.5h | Minimal B2 integration and independent gates | Post-B+ lanes remain quarantined |
| T+8.5–10h | Minimal C2 integration, focused B+ polish, final App/DMG offline verification | Only exact action-time children wait; unrelated READY verification continues |
| After T+10h | Continue incomplete safe B+ work without weakening gates; post-B+ work starts only after B+ closure | No deadline converts WAIT_OWNER/ADMIN/EXTERNAL into authority |

When the Owner is present, the return queue is worked in dependency order
when the Owner is present. While any queue item is unavailable, Integrator
continues repair, review, CI, ordinary merge, parity, package, static audit, or
another unrelated READY item. Time pressure never permits a skipped gate,
admin bypass, fabricated proof, or autonomous product decision.

Finding repair invalidates the affected freeze. Product and security reviewers
continue independently; findings are batched once, fixed only by the owning
lane, then both reviewers rerun on the new fingerprint.

The times above are latest-safe internal milestones under the ten-hour delivery
deadline. They do not create authority to bypass or combine any gate.
Each phase still includes Stage 4–8. Final integrated verification cannot
replace any preceding phase's review, CI, or merge gates.

## 7. Stage handoffs

The writable implementation handoff contains:

```text
stage
lane
dependency_merge_sha
base_sha
head_sha
tree_sha
owner_interface_decisions
design_doc_sha256
execution_control_sha256
canonical_patch_sha256
interface_schema_sha256
owned_path_manifest_sha256
git_diff_check_receipt
focused_test_commands_and_log_sha256
product_scout_receipt_and_tree_sha
reviewer_ids_and_same_tree_sha
ci_run_and_head_sha
forbidden_effects = 0
provider_processing_consent = NOT_GRANTED_AUTONOMOUSLY
real_skill_lifecycle_authority = 0
real_channel_send_authority = 0
must_not_implement[]
stop_conditions[]
```

A field may be explicitly `PENDING_AT_THIS_STAGE` only when it belongs to a
later named stage. It may not be omitted, inherited from another SHA, or
relabeled PASS.

Repair24 handoff also binds
`(ca26036809609deb381f901b04329328aefa04cb,
c86e5903e72dd693d6e3cec6cd455ebd581116e7, 29707715009)`.

The B2 diagnostic handoff binds its isolated head/base/tree and include/exclude
manifest, but it is not a staged mainline implementation or merge gate.

## 8. PR and merge policy

The implementation sequence is:

1. PR1 — current Choice contract, Core, Mac, model selection, Markdown,
   Host-owned `choice.begin`, consolidated confirmation,
   Reminders/Evidence/Receipt.
2. PR2 — iMessage same-account self-chat private inbox only.
3. Core+iMessage Hero checkpoint — same-main local App/DMG offline verification, no install.
4. Minimal B2: exactly one real import, at most three cards, one selected card,
   and only its confirmed Markdown diff.
5. Minimal C2: exactly one public instruction-only Skill, audited and enabled,
   then one no-external-effect use.
6. Final same-main B+ App/DMG offline verification, no install.

The one additional read-only iMessage source, PR3 Discord, broader B2/C2, and
product-wide presentation follow only after B+ and are not closure gates.

Each active PR starts from the preceding merged main SHA. It must be independently
reviewable and rollbackable. Normal auto-merge is permitted only after:

- focused and required full verification passes;
- pre/post Product Scout gates applicable to that stage pass;
- two fresh same-fingerprint reviewers report P0/P1/P2=`0/0/0`;
- CI is green on the exact PR head/integration tree with content parity;
- no unresolved review thread or blocker exists;
- no admin/owner bypass is needed.

Passing those gates grants only `IMPLEMENTATION_MERGE_READY` for that exact PR
head: implementation, deterministic tests, reviewers, CI, and normal merge.
It never implies `REAL_PRODUCT_PROOF`. Real install, permission, Discord token,
provider/channel, Mission, Reminder, manual Evidence/readback, or installed-
runtime proof remains separate and action-time gated. PR2 cannot start before
PR1 normally merges; minimal B2/C2 integration cannot start before the Core
checkpoint; each later gate cannot retroactively replace an earlier one.

If a normal merge is rejected by branch protection, stop. Never use `--admin`
or rely on an implicit owner bypass.

## 9. Verification matrix

### Shared/PR1

- exactly three dynamic ChoiceOptions plus product-owned D;
- public Host-owned `choice.begin` is the sole first-local-question intake/
  create route; it validates bounded question, idempotent request ID, and exact
  selected-model/catalog/protocol references; derives the authenticated Mac
  source/delivery binding; commits initial interpreting session plus audit in
  one SQLite `IMMEDIATE` transaction before model work; exact replay returns the
  existing operation while changed replay, Off, missing selection, drift, or
  unresolved-session conflict fails closed;
- the first ChoiceSet is stored only by a private result commit bound to
  operation ID, generation, session revision, selected model/catalog/protocol
  provenance, and source manifest; stale/late output cannot commit, and no
  public raw ChoiceSession/ChoiceSet snapshot writer exists;
- Host-owned `choice.select` validates the active ChoiceSet and exact expected
  revision, then atomically persists Selection, the next ChoiceSession
  revision, and audit evidence; its D variant accepts bounded text/request ID
  while Host derives/seals the authenticated batch, so callers never mint a
  batch ID; stale/cross-session/changed-replay cases fail closed, restart
  retains the commit, and no raw snapshot write is exposed;
- D intake proves one-transaction explicit-submit persistence or recoverable
  encrypted collecting-batch persistence, crash recovery at collect/seal/
  Selection boundaries, encrypted raw-body retention/deletion, and no plaintext
  in logs/evidence/remote;
- private refinement-result commit is bound to the exact committed Selection,
  operation, generation, revisions, provenance, manifest and audit; replay is
  idempotent; one transaction commits operation/result/frame/set/session/audit;
  late/Off/cancel/drift results fail closed;
- English-only product UI and deterministic first launch: account scan →
  explicit compatible model/effort → one question → first dynamic ChoiceSet,
  with zero model work before selection;
- strict schemas, bounded fields, distinct choices, stale revision rejection;
- required persisted batch `deliveryBindingId`, first-envelope Host derivation,
  exact later-envelope match, typed blocked historical migration, quiet/hard
  clocks, attachment continuation, immediate Off/cancel/confirm, and restart
  during every batching state;
- one global session across surfaces, late-result retirement, dedupe/races;
- cross-surface dedupe only by durable shared OpenOpen correlation, otherwise
  preserve both envelopes; latest-owner-active reply only, Mac local mirror,
  no broadcast, and proactive/new-recipient/cross-channel confirmation;
- Host-owned deterministic 30-minute soft-idle and 24-hour stale-review Store
  transitions with persisted deadlines, expected revision/generation,
  private Host-derived clock evidence, sleep/reboot/backward-clock uncertainty,
  restart/idempotency and late-timer fencing; transition alone starts no model/
  effect work;
- explicit selected-model and supported-effort persistence, typed
  `not_applicable`, and model/catalog/effort provenance;
- missing/removed/incompatible/exhausted model produces typed Need you;
- no legacy fixed-Sol default or Auto path;
- Markdown render-intent/descriptor-safe staging/atomic-rename/exact receipt
  crash recovery, atomic no-clobber/swap base-CAS, concurrent Owner edit and
  partial-manifest reconciliation plus traversal, symlink, hardlink, owner/mode,
  collision, size, digest drift, conflict, secret and prompt-injection cases;
- dedicated `choice.confirm` payload/session/audit atomicity, restart, exact
  revision/digest and effect-scope drift, plus proof that legacy
  `mission.confirm` cannot satisfy Choice confirmation;
- Reminders permission denial, partial write, replay, wrong list/time/count,
  readback/Evidence mismatch, false Done, restart, and Global Off.
- Reminder schedule proposal derives only from explicit temporal input,
  validates a future instant in the selected timezone, requires user selection
  when time is absent, never uses a fixed/question-time default, binds exact
  date/time/timezone/list/count, reconfirms every edit, and cannot authorize the
  real write.

### PR2

- self-chat `is_from_me` user input versus OpenOpen echo classification;
- durable pre/post marker, duplicate echo, loop, cursor/restart and stale
  ChoiceSet behavior;
- groups rejected before body persistence/model access;
- the additional read-only source remains unavailable inside PR2 itself;
- no wake-word address requirement in the dedicated private inbox;
- Messages permission deny/cancel/revoke/regrant/restart never repeats a modal,
  never reports false On, leaves Off reachable, and allows zero provider/model/
  effect work until revalidation.

### Post-B+ iMessage read-only source

- exactly one additional individually selected/revocable one-to-one binding;
- no outbound path, recipient derivation, reactive reply mirroring, or effect
  authority from this source;
- second-source/group/ambiguous/stale/revoked routes fail before body
  persistence/model access;
- restart, revoke/regrant, cursor, dedupe, and Off preserve the exact binding.

### B2 Dynamic Memory

- singleton and contiguous split conversation layouts; immutable snapshot,
  traversal, collision, entry/type/size/memory, corruption, cancellation, and
  partial-catalog failure closure;
- automatic real-export diagnostic is in-place local/read-only/no-network/no-
  retention and reports only redacted PASS/FAIL plus bounded failure class;
- preview session exposes at most three dynamic candidate cards plus D, uses no
  fixed categories, and disposes unselected raw/derived content on cancel,
  failure, expiry, and completion;
- no provider request before exact Owner consent; selected-card revision and
  source/model/catalog provenance bind the semantic Markdown diff and its
  consolidated confirmation.
- B+ cardinality is exactly one real import and one Owner-selected card from
  no more than three candidates; only the exact confirmed diff persists.

### C2 instruction-only Skills

- canonical public GitHub identity, immutable commit/digest, bounded fetch and
  redirect/host rules;
- license, path, symlink, size, executable, permission expansion, and malicious
  fixture rejection;
- Candidate → Staged → Promoted → Runnable without skip; exact promotion nonce/
  revision/digest binding, update, and rollback;
- no instruction/script execution, live Skill selection, promotion, enable, or
  first use during autonomous verification.
- B+ cardinality is exactly one public instruction-only Skill and one no-
  external-effect use; acquisition, stage/promotion/enablement, and first use
  remain separately confirmed.

### PR3 — post-B+

- Bot token is at rest only in Keychain, transient only in authenticated local
  Gateway memory, never logged/persisted, redacted from diagnostics, and
  released/zeroized on stop where supported; remove/rotate is deterministic;
- authenticated owner plus expected Bot/application/exact-DM identity and
  install-link binding; identity/intent drift fails closed;
- unrelated events are rejected before body persistence/model access; no
  shared/cloud Bot;
- Gateway disconnect/reconnect/cursor/restart;
- offline pre-consent recap is deterministic metadata-only; discard/continue
  is revisioned and only continue admits bounded owner-bound bodies;
- token removal, permission/intent drift, restart, and Off keep provider/model/
  effect work at zero until fresh revalidation;
- no old message starts model/effect work automatically.

### Product liveness

Both Scouts inspect modal loops, unreachable controls/states, focus stealing,
repeated alerts, failure coupling, false Done/On, retry/replay, restart,
permission, and Off dead ends. Any P0/P1 blocks freeze/merge until fixed and
re-audited.

## 10. Action-time and external gates

The following are never crossed autonomously:

- installation or uninstall;
- administrator password, passkey, biometric, 2FA, or macOS security change;
- Full Disk Access, Messages Automation, EventKit/Reminders, Background
  Activity, ServiceManagement, or another permission;
- Discord token entry, first channel connection/send, proactive delivery, a new
  recipient, or cross-channel delivery; reactive replies on the already
  connected owner-active channel are covered by that binding;
- Mission confirmation or exact Reminder write/manual Evidence/readback;
- any newly different real ChatGPT ZIP selection/disclosure; the one supplied
  export is limited to the recorded local/no-network/no-retention B2 diagnostic;
- consent to send bounded history excerpts to a selected model and any real
  Memory-card selection/commit;
- real public Skill selection, promotion, update, enable, rollback, or first
  use;
- public release, destructive user-data action, or owner/admin merge bypass.

Only the affected node and its descendants enter `WAIT_OWNER`, `WAIT_ADMIN`,
or `WAIT_EXTERNAL`. Every unrelated READY lane continues. A later Owner return
fresh-revalidates the exact fingerprint and runtime/UI state before resuming.

### Owner-return queue

Queue records are non-sensitive. `first_seen_at` is recorded only when the
boundary is actually reached; before then the item stays `PREPARE` with
`first_seen_at=NOT_REACHED`. The implementation task prepares the dependency,
recovery condition, visible decision/action packet, and exact verification to
run after the Owner acts. It never records a password, token, private message,
recipient identifier, ZIP locator, private source metadata, or secret value.
Each suffixed row below is an independently stateful node with its own eventual
fingerprint, `first_seen_at`, evidence, and recovery condition. A parent number
is organizational only and can never freeze or advance a sibling. If one row
later represents repeated concrete sends or lifecycle actions, each concrete
action receives another child ID rather than sharing the template's state.

| ID | Owner-needed closure node | Autonomous state / dependency | Return action |
| --- | --- | --- | --- |
| `OWNER_RETURN-01A` | first-screen composition, card density, and D presentation | `PREPARE`; neutral first-launch semantic screen/fixtures first | Owner selects this presentation packet; run focused UI tests/review |
| `OWNER_RETURN-01B` | confirmation-card wording, visible fields, and inline-edit behavior | `PREPARE`; exact semantic confirmation fields and neutral controls first | Owner selects this packet independently |
| `OWNER_RETURN-01C` | visible CommunicationProfile dimensions and revocation presentation | `PREPARE`; inspectable/revocable profile semantics first | Owner selects visible fields/presentation independently |
| `OWNER_RETURN-01D` | new Persona behavior, humor, tone, pacing, final English copy, and visual system | `PREPARE`; the reviewed default Persona bundle's PR1 technical migration is locked, while any replacement behavior waits for Owner selection | Owner selects a replacement Persona/visual packet if desired |
| `OWNER_RETURN-01E` | progress-notification wording and frequency | `PREPARE`; notification state/frequency safety semantics first | Owner selects this packet independently |
| `OWNER_RETURN-01F` | Mission-in-progress hierarchy and new-topic presentation | `PREPARE`; functional Mission/new-topic states first | Owner selects this packet independently |
| `OWNER_RETURN-01G` | simultaneous-input visual treatment and English rule wording | `PREPARE`; latest-owner-active arbitration semantics/tests first | Owner selects visual/copy treatment only; authority stays locked |
| `OWNER_RETURN-01H` | 30-minute/24-hour return copy | `PREPARE`; deterministic idle/stale states and neutral recap fields first | Owner selects this copy packet independently |
| `OWNER_RETURN-01I` | incident/error/recovery language and technical-detail placement | `PREPARE`; typed incident/recovery states and reachable controls first | Owner selects this packet independently |
| `OWNER_RETURN-01J` | final iMessage/Discord setup presentation | `PREPARE`; both functional setup contracts and synthetic tests first | Owner selects this packet independently |
| `OWNER_RETURN-02` | final App/DMG install and administrator authentication | `PREPARE`; exact same-main offline package/receipt first | Owner performs installation/authentication; verify installed equality |
| `OWNER_RETURN-03A` | macOS Messages permission | `PREPARE`; PR2 permission states/recovery UI pass synthetic tests first | Owner grants or denies Messages access; verify deny/cancel/revoke/regrant/restart |
| `OWNER_RETURN-03B` | macOS Reminders/EventKit permission | `PREPARE`; Reminder permission states/recovery UI pass synthetic tests first | Owner grants or denies Reminders access; verify deny/cancel/revoke/regrant/restart |
| `OWNER_RETURN-04A` | real ChatGPT sign-in | `PREPARE`; authenticated account recovery UI first | Owner signs in; verify only account identity/status, with no model call |
| `OWNER_RETURN-04B` | explicit live model/effort selection | `PREPARE`; 04A plus catalog/picker/typed recovery implementation | Owner selects from the live compatible catalog; verify persisted provenance |
| `OWNER_RETURN-04C` | first real Choice Loop | `PREPARE`; 04B plus normally merged/installed Choice implementation | Owner submits one bounded question; verify the real Choice path separately |
| `OWNER_RETURN-05A` | consolidated Mission/Choice confirmation | `PREPARE`; exact immutable payload and confirmation gate first | Owner confirms or edits the exact payload; no effect is implied |
| `OWNER_RETURN-05B` | real Reminder write and readback | `PREPARE`; 03B and 05A plus exact effect permit | Owner confirms the exact Reminder write; verify readback |
| `OWNER_RETURN-05C` | manual Evidence and Receipt acceptance | `PREPARE`; 05B readback and Evidence surface first | Owner supplies/accepts the exact Evidence; verify Receipt separately |
| `OWNER_RETURN-06A1` | same-account iMessage self-chat selection | `PREPARE`; PR2 normally merged/installed and 03A complete | Owner selects the exact self-chat binding; no inbound/echo proof is implied |
| `OWNER_RETURN-06A2` | same-account iMessage real inbound/echo proof | `PREPARE`; 06A1 exact binding is current and freshly revalidated | Owner performs the bounded proof interaction; verify inbound classification, product echo, loop/dedupe, and restart separately |
| `OWNER_RETURN-06B` | exactly one additional iMessage read-only source | `PREPARE`; Core checkpoint plus read-only integration normally merged | Owner selects or revokes one eligible one-to-one source; verify inbound only |
| `OWNER_RETURN-07A` | personal Discord Bot creation | `PREPARE`; synthetic wizard/identity tests first | Owner creates the personal Bot; no token or connection is implied |
| `OWNER_RETURN-07B` | Discord token entry/validation | `PREPARE`; 07A plus Keychain/doctor implementation | Owner enters the token only in secure UI; no provider connection is implied |
| `OWNER_RETURN-07C` | first Discord provider connection | `PREPARE`; 07B plus exact Bot/application/DM binding | Owner confirms connection; no send is implied |
| `OWNER_RETURN-07D` | first Discord DM send | `PREPARE`; 07C connected binding plus exact recipient/payload | Owner confirms the first send separately |
| `OWNER_RETURN-08A` | consent to send bounded real-export excerpts to the selected model | `PREPARE`; local scanner/preview/disposal and scope UI first | Owner selects exact processing scope/model |
| `OWNER_RETURN-08B` | real Memory-card selection | `PREPARE`; 08A provider result and revision-bound cards | Owner selects/rejects cards; no Markdown write is implied |
| `OWNER_RETURN-08C` | confirmed Memory Markdown diff commit | `PREPARE`; 08B plus exact semantic diff | Owner confirms the exact diff; verify committed manifest/receipt |
| `OWNER_RETURN-09A` | real public GitHub Skill selection/acquisition | `PREPARE`; synthetic acquisition/audit first | Owner supplies/selects the public URL; verify immutable identity/digest |
| `OWNER_RETURN-09B` | real Skill stage | `PREPARE`; 09A plus audited candidate | Owner confirms exact staging identity/digest |
| `OWNER_RETURN-09C` | real Skill promotion to Runnable eligibility | `PREPARE`; 09B plus exact promotion nonce/revision/digest | Owner confirms exact promotion; no use is implied |
| `OWNER_RETURN-09D` | real Skill update or rollback | `PREPARE`; promoted existing version plus a separately audited target | Owner confirms each exact update/rollback as a distinct child action |
| `OWNER_RETURN-09E` | real Skill enablement | `PREPARE`; 09C Runnable eligibility plus exact current promoted digest | Owner confirms enablement only; no use is implied |
| `OWNER_RETURN-09F` | first real Skill use | `PREPARE`; 09E enabled exact digest plus Mission action-time gate | Owner confirms first use separately; verify no script/effect authority expansion |
| `OWNER_RETURN-10A` | final real cross-channel proof | `PREPARE`; all required real bindings and receipts first | Owner confirms each exact new-recipient/cross-channel send |
| `OWNER_RETURN-10B` | release/publish | `PREPARE`; final integrated proof and 10A are complete | Owner separately authorizes release/publish |
| `OWNER_RETURN-11` | Host-owned intake architecture for a fresh D natural-conversation turn after the initial batch retires | `RESOLVED`; first seen `2026-07-20T08:45:14Z`; approved `OWNER-20260720-CHOICE-D-SELECT` | Implement the command-owned D variant; Mac-supplied/reused batch IDs and raw snapshot writers remain forbidden |
| `OWNER_RETURN-12` | private post-selection refinement-result commit | `RESOLVED`; first seen `2026-07-20T08:47:00Z`; approved `OWNER-20260720-REFINEMENT-RESULT` | Implement exact Selection/operation/generation/revision/provenance/manifest binding and replay fences |
| `OWNER_RETURN-13` | atomic Markdown render and crash reconciliation | `RESOLVED`; first seen `2026-07-20T08:59:00Z`; approved `OWNER-20260720-MARKDOWN-RENDER` | Implement render intent → staged-file sync → atomic same-directory rename → parent-directory sync → final digest verification → exact receipt, with typed ambiguous recovery |
| `OWNER_RETURN-14` | deterministic soft-idle/stale-review transition | `RESOLVED`; first seen `2026-07-20T09:01:00Z`; approved `OWNER-20260720-IDLE-STALE` | Implement Host-owned persisted-deadline transition with expected revision/generation and no timer-created model/effect authority |
| `OWNER_RETURN-15` | Reminder schedule grounding and missing-time behavior | `RESOLVED`; first seen `2026-07-20T14:57:05.827Z`; approved `OWNER-20260720-REMINDER-SCHEDULE-BG` | Implement explicit-time-only visible/editable proposal, future/timezone validation, exact digest/revision binding, and a separate real-write gate |
| `OWNER_RETURN-16` | narrow fourteen-hour Core/B2/C2 Demo | `RESOLVED`; first seen `2026-07-20T16:51:52Z`; approved `OWNER-20260720-14H-DEMO-CORE-B2-C2` | Protect Core; bound B2 to one import/one card/one diff and C2 to one public instruction-only no-effect-use Skill; polish only three Demo screens |
| `OWNER_RETURN-17` | include iMessage in the narrow Demo | `RESOLVED`; first seen `2026-07-20T16:51:52Z`; approved `OWNER-20260720-14H-DEMO-IMSG-INCLUDE` | Include PR2 self-chat only; keep extra read-only source/Discord post-Demo and all permission/selection/install/send gates intact |
| `OWNER_RETURN-18` | ten-hour B+ Hero-first closure | `RESOLVED`; first seen `2026-07-20T20:05:39Z`; approved `OWNER-20260720-10H-BPLUS-HERO` | Treat Core+iMessage as the independent Hero gate; then land minimal B2 and minimal C2 as narrow proof chapters in the final B+ package without weakening any gate |
| `OWNER_RETURN-19` | ten-hour latest-safe B+ delivery deadline | `RESOLVED`; first seen `2026-07-20T20:30:11Z`; approved `OWNER-20260720-10H-BPLUS-DEADLINE` | Treat ten hours as the delivery deadline; notify immediately if an exact Owner action or external blocker threatens it; never bypass a gate |

An item changes to `WAIT_OWNER`, `WAIT_ADMIN`, or `WAIT_EXTERNAL` only when its
dependency is ready and its boundary is reached. The record then includes its
UTC `first_seen_at`, exact fingerprint/state, evidence, and recovery condition.
An unchanged queue item is never repeatedly reported. Queue presence is not a
reason for the implementation task or monitor to stop while safe READY work
exists elsewhere.

## 11. Current execution ledger

| Item | State | Evidence / next condition |
| --- | --- | --- |
| Repair24 source/CI identity | PASS | `ca26036… → c86e590…`, CI `29707715009` |
| Repair24 local package/DMG verification | READY | safe local verification only; preserve prior artifacts and receipts |
| Repair24 install/installed-runtime equality | WAIT_OWNER | fresh exact action-time install/runtime gate; no prior proof reuse |
| Dirty `agent/product-shell` tree | PRESERVE | never fork/pull/reset/stage |
| Verified PR1 base | READY | branch `agent/choice-loop-pr1` started from `c86e590…`; current in-progress diff is preserved |
| Choice design/control docs | IN_PROGRESS | freeze hashes, cross-doc sync, two reviewers |
| Choice PR1 interface decisions | LOCKED | prior four plus `OWNER-20260720-CHOICE-D-SELECT`, `OWNER-20260720-REFINEMENT-RESULT`, `OWNER-20260720-MARKDOWN-RENDER`, and `OWNER-20260720-IDLE-STALE`; implementation resumes only at the newly reviewed document fingerprint |
| B+ closure decisions | LOCKED | retained prior bounds plus `OWNER-20260720-10H-BPLUS-HERO` and `OWNER-20260720-10H-BPLUS-DEADLINE` |
| Ten-hour B+ route | LOCKED | PR1 → PR2 self-chat → Core+iMessage Hero checkpoint → minimal B2 → minimal C2 → final B+ package; target never bypasses gates |
| Product Scout pre-freeze | IN_PROGRESS | one model-drift cancellation P1 repaired with focused/full Swift PASS; final Scout PASS waits for the four newly approved mechanisms on one implementation fingerprint |
| B2 Dynamic Memory | BPLUS_READY_AFTER_HERO | head `1463057…`; narrow one-import/≤3-card/one-selected-card/one-confirmed-diff integration follows Hero checkpoint; every real processing/selection/diff child remains separately gated |
| C2 Skills | BPLUS_READY_AFTER_B2 | head `f4ec71c…`; narrow one-public-instruction-only-Skill audit/enable/no-effect-use follows B2; each real lifecycle child remains separately gated |
| Staged-scope handoff | WAIT_STAGED_DOC_HANDOFF | exact reviewed Master/design/control/current-index fingerprint; PR1 may continue its already-authorized slice while no new staged lane receives shared integration authority |
| PR2 | WAIT_PR1_MERGE | branch from PR1 merge SHA |
| One iMessage read-only source | POST_BPLUS | exactly one individually selected/revocable source, no outbound authority |
| PR3 Discord | POST_BPLUS | adapter-only synthetic preparation may proceed; no B+ closure dependency; real token/provider remains Owner gated |
| Final B+ App/DMG | WAIT_BPLUS_MERGES | same-main offline verification only; no install |
| Real provider/effect proof | WAIT_OWNER | exact action-time requests only |
| Owner-return closure queue | ACTIVE | `OWNER_RETURN-11` through `OWNER_RETURN-19` are resolved; every listed unresolved child node under families 01–10 remains independently PREPARE until reached, and no nonexistent parent ID may freeze or advance a child; queue cannot stop unrelated READY work |
