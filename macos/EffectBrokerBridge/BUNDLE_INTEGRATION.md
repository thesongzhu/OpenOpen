# Effect broker bundle integration

The production app bundle contains exactly these broker artifacts:

```text
OpenOpen.app/Contents/
├── Library/LaunchDaemons/com.thesongzhu.OpenOpen.EffectBroker.plist
└── MacOS/
    ├── OpenOpenEffectBroker
    └── OpenOpenEffectBrokerWorker
```

`scripts/stage_effect_broker_components.sh` builds and copies those artifacts
into an existing app bundle. Its output is deliberately unsigned and is not
installation, cross-UID, notarization, or release proof.

Release packaging signs the worker first with identifier
`com.thesongzhu.OpenOpen.EffectBroker.Worker`, signs the daemon with identifier
`com.thesongzhu.OpenOpen.EffectBroker`, and then signs the containing app with
identifier `com.thesongzhu.OpenOpen`, all under one Team ID. The daemon refuses
to start outside root, verifies its own identifier and Team, copies the already
signed worker into a root-owned mode-`0700` directory, re-verifies the copied
Mach-O with strict/all-architectures Security.framework flags, and executes
only that mode-`0500` copy. The app never receives a raw filesystem executor.

The worker process pipe is private to the signed root daemon. It is not an app
or caller transport, socket, shell, or unsigned fallback. The only user-session
entry point remains the privileged Mach service with mutual code-identity
requirements. Worker requests are canonical typed JSON plus an inherited
payload descriptor; the worker reparses and revalidates them before the Rust
broker can act.

`SMAppService` registration, admin approval, persistent Keychain enrollment,
real different-UID denial, signing, notarization, and Gatekeeper evidence must
all be captured from the same signed build before this boundary can be called
release-proven.
