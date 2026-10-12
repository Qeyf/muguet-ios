# Authorized game input and file verification

No commercial game code or assets are included in this repository. No real `game.zip` was produced during this work. Validator fixtures are deliberately tiny synthetic data and an empty WASM module; they are not a game or a playable export.

## Required layout

```text
game.zip
  data/
    manifest.json
    <each retained file referenced by the manifest>
  b/
    <one-build-id>/
      game.wasm
      shaders/
        index.json
        <hash>.wgsl
        <hash>.consts.json  (where required)
```

ZIPs can wrap this layout in one outer folder. For extracted inputs the folder picker expects `data/` and `b/` directly. Version 0.1.2 also accepts a folder of independent ZIPs with one shared game root inside their combined inventory. Supply one complete build; mixing code, shader or data versions is unsupported. Keep source files out of the app's current `Documents/game` folder while importing.

`data/manifest.json` must be a JSON object with a nonempty `files` array. Each row contains `[relative_path, byte_length, ...]`. The order is meaningful: upstream I/O identifies files by row index. Every retained reference must exist with exactly that uncompressed byte length. The importer preserves order and normalizes paths. It intentionally excludes upstream `env_test` and `verityradio` content and updates manifest sizes when it changes the inherited DLC list. At least one mandatory retained asset is required.

`shaders/index.json` is a JSON object keyed by 16-character hexadecimal shader hashes. The runtime reads loose `<hash>.wgsl` for records with `ok: true` and an array in `<hash>.consts.json` when `consts` requests it. Metadata keys starting with `_` are ignored. Browser `_packs` and binary packs do not replace these loose files. `ok: false` records are reported as unavailable; successful inventory checks do not prove those rendering paths work.

For an iOS game build, `game.wasm` must be from the same build directory. Wasmtime and upstream patching must accept the actual module, import ABI, memory64/shared-memory features, exports and name section. Upstream patching expects names including `GtaThread::Update(int)`, `CExplosionManager::AddExplosion(CExplosionManager::CExplosionArgs&, CProjectile*, bool)` and `CTimeWarper::SetTargetTimeWarp(TW_Dir)`. An eight-byte WASM magic/version check alone establishes none of these.

## What the phone can prepare

Select an authorized complete ZIP, extracted export folder, or folder containing every independent ZIP part in Files. The app holds security-scoped access and coordinates reads through `NSFileCoordinator`. The importer checks inventory and metadata before writing, streams files into a staging folder, reports progress, performs CRC/length checks, and only then replaces the old game. Failed imports remove staging and preserve the existing game where possible. A launcher without embedded code can import assets, but its Play control stays disabled.

The phone does not merge unrelated retail GTA files, reconstruct missing shaders, convert a Windows executable into this WASM ABI, or compile/sign new native game code. The upstream supported path compiles WASM ahead of time on an Apple Silicon Mac. A generic on-device converter would change the architecture and would not establish either compatibility or executable-code permission.

## Lawful source gate

The supplied HML host and `alihammoud21/playgta5` are investigation leads. Neither was verified as a rights-holder-authorized distributor. The latter expressly claims no redistribution license for the assets. Public reachability and filenames do not establish rights. No WASM, shader source, shader pack or Rockstar game archive was downloaded from these candidates.

Muguet's GPL license covers its supplied source code, not a separate grant for Rockstar content. A purchased PC installation is not evidence that an independent rehosted browser snapshot may be acquired or redistributed. No documented authorized PC-installation-to-this-WASM exporter or supported retail-version mapping was found in the inspected projects.

To continue, supply an already lawful, complete browser export with provenance covering local use and compilation, plus producer/exporter information, build identity and trusted checksums. If starting with a licensed PC copy, first provide the authorized exporter/toolchain and its supported game version; do not send arbitrary `.exe`/`.rpf` files as though they are interchangeable with this format. Keep personal game assets private and use them locally or through an access-controlled source; do not attach them to a public PR or release.

## Multiple independent archives

The folder-of-parts import is implemented in the production Rust importer. [GAME_PARTS.md](GAME_PARTS.md) gives the one-selection phone procedure, exact actual input hashes, missing-asset boundary and limits. The desktop Python validator still expects one complete ZIP or an extracted root. No byte concatenation, additional codec or retail-PC conversion was added.

## Offline validation

```bash
python3 scripts/validate_game_assets.py /private/path/game.zip --require-code \
  --report /private/path/game-validation.json
python3 scripts/validate_game_assets.py /private/path/export-folder --require-code \
  --report /private/path/folder-validation.json
```

Optional `--expected-build-id` and `--expected-wasm-sha256` compare against known producer/build values. The script hashes files by reading them to EOF, so ZIP CRC is checked without extraction. Its report lists file SHA-256 values, the WASM size/hash, manifest references, usable/unavailable shader counts, selected/import bytes, archive hash and capacity estimates.

`valid: true` means the specified inventory, path, size, CRC, hash and WASM-header checks passed. It deliberately records `game_runtime_verified: false` and `license_verified: false`. Full WASM validation and patch compatibility occur in the real host build; Metal and gameplay require the device. Matching directory IDs prevent accidental build mixing but are not cryptographic proof of asset-to-code compatibility. The recorded WASM hash identifies compilation input; the phone does not independently hash a producer-signed complete asset inventory.

After validation, an authorized folder can be zipped on the Mac without symlinks:

```bash
cd /private/path/export-folder
zip -r /private/path/game.zip data b
cd /path/to/muguet-ios
python3 scripts/validate_game_assets.py /private/path/game.zip --require-code \
  --report /private/path/game-validation.json
shasum -a 256 /private/path/game.zip
```

Validation is mandatory after packaging: inspect all referenced paths, sizes and checksums; do not equate a successful ZIP command with complete game data.

## Archive safety and capacity

The importer rejects traversal, absolute/drive paths, control characters, empty or overlong path components, duplicate/case-conflicting paths, file/directory conflicts, symlinks and special files. Limits: 250,000 entries, 128 GiB total uncompressed, 32 GiB per file, 32 MiB JSON metadata, 1,024 UTF-8 bytes per path and 255 per component. ZIP support is stored/deflate, without encryption.

Python additionally uses NFD/casefold canonicalization; Rust uses lowercase spelling and exclusive output creation. Unicode equivalence on Apple's filesystem remains a device-test item. ZIP central-directory parsing allocates library metadata before the application can enforce the entry cap. Folder reads are coordinated and check ancestors, but a hostile concurrently mutating provider is not a fully eliminated race. These limits are recorded rather than described as a complete adversarial-filesystem sandbox.

The report's `minimum_free_import_bytes_with_margin` estimates the new imported data plus 16 MiB margin. Allow more for filesystem overhead, cloud-provider materialization, backup/staging and unrelated app/system needs. The old game remains on disk until the new import succeeds. `storage_with_source_zip_bytes` adds the ZIP and imported data, but excludes that previous game and app build outputs. Always compare actual free space before import/build. Producer metadata describing 20,944,285,552 bytes of data was inspected; actual assets, their sizes and completeness were not verified here.

## Evidence sources

- Upstream schema: `src/import.rs`, `src/io.rs`, `src/fs.rs`, `src/gpu.rs`, `src/patch.rs`, `src/main.rs`.
- https://github.com/c22dev/muguet
- https://github.com/alihammoud21/playgta5
- https://github.com/alihammoud21/playgta5/releases/tag/full-game-v1
- https://github.com/shadany7824/playgta5/blob/main/data-manifest.json
- https://github.com/shadany7824/playgta5/blob/main/shader-index.json
- https://www.take2games.com/legal/en-US/
