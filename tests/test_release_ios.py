"""Release policy tests with synthetic ZIPs; no signing or game is simulated."""
import hashlib
import json
import os
from pathlib import Path
import plistlib
import subprocess
import tempfile
import textwrap
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]


class IOSReleasePolicy(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.package = self.root / "package"
        self.package.mkdir()
        self.evidence = self.root / "evidence"
        (self.evidence / "simulator").mkdir(parents=True)
        (self.evidence / "status.txt").write_text("success\n")
        (self.evidence / "simulator/status.txt").write_text("passed\n")
        (self.evidence / "simulator/test-summary.json").write_text(json.dumps(
            {"totalTestCount": 1, "passedTests": 1, "failedTests": 0, "skippedTests": 0}))
        (self.evidence / "simulator/selection.json").write_text(json.dumps(
            {"runtime_version": "27.2", "runtime_build": "SYNTHETIC", "device_type_name": "Synthetic iPhone"}))
        (self.evidence / "environment.txt").write_text(
            "source_revision=" + "a" * 40 + "\nmode=sideload\ngame_source=none\niphoneos_sdk=27.2\n")
        self.ipa = self.package / "Muguet-Launcher-sideload.ipa"
        self.write_ipa()
        with zipfile.ZipFile(self.package / "Muguet-AOT-compiler-macos-arm64.zip", "w") as archive:
            archive.writestr("fixture.txt", "Synthetic compiler package; no executable")
        self.output = self.root / "release"
        self.env = dict(os.environ, GITHUB_SHA="a" * 40, GITHUB_REPOSITORY="Qeyf/muguet-ios",
                        GITHUB_RUN_ID="123", BUNDLE_ID="com.qeyf.muguet")

    def tearDown(self):
        self.temp.cleanup()

    def write_ipa(self, profile=False):
        info = {"CFBundleIdentifier": "com.qeyf.muguet", "CFBundleShortVersionString": "0.1.1", "CFBundleVersion": "2"}
        with zipfile.ZipFile(self.ipa, "w") as archive:
            archive.writestr("Payload/Muguet.app/Info.plist", plistlib.dumps(info))
            archive.writestr("Payload/Muguet.app/muguet", "Synthetic ZIP fixture; not a native application")
            if profile:
                archive.writestr("Payload/Muguet.app/embedded.mobileprovision", "Synthetic private profile")
        (self.evidence / "artifact-sha256.txt").write_text(
            hashlib.sha256(self.ipa.read_bytes()).hexdigest() + "  build/Muguet-Launcher-sideload.ipa\n")

    def prepare(self):
        helper = ROOT / "scripts/prepare-ios-release.py"
        self.assertTrue(helper.is_file(), "Release packaging policy is missing")
        return subprocess.run(["python3", str(helper), str(self.package), str(self.evidence), str(self.output)],
                              env=self.env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

    def test_valid_release_preserves_ipa_bytes_version_and_physical_test_limit(self):
        result = self.prepare()
        self.assertEqual(result.returncode, 0, result.stdout)
        published = self.output / "Muguet-Launcher-v0.1.1-sideload.ipa"
        self.assertEqual(published.read_bytes(), self.ipa.read_bytes())
        self.assertEqual((self.output / "tag.txt").read_text().strip(), "v0.1.1")
        report = (self.output / "TEST_REPORT.md").read_text()
        self.assertIn("Physical iPhone retest: pending", report)
        self.assertIn("Game code/assets: absent", report)
        self.assertIn("actions/runs/123", report)
        sums = (self.output / "SHA256SUMS").read_text()
        self.assertIn(hashlib.sha256(published.read_bytes()).hexdigest() + "  " + published.name, sums)

    def test_failed_or_skipped_native_ui_test_blocks_release(self):
        (self.evidence / "simulator/test-summary.json").write_text(json.dumps(
            {"totalTestCount": 1, "passedTests": 0, "failedTests": 0, "skippedTests": 1}))
        result = self.prepare()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("native launcher UI test", result.stdout)
        self.assertFalse(self.output.exists())


    def test_changed_ipa_cannot_reuse_runner_hash_evidence(self):
        self.ipa.write_bytes(self.ipa.read_bytes() + b"changed")
        result = self.prepare()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("IPA SHA-256", result.stdout)
        self.assertFalse(self.output.exists())

    def test_private_profile_or_game_build_cannot_enter_public_launcher_release(self):
        self.write_ipa(profile=True)
        result = self.prepare()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("provisioning profile", result.stdout)
        self.assertFalse(self.output.exists())
        self.write_ipa()
        with (self.evidence / "environment.txt").open("a") as output:
            output.write("game_source=secret-download\n")
        result = self.prepare()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("launcher-only sideload", result.stdout)
        self.assertFalse(self.output.exists())


class ReleaseTagPolicy(unittest.TestCase):
    def test_existing_orphan_tag_blocks_publication_without_replacing_it(self):
        self.run_publish(existing_tag=True)

    def test_new_release_creates_exact_source_tag_before_publishing(self):
        self.run_publish(existing_tag=False)

    def run_publish(self, existing_tag):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / 'release-output').mkdir()
            (root / 'release-output/tag.txt').write_text('v0.1.1\n')
            commands = root / 'commands.jsonl'
            gh = root / 'gh'
            gh.write_text('''#!/usr/bin/env python3
import json, os, sys
args = sys.argv[1:]
with open(os.environ['RELEASE_COMMANDS'], 'a') as output:
    output.write(json.dumps(args) + '\\n')
if args[:2] == ['release', 'view']:
    sys.exit(1) # Synthetic absent release; a Git tag may still exist.
if args[:1] == ['api'] and os.environ['RELEASE_EXISTING_TAG'] == '1':
    sys.exit('Synthetic GitHub 422: Reference already exists')
''')
            gh.chmod(0o755)
            workflow = (ROOT / '.github/workflows/ios-build.yml').read_text()
            script = textwrap.dedent(workflow.split('      - name: Publish once through official GitHub CLI\n', 1)[1].split('        run: |\n', 1)[1])
            env = dict(os.environ, PATH=str(root) + os.pathsep + os.environ['PATH'], GH_REPO='Qeyf/muguet-ios',
                       GITHUB_SHA='a' * 40, RELEASE_COMMANDS=str(commands), RELEASE_EXISTING_TAG=str(int(existing_tag)))
            result = subprocess.run(['bash', '-c', script], cwd=root, env=env, text=True,
                                    stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
            recorded = [json.loads(line) for line in commands.read_text().splitlines()]
            creates = [args for args in recorded if args[:2] == ['release', 'create']]
            if existing_tag:
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertEqual(creates, [], 'An existing source tag must never receive newly built assets')
            else:
                self.assertEqual(result.returncode, 0, result.stdout)
                tags = [args for args in recorded if args[:1] == ['api']]
                self.assertEqual(len(tags), 1, 'The exact source tag must be created explicitly')
                tag = tags[0]
                self.assertIn('ref=refs/tags/v0.1.1', tag)
                self.assertIn('sha=' + 'a' * 40, tag)
                self.assertIn('--verify-tag', creates[0])
