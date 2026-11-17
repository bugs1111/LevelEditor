# Mac cross-build command verification

Date: 2026-11-17 (Asia/Shanghai). Host: Apple Silicon macOS, Rust/Cargo 1.100.0.
Application source baseline: `5ab95ca`, followed by the workspace
[project-close fix](close-project-fix.md). Cargo inputs and resources remain
unchanged. Artifact hashes and sizes have been refreshed after the fix.

The final `just build -t <win|mac|linux> -m <debug|release>` entry was executed
for all six combinations. Each returned exit 0 and delivered the selected
executable, all 253 assets/examples files and both license notices to an
independent `dist/<platform>/<mode>/` directory. Every resource was compared
byte-for-byte with the repository. See [machine evidence](build-command.json).

| Target | Debug executable bytes | Release executable bytes | Actual format |
|---|---:|---:|---|
| mac | 106,764,792 | 24,330,608 | Mach-O ARM64 |
| win | 447,241,050 | 26,514,933 | PE32+ x86-64 |
| linux | 480,601,912 | 28,983,672 | ELF64 x86-64 PIE |

Windows uses `x86_64-pc-windows-gnu`, Homebrew mingw-w64 14.0.0_3 / GCC 16.2.0
and `+crt-static`. Both import tables contain Windows system/UCRT DLLs and no
libgcc_s, libstdc++ or libwinpthread DLL dependencies. The existing published
Windows CI package uses MSVC; this local cross-build entry uses GNU.

Linux uses `x86_64-unknown-linux-gnu.2.28`, cargo-zigbuild 0.21.8 and Zig 0.15.2
from the project-local `.tools/cross` venv. Both ELF binaries require at most
GLIBC_2.28 and use `/lib64/ld-linux-x86-64.so.2`. Their NEEDED entries are libc,
libm, libpthread, libdl and the loader. Vulkan and X11/Wayland libraries are
loaded dynamically and remain runtime requirements on the destination machine;
their absence from NEEDED is not evidence they are unnecessary. No fabricated
pkg-config data or replacement Linux libraries were used to obtain a link.
Zig/LLD emitted a nonfatal deprecated-linker-optimization warning during the
initial release link; the build completed successfully.

Both delivered macOS executables passed strict signature verification and
actually started with the delivered Entities example, recorded the opened
project in isolated settings and remained alive. Tests terminated only their
own processes. This is startup/document-opening evidence; Windows/Linux
cross-compilation does not establish GUI execution on those systems.

Six Python integration tests cover all six dispatch routes, paths containing
spaces, required/invalid arguments, help without compiler startup, child failure,
resource collection and stale resource replacement. The missing build recipe
failed before implementation. A later regression reproduced loss of the old
output on packaging failure, then passed after staging-before-replacement was
implemented. Compilation or resource-copy failure now preserves the old output;
a failed publish rename attempts restoration, retaining the backup if that
restoration fails. A separate read-only review found no remaining important
issue. Existing push/pull/run temporary-repository checks, just formatting and
diff checks also passed.

Raw evidence is retained locally under `.tools/build-command/`: initial Windows
and Linux compiler logs, PE/ELF inspections, final `verified/` command logs and
hashes, and `mac-startup/` isolated profiles/results. The portable output
directories are build products, not newly published update-channel releases.

Reproduce the command-contract checks:

```sh
python3 tests/test_build_command.py
just --fmt --check
```
