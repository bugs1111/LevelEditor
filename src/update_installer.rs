use super::*;
use std::{
    net::{TcpListener, TcpStream},
    process::{Command, Stdio},
    time::Instant,
};

struct Plan {
    staged: PathBuf,
    target: PathBuf,
    backup: PathBuf,
    reopen: Option<PathBuf>,
    socket: String,
    nonce: String,
}
fn path(value: &Value, name: &str) -> Result<PathBuf> {
    let path = PathBuf::from(
        value[name]
            .as_str()
            .ok_or_else(|| format!("Update plan is missing {name}"))?,
    );
    if !path.is_absolute() {
        return Err("Update plan paths must be absolute".into());
    }
    Ok(path)
}
impl Plan {
    fn read(pathname: &Path) -> Result<Self> {
        let value: Value = serde_json::from_slice(&fs::read(pathname).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let socket = value["socket"]
            .as_str()
            .ok_or("Update plan has no process handshake")?
            .to_owned();
        let address = socket
            .parse::<std::net::SocketAddr>()
            .map_err(|_| "Invalid update handshake".to_owned())?;
        if !address.ip().is_loopback() {
            return Err("Update handshake must be local".into());
        }
        let nonce = value["nonce"]
            .as_str()
            .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or("Invalid update handshake token")?
            .to_owned();
        Ok(Self {
            staged: path(&value, "staged")?,
            target: path(&value, "target")?,
            backup: path(&value, "backup")?,
            reopen: value["reopen"].as_str().map(PathBuf::from),
            socket,
            nonce,
        })
    }
    fn write(&self, path: &Path) -> Result<()> {
        let text = |path: &Path| {
            path.to_str()
                .ok_or("Installation paths must be valid Unicode")
                .map(str::to_owned)
        };
        let value = json!({"staged":text(&self.staged)?,"target":text(&self.target)?,"backup":text(&self.backup)?,"reopen":self.reopen.as_deref().map(text).transpose()?,"socket":self.socket,"nonce":self.nonce});
        fs::write(path, serde_json::to_vec(&value).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
}
fn installed_root(exe: &Path) -> Result<PathBuf> {
    let root = if cfg!(target_os = "macos") {
        exe.parent().and_then(Path::parent).and_then(Path::parent)
    } else {
        exe.parent()
    }
    .ok_or("Cannot locate the installed application")?
    .to_owned();
    let product = validate_root(&root)?;
    if fs::canonicalize(executable(&root, product)).map_err(|e| e.to_string())?
        != fs::canonicalize(exe).map_err(|e| e.to_string())?
    {
        return Err("Current executable is outside the packaged installation".into());
    }
    Ok(root)
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir(target).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        let dest = target.join(entry.file_name());
        if kind.is_symlink() {
            return Err("Update packages cannot contain symbolic links".into());
        }
        if kind.is_dir() {
            copy_tree(&entry.path(), &dest)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &dest).map_err(|e| e.to_string())?;
            OpenOptions::new()
                .write(true)
                .open(&dest)
                .and_then(|file| file.sync_all())
                .map_err(|e| e.to_string())?;
        } else {
            return Err("Update package contains a non-regular file".into());
        }
    }
    Ok(())
}
fn prepare_helper(exe: &Path, work: &Path, os: &str) -> Result<PathBuf> {
    // The macOS main executable is signed together with its bundle. It can
    // keep running while the helper renames that bundle to the backup.
    if os == "macos" {
        return Ok(exe.to_owned());
    }
    let helper = work.join(if os == "windows" {
        "level-editor-update-helper.exe"
    } else {
        "level-editor-update-helper"
    });
    fs::copy(exe, &helper).map_err(|e| e.to_string())?;
    Ok(helper)
}
pub(super) fn launch(prepared: &Prepared, reopen: Option<PathBuf>) -> Result<()> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let target = installed_root(&exe)?;
    validate_root(&prepared.root)?;
    let parent = target
        .parent()
        .ok_or("Installation has no parent directory")?;
    let nonce = format!(
        "{:x}",
        Sha256::digest(prepared.work.as_os_str().to_string_lossy().as_bytes())
    );
    let staged = parent.join(format!(".level-editor-stage-{}", &nonce[..16]));
    let backup = parent.join(format!(".level-editor-backup-{}", &nonce[..16]));
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|e| format!("Cannot start the local update handshake: {e}"))?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let plan = Plan {
        staged: staged.clone(),
        target,
        backup,
        reopen,
        socket: listener
            .local_addr()
            .map_err(|e| e.to_string())?
            .to_string(),
        nonce,
    };
    let plan_path = prepared.work.join("apply-update.json");
    // Prepare beside the installation before quitting, so permission/copy failures
    // leave the running editor and previous application untouched.
    if let Err(error) = copy_tree(&prepared.root, &staged) {
        let _ = fs::remove_dir_all(&staged);
        return Err(format!(
            "Cannot prepare this installation for replacement: {error}"
        ));
    }
    let result = (|| {
        plan.write(&plan_path)?;
        let helper = prepare_helper(&exe, &prepared.work, std::env::consts::OS)?;
        let mut command = Command::new(helper);
        command
            .arg("--apply-update")
            .arg(&plan_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command
            .spawn()
            .map_err(|e| format!("Could not start the update helper: {e}"))?;
        let started = Instant::now();
        loop {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_nonblocking(false).map_err(|e| e.to_string())?;
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .map_err(|e| e.to_string())?;
                    let mut nonce = [0; 64];
                    stream.read_exact(&mut nonce).map_err(|e| e.to_string())?;
                    if nonce.as_slice() != plan.nonce.as_bytes() {
                        continue;
                    }
                    // Keep this OS socket alive until the *process* exits. Dropping an
                    // App/viewport is not enough on Windows, where the old EXE is locked.
                    std::mem::forget(stream);
                    return Ok(());
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error.to_string()),
            }
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                return Err(format!(
                    "Update helper exited before the restart handshake ({status})"
                ));
            }
            if started.elapsed() > Duration::from_secs(15) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(
                    "Update helper did not become ready; the editor has remained open".into(),
                );
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(staged);
    }
    result
}
fn spawn_editor(exe: &Path, reopen: Option<&Path>) -> Result<()> {
    spawn_and_wait(exe, reopen, Duration::from_secs(60))
}
fn receive_startup_ack(listener: &TcpListener, nonce: &str) -> Result<bool> {
    loop {
        let mut socket = match listener.accept() {
            Ok((socket, _)) => socket,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(false),
            Err(error) => return Err(error.to_string()),
        };
        socket.set_nonblocking(false).map_err(|e| e.to_string())?;
        socket
            .set_read_timeout(Some(Duration::from_secs(1)))
            .map_err(|e| e.to_string())?;
        let mut message = [0; 64];
        if socket.read_exact(&mut message).is_ok() && message.as_slice() == nonce.as_bytes() {
            return Ok(true);
        }
    }
}
fn exited_before_ready(
    listener: &TcpListener,
    nonce: &str,
    status: std::process::ExitStatus,
) -> Result<()> {
    // A completed first frame can send its ACK after accept() said WouldBlock,
    // then exit before try_wait(). Inspect that queued ACK before rolling back.
    if receive_startup_ack(listener, nonce)? {
        Ok(())
    } else {
        Err(format!(
            "Updated editor exited before its first successful frame ({status})"
        ))
    }
}
fn spawn_and_wait(exe: &Path, reopen: Option<&Path>, timeout: Duration) -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let nonce = format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{}-{}-{:?}",
                exe.display(),
                std::process::id(),
                SystemTime::now()
            )
            .as_bytes()
        )
    );
    let mut command = Command::new(exe);
    if let Some(path) = reopen {
        command.arg(path);
    }
    if let Some(parent) = exe.parent() {
        command.current_dir(parent);
    }
    command
        .env(
            "LDTK_UPDATE_READY_ADDR",
            listener
                .local_addr()
                .map_err(|e| e.to_string())?
                .to_string(),
        )
        .env("LDTK_UPDATE_READY_NONCE", &nonce);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let started = Instant::now();
    loop {
        if receive_startup_ack(&listener, &nonce)? {
            return Ok(());
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            return exited_before_ready(&listener, &nonce, status);
        }
        if started.elapsed() >= timeout {
            if receive_startup_ack(&listener, &nonce)? {
                return Ok(());
            }
            let _ = child.kill();
            let _ = child.wait();
            return Err(
                "Updated editor did not confirm a successful first frame; startup was cancelled"
                    .into(),
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}
fn replace(plan: &Plan, restart: impl Fn(&Path, Option<&Path>) -> Result<()>) -> Result<()> {
    let product = validate_root(&plan.staged)?;
    validate_root(&plan.target)?;
    let parent = plan
        .target
        .parent()
        .ok_or("Installation has no parent directory")?;
    if plan.staged.parent() != Some(parent)
        || plan.backup.parent() != Some(parent)
        || plan.staged == plan.target
        || plan.backup == plan.target
        || plan.backup.exists()
    {
        return Err("Update replacement paths are invalid".into());
    }
    let started = Instant::now();
    loop {
        match fs::rename(&plan.target, &plan.backup) {
            Ok(()) => break,
            Err(error)
                if cfg!(windows)
                    && error.kind() == std::io::ErrorKind::PermissionDenied
                    && started.elapsed() < Duration::from_secs(30) =>
            {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(error) => return Err(format!("Could not move the previous installation: {error}")),
        }
    }
    if let Err(error) = fs::rename(&plan.staged, &plan.target) {
        fs::rename(&plan.backup,&plan.target).map_err(|restore|format!("Could not install update ({error}); restore the previous application from {} ({restore})",plan.backup.display()))?;
        return Err(format!(
            "Could not install update; previous installation restored: {error}"
        ));
    }
    if let Err(error) = restart(&executable(&plan.target, product), plan.reopen.as_deref()) {
        fs::rename(&plan.target, &plan.staged)
            .map_err(|e| format!("New executable did not start ({error}); rollback failed: {e}"))?;
        fs::rename(&plan.backup, &plan.target).map_err(|e| {
            format!(
                "New executable did not start ({error}); previous application is at {}: {e}",
                plan.backup.display()
            )
        })?;
        return Err(format!(
            "The updated editor could not start. Previous installation restored: {error}"
        ));
    }
    let _ = fs::remove_dir_all(&plan.backup);
    Ok(())
}
/// Returns false for an ordinary editor launch; this runs before any native UI.
pub fn apply_update_cli() -> Option<Result<()>> {
    let args = std::env::args_os().collect::<Vec<_>>();
    if args.get(1).is_none_or(|arg| arg != "--apply-update") {
        return None;
    }
    let Some(path) = args.get(2).map(PathBuf::from) else {
        return Some(Err("Missing update plan".into()));
    };
    let result = run_plan(&path);
    if let Err(error) = &result
        && !path.with_file_name("apply-error.seen").exists()
    {
        let _ = fs::write(path.with_file_name("apply-error.txt"), error);
    }
    Some(result)
}
fn run_plan(path: &Path) -> Result<()> {
    let plan = Plan::read(path)?;
    let address = plan
        .socket
        .parse()
        .map_err(|_| "Invalid handshake address".to_owned())?;
    let mut stream =
        TcpStream::connect_timeout(&address, Duration::from_secs(10)).map_err(|e| e.to_string())?;
    stream
        .write_all(plan.nonce.as_bytes())
        .map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
        .map_err(|e| e.to_string())?;
    let mut byte = [0];
    match stream.read(&mut byte) {
        Ok(0) => {}
        Ok(_) => return Err("Invalid parent-process handshake".into()),
        Err(_) => return Err("Editor did not exit; update was not installed".into()),
    }
    let result = replace(&plan, spawn_editor);
    if let Err(error) = &result {
        let mut error = error.clone();
        let _ = fs::write(path.with_file_name("apply-error.txt"), &error);
        let restart = validate_root(&plan.target).and_then(|product| {
            spawn_editor(&executable(&plan.target, product), plan.reopen.as_deref())
        });
        if let Err(restart) = restart {
            error.push_str(&format!(
                " Previous editor also failed to restart: {restart}"
            ));
        }
        let _ = fs::write(path.with_file_name("apply-error.txt"), &error);
        return Err(error);
    } else if let Some(work) = path.parent()
        && safe_work(work)
    {
        let _ = fs::write(work.join("cleanup-pending"), b"installed");
        cleanup_pending_work();
    }
    result
}
pub(super) fn prior_failure() -> Option<String> {
    let mut paths = fs::read_dir(std::env::temp_dir())
        .ok()?
        .filter_map(std::result::Result::ok)
        .filter(|entry| is_update_work_name(&entry.file_name()))
        .map(|entry| entry.path().join("apply-error.txt"))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    paths.sort_by_key(|path| fs::metadata(path).and_then(|m| m.modified()).ok());
    let path = paths.pop()?;
    let error = fs::read_to_string(&path).ok()?;
    let _ = fs::rename(&path, path.with_extension("seen"));
    Some(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn macos_helper_runs_in_the_installed_app_alongside_its_signature_resources() {
        let dir = tempfile::tempdir().unwrap();
        let contents = dir.path().join("Level Editor.app/Contents");
        let installed = contents.join("MacOS/level-editor");
        fs::create_dir_all(installed.parent().unwrap()).unwrap();
        fs::write(&installed, b"packaged executable").unwrap();
        assert_eq!(
            prepare_helper(&installed, dir.path(), "macos").unwrap(),
            installed
        );
    }
    fn package(root: &Path, bytes: &[u8]) {
        package_with_product(root, bytes, PRODUCT);
    }
    fn package_with_product(root: &Path, bytes: &[u8], product: &str) {
        let binary = executable(root, product);
        let marker = marker_path(root, product);
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        fs::create_dir_all(marker.parent().unwrap()).unwrap();
        fs::write(binary, bytes).unwrap();
        fs::write(marker, json!({"product": product, "layout": 1}).to_string()).unwrap();
    }
    #[test]
    fn replacement_restarts_the_new_executable_and_removes_only_its_backup() {
        let dir = tempfile::tempdir().unwrap();
        let plan = Plan {
            staged: dir.path().join("stage"),
            target: dir.path().join("installed"),
            backup: dir.path().join("backup"),
            reopen: Some(dir.path().join("my project.ldtk")),
            socket: "127.0.0.1:1".into(),
            nonce: "0".repeat(64),
        };
        package(&plan.target, b"old");
        package(&plan.staged, b"new");
        replace(&plan, |exe, reopen| {
            assert_eq!(fs::read(exe).unwrap(), b"new");
            assert_eq!(reopen, plan.reopen.as_deref());
            Ok(())
        })
        .unwrap();
        assert_eq!(fs::read(executable(&plan.target, PRODUCT)).unwrap(), b"new");
        assert!(!plan.backup.exists());
    }
    #[test]
    fn restart_failure_restores_the_previous_installation() {
        let dir = tempfile::tempdir().unwrap();
        let plan = Plan {
            staged: dir.path().join("stage"),
            target: dir.path().join("installed"),
            backup: dir.path().join("backup"),
            reopen: None,
            socket: "127.0.0.1:1".into(),
            nonce: "0".repeat(64),
        };
        package_with_product(&plan.target, b"old", LEGACY_PRODUCT);
        package(&plan.staged, b"new");
        assert!(
            replace(&plan, |_, _| Err("fixture process could not start".into()))
                .unwrap_err()
                .contains("restored")
        );
        assert_eq!(
            fs::read(executable(&plan.target, LEGACY_PRODUCT)).unwrap(),
            b"old"
        );
        assert_eq!(fs::read(executable(&plan.staged, PRODUCT)).unwrap(), b"new");
    }
    #[test]
    fn development_binaries_cannot_replace_their_containing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join(PRODUCT);
        fs::write(&exe, b"dev").unwrap();
        assert!(installed_root(&exe).is_err());
    }
    #[test]
    fn installation_copy_flushes_executable_and_resources_and_preserves_an_existing_target() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("package");
        let target = dir.path().join("copied");
        package(&source, b"packaged executable");
        fs::create_dir_all(source.join("assets")).unwrap();
        fs::write(source.join("assets/icon.png"), b"fixture icon").unwrap();
        copy_tree(&source, &target).unwrap();
        assert_eq!(
            fs::read(executable(&target, PRODUCT)).unwrap(),
            b"packaged executable"
        );
        assert_eq!(
            fs::read(target.join("assets/icon.png")).unwrap(),
            b"fixture icon"
        );
        fs::write(executable(&target, PRODUCT), b"existing application").unwrap();
        assert!(copy_tree(&source, &target).is_err());
        assert_eq!(
            fs::read(executable(&target, PRODUCT)).unwrap(),
            b"existing application"
        );
    }
    #[test]
    fn fixture_helper_child() {
        if let Some(path) = std::env::var_os("LDTK_UPDATE_FIXTURE_PLAN") {
            run_plan(Path::new(&path)).unwrap();
        }
    }
    #[test]
    fn fixture_restarted_editor() {
        if let Some(path) = std::env::var_os("LDTK_UPDATE_FIXTURE_RESTART") {
            super::super::send_ready().unwrap();
            fs::write(path, b"Rust process restarted").unwrap();
        }
    }
    #[test]
    fn fixture_exits_without_a_ready_frame() {}
    #[test]
    fn fixture_sends_invalid_startup_message() {
        if let Ok(message) = std::env::var("LDTK_UPDATE_FIXTURE_MESSAGE") {
            let mut socket =
                TcpStream::connect(std::env::var("LDTK_UPDATE_READY_ADDR").unwrap()).unwrap();
            socket.write_all(message.as_bytes()).unwrap();
        }
    }
    fn completed_startup_message(message: Option<&str>) -> (TcpListener, std::process::ExitStatus) {
        let dir = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                if message.is_some() {
                    "updater::installer::tests::fixture_sends_invalid_startup_message"
                } else {
                    "updater::installer::tests::fixture_restarted_editor"
                },
            ])
            .env(
                "LDTK_UPDATE_READY_ADDR",
                listener.local_addr().unwrap().to_string(),
            )
            .env("LDTK_UPDATE_READY_NONCE", "a".repeat(64))
            .env("LDTK_UPDATE_FIXTURE_RESTART", dir.path().join("restarted"))
            .stdout(Stdio::null());
        if let Some(message) = message {
            command.env("LDTK_UPDATE_FIXTURE_MESSAGE", message);
        }
        let status = command.spawn().unwrap().wait().unwrap();
        assert!(status.success());
        (listener, status)
    }
    #[test]
    fn a_queued_first_frame_ack_wins_when_the_process_exits_before_the_status_poll() {
        let (listener, status) = completed_startup_message(None);
        exited_before_ready(&listener, &"a".repeat(64), status).unwrap();
    }
    #[test]
    fn an_exited_process_with_invalid_or_truncated_ack_is_still_rejected() {
        for message in ["b".repeat(64), "a".repeat(12)] {
            let (listener, status) = completed_startup_message(Some(&message));
            assert!(
                exited_before_ready(&listener, &"a".repeat(64), status)
                    .unwrap_err()
                    .contains("exited before its first successful frame")
            );
        }
    }
    #[test]
    fn an_immediately_exiting_updated_process_is_not_accepted_as_a_successful_restart() {
        let dir = tempfile::tempdir().unwrap();
        let plan = Plan {
            staged: dir.path().join("stage"),
            target: dir.path().join("installed"),
            backup: dir.path().join("backup"),
            reopen: Some(PathBuf::from("fixture_exits_without_a_ready_frame")),
            socket: "127.0.0.1:1".into(),
            nonce: "0".repeat(64),
        };
        package(&plan.target, b"old installation");
        package(&plan.staged, b"new");
        fs::copy(
            std::env::current_exe().unwrap(),
            executable(&plan.staged, PRODUCT),
        )
        .unwrap();
        let error = replace(&plan, |exe, reopen| {
            spawn_and_wait(exe, reopen, Duration::from_secs(5))
        })
        .unwrap_err();
        assert!(error.contains("exited before its first successful frame"));
        assert_eq!(
            fs::read(executable(&plan.target, PRODUCT)).unwrap(),
            b"old installation"
        );
        assert!(!plan.backup.exists());
    }
    #[test]
    fn update_helper_waits_for_process_socket_close_then_replaces_and_restarts() {
        exercise_update_helper(None);
    }
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires an extracted, notarized release ZIP"]
    fn notarized_release_helper_confirms_restart_handshake() {
        let bundle = std::env::var_os("LEVEL_EDITOR_TEST_NOTARIZED_BUNDLE").unwrap();
        exercise_update_helper(Some(Path::new(&bundle)));
    }
    fn exercise_update_helper(release_bundle: Option<&Path>) {
        let dir = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let restart = dir.path().join("restarted.txt");
        let plan = Plan {
            staged: dir.path().join("Stage.app"),
            target: dir.path().join("Level Editor.app"),
            backup: dir.path().join("backup"),
            reopen: Some(PathBuf::from("fixture_restarted_editor")),
            socket: listener.local_addr().unwrap().to_string(),
            nonce: "1".repeat(64),
        };
        package(&plan.staged, b"new installation");
        let test_binary = std::env::current_exe().unwrap();
        fs::copy(&test_binary, executable(&plan.staged, PRODUCT)).unwrap();
        #[cfg(target_os = "macos")]
        {
            fs::write(
                plan.staged.join("Contents/Info.plist"),
                include_bytes!("../scripts/macos-info.plist"),
            )
            .unwrap();
            assert!(
                Command::new("/usr/bin/codesign")
                    .args(["--force", "--sign", "-", "--options", "runtime"])
                    .arg(&plan.staged)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let source = if cfg!(target_os = "macos") {
            let bundle = release_bundle.unwrap_or(&plan.staged);
            copy_tree(bundle, &plan.target).unwrap();
            executable(&plan.target, PRODUCT)
        } else {
            package(&plan.target, b"old installation");
            test_binary
        };
        let helper = prepare_helper(&source, dir.path(), std::env::consts::OS).unwrap();
        let plan_path = dir.path().join("apply.json");
        plan.write(&plan_path).unwrap();
        let mut command = Command::new(helper);
        if release_bundle.is_some() {
            command.arg("--apply-update").arg(&plan_path);
        } else {
            command
                .args(["--exact", "updater::installer::tests::fixture_helper_child"])
                .env("LDTK_UPDATE_FIXTURE_PLAN", &plan_path);
        }
        let mut child = command
            .env("LDTK_UPDATE_FIXTURE_RESTART", &restart)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let start = Instant::now();
        let mut socket = loop {
            if let Ok((stream, _)) = listener.accept() {
                break stream;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "update helper did not connect"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut nonce = [0; 64];
        socket.read_exact(&mut nonce).unwrap();
        assert_eq!(nonce.as_slice(), plan.nonce.as_bytes());
        assert!(plan.target.exists());
        assert!(plan.staged.exists());
        assert!(!plan.backup.exists());
        assert!(
            !restart.exists(),
            "helper cannot install while the parent socket is alive"
        );
        drop(socket);
        assert!(child.wait().unwrap().success());
        let start = Instant::now();
        while !restart.exists() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "updated Rust process did not restart"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(fs::read(restart).unwrap(), b"Rust process restarted");
        assert!(!plan.staged.exists());
        assert!(!plan.backup.exists());
    }
}
