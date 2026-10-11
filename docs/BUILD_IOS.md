# iOS build and signing

Verified source baseline: upstream `3c4ea719ac56049f92681f854783360133b55156` on 2026-10-11. This preserves the upstream architecture: a Rust runtime, SwiftUI launcher, Wasmtime AOT module embedded in the signed Mach-O executable, and the patched upstream wgpu-core on Metal.

## Toolchain

| Component | Pin / requirement |
| --- | --- |
| Host | Apple Silicon, macOS 26.6+ for Xcode 27 |
| Xcode | 27.2 beta 2, build `27B5028f` |
| SDK / target | `iphoneos` 27.2 / `aarch64-apple-ios`; minimum deployment 15.0 |
| Rust | 1.98.1, `--locked`; install the iOS target under this same toolchain |
| XcodeGen | 2.46.0, official release SHA-256 checked by CI |
| Wasmtime | 49.0.2 in Cargo.lock; host compiler and device runtime share this lock |
| wgpu | 30.0.1; upstream vendored wgpu-core patch is retained |

The phone's iOS 27.2 beta 3 does not imply an Xcode 27.2 beta 3 exists. Apple and GitHub's checked inventory list beta 2. The workflow explicitly checks SDK and Xcode build before compilation; update a changed pin only after checking the official inventory.

```bash
export DEVELOPER_DIR=/Applications/Xcode_27.2_beta_2.app/Contents/Developer
rustup toolchain install 1.98.1 --profile minimal
rustup target add aarch64-apple-ios --toolchain 1.98.1
xcodebuild -version
xcrun --sdk iphoneos --show-sdk-version
rustc +1.98.1 --version --verbose
```

Install XcodeGen 2.46.0 from its official release, or let the CI helper verify and install it. No game files are required for the launcher build.

## Portable checks

```bash
python3 -m unittest discover -s tests -p 'test_*.py' -v
cargo +1.98.1 test --locked --manifest-path tests/import-harness/Cargo.toml
bash -n make-ios.sh scripts/build-ios-ci.sh
git diff --check
```

The Rust harness compiles the exact production importer, not a separate imitation. Python command-recorder tests check build arguments and signing policy; their synthetic package is never a deliverable or proof of native compilation.

## Build modes

Launcher only, ad-hoc self-signature:

```bash
BUNDLE_ID=com.qeyf.muguet ./make-ios.sh --ipa
```

Matching authorized game export, separate sideload artifact:

```bash
BUNDLE_ID=com.qeyf.muguet MUGUET_IOS_HEAP_GB=5 ./make-ios.sh game.zip --ipa
```

On an authenticated Mac with a connected, paired, unlocked phone and Developer Mode:

```bash
TEAM=<TEAM_ID> BUNDLE_ID=com.qeyf.muguet MUGUET_IOS_HEAP_GB=5 FREE=1 \
  ./make-ios.sh game.zip --install <device-identifier>
```

`FREE=1` chooses the upstream basic entitlement set, even for a paid account; it does not guarantee 5 GB physical memory. Extended mode requires a profile supporting both upstream memory entitlements. Choose a cap based on measured gameplay; an 8 GB cap is not a successful allocation guarantee. The host AOT configuration and iOS runtime cap must agree. Changing it requires rebuilding the app.

The host precompiler uses the macOS SDK and an explicit Rust host target. It patches the real WASM through upstream `src/patch.rs`, precompiles it for `aarch64-apple-ios`, and writes `game.cwasm`. The device build disables JIT and `build.rs` embeds that module into `__GAME,__code`; Xcode preserves the executable segment and signs it. No imported executable code is compiled or made executable on the phone. Upstream optional Binaryen/wasm-opt optimization is not enabled in this reproducible path; the upstream precompiler still patches raw WASM itself, so no additional optimizer is required.

ZIP builds bind the embedded build ID and record the source WASM SHA-256. Legacy raw `.wasm` builds are supported with an explicit warning; they cannot establish matching data/shader identity and should not be used for reproducible distribution.

## Apple Developer archive/export

First check the Apple Developer team, an explicit App ID `com.qeyf.muguet`, certificate/private-key ownership, expiry, capability support and the target iPhone UDID. Do not reuse a profile for a different app or revoke another project's certificate. A Development certificate/profile supports `debugging`; a distribution certificate and Ad Hoc profile supports `release-testing`. `app-store-connect` exports a package but does not upload it, establish TestFlight eligibility, or complete App Review.

Local `--archive` expects installed signing assets:

```bash
TEAM=<TEAM_ID> BUNDLE_ID=com.qeyf.muguet FREE=1 MUGUET_IOS_HEAP_GB=5 \
MUGUET_SIGNING_IDENTITY='<matching certificate name or SHA-1>' \
MUGUET_PROVISIONING_PROFILE_UUID=<PROFILE_UUID> \
MUGUET_EXPORT_OPTIONS_PLIST=/private/path/ExportOptions.plist \
  ./make-ios.sh game.zip --archive
```

Use a manual `ExportOptions.plist` matching the actual method, team, certificate and `{com.qeyf.muguet: <profile UUID>}` provisioning map. Profiles can be installed under Xcode's `~/Library/Developer/Xcode/UserData/Provisioning Profiles` directory; the standalone script also recognizes the legacy `~/Library/MobileDevice/Provisioning Profiles` path. CI generates export options after validating supplied inputs.

Outputs on success: `build/Muguet.xcarchive` and `build/export/*.ipa`; launcher-only archives are named `Muguet-Launcher.xcarchive`. Run `codesign --verify --deep --strict` against both archived and extracted exported apps. Verify embedded profile, team, arm64 architecture, iOS Mach-O platform and SDK. A valid ad-hoc self-signature proves integrity but is not a developer-distribution identity.

## GitHub Actions

`.github/workflows/ios-build.yml` uses the standard public-preview `xcode-27` arm64 runner, explicit Xcode selection and commit-pinned official checkout/upload actions. Pushes to main and pull requests build the launcher. Optional game/signing secrets are limited to explicitly selected manual builds; PRs never receive them. No paid larger runner or self-hosted machine is provisioned automatically.

Manual inputs:

| Input | Values |
| --- | --- |
| `build_mode` | `sideload`, `developer-archive` |
| `game_source` | `none`, `secret-download`, `self-hosted` |
| `heap_gb` | integer 1..8; default 5 |
| `export_method` | `debugging`, `release-testing`, `app-store-connect` |

For an authorized download, set `MUGUET_GAME_URL` to a private HTTPS ZIP URL and `MUGUET_GAME_SHA256` to its independently known whole-archive SHA-256. The downloader requires HTTPS redirects and hides the URL from logs. For local input, use an existing self-hosted Apple Silicon Mac with labels `macOS`, `ARM64`, `muguet-ios`, variable `MUGUET_GAME_LOCAL_PATH`, and the archive hash secret. Large assets never enter Git history.

Developer mode requires repository secrets:

- `APPLE_TEAM_ID`
- `MUGUET_SIGNING_IDENTITY`
- `MUGUET_SIGNING_P12_BASE64`
- `MUGUET_SIGNING_P12_PASSWORD`
- `MUGUET_PROVISION_PROFILE_BASE64`
- `MUGUET_DEVICE_UDID` for Development/Ad Hoc device exports

Optional repository variable `MUGUET_BUNDLE_ID` overrides the default `com.qeyf.muguet`. Do not put secrets in issues, chat, workflow inputs, build logs, or repository files. The CI helper imports P12/profile into a temporary private keychain, verifies profile constraints, restores the previous keychain search list and any existing same-UUID profile, and removes temporary secrets on exit.

The helper records dependency/toolchain facts, importer tests, disk capacity, build logs and available Xcode results under `build/ios-ci/`, including failure status. Successful packaging checks strict codesign, the selected team for developer builds, arm64, `IOS` platform, deployment version and SDK for both archive and exported app. Artifacts retain evidence, actual IPA and compressed archive for 14 days. A no-game launcher artifact is explicitly labeled and cannot start GTA V.

The launcher path was executed successfully in [run 38149241526](https://github.com/Qeyf/muguet-ios/actions/runs/38149241526) on the actual Apple Silicon runner. Downloaded artifact integrity, environment, signature, exact IPA hash and limitations are recorded in [TEST_REPORT.md](TEST_REPORT.md). The developer archive and real-game AOT paths still require their matching private inputs and have not been executed.

## Capacity and physical testing

GitHub's standard arm64 runner lists 7 GB RAM and 14 GB SSD; actual free capacity is recorded and a 2 GiB build reserve is enforced. Authorized downloads also reject a declared or streamed size that would consume that reserve. This reserve is a minimum preflight, not a guarantee that every large AOT build will fit. Producer metadata inspected elsewhere describes roughly 20.9 GB of data alone, which is not downloaded or verified here. A complete source ZIP, extraction, Rust outputs, archive and temporary copies can exceed the standard runner. Use existing suitable self-hosted hardware when the measured archive and build exceed capacity. A paid alternative requires separate authorization.

Installation and gameplay tests are in [TEST_REPORT.md](TEST_REPORT.md). Record the exact beta OS build, app source revision, WASM/archive hashes, signing profile and device model before testing. Capture Jetsam reports and Metal errors instead of retrying a failing command without diagnosis.

## Primary references

- https://developer.apple.com/xcode/system-requirements/
- https://developer.apple.com/news/releases/
- https://github.com/actions/runner-images/blob/main/images/macos/xcode-27-arm64-Readme.md
- https://docs.github.com/en/actions/reference/runners/github-hosted-runners
- https://developer.apple.com/help/account/provisioning-profiles/create-a-development-provisioning-profile/
- https://developer.apple.com/help/account/provisioning-profiles/create-an-ad-hoc-provisioning-profile/
- https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.kernel.increased-memory-limit
- https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.kernel.extended-virtual-addressing
- https://developer.apple.com/documentation/os/os_proc_available_memory()
- https://github.com/bytecodealliance/wasmtime/tree/v49.0.2
- https://github.com/yonaskolb/XcodeGen/releases/tag/2.46.0
