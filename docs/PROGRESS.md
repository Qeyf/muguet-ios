# Progress and blocking inputs

Updated 2026-10-12; each status describes evidence rather than intended behavior.

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
- Matched the user’s real crash UUID and packaged plist; diagnosed the UIKit no-scene-lifecycle assertion using Apple’s official documentation. Added a single-scene configuration and exact-revision API-compatible winit contributor backport; retained SDK 27.2 and all dependency versions.
- Completed actual corrected native PR run [38170769173](https://github.com/Qeyf/muguet-ios/actions/runs/38170769173): 64 Python, 22 real Rust importer tests, native compiler/device build, strict codesign and one genuine launch/background/reactivation XCUITest passed without skips. Simulator: iPhone 18 Pro Max, iOS 27.0 (`24A434`); SDK: 27.2. PR #3 merged.
- Downloaded and independently checked the corrected version 0.1.1/build 2 IPA, including scene configuration, native scene selectors, UUID, SHA-256 and CRC. Delivered that intermediate PR package for LCSign/device retesting.
- Release-tag and runner tool-handoff review regressions failed before correction, passed after; all 66 local Python tests now pass.
- Completed main [run 38172991013](https://github.com/Qeyf/muguet-ios/actions/runs/38172991013): all 66 Python and 22 real Rust tests, actual compiler/device builds, strict codesign and real launch/background/reactivation XCUITest passed.
- Published [v0.1.1 / build 2](https://github.com/Qeyf/muguet-ios/releases/tag/v0.1.1) through official GitHub CLI 2.102.0 at exact tested source `c0aa4af39503a91f87a0ab50b6ae346f7783bea9`. IPA, compiler, 72-file native evidence, test report and SHA256SUMS are versioned Release assets. All five public files were downloaded and their digests checked; IPA/compiler exactly match CI, and evidence contents match the same run. No existing version was overwritten.
- Inspected the user’s launcher screenshot and researched the reported 14-ZIP input model. Hash-verified only two public metadata JSONs, checked their path/part/count consistency, and recorded the nested root, critical code/manifest paths and storage estimates in GAME_PARTS.md. Actual user archives and commercial binaries were not supplied or downloaded.
- Updated build, assets, test and continuation documentation. The report-only follow-up changes no native source; Markdown-only main pushes do not repeat the heavy build. The filter’s YAML and the six publication policy tests passed locally.

## In progress

- On 2026-10-12 the user reports opening the launcher and supplies a visible screenshot. Its app version/UUID/OS build are not shown; physical stability, picker/import, background recovery and gameplay still need testing. The screen correctly shows missing embedded game code.
- The user signs locally with LCSign using their own profile. The new crash correction does not require account login or signing-key transfer. Developer archive/export inputs remain unavailable to this environment; no profile eligibility/private-key ownership has been independently verified here.

## Blocked

| Milestone | Actual blocker |
| --- | --- |
| Lawful real game export / game.zip | No rights-holder-authorized compatible WASM/data/shader source or supported retail exporter established |
| Binary hash/ABI/asset consistency | No real authorized binaries supplied; published mirror numbers are not independently measured files |
| Native arm64 game AOT | Apple Silicon launcher and upstream host compiler builds are verified; game AOT still requires the exact lawful real WASM and adequate game-compilation RAM/disk |
| Developer signed device IPA | The user re-signs through LCSign with their own profile; profile/device/capability eligibility has not been verified in this environment |
| Physical iPhone game validation | User-reported launcher screenshot is available; this environment has no device connection, identified game build or gameplay trace |
| FPS, RAM, heat, audio/input, Jetsam | Require a signed game build, complete matching assets and actual device testing |

No developer-installer-ready or playable GTA V result is claimed. The actual launcher-only self-signed IPA is a verified separate intermediate output.

## Next technical actions

1. Identify the user’s actual 14 ZIP filenames/source and authorized export. Obtain the actual compatible WASM plus metadata; verify the complete local data/shader set with the existing validator. Preserve the working launcher; subsequent tests should identify its version/UUID and exact OS build.
2. Obtain the already lawful complete browser export and trusted producer/build hashes. Validate locally; keep game data out of Git/public artifacts.
3. On adequate existing Apple Silicon capacity, run AOT and device build against the same lock/toolchain/heap cap. Check required module symbols/imports through the upstream precompiler.
4. Re-sign the corrected launcher locally through LCSign, preserving required capability eligibility in the user’s matching profile, then retest physical launch. A later developer archive can use a matching local Mac keychain or protected secrets without placing private keys in Git.
5. Export `debugging` or registered-device `release-testing` IPA, verify archive/export signatures, install, and fill the physical test table using measurements.

No paid runner, certificate revocation, unrelated signing change, App Store upload or commercial asset redistribution is performed by these preparations.

## 2026-10-12 continuation

- Actual user-supplied game.wasm, manifest.json and index.json were measured and match the declared 8b0b5899ed snapshot. Full assets and loose shader bytes remain absent here.
- Implemented direct independent-ZIP-folder import using the existing library, FFI and staging transaction; no intermediate combined ZIP or extracted source copy.
- Candidate version 0.1.2/build 3: 48 production Rust tests and 66 portable Python checks passed; independent review completed and a measured cross-archive metadata performance issue fixed.
- A private build orchestration repo Qeyf/muguet-ios-game-build was created. It preserves source history by cloning the full public fork history at the exact build SHA; it is not a source-history mirror.
- Native candidate builds, exact input transfer, embedded game-code IPA checks and versioned delivery remain in progress. No actual candidate IPA or physical gameplay is marked successful until those gates pass.
