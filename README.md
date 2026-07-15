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

Stage a local ad-hoc app only from an explicit pinned Codex package root:

```bash
scripts/stage_openopen_app.sh \
  --codex-package-root /absolute/path/to/codex-0.144.0-package \
  --output /absolute/new/path/OpenOpen.app
```

The script verifies every pinned component hash before and after copying,
includes the protected broker daemon/worker/LaunchDaemon plist, and exclusively
claims a new output directory instead of merging into an existing app.
`STAGED_AD_HOC_NOT_RELEASE_PROOF` is a local diagnostic only;
it is not Developer ID signing, notarization, clean-install, or release proof.

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
