# OpenAI Build Week Disclosure

## Pre-existing before July 13, 2026

- Friday's general Mission, WorkItem, workflow, trust, storage, and Skill
  lifecycle substrate.
- Friday source-of-truth commit:
  `4870f31fa088bef7eb9f4f256ec62993b02eda80`.
- The OpenOpen product direction and planning discussion.

## Built during Build Week

- The standalone OpenOpen repository, master product specification, acceptance
  ledger, and provenance disclosure.
- A committed Rust/Swift foundation and adversarial test suite covering
  protocol types, lifecycle, approval, Evidence, encrypted state, channel
  dedupe, audit mechanisms, and the protected effect-broker contract. Public
  bootstrap `19ecdd9…` passes 95 Rust and 25 Swift tests locally and the
  foundation has two isolated reviewer PASS reports. GitHub Actions follow-up
  run `29370433505`, associated with PR head `923c88a…`, passes the 95 Rust and
  25 Swift tests plus the strict build, lint, format, plist, and clean-diff
  checks on synthesized merge `d502b3d…`; both commits have tree `ecc50fa…`.
  This is PR
  integration-tree evidence, not exact-head proof. Signed/admin installation,
  cross-UID proof, product E2E, and release acceptance remain unclaimed.
- Product-shell commit `e2313fe…` implements broker-acknowledged,
  monotonic, signed persistent default-Off runtime control, a pinned and
  outer-sandboxed Codex app-server client, one fail-closed user-scoped Core
  with one persistent pinned Codex process authorized by a broker-signed
  root-protected durable audit-token lease,
  cancellable Rust host operations, a SwiftUI window/menu bar/Settings shell,
  Keychain bootstrap, Login Item registration, and an explicit ad-hoc app
  staging path. Thirteen formal isolated reviewer cycles plus one additional
  isolated pre-freeze security audit found issues in earlier versions; the
  thirteenth repaired tree passes 134 ordinary Rust tests and 67 Swift tests.
  Two fresh isolated reviewers PASS frozen Repair20 fingerprint `29a00413…`
  with no P0/P1/P2 findings. User-requested Off immediately blocks
  new model entry and advances the operation generation, but a known-On runtime
  reports Off only after protected broker proof; fallible pre-apply,
  response-loss, and dashboard-mismatch paths preserve On, show a transition,
  or show Unknown rather than inventing Off. A fresh Core with no protected
  history may still report its explicit default-Off state. Global Off neither
  spawns nor reacquires Codex; it
  first cancels active Core work before any fallible broker-trust dependency.
  A revision-bound Core pending-Off latch prevents a canceled login or model
  operation from being resurrected by replaying the prior protected On state;
  only a sufficiently new protected On commit or recovery can reopen work,
  and broker acceptance terminates the exact leased Codex and Core audit-token
  incarnations before protected Off persistence. App/Core cleanup and
  root-worker timeout cleanup hold no numeric PID/PGID signal authority.
  Before any Keychain master is loaded or written to a child, the App validates
  the bundled Core's exact signing identifier and App Team both statically and
  again against the running Core's Mach audit-token incarnation.
  The one persistent pinned Codex runtime starts uninitialized, receives its
  full exact broker lease before initialization or model/account work, is
  exact-signature/hash validated, cannot fork under the outer sandbox, and
  passes two explicit sandboxed initialize/account-read diagnostics from the
  fresh Repair20 stage. Draft PR #2 carries commit `e2313fe…`; Actions run
  `29386477267` passes the complete strict suite on synthesized merge
  `487dae1…`, whose tree `2cae9eb…` equals the PR head tree. This is
  integration-tree plumbing evidence, not exact-head or release proof. Real
  ChatGPT login/model output, signed packaging, cross-UID installation,
  notarization, clean install, external users, and product E2E remain pending.

## Planned Build Week additions (not yet claimed)

- The optional macOS voice entry route; Hero A currently uses explicit text
  input.
- The bounded iMessage and Discord routes.
- Receipt-image extraction, local XLSX generation, and readback.
- OpenOpen-specific workflow learning, GitHub Skill import, packaging, real
  runtime evidence, and user validation.

## Implemented locally, proof pending

- Hero A now connects explicit text input to the protected pinned GPT-5.6
  structured Outcome route, command-owned Mission confirmation, real EventKit
  Reminders creation/readback, signed Reminder Evidence, and a Receipt UI.
  The first candidate's full local suite and explicitly ad-hoc stage passed,
  but both fresh closure reviewers rejected its recovery, Mission isolation,
  exact Reminder approval, and approval-time boundaries. Repair1 passed its
  governance review but failed functional review because one logical approval
  could drift to a different physical EventKit list after restart. Repair2
  bound the EventKit source/calendar target, but both replacement reviewers
  rejected it: a first-write approval could later attach to a newly appearing
  same-name list, cancellation could cross a pending calendar write, and a
  restart with every marker missing could recreate the complete mirror.
  Repair3 required one existing uniquely selected physical OpenOpen list,
  removed calendar creation, and recorded exact Reminder links as signed
  `ReminderMirrored` Evidence. Both replacement reviewers nevertheless rejected
  frozen fingerprint `fa9d905e…`: after EventKit could have committed, the App
  still retained volatile `createOnce` authority, so readback failure, Off, or
  marker loss could later issue a second batch. The three-attempt supervisor
  classified the shared root as `STUCK: same_root_cause`; the owner-approved
  strict at-most-once repair was then implemented locally. Before EventKit,
  `mission.reminders.begin` atomically persists signed per-item dispatch
  Evidence and irreversibly converts all later attempts to read-only recovery.
  EventKit marker v2 and persisted links bind the exact dispatch token. Missing,
  moved, changed, or incomplete markers after dispatch fail closed and never
  authorize another write through the App route. Its functional reviewer
  passed, but governance rejected frozen fingerprint `4cabaeb4…` because the
  lower-level EventKit writer still exposed a reusable copyable-Mission API.
  Repair5 makes that writer internal, requires the complete one-shot
  `ReminderDispatchStart`, and consumes each Mission's execution claim before
  permissions, marker discovery, or EventKit. Replaying the retained first
  start now fails before any external boundary. The full local verification
  passes 146 ordinary
  Rust tests with one environment-gated runtime test skipped in the ordinary
  run, plus 40 broker/signing and 43 App Swift tests, Release builds, strict
  lint/format, metadata checks, and two explicit pinned-runtime sandbox
  diagnostics. Fresh
  `/private/tmp/OpenOpen-Stage-HeroA-Repair5.app` reports
  `STAGED_AD_HOC_NOT_RELEASE_PROOF`. Two entirely fresh functional and
  governance reviewers PASS frozen fingerprint
  `4b41a04f7b28573e1a04cb19c79f499b497a2240efbcc236f003f4feb97971cf`
  with zero P0/P1/P2 findings after independent complete verification. Reviewed
  commit/push/current CI, real ChatGPT output, and real user Reminders proof
  remain pending. This is not product E2E or release proof.

Items move from Planned to Built only after implementation and the stated proof
tier. Built does not imply signed-build or real-provider proof.

## Not claimed

- Friday as a whole was not built during Build Week.
- A green CI run is not proof of a working user experience.
- OpenOpen has no cloud service, mobile app, Telegram adapter, marketplace,
  ambient monitoring, private iMessage bridge, payment execution, or silent
  self-upgrade.
- Demo video production is not part of this implementation phase.

## Required submission evidence

The final submission must identify the primary Codex `/feedback` session and
explain separately how Codex accelerated implementation and how GPT-5.6 is
used by the running product.
