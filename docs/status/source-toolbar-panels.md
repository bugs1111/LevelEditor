# Toolbar floating panels — 2026-11-17

The World toolbar button now opens the original-style floating WorldPanel at
client `(0,34)`, 400 logical pixels wide, above the existing layer sidebar.
Closing it exits world mode. All nine toolbar destinations toggle and switch
their floating panels. World keeps the canvas interactive; other form overlays
block painting and world context actions through their dimmed canvas mask.
Zooming out into World also opens its form. Escape cancels level placement
before closing WorldPanel. Switching panels commits tracked field drafts first.

The World form follows the user's reference screenshot: create/duplicate/delete
level actions, World settings, current level properties, and custom fields.
Current-level and project settings use their source single-column forms rather
than a definition-navigation list. Their outer widths are 490 and 840 logical
pixels. The World canvas toolbar follows the wider overlay edge. Single-world
identifier disabling, conditional GridVania controls, friendly layout choices,
IID copying, paired numeric inputs, confirmations, recovery, and undo use the
existing project services. Linear coordinates display computed runtime positions
while preserving serialized `-1` coordinates during drawing and depth edits.

Reference: the supplied screenshots and the pinned local LDtk 1.5.3 source under
`.reference/ldtk`, especially `Editor.hx`, `Panel.hx`, `WorldPanel.hx`,
`LevelInstanceForm.hx`, `worldPanel.html`, `levelInstanceForm.html`,
`editProject.html`, and `app.scss`.

Verification:

- The initial World-overlay regression failed before integration. Three review
  regressions reproduced the wheel transition, right-click mask bypass, and
  placement-cancel errors. A separate depth-arrow regression reproduced writes
  of runtime coordinates into a Linear world's saved coordinates. All pass with
  the fixes. Logs are in `.tools/source-toolbar-*-red.log` and
  `.tools/source-linear-depth-red.log`.
- Full workspace tests passed: application **341 passed, 6 ignored**; core and
  integration targets passed. Native-example tests: **345 passed, 6 ignored**.
  Strict all-target Clippy, rustfmt, and `git diff --check` passed.
- All nine actual toolbar clicks were exercised in native Apple Metal windows
  against `AutoLayers_1_basic.ldtk`. Each capture reached its expected panel,
  retained clean document data, had no focus mask, and reported zero surface
  errors. Every PNG was visually inspected. Inputs were injected through the
  application's native input hook; this does not verify OS mouse delivery.
- Six portable outputs were rebuilt with `just build -t win|mac|linux -m
  debug|release`; each output's 253 resource files and both license files were
  compared with the source. Current hashes are in `build-command.json`.
- The Mac application bundle was rebuilt and passed deep/strict ad-hoc signature
  verification. Native startup checks cover both Mac modes and the app bundle.

Evidence: `.tools/source-toolbar-panels/` contains native captures, their parsed
states (`native-results.json`), build logs, and startup results. Full test and
Clippy logs are `.tools/source-toolbar-{workspace-tests,example-tests,clippy}.log`.

Scope: this restores the floating interaction and the World/level/project form
structure. It is not a claim of pixel-perfect parity across every editor.
Help and definition-editor controls remained in use at this checkpoint. Application settings was subsequently restored in its own module; the broader remaining source differences are recorded in [source-ui-parity-audit.md](source-ui-parity-audit.md).
The background-image picker retains its existing select/remove operations rather
than the source's complete remembered-file menu; the project file-extension
rename toggle still needs its own validated disk-rename implementation.
Screenshots use a 1440×900 client viewport at 2× DPI, so the World form's last
button requires scrolling at that height. The updater failure badge visible in
captures is unrelated to panel rendering. Windows/Linux binaries were
cross-linked on Mac; these exact builds were not executed on target desktops.

World capture: `.tools/source-toolbar-panels/world.png`.
