# iOS 27 launch regression

## Evidence and cause

The user re-signed the delivered launcher through LCSign and reported an immediate crash on iPhone18,2 / iOS 27.2 beta build 24B5099f. The supplied log records EXC_BREAKPOINT/SIGTRAP in `___UIApplicationEvaluateRuntimeIssueForNoSceneLifecycleAdoption_block_invoke`, before the launcher can appear. The app's UUID `4dd936e3-899a-34b2-97a3-be16dfbb6afb` matches the first delivered IPA exactly. Personal signing/device identifiers from the log are not included here.

The actual package executes Rust main and winit's `UIApplicationMain` path. SwiftUI is a linked UIHostingController helper, not the application entry. Its Info.plist has no UIApplicationSceneManifest; the pinned winit 0.30.13 creates a legacy UIWindow without assigning a UIWindowScene. Earlier successful builds proved compilation/signature integrity, not scene lifecycle adoption or launch. This report supersedes any inference of iOS 27 launch compatibility from compilation alone.

Apple [TN3187](https://developer.apple.com/documentation/technotes/tn3187-migrating-to-the-uikit-scene-based-life-cycle) and [UIKit migration guidance](https://developer.apple.com/documentation/uikit/transitioning-to-the-uikit-scene-based-life-cycle) require scene adoption for apps built with the iOS 27 SDK. Application notifications may still track aggregate foreground/background state. Scene declarations alone do not associate a window with a scene.

## Bounded implementation plan

Goal: retain the existing Rust entry, SwiftUI launcher, Metal and Wasmtime AOT architecture while adopting the documented single-scene lifecycle and testing actual launch.

- [x] Match the supplied crash UUID to the delivered executable and inspect the packaged plist, entry and selected SDK.
- [x] Read Apple's current scene requirements and audit available upstream fixes. Released winit 0.30.13 and 0.31 beta 3 predate the relevant contribution.
- [x] Add failing package-validation regressions for missing scene configuration and missing native scene association; verify the old build is rejected.
- [x] Pin the audited API-compatible 0.30.13 contributor backport, add a valid one-scene UIWindowScene manifest, and update only the affected lock entry. `dpi` 0.1.2 and all other versions remain unchanged.
- [x] Build the same launcher for the official iOS simulator SDK and add XCUITest launch/background checks. Retain actual logs, runtime facts and xcresult.
- [x] Run the portable suite and actual Apple Silicon build/UI tests; independently inspect the resulting package before delivery.
- [x] Deliver a separately named fixed IPA with its hash/UUID; leave physical-device success pending the user's reinstall and retest.
- [x] Publish v0.1.1 (build 2) as a launcher prerelease through the official GitHub CLI after the main-branch native build/UI test passes. Include IPA, compiler, evidence, test report and SHA256SUMS; never replace an existing published version.

Release extension: `.github/workflows/ios-build.yml` grants write permission only to a dependent main-push release job. `scripts/prepare-ios-release.py` consumes that same run's launcher/evidence artifacts, rejects failed native tests, game builds, signature profiles or inconsistent IPA hashes, and produces versioned files plus an honest test report. `tests/test_release_ios.py` exercises valid packaging and these failure gates before the official `gh release create --target <exact SHA>` publishes the prerelease. [v0.1.1 was published](https://github.com/Qeyf/muguet-ios/releases/tag/v0.1.1) after main run 38172991013 passed.

Release review found `gh release create --target` alone can reuse an orphan Git tag pointing to a different source. Two regressions reproduced that behavior. Publication now creates the exact source ref explicitly through the official GitHub REST API before `gh release create --verify-tag`; any existing orphan tag is rejected and retained unchanged.

The selected source is [winit PR #4723](https://github.com/rust-windowing/winit/pull/4723)'s API-compatible [0.30 backport](https://github.com/molenick/winit/tree/94d6a23c425023810a0879aa7a03abb2e4f1decc). It is an open contributor change, not a released or maintainer-approved fix. Its exact SHA is `94d6a23c425023810a0879aa7a03abb2e4f1decc`, one commit on official v0.30.13. The diff adds UIKit scene feature bindings, capability detection and scene-to-window association; it does not change public winit APIs. Original license and credit remain at the pinned source. Its author's 0.31 simulator evidence does not establish success for this backport. This project's own 0.30 backport build and native XCUITest passed in run 38170769173.

No SDK metadata is lowered, private UIKit lifecycle API is invoked, or signing certificate changed. No real game WASM/data has been supplied; a launcher test cannot establish gameplay, Metal shader correctness, FPS or device memory behavior.

## Corrected native result

[PR #3](https://github.com/Qeyf/muguet-ios/pull/3) was merged after [actual run 38170769173](https://github.com/Qeyf/muguet-ios/actions/runs/38170769173) succeeded: 64 portable Python tests, 22 production Rust importer tests, native Mac/compiler and iOS builds, strict self-signature checks, and one genuine XCUITest with no failures or skips. The test checks visible launcher controls, Home/background transition and successful reactivation. It used iPhone 18 Pro Max / iOS 27.0 (`24A434`) with iOS SDK 27.2. This is not a test of the user’s physical iOS 27.2 beta 3 device.

The first corrected PR IPA was independently downloaded and checked: version 0.1.1/build 2, 9,448,909 bytes, SHA-256 `2239d7eb73c4154253a8d65f4f13685bdd7d6a0e4474033da14790fc718697b7`, executable UUID `c9c912da-d158-34bd-9e8b-d0cd6dda463f`. Its real plist has the single-scene configuration and its native binary contains the scene-association selectors. No provisioning profile or game segment is embedded. The user's LCSign re-signing changes the signature/package digest; use the executable UUID and app version to identify a later crash.

A release-tag review added two portable regressions, both passing with the full 66-test local suite. The independent [main run 38172991013](https://github.com/Qeyf/muguet-ios/actions/runs/38172991013) passed at `c0aa4af39503a91f87a0ab50b6ae346f7783bea9`: 66 Python tests, 22 real Rust importer tests, native compiler/device build, strict codesign and one actual XCUITest without failures or skips. The same measured simulator/runtime was used; physical iOS 27.2 beta 3 retest remains pending.

The published Release IPA is a separate main compilation: 9448912 bytes, SHA-256 `ca8cc17e4809b53525773b4cf51a528fd5de4660a26d0cb8cb17043b8cb49c68`, executable UUID `7e89047c-9598-3d8c-9228-3be58e1a98b6`. All five public assets were downloaded and checked against GitHub asset digests and SHA256SUMS. IPA/compiler bytes match the native Actions package exactly; all 72 release evidence files match that run’s evidence. The release Git ref was independently checked to target the exact native source commit above. Earlier corrected PR bytes/UUID remain separately identified, not silently replaced.

## User-reported visible launch on 2026-10-12

The user subsequently reports opening the app and supplies a screenshot of the visible Muguet launcher with disabled Play and the missing-embedded-code message. The screenshot does not identify version/UUID or OS build. This is narrower physical evidence than a fully identified launch/background/game regression test; those remain pending. The next blocker is a real authorized compatible game input and a matching embedded-code build, documented in GAME_PARTS.md.
