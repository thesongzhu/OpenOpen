# Persona draft-03-en human-style conversation stress report

Date: 2026-07-20

Result: **PASS after one routing correction**

This is a curated human-style transcript stress test. It is not evidence of a
live ChatGPT model run, a real iMessage exchange, or independent human-subject
research. It tests the approved persona contract against realistic non-developer
language before those separately gated live steps.

## Coverage

- 50 distinct daily-life situations
- 46 communication styles
- 31 domains
- 43 Mac conversations and 7 iMessage self-chat conversations
- all eight typed decisions represented
- ordinary, vague, fragmented, typo-heavy, rambling, emotional, angry,
  sarcastic, demanding, anxious, deferential, expert, nontechnical,
  attachment-seeking, correction, rapid-input, and limited-English expression
- reminders, external effects, permission, credentials, health, legal, finance,
  family, caregiving, continuity, cancellation, duplicate submission, partial
  failure, restart, Off, Memory correction, and iMessage isolation boundaries

The exact transcripts and expected replies are in
`persona-draft-03-human-stress-50.jsonl`. Each case includes the user language,
surface, Host decision, exact expected reply, and case-specific hard assertions.

## Automated assertions

The validator requires:

- exactly 50 unique cases and situations;
- at least 20 expression styles and 15 domains;
- both Mac and iMessage coverage;
- no more than one question per response;
- no canned praise, generic closing question, attachment claim, technical
  leakage, or AI disclaimer;
- the `OpenOpen · AI` identity on every iMessage output and nowhere else;
- no effect claim in preview/failure cases;
- A/B/C plus free-form D for every Choice route;
- representation of every typed conversation decision.

Validator output:

```json
{"cases":50,"decisions":{"choice":3,"clarify":1,"direct":26,"editablePreview":4,"needUser":4,"progress":2,"receipt":3,"safeFailure":7},"domains":31,"status":"PASS","styles":46}
```

## Human review finding and repair

The first pass exposed one material defect: the deterministic router treated
every high-stakes topic as a clarification. That would make a clear urgent
health request or court-deadline warning hesitate instead of leading with the
safe judgment.

The repair separates risk from ambiguity:

- a clear high-stakes request answers directly with visible uncertainty and
  qualified-help guidance;
- a missing value that can materially change the answer uses one focused
  clarification;
- an irreversible or external effect still produces an editable preview and
  retains separate action-time confirmation.

The health, legal, and financial transcripts were rerun under the corrected
route. The second pass is clean.

The credential transcript was also tightened: OpenOpen says it will not use or
repeat a posted secret, rather than making an unverifiable claim that the chat
system does not store it.

## Reproduction

Run:

```sh
python3 scripts/validate_persona_human_stress.py
cargo test -p openopen-persona
cargo clippy -p openopen-persona --all-targets -- -D warnings
```

Corpus SHA-256:
`411378781e4611a117d9ba8bd01bc845694334c9dd1a4309306fa08b63d31910`

## Remaining live gates

This pass does not replace:

1. three fixed-model style-evaluation runs against the same bundle digest;
2. the Owner's four dogfood transcript sets in the actual app;
3. isolated product and safety reviews;
4. explicit activation of the reviewed digest;
5. repetition through the real approved same-account iMessage self-chat.
