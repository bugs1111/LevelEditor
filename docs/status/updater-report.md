# Native Rust update channel

Source: `bugs1111/LevelEditor` on GitHub. This repository remains private. Update checks use its latest **published stable** release, not upstream Electron installers or draft releases.

The native flow follows the original `App.hx` update entry: background check, available/download notification, progress, ready-to-install notification, unsaved-document guard, then the updating page and restart. `Automatically download updates` consumes the original `autoInstallUpdates` setting; installation always requires the explicit Install action. The Settings Check button also opens the native update dialog. Cancel/check/download, errors and a Releases link remain available.

## Release contract

New releases publish `level-editor-updates.json`. The updater also reads `ldtk-rs-updates.json` from older releases.

```json
{
  "version": "<version>",
  "targets": {
    "aarch64-apple-darwin": {
      "asset": "level-editor-v<version>-aarch64-apple-darwin.zip",
      "sha256": "64 lowercase hexadecimal characters",
      "size": 123456
    }
  }
}
```

Other supported target keys are `x86_64-apple-darwin`, `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu`. macOS ZIPs contain `LDtk Rust.app/`; Windows/Linux ZIPs contain `ldtk-rs/`. A packaged installation contains `{"product":"ldtk-rs","layout":1}` in `Contents/Resources/ldtk-rs-install.json` on macOS or `ldtk-rs-install.json` beside the other platforms' executable. Development/cargo binaries cannot replace their enclosing build directory.

GitHub's asset API URL with `Accept: application/octet-stream` supports private downloads. Authentication is read from `GH_TOKEN`/`GITHUB_TOKEN` or a password-masked session input. Tokens are not embedded, saved to settings, included in an install plan or printed to logs. Redirects are followed explicitly with automatic redirects disabled; Authorization is attached only to the original API origin and is dropped for object-storage redirects.

Metadata/OS/architecture/version/asset-size checks precede the download. The worker streams the ZIP to disk, verifies exact byte count and SHA-256, rejects unsafe paths/symlinks/unexpected archive roots, bounds expansion and restores executable permissions. Download/cancel/error paths do not install anything.

## Replacement and restart

The worker copies the current Rust executable to a temporary helper and prepares the verified application beside the installation. A loopback handshake confirms the helper is running; the parent retains its socket until actual process exit. The helper waits for EOF before replacing the old directory, with additional Windows file-lock retries. It renames the previous application to a backup, publishes the staged package and starts its new executable with the current project path. The new process sends a separate local nonce ACK after its first successful editor `update_frame`. The backup is retained until that ACK arrives. Replacement failure, immediate process exit or a 60-second startup-confirmation timeout stops the new process and restores the previous application; the restored application also confirms startup. Errors are recorded before restarting the previous application so its update dialog can display them.

This is an application-directory replacement under the current user's existing permissions. It does not elevate permissions, alter file associations, install an Electron package or update a real installation during verification. The first editor-frame ACK is a bounded startup check, not a guarantee that every later frame, GPU presentation or project operation will succeed.

Ready stages are cleaned when replaced by a fresh check, dropped or closed normally. Closing also signals cancellation for checks/downloads; installing/restarting never deletes a live helper workspace. A successful helper marks its own temporary workspace for scoped cleanup. The new process retries this cleanup while a Windows helper executable remains locked. Sudden process termination during active downloading/unpacking can leave an incomplete temporary directory; this implementation does not introduce a general cache/recovery service for that case.

## Verification

- Core/helper checks cover a real local HTTP feed, private-asset headers, semantic-version precedence excluding build metadata, misflagged prerelease rejection, absent/private release errors, cross-origin credential stripping, SHA/size failure, cancellation during a slow metadata response, ZIP traversal rejection, Ready-stage cleanup and direct installation copying. Temporary-process checks cover replacement rollback, an immediately exiting executable without a startup ACK, and a copied Rust helper that waits before replacement and starts a new Rust process which confirms readiness. The copy flush opens a writable handle, as required by Windows `FlushFileBuffers`.
- Three actual egui regressions passed: Ready → Install closes its modal before the clickable Save/Discard/Cancel guard; a focused String draft commits before Install's dirty decision; session credentials are password-masked and absent from settings/rendered text. The modal-ordering failure was observed before its fix. Installation is intercepted in these UI tests, while Save writes only a temporary project.
- Local HTTP and helper tests need loopback permission in the restricted desktop sandbox. The initial `EPERM` bind failures were environmental; the same tests passed with that permission. The test uses fixture credentials and temporary application trees, never the installed editor.
- Full workspace/target checks and first published-feed verification are recorded in the final integration report. Windows/Linux compilation is distinct from native updater runtime verification on those operating systems.
- `updater::tests::live_private_release_downloads_verifies_and_stages_without_installing` is explicitly ignored by ordinary tests. On 2026-11-17 it was run explicitly against published private release `v0.1.0`: **1 passed, 0 failed**. With network permission and a private-repository token only in the child environment, it checked from acceptance version `0.0.0`, downloaded the real private manifest and ARM macOS archive, verified SHA/size/layout and removed its own stage. It never invoked installation. Evidence: `.tools/live-private-release-acceptance.log` and [platform-acceptance.json](platform-acceptance.json). Both published macOS bundles also passed strict signature verification, and the downloaded ARM bundle passed isolated cold/warm document opening.

For 0.1.1, all four native build/test/package jobs and the separate six-scene GUI/three-test GPU matrix passed. The four CI ZIPs and merged update manifest are verified locally; both macOS signatures and the CI ARM application's isolated cold/warm opening passed. Automatic approval review rejected uploading these packages and creating the stable GitHub Release, including after prior-chat authorization was supplied. The latest published release remains v0.1.0. Creating v0.1.1 and repeating the live private-feed download/stage/cleanup test against that new release remain pending explicit current-user approval. The existing v0.1.0 live result is not presented as a v0.1.1 published-feed result. See [release-0.1.1-acceptance.json](release-0.1.1-acceptance.json).
