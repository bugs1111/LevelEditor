"""Exercise the real just/build boundary with the expensive compiler replaced."""
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
MAC_EXECUTABLE = "LevelEditor.app/Contents/MacOS/ldtk-rs"


class BuildCommandTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="ldtk build ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        shutil.copy2(ROOT / "justfile", self.root / "justfile")
        (self.root / "scripts").mkdir()
        if (ROOT / "scripts/build.py").exists():
            shutil.copy2(ROOT / "scripts/build.py", self.root / "scripts/build.py")
        for name in ("package-macos", "macos-info.plist", "ldtk-rs-install.json"):
            shutil.copy2(ROOT / "scripts" / name, self.root / "scripts" / name)
        (self.root / "Cargo.toml").write_text('[package]\nversion = "0.42.0"\n')
        self.tools = self.root / "shared-bin"
        self.tools.mkdir()
        plutil = self.tools / "plutil"
        plutil.write_text(f"#!{sys.executable}\n" + '''
import pathlib, plistlib, sys
path = pathlib.Path(sys.argv[-1])
data = plistlib.loads(path.read_bytes())
if sys.argv[1] == '-replace':
    data[sys.argv[2]] = sys.argv[4]
    path.write_bytes(plistlib.dumps(data))
''')
        plutil.chmod(0o755)
        codesign = self.tools / "codesign"
        codesign.write_text("#!/bin/sh\nexit 0\n")
        codesign.chmod(0o755)
        legacy_cargo = self.root / ".tools/cargo/bin/cargo"
        legacy_cargo.parent.mkdir(parents=True)
        legacy_cargo.write_text("#!/bin/sh\necho 'Selected the legacy project-local Cargo' >&2\nexit 91\n")
        legacy_cargo.chmod(0o755)
        cargo = self.tools / "cargo"
        cargo.write_text(f"#!{sys.executable}\n" + '''
import json, os, pathlib, shutil, sys
root = pathlib.Path(__file__).resolve().parents[1]
if sys.argv[1:] == ['-vV']:
    print('host: ' + os.getenv('BUILD_TEST_HOST', 'aarch64-apple-darwin'))
    sys.exit(0)
with (root / 'calls.jsonl').open('a') as log:
    log.write(json.dumps({'args': sys.argv[1:], 'linker': os.getenv('CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER'), 'rustflags': os.getenv('RUSTFLAGS'), 'zigbuild': shutil.which('cargo-zigbuild')}) + '\\n')
if os.getenv('BUILD_TEST_EXIT'):
    sys.exit(int(os.environ['BUILD_TEST_EXIT']))
args = sys.argv[1:]
target = args[args.index('--target') + 1] if '--target' in args else None
folder = pathlib.Path(os.environ['CARGO_TARGET_DIR'])
if target:
    folder /= target.removesuffix('.2.28')
folder /= 'release' if '--release' in args else 'debug'
folder.mkdir(parents=True, exist_ok=True)
(folder / ('ldtk-rs.exe' if target and 'windows' in target else 'ldtk-rs')).write_bytes(b'compiled executable')
''')
        cargo.chmod(0o755)
        rustup = self.tools / "rustup"
        rustup.write_text(f"#!{sys.executable}\n" + '''
import os, pathlib, sys
root = pathlib.Path(__file__).resolve().parents[1]
if os.getenv('BUILD_TEST_NO_RUSTUP'):
    sys.exit(93)
if sys.argv[1:] == ['target', 'list', '--installed']:
    print('aarch64-apple-darwin\\nx86_64-pc-windows-gnu\\nx86_64-unknown-linux-gnu')
elif sys.argv[1:3] == ['target', 'add']:
    with (root / 'installed-targets.txt').open('a') as log:
        log.write(' '.join(sys.argv[3:]) + '\\n')
elif sys.argv[1] == 'which':
    print(root / 'shared-bin' / sys.argv[2])
else:
    sys.exit(92)
''')
        rustup.chmod(0o755)
        shutil.copy2(ROOT / "scripts/cargo", self.root / "scripts/cargo")
        linker = self.tools / "x86_64-w64-mingw32-gcc"
        linker.write_text("#!/bin/sh\nexit 0\n")
        linker.chmod(0o755)
        self.home = self.root / "user"
        self.cross = self.home / ".cargo/cross"
        for cross in (self.root / ".tools/cross", self.cross):
            (cross / "bin").mkdir(parents=True)
            for tool in ("cargo-zigbuild", "python-zig"):
                (cross / "bin" / tool).touch(mode=0o755)
            (cross / "bin/python3").symlink_to(sys.executable)
            (cross / "versions.txt").write_text("cargo-zigbuild==0.21.8\nziglang==0.15.2\n")
        for name in [
            "assets/icon.png", "assets/LevelEditor.icns", "assets/project.icns",
            "examples/original/example.ldtk", "LICENSE", "THIRD_PARTY_NOTICES.md",
        ]:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(name)
        self.env = os.environ.copy()
        self.env["HOME"] = str(self.home)
        self.env.pop("CARGO_HOME", None)
        self.env["PATH"] = str(self.tools) + os.pathsep + self.env["PATH"]

    def run_build(self, *args):
        return subprocess.run(["just", "build", *args], cwd=self.root, env=self.env,
                              text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

    def test_defaults_build_all_platforms_in_release_mode(self):
        result = self.run_build()
        self.assertEqual(result.returncode, 0, result.stdout)
        calls = [json.loads(line) for line in (self.root / "calls.jsonl").read_text().splitlines()]
        self.assertEqual(len(calls), 3)
        self.assertTrue(all("--release" in call["args"] for call in calls))
        native = next(call for call in calls if "--target" not in call["args"])
        self.assertIsNone(native["linker"])
        self.assertNotIn("+crt-static", native["rustflags"] or "")
        for platform, binary in [("mac", MAC_EXECUTABLE), ("win", "ldtk-rs.exe"), ("linux", "ldtk-rs")]:
            self.assertTrue((self.root / "dist" / platform / "release" / binary).is_file())

    def test_explicit_mac_architectures_have_separate_outputs(self):
        for arch, target in [("arm64", None), ("x86_64", "x86_64-apple-darwin")]:
            with self.subTest(arch=arch):
                result = self.run_build("-t", "mac", "-a", arch)
                self.assertEqual(result.returncode, 0, result.stdout)
                args = json.loads((self.root / "calls.jsonl").read_text().splitlines()[-1])["args"]
                self.assertIn("--release", args)
                if target:
                    self.assertEqual(args[args.index("--target") + 1], target)
                else:
                    self.assertNotIn("--target", args)
                self.assertTrue((self.root / "dist/mac" / arch / "release" / MAC_EXECUTABLE).is_file())
        self.assertEqual((self.root / "installed-targets.txt").read_text(), "x86_64-apple-darwin\n")

    def test_all_debug_with_arch_only_changes_mac_selection(self):
        result = self.run_build("-t", "all", "-m", "debug", "-a", "x86_64")
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertTrue((self.root / "dist/mac/x86_64/debug" / MAC_EXECUTABLE).is_file())
        self.assertTrue((self.root / "dist/win/debug/ldtk-rs.exe").is_file())
        self.assertTrue((self.root / "dist/linux/debug/ldtk-rs").is_file())

    def test_native_mac_build_does_not_invoke_rustup(self):
        self.env["BUILD_TEST_NO_RUSTUP"] = "1"
        self.env["BUILD_TEST_HOST"] = "x86_64-apple-darwin"
        result = self.run_build("-t", "mac", "-a", "x86_64", "-m", "debug")
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertTrue((self.root / "dist/mac/x86_64/debug" / MAC_EXECUTABLE).is_file())
        self.assertFalse((self.root / "installed-targets.txt").exists())

    def test_all_six_routes_and_portable_resources(self):
        for platform, command, target in [
            ("win", "build", "x86_64-pc-windows-gnu"),
            ("mac", "build", None),
            ("linux", "zigbuild", "x86_64-unknown-linux-gnu.2.28"),
        ]:
            for mode in ("debug", "release"):
                with self.subTest(platform=platform, mode=mode):
                    result = self.run_build("-t", platform, "-m", mode)
                    self.assertEqual(result.returncode, 0, result.stdout)
                    call = json.loads((self.root / "calls.jsonl").read_text().splitlines()[-1])
                    args = call["args"]
                    self.assertEqual(args[0], command)
                    self.assertIn("--locked", args)
                    self.assertEqual(args[args.index("--bin") + 1], "ldtk-rs")
                    self.assertEqual("--release" in args, mode == "release")
                    if target:
                        self.assertEqual(args[args.index("--target") + 1], target)
                    else:
                        self.assertNotIn("--target", args)
                    if platform == "win":
                        self.assertEqual(call["linker"], str(self.tools / "x86_64-w64-mingw32-gcc"))
                        self.assertIn("+crt-static", call["rustflags"])
                    if platform == "linux":
                        self.assertEqual(call["zigbuild"], str(self.cross / "bin/cargo-zigbuild"))
                        for tool, launcher in (("cargo-zigbuild", "cargo-zigbuild"), ("zig", "python-zig")):
                            entry = self.home / ".cargo/bin" / tool
                            self.assertEqual(entry.resolve(strict=True), self.cross / "bin" / launcher)
                    out = self.root / "dist" / platform / mode
                    if platform == "mac":
                        binary = out / MAC_EXECUTABLE
                        resources = out / "LevelEditor.app/Contents/Resources"
                        prefix = resources
                        plist = plistlib.loads((resources.parent / "Info.plist").read_bytes())
                        self.assertEqual(plist["CFBundleName"], "LevelEditor")
                        self.assertEqual(plist["CFBundleExecutable"], binary.name)
                        self.assertEqual(plist["CFBundleShortVersionString"], "0.42.0")
                        self.assertEqual(plist["CFBundleIconFile"], "LevelEditor.icns")
                        self.assertEqual((resources / "LevelEditor.icns").read_bytes(),
                                         (self.root / "assets/LevelEditor.icns").read_bytes())
                    else:
                        binary = out / ("ldtk-rs.exe" if platform == "win" else "ldtk-rs")
                        resources = out / "resources"
                        prefix = out
                    self.assertEqual(binary.read_bytes(), b"compiled executable")
                    for name in ("assets/icon.png", "examples/original/example.ldtk"):
                        self.assertEqual((resources / name).read_bytes(), (self.root / name).read_bytes())
                    for name in ("LICENSE", "THIRD_PARTY_NOTICES.md"):
                        self.assertEqual((prefix / name).read_bytes(), (self.root / name).read_bytes())

    def test_invalid_arguments_do_not_start_compiler(self):
        for args in [
            ["-t", "win", "-m", "fast"], ["-t", "other", "-m", "debug"],
            ["-t", "win", "-m", "debug", "extra"], ["-t", "mac", "-a", "other"],
            ["-t", "win", "-a", "arm64"], ["-t", "linux", "-a", "x86_64"],
        ]:
            with self.subTest(args=args):
                self.assertNotEqual(self.run_build(*args).returncode, 0)
                self.assertFalse((self.root / "calls.jsonl").exists())

    def test_help_does_not_start_compiler(self):
        result = self.run_build("-h")
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertFalse((self.root / "calls.jsonl").exists())

    def test_failure_keeps_previous_output(self):
        old = self.root / "dist/mac/debug" / MAC_EXECUTABLE
        old.parent.mkdir(parents=True)
        old.write_bytes(b"previous build")
        self.env["BUILD_TEST_EXIT"] = "17"
        result = self.run_build("-t", "mac", "-m", "debug")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(old.read_bytes(), b"previous build")
        self.assertIn("17", result.stdout)

    def test_success_removes_stale_resources(self):
        stale = self.root / "dist/mac/debug/LevelEditor.app/Contents/Resources/old-file"
        stale.parent.mkdir(parents=True)
        stale.touch()
        result = self.run_build("-t", "mac", "-m", "debug")
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertFalse(stale.exists())

    def test_packaging_failure_keeps_previous_output(self):
        old = self.root / "dist/mac/debug" / MAC_EXECUTABLE
        old.parent.mkdir(parents=True)
        old.write_bytes(b"previous build")
        (self.root / "LICENSE").unlink()
        result = self.run_build("-t", "mac", "-m", "debug")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(old.read_bytes(), b"previous build")


if __name__ == "__main__":
    unittest.main()
