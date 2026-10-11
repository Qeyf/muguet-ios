#!/usr/bin/env python3
"""Inspect an authorized Muguet export without extracting or executing it.

Success means inventory/format checks passed, not that this is licensed GTA V
or that Wasmtime, Metal or the game has run. Real compilation is a later gate.
"""

import argparse
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys
import unicodedata
import zipfile

MAX_FILES = 250_000
MAX_TOTAL = 128 << 30
MAX_FILE = 32 << 30
MAX_JSON = 32 << 20
SKIPPED = {"env_test", "verityradio"}


class InvalidExport(ValueError):
    pass


def safe_path(name):
    if not isinstance(name, str) or not name:
        raise InvalidExport("invalid path")
    normalized = name.replace("\\", "/")
    normalized = normalized[:-1] if normalized.endswith("/") else normalized
    parts = normalized.split("/")
    if (len(normalized.encode("utf-8")) > 1024 or normalized.startswith("/")
            or any(p in {"", ".", ".."} or ":" in p or len(p.encode("utf-8")) > 255
                   or any(unicodedata.category(c) == "Cc" for c in p) for p in parts)):
        raise InvalidExport(f"unsafe path: {name!r}")
    return "/".join(parts)


def digest(stream):
    sha = hashlib.sha256()
    count = 0
    for chunk in iter(lambda: stream.read(1 << 20), b""):
        count += len(chunk)
        if count > MAX_FILE:
            raise InvalidExport("file exceeds 32 GiB limit")
        sha.update(chunk)
    return sha.hexdigest(), count


class Inventory:
    def __init__(self, source):
        self.source = source
        self.archive = None
        self.entries = {}
        self.names = set()
        self.sizes = {}
        self.paths = {}
        self.count = 0
        self.total = 0

    def add(self, name, entry, size, directory=False):
        name = safe_path(name)
        self.count += 1
        if self.count > MAX_FILES:
            raise InvalidExport("export exceeds 250,000 entries")
        parts = name.split("/")
        for length in range(1, len(parts) + 1):
            path = "/".join(parts[:length])
            key = unicodedata.normalize("NFD", path).casefold()
            is_directory = length != len(parts) or directory
            explicit = length == len(parts)
            previous = self.paths.get(key)
            if previous:
                spelling, was_directory, was_explicit = previous
                if (spelling != path or was_directory != is_directory
                        or (explicit and was_explicit and not is_directory)):
                    raise InvalidExport(f"duplicate or conflicting file path: {path}")
                explicit = explicit or was_explicit
            self.paths[key] = (path, is_directory, explicit)
        self.names.add(name)
        if directory:
            return
        if size > MAX_FILE:
            raise InvalidExport(f"file exceeds 32 GiB limit: {name}")
        self.total += size
        if self.total > MAX_TOTAL:
            raise InvalidExport("export exceeds file-count/128 GiB size limit")
        self.entries[name] = entry
        self.sizes[name] = size

    def __enter__(self):
        if self.source.is_symlink():
            raise InvalidExport("source is a symlink")
        if self.source.is_dir():
            for root, dirs, files in os.walk(self.source, followlinks=False):
                for name in dirs + files:
                    path = Path(root) / name
                    if path.is_symlink():
                        raise InvalidExport(f"symlink path: {path.relative_to(self.source)}")
                for name in dirs:
                    path = Path(root) / name
                    self.add(path.relative_to(self.source).as_posix(), path, 0, directory=True)
                for name in files:
                    path = Path(root) / name
                    info = path.stat()
                    if not stat.S_ISREG(info.st_mode):
                        raise InvalidExport("non-regular file in export")
                    self.add(path.relative_to(self.source).as_posix(), path, info.st_size)
        else:
            self.archive = zipfile.ZipFile(self.source)
            try:
                if len(self.archive.infolist()) > MAX_FILES:
                    raise InvalidExport("ZIP exceeds 250,000 entries")
                for info in self.archive.infolist():
                    safe_path(info.filename)
                    mode = info.external_attr >> 16
                    if stat.S_ISLNK(mode) or (stat.S_IFMT(mode) not in {0, stat.S_IFREG, stat.S_IFDIR}):
                        raise InvalidExport(f"symlink/non-regular ZIP path: {info.filename}")
                    if info.flag_bits & 1:
                        raise InvalidExport("encrypted ZIP entries are unsupported")
                    if info.compress_type not in {zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED}:
                        raise InvalidExport("only stored/deflate ZIP compression is supported")
                    self.add(info.filename, info, info.file_size, directory=info.is_dir())
            except Exception:
                self.archive.close()
                raise
        return self

    def __exit__(self, *_):
        if self.archive:
            self.archive.close()

    @contextmanager
    def open(self, name):
        if name not in self.entries:
            raise InvalidExport(f"missing file: {name}")
        if self.archive:
            with self.archive.open(self.entries[name]) as stream:
                yield stream
        else:
            # Reject a replaced symlink rather than following it between checks.
            flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
            with os.fdopen(os.open(self.entries[name], flags), "rb") as stream:
                yield stream

    def json(self, name):
        if self.sizes.get(name, 0) > MAX_JSON:
            raise InvalidExport("JSON metadata exceeds 32 MiB limit")
        with self.open(name) as stream:
            return json.load(stream)


def inspect(source, require_code=False, expected_build_id=None, expected_wasm_sha256=None):
    warnings = ["Inventory validation does not establish a license or successful game execution.",
                "Full WASM validation, ABI/name-section compatibility and Metal shader compilation require the upstream build/runtime."]
    with Inventory(source) as inv:
        manifests = [n for n in inv.entries if n == "data/manifest.json" or n.endswith("/data/manifest.json")]
        if len(manifests) != 1:
            raise InvalidExport("expected exactly one data/manifest.json root")
        manifest_name = manifests[0]
        prefix = manifest_name[:-len("data/manifest.json")]
        if source.is_dir() and prefix:
            raise InvalidExport("select the folder directly containing data/ and b/")
        names = {n[len(prefix):]: n for n in inv.entries if n.startswith(prefix)}
        # Validate after prefix removal, not only against the ZIP's outer root.
        for rel in names:
            safe_path(rel)
        builds = {m.group(1) for n in names if (m := re.fullmatch(r"b/([^/]+)/shaders/index\.json", n))}
        if len(builds) != 1:
            raise InvalidExport("expected exactly one build with shaders/index.json")
        build = next(iter(builds))
        if expected_build_id and build != expected_build_id:
            raise InvalidExport("build ID does not match the compiled application")
        for name in inv.names:
            if not name.startswith(prefix):
                continue
            rel = name[len(prefix):]
            # An explicit empty directory is still a competing build.
            # Regular metadata files immediately under b/ are not builds.
            if name in inv.entries and "/" not in rel[2:]:
                continue
            match = re.match(r"b/([^/]+)(?:/|$)", rel)
            if match and match.group(1) != build:
                raise InvalidExport("multiple game build directories are ambiguous")
        manifest = inv.json(manifest_name)
        rows = manifest.get("files") if isinstance(manifest, dict) else None
        if not isinstance(rows, list) or not rows:
            raise InvalidExport("manifest.files must be a non-empty array")
        refs = set()
        skipped_refs = 0
        for row in rows:
            if not isinstance(row, list) or len(row) < 2 or not isinstance(row[0], str):
                raise InvalidExport("invalid manifest file row")
            rel = safe_path(row[0])
            size = row[1]
            if isinstance(size, bool) or not isinstance(size, int) or size < 0 or size > MAX_FILE:
                raise InvalidExport("invalid manifest file size")
            key = unicodedata.normalize("NFD", rel).casefold()
            if key in refs:
                raise InvalidExport("duplicate manifest path")
            refs.add(key)
            if any(c.lower() in SKIPPED for c in rel.split("/")):
                skipped_refs += 1
                continue
            path = prefix + "data/" + rel
            if path not in inv.entries:
                raise InvalidExport(f"missing manifest reference: {rel}")
            if inv.sizes[path] != size:
                raise InvalidExport(f"manifest size mismatch: {rel}")
        if skipped_refs == len(rows):
            raise InvalidExport("manifest has no required assets after intentional skips")

        shader_root = f"b/{build}/shaders/"
        shader_index = inv.json(prefix + shader_root + "index.json")
        if not isinstance(shader_index, dict):
            raise InvalidExport("shader index must be a JSON object")
        shader_count = 0
        unavailable = 0
        for shader, record in shader_index.items():
            if shader.startswith("_"):
                continue
            if not re.fullmatch(r"[0-9a-fA-F]{16}", shader) or not isinstance(record, dict):
                raise InvalidExport("invalid shader index record")
            if record.get("ok") is not True:
                unavailable += 1
                continue
            shader_count += 1
            name = prefix + shader_root + shader + ".wgsl"
            if not inv.sizes.get(name):
                raise InvalidExport(f"missing/empty shader source: {shader}")
            if record.get("consts") is not None and record.get("consts") is not False:
                consts = inv.json(prefix + shader_root + shader + ".consts.json")
                if not isinstance(consts, list):
                    raise InvalidExport(f"invalid shader constants: {shader}")
        if not shader_count:
            raise InvalidExport("shader index has no usable shader records")
        if unavailable:
            warnings.append(f"{unavailable} shader records are marked unavailable by the producer; rendering remains unverified.")

        wasm_name = prefix + f"b/{build}/game.wasm"
        wasm_hash = None
        wasm_size = None
        if wasm_name in inv.entries:
            with inv.open(wasm_name) as stream:
                if stream.read(8) != b"\x00asm\x01\x00\x00\x00":
                    raise InvalidExport("game.wasm is not a WebAssembly v1 binary")
            wasm_size = inv.sizes[wasm_name]
        elif require_code:
            raise InvalidExport("missing b/<same-build>/game.wasm")
        hashes = {}
        selected_bytes = 0
        import_bytes = 0
        for rel, name in sorted(names.items()):
            if any(c.lower() in SKIPPED for c in rel.split("/")):
                continue
            wanted = rel.startswith("data/") or rel.startswith(shader_root) or name == wasm_name
            if not wanted:
                continue
            with inv.open(name) as stream:
                sha, count = digest(stream)  # Reading to EOF also checks ZIP CRC.
            if count != inv.sizes[name]:
                raise InvalidExport(f"changed/truncated file: {rel}")
            hashes[rel] = sha
            selected_bytes += count
            if name == wasm_name:
                wasm_hash = sha
            else:
                import_bytes += count
        if expected_wasm_sha256 and wasm_hash != expected_wasm_sha256.lower():
            raise InvalidExport("WASM SHA-256 does not match the expected build")
        if source.is_file():
            with source.open("rb") as stream:
                archive_sha, archive_bytes = digest_archive(stream)
        else:
            archive_sha, archive_bytes = None, 0
        return {
            "valid": True, "validation_scope": "inventory, paths, sizes, CRC, SHA-256 and WASM header only",
            "game_runtime_verified": False, "license_verified": False,
            "build_id": build, "prefix": prefix, "file_count": len(hashes),
            "manifest_reference_count": len(rows), "skipped_manifest_references": skipped_refs,
            "shader_count": shader_count, "unavailable_shader_count": unavailable,
            "wasm_sha256": wasm_hash, "wasm_bytes": wasm_size,
            "archive_sha256": archive_sha, "archive_bytes": archive_bytes,
            "selected_bytes": selected_bytes, "ios_import_bytes": import_bytes,
            "minimum_free_import_bytes_with_margin": import_bytes + (16 << 20),
            "storage_with_source_zip_bytes": archive_bytes + import_bytes,
            "file_sha256": hashes, "warnings": warnings, "errors": [],
        }


def digest_archive(stream):
    sha = hashlib.sha256()
    count = 0
    for chunk in iter(lambda: stream.read(1 << 20), b""):
        count += len(chunk)
        sha.update(chunk)
    return sha.hexdigest(), count


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("--require-code", action="store_true")
    parser.add_argument("--report", type=Path)
    parser.add_argument("--expected-build-id")
    parser.add_argument("--expected-wasm-sha256")
    args = parser.parse_args()
    try:
        report = inspect(args.source, args.require_code, args.expected_build_id, args.expected_wasm_sha256)
    except (InvalidExport, OSError, ValueError, KeyError, zipfile.BadZipFile, RuntimeError, EOFError) as error:
        report = {"valid": False, "game_runtime_verified": False, "license_verified": False, "errors": [str(error)]}
    result = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(result)
    print(result, end="")
    return 0 if report["valid"] else 1


if __name__ == "__main__":
    sys.exit(main())
