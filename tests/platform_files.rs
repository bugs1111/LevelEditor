#[path = "../src/platform_files.rs"]
mod platform_files;

fn main() {
    #[cfg(target_os = "macos")]
    if std::env::args().nth(1).as_deref() == Some("--open-file-probe") {
        mac::run_probe(std::path::Path::new(&std::env::args_os().nth(2).unwrap()));
        return;
    }
    bundle_declares_project_documents_without_claiming_external_levels();
    #[cfg(target_os = "macos")]
    {
        mac::open_documents_survive_startup_and_wake_the_editor();
        mac::native_quit_reaches_the_editor();
        // LaunchServices needs an unsandboxed GUI session. The default test
        // still exercises real AppleEvent delivery to this isolated process.
        if std::env::args().any(|arg| arg == "--bundle-open") {
            mac::bundle_cold_and_warm_launch_deliver_project_documents();
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _guard = platform_files::install();
        platform_files::set_context(&eframe::egui::Context::default());
        assert!(platform_files::take_open_file().is_none());
    }
    println!("platform_files: all tests passed");
}

fn bundle_declares_project_documents_without_claiming_external_levels() {
    let plist = std::process::Command::new(if cfg!(windows) { "python" } else { "python3" })
        .args([
            "-c",
            "import json,plistlib,sys; print(json.dumps(plistlib.load(open(sys.argv[1],'rb'))))",
        ])
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/macos-info.plist"
        ))
        .output()
        .unwrap();
    assert!(plist.status.success());
    let value: serde_json::Value = serde_json::from_slice(&plist.stdout).unwrap();
    assert_eq!(value["CFBundleIdentifier"], "io.ldtk.rust-port");
    let documents = value["CFBundleDocumentTypes"]
        .as_array()
        .expect("project document association");
    assert_eq!(documents.len(), 1);
    assert_eq!(documents[0]["CFBundleTypeRole"], "Editor");
    assert_eq!(documents[0]["LSHandlerRank"], "Alternate");
    assert_eq!(
        documents[0]["LSItemContentTypes"],
        serde_json::json!(["io.ldtk.project"])
    );
    let types = value["UTImportedTypeDeclarations"]
        .as_array()
        .expect("source LDtk format is imported");
    assert_eq!(types.len(), 1);
    assert_eq!(types[0]["UTTypeIdentifier"], "io.ldtk.project");
    assert_eq!(
        types[0]["UTTypeTagSpecification"]["public.filename-extension"],
        serde_json::json!(["ldtk"])
    );
    assert_eq!(
        types[0]["UTTypeConformsTo"],
        serde_json::json!(["public.json"])
    );
}

#[cfg(target_os = "macos")]
mod mac {
    use super::platform_files;
    use objc2::{
        DefinedClass, MainThreadOnly, class, define_class, msg_send, runtime::AnyObject, sel,
    };
    use objc2_app_kit::{NSApplication, NSApplicationDelegate, NSApplicationTerminateReply};
    use objc2_foundation::{
        MainThreadMarker, NSAppleEventDescriptor as Descriptor, NSAppleEventSendOptions, NSObject,
        NSObjectProtocol, NSString, NSURL,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[link(name = "AppKit", kind = "framework")]
    unsafe extern "C" {}

    fn send(paths: &[&std::path::Path]) {
        let target = Descriptor::currentProcessDescriptor();
        let event =
            Descriptor::appleEventWithEventClass_eventID_targetDescriptor_returnID_transactionID(
                u32::from_be_bytes(*b"aevt"),
                u32::from_be_bytes(*b"odoc"),
                Some(&target),
                -1,
                0,
            );
        let list = Descriptor::listDescriptor();
        for (index, path) in paths.iter().enumerate() {
            let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str().unwrap()));
            list.insertDescriptor_atIndex(
                &Descriptor::descriptorWithFileURL(&url),
                (index + 1) as isize,
            );
        }
        event.setParamDescriptor_forKeyword(&list, u32::from_be_bytes(*b"----"));
        event
            .sendEventWithOptions_timeout_error(
                NSAppleEventSendOptions::WaitForReply | NSAppleEventSendOptions::NeverInteract,
                2.0,
            )
            .expect("real self-targeted open-documents AppleEvent must reach registered handler");
    }

    pub fn open_documents_survive_startup_and_wake_the_editor() {
        // A separate test executable owns this AppKit instance; the user's editor is never targeted.
        let _: *mut AnyObject = unsafe { msg_send![class!(NSApplication), sharedApplication] };
        let guard: platform_files::OpenFilesGuard = platform_files::install();
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("初期 project #1.ldtk");
        let second = dir.path().join("Another.LDTK");
        let external = dir.path().join("Level.ldtkl");
        let unrelated = dir.path().join("map.json");
        for path in [&first, &second, &external, &unrelated] {
            std::fs::write(path, "{}").unwrap();
        }
        send(&[&first, &external, &second, &unrelated]);
        assert_eq!(platform_files::take_open_file(), Some(first.clone()));
        let wake = Arc::new(AtomicUsize::new(0));
        let counter = wake.clone();
        let ctx = eframe::egui::Context::default();
        ctx.set_request_repaint_callback(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        platform_files::set_context(&ctx);
        assert!(
            wake.load(Ordering::SeqCst) > 0,
            "pending startup event wakes newly attached UI"
        );
        assert_eq!(platform_files::take_open_file(), Some(second));
        assert!(
            platform_files::take_open_file().is_none(),
            "external levels and unrelated JSON are not projects"
        );
        for _ in 0..3 {
            let _ = ctx.run(Default::default(), |_| {});
        }
        let before = wake.load(Ordering::SeqCst);
        send(&[&first]);
        assert!(
            wake.load(Ordering::SeqCst) > before,
            "later Finder events wake idle editor"
        );
        assert_eq!(platform_files::take_open_file(), Some(first));
        assert!(platform_files::take_open_file().is_none());
        drop(guard);
        let _guard_again = platform_files::install();
        assert!(
            platform_files::take_open_file().is_none(),
            "reinstall does not replay consumed requests"
        );
    }

    pub fn native_quit_reaches_the_editor() {
        let _guard = platform_files::install();
        let mut builder = winit::event_loop::EventLoop::builder();
        platform_files::configure_quit(&mut builder);
        let _event_loop = builder.build().unwrap();
        let app = NSApplication::sharedApplication(MainThreadMarker::new().unwrap());
        let delegate = app.delegate().unwrap();
        let ctx = eframe::egui::Context::default();
        for _ in 0..3 {
            let _ = ctx.run(Default::default(), |_| {});
        }
        let wake = Arc::new(AtomicUsize::new(0));
        let counter = wake.clone();
        ctx.set_request_repaint_callback(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        platform_files::set_context(&ctx);
        assert!(!platform_files::take_quit_request());
        for _ in 0..2 {
            assert_eq!(
                delegate.applicationShouldTerminate(&app),
                NSApplicationTerminateReply::TerminateCancel,
            );
        }
        assert!(wake.load(Ordering::SeqCst) > 0);
        assert!(platform_files::take_quit_request());
        assert!(!platform_files::take_quit_request());
    }

    struct ProbeIvars {
        receipt: std::path::PathBuf,
        started: std::time::Instant,
    }

    define_class!(
        #[unsafe(super = NSObject)]
        #[thread_kind = MainThreadOnly]
        #[ivars = ProbeIvars]
        struct ProbeTimer;

        unsafe impl NSObjectProtocol for ProbeTimer {}

        impl ProbeTimer {
            #[unsafe(method(tick:))]
            fn tick(&self, _timer: &AnyObject) {
                use std::io::Write;
                let receipt = &self.ivars().receipt;
                while let Some(path) = platform_files::take_open_file() {
                    let mut file = std::fs::OpenOptions::new().append(true).open(receipt).unwrap();
                    writeln!(file, "{}", path.display()).unwrap();
                    file.flush().unwrap();
                }
                if std::fs::read_to_string(receipt).unwrap().lines().count() >= 3 {
                    std::process::exit(0);
                }
                if self.ivars().started.elapsed() > std::time::Duration::from_secs(20) {
                    std::process::exit(2);
                }
            }
        }
    );

    pub fn run_probe(receipt: &std::path::Path) {
        let _guard: platform_files::OpenFilesGuard = platform_files::install();
        std::fs::write(receipt, format!("{}\n", std::process::id())).unwrap();
        let allocated = ProbeTimer::alloc(MainThreadMarker::new().unwrap()).set_ivars(ProbeIvars {
            receipt: receipt.into(),
            started: std::time::Instant::now(),
        });
        let observer: objc2::rc::Retained<ProbeTimer> =
            unsafe { msg_send![super(allocated), init] };
        let _: *mut AnyObject = unsafe {
            msg_send![class!(NSTimer),
                scheduledTimerWithTimeInterval: 0.02_f64,
                target: &*observer,
                selector: sel!(tick:),
                userInfo: std::ptr::null_mut::<AnyObject>(),
                repeats: true
            ]
        };
        let app: *mut AnyObject = unsafe { msg_send![class!(NSApplication), sharedApplication] };
        let _: () = unsafe { msg_send![app, run] };
    }

    pub fn bundle_cold_and_warm_launch_deliver_project_documents() {
        use std::process::Command;
        let temporary = tempfile::tempdir().unwrap();
        let app = temporary.path().join("LDtk native open test.app");
        let contents = app.join("Contents");
        std::fs::create_dir_all(contents.join("MacOS")).unwrap();
        std::fs::create_dir_all(contents.join("Resources")).unwrap();
        std::fs::copy(
            std::env::current_exe().unwrap(),
            contents.join("MacOS/platform-files-probe"),
        )
        .unwrap();
        let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        for name in ["LevelEditor.icns", "project.icns"] {
            std::fs::copy(assets.join(name), contents.join("Resources").join(name)).unwrap();
        }
        let plist = contents.join("Info.plist");
        assert!(Command::new("python3").args(["-c", "import plistlib,sys; p=plistlib.load(open(sys.argv[1],'rb')); p.update(CFBundleExecutable='platform-files-probe',CFBundleIdentifier='io.ldtk.native-open-test.'+sys.argv[3],LSUIElement=True); plistlib.dump(p,open(sys.argv[2],'wb'))"])
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/macos-info.plist")).arg(&plist)
            .arg(std::process::id().to_string()).status().unwrap().success());
        assert!(
            Command::new("codesign")
                .args(["--force", "--deep", "--sign", "-"])
                .arg(&app)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("codesign")
                .args(["--verify", "--deep", "--strict"])
                .arg(&app)
                .status()
                .unwrap()
                .success()
        );
        let receipt = temporary.path().join("receipt.txt");
        let probe_log = temporary.path().join("probe.log");
        let cold = temporary.path().join("Cold 初期 #1.ldtk");
        let warm = temporary.path().join("Warm #2.ldtk");
        for path in [&cold, &warm] {
            std::fs::write(path, "{}").unwrap();
        }
        // Explicit app paths never change the user's default LDtk handler.
        assert!(
            Command::new("open")
                .arg("--stderr")
                .arg(&probe_log)
                .args(["-g", "-n", "-a"])
                .arg(&app)
                .arg(&cold)
                .arg("--args")
                .arg("--open-file-probe")
                .arg(&receipt)
                .status()
                .unwrap()
                .success()
        );
        struct ProbeCleanup {
            app: std::path::PathBuf,
            receipt: std::path::PathBuf,
        }
        impl Drop for ProbeCleanup {
            fn drop(&mut self) {
                if let Some(pid) = std::fs::read_to_string(&self.receipt)
                    .ok()
                    .and_then(|text| text.lines().next().map(str::to_owned))
                {
                    let _ = Command::new("kill").args(["-TERM", &pid]).output();
                }
                let _ = Command::new("/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister")
                    .arg("-u").arg(&self.app).output();
            }
        }
        let _cleanup = ProbeCleanup {
            app: app.clone(),
            receipt: receipt.clone(),
        };
        fn wait_for_lines(receipt: &std::path::Path, count: usize) -> Vec<String> {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                let lines: Vec<String> = std::fs::read_to_string(receipt)
                    .unwrap_or_default()
                    .lines()
                    .map(str::to_owned)
                    .collect();
                if lines.len() >= count {
                    return lines;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "bundle open did not deliver expected event: {lines:?}; log={:?}",
                    std::fs::read_to_string(receipt.with_file_name("probe.log"))
                );
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
        let first = wait_for_lines(&receipt, 2);
        assert_eq!(
            std::fs::canonicalize(&first[1]).unwrap(),
            std::fs::canonicalize(&cold).unwrap()
        );
        assert!(
            Command::new("open")
                .args(["-g", "-a"])
                .arg(&app)
                .arg(&warm)
                .status()
                .unwrap()
                .success()
        );
        let both = wait_for_lines(&receipt, 3);
        assert_eq!(
            both[0], first[0],
            "warm event reaches the already running bundle process"
        );
        assert_eq!(
            std::fs::canonicalize(&both[2]).unwrap(),
            std::fs::canonicalize(&warm).unwrap()
        );
        println!("LaunchServices cold and warm bundle opens passed");
    }
}
