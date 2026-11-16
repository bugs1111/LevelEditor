# Independent native acceptance review

Review date: 2026-11-16. Source oracle: `.reference/ldtk` commit `2b7b5512f2f88fa281cb6492debbb26ead7838d4`. Reviewed current call paths in `app.rs`, editor services/hooks/history/pointer handling, level navigation, ruler resizing, field editors and pickers, selection handling, command palette, Help and native file/export entries. This is a source/call-path review, not a claim of complete visual or interaction parity. Production and test files were not modified by this reviewer; the root author is integrating fixes concurrently. No full-workspace test run was started by this reviewer.

## Blocking findings sent to the integration owner

### P1 — Level navigation retained an entity index from the previous level

Trigger: select an entity in level A, open the command palette, choose level B, then press Delete. `search.rs`'s `Destination::Level` replaced `selected` and `layer` but retained `entity_index`. `selection_tools.rs::delete_selection` then called `remove_entity_with_references` on B/current layer with that old index. B's entity at the same array index was removed without being selected there. `chrome.rs::world_navigation` also replaced levels/worlds without clearing the level-specific entity selection, selection group or picker. Search omitted `active_world` and `world_depth` synchronization, so subsequent world controls and ruler input could address an inconsistent view.

Original `page/Editor.hx::selectLevel` (1372 onward) stops the running tool, selects the target world and emits `LevelSelected`; original tools consume that event. Native `navigation.rs::wheel_zoom` already demonstrated appropriate selection cleanup in its own navigation route. Integration owner acknowledged the finding and is adding a common level-selection route and regression coverage. **Re-inspection of the final fix and its test evidence is still required.**

### P1 — Buffered input could be omitted from Save or discarded by Close/New

Trigger: focus a String/property text field in a clean saved project, type a new value without Enter/blur, then press Cmd/Ctrl+S or Cmd/Ctrl+W. `ui_fields.rs::buffered_text_kind` stores only an egui draft until Enter/blur. `shortcuts.rs` permits the save/close actions while text has focus. Save clones the unchanged Project in `hooks.rs::save_async_at`, and `dialogs` returns early while saving; no owner editor runs to apply the draft first. Close/New checks only `dirty`, which has not changed for the draft, and can discard it without an unsaved-changes prompt.

This was established from the current input/action ordering (`update_frame`: shortcuts before dialogs) and draft ownership; a new runtime regression was not executed by this reviewer. A fix must apply the owner editor's draft **before** the save snapshot or dirty/close decision; surrendering focus and taking the snapshot in the same frame is insufficient. Original `page/Editor.hx::onClose` closes modals before checking `needSaving`, and `ui/Modal.hx:264–265` explicitly blurs focused input to validate edits before closing. Integration owner was notified. **Save, close, new/open and closing the property panel need an actual focused-input regression.**

### P2 — Cross-level EntityRef canvas picking and following references were incomplete

Original `ui/vp/EntityRefPicker.hx` permits level navigation for an out-of-level reference, picks against the current level (103–116), and returns to the source after picking/canceling. Native `field_tools.rs::field_pick_input` selected hit-test levels using the stored source `pick.key`: non-world mode used only that source level, and world mode used the source world's placements. Navigating to another level/world therefore did not make its entities pickable through this workflow. Clearing all pick state during a generic navigation fix would also prevent this original workflow.

The native EntityRef combo box can assign references across levels, so this finding is specifically the missing canvas workflow, not an assertion that all cross-level assignment is impossible. `ui_fields.rs` also lacked an action to follow the current reference; original `ui/FieldInstancesForm.hx:540–550` invokes `Editor.followEntityRef`. Integration owner was notified. **Re-inspect the final picker/navigation integration, including source restoration and permitted-target rules.**

### P2 — Command palette omitted searchable worlds/external enums and keyword matching

Original `ui/CommandPalette.hx:120` combines internal/external enums; 151–159 provides world results; 211–249 adds category/context keywords and matches whitespace-separated query terms. Native `search.rs` had no world result at all despite its heading, omitted external enums, and compared the entire query as one substring of a definition name. Consequently source-supported queries such as `entity Player` and world-context level searches did not work. Integration owner acknowledged this and is implementing worlds, external enums and token matching. **Re-inspection and targeted result-selection tests are still required.**

## Additional verified coverage differences

- Help (`app.rs::dialogs`, `Panel::Help`) displayed only a short shortcut list. Original `ui/modal/panel/Help.hx` and its template include documentation/JSON documentation, community/home links, changelog and tutorial links. This is a real incomplete Help entry, but was not prioritized above the data-loss and navigation failures.
- Native settings preserve `uiStates`, `lastUiDirs` and `recentDirColors` as JSON keys but have no functional readers/writers for them beyond defaults. Original `electron.common/Settings.hx:194–312` owns project-specific UI state and per-picker directory memory; `ui/Tileset.hx:46–117` uses state for atlas fit; `App.hx:914–942` owns recent-folder colors. Their existence in settings JSON is **not** evidence that their workflows are implemented. Native recent project/folder lists themselves have real open actions (`library.rs`). These persistence/presentation differences do not establish a project-save corruption bug.
- The Export entry has real buttons for simplified, PNG, CSV, TMX and GameMaker, each choosing a directory and calling `export_async`; the worker dispatches the matching core exporter. Import Ogmo routes through the unsaved-changes request flow. This review found no evidence supporting a claim that those menu entries were mere placeholders. It did not independently repeat filesystem output-oracle acceptance.

## Checks that did not establish a new blocker

- The new ruler route is wired ahead of canvas editing, uses preview state, and commits through level history; `finish_gesture` consumes it on release/focus loss and save/navigation. This inspection did not find a new recursion or ownership bug in that integration.
- Hooks have explicit BeforeSave → async save → AfterSave → pending-action continuation. Failed commands remain visible until closed; the current code continues the source-style save sequence afterward. No additional concrete hook failure was established here.
- Selection groups validate their owning level IID in core before delete/move. Thus a stale group alone fails with a level-mismatch error; the destructive navigation finding above specifically concerns the separate plain entity index fallback.
- Existing findings in `remaining-parity-audit.md` were treated as historical until rechecked. Implemented Point gestures, palette pick reset, automatic world zoom, names, world depth hits/snapping, rulers and field buffering were not re-reported merely because old tables described them as absent.

Passing algorithm/unit tests cannot close the remaining visual, whole-workflow or smoothness acceptance. In particular, focused-input action ordering and cross-level navigation are integration behaviors and need actual egui/native event replay. This report does not certify complete parity.

## Integration disposition after the review

The findings above are retained as the discovery snapshot. They are no longer a list of unimplemented native entries. The integration owner subsequently added common level/world navigation with selection cleanup; deferred whole-owner navigation/search/follow; actual save/close/panel/input-batch regressions; cross-level/world canvas reference picking and return; searchable worlds/external enums and AND keyword matching; source Help/changelog; project-specific UI state, directory memory and recent-directory colors. These implementations are covered in the native modules' input regressions, `ui_commit`, `modal_workflow_tests`, `field_tools_tests`, `search` and `settings` tests.

Later focused-input review also found same-batch text/pair insertion and deferred Follow/Search owner changes. Those were reproduced and corrected; the new multiline field route commits before same-batch Cmd/Ctrl+S. Home backup routing now binds the actual original path even when another copy has the same project IID. Repeated Ogmo import preserves an existing destination by choosing an unused name. Final integrated commands and results are recorded in `final-verification.md`; those commands do not retroactively turn this discovery report into an independent visual-parity certification.
