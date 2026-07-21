# OpenOpen Private Agent Choice Loop Design

Status: `OWNER_APPROVED_DIRECTION`

Owner approval source: the Primary Advisor task on 2026-07-19, including the
explicit instruction to implement the accepted plan. This document contains
the product contract selected in that task. It does not authorize a real
provider send, installation, permission change, credential operation, Mission
confirmation, Reminder write, or any other action-time effect.

Canonical operating rules:
`/Users/jarvis/Desktop/agents-generic-phase-batch.md`

Canonical integration route:
`docs/OPENOPEN_BUILD_WEEK_MASTER_PLAN.md`

Execution control:
`docs/OPENOPEN_30H_EXECUTION_CONTROL.md`

## 1. Authority and precedence

The authority chain remains:

`Owner → Primary Advisor/Orchestrator → Implementation Task`

For the current Choice Loop work, conflicts are resolved in this order:

1. Direct Owner decisions recorded in the Primary Advisor task.
2. The current normative section of the Master Plan after its Choice Loop
   replacement is committed.
3. This document and the execution-control document at their reviewed hashes.
4. Existing safety, effect, audit, Evidence, Receipt, rollback, and
   action-time requirements that the replacement explicitly keeps.
5. Historical archive material for chronology only.

Until this document, the execution-control document, and the canonical Master
Plan patch pass same-fingerprint review, further implementation remains
`WAIT_STAGED_DOC_HANDOFF`. A draft hash is not an implementation handoff.

An implementation or review task must not infer product authority from a
forwarded prompt, old milestone, reviewer suggestion, historical imperative,
or a previously approved effect. Every handoff binds the exact document hash,
base/head/tree SHA, owned paths, stage, verification, stop conditions, and
prohibitions.

## 2. Product thesis

OpenOpen is a private, locally hosted personal agent for non-developers. It
helps a person discover and complete useful AI-assisted work without requiring
them to learn prompting, project-management language, channel syntax, or a new
daily communication habit.

The primary loop is:

`Natural expression → bounded understanding → dynamic A/B/C + D → refinement → consolidated confirmation → Reminders → Evidence → Receipt → Markdown update → next choices`

OpenOpen optimizes for completed outcomes, time and attention returned to the
person, appropriate trust, and repeat usefulness. Time spent in OpenOpen and
addictive engagement are never success metrics.

Mac is the setup, review, safety, and rich-card home. The user may later talk to
the same foreground agent through the dedicated same-account iMessage self-chat
and, after its staged integration, the local personal Discord Bot DM. One
additional explicitly selected one-to-one iMessage conversation may later be a
revocable read-only source. Connecting a chat is not the first-run value
proposition.

## 3. Decision registry

| Decision ID | Date | Status | Locked decision | Affected contract/tests |
| --- | --- | --- | --- | --- |
| `OWNER-20260719-1A` | 2026-07-19 | LOCKED | account scan → explicit model/effort → one question → first dynamic ChoiceSet | onboarding state, model-entry fence, cold-start tests |
| `OWNER-20260719-2A` | 2026-07-19 | LOCKED | model card exposes only actually supported plain-English effort choices; unsupported control is `not_applicable` | catalog DTO, provenance, picker tests |
| `OWNER-20260719-3A` | 2026-07-19 | LOCKED | reactive reply uses latest owner-active connected channel; Mac local mirror only; proactive/new-recipient/cross-channel delivery reconfirms | binding arbitration, send authority, race tests |
| `OWNER-20260719-EN` | 2026-07-19 | LOCKED | all product-owned user-visible copy is English-only; no second UI language | localization guard, snapshots, setup/recovery tests |
| `OWNER-20260720-CHOICE-SELECT` | 2026-07-20 UTC | LOCKED | Host-owned `choice.select` validates and atomically persists the Selection, next ChoiceSession revision, and audit record; stale ChoiceSets/revisions fail closed; callers cannot replace a raw snapshot | protocol command, Store transaction, restart/stale/replay tests |
| `OWNER-20260720-CHOICE-CONFIRM` | 2026-07-20 UTC | LOCKED | dedicated `choice.confirm` command owns the consolidated Choice confirmation; legacy `mission.confirm` cannot satisfy or alias it | immutable confirmation payload, Store/audit, exact-drift and legacy-route rejection tests |
| `OWNER-20260720-BATCH-BINDING` | 2026-07-20 UTC | LOCKED | every ConversationTurnBatch durably binds the first authenticated SourceEnvelope delivery binding; every later envelope must match; historical missing binding blocks without inference | batch schema/migration, Host derivation, mismatch/restart tests |
| `OWNER-20260720-PERSONA-PR1` | 2026-07-20 UTC | LOCKED | the reviewed default Persona bundle is a PR1 technical migration: exact content digests, verified local revision storage, protocol/model-operation provenance, audit/replay behavior, and non-executable boundaries are required; it cannot grant authority or effects, and PR1 has no mutable Persona stage/activate/rollback RPC | bundle/signature/layout/tamper/downgrade tests; Choice and model-request revision binding; public-route rejection |
| `OWNER-20260720-CHOICE-BEGIN` | 2026-07-20 UTC | LOCKED | Host-owned `choice.begin` is the sole first-local-question intake/create route; Host derives the authenticated Mac envelope and sealed batch, atomically creates initial session/audit state, and privately commits only a current revision-bound first ChoiceSet | cold-start RPC, Store transaction, late/stale/model-drift and no-effect tests |
| `OWNER-20260720-CHOICE-D-SELECT` | 2026-07-20 UTC | LOCKED | D uses a command-owned `choice.select` request variant carrying bounded untrusted text and an idempotent request ID, never a caller-created batch ID; Host authenticates the caller, derives and seals the D envelope/batch, and atomically commits Selection/session/audit | D continuation RPC, same-binding/batching/replay/restart/Off tests |
| `OWNER-20260720-REFINEMENT-RESULT` | 2026-07-20 UTC | LOCKED | every post-selection model result enters only through a private refinement-result commit bound to Selection, operation, generation, expected session/interpretation revision, model/catalog/protocol provenance, source manifest, and audit anchor | private Host/Store transition, exact replay, late/Off/drift tests |
| `OWNER-20260720-MARKDOWN-RENDER` | 2026-07-20 UTC | LOCKED | Markdown rendering uses a Store-owned render intent, descriptor-safe staging, staged-file sync, atomic same-directory rename, parent-directory sync, final digest verification, and only then an exact render receipt; restart either adopts the exact intended bytes or enters typed reconciliation, never guesses or treats Markdown as authority | render-intent/receipt schema, crash matrix, semantic-diff/reconfirmation tests |
| `OWNER-20260720-IDLE-STALE` | 2026-07-20 UTC | LOCKED | deterministic 30-minute soft-idle and 24-hour stale-review transitions are Host-owned internal commands using persisted deadlines, expected revision/generation, and Store transactions; timers are hints only and never create model/effect authority | scheduler/restart/idempotency/late-timer/Off tests |
| `OWNER-20260720-24H-CORE-A` | 2026-07-20 UTC | SUPERSEDED_SCHEDULE | established PR1/PR2/Core checkpoint as the protected path and preserved every safety/merge gate; its 24-hour pacing and permanent support-lane freeze are replaced by `OWNER-20260720-16H-FULL-FIRE` | historical dependency decision; retained safety gates |
| `OWNER-20260720-16H-FULL-FIRE` | 2026-07-20 UTC | SUPERSEDED_SCHEDULE | retained Core-first, lane/resource, and exact-node deferred-Owner safety; broader post-Core order and sixteen-hour schedule replaced by the ten-hour B+ route | historical schedule; retained safety/liveness gates |
| `OWNER-20260720-B2-DYNAMIC-CARDS-CONSENT` | 2026-07-20 UTC | LOCKED | B2 previews dynamic Memory candidate cards; automatic work is local/no-network, and only a later exact Owner consent may send bounded source excerpts to the explicitly selected OpenAI model; only Owner-selected cards may reach a confirmed Markdown diff | preview-session schema, data-disposal, provider-consent, selection/revision tests |
| `OWNER-20260720-IMSG-ONE-READONLY` | 2026-07-20 UTC | LOCKED | V1 admits at most one additional individually selected and revocable one-to-one iMessage source; it is read-only and can never acquire outbound authority | binding cardinality, revoke/restart/stale tests, outbound rejection |
| `OWNER-20260720-DESIGN-AFTER-FUNCTION` | 2026-07-20 UTC | LOCKED | advanced visual composition, new Persona behavior, final English copy, density, and animation stay Owner-open until the functional staged integrations and offline package are ready; the reviewed default Persona bundle's technical migration is the narrow PR1 exception | neutral semantic UI fixtures, verified default Persona bundle, later Owner design registry |
| `OWNER-20260720-24H-CLOSURE-QUEUE` | 2026-07-20 UTC | SUPERSEDED_SCHEDULE | retain the deduplicated return queue and exact-node liveness behavior; twenty-four-hour scheduling is replaced by the ten-hour B+ route | execution liveness, Owner-return queue, design decision packets |
| `OWNER-20260720-REMINDER-SCHEDULE-BG` | 2026-07-20 UTC | LOCKED | visible/editable Reminder scheduling derives only from explicit user temporal information; missing time requires user selection, never a fixed default or question-time inference; exact future date/time/timezone/list/count bind confirmation and every edit reconfirms; real write remains separately gated | proposal validation, future/timezone tests, digest/revision drift, action-time write separation |
| `OWNER-20260720-14H-DEMO-CORE-B2-C2` | 2026-07-20 UTC | SUPERSEDED_SCHEDULE | retain exact Core/B2/C2 cardinality, action-time gates, and narrow UI bounds; fourteen-hour schedule and co-equal Demo narrative are replaced by B+ | retained bounded acceptance and UI scope |
| `OWNER-20260720-14H-DEMO-IMSG-INCLUDE` | 2026-07-20 UTC | SUPERSEDED_NARRATIVE | retain PR2 same-account self-chat scope and every permission/selection/install/send gate; Demo naming is replaced by the B+ Hero checkpoint | retained PR2 acceptance and channel gates |
| `OWNER-20260720-10H-BPLUS-HERO` | 2026-07-20 UTC | LOCKED | retains PR1+PR2 Core+iMessage as the independent Hero completion gate for one real verified outcome loop, followed by bounded B2 then bounded C2 in the final B+ package | Hero checkpoint, B+ dependency graph, narrow narrative, post-B+ quarantine |
| `OWNER-20260720-10H-BPLUS-DEADLINE` | 2026-07-20 UTC | LOCKED | ten hours is the latest-safe B+ delivery target and execution deadline, never a gate bypass; any exact Owner action, external outage, or normal-merge rejection that threatens it is reported immediately rather than silently extending the plan | pacing, immediate notification, retained safety and action-time gates |

These decisions supersede only contradictory draft wording from the initial
Choice Loop document review. They do not alter effect, privacy, Evidence,
Receipt, Off, or action-time gates.

### 3.1 LOCKED

- A ChoiceSet contains exactly three dynamically generated direction choices
  plus D. A/B/C are not fixed product categories.
- D is always available as natural conversation/free description.
- Selecting A/B/C/D narrows or refines intent. It is never Mission or effect
  authority.
- Selection is accepted only through the Host-owned `choice.select` command.
  A/B/C names one current option. D instead carries bounded untrusted text, an
  idempotent request ID, and expected persisted session/ChoiceSet/model/catalog/
  protocol references; it never accepts a caller-created or reused batch ID.
  Host authenticates the caller, derives the SourceEnvelope and delivery
  binding, creates and seals the D batch under the existing quiet/hard-cap
  rules (an explicit complete Mac submission may seal its one-message batch),
  then atomically persists the exact Selection, next ChoiceSession revision,
  operation, and audit record. Exact replay returns the existing operation;
  changed replay, binding/revision drift, Off, cancel, or a retired ChoiceSet
  fails closed. No caller receives a raw whole-snapshot replacement path.
- `choice.begin` is the only public route that accepts the first bounded local
  question. Host authenticates the Mac caller, derives rather than trusts the
  SourceEnvelope/delivery binding, seals the one-question batch, verifies the
  exact persisted model/catalog/protocol provenance, and commits the initial
  `interpreting` ChoiceSession plus audit in one `IMMEDIATE` transaction. A
  model runs only after that commit. Its result enters through one private
  operation/generation/session-revision/provenance-bound commit; stale or late
  results are discarded. The public response never contains a raw snapshot
  writer, and neither begin nor result commit grants an external effect.
- There is exactly one global foreground ChoiceSession across the active Mac
  and iMessage self-chat surfaces. A later Discord phase must reuse this same
  invariant rather than create another foreground session. Confirmed Missions
  may continue in the background.
- A 30-minute idle period is soft idle, not expiry. A Host-owned internal
  transition uses the persisted deadline plus expected session revision and
  generation to atomically enter `softIdle`; returning recalibrates and
  refreshes the ChoiceSet while keeping the session.
- At 24 hours the same deterministic Host/Store mechanism enters
  `staleReview`, retires the old ChoiceSet, and records neutral recap inputs.
  Timers only request the transition; Store state is authority. The next
  authenticated owner re-entry starts any bounded recap/refresh model work.
  No timer, old choice, or offline message starts model/effect work itself.
- Explicit finish/cancel/new-topic closes, cancels, or switches the foreground
  session through a persisted revisioned transition.
- Same-surface short messages use an approximately 2.5-second quiet window and
  an 8-second hard cap. Attachments or an explicit continuation may extend only
  within the hard cap. Off, cancel, and confirm bypass batching.
- Every batch has one required durable delivery binding. Host derives it from
  the first authenticated SourceEnvelope and admits later envelopes only when
  their Host-derived binding matches exactly. A restart preserves the binding;
  a historical batch without it enters typed blocked recovery and is never
  repaired by identity, provider ID, body, timing, or source-ID inference.
- Dedicated private surfaces treat each owner message as addressed to
  OpenOpen. A summon word or phrases such as “next?” refresh choices; they are
  not an address gate.
- The account catalog is scanned for visible, protocol-compatible GPT/Codex
  models. On first launch, OpenOpen presents that scan before any model work.
  The user explicitly selects a model and one of that model's actually
  supported effort levels. There is no Auto route, fixed Sol requirement,
  silent fallback, guessed future-model tier, or hidden effort default.
- The selected model, requested/actual effort, catalog revision/fingerprint,
  session revision, Mission revision, and Receipt provenance are durably bound.
- A compatible model with no user-controllable effort surface uses typed
  `not_applicable`; it is not excluded and no effort value is invented.
- First launch follows this fixed semantic sequence: English-only welcome and
  account capability scan → explicit model-and-effort choice → one simple
  high-value question → the first dynamic A/B/C plus D. No dynamic ChoiceSet or
  other model-generated content exists before model-and-effort selection.
- Every user-visible UI label, option, state, help message, setup instruction,
  notification, and recovery explanation is English-only. OpenOpen must not
  display a second language. User-authored and imported content remains in its
  original language and is data, not product UI copy.
- For an already connected interactive channel, a reactive reply to an owner
  message may return without per-message confirmation. It returns only on the
  most recently accepted owner-active interactive channel. Mac mirrors local
  session state but does not copy the reply into another channel. Proactive
  delivery, a new recipient, or any cross-channel delivery requires a new
  exact confirmation.
- Reminders is the first external work effect and is implemented end-to-end
  before another effect family is claimed.
- User-visible understanding and task continuity use bounded plaintext
  Markdown under `~/Documents/OpenOpen`. Security authority, secrets,
  credentials, grants, audit anchors, and effect state remain in Store/Keychain.
- External Markdown edits never execute. Digest drift produces a semantic diff
  and requires acceptance/reconfirmation before dependent effects continue.
- iMessage groups are excluded, and unrelated Discord events are rejected
  before body persistence or model access.
- Competition V1 remains OpenAI-only. Claude/Anthropic is not integrated or
  claimed. The Owner-supplied real ChatGPT export may be used automatically
  only for an in-place local/read-only/no-network/no-retention B2 diagnostic
  with zero repository or retained derived data. Mainline B2 uses synthetic
  fixtures until the Owner later confirms the exact provider-processing scope;
  no source excerpt reaches a model before that consent.

### 3.2 KEEP

- Single command-owned Mission persistence and atomic state/audit commits.
- Typed, digest-bound effect authorization; changed payload, recipient, data,
  time, list, count, or scope requires a new confirmation.
- Evidence-before-Done and Evidence-backed Receipt.
- Global Off, generation fences, late-result discard, restart recovery,
  durable dedupe, and no provider replay.
- Repair24 stable incident identity, durable acknowledgement, non-blocking
  activity, and reachable Dashboard/Settings/Off controls.
- Protected broker, Keychain, root-owned workspace boundaries, no cloud, no
  central telemetry, and no silent fallback.
- The two historical terminal provider dispatches remain terminal and must
  never retry, resend, edit, delete, or reopen.

### 3.3 RETIRED FROM CURRENT PRODUCT AUTHORITY

- Fixed `gpt-5.6-sol`, Auto model routing, and model/effort substitution.
- `Connect Messages` as the first screen or first value proposition.
- One input → one `OutcomeSuggestion` as the primary product experience.
- The 15-minute per-input `IntentSession`, the two-message correction cap as
  the main conversation mechanism, and at-most-one-question as a universal UX.
- Fixed Hero A/B/C category semantics for A/B/C choices.
- Quick Passport, Deep ZIP, proactive suggestion, GitHub Skill, and Workflow
  Candidate as the Choice Loop critical path. Deep ZIP and Skills remain
  isolated supporting lanes and may not redefine shared Choice contracts.
- Arbitrary conversation selection, groups, ambient observation, Slack,
  shared/cloud Discord Bot, Discord reading unrelated DMs, third-party message
  authority, and automatic processing of offline backlog.
- Marketplace/search/ratings, arbitrary Skill scripts, automatic Workflow
  execution, Private Memory, `MEMORY.md` export, Hero B/C, and mobile UI in the
  current slice.
- Repair23 Dashboard input as Repair24 or Choice Loop proof.

### 3.4 OWNER_OPEN

The implementation task must provide neutral semantic fields and fixtures but
must not decide the following:

1. First-screen visual composition, card density, and D presentation within
   the locked first-launch sequence and English-only product-language rule.
2. Final confirmation-card wording, visible fields, and inline-edit details.
3. Which CommunicationProfile dimensions are directly visible or adjustable.
4. New Persona behavior, humor, tone, pacing, final copy, and visual system;
   the reviewed default Persona bundle's technical migration is locked PR1
   scope, not a license to author a replacement voice.
5. Notification wording/frequency inside an approved direction.
6. Mission-in-progress screen hierarchy and new-topic presentation.
7. Simultaneous-input visual treatment and the English wording of the locked
   latest-owner-active-channel rule; recipient authority itself is not open.
8. The exact user-facing 30-minute/24-hour recap copy.
9. User-facing incident/error/recovery language and technical-detail placement.
10. Final iMessage and Discord setup presentation after their contracts are
    safe and testable.

The prior D-intake architecture item is resolved by
`OWNER-20260720-CHOICE-D-SELECT`; only its Owner-open visual presentation
remains part of item 1. Mac cannot fabricate or reuse a batch identity, and no
raw snapshot writer may be introduced.

Each Owner decision is appended with date, status, affected semantic fields,
affected tests, and superseded decision IDs. An implementation lane waits on
only the affected presentation item and continues unrelated READY work.
During autonomous work it must also prepare, without choosing, one bounded
decision packet for each of the ten items above once its functional dependency
exists. The packet contains current neutral structure, two or three safe
alternatives, affected semantic fields/tests, and the smallest follow-on diff.
Those packets are handled with the Owner in the closure window and do not block
unrelated engineering.

### 3.5 TEN-HOUR LATEST-SAFE B+ DELIVERY

The protected Hero sequence is PR1, PR2 same-account self-chat, and the
same-main Core+iMessage checkpoint App/DMG. That checkpoint must independently
prove one complete real loop: natural input → dynamic A/B/C+D → editable exact
confirmation → Reminder → readback/Evidence → Receipt → Markdown → next choices,
including Off, restart recovery, and duplicate-effect prevention. Minimal B2
and then minimal C2 are narrow proof chapters in the final same-main B+ App/DMG;
they do not redefine or delay the Hero checkpoint. Ten hours is the latest-safe
delivery target and execution deadline, never authority to skip a safety,
privacy, review, CI, merge, permission, effect, or release gate. The following
post-B+ lanes may prepare only inside isolated owned paths and may not change
shared Choice authority, block B+, or claim real provider/effect proof:

- one additional, individually selected and revocable one-to-one iMessage
  read-only source; groups remain rejected and outbound stays impossible;
- broader B2 Dynamic Memory beyond one import, at most three candidates, one
  selected card, and one confirmed Markdown diff;
- broader C2 Skills beyond one public instruction-only Skill's acquisition,
  audit, enablement, and one no-external-effect use; and
- PR3 local personal Discord Bot DM with synthetic token/events only during
  autonomous implementation.

After the Hero checkpoint, the Integrator lands minimal B2 and then minimal C2
before the final B+ App/DMG. Each retains its own Scout, two-reviewer, matrix,
CI, ordinary merge, rollback, and action-time gates. The extra iMessage source,
Discord, broader expansion, complex animation, persona, product-wide final
copy, and full visual system are post-B+. Neutral English-only accessible
structure remains required meanwhile.

## 4. Core contracts

The shared protocol and Store introduce versioned, strict types. Unknown enum
variants, missing required provenance, stale revisions, and cross-session IDs
fail closed.

```text
SourceEnvelope {
  id, surface, deliveryBindingId, providerMessageId?, ownerId,
  receivedAt, monotonicSequence, bodyDigest, attachmentManifest?,
  thirdPartyData, sessionHint?, schemaVersion
}

ChoiceBeginRequest {
  requestId, boundedLocalQuestion, expectedModelProvenanceRef,
  expectedCatalogRevision, expectedProtocolRevision
}

ChoiceBeginAccepted {
  requestId, operationId, choiceSessionId, acceptedSessionRevision,
  sourceEnvelopeId, conversationTurnBatchId, state=interpreting
}

ChoiceSelectRequest = Option {
  requestId, choiceSessionId, choiceSetId, selectedOptionId,
  expectedSessionRevision, expectedModelProvenanceRef,
  expectedCatalogRevision, expectedProtocolRevision
} | NaturalConversation {
  requestId, choiceSessionId, choiceSetId, boundedDText,
  expectedSessionRevision, expectedModelProvenanceRef,
  expectedCatalogRevision, expectedProtocolRevision
}

ChoiceRefinementResult {
  operationId, selectionId, generation, choiceSessionId,
  expectedSessionRevision, baseInterpretationRevision,
  interpretationFrame, choiceSet, modelProvenance,
  sourceManifestDigest, auditAnchor
}

ChoiceDIntakeRecord {
  requestId, requestDigest, choiceSessionId, choiceSetId,
  encryptedBodyRef, sourceEnvelopeId, conversationTurnBatchId,
  expectedSessionRevision, expectedModelProvenanceRef,
  expectedCatalogRevision, expectedProtocolRevision, state
}

ConversationTurnBatch {
  id, choiceSessionId, deliveryBindingId, sourceEnvelopeIds[], openedAt,
  quietDeadline, hardDeadline, sealedAt, sealReason, revision
}

ChoiceSession {
  id, state, revision, modelSelectionState, communicationProfileRevision,
  activeChoiceSetId?, activeInterpretationRevision?, openedAt,
  lastInputAt, softIdleAt, staleReviewAt, primaryDeliveryBindingId?,
  pendingConfirmationId?, backgroundMissionIds[]
}

ChoiceSessionState = interpreting | active | refining | softIdle | staleReview |
                     awaitingConfirmation | executing | completed |
                     cancelled | blocked

ChoiceSet {
  id, choiceSessionId, sessionRevision, interpretationRevision,
  generatedAt, expiresOnRevision, options[3], dAvailable,
  sourceManifestDigest, modelProvenance
}

ChoiceOption {
  id, position, direction, rationale, expectedResult,
  informationNeeded[], externalEffectsPreview[], sourceCategories[]
}

Selection = OptionSelection {
  id, choiceSessionId, choiceSetId, selectedOptionId,
  expectedSessionRevision, selectedAt
} | NaturalConversationSelection {
  id, choiceSessionId, choiceSetId, dInputBatchId,
  expectedSessionRevision, selectedAt
}

ChoiceConsolidatedConfirmation {
  id, choiceSessionId, choiceSetId, expectedSessionRevision,
  interpretationRevision, payloadRevision, payloadDigest, goal, steps[],
  markdownManifestDigests[], documentDiffDigest, modelProvenance,
  reminderPayload, evidenceRequirements[], deliveryBindingId?, recipient?,
  deliveryScope?, dataCategories[], retention, permissions[], effectClasses[],
  confirmedAt
}

ModelSelectionState = unselected | selected {
  modelProvenanceRef
} | unavailable {
  catalogRevision, reason
}

InterpretationFrame {
  choiceSessionId, revision, understoodGoal, currentContext,
  assumptions[], constraints[], uncertainties[], whatToAvoid[],
  sourceManifestDigest
}

UnderstandingPatch {
  choiceSessionId, baseInterpretationRevision, operations[],
  explanation, requiresConfirmation
}

CommunicationProfile {
  revision, explicitPreferences[], inferredPreferences[],
  corrections[], perSurfaceOverrides[], updatedAt
}

OutcomeDecision {
  choiceSessionId, interpretationRevision, goal, steps[],
  requiredInformation[], requiredEffects[], completionEvidence[],
  modelProvenance
}

DocumentManifest {
  rootVersion, entries[{relativePath, sha256, byteLength, mode}],
  aggregateDigest, generatedAt
}

DocumentDiff {
  baseManifestDigest, observedManifestDigest, semanticChanges[],
  securityRejections[], requiresConfirmation
}

DocumentRenderIntent {
  id, choiceSessionId, expectedSessionRevision, baseManifestDigest,
  desiredManifest, typedStateDigest, renderSchemaRevision, state
}

DocumentRenderReceipt {
  intentId, desiredManifestDigest, observedManifestDigest,
  atomicRenameCompletedAt, directorySyncCompletedAt, state
}

ChoiceDeadlineCheck {
  requestId, choiceSessionId, expectedSessionRevision, expectedGeneration,
  expectedDeadlineRevision
}

ChannelDeliveryBinding {
  id, surface, role, ownerIdentity, conversationIdentity,
  allowedDirection, consentRevision, cursor, generation, state
}

ReadOnlyConversationBinding {
  id, channelDeliveryBindingId, ownerIdentity, conversationIdentity,
  allowedDirection=inboundOnly, consentRevision, cursor, generation, state
}

DeepZipPreviewSession {
  id, state, revision, ephemeralSourceRef, catalogRevision,
  selectedModelProvenance?, providerProcessingConsentId?,
  activeCandidateCardIds[], expiresAt, disposalState
}

MemoryCandidateCard {
  id, previewSessionId, previewRevision, title, candidateText,
  ephemeralSourceRefs[], modelProvenance?, state
}

MemoryImportDecision {
  id, previewSessionId, expectedPreviewRevision, selectedCardIds[],
  rejectedCardIds[], customDInput?, markdownDiffDigest,
  providerProcessingConsentId?, decidedAt
}
```

`SourceEnvelope.deliveryBindingId` and `SourceEnvelope.ownerId` are derived by
Host from an authenticated `ChannelDeliveryBinding`; an adapter or message body
cannot supply or override them. Message bodies and attachment metadata remain
untrusted data.

`ConversationTurnBatch.deliveryBindingId` is required persisted state, not a
derived display value. Host copies it from the first accepted authenticated
`SourceEnvelope` and rejects any later envelope whose Host-derived binding is
different. Missing historical values migrate to typed blocked recovery; no
field combination is permitted to infer the missing authority.

`ModelProvenance` has a stable ID and binds exact model ID, requested and actual
effort (`not_applicable` when the selected model exposes no effort control),
complete catalog fingerprint/revision, account/workspace display class,
protocol schema revision, and turn identity. Missing historical model
provenance migrates to a typed blocked state; it never defaults to a model.

`ReadOnlyConversationBinding` has cardinality one outside the dedicated
self-chat binding. It can produce untrusted inbound envelopes only and cannot
be converted into an outbound recipient. `DeepZipPreviewSession` and every
ephemeral source/card reference are private local state and are destroyed on
cancel, failure, expiry, or completion. After confirmation, only selected-card
content, its digest/provenance, the confirmed semantic Markdown diff, and body-
free audit state remain; source paths, raw members, unselected cards, catalog
bodies, and temporary derivatives do not.

## 5. Choice generation and conversation

- `choice.begin` is the sole first-local-question intake/create command. Its
  request carries only bounded untrusted question data, an idempotent request
  ID, and the caller's expected persisted model/catalog/protocol references;
  it cannot supply trusted owner, delivery-binding, envelope, batch, session,
  audit, or ChoiceSet fields. Host derives and commits those trusted fields.
- An exact replay returns the already accepted operation identity. A changed
  payload under the same request ID, missing model selection, catalog drift,
  an existing unresolved foreground session, Off, or stale protocol state
  fails closed with a typed recovery state and no model call.
- The first ChoiceSet is stored only by an internal Host result commit bound to
  the accepted operation ID, current generation, exact session revision,
  selected model/catalog/protocol provenance, and accepted source manifest.
  No adapter, Swift caller, or model can call that commit or save a snapshot.

- A/B/C must be materially distinct next directions grounded in the current
  InterpretationFrame, confirmed Mission/Receipt history, accepted Markdown,
  changed information, pending steps, and safe alternative paths.
- When prior task history exists, A/B/C preferentially expose the three most
  useful current directions: an update that needs new owner information, a
  prepared next step that can be reviewed, or a materially different safe
  path. They are still generated from current state and are never fixed
  categories or engagement bait.
- A/B/C do not need to be executable actions. When execution is not yet safe,
  valid directions include reviewing known context, supplying missing
  information, narrowing scope, or pausing. The schema still contains exactly
  three choices plus D.
- Each option exposes semantic fields for rationale, expected result,
  information needed, and external-effect preview. OWNER_OPEN presentation may
  show a subset, but the fields remain available for review and accessibility.
- D supports continuing natural conversation for as many turns as useful. It
  is not limited to one clarification.
- A model response contains a strict InterpretationFrame/patch, response plan,
  and exactly three choices. D is product-owned and cannot be removed or
  renamed by model output.
- The Host validates source IDs, revisions, option count/distinctness, effect
  previews, bounded lengths, and model provenance before committing a new
  ChoiceSet.
- `choice.select` is the only production selection write route. It validates
  the active ChoiceSet and exact expected revision. A/B/C names one current
  option. D supplies bounded text and a request ID; Host derives and seals its
  authenticated same-binding batch. For every A/B/C/D variant, one Store-owned
  SQLite `IMMEDIATE` transaction persists the Selection, creates the pending
  refinement operation, advances the exact session revision/state, and appends
  audit; D includes its encrypted intake/envelope/batch/request registry in that
  same transaction. No committed Selection may exist without its durable
  pending operation. Exact replay is idempotent and changed replay or stale/
  cross-session data fails closed. It cannot authorize a Mission, Reminder,
  delivery, or other effect.
- An explicit complete Mac D submission uses one SQLite `IMMEDIATE`
  transaction to persist the request-digest registry, Keychain-master-key
  encrypted body, Host-derived envelope, sealed one-message batch, Selection,
  pending refinement operation, next session revision/state, and audit. For a
  future multi-envelope dedicated surface, the same command-owned operation
  persists each encrypted envelope plus the open batch transactionally; a
  private Host deadline check atomically seals that persisted batch together
  with Selection, pending refinement operation, session revision, and audit.
  Therefore crash recovery sees either a persisted collecting batch whose
  original deadlines resume, or the complete selected operation—never an
  orphan seal or model-eligible partial Selection. Exact request replay returns
  the persisted collecting/selected phase; a changed request digest fails.
- First-question and D plaintext are Store-private encrypted blobs using the
  existing Keychain-derived Store encryption boundary, with AAD binding the
  request/session/envelope/batch identity. Only bounded active model-brief
  construction may decrypt them. Cancel deletes pending raw blobs; successful
  refinement plus the accepted typed-state Markdown render receipt deletes the
  raw blob and retains only accepted typed state plus body-free digest/audit
  tombstones. Restart retains encrypted text only while its operation is
  recoverable. Transient plaintext buffers are bounded and zeroized where the
  runtime supports it; no log, evidence, Receipt, or remote contains the body.
- The atomic `choice.select` transaction creates the private refinement
  operation for every A/B/C/D Selection. Only its exact
  `ChoiceRefinementResult` can advance `refining`; an exact retry
  returns the committed result, while changed Selection, revision/generation,
  source manifest, model/catalog/protocol provenance, Off, cancel, or a late
  result fails closed without a model retry or effect.
- The private refinement-result Store call uses one SQLite `IMMEDIATE`
  transaction to verify and complete the pending operation, persist the result
  digest and encrypted InterpretationFrame/new ChoiceSet, advance the exact
  session revision/state, and append its audit anchor. There is no intermediate
  model-eligible or UI-visible result state.
- Choice preparation compiles a bounded `MODEL_BRIEF.md` from typed task state,
  decisions, questions, path notes, updates, and accepted source manifests.
  Every compatible selected model receives the same semantic brief contract;
  no model depends on hidden session memory or a model-specific prompt trick.
- A ChoiceSet is stale immediately when the ChoiceSession revision, accepted
  Markdown manifest, selected model, source consent, or InterpretationFrame
  revision changes.

## 6. Batching and cross-surface arbitration

- Only messages whose Host-derived delivery binding exactly matches the
  batch's required persisted `deliveryBindingId` share a quiet-window batch.
  The first authenticated envelope establishes that value; an adapter cannot
  set it and a later envelope cannot change it.
- Cross-surface envelopes are deduped only when they carry the same durable
  OpenOpen idempotency or reply-correlation ID. Provider ID, identity, body,
  digest, or timing similarity alone is not cross-surface identity proof.
  Without the durable correlation, both envelopes are preserved, ordered by
  Host monotonic sequence, and explicitly arbitrated in the same foreground
  session.
- A new owner turn arriving during model generation advances the session
  revision, retires the late result, and recompiles bounded context. It never
  creates a second simultaneous foreground session.
- A conversational reply returns only to the most recently accepted
  owner-active interactive channel. Sending from another connected channel is
  the user's explicit channel switch. Mac mirrors local session state but does
  not copy the reply into another channel, and OpenOpen never broadcasts.
  Simultaneous inputs retain their source bindings and advance one monotonic
  ChoiceSession; stale outputs are retired. A proactive delivery, new
  recipient, or cross-channel delivery requires an exact confirmation.
- Third-party source messages are untrusted data. They may update bounded
  context only under active consent and never select, confirm, send, or act.

## 7. Understanding and communication learning

- Understanding is an explicit revisioned frame, not hidden free-form memory.
- The model receives only manifest-listed, bounded context compiled by Host.
  It cannot roam the filesystem or write Markdown.
- Explicit communication preferences apply immediately. Inferred preferences
  require repeated consistent evidence; the supporting observations and
  confidence are retained so the user can inspect or correct them.
- A correction wins over inference. Forget/revoke removes the inferred rule
  and its future use.
- CommunicationProfile may adapt English word choice/register, directness,
  response length, pacing, formatting, and explanation depth. It never changes
  the English-only product output language or locale, approval, recipient,
  data, effect, retention, model, or evidence authority.
- Sensitive traits, emotional diagnosis, health/financial/legal inference,
  and claims about a person's identity are not CommunicationProfile fields.

## 8. Consolidated confirmation and autonomy

The Host exposes one dedicated `choice.confirm` command for the consolidated
Choice confirmation. It does not reuse, call through, or accept evidence from
the legacy `mission.confirm` route. The command binds one immutable payload
digest containing:

- interpreted goal and ordered steps;
- exact Reminder list, item text, due dates/times/time zone, item count, and
  completion Evidence expectations;
- exact Markdown paths plus base/observed manifest digests and semantic diff;
- selected model/effort/catalog/protocol provenance;
- any conversational delivery binding and exact external recipient/scope;
- data categories, retention, permissions, and effect classes.

Host validates the active ChoiceSession/ChoiceSet and expected revisions, then
atomically commits the exact confirmation payload, the resulting session
transition, and its audit record. No partial confirmation may survive restart.
Confirmation is still not the real external effect: the exact action-time gate
must independently authorize each Reminder write, delivery, permission, or
other effect, and any bound-field drift creates a new confirmation revision.

A Reminder proposal may include visible/editable date, time, and timezone only
when each value is grounded in explicit user-provided temporal information.
Host validates that the resulting instant is in the future in the selected
timezone. If the user did not provide an exact time, scheduling is incomplete
and Host must ask the user to choose; it may not fill a fixed default or infer
from the question timestamp. The exact future date/time/timezone/list/count
bind the confirmation digest, and every edit creates a new revision and
reconfirmation. Confirmation still cannot authorize the real Reminder write.

The user may confirm the recommended payload, adjust it, or choose not now.
Any change to the exact Reminder payload/time/list/count, recipient, external
write, data disclosure, permission, model, effect, or document digest creates
a new revision and requires confirmation.

Inside an approved direction, OpenOpen may reason, compile context, prepare
drafts, update internal session state, and prepare Markdown diffs. On an
already connected interactive binding, it may reactively reply to the owner on
the most recently accepted owner-active channel without per-message
confirmation. It may not send proactively, add a recipient, deliver across
channels, create/change Reminders, expand data, install, change permissions, or
cross another external effect without the applicable confirmation.

## 9. Markdown continuity

The user-visible root is `~/Documents/OpenOpen`:

```text
INDEX.md
profile/USER.md
profile/COMMUNICATION.md
sources/INDEX.md
sources/<source>.md
tasks/<task>/OVERVIEW.md
tasks/<task>/STATE.md
tasks/<task>/DECISIONS.md
tasks/<task>/QUESTIONS.md
tasks/<task>/MODEL_BRIEF.md
tasks/<task>/paths/<path>.md
tasks/<task>/updates/<event>.md
sessions/<session>/SESSION.md
sessions/<session>/choice-sets/<choice-set>.md
```

Files are mode `0600`, directories `0700`, owned by the current user, and
written through a Store-owned render intent followed by descriptor-safe staging.
Host first syncs the staged file. Creation then uses a no-clobber rename;
replacement uses an atomic same-directory swap/CAS-equivalent rename that
retains the displaced inode. Host proves that displaced inode and digest equal
the intent's base manifest, so a concurrent Owner edit can never be silently
overwritten. It validates both displaced base and final desired inode/
digest, syncs the parent directory, revalidates the final digest, and only then
commits an exact-digest render receipt. A mismatch swaps back only when both
current paths still match the captured inodes/digests; otherwise both versions
are preserved in typed reconciliation with no receipt or dependent work. On
restart, an intent without a receipt is adopted only when
descriptor-pinned inspection proves the final bytes exactly equal the intended
manifest and the retained displaced file proves the exact base; otherwise it
enters typed reconciliation and dependent work stays blocked. Multi-file
intents retain an entry journal and grant no aggregate receipt until every
entry passes; partial swaps are recovered or reconciled, never presented as a
completed manifest. A receipt never grants effect authority. Host rejects absolute/parent
paths, unknown manifest paths,
symlinks, hardlinks, non-regular files, owner/mode mismatch, path replacement,
oversize files, duplicate normalized paths, Unicode/case collisions, and root
escape. Store retains document manifest digests and audit anchors, not effect
authority in Markdown.

Markdown text is untrusted state data. Embedded instructions, tool requests,
recipients, secrets, approvals, or authority claims cannot override the fixed
contracts. A model may propose typed content; only Host renders accepted typed
state into exact manifest paths. `MODEL_BRIEF.md` is a reproducible, bounded
view of accepted typed state, not a system prompt and never an authority or
effect grant. Path notes and updates remain separate so a later compatible
model can reconstruct what changed without rereading an unbounded chat.

## 10. Channels

### 10.1 Mac

Mac is the primary setup, status, review, confirmation, recovery, and rich-card
surface. On cold start it shows an English-only welcome, scans the account,
requires an explicit compatible model and supported effort choice, asks one
simple high-value question, and only then generates the first dynamic A/B/C
plus D. It does not require channel setup first. The capability scan and picker
are product-owned setup, not a static or model-generated ChoiceSet.

### 10.2 iMessage

The dedicated same-account self-chat is an interactive, bidirectional private
inbox. User-authored self-chat messages are accepted as inbound even though
Messages represents them as sent by the current account. OpenOpen outbound is
marked with a durable product-owned identity/echo marker and is never re-ingested
as user authority. Ambiguous identity, duplicate echo, loop, missing marker
proof, or cursor drift keeps the binding Off.

V1 may add exactly one additional local one-to-one read-only source after PR2
and B+ closure. It is individually selected, revision-bound, and
revocable. It never receives OpenOpen output and never becomes recipient or
effect authority. A second additional source, any group conversation, ambiguous
identity, or stale/revoked binding is rejected before body persistence or model
access.

V1 uses the public/basic `imsg` boundary only. It does not use private IMCore,
SIP changes, Accessibility automation, or a TCP service.

### 10.3 Discord

Discord is a post-B+ integration. Its owned-path preparation may run in
parallel, but shared wiring and merge occur only after B+ closure and
cannot delay or satisfy PR1/PR2.

The user creates a personal Discord application/Bot through a guided local
wizard. The Bot token is entered once into a secure field, validated against
the expected Bot identity, and stored at rest only in Keychain. It exists
transiently only in the authenticated local Gateway client's memory, is never
logged or persisted elsewhere, is redacted from diagnostics, and is released
or zeroized on stop where supported. OpenOpen generates the official
installation link and runs an explicit doctor.

The Mac hosts the Bot Gateway. There is no OpenOpen-operated relay, cloud,
shared Bot, remote queue, or normal-user token. Setup binds the authenticated
owner Discord user ID, expected Bot/application ID, and exact personal DM
channel. Before body persistence or model access, Host validates author, Bot,
application, and conversation metadata against that binding. Unrelated events
are discarded after metadata-only classification; their bodies are not
persisted or model-visible.

When the Mac is offline, the Bot is offline. On reconnect, the pre-consent
recap is deterministic metadata only: bounded count, time range, and redacted
provider correlation IDs. No message body and no model are used before an
English continue/discard choice. Only after explicit continue may bounded
owner-bound bodies enter the normal batching/model path. Discard advances the
cursor without body persistence or model/effect work.

## 11. Model and plan behavior

OpenOpen supports ChatGPT sign-in and displays only non-hidden models returned
by the complete account catalog that satisfy the pinned stable protocol and
input requirements. On first launch, the user explicitly selects a model and
one of its actually supported effort levels before model work. Models with no
user-controllable effort use typed `not_applicable`. The selection persists and
is shown in provenance/review surfaces.

OpenAI currently documents a Free plan for exploring Codex on quick coding
tasks and supports ChatGPT sign-in for local Codex work. Availability, models,
usage, credits, and limits remain account/plan dependent. OpenOpen may say
“Start with the plan and models available to your account.” It must not promise
unlimited free work, a particular model, or continuous availability.

The model picker presents the complete compatible scan as explicit English-only
choices with stable semantic fields: model display name/ID, account
availability, supported effort controls, current limit/status information when
the protocol provides it, and compatibility reason. Supported effort values
are presented with plain-English labels such as `Faster`, `More thoughtful`,
and `Deepest`, while retaining the exact protocol value in provenance. The
product does not preselect, rank as an undisclosed default, or silently replace
the owner's model or effort selection.

Official references:

- https://learn.chatgpt.com/docs/pricing
- https://learn.chatgpt.com/docs/auth

Model disappearance, protocol incompatibility, catalog drift, exhausted
capacity, or missing selection produces a typed `Need you`. It never changes
model or effort automatically.

## 12. Recovery and liveness

- ChoiceSession, batch, ChoiceSet, selected model, document manifest,
  delivery binding, confirmation, Mission, Evidence, Receipt, incident, and
  their audit anchors survive restart at committed revisions.
- Deadline checks are private Host/Store calls, never public RPCs. A scheduler
  supplies only a wake hint; it cannot choose target state or trusted time.
  Host samples continuous monotonic time on the same boot and binds it to the
  persisted deadline revision after opening the Store transition. Across boot,
  Host uses the persisted boot/monotonic/wall anchor; a backward or ambiguous
  wall-clock change yields typed `clockUncertain` and no transition. Forward
  wall time may satisfy only the already persisted deadline. Store derives the
  target from current state/deadlines and rechecks revision/generation inside
  the transaction. Sleep, reboot, forward/backward clock, exact replay, and
  injected private-clock tests are deterministic.
- Global Off immediately blocks new batching/model/effect entry, advances the
  generation, cancels in-flight work, and retires late results. It does not
  delete history.
- Off, cancel, permission denial, model failure, provider loss, document
  conflict, and channel fault each leave an explicit reachable recovery action.
- Repeated polling never reopens an acknowledged incident, steals focus,
  repeats an alert, changes On/Done state, or retries a provider/effect.
- Failure in one source/channel is isolated and cannot erase another valid
  ChoiceSet, Mission, or interactive control.

## 13. Acceptance

### PR1 — Core, Mac, Markdown, Reminders

- exactly three dynamic options plus D;
- Host-owned `choice.begin` derives the authenticated Mac envelope/batch and
  atomically creates initial session/audit state before model work; exact
  replay is idempotent, changed replay/model/catalog/protocol drift/Off fails
  closed, and only a private current-result commit can create the first set;
- Host-owned `choice.select` atomic Selection/session/audit persistence, stale
  ChoiceSet and revision rejection, restart retention, command-owned D text
  intake with Host-derived/sealed batch identity, exact replay/change rejection,
  and no raw snapshot write route;
- private Selection-bound refinement-result commit with exact retry, restart,
  single-transaction operation/result/frame/set/session/audit persistence,
  encrypted raw-turn lifecycle, and late/Off/cancel/model/catalog/protocol/
  manifest drift rejection;
- English-only product UI; first-launch scan → explicit model/effort → one
  question → first dynamic ChoiceSet, with zero model work before selection;
- multi-turn refinement, 2.5-second quiet/8-second hard-cap batching;
- required persisted batch delivery binding, first-envelope Host derivation,
  later-envelope mismatch rejection, restart retention, and typed blocked
  migration for historical missing values;
- one persisted global session and deterministic Host-owned 30-minute soft-idle/
  24-hour stale-review transitions with private clock derivation, sleep/reboot/
  clock-uncertainty, restart/idempotency/late-timer fencing;
- persisted explicit model/effort selection, typed `not_applicable`, and
  complete provenance;
- historical missing-model migration blocks instead of defaulting;
- semantic Markdown render-intent/atomic-rename/receipt recovery, manifest/diff/
  reconfirmation, atomic no-clobber/swap base-CAS, concurrent Owner edit,
  partial-manifest/crash ambiguity, and filesystem attacks fail closed;
- dedicated `choice.confirm` immutable payload/session/audit transaction;
  legacy `mission.confirm` rejection for Choice authority; then separately
  gated real Reminders → Evidence → Receipt → Markdown update → next ChoiceSet;
- explicit-time-only Reminder proposal, missing-time choice requirement,
  future/timezone validation, no fixed/question-time default, exact schedule
  digest binding, edit/revision/reconfirmation, and separate real-write gate;
- restart, Off, stale/late result, duplicate/race, and incident liveness tests.

### PR2 — iMessage

- same-account self-chat user/outbound classification;
- durable echo marker, cursor, dedupe, restart, and loop fail-closed behavior;
- no wake-word address gate in the dedicated inbox;
- groups rejected before body persistence/model access; the additional
  read-only source is unavailable in PR2 itself.
- Messages permission deny/cancel/revoke/regrant/restart is deterministic,
  never repeats a modal, never shows false On, and keeps Off reachable.

### Post-B+ iMessage read-only source

- exactly one additional individually selected and revocable one-to-one
  binding; a second source and all groups fail closed;
- inbound-only authority, no outbound route, no recipient derivation, and no
  reply mirroring to the source;
- revoke/restart/stale-revision/cursor/dedupe tests reject body persistence and
  model work until fresh validation.

### B2 — minimal B+ Memory proof chapter

- singleton and contiguous split conversation members pass the fail-closed
  scanner; traversal, collision, corruption, limits, cancellation, and partial
  catalog cases fail closed;
- automatic real-export diagnostics are local, read-only, no-network, and
  no-retention, with only redacted PASS/FAIL and bounded failure class output;
- exactly one real import produces at most three dynamic Memory candidate cards,
  uses no fixed category taxonomy, and persists no unselected raw or derived
  content;
- only exact Owner consent may allow bounded source excerpts to the selected
  OpenAI model; exactly one Owner-selected card produces a revision-bound
  semantic Markdown diff and only that exact diff may be confirmed/persisted.

### C2 — minimal B+ instruction-only Skill proof chapter

- exactly one public instruction-only Skill with canonical GitHub identity and
  immutable commit/digest acquisition;
- license/path/symlink/size/executable/permission and malicious-fixture audit;
- no skip in Candidate → Staged → Promoted → Runnable, exact promotion/update/
  rollback binding, no script execution, silent update, or self-promotion;
- autonomous verification uses synthetic fixtures only; real selection,
  acquisition, stage/promotion/enablement, and first use are separate Owner
  gates, and the B+ use has no external effect.

### PR3 — Discord — post-B+

- local personal Bot wizard, owner/Bot/application/exact-DM identity validation,
  Keychain-at-rest token boundary, rotation, removal, and identity/intent drift;
- unrelated events rejected before body persistence/model access;
- Gateway disconnect/reconnect and metadata-only pre-consent backlog ask-only;
- token removal, permission/intent drift, restart, and Off keep provider/model/
  effect work at zero until revalidation;
- no OpenOpen cloud/relay/shared Bot and no automatic old-message execution.

Every meaningful phase requires focused tests, `git diff --check`, relevant
type/lint/secret checks, a pre-freeze and post-freeze Product Scout, two fresh
read-only reviewers on the same fingerprint, CI bound to the exact head, and
honest proof wording. Mocks, CI, signatures, or component probes do not replace
real provider/effect proof.

## 14. Explicit action-time boundaries

Installation, administrator approval, password/passkey/Touch ID/2FA, macOS
permission changes, Discord token handling, first channel connection/send,
proactive delivery, a new recipient, cross-channel delivery, Mission
confirmation, Reminder write/manual completion/readback, any newly different
real ZIP disclosure, consent to send bounded history excerpts to a selected
model, selecting/committing Memory cards, Skill selection/promotion/update/
enable/rollback/first use, and public release remain exact action-time
boundaries. The one supplied export is limited to the exact local/no-network/
no-retention B2 diagnostic recorded above. A reactive reply on
the already connected owner-active channel is covered by that binding and needs
no per-message confirmation. Historical approval or a design decision never
substitutes for another effect.
