#[cfg(not(target_os = "linux"))]
pub(super) use rfd::FileDialog;

#[cfg(target_os = "linux")]
pub(super) use linux::FileDialog;

#[cfg(target_os = "linux")]
mod linux {
    use libloading::Library;
    use std::{
        ffi::{CStr, CString, OsString, c_char, c_int, c_void},
        os::unix::ffi::{OsStrExt, OsStringExt},
        path::{Path, PathBuf},
        ptr,
        sync::OnceLock,
    };

    #[derive(Default)]
    pub struct FileDialog {
        filters: Vec<(String, Vec<String>)>,
        directory: Option<PathBuf>,
        filename: Option<String>,
        title: Option<String>,
    }

    #[repr(i32)]
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Action {
        Open = 0,
        Save = 1,
        Folder = 2,
    }

    impl FileDialog {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn add_filter(mut self, name: impl Into<String>, extensions: &[impl ToString]) -> Self {
            self.filters.push((
                name.into(),
                extensions.iter().map(ToString::to_string).collect(),
            ));
            self
        }

        pub fn set_directory(mut self, path: impl AsRef<Path>) -> Self {
            self.directory = Some(path.as_ref().to_owned());
            self
        }

        pub fn set_file_name(mut self, name: impl Into<String>) -> Self {
            self.filename = Some(name.into());
            self
        }

        pub fn set_title(mut self, title: impl Into<String>) -> Self {
            self.title = Some(title.into());
            self
        }

        pub fn pick_file(self) -> Option<PathBuf> {
            self.choose(Action::Open)
        }

        pub fn save_file(self) -> Option<PathBuf> {
            self.choose(Action::Save)
        }

        pub fn pick_folder(self) -> Option<PathBuf> {
            self.choose(Action::Folder)
        }

        fn choose(self, action: Action) -> Option<PathBuf> {
            static GTK: OnceLock<Option<Gtk>> = OnceLock::new();
            if let Some(gtk) = GTK.get_or_init(Gtk::load)
                && let Ok(result) = gtk.choose(&self, action)
            {
                // A canceled GTK dialog is a completed request.
                return result;
            }
            let mut dialog = rfd::FileDialog::new();
            for (name, extensions) in self.filters {
                dialog = dialog.add_filter(name, &extensions);
            }
            if let Some(dir) = self.directory {
                dialog = dialog.set_directory(dir);
            }
            if let Some(filename) = self.filename {
                dialog = dialog.set_file_name(filename);
            }
            if let Some(title) = self.title {
                dialog = dialog.set_title(title);
            }
            match action {
                Action::Open => dialog.pick_file(),
                Action::Save => dialog.save_file(),
                Action::Folder => dialog.pick_folder(),
            }
        }
    }

    type Object = *mut c_void;
    type SetString = unsafe extern "C" fn(Object, *const c_char);

    struct Gtk {
        native_new: unsafe extern "C" fn(
            *const c_char,
            Object,
            c_int,
            *const c_char,
            *const c_char,
        ) -> Object,
        native_run: unsafe extern "C" fn(Object) -> c_int,
        set_modal: unsafe extern "C" fn(Object, c_int),
        set_folder: unsafe extern "C" fn(Object, *const c_char) -> c_int,
        set_name: SetString,
        set_overwrite_prompt: unsafe extern "C" fn(Object, c_int),
        get_filename: unsafe extern "C" fn(Object) -> *mut c_char,
        filter_new: unsafe extern "C" fn() -> Object,
        filter_name: SetString,
        filter_pattern: SetString,
        add_filter: unsafe extern "C" fn(Object, Object),
        unref: unsafe extern "C" fn(Object),
        free: unsafe extern "C" fn(Object),
        default_display: unsafe extern "C" fn() -> Object,
        flush_display: unsafe extern "C" fn(Object),
        _library: Library,
    }

    impl Gtk {
        fn load() -> Option<Self> {
            // The GTK 3 ABI is already present on systems that run the Linux LDtk preview.
            // Resolve it at runtime so building the editor requires no GTK development package.
            let library = unsafe { Library::new("libgtk-3.so.0") }.ok()?;
            macro_rules! symbol {
                ($name:literal) => {
                    *unsafe { library.get($name) }.ok()?
                };
            }
            let init: unsafe extern "C" fn(*mut c_int, *mut *mut *mut c_char) -> c_int =
                symbol!(b"gtk_init_check");
            let gtk = Self {
                native_new: symbol!(b"gtk_file_chooser_native_new"),
                native_run: symbol!(b"gtk_native_dialog_run"),
                set_modal: symbol!(b"gtk_native_dialog_set_modal"),
                set_folder: symbol!(b"gtk_file_chooser_set_current_folder"),
                set_name: symbol!(b"gtk_file_chooser_set_current_name"),
                set_overwrite_prompt: symbol!(b"gtk_file_chooser_set_do_overwrite_confirmation"),
                get_filename: symbol!(b"gtk_file_chooser_get_filename"),
                filter_new: symbol!(b"gtk_file_filter_new"),
                filter_name: symbol!(b"gtk_file_filter_set_name"),
                filter_pattern: symbol!(b"gtk_file_filter_add_pattern"),
                add_filter: symbol!(b"gtk_file_chooser_add_filter"),
                unref: symbol!(b"g_object_unref"),
                free: symbol!(b"g_free"),
                default_display: symbol!(b"gdk_display_get_default"),
                flush_display: symbol!(b"gdk_display_flush"),
                _library: library,
            };
            // File dialogs are opened synchronously by the editor's UI thread. GTK accepts
            // null argc/argv when it should leave the process's command-line arguments alone.
            (unsafe { init(ptr::null_mut(), ptr::null_mut()) } != 0).then_some(gtk)
        }

        fn choose(&self, options: &FileDialog, action: Action) -> Result<Option<PathBuf>, ()> {
            let title =
                CString::new(options.title.as_deref().unwrap_or_default()).map_err(|_| ())?;
            // GTK copies these strings. The live chooser owns its filters, and its result is
            // a separately allocated filename that must be released with g_free.
            unsafe {
                let chooser = (self.native_new)(
                    title.as_ptr(),
                    ptr::null_mut(),
                    action as c_int,
                    ptr::null(),
                    ptr::null(),
                );
                if chooser.is_null() {
                    return Err(());
                }
                (self.set_modal)(chooser, 1);
                if let Some(dir) = &options.directory
                    && let Ok(dir) = CString::new(dir.as_os_str().as_bytes())
                {
                    (self.set_folder)(chooser, dir.as_ptr());
                }
                if action == Action::Save {
                    (self.set_overwrite_prompt)(chooser, 1);
                    if let Some(name) = &options.filename
                        && let Ok(name) = CString::new(name.as_str())
                    {
                        (self.set_name)(chooser, name.as_ptr());
                    }
                }
                for (name, extensions) in &options.filters {
                    if let Ok(name) = CString::new(name.as_str()) {
                        let filter = (self.filter_new)();
                        (self.filter_name)(filter, name.as_ptr());
                        for extension in extensions {
                            if let Ok(pattern) = CString::new(format!("*.{extension}")) {
                                (self.filter_pattern)(filter, pattern.as_ptr());
                            }
                        }
                        (self.add_filter)(chooser, filter);
                    }
                }
                const GTK_RESPONSE_ACCEPT: c_int = -3;
                let mut result = None;
                if (self.native_run)(chooser) == GTK_RESPONSE_ACCEPT {
                    let filename = (self.get_filename)(chooser);
                    if !filename.is_null() {
                        let bytes = CStr::from_ptr(filename).to_bytes().to_vec();
                        (self.free)(filename.cast());
                        result = Some(PathBuf::from(OsString::from_vec(bytes)));
                    }
                }
                (self.unref)(chooser);
                // Winit resumes its own event loop. Send GTK's queued window
                // removal before leaving the nested GTK loop.
                let display = (self.default_display)();
                if !display.is_null() {
                    (self.flush_display)(display);
                }
                Ok(result)
            }
        }
    }
}
