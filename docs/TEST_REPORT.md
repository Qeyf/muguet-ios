# Verification report

Date: 2026-10-11. Upstream main: `3c4ea719ac56049f92681f854783360133b55156`. Device target is user-reported iPhone 17 Pro Max, arm64, iOS 27.2 developer beta 3; the physical device and installed OS were not accessed.

## Current evidence

| Check | Result / limit |
| --- | --- |
| Source checkout | Full upstream history retained; latest source fixes, lock and contributor credits integrated |
| Rust 1.98.1 installation | Official Rustup installer checksum verified; real compiler installed on Linux |
| Dependency lock | `cargo +1.98.1 metadata --locked --no-deps` passed; Cargo.lock retained |
| Rust production importer | 22 tests passed; real `src/import.rs` compiled by standalone harness |
| Python asset/security tests | 24 tests passed after review fixes; synthetic inputs, not game data |
| Build policy tests | 25 portable command-recorder tests passed locally, including UDID, actual disk-capacity evidence and no-game host compiler failure/packaging policy; initial native runs used 23 |
| Shell/diff checks | Bash syntax and `git diff --check` passed locally |
| Local native build | Real command attempted, rejected: `iOS builds require macOS with Xcode installed` |
| GitHub fork | `Qeyf/muguet-ios` created; separate work branch, main preserved |
| Apple Silicon native build | Passed [run 38149241526](https://github.com/Qeyf/muguet-ios/actions/runs/38149241526); Rust arm64 static library, Swift launcher, native link and Xcode build succeeded |
| Native macOS AOT compiler tool | New no-game compilation gate prepared; actual CI compilation pending. No real game WASM is executed by this check |
| Launcher IPA | Produced and downloaded `Muguet-Launcher-sideload.ipa`, 9,448,192 bytes; ZIP CRC and SHA-256 match CI evidence |
| Launcher signature/platform | Actual macOS strict codesign passed; ad-hoc self-signature, no TeamIdentifier or provisioning profile; arm64, IOS, minimum 15.0, SDK 27.2 |
| Real WASM/asset verification | Blocked by missing authorized complete input; candidates not binary-validated |
| AOT game compilation | Not run; no lawful real game WASM available |
| Developer signing/profile | Not configured or verified; Apple portal sign-in and device UDID are outstanding |
| Installable developer IPA | Not produced |
| Device/gameplay/FPS/RAM/thermal testing | Not run |

A review follow-up found regular metadata files such as `b/.DS_Store` were incorrectly counted as build directories. Two positive ZIP/folder fixtures reproduced the error, then passed after matching the production importer policy. Disk-capacity diagnostics and download reserve enforcement were also added after review.

No synthetic fixture, recorder-produced ZIP, launcher without code, valid inventory or self-signature is described as a working GTA V installation.

## Actual native build and artifact evidence

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
