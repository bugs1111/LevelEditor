# Original toolbar floating panels implementation plan

**Goal:** Match the original LDtk 1.5.3 toolbar's floating-panel behavior and the supplied WorldPanel screenshot.

**Reference:** The user's three screenshots; `.reference/ldtk` at the existing locked baseline, especially `page/Editor.hx`, `ui/modal/Panel.hx`, `WorldPanel.hx`, `worldPanel.html`, `levelInstanceForm.html` and their SCSS.

**Approach:** Keep the existing editor sidebar beneath floating panels. Entering world mode from its toolbar/shortcut opens the WorldPanel; closing it exits world mode. World, current-level and project forms use their original single-column layouts rather than the invented definition-navigation list. Existing layers/entities/enums/tilesets editors keep their dedicated definition layouts. Reuse current validated edit/history/confirmation services.

**Ownership:** Root owns toolbar/shortcuts/panel lifecycle, common floating shell, integration tests and screenshots. Independent agents own only the World/level form module and project form module respectively.

- [x] Add failing real-egui tests for every toolbar destination, World floating geometry, unchanged base sidebar, and close/toggle behavior.
- [x] Restore source World/level form sections, labels, paired controls, copy buttons, level actions and custom fields using source templates.
- [x] Restore project form's single-column structure and action row; preserve existing feature behavior.
- [x] Integrate source panel widths, placement, dimming/input policy, close behavior and world toolbar offset; keep field-draft commit ordering.
- [x] Verify actual World/level data edits, confirmation/cancel, dropdowns and palette/canvas isolation; run workspace/example tests, strict Clippy and formatting.
- [x] Capture and inspect real native screenshots for all main toolbar panels, including the same AutoLayers world example; rebuild the Mac application and record remaining scope accurately.

Result and remaining fidelity scope: `docs/status/source-toolbar-panels.md`.

World width is 400 logical pixels with its own 16px inner scroller. Current-level source content is 450px plus general panel margins. Preserve viewport/DPI distinctions when comparing the user's OS-framed screenshot against client-area captures. No claim of complete pixel parity follows from state tests alone.
