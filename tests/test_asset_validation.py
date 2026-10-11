"""Synthetic archive security fixtures; none of these is a playable game."""

import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import zipfile

SCRIPT = Path(__file__).resolve().parents[1] / "scripts/validate_game_assets.py"
BUILD = "test-build"
SHADER = "0123456789abcdef"
WASM = b"\x00asm\x01\x00\x00\x00"  # An empty module; explicitly NOT GTA V.


def entries():
    return {
        "data/manifest.json": json.dumps({"files": [["example.bin", 4, 0]]}).encode(),
        "data/example.bin": b"test",
        f"b/{BUILD}/game.wasm": WASM,
        f"b/{BUILD}/shaders/index.json": json.dumps({SHADER: {"ok": True}}).encode(),
        f"b/{BUILD}/shaders/{SHADER}.wgsl": b"// synthetic fixture\n",
    }


class AssetValidationTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.addCleanup(self.tmp.cleanup)

    def check_archive(self, files, *, expected=0, prefix="", extra=None):
        archive = self.root / "fixture.zip"
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as out:
            for name, data in files.items():
                out.writestr(prefix + name, data)
            if extra:
                for name, data in extra:
                    out.writestr(name, data)
        report = self.root / "report.json"
        run = subprocess.run(
            [sys.executable, str(SCRIPT), str(archive), "--require-code", "--report", str(report)],
            text=True, capture_output=True,
        )
        self.assertEqual(run.returncode, expected, run.stdout + run.stderr)
        return json.loads(report.read_text())

    def test_reports_hashes_without_claiming_gameplay_or_license(self):
        report = self.check_archive(entries())
        self.assertEqual(report["build_id"], BUILD)
        self.assertEqual(report["wasm_sha256"], hashlib.sha256(WASM).hexdigest())
        self.assertFalse(report["game_runtime_verified"])
        self.assertFalse(report["license_verified"])
        self.assertEqual(report["file_count"], 5)

    def test_wrapped_export_is_supported(self):
        report = self.check_archive(entries(), prefix="export/")
        self.assertEqual(report["prefix"], "export/")

    def test_rejects_wrapped_parent_traversal(self):
        report = self.check_archive(entries(), expected=1, prefix="export/",
                                    extra=[("export/data/../../outside", b"bad")])
        self.assertTrue(any("path" in e.lower() for e in report["errors"]))

    def test_rejects_backslash_traversal(self):
        self.check_archive(entries(), expected=1,
                           extra=[("data/..\\..\\outside", b"bad")])

    def test_rejects_symlink(self):
        files = entries()
        archive = self.root / "symlink.zip"
        with zipfile.ZipFile(archive, "w") as out:
            for name, data in files.items():
                out.writestr(name, data)
            link = zipfile.ZipInfo("data/link")
            link.create_system = 3
            link.external_attr = (0o120777 << 16)
            out.writestr(link, "../../outside")
        run = subprocess.run([sys.executable, str(SCRIPT), str(archive), "--require-code"],
                             text=True, capture_output=True)
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)

    def test_rejects_case_collision(self):
        self.check_archive(entries(), expected=1, extra=[("data/EXAMPLE.bin", b"test")])

    def test_rejects_case_colliding_ancestor_directories(self):
        self.check_archive(entries(), expected=1,
                           extra=[("data/Foo/a", b"a"), ("data/foo/b", b"b")])

    def test_rejects_file_directory_conflict(self):
        self.check_archive(entries(), expected=1,
                           extra=[("data/dir", b"a"), ("data/dir/x", b"b")])

    def test_rejects_empty_second_build_directory(self):
        self.check_archive(entries(), expected=1, extra=[("b/other/", b"")])

    def test_rejects_control_character_in_path(self):
        self.check_archive(entries(), expected=1, extra=[("data/line\nname", b"a")])

    def test_rejects_overlong_path_component(self):
        self.check_archive(entries(), expected=1, extra=[("data/" + "a" * 256, b"a")])

    def test_rejects_only_skipped_manifest_references(self):
        files = entries()
        files["data/manifest.json"] = json.dumps(
            {"files": [["env_test/ignored", 1, 0]]}).encode()
        self.check_archive(files, expected=1)

    def test_rejects_repeated_trailing_separator(self):
        self.check_archive(entries(), expected=1, extra=[("data/extra//", b"")])

    def test_accepts_explicit_directories_after_their_children(self):
        self.check_archive(entries(), extra=[("data/", b""), (f"b/{BUILD}/", b"")])

    def test_ignores_regular_metadata_file_directly_under_builds_root(self):
        self.check_archive(entries(), extra=[("b/.DS_Store", b"metadata")])

    def test_rejects_missing_manifest_reference(self):
        files = entries()
        del files["data/example.bin"]
        self.check_archive(files, expected=1)

    def test_rejects_size_mismatch(self):
        files = entries()
        files["data/example.bin"] = b"incorrect"
        self.check_archive(files, expected=1)

    def test_rejects_malformed_manifest(self):
        files = entries()
        files["data/manifest.json"] = b'{"files":"wrong"}'
        self.check_archive(files, expected=1)

    def test_rejects_missing_shader(self):
        files = entries()
        del files[f"b/{BUILD}/shaders/{SHADER}.wgsl"]
        self.check_archive(files, expected=1)

    def test_rejects_missing_shader_constants(self):
        files = entries()
        files[f"b/{BUILD}/shaders/index.json"] = json.dumps(
            {SHADER: {"ok": True, "consts": True}}).encode()
        self.check_archive(files, expected=1)

    def test_rejects_ambiguous_builds(self):
        files = entries()
        files["b/other/shaders/index.json"] = b"{}"
        self.check_archive(files, expected=1)

    def test_rejects_non_wasm_code(self):
        files = entries()
        files[f"b/{BUILD}/game.wasm"] = b"not a wasm module"
        self.check_archive(files, expected=1)

    def test_directory_has_same_inventory(self):
        folder = self.root / "export"
        for name, data in entries().items():
            path = folder / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        run = subprocess.run([sys.executable, str(SCRIPT), str(folder), "--require-code"],
                             text=True, capture_output=True)
        self.assertEqual(run.returncode, 0, run.stderr + run.stdout)
        self.assertEqual(json.loads(run.stdout)["build_id"], BUILD)

    def test_directory_ignores_regular_metadata_under_builds_root(self):
        folder = self.root / "export"
        for name, data in dict(entries(), **{"b/.DS_Store": b"metadata"}).items():
            path = folder / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        run = subprocess.run([sys.executable, str(SCRIPT), str(folder), "--require-code"],
                             text=True, capture_output=True)
        self.assertEqual(run.returncode, 0, run.stderr + run.stdout)


if __name__ == "__main__":
    unittest.main()
