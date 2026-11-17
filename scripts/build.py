#!/usr/bin/env python3
"""Build portable Windows, macOS and Linux executables from a Mac."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[1]
CROSS_VERSIONS = "cargo-zigbuild==0.21.8\nziglang==0.15.2\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("-t", choices=["win", "mac", "linux"], required=True)
    parser.add_argument("-m", choices=["debug", "release"], required=True)
    args = parser.parse_args()
    env = os.environ.copy()
    local_cargo = ROOT / ".tools/cargo/bin"
    if (local_cargo / "cargo").is_file():
        env.update(CARGO_HOME=str(ROOT / ".tools/cargo"), RUSTUP_HOME=str(ROOT / ".tools/rustup"))
        env["PATH"] = str(local_cargo) + os.pathsep + env["PATH"]
    cargo = str(ROOT / "scripts/cargo")
    host = next(line.removeprefix("host: ") for line in subprocess.check_output(
        [cargo, "-vV"], env=env, text=True).splitlines() if line.startswith("host: "))
    if not host.endswith("apple-darwin"):
        parser.error("This cross-build entry requires a macOS host.")
    target = {"win": "x86_64-pc-windows-gnu", "linux": "x86_64-unknown-linux-gnu", "mac": host}[args.t]
    installed = subprocess.check_output(["rustup", "target", "list", "--installed"], env=env, text=True).splitlines()
    if target not in installed:
        subprocess.run(["rustup", "target", "add", target], env=env, check=True)

    command = [cargo, "build"]
    build_root = ROOT / "target"
    if args.t == "win":
        if not shutil.which("x86_64-w64-mingw32-gcc", path=env["PATH"]):
            if not shutil.which("brew", path=env["PATH"]):
                parser.error("Windows builds require MinGW: install Homebrew, then brew install mingw-w64.")
            subprocess.run(["brew", "install", "mingw-w64"], env=env, check=True)
        env["CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER"] = shutil.which("x86_64-w64-mingw32-gcc", path=env["PATH"])
        env["RUSTFLAGS"] = (env.get("RUSTFLAGS", "") + " -C target-feature=+crt-static").strip()
    elif args.t == "linux":
        cross = ROOT / ".tools/cross"
        python = cross / "bin/python3"
        versions = cross / "versions.txt"
        if not (cross / "bin/cargo-zigbuild").is_file() or not versions.is_file() or versions.read_text() != CROSS_VERSIONS:
            if not python.exists():
                subprocess.run([sys.executable, "-m", "venv", str(cross)], check=True)
            subprocess.run([str(python), "-m", "pip", "install", "--disable-pip-version-check", *CROSS_VERSIONS.splitlines()], check=True)
            versions.write_text(CROSS_VERSIONS)
        env["PATH"] = str(cross / "bin") + os.pathsep + env["PATH"]
        env["CARGO_ZIGBUILD_CACHE_DIR"] = str(cross / "cache")
        env["ZIG_GLOBAL_CACHE_DIR"] = str(cross / "zig-cache")
        command[1] = "zigbuild"
    if args.t != "mac":
        build_root /= "build/" + args.t
        command += ["--target", target + (".2.28" if args.t == "linux" else "")]
    env["CARGO_TARGET_DIR"] = str(build_root)
    command += ["--bin", "ldtk-rs", "--locked"]
    if args.m == "release":
        command.append("--release")
    subprocess.run(command, cwd=ROOT, env=env, check=True)

    name = "ldtk-rs.exe" if args.t == "win" else "ldtk-rs"
    binary = build_root / (target if args.t != "mac" else "") / args.m / name
    output = ROOT / "dist" / args.t / args.m
    output.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix=f".{args.m}-", dir=output.parent))
    previous = staging / "previous"
    try:
        packaged = staging / "output"
        packaged.mkdir()
        shutil.copy2(binary, packaged / name)
        shutil.copytree(ROOT / "assets", packaged / "resources/assets")
        shutil.copytree(ROOT / "examples/original", packaged / "resources/examples/original")
        for notice in ("LICENSE", "THIRD_PARTY_NOTICES.md"):
            shutil.copy2(ROOT / notice, packaged / notice)
        if output.exists():
            output.rename(previous)
        packaged.rename(output)
    finally:
        if previous.exists() and not output.exists():
            previous.rename(output)
        shutil.rmtree(staging)
    print(f"Built {target} ({args.m}): {output / name}")


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        sys.exit(error.returncode)
