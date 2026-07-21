# OpenOpen Choice Loop product validation

Only consent-safe aggregates belong in new Choice Loop validation records. Raw
messages, Markdown task bodies, Receipts, imports, names, emails, phone numbers,
Discord/iMessage identities, tokens, model prompts, and recordings must remain
local and must not be newly committed. Historical evidence elsewhere in the
repository may contain quarantined legacy provider identifiers; it is
non-normative and not a precedent. Every newly published participant aggregate
requires explicit consent.

The current product is designed for non-developers. Validation asks whether
OpenOpen reduces learning and decision burden while preserving understanding
and control. It does not measure engagement time as success.

## Current validation questions

- Can a person reach a useful first direction without connecting a channel,
  learning prompting, or understanding model jargon?
- Does English-only first launch clearly scan the account, require explicit
  model and supported-effort selection, ask one simple question, and only then
  generate dynamic A/B/C plus D, with zero model work before selection?
- Does Host-owned `choice.begin` create exactly one initial interpreting
  session/audit transaction for an exact request replay, reject changed replay
  and stale/late results, and expose no public raw snapshot writer?
- Is the first-question body Store-private and Keychain-master-key encrypted
  with request/session/envelope/batch AAD; retained across restart only while
  recoverable; deleted with every raw/derived representation after cancel or
  accepted typed-state render receipt; reduced only to accepted typed state and
  body-free request-digest/audit tombstones; and excluded from logs, evidence,
  Receipts, and remote with bounded transient buffers zeroized where supported?
- Does a model with no effort control display an English `Not applicable` state
  without being hidden, defaulted, or substituted?
- Do three dynamic choices help the person see important next directions they
  would not have known to ask for, while D still feels like ordinary
  conversation?
- Do choices grounded in prior task Markdown correctly surface changed
  information, a prepared next step, or a safe alternative without becoming
  fixed categories?
- Does the user understand that choosing A/B/C/D refines intent and does not
  authorize work?
- Does every A/B/C/D selection atomically persist Selection, create its pending
  refinement operation, advance the exact session revision/state, and append
  audit in one SQLite `IMMEDIATE` transaction, with crash injection proving no
  committed Selection can exist without its operation?
- Does D submit bounded text plus an idempotent request ID only, while Host
  derives/seals the authenticated batch and rejects changed/stale replay?
- Does explicit D intake atomically persist encrypted body/envelope/batch/
  Selection/operation/session/request-digest registry/audit, and does quiet-
  window crash recovery avoid orphan seal, partial Selection, or unintended
  plaintext retention? Does cancel or accepted typed-state render receipt
  delete the raw encrypted body while retaining only body-free digest/audit
  tombstones?
- Can only the private Selection/revision/generation/provenance/manifest-bound
  refinement commit accept the next model result, with exact operation and
  audit binding? Does one SQLite `IMMEDIATE` transaction complete the operation,
  persist result digest plus encrypted frame/set, advance session state/revision,
  and append audit with no intermediate model/UI-visible result?
- Can the user review and edit the one consolidated confirmation, including
  exact steps, Reminder contents/times/count, Markdown changes, model
  provenance, recipients, data, and effects?
- When explicit temporal information is absent, does Host require the user to
  choose a Reminder time instead of filling a fixed or question-time default?
  Does it validate a future instant, bind exact date/time/timezone/list/count,
  reconfirm every edit, and keep real write separately gated?
- Does the model picker show only the live compatible account catalog and make
  explicit selection understandable without suggesting an unlimited-free or
  guaranteed-model promise?
- After restart, 30 minutes, or 24 hours, does the recap preserve continuity
  without executing stale choices or offline messages?
- Are idle/stale transitions Host-owned persisted-deadline Store commands whose
  timer hints cannot independently start model or effect work, whose time/state
  is Host-derived, and whose sleep/reboot/backward/ambiguous clock cases block
  safely?
- Does Markdown crash recovery prove intent → staged-file sync → no-clobber
  creation or atomic swap/CAS retaining the displaced base → parent-directory
  sync → final and displaced-base digest/inode verification → receipt, with
  every ambiguous state blocked for typed reconciliation?
- Can a concurrent Owner edit at every pre/post-swap point never be overwritten,
  with no-clobber creation, displaced-base CAS validation, and both versions
  preserved whenever safe swap-back is impossible?
- Are Off, cancel, permission denial, model loss, document conflict, channel
  loss, and recovery understandable and reachable without modal loops, focus
  stealing, repeated alerts, false Done/On, or retry?
- Does the bounded task package let a later compatible selected model continue
  accurately without hidden memory or filesystem roaming?
- Do Reminders Evidence and the Receipt make completion trustworthy rather than
  merely claimed?

## Channel validation

Mac validation covers first-run value, model selection, neutral Choice cards,
D conversation, confirmation, activity, recovery, Settings, and Global Off.
Every product-owned visible string must be English-only; second-language UI
copy is a failure, while user-authored/imported content retains its source
language.

iMessage B+ Hero validation covers the same-account self-chat identity, OpenOpen echo
visibility, loop/dedupe/restart behavior, and clear group rejection. The post-
B+ source permits exactly one individually selected/revocable one-to-one
read-only binding, rejects a second source and every outbound/recipient route,
and never mirrors a reply there. Permission deny/cancel/revoke/regrant/restart
must avoid repeat modals and false On, keep Off reachable, and permit zero
provider/model/effect work until fresh validation.

Post-B+ Discord validation covers guided personal Bot creation, authenticated owner plus
expected Bot/application/exact-DM identity, Keychain-at-rest token entry,
transient-memory redaction, token remove/rotate, intent/identity drift, Off,
offline behavior, and deterministic metadata-only pre-consent recap. Unrelated
events must be rejected before body persistence/model access. Only explicit
English `Continue` admits bounded owner-bound bodies; no shared/cloud Bot exists.

Cross-channel validation proves that a reactive reply uses only the most
recently accepted owner-active connected channel, Mac mirrors local state
without copying chat output, and there is no broadcast. Proactive delivery, a
new recipient, or cross-channel delivery must require exact confirmation.

## Privacy and proof handling

Validation records non-body result classes and aggregate task timings only.
Credential-like material, raw prompt injection, private messages, and matched
secret values never enter committed logs, fixtures, screenshots, or model-
visible previews. Failure is reported as failure and fixed only within the
approved scope; results are never rounded, rewritten, or replaced by CI.

The Owner-supplied real ChatGPT export may be used only in place for an
isolated local, read-only, no-network, no-retention B2 diagnostic. Its path,
hash, content, member metadata, counts, catalog, extracted files, temporary
files, and derived data must not enter repository files, evidence, logs, or a
remote. Committed compatibility fixtures must be newly generated synthetic
data; only redacted PASS/FAIL and bounded failure classes may be reported.

B2 B+ validation begins only after the Hero checkpoint. It verifies exactly
one real import, at most three dynamic Memory candidate cards without fixed
categories, revisioned preview/selection/disposal, zero provider request before
exact Owner consent, one Owner-selected-card semantic Markdown diff, and no
unselected raw or derived retention. Only its exact confirmed diff persists.
The local diagnostic never satisfies this product route.

C2 B+ validation uses synthetic fixtures during autonomous work and verifies
canonical immutable GitHub identity, structural/license/permission rejection,
Candidate → Staged → Promoted → Runnable, exact update/rollback, and no script
execution. B+ admits exactly one public instruction-only Skill and one
separately confirmed no-external-effect use. Real selection, acquisition,
stage/promotion/enablement, and first use remain separate Owner actions.

Formal external-user, clean-install, public-release, model-assisted real ZIP
product validation, and real Skill validation remain separate future or
action-time work unless the Owner explicitly authorizes the exact node. The
bounded B2 diagnostic above does not satisfy real ZIP product validation.
