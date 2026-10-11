#!/usr/bin/env bash
# Launch the real Rust/Swift no-code app with Apple's native UI test runner.
set +x
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
fail() { printf 'error: %s\n' "$*" >&2; exit 1; }
LOG_DIR="$PWD/build/ios-ci/simulator"
mkdir -p "$LOG_DIR"
printf 'running\n' > "$LOG_DIR/status.txt"
SIM_DEVICE=""
cleanup() {
  result=$?
  trap - EXIT
  if [ -n "$SIM_DEVICE" ]; then
    xcrun simctl shutdown "$SIM_DEVICE" >> "$LOG_DIR/cleanup.log" 2>&1 || true
    xcrun simctl delete "$SIM_DEVICE" >> "$LOG_DIR/cleanup.log" 2>&1 || true
  fi
  if [ "$result" = 0 ]; then printf 'passed\n' > "$LOG_DIR/status.txt"; else printf 'failed (%s)\n' "$result" > "$LOG_DIR/status.txt"; fi
  exit "$result"
}
trap cleanup EXIT
[ "$(uname -s)" = Darwin ] || fail 'native simulator tests require macOS with Xcode'
[ "$(uname -m)" = arm64 ] || fail 'native simulator tests require an Apple silicon Mac'
[ -f Cargo.lock ] || fail 'Cargo.lock is required for native simulator tests'
export PATH="${HOME}/.cargo/bin:/opt/homebrew/opt/rustup/bin:$PATH"
export MUGUET_IOS_HEAP_GB="${MUGUET_IOS_HEAP_GB-5}"
case "$MUGUET_IOS_HEAP_GB" in [1-8]) ;; *) fail 'MUGUET_IOS_HEAP_GB must be an integer from 1 to 8' ;; esac
unset MUGUET_GAME_CODE MUGUET_GAME_BUILD_ID MUGUET_GAME_WASM_SHA256 || true
[ "$(xcodegen --version)" = 'Version: 2.46.0' ] || fail 'native simulator tests require the prepared XcodeGen 2.46.0'
SDK_VERSION="$(xcrun --sdk iphonesimulator --show-sdk-version)"
[[ "$SDK_VERSION" == 27.* ]] || fail 'native simulator tests require an iOS 27 simulator SDK'
SIM_SDKROOT="$(xcrun --sdk iphonesimulator --show-sdk-path)"
{
  xcodebuild -version
  printf 'simulator_sdk=%s\nsimulator_sdk_path=%s\n' "$SDK_VERSION" "$SIM_SDKROOT"
  printf 'rust_toolchain=1.98.1\nheap_gb=%s\nphysical_device_tests=not-run\ngameplay_tests=not-run\n' "$MUGUET_IOS_HEAP_GB"
} | tee "$LOG_DIR/environment.txt"

xcrun simctl list --json > "$LOG_DIR/inventory.json"
python3 - "$LOG_DIR/inventory.json" "$LOG_DIR/selection.json" <<'PY'
import json, re, sys
with open(sys.argv[1]) as source:
    inventory = json.load(source)
def version(value):
    return tuple(int(part) for part in value.split('.'))
runtimes = sorted((r for r in inventory['runtimes']
                   if r.get('isAvailable') and r['identifier'].startswith('com.apple.CoreSimulator.SimRuntime.iOS-')
                   and version(r['version'])[0] == 27), key=lambda r: version(r['version']), reverse=True)
if not runtimes:
    sys.exit('No available iOS 27 simulator runtime; native launch verification cannot run')
types = {t['identifier']: t for t in inventory['devicetypes']}
for runtime in runtimes:
    supported = runtime.get('supportedDeviceTypes')
    if supported is not None:
        candidates = [types.get(t['identifier'], t) for t in supported]
    else:
        parts = version(runtime['version']) + (0, 0)
        number = (parts[0] << 16) | (parts[1] << 8) | parts[2]
        candidates = [t for t in types.values() if 'minRuntimeVersion' in t and 'maxRuntimeVersion' in t
                      and t['minRuntimeVersion'] <= number <= t['maxRuntimeVersion']]
    candidates = [t for t in candidates if t.get('name', '').startswith('iPhone')]
    if not candidates:
        continue
    # Prefer a large viewport so both no-code launcher controls are visible.
    def rank(device):
        model = re.search(r'iPhone (\d+)', device['name'])
        return ('Pro Max' in device['name'], int(model[1]) if model else 0, device['name'])
    device = max(candidates, key=rank)
    with open(sys.argv[2], 'w') as output:
        json.dump({'runtime_identifier': runtime['identifier'], 'runtime_version': runtime['version'],
                   'runtime_build': runtime.get('buildversion'), 'device_type_identifier': device['identifier'],
                   'device_type_name': device['name']}, output, indent=2)
    break
else:
    sys.exit('No compatible iPhone type for an available iOS 27 simulator runtime')
PY
SIM_RUNTIME="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["runtime_identifier"])' "$LOG_DIR/selection.json")"
SIM_TYPE="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["device_type_identifier"])' "$LOG_DIR/selection.json")"

rustup target add --toolchain 1.98.1 aarch64-apple-ios-sim 2>&1 | tee "$LOG_DIR/rust-target.log"
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=/home --remap-path-prefix=$PWD=/muguet"
SDKROOT="$SIM_SDKROOT" IPHONEOS_DEPLOYMENT_TARGET=15.0 cargo +1.98.1 build --locked --release \
  --no-default-features --target aarch64-apple-ios-sim --target-dir target/ios-simulator \
  2>&1 | tee "$LOG_DIR/rust-build.log"
(cd ios && xcodegen generate --quiet)

SIM_DEVICE="$(xcrun simctl create "Muguet scene verification ${GITHUB_RUN_ID:-local}-$$" "$SIM_TYPE" "$SIM_RUNTIME")"
[[ "$SIM_DEVICE" =~ ^[A-Fa-f0-9-]{36}$ ]] || fail 'simctl did not return a simulator UUID'
printf '%s\n' "$SIM_DEVICE" > "$LOG_DIR/device-uuid.txt"
xcrun simctl boot "$SIM_DEVICE" 2>&1 | tee "$LOG_DIR/boot.log"
xcrun simctl bootstatus "$SIM_DEVICE" -b 2>&1 | tee -a "$LOG_DIR/boot.log"
RESULT="$LOG_DIR/ui-tests.xcresult"
rm -rf "$RESULT"
xcodebuild -project ios/Muguet.xcodeproj -scheme MuguetUITests -configuration Release \
  -sdk iphonesimulator -destination "platform=iOS Simulator,id=$SIM_DEVICE" \
  -derivedDataPath target/ios-simulator-xcode -resultBundlePath "$RESULT" \
  -parallel-testing-enabled NO \
  -only-testing:MuguetUITests/LauncherUITests/testLauncherSurvivesSceneReactivation \
  test ARCHS=arm64 ONLY_ACTIVE_ARCH=YES CODE_SIGNING_ALLOWED=NO \
  MUGUET_APP_BUNDLE_ID="${BUNDLE_ID:-com.qeyf.muguet}" \
  2>&1 | tee "$LOG_DIR/xcodebuild.log"
xcrun xcresulttool get test-results summary --path "$RESULT" > "$LOG_DIR/test-summary.json"
python3 - "$LOG_DIR/test-summary.json" <<'PY'
import json, sys
with open(sys.argv[1]) as source:
    summary = json.load(source)
if not (summary.get('totalTestCount') == 1 and summary.get('passedTests') == 1
        and summary.get('failedTests') == 0 and summary.get('skippedTests') == 0):
    sys.exit('The launcher UI test did not pass exactly once; inspect the retained xcresult')
PY
