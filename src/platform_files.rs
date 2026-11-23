//! macOS document-open and quit requests go through the app's dirty-document guard.
//!
//! Install on the main thread before `run_native`, retain the guard until it exits,
//! and attach the egui context when the app is constructed. Startup events remain
//! queued until the app can handle them. This does not replace winit's app delegate.

#[cfg(target_os = "macos")]
pub use mac::{
    OpenFilesGuard, configure_quit, install, set_context, take_open_file, take_quit_request,
};

#[cfg(not(target_os = "macos"))]
pub struct OpenFilesGuard;
#[cfg(not(target_os = "macos"))]
pub fn install() -> OpenFilesGuard {
    OpenFilesGuard
}
#[cfg(not(target_os = "macos"))]
pub fn set_context(_ctx: &eframe::egui::Context) {}
#[cfg(not(target_os = "macos"))]
pub fn take_open_file() -> Option<std::path::PathBuf> {
    None
}

#[cfg(target_os = "macos")]
mod mac {
    use eframe::egui::Context;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{MainThreadOnly, class, define_class, msg_send, sel};
    use objc2_foundation::{
        MainThreadMarker, NSAppleEventDescriptor, NSAppleEventManager, NSObject, NSObjectProtocol,
        NSString,
    };
    use std::collections::VecDeque;
    use std::ffi::{CStr, OsStr};
    use std::os::unix::ffi::OsStrExt;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    const CORE_EVENT: u32 = u32::from_be_bytes(*b"aevt");
    const OPEN_DOCUMENTS: u32 = u32::from_be_bytes(*b"odoc");
    const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");

    #[derive(Default)]
    struct OpenFiles {
        paths: VecDeque<PathBuf>,
        context: Option<Context>,
        quit_requested: bool,
    }

    fn state() -> &'static Mutex<OpenFiles> {
        static STATE: OnceLock<Mutex<OpenFiles>> = OnceLock::new();
        STATE.get_or_init(Mutex::default)
    }

    fn enqueue(descriptor: &NSAppleEventDescriptor) {
        let Some(url) = descriptor.fileURLValue().filter(|url| url.isFileURL()) else {
            return;
        };
        // NSURL owns this NUL-terminated filesystem string until `url` is dropped.
        let bytes = unsafe { CStr::from_ptr(url.fileSystemRepresentation().as_ptr()) }.to_bytes();
        let path = PathBuf::from(OsStr::from_bytes(bytes));
        // External .ldtkl documents contain a level, not the owning project.
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ldtk"))
        {
            return;
        }
        let context = {
            let mut state = state()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.paths.push_back(path);
            state.context.clone()
        };
        if let Some(context) = context {
            context.request_repaint();
        }
    }

    define_class!(
        // NSObject has no additional subclassing requirements. Cocoa delivers
        // AppleEvent selectors on the application main thread.
        #[unsafe(super = NSObject)]
        #[thread_kind = MainThreadOnly]
        struct DocumentHandler;

        unsafe impl NSObjectProtocol for DocumentHandler {}

        impl DocumentHandler {
            #[unsafe(method(applicationWillFinishLaunching:))]
            fn application_will_finish_launching(&self, _notification: &AnyObject) {
                self.register();
            }

            #[unsafe(method(handleOpenDocuments:withReplyEvent:))]
            fn handle_open_documents(&self, event: &NSAppleEventDescriptor, _reply: Option<&NSAppleEventDescriptor>) {
                let Some(files) = event.paramDescriptorForKeyword(DIRECT_OBJECT) else {
                    return;
                };
                if files.numberOfItems() == 0 {
                    enqueue(&files);
                } else {
                    for index in 1..=files.numberOfItems() {
                        if let Some(file) = files.descriptorAtIndex(index) {
                            enqueue(&file);
                        }
                    }
                }
            }
        }
    );

    impl DocumentHandler {
        fn register(&self) {
            // The selector above has the required two-descriptor AppleEvent signature.
            unsafe {
                NSAppleEventManager::sharedAppleEventManager()
                    .setEventHandler_andSelector_forEventClass_andEventID(
                        self,
                        sel!(handleOpenDocuments:withReplyEvent:),
                        CORE_EVENT,
                        OPEN_DOCUMENTS,
                    );
            }
        }
    }

    /// Owns the native event target. Drop it on the installing main thread.
    pub struct OpenFilesGuard {
        _handler: Retained<DocumentHandler>,
    }

    pub fn install() -> OpenFilesGuard {
        let main_thread =
            MainThreadMarker::new().expect("document events must be installed on the main thread");
        // NSApplication installs its default AppleEvent handlers during init.
        // Initialize it first so our handler survives subsequent winit startup.
        let _: *mut AnyObject = unsafe { msg_send![class!(NSApplication), sharedApplication] };
        let handler: Retained<DocumentHandler> =
            unsafe { msg_send![DocumentHandler::alloc(main_thread), init] };
        handler.register();
        // AppKit may reset default handlers while starting its event loop. Its
        // will-finish notification precedes delivery of the initial open event.
        unsafe {
            let center: *mut AnyObject = msg_send![class!(NSNotificationCenter), defaultCenter];
            let _: () = msg_send![center,
                addObserver: &*handler,
                selector: sel!(applicationWillFinishLaunching:),
                name: &*NSString::from_str("NSApplicationWillFinishLaunchingNotification"),
                object: std::ptr::null_mut::<AnyObject>()
            ];
        }
        OpenFilesGuard { _handler: handler }
    }

    impl Drop for OpenFilesGuard {
        fn drop(&mut self) {
            unsafe {
                let center: *mut AnyObject = msg_send![class!(NSNotificationCenter), defaultCenter];
                let _: () = msg_send![center, removeObserver: &*self._handler];
            }
            NSAppleEventManager::sharedAppleEventManager()
                .removeEventHandlerForEventClass_andEventID(CORE_EVENT, OPEN_DOCUMENTS);
            let mut state = state()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.context = None;
            state.quit_requested = false;
        }
    }

    pub fn set_context(ctx: &Context) {
        let pending = {
            let mut state = state()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.context = Some(ctx.clone());
            !state.paths.is_empty() || state.quit_requested
        };
        if pending {
            ctx.request_repaint();
        }
    }

    pub fn take_open_file() -> Option<PathBuf> {
        state()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .paths
            .pop_front()
    }

    pub fn configure_quit<T: 'static>(builder: &mut winit::event_loop::EventLoopBuilder<T>) {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        builder.with_quit_request_handler(request_quit);
    }

    fn request_quit() {
        let context = {
            let mut state = state()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.quit_requested = true;
            state.context.clone()
        };
        if let Some(context) = context {
            context.request_repaint();
        }
    }

    pub fn take_quit_request() -> bool {
        let mut state = state()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut state.quit_requested)
    }
}
