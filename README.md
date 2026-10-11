# Muguet
A native Mach-O iOS/macOS port of GTA V, with multiplayer

This fork preserves [c22dev/muguet](https://github.com/c22dev/muguet), its attribution and GPL-3.0-or-later license. The iOS work uses the official iOS 27.2 SDK, locked Rust dependencies, safe ZIP/folder import and separate developer-signing and sideload packaging paths. Game code and commercial assets are not supplied.

Current verified results and remaining gates: [test report](docs/TEST_REPORT.md) and [progress](docs/PROGRESS.md). A launcher build without embedded game code cannot play GTA V.

[The corrected launcher passed Apple Silicon CI run 38170769173](https://github.com/Qeyf/muguet-ios/actions/runs/38170769173): actual arm64 device compilation, strict ad-hoc codesign and a native launch/background/reactivation XCUITest. The build uses iOS SDK 27.2; the measured simulator is iPhone 18 Pro Max / iOS 27.0 (`24A434`), not the user’s physical iPhone 17 Pro Max / iOS 27.2 beta 3. The IPA is a launcher with no embedded game code and requires re-signing through the user’s own LCSign profile.

The user installed that first IPA through LCSign and reported an immediate UIKit scene-lifecycle crash on iOS 27.2. Compilation did not prove launch compatibility. The current fix pins an audited, API-compatible winit contributor backport, declares a single UIWindowScene, and passed the actual simulator launch/reactivation test before delivery. See [the crash evidence and source audit](docs/IOS27_SCENE_FIX.md). Physical-device retesting remains pending.

[Version 0.1.1 / build 2 is published in GitHub Releases](https://github.com/Qeyf/muguet-ios/releases/tag/v0.1.1) after [successful main build and native UI run 38172991013](https://github.com/Qeyf/muguet-ios/actions/runs/38172991013). Future versioned downloads are published after the same native gates pass. Each launcher prerelease includes the IPA, standalone Mac AOT compiler, native evidence, test report and SHA256SUMS. Its tag targets the exact tested source; existing published versions are retained. The first scene fix is version 0.1.1, build 2. Increase the app version in `ios/Info.plist` for each new release.

## How does this work ?
this whole port relies on the WebAssembly compilation of the game. in a browser, that `game.wasm` runs inside the JS engine, behind WebGPU, Web Audio, web workers...
Muguet throws that layer away and runs the same `game.wasm` as a regular native app, with [wasmtime](https://wasmtime.dev) for the code and [wgpu](https://wgpu.rs) on Metal for the graphics

On macOS, the game code is compiled to native arm64 the first time you launch it, then cached. On iOS, we can't properly do that (i dont rlly want you to deal with the hassle of enabling JIT), so the game code is compiled when you build the app and baked into it

On top of that, Muguet adds a small UDP multiplayer (for other people who have the same port to play with you)

## Game files
Because I care about me not getting sued, you will need your own copy as a `.zip` : the folder holding `data/` and `b/<build>/` (with `game.wasm` and `shaders/`)

It's basically the same source as the web port.

Muguet itself will then asks for that zip

The iOS launcher also lets you select a folder directly containing `data/` and `b/` in Files. It validates the manifest and shader references, copies into staging, and replaces the previous game only after a successful import. This prepares assets on the phone; executable game code must already be compiled and signed into the app. Ordinary PC GTA V files are not a verified input to this pipeline. See [GAME_ASSETS.md](docs/GAME_ASSETS.md) for lawful acquisition requirements, the exact layout and validation commands.

## Requirements
- Apple Silicon Mac, macOS 14 or later
- Rust toolchain
- Xcode 26 or later for upstream; this fork's verified CI pin is Xcode 27.2 beta 2 (`27B5028f`), iOS SDK 27.2

For iOS, also :
- `rustup` with `rustup toolchain install 1.98.1` and `rustup target add aarch64-apple-ios --toolchain 1.98.1`
- `xcodegen` (`brew install xcodegen`)
- an Apple developer account
- a device on iOS 15 or later, ideally 8 GB of RAM

Xcode 27 requires macOS 26.6 or later. The user's iPhone 17 Pro Max / iOS 27.2 developer beta 3 has not been physically tested here. Neither physical RAM nor a memory entitlement guarantees the app an 8 GB allocation. The launcher reports Apple's current process-memory estimate; `MUGUET_IOS_HEAP_GB` controls the compile-time guest cap and must match the precompiler and runtime.

## Building
macOS :
```
./make-app.sh
```
gives `build/Muguet.app`

iOS :
```
TEAM=<your team id> BUNDLE_ID=com.qeyf.muguet ./make-ios.sh game.zip --install <device-identifier>
```
`BUNDLE_ID=com.qeyf.muguet ./make-ios.sh game.zip --ipa` creates `build/Muguet-sideload.ipa` with an ad-hoc self-signature and a default 5 GB guest cap. It is not an Apple Development or registered-device Ad Hoc distribution signature. With no game input, the output is explicitly named `build/Muguet-Launcher-sideload.ipa`.

For an Apple Developer signed IPA, use `--archive` with a matching installed certificate, profile and export options, or the `developer-archive` CI mode. Development and Ad Hoc profiles must include the target iPhone's UDID; capabilities and export methods must match. Complete commands, secrets and diagnostic outputs are documented in [BUILD_IOS.md](docs/BUILD_IOS.md). No signing material belongs in Git.

Please install this app with AltStore. This probably won't work well with Sideloadly (or even SideStore, as per [this issue])(https://github.com/SideStore/SideStore/issues/1616)

Upstream supports a paid developer account. Code-signing verification, installation, launch and successful gameplay are separate checks; see the device checklist in [TEST_REPORT.md](docs/TEST_REPORT.md).

## Multiplayer
One player hosts from the launcher (or runs `muguet-server` on any machine), the others join with that address. UDP port 47520

## Limitations
- iOS needs the increased memory limit entitlement
- Story missions are single player only
- A lawful matching browser WASM/data/shader export is required. No supported retail-PC-to-WASM converter was found.
- Import validates file inventory and build-directory identity; it does not establish asset licensing, a producer signature, shader compilation or actual gameplay.
- Memory pressure, Jetsam, beta SDK compatibility and GPU allocations require testing on the actual device.

## Credits
- [Kagander](https://github.com/Kagander) for fixing the minimap
- [inj-src](https://github.com/inj-src) for decoding bc textures on all cores
- [Dbhardwaj99](https://github.com/Dbhardwaj99) for reporting real free disk space
- [dre4moff](https://github.com/dre4moff) for releasing controller input when the app loses focus

Contributions are welcome !
