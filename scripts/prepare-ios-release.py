"""Package the same run's verified launcher for GitHub Releases (no upload)."""
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import sys
import zipfile


def prepare(package, evidence, output):
    def require(condition, message):
        if not condition:
            raise ValueError(message)

    facts = dict(line.split("=", 1) for line in (evidence / "environment.txt").read_text().splitlines() if "=" in line)
    require(facts.get("mode") == "sideload" and facts.get("game_source") == "none", "Only launcher-only sideload runs may be released publicly")
    require(facts.get("source_revision") == os.environ["GITHUB_SHA"], "Release source differs from the verified native run")
    require((evidence / "status.txt").read_text().strip() == "success", "Native device build did not pass")
    require((evidence / "simulator/status.txt").read_text().strip() == "passed", "The native launcher UI test did not pass")
    summary = json.loads((evidence / "simulator/test-summary.json").read_text())
    require(summary.get("totalTestCount") == summary.get("passedTests") == 1 and summary.get("failedTests") == summary.get("skippedTests") == 0,
            "The native launcher UI test must pass exactly once without skips")
    ipa = package / "Muguet-Launcher-sideload.ipa"
    digest = hashlib.sha256(ipa.read_bytes()).hexdigest()
    recorded = (evidence / "artifact-sha256.txt").read_text().splitlines()
    require(any(row.split() == [digest, "build/Muguet-Launcher-sideload.ipa"] for row in recorded), "IPA SHA-256 differs from native runner evidence")
    with zipfile.ZipFile(ipa) as archive:
        require(archive.testzip() is None, "IPA ZIP integrity failed")
        require(not any(name.endswith("/embedded.mobileprovision") for name in archive.namelist()), "Public sideload IPA must not include a provisioning profile")
        info = plistlib.loads(archive.read("Payload/Muguet.app/Info.plist"))
    version = info["CFBundleShortVersionString"]
    require(re.fullmatch(r"\d+\.\d+\.\d+", version), "Release app version must have three numeric components")
    require(info["CFBundleIdentifier"] == os.environ.get("BUNDLE_ID", "com.qeyf.muguet"), "Unexpected release bundle identifier")
    require(not output.exists(), "Release output already exists; do not overwrite a prepared version")
    compiler = package / "Muguet-AOT-compiler-macos-arm64.zip"
    with zipfile.ZipFile(compiler) as archive:
        require(archive.testzip() is None, "Compiler ZIP integrity failed")
    selection = json.loads((evidence / "simulator/selection.json").read_text())
    tag = "v" + version
    run = f"https://github.com/{os.environ['GITHUB_REPOSITORY']}/actions/runs/{os.environ['GITHUB_RUN_ID']}"
    report = (
        f"# Muguet {tag} launcher — build {info['CFBundleVersion']}\n\n"
        f"Native build and UI-test evidence: {run}\nSource: `{os.environ['GITHUB_SHA']}`\n\n"
        f"- iOS SDK: {facts['iphoneos_sdk']}; arm64 device compilation and strict self-signature checks passed.\n"
        f"- Simulator: {selection['device_type_name']}, iOS {selection['runtime_version']} ({selection.get('runtime_build')}).\n"
        "- Native UI test: visible launcher, ZIP import button and disabled Play; background/reactivation passed.\n"
        "- Physical iPhone retest: pending. No physical-device success is inferred from simulator results.\n"
        "- Game code/assets: absent. The launcher and Mac compiler have not run GTA V.\n"
        "- IPA signature: ad-hoc self-signature; re-sign through LCSign with your own eligible profile.\n\n"
        "This prerelease addresses the supplied UIKit no-scene-lifecycle startup assertion with a single UIWindowScene and the pinned winit contributor backport.\n"
        "It is not a released upstream winit fix. See docs/IOS27_SCENE_FIX.md at this tag for the source audit.\n\n"
        "Keep prior installed game data backed up before replacing an application. Re-sign this version, install it, and retest launch and background recovery on the actual iPhone.\n"
    )
    output.mkdir(parents=True)
    shutil.copyfile(ipa, output / f"Muguet-Launcher-{tag}-sideload.ipa")
    shutil.copyfile(compiler, output / f"Muguet-AOT-compiler-{tag}-macos-arm64.zip")
    with zipfile.ZipFile(output / f"Muguet-build-evidence-{tag}.zip", "w", zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(evidence.rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(evidence).as_posix())
    (output / "TEST_REPORT.md").write_text(report)
    (output / "SHA256SUMS").write_text("".join(hashlib.sha256(path.read_bytes()).hexdigest() + "  " + path.name + "\n" for path in sorted(output.iterdir())))
    (output / "tag.txt").write_text(tag + "\n")
    print(tag)


if __name__ == "__main__":
    try:
        prepare(*(Path(value) for value in sys.argv[1:4]))
    except (OSError, ValueError, KeyError, zipfile.BadZipFile) as error:
        sys.exit(f"Release preparation rejected: {error}")
