# OpenOpen

OpenOpen is a local-first outcome distribution layer for people who know AI
exists but do not yet know what to ask it to do. It turns an explicit voice,
message, or receipt into a bounded Mission, trackable steps, a `Need you`
boundary, and an evidence-backed Receipt in apps the user already knows.

The Build Week product targets macOS 14+ on Apple Silicon and uses the user's
own ChatGPT plan through an embedded Codex App Server. OpenOpen has no cloud
service and no central telemetry.

## Current status

Implementation is in progress. A green unit test or mock is not release proof.
The product must not be described as `PRODUCT_READY_FOR_DEMO` until every gate
in [the master plan](docs/OPENOPEN_BUILD_WEEK_MASTER_PLAN.md) passes on the same
signed build and commit.

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

The Developer-ID output remains `NOT_NOTARIZED_NOT_RELEASE_PROOF`. Apple
notarization credentials, submission, stapling, Gatekeeper acceptance, and
administrator/cross-UID installation are separate required gates.

No credentials belong in this repository. ChatGPT and Discord credentials are
stored at runtime in the macOS Keychain.

## Scope

Build Week v1 contains three complete product routes:

1. Voice to actions in Reminders and the selected chat.
2. Availability collection across one approved iMessage conversation and one
   approved Discord channel, with at most one follow-up and owner confirmation.
3. Receipt images from chat to a locally generated XLSX and a verifiable
   Receipt.

Telegram, an OpenOpen cloud, ambient surveillance, self-bots, private iMessage
APIs, payments, and silent model or Skill upgrades are intentionally excluded.

## Provenance

OpenOpen builds on selected MIT-licensed state-machine and safety ideas from
Friday. See [BUILD_WEEK.md](BUILD_WEEK.md),
[PROVENANCE.md](PROVENANCE.md), and
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
