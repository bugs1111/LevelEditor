"""Exercise the real just/build boundary with the expensive compiler replaced."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class BuildCommandTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="ldtk build ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        shutil.copy2(ROOT / "justfile", self.root / "justfile")
        (self.root / "scripts").mkdir()
        if (ROOT / "scripts/build.py").exists():
            shutil.copy2(ROOT / "scripts/build.py", self.root / "scripts/build.py")
        self.tools = self.root / ".tools/cargo/bin"
        self.tools.mkdir(parents=True)
        cargo = self.tools / "cargo"
        cargo.write_text(f"#!{sys.executable}\n" + '''
import json, os, pathlib, sys
root = pathlib.Path(__file__).resolve().parents[3]
if sys.argv[1:] == ['-vV']:
    print('host: aarch64-apple-darwin')
    sys.exit(0)
with (root / 'calls.jsonl').open('a') as log:
    log.write(json.dumps({'args': sys.argv[1:], 'linker': os.getenv('CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER'), 'rustflags': os.getenv('RUSTFLAGS')}) + '\\n')
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
        rustup.write_text("#!/bin/sh\nprintf '%s\\n' aarch64-apple-darwin x86_64-pc-windows-gnu x86_64-unknown-linux-gnu\n")
        rustup.chmod(0o755)
        shutil.copy2(ROOT / "scripts/cargo", self.root / "scripts/cargo")
        linker = self.tools / "x86_64-w64-mingw32-gcc"
        linker.write_text("#!/bin/sh\nexit 0\n")
        linker.chmod(0o755)
        cross = self.root / ".tools/cross/bin"
        cross.mkdir(parents=True)
        (cross / "cargo-zigbuild").touch()
        (cross / "python3").symlink_to(sys.executable)
        (cross.parent / "versions.txt").write_text("cargo-zigbuild==0.21.8\nziglang==0.15.2\n")
        for name in ["assets/icon.png", "examples/original/example.ldtk", "LICENSE", "THIRD_PARTY_NOTICES.md"]:
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(name)
        self.env = os.environ.copy()
        self.env["PATH"] = str(self.tools) + os.pathsep + self.env["PATH"]

    def run_build(self, *args):
        return subprocess.run(["just", "build", *args], cwd=self.root, env=self.env,
                              text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)

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
                    out = self.root / "dist" / platform / mode
                    binary = out / ("ldtk-rs.exe" if platform == "win" else "ldtk-rs")
                    self.assertEqual(binary.read_bytes(), b"compiled executable")
                    for src, dest in [("assets/icon.png", "resources/assets/icon.png"), ("examples/original/example.ldtk", "resources/examples/original/example.ldtk"), ("LICENSE", "LICENSE"), ("THIRD_PARTY_NOTICES.md", "THIRD_PARTY_NOTICES.md")]:
                        self.assertEqual((out / dest).read_bytes(), (self.root / src).read_bytes())

    def test_invalid_arguments_do_not_start_compiler(self):
        for args in [[], ["-t", "win"], ["-t", "win", "-m", "fast"], ["-t", "other", "-m", "debug"], ["-t", "win", "-m", "debug", "extra"]]:
            with self.subTest(args=args):
                self.assertNotEqual(self.run_build(*args).returncode, 0)
                self.assertFalse((self.root / "calls.jsonl").exists())

    def test_help_does_not_start_compiler(self):
        result = self.run_build("-h")
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertFalse((self.root / "calls.jsonl").exists())

    def test_failure_keeps_previous_output(self):
        old = self.root / "dist/mac/debug/ldtk-rs"
        old.parent.mkdir(parents=True)
        old.write_bytes(b"previous build")
        self.env["BUILD_TEST_EXIT"] = "17"
        result = self.run_build("-t", "mac", "-m", "debug")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(old.read_bytes(), b"previous build")
        self.assertIn("17", result.stdout)

    def test_success_removes_stale_resources(self):
        stale = self.root / "dist/mac/debug/resources/old-file"
        stale.parent.mkdir(parents=True)
        stale.touch()
        result = self.run_build("-t", "mac", "-m", "debug")
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertFalse(stale.exists())

    def test_packaging_failure_keeps_previous_output(self):
        old = self.root / "dist/mac/debug/ldtk-rs"
        old.parent.mkdir(parents=True)
        old.write_bytes(b"previous build")
        (self.root / "LICENSE").unlink()
        result = self.run_build("-t", "mac", "-m", "debug")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(old.read_bytes(), b"previous build")


if __name__ == "__main__":
    unittest.main()
