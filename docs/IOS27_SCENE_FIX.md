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
- [ ] Build the same launcher for the official iOS simulator SDK and add XCUITest launch/background checks. Retain actual logs, runtime facts and xcresult.
- [ ] Run the portable suite and actual Apple Silicon build/UI tests; independently inspect the resulting package before delivery.
- [ ] Deliver a separately named fixed IPA with its hash/UUID; leave physical-device success pending the user's reinstall and retest.
- [ ] Publish v0.1.1 (build 2) as a launcher prerelease through the official GitHub CLI after the main-branch native build/UI test passes. Include IPA, compiler, evidence, test report and SHA256SUMS; never replace an existing published version.

Release extension: `.github/workflows/ios-build.yml` will grant write permission only to a dependent main-push release job. `scripts/prepare-ios-release.py` will consume that same run's launcher/evidence artifacts, reject failed native tests, game builds, signature profiles or inconsistent IPA hashes, and produce versioned files plus an honest test report. `tests/test_release_ios.py` will exercise valid packaging and these failure gates before the official `gh release create --target <exact SHA>` publishes the prerelease. No release exists yet.

The selected source is [winit PR #4723](https://github.com/rust-windowing/winit/pull/4723)'s API-compatible [0.30 backport](https://github.com/molenick/winit/tree/94d6a23c425023810a0879aa7a03abb2e4f1decc). It is an open contributor change, not a released or maintainer-approved fix. Its exact SHA is `94d6a23c425023810a0879aa7a03abb2e4f1decc`, one commit on official v0.30.13. The diff adds UIKit scene feature bindings, capability detection and scene-to-window association; it does not change public winit APIs. Original license and credit remain at the pinned source. Its author's 0.31 simulator evidence does not establish success for this backport; this project's own native tests are required.

No SDK metadata is lowered, private UIKit lifecycle API is invoked, or signing certificate changed. No real game WASM/data has been supplied; a launcher test cannot establish gameplay, Metal shader correctness, FPS or device memory behavior.
