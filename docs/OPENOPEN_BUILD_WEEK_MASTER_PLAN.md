# OpenOpen Build Week Master Plan

Status: OWNER_APPROVED_CHOICE_LOOP_CONTRACT

Canonical operating rules:
/Users/jarvis/Desktop/agents-generic-phase-batch.md

Current product route:
ten-hour latest-safe B+ delivery — PR1 → PR2 self-chat → Core+iMessage Hero
checkpoint → minimal B2 → minimal C2 → final B+ App/DMG

The former BUILD_WEEK_COMPETITION_READY authority is retired. Its remaining
chronological evidence is preserved below the historical boundary, while the
superseded normative wording remains available in repository history rather
than as a second live contract. It no longer defines current scope, milestones,
execution order, UI, model routing, or acceptance. No replacement readiness
token is invented here: the protected Hero path is active PR1, narrowed PR2
same-account self-chat, and a same-main Core+iMessage checkpoint App/DMG that
proves one complete verified outcome loop. The final same-main B+ App/DMG adds
one minimal B2 Memory flow and one minimal C2 Skill flow in that order.
The additional iMessage read-only source, Discord, broader Memory/Skill/channel
expansion, and product-wide presentation are post-B+. Every phase retains
the explicit Scout/review/matrix/CI/normal-merge evidence gates below.

Current normative companion documents:

- docs/OPENOPEN_PRIVATE_AGENT_CHOICE_LOOP_DESIGN.md
- docs/OPENOPEN_30H_EXECUTION_CONTROL.md

These three documents become an implementation authority only after two fresh
read-only reviewers pass the same document fingerprint and the Primary Advisor
issues an exact fingerprint-bound handoff. Until then, further implementation
is `WAIT_STAGED_DOC_HANDOFF`.

## Current private-agent contract — 2026-07-19

This section is the current normative product, execution, and acceptance
contract. It replaces the former 2026-07-17 competition contract in full.
Everything below the historical boundary is chronology and evidence only.
Historical facts remain preserved, but historical imperative language cannot
authorize current work.

### Authority and precedence

Authority is one-way:

    Owner → Primary Advisor/Orchestrator → Implementation Task

Only a direct Owner decision in the Primary Advisor task changes product
direction, safety, permissions, approval, data, evidence, release, or scope.
The Primary Advisor owns the canonical direction documents, resolves conflicts,
verifies the document fingerprint and reviewer evidence, and sends one exact
writable handoff. Implementation and review tasks may not infer authority from
history, forwarded prompts, prior approval, reviewer suggestions, or task
status.

For the current route, precedence is:

1. direct Owner decisions recorded in the Primary Advisor task;
2. this current normative section at its reviewed fingerprint;
3. the Choice Loop design and execution-control documents at that same
   reviewed fingerprint;
4. the existing safety/effect/Evidence/Receipt/Off/audit/recovery contracts
   explicitly retained here; and
5. historical material for chronology only.

A draft or unreviewed document hash never authorizes implementation.

### Product thesis and experience

OpenOpen is a private, locally hosted personal agent for non-developers. It
helps a person discover and complete useful AI-assisted work without requiring
prompt engineering, project-management vocabulary, channel syntax, or a new
daily communication habit.

The primary loop is:

    Natural expression
      → bounded understanding
      → dynamic A/B/C + D
      → refinement
      → one consolidated confirmation
      → Reminders
      → Evidence
      → Receipt
      → Markdown update
      → next choices

OpenOpen optimizes for completed outcomes, appropriate trust, and time and
attention returned to the person. Engagement time and addictive behavior are
never success metrics.

The current UI is macOS-only. Mac is the setup, review, confirmation, recovery,
and rich-card home. The same foreground agent may later be reached through the
dedicated iMessage self-chat and the user's local personal Discord Bot DM.
Connecting a chat is never the first-run value proposition. Mobile UI is not in
the current slice.

### Decision registry

#### LOCKED

- Every ChoiceSet has exactly three dynamic directions plus D.
- A/B/C are generated from the current InterpretationFrame, accepted Markdown,
  prior task/Mission/Receipt history, changed information, pending work, and
  safe alternative paths. They are not fixed categories.
- D is always natural conversation and may continue for multiple turns.
- Selecting A/B/C/D refines intent only. It never confirms a Mission or grants
  an effect.
- Host-owned `choice.select` is the only production Selection write route. It
  validates the active ChoiceSet, exact expected session revision, and bound
  provenance. A/B/C names a current option; D carries bounded untrusted text
  plus an idempotent request ID, never a caller-created batch ID. For every
  A/B/C/D variant, one SQLite `IMMEDIATE` transaction persists the Selection,
  creates its pending refinement operation, advances the next ChoiceSession
  revision/state, and appends audit; no Selection can commit without that
  operation. Host additionally derives and seals the authenticated D envelope/
  batch inside that same D transaction. Exact replay is idempotent; changed/
  stale/binding-drift input fails closed and no caller may replace a raw whole
  snapshot.
- Explicit complete Mac D intake commits its encrypted body, derived envelope,
  sealed batch, Selection, pending refinement operation, session/request
  registry, and audit together. Future quiet-window D collection persists
  encrypted envelopes/open batch first and can seal only together with the
  Selection/operation/session/audit transaction. Raw first-question/D text uses
  the existing Keychain-derived Store encryption, is deleted after cancel or
  accepted typed-state render receipt, and never enters logs/evidence/remote.
- A private refinement-result `IMMEDIATE` transaction alone completes the
  pending operation and persists result digest, encrypted frame/new ChoiceSet,
  session state/revision, and audit. No partial result is model/UI eligible.
- Host-owned `choice.begin` is the only public first-local-question
  intake/create route. Host derives the authenticated Mac SourceEnvelope and
  sealed one-question batch, verifies exact persisted model/catalog/protocol
  provenance, and atomically commits the initial `interpreting` ChoiceSession
  plus audit before model work. Only a private operation/generation/revision/
  provenance-bound result commit may create the first ChoiceSet. No raw
  snapshot writer or external effect authority is exposed.
- There is one global foreground ChoiceSession across the active Mac and
  iMessage self-chat surfaces. A later Discord phase must reuse that same
  session invariant. Confirmed Missions may continue in the background.
- Host-owned deterministic Store transitions use persisted deadlines and exact
  session revision/generation to enter 30-minute soft idle and 24-hour stale
  review. Timers are hints only; transitions alone run no model/effect work.
  Owner re-entry recalibrates/refreshes, and old choices/offline messages never
  execute.
- Explicit finish, cancel, or new topic persists the corresponding revisioned
  session transition.
- Same-delivery-binding bursts use an approximately 2.5-second quiet window and
  an 8-second hard cap. Off, cancel, and confirm bypass batching.
- Each ConversationTurnBatch has one required durable delivery binding derived
  by Host from its first authenticated SourceEnvelope. Later envelopes must
  match exactly. Restart preserves the binding; a historical missing value is
  typed blocked and never inferred from identity, provider ID, content, time,
  or source IDs.
- Dedicated private surfaces treat each owner message as addressed. An optional
  owner summon phrase refreshes choices; it is not an address or authority gate.
- OpenOpen scans the account's complete visible catalog for protocol-compatible
  GPT/Codex models and presents the compatible set before any model work. The
  user explicitly selects and persists one model and one actually supported
  effort level. There is no Auto route, fixed Sol requirement, hidden model or
  effort default, or silent fallback.
- Exact model ID, requested/actual effort, catalog fingerprint/revision,
  session/Mission revision, protocol revision, and Receipt provenance are
  durably bound.
- A compatible model without user-controllable effort uses typed
  `not_applicable`; no effort is invented and the model is not excluded.
- First launch is English-only welcome and account scan → explicit model and
  effort choice → one simple high-value question → first dynamic A/B/C plus D.
  No model-generated content or dynamic ChoiceSet exists before selection.
- All product-owned user-visible labels, options, states, help, setup,
  notifications, and recovery copy are English-only and never display a second
  language. User-authored/imported content remains source data in its original
  language.
- A reactive reply on an already connected binding returns only on the most
  recently accepted owner-active interactive channel without per-message
  confirmation. Mac mirrors local state and never duplicates the reply across
  channels. Proactive, new-recipient, and cross-channel delivery require exact
  confirmation.
- Missing historical selection, removed/incompatible model, catalog drift, or
  unavailable capacity produces typed Need you. It never substitutes a model.
- Reminders is the first end-to-end external work effect.
- User-visible continuity is bounded plaintext Markdown under
  ~/Documents/OpenOpen. Credentials, permission/effect authority, audit
  anchors, security state, and tokens stay in Store/Keychain.
- External Markdown edits execute nothing. Digest drift produces a semantic
  diff and reconfirmation before dependent effect work.
- Host rendering follows an exact Store render intent → descriptor-safe staged
  write → staged-file sync → atomic same-directory rename → parent-directory
  sync → final digest verification → exact receipt protocol. Restart adopts
  only bytes matching the intended manifest and displaced base. Existing files
  use an atomic swap/CAS-equivalent retaining the displaced inode/digest;
  creation is no-clobber. Concurrent Owner edits, partial multi-file swaps, or
  ambiguity preserve both versions in typed reconciliation without a receipt.
- Competition V1 remains OpenAI-only. Claude/Anthropic is not integrated. The
  Owner-supplied real ChatGPT export is authorized automatically only for an
  in-place local, read-only, no-network, no-retention B2 diagnostic. Mainline
  B2 may prepare with synthetic fixtures; bounded history excerpts may reach
  the explicitly selected OpenAI model only after a later exact Owner consent.

#### KEEP

- Command-owned Mission persistence and atomic Store/audit commits.
- Exact digest-bound confirmation: changes to payload, recipient, data, time,
  list/count, document digest, model, permission, or effect require a new
  revision and confirmation.
- Evidence-before-Done and Evidence-backed Receipt.
- Global Off, generation fences, cancellation, late-result discard, restart
  recovery, durable dedupe, and no provider/effect replay.
- Repair24 stable incident identity, durable acknowledgement, non-blocking
  activity, and reachable Dashboard, Settings, and Off.
- Protected broker, Keychain, local-only service, no central telemetry, and no
  cloud relay.
- Both historical provider dispatches remain terminal and must never retry,
  resend, edit, delete, or reopen.

#### RETIRED FROM CURRENT AUTHORITY

- fixed gpt-5.6-sol, Auto routing, and model/effort substitution;
- Connect Messages as the first screen;
- one input → one OutcomeSuggestion as the primary experience;
- the 15-minute per-input IntentSession and two-message correction cap as the
  main conversation model;
- fixed Hero A/B/C meanings;
- Quick Passport, Deep ZIP, proactive suggestion, GitHub Skill, and Workflow
  Candidate as the Choice Loop critical path;
- arbitrary iMessage conversation selection, all groups, ambient observation,
  Slack, shared/cloud Discord Bot, unrelated Discord DMs, third-party message
  authority, and automatic offline replay;
- marketplace/search/ratings, arbitrary Skill scripts, automatic Workflow
  execution, Private Memory, MEMORY.md export, Hero B/C, and current mobile UI;
- Repair23 Dashboard input as Repair24 or Choice Loop evidence.

B2 Deep ZIP/Dynamic Memory and C2 Skills are isolated staged support lanes.
They may prepare concurrently inside their owned paths, but cannot modify the
shared Choice contract or delay the PR1/PR2 Hero checkpoint. Shared integration
is serialized by the Integrator only after that checkpoint.

#### HERO-FIRST B+ CLOSURE — OWNER LOCKED 2026-07-20

The non-blockable critical path is PR1 plus PR2 same-account self-chat,
followed by same-main deterministic verification and a Core+iMessage Hero
checkpoint local App/DMG offline-verification receipt. During that path, B2,
C2, and post-B+ channels may prepare only in their owned paths. After the Hero
checkpoint, the Integrator lands minimal B2 Dynamic Memory and then minimal C2
instruction-only Skills before the final same-main B+ App/DMG. The additional
iMessage read-only source and PR3 local personal Discord Bot DM remain post-B+.
Ten hours is the latest-safe delivery target and execution deadline, never a
gate bypass. Any external outage, normal-merge rejection, permission, or exact
Owner action that threatens it is surfaced immediately rather than silently
extending the plan; unrelated safe READY work continues
afterward. Advanced visual/final-copy/animation work remains frozen until
these functional integrations are ready. The direct Owner-approved default
Persona bundle migration and review is PR1 work: its exact revision, content
digests, verified storage, protocol provenance, model request binding, and
audit/replay behavior must be complete before PR1 freezes. That authorization
does not authorize a new Persona voice, humor, pacing, or final copy beyond
the reviewed default bundle. Neutral English-only accessible UI remains
required for functional testing.

This staging changes only schedule scope. It does not weaken privacy, Host/
Store atomicity, Off, restart, Evidence-before-Done, Receipt, permission/effect
gates, Product Scouts, two-reviewer requirements, exact-head CI, content
parity, ordinary merge, or no-admin-bypass rules. A prepared support lane is
not complete, cannot block or satisfy the critical path, and cannot enter
mainline before its named dependency and independent gates pass.

Locked staged decisions:

- `OWNER-20260720-16H-FULL-FIRE`: its Core-first, maximum-four-lane/two-heavy-
  job, and exact-node deferred-Owner safety rules remain locked; its sixteen-
  hour schedule and broader integration order are superseded by the fourteen-
  hour Demo decisions below.
- `OWNER-20260720-B2-DYNAMIC-CARDS-CONSENT`: automatic B2 work is local and
  no-network; semantic preview exposes at most three dynamic candidate cards
  plus D, and only later exact Owner consent may send bounded source excerpts
  to the selected OpenAI model. Only selected cards may form a confirmed
  Markdown diff.
- `OWNER-20260720-IMSG-ONE-READONLY`: V1 permits at most one additional
  individually selected, revocable, one-to-one read-only iMessage source with
  no outbound authority; groups remain rejected.
- `OWNER-20260720-DESIGN-AFTER-FUNCTION`: advanced visual, new Persona
  behavior, final copy, density, and animation remain Owner-open until
  functional integration; the direct `OWNER-20260720-PERSONA-PR1` exception
  covers only the reviewed default bundle's technical migration and audit.
- `OWNER-20260720-PERSONA-PR1`: the direct Owner authorization moves the
  reviewed default Persona bundle's technical migration and audit into PR1.
  It must remain a verified non-executable bundle whose revision is bound to
  every Choice/model operation and durable audit; it cannot grant tools,
  recipients, permissions, Memory, retention, or effects. New Persona
  behavior and final conversational copy remain Owner-open. PR1 exposes no
  mutable Persona stage, activation, or rollback RPC; any later revision
  change needs a separately reviewed Owner action-time design.
- `OWNER-20260720-24H-CLOSURE-QUEUE`: its deduplicated Owner-return queue and
  liveness discipline remain locked; its twenty-four-hour schedule is
  superseded by the ten-hour B+ decision below. A reached
  Owner/admin/external boundary freezes only its exact node; unrelated READY
  work never waits for the Owner. The target never authorizes credentials,
  permissions, real provider/effect work, release, a merge bypass, or an
  invented product/design decision.
- `OWNER-20260720-REMINDER-SCHEDULE-BG`: Host may propose visible/editable
  Reminder date/time/timezone only from explicit user-provided temporal
  information. Missing time requires user selection; no fixed default and no
  inference from the question timestamp are allowed. Exact future date/time/
  timezone/list/count bind the confirmation digest; edits create a new
  revision and reconfirmation. Choice confirmation does not authorize the
  separate real Reminder write.
- `OWNER-20260720-14H-DEMO-CORE-B2-C2`: its exact Core/B2/C2 cardinality,
  action-time gates, and narrow UI bounds remain locked; its fourteen-hour
  schedule and co-equal Demo narrative are superseded by the B+ decision below.
- `OWNER-20260720-14H-DEMO-IMSG-INCLUDE`: its PR2 same-account self-chat scope
  and permission/selection/install/send gates remain locked; its Demo naming is
  superseded by the B+ decision below.
- `OWNER-20260720-10H-BPLUS-HERO`: the independently verifiable Hero completion gate
  is PR1 plus PR2 same-account self-chat plus a same-main Core+iMessage
  checkpoint that completes natural input → dynamic A/B/C+D → editable exact
  confirmation → real Reminder → readback/Evidence → Receipt → Markdown → next
  choices, and proves Off, restart recovery, and duplicate submission cannot
  duplicate effects. The final B+ package then adds exactly one B2 import with
  at most three candidate cards and one Owner-selected card whose exact
  Markdown diff is separately confirmed, and exactly one public instruction-
  only C2 Skill through acquisition/audit/enablement plus one no-external-
  effect use. B2/C2 are narrow proof chapters, not co-equal product stories,
  and may not delay or redefine the Hero gate. UI polish is limited to Core,
  B2, and C2 plus minimal iMessage setup/status. Extra read-only iMessage,
  Discord, broader expansion, full visual system, complex animation, persona,
  and product-wide final copy are post-B+. No B+ inclusion grants permission,
  selection, installation, real send/write/use, release, or merge-bypass
  authority.
- `OWNER-20260720-10H-BPLUS-DEADLINE`: supersedes only the prior earliest-safe
  pacing language. Ten hours is the latest-safe B+ delivery target and
  execution deadline. It never waives or combines a gate; a threatened deadline
  triggers immediate Owner notification with the exact action or external
  blocker rather than silent schedule extension.
- `OWNER-20260720-CHOICE-D-SELECT`: D is the bounded-text/idempotent-request
  variant of command-owned `choice.select`; Host derives/seals the batch and
  callers never supply batch identity.
- `OWNER-20260720-REFINEMENT-RESULT`: post-selection model output commits only
  through a private Selection/operation/generation/revision/provenance/manifest-
  bound transition with idempotent replay and late/Off/drift rejection.
- `OWNER-20260720-MARKDOWN-RENDER`: Store render intent, descriptor-safe staged
  write/file sync, atomic same-directory rename, parent-directory sync, final
  digest verification, and exact receipt define Markdown recovery; ambiguity
  blocks.
- `OWNER-20260720-IDLE-STALE`: Host-owned persisted-deadline transitions own
  soft-idle/stale-review; timer hints have no model/effect authority. Host—not
  caller/scheduler—derives time and state; same-boot continuous monotonic time
  is used, while reboot/backward/ambiguous clock evidence blocks safely.

#### OWNER_OPEN

The implementation task supplies neutral semantic fields and fixtures but does
not decide:

1. first-screen composition, card density, or D presentation inside the locked
   first-launch sequence and English-only product-language rule;
2. confirmation-card wording, visible fields, or inline-edit behavior;
3. visible CommunicationProfile dimensions and revocation presentation;
4. new Persona behavior, humor, tone, pacing, final copy, or visual system;
5. progress-notification wording/frequency inside an approved direction;
6. Mission-in-progress hierarchy and new-topic presentation;
7. simultaneous-input visual treatment and English wording for the locked
   latest-owner-active-channel rule, but not recipient authority;
8. exact 30-minute/24-hour return copy;
9. incident/error/recovery language and technical-detail placement; or
10. final iMessage/Discord setup presentation.

The prior D-intake architecture item is resolved by
`OWNER-20260720-CHOICE-D-SELECT`; its visual presentation remains covered by
item 1. Mac may not fabricate a batch ID, reopen the retired batch, or receive
a raw snapshot writer.

Each later Owner decision records date, status, affected semantic fields/tests,
and superseded decision IDs. Only the affected presentation node waits.
Before the Owner returns, every open presentation or interface item must have a bounded
decision packet containing the neutral semantic structure, current functional
screens, safe alternatives, affected tests, and the smallest resulting diff.
No packet may silently choose persona, copy, density, animation, or visual
direction. Once the functional dependency is ready, these packets join the
Owner-return queue rather than idling the implementation task.

### Core contract

The exact strict schemas are defined in
docs/OPENOPEN_PRIVATE_AGENT_CHOICE_LOOP_DESIGN.md. PR1 introduces versioned:

- SourceEnvelope and ConversationTurnBatch;
- ChoiceSession, ChoiceSet, ChoiceOption, and Selection;
- InterpretationFrame and UnderstandingPatch;
- CommunicationProfile and OutcomeDecision;
- DocumentManifest and DocumentDiff; and
- ChannelDeliveryBinding.

The post-B+ read-only-source phase adds strict versioned
`ReadOnlyConversationBinding`. The post-Hero B+ B2/C2 phases add
`DeepZipPreviewSession`, `MemoryCandidateCard`, and `MemoryImportDecision`
contracts plus the instruction-only lifecycle summaries. Each type remains
unavailable to earlier callers until its own normally merged phase enables it.

Unknown variants, missing required provenance, stale revisions, cross-session
IDs, invalid option count, non-distinct directions, oversized fields, or
authority in untrusted text fail closed.

Selection is a tagged exact-one enum for A/B/C option selection versus D input.
ChoiceSession has a typed preselection state and a stable model/effort
provenance reference. Host derives owner and delivery-binding identity only
from an authenticated binding; adapters and message bodies cannot assert it.
ConversationTurnBatch persists that Host-derived delivery-binding ID as a
required field; all member envelopes must carry the same authenticated binding
and missing historical values fail closed. Selection writes use
`choice.select`, not the continuity read/raw-snapshot route, and atomically bind
the Selection, next session revision, and audit evidence. Its D variant carries
bounded text/request ID; Host derives and seals the authenticated batch. A
private refinement-result commit is the only post-selection model-result write
and binds Selection, operation, generation, revisions, provenance, manifest,
and audit.
The first local question uses only `choice.begin`. Its caller supplies bounded
untrusted question data, an idempotent request ID, and expected persisted model/
catalog/protocol references; Host supplies every trusted envelope, binding,
batch, session, audit, and private result-commit field.
Cross-surface dedupe requires one durable shared OpenOpen idempotency or reply-
correlation ID. Content, identity, provider ID, digest, or timing similarity
alone never drops an envelope.

A model turn returns a strict interpretation/patch, response plan, and exactly
three candidate directions. D is product-owned. Host validates and commits the
ChoiceSet. Any accepted Markdown manifest change, model selection change,
source-consent change, or session/interpretation revision makes the prior
ChoiceSet stale.

### Task continuity and Markdown

The user-visible root is ~/Documents/OpenOpen. A task package preserves:

- overview, current state, decisions, and open questions;
- a bounded reproducible MODEL_BRIEF.md;
- separate notes for materially different paths;
- immutable update/event records;
- session and ChoiceSet summaries; and
- manifest digests binding the accepted state.

MODEL_BRIEF.md is a bounded view rendered from accepted typed state. It is not
system authority, an effect grant, or permission for a model to roam the
filesystem. A later compatible selected model receives the same semantic brief
contract so it can understand the user's task without hidden session memory.

Host allows only manifest-listed regular files and the approved render-intent →
descriptor-safe staged write/file sync → atomic no-clobber creation or swap/CAS
replacement retaining the displaced base → parent-directory sync → final/base
digest verification → exact receipt recovery flow,
with current-user ownership, mode 0600 files and 0700 directories, bounded sizes, normalized
relative paths, and exact digests. Traversal, absolute/parent paths, symlink,
hardlink, special-file, path replacement, owner/mode mismatch, Unicode/case
collision, prompt injection, or digest drift fails closed.

### Channels

#### Mac

Mac is primary. Cold start shows an English-only welcome, scans the account,
requires explicit compatible model and supported effort selection, asks one
simple high-value question, and only then generates the first dynamic A/B/C
plus D. It does not require channel setup. The scan and picker are setup, not a
static or model-generated ChoiceSet.

#### iMessage

The same-account self-chat is a bidirectional interactive private inbox.
User-authored self-chat input is distinguished from OpenOpen output with a
durable product-owned identity/echo marker. Ambiguous identity, duplicate echo,
loop, missing marker proof, or cursor drift keeps the binding Off.

After B+ closure, V1 may add exactly one additional local
one-to-one read-only source. It is individually selected, revision-bound, and
revocable, never receives OpenOpen output, and never becomes recipient/effect
authority. A second additional source, groups, ambiguous identity, or stale/
revoked binding is rejected before body persistence or model access. V1 stays
on the public/basic imsg boundary; no private IMCore, SIP change, Accessibility
automation, or TCP service is allowed.

#### Discord

Discord is a post-B+ integration. Owned-path preparation may proceed
concurrently, but shared wiring and merge occur only after B+ closure
and cannot delay or satisfy PR1/PR2.

The user creates a local personal Discord application/Bot. Its token is entered
once through a secure field, validated against the expected Bot identity, and
stored at rest only in Keychain. It exists transiently only in authenticated
local Gateway process memory, is never logged or persisted elsewhere, is
redacted from diagnostics, and is released or zeroized on stop where supported.
The Mac hosts the Bot Gateway.

Setup binds the authenticated owner Discord user ID, expected Bot/application
ID, and exact personal DM channel. Before body persistence or model access,
Host validates author, Bot, application, and conversation metadata. Unrelated
events are discarded after metadata-only classification; their bodies never
persist or reach a model.

When the Mac is offline, the Bot is offline. On reconnect, the pre-consent
recap contains deterministic metadata only: bounded count, time range, and
redacted correlation IDs. No body or model is used before the English
continue/discard choice. Only explicit continue admits bounded owner-bound
bodies into the normal path; discard advances the cursor without persistence,
model work, or effect. Old messages never auto-execute.

### Model and plan behavior

The model picker shows the complete compatible account scan with stable
English-only semantic fields for model identity, account availability,
supported effort controls, protocol compatibility, and live limit/status
information when the protocol provides it. The user explicitly chooses the
model and a supported plain-English effort option such as `Faster`, `More
thoughtful`, or `Deepest`; the exact protocol values remain in provenance. A
model without an effort control uses `not_applicable`. The product does not
preselect or silently rank a model or effort default.

OpenAI documents a Free plan path and ChatGPT sign-in, but availability, models,
usage, credits, and limits remain account/plan dependent. Product copy may say
“Start with the plan and models available to your account.” It must never
promise unlimited free work, a particular model, or continuous availability.

Official references:

- https://learn.chatgpt.com/docs/pricing
- https://learn.chatgpt.com/docs/auth

The implementation task's own model/effort setting is execution infrastructure
only and can never become the product's model contract.

### Understanding, communication, and safety

Understanding is a revisioned frame, not hidden free-form memory. The model
receives only bounded manifest-listed context compiled by Host. Third-party
messages and Markdown are untrusted data and cannot select, confirm, send, or
act.

Explicit communication preferences apply immediately. Inferred preferences
require repeated supporting evidence, remain inspectable/correctable, and are
removed by correction or revocation. CommunicationProfile may affect English
word choice/register, directness, length, pacing, formatting, and explanation
depth only. It cannot change the English-only UI/output language, locale,
recipients, data, retention, model, permissions, effects, approvals, or
Evidence. Sensitive traits and health/financial/legal/emotional diagnosis are
not inferred profile fields.

### Confirmation and effects

The dedicated Host-owned `choice.confirm` command owns consolidated Choice
confirmation. It never aliases, calls through, or accepts the legacy
`mission.confirm` route as Choice authority. It binds one immutable payload
digest containing:

- interpreted goal and ordered steps;
- exact Reminder items, text, date/time/timezone, count, and Evidence;
- exact Markdown paths, base/observed manifests, and semantic diff;
- selected model/effort/catalog/protocol provenance;
- any external recipient/conversation binding;
- data categories, retention, permission, and effect classes.

Host validates the active ChoiceSession/ChoiceSet and expected revisions, then
commits the exact confirmation payload, resulting session transition, and
audit record in one Store transaction. No partial confirmation survives
restart. This confirmation prepares authority for later exact effect gates; it
does not itself perform a Reminder write, delivery, permission change, or any
other external effect.

Host creates a visible, editable Reminder schedule proposal only when the user
has explicitly supplied temporal information. It validates that the proposed
date/time is future-facing in the selected timezone. When an exact time is
absent, the proposal remains incomplete and requires user selection; Host may
not insert a fixed time or infer one from the question/event timestamp. The
exact future date, time, timezone, list, and count are confirmation-bound.

The user can confirm, edit, or defer. Any exact-payload change creates a new
revision and confirmation. Inside an approved direction, OpenOpen may reason,
compile context, prepare drafts, update internal session state, and prepare
Markdown diffs. It may reactively reply on the most recently accepted
owner-active channel under an already connected binding without per-message
confirmation. It may not create/change Reminders, send proactively, add a
recipient, deliver across channels, expand data, install, change permissions,
or cross another effect without the applicable action-time gate.

Installation, administrator/password/passkey/Touch ID/2FA, macOS permissions,
Discord token entry, first channel connection/send, proactive delivery, a new
recipient, cross-channel delivery, Mission confirmation, Reminder write/manual
Evidence/readback, any newly different real ZIP disclosure, consent to send
bounded history excerpts to a selected model, selecting/committing Memory
cards, real Skill selection/promotion/update/enable/rollback/first use, public
release, destructive user data, and owner/admin bypass are never autonomously
crossed. The one supplied export is limited to the exact local/no-network/
no-retention B2 diagnostic already recorded above.

### Implementation sequence

PR1 — Choice Core and Mac:

- shared strict schemas and Store migrations;
- burst batching and one global ChoiceSession;
- English-only first launch and explicit compatible-model/effort scan,
  selection, typed `not_applicable`, and provenance;
- Mac neutral functional surfaces;
- bounded Markdown manifest/diff/rendering;
- consolidated confirmation;
- real Reminders, Evidence, Receipt, and next ChoiceSet;
- restart, Off, incident, race, stale, and late-result closure.
- sole Host-owned `choice.begin` intake/create route and private current-result
  commit for the first ChoiceSet.

PR2 — iMessage:

- same-account self-chat private inbox;
- echo identity, cursor, dedupe, restart, and loop closure;
- group rejection before persistence/model access;
- permission deny/cancel/revoke/regrant/restart with no repeat modal, false On,
  or unreachable Off.

Post-B+ iMessage read-only source:

- exactly one additional individually selected, revision-bound, revocable
  one-to-one source;
- inbound-only authority with no outbound, recipient derivation, or reply
  mirroring;
- second-source/group/stale/revoked/ambiguous routes rejected before body
  persistence or model work.

B2 — minimal B+ Memory proof chapter:

- fail-closed singleton/split ChatGPT export scanner and synthetic adversarial
  fixtures;
- exactly one real import and one local preview session with at most three
  dynamic Memory candidate cards, no fixed categories, and deletion of
  unselected raw/derived data;
- exact later Owner provider-processing consent before any bounded excerpt is
  sent to the selected OpenAI model;
- exactly one Owner-selected card produces a revision-bound semantic Markdown
  diff; only that exact diff may be confirmed and persisted.

C2 — minimal B+ instruction-only Skill proof chapter:

- exactly one public instruction-only Skill with canonical GitHub identity,
  immutable commit/digest, and structural/
  license/permission audit;
- Candidate → Staged → Promoted → Runnable with exact promotion/update/
  rollback binding and no scripts, silent update, or self-promotion;
- one Owner-selected acquisition, audit, enablement, and first use are separate
  action-time gates; the first use must have no external effect. Synthetic
  fixtures remain the autonomous verification route.

PR3 — Discord post-B+:

- local personal Bot setup plus owner/Bot/application/exact-DM validation;
- Keychain-at-rest token boundary, rotation/removal, and intent/identity drift;
- Gateway reconnect/cursor/restart;
- metadata-only pre-consent offline backlog ask;
- no cloud/shared Bot, unrelated DM, or automatic replay.

Each integration starts from the preceding merged main SHA. Each is
independently reviewable and rollbackable. Owned-path preparation is not merge
authority and may not modify root/shared files.

### Verification and merge gates

PR1 must cover exactly three dynamic choices plus D; English-only first-launch
scan → explicit model/effort → one question → first ChoiceSet, with zero model
work before selection; Host-owned `choice.begin` authenticated intake/session/
audit atomicity, exact replay, changed-replay/model/catalog/protocol/Off
rejection, and private stale-result rejection; Host-owned `choice.select`
atomic Selection/session/audit
persistence with stale revision rejection and no raw snapshot write;
command-owned D text intake/Host-derived batch and private refinement-result
single-transaction binding/replay fences, encrypted raw-turn retention/deletion
and collect/seal/commit crash recovery; required
persisted batch binding with Host derivation, mismatch rejection, restart, and
typed blocked missing-field migration; deterministic 2.5/8-second batching;
Host-owned deterministic 30-minute/24-hour transitions; stale ChoiceSet,
private Host-clock sleep/reboot/clock-uncertainty behavior, late-timer, and
late-result rejection;
model/catalog/effort
provenance and typed `not_applicable`; no Sol/Auto fallback; Markdown render-
intent/atomic-rename/receipt crash recovery plus traversal,
atomic no-clobber/swap base-CAS, concurrent Owner edit, partial-manifest
reconciliation, symlink, hardlink, collision, permission, size, digest,
conflict, secret, and
prompt-injection paths; dedicated `choice.confirm` atomic payload/session/audit
commit, exact confirmation drift, and proof that legacy `mission.confirm`
cannot grant Choice authority; Reminder permission,
partial-write, replay, readback/Evidence mismatch, false Done, restart, and Off.

PR2 must cover self-chat is_from_me classification, OpenOpen echo markers,
duplicate/loop/cursor/restart, group rejection, no wake-word address gate, and
deterministic permission deny/cancel/
revoke/regrant/restart without repeated modal, false On, or blocked Off.

The post-B+ read-only-source phase must cover exact cardinality one,
individual select/revoke, restart, stale/cursor/dedupe behavior, second-source
and group rejection, and proof that no outbound or recipient route exists.

B2 must cover split-member continuity, immutable snapshot/limit/path/collision/
corruption/cancellation failure closure, preview revision/restart/disposal,
three dynamic cards plus D without fixed categories, no automatic provider
request, explicit processing consent, selected-card-only Markdown diff, and no
private data in repository, evidence, logs, or remote.

C2 must cover immutable source/digest, acquisition limits/redirects, path/
symlink/license/executable/permission rejection, exact lifecycle transitions,
promotion nonce/revision binding, update/rollback, and zero instruction/script
execution before a later Mission gate.

PR3 must cover token at-rest/transient-memory boundaries, remove/rotate,
authenticated owner plus expected Bot/application/exact-DM identity, rejection
of unrelated events before body persistence/model access, Gateway loss/recovery,
metadata-only pre-consent offline backlog, intent/identity drift, Off, and zero
automatic model/effect work from old messages.

A read-only Product Scout runs before freeze and again after the implementation
freeze. It checks modal loops, unreachable controls/states, focus stealing,
repeated alerts, failure coupling, false Done/On, retry/replay, restart,
permission, and Off dead ends. Any P0/P1 invalidates the affected freeze and
blocks merge until fixed and re-audited.

Every meaningful fingerprint requires focused verification, relevant full
Rust/Swift matrices, lint/format/diff/secret checks, two fresh read-only
reviewers reporting P0/P1/P2=0/0/0 on the same fingerprint, CI green on the
exact PR head/integration tree, no unresolved thread, and content parity.

Each active PR earns `IMPLEMENTATION_MERGE_READY` independently before the next
active PR branches: applicable Scout, two fresh reviewers, exact-head CI,
content parity, and normal merge with no bypass. This status proves the implementation and
deterministic local gates only. `REAL_PRODUCT_PROOF` is separate and remains at
the exact action-time Owner gate for install, permission, provider/channel,
Mission, Reminder, Evidence, and installed-runtime effects. A later PR's gate
cannot substitute for an earlier PR's gate.

Normal auto-merge is allowed only after all gates pass. If GitHub rejects the
normal merge, stop. No --admin or implicit owner bypass is authorized.

Mocks, fixtures, CI, signatures, screenshots, or component probes support
evidence but never replace a real provider, permission, install, Reminder, or
runtime path that is claimed.

### Execution control

The current B+ window is ten hours from the reviewed handoff. It is the latest-
safe delivery target and execution deadline, not a gate bypass. PR1, narrowed PR2, and the
same-main Core+iMessage Hero checkpoint App/DMG are the independent completion
gate; minimal B2 and minimal C2 then land in the final B+ App/DMG. The extra
read-only iMessage source, Discord, broader B2/C2, and advanced UI/new
Persona behavior/copy are post-B+. The reviewed default Persona bundle's PR1
technical migration remains in scope. External waits for install, permissions,
credentials, real sends, provider-processing consent, Skill lifecycle actions,
and manual Evidence are separate.

The prior twenty-four-hour schedule is superseded, but its closure discipline
remains: every Owner-independent B+ route is merged/verified or carries an
exact evidence-backed blocker, every Owner-dependent child is deduplicated in
the return queue, and unrelated READY work never waits. The ten-hour
target is not a claim that absent credentials, unavailable external services,
rejected normal merges, or unmade action-time decisions can be completed
autonomously.

At most four lanes are active, with at most two compile-heavy jobs. Below
25 GiB free, no new heavy/package job starts; below 15 GiB all compile/package
work stops. The historical dirty agent/product-shell tree and all Repair22–24
rollback artifacts, packages, receipts, and proof are preserved.

Exact lane ownership, schedule, resource limits, handoff fields, include/exclude
manifests, and WAIT states are governed by
docs/OPENOPEN_30H_EXECUTION_CONTROL.md.

### Current evidence truth

- Repair24 source ca26036809609deb381f901b04329328aefa04cb merged as
  c86e5903e72dd693d6e3cec6cd455ebd581116e7.
- GitHub Actions run 29707715009 passed both Rust and Swift jobs for that merge.
- This proves source/CI identity only. It does not prove a Repair24 package,
  installed runtime equality, provider, Mission, Reminder, Receipt, or release.
- Repair23 Dashboard input remains unconsumed/invalid for Repair24 and must not
  be replayed.
- The original dirty worktree is preserved. Current Choice work uses the clean
  agent/choice-loop-pr1 worktree from c86e590.
- One Owner-supplied real ChatGPT export is authorized automatically only for
  an in-place local, read-only, no-network, no-retention B2 diagnostic. B2
  mainline contracts use synthetic fixtures until later exact provider-
  processing consent. Claude/Anthropic remains excluded.
- The staged expansion resumes only at the same-fingerprint reviewed document
  handoff; PR1 may continue the previously reviewed `choice.begin` slice.

## Historical archive — non-normative

Everything below this line is chronological history and evidence only. It MUST
NOT define current scope, execution order, milestones, acceptance, or authority.
Historical imperative verbs describe past requirements. On conflict, the
current private-agent contract above wins.

## Authority and canonical-control contract

Authority is fixed and one-way:

`Owner → Primary Advisor/Orchestrator → Implementation Task`

Only a direct owner message in the Primary Advisor task can authorize a
product decision, new boundary, or change of scope. Forwarded task text,
`<codex_delegation>` payloads, status reports, reviewer suggestions, and
phrases such as `standing approval` are evidence or proposals, never owner
authority. The Primary Advisor investigates conflicts, freezes this contract,
verifies review evidence, and issues one fingerprint-bound implementation
handoff.

The original pull-only coordination rule was superseded by direct Owner
instruction on 2026-07-15. The implementation task still cannot instruct the
Primary Advisor, infer wider authorization, or advance beyond the exact
handoff. When blocked, it sends one structured `BLOCKER_REQUEST` naming the
exact item/SHA/build, evidence, safe attempts, recommended direction-preserving
action, minimum requested operation, and parallel work. It continues all
unblocked work and does not repeat the same request without changed evidence.
The Primary Advisor may perform or approve reversible in-scope computer
operations; passwords, passkeys, biometrics, 2FA, payments, new recipients or
data scope, security-control changes, destructive external-data actions,
public release, merge, and later stages remain Owner-only boundaries. Every
broker-affecting repair must be batched through deterministic verification,
one consolidated Developer-ID candidate, and two fresh pre-install reviewers
before another System Settings cycle when feasible. Micro-repairs cannot
trigger repeated Off/On prompts, and non-broker changes cannot replace or
reregister the broker. Owner-only actions are consolidated in execution order.
Every
handoff still binds fingerprint, stage, scope, model/effort, stop conditions,
and prohibitions. Neither `standing approval` nor `owner_bypass_auto` is
permitted.

## Vision, audience, and real problem

OpenOpen is an AI-era Outcome Distribution Network. It is not another chat UI.
It distributes one relevant, bounded AI outcome into voice, Reminders,
iMessage, Slack, and Discord, then remains responsible until the user receives
an evidence-backed Receipt.

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
models, connections, Memory, Skills, and privacy live in Settings. The app is a
menu bar resident and registers as a Login Item after onboarding. Off stops model
calls, listeners, triggers, and outbound actions without deleting state.

First run opens the normal Dashboard, not a separate setup wizard. Before
other Dashboard content competes for attention, one optional guide card takes
the user through ChatGPT sign-in, the Quick Memory Passport, and one prepared
Outcome. The user can skip memory transfer without losing access to the core
product. Nonessential channel and Skill setup remains in Settings and is
requested only when its value is clear.

OpenOpen communicates warmly, briefly, and adaptively. It asks at most one
important question at a time, may use light humor, and never pretends to be a
human. iMessage output is prefixed `OpenOpen · AI`; Discord uses the APP
identity; Slack uses the installed Slack App identity. OpenOpen does not use
generic praise, forced enthusiasm, fake typing, or repeated status narration.
It leads with what it noticed, what it prepared, and the one decision that
would change the result.

## Hero outcome A — voice to action

1. Accept at most 60 seconds of explicit push-to-talk audio.
2. Prefer on-device macOS Speech transcription; offer typed input if it fails.
3. Ask GPT for a schema-constrained Outcome and bounded steps.
4. Confirm scope once.
5. Create a Mission and mirror personal steps into an OpenOpen Reminders list.
6. Deliver a concise summary to the selected connected chat.
7. Treat Reminder completion as Evidence and issue a Receipt.
8. Offer at most one adjacent Outcome.

There is no always-listening microphone. Hero A remains an explicit voice,
text, or approved-channel Mission. Separately, the judge slice may observe only
individually consented, owner-selected Slack and iMessage conversations to find
the bounded opportunity patterns defined below; this never grants execution or
outbound authority.

## FRIDAY_ALPHA_READY — accelerated intermediate milestone

`FRIDAY_ALPHA_READY` is an explicit intermediate delivery milestone targeting
Thursday/Friday, July 16–17, 2026 in `America/Los_Angeles`. It does not replace,
rename, weaken, or satisfy `PRODUCT_READY_FOR_DEMO`.

The Friday alpha is one narrow product loop, not three separate heroes:

1. Hero A accepts explicit voice or text input, or an explicitly addressed
   message from an approved iMessage or Discord conversation.
2. The pinned real GPT-5.6 route produces one structured Outcome; the owner
   confirms one bounded Mission.
3. OpenOpen performs the exact approved Reminders write and readback, treats
   Reminder completion as Evidence, and issues the Receipt.
4. iMessage and Discord are real bidirectional entry/readback surfaces for that
   same Mission. `Need you`, concise progress, and the Receipt may return only
   to the originating approved conversation. A sent message is never completion
   Evidence.
5. Both channels prove allowlisting or pairing, durable message-ID dedupe and
   cursor recovery, restart without duplicate send, and global Off preventing
   listeners, model calls, and outbound work.

After two similar Evidence-complete Hero A successes, OpenOpen may propose one
Workflow Candidate. That differentiation slice follows stable Hero A plus both
channels and must not delay the first installable Friday alpha. Cross-channel
group availability (Hero B) and receipt-to-XLSX (Hero C) remain required by the
final plan but move after `FRIDAY_ALPHA_READY`.

## JUDGE_SLICE_READY — early product handoff milestone

`JUDGE_SLICE_READY` is a second intermediate milestone. It exists so the owner
can receive a real, judgeable build early enough to polish product behavior and
visual design. It does not replace, rename, weaken, or satisfy
`FRIDAY_ALPHA_READY` or `PRODUCT_READY_FOR_DEMO`.

The implementation order is fixed:

1. Finish and preserve the already reviewed Hero A+iMessage+Discord Friday
   alpha; do not rewrite that slice to add Slack or memory.
2. Add the Quick Memory Passport and Auto model policy.
3. Add the direct-local Slack connection and the consented Slack+iMessage
   opportunity-to-private-preview route.
4. Prove one personalized prepared suggestion → owner confirmation → Hero A
   Reminders Evidence → Receipt loop without an unauthorized external effect.

`JUDGE_SLICE_READY` requires one same-SHA Developer-ID build that the owner can
run on the designated test Mac, real ChatGPT sign-in/model output, one reviewed
Memory Passport, real Slack and iMessage input, real Reminders write/readback,
restart without duplicate delivery, global Off, and no open blocker in that
route. If notarization is still pending, the milestone and package must say so
prominently; it is an internal owner-test build and not public release proof.
The full ZIP import, Hero B/C, Skills, three-user validation, notarization, and
release proof remain later gates.

The UI visual system is deliberately not frozen by this milestone. After the
product contract is implemented, a separate Figma/SwiftUI pass will establish
tokens, components, motion, and the final judge-facing visual language. That
pass may improve presentation but may not change Mission, approval, Evidence,
memory, channel-consent, or release semantics.

## Hero outcome B — collect availability and decide

1. The owner selects one iMessage conversation, one Slack channel, candidate
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
6. Generate a local XLSX containing detail, summary, formulas, and source refs
   with exact `rust_xlsxwriter 0.96.0`. It remains planned, not distributed, in
   the current payload; when introduced, lock the exact crate/source closure and
   update provenance and notices before any distribution claim.
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
- A minimal Rust Core owns Mission, Workflow, Memory, Skill, SQLite, approval,
  Evidence, channels, recovery, and XLSX generation.
- SwiftUI manages Rust Core as a child process over JSON-RPC/stdio.
- Rust Core manages a pinned Codex App Server over JSON-RPC/stdio. Core/App
  traffic opens no port; only managed ChatGPT sign-in temporarily lets that
  pinned child listen on localhost TCP 1455 or fallback 1457 for the OAuth
  callback.
- Codex proposes schema-constrained actions. Rust gates and executes every
  external effect.

The stable RPC families are `account.*`, `outcome.*`, `mission.*`, `channel.*`,
`receipt.*`, `memory.*`, `workflow.*`, and `skill.*`.

The stable domain contracts are `OutcomeSuggestion`, `Mission`, `WorkItem`,
`ApprovalRequest`, `NeedsMe`, `EvidenceRef`, `Receipt`, `ChannelEnvelope`,
`MemoryImportSession`, `MemoryCandidate`, `MemoryRecord`, `MemoryConflict`,
`MemoryUseGrant`, `WorkflowCandidate`, `WorkflowDefinition`, `SkillPackage`,
and `SkillPermissionManifest`.

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
- The root effect broker creates that writable runtime home only at
  `/Library/Application Support/com.thesongzhu.OpenOpenRuntime/users/<authenticated-audit-euid>/CodexHome`
  as a case-sensitive `tmpfs` mount with `nodev`, `nosuid`, and `noexec`, a
  fixed 256 MiB/32768-node bound, root-owned non-writable ancestors, and a
  user-owned mode-`0700` mount root. The request contains no path or user ID.
  Core independently requires the exact current-EUID path, owner, mode,
  filesystem type, exact mount point, and a different device from the login
  Keychain before it creates any Codex state. This kernel-enforced
  cross-filesystem boundary makes a hard link from the login Keychain into the
  writable runtime home fail instead of relying on a path-only check.
- Pin direct Keychain auth by setting and verifying effective
  `features.secret_auth_storage=false`. On macOS the exact pinned legacy
  Security.framework backend requires the canonical current user's login
  Keychain database to be readable inside the outer sandbox; only that exact
  encrypted database file is read-only during every account-read, model-list,
  and model-turn runtime, while all plaintext lookup remains scoped by the
  pinned `Codex Auth` service/account API. A first official managed login uses
  a distinct short-lived login-only Codex process whose sandbox adds write
  authority only to that exact canonical database file. It receives no account
  read, model-list, thread, or turn route. Success, failure, cancellation, URL
  launch failure, and Global Off all destroy it; the App then requires the old
  exact Codex audit-token process to be dead, rotates the broker-signed lease
  without signalling the unchanged authenticated Core, and starts a fresh
  read-only model process before account/model work.
- Use managed `account/login/start`, `account/read`, `model/list`, and stable
  thread/turn/events.
- Never read, copy, or parse `~/.codex/auth.json`; OpenOpen never receives OAuth
  tokens.
- OpenOpen never clones or migrates a Keychain credential into the new runtime
  account. The owner signs in through the pinned official Codex flow for that
  exact canonical runtime home; any password, passkey, biometric, or 2FA prompt
  remains an Owner-only official-UI boundary.
- Accept any ChatGPT plan and present only GPT models returned for the account.
- The ordinary product default is `Auto`, optimized for a quality/usage
  balance. The user may instead select any non-hidden model actually returned
  by the complete paginated `model/list` catalog. `planType` is display-only;
  it never grants capability that the catalog does not return.
- Rust resolves obvious deterministic work without a router call. When task
  classification is ambiguous, the router uses the catalog's unique non-hidden
  `isDefault` model at the lowest supported effort not below `low`. The strict
  result is exactly one of `repeatable extraction`, `everyday multi-step`, or
  `complex judgment`; it cannot select a model ID or effort.
- Execution mapping is deterministic and subscription-aware through the
  actual catalog: extraction uses the first eligible model in
  `gpt-5.6-luna → gpt-5.6-terra → gpt-5.6-sol` at `low`; everyday multi-step
  uses `gpt-5.6-terra → gpt-5.6-sol` at `medium`; complex judgment requires
  exact `gpt-5.6-sol` at `high` and has no fallback. A target effort may move
  only upward through `low → medium → high → xhigh → max` to the first effort
  the chosen model actually supports. There is no downward model or effort
  substitution.
- In those fixed orders, a model is eligible only when its exact ID exists in
  the complete paginated catalog, `hidden == false`, it supports the required
  input modality, and it supports either the target effort or the first higher
  effort in the fixed effort order. Catalog capability chooses the first model;
  quota and rate-limit state never reorder the list. After that model is
  selected, explicit exhaustion creates Need you and does not advance to the
  next model.
- A user-selected model is exact and never rerouted. An unrecognized future
  model is available only for manual selection until this contract classifies
  it; Auto never guesses its tier.
- When the pinned stable protocol exposes `account/rateLimits/read`, rate-limit
  data is a hard gate only. A value below 100% never changes routing. Explicit
  exhaustion, 100% with no credits, or zero remaining capacity creates Need
  you; the product does not move to another mapped model or silently change
  effort.
- Working state and Receipt record plan display value, catalog fingerprint,
  router model, task class, execution model, and actual reasoning effort.
  Model disappearance, quota exhaustion, a non-unique default router, or no
  suitable model creates Need you.
- Use exact `gpt-5.6-sol` with `high` reasoning for competition and release
  proof, independent of the ordinary Auto policy.
- Use `gpt-5.6-sol` with `high` reasoning for the Codex implementation goal and
  its isolated reviewers. The repository pins these defaults in
  `.codex/config.toml`; the background task also passes them explicitly.
- OpenAI's current model guide describes Sol as flagship, Terra as the
  intelligence/cost balance, and Luna as efficient high-volume work; this
  product mapping remains gated by the account's actual Codex catalog:
  https://developers.openai.com/api/docs/guides/latest-model

For untrusted receipt, chat, and Skill inputs, use a strict output schema,
isolated Mission cwd, no model-controlled network, no automatic approval, and
no external writes. The host refuses any filesystem request outside the
Mission workspace. Tool requests, schema failure, scope drift, prompt
injection, or canary access fail closed.

## Memory cold start and personal operating model

Memory exists to shorten time-to-value, not to let OpenOpen impersonate the
user or silently accumulate a dossier. The pipeline is fixed:

`Source Evidence → Memory Candidate → Confirmed Personal Operating Model → Approved Workflow`

### Quick Memory Passport

After ChatGPT sign-in, the Dashboard guide card offers an optional 60–90 second
handoff from ChatGPT or Claude:

1. OpenOpen opens the provider's official product and copies a bounded export
   prompt. The user pastes it and returns the result manually. OpenOpen does
   not automate Accessibility, scrape cookies, reuse a browser session, call a
   private history API, or claim that Codex login grants ChatGPT history.
2. The prompt asks for response preferences, relevant personal context,
   active projects/goals, recurring routines, tools, and corrections the user
   has taught the provider. It explicitly excludes passwords, tokens, recovery
   codes, security-question answers, and other authentication material.
3. The returned text enters one isolated import turn with no web, MCP, channel,
   filesystem, or external-effect tools and a strict output schema. A local
   secret-pattern pass runs before model input. URLs, tool requests,
   instructions embedded in source text, schema drift, and prompt injection
   fail closed.
   Before any Claude-derived text is sent to an OpenAI model, OpenOpen names
   the Claude source, the OpenAI destination/provider, data categories, model
   and effort, retention period, and local direct-connection path, then obtains
   a separate explicit cross-provider approval. Refusal permits local catalog
   and manual review only; it sends no Claude-derived body to OpenAI.
4. Results are only `MemoryCandidate` values, grouped as About me, How to work
   with me, Active projects, Recurring routines, and Corrections. Ordinary
   candidates can be reviewed in a batch; conflicts, low-confidence items, and
   private candidates are reviewed individually. Unconfirmed candidates expire
   after seven days.
5. Only user-confirmed candidates become durable encrypted records. Raw pasted
   handoff text is encrypted while pending and is deleted immediately after
   review, cancellation, or refusal. Crash leftovers have a hard one-hour TTL;
   startup purges them before any model or channel call. Only source provider,
   timestamp, and content hash remain for provenance.
6. The first confirmed Passport must immediately support one prepared Outcome
   using ordinary confirmed memory only. Before a Mission is confirmed, the
   prepared suggestion cannot retrieve or use Private Memory. The suggestion
   shows which confirmed memory categories informed it and lets the user
   correct or suppress them.

### Deep history import

Settings also supports official ChatGPT and Claude ZIP exports. It is not a
first-value blocker because provider export delivery can be delayed.

- Validate archive type and hashes in an isolated import workspace. Reject an
  archive over 1 GiB compressed, more than 25,000 entries, any entry over
  512 MiB decompressed, more than 4 GiB total decompressed data, a per-entry or
  aggregate compression ratio over 100:1, a path over 512 bytes, or directory
  depth over 16. Reject symlinks, special files, and nested archives. Parsing
  has a ten-minute wall-clock limit and 512 MiB RSS limit; any failure leaves
  no partial import.
- Keep the original archive at the user-selected location through a
  security-scoped bookmark plus hash; do not silently duplicate it. If it is
  moved or deleted, enter Need you.
- Catalog and locally index all supported conversation metadata/body first,
  then analyze recent, repeated, corrected, and active-project material before
  offering a visible `Go deeper` pass. Never send an entire large archive to a
  model by default.
- Sending Claude-derived history to an OpenAI model requires an explicit
  cross-provider disclosure and approval. Declining still permits local
  catalog-only import.
- Port only the minimal parser and adversarial-test semantics needed from
  `queelius/ctk` at `99784b7582a583fbae0725a5288797739dc347dd` (MIT)
  and `slyubarskiy/chatgpt-conversation-extractor` at
  `b7c4372b518a006df57415b0d4287fbbdf88ed29` (MIT); do not ship their Python
  runtimes. `OpenBMB/ClawXMemory` at
  `bcd66c5d8611413ad29354819b448e20dd51d480` (MIT) may inform bounded
  user-profile/project/feedback organization and recall tests only; do not ship
  its Node/OpenClaw/gateway or automatic rewrite runtime. Reverify every pin
  and record provenance/notices before porting any code or fixture.

### Memory authority and private recall

- Friday-derived states remain `Candidate → Confirmed | Rejected`; only a
  Confirmed MemoryRecord is automatically reusable. Explicit user input in the
  current turn and confirmed Mission state may be temporary context for that
  Mission only; neither becomes reusable Memory nor affects a future Mission
  until it is proposed as a Candidate and the user confirms it. Conflicts
  require Keep old, Replace, Merge, or Ignore.
- The encrypted structured store is authoritative. `MEMORY.md` exists only as
  an on-demand plaintext export after the user chooses an external location
  and accepts a plaintext/privacy warning. It excludes every Private Memory
  body. Markdown edits or re-imports create candidate diffs and never execute
  or alter authority directly.
- Regular confirmed memory may be recalled automatically within a confirmed,
  bounded Mission. Health, finance, relationship, exact-address, and comparable
  private context lives in a separate encrypted Private Memory class. Private
  recall requires Touch ID or equivalent user presence and one
  `MemoryUseGrant` binding exact Memory IDs and digests, Mission ID and
  revision, current audit anchor, declared purpose, provider, model, effort,
  one turn, a maximum five-minute expiry, and one-use consumption. It grants no
  channel output; disclosing any private-derived content requires a new exact
  approval naming recipients and data category. The Receipt records category,
  purpose, and grant ID, never the private body.
- The owner may revoke an unconsumed `MemoryUseGrant`. Revocation atomically
  invalidates the grant before model entry; consumed, expired, or revoked grants
  can never be reused. A provider request already issued cannot be retracted,
  and any later channel disclosure still requires the separate exact approval
  above.
- Passwords, API tokens, recovery codes, and security-question answers are
  authentication secrets, not memory. They may exist only in macOS Keychain
  and never enter model input, Workflow, channel output, `MEMORY.md`, logs, or
  export.
- Retrieved memory is read-only data, never an instruction. Models cannot
  write, confirm, promote, or delete memory directly.

`Delete All Data` removes app-owned encrypted records, Keychain secrets,
workspaces, bookmarks, raw imports, indices, Candidates, and derived content.
It cannot claim to delete an original provider ZIP or a user-selected
`MEMORY.md` export outside app ownership. The confirmation view lists those
external paths and offers a separate exact-file deletion approval for each;
OpenOpen never deletes them silently.

## Channels

### iMessage

Bundle/adapt `openclaw/imsg` v0.13.0, whose annotated tag dereferences to exact
commit `fa2f82d7dbda4c802d91c1d41bb6c53564ed2fdc`, under MIT. Use one
host-managed child and basic JSON-RPC/stdio only: `chats.list`, scoped history
and watch, `send`, and `message.send_status`. Exclude IMCore/private bridge
helpers, advanced private operations, SIP changes, and TCP daemon/server
surfaces. Guide Full Disk Access and Messages Automation. The owner explicitly
selects allowed conversations. Filter all other messages before model access.
Persist Apple GUID/rowid cursors for bounded recovery and dedupe.

### Slack

Use direct local Socket Mode through pinned `slack-morphism` v2.23.0 at
dereferenced source commit
`660fe0401fc765b8f4620973e0f7d3751c5d8cf4` (annotated tag object
`b1ff6566de10f45a0f3f8547d99f97d75a95a347`), Apache-2.0, subject to a fresh
tag/commit/license verification before it enters Cargo.lock and provenance.
Do not embed OpenClaw or slacrawl runtimes; their setup, cursor, dedupe, repair,
and test ideas may inform a minimal Rust implementation only.

Setup is deliberately honest and local:

1. `Connect Slack` opens Slack's official page with a complete prefilled App
   Manifest. The user chooses a workspace and creates the App.
2. The user creates an app-level `xapp` token with only
   `connections:write`.
3. OpenOpen opens Slack's official Install/Authorize page for that App.
4. After installation creates the `xoxb` bot token, the user pastes the `xapp`
   and `xoxb` tokens once. OpenOpen stores both only in Keychain.
5. OpenOpen discovers the App, workspace, user, and channels; pairs the owner
   and selected channels; and runs a real send/receive/permission doctor.

The Manifest enables Socket Mode and subscribes exactly to `app_mention`,
`message.channels`, and `message.groups`. A missing subscription, intent,
scope, explicit channel invitation, or real inbound/outbound probe fails setup
instead of presenting the connection as ready.

There is no OpenOpen OAuth broker, callback server, signing-secret flow, public
endpoint, user token, self-bot, or cloud relay. The bot scopes are limited to
`app_mentions:read`, `channels:history`, `channels:read`, `groups:history`,
`groups:read`, `chat:write`, and `users:read`; the app-level token has only
`connections:write`. The bot must be explicitly invited to a selected channel.
There are no DMs, files, search, auto-join, or broad user-token scopes.

### Consented opportunity observation

Build Week observation is limited to owner-selected Slack and iMessage
conversations and only to the repeated patterns needed by the heroes and
approved Workflows. It is not arbitrary surveillance.

- Every human participant consents individually. An owner cannot consent on
  another person's behalf. Observation begins only when every currently
  identifiable human participant has consented; a membership change pauses it
  until the new consent set is complete. Before consent, the participant's
  message body is neither persisted nor sent to a model.
- The owner first approves one plain-language consent notice for the selected
  conversation. It names the exact read scope, OpenAI processing, the derived
  private preview visible to the owner, the 24-hour raw-retention limit,
  revocation and deletion behavior, and the fact that a provider call already
  made cannot be retracted. A participant opts in or revokes from a stable
  provider identity using exact `OpenOpen yes` or `OpenOpen stop` after NFC
  normalization, surrounding-whitespace trim, and ASCII case folding. The
  consent record binds provider participant ID, conversation, disclosure
  version, decision, and time. Ambiguous or unavailable identity leaves that
  participant disabled; the owner cannot override it.
- Observation has a global switch plus a per-conversation switch. Revocation
  atomically stops queued and model work, deletes retained raw bodies, indices,
  summaries, previews, `MemoryCandidate`, `MemoryConflict`, confirmed
  `MemoryRecord`, and `WorkflowCandidate` rows derived from that participant;
  invalidates every unconsumed MemoryUseGrant that references the deleted
  records; and pauses dependent approved Workflows. A provider request already
  issued cannot be retracted. Immutable audit retains only a bodyless tombstone
  and digest.
- Consent, revocation, and accepted-message dedupe/queue state must commit
  durably before a Slack acknowledgement. Persistence failure sends no ACK.
  Consented raw message bodies expire within 24 hours; unconsented bodies are
  never retained or modeled.
- A high-value bounded event may produce one immediate private suggestion;
  otherwise events are consolidated locally. OpenOpen sends at most one
  unsolicited suggestion per approved conversation per 24 hours.
- Observation may prepare a private preview, such as a task draft, availability
  intersection, or expense summary. It cannot create a Mission, write
  Reminders, send a message, add a recipient, share data, or widen scope until
  the owner confirms the exact proposal.
- Discord remains explicit Mission/`@OpenOpen` input in V1 and is not an
  ambient observation source.

Every channel send uses a typed approval envelope binding recipients/channel,
message class, exact or bounded content, style, frequency, expiry, and
sensitive-data category. Any drift returns to Need you. OpenOpen always uses
its disclosed AI/App identity; it never silently speaks as the human.

### Discord

Use `serenity-rs/serenity` v0.12.5 at exact commit
`1809beb0fc24f3942c500058ad4fa47e6a97d3f9`, ISC, and the official Bot
Gateway only. Never automate a normal user token. The local three-step wizard
asks the user to create/enable the bot, paste the token once, and approve the
official install page. Store the token only in Keychain. Infer IDs, build
least-privilege install links, pair one owner and one approved channel, probe
permissions/intents/attachments, and prove real inbound, outbound, reconnect,
and restart traffic. V1 requires the paired owner, approved channel, and an
explicit `@OpenOpen` mention; bots are ignored and outbound `allowed_mentions`
prevents unintended mentions.

## Workflows and Skills

Two similar verified successes create a Workflow Candidate. Approval stores a
recipe; every invocation creates a new bounded Mission. There is no infinite
Mission and no silent automation expansion.

The user chooses one permission mode per Workflow: Only suggest, Ask before
acting (default), or Auto safe tasks. Auto applies only to the exact signed
Workflow definition and typed permission manifest; a new recipient, channel,
data class, message class, frequency, cost, write, or irreversible effect
returns to Need you. Natural-language rules compile to that typed manifest and
are never execution authority by themselves. Workflow discovery is limited to
the three heroes and repeated Evidence-complete behavior, not arbitrary
"anything useful" mining.

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

OpenOpen has no cloud API and no central telemetry. ChatGPT, Slack, Discord,
and GitHub connections originate locally. Secrets and encryption roots live in
Keychain. Sensitive bodies and memory use encrypted blobs; logs contain
redacted metadata. The app provides Export My Data and Delete All Data under
the app-owned versus external-file boundary defined above; it never reports a
user-owned provider archive or plaintext export deleted unless the user
separately approved that exact file deletion and the filesystem operation
succeeded.

Sleep, offline state, or runtime/channel crash persists Paused state and never
fabricates completion. Recovery uses bounded exponential backoff and durable
dedupe.

## Explicit exclusions

No Telegram, mobile app, OpenOpen cloud, shared Discord bot, unapproved or
hidden ambient surveillance, always-listening microphone, complete marketplace,
private iMessage API, SIP change, Slack user token/self-bot/OAuth broker,
browser-cookie or ChatGPT-history scraping, Accessibility-driven provider
automation, collection of authentication secrets as memory, payment, booking,
purchase, silent model fallback, silent Skill update, or demo production.

## Friday provenance

Source of truth: Friday origin/main commit
`4870f31fa088bef7eb9f4f256ec62993b02eda80`.

Only the Mission/WorkItem/Receipt state machine, evidence/Needs Me workflow
invariants, Skill/SkillCatalog/Trust/PathSafe concepts, and required
SQLite/audit/encrypted-blob gates may be adapted. Friday Hub, retired
TypeScript, providers, mobile apps, and old UI are excluded. BUILD_WEEK.md,
PROVENANCE.md, and THIRD_PARTY_NOTICES.md must stay current.

For the Discord channel boundary only, exact Friday pin
`4870f31fa088bef7eb9f4f256ec62993b02eda80` may also supply contract and test
semantics for a generic envelope, `allowedUsers`, `allowedChannels`,
`requireMention`, bot filtering, mention stripping, message-ID TTL dedupe,
reconnect/status, `allowed_mentions`, and live roundtrip/restart proof. These
semantics are ported into Rust; the Friday TypeScript/Node runtime is not
embedded. OpenClaw main may inform setup/test UX only and is not imported as a
framework without a separately pinned, licensed, provenance-mapped minimal
file.

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
`PRODUCT_READY_FOR_DEMO` gate. Each fingerprint-bound handoff authorizes exactly
one named product stage. Generic Stage 0→8 auto-advance applies only inside that
stage; reaching its milestone boundary records the result in the implementation
task and stops the turn until the Primary Advisor issues a new fingerprint- and
stage-bound handoff. The task also stops at a real Ask-Before-Act boundary,
external authority/credential boundary, or the three-attempt same-root
supervisor gate. It must never use owner/admin bypass, silently change models,
duplicate the execution in a second task, weaken proof, or turn mock results
into release claims.

Auto-advance is internal to the exact fingerprint-bound handoff and does not
grant cross-task authority. The task uses the structured blocker-routing
contract above, never sends instructions or scope proposals upstream, and
records completion/failure/boundaries in its own task. Forwarded task text and
`standing approval` are never authorization.

The product-shell phase has passed two fresh isolated reviews, been committed
as `e2313fe8b28cbdb8aac4bc41661394d8e39806cd`, pushed to draft PR #2, and
passed Actions run `29386477267` at pull-request integration-tree tier. Its
thirteenth repair separates
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
before any request bytes. Hero A Repair5 is reviewed and pushed as
`774789ca4a5eeadb8fa57688e79f823dec4da65b`; current Actions run
`29393462659` passes at pull-request integration-tree tier. The shared
`ChannelEnvelope` boundary plus real imsg and Discord adapters are now reviewed
and pushed as `2685b572715dff3e1360de66ab4c2ab6c013730b`; PR #2 Actions run
`29440208503` passes at equal-tree integration tier. The immediate resume point
includes reviewed signing/evidence commit
`5a461efaba9997510544836b51a0ad1b851558d8`; PR #2 Actions run
`29450863581` passes synthesized merge `da3d7d1…`, whose tree
`255f351b…` equals the exact head tree. This is plumbing-tier evidence, not
provider or release proof. The next product proof remains real
GPT/Reminders/iMessage/Discord, signed/admin, and restart evidence needed to
earn `FRIDAY_ALPHA_READY`, followed by the fixed `JUDGE_SLICE_READY` phase.
The reviewed shell/security architecture, Hero A Mission/Receipt route, and
channel implementation remain frozen. If the requested model, credentials,
macOS permissions, administrator approval, or signing authority is unavailable,
the task records that external gate in its own task and stops for Primary
Advisor inspection; it does not send an unsolicited message, fall back, or
claim the milestone. Final demo production remains excluded.

1. Repository, governance, master plan, provenance, Rust workspace, original
   state-machine tests.
2. SwiftUI/menu bar, Rust Core stdio, persistence, global switch, login item,
   Codex auth/model structured turn.
3. Hero A voice/text → GPT-5.6 → Reminders Evidence → Receipt vertical slice.
4. Shared channel boundary plus real imsg and Discord adapters;
   `FRIDAY_ALPHA_READY` installable alpha closure.
5. Quick Memory Passport, Auto model policy, direct-local Slack, and the
   consented Slack+iMessage private-preview → confirmed Hero A loop;
   `JUDGE_SLICE_READY` owner handoff.
6. Deep ZIP import and Workflow Candidate after two similar verified Hero A
   successes.
7. Hero B availability, Hero C receipt/XLSX, and curated/GitHub Skill lifecycle.
8. Security, stress, clean install, real-provider proof, external users,
   signing, notarization, and PRODUCT_READY_FOR_DEMO gate.

Each meaningful phase requires focused verification and two isolated read-only
reviewers. A same-root failure gets at most three consecutive implementation
attempts; a supervisor then decides whether work is stuck. Tests, acceptance,
or proof may never be weakened to escape the loop.

## Acceptance matrix

Automated coverage includes every legal/illegal Mission transition, Evidence
completion gate, expanded-scope approval, second-follow-up denial, app-server
contracts, untrusted-input containment, channel authorization/dedupe/recovery,
Slack manifest/setup/pairing/Socket Mode/scopes, participant consent and
revocation, membership-change pause, durable-before-ACK, pre-consent discard,
revocation deletion of confirmed participant-derived Memory and invalidation of
its unconsumed grants, 24-hour raw expiry, suggestion rate limit, Reminders
lifecycle, Quick Passport
same-provider and Claude-to-OpenAI refusal/approval/one-hour cleanup, ZIP
traversal/fixed-limit/cycle/parser/no-partial-import cases, secret redaction,
Candidate confirmation/conflict/expiry, Touch ID Private Memory grant binding,
one-use/expiry/channel-output denial and revocation, `MEMORY.md` plaintext
export warning/non-authority/external-delete boundary, Auto model catalog,
deterministic class mapping, manual-model exactness, effort-up-only,
rate-limit hard gate, unknown future model and quota failure, receipt
confidence/dedup/XLSX formulas, Workflow repeat gate,
Skill path/symlink/update/rollback, global Off, sleep/offline/crash, 100
shuffled duplicate envelopes, ten concurrent Missions, ten receipts, secret
scan, lint, diff check, code signing, notarization, and Gatekeeper.

Release proof must come from one SHA and one signed build, use exact
`gpt-5.6-sol` with `high` reasoning, contain nonzero all-passing scenarios, and
have an empty blocker list. It separately runs the complete Hero A chain
(explicit input → structured Outcome → owner-confirmed Mission → real
Reminders write/readback → Evidence → Receipt), Hero B chain (real iMessage and
Slack collection → structured replies → Rust intersection → at most one
follow-up → owner decision → published result → Receipt), and Hero C chain
(real iMessage/Discord image → validation/extraction → low-confidence review →
formula-correct XLSX → approved save/readback → Receipt). It also proves real
iMessage, Slack, and Discord bidirectional traffic, one reviewed Quick Memory
Passport from every provider publicly claimed, one Private Memory grant,
restart recovery without duplicate delivery, and Global Off preventing new
listener/model/outbound work. Mocks, fixtures, CI, screenshots, signatures, or
component-only runs never substitute for these end-to-end proofs.

`JUDGE_SLICE_READY` requires the focused owner-test route defined above, both
fresh reviewers, a same-SHA signed package, and an empty focused blocker list.
It may honestly remain unnotarized, has no public-release meaning, and cannot be
used to claim `PRODUCT_READY_FOR_DEMO`.

External validation requires one clean install and three unguided target users.
All three complete a first Outcome; at least two return within 48 hours. Failed
validation is reported and fixed, never rewritten.

In a preconfigured environment, sign-in through the first personalized real
Outcome targets 90 seconds. One clean install targets five minutes through
sign-in, just-in-time permissions, and voice/text → Reminders, excluding time
spent waiting for provider OAuth/2FA or a provider data-export email.

`PRODUCT_READY_FOR_DEMO` requires all automated and real E2E tests, both
reviewers, current-SHA proof, user metrics, signed/notarized clean install,
complete docs/notices/sample data, a clean worktree, and no hidden fallback,
mock-only route, secret, or unfinished claimed route.

## Implementation ledger

| Date | Decision or result | Evidence | Status |
| --- | --- | --- | --- |
| 2026-07-18 | Competition V1 is OpenAI-only; Claude and cross-provider import are excluded/not claimed | Direct Owner decision; current competition contract and synchronized current-scope documents | approved scope; historical Claude planning below the non-normative boundary remains superseded chronology |
| 2026-07-17 | Owner replaces the full production gate with `BUILD_WEEK_COMPETITION_READY` and freezes the adoption loop, approved-source learning/proactivity contract, Agent Understanding v1, complete ChatGPT+Claude Deep ZIP, instruction-only public GitHub Skills, Workflow Candidate, Owner design gate, and four-lane Sol high/max execution policy | Direct Owner decisions in Primary Advisor task; current competition contract at top of this document; cross-document audit of README, BUILD_WEEK, acceptance, release-proof, validation, provenance, and notices | approved scope; documentation fingerprint/reviews and Alpha baseline closure required before new parallel lanes |
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
| 2026-07-14 | Repair20 receives two fresh formal PASS reports on frozen fingerprint `29a00413…`: security Reviewer AC and governance Reviewer AD report no P0/P1/P2 findings | independent code, tests, staging, reviewer-accounting, provenance, live-remote, and no-overclaim review; both verify the fingerprint before/after | product-shell Stage 5 PASS; no signed/admin, cross-UID, provider, notarization, clean-install, product-E2E, external-user, or release proof is implied |
| 2026-07-14 | Reviewed product-shell commit `e2313fe8b28cbdb8aac4bc41661394d8e39806cd` was pushed, draft PR #2 opened, and Actions run `29386477267` passed every strict step | live PR/run/log/ref inspection; synthesized merge `487dae1…` and head `e2313fe…` share tree `2cae9eb…`; 134 ordinary Rust and 40+27 Swift tests plus release/lint/format/plist/script/clean-diff checks pass | Stage 6 and PR integration-tree Stage 7 plumbing PASS; PR remains draft/unmerged; exact-head, signed/admin, cross-UID, real-provider, notarization, clean-install, product-E2E, external-user, and release proof remain pending |
| 2026-07-14 | Hero A implementation candidate now connects explicit text input to the pinned real GPT-5.6 structured Outcome route, typed Mission confirmation, real EventKit Reminders creation and exact identifier readback, signed Reminder-completion Evidence, and an Evidence-backed Receipt in the simple UI | 139 ordinary Rust tests and 40 broker/signing plus 30 App Swift tests; workspace release/strict lint/format/plist/script/diff/credential checks; `/private/tmp/OpenOpen-Stage-HeroA.app` reports `STAGED_AD_HOC_NOT_RELEASE_PROOF` and passes deep ad-hoc identity/hash verification | focused and full local implementation verification PASS; superseded by the first Hero A closure review; no real ChatGPT output, user Reminders mutation/readback, signed/admin, cross-UID, current GitHub CI, notarization, clean-install, product-E2E, external-user, or release proof is claimed |
| 2026-07-14 | First Hero A closure reviewers both FAIL frozen fingerprint `1711864f…`: composite confirmation/completion could strand partial typed-command state or lose a committed Receipt across Core termination/response loss; the App could reuse a completed Mission for a second Outcome; the EventKit write lacked the exact `NewExternalWrite` approval required by `ActionGate`; owner approval time was copied from model suggestion time; disclosure undercounted Rust tests by one | two fresh isolated reports; functional reran 18+3 Host and 30 App tests, governance reran the full 139-Rust/70-Swift suite and staging/remote checks; both verified the same fingerprint before/after | Hero A closure FAIL; repair is limited to atomic typed-command batches, durable replay/dashboard recovery, exact owner-approved Reminder authorization, real click time, and multi-Mission isolation |
| 2026-07-14 | Hero A closure repair moves each composite confirmation or completion into one Store-owned typed-command transaction; exact committed retries survive Host restart and changed retries fail without audit movement; Dashboard restores the newest authorized Active Mission and latest Receipt; the explicit confirmation click records its observed time and owner-approves an exact `ReminderWrite`/`NewExternalWrite` payload before EventKit can run; completed local state cannot be reused by the next Outcome | 143 ordinary Rust tests and 40 broker/signing plus 35 App Swift tests; full workspace test/release/strict Clippy/format checks; shared Rust/Swift payload hash vector `68b51a9f…`; focused rollback, restart, dashboard, invalid-authorization-no-write, sequential-Mission, and exact Rust JSON decode tests; `/private/tmp/OpenOpen-Stage-HeroA-Repair1.app` passes deep ad-hoc identity/hash staging and is explicitly not release proof | full local repair/staging verification PASS; two fresh isolated replacement reviewers pending; no real provider/Reminders, signed/admin, cross-UID, CI, notarization, product-E2E, external-user, or release proof is claimed |
| 2026-07-14 | Hero A Repair1 replacement review: fresh functional Reviewer F FAILS frozen fingerprint `3e839145…` because the logical Reminder approval could resolve to another physical EventKit calendar after restart/default-account or list-name drift; fresh governance Reviewer E PASSES that same tree, but its PASS is not reusable after the boundary repair | both isolated reviewers rerun 143 ordinary Rust and 40+35 Swift tests and verify the same fingerprint before/after; functional review traces the duplicate-mirror route through dashboard link loss and default-source list resolution | Repair1 is superseded; repair remains limited to exact physical EventKit target authorization and recovery |
| 2026-07-14 | Hero A Repair2 resolves the EventKit target read-only before confirmation, persists the exact source/calendar descriptor in the audited `NewExternalWrite` approval, and hashes it with the Mission and ordered work items. Changed-target confirmation retries fail without audit movement; EventKit writes/recovery remain on the approved target, recover exact markers after rename, and fail closed on missing or ambiguous calendars | 144 ordinary Rust tests and 40 broker/signing plus 37 App Swift tests; full workspace test/release/strict Clippy/format/plist/script/diff checks; shared Rust/Swift V2 vector `188605fc…`; deterministic changed-target, default-account drift, renamed-list, ambiguity, dashboard, and exact JSON tests; `/private/tmp/OpenOpen-Stage-HeroA-Repair2.app` passes deep ad-hoc identity/hash staging and is explicitly not release proof | full local Repair2 verification/staging PASS; two fresh isolated replacement reviewers pending; no real provider/Reminders, signed/admin, cross-UID, CI, notarization, product-E2E, external-user, or release proof is claimed |
| 2026-07-14 | Hero A Repair2 replacement review: both fresh reviewers FAIL frozen fingerprint `76ca9834…`. Functional review proves that deleting or moving every Mission marker before restart leaves zero recovery markers, which Repair2 treated as first write and duplicated. Governance review proves an initially absent calendar identifier could later attach the approval to a newly appearing same-name list, and cancellation could resume across awaited discovery into calendar persistence | two isolated reports on one unchanged fingerprint; both independently rerun the 144-Rust/77-Swift suites and verify the fingerprint before/after | Repair2 is superseded; repair remains limited to exact physical target selection, one-time write authority, durable exact-link recovery, and cancellation before every effect boundary |
| 2026-07-14 | Hero A Repair3 requires one pre-existing uniquely selected physical OpenOpen Reminders list and never creates a calendar. The original confirmation response alone carries `createOnce`; every dashboard/restart/retry response carries `recoverOnly`. After exact EventKit readback, Core atomically records one signed `ReminderMirrored` Evidence link per WorkItem; persisted links restore without EventKit creation, while zero or partial markers without persisted links fail closed. Cancellation is checked after every awaited boundary and before/after the reminder commit/readback path | 145 ordinary Rust tests pass with one exact-runtime test ignored unless its pinned binary is supplied; 40 broker/signing plus 40 App Swift tests pass; release builds, strict Clippy, Rust/Swift format, plist/script, and diff checks pass. Deterministic tests cover no-list confirmation failure, renamed/ambiguous exact targets, response-loss `createOnce`→`recoverOnly`, durable exact-link idempotency, zero-marker no-recreate, and persisted-link restart with zero external writes. `/private/tmp/OpenOpen-Stage-HeroA-Repair3.app` passes deep exact-identity and pinned-hash ad-hoc verification with `STAGED_AD_HOC_NOT_RELEASE_PROOF` | full local Repair3 verification/staging PASS; two entirely fresh isolated replacement reviewers remain required; no real provider/Reminders, signed/admin, cross-UID, current CI, notarization, clean-install, product-E2E, external-user, or release proof is claimed |
| 2026-07-14 | Hero A Repair3 replacement review: both fresh reviewers FAIL frozen fingerprint `fa9d905ec85907719c98c4f968fff497261677a2e175e6631b6f34ccebad1417`. EventKit could commit and then lose readback or cross Off while the App still retained volatile `createOnce`; deletion, movement, or mutation of every marker could therefore make a later retry issue a second batch | two isolated reports on one unchanged fingerprint; both trace the same post-commit/retry route | Repair3 is superseded; its green local suite and ad-hoc stage are not closure evidence |
| 2026-07-14 | The three Hero A repairs shared the same missing durable at-most-once dispatch invariant. Supervisor verdict is `STUCK: same_root_cause`; the owner had already approved every recommended fix preserving direction, selecting strict at-most-once dispatch | supervisor diagnosis plus owner instructions | Repair4 authorized without changing the final gate or claiming EventKit commit certainty |
| 2026-07-14 | Hero A Repair4 adds `mission.reminders.begin`: before EventKit, Core atomically attaches signed deterministic `ReminderDispatchStarted` Evidence for every WorkItem. Only the first durable start returns `executeNow=true`; response loss, restart, Off, precommit failure, post-commit readback failure, and every later call return recovery-only. EventKit marker v2 and `ReminderMirrored` Evidence bind the exact dispatch token; missing, moved, mutated, partial, or ambiguous state never authorizes a second batch | 146 ordinary Rust tests pass with one environment-gated runtime test skipped in the ordinary run; 40 broker/signing plus 42 App Swift tests pass; release builds, strict Clippy/warnings/format, plist/script/diff checks, and two explicit pinned-runtime sandbox diagnostics pass. `/private/tmp/OpenOpen-Stage-HeroA-Repair4.app` reports `STAGED_AD_HOC_NOT_RELEASE_PROOF` and passes exact identity/pinned-hash staging | full local Repair4 verification/staging PASS; two entirely fresh isolated replacement reviewers remain required; no real provider/Reminders, signed/admin, cross-UID, current CI, notarization, clean-install, product-E2E, external-user, or release proof is claimed |
| 2026-07-14 | Hero A Repair4 replacement review: functional Reviewer A PASSES frozen fingerprint `4cabaeb4…`; governance Reviewer B FAILS the same tree because the lower-level public EventKit writer accepted only a reusable `ConfirmedMission`, not the one-shot start decision. Retaining the first Mission, deleting all markers, and calling that API again could issue a second batch | both reviewers independently rerun the full 146-Rust/82-Swift suite, staging, and fingerprint checks | Repair4 is superseded; the functional PASS is historical only after the safety edit |
| 2026-07-14 | Hero A Repair5 removes the public raw writer route. The internal writer accepts the complete `ReminderDispatchStart` and consumes its Mission claim before permission requests, marker discovery, or EventKit. The same-process retained-start replay fails before any external boundary; restart still obtains only `executeNow=false` from durable Core state and enters read-only recovery | 146 ordinary Rust tests pass with one environment-gated runtime test skipped in the ordinary run; 40 broker/signing plus 43 App Swift tests pass, including direct retained-start replay; release builds, strict Clippy/warnings/format, plist/script/diff checks, and two explicit pinned-runtime sandbox diagnostics pass. `/private/tmp/OpenOpen-Stage-HeroA-Repair5.app` reports `STAGED_AD_HOC_NOT_RELEASE_PROOF` and passes exact identity/pinned-hash staging | full local Repair5 verification/staging PASS; two entirely fresh isolated replacement reviewers remain required; no real provider/Reminders, signed/admin, cross-UID, current CI, notarization, clean-install, product-E2E, external-user, or release proof is claimed |
| 2026-07-14 | Hero A Repair5 receives two entirely fresh isolated PASS reports on frozen fingerprint `4b41a04f7b28573e1a04cb19c79f499b497a2240efbcc236f003f4feb97971cf`: functional and governance reviewers report zero P0/P1/P2 findings | both independently trace every EventKit writer/client construction path, durable dispatch/restart/Off and exact target/token/link/Evidence binding; each reruns the complete 146 ordinary Rust and 40+43 Swift suites, strict checks, ad-hoc stage verification, two pinned-runtime diagnostics, and verifies the same fingerprint before/after | Hero A Stage 5 PASS; reviewed commit/push and current Actions remain pending. No real ChatGPT/Reminders, signed/admin, cross-UID, notarized, clean-install, product-E2E, external-user, or release proof is implied |
| 2026-07-14 | Reviewed Hero A commit `774789ca4a5eeadb8fa57688e79f823dec4da65b` was pushed to draft PR #2 and current Actions run `29393462659` passed every strict step | live GitHub run/ref/log inspection; synthesized merge `bccdf360…` and head `774789c…` share tree `e8f3605…`; 146 ordinary Rust and 40+43 Swift tests plus release/lint/format/plist/script/clean-diff checks pass | Hero A Stage 6 and PR integration-tree Stage 7 plumbing PASS; no real ChatGPT/Reminders, exact-head, signed/admin, cross-UID, notarized, clean-install, product-E2E, external-user, or release proof is implied |
| 2026-07-14 | Owner approves accelerated intermediate milestone `FRIDAY_ALPHA_READY`: Hero A plus real bidirectional iMessage and Discord for the same bounded Mission loop, targeted for July 16–17, 2026 America/Los_Angeles | owner decision; verified imsg v0.13.0 annotated tag→`fa2f82d…` MIT, serenity v0.12.5→`1809beb…` ISC, and Friday adapter source pin `4870f31…` MIT | current phase; Heroes B/C move after the alpha but remain required for `PRODUCT_READY_FOR_DEMO`; final gate unchanged |
| 2026-07-15 | Friday channel implementation checkpoint adds the shared command-owned ChannelEnvelope/pairing/dedupe/cursor/model/outbound boundary, exact serenity Bot Gateway/HTTP adapter, one-child basic-stdio imsg adapter, Swift Keychain/Connections UI, and receipt-bound imsg build/staging | complete current-tree verification passes 175 Rust tests with one explicit environment-gated runtime test plus 87 Swift tests, release/strict lint/format/plist/script/diff checks, and two pinned imsg boundary tests; `/private/tmp/OpenOpen-FridayAlpha-Final.app` reports `STAGED_AD_HOC_NOT_RELEASE_PROOF`; its ad-hoc unnotarized DMG passes read-only mount/copy/signature install testing at SHA-256 `0f9b7fd3…` | local implementation/package verification PASS; two fresh closure reviewers and real GPT/Reminders/iMessage/Discord proof remain pending; `FRIDAY_ALPHA_READY` is not yet earned and no release proof is claimed |
| 2026-07-15 | First Friday-alpha closure review: both fresh reviewers FAIL unchanged fingerprint `136a42ba…`. They reproduce a cursor-advance crash window before durable model enqueue, unreachable Need-you/Receipt return routes, manual-ID Discord setup without install/permission/intent probes, pre-sign-only imsg identity with symlink/replaceable runtime, compiled private IMCore/SIP/bridge code, literal/stale status UI, a public post-genesis Mission-origin binder, and incomplete transitive notices | two isolated reports; each reruns the complete 175-Rust/87-Swift verification and preserves the fingerprint. A follow-up packaging audit also finds the imsg binary references a deleted-build-root `PhoneNumberKit_PhoneNumberKit.bundle` that the App/DMG does not contain | Friday-alpha closure FAIL; the prior DMG remains historical ad-hoc mount/signature evidence only and is not a runnable alpha candidate. Repair attempt 1 is in progress; no push or provider/release claim |
| 2026-07-15 | Friday-alpha Repair1 local candidate atomically commits accepted observation/cursor/queued model work; restricts channel origin to `CreateMission` genesis; closes exact Need-you/Receipt authorization/readback; splits connection/event status; implements Discord's token-derived install/pair/probe/confirm wizard; prepares imsg without RPC bytes until exact running identity validation; compiles only whitelisted imsg sources; ships the PhoneNumberKit tree; and generates the complete content-addressed notice closure | 186 Rust tests pass with one explicit environment-gated Codex runtime test; 40 broker/signing plus 49 App Swift tests pass; strict release/Clippy/format/diff/plist/script/notices checks pass. `/private/tmp/OpenOpen-FridayAlpha-Repair1-Final.app` contains exactly four Codex runtime files, signed imsg build/runtime receipts, resource tree `7a5cb869…`, 597 notice texts, and passes deep ad-hoc staging plus staged basic RPC. Its ad-hoc, unnotarized DMG passes read-only mount/copy/signature install testing at SHA-256 `04f02c846f…` | full local closure verification PASS; two fresh replacement reviewers pending. The stage says `STAGED_AD_HOC_NOT_RELEASE_PROOF`, has Team `not set`, and is not a Developer-ID runnable alpha. No push, provider proof, or `FRIDAY_ALPHA_READY` claim |
| 2026-07-15 | Friday-alpha Repair1 replacement review: both entirely fresh reviewers FAIL unchanged fingerprint `10160bb13293036008479241224cc2f34c842bd5433c5c44468346ef4ca7d01d`. Host handed already-prefixed approved iMessage wire text to the single-prefix adapter, so live sends failed before RPC; the patched send response returned no real provider identity; failed iMessage activation and repeated Discord setup could leave a prepared session wedged; and the ledger said reviewer completion before review | two isolated reports on one unchanged fingerprint; both trace the exact Host→adapter/Swift lifecycle routes and preserve the reviewed tree | Repair1 is superseded; repair is limited to single prefix ownership, real provider GUID plus no-resend recovery, prepared-session cleanup, deterministic setup restart, and honest ledger wording |
| 2026-07-15 | Friday-alpha Repair2 preserves the Store-approved final iMessage wire bytes but strips exactly one authorized prefix at the Host/adapter boundary. The pinned basic sender records a pre-send row high-water, sends exactly once, and returns identity only for one exact same-chat/text local row with a real GUID; zero/ambiguous matches remain uncertain, while restart recovery is bounded read-only and never resends. Swift cleans prepared iMessage state after activation/proof failure and stops prior Discord setup before restart | exact patch applies with 449-line server and 88-line test additions; four pinned upstream imsg tests, 187 ordinary Rust tests with one explicit environment-gated runtime test, and 40 broker/signing plus 51 App tests pass. Release/strict Clippy/Rust+Swift format/plist/script/notices/diff checks pass. One old Host cancellation timing assertion failed under the first parallel load, then passed 20/20 isolated repetitions and the complete exact suite rerun. `/private/tmp/OpenOpen-FridayAlpha-Repair2-Final.app` passes deep ad-hoc staging/basic RPC; its DMG passes read-only mount/copy/signature install testing at SHA-256 `15c1429b…` | full local Repair2 verification/staging PASS; two entirely fresh replacement reviewers pending. The stage remains `STAGED_AD_HOC_NOT_RELEASE_PROOF` with Team `not set`; no push, real provider proof, Developer-ID/notarization, or `FRIDAY_ALPHA_READY` claim |
| 2026-07-15 | Friday-alpha Repair2 replacement review: both entirely fresh reviewers FAIL unchanged fingerprint `1a983c72ad9f70e7cd321c9782e4e127e42e006ba190daec5f76947831064494`. Functional review proves a prior Mission's same-text iMessage GUID could be promoted from history and misbound as the current outbound's delivery. Governance review proves prepare response loss could retain a child, the product lacked a nontechnical `chats.list` selection route, and the ledger top summary was stale | two isolated reports on one unchanged fingerprint; each reruns the complete 187-Rust/91-Swift suite and independently checks the pinned patch/App/DMG evidence | Repair2 is superseded; no push or provider/release claim. Repair remains limited to history-never-Sent, complete send observation, deterministic child cleanup, two-proof signed discovery, explicit chat/participant selection, and honest ledger state |
| 2026-07-15 | Friday-alpha Repair3 makes every iMessage history recovery outcome `Uncertain`, so only the exact synchronous send RPC can bind a provider GUID; the pinned sender sends once and accumulates candidates for the full two-second window. A separate prepare/validate/list discovery child uses two fresh proofs, sends no RPC bytes before exact running Mach validation, returns only bounded exact-iMessage chats, and is cleared on success, failure, stop, or Off. Swift pre-stops before every connection attempt, cleans all failed prepares, and presents conversation/participant pickers | clean pinned patch apply with 55/12/472/127 hunk counts; five upstream OpenOpen tests; 190 ordinary Rust tests with one explicit environment-gated Codex test; 40 broker/signing plus 53 App Swift tests; release/strict Clippy/Rust+Swift format/plist/script/diff checks; two explicit pinned Codex diagnostics; independent notices check reports 190 OpenOpen, 924 Codex, 1888 documents, 597 texts. Fresh v4 imsg binary SHA `635c9981…`, build receipt `c1769b40…`, resource tree `7a5cb869…` | complete local code/dependency verification PASS; closure review and package facts continue below. No push, real GPT/Reminders/iMessage/Discord traffic, Developer-ID/notarization, or `FRIDAY_ALPHA_READY` claim |
| 2026-07-15 | First Repair3 closure review: fresh functional Reviewer PASS, fresh governance Reviewer FAIL on unchanged fingerprint `11d34c594ec1f1f2988d763a25a76244f477cc854254b390d20db5b88290499a`. Governance finds one P2: the Discord provenance paragraph still called historical Repair2 187-Rust/91-Swift data “current,” contradicting the Repair3 190-Rust/93-Swift ledger; no product, security, provider, or packaging boundary finding | functional reruns full Rust/focused Swift/pinned patch/notices/package checks; governance reruns full Swift/focused Rust/static/package/remote checks; both preserve the fingerprint | the functional PASS is historical after the evidence edit. Fix is limited to labeling Repair2 historical and Repair3 current, followed by a fresh two-reviewer cycle |
| 2026-07-15 | Repair3 evidence-only fix labels the Discord Repair2 verification historical and the 190-Rust/93-Swift Repair3 tree current; no product code, gate, or provider claim changes. A replacement package embeds the corrected provenance | `/private/tmp/OpenOpen-FridayAlpha-Repair3-Final2.app` passes deep ad-hoc verification and staged RPC with the same signed imsg/runtime receipts; its read-only mount/copy/signature-tested DMG SHA is `bff4d18b…`; Team remains `not set` | evidence/package repair PASS locally; two entirely fresh replacement reviewers pending. Package remains `STAGED_AD_HOC_NOT_RELEASE_PROOF`; no push or `FRIDAY_ALPHA_READY` claim |
| 2026-07-15 | Repair3 evidence-fix replacement review: two entirely fresh isolated reviewers PASS unchanged fingerprint `3e2015475d98b74d88a3de4c36e3a1aa4e8bcd1659a3356c5f36f7bd68103ae3` with zero P0/P1/P2 | both verify the prior P2 is closed, current/historical evidence is consistent, Final2 embeds byte-identical provenance, remote still points to historical `774789c…`, and focused Host/imsg/Swift/pinned-patch/static/package checks pass | Friday-alpha Repair3 Stage 5 PASS; exact commit/push and current-SHA CI pending. Real provider, Developer-ID/notarization, admin/cross-UID, and `FRIDAY_ALPHA_READY` remain pending |
| 2026-07-15 | Reviewed Friday-alpha implementation commit `2685b572715dff3e1360de66ab4c2ab6c013730b` was pushed to draft PR #2 and Actions run `29440208503` passed every strict workflow step | live ref/PR/run/job inspection; synthesized merge `99ee2b10efb388ea6bb61ee88afe3092f2301a71` and exact head share tree `730bce09952c5c63374ffef7b3578aa723294323`; the run record names head `2685b57…`, and the integration job passes Rust/Swift test, release, lint, format, metadata, script, and clean-diff steps | Friday-alpha Repair3 Stage 6 and PR integration-tree Stage 7 plumbing PASS. PR #2 remains draft/unmerged; real GPT/Reminders/iMessage/Discord traffic, Developer-ID/notarization, administrator/cross-UID proof, and `FRIDAY_ALPHA_READY` remain pending |
| 2026-07-15 | Evidence follow-up `becea456a426a76a74428ed46a20311f2986d219` passed its own PR Actions run `29442001103`; PR #2 metadata now names Hero A+iMessage+Discord and preserves every proof exclusion | live run/job/ref inspection; synthesized merge `2b80e2c50878eb83377b08ef8e8da595570c6648` and exact head share tree `bb84694e4bf07da059d9ab60c3866e4e3f57e757`; all strict workflow steps pass | evidence Stage 6/7 follow-up PASS at integration-tree tier. Draft/unmerged status and all provider/release gates remain unchanged |
| 2026-07-15 | Local signing slice adds explicit Developer-ID mode without fallback, hardened runtime and secure timestamps for every distributed Mach-O, an exact Apple Events entitlement plus purpose string for the pinned imsg sender, optional same-Team DMG signing, and Apple-anchor/get-task-allow verification. The first real attempt failed closed because the entitlement verifier treated the dotted key as a plist path. A subsequent all-Mach-O scan found pinned Codex `rg` was linker-ad-hoc; the final path verifies its upstream hash, re-signs only that same Mach-O for notarization, and persists upstream/runtime hashes plus Team/CDHash while leaving the OpenAI-signed Codex/code-mode-host unchanged | historical `/private/tmp/OpenOpen-FridayAlpha-DeveloperID-v3.app` had Team `UHDY2275L5` and DMG SHA `0d51c849…`; its structural checks and 40+53 Swift tests passed before review | superseded by the first signing reviewer FAIL; v3 is not an alpha candidate or signing PASS |
| 2026-07-15 | First signing-slice governance reviewer rejects frozen fingerprint `eaa4bc2e…`: staging trusted caller-authored imsg receipt data, DMG creation accepted a dynamic same-Team app without exact bundle/nested-code/receipt binding, and README omitted mandatory imsg inputs. The functional reviewer was interrupted after the tree became obsolete and is not counted | isolated review plus local reproduction; repair pins the exact imsg build receipt/bytes/size/patch/runtime/resource/source manifests and runtime allowlist, and requires exact OpenOpen bundle, eight Mach-O identities, Teams, Apple anchors, hardened runtime/timestamps, entitlement split, receipts, upstream hashes, and frozen CDHashes before DMG creation | repair in progress; replacement package and two entirely fresh reviewers required. No notarization, provider, admin/cross-UID, `FRIDAY_ALPHA_READY`, or release proof is claimed |
| 2026-07-15 | Signing repair v4 pins exact imsg inputs and complete App file/Mach-O/signature/receipt content before DMG creation | local focused negatives and reproducible rebuild passed; reference DMG SHA `feec94d3…` | superseded by the v4 reviewer FAIL below; this is historical local evidence only |
| 2026-07-15 | Both fresh v4 reviewers reject unchanged fingerprint `08a58745c03de8195c06376e245d7791d0b49e2700e0223de817e7bf8a478b41` | functional review reproduces a non-executable main binary accepted into a success-labeled DMG; governance review additionally finds no exact owner leaf-certificate binding and incomplete post-sign/final-copy unsigned-content binding | v4 FAIL and historical; no signing PASS, commit, push, or release claim |
| 2026-07-15 | Replacement signing repair pins exact owner Developer-ID leaf SHA-256 `a7e43925…`, normalizes and binds the complete directory/file/type/mode contract, and verifies every owned Mach-O before and after signing plus at final output. The scripts explicitly do not claim isolation from a process already authorized to invoke `codesign` with the owner key | `/private/tmp/OpenOpen-FridayAlpha-DeveloperID-v5-final-review.app` embeds provenance SHA `4b584d99…`; all owner code and signed DMG use the exact pinned leaf; 18 directories are `0755`, eight Mach-O files are `0755`, and 609 other files are `0644`; main/resource/directory modes, extra directory, wrong identity, ACL, BSD flags, and behavior-changing extended attributes fail before output; ad-hoc regression passes; full 190 ordinary Rust and 40+53 Swift tests plus release/strict lint/format/plist/script/diff/credential checks pass; signed DMG SHA `494caddf…` | complete local replacement verification PASS; two entirely fresh reviewers pending. Gatekeeper correctly reports `Unnotarized Developer ID`; no notarization, provider, admin/cross-UID, `FRIDAY_ALPHA_READY`, or release proof is claimed |
| 2026-07-15 | Signing v5 first closure review: fresh functional and governance reviewers both PASS unchanged frozen fingerprint `fdf5a00e8c0c4ca92ab4ff8e9cf33041c801bac5e74304bc484c613c35e33235` with zero P0/P1/P2 | both independently verify the exact owner leaf, pre/post/final unsigned content, complete layout/type/mode/operational metadata, entitlements, receipts, mounted/copied App, DMG SHA `494caddf…`, expected Gatekeeper rejection, live remote facts, and no-overclaim boundary | first v5 Stage 5 PASS. The evidence/provenance-bound replacement tree requires its own two fresh PASS reports before exact commit/push; no notarization, provider, admin/cross-UID, `FRIDAY_ALPHA_READY`, or release proof is implied |
| 2026-07-15 | The evidence-bound v5 candidate rebuild changes no product code and embeds byte-identical current provenance SHA `315deb30…` | `/private/tmp/OpenOpen-FridayAlpha-DeveloperID-v5-evidence-final.app` passes the same exact stage contract; its mounted/copied and integrity-checked signed DMG SHA is `b7f3e718…` | local replacement package verification PASS; two fresh unchanged-fingerprint reviewers are required before commit. It remains unnotarized and is not provider, admin/cross-UID, `FRIDAY_ALPHA_READY`, or release proof |
| 2026-07-15 | First final-evidence v5 review is rejected on one governance P2: embedded provenance still says “not yet reviewed,” so the statement becomes false when the required external review completes. The peer reviewer is interrupted when the fingerprint becomes obsolete and is not counted | governance Reviewer B otherwise verifies exact leaf/content/layout/modes/signatures/mount/Gatekeeper/remote facts on fingerprint `c9505bed…`; DMG SHA `b7f3e718…` | narrow evidence-only repair removes dynamic review status from embedded provenance and keeps every notarization/provider/admin/release exclusion; rebuild and two entirely fresh reviewers required |
| 2026-07-15 | Final-evidence v5 Repair1 removes the dynamic review-status sentence from embedded provenance and records that reviewer status lives in external task/PR evidence | `/private/tmp/OpenOpen-FridayAlpha-DeveloperID-v5-evidence-final2.app` embeds provenance SHA `155aa65a…`, passes the unchanged exact stage contract, and produces mounted/copied exact-verifier DMG SHA `7c022b83…` | local evidence-only repair verification PASS; two entirely fresh unchanged-fingerprint reviewers required. No product code or proof gate changed; package remains unnotarized and is not release proof |
| 2026-07-15 | Final2 replacement review and Stage 6/7: two entirely fresh isolated reviewers PASS unchanged fingerprint `026b2b1f…` with zero P0/P1/P2; the reviewed signing/evidence tree is committed and pushed as `5a461efaba9997510544836b51a0ad1b851558d8` | draft PR #2 Actions run `29450863581` completes SUCCESS; job `87472696571` checks synthesized merge `da3d7d1…`, and its tree `255f351b…` equals the exact head tree; logs pass 190 Rust tests with one explicit environment-gated ignore, 40 broker/signing Swift and 53 App Swift tests, release, strict lint/format, metadata/script, and clean-diff checks | signing/evidence review, branch push, and integration-tree plumbing PASS. PR remains draft/open; the package remains unnotarized and is not provider, admin/cross-UID, `FRIDAY_ALPHA_READY`, or release proof |
| 2026-07-15 | Owner locks the next product sequence: preserve the current Hero A+iMessage+Discord alpha, then deliver Quick Memory Passport + Auto model routing + direct-local Slack + consented Slack/iMessage private preview as `JUDGE_SLICE_READY`; Deep ZIP import follows without blocking first value. Private context is model-usable only through a per-Mission user-presence grant; authentication secrets remain Keychain-only. UI/Figma is a later focused pass | owner decisions in the source task; official provider capability/export review; Friday memory-state audit; pinned OSS feasibility review recorded in the planning conversation | approved canonical product contract for the next phase. This row records direction only and claims no implementation, provider, package, or release proof |
| 2026-07-15 | First installed Developer-ID owner-test exposed two launch blockers without widening scope: direct distribution could not use the data-protection Keychain selector, then the App waited for EOF while reading short replies from the intentionally persistent Core process. Commit `600d1e1…` fixes the single explicit login-Keychain backend; `121e73f…` consumes pipe bytes as soon as readable and adds persistent-child plus explicit real-Core protocol regressions | 190 ordinary Rust tests, 41 broker/signing and 55 App Swift tests, release builds, strict Clippy/warnings/format/plist checks, the explicit isolated real signed-Core round trip, and Developer-ID staging all pass. Exact-clean-commit local DMG SHA-256 is `58b07dbc…`; it is explicitly unnotarized and not release proof | focused repair verification, commit, push, and owner-test package PASS only; current PR CI and final review remain pending. Installed proof confirms Core replies now arrive, but the first broker registration remains incomplete; no administrator/cross-UID, real GPT/Reminders/iMessage/Discord, integrated Mission, reviewer, `FRIDAY_ALPHA_READY`, or release proof is claimed |
| 2026-07-15 | The repaired installed App reaches ServiceManagement, whose live log resolves the signed `BundleProgram` but reports no BackgroundTaskManagement record and status `notFound`. Commit `ecf232b…` routes both `notRegistered` and this first-install `notFound` state through the same official `SMAppService.register()` call; enabled or approval-pending services are never re-registered, and no `launchctl` fallback exists | focused state tests plus the complete 43 broker/signing and 55 App Swift suite, release, strict warnings/format/plist checks, exact Developer-ID staging, and exact-commit DMG SHA `816fe86d…` pass. PR run `29462137565` succeeds; the owner-approved installed route creates the all-users BTM record and submits the system daemon | focused registration repair, push, CI, and real administrator approval PASS. The first approved daemon starts but exits before protected worker installation; provider/integrated-Mission/reviewer/Friday milestone and release proof remain absent |
| 2026-07-15 | Real launchd execution exposes that `BundleProgram` supplies a relative `argv[0]`; the broker incorrectly used that caller-controlled string to find its sibling worker and failed closed before creating protected state. Commit `1a31abc…` instead derives its own absolute executable path from the kernel process identity, requires the exact symlink-free `.app/Contents/MacOS/OpenOpenEffectBroker` layout, and only then selects the signed sibling worker | focused kernel-path/layout regression plus all 44 broker/signing and 55 App Swift tests, release, strict warnings/format/plist checks, exact Developer-ID staging, and local DMG SHA `80358f54…` pass. The owner-approved installed same-commit service remains running as root and creates the root-owned mode-`0700` protected directory without a manual bootstrap; PR run `29462729644` succeeds | focused implementation/push/CI and installed root-daemon PASS only. The next On attempt still fails closed before model initialization; no provider, integrated Mission, reviewer, milestone, or release claim exists |
| 2026-07-15 | The installed signed package exposes a second exact invariant mismatch: staging preserves upstream Codex `rg` bytes, then owner-signs that nested Mach-O, while Core compared the post-sign whole-file hash with the upstream pre-sign pin and rejected `broker.codex.prepare`. Commit `a433d1c…` retains the immutable upstream pin and accepts the signed distribution form only through the strict staged runtime receipt binding schema/component/version, upstream SHA, actual runtime SHA, `rg` identifier, bounded Team ID, and CDHash with no unknown fields | two focused tamper/false-upstream regressions, 194 ordinary Rust tests with one explicit environment-gated ignore, 44 broker/signing plus 55 App Swift tests, strict workspace Clippy, real initialize/account-read through the installed signed runtime, and exact-clean Developer-ID App/DMG staging pass at DMG SHA `c8b9cf89…`. The installed same-commit App reaches protected On with the exact Codex child and recovers On across a real App/Core/Codex restart; PR run `29463375280` succeeds | focused implementation/push/package, CI, and installed On/restart PASS only; provider/integrated-Mission proof, final reviewers, and `FRIDAY_ALPHA_READY` remain pending |
| 2026-07-15 | The first real Global Off attempt exposes the replacement-Core enrollment gap: protected Off terminates the exact leased Core/Codex and persists revision 2, but App immediately asks the replacement Core to verify that broker checkpoint before reinstalling the pinned broker enrollment. Commit `4b5df72…` reprovisions only after observing the changed Core instance nonce, then performs the existing signed checkpoint recovery | focused termination-after-broker-acceptance regression and the prior fail-closed recovery regression pass; the complete 44 broker/signing plus 56 App Swift suite, warnings-as-errors release build, strict format/plist/script checks, exact-clean Developer-ID App/DMG staging at DMG SHA `425ce241…`, and PR run `29463905626` pass. The installed same-commit build repairs stale local revision-1 On to protected revision-2 Off at launch, reaches revision-3 On with the exact Codex child, revision-4 Off after terminating the exact old Core/Codex without an alert or duplicate, then revision-5 On with a fresh Codex child | focused implementation/push/package/CI and installed replacement-Core Off/On recovery PASS only. Managed ChatGPT is not connected, no channel is paired, and the Discord Keychain item is absent; real provider credentials/permissions/participants, integrated Mission proof, final reviewers, and `FRIDAY_ALPHA_READY` remain pending |
| 2026-07-15 | The first owner login attempt reaches the signed managed-login route but the UI reports `Local operation failed closed`. The exact pinned Codex app-server succeeds outside the outer sandbox; inside it, `account/login/start` fails with `failed to start login server: Operation not permitted (os error 1)`. Exact pinned source binds only localhost callback ports 1455 and fallback 1457. The minimum repair permits inbound TCP only on those two localhost ports and leaves wildcard inbound denied | a new explicit environment-gated diagnostic reproduces the pre-repair failure and then starts the official `https://auth.openai.com/` flow through the repaired outer sandbox; the sandbox-profile regression requires both exact local ports and rejects wildcard inbound. All 194 ordinary Rust tests pass with two explicit environment-gated diagnostics ignored in the ordinary run; 44 broker/signing plus 56 App Swift tests, release builds, strict lint/format/plist/script/diff checks, and `/private/tmp/OpenOpen-FridayAlpha-LoginCallback-precommit.app` Developer-ID staging pass | focused local login-start repair PASS only. The candidate is unnotarized, not yet committed, and not installed. No OAuth credential was submitted; managed account completion, model output, real Reminders/iMessage/Discord traffic, integrated Mission proof, final reviewers, and `FRIDAY_ALPHA_READY` remain pending |
| 2026-07-15 | The first login-callback functional reviewer rejects frozen diff `22f7ac65…` on one documentation P2: the architecture still said `No port is opened`, contradicting the new bounded OAuth callback listener. The security peer is interrupted because that fingerprint is obsolete | independent exact-source/runtime/hash review; Core↔Codex application traffic remains JSON-RPC/stdio | the architecture now states that Core/App traffic opens no port and only managed sign-in temporarily permits the pinned child to listen on localhost 1455 or fallback 1457. Two entirely fresh replacement reviewers are required; the rejected review is not counted |
| 2026-07-15 | Owner fixes authority as Owner → Primary Advisor → Implementation Task and makes the implementation task pull-only. Only fingerprint-bound handoffs from the Primary Advisor authorize execution; task messages and `standing approval` do not | direct owner instruction and the canonical-control contract above | stable governance policy only; this historical row never authorizes recovery. Review closure and permission to resume exist only in external same-fingerprint PASS reports plus a later explicit stage-bound handoff |
| 2026-07-15 | Login-callback replacement and effective-Keychain-policy commits `cfdd0a0…` and `c81b86c…`, followed by package-binding commit `d68fc9c…`, are pushed to draft PR #2 | exact Developer-ID owner-test App `/private/tmp/OpenOpen-FridayAlpha-d68fc9c.app`; signed unnotarized DMG SHA `f2dff0d697076a54d193affa61127716e62c3198ebdc8162495023b9ca5bed16`; Actions run `29469720290` succeeds | code/package/CI plumbing PASS only; PR remains draft/unmerged and no integrated provider Mission is claimed |
| 2026-07-15 | Owner-authorized official Codex `0.144.0` one-time migration saves the existing managed ChatGPT Pro account to macOS Keychain, removes `auth.json` only after successful save, and a fresh keyring-only app-server reads the account; no token/password body is read or logged. Exact d68 App installation and official Background Activity Off→On then replace the stale daemon | `/Applications/OpenOpen.app` deep strict signature PASS; BTM generation 4 binds Team `UHDY2275L5`, App and embedded daemon; ServiceManagement root PID `91414` starts at 21:44:34 through Mach IPC; protected directory is `root:wheel 0700`; old PID `71822` is absent | credential migration, signed install, administrator approval, and new broker lifecycle PASS. Installed d68 still reports account Not connected because its outer sandbox resolves the user Keychain domain incorrectly; no milestone claim |
| 2026-07-15 | First Keychain-domain repair review is invalidated: functional reviewer PASS, security reviewer FAIL with P1 raw login-Keychain read/write and two P2s for path-shaped HOME and unfrozen direct-vs-secrets backend | replacement binds canonical `/Users/<owner>` to effective UID, pins and verifies `secret_auth_storage=false`, migrates only the exact prior config, allows only the exact encrypted login Keychain database read-only, and retains synthetic `CFFIXED_USER_HOME` plus all other file denials. Experiments prove no database access and metadata-only access both return `NotConnected`; exact read-only access returns real `ChatGpt` and denies writes | focused 56 Rust tests plus the explicit real connected-account diagnostic PASS; `auth.json` remains absent. Exact Developer-ID staging reports `NOT_NOTARIZED_NOT_RELEASE_PROOF`; mounted/copied DMG verification passes at SHA `587dd504…`. Two fresh replacement reviewers, commit/push/CI, signed install, and App UI/model proof remain pending |
| 2026-07-15 | Direct Owner override supersedes only the historical pull-only coordination rule with one structured `BLOCKER_REQUEST` route and reversible in-scope Advisor computer operations | the administrator password was entered only in the official System Settings secure field by the Owner; neither implementation nor Advisor handled it | governance update only; stage/scope/proof/secret/recipient/release gates are unchanged |
| 2026-07-15 | Keychain-domain Repair2 functional reviewer PASS is historical, but its security reviewer finds one P2: a symlinked `Library` or `Keychains` ancestor could redirect the nominal login database into a sandbox-writable root | Repair3 requires every relative component to be an ordinary directory/file, exact canonical equality, and explicit disjointness from both writable roots; regressions cover `Library`, `Keychains`, final-entry symlinks, and both overlap directions | Repair2 fingerprint `32fb6bbd…` is invalidated and no PASS is reused. All 200 ordinary Rust tests with two explicit real-runtime tests ignored in the ordinary run, the explicit R3 outer-sandbox connected-account diagnostic, 44 broker/signing Swift tests, 57 App Swift tests, release/strict lint/format/plist/script/diff checks, and exact Developer-ID App/DMG verification pass. Core pins are unsigned-before `37261e8b…`, unsigned-after `0e9c2440…`, and CDHash `088982d5…`; unnotarized R3 DMG SHA is `6ec2370a…`. Two entirely fresh reviewers remain required |
| 2026-07-15 | Keychain-domain Repair3 product reviewer PASS is historical, but its fresh security reviewer invalidates frozen fingerprint `cdac0ea5…` on one P2: the ordinary user can hard-link the real login Keychain database into a same-device writable `CODEX_HOME`, bypassing the literal-path write denial | the route is reproduced locally without reading database contents. Repair4 moves only volatile Codex runtime state to a broker-created, fixed-path, bounded case-sensitive tmpfs on a different device, retains the exact login database as read-only, and requires the pinned official Codex login for the new canonical runtime account. No credential is read, copied, parsed, cloned, or logged | R3 code/tests/package/reviewer PASS are invalidated and its DMG is historical. Repair4 focused verification currently passes 26 Codex-client, 29 Host, three Host-main, 46 broker/signing, and 58 App tests; signed install, real mount/hard-link proof, official account login, package rebinding, full verification, and two entirely fresh reviewers remain pending |
| 2026-07-15 | Repair4 precommit verification and package rebinding pass without weakening the runtime-home or credential boundary | the complete Rust workspace tests, release build, strict Clippy/format, 47 broker/signing and 58 App Swift tests, warnings-as-errors release build, strict Swift format, plist/script/diff/conflict/credential scans, exact Developer-ID staging, and original/mounted/copied DMG verification pass. The sole first lint failure was Rust 1.96's `cast_sign_loss`; replacing `i8 as u8` with behavior-identical `cast_unsigned()` closes it and the full strict lint rerun passes. Final App/Core/broker CDHashes are `e902db76…`, `2c1ba928…`, and `0c7bec14…`; unnotarized DMG SHA-256 is `3be1970e75047c2025ac9194a86b346e8632fc6145a54f985a74c95a2d21aafd` | local code/package verification PASS only. System Settings still must stop the old d68 broker before install; real tmpfs/device/flags/owner and kernel hard-link proof, official login/model output, two fresh reviewers, commit/push/CI, and the integrated Friday Mission remain pending |
| 2026-07-15 | The approved old-broker Off operation removes PID `91414` and its system service, allowing exact Repair4 installation. Its first maintenance-Off launch then exposes one direction-preserving defect: `Host::open` still creates the broker-owned Codex directories before the tmpfs exists, so ordinary-user Core exits. Repair5 creates only the ordinary model-input directory at Store startup; the Codex client already validates the exact tmpfs and only then creates the nested synthetic home during `broker.codex.prepare` | focused regression proves Host/Store/Dashboard open while both Codex directories remain absent; all 30 Host plus three Host-main tests, the complete Rust workspace/release/strict Clippy/format, 47 broker/signing plus 58 App Swift tests, warnings-as-errors release/strict format, and plist/script/diff/conflict/credential checks pass. Exact Repair5 staged/installed file-manifest SHA is `1ca8f7fa24f43617bbf79e11a7540f011ac3bb20fd37156be3050f17a4840573`; installed App/Core/broker CDHashes are `cd740e27…`, `b1582043…`, and `0c7bec14…`. App PID `73188` and Core PID `73189` remain live from `/Applications/OpenOpen.app` with the all-users background item Off; no broker service exists | signed Repair5 install and honest maintenance-Off startup PASS only. Background Activity On, real tmpfs/device/flags/owner/case/hard-link proof, official runtime login/model output, final package rebinding, two fresh reviewers, commit/push/CI, and the integrated Friday Mission remain pending |
| 2026-07-15 | Repair5 Background Activity On and product On launch new root broker PID `92981` at 23:57:58 from the exact installed kernel path. The real 8192-node tmpfs then proves the hard-link boundary but exposes only 62 free nodes after official Codex initialization (8126 visible runtime entries), which is not enough for safe login/Mission work. Repair6 raises only the fixed inode ceiling to 32768; the 256 MiB byte limit and every mount/security flag remain unchanged | real mount is `tmpfs,local,nodev,noexec,nosuid`; root ancestors are `root:wheel 0711`, mount root is `jarvis:wheel 0700`, runtime device `872415239` differs from Keychain device `16777231`, a real hard-link attempt returns `Cross-device link`, and two case-distinct probes have different inodes and are cleaned up. No Keychain body is read and `auth.json` is absent | Repair5 kernel boundary PASS but capacity FAIL; no login/model attempt is made on a 99%-inode-full mount. Repair6 rebuild/reinstall/remount and all later real gates remain pending |
| 2026-07-16 | Repair6 stages and installs only after product Off, exact process/service shutdown, and official Background Activity Off. Source/installing/installed manifests are identical. Because stopping the daemon does not unmount an existing tmpfs, the Owner then runs only the exact ordinary sudo `umount` in local Terminal; no credential enters chat | Repair6 App/Core/broker CDHashes are `9dbe6371…`, `b1582043…`, and `11df36a5…`; all three file-manifest SHA values are `92c689b224e33f8bb99fe5a1161f3fb08e51665358a7c56e24ef17f674254747`; modes/deep Developer-ID verification pass and Repair5/Repair4/d68 rollback Apps remain. Fresh mount inspection shows no tmpfs entry and reveals a root:wheel mode-`0700` directory with link count 2/size 64. Background Activity remains Off, and no App/Core/Codex/broker/service exists | exact Repair6 install and stale-mount removal PASS only. A TTY-scoped privileged read-only emptiness enumeration, official Background Activity On, broker-enforced pre-mount empty check, 32768-node remount/kernel proof, login/model, final package/reviewers, commit/push/CI, and integrated Mission remain pending |
| 2026-07-16 | Repair6 Background Activity On and the official product XPC route create the real 32768-node mount, but the first official OAuth callback fails closed with sanitized `persist_failed`: the read-only model sandbox cannot save the new managed credential to Keychain. Repair7 separates one login-only process from every read-only model process and rotates only the Codex member of the broker lease after the old exact audit-token incarnation is dead | real Repair6 kernel proof shows exact installed App/Core/broker/Codex identities, `tmpfs,local,nodev,noexec,nosuid`, 256 MiB, 32768 nodes with 24638 free after pinned initialization, different runtime/Keychain devices, real `EXDEV` hard-link rejection, distinct case-sensitive inodes, and absent `auth.json`. Repair7 passes the complete Rust workspace, 49 broker/signing plus 60 App Swift tests, release builds, strict Clippy/warnings/format, and focused login-success/failure plus live-old-Codex rotation rejection tests. The consolidated login process returns only completion, is destroyed, and performs no account read; fresh account/models require the replacement read-only runtime. Developer-ID App `/private/tmp/OpenOpen-FridayAlpha-KeychainLogin-R7-consolidated.app` has file-manifest SHA `a551f6ba84abdd2e9fbf450cdf86c9fe27defa04e44dcf4dc0bc869f833d7713`; App/Core/broker CDHashes are `169fc970…`, `08b035f6…`, and `a67652d3…`. Exact verified DMG SHA is `a8364887266e171a4fe3ea573d2cbba7b68d40647290bf88c6005cbb6717fb4d`; Gatekeeper honestly reports `Unnotarized Developer ID` | local Repair7 implementation/package verification PASS only. Official Background Activity Off now shows the exact all-users OpenOpen switch Off; old broker PID/service are absent. The product is Off, old App/Core/Codex are quit, and no open file remains under the still-mounted tmpfs. One exact ordinary Owner `sudo umount`, exact R7 install, one later Background On, official OAuth completion, fresh read-only account/models, real structured Outcome, integrated Mission, final reviewers, commit/push/CI, and `FRIDAY_ALPHA_READY` remain pending. The package is unnotarized and is not release proof |
| 2026-07-16 | Two fresh R7 pre-install reviewers reject workspace fingerprint `1f083b3e…` with zero P0/P1 and four P2 findings: the explicit real-login diagnostic still used the old Model-purpose/temp-home construction, current summaries retained R5/R6 status, existing tmpfs acceptance omitted exact byte/inode re-attestation, and `AGENTS.md` retained the superseded pull-only rule | repair uses the fixed broker mount and login-only client in the explicit diagnostic, proves account/model rejection there, re-attests exact 256 MiB and 32768 total inodes for both existing and new mounts with behavioral mismatch tests, and synchronizes governance/current summaries | fingerprint `1f083b3e…`, App manifest `a551f6ba…`, and DMG `a8364887…` are invalidated. Full deterministic verification, one consolidated replacement package, and two entirely fresh reviewers are required before the single later install cycle; no PASS is reused |
| 2026-07-16 | Repair7 replacement closes the four approved P2 findings without widening the product route | 202 ordinary Rust tests, release, strict Clippy/format, 50 broker/signing plus 60 App Swift tests, warnings-as-errors release/strict format, and the corrected explicit real login-only diagnostic against the fixed broker mount pass. Exact Developer-ID App `/private/tmp/OpenOpen-FridayAlpha-KeychainLogin-R7-replacement.app` has 635-entry manifest SHA `0080905e…`; App/Core/broker CDHashes are `9b822312…`, `08b035f6…`, and `92517426…`. Mounted/copied exact-verifier DMG SHA is `8735d341…`; Gatekeeper reports `Unnotarized Developer ID` | local implementation/package verification PASS only. Two entirely fresh pre-install reviewers remain required before one batched Owner unmount/install/Background-On cycle. No provider, integrated Mission, milestone, notarization, or release proof is claimed |
| 2026-07-16 | Both fresh reviewers reject replacement fingerprint `84b5651c…` on one shared P1: repeated prepare required root ownership before checking a mount that first prepare deliberately chowned to the audit EUID | replacement2 first validates root-owned ancestors, accepts only a fully exact audit-EUID-owned existing mount, and mounts only an unmounted root-owned empty same-device directory. Manager-level tests prove two prepares mount once and wrong owner/capacity reject without remount. The full 202 Rust, 52 broker/signing, and 60 App tests plus release/strict lint/format pass. Developer-ID App `/private/tmp/OpenOpen-FridayAlpha-KeychainLogin-R7-replacement2.app` has 635-entry manifest SHA `1490a4ad…`; App/Core/broker CDHashes are `aa3e9d95…`, `08b035f6…`, and `54defef5…`; exact-verifier DMG SHA is `ca3043f2…` and remains unnotarized | the failed fingerprint/package are invalidated. Replacement2 local code/package verification PASS only; two entirely fresh pre-install reviewers are required before one batched install cycle. No provider, integrated Mission, milestone, notarization, or release proof is claimed |
| 2026-07-16 | Two fresh replacement2 reviewers pass with zero P0/P1/P2, then the exact installed build and official XPC route prove the new broker/mount but real OAuth completion still fails closed at Keychain persistence | installed App/Core/broker identities match reviewed replacement2; root broker PID `93107` has `runs=1`, exact Team/CDHash/path, and creates a fresh `tmpfs,local,nodev,noexec,nosuid` CodexHome with exactly 256 MiB/32768 nodes and 24636 free. Kernel Seatbelt evidence binds the short-lived login-only Codex PID's `persist_failed` to denied metadata on the exact Keychains directory and denied read-data on its `.fl<hex>` lock file. Repair8 adds only those login-only reads; the model profile remains unchanged and tests prove every lock/sibling write, sibling read, nonmatching dot-file read, and model lock read remain denied while only the exact login database is writable. Full 202 ordinary Rust, 52 broker/signing, and 60 App tests plus Release/strict checks pass. Exact Developer-ID App `/private/tmp/OpenOpen-FridayAlpha-KeychainLock-R8-candidate.app` has 635-entry manifest SHA `5ca2f1c0…`; App/Core/broker CDHashes are `eb4e77fb…`, `3206dadd…`, and `54defef5…`; exact-verifier DMG SHA is `a84fc84e…` and Gatekeeper reports `Unnotarized Developer ID` | replacement2 reviewer/package evidence is historical after the real failure. Two fresh Repair8 reviewers, one batched replacement cycle, real OAuth/account/models/structured Outcome, integrated Mission, final reviewers, commit/push/CI, and `FRIDAY_ALPHA_READY` remain pending. No callback query, credential/token body, notarization, milestone, or release proof is claimed |
| 2026-07-16 | Both fresh Repair8 reviewers reject frozen workspace `86ec3b99…` and App/DMG `5ca2f1c0…`/`a84fc84e…` with zero P0/P1 and the same P2: present-tense BUILD_WEEK, PROVENANCE, and current-blocker text still claimed Repair7 install/reviewer/mount work was pending | both independently match all frozen fingerprints and find no additional implementation or security issue. The synchronized summaries now record replacement2 reviewer PASS, installed root broker/tmpfs, real OAuth lock denial, and Repair8 replacement status without altering historical chronology or exclusions | the rejected workspace and package are invalidated; no reviewer result is reused. Because PROVENANCE is embedded byte-identically, one newly staged/signed package, full final checks, and two entirely fresh replacement reviewers are required before the single batched install cycle |
| 2026-07-16 | Repair8 synchronized replacement closes the shared documentation P2 and rebuilds the embedded-provenance package | full 202 ordinary Rust, 52 broker/signing, and 60 App tests plus Release/strict checks remain green; repository and embedded PROVENANCE are byte-identical. Exact Developer-ID App `/private/tmp/OpenOpen-FridayAlpha-KeychainLock-R8-replacement.app` has 635-entry manifest SHA `9bde9be3…`; App/Core/broker CDHashes are `73e76889…`, `3206dadd…`, and `54defef5…`; exact mounted/copied-verifier DMG SHA is `2df5a91a…`; Gatekeeper reports `Unnotarized Developer ID` | synchronized local implementation/package verification only. Two entirely fresh replacement reviewers, one batched install cycle, real OAuth/account/models/structured Outcome, integrated Mission, commit/push/CI, and `FRIDAY_ALPHA_READY` remain pending. No provider, milestone, notarization, or release proof is claimed |
| 2026-07-16 | Repair8 synchronized replacement product reviewer PASS is historical; its fresh security reviewer rejects the unchanged fingerprint with one P1 because post-lease login retirement can still reach Core `Child::kill()` by numeric PID | Repair9 marks the transport irreversibly broker-bound when the signed exact lease is installed. Every later success, failure, cancellation, URL failure, Global Off, and drop closes stdin and uses a wait-only reaper; same-Core lease rotation terminates only the old Codex Mach audit-token incarnation in the broker and never signals App/Core. All 203 ordinary Rust, 54 broker/signing, and 60 App tests plus release/strict/static checks pass, including live-old-Codex exact termination, termination failure, dead-token rotation, and reused numeric PID. Synchronized Developer-ID App `/private/tmp/OpenOpen-FridayAlpha-LeaseBoundary-R9-final.app` has 635-entry manifest SHA `d80220a3…`; App/Core/broker CDHashes are `6d99da86…`, `c948c821…`, and `12957664…`; exact-verifier DMG SHA is `9dbe22fc…`; embedded PROVENANCE is byte-identical and Gatekeeper reports `Unnotarized Developer ID` | Repair8 replacement package/fingerprint and product PASS are invalidated. Two entirely fresh reviewers are required before the one batched install/OAuth retry. No provider, milestone, notarization, or release proof is claimed |
| 2026-07-16 | Repair9 final reviewers reject unchanged workspace `232f55a4…`, App `d80220a3…`, and DMG `9dbe22fc…`: product finds one P1 login failure/cancel/invalid-URL retry wedge plus one stale Repair7 stress-row P2; security finds one P1 because audit-token re-inspection failure was collapsed into death before lease release | Repair10 removes only the stale local-lease prerequisite from login candidate preparation while retaining trusted-broker, runtime-enabled, no-active-operation, uninitialized-candidate, signed-lease, and initialize gates. App remembers a completed managed login across a post-login model-preparation failure and retries fresh read-only account/models without a second login. Broker liveness is explicit alive/dead/inspection-failure; only exact token mismatch or ESRCH proves death, and unknown rejects same-Core rotation and Global Off without termination, persistence, or lease release. New retry and inspection-unknown regressions pass; the full 203 ordinary Rust, 56 broker/signing, and 62 App suites plus release/strict checks pass. Synchronized Developer-ID App `/private/tmp/OpenOpen-FridayAlpha-LeaseRecovery-R10-final.app` has 635-entry manifest SHA `09478032…`; App/Core/broker CDHashes are `5e77b8b7…`, `62769b84…`, and `6012c638…`; exact-verifier DMG SHA is `7e5eb9af…`; embedded PROVENANCE is byte-identical and Gatekeeper reports `Unnotarized Developer ID` | Repair9 package and all prior reviewer results are invalidated. Two entirely fresh reviewers are required before the one batched install/OAuth retry. No provider, milestone, notarization, or release proof is claimed |
| 2026-07-16 | Repair10 final product reviewer passes unchanged fingerprint with zero findings, but its fresh security reviewer rejects one P1: broker acquire may durably create the exact lease before Core receives/answers the separate lease-install request, so response loss could still send abort through an unbound transport's numeric `Child::kill()` | Repair11 adds one explicit irreversible `broker.codex.candidate.bind` Core handoff immediately after spawn and before App may call broker acquire. Request loss before handoff leaves no broker lease; response loss after handoff, broker response loss, and Core install request/reply loss all leave a pipe-close/wait-only candidate. Broker exact-token recovery/rotation remains authoritative. Focused pre-persistence rejection, durable broker-response-loss/rotation-retry, and install-response-loss tests prove every abort saw an already-bound candidate. The full 203 ordinary Rust, 56 broker/signing, and 64 App suites plus release/strict checks pass | Repair10 package and both reviewer results are invalidated. The synchronized Repair11 Developer-ID package is complete; two entirely fresh reviewers are required before the one batched install/OAuth retry. No provider, milestone, notarization, or release proof is claimed |
| 2026-07-16 | Repair11 synchronized Developer-ID owner-test package freezes the pre-broker handoff repair without changing the installed Repair7 runtime | Exact App `/private/tmp/OpenOpen-FridayAlpha-BrokerHandoff-R11-final.app` has 18 directories plus 617 files and normalized manifest SHA `3c555dedb149070644ceeb2beed6fdc5b7b9ff2940d8efec7f177a1c1f9e8ecd`; App/Core/broker CDHashes are `aa1f39b946…`, `cae5d6bfec…`, and `6012c63841…`. Exact mounted/copied-verifier DMG SHA is `13d984813e94e3d36c17dc25d82e458a91af20d8f29ecd9bfd6aa81df45d1208`; embedded PROVENANCE is byte-identical, deep strict verification passes, and Gatekeeper reports `Unnotarized Developer ID` | local implementation/package verification PASS only. Two entirely fresh pre-install reviewers and then one batched Off/unmount/install/On/provider cycle remain required. No provider, integrated Mission, milestone, notarization, or release proof is claimed |
| 2026-07-16 | First Repair11 final reviewer pair reports zero P0/P1 implementation findings but rejects current evidence with product P2×2 and security P2×1 | Both find stale Current blockers/package-pending summaries; product additionally proves the broker-response-loss test claim exceeded the existing pre-persistence failure test. A new App regression persists the first exact-lease generation, loses its acquire response, proves the already-bound candidate is aborted without initialization, and succeeds only after retry rotates the durable generation. Production sources, scripts, embedded PROVENANCE, App/DMG bytes, and installed Repair7 are unchanged | first reviewer results are invalidated. Current summaries now name completed Repair11 package and 64-App suite; two entirely fresh reviewers remain required before any installed-runtime mutation. No provider, milestone, notarization, or release proof is claimed |
| 2026-07-16 | Two fresh Repair11 reviewers pass the unchanged pre-install fingerprint with zero P0/P1/P2; exact Repair11 is then installed and reaches real official OAuth/MFA completion before credential persistence fails closed | ServiceManagement launches the exact root broker from the installed App; kernel-path/signature checks and a fresh 256 MiB/32768-node `tmpfs,local,nodev,noexec,nosuid` runtime pass with distinct runtime/Keychain devices and absent `auth.json`. Seatbelt binds the login-only Codex PID's sanitized `persist_failed` to denied creation of Security.framework's exact `login.keychain-db.sb-<8 hex>-<6 alphanumeric>` atomic-save sidecar. Repair12 grants only anchored `file-write-create` for that login-only shape; the model profile is unchanged, invalid names fail, an existing sidecar cannot be rewritten, and a pre-created matching hard link remains non-writable. The full 203 ordinary Rust, 56 broker/signing, and 64 App tests plus release/strict checks pass. Synchronized Developer-ID App `/private/tmp/OpenOpen-FridayAlpha-KeychainSidecar-R12-final2.app` has 18 directories plus 617 files and manifest SHA `b954ae640be0f27467e2303a90db3d68345355fda397ab34df4ad9efeebedb32`; App/Core/broker CDHashes are `549b4ad31f…`, `07ad9ba3ac…`, and `6012c63841…`; exact-verifier DMG SHA is `16a11f84570ae27ae1b71f851ee0f96bb301148a4f17c82ce4ea33803d4aad54`; embedded PROVENANCE is byte-identical and Gatekeeper reports `Unnotarized Developer ID` | Repair11 provider completion is historical after the real failure. Two entirely fresh Repair12 pre-install reviewers are required before one final batched replacement/OAuth retry. No credential body, account/models, structured Outcome, integrated Mission, milestone, notarization, or release proof is claimed |
| 2026-07-16 | Repair12 package is invalidated before install when both isolated review paths reproduce one P1: create-only sidecar authority fails at the next Security.framework atomic-save operation | A disposable sandboxed Keychain reports exact `file-write-mode` denial at `fchmod`, returns 161/EPERM, and contains no item; create→rename also fails. Repair13 adds only anchored login-only sidecar create/mode/owner/flags/times/unlink operations and no separate sidecar `file-write-data` or generic directory rule; newly created descriptor writes are necessary and explicit. The model profile is unchanged. The disposable Security.framework save/readback/cleanup succeeds; model save, invalid names, pre-created-hardlink content write, and hardlink rename-over-database fail. Same-UID metadata on a pre-created hard link can change and is explicitly documented without claiming content isolation. Full 203 ordinary Rust, 56 broker/signing, and 64 App suites plus release/strict checks pass. Synchronized Developer-ID App `/private/tmp/OpenOpen-FridayAlpha-KeychainAtomic-R13-final2.app` has 18 directories plus 617 files and manifest SHA `869daf8407b005c83d40a3890cbfbb1fd0943fc6e06f0197e4123a884ceb47fc`; App/Core/broker CDHashes are `bc58615e04…`, `9fdfe5aca8…`, and `6012c63841…`; exact-verifier DMG SHA is `a84a798a9cc0fad8b330f955be116999f4632628d344d5df8a84a8bea412bd39`; embedded PROVENANCE is byte-identical and Gatekeeper reports `Unnotarized Developer ID` | R12 package/reviewer evidence is historical and no install cycle is spent on it. Two fresh Repair13 reviewers, one batched install, real account/models/Outcome/integrated Mission, and every release gate remain pending |
| 2026-07-16 | First Repair13 reviewer pair finds no implementation/package P0/P1 but rejects stale current summaries and overbroad data-write wording with product P2×1 and security P2×2 | Current text now records R13 full/package completion, exact create/mode/owner/flags/times/unlink authority, necessary newly created descriptor bytes, no separate `file-write-data` predicate, and denied existing/pre-created-hardlink content/rename. Because PROVENANCE is embedded, final2 is historical. Synchronized Developer-ID App `/private/tmp/OpenOpen-FridayAlpha-KeychainAtomic-R13-final3.app` has 18 directories plus 617 files and manifest SHA `0a5620b6f3184f92b6ec88e3bfd920b38690cdb73eaa7065d6c1b04b68a147bc`; App/Core/broker CDHashes are `c61a7aec4b…`, `9fdfe5aca8…`, and `6012c63841…`; exact-verifier DMG SHA is `1879e6db5e89e7abcc5551e9f96009f90fb4c48010894536aa0d6fd7dc5368ac`; embedded PROVENANCE is byte-identical, deep verification passes, and Gatekeeper reports `Unnotarized Developer ID` | first Repair13 pair and final2 are invalidated. Two entirely fresh final3 reviewers, one batched install, real account/models/Outcome/integrated Mission, and every release gate remain pending |
| 2026-07-16 | Repair13 final3 product reviewer passes with zero findings; security finds one provenance-governance P2 and no implementation/security/package defect | Embedded PROVENANCE dynamically said two reviewers remained pending, which would become false upon review completion. The sentence is replaced by the immutable rule that review status exists only in external task/PR evidence and the package never self-certifies. Synchronized Developer-ID App `/private/tmp/OpenOpen-FridayAlpha-KeychainAtomic-R13-final4.app` has 18 directories plus 617 files and manifest SHA `2a17370077e4df61de91326bab9e8e001d1ce32292261ea67c12e82347b4f098`; App/Core/broker CDHashes are `d2f44ba340…`, `9fdfe5aca8…`, and `6012c63841…`; exact-verifier DMG SHA is `a4288eae0250d90877eaf848a6d32bfaf41b452693eca547cee7d50669d4fac6`; embedded PROVENANCE is byte-identical, deep verification passes, and Gatekeeper reports `Unnotarized Developer ID` | final3 product PASS/security result and package are invalidated by the embedded-byte change. Two entirely fresh final4 reviewers, one batched install, real account/models/Outcome/integrated Mission, and every release gate remain pending |
| 2026-07-16 | Repair14 replaces the singular Mission channel origin with one Mission-bound `ChannelRouteSet` because a single origin could not honestly bind real iMessage and Discord participation to one chronological Mission | Mission genesis atomically writes state, one primary route, and audit. An additional exact paired route requires one typed owner approval and atomically advances the route-set revision and audit; its outbound classes default Off. Bound inbound becomes a typed Mission participation event and never a free Outcome, Mission creation, scope grant, or completion Evidence. Exact route/revision/class authority governs outbound and Receipt return. Empty/one-origin migration, invalid migration, isolated wrong-owner/stale-revision/changed-recipient/class negatives, Global Off, restart, 100 duplicated/out-of-order two-route events, and 10 concurrent Missions with cross-route/cross-conversation rejection pass. The first final security reviewer passed, but the product reviewer found P1×2: Swift discarded the durable event, and a terminal Mission route captured all later explicit input from that conversation. Final2 closed those roots with a Store-verified participation command, exact Swift event display, and terminal-route release. Both fresh final2 reviewers then found the same P1: Swift rejected a legitimate immutable historical duplicate after an additional route advanced the set. Final3 accepts historical recovery only when the event revision is not in the future and the exact route already existed at that revision; future, unknown, and pre-route history fail closed. The first final3 pair found no implementation/package fault but rejected a stale current-blocker paragraph and the missing direct pre-route regression; both are synchronized. Full deterministic verification passes 215 ordinary Rust tests with two real-runtime diagnostics ignored, 56 broker/signing Swift tests, and 71 App Swift tests, plus release and strict Clippy/warnings/format checks. Synchronized Developer-ID App `/private/tmp/OpenOpen-FridayAlpha-ChannelRouteSet-R14-final3-preinstall.app` contains 18 directories plus 617 files; App/Core/broker/worker CDHashes are `abe55bfb…`, `f2b144f5…`, unchanged `6012c638…`, and `9200195f…`. Exact-verifier DMG SHA is `43167af0fdc03c6d2ff9c39340b25535d57a5baed4728cb6c913eb394dfc45d9`; embedded PROVENANCE is byte-identical and Gatekeeper reports `Unnotarized Developer ID` | synchronized local implementation/package verification only. Every prior Repair14 review result, including both final2 failures and the first final3 P2 pair, is historical. Exact static scans and two entirely fresh unchanged-fingerprint final3 pre-install reviewers remain required; the App/DMG is not installed. Installed Repair13 diagnostics are historical and cannot substitute for the same-SHA integrated Mission proof. `FRIDAY_ALPHA_READY` is not earned |

| 2026-07-16 | Repair15 real iMessage discovery diagnosis finds that the exact bundled `imsg` basic-RPC route succeeds against the owner-approved Full Disk Access boundary, but Host rejects the complete result because most legitimate Messages conversations have an empty display name | Messages database count-only diagnostic reports 24 chat rows; bounded `chats.list` returns 23 unique positive-ID `service=iMessage` rows with valid participants and no RPC error, while 21 legitimate rows have empty `name`. No message body, name, address, or token is recorded. Repair15 permits empty-but-bounded/NUL-free/trimmed names while retaining every participant, ID, service, sorting, dedupe, pairing, and scope gate. The 220-executed-Rust/56+72-Swift matrix and release/strict/static checks pass. Developer-ID final2 App manifest is `7edd3067…`; exact-verifier unnotarized DMG SHA is `6bcf0ade…` | local implementation/package verification PASS only. Two fresh reviewers, one consolidated install cycle, and the real integrated Mission proof remain pending; the installed Repair14 diagnostics are not milestone proof |
| 2026-07-16 | Repair16 fixes the Developer-ID Discord token Keychain contract exposed by the first approved real setup attempt | The approved Friday token was entered directly into the OpenOpen SecureField without output or a temporary copy, but setup failed before any provider request because `DiscordTokenKeychain` selected the data-protection Keychain and reproduced `errSecMissingEntitlement (-34018)`. The repair uses the same single native login Keychain backend as the established broker store, with no fallback, while retaining the exact service/account/accessibility and redacted errors. Disposable unique-item tests prove save/readback/update/delete and cleanup; a separate regression reproduces the rejected legacy selector. The first frozen candidate's security reviewer passed, but its product reviewer found P1×1 false-connected feedback and P2×1 mutable paired-iMessage controls. A later product reviewer found P2×1 missing direct `reconnecting` coverage. The next product reviewer found P2×1 because a late Discord-start failure could overwrite Global Off cleanup: `connectDiscord` lacked the generation guard used by adjacent channel flows. Every associated security result is non-reusable after the edits. Repair16 now tracks connecting/reconnecting/connected/faulted without stale success, locks connected iMessage selection, directly regresses reconnecting, and rejects stale Discord-start callbacks after Off while proving no feedback/error/poll resurrection. The complete 220-Rust/56+79-Swift strict matrix passes. Prior manifests `452f9c0e…`/`b8770e96…` and DMGs `4080763d…`/`03c4f691…` are invalid historical artifacts. Synchronized final3 App manifest is `486ba7ac…`; App/Core/broker/worker CDHashes are `8f8a1388…`, `416052fb…`, `002e5156…`, and `9200195f…`; exact-verifier DMG SHA is `9e43064a…`, embedded PROVENANCE is byte-identical, and Gatekeeper reports `Unnotarized Developer ID`. iMessage is durably paired with no send; the real Discord item remains absent and no Discord traffic occurred | replacement local implementation/package verification PASS only. Two entirely fresh pre-install reviewers, one consolidated replacement cycle, and the integrated Mission remain pending; `FRIDAY_ALPHA_READY` is not earned |
| 2026-07-16 | Repair16 final3 reviewers invalidate that candidate before install: product reports P0/P1/P2 `0/1/1`, security `0/0/1` | Product proves a late iMessage activation or channel poll could overwrite Global Off state because generation/cancellation was checked after one mutation or not after the await. Both reviewers reject stale current-state paragraphs that still named final2/78 tests. Final4 validates the captured generation immediately after every channel await and before status, feedback, Mission-event, or suggestion mutation; stale cleanup cannot stop a newer generation. Delayed activation→Off and delayed poll→Off regressions pass. The full 220-Rust/56+81-Swift strict matrix passes. Developer-ID final4 App `/private/tmp/OpenOpen-FridayAlpha-DiscordState-R16-final4-preinstall.app` has 18 directories plus 617 files, manifest `5c155f4b…`, and App/Core/broker/worker CDHashes `77ffc388…`, `416052fb…`, `002e5156…`, and `9200195f…`; exact-verifier DMG SHA is `ef1e3cb9…`, embedded PROVENANCE is byte-identical, and Gatekeeper reports `Unnotarized Developer ID` | final3 and both reviewer results are historical. Final4 local implementation/package verification passes; two entirely fresh pre-install reviewers, one consolidated replacement cycle, and the integrated Mission remain pending. No provider, milestone, notarization, or release proof is claimed |
| 2026-07-16 | Repair16 final4 review invalidates that candidate before install: security reports P0/P1/P2 `0/0/0`; product reports `0/1/1` | Product proves Discord start was not response-loss idempotent: after Host created the one live provider session but the RPC reply was lost, retry returned `AlreadyRunning` and Swift could not reattach. The delayed-poll Off regression was also cancellation-aware, so it did not prove a callback completed after cancellation. Final5 validates the exact durable pairing before returning the already-running adapter's current status, never creates a duplicate provider session, and rejects changed pairing. Swift retry fault injection proves one committed session across two start RPCs. The poll mock uses a detached non-cooperative delay and asserts its one late callback returns but cannot mutate state after Off. The complete 221-Rust/56+82-Swift strict matrix, release, strict Clippy/warnings/format, diff, conflict, and secret checks pass. Developer-ID final5 App `/private/tmp/OpenOpen-FridayAlpha-DiscordState-R16-final5-preinstall.app` has 18 directories plus 617 files, normalized manifest `bdeff03b…`, and App/Core/broker/worker CDHashes `aea19e6f…`, `fd7b31d7…`, `002e5156…`, and `9200195f…`; exact-verifier DMG SHA is `bd8f2b08…`, embedded PROVENANCE SHA `1f27067c…` is byte-identical, and Gatekeeper reports `Unnotarized Developer ID` | final4 and both reviewer results are historical. Final5 is synchronized local implementation/package evidence only. Two entirely fresh final5 reviewers, one consolidated replacement cycle, and the same-build integrated Mission remain pending. No provider, milestone, notarization, or release proof is claimed |
| 2026-07-16 | Repair16 final5 product review invalidates that candidate before install with P0/P1/P2 `0/1/0`; its security review is interrupted and non-reusable | Host durably stored the exact Discord pairing and cleared setup before replying, but a lost pairing-confirm response left Swift's setup/candidate UI stale. Retrying the visible Confirm action necessarily hit a removed setup session, while the alternate durable-pairing start did not clear the stale UI. Final6's confirmation catch reads only the verified durable Discord pairing, requires exact equality to the candidate's owner, conversation, guild, bot/application, setup source message, and candidate ID, then starts the bounded route and clears obsolete setup UI. Any mismatch or missing token remains faulted. A real-shape fault injection commits pairing, loses the reply, recovers with one confirmation and one provider session, and proves clean connecting UI. The complete 221-Rust/56+83-Swift strict matrix, release, strict Clippy/warnings/format, diff, conflict, and secret checks pass. Developer-ID final6 App `/private/tmp/OpenOpen-FridayAlpha-DiscordState-R16-final6-preinstall.app` has 18 directories plus 617 files, normalized manifest `9d0ee12a…`, and App/Core/broker/worker CDHashes `b053692c…`, `fd7b31d7…`, `002e5156…`, and `9200195f…`; exact-verifier DMG SHA is `ca9f24d4…`, embedded PROVENANCE SHA `0bca09ab…` is byte-identical, and Gatekeeper reports `Unnotarized Developer ID` | final5 and its product result are historical. Final6 is synchronized local implementation/package evidence only. Two entirely fresh final6 reviewers, one consolidated replacement cycle, and the same-build integrated Mission remain pending. No provider, milestone, notarization, or release proof is claimed |
| 2026-07-16 | Repair16 final6 product review invalidates that candidate before install with P0/P1/P2 `0/1/0`; its security review is interrupted and non-reusable | Host returned every exact already-present Discord session, including a terminal faulted adapter whose provider task had ended, so Swift's visible retry could only reattach to a permanent fault and could not meet provider-failure recovery. Final7 records whether the initial provider launch is still pending: an immediate response-loss retry retains that exact scheduled/live session, while an exact retry first stops and replaces one terminal faulted or launch-complete disconnected session; changed pairing still fails closed and no outbound effect is created. Host and Swift fault→retry regressions prove one replacement session. The complete 222-Rust/56+84-Swift strict matrix, release, strict Clippy/warnings/format, diff, conflict, and secret checks pass. Developer-ID final7 App `/private/tmp/OpenOpen-FridayAlpha-DiscordState-R16-final7-preinstall.app` has 18 directories plus 617 files, normalized manifest `5044a828…`, and App/Core/broker/worker CDHashes `ad567d44…`, `8905f8ee…`, `002e5156…`, and `9200195f…`; exact-verifier DMG SHA is `8ea86593…`, embedded PROVENANCE SHA `3f32b64c…` is byte-identical, and Gatekeeper reports `Unnotarized Developer ID` | final6 and its product result are historical. Final7 is synchronized local implementation/package evidence only. Two entirely fresh final7 reviewers, one consolidated replacement cycle, and the same-build integrated Mission remain pending. No provider, milestone, notarization, or release proof is claimed |
| 2026-07-16 | Repair16 final7 product review invalidates that candidate before install with P0/P1/P2 `0/1/0`; its security review is interrupted and non-reusable | Product proves restart recovery could expose raw `Connected` and an outbound handle before the recovered envelopes and final high-water cursor were durably accepted. A failed recovery receiver also remained installed, so every later poll/retry could wedge on the same terminal failure. Final8 retains one exact recovered event until its Store write succeeds, repeats it after Store failure or response loss, reports only Connecting and denies outbound while recovery is unresolved, atomically rejects malformed batches, and stops/removes one failed session for a clean exact-pairing retry. Global Off removes pending recovery. The outbound handle gate precedes every Mission approval/outbound-intent mutation. The complete 227-executed-Rust/56+84-Swift strict matrix, 100 dual-route duplicate/out-of-order events, 10 concurrent Missions, release, strict Clippy/warnings/format, metadata, notice, diff, conflict, and secret checks pass. Developer-ID final8 App `/private/tmp/OpenOpen-FridayAlpha-DiscordRecovery-R16-final8-preinstall.app` has 18 directories plus 617 files, normalized manifest `64fc1bff…`, and App/Core/broker/worker CDHashes `ead00b7e…`, `b3b85a63…`, unchanged `002e5156…`, and unchanged `9200195f…`; exact-verifier DMG SHA is `18f9cb2b…`, embedded PROVENANCE SHA `1d93a19a…` is byte-identical, and Gatekeeper reports `Unnotarized Developer ID` | final7 and both reviewer results are historical. Final8 is synchronized local implementation/package evidence only. Two entirely fresh final8 reviewers, one consolidated replacement cycle, and the same-build integrated Mission remain pending. No provider, milestone, notarization, or release proof is claimed |
| 2026-07-17 | Repair16 final8 product review invalidates that candidate before install with P0/P1/P2 `0/1/0`; its unfinished security review is interrupted and non-reusable | Product proves the first recovered intent could claim GPT and surface an Outcome while a later correction and the final provider high-water cursor were still pending. Final9 continues to atomically persist and acknowledge each exact recovery event, but denies every queued Discord model claim until the final high-water cursor is durably accepted. A chronological two-message original-intent→correction Host regression proves both dispatches remain queued, no suggestion is exposed during partial recovery, and the oldest dispatch becomes eligible only after cursor closure. The complete 228-executed-Rust/56+84-Swift strict matrix, 100 dual-route duplicate/out-of-order events, 10 concurrent Missions, release, strict Clippy/warnings/format, metadata, notice, diff, conflict, and secret checks pass. Developer-ID final9 App `/private/tmp/OpenOpen-FridayAlpha-DiscordRecovery-R16-final9-preinstall.app` has 18 directories plus 617 files, normalized manifest `7c666b20…`, and App/Core/broker/worker CDHashes `8b7c2689…`, `4722f57a…`, unchanged `002e5156…`, and unchanged `9200195f…`; exact-verifier DMG SHA is `15e4ca4f…`, embedded PROVENANCE SHA `cd7d15d7…` is byte-identical, and Gatekeeper reports `Unnotarized Developer ID` | final8 and both reviewer results are historical. Final9 is synchronized local implementation/package evidence only. Two entirely fresh final9 reviewers, one consolidated replacement cycle, and the same-build integrated Mission remain pending. No provider, milestone, notarization, or release proof is claimed |
| 2026-07-17 | Repair16 final9 product review invalidates that candidate before install with P0/P1/P2 `0/1/0`; its security review is interrupted and non-reusable | Product proves that cursor closure did not arbitrate multiple queued Outcomes: Host surfaced the oldest result while a later correction remained queued, Swift retained that obsolete result, and Dashboard could restore it before recovery after restart. Final10 persists every intermediate result for audit but withholds and rejects confirmation while the same channel has queued/started work, releases/restores only the newest ready result after recovery, and supplies the final GPT turn with at most eight chronological same-owner/same-conversation messages only when signed audit order proves every predecessor completed after the later message was already durable. The existing 16 KiB prompt bound remains; invalid, changed, unrelated, or over-bounded context fails closed. Swift clears a recovering channel result and replaces it only with the final Host result. Host+Swift regressions cover original→correction, stale Dashboard recovery, response loss/restart, exact confirmation, and Global Off generation behavior. The complete 228-executed-Rust/56+85-Swift strict matrix, 100 dual-route duplicate/out-of-order events, 10 concurrent Missions, release, strict Clippy/warnings/format, metadata, notice, diff, conflict, and secret checks pass. Developer-ID final10 App `/private/tmp/OpenOpen-FridayAlpha-DiscordRecovery-R16-final10-preinstall.app` has 18 directories plus 617 files, normalized manifest `c19d08db370e6b4277319868a69e673d4c44cc2ceb9d826405ae20eee826ec94`, and App/Core/broker/worker CDHashes `5dd1cf48…`, `5c5aa618…`, unchanged `002e5156…`, and unchanged `9200195f…`; exact mounted/copied verifier DMG SHA is `9c786aa40a39340033fadf4d7f2864945cd0b4f1f7cad8b15b47a3cddc56644c`, embedded PROVENANCE SHA `c7af7a7e…` is byte-identical, and Gatekeeper reports `Unnotarized Developer ID` | final9 and both reviewer results are historical. Final10 is synchronized local implementation/package evidence only. Two entirely fresh final10 reviewers, one consolidated replacement cycle, and the same-build integrated Mission remain pending. No provider, milestone, notarization, or release proof is claimed |
| 2026-07-17 | Repair16 final10 review pair invalidates that candidate before install; both fresh reviewers report P0/P1/P2 `0/1/0` | Both reviewers prove signed audit overlap established only concurrency, not that the later same-owner/same-conversation message corrected the earlier intent. Final10 could therefore combine unrelated intents into one confirmable Outcome. Final11 requires the exact case-insensitive `Correction to previous:` owner directive before importing only the immediately preceding result, and still requires the later durable-observation-before-earlier-result audit order. Time overlap alone imports nothing; an unmatched directive remains a single-message request. Host rejects caller-assembled multi-message context without the directive. The complete final11 matrix passes: 230 executed Rust tests with two explicit real-runtime diagnostics ignored, 56 broker/signing plus 85 App tests, 100 duplicate/out-of-order dual-route events, 10 concurrent Missions, release, strict lint/format, metadata, notices, plist/scripts, diff, conflict, credential-path, and secret checks. Final11 Developer-ID App `/private/tmp/OpenOpen-FridayAlpha-DiscordRecovery-R16-final11-preinstall.app` has 18 directories plus 617 files, normalized manifest `5bf86d5d…`, and App/Core/broker/worker CDHashes `6caafeac…`, `88f76fae…`, unchanged `002e5156…`, and unchanged `9200195f…`; exact read-only mounted/copied-verifier DMG SHA is `a2887a91…`, embedded PROVENANCE SHA `9fd57db9…` is byte-identical, and Gatekeeper reports `Unnotarized Developer ID` | final10 App/DMG and both review results are historical and must not be installed. Final11 is synchronized local implementation/package evidence only. Two entirely fresh final11 reviewers, one consolidated replacement cycle, and the same-build integrated Mission remain pending. No provider, milestone, notarization, or release proof is claimed |
| 2026-07-17 | Repair16 final11 review pair invalidates that candidate before install: product P0/P1/P2 `0/0/1`; security `0/1/1` | Product proves Host accepted caller-assembled correction context above the exact two-message cap. Security proves Store's ready-only lookup could skip immediate started B and bind correction C to older ready A because later queued claims were not serialized; it also identifies stale present-tense final10 blocker text. Final12 makes one started dispatch block every later claim on that channel, atomically begins only the oldest queued source, selects the immediate accepted predecessor before requiring that exact dispatch to be ready and audit-qualified, leaves unmatched/unready context single-message without importing older rows, rejects Host context above two, and updates the stale current-state paragraph. The complete final12 matrix passes: 231 executed Rust tests with two explicit real-runtime diagnostics ignored, 56 broker/signing plus 85 App tests, 100 duplicate/out-of-order dual-route events, 10 concurrent Missions, release, strict lint/format, metadata, notices, plist/scripts, diff, conflict, credential-path, and secret checks. Final12 Developer-ID App `/private/tmp/OpenOpen-FridayAlpha-DiscordRecovery-R16-final12-preinstall.app` has 18 directories plus 617 files, normalized manifest `74b71f64…`, and App/Core/broker/worker CDHashes `be5761e1…`, `8b75c01e…`, unchanged `002e5156…`, and unchanged `9200195f…`; exact read-only mounted/copied-verifier DMG SHA is `d09a8008…`, embedded PROVENANCE SHA `80d25f13…` is byte-identical, and Gatekeeper reports `Unnotarized Developer ID` | final11 package and both reviews are historical and must not be installed. Final12 is synchronized local implementation/package evidence only. Two entirely fresh final12 reviewers, one consolidated replacement cycle, and the same-build integrated Mission remain pending. No provider, milestone, notarization, or release proof is claimed |
| 2026-07-17 | Repair16 final12 review pair invalidates that candidate before install: product P0/P1/P2 `0/1/1`; security `0/0/2` | Both reviewers identify stale current-state text that still names final11/final10. Product additionally proves a distinct restart P1: strict FIFO leaves the one consumed dispatch `started`, but Host restart polling selected only `queued` work, so the existing `RecoverOnly`→`Need you` path was unreachable and the in-flight row blocked every later correction. Final13 exposes only the exact single started source as recovery-only before any queued claim, never grants a second model call, fails closed on multiple started rows, surfaces local `Need you`, and stops automatic channel polling. The complete final13 matrix passes 233 executed Rust tests with two explicit real-runtime diagnostics ignored, 56 broker/signing plus 86 App tests, the 100-event and 10-Mission stress suites, release, strict lint/format, metadata, notices, plist/scripts, diff, conflict, credential-path, and secret checks. Developer-ID App `/private/tmp/OpenOpen-FridayAlpha-DiscordRecovery-R16-final13-preinstall.app` has 18 directories plus 617 files, manifest `dc83e390…`, App/Core/broker/worker CDHashes `5691aaad…`/`c07958ff…`/unchanged `002e5156…`/`9200195f…`, and embedded PROVENANCE `f027dc0d…`; exact read-only mounted/copied-verifier DMG SHA is `9d6a1eb0…`, and Gatekeeper reports `Unnotarized Developer ID` | final12 App/DMG and both review results are historical and must not be installed. Final13 local verification/package PASS; two entirely fresh final13 reviewers, one consolidated install cycle, and the same-build integrated Mission remain pending. No install, provider, milestone, notarization, or release proof is claimed |
| 2026-07-17 | Repair16 final13 review pair invalidates that candidate before install; both fresh reviewers report P0/P1/P2 `0/0/1` | Both independently pass the implementation, complete Rust/Swift matrix, started-dispatch recovery, FIFO/correction/high-water boundaries, Keychain/secrets, exact package, signatures, mounted/copy manifest, and no-overclaim checks. Both identify the same sole P2: a later live `Current blockers` paragraph still presented final12 as current and its already completed matrix/package/review as pending. Final14 changes only that stale current-state paragraph and records final13 as historical; no product, test, provider, broker, sandbox, recipient, or package authority changes. The complete final14 matrix passes 233 executed Rust tests with two explicit real-runtime diagnostics ignored, 56 broker/signing plus 86 App tests, both stress suites, release, strict lint/format, metadata, notices, plist/scripts, diff, conflict, credential-path, and secret checks. Developer-ID App `/private/tmp/OpenOpen-FridayAlpha-DiscordRecovery-R16-final14-preinstall.app` has 18 directories plus 617 files, manifest `69a28c4f…`, App/Core/broker/worker CDHashes `fdd2cd29…`/`c07958ff…`/unchanged `002e5156…`/`9200195f…`, and embedded PROVENANCE `f027dc0d…`; exact mounted/copied-verifier DMG SHA is `1ca74ccd…`, and Gatekeeper reports `Unnotarized Developer ID` | final13 App/DMG and both reviews are historical and must not be installed. Final14 local verification/package PASS; two fresh reviewers, one consolidated install cycle, and the same-build integrated Mission remain pending. No install, provider, milestone, notarization, or release proof is claimed |

## Historical blocker log — non-normative

Everything under this heading is a chronological record of superseded
Repair16/Repair17 decisions. It is not a current blocker list, execution
instruction, acceptance gate, or authority source. In particular, its
present-tense statements about absent Discord pairing, pending Repair17
installation, missing signed/admin proof, and required Slack/Auto/Hero B/C work
are historical facts from their dated moments and are false for current
planning. The dated normative contract and current Repair19 state above are the
only current authority: Repair18 final4 is installed, iMessage and Discord are
durably paired, the first Discord Alpha intent was sent exactly once and must
not be resent, Repair19 final5 recovery is the first unclosed Alpha item, and
Slack/Auto/Hero B/C remain excluded.

- Repair16 is the first unclosed Friday-alpha item. Installed Repair15 now
  discovers the bounded Messages list and has one owner-selected durable
  iMessage pairing; no message was sent. The first approved real Discord setup
  failed before provider access because its token store selected the
  data-protection Keychain, which returns `errSecMissingEntitlement (-34018)`
  for this Developer-ID distribution. Repair16 removes only that selector and
  keeps one native login Keychain backend with the exact service, account,
  accessibility, validation, and redacted error contract. Disposable
  save/readback/update/delete and legacy-failure tests pass. The real Discord
  item remains absent; no token value, channel body, Discord pairing, send,
  Mission, or Receipt is recorded. The full strict matrix and synchronized
  first frozen App/DMG and its security PASS are historical after product
  review found P1×1/P2×1. The replacement state machine and iMessage control
  lock and Discord generation guard passed the 220-Rust/56+79-Swift strict
  matrix, but final3 product review found one P1 late iMessage activation/poll
  route and both reviewers found the stale final2 documentation P2. Final4
  fenced every channel await, but product review found the distinct P1
  Discord-start response-loss reattachment gap and P2 cancellation-aware poll
  test gap; its security PASS is historical. Final5 closed those roots but
  product review found the preceding pairing-confirm response-loss P1. Final6
  recovers only an exact durable match for every confirmed candidate identity,
  clears stale setup UI, and starts one provider session, but its product
  reviewer found that a terminal faulted provider session could never recover
  through the visible retry. Final7 preserved one pending/live session across
  response loss but its product reviewer found that restart recovery still
  exposed Connected/outbound before durable cursor closure and could retain a
  failed receiver. Final8 makes recovered-event delivery Store-acknowledged,
  denies Connected/outbound while pending, stops failed sessions, and preserves
  exact retries across Store failure or response loss, but its product reviewer
  found that the first recovered intent could reach GPT before a later
  correction and final provider high-water cursor were durable. Final9 keeps
  every queued Discord model dispatch closed until that final cursor commits,
  but its product reviewer proved the oldest result could then surface while a
  correction remained queued. Final10 adds recovery-aware suggestion
  arbitration, exact newest-result restoration, and Swift replacement of a
  stale channel result. Both final10 reviewers then invalidate its chronological
  context because audit overlap alone could merge unrelated intents. Final11
  added the exact `Correction to previous:` owner directive and a two-message
  cap, but its reviewers found an overbound Host caller and a ready-only lookup
  that could skip an unresolved immediate predecessor. Final12 closed those
  context/FIFO roots and passed its complete 231-Rust/56+85-Swift matrix, but
  its fresh review pair invalidated it before install: product found that a
  Host restart could not select the exact durable `started` dispatch for the
  existing recovery-only `Need you` path, while both reviewers found stale
  final11/final10 current-state text. Final13 selects the exact single started
  source before queued work, never repeats the model call, rejects multiple
  started rows, surfaces local `Need you`, and stops automatic polling for that
  paused channel. The complete final13 matrix passes 233 executed Rust tests
  with two explicit real-runtime diagnostics ignored, 56 broker/signing plus
  86 App tests, both stress suites, release, strict lint/format, metadata,
  notices, plist/scripts, diff, conflict, credential-path, and secret checks.
  Synchronized final13 Developer-ID App has 18 directories plus 617 files,
  manifest `dc83e390…`, App/Core/broker/worker CDHashes `5691aaad…`/
  `c07958ff…`/unchanged `002e5156…`/`9200195f…`, and embedded PROVENANCE
  `f027dc0d…`. Its exact read-only mounted/copied-verifier DMG SHA is
  `9d6a1eb0…`; Gatekeeper honestly reports `Unnotarized Developer ID`. Two
  fresh reviewers and one consolidated install cycle remain pending. Both
  final13 reviewers then pass code, tests, recovery, security, and package
  checks but invalidate that fingerprint on one shared P2: the later live
  blocker paragraph below still called final12 current. Final14 changes only
  that stale paragraph and records final13 as historical. The complete final14
  233-Rust/56+86-Swift matrix and synchronized Developer-ID package pass;
  App manifest is `69a28c4f…`, App/Core/broker/worker CDHashes are
  `fdd2cd29…`/`c07958ff…`/unchanged `002e5156…`/`9200195f…`, embedded
  PROVENANCE is `f027dc0d…`, and mounted/copied DMG SHA is `1ca74ccd…`.
  Gatekeeper honestly reports `Unnotarized Developer ID`. Two fresh final14
  reviewers passed and that exact package was installed with administrator
  approval; its broker/tmpfs/account/model checks pass. The first real Discord
  setup then reached the official Gateway, but SwiftUI localized the 19-digit
  `UInt64` bot ID in the visible pairing instruction and inserted grouping
  commas, producing an invalid mention. Repair16 final15 constructs the exact
  instruction with `String(botUserId)` and renders it through
  `Text(verbatim:)`; a grouping-locale regression proves the mention contains
  only the exact decimal digits and the complete instruction preserves the
  32-lowercase-hex pairing code. The Owner then sent that corrected command
  exactly once through Discord's official mention picker in the approved
  channel. Durable pairing is still absent because the Connections page's
  always-mounted `SecureField` closes the authorized Computer Use pipe before
  the official Check action can be operated. The unreviewed final15 staging is
  therefore invalidated before install. Repair17 retains the verbatim
  instruction and isolates token entry in an explicitly opened, auto-focused
  secure sheet; ordinary Connections contains no secret field, and cancel,
  submit, dismissal, and Global Off erase the ephemeral draft. The unchanged
  Keychain validation/save and official setup path remain the only token route.
  The complete 233-executed-Rust/56+90-Swift matrix, both stress suites,
  release, strict lint/format, metadata, notices, plist/scripts, and static
  scans pass locally. The synchronized Repair17 Developer-ID App at
  `/private/tmp/OpenOpen-FridayAlpha-SecureConnections-R17-final1-preinstall.app`
  has 18 directories plus 617 files, normalized manifest `6ae53c60668e…`,
  App/Core/broker/worker CDHashes `ece5a62f…`/`c07958ff…`/unchanged
  `002e5156…`/`9200195f…`, and byte-identical embedded PROVENANCE
  `a08fc810…`. Its exact read-only mounted/copied-verifier DMG SHA is
  `a1b3cf15…`; Gatekeeper reports `Unnotarized Developer ID`. The first
  Repair17 product reviewer rejects this freeze with P0/P1/P2=`0/1/1`: the
  package was synchronized but one ledger row still called it pending, and the
  documented install-before-confirm sequence would destroy the current
  in-memory Discord setup session and replace its already-sent random pairing
  code. The incomplete security review is stopped and non-reusable. Installation
  is now explicitly prohibited until the Owner-visible current Repair16 UI
  first runs Check, verifies the exact approved Friday guild/channel/owner, and
  action-time Confirms that candidate into durable pairing. If that live setup
  session is lost first, no second message or fabricated pairing is allowed;
  the task must stop at that external UI boundary. This documentation repair
  corrects the package contradiction and execution order without changing App,
  Core, broker, provider, or package bytes. Two entirely fresh reviewers, then
  the one consolidated Repair17 install cycle, and the real same-Mission closure
  remain pending; no Discord pairing, Mission, Receipt, or milestone is claimed
  here.

- The linearizable global effect fence/reconciliation foundation is committed
  at `19ecdd9…`, has two fresh isolated reviewer PASS reports, and passes the
  local same-SHA suites. Follow-up Actions run `29370433505` passes synthesized
  PR merge `d502b3d…`, whose tree equals CI workflow head `923c88a…`; this is not direct
  exact-head proof. Signed/admin, cross-UID, and release proof remain absent.
- Public `thesongzhu/OpenOpen` now exists and `main` points to reviewed
  bootstrap `19ecdd9…`. Draft PR #1 carries the CI workflow on
  `agent/foundation-ci`; it must remain unmerged until its current head checks
  and the applicable later proof gates are honestly satisfied.
- Managed ChatGPT Keychain save/read is real, but no real structured Outcome or
  integrated Mission closure exists. Real iMessage, Discord, Reminders,
  notarization, clean-machine, and three-user evidence have not yet been run.
  Slack, Memory Passport, Auto
  routing, Deep ZIP, Hero B, and Hero C are contract-only and not implemented
  or claimed. All remain required at their named milestones and cannot be
  represented by mocks.
- This document never self-certifies its own review or handoff status. Those
  facts exist only in external task evidence bound to the exact document
  fingerprint. Without two fresh same-fingerprint PASS reports and an explicit
  stage-bound handoff from the Primary Advisor, implementation remains
  unauthorized to resume or expand scope. Blocker routing is coordination,
  never stage authority.
- Hero A's first locally verified candidate failed both fresh closure reviews;
  Repair1 then passed governance but failed functional review on physical
  EventKit target drift. Both Repair2 reviewers failed its first-write,
  cancellation, and all-markers-missing recovery boundary. Both Repair3
  reviewers failed because possible EventKit commit still left volatile
  `createOnce` authority reusable. Repair4 persisted signed dispatch Evidence
  before EventKit, but governance found its lower-level writer still accepted
  reusable Mission copies. Repair5 makes the writer internal and consumes the
  complete one-shot start before any external boundary. The full local
  146-Rust/83-Swift suite, strict static checks, two pinned-runtime diagnostics,
  and fresh ad-hoc Repair5 staging pass. Two entirely fresh replacement
  reviewers PASS frozen fingerprint `4b41a04f…` with no P0/P1/P2 findings. No
  real provider output or user Reminders write/readback has been run. Reviewed
  commit `774789c…` is pushed to draft PR #2 and current Actions run
  `29393462659` passes on equal-tree synthesized merge `bccdf360…`; this is
  plumbing evidence, not product or release proof.
- Product-shell commit `e2313fe…` is pushed to draft PR #2. Two fresh Repair20
  reviewers PASS frozen fingerprint `29a00413…`, and Actions run `29386477267`
  passes synthesized merge `487dae1…`, whose tree equals the head tree. This is
  not release proof. Later exact Developer-ID installation, administrator
  ServiceManagement approval, protected root daemon, official Keychain
  migration, and keyring-only account read are real. The current read-only
  login-Keychain Repair2 failed security review on ancestor-symlink redirection,
  and Repair3 then failed on the remaining same-device hard-link route. Repair4
  uses a root-brokered fixed tmpfs runtime home on a different device and does
  not copy credentials. Repair5 closes the maintenance-Off Core-start defect;
  its full Rust/Swift verification and exact signed installation pass. Repair6
  later proves the exact 256 MiB/32768-node mount, flags, ownership, different-
  device hard-link rejection, and case sensitivity. Repair7 replacement2 then
  passed two fresh reviewers, was installed, launched exact root broker PID
  `93107` and the bounded tmpfs, and reached real OAuth before the login-only
  sandbox denied the Keychain `.fl<hex>` lock read. Repair8 narrows that lock
  protocol repair to the login-only profile. Its first two fresh reviewers
  rejected only stale current summaries; after synchronization, product review
  passed but security review found one P1 in Core's post-lease numeric-PID kill
  path. Repair9 moves that termination to the broker's exact Mach audit-token
  boundary and leaves Core with pipe-close/wait-only cleanup. Its final
  reviewers found the retry wedge and inspection-unknown ambiguity; Repair10
  closes both without widening runtime or Keychain authority. Repair11 then
  irreversibly binds every Codex candidate before broker acquisition, and its
  test-only review repair directly proves durable acquire-response loss followed
  by exact-lease rotation retry. Two fresh reviewers passed Repair11; its exact
  Developer-ID App `3c555ded…`/DMG `13d98481…` was installed and the signed
  broker/bounded tmpfs passed kernel verification. Real OAuth/MFA completed but
  final Keychain persistence failed closed on Security.framework's exact
  atomic-save sidecar create. Repair12 grants only anchored login-only
  `file-write-create` for that shape; model access, existing-path writes, and
  hard-link writes remain denied in focused tests. Repair12's synchronized
  package is historical after both review paths reproduced its next-step
  `file-write-mode` P1. Repair13 completes only the exact sidecar metadata and
  unlink lifecycle without a separate sidecar `file-write-data` predicate or
  any model authority; newly created descriptor writes are necessary and
  explicit. Focused disposable Keychain save/readback and hardlink negatives,
  full verification, final4 review, installation, and provider/runtime
  diagnostics are historical inputs to Repair14. They do not prove one
  integrated Mission. Repair14 now binds primary and additional approved
  channel routes to one Mission with typed atomic participation and exact
  per-route outbound authority. Final3 binds each event to a Store-verified
  no-authority Mission participation command, exposes exact current and valid
  immutable historical events in Swift, rejects future/unknown/pre-route
  history, and releases a terminal route for the next explicit Outcome. Its
  215-Rust/56+71-Swift deterministic suite and synchronized Developer-ID
  App/DMG are historical after the real unnamed-chat failure. Repair15's
  220-executed-Rust/56+72-Swift suite and final2 Developer-ID App/DMG pass and
  its installed UI completes one exact owner-selected iMessage pairing without
  a send. The first approved Discord setup then exposes Repair16's
  data-protection-Keychain mismatch before provider access. Repair16's native
  login-Keychain query and disposable lifecycle pass. The first synchronized
  candidates are invalidated by product-review findings, including final4's
  Discord start-response-loss P1/P2 and final5's pairing-confirm-response-loss
  P1. Final6 then failed product review on terminal provider-session recovery.
  Final7 then failed product review on unresolved recovery authority. Final8
  failed product review on partial-batch model ordering. Final9 then failed
  product review because the oldest post-cursor result could surface before a
  queued correction. Both final10 reviewers rejected implicit correction
  merging. Final11 then failed because dispatch was not strictly serial and
  Host did not independently cap caller-built context at two. Final12 then
  passed its complete matrix/package but was invalidated before install because
  Host restart could not surface one durable started dispatch and two current
  summaries were stale. Final13 closes that restart path without a second model
  call, passes the complete 233-Rust/56+86-Swift matrix and synchronized
  Developer-ID App/DMG, but both fresh reviewers invalidate it on the sole
  remaining P2: this paragraph still named final12 current. Final14 changes
  only this stale paragraph. Its complete 233-Rust/56+86-Swift matrix and
  synchronized Developer-ID App/DMG pass, with manifest `69a28c4f…`, exact
  App/Core/broker/worker CDHashes, mounted/copied DMG `1ca74ccd…`, and honest
  `Unnotarized Developer ID`. Two fresh reviewers, one batched re-install, the
  chronological same-build provider story, commit/push/CI, and
  `FRIDAY_ALPHA_READY` remain pending.
  Outcome, notarization, clean install, product E2E, external-user evidence,
  and release proof do not yet exist.
- `FRIDAY_ALPHA_READY` is not yet earned. The local adapters, pairing,
  dedupe/cursor, once-only dispatch, Off shutdown, full suite, and ad-hoc DMG
  install test passed before both closure reviewers rejected fingerprint
  `136a42ba…`. Repair1 closed the reported atomic queue, typed readback,
  Discord setup/probe, signed running-imsg, positive compile-source, resource,
  status, origin-authority, and notice routes, but both replacement reviewers
  rejected fingerprint `10160bb1…` on iMessage prefix/identity and prepared
  session lifecycle routes. Both Repair2 reviewers then rejected fingerprint
  `1a983c72…` on history-to-Sent misbinding, prepare response loss, missing
  product chat selection, and stale ledger state. Repair3 closes only those
  approved blockers; its current 190-Rust/93-Swift suite, fresh pinned imsg v4
  build, two Codex diagnostics, independent notice closure, and ad-hoc App/DMG
  install verification pass. Two entirely fresh replacement reviewers PASS
  frozen fingerprint `3e201547…` with zero P0/P1/P2. Reviewed commit
  `2685b572715dff3e1360de66ab4c2ab6c013730b` is pushed to draft PR #2;
  Actions run `29440208503` passes on synthesized merge `99ee2b10…`, whose
  tree `730bce09…` equals the exact head tree. This is integration plumbing,
  not provider or release proof. Real bidirectional iMessage/Discord plus
  GPT/Reminders traffic, notarization, administrator approval, and real channel
  credentials are external-authority gates and will not be fabricated. The
  historical Developer-ID v3 failed its first governance review. Both v4
  reviewers then rejected fingerprint `08a58745…` on mode, exact certificate,
  and post-sign binding routes. The v5 closure candidate passes the focused
  replacement checks and two fresh reviewers PASS fingerprint `fdf5a00e…` with
  zero P0/P1/P2. The provenance-bound replacement App/DMG is rebuilt at DMG SHA
  `b7f3e718…`, but its first final-evidence review found one dynamic provenance
  sentence and invalidated that fingerprint. Final2 removes only that dynamic
  sentence, embeds provenance SHA `155aa65a…`, and produces DMG SHA
  `7c022b83…`. Two entirely fresh replacement reviewers PASS unchanged
  fingerprint `026b2b1f…` with zero P0/P1/P2; reviewed commit `5a461ef…` is
  pushed to draft PR #2 and Actions run `29450863581` passes on equal-tree
  synthesized merge `da3d7d1…`. It remains unnotarized; no provider,
  admin/cross-UID, `FRIDAY_ALPHA_READY`, or release proof exists yet.

## Unclaimed capabilities

Until the corresponding same-SHA evidence is linked here, every product route
is `implementation_in_progress`, not production-ready.
