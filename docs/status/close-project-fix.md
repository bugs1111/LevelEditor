# Project-close crash fix — 2026-11-17

The sidebar exit icon means “Close project” and returns to Home. Its actual
press/release at logical `(285,18)` reproduced the reported
`Option::unwrap()` on `None`: `Action::Close` removed `self.project`, then
the same sidebar frame continued reading project layer metadata. The original
backtrace points to `src/editor/chrome.rs:190`.

The production change rechecks project presence immediately after the toolbar
and ends that sidebar closure if the project was closed. The rest of the frame
continues normally and renders Home. This also precedes world-navigation access.
Save, discard, cancellation and application-quit behavior are unchanged.

Three real egui/App regressions cover:

- Clicking the exit icon from both level and world view returns to Home without
  entering crash recovery. This test failed before the guard and passed after it.
- Dirty project Save, Discard changes and Cancel use actual modal clicks and
  verify both on-disk and in-memory content.
- Native `ViewportEvent::Close` is accepted for a clean project and cancelled
  with the existing pending Quit/save guard for a dirty project, without panic.

The full workspace passed (native app 319 passed, 6 ignored), as did the native
example target (323 passed, 6 ignored), strict all-target Clippy, rustfmt and diff
checks. A read-only review found no important defect in the guard or regressions.

An actual Apple M6/Metal capture opened the Entities example through an isolated
last-project setting and injected the sidebar click. Before input it recorded
`Entities_demo` ready; after input and at capture it recorded no project,
`dirty=false`, no focus mask, zero surface errors and no panic. The Home PNG was
visually inspected. This uses the application's input hook, not OS mouse delivery.

All six Mac/Windows/Linux debug/release portable outputs were rebuilt with the
production fix; their binaries and all 253 resource files were verified. Both Mac
outputs started and opened the delivered example with empty stderr. The macOS
`.app` was refreshed and passed deep/strict ad-hoc signature verification.
Windows/Linux output was cross-linked, without new native GUI runtime claims.

Evidence: `.tools/close-project-red.log`, `.tools/close-project-flow.log`, and
`.tools/close-project/` (full checks, native capture, startup, and build logs).
Current hashes and native capture state are in [build-command.json](build-command.json).
