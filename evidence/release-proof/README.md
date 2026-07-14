# Release proof

Release proof artifacts are generated per commit and signed build. An artifact
is eligible only when its SHA matches the candidate, scenario count is nonzero,
every scenario passes, and `blockers` is empty. Private message bodies,
credentials, and user data must never be committed.
