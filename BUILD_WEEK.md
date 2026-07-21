# OpenAI Build Week Disclosure

Current product scope, authority, privacy, milestone, and acceptance semantics
come only from `docs/OPENOPEN_BUILD_WEEK_MASTER_PLAN.md`. This disclosure is a
chronological Build Week/provenance record and cannot authorize implementation
or override that canonical contract.

## Current private-agent direction — 2026-07-19

The Owner replaced the former competition critical path with the private-agent
Choice Loop: natural expression → bounded understanding → dynamic A/B/C plus D
→ refinement → one exact confirmation → Reminders → Evidence → Receipt →
bounded Markdown update → next choices.

The current UI is macOS-only. Mac is primary; the B+ Hero channel is the dedicated
same-account iMessage self-chat. A local personal Discord Bot DM and one
additional one-to-one iMessage read-only source are post-B+. The
account's compatible GPT/Codex
models are scanned and presented
for explicit model and supported-effort selection. First launch and all
product-owned visible copy are English-only: account scan → selection → one
simple question → first dynamic A/B/C plus D, with no model work before
selection. Reactive replies use only the latest owner-active connected channel;
Mac does not duplicate them across channels. Fixed Sol, Auto, silent fallback,
Connect Messages first, old Outcome/15-minute IntentSession UX, groups,
shared/cloud Discord Bot, offline replay, Claude/Anthropic, and Repair23 input
reuse are not current authority.

Repair24 source `ca26036…` merged as `c86e590…`, and CI `29707715009` passed.
That is source/CI identity only. The protected Hero route is PR1 Choice Core+Mac,
PR2 iMessage same-account self-chat, and a same-main Core+iMessage checkpoint
App/DMG proving one complete real outcome loop. Minimal B2 and minimal C2 then
land as narrow proof chapters in the final B+ App/DMG.
Host-owned `choice.begin` is the only public first-local-question
intake/create route and must commit the initial session/audit atomically before
model work. B+ B2 is one real import, at most three candidates, one Owner-
selected card, and one separately confirmed Markdown diff. B+ C2 is one
public instruction-only Skill and one no-external-effect use. Extra iMessage,
Discord, broader expansion, and advanced presentation are post-B+. Real install, providers,
permissions, Mission, Reminder, Evidence, Skill lifecycle, and release
boundaries remain unclaimed. The supplied real ChatGPT export is authorized
only for an isolated local, read-only, no-network, no-retention B2 diagnostic;
neither it nor derived metadata may enter repository evidence or a remote.
Sending bounded excerpts to the selected OpenAI model and committing selected
Memory cards require later exact Owner consent.

Reminder proposals use only explicit user temporal information. Missing time
requires user selection; fixed/question-time defaults are forbidden. Exact
future date/time/timezone/list/count bind confirmation and every edit
reconfirms, while the real write remains a separate action-time gate.

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
  Each pinned Codex runtime starts uninitialized and receives its full exact
  broker lease before initialization or any authorized work. The normal
  account/model runtime is persistent and keeps the canonical login Keychain
  read-only. A distinct short-lived official-login runtime alone may write the
  exact login Keychain database; it exposes no account/model/turn work and is
  destroyed before a fresh read-only runtime and lease are accepted. Both are
  exact-signature/hash validated, cannot fork under the outer sandbox, and
  passes two explicit sandboxed initialize/account-read diagnostics from the
  fresh Repair20 stage. Draft PR #2 carries commit `e2313fe…`; Actions run
  `29386477267` passes the complete strict suite on synthesized merge
  `487dae1…`, whose tree `2cae9eb…` equals the PR head tree. This is
  integration-tree plumbing evidence, not exact-head or release proof. Real
  ChatGPT login/model output, signed packaging, cross-UID installation,
  notarization, clean install, external users, and product E2E remain pending.
- Hero A commit `774789ca4a5eeadb8fa57688e79f823dec4da65b`
  implements the reviewed explicit-input → structured GPT-5.6 Outcome → bounded
  Mission → exact Reminders dispatch/readback → completion Evidence → Receipt
  route. Two fresh reviewers PASS fingerprint `4b41a04f…`; Actions run
  `29393462659` passes the complete strict suite on synthesized merge
  `bccdf360…`, whose tree `e8f3605…` equals the head tree. This remains
  plumbing evidence, not real provider/Reminders or release proof.
- Friday-alpha implementation commit
  `2685b572715dff3e1360de66ab4c2ab6c013730b` adds the reviewed shared channel
  boundary plus exact imsg and Discord entry/readback adapters to draft PR #2.
  Two fresh reviewers PASS fingerprint `3e201547…` with zero P0/P1/P2, and
  Actions run `29440208503` passes the complete strict workflow on synthesized
  merge `99ee2b10…`; its tree `730bce09…` equals the exact head tree. This is
  reviewed implementation and PR integration-tree plumbing evidence only.
  Evidence follow-up `becea456…` is also pushed and Actions run `29442001103`
  passes on equal-tree synthesized merge `2b80e2c…`. Real
  GPT/Reminders/iMessage/Discord traffic, notarization,
  administrator/cross-UID proof, and `FRIDAY_ALPHA_READY` remain pending.
- The local signing slice adds an explicit Developer-ID-only staging
  mode with no identity fallback, hardened runtime plus secure timestamps for
  every OpenOpen executable, and the one Apple Events entitlement required by
  the pinned imsg sender. It verifies the exact upstream Codex `rg` hash before
  re-signing that same Mach-O for notarization and records both hashes plus its
  runtime Team/CDHash. Historical v3 had Team `UHDY2275L5` and DMG SHA
  `0d51c849…`; its structural and 93-Swift checks passed before review. The
  first governance reviewer rejected fingerprint `eaa4bc2e…` because staging
  trusted caller-authored imsg receipt data, the DMG creator did not bind the
  exact same-Team App/nested code/receipts, and README omitted mandatory imsg
  inputs. v3 is superseded. The repair pins exact imsg bytes, receipt, patch,
  source/runtime/resource manifests and runtime allowlist, then requires the
  exact OpenOpen bundle, eight Mach-O identities, Teams, Apple anchors,
  hardened runtime/timestamps, entitlement split, receipts, upstream hashes,
  and frozen CDHashes before DMG creation. Local v4 passed those focused checks
  and produced signed DMG SHA `feec94d3…`, but both fresh v4 reviewers rejected
  fingerprint `08a58745…`: file and directory modes were not bound, the exact
  owner certificate leaf was not pinned, and staging did not compare owned
  unsigned Mach-O content after every signature/final copy. v4 is historical,
  not a signing PASS. The replacement repair pins the exact owner leaf, normalizes
  and verifies the complete directory/file mode contract, and checks every owned
  Mach-O before and after signing plus at final output. Closure candidate v5
  embeds byte-identical provenance, has exact owner leaf SHA `a7e43925…` on all
  owner code and the DMG, and passes the new mode/directory/identity negatives
  plus the full 190-Rust/40+53-Swift strict suite; its signed review DMG SHA is
  `494caddf…`. Fresh functional and governance reviewers both PASS unchanged
  fingerprint `fdf5a00e…` with zero P0/P1/P2. Gatekeeper correctly reports
  `Unnotarized Developer ID`; this is not `FRIDAY_ALPHA_READY` or release
  proof. At that checkpoint the provenance-bound replacement tree still
  required its own two fresh reviewers on an unchanged final fingerprint. The
  rebuilt
  App `/private/tmp/OpenOpen-FridayAlpha-DeveloperID-v5-evidence-final.app`
  embeds provenance SHA `315deb30…`; its exactly verified signed DMG SHA is
  `b7f3e718…`. The first final-evidence governance reviewer rejects that
  fingerprint on one P2: another provenance paragraph still says “not yet
  reviewed,” which becomes false when review completes. The peer review was
  interrupted and is not counted. The narrow replacement removes dynamic
  review status from embedded provenance; all package and proof gates remain
  unchanged. Final2 embeds provenance SHA `155aa65a…`; its exactly verified
  signed DMG SHA is `7c022b83…`. Two entirely fresh replacement reviewers PASS
  unchanged fingerprint `026b2b1f…`; reviewed commit `5a461ef…` is pushed to
  draft PR #2 and Actions run `29450863581` passes on equal-tree synthesized
  merge `da3d7d1…`. This closes signing/evidence and integration plumbing only.
  The package remains unnotarized, and no provider, administrator/cross-UID,
  `FRIDAY_ALPHA_READY`, or release proof is claimed.

## Superseded competition additions — historical only

This section records the former 2026-07-17 plan and does not define the current
Choice Loop critical path.

- The optional macOS voice entry route; Hero A currently uses explicit text
  input.
- Real-provider and restart proof needed to earn `FRIDAY_ALPHA_READY` for the
  implemented bounded iMessage and Discord entry/readback slice, targeted for
  July 16–17, 2026 `America/Los_Angeles`.
- Agent Understanding v1, Quick Memory Passport, complete ChatGPT ZIP
  import with fixed fail-closed resource limits, approved-source learning,
  bounded proactive suggestions, and one confirmed personalized Hero A
  outcome.
- OpenOpen-specific Workflow Candidate and instruction-only public GitHub Skill
  import/audit/promotion/use/update/rollback.
- Judge-facing UI, persona, communication rubric, product screenshots, Devpost
  copy, and a three-minute storyboard after the Owner design gate.

Slack, Auto routing, Hero B/C, arbitrary Skill scripts, notarization, formal
three-user/48-hour validation, production release proof, and video production
are post-competition or excluded work and are not required by
`BUILD_WEEK_COMPETITION_READY`.

The Owner's 2026-07-18 scope decision makes competition V1 OpenAI-only.
Claude, Anthropic integrations, cross-provider import, and Claude-specific
proof are excluded and not claimed; earlier references remain historical only.

## Implemented locally, proof pending

- Keychain-domain Repair3 is historical after its security reviewer reproduced
  a same-device hard-link alias from the real login Keychain database into the
  writable Codex home. Repair4 uses the existing protected root broker to
  create one fixed, bounded case-sensitive tmpfs runtime home on a different
  device with `nodev,nosuid,noexec`; Core independently verifies the exact
  current-EUID path, mount, owner, mode, and device before Codex starts. The
  exact login database remains read-only. No credential is read, copied,
  parsed, cloned, or migrated; the new canonical runtime account requires the
  pinned official Codex login flow. The first signed Repair4 install exposed
  that `Host::open` still tried to create the broker-owned path while the
  background service was Off, so Core exited before it could report the honest
  maintenance state. Repair5 leaves that path absent at Store startup; the
  Codex client validates the exact broker mount before creating its nested
  synthetic home. Repair6 subsequently proves the real 256 MiB/32768-node
  mount, security flags, different-device hard-link rejection, case
  sensitivity, and safe inode headroom. The first official callback then
  exposes the read-only Keychain persistence failure that Repair7 isolates
  behind a completion-only login process. Two fresh reviewers then passed
  replacement2; its exact installed App launched root broker PID `93107`,
  recreated the bounded tmpfs, and reached real OAuth before Seatbelt denied
  the Keychain `.fl<hex>` lock read. Repair8 adds only that login-only lock
  protocol read. Its first two fresh reviewers found no implementation issue
  but rejected stale present-tense summaries, so that candidate is invalidated.
  The synchronized replacement then passed product review but failed security
  review on one P1: after an exact broker lease was installed, Core could still
  reach `Child::kill()` by numeric PID while retiring the login process. Repair9
  permanently removes Core numeric-signal authority once the lease is bound;
  same-Core rotation instead retires only the exact old Codex Mach audit-token
  incarnation in the broker, then releases the old lease. Its final reviewers
  nevertheless found a retry wedge after local lease retirement and an
  inspection-failure ambiguity that could be mistaken for process death.
  Repair10 permits a new uninitialized candidate in that recoverable old-lease
  state while keeping every operation gated on the new signed lease; broker
  liveness is now alive/dead/inspection-failure and unknown always rejects.
  All 203 ordinary Rust, 56 broker/signing, and 62 App tests plus release/strict
  checks pass. Synchronized Developer-ID App manifest is `09478032…` and exact
  DMG SHA is `7e5eb9af…`; Gatekeeper reports `Unnotarized Developer ID`. Two new
  reviewers then split: product passed, while security found one pre-install P1
  where broker-acquire/Core-install response loss preceded Core's lease-bound
  mark. Repair11 adds an irreversible Core handoff before broker acquisition;
  every later cleanup is pipe-close/wait-only. All 203 Rust, 56 broker/signing,
  and 64 App tests plus release/strict checks pass. The App suite now directly
  proves a durable broker-acquire response loss followed by exact-lease
  rotation retry, in addition to Core install-response loss. Exact synchronized
  Developer-ID App
  `/private/tmp/OpenOpen-FridayAlpha-BrokerHandoff-R11-final.app` has 635-entry
  manifest SHA `3c555ded…`; App/Core/broker CDHashes are `aa1f39b9…`,
  `cae5d6bf…`, and `6012c638…`. Exact DMG SHA is `13d98481…`, embedded
  PROVENANCE is byte-identical, and Gatekeeper honestly reports `Unnotarized
  Developer ID`. Two new reviewers, installed OAuth/account/model proof, and
  exact commit/push/CI remain pending.
- The Friday channel slice now has one shared typed `ChannelEnvelope`, exact
  immutable pairing, encrypted durable cursors/dedupe, once-only model and
  outbound starts, Mission-origin binding, and global-Off transport shutdown.
  The Rust imsg adapter owns one stdio child and calls only the approved basic
  RPC surface; the exact upstream commit is narrowed by a tracked patch and
  dependency lock, and each build emits a hash-bound receipt. The Discord
  adapter uses exact serenity commit `1809beb…`, the official Bot Gateway/HTTP,
  a paired owner/channel, explicit mention, bot filtering, disabled outbound
  mentions, bounded recovery, and deterministic nonce readback. Discord tokens
  are device-only Keychain data in Swift. Focused adapter/Store/Host and the
  full current-tree verification passes 175 Rust tests with one explicit
  pinned-runtime test environment-gated in the ordinary suite, plus 40
  broker/signing and 47 App Swift tests. Release builds, strict Clippy,
  warnings-as-errors, Rust/Swift format, plist/script/diff checks, and the two
  pinned imsg boundary tests pass. `/private/tmp/OpenOpen-FridayAlpha-Final.app`
  passes receipt-bound nested imsg staging and deep ad-hoc signing with
  `STAGED_AD_HOC_NOT_RELEASE_PROOF`; its DMG is mounted read-only, copied to an
  isolated install-test directory, and signature-verified with SHA-256
  `0f9b7fd3ca54c27138c52fe42a0cb31a3a4a13260d0d945a954d608cab39bd15`.
  Both fresh closure reviewers subsequently rejected frozen fingerprint
  `136a42ba…` for durable-recovery, readback, Discord setup/probe, imsg runtime/
  compiled-surface, status, origin-authority, and notices gaps. A packaging
  audit also found the staged imsg omitted its locked PhoneNumberKit resource
  bundle. That App/DMG is therefore historical ad-hoc mount/signature evidence
  only, not a runnable alpha candidate. Repair1 now atomically queues accepted
  model work with the cursor, restricts Mission-origin binding to command-owned
  genesis, closes exact Need-you/Receipt readback, separates connection from
  event status, replaces manual Discord IDs with the official token-derived
  install/pair/probe/confirm flow, and validates the prepared running imsg child
  before sending RPC bytes. The imsg build uses a positive source whitelist and
  ships its locked PhoneNumberKit resource tree. Complete notices now cover 190
  OpenOpen and 924 Codex dependency identities with 597 unique texts. The
  Repair1 passed 186 Rust tests (one explicit environment-gated Codex runtime
  test), 40 broker/signing tests, 49 App tests, strict formatting, and ad-hoc
  staging. Both fresh replacement reviewers nevertheless rejected frozen
  fingerprint `10160bb1…`: Host passed already-prefixed iMessage wire text into
  the single-prefix adapter, basic send returned no real provider identity,
  failed iMessage activation and repeated Discord setup could retain a wedged
  prepared session, and one ledger sentence overstated reviewer completion.
  Repair2 strips exactly one authorized prefix at the Host/adapter boundary,
  obtains only a unique real post-send GUID after a pre-send database
  high-water mark, and uses bounded read-only GUID recovery without ever
  resending. Swift now stops a prepared iMessage child after activation/proof
  failure and stops any prior Discord setup before starting another. The
  corrected local candidate passes 187 ordinary Rust tests with one explicit
  environment-gated Codex runtime test, 40 broker/signing plus 51 App tests,
  four pinned upstream imsg tests, release builds, strict lint/format/plist/
  script/notices/diff checks, and a fresh pinned imsg build. One old Host
  cancellation timing assertion failed during a heavily parallel first run;
  it then passed 20/20 isolated repetitions and the complete exact suite rerun.
  `/private/tmp/OpenOpen-FridayAlpha-Repair2-Final.app` contains only the four
  pinned Codex files, signed imsg build/runtime receipts, the complete resource
  tree, and all 597 notice texts; staged basic RPC passes. Its ad-hoc,
  unnotarized DMG passes read-only mount/copy/signature install testing at
  SHA-256 `15c1429b0b05564b890d5766cd9e1df2aa6d291b5248cc252a11bb7ee2dc02ec`.
  It remains `STAGED_AD_HOC_NOT_RELEASE_PROOF`, has Team `not set`, and is not a
  Developer-ID runnable/release alpha. Two entirely fresh replacement
  reviewers and real ChatGPT/Reminders/iMessage/Discord traffic remain
  pending. No provider or release proof is claimed.

- Both Friday-alpha Repair2 replacement reviewers FAIL unchanged fingerprint
  `1a983c72ad9f70e7cd321c9782e4e127e42e006ba190daec5f76947831064494`.
  A history scan could misbind another Mission's same-text GUID as the current
  outbound delivery; prepare response loss could retain the child; and the
  product still required Messages database identity entry rather than an
  explicit conversation picker. The top acceptance summary was also stale.
  Repair2's green local suite and ad-hoc DMG are historical evidence only.
- Friday-alpha Repair3 keeps every history recovery outcome `Uncertain`; only
  the exact synchronous send RPC may bind its GUID. The pinned sender sends
  once and observes the complete two-second window. iMessage discovery uses a
  no-RPC prepare child, exact running Mach validation, a second fresh-proof
  list call, bounded exact-iMessage results, and unconditional child cleanup.
  The App pre-stops old sessions, survives prepare response loss, and exposes
  conversation/participant pickers. The fresh v4 pinned build passes five
  upstream tests with binary SHA `635c9981…`, build receipt `c1769b40…`, and
  resource tree `7a5cb869…`. The complete local tree passes 190 ordinary Rust
  tests plus one explicit environment-gated test, 40 broker/signing and 53 App
  Swift tests, strict release/lint/format/plist/script/diff checks, two pinned
  Codex diagnostics, and the independent 597-text notice closure.
  `/private/tmp/OpenOpen-FridayAlpha-Repair3-Final2.app` passes deep ad-hoc
  verification and staged RPC with signed imsg SHA `04736f58…`, build receipt
  `c1769b40…`, and Team `not set`. Its ad-hoc, unnotarized DMG passes read-only
  mount/copy/signature installation at SHA-256 `bff4d18b…`. Both artifacts are
  explicitly not release proof. Two entirely fresh replacement reviewers
  remain pending; no provider, Developer-ID, notarization, release, or
  `FRIDAY_ALPHA_READY` proof is claimed.
- First Repair3 closure review returns functional PASS and governance FAIL on
  unchanged fingerprint `11d34c59…`. The sole P2 is evidence wording:
  PROVENANCE still called the historical Repair2 187-Rust/91-Swift suite
  “current,” contradicting the Repair3 190-Rust/93-Swift ledger. No product,
  security, provider, or package boundary finding is reported. The sentence is
  corrected to distinguish historical Repair2 from current Repair3, and the
  replacement Final2 App/DMG above embeds that correction. Two entirely fresh
  replacement reviewers remain required before push.
- Two entirely fresh Repair3 evidence-fix replacement reviewers PASS unchanged
  fingerprint `3e2015475d98b74d88a3de4c36e3a1aa4e8bcd1659a3356c5f36f7bd68103ae3`
  with zero P0/P1/P2. They independently confirm the current/historical
  provenance distinction, byte-identical embedded Final2 provenance, focused
  Host/imsg/Swift/pinned-patch checks, ad-hoc package facts, and honest remote/
  external-gate wording. Reviewed commit `2685b572715dff3e1360de66ab4c2ab6c013730b`
  is now pushed to draft PR #2, and Actions run `29440208503` passes on
  equal-tree synthesized merge `99ee2b10…`. This closes Stage 6 and PR
  integration-tree Stage 7 plumbing only; no provider or release proof is
  implied.

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
  commit `774789c…` is pushed and current Actions `29393462659` passes on an
  equal-tree synthesized merge. Real ChatGPT output and real user Reminders
  proof remain pending. This is not product E2E or release proof.

- The first real Connect ChatGPT attempt reached the signed managed-login path
  but failed closed because OpenOpen's outer sandbox denied the pinned Codex
  app-server's localhost OAuth callback listener. The minimum candidate allows
  only localhost TCP ports 1455 and 1457, matching exact pinned Codex `0.144.0`
  source; wildcard inbound remains denied. The repaired real sandbox diagnostic
  starts the official OpenAI authorization URL, and the complete 194 ordinary
  Rust, 44 broker/signing Swift, and 56 App Swift suites plus strict checks and
  Developer-ID staging pass. The candidate is not yet committed or installed,
  and no OAuth completion, provider traffic, milestone, or release proof is
  claimed.

- Repair6 then completed the real protected runtime-home proof: 256 MiB,
  32768 nodes, `tmpfs,local,nodev,noexec,nosuid`, different runtime/Keychain
  devices, kernel `EXDEV` for a real hard-link attempt, case-sensitive distinct
  inodes, and no `auth.json`. Its official OAuth callback reached the provider
  but failed closed with sanitized `persist_failed` because the read-only model
  process could not save the new Keychain item. Repair7 adds the bounded
  login-only process and same-App/Core Codex-only broker lease rotation. The
  first pre-install reviewers invalidated its package on four P2 findings. The
  next replacement closed those findings but both reviewers found one P1: a
  second prepare rejected the already-valid user-owned mount before
  re-attestation. Replacement2 accepts only an exact existing mount and mounts
  only an unmounted root-owned empty directory; wrong owner or capacity fails
  without remount. It passes 202 ordinary Rust tests, the corrected explicit
  real login-only diagnostic, 52 broker/signing and 60 App Swift tests, release
  builds, and strict lint/format. The exact App entry-manifest SHA is
  `1490a4ad…`; App/Core/broker CDHashes are `aa3e9d95…`, `08b035f6…`, and
  `54defef5…`. Exact DMG SHA is `ca3043f2…`; Gatekeeper honestly rejects it as
  `Unnotarized Developer ID`. Two entirely fresh pre-install reviewers passed
  the unchanged replacement2 fingerprint with zero P0/P1/P2, after which the
  exact App was installed and its official Background Activity route launched
  root broker PID `93107`. The fresh runtime is again 256 MiB/32768 nodes with
  the required `tmpfs,local,nodev,noexec,nosuid` flags and 24636 free nodes.
  The first real replacement2 OAuth completion still fails closed with
  `persist_failed`: kernel Seatbelt evidence binds the login-only Codex PID to
  denied metadata access on the exact Keychains directory and denied read
  access to its `.fl<hex>` lock file. Repair8 adds only those login-only read
  rules; the model profile remains unchanged, and the lock plus every other
  Keychain file remain non-writable. The complete 202 ordinary Rust, 52
  broker/signing, and 60 App tests plus Release, strict lint/format,
  metadata/script/plist, conflict, and secret-pattern checks pass. The first
  Repair8 package is historical after the shared documentation P2. Synchronized
  Developer-ID App
  `/private/tmp/OpenOpen-FridayAlpha-KeychainLock-R8-replacement.app` has
  635-entry manifest SHA `9bde9be3…`; App/Core/broker CDHashes are
  `73e76889…`, `3206dadd…`, and `54defef5…`. Exact mounted/copied-verifier DMG
  SHA is `2df5a91a…`; Gatekeeper honestly reports `Unnotarized Developer ID`.
  Its fresh product reviewer passed, but the fresh security reviewer rejected
  one P1 in the post-lease numeric-PID termination path, invalidating that
  package. Repair9 moves post-lease termination exclusively to the broker's
  exact audit-token boundary; Core only closes stdin and asynchronously reaps.
  All 203 ordinary Rust, 54 broker/signing, and 60 App tests plus release and
  strict lint/format checks pass. Synchronized Developer-ID App
  `/private/tmp/OpenOpen-FridayAlpha-LeaseBoundary-R9-final.app` has 635-entry
  manifest SHA `d80220a3…`; App/Core/broker CDHashes are `6d99da86…`,
  `c948c821…`, and `12957664…`. Exact mounted/copied-verifier DMG SHA is
  `9dbe22fc…`; Gatekeeper honestly reports `Unnotarized Developer ID`. Final
  static verification passes; two entirely fresh reviewers, one batched
  re-install, provider completion, and `FRIDAY_ALPHA_READY` were pending. Those
  reviewers then rejected one P1 each: ordinary login retry wedged before
  broker rotation, and token re-inspection failure was ambiguous with death;
  the product reviewer also found one stale Repair7 P2 in the stress row.
  Repair10 removes the stale precondition, keeps fresh candidates uninitialized
  until exact broker lease installation, introduces explicit three-state
  liveness, and adds failure/cancel/invalid-URL/post-login-model retry plus
  same-Core/Off inspection-unknown tests. All 203 Rust, 56 broker/signing, and
  62 App tests plus release/strict checks pass. Synchronized Developer-ID App
  `/private/tmp/OpenOpen-FridayAlpha-LeaseRecovery-R10-final.app` has 635-entry
  manifest SHA `09478032…`; App/Core/broker CDHashes are `5e77b8b7…`,
  `62769b84…`, and `6012c638…`. Exact mounted/copied-verifier DMG SHA is
  `7e5eb9af…`; Gatekeeper honestly reports `Unnotarized Developer ID`. Two new
  reviewers then produced a product PASS and security FAIL on one response-loss
  P1. Repair11 binds the uninitialized candidate before broker acquisition, so
  broker response loss and Core lease-install request/reply loss cannot restore
  numeric signal authority. The existing exact-token broker recovery remains
  authoritative. The first R11 reviewer pair found no implementation or secret
  defect but rejected stale current summaries and the missing direct durable
  broker-acquire-response-loss retry test. That test now persists one lease,
  loses the response, aborts only the already-bound candidate, and succeeds on
  retry only after rotating the durable generation. All 203 Rust, 56
  broker/signing, and 64 App tests plus release/strict checks pass. Rebuilt
  Developer-ID App manifest is
  `3c555ded…`, exact DMG SHA is `13d98481…`, embedded PROVENANCE is
  byte-identical, and Gatekeeper reports `Unnotarized Developer ID`. Two fresh
  reviewers and the single install/provider cycle remain pending. No callback
  query, credential, or token body is recorded.

- Repair11 was subsequently installed through the reviewed ServiceManagement
  route. Its exact root broker and fresh 256 MiB/32768-node
  `tmpfs,local,nodev,noexec,nosuid` runtime were verified from kernel paths;
  the runtime and login Keychain remain on different devices and `auth.json`
  remains absent. A real official OAuth/MFA flow reached completion, then
  failed closed with sanitized `persist_failed`. Kernel Seatbelt evidence binds
  the login-only Codex PID to a denied Security.framework atomic-save create at
  the exact shape `login.keychain-db.sb-<8 hex>-<6 alphanumeric>`. Repair12
  permits only `file-write-create` for that anchored sidecar shape in the
  login-only profile. The read-only model profile receives no such rule;
  focused sandbox tests prove invalid names, rewriting an existing sidecar, and
  writing through a pre-created matching hard link all remain denied. The full
  203 ordinary Rust, 56 broker/signing, and 64 App suites plus release builds,
  strict lint/format, metadata, and script checks pass. Synchronized
  Developer-ID App
  `/private/tmp/OpenOpen-FridayAlpha-KeychainSidecar-R12-final2.app` has 18
  directories plus 617 files and normalized manifest SHA `b954ae64…`;
  App/Core/broker CDHashes are `549b4ad3…`, `07ad9ba3…`, and `6012c638…`.
  Exact mounted/copied-verifier DMG SHA is `16a11f84…`; embedded PROVENANCE is
  byte-identical and Gatekeeper honestly reports `Unnotarized Developer ID`.
  Two fresh pre-install reviewers remain pending. No
  credential body, account/model success, provider Outcome, integrated Mission,
  milestone, notarization, or release proof is claimed.

- Both first Repair12 replacement reviews are invalidated. Product and security
  independently reproduce one P1: create-only sidecar authority reaches the
  next Security.framework `fchmod` step and fails with `file-write-mode`, so an
  R12 install would predictably repeat `persist_failed`. Repair13 grants only
  the anchored login-only sidecar operations observed across the complete
  atomic-save lifecycle: create, mode, owner, flags, times, and unlink. It adds
  no separate sidecar `file-write-data` or generic directory rule; creation
  necessarily allows bytes through the newly created descriptor. A disposable
  Keychain save/readback now succeeds inside the exact profile while the model
  profile fails; invalid names fail, and a pre-created matching hard link still
  cannot be content-written or renamed over the database. Same-UID metadata on
  that pre-created link can change, which is explicit and does not widen content
  authority. The full 203 ordinary Rust, 56 broker/signing, and 64 App suites,
  release builds, strict lint/format, metadata, and script checks pass.
  Synchronized Developer-ID App
  `/private/tmp/OpenOpen-FridayAlpha-KeychainAtomic-R13-final4.app` has 18
  directories plus 617 files and normalized manifest SHA `2a173700…`;
  App/Core/broker CDHashes are `d2f44ba3…`, `9fdfe5ac…`, and `6012c638…`.
  Exact mounted/copied-verifier DMG SHA is `a4288eae…`; embedded PROVENANCE is
  byte-identical and Gatekeeper honestly reports `Unnotarized Developer ID`.
  Its later review/install/provider diagnostics are historical inputs to the
  current repair and neither package is release proof.

- Repair14 removes the singular Mission-channel-origin limitation. One primary
  route is created atomically with the Mission; an additional durable paired
  route requires one explicit owner approval naming its exact channel,
  conversation, owner, inbound classes, and outbound classes. Additional
  outbound classes default Off. Accepted bound inbound becomes a typed event
  on the same Mission and never a new Outcome, new Mission, scope grant, or
  completion Evidence. Outbound progress, Need you, and Receipt use exact
  per-route class/revision authority and durable provider identity. Empty and
  one-origin migration, invalid-state rejection, 100 duplicated/out-of-order
  events across iMessage and Discord, 10 concurrent Missions, restart,
  wrong-owner/pairing/revision/class, response-loss, and Global Off pass. The
  first final product reviewer then found that Swift discarded the typed event
  and terminal Mission routes captured later explicit input; the security
  reviewer otherwise passed. Repair14 final2 atomically applies a
  Store-verified participation command that changes only the existing Mission
  revision/timestamp, leaves approvals/scope/Evidence unchanged, exposes exact
  correlated metadata in Swift, and lets terminal routes yield to the existing
  explicit-Outcome path. Both fresh final2 reviewers then found one shared P1:
  after an additional route advanced the set, Swift rejected the immutable
  historical event returned for duplicate/restart recovery. Final3 accepts
  history only when the event revision is not in the future and the exact route
  already existed at that revision; future and unknown routes still fail
  closed. The first final3 reviewer pair found no implementation fault, but
  rejected one stale current-blocker paragraph and the missing direct
  pre-route-history regression. Both are synchronized, and final3 now directly
  tests valid historical, future, unknown, and pre-route cases. The full local
  matrix is 215 ordinary Rust tests with two explicit
  real-runtime diagnostics ignored, 56 broker/signing Swift tests, 71 App
  Swift tests, and strict
  release/lint/format checks. The synchronized final3
  Developer-ID App has 18 directories plus 617 files,
  preserves the exact broker CDHash `6012c638…`, and embeds byte-identical
  PROVENANCE. App/Core/worker CDHashes are `abe55bfb…`, `f2b144f5…`, and
  `9200195f…`. Exact-verifier unnotarized DMG SHA-256 is
  `43167af0fdc03c6d2ff9c39340b25535d57a5baed4728cb6c913eb394dfc45d9`.
  Full static scans, two fresh pre-install reviews, installation, and one real
  correlated same-build Mission remain pending.

- Repair15 closes the real iMessage discovery contract mismatch exposed only
  after Repair14 was installed and OpenOpen received official Full Disk Access.
  The exact bundled basic-RPC `chats.list` route returns 23 bounded iMessage
  chats, but 21 legitimate chats have an empty optional display name. Host now
  accepts only an empty-but-bounded, NUL-free, trimmed name and preserves it for
  the existing Swift participant fallback; every participant, service, positive
  unique ID, sort/dedupe, pairing, and scope gate remains unchanged. A
  successful zero-result discovery now says `No Messages conversations found.`
  instead of remaining silent. Focused tests and the complete current matrix
  pass: 220 executed Rust tests plus two explicit real-runtime diagnostics
  ignored, 56 broker/signing Swift tests, and 72 App Swift tests, together with
  release, strict lint/format, metadata/script/plist, diff, conflict, and
  secret-pattern checks. Synchronized Developer-ID final2 App contains 18
  directories plus 617 files with normalized manifest SHA `7edd3067…`;
  App/Core/broker/worker CDHashes are `ad99ab59…`, `416052fb…`, unchanged
  `6012c638…`, and unchanged `9200195f…`. Exact-verifier DMG SHA is
  `6bcf0ade…`; embedded PROVENANCE SHA is `26b575bf…`, deep verification passes,
  and Gatekeeper honestly reports `Unnotarized Developer ID`. No message body,
  participant value, token, pairing, send, Mission, Receipt, or milestone proof
  is claimed. Two fresh pre-install reviewers remain pending.

- Installed Repair15 subsequently completed one exact owner-selected durable
  iMessage pairing through the official UI, with no message send. The first
  approved real Discord setup attempt used the existing Friday token only in
  memory and typed it directly into OpenOpen, but failed before provider access:
  `DiscordTokenKeychain` selected the data-protection Keychain and the direct
  Developer-ID product returned `errSecMissingEntitlement (-34018)`. Repair16
  removes only that selector and uses the same single native macOS login
  Keychain backend as the established broker store; no retry or second backend
  exists. Exact service/account/accessibility, token validation, update/insert,
  read/delete, and redacted errors remain unchanged. Disposable unique-item
  tests complete save/readback/update/delete with cleanup and independently
  reproduce the legacy `-34018` route. The connection UI now disables iMessage
  pairing until both exact choices exist, replaces the paired action with an
  unmistakable connected state, and shows concise Discord setup success/failure
  feedback without token content. The first frozen candidate is invalidated:
  its security reviewer passed, but the product reviewer found one P1 because
  the real adapter starts as `connecting` while Swift claimed connected, and
  one P2 because paired iMessage selection controls remained mutable. The
  replacement tracks connecting/reconnecting/connected/faulted without stale
  success text and locks discovery/conversation/owner selection while the
  durable route is connected. The current ordinary suites pass 220 Rust,
  56 broker/signing Swift, and 79 App Swift tests. The real Discord Keychain
  item remains absent; no Discord connection, pairing, message, Mission, or
  milestone is claimed. The prior manifest `452f9c0e…`, App CDHash
  `c979d2d3…`, and DMG `4080763d…` are invalid historical artifacts and must
  not be installed. Synchronized replacement Developer-ID App
  `/private/tmp/OpenOpen-FridayAlpha-DiscordState-R16-final2-preinstall.app`
  has 18 directories plus 617 files and manifest `b8770e96…`;
  App/Core/broker/worker CDHashes are `9de8d932…`, unchanged `416052fb…`,
  clean-rebuilt `002e5156…`, and unchanged `9200195f…`. Broker source, trust,
  sandbox, and authority are unchanged. Exact mounted/copied-verifier DMG SHA
  is `03c4f691824005ced78bf0c1ebf9cd135c74077110bac8305eed3b36ba03cc9e`;
  Gatekeeper honestly reports `Unnotarized Developer ID`. The next product
  reviewer found one P2 test gap because the production `reconnecting` branch
  had no direct Swift regression; the in-progress security review is not
  reusable. A direct connecting→reconnecting→connected/no-stale-success
  regression now closes that gap without changing production or signed App
  bytes. The following product review found another P2: a late Discord-start
  failure could overwrite Global Off cleanup because `connectDiscord` alone
  lacked the generation guard used by adjacent channel flows. Repair16 now
  rejects that stale callback and deterministically proves Off leaves Discord
  disconnected with no feedback/error or polling. The prior signed App/DMG is
  invalidated. The final3 product reviewer then found one P1: late iMessage
  activation and channel-poll callbacks could overwrite Global Off cleanup;
  product and security also found the same stale final2 documentation P2. The
  repair validates the captured runtime generation immediately after every
  channel await and before status, feedback, Mission-event, or suggestion
  mutation; stale cleanup cannot stop a newer generation. Delayed activation
  and delayed poll regressions prove Off remains disconnected with no late
  event, suggestion, feedback, error, or continuing poll. The complete
  220-Rust/56+81-Swift strict matrix passes. Synchronized final4 Developer-ID
  App has 18 directories plus 617 files, manifest `5c155f4b…`, and
  App/Core/broker/worker CDHashes `77ffc388…`, `416052fb…`, `002e5156…`, and
  `9200195f…`. Exact-verifier DMG SHA is
  `ef1e3cb9b7eab22812cab4af2f6c81de203a9be6e02846e3210625dae4737da4`;
  embedded PROVENANCE is byte-identical and Gatekeeper reports `Unnotarized
  Developer ID`. Final4 security review passed, but product review invalidated
  it with P1×1 because a lost Discord-start response left the already-created
  provider session non-reattachable, plus P2×1 because the delayed-poll test
  used a cancellation-aware delay and never proved rejection of a callback
  that actually returned after Off. Final5 makes start retry idempotent only
  for the exact durable Discord pairing, returns the existing session state
  without creating another provider session, and rejects any changed pairing.
  Its poll test uses a non-cooperative delayed callback and proves that callback
  returns exactly once but cannot republish status, suggestion, Mission event,
  error, or polling after Off. The complete 221-Rust/56+82-Swift strict matrix
  passes. Synchronized final5 Developer-ID App has 18 directories plus 617
  files, normalized manifest `bdeff03b…`, and App/Core/broker/worker CDHashes
  `aea19e6f…`, `fd7b31d7…`, unchanged `002e5156…`, and unchanged `9200195f…`.
  Exact-verifier DMG SHA is `bd8f2b08682ba8e1bc833ee16a5b37ef2c7c62ca6cc9ceea8be8dabbeea7bbd9`;
  embedded PROVENANCE SHA `1f27067c…` is byte-identical and Gatekeeper reports
  `Unnotarized Developer ID`. All earlier Repair16 packages and reviews are
  historical. Final5 product review then found a distinct P1 at the preceding
  pairing-confirm commit/reply boundary: Host could durably pair and clear its
  setup session before a lost response left Swift showing a stale Confirm
  action that could never succeed. Final6 recovers only when the durable
  pairing exactly matches the confirmed candidate's owner, conversation,
  provider identities, setup message, and candidate ID; it then clears stale
  setup UI and starts the exact route. The complete 221-Rust/56+83-Swift strict
  matrix passes. Synchronized final6 Developer-ID App has 18 directories plus
  617 files, normalized manifest `9d0ee12a…`, and App/Core/broker/worker
  CDHashes `b053692c…`, `fd7b31d7…`, `002e5156…`, and `9200195f…`.
  Exact-verifier DMG SHA is
  `ca9f24d4d02f6b71b4db52f041dc9934cce7b623a4fe2f68cf2e03c9a9684057`;
  embedded PROVENANCE SHA `0bca09ab…` is byte-identical and Gatekeeper reports
  `Unnotarized Developer ID`. Final6 product review invalidated that candidate
  with P1×1: an exact already-present terminal Discord session was returned as
  faulted forever, so the visible retry could not restart the provider without
  a whole App/Global-Off cycle. Its security review was interrupted and is
  non-reusable. Final7 retains exact response-loss reattachment for a pending
  or live session, while a terminal faulted or launch-complete disconnected
  session is stopped and replaced exactly once for the unchanged durable
  pairing. The complete 222-Rust/56+84-Swift strict matrix passes. Synchronized
  final7 Developer-ID App has 18 directories plus 617 files, normalized
  manifest `5044a828…`, and App/Core/broker/worker CDHashes `ad567d44…`,
  `8905f8ee…`, `002e5156…`, and `9200195f…`. Exact-verifier DMG SHA is
  `8ea865939db29740fdd65c3f0c658c18f67f62be22b302ed6917a6da813e095b`;
  embedded PROVENANCE SHA `3f32b64c…` is byte-identical and Gatekeeper reports
  `Unnotarized Developer ID`. Final7 product review invalidated that candidate
  with P1×1: raw adapter Connected and an outbound handle could appear before
  restart recovery's envelopes and final cursor were durably accepted, while
  a failed receiver could remain installed and wedge retries. Its security
  review is interrupted and non-reusable. Final8 retains each exact recovered
  event until Store acknowledgement, repeats it after Store failure or response
  loss, denies Connected/outbound while unresolved, atomically rejects malformed
  batches, stops failed sessions, and clears pending recovery on Global Off.
  The outbound handle gate now precedes Mission approval and durable outbound
  intent mutation. The complete 227-executed-Rust/56+84-Swift strict matrix,
  release/static/metadata/notice checks, 100 dual-route duplicate/out-of-order
  events, and 10 concurrent Missions pass. Synchronized final8 Developer-ID
  App has 18 directories plus 617 files, normalized manifest `64fc1bff…`, and
  App/Core/broker/worker CDHashes `ead00b7e…`, `b3b85a63…`, unchanged
  `002e5156…`, and unchanged `9200195f…`. Exact-verifier DMG SHA is
  `18f9cb2bc7bc1b134469fdba9ec720835c68df487e1e7fcd44499920f102c866`;
  embedded PROVENANCE SHA `1d93a19a…` is byte-identical and Gatekeeper reports
  `Unnotarized Developer ID`. Final8 product review invalidated that candidate
  with P1×1: a recovered first intent could start GPT and surface an Outcome
  before a later correction and final provider high-water cursor were durable.
  Its unfinished security review is interrupted and non-reusable. Final9
  durably accepts the complete recovery sequence first, then releases the
  oldest queued model dispatch only after the final high-water cursor is
  acknowledged. A two-message original-intent→correction Host regression proves
  both messages remain queued and no suggestion is surfaced during partial
  recovery. The complete 228-executed-Rust/56+84-Swift strict matrix,
  release/static/metadata/notice checks, 100 dual-route duplicate/out-of-order
  events, and 10 concurrent Missions pass. Synchronized final9 Developer-ID
  App has 18 directories plus 617 files, normalized manifest `7c666b20…`, and
  App/Core/broker/worker CDHashes `8b7c2689…`, `4722f57a…`, unchanged
  `002e5156…`, and unchanged `9200195f…`. Exact-verifier DMG SHA is
  `15e4ca4f8851e63b5b9a64da8ba7300e8683422afe84b8e490c83016cbdb4c0a`;
  embedded PROVENANCE SHA `cd7d15d7…` is byte-identical and Gatekeeper reports
  `Unnotarized Developer ID`. Final9 product review invalidated that candidate
  with P1×1: after cursor closure, the oldest queued result could still surface
  while a later correction remained queued; Swift then retained the obsolete
  result, and restart Dashboard restoration could expose it before recovery.
  Its security review is interrupted and non-reusable. Final10 withholds and
  rejects confirmation of any channel suggestion while queued/started work
  remains, restores only the newest ready result after recovery, and gives the
  final GPT turn a bounded chronological same-owner/same-conversation correction
  context proven by Store audit order. Swift clears a recovering stale channel
  result and replaces it only with the final Host-arbitrated Outcome. The
  complete 228-executed-Rust/56+85-Swift strict matrix, release/static/metadata/
  notice checks, 100 dual-route duplicate/out-of-order events, and 10 concurrent
  Missions pass. Synchronized final10 Developer-ID App has 18 directories plus
  617 files, normalized manifest `c19d08db…`, and App/Core/broker/worker CDHashes
  `5dd1cf48…`, `5c5aa618…`, unchanged `002e5156…`, and unchanged `9200195f…`.
  Exact-verifier DMG SHA is
  `9c786aa40a39340033fadf4d7f2864945cd0b4f1f7cad8b15b47a3cddc56644c`;
  embedded PROVENANCE SHA `c7af7a7e…` is byte-identical and Gatekeeper reports
  `Unnotarized Developer ID`. Both entirely fresh final10 reviewers independently
  invalidate that package with P0/P1/P2=`0/1/0`: audit overlap proved only
  concurrency, not that a same-owner message was a correction, so unrelated
  intents could be merged. Final11 requires the exact case-insensitive owner
  directive `Correction to previous:` before importing only the immediately
  preceding audit-qualified message; time overlap alone imports nothing and an
  unmatched directive stays single-message. The complete final11 matrix passes:
  230 executed Rust tests with two explicit real-runtime diagnostics ignored,
  56 broker/signing plus 85 App tests, 100 dual-route duplicate/out-of-order
  events, 10 concurrent Missions, release, strict Clippy/warnings/format,
  metadata, notices, plist/script, diff, conflict, credential-path, and secret
  checks. Synchronized final11 Developer-ID App has 18 directories plus 617
  files, normalized manifest `5bf86d5d…`, and App/Core/broker/worker CDHashes
  `6caafeac…`, `88f76fae…`, unchanged `002e5156…`, and unchanged `9200195f…`.
  Exact read-only mounted/copied-verifier DMG SHA is `a2887a91…`; embedded
  PROVENANCE SHA `9fd57db9…` is byte-identical and Gatekeeper honestly reports
  `Unnotarized Developer ID`. Final11 reviewers invalidate that package before
  install: product reports P0/P1/P2=`0/0/1` because Host accepted a caller-built
  3+ message context; security reports `0/1/1` because Store could skip a
  started immediate predecessor for an older ready result and one current-state
  paragraph still named final10 pending. Final12 serializes each channel's model
  dispatch, atomically begins only the oldest queued source, selects the
  immediate accepted predecessor before readiness/audit qualification, and
  independently rejects Host context above two. Focused serial FIFO,
  immediate-predecessor, unrelated-intent, explicit-correction, and overbound
  tests pass. The complete final12 matrix passes: 231 executed Rust tests with
  two explicit real-runtime diagnostics ignored, 56 broker/signing plus 85 App
  tests, 100 dual-route duplicate/out-of-order events, 10 concurrent Missions,
  release, strict Clippy/warnings/format, metadata, notices, plist/scripts,
  diff, conflict, credential-path, and secret checks. Synchronized final12
  Developer-ID App has 18 directories plus 617 files, normalized manifest
  `74b71f64…`, and App/Core/broker/worker CDHashes `be5761e1…`, `8b75c01e…`,
  unchanged `002e5156…`, and unchanged `9200195f…`. Exact read-only mounted/
  copied-verifier DMG SHA is `d09a8008…`; embedded PROVENANCE SHA `80d25f13…`
  is byte-identical and Gatekeeper honestly reports `Unnotarized Developer ID`.
  Final12 reviewers invalidate it before install: product reports P0/P1/P2=
  `0/1/1`, security `0/0/2`. Both find stale final11/final10 current-state
  text; product also proves a distinct restart P1 because Host selected only
  queued work and could not surface the exact durable started dispatch through
  the existing recovery-only `Need you` route. Final13 selects the exact single
  started source before any queued work, never repeats the model call, fails
  closed on multiple started rows, surfaces local `Need you`, and stops
  automatic polling for the paused channel. The complete final13 matrix passes
  233 executed Rust tests with two explicit real-runtime diagnostics ignored,
  56 broker/signing plus 86 App tests, both stress suites, release, strict
  lint/format, metadata, notices, plist/scripts, diff, conflict,
  credential-path, and secret checks. Synchronized final13 Developer-ID App
  has 18 directories plus 617 files, manifest `dc83e390…`, App/Core/broker/
  worker CDHashes `5691aaad…`/`c07958ff…`/unchanged `002e5156…`/`9200195f…`,
  and embedded PROVENANCE `f027dc0d…`. Exact read-only mounted/copied-verifier
  DMG SHA is `9d6a1eb0…`; Gatekeeper honestly reports `Unnotarized Developer
  ID`. Both final13 reviewers independently pass code, recovery, security,
  full suites, and package checks but invalidate it on one shared P2: a later
  live Master Plan blocker paragraph still named final12 current. Final14
  changes only that stale paragraph and records final13 as historical. Its
  complete 233-Rust/56+86-Swift matrix, both stress suites, release, strict
  lint/format, metadata, notices, plist/scripts, diff, conflict,
  credential-path, and secret checks pass. Synchronized final14 Developer-ID
  App has 18 directories plus 617 files, manifest `69a28c4f…`, App/Core/
  broker/worker CDHashes `fdd2cd29…`/`c07958ff…`/unchanged `002e5156…`/
  `9200195f…`, and embedded PROVENANCE `f027dc0d…`; exact mounted/copied-
  verifier DMG SHA is `1ca74ccd…`, and Gatekeeper reports `Unnotarized
  Developer ID`. This pre-install statement is historical: final14 was later
  installed with administrator approval and passed real broker/tmpfs/account/
  model checks. One durable iMessage pairing exists; Discord setup, the real
  connected Alpha Mission, post-install Alpha review, and all new competition-
  phase reviews remain pending.

- The first installed-final14 Discord setup reached the official Gateway and
  produced the setup session without exposing the Owner-entered Keychain token,
  but the visible SwiftUI instruction localized the 19-digit bot snowflake and
  inserted grouping commas, so the displayed mention was not valid Discord
  syntax. Repair16 final15 constructs the exact line from
  `String(botUserId)` and displays it with `Text(verbatim:)`; a grouping-locale
  regression proves the pure-decimal mention, exact separators, and unchanged
  32-lowercase-hex pairing code. Before that candidate reached review, the
  Owner sent the corrected real pairing command exactly once through Discord's
  actual mention picker. The official OpenOpen Check action then remained
  inaccessible to Computer Use because the Connections page kept a
  `SecureField` mounted at all times. Repair17 batches both Alpha presentation
  fixes: the Connections page contains no secret field until the user opens an
  explicit auto-focused secure sheet, and cancel, submit, sheet dismissal, or
  Global Off erases the ephemeral draft. Token persistence and setup still use
  the unchanged single login-Keychain path. The complete
  233-executed-Rust/56+90-Swift matrix plus release/strict/static/notice checks
  passes locally. The synchronized Repair17 Developer-ID App has 18
  directories plus 617 files, normalized manifest `6ae53c60…`, App/Core/
  broker/worker CDHashes `ece5a62f…`/`c07958ff…`/unchanged `002e5156…`/
  `9200195f…`, and embedded PROVENANCE `a08fc810…`. Its exact read-only
  mounted/copied-verifier DMG SHA is `a1b3cf15…`; Gatekeeper honestly reports
  `Unnotarized Developer ID`. The Owner-authorized comma-removal workaround is
  historical, not final UX. The first Repair17 product reviewer rejects the
  frozen execution order with P0/P1/P2=`0/1/1`: Global Off/install would destroy
  the current host-memory setup and its already-sent random pairing code, and a
  ledger row incorrectly still called the synchronized package pending. The
  incomplete security review is non-reusable. Before any Repair17 install, the
  Owner-visible current Repair16 UI must Check the existing message, verify the
  exact approved Friday guild/channel/owner, and action-time Confirm durable
  pairing. Losing that live session is an external UI boundary; it does not
  authorize a second message. This documentation-only correction leaves all
  package bytes unchanged. Two entirely fresh reviewers, one later consolidated
  Repair17 install, and the real connected Mission remain pending.

- On 2026-07-17 the Owner completed the official Discord Check and Confirm
  actions. The authoritative Store then contained exactly two durable pairings
  (iMessage and Discord) and zero Mission, Receipt, or outbound rows. The
  canonical OpenOpen Discord identity is application/bot
  `1472779237038493808`, guild `1476443817946124481`, channel
  `1476443818646831229`, and owner `370355408730324993`; it supersedes the
  separate June 24A environment for OpenOpen only. The credential remains only
  in OpenOpen Keychain. Real runtime evidence then invalidated Repair17 before
  install: Core and its Codex/imsg children exited at 14:51:52 PDT while App and
  broker remained, leaving disabled iMessage controls and cached Discord green
  state. Repair18 removes the root cancellation bug that closed shared Core
  stdin, adds one typed lifecycle signal, immediately invalidates cached
  providers, boundedly restores both exact durable listeners without overlapping
  Core generations, and enters `Need you` after three failures. Its first frozen
  package was invalidated before install when both fresh reviewers reported
  P0/P1/P2=`0/2/1`: cleanup could launch uncounted Core generations, Global Off
  did not fence recovery/startup listener RPCs, and cancellation-before-write
  lacked a deterministic regression. The replacement makes cleanup require an
  existing Core, terminates and verifies the exact Core generation after every
  failed recovery attempt, interrupts already-written noncooperative work for
  Off, and removes the unreachable pre-write tombstone. That final2 replacement
  passed the complete local matrix and produced App manifest `38d7cadd…` and DMG
  SHA `acfa05b2…`, but its two fresh reviewers invalidated it before install:
  product reported P0/P1/P2=`0/1/0`, security `0/1/1`. A single recovery attempt
  could still silently start Core B after Core A exited and then publish a
  stitched dual-listener/account/model result; running-Code validation also
  captured a token independently from the token retained for exact termination.
  Final3 added one explicit generation fence around each complete recovery and
  retained one captured audit token, then passed its full deterministic matrix
  and produced uninstalled App manifest `e55269d3…` and DMG SHA `856f5403…`.
  Both fresh final3 reviewers invalidated that package with P0/P1/P2=`0/1/0`.
  Product found that startup provisioning/dashboard/Codex calls ran before the
  fence and could therefore stitch generations or escape Global Off tracking.
  Security found that a post-launch validation/bootstrap failure could ignore
  exact termination and launch a replacement while the old child remained
  live. Final4 tracks and fences the entire startup restore from its first Core
  RPC. A failed child is forgotten only after the same audit-token terminator is
  accepted and exact exit is observed; otherwise it is quarantined without an
  input pipe and blocks every replacement launch until exact exit. Three startup
  or recovery failures pause with `Need you`, and Global Off cancels the tracked
  startup prefix. All 233 executed Rust tests (two explicit real-runtime
  diagnostics ignored), 56 broker/signing tests, and 108 App tests pass with
  both release builds, strict Clippy/warnings/format, locked metadata, notices,
  plist/scripts, diff, conflict, and credential-shape checks. Package and review
  status remains external exact-fingerprint evidence; this document never
  self-authorizes installation. Final4 subsequently passed both fresh reviews,
  was installed, and proved its exact signed runtime plus both durable paired
  listeners with zero Mission/Receipt/outbound rows. The first approved real
  Discord Alpha intent was then sent exactly once as provider message
  `1527903489252921485`; it was durably accepted and claimed once, but the
  pinned Codex child exited before a structured Outcome. Repair19 removes the
  unsupported structured-output `uniqueItems` keyword while retaining
  duplicate-source rejection after parse, terminalizes failed claimed model
  work without replay, keeps explicit-correction polling live, and starts
  bounded exact-Core recovery. A final1 verification race additionally showed
  stdout EOF waking a retry before the Process callback revoked the old input;
  final2 revoked that generation's shared input first, but two fresh reviews
  invalidated the candidate before install. Security proved Global Off could
  still race a late durable model result and an already-installed pre-write RPC
  retained its captured input handle after EOF revocation. Product found stale
  final4/current-package text. Final3 holds the exact active-operation gate
  through reconciliation, requires the signed Store runtime to remain On in the
  same immediate transaction, and serializes stdout EOF revocation with the
  request's last pre-write generation/input/pending check. Its complete
  235-executed-Rust (two explicit external-runtime diagnostics ignored) and
  56+110 Swift suites pass, but fresh final3 product review invalidated that
  package with P0/P1/P2=`0/1/0` while security passed `0/0/0`: successful Core
  recovery cleared the transient warning and the restarted Host did not
  rediscover the durable failed dispatch. Final4 persistently returns `Need
  you` only while the newest accepted dispatch is failed; a later explicit
  queued/ready correction supersedes it, and recovery never grants another
  model call. Both fresh final4 reviewers then invalidated that package with
  P0/P1/P2=`0/1/0`. Product proved persistent failure feedback returned before
  the only provider poll, starving the correction it required. Security proved
  a Discord failure could clear a valid iMessage Outcome because Swift cleared
  any channel suggestion without exact origin identity. Final5 keeps the
  transport poll reachable, persists a provider correction before it can
  supersede the failure, and returns an exact same-channel suggestion ID for
  invalidation; other-channel Outcomes remain visible. Final5b passed two fresh
  reviews, was installed, and atomically terminalized the sole consumed
  dispatch without replay, resend, suggestion, Mission, Receipt, or outbound
  work. Official Global Off then exposed Repair20: after exact old-Core
  shutdown, the replacement Core lacked the volatile broker enrollment and
  rejected signed runtime-history validation before the later provisioning
  step. Repair20 provisions the already pinned enrollment first only for that
  positively quiesced replacement branch, then uses the unchanged protected
  Off transaction. Normal live-Core Off still prepares/cancels before
  provisioning; unproven exact shutdown performs neither prepare nor broker
  apply. The first replacement review pair invalidated final1 before install:
  product reported P0/P1/P2=`0/0/1` for the absent same-model failed-shutdown
  retry regression, and security reported `0/1/0` because a newer On could
  publish false convergence without restoring the stopped Core and listeners.
  The replacement state machine now finishes the protected Off transaction
  after exact quiescence even when On arrives, and exposes On only after the
  exact Core generation, Codex, account/models, and both durable listeners are
  restored. Failed shutdown→retry Off, failed shutdown→revalidated On,
  quiesced Off→newer On, and restoration→newer Off all prove zero model/provider
  replay. Both fresh final2 reviewers rejected that package with
  P0/P1/P2=`0/2/0`: a refused exact Core terminator could not be retried on its
  same captured audit token, and the product could publish On before listener
  restoration completed. Final3 quarantines and retries the same exact Core
  generation. Fresh final3 product/security review then rejected it with
  `0/1/0` and `0/2/0`: ordinary persisted protected Off→On bypassed the special
  restoration latch, and Discord `connecting`/`reconnecting` counted as ready.
  Final4 keeps every production-lifecycle protected On Turning On and
  non-model-capable until the fenced Core, Codex, account/models, iMessage, and
  exact Discord `connected` complete. Bounded connecting timeout or terminal
  status pauses fail closed; a newer Off interrupts without replay. Both fresh
  final4 reviewers invalidated that package before install: product reported
  `0/1/0` for transient Unknown during Core-death recovery, while security
  reported `0/2/0` for missing exact account/model readiness and missing exact
  Discord-Connected model gating. Final5 keeps Core-death recovery Turning On,
  requires a fresh managed ChatGPT plus `gpt-5.6-sol`/`high` proof before model
  entry or displayed On, and requires exact Discord Connected for model and
  outbound work after recovery. Account/model absence never blocks protected
  Off. Final5 passed its complete matrix and two fresh reviews, was installed,
  and preserved pairings=2, the failed/no-suggestion dispatch, and Mission/
  Receipt/outbound=0/0/0. Its bounded recovery then reached `.paused`; one exact
  official Off attempt exposed that `.paused` was omitted from the existing
  replacement-Core provisioning condition. Repair21 adds only that state to the
  reviewed quiesced path, which provisions pinned broker trust before the
  replacement Core prepares Off. Focused success and shutdown/provision/prepare-
  failure regressions preserve monotonic Store authority and prove zero false
  Off/model/provider work. Repair21 passed 238 executed Rust tests with two
  explicit real-runtime diagnostics ignored, 56+127 Swift tests, synchronized
  Developer-ID packaging, and two fresh P0/P1/P2=`0/0/0` reviews. The installed
  build then committed protected Off at revision 28 with pairings=2, the failed
  dispatch unchanged, and Mission/Receipt/outbound=0/0/0. Official On advanced
  to revision 29 but exposed a deterministic Discord restore deadlock: the App
  required Connected before calling the existing typed recovery poll that
  clears a durable cursor's `recovery_required` state. Repair22 drives that
  bounded, cancellable typed recovery inside the Core-generation fence before
  publishing Connected or enabling model/outbound work. The complete Repair22
  matrix passes 238 executed Rust tests with two explicit real-runtime
  diagnostics ignored, 56+140 Swift tests, release, strict lint/format,
  metadata, notices, scripts, and secret scans. Synchronized candidate and
  reviewer identities remain external exact-fingerprint evidence; two fresh
  reviewers and any later install remain pending. Final1 through final4 are
  invalid historical artifacts.
  Repair22 final2 passed two fresh reviews, was installed, durably drained the
  cursor, and restored both approved listeners. The exact Owner-approved
  correction was sent once as provider message `1528211998263738570`, observed
  and dispatched once, then failed before structured-output parsing. The
  original and correction dispatches are terminal and never retry. Repair23
  closes the pinned 0.144.0 passive-progress incompatibility: it uses the exact
  `item/agentMessage/delta` name, accepts only the bounded identity-checked
  reasoning/token metadata set, and keeps unknown/tool/action/reroute events
  fail closed. Repair23 final2 was invalidated before installation when its
  fresh product reviewer found that allowed passive metadata did not fully
  reject pinned-schema-malformed active flags and rate-limit fields. Both fresh
  final3 reviewers then invalidated that package before installation with
  P0/P1/P2=`0/0/2`: `turn/started` did not require `items`, allowed lifecycle
  and completed items did not require bounded IDs and their mandatory fields,
  and the embedded current-state text still called the package pending. Final4
  added those shapes, but its fresh product/security reviewers invalidated it
  before install with combined P0/P1/P2=`0/1/1`: malformed nested
  `memoryCitation` objects passed, and turn-validation/output errors did not
  terminate the reusable Codex transport. Final5 closed those findings, but
  both fresh final5 reviewers invalidated it before install with
  P0/P1/P2=`0/1/0`: terminal `itemsView: summary|notLoaded` was accepted as a
  complete authority list and could hide tool/action items. Final6 is invalid:
  it did not validate optional pinned Turn fields on `turn/started`, and its
  Swift log ended in `swift-format: command not found` while its receipt called
  that log PASS. Supervisor-approved final7 consolidates all three production
  Turn routes behind one stage-aware validator, validates complete accepted
  nested item shapes and lifecycle/error consistency, grants output authority
  only to the terminal full item list, and retires the transport after every
  malformed protocol or output path. Its complete current-fingerprint matrix
  passes 253 executed Rust tests with two explicit real-runtime diagnostics
  ignored, 56 broker/signing plus 140 App tests, release builds, strict lint/
  format, locked metadata, notices, plist/scripts, diff/conflict, and secret
  scans. Exact package and fresh-review facts are bound only by the external
  immutable final7 receipt; this current-state text does not self-certify or
  dynamically negate them. Installed final7b then exposed one distinct
  product-liveness defect: the same durable terminal failure reopened a
  blocking modal on every poll. Repair24 preserves the failed dispatch and
  audit, persists one incident plus atomic acknowledgement, keeps acknowledged
  history non-blocking, scopes acknowledgement feedback/refresh per incident,
  validates channel DTOs before use, and isolates already-connected route
  effects from missing model readiness without allowing new model work. Its
  required typed poll capability also prevents account-pending or recovery
  polling from selecting or starting model work. A fresh pre-freeze Product
  Scout reports P0/P1/P2=`0/0/0`. Final6's Rust matrix passed but its Swift
  matrix exposed a fixed-delay test-scheduling race and remains historical
  FAIL evidence. A supervisor authorized only deterministic Dashboard-entry
  synchronization in the two sibling tests. The resulting fingerprint must
  rerun the 269-Rust/two-ignored and 56+193-Swift matrix, release builds, strict
  lint/format, metadata, notices, plist/scripts, diff/conflict, and bounded
  secret scanning. Any signed package, frozen-fingerprint Product
  Scout, and fresh-review facts are bound only by a later external immutable
  receipt; installation, a new explicitly approved input, and the real
  connected Mission remain required. No milestone is claimed.

## Friday alpha target (not yet achieved)

`FRIDAY_ALPHA_READY` requires Hero A plus real bidirectional iMessage and
Discord surfaces for the same bounded Mission loop. Both channels must enforce
pairing/allowlists, durable message-ID dedupe and cursor recovery, restart
without duplicate sends, and global Off preventing listener, model, and
outbound work. A sent chat message is never completion Evidence. Hero B and
Hero C followed this historical milestone in the prior full-product roadmap.
They are excluded from the current `BUILD_WEEK_COMPETITION_READY` scope by the
dated canonical contract in the Master Plan.

Items move from Planned to Built only after implementation and the stated proof
tier. Built does not imply signed-build or real-provider proof.

## Not claimed

- Friday as a whole was not built during Build Week.
- A green CI run is not proof of a working user experience.
- OpenOpen has no cloud service, mobile app, Telegram adapter, marketplace,
  hidden or unconsented ambient monitoring, private iMessage bridge, payment
  execution, or silent self-upgrade.
- Demo video production is not part of this implementation phase.

## Required submission evidence

The final submission must identify the primary Codex `/feedback` session and
explain separately how Codex accelerated implementation and how GPT-5.6 is
used by the running product.
