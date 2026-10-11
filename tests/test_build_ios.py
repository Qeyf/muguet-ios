"""Portable checks for the iOS build contract; no Apple compilation is simulated.

The command recorder substitutes macOS-only tools so these tests can observe
arguments, fail-fast policy, and packaging behavior on Linux. It cannot prove
that Xcode or the Rust iOS target compiles the application.
"""

import json
import hashlib
import base64
import datetime
import plistlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import zipfile


ROOT = Path(__file__).resolve().parents[1]

RECORDER = r'''#!/usr/bin/env python3
import json, os, pathlib, plistlib, sys, zipfile
name = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ["BUILD_COMMAND_LOG"], "a") as out:
    out.write(json.dumps({"command": name, "args": args,
        "heap": os.environ.get("MUGUET_IOS_HEAP_GB"),
        "build_id": os.environ.get("MUGUET_GAME_BUILD_ID"),
        "wasm_sha256": os.environ.get("MUGUET_GAME_WASM_SHA256"),
        "game_code": os.environ.get("MUGUET_GAME_CODE"),
        "sdkroot": os.environ.get("SDKROOT")}) + "\n")
def value(flag, default=""):
    return args[args.index(flag) + 1] if flag in args else default
if name == "uname":
    print("arm64" if args == ["-m"] else "Darwin")
elif name == "cargo":
    if "--version" in args:
        print("cargo 1.98.1 (test recorder)")
    elif "test" in args:
        print("production importer test command recorded; no compilation performed")
        if os.environ.get("BUILD_FAIL_IMPORT_TESTS") == "1":
            sys.exit("synthetic importer test failure")
    elif "build" in args:
        target = pathlib.Path(value("--target-dir", "target"))
        if os.environ.get("BUILD_FAIL_HOST_COMPILER") == "1" and target.name == "ios-precompiler":
            sys.exit("synthetic host compiler build failure")
        binary = target / (value("--target") + "/release/muguet" if "--target" in args else "release/muguet")
        binary.parent.mkdir(parents=True, exist_ok=True)
        binary.write_text(pathlib.Path(__file__).read_text())
        binary.chmod(0o755)
elif name == "muguet":
    with open(os.environ["BUILD_COMMAND_LOG"], "a") as out:
        import hashlib
        out.write(json.dumps({"command": "precompiler-input", "sha256": hashlib.sha256(pathlib.Path(args[1]).read_bytes()).hexdigest()}) + "\n")
    pathlib.Path(args[-1]).write_bytes(b"precompiled recorder output")
elif name == "rustc":
    print("rustc 1.98.1 (test recorder)")
    if "-vV" in args or "--verbose" in args:
        print("host: aarch64-apple-darwin")
elif name == "xcodegen":
    if "--version" in args:
        version = os.environ.get("BUILD_INSTALLED_XCODEGEN_VERSION", "2.46.0") if pathlib.Path(__file__).parent.name == "tools" else "2.46.0"
        print("Version: " + version)
elif name == "curl":
    # Synthetic public-release ZIP, not the downloaded upstream binary.
    with zipfile.ZipFile(value("-o"), "w") as archive:
        executable = zipfile.ZipInfo("xcodegen/bin/xcodegen")
        executable.create_system = 3
        executable.external_attr = 0o100755 << 16
        archive.writestr(executable, pathlib.Path(__file__).read_text())
        archive.writestr("xcodegen/share/xcodegen/SettingPresets/fixture.yml", "synthetic fixture")
elif name == "shasum":
    if pathlib.Path(args[-1]).name == "xcodegen.zip":
        # Explicit checksum boundary fixture; this does not validate release bytes.
        digest = "0" * 64 if os.environ.get("BUILD_XCODEGEN_BAD_CHECKSUM") == "1" else "4d9e34b62172d645eed6457cac13fc222569974098ef4ee9c3368bedf0196806"
        print(digest + "  " + args[-1])
    else:
        os.execv(os.environ["BUILD_REAL_SHASUM"], [os.environ["BUILD_REAL_SHASUM"], *args])
elif name == "xcodebuild":
    if "-version" in args:
        print("Xcode 27.2\nBuild version 27B5028f")
    elif "-exportArchive" in args:
        path = pathlib.Path(value("-exportPath"))
        path.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(path / "Muguet.ipa", "w") as archive:
            archive.writestr("Payload/Muguet.app/muguet", "binary")
    else:
        if "archive" in args:
            app = pathlib.Path(value("-archivePath")) / "Products/Applications/Muguet.app"
        else:
            app = pathlib.Path(value("-derivedDataPath")) / "Build/Products/Release-iphoneos/Muguet.app"
        app.mkdir(parents=True, exist_ok=True)
        selectors = b"" if os.environ.get("BUILD_MISSING_SCENE_BACKEND") == "1" else b"\0UIWindowScene\0connectedScenes\0setWindowScene:\0"
        (app / "muguet").write_bytes(b"recorder binary" + selectors)
        info = plistlib.loads(pathlib.Path("ios/Info.plist").read_bytes())
        if os.environ.get("BUILD_MISSING_SCENE_MANIFEST") == "1":
            info.pop("UIApplicationSceneManifest", None)
        (app / "Info.plist").write_bytes(plistlib.dumps(info))
        if "archive" in args:
            (app / "embedded.mobileprovision").write_bytes(b"recorder profile")
elif name == "xcrun":
    if "--show-sdk-version" in args:
        print("27.2")
    elif "--show-sdk-path" in args:
        if "macosx" in args:
            print("/Applications/Xcode_27.2_beta_2.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX27.2.sdk")
        else:
            print("/Applications/Xcode_27.2_beta_2.app/Contents/Developer/Platforms/iPhoneOS.platform/Developer/SDKs/iPhoneOS27.2.sdk")
    elif "lipo" in args:
        print("arm64")
    elif "vtool" in args and "-show-build" in args:
        platform = "MACOS" if any("ios-precompiler" in arg for arg in args) else "IOS"
        print(f"Load command 0\n      cmd LC_BUILD_VERSION\n platform {platform}\n    minos 15.0\n      sdk 27.2")
elif name == "codesign":
    if "-d" in args or "--display" in args:
        print("Signature=adhoc\nTeamIdentifier=not set", file=sys.stderr)
elif name == "security":
    if "find-identity" in args:
        print('1) AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA "Apple Development: Recorder (TEST123456)"')
    elif "cms" in args:
        sys.stdout.buffer.write(pathlib.Path(os.environ["BUILD_PROFILE_PLIST"]).read_bytes())
    elif "list-keychains" in args and "-s" not in args:
        print('"' + os.environ["HOME"] + '/login.keychain-db"')
elif name == "sw_vers":
    print("ProductVersion: 27.0.1")
elif name == "git":
    print("11a914d")
elif name == "cp":
    if os.environ.get("BUILD_FAIL_PROFILE_BACKUP") == "1" and any(arg.endswith("original.mobileprovision") for arg in args):
        sys.exit(1)
    os.execv("/bin/cp", ["/bin/cp", *[arg.replace("-cRp", "-Rp") for arg in args]])
'''


class IOSBuildContract(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        shutil.copy(ROOT / "make-ios.sh", self.root / "make-ios.sh")
        shutil.copytree(ROOT / "ios", self.root / "ios")
        (self.root / "Cargo.lock").write_text("locked test fixture")
        (self.root / "scripts").mkdir()
        shutil.copy(ROOT / "scripts/validate_game_assets.py", self.root / "scripts/validate_game_assets.py")
        shutil.copy(ROOT / "LICENSE", self.root / "LICENSE")
        if (ROOT / "scripts/build-ios-ci.sh").exists():
            shutil.copy(ROOT / "scripts/build-ios-ci.sh", self.root / "scripts/build-ios-ci.sh")
        shutil.copytree(ROOT / "tests/import-harness", self.root / "tests/import-harness",
                        ignore=shutil.ignore_patterns("target"))
        tools = self.root / "tools"
        tools.mkdir()
        for tool in ("uname", "cargo", "rustc", "rustup", "xcodegen", "xcodebuild", "xcrun", "codesign", "security", "sw_vers", "git", "cp"):
            path = tools / tool
            path.write_text(RECORDER)
            path.chmod(0o755)
        # The build prefers rustup's conventional paths. Put recorders in the
        # temporary HOME too so macOS tests never pick up a real Homebrew Rust.
        cargo_bin = self.root / "home/.cargo/bin"
        cargo_bin.mkdir(parents=True)
        for tool in ("cargo", "rustc", "rustup"):
            (cargo_bin / tool).symlink_to(tools / tool)
        self.log = self.root / "commands.jsonl"
        self.env = dict(os.environ, PATH=str(tools) + os.pathsep + os.environ["PATH"],
                        HOME=str(self.root / "home"), BUILD_COMMAND_LOG=str(self.log))
        for key in tuple(self.env):
            if key.startswith("MUGUET_") or key in ("TEAM", "FREE", "BUNDLE_ID", "GITHUB_PATH"):
                self.env.pop(key)

    def tearDown(self):
        self.temp.cleanup()

    def run_build(self, *args, env=None, ci=False):
        path = self.root / ("scripts/build-ios-ci.sh" if ci else "make-ios.sh")
        self.assertTrue(path.is_file(), f"missing build helper: {path.name}")
        return subprocess.run(["bash", str(path), *args], cwd=self.root,
                              env=dict(self.env, **(env or {})), text=True,
                              stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

    def commands(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def signed_fixture_env(self):
        developer = self.root / "Xcode/Contents/Developer"
        developer.mkdir(parents=True, exist_ok=True)
        uuid = "01234567-89AB-CDEF-0123-456789ABCDEF"
        profile = {"UUID": uuid, "TeamIdentifier": ["TEST123456"],
                   "ExpirationDate": datetime.datetime(2030, 1, 1),
                   "ProvisionedDevices": ["SYNTHETIC-DEVICE"],
                   "Entitlements": {"application-identifier": "TEST123456.com.qeyf.muguet", "get-task-allow": True}}
        decoded = self.root / "profile.plist"
        decoded.write_bytes(plistlib.dumps(profile))
        return dict(DEVELOPER_DIR=str(developer), RUNNER_TEMP=str(self.root),
                    MUGUET_CI_MODE="developer-archive", TEAM="TEST123456", BUNDLE_ID="com.qeyf.muguet",
                    MUGUET_DEVICE_UDID="SYNTHETIC-DEVICE",
                    MUGUET_SIGNING_IDENTITY="Apple Development: Recorder (TEST123456)",
                    MUGUET_SIGNING_P12_BASE64=base64.b64encode(b"SYNTHETIC-NOT-A-CERTIFICATE").decode(),
                    MUGUET_SIGNING_P12_PASSWORD="synthetic-password",
                    MUGUET_PROVISION_PROFILE_BASE64=base64.b64encode(b"SYNTHETIC-NOT-A-PROFILE").decode(),
                    BUILD_PROFILE_PLIST=str(decoded)), uuid

    def test_heap_rejects_invalid_values_before_building(self):
        for value in ("0", "9", "01", "abc", "", "5.0"):
            with self.subTest(value=value):
                result = self.run_build("--ipa", env={"MUGUET_IOS_HEAP_GB": value})
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("MUGUET_IOS_HEAP_GB must be an integer from 1 to 8", result.stdout)
        self.assertFalse(any(c["command"] == "cargo" for c in self.commands()))

    def test_no_game_build_has_explicit_launcher_sideload_package(self):
        result = self.run_build("--ipa")
        self.assertEqual(result.returncode, 0, result.stdout)
        ipa = self.root / "build/Muguet-Launcher-sideload.ipa"
        self.assertTrue(ipa.is_file(), result.stdout)
        with zipfile.ZipFile(ipa) as archive:
            self.assertIn("Payload/Muguet.app/muguet", archive.namelist())
        cargo = [c for c in self.commands() if c["command"] == "cargo" and "build" in c["args"]]
        self.assertEqual(len(cargo), 1)
        self.assertEqual(cargo[0]["args"][0], "+1.98.1")
        self.assertIn("--locked", cargo[0]["args"])
        self.assertIn("--no-default-features", cargo[0]["args"])
        self.assertEqual(cargo[0]["heap"], "5")
        self.assertIsNone(cargo[0]["game_code"])

    def test_host_precompiler_and_runtime_share_locked_toolchain_and_heap(self):
        game = self.root / "authorized.wasm"
        game.write_bytes(b"\0asm\1\0\0\0")
        result = self.run_build(str(game), "--ipa", env={"MUGUET_IOS_HEAP_GB": "4"})
        self.assertEqual(result.returncode, 0, result.stdout)
        cargo = [c for c in self.commands() if c["command"] == "cargo" and "build" in c["args"]]
        self.assertEqual(len(cargo), 2)
        for command in cargo:
            self.assertEqual(command["args"][0], "+1.98.1")
            self.assertIn("--locked", command["args"])
            self.assertEqual(command["heap"], "4")
        precompiler = [c for c in self.commands() if c["command"] == "muguet"]
        self.assertEqual(precompiler[0]["args"][0], "--precompile-ios")
        self.assertEqual(precompiler[0]["heap"], "4")
        self.assertIn("raw .wasm", result.stdout)

    def test_archive_requires_signing_assets_before_building(self):
        result = self.run_build("--archive", env={"TEAM": "TEST123456"})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("MUGUET_SIGNING_IDENTITY", result.stdout)
        self.assertFalse(any(c["command"] == "cargo" for c in self.commands()))

    def test_ci_archive_requires_secrets_before_installing_tools(self):
        result = self.run_build(ci=True, env={"MUGUET_CI_MODE": "developer-archive"})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("developer archive requires", result.stdout)
        self.assertFalse(any(c["command"] in ("cargo", "rustup") for c in self.commands()))

    def test_ci_secret_download_requires_sha_without_logging_url(self):
        secret = "https://authorized.invalid/private.zip?token=keep-this-private"
        result = self.run_build(ci=True, env={"MUGUET_GAME_SOURCE": "secret-download", "MUGUET_GAME_URL": secret})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("MUGUET_GAME_SHA256", result.stdout)
        self.assertNotIn(secret, result.stdout)
        self.assertNotIn("keep-this-private", result.stdout)

    def test_ci_local_game_requires_self_hosted_runner(self):
        result = self.run_build(ci=True, env={"MUGUET_GAME_SOURCE": "self-hosted", "MUGUET_GAME_LOCAL_PATH": "/private/game.zip", "RUNNER_ENVIRONMENT": "github-hosted"})
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("self-hosted runner", result.stdout)

    def write_game_zip(self, *, backslashes=False):
        wasm = b"\0asm\1\0\0\0"
        files = {
            "data/manifest.json": json.dumps({"files": [["example.bin", 4, 0]]}),
            "data/example.bin": b"test",
            "b/test-build/game.wasm": wasm,
            "b/test-build/shaders/index.json": json.dumps({"0123456789abcdef": {"ok": True}}),
            "b/test-build/shaders/0123456789abcdef.wgsl": b"// synthetic fixture\n",
        }
        game = self.root / "synthetic-not-playable.zip"
        with zipfile.ZipFile(game, "w") as archive:
            for name, content in files.items():
                member = "export/" + name
                archive.writestr(member.replace("/", "\\") if backslashes else member, content)
        return game, wasm

    def test_validated_wrapped_zip_metadata_reaches_runtime_build(self):
        game, wasm = self.write_game_zip()
        result = self.run_build(str(game), "--ipa")
        self.assertEqual(result.returncode, 0, result.stdout)
        report = json.loads((self.root / "build/ios-ci/game-validation.json").read_text())
        self.assertTrue(report["valid"])
        self.assertFalse(report["game_runtime_verified"])
        runtime = [c for c in self.commands() if c["command"] == "cargo" and "aarch64-apple-ios" in c["args"]]
        self.assertEqual(runtime[0]["build_id"], "test-build")
        self.assertEqual(runtime[0]["wasm_sha256"], hashlib.sha256(wasm).hexdigest())
        precompiler = [c for c in self.commands() if c["command"] == "precompiler-input"]
        self.assertEqual(precompiler[0]["sha256"], hashlib.sha256(wasm).hexdigest())

    def test_validated_windows_zip_paths_extract_the_matching_code(self):
        game, wasm = self.write_game_zip(backslashes=True)
        result = self.run_build(str(game), "--ipa")
        self.assertEqual(result.returncode, 0, result.stdout)
        precompiler = [c for c in self.commands() if c["command"] == "precompiler-input"]
        self.assertEqual(precompiler[0]["sha256"], hashlib.sha256(wasm).hexdigest())

    def test_invalid_zip_fails_before_host_compilation(self):
        game = self.root / "invalid.zip"
        with zipfile.ZipFile(game, "w") as archive:
            archive.writestr("b/other/game.wasm", b"\0asm\1\0\0\0")
        result = self.run_build(str(game), "--ipa")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(c["command"] == "cargo" for c in self.commands()))

    def test_ci_no_game_records_package_checks_and_explicit_device_limit(self):
        developer = self.root / "Xcode/Contents/Developer"
        developer.mkdir(parents=True)
        result = self.run_build(ci=True, env={"DEVELOPER_DIR": str(developer), "RUNNER_TEMP": str(self.root)})
        self.assertEqual(result.returncode, 0, result.stdout)
        evidence = self.root / "build/ios-ci"
        self.assertEqual((evidence / "status.txt").read_text(), "success\n")
        self.assertIn("physical_device_tests=not-run", (evidence / "environment.txt").read_text())
        self.assertIn("27.2", (evidence / "app-macho-build.txt").read_text())
        self.assertTrue((self.root / "build/Muguet-Launcher-sideload.ipa").is_file())
        rustup = [c for c in self.commands() if c["command"] == "rustup"]
        self.assertIn("1.98.1", rustup[0]["args"])
        self.assertIn("aarch64-apple-ios", rustup[1]["args"])

    def xcodegen_download_env(self):
        developer = self.root / "Xcode/Contents/Developer"
        developer.mkdir(parents=True)
        real_shasum = shutil.which("shasum")
        self.assertIsNotNone(real_shasum)
        for name in ("curl", "shasum"):
            tool = self.root / "tools" / name
            tool.write_text(RECORDER)
            tool.chmod(0o755)
        github_path = self.root / "github-path.txt"
        github_path.write_text("")
        return dict(DEVELOPER_DIR=str(developer), RUNNER_TEMP=str(self.root), GITHUB_PATH=str(github_path),
                    BUILD_INSTALLED_XCODEGEN_VERSION="2.45.0", BUILD_REAL_SHASUM=real_shasum), github_path

    def test_ci_verified_xcodegen_survives_cleanup_for_the_next_workflow_process(self):
        env, github_path = self.xcodegen_download_env()
        result = self.run_build(ci=True, env=env)
        self.assertEqual(result.returncode, 0, result.stdout)
        directories = github_path.read_text().splitlines()
        self.assertEqual(len(directories), 1, "The verified executable directory must be handed to the next step")
        self.assertTrue((Path(directories[0]) / "xcodegen").is_file())
        self.assertFalse(any(self.root.glob("muguet-ios.*")), "Private build inputs must still be cleaned up")
        next_step_env = dict(self.env, **env)
        next_step_env["PATH"] = os.pathsep.join([*directories, self.env["PATH"]])
        next_step = subprocess.run(["xcodegen", "--version"], env=next_step_env, text=True,
                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        self.assertEqual(next_step.returncode, 0, next_step.stdout)
        self.assertEqual(next_step.stdout.strip(), "Version: 2.46.0")

    def test_ci_xcodegen_checksum_failure_cannot_publish_a_tool_directory(self):
        env, github_path = self.xcodegen_download_env()
        result = self.run_build(ci=True, env=dict(env, BUILD_XCODEGEN_BAD_CHECKSUM="1"))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("XcodeGen release checksum mismatch", result.stdout)
        self.assertEqual(github_path.read_text(), "")
        self.assertFalse(any(c["command"] == "cargo" for c in self.commands()))

    def test_ci_rejects_package_without_scene_configuration(self):
        developer = self.root / "Xcode/Contents/Developer"
        developer.mkdir(parents=True)
        result = self.run_build(ci=True, env={"DEVELOPER_DIR": str(developer), "RUNNER_TEMP": str(self.root),
                                              "BUILD_MISSING_SCENE_MANIFEST": "1"})
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("application package lacks a valid single UIWindowScene configuration", result.stdout)

    def test_ci_rejects_native_binary_without_scene_window_association(self):
        developer = self.root / "Xcode/Contents/Developer"
        developer.mkdir(parents=True)
        result = self.run_build(ci=True, env={"DEVELOPER_DIR": str(developer), "RUNNER_TEMP": str(self.root),
                                              "BUILD_MISSING_SCENE_BACKEND": "1"})
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn("application binary lacks UIKit window scene association", result.stdout)

    def test_project_vtool_uses_the_selected_sdk_version(self):
        text = (ROOT / "ios/project.yml").read_text()
        script = text.split("        script: |\n", 1)[1]
        script = "\n".join(line[10:] for line in script.splitlines())
        result = subprocess.run(["bash", "-c", script], cwd=self.root,
                                env=dict(self.env, TARGET_BUILD_DIR=str(self.root / "app"),
                                         EXECUTABLE_PATH="muguet", SRCROOT=str(self.root / "ios"),
                                         IPHONEOS_DEPLOYMENT_TARGET="15.0"),
                                text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        self.assertEqual(result.returncode, 0, result.stdout)
        vtool = [c for c in self.commands() if c["command"] == "xcrun" and "-set-build-version" in c["args"]]
        self.assertEqual(vtool[0]["args"][1:5], ["-set-build-version", "ios", "15.0", "27.2"])

    def test_developer_mode_rejects_an_ad_hoc_signature(self):
        env, _ = self.signed_fixture_env()
        result = self.run_build(ci=True, env=env)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("developer artifact signing team does not match", result.stdout)
        self.assertIn("failed", (self.root / "build/ios-ci/status.txt").read_text())
        self.assertFalse(any(self.root.glob("muguet-ios.*")))
        self.assertNotIn("synthetic-password", result.stdout)

    def test_failed_profile_backup_preserves_the_existing_profile(self):
        env, uuid = self.signed_fixture_env()
        profile = self.root / "home/Library/Developer/Xcode/UserData/Provisioning Profiles" / (uuid + ".mobileprovision")
        profile.parent.mkdir(parents=True)
        original = b"existing runner profile must survive a failed backup"
        profile.write_bytes(original)
        result = self.run_build(ci=True, env=dict(env, BUILD_FAIL_PROFILE_BACKUP="1"))
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(profile.exists(), "failed setup deleted a pre-existing profile")
        self.assertEqual(profile.read_bytes(), original)

    def test_signing_cleanup_restores_runner_keychain_search_list(self):
        env, _ = self.signed_fixture_env()
        result = self.run_build(ci=True, env=env)
        self.assertNotEqual(result.returncode, 0)
        keychain_lists = [c for c in self.commands() if c["command"] == "security" and "list-keychains" in c["args"] and "-s" in c["args"]]
        self.assertEqual(keychain_lists[-1]["args"], ["list-keychains", "-d", "user", "-s", str(self.root / "home/login.keychain-db")])

    def test_host_build_overrides_an_ambient_cross_compile_target(self):
        game = self.root / "synthetic.wasm"
        game.write_bytes(b"\0asm\1\0\0\0")
        result = self.run_build(str(game), "--ipa", env={"CARGO_BUILD_TARGET": "aarch64-apple-ios"})
        self.assertEqual(result.returncode, 0, result.stdout)
        cargo = [c for c in self.commands() if c["command"] == "cargo" and "build" in c["args"]]
        self.assertIn("--target", cargo[0]["args"])
        self.assertEqual(cargo[0]["args"][cargo[0]["args"].index("--target") + 1], "aarch64-apple-darwin")

    def test_ci_runs_locked_production_importer_tests_before_app_build(self):
        developer = self.root / "Xcode/Contents/Developer"
        developer.mkdir(parents=True)
        result = self.run_build(ci=True, env={"DEVELOPER_DIR": str(developer), "RUNNER_TEMP": str(self.root),
                                             "CARGO_BUILD_TARGET": "aarch64-apple-ios"})
        self.assertEqual(result.returncode, 0, result.stdout)
        commands = self.commands()
        importer = [(index, c) for index, c in enumerate(commands) if c["command"] == "cargo" and "test" in c["args"]]
        self.assertEqual(len(importer), 1)
        index, command = importer[0]
        self.assertEqual(command["args"][0], "+1.98.1")
        self.assertIn("--locked", command["args"])
        self.assertEqual(command["args"][command["args"].index("--manifest-path") + 1], "tests/import-harness/Cargo.toml")
        self.assertEqual(command["args"][command["args"].index("--target") + 1], "aarch64-apple-darwin")
        app_build = next(i for i, c in enumerate(commands) if c["command"] == "cargo" and "build" in c["args"])
        self.assertLess(index, app_build)
        log = (self.root / "build/ios-ci/importer-tests.log").read_text()
        self.assertIn("production importer test command recorded", log)

    def test_ci_importer_test_failure_blocks_app_build_and_retains_log(self):
        developer = self.root / "Xcode/Contents/Developer"
        developer.mkdir(parents=True)
        result = self.run_build(ci=True, env={"DEVELOPER_DIR": str(developer), "RUNNER_TEMP": str(self.root),
                                             "BUILD_FAIL_IMPORT_TESTS": "1"})
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(c["command"] == "cargo" and "build" in c["args"] for c in self.commands()))
        self.assertIn("synthetic importer test failure", (self.root / "build/ios-ci/importer-tests.log").read_text())
        self.assertIn("failed", (self.root / "build/ios-ci/status.txt").read_text())

    def test_ci_builds_and_packages_host_aot_compiler_without_game(self):
        developer = self.root / "Xcode/Contents/Developer"
        developer.mkdir(parents=True)
        result = self.run_build(ci=True, env={"DEVELOPER_DIR": str(developer), "RUNNER_TEMP": str(self.root),
                                             "CARGO_BUILD_TARGET": "aarch64-apple-ios", "SDKROOT": "/ambient-ios-sdk"})
        self.assertEqual(result.returncode, 0, result.stdout)
        commands = self.commands()
        host = [c for c in commands if c["command"] == "cargo" and "build" in c["args"]
                and "target/ios-precompiler" in c["args"]]
        self.assertEqual(len(host), 1)
        self.assertEqual(host[0]["args"][0], "+1.98.1")
        self.assertIn("--locked", host[0]["args"])
        self.assertNotIn("--no-default-features", host[0]["args"])
        self.assertEqual(host[0]["args"][host[0]["args"].index("--target") + 1], "aarch64-apple-darwin")
        self.assertIn("MacOSX.platform", host[0]["sdkroot"])
        self.assertIsNone(host[0]["game_code"])
        self.assertFalse(any(c["command"] == "muguet" for c in commands))
        archive = self.root / "build/Muguet-AOT-compiler-macos-arm64.zip"
        with zipfile.ZipFile(archive) as package:
            self.assertEqual(set(package.namelist()), {"muguet", "LICENSE", "README.txt", "SHA256SUMS"})
            self.assertIsNone(package.testzip())
            digest = hashlib.sha256(package.read("muguet")).hexdigest()
            self.assertIn(digest, package.read("SHA256SUMS").decode())
            self.assertIn("No game code", package.read("README.txt").decode())

    def test_ci_host_compiler_failure_retains_log_and_blocks_app_build(self):
        developer = self.root / "Xcode/Contents/Developer"
        developer.mkdir(parents=True)
        result = self.run_build(ci=True, env={"DEVELOPER_DIR": str(developer), "RUNNER_TEMP": str(self.root),
                                             "BUILD_FAIL_HOST_COMPILER": "1"})
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(c["command"] == "xcodebuild" and "build" in c["args"] for c in self.commands()))
        self.assertIn("synthetic host compiler build failure", (self.root / "build/ios-ci/host-compiler.log").read_text())
        self.assertIn("failed", (self.root / "build/ios-ci/status.txt").read_text())

    def test_device_exports_require_udid_before_installing_tools(self):
        for method in ("debugging", "release-testing"):
            with self.subTest(method=method):
                self.log.unlink(missing_ok=True)
                env, _ = self.signed_fixture_env()
                env.pop("MUGUET_DEVICE_UDID")
                env["MUGUET_EXPORT_METHOD"] = method
                result = self.run_build(ci=True, env=env)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("requires MUGUET_DEVICE_UDID", result.stdout)
                self.assertFalse(any(c["command"] == "rustup" for c in self.commands()))

    def test_device_exports_reject_udid_missing_from_profile(self):
        for method in ("debugging", "release-testing"):
            with self.subTest(method=method):
                self.log.unlink(missing_ok=True)
                env, _ = self.signed_fixture_env()
                env["MUGUET_EXPORT_METHOD"] = method
                env["MUGUET_DEVICE_UDID"] = "PRIVATE-DEVICE-NOT-ENROLLED"
                profile_path = Path(env["BUILD_PROFILE_PLIST"])
                profile = plistlib.loads(profile_path.read_bytes())
                profile["Entitlements"]["get-task-allow"] = method == "debugging"
                profile_path.write_bytes(plistlib.dumps(profile))
                result = self.run_build(ci=True, env=env)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("MUGUET_DEVICE_UDID is not enrolled", result.stdout)
                self.assertNotIn("PRIVATE-DEVICE-NOT-ENROLLED", result.stdout)
                self.assertFalse(any(c["command"] == "xcodebuild" and "archive" in c["args"] for c in self.commands()))

    def test_app_store_export_accepts_a_profile_without_device_udid(self):
        env, _ = self.signed_fixture_env()
        env.pop("MUGUET_DEVICE_UDID")
        env["MUGUET_EXPORT_METHOD"] = "app-store-connect"
        profile_path = Path(env["BUILD_PROFILE_PLIST"])
        profile = plistlib.loads(profile_path.read_bytes())
        profile.pop("ProvisionedDevices")
        profile["Entitlements"]["get-task-allow"] = False
        profile_path.write_bytes(plistlib.dumps(profile))
        result = self.run_build(ci=True, env=env)
        # Signing is deliberately a recorder, so the eventual signature check
        # fails. This assertion proves that no device gate blocks App Store export.
        self.assertIn("developer artifact signing team does not match", result.stdout)
        self.assertTrue(any(c["command"] == "xcodebuild" and "archive" in c["args"] for c in self.commands()))

    def test_workflow_uses_the_user_bundle_id_default(self):
        workflow = (ROOT / ".github/workflows/ios-build.yml").read_text()
        bundle = next(line for line in workflow.splitlines() if line.strip().startswith("BUNDLE_ID:"))
        self.assertIn("'com.qeyf.muguet'", bundle)

    def test_ci_records_build_and_private_input_disk_capacity(self):
        developer = self.root / "Xcode/Contents/Developer"
        developer.mkdir(parents=True)
        result = self.run_build(ci=True, env={"DEVELOPER_DIR": str(developer), "RUNNER_TEMP": str(self.root)})
        self.assertEqual(result.returncode, 0, result.stdout)
        capacity = self.root / "build/ios-ci/capacity.json"
        self.assertTrue(capacity.is_file(), "CI evidence lacks actual disk-capacity measurements")
        report = json.loads(capacity.read_text())
        self.assertGreater(report["workspace_free_bytes"], 0)
        self.assertGreater(report["private_input_free_bytes"], 0)
        self.assertEqual(report["reserved_build_bytes"], 2 << 30)


if __name__ == "__main__":
    unittest.main()
