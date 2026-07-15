# External validation

Only consent-safe aggregates belong here. Raw participant messages, receipts,
memory imports, names, emails, phone numbers, Slack/Discord IDs, and recordings
remain local and are never committed. Every published row requires the user's
explicit consent for that anonymous aggregate.

Required gate: three unguided target users complete a first Outcome, and at
least two independently return within 48 hours. At least one run starts from a
clean macOS user installation.

For each user, record only anonymous timing and outcome fields: install type,
sign-in/permission duration, time to first real Outcome, completion or failure,
confusion category, whether `Need you` was understandable, whether the Receipt
contained real Evidence, and 48-hour voluntary reuse. The target is 90 seconds
from sign-in to first real Outcome in a preconfigured environment and five
minutes for a clean install through voice/text → Reminders, excluding external
OAuth/2FA or provider-export waiting time.

Quick Passport validation separately records whether source/destination,
retention, Claude-to-OpenAI disclosure, candidate review, correction, and raw
deletion were understood. Consented Slack/iMessage validation records only
consent completion, membership-change pause, revocation completion, preview
usefulness, and whether any unapproved action occurred; raw participant text is
never copied here.

Failure is reported as failure and fixed within the approved scope. The gate is
exactly 3/3 first-Outcome completion and at least 2/3 independent 48-hour reuse;
data is never rounded, rewritten, or replaced by the owner/test team.
