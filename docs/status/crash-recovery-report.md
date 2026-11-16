# In-memory crash recovery

## API and behavior

`Project::write_crash_recovery(&self, fallback_root: impl AsRef<Path>) -> Result<PathBuf>` returns the newly written recovery **project file**, not its directory.

The implementation is isolated in `crates/ldtk-core/src/project_crash.rs`, registered as a child module of `project.rs`. It reads no saved project content and does not invoke ordinary disk backup, document save, custom commands, exports, or backup rotation. It serializes current root JSON and referenced loaded external levels, updates the external summaries in the snapshot, and leaves the caller's project and original files unchanged.

Saved projects use their configured backup directory or `<project-stem>/backups`. Names follow `<iid>_<YYYY-MM-DD_HH-MM-SS>_crash`; collisions receive the existing recognized `_NNNN` suffix. Timestamp generation reuses the existing core UTC helper; original LDtk formats local time, so the filename grammar matches but time-zone display follows the native core's existing convention.

For a project without a path, `fallback_root/Untitled.ldtk` is its virtual source location; the default backup directory is `fallback_root/Untitled/backups`. A configured `backupRelPath`, if present, is resolved relative to that virtual source directory. Relative resources from an unsaved project retain their working-directory resolution. To recover, call `Project::load_backup(recovery_file, fallback_root.join("Untitled.ldtk"))`. The virtual source file is never created. A clone with that virtual source assigned can use the existing `list_backups`; `list_backups` on the original pathless project retains its existing empty result.

The existing `list_backups` recognizes `_crash` entries and `load_backup` rebases tileset, background and file-field resources against the original source location. External levels with parent traversal are remapped inside the recovery directory. Referenced but unloaded levels and duplicate outputs fail before creating a backup directory. New recovery directories are claimed with exclusive creation, and write failures remove only that newly created directory. Root publication uses the existing atomic staging helper after external files.

## Source evidence

- Pinned upstream `App.hx` lines 108–118 passes the dirty in-memory project into the crash page.
- `page/CrashReport.hx` lines 94–124 creates `makeBackupDirName("crash")`, calls `prepareProjectSavingData`, and writes the returned root and external JSON directly.
- `data/Project.hx` lines 84–100 defines the default/configured backup root and suffix naming.
- `ui/ProjectSaver.hx` lines 667–712 separates external documents from root summaries. The native implementation snapshots the existing loaded JSON rather than running normal save/export operations during recovery.

## Verification

The three new real-filesystem tests first failed against the explicit unimplemented API (`/private/tmp/crash-red.log`), then passed (`/private/tmp/crash-green.log`). They cover:

- Unsaved root and external changes overriding older disk content; unchanged source bytes, path, root JSON and loaded external map; repeated distinct crash snapshots even with backup limit zero; recognized crash listings; root summary synchronization; loaded resource rebasing; absence of configured exports.
- A never-saved inline project, fallback location, recovery recognition and relative-resource preservation without creating the virtual original.
- Missing loaded external data failing without publishing a backup, and a parent-relative external level being contained and recoverable.

Strict core Clippy passes (`/private/tmp/crash-clippy.log`). The default native test command currently reports 78 passed, 3 failed, 4 ignored. Failures are concurrent app/history work, outside this change: `os_paste_event_in_entity_panel_is_one_undoable_insert`, `native_add_field_chooses_type_before_mutating_and_can_undo`, and `select_all_copies_and_pastes_visible_layers_together_as_one_undo` (`/private/tmp/crash-workspace-tests.log`). They were reported to the owning agents rather than hidden or changed here.

Explicit full core verification passed 206 tests, with 13 resource/GPU/benchmark tests explicitly ignored (`/private/tmp/crash-core-tests.log`).

## Native integration

`src/editor/crash.rs` provides `EditorApp::capture_crash(message, fallback)` and `crash_dialog(ctx)`. The root agent registered the module and owns the update catch/unwind and Drop guards. Capture is idempotent and writes only for a dirty project or an in-progress stroke/gesture (whose JSON can already differ before history commit). It stops transient interaction and command continuations without calling gesture cancel/finish, preserving the exact current document. Incomplete undo state is discarded; the dialog explains that restart is recommended and undo was reset. A successful snapshot is persisted in recent projects. Recovery write failures retain the dirty memory and display the original fault plus the error. Close app explicitly bypasses normal save/hooks; Keep window dismisses the crash state.

Five real native integration tests were observed failing with stubs (`/private/tmp/native-crash-red.log`) and then passing (`/private/tmp/native-crash-green.log`). They verify dirty memory and persisted recents, command cancellation and repeated-capture idempotence, clean crash and ordinary dirty Drop not creating snapshots, current entity drag position before dirty/history commit, real panic-unwind Drop recovery, and a failing recovery destination retaining memory. Native full verification now passes **95 tests / 4 explicit ignores** (`/private/tmp/native-crash-full.log`), resolving the earlier unrelated in-progress failures. Strict native/smoke Clippy also passes (`/private/tmp/native-crash-clippy.log`). Existing asynchronous filesystem work cannot be retroactively canceled; its future UI completion is detached so it cannot replace the recovered document or start further hook continuations.
