"""Check the macOS display rename without breaking the installed updater."""
import importlib.util
from pathlib import Path
import plistlib
import tempfile
import unittest
import zipfile


ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("package_release", ROOT / "scripts/package-release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleasePackageTests(unittest.TestCase):
    def test_level_editor_bundle_keeps_the_legacy_updater_archive_root(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            bundle = root / "dist/LevelEditor.app"
            executable = bundle / "Contents/MacOS/ldtk-rs"
            executable.parent.mkdir(parents=True)
            executable.write_bytes(b"native executable")
            executable.chmod(0o755)
            marker = bundle / "Contents/Resources/ldtk-rs-install.json"
            marker.parent.mkdir()
            marker.write_bytes((ROOT / "scripts/ldtk-rs-install.json").read_bytes())
            (bundle / "Contents/Info.plist").write_bytes((ROOT / "scripts/macos-info.plist").read_bytes())
            previous = release.ROOT
            release.ROOT = root
            try:
                for target in ["aarch64-apple-darwin", "x86_64-apple-darwin"]:
                    output = root / f"{target}.zip"
                    release.package(target, output)
                    with zipfile.ZipFile(output) as archive:
                        self.assertTrue(all(Path(name).parts[0] == "LDtk Rust.app" for name in archive.namelist()), "existing updaters reject other archive roots")
                        info = plistlib.loads(archive.read("LDtk Rust.app/Contents/Info.plist"))
                        self.assertEqual(info["CFBundleName"], "LevelEditor")
                        self.assertEqual(info["CFBundleDisplayName"], "LevelEditor")
                        self.assertEqual(archive.read("LDtk Rust.app/Contents/MacOS/ldtk-rs"), b"native executable")
                        self.assertTrue((archive.getinfo("LDtk Rust.app/Contents/MacOS/ldtk-rs").external_attr >> 16) & 0o111)
            finally:
                release.ROOT = previous


if __name__ == "__main__":
    unittest.main()
