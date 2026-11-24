#!/usr/bin/env python3
"""Create native release ZIPs and the updater's checksummed target metadata."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import struct
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]
VERSION = re.search(r'^version = "([^"]+)"', (ROOT / "Cargo.toml").read_text(), re.M)[1]
MANIFEST_NAME = "level-editor-updates.json"
TARGETS = {"aarch64-apple-darwin", "x86_64-apple-darwin",
           "x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"}


def archive_name(target):
    return f"level-editor-v{VERSION}-{target}.zip"


def macho_matches(executable, header, cpu):
    def thin_matches(data):
        # A 64-bit little-endian Mach-O with the requested CPU and MH_EXECUTE.
        return (len(data) >= 32 and data[:4] == b"\xcf\xfa\xed\xfe"
                and struct.unpack_from("<I4xI", data, 4) == (cpu, 2))

    if thin_matches(header):
        return True

    # Universal binaries have a big-endian table of slices with 32- or 64-bit offsets.
    if header[:4] == b"\xca\xfe\xba\xbe":
        entry = struct.Struct(">5I")
    elif header[:4] == b"\xca\xfe\xba\xbf":
        entry = struct.Struct(">2I2Q2I")
    else:
        return False
    if len(header) < 8:
        return False

    count = struct.unpack_from(">I", header, 4)[0]
    table_size = count * entry.size
    file_size = executable.seek(0, 2)
    if 8 + table_size > min(file_size, 4096):
        return False
    executable.seek(8)
    entries = executable.read(table_size)
    if len(entries) != table_size:
        return False
    for architecture, _, offset, size, *_ in entry.iter_unpack(entries):
        if architecture == cpu and 8 + table_size <= offset and size >= 32 and offset + size <= file_size:
            executable.seek(offset)
            if thin_matches(executable.read(32)):
                return True
    return False


def check_executable_target(path, target):
    with path.open("rb") as executable:
        header = executable.read(64)
        if target.endswith("apple-darwin"):
            cpu = 0x0100000C if target.startswith("aarch64") else 0x01000007
            matches = macho_matches(executable, header, cpu)
        elif target == "x86_64-pc-windows-msvc":
            matches = False
            if header[:2] == b"MZ" and len(header) == 64:
                executable.seek(struct.unpack_from("<I", header, 60)[0])
                subsystem_offset = 24 + 68
                pe_header = executable.read(subsystem_offset + 2)
                # The PE signature, AMD64 machine, executable image (not DLL), and PE32+.
                if len(pe_header) == subsystem_offset + 2 and pe_header[:6] == b"PE\0\0\x64\x86":
                    optional_size, flags, magic = struct.unpack_from("<3H", pe_header, 20)
                    matches = optional_size >= 70 and (flags & 0x2002) == 2 and magic == 0x20B
                    if matches and struct.unpack_from("<H", pe_header, subsystem_offset)[0] != 2:
                        raise ValueError(f"{path} must use the Windows GUI subsystem")
        else:
            # ELF64 little endian, x86-64, with either the executable or PIE file type.
            matches = (len(header) == 64 and header[:6] == b"\x7fELF\x02\x01"
                       and struct.unpack_from("<2H", header, 16) in ((2, 62), (3, 62)))
    if not matches:
        raise ValueError(f"{path} does not contain a {target} executable")


def package(target, destination):
    if target.endswith("apple-darwin"):
        source = ROOT / "dist/LevelEditor.app"
        if not (source / "Contents/Resources/ldtk-rs-install.json").is_file():
            raise ValueError("Run scripts/package-macos before creating the release archive")
        check_executable_target(source / "Contents/MacOS/ldtk-rs", target)
        # Installed updaters require this archive root; the app's display name
        # and the local bundle are LevelEditor independently of that format.
        write_zip(source, destination, "LDtk Rust.app")
    else:
        binary = "ldtk-rs.exe" if "windows" in target else "ldtk-rs"
        executable = ROOT / "target/release" / binary
        check_executable_target(executable, target)
        with tempfile.TemporaryDirectory(prefix="ldtk-release-") as staging:
            source = Path(staging) / "ldtk-rs"
            source.mkdir()
            shutil.copy2(executable, source / binary)
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
            if target not in TARGETS or target in targets or entry["asset"] != archive_name(target):
                raise ValueError(f"Invalid release package name or target: {path}")
            archive = args.merge / entry["asset"]
            if archive.stat().st_size != entry["size"] or hashlib.sha256(archive.read_bytes()).hexdigest() != entry["sha256"]:
                raise ValueError(f"Release checksum mismatch: {archive}")
            targets[target] = entry
        if set(targets) != TARGETS:
            raise ValueError(f"Incomplete release targets: {sorted(targets)}")
        manifest = args.merge / MANIFEST_NAME
        manifest.write_text(json.dumps({"version": VERSION, "targets": targets}, indent=2) + "\n")
        print(manifest)
    elif args.target:
        output = ROOT / "dist"
        output.mkdir(exist_ok=True)
        name = archive_name(args.target)
        archive = output / name
        package(args.target, archive)
        entry = {"asset": name, "sha256": hashlib.sha256(archive.read_bytes()).hexdigest(), "size": archive.stat().st_size}
        (output / f"target-{args.target}.json").write_text(json.dumps({"version": VERSION, "target": args.target, "entry": entry}, indent=2) + "\n")
        print(archive)
    else:
        parser.error("choose --target or --merge")


if __name__ == "__main__":
    main()
