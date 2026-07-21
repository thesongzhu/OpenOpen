# OpenOpen Default Persona — draft-03-en

Status: implemented, verified locally, and not activated for real dogfood.

This document records the production contract for
`openopen.nondev.default / draft-03-en`. It is an English-only language and
interaction-shape layer for non-developers. It cannot select models, define
tools, grant permissions, choose recipients, write Memory, authorize effects,
or change retention.

## Locked behavior

- Greeting: `Hi — what are you working through?`
- Warm and companion-like without attachment, exclusivity, guilt, dependency,
  fabricated experience, or claims of human feeling.
- Answer first; normally one to five sentences; at most one focused question.
- Recommend clearly and disagree gently when a material reason exists.
- Use rare dry humor only after the useful answer and never in sensitive,
  high-stakes, permission, confirmation, privacy, failure, or security contexts.
- Use A/B/C plus a free-form option only when different outcomes materially
  change the result.
- Treat inferred style changes as session-local until the user confirms a
  durable preference.
- Keep proactive contact grounded in a selected reminder, task change,
  required decision, failure, or completion.
- Never infer a missing Reminder time or turn a conversational confirmation
  into authority for a real effect.

The complete behavior matrix is stored in `scenarios.json`; deterministic and
adversarial cases are stored in `evals.jsonl`.

## Bundle boundary

The app embeds exactly five non-executable files:

1. `manifest.json`
2. `persona.json`
3. `messages.en.json`
4. `scenarios.json`
5. `evals.jsonl`

The manifest is canonical JSON and binds the other four files by SHA-256. A
future reviewed local update must satisfy all of the following technical
validation rules before it can even be proposed for activation:

- the directory and every allowlisted file are regular, non-symlink, and
  non-executable;
- no unknown file is present beyond the bounded code-signing metadata;
- the bundle is within per-file and aggregate size limits;
- schema, locale, Host compatibility, content digests, persona ID, and revision
  ordering are valid;
- macOS verifies a Developer ID Application signature from the exact approved
  Team ID and the signer identity matches the manifest;
- test-only lifecycle validation materializes an immutable private verified
  copy before returning a human-readable diff.

The PR1 production binary does not compile a mutable Persona lifecycle: it
opens only the embedded app-sealed default and does not read, write, stage,
activate, or roll back a local Persona registry. An invalid candidate therefore
cannot replace the running revision. The update-validation code and its tests
remain quarantined as a future reuse boundary, not a current product route.

## PR1 default and provenance

PR1 exposes only typed read-only status for the embedded, reviewed default
revision. It deliberately exposes no public Persona staging, activation, or
rollback RPC: a new conversation-style revision is a separate reviewed
Owner action-time design, not an incidental local control or a bearer nonce.
The bundle loader and its update-validation tests remain in the technical
migration so a later reviewed lifecycle can reuse the same bounded, signed
format without weakening the default path.

Every newly accepted Choice turn snapshots a `PersonaRevisionRef`. The same
reference is copied into the initial result, refinement operation, refinement
result, ChoiceSet, and Host-owned model request. The model receives the verified
bundle's developer instructions separately from untrusted user text. A later
persona activation cannot reinterpret a pending confirmation, retry an effect,
rewrite Memory, or change an in-flight request.

The verified renderer is a deterministic local formatting contract. In PR1 the
complete bundle compiler supplies the model-facing instructions for initial and
refinement requests, and every request carries the exact revision reference.
Mac and iMessage presentation wiring is not claimed by this document: the
same-account iMessage surface belongs to PR2 and must consume this exact bundle
and reference through its own reviewed integration.

## Evaluation gate

The embedded revision contains 50 canonical situations, four expression
variants for each, 12 multi-turn suites, and 8 adversarial suites: 220 cases in
total. Deterministic tests cover bundle validation, routing, source-bound
instruction compilation, request provenance, signature/layout rejection, and
the quarantined future lifecycle-validation boundary. They do not prove a real
model response, a Mac presentation, an iMessage reply, or a real activation.

A separate pre-activation human-style stress set contains 50 distinct
non-developer daily-life situations across 46 expression styles and 31 domains.
It is kept outside the signed bundle allowlist under `evidence/validation/` and
is validated by `scripts/validate_persona_human_stress.py`. The first review
found and repaired an overbroad high-stakes clarification route; the corrected
set passes every deterministic assertion.

Before a future mutable activation, the exact bundle digest still requires:

1. the four Owner dogfood transcript sets in a local conversation harness;
2. isolated product and safety reviews against that same digest;
3. explicit activation of only the reviewed digest;
4. the same four transcripts through the real approved iMessage self-chat;
5. redacted behavior findings only.

Any content change creates `draft-04-en` or later and requires its own
reviewed Owner action-time lifecycle. The approved revision is never edited in
place.

## Frozen UI boundary

The selected V3 visual artifact is unchanged by this implementation. The bundle
centralizes conversation strings and typed behavior only. No UI-visible copy may
ship until a separately named UI revision receives direct Owner approval.
