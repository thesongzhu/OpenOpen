# OpenOpen draft-03-en vs OpenClaw v2026.7.1

Status: source-contract comparison complete; live same-model A/B not run.

## Fair baseline

The OpenClaw baseline is the official `openclaw/openclaw` tag `v2026.7.1`,
commit `2d2ddc43d0dcf71f31283d780f9fe9ff4cc04fe4`. The comparison uses nine
official sources:

- `docs/reference/templates/SOUL.md`, SHA-256
  `79c61aaee618c787c164c5d767053a83c6e218191b3b6ebb66fd07320b071ce8`;
- `docs/reference/AGENTS.default.md`, SHA-256
  `645342f8c6e2805135817cf4bbc2c8bd1d57066054ed671eda93876b2762ffb1`;
- `docs/concepts/system-prompt.md`, SHA-256
  `1aabd41b5d4b51ed139d47b506017322c240bb1002bae901886d5f7991c0dc5e`;
- `docs/concepts/personal-agent-benchmark-pack.md`, SHA-256
  `35da45e4b22b1044a777fa8d6bce87f9ace377950dd0af3f2419b40cfe4d9be6`;
- `docs/reference/templates/USER.md`, SHA-256
  `599bd4d663c852bca679a341d53605c1a48b7cd7601bd7d102ee5407828dbacb`;
- `docs/concepts/qa-e2e-automation.md`, SHA-256
  `602dcdd6743d63a4e11c40096165b95aa8c96aeee0c526cff0af365c717c2076`;
- `extensions/qa-lab/src/character-eval.ts`, SHA-256
  `ed6c6d72d5e3fd0d1f240a7acebf5929af7f98bfee7fa82a1de181988e6a7dc6`;
- `qa/scenarios/character/character-vibes-gollum.yaml`, SHA-256
  `cd4dfd9e2f0830e21bb9c8ffbb5e7439ead15e76983035105a2538e70020507d`;
- `qa/scenarios/character/character-vibes-c3po.yaml`, SHA-256
  `74864f2d3152181112e9f6f2b7bda7adecbd2ff80994223b1314d9fc8cc36368`.

OpenClaw has no single runtime default persona. It composes a system prompt
from runtime facts and injects workspace files including `AGENTS.md`,
`SOUL.md`, `USER.md`, and Memory. The official `SOUL.md` template is therefore
the closest reproducible default-persona baseline, not a guarantee of exact
model output.

The OpenOpen baseline is `openopen.nondev.default / draft-03-en`; its manifest
SHA-256 is
`ad187f5df6ec872c7df84424b7db60c8cc0a7885c9efeea042adf3a38183a702`.
The canonical 50 daily-life inputs are unchanged from
`persona-draft-03-human-stress-50.jsonl`.

## Result across the same 50 situations

| OpenClaw official-default coverage | Cases | Meaning |
|---|---:|---|
| Explicit | 6 | An official default or personal-pack contract directly covers the essential behavior. |
| Partial | 20 | The mechanism points in the same direction, but the user-facing rule or safety boundary is incomplete. |
| Unspecified | 23 | Behavior is left to the selected model or a user-authored workspace persona. |
| Different default | 1 | OpenClaw recommends capturing preferences in Memory; OpenOpen requires explicit confirmation before inferred style becomes durable. |

This is not a 50–6 quality score. It measures how much behavior is fixed by the
official default contract. A strong model may produce excellent OpenClaw
answers in unspecified cases, and a weak or mismatched model may fail either
product's language-level expectations.

## What OpenClaw does better today

1. **Live character-eval infrastructure.** Its QA lab can run the same
   multi-turn scenario across a model panel and obtain judged reports.
2. **Fast personality iteration.** `SOUL.md` is short, editable, opinionated,
   and easy for a technical owner to tune.
3. **Broad operational surface.** Its default workspace and personal-agent pack
   cover channel routing, local tools, Memory, progress honesty, redaction,
   approval denial, and failure recovery.
4. **Naturalness philosophy.** The official guidance correctly pushes against
   filler, hedging, corporate prose, and unnecessary questions.

## What OpenOpen fixes more deeply for this product

1. **Non-developer interaction is product behavior, not a user-maintained
   prompt file.** Greeting, response density, repair, overwhelm, high-stakes
   uncertainty, topic switching, returns, and structured choices are explicit.
2. **Conversation cannot grant effect authority.** Editable previews,
   confirmation revisions, exact Reminder fields, Receipts, duplicate
   suppression, Off, and restart semantics are Host-owned.
3. **Memory inference is not silent.** Session adaptation and durable Memory
   are deliberately separate.
4. **App and iMessage share one pinned Persona revision.** OpenClaw is more
   customizable; OpenOpen is designed to be more reproducible for a default
   consumer product.
5. **Attachment boundaries are explicit.** Warmth is allowed without
   exclusivity, dependency, fabricated feelings, or relationship-maintenance
   messages.

## Important weaknesses found in OpenOpen's current proof

- The 50 OpenOpen replies are curated golden responses, not 50 independently
  sampled live model outputs.
- No human participant panel has graded either product.
- OpenClaw's official character scenarios are Gollum and C-3PO coding chats,
  while its ten-scenario personal pack is mostly deterministic workflow QA.
  Neither is a substitute for this ordinary-life non-developer corpus.
- The local OpenClaw state was last touched by version `2026.2.15`; its CLI
  symlink points to a missing development checkout, no App or Gateway process
  is present, and no live run was attempted.

## Required live A/B before claiming a winner

Run all 50 inputs through both products using the same provider, model,
thinking level, locale, empty user Memory, and fresh session per case. Blind the
product labels and collect three independent runs per input. Score:

1. useful answer or next step;
2. unnecessary question count;
3. naturalness for a non-developer;
4. cognitive load;
5. inference errors;
6. technical leakage;
7. action or Memory overreach;
8. response consistency across runs.

That run requires a repaired or isolated OpenClaw installation plus explicit
provider/model-cost authorization. Installing dependencies, using stored
credentials, or sending model traffic was intentionally not inferred from this
source-level comparison request.

## Decision

Do not copy OpenClaw's default persona wholesale. Preserve OpenOpen's typed
non-developer and action-boundary contracts, but adopt OpenClaw's stronger eval
method next: raw multi-turn transcripts, same-model candidate runs, blind
labels, multiple judges, and repeat-run variance. No PersonaBundle or UI change
is justified until that live A/B exists.
