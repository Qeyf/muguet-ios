# Progress and blocking inputs

Updated 2026-10-11; each status describes evidence rather than intended behavior.

## Completed

- Cloned upstream with history/license/attribution; reviewed Rust, Wasmtime, wgpu, Metal-facing code, launcher, build script, lock and asset import.
- Refreshed main to `3c4ea719ac56049f92681f854783360133b55156`; the overnight upstream changes include Metal/shader/minimap, texture decoding, statfs, controller focus handling, background code loading and local ZIP discovery. Their source and updated Cargo.lock are retained. The earlier audit snapshot used `11a914d` and remains identified as a snapshot.
- Verified official iOS 27.2 SDK / Xcode 27.2 beta 2 availability and GitHub's standard `xcode-27` arm64 image.
- Investigated supplied game sources and public metadata without downloading unverified commercial game binaries or assets.
- Created `Qeyf/muguet-ios` fork and separate `work/ios27-verified-build` branch; upstream main remains intact.
- Added ZIP/folder manual import, safe staging, mandatory file/shader checks, same-build binding, disabled Play without embedded code, and Apple's advisory memory reading.
- Preserved upstream AOT architecture and dependencies; added locked host/device build, explicit SDK scope, developer archive/export mode and honest sideload/launcher artifact names.
- Added CI diagnostics, optional private hash-checked authorized inputs, temporary signing assets with cleanup and package signature/architecture checks.
- Ran real importer tests and portable policy/security checks; fixed an independent-review validator mismatch with failing-before/passing-after evidence.
- Prepared build, assets, test and continuation documentation.

## In progress

- First actual GitHub Apple Silicon launcher build. Record its URL, log stage, exit status and real artifacts in the test report when available. No game export is needed for this stage.

## Blocked

| Milestone | Actual blocker |
| --- | --- |
| Lawful real game export / game.zip | No rights-holder-authorized compatible WASM/data/shader source or supported retail exporter established |
| Binary hash/ABI/asset consistency | No real authorized binaries supplied; published mirror numbers are not independently measured files |
| Native arm64 game AOT | Requires that exact real WASM and a suitable Apple Silicon host |
| Developer signed device IPA | Apple Developer sign-in, team/App ID/certificate/profile/capabilities and target UDID not verified |
| Physical iPhone execution | This environment has no connection to the user's phone |
| FPS, RAM, heat, audio/input, Jetsam | Require a signed game build, complete matching assets and actual device testing |

No installer-ready or playable GTA V result is claimed. A launcher-only self-signed package, if the actual CI produces one, is a separate intermediate output.

## Next technical actions

1. Inspect actual CI logs, correct a demonstrated cause, and rerun the affected stage; do not repeatedly run an unchanged failing build.
2. Obtain the already lawful complete browser export and trusted producer/build hashes. Validate locally; keep game data out of Git/public artifacts.
3. On adequate existing Apple Silicon capacity, run AOT and device build against the same lock/toolchain/heap cap. Check required module symbols/imports through the upstream precompiler.
4. Authenticate Apple Developer securely, confirm `com.qeyf.muguet`, target UDID and capability eligibility, and provide matching signing inputs through protected secrets or the local Mac keychain.
5. Export `debugging` or registered-device `release-testing` IPA, verify archive/export signatures, install, and fill the physical test table using measurements.

No paid runner, certificate revocation, unrelated signing change, App Store upload or commercial asset redistribution is performed by these preparations.
