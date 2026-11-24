"""Check that native release archives preserve the updater layout and target."""
import hashlib
import importlib.util
import json
from pathlib import Path
import plistlib
import struct
import tempfile
import unittest
from unittest.mock import patch
import zipfile


ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("package_release", ROOT / "scripts/package-release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


def macho(cpu):
    return struct.pack("<8I", 0xFEEDFACF, cpu, 0, 2, 0, 0, 0, 0)


class ReleasePackageTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.addCleanup(setattr, release, "ROOT", release.ROOT)
        release.ROOT = self.root

    def make_bundle(self):
        bundle = self.root / "dist/Level Editor.app"
        executable = bundle / "Contents/MacOS/level-editor"
        executable.parent.mkdir(parents=True)
        marker = bundle / "Contents/Resources/level-editor-install.json"
        marker.parent.mkdir()
        marker.write_bytes((ROOT / "scripts/level-editor-install.json").read_bytes())
        (bundle / "Contents/Info.plist").write_bytes((ROOT / "scripts/macos-info.plist").read_bytes())
        return executable

    def test_macos_archives_use_the_level_editor_application_name(self):
        executable = self.make_bundle()
        for target, cpu in [("aarch64-apple-darwin", 0x0100000C), ("x86_64-apple-darwin", 0x01000007)]:
            with self.subTest(target=target):
                binary = macho(cpu)
                executable.write_bytes(binary)
                executable.chmod(0o755)
                output = self.root / f"{target}.zip"
                release.package(target, output)
                with zipfile.ZipFile(output) as archive:
                    self.assertEqual({Path(name).parts[0] for name in archive.namelist()}, {"Level Editor.app"})
                    info = plistlib.loads(archive.read("Level Editor.app/Contents/Info.plist"))
                    self.assertEqual(info["CFBundleName"], "Level Editor")
                    self.assertEqual(info["CFBundleDisplayName"], "Level Editor")
                    self.assertEqual(info["CFBundleExecutable"], "level-editor")
                    entry = "Level Editor.app/Contents/MacOS/level-editor"
                    self.assertEqual(archive.read(entry), binary)
                    self.assertTrue((archive.getinfo(entry).external_attr >> 16) & 0o111)
                    marker = archive.read("Level Editor.app/Contents/Resources/level-editor-install.json")
                    self.assertEqual(json.loads(marker), {"product": "level-editor", "layout": 1})

    def test_arm_bundle_cannot_be_archived_as_intel(self):
        self.make_bundle().write_bytes(macho(0x0100000C))
        output = self.root / "intel.zip"
        with self.assertRaisesRegex(ValueError, "x86_64-apple-darwin"):
            release.package("x86_64-apple-darwin", output)
        self.assertFalse(output.exists())

    def test_packaging_and_manifest_use_branded_release_asset_names(self):
        self.make_bundle().write_bytes(macho(0x0100000C))
        arm = "aarch64-apple-darwin"
        with patch("sys.argv", ["package-release.py", "--target", arm]):
            release.main()
        output = self.root / "dist"
        arm_asset = output / f"level-editor-v{release.VERSION}-{arm}.zip"
        self.assertTrue(arm_asset.is_file())
        for target in release.TARGETS - {arm}:
            asset = f"level-editor-v{release.VERSION}-{target}.zip"
            with zipfile.ZipFile(output / asset, "w") as archive:
                archive.writestr("fixture", target)
            contents = (output / asset).read_bytes()
            (output / f"target-{target}.json").write_text(json.dumps({
                "version": release.VERSION, "target": target, "entry": {
                    "asset": asset, "size": len(contents),
                    "sha256": hashlib.sha256(contents).hexdigest(),
                },
            }))
        with patch("sys.argv", ["package-release.py", "--merge", str(output)]):
            release.main()
        manifest = json.loads((output / "level-editor-updates.json").read_text())
        self.assertEqual(manifest["version"], release.VERSION)
        self.assertEqual(set(manifest["targets"]), release.TARGETS)
        self.assertEqual({entry["asset"] for entry in manifest["targets"].values()},
                         {path.name for path in output.glob("level-editor-v*.zip")})
        self.assertEqual(list(output.glob("ldtk-rs-*")), [])

    def test_universal_bundle_accepts_only_architectures_in_its_slices(self):
        executable = self.make_bundle()
        x64 = 0x01000007
        arm64 = 0x0100000C
        for magic, layout, alignment in (
            (0xCAFEBABE, struct.Struct(">5I"), (0,)),
            (0xCAFEBABF, struct.Struct(">2I2Q2I"), (0, 0)),
        ):
            with self.subTest(magic=hex(magic)):
                start = 8 + 2 * layout.size
                header = (struct.pack(">2I", magic, 2)
                          + layout.pack(arm64, 0, start, 32, *alignment)
                          + layout.pack(x64, 0, start + 32, 32, *alignment))
                executable.write_bytes(header + macho(arm64) + macho(x64))
                for target in ("aarch64-apple-darwin", "x86_64-apple-darwin"):
                    release.package(target, self.root / f"{target}.zip")
                executable.write_bytes(header + macho(arm64) + macho(arm64))
                with self.assertRaisesRegex(ValueError, "x86_64-apple-darwin"):
                    release.package("x86_64-apple-darwin", self.root / "bad-intel.zip")

    def test_portable_archives_package_only_the_matching_binary(self):
        folder = self.root / "target/release"
        folder.mkdir(parents=True)
        for directory in ("assets", "examples/original", "scripts"):
            (self.root / directory).mkdir(parents=True)
        for filename in ("LICENSE", "THIRD_PARTY_NOTICES.md"):
            (self.root / filename).write_text("test fixture")
        marker = "scripts/level-editor-install.json"
        (self.root / marker).write_bytes((ROOT / marker).read_bytes())

        # Minimal 64-bit executable headers. These probe target matching, not OS loading.
        elf = b"\x7fELF\x02\x01\x01" + bytes(9) + struct.pack("<2H", 3, 62) + bytes(44)
        dos = b"MZ" + bytes(58) + struct.pack("<I", 64)
        pe = (dos + struct.pack("<4sHHIIIHHH", b"PE\0\0", 0x8664, 1, 0, 0, 0, 240, 2, 0x20B)
              + bytes(66) + struct.pack("<H", 2) + bytes(170))
        for target, name, header in (
            ("x86_64-unknown-linux-gnu", "level-editor", elf),
            ("x86_64-pc-windows-msvc", "level-editor.exe", pe),
        ):
            with self.subTest(target=target):
                executable = folder / name
                executable.write_bytes(header)
                output = self.root / f"{target}.zip"
                release.package(target, output)
                with zipfile.ZipFile(output) as archive:
                    self.assertEqual(archive.read(f"level-editor/{name}"), header)
                    marker = archive.read("level-editor/level-editor-install.json")
                    self.assertEqual(json.loads(marker), {"product": "level-editor", "layout": 1})
                executable.write_bytes(pe if header == elf else elf)
                with self.assertRaisesRegex(ValueError, target):
                    release.package(target, self.root / "bad.zip")
                self.assertFalse((self.root / "bad.zip").exists())

        windows = folder / "level-editor.exe"
        console = bytearray(pe)
        struct.pack_into("<H", console, 64 + 24 + 68, 3)
        windows.write_bytes(console)
        output = self.root / "console.zip"
        with self.assertRaisesRegex(ValueError, "Windows GUI subsystem"):
            release.package("x86_64-pc-windows-msvc", output)
        self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
