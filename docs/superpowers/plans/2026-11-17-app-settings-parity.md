# Application settings parity and toolbar audit

**Goal:** Restore the source Application settings dialog and record confirmed remaining toolbar UI differences from code comparison.

**Reference:** `.reference/ldtk/app/assets/tpl/editAppSettings.html`, `ui/modal/dialog/EditAppSettings.hx`, `ui/modal/Dialog.hx`, and `app.scss` at the pinned LDtk 1.5.3 baseline.

**Scope:** A focused `application_settings.rs` module replaces the generic settings form. Preserve existing persistence, fullscreen/scale effects, log editor, update download and explicit install confirmation. Other panels receive a read-only audit; do not silently describe an opened panel as source parity.

- [x] Reproduce missing groups, columns, friendly selectors, percentage bounds and last-project clearing with real egui clicks/typing before implementation.
- [x] Restore the 940px content / 380px left column / 32px gap source layout, heading, help badges, close action and scroll behavior.
- [x] Restore source option sets: 18 UI scales, 11 intensity percentages, nearby distances Disabled/64/96/128 pixels, friendly world/field/navigation choices, and 25–300% wheel speed.
- [x] Persist and apply actual settings; disabling reopen clears last project, GPU changes show restart notice, update option uses the existing cross-platform download service with accurate help text.
- [x] Run regression/workspace/example tests, Clippy and native captures from editor and Home. Rebuild the Mac application and refresh requested portable outputs.
- [x] Record a code-linked audit of other panels, separating fixed differences, remaining migration gaps and intentional native behavior.

Success means source structure and option semantics are tested and visually inspected, not merely that the settings modal can open. The native updater supports automatic download across platforms and explicit installation; preserve that established behavior and document this difference from the upstream Windows auto-install policy.
