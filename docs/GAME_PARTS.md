# Import all independent ZIP parts with one folder selection

Updated 2026-10-12. Version 0.1.2/build 3 adds folder-of-ZIPs preparation through the existing SwiftUI fileImporter, security-scoped access, NSFileCoordinator and pinned ZIP library. Native build and actual phone results are tracked separately in TEST_REPORT.md.

## On the iPhone

1. Keep all fourteen independent ZIPs in one local Files folder, preferably under On My iPhone. Keep their original names. Finish downloading every file before selection.
2. Do not extract them individually, concatenate their bytes, or put the ZIPs inside another ZIP. Keep an extracted `data/manifest.json` root in a separate folder.
3. Install the matching game-code IPA through your own LCSign profile. A public launcher-only IPA can prepare data but cannot enable Play.
4. Tap **Choose game folder…** and choose the folder holding all ZIPs once. Its ZIP members may use the producer's shared `mirror/playgta5.com/` wrapper; the importer identifies that single game root itself.
5. Wait for import to finish. Every manifest asset and required loose shader/constants file must be present across the union. Failures preserve the previous game and show the actual error.
6. When a game-code app and the matching complete assets are present, Play becomes available. Device initialization, rendering, audio and performance still need a real phone test.

A complete single game ZIP and an extracted folder directly holding `data/` and `b/` remain supported. This feature is not a PC-game converter. Split ZIP streams (`.z01`, `.zip.001`) require a different source and are rejected.

## What is checked

The importer inventories every top-level independent `.zip` before staging. A `name-part-01-of-14.zip` set requires every declared part, one prefix and one total; missing/duplicate parts, mixed numbered sets and unrelated extra ZIPs are rejected. Generic archive names require a complete combined manifest/shader inventory.

The union must have one manifest and one matching build under `b/`. Validation rejects conflicting/case-colliding paths, traversal, symlinks, special files, foreign build roots, missing assets, incorrect byte lengths, missing WGSL/constants and invalid JSON. It enforces 256 ZIPs, 250,000 entries, 128 GiB aggregate uncompressed size, 32 GiB per file and 32 MiB per metadata file.

Files stream directly from the selected ZIPs into the existing import staging area. Only one ZIP reader is cached; no combined archive or full intermediate extracted source is created. Reopened members are bound to their inventoried path, size, type and CRC. Constants are read in archive order to avoid repeatedly parsing different ZIP directories. Existing manifest row order and the upstream intentional skips remain intact.

A successful import replaces old data only after validation and complete reads. It is an inventory/integrity result, not proof of origin, copyright permission or successful gameplay.

## Actual supplied input identity

The user supplied the following three files; their real bytes were measured. The complete fourteen ZIPs, loose shader files and commercial asset bytes have not been supplied to this workspace.

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `game.wasm` | 63,201,802 | `11ca8d2c04c5e843d18ff4aea4899d72c86973c6b031df334e67c446b2ae83e0` |
| `data/manifest.json` | 393,014 | `c5d7b041dbcde22e54f97dc271a52690ea6101391bfda3fb9ecd13d8ef748c1a` |
| `b/8b0b5899ed/shaders/index.json` | 685,955 | `11c331895051b30126980ef666bd8935314ab400348b17c56ef8fe8983f50ee0` |

These match the declared `8b0b5899ed` metadata snapshot. Matching hashes establish byte identity; no rights-holder distribution license was verified. No unauthorized game mirror payload was downloaded or published.

The actual manifest declares 5,814 assets / 20,944,285,552 bytes. Upstream intentionally skips 277 `env_test`/`verityradio` entries / 5,502,243,796 bytes, leaving 5,537 assets / 15,442,041,756 bytes (14.38 GiB), plus imported loose shaders and metadata. The actual shader index has 4,919 records: 4,808 marked successful, 111 marked failed and 1,786 constants tables required. Missing/failed rendering paths are not repaired by import.

Allow free space for at least the new 15.44 GB of retained data, shader files and filesystem overhead, in addition to ZIPs and any previous game. The importer does not guarantee a free-space reservation; write failures roll back staging. Cloud providers may require extra local materialization. The earlier estimate involving both an extracted source copy and an import copy does not apply to this direct ZIP-folder path.

## Code compilation and private builds

On-device preparation does not add new executable game code. The existing upstream `--precompile-ios` path patches and precompiles the matching user-provided WASM, then make-ios.sh embeds it in the signed executable's `__GAME` section. The app must be built again for another game-code build.

Public source history, license and launcher-only releases remain in Qeyf/muguet-ios. Real user game inputs and game-code build outputs belong only in the private Qeyf/muguet-ios-game-build orchestration workspace or local/private storage. Public Actions reject game-input modes before download/build. Raw-WASM compilation does not prove complete asset/shader availability.

The desktop `scripts/validate_game_assets.py` currently supports a complete ZIP or an extracted root. It does not accept a folder of independent ZIPs; the new phone path is validated by the production Rust importer. No fake complete game.zip is produced from the three supplied files.

## References

- `src/import.rs`, `ios/Launcher.swift`, `src/main.rs`, `make-ios.sh`, `build.rs`.
- https://developer.apple.com/documentation/swiftui/view/fileimporter(ispresented:allowedcontenttypes:allowsmultipleselection:oncompletion:)
- https://developer.apple.com/documentation/foundation/nsurl/startaccessingsecurityscopedresource()
- https://developer.apple.com/documentation/foundation/nsfilecoordinator
- https://github.com/c22dev/muguet
