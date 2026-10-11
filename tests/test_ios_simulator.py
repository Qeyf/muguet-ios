"""Build boundary checks only; recorders cannot prove an iOS UI test passed.

These run the real shell helper and (when Rust 1.98.1 is installed) build.rs.
Apple command boundaries are recorded because those tools cannot run on Linux.
Actual launch and scene recovery are covered by ios/LauncherUITests.swift in CI.
"""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SIM_UUID = "01234567-89AB-CDEF-0123-456789ABCDEF"
INVENTORY = {
    "runtimes": [
        {"identifier": "com.apple.CoreSimulator.SimRuntime.iOS-26-2", "name": "iOS 26.2", "version": "26.2", "isAvailable": True},
        {"identifier": "com.apple.CoreSimulator.SimRuntime.iOS-27-0", "name": "iOS 27.0", "version": "27.0", "isAvailable": True},
        {"identifier": "com.apple.CoreSimulator.SimRuntime.iOS-27-2", "name": "iOS 27.2", "version": "27.2", "buildversion": "24B5089g", "isAvailable": True,
         "supportedDeviceTypes": [{"name": "iPhone 18 Pro Max", "identifier": "com.apple.CoreSimulator.SimDeviceType.iPhone-18-Pro-Max"}]},
        {"identifier": "com.apple.CoreSimulator.SimRuntime.iOS-27-3", "name": "iOS 27.3", "version": "27.3", "isAvailable": False},
    ],
    "devicetypes": [{"name": "iPhone 18 Pro Max", "identifier": "com.apple.CoreSimulator.SimDeviceType.iPhone-18-Pro-Max", "productFamily": "iPhone", "minRuntimeVersion": 1769472, "maxRuntimeVersion": 4294967295}],
    "devices": {}, "pairs": {},
}

RECORDER = r'''#!/usr/bin/env python3
import json, os, pathlib, sys
name = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ["SIM_COMMAND_LOG"], "a") as output:
    output.write(json.dumps({"command": name, "args": args,
        "sdkroot": os.environ.get("SDKROOT"), "heap": os.environ.get("MUGUET_IOS_HEAP_GB"),
        "game_code": os.environ.get("MUGUET_GAME_CODE")}) + "\n")
if name == "uname": print("arm64" if args == ["-m"] else "Darwin")
elif name == "xcodegen" and "--version" in args: print("Version: 2.46.0")
elif name == "xcodebuild":
    if "-version" in args: print("Xcode 27.2\nBuild version 27B5028f")
    elif "test" in args:
        result = pathlib.Path(args[args.index("-resultBundlePath") + 1])
        result.mkdir(parents=True)
        print("native test command recorded; no iOS launch was performed")
        if os.environ.get("SIM_FAIL_NATIVE_TEST") == "1": sys.exit(65)
elif name == "swiftc":
    if os.environ.get("SIM_FAIL_SWIFTC") == "1": sys.exit(1)
elif name == "xcrun":
    if "--show-sdk-version" in args: print("27.2")
    elif "--show-sdk-path" in args: print("/Xcode/SDKs/" + args[args.index("--sdk") + 1] + "27.2.sdk")
    elif "-f" in args: print(pathlib.Path(sys.argv[0]).parent / args[args.index("-f") + 1])
    elif args[:2] == ["simctl", "list"]: print(pathlib.Path(os.environ["SIM_INVENTORY"]).read_text())
    elif args[:2] == ["simctl", "create"]: print("01234567-89AB-CDEF-0123-456789ABCDEF")
    elif args[:2] == ["simctl", "bootstatus"] and os.environ.get("SIM_FAIL_BOOT") == "1": sys.exit(1)
    elif args[:4] == ["xcresulttool", "get", "test-results", "summary"]:
        count = int(os.environ.get("SIM_EXECUTED_TESTS", "1"))
        print(json.dumps({"totalTestCount": count, "passedTests": count, "failedTests": 0, "skippedTests": 0, "testFailures": []}))
'''


class SimulatorContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.build_script_temp = tempfile.TemporaryDirectory()
        cls.build_script = Path(cls.build_script_temp.name) / "build-script"
        rustc = shutil.which("rustc")
        cls.has_rust = bool(rustc and subprocess.run([rustc, "+1.98.1", "--version"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0)
        if cls.has_rust:
            subprocess.run([rustc, "+1.98.1", "--edition=2021", str(ROOT / "build.rs"), "-o", str(cls.build_script)], check=True)

    @classmethod
    def tearDownClass(cls):
        cls.build_script_temp.cleanup()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        (self.root / "scripts").mkdir()
        helper = ROOT / "scripts/test-ios-simulator.sh"
        if helper.exists(): shutil.copy(helper, self.root / "scripts")
        (self.root / "Cargo.lock").write_text("locked boundary fixture")
        (self.root / "ios").mkdir()
        tools = self.root / "tools"
        tools.mkdir()
        for name in ("uname", "rustup", "cargo", "xcodegen", "xcodebuild", "xcrun", "swiftc"):
            path = tools / name
            path.write_text(RECORDER)
            path.chmod(0o755)
        cargo_bin = self.root / "home/.cargo/bin"
        cargo_bin.mkdir(parents=True)
        for name in ("cargo", "rustup"): (cargo_bin / name).symlink_to(tools / name)
        self.log = self.root / "commands.jsonl"
        self.inventory = self.root / "inventory.json"
        self.inventory.write_text(json.dumps(INVENTORY))
        self.env = {key: value for key, value in os.environ.items() if not key.startswith("MUGUET_")}
        self.env.update(PATH=str(tools) + os.pathsep + os.environ["PATH"], HOME=str(self.root / "home"),
                        SIM_COMMAND_LOG=str(self.log), SIM_INVENTORY=str(self.inventory), OUT_DIR=str(self.root / "out"))

    def tearDown(self):
        self.temp.cleanup()

    def commands(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def run_helper(self, **env):
        helper = self.root / "scripts/test-ios-simulator.sh"
        self.assertTrue(helper.is_file(), "simulator test helper must exist")
        return subprocess.run(["bash", str(helper)], cwd=self.root, env=dict(self.env, **env), text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

    def run_build_script(self, target):
        if not self.has_rust: self.skipTest("Rust 1.98.1 is required to execute the real build.rs")
        return subprocess.run([str(self.build_script)], cwd=self.root, env=dict(self.env, TARGET=target, CARGO_CFG_TARGET_OS="ios"),
                              text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

    def test_swift_library_selects_the_simulator_sdk_and_abi(self):
        result = self.run_build_script("aarch64-apple-ios-sim")
        self.assertEqual(result.returncode, 0, result.stdout)
        swift = next(c for c in self.commands() if c["command"] == "swiftc")
        self.assertEqual(swift["args"][swift["args"].index("-target") + 1], "arm64-apple-ios15.0-simulator")
        self.assertEqual(swift["args"][swift["args"].index("-sdk") + 1], "/Xcode/SDKs/iphonesimulator27.2.sdk")
        self.assertIn("lib/swift/iphonesimulator", result.stdout)

    def test_swift_library_keeps_the_device_sdk_and_abi(self):
        result = self.run_build_script("aarch64-apple-ios")
        self.assertEqual(result.returncode, 0, result.stdout)
        swift = next(c for c in self.commands() if c["command"] == "swiftc")
        self.assertEqual(swift["args"][swift["args"].index("-target") + 1], "arm64-apple-ios15.0")
        self.assertEqual(swift["args"][swift["args"].index("-sdk") + 1], "/Xcode/SDKs/iphoneos27.2.sdk")

    def test_postbuild_uses_simulator_binary_and_true_platform(self):
        text = (ROOT / "ios/project.yml").read_text()
        lines = text.split("        script: |\n", 1)[1].splitlines()
        script = "\n".join(line[10:] for line in lines)
        result = subprocess.run(["bash", "-c", script], cwd=self.root,
                                env=dict(self.env, PLATFORM_NAME="iphonesimulator", TARGET_BUILD_DIR=str(self.root / "app"),
                                         EXECUTABLE_PATH="muguet", SRCROOT=str(self.root / "ios"), IPHONEOS_DEPLOYMENT_TARGET="15.0"),
                                text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        self.assertEqual(result.returncode, 0, result.stdout)
        call = next(c for c in self.commands() if c["command"] == "xcrun" and "-set-build-version" in c["args"])
        self.assertEqual(call["args"][1:5], ["-set-build-version", "iossim", "15.0", "27.2"])
        self.assertTrue(call["args"][-1].endswith("target/ios-simulator/aarch64-apple-ios-sim/release/muguet"))

    def test_helper_selects_available_ios27_and_builds_only_locked_no_code_simulator(self):
        result = self.run_helper(MUGUET_GAME_CODE="ambient-code-must-not-be-embedded", MUGUET_IOS_HEAP_GB="4", CARGO_BUILD_TARGET="aarch64-apple-ios")
        self.assertEqual(result.returncode, 0, result.stdout)
        calls = self.commands()
        create = next(c for c in calls if c["command"] == "xcrun" and c["args"][:2] == ["simctl", "create"])
        self.assertEqual(create["args"][-2:], ["com.apple.CoreSimulator.SimDeviceType.iPhone-18-Pro-Max", "com.apple.CoreSimulator.SimRuntime.iOS-27-2"])
        cargo = next(c for c in calls if c["command"] == "cargo" and "build" in c["args"])
        self.assertEqual(cargo["args"][0], "+1.98.1")
        self.assertIn("--locked", cargo["args"])
        self.assertIn("--no-default-features", cargo["args"])
        self.assertEqual(cargo["args"][cargo["args"].index("--target") + 1], "aarch64-apple-ios-sim")
        self.assertEqual(cargo["sdkroot"], "/Xcode/SDKs/iphonesimulator27.2.sdk")
        self.assertEqual(cargo["heap"], "4")
        self.assertIsNone(cargo["game_code"])
        test = next(c for c in calls if c["command"] == "xcodebuild" and "test" in c["args"])
        self.assertEqual(test["args"][test["args"].index("-destination") + 1], "platform=iOS Simulator,id=" + SIM_UUID)
        self.assertIn("-only-testing:MuguetUITests/LauncherUITests/testLauncherSurvivesSceneReactivation", test["args"])
        self.assertEqual(calls[-1]["args"], ["simctl", "delete", SIM_UUID])
        self.assertTrue((self.root / "build/ios-ci/simulator/ui-tests.xcresult").is_dir())

    def test_helper_rejects_missing_available_ios27_before_creating_device(self):
        inventory = json.loads(self.inventory.read_text())
        for runtime in inventory["runtimes"]:
            if runtime["version"].startswith("27."): runtime["isAvailable"] = False
        self.inventory.write_text(json.dumps(inventory))
        result = self.run_helper()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("available iOS 27", result.stdout)
        self.assertFalse(any(c["command"] == "xcrun" and c["args"][:2] == ["simctl", "create"] for c in self.commands()))

    def test_native_test_failure_retains_logs_and_deletes_created_simulator(self):
        result = self.run_helper(SIM_FAIL_NATIVE_TEST="1")
        self.assertEqual(result.returncode, 65, result.stdout)
        self.assertTrue((self.root / "build/ios-ci/simulator/ui-tests.xcresult").is_dir())
        self.assertIn("no iOS launch was performed", (self.root / "build/ios-ci/simulator/xcodebuild.log").read_text())
        self.assertIn("failed", (self.root / "build/ios-ci/simulator/status.txt").read_text())
        self.assertEqual(self.commands()[-1]["args"], ["simctl", "delete", SIM_UUID])

    def test_zero_executed_tests_cannot_report_native_ui_success(self):
        result = self.run_helper(SIM_EXECUTED_TESTS="0")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("launcher UI test did not pass", result.stdout)
        self.assertIn("failed", (self.root / "build/ios-ci/simulator/status.txt").read_text())


if __name__ == "__main__": unittest.main()
