# Third-party notices

This file records direct dependencies or substantially adapted implementation
portions. Transitive dependency notices will be generated from lockfiles before
release.

## Friday

- Upstream: https://github.com/thesongzhu/Friday
- Source commit: `4870f31fa088bef7eb9f4f256ec62993b02eda80`
- License: MIT
- Copyright: Copyright (c) 2026 Friday contributors

The Friday MIT license is reproduced below.

```text
MIT License

Copyright (c) 2026 Friday contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Current direct Rust dependencies

Versions below are the versions resolved by `Cargo.lock` on 2026-07-14. This
is a direct-dependency inventory, not the final transitive distribution report.

| Package | Version | License |
| --- | --- | --- |
| aes-gcm | 0.10.3 | Apache-2.0 OR MIT |
| ed25519-dalek | 2.2.0 | BSD-3-Clause |
| getrandom | 0.3.4 | MIT OR Apache-2.0 |
| hex | 0.4.3 | MIT OR Apache-2.0 |
| rusqlite | 0.38.0 | MIT |
| rustix | 1.1.4 | Apache-2.0 OR MIT |
| serde | 1.0.228 | MIT OR Apache-2.0 |
| serde_json | 1.0.150 | MIT OR Apache-2.0 |
| sha2 | 0.10.9 | MIT OR Apache-2.0 |
| thiserror | 2.0.18 | MIT OR Apache-2.0 |
| tempfile (development only) | 3.27.0 | MIT OR Apache-2.0 |
| zeroize | 1.9.0 | Apache-2.0 OR MIT |

## Pinned Codex component under implementation

- Upstream: https://github.com/openai/codex
- Runtime version: `0.144.0`
- Source commit recorded by the package: `767822446c7a594caa19609ca435281a9ec67e0d`
- License: Apache-2.0

Generated app-server schemas and a component-hash manifest are tracked. The
runtime binary is not stored in Git; the local staging script accepts only an
explicit package root matching all pinned hashes. The Apache-2.0 license text
and complete runtime/transitive notice set remain required before signed
distribution.

## Planned dependencies not yet present

The following components are fixed by the product plan but are not currently
in the source tree or distribution. They must move into the current inventory,
with locked artifacts and full notices, when implemented.

### imsg

- Upstream: https://github.com/openclaw/imsg
- Version: v0.13.0
- License: MIT
- Copyright: Copyright (c) 2026 Peter Steinberger

The release package must reproduce the imsg MIT license. OpenOpen will bundle
only the basic CLI and required resources; the private bridge helper is
excluded.

### Rust crates

- `serenity` 0.12.5
- `rust_xlsxwriter` 0.96.0

Their license texts and the complete transitive inventory must be generated and
verified before distribution.
