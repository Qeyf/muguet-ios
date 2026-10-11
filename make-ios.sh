#!/usr/bin/env bash
# Build the upstream Rust iOS launcher and optionally embed host-precompiled code.
set +x
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

fail() { printf 'error: %s\n' "$*" >&2; exit 1; }
usage() {
  cat <<'EOF'
Usage: ./make-ios.sh [authorized-game.zip|game.wasm] [--ipa|--archive|--install] [device-id]
  --ipa      Create an ad-hoc self-signed sideload IPA (default heap: 5 GB).
  --archive  Archive and export with installed Apple developer signing assets.
  --install  Build with automatic development signing and install on a device.
Developer builds require TEAM. --archive also requires MUGUET_SIGNING_IDENTITY,
MUGUET_PROVISIONING_PROFILE_UUID and MUGUET_EXPORT_OPTIONS_PLIST.
MUGUET_IOS_HEAP_GB accepts 1..8; FREE=1 selects the basic developer entitlements.
EOF
}

GAME=""
MODE=""
DEVICE=""
for arg in "$@"; do
  case "$arg" in
    --help|-h) usage; exit 0 ;;
    --install|--ipa|--archive)
      [ -z "$MODE" ] || fail 'choose one build mode'
      MODE="$arg" ;;
    *.zip|*.wasm)
      [ -z "$GAME" ] || fail 'supply one game archive or Wasm file'
      GAME="$arg" ;;
    --*) fail "unknown option: $arg" ;;
    *) [ -z "$DEVICE" ] || fail 'supply one device identifier'; DEVICE="$arg" ;;
  esac
done
[ -z "$DEVICE" ] || [ "$MODE" = --install ] || fail 'a device identifier requires --install'

TEAM="${TEAM:-}"
BUNDLE_ID="${BUNDLE_ID:-ch.cclerc.muguet.ios}"
ENTITLEMENTS=Muguet.entitlements
HEAP_GB=8
if [ "${FREE:-0}" = 1 ] || [ "$MODE" = --ipa ]; then
  ENTITLEMENTS=Muguet-free.entitlements
  HEAP_GB=5
fi
# An explicitly empty setting is invalid, just like a nonnumeric setting.
export MUGUET_IOS_HEAP_GB="${MUGUET_IOS_HEAP_GB-$HEAP_GB}"
case "$MUGUET_IOS_HEAP_GB" in
  [1-8]) ;;
  *) fail 'MUGUET_IOS_HEAP_GB must be an integer from 1 to 8' ;;
esac
if [ "$MODE" != --ipa ]; then
  [ -n "$TEAM" ] || fail 'set TEAM=<your Apple developer team id>'
fi
if [ "$MODE" = --archive ]; then
  [ -n "${MUGUET_SIGNING_IDENTITY:-}" ] || fail '--archive requires MUGUET_SIGNING_IDENTITY'
  [ -n "${MUGUET_PROVISIONING_PROFILE_UUID:-}" ] || fail '--archive requires MUGUET_PROVISIONING_PROFILE_UUID'
  [ -f "${MUGUET_EXPORT_OPTIONS_PLIST:-}" ] || fail '--archive requires an existing MUGUET_EXPORT_OPTIONS_PLIST'
  PROFILE="$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles/$MUGUET_PROVISIONING_PROFILE_UUID.mobileprovision"
  LEGACY_PROFILE="$HOME/Library/MobileDevice/Provisioning Profiles/$MUGUET_PROVISIONING_PROFILE_UUID.mobileprovision"
  [ -f "$PROFILE" ] || [ -f "$LEGACY_PROFILE" ] || fail '--archive provisioning profile is not installed'
fi
[ "$(uname -s)" = Darwin ] || fail 'iOS builds require macOS with Xcode installed'
[ -f Cargo.lock ] || fail 'Cargo.lock is required for reproducible iOS builds'
[ -z "$GAME" ] || [ -f "$GAME" ] || fail 'game input does not exist'

TOOLCHAIN=1.98.1
export PATH="${HOME}/.cargo/bin:/opt/homebrew/opt/rustup/bin:$PATH"
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=/home --remap-path-prefix=$PWD=/muguet"
IOS_SDKROOT="$(xcrun --sdk iphoneos --show-sdk-path)"
IOS_SDK_VERSION="$(xcrun --sdk iphoneos --show-sdk-version)"
printf 'Using iOS SDK %s; Rust %s; heap %s GB\n' "$IOS_SDK_VERSION" "$TOOLCHAIN" "$MUGUET_IOS_HEAP_GB"
mkdir -p build/ios-ci

ARTIFACT_NAME=Muguet-Launcher
if [ -n "$GAME" ]; then
  ARTIFACT_NAME=Muguet
  mkdir -p target/ios-game
  case "$GAME" in
    *.zip)
      # Validate the whole assets/code build before selecting anything to embed.
      python3 scripts/validate_game_assets.py "$GAME" --require-code --report build/ios-ci/game-validation.json
      MUGUET_GAME_BUILD_ID="$(python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); assert r["valid"]; print(r["build_id"])' build/ios-ci/game-validation.json)"
      MUGUET_GAME_WASM_SHA256="$(python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print(r["wasm_sha256"])' build/ios-ci/game-validation.json)"
      export MUGUET_GAME_BUILD_ID MUGUET_GAME_WASM_SHA256
      # Python addresses this exact validated member without wildcard expansion.
      python3 - "$GAME" target/ios-game/game.wasm build/ios-ci/game-validation.json <<'PY'
import hashlib, json, os, sys, zipfile
report = json.load(open(sys.argv[3]))
member = report["prefix"] + "b/" + os.environ["MUGUET_GAME_BUILD_ID"] + "/game.wasm"
with zipfile.ZipFile(sys.argv[1]) as archive:
    matches = [info for info in archive.infolist() if info.filename.replace("\\", "/").rstrip("/") == member]
    if len(matches) != 1:
        sys.exit("Validated game code member is missing or ambiguous")
    sha = hashlib.sha256()
    with archive.open(matches[0]) as source, open(sys.argv[2], "wb") as output:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            sha.update(chunk)
            output.write(chunk)
    if sha.hexdigest() != report["wasm_sha256"]:
        os.unlink(sys.argv[2])
        sys.exit("Game code changed after archive validation")
PY
      ;;
    *.wasm)
      printf 'raw .wasm input: matching assets cannot be checked without its game archive\n'
      cp "$GAME" target/ios-game/game.wasm
      MUGUET_GAME_WASM_SHA256="$(shasum -a 256 target/ios-game/game.wasm | awk '{print $1}')"
      export MUGUET_GAME_WASM_SHA256
      ;;
  esac
  # SDKROOT is scoped separately so the host precompiler uses the macOS SDK.
  unset MUGUET_GAME_CODE || true
  HOST_TARGET="$(rustc "+$TOOLCHAIN" -vV | awk '$1 == "host:" {print $2}')"
  [ -n "$HOST_TARGET" ] || fail 'Rust did not report its host target'
  SDKROOT="$(xcrun --sdk macosx --show-sdk-path)" cargo "+$TOOLCHAIN" build --locked --release \
    --target "$HOST_TARGET" --target-dir target/ios-precompiler
  "./target/ios-precompiler/$HOST_TARGET/release/muguet" --precompile-ios target/ios-game/game.wasm target/ios-game/game.cwasm
  rm target/ios-game/game.wasm
  export MUGUET_GAME_CODE="$PWD/target/ios-game/game.cwasm"
else
  printf 'No game input: building the launcher without embedded game code\n'
  unset MUGUET_GAME_CODE MUGUET_GAME_BUILD_ID MUGUET_GAME_WASM_SHA256 || true
fi

SDKROOT="$IOS_SDKROOT" IPHONEOS_DEPLOYMENT_TARGET=15.0 cargo "+$TOOLCHAIN" build \
  --locked --release --no-default-features --target aarch64-apple-ios --target-dir target/ios

(cd ios && xcodegen generate --quiet)
rm -rf target/ios-xcode/Build/Products
APP="$PWD/target/ios-xcode/Build/Products/Release-iphoneos/Muguet.app"
COMMON=(-project ios/Muguet.xcodeproj -scheme Muguet -configuration Release
  -sdk iphoneos -destination 'generic/platform=iOS' -derivedDataPath target/ios-xcode)
if [ -n "${MUGUET_BUILD_RESULT_PATH:-}" ]; then
  COMMON+=(-resultBundlePath "$MUGUET_BUILD_RESULT_PATH")
fi
if [ "$MODE" = --ipa ]; then
  xcodebuild "${COMMON[@]}" build PRODUCT_BUNDLE_IDENTIFIER="$BUNDLE_ID" CODE_SIGNING_ALLOWED=NO
  rm -f "$APP/embedded.mobileprovision"
  codesign --force --sign - --entitlements ios/Muguet-sideload.entitlements --generate-entitlement-der "$APP"
elif [ "$MODE" = --archive ]; then
  ARCHIVE="$PWD/build/$ARTIFACT_NAME.xcarchive"
  rm -rf "$ARCHIVE" build/export
  xcodebuild "${COMMON[@]}" -archivePath "$ARCHIVE" archive \
    PRODUCT_BUNDLE_IDENTIFIER="$BUNDLE_ID" CODE_SIGN_STYLE=Manual \
    CODE_SIGN_ENTITLEMENTS="$ENTITLEMENTS" DEVELOPMENT_TEAM="$TEAM" \
    CODE_SIGN_IDENTITY="$MUGUET_SIGNING_IDENTITY" PROVISIONING_PROFILE_SPECIFIER="$MUGUET_PROVISIONING_PROFILE_UUID"
  APP="$ARCHIVE/Products/Applications/Muguet.app"
  xcodebuild -exportArchive -archivePath "$ARCHIVE" -exportPath "$PWD/build/export" \
    -exportOptionsPlist "$MUGUET_EXPORT_OPTIONS_PLIST"
else
  xcodebuild "${COMMON[@]}" -allowProvisioningUpdates build PRODUCT_BUNDLE_IDENTIFIER="$BUNDLE_ID" \
    CODE_SIGN_ENTITLEMENTS="$ENTITLEMENTS" DEVELOPMENT_TEAM="$TEAM"
fi
codesign --verify --deep --strict "$APP"
printf '%s\n' "$APP" > build/ios-ci/app-path.txt
printf 'built: %s (%s)\n' "$APP" "$(du -sh "$APP" | cut -f1)"

if [ "$MODE" = --ipa ]; then
  STAGE="$(mktemp -d)"
  trap 'rm -rf "$STAGE"' EXIT
  mkdir "$STAGE/Payload"
  cp -Rp "$APP" "$STAGE/Payload/"
  IPA="$PWD/build/$ARTIFACT_NAME-sideload.ipa"
  rm -f "$IPA"
  (cd "$STAGE" && zip -qry "$IPA" Payload)
  printf 'built: %s (%s); ad-hoc self-signed for sideloading\n' "$IPA" "$(du -h "$IPA" | cut -f1)"
fi

if [ "$MODE" = --install ]; then
  if [ -z "$DEVICE" ]; then
    DEVICE="$(xcrun devicectl list devices 2>/dev/null | awk '/available \(paired\)|connected/ && /iPhone|iPad/ {for (i=1;i<=NF;i++) if ($i ~ /^[0-9A-F]{8}-[0-9A-F]{16}$/) {print $i; exit}}')"
  fi
  [ -n "$DEVICE" ] || fail 'no iPhone / iPad connected (unlocked, Developer Mode on)'
  xcrun devicectl device install app --device "$DEVICE" "$APP"
  printf 'installed on %s\n' "$DEVICE"
fi
