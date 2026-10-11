# Verification report

Updated: 2026-10-12. Retained runtime baseline: `3c4ea719ac56049f92681f854783360133b55156`. Audited upstream snapshot `a2f781683160993ad15a8a04fbf09e7027a831a6` was separately reviewed; its later README/desktop-build changes do not supply an iOS scene fix. Device target is user-reported iPhone 17 Pro Max, arm64, iOS 27.2 developer beta 3; the physical device and installed OS were not accessed.

**Subsequent physical crash evidence:** the user re-signed the first launcher IPA through LCSign and supplied an iPhone18,2 / iOS 27.2 `24B5099f` crash log. It records an immediate UIKit scene-lifecycle assertion. The executable UUID matches the delivered package exactly. This establishes a failed first launch for that package on the reported device, not successful installation/gameplay testing by this environment. The correction and required native launch verification are recorded in [IOS27_SCENE_FIX.md](IOS27_SCENE_FIX.md); the user subsequently reported reaching the launcher and supplied a screenshot on 2026-10-12. The screen shows missing embedded game code and disabled Play. The app version/UUID and OS build are not shown, so this is user-reported visible-launch evidence, not an independently identified Release build or full physical regression test.

## Current evidence

| Check | Result / limit |
| --- | --- |
| Source checkout | Full upstream history retained; latest source fixes, lock and contributor credits integrated |
| Rust 1.98.1 installation | Official Rustup installer checksum verified; real compiler installed on Linux |
| Dependency lock | `cargo +1.98.1 metadata --locked --no-deps` passed; Cargo.lock retained |
| Rust production importer | 22 tests passed; real `src/import.rs` compiled by standalone harness |
| Python asset/security tests | 24 tests passed after review fixes; synthetic inputs, not game data |
| Portable policies | 66 local Python tests passed: 24 assets, 29 build policy, 7 simulator boundary and 6 release policy. Main run 38172991013 passed all 66 as well; the preceding PR run passed its then-current 64 tests. The two added tests cover exact-source/orphan-tag publication |
| Shell/diff checks | Bash syntax and `git diff --check` passed locally |
| Local native build | Real command attempted, rejected: `iOS builds require macOS with Xcode installed` |
| GitHub fork | `Qeyf/muguet-ios` created; separate work branch; upstream baseline preserved as `upstream-main`, verified CI integrated into default main |
| Corrected Apple Silicon build | Passed [main run 38172991013](https://github.com/Qeyf/muguet-ios/actions/runs/38172991013): Rust arm64 device executable, linked SwiftUI helper, host AOT compiler, actual Xcode packaging and strict codesign |
| Native launcher lifecycle | One genuine XCUITest passed without failures/skips: visible controls, Home/background and reactivation on iPhone 18 Pro Max / iOS 27.0 (`24A434`), built with SDK 27.2 |
| Native macOS AOT compiler tool | Passed actual run 38151257119; arm64 MACOS, SDK 27.2, strict codesign, compiled/package hashes and ZIP CRC verified. No real game WASM was executed |
| Published main IPA | [v0.1.1/build 2](https://github.com/Qeyf/muguet-ios/releases/tag/v0.1.1), 9448912 bytes; SHA-256 `ca8cc17e4809b53525773b4cf51a528fd5de4660a26d0cb8cb17043b8cb49c68`; public download/CRC/hash/plist/Mach-O independently verified |
| Launcher signature/platform | Actual macOS strict codesign passed; ad-hoc self-signature, no TeamIdentifier or provisioning profile; arm64, IOS, minimum 15.0, SDK 27.2 |
| Real WASM/asset verification | Blocked by missing authorized complete input; candidates not binary-validated |
| AOT game compilation | Not run; no lawful real game WASM available |
| User signing/profile | User signs through LCSign with their own profile. Signing secrets are not required for this crash fix; this environment has not verified that private profile or its entitlements |
| Installable developer IPA | Not produced |
| Device launch | First package crashed at the UIKit assertion. User reports later launcher opening and supplies visible-screen evidence on 2026-10-12; screenshot has no version/UUID/OS-build identification. Stability, picker/import/background and game tests remain pending |
| Gameplay/FPS/RAM/thermal testing | Not run |

A review follow-up found regular metadata files such as `b/.DS_Store` were incorrectly counted as build directories. Two positive ZIP/folder fixtures reproduced the error, then passed after matching the production importer policy. Disk-capacity diagnostics and download reserve enforcement were also added after review.

No synthetic fixture, recorder-produced ZIP, launcher without code, valid inventory or self-signature is described as a working GTA V installation.

## Corrected scene build evidence

[Run 38170769173](https://github.com/Qeyf/muguet-ios/actions/runs/38170769173), native job `114562716712`, passed. PR head `597363ba9f9652a3fb62eb35ace7c1c7c75fa3b4` and actual checkout `aec919b3c9a8630942c78f5be8ee9304ebbde9ff` were independently confirmed to have the identical tree `c3729bec92fa8b855caf4afa8f96cbd360b2047c`. The source was merged as PR #3. Xcode 27.2 beta 2 (`27B5028f`), SDK 27.2, Rust 1.98.1 and XcodeGen 2.46.0 were used on macOS 27.0.1. The simulator actually ran iOS 27.0 (`24A434`), not iOS 27.2.

The corrected IPA’s UUID is `c9c912da-d158-34bd-9e8b-d0cd6dda463f`. Packaged UIApplicationSceneManifest, UIWindowScene, connectedScenes and setWindowScene selectors are present. arm64/IOS/minimum 15.0/SDK 27.2, ZIP CRC and native-run SHA-256 all passed independent inspection. Actual macOS strict codesign passed; Linux inspection only reads the embedded blob. The ad-hoc package has increased-memory-limit but no extended-virtual-addressing entitlement, provisioning profile or game segment. No 8 GB allocation or device entitlement eligibility is claimed.

### Published main Release

[Run 38172991013](https://github.com/Qeyf/muguet-ios/actions/runs/38172991013) passed both the actual native job `114569406535` and publication job `114574462768`. Exact source/tag target: `c0aa4af39503a91f87a0ab50b6ae346f7783bea9`. It passed 66 Python tests, 22 production Rust importer tests, both native compiler/device builds, strict codesign and one genuine XCUITest (33.184 seconds; zero failed/skipped, no runtime warnings). SDK 27.2 and simulator iPhone 18 Pro Max / iOS 27.0 (`24A434`) are separate measured facts.

[v0.1.1 / build 2](https://github.com/Qeyf/muguet-ios/releases/tag/v0.1.1) is a prerelease, with no physical-device success claimed. The exact published IPA is 9,448,912 bytes, SHA-256 `ca8cc17e4809b53525773b4cf51a528fd5de4660a26d0cb8cb17043b8cb49c68`, UUID `7e89047c-9598-3d8c-9228-3be58e1a98b6`. It is a new main compilation; the earlier corrected PR IPA above has a different digest/UUID and remains historical evidence.

All five public Release assets were downloaded and SHA-256 checked against GitHub metadata; every SHA256SUMS row matched. The IPA and Mac compiler are byte-identical to the same run’s native artifact. Every file in the 72-file published evidence ZIP is byte-identical to that run’s native evidence, including xcresults. ZIP CRC/paths passed. The release ref points to the tested source, not a later documentation commit.

```text
IPA: ca8cc17e4809b53525773b4cf51a528fd5de4660a26d0cb8cb17043b8cb49c68
Mac compiler ZIP: 5afbcf3cf9a4706b4b8a135389ea3768e9b8896f032938458f378cd1e1e6da64
Release evidence ZIP: 2ec7b1dc9f2ce0a7865a29f4ce52837ace7d0ae9e5933822e7002785140aab35
SHA256SUMS: 3680a5a59f4c19f79622e3af0acdfb2fc5cdbcfc2c88a13a6a6c52a1311bae0e
Release report: 3d1ab9c170d4771f827426a51efd4c7992c79e2722cb5a63cb51749656f4886b
Native package artifact ZIP: 5fa872ebd4067252f4139f65c5d422c3bda488b7fe9dc62ffac97ec160bcd86f
Native evidence artifact ZIP: 8116abfcba1e8208d5b095b6d5a2bbe0bd1b8ff9ce603927fd2ced24bf7066e7
```

## Historical first-build evidence — crashed on iOS 27.2

The first PR build completed successfully on 2026-10-11 using GitHub's `xcode-27` Apple Silicon runner: macOS 27.0.1, image `20261006.0244.1`, Xcode 27.2 beta 2 (`27B5028f`), iOS SDK 27.2, Rust 1.98.1 and XcodeGen 2.46.0. The tested PR head was `3c67a8bc584a0fc06f5c5493835efbc9612cd8f8`; checkout recorded GitHub's PR merge revision `b409211c830af0bf4ed9edcad8c470b7eacc0e55`. Job `114498991869` passed the 47 portable Python tests, 22 production Rust importer tests, real native build, strict code-signature/platform checks and artifact uploads. The build emitted two warnings from the upstream patched wgpu-core (unused `clear` and an unfulfilled lint expectation), with no build error.

Downloaded artifact ZIPs were checked against GitHub's reported SHA-256 and passed CRC checks. The extracted IPA also matches `artifact-sha256.txt` from the runner:

```text
Muguet-Launcher-sideload.ipa
bytes: 9448192
sha256: adcd7f2a2d91989e01fc6029da2f6fc36e137a209711f577eec39925a704b60b
IPA artifact ZIP: bbc13dbcb3a3fc8e4c823ae7e766e139a8c7fd366727b71dfd370d0172d36abd
Evidence artifact ZIP: 999647be0a0891107c815299acf650cec64fd9b67045a0f66c5836450cdb5c17
```

Local read-only inspection of the actual IPA independently confirmed `com.qeyf.muguet`, arm64 Mach-O, IOS platform, minimum 15.0, SDK 27.2, an embedded signature blob, no `embedded.mobileprovision` and no `__GAME` segment. It did not rerun Apple's codesign tool on Linux. `Signature=adhoc` and `TeamIdentifier=not set` are recorded by actual macOS codesign output. This package requires authorized re-signing for installation and cannot play GTA V. No AOT game code was compiled, no `game.zip` or developer-signed IPA was produced, and no `.xcarchive` was generated in this sideload run.

The evidence artifact contains actual build/environment/importer logs, signature and Mach-O output, disk-capacity measurements, the IPA digest and `build.xcresult`. Artifacts expire after 14 days on GitHub; the downloaded evidence and IPA are also delivered separately. The later report-only commit does not change the runtime tested by this run.

### Actual standalone upstream AOT compiler

[Run 38151257119](https://github.com/Qeyf/muguet-ios/actions/runs/38151257119), job `114505021093`, also completed successfully. Its PR head was `1f013ab03ec2eb35f714e9d080e45bf5822c95eb`; actual checkout `b1221459b28e7e2553a028e024e6c90d8a971af6` has the identical tree `e593e5d8ce6f3887676442d21cee4465631d00cb`. It passed 49 Python tests (24 asset/security and 25 build policy), 22 production Rust importer tests, the real upstream macOS compiler build, and the real native iOS launcher build. Host compilation took 7 minutes 14 seconds with Rust 1.98.1 and the macOS SDK. No WASM was executed or precompiled in this run. [PR #2](https://github.com/Qeyf/muguet-ios/pull/2) was merged after the actual CI result and independent review.

`Muguet-AOT-compiler-macos-arm64.zip` contains the compiler, GPL license, exact source revision/instructions and `SHA256SUMS`. The package and binary were downloaded and independently verified:

```text
package bytes: 7004090
package sha256: a22218cfa69535b91ed505c737da40b3d56d2379c0e816c9701cad0e3dd5130d
compiler bytes: 22033216
compiler sha256: 409755904b7cc9cb336accd094b482f2ea4e45667fdd64e2c1f4101c2ec3fd49
run artifact ZIP: 8cfbf7cf5955d4f8b18e585e872445afced9e0c3fc9e97c5f03530d3272c6107
run evidence ZIP: ec421758eb0d3801ac6827b29b789c228b30d6f6f9a6850959061385c6050af5
```

Actual macOS `codesign --verify --strict` succeeded. Read-only Linux parsing independently confirms arm64, executable Mach-O, MACOS platform, minimum macOS 11.0, SDK 27.2 and an embedded signature blob. This is an ad-hoc signed, unnotarized Mac tool, not an iOS game binary. The distributed project still requires macOS 14+ for its build workflow. Real game AOT needs an authorized compatible WASM and matching build configuration. The separately delivered launcher IPA remains the first run's exact package/hash above; this later run's IPA also passed its own runner SHA and CRC checks.

## Defects and regression evidence

The upstream importer accepted unsafe wrapped paths and weak build/manifest selection. Preflight now checks the final relative path, conflicting entries, one build, mandatory asset sizes and shader references before extraction. Staged import preserves the previous game on CRC/read failures. Tests cover wrapped/backslash traversal, exact ZIP duplicate names, case/path conflicts, source symlinks, missing/invalid metadata, ambiguous builds, missing constants, CRC failure cleanup, folder mutations, valid ZIP/folder import and manifest order.

Independent code review found the Python validator accepted several inputs the device importer rejected. The root cause was ignoring directories/ancestors and missing control/path-length/retained-row checks. Seven regression cases failed before the correction, then passed; an explicit-directory positive case also passes. Python now checks empty competing build directories, directory spelling conflicts, file/directory conflicts, control characters, path limits and a manifest containing only intentionally skipped entries.

The command-recording build tests check locked toolchain use, separate host/device SDKs, matching heap cap, safe exact ZIP code selection, launcher artifact naming, early signing requirements, profile/keychain restoration and secret input policy. They do not execute Apple tools and cannot establish Swift, Xcode, native linking or signing success.

## Physical iPhone test procedure

Complete each row with measured evidence on the actual target. Pending rows must remain pending.

| Test | Required evidence / acceptance |
| --- | --- |
| Export signature | Strict codesign succeeds on extracted IPA; correct team, bundle, certificate, matching profile and target UDID |
| Install | Paired unlocked iPhone with Developer Mode; devicectl or authorized Apple distribution succeeds |
| First launch | App stays open; record exact OS build and console/crash output |
| ZIP selection | Files picker obtains security scope; valid authorized export imports with progress |
| Folder selection | Select root containing data/b; coordinated reads complete without provider errors |
| Failed import | Invalid/partial ZIP or cancelled/provider read leaves previous valid game intact |
| Game initialization | Matching embedded code and assets; constructor/main/TLS calls succeed |
| Metal/shaders | Required shaders and constants initialize; no validation error or black-world failure |
| Game code | Embedded signed AOT module loads with matching Wasmtime engine configuration |
| Playable scene | Reach actual menu and rendered game scene; record video/logs rather than only a launcher screenshot |
| Controls | Touch movement/buttons and an extended external gamepad work |
| Audio | Audible output, mute setting and interruptions tested |
| RAM | Record guest heap, process footprint, advisory available memory and GPU allocations; capture Jetsam reason on failure |
| FPS/thermal | Measure a defined scene for at least 10 minutes; record resolution/quality/cap, FPS and thermal state |
| Background/foreground | Repeat transitions; inspect suspended import/game, audio recovery and crashes |
| OOM/crash | Inspect Analytics Data Jetsam/crash reports and symbolicate with the exact app build/dSYM |
| Regression | Repeat imports and launch after changes using identical hashes/configuration |

Use Apple's Instruments on the connected device for memory/CPU/Metal measurements. Enable Metal API validation for a development build when diagnosing rendering. Collect `.xcresult`, build logs, device console/crash reports and source/WASM hashes. Do not promote a different phone, simulator or browser result to this device's success.

## Remaining platform uncertainties

`os_proc_available_memory()` is a changing process allowance estimate, not a guest-plus-Metal allocation guarantee. Extended virtual addressing increases address space, not physical RAM; increased memory limit is conditional on device/OS/profile. The upstream shared-memory minimum and GPU usage must fit. Runtime heap reduction is not an automatic universal solution because the AOT engine configuration and module memory requirements constrain it.

Native Rust/Swift compilation and linking with the selected beta SDK are verified for the launcher. File-provider behavior on the phone, Apple filesystem Unicode normalization, signed custom-code-memory execution with real AOT game code, background recovery, device Jetsam and actual shader validity remain unverified until device testing. The upstream line-based DLC XML cleanup remains unchanged; compact producer XML with skipped names is a known inherited risk.

## User-reported launcher and 14-part follow-up

On 2026-10-12, the user supplied a launcher screenshot. It confirms the visible no-embedded-game-code state for that reported run, not successful GTA V initialization. Screenshot SHA-256: `7348cd8b63d38498ce5e30eace6cd3af0139aafa0c56d493f996f12f7304f670`; 2868 × 1320 pixels. The image itself and personal device data are not committed. The reported 14 ZIPs are not present in this workspace. Candidate metadata was hash-checked and internally examined; actual archives/WASM/shader compatibility and license remain unverified. See GAME_PARTS.md.
