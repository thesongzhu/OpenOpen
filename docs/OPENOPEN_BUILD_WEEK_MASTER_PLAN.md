# OpenOpen Build Week Master Plan

Status: `IMPLEMENTATION_IN_PROGRESS`

Canonical operating rules:
`/Users/jarvis/Desktop/agents-generic-phase-batch.md`

Product completion token: `PRODUCT_READY_FOR_DEMO`

Demo recording, editing, publishing, and Devpost submission are explicitly out
of scope until that token is honestly earned.

## Vision, audience, and real problem

OpenOpen is an AI-era Outcome Distribution Network. It is not another chat UI.
It distributes one relevant, bounded AI outcome into voice, Reminders,
iMessage, and Discord, then remains responsible until the user receives an
evidence-backed Receipt.

The initial validation cohort is busy, non-technical independent workers. The
cohort is a credible wedge, not the long-term product boundary. The real
problem is not awareness of ChatGPT; it is that ordinary people do not know
what outcome to request, do not want to learn a new interface, and cannot trust
an agent that sends something and calls it finished.

The fixed loop is:

`Context → One useful suggestion → Confirmed Mission → Trackable Steps → Need you → Evidence-backed Receipt → Workflow Candidate → Approved Workflow`

Success is measured by outcomes completed, time saved, second-week/48-hour
reuse, and non-AI participant activation. Time spent in OpenOpen and addictive
engagement are not success metrics.

## Product language and surfaces

V1 is English-only. User-facing labels are:

- `I can help` — one evidence-supported Outcome suggestion.
- `Working on it` — an active bounded Mission.
- `Need you` — an approval, ambiguity, or scope boundary.
- `Done` — an Evidence-backed Receipt.
- `Make this automatic?` — a Workflow Candidate after repeat success.

The main app contains one global On/Off control, one microphone, one text input,
at most one suggestion, and at most three active outcome cards. Account,
models, connections, Skills, and privacy live in Settings. The app is a menu
bar resident and registers as a Login Item after onboarding. Off stops model
calls, listeners, triggers, and outbound actions without deleting state.

OpenOpen communicates warmly, briefly, and adaptively. It asks at most one
important question at a time, may use light humor, and never pretends to be a
human. iMessage output is prefixed `OpenOpen · AI`; Discord uses the APP
identity.

## Hero outcome A — voice to action

1. Accept at most 60 seconds of explicit push-to-talk audio.
2. Prefer on-device macOS Speech transcription; offer typed input if it fails.
3. Ask GPT for a schema-constrained Outcome and bounded steps.
4. Confirm scope once.
5. Create a Mission and mirror personal steps into an OpenOpen Reminders list.
6. Deliver a concise summary to the selected connected chat.
7. Treat Reminder completion as Evidence and issue a Receipt.
8. Offer at most one adjacent Outcome.

There is no always-listening or ambient source scan.

## Hero outcome B — collect availability and decide

1. The owner selects one iMessage conversation, one Discord channel, candidate
   dates, a deadline, and a maximum of one follow-up.
2. OpenOpen presents the exact recipients and mandate for confirmation.
3. Participant replies may update only the existing Mission; they grant no
   authority and cannot create new Missions.
4. GPT structures availability. Rust computes the intersection and at most
   three candidates.
5. OpenOpen follows up with non-responders at most once.
6. Only the Mission owner can choose the final time.
7. Only after owner approval does OpenOpen publish the decision.
8. The Receipt records participant claims, non-responders, source message IDs,
   owner approval, and delivery outcomes.

## Hero outcome C — receipt images to XLSX

1. Accept JPEG, PNG, or HEIC attachments from an approved iMessage or Discord
   context; maximum ten files and 15 MB per file.
2. Validate MIME, size, source message ID, and SHA-256. Deduplicate before model
   input.
3. Copy the normalized image to an isolated Mission workspace.
4. Ask the selected GPT for a strict schema containing vendor, date, currency,
   subtotal, tax, tip, total, category, confidence, and sourceMessageId.
5. Put low-confidence fields in one concise Need you review.
6. Generate a local XLSX containing detail, summary, formulas, and source refs.
7. Save only to an owner-approved location and return the summary and file to
   the originating chat.
8. Record the actual model, input hashes, confirmations, XLSX hash, and delivery
   result in the Receipt.
9. After the second verified similar run, propose an approved Workflow.

Raw receipt images are deleted 24 hours after Mission completion by default.
Confirmed structured fields, hashes, and Receipts remain until the user deletes
them.

## Architecture

The distribution is one Developer-ID-signed and notarized macOS 14+ Apple
Silicon DMG. The user installs no Node, Rust, Homebrew, Codex CLI, or separate
server.

- SwiftUI owns the window, menu bar, Speech, EventKit, permission UI, and global
  switch.
- A minimal Rust Core owns Mission, Workflow, Skill, SQLite, approval,
  Evidence, channels, recovery, and XLSX generation.
- SwiftUI manages Rust Core as a child process over JSON-RPC/stdio.
- Rust Core manages a pinned Codex App Server over JSON-RPC/stdio. No port is
  opened.
- Codex proposes schema-constrained actions. Rust gates and executes every
  external effect.

The stable RPC families are `account.*`, `outcome.*`, `mission.*`, `channel.*`,
`receipt.*`, `workflow.*`, and `skill.*`.

The stable domain contracts are `OutcomeSuggestion`, `Mission`, `WorkItem`,
`ApprovalRequest`, `NeedsMe`, `EvidenceRef`, `Receipt`, `ChannelEnvelope`,
`WorkflowCandidate`, `WorkflowDefinition`, `SkillPackage`, and
`SkillPermissionManifest`.

Mission lifecycle:

`proposed → awaitingConfirmation → active ↔ needsMe/paused → completed | failed | cancelled`

A new recipient, broader scope, new data disclosure, new external write, cost,
deletion, payment, or irreversible action always returns to Need you. A sent
message is never completion evidence by itself.

### Command-owned Mission persistence

The Rust Core has one authoritative state machine. Production persistence must
not accept an arbitrary caller-assembled `Mission` snapshot, even when a second
validator appears to approve it. That creates two imperfect transition
implementations and allows states that no domain command can legally reach.

The approved design is:

- A typed `CreateMission` command constructs the only legal genesis state,
  including the exact pending owner scope confirmation. Callers cannot inject
  pre-decided approvals, WorkItems, Evidence, Receipts, or terminal state.
- Every later mutation is a typed domain command: confirmation decision, scope
  change request, approval decision, Mission pause/resume/fail/cancel, WorkItem
  transition, Evidence attachment, or Evidence-backed completion/Receipt.
- The Store loads the current encrypted state inside its transaction, applies
  the command through the domain state machine, validates the resulting
  snapshot defensively, and writes state plus the bound audit record in that
  same transaction.
- The production Store exposes no raw `commit_mission(&Mission)` or equivalent
  snapshot-replacement route. Snapshot validation remains defense in depth and
  import/recovery checking; it is not mutation authority.
- Optimistic concurrency binds every command to the current audit anchor.
  Duplicate command IDs are idempotent only when the original command and
  result match exactly; conflicting reuse fails closed.
- Tests must prove that every persisted transition is command-reachable, every
  legal command is persistable, illegal command/state pairs fail without a
  state or audit write, and state plus audit remain atomic across rollback and
  reopen.

### Host-derived Mission workspaces

The host owns one trusted Missions root. A Mission ID is validated as one
normal path component, and the host derives the exact workspace as
`<trusted-root>/<mission-id>`. A model, channel payload, RPC caller, imported
snapshot, or Mission object cannot choose or redefine that root.

Before canonicalization or use, the host rejects a missing root, a Mission root
that is a symlink or other alias, separators/dot segments in the Mission ID,
and any canonical target outside the derived workspace. File effects must use
no-follow or equivalently race-safe opens and revalidate containment at the
effect boundary. Regression coverage includes a sibling alias where
`mission-1` is a symlink to `mission-2`, dangling and nested symlinks, traversal,
and workspace replacement between proposal and execution.

### Protected effect broker

The Mission workspace boundary is owned by an independent macOS security
principal, not by the user-session Rust Core. A same-UID process can rename,
re-permission, hard-link, or relocate another same-UID process's files, so no
amount of end-of-write path revalidation inside Core is accepted as proof that
an output remained at its exact Mission path through success return.

The approved design is:

- A signed `SMAppService` LaunchDaemon runs the effect broker under a distinct
  security principal and exclusively owns
  `/Library/Application Support/com.thesongzhu.OpenOpen/Users/<audit-euid>/Missions`.
  The user namespace is derived from authenticated XPC peer credentials, never
  from a request field. The Core and other
  user-session processes have no direct read, create, rename, link, chmod,
  delete, or write capability in that tree.
- Core has no production Mission-root path configuration or raw filesystem
  executor. After the Store verifies the current Mission and complete audit
  chain, Core may issue only a typed, signed, short-lived effect permit.
- Every permit binds one effect ID, the exact canonical Mission ID and target,
  payload SHA-256 and byte length, approved scope and action digest, current
  Store audit anchor, `execute`, `reattestOnly`, or `reconcile` purpose, broker
  session nonce, issue time, and expiry. Changed, stale, cross-session, or
  caller-assembled state fails closed. A new authorization creates one global
  unresolved-effect fence in the same Store transaction as its signed audit
  row. No later Mission audit can advance until the Store atomically records
  either the broker Receipt or a signed definitive noncommit and clears that
  fence. `reattestOnly` is strictly read-only; only `reconcile` may classify a
  crash state, finish proof of an already-renamed inode, or scrub a pre-rename
  stage and persist a permanent noncommit tombstone.
- Core loads a durable `TrustedBrokerEnrollment` created only by the signed
  installation/enrollment flow. The session key and key ID must exactly match
  that pinned enrollment before permit issue or Receipt verification; Core
  never learns trust from a self-consistent live session or TOFU.
- The Swift host connects through the privileged Mach bootstrap. Both XPC
  endpoints pin the exact peer signing identifier and the daemon's own signing
  team before activating the connection; there is no user-session socket,
  shell, raw-path API, or stdio fallback to the privileged executor. The
  daemon's private Rust worker is copied only after signature verification into
  a root-owned `0700` directory, reverified there, and invoked over inherited
  pipes that are not exposed as a caller transport.
- The broker revalidates the typed command and payload, performs descriptor-
  relative no-follow writes inside its protected tree, and keeps a protected
  exact-idempotency journal. A protected `flock` serializes independent worker
  processes across the complete journal/payload/filesystem operation. The
  journal persists the unique staged device/inode before rename, a pre-rename
  intent separately from the completion proof, and a terminal noncommit
  tombstone. Completion time is recorded only after rename, final path/hash/
  inode validation, file `fsync`, directory `fsync`, and stage-directory
  cleanup. Reusing an effect ID with changed bytes conflicts; every committed
  retry drains and hashes the supplied payload, and an old `execute` permit is
  permanently rejected after noncommit.
- Success returns a broker-signed effect Receipt binding the permit hash,
  Mission, output hash and size, exact committed target, broker session, and
  completion time plus attestation time. Core verifies the pinned broker key,
  exact permit/result binding, and time ordering, then persists the encrypted
  Receipt and a bound audit row before it can be used as Evidence. Receipt or
  noncommit persistence, its audit row, and fence deletion share one Store
  transaction; rollback leaves the fence active. Store verification requires
  every authorization to have exactly one resolution state: unresolved fence,
  committed Receipt, or signed noncommit. Deletion, overlap, orphan rows, and
  ciphertext/signature/audit mismatch all fail closed.
- LaunchDaemon registration and execution require explicit macOS administrator
  approval. Missing approval, code signing, peer identity, protected-root
  ownership, broker session, or Receipt verification produces Need you or a
  fail-closed error; it never activates a same-UID writer.

Same-UID unit tests and unsigned local processes may prove protocol and
filesystem logic only. Boundary closure additionally requires a signed,
installed helper running as the distinct principal and adversarial proof that
the ordinary app user cannot mutate the protected tree or impersonate either
XPC peer. Those later results must not be fabricated from mocks or plist
inspection.

## Codex and model contract

- Pin the Codex runtime and generate protocol schemas from that exact binary.
- Use only stable methods and do not opt into experimental API.
- Store Codex state under an app-specific CODEX_HOME with Keychain credentials.
- Use managed `account/login/start`, `account/read`, `model/list`, and stable
  thread/turn/events.
- Never read, copy, or parse `~/.codex/auth.json`; OpenOpen never receives OAuth
  tokens.
- Accept any ChatGPT plan and present only GPT models returned for the account.
- Use GPT-5.6 Sol for competition and release proof.
- Use `gpt-5.6-sol` with `high` reasoning for the Codex implementation goal and
  its isolated reviewers. The repository pins these defaults in
  `.codex/config.toml`; the background task also passes them explicitly.
- Never silently switch models. Unavailable access or quota creates Need you.
- Every Receipt records the actual model.

For untrusted receipt, chat, and Skill inputs, use a strict output schema,
isolated Mission cwd, no model-controlled network, no automatic approval, and
no external writes. The host refuses any filesystem request outside the
Mission workspace. Tool requests, schema failure, scope drift, prompt
injection, or canary access fail closed.

## Channels

### iMessage

Bundle imsg v0.13.0 basic mode and its required resources. Exclude the private
bridge helper; never ask the user to disable SIP. Guide Full Disk Access and
Messages Automation. The owner explicitly selects allowed conversations.
Filter all other messages before model access. Persist Apple GUID/rowid cursors
for bounded recovery and dedupe.

### Discord

Use serenity 0.12.5 and an official Bot Gateway. Never automate a normal user
token. The local three-step wizard asks the user to create/enable the bot,
paste the token once, and approve the official install page. Store the token
only in Keychain. Infer IDs, build least-privilege install links, pair the
owner, probe permissions/intents/attachments, and prove real bidirectional
traffic. By default only paired users, approved channels, and explicit
mentions can start work.

## Workflows and Skills

Two similar verified successes create a Workflow Candidate. Approval stores a
recipe; every invocation creates a new bounded Mission. There is no infinite
Mission and no silent automation expansion.

First-party signed Skills are voice-actions, group-availability, and
receipt-xlsx. Discovery consists of a small curated list plus public GitHub URL
import. Packages are pinned to owner/repo/path/commit/digest and pass license,
path, symlink, size, executable, permission, and test checks.

The lifecycle is `Candidate → Staged → Promoted → Runnable`, with no skip.
Runnable is eligibility, not authority; first execution still requires a
Mission approval. Updates show commit, diff, permission changes, tests, and a
rollback pointer and require explicit promotion. There is no search market,
rating, silent update, or self-modification.

## Local data and privacy

OpenOpen has no cloud API and no central telemetry. ChatGPT, Discord, and
GitHub connections originate locally. Secrets and encryption roots live in
Keychain. Sensitive bodies use encrypted blobs; logs contain redacted
metadata. The app provides Export My Data and Delete All Data.

Sleep, offline state, or runtime/channel crash persists Paused state and never
fabricates completion. Recovery uses bounded exponential backoff and durable
dedupe.

## Explicit exclusions

No Telegram, mobile app, OpenOpen cloud, shared Discord bot, ambient
surveillance, always-listening microphone, complete marketplace, private
iMessage API, SIP change, payment, booking, purchase, silent model fallback,
silent Skill update, or demo production.

## Friday provenance

Source of truth: Friday origin/main commit
`4870f31fa088bef7eb9f4f256ec62993b02eda80`.

Only the Mission/WorkItem/Receipt state machine, evidence/Needs Me workflow
invariants, Skill/SkillCatalog/Trust/PathSafe concepts, and required
SQLite/audit/encrypted-blob gates may be adapted. Friday Hub, retired
TypeScript, providers, mobile apps, and old UI are excluded. BUILD_WEEK.md,
PROVENANCE.md, and THIRD_PARTY_NOTICES.md must stay current.

## Execution phases

### Automated execution contract

This document is the sole product, architecture, execution, and acceptance
source for the implementation goal. `/Users/jarvis/Desktop/agents-generic-phase-batch.md`
is the mandatory operating-rules source. At startup and after every resume or
context compaction, the executing task reads both files in full, reads
`AGENTS.md`, inspects current Git state and relevant source/tests, and then
continues from the first unclosed ledger item.

The execution task runs on `gpt-5.6-sol` with `high` reasoning and creates a
goal whose objective is to implement this master plan through the honest
`PRODUCT_READY_FOR_DEMO` gate. Stage 0→8 and subsequent phases auto-advance
under the generic rules. It stops only at a real Ask-Before-Act boundary,
external authority/credential boundary, the three-attempt same-root supervisor
gate, or the final product gate. It must never use owner/admin bypass, silently
change models, duplicate the execution in a second task, weaken proof, or turn
mock results into release claims.

The immediate resume point is two fresh isolated reviews of one frozen thirteenth
product-shell repair. The repair separates
authoritative protected state, user intent, model-entry permission, and
transition/unknown presentation. User-requested Off immediately blocks new
model entry and advances the operation generation, then cancels Core work
before any fallible broker-trust dependency. A known-On runtime may display Off
only after protected broker proof; a fresh Core with no protected history may
report its explicit default-Off state. Broker response loss, dashboard failure,
and Core/broker mismatch show Unknown until fresh proof, never a fabricated
Off. Core also retains a revision-bound pending-Off latch under the same lock
as active operation authority: canceled login/model work cannot be resurrected
by replaying an older protected On state, and only a sufficiently new protected
On revision can reopen work. App model authorization requires exact protected
enabled, revision, and timestamp convergence. Before the App loads or writes
the Keychain Core master, it requires an exact static Core signing identifier/
App-Team match and then revalidates the running Core's exact Mach audit-token
incarnation against that same requirement. The exact persistent
Codex child still starts uninitialized, receives its full durable Mach
audit-token lease before initialization or any model/account request, and Off
does not spawn or reacquire it. App/Core retain no numeric process-signal
authority, and each root worker token remains bound to a stable exact identity
before any request bytes. Only after both
reviewers pass may the task commit and push `agent/product-shell`, open or
update its draft PR, and require current product-shell GitHub CI. The task then
continues the listed phases automatically. If the requested model is
unavailable, it records a blocker and asks for direction; it does not fall
back. Final demo production remains excluded.

1. Repository, governance, master plan, provenance, Rust workspace, original
   state-machine tests.
2. SwiftUI/menu bar, Rust Core stdio, persistence, global switch, login item,
   Codex auth/model structured turn.
3. Voice/Reminders and receipt/XLSX vertical slices.
4. imsg and Discord real adapters plus availability Mission.
5. Workflow learning and curated/GitHub Skill lifecycle.
6. Security, stress, clean install, real-provider proof, external users,
   signing, notarization, and PRODUCT_READY_FOR_DEMO gate.

Each meaningful phase requires focused verification and two isolated read-only
reviewers. A same-root failure gets at most three consecutive implementation
attempts; a supervisor then decides whether work is stuck. Tests, acceptance,
or proof may never be weakened to escape the loop.

## Acceptance matrix

Automated coverage includes every legal/illegal Mission transition, Evidence
completion gate, expanded-scope approval, second-follow-up denial, app-server
contracts, untrusted-input containment, channel authorization/dedupe/recovery,
Reminders lifecycle, receipt confidence/dedup/XLSX formulas, Workflow repeat
gate, Skill path/symlink/update/rollback, global Off, sleep/offline/crash,
100 shuffled duplicate envelopes, ten concurrent Missions, ten receipts,
secret scan, lint, diff check, code signing, notarization, and Gatekeeper.

Release proof must come from the same SHA and signed build and contain nonzero,
all-passing, blocker-free scenarios for GPT-5.6 Sol, real iMessage and Discord
traffic, real Reminders completion, a real image-to-XLSX result, and restart
recovery without duplicate delivery. Mocks and CI are never substituted for
this proof.

External validation requires one clean install and three unguided target users.
All three complete a first Outcome; at least two return within 48 hours. Failed
validation is reported and fixed, never rewritten.

`PRODUCT_READY_FOR_DEMO` requires all automated and real E2E tests, both
reviewers, current-SHA proof, user metrics, signed/notarized clean install,
complete docs/notices/sample data, a clean worktree, and no hidden fallback,
mock-only route, secret, or unfinished claimed route.

## Implementation ledger

| Date | Decision or result | Evidence | Status |
| --- | --- | --- | --- |
| 2026-07-14 | Canonical plan created; implementation begins from an empty OpenOpen repository | Initial repository diff | in progress |
| 2026-07-14 | GitHub identity blocker resolved: `gh auth status` and `gh api user` both report only active owner `thesongzhu`; local author is `thesongzhu` with account-ID noreply email; no remote exists | local CLI/API/config inspection | identity ready; public repo creation/push remains gated on reviewer and Stage 6 |
| 2026-07-14 | Friday working directory drifted to detached `25329515…`; all OpenOpen provenance reads remain pinned to `4870f31…` through `git show` | local Git inspection | isolated; no source drift imported |
| 2026-07-14 | Foundation reviewers rejected the first Rust pass for approval, exact-action, Evidence, audit-anchor, recovery, and disclosure gaps | two isolated reviewer reports | repair and reviewer rerun in progress |
| 2026-07-14 | Friday `origin/main` advanced to `0871c37…`; the fixed Build Week source remains ancestor `4870f31…` and all reads use the immutable pin | local Git ancestry check | upstream drift isolated |
| 2026-07-14 | Foundation reviewer rerun found deeper Evidence scope, duplicate approval, workspace containment, atomic audit, WorkItem, recovery, JSON-RPC, and disclosure gaps | two isolated reviewer reports | repaired; 21 local tests/check/strict Clippy pass; fresh reviewer rerun pending |
| 2026-07-14 | Next reviewer rerun reproduced persistence invariant bypass, WorkItem approval replay, dangling-symlink escape, unbound/middle-deletable audit, optional outbound disclosure, parent-state drift, and invalid-request misclassification | two isolated reviewer reports and isolated adversarial reproductions | repaired and superseded by the next verification cycle |
| 2026-07-14 | Latest isolated reviewers reproduced pre-approved Approval injection, audited state-row deletion, mutable Receipt IDs, post-approval WorkItem injection, and primitive JSON-RPC params | two isolated reviewer reports; 36 local tests plus check/build/strict Clippy and host stdio probes pass after repair | local verification PASS; fresh reviewer rerun pending; not current-SHA or release proof |
| 2026-07-14 | Follow-up reviewers reproduced direct persisted decisions outside confirmation, unapproved Mission/WorkItem resume, free-standing genesis approvals, and a Mission-declared workspace trust root | two isolated reviewer reports; 39 local tests plus check/build/strict Clippy, host stdio probes, per-file whitespace, secret, and cleanup checks pass after repair | local verification PASS; third reviewer rerun pending; not current-SHA or release proof |
| 2026-07-14 | Third reviewer cycle reproduced a command/API versus persistence split: the Store still accepted caller-built snapshots and its duplicate transition validator admitted a new `MissionScope` decision that the domain API forbids | isolated reviewer reproduction and three-attempt supervisor verdict `STUCK: same_root_cause` | unresolved foundation blocker; prior 39-test green result is not closure |
| 2026-07-14 | Owner approved the recommended single command-owned state machine and the host-derived, non-alias Mission workspace repair; execution is authorized to resume automatically | user instruction plus the architecture contracts above | approved implementation direction |
| 2026-07-14 | Goal execution is pinned to `gpt-5.6-sol` with `high` reasoning and must auto-advance under the generic Stage 0→8 rules | `.codex/config.toml`; background task `019f6033-7913-7900-94d0-f3938acaddc2` confirmed the exact goal active | active; foundation entrypoint audit in progress |
| 2026-07-14 | First fresh review of the approved command-owned persistence and host-derived workspace repair reproduced invalid Mission path IDs, ordinary workspace replacement, and an unsigned command-result hash | two isolated reviewer reports; repaired tree passed 43 local tests plus fmt/check/build/strict Clippy and host stdio probes | superseded by the next fresh reviewer cycle; not release proof |
| 2026-07-14 | Second fresh review reproduced NUL Mission ID persistence, case-insensitive Mission workspace aliasing, and an unbound command-result Mission ownership column | two isolated reviewer reports; repaired tree now passes 45 local tests plus fmt/check/build/strict Clippy, host stdio, secret, and cleanup checks | local verification PASS; two fresh reviewers pending; no commit SHA or release proof |
| 2026-07-14 | Third fresh review reproduced jointly mutable command-result ciphertext/hash, normalized path-like Mission IDs, fresh-Gate case aliases, hard-link truncation outside a Mission workspace, and future Evidence accepted before observation | two isolated reviewer reports | foundation FAIL; superseded by the owner-approved full invariant repair |
| 2026-07-14 | Three consecutive reviewer cycles remained on the same command/result, path-identity, and causal-proof invariant root | isolated supervisor verdict `STUCK: same_root_cause`; owner approved “方案 1：完整不变量重构（推荐）” | full invariant refactor authorized; no acceptance or release gate weakened |
| 2026-07-14 | Full invariant repair now uses one canonical lowercase ASCII Mission ID, exact descriptor-derived workspace names, hard-link-safe atomic replacement, signed and globally decrypted command-result rows, and causal Evidence time checks | 49 local tests plus fmt/check/build/strict Clippy, host stdio probes, forbidden-route, credential-pattern, cleanup, and touched-file whitespace checks | local verification PASS; two fresh isolated reviewers pending; no commit SHA or release proof |
| 2026-07-14 | Two fresh reviewers then reproduced a concurrent workspace-relocation escape and an externally hard-linkable streaming temporary inode | two isolated reviewer reports and adversarial long-write reproductions | foundation remained FAIL; repaired without weakening the workspace boundary |
| 2026-07-14 | Mission writes now stream into a non-enumerable private staging directory, re-open and revalidate the exact destination at the effect boundary, and remove staged or relocated output on failure | 51 local tests plus fmt/check/build/strict Clippy, host stdio probes, forbidden-route, credential-pattern, cleanup, and all-file whitespace checks | local verification PASS; two new isolated reviewers pending; no commit SHA or release proof |
| 2026-07-14 | The next two reviewers reproduced a pre-permission-change staging-directory FD race and same-UID permission restoration; one also reproduced failure of an unchanged command-envelope retry | two isolated reviewer reports and external adversarial harnesses | foundation remained FAIL; both blockers repaired in the approved invariant fix-loop |
| 2026-07-14 | Staging directories now start search-only via macOS `O_SEARCH`, payload inodes remain mode `0000` while streaming, link/permission changes are detected and scrubbed, and exact duplicate commands are resolved before a new-command anchor check | 52 local tests plus fmt/check/build/strict Clippy, host stdio probes, forbidden-route, credential-pattern, cleanup, and all-file whitespace checks | local verification PASS; two fresh isolated reviewers pending; no commit SHA or release proof |
| 2026-07-14 | Two fresh reviewers reproduced post-rename relocation: one moved the final file outside and one moved the whole Mission workspace after the last destination check; both writes returned success with the output outside the trusted root | two isolated reviewer reports and adversarial long-write reproductions | foundation FAIL; the prior 52-test green result is superseded |
| 2026-07-14 | Three workspace-write repair cycles shared the same missing linearizable effect-boundary invariant | isolated supervisor verdict `STUCK: same_root_cause` | same-UID Core-owned filesystem design rejected |
| 2026-07-14 | Owner approved “新的方案 1：独立 effect broker 安全边界” and authorized recommended implementation choices that preserve the independent-principal, broker-exclusive, typed-command, fail-closed direction | user instructions | protected effect-broker refactor authorized; acceptance and release-proof gates unchanged |
| 2026-07-14 | Protected broker implementation now includes pinned non-TOFU Core/broker trust, audited encrypted effect Receipts, a root-only persistent worker, exact production root derivation from XPC EUID, descriptor-relative journaled writes, cross-session recovery attestations, mutual XPC code requirements, a real LaunchDaemon executable/backend, and explicit bundle layout | 74 Rust tests and 21 Swift tests; workspace fmt/test/release build/strict Clippy and Swift warnings-as-errors debug/release build pass | focused local implementation verification PASS; two fresh combined reviewers pending; signed/admin/cross-UID proof still absent |
| 2026-07-14 | First combined protected-broker reviewers reproduced caller-selectable/unwired enrollment, unbounded worker/payload blocking, deletable effect authorization, crash-window commit-time drift, non-idempotent cross-session attestation, and Rust/Swift schema mismatch | two isolated FAIL reports and external adversarial reproductions | repaired; supersedes the prior local green result |
| 2026-07-14 | Protected-broker repair now uses Keychain-backed admin-enabled signed provisioning, a Core-signed install record, an audited authorization anchor in every permit, bounded worker/payload deadlines with forced reap, pre-rename durable commit intent, durable per-session attestation, verified immutable reattestation handling, and one protocol bounds contract | 77 Rust tests and 23 Swift tests; Rust fmt/test/release/strict Clippy plus Swift warnings-as-errors debug/release, strict format, and plist lint pass | focused local verification PASS; two new isolated reviewers pending; signed/admin/cross-UID proof still absent |
| 2026-07-14 | Second combined protected-broker reviewers reproduced rename after permit expiry, an unrecoverable broker commit after unrelated audit advancement, Receipt reuse across distinct permits, committed retries that did not consume or validate payload, and duplicate approval IDs accepted only by Swift | two fresh isolated FAIL reports and adversarial reproductions | repaired; the prior 77-test green result is superseded and was not a foundation pass |
| 2026-07-14 | Protected-broker repair now rechecks expiry at the exact commit callback, binds every Receipt to the hash of the complete signed permit, consumes and hashes every retry payload, appends verified already-committed results from the current audit tail without authorizing another write, and rejects duplicate approval IDs in both DTO layers | 82 Rust tests and 23 Swift tests; Rust fmt/workspace tests/release/strict Clippy, Swift warnings-as-errors tests/debug/release, strict format, plist lint, host stdio probes, credential scan, and all-file whitespace scan pass locally | focused local verification PASS; two new isolated reviewers pending; no remote, current-SHA, signed/admin, cross-UID, or release proof |
| 2026-07-14 | Third combined protected-broker reviewers reproduced recovery acceptance for a commit after an intervening audit, permanent loss when rename succeeded but the response was lost before permit/session expiry, and retry attestation using the pre-stream entry time | two fresh isolated FAIL reports and deterministic sequence analysis | repaired; the prior 82-test local green result is superseded and was not a foundation pass |
| 2026-07-14 | Recovery authority is now explicit: signed permits distinguish `execute` from `reattestOnly`; audit advancement can produce only the latter; the broker requires an existing matching journal/workspace/output and never creates under recovery authority; signed Store-observed audit time proves commit-before-audit; completion time is reread and revalidated after payload plus output verification | 87 Rust tests and 23 Swift tests; Rust fmt/workspace tests/release/strict Clippy plus Swift warnings-as-errors tests/debug/release, strict format, and plist lint pass locally | focused local verification PASS; two new isolated reviewers pending; no remote, current-SHA, signed/admin, cross-UID, GitHub CI, or release proof |
| 2026-07-14 | The next two combined reviewers reproduced the remaining non-linearizable root: commit time was still pre-rename intent time, `reattestOnly` could clean or rewrite state, and a live old `execute` permit could write after pause/cancel before Store rejected the late Receipt | two fresh isolated FAIL reports; third consecutive same-root cycle; supervisor verdict `STUCK: same_root_cause` | prior 87/23 local green result superseded; full effect fence/reconciliation redesign required |
| 2026-07-14 | Owner had already approved every recommended change preserving the full-invariant and independent-broker direction, so the supervisor design proceeded without an owner/admin bypass | owner instructions plus supervisor minimum-complete design | global fence, explicit reconciliation, cross-process worker serialization, inode-owned recovery, terminal noncommit, and read-only reattestation authorized |
| 2026-07-14 | Linearizable effect repair now atomically binds authorization/fence/audit, blocks every later Mission audit behind the global fence, clears only with an atomic Receipt or signed noncommit outcome, serializes independent workers with a protected file lock, records completion only after durable post-rename validation, recovers only the persisted staged inode, permanently rejects Execute after noncommit, and wires typed reconciliation through Rust worker and Swift XPC | 94 Rust tests and 25 Swift tests pass locally; Rust release/fmt/strict Clippy and Swift warnings-as-errors test/release/strict-format/plist checks pass; focused tests cover concurrent streaming versus pause, two workers, post-rename recovery, wrong-inode same-hash rejection, cleanup-before-tombstone crash recovery, noncommit tombstone, read-only reattestation, and atomic fence rollback/tamper | focused local implementation verification PASS; two fresh isolated reviewers pending; no remote, commit SHA, GitHub CI, signed/admin, cross-UID, notarization, or release proof |
| 2026-07-14 | The first fresh reviewer reproduced a permanent fence deadlock when the initial signed noncommit response was lost and Core later issued a different reconciliation permit/session | isolated Reviewer B FAIL with deterministic restart sequence; Reviewer A was interrupted after the reviewed tree became obsolete | repaired; the prior 94-test local green result is superseded and was not foundation closure |
| 2026-07-14 | A noncommit tombstone now keeps its terminal classification while issuing and durably caching a fresh signed attestation for each new valid matching reconciliation permit/session; Execute authority is never reopened | 95 Rust tests and 25 Swift tests pass locally; the new persistent Store↔broker test loses the first response, restarts both sides, rotates the broker session, clears the fence with the fresh attestation, advances the Mission, and still rejects the old Execute; Rust release/fmt/strict Clippy pass | focused local implementation verification PASS; two new isolated reviewers pending; no remote, commit SHA, GitHub CI, signed/admin, cross-UID, notarization, or release proof |
| 2026-07-14 | Two fresh isolated reviewers accepted the repaired global effect fence/reconciliation foundation | Reviewer C PASS after adversarial Store/broker crash, inode, ordering, and lost-response review; Reviewer D PASS after independent persistence, cryptographic binding, Rust↔Swift parity, tamper, and no-overclaim review; both reran tests in disposable build directories | foundation Stage 5 PASS; Stage 6 commit/push pending; this is not signed/admin/cross-UID/current-SHA/release proof |
| 2026-07-14 | The reviewed bootstrap was committed as `19ecdd9c290dd685f1e79ff525c71b8d38504db8`, public repository `thesongzhu/OpenOpen` was created, and `main` was pushed with exact local/remote parity | local Git status/log, `git ls-remote`, and GitHub repository API | Stage 6 bootstrap PASS; the same SHA passes 95 Rust and 25 Swift tests locally; GitHub CI was absent at bootstrap and is being added through `agent/foundation-ci` |
| 2026-07-14 | Repository CI uses a read-only-permission `macos-26` job, a commit-pinned official checkout action, Rust 1.96.0, Xcode/Swift runner provenance, the full strict Rust/Swift test/build/format/plist suite, a 30-minute timeout, concurrency cancellation, and a final tracked-diff check | workflow diff plus official GitHub runner-image and checkout release records | local verification and two isolated workflow reviewers pending; no GitHub run is claimed yet |
| 2026-07-14 | First CI review: technical Reviewer A PASS; governance Reviewer B FAIL because one blocker still said commit SHA was absent and README used present tense before any workflow run | two isolated reviewer reports | evidence wording corrected without changing workflow or proof gates; two fresh reviewers pending |
| 2026-07-14 | Second CI review: Reviewer C PASS; Reviewer D FAIL because `BUILD_WEEK.md` and `PROVENANCE.md` still described the foundation as pre-commit/reviewer-pending | two isolated reviewer reports plus full Markdown current-state scan | all current provenance/disclosure surfaces synchronized; historical ledger rows remain chronological; third fresh reviewer cycle pending |
| 2026-07-14 | Third CI review: Reviewer E PASS; Reviewer F PASS | two fresh isolated reports covering workflow security, live remote facts, disposable 95/25 reruns, and repository-wide evidence-state consistency | CI Stage 5 PASS; branch commit/push, draft PR, and first inspected Actions run pending |
| 2026-07-14 | CI branch commit `b61766b5f6cb5f208583633cc0d8244b8cfd2ea8` was pushed, draft PR #1 was opened, and pull-request Actions run `29369643001` passed every workflow step | GitHub PR/run API and logs show checkout of synthesized merge `e0fed49af5ff7f65f579f6f94f509d1f7e253ff8`, whose tree equals PR head `b61766b…`; 95 Rust and 25 Swift tests, release builds, strict lint/format, plist lint, and clean tracked diff pass on macOS 26.4 / Xcode 26.5 / Swift 6.3.2 / Rust 1.96.0 | CI Stage 7 PR integration-tree plumbing PASS, not exact-head proof; the PR remains draft and the evidence-only follow-up tree must pass its own check; no signed/admin, cross-UID, notarization, product-E2E, or release proof is implied |
| 2026-07-14 | First post-run evidence review: factual Reviewer A PASS; governance Reviewer B FAIL because the initial wording treated a `pull_request` run as direct head-SHA execution | two isolated reports plus checkout-log and commit-tree verification | wording now records synthesized merge `e0fed49…`, equal tree with head `b61766b…`, and the exact-head/release-proof boundary; two fresh reviewers pending |
| 2026-07-14 | Second post-run evidence review: Reviewer C PASS; Reviewer D PASS | two fresh isolated reports independently verifying PR/base/head state, merge-ref checkout, tree parity, 95/25 logs, toolchain versions, reviewer history, and no-overclaim boundaries | evidence-update Stage 5 PASS; exact-file commit/push and the follow-up PR integration check remain pending |
| 2026-07-14 | Evidence follow-up commit `923c88abb6099267d1e636544de4d6bb4814c5e0` was pushed to draft PR #1 and Actions run `29370433505` passed every step | live GitHub PR/run/ref inspection; synthesized merge `d502b3d…` and head `923c88a…` share tree `ecc50fa…` | CI evidence follow-up PASS at PR integration-tree tier; PR stays draft and unmerged; not product/release proof |
| 2026-07-14 | GitHub identity is verified as sole active CLI/API account `thesongzhu`; local author is `thesongzhu` with account-ID noreply mail; public remote already exists and has exact refs above | `gh auth status`, `gh api user`, local Git config, GitHub refs | prior `mxclip` authentication blocker is closed; no owner/admin bypass was used |
| 2026-07-14 | Product-shell working tree adds Store-owned signed default-Off control, cancellable host operations, pinned Codex `0.144.0` schemas/runtime hashes, outer macOS sandbox plus short-lived model-input workspaces, managed ChatGPT-only routes, SwiftUI window/menu bar/Settings, Keychain bootstrap, Login Item registration, and explicit ad-hoc staging | 112 Rust tests, 28 Swift tests, strict Rust/Swift builds/lint/format, 267-schema manifest verification, exact-runtime sandboxed initialize/account-read diagnostic, and one hash-verified ad-hoc staged app | focused local implementation PASS; two fresh isolated reviewers, commit/push, product-shell Actions, real login/model outcome, signed/notarized package, and product E2E remain pending; no release claim |
| 2026-07-14 | First product-shell reviewers both FAIL: they reproduced post-Off use of an old Execute permit, signed runtime-row rollback, unbounded/uncancellable local RPC paths, stale Core-process callbacks, switch/login-item UI divergence, missing broker bundle artifacts, floating response IDs, window restoration failure, and staging TOCTOU | two fresh isolated reviewer reports plus deterministic reproductions | prior 112/28 green result superseded; product-shell branch remains local and unpushed |
| 2026-07-14 | Product-shell repair now uses a Core-signed/broker-persisted monotonic runtime revision and broker-signed acceptance Receipt before Core commit; the broker serializes switch transitions with effects and rejects every stale Execute revision. Runtime history detects valid-row rollback; Core I/O is bounded, cancellable, deadline-limited, and generation-isolated; Swift switch reconciliation is serialized; staging includes the daemon/worker/plist and exclusively claims its output after post-copy pin verification | 116 Rust tests and 33 Swift tests pass locally; strict release builds/Clippy/format pass; the exact pinned runtime passes its real sandboxed initialize/account-read diagnostic; `/private/tmp/OpenOpen-Stage-Repaired.app` passes deep ad-hoc verification and contains all broker artifacts | focused local repair PASS; two new isolated reviewers required before commit/push; no product-shell GitHub CI, signed/admin install, cross-UID proof, real login/model outcome, notarization, product E2E, or release proof |
| 2026-07-14 | Second product-shell reviewer cycle: Reviewer C FAIL and Reviewer D FAIL. They reproduced a legacy put worker committing across daemon restart after a newer Off, complete valid-prefix/whole-database runtime rollback with no recoverable Core jump, model routes trusting only rolled-back Store state, broker-accepted Off reverting the UI to On, refresh/toggle generation races, unbounded model-catalog/Host response accumulation, and incomplete Keychain-derived secret zeroization | two fresh isolated reports; both independently reran the then-current 116 Rust and 33 Swift tests | prior 116/33 green result superseded; product-shell branch remains local and unpushed |
| 2026-07-14 | Second product-shell repair moves runtime transition, revision check, exact pre-rename fence, and the whole effect commit under one root-owned cross-process lock; the broker persists a nonce-bound high-water checkpoint; Core can record a broker-signed recovery jump after whole-database rollback; every model route consumes a fresh Core challenge bound into the broker's current-state Receipt; Swift keeps model entry fail-closed until protected/Core state converges and generation-guards refresh; model/catalog/outbound frames and the response queue are bounded; Keychain-derived buffers use `zeroize` | 121 Rust tests and 35 Swift tests pass locally; focused tests cover legacy worker versus restarted-daemon Off, revision-1 snapshot restored after revision-10 Off and continued revision-11 operation, rolled-back account/models/outcome rejection with a live Off checkpoint, accepted-Off Core failure, delayed refresh versus toggle, model bounds, and oversized Host output. Full Rust/Swift test/release/strict lint/format/plist/diff checks pass; the exact pinned runtime diagnostic passes; `/private/tmp/OpenOpen-Stage-Repair2.app` passes deep ad-hoc verification | focused local repair PASS; two fresh isolated reviewers required before commit/push; no product-shell GitHub CI, signed/admin install, cross-UID proof, real login/model outcome, notarization, product E2E, or release proof |
| 2026-07-14 | Third product-shell reviewer cycle: Reviewer E FAIL and Reviewer F FAIL. They reproduced stale On proof/generation races, reuse of one consumed challenge for account and models, failed-Off-to-On convergence and stale-refresh failure gaps, Swift-side effect private-key derivation/copies, unbounded Codex stdout/turn accumulation, and insufficient legacy-worker runtime/fence evidence | two fresh isolated reports; both reviewed the then-current 121/35 green tree and focused evidence | prior 121/35 green result superseded; product-shell branch remains local and unpushed |
| 2026-07-14 | Third product-shell repair makes Rust Core the sole effect-key derivation and enrollment-signing authority; Swift passes only the public broker trust anchor. Off clears the outstanding challenge; every model entry is generation-bound, account/models use separate fresh proofs, convergence covers desired/UI/Core/protected state, and stale refresh outcomes are ignored. Codex now uses a termination-safe bounded stdout queue plus item/text ceilings. The legacy-worker test performs a persistent SQLite On-1 → Off-2 transition under the shared guard and proves an old revision is rejected at the exact pre-rename fence | 125 Rust tests and 38 Swift tests pass locally; release builds, warnings-as-errors, strict Clippy/format/plist/script/diff checks, and `/private/tmp/OpenOpen-Stage-Repair5.app` deep ad-hoc verification pass. One cold diagnostic exposed the former two-second version-probe bound; it remains force-bounded at five seconds and the final exact diagnostic passed twice consecutively | focused local repair PASS; two fresh isolated reviewers required before commit/push; no product-shell GitHub CI, signed/admin install, cross-UID proof, real login/model outcome, notarization, product E2E, or release proof |
| 2026-07-14 | Fourth product-shell reviewer cycle: security Reviewer G FAIL and governance Reviewer H FAIL. Reviewer G reproduced a multi-App/Core window in which one Host's broker-accepted Off could not revoke another Host's process-local challenge or active model token. Reviewer H found `BUILD_WEEK.md` and `PROVENANCE.md` still reporting the obsolete second 121/35 repair | two isolated reports on frozen diff `0731370…`; focused suites and staging checks passed but could not close either finding | prior 125/38 green result superseded; product-shell branch remains local and unpushed |
| 2026-07-14 | Fourth product-shell repair holds a private user-scoped SQLite exclusive Core-instance lock for the Host lifetime and declares `LSMultipleInstancesProhibited`; a second App/Core therefore fails closed before Store/model authority opens, leaving global Off's challenge invalidation and cancellation token attached to the only running model process. Live disclosure surfaces are synchronized | 126 Rust tests and 38 Swift tests pass locally; a deterministic child-process test proves a second independent Host against the same support directory is rejected and can take over only after the first exits. Release/lint/format/plist/script/diff checks, `/private/tmp/OpenOpen-Stage-Repair6.app` deep ad-hoc verification, and two consecutive exact pinned-runtime diagnostics pass | focused local repair PASS; two fresh isolated reviewers required before commit/push; no product-shell GitHub CI, signed/admin install, cross-UID proof, real login/model outcome, notarization, product E2E, or release proof |
| 2026-07-14 | Fifth product-shell reviewer cycle: security Reviewers I and J both FAIL the frozen fourth repair. They independently found that Host released its user lock before detached model/Codex work was guaranteed dead, and that a same-EUID process could unlink/recreate the user-owned SQLite lock path to split exclusion | two fresh isolated reports on frozen diff `5c1b663…`; reviewers otherwise found the effect fence, generation/challenge guards, Core-only effect signing, Codex bounds, staging labels, and remote disclosure honest | prior 126/38 green result superseded; product-shell branch remains local and unpushed |
| 2026-07-14 | Fifth product-shell repair replaces the user lock as security authority with a broker-signed, root-owned durable Core lease. The lease binds audit EUID, authenticated App PID/start time, dynamically team-validated bundled Core child PID/start time, and a fresh per-Host nonce; Core verifies the enrolled broker signature and gates every model and On path. Core leads a private process group, and every spawned pinned Codex process is actively checked to inherit that exact PGID. Global Off first durably persists protected Off while retaining the old lease, then revokes and reaps that complete PGID, confirms it empty, exact-CAS clears the lease, and only then returns acceptance to App. A daemon crash can therefore leave only safe Off plus an occupied old lease, never a lease-free On window. Durable protected SQLite state survives daemon restart; concurrent acquire has exactly one winner; `LSMultipleInstancesProhibited` remains only a cooperative secondary control | 129 Rust tests and 43 Swift tests pass locally, including durable restart, concurrent acquire, exact release, caller-authority derivation, stale-group retirement, Off-persist-before-reap ordering, exact-leader On rejection, signature/nonce/PID binding, Codex-PGID containment, and no-lease fail-closed coverage. Rust release/fmt/strict Clippy, Swift warnings-as-errors test/release/strict format, plist/script checks, deep ad-hoc verification of `/private/tmp/OpenOpen-Stage-Repair10.app`, and two consecutive exact pinned-runtime diagnostics pass | focused local repair PASS; two fresh isolated reviewer PASS reports still required before product-shell commit/push; no signed/admin or cross-UID installation proof, product-shell GitHub CI, real login/model output, notarization, product E2E, or release proof |
| 2026-07-14 | Sixth product-shell reviewer cycle: fresh security Reviewer M FAILS the frozen fifth repair because a daemon crash after protected Off persistence but before PGID reaping can leave the active model group running, and an unrelated process reusing the old Core PID can wedge the durable lease. Governance Reviewer N is canceled as soon as the frozen tree is invalidated; no partial result is counted | isolated security report on frozen fingerprint `6ce2ef2…`; Reviewer M independently reruns the 129-Rust/43-Swift suites and verifies the fingerprint before and after | fifth repair is superseded; product-shell branch remains local and unpushed |
| 2026-07-14 | Sixth product-shell repair keeps the old lease occupied, validates the exact Core incarnation, delivers SIGKILL to its exact PGID, and proves the group empty before protected Off persistence. Failed signal delivery rejects without a protected-state write; after successful delivery the group cannot finish even if the daemon exits. The broker then persists Off, exact-CAS releases the lease, and only then returns acceptance. A changed start time proves PID reuse, so the unrelated process receives no signal while the signed stale lease is exactly retired | 129 Rust tests and 45 Swift tests pass locally, including kill-before-persistence, failed-kill/no-persistence, reused-PID/no-signal lease recovery, durable restart, concurrent acquire, exact release, authority derivation, signature/nonce/PID binding, Codex-PGID containment, and no-lease fail-closed coverage. Rust release/fmt/strict Clippy, Swift warnings-as-errors test/release/strict format, plist/script checks, deep ad-hoc verification of `/private/tmp/OpenOpen-Stage-Repair11.app`, and two consecutive exact pinned-runtime diagnostics pass | focused local repair PASS; two entirely fresh isolated reviewer PASS reports still required before product-shell commit/push; no signed/admin or cross-UID installation proof, product-shell GitHub CI, real login/model output, notarization, product E2E, or release proof |
| 2026-07-14 | Seventh product-shell reviewer cycle: fresh governance Reviewer P FAILS because the canonical plan still names the completed foundation as the immediate resume point and calls the current state the fifth repair after five cycles. Security Reviewer O is canceled when the fingerprint is invalidated; no partial result is counted | isolated governance report on frozen fingerprint `8905784…`; all other evidence, counts, Repair11 labeling, remote facts, and unclaimed tiers were found honest | documentation state is corrected and refrozen; two entirely fresh reviewers remain required |
| 2026-07-14 | Eighth product-shell reviewer cycle: fresh security Reviewer Q and governance Reviewer R both FAIL frozen fingerprint `1dda502…`. Reviewer Q finds that Codex can change PGID after the one-time check, that numeric `killpg` has a check-to-signal PID-reuse race, and that running Core validation binds only Team ID rather than the exact Core identifier. Reviewer R independently finds the direct `zeroize` dependency absent from `THIRD_PARTY_NOTICES.md` | two isolated reports on one unchanged fingerprint; reviewers otherwise confirm the remote/CI facts and unclaimed proof tiers | the sixth repair is superseded; no reviewer result is counted as PASS |
| 2026-07-14 | Seventh product-shell repair makes the root broker derive stable Mach audit tokens for the exact running Core and one prestarted persistent Codex process, binds both tokens/PIDs/start times into the signed durable lease and exact-CAS release, and requires token stability across identity inspection. On requires both exact incarnations live. Off terminates Codex then Core with `proc_terminate_with_audittoken`, proves both tokens dead, then persists protected Off and releases the lease; no numeric PID/PGID signal remains in the security path. The Codex sandbox denies fork, so the pinned process cannot create unregistered descendants; its mutable PGID is only cooperative defense in depth. Running Core validation now binds exact identifier plus App Team, while Codex binds exact identifier, OpenAI Team, and CDHash. The direct `zeroize 1.9.0` notice is recorded | 130 ordinary Rust tests pass with one explicit real-runtime test separately passing twice; 37 broker/signing Swift tests plus 12 App tests pass. Coverage includes real audit-token termination, PID-reuse snapshot rejection, exact token termination order, failed-termination/no-persistence, signed preimage and exact release tamper, sandbox fork denial, and exact Core/Codex signing requirements. Rust release/fmt/strict Clippy, Swift warnings-as-errors test/release/strict format, plist/script/diff checks pass. The first Repair12 stage exposed and rejected a deep-sign identifier rewrite; corrected `/private/tmp/OpenOpen-Stage-Repair14.app` passes deep ad-hoc verification with exact identifiers and pinned Codex hashes, plus two consecutive exact-runtime initialize/account-read diagnostics | focused local repair PASS only; two entirely fresh isolated reviewer PASS reports remain required before product-shell commit/push. The stage is explicitly ad-hoc, not signed/admin, cross-UID, notarized, clean-install, product-E2E, or release proof |
| 2026-07-14 | Ninth product-shell reviewer cycle: fresh security Reviewer S and governance Reviewer T both FAIL frozen fingerprint `81b20d6…`. Reviewer S finds that Codex was initialized before its broker lease and that App/Core timeout/shutdown cleanup still used numeric PID/PGID signaling; the root broker worker timeout path likewise used a numeric PID after an `isRunning` check. Reviewer T finds `BUILD_WEEK.md` undercounting the issue-finding cycles | two isolated reports on one unchanged fingerprint; both independently confirm the exact accepted-Off audit-token path, Repair14 evidence, local suites, and remote/CI disclosure while rejecting the remaining lifecycle and governance gaps | the seventh repair is superseded; no reviewer result is counted as PASS |
| 2026-07-14 | Eighth product-shell repair splits exact Codex spawn from initialization: the Host starts one sandboxed, non-forking Codex child without sending a request, the broker persists and Core installs the full exact audit-token lease, and only then may initialize/account/model traffic begin. Pre-lease failure drops the unreaped Rust-owned child, whose exact `Child` handle kills and waits it without PID reuse; after lease installation failures remain fail-closed under durable broker authority. App/Core production cleanup closes pipes and waits without numeric signaling. Every root worker is token-snapshotted before request bytes are written and all timeout/error reaping uses only `proc_terminate_with_audittoken` plus exact-death verification | 131 ordinary Rust tests pass; 39 broker/signing Swift tests plus 14 App tests pass. Focused coverage rejects pre-initialize requests, proves lease installation precedes initialization, aborts an uninitialized candidate on acquire failure, forbids App/Core numeric signal authority, requires an exact worker token before work, and forbids root-worker numeric fallback. The full Rust/Swift release, strict lint/format, plist, script, and diff verification set passes locally. Exclusive `/private/tmp/OpenOpen-Stage-Repair15.app` passes deep strict ad-hoc verification with exact App/Core/broker/worker identifiers and pinned Codex identifier `codex`, Team `2DC432GLL2`, CDHash `cf4f00…`, and all four manifest hashes; its real sandbox initialize/account-read diagnostic passes twice consecutively | focused local repair PASS only; two entirely new isolated reviewer PASS reports, product-shell commit/push, and current GitHub CI remain required. Repair15 is explicitly ad-hoc, not signed/admin, cross-UID, real-provider, notarized, clean-install, product-E2E, external-user, or release proof |
| 2026-07-14 | Tenth product-shell reviewer cycle: fresh security Reviewer U FAILS frozen fingerprint `dd3b1cea…`; governance Reviewer V PASSES the same tree, but its PASS is not reusable after a security-invalidating edit. Reviewer U proves repeated provisioning reinitializes the already initialized Codex before an Off request can reach Core/broker, leaving protected On and active model work behind a false-Off UI; U also proves initial worker token selection can bind a PID-reused unrelated root process before later exact-token cleanup. Reviewer V independently validates counts, Repair15, remote facts, and unclaimed tiers | two isolated reports on one unchanged fingerprint; both rerun the full suites and verify the fingerprint before/after | the eighth repair is superseded; the governance PASS is historical process evidence only, and two entirely fresh reviewers remain required on the next tree |
| 2026-07-14 | Ninth product-shell repair separates broker trust from Codex readiness. On/model paths cache readiness only for the exact Core instance nonce, while Host initialize is idempotent under the same immutable installed lease. Off never spawns, reacquires, or initializes Codex: after broker trust it directly prepares Off in Core, clearing challenges and canceling active work, then applies protected Off against the durable lease; a dead Codex or failed future acquire cannot block this route. Root worker authority now requires no observed exit plus token-before → exact identity → token-after stability, binding PID, daemon parent, root EUID, nonzero start time, and the canonical protected worker executable before any request/payload byte. Mismatch or observed exit closes pipes and never signals the captured replacement token | 131 ordinary Rust tests pass; 40 broker/signing Swift tests plus 15 App tests pass. New deterministic coverage uses a duplicate-initialize-rejecting Core, proves On→Off reaches broker exactly once per state and cancels active work, proves a dead-Codex/future-acquire failure cannot block Off, proves repeated account/models proofs do not reinitialize, and rejects immediate-exit/PID-reuse worker authority without termination. The full strict Rust/Swift verification passes. Exclusive `/private/tmp/OpenOpen-Stage-Repair16.app` passes deep strict ad-hoc verification with exact identifiers and pinned Codex identity/four hashes; its real sandbox initialize/account-read diagnostic passes twice consecutively | focused local repair PASS only; two entirely fresh isolated reviewer PASS reports, product-shell commit/push, and current GitHub CI remain required. Repair16 is explicitly not signed/admin, cross-UID, real-provider, notarized, clean-install, product-E2E, external-user, or release proof |
| 2026-07-14 | Eleventh product-shell reviewer cycle: fresh security Reviewer W FAILS frozen fingerprint `dd9ad888…`; governance Reviewer X PASSES that same tree, but its PASS is not reusable after the security-invalidating edit. W proves `requestEnabled(false)` published Off before Core cancellation or protected broker proof, so repeated provisioning failure could leave protected On and active work behind a false-Off UI; dashboard failure likewise invented Off. X independently validates the prior lifecycle repair, counts, Repair16, live remote facts, and unclaimed tiers | two isolated reports on one unchanged fingerprint; both rerun the full suites and verify the fingerprint before/after | the ninth repair is superseded; the governance PASS is historical process evidence only, and two entirely fresh reviewers remain required on the next frozen tree |
| 2026-07-14 | Tenth product-shell repair separates authoritative protected state, desired state, model-entry permission, and transition/unknown presentation. Off intent immediately advances generation and blocks new model entry, but does not set authoritative `enabled` false; Core clears the challenge and cancels active work before cached broker trust is consulted. A known-On runtime may display Off only after broker acceptance or fresh matching protected status; a fresh Core with no protected history may report its explicit default-Off state. Pre-apply failure preserves the last certain state, response loss and dashboard failure show Unknown, and Core/broker mismatch cannot fabricate Off. Broker trust and Codex readiness remain cached independently by exact Core instance nonce; prior uninitialized-Codex and stable worker-token invariants remain intact | 131 ordinary Rust tests pass; 40 broker/signing Swift tests plus 21 App tests pass. Fault injection covers provisioning failure after On, Core Off-prepare failure, broker rejection before persistence, response loss after persistence followed by fresh proof, dashboard failure, missing protected proof while Core reports On, dead Codex/future lease-acquire failure, and desired-Off model-entry blocking. The full strict Rust/Swift verification passes. Exclusive `/private/tmp/OpenOpen-Stage-Repair17.app` passes deep strict ad-hoc verification with exact identifiers and pinned Codex identity/four hashes; its real sandbox initialize/account-read diagnostic passes twice consecutively | focused local repair PASS only; two entirely fresh isolated reviewer PASS reports, product-shell commit/push, and current GitHub CI remain required. Repair17 is explicitly not signed/admin, cross-UID, real-provider, notarized, clean-install, product-E2E, external-user, or release proof |
| 2026-07-14 | One additional isolated pre-freeze security audit rejects Repair17 before formal Stage 5. It proves a failed-Off refresh could erase explicit Off intent and reopen model entry; Host reused one cancellation token and removed active authority before the exact operation finished; initial UI and nondefault Core Off snapshots could claim Off without sufficient proof; an obsolete await failure could strand a newer toggle. Its first repair re-audit then finds a narrower login-install-versus-cancel lock gap | read-only preflight plus deterministic App/Host interleavings; no formal reviewer result is reused or counted | Repair17 is superseded before freeze; this is issue-finding process evidence, not one of the two required Stage-5 reviewer PASS reports |
| 2026-07-14 | Eleventh product-shell repair keeps explicit user intent pending until convergence, starts UI state Unknown, accepts brokerless Off only for the exact Core default `revision=0/updatedAt=0`, matches protected/Core revision and timestamp, skips Codex readiness while Off is pending, and continues reconciliation after an obsolete generation fails. Host allocates a unique cancellation identity per operation, retains canceled active work until the exact worker finishes, prevents stale finish from clearing a successor, and serializes login install/cancel under one `active → login` boundary with exact-token validation | 133 ordinary Rust tests pass; 40 broker/signing Swift tests plus 24 App tests pass. Deterministic tests cover failed-Off refresh preserving intent, exact default-Off startup, nondefault brokerless Off rejection, stale-await latest-intent convergence, token nonreuse, canceled-active exclusion, stale-finish isolation, and a real threaded login-install/cancel interleaving. Full Rust/Swift strict test/release/lint/format checks pass. Exclusive `/private/tmp/OpenOpen-Stage-Repair18.app` is freshly staged with `STAGED_AD_HOC_NOT_RELEASE_PROOF`, passes deep exact-identifier and four-component-hash verification, and its explicit real pinned-runtime sandbox initialize/account-read diagnostic passes twice consecutively | focused repair and non-official preflight PASS only; two entirely fresh formal reviewers remain required before product-shell commit/push/current GitHub CI. No signed/admin, cross-UID, real-provider, notarized, clean-install, product-E2E, external-user, or release proof is claimed |
| 2026-07-14 | Twelfth product-shell reviewer cycle: fresh security Reviewer Y FAILS frozen fingerprint `2426b866…`; governance Reviewer Z PASSES that same tree, but its PASS is not reusable after the security-invalidating edit. Y proves canceled pending-login work could release the active slot, then let a new route reset shared cancellation while protected state still reported the old On; Y also finds App model authorization omitted protected/Core `updatedAtMs` equality. Z independently validates the then-current counts, Repair18, live remote facts, and unclaimed tiers | two isolated reports on one unchanged fingerprint; both verify the fingerprint before and after their review, and governance reruns the full suites | the eleventh repair is superseded; the governance PASS is historical process evidence only, and two entirely fresh reviewers remain required on the next frozen tree |
| 2026-07-14 | Twelfth product-shell repair makes one locked Host operation gate own startup-unknown, enabled, revision-bound pending-Off, and the exact active token. Off cancellation clears pending login state; an older On commit/recovery cannot release the latch, while a sufficiently new broker-protected On revision can. App model authorization now requires recovered enabled, revision, and `updatedAtMs` to match the protected authorization exactly | 134 ordinary Rust tests pass; 40 broker/signing Swift tests plus 25 App tests pass. Deterministic coverage reproduces canceled-login slot release, rejects old-On commit and recovery replay, accepts only the fresh protected revision, and rejects timestamp-mismatched model entry. Full Rust/Swift strict test/release/lint/format, plist/script/diff, and credential checks pass. Exclusive `/private/tmp/OpenOpen-Stage-Repair19.app` is freshly staged with `STAGED_AD_HOC_NOT_RELEASE_PROOF`, passes deep exact-identifier and four-component-hash verification, and its correctly selected explicit real pinned-runtime sandbox initialize/account-read diagnostic passes twice consecutively | focused local repair PASS only; two entirely fresh formal reviewer PASS reports remain required before product-shell commit/push/current GitHub CI. No signed/admin, cross-UID, real-provider, notarized, clean-install, product-E2E, external-user, or release proof is claimed |
| 2026-07-14 | Thirteenth product-shell reviewer cycle: fresh security Reviewer AA FAILS frozen fingerprint `b0d9e514…`; governance Reviewer AB PASSES that same tree, but its PASS is not reusable after the security-invalidating edit. AA proves the App loaded and sent its Keychain master after checking only path shape, allowing a same-UID regular-file Core replacement to receive Core authority before the broker's later identity check. AB independently validates Repair19 counts/staging, live remote facts, provenance, and unclaimed tiers | two isolated reports on one unchanged fingerprint; security reruns the focused 16-Host/25-App suites, governance reruns the full suites and stage checks, and both verify the fingerprint before/after | the twelfth repair is superseded; governance PASS is historical process evidence only, and two entirely fresh reviewers remain required on the next frozen tree |
| 2026-07-14 | Thirteenth product-shell repair validates exact Core signing identifier and current App Team statically before launch, then obtains the running Core's Mach audit token and validates that exact incarnation against the same requirement before the Keychain master loader or private bootstrap write can run | 134 ordinary Rust tests pass; 40 broker/signing Swift tests plus 27 App tests pass. Focused tests prove an unsigned regular replacement and running-auth failure both leave master loading at zero. Full Rust/Swift strict test/release/lint/format, plist/script/diff, and credential checks pass. Exclusive `/private/tmp/OpenOpen-Stage-Repair20.app` is freshly staged with `STAGED_AD_HOC_NOT_RELEASE_PROOF`, passes deep exact-identifier and four-component-hash verification, and its correctly selected explicit real pinned-runtime sandbox initialize/account-read diagnostic passes twice consecutively | focused local repair PASS only; two entirely fresh formal reviewer PASS reports remain required before product-shell commit/push/current GitHub CI. No signed/admin, cross-UID, real-provider, notarized, clean-install, product-E2E, external-user, or release proof is claimed |

## Current blockers

- The linearizable global effect fence/reconciliation foundation is committed
  at `19ecdd9…`, has two fresh isolated reviewer PASS reports, and passes the
  local same-SHA suites. Follow-up Actions run `29370433505` passes synthesized
  PR merge `d502b3d…`, whose tree equals CI workflow head `923c88a…`; this is not direct
  exact-head proof. Signed/admin, cross-UID, and release proof remain absent.
- Public `thesongzhu/OpenOpen` now exists and `main` points to reviewed
  bootstrap `19ecdd9…`. Draft PR #1 carries the CI workflow on
  `agent/foundation-ci`; it must remain unmerged until its current head checks
  and the applicable later proof gates are honestly satisfied.
- Real ChatGPT, iMessage, Discord, Reminders, notarization, clean-machine, and
  three-user evidence have not yet been run. They remain required and cannot be
  represented by mocks.
- The product-shell changes are still an uncommitted local working tree. Its
  thirteenth repaired focused suites are green after thirteen formal reviewer
  cycles plus one additional pre-freeze security audit found issues (security
  Reviewer AA invalidated frozen `b0d9e514…`, governance Reviewer AB passed that
  obsolete tree), but two fresh reviewer
  passes, a GitHub branch/PR run,
  real managed login/model output, and signed package proof do not yet exist.

## Unclaimed capabilities

Until the corresponding same-SHA evidence is linked here, every product route
is `implementation_in_progress`, not production-ready.
