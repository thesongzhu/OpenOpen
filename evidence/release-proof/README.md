# Release proof

Release proof is generated for one exact commit and one exact signed build. An
artifact is eligible only when its commit and package hashes match the
candidate, scenario count is nonzero, every scenario passes, and `blockers` is
empty. It records exact `gpt-5.6-sol` with `high` reasoning for every model
step. Private bodies, credentials, raw memory, receipts, and user data are never
committed.

## Required complete scenarios

- Hero A: explicit voice/text input → schema-constrained Outcome → owner scope
  confirmation → real Reminders write/readback → completion Evidence → Receipt.
- Hero B: real iMessage and Slack collection → structured replies → Rust
  intersection → at most one follow-up → owner decision → published result →
  Receipt.
- Hero C: real iMessage or Discord image → validation and extraction → one
  low-confidence review when needed → formula-correct XLSX → approved save and
  originating-channel readback → Receipt.
- Real bidirectional iMessage, Slack, and Discord provider IDs, with approved
  pairing/allowlists, durable dedupe, restart recovery, and no duplicate send.
- One reviewed Quick Memory Passport for every source provider publicly
  claimed, including the Claude-to-OpenAI disclosure path when Claude support
  is claimed, plus one exact one-use Private Memory grant.
- Global Off proving no new listener, model, Workflow, or outbound effect can
  begin; sleep/offline/crash recovery must remain bounded and honest.
- Developer-ID signing, notarization, staple, Gatekeeper acceptance,
  administrator/cross-UID helper proof, and a clean macOS 14+ Apple Silicon
  install of the same package.

## Ineligible substitutes

Mocks, fixtures, screenshots, CI success, component-only probes, schema tests,
ad-hoc packages, and signature inspection are supporting evidence only. They
cannot satisfy a real provider, user-experience, clean-install, or release
scenario.
