# Broker trust contract

The authenticated XPC `session` result carries the broker's persistent Ed25519
public key and its SHA-256 key ID. Core may use that session to issue permits or
verify Receipts only after `BrokerSessionTrustValidator` matches both values to
an existing `EnrolledBrokerTrustAnchor`.

`KeychainBrokerTrustAnchorStore` is the production provider.
`BrokerEnrollmentCoordinator.provisionAfterAdminApproval` runs only after
`SMAppService` reports the daemon enabled, activates an XPC connection pinned
to the exact daemon identifier and Team ID, enrolls the durable Keychain-derived
Core effect key in the root worker, and only then persists the broker anchor.
The host also emits a Core-signed installation record; Rust Core verifies that
signature before constructing its opaque `TrustedBrokerEnrollment`. Exact
retries are idempotent and changed keys or requirements fail as rotation. A
normal request/model/RPC path cannot create trust from a live session. Missing
enrollment, mismatch, or rotation fails closed; there is no unsigned fallback.

The broker signing seed must persist across daemon sessions in root-only
storage, preferably a System Keychain item accessible only to the signed broker
or a mode `0600` file in a root-owned mode `0700` broker directory. It must not
be generated per XPC session, shipped in the app bundle, or accepted from a
caller. Rotation is an explicit, authorized operation that atomically enrolls
the replacement public key in Core before the prior anchor is retired.

The final host integration must pass only a validated session into Core's
permit and Receipt APIs. Core must independently require the enrolled key; a
self-consistent session key ID and public key are not sufficient trust proof.
Each Receipt binds the SHA-256 hash of the complete signed permit, not only the
stable command hash. A committed retry must consume and hash the supplied
payload before returning an existing or freshly attested Receipt. Permit and
daemon-session expiry are rechecked immediately before rename and again, using
a fresh clock reading, after payload/output validation before a Receipt is
returned. Permits bind either `execute` or `reattestOnly`. If a valid Store
event advances the audit, Core may issue only `reattestOnly`; the broker must
find the exact existing journal, pinned workspace, and committed output and
must not create a workspace, stage, or file. Core records that result only when
its broker-signed commit time is strictly earlier than the Store-observed time
bound into the first intervening signed audit row. A nonempty older ledger
without observation times fails closed; ordering proof is never synthesized.
