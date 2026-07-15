# Provenance Map

## Friday

- Upstream: `https://github.com/thesongzhu/Friday`
- Source commit: `4870f31fa088bef7eb9f4f256ec62993b02eda80`
- License: MIT, copyright Friday contributors

Only the smallest tested concepts or implementation portions needed by
OpenOpen may be adapted. Every adapted file must be added to this table before
staging.

| OpenOpen module | Friday source | Adaptation | Verification |
| --- | --- | --- | --- |
| `crates/openopen-core/src/mission.rs` | `rust-core/crates/friday-core/src/mission.rs` | Independent OpenOpen lifecycle adapted from Friday's legal-transition and proof-before-completion invariants | bootstrap `19ecdd9…`; two isolated foundation reviewers PASS; local 95-test verification; follow-up PR run `29370433505` PASS on synthesized merge `d502b3d…`, whose tree equals head `923c88a…`; exact-head and release proof pending |
| `crates/openopen-core/src/store.rs` | `rust-core/crates/friday-storage/src/mission.rs`, `rust-core/crates/friday-storage/src/audit.rs` | Independent encrypted lifecycle persistence and signed/tail-anchored audit design | bootstrap `19ecdd9…`; two isolated foundation reviewers PASS; local 95-test verification; follow-up PR run `29370433505` PASS on synthesized merge `d502b3d…`, whose tree equals head `923c88a…`; product-shell `e2313fe…` passes local 134-Rust/67-Swift verification, two fresh Repair20 reviewers PASS fingerprint `29a00413…`, and PR #2 run `29386477267` PASS on synthesized merge `487dae1…`, whose tree equals the head tree; the first Hero A candidate passed 139-Rust/70-Swift locally but failed both closure reviews; Repair1 passed governance but failed functional review on physical EventKit target drift; both Repair2 reviewers failed its first-write/cancellation/all-markers-missing recovery boundary; both Repair3 reviewers then failed frozen `fa9d905e…` because volatile `createOnce` authority survived possible EventKit commit; Repair4 added durable signed dispatch Evidence but governance rejected fingerprint `4cabaeb4…` because its lower-level writer still accepted reusable Mission copies; Repair5 consumes an internal one-shot dispatch start before any external boundary, passes 146 ordinary Rust plus 83 Swift tests and ad-hoc staging locally, and receives two fresh reviewer PASS reports on frozen fingerprint `4b41a04f…`; Hero A commit `774789c…` is pushed and Actions `29393462659` passes on equal-tree synthesized merge `bccdf360…`; exact-head, signed/admin, cross-UID, provider, and release proof remain pending |

Workflow and Skill adaptations remain planned and are not yet present or
claimed.

No Friday Hub, provider, TypeScript execution route, mobile application, or UI
is imported.

The local Friday working tree was observed detached at `25329515…` during this
phase. Its `origin/main` later advanced to
`0871c37a5f88f30371ad86fa9583fe24c09ef253`; the required pin remains an
ancestor. All provenance inspection and adaptation uses the immutable form
`git show 4870f31fa088bef7eb9f4f256ec62993b02eda80:<path>` and does not import
later upstream changes.

## Other sources

### Friday Discord adapter contract (implemented Rust port; provider proof pending)

- Exact source: Friday commit
  `4870f31fa088bef7eb9f4f256ec62993b02eda80`
- License: MIT
- Relevant paths: `src/channels/friday-channel-adapters.types.ts`,
  `src/channels/discord/discord-config.schema.ts`,
  `src/channels/discord/friday-discord-channel.ts`,
  `src/channels/discord/discord-service.ts`, and the matching dedupe,
  reconnect, adapter, and live-roundtrip tests.
- Allowed adaptation: generic envelope, user/channel allowlists, explicit
  mention gating/stripping, bot filtering, message-ID TTL dedupe,
  reconnect/status, safe `allowed_mentions`, and live roundtrip/restart test
  semantics ported into Rust. The Friday TypeScript/Node runtime is excluded.
- OpenOpen paths: `crates/openopen-discord-adapter/src/lib.rs`,
  `crates/openopen-host/src/channels.rs`, and the shared typed channel records
  in `crates/openopen-protocol` and `crates/openopen-core`.
- Verification: 19 focused adapter tests cover the token-derived identity,
  official `scope=bot` install URL and permission bits `101376`, 128-bit
  pairing code, Ready/intent gate, live permissions/history probe, explicit
  candidate binding, bot/DM/code rejection, identity drift, operational
  traffic, reconnect, dedupe, and recovery. The historical Repair2 local suite
  passed 187 Rust and 91 Swift tests before its replacement-review failure;
  the current Repair3 tree passes 190 Rust and 93 Swift tests. Two fresh
  reviewers PASS fingerprint `3e201547…`; implementation commit `2685b57…`
  is pushed, and PR #2 Actions run `29440208503` passes on synthesized merge
  `99ee2b10…`, whose tree equals the exact head tree. Real Discord
  roundtrip/restart proof remains pending.

### imsg (implemented basic-RPC adaptation; provider proof pending)

- Upstream: `https://github.com/openclaw/imsg`
- Version/tag: `v0.13.0` (annotated tag object `1677a9fe…`)
- Exact dereferenced commit:
  `fa2f82d7dbda4c802d91c1d41bb6c53564ed2fdc`
- License: MIT, copyright 2026 Peter Steinberger
- Allowed surface: one host-managed basic JSON-RPC/stdio child using
  `chats.list`, scoped history/watch, `send`, and `message.send_status`.
  IMCore/private bridge, advanced private operations, SIP changes, and TCP
  daemon/server surfaces are excluded.
- OpenOpen paths: `crates/openopen-imsg-adapter`,
  `third_party/imsg/openopen-basic-rpc.patch`,
  `third_party/imsg/Package.resolved`, and `scripts/build_pinned_imsg.sh`.
- The tracked patch reduces the CLI to `rpc`, uses positive compile-source
  manifests (25 `IMsgCore` and 13 CLI sources), reduces dispatch to the six
  approved methods, and requires selected-chat plain-text AppleScript sends.
  The build script verifies the exact upstream commit, patch, dependency lock,
  source manifests, static forbidden markers, version/help surface, and emits
  schema-v2 build and resource receipts. It never compiles or stages
  `IMsgHelper/IMsgInjected.m`, IMCore/private bridge helpers, SIP changes, or a
  TCP service.
- Historical local integration: the former 8 focused Rust adapter tests and two upstream
  OpenOpen boundary tests passed. A receipt-bound binary with artifact SHA-256
  `626439fbf79a8b7a44bc189c088cda2f9d4c53d5776dfe404f6a65dd83a5fc11`
  is staged at `Contents/Resources/iMessage/0.13.0/bin/imsg` in
  `/private/tmp/OpenOpen-FridayAlpha-Final.app`, which reports
  `STAGED_AD_HOC_NOT_RELEASE_PROOF`. The resulting ad-hoc, unnotarized DMG
  passes read-only mount/copy/signature install testing; its SHA-256 is
  `0f9b7fd3ca54c27138c52fe42a0cb31a3a4a13260d0d945a954d608cab39bd15`.
  That package is now historical only: both current-tree reviewers rejected
  its pre-sign-only runtime binding and found private IMCore/SIP/bridge code
  still compiled into the Mach-O, while a follow-up audit found the locked
  `PhoneNumberKit_PhoneNumberKit.bundle` absent from the App and referenced
  only through a deleted temporary build path. Repair must use an explicit
  compile-source whitelist, a signed-runtime receipt plus running-child
  identity validation, and a receipt-bound resource tree. Repair1 implemented
  those boundaries: 10 focused Rust adapter tests passed; the build receipt
  bound the three-file runtime tree and resource tree `7a5cb869…`; Host prepared
  the child without sending RPC bytes, the App validated its exact running Mach
  identity, and only a second proof-bearing RPC activated the watch. Its
  replacement reviewers then rejected frozen fingerprint `10160bb1…` because
  Host handed the already-prefixed approved wire body to an adapter that
  requires unprefixed content and the patched send result did not return a real
  provider identity. Repair2 preserved the audited final wire body and added a
  bounded read-only restart scan, but both replacement reviewers rejected
  frozen fingerprint `1a983c72…`: a prior Mission's same-text GUID could be
  misbound to the current outbound, prepare response loss could wedge retry,
  and the UI still required database IDs instead of an explicit conversation
  selection route. Repair3 never promotes any history scan to `Sent`; only the
  exact synchronous send RPC may return a provider GUID. The patched sender
  sends once, accumulates candidates for the complete two-second window, and
  keeps any multi-candidate observation permanently ambiguous. Conversation
  discovery is a separate prepare/validate/list flow: prepare sends no RPC
  bytes before exact running Mach identity validation, list consumes a second
  fresh proof, returns only bounded exact `service == "iMessage"` chats, and
  always shuts down its child. Global Off, explicit stop, activation failure,
  and prepare response loss also clear the prepared child. Swift exposes those
  chats and participants for explicit selection instead of free-form database
  identity entry.
- Repair3's fresh pinned build passes five upstream OpenOpen RPC tests. Its
  unsigned binary SHA-256 is
  `635c99814fc3dbefffacaeb5222d4bf2ed340d019e726751ead909addc9122a1`,
  build receipt SHA-256 is
  `c1769b4093faa6e8bde56cdb16ad2c950ee39ea5501630e0ba022901b56a7b3d`,
  and resource tree remains `7a5cb869…`. The complete local tree passes 190
  ordinary Rust tests with one explicit environment-gated Codex test, 40
  broker/signing plus 53 App Swift tests, release/strict lint/format checks,
  two explicit pinned Codex diagnostics, and an independent 597-text notice
  closure check. Two fresh replacement reviewers PASS fingerprint
  `3e201547…`; implementation commit `2685b57…` is pushed, and PR #2 Actions
  run `29440208503` passes on equal-tree synthesized merge `99ee2b10…`.
  Evidence follow-up `becea456…` passes run `29442001103`. The subsequent
  local signing slice gives the pinned sender only
  `com.apple.security.automation.apple-events`, signs it and every OpenOpen
  executable with hardened runtime, a secure timestamp, and Team
  `UHDY2275L5`, and records that Team/CDHash in its runtime receipt. It first
  verifies the upstream Codex `rg` SHA-256 `4fdf1d83…`; because that upstream
  nested executable is linker-ad-hoc, Developer-ID mode re-signs the same
  Mach-O with OpenOpen Team/hardened runtime/timestamp and records upstream and
  runtime hashes plus Team/CDHash in `CODEX-RUNTIME-RECEIPT.json`. The signed
  local v3 app and DMG passed every-Mach-O structural verification, but the
  first signing-slice governance reviewer rejected frozen fingerprint
  `eaa4bc2e…`: staging still trusted a caller-supplied imsg receipt, the DMG
  creator accepted a same-Team renamed app without exact nested-code and
  receipt binding, and the README omitted the mandatory pinned inputs. v3 is
  therefore historical and not an alpha candidate. The first repair pins the exact
  schema-v2 build receipt, unsigned imsg bytes/size, patch, runtime/resource
  trees, compiled-source manifests, and the three-file runtime allowlist before
  any build or signature. DMG creation additionally requires the exact
  OpenOpen bundle/signing identifiers, eight-Mach-O allowlist, Apple anchors,
  Teams, hardened runtime/timestamps, entitlement split, runtime receipts,
  upstream hashes, and frozen CDHashes. Local v4 satisfied that focused contract,
  but both fresh v4 reviewers rejected unchanged fingerprint `08a58745…`: the
  App verifier omitted modes, the DMG accepted any generic same-Team Developer
  ID leaf rather than the exact owner certificate, and staging lacked complete
  post-sign/final-copy unsigned-content checks. v4 and DMG SHA `feec94d3…` are
  historical, not signing PASS evidence. The replacement repair pins owner leaf
  SHA-256 `a7e43925…`, binds the exact directory/file/type/mode allowlists, and
  checks every owned Mach-O before and after signing and at final output. A
  process already authorized to use the owner's signing identity can call
  `codesign` directly; output verification does not claim otherwise. Fresh
  Focused replacement verification now passes exact owner-leaf checks on every
  owned component and the DMG; all 18 directories are `0755`, the eight Mach-O
  files are `0755`, and every other regular file is `0644`. Main-executable,
  resource-file, directory-mode, extra-directory, and wrong-identity negatives
  all fail before output, while the ad-hoc diagnostic route still stages
  successfully. The provenance-embedded replacement candidate satisfies this
  focused contract. Fresh functional and governance reviewers both PASS frozen
  fingerprint `fdf5a00e8c0c4ca92ab4ff8e9cf33041c801bac5e74304bc484c613c35e33235`
  with zero P0/P1/P2 after independently checking the original, mounted, and
  copied App plus the focused negative routes. That first closure was
  historical; the provenance-bound Final2 replacement later received two
  entirely fresh PASS reports on unchanged fingerprint `026b2b1f…`, was
  committed and pushed as `5a461ef…`, and passed PR #2 Actions run
  `29450863581` on equal-tree synthesized merge `da3d7d1…`. Reviewer results
  remain task/PR evidence and are not recursively embedded into the reviewed
  package. These focused packaging facts are not a substitute for the recorded
  full suite. Full Disk Access, Messages Automation, real bidirectional
  traffic, notarization, and release proof remain pending.

### serenity (implemented exact direct dependency; provider proof pending)

- Upstream: `https://github.com/serenity-rs/serenity`
- Version/tag: `v0.12.5`
- Exact commit: `1809beb0fc24f3942c500058ad4fa47e6a97d3f9`
- License: ISC, copyright 2016 Serenity Contributors
- Allowed surface: official Discord Bot Gateway/HTTP only, with token stored in
  Keychain and least-privilege intents/permissions.
- OpenOpen path: `crates/openopen-discord-adapter/Cargo.toml`; `Cargo.lock`
  records the full exact Git source ending in
  `#1809beb0fc24f3942c500058ad4fa47e6a97d3f9`. The Swift app sends the token
  once from a device-only Keychain item; the adapter probes the official bot
  identity and does not accept user tokens.

Codex app-server schemas are generated from pinned runtime `0.144.0`; the
tracked manifest binds the runtime package components and all 267 schemas. The
runtime binary itself is not stored in Git. Local staging accepts only an
explicit package root whose four component hashes match the manifest. This is
implementation provenance, not redistribution or provider proof. The current
local Developer-ID candidate verifies all four immutable upstream Codex file
hashes before signing. It preserves the OpenAI-signed Codex and code-mode-host
bytes/signatures; only the same upstream linker-ad-hoc `rg` Mach-O is re-signed
for notarization, with a separate content/signature receipt. The candidate is
not notarized, installed with administrator approval, or release proof. Review
status is recorded in external task/PR evidence and is not recursively embedded
into a reviewed package.

`imsg` and serenity are present through the exact adaptation and direct
dependency paths above. imsg's three Swift dependencies are locked by the
tracked `Package.resolved`; serenity is locked by `Cargo.lock`. The current
Friday-alpha distribution notice closure contains 190 OpenOpen and 924 Codex
third-party package identities, 1888 document references, and 597 unique
content-addressed texts; manifest SHA-256 is `818495226dda3332…`. Hero C's
future exact `rust_xlsxwriter 0.96.0` is not distributed and must extend this
closure, provenance map, and notices when implemented. Neither the ad-hoc
history nor the current local Developer-ID
candidate is real provider, notarization, clean-install, or release proof.
