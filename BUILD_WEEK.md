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
- The current uncommitted product-shell slice implements broker-acknowledged,
  monotonic, signed persistent default-Off runtime control, a pinned and
  outer-sandboxed Codex app-server client, one fail-closed user-scoped Core
  with one persistent pinned Codex process authorized by a broker-signed
  root-protected durable audit-token lease,
  cancellable Rust host operations, a SwiftUI window/menu bar/Settings shell,
  Keychain bootstrap, Login Item registration, and an explicit ad-hoc app
  staging path. Thirteen formal isolated reviewer cycles plus one additional
  isolated pre-freeze security audit found issues in earlier versions; the
  thirteenth repaired local working tree now passes 134 ordinary Rust tests and
  67 Swift tests. User-requested Off immediately blocks
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
  fresh Repair20 stage. Two new
  isolated reviewers, a pushed commit, GitHub CI, real ChatGPT login/model output,
  signed packaging, and product E2E remain pending, so none of those higher
  proof tiers is claimed.

## Planned Build Week additions (not yet claimed)

- The macOS voice/Reminders route.
- The bounded iMessage and Discord routes.
- Receipt-image extraction, local XLSX generation, and readback.
- OpenOpen-specific workflow learning, GitHub Skill import, packaging, real
  runtime evidence, and user validation.

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
