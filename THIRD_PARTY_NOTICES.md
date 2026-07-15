# Third-Party Notices

This file describes the deterministic notice payload for the current OpenOpen Friday-alpha distribution target. It is implementation and attribution evidence; it is not signing, notarization, provider, or release proof.

The content-addressed payload is in `third_party/notices/manifest.json` and `third_party/notices/texts/<sha256>.txt`. Regenerate or verify it with `scripts/generate_third_party_notices.sh`; verification is offline and rejects closure drift, empty source/license fields, missing texts, hash mismatches, and duplicate package identities.

## Generated closure

- Target: `aarch64-apple-darwin`; dependency kinds: normal and build (development-only dependencies excluded).
- OpenOpen roots: `openopen-host` and `openopen-effect-broker`; 190 transitive third-party Rust package identities.
- Codex roots: `codex-cli` and `codex-code-mode-host`; 924 transitive third-party Rust package identities.
- Notice documents: 1888 references resolving to 597 unique SHA-256-addressed text files.

## Exact runtime and source pins

- OpenAI Codex app-server `0.144.0`: official source commit `767822446c7a594caa19609ca435281a9ec67e0d`, Apache-2.0. The OpenOpen protocol manifest maps the distributed package hashes to this exact source commit; the upstream `codex-package.json` does not itself record the source commit. The payload includes the root Apache-2.0 license, root NOTICE (including Ratatui attribution), and the normal/build closure for `codex-cli` plus `codex-code-mode-host`.
- Bundled ripgrep `15.1.0`: source commit `af60c2de9d85e7f3d81c78601669468cf02dabab`, MIT OR Unlicense; `COPYING`, `LICENSE-MIT`, and `UNLICENSE` are included.
- imsg `0.13.0`: dereferenced commit `fa2f82d7dbda4c802d91c1d41bb6c53564ed2fdc`, MIT, copyright 2026 Peter Steinberger. Its exact Swift pins and notices are Commander 0.2.4 at `bd219c4ee9032fee3e009856f81fcc6ec09a85f4`, PhoneNumberKit 5.0.4 at `ab06a8333394f4a4fb6eecca447dae0aa06c1eca`, and SQLite.swift 0.16.0 at `964c300fb0736699ce945c9edb56ecd62eba27a3` (all MIT).
- serenity `0.12.5`: exact commit `1809beb0fc24f3942c500058ad4fa47e6a97d3f9`, ISC, present exactly once in the OpenOpen Rust closure.
- Friday contract source: immutable MIT commit `4870f31fa088bef7eb9f4f256ec62993b02eda80`; its license text is included. OpenOpen ports the contract/test semantics and does not distribute Friday's TypeScript/Node runtime.

## Planned but not distributed

`rust_xlsxwriter` for Hero C is planned after `FRIDAY_ALPHA_READY`; it is not part of this payload or current distribution closure. Its future notices must be generated from the then-locked closure before distribution.
