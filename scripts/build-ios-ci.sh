#!/usr/bin/env bash
# Reproducible Apple-runner build. Never enable shell tracing around secrets.
set +x
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

fail() { printf 'error: %s\n' "$*" >&2; exit 1; }
redact_output() {
  python3 -u -c 'import os,sys
keys=("MUGUET_GAME_URL","MUGUET_SIGNING_P12_BASE64","MUGUET_SIGNING_P12_PASSWORD","MUGUET_PROVISION_PROFILE_BASE64","MUGUET_SIGNING_IDENTITY","MUGUET_DEVICE_UDID","TEAM")
secrets=[os.environ[k] for k in keys if os.environ.get(k)]
for line in sys.stdin:
    for secret in secrets: line=line.replace(secret,"<redacted>")
    sys.stdout.write(line)'
}
LOG_DIR="$PWD/build/ios-ci"
mkdir -p "$LOG_DIR"
printf 'running\n' > "$LOG_DIR/status.txt"
PRIVATE_DIR=""
KEYCHAIN=""
KEYCHAIN_ADDED=0
PROFILE_PATH=""
PROFILE_BACKUP=0
ORIGINAL_KEYCHAINS=()
cleanup() {
  result=$?
  trap - EXIT
  if [ "$KEYCHAIN_ADDED" = 1 ]; then
    security list-keychains -d user -s ${ORIGINAL_KEYCHAINS[@]+"${ORIGINAL_KEYCHAINS[@]}"} >/dev/null 2>&1 || true
  fi
  if [ -n "$KEYCHAIN" ]; then security delete-keychain "$KEYCHAIN" >/dev/null 2>&1 || true; fi
  if [ -n "$PROFILE_PATH" ]; then
    if [ "$PROFILE_BACKUP" = 1 ]; then
      cp "$PRIVATE_DIR/original.mobileprovision" "$PROFILE_PATH" || true
    else
      rm -f "$PROFILE_PATH"
    fi
  fi
  [ -z "$PRIVATE_DIR" ] || rm -rf "$PRIVATE_DIR"
  if [ "$result" = 0 ]; then printf 'success\n' > "$LOG_DIR/status.txt"; else printf 'failed (%s)\n' "$result" > "$LOG_DIR/status.txt"; fi
  exit "$result"
}
trap cleanup EXIT

MODE="${MUGUET_CI_MODE:-sideload}"
GAME_SOURCE="${MUGUET_GAME_SOURCE:-none}"
export MUGUET_IOS_HEAP_GB="${MUGUET_IOS_HEAP_GB-5}"
case "$MUGUET_IOS_HEAP_GB" in [1-8]) ;; *) fail 'MUGUET_IOS_HEAP_GB must be an integer from 1 to 8' ;; esac
case "$MODE" in
  sideload) ;;
  developer-archive)
    for key in TEAM MUGUET_SIGNING_IDENTITY MUGUET_SIGNING_P12_BASE64 MUGUET_SIGNING_P12_PASSWORD MUGUET_PROVISION_PROFILE_BASE64; do
      [ -n "${!key:-}" ] || fail "developer archive requires $key"
    done
    export MUGUET_EXPORT_METHOD="${MUGUET_EXPORT_METHOD:-debugging}"
    case "$MUGUET_EXPORT_METHOD" in
      debugging|release-testing)
        [ -n "${MUGUET_DEVICE_UDID:-}" ] || fail "$MUGUET_EXPORT_METHOD export requires MUGUET_DEVICE_UDID"
        ;;
      app-store-connect) ;;
      *) fail 'unsupported MUGUET_EXPORT_METHOD' ;;
    esac
    ;;
  *) fail 'MUGUET_CI_MODE must be sideload or developer-archive' ;;
esac
case "$GAME_SOURCE" in
  none) ;;
  secret-download)
    [ -n "${MUGUET_GAME_URL:-}" ] || fail 'secret download requires MUGUET_GAME_URL'
    [[ "${MUGUET_GAME_SHA256:-}" =~ ^[A-Fa-f0-9]{64}$ ]] || fail 'secret download requires a 64-character MUGUET_GAME_SHA256'
    ;;
  self-hosted)
    [ "${RUNNER_ENVIRONMENT:-}" = self-hosted ] || fail 'local game inputs require a self-hosted runner'
    [ -f "${MUGUET_GAME_LOCAL_PATH:-}" ] || fail 'self-hosted game input requires an existing MUGUET_GAME_LOCAL_PATH'
    [[ "${MUGUET_GAME_SHA256:-}" =~ ^[A-Fa-f0-9]{64}$ ]] || fail 'self-hosted input requires a 64-character MUGUET_GAME_SHA256'
    ;;
  *) fail 'MUGUET_GAME_SOURCE must be none, secret-download or self-hosted' ;;
esac
[ "$(uname -s)" = Darwin ] || fail 'iOS CI builds require macOS with Xcode installed'
[ "$(uname -m)" = arm64 ] || fail 'the verified iOS CI environment requires an Apple silicon runner'

# Official runner inventory, verified 2026-10-11:
# https://github.com/actions/runner-images/blob/main/images/macos/xcode-27-arm64-Readme.md
export DEVELOPER_DIR="${DEVELOPER_DIR:-/Applications/Xcode_27.2_beta_2.app/Contents/Developer}"
[ -d "$DEVELOPER_DIR" ] || fail 'the pinned Xcode 27.2 beta 2 is absent; update the verified pin explicitly'
SDK_VERSION="$(xcrun --sdk iphoneos --show-sdk-version)"
[ "$SDK_VERSION" = 27.2 ] || fail 'selected Xcode must provide the pinned iOS 27.2 SDK'
XCODE_VERSION="$(xcodebuild -version)"
[[ "$XCODE_VERSION" == *'Build version 27B5028f'* ]] || fail 'selected Xcode build differs from the verified 27.2 beta 2 pin'

PRIVATE_DIR="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/muguet-ios.XXXXXX")"
chmod 700 "$PRIVATE_DIR"
python3 - "$LOG_DIR/capacity.json" "$PWD" "$PRIVATE_DIR" <<'PY'
import json, shutil, sys
reserve = 2 << 30
report = {"workspace_free_bytes": shutil.disk_usage(sys.argv[2]).free,
          "private_input_free_bytes": shutil.disk_usage(sys.argv[3]).free,
          "reserved_build_bytes": reserve}
with open(sys.argv[1], "w") as output:
    json.dump(report, output, indent=2)
if min(report["workspace_free_bytes"], report["private_input_free_bytes"]) < reserve:
    sys.exit("Insufficient free disk capacity: at least 2 GiB is required before the build")
PY
export PATH="${HOME}/.cargo/bin:/opt/homebrew/opt/rustup/bin:$PATH"
TOOLCHAIN=1.98.1
XCODEGEN_VERSION=2.46.0
# Digest supplied by the official GitHub release API for xcodegen.zip.
XCODEGEN_SHA256=4d9e34b62172d645eed6457cac13fc222569974098ef4ee9c3368bedf0196806
if ! command -v xcodegen >/dev/null || [ "$(xcodegen --version)" != "Version: $XCODEGEN_VERSION" ]; then
  curl --proto '=https' --proto-redir '=https' --tlsv1.2 --fail --location --silent --show-error \
    "https://github.com/yonaskolb/XcodeGen/releases/download/$XCODEGEN_VERSION/xcodegen.zip" -o "$PRIVATE_DIR/xcodegen.zip"
  [ "$(shasum -a 256 "$PRIVATE_DIR/xcodegen.zip" | awk '{print $1}')" = "$XCODEGEN_SHA256" ] || fail 'XcodeGen release checksum mismatch'
  XCODEGEN_TOOL_DIR="$PRIVATE_DIR/tools"
  if [ -n "${GITHUB_PATH:-}" ]; then
    [ -d "${RUNNER_TEMP:-}" ] || fail 'GitHub tool handoff requires RUNNER_TEMP'
    # This verified public tool must outlive private signing/input cleanup.
    XCODEGEN_TOOL_DIR="$(mktemp -d "$RUNNER_TEMP/muguet-xcodegen.XXXXXX")"
  fi
  unzip -q "$PRIVATE_DIR/xcodegen.zip" -d "$XCODEGEN_TOOL_DIR"
  export PATH="$XCODEGEN_TOOL_DIR/xcodegen/bin:$PATH"
fi
[ "$(xcodegen --version)" = "Version: $XCODEGEN_VERSION" ] || fail 'XcodeGen version mismatch'
if [ -n "${GITHUB_PATH:-}" ]; then
  printf '%s\n' "$(dirname "$(command -v xcodegen)")" >> "$GITHUB_PATH"
fi
rustup toolchain install "$TOOLCHAIN" --profile minimal --no-self-update
rustup target add --toolchain "$TOOLCHAIN" aarch64-apple-ios

# Only explicit, nonsecret build facts enter artifacts.
{
  printf 'recorded_utc=%s\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  printf 'source_revision=%s\n' "$(git rev-parse HEAD)"
  printf 'runner_image=%s\n' "${ImageOS:-unknown}"
  printf 'runner_image_version=%s\n' "${ImageVersion:-unknown}"
  sw_vers
  printf '%s\n' "$XCODE_VERSION"
  printf 'developer_dir=%s\n' "$DEVELOPER_DIR"
  printf 'iphoneos_sdk=%s\n' "$SDK_VERSION"
  printf 'iphoneos_sdk_path=%s\n' "$(xcrun --sdk iphoneos --show-sdk-path)"
  cargo "+$TOOLCHAIN" --version
  rustc "+$TOOLCHAIN" --version --verbose
  xcodegen --version
  printf 'cargo_lock_sha256=%s\n' "$(shasum -a 256 Cargo.lock | awk '{print $1}')"
  printf 'mode=%s\nheap_gb=%s\ngame_source=%s\n' "$MODE" "$MUGUET_IOS_HEAP_GB" "$GAME_SOURCE"
  printf 'physical_device_tests=not-run\ngameplay_tests=not-run\n'
} | tee "$LOG_DIR/environment.txt"

# Exercise the production importer through its pinned harness on the host before
# downloading game inputs, installing signing assets, or compiling the app.
HOST_TARGET="$(rustc "+$TOOLCHAIN" -vV | awk '$1 == "host:" {print $2}')"
[ -n "$HOST_TARGET" ] || fail 'could not determine the pinned Rust host target'
SDKROOT="$(xcrun --sdk macosx --show-sdk-path)" cargo "+$TOOLCHAIN" test --locked \
  --manifest-path tests/import-harness/Cargo.toml --target "$HOST_TARGET" \
  --target-dir target/ios-import-tests 2>&1 | redact_output | tee "$LOG_DIR/importer-tests.log"

# A lawful game input is not required to compile the upstream host compiler.
# Game builds compile and use it in make-ios.sh; launcher runs verify it here.
if [ "$GAME_SOURCE" = none ]; then
  (
    unset MUGUET_GAME_CODE MUGUET_GAME_BUILD_ID MUGUET_GAME_WASM_SHA256 || true
    export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=/home --remap-path-prefix=$PWD=/muguet"
    SDKROOT="$(xcrun --sdk macosx --show-sdk-path)" cargo "+$TOOLCHAIN" build --locked --release \
      --jobs 2 --target "$HOST_TARGET" --target-dir target/ios-precompiler
  ) 2>&1 | redact_output | tee "$LOG_DIR/host-compiler.log"
  HOST_BINARY="$PWD/target/ios-precompiler/$HOST_TARGET/release/muguet"
  xcrun lipo -archs "$HOST_BINARY" | tee "$LOG_DIR/host-compiler-architectures.txt"
  [ "$(cat "$LOG_DIR/host-compiler-architectures.txt")" = arm64 ] || fail 'host compiler must be arm64'
  xcrun vtool -show-build "$HOST_BINARY" > "$LOG_DIR/host-compiler-macho.txt"
  [[ "$(cat "$LOG_DIR/host-compiler-macho.txt")" == *'platform MACOS'* ]] || fail 'host compiler must target macOS'
  codesign --verify --strict "$HOST_BINARY" > "$LOG_DIR/host-compiler-codesign-verify.txt" 2>&1
  python3 - "$HOST_BINARY" "$LOG_DIR/host-compiler-sha256.txt" "$PWD/build/Muguet-AOT-compiler-macos-arm64.zip" <<'PY'
import hashlib, os, pathlib, subprocess, sys, zipfile
binary = pathlib.Path(sys.argv[1])
digest = hashlib.sha256(binary.read_bytes()).hexdigest()
sums = digest + "  muguet\n"
pathlib.Path(sys.argv[2]).write_text(sums)
revision = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
readme = ("Muguet upstream AOT compiler, macOS arm64. No game code or assets included.\n"
          f"Source: https://github.com/Qeyf/muguet-ios/tree/{revision}\n"
          f"Rust 1.98.1; build heap cap: {os.environ['MUGUET_IOS_HEAP_GB']} GiB.\n"
          "Use only with an authorized compatible WASM export and the same iOS build configuration.\n"
          "./muguet --precompile-ios /authorized/game.wasm /private/game.cwasm\n"
          "The compiler has not processed real game WASM in this launcher run.\n"
          "The local signature is ad-hoc; this is not a notarized macOS distribution or an iOS app.\n")
with zipfile.ZipFile(sys.argv[3], "w", compression=zipfile.ZIP_DEFLATED) as package:
    package.write(binary, "muguet")
    package.write("LICENSE", "LICENSE")
    package.writestr("README.txt", readme)
    package.writestr("SHA256SUMS", sums)
PY
fi

GAME_ARGS=()
ARTIFACT_NAME=Muguet-Launcher
if [ "$GAME_SOURCE" != none ]; then
  ARTIFACT_NAME=Muguet
  if [ "$GAME_SOURCE" = secret-download ]; then
    # Read the authorized URL only from the environment, never argv or logs.
    python3 - "$PRIVATE_DIR/authorized-game.zip" <<'PY'
import os, pathlib, shutil, sys, urllib.parse, urllib.request
class HTTPSRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, fp, code, message, headers, newurl):
        if urllib.parse.urlsplit(newurl).scheme != "https":
            raise ValueError("HTTPS required")
        return super().redirect_request(request, fp, code, message, headers, newurl)
try:
    url = os.environ["MUGUET_GAME_URL"]
    parts = urllib.parse.urlsplit(url)
    if parts.scheme != "https" or not parts.netloc or any(ord(c) < 32 for c in url):
        raise ValueError("HTTPS required")
    opener = urllib.request.build_opener(HTTPSRedirect())
    reserve = 2 << 30
    budget = shutil.disk_usage(pathlib.Path(sys.argv[1]).parent).free - reserve
    with opener.open(url, timeout=120) as source, open(sys.argv[1], "wb") as output:
        length = source.headers.get("Content-Length")
        if budget <= 0 or (length is not None and int(length) > budget):
            sys.exit("Authorized archive exceeds available disk capacity after the 2 GiB build reserve")
        received = 0
        for chunk in iter(lambda: source.read(1 << 20), b""):
            received += len(chunk)
            if received > budget or shutil.disk_usage(pathlib.Path(sys.argv[1]).parent).free < len(chunk) + reserve:
                sys.exit("Authorized archive download reached the reserved disk-capacity limit")
            output.write(chunk)
except Exception:
    sys.exit("Authorized game download failed; URL and credentials were withheld")
PY
    GAME_PATH="$PRIVATE_DIR/authorized-game.zip"
  else
    GAME_PATH="$MUGUET_GAME_LOCAL_PATH"
    case "$GAME_PATH" in *.zip|*.wasm) ;; *) fail 'local game input must end in .zip or .wasm' ;; esac
  fi
  ACTUAL_SHA="$(shasum -a 256 "$GAME_PATH" | awk '{print $1}')"
  EXPECTED_SHA="$(printf '%s' "$MUGUET_GAME_SHA256" | tr 'A-F' 'a-f')"
  [ "$ACTUAL_SHA" = "$EXPECTED_SHA" ] || fail 'authorized game input SHA-256 mismatch'
  printf 'input_sha256=%s\n' "$ACTUAL_SHA" > "$LOG_DIR/game-input.txt"
  GAME_ARGS=("$GAME_PATH")
fi

# Preserve the standalone build script's FREE override. CI chooses basic
# developer entitlements for <=5 GB and the upstream extended set above 5 GB.
if [ "${FREE+x}" != x ]; then
  case "$MUGUET_IOS_HEAP_GB" in [1-5]) FREE=1 ;; *) FREE=0 ;; esac
fi
export FREE
if [ "$MODE" = developer-archive ]; then
  export BUNDLE_ID="${BUNDLE_ID:-ch.cclerc.muguet.ios}"
  python3 - "$PRIVATE_DIR" <<'PY'
import base64, os, pathlib, sys
try:
    for name, secret in (("certificate.p12", "MUGUET_SIGNING_P12_BASE64"), ("profile.mobileprovision", "MUGUET_PROVISION_PROFILE_BASE64")):
        path = pathlib.Path(sys.argv[1], name)
        path.write_bytes(base64.b64decode(os.environ[secret], validate=True))
        path.chmod(0o600)
except Exception:
    sys.exit("Signing assets could not be decoded; secret values were withheld")
PY
  security cms -D -i "$PRIVATE_DIR/profile.mobileprovision" > "$PRIVATE_DIR/profile.plist" 2> "$PRIVATE_DIR/profile-errors.txt" || fail 'provisioning profile could not be decoded'
  export MUGUET_EXPORT_OPTIONS_PLIST="$PRIVATE_DIR/ExportOptions.plist"
  MUGUET_PROVISIONING_PROFILE_UUID="$(python3 - "$PRIVATE_DIR/profile.plist" <<'PY'
import datetime, fnmatch, os, plistlib, re, sys
try:
    with open(sys.argv[1], "rb") as source:
        profile = plistlib.load(source)
    team, bundle, method = os.environ["TEAM"], os.environ["BUNDLE_ID"], os.environ["MUGUET_EXPORT_METHOD"]
    identifier = profile["Entitlements"]["application-identifier"]
    uuid = profile["UUID"]
    if team not in profile["TeamIdentifier"] or not fnmatch.fnmatchcase(team + "." + bundle, identifier):
        raise ValueError("team/bundle mismatch")
    if not re.fullmatch(r"[A-Fa-f0-9-]{36}", uuid):
        raise ValueError("invalid profile UUID")
    if profile["ExpirationDate"] <= datetime.datetime.now(datetime.timezone.utc).replace(tzinfo=None):
        raise ValueError("expired profile")
    provisioned_devices = profile.get("ProvisionedDevices", [])
    devices = bool(provisioned_devices)
    debugging = bool(profile["Entitlements"].get("get-task-allow"))
    if method == "debugging" and not (devices and debugging):
        raise ValueError("development profile required")
    if method == "release-testing" and not (devices and not debugging):
        raise ValueError("ad-hoc distribution profile required")
    if method == "app-store-connect" and (devices or debugging or profile.get("ProvisionsAllDevices")):
        raise ValueError("App Store distribution profile required")
    if method in ("debugging", "release-testing") and os.environ["MUGUET_DEVICE_UDID"] not in provisioned_devices:
        sys.exit("Requested MUGUET_DEVICE_UDID is not enrolled in the signing profile")
    if os.environ["FREE"] != "1":
        for entitlement in ("com.apple.developer.kernel.extended-virtual-addressing", "com.apple.developer.kernel.increased-memory-limit"):
            if not profile["Entitlements"].get(entitlement):
                raise ValueError("extended memory profile entitlements required")
    export = {"method": method, "destination": "export", "teamID": team, "signingStyle": "manual",
              "signingCertificate": os.environ["MUGUET_SIGNING_IDENTITY"], "provisioningProfiles": {bundle: uuid}}
    with open(os.environ["MUGUET_EXPORT_OPTIONS_PLIST"], "wb") as output:
        plistlib.dump(export, output)
    print(uuid)
except Exception:
    sys.exit("Signing profile is invalid, expired, or does not match the requested team, app, export method and entitlements")
PY
)"
  export MUGUET_PROVISIONING_PROFILE_UUID
  KEYCHAIN="$PRIVATE_DIR/signing.keychain-db"
  KEYCHAIN_PASSWORD="$(python3 -c 'import secrets; print(secrets.token_urlsafe(32))')"
  while IFS= read -r item; do
    item="${item#*\"}"
    item="${item%\"*}"
    [ -z "$item" ] || ORIGINAL_KEYCHAINS+=("$item")
  done < <(security list-keychains -d user)
  security create-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN" > "$PRIVATE_DIR/keychain.txt" 2>&1
  security set-keychain-settings -lut 21600 "$KEYCHAIN" >> "$PRIVATE_DIR/keychain.txt" 2>&1
  security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$KEYCHAIN" >> "$PRIVATE_DIR/keychain.txt" 2>&1
  security import "$PRIVATE_DIR/certificate.p12" -k "$KEYCHAIN" -P "$MUGUET_SIGNING_P12_PASSWORD" \
    -T /usr/bin/codesign -T /usr/bin/security >> "$PRIVATE_DIR/keychain.txt" 2>&1 || fail 'signing certificate import failed'
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KEYCHAIN_PASSWORD" "$KEYCHAIN" >> "$PRIVATE_DIR/keychain.txt" 2>&1
  KEYCHAIN_ADDED=1
  security list-keychains -d user -s "$KEYCHAIN" ${ORIGINAL_KEYCHAINS[@]+"${ORIGINAL_KEYCHAINS[@]}"}
  security find-identity -v -p codesigning "$KEYCHAIN" > "$PRIVATE_DIR/identities.txt"
  python3 - "$PRIVATE_DIR/identities.txt" <<'PY'
import os, re, sys
identities = open(sys.argv[1]).read()
wanted = os.environ["MUGUET_SIGNING_IDENTITY"]
found = any(wanted == name or wanted.lower() == digest.lower() for digest, name in re.findall(r'([a-fA-F0-9]{40}) "([^"]+)"', identities))
if not found:
    sys.exit("Requested signing identity is not available in the imported keychain")
PY
  PROFILE_DIR="$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles"
  mkdir -p "$PROFILE_DIR"
  INSTALL_PROFILE_PATH="$PROFILE_DIR/$MUGUET_PROVISIONING_PROFILE_UUID.mobileprovision"
  if [ -f "$INSTALL_PROFILE_PATH" ]; then
    cp "$INSTALL_PROFILE_PATH" "$PRIVATE_DIR/original.mobileprovision"
    PROFILE_BACKUP=1
  fi
  PROFILE_PATH="$INSTALL_PROFILE_PATH"
  cp "$PRIVATE_DIR/profile.mobileprovision" "$PROFILE_PATH"
  MODE_ARG=--archive
else
  MODE_ARG=--ipa
fi

export MUGUET_BUILD_RESULT_PATH="$LOG_DIR/build.xcresult"
rm -rf "$MUGUET_BUILD_RESULT_PATH"
./make-ios.sh ${GAME_ARGS[@]+"${GAME_ARGS[@]}"} "$MODE_ARG" 2>&1 | redact_output | tee "$LOG_DIR/build.log"
APP="$(cat "$LOG_DIR/app-path.txt")"
verify_app() {
  app="$1"
  label="$2"
  codesign --verify --deep --strict "$app" 2>&1 | redact_output | tee "$LOG_DIR/$label-codesign-verify.txt"
  codesign --display --verbose=4 "$app" > "$PRIVATE_DIR/$label-codesign.txt" 2>&1
  redact_output < "$PRIVATE_DIR/$label-codesign.txt" > "$LOG_DIR/$label-codesign.txt"
  if [ "$MODE" = sideload ]; then
    grep -Fx 'Signature=adhoc' "$PRIVATE_DIR/$label-codesign.txt" >/dev/null || fail 'sideload artifact is not ad-hoc self-signed'
  else
    grep -Fx "TeamIdentifier=$TEAM" "$PRIVATE_DIR/$label-codesign.txt" >/dev/null || fail 'developer artifact signing team does not match'
    if grep -Fx 'Signature=adhoc' "$PRIVATE_DIR/$label-codesign.txt" >/dev/null; then fail 'developer artifact is unexpectedly ad-hoc signed'; fi
    [ -f "$app/embedded.mobileprovision" ] || fail 'developer artifact lacks its provisioning profile'
  fi
  arch="$(xcrun lipo -archs "$app/muguet")"
  printf '%s\n' "$arch" > "$LOG_DIR/$label-architectures.txt"
  [ "$arch" = arm64 ] || fail 'the application executable is not exclusively arm64'
  xcrun vtool -show-build "$app/muguet" > "$LOG_DIR/$label-macho-build.txt"
  grep -E 'platform[[:space:]]+IOS$' "$LOG_DIR/$label-macho-build.txt" >/dev/null || fail 'the application executable does not target iOS'
  [ "$(awk '$1 == "sdk" {print $2}' "$LOG_DIR/$label-macho-build.txt")" = "$SDK_VERSION" ] || fail 'Mach-O SDK does not match the selected iOS SDK'
  [ "$(awk '$1 == "minos" {print $2}' "$LOG_DIR/$label-macho-build.txt")" = 15.0 ] || fail 'Mach-O minimum iOS version differs from the deployment target'
  xcrun otool -L "$app/muguet" > "$LOG_DIR/$label-linked-libraries.txt"
  # Inspect the actual package, including the native scene association code.
  # UI launch tests separately prove that these declarations are used at runtime.
  python3 - "$app" "$LOG_DIR/$label-scene-lifecycle.json" <<'PY'
import json, plistlib, sys
from pathlib import Path
app = Path(sys.argv[1])
info = plistlib.loads((app / "Info.plist").read_bytes())
manifest = info.get("UIApplicationSceneManifest", {})
configs = manifest.get("UISceneConfigurations", {}).get("UIWindowSceneSessionRoleApplication", [])
if (manifest.get("UIApplicationSupportsMultipleScenes") is not False or
        len(configs) != 1 or not configs[0].get("UISceneConfigurationName") or
        configs[0].get("UISceneClassName") != "UIWindowScene"):
    sys.exit("application package lacks a valid single UIWindowScene configuration")
binary = (app / "muguet").read_bytes()
required = [b"UIWindowScene\0", b"connectedScenes\0", b"setWindowScene:\0"]
if not all(selector in binary for selector in required):
    sys.exit("application binary lacks UIKit window scene association")
Path(sys.argv[2]).write_text(json.dumps({
    "scene_class": "UIWindowScene", "multiple_scenes": False,
    "native_scene_association_selectors": "present",
    "runtime_launch": "requires separate native UI test",
}, indent=2) + "\n")
PY
}
verify_app "$APP" app
if [ "$MODE" = developer-archive ]; then
  EXPORTED_IPAS=(build/export/*.ipa)
  [ "${#EXPORTED_IPAS[@]}" = 1 ] && [ -f "${EXPORTED_IPAS[0]}" ] || fail 'developer export did not produce exactly one IPA'
  unzip -q "${EXPORTED_IPAS[0]}" -d "$PRIVATE_DIR/exported"
  verify_app "$PRIVATE_DIR/exported/Payload/Muguet.app" exported
  ditto -c -k --keepParent "$PWD/build/$ARTIFACT_NAME.xcarchive" "$PWD/build/$ARTIFACT_NAME.xcarchive.zip"
  shasum -a 256 "${EXPORTED_IPAS[0]}" "build/$ARTIFACT_NAME.xcarchive.zip" > "$LOG_DIR/artifact-sha256.txt"
else
  shasum -a 256 "build/$ARTIFACT_NAME-sideload.ipa" > "$LOG_DIR/artifact-sha256.txt"
fi
printf 'Build and package verification passed. Device and gameplay tests have not run.\n'
