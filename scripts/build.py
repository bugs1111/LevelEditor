#!/usr/bin/env python3
"""Build a macOS application bundle and portable Windows and Linux programs from a Mac."""
import argparse
from contextlib import suppress
import fcntl
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[1]
CROSS_VERSIONS = "cargo-zigbuild==0.21.8\nziglang==0.15.2\n"
MAC_TARGETS = {"arm64": "aarch64-apple-darwin", "x86_64": "x86_64-apple-darwin"}


def configure_linux_cross(env):
    cargo_home = Path(env.get("CARGO_HOME") or Path.home() / ".cargo").expanduser().resolve()
    shared_bin = cargo_home / "bin"
    shared_bin.mkdir(parents=True, exist_ok=True)
    cross = cargo_home / "cross"
    cross_bin = cross / "bin"
    python = cross_bin / "python3"
    versions = cross / "versions.txt"
    launchers = {"cargo-zigbuild": "cargo-zigbuild", "zig": "python-zig"}
    with (cargo_home / "cross.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if not versions.is_file() or versions.read_text() != CROSS_VERSIONS or not all(
            (cross_bin / launcher).is_file() for launcher in launchers.values()
        ):
            if not (cross_bin / "pip").is_file():
                subprocess.run([sys.executable, "-m", "venv", str(cross)], check=True)
            subprocess.run([str(python), "-m", "pip", "install", "--disable-pip-version-check", *CROSS_VERSIONS.splitlines()], check=True)
            versions.write_text(CROSS_VERSIONS)
        for tool, launcher in launchers.items():
            # Keep a tool the user already installed under this name.
            with suppress(FileExistsError):
                (shared_bin / tool).symlink_to(Path("../cross/bin") / launcher)
    env["PATH"] = str(cross_bin) + os.pathsep + env["PATH"]
    env["CARGO_ZIGBUILD_CACHE_DIR"] = str(cross / "cache")
    env["ZIG_GLOBAL_CACHE_DIR"] = str(cross / "zig-cache")


def cross_toolchain(targets, env, parser):
    env = env.copy()
    rustup = shutil.which("rustup", path=env["PATH"])
    if rustup is None:
        cargo_home = Path(env.get("CARGO_HOME") or Path.home() / ".cargo").expanduser()
        rustup = shutil.which(str(cargo_home / "bin/rustup"))
    if rustup is None:
        parser.error("Cross-compilation requires a Rustup toolchain with the requested targets. "
                     "Install Rustup and a stable toolchain. Native macOS builds can use Homebrew Rust.")
    installed = subprocess.check_output(
        [rustup, "target", "list", "--installed"], cwd=ROOT, env=env, text=True
    ).splitlines()
    missing = [target for target in targets if target not in installed]
    if missing:
        subprocess.run([rustup, "target", "add", *missing], cwd=ROOT, env=env, check=True)
    # Select the same toolchain that received the targets, even when Homebrew is first in PATH.
    cargo = subprocess.check_output([rustup, "which", "cargo"], cwd=ROOT, env=env, text=True).strip()
    env["RUSTC"] = subprocess.check_output([rustup, "which", "rustc"], cwd=ROOT, env=env, text=True).strip()
    return cargo, env


def build(platform, mode, target, host, cargo, env, output, parser):
    env = env.copy()
    command = [cargo, "build"]
    build_root = ROOT / "target"
    if platform == "win":
        if not shutil.which("x86_64-w64-mingw32-gcc", path=env["PATH"]):
            if not shutil.which("brew", path=env["PATH"]):
                parser.error("Windows builds require MinGW: install Homebrew, then brew install mingw-w64.")
            subprocess.run(["brew", "install", "mingw-w64"], env=env, check=True)
        env["CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER"] = shutil.which("x86_64-w64-mingw32-gcc", path=env["PATH"])
        env["RUSTFLAGS"] = (env.get("RUSTFLAGS", "") + " -C target-feature=+crt-static").strip()
    elif platform == "linux":
        configure_linux_cross(env)
        command[1] = "zigbuild"
    if target != host:
        build_root /= "build/" + platform
        command += ["--target", target + (".2.28" if platform == "linux" else "")]
    env["CARGO_TARGET_DIR"] = str(build_root)
    command += ["--bin", "level-editor", "--locked"]
    if mode == "release":
        command.append("--release")
    subprocess.run(command, cwd=ROOT, env=env, check=True)

    name = "level-editor.exe" if platform == "win" else "level-editor"
    binary = build_root / (target if target != host else "") / mode / name
    product = "Level Editor.app" if platform == "mac" else name
    output.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix=f".{mode}-", dir=output.parent))
    previous = staging / "previous"
    try:
        packaged = staging / "output"
        packaged.mkdir()
        if platform == "mac":
            subprocess.run(
                [ROOT / "scripts/package-macos", binary, packaged / product],
                cwd=ROOT, env=env, check=True, stdout=subprocess.DEVNULL,
            )
        else:
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
    print(f"Built {target} ({mode}): {output / product}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("-t", choices=["all", "win", "mac", "linux"], default="all",
                        help="platform to build (default: all)")
    parser.add_argument("-m", choices=["release", "debug"], default="release",
                        help="build profile (default: release)")
    parser.add_argument("-a", choices=MAC_TARGETS, help="macOS architecture (default: native)")
    args = parser.parse_args()
    if args.a and args.t not in ("all", "mac"):
        parser.error("-a selects the macOS architecture and requires -t mac or -t all.")

    env = os.environ.copy()
    cargo = str(ROOT / "scripts/cargo")
    host = next(line.removeprefix("host: ") for line in subprocess.check_output(
        [cargo, "-vV"], cwd=ROOT, env=env, text=True).splitlines() if line.startswith("host: "))
    if not host.endswith("apple-darwin"):
        parser.error("This cross-build entry requires a macOS host.")
    targets = {
        "mac": MAC_TARGETS[args.a] if args.a else host,
        "win": "x86_64-pc-windows-gnu",
        "linux": "x86_64-unknown-linux-gnu",
    }
    platforms = list(targets) if args.t == "all" else [args.t]
    cross_targets = [targets[platform] for platform in platforms if targets[platform] != host]
    cross_cargo, cross_env = cross_toolchain(cross_targets, env, parser) if cross_targets else (cargo, env)
    for platform in platforms:
        target = targets[platform]
        output = ROOT / "dist" / platform
        if platform == "mac" and args.a:
            output /= args.a
        selected_cargo, selected_env = (cargo, env) if target == host else (cross_cargo, cross_env)
        build(platform, args.m, target, host, selected_cargo, selected_env, output / args.m, parser)


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        sys.exit(error.returncode)
