# Cross-platform just build implementation plan

> **For agentic workers:** Use superpowers:executing-plans for the shared command; independent Windows/Linux toolchain probes use superpowers:dispatching-parallel-agents.

**Goal:** On this Mac, `just build -t win|mac|linux -m debug|release` produces a usable executable plus resources for the selected platform/profile.

**Architecture:** A positional-argument just recipe delegates to a small Python build script. Reuse the repository Rust toolchain and locked dependencies. Native macOS uses the host architecture; Windows uses x86_64 GNU/MinGW with static compiler runtime; Linux uses x86_64 GNU via pinned cargo-zigbuild/Zig and a glibc 2.28 floor. Copy successful output, assets, examples and license notices into `dist/<platform>/<mode>/`.

**Tech stack:** just, Python 3, existing Cargo wrapper, Homebrew mingw-w64, project-local cargo-zigbuild 0.21.8 and ziglang 0.15.2.

**Spec:** Current user request and clarification: both platform/mode flags, with real Mac-to-Windows/Linux cross compilation.

## Constraints and review focus

- Modify only the build entry, its tests and usage documentation; no application/Cargo/CI behavior changes.
- Both `-t` and `-m` are required; invalid/missing arguments fail before any compiler/tool installation.
- Build failure preserves previous delivered output and returns the failing command's exit status.
- Paths with spaces remain intact; do not interpolate user arguments into shell code.
- Runtime resources accompany the executable; each platform/profile has an independent output directory.
- Cross-link evidence does not establish Windows/Linux GUI runtime behavior.

## Tasks

- [x] Add `tests/test_build_command.py`: six platform/profile routes, invalid arguments/help, failure propagation, resource copying and stale output cleanup. Run before implementation and observe the missing-build-recipe failures.
- [x] Add `scripts/build.py` and the positional `build *args` recipe. Bootstrap missing Rust targets and the selected linker tooling; reuse installed tooling. Run command tests, existing just integration checks, just formatting and diff checks.
- [x] Complete real debug/release builds for Windows GNU and Linux GNU using separate target directories. Verify PE/ELF architecture and runtime library requirements. Complete native macOS debug/release builds through the final just entry.
- [x] Verify all six final commands and output resources; document actual paths, installed tool versions, glibc/runtime requirements and execution limits in README and a compact build report.
