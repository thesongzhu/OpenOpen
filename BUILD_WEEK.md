# OpenAI Build Week Disclosure

Current product scope, authority, privacy, milestone, and acceptance semantics
come only from `docs/OPENOPEN_BUILD_WEEK_MASTER_PLAN.md`. This disclosure is a
chronological Build Week/provenance record and cannot authorize implementation
or override that canonical contract.

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

## Planned Build Week additions (not yet claimed)

- The optional macOS voice entry route; Hero A currently uses explicit text
  input.
- Real-provider and restart proof needed to earn `FRIDAY_ALPHA_READY` for the
  implemented bounded iMessage and Discord entry/readback slice, targeted for
  July 16–17, 2026 `America/Los_Angeles`.
- The `JUDGE_SLICE_READY` route: Quick Memory Passport, deterministic
  subscription-aware Auto model routing, direct-local Slack Socket Mode,
  participant-consented Slack/iMessage opportunity previews, and one confirmed
  personalized Hero A outcome. None is claimed implemented by this row.
- Deep ChatGPT/Claude ZIP import with fixed fail-closed resource limits.
- Receipt-image extraction, local XLSX generation, and readback.
- OpenOpen-specific workflow learning, GitHub Skill import, packaging, real
  runtime evidence, and user validation.

## Implemented locally, proof pending

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

## Friday alpha target (not yet achieved)

`FRIDAY_ALPHA_READY` requires Hero A plus real bidirectional iMessage and
Discord surfaces for the same bounded Mission loop. Both channels must enforce
pairing/allowlists, durable message-ID dedupe and cursor recovery, restart
without duplicate sends, and global Off preventing listener, model, and
outbound work. A sent chat message is never completion Evidence. Hero B and
Hero C follow this intermediate milestone and remain required by the final
`PRODUCT_READY_FOR_DEMO` gate.

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
