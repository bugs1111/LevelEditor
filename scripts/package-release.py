#!/usr/bin/env python3
"""Create native release ZIPs and the updater's checksummed target metadata."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
VERSION = re.search(r'^version = "([^"]+)"', (ROOT / "Cargo.toml").read_text(), re.M)[1]
TARGETS = {"aarch64-apple-darwin", "x86_64-apple-darwin",
           "x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"}


def package(target, destination):
    if target.endswith("apple-darwin"):
        source = ROOT / "dist/LevelEditor.app"
        if not (source / "Contents/Resources/ldtk-rs-install.json").is_file():
            raise ValueError("Run scripts/package-macos before creating the release archive")
        # Installed updaters require this archive root; the app's display name
        # and the local bundle are LevelEditor independently of that format.
        write_zip(source, destination, "LDtk Rust.app")
    else:
        with tempfile.TemporaryDirectory(prefix="ldtk-release-") as staging:
            source = Path(staging) / "ldtk-rs"
            source.mkdir()
            binary = "ldtk-rs.exe" if "windows" in target else "ldtk-rs"
            shutil.copy2(ROOT / "target/release" / binary, source / binary)
            shutil.copytree(ROOT / "assets", source / "resources/assets")
            shutil.copytree(ROOT / "examples/original", source / "resources/examples/original")
            for name in ["LICENSE", "THIRD_PARTY_NOTICES.md"]:
                shutil.copy2(ROOT / name, source / name)
            shutil.copy2(ROOT / "scripts/ldtk-rs-install.json", source / "ldtk-rs-install.json")
            write_zip(source, destination)


def write_zip(source, destination, archive_root=None):
    with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED, compresslevel=6) as archive:
        for path in sorted(source.rglob("*")):
            if path.is_file():
                archive.write(path, Path(archive_root or source.name) / path.relative_to(source))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=sorted(TARGETS))
    parser.add_argument("--merge", type=Path, help="Merge target metadata in an artifact directory")
    args = parser.parse_args()
    if args.merge:
        if "-" in VERSION.split("+", 1)[0]:
            raise ValueError("A prerelease version cannot be published to the stable update channel")
        targets = {}
        for path in sorted(args.merge.glob("target-*.json")):
            metadata = json.loads(path.read_text())
            if metadata["version"] != VERSION:
                raise ValueError(f"Release version mismatch: {path}")
            target = metadata["target"]
            entry = metadata["entry"]
            archive = args.merge / entry["asset"]
            if archive.stat().st_size != entry["size"] or hashlib.sha256(archive.read_bytes()).hexdigest() != entry["sha256"]:
                raise ValueError(f"Release checksum mismatch: {archive}")
            targets[target] = entry
        if set(targets) != TARGETS:
            raise ValueError(f"Incomplete release targets: {sorted(targets)}")
        manifest = args.merge / "ldtk-rs-updates.json"
        manifest.write_text(json.dumps({"version": VERSION, "targets": targets}, indent=2) + "\n")
        print(manifest)
    elif args.target:
        output = ROOT / "dist"
        output.mkdir(exist_ok=True)
        name = f"ldtk-rs-v{VERSION}-{args.target}.zip"
        archive = output / name
        package(args.target, archive)
        entry = {"asset": name, "sha256": hashlib.sha256(archive.read_bytes()).hexdigest(), "size": archive.stat().st_size}
        (output / f"target-{args.target}.json").write_text(json.dumps({"version": VERSION, "target": args.target, "entry": entry}, indent=2) + "\n")
        print(archive)
    else:
        parser.error("choose --target or --merge")


if __name__ == "__main__":
    main()
