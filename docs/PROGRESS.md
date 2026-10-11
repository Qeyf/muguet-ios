# Progress and blocking inputs

Updated 2026-10-11; each status describes evidence rather than intended behavior.

## Completed

- Cloned upstream with history/license/attribution; reviewed Rust, Wasmtime, wgpu, Metal-facing code, launcher, build script, lock and asset import.
- Refreshed main to `3c4ea719ac56049f92681f854783360133b55156`; the overnight upstream changes include Metal/shader/minimap, texture decoding, statfs, controller focus handling, background code loading and local ZIP discovery. Their source and updated Cargo.lock are retained. The earlier audit snapshot used `11a914d` and remains identified as a snapshot.
- Verified official iOS 27.2 SDK / Xcode 27.2 beta 2 availability and GitHub's standard `xcode-27` arm64 image.
- Investigated supplied game sources and public metadata without downloading unverified commercial game binaries or assets.
- Created `Qeyf/muguet-ios` fork and separate `work/ios27-verified-build` branch; upstream history is retained, with the audited upstream main snapshot preserved as `upstream-main`.
- Added ZIP/folder manual import, safe staging, mandatory file/shader checks, same-build binding, disabled Play without embedded code, and Apple's advisory memory reading.
- Preserved upstream AOT architecture and dependencies; added locked host/device build, explicit SDK scope, developer archive/export mode and honest sideload/launcher artifact names.
- Added CI diagnostics, optional private hash-checked authorized inputs, temporary signing assets with cleanup and package signature/architecture checks.
- Ran real importer tests and portable policy/security checks; fixed an independent-review validator mismatch with failing-before/passing-after evidence.
- Completed [actual Apple Silicon CI run 38149241526](https://github.com/Qeyf/muguet-ios/actions/runs/38149241526): 47 Python tests, 22 Rust importer tests, Rust/Swift/native build, strict codesign and arm64/IOS/SDK checks passed.
- A second actual native build [38150188431](https://github.com/Qeyf/muguet-ios/actions/runs/38150188431) also passed. Integrated the verified pipeline into the fork's default `main` so GitHub can offer manual game/signing dispatch; no protected signing/game run has been performed.
- Downloaded and checksum/CRC-verified the real launcher IPA and build evidence, including `.xcresult`. The IPA has an ad-hoc self-signature and no game code or provisioning profile; its exact hash is recorded in the test report.
- Completed [actual run 38151257119](https://github.com/Qeyf/muguet-ios/actions/runs/38151257119): 49 Python and 22 production Rust tests, upstream macOS arm64 AOT compiler build/package/signature checks, and the native iOS launcher build all passed. Downloaded the separate compiler package and verified its binary/package SHA-256, ZIP CRC, GPL license, source revision and MACOS/arm64 format. It contains no game code/assets and has processed no real WASM. PR #2 was merged after verification and independent review.
- Two new no-game compilation/failure-gate tests reproduced the missing stage before implementation, then passed with all 49 Python tests locally and in real CI.
- Prepared build, assets, test and continuation documentation.

## In progress

- Secure Apple Developer authentication and read-only verification of the team, matching App ID and registered device. Signing material is not available to the build yet.

## Blocked

| Milestone | Actual blocker |
| --- | --- |
| Lawful real game export / game.zip | No rights-holder-authorized compatible WASM/data/shader source or supported retail exporter established |
| Binary hash/ABI/asset consistency | No real authorized binaries supplied; published mirror numbers are not independently measured files |
| Native arm64 game AOT | Apple Silicon launcher and upstream host compiler builds are verified; game AOT still requires the exact lawful real WASM and adequate game-compilation RAM/disk |
| Developer signed device IPA | Apple Developer sign-in, team/App ID/certificate/profile/capabilities and target UDID not verified |
| Physical iPhone execution | This environment has no connection to the user's phone |
| FPS, RAM, heat, audio/input, Jetsam | Require a signed game build, complete matching assets and actual device testing |

No developer-installer-ready or playable GTA V result is claimed. The actual launcher-only self-signed IPA is a verified separate intermediate output.

## Next technical actions

1. Preserve the successful native build and its downloaded evidence. Correct and rerun only an affected stage if a new input or demonstrated failure requires it.
2. Obtain the already lawful complete browser export and trusted producer/build hashes. Validate locally; keep game data out of Git/public artifacts.
3. On adequate existing Apple Silicon capacity, run AOT and device build against the same lock/toolchain/heap cap. Check required module symbols/imports through the upstream precompiler.
4. Authenticate Apple Developer securely, confirm `com.qeyf.muguet`, target UDID and capability eligibility, and provide matching signing inputs through protected secrets or the local Mac keychain.
5. Export `debugging` or registered-device `release-testing` IPA, verify archive/export signatures, install, and fill the physical test table using measurements.

No paid runner, certificate revocation, unrelated signing change, App Store upload or commercial asset redistribution is performed by these preparations.
