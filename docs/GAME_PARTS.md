# Preparing independent game ZIP archives

Updated 2026-10-12. This is an input-preparation record, not a verified game export or a new game IPA. The user reported reaching the launcher on the target phone and supplied a screenshot. That image shows missing embedded game code and a disabled Play control; it does not identify the app version or establish game execution.

## Inputs needed before changing import behavior

The actual 14 user ZIPs have not been supplied. Confirm their filenames and source first. Independent ZIP archives, byte-split ZIP volumes (`.z01`, `.zip.001`) and Windows retail/repack archives require different handling. Renaming a retail archive or concatenating independent ZIP bytes does not produce this project's supported input. The existing ZIP/folder importer should be retained until a real input demonstrates a missing supported case.

The researched `full-game-v1` candidate declares 14 independent `playgta5-part-01-of-14.zip` through `playgta5-part-14-of-14.zip` archives. Its README and downloaded JSON metadata were inspected; its commercial payload was not downloaded. The source expressly does not claim an asset redistribution license. It is not a recommended or rights-holder-verified download source. Technical instructions below apply only to an authorized export with this layout.

## Expected preparation and selection

1. Compare every existing source archive against the appropriate trusted checksum inventory. Validate archive entries and CRC, retain relative paths, reject traversal, symlinks, duplicate/conflicting files and incomplete part sets. These checks cannot be marked passed without the actual archives.
2. For independent archives from a single authorized export, extract their non-conflicting contents into one empty export location using a trusted archive utility. Preserve the directory hierarchy; do not replace a populated data folder with another partial folder. Do not compress the 14 ZIP files inside another ZIP.
3. Select the actual game root in Muguet's existing **Choose game folder** picker. It must directly contain `data/` and `b/`. In the researched metadata, that root is `mirror/playgta5.com`, not the outer download directory or `runtime/`.
4. Validate that complete root with the existing production-compatible validator before packaging/building. The phone will also stage, check and copy a complete input through its existing importer. It replaces the old game only after a successful import.

```text
<selected-game-root>/
  data/
    manifest.json
    <all retained referenced assets>
  b/
    <one-build-id>/
      game.wasm
      shaders/
        index.json
        <referenced loose .wgsl and .consts.json files>
```

Apple documents that tapping a ZIP in Files extracts it into a folder. This alone does not merge multiple folders or prove completeness. A single optional `game.zip` must contain `data/` and `b/` as the game layout; selecting the complete folder already avoids creating another large ZIP. The app uses documented SwiftUI fileImporter, security-scoped URLs and NSFileCoordinator. No new file-picker library or custom archive-concatenation algorithm is introduced.

## Measured metadata, not measured game payload

The two actual downloaded metadata JSON files passed SHA-256 checks against Release asset metadata. Their path list has unique safe relative names, consistent declared counts/totals and valid part references. All individual commercial-file hashes below remain publisher declarations, not measured local binaries or trusted producer signatures.

| Declared item | Value |
| --- | --- |
| Mirror inventory | 12,482 files; 21,158,604,965 bytes |
| Fourteen ZIPs together | 21,192,696,051 bytes |
| Build directory | `8b0b5899ed` |
| WASM | `mirror/playgta5.com/b/8b0b5899ed/game.wasm`; 63,201,802 bytes |
| WASM declared SHA-256 | `11ca8d2c04c5e843d18ff4aea4899d72c86973c6b031df334e67c446b2ae83e0` |
| Shader index | `mirror/playgta5.com/b/8b0b5899ed/shaders/index.json`; 685,955 bytes |
| Asset manifest | `mirror/playgta5.com/data/manifest.json`; 393,014 bytes |
| Part containing all three critical files | `playgta5-part-01-of-14.zip` |
| Actual download-manifest.json SHA-256 | `83a2865640038e07352e554f7bebf78ce3fbdb49235714e5d69af06e7c4082ae` |
| Actual file-inventory.json SHA-256 | `6def50e4232646763c1ad4c7e317a5afbd884b81c428e46f975ca7f58ee1d835` |

No WASM ABI/name-section checks, archive CRC, actual shader references, user-file hashes, license grant or gameplay have passed yet. The folder/build names alone do not establish version compatibility. The 14 parts already on the user's phone must first be matched to the stated source; this document does not assume that match.

## Storage and validation

Metadata-only arithmetic: retaining the ZIPs plus one full mirror uses approximately 42.35 GB (decimal). Retaining the ZIPs, extracted source and a second full copy during app import uses approximately 63.51 GB before filesystem overhead, any previous game and unrelated app/system data. This is a conservative mirror-copy estimate, not a measured importer total. The existing validator reports actual selected/import bytes and capacity needs once actual inputs are available. Do not delete original archives before verification and a recoverable backup.

```bash
python3 scripts/validate_game_assets.py /private/path/selected-game-root --require-code \
  --expected-build-id <verified-build-id> \
  --expected-wasm-sha256 <verified-source-wasm-hash> \
  --report /private/path/game-validation.json
```

Do not use the candidate publisher's declared hash as a proof of rights or a trusted origin. Use actual-file checks and an authorized producer/export source. A successful inventory is still not game-runtime validation. Windows `.cmd`, `.ps1` and the bundled Python executable are not iOS build inputs.

## Game code and the next actual build

This launcher IPA has no embedded game code. Asset import cannot enable Play by itself. The existing upstream path precompiles the actual matching WASM on Apple Silicon and embeds/signs the compiled module into a new arm64 IPA. Import preparation can happen on the phone; the current architecture does not compile and sign new native executable code there.

For an authorized complete export, use the documented ZIP build after full validation, or the upstream-supported raw-WASM build on a controlled Mac with independently verified matching assets. A raw WASM build does not verify the full data/shader archive. Game-code builds must be kept in private/local build storage; this public repository's launcher-only releases contain no commercial game code. Do not run a game-input job in a public Actions repository: its existing game artifact-upload path is not private distribution.

Before actual game AOT, provide the download/source identity and an authorized export's real `game.wasm`, `data/manifest.json` and `shaders/index.json`. The researched WASM declaration is only about 63 MB, so the full 21 GB asset set need not be uploaded merely to examine the actual module. A private complete-input validator report and inventory are still needed to establish asset completeness. No game build is marked successful until host compilation, embedded module checks and real device initialization pass.

## References

- `src/import.rs`, `ios/Launcher.swift`, `make-ios.sh`, `build.rs`, `scripts/validate_game_assets.py`.
- https://developer.apple.com/documentation/swiftui/view/fileimporter(ispresented:allowedcontenttypes:allowsmultipleselection:oncompletion:)
- https://developer.apple.com/documentation/foundation/nsurl/startaccessingsecurityscopedresource()
- https://developer.apple.com/documentation/foundation/nsfilecoordinator
- https://support.apple.com/en-us/102532
- Candidate metadata/provenance audit: `alihammoud21/playgta5`, tag `full-game-v1`; not an authorized download recommendation.
