# OpenOpen

OpenOpen is a private, local-first personal agent for non-developers. A person
can describe what is happening in ordinary language, choose from three useful
next directions or keep talking, refine the plan, confirm one exact payload,
and receive Evidence-backed completion without learning prompting or project-
management vocabulary.

The Build Week product targets macOS 14+ on Apple Silicon and uses the user's
own ChatGPT plan through an embedded Codex App Server. OpenOpen has no cloud
service and no central telemetry.

The current UI is macOS-only. Mac is the setup/review home; the B+ Hero channel is
the dedicated same-account iMessage self-chat. A locally hosted personal
Discord Bot DM and one additional one-to-one iMessage read-only source are
post-B+ integrations.
Channel setup is not the first-run value proposition. OpenOpen is OpenAI-only
in the current slice; Claude/Anthropic is excluded.

## Current status

Repair24 source `ca26036…` is merged as `c86e590…`; CI run `29707715009`
passed both Rust and Swift jobs. That proves source/CI identity only, not a
package, installed runtime, provider send, Mission, Reminder, Receipt, or
release.

The Owner-approved current route is documented in
[the Master Plan](docs/OPENOPEN_BUILD_WEEK_MASTER_PLAN.md),
[Choice Loop design](docs/OPENOPEN_PRIVATE_AGENT_CHOICE_LOOP_DESIGN.md), and
[10-hour B+ execution control](docs/OPENOPEN_30H_EXECUTION_CONTROL.md).
The protected Hero path is PR1 Choice Core+Mac, PR2 iMessage same-account self-
chat, and a same-main Core+iMessage checkpoint App/DMG that proves one complete
real outcome loop. Minimal B2 and then minimal C2 follow as narrow proof
chapters in the final B+ App/DMG. The extra read-only iMessage source, Discord,
broader Memory/Skills, and product-wide presentation are post-B+. Ten hours is
the latest-safe delivery target and execution deadline, never a reason to skip
a gate; any external blocker that threatens it is surfaced immediately.

The product scans the account's compatible GPT/Codex models and requires an
explicit model and supported-effort selection. A model without an effort
control uses `not_applicable`. It has no fixed Sol, Auto route, hidden default,
or silent fallback. Account plans, models, and limits are shown from the live
catalog; OpenOpen does not promise unlimited free use or a particular model.

First launch is English-only welcome/account scan → explicit model and effort
choice → one simple question → the first dynamic A/B/C plus D. No model work
occurs before selection, and product UI never displays a second language.
Host-owned `choice.begin` is the sole public first-local-question intake route;
it atomically creates the initial interpreting session/audit state before model
work and accepts the first ChoiceSet only through a private revision- and
provenance-bound result commit.
Reactive replies use only the most recently accepted owner-active connected
channel; Mac mirrors local state without copying chat output. Proactive, new-
recipient, and cross-channel delivery requires exact confirmation.

## Repair history snapshot — non-normative

Everything in this section records the earlier Repair path. Present-tense
phrases here are historical evidence and cannot override the current Choice
Loop contract or current GitHub state.

Implementation is in progress. A green unit test or mock is not release proof.
The current competition product must not be described as
`BUILD_WEEK_COMPETITION_READY` until the dated current competition contract in
[the master plan](docs/OPENOPEN_BUILD_WEEK_MASTER_PLAN.md) passes. Older full
production gates remain honest post-competition roadmap items.

Historical reviewed commit `5a461ef…` and PR Actions run `29450863581` closed
an earlier code/signing plumbing review only; they do not review the current
implementation. Installed Repair19 final5b has exact signed runtime proof and
exactly two durable pairings (iMessage and the canonical Friday Discord route).
Its one-time first-start recovery terminalized the sole consumed Discord model
dispatch as failed without replay, resend, suggestion, Mission, Receipt, or
outbound work. The original provider message must not be resent.

Repair19 removes only the unsupported structured-output
`uniqueItems` keyword, terminalizes the consumed failed dispatch without
replay, keeps explicit-correction polling live, and boundedly recovers the
exact Core. Two fresh final2 reviews rejected that candidate before install:
Global Off could still race a late durable suggestion, an RPC already installed
before stdout EOF could retain stale write authority, and current-status text
was stale. Final3 makes the Store reject model results across Off in the same
immediate transaction, holds the active-operation gate through reconciliation,
and serializes EOF revocation with the final pre-write authorization check. The
fresh final3 security review passed, but product review invalidated that package
with one P1: successful Core recovery cleared the transient warning while the
durable failed dispatch was no longer rediscovered. Final4 makes the newest
failed dispatch persistently surface `Need you` after restart until a later
explicit correction supersedes it, without granting another model call. The
final3 and final4 were both invalidated before install. Final4 product review
proved persistent `Need you` starved the provider poll that must ingest a
correction; security review proved one channel's failure could clear another
channel's valid Outcome. Final5 polls transport before persistent failure
feedback and invalidates only the exact same-channel suggestion ID. Final5b
passed two fresh reviews, was installed, and produced the bounded recovery
above. Real use then exposed Repair20's exact Global-Off ordering defect: after
the exact old Core was stopped, the replacement Core tried to verify signed
runtime history before receiving the pinned broker enrollment. Repair20
provisions that enrollment first only for the positively quiesced replacement-
Core branch; normal live-Core Off still cancels and prepares before any
provisioning, and an unproven shutdown performs neither prepare nor broker
apply. The first replacement review pair invalidated `final1` before install:
product found the missing same-model failed-shutdown retry regression, and
security found that a newer On intent could falsely converge without restoring
the killed Core and both listeners. Repair20 now latches the quiesced transition
until protected Off commits. If On is still desired, the UI remains Turning On
and model entry stays disabled until the exact Core generation, Codex, account/
models, and both durable listeners are restored; a newer Off re-quiesces and
converges Off without replay. Failed shutdown may be retried on the same model,
while a newer On must revalidate both durable listeners before it can display
On. Both fresh final2 reviewers rejected that package with P0/P1/P2=`0/2/0`:
a refused exact Core terminator was not retried on the same captured audit
token, and recovery could publish On before listener restoration completed.
Final3 quarantines and retries that exact Core generation, and publishes On only
after the fenced Core, Codex, account/models, and both listeners are restored.
Fresh final3 product/security review nevertheless rejected it with `0/1/0` and
`0/2/0`: ordinary persisted Off→On bypassed that special latch, and Discord
`connecting`/`reconnecting` counted as restored. Final4 applies the restoration
gate to every production-lifecycle protected On and requires bounded proof of
exact Discord `connected`; terminal status or timeout pauses fail closed, and a
newer Off interrupts without replay. Both fresh final4 reviewers invalidated
that package before install: product reported P0/P1/P2=`0/1/0` because a real
Core-death recovery briefly displayed Unknown instead of Turning On; security
reported `0/2/0` because protected On could publish without exact ChatGPT plus
`gpt-5.6-sol`/`high`, and Host model work did not require Discord's exact live
`connected` state after recovery. Final5 keeps every Core-death recovery
Turning On, enables model entry only after the managed ChatGPT account and
exact Sol/high catalog are freshly proven, and requires exact Discord Connected
for both model and outbound work. Missing account/model readiness opens only the
managed Account setup boundary; it never blocks protected Off. Final5 passed
its complete matrix and two fresh reviews, was installed, and preserved the two
pairings, failed dispatch, and zero Mission/Receipt/outbound state. Its bounded
recovery then reached `.paused`; the one official Off attempt still failed
because `.paused` was omitted from the existing replacement-Core provisioning
condition. Repair21 adds only that explicit state to the already reviewed
quiesced path, so broker trust is provisioned before the replacement Core
prepares Off. Direct tests cover the successful monotonic commit and shutdown/
provision/prepare failures, with no false Off or external work. The Repair21
complete matrix passed 238 executed Rust tests with two explicit real-runtime
diagnostics ignored and 56 broker/signing plus 127 App tests. Its synchronized
Developer-ID App manifest `509ee4b7…` and DMG `02964b91…` passed two fresh
P0/P1/P2=`0/0/0` reviews, were installed through the official broker route,
and durably committed Global Off from revision 27 to 28 while preserving both
pairings, the failed/no-suggestion dispatch, and zero Mission/Receipt/outbound
work. A later official On advanced to revision 29 but exposed Repair22: a
    cursor-bearing Discord restore waited for Connected before it drove the
    existing typed recovery poll that must clear `recovery_required`. Repair22
    drains that transaction before Connected/model/outbound readiness. Its
    final2 package passed two fresh reviews, was installed, restored both
    durable listeners, and accepted the exact Owner-approved correction once as
    provider message `1528211998263738570`. The dispatch failed before
    structured JSON parsing because the client rejected pinned passive
    high-reasoning progress methods and used obsolete `agentMessage/delta`
    instead of `item/agentMessage/delta`. Repair23 accepts only the closed
    pinned passive progress set with strict identity/count/byte/index checks;
    tools, actions, reroutes, malformed or unknown notifications remain fail
    closed. Both real provider dispatches remain terminal and never retry.
    Repair23 final2 was invalidated before installation when its fresh product
    reviewer found that allowed passive metadata did not fully reject pinned-
    schema-malformed active flags and rate-limit fields. Both fresh final3
    reviewers then invalidated that package before installation with
    P0/P1/P2=`0/0/2`: `turn/started` did not require its pinned `items`, allowed
    items did not require bounded IDs and type-specific mandatory fields, and
    the embedded current-state text still called the already-built package
    pending. Final4 added the missing Turn/item/lifecycle validation, but its
    fresh product/security reviews invalidated it before install with combined
    P0/P1/P2=`0/1/1`: malformed `memoryCitation` objects passed incomplete
    nested validation, and protocol/item/output errors returned failure without
    terminating the reusable Codex transport. Final5 closed those findings,
    but both fresh final5 reviewers invalidated it before install with
    P0/P1/P2=`0/1/0`: `turn/completed` did not require the pinned full
    `itemsView`, so a valid display summary could hide tool/action items while
    supplying an otherwise valid final JSON. Final6 is also invalid: its
    `turn/started` route did not validate every optional pinned Turn field, and
    its recorded Swift log ended with a missing `swift-format` command while
    the receipt incorrectly called that log PASS. Supervisor-approved final7
    replaces the fragmented checks with one sealed stage-aware gateway used by
    `turn/start`, `turn/started`, and `turn/completed`, validates the complete
    known Turn and accepted nested item shapes, rebuilds authority only from a
    terminal full list, and terminates the transport on every turn/protocol/
    output error. The complete current-fingerprint matrix passes 253 executed
    Rust tests with two explicit real-runtime diagnostics ignored, 56 broker/
    signing plus 140 App tests, release builds, strict lint/format, locked
    metadata, notices, plist/scripts, diff/conflict, and secret scans. The
    signed-package and fresh-review facts are intentionally bound only by the
    external immutable final7 receipt; this embedded-capable text does not
    self-certify or dynamically negate them. The exact final7b build was later
    installed for bounded runtime validation. That real state exposed a
    recurring blocking modal for the same durable terminal failure. Repair24
    keeps the failed dispatch immutable, persists one incident and atomic
    acknowledgement, prevents repeat modal presentation across polling/restart,
    and leaves the incident visible as non-blocking activity without any model
    or provider retry. Incident-scoped feedback/refresh, strict channel DTOs,
    model-gated Discord setup, connected-route effects during account setup,
    and an explicit required poll capability that blocks model selection while
    the account or recovery path is not model-ready close the remaining
    liveness findings without granting model authority. A fresh pre-freeze
    Product Scout reports P0/P1/P2=`0/0/0`. Final6's Rust matrix passed but its
    Swift matrix exposed a fixed-delay test-scheduling race and is historical
    FAIL evidence. An isolated supervisor authorized only a bounded Mock Core
    Dashboard-entry synchronization in the two sibling generation tests. The
    resulting fingerprint must rerun all 269 executed Rust tests with two
    explicit real-runtime diagnostics ignored, all 56+193 Swift tests, both
    release builds, warnings-as-errors, strict formatting/lint, notices,
    metadata, plist/scripts, diff/conflict, and bounded secret scanning.
    Repair24 package and review facts are bound only by a later external
    immutable receipt; installation, new input, and connected Mission proof
    remain pending.
`final1` through `final6` remain invalid historical artifacts. No real
connected Mission proof is claimed.
This file neither self-authorizes installation nor claims real-provider
closure.
PR #2 remains draft and unmerged; every Developer-ID candidate remains
unnotarized.
Notarization and external-user proof are not current competition gates and are
not claimed.

## Development

Requirements:

- macOS 14+
- Xcode 26+
- Rust 1.96+

Run the Rust checks:

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
```

Run the current protected effect-broker Swift package checks:

```bash
cd macos/EffectBrokerBridge
swift test -Xswiftc -warnings-as-errors
swift build -c release -Xswiftc -warnings-as-errors
xcrun swift-format lint --recursive --strict Sources Tests
```

The repository CI workflow is configured to run these checks using the
explicit `macos-26` runner label. Availability and inspected-run status are
recorded in the acceptance ledger; CI is plumbing-tier evidence only and is
not a substitute for signed/admin-installed cross-UID or real product proof.

First build the exact pinned imsg runtime from a clean checkout at commit
`fa2f82d7dbda4c802d91c1d41bb6c53564ed2fdc`. The stage rejects any imsg
binary, receipt, resource, compiled-source manifest, or runtime-tree hash that
differs from the reviewed Friday-alpha build:

```bash
IMSG_SOURCE=/absolute/path/to/clean-imsg-checkout
IMSG_RUNTIME=/private/tmp/OpenOpen-imsg-runtime
IMSG_RECEIPT=/private/tmp/OpenOpen-imsg-build-receipt.json
scripts/build_pinned_imsg.sh \
  --source-root "$IMSG_SOURCE" \
  --output "$IMSG_RUNTIME" \
  --receipt "$IMSG_RECEIPT"
```

Stage a local ad-hoc diagnostic from that exact imsg runtime and an explicit
pinned Codex package root:

```bash
CODEX_ROOT=/absolute/path/to/codex-0.144.0-package
scripts/stage_openopen_app.sh \
  --codex-package-root "$CODEX_ROOT" \
  --imsg-binary "$IMSG_RUNTIME/bin/imsg" \
  --imsg-receipt "$IMSG_RECEIPT" \
  --output /absolute/new/path/OpenOpen-AdHoc.app
```

The script verifies every pinned component hash before and after copying,
includes the protected broker daemon/worker/LaunchDaemon plist, and exclusively
claims a new output directory instead of merging into an existing app.
`STAGED_AD_HOC_NOT_RELEASE_PROOF` is a local diagnostic only;
it is not Developer ID signing, notarization, clean-install, or release proof.

To produce the separate Developer-ID-signed but still unnotarized alpha
candidate, pass one exact certificate name explicitly to both stages. Omitting
the option never produces a Developer-ID claim, and an unavailable or non-Apple
identity fails closed:

```bash
DEVELOPER_ID_IDENTITY='Developer ID Application: Wenxin Dou (UHDY2275L5)'
SIGNED_APP=/absolute/new/path/OpenOpen-DeveloperID.app
SIGNED_DMG=/absolute/new/path/OpenOpen-DeveloperID.dmg
scripts/stage_openopen_app.sh \
  --codex-package-root "$CODEX_ROOT" \
  --imsg-binary "$IMSG_RUNTIME/bin/imsg" \
  --imsg-receipt "$IMSG_RECEIPT" \
  --output "$SIGNED_APP" \
  --developer-id-identity "$DEVELOPER_ID_IDENTITY"
scripts/create_alpha_dmg.sh \
  --app "$SIGNED_APP" \
  --output "$SIGNED_DMG" \
  --developer-id-identity "$DEVELOPER_ID_IDENTITY"
```

This frozen alpha path accepts only that exact owner certificate and verifies
its SHA-256 leaf fingerprint after every owner signature and on the DMG. The
scripts also compare each owned Mach-O before and after signing, then recheck
the final App. A process already authorized to use the owner's private signing
identity can invoke Apple's `codesign` directly; these scripts prove the exact
candidate output and do not claim to sandbox that already-authorized signer.

The Developer-ID output remains `NOT_NOTARIZED_NOT_RELEASE_PROOF`.
Administrator/cross-UID runtime identity remains required for the tested
competition build. Apple notarization, submission, stapling, Gatekeeper public
release, and clean-machine distribution are post-competition production gates
and are not claimed.

No credentials belong in this repository. ChatGPT and Discord credentials are
stored at runtime in the macOS Keychain.

## Scope

The current route is:

`Natural expression → understanding → dynamic A/B/C + D → refinement → exact confirmation → Reminders → Evidence → Receipt → Markdown update → next choices`

PR1 builds Choice Core, Mac, explicit model selection, bounded Markdown, and
Reminders/Evidence/Receipt. Reminder schedules are proposed only from explicit
user time information; missing time requires user selection, and every exact
future schedule edit reconfirms without authorizing the separate real write.
PR2 adds only the same-account iMessage self-chat private inbox and earns a
Hero checkpoint. B+ then completes one-import/one-card B2 and one public
instruction-only/no-effect-use C2 before the final package. A revocable extra
iMessage read-only source, Discord, and broader expansion are post-B+. B2 may use the
supplied export automatically only for the separately bounded, non-gating,
local/read-only/no-network/no-retention diagnostic; semantic model processing
requires later exact Owner consent.

Groups, Slack, Auto routing, fixed Sol, shared/cloud Discord Bot, ambient
surveillance, unrelated Discord DMs, offline auto-replay, Hero B/C, arbitrary
Skill scripts, mobile UI, and silent model/Skill upgrades are excluded.

## Provenance

OpenOpen builds on selected MIT-licensed state-machine and safety ideas from
Friday. See [BUILD_WEEK.md](BUILD_WEEK.md),
[PROVENANCE.md](PROVENANCE.md), and
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
